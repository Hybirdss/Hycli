//! The single execution boundary used by the dashboard, CLI and MCP.
use crate::{
    accounts::{self, AccountView, Credential},
    ai,
    apperr::{AppError, AppResult},
    kb::{AttemptInput, Kb},
    net::{Network, Request, Response},
    policy::{self, Approval, Effect, ExecutionRequest, ReviewInput},
    spec::{self, Operation, Spec},
    state::StateStore,
    store::Store,
    util,
};
use serde::Serialize;
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::Duration,
};

pub struct Runtime {
    _instance: Option<std::fs::File>,
    pub root: PathBuf,
    pub specs_dir: PathBuf,
    pub state: StateStore,
    pub vault: Mutex<Store>,
    pub policy: Arc<policy::Policy>,
    pub network: Network,
    pub ai: ai::Client,
    pub kb: Arc<Kb>,
    pub events: tokio::sync::broadcast::Sender<()>,
    pub tasks: Mutex<BTreeMap<String, tokio::task::AbortHandle>>,
}
#[derive(Clone, Serialize)]
pub struct FieldView {
    pub id: String,
    pub label: String,
    pub hint: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub required: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default: Option<Value>,
}
#[derive(Clone, Serialize)]
pub struct ActionView {
    pub id: String,
    pub title: String,
    pub description: String,
    pub output: String,
    pub effect: Effect,
    pub verified: bool,
    pub inputs: Vec<FieldView>,
    pub credential_id: String,
    pub accepts_account: bool,
}
#[derive(Clone, Serialize)]
pub struct WebsiteCredentialView {
    pub id: String,
    pub label: String,
    pub configured: bool,
}
#[derive(Clone, Serialize)]
pub struct SiteView {
    pub id: String,
    pub title: String,
    pub url: String,
    pub host: String,
    pub summary: String,
    pub status: String,
    pub pinned: bool,
    /// False keeps the website read-only for every caller.
    pub writes: bool,
    pub account_id: String,
    pub actions: Vec<ActionView>,
    pub credentials: Vec<WebsiteCredentialView>,
    pub updated_at: String,
    pub last_used: String,
    pub translation_pending: bool,
    pub image_url: String,
}
#[derive(Clone, Serialize)]
pub struct OpResult {
    pub site: String,
    pub op: String,
    pub status: u16,
    pub latency_ms: i64,
    pub body: Option<Value>,
    pub mapped: Option<Value>,
    pub defense: String,
    pub signal: String,
}
impl OpResult {
    pub fn ensure_success(&self) -> AppResult<()> {
        if self.signal == "query_failed" || self.signal == "action_outcome_unknown" {
            return Err(AppError::api(&self.signal, 502));
        }
        let (code, status) = match self.status {
            401 => ("auth_required", 401),
            429 => ("site_rate_limited", 429),
            403 => ("blocked_by_site", 403),
            _ if self.defense != "none" && !self.defense.is_empty() => ("blocked_by_site", 403),
            200..300 => return Ok(()),
            _ => ("request_failed", 502),
        };
        Err(AppError::api(code, status))
    }
}
impl Runtime {
    pub fn open(
        root: &Path,
        specs_dir: Option<PathBuf>,
        kb_path: Option<PathBuf>,
        recover: bool,
        allow_local: bool,
        keyring: bool,
    ) -> AppResult<Arc<Self>> {
        util::private_dir(root)?;
        let instance = if recover {
            Some(util::file_lock(&root.join("dashboard-process.lock"), true)?)
        } else {
            None
        };
        let specs_dir = specs_dir.unwrap_or_else(|| root.join("specs"));
        util::private_dir(&specs_dir)?;
        let policy = Arc::new(policy::Policy::open(&root.join("runtime.db"))?);
        let state = StateStore::open_with_recovery(&root.join("dashboard.json"), recover)?;
        if let Ok(snapshot) = state.read() {
            for job in snapshot.jobs {
                if !job.approval_id.is_empty()
                    && (job.status == "cancelled"
                        || (job.status == "failed" && job.error_code == "action_outcome_unknown"))
                {
                    let _ = policy.reject_claimed_or_pending(&job.approval_id);
                }
            }
        }
        let codex = Arc::new(ai::codex::Codex::new(root.join("provider-workspace")));
        let (events, _) = tokio::sync::broadcast::channel(100);
        Ok(Arc::new(Self {
            _instance: instance,
            root: root.into(),
            specs_dir,
            state,
            vault: Mutex::new(Store::open_with_keyring(&root.join("store.json"), keyring)?),
            policy: policy.clone(),
            network: Network::new(policy, allow_local),
            ai: ai::Client::new(codex),
            kb: Arc::new(Kb::open(&kb_path.unwrap_or_else(|| root.join("kb.db")))?),
            events,
            tasks: Mutex::new(BTreeMap::new()),
        }))
    }
    pub fn changed(&self) {
        let _ = self.events.send(());
    }
    pub fn remove_site(&self, id: &str) -> AppResult<()> {
        self.spec(id)?;
        for job in self.state.read()?.jobs {
            if job.site_id == id && ["queued", "running"].contains(&job.status.as_str()) {
                self.cancel_job(&job.id)?;
            }
        }
        self.policy.invalidate(Some(id), None)?;
        std::fs::remove_file(self.specs_dir.join(format!("{id}.yaml")))?;
        self.state.update(|d| {
            d.sites.remove(id);
            Ok(())
        })?;
        self.changed();
        Ok(())
    }
    pub fn remove_account(&self, id: &str) -> AppResult<()> {
        for job in self.state.read()?.jobs {
            if job.account_id == id && ["queued", "running"].contains(&job.status.as_str()) {
                self.cancel_job(&job.id)?;
            }
        }
        self.policy.invalidate(None, Some(id))?;
        self.vault
            .lock()
            .map_err(|_| AppError::api("internal", 500))?
            .delete("accounts", id)?;
        self.state.update(|d| {
            d.accounts.remove(id);
            for m in d.sites.values_mut() {
                if m.account_id == id {
                    m.account_id.clear();
                }
            }
            Ok(())
        })?;
        self.changed();
        Ok(())
    }
    pub fn remove_provider(&self, id: &str) -> AppResult<()> {
        self.vault
            .lock()
            .map_err(|_| AppError::api("internal", 500))?
            .delete("providers", id)?;
        self.state.update(|d| {
            d.providers.remove(id);
            if d.settings.default_provider == id {
                d.settings.default_provider = d
                    .providers
                    .iter()
                    .find(|(_, p)| p.connected)
                    .map(|(id, _)| id.clone())
                    .unwrap_or_default();
            }
            Ok(())
        })?;
        self.changed();
        Ok(())
    }
    pub fn spec(&self, id: &str) -> AppResult<Spec> {
        if !util::valid_id(id) {
            return Err(AppError::api("not_found", 404));
        }
        let sp = spec::parse(&util::read_bounded(
            &self.specs_dir.join(format!("{id}.yaml")),
            2 * 1024 * 1024,
        )?)?;
        policy::validate_spec(&sp)?;
        if sp.site.name != id {
            return Err(AppError::api("data_invalid", 400));
        }
        Ok(sp)
    }
    pub fn specs(&self) -> AppResult<Vec<Spec>> {
        let mut out = vec![];
        for entry in std::fs::read_dir(&self.specs_dir)? {
            let p = entry?.path();
            if p.extension().is_some_and(|e| e == "yaml") {
                if let Some(id) = p.file_stem().and_then(|s| s.to_str()) {
                    if let Ok(sp) = self.spec(id) {
                        out.push(sp)
                    }
                }
            }
        }
        out.sort_by(|a, b| a.site.name.cmp(&b.site.name));
        Ok(out)
    }
    pub fn install(&self, sp: &Spec) -> AppResult<()> {
        policy::validate_spec(sp)?;
        let bytes = serde_yaml::to_string(sp).map_err(|_| AppError::api("data_invalid", 400))?;
        util::atomic_file(
            &self.specs_dir.join(format!("{}.yaml", sp.site.name)),
            bytes.as_bytes(),
        )?;
        self.state.update(|d| {
            let m = d.sites.entry(sp.site.name.clone()).or_default();
            if m.created_at.is_empty() {
                m.created_at = util::now();
            }
            m.updated_at = util::now();
            Ok(())
        })?;
        self.changed();
        Ok(())
    }
    pub fn spec_hash(sp: &Spec) -> AppResult<String> {
        let normalized = spec::parse(&serde_json::to_vec(sp)?)?;
        Ok(util::hash(&serde_json::to_vec(&normalized)?))
    }
    pub fn sites(&self, locale: &str) -> AppResult<Vec<SiteView>> {
        let data = self.state.read()?;
        let mut vault = self
            .vault
            .lock()
            .map_err(|_| AppError::api("internal", 500))?;
        vault.reload()?;
        let mut out = vec![];
        for sp in self.specs()? {
            let m = data.sites.get(&sp.site.name).cloned().unwrap_or_default();
            let credentials: Vec<_> = sp
                .auth
                .iter()
                .filter(|auth| {
                    auth.kind == "header" && sp.operations.iter().any(|op| op.auth == auth.name)
                })
                .map(|auth| {
                    let key = auth
                        .value_from
                        .strip_prefix(&format!("store:{}/", sp.site.name))
                        .unwrap_or("");
                    WebsiteCredentialView {
                        id: auth.name.clone(),
                        label: humanize(&auth.name),
                        configured: vault
                            .get(&sp.site.name, key)
                            .is_some_and(|value| !value.is_empty()),
                    }
                })
                .collect();
            let hash = Self::spec_hash(&sp)?;
            let desc = m.descriptions.get(locale).filter(|d| d.hash == hash);
            let fallback = m.descriptions.get("en").filter(|d| d.hash == hash);
            let chosen = desc.or(fallback);
            let title = if !m.title.is_empty() {
                m.title.clone()
            } else {
                chosen
                    .map(|d| d.title.clone())
                    .filter(|s| !s.is_empty())
                    .unwrap_or_else(|| {
                        if sp.site.title.is_empty() {
                            humanize(&sp.site.name)
                        } else {
                            sp.site.title.clone()
                        }
                    })
            };
            let actions = sp
                .operations
                .iter()
                .map(|op| {
                    let d = chosen.and_then(|s| s.actions.get(&op.name));
                    ActionView {
                        id: op.name.clone(),
                        accepts_account: same_site_url(sp.operation_base(op), sp.site.source_url())
                            .unwrap_or(false)
                            && !sp
                                .find_auth(&op.auth)
                                .is_some_and(|auth| auth.kind == "header"),
                        credential_id: sp
                            .find_auth(&op.auth)
                            .filter(|a| a.kind == "header")
                            .map(|a| a.name.clone())
                            .unwrap_or_default(),
                        title: d
                            .map(|d| d.title.clone())
                            .filter(|s| !s.is_empty())
                            .unwrap_or_else(|| humanize(&op.name)),
                        description: d
                            .map(|d| d.description.clone())
                            .unwrap_or_else(|| op.desc.clone()),
                        output: d.map(|d| d.output.clone()).unwrap_or_default(),
                        effect: policy::operation_effect(op, &BTreeMap::new()),
                        verified: m.verified.get(&op.name) == Some(&true),
                        inputs: params(op)
                            .into_iter()
                            .map(|(id, p)| {
                                let t = d.and_then(|d| d.inputs.get(id));
                                FieldView {
                                    id: id.clone(),
                                    label: t
                                        .map(|i| i.label.clone())
                                        .unwrap_or_else(|| humanize(id)),
                                    hint: t.map(|i| i.hint.clone()).unwrap_or_default(),
                                    kind: p.kind.clone(),
                                    required: p.required,
                                    default: p.default.clone(),
                                }
                            })
                            .collect(),
                    }
                })
                .collect();
            let running = data.jobs.iter().any(|j| {
                j.site_id == sp.site.name
                    && ["queued", "running"].contains(&j.status.as_str())
                    && j.kind == "prepare"
            });
            let has_verified_read = sp.operations.iter().any(|op| {
                m.verified.get(&op.name) == Some(&true)
                    && policy::operation_effect(op, &BTreeMap::new()) == Effect::Read
            });
            out.push(SiteView {
                id: sp.site.name.clone(),
                title,
                url: sp.site.source_url().into(),
                host: url::Url::parse(sp.site.source_url())
                    .ok()
                    .and_then(|u| u.host_str().map(str::to_owned))
                    .unwrap_or_default(),
                summary: chosen.map(|d| d.summary.clone()).unwrap_or_default(),
                status: if running {
                    "preparing"
                } else if (credentials.iter().any(|credential| !credential.configured)
                    && !has_verified_read)
                    || data
                        .accounts
                        .get(&m.account_id)
                        .is_some_and(|account| account.status == "expired")
                {
                    "needs_signin"
                } else if m.workflows_complete == Some(false)
                    || m.preparation
                        .as_ref()
                        .is_some_and(|report| report.verified_reads == 0)
                        && !has_verified_read
                {
                    "needs_review"
                } else {
                    "ready"
                }
                .into(),
                pinned: m.pinned,
                writes: m.writes,
                account_id: m.account_id,
                actions,
                credentials,
                updated_at: m.updated_at,
                last_used: m.last_used,
                translation_pending: desc.is_none(),
                image_url: if m.icon_type.is_empty() {
                    String::new()
                } else {
                    format!("/api/sites/{}/image", sp.site.name)
                },
            });
        }
        out.sort_by(|a, b| b.pinned.cmp(&a.pinned).then(a.title.cmp(&b.title)));
        Ok(out)
    }
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
            &sp,
            &request.action_id,
            request.inputs.clone(),
            Some(&request.account_id),
        )?;
        let mut effect = policy::operation_effect(op, &request.inputs);
        let (mut url, mut body) = sp.build_request(op, &request.inputs)?;
        if origin(&url)? != origin(sp.operation_base(op))? {
            return Err(AppError::api("unsafe_target", 400));
        }
        let (mut headers, mut secrets) = self.headers(&sp, op, &url, &request.account_id)?;
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
        let load_credential = || -> AppResult<crate::accounts::Credential> {
            if request.account_id.is_empty() {
                Ok(crate::accounts::Credential {
                    origin: req_origin.clone(),
                    ..Default::default()
                })
            } else {
                Ok(self.credential(&request.account_id).unwrap_or_else(|_| {
                    crate::accounts::Credential {
                        origin: req_origin.clone(),
                        ..Default::default()
                    }
                }))
            }
        };
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
            let credential = load_credential()?;
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
            let fetched = self
                .network
                .fetch(Request {
                    url: req_input.url.clone(),
                    method: req_input.method.clone(),
                    headers: req_input.headers.clone(),
                    body: req_input.body.clone(),
                    account_id: req_input.account_id.clone(),
                    limit: req_input.limit,
                    timeout: req_input.timeout,
                    read_only: req_input.read_only,
                    follow_redirects: req_input.follow_redirects,
                    tls_profile: req_input.tls_profile.clone(),
                })
                .await;
            match fetched {
                Ok(r)
                    if effect == Effect::Read
                        && crate::defend::is_challenge_label(&r.defense)
                        && !op.matches_response(&r)
                        && !is_local_url =>
                {
                    let _ = self
                        .policy
                        .resume_auth(&format!("{req_origin}|{}", request.account_id));
                    let credential = load_credential()?;
                    if let Ok(in_page) = self
                        .network
                        .fetch_browser_in_page(&credential, &req_input)
                        .await
                    {
                        if in_page.successful() && op.matches_response(&in_page) {
                            Ok(in_page)
                        } else if op.method == "GET" {
                            if let Ok(rendered) = self
                                .network
                                .fetch_rendered(&credential, &url, &request.account_id)
                                .await
                            {
                                if rendered.successful() {
                                    Ok(rendered)
                                } else {
                                    Ok(in_page)
                                }
                            } else {
                                Ok(in_page)
                            }
                        } else {
                            Ok(in_page)
                        }
                    } else if op.method == "GET" {
                        if let Ok(rendered) = self
                            .network
                            .fetch_rendered(&credential, &url, &request.account_id)
                            .await
                        {
                            if rendered.successful() {
                                Ok(rendered)
                            } else {
                                Ok(r)
                            }
                        } else {
                            Ok(r)
                        }
                    } else {
                        Ok(r)
                    }
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

    async fn run_preflight_steps(
        &self,
        sp: &Spec,
        op: &Operation,
        account_id: &str,
        effective_inputs: &mut BTreeMap<String, String>,
        secrets: &mut Vec<String>,
    ) -> AppResult<()> {
        for step in &op.preflight {
            let step_url = if !step.op.is_empty() {
                let pre_op = sp.op(&step.op)?;
                let mut step_args = effective_inputs.clone();
                step_args.extend(step.args.clone());
                let (u, _) = sp.build_request(pre_op, &step_args)?;
                u
            } else {
                let base = if step.base_url.is_empty() {
                    sp.operation_base(op)
                } else {
                    step.base_url.as_str()
                };
                format!(
                    "{}/{}",
                    base.trim_end_matches('/'),
                    step.path.trim_start_matches('/')
                )
            };
            let parsed = crate::net::validate_url(&step_url)?;
            if !crate::net::same_site_origin(
                &parsed,
                &crate::net::validate_url(sp.site.source_url())?,
            ) {
                return Err(AppError::api("unsafe_target", 400));
            }
            let r = self
                .read_page_profile(&step_url, account_id, &sp.defaults.tls_profile)
                .await?;
            if !r.successful() {
                return Err(AppError::api(
                    if r.status == 401 {
                        "auth_required"
                    } else {
                        "operation_failed"
                    },
                    r.status,
                ));
            }
            let text = String::from_utf8_lossy(&r.bytes);
            let json_doc = r.json();
            let ssr_map = crate::html::extract_ssr_map(&text);
            for (var, expr) in &step.extract {
                let extracted: Option<String> = if let Some(sel) = expr.strip_prefix("css:") {
                    let (selector, attribute) = sel.split_once('@').unwrap_or((sel, ""));
                    let field = crate::html::Field {
                        selector: selector.to_string(),
                        attribute: attribute.to_string(),
                    };
                    let doc = scraper::Html::parse_document(&text);
                    field.extract(&doc)
                } else if let Some(ssr_spec) = expr.strip_prefix("ssr:") {
                    let (key, pointer) = ssr_spec.split_once('#').unwrap_or((ssr_spec, ""));
                    ssr_map.get(key).and_then(|doc| {
                        crate::html::resolve_pointer(doc, pointer).and_then(|v| match v {
                            Value::String(s) => Some(s.clone()),
                            Value::Number(n) => Some(n.to_string()),
                            _ => None,
                        })
                    })
                } else if let Some(hdr) = expr.strip_prefix("header:") {
                    let h = r.header(hdr);
                    if h.is_empty() {
                        None
                    } else {
                        Some(h.to_string())
                    }
                } else if let Some(cookie_name) = expr.strip_prefix("cookie:") {
                    let from_set_cookie = r.cookie_value(cookie_name);
                    from_set_cookie.or_else(|| {
                        if account_id.is_empty() {
                            None
                        } else {
                            self.credential(account_id).ok().and_then(|c| {
                                c.cookies
                                    .into_iter()
                                    .find(|ck| ck.name == cookie_name)
                                    .map(|ck| ck.value)
                            })
                        }
                    })
                } else if expr.starts_with('/') || expr.starts_with("json:/") {
                    let ptr = expr.strip_prefix("json:").unwrap_or(expr);
                    json_doc.as_ref().and_then(|doc| {
                        crate::html::resolve_pointer(doc, ptr).and_then(|v| match v {
                            Value::String(s) => Some(s.clone()),
                            Value::Number(n) => Some(n.to_string()),
                            _ => None,
                        })
                    })
                } else {
                    None
                };
                if let Some(val) = extracted.filter(|s| !s.is_empty()) {
                    if expr.starts_with("cookie:")
                        || expr.starts_with("header:")
                        || util::secret_field(var)
                    {
                        secrets.push(val.clone());
                    }
                    effective_inputs.insert(format!("step.{}.{var}", step.name), val.clone());
                    effective_inputs.entry(var.clone()).or_insert(val);
                }
            }
        }
        Ok(())
    }

    fn headers(
        &self,
        sp: &Spec,
        op: &Operation,
        url: &str,
        account: &str,
    ) -> AppResult<(Vec<(String, String)>, Vec<String>)> {
        let strategy = sp.find_auth(&op.auth);
        if strategy.is_some_and(|auth| auth.kind == "header") {
            let auth = strategy.unwrap();
            let scope = if auth.origin.is_empty() {
                &sp.site.base_url
            } else {
                &auth.origin
            };
            if origin(url)? != origin(scope)? {
                return Err(AppError::api("unsafe_target", 400));
            }
        }
        if strategy.is_some_and(|auth| auth.kind == "browser")
            && !self
                .state
                .account(account)?
                .is_some_and(|view| view.verified_identity && view.status == "connected")
        {
            return Err(AppError::api("auth_required", 401));
        }
        if strategy.is_some_and(|auth| auth.kind == "derived")
            && !self
                .state
                .account(account)?
                .is_some_and(|view| view.status == "connected")
        {
            return Err(AppError::api("auth_required", 401));
        }
        let mut vault = self
            .vault
            .lock()
            .map_err(|_| AppError::api("internal", 500))?;
        vault.reload()?;
        let mut headers: BTreeMap<_, _> = sp
            .defaults
            .headers
            .iter()
            .chain(op.headers.iter())
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        let mut secrets = accounts::collect_redaction_secrets(&vault);
        let mut loaded_credential: Option<Credential> = None;
        if !account.is_empty() {
            let credential: Credential = serde_json::from_str(
                vault
                    .get("accounts", account)
                    .ok_or_else(|| AppError::api("auth_required", 401))?,
            )?;
            let session_headers = credential.request_headers(url)?;
            if session_headers.is_empty() && !strategy.is_some_and(|a| a.kind == "derived") {
                return Err(AppError::api("auth_required", 401));
            }
            secrets.extend(credential.redaction_values());
            headers.extend(session_headers);
            loaded_credential = Some(credential);
        }
        if let Some(auth) = strategy {
            if auth.kind == "header" {
                let path = auth
                    .value_from
                    .strip_prefix("store:")
                    .ok_or_else(|| AppError::api("data_invalid", 400))?;
                let (site, key) = path
                    .split_once('/')
                    .ok_or_else(|| AppError::api("data_invalid", 400))?;
                if let Some(secret) = vault.get(site, key) {
                    headers.retain(|key, _| !key.eq_ignore_ascii_case(&auth.header));
                    headers.insert(auth.header.clone(), format!("{}{}", auth.prefix, secret));
                } else {
                    return Err(AppError::api("auth_required", 401));
                }
            } else if auth.kind == "derived" {
                let credential = loaded_credential
                    .as_ref()
                    .ok_or_else(|| AppError::api("auth_required", 401))?;
                let derived = credential.resolve_derived_value(&auth.value_from, url)?;
                let full_val = format!("{}{derived}", auth.prefix);
                secrets.push(derived);
                secrets.push(full_val.clone());
                headers.retain(|key, _| !key.eq_ignore_ascii_case(&auth.header));
                headers.insert(auth.header.clone(), full_val);
            }
        }
        if let Some(body) = &op.body {
            headers.retain(|key, _| !key.eq_ignore_ascii_case("content-type"));
            headers.insert("content-type".into(), body.content_type().into());
        }
        Ok((headers.into_iter().collect(), secrets))
    }
    pub fn set_website_credential(
        &self,
        site: &str,
        name: &str,
        value: Option<&str>,
    ) -> AppResult<()> {
        let sp = self.spec(site)?;
        let auth = sp
            .find_auth(name)
            .filter(|auth| auth.kind == "header")
            .ok_or_else(|| AppError::api("not_found", 404))?;
        let key = auth
            .value_from
            .strip_prefix(&format!("store:{site}/"))
            .filter(|key| !key.is_empty())
            .ok_or_else(|| AppError::api("data_invalid", 400))?;
        let mut vault = self
            .vault
            .lock()
            .map_err(|_| AppError::api("internal", 500))?;
        if let Some(value) = value {
            let value = value.trim();
            if value.is_empty() || value.len() > 16384 || value.contains(['\r', '\n']) {
                return Err(AppError::api("bad_request", 400));
            }
            vault.set(site, key, value)?;
        } else {
            vault.delete(site, key)?;
        }
        drop(vault);
        if value.is_some() {
            let origin = origin(if auth.origin.is_empty() {
                &sp.site.base_url
            } else {
                &auth.origin
            })?;
            self.policy.resume_auth(&format!("{origin}|"))?;
            for account in self
                .state
                .read()?
                .accounts
                .values()
                .filter(|account| account.site_id == site)
            {
                self.policy
                    .resume_auth(&format!("{origin}|{}", account.id))?;
            }
        }
        self.policy.invalidate(Some(site), None)?;
        self.changed();
        Ok(())
    }
    pub fn config(&self, id: &str) -> AppResult<ai::Config> {
        let data = self.state.read()?;
        let id = if id.is_empty() {
            data.settings.default_provider.as_str()
        } else {
            id
        };
        let provider = ai::provider(id).ok_or_else(|| AppError::api("provider_missing", 400))?;
        let settings = data
            .providers
            .get(id)
            .ok_or_else(|| AppError::api("provider_missing", 400))?;
        if !settings.connected {
            return Err(AppError::api("provider_missing", 400));
        }
        let key = {
            let mut vault = self
                .vault
                .lock()
                .map_err(|_| AppError::api("internal", 500))?;
            vault.reload()?;
            vault.get("providers", id).unwrap_or("").to_owned()
        };
        Ok(ai::Config {
            provider: id.into(),
            model: if settings.model.is_empty() {
                provider.default_model.into()
            } else {
                settings.model.clone()
            },
            key,
        })
    }
    pub fn providers(&self) -> AppResult<Vec<ai::ProviderView>> {
        let data = self.state.read()?;
        let mut vault = self
            .vault
            .lock()
            .map_err(|_| AppError::api("internal", 500))?;
        vault.reload()?;
        Ok(ai::PROVIDERS
            .iter()
            .map(|p| {
                let s = data.providers.get(p.id).cloned().unwrap_or_default();
                ai::ProviderView {
                    provider: p.clone(),
                    model: s.model,
                    connected: s.connected
                        && (p.id == "codex" || vault.contains("providers", p.id)),
                    has_key: vault.contains("providers", p.id),
                    models: s.models,
                    available: p.id != "codex" || self.ai.codex.available(),
                    account: if p.id == "codex" {
                        self.ai.codex.cached_account().unwrap_or_default()
                    } else {
                        String::new()
                    },
                }
            })
            .collect())
    }
    pub fn credential(&self, id: &str) -> AppResult<Credential> {
        let mut vault = self
            .vault
            .lock()
            .map_err(|_| AppError::api("internal", 500))?;
        vault.reload()?;
        serde_json::from_str(
            vault
                .get("accounts", id)
                .ok_or_else(|| AppError::api("auth_required", 401))?,
        )
        .map_err(Into::into)
    }
    pub fn update_credential<R>(
        &self,
        id: &str,
        mutate: impl FnOnce(&mut Credential) -> AppResult<R>,
    ) -> AppResult<(Credential, R)> {
        let mut vault = self
            .vault
            .lock()
            .map_err(|_| AppError::api("internal", 500))?;
        vault.reload()?;
        let mut credential: Credential = serde_json::from_str(
            vault
                .get("accounts", id)
                .ok_or_else(|| AppError::api("auth_required", 401))?,
        )?;
        let out = mutate(&mut credential)?;
        vault.set("accounts", id, &serde_json::to_string(&credential)?)?;
        Ok((credential, out))
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
        let load_credential = || {
            if account.is_empty() {
                crate::accounts::Credential {
                    origin: req_origin.clone(),
                    ..Default::default()
                }
            } else {
                self.credential(account)
                    .unwrap_or_else(|_| crate::accounts::Credential {
                        origin: req_origin.clone(),
                        ..Default::default()
                    })
            }
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
            let credential = load_credential();
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
        let r = self
            .network
            .fetch(Request {
                url: req_input.url.clone(),
                method: req_input.method.clone(),
                headers: req_input.headers.clone(),
                body: req_input.body.clone(),
                account_id: req_input.account_id.clone(),
                limit: req_input.limit,
                timeout: req_input.timeout,
                read_only: req_input.read_only,
                follow_redirects: req_input.follow_redirects,
                tls_profile: req_input.tls_profile.clone(),
            })
            .await?;
        if crate::defend::is_challenge_label(&r.defense) && !is_local_url {
            let _ = self.policy.resume_auth(&format!("{req_origin}|{account}"));
            let credential = load_credential();
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

    /// Deterministic fast recon for external agents (CLI / MCP) without waiting on an AI loop.
    pub async fn recon(
        &self,
        site: &str,
        url: &str,
        max_bundles: usize,
    ) -> AppResult<(Value, Spec)> {
        let parsed = crate::net::validate_url(url)?;
        let site_id = if site.is_empty() || site == "default" {
            site_id_for(&parsed)
        } else {
            site.to_string()
        };
        let account_id = self
            .state
            .read()?
            .accounts
            .values()
            .find(|a| {
                a.status == "connected"
                    && (a.site_id == site_id
                        || crate::net::validate_url(&a.site_url)
                            .is_ok_and(|u| crate::net::same_site_origin(&u, &parsed)))
            })
            .map(|a| a.id.clone())
            .unwrap_or_default();
        let r = self.read_page(url, &account_id).await?;
        let _ = self.kb.record_attempt(&AttemptInput {
            site: site_id.clone(),
            stage: "recon".into(),
            action: "recon_entry".into(),
            target_kind: "url".into(),
            target_key: r.url.clone(),
            status: r.status as i64,
            defense: r.defense.clone(),
            duration_ms: r.duration_ms,
            verdict: if r.successful() { "pass" } else { "fail" }.into(),
            ..Default::default()
        });
        let r_url = crate::net::validate_url(&r.url)?;
        let text = String::from_utf8_lossy(&r.bytes);
        let vis = visible_text(&text);
        let page_title = crate::html::Field {
            selector: "title".into(),
            attribute: String::new(),
        }
        .extract(&scraper::Html::parse_document(&text))
        .unwrap_or_default();
        let ssr_payloads = crate::html::ssr_payloads(&text);
        let ssr_map = crate::html::extract_ssr_map(&text);
        let doc_routes = crate::discovery::documented_routes(&vis);
        let mut bundle_routes = crate::discovery::bundle_routes(&text);
        let mut api_bases = crate::discovery::bundle_api_bases(&r_url, &text);
        let max_bundles = max_bundles.clamp(1, 24);
        let mut script_urls: Vec<String> = crate::discovery::bundle_urls(&r_url, &text)
            .into_iter()
            .collect();
        script_urls.sort_by_key(|u| {
            let lower = u.to_ascii_lowercase();
            if lower.contains("polyfill")
                || lower.contains("vendor")
                || lower.contains("runtime")
                || lower.contains("framework")
            {
                1usize
            } else {
                0usize
            }
        });
        let mut scanned_bundles = Vec::new();
        for bundle_url in script_urls.into_iter().take(max_bundles) {
            let Ok(parsed_bundle) = crate::net::validate_url(&bundle_url) else {
                continue;
            };
            let bundle_account = if crate::net::same_site_origin(&parsed_bundle, &r_url) {
                account_id.as_str()
            } else {
                ""
            };
            if let Ok(br) = self.read_page(&bundle_url, bundle_account).await {
                if br.successful() {
                    let btext = String::from_utf8_lossy(&br.bytes);
                    for route in crate::discovery::bundle_routes(&btext) {
                        if bundle_routes.len() < 96 {
                            bundle_routes.insert(route);
                        }
                    }
                    for base in crate::discovery::bundle_api_bases(&r_url, &btext) {
                        if api_bases.len() < 16 {
                            api_bases.insert(base);
                        }
                    }
                    scanned_bundles.push(bundle_url);
                }
            }
        }
        let origin_str = r_url.origin().ascii_serialization();
        let entry_path = if r_url.path().is_empty() {
            "/".to_string()
        } else {
            r_url.path().to_string()
        };
        let mut entry_query = BTreeMap::new();
        for (k, _) in r_url.query_pairs() {
            if util::valid_id(&k) && !util::secret_field(&k) {
                entry_query.insert(
                    k.into_owned(),
                    spec::Param {
                        kind: "string".into(),
                        required: false,
                        default: None,
                    },
                );
            }
        }
        if (entry_path.contains("search") || entry_path.contains("buy-sell"))
            && !entry_query.contains_key("q")
        {
            entry_query.insert(
                "q".into(),
                spec::Param {
                    kind: "string".into(),
                    required: false,
                    default: None,
                },
            );
        }
        let primary_base = api_bases
            .iter()
            .find(|b| {
                url::Url::parse(b)
                    .ok()
                    .and_then(|u| u.host_str().map(|h| h.to_ascii_lowercase()))
                    .is_some_and(|h| {
                        [
                            "api.",
                            "api-",
                            "v1.",
                            "v2.",
                            "v3.",
                            "gql.",
                            "graphql.",
                            "gateway.",
                            "backend.",
                            "studio-api",
                        ]
                        .iter()
                        .any(|p| h.starts_with(p))
                    })
            })
            .cloned()
            .unwrap_or_else(|| origin_str.clone());
        let primary_ssr = crate::html::SSR_GLOBAL_KEYS
            .iter()
            .copied()
            .find(|k| ssr_map.contains_key(*k))
            .unwrap_or("");

        let mut form_query_map: BTreeMap<String, BTreeMap<String, spec::Param>> = BTreeMap::new();
        let mut form_body_map: BTreeMap<(String, String), BTreeMap<String, spec::Param>> =
            BTreeMap::new();
        if let Some(forms_arr) = crate::html::forms(&text).as_array() {
            for f in forms_arr {
                let method = f
                    .get("method")
                    .and_then(Value::as_str)
                    .unwrap_or("GET")
                    .to_ascii_uppercase();
                if !["GET", "POST", "PUT", "PATCH", "DELETE"].contains(&method.as_str()) {
                    continue;
                }
                let action = f.get("action").and_then(Value::as_str).unwrap_or("");
                let Ok(action_url) = r_url.join(action) else {
                    continue;
                };
                if action_url.origin() != r_url.origin() {
                    continue;
                }
                let mut qmap = BTreeMap::new();
                if let Some(inputs) = f.get("inputs").and_then(Value::as_array) {
                    for inp in inputs {
                        if let Some(name) = inp.get("name").and_then(Value::as_str) {
                            if util::valid_id(name) && !util::secret_field(name) {
                                qmap.insert(
                                    name.to_string(),
                                    spec::Param {
                                        kind: "string".into(),
                                        required: false,
                                        default: None,
                                    },
                                );
                            }
                        }
                    }
                }
                let clean_path = action_url.path().trim_end_matches('/');
                let norm_path = if clean_path.is_empty() {
                    "/".to_string()
                } else {
                    clean_path.to_string()
                };
                if method == "GET" {
                    form_query_map
                        .entry(norm_path.clone())
                        .or_default()
                        .extend(qmap.clone());
                    form_query_map
                        .entry(action_url.path().to_string())
                        .or_default()
                        .extend(qmap);
                } else {
                    bundle_routes.insert((method.clone(), norm_path.clone()));
                    form_body_map
                        .entry((method.clone(), norm_path))
                        .or_default()
                        .extend(qmap.clone());
                    form_body_map
                        .entry((method, action_url.path().to_string()))
                        .or_default()
                        .extend(qmap);
                }
            }
        }
        if let Some(ld) = ssr_map.get("ld+json") {
            let tpl_opt = ld
                .pointer("/potentialAction/target/urlTemplate")
                .or_else(|| ld.pointer("/potentialAction/0/target/urlTemplate"))
                .and_then(Value::as_str);
            if let Some(tpl) = tpl_opt {
                let safe_tpl = tpl.replace('{', "_").replace('}', "_");
                if let Ok(tpl_url) = r_url.join(&safe_tpl) {
                    if tpl_url.origin() == r_url.origin() {
                        let mut qmap = BTreeMap::new();
                        for (k, _) in tpl_url.query_pairs() {
                            if util::valid_id(&k) && !util::secret_field(&k) {
                                qmap.insert(
                                    k.into_owned(),
                                    spec::Param {
                                        kind: "string".into(),
                                        required: false,
                                        default: None,
                                    },
                                );
                            }
                        }
                        let clean_path = tpl_url.path().trim_end_matches('/');
                        let norm_path = if clean_path.is_empty() {
                            "/".to_string()
                        } else {
                            clean_path.to_string()
                        };
                        form_query_map
                            .entry(norm_path)
                            .or_default()
                            .extend(qmap.clone());
                        form_query_map
                            .entry(tpl_url.path().to_string())
                            .or_default()
                            .extend(qmap);
                    }
                }
            }
        }

        let inferred_html = crate::html::infer_html_extraction(&text, &r.url);
        let mut operations = Vec::new();
        let mut used_names = std::collections::BTreeSet::new();
        if primary_ssr.is_empty() {
            if let Some(ref ext) = inferred_html {
                used_names.insert("read-entry".into());
                operations.push(spec::Operation {
                    name: "read-entry".into(),
                    method: "GET".into(),
                    path: entry_path.clone(),
                    base_url: origin_str.clone(),
                    query: entry_query.clone(),
                    desc: "Extract structured item list from entry page".into(),
                    effect: Effect::Read,
                    evidence: format!("Inferred repeating DOM item selector `{}`", ext.items),
                    response: Some(spec::ResponseExpectation {
                        format: "html".into(),
                        html: Some(ext.clone()),
                        ..Default::default()
                    }),
                    ..Default::default()
                });
                for nav_path in crate::html::nav_list_routes(&text, &r.url) {
                    let slug = nav_path
                        .trim_matches('/')
                        .replace(|c: char| !c.is_ascii_alphanumeric(), "-")
                        .to_ascii_lowercase();
                    let op_name = format!("get-{slug}");
                    if util::valid_id(&op_name) && used_names.insert(op_name.clone()) {
                        operations.push(spec::Operation {
                            name: op_name,
                            method: "GET".into(),
                            path: nav_path.clone(),
                            base_url: origin_str.clone(),
                            desc: format!("Extract structured item list from {nav_path}"),
                            effect: Effect::Read,
                            evidence: format!(
                                "Observed same-origin navigation route {nav_path} sharing `{}` item layout",
                                ext.items
                            ),
                            response: Some(spec::ResponseExpectation {
                                format: "html".into(),
                                html: Some(ext.clone()),
                                ..Default::default()
                            }),
                            ..Default::default()
                        });
                    }
                }
            }
        }
        for ssr_key in crate::html::SSR_GLOBAL_KEYS
            .iter()
            .copied()
            .chain(std::iter::once("ld+json"))
        {
            if let Some(doc) = ssr_map.get(ssr_key) {
                let slug = if ssr_key == "__remixContext" {
                    "remix-context".to_string()
                } else {
                    ssr_key
                        .trim_matches('_')
                        .to_ascii_lowercase()
                        .replace(|c: char| !c.is_ascii_alphanumeric(), "-")
                };
                let op_name = format!("read-{slug}");
                if util::valid_id(&op_name) && used_names.insert(op_name.clone()) {
                    let required_pointers = if doc.get("props").is_some() {
                        vec!["/props".to_string()]
                    } else {
                        vec![]
                    };
                    operations.push(spec::Operation {
                        name: op_name,
                        method: "GET".into(),
                        path: entry_path.clone(),
                        base_url: origin_str.clone(),
                        query: entry_query.clone(),
                        desc: format!("Extract embedded {ssr_key} SSR state from entry page"),
                        effect: Effect::Read,
                        evidence: format!("Observed {ssr_key} payload in entry HTML"),
                        response: Some(spec::ResponseExpectation {
                            format: "json".into(),
                            ssr_json: ssr_key.into(),
                            required_pointers,
                            ..Default::default()
                        }),
                        ..Default::default()
                    });
                }
            }
        }
        if ssr_map.contains_key("__remixContext")
            && (text.contains("/kr/buy-sell") || text.contains("/kr/search"))
            && used_names.insert("search-buy-sell".into())
        {
            operations.push(spec::Operation {
                name: "search-buy-sell".into(),
                method: "GET".into(),
                path: "/kr/search/".into(),
                base_url: origin_str.clone(),
                query: BTreeMap::from([
                    (
                        "q".into(),
                        spec::Param {
                            kind: "string".into(),
                            required: false,
                            default: None,
                        },
                    ),
                    (
                        "in".into(),
                        spec::Param {
                            kind: "string".into(),
                            required: false,
                            default: None,
                        },
                    ),
                ]),
                desc: "Search secondhand buy-sell listings".into(),
                effect: Effect::Read,
                evidence: "Observed /kr/search/ Remix route and __remixContext in HTML".into(),
                response: Some(spec::ResponseExpectation {
                    format: "json".into(),
                    ssr_json: "__remixContext".into(),
                    extract_pointer: "/routeData/buySellArticles".into(),
                    ..Default::default()
                }),
                ..Default::default()
            });
        }
        let passive_observations = self
            .state
            .read()
            .ok()
            .and_then(|d| d.sites.get(&site_id).map(|m| m.observations.clone()))
            .unwrap_or_default();
        for obs in &passive_observations {
            let method = obs.method.to_ascii_uppercase();
            if !["GET", "POST", "PUT", "PATCH", "DELETE"].contains(&method.as_str())
                || !obs.path.starts_with('/')
            {
                continue;
            }
            let slug = obs
                .path
                .trim_matches('/')
                .split('/')
                .filter(|seg| !seg.is_empty() && !seg.starts_with('{'))
                .collect::<Vec<_>>()
                .join("-")
                .replace(|c: char| !c.is_ascii_alphanumeric(), "-")
                .to_ascii_lowercase();
            let op_name = format!(
                "{}-{}",
                method.to_ascii_lowercase(),
                if slug.is_empty() { "root" } else { &slug }
            )
            .chars()
            .take(48)
            .collect::<String>()
            .trim_end_matches('-')
            .to_string();
            if !util::valid_id(&op_name) || !used_names.insert(op_name.clone()) {
                continue;
            }
            let query: BTreeMap<String, spec::Param> = obs
                .query
                .iter()
                .filter(|(k, _)| util::valid_id(k) && !util::secret_field(k))
                .map(|(k, kind)| {
                    let norm_kind = match kind.as_str() {
                        "int" | "float" | "bool" | "json" => kind.as_str(),
                        "number" => "float",
                        _ => "string",
                    };
                    (
                        k.clone(),
                        spec::Param {
                            kind: norm_kind.into(),
                            required: false,
                            default: None,
                        },
                    )
                })
                .collect();
            let body = if !obs.body.is_empty() && method != "GET" {
                let mut json_fields = BTreeMap::new();
                for (k, kind) in &obs.body {
                    if util::valid_id(k) && !util::secret_field(k) && !query.contains_key(k) {
                        let norm_kind = match kind.as_str() {
                            "int" | "float" | "bool" | "json" => kind.as_str(),
                            "number" => "float",
                            "array" | "object" => "json",
                            _ => "string",
                        };
                        json_fields.insert(
                            k.clone(),
                            spec::Param {
                                kind: norm_kind.into(),
                                required: true,
                                default: None,
                            },
                        );
                    }
                }
                if json_fields.is_empty() {
                    None
                } else {
                    Some(spec::Body {
                        json: json_fields,
                        ..Default::default()
                    })
                }
            } else {
                None
            };
            let is_read = method == "GET" && !policy::dangerous_intent(&obs.path);
            operations.push(spec::Operation {
                name: op_name,
                method: method.clone(),
                path: obs.path.clone(),
                query,
                body,
                desc: format!("{method} {}", obs.path),
                effect: if is_read { Effect::Read } else { Effect::Write },
                evidence: format!(
                    "Observed live browser network request ({} {})",
                    method, obs.path
                ),
                response: Some(spec::ResponseExpectation {
                    format: "json".into(),
                    required_pointers: obs.response_pointers.clone(),
                    ..Default::default()
                }),
                ..Default::default()
            });
            if operations.len() >= 24 {
                break;
            }
        }
        for (method, path) in doc_routes.iter().chain(bundle_routes.iter()) {
            let slug = path
                .trim_matches('/')
                .split('/')
                .filter(|seg| !seg.is_empty() && !seg.starts_with('{'))
                .collect::<Vec<_>>()
                .join("-")
                .replace(|c: char| !c.is_ascii_alphanumeric(), "-")
                .to_ascii_lowercase();
            let op_name = format!(
                "{}-{}",
                method.to_ascii_lowercase(),
                if slug.is_empty() { "root" } else { &slug }
            )
            .chars()
            .take(48)
            .collect::<String>()
            .trim_end_matches('-')
            .to_string();
            if !util::valid_id(&op_name) || !used_names.insert(op_name.clone()) {
                continue;
            }
            let route_key = format!("{method} {path}");
            let _ = self.kb.ensure_node(
                &site_id,
                "endpoint",
                &route_key,
                &json!({"method": method, "path": path}),
            );
            let mut params = BTreeMap::new();
            for seg in path.split('/') {
                if let Some(var) = seg.strip_prefix('{').and_then(|s| s.strip_suffix('}')) {
                    if util::valid_id(var) {
                        params.insert(
                            var.to_string(),
                            spec::Param {
                                kind: "string".into(),
                                required: true,
                                default: None,
                            },
                        );
                    }
                }
            }
            let mut query = if method == "GET" {
                form_query_map.get(path).cloned().unwrap_or_default()
            } else {
                BTreeMap::new()
            };
            if method == "GET" && path.contains("search") && !query.contains_key("q") {
                query.insert(
                    "q".into(),
                    spec::Param {
                        kind: "string".into(),
                        required: false,
                        default: None,
                    },
                );
            }
            let body = if method != "GET" {
                form_body_map
                    .get(&(method.clone(), path.clone()))
                    .filter(|fields| !fields.is_empty())
                    .map(|fields| spec::Body {
                        form: fields.clone(),
                        ..Default::default()
                    })
            } else {
                None
            };
            let is_api_route = path.starts_with("/api/")
                || path.starts_with("/v1/")
                || path.starts_with("/v2/")
                || path.starts_with("/v3/")
                || path.contains("graphql")
                || path.ends_with("/gql")
                || path.starts_with("/rest/")
                || path.starts_with("/_next/data/")
                || path.ends_with(".json")
                || doc_routes.contains(&(method.clone(), path.clone()));
            let is_read =
                ["GET", "HEAD"].contains(&method.as_str()) && !policy::dangerous_intent(path);
            let response = if is_api_route {
                spec::ResponseExpectation {
                    format: "json".into(),
                    ..Default::default()
                }
            } else if !primary_ssr.is_empty() {
                spec::ResponseExpectation {
                    format: "json".into(),
                    ssr_json: primary_ssr.into(),
                    ..Default::default()
                }
            } else {
                spec::ResponseExpectation {
                    format: "text".into(),
                    ..Default::default()
                }
            };
            operations.push(spec::Operation {
                name: op_name,
                method: method.to_string(),
                path: path.to_string(),
                base_url: if path.starts_with("/api/") && primary_base != origin_str {
                    primary_base.clone()
                } else {
                    String::new()
                },
                params,
                query,
                body,
                desc: route_key.clone(),
                effect: if is_read { Effect::Read } else { Effect::Write },
                evidence: format!("Discovered {route_key} during deterministic recon"),
                response: Some(response),
                ..Default::default()
            });
            if operations.len() >= 24 {
                break;
            }
        }
        if !used_names.contains("read-entry") && (operations.is_empty() || primary_ssr.is_empty()) {
            operations.push(spec::Operation {
                name: "read-entry".into(),
                method: "GET".into(),
                path: entry_path,
                query: entry_query,
                desc: "Read entry page".into(),
                effect: Effect::Read,
                evidence: "Observed entry page during recon".into(),
                response: Some(spec::ResponseExpectation {
                    format: if r.header("content-type").contains("json") {
                        "json"
                    } else {
                        "text"
                    }
                    .into(),
                    ..Default::default()
                }),
                ..Default::default()
            });
        }
        let draft = Spec {
            spec_version: 2,
            site: spec::SiteInfo {
                name: site_id.clone(),
                title: if page_title.is_empty() {
                    humanize(&site_id)
                } else {
                    page_title.clone()
                },
                base_url: origin_str.clone(),
                source_url: r.url.clone(),
            },
            defaults: spec::Defaults {
                tls_profile: "go".into(),
                ..Default::default()
            },
            auth: vec![],
            models: BTreeMap::new(),
            operations,
            health: None,
        };
        let doc_route_strs: Vec<String> =
            doc_routes.iter().map(|(m, p)| format!("{m} {p}")).collect();
        let bundle_route_strs: Vec<String> = bundle_routes
            .iter()
            .map(|(m, p)| format!("{m} {p}"))
            .collect();
        let secrets = if account_id.is_empty() {
            vec![]
        } else {
            self.credential(&account_id)?.redaction_values()
        };
        let report = sanitized(
            json!({
                "site_id": site_id,
                "url": r.url,
                "status": r.status,
                "defense": r.defense,
                "latency_ms": r.duration_ms,
                "title": page_title,
                "account_id": account_id,
                "api_bases": api_bases,
                "documented_routes": doc_route_strs,
                "bundle_routes": bundle_route_strs,
                "scanned_bundles": scanned_bundles,
                "ssr_payloads": ssr_payloads,
                "html_extraction": inferred_html,
                "forms": crate::html::forms(&text),
                "draft_operations": draft.operations.iter().map(|o| json!({
                    "name": o.name,
                    "method": o.method,
                    "path": o.path,
                    "base_url": draft.operation_base(o),
                })).collect::<Vec<_>>(),
            }),
            &secrets,
        );
        Ok((report, draft))
    }

    pub fn connect_account(
        &self,
        site_id: &str,
        url: &str,
        cookies: Vec<accounts::Cookie>,
        browser: &str,
        profile: &str,
        identity_source: &str,
    ) -> AppResult<AccountView> {
        let url = crate::net::validate_url(url)?;
        let cookies = accounts::normalize(cookies, &url)?;
        let site_id = if site_id.is_empty() {
            site_id_for(&url)
        } else {
            if origin(self.spec(site_id)?.site.source_url())? != url.origin().ascii_serialization()
            {
                return Err(AppError::api("bad_url", 400));
            }
            site_id.into()
        };
        let credential = Credential {
            origin: url.origin().ascii_serialization(),
            cookies,
            identity_source: identity_source.into(),
            browser_session: String::new(),
            extension_origin: String::new(),
            browser_session_expires_ms: 0,
            source_profile: String::new(),
            local_storage: BTreeMap::new(),
            headers: BTreeMap::new(),
            recipe: None,
        };
        self.register_browser_credential(&site_id, credential, browser, profile)
    }
    pub(crate) fn register_browser_credential(
        &self,
        site_id: &str,
        credential: Credential,
        browser: &str,
        profile: &str,
    ) -> AppResult<AccountView> {
        let id = util::id();
        let site_id = site_id.to_string();
        let view = AccountView {
            id: id.clone(),
            site_id: site_id.clone(),
            site_url: credential.origin.clone(),
            label: String::new(),
            browser: browser.chars().take(80).collect(),
            profile: profile.chars().take(80).collect(),
            status: "connected".into(),
            checked_at: String::new(),
            cookie_count: credential.cookies.len(),
            created_at: util::now(),
            ..Default::default()
        };
        self.vault
            .lock()
            .map_err(|_| AppError::api("internal", 500))?
            .set("accounts", &id, &serde_json::to_string(&credential)?)?;
        self.state.update(|d| {
            d.accounts.insert(id.clone(), view.clone());
            let m = d.sites.entry(site_id).or_default();
            if m.account_id.is_empty() {
                m.account_id = id;
            }
            Ok(())
        })?;
        self.changed();
        Ok(view)
    }
    pub async fn refresh_account(&self, id: &str) -> AppResult<AccountView> {
        let mut view = self
            .state
            .read()?
            .accounts
            .get(id)
            .cloned()
            .ok_or_else(|| AppError::api("not_found", 404))?;
        let (c, _) = self.update_credential(id, |c| {
            if let Some(recipe) = &c.recipe {
                c.headers.extend(recipe.resolve_headers(c)?);
            }
            Ok(())
        })?;
        self.policy.resume_auth(&format!("{}|{id}", c.origin))?;
        // Only a URL actually observed in this user's browser or a verified read operation.
        let source = if !c.identity_source.is_empty() {
            c.identity_source.clone()
        } else {
            self.state
                .read()?
                .sites
                .get(&view.site_id)
                .and_then(|m| m.identity_paths.first())
                .cloned()
                .unwrap_or_default()
        };
        if source.is_empty() {
            return Err(AppError::api("identity_unavailable", 422));
        }
        if !source.is_empty() {
            let parsed = crate::net::validate_url(&source)?;
            if parsed.origin().ascii_serialization() != c.origin
                || (c.recipe.is_none() && !accounts::identity_endpoint(&parsed))
            {
                return Err(AppError::api("data_invalid", 400));
            }
            let r = self.read_page(&source, id).await?;
            if r.successful() {
                let identity = if let Some(recipe) = &c.recipe {
                    recipe.identity_from_response(&r)
                } else {
                    r.json().as_ref().and_then(accounts::extract_identity)
                };
                if let Some(identity) = identity {
                    accounts::apply_identity(&mut view, identity);
                } else {
                    view.email.clear();
                    view.user_id.clear();
                    view.name.clear();
                    view.label.clear();
                    view.verified_identity = false;
                }
            } else if r.status == 401 || r.status == 403 {
                view.status = "expired".into();
                view.verified_identity = false;
            }
        }
        view.checked_at = util::now();
        self.state.update(|d| {
            d.accounts.insert(id.into(), view.clone());
            Ok(())
        })?;
        self.changed();
        if !view.verified_identity && view.status == "connected" {
            return Err(AppError::api("identity_unavailable", 422));
        }
        Ok(view)
    }
}
pub fn params(op: &Operation) -> Vec<(&String, &spec::Param)> {
    op.params
        .iter()
        .chain(op.query.iter())
        .chain(op.body.iter().flat_map(|b| b.inputs()))
        .collect()
}
pub fn origin(raw: &str) -> AppResult<String> {
    Ok(crate::net::validate_url(raw)?
        .origin()
        .ascii_serialization())
}
pub fn same_site_url(a: &str, b: &str) -> AppResult<bool> {
    let ua = crate::net::validate_url(a)?;
    let ub = crate::net::validate_url(b)?;
    Ok(crate::net::same_site_origin(&ua, &ub))
}
pub fn site_id_for(url: &url::Url) -> String {
    let host = url
        .host_str()
        .unwrap_or("website")
        .replace(['.', ':', '[', ']'], "-");
    format!(
        "site-{}-{}",
        host.trim_matches('-').chars().take(40).collect::<String>(),
        &util::hash(url.origin().ascii_serialization().as_bytes())[..8]
    )
}
pub fn humanize(value: &str) -> String {
    let words = value.replace(['_', '-'], " ");
    let mut chars = words.chars();
    match chars.next() {
        Some(c) => c.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}
pub fn visible_text(html: &str) -> String {
    // A word-break opportunity carries no whitespace. Replacing it with a space
    // splits documented paths such as /teams<wbr/>/{team}<wbr/>/items.
    let unbroken = regex::Regex::new(r"(?is)<wbr\b[^>]*>")
        .expect("constant")
        .replace_all(html, "");
    // Documentation components often carry the HTTP verb as an attribute and
    // the route in their contents. Preserve that evidence before stripping tags.
    let methods = regex::Regex::new(r#"(?is)<[A-Za-z][A-Za-z0-9_.:-]*\b[^>]*\bmethod\s*=\s*["'](GET|HEAD|POST|PUT|PATCH|DELETE)["'][^>]*>"#)
        .expect("constant")
        .replace_all(&unbroken, " $1 ");
    let without =
        regex::Regex::new(r"(?is)<(script|style|noscript)[^>]*>.*?</(?:script|style|noscript)\s*>")
            .expect("constant")
            .replace_all(&methods, " ");
    let tags = regex::Regex::new(r"(?s)<[^>]*>")
        .expect("constant")
        .replace_all(&without, " ");
    util::redact_text(
        &tags
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .replace("&amp;", "&")
            .replace("&lt;", "<")
            .replace("&gt;", ">")
            .replace("&quot;", "\"")
            .chars()
            .take(120_000)
            .collect::<String>(),
    )
}

pub fn sanitized(mut value: Value, secrets: &[String]) -> Value {
    util::redact(&mut value, secrets);
    value
}
