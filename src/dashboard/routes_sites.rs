use super::{ActionBody, Import, Prepare, ProviderBody, Web, language, ok};
use crate::{
    apperr::{AppError, AppResult},
    policy::{self, Effect},
    util,
};
use axum::{
    Json,
    extract::{Path, State},
    http::{HeaderMap, header},
    response::{IntoResponse, Response},
};
use serde_json::{Value, json};

pub(super) async fn prepare(
    State(web): State<Web>,
    headers: HeaderMap,
    Json(body): Json<Prepare>,
) -> AppResult<Json<Value>> {
    Ok(Json(
        json!({"job":web.core.start_prepare_with_intent(&body.url,&body.site_id,&body.provider,language(&headers),&body.intent)?}),
    ))
}

pub(super) async fn import_spec(
    State(web): State<Web>,
    Json(body): Json<Import>,
) -> AppResult<Json<Value>> {
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

pub(super) async fn edit_site(
    State(web): State<Web>,
    Path(id): Path<String>,
    Json(body): Json<Value>,
) -> AppResult<Json<Value>> {
    let selected_spec = web.core.spec(&id)?;
    if let Some(account) = body.get("account_id").and_then(Value::as_str) {
        web.core
            .select_account(&id, account, selected_spec.site.source_url())?;
    }
    web.core.state.update(|d| {
        let m = d.sites.entry(id.clone()).or_default();
        if let Some(title) = body.get("title").and_then(Value::as_str) {
            m.title = title.trim().chars().take(100).collect();
        }
        if let Some(pinned) = body.get("pinned").and_then(Value::as_bool) {
            m.pinned = pinned;
        }
        if let Some(writes) = body.get("writes").and_then(Value::as_bool) {
            m.writes = writes;
        }
        m.updated_at = util::now();
        Ok(())
    })?;
    if body.get("writes") == Some(&Value::Bool(false)) {
        web.core.policy.invalidate(Some(&id), None)?;
        if let Ok(snapshot) = web.core.state.read() {
            for job in snapshot.jobs {
                if job.site_id == id
                    && ["queued", "running"].contains(&job.status.as_str())
                    && job.is_write_action()
                {
                    let _ = web.core.cancel_job(&job.id);
                }
            }
        }
    }
    web.core.changed();
    Ok(ok())
}

pub(super) async fn save_website_credential(
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

pub(super) async fn delete_website_credential(
    State(web): State<Web>,
    Path((id, name)): Path<(String, String)>,
) -> AppResult<Json<Value>> {
    web.core.set_website_credential(&id, &name, None)?;
    Ok(ok())
}

pub(super) async fn delete_site(
    State(web): State<Web>,
    Path(id): Path<String>,
) -> AppResult<Json<Value>> {
    web.core.remove_site(&id)?;
    Ok(ok())
}

pub(super) async fn export_spec(
    State(web): State<Web>,
    Path(id): Path<String>,
) -> AppResult<Response> {
    let sp = web.core.spec(&id)?;
    let content = serde_yaml::to_string(&sp).map_err(|_| AppError::api("internal", 500))?;
    Ok((
        [(header::CONTENT_TYPE, "application/yaml; charset=utf-8")],
        content,
    )
        .into_response())
}

pub(super) async fn site_image(
    State(web): State<Web>,
    Path(id): Path<String>,
) -> AppResult<Response> {
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

pub(super) async fn describe(
    State(web): State<Web>,
    Path(id): Path<String>,
    headers: HeaderMap,
    Json(body): Json<Prepare>,
) -> AppResult<Json<Value>> {
    Ok(Json(
        json!({"job":web.core.start_describe(&id,&body.provider,language(&headers))?}),
    ))
}

pub(super) async fn action(
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
