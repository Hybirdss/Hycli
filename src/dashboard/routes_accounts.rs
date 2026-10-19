use super::{Import, Pair, Prepare, ProviderBody, Web, ok, open_url, token};
use crate::{
    accounts, ai,
    apperr::{AppError, AppResult},
    policy,
    state::ProviderSettings,
    util,
};
use axum::{
    Json,
    extract::{Path, Query, State},
    http::header,
    response::{IntoResponse, Response},
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::BTreeMap;

pub(super) async fn save_provider(
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
    let model = body.model.trim();
    let config = ai::Config {
        provider: id.clone(),
        model: if model.is_empty() {
            provider.default_model.into()
        } else {
            model.to_owned()
        },
        key: key.trim().to_owned(),
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

pub(super) async fn check_provider(
    State(web): State<Web>,
    Path(id): Path<String>,
) -> AppResult<Json<Value>> {
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

pub(super) async fn delete_provider(
    State(web): State<Web>,
    Path(id): Path<String>,
) -> AppResult<Json<Value>> {
    web.core.remove_provider(&id)?;
    Ok(ok())
}

pub(super) async fn codex_login(State(web): State<Web>) -> AppResult<Json<Value>> {
    Ok(Json(web.core.ai.codex.login().await?))
}

pub(super) async fn codex_account(State(web): State<Web>) -> AppResult<Json<Value>> {
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

pub(super) async fn pair(
    State(web): State<Web>,
    Json(body): Json<Prepare>,
) -> AppResult<Json<Value>> {
    let url = crate::net::website_url(&body.url)?;
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

pub(super) async fn import_account(
    State(web): State<Web>,
    Json(body): Json<Import>,
) -> AppResult<Json<Value>> {
    let cookies = accounts::parse_cookies(&body.content, &body.url)?;
    let account =
        web.core
            .connect_account(&body.site_id, &body.url, cookies, "File import", "", "")?;
    Ok(Json(json!({"account":account})))
}

pub(super) async fn refresh_account(
    State(web): State<Web>,
    Path(id): Path<String>,
) -> AppResult<Json<Value>> {
    Ok(Json(
        json!({"account":web.core.refresh_account(&id).await?}),
    ))
}

pub(super) async fn delete_account(
    State(web): State<Web>,
    Path(id): Path<String>,
) -> AppResult<Json<Value>> {
    web.core.remove_account(&id)?;
    Ok(ok())
}

pub(super) async fn extension(
    Query(query): Query<BTreeMap<String, String>>,
) -> AppResult<Response> {
    Ok((
        [(header::CONTENT_TYPE, "application/zip")],
        crate::companion::archive(query.get("browser").is_some_and(|b| b == "firefox"))?,
    )
        .into_response())
}

pub(super) async fn browser_discovery(State(web): State<Web>) -> AppResult<Json<Value>> {
    let profiles = tokio::task::spawn_blocking(crate::browser_discovery::discover)
        .await
        .map_err(|_| AppError::api("internal", 500))?;
    let accounts: Vec<_> = web.core.state.read()?.accounts.values().cloned().collect();
    Ok(Json(
        json!({"profiles":profiles,"accounts":accounts,"method":"native_then_companion","credentials_exposed":false}),
    ))
}

#[derive(Deserialize)]
pub(super) struct BrowserFind {
    url: String,
    #[serde(default)]
    site_id: String,
    #[serde(default)]
    profile_id: String,
}

pub(super) async fn browser_find(
    State(web): State<Web>,
    Json(body): Json<BrowserFind>,
) -> AppResult<Json<Value>> {
    Ok(Json(serde_json::to_value(
        web.core
            .find_browser_accounts(&body.site_id, &body.url, &body.profile_id)
            .await?,
    )?))
}

pub(super) async fn browser_select(
    State(web): State<Web>,
    Json(body): Json<Value>,
) -> AppResult<Json<Value>> {
    let id = body.get("account_id").and_then(Value::as_str).unwrap_or("");
    let target = crate::net::website_url(body.get("url").and_then(Value::as_str).unwrap_or(""))?
        .origin()
        .ascii_serialization();
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

pub(super) async fn browser_login(
    State(web): State<Web>,
    Json(body): Json<BrowserFind>,
) -> AppResult<Json<Value>> {
    let target = crate::net::website_url(&body.url)?;
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
