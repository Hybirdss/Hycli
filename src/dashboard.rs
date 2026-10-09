//! Loopback-only companion API. Credential values have no read/export endpoint.
use crate::{
    accounts, ai,
    apperr::{AppError, AppResult},
    policy::{self, Effect},
    runtime::Runtime,
    state::{self, ProviderSettings, Settings},
    util,
};
use axum::{
    Json, Router,
    extract::{DefaultBodyLimit, Path, Query, Request, State},
    http::{HeaderMap, StatusCode, header},
    middleware::{self, Next},
    response::{IntoResponse, Response, Sse, sse::Event},
    routing::{get, patch, post, put},
};
use futures_util::StreamExt;
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    convert::Infallible,
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio_stream::wrappers::BroadcastStream;
#[derive(Clone)]
pub struct Web {
    pub core: Arc<Runtime>,
    pub port: u16,
    pub session: String,
    pub csrf: String,
    pub instance_id: String,
    pub shutdown: tokio::sync::watch::Sender<bool>,
    pub pairs: Arc<Mutex<BTreeMap<String, Pair>>>,
}
#[derive(Clone)]
pub struct Pair {
    pub site_id: String,
    pub url: String,
    pub expires: i64,
    pub extension_origin: String,
    pub token: String,
}
#[derive(rust_embed::RustEmbed)]
#[folder = "web/"]
struct Assets;
#[derive(Deserialize, Default)]
struct Prepare {
    #[serde(default)]
    url: String,
    #[serde(default)]
    site_id: String,
    #[serde(default)]
    provider: String,
    #[serde(default)]
    intent: String,
}
#[derive(Deserialize, Default)]
struct ProviderBody {
    #[serde(default)]
    key: Option<String>,
    #[serde(default)]
    model: String,
    #[serde(default)]
    default: bool,
}
#[derive(Deserialize, Default)]
struct Import {
    content: String,
    #[serde(default)]
    site_id: String,
    #[serde(default)]
    url: String,
}
#[derive(Deserialize, Default)]
struct ActionBody {
    #[serde(default)]
    inputs: BTreeMap<String, String>,
    #[serde(default)]
    account_id: Option<String>,
}
fn language(headers: &HeaderMap) -> &str {
    state::locale_or_default(
        headers
            .get("accept-language")
            .and_then(|s| s.to_str().ok())
            .unwrap_or("en"),
    )
}
fn ok() -> Json<Value> {
    Json(json!({"ok":true}))
}
fn token() -> String {
    format!("{}{}", util::id(), util::id()).replace('-', "")
}
impl Web {
    pub fn new(core: Arc<Runtime>, port: u16) -> Self {
        Self {
            core,
            port,
            session: token(),
            csrf: token(),
            instance_id: util::id(),
            shutdown: tokio::sync::watch::channel(false).0,
            pairs: Arc::new(Mutex::new(BTreeMap::new())),
        }
    }
}
pub async fn serve(core: Arc<Runtime>, port: u16, open_browser: bool) -> AppResult<()> {
    serve_with_options(core, port, open_browser, false).await
}
pub async fn serve_with_options(
    core: Arc<Runtime>,
    port: u16,
    open_browser: bool,
    auto_port: bool,
) -> AppResult<()> {
    let listener = match tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, port)).await
    {
        Ok(listener) => listener,
        Err(e) if auto_port && e.kind() == std::io::ErrorKind::AddrInUse => {
            tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)).await?
        }
        Err(e) => return Err(e.into()),
    };
    let port = listener.local_addr()?.port();
    let images = core.clone();
    tokio::spawn(async move {
        images.fill_missing_site_images().await;
    });
    let web = Web::new(core.clone(), port);
    let _registration = crate::desktop::Registration::publish(
        &core.root,
        &crate::desktop::Instance {
            port,
            id: web.instance_id.clone(),
        },
    )?;
    let url = format!("http://127.0.0.1:{port}");
    eprintln!("Hycli dashboard: {url}");
    if open_browser {
        if let Err(e) = open_url(&url) {
            eprintln!("{e}. Open {url} in your browser.");
        }
    }
    let shutdown = web.shutdown.clone();
    let mut shutdown_rx = shutdown.subscribe();
    let mut done_rx = shutdown.subscribe();
    let server = axum::serve(listener, router(web)).with_graceful_shutdown(async move {
        tokio::select! {
            _ = shutdown_rx.wait_for(|value| *value) => {},
            _ = shutdown_signal() => { shutdown.send_replace(true); },
        }
    });
    // SSE streams close on the signal; bound any remaining keepalive connections.
    tokio::select! {
        result = std::future::IntoFuture::into_future(server) => result.map_err(Into::into),
        _ = async { let _ = done_rx.wait_for(|value| *value).await; tokio::time::sleep(Duration::from_secs(5)).await; } => Ok(()),
    }
}
async fn shutdown_signal() {
    #[cfg(unix)]
    {
        if let Ok(mut term) =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        {
            tokio::select! { _ = tokio::signal::ctrl_c() => {}, _ = term.recv() => {} }
            return;
        }
    }
    let _ = tokio::signal::ctrl_c().await;
}
async fn shutdown(State(web): State<Web>) -> AppResult<Json<Value>> {
    if web
        .core
        .state
        .read()?
        .jobs
        .iter()
        .any(|job| ["queued", "running"].contains(&job.status.as_str()))
    {
        return Err(AppError::api("work_in_progress", 409));
    }
    web.shutdown.send_replace(true);
    Ok(ok())
}
pub fn router(web: Web) -> Router {
    Router::new()
        .route("/api/session", get(session))
        .route("/api/shutdown", post(shutdown))
        .route("/api/state", get(snapshot))
        .route("/api/events", get(events))
        .route("/api/activity/export", get(export_activity))
        .route("/api/activity", axum::routing::delete(clear_activity))
        .route("/api/settings", patch(settings))
        .route(
            "/api/providers/{id}",
            put(save_provider).delete(delete_provider),
        )
        .route("/api/providers/{id}/check", post(check_provider))
        .route("/api/providers/codex/login", post(codex_login))
        .route("/api/providers/codex/account", get(codex_account))
        .route("/api/sites/prepare", post(prepare))
        .route(
            "/api/sites/{id}/credentials/{name}",
            put(save_website_credential).delete(delete_website_credential),
        )
        .route("/api/sites/import", post(import_spec))
        .route("/api/sites/{id}", patch(edit_site).delete(delete_site))
        .route("/api/sites/{id}/export", get(export_spec))
        .route("/api/sites/{id}/image", get(site_image))
        .route("/api/sites/{id}/describe", post(describe))
        .route("/api/sites/{id}/actions/{action}", post(action))
        .route("/api/approvals/{id}/approve", post(approve))
        .route("/api/approvals/{id}/reject", post(reject))
        .route("/api/jobs/{id}", get(job_details))
        .route("/api/jobs/{id}/cancel", post(cancel))
        .route("/api/jobs/{id}/retry", post(retry))
        .route("/api/jobs/{id}/result", get(result))
        .route("/api/jobs/{id}/summarize", post(summarize))
        .route("/api/accounts/pair", post(pair))
        .route("/api/accounts/import", post(import_account))
        .route("/api/accounts/{id}/refresh", post(refresh_account))
        .route("/api/accounts/{id}", axum::routing::delete(delete_account))
        .route("/api/browser/extension", get(extension))
        .route("/api/browser/discovery", get(browser_discovery))
        .route("/api/browser/find", post(browser_find))
        .route("/api/browser/login", post(browser_login))
        .route("/api/browser/select", post(browser_select))
        .route("/api/handoff", get(handoff))
        .route(
            "/bridge/observe",
            post(crate::companion::observe).options(crate::companion::options),
        )
        .route(
            "/bridge/pair",
            post(crate::companion::pair).options(crate::companion::options),
        )
        .route(
            "/bridge/refresh",
            post(crate::companion::refresh).options(crate::companion::options),
        )
        .route(
            "/bridge/connect",
            post(crate::companion::connect).options(crate::companion::options),
        )
        .route("/guide/browser", get(guide))
        .fallback(get(asset))
        .layer(DefaultBodyLimit::max(2 * 1024 * 1024 + 32768))
        .layer(middleware::from_fn_with_state(web.clone(), guard))
        .with_state(web)
}
async fn guard(State(web): State<Web>, request: Request, next: Next) -> Response {
    let fail = || AppError::api("forbidden", 403).into_response();
    let host = request
        .headers()
        .get(header::HOST)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    let expected = format!("127.0.0.1:{}", web.port);
    let localhost = format!("localhost:{}", web.port);
    if host != expected && host != localhost {
        return fail();
    }
    let path = request.uri().path();
    let bridge = path.starts_with("/bridge/");
    if !bridge {
        if let Some(origin) = request
            .headers()
            .get(header::ORIGIN)
            .and_then(|v| v.to_str().ok())
        {
            if origin != format!("http://{host}") {
                return fail();
            }
        }
        if request
            .headers()
            .get("sec-fetch-site")
            .and_then(|v| v.to_str().ok())
            .is_some_and(|s| s != "same-origin" && s != "none")
        {
            return fail();
        }
        if path.starts_with("/api/") && path != "/api/session" {
            let cookie = request
                .headers()
                .get(header::COOKIE)
                .and_then(|v| v.to_str().ok())
                .unwrap_or("");
            let received = cookie
                .split(';')
                .find_map(|p| p.trim().strip_prefix("hycli_session="))
                .unwrap_or("");
            if !util::constant_eq(received, &web.session) {
                return AppError::api("session_expired", 401).into_response();
            }
            if request.method() != axum::http::Method::GET
                && request.method() != axum::http::Method::HEAD
            {
                let csrf = request
                    .headers()
                    .get("x-hycli-csrf")
                    .and_then(|v| v.to_str().ok())
                    .unwrap_or("");
                if !util::constant_eq(csrf, &web.csrf) {
                    return AppError::api("session_expired", 401).into_response();
                }
            }
        }
    }
    if *web.shutdown.borrow() {
        return AppError::api("shutting_down", 503).into_response();
    }
    let mut response = next.run(request).await;
    let headers = response.headers_mut();
    headers.insert(header::CACHE_CONTROL, "no-store".parse().unwrap());
    headers.insert("x-content-type-options", "nosniff".parse().unwrap());
    headers.insert("referrer-policy", "no-referrer".parse().unwrap());
    if !headers.contains_key("content-security-policy") {
        headers.insert("content-security-policy","default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data:; connect-src 'self'; font-src 'self'; frame-ancestors 'none'; base-uri 'none'; form-action 'self'".parse().unwrap());
    }
    response
}
async fn session(State(web): State<Web>) -> Response {
    (
        [(
            header::SET_COOKIE,
            format!(
                "hycli_session={}; Path=/; HttpOnly; SameSite=Strict",
                web.session
            ),
        )],
        Json(json!({"csrf":web.csrf,"instance_id":web.instance_id})),
    )
        .into_response()
}
async fn snapshot(State(web): State<Web>, headers: HeaderMap) -> AppResult<Json<Value>> {
    let d = web.core.state.read()?;
    let storage = web
        .core
        .vault
        .lock()
        .map_err(|_| AppError::api("internal", 500))?
        .protection()
        .to_owned();
    Ok(Json(
        json!({"settings":d.settings,"providers":web.core.providers()?,"sites":web.core.sites(language(&headers))?,"accounts":d.accounts.values().collect::<Vec<_>>(),"jobs":d.jobs,"approvals":web.core.policy.pending()?,"engine":{"available":true,"compatible":true,"name":"Hycli Rust","version":env!("CARGO_PKG_VERSION"),"path":std::env::current_exe().ok()},"version":env!("CARGO_PKG_VERSION"),"data_location":web.core.root,"credential_storage":storage,"browser_connected":d.accounts.values().any(|a|a.browser!="File import")}),
    ))
}
async fn events(
    State(web): State<Web>,
) -> Sse<impl futures_util::Stream<Item = Result<Event, Infallible>>> {
    let mut shutdown = web.shutdown.subscribe();
    let stream = BroadcastStream::new(web.core.events.subscribe())
        .map(|_| Ok(Event::default().data("update")))
        .take_until(async move {
            let _ = shutdown.wait_for(|value| *value).await;
        });
    Sse::new(stream)
        .keep_alive(axum::response::sse::KeepAlive::new().interval(Duration::from_secs(15)))
}
async fn settings(State(web): State<Web>, Json(patch): Json<Value>) -> AppResult<Json<Value>> {
    web.core.state.update(|d| {
        let mut v = serde_json::to_value(&d.settings)?;
        let obj = patch
            .as_object()
            .ok_or_else(|| AppError::api("bad_request", 400))?;
        for (k, x) in obj {
            if ![
                "locale",
                "theme",
                "reduce_motion",
                "notifications",
                "default_provider",
            ]
            .contains(&k.as_str())
            {
                return Err(AppError::api("bad_request", 400));
            }
            v[k] = x.clone();
        }
        let s: Settings = serde_json::from_value(v)?;
        if state::locale_or_default(&s.locale) != s.locale
            || !["light", "dark", "system"].contains(&s.theme.as_str())
            || (!s.default_provider.is_empty() && ai::provider(&s.default_provider).is_none())
        {
            return Err(AppError::api("bad_request", 400));
        }
        d.settings = s;
        Ok(())
    })?;
    web.core.changed();
    if patch.get("locale").is_some() {
        let data = web.core.state.read()?;
        if !data.settings.default_provider.is_empty() {
            for site in web.core.sites(&data.settings.locale)? {
                if site.translation_pending
                    && !data.jobs.iter().any(|j| {
                        j.kind == "describe"
                            && j.site_id == site.id
                            && j.locale == data.settings.locale
                            && ["queued", "running"].contains(&j.status.as_str())
                    })
                {
                    let _ = web.core.start_describe(
                        &site.id,
                        &data.settings.default_provider,
                        &data.settings.locale,
                    );
                }
            }
        }
    }
    Ok(ok())
}
async fn save_provider(
    State(web): State<Web>,
    Path(id): Path<String>,
    Json(body): Json<ProviderBody>,
) -> AppResult<Json<Value>> {
    let provider = ai::provider(&id).ok_or_else(|| AppError::api("provider_missing", 400))?;
    if body.model.len() > 160 {
        return Err(AppError::api("bad_request", 400));
    }
    let key = match body.key {
        Some(key) => key,
        None => {
            let mut vault = web
                .core
                .vault
                .lock()
                .map_err(|_| AppError::api("internal", 500))?;
            vault.reload()?;
            vault.get("providers", &id).unwrap_or_default().to_owned()
        }
    };
    if key.len() > 8192 {
        return Err(AppError::api("bad_request", 400));
    }
    if id == "codex" && web.core.ai.codex.refresh_account().await?.is_none() {
        return Err(AppError::api("auth_required", 401));
    }
    let config = ai::Config {
        provider: id.clone(),
        model: if body.model.is_empty() {
            provider.default_model.into()
        } else {
            body.model
        },
        key,
    };
    let models = web.core.ai.check(&config).await?;
    if id != "codex" {
        web.core
            .vault
            .lock()
            .map_err(|_| AppError::api("internal", 500))?
            .set("providers", &id, &config.key)?;
    }
    web.core.state.update(|d| {
        d.providers.insert(
            id.clone(),
            ProviderSettings {
                model: config.model,
                connected: true,
                models,
                checked_at: util::now(),
            },
        );
        if body.default || d.settings.default_provider.is_empty() {
            d.settings.default_provider = id;
        }
        Ok(())
    })?;
    web.core.changed();
    Ok(ok())
}
async fn check_provider(State(web): State<Web>, Path(id): Path<String>) -> AppResult<Json<Value>> {
    if id == "codex" {
        if web.core.ai.codex.refresh_account().await?.is_none() {
            web.core.state.update(|d| {
                if let Some(p) = d.providers.get_mut("codex") {
                    p.connected = false;
                }
                Ok(())
            })?;
            web.core.changed();
            return Err(AppError::api("auth_required", 401));
        }
        let models = web.core.ai.codex.models().await?;
        web.core.state.update(|d| {
            let p = d.providers.entry(id.clone()).or_default();
            p.models = models;
            p.connected = true;
            p.checked_at = util::now();
            if d.settings.default_provider.is_empty() {
                d.settings.default_provider = id.clone();
            }
            Ok(())
        })?;
    } else {
        let config = web.core.config(&id)?;
        let models = web.core.ai.check(&config).await?;
        web.core.state.update(|d| {
            if let Some(p) = d.providers.get_mut(&id) {
                p.models = models;
                p.checked_at = util::now();
            }
            Ok(())
        })?;
    }
    web.core.changed();
    Ok(ok())
}
async fn delete_provider(State(web): State<Web>, Path(id): Path<String>) -> AppResult<Json<Value>> {
    web.core.remove_provider(&id)?;
    Ok(ok())
}
async fn codex_login(State(web): State<Web>) -> AppResult<Json<Value>> {
    Ok(Json(web.core.ai.codex.login().await?))
}
async fn codex_account(State(web): State<Web>) -> AppResult<Json<Value>> {
    let account = web.core.ai.codex.refresh_account().await?;
    if account.is_some() {
        let models = web.core.ai.codex.models().await?;
        web.core.state.update(|d| {
            let p = d.providers.entry("codex".into()).or_default();
            p.connected = true;
            p.models = models;
            if d.settings.default_provider.is_empty() {
                d.settings.default_provider = "codex".into();
            }
            Ok(())
        })?;
        web.core.changed();
    } else {
        web.core.state.update(|d| {
            if let Some(p) = d.providers.get_mut("codex") {
                p.connected = false;
            }
            Ok(())
        })?;
        web.core.changed();
    }
    Ok(Json(
        json!({"connected":account.is_some(),"account":account}),
    ))
}
async fn prepare(
    State(web): State<Web>,
    headers: HeaderMap,
    Json(body): Json<Prepare>,
) -> AppResult<Json<Value>> {
    Ok(Json(
        json!({"job":web.core.start_prepare_with_intent(&body.url,&body.site_id,&body.provider,language(&headers),&body.intent)?}),
    ))
}
async fn import_spec(State(web): State<Web>, Json(body): Json<Import>) -> AppResult<Json<Value>> {
    let sp = crate::spec::parse(body.content.as_bytes())?;
    web.core.install(&sp)?;
    let d = web.core.state.read()?;
    if !d.settings.default_provider.is_empty() {
        let _ = web.core.start_describe(
            &sp.site.name,
            &d.settings.default_provider,
            &d.settings.locale,
        );
    }
    Ok(Json(json!({"site_id":sp.site.name})))
}
async fn edit_site(
    State(web): State<Web>,
    Path(id): Path<String>,
    Json(body): Json<Value>,
) -> AppResult<Json<Value>> {
    web.core.spec(&id)?;
    web.core.state.update(|d| {
        if let Some(account) = body.get("account_id").and_then(Value::as_str) {
            if !account.is_empty() && !d.accounts.get(account).is_some_and(|a| a.site_id == id) {
                return Err(AppError::api("bad_request", 400));
            }
        }
        let m = d.sites.entry(id).or_default();
        if let Some(title) = body.get("title").and_then(Value::as_str) {
            m.title = title.trim().chars().take(100).collect();
        }
        if let Some(pinned) = body.get("pinned").and_then(Value::as_bool) {
            m.pinned = pinned;
        }
        if let Some(account) = body.get("account_id").and_then(Value::as_str) {
            m.account_id = account.into();
        }
        m.updated_at = util::now();
        Ok(())
    })?;
    web.core.changed();
    Ok(ok())
}
async fn save_website_credential(
    State(web): State<Web>,
    Path((id, name)): Path<(String, String)>,
    Json(body): Json<ProviderBody>,
) -> AppResult<Json<Value>> {
    web.core.set_website_credential(
        &id,
        &name,
        Some(
            body.key
                .as_deref()
                .ok_or_else(|| AppError::api("bad_request", 400))?,
        ),
    )?;
    Ok(ok())
}
async fn delete_website_credential(
    State(web): State<Web>,
    Path((id, name)): Path<(String, String)>,
) -> AppResult<Json<Value>> {
    web.core.set_website_credential(&id, &name, None)?;
    Ok(ok())
}
async fn delete_site(State(web): State<Web>, Path(id): Path<String>) -> AppResult<Json<Value>> {
    web.core.remove_site(&id)?;
    Ok(ok())
}
async fn export_spec(State(web): State<Web>, Path(id): Path<String>) -> AppResult<Response> {
    let sp = web.core.spec(&id)?;
    let content = serde_yaml::to_string(&sp).map_err(|_| AppError::api("internal", 500))?;
    Ok((
        [(header::CONTENT_TYPE, "application/yaml; charset=utf-8")],
        content,
    )
        .into_response())
}
async fn site_image(State(web): State<Web>, Path(id): Path<String>) -> AppResult<Response> {
    let (kind, bytes) = web.core.site_image(&id)?;
    Ok((
        [
            (header::CONTENT_TYPE, kind.as_str()),
            (
                header::CONTENT_SECURITY_POLICY,
                "default-src 'none'; style-src 'unsafe-inline'; sandbox; frame-ancestors 'none'",
            ),
        ],
        bytes,
    )
        .into_response())
}
async fn describe(
    State(web): State<Web>,
    Path(id): Path<String>,
    headers: HeaderMap,
    Json(body): Json<Prepare>,
) -> AppResult<Json<Value>> {
    Ok(Json(
        json!({"job":web.core.start_describe(&id,&body.provider,language(&headers))?}),
    ))
}
async fn action(
    State(web): State<Web>,
    Path((id, action)): Path<(String, String)>,
    headers: HeaderMap,
    Json(body): Json<ActionBody>,
) -> AppResult<Json<Value>> {
    let sp = web.core.spec(&id)?;
    let req = web
        .core
        .request(&sp, &action, body.inputs, body.account_id.as_deref())?;
    if policy::operation_effect(sp.op(&action)?, &req.inputs) != Effect::Read {
        return Ok(Json(
            json!({"approval":web.core.approval(req,language(&headers))?}),
        ));
    }
    Ok(Json(
        json!({"job":web.core.start_action(req,None,language(&headers))?}),
    ))
}
async fn approve(
    State(web): State<Web>,
    Path(id): Path<String>,
    headers: HeaderMap,
) -> AppResult<Json<Value>> {
    let review = web.core.policy.review(&id)?;
    let request = review
        .request
        .ok_or_else(|| AppError::api("approval_expired", 409))?;
    web.core.policy.claim(&id)?;
    Ok(Json(
        json!({"job":web.core.start_action(request,Some(id),language(&headers))?}),
    ))
}
async fn reject(State(web): State<Web>, Path(id): Path<String>) -> AppResult<Json<Value>> {
    web.core.policy.reject(&id)?;
    web.core.changed();
    Ok(ok())
}
async fn cancel(State(web): State<Web>, Path(id): Path<String>) -> AppResult<Json<Value>> {
    web.core.cancel_job(&id)?;
    Ok(ok())
}
async fn retry(State(web): State<Web>, Path(id): Path<String>) -> AppResult<Json<Value>> {
    let job = web.core.state.job(&id)?;
    if job.status != "failed" {
        return Err(AppError::api("bad_request", 400));
    }
    let next = match job.kind.as_str() {
        "prepare" => web.core.start_prepare_with_intent(
            &job.url,
            if web
                .core
                .specs_dir
                .join(format!("{}.yaml", job.site_id))
                .exists()
            {
                &job.site_id
            } else {
                ""
            },
            &job.provider,
            &job.locale,
            &job.intent,
        )?,
        "describe" => web
            .core
            .start_describe(&job.site_id, &job.provider, &job.locale)?,
        "summary" => web.core.start_summary(&job.parent_id, &job.provider)?,
        _ => return Err(AppError::api("bad_request", 400)),
    };
    Ok(Json(json!({"job":next})))
}
async fn job_details(State(web): State<Web>, Path(id): Path<String>) -> AppResult<Json<Value>> {
    Ok(Json(json!(web.core.state.job(&id)?)))
}
async fn result(State(web): State<Web>, Path(id): Path<String>) -> AppResult<Json<Value>> {
    Ok(Json(web.core.state.job(&id)?.result.unwrap_or(Value::Null)))
}
async fn summarize(
    State(web): State<Web>,
    Path(id): Path<String>,
    Json(body): Json<Prepare>,
) -> AppResult<Json<Value>> {
    Ok(Json(
        json!({"job": web.core.start_summary(&id, &body.provider)?}),
    ))
}

