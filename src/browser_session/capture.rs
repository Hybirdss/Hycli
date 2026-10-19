use crate::{
    accounts::{self, Credential, Identity, Observation as AccountObservation},
    apperr::{AppError, AppResult},
    util,
};
use chromiumoxide::{
    Browser, Page,
    cdp::browser_protocol::{fetch, network},
};
use futures_util::StreamExt;
use serde_json::Value;
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
    time::Duration,
};

use super::{
    config::{
        STEALTH_INIT_SCRIPT, build_stealth_browser_config, can_run_headed, cookie_param_for,
        unavailable,
    },
    profile_lock::{acquire_profile_dir, discover_live_cdp_endpoint},
};

#[derive(Default)]
pub struct Capture {
    pub identity: Option<Identity>,
    pub identity_url: String,
    pub headers: BTreeMap<String, String>,
    pub observations: Vec<AccountObservation>,
    pub html: String,
    pub final_url: String,
    pub status: u16,
    pub authenticated_requests: usize,
}

fn capture_needs_headed_escalation(capture: &Capture) -> bool {
    let (cls, _) = crate::defend::classify(&crate::defend::Observation {
        status_code: capture.status,
        headers: Vec::new(),
        body_snippet: capture.html.chars().take(4096).collect(),
    });
    if cls.is_challenge() {
        return true;
    }
    if matches!(capture.status, 403 | 429 | 503) {
        let lower = capture.html.to_ascii_lowercase();
        if lower.contains("access denied")
            || lower.contains("edgesuite")
            || lower.contains("akamai")
            || lower.contains("just a moment")
            || lower.contains("cf-chl")
        {
            return true;
        }
    }
    false
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
    if let Some(cdp_url) = discover_live_cdp_endpoint(credential) {
        if let Ok(Ok((browser, mut handler))) =
            tokio::time::timeout(Duration::from_secs(3), Browser::connect(cdp_url)).await
        {
            let url = crate::net::validate_url(entry)?;
            let cred_origin = crate::net::validate_url(&credential.origin)?;
            if !crate::net::same_site_origin(&url, &cred_origin) || !navigation_safe(&url) {
                return Err(AppError::api("unsafe_target", 400));
            }
            crate::net::resolve(&url, allow_local).await?;
            let drive = tokio::spawn(async move { while handler.next().await.is_some() {} });
            let result = tokio::time::timeout(
                Duration::from_secs(35),
                observe(&browser, credential, entry, allow_local),
            )
            .await
            .map_err(unavailable)
            .and_then(|r| r);
            drive.abort();
            if result.is_ok() {
                return result;
            }
        }
    }
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

async fn run_capture_with_mode(
    credential: &Credential,
    entry: &str,
    allow_local: bool,
    engine: &crate::environment::BrowserEngine,
    headed: bool,
) -> AppResult<Capture> {
    let (profile_dir, _profile_lock, _snapshot) =
        acquire_profile_dir(&credential.origin).map_err(unavailable)?;
    let config =
        build_stealth_browser_config(engine, Some(&profile_dir), &credential.origin, headed)
            .map_err(unavailable)?;
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

pub async fn capture_with_engine(
    credential: &Credential,
    entry: &str,
    allow_local: bool,
    engine: &crate::environment::BrowserEngine,
) -> AppResult<Capture> {
    let url = crate::net::validate_url(entry)?;
    let cred_origin = crate::net::validate_url(&credential.origin)?;
    if !crate::net::same_site_origin(&url, &cred_origin) || !navigation_safe(&url) {
        return Err(AppError::api("unsafe_target", 400));
    }
    crate::net::resolve(&url, allow_local).await?;
    if engine.protocol != "chromium-cdp" {
        return Err(AppError::api("browser_unavailable", 422));
    }
    let force_headed = std::env::var_os("HYCLI_HEADED").is_some();
    let first = run_capture_with_mode(credential, entry, allow_local, engine, force_headed).await?;
    if !force_headed && capture_needs_headed_escalation(&first) && can_run_headed() {
        if let Ok(escalated) =
            run_capture_with_mode(credential, entry, allow_local, engine, true).await
        {
            if !capture_needs_headed_escalation(&escalated)
                || escalated.observations.len() > first.observations.len()
            {
                return Ok(escalated);
            }
        }
    }
    Ok(first)
}

fn is_safe_graphql_read_post(url: &url::Url, post_data: Option<&str>) -> bool {
    let lower = url.path().to_ascii_lowercase();
    if !lower.contains("graphql") && !lower.ends_with("/gql") {
        return false;
    }
    let Some(raw) = post_data else {
        return false;
    };
    let Ok(val) = serde_json::from_str::<Value>(raw) else {
        return false;
    };
    let Some(query) = val.get("query").and_then(Value::as_str) else {
        return false;
    };
    use graphql_parser::query::{Definition, OperationDefinition};
    let Ok(doc) = graphql_parser::parse_query::<&str>(query) else {
        return false;
    };
    let mut has_query = false;
    for def in &doc.definitions {
        if let Definition::Operation(op) = def {
            match op {
                OperationDefinition::Mutation(_) | OperationDefinition::Subscription(_) => {
                    return false;
                }
                OperationDefinition::Query(_) | OperationDefinition::SelectionSet(_) => {
                    has_query = true;
                }
            }
        }
    }
    has_query
}

fn extract_post_body_schema(post_data: Option<&str>) -> BTreeMap<String, String> {
    let mut schema = BTreeMap::new();
    let Some(raw) = post_data else {
        return schema;
    };
    if let Ok(Value::Object(map)) = serde_json::from_str::<Value>(raw) {
        for (k, v) in map.into_iter().take(40) {
            if !util::secret_field(&k) {
                let kind = match v {
                    Value::Number(_) => "number",
                    Value::Bool(_) => "bool",
                    Value::Array(_) => "array",
                    Value::Object(_) => "object",
                    _ => "string",
                };
                schema.insert(k, kind.into());
            }
        }
    } else {
        for (k, _) in url::form_urlencoded::parse(raw.as_bytes()).take(40) {
            if !util::secret_field(&k) {
                schema.insert(k.into_owned(), "string".into());
            }
        }
    }
    schema
}

async fn observe(
    browser: &Browser,
    credential: &Credential,
    entry: &str,
    allow_local: bool,
) -> AppResult<Capture> {
    let page = browser.new_page("about:blank").await.map_err(unavailable)?;
    for cookie in &credential.cookies {
        let value = cookie_param_for(cookie, &credential.origin);
        page.set_cookie(value).await.map_err(unavailable)?;
    }
    let origin = serde_json::to_string(&credential.origin)?;
    let storage = serde_json::to_string(&credential.local_storage)?;
    page.evaluate_on_new_document(format!(
        "{STEALTH_INIT_SCRIPT}\nif(location.origin==={origin}){{for(const [key,value] of Object.entries({storage})){{try{{localStorage.setItem(key,value);sessionStorage.setItem(key,value)}}catch(_){{}}}}}}"
    ))
    .await
    .map_err(unavailable)?;
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
    let target_url = crate::net::validate_url(&target_origin)?;
    let listener_target_url = target_url.clone();
    let listener = tokio::spawn(async move {
        let mut requests = std::collections::HashMap::new();
        let mut identities = std::collections::HashMap::new();
        let mut obs_bodies: std::collections::HashMap<network::RequestId, (String, String)> =
            std::collections::HashMap::new();
        let mut checked_origins = BTreeMap::new();
        loop {
            tokio::select! {
                Some(event) = paused.next() => {
                    let parsed = crate::net::validate_url(&event.request.url).ok();
                    let same = parsed.as_ref().is_some_and(|u| crate::net::same_site_origin(u, &listener_target_url));
                    let method = event.request.method.to_ascii_uppercase();
                    let post_data = event
                        .request
                        .post_data_entries
                        .as_ref()
                        .and_then(|entries| entries.first())
                        .and_then(|entry| entry.bytes.as_ref())
                        .and_then(|b| {
                            let slice: &[u8] = b.as_ref();
                            String::from_utf8(slice.to_vec()).ok()
                        });
                    let is_safe_method = ["GET", "HEAD", "OPTIONS"].contains(&method.as_str())
                        || (method == "POST"
                            && parsed
                                .as_ref()
                                .is_some_and(|u| is_safe_graphql_read_post(u, post_data.as_deref())));
                    let mut permitted = parsed.as_ref().is_some_and(|url| {
                        crate::policy::safe_read_url(url)
                            || (event.resource_type == network::ResourceType::Document
                                && same
                                && navigation_safe(url))
                    }) && is_safe_method;
                    if let Some(url) = &parsed {
                        let origin = url.origin().ascii_serialization();
                        let public = if let Some(public) = checked_origins.get(&origin) { *public } else {
                            let public = crate::net::resolve(url, allow_local).await.is_ok();
                            checked_origins.insert(origin, public);
                            public
                        };
                        permitted &= public;
                    }
                    let values = event.request.headers.inner().as_object();
                    let auth_header_names: Vec<String> = values
                        .into_iter()
                        .flatten()
                        .filter(|(k, _)| accounts::browser_header(k))
                        .map(|(k, _)| k.to_ascii_lowercase())
                        .collect();
                    if !permitted {
                        // Intercept-and-Abort: record write/mutation schema before blocking network send.
                        if same
                            && ["POST", "PUT", "PATCH", "DELETE"].contains(&method.as_str())
                        {
                            if let Some(url) = &parsed {
                                if let Ok(mut capture) = captured.lock() {
                                    if capture.observations.len() < 80 {
                                        capture.observations.push(AccountObservation {
                                            method,
                                            path: url.path().into(),
                                            query: url
                                                .query_pairs()
                                                .filter(|(k, _)| !util::secret_field(k))
                                                .map(|(k, _)| (k.into_owned(), "string".into()))
                                                .collect(),
                                            body: extract_post_body_schema(post_data.as_deref()),
                                            status: 0,
                                            observed_at: util::now(),
                                            response_pointers: Vec::new(),
                                            auth_headers: auth_header_names,
                                        });
                                    }
                                }
                            }
                        }
                        let _ = task_page.execute(fetch::FailRequestParams::new(event.request_id.clone(), network::ErrorReason::BlockedByClient)).await;
                        continue;
                    }
                    let mut command = fetch::ContinueRequestParams::new(event.request_id.clone());
                    let values = event.request.headers.inner().as_object();
                    if same && (method == "GET" || method == "POST") {
                        if let Some(id) = &event.network_id {
                            let headers: BTreeMap<String, String> = values.into_iter().flatten()
                                .filter(|(k, _)| accounts::browser_header(k))
                                .filter_map(|(k,v)| v.as_str().map(|v| (k.to_ascii_lowercase(), v.into())))
                                .collect();
                            if !headers.is_empty() {
                                if let Ok(mut capture) = captured.lock() {
                                    capture.authenticated_requests += 1;
                                    for (k, v) in &headers {
                                        capture.headers.entry(k.clone()).or_insert_with(|| v.clone());
                                    }
                                }
                            }
                            let body_schema = extract_post_body_schema(post_data.as_deref());
                            requests.insert(id.clone(), (parsed.unwrap(), headers, method, body_schema));
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
                    if let Some((url, headers, req_method, body_schema)) = requests.remove(&event.request_id) {
                        let status = event.response.status as u16;
                        if event.r#type == network::ResourceType::Document {
                            if let Ok(mut capture) = captured.lock() { capture.status = status; }
                        }
                        if (200..300).contains(&status) && matches!(event.r#type, network::ResourceType::Xhr | network::ResourceType::Fetch) {
                            let path_str: String = url.path().into();
                            let auth_headers: Vec<String> = headers.keys().cloned().collect();
                            if let Ok(mut capture) = captured.lock() {
                                if capture.observations.len() < 80 {
                                    capture.observations.push(AccountObservation {
                                        method: req_method.clone(),
                                        path: path_str.clone(),
                                        query: url.query_pairs().filter(|(k, _)| !util::secret_field(k)).map(|(k, _)| (k.into_owned(), "string".into())).collect(),
                                        body: body_schema,
                                        status,
                                        observed_at: util::now(),
                                        response_pointers: Vec::new(),
                                        auth_headers,
                                    });
                                }
                            }
                            if obs_bodies.len() < 32 {
                                obs_bodies.insert(event.request_id.clone(), (req_method, path_str));
                            }
                            if accounts::identity_endpoint(&url) { identities.insert(event.request_id.clone(), (url, headers)); }
                        }
                    }
                }
                Some(event) = finished.next() => {
                    let is_id = identities.remove(&event.request_id);
                    let obs_target = obs_bodies.remove(&event.request_id);
                    if is_id.is_some() || obs_target.is_some() {
                        if let Ok(value) = identity_body(&task_page, event.request_id.clone()).await {
                            if let Some((url, headers)) = is_id {
                                if let Some(identity) = accounts::extract_identity(&value) {
                                    if let Ok(mut capture) = captured.lock() {
                                        capture.identity = Some(identity);
                                        capture.identity_url = url.to_string();
                                        for (k, v) in headers {
                                            capture.headers.insert(k, v);
                                        }
                                    }
                                }
                            }
                            if let Some((req_method, path_str)) = obs_target {
                                let pointers = extract_json_pointers(&value);
                                if !pointers.is_empty() {
                                    if let Ok(mut capture) = captured.lock() {
                                        if let Some(obs) = capture
                                            .observations
                                            .iter_mut()
                                            .rev()
                                            .find(|o| o.method == req_method && o.path == path_str)
                                        {
                                            obs.response_pointers = pointers;
                                        }
                                    }
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
    let anonymous = credential.cookies.is_empty() && credential.local_storage.is_empty();
    for tick in 0..24 {
        if tick == 2 {
            // Trigger lazy-loaded list items and scroll-activated XHR/fetch endpoints.
            let _ = page
                .evaluate("try { window.scrollBy(0, 2000); } catch (_) {}")
                .await;
        }
        if result.lock().is_ok_and(|c| c.identity.is_some()) {
            break;
        }
        if anonymous && tick >= 3 && result.lock().is_ok_and(|c| (200..400).contains(&c.status)) {
            if let Ok(dom) = page.content().await {
                let lower = dom.to_ascii_lowercase();
                if dom.len() > 256
                    && !lower.contains("just a moment")
                    && !lower.contains("cf-chl")
                    && !lower.contains("challenge-platform")
                {
                    break;
                }
            }
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
    listener.abort();
    let _ = listener.await;
    if let Ok(eval) = page
        .evaluate("() => Array.isArray(window.__hycli_apis) ? window.__hycli_apis : []")
        .await
    {
        if let Ok(Value::Array(items)) = eval.into_value::<Value>() {
            if let Ok(mut capture) = result.lock() {
                for item in items.into_iter().take(40) {
                    let Some(raw_url) = item.get("url").and_then(Value::as_str) else {
                        continue;
                    };
                    let Ok(parsed) = target_url.join(raw_url) else {
                        continue;
                    };
                    if !crate::net::same_site_origin(&parsed, &target_url) {
                        continue;
                    }
                    let method = item
                        .get("method")
                        .and_then(Value::as_str)
                        .unwrap_or("GET")
                        .to_ascii_uppercase();
                    let status = item.get("status").and_then(Value::as_u64).unwrap_or(200) as u16;
                    let path_str = parsed.path().to_string();
                    let pointers = item
                        .get("data")
                        .map(extract_json_pointers)
                        .unwrap_or_default();
                    if capture.identity.is_none() && accounts::identity_endpoint(&parsed) {
                        if let Some(data) = item.get("data") {
                            if let Some(identity) = accounts::extract_identity(data) {
                                capture.identity = Some(identity);
                                capture.identity_url = parsed.to_string();
                            }
                        }
                    }
                    if let Some(existing) = capture
                        .observations
                        .iter_mut()
                        .find(|o| o.method == method && o.path == path_str)
                    {
                        if existing.response_pointers.is_empty() && !pointers.is_empty() {
                            existing.response_pointers = pointers;
                        }
                    } else if capture.observations.len() < 80 {
                        let auth_headers: Vec<String> = capture.headers.keys().cloned().collect();
                        capture.observations.push(AccountObservation {
                            method,
                            path: path_str,
                            query: parsed
                                .query_pairs()
                                .filter(|(k, _)| !util::secret_field(k))
                                .map(|(k, _)| (k.into_owned(), "string".into()))
                                .collect(),
                            body: extract_post_body_schema(
                                item.get("body").and_then(Value::as_str),
                            ),
                            status,
                            observed_at: util::now(),
                            response_pointers: pointers,
                            auth_headers,
                        });
                    }
                }
            }
        }
    }
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

fn extract_json_pointers(value: &Value) -> Vec<String> {
    let mut pointers = Vec::new();
    match value {
        Value::Object(map) => {
            for (k, v) in map.iter().take(6) {
                if !util::secret_field(k) && !k.contains('/') && !k.contains('~') {
                    pointers.push(format!("/{k}"));
                    if let Value::Array(arr) = v {
                        if let Some(Value::Object(first)) = arr.first() {
                            for sub_k in first.keys().take(2) {
                                if !util::secret_field(sub_k)
                                    && !sub_k.contains('/')
                                    && !sub_k.contains('~')
                                {
                                    pointers.push(format!("/{k}/0/{sub_k}"));
                                }
                            }
                        }
                    }
                }
            }
        }
        Value::Array(arr) => {
            if let Some(Value::Object(first)) = arr.first() {
                for k in first.keys().take(4) {
                    if !util::secret_field(k) && !k.contains('/') && !k.contains('~') {
                        pointers.push(format!("/0/{k}"));
                    }
                }
            }
        }
        _ => {}
    }
    pointers.truncate(6);
    pointers
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
