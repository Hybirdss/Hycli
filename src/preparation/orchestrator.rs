use super::{
    grounding::{filter_worker_spec, merge_worker_spec, uncovered_routes},
    models::{
        Plan, Source, WorkflowReview, decode_plan, redact_self_qa, valid_understanding,
        workflow_review_feedback,
    },
    prompts::{
        SYSTEM, WORKERS, build_review_task, build_round_prompt, build_understand_task,
        build_worker_task,
    },
};
use crate::{
    ai,
    apperr::{AppError, AppResult},
    policy,
    runtime::Runtime,
    smoke::ReadCheck,
    spec::Spec,
    state::{Job, SiteDescription, WebsiteUnderstanding},
    util,
};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

impl Runtime {
    pub(super) async fn prepare(self: &Arc<Self>, job: &Job, config: &ai::Config) -> AppResult<()> {
        self.job_progress(&job.id, "understand", 0)?;
        let mut environment = tokio::task::spawn_blocking(crate::environment::inspect)
            .await
            .map_err(|_| AppError::api("internal", 500))?;
        self.job_note(
            &job.id,
            "environment_checked",
            "check",
            environment.browsers.len(),
            "",
        )?;
        let mut account_id = self.browser_account_for(&job.site_id, &job.url)?;
        self.state
            .update_job(&job.id, |work| work.account_id = account_id.clone())?;
        self.job_progress(&job.id, "connect", 1)?;
        let initial = crate::net::validate_url(&job.url)?;
        let mut sources = vec![Source {
            url: "environment:current".into(),
            kind: "Observed environment and available tools".into(),
            content: json!(environment),
        }];
        let mut documents = BTreeMap::<String, String>::new();
        let mut tool_attempts = 0usize;
        let mut session_tool_attempts = 0usize;
        let mut completed_calls = BTreeMap::<String, String>::new();
        let mut links = BTreeSet::new();
        let mut seen = BTreeSet::new();
        let mut evidence = BTreeSet::new();
        let mut routes = BTreeSet::<(String, String)>::new();
        // API operations published by schemas or reference text: the coverage target.
        let mut api_routes = BTreeSet::<(String, String)>::new();
        let mut coverage_rounds = 0usize;
        let mut browser_reads = BTreeSet::<String>::new();
        let mut api_prefixes = BTreeSet::<String>::new();
        let mut api_bases = BTreeSet::from([initial.origin().ascii_serialization()]);
        let mut queue = vec![job.url.clone()];
        let installed = self.spec(&job.site_id).ok();
        // Existing actions are part of the same workflow: extensions must be
        // able to use an installed prerequisite without rediscovering its schema.
        let mut candidate: Option<Spec> = installed.clone();
        let mut capability_gaps = BTreeMap::<String, Vec<String>>::new();
        let mut self_interrogation = BTreeMap::<String, Vec<crate::state::SelfQa>>::new();
        let mut understanding: Option<WebsiteUnderstanding> = None;
        let mut workflow_review: Option<WorkflowReview> = None;
        let mut review_requested = false;
        let mut description = SiteDescription::default();
        let mut feedback = BTreeMap::<String, String>::new();
        let mut icon_candidates = vec![];
        let mut verification = BTreeMap::<String, bool>::new();
        let mut read_attempts = BTreeSet::new();
        let mut read_results = BTreeMap::<String, Value>::new();
        // Passing checks in execution order become the website's token-free smoke test.
        let mut smoke_checks = Vec::<ReadCheck>::new();
        let mut attempted = 0usize;
        let mut repair_rounds = 0usize;
        let mut auth_requirements = BTreeMap::new();
        let mut observed_session_routes = BTreeSet::new();
        if let Some(meta) = self.state.read()?.sites.get(&job.site_id) {
            if !meta.observations.is_empty() {
                for seen in &meta.observations {
                    evidence.insert(seen.path.clone());
                    routes.insert((seen.method.clone(), seen.path.clone()));
                    if (200..300).contains(&seen.status) {
                        observed_session_routes
                            .insert((seen.method.to_ascii_uppercase(), seen.path.clone()));
                    }
                }
                sources.push(Source {
                    url: initial.origin().ascii_serialization(),
                    kind: "Request shapes actually observed in the user's browser (no credentials or values)".into(),
                    content: json!(meta.observations),
                });
            }
        }
        for round in 0..20 {
            self.ingest_round_reads(
                job,
                &initial,
                round,
                &account_id,
                &mut queue,
                &mut seen,
                &mut sources,
                &mut api_bases,
                &mut routes,
                &mut links,
                &mut api_routes,
                &mut evidence,
                &mut api_prefixes,
                &mut documents,
                &mut auth_requirements,
                &mut icon_candidates,
            )
            .await?;
            self.job_progress(
                &job.id,
                if understanding.is_none() {
                    "understand"
                } else if review_requested {
                    "review"
                } else {
                    "prepare"
                },
                if understanding.is_none() { 1 } else { 2 },
            )?;
            let link_list: Vec<_> =
                crate::discovery::ranked(links.iter().filter(|u| !seen.contains(*u)))
                    .into_iter()
                    .take(160)
                    .collect();
            let prompt = build_round_prompt(
                job,
                &initial,
                round,
                &environment,
                tool_attempts,
                session_tool_attempts,
                seen.len(),
                &documents,
                &browser_reads,
                &api_prefixes,
                &api_bases,
                &routes,
                &link_list,
                &sources,
                &self_interrogation,
                &feedback,
                &api_routes,
                installed.as_ref(),
                candidate.as_ref(),
                understanding.as_ref(),
                workflow_review.as_ref(),
            );
            let assessing = understanding.is_none();
            let reviewing = !assessing && review_requested;
            let replies = if assessing {
                self.job_note(&job.id, "understanding", "", 0, "")?;
                // User value comes first. Machine capabilities and implementation
                // assignments do not determine or narrow the selected workflows.
                let task = build_understand_task(
                    job,
                    &api_bases,
                    &api_prefixes,
                    &routes,
                    &sources,
                    &link_list,
                    &feedback,
                );
                vec![(
                    "understand",
                    self.plan_worker(job, config, "understand", task).await,
                )]
            } else if reviewing {
                self.job_progress(&job.id, "review", 2)?;
                self.job_note(&job.id, "workflow_review", "", 0, "")?;
                let task = build_review_task(&prompt);
                vec![(
                    "review",
                    self.plan_worker(job, config, "review", task).await,
                )]
            } else {
                self.job_note(&job.id, "agents_started", "", 3, "")?;
                futures_util::future::join_all(WORKERS.iter().map(|(role, assignment)| {
                    let task = build_worker_task(&prompt, role, assignment, &feedback);
                    async move { (*role, self.plan_worker(job, config, role, task).await) }
                }))
                .await
            };
            feedback.clear();
            let mut next_links = BTreeSet::new();
            let mut calls = Vec::new();
            let mut failures = vec![];
            let mut successes = 0;
            for (role, result) in replies {
                let mut plan = match result {
                    Ok(plan) => {
                        successes += 1;
                        plan
                    }
                    Err(error) => {
                        failures.push(error);
                        continue;
                    }
                };
                redact_self_qa(&mut plan.self_qa);
                if !plan.self_qa.is_empty() {
                    self_interrogation.insert(role.into(), plan.self_qa.clone());
                }
                if role == "understand" {
                    if let Some(mut value) = plan.understanding.clone().filter(valid_understanding)
                    {
                        value.summary = util::redact_text(&value.summary);
                        if value.self_qa.is_empty() && !plan.self_qa.is_empty() {
                            value.self_qa = plan.self_qa.clone();
                        }
                        redact_self_qa(&mut value.self_qa);
                        if !value.self_qa.is_empty() {
                            self_interrogation.insert("understand".into(), value.self_qa.clone());
                        }
                        for workflow in &mut value.workflows {
                            workflow.title = util::redact_text(&workflow.title);
                            workflow.benefit = util::redact_text(&workflow.benefit);
                            workflow.success_criteria =
                                util::redact_text(&workflow.success_criteria);
                            workflow.steps = workflow
                                .steps
                                .iter()
                                .map(|step| util::redact_text(step))
                                .collect();
                        }
                        self.state
                            .update_job(&job.id, |work| work.understanding = Some(value.clone()))?;
                        understanding = Some(value);
                        self.job_note(&job.id, "workflow_planned", "", 0, "")?;
                    } else {
                        feedback.insert("understand".into(), "Return a website understanding with self_qa, meaningful workflows, complete ordered steps, user benefit and observable success criteria. Read further published sources if needed; do not start from a handpicked endpoint.".into());
                    }
                }
                if role == "review" {
                    let mut review = plan.workflow_review.clone();
                    if let Some(ref mut rev) = review {
                        if rev.self_qa.is_empty() && !plan.self_qa.is_empty() {
                            rev.self_qa = plan.self_qa.clone();
                        }
                        redact_self_qa(&mut rev.self_qa);
                        if !rev.self_qa.is_empty() {
                            self_interrogation.insert("review".into(), rev.self_qa.clone());
                        }
                    }
                    workflow_review = review;
                    review_requested = false;
                }
                capability_gaps.insert(
                    role.into(),
                    plan.capability_gaps
                        .iter()
                        .take(40)
                        .map(|gap| util::redact_text(gap))
                        .collect(),
                );
                calls.extend(
                    plan.calls
                        .into_iter()
                        .filter(|call| {
                            ["check", "review"].contains(&role)
                                || !call.tool.starts_with("session.")
                        })
                        .enumerate()
                        .map(|(index, call)| (index, role, call)),
                );
                next_links.extend(
                    plan.read_urls
                        .into_iter()
                        .filter(|u| links.contains(u) && !seen.contains(u)),
                );
                if let Some(mut sp) = plan.spec.filter(|_| !assessing) {
                    sp.site.name = job.site_id.clone();
                    let requested_base = sp.site.base_url.trim_end_matches('/').to_string();
                    if !api_bases.contains(&requested_base) {
                        feedback.insert(role.into(), "Select an exact published base URL from documented_api_bases; a server address cannot be guessed.".into());
                        continue;
                    }
                    sp.site.base_url = requested_base;
                    sp.site.source_url = initial.to_string();
                    sp.defaults.tls_profile = "standard".into();
                    for op in &mut sp.operations {
                        // New API definitions must distinguish a data response from an HTML
                        // login/app fallback. The agent can declare another documented format.
                        if op.response.is_none() {
                            op.response = Some(crate::spec::ResponseExpectation {
                                format: "json".into(),
                                required_pointers: vec![],
                                ..Default::default()
                            });
                        }
                    }
                    if sp.operations.iter().any(|op| {
                        op.response.as_ref().is_some_and(|expected| {
                            expected.format == "html" && expected.required_html_fields.is_empty()
                        })
                    }) {
                        feedback.insert(role.into(), "HTML actions require at least one observed unique nonempty required_html_fields selector and attribute. Use session.observe, session.read or browser.render to inspect the expected page; a generic HTML response may be a login fallback.".into());
                        continue;
                    }
                    let proposed: Vec<_> = sp
                        .operations
                        .iter()
                        .map(|op| format!("{} {}", op.method, op.path))
                        .collect();
                    filter_worker_spec(
                        &mut sp,
                        &initial,
                        &account_id,
                        &api_bases,
                        &api_prefixes,
                        &routes,
                        &evidence,
                        &browser_reads,
                        &observed_session_routes,
                        &auth_requirements,
                    );
                    if sp.operations.is_empty() {
                        if !proposed.is_empty() {
                            feedback.insert(role.into(),format!("The proposed actions had no matching method/path evidence after applying the documented API prefix: {}. Use documented_method_paths and documented_api_prefixes, or read a supporting source. Do not silently repeat the rejected definition.",proposed.join(", ")));
                        }
                        continue;
                    }
                    // Apply the same defaults used when the installed definition is reloaded.
                    // Otherwise its execution hash differs before the first verification read.
                    sp = crate::spec::parse(&serde_json::to_vec(&sp)?)?;
                    if policy::validate_spec(&sp).is_err() {
                        feedback.insert(role.into(), "Correct the spec using the same observations: check declared inputs and types, required/default combinations, auth references and secret-free headers.".into());
                        self.job_note(&job.id, "agent_retry", role, 0, "")?;
                        continue;
                    }
                    let accepted_names = if let Some(existing) = &mut candidate {
                        match merge_worker_spec(existing, sp, role) {
                            Ok(names) => names,
                            Err(_) => {
                                feedback.insert(role.into(), format!("Use the selected API server {} and preserve its evidenced routes.",existing.site.base_url));
                                continue;
                            }
                        }
                    } else {
                        let names = sp
                            .operations
                            .iter()
                            .map(|operation| (operation.name.clone(), operation.name.clone()))
                            .collect();
                        candidate = Some(sp);
                        names
                    };
                    if description.title.is_empty() {
                        description.title = plan.description.title;
                    }
                    if description.summary.is_empty() {
                        description.summary = plan.description.summary;
                    }
                    for (name, action) in plan.description.actions {
                        if let Some(accepted) = accepted_names.get(&name) {
                            description.actions.insert(accepted.clone(), action);
                        }
                    }
                }
            }
            if successes == 0 {
                return Err(failures
                    .into_iter()
                    .next()
                    .unwrap_or_else(|| AppError::api("ai_unavailable", 502)));
            }
            if let Some(sp) = &candidate {
                self.state.update_job(&job.id, |work| {
                    work.found = sp
                        .operations
                        .iter()
                        .filter_map(|op| description.actions.get(&op.name))
                        .map(|action| crate::progress::public_note(&action.title))
                        .filter(|title| !title.is_empty())
                        .take(12)
                        .collect();
                })?;
                self.job_note(&job.id, "found", "", sp.operations.len(), "")?;
            }
            let had_calls = !calls.is_empty();
            // Interleave specialists so documentation reads cannot consume the sign-in
            // worker's final verification slot. Keep each worker's dependent order.
            calls.sort_by_key(|(index, role, _)| {
                (
                    *index,
                    match *role {
                        "check" => 0,
                        "explore" => 1,
                        _ => 2,
                    },
                )
            });
            let mut requested = BTreeSet::new();
            for (_, role, call) in calls {
                if requested.len() >= 6 {
                    break;
                }
                // Local evidence searches and queued documentation pages have their
                // own bounded loop/page budget. They must not starve session checks.
                let session_tool =
                    call.tool.starts_with("session.") || call.tool == "browser.render";
                if (session_tool && session_tool_attempts >= 32)
                    || (call.tool == "web.read" && seen.len() >= 48)
                {
                    feedback.insert(role.into(), "This capability's request budget is exhausted. Reuse completed evidence and return the supported operations; do not repeat this request.".into());
                    continue;
                }
                let mut signature = serde_json::to_string(&call)?;
                if let Some(id) = call.arguments.get("account_id").and_then(Value::as_str) {
                    if let Ok(credential) = self.credential(id) {
                        signature.push_str(&util::hash(&serde_json::to_vec(&credential)?));
                    }
                }
                if let Some(previous) = completed_calls.get(&signature) {
                    feedback.insert(role.into(),format!("This exact request already has a result at {previous}. Reuse that result or choose another available tool/source. Repeating it does not provide new evidence."));
                    continue;
                }
                if !requested.insert(signature.clone()) {
                    continue;
                }
                tool_attempts += 1;
                if session_tool {
                    session_tool_attempts += 1;
                }
                self.job_note(&job.id, "tool_started", role, tool_attempts, "")?;
                let context = crate::agent_tools::Context {
                    job,
                    environment: &environment,
                    links: &links,
                    routes: &routes,
                    prefixes: &api_prefixes,
                    documents: &documents,
                };
                let content = match self.preparation_tool(&call, &context).await {
                    Ok(result) => {
                        if let Some(url) = result.browser_read {
                            browser_reads.insert(url);
                        }
                        if let Some(updated) = result.environment {
                            environment = updated;
                        }
                        if let Some(url) = result.read_url {
                            if !seen.contains(&url) {
                                next_links.insert(url);
                            }
                        }
                        links.extend(result.links);
                        for (_, path) in &result.routes {
                            evidence.insert(path.clone());
                        }
                        routes.extend(result.routes);
                        if let Some(arr) = result
                            .content
                            .pointer("/recon/api_bases")
                            .and_then(Value::as_array)
                        {
                            for base in arr.iter().filter_map(Value::as_str) {
                                api_bases.insert(base.trim_end_matches('/').to_string());
                            }
                        }
                        if let Some((url, text)) = result.document {
                            if let Ok(base) = url::Url::parse(&url) {
                                routes.insert(("GET".into(), base.path().into()));
                                api_bases.extend(crate::discovery::api_bases(&base, &text));
                                api_bases.extend(crate::discovery::bundle_api_bases(&base, &text));
                            }
                            let documented = crate::discovery::documented_routes(&text);
                            api_routes.extend(documented.iter().cloned());
                            routes.extend(documented);
                            let js_routes = crate::discovery::bundle_routes(&text);
                            for (_, path) in &js_routes {
                                evidence.insert(path.clone());
                            }
                            routes.extend(js_routes);
                            api_prefixes.extend(crate::discovery::api_prefixes(&initial, &text));
                            documents.insert(url, text);
                        }
                        self.job_note(&job.id, "tool_completed", role, tool_attempts, "")?;
                        result.content
                    }
                    Err(error) => {
                        self.job_note(&job.id, "tool_failed", role, tool_attempts, "")?;
                        let next_step = if call.tool == "session.verify"
                            && error.public_code == "unsupported_evidence"
                        {
                            "identity_url must be an observed or documented GET. Set recipe.evidence to the actual page URL returned by session.observe/browser.render or an exact source_catalog URL, not a tool: result identifier. Inspect the page first if needed."
                        } else {
                            "Use the failure to choose another available tool, source or supported connection recipe. Never report success from this attempt."
                        };
                        json!({"error":error.public_code,"next_step":next_step})
                    }
                };
                if call.tool != "environment.inspect" {
                    completed_calls
                        .insert(signature, format!("tool:{}:{tool_attempts}", call.tool));
                }
                sources.push(Source {
                    url: format!("tool:{}:{tool_attempts}", call.tool),
                    kind: "Actual agent tool result".into(),
                    content: json!({"tool":call.tool,"arguments":call.arguments,"result":content}),
                });
                let selected = self.browser_account_for(&job.site_id, &job.url)?;
                if selected != account_id {
                    account_id = selected;
                    self.state
                        .update_job(&job.id, |work| work.account_id = account_id.clone())?;
                }
            }
            queue = crate::discovery::ranked(next_links.iter())
                .into_iter()
                .take(4)
                .collect();
            if assessing {
                continue;
            }
            if queue.is_empty() && feedback.is_empty() && !had_calls {
                if understanding.is_some() && !reviewing {
                    review_requested = true;
                    continue;
                }
                if candidate.is_none() {
                    for (role, _) in WORKERS {
                        feedback.insert(role.into(), "No useful workflow action has been implemented yet. Inspect the reviewer feedback and try a different evidenced source or tool; do not stop after one unsupported approach.".into());
                    }
                    continue;
                }
                if let (Some(plan), Some(spec)) = (&understanding, &candidate)
                    && let Some(msg) = workflow_review_feedback(
                        plan,
                        spec,
                        workflow_review.as_ref(),
                        &sources,
                        reviewing,
                    )
                {
                    for (role, _) in WORKERS {
                        feedback.insert(role.into(), msg.clone());
                    }
                    self.job_note(&job.id, "workflow_incomplete", "", 0, "")?;
                    continue;
                }
                let uncovered = uncovered_routes(&api_routes, candidate.as_ref(), &api_prefixes);
                if !uncovered.is_empty() && coverage_rounds < 2 {
                    coverage_rounds += 1;
                    for role in ["explore", "organize"] {
                        feedback.insert(role.into(), format!("The planned workflows are covered, but {} documented operations have no action yet: {}. Implement each one a user could use, or report it in capability_gaps with its concrete reason.", uncovered.len(), uncovered.iter().take(120).cloned().collect::<Vec<_>>().join(", ")));
                    }
                    self.job_note(&job.id, "coverage_incomplete", "", uncovered.len(), "")?;
                    continue;
                }
                if let Some(proposed) = candidate.as_mut()
                    && self
                        .verify_candidate_reads(
                            job,
                            &account_id,
                            proposed,
                            &mut description,
                            &mut workflow_review,
                            &mut review_requested,
                            &mut verification,
                            &mut read_attempts,
                            &mut read_results,
                            &mut smoke_checks,
                            &mut attempted,
                            &mut repair_rounds,
                            &mut sources,
                            &mut feedback,
                        )
                        .await?
                {
                    continue;
                }
                break;
            }
        }
        self.finalize_preparation(
            job,
            &initial,
            environment,
            &account_id,
            installed,
            candidate,
            description,
            understanding,
            workflow_review,
            self_interrogation,
            capability_gaps,
            verification,
            smoke_checks,
            api_routes,
            api_prefixes,
            documents.len(),
            tool_attempts,
            attempted,
            icon_candidates,
        )
        .await
    }

