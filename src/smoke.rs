//! Token-free read checks recorded while a website is prepared. Replaying them tells
//! an expired sign-in apart from a website whose responses changed, without AI.
use crate::{
    apperr::AppResult,
    policy::{self, Effect},
    runtime::{self, Runtime},
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

/// One read with sample inputs. `$result:<action>:<json-pointer>` takes a value from an
/// earlier check's response, so private identifiers are resolved again on every run.
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq)]
#[serde(default)]
pub struct ReadCheck {
    pub action: String,
    #[serde(deserialize_with = "plain_values")]
    pub inputs: BTreeMap<String, String>,
}

fn plain_values<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<BTreeMap<String, String>, D::Error> {
    let values = Option::<BTreeMap<String, Value>>::deserialize(deserializer)?.unwrap_or_default();
    Ok(values
        .into_iter()
        .map(|(key, value)| {
            (
                key,
                match value {
                    Value::String(text) => text,
                    value => value.to_string(),
                },
            )
        })
        .collect())
}

/// References keep private identifiers in the broker. Models see response shapes,
/// choose an exact JSON pointer, and never need the private leaf value itself.
pub(crate) fn resolve(
    check: &ReadCheck,
    results: &BTreeMap<String, Value>,
) -> Option<BTreeMap<String, String>> {
    check
        .inputs
        .iter()
        .map(|(key, value)| {
            let value = if let Some(reference) = value.strip_prefix("$result:") {
                let (action, pointer) = reference.split_once(':')?;
                if !pointer.is_empty() && !pointer.starts_with('/') {
                    return None;
                }
                match results.get(action)?.pointer(pointer)? {
                    Value::Null | Value::Array(_) | Value::Object(_) => return None,
                    Value::String(text) => text.clone(),
                    value => value.to_string(),
                }
            } else {
                value.clone()
            };
            Some((key.clone(), value))
        })
        .collect()
}

#[derive(Debug, Serialize)]
pub struct CheckResult {
    pub action: String,
    /// ok | signed_out | site_changed | blocked | unreachable | skipped
    pub outcome: &'static str,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub error: String,
}

#[derive(Debug, Serialize)]
pub struct SiteCheck {
    pub site: String,
    /// ok | signed_out | site_changed | blocked | unreachable | unknown | no_checks
    pub verdict: &'static str,
    /// verified | signed_out | unverified, or absent for a website without an account.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub account: Option<&'static str>,
    pub checks: Vec<CheckResult>,
    pub next: &'static str,
}

fn outcome(code: &str, status: u16) -> &'static str {
    match code {
        "" => "ok",
        "auth_required" | "identity_unavailable" => "signed_out",
        "response_mismatch" | "query_failed" | "not_found" | "data_invalid" => "site_changed",
        "request_failed" if [404, 405, 410].contains(&status) => "site_changed",
        "site_rate_limited" | "rate_limited" | "blocked_by_site" => "blocked",
        _ => "unreachable",
    }
}

fn next_step(verdict: &str) -> &'static str {
    match verdict {
        "ok" => "Recorded reads still return their expected results.",
        "signed_out" => {
            "The website session expired. Reconnect the account in the Hycli dashboard; do not prepare the website again."
        }
        "site_changed" => {
            "The account is fine but responses no longer match. Run `hycli request SITE \"Repair the failing reads\"` or prepare the website again."
        }
        "blocked" => {
            "The website is limiting or challenging requests. Wait, or resolve it in your browser."
        }
        "unreachable" => "The website could not be reached. Check the connection and retry later.",
        "unknown" => {
            "Responses changed and the account identity could not be confirmed. Reconnect the account in the dashboard, then check again."
        }
        _ => "No recorded read is available. Prepare the website again to record checks.",
    }
}

impl Runtime {
    /// Replay a website's recorded reads. Never runs a change and never calls AI.
    pub async fn check_site(&self, site: &str) -> AppResult<SiteCheck> {
        let sp = self.spec(site)?;
        let meta = self
            .state
            .read()?
            .sites
            .get(site)
            .cloned()
            .unwrap_or_default();
        let mut plan = meta.smoke.clone();
        if plan.is_empty() {
            // Definitions without recorded checks: the declared health read, otherwise
            // verified input-free reads.
            plan = sp
                .operations
                .iter()
                .filter(|op| {
                    let chosen = match &sp.health {
                        Some(health) => health.op == op.name,
                        None => meta.verified.get(&op.name) == Some(&true),
                    };
                    chosen && runtime::params(op).iter().all(|(_, p)| !p.required)
                })
                .map(|op| ReadCheck {
                    action: op.name.clone(),
                    inputs: BTreeMap::new(),
                })
                .collect();
        }
        let account = if meta.account_id.is_empty() {
            None
        } else {
            Some(match self.refresh_account(&meta.account_id).await {
                Ok(view) if view.status == "expired" => "signed_out",
                Ok(_) => "verified",
                Err(error) if outcome(&error.public_code, error.http_status) == "signed_out" => {
                    let expired = self
                        .state
                        .read()?
                        .accounts
                        .get(&meta.account_id)
                        .is_some_and(|a| a.status == "expired");
                    if expired { "signed_out" } else { "unverified" }
                }
                Err(_) => "unverified",
            })
        };
        let mut results = BTreeMap::<String, Value>::new();
        let mut checks = vec![];
        for check in plan {
            let mut record = |outcome, error: String| {
                checks.push(CheckResult {
                    action: check.action.clone(),
                    outcome,
                    error,
                })
            };
            let Ok(op) = sp.op(&check.action) else {
                record("skipped", "not_found".into());
                continue;
            };
            let Some(inputs) = resolve(&check, &results) else {
                record("skipped", "missing_earlier_result".into());
                continue;
            };
            if policy::operation_effect(op, &inputs) != Effect::Read {
                record("skipped", "not_a_read".into());
                continue;
            }
            let run = match self.request(&sp, &check.action, inputs, None) {
                Ok(request) => self.execute(request, None).await,
                Err(error) => Err(error),
            };
            match run.and_then(|(result, _)| result.ensure_success().map(|_| result)) {
                Ok(result) => {
                    if let Some(body) = result.mapped.or(result.body) {
                        results.insert(check.action.clone(), body);
                    }
                    record("ok", String::new());
                }
                Err(error) => record(
                    outcome(&error.public_code, error.http_status),
                    error.public_code,
                ),
            }
        }
        let has = |value: &str| checks.iter().any(|check| check.outcome == value);
        let verdict = if account == Some("signed_out") || has("signed_out") {
            "signed_out"
        } else if has("site_changed") {
            if account == Some("unverified") {
                "unknown"
            } else {
                "site_changed"
            }
        } else if has("blocked") {
            "blocked"
        } else if has("unreachable") {
            "unreachable"
        } else if has("ok") {
            "ok"
        } else {
            "no_checks"
        };
        Ok(SiteCheck {
            site: site.into(),
            verdict,
            account,
            checks,
            next: next_step(verdict),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn failures_separate_sign_in_from_changed_responses() {
        assert_eq!(outcome("auth_required", 401), "signed_out");
        assert_eq!(outcome("response_mismatch", 200), "site_changed");
        assert_eq!(outcome("request_failed", 404), "site_changed");
        assert_eq!(outcome("request_failed", 503), "unreachable");
        assert_eq!(outcome("site_rate_limited", 429), "blocked");
    }
}
