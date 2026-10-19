use super::{Web, token};
use crate::{
    apperr::{AppError, AppResult},
    runtime::Runtime,
    util,
};
use axum::{
    Json,
    extract::{Query, Request, State},
    http::{HeaderMap, header},
    middleware::Next,
    response::{IntoResponse, Response},
};
use serde::Deserialize;
use serde_json::json;
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
};

#[derive(serde::Serialize, Deserialize)]
struct Secrets {
    session: String,
    csrf: String,
    launch: String,
}

impl Web {
    /// Secrets persist in the private data directory, so an open dashboard
    /// reconnects after a restart without a new launch address.
    pub fn new(core: Arc<Runtime>, port: u16) -> AppResult<Self> {
        let path = core.root.join("dashboard-session.json");
        let secrets = match util::read_bounded(&path, 4096)
            .ok()
            .and_then(|data| serde_json::from_slice::<Secrets>(&data).ok())
        {
            Some(secrets) => secrets,
            None => {
                let secrets = Secrets {
                    session: token(),
                    csrf: token(),
                    launch: token(),
                };
                util::atomic_file(&path, &serde_json::to_vec(&secrets)?)?;
                secrets
            }
        };
        Ok(Self {
            core,
            port,
            session: secrets.session,
            csrf: secrets.csrf,
            launch: secrets.launch,
            instance_id: util::id(),
            shutdown: tokio::sync::watch::channel(false).0,
            pairs: Arc::new(Mutex::new(BTreeMap::new())),
        })
    }

    pub fn launch_url(&self) -> String {
        format!("http://127.0.0.1:{}/?launch={}", self.port, self.launch)
    }
}

fn session_cookie(headers: &HeaderMap) -> &str {
    headers
        .get(header::COOKIE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .split(';')
        .find_map(|p| p.trim().strip_prefix("hycli_session="))
        .unwrap_or("")
}

pub(super) async fn guard(State(web): State<Web>, request: Request, next: Next) -> Response {
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
            if !util::constant_eq(session_cookie(request.headers()), &web.session) {
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
        headers.insert(
            "content-security-policy",
            "default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data:; connect-src 'self'; font-src 'self'; frame-ancestors 'none'; base-uri 'none'; form-action 'self'"
                .parse()
                .unwrap(),
        );
    }
    response
}

pub(super) async fn session(
    State(web): State<Web>,
    headers: HeaderMap,
    Query(query): Query<BTreeMap<String, String>>,
) -> Response {
    let launched = query
        .get("launch")
        .is_some_and(|value| util::constant_eq(value, &web.launch));
    if !launched && !util::constant_eq(session_cookie(&headers), &web.session) {
        return AppError::api("launch_required", 401).into_response();
    }
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
