//! First-party requests with bounded buffering and owned file streams. Redirects never carry a credential to another origin.
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
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};

pub struct Network {
    pub policy: Arc<Policy>,
    pub allow_local: bool,
    pub clients: std::sync::Mutex<std::collections::BTreeMap<String, reqwest::Client>>,
}
#[derive(Clone)]
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
    /// multipart/form-data parts. When present, sent in place of `body`; the
    /// network layer reads any file parts from disk and sets the boundary.
    pub multipart: Option<Vec<crate::spec::MultipartPart>>,
}
impl Request {
    fn has_file_upload(&self) -> bool {
        self.multipart.as_ref().is_some_and(|parts| {
            parts
                .iter()
                .any(|part| matches!(part, crate::spec::MultipartPart::File { .. }))
        })
    }

    fn check_read_headers(&self) -> AppResult<()> {
        if self.read_only && self.has_file_upload() {
            return Err(AppError::api("approval_required", 409));
        }
        if self.read_only
            && crate::policy::request_headers_effect(
                self.headers
                    .iter()
                    .map(|(name, value)| (name.as_str(), value.as_str())),
            ) != crate::policy::Effect::Read
        {
            return Err(AppError::api("approval_required", 409));
        }
        Ok(())
    }
}
pub struct Response {
    pub url: String,
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub bytes: Vec<u8>,
    pub duration_ms: i64,
    pub defense: String,
}
/// A receiver factory is cloneable; each writable/sealed attempt it creates is not.
/// Only explicit final-resource requests carry this factory. Tokens and identity
/// responses continue through the ordinary bounded buffered path.
#[derive(Clone)]
pub(crate) struct FileReceiver {
    root: std::path::PathBuf,
}
impl FileReceiver {
    pub(crate) fn new(root: &std::path::Path) -> Self {
        Self {
            root: root.to_owned(),
        }
    }
    async fn begin(&self) -> AppResult<crate::artifacts::WritableArtifact> {
        let root = self.root.clone();
        tokio::task::spawn_blocking(move || crate::artifacts::ArtifactStore::open(&root)?.begin())
            .await
            .map_err(|_| AppError::api("internal", 500))?
    }
}
pub(crate) struct FileResponse {
    pub response: Response,
    pub artifact: Option<crate::artifacts::SealedArtifact>,
}
impl FileResponse {
    pub(crate) fn buffered(response: Response) -> Self {
        Self {
            response,
            artifact: None,
        }
    }
}

impl Response {
    pub fn header(&self, key: &str) -> &str {
        self.headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(key))
            .map(|(_, v)| v.as_str())
            .unwrap_or("")
    }
    pub fn cookie_value(&self, name: &str) -> Option<String> {
        let prefix = format!("{name}=");
        self.headers
            .iter()
            .filter(|(k, _)| k.eq_ignore_ascii_case("set-cookie"))
            .find_map(|(_, v)| {
                v.split(';')
                    .next()
                    .and_then(|pair| pair.trim().strip_prefix(&prefix))
                    .map(str::to_owned)
            })
    }
    pub fn json(&self) -> Option<Value> {
        serde_json::from_slice(&self.bytes).ok()
    }
    pub fn successful(&self) -> bool {
        (200..300).contains(&self.status) && self.defense == "none"
    }
}
impl Network {
    pub fn new(policy: Arc<Policy>, allow_local: bool) -> Self {
        Self {
            policy,
            allow_local,
            clients: std::sync::Mutex::new(std::collections::BTreeMap::new()),
        }
    }

    fn pooled_client(
        &self,
        host: &str,
        ua: &str,
        addresses: &[SocketAddr],
        timeout: Duration,
        mode: TransferMode,
    ) -> AppResult<reqwest::Client> {
        let mut addr_keys: Vec<String> = addresses.iter().map(ToString::to_string).collect();
        addr_keys.sort();
        let pool_key = format!(
            "{}|{}|{:?}|{:?}|{}",
            host,
            ua,
            timeout,
            mode,
            addr_keys.join(",")
        );
        if let Ok(guard) = self.clients.lock() {
            if let Some(existing) = guard.get(&pool_key) {
                return Ok(existing.clone());
            }
        }
        let builder = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .retry(reqwest::retry::never());
        // A total deadline is appropriate for bounded buffered requests, not
        // files of arbitrary size. File transfers are monitored for idle time.
        let builder = match mode {
            TransferMode::Buffered => builder.timeout(timeout.min(Duration::from_secs(90))),
            TransferMode::File => builder,
        };
        let client = builder
            .connect_timeout(Duration::from_secs(10))
            .pool_idle_timeout(Duration::from_secs(90))
            .pool_max_idle_per_host(6)
            .tcp_keepalive(Duration::from_secs(30))
            .user_agent(ua)
            .resolve_to_addrs(host, addresses)
            .build()
            .map_err(|_| AppError::api("internal", 500))?;
        if let Ok(mut guard) = self.clients.lock() {
            if guard.len() >= 64
                && let Some(first_key) = guard.keys().next().cloned()
            {
                guard.remove(&first_key);
            }
            guard.insert(pool_key, client.clone());
        }
        Ok(client)
    }

    /// Execute an in-page `window.fetch()` inside an isolated world on the origin's browser tab.
    pub async fn fetch_browser_in_page(
        &self,
        credential: &crate::accounts::Credential,
        input: &Request,
    ) -> AppResult<Response> {
        self.fetch_browser_in_page_guarded(credential, input, None)
            .await
    }

    pub(crate) async fn fetch_browser_in_page_guarded(
        &self,
        credential: &crate::accounts::Credential,
        input: &Request,
        check_authority: Option<&(dyn Fn() -> AppResult<()> + Sync)>,
    ) -> AppResult<Response> {
        input.check_read_headers()?;
        if input.multipart.is_some() {
            return Err(AppError::api("unsupported_transport", 400));
        }
        let initial = validate_url(&input.url)?;
        let origin_str = initial.origin().ascii_serialization();
        let strategy_key = Policy::strategy_key(
            &input.method,
            &input.url,
            "browser-fetch",
            &input.account_id,
        );
        let host_scope = format!("{origin_str}|*");
        let host_token = self.policy.acquire(&host_scope).await?;
        let mut host_lease = Lease {
            policy: &self.policy,
            scope: &host_scope,
            token: &host_token,
            released: false,
            observed: None,
        };
        let scope = format!("{origin_str}|{}", input.account_id);
        if input.account_id.is_empty() {
            self.policy.resume_auth(&scope)?;
        }
        let token = self.policy.acquire(&scope).await?;
        let mut lease = Lease {
            policy: &self.policy,
            scope: &scope,
            token: &token,
            released: false,
            observed: None,
        };
        if let Some(check) = check_authority {
            check()?;
        }
        let started = Instant::now();
        let fetched =
            crate::browser_session::fetch_in_page(crate::browser_session::InPageFetchRequest {
                credential,
                method: &input.method,
                url: &input.url,
                headers: &input.headers,
                body: input.body.as_deref(),
                allow_local: self.allow_local,
            })
            .await?;
        if !same_site_origin(&validate_url(&fetched.url)?, &initial)
            || fetched.bytes.len() > input.limit
        {
            return Err(AppError::api("response_mismatch", 502));
        }
        let (defense, _) = defend::classify(&defend::Observation {
            status_code: fetched.status,
            headers: fetched.headers.clone(),
            body_snippet: String::from_utf8_lossy(
                &fetched.bytes[..fetched.bytes.len().min(16_384)],
            )
            .into_owned(),
        });
        let challenged = defense != defend::Class::None && defense != defend::Class::RateLimit;
        if challenged {
            let _ = self
                .policy
                .record_blocked_strategy(&strategy_key, defense.as_str(), 180_000);
        } else if (200..300).contains(&fetched.status) {
            let _ = self.policy.clear_strategy(&strategy_key);
            let _ = self
                .policy
                .record_preferred_transport(&origin_str, "browser-fetch");
        }
        let retry_after = retry_after(&fetched.headers);
        self.policy
            .release(&scope, &token, fetched.status, retry_after, challenged)?;
        lease.released = true;
        self.policy.release(
            &host_scope,
            &host_token,
            if fetched.status == 429 { 429 } else { 0 },
            retry_after,
            false,
        )?;
        host_lease.released = true;
        Ok(Response {
            url: fetched.url,
            status: fetched.status,
            headers: fetched.headers,
            bytes: fetched.bytes,
            duration_ms: started.elapsed().as_millis() as i64,
            defense: defense.as_str().into(),
        })
    }

    /// Rendered reads share HTTP pacing and account gates with ordinary operations.
    pub async fn fetch_rendered(
        &self,
        credential: &crate::accounts::Credential,
        entry: &str,
        account: &str,
    ) -> AppResult<Response> {
        self.fetch_rendered_guarded(credential, entry, account, None)
            .await
    }

