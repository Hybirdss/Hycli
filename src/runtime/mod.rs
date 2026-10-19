//! The single execution boundary used by the dashboard, CLI and MCP.
mod accounts;
mod execution;
mod preflight;
mod recon;
mod transport;
mod views;

use crate::{
    ai,
    apperr::{AppError, AppResult},
    kb::Kb,
    net::Network,
    policy::{self, Effect},
    spec::{self, Operation, Spec},
    state::StateStore,
    store::Store,
    util,
};
use serde::Serialize;
use serde_json::Value;
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
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
