use crate::{
    accounts,
    apperr::{AppError, AppResult},
    kb::AttemptInput,
    net::Request,
    policy::{self, Approval, Effect, ExecutionRequest, ReviewInput},
    spec::Spec,
    util,
};
use serde_json::{Value, json};
use std::{collections::BTreeMap, time::Duration};

use super::{OpResult, Runtime, humanize, origin, params, same_site_url, sanitized, visible_text};

impl Runtime {
    pub fn request(
        &self,
        sp: &Spec,
        op: &str,
        inputs: BTreeMap<String, String>,
        account: Option<&str>,
    ) -> AppResult<ExecutionRequest> {
        let op = sp.op(op)?;
        let names = params(op);
        if inputs.len() > 100
            || inputs
                .iter()
                .any(|(k, v)| !names.iter().any(|(n, _)| *n == k) || v.len() > 32768)
        {
            return Err(AppError::api("bad_request", 400));
        }
        let (url, _) = sp.build_request(op, &inputs)?;
        let explicit_account = account.is_some_and(|value| !value.is_empty());
        let mut account = match account {
            Some(value) => value.to_owned(),
            None => self
                .state
                .site(&sp.site.name)?
                .map(|s| s.account_id)
                .unwrap_or_default(),
        };
        if !account.is_empty() {
            let a = self
                .state
                .account(&account)?
                .ok_or_else(|| AppError::api("auth_required", 401))?;
            if a.site_id != sp.site.name || !same_site_url(&a.site_url, &url)? {
                if explicit_account {
                    return Err(AppError::api("auth_required", 401));
                }
                account.clear();
            }
        }
        let (headers, _) = self.headers(sp, op, &url, &account)?;
        Ok(ExecutionRequest {
            site_id: sp.site.name.clone(),
            action_id: op.name.clone(),
            account_id: account,
            inputs,
            spec_hash: Self::spec_hash(sp)?,
            credential_hash: util::hash(&serde_json::to_vec(&headers)?),
        })
    }
    /// Websites start with writes off until enabled once for that CLI (`hycli writes <site> allow`, MCP `hycli_writes`, or dashboard toggle).
    pub fn ensure_writes(&self, site: &str) -> AppResult<()> {
        if self.state.site_writes(site)? {
            return Ok(());
        }
        Err(AppError::api("writes_disabled", 409))
    }
    pub fn set_writes(&self, site: &str, writes: bool) -> AppResult<()> {
        self.spec(site)?;
        self.state.update(|d| {
            let m = d.sites.entry(site.to_string()).or_default();
            m.writes = writes;
            m.updated_at = util::now();
            Ok(())
        })?;
        if !writes {
            self.policy.invalidate(Some(site), None)?;
            if let Ok(snapshot) = self.state.read() {
                for job in snapshot.jobs {
                    if job.site_id == site
                        && ["queued", "running"].contains(&job.status.as_str())
                        && job.is_write_action()
                    {
                        let _ = self.cancel_job(&job.id);
                    }
                }
            }
        }
        self.changed();
        Ok(())
    }
    pub fn approval(&self, request: ExecutionRequest, locale: &str) -> AppResult<Approval> {
        self.ensure_writes(&request.site_id)?;
        let site = self
            .sites(locale)?
            .into_iter()
            .find(|s| s.id == request.site_id)
            .ok_or_else(|| AppError::api("not_found", 404))?;
        let action = site
            .actions
            .iter()
            .find(|a| a.id == request.action_id)
            .ok_or_else(|| AppError::api("not_found", 404))?;
        let account = if request.account_id.is_empty() {
            String::new()
        } else {
            self.state
                .account(&request.account_id)?
                .map(|a| a.label)
                .unwrap_or_default()
        };
        let sp = self.spec(&request.site_id)?;
        let op = sp.op(&request.action_id)?;
        let inputs = params(op)
            .into_iter()
            .filter_map(|(k, p)| {
                let value = request
                    .inputs
                    .get(k)
                    .cloned()
                    .or_else(|| p.default.as_ref().map(util::plain))?;
                let label = action
                    .inputs
                    .iter()
                    .find(|i| i.id == *k)
                    .map(|i| i.label.clone())
                    .unwrap_or_else(|| humanize(k));
                Some(ReviewInput { label, value })
            })
            .collect();
        let review = self.policy.request_approval(
            request,
            site.title,
            action.title.clone(),
            account,
            inputs,
        )?;
        self.changed();
        Ok(review)
    }
    pub async fn run_op(
        &self,
        sp: &Spec,
        op: &str,
        args: &BTreeMap<String, String>,
    ) -> AppResult<(OpResult, i64)> {
        self.run_op_for_account(sp, op, args, None).await
    }
    pub async fn run_op_for_account(
        &self,
        sp: &Spec,
        op: &str,
        args: &BTreeMap<String, String>,
        account: Option<&str>,
    ) -> AppResult<(OpResult, i64)> {
        let op_def = sp.op(op)?;
        let effect = policy::operation_effect(op_def, args);
        if op_def.effect == Effect::Read && effect != Effect::Read {
            return Err(AppError::api("approval_required", 409));
        }
        if effect != Effect::Read {
            self.ensure_writes(&sp.site.name)?;
        }
        let request = self.request(sp, op, args.clone(), account)?;
        if self.spec(&sp.site.name).is_ok() {
            self.execute_observed(request, &self.state.settings()?.locale)
                .await
        } else {
            self.execute_definition(sp, request, None, false, None)
                .await
        }
    }
    /// Approval IDs are validated when supplied by the dashboard handler; direct CLI/MCP calls execute when writes are enabled.
    pub(crate) async fn execute(
        &self,
        request: ExecutionRequest,
        approval: Option<&str>,
    ) -> AppResult<(OpResult, i64)> {
        self.execute_for_job(request, approval, None).await
    }
    pub(crate) async fn execute_for_job(
        &self,
        request: ExecutionRequest,
        approval: Option<&str>,
        job_id: Option<&str>,
    ) -> AppResult<(OpResult, i64)> {
        let sp = self.spec(&request.site_id)?;
        self.execute_definition(&sp, request, approval, true, job_id)
            .await
    }
    /// Verify a proposed read without replacing the user's installed definition.
    pub async fn verify_candidate_with_inputs(
        &self,
        sp: &Spec,
        action: &str,
        account: &str,
        inputs: BTreeMap<String, String>,
    ) -> AppResult<(OpResult, i64)> {
        policy::validate_spec(sp)?;
        let op = sp.op(action)?;
        if policy::operation_effect(op, &inputs) != Effect::Read {
            return Err(AppError::api("approval_required", 409));
        }
        let request = self.request(sp, action, inputs, Some(account))?;
        self.execute_definition(sp, request, None, false, None)
            .await
    }
    async fn execute_definition(
        &self,
        sp: &Spec,
        request: ExecutionRequest,
        approval: Option<&str>,
        installed: bool,
        job_id: Option<&str>,
    ) -> AppResult<(OpResult, i64)> {
        let op = sp.op(&request.action_id)?;
        if Self::spec_hash(sp)? != request.spec_hash {
            return Err(AppError::api("approval_expired", 409));
        }
        let mut checked = self.request(
            sp,
            &request.action_id,
            request.inputs.clone(),
            Some(&request.account_id),
        )?;
        let mut effect = policy::operation_effect(op, &request.inputs);
        let (mut url, mut body) = sp.build_request(op, &request.inputs)?;
        if origin(&url)? != origin(sp.operation_base(op))? {
            return Err(AppError::api("unsafe_target", 400));
        }
        let (mut headers, mut secrets) = self.headers(sp, op, &url, &request.account_id)?;
        checked.credential_hash = util::hash(&serde_json::to_vec(&headers)?);
        if effect != Effect::Read || approval.is_some() {
            if op.effect == Effect::Read && approval.is_none() {
                return Err(AppError::api("approval_required", 409));
            }
            self.ensure_writes(&sp.site.name)?;
            if let Some(approval_id) = approval {
                self.policy.verify_consumable(approval_id, &checked)?;
            }
        }
        if !op.preflight.is_empty() {
            let mut effective_inputs = request.inputs.clone();
            self.run_preflight_steps(
                sp,
                op,
                &request.account_id,
                &mut effective_inputs,
                &mut secrets,
            )
            .await?;
            let post_effect = policy::operation_effect(op, &effective_inputs);
            if post_effect != Effect::Read {
                if effect == Effect::Read || (op.effect == Effect::Read && approval.is_none()) {
                    return Err(AppError::api("approval_required", 409));
                }
                effect = post_effect;
                self.ensure_writes(&sp.site.name)?;
            }
            let (rebuilt_url, rebuilt_body) = sp.build_request(op, &effective_inputs)?;
            if origin(&rebuilt_url)? != origin(sp.operation_base(op))? {
                return Err(AppError::api("unsafe_target", 400));
            }
            url = rebuilt_url;
            body = rebuilt_body;
            let empty_resolved = BTreeMap::new();
            for (_, v) in &mut headers {
                if v.contains("{{") {
                    *v = crate::spec::interpolate_template_str(
                        v,
                        &empty_resolved,
                        &effective_inputs,
                    );
                }
            }
        }
        if effect != Effect::Read || approval.is_some() {
            self.ensure_writes(&sp.site.name)?;
            if let Some(approval_id) = approval {
                self.policy.consume(approval_id, &checked)?;
            }
            if let Some(id) = job_id {
                self.state.mark_job_dispatched(id)?;
            }
        }
        let req_origin = origin(&url)?;
        let is_local_url =
            url.starts_with("http://127.0.0.1") || url.starts_with("http://localhost");
        let req_input = Request {
            url: url.clone(),
            method: op.method.clone(),
            headers,
            body,
            account_id: request.account_id.clone(),
            limit: 8 * 1024 * 1024,
            timeout: Duration::from_secs(45),
            read_only: effect == Effect::Read,
            follow_redirects: effect == Effect::Read,
            tls_profile: sp.defaults.tls_profile.clone(),
        };
        let direct_key =
            policy::Policy::strategy_key(&op.method, &url, "http", &request.account_id);
        let prefer_browser_fetch = op.transport == "browser-fetch"
            || (!is_local_url
                && op.transport.is_empty()
                && (self
                    .policy
                    .is_strategy_blocked(&direct_key)
                    .ok()
                    .flatten()
                    .is_some()
                    || self
                        .policy
                        .preferred_transport(&req_origin)
                        .ok()
                        .flatten()
                        .as_deref()
                        == Some("browser-fetch")));
        let response = if op.transport == "browser" {
            let credential = if request.account_id.is_empty() {
                crate::accounts::Credential {
                    origin: req_origin,
                    ..Default::default()
                }
            } else {
                self.credential(&request.account_id)?
            };
            self.network
                .fetch_rendered(&credential, &url, &request.account_id)
                .await
        } else if prefer_browser_fetch {
            let credential = self.fallback_credential(&request.account_id, &req_origin);
            match self
                .network
                .fetch_browser_in_page(&credential, &req_input)
                .await
            {
                Ok(r) => Ok(r),
                Err(_) if op.transport != "browser-fetch" && effect == Effect::Read => {
                    self.network.fetch(req_input).await
                }
                Err(e) => Err(e),
            }
        } else {
            let fetched = self.network.fetch(req_input.clone()).await;
            match fetched {
                Ok(r)
                    if effect == Effect::Read
                        && crate::defend::is_challenge_label(&r.defense)
                        && !op.matches_response(&r)
                        && !is_local_url =>
                {
                    Ok(self
                        .escalate_challenged_read(&req_input, r, true, |resp| {
                            op.matches_response(resp)
                        })
                        .await)
                }
                other => other,
            }
        };
        let response = match response {
            Ok(r) => r,
            Err(e) => {
                let _ = self.kb.record_attempt(&AttemptInput {
                    site: sp.site.name.clone(),
                    stage: if installed { "use" } else { "verify" }.into(),
                    action: op.name.clone(),
                    verdict: "error".into(),
                    ..Default::default()
                });
                let pre_dispatch_err = matches!(
                    e.public_code.as_str(),
                    "unsafe_target"
                        | "rate_limited"
                        | "blocked_by_site"
                        | "data_invalid"
                        | "auth_required"
                );
                return Err(if effect != Effect::Read && !pre_dispatch_err {
                    AppError::api("action_outcome_unknown", 502)
                } else {
                    e
                });
            }
        };
        let parsed_json = op.parse_json_response(&response);
        let response_shape = op.matches_response(&response);
        let query_error = parsed_json.as_ref().is_some_and(|body| {
            body.get("errors")
                .and_then(Value::as_array)
                .is_some_and(|errors| !errors.is_empty())
        }) && op.graphql_query(&request.inputs);
        let matches_expected = response_shape && !query_error;
        let attempt = self.kb.record_attempt(&AttemptInput {
            site: sp.site.name.clone(),
            stage: if installed { "use" } else { "verify" }.into(),
            action: op.name.clone(),
            target_kind: "operation".into(),
            target_key: op.name.clone(),
            status: response.status as i64,
            duration_ms: response.duration_ms,
            defense: response.defense.clone(),
            verdict: if matches_expected { "pass" } else { "fail" }.into(),
            ..Default::default()
        })?;
        let extracted = op
            .response
            .as_ref()
            .and_then(|response| response.html.as_ref())
            .and_then(|extraction| {
                extraction.extract(&String::from_utf8_lossy(&response.bytes), &response.url)
            });
        let raw = extracted.or(parsed_json).unwrap_or_else(
            || json!({"text":visible_text(&String::from_utf8_lossy(&response.bytes))}),
        );
        let clean = sanitized(raw.clone(), &secrets);
        let mapped = if op.model.is_empty() {
            None
        } else {
            let model_bytes = serde_json::to_vec(&raw).unwrap_or_else(|_| response.bytes.clone());
            Some(sanitized(
                sp.apply_model(&op.model, &model_bytes)?,
                &secrets,
            ))
        };
        self.state.update(|d| {
            if installed {
                let m = d.sites.entry(sp.site.name.clone()).or_default();
                m.last_used = util::now();
                if effect == Effect::Read {
                    m.verified.insert(op.name.clone(), matches_expected);
                }
            }
            if let Some(a) = d.accounts.get_mut(&request.account_id) {
                a.checked_at = util::now();
                if response.status == 401 {
                    a.status = "expired".into();
                } else if response.successful()
                    && accounts::identity_endpoint(
                        &url::Url::parse(&response.url)
                            .map_err(|_| AppError::api("bad_url", 400))?,
                    )
                {
                    if let Some(identity) = accounts::extract_identity(&raw) {
                        accounts::apply_identity(a, identity);
                    }
                }
            }
            Ok(())
        })?;
        self.changed();
        if response.successful() && !response_shape {
            return Err(AppError::api("response_mismatch", 502));
        }
        Ok((
            OpResult {
                site: sp.site.name.clone(),
                op: request.action_id,
                status: response.status,
                latency_ms: response.duration_ms,
                body: Some(clean),
                mapped,
                defense: response.defense,
                signal: if query_error {
                    if effect == Effect::Read {
                        "query_failed"
                    } else {
                        "action_outcome_unknown"
                    }
                    .into()
                } else {
                    String::new()
                },
            },
            attempt,
        ))
    }
}