async fn pair(State(web): State<Web>, Json(body): Json<Prepare>) -> AppResult<Json<Value>> {
    let url = crate::net::validate_url(&body.url)?;
    if !body.site_id.is_empty()
        && crate::runtime::origin(web.core.spec(&body.site_id)?.site.source_url())?
            != url.origin().ascii_serialization()
    {
        return Err(AppError::api("bad_url", 400));
    }
    let code = util::id().replace('-', "")[..10].to_ascii_uppercase();
    let expires = util::unix_ms() + 300000;
    let mut pairs = web
        .pairs
        .lock()
        .map_err(|_| AppError::api("internal", 500))?;
    pairs.retain(|_, p| p.expires > util::unix_ms());
    if pairs.len() >= 20 {
        return Err(AppError::api("rate_limited", 429));
    }
    pairs.insert(
        code.clone(),
        Pair {
            site_id: body.site_id,
            url: url.to_string(),
            expires,
            extension_origin: String::new(),
            token: token(),
        },
    );
    Ok(Json(
        json!({"code":format!("{} {}",&code[..5],&code[5..]),"expires_at":chrono::DateTime::from_timestamp_millis(expires).unwrap().to_rfc3339()}),
    ))
}
async fn import_account(
    State(web): State<Web>,
    Json(body): Json<Import>,
) -> AppResult<Json<Value>> {
    let cookies = accounts::parse_cookies(&body.content, &body.url)?;
    let account =
        web.core
            .connect_account(&body.site_id, &body.url, cookies, "File import", "", "")?;
    Ok(Json(json!({"account":account})))
}
async fn refresh_account(State(web): State<Web>, Path(id): Path<String>) -> AppResult<Json<Value>> {
    Ok(Json(
        json!({"account":web.core.refresh_account(&id).await?}),
    ))
}
async fn delete_account(State(web): State<Web>, Path(id): Path<String>) -> AppResult<Json<Value>> {
    web.core.remove_account(&id)?;
    Ok(ok())
}
async fn extension(Query(query): Query<BTreeMap<String, String>>) -> AppResult<Response> {
    Ok((
        [(header::CONTENT_TYPE, "application/zip")],
        crate::companion::archive(query.get("browser").is_some_and(|b| b == "firefox"))?,
    )
        .into_response())
}
async fn handoff(
    State(web): State<Web>,
    Query(query): Query<BTreeMap<String, String>>,
) -> AppResult<Json<Value>> {
    let exe = std::env::current_exe()?.to_string_lossy().into_owned();
    let mut args = vec!["mcp".to_string()];
    if web.core.network.allow_local {
        args.push("--allow-local".into());
    }
    if let Some(site) = query.get("site") {
        web.core.spec(site)?;
        args.extend(["--sites-only".into(), "--only-site".into(), site.clone()]);
    }
    let quote = |value: &str| {
        if cfg!(windows) {
            format!("'{}'", value.replace('\'', "''"))
        } else {
            format!("'{}'", value.replace('\'', "'\\''"))
        }
    };
    let invocation = std::iter::once(quote(&exe))
        .chain(args.iter().map(|arg| quote(arg)))
        .collect::<Vec<_>>()
        .join(" ");
    let command = if cfg!(windows) {
        format!(
            "$env:HYCLI_DATA_DIR = {}; & {invocation}",
            quote(&web.core.root.to_string_lossy())
        )
    } else {
        format!(
            "HYCLI_DATA_DIR={} {invocation}",
            quote(&web.core.root.to_string_lossy())
        )
    };
    let instructions = include_str!("../agent/AGENTS.md");
    Ok(Json(
        json!({"instructions":instructions,"config":{"mcpServers":{"hycli":{"command":exe,"args":args,"env":{"HYCLI_DATA_DIR":web.core.root}}}},"command":command}),
    ))
}
async fn guide() -> Response {
    (
        [(header::CONTENT_TYPE, "text/html; charset=utf-8")],
        include_str!("../browser-companion/guide.html"),
    )
        .into_response()
}
async fn asset(request: Request) -> Response {
    let path = request.uri().path().trim_start_matches('/');
    let path = if path.is_empty() { "index.html" } else { path };
    if path.starts_with("api/") || path.starts_with("bridge/") {
        return StatusCode::NOT_FOUND.into_response();
    }
    let asset = Assets::get(path).or_else(|| {
        if !path.contains('.') {
            Assets::get("index.html")
        } else {
            None
        }
    });
    match asset {
        Some(file) => (
            [(
                header::CONTENT_TYPE,
                mime_guess::from_path(path)
                    .first_or_octet_stream()
                    .to_string(),
            )],
            file.data.into_owned(),
        )
            .into_response(),
        None => StatusCode::NOT_FOUND.into_response(),
    }
}
pub fn open_url(url: &str) -> std::io::Result<()> {
    let mut command = if cfg!(target_os = "macos") {
        std::process::Command::new("open")
    } else if cfg!(windows) {
        let mut c = std::process::Command::new("rundll32.exe");
        c.arg("url.dll,FileProtocolHandler");
        c
    } else {
        std::process::Command::new("xdg-open")
    };
    command
        .arg(url)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .stdin(std::process::Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    command.spawn()?;
    Ok(())
}

async fn export_activity(State(web): State<Web>) -> AppResult<Json<Value>> {
    Ok(Json(
        json!({"version":1,"exported_at":util::now(),"jobs":web.core.state.read()?.jobs}),
    ))
}
async fn clear_activity(State(web): State<Web>) -> AppResult<Json<Value>> {
    web.core.state.update(|d| {
        d.jobs
            .retain(|j| ["queued", "running"].contains(&j.status.as_str()));
        Ok(())
    })?;
    web.core.changed();
    Ok(ok())
}

async fn browser_discovery(State(web): State<Web>) -> AppResult<Json<Value>> {
    let profiles = tokio::task::spawn_blocking(crate::browser_discovery::discover)
        .await
        .map_err(|_| AppError::api("internal", 500))?;
    let accounts: Vec<_> = web.core.state.read()?.accounts.values().cloned().collect();
    Ok(Json(
        json!({"profiles":profiles,"accounts":accounts,"method":"native_then_companion","credentials_exposed":false}),
    ))
}

#[derive(Deserialize)]
struct BrowserFind {
    url: String,
    #[serde(default)]
    site_id: String,
    #[serde(default)]
    profile_id: String,
}
async fn browser_find(
    State(web): State<Web>,
    Json(body): Json<BrowserFind>,
) -> AppResult<Json<Value>> {
    Ok(Json(serde_json::to_value(
        web.core
            .find_browser_accounts(&body.site_id, &body.url, &body.profile_id)
            .await?,
    )?))
}
async fn browser_select(State(web): State<Web>, Json(body): Json<Value>) -> AppResult<Json<Value>> {
    let id = body.get("account_id").and_then(Value::as_str).unwrap_or("");
    let target = crate::runtime::origin(body.get("url").and_then(Value::as_str).unwrap_or(""))?;
    web.core.state.update(|state| {
        let account = state
            .accounts
            .get(id)
            .ok_or_else(|| AppError::api("not_found", 404))?;
        if account.site_url != target || account.status != "connected" || !account.verified_identity
        {
            return Err(AppError::api("auth_required", 401));
        }
        state
            .sites
            .entry(account.site_id.clone())
            .or_default()
            .account_id = id.into();
        Ok(())
    })?;
    web.core.changed();
    Ok(ok())
}
async fn browser_login(
    State(web): State<Web>,
    Json(body): Json<BrowserFind>,
) -> AppResult<Json<Value>> {
    let target = crate::net::validate_url(&body.url)?;
    if !policy::safe_read_url(&target) {
        return Err(AppError::api("bad_url", 400));
    }
    if !body.site_id.is_empty()
        && crate::runtime::origin(web.core.spec(&body.site_id)?.site.source_url())?
            != target.origin().ascii_serialization()
    {
        return Err(AppError::api("bad_url", 400));
    }
    let mut login_url = crate::browser_discovery::login_url(&target);
    {
        if let Ok(page) = web.core.read_page(&login_url, "").await {
            if page.successful() {
                if let Some(found) = crate::browser_discovery::published_login(
                    &target,
                    &String::from_utf8_lossy(&page.bytes),
                ) {
                    login_url = found;
                }
            }
        }
    }
    open_url(&login_url).map_err(|_| AppError::api("browser_unavailable", 422))?;
    Ok(Json(
        json!({"opened":true,"login_url":login_url,"verified":false}),
    ))
}
