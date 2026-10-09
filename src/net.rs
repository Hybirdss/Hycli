//! Bounded first-party requests. Redirects never carry a credential to another origin.
use crate::{
    apperr::{AppError, AppResult},
    defend,
    policy::Policy,
    util,
};
use futures_util::StreamExt;
use reqwest::{
    Method,
    header::{HeaderMap, HeaderName, HeaderValue},
};
use serde_json::Value;
use std::{
    net::{IpAddr, SocketAddr},
    sync::Arc,
    time::{Duration, Instant},
};

pub struct Network {
    pub policy: Arc<Policy>,
    pub allow_local: bool,
}
pub struct Request {
    pub url: String,
    pub method: String,
    pub headers: Vec<(String, String)>,
    pub body: Option<Vec<u8>>,
    pub account_id: String,
    pub limit: usize,
    pub timeout: Duration,
    pub read_only: bool,
    pub follow_redirects: bool,
    /// Compatibility metadata; requests always use the standard HTTP/TLS stack.
    pub tls_profile: String,
}
pub struct Response {
    pub url: String,
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub bytes: Vec<u8>,
    pub duration_ms: i64,
    pub defense: String,
}
impl Response {
    pub fn header(&self, key: &str) -> &str {
        self.headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(key))
            .map(|(_, v)| v.as_str())
            .unwrap_or("")
    }
    pub fn json(&self) -> Option<Value> {
        serde_json::from_slice(&self.bytes).ok()
    }
    pub fn successful(&self) -> bool {
        (200..300).contains(&self.status) && self.defense == "none"
    }
}
impl Network {
    /// Rendered reads share HTTP pacing and account gates with ordinary operations.
    pub async fn fetch_rendered(
        &self,
        credential: &crate::accounts::Credential,
        entry: &str,
        account: &str,
    ) -> AppResult<Response> {
        let initial = validate_url(entry)?;
        let host_scope = format!("{}|*", initial.origin().ascii_serialization());
        let host_token = self.policy.acquire(&host_scope).await?;
        let mut host_lease = Lease {
            policy: &self.policy,
            scope: &host_scope,
            token: &host_token,
            released: false,
        };
        let scope = format!("{}|{}", initial.origin().ascii_serialization(), account);
        if account.is_empty() {
            self.policy.resume_auth(&scope)?;
        }
        let token = self.policy.acquire(&scope).await?;
        let mut lease = Lease {
            policy: &self.policy,
            scope: &scope,
            token: &token,
            released: false,
        };
        let started = Instant::now();
        let capture = crate::browser_session::capture(credential, entry, self.allow_local).await?;
        if validate_url(&capture.final_url)?.origin() != initial.origin()
            || capture.html.len() > 8 * 1024 * 1024
        {
            return Err(AppError::api("response_mismatch", 502));
        }
        let headers = vec![("content-type".into(), "text/html; charset=utf-8".into())];
        let (defense, _) = defend::classify(&defend::Observation {
            status_code: capture.status,
            headers: headers.clone(),
            body_snippet: capture.html.chars().take(16_384).collect(),
        });
        self.policy.release(
            &scope,
            &token,
            capture.status,
            None,
            defense != defend::Class::None && defense != defend::Class::RateLimit,
        )?;
        lease.released = true;
        self.policy.release(
            &host_scope,
            &host_token,
            if capture.status == 429 { 429 } else { 0 },
            None,
            false,
        )?;
        host_lease.released = true;
        Ok(Response {
            url: capture.final_url,
            status: capture.status,
            headers,
            bytes: capture.html.into_bytes(),
            duration_ms: started.elapsed().as_millis() as i64,
            defense: defense.as_str().into(),
        })
    }
    pub async fn fetch(&self, input: Request) -> AppResult<Response> {
        let initial = validate_url(&input.url)?;
        let mut target = initial.clone();
        let mut redirects = 0;
        let mut retries = 0;
        let start = Instant::now();
        loop {
            let addresses = resolve(&target, self.allow_local).await?;
            let host_scope = format!("{}|*", target.origin().ascii_serialization());
            let host_token = self.policy.acquire(&host_scope).await?;
            let mut host_lease = Lease {
                policy: &self.policy,
                scope: &host_scope,
                token: &host_token,
                released: false,
            };
            let scope = format!(
                "{}|{}",
                target.origin().ascii_serialization(),
                input.account_id
            );
            if input.account_id.is_empty() {
                // A protected public URL does not imply that anonymous access to the
                // entire website has lost a session. Also repair older persisted gates.
                self.policy.resume_auth(&scope)?;
            }
            let token = self.policy.acquire(&scope).await?;
            let mut lease = Lease {
                policy: &self.policy,
                scope: &scope,
                token: &token,
                released: false,
            };
            let client = reqwest::Client::builder()
                .redirect(reqwest::redirect::Policy::none())
                .retry(reqwest::retry::never())
                .timeout(input.timeout.min(Duration::from_secs(90)))
                .connect_timeout(Duration::from_secs(10))
                .user_agent(concat!("Hycli/", env!("CARGO_PKG_VERSION")))
                .resolve_to_addrs(target.host_str().expect("validated host"), &addresses)
                .build()
                .map_err(|_| AppError::api("internal", 500))?;
            let mut headers = HeaderMap::new();
            for (key, value) in &input.headers {
                let name = HeaderName::from_bytes(key.as_bytes())
                    .map_err(|_| AppError::api("data_invalid", 400))?;
                let mut value =
                    HeaderValue::from_str(value).map_err(|_| AppError::api("data_invalid", 400))?;
                if util::secret_field(key) {
                    value.set_sensitive(true);
                }
                headers.insert(name, value);
            }
            let method = Method::from_bytes(input.method.as_bytes())
                .map_err(|_| AppError::api("data_invalid", 400))?;
            let mut request = client.request(method, target.clone()).headers(headers);
            if let Some(body) = &input.body {
                request = request.body(body.clone());
            }
            let response = request.send().await;
            let response = match response {
                Ok(response) => response,
                Err(error) => {
                    let is_timeout = error.is_timeout();
                    drop(lease);
                    if input.read_only && retries < 1 && (is_timeout || error.is_connect()) {
                        retries += 1;
                        tokio::time::sleep(Duration::from_secs(2)).await;
                        continue;
                    }
                    return Err(AppError::api(
                        if is_timeout {
                            "timeout"
                        } else {
                            "request_failed"
                        },
                        502,
                    ));
                }
            };
            let status = response.status().as_u16();
            let gate_status = if status == 401 && input.account_id.is_empty() {
                0
            } else {
                status
            };
            let headers: Vec<(String, String)> = response
                .headers()
                .iter()
                .filter_map(|(key, value)| {
                    value
                        .to_str()
                        .ok()
                        .map(|v| (key.as_str().to_owned(), v.to_owned()))
                })
                .collect();
            let retry_after = retry_after(&headers);
            if response
                .content_length()
                .is_some_and(|length| length > input.limit as u64)
            {
                self.policy
                    .release(&scope, &token, gate_status, retry_after, status == 403)?;
                lease.released = true;
                self.policy.release(
                    &host_scope,
                    &host_token,
                    if status == 429 { 429 } else { 0 },
                    retry_after,
                    false,
                )?;
                host_lease.released = true;
                return Err(AppError::api("too_large", 413));
            }
            let mut bytes = Vec::new();
            let mut stream = response.bytes_stream();
            while let Some(chunk) = stream.next().await {
                let chunk = chunk.map_err(|_| AppError::api("request_failed", 502))?;
                if bytes.len() + chunk.len() > input.limit {
                    self.policy
                        .release(&scope, &token, gate_status, retry_after, status == 403)?;
                    lease.released = true;
                    self.policy.release(
                        &host_scope,
                        &host_token,
                        if status == 429 { 429 } else { 0 },
                        retry_after,
                        false,
                    )?;
                    host_lease.released = true;
                    return Err(AppError::api("too_large", 413));
                }
                bytes.extend_from_slice(&chunk);
            }
            let (defense, _) = defend::classify(&defend::Observation {
                status_code: status,
                headers: headers.clone(),
                body_snippet: String::from_utf8_lossy(&bytes[..bytes.len().min(16_384)])
                    .into_owned(),
            });
            self.policy.release(
                &scope,
                &token,
                gate_status,
                retry_after,
                defense != defend::Class::None && defense != defend::Class::RateLimit,
            )?;
            lease.released = true;
            self.policy.release(
                &host_scope,
                &host_token,
                if status == 429 { 429 } else { 0 },
                retry_after,
                false,
            )?;
            host_lease.released = true;
            if input.follow_redirects
                && input.read_only
                && matches!(status, 301 | 302 | 303 | 307 | 308)
            {
                let location = headers
                    .iter()
                    .find(|(key, _)| key == "location")
                    .map(|(_, value)| value);
                if let Some(location) = location {
                    let next = target
                        .join(location)
                        .map_err(|_| AppError::api("bad_url", 400))?;
                    validate_url(next.as_str())?;
                    if next.origin() == initial.origin()
                        && crate::policy::safe_read_url(&next)
                        && redirects < 4
                    {
                        target = next;
                        redirects += 1;
                        continue;
                    }
                }
            }
            // Automatic retries are bounded, read-only and never bypass a site's cooldown.
            if input.read_only
                && status == 503
                && defense == defend::Class::None
                && retries < 1
                && retry_after.is_none()
            {
                retries += 1;
                tokio::time::sleep(Duration::from_secs(2)).await;
                continue;
            }
            return Ok(Response {
                url: target.to_string(),
                status,
                headers,
                bytes,
                duration_ms: start.elapsed().as_millis() as i64,
                defense: defense.as_str().into(),
            });
        }
    }
}
struct Lease<'a> {
    policy: &'a Policy,
    scope: &'a str,
    token: &'a str,
    released: bool,
}
impl Drop for Lease<'_> {
    fn drop(&mut self) {
        if !self.released {
            let _ = self.policy.release(self.scope, self.token, 0, None, false);
        }
    }
}

