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
    pub(crate) fn fallback_credential(
        &self,
        account: &str,
        req_origin: &str,
    ) -> crate::accounts::Credential {
        if account.is_empty() {
            crate::accounts::Credential {
                origin: req_origin.to_owned(),
                ..Default::default()
            }
        } else {
            self.credential(account)
                .unwrap_or_else(|_| crate::accounts::Credential {
                    origin: req_origin.to_owned(),
                    ..Default::default()
                })
        }
    }

    pub(crate) async fn escalate_challenged_read(
        &self,
        req_input: &Request,
        r: Response,
        keep_unsuccessful_in_page: bool,
        accept_in_page: impl Fn(&Response) -> bool,
    ) -> Response {
        let req_origin = origin(&req_input.url).unwrap_or_default();
        let _ = self
            .policy
            .resume_auth(&format!("{req_origin}|{}", req_input.account_id));
        let credential = self.fallback_credential(&req_input.account_id, &req_origin);
        let mut fallback = r;
        if let Ok(in_page) = self
            .network
            .fetch_browser_in_page(&credential, req_input)
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
            if let Ok(rendered) = self
                .network
                .fetch_rendered(&credential, &req_input.url, &req_input.account_id)
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
        let parsed = crate::net::validate_url(url)?;
        if !policy::safe_read_url(&parsed) {
            return Err(AppError::api("approval_required", 409));
        }
        let mut headers = vec![];
        if !account.is_empty() {
            headers.extend(self.credential(account)?.request_headers(url)?);
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
            let credential = self.fallback_credential(account, &req_origin);
            if let Ok(in_page) = self
                .network
                .fetch_browser_in_page(&credential, &req_input)
                .await
            {
                if in_page.successful() {
                    return Ok(in_page);
                }
            }
            if let Ok(rendered) = self.network.fetch_rendered(&credential, url, account).await {
                if rendered.successful() {
                    return Ok(rendered);
                }
            }
        }
        let r = self.network.fetch(req_input.clone()).await?;
        if crate::defend::is_challenge_label(&r.defense) && !is_local_url {
            return Ok(self
                .escalate_challenged_read(&req_input, r, false, |_| true)
                .await);
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
