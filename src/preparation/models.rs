use super::{grounding::verification_key, prompts::WORKERS};
use crate::{
    ai,
    apperr::{AppError, AppResult},
    policy::{self, Effect},
    runtime::{self, Runtime},
    smoke::{self, ReadCheck},
    spec::Spec,
    state::{Job, SiteDescription, WebsiteUnderstanding},
    util,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Serialize)]
pub(super) struct Source {
    pub(super) url: String,
    pub(super) kind: String,
    pub(super) content: Value,
}

#[derive(Clone, Deserialize, Default)]
#[serde(default)]
pub(super) struct Plan {
    pub(super) understanding: Option<WebsiteUnderstanding>,
    pub(super) workflow_review: Option<WorkflowReview>,
    pub(super) self_qa: Vec<crate::state::SelfQa>,
    pub(super) read_urls: Vec<String>,
    #[serde(alias = "tool_calls")]
    pub(super) calls: Vec<crate::agent_tools::ToolCall>,
    pub(super) spec: Option<Spec>,
    pub(super) description: SiteDescription,
    pub(super) note: String,
    pub(super) capability_gaps: Vec<String>,
}

#[derive(Clone, Default, Deserialize, Serialize)]
#[serde(default)]
pub(super) struct WorkflowReview {
    pub(super) decision: String,
    pub(super) workflows: Vec<WorkflowCoverage>,
    pub(super) next_steps: Vec<String>,
    pub(super) read_checks: Vec<ReadCheck>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(super) self_qa: Vec<crate::state::SelfQa>,
}

#[derive(Clone, Default, Deserialize, Serialize)]
#[serde(default)]
pub(super) struct WorkflowCoverage {
    pub(super) id: String,
    pub(super) steps: Vec<StepCoverage>,
}

#[derive(Clone, Default, Deserialize, Serialize)]
#[serde(default)]
pub(super) struct StepCoverage {
    pub(super) step: usize,
    pub(super) actions: Vec<String>,
    pub(super) input_sources: BTreeMap<String, String>,
    pub(super) result_check: String,
    pub(super) blocker: String,
    pub(super) attempts: Vec<String>,
}

pub(super) fn step_covered(step: &StepCoverage, spec: &Spec) -> bool {
    step.blocker.trim().is_empty()
        && !step.result_check.trim().is_empty()
        && !step.actions.is_empty()
        && step.actions.iter().all(|name| {
            spec.op(name).is_ok_and(|action| {
                runtime::params(action)
                    .iter()
                    .filter(|(_, param)| param.required)
                    .all(|(id, _)| {
                        step.input_sources
                            .get(&format!("{name}.{id}"))
                            .or_else(|| step.input_sources.get(*id))
                            .is_some_and(|source| !source.trim().is_empty())
                    })
            })
        })
}

pub(super) fn complete_workflows(
    plan: &WebsiteUnderstanding,
    review: &WorkflowReview,
    spec: &Spec,
) -> usize {
    plan.workflows
        .iter()
        .filter(|workflow| {
            let matches: Vec<_> = review
                .workflows
                .iter()
                .filter(|item| item.id == workflow.id)
                .collect();
            if matches.len() != 1 || matches[0].steps.len() != workflow.steps.len() {
                return false;
            }
            (1..=workflow.steps.len()).all(|index| {
                let steps: Vec<_> = matches[0]
                    .steps
                    .iter()
                    .filter(|step| step.step == index)
                    .collect();
                steps.len() == 1 && step_covered(steps[0], spec)
            })
        })
        .count()
}

