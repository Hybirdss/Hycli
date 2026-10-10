//! MCP tools share the exact same execution and approval boundary as the dashboard.
use crate::{
    apperr::{AppError, AppResult},
    policy::{self, Effect},
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
        let language = self.core.state.read()?.settings.locale;
        let mut out = vec![];
        for site in self.core.sites(&language)? {
            if self.allowed(&site.id).is_err() {
                continue;
            }
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
                let description = format!(
                    "{} · {}. {} {}",
                    site.title,
                    action.title,
                    action.description,
                    if action.effect == Effect::Read {
                        "Runs a bounded read."
                    } else {
                        "Requests user approval in the Hycli dashboard; does not execute a change until the user approves."
                    }
                );
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
        out.push(Tool::new("hycli_result","Read a website action's status and sanitized result, including after the user approves it in the dashboard.",Arc::new(result_schema)).with_annotations(ToolAnnotations::new().read_only(true).destructive(false).idempotent(true).open_world(false)));
        let check_schema: JsonObject = serde_json::from_value(
            json!({"type":"object","properties":{"site":{"type":"string","description":"Installed website ID; omit to check every website"}},"additionalProperties":false}),
        )?;
        out.push(Tool::new("hycli_check","Replay each website's recorded reads without AI. Tells an expired sign-in (verdict signed_out: ask the user to reconnect) apart from a changed website (site_changed: request a repair) before you retry anything. Never runs a change.",Arc::new(check_schema)).with_annotations(ToolAnnotations::new().read_only(true).destructive(false).idempotent(true).open_world(true)));
        out.push(Tool::new("hycli_activity", "Read current work, completed stages, agent status and plain-language work notes. No credential values or raw request details.", Arc::new(serde_json::from_value(json!({"type":"object","properties":{},"additionalProperties":false}))?)).with_annotations(ToolAnnotations::new().read_only(true).destructive(false).idempotent(true).open_world(false)));
        if !self.sites_only {
            out.push(Tool::new("hycli_run", "Run an installed website action by ID, including actions prepared during this connection. Inspect hycli_sites for typed inputs. Reads run immediately; changes return a receipt for the user to review in the dashboard.", Arc::new(serde_json::from_value(json!({"type":"object","properties":{"site":{"type":"string"},"action":{"type":"string"},"inputs":{"type":"object","additionalProperties":true},"account":{"type":"string","description":"Optional saved website account ID; omit to use the selected account"}},"required":["site","action"],"additionalProperties":false}))?)).with_annotations(ToolAnnotations::new().read_only(false).destructive(true).idempotent(false).open_world(true)));
            for (name, description, schema) in [
                (
                    "hycli_sites",
                    "List installed websites and the actions available to the current account.",
                    json!({"type":"object","properties":{},"additionalProperties":false}),
                ),
                (
                    "hycli_prepare",
                    "Prepare tools using actual documentation and bounded reads. No live writes, fuzzing or guessed endpoint sweeps.",
                    json!({"type":"object","properties":{"url":{"type":"string"},"provider":{"type":"string"},"intent":{"type":"string","description":"The user outcome to support, including prerequisite list/search actions for opaque identifiers"}},"required":["url"],"additionalProperties":false}),
                ),
                (
                    "hycli_request",
                    "Ask the AI to add missing capabilities to an installed website while preserving its existing actions. Returns a preparation job; follow hycli_job for added actions and capability gaps. Does not execute website changes.",
                    json!({"type":"object","properties":{"site":{"type":"string"},"instruction":{"type":"string","description":"Missing functionality in the user's own words, including full workflows such as editing and publishing blog posts"},"provider":{"type":"string"}},"required":["site","instruction"],"additionalProperties":false}),
                ),
                (
                    "hycli_job",
                    "Read preparation progress or a sanitized action result. Never contains sign-in secrets.",
                    json!({"type":"object","properties":{"id":{"type":"string"}},"required":["id"],"additionalProperties":false}),
                ),
                (
                    "hycli_accounts",
                    "List account metadata. Credential values are never returned.",
                    json!({"type":"object","properties":{},"additionalProperties":false}),
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
                            .read_only(true)
                            .destructive(false)
                            .idempotent(!["hycli_prepare", "hycli_request"].contains(&name))
                            .open_world(["hycli_prepare", "hycli_request"].contains(&name)),
                    ),
                );
            }
        }
        Ok(out)
    }
    async fn invoke(&self, name: &str, args: JsonObject) -> AppResult<Value> {
        let get = |key: &str| args.get(key).and_then(Value::as_str).unwrap_or("");
        let locale = self.core.state.read()?.settings.locale;
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
            // 입력값은 MCP로 노출하지 않는다 — 승인 상세는 대시보드 세션에서만.
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
                "note": "입력값·승인 상세는 Hycli 대시보드에서만 확인된다"
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
        let op = sp.op(action)?;
        let request = self.core.request(&sp, action, inputs, account)?;
        if policy::operation_effect(op, &request.inputs) != Effect::Read {
            let review = self.core.approval(request, locale)?;
            return Ok(
                json!({"status":"awaiting_user_approval","approval":review,"result_id":review.id,"next":"The user must approve this exact action in the Hycli dashboard. Keep this receipt and use hycli_result to read its status and result. Do not submit a duplicate action while it is pending."}),
            );
        }
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
        let mut previous = server
            .tools()
            .ok()
            .and_then(|tools| serde_json::to_vec(&tools).ok())
            .map(|v| util::hash(&v));
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(std::time::Duration::from_secs(3)).await;
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
            Err(error) => CallToolResult::error(vec![ContentBlock::text(
                json!({"error":{"code":error.public_code,"next_step":error.remedy}}).to_string(),
            )]),
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
