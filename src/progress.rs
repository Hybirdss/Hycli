//! Public work notes describe observable activity, never private model reasoning or request URLs.
use crate::{
    apperr::AppResult,
    runtime::Runtime,
    state::{Job, JobAgent, JobNote},
    util,
};

pub fn public_note(value: &str) -> String {
    let text = util::redact_text(value).replace(['\n', '\r', '\t'], " ");
    // Model summaries are optional. Prefer a safe template whenever a summary contains
    // technical addresses, code, secret placeholders or likely request details.
    let lower = text.to_ascii_lowercase();
    if text.len() > 700
        || [
            "http",
            "www.",
            "/",
            "\\",
            "`",
            "[redacted",
            "bearer ",
            "api.",
            "endpoint",
            "authorization",
            "cookie",
            "token",
            "password",
        ]
        .iter()
        .any(|p| lower.contains(p))
    {
        return String::new();
    }
    text.trim().chars().take(200).collect()
}

fn failure_note_code(code: &str) -> &'static str {
    match code {
        "ai_response_incomplete" => "agent_failed_ai_response_incomplete",
        "ai_response_invalid" => "agent_failed_ai_response_invalid",
        "ai_connection_failed" => "agent_failed_ai_connection_failed",
        "ai_unavailable" => "agent_failed_ai_unavailable",
        "ai_refused" => "agent_failed_ai_refused",
        "rate_limited" => "agent_failed_rate_limited",
        "timeout" => "agent_failed_timeout",
        "credentials_invalid" => "agent_failed_credentials_invalid",
        "provider_missing" => "agent_failed_provider_missing",
        "model_unavailable" => "agent_failed_model_unavailable",
        "insufficient_balance" => "agent_failed_insufficient_balance",
        "ai_billing_error" => "agent_failed_ai_billing_error",
        "ai_permission_denied" => "agent_failed_ai_permission_denied",
        "ai_request_invalid" => "agent_failed_ai_request_invalid",
        "cancelled" => "agent_failed_cancelled",
        _ => "agent_failed",
    }
}

