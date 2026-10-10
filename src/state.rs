use crate::{
    accounts::AccountView,
    apperr::{AppError, AppResult},
    util,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::Mutex,
};

#[derive(Clone, Serialize, Deserialize)]
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
#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ProviderSettings {
    pub model: String,
    pub connected: bool,
    pub models: Vec<String>,
    pub checked_at: String,
}
#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct InputDescription {
    pub label: String,
    pub hint: String,
}
#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ActionDescription {
    pub title: String,
    pub description: String,
    pub output: String,
    pub inputs: BTreeMap<String, InputDescription>,
}
#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct SiteDescription {
    pub title: String,
    pub summary: String,
    pub actions: BTreeMap<String, ActionDescription>,
    pub hash: String,
}
#[derive(Clone, Default, Serialize, Deserialize)]
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
#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct WebsiteUnderstanding {
    pub summary: String,
    pub workflows: Vec<WebsiteWorkflow>,
}
#[derive(Clone, Default, Serialize, Deserialize)]
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
}
#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct JobAgent {
    pub id: String,
    pub status: String,
    pub found: usize,
    pub started_at: String,
    pub finished_at: String,
}
#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct JobNote {
    pub seq: u64,
    pub at: String,
    pub code: String,
    pub message: String,
    pub agent: String,
    pub count: usize,
}
#[derive(Clone, Serialize, Deserialize)]
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
        }
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
pub struct StateStore {
    path: PathBuf,
    data: Mutex<Data>,
}
impl StateStore {
    pub fn open(path: &Path) -> AppResult<Self> {
        Self::open_with_recovery(path, true)
    }
    pub fn open_with_recovery(path: &Path, recover: bool) -> AppResult<Self> {
        let store = Self {
            path: path.to_owned(),
            data: Mutex::new(Data::default()),
        };
        store.update(|data| {
            if recover {
                for job in &mut data.jobs {
                    if ["queued", "running"].contains(&job.status.as_str()) {
                        let uncertain = job.kind == "action" && !job.approval_id.is_empty();
                        job.status = if uncertain { "failed" } else { "cancelled" }.into();
                        job.phase = job.status.clone();
                        job.error_code = if uncertain {
                            "action_outcome_unknown"
                        } else {
                            "cancelled"
                        }
                        .into();
                        job.finished_at = util::now();
                        job.updated_at = util::now();
                        for agent in &mut job.agents {
                            if ["running", "queued"].contains(&agent.status.as_str()) {
                                agent.status = "cancelled".into();
                                agent.finished_at = util::now();
                            }
                        }
                        job.note("interrupted", "", 0, "");
                    }
                }
            }
            Ok(())
        })?;
        Ok(store)
    }
    pub fn read(&self) -> AppResult<Data> {
        if self.path.exists() {
            Ok(serde_json::from_slice(&util::read_bounded(
                &self.path,
                32 * 1024 * 1024,
            )?)?)
        } else {
            Ok(Data::default())
        }
    }
    pub fn update<T>(&self, apply: impl FnOnce(&mut Data) -> AppResult<T>) -> AppResult<T> {
        let mut guard = self
            .data
            .lock()
            .map_err(|_| AppError::api("internal", 500))?;
        let _file_lock = util::file_lock(&self.path.with_extension("lock"), false)?;
        let mut next = self.read()?;
        let old_jobs: Vec<_> = next.jobs.iter().map(|j| j.id.clone()).collect();
        let result = apply(&mut next)?;
        if next.jobs.len() > 500 {
            let mut finished = 0;
            next.jobs.retain(|j| {
                if ["running", "queued"].contains(&j.status.as_str()) {
                    true
                } else {
                    finished += 1;
                    finished <= 400
                }
            });
        }
        for job in &mut next.jobs {
            if let Some(value) = job.result.take() {
                let bytes = serde_json::to_vec(&value)?;
                if bytes.len() > 32 * 1024 * 1024 {
                    return Err(AppError::api("too_large", 413));
                }
                util::atomic_file(&self.result_path(&job.id)?, &bytes)?;
            }
        }
        let bytes = serde_json::to_vec(&next)?;
        if bytes.len() > 32 * 1024 * 1024 {
            return Err(AppError::api("too_large", 413));
        }
        util::atomic_file(&self.path, &bytes)?;
        for id in old_jobs {
            if !next.jobs.iter().any(|j| j.id == id) {
                let _ = std::fs::remove_file(self.result_path(&id)?);
            }
        }
        *guard = next;
        Ok(result)
    }
    pub fn add_job(&self, job: Job) -> AppResult<()> {
        self.update(|data| {
            if data.jobs.iter().any(|old| {
                ["running", "queued"].contains(&old.status.as_str())
                    && old.kind == job.kind
                    && match job.kind.as_str() {
                        "prepare" => old.site_id == job.site_id,
                        "describe" => old.site_id == job.site_id && old.locale == job.locale,
                        "summary" => old.parent_id == job.parent_id,
                        _ => false,
                    }
            }) {
                return Err(AppError::api("job_running", 409));
            }
            data.jobs.insert(0, job);
            Ok(())
        })
    }
    fn result_path(&self, id: &str) -> AppResult<PathBuf> {
        // Job IDs are UUID hex strings and may begin with a digit, unlike site IDs.
        if id.len() != 32
            || !id
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return Err(AppError::api("not_found", 404));
        }
        Ok(self
            .path
            .parent()
            .unwrap_or(Path::new("."))
            .join("results")
            .join(format!("{id}.json")))
    }
    pub fn job(&self, id: &str) -> AppResult<Job> {
        let mut job = self
            .read()?
            .jobs
            .into_iter()
            .find(|j| j.id == id)
            .ok_or_else(|| AppError::api("not_found", 404))?;
        if job.result.is_none() {
            let path = self.result_path(id)?;
            if path.exists() {
                job.result = Some(serde_json::from_slice(&util::read_bounded(
                    &path,
                    32 * 1024 * 1024,
                )?)?);
            }
        }
        Ok(job)
    }
    pub fn update_job(&self, id: &str, apply: impl FnOnce(&mut Job)) -> AppResult<()> {
        self.update(|data| {
            let job = data
                .jobs
                .iter_mut()
                .find(|j| j.id == id)
                .ok_or_else(|| AppError::api("not_found", 404))?;
            apply(job);
            job.updated_at = util::now();
            Ok(())
        })
    }
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