pub(super) fn workflow_review_feedback(
    plan: &WebsiteUnderstanding,
    spec: &Spec,
    workflow_review: Option<&WorkflowReview>,
    sources: &[Source],
    reviewing: bool,
) -> Option<String> {
    let complete = workflow_review.is_some_and(|review| {
        review.decision == "complete"
            && complete_workflows(plan, review, spec) == plan.workflows.len()
    });
    let grounded_blocker = workflow_review.is_some_and(|review| {
        review.decision == "blocked"
            && plan.workflows.iter().all(|workflow| {
                let entries: Vec<_> = review
                    .workflows
                    .iter()
                    .filter(|item| item.id == workflow.id)
                    .collect();
                entries.len() == 1
                    && entries[0].steps.len() == workflow.steps.len()
                    && (1..=workflow.steps.len()).all(|index| {
                        let steps: Vec<_> = entries[0]
                            .steps
                            .iter()
                            .filter(|step| step.step == index)
                            .collect();
                        if steps.len() != 1 {
                            return false;
                        }
                        let step = steps[0];
                        if step.blocker.trim().is_empty() {
                            return step_covered(step, spec);
                        }
                        step.attempts
                            .iter()
                            .filter(|attempt| sources.iter().any(|source| &source.url == *attempt))
                            .collect::<BTreeSet<_>>()
                            .len()
                            >= 2
                    })
            })
    });
    let requested_reads =
        reviewing && workflow_review.is_some_and(|review| !review.read_checks.is_empty());
    if complete || grounded_blocker || requested_reads {
        return None;
    }
    let next = workflow_review
        .map(|review| {
            let mut lines = review.next_steps.clone();
            for qa in &review.self_qa {
                if !qa.action_taken.trim().is_empty() {
                    lines.push(format!(
                        "[{}] {} -> {}",
                        qa.lens, qa.question, qa.action_taken
                    ));
                }
            }
            lines.join("\n")
        })
        .unwrap_or_default();
    Some(format!(
        "The end-to-end review found unfinished work. Revisit every planned step and supply the missing capabilities and input discovery; do not stop at the existing subset. {next}"
    ))
}

pub(super) fn valid_understanding(value: &WebsiteUnderstanding) -> bool {
    !value.summary.trim().is_empty()
        && !value.workflows.is_empty()
        && value.workflows.len() <= 12
        && value
            .workflows
            .iter()
            .map(|workflow| &workflow.id)
            .collect::<BTreeSet<_>>()
            .len()
            == value.workflows.len()
        && value.workflows.iter().all(|workflow| {
            !workflow.id.trim().is_empty()
                && !workflow.title.trim().is_empty()
                && !workflow.benefit.trim().is_empty()
                && !workflow.success_criteria.trim().is_empty()
                && !workflow.steps.is_empty()
                && workflow.steps.len() <= 12
                && workflow.steps.iter().all(|step| !step.trim().is_empty())
        })
}

pub(super) fn normalize_self_qa(container: &mut serde_json::Map<String, Value>) {
    if container.get("self_qa").is_some_and(|v| !v.is_array()) {
        container.remove("self_qa");
        return;
    }
    if let Some(items) = container.get_mut("self_qa").and_then(Value::as_array_mut) {
        items.retain(Value::is_object);
        for item in items.iter_mut().filter_map(Value::as_object_mut) {
            for key in ["lens", "question", "evidence", "answer", "action_taken"] {
                if item.get(key).is_some_and(|v| !v.is_string()) {
                    item.remove(key);
                }
            }
        }
    }
}

pub(super) fn redact_self_qa(items: &mut Vec<crate::state::SelfQa>) {
    items.truncate(12);
    for item in items.iter_mut() {
        item.redact();
    }
    items.retain(|item| !item.question.trim().is_empty() || !item.answer.trim().is_empty());
}

pub(super) fn decode_plan(text: &str) -> AppResult<Plan> {
    let mut value: Value = ai::decode_json(text)?;
    // Providers commonly express an omitted optional section as null. Normalize only
    // empty planning sections; operation validation remains strict.
    if let Some(root) = value.as_object_mut() {
        normalize_self_qa(root);
    }
    for field in ["calls", "tool_calls", "read_urls"] {
        if value.get(field).is_some_and(Value::is_null) {
            value.as_object_mut().unwrap().remove(field);
        }
    }
    if value.get("description").is_some_and(Value::is_null) {
        value.as_object_mut().unwrap().remove("description");
    }
    if value
        .pointer("/description/actions")
        .is_some_and(Value::is_null)
    {
        value["description"]["actions"] = json!({});
    }
    if let Some(understanding) = value
        .get_mut("understanding")
        .and_then(Value::as_object_mut)
    {
        normalize_self_qa(understanding);
    }
    if let Some(review) = value
        .get_mut("workflow_review")
        .and_then(Value::as_object_mut)
    {
        normalize_self_qa(review);
        for key in ["read_checks", "next_steps", "workflows"] {
            if review.get(key).is_some_and(Value::is_null) {
                review.remove(key);
            }
        }
        if let Some(workflows) = review.get_mut("workflows").and_then(Value::as_array_mut) {
            for workflow in workflows {
                if let Some(steps) = workflow.get_mut("steps").and_then(Value::as_array_mut) {
                    for step in steps.iter_mut().filter_map(Value::as_object_mut) {
                        for key in [
                            "actions",
                            "input_sources",
                            "result_check",
                            "blocker",
                            "attempts",
                        ] {
                            if step.get(key).is_some_and(Value::is_null) {
                                step.remove(key);
                            }
                        }
                    }
                }
            }
        }
    }
    serde_json::from_value(value).map_err(|_| AppError::api("ai_response_invalid", 502))
}

