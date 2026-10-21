//! Bounded session diagnostics shared by command surfaces; response values stay local.
use super::Runtime;
use crate::{
    accounts::{AccountView, Credential, Observation},
    apperr::{AppError, AppResult},
    session_recipe::ConnectionRecipe,
};
use serde_json::{Value, json};

impl Runtime {
    pub(crate) fn inspect_account_session(&self, id: &str) -> AppResult<Value> {
        let (account, credential) = self.session_account(id)?;
        let credential = self.capture_candidate(id, &credential)?;
        Self::session_output(
            &credential,
            Self::inspect_session_snapshot(
                id,
                &credential,
                &super::origin(&account.site_url)?,
                "",
                0,
            )?,
        )
    }

    pub(crate) async fn observe_account_session(
        &self,
        id: &str,
        url: &str,
        browser: bool,
    ) -> AppResult<Value> {
        let (account, credential) = self.session_account(id)?;
        let target = session_get_target(&account, url)?;
        if browser {
            // Page-level interception cannot constrain every worker and WebSocket channel.
            // Reject before launching a browser, sending a request or changing saved state.
            return Err(AppError::api("browser_diagnostic_unsupported", 422));
        }
        let (observations, result, json_response) = {
            self.policy
                .resume_auth(&format!("{}|{id}", target.origin().ascii_serialization()))?;
            // A diagnostic is one request: every next URL needs its own validation.
            let response = self
                .network
                .fetch(crate::net::Request {
                    url: target.to_string(),
                    method: "GET".into(),
                    headers: credential.request_headers(target.as_str())?,
                    body: None,
                    account_id: id.into(),
                    limit: 2 * 1024 * 1024,
                    timeout: std::time::Duration::from_secs(25),
                    read_only: true,
                    follow_redirects: false,
                    tls_profile: "standard".into(),
                    multipart: None,
                })
                .await?;
            let observation = Observation {
                method: "GET".into(),
                path: target.path().into(),
                query: target
                    .query_pairs()
                    .map(|(key, _)| (key.into_owned(), "string".into()))
                    .collect(),
                status: response.status,
                observed_at: crate::util::now(),
                ..Default::default()
            };
            let mime = response
                .header("content-type")
                .split(';')
                .next()
                .unwrap_or("")
                .trim()
                .to_string();
            let json_response = (mime == "application/json" || mime.ends_with("+json"))
                && response.json().is_some();
            (
                vec![observation],
                json!({"status":response.status,"successful":response.successful(),"response_shape":response.json().as_ref().map(|body|crate::session_recipe::shape(body,0)),"values_exposed":false}),
                json_response,
            )
        };
        let _guard = crate::util::file_lock(&self.root.join("account-selection.lock"), false)?;
        if serde_json::to_value(self.credential(id)?)? != serde_json::to_value(&credential)? {
            return Err(AppError::api("conflict", 409));
        }
        self.state.update(|data| {
            let current = data
                .accounts
                .get(id)
                .ok_or_else(|| AppError::api("not_found", 404))?;
            if current.site_id != account.site_id || current.site_url != account.site_url {
                return Err(AppError::api("conflict", 409));
            }
            let meta = data.sites.entry(account.site_id.clone()).or_default();
            for observation in &observations {
                if let Some(old) = meta.observations.iter_mut().find(|old| {
                    old.method == observation.method
                        && old.path == observation.path
                        && old.query.keys().eq(observation.query.keys())
                }) {
                    *old = observation.clone();
                } else {
                    meta.observations.push(observation.clone());
                }
            }
            Ok(())
        })?;
        let read_evidence =
            self.save_account_read_observation(id, &credential, target.as_str(), json_response)?;
        self.changed();
        Self::session_output(
            &credential,
            json!({"account_id":id,"observation":result,"requests":observations,"read_evidence":read_evidence}),
        )
    }

    pub(crate) async fn verify_observed_session(
        &self,
        id: &str,
        recipe: &ConnectionRecipe,
    ) -> AppResult<Value> {
        let (account, credential) = self.session_account(id)?;
        let candidate = self.capture_candidate(id, &credential)?;
        self.verify_observed_snapshot(id, recipe, &account, credential, candidate).await
    }

    async fn verify_observed_snapshot(
        &self,
        id: &str,
        recipe: &ConnectionRecipe,
        account: &AccountView,
        credential: Credential,
        candidate: Credential,
    ) -> AppResult<Value> {
        let target = session_get_target(account, &recipe.identity_url)?;
        let query_keys: std::collections::BTreeSet<_> = target
            .query_pairs()
            .map(|(key, _)| key.into_owned())
            .collect();
        let observed = self
            .state
            .read()?
            .sites
            .get(&account.site_id)
            .is_some_and(|meta| {
                meta.observations.iter().any(|observation| {
                    observation.method == "GET"
                        && observation.path == target.path()
                        && observation.query.keys().eq(query_keys.iter())
                })
            });
        if !observed || recipe.evidence != "observed_browser_request" {
            return Err(AppError::api("unsupported_evidence", 422));
        }
        let view = self
            .verify_session_recipe_candidate(id, recipe, credential.clone(), candidate.clone())
            .await?;
        Self::session_output(&candidate, Self::session_output(&credential, serde_json::to_value(view)?)?)
    }

