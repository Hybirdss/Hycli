//! One-time, per-site pairing between a browser extension and the local broker.
use crate::{
    accounts::Cookie,
    apperr::{AppError, AppResult},
    dashboard::Web,
    util,
};
use axum::{
    Json,
    extract::State,
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
};
use serde::Deserialize;
use serde_json::json;
#[derive(Deserialize)]
pub struct PairBody {
    code: String,
}
#[derive(Deserialize)]
pub struct ConnectBody {
    token: String,
    cookies: Vec<Cookie>,
    #[serde(default)]
    browser: String,
    #[serde(default)]
    profile: String,
    #[serde(default)]
    identity_source: String,
}
fn extension_origin(headers: &HeaderMap) -> AppResult<String> {
    let origin = headers
        .get(header::ORIGIN)
        .and_then(|h| h.to_str().ok())
        .ok_or_else(|| AppError::api("forbidden", 403))?;
    let u = url::Url::parse(origin).map_err(|_| AppError::api("forbidden", 403))?;
    if !["chrome-extension", "moz-extension"].contains(&u.scheme())
        || u.host_str().is_none()
        || u.path() != "" && u.path() != "/"
        || u.query().is_some()
        || u.fragment().is_some()
    {
        return Err(AppError::api("forbidden", 403));
    }
    Ok(origin.into())
}
fn cors(origin: &str, value: impl IntoResponse) -> Response {
    let mut response = value.into_response();
    response
        .headers_mut()
        .insert(header::ACCESS_CONTROL_ALLOW_ORIGIN, origin.parse().unwrap());
    response
        .headers_mut()
        .insert(header::VARY, "Origin".parse().unwrap());
    response.headers_mut().insert(
        header::ACCESS_CONTROL_ALLOW_METHODS,
        "POST, OPTIONS".parse().unwrap(),
    );
    response.headers_mut().insert(
        header::ACCESS_CONTROL_ALLOW_HEADERS,
        "content-type".parse().unwrap(),
    );
    response
}
pub async fn options(headers: HeaderMap) -> AppResult<Response> {
    let origin = extension_origin(&headers)?;
    Ok(cors(&origin, StatusCode::NO_CONTENT))
}
pub async fn pair(
    State(web): State<Web>,
    headers: HeaderMap,
    Json(body): Json<PairBody>,
) -> AppResult<Response> {
    let origin = extension_origin(&headers)?;
    let code = body.code.replace(' ', "").to_ascii_uppercase();
    let mut pairs = web
        .pairs
        .lock()
        .map_err(|_| AppError::api("internal", 500))?;
    let p = pairs
        .get_mut(&code)
        .ok_or_else(|| AppError::api("pairing_expired", 400))?;
    if p.expires < util::unix_ms()
        || (!p.extension_origin.is_empty() && p.extension_origin != origin)
    {
        return Err(AppError::api("pairing_expired", 400));
    }
    p.extension_origin = origin.clone();
    Ok(cors(
        &origin,
        Json(json!({"token":p.token,"url":p.url,"expires_at":p.expires})),
    ))
}
pub async fn connect(
    State(web): State<Web>,
    headers: HeaderMap,
    Json(body): Json<ConnectBody>,
) -> AppResult<Response> {
    let origin = extension_origin(&headers)?;
    let pairing = {
        let mut pairs = web
            .pairs
            .lock()
            .map_err(|_| AppError::api("internal", 500))?;
        let key = pairs
            .iter()
            .find(|(_, p)| {
                p.expires > util::unix_ms()
                    && p.extension_origin == origin
                    && util::constant_eq(&p.token, &body.token)
            })
            .map(|(k, _)| k.clone())
            .ok_or_else(|| AppError::api("pairing_expired", 400))?;
        pairs
            .remove(&key)
            .ok_or_else(|| AppError::api("pairing_expired", 400))?
    };
    let source = if body.identity_source.is_empty() {
        String::new()
    } else {
        let u = crate::net::validate_url(&body.identity_source)?;
        if crate::runtime::origin(&pairing.url)? != u.origin().ascii_serialization()
            || !crate::accounts::identity_endpoint(&u)
        {
            return Err(AppError::api("data_invalid", 400));
        }
        u.to_string()
    };
    let account = web.core.connect_account(
        &pairing.site_id,
        &pairing.url,
        body.cookies,
        &body.browser,
        &body.profile,
        &source,
    )?;
    let session_token = format!("{}{}", util::id(), util::id());
    let mut credential = web.core.credential(&account.id)?;
    credential.browser_session = session_token.clone();
    credential.extension_origin = origin.clone();
    credential.browser_session_expires_ms = util::unix_ms() + 7 * 24 * 60 * 60 * 1000;
    web.core
        .vault
        .lock()
        .map_err(|_| AppError::api("internal", 500))?
        .set(
            "accounts",
            &account.id,
            &serde_json::to_string(&credential)?,
        )?;
    let core = web.core.clone();
    let id = account.id.clone();
    if !source.is_empty() {
        tokio::spawn(async move {
            let _ = core.refresh_account(&id).await;
        });
    }
    Ok(cors(
        &origin,
        Json(json!({"connected":true,"account_id":account.id,"session_token":session_token})),
    ))
}
pub fn archive(firefox: bool) -> AppResult<Vec<u8>> {
    use std::io::{Cursor, Write};
    let mut archive = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for (name, bytes) in [
        (
            "manifest.json",
            if firefox {
                include_bytes!("../browser-companion/manifest.firefox.json").as_slice()
            } else {
                include_bytes!("../browser-companion/manifest.json").as_slice()
            },
        ),
        (
            "popup.html",
            include_bytes!("../browser-companion/popup.html").as_slice(),
        ),
        (
            "locales.js",
            include_bytes!("../browser-companion/locales.js").as_slice(),
        ),
        (
            "popup.js",
            include_bytes!("../browser-companion/popup.js").as_slice(),
        ),
        (
            "background.js",
            include_bytes!("../browser-companion/background.js").as_slice(),
        ),
        (
            "style.css",
            include_bytes!("../browser-companion/style.css").as_slice(),
        ),
        (
            "guide.html",
            include_bytes!("../browser-companion/guide.html").as_slice(),
        ),
        (
            "bird.png",
            include_bytes!("../dashboard/public/brand/browser-companion.png").as_slice(),
        ),
    ] {
        archive
            .start_file(
                name,
                zip::write::SimpleFileOptions::default()
                    .compression_method(zip::CompressionMethod::Deflated),
            )
            .map_err(|_| AppError::api("internal", 500))?;
        archive.write_all(bytes)?;
    }
    Ok(archive
        .finish()
        .map_err(|_| AppError::api("internal", 500))?
        .into_inner())
}

