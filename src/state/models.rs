use crate::{accounts::AccountView, util};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

pub(super) const MAX_PAYLOAD_BYTES: usize = 32 * 1024 * 1024;
pub(super) const MAX_JOBS_TOTAL: usize = 500;
pub(super) const MAX_JOBS_FINISHED: usize = 400;

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct Settings {
    pub locale: String,
    pub theme: String,
    pub reduce_motion: bool,
    pub notifications: bool,
    pub default_provider: String,
    pub engine_path: String,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            locale: "en".into(),
            theme: "light".into(),
            reduce_motion: false,
            notifications: false,
            default_provider: String::new(),
            engine_path: String::new(),
        }
    }
}
#[derive(Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct ProviderSettings {
    pub model: String,
    pub connected: bool,
    pub models: Vec<String>,
    pub checked_at: String,
}
#[derive(Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct InputDescription {
    pub label: String,
    pub hint: String,
}
#[derive(Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct ActionDescription {
    pub title: String,
    pub description: String,
    pub output: String,
    pub inputs: BTreeMap<String, InputDescription>,
}
#[derive(Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct SiteDescription {
    pub title: String,
    pub summary: String,
    pub actions: BTreeMap<String, ActionDescription>,
    pub hash: String,
}
#[derive(Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct PreparationReport {
    pub os: String,
    pub architecture: String,
    pub sources: usize,
    pub tool_calls: usize,
    pub attempted_reads: usize,
    pub verified_reads: usize,
    pub account_verified: bool,
    pub workflows_total: usize,
    pub workflows_complete: usize,
}
#[derive(Clone, Default, Serialize, Deserialize, PartialEq, Eq, Debug)]
#[serde(default)]
pub struct SelfQa {
    pub lens: String,
    pub question: String,
    pub evidence: String,
    pub answer: String,
    pub action_taken: String,
}
impl SelfQa {
    pub fn redact(&mut self) {
        self.lens = util::redact_text(&self.lens);
        self.question = util::redact_text(&self.question);
        self.evidence = util::redact_text(&self.evidence);
        self.answer = util::redact_text(&self.answer);
        self.action_taken = util::redact_text(&self.action_taken);
    }
}
#[derive(Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct WebsiteUnderstanding {
    pub summary: String,
    pub workflows: Vec<WebsiteWorkflow>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub self_qa: Vec<SelfQa>,
}
#[derive(Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct WebsiteWorkflow {
    pub id: String,
    pub title: String,
    pub benefit: String,
    pub steps: Vec<String>,
    pub success_criteria: String,
}
#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct SiteMeta {
    pub workflows_complete: Option<bool>,
    pub pinned: bool,
    pub title: String,
    pub created_at: String,
    pub updated_at: String,
    pub last_used: String,
    pub account_id: String,
    pub descriptions: BTreeMap<String, SiteDescription>,
    pub verified: BTreeMap<String, bool>,
    pub identity_paths: Vec<String>,
    pub observations: Vec<crate::accounts::Observation>,
    pub connection_recipe: Option<crate::session_recipe::ConnectionRecipe>,
    pub preparation: Option<PreparationReport>,
    pub icon_type: String,
    pub icon_source: String,
    /// Off until confirmed once for this CLI (`hycli writes <site> allow`, MCP `hycli_writes`, or dashboard toggle).
    pub writes: bool,
    /// Reads that passed during preparation, replayed by `hycli check` without AI.
    pub smoke: Vec<crate::smoke::ReadCheck>,
}
#[derive(Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct JobAgent {
    pub id: String,
    pub status: String,
    pub found: usize,
    pub started_at: String,
    pub finished_at: String,
}
#[derive(Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct JobNote {
    pub seq: u64,
    pub at: String,
    pub code: String,
    pub message: String,
    pub agent: String,
    pub count: usize,
}
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DispatchPhase {
    #[default]
    Pending,
    PreDispatch,
    Dispatched,
}
impl DispatchPhase {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::PreDispatch => "pre_dispatch",
            Self::Dispatched => "dispatched",
        }
    }
    pub fn from_str(value: &str) -> Self {
        match value {
            "pre_dispatch" => Self::PreDispatch,
            "dispatched" => Self::Dispatched,
            _ => Self::Pending,
        }
    }
}
fn is_default_effect(effect: &crate::policy::Effect) -> bool {
    *effect == crate::policy::Effect::Read
}
fn is_default_dispatch_phase(phase: &DispatchPhase) -> bool {
    *phase == DispatchPhase::Pending
}
fn is_zero_pid(pid: &u32) -> bool {
    *pid == 0
}
#[derive(Clone, Serialize, Deserialize, PartialEq)]
pub struct Job {
    pub id: String,
    pub kind: String,
    pub site_id: String,
    pub site_title: String,
    pub url: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub intent: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub understanding: Option<WebsiteUnderstanding>,
    pub action_id: String,
    pub action_title: String,
    pub provider: String,
    pub locale: String,
    pub account_id: String,
    pub status: String,
    pub phase: String,
    pub error_code: String,
    pub found: Vec<String>,
    pub started_at: String,
    pub updated_at: String,
    pub finished_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    pub summary: String,
    #[serde(default)]
    pub approval_id: String,
    #[serde(default)]
    pub progress_done: usize,
    #[serde(default = "default_progress_total")]
    pub progress_total: usize,
    #[serde(default)]
    pub agents: Vec<JobAgent>,
    #[serde(default)]
    pub notes: Vec<JobNote>,
    #[serde(default)]
    pub parent_id: String,
    #[serde(default, skip_serializing_if = "is_default_effect")]
    pub effect: crate::policy::Effect,
    #[serde(default, skip_serializing_if = "is_default_dispatch_phase")]
    pub dispatch_phase: DispatchPhase,
    #[serde(default, skip_serializing_if = "is_zero_pid")]
    pub owner_pid: u32,
}
fn default_progress_total() -> usize {
    5
}
impl Job {
    pub fn new(kind: &str, locale: &str) -> Self {
        Self {
            id: util::id(),
            kind: kind.into(),
            site_id: String::new(),
            site_title: String::new(),
            url: String::new(),
            intent: String::new(),
            understanding: None,
            action_id: String::new(),
            action_title: String::new(),
            provider: String::new(),
            locale: locale_or_default(locale).into(),
            account_id: String::new(),
            status: "queued".into(),
            phase: "queued".into(),
            error_code: String::new(),
            found: vec![],
            started_at: util::now(),
            updated_at: util::now(),
            finished_at: String::new(),
            result: None,
            summary: String::new(),
            approval_id: String::new(),
            progress_done: 0,
            progress_total: if kind == "prepare" { 5 } else { 2 },
            agents: vec![],
            notes: vec![],
            parent_id: String::new(),
            effect: crate::policy::Effect::Read,
            dispatch_phase: DispatchPhase::Pending,
            owner_pid: 0,
        }
    }
    pub fn is_write_action(&self) -> bool {
        self.kind == "action"
            && (self.effect != crate::policy::Effect::Read || !self.approval_id.is_empty())
    }
    pub fn is_outcome_uncertain(&self) -> bool {
        self.is_write_action() && self.dispatch_phase != DispatchPhase::PreDispatch
    }
    pub fn mark_interrupted(&mut self, note_code: &str) {
        let uncertain = self.is_outcome_uncertain();
        self.status = if uncertain { "failed" } else { "cancelled" }.into();
        self.phase = self.status.clone();
        if uncertain {
            self.error_code = "action_outcome_unknown".into();
        } else if self.error_code.is_empty() || note_code == "interrupted" {
            self.error_code = "cancelled".into();
        }
        let now = util::now();
        self.finished_at = now.clone();
        self.updated_at = now.clone();
        for agent in &mut self.agents {
            if ["running", "queued"].contains(&agent.status.as_str()) {
                agent.status = "cancelled".into();
                agent.finished_at = now.clone();
            }
        }
        self.note(note_code, "", 0, "");
    }
}
#[derive(Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct Data {
    pub settings: Settings,
    pub providers: BTreeMap<String, ProviderSettings>,
    pub sites: BTreeMap<String, SiteMeta>,
    pub accounts: BTreeMap<String, AccountView>,
    pub jobs: Vec<Job>,
}

