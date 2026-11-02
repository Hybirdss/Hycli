//! Bounded, evidence-led reads without installing an action or granting write authority.
//! Imported captures describe requests; only the current private account proof authorizes credentials.
mod artifact;
mod deployment;
mod evidence;
mod request;
mod strict_json;
#[cfg(test)]
mod tests;

use super::Runtime;
use crate::{
    accounts::{AccountView, Credential},
    apperr::{AppError, AppResult},
    net, policy, util,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{collections::BTreeMap, path::Path, time::Duration};

const MAX_REQUEST: usize = 128 * 1024;
const MAX_RESPONSE: usize = 2 * 1024 * 1024;
const MAX_ARTIFACT: usize = 1024 * 1024;
fn invalid() -> AppError {
    AppError::api("data_invalid", 400)
}
fn unsupported() -> AppError {
    AppError::api("unsupported_evidence", 422)
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct AccountRead {
    version: u8,
    request: request::ReadRequest,
    evidence: evidence::Evidence,
    #[serde(default)]
    persisted: Option<Persisted>,
    #[serde(default)]
    pagination: Option<ReadPagination>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Persisted {
    format: String,
    evidence: evidence::ArtifactEvidence,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ReadPagination {
    #[serde(default)]
    query: String,
    #[serde(default)]
    variable: String,
    next: String,
}
struct Snapshot {
    account: AccountView,
    credential: Credential,
    binding: String,
}
impl Runtime {
    fn account_read_snapshot(&self, id: &str) -> AppResult<Snapshot> {
        let _guard = self.selection_lock()?;
        self.account_read_snapshot_locked(id)
    }
    fn account_read_snapshot_locked(&self, id: &str) -> AppResult<Snapshot> {
        let credential = self.credential(id)?;
        let data = self.state.read()?;
        let account = data
            .accounts
            .get(id)
            .ok_or_else(|| AppError::api("auth_required", 401))?;
        let generation = data
            .identity_generations
            .get(id)
            .map(String::as_str)
            .unwrap_or("");
        if !credential.proves_identity(account, generation)?
            || super::origin(&account.site_url)? != super::origin(&credential.origin)?
        {
            return Err(AppError::api("auth_required", 401));
        }
        Ok(Snapshot {
            account: account.clone(),
            binding: credential.identity_proof_for(account, generation)?,
            credential,
        })
    }
    fn ensure_account_read(&self, id: &str, binding: &str) -> AppResult<()> {
        if self.account_read_snapshot(id)?.binding != binding {
            return Err(AppError::api("auth_required", 401));
        }
        Ok(())
    }
    /// One bounded page. No account discovery, identity refresh, preflight or browser fallback.
    pub(crate) async fn read_account_request(
        &self,
        id: &str,
        path: &Path,
        cursor: Option<&str>,
    ) -> AppResult<Value> {
        let bytes = util::read_bounded(path, MAX_REQUEST).map_err(|_| invalid())?;
        let descriptor: AccountRead = serde_json::from_value(strict_json::parse(&bytes)?)?;
        if descriptor.version != 1 {
            return Err(invalid());
        }
        let snapshot = self.account_read_snapshot(id)?;
        let target = net::validate_url(&descriptor.request.url)?;
        if target.origin() != net::validate_url(&snapshot.credential.origin)?.origin() {
            return Err(AppError::api("unsafe_target", 400));
        }
        let mut secrets = snapshot.credential.session_redaction_values();
        {
            let mut vault = self.vault.lock().map_err(|_| invalid())?;
            vault.reload()?;
            secrets.extend(collect_read_secrets(&vault));
        }
        let input_secrets = read_input_secrets(&secrets);
        screen_metadata_secrets(&serde_json::to_value(&descriptor)?, &input_secrets)?;
        screen_request_secrets(&descriptor.request, &input_secrets)?;
        let base = path.parent().unwrap_or(Path::new("."));
        let mut prepared = descriptor.request.prepare()?;
        prepared.screen_normalized_secrets(&input_secrets)?;
        let observed = self.read_evidence(&descriptor.evidence, base, id, &snapshot)?;
        observed.matches_request(&descriptor.request, &prepared)?;
        let mut evidence_digest = observed.digest.clone();
        let mut documentless_name = None;
        if let Some(persisted) = &descriptor.persisted {
            let (operation_id, operation_name) = prepared.persisted_selector()?;
            // Hash authority comes from the actual APQ protocol, never a descriptor flag.
            // JSON-shaped public data cannot authorize an arbitrary registered operation ID.
            let apq = prepared.uses_apq_protocol();
            let require_executable = match persisted.format.as_str() {
                "apollo" | "relay" if apq => false,
                "relay-artifact" if !apq => true,
                _ => return Err(unsupported()),
            };
            let artifact = self
                .read_artifact_evidence(
                    &persisted.evidence,
                    base,
                    id,
                    &snapshot,
                    require_executable,
                )
                .await?;
            let verified = artifact::parse(
                &persisted.format,
                &artifact.bytes,
                &operation_id,
                operation_name.as_deref(),
                descriptor.request.apollo_client_name.as_deref(),
                apq,
            )?;
            evidence_digest.push_str(&artifact.digest);
            if let Some(document) = verified.document {
                prepared.semantic_document = Some(document);
            } else {
                documentless_name = Some(verified.name);
            }
        } else if prepared.persisted_id.is_some() {
            return Err(unsupported());
        }
        prepared.validate_read(documentless_name.as_deref())?;
        if let Some(pagination) = &descriptor.pagination {
            pagination.validate(&prepared)?;
        }
        if cursor.is_some() && descriptor.pagination.is_none() {
            return Err(invalid());
        }
        let binding = util::hash(&serde_json::to_vec(&(
            "account-read-v1",
            id,
            &snapshot.binding,
            &descriptor,
            &evidence_digest,
        ))?);
        let private_cursor = cursor
            .map(|handle| self.resolve_cursor_binding(&binding, handle, &input_secrets))
            .transpose()?;
        if let (Some(pagination), Some(value)) = (&descriptor.pagination, &private_cursor) {
            prepared.apply_cursor(pagination, value)?;
            prepared.validate_read(documentless_name.as_deref())?;
        }
        let headers = prepared.headers(
            &snapshot.credential,
            descriptor.request.apollo_client_name.as_deref(),
        )?;
        let check = || self.ensure_account_read(id, &snapshot.binding);
        let response = self
            .network
            .fetch_guarded(
                net::Request {
                    url: prepared.url.to_string(),
                    method: prepared.method.clone(),
                    headers,
                    body: prepared.wire_body()?,
                    account_id: id.into(),
                    limit: MAX_RESPONSE,
                    timeout: Duration::from_secs(25),
                    read_only: true,
                    follow_redirects: false,
                    tls_profile: "standard".into(),
                    multipart: None,
                },
                prepared
                    .query_cursor
                    .as_ref()
                    .map(|(k, v)| (k.as_str(), v.as_str())),
                &check,
            )
            .await?;
        check()?;
        if response.status == 401 {
            self.invalidate_identity_if_current(id, &snapshot.credential, true)?;
        }
        if (300..400).contains(&response.status) {
            return Err(AppError::api("read_redirect", 502));
        }
        let failure_body = (!response.successful())
            .then(|| strict_json::parse(&response.bytes).ok())
            .flatten();
        super::failure::ensure_success(
            response.status,
            &response.defense,
            "",
            Some(crate::apperr::SafeHttpDiagnostic::from_response(
                response.status,
                &response.defense,
                failure_body.as_ref(),
                &secrets,
            )),
        )?;
        let content_type = response
            .header("content-type")
            .split(';')
            .next()
            .unwrap_or("")
            .trim();
        if content_type != "application/json" && !content_type.ends_with("+json") {
            return Err(AppError::api("response_mismatch", 502));
        }
        let mut raw = strict_json::parse(&response.bytes)
            .map_err(|_| AppError::api("response_mismatch", 502))?;
        if prepared.graphql
            && raw
                .get("errors")
                .is_some_and(|e| !e.is_null() && e.as_array().is_none_or(|a| !a.is_empty()))
        {
            return Err(AppError::api("query_failed", 502));
        }
        if prepared.graphql
            && !raw
                .get("data")
                .is_some_and(|v| v.is_object() || v.is_null())
        {
            return Err(AppError::api("response_mismatch", 502));
        }
        // Historical cursor values are private even when echoed by another page. Keep this
        // output-only: treating them as input credentials would break valid continuation.
        let mut cursors = self.pagination_values()?;
        cursors.extend(private_cursor);
        let next_value = if let Some(pagination) = &descriptor.pagination {
            match raw.pointer_mut(&pagination.next) {
                None | Some(Value::Null) => None,
                Some(value) => {
                    let value = std::mem::replace(value, Value::Null);
                    let value = value
                        .as_str()
                        .ok_or_else(|| AppError::api("pagination_invalid", 400))?;
                    if value.is_empty() {
                        None
                    } else {
                        if !super::pagination::safe_value(value, &input_secrets) {
                            return Err(AppError::api("pagination_invalid", 400));
                        }
                        let mut next_request = prepared.clone_without_cursor();
                        next_request.apply_cursor(pagination, value)?;
                        next_request.validate_read(documentless_name.as_deref())?;
                        cursors.push(value.into());
                        Some(value.to_string())
                    }
                }
            }
        } else {
            None
        };
        super::pagination::scrub(&mut raw, &cursors)?;
        secrets.extend(cursors);
        redact_keys(&mut raw, &secrets)?;
        util::redact(&mut raw, &secrets);
        // Hold the account-selection lock through final authority check and cursor publication.
        let _guard = self.selection_lock()?;
        if self.account_read_snapshot_locked(id)?.binding != snapshot.binding {
            return Err(AppError::api("auth_required", 401));
        }
        let next_cursor = next_value
            .map(|value| self.store_cursor_binding(&snapshot.account.site_id, id, &binding, &value))
            .transpose()?;
        Ok(json!({"account_id":id,"status":response.status,"body":raw,"next_cursor":next_cursor}))
    }
}
fn redact_keys(value: &mut Value, secrets: &[String]) -> AppResult<()> {
    match value {
        Value::Object(fields) => {
            // Do not silently merge keys after redaction.
            let mut clean = serde_json::Map::new();
            for (key, mut child) in std::mem::take(fields) {
                let mut key = Value::String(key);
                util::redact(&mut key, secrets);
                redact_keys(&mut child, secrets)?;
                if clean
                    .insert(key.as_str().unwrap_or("[private]").into(), child)
                    .is_some()
                {
                    return Err(AppError::api("response_mismatch", 502));
                }
            }
            *fields = clean;
        }
        Value::Array(values) => {
            for value in values {
                redact_keys(value, secrets)?;
            }
        }
        Value::Number(number) => {
            if numeric_secret(number, secrets) {
                *value = Value::String("[private]".into());
            }
        }
        _ => {}
    }
    Ok(())
}
// A JSON number still carries its serialized bytes (and GraphQL IDs may be numeric).
fn numeric_secret(number: &serde_json::Number, secrets: &[String]) -> bool {
    let mut forms = vec![number.to_string()];
    if let Some(number) = number.as_f64().filter(|n| n.fract() == 0.0) {
        forms.push(format!("{number:.0}"));
    }
    // Short auth-state flags such as "1" are common public data, not meaningful
    // numeric secret candidates. Match the existing long-scalar redaction threshold.
    secrets
        .iter()
        .any(|secret| secret.chars().count() >= 8 && forms.iter().any(|text| text.contains(secret)))
}
impl ReadPagination {
    fn validate(&self, request: &request::Prepared) -> AppResult<()> {
        let name = if self.query.is_empty() {
            &self.variable
        } else {
            &self.query
        };
        if self.query.is_empty() == self.variable.is_empty()
            || name.is_empty()
            || name.len() > 64
            || !name
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
            || policy::dangerous_intent(name)
            || (util::secret_field(name)
                && !["pagetoken", "nextpagetoken", "continuationtoken"]
                    .contains(&name.to_ascii_lowercase().replace(['-', '_'], "").as_str()))
            || !self.next.starts_with('/')
            || self.next.len() > 256
            || self.next.split('/').skip(1).any(|part| {
                part.is_empty()
                    || (util::secret_field(part)
                        && !["pagetoken", "nextpagetoken", "continuationtoken"]
                            .contains(&part.to_ascii_lowercase().replace(['-', '_'], "").as_str()))
            })
            || (!self.variable.is_empty() && !request.graphql)
            || (!self.query.is_empty() && (request.method != "GET" || request.graphql))
        {
            return Err(invalid());
        }
        for part in self.next.split('/').skip(1) {
            let mut chars = part.chars();
            while let Some(c) = chars.next() {
                if c == '~' && !matches!(chars.next(), Some('0' | '1')) {
                    return Err(invalid());
                }
            }
        }
        request.ensure_cursor_absent(self)
    }
}

fn read_input_secrets(secrets: &[String]) -> Vec<String> {
    // An auth-state flag such as "1" is not a meaningful secret match for public
    // page/limit constants, including their GraphQL/URL text representation.
    // Keep all values for output redaction and credential snapshot binding.
    secrets
        .iter()
        .filter(|value| {
            value.chars().count() >= 8
                || !serde_json::from_str::<Value>(value).is_ok_and(|value| value.is_number())
        })
        .cloned()
        .collect()
}

fn screen_metadata_secrets(value: &Value, secrets: &[String]) -> AppResult<()> {
    // Version/entry indexes are local metadata, not wire data; short auth cookies may
    // legitimately equal them. Still screen all descriptor strings and object keys.
    screen_value_secrets_inner(value, secrets, false)
}
fn screen_value_secrets(value: &Value, secrets: &[String]) -> AppResult<()> {
    screen_value_secrets_inner(value, secrets, true)
}
fn screen_value_secrets_inner(value: &Value, secrets: &[String], numbers: bool) -> AppResult<()> {
    fn unchanged(value: &str, secrets: &[String]) -> bool {
        let mut clean = Value::String(value.into());
        util::redact(&mut clean, secrets);
        clean == Value::String(value.into())
    }
    fn walk(value: &Value, secrets: &[String], numbers: bool) -> bool {
        match value {
            Value::Object(fields) => fields
                .iter()
                .all(|(key, value)| unchanged(key, secrets) && walk(value, secrets, numbers)),
            Value::Array(values) => values.iter().all(|v| walk(v, secrets, numbers)),
            Value::String(text) => unchanged(text, secrets),
            Value::Number(number) => !numbers || !numeric_secret(number, secrets),
            _ => true,
        }
    }
    let mut clean = value.clone();
    util::redact(&mut clean, secrets);
    if clean != *value || !walk(value, secrets, numbers) {
        return Err(invalid());
    }
    Ok(())
}

fn screen_request_secrets(request: &request::ReadRequest, secrets: &[String]) -> AppResult<()> {
    screen_value_secrets(&serde_json::to_value(request)?, secrets)?;
    fn unchanged(value: &str, secrets: &[String]) -> bool {
        let mut clean = Value::String(value.into());
        util::redact(&mut clean, secrets);
        clean == Value::String(value.into())
    }
    let url = url::Url::parse(&request.url).map_err(|_| invalid())?;
    for part in std::iter::once(url.path().to_string()).chain(
        url.query_pairs()
            .flat_map(|(k, v)| [k.into_owned(), v.into_owned()]),
    ) {
        let mut decoded = part;
        for _ in 0..3 {
            if !unchanged(&decoded, secrets) {
                return Err(invalid());
            }
            let next = url::form_urlencoded::parse(
                format!("v={}", decoded.replace('+', "%2B")).as_bytes(),
            )
            .next()
            .map(|(_, v)| v.into_owned())
            .unwrap_or_default();
            if next == decoded {
                break;
            }
            decoded = next;
        }
    }
    Ok(())
}

fn collect_read_secrets(vault: &crate::store::Store) -> Vec<String> {
    let mut secrets = Vec::new();
    for (key, value) in vault.entries() {
        if key.starts_with("accounts/") {
            if let Ok(credential) = serde_json::from_str::<Credential>(value) {
                secrets.extend(credential.session_redaction_values());
            }
        } else if !key.starts_with('@') && !value.is_empty() {
            secrets.push(value.clone());
        }
    }
    secrets
}
