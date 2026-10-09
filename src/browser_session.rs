//! Restore one site's session in a private browser and observe its own read requests.
use crate::{
    accounts::{self, Credential, Identity, Observation},
    apperr::{AppError, AppResult},
    util,
};
use chromiumoxide::{
    Browser, Page,
    browser::BrowserConfig,
    cdp::browser_protocol::{fetch, network},
};
use futures_util::StreamExt;
use serde_json::Value;
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
    time::Duration,
};

#[derive(Default)]
pub struct Capture {
    pub identity: Option<Identity>,
    pub identity_url: String,
    pub headers: BTreeMap<String, String>,
    pub observations: Vec<Observation>,
    pub html: String,
    pub final_url: String,
    pub status: u16,
    pub authenticated_requests: usize,
}
fn unavailable<T>(_: T) -> AppError {
    AppError::api("browser_unavailable", 422)
}

pub fn navigation_safe(url: &url::Url) -> bool {
    crate::policy::safe_read_url(url)
        || (url.query().is_none()
            && matches!(
                url.path().trim_end_matches('/').rsplit('/').next(),
                Some("login" | "signin" | "log-in" | "sign-in")
            ))
}

pub async fn capture(
    credential: &Credential,
    entry: &str,
    allow_local: bool,
) -> AppResult<Capture> {
    let environment = tokio::task::spawn_blocking(crate::environment::inspect)
        .await
        .map_err(unavailable)?;
    let engine = environment
        .browsers
        .iter()
        .find(|b| b.protocol == "chromium-cdp")
        .ok_or_else(|| AppError::api("browser_unavailable", 422))?;
    capture_with_engine(credential, entry, allow_local, engine).await
}

pub async fn capture_with_engine(
    credential: &Credential,
    entry: &str,
    allow_local: bool,
    engine: &crate::environment::BrowserEngine,
) -> AppResult<Capture> {
    let url = crate::net::validate_url(entry)?;
    if url.origin().ascii_serialization() != credential.origin || !navigation_safe(&url) {
        return Err(AppError::api("unsafe_target", 400));
    }
    crate::net::resolve(&url, allow_local).await?;
    let snapshot = crate::browser_storage::Snapshot::new().map_err(unavailable)?;
    if engine.protocol != "chromium-cdp" {
        return Err(AppError::api("browser_unavailable", 422));
    }
    let builder = BrowserConfig::builder()
        .chrome_executable(&engine.executable)
        .user_data_dir(&snapshot.0)
        .arg("--no-first-run")
        .arg("--disable-background-networking")
        .arg("--disable-dev-shm-usage");
    let config = builder.build().map_err(unavailable)?;
    let (mut browser, mut handler) =
        tokio::time::timeout(Duration::from_secs(35), Browser::launch(config))
            .await
            .map_err(unavailable)?
            .map_err(unavailable)?;
    let drive = tokio::spawn(async move { while handler.next().await.is_some() {} });
    let result = tokio::time::timeout(
        Duration::from_secs(35),
        observe(&browser, credential, entry, allow_local),
    )
    .await
    .map_err(unavailable)
    .and_then(|r| r);
    let _ = browser.close().await;
    drive.abort();
    result
}