    pub(crate) async fn refresh_account_session(&self, id: &str) -> AppResult<Value> {
        let (account, credential) = self.session_account(id)?;
        let recipe = credential
            .recipe
            .clone()
            .ok_or_else(|| AppError::api("identity_unavailable", 422))?;
        let result = self
            .verify_observed_snapshot(id, &recipe, &account, credential.clone(), credential.clone())
            .await;
        if let Err(error) = &result {
            if matches!(
                error.public_code.as_str(),
                "auth_required" | "identity_unavailable"
            ) {
                self.invalidate_identity_if_current(
                    id,
                    &credential,
                    error.public_code == "auth_required",
                )?;
            }
        }
        result
    }

    fn session_output(credential: &Credential, value: Value) -> AppResult<Value> {
        let secrets = credential.session_redaction_values();
        Ok(super::sanitized(value, &secrets))
    }

    fn session_account(&self, id: &str) -> AppResult<(AccountView, Credential)> {
        let account = self
            .state
            .account(id)?
            .ok_or_else(|| AppError::api("not_found", 404))?;
        let credential = self.credential(id)?;
        if crate::net::validate_url(&account.site_url)?.origin()
            != crate::net::validate_url(&credential.origin)?.origin()
        {
            return Err(AppError::api("unsafe_target", 400));
        }
        Ok((account, credential))
    }
}

fn session_get_target(account: &AccountView, url: &str) -> AppResult<url::Url> {
    let target = crate::net::validate_url(url)?;
    if target.origin() != crate::net::validate_url(&account.site_url)?.origin()
        || !crate::policy::safe_read_url(&target)
    {
        return Err(AppError::api("unsafe_target", 400));
    }
    if !crate::policy::safe_session_request(
        &target,
        &crate::net::validate_url(&account.site_url)?,
        "GET",
    ) {
        return Err(AppError::api("unsafe_target", 400));
    }
    Ok(target)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{Json, Router, routing::get};
    use std::sync::Arc;

    #[test]
    fn session_output_preserves_colliding_secret_keys_and_ordinary_entries() {
        let credential = Credential {
            headers: [
                ("authorization".into(), "session-canary-one".into()),
                ("x-session".into(), "session-canary-two".into()),
            ]
            .into(),
            ..Credential::default()
        };
        let result = Runtime::session_output(
            &credential,
            json!({"nested":[{
                "session-canary-one":"first entry",
                "session-canary-two":"second entry",
                "[private]":"ordinary entry",
                "[private]#_1":"ordinary suffixed entry"
            }]}),
        )
        .unwrap();
        let text = result.to_string();
        assert!(!text.contains("session-canary-one"));
        assert!(!text.contains("session-canary-two"));
        let fields = result["nested"][0].as_object().unwrap();
        assert_eq!(fields.len(), 4);
        assert_eq!(fields["[private]"], "ordinary entry");
        assert_eq!(fields["[private]#_1"], "ordinary suffixed entry");
        assert!(fields.values().any(|value| value == "first entry"));
        assert!(fields.values().any(|value| value == "second entry"));
    }

    #[tokio::test]
    async fn stale_http_observation_cannot_publish_or_expose_previous_session_metadata() {
        let entered = Arc::new(tokio::sync::Notify::new());
        let release = Arc::new(tokio::sync::Notify::new());
        let arrived = entered.clone();
        let resume = release.clone();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let origin = format!("http://{}", listener.local_addr().unwrap());
        let router = Router::new().route(
            "/current-context",
            get(move || {
                let arrived = arrived.clone();
                let resume = resume.clone();
                async move {
                    arrived.notify_one();
                    resume.notified().await;
                    Json(json!({"synthetic-old-session":"private shape key"}))
                }
            }),
        );
        let server = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
        let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join(".cache/tests")
            .join(crate::util::id());
        let core = Arc::new(Runtime::open(&root, None, None, false, true, false).unwrap());
        let credential: Credential = serde_json::from_value(json!({"origin":origin,"cookies":[{"name":"sid","value":"synthetic-old-session","domain":"127.0.0.1","path":"/"}]})).unwrap();
        let account = core
            .register_browser_credential("fixture", credential, "Fixture", "Synthetic")
            .unwrap();
        let running = core.clone();
        let id = account.id.clone();
        let target = format!("{origin}/current-context");
        let pending =
            tokio::spawn(async move { running.observe_account_session(&id, &target, false).await });
        tokio::time::timeout(std::time::Duration::from_secs(5), entered.notified())
            .await
            .unwrap();
        core.update_credential(&account.id, |saved| {
            saved.cookies[0].value = "synthetic-new-session".into();
            Ok(())
        })
        .unwrap();
        let before = serde_json::to_value(core.state.read().unwrap().sites.get("fixture")).unwrap();
        release.notify_one();
        let result = tokio::time::timeout(std::time::Duration::from_secs(5), pending)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(result.unwrap_err().public_code, "conflict");
        assert_eq!(
            serde_json::to_value(core.state.read().unwrap().sites.get("fixture")).unwrap(),
            before
        );
        server.abort();
        drop(core);
        std::fs::remove_dir_all(root).unwrap();
    }
}