impl Runtime {
    #[allow(clippy::too_many_arguments)]
    pub(super) async fn verify_candidate_reads(
        &self,
        job: &Job,
        account_id: &str,
        proposed: &mut Spec,
        description: &mut SiteDescription,
        workflow_review: &mut Option<WorkflowReview>,
        review_requested: &mut bool,
        verification: &mut BTreeMap<String, bool>,
        read_attempts: &mut BTreeSet<String>,
        read_results: &mut BTreeMap<String, Value>,
        smoke_checks: &mut Vec<ReadCheck>,
        attempted: &mut usize,
        repair_rounds: &mut usize,
        sources: &mut Vec<Source>,
        feedback: &mut BTreeMap<String, String>,
    ) -> AppResult<bool> {
        self.job_progress(&job.id, "verify", 3)?;
        self.job_note(&job.id, "checking", "", 0, "")?;
        let mut failures = Vec::new();
        let mut failed_names = BTreeSet::new();
        read_results.retain(|name, _| {
            proposed
                .op(name)
                .ok()
                .and_then(|operation| verification_key(proposed, operation).ok())
                .is_some_and(|key| verification.get(&key) == Some(&true))
        });
        let mut checks = workflow_review
            .as_ref()
            .map(|review| review.read_checks.clone())
            .unwrap_or_default();
        let explicit: BTreeSet<_> = checks.iter().map(|check| check.action.clone()).collect();
        checks.extend(
            proposed
                .operations
                .iter()
                .filter(|operation| !explicit.contains(&operation.name))
                .map(|operation| ReadCheck {
                    action: operation.name.clone(),
                    inputs: BTreeMap::new(),
                }),
        );
        let before_attempts = *attempted;
        for check in checks.into_iter().take(24) {
            if *attempted >= 24 {
                break;
            }
            let Ok(operation) = proposed.op(&check.action) else {
                continue;
            };
            let Some(inputs) = smoke::resolve(&check, read_results) else {
                continue;
            };
            if policy::operation_effect(operation, &inputs) != Effect::Read
                || runtime::params(operation)
                    .iter()
                    .any(|(id, param)| param.required && !inputs.contains_key(*id))
            {
                continue;
            }
            let key = verification_key(proposed, operation)?;
            let attempt_key = util::hash(&serde_json::to_vec(&(&key, &inputs))?);
            if read_attempts.contains(&attempt_key) {
                continue;
            }
            let account = if runtime::same_site_url(proposed.operation_base(operation), &job.url)
                .unwrap_or(false)
            {
                account_id
            } else {
                ""
            };
            // Missing credentials are an access requirement, not a failed test request.
            if self
                .request(proposed, &operation.name, inputs.clone(), Some(account))
                .is_err_and(|error| error.public_code == "auth_required")
            {
                continue;
            }
            read_attempts.insert(attempt_key);
            *attempted += 1;
            let result = self
                .verify_candidate_with_inputs(proposed, &operation.name, account, inputs)
                .await;
            let passed = result
                .as_ref()
                .is_ok_and(|(result, _)| result.ensure_success().is_ok());
            verification.insert(key, passed);
            let body = result.as_ref().ok().and_then(|(response, _)| {
                response.mapped.clone().or_else(|| response.body.clone())
            });
            sources.push(Source {
                url: format!("read-check:{}", *attempted),
                kind: "Actual workflow read check".into(),
                content: json!({
                    "action": operation.name,
                    "passed": passed,
                    "status": result.as_ref().ok().map(|(response, _)| response.status),
                    "error": result.as_ref().err().map(|error| &error.public_code),
                    "response_shape": body.as_ref().map(|value| crate::session_recipe::shape(value, 0)),
                    "values_exposed": false
                }),
            });
            if passed {
                smoke_checks.retain(|item| item.action != check.action);
                smoke_checks.push(check.clone());
                if let Some(body) = body {
                    read_results.insert(operation.name.clone(), body);
                }
                continue;
            }
            read_results.remove(&operation.name);
            let (code, status) = match &result {
                Ok((response, _)) => (
                    response
                        .ensure_success()
                        .err()
                        .map(|error| error.public_code)
                        .unwrap_or_default(),
                    response.status,
                ),
                Err(error) => (error.public_code.clone(), error.http_status),
            };
            if code == "auth_required" && account.is_empty() {
                continue;
            }
            let strategy_remedy = match code.as_str() {
                "blocked_by_site" => {
                    "Direct HTTP was challenged on this endpoint. Do NOT repeat the exact same direct HTTP operation; set `transport: \"browser-fetch\"` (in-page fetch) or `transport: \"browser\"` (rendered HTML), or pivot to an SSR JSON / alternative API route."
                }
                "auth_required" => {
                    "Endpoint returned 401; verify derived auth header/cookie mapping or preflight token extraction, or set `transport: \"browser-fetch\"`."
                }
                "browser_unavailable" => {
                    "Browser engine unavailable for rendered transport; use `transport: \"http\"` with SSR JSON extraction or derived headers."
                }
                "site_rate_limited" | "rate_limited" => {
                    "Origin rate-limited (429); avoid repeating this endpoint in the current burst."
                }
                _ => {
                    "Inspect the response shape and status, adjust path/query/headers/response selectors, and do not repeat the exact same failing definition."
                }
            };
            failures.push(json!({
                "action": operation.name,
                "method": operation.method,
                "path": operation.path,
                "transport": if operation.transport.is_empty() { "http" } else { &operation.transport },
                "error": code,
                "status": status,
                "strategy_remedy": strategy_remedy,
                "expected_response": operation.response,
                "response": result.as_ref().ok().and_then(|(response, _)| response.body.clone())
            }));
            failed_names.insert(operation.name.clone());
            if ["site_rate_limited", "rate_limited"].contains(&code.as_str()) {
                break;
            }
        }
        if !failures.is_empty() && *repair_rounds < 2 && *attempted < 24 {
            *repair_rounds += 1;
            *workflow_review = None;
            // Let revised operations replace rejected candidates instead of being deduplicated away.
            proposed
                .operations
                .retain(|operation| !failed_names.contains(&operation.name));
            description
                .actions
                .retain(|name, _| !failed_names.contains(name));
            sources.push(Source {
                url: format!("verification:{}", *repair_rounds),
                kind: "Read verification failures; revise using observed evidence".into(),
                content: json!({"failures":failures}),
            });
            for (role, _) in WORKERS {
                feedback.insert(
                    role.into(),
                    "A proposed read failed its actual pre-installation check. Inspect `strategy_remedy` and the verification evidence: if direct HTTP was challenged (`blocked_by_site`), pivot strategy by setting `transport: \"browser-fetch\"` or `transport: \"browser\"` or switching to SSR JSON extraction. Never repeat the exact same failing operation definition or invent a successful result. Mutations must never be tested.".into(),
                );
            }
            self.job_note(&job.id, "agent_retry", "", *repair_rounds, "")?;
            return Ok(true);
        }
        if *attempted > before_attempts {
            // The reviewer must see actual results before it can finish.
            *workflow_review = None;
            *review_requested = true;
            return Ok(true);
        }
        let missing_reads: BTreeSet<_> = workflow_review
            .as_ref()
            .into_iter()
            .flat_map(|review| &review.workflows)
            .flat_map(|workflow| &workflow.steps)
            .filter(|step| step.blocker.trim().is_empty())
            .flat_map(|step| &step.actions)
            .filter(|name| {
                proposed.op(name).is_ok_and(|operation| {
                    policy::operation_effect(operation, &BTreeMap::new()) == Effect::Read
                        && verification_key(proposed, operation)
                            .is_ok_and(|key| verification.get(&key) != Some(&true))
                })
            })
            .cloned()
            .collect();
        if !missing_reads.is_empty() && *attempted < 24 {
            feedback.insert(
                "review".into(),
                format!(
                    "These workflow reads still have no successful execution: {}. Supply evidence-based read_checks with all required inputs and valid earlier-result references, or explicitly document the observed blocker and alternative attempts. Do not declare complete.",
                    missing_reads.into_iter().collect::<Vec<_>>().join(", ")
                ),
            );
            *review_requested = true;
            return Ok(true);
        }
        Ok(false)
    }
}

/// Report missing auth declarations per operation, not per spec.
/// This is diagnostic only: public writes can be intentional, selected browser
/// credentials can still apply, and declaring auth does not prove it is configured.
pub(super) fn writes_without_declared_auth(sp: &Spec) -> Vec<String> {
    sp.operations
        .iter()
        .filter(|op| {
            policy::operation_effect(op, &std::collections::BTreeMap::new()) != Effect::Read
                && sp
                    .auth
                    .iter()
                    .find(|strategy| strategy.name == op.auth)
                    .is_none_or(|strategy| strategy.kind == "none")
        })
        .map(|op| op.name.clone())
        .collect()
}
