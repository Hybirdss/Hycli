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
    let revision = web.core.begin_provider_change(&id)?;
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
    if id == "codex" && read_codex_account(&web).await?.is_none() {
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
    web.core.finish_provider_change(&id, revision, || {
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
                d.settings.default_provider = id.clone();
            }
            Ok(())
        })?;
        Ok(())
    })?;
    web.core.changed();
    Ok(ok())
}

pub(super) async fn check_provider(
    State(web): State<Web>,
    Path(id): Path<String>,
) -> AppResult<Json<Value>> {
    ai::provider(&id).ok_or_else(|| AppError::api("provider_missing", 400))?;
    let revision = web.core.begin_provider_change(&id)?;
    if id == "codex" {
        if read_codex_account(&web).await?.is_none() {
            return Err(AppError::api("auth_required", 401));
        }
        let models = web.core.ai.codex.models().await;
        let models_error = models.as_ref().err().map(|error| error.public_code.clone());
        activate_codex(&web, revision, None, models.ok())?;
        let mut response = serde_json::to_value(web.core.ai.codex.account_state())?;
        response["ok"] = json!(true);
        response["connected"] = json!(true);
        if let Some(error) = models_error {
            response["models_error"] = json!(error);
        }
        return Ok(Json(response));
    }
    let config = web.core.config(&id)?;
    let models = web.core.ai.check(&config).await?;
    web.core.finish_provider_change(&id, revision, || {
        web.core.state.update(|d| {
            if let Some(p) = d.providers.get_mut(&id) {
                p.models = models;
                p.checked_at = util::now();
            }
            Ok(())
        })
    })?;
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
    let revision = web.core.begin_provider_change("codex")?;
    Ok(Json(web.core.ai.codex.login_with_intent(revision, || {
        web.core.finish_provider_change("codex", revision, || Ok(()))
    }).await?))
}

#[derive(Deserialize)]
pub(super) struct CancelLogin {
    login_id: String,
}
pub(super) async fn codex_cancel_login(
    State(web): State<Web>,
    Json(body): Json<CancelLogin>,
) -> AppResult<Json<Value>> {
    validate_login_id(&body.login_id)?;
    web.core.ai.codex.cancel_login(&body.login_id).await?;
    Ok(ok())
}

#[derive(Default, Deserialize)]
pub(super) struct CodexAccountQuery {
    login_id: Option<String>,
}
pub(super) async fn codex_account(
    State(web): State<Web>,
    Query(query): Query<CodexAccountQuery>,
) -> AppResult<Json<Value>> {
    if let Some(id) = &query.login_id {
        validate_login_id(id)?;
        web.core.ai.codex.login_status(id)?;
    }
    let account = read_codex_account(&web).await?;
    let mut response = serde_json::to_value(web.core.ai.codex.account_state())?;
    // This endpoint describes official account evidence, not saved configuration.
    // A passive read never creates a provider or changes its model/default.
    response["connected"] = json!(account.is_some());
    response["account"] = json!(account);
    if let Some(id) = &query.login_id {
        let (status, revision, activation_pending) = web.core.ai.codex.login_status(id)?;
        web.core.finish_provider_change("codex", revision, || Ok(()))?;
        response["login_status"] = json!(status);
        if status == ai::codex::LoginStatus::Completed && account.is_some() && activation_pending {
            let models = web.core.ai.codex.models().await;
            let models_error = models.as_ref().err().map(|error| error.public_code.clone());
            activate_codex(&web, revision, Some(id), models.ok())?;
            if let Some(error) = models_error {
                response["models_error"] = json!(error);
            }
        }
    }
    Ok(Json(response))
}

fn validate_login_id(id: &str) -> AppResult<()> {
    if id.is_empty() || id.len() > 512 {
        return Err(AppError::api("bad_request", 400));
    }
    Ok(())
}

async fn read_codex_account(web: &Web) -> AppResult<Option<String>> {
    let result = web.core.ai.codex.refresh_account().await;
    web.core.changed();
    result
}

fn activate_codex(
    web: &Web,
    revision: u64,
    login_id: Option<&str>,
    models: Option<Vec<String>>,
) -> AppResult<()> {
    web.core.finish_provider_change("codex", revision, || {
        let apply = || web.core.state.update(|d| {
            let provider = d.providers.entry("codex".into()).or_default();
            provider.connected = true;
            if let Some(models) = models {
                provider.models = models;
                provider.checked_at = util::now();
            }
            // An empty model delegates to Codex's own configured default, as before.
            // Never overwrite an existing Hycli selection while refreshing evidence.
            if d.settings.default_provider.is_empty() {
                d.settings.default_provider = "codex".into();
            }
            Ok(())
        });
        if let Some(id) = login_id {
            web.core.ai.codex.activate_login(id, revision, apply)
        } else {
            apply()
        }
    })?;
    web.core.changed();
    Ok(())
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
    if let Some(site) = body
        .get("site_id")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
    {
        web.core.select_verified_account(site, id, &target)?;
        return Ok(ok());
    }
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
