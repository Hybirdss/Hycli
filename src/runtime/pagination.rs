//! Opaque continuation capabilities. Raw values never enter public requests/results.
use super::Runtime;
use crate::{
    apperr::{AppError, AppResult},
    policy::{self, Effect, ExecutionRequest},
    spec::Operation,
    util,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
const PREFIX: &str = "@pagination/";
const TTL: i64 = 15 * 60 * 1000;
const MAX: usize = 256;
const MAX_VALUE: usize = 4096;
#[derive(Serialize, Deserialize)]
struct Record {
    site: String,
    account: String,
    binding: String,
    expires: i64,
    value: String,
}
fn invalid() -> AppError {
    AppError::api("data_invalid", 400)
}
fn expired() -> AppError {
    AppError::api("pagination_expired", 409)
}
pub(super) fn safe_value(value: &str, secrets: &[String]) -> bool {
    if value.chars().count() < 8
        || value.len() > MAX_VALUE
        || value.chars().any(char::is_control)
        || value.contains("://")
        || value.contains("-----BEGIN")
        || secrets
            .iter()
            .any(|secret| !secret.is_empty() && value.contains(secret))
    {
        return false;
    }
    let mut clean = Value::String(value.into());
    util::redact(&mut clean, secrets);
    clean == Value::String(value.into())
}
/// Scrub current-request continuation echoes before model projection.
/// Cursor-bearing object keys fail closed to preserve payload/model structure.
pub(super) fn scrub(value: &mut Value, cursors: &[String]) -> AppResult<()> {
    match value {
        Value::String(value) => {
            for cursor in cursors.iter().filter(|s| !s.is_empty()) {
                *value = value.replace(cursor, "[private]");
            }
        }
        Value::Array(values) => {
            for value in values {
                scrub(value, cursors)?;
            }
        }
        Value::Object(values) => {
            // Rewriting keys could merge fields or change model lookup structure.
            // Reject the response rather than return silently corrupted data.
            if values
                .keys()
                .any(|key| cursors.iter().any(|c| !c.is_empty() && key.contains(c)))
            {
                return Err(AppError::api("pagination_invalid", 400));
            }
            for value in values.values_mut() {
                scrub(value, cursors)?;
            }
        }
        _ => {}
    }
    Ok(())
}
/// Match the final URL assembled by the network layer without exposing its cursor.
pub(super) fn continuation_effect(
    op: &Operation,
    inputs: &BTreeMap<String, String>,
    url: &str,
    value: &str,
) -> Effect {
    let Some(pagination) = &op.pagination else {
        return Effect::Unknown;
    };
    let Ok(mut url) = url::Url::parse(url) else {
        return Effect::Unknown;
    };
    url.query_pairs_mut().append_pair(&pagination.query, value);
    let mut inputs = inputs.clone();
    inputs.remove(&pagination.input);
    policy::rendered_request_effect(op, &inputs, url.as_str(), None)
}

fn binding(request: &ExecutionRequest, op: &Operation, url: &str) -> AppResult<String> {
    let mut inputs = request.inputs.clone();
    if let Some(p) = &op.pagination {
        inputs.remove(&p.input);
    }
    Ok(util::hash(&serde_json::to_vec(&(
        request.site_id.as_str(),
        request.action_id.as_str(),
        request.spec_hash.as_str(),
        request.account_id.as_str(),
        request.credential_hash.as_str(),
        url,
        inputs,
    ))?))
}
fn prune(records: &mut BTreeMap<String, String>, now: i64) {
    records.retain(|_, v| {
        serde_json::from_str::<Record>(v)
            .is_ok_and(|r| r.expires > now && r.expires <= now + TTL && r.value.len() <= MAX_VALUE)
    });
}
impl Runtime {
    pub(crate) fn pagination_values(&self) -> AppResult<Vec<String>> {
        self.vault
            .lock()
            .map_err(|_| invalid())?
            .pagination_transaction(|records| {
                prune(records, util::unix_ms());
                Ok(records
                    .values()
                    .filter_map(|v| serde_json::from_str::<Record>(v).ok().map(|r| r.value))
                    .collect())
            })
    }
    pub(crate) fn clear_pagination(&self, site: &str) -> AppResult<()> {
        self.vault
            .lock()
            .map_err(|_| invalid())?
            .pagination_transaction(|r| {
                r.retain(|_, v| {
                    serde_json::from_str::<Record>(v).is_ok_and(|record| record.site != site)
                });
                Ok(())
            })
    }
    pub(crate) fn resolve_cursor(
        &self,
        request: &ExecutionRequest,
        op: &Operation,
        url: &str,
        secrets: &[String],
    ) -> AppResult<Option<String>> {
        let Some(p) = &op.pagination else {
            return Ok(None);
        };
        let Some(handle) = request.inputs.get(&p.input) else {
            return Ok(None);
        };
        self.resolve_cursor_binding(&binding(request, op, url)?, handle, secrets)
            .map(Some)
    }
    pub(super) fn resolve_cursor_binding(
        &self,
        expected: &str,
        handle: &str,
        secrets: &[String],
    ) -> AppResult<String> {
        if !handle.starts_with("hc_")
            || handle.len() != 35
            || !handle[3..].bytes().all(|c| c.is_ascii_hexdigit())
        {
            return Err(expired());
        }
        let result = self
            .vault
            .lock()
            .map_err(|_| invalid())?
            .pagination_transaction(|records| {
                prune(records, util::unix_ms());
                let key = format!("{PREFIX}{handle}");
                let record = records
                    .get(&key)
                    .and_then(|v| serde_json::from_str::<Record>(v).ok());
                match record {
                    Some(r) if r.binding == expected && safe_value(&r.value, secrets) => {
                        Ok(Some(r.value))
                    }
                    _ => {
                        records.remove(&key);
                        Ok(None)
                    }
                }
            })?;
        result.ok_or_else(expired)
    }
    pub(crate) fn capture_cursor(
        &self,
        request: &ExecutionRequest,
        op: &Operation,
        url: &str,
        raw: &mut Value,
        secrets: &mut Vec<String>,
        issue: bool,
    ) -> AppResult<Option<String>> {
        let Some(p) = &op.pagination else {
            return Ok(None);
        };
        let Some(value) = raw.pointer_mut(&p.next) else {
            return Ok(None);
        };
        let value = std::mem::replace(value, Value::Null);
        if value.is_null() || value.as_str() == Some("") {
            return Ok(None);
        }
        let value = value
            .as_str()
            .ok_or_else(|| AppError::api("pagination_invalid", 400))?;
        if !safe_value(value, secrets) {
            return Err(AppError::api("pagination_invalid", 400));
        }
        if continuation_effect(op, &request.inputs, url, value) != Effect::Read {
            return Err(invalid());
        }
        secrets.push(value.into());
        if !issue {
            return Ok(None);
        }
        self.store_cursor_binding(
            &request.site_id,
            &request.account_id,
            &binding(request, op, url)?,
            value,
        )
        .map(Some)
    }
    pub(super) fn store_cursor_binding(
        &self,
        site: &str,
        account: &str,
        binding: &str,
        value: &str,
    ) -> AppResult<String> {
        let handle = format!("hc_{}", util::id());
        let record = serde_json::to_string(&Record {
            site: site.into(),
            account: account.into(),
            binding: binding.into(),
            expires: util::unix_ms() + TTL,
            value: value.into(),
        })?;
        self.vault
            .lock()
            .map_err(|_| invalid())?
            .pagination_transaction(|records| {
                prune(records, util::unix_ms());
                while records.len() >= MAX {
                    let key = records
                        .iter()
                        .min_by_key(|(_, v)| {
                            serde_json::from_str::<Record>(v)
                                .map(|r| r.expires)
                                .unwrap_or(0)
                        })
                        .map(|(k, _)| k.clone())
                        .unwrap();
                    records.remove(&key);
                }
                records.insert(format!("{PREFIX}{handle}"), record);
                Ok(())
            })?;
        Ok(handle)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rendered_continuation_protocol_selector_requires_review() {
        let mut operation = op();
        operation.pagination.as_mut().unwrap().query = "extensions".into();
        let value = r#"{"persistedQuery":{"sha256Hash":"synthetic-reference"}}"#;
        assert_eq!(
            continuation_effect(
                &operation,
                &BTreeMap::new(),
                "https://fixture.example.test/items",
                value
            ),
            Effect::Unknown
        );
    }

    #[test]
    fn rendered_continuation_values_are_data_unless_the_field_selects_a_protocol() {
        let mut operation = op();
        let inputs = BTreeMap::from([("cursor".into(), "hc_synthetic-handle".into())]);
        for value in [
            "synthetic-page-token",
            "mutation { sampleChange }",
            r#"{"query":"mutation { sampleChange }"}"#,
            r#"{"operation":"update"}"#,
            r#"{"extensions":{"persistedQuery":{"sha256Hash":"synthetic-reference"}}}"#,
        ] {
            assert_eq!(
                continuation_effect(
                    &operation,
                    &inputs,
                    "https://fixture.example.test/items?limit=10",
                    value
                ),
                Effect::Read
            );
        }
        operation.pagination.as_mut().unwrap().query = "query".into();
        assert_eq!(
            continuation_effect(
                &operation,
                &inputs,
                "https://fixture.example.test/items",
                "query { items { id } }"
            ),
            Effect::Read
        );
        assert_eq!(
            continuation_effect(
                &operation,
                &inputs,
                "https://fixture.example.test/items",
                "mutation { sampleChange }"
            ),
            Effect::Write
        );
        operation.pagination.as_mut().unwrap().query = "action".into();
        assert_eq!(
            continuation_effect(
                &operation,
                &inputs,
                "https://fixture.example.test/items",
                "update"
            ),
            Effect::Write
        );
    }

    fn core() -> std::sync::Arc<Runtime> {
        Runtime::open(
            &std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join(".cache/pagination-unit")
                .join(util::id()),
            None,
            None,
            false,
            true,
            false,
        )
        .unwrap()
    }
    fn op() -> Operation {
        serde_json::from_value(serde_json::json!({"name":"list-items","method":"GET","path":"/items","effect":"read","evidence":"Synthetic pagination documentation","pagination":{"input":"cursor","query":"pageToken","next":"/nextPageToken"}})).unwrap()
    }
    fn request() -> ExecutionRequest {
        ExecutionRequest {
            site_id: "fixture".into(),
            action_id: "list-items".into(),
            account_id: "account-a".into(),
            inputs: BTreeMap::new(),
            spec_hash: "spec-a".into(),
            credential_hash: "credential-a".into(),
        }
    }
    #[test]
    fn rejected_stale_install_preserves_current_pagination_until_authorized_replacement() {
        let core = core();
        let origin = "https://example.com";
        let sp = crate::spec::parse(serde_json::json!({"spec_version":1,"site":{"name":"fixture","title":"Original","base_url":origin},"operations":[{"name":"read-items","method":"GET","path":"/items","effect":"read","evidence":"Synthetic fixture documentation"}]}).to_string().as_bytes()).unwrap();
        core.install(&sp).unwrap();
        core.select_account("fixture", "", origin).unwrap();
        let stale = core.selection_snapshot("fixture", origin).unwrap().unwrap();
        core.select_account("fixture", "", origin).unwrap();
        let current = core.selection_snapshot("fixture", origin).unwrap().unwrap();
        let mut raw = serde_json::json!({"nextPageToken":"opaque-current-page"});
        core.capture_cursor(
            &request(),
            &op(),
            "https://example.com/items",
            &mut raw,
            &mut vec![],
            true,
        )
        .unwrap();
        let mut replacement = sp.clone();
        replacement.site.title = "Replacement".into();
        assert!(core.install_selected(&replacement, Some(&stale)).is_err());
        assert_eq!(
            core.pagination_values().unwrap(),
            vec!["opaque-current-page"]
        );
        assert_eq!(
            Runtime::spec_hash(&core.spec("fixture").unwrap()).unwrap(),
            Runtime::spec_hash(&sp).unwrap()
        );
        core.install_selected(&replacement, Some(&current)).unwrap();
        assert!(core.pagination_values().unwrap().is_empty());
    }

    #[test]
    fn private_identity_publication_and_mutation_invalidate_only_its_account_cursors() {
        let core = core();
        let origin = "https://example.com";
        let cookies = crate::accounts::parse_cookies(&serde_json::json!([{"name":"sid","value":"synthetic-private-session","domain":"example.com","path":"/","secure":true}]).to_string(), origin).unwrap();
        let account = core
            .connect_account("", origin, cookies, "Fixture", "Synthetic", "")
            .unwrap();
        let mut own = request();
        own.account_id = account.id.clone();
        for (request, cursor) in [
            (&own, "opaque-own-page"),
            (&request(), "opaque-unrelated-page"),
        ] {
            let mut raw = serde_json::json!({"nextPageToken":cursor});
            core.capture_cursor(
                request,
                &op(),
                "https://example.com/items",
                &mut raw,
                &mut vec![],
                true,
            )
            .unwrap();
        }
        let expected = core.credential(&account.id).unwrap();
        let view = core
            .publish_verified_identity(
                &account.id,
                &expected,
                expected.clone(),
                crate::accounts::Identity {
                    user_id: "synthetic-person".into(),
                    ..Default::default()
                },
            )
            .unwrap();
        let data = core.state.read().unwrap();
        assert!(
            core.credential(&account.id)
                .unwrap()
                .proves_identity(&view, &data.identity_generations[&account.id])
                .unwrap()
        );
        assert_eq!(
            core.pagination_values().unwrap(),
            vec!["opaque-unrelated-page"]
        );
        let mut raw = serde_json::json!({"nextPageToken":"opaque-renewed-page"});
        core.capture_cursor(
            &own,
            &op(),
            "https://example.com/items",
            &mut raw,
            &mut vec![],
            true,
        )
        .unwrap();
        core.update_credential(&account.id, |credential| {
            credential.source_profile = "replacement-profile".into();
            Ok(())
        })
        .unwrap();
        assert!(
            core.credential(&account.id)
                .unwrap()
                .identity_proof
                .is_empty()
        );
        assert_eq!(
            core.pagination_values().unwrap(),
            vec!["opaque-unrelated-page"]
        );
    }

    #[test]
    fn ambiguous_short_values_are_rejected_before_literal_echo_scrubbing() {
        assert!(!safe_value("2", &[]));
        assert!(!safe_value("1234567", &[]));
        assert!(!safe_value("prefix-short-suffix", &["short".into()]));
        let mut value =
            serde_json::json!({"echo":"prefixopaque-nextsuffix","year":"2026","v2":"ok"});
        scrub(&mut value, &["opaque-next".into()]).unwrap();
        assert_eq!(value["echo"], "prefix[private]suffix");
        assert_eq!(value["year"], "2026");
        assert_eq!(value["v2"], "ok");
        let mut collision = serde_json::json!({"opaque-next":"one","[private]":"two"});
        assert!(scrub(&mut collision, &["opaque-next".into()]).is_err());
        let mut model_key = serde_json::json!({"xopaque-nextx":"one"});
        assert!(scrub(&mut model_key, &["opaque-next".into()]).is_err());
    }
    #[test]
    fn rejects_secret_shaped_or_known_values_without_storing_them() {
        let core = core();
        for value in [
            "Bearer synthetic-secret",
            "sk-012345678901234567890",
            "eyJabcdefghijk.abcdefghijk.abcdefghijk",
            "-----BEGIN PRIVATE KEY-----",
            "known-credential-value",
            "https://example.com/next",
        ] {
            let mut raw = serde_json::json!({"nextPageToken":value});
            assert!(
                core.capture_cursor(
                    &request(),
                    &op(),
                    "https://example.com/items",
                    &mut raw,
                    &mut vec!["known-credential-value".into()],
                    true
                )
                .is_err()
            );
            assert!(core.pagination_values().unwrap().is_empty());
        }
    }
    #[test]
    fn expiry_identity_spec_and_request_binding_are_enforced() {
        let core = core();
        let op = op();
        let url = "https://example.com/items";
        for changed in ["account", "credential", "spec", "args", "url", "expired"] {
            let original = request();
            let mut raw = serde_json::json!({"nextPageToken":"opaque-next-page"});
            let h = core
                .capture_cursor(&original, &op, url, &mut raw, &mut vec![], true)
                .unwrap()
                .unwrap();
            let mut req = original.clone();
            req.inputs.insert("cursor".into(), h.clone());
            let mut target = url;
            match changed {
                "account" => req.account_id = "account-b".into(),
                "credential" => req.credential_hash = "credential-b".into(),
                "spec" => req.spec_hash = "spec-b".into(),
                "args" => {
                    req.inputs.insert("q".into(), "different".into());
                }
                "url" => target = "https://example.com/other",
                _ => core
                    .vault
                    .lock()
                    .unwrap()
                    .pagination_transaction(|r| {
                        let k = format!("{PREFIX}{h}");
                        let mut item: Record = serde_json::from_str(&r[&k])?;
                        item.expires = util::unix_ms() - 1;
                        r.insert(k, serde_json::to_string(&item)?);
                        Ok(())
                    })
                    .unwrap(),
            };
            assert!(
                core.resolve_cursor(&req, &op, target, &[]).is_err(),
                "{changed}"
            );
            assert!(core.pagination_values().unwrap().is_empty());
        }
    }
    #[test]
    fn bounded_transactions_preserve_credentials_and_expiry_does_not_slide() {
        let core = core();
        core.vault
            .lock()
            .unwrap()
            .set("fixture", "api-key", "test-credential")
            .unwrap();
        let op = op();
        let req = request();
        let url = "https://example.com/items";
        let mut raw = serde_json::json!({"nextPageToken":"opaque-next-page"});
        let h = core
            .capture_cursor(&req, &op, url, &mut raw, &mut vec![], true)
            .unwrap()
            .unwrap();
        let expiry = core
            .vault
            .lock()
            .unwrap()
            .pagination_transaction(|r| {
                Ok(serde_json::from_str::<Record>(&r[&format!("{PREFIX}{h}")])?.expires)
            })
            .unwrap();
        let mut req = req;
        req.inputs.insert("cursor".into(), h.clone());
        assert!(core.resolve_cursor(&req, &op, url, &[]).unwrap().is_some());
        assert_eq!(
            core.vault
                .lock()
                .unwrap()
                .pagination_transaction(|r| Ok(serde_json::from_str::<Record>(
                    &r[&format!("{PREFIX}{h}")]
                )?
                .expires))
                .unwrap(),
            expiry
        );
        for i in 0..MAX + 1 {
            let mut raw = serde_json::json!({"nextPageToken":format!("opaque-{i}")});
            core.capture_cursor(&request(), &op, url, &mut raw, &mut vec![], true)
                .unwrap();
        }
        assert_eq!(core.pagination_values().unwrap().len(), MAX);
        assert!(core.vault.lock().unwrap().contains("fixture", "api-key"));
    }
}