impl Job {
    pub fn note(&mut self, code: &str, agent: &str, count: usize, summary: &str) {
        let summary = public_note(summary);
        let public_code = if !summary.is_empty() {
            "agent_summary"
        } else {
            code
        };
        let message = if !summary.is_empty() {
            summary
        } else {
            match code {
                "understanding" => "Understanding the website and selecting useful complete workflows.".into(),
                "workflow_planned" => "Selected the user outcomes and the steps needed to complete them.".into(),
                "workflow_review" => "Checking every workflow step, required input and final result.".into(),
                "workflow_incomplete" => "Some workflow steps are missing. Exploring further approaches.".into(),
                "coverage_incomplete" => format!("{count} documented operations have no action yet. Adding them."),
                "environment_checked" => "Checked this device and its available browsers.".into(),
                "tool_started" => "The agent is trying the next available method.".into(),
                "tool_completed" => "The method returned a result for the agent to inspect.".into(),
                "tool_failed" => "This method could not finish. The agent will use the result to choose another approach.".into(),
                "connection_verified" => "Verified the signed-in account with the website.".into(),
                "browser_search" => "Looking for an existing website sign-in.".into(),
                "browser_found" => "Found an existing browser session.".into(),
                "browser_missing" => {
                    "No connected sign-in was found. Continuing with public information.".into()
                }
                "started" => "Started preparing your website.".into(),
                "reading" => "Reading information published by the website.".into(),
                "read" => format!("Read {count} useful pages."),
                "agents_started" => "Three agents are working together.".into(),
                "agent_queued" => "Waiting for an available agent.".into(),
                "agent_started" => "Reviewing the available information.".into(),
                "agent_more" => "Found another useful source to check.".into(),
                "agent_retry" => "Refining the proposed actions.".into(),
                "agent_done" => format!("Prepared {count} supported actions."),
                "agent_failed" => "This agent could not finish. Keeping the other findings.".into(),
                code if code.starts_with("agent_failed_") => crate::apperr::AppError::api(code.trim_start_matches("agent_failed_"), 502).remedy,
                "ai_stopped" => "The AI stopped before finishing. Keeping the actions found so far.".into(),
                "found" => format!("Found {count} useful actions."),
                "checking" => "Checking the actions that can be safely read.".into(),
                "checked" => format!("Checked {count} read actions."),
                "icon_found" => "Found the website's official image.".into(),
                "icon_missing" => "The website does not have a usable image yet.".into(),
                "describing" => "Writing clear descriptions of the available actions.".into(),
                "summarizing" => "Summarizing your result.".into(),
                "running" => "Running the requested action.".into(),
                "completed" => "Finished. Your result is ready.".into(),
                "failed" => "Stopped because this work could not be completed.".into(),
                "cancelled" => "Stopped at your request.".into(),
                "interrupted" => "This work was interrupted when the app stopped.".into(),
                _ => "Working on your request.".into(),
            }
        };
        let seq = self.notes.last().map_or(1, |n| n.seq + 1);
        self.notes.push(JobNote {
            seq,
            at: util::now(),
            code: public_code.into(),
            message,
            agent: agent.into(),
            count,
        });
        if self.notes.len() > 96 {
            self.notes.remove(1);
        }
    }
}
impl Runtime {
    pub(crate) fn job_note(
        &self,
        id: &str,
        code: &str,
        agent: &str,
        count: usize,
        summary: &str,
    ) -> AppResult<()> {
        self.state.update_job(id, |job| {
            if job.status == "running" {
                job.note(code, agent, count, summary);
            }
        })?;
        self.changed();
        Ok(())
    }
    pub(crate) fn job_progress(&self, id: &str, phase: &str, done: usize) -> AppResult<()> {
        self.state.update_job(id, |job| {
            if job.status == "running" {
                job.phase = phase.into();
                job.progress_done = job.progress_done.max(done).min(job.progress_total);
            }
        })?;
        self.changed();
        Ok(())
    }
    pub(crate) fn agent_progress(
        &self,
        id: &str,
        agent_id: &str,
        status: &str,
        found: usize,
        summary: &str,
        failure: Option<&crate::apperr::AppError>,
    ) -> AppResult<()> {
        self.state.update_job(id, |job| {
            if job.status != "running" {
                return;
            }
            if let Some(agent) = job.agents.iter_mut().find(|a| a.id == agent_id) {
                agent.status = status.into();
                agent.found = found;
                if agent.started_at.is_empty() && status == "running" {
                    agent.started_at = util::now();
                }
                if ["completed", "failed"].contains(&status) {
                    agent.finished_at = util::now();
                }
            } else {
                job.agents.push(JobAgent {
                    id: agent_id.into(),
                    status: status.into(),
                    found,
                    started_at: if status == "running" {
                        util::now()
                    } else {
                        String::new()
                    },
                    finished_at: String::new(),
                });
            }
            job.note(
                match status {
                    "queued" => "agent_queued",
                    "completed" => "agent_done",
                    "failed" => failure.map_or("agent_failed", |error| {
                        failure_note_code(&error.public_code)
                    }),
                    _ => "agent_started",
                },
                agent_id,
                found,
                summary,
            );
        })?;
        self.changed();
        Ok(())
    }
}

struct ObservedJobGuard<'a> {
    core: &'a Runtime,
    job_id: String,
    active: bool,
}

impl Drop for ObservedJobGuard<'_> {
    fn drop(&mut self) {
        if !self.active {
            return;
        }
        let _ = self.core.state.update_job(&self.job_id, |work| {
            if !["queued", "running"].contains(&work.status.as_str()) {
                return;
            }
            let note_code = if work.is_outcome_uncertain() {
                "failed"
            } else {
                "cancelled"
            };
            work.mark_interrupted(note_code);
        });
        self.core.changed();
    }
}

