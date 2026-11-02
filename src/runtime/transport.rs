use crate::{
    apperr::{AppError, AppResult},
    kb::AttemptInput,
    net::{Request, Response},
    policy,
};
use serde_json::{Value, json};
use std::time::Duration;

use super::{Runtime, origin};

impl Runtime {
    pub(crate) async fn retry_read_after_browser(
        &self,
        browser: impl std::future::Future<Output = AppResult<Response>>,
        request: Request,
        allow_http_retry: bool,
        check_authority: impl Fn() -> AppResult<()> + Sync,
    ) -> AppResult<Response> {
        check_authority()?;
        match browser.await {
            Ok(response) => Ok(response),
            Err(_) if allow_http_retry => {
                check_authority()?;
                self.network
                    .fetch_guarded(request, None, &check_authority)
                    .await
            }
            Err(error) => Err(error),
        }
    }
    pub(crate) async fn escalate_challenged_read_snapshot(
        &self,
        req_input: &Request,
        r: Response,
        keep_unsuccessful_in_page: bool,
        snapshot: Option<&crate::accounts::Credential>,
        check_authority: impl Fn() -> AppResult<()> + Sync,
        accept_in_page: impl Fn(&Response) -> bool,
    ) -> Response {
        let req_origin = origin(&req_input.url).unwrap_or_default();
        let _ = self
            .policy
            .resume_auth(&format!("{req_origin}|{}", req_input.account_id));
        let credential = snapshot
            .cloned()
            .unwrap_or_else(|| crate::accounts::Credential {
                origin: req_origin.clone(),
                ..Default::default()
            });
        let mut fallback = r;
        if check_authority().is_err() {
            return fallback;
        }
        if let Ok(in_page) = self
            .network
            .fetch_browser_in_page_guarded(&credential, req_input, Some(&check_authority))
            .await
        {
            if in_page.successful() && accept_in_page(&in_page) {
                return in_page;
            }
            if keep_unsuccessful_in_page {
                fallback = in_page;
            }
        }
        if req_input.method == "GET" {
            if check_authority().is_err() {
                return fallback;
            }
            if let Ok(rendered) = self
                .network
                .fetch_rendered_guarded(
                    &credential,
                    &req_input.url,
                    &req_input.account_id,
                    Some(&check_authority),
                )
                .await
            {
                if rendered.successful() {
                    return rendered;
                }
            }
        }
        fallback
    }

    pub async fn read_page(&self, url: &str, account: &str) -> AppResult<Response> {
        self.read_page_profile(url, account, "go").await
    }

