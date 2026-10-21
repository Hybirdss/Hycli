use crate::{
    apperr::{AppError, AppResult},
    kb::AttemptInput,
    net::Request,
    policy::{self, Approval, Effect, ExecutionRequest, ReviewInput},
    spec::Spec,
    util,
};
use serde_json::{Value, json};
use std::{collections::BTreeMap, time::Duration};

use super::{OpResult, Runtime, humanize, origin, params, sanitized, visible_text};

#[derive(Clone, Copy, PartialEq, Eq)]
enum ExecutionPurpose {
    Run,
    Verify,
}

struct PreparedOutcome {
    result: OpResult,
    artifact: Option<crate::artifacts::PublishedArtifact>,
    dispatched_write: bool,
}

impl PreparedOutcome {
    fn buffered(result: OpResult, dispatched_write: bool) -> Self {
        Self {
            result,
            artifact: None,
            dispatched_write,
        }
    }
}

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
        let mut wire_inputs = inputs.clone();
        if let Some(p) = &op.pagination {
            wire_inputs.remove(&p.input);
        }
        let (request_url, _) = sp.build_request(op, &wire_inputs)?;
        let url = if op.derived_target {
            sp.site.base_url.clone()
        } else {
            request_url
        };
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
            if self
                .selected_credential(&sp.site.name, &url, &account)
                .is_err()
            {
                if explicit_account
                    || self
                        .state
                        .site(&sp.site.name)?
                        .is_some_and(|m| m.account_binding.is_some())
                {
                    return Err(AppError::api("auth_required", 401));
                }
                account.clear();
            }
        }
        super::oauth::validate_operation(sp, op, &inputs, &account)?;
        let (headers, secrets) = self.headers(sp, op, &url, &account)?;
        let request = ExecutionRequest {
            site_id: sp.site.name.clone(),
            action_id: op.name.clone(),
            account_id: account,
            inputs,
            spec_hash: Self::spec_hash(sp)?,
            credential_hash: self
                .oauth_credential_hash(sp, op)?
                .unwrap_or(util::hash(&serde_json::to_vec(&headers)?)),
        };
        // Validate opaque capability before any dashboard job persists its inputs.
        self.resolve_cursor(&request, op, &url, &secrets)?;
        Ok(request)
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
            let (outcome, attempt) = Box::pin(self.execute_definition(
                sp,
                request,
                None,
                false,
                None,
                ExecutionPurpose::Run,
            ))
            .await?;
            self.finish_prepared(None, outcome, attempt)
        }
    }
    /// Approval IDs are validated when supplied by the dashboard handler; direct CLI/MCP calls execute when writes are enabled.
    #[cfg(test)]
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
        let (outcome, attempt) = Box::pin(self.execute_definition(
            &sp,
            request,
            approval,
            true,
            job_id,
            ExecutionPurpose::Run,
        ))
        .await?;
        self.finish_prepared(job_id, outcome, attempt)
    }
    pub(crate) async fn verify_installed_read(
        &self,
        request: ExecutionRequest,
    ) -> AppResult<(OpResult, i64)> {
        let sp = self.spec(&request.site_id)?;
        let op = sp.op(&request.action_id)?;
        if policy::operation_effect(op, &request.inputs) != Effect::Read {
            return Err(AppError::api("approval_required", 409));
        }
        let (outcome, attempt) = Box::pin(self.execute_definition(
            &sp,
            request,
            None,
            true,
            None,
            ExecutionPurpose::Verify,
        ))
        .await?;
        self.finish_prepared(None, outcome, attempt)
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
        let (outcome, attempt) = Box::pin(self.execute_definition(
            sp,
            request,
            None,
            false,
            None,
            ExecutionPurpose::Verify,
        ))
        .await?;
        self.finish_prepared(None, outcome, attempt)
    }
    fn finish_prepared(
        &self,
        job_id: Option<&str>,
        mut outcome: PreparedOutcome,
        attempt: i64,
    ) -> AppResult<(OpResult, i64)> {
        let map_error = |error| {
            if outcome.dispatched_write {
                AppError::api("action_outcome_unknown", 502)
            } else {
                error
            }
        };
        if let Some(id) = job_id {
            outcome.result.ensure_success()?;
            let completed = self
                .state
                .complete_action(
                    id,
                    outcome.result.clone().output(),
                    outcome.result.artifact.clone(),
                )
                .map_err(map_error)?;
            if !completed {
                return Err(map_error(AppError::api("cancelled", 409)));
            }
        }
        if let Some(artifact) = outcome.artifact.take() {
            outcome.result.artifact = Some(artifact.persisted());
        }
        self.changed();
        Ok((outcome.result, attempt))
    }

    // Callers heap-own this large operation state so it is not embedded in every
    // outer CLI/MCP future. The box stays in the same task and drops with its owner.
    async fn execute_definition(
        &self,
        sp: &Spec,
        request: ExecutionRequest,
        approval: Option<&str>,
        installed: bool,
        job_id: Option<&str>,
        purpose: ExecutionPurpose,
    ) -> AppResult<(PreparedOutcome, i64)> {
        let op = sp.op(&request.action_id)?;
        let file_response = op
            .response
            .as_ref()
            .is_some_and(|expected| expected.format == "file");
        if file_response {
            policy::validate_spec(sp)?;
        }
        if Self::spec_hash(sp)? != request.spec_hash {
            return Err(AppError::api("approval_expired", 409));
        }
        let mut checked = self.request(
            sp,
            &request.action_id,
            request.inputs.clone(),
            Some(&request.account_id),
        )?;
        let mut wire_inputs = request.inputs.clone();
        if let Some(p) = &op.pagination {
            wire_inputs.remove(&p.input);
        }
        let (mut url, mut body) = sp.build_request(op, &wire_inputs)?;
        let source_url = if op.derived_target {
            sp.site.base_url.clone()
        } else {
            url.clone()
        };
        let mut multipart = if op.body.as_ref().is_some_and(|b| !b.multipart.is_empty()) {
            Some(sp.build_multipart(op, &wire_inputs)?)
        } else {
            None
        };
        if multipart.is_some() && ["browser", "browser-fetch"].contains(&op.transport.as_str()) {
            return Err(AppError::api("unsupported_transport", 400));
        }
        let mut effect =
            policy::rendered_request_effect(op, &request.inputs, &url, body.as_deref());
        if !op.derived_target && origin(&url)? != origin(sp.operation_base(op))? {
            return Err(AppError::api("unsafe_target", 400));
        }
        super::oauth::validate_operation(sp, op, &request.inputs, &request.account_id)?;
        let (mut headers, mut secrets, credential_snapshot) =
            self.headers_snapshot(sp, op, &source_url, &request.account_id)?;
        let mut source_secrets = credential_snapshot
            .as_ref()
            .map(|credential| credential.redaction_values())
            .unwrap_or_default();
        for (name, value) in &headers {
            if let Some(auth) = sp
                .find_auth(&op.auth)
                .filter(|auth| name.eq_ignore_ascii_case(&auth.header))
            {
                source_secrets.push(value.clone());
                if let Some(token) = value.strip_prefix(&auth.prefix) {
                    source_secrets.push(token.to_owned());
                }
            }
            if !value.contains("{{")
                && !["accept", "content-type", "user-agent", "content-md5"]
                    .iter()
                    .any(|public| name.eq_ignore_ascii_case(public))
            {
                source_secrets.push(value.clone());
                if name.eq_ignore_ascii_case("authorization") {
                    if let Some((_, token)) = value.split_once(' ') {
                        source_secrets.push(token.to_owned());
                    }
                }
            }
        }
        let source_header_hash = util::hash(&serde_json::to_vec(&headers)?);
        let mut destination_inputs = BTreeMap::new();
        effect = effect.combine(policy::request_headers_effect(
            headers
                .iter()
                .map(|(name, value)| (name.as_str(), value.as_str())),
        ));
        checked.credential_hash = self
            .oauth_credential_hash(sp, op)?
            .unwrap_or(util::hash(&serde_json::to_vec(&headers)?));
        let cursor = self.resolve_cursor(&checked, op, &url, &secrets)?;
        if let Some(value) = &cursor {
            if super::pagination::continuation_effect(op, &request.inputs, &url, value)
                != Effect::Read
            {
                return Err(AppError::api("unsafe_target", 400));
            }
            secrets.push(value.clone());
        }
        if effect != Effect::Read || approval.is_some() {
            if op.effect == Effect::Read && approval.is_none() {
                return Err(AppError::api("approval_required", 409));
            }
            self.ensure_writes(&sp.site.name)?;
            if let Some(approval_id) = approval {
                self.policy.verify_consumable(approval_id, &checked)?;
            }
        }
        let source_selection = self.state.site(&sp.site.name)?.map(|site| {
            (
                site.account_selection_revision,
                site.account_id,
                site.account_binding,
            )
        });
        let check_source_authority = || {
            let current_selection = self.state.site(&sp.site.name)?.map(|site| {
                (
                    site.account_selection_revision,
                    site.account_id,
                    site.account_binding,
                )
            });
            if current_selection != source_selection {
                return Err(AppError::api("auth_required", 401));
            }
            self.ensure_credential_snapshot(
                &sp.site.name,
                &source_url,
                &request.account_id,
                credential_snapshot.as_ref(),
            )?;
            let (current, _, _) =
                self.headers_snapshot(sp, op, &source_url, &request.account_id)?;
            if util::hash(&serde_json::to_vec(&current)?) != source_header_hash {
                return Err(AppError::api("auth_required", 401));
            }
            Ok(())
        };
        let mut effective_inputs = request.inputs.clone();
        if !op.preflight.is_empty() {
            destination_inputs = self
                .run_preflight_steps(
                    sp,
                    op,
                    &request.account_id,
                    credential_snapshot.as_ref(),
                    &mut effective_inputs,
                    &mut secrets,
                    &check_source_authority,
                )
                .await?;
            for step in &op.preflight {
                for (field, expression) in &step.extract {
                    if super::preflight::source_credential_expression(expression) {
                        if let Some(value) =
                            destination_inputs.get(&format!("step.{}.{field}", step.name))
                        {
                            source_secrets.push(value.clone());
                        }
                    }
                }
            }
            let (rebuilt_url, rebuilt_body) = sp.build_final_request(op, &effective_inputs)?;
            let post_effect = policy::rendered_request_effect(
                op,
                &effective_inputs,
                &rebuilt_url,
                rebuilt_body.as_deref(),
            );
            if post_effect != Effect::Read {
                if effect == Effect::Read || (op.effect == Effect::Read && approval.is_none()) {
                    return Err(AppError::api("approval_required", 409));
                }
                effect = effect.combine(post_effect);
                self.ensure_writes(&sp.site.name)?;
            }
            if !op.derived_target && origin(&rebuilt_url)? != origin(sp.operation_base(op))? {
                return Err(AppError::api("unsafe_target", 400));
            }
            url = rebuilt_url;
            body = rebuilt_body;
            if op.body.as_ref().is_some_and(|b| !b.multipart.is_empty()) {
                let mut multipart_inputs = effective_inputs.clone();
                for (name, param) in &op.body.as_ref().expect("multipart body").multipart {
                    if param.kind == "file" {
                        multipart_inputs.remove(name);
                        if let Some(path) = request.inputs.get(name) {
                            multipart_inputs.insert(name.clone(), path.clone());
                        }
                    }
                }
                multipart = Some(sp.build_multipart(op, &multipart_inputs)?);
            }
            let empty_resolved = BTreeMap::new();
            let mut credential_headers: Vec<String> = credential_snapshot
                .as_ref()
                .map(|credential| credential.request_headers(&source_url))
                .transpose()?
                .unwrap_or_default()
                .into_iter()
                .map(|(name, _)| name)
                .collect();
            if let Some(auth) = sp
                .find_auth(&op.auth)
                .filter(|auth| matches!(auth.kind.as_str(), "header" | "derived"))
            {
                credential_headers.push(auth.header.clone());
            }
            for (name, value) in &mut headers {
                if credential_headers
                    .iter()
                    .any(|private| name.eq_ignore_ascii_case(private))
                {
                    continue;
                }
                if let Some(template) = op
                    .headers
                    .get(name)
                    .or_else(|| sp.defaults.headers.get(name))
                    .filter(|template| template.contains("{{"))
                {
                    *value = crate::spec::interpolate_template_str(
                        template,
                        &empty_resolved,
                        &effective_inputs,
                    );
                }
            }
        }
        if op.preflight.is_empty() {
            (url, body) = sp.build_final_request(op, &wire_inputs)?;
        }
        if origin(&url)? != origin(&source_url)? {
            super::preflight::validate_destination_url(
                sp,
                op,
                &destination_inputs,
                &source_secrets,
                &url,
            )?;
            super::preflight::validate_generated_request(
                op,
                &request.inputs,
                &effective_inputs,
                &destination_inputs,
                &source_secrets,
            )?;
            headers =
                super::preflight::destination_headers(op, &destination_inputs, &source_secrets)?;
        }
        check_source_authority()?;
        // A derived_target op is the one place a request may leave the site's
        // declared origin. Validate the final URL here so it is covered whether
        // or not a preflight ran: require https (loopback for local tests) so a
        // file is never uploaded in cleartext.
        if op.derived_target {
            let parsed = crate::net::validate_url(&url)?;
            let host = parsed.host_str().unwrap_or_default();
            let local = host == "127.0.0.1" || host == "localhost" || host == "::1";
            if parsed.scheme() != "https" && !local {
                return Err(AppError::api("unsafe_target", 400));
            }
        }
        effect = effect.combine(policy::request_headers_effect(
            headers
                .iter()
                .map(|(name, value)| (name.as_str(), value.as_str())),
        ));
        if effect != Effect::Read || approval.is_some() {
            self.ensure_writes(&sp.site.name)?;
            if let Some(approval_id) = approval {
                self.policy.consume(approval_id, &checked)?;
            }
            if let Some(id) = job_id {
                self.state.mark_job_dispatched(id)?;
            }
        }
        self.ensure_credential_snapshot(
            &sp.site.name,
            &source_url,
            &request.account_id,
            credential_snapshot.as_ref(),
        )?;
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
            multipart,
        };
        let direct_key =
            policy::Policy::strategy_key(&op.method, &url, "http", &request.account_id);
        let prefer_browser_fetch = !file_response
            && (op.transport == "browser-fetch"
                || (req_input.multipart.is_none()
                    && !op.derived_target
                    && !is_local_url
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
                            == Some("browser-fetch"))));
        let received = if file_response {
            let receiver = crate::net::FileReceiver::new(&self.root);
            if sp
                .find_auth(&op.auth)
                .is_some_and(|auth| auth.oauth_refresh.is_some())
            {
                self.fetch_oauth_operation(
                    sp,
                    op,
                    req_input,
                    None,
                    &mut secrets,
                    &checked.credential_hash,
                    Some(&receiver),
                )
                .await
            } else {
                self.network
                    .fetch_file_guarded(req_input, &check_source_authority, &receiver)
                    .await
            }
        } else {
            let response = if sp
                .find_auth(&op.auth)
                .is_some_and(|auth| auth.oauth_refresh.is_some())
            {
                self.fetch_oauth_operation(
                    sp,
                    op,
                    req_input,
                    op.pagination
                        .as_ref()
                        .and_then(|p| cursor.as_deref().map(|v| (p.query.as_str(), v))),
                    &mut secrets,
                    &checked.credential_hash,
                    None,
                )
                .await
                .map(|received| received.response)
            } else if let Some(p) = &op.pagination {
                self.network
                    .fetch_paginated(req_input, cursor.as_deref().map(|v| (p.query.as_str(), v)))
                    .await
            } else if op.transport == "browser" {
                let credential = if request.account_id.is_empty() {
                    crate::accounts::Credential {
                        origin: req_origin,
                        ..Default::default()
                    }
                } else {
                    credential_snapshot
                        .clone()
                        .ok_or_else(|| AppError::api("auth_required", 401))?
                };
                self.network
                    .fetch_rendered(&credential, &url, &request.account_id)
                    .await
            } else if prefer_browser_fetch {
                let credential =
                    credential_snapshot
                        .clone()
                        .unwrap_or_else(|| crate::accounts::Credential {
                            origin: req_origin.clone(),
                            ..Default::default()
                        });
                self.retry_read_after_browser(
                    self.network.fetch_browser_in_page(&credential, &req_input),
                    req_input.clone(),
                    op.transport != "browser-fetch" && effect == Effect::Read,
                    || {
                        self.ensure_credential_snapshot(
                            &sp.site.name,
                            &url,
                            &request.account_id,
                            credential_snapshot.as_ref(),
                        )
                    },
                )
                .await
            } else {
                let fetched = self
                    .network
                    .fetch_guarded(req_input.clone(), None, &check_source_authority)
                    .await;
                match fetched {
                    Ok(r)
                        if effect == Effect::Read
                            && crate::defend::is_challenge_label(&r.defense)
                            && !op.matches_response(&r)
                            && !is_local_url
                            && !file_response =>
                    {
                        self.ensure_credential_snapshot(
                            &sp.site.name,
                            &url,
                            &request.account_id,
                            credential_snapshot.as_ref(),
                        )?;
                        Ok(self
                            .escalate_challenged_read_snapshot(
                                &req_input,
                                r,
                                true,
                                credential_snapshot.as_ref(),
                                || {
                                    self.ensure_credential_snapshot(
                                        &sp.site.name,
                                        &url,
                                        &request.account_id,
                                        credential_snapshot.as_ref(),
                                    )
                                },
                                |resp| op.matches_response(resp),
                            )
                            .await)
                    }
                    other => other,
                }
            };
            response.map(crate::net::FileResponse::buffered)
        };
        let mut received = match received {
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
                        // Network maps lease loss after any write dispatch to
                        // action_outcome_unknown; this label is pre-dispatch.
                        | "request_lease_lost"
                );
                return Err(if effect != Effect::Read && !pre_dispatch_err {
                    AppError::api("action_outcome_unknown", 502)
                } else {
                    e
                });
            }
        };
        let response = &received.response;
        let parsed_json = if file_response {
            None
        } else {
            op.parse_json_response(&response)
        };
        let response_shape = if file_response {
            op.matches_file_response(&received)
        } else {
            op.matches_response(response)
        };
        let query_error = parsed_json.as_ref().is_some_and(|body| {
            body.get("errors")
                .and_then(Value::as_array)
                .is_some_and(|errors| !errors.is_empty())
        }) && op.graphql_query(&request.inputs);
        let matches_expected = response_shape && !query_error;
        // Once a mutating endpoint has succeeded, local bookkeeping or artifact
        // publication failure must not invite an unsafe replay of that mutation.
        let dispatched_file_write =
            file_response && response.successful() && effect != Effect::Read;
        let file_completion_error = |error| {
            if dispatched_file_write {
                AppError::api("action_outcome_unknown", 502)
            } else {
                error
            }
        };
        let attempt = self
            .kb
            .record_attempt(&AttemptInput {
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
            })
            .map_err(file_completion_error)?;
        if file_response {
            return (|| {
                super::failure::ensure_success(
                    response.status, &response.defense, "",
                    Some(crate::apperr::SafeHttpDiagnostic::from_response(response.status, &response.defense, response.json().as_ref(), &secrets)),
                )?;
                if !matches_expected { return Err(AppError::api("response_mismatch", 502)); }
                let check_authority = || -> AppResult<()> {
                    let current_selection = self.state.site(&sp.site.name)?.map(|site| (site.account_selection_revision, site.account_id.clone(), site.account_binding.clone()));
                    if current_selection != source_selection { return Err(AppError::api("auth_required", 401)); }
                    self.ensure_credential_snapshot(&sp.site.name, &source_url, &request.account_id, credential_snapshot.as_ref())?;
                    let current = self.request(sp, &request.action_id, request.inputs.clone(), Some(&request.account_id))?;
                    if current.credential_hash != checked.credential_hash { return Err(AppError::api("auth_required", 401)); }
                    if installed && Self::spec_hash(&self.spec(&sp.site.name)?)? != request.spec_hash { return Err(AppError::api("conflict", 409)); }
                    if let Some(id) = job_id {
                        if self.state.job(id)?.status != "running" { return Err(AppError::api("cancelled", 409)); }
                    }
                    Ok(())
                };
                check_authority()?;
                self.state.update(|data| {
                    if installed {
                        let meta = data.sites.entry(sp.site.name.clone()).or_default();
                        meta.last_used = util::now();
                        if effect == Effect::Read { meta.verified.insert(op.name.clone(), true); }
                    }
                    Ok(())
                })?;
                let sealed = received.artifact.take().ok_or_else(|| AppError::api("response_mismatch", 502))?;
                let (output, artifact) = if purpose == ExecutionPurpose::Run {
                    let filename = crate::artifacts::response_filename(response.header("content-disposition"));
                    let metadata = sanitized(json!({"filename": filename}), &secrets);
                    let media_type = response.header("content-type").split(';').next().unwrap_or("").trim().to_ascii_lowercase();
                    check_authority()?;
                    let published = sealed.publish(metadata["filename"].as_str().unwrap_or("download.bin"), &media_type,
                        crate::artifacts::Provenance { site: sp.site.name.clone(), action: op.name.clone(), source_origin: origin(&source_url)? })?;
                    (None, Some(published))
                } else {
                    (Some(json!({"kind":"file_verification", "status":"verified", "byte_count":sealed.byte_count(), "persisted":false, "content_trust":"untrusted", "inspection":"not_inspected"})), None)
                };
                let result = OpResult {
                    artifact: artifact.as_ref().map(|published| published.manifest().clone()),
                    diagnostic: None, paginated: false, next_cursor: None,
                    site: sp.site.name.clone(), op: request.action_id, status: response.status,
                    latency_ms: response.duration_ms, body: output, mapped: None,
                    defense: response.defense.clone(), signal: String::new(),
                };
                Ok((PreparedOutcome { result, artifact, dispatched_write: dispatched_file_write }, attempt))
            })().map_err(file_completion_error);
        }
        let response = received.response;
        let extracted = op
            .response
            .as_ref()
            .and_then(|response| response.html.as_ref())
            .and_then(|extraction| {
                extraction.extract(&String::from_utf8_lossy(&response.bytes), &response.url)
            });
        let mut raw = extracted.or(parsed_json).unwrap_or_else(
            || json!({"text":visible_text(&String::from_utf8_lossy(&response.bytes))}),
        );
        let mut private_cursors = Vec::new();
        if let Some(value) = &cursor {
            private_cursors.push(value.clone());
        }
        if let Some(value) = op
            .pagination
            .as_ref()
            .and_then(|p| raw.pointer(&p.next))
            .and_then(Value::as_str)
        {
            private_cursors.push(value.into());
        }
        let next_cursor = if response.successful() && !query_error {
            self.capture_cursor(&checked, op, &url, &mut raw, &mut secrets, matches_expected)?
        } else {
            None
        };
        if response.successful() && !query_error {
            super::pagination::scrub(&mut raw, &private_cursors)?;
        }
        secrets.extend(private_cursors.iter().cloned());
        let clean = sanitized(raw.clone(), &secrets);
        let mapped = if op.model.is_empty() || !response.successful() || query_error {
            None
        } else {
            let model_bytes = serde_json::to_vec(&raw)?;
            let mut mapped = sp.apply_model(&op.model, &model_bytes)?;
            super::pagination::scrub(&mut mapped, &private_cursors)?;
            Some(sanitized(mapped, &secrets))
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
                }
            }
            Ok(())
        })?;
        self.changed();
        if response.successful() && !response_shape {
            return Err(AppError::api("response_mismatch", 502));
        }
        let dispatched_write = effect != Effect::Read && response.successful();
        Ok((
            PreparedOutcome::buffered(
                OpResult {
                    artifact: None,
                    diagnostic: Some(crate::apperr::SafeHttpDiagnostic::from_response(
                        response.status,
                        &response.defense,
                        response.json().as_ref(),
                        &secrets,
                    )),
                    paginated: op.pagination.is_some(),
                    next_cursor,
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
                dispatched_write,
            ),
            attempt,
        ))
    }
}