impl Runtime {
    /// Terminal and MCP actions have the same visible work record and recovery guarantees as dashboard actions.
    pub(crate) async fn execute_observed(
        &self,
        request: crate::policy::ExecutionRequest,
        locale: &str,
    ) -> AppResult<(crate::runtime::OpResult, i64)> {
        let mut job = Job::new("action", locale);
        job.site_id = request.site_id.clone();
        job.action_id = request.action_id.clone();
        job.account_id = request.account_id.clone();
        if let Ok(sp) = self.spec(&request.site_id)
            && let Ok(op) = sp.op(&request.action_id)
        {
            job.effect = crate::policy::operation_effect(op, &request.inputs);
            if op.effect == crate::policy::Effect::Read && job.effect != crate::policy::Effect::Read
            {
                return Err(crate::apperr::AppError::api("approval_required", 409));
            }
            if job.effect != crate::policy::Effect::Read {
                self.ensure_writes(&request.site_id)?;
            }
        }
        if let Some(site) = self
            .sites(locale)?
            .into_iter()
            .find(|site| site.id == job.site_id)
        {
            job.site_title = site.title;
            if let Some(action) = site
                .actions
                .into_iter()
                .find(|action| action.id == job.action_id)
            {
                job.action_title = action.title;
                if job.effect == crate::policy::Effect::Read {
                    job.effect = action.effect;
                }
            }
        }
        job.status = "running".into();
        job.phase = "run".into();
        job.dispatch_phase = crate::state::DispatchPhase::PreDispatch;
        job.owner_pid = std::process::id();
        job.progress_done = 1;
        job.note("running", "", 0, "");
        self.state.add_job(job.clone())?;
        self.changed();
        let mut guard = ObservedJobGuard {
            core: self,
            job_id: job.id.clone(),
            active: true,
        };
        let mut rx = self.events.subscribe();
        let cancelled = async {
            loop {
                tokio::select! {
                    _ = rx.recv() => {}
                    _ = tokio::time::sleep(std::time::Duration::from_millis(100)) => {}
                }
                if let Ok(current) = self.state.job(&job.id)
                    && ["cancelled", "failed"].contains(&current.status.as_str())
                {
                    return if !current.error_code.is_empty() {
                        current.error_code
                    } else if current.status == "cancelled" {
                        "cancelled".into()
                    } else {
                        "action_outcome_unknown".into()
                    };
                }
            }
        };
        let result = tokio::select! {
            value = self.execute_for_job(request, None, Some(&job.id)) => value.and_then(|result| {
                result.0.ensure_success()?;
                Ok(result)
            }),
            code = cancelled => Err(crate::apperr::AppError::api(&code, 409)),
        };
        guard.active = false;
        // Successful actions were conditionally completed by the same runtime
        // finalizer that still owned their artifact guard. Do not persist twice.
        if result.is_ok() {
            return result;
        }
        let persisted = self.state.update_job(&job.id, |work| {
            if !["queued", "running"].contains(&work.status.as_str()) {
                return;
            }
            work.finished_at = util::now();
            match &result {
                Ok(_) => return, // Runtime conditionally persisted the full action outcome.
                Err(error) => {
                    work.status = "failed".into();
                    work.phase = "failed".into();
                    work.error_code = error.public_code.clone();
                    work.note("failed", "", 0, "");
                }
            }
        });
        if let Err(error) = persisted {
            let error = if result
                .as_ref()
                .is_err_and(|error| error.public_code == "action_outcome_unknown")
                || (job.is_write_action()
                    && result
                        .as_ref()
                        .is_ok_and(|(value, _)| value.artifact.is_some()))
            {
                crate::apperr::AppError::api("action_outcome_unknown", 502)
            } else {
                error
            };
            let _ = self.state.update_job(&job.id, |work| {
                if ["queued", "running"].contains(&work.status.as_str()) {
                    work.status = "failed".into();
                    work.phase = "failed".into();
                    work.error_code = error.public_code.clone();
                    work.finished_at = util::now();
                    work.result = None;
                }
            });
            self.changed();
            return Err(error);
        }
        self.changed();
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn worker_failure_notes_have_nonempty_static_remedies() {
        for code in [
            "ai_response_incomplete",
            "ai_response_invalid",
            "ai_connection_failed",
            "ai_unavailable",
            "ai_refused",
            "rate_limited",
            "timeout",
            "credentials_invalid",
            "provider_missing",
            "model_unavailable",
            "insufficient_balance",
            "ai_billing_error",
            "ai_permission_denied",
            "ai_request_invalid",
            "cancelled",
        ] {
            let mut job = Job::new("prepare", "en");
            job.note(failure_note_code(code), "explore", 0, "");
            assert!(!job.notes[0].message.is_empty(), "{code}");
            assert_eq!(job.notes[0].code, format!("agent_failed_{code}"));
        }
        assert_eq!(
            failure_note_code("private-unknown-provider-body"),
            "agent_failed"
        );
    }
    #[test]
    fn public_notes_never_accept_addresses_or_request_details() {
        for value in [
            "Reading https://private.example/api/items",
            "GET /api/items",
            "Authorization Bearer private",
            "cookie sid=private",
            "password=private",
            "`fetch code`",
        ] {
            assert!(public_note(value).is_empty());
        }
        assert_eq!(
            public_note("Found a way to search your notes."),
            "Found a way to search your notes."
        );
        let mut job = Job::new("prepare", "en");
        for _ in 0..120 {
            job.note("reading", "", 0, "");
        }
        assert_eq!(job.notes.len(), 96);
        assert_eq!(job.notes[0].seq, 1);
        assert_eq!(job.notes.last().unwrap().seq, 120);
    }
}