    pub async fn read_page_profile(
        &self,
        url: &str,
        account: &str,
        tls_profile: &str,
    ) -> AppResult<Response> {
        let credential = if account.is_empty() {
            None
        } else {
            Some(self.credential(account)?)
        };
        self.read_page_snapshot(url, account, tls_profile, credential.as_ref())
            .await
    }
    pub(crate) async fn read_page_for_job(
        &self,
        job: &crate::state::Job,
        url: &str,
        account: &str,
    ) -> AppResult<Response> {
        let credential = if account.is_empty() {
            None
        } else {
            Some(self.job_credential(job, account, url)?)
        };
        self.read_page_checked(url, account, "go", credential.as_ref(), || {
            if account.is_empty() {
                return Ok(());
            }
            let current = self.job_credential(job, account, url)?;
            if serde_json::to_vec(&current)?
                != serde_json::to_vec(
                    credential
                        .as_ref()
                        .ok_or_else(|| AppError::api("auth_required", 401))?,
                )?
            {
                return Err(AppError::api("auth_required", 401));
            }
            Ok(())
        })
        .await
    }
    pub(crate) async fn read_page_snapshot(
        &self,
        url: &str,
        account: &str,
        tls_profile: &str,
        snapshot: Option<&crate::accounts::Credential>,
    ) -> AppResult<Response> {
        self.read_page_checked(url, account, tls_profile, snapshot, || {
            if account.is_empty() {
                return Ok(());
            }
            let _guard = self.selection_lock()?;
            let current = self.credential(account)?;
            if serde_json::to_vec(&current)?
                != serde_json::to_vec(snapshot.ok_or_else(|| AppError::api("auth_required", 401))?)?
            {
                return Err(AppError::api("auth_required", 401));
            }
            Ok(())
        })
        .await
    }
    pub(crate) async fn read_page_checked(
        &self,
        url: &str,
        account: &str,
        tls_profile: &str,
        snapshot: Option<&crate::accounts::Credential>,
        check_authority: impl Fn() -> AppResult<()> + Sync,
    ) -> AppResult<Response> {
        let parsed = crate::net::validate_url(url)?;
        if !policy::safe_read_url(&parsed) {
            return Err(AppError::api("approval_required", 409));
        }
        let mut headers = vec![];
        if !account.is_empty() {
            headers.extend(
                snapshot
                    .ok_or_else(|| AppError::api("auth_required", 401))?
                    .request_headers(url)?,
            );
        }
        if policy::request_headers_effect(
            headers
                .iter()
                .map(|(name, value)| (name.as_str(), value.as_str())),
        ) != policy::Effect::Read
        {
            return Err(AppError::api("approval_required", 409));
        }
        let req_origin = parsed.origin().ascii_serialization();
        let is_local_url =
            url.starts_with("http://127.0.0.1") || url.starts_with("http://localhost");
        let req_input = Request {
            url: url.into(),
            method: "GET".into(),
            headers,
            body: None,
            account_id: account.into(),
            limit: 2 * 1024 * 1024,
            timeout: Duration::from_secs(30),
            read_only: true,
            follow_redirects: true,
            tls_profile: tls_profile.into(),
            multipart: None,
        };
        let direct_key = policy::Policy::strategy_key("GET", url, "http", account);
        if !is_local_url
            && self
                .policy
                .is_strategy_blocked(&direct_key)
                .ok()
                .flatten()
                .is_some()
        {
            let credential = snapshot
                .cloned()
                .unwrap_or_else(|| crate::accounts::Credential {
                    origin: req_origin.clone(),
                    ..Default::default()
                });
            check_authority()?;
            if let Ok(in_page) = self
                .network
                .fetch_browser_in_page_guarded(&credential, &req_input, Some(&check_authority))
                .await
            {
                check_authority()?;
                if in_page.successful() {
                    return Ok(in_page);
                }
            }
            check_authority()?;
            if let Ok(rendered) = self
                .network
                .fetch_rendered_guarded(&credential, url, account, Some(&check_authority))
                .await
            {
                check_authority()?;
                if rendered.successful() {
                    return Ok(rendered);
                }
            }
        }
        check_authority()?;
        let r = self
            .network
            .fetch_guarded(req_input.clone(), None, &check_authority)
            .await?;
        check_authority()?;
        if crate::defend::is_challenge_label(&r.defense) && !is_local_url {
            let recovered = self
                .escalate_challenged_read_snapshot(
                    &req_input,
                    r,
                    false,
                    snapshot,
                    &check_authority,
                    |_| true,
                )
                .await;
            check_authority()?;
            return Ok(recovered);
        }
        Ok(r)
    }

    pub async fn probe(&self, site: &str, url: &str, profile: &str) -> AppResult<(Value, i64)> {
        let r = self.read_page_profile(url, "", profile).await?;
        let id = self.kb.record_attempt(&AttemptInput {
            site: site.into(),
            stage: "access".into(),
            action: "probe".into(),
            status: r.status as i64,
            defense: r.defense.clone(),
            duration_ms: r.duration_ms,
            verdict: if r.successful() { "pass" } else { "fail" }.into(),
            ..Default::default()
        })?;
        Ok((
            json!({"url":r.url,"status":r.status,"defense":r.defense,"latency_ms":r.duration_ms,"content_type":r.header("content-type")}),
            id,
        ))
    }

    /// Launch a local browser probe and record the attempt in the knowledge base.
    pub async fn probe_browser_stealth(&self, site: &str, url: &str) -> AppResult<(Value, i64)> {
        let r = crate::browser::stealth_probe(url).await?;
        let verdict = if r.defense == "none" && (200..400).contains(&r.status) {
            "pass"
        } else if r.defense != "none" && !r.defense.is_empty() {
            "blocked"
        } else {
            "fail"
        };
        let id = self.kb.record_attempt(&AttemptInput {
            site: site.into(),
            stage: "access".into(),
            action: "probe".into(),
            verdict: verdict.into(),
            target_kind: "url".into(),
            target_key: url.into(),
            status: r.status as i64,
            defense: r.defense.clone(),
            duration_ms: r.latency_ms,
            tls_profile: "browser-stealth".into(),
            ..Default::default()
        })?;
        Ok((
            json!({"url":r.url,"status":r.status,"defense":r.defense,"signal":r.signal,
                   "latency_ms":r.latency_ms,"content_type":r.content_type,"tls_profile":"browser-stealth"}),
            id,
        ))
    }
}
