use super::{Prepare, Web, language, ok};
use crate::{
    apperr::{AppError, AppResult},
    util,
};
use axum::{
    Json,
    extract::{Path, State},
    http::HeaderMap,
};
use serde_json::{Value, json};

pub(super) async fn approve(
    State(web): State<Web>,
    Path(id): Path<String>,
    headers: HeaderMap,
) -> AppResult<Json<Value>> {
    let review = web.core.policy.review(&id)?;
    let request = review
        .request
        .ok_or_else(|| AppError::api("approval_expired", 409))?;
    web.core.policy.claim(&id)?;
    let job = match web
        .core
        .start_action(request, Some(id.clone()), language(&headers))
    {
        Ok(job) => job,
        Err(err) => {
            let _ = web.core.policy.unclaim(&id);
            return Err(err);
        }
    };
    Ok(Json(json!({"job": job})))
}

pub(super) async fn reject(
    State(web): State<Web>,
    Path(id): Path<String>,
) -> AppResult<Json<Value>> {
    web.core.policy.reject(&id)?;
    web.core.changed();
    Ok(ok())
}

pub(super) async fn cancel(
    State(web): State<Web>,
    Path(id): Path<String>,
) -> AppResult<Json<Value>> {
    web.core.cancel_job(&id)?;
    Ok(ok())
}

pub(super) async fn retry(
    State(web): State<Web>,
    Path(id): Path<String>,
) -> AppResult<Json<Value>> {
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

pub(super) async fn job_details(
    State(web): State<Web>,
    Path(id): Path<String>,
) -> AppResult<Json<Value>> {
    Ok(Json(json!(web.core.state.job(&id)?)))
}

pub(super) async fn result(
    State(web): State<Web>,
    Path(id): Path<String>,
) -> AppResult<Json<Value>> {
    Ok(Json(web.core.state.job(&id)?.result.unwrap_or(Value::Null)))
}

pub(super) async fn summarize(
    State(web): State<Web>,
    Path(id): Path<String>,
    Json(body): Json<Prepare>,
) -> AppResult<Json<Value>> {
    Ok(Json(
        json!({"job": web.core.start_summary(&id, &body.provider)?}),
    ))
}

pub(super) async fn export_activity(State(web): State<Web>) -> AppResult<Json<Value>> {
    Ok(Json(
        json!({"version":1,"exported_at":util::now(),"jobs":web.core.state.read()?.jobs}),
    ))
}

pub(super) async fn clear_activity(State(web): State<Web>) -> AppResult<Json<Value>> {
    web.core.state.update(|d| {
        d.jobs
            .retain(|j| ["queued", "running"].contains(&j.status.as_str()));
        Ok(())
    })?;
    web.core.changed();
    Ok(ok())
}