pub fn validate_url(raw: &str) -> AppResult<url::Url> {
    if raw.len() > 8192 || raw.chars().any(char::is_control) {
        return Err(AppError::api("bad_url", 400));
    }
    let mut url = url::Url::parse(raw).map_err(|_| AppError::api("bad_url", 400))?;
    if !["http", "https"].contains(&url.scheme())
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return Err(AppError::api("bad_url", 400));
    }
    if url.query_pairs().any(|(k, _)| util::secret_field(&k)) {
        return Err(AppError::api("bad_url", 400));
    }
    url.set_fragment(None);
    Ok(url)
}
pub async fn resolve(url: &url::Url, allow_local: bool) -> AppResult<Vec<SocketAddr>> {
    let host = url
        .host_str()
        .ok_or_else(|| AppError::api("bad_url", 400))?;
    let port = url
        .port_or_known_default()
        .ok_or_else(|| AppError::api("bad_url", 400))?;
    let addresses: Vec<SocketAddr> = tokio::time::timeout(
        Duration::from_secs(8),
        tokio::net::lookup_host((host.trim_matches(['[', ']']), port)),
    )
    .await
    .map_err(|_| AppError::api("timeout", 502))?
    .map_err(|_| AppError::api("request_failed", 502))?
    .collect();
    if addresses.is_empty() || (!allow_local && addresses.iter().any(|a| !public_ip(a.ip()))) {
        return Err(AppError::api("unsafe_target", 400));
    }
    Ok(addresses)
}
fn public_ip(address: IpAddr) -> bool {
    match address {
        IpAddr::V4(ip) => {
            let o = ip.octets();
            !(ip.is_private()
                || ip.is_loopback()
                || ip.is_link_local()
                || ip.is_unspecified()
                || ip.is_broadcast()
                || ip.is_multicast()
                || ip.is_documentation()
                || o[0] == 0
                || o[0] >= 240
                || (o[0] == 100 && (64..=127).contains(&o[1]))
                || (o[0] == 198 && (o[1] == 18 || o[1] == 19))
                || (o[0] == 192 && o[1] == 0 && o[2] == 0))
        }
        IpAddr::V6(ip) => {
            if let Some(v4) = ip.to_ipv4_mapped() {
                return public_ip(IpAddr::V4(v4));
            }
            let s = ip.segments();
            !(ip.is_loopback()
                || ip.is_unspecified()
                || ip.is_multicast()
                || ip.is_unique_local()
                || ip.is_unicast_link_local()
                || s[0] & 0xe000 != 0x2000
                || (s[0] == 0x2001 && (s[1] == 0xdb8 || s[1] == 0 || s[1] == 2))
                || s[0] == 0x2002)
        }
    }
}
pub fn retry_after(headers: &[(String, String)]) -> Option<Duration> {
    let value = headers
        .iter()
        .find(|(key, _)| key.eq_ignore_ascii_case("retry-after"))
        .map(|(_, value)| value)?;
    if let Ok(seconds) = value.trim().parse::<u64>() {
        return Some(Duration::from_secs(seconds.max(1).min(86_400)));
    }
    httpdate::parse_http_date(value).ok().map(|date| {
        date.duration_since(std::time::SystemTime::now())
            .unwrap_or(Duration::from_secs(1))
            .min(Duration::from_secs(86_400))
    })
}