async fn observe(
    browser: &Browser,
    credential: &Credential,
    entry: &str,
    allow_local: bool,
) -> AppResult<Capture> {
    let page = browser.new_page("about:blank").await.map_err(unavailable)?;
    for cookie in &credential.cookies {
        // Bind to the exact host even when the source cookie included sibling subdomains.
        let mut value = network::CookieParam::new(&cookie.name, &cookie.value);
        value.url = Some(format!("{}/", credential.origin));
        value.path = Some(cookie.path.clone());
        value.secure = Some(cookie.secure);
        value.http_only = Some(cookie.http_only);
        value.same_site = match cookie.same_site.as_str() {
            "strict" => Some(network::CookieSameSite::Strict),
            "lax" => Some(network::CookieSameSite::Lax),
            "no_restriction" => Some(network::CookieSameSite::None),
            _ => None,
        };
        page.set_cookie(value).await.map_err(unavailable)?;
    }
    let origin = serde_json::to_string(&credential.origin)?;
    let storage = serde_json::to_string(&credential.local_storage)?;
    page.evaluate_on_new_document(format!("if(location.origin==={origin}){{for(const [key,value] of Object.entries({storage})){{localStorage.setItem(key,value)}}}}")).await.map_err(unavailable)?;
    page.execute(network::EnableParams::default())
        .await
        .map_err(unavailable)?;
    let mut paused = page
        .event_listener::<fetch::EventRequestPaused>()
        .await
        .map_err(unavailable)?;
    let mut responses = page
        .event_listener::<network::EventResponseReceived>()
        .await
        .map_err(unavailable)?;
    let mut finished = page
        .event_listener::<network::EventLoadingFinished>()
        .await
        .map_err(unavailable)?;
    page.execute(fetch::EnableParams::default())
        .await
        .map_err(unavailable)?;
    let result = Arc::new(Mutex::new(Capture::default()));
    let captured = result.clone();
    let task_page = page.clone();
    let target_origin = credential.origin.clone();
    let listener = tokio::spawn(async move {
        let mut requests = std::collections::HashMap::new();
        let mut identities = std::collections::HashMap::new();
        let mut checked_origins = BTreeMap::new();
        loop {
            tokio::select! {
                Some(event) = paused.next() => {
                    let parsed = crate::net::validate_url(&event.request.url).ok();
                    let mut permitted = parsed.as_ref().is_some_and(|url| crate::policy::safe_read_url(url) || (event.resource_type == network::ResourceType::Document && url.origin().ascii_serialization() == target_origin && navigation_safe(url)))
                        && ["GET", "HEAD", "OPTIONS"].contains(&event.request.method.as_str());
                    if let Some(url) = &parsed {
                        let origin = url.origin().ascii_serialization();
                        let public = if let Some(public) = checked_origins.get(&origin) { *public } else {
                            let public = crate::net::resolve(url, allow_local).await.is_ok();
                            checked_origins.insert(origin, public);
                            public
                        };
                        permitted &= public;
                    }
                    if !permitted {
                        let _ = task_page.execute(fetch::FailRequestParams::new(event.request_id.clone(), network::ErrorReason::BlockedByClient)).await;
                        continue;
                    }
                    let same = parsed.as_ref().is_some_and(|u| u.origin().ascii_serialization() == target_origin);
                    let mut command = fetch::ContinueRequestParams::new(event.request_id.clone());
                    let values = event.request.headers.inner().as_object();
                    if same && event.request.method == "GET" {
                        if let Some(id) = &event.network_id {
                            let headers: BTreeMap<String, String> = values.into_iter().flatten()
                                .filter(|(k, _)| accounts::browser_header(k))
                                .filter_map(|(k,v)| v.as_str().map(|v| (k.to_ascii_lowercase(), v.into())))
                                .collect();
                            if !headers.is_empty() {
                                if let Ok(mut capture) = captured.lock() {
                                    capture.authenticated_requests += 1;
                                    if capture.headers.is_empty() { capture.headers = headers.clone(); }
                                }
                            }
                            requests.insert(id.clone(), (parsed.unwrap(), headers));
                        }
                    } else if !same {
                        // Cross-origin resource requests must never carry the restored session.
                        command.headers = Some(values.into_iter().flatten()
                            .filter(|(k, _)| !accounts::browser_header(k) && !k.eq_ignore_ascii_case("cookie"))
                            .filter_map(|(k,v)| v.as_str().map(|v| fetch::HeaderEntry::new(k, v))).collect());
                    }
                    let _ = task_page.execute(command).await;
                }
                Some(event) = responses.next() => {
                    if let Some((url, headers)) = requests.remove(&event.request_id) {
                        let status = event.response.status as u16;
                        if event.r#type == network::ResourceType::Document {
                            if let Ok(mut capture) = captured.lock() { capture.status = status; }
                        }
                        if (200..300).contains(&status) && matches!(event.r#type, network::ResourceType::Xhr | network::ResourceType::Fetch) {
                            if let Ok(mut capture) = captured.lock() {
                                if capture.observations.len() < 80 {
                                    capture.observations.push(Observation { method: "GET".into(), path: url.path().into(), query: url.query_pairs().map(|(k, _)| (k.into_owned(), "string".into())).collect(), body: BTreeMap::new(), status, observed_at: util::now() });
                                }
                            }
                            if accounts::identity_endpoint(&url) { identities.insert(event.request_id.clone(), (url, headers)); }
                        }
                    }
                }
                Some(event) = finished.next() => {
                    if let Some((url, headers)) = identities.remove(&event.request_id) {
                        if let Ok(value) = identity_body(&task_page, event.request_id.clone()).await {
                            if let Some(identity) = accounts::extract_identity(&value) {
                                if let Ok(mut capture) = captured.lock() {
                                    capture.identity = Some(identity);
                                    capture.identity_url = url.to_string();
                                    capture.headers = headers;
                                }
                            }
                        }
                    }
                }
                else => break,
            }
        }
    });
    // Navigation only: never click a control or submit a form to discover a session.
    let _ = tokio::time::timeout(Duration::from_secs(18), page.goto(entry)).await;
    for _ in 0..24 {
        if result.lock().is_ok_and(|c| c.identity.is_some()) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
    listener.abort();
    let _ = listener.await;
    let html = page.content().await.unwrap_or_default();
    let final_url = page
        .url()
        .await
        .ok()
        .flatten()
        .unwrap_or_else(|| entry.into());
    let _ = page.close().await;
    let mut result = result.lock().map_err(unavailable)?;
    result.html = html;
    result.final_url = final_url;
    Ok(std::mem::take(&mut *result))
}

async fn identity_body(page: &Page, id: network::RequestId) -> AppResult<Value> {
    let body = page
        .execute(network::GetResponseBodyParams::new(id))
        .await
        .map_err(unavailable)?
        .result;
    if body.body.len() > 1024 * 1024 {
        return Err(AppError::api("too_large", 413));
    }
    let bytes = if body.base64_encoded {
        use base64::Engine;
        base64::engine::general_purpose::STANDARD
            .decode(body.body)
            .map_err(unavailable)?
    } else {
        body.body.into_bytes()
    };
    serde_json::from_slice(&bytes).map_err(|_| AppError::api("data_invalid", 400))
}