    pub(crate) async fn fetch_rendered_guarded(
        &self,
        credential: &crate::accounts::Credential,
        entry: &str,
        account: &str,
        check_authority: Option<&(dyn Fn() -> AppResult<()> + Sync)>,
    ) -> AppResult<Response> {
        let initial = validate_url(entry)?;
        let origin_str = initial.origin().ascii_serialization();
        let strategy_key = Policy::strategy_key("GET", entry, "browser", account);
        let host_scope = format!("{origin_str}|*");
        let host_token = self.policy.acquire(&host_scope).await?;
        let mut host_lease = Lease {
            policy: &self.policy,
            scope: &host_scope,
            token: &host_token,
            released: false,
            observed: None,
        };
        let scope = format!("{origin_str}|{account}");
        if account.is_empty() {
            self.policy.resume_auth(&scope)?;
        }
        let token = self.policy.acquire(&scope).await?;
        let mut lease = Lease {
            policy: &self.policy,
            scope: &scope,
            token: &token,
            released: false,
            observed: None,
        };
        if let Some(check) = check_authority {
            check()?;
        }
        let started = Instant::now();
        let capture = crate::browser_session::capture(credential, entry, self.allow_local).await?;
        if !same_site_origin(&validate_url(&capture.final_url)?, &initial)
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
        let challenged = defense != defend::Class::None && defense != defend::Class::RateLimit;
        if challenged {
            let _ = self
                .policy
                .record_blocked_strategy(&strategy_key, defense.as_str(), 180_000);
        } else if (200..300).contains(&capture.status) {
            let _ = self.policy.clear_strategy(&strategy_key);
        }
        self.policy
            .release(&scope, &token, capture.status, None, challenged)?;
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
        self.fetch_inner(input, None, None, None, None)
            .await
            .map(|received| received.response)
    }
    /// The continuation key/value is supplied only by the private runtime resolver.
    pub(crate) async fn fetch_paginated(
        &self,
        mut input: Request,
        continuation: Option<(&str, &str)>,
    ) -> AppResult<Response> {
        if input.method != "GET" || !input.read_only {
            return Err(AppError::api("unsafe_target", 400));
        }
        input.follow_redirects = false;
        self.fetch_inner(input, continuation, None, None, None)
            .await
            .map(|received| received.response)
    }
    /// Recheck private account authority after every wait and immediately before dispatch.
    /// The callback is also run for each bounded transport retry.
    pub(crate) async fn fetch_guarded(
        &self,
        input: Request,
        continuation: Option<(&str, &str)>,
        check_authority: &(dyn Fn() -> AppResult<()> + Sync),
    ) -> AppResult<Response> {
        self.fetch_inner(input, continuation, Some(check_authority), None, None)
            .await
            .map(|received| received.response)
    }
    pub(crate) async fn fetch_oauth_guarded(
        &self,
        mut input: Request,
        continuation: Option<(&str, &str)>,
        check_authority: &(dyn Fn() -> AppResult<()> + Sync),
        dispatched: Option<&std::sync::atomic::AtomicBool>,
    ) -> AppResult<Response> {
        input.follow_redirects = false;
        self.fetch_inner(input, continuation, Some(check_authority), dispatched, None)
            .await
            .map(|received| received.response)
    }
    pub(crate) async fn fetch_file_guarded(
        &self,
        input: Request,
        check_authority: &(dyn Fn() -> AppResult<()> + Sync),
        receiver: &FileReceiver,
    ) -> AppResult<FileResponse> {
        self.fetch_inner(input, None, Some(check_authority), None, Some(receiver))
            .await
    }
    pub(crate) async fn fetch_oauth_resource_guarded(
        &self,
        mut input: Request,
        continuation: Option<(&str, &str)>,
        check_authority: &(dyn Fn() -> AppResult<()> + Sync),
        receiver: Option<&FileReceiver>,
    ) -> AppResult<FileResponse> {
        input.follow_redirects = false;
        self.fetch_inner(input, continuation, Some(check_authority), None, receiver)
            .await
    }
    async fn fetch_inner(
        &self,
        input: Request,
        continuation: Option<(&str, &str)>,
        check_authority: Option<&(dyn Fn() -> AppResult<()> + Sync)>,
        dispatched: Option<&std::sync::atomic::AtomicBool>,
        receiver: Option<&FileReceiver>,
    ) -> AppResult<FileResponse> {
        input.check_read_headers()?;
        let mut initial = validate_url(&input.url)?;
        if let Some((key, value)) = continuation {
            if initial.query_pairs().any(|(name, _)| name == key) {
                return Err(AppError::api("data_invalid", 400));
            }
            initial.query_pairs_mut().append_pair(key, value);
            if initial.as_str().len() > 8192 {
                return Err(AppError::api("bad_url", 400));
            }
        }
        let file_upload = input.has_file_upload();
        let transfer_mode = if file_upload || receiver.is_some() {
            TransferMode::File
        } else {
            TransferMode::Buffered
        };
        let after_dispatch = |error: AppError| {
            if file_upload || (!input.read_only && error.public_code == "request_lease_lost") {
                AppError::api("action_outcome_unknown", 502)
            } else {
                error
            }
        };
        let mut target = initial.clone();
        let mut redirects = 0;
        let mut retries = 0;
        let start = Instant::now();
        loop {
            if let Some(check) = check_authority {
                check()?;
            }
            let addresses = resolve(&target, self.allow_local).await?;
            let host_scope = format!("{}|*", target.origin().ascii_serialization());
            let host_token = self.policy.acquire(&host_scope).await?;
            let mut host_lease = Lease {
                policy: &self.policy,
                scope: &host_scope,
                token: &host_token,
                released: false,
                observed: None,
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
            let progress = Arc::new(TransferProgress::new());
            let mut monitor = TransferMonitor::new(
                &self.policy,
                vec![(&host_scope, &host_token)],
                progress.clone(),
            );
            let token = monitor.wait(self.policy.acquire(&scope), None).await??;
            monitor.leases.push((&scope, &token));
            let mut lease = Lease {
                policy: &self.policy,
                scope: &scope,
                token: &token,
                released: false,
                observed: None,
            };
            const CHROME_UA: &str = "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/134.0.0.0 Safari/537.36";
            let ua = if input.tls_profile == "hycli" {
                concat!("Hycli/", env!("CARGO_PKG_VERSION"))
            } else {
                CHROME_UA
            };
            let client = self.pooled_client(
                target.host_str().expect("validated host"),
                ua,
                &addresses,
                input.timeout,
                transfer_mode,
            )?;
            let mut headers = HeaderMap::new();
            if matches!(
                input.tls_profile.as_str(),
                "go" | "chrome" | "browser" | "stealth" | "default" | "standard" | ""
            ) {
                let path_lower = target.path().to_ascii_lowercase();
                let is_mutation = !matches!(input.method.as_str(), "GET" | "HEAD" | "OPTIONS");
                let has_json_hint = input.body.is_some()
                    || path_lower.contains("/api/")
                    || path_lower.starts_with("/v1/")
                    || path_lower.starts_with("/v2/")
                    || path_lower.starts_with("/v3/")
                    || path_lower.contains("graphql")
                    || path_lower.ends_with(".json")
                    || input.headers.iter().any(|(k, _)| {
                        k.eq_ignore_ascii_case("content-type")
                            || k.eq_ignore_ascii_case("x-requested-with")
                            || k.eq_ignore_ascii_case("authorization")
                    });
                headers.insert(
                    HeaderName::from_static("accept"),
                    HeaderValue::from_static(if has_json_hint {
                        "application/json, text/plain, */*"
                    } else {
                        "text/html,application/xhtml+xml,application/xml;q=0.9,image/avif,image/webp,image/apng,*/*;q=0.8"
                    }),
                );
                headers.insert(
                    HeaderName::from_static("accept-language"),
                    HeaderValue::from_static("en-US,en;q=0.9,ko;q=0.8"),
                );
                headers.insert(
                    HeaderName::from_static("sec-ch-ua"),
                    HeaderValue::from_static(
                        "\"Chromium\";v=\"134\", \"Not:A-Brand\";v=\"24\", \"Google Chrome\";v=\"134\"",
                    ),
                );
                headers.insert(
                    HeaderName::from_static("sec-ch-ua-mobile"),
                    HeaderValue::from_static("?0"),
                );
                headers.insert(
                    HeaderName::from_static("sec-ch-ua-platform"),
                    HeaderValue::from_static("\"Linux\""),
                );
                if has_json_hint || is_mutation {
                    headers.insert(
                        HeaderName::from_static("sec-fetch-site"),
                        HeaderValue::from_static("same-origin"),
                    );
                    headers.insert(
                        HeaderName::from_static("sec-fetch-mode"),
                        HeaderValue::from_static("cors"),
                    );
                    headers.insert(
                        HeaderName::from_static("sec-fetch-dest"),
                        HeaderValue::from_static("empty"),
                    );
                } else {
                    headers.insert(
                        HeaderName::from_static("sec-fetch-site"),
                        HeaderValue::from_static(if redirects > 0 {
                            "same-origin"
                        } else {
                            "none"
                        }),
                    );
                    headers.insert(
                        HeaderName::from_static("sec-fetch-mode"),
                        HeaderValue::from_static("navigate"),
                    );
                    headers.insert(
                        HeaderName::from_static("sec-fetch-dest"),
                        HeaderValue::from_static("document"),
                    );
                    headers.insert(
                        HeaderName::from_static("upgrade-insecure-requests"),
                        HeaderValue::from_static("1"),
                    );
                }
                let origin_str = target.origin().ascii_serialization();
                if (has_json_hint || is_mutation || !input.account_id.is_empty())
                    && let Ok(ref_val) = HeaderValue::from_str(&format!("{origin_str}/"))
                {
                    headers.insert(HeaderName::from_static("referer"), ref_val);
                }
                if is_mutation && let Ok(orig_val) = HeaderValue::from_str(&origin_str) {
                    headers.insert(HeaderName::from_static("origin"), orig_val);
                }
            }
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
            if input.multipart.is_some() {
                headers.remove(reqwest::header::CONTENT_TYPE);
            }
            let method = Method::from_bytes(input.method.as_bytes())
                .map_err(|_| AppError::api("data_invalid", 400))?;
            if let Some(check) = check_authority {
                check()?;
            }
            let mut request = client.request(method, target.clone()).headers(headers);
            let upload = Arc::new(UploadCompletion::default());
            // Hyper can retain a request body after an early final response.
            // Cancellation belongs to this attempt, not to the body owner.
            let upload_guard = UploadGuard(upload.clone());
            if let Some(parts) = &input.multipart {
                let form = monitor
                    .wait(
                        build_multipart_form(parts, progress.clone(), upload.clone()),
                        None,
                    )
                    .await??;
                request = request.multipart(form);
            } else if let Some(body) = &input.body {
                request = request.body(body.clone());
            }
            monitor.renew()?;
            // Renewal can wait for SQLite's write lock. Recheck the selected
            // source authority after that wait, immediately before dispatch.
            if let Some(check) = check_authority {
                check()?;
            }
            progress.mark();
            let idle_timeout = (transfer_mode == TransferMode::File).then_some(input.timeout);
            if let Some(dispatched) = dispatched {
                dispatched.store(true, std::sync::atomic::Ordering::SeqCst);
            }
            let response = monitor
                .wait(request.send(), idle_timeout)
                .await
                .map_err(&after_dispatch)?;
            let response = match response {
                Ok(response) => response,
                Err(error) => {
                    upload_guard.cancel();
                    // This no-retry OAuth POST failed before an HTTP request could
                    // be sent. Other transport failures remain dispatch-uncertain.
                    if !input.read_only && !input.follow_redirects && error.is_connect() {
                        if let Some(dispatched) = dispatched {
                            dispatched.store(false, std::sync::atomic::Ordering::SeqCst);
                        }
                    }
                    let is_timeout = error.is_timeout();
                    drop(lease);
                    if input.read_only && retries < 1 && (is_timeout || error.is_connect()) {
                        retries += 1;
                        tokio::time::sleep(Duration::from_secs(2)).await;
                        continue;
                    }
                    return Err(after_dispatch(AppError::api(
                        if is_timeout {
                            "timeout"
                        } else {
                            "request_failed"
                        },
                        502,
                    )));
                }
            };
            // A final response can arrive before Hyper finishes consuming the
            // upload. Do not let a retained body keep reading after this attempt
            // returns or releases its leases, even if the peer keeps receiving.
            upload_guard.cancel();
            progress.mark();
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
            lease.observed = Some((gate_status, retry_after));
            host_lease.observed = Some((if status == 429 { 429 } else { 0 }, retry_after));
            if !upload.complete() {
                // Even an early rejection of a still-streaming upload carries
                // authoritative pacing metadata. Cancellation must not erase
                // an observed 429 or Retry-After from either request gate.
                self.policy
                    .release(&scope, &token, gate_status, retry_after, false)
                    .map_err(&after_dispatch)?;
                lease.released = true;
                self.policy
                    .release(
                        &host_scope,
                        &host_token,
                        if status == 429 { 429 } else { 0 },
                        retry_after,
                        false,
                    )
                    .map_err(&after_dispatch)?;
                host_lease.released = true;
                return Err(after_dispatch(AppError::api("request_failed", 502)));
            }
            let file_status = receiver.is_some() && (200..300).contains(&status);
            if receiver.is_some()
                && (status == 206
                    || headers
                        .iter()
                        .any(|(name, _)| name.eq_ignore_ascii_case("content-range")))
            {
                return Err(after_dispatch(AppError::api("response_mismatch", 502)));
            }
            if !file_status
                && response
                    .content_length()
                    .is_some_and(|length| length > input.limit as u64)
            {
                release_http_leases(
                    &mut lease,
                    &mut host_lease,
                    gate_status,
                    status,
                    retry_after,
                    false,
                )
                .map_err(&after_dispatch)?;
                return Err(after_dispatch(AppError::api("too_large", 413)));
            }
            // A new stage belongs only to this attempt. Redirect, retry, OAuth
            // replacement, error and cancellation all drop its cleanup owner.
            let mut writable = if file_status {
                Some(
                    monitor
                        .wait(receiver.expect("file receiver").begin(), None)
                        .await
                        .map_err(&after_dispatch)?
                        .map_err(&after_dispatch)?,
                )
            } else {
                None
            };
            progress.mark();
            let mut bytes = Vec::new();
            let mut stream = response.bytes_stream();
            while let Some(chunk) = monitor
                .wait(stream.next(), idle_timeout)
                .await
                .map_err(&after_dispatch)?
            {
                let chunk =
                    chunk.map_err(|_| after_dispatch(AppError::api("request_failed", 502)))?;
                if !chunk.is_empty() {
                    progress.mark();
                }
                if let Some(file) = writable.as_mut() {
                    monitor
                        .wait(file.write(&chunk), None)
                        .await
                        .map_err(&after_dispatch)?
                        .map_err(&after_dispatch)?;
                    // Awaited local writes are backpressure, not network idle.
                    progress.mark();
                } else {
                    if chunk.len() > input.limit.saturating_sub(bytes.len()) {
                        release_http_leases(
                            &mut lease,
                            &mut host_lease,
                            gate_status,
                            status,
                            retry_after,
                            false,
                        )
                        .map_err(&after_dispatch)?;
                        return Err(after_dispatch(AppError::api("too_large", 413)));
                    }
                    bytes.extend_from_slice(&chunk);
                }
            }
            let artifact = match writable {
                Some(file) => Some(
                    monitor
                        .wait(file.seal(), None)
                        .await
                        .map_err(&after_dispatch)?
                        .map_err(&after_dispatch)?,
                ),
                None => None,
            };
            if !upload.complete() {
                return Err(after_dispatch(AppError::api("request_failed", 502)));
            }
            monitor.renew().map_err(&after_dispatch)?;
            let (defense, _) = defend::classify(&defend::Observation {
                status_code: status,
                headers: headers.clone(),
                body_snippet: String::from_utf8_lossy(
                    artifact
                        .as_ref()
                        .map(|file| file.prefix())
                        .unwrap_or(&bytes[..bytes.len().min(16_384)]),
                )
                .into_owned(),
            });
            let challenged = defense != defend::Class::None && defense != defend::Class::RateLimit;
            let strategy_key =
                Policy::strategy_key(&input.method, target.as_str(), "http", &input.account_id);
            if challenged {
                let _ =
                    self.policy
                        .record_blocked_strategy(&strategy_key, defense.as_str(), 300_000);
            } else if (200..300).contains(&status) {
                let _ = self.policy.clear_strategy(&strategy_key);
            }
            release_http_leases(
                &mut lease,
                &mut host_lease,
                gate_status,
                status,
                retry_after,
                challenged,
            )
            .map_err(&after_dispatch)?;
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
            return Ok(FileResponse {
                response: Response {
                    url: if continuation.is_some() {
                        input.url.clone()
                    } else {
                        target.to_string()
                    },
                    status,
                    headers,
                    bytes,
                    duration_ms: start.elapsed().as_millis() as i64,
                    defense: defense.as_str().into(),
                },
                artifact,
            });
        }
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum TransferMode {
    Buffered,
    File,
}

/// Progress is recorded when the HTTP body consumer asks for an upload chunk,
/// or the owning receiver accepts a response chunk. There is no producer task.
struct TransferProgress {
    last: Mutex<tokio::time::Instant>,
    changed: tokio::sync::Notify,
    local_work: AtomicUsize,
}
impl TransferProgress {
    fn new() -> Self {
        Self {
            last: Mutex::new(tokio::time::Instant::now()),
            changed: tokio::sync::Notify::new(),
            local_work: AtomicUsize::new(0),
        }
    }
    fn mark(&self) {
        *self.last.lock().expect("transfer progress mutex") = tokio::time::Instant::now();
        self.changed.notify_one();
    }
    fn deadline(&self, timeout: Duration) -> Option<tokio::time::Instant> {
        (self.local_work.load(Ordering::SeqCst) == 0)
            .then(|| *self.last.lock().expect("transfer progress mutex") + timeout)
    }
    fn local_work(self: &Arc<Self>) -> LocalTransferWork {
        self.local_work.fetch_add(1, Ordering::SeqCst);
        self.changed.notify_one();
        LocalTransferWork {
            progress: self.clone(),
        }
    }
}

/// A body read is local disk work only while its consumer is actively polling
/// that read. No guard lives across a yielded chunk or an idle HTTP consumer.
struct LocalTransferWork {
    progress: Arc<TransferProgress>,
}
impl Drop for LocalTransferWork {
    fn drop(&mut self) {
        // Record resumption before making a network idle deadline visible.
        self.progress.mark();
        self.progress.local_work.fetch_sub(1, Ordering::SeqCst);
        self.progress.changed.notify_one();
    }
}

/// Wait in the attempt's owning future, renewing exact lease tokens. Passing
/// `None` disables network-idle accounting while awaiting local disk work;
/// callers mark progress after that work before awaiting network input again.
/// Dropping this monitor/future drops the transfer, not a detached worker.
struct TransferMonitor<'a> {
    policy: &'a Policy,
    leases: Vec<(&'a str, &'a str)>,
    progress: Arc<TransferProgress>,
    renew_at: tokio::time::Instant,
}
impl<'a> TransferMonitor<'a> {
    const RENEW_EVERY: Duration = Duration::from_secs(30);

    fn new(
        policy: &'a Policy,
        leases: Vec<(&'a str, &'a str)>,
        progress: Arc<TransferProgress>,
    ) -> Self {
        Self {
            policy,
            leases,
            progress,
            renew_at: tokio::time::Instant::now() + Self::RENEW_EVERY,
        }
    }
    fn renew(&mut self) -> AppResult<()> {
        for (scope, token) in &self.leases {
            self.policy.renew(scope, token)?;
        }
        self.renew_at = tokio::time::Instant::now() + Self::RENEW_EVERY;
        Ok(())
    }
    async fn wait<F: std::future::Future>(
        &mut self,
        future: F,
        idle: Option<Duration>,
    ) -> AppResult<F::Output> {
        tokio::pin!(future);
        loop {
            // A newly registered sleep can initially return Pending even when
            // already due. Check synchronously before polling a ready transfer.
            if self.renew_at <= tokio::time::Instant::now() {
                self.renew()?;
            }
            let idle_deadline = idle.and_then(|timeout| self.progress.deadline(timeout));
            if idle_deadline.is_some_and(|deadline| deadline <= tokio::time::Instant::now()) {
                return Err(AppError::api("timeout", 502));
            }
            tokio::select! {
                biased;
                _ = tokio::time::sleep_until(self.renew_at) => self.renew()?,
                _ = async {
                    match idle_deadline {
                        Some(deadline) => tokio::time::sleep_until(deadline).await,
                        None => std::future::pending::<()>().await,
                    }
                } => {
                    // A body poll may have advanced progress while this timer
                    // was queued. Recheck rather than timing out an active body.
                    if idle.and_then(|timeout| self.progress.deadline(timeout)).is_some_and(|deadline| deadline <= tokio::time::Instant::now()) {
                        return Err(AppError::api("timeout", 502));
                    }
                },
                _ = self.progress.changed.notified(), if idle.is_some() => {},
                output = &mut future => {
                    if self.renew_at <= tokio::time::Instant::now() { self.renew()?; }
                    return Ok(output);
                },
            }
        }
    }
}

fn release_http_leases(
    account: &mut Lease<'_>,
    host: &mut Lease<'_>,
    gate_status: u16,
    status: u16,
    retry_after: Option<Duration>,
    challenged: bool,
) -> AppResult<()> {
    account.policy.release(
        account.scope,
        account.token,
        gate_status,
        retry_after,
        challenged,
    )?;
    account.released = true;
    host.policy.release(
        host.scope,
        host.token,
        if status == 429 { 429 } else { 0 },
        retry_after,
        false,
    )?;
    host.released = true;
    Ok(())
}

struct Lease<'a> {
    policy: &'a Policy,
    scope: &'a str,
    token: &'a str,
    released: bool,
    // Retain known pacing metadata through every later response/body failure.
    observed: Option<(u16, Option<Duration>)>,
}
impl Drop for Lease<'_> {
    fn drop(&mut self) {
        if !self.released {
            let (status, retry_after) = self.observed.unwrap_or((0, None));
            let _ = self
                .policy
                .release(self.scope, self.token, status, retry_after, false);
        }
    }
}

