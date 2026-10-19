//! Loopback-only companion API. Credential values have no read/export endpoint.
mod auth;
mod routes_accounts;
mod routes_jobs;
mod routes_sites;

use crate::{
    ai,
    apperr::{AppError, AppResult},
    runtime::Runtime,
    state::{self, Settings},
    util,
};
use axum::{
    Json, Router,
    extract::{DefaultBodyLimit, Query, Request, State},
    http::{HeaderMap, StatusCode, header},
    middleware,
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
    /// Shown only in the address `hycli dashboard` opens; a plain local request cannot
    /// mint a dashboard session, so a tool-calling agent cannot approve its own change.
    pub launch: String,
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
pub(super) struct Prepare {
    #[serde(default)]
    pub(super) url: String,
    #[serde(default)]
    pub(super) site_id: String,
    #[serde(default)]
    pub(super) provider: String,
    #[serde(default)]
    pub(super) intent: String,
}

#[derive(Deserialize, Default)]
pub(super) struct ProviderBody {
    #[serde(default)]
    pub(super) key: Option<String>,
    #[serde(default)]
    pub(super) model: String,
    #[serde(default)]
    pub(super) default: bool,
}

#[derive(Deserialize, Default)]
pub(super) struct Import {
    pub(super) content: String,
    #[serde(default)]
    pub(super) site_id: String,
    #[serde(default)]
    pub(super) url: String,
}

#[derive(Deserialize, Default)]
pub(super) struct ActionBody {
    #[serde(default)]
    pub(super) inputs: BTreeMap<String, String>,
    #[serde(default)]
    pub(super) account_id: Option<String>,
}

pub(super) fn language(headers: &HeaderMap) -> &str {
    state::locale_or_default(
        headers
            .get("accept-language")
            .and_then(|s| s.to_str().ok())
            .unwrap_or("en"),
    )
}

pub(super) fn ok() -> Json<Value> {
    Json(json!({"ok":true}))
}

pub(super) fn token() -> String {
    format!("{}{}", util::id(), util::id()).replace('-', "")
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
        Err(e)
            if auto_port
                && matches!(
                    e.kind(),
                    std::io::ErrorKind::AddrInUse | std::io::ErrorKind::PermissionDenied
                ) =>
        {
            tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)).await?
        }
        Err(e) => return Err(e.into()),
    };
    let port = listener.local_addr()?.port();
    let images = core.clone();
    tokio::spawn(async move {
        images.fill_missing_site_images().await;
    });
    let web = Web::new(core.clone(), port)?;
    let _registration = crate::desktop::Registration::publish(
        &core.root,
        &crate::desktop::Instance {
            port,
            id: web.instance_id.clone(),
        },
    )?;
    let url = web.launch_url();
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
        .route("/api/session", get(auth::session))
        .route("/api/shutdown", post(shutdown))
        .route("/api/state", get(snapshot))
        .route("/api/events", get(events))
        .route("/api/activity/export", get(routes_jobs::export_activity))
        .route(
            "/api/activity",
            axum::routing::delete(routes_jobs::clear_activity),
        )
        .route("/api/settings", patch(settings))
        .route(
            "/api/providers/{id}",
            put(routes_accounts::save_provider).delete(routes_accounts::delete_provider),
        )
        .route(
            "/api/providers/{id}/check",
            post(routes_accounts::check_provider),
        )
        .route(
            "/api/providers/codex/login",
            post(routes_accounts::codex_login),
        )
        .route(
            "/api/providers/codex/account",
            get(routes_accounts::codex_account),
        )
        .route("/api/sites/prepare", post(routes_sites::prepare))
        .route(
            "/api/sites/{id}/credentials/{name}",
            put(routes_sites::save_website_credential)
                .delete(routes_sites::delete_website_credential),
        )
        .route("/api/sites/import", post(routes_sites::import_spec))
        .route(
            "/api/sites/{id}",
            patch(routes_sites::edit_site).delete(routes_sites::delete_site),
        )
        .route("/api/sites/{id}/export", get(routes_sites::export_spec))
        .route("/api/sites/{id}/image", get(routes_sites::site_image))
        .route("/api/sites/{id}/describe", post(routes_sites::describe))
        .route(
            "/api/sites/{id}/actions/{action}",
            post(routes_sites::action),
        )
        .route("/api/approvals/{id}/approve", post(routes_jobs::approve))
        .route("/api/approvals/{id}/reject", post(routes_jobs::reject))
        .route("/api/jobs/{id}", get(routes_jobs::job_details))
        .route("/api/jobs/{id}/cancel", post(routes_jobs::cancel))
        .route("/api/jobs/{id}/retry", post(routes_jobs::retry))
        .route("/api/jobs/{id}/result", get(routes_jobs::result))
        .route("/api/jobs/{id}/summarize", post(routes_jobs::summarize))
        .route("/api/accounts/pair", post(routes_accounts::pair))
        .route(
            "/api/accounts/import",
            post(routes_accounts::import_account),
        )
        .route(
            "/api/accounts/{id}/refresh",
            post(routes_accounts::refresh_account),
        )
        .route(
            "/api/accounts/{id}",
            axum::routing::delete(routes_accounts::delete_account),
        )
        .route("/api/browser/extension", get(routes_accounts::extension))
        .route(
            "/api/browser/discovery",
            get(routes_accounts::browser_discovery),
        )
        .route("/api/browser/find", post(routes_accounts::browser_find))
        .route("/api/browser/login", post(routes_accounts::browser_login))
        .route("/api/browser/select", post(routes_accounts::browser_select))
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
        .layer(middleware::from_fn_with_state(web.clone(), auth::guard))
        .with_state(web)
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
    let instructions = include_str!("../../agent/AGENTS.md");
    Ok(Json(
        json!({"instructions":instructions,"config":{"mcpServers":{"hycli":{"command":exe,"args":args,"env":{"HYCLI_DATA_DIR":web.core.root}}}},"command":command}),
    ))
}

async fn guide() -> Response {
    (
        [(header::CONTENT_TYPE, "text/html; charset=utf-8")],
        include_str!("../../browser-companion/guide.html"),
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
    let mut child = command.spawn()?;
    std::thread::spawn(move || {
        let _ = child.wait();
    });
    Ok(())
}
