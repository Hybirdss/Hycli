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
                    "failed" => "agent_failed",
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

impl Runtime {
    /// Terminal and MCP reads have the same visible work record as dashboard actions.
    pub(crate) async fn execute_observed(
        &self,
        request: crate::policy::ExecutionRequest,
        locale: &str,
    ) -> AppResult<(crate::runtime::OpResult, i64)> {
        let mut job = Job::new("action", locale);
        job.site_id = request.site_id.clone();
        job.action_id = request.action_id.clone();
        job.account_id = request.account_id.clone();
        if let Some(site) = self
            .sites(locale)?
            .into_iter()
            .find(|site| site.id == job.site_id)
        {
            job.site_title = site.title;
            job.action_title = site
                .actions
                .into_iter()
                .find(|action| action.id == job.action_id)
                .map(|action| action.title)
                .unwrap_or_default();
        }
        job.status = "running".into();
        job.phase = "run".into();
        job.progress_done = 1;
        job.note("running", "", 0, "");
        self.state.add_job(job.clone())?;
        self.changed();
        let cancelled = async {
            loop {
                tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                if self
                    .state
                    .job(&job.id)
                    .is_ok_and(|job| job.status == "cancelled")
                {
                    return;
                }
            }
        };
        let result = tokio::select! {
            value = self.execute(request, None) => value.and_then(|result| {
                result.0.ensure_success()?;
                Ok(result)
            }),
            () = cancelled => Err(crate::apperr::AppError::api("cancelled", 409)),
        };
        self.state.update_job(&job.id, |work| {
            if work.status == "cancelled" {
                return;
            }
            work.finished_at = util::now();
            match &result {
                Ok((value, _)) if (200..300).contains(&value.status) && value.defense == "none" => {
                    work.status = "completed".into();
                    work.phase = "ready".into();
                    work.progress_done = work.progress_total;
                    work.result = value.mapped.clone().or_else(|| value.body.clone());
                    work.note("completed", "", 0, "");
                }
                Ok(_) => {
                    work.status = "failed".into();
                    work.phase = "failed".into();
                    work.error_code = "operation_failed".into();
                    work.note("failed", "", 0, "");
                }
                Err(error) => {
                    work.status = "failed".into();
                    work.phase = "failed".into();
                    work.error_code = error.public_code.clone();
                    work.note("failed", "", 0, "");
                }
            }
        })?;
        self.changed();
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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