#[cfg(test)]
mod stack_ownership_tests {
    use super::*;

    #[test]
    fn action_adapters_do_not_embed_the_operation_state_machine() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join(".cache/action-future")
            .join(util::id());
        let core = Runtime::open(&root, None, None, false, true, false).unwrap();
        let sp = crate::spec::parse(
            br#"{"site":{"name":"future-fixture","base_url":"https://example.test"},"operations":[{"name":"read","method":"GET","path":"/items","effect":"read"}]}"#,
        )
        .unwrap();
        let request = core
            .request(&sp, "read", BTreeMap::new(), Some(""))
            .unwrap();
        let inputs = BTreeMap::new();
        {
            let operation = core.execute_definition(
                &sp,
                request.clone(),
                None,
                false,
                None,
                ExecutionPurpose::Run,
            );
            let installed = core.execute_for_job(request.clone(), None, None);
            let check = core.verify_installed_read(request);
            let direct = core.run_op(&sp, "read", &inputs);
            let candidate = core.verify_candidate_with_inputs(&sp, "read", "", BTreeMap::new());
            let operation_bytes = std::mem::size_of_val(&operation);
            for adapter_bytes in [
                std::mem::size_of_val(&installed),
                std::mem::size_of_val(&check),
                std::mem::size_of_val(&direct),
                std::mem::size_of_val(&candidate),
            ] {
                assert!(
                    adapter_bytes < operation_bytes,
                    "adapter embeds the operation state: {adapter_bytes} >= {operation_bytes}"
                );
            }
            // Construction and dropping do not poll a command or detach work.
        }
        assert!(core.state.read().unwrap().jobs.is_empty());
        drop(core);
        std::fs::remove_dir_all(root).unwrap();
    }
}