pub const LOCALES: [(&str, &str); 20] = [
    ("en", "English"),
    ("ko", "Korean"),
    ("ja", "Japanese"),
    ("zh-CN", "Simplified Chinese (Mainland China)"),
    ("zh-TW", "Traditional Chinese (Taiwan)"),
    ("es", "Spanish"),
    ("fr", "French"),
    ("de", "German"),
    ("pt-BR", "Brazilian Portuguese"),
    ("id", "Indonesian"),
    ("it", "Italian"),
    ("nl", "Dutch"),
    ("pl", "Polish"),
    ("tr", "Turkish"),
    ("ru", "Russian"),
    ("uk", "Ukrainian"),
    ("vi", "Vietnamese"),
    ("th", "Thai"),
    ("ar", "Modern Standard Arabic"),
    ("hi", "Hindi"),
];
pub fn locale_or_default(locale: &str) -> &str {
    let mut choices: Vec<_> = locale
        .split(',')
        .filter_map(|entry| {
            let mut fields = entry.trim().split(';');
            let tag = fields.next()?.trim();
            let quality = fields
                .find_map(|part| {
                    part.trim()
                        .strip_prefix("q=")
                        .and_then(|v| v.parse::<f32>().ok())
                })
                .unwrap_or(1.0);
            (quality.is_finite() && quality > 0.0 && quality <= 1.0).then_some((tag, quality))
        })
        .collect();
    choices.sort_by(|a, b| b.1.total_cmp(&a.1));
    for (tag, _) in choices {
        if let Some((id, _)) = LOCALES.iter().find(|(id, _)| id.eq_ignore_ascii_case(tag)) {
            return id;
        }
        let base = tag.split('-').next().unwrap_or(tag);
        if let Some((id, _)) = LOCALES.iter().find(|(id, _)| id.eq_ignore_ascii_case(base)) {
            return id;
        }
        if base.eq_ignore_ascii_case("pt") {
            return "pt-BR";
        }
    }
    "en"
}
pub fn language_name(locale: &str) -> &str {
    LOCALES
        .iter()
        .find(|(id, _)| *id == locale)
        .map(|(_, name)| *name)
        .unwrap_or("English")
}