#[derive(Deserialize)]
pub struct ObserveBody {
    token: String,
    observations: Vec<crate::accounts::Observation>,
}
pub async fn observe(
    State(web): State<Web>,
    headers: HeaderMap,
    Json(body): Json<ObserveBody>,
) -> AppResult<Response> {
    let origin = extension_origin(&headers)?;
    if body.observations.len() > 40 {
        return Err(AppError::api("too_large", 413));
    }
    let mut matched = None;
    for account in web.core.state.read()?.accounts.values() {
        let credential = web.core.credential(&account.id)?;
        if credential.extension_origin == origin
            && credential.browser_session_expires_ms > util::unix_ms()
            && util::constant_eq(&credential.browser_session, &body.token)
        {
            matched = Some((account.clone(), credential));
            break;
        }
    }
    let (account, credential) = matched.ok_or_else(|| AppError::api("forbidden", 403))?;
    let mut observations = vec![];
    let mut identity = String::new();
    for mut observation in body.observations {
        if !["GET", "POST", "PUT", "PATCH", "DELETE", "HEAD"].contains(&observation.method.as_str())
            || !observation.path.starts_with('/')
            || observation.path.starts_with("//")
            || observation.path.contains(['?', '#', '\\', '\r', '\n'])
            || observation.path.len() > 600
            || observation.query.len() > 80
            || observation.body.len() > 80
        {
            continue;
        }
        observation.query.retain(|key, kind| {
            !util::secret_field(key)
                && key.len() < 100
                && ["string", "int", "float", "bool", "json"].contains(&kind.as_str())
        });
        observation.body.retain(|key, kind| {
            !util::secret_field(key)
                && key.len() < 100
                && ["string", "int", "float", "bool", "json"].contains(&kind.as_str())
        });
        if util::redact_text(&observation.path) != observation.path
            || credential
                .redaction_values()
                .iter()
                .any(|secret| !secret.is_empty() && observation.path.contains(secret))
        {
            continue;
        }
        observation.observed_at = util::now();
        if observation.method == "GET"
            && observation.query.is_empty()
            && (200..300).contains(&observation.status)
        {
            let u =
                crate::net::validate_url(&format!("{}{}", credential.origin, observation.path))?;
            if crate::accounts::identity_endpoint(&u) {
                identity = u.to_string();
            }
        }
        observations.push(observation);
    }
    web.core.state.update(|d| {
        let meta = d.sites.entry(account.site_id.clone()).or_default();
        for observation in observations {
            meta.observations
                .retain(|old| old.method != observation.method || old.path != observation.path);
            meta.observations.push(observation);
        }
        if meta.observations.len() > 160 {
            meta.observations.drain(..meta.observations.len() - 160);
        }
        if !identity.is_empty() && !meta.identity_paths.contains(&identity) {
            meta.identity_paths.insert(0, identity.clone());
            meta.identity_paths.truncate(8);
        }
        Ok(())
    })?;
    if !identity.is_empty() && !account.verified_identity {
        let core = web.core.clone();
        let id = account.id;
        tokio::spawn(async move {
            let _ = core.refresh_account(&id).await;
        });
    }
    web.core.changed();
    Ok(cors(&origin, Json(json!({"ok":true}))))
}

