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
        let codex = Arc::new(ai::codex::Codex::new(root.join("provider-workspace")));
        let (events, _) = tokio::sync::broadcast::channel(100);
        Ok(Arc::new(Self {
            _instance: instance,
            root: root.into(),
            specs_dir,
            state: StateStore::open_with_recovery(&root.join("dashboard.json"), recover)?,
            vault: Mutex::new(Store::open_with_keyring(&root.join("store.json"), keyring)?),
            policy: policy.clone(),
            network: Network {
                policy,
                allow_local,
            },
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
                        accepts_account: origin(sp.operation_base(op)).ok()
                            == origin(sp.site.source_url()).ok()
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
        let data = self.state.read()?;
        let explicit_account = account.is_some_and(|value| !value.is_empty());
        let mut account = account.map(str::to_owned).unwrap_or_else(|| {
            data.sites
                .get(&sp.site.name)
                .map(|s| s.account_id.clone())
                .unwrap_or_default()
        });
        if !account.is_empty() {
            let a = data
                .accounts
                .get(&account)
                .ok_or_else(|| AppError::api("auth_required", 401))?;
            if a.site_id != sp.site.name || origin(&a.site_url)? != origin(&url)? {
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
    pub fn approval(&self, request: ExecutionRequest, locale: &str) -> AppResult<Approval> {
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
        let account = self
            .state
            .read()?
            .accounts
            .get(&request.account_id)
            .map(|a| a.label.clone())
            .unwrap_or_default();
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
        let request = self.request(sp, op, args.clone(), account)?;
        if policy::operation_effect(sp.op(op)?, args) != Effect::Read {
            let review = self.approval(request, "en")?;
            return Err(crate::apperr::usage(
                format!("User approval required: {}", review.id),
                "Open the Hycli dashboard and review this exact action. No request was sent.",
            ));
        }
        self.execute_observed(request, &self.state.read()?.settings.locale)
            .await
    }
    /// Approval IDs are accepted only through the authenticated dashboard handler.
    pub(crate) async fn execute(
        &self,
        request: ExecutionRequest,
        approval: Option<&str>,
    ) -> AppResult<(OpResult, i64)> {
        let sp = self.spec(&request.site_id)?;
        self.execute_definition(&sp, request, approval, true).await
    }
    /// Verify a proposed read without replacing the user's installed definition.
    pub(crate) async fn verify_candidate(
        &self,
        sp: &Spec,
        action: &str,
        account: &str,
    ) -> AppResult<(OpResult, i64)> {
        self.verify_candidate_with_inputs(sp, action, account, BTreeMap::new())
            .await
    }
    pub(crate) async fn verify_candidate_with_inputs(
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
        self.execute_definition(sp, request, None, false).await
    }
    async fn execute_definition(
        &self,
        sp: &Spec,
        request: ExecutionRequest,
        approval: Option<&str>,
        installed: bool,
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
        let effect = policy::operation_effect(op, &request.inputs);
        let (url, body) = sp.build_request(op, &request.inputs)?;
        if origin(&url)? != origin(sp.operation_base(op))? {
            return Err(AppError::api("unsafe_target", 400));
        }
        let (headers, secrets) = self.headers(&sp, op, &url, &request.account_id)?;
        checked.credential_hash = util::hash(&serde_json::to_vec(&headers)?);
        if effect != Effect::Read {
            self.policy.consume(
                approval.ok_or_else(|| AppError::api("approval_required", 409))?,
                &checked,
            )?;
        }
        let response = if op.transport == "browser" {
            let credential = if request.account_id.is_empty() {
                crate::accounts::Credential {
                    origin: origin(&url)?,
                    ..Default::default()
                }
            } else {
                self.credential(&request.account_id)?
            };
            self.network
                .fetch_rendered(&credential, &url, &request.account_id)
                .await
        } else {
            self.network
                .fetch(Request {
                    url,
                    method: op.method.clone(),
                    headers,
                    body,
                    account_id: request.account_id.clone(),
                    limit: 8 * 1024 * 1024,
                    timeout: Duration::from_secs(45),
                    read_only: effect == Effect::Read,
                    follow_redirects: effect == Effect::Read,
                    tls_profile: sp.defaults.tls_profile.clone(),
                })
                .await
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
                return Err(if effect != Effect::Read {
                    AppError::api("action_outcome_unknown", 502)
                } else {
                    e
                });
            }
        };
        let response_shape = op.matches_response(&response);
        let query_error = response.json().is_some_and(|body| {
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
        let raw = extracted.or_else(|| response.json()).unwrap_or_else(
            || json!({"text":visible_text(&String::from_utf8_lossy(&response.bytes))}),
        );
        let clean = sanitized(raw.clone(), &secrets);
        let mapped = if op.model.is_empty() {
            None
        } else {
            Some(sanitized(
                sp.apply_model(&op.model, &response.bytes)?,
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
                if response.status == 401 || response.status == 403 {
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
                .read()?
                .accounts
                .get(account)
                .is_some_and(|view| view.verified_identity && view.status == "connected")
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
        let mut secrets = vault.values_for_redaction();
        if !account.is_empty() {
            let credential: Credential = serde_json::from_str(
                vault
                    .get("accounts", account)
                    .ok_or_else(|| AppError::api("auth_required", 401))?,
            )?;
            let session_headers = credential.request_headers(url)?;
            if session_headers.is_empty() {
                return Err(AppError::api("auth_required", 401));
            }
            secrets.extend(credential.redaction_values());
            headers.extend(session_headers);
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
        self.network
            .fetch(Request {
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
            })
            .await
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
    /// 스텔스 런치 관찰 — 원장 기록 포함. probe와 동일 계약.
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
        let mut c = self.credential(id)?;
        if let Some(recipe) = &c.recipe {
            c.headers.extend(recipe.resolve_headers(&c)?);
            self.vault
                .lock()
                .map_err(|_| AppError::api("internal", 500))?
                .set("accounts", id, &serde_json::to_string(&c)?)?;
        }
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