    pub(super) async fn plan_worker(
        &self,
        job: &Job,
        config: &ai::Config,
        role: &str,
        prompt: Value,
    ) -> AppResult<Plan> {
        self.agent_progress(&job.id, role, "queued", 0, "")?;
        let result = self
            .ai
            .complete_when_ready(config, SYSTEM, &prompt.to_string(), || {
                self.agent_progress(&job.id, role, "running", 0, "")
            })
            .await;
        if std::env::var_os("HYCLI_PREPARATION_TRACE").is_some() {
            let mut trace = json!({"role":role,"prompt":prompt,"response":result.as_ref().ok(),"error":result.as_ref().err().map(|e| &e.public_code)});
            let mut secrets = Vec::new();
            for account in self.state.read()?.accounts.keys() {
                if let Ok(credential) = self.credential(account) {
                    secrets.extend(credential.redaction_values());
                }
            }
            util::redact(&mut trace, &secrets);
            let directory = self.root.join("preparation-traces");
            util::private_dir(&directory)?;
            util::atomic_file(
                &directory.join(format!("{}-{}-{}.json", job.id, role, util::id())),
                &serde_json::to_vec(&trace)?,
            )?;
        }
        let plan = result.and_then(|text| decode_plan(&text));
        match &plan {
            Ok(plan) => self.agent_progress(
                &job.id,
                role,
                "completed",
                plan.spec.as_ref().map_or(0, |s| s.operations.len()),
                &plan.note,
            )?,
            Err(_) => self.agent_progress(&job.id, role, "failed", 0, "")?,
        }
        plan
    }
}