#[derive(Deserialize)]
pub struct RefreshBody {
    token: String,
    cookies: Vec<Cookie>,
    #[serde(default)]
    identity_source: String,
}
/// A previously paired browser may refresh only its own origin-bound sign-in.
pub async fn refresh(
    State(web): State<Web>,
    headers: HeaderMap,
    Json(body): Json<RefreshBody>,
) -> AppResult<Response> {
    let origin = extension_origin(&headers)?;
    let mut matched = None;
    for account in web.core.state.read()?.accounts.values() {
        let credential = web.core.credential(&account.id)?;
        if credential.extension_origin == origin
            && credential.browser_session_expires_ms > util::unix_ms()
            && !credential.browser_session.is_empty()
            && util::constant_eq(&credential.browser_session, &body.token)
        {
            matched = Some((account.clone(), credential));
            break;
        }
    }
    let (account, mut credential) = matched.ok_or_else(|| AppError::api("forbidden", 403))?;
    let url = crate::net::validate_url(&credential.origin)?;
    let normalized = if body.cookies.is_empty() {
        vec![]
    } else {
        crate::accounts::normalize(body.cookies, &url)?
    };
    let changed = serde_json::to_vec(&normalized)? != serde_json::to_vec(&credential.cookies)?;
    credential.cookies = normalized;
    if !body.identity_source.is_empty() {
        let source = crate::net::validate_url(&body.identity_source)?;
        if source.origin() != url.origin() || !crate::accounts::identity_endpoint(&source) {
            return Err(AppError::api("data_invalid", 400));
        }
        credential.identity_source = source.to_string();
    }
    web.core
        .vault
        .lock()
        .map_err(|_| AppError::api("internal", 500))?
        .set(
            "accounts",
            &account.id,
            &serde_json::to_string(&credential)?,
        )?;
    web.core.state.update(|state| {
        if let Some(account) = state.accounts.get_mut(&account.id) {
            account.cookie_count = credential.cookies.len();
            if credential.cookies.is_empty() {
                account.status = "expired".into();
            }
            if changed {
                account.verified_identity = false;
                account.label.clear();
                account.name.clear();
                account.email.clear();
                account.user_id.clear();
            }
        }
        Ok(())
    })?;
    if !credential.identity_source.is_empty() && !credential.cookies.is_empty() {
        let core = web.core.clone();
        let id = account.id.clone();
        tokio::spawn(async move {
            let _ = core.refresh_account(&id).await;
        });
    }
    web.core.changed();
    Ok(cors(
        &origin,
        Json(json!({"refreshed":true,"account_id":account.id})),
    ))
}