/// Normalize a user-entered website address. Request and SiteSpec validation
/// still require an explicit scheme via `validate_url`.
pub fn website_url(raw: &str) -> AppResult<url::Url> {
    let raw = raw.trim();
    let address = if raw.starts_with("//") {
        format!("https:{raw}")
    } else if raw.contains("://") {
        raw.to_owned()
    } else {
        format!("https://{raw}")
    };
    validate_url(&address)
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

pub fn registrable_domain(host: &str) -> Option<String> {
    let host = host.trim_matches('.').to_ascii_lowercase();
    if host.is_empty()
        || host == "localhost"
        || host.parse::<IpAddr>().is_ok()
        || host.starts_with('[')
    {
        return None;
    }
    let parts: Vec<&str> = host.split('.').collect();
    if parts.len() < 2 {
        return None;
    }
    let tail2 = format!("{}.{}", parts[parts.len() - 2], parts[parts.len() - 1]);
    let multipart_tlds = [
        "co.kr", "or.kr", "ne.kr", "go.kr", "ac.kr", "co.jp", "ne.jp", "or.jp", "ac.jp", "go.jp",
        "co.uk", "org.uk", "ac.uk", "gov.uk", "com.au", "net.au", "org.au", "com.br", "com.cn",
        "com.tw", "com.hk", "com.sg",
    ];
    if parts.len() >= 3 && multipart_tlds.contains(&tail2.as_str()) {
        Some(format!(
            "{}.{}.{}",
            parts[parts.len() - 3],
            parts[parts.len() - 2],
            parts[parts.len() - 1]
        ))
    } else {
        Some(tail2)
    }
}

/// True when two URLs share the exact origin or belong to the same registrable domain (eTLD+1)
/// with the same scheme and port (e.g. `https://suno.com` and `https://studio-api-prod.suno.com`).
pub fn same_site_origin(a: &url::Url, b: &url::Url) -> bool {
    if a.origin() == b.origin() {
        return true;
    }
    if a.scheme() != b.scheme() || a.port_or_known_default() != b.port_or_known_default() {
        return false;
    }
    let (Some(ha), Some(hb)) = (a.host_str(), b.host_str()) else {
        return false;
    };
    match (registrable_domain(ha), registrable_domain(hb)) {
        (Some(da), Some(db)) => da == db,
        _ => false,
    }
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

const UPLOAD_CHUNK_BYTES: usize = 64 * 1024;

#[derive(Default)]
struct UploadCompletion {
    pending: AtomicUsize,
    failed: AtomicBool,
    cancelled: AtomicBool,
    cancellation: tokio::sync::Notify,
    production: Mutex<()>,
    #[cfg(test)]
    produced: AtomicUsize,
}
impl UploadCompletion {
    fn check_cancelled(&self) -> std::io::Result<()> {
        if self.cancelled.load(Ordering::SeqCst) {
            Err(std::io::Error::new(
                std::io::ErrorKind::Interrupted,
                "upload attempt ended",
            ))
        } else {
            Ok(())
        }
    }
    async fn cancellation(&self) {
        let notified = self.cancellation.notified();
        tokio::pin!(notified);
        notified.as_mut().enable();
        if !self.cancelled.load(Ordering::SeqCst) {
            notified.await;
        }
    }
    fn complete(&self) -> bool {
        self.pending.load(Ordering::SeqCst) == 0 && !self.failed.load(Ordering::SeqCst)
    }
}

struct UploadGuard(Arc<UploadCompletion>);
impl UploadGuard {
    fn cancel(&self) {
        let _production = self.0.production.lock().expect("upload production mutex");
        self.0.cancelled.store(true, Ordering::SeqCst);
        self.0.cancellation.notify_waiters();
    }
}
impl Drop for UploadGuard {
    fn drop(&mut self) {
        self.cancel();
    }
}

#[cfg(test)]
static UPLOAD_PROBES: Mutex<Vec<(String, Arc<UploadCompletion>)>> = Mutex::new(Vec::new());

/// One explicitly selected regular file. Metadata and bytes come from the same
/// opened handle. The initial length is the contract, not a growing tail. This
/// detects observed changes; it does not make the underlying file immutable.
struct UploadFile {
    file: tokio::fs::File,
    initial: std::fs::Metadata,
    remaining: u64,
    finished: bool,
    progress: Arc<TransferProgress>,
    completion: Arc<UploadCompletion>,
}
impl UploadFile {
    async fn open(
        path: &str,
        progress: Arc<TransferProgress>,
        completion: Arc<UploadCompletion>,
    ) -> AppResult<Self> {
        let mut options = tokio::fs::OpenOptions::new();
        options.read(true);
        // Opening a FIFO/device must not wait for another process before the
        // same-handle regular-file check can reject it.
        #[cfg(unix)]
        options.custom_flags(libc::O_NONBLOCK);
        let file = options
            .open(path)
            .await
            .map_err(|_| AppError::api("data_invalid", 400))?;
        let initial = file
            .metadata()
            .await
            .map_err(|_| AppError::api("data_invalid", 400))?;
        if !initial.is_file() {
            return Err(AppError::api("data_invalid", 400));
        }
        #[cfg(test)]
        UPLOAD_PROBES
            .lock()
            .unwrap()
            .push((path.into(), completion.clone()));
        let remaining = initial.len();
        if remaining != 0 {
            completion.pending.fetch_add(1, Ordering::SeqCst);
        }
        Ok(Self {
            file,
            initial,
            remaining,
            finished: remaining == 0,
            progress,
            completion,
        })
    }
    async fn check_metadata(&self) -> std::io::Result<()> {
        let current = self.file.metadata().await?;
        if !upload_metadata_matches(&self.initial, &current) {
            return Err(std::io::Error::other("upload source changed"));
        }
        Ok(())
    }
    async fn next_chunk(&mut self) -> std::io::Result<Option<Vec<u8>>> {
        // Final metadata was checked before yielding the last advertised byte.
        // Preserve EOF when an early response cancels an already finished file;
        // manufacturing an error here could abort a valid response connection.
        if self.finished {
            return Ok(None);
        }
        let completion = self.completion.clone();
        completion.check_cancelled()?;
        let result = tokio::select! {
            biased;
            _ = completion.cancellation() => return Err(std::io::Error::new(std::io::ErrorKind::Interrupted, "upload attempt ended")),
            result = self.read_chunk() => result,
        };
        let _production = completion
            .production
            .lock()
            .expect("upload production mutex");
        completion.check_cancelled()?;
        if result.as_ref().is_ok_and(|chunk| chunk.is_some()) {
            if self.remaining == 0 {
                completion.pending.fetch_sub(1, Ordering::SeqCst);
                self.finished = true;
            }
            self.progress.mark();
            #[cfg(test)]
            completion.produced.fetch_add(1, Ordering::SeqCst);
        }
        result
    }
    async fn read_chunk(&mut self) -> std::io::Result<Option<Vec<u8>>> {
        let _local_work = self.progress.local_work();
        if self.remaining == 0 {
            return Ok(None);
        }
        self.check_metadata().await?;
        let chunk = read_upload_chunk(&mut self.file, self.remaining).await?;
        self.remaining -= chunk.len() as u64;
        // Validate before yielding the last advertised byte. A length-aware
        // HTTP consumer is not required to poll this stream once more at EOF.
        self.check_metadata().await?;
        Ok(Some(chunk))
    }
    fn body(self) -> reqwest::Body {
        reqwest::Body::wrap_stream(futures_util::stream::try_unfold(
            self,
            |mut source| async move {
                match source.next_chunk().await {
                    Ok(Some(chunk)) => Ok(Some((chunk, source))),
                    Ok(None) => Ok(None),
                    Err(error) => {
                        source.completion.failed.store(true, Ordering::SeqCst);
                        Err(error)
                    }
                }
            },
        ))
    }
}

async fn read_upload_chunk(
    reader: &mut (impl tokio::io::AsyncRead + Unpin),
    remaining: u64,
) -> std::io::Result<Vec<u8>> {
    use tokio::io::AsyncReadExt;
    let mut chunk = vec![0; remaining.min(UPLOAD_CHUNK_BYTES as u64) as usize];
    let read = reader.read(&mut chunk).await?;
    if read == 0 && remaining != 0 {
        return Err(std::io::Error::new(
            std::io::ErrorKind::UnexpectedEof,
            "upload source ended early",
        ));
    }
    chunk.truncate(read);
    Ok(chunk)
}

fn upload_metadata_matches(initial: &std::fs::Metadata, current: &std::fs::Metadata) -> bool {
    if !current.is_file()
        || initial.len() != current.len()
        || initial.modified().ok() != current.modified().ok()
        || initial.created().ok() != current.created().ok()
    {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if initial.dev() != current.dev()
            || initial.ino() != current.ino()
            || initial.ctime() != current.ctime()
            || initial.ctime_nsec() != current.ctime_nsec()
        {
            return false;
        }
    }
    true
}

/// Build a bounded-memory multipart stream. Each part retains its original
/// regular-file handle; no byte ceiling, full-buffer read or disk snapshot.
async fn build_multipart_form(
    parts: &[crate::spec::MultipartPart],
    progress: Arc<TransferProgress>,
    completion: Arc<UploadCompletion>,
) -> AppResult<reqwest::multipart::Form> {
    use crate::spec::MultipartPart;
    let mut form = reqwest::multipart::Form::new();
    for part in parts {
        match part {
            MultipartPart::Text { name, value } => {
                form = form.text(name.clone(), value.clone());
            }
            MultipartPart::File { name, path } => {
                let source = UploadFile::open(path, progress.clone(), completion.clone()).await?;
                let length = source.remaining;
                let filename = std::path::Path::new(path)
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_else(|| "upload".into());
                let ext = std::path::Path::new(path)
                    .extension()
                    .and_then(|e| e.to_str())
                    .map(|e| e.to_ascii_lowercase());
                let mime = match ext.as_deref() {
                    Some("gif") => "image/gif",
                    Some("png") => "image/png",
                    Some("jpg") | Some("jpeg") => "image/jpeg",
                    Some("webp") => "image/webp",
                    Some("mp4") => "video/mp4",
                    _ => "application/octet-stream",
                };
                let field = reqwest::multipart::Part::stream_with_length(source.body(), length)
                    .file_name(filename)
                    .mime_str(mime)
                    .map_err(|_| AppError::api("data_invalid", 400))?;
                form = form.part(name.clone(), field);
            }
        }
    }
    Ok(form)
}

#[cfg(test)]
mod website_input_tests {
    use super::*;
    fn transfer_root() -> std::path::PathBuf {
        let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join(".cache/upload-file-tests")
            .join(util::id());
        std::fs::create_dir_all(&root).unwrap();
        root
    }
    async fn source(path: &std::path::Path) -> UploadFile {
        UploadFile::open(
            path.to_str().unwrap(),
            Arc::new(TransferProgress::new()),
            Arc::new(UploadCompletion::default()),
        )
        .await
        .unwrap()
    }
    fn upload_request(url: String, path: &std::path::Path, timeout: Duration) -> Request {
        Request {
            url,
            method: "POST".into(),
            headers: vec![],
            body: None,
            account_id: String::new(),
            limit: 4096,
            timeout,
            read_only: false,
            follow_redirects: true,
            tls_profile: "hycli".into(),
            multipart: Some(vec![crate::spec::MultipartPart::File {
                name: "file".into(),
                path: path.to_string_lossy().into_owned(),
            }]),
        }
    }

    #[tokio::test]
    async fn source_chunk_reader_preserves_early_eof_and_read_failures() {
        let mut short = &b"abc"[..];
        assert_eq!(read_upload_chunk(&mut short, 4).await.unwrap(), b"abc");
        assert_eq!(
            read_upload_chunk(&mut short, 1).await.unwrap_err().kind(),
            std::io::ErrorKind::UnexpectedEof
        );
        struct FailingReader;
        impl tokio::io::AsyncRead for FailingReader {
            fn poll_read(
                self: std::pin::Pin<&mut Self>,
                _: &mut std::task::Context<'_>,
                _: &mut tokio::io::ReadBuf<'_>,
            ) -> std::task::Poll<std::io::Result<()>> {
                std::task::Poll::Ready(Err(std::io::Error::other("synthetic read failure")))
            }
        }
        assert_eq!(
            read_upload_chunk(&mut FailingReader, 1)
                .await
                .unwrap_err()
                .kind(),
            std::io::ErrorKind::Other
        );
    }

    #[tokio::test]
    async fn file_stream_exceeds_old_budget_with_one_bounded_chunk() {
        let root = transfer_root();
        let path = root.join("sparse.bin");
        let length = 100 * 1024 * 1024 + UPLOAD_CHUNK_BYTES as u64 + 7;
        std::fs::File::create(&path)
            .unwrap()
            .set_len(length)
            .unwrap();
        let mut source = source(&path).await;
        let mut actual = 0;
        let mut peak_buffer = 0;
        while let Some(chunk) = source.next_chunk().await.unwrap() {
            actual += chunk.len() as u64;
            peak_buffer = peak_buffer.max(chunk.capacity());
            assert!(chunk.iter().all(|byte| *byte == 0));
        }
        assert_eq!(actual, length);
        assert_eq!(peak_buffer, UPLOAD_CHUNK_BYTES);
        assert!(source.completion.complete());
        drop(source);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn multipart_over_old_budget_reaches_bounded_http_consumer() {
        use axum::{Json, Router, routing::post};
        let root = transfer_root();
        let path = root.join("sparse.bin");
        let length = 100 * 1024 * 1024 + 17;
        std::fs::File::create(&path)
            .unwrap()
            .set_len(length)
            .unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let origin = format!("http://{}", listener.local_addr().unwrap());
        let server = tokio::spawn(async move {
            axum::serve(
                listener,
                Router::new().route(
                    "/upload",
                    post(|request: axum::extract::Request| async move {
                        let mut body = request.into_body().into_data_stream();
                        let (mut zeros, mut total) = (0_u64, 0_u64);
                        while let Some(chunk) = body.next().await {
                            let chunk = chunk.unwrap();
                            zeros += chunk.iter().filter(|byte| **byte == 0).count() as u64;
                            total += chunk.len() as u64;
                        }
                        Json(serde_json::json!({"zeros": zeros, "total": total}))
                    }),
                ),
            )
            .await
            .unwrap();
        });
        let policy = Arc::new(Policy::with_limits(&root.join("policy.db"), 0, 100).unwrap());
        let network = Network::new(policy, true);
        let response = network
            .fetch(upload_request(
                format!("{origin}/upload"),
                &path,
                Duration::from_secs(5),
            ))
            .await
            .unwrap();
        let result = response.json().unwrap();
        assert_eq!(result["zeros"].as_u64(), Some(length));
        assert!(result["total"].as_u64().unwrap() < length + 4096);
        server.abort();
        let _ = server.await;
        drop(network);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn truncation_growth_and_same_length_rewrites_fail_instead_of_following_tail() {
        use std::io::{Seek, Write};
        let root = transfer_root();
        for mutation in ["truncate", "grow", "rewrite"] {
            let path = root.join(mutation);
            std::fs::write(&path, vec![b'x'; 2 * UPLOAD_CHUNK_BYTES]).unwrap();
            let mut source = source(&path).await;
            assert_eq!(
                source.next_chunk().await.unwrap().unwrap().len(),
                UPLOAD_CHUNK_BYTES
            );
            // Ensure timestamp-only platforms can observe the rewrite as well.
            tokio::time::sleep(Duration::from_millis(20)).await;
            let mut writer = std::fs::OpenOptions::new().write(true).open(&path).unwrap();
            match mutation {
                "truncate" => writer.set_len(1).unwrap(),
                "grow" => writer.set_len(3 * UPLOAD_CHUNK_BYTES as u64).unwrap(),
                _ => {
                    writer
                        .seek(std::io::SeekFrom::Start(UPLOAD_CHUNK_BYTES as u64))
                        .unwrap();
                    writer.write_all(b"changed").unwrap();
                }
            }
            assert!(source.next_chunk().await.is_err(), "{mutation}");
        }
        std::fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn replacement_path_does_not_change_the_open_file_identity() {
        let root = transfer_root();
        let path = root.join("selected");
        std::fs::write(&path, b"selected-by-user").unwrap();
        let mut source = source(&path).await;
        std::fs::rename(&path, root.join("original")).unwrap();
        std::fs::write(&path, b"not-selected").unwrap();
        // Renaming can update ctime and be conservatively rejected. If accepted,
        // it must still read the selected handle, never reopen the path.
        if let Ok(Some(bytes)) = source.next_chunk().await {
            assert_eq!(bytes, b"selected-by-user");
        }
        drop(source);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn devices_and_directories_are_not_upload_sources() {
        let root = transfer_root();
        for path in ["/dev/null", root.to_str().unwrap()] {
            assert!(
                UploadFile::open(
                    path,
                    Arc::new(TransferProgress::new()),
                    Arc::new(UploadCompletion::default())
                )
                .await
                .is_err()
            );
        }
        std::fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn idle_monitor_tracks_upload_progress_and_does_not_timeout_disk_waits() {
        let root = transfer_root();
        let policy = Policy::with_limits(&root.join("policy.db"), 0, 100).unwrap();
        let token = policy.acquire("fixture").await.unwrap();
        let progress = Arc::new(TransferProgress::new());
        let mut monitor =
            TransferMonitor::new(&policy, vec![("fixture", &token)], progress.clone());
        let active = async {
            for _ in 0..8 {
                tokio::time::sleep(Duration::from_millis(10)).await;
                progress.mark();
            }
        };
        monitor
            .wait(active, Some(Duration::from_millis(30)))
            .await
            .unwrap();
        assert_eq!(
            monitor
                .wait(
                    std::future::pending::<()>(),
                    Some(Duration::from_millis(30))
                )
                .await
                .unwrap_err()
                .public_code,
            "timeout"
        );
        monitor
            .wait(tokio::time::sleep(Duration::from_millis(60)), None)
            .await
            .unwrap();
        progress.mark();
        monitor
            .wait(
                async {
                    let _disk_read = progress.local_work();
                    tokio::time::sleep(Duration::from_millis(60)).await;
                },
                Some(Duration::from_millis(30)),
            )
            .await
            .unwrap();
        assert_eq!(
            monitor
                .wait(
                    std::future::pending::<()>(),
                    Some(Duration::from_millis(30))
                )
                .await
                .unwrap_err()
                .public_code,
            "timeout"
        );
        drop(monitor);
        drop(policy);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn monitor_stops_at_lost_lease_without_reacquiring() {
        let root = transfer_root();
        let policy = Policy::with_limits(&root.join("policy.db"), 0, 100).unwrap();
        let token = policy.acquire("fixture").await.unwrap();
        let mut monitor = TransferMonitor::new(
            &policy,
            vec![("fixture", &token)],
            Arc::new(TransferProgress::new()),
        );
        policy.release("fixture", &token, 0, None, false).unwrap();
        let replacement = policy.acquire("fixture").await.unwrap();
        monitor.renew_at = tokio::time::Instant::now();
        assert_eq!(
            monitor
                .wait(std::future::ready(()), None)
                .await
                .unwrap_err()
                .public_code,
            "request_lease_lost"
        );
        policy.renew("fixture", &replacement).unwrap();
        drop(monitor);
        drop(policy);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn pool_identity_separates_file_mode_and_subsecond_timeouts() {
        let root = transfer_root();
        let network = Network::new(
            Arc::new(Policy::open(&root.join("policy.db")).unwrap()),
            true,
        );
        let addresses = ["127.0.0.1:80".parse().unwrap()];
        for (mode, timeout) in [
            (TransferMode::Buffered, 30),
            (TransferMode::File, 30),
            (TransferMode::Buffered, 40),
        ] {
            network
                .pooled_client(
                    "fixture.test",
                    "test",
                    &addresses,
                    Duration::from_millis(timeout),
                    mode,
                )
                .unwrap();
        }
        assert_eq!(network.clients.lock().unwrap().len(), 3);
        drop(network);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn late_source_failure_is_uncertain_and_never_replayed() {
        use axum::{Router, http::StatusCode, routing::post};
        let root = transfer_root();
        let path = root.join("selected.bin");
        std::fs::File::create(&path)
            .unwrap()
            .set_len(128 * 1024 * 1024)
            .unwrap();
        let mutation_path = path.clone();
        let hits = Arc::new(AtomicUsize::new(0));
        let server_hits = hits.clone();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let origin = format!("http://{}", listener.local_addr().unwrap());
        let server = tokio::spawn(async move {
            axum::serve(
                listener,
                Router::new().route(
                    "/upload",
                    post(move |request: axum::extract::Request| {
                        let path = mutation_path.clone();
                        let hits = server_hits.clone();
                        async move {
                            hits.fetch_add(1, Ordering::SeqCst);
                            let mut body = request.into_body().into_data_stream();
                            assert!(body.next().await.is_some());
                            std::fs::OpenOptions::new()
                                .write(true)
                                .open(path)
                                .unwrap()
                                .set_len(0)
                                .unwrap();
                            while let Some(chunk) = body.next().await {
                                if chunk.is_err() {
                                    break;
                                }
                            }
                            StatusCode::SERVICE_UNAVAILABLE
                        }
                    }),
                ),
            )
            .await
            .unwrap();
        });
        let network = Network::new(
            Arc::new(Policy::with_limits(&root.join("policy.db"), 0, 100).unwrap()),
            true,
        );
        let error = network
            .fetch(upload_request(
                format!("{origin}/upload"),
                &path,
                Duration::from_secs(3),
            ))
            .await
            .err()
            .unwrap();
        assert_eq!(error.public_code, "action_outcome_unknown");
        assert_eq!(hits.load(Ordering::SeqCst), 1);
        server.abort();
        let _ = server.await;
        drop(network);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn upload_idle_failure_is_uncertain_and_never_replayed() {
        use tokio::io::AsyncReadExt;
        let root = transfer_root();
        let path = root.join("selected.bin");
        std::fs::write(&path, b"selected-file").unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let origin = format!("http://{}", listener.local_addr().unwrap());
        let hits = Arc::new(AtomicUsize::new(0));
        let server_hits = hits.clone();
        let server = tokio::spawn(async move {
            loop {
                let (mut socket, _) = listener.accept().await.unwrap();
                server_hits.fetch_add(1, Ordering::SeqCst);
                let mut chunk = [0; 4096];
                let _ = socket.read(&mut chunk).await;
                tokio::time::sleep(Duration::from_secs(10)).await;
            }
        });
        let network = Network::new(
            Arc::new(Policy::with_limits(&root.join("policy.db"), 0, 100).unwrap()),
            true,
        );
        let error = network
            .fetch(upload_request(
                format!("{origin}/upload"),
                &path,
                Duration::from_millis(100),
            ))
            .await
            .err()
            .unwrap();
        assert_eq!(error.public_code, "action_outcome_unknown");
        assert_eq!(hits.load(Ordering::SeqCst), 1);
        server.abort();
        let _ = server.await;
        drop(network);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn lease_loss_after_dispatch_is_uncertain_and_does_not_release_new_owner() {
        use axum::{Json, Router, routing::post};
        let root = transfer_root();
        let path = root.join("selected.bin");
        std::fs::write(&path, b"selected-file").unwrap();
        let policy_path = root.join("policy.db");
        let policy = Arc::new(Policy::with_limits(&policy_path, 0, 100).unwrap());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let origin = format!("http://{}", listener.local_addr().unwrap());
        let host_scope = format!("{origin}|*");
        let server_scope = host_scope.clone();
        let server_db = policy_path.clone();
        let hits = Arc::new(AtomicUsize::new(0));
        let server_hits = hits.clone();
        let server = tokio::spawn(async move {
            axum::serve(
                listener,
                Router::new().route(
                    "/upload",
                    post(move |request: axum::extract::Request| {
                        let db = server_db.clone();
                        let scope = server_scope.clone();
                        let hits = server_hits.clone();
                        async move {
                            hits.fetch_add(1, Ordering::SeqCst);
                            let mut body = request.into_body().into_data_stream();
                            while let Some(chunk) = body.next().await {
                                chunk.unwrap();
                            }
                            rusqlite::Connection::open(db).unwrap().execute(
                        "UPDATE request_gate SET lease='replacement-owner' WHERE scope=?1", [scope],
                    ).unwrap();
                            Json(serde_json::json!({"ok":true}))
                        }
                    }),
                ),
            )
            .await
            .unwrap();
        });
        let network = Network::new(policy.clone(), true);
        let error = network
            .fetch(upload_request(
                format!("{origin}/upload"),
                &path,
                Duration::from_secs(3),
            ))
            .await
            .err()
            .unwrap();
        assert_eq!(error.public_code, "action_outcome_unknown");
        assert_eq!(hits.load(Ordering::SeqCst), 1);
        policy.renew(&host_scope, "replacement-owner").unwrap();
        server.abort();
        let _ = server.await;
        drop(network);
        drop(policy);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn dropping_upload_future_releases_owned_leases_without_a_transfer_task() {
        use tokio::io::AsyncReadExt;
        let root = transfer_root();
        let path = root.join("selected.bin");
        std::fs::File::create(&path)
            .unwrap()
            .set_len(128 * 1024 * 1024)
            .unwrap();
        let policy_path = root.join("policy.db");
        let network = Network::new(
            Arc::new(Policy::with_limits(&policy_path, 0, 100).unwrap()),
            true,
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let origin = format!("http://{}", listener.local_addr().unwrap());
        let received = Arc::new(tokio::sync::Notify::new());
        let server_received = received.clone();
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut bytes = [0; 4096];
            assert_ne!(socket.read(&mut bytes).await.unwrap(), 0);
            server_received.notify_one();
            tokio::time::sleep(Duration::from_secs(60)).await;
        });
        {
            let fetch = network.fetch(upload_request(
                format!("{origin}/upload"),
                &path,
                Duration::from_secs(30),
            ));
            tokio::pin!(fetch);
            tokio::select! {
                _ = received.notified() => {},
                _ = &mut fetch => panic!("upload should remain pending"),
            }
        }
        let active: i64 = rusqlite::Connection::open(policy_path)
            .unwrap()
            .query_row(
                "SELECT COUNT(*) FROM request_gate WHERE lease<>''",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(active, 0);
        server.abort();
        let _ = server.await;
        drop(network);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn retained_file_source_stops_when_owning_attempt_is_dropped() {
        let root = transfer_root();
        let path = root.join("selected.bin");
        std::fs::File::create(&path)
            .unwrap()
            .set_len(2 * UPLOAD_CHUNK_BYTES as u64)
            .unwrap();
        let mut source = source(&path).await;
        let guard = UploadGuard(source.completion.clone());
        assert!(source.next_chunk().await.unwrap().is_some());
        let produced = source.completion.produced.load(Ordering::SeqCst);
        drop(guard);
        assert_eq!(
            source.next_chunk().await.unwrap_err().kind(),
            std::io::ErrorKind::Interrupted
        );
        assert_eq!(source.completion.produced.load(Ordering::SeqCst), produced);
        drop(source);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn early_final_response_stops_new_source_production_while_peer_keeps_reading() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let root = transfer_root();
        let path = root.join("selected.bin");
        std::fs::File::create(&path)
            .unwrap()
            .set_len(256 * 1024 * 1024)
            .unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let origin = format!("http://{}", listener.local_addr().unwrap());
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut header = Vec::new();
            while !header.ends_with(b"\r\n\r\n") {
                header.push(socket.read_u8().await.unwrap());
                assert!(header.len() < 8192);
            }
            socket
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n")
                .await
                .unwrap();
            // Deliberately retain the connection and continue consuming any
            // body data. Bytes already queued by Hyper/the kernel are allowed.
            let until = tokio::time::Instant::now() + Duration::from_millis(200);
            let mut bytes = [0; UPLOAD_CHUNK_BYTES];
            loop {
                match tokio::time::timeout_at(until, socket.read(&mut bytes)).await {
                    Ok(Ok(0)) | Ok(Err(_)) | Err(_) => break,
                    Ok(Ok(_)) => {}
                }
            }
        });
        let network = Network::new(
            Arc::new(Policy::with_limits(&root.join("policy.db"), 0, 100).unwrap()),
            true,
        );
        let error = network
            .fetch(upload_request(
                format!("{origin}/upload"),
                &path,
                Duration::from_secs(3),
            ))
            .await
            .err()
            .unwrap();
        assert_eq!(error.public_code, "action_outcome_unknown");
        let probe = UPLOAD_PROBES
            .lock()
            .unwrap()
            .iter()
            .rev()
            .find(|(source, _)| source == path.to_str().unwrap())
            .unwrap()
            .1
            .clone();
        let produced_at_return = probe.produced.load(Ordering::SeqCst);
        assert!(probe.cancelled.load(Ordering::SeqCst));
        server.await.unwrap();
        tokio::time::sleep(Duration::from_millis(50)).await;
        assert_eq!(probe.produced.load(Ordering::SeqCst), produced_at_return);
        assert!(produced_at_return < 256 * 1024 * 1024 / UPLOAD_CHUNK_BYTES);
        drop(network);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn authority_is_rechecked_after_waiting_for_lease_renewal_write_lock() {
        let root = transfer_root();
        let path = root.join("selected.bin");
        std::fs::write(&path, b"selected-file").unwrap();
        let policy_path = root.join("policy.db");
        let network = Network::new(
            Arc::new(Policy::with_limits(&policy_path, 0, 100).unwrap()),
            true,
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let origin = format!("http://{}", listener.local_addr().unwrap());
        let changed = Arc::new(AtomicBool::new(false));
        let changed_on_lock = changed.clone();
        let (start, started) = std::sync::mpsc::channel();
        let (locked, lock_ready) = std::sync::mpsc::channel();
        let lock_ready = Mutex::new(lock_ready);
        let writer = std::thread::spawn(move || {
            started.recv().unwrap();
            let conn = rusqlite::Connection::open(policy_path).unwrap();
            conn.execute_batch("BEGIN IMMEDIATE").unwrap();
            locked.send(()).unwrap();
            std::thread::sleep(Duration::from_millis(100));
            changed_on_lock.store(true, Ordering::SeqCst);
            conn.execute_batch("COMMIT").unwrap();
        });
        let checks = AtomicUsize::new(0);
        let check = || {
            if checks.fetch_add(1, Ordering::SeqCst) == 1 {
                start.send(()).unwrap();
                lock_ready.lock().unwrap().recv().unwrap();
            }
            if changed.load(Ordering::SeqCst) {
                Err(AppError::api("auth_required", 401))
            } else {
                Ok(())
            }
        };
        let error = network
            .fetch_guarded(
                upload_request(format!("{origin}/upload"), &path, Duration::from_secs(1)),
                None,
                &check,
            )
            .await
            .err()
            .unwrap();
        writer.join().unwrap();
        assert_eq!(error.public_code, "auth_required");
        assert!(
            tokio::time::timeout(Duration::from_millis(30), listener.accept())
                .await
                .is_err()
        );
        drop(network);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn early_upload_rate_limit_stops_source_and_preserves_both_cooldowns() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let root = transfer_root();
        let path = root.join("selected.bin");
        std::fs::File::create(&path)
            .unwrap()
            .set_len(256 * 1024 * 1024)
            .unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let origin = format!("http://{}", listener.local_addr().unwrap());
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut header = Vec::new();
            while !header.ends_with(b"\r\n\r\n") {
                header.push(socket.read_u8().await.unwrap());
                assert!(header.len() < 8192);
            }
            socket.write_all(b"HTTP/1.1 429 Too Many Requests\r\nRetry-After: 60\r\nContent-Length: 0\r\n\r\n").await.unwrap();
            let until = tokio::time::Instant::now() + Duration::from_millis(200);
            let mut bytes = [0; UPLOAD_CHUNK_BYTES];
            loop {
                match tokio::time::timeout_at(until, socket.read(&mut bytes)).await {
                    Ok(Ok(0)) | Ok(Err(_)) | Err(_) => break,
                    Ok(Ok(_)) => {}
                }
            }
        });
        let policy = Arc::new(Policy::with_limits(&root.join("policy.db"), 0, 100).unwrap());
        let network = Network::new(policy.clone(), true);
        let mut request = upload_request(format!("{origin}/upload"), &path, Duration::from_secs(3));
        request.account_id = "selected-account".into();
        let error = network.fetch(request).await.err().unwrap();
        assert_eq!(error.public_code, "action_outcome_unknown");
        let probe = UPLOAD_PROBES
            .lock()
            .unwrap()
            .iter()
            .rev()
            .find(|(source, _)| source == path.to_str().unwrap())
            .unwrap()
            .1
            .clone();
        let produced_at_return = probe.produced.load(Ordering::SeqCst);
        assert!(probe.cancelled.load(Ordering::SeqCst));
        for scope in [format!("{origin}|*"), format!("{origin}|selected-account")] {
            assert_eq!(
                policy.acquire(&scope).await.unwrap_err().public_code,
                "rate_limited"
            );
        }
        server.await.unwrap();
        assert_eq!(probe.produced.load(Ordering::SeqCst), produced_at_return);
        drop(network);
        drop(policy);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn completed_file_preserves_eof_after_owning_attempt_cancellation() {
        let root = transfer_root();
        let path = root.join("selected.bin");
        std::fs::write(&path, b"complete").unwrap();
        let mut source = source(&path).await;
        let guard = UploadGuard(source.completion.clone());
        assert_eq!(source.next_chunk().await.unwrap().unwrap(), b"complete");
        assert!(source.completion.complete());
        drop(guard);
        assert!(source.next_chunk().await.unwrap().is_none());
        assert!(source.completion.complete());
        drop(source);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn late_upload_response_failures_preserve_observed_rate_limit_cooldowns() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        for stalled in [false, true] {
            let root = transfer_root();
            let path = root.join("selected.bin");
            std::fs::write(&path, b"complete").unwrap();
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let origin = format!("http://{}", listener.local_addr().unwrap());
            let server = tokio::spawn(async move {
                let (mut socket, _) = listener.accept().await.unwrap();
                let mut header = Vec::new();
                while !header.ends_with(b"\r\n\r\n") {
                    header.push(socket.read_u8().await.unwrap());
                    assert!(header.len() < 8192);
                }
                let length: usize = String::from_utf8(header)
                    .unwrap()
                    .lines()
                    .find_map(|line| {
                        let (key, value) = line.split_once(':')?;
                        key.eq_ignore_ascii_case("content-length")
                            .then(|| value.trim().parse().unwrap())
                    })
                    .unwrap();
                assert!(length < 4096);
                let mut body = [0; 4096];
                socket.read_exact(&mut body[..length]).await.unwrap();
                socket.write_all(b"HTTP/1.1 429 Too Many Requests\r\nRetry-After: 60\r\nContent-Length: 100\r\n\r\nx").await.unwrap();
                if stalled {
                    tokio::time::sleep(Duration::from_secs(1)).await;
                }
                socket.shutdown().await.unwrap();
            });
            let policy = Arc::new(Policy::with_limits(&root.join("policy.db"), 0, 100).unwrap());
            let network = Network::new(policy.clone(), true);
            let mut request = upload_request(
                format!("{origin}/upload"),
                &path,
                Duration::from_millis(100),
            );
            request.account_id = "selected-account".into();
            let error = network.fetch(request).await.err().unwrap();
            assert_eq!(error.public_code, "action_outcome_unknown");
            for scope in [format!("{origin}|*"), format!("{origin}|selected-account")] {
                assert_eq!(
                    policy.acquire(&scope).await.unwrap_err().public_code,
                    "rate_limited"
                );
            }
            server.abort();
            let _ = server.await;
            drop(network);
            drop(policy);
            std::fs::remove_dir_all(root).unwrap();
        }
    }

    #[tokio::test]
    #[ignore = "real-time transfer crosses the 45/90 second deadlines and 120 second lease; run explicitly"]
    async fn active_file_transfer_renews_leases_across_old_total_deadlines() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let root = transfer_root();
        let path = root.join("selected.bin");
        std::fs::File::create(&path)
            .unwrap()
            .set_len(32 * 1024 * 1024)
            .unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let origin = format!("http://{}", listener.local_addr().unwrap());
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut header = Vec::new();
            while !header.ends_with(b"\r\n\r\n") {
                header.push(socket.read_u8().await.unwrap());
                assert!(header.len() < 8192);
            }
            let length: usize = String::from_utf8(header)
                .unwrap()
                .lines()
                .find_map(|line| {
                    let (key, value) = line.split_once(':')?;
                    key.eq_ignore_ascii_case("content-length")
                        .then(|| value.trim().parse().unwrap())
                })
                .unwrap();
            let mut remaining = length;
            let mut chunk = [0; 256 * 1024];
            while remaining > 0 {
                let capacity = remaining.min(chunk.len());
                socket.read_exact(&mut chunk[..capacity]).await.unwrap();
                remaining -= capacity;
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
            socket
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 13\r\nConnection: close\r\n\r\n")
                .await
                .unwrap();
            // Receiving also remains live beyond the old lease boundary.
            for _ in 0..13 {
                socket.write_all(b"x").await.unwrap();
                tokio::time::sleep(Duration::from_secs(10)).await;
            }
        });
        let network = Network::new(
            Arc::new(Policy::with_limits(&root.join("policy.db"), 0, 100).unwrap()),
            true,
        );
        let before = Instant::now();
        let response = network
            .fetch(upload_request(
                format!("{origin}/upload"),
                &path,
                Duration::from_secs(45),
            ))
            .await
            .unwrap();
        assert!(before.elapsed() > Duration::from_secs(240));
        assert_eq!(response.status, 200);
        assert_eq!(response.bytes, b"xxxxxxxxxxxxx");
        server.abort();
        let _ = server.await;
        drop(network);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn fifo_is_rejected_without_waiting_for_a_writer() {
        use std::os::unix::ffi::OsStrExt;
        let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join(".cache/upload-file-tests")
            .join(util::id());
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("owned-fixture.fifo");
        let c_path = std::ffi::CString::new(path.as_os_str().as_bytes()).unwrap();
        assert_eq!(unsafe { libc::mkfifo(c_path.as_ptr(), 0o600) }, 0);
        // A rescue writer bounds runtime shutdown even if nonblocking open regresses.
        let rescue_path = path.clone();
        let (cancel, canceled) = std::sync::mpsc::channel();
        let rescue_used = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let used = rescue_used.clone();
        let rescue = std::thread::spawn(move || {
            if canceled.recv_timeout(Duration::from_secs(2)).is_ok() {
                return;
            }
            used.store(true, std::sync::atomic::Ordering::SeqCst);
            use std::os::unix::fs::OpenOptionsExt;
            let _ = std::fs::OpenOptions::new()
                .read(true)
                .write(true)
                .custom_flags(libc::O_NONBLOCK)
                .open(rescue_path);
        });
        let result = tokio::time::timeout(
            Duration::from_secs(3),
            UploadFile::open(
                path.to_str().unwrap(),
                Arc::new(TransferProgress::new()),
                Arc::new(UploadCompletion::default()),
            ),
        )
        .await;
        let _ = cancel.send(());
        rescue.join().unwrap();
        assert!(!rescue_used.load(std::sync::atomic::Ordering::SeqCst));
        assert!(result.unwrap().is_err());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn read_only_file_multipart_is_rejected_before_file_resolution() {
        let mut request = Request {
            url: "https://fixture.example.test/search".into(),
            method: "POST".into(),
            headers: vec![],
            body: None,
            account_id: String::new(),
            limit: 1024,
            timeout: Duration::from_secs(1),
            read_only: true,
            follow_redirects: false,
            tls_profile: String::new(),
            multipart: Some(vec![crate::spec::MultipartPart::File {
                name: "file".into(),
                path: "/synthetic/never-opened".into(),
            }]),
        };
        assert_eq!(
            request.check_read_headers().unwrap_err().public_code,
            "approval_required"
        );
        request.read_only = false;
        assert!(request.check_read_headers().is_ok());
    }
    #[test]
    fn read_headers_guard_is_pure_and_preserves_non_read_requests() {
        let mut request = Request {
            url: "https://fixture.example.test/items".into(),
            method: "GET".into(),
            headers: vec![("X-HTTP-Method-Override".into(), "synthetic-selector".into())],
            body: None,
            account_id: String::new(),
            limit: 1024,
            timeout: Duration::from_secs(1),
            read_only: true,
            follow_redirects: false,
            tls_profile: String::new(),
            multipart: None,
        };
        let error = request.check_read_headers().unwrap_err();
        assert_eq!(error.public_code, "approval_required");
        request.read_only = false;
        assert!(request.check_read_headers().is_ok());
        request.read_only = true;
        request.headers = vec![("Accept".into(), "application/json".into())];
        assert!(request.check_read_headers().is_ok());
    }
    #[tokio::test]
    async fn private_query_timeout_does_not_expose_wire_url() {
        let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join(".cache/pagination-tests")
            .join(util::id());
        std::fs::create_dir_all(&root).unwrap();
        let policy = Arc::new(Policy::open(&root.join("policy.db")).unwrap());
        let network = Network::new(policy, true);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}/items", listener.local_addr().unwrap());
        let server = tokio::spawn(async move {
            loop {
                let (stream, _) = listener.accept().await.unwrap();
                tokio::spawn(async move {
                    tokio::time::sleep(Duration::from_secs(10)).await;
                    drop(stream);
                });
            }
        });
        let result = network
            .fetch_paginated(
                Request {
                    url,
                    method: "GET".into(),
                    headers: vec![],
                    body: None,
                    account_id: String::new(),
                    limit: 1024,
                    timeout: Duration::from_millis(30),
                    read_only: true,
                    follow_redirects: true,
                    tls_profile: String::new(),
                    multipart: None,
                },
                Some(("pageToken", "private-timeout-cursor")),
            )
            .await;
        let error = result.err().expect("slow response must time out");
        assert_eq!(error.public_code, "timeout");
        assert!(!format!("{error:?}").contains("private-timeout-cursor"));
        for entry in std::fs::read_dir(root).unwrap() {
            let path = entry.unwrap().path();
            if path.is_file() {
                assert!(
                    !String::from_utf8_lossy(&std::fs::read(path).unwrap())
                        .contains("private-timeout-cursor")
                );
            }
        }
        server.abort();
    }
    #[test]
    fn website_input_normalizes_domains_without_weakening_request_validation() {
        assert_eq!(
            website_url("  blog.example.com  ").unwrap().as_str(),
            "https://blog.example.com/"
        );
        assert_eq!(
            website_url("//example.org/path").unwrap().as_str(),
            "https://example.org/path"
        );
        assert!(validate_url("example.org").is_err());
        for value in [
            "",
            "file:///etc/passwd",
            "https://user:password@example.org",
            "javascript:alert(1)",
        ] {
            assert!(website_url(value).is_err(), "{value}");
        }
    }
}

#[cfg(test)]
mod file_receiver_tests {
    use super::*;
    use crate::artifacts::{ArtifactStore, Provenance, TRANSFER_CHUNK_BYTES};
    use axum::{Router, body::Body, http::Response as HttpResponse, routing::get};

    fn request(url: String) -> Request {
        Request {
            url,
            method: "GET".into(),
            headers: vec![],
            body: None,
            account_id: String::new(),
            limit: 8 * 1024 * 1024,
            timeout: Duration::from_secs(3),
            read_only: true,
            follow_redirects: true,
            tls_profile: String::new(),
            multipart: None,
        }
    }
    fn empty(root: &std::path::Path) -> bool {
        let path = root.join("artifacts");
        !path.exists() || std::fs::read_dir(path).unwrap().next().is_none()
    }
    async fn fixture() -> (String, tokio::task::JoinHandle<()>) {
        let app = Router::new()
            .route("/known", get(|| async { large(true) }))
            .route("/unknown", get(|| async { large(false) }))
            .route(
                "/idle",
                get(|| async {
                    HttpResponse::builder()
                        .header("content-type", "application/octet-stream")
                        .body(Body::from_stream(futures_util::stream::unfold(
                            0,
                            |n| async move {
                                if n > 0 {
                                    tokio::time::sleep(Duration::from_secs(60)).await;
                                }
                                Some((Ok::<_, std::io::Error>(vec![1; 1024]), n + 1))
                            },
                        )))
                        .unwrap()
                }),
            )
            .route(
                "/diagnostic",
                get(|| async {
                    HttpResponse::builder()
                        .status(401)
                        .body(Body::from_stream(futures_util::stream::iter((0..130).map(
                            |_| Ok::<_, std::io::Error>(vec![0; TRANSFER_CHUNK_BYTES]),
                        ))))
                        .unwrap()
                }),
            )
            .route(
                "/partial",
                get(|| async {
                    HttpResponse::builder()
                        .status(206)
                        .header("content-range", "bytes 0-2/4")
                        .body(Body::from("abc"))
                        .unwrap()
                }),
            );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let origin = format!("http://{}", listener.local_addr().unwrap());
        (
            origin,
            tokio::spawn(async move {
                axum::serve(listener, app).await.unwrap();
            }),
        )
    }
    fn large(known: bool) -> HttpResponse<Body> {
        let mut response =
            HttpResponse::builder().header("content-type", "application/octet-stream");
        if known {
            response = response.header(
                "content-length",
                (1601u64 * TRANSFER_CHUNK_BYTES as u64).to_string(),
            );
        }
        response
            .body(Body::from_stream(futures_util::stream::iter(
                (0..1601).map(|_| Ok::<_, std::io::Error>(vec![7; TRANSFER_CHUNK_BYTES])),
            )))
            .unwrap()
    }
    #[tokio::test]
    async fn file_receiver_streams_known_and_unknown_lengths_above_old_ceilings() {
        let (origin, server) = fixture().await;
        let root = std::env::temp_dir().join(format!("hycli-net-file-{}", util::id()));
        util::private_dir(&root).unwrap();
        let network = Network::new(
            Arc::new(Policy::with_limits(&root.join("policy.db"), 0, 100).unwrap()),
            true,
        );
        for path in ["known", "unknown"] {
            let received = network
                .fetch_file_guarded(
                    request(format!("{origin}/{path}")),
                    &|| Ok(()),
                    &FileReceiver::new(&root),
                )
                .await
                .unwrap();
            assert!(
                received.response.bytes.is_empty(),
                "file bytes are not a diagnostic buffer"
            );
            let file = received.artifact.unwrap();
            assert_eq!(file.byte_count(), 1601 * TRANSFER_CHUNK_BYTES as u64);
            let published = file
                .publish(
                    "large.bin",
                    "application/octet-stream",
                    Provenance {
                        site: "fixture".into(),
                        action: "download".into(),
                        source_origin: origin.clone(),
                    },
                )
                .unwrap();
            let opened = ArtifactStore::open(&root)
                .unwrap()
                .open_completed(&published.manifest().artifact_id)
                .unwrap();
            let stream = opened.into_stream();
            futures_util::pin_mut!(stream);
            let mut count = 0u64;
            while let Some(chunk) = stream.next().await {
                let chunk = chunk.unwrap();
                assert!(chunk.len() <= TRANSFER_CHUNK_BYTES);
                assert!(chunk.iter().all(|byte| *byte == 7));
                count += chunk.len() as u64;
            }
            assert_eq!(count, 1601 * TRANSFER_CHUNK_BYTES as u64);
            drop(published);
            assert!(empty(&root));
        }
        server.abort();
        drop(network);
        std::fs::remove_dir_all(root).unwrap();
    }
    #[tokio::test]
    async fn file_receiver_idle_cancellation_partial_and_diagnostic_failure_clean_owned_stage() {
        let (origin, server) = fixture().await;
        let root = std::env::temp_dir().join(format!("hycli-net-file-{}", util::id()));
        util::private_dir(&root).unwrap();
        let network = Arc::new(Network::new(
            Arc::new(Policy::with_limits(&root.join("policy.db"), 0, 100).unwrap()),
            true,
        ));
        for (path, code) in [
            ("partial", "response_mismatch"),
            ("diagnostic", "too_large"),
            ("idle", "timeout"),
        ] {
            let mut input = request(format!("{origin}/{path}"));
            input.timeout = Duration::from_millis(100);
            assert_eq!(
                network
                    .fetch_file_guarded(input, &|| Ok(()), &FileReceiver::new(&root))
                    .await
                    .err()
                    .unwrap()
                    .public_code,
                code
            );
            assert!(empty(&root));
        }
        let core = network.clone();
        let path = root.clone();
        let url = format!("{origin}/idle");
        let transfer = tokio::spawn(async move {
            core.fetch_file_guarded(request(url), &|| Ok(()), &FileReceiver::new(&path))
                .await
        });
        tokio::time::timeout(Duration::from_secs(5), async {
            while empty(&root) {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .unwrap();
        transfer.abort();
        let _ = transfer.await;
        tokio::time::timeout(Duration::from_secs(5), async {
            while !empty(&root) {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .unwrap();
        server.abort();
        drop(network);
        std::fs::remove_dir_all(root).unwrap();
    }
    #[tokio::test]
    async fn file_receiver_redirect_and_service_retry_use_fresh_attempt_state() {
        let root = std::env::temp_dir().join(format!("hycli-net-file-{}", util::id()));
        util::private_dir(&root).unwrap();
        let attempts = Arc::new(AtomicUsize::new(0));
        let count = attempts.clone();
        let blocked_attempts = Arc::new(AtomicUsize::new(0));
        let blocked_count = blocked_attempts.clone();
        let app = Router::new()
            .route(
                "/redirect",
                get(|| async {
                    HttpResponse::builder()
                        .status(302)
                        .header("location", "/final")
                        .body(Body::from("redirect diagnostic, not file bytes"))
                        .unwrap()
                }),
            )
            .route(
                "/retry",
                get(move || {
                    let count = count.clone();
                    async move {
                        if count.fetch_add(1, Ordering::SeqCst) == 0 {
                            HttpResponse::builder()
                                .status(503)
                                .header("content-type", "application/json")
                                .body(Body::from(r#"{"error":"temporarily unavailable"}"#))
                                .unwrap()
                        } else {
                            HttpResponse::builder()
                                .header("content-type", "application/octet-stream")
                                .body(Body::from("final"))
                                .unwrap()
                        }
                    }
                }),
            )
            .route(
                "/blocked",
                get(move || {
                    let count = blocked_count.clone();
                    async move {
                        count.fetch_add(1, Ordering::SeqCst);
                        HttpResponse::builder()
                            .status(503)
                            .body(Body::from("unclassified rejection"))
                            .unwrap()
                    }
                }),
            )
            .route(
                "/final",
                get(|| async {
                    HttpResponse::builder()
                        .header("content-type", "application/octet-stream")
                        .body(Body::from("final"))
                        .unwrap()
                }),
            );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let origin = format!("http://{}", listener.local_addr().unwrap());
        let server = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        let network = Network::new(
            Arc::new(Policy::with_limits(&root.join("policy.db"), 0, 100).unwrap()),
            true,
        );
        for path in ["redirect", "retry"] {
            let received = network
                .fetch_file_guarded(
                    request(format!("{origin}/{path}")),
                    &|| Ok(()),
                    &FileReceiver::new(&root),
                )
                .await
                .unwrap();
            assert_eq!(received.response.status, 200);
            assert!(received.response.bytes.is_empty());
            let sealed = received.artifact.unwrap();
            assert_eq!(sealed.byte_count(), 5);
            assert_eq!(sealed.prefix(), b"final");
            assert_eq!(
                std::fs::read_dir(root.join("artifacts")).unwrap().count(),
                1
            );
            drop(sealed);
            assert!(empty(&root));
        }
        assert_eq!(attempts.load(Ordering::SeqCst), 2);
        let blocked = network
            .fetch_file_guarded(
                request(format!("{origin}/blocked")),
                &|| Ok(()),
                &FileReceiver::new(&root),
            )
            .await
            .unwrap();
        assert_eq!(blocked.response.status, 503);
        assert_eq!(blocked.response.defense, "unknown");
        assert_eq!(blocked.response.bytes, b"unclassified rejection");
        assert!(blocked.artifact.is_none());
        assert_eq!(blocked_attempts.load(Ordering::SeqCst), 1);
        assert!(empty(&root));
        server.abort();
        drop(network);
        std::fs::remove_dir_all(root).unwrap();
    }
}
