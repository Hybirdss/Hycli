//! MCP tools share the exact same execution and approval boundary as the dashboard.
use crate::{
    apperr::{AppError, AppResult},
    policy::Effect,
    runtime::Runtime,
    util,
};
use rmcp::{
    RoleServer, ServerHandler, ServiceExt,
    model::*,
    service::{NotificationContext, RequestContext},
};
use serde_json::{Value, json};
use std::{collections::BTreeMap, sync::Arc};
#[derive(Clone)]
pub struct Server {
    pub core: Arc<Runtime>,
    pub sites_only: bool,
    pub only_site: Option<String>,
}
impl Server {
    fn allowed(&self, site: &str) -> AppResult<()> {
        if self.only_site.as_ref().is_some_and(|s| s != site) {
            return Err(AppError::api("not_found", 404));
        }
        Ok(())
    }
    pub(crate) fn tools(&self) -> AppResult<Vec<Tool>> {
        let language = self.core.state.settings()?.locale;
        self.tools_from_snapshot(self.core.sites(&language)?)
    }
    pub(crate) fn tools_from_snapshot(
        &self,
        sites: Vec<crate::runtime::SiteView>,
    ) -> AppResult<Vec<Tool>> {
        let mut out = vec![];
        for site in sites {
            if self.allowed(&site.id).is_err() {
                continue;
            }
            // Supplementary help must not invalidate the already captured listing
            // if an installed definition changes concurrently. Execution revalidates.
            let definition = self.core.spec(&site.id).ok();
            // A read-only website offers its change actions to no agent.
            for action in site
                .actions
                .into_iter()
                .filter(|action| site.writes || action.effect == Effect::Read)
            {
                let mut properties = serde_json::Map::new();
                let mut required = vec![];
                for input in &action.inputs {
                    let kind = match input.kind.as_str() {
                        "int" => "integer",
                        "float" => "number",
                        "bool" => "boolean",
                        _ => "string",
                    };
                    let mut schema =
                        json!({"type":kind,"description":format!("{}: {}",input.label,input.hint)});
                    if input.kind == "json" {
                        schema.as_object_mut().unwrap().remove("type");
                    }
                    if let Some(default) = &input.default {
                        schema["default"] = default.clone();
                    }
                    properties.insert(input.id.clone(), schema);
                    if input.required {
                        required.push(input.id.clone());
                    }
                }
                let schema = json!({"type":"object","properties":properties,"required":required,"additionalProperties":false});
                let schema: JsonObject = serde_json::from_value(schema)?;
                let name = format!("{}_{}", site.id, action.id);
                let file_output = definition
                    .as_ref()
                    .and_then(|spec| spec.op(&action.id).ok())
                    .and_then(|op| op.response.as_ref())
                    .is_some_and(|response| response.format == "file");
                let mut description = format!(
                    "{} · {}. {} {}",
                    site.title,
                    action.title,
                    action.description,
                    if action.effect == Effect::Read && file_output {
                        "Streams a file read."
                    } else if action.effect == Effect::Read {
                        "Runs a bounded read."
                    } else {
                        "Runs a website change."
                    }
                );
                if (file_output
                    || definition
                        .as_ref()
                        .and_then(|spec| spec.op(&action.id).ok())
                        .is_some_and(|op| op.pagination.is_some()))
                    && !action.output.is_empty()
                {
                    description.push(' ');
                    description.push_str(&action.output);
                }
                out.push(
                    Tool::new(name, description, Arc::new(schema)).with_annotations(
                        ToolAnnotations::new()
                            .read_only(action.effect == Effect::Read)
                            .destructive(action.effect != Effect::Read)
                            .idempotent(action.effect == Effect::Read)
                            .open_world(true),
                    ),
                );
            }
        }
        let result_schema: JsonObject = serde_json::from_value(
            json!({"type":"object","properties":{"id":{"type":"string","description":"Job ID or approval ID returned by a website action"}},"required":["id"],"additionalProperties":false}),
        )?;
        out.push(Tool::new("hycli_result","Read a website action's status and sanitized result, including after dashboard approval. For a completed file action, use its typed artifact, never file-shaped result/body JSON. Automatically list/explain each file from metadata; distinguish completion, checked availability and actual inspection. Paths belong to the Hycli host; remote access is not assumed.",Arc::new(result_schema)).with_annotations(ToolAnnotations::new().read_only(true).destructive(false).idempotent(true).open_world(false)));
        let check_schema: JsonObject = serde_json::from_value(
            json!({"type":"object","properties":{"site":{"type":"string","description":"Installed website ID; omit to check every website"}},"additionalProperties":false}),
        )?;
        out.push(Tool::new("hycli_check","Replay each website's recorded reads without AI. Tells an expired sign-in (verdict signed_out: ask the user to reconnect) apart from a changed website (site_changed: request a repair) before you retry anything. Never runs a change.",Arc::new(check_schema)).with_annotations(ToolAnnotations::new().read_only(true).destructive(false).idempotent(true).open_world(true)));
        out.push(Tool::new("hycli_activity", "Read current work, available recent action history, completed stages, agent status and work notes. Use completed action jobs with typed artifacts to list/explain files for the current task; fetch details through hycli_result or hycli_job when available. Include artifact/job IDs, metadata-based purpose, source/action, filename, host-local path, type, actual size and creation time; distinguish completion, availability and inspection. Not an exhaustive permanent catalogue: history clearing/pruning can omit records while files remain. No credential values or raw request details.", Arc::new(serde_json::from_value(json!({"type":"object","properties":{},"additionalProperties":false}))?)).with_annotations(ToolAnnotations::new().read_only(true).destructive(false).idempotent(true).open_world(false)));
        if !self.sites_only {
            out.push(Tool::new("hycli_run", "Run an installed website action by ID, including actions prepared during this connection. Inspect hycli_sites for typed inputs and file-output guidance. Automatically list/explain returned completed artifacts from metadata, not file-shaped website JSON; completion does not prove current availability or content inspection. Reuse an explicit host-local file path only for the authorized task.", Arc::new(serde_json::from_value(json!({"type":"object","properties":{"site":{"type":"string"},"action":{"type":"string"},"inputs":{"type":"object","additionalProperties":true},"account":{"type":"string","description":"Optional saved website account ID; omit to use the selected account"}},"required":["site","action"],"additionalProperties":false}))?)).with_annotations(ToolAnnotations::new().read_only(false).destructive(true).idempotent(false).open_world(true)));
            for (name, description, schema) in [
                (
                    "hycli_sites",
                    "List installed websites and the actions available to the current account.",
                    json!({"type":"object","properties":{},"additionalProperties":false}),
                ),
                (
                    "hycli_recon",
                    "Deterministic fast recon without an AI loop: fetches the URL, mines SSR JSON (__NEXT_DATA__, __NUXT_DATA__, ld+json, window.__STATE__), HTML forms, documented routes, and prioritized JS bundle fetch/axios/GraphQL endpoints (up to 24 bundles). Use this to exhaust the raw surface before authoring or requesting tools.",
                    json!({"type":"object","properties":{"url":{"type":"string"},"site":{"type":"string"},"max_bundles":{"type":"integer","default":8}},"required":["url"],"additionalProperties":false}),
                ),
                (
                    "hycli_prepare",
                    "Prepare website tools using structured adversarial self-interrogation (self_qa), SSR/JS bundle/form recon, SiteSpec v2 preflight extractors, derived session headers, and live read verification. No live writes, fuzzing or guessed endpoint sweeps.",
                    json!({"type":"object","properties":{"url":{"type":"string"},"provider":{"type":"string"},"intent":{"type":"string","description":"The user outcome to support, including prerequisite list/search actions for opaque identifiers"}},"required":["url"],"additionalProperties":false}),
                ),
                (
                    "hycli_request",
                    "Ask the AI to add missing capabilities to an installed website while preserving its existing actions. Runs adversarial self-interrogation (self_qa) across SSR state, JS bundles, HTML forms, and preflight/derived auth to close workflow gaps. Follow hycli_job for added actions, self_qa audit trail, and capability gaps. Does not execute website changes.",
                    json!({"type":"object","properties":{"site":{"type":"string"},"instruction":{"type":"string","description":"Missing functionality in the user's own words, including full workflows such as editing and publishing blog posts"},"provider":{"type":"string"}},"required":["site","instruction"],"additionalProperties":false}),
                ),
                (
                    "hycli_job",
                    "Read preparation progress, structured self_qa audit trail, or a sanitized action result. For completed action jobs, typed artifact metadata supports a file list/explanation, not a content summary. Show filename, artifact/job IDs, metadata-based purpose, source/action, Hycli-host path, type, actual size and creation time; separate completion, checked availability and actual inspection. Never infer file authority from result/body JSON. Never contains sign-in secrets.",
                    json!({"type":"object","properties":{"id":{"type":"string"}},"required":["id"],"additionalProperties":false}),
                ),
                (
                    "hycli_accounts",
                    "List account metadata. Credential values are never returned.",
                    json!({"type":"object","properties":{},"additionalProperties":false}),
                ),
                (
                    "hycli_writes",
                    "Enable or disable write actions for an installed website CLI after asking the user once per CLI whether to allow writes.",
                    json!({"type":"object","properties":{"site":{"type":"string","description":"Installed website ID"},"allow":{"type":"boolean","description":"Set to true after the user confirms once to allow write actions for this CLI; false to disable writes"}},"required":["site","allow"],"additionalProperties":false}),
                ),
            ] {
                out.push(
                    Tool::new(
                        name,
                        description,
                        Arc::new(serde_json::from_value::<JsonObject>(schema)?),
                    )
                    .with_annotations(
                        ToolAnnotations::new()
                            .read_only(name != "hycli_writes")
                            .destructive(name == "hycli_writes")
                            .idempotent(!["hycli_prepare", "hycli_request"].contains(&name))
                            .open_world(
                                ["hycli_prepare", "hycli_request", "hycli_recon"].contains(&name),
                            ),
                    ),
                );
            }
        }
        Ok(out)
    }
    async fn invoke(&self, name: &str, args: JsonObject) -> AppResult<Value> {
        let get = |key: &str| args.get(key).and_then(Value::as_str).unwrap_or("");
        let locale = self.core.state.settings()?.locale;
        if name == "hycli_activity" {
            let jobs: Vec<_> = self
                .core
                .state
                .read()?
                .jobs
                .into_iter()
                .filter(|job| self.allowed(&job.site_id).is_ok())
                .map(|mut job| {
                    job.result = None;

                    job.url.clear();
                    job
                })
                .collect();
            return Ok(json!(jobs));
        }
        if name == "hycli_check" {
            let mut sites: Vec<_> = self
                .core
                .specs()?
                .into_iter()
                .map(|sp| sp.site.name)
                .filter(|id| self.allowed(id).is_ok())
                .collect();
            if !get("site").is_empty() {
                self.allowed(get("site"))?;
                self.core.spec(get("site"))?;
                sites = vec![get("site").to_string()];
            }
            let mut out = vec![];
            for site in sites {
                out.push(self.core.check_site(&site).await?);
            }
            return Ok(json!(out));
        }
        if name == "hycli_result" {
            let id = get("id");
            if let Some(job) = self
                .core
                .state
                .read()?
                .jobs
                .into_iter()
                .find(|j| j.id == id || j.approval_id == id)
            {
                self.allowed(&job.site_id)?;
                return Ok(json!(self.core.state.job(&job.id)?));
            }
            let approval = self.core.policy.review(id)?;
            self.allowed(&approval.site_id)?;
            // Input values are not exposed over MCP; approval details are visible only in the local dashboard.
            return Ok(json!({"status":"awaiting_user_approval","approval":{
                "id": approval.id,
                "site_id": approval.site_id,
                "site_title": approval.site_title,
                "action_id": approval.action_id,
                "action_title": approval.action_title,
                "account_label": approval.account_label,
                "state": approval.status,
                "expires_at": approval.expires_at,
                "inputs": approval.inputs.iter().map(|i| json!({"label": i.label})).collect::<Vec<_>>(),
                "note": "Input values and approval details are reviewed only in the Hycli dashboard"
            }}));
        }
        if !self.sites_only {
            match name {
                "hycli_sites" => {
                    return Ok(json!(
                        self.core
                            .sites(&locale)?
                            .into_iter()
                            .filter(|s| self.allowed(&s.id).is_ok())
                            .collect::<Vec<_>>()
                    ));
                }
                "hycli_recon" => {
                    if self.only_site.is_some() {
                        return Err(AppError::api("bad_request", 400));
                    }
                    let max_bundles =
                        args.get("max_bundles").and_then(Value::as_u64).unwrap_or(8) as usize;
                    let (report, _) = self
                        .core
                        .recon(get("site"), get("url"), max_bundles.min(24))
                        .await?;
                    return Ok(report);
                }
                "hycli_accounts" => {
                    return Ok(json!(
                        self.core
                            .state
                            .read()?
                            .accounts
                            .values()
                            .filter(|a| self.allowed(&a.site_id).is_ok())
                            .collect::<Vec<_>>()
                    ));
                }
                "hycli_job" => {
                    let job = self.core.state.job(get("id"))?;
                    self.allowed(&job.site_id)?;
                    return Ok(json!(job));
                }
                "hycli_prepare" => {
                    if self.only_site.is_some() {
                        return Err(AppError::api("bad_request", 400));
                    }
                    return Ok(json!(self.core.start_prepare_with_intent(
                        get("url"),
                        "",
                        get("provider"),
                        &locale,
                        get("intent")
                    )?));
                }
                "hycli_request" => {
                    self.allowed(get("site"))?;
                    return Ok(json!(self.core.start_request(
                        get("site"),
                        get("instruction"),
                        get("provider"),
                        &locale
                    )?));
                }
                "hycli_run" => {
                    let inputs = match args.get("inputs") {
                        None => BTreeMap::new(),
                        Some(Value::Object(values)) => values
                            .iter()
                            .map(|(k, v)| (k.clone(), util::plain(v)))
                            .collect(),
                        _ => return Err(AppError::api("bad_request", 400)),
                    };
                    let account = args
                        .get("account")
                        .map(|value| {
                            value
                                .as_str()
                                .ok_or_else(|| AppError::api("bad_request", 400))
                        })
                        .transpose()?;
                    return self
                        .run_action(get("site"), get("action"), inputs, account, &locale)
                        .await;
                }
                "hycli_writes" => {
                    let site = get("site");
                    self.allowed(site)?;
                    let allow = args
                        .get("allow")
                        .and_then(Value::as_bool)
                        .ok_or_else(|| AppError::api("bad_request", 400))?;
                    self.core.set_writes(site, allow)?;
                    return Ok(json!({"site": site, "writes": allow}));
                }
                _ => {}
            }
        }
        for sp in self.core.specs()? {
            if self.allowed(&sp.site.name).is_err() {
                continue;
            }
            for op in &sp.operations {
                if name != format!("{}_{}", sp.site.name, op.name) {
                    continue;
                }
                let inputs: BTreeMap<_, _> = args
                    .iter()
                    .map(|(k, v)| (k.clone(), util::plain(v)))
                    .collect();
                return self
                    .run_action(&sp.site.name, &op.name, inputs, None, &locale)
                    .await;
            }
        }
        Err(AppError::api("not_found", 404))
    }
    async fn run_action(
        &self,
        site: &str,
        action: &str,
        inputs: BTreeMap<String, String>,
        account: Option<&str>,
        locale: &str,
    ) -> AppResult<Value> {
        self.allowed(site)?;
        let sp = self.core.spec(site)?;
        let request = self.core.request(&sp, action, inputs, account)?;
        let (result, _) = self.core.execute_observed(request, locale).await?;
        Ok(json!(result))
    }
}
impl ServerHandler for Server {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(
            ServerCapabilities::builder()
                .enable_tools()
                .enable_tool_list_changed()
                .build(),
        )
    }
    async fn on_initialized(&self, context: NotificationContext<RoleServer>) {
        let server = self.clone();
        let mut rx = server.core.events.subscribe();
        let mut previous = server
            .tools()
            .ok()
            .and_then(|tools| serde_json::to_vec(&tools).ok())
            .map(|v| util::hash(&v));
        tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = rx.recv() => {}
                    _ = tokio::time::sleep(std::time::Duration::from_secs(30)) => {}
                }
                if context.peer.is_transport_closed() {
                    break;
                }
                let current = server
                    .tools()
                    .ok()
                    .and_then(|tools| serde_json::to_vec(&tools).ok())
                    .map(|v| util::hash(&v));
                if current.is_some() && current != previous {
                    previous = current;
                    if context.peer.notify_tool_list_changed().await.is_err() {
                        break;
                    }
                }
            }
        });
    }
    async fn list_tools(
        &self,
        _: Option<PaginatedRequestParams>,
        _: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, ErrorData> {
        self.tools()
            .map(ListToolsResult::with_all_items)
            .map_err(|_| ErrorData::internal_error("Unable to list website tools", None))
    }
    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, ErrorData> {
        let result = tokio::select! {_ = context.ct.cancelled()=>Err(AppError::api("cancelled",409)),result=self.invoke(&request.name,request.arguments.unwrap_or_default())=>result};
        Ok(match result {
            Ok(value) => CallToolResult::success(vec![ContentBlock::text(value.to_string())]),
            Err(error) => {
                let mut payload = error.public_error();
                payload["error"]["next_step"] = json!(error.remedy);
                CallToolResult::error(vec![ContentBlock::text(payload.to_string())])
            }
        }
        .into())
    }
}
pub async fn serve(server: Server) -> AppResult<()> {
    let running = server
        .serve(rmcp::transport::stdio())
        .await
        .map_err(|_| AppError::api("internal", 500))?;
    running
        .waiting()
        .await
        .map_err(|_| AppError::api("internal", 500))?;
    Ok(())
}
