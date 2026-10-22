use super::{Prepare, Web, language, ok};
use crate::{
    apperr::{AppError, AppResult},
    util,
};
use axum::{
    Json,
    extract::{Path, State},
    http::{HeaderMap, header},
    response::{IntoResponse, Response},
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

pub(super) async fn artifact(
    State(web): State<Web>,
    Path((id, artifact_id)): Path<(String, String)>,
) -> AppResult<Response> {
    let opened = open_job_artifact(&web, &id, &artifact_id).await?;
    let headers = artifact_headers(&opened.manifest);
    Ok((headers, axum::body::Body::from_stream(opened.into_stream())).into_response())
}

pub(super) async fn artifact_head(
    State(web): State<Web>,
    Path((id, artifact_id)): Path<(String, String)>,
) -> AppResult<Response> {
    // Availability only. A HEAD cannot verify the payload digest.
    let opened = open_job_artifact(&web, &id, &artifact_id).await?;
    Ok((
        artifact_headers(&opened.manifest),
        axum::body::Body::empty(),
    )
        .into_response())
}

async fn open_job_artifact(
    web: &Web,
    id: &str,
    artifact_id: &str,
) -> AppResult<crate::artifacts::OpenedArtifact> {
    let job = web.core.state.job(id)?;
    if job.status != "completed" || job.kind != "action" {
        return Err(AppError::api("not_found", 404));
    }
    let expected = job
        .artifact
        .ok_or_else(|| AppError::api("not_found", 404))?;
    if expected.kind != "file"
        || expected.version != 1
        || expected.artifact_id != artifact_id
        || expected.status != "complete"
    {
        return Err(AppError::api("not_found", 404));
    }
    let root = web.core.root.clone();
    let artifact_id = artifact_id.to_owned();
    let opened = tokio::task::spawn_blocking(move || {
        crate::artifacts::ArtifactStore::open(&root)?.open_completed(&artifact_id)
    })
    .await
    .map_err(|_| AppError::api("internal", 500))??;
    if opened.manifest != expected {
        return Err(AppError::api("data_invalid", 409));
    }
    Ok(opened)
}

fn artifact_headers(
    stored: &crate::artifacts::CompletedArtifact,
) -> [(header::HeaderName, String); 4] {
    let filename: String = stored
        .filename
        .as_bytes()
        .iter()
        .map(|byte| format!("%{byte:02X}"))
        .collect();
    [
        (header::CONTENT_TYPE, "application/octet-stream".into()),
        (
            header::CONTENT_DISPOSITION,
            format!("attachment; filename=\"download\"; filename*=UTF-8''{filename}"),
        ),
        (header::CACHE_CONTROL, "no-store".into()),
        (header::X_CONTENT_TYPE_OPTIONS, "nosniff".into()),
    ]
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::artifacts::{ArtifactStore, Provenance};
    use crate::{runtime::Runtime, state::Job};

    #[tokio::test]
    async fn file_retrieval_requires_session_owning_job_and_intact_artifact() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join(".cache/file-route")
            .join(util::id());
        let core = Runtime::open(&root, None, None, false, true, false).unwrap();
        let store = ArtifactStore::open(&root).unwrap();
        let bytes = b"\x89PNG\r\n\x1a\n\x00\xff\x80binary";
        let mut writable = store.begin().unwrap();
        writable.write(bytes).await.unwrap();
        let file = writable
            .seal()
            .await
            .unwrap()
            .publish(
                "report.html",
                "application/octet-stream",
                Provenance {
                    site: "fixture".into(),
                    action: "download".into(),
                    source_origin: "https://example.test".into(),
                },
            )
            .unwrap()
            .persisted();
        let mut job = Job::new("action", "en");
        job.status = "completed".into();
        job.site_id = "fixture".into();
        job.result = Some(serde_json::to_value(&file).unwrap());
        job.artifact = Some(file.clone());
        core.state.add_job(job.clone()).unwrap();
        let mut other = Job::new("action", "en");
        other.status = "completed".into();
        other.result = Some(serde_json::to_value(&file).unwrap());
        core.state.add_job(other.clone()).unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let web = Web::new(core.clone(), port).unwrap();
        let ticket = web.test_bootstrap_ticket();
        let server = tokio::spawn(async move {
            axum::serve(listener, super::super::router(web))
                .await
                .unwrap();
        });
        let client = reqwest::Client::builder().no_proxy().build().unwrap();
        let session = client
            .post(format!("http://127.0.0.1:{port}/api/bootstrap"))
            .json(&serde_json::json!({"ticket": ticket}))
            .send()
            .await
            .unwrap();
        assert_eq!(session.status(), 200);
        let cookie = session.headers()["set-cookie"]
            .to_str()
            .unwrap()
            .split(';')
            .next()
            .unwrap()
            .to_owned();
        let endpoint = format!(
            "http://127.0.0.1:{port}/api/jobs/{}/artifacts/{}",
            job.id, file.artifact_id
        );
        assert_eq!(client.get(&endpoint).send().await.unwrap().status(), 401);
        for _ in 0..2 {
            let response = client
                .get(&endpoint)
                .header("cookie", &cookie)
                .send()
                .await
                .unwrap();
            assert_eq!(response.status(), 200);
            assert_eq!(
                response.headers()["content-type"],
                "application/octet-stream"
            );
            assert!(
                response.headers()["content-disposition"]
                    .to_str()
                    .unwrap()
                    .starts_with("attachment;")
            );
            assert!(response.headers().get(header::CONTENT_LENGTH).is_none());
            assert_eq!(response.headers()["cache-control"], "no-store");
            assert_eq!(response.headers()["x-content-type-options"], "nosniff");
            assert_eq!(response.bytes().await.unwrap().as_ref(), bytes);
        }
        assert_eq!(
            client
                .get(endpoint.replace(&job.id, &other.id))
                .header("cookie", &cookie)
                .send()
                .await
                .unwrap()
                .status(),
            404
        );
        assert_eq!(
            core.start_summary(&other.id, "absent-provider")
                .err()
                .unwrap()
                .public_code,
            "provider_missing"
        );
        assert_eq!(
            core.start_summary(&job.id, "absent-provider")
                .err()
                .unwrap()
                .public_code,
            "bad_request"
        );
        core.state
            .update_job(&job.id, |job| {
                job.artifact.as_mut().unwrap().local_path = "/unrelated/private-file".into();
            })
            .unwrap();
        assert_eq!(
            client
                .get(&endpoint)
                .header("cookie", &cookie)
                .send()
                .await
                .unwrap()
                .status(),
            409
        );
        core.state
            .update_job(&job.id, |job| {
                job.artifact = Some(file.clone());
            })
            .unwrap();
        let head = client
            .head(&endpoint)
            .header("cookie", &cookie)
            .send()
            .await
            .unwrap();
        assert_eq!(head.status(), 200);
        assert_eq!(client.head(&endpoint).send().await.unwrap().status(), 401);
        // HEAD checks availability, not integrity. Same-size corruption can emit
        // provisional bytes, but must fail the actual HTTP transfer at EOF.
        std::fs::write(&file.local_path, vec![b'x'; bytes.len()]).unwrap();
        assert_eq!(
            client
                .head(&endpoint)
                .header("cookie", &cookie)
                .send()
                .await
                .unwrap()
                .status(),
            200
        );
        match client.get(&endpoint).header("cookie", &cookie).send().await {
            Ok(response) => {
                assert!(response.headers().get(header::CONTENT_LENGTH).is_none());
                assert!(response.bytes().await.is_err());
            }
            Err(_) => {} // HTTP may fail before response headers are delivered.
        }
        std::fs::write(&file.local_path, b"changed synthetic bytes").unwrap();
        assert!(
            !client
                .get(&endpoint)
                .header("cookie", &cookie)
                .send()
                .await
                .unwrap()
                .status()
                .is_success()
        );
        std::fs::remove_file(&file.local_path).unwrap();
        assert_eq!(
            client
                .get(&endpoint)
                .header("cookie", &cookie)
                .send()
                .await
                .unwrap()
                .status(),
            404
        );
        assert_eq!(core.state.read().unwrap().jobs.len(), 2);
        server.abort();
        let _ = server.await;
        drop(core);
        std::fs::remove_dir_all(root).unwrap();
    }
}
