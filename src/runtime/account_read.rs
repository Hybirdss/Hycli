//! Evidence-bound, read-only account requests.
//!
//! Request descriptors describe a request, but never grant credential authority. The
//! current verified account and a successful browser observation remain mandatory.
use super::Runtime;
use crate::{
    accounts::{self, AccountView, Credential, Observation},
    apperr::{AppError, AppResult},
    net::{self, Request},
    policy, util,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
    time::Duration,
};

const MAX_REQUEST: usize = 128 * 1024;
const MAX_RESPONSE: usize = 2 * 1024 * 1024;
const CURSOR_TTL_MS: i64 = 24 * 60 * 60 * 1000;

fn invalid() -> AppError {
    AppError::api("data_invalid", 400)
}
fn unsupported() -> AppError {
    AppError::api("unsupported_evidence", 422)
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct AccountRead {
    pub(crate) version: u8,
    pub(crate) request: ReadRequest,
    pub(crate) evidence: Evidence,
    #[serde(default)]
    pub(crate) pagination: Option<ReadPagination>,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ReadRequest {
    pub(crate) method: String,
    pub(crate) url: String,
    #[serde(default)]
    pub(crate) json: Option<Value>,
    #[serde(default)]
    pub(crate) form: Option<BTreeMap<String, String>>,
    #[serde(default)]
    pub(crate) apollo_client_name: Option<String>,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Evidence {
    pub(crate) observation: Option<String>,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ReadPagination {
    #[serde(default)]
    pub(crate) query: String,
    #[serde(default)]
    pub(crate) variable: String,
    pub(crate) next: String,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct CursorRecord {
    account: String,
    binding: String,
    value: String,
    expires: i64,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ObservationRecord {
    account: String,
    binding: String,
    observation: Observation,
    expires: i64,
}

#[derive(Clone, Serialize)]
pub(crate) struct ReadObservationReference {
    pub(crate) reference: String,
    pub(crate) method: String,
    pub(crate) path: String,
    pub(crate) query: BTreeMap<String, String>,
    pub(crate) body: BTreeMap<String, String>,
    pub(crate) status: u16,
}

#[derive(Clone)]
struct Snapshot {
    account: AccountView,
    credential: Credential,
    binding: String,
}

#[derive(Clone)]
struct Prepared {
    method: String,
    url: url::Url,
    body: Option<Value>,
    form: Option<BTreeMap<String, String>>,
    graphql: bool,
    apollo_client_name: Option<String>,
}

impl ReadRequest {
    fn prepare(&self) -> AppResult<Prepared> {
        if !matches!(self.method.as_str(), "GET" | "POST")
            || (self.json.is_some() && self.form.is_some())
        {
            return Err(unsupported());
        }
        if self.apollo_client_name.as_ref().is_some_and(|name| {
            name.is_empty() || name.len() > 128 || name.chars().any(char::is_control)
        }) {
            return Err(invalid());
        }
        let parsed = url::Url::parse(&self.url).map_err(|_| invalid())?;
        if parsed.fragment().is_some() {
            return Err(invalid());
        }
        let url = net::validate_url(&self.url)?;
        let query_keys = unique_query_keys(&url)?;
        if self.method == "GET" && (self.json.is_some() || self.form.is_some()) {
            return Err(invalid());
        }
        if self.method == "POST" && !query_keys.is_empty() {
            return Err(unsupported());
        }
        let graphql = url.path().to_ascii_lowercase().contains("graphql")
            || url.path().to_ascii_lowercase().ends_with("/gql");
        if self.method == "POST" && self.json.is_none() && self.form.is_none() {
            return Err(unsupported());
        }
        if graphql && self.form.is_some() {
            return Err(unsupported());
        }
        if let Some(body) = &self.json {
            if !body.is_object() {
                return Err(invalid());
            }
        }
        Ok(Prepared {
            method: self.method.clone(),
            url,
            body: self.json.clone(),
            form: self.form.clone(),
            graphql,
            apollo_client_name: self.apollo_client_name.clone(),
        })
    }
}

impl Prepared {
    fn validate_read(&self) -> AppResult<()> {
        if !policy::safe_read_url(&self.url) || policy::dangerous_intent(self.url.path()) {
            return Err(unsupported());
        }
        if let Some(body) = &self.body {
            if contains_change_intent(body) {
                return Err(unsupported());
            }
            if self.graphql {
                let raw = serde_json::to_string(body)?;
                if !crate::browser_session::is_safe_graphql_read_post(&self.url, Some(&raw)) {
                    return Err(unsupported());
                }
                if let Some(query) = body.get("query").and_then(Value::as_str) {
                    validate_graphql_query(query)?;
                }
            } else if !looks_like_read_post(self.url.path()) {
                return Err(unsupported());
            }
        }
        if let Some(form) = &self.form {
            if form.keys().any(|key| util::secret_field(key))
                || form.values().any(|value| policy::dangerous_intent(value))
            {
                return Err(unsupported());
            }
            if !looks_like_read_post(self.url.path()) {
                return Err(unsupported());
            }
        }
        Ok(())
    }

    fn wire_body(&self) -> AppResult<Option<Vec<u8>>> {
        if self.method == "GET" {
            return Ok(None);
        }
        if let Some(body) = &self.body {
            return Ok(Some(serde_json::to_vec(body)?));
        }
        let Some(form) = &self.form else {
            return Err(invalid());
        };
        let mut serializer = url::form_urlencoded::Serializer::new(String::new());
        for (key, value) in form {
            serializer.append_pair(key, value);
        }
        Ok(Some(serializer.finish().into_bytes()))
    }

    fn headers(&self, credential: &Credential) -> AppResult<Vec<(String, String)>> {
        let mut headers = credential.request_headers(self.url.as_str())?;
        headers.retain(|(key, _)| {
            !matches!(
                key.to_ascii_lowercase().as_str(),
                "accept"
                    | "content-type"
                    | "apollographql-client-name"
                    | "x-http-method-override"
                    | "x-method-override"
                    | "x-http-method"
                    | "x-original-url"
                    | "x-rewrite-url"
            )
        });
        headers.push(("accept".into(), "application/json".into()));
        if let Some(client) = &self.apollo_client_name {
            headers.push(("apollographql-client-name".into(), client.clone()));
        }
        if self.method == "POST" {
            headers.push((
                "content-type".into(),
                if self.body.is_some() {
                    "application/json"
                } else {
                    "application/x-www-form-urlencoded"
                }
                .into(),
            ));
        }
        if !headers.iter().any(|(key, _)| {
            key.eq_ignore_ascii_case("cookie")
                || util::secret_field(key)
                || accounts::browser_header(key)
        }) {
            return Err(AppError::api("auth_required", 401));
        }
        Ok(headers)
    }
}

impl Runtime {
    pub(crate) async fn read_account_request(
        &self,
        id: &str,
        path: &Path,
        cursor: Option<&str>,
    ) -> AppResult<Value> {
        let bytes = util::read_bounded(path, MAX_REQUEST).map_err(|_| invalid())?;
        let descriptor: AccountRead = serde_json::from_slice(&bytes).map_err(|_| invalid())?;
        if descriptor.version != 1
            || descriptor
                .evidence
                .observation
                .as_deref()
                .is_none_or(str::is_empty)
        {
            return Err(invalid());
        }
        let snapshot = self.account_read_snapshot(id)?;
        let secrets = self.read_secrets(&snapshot.credential)?;
        screen_value(&serde_json::to_value(&descriptor)?, &secrets)?;
        let prepared = descriptor.request.prepare()?;
        if prepared.url.origin() != net::validate_url(&snapshot.credential.origin)?.origin() {
            return Err(AppError::api("unsafe_target", 400));
        }
        prepared.validate_read()?;
        let observation_ref = descriptor
            .evidence
            .observation
            .as_deref()
            .ok_or_else(invalid)?;
        if matches!(observation_ref, "state" | "observed") {
            if !self.matches_observation(&snapshot.account, &prepared)? {
                return Err(unsupported());
            }
        } else {
            let observation = self.resolve_observation(observation_ref, id, &snapshot.binding)?;
            if !observation_matches(&observation, &prepared) {
                return Err(unsupported());
            }
        }
        let binding = util::hash(&serde_json::to_vec(&(
            "account-read-v1",
            id,
            &snapshot.binding,
            &descriptor,
        ))?);
        let cursor_value = cursor
            .map(|handle| self.resolve_cursor(handle, id, &binding))
            .transpose()?;
        let mut prepared = prepared;
        if let Some(pagination) = &descriptor.pagination {
            pagination.validate(&prepared)?;
            if let Some(value) = &cursor_value {
                pagination.apply(&mut prepared, value)?;
            }
        } else if cursor.is_some() {
            return Err(invalid());
        }
        prepared.validate_read()?;
        let body = prepared.wire_body()?;
        let headers = prepared.headers(&snapshot.credential)?;
        let account_id = id.to_owned();
        let expected_binding = snapshot.binding.clone();
        let check = || self.ensure_account_binding(&account_id, &expected_binding);
        let response = self
            .network
            .fetch_guarded(
                Request {
                    url: prepared.url.to_string(),
                    method: prepared.method.clone(),
                    headers,
                    body,
                    account_id: id.into(),
                    limit: MAX_RESPONSE,
                    timeout: Duration::from_secs(25),
                    read_only: true,
                    follow_redirects: false,
                    tls_profile: "standard".into(),
                },
                &check,
            )
            .await?;
        check()?;
        if response.status == 401 {
            return Err(AppError::api("auth_required", 401));
        }
        if response.status == 429 {
            return Err(AppError::api("site_rate_limited", 429));
        }
        if !response.successful() {
            return Err(AppError::api(
                if response.status == 403 || response.defense != "none" {
                    "blocked_by_site"
                } else if (300..400).contains(&response.status) {
                    "read_redirect"
                } else {
                    "request_failed"
                },
                if response.status == 403 || response.defense != "none" {
                    403
                } else {
                    502
                },
            ));
        }
        if net::validate_url(&response.url)?.origin() != prepared.url.origin() {
            return Err(AppError::api("response_mismatch", 502));
        }
        let content_type = response
            .header("content-type")
            .split(';')
            .next()
            .unwrap_or("")
            .trim();
        if content_type != "application/json" && !content_type.ends_with("+json") {
            return Err(AppError::api("response_mismatch", 502));
        }
        let mut value: Value = serde_json::from_slice(&response.bytes)
            .map_err(|_| AppError::api("response_mismatch", 502))?;
        if prepared.graphql {
            if value
                .get("errors")
                .and_then(Value::as_array)
                .is_some_and(|errors| !errors.is_empty())
            {
                return Err(AppError::api("query_failed", 502));
            }
            if !value
                .get("data")
                .is_some_and(|data| data.is_object() || data.is_null())
            {
                return Err(AppError::api("response_mismatch", 502));
            }
        }
        let mut raw_cursors = vec![];
        let next_value = descriptor
            .pagination
            .as_ref()
            .and_then(|pagination| pagination_value(&value, &pagination.next))
            .map(|next| match next {
                Value::Null => Ok(None),
                Value::String(value) if value.is_empty() => Ok(None),
                Value::String(value) => Ok(Some(value.clone())),
                _ => Err(AppError::api("pagination_invalid", 400)),
            })
            .transpose()?
            .flatten();
        if let Some(value) = &cursor_value {
            raw_cursors.push(value.clone());
        }
        if let Some(value) = &next_value {
            raw_cursors.push(value.clone());
        }
        if let Some(pagination) = &descriptor.pagination {
            if pagination.next == "$x_timeline_bottom" {
                scrub_x_timeline_cursors(&mut value);
            } else if let Some(pointer) = value.pointer_mut(&pagination.next) {
                *pointer = Value::Null;
            }
        }
        util::redact(&mut value, &secrets);
        for cursor in &raw_cursors {
            replace_string(&mut value, cursor, "[private]");
        }
        let next_cursor = next_value
            .map(|value| self.store_cursor(id, &binding, value))
            .transpose()?;
        check()?;
        Ok(json!({
            "account_id": id,
            "status": response.status,
            "body": value,
            "next_cursor": next_cursor,
        }))
    }

    pub(crate) fn mint_read_observations(
        &self,
        id: &str,
    ) -> AppResult<Vec<ReadObservationReference>> {
        let account = self
            .state
            .read()?
            .accounts
            .get(id)
            .cloned()
            .ok_or_else(|| AppError::api("not_found", 404))?;
        if !account.verified_identity || account.status != "connected" {
            return Ok(vec![]);
        }
        let credential = self.credential(id)?;
        let binding = credential_binding(&account, &credential)?;
        let observations = self
            .state
            .read()?
            .sites
            .get(&account.site_id)
            .map(|site| site.observations.clone())
            .unwrap_or_default();
        let mut vault = self.vault.lock().map_err(|_| invalid())?;
        vault.reload()?;
        let now = util::unix_ms();
        let old_keys: Vec<String> = vault
            .entries()
            .iter()
            .filter_map(|(key, raw)| {
                if !key.starts_with("@read-observation/") {
                    return None;
                }
                let expired = serde_json::from_str::<ObservationRecord>(raw)
                    .ok()
                    .is_none_or(|record| record.expires <= now);
                expired.then(|| key.trim_start_matches("@read-observation/").to_owned())
            })
            .collect();
        for key in old_keys {
            vault.delete("@read-observation", &key)?;
        }
        let mut references = vec![];
        for observation in observations {
            if observation.account_id != id
                || !(200..300).contains(&observation.status)
                || !matches!(observation.method.as_str(), "GET" | "POST")
            {
                continue;
            }
            let reference = format!("ro_{}", util::id());
            let record = ObservationRecord {
                account: id.into(),
                binding: binding.clone(),
                observation,
                expires: now + CURSOR_TTL_MS,
            };
            vault.set(
                "@read-observation",
                &reference,
                &serde_json::to_string(&record)?,
            )?;
            references.push(ReadObservationReference {
                reference,
                method: record.observation.method.clone(),
                path: record.observation.path.clone(),
                query: record.observation.query.clone(),
                body: record.observation.body.clone(),
                status: record.observation.status,
            });
            if references.len() >= 32 {
                break;
            }
        }
        Ok(references)
    }

    fn account_read_snapshot(&self, id: &str) -> AppResult<Snapshot> {
        let account = self
            .state
            .read()?
            .accounts
            .get(id)
            .cloned()
            .ok_or_else(|| AppError::api("auth_required", 401))?;
        let credential = self.credential(id)?;
        if !account.verified_identity
            || account.status != "connected"
            || net::validate_url(&account.site_url)?.origin()
                != net::validate_url(&credential.origin)?.origin()
        {
            return Err(AppError::api("auth_required", 401));
        }
        let binding = credential_binding(&account, &credential)?;
        Ok(Snapshot {
            account,
            credential,
            binding,
        })
    }

    fn ensure_account_binding(&self, id: &str, expected: &str) -> AppResult<()> {
        let snapshot = self.account_read_snapshot(id)?;
        if snapshot.binding != expected {
            return Err(AppError::api("auth_required", 401));
        }
        Ok(())
    }

    fn read_secrets(&self, credential: &Credential) -> AppResult<Vec<String>> {
        let mut vault = self.vault.lock().map_err(|_| invalid())?;
        vault.reload()?;
        let mut secrets = credential.redaction_values();
        secrets.extend(accounts::collect_redaction_secrets(&vault));
        secrets.sort_by_key(|value| std::cmp::Reverse(value.len()));
        secrets.dedup();
        Ok(secrets)
    }

    fn matches_observation(&self, account: &AccountView, prepared: &Prepared) -> AppResult<bool> {
        let data = self.state.read()?;
        Ok(data.sites.get(&account.site_id).is_some_and(|site| {
            site.observations
                .iter()
                .any(|observed| observation_matches(observed, prepared))
        }))
    }

    fn resolve_observation(
        &self,
        reference: &str,
        account: &str,
        binding: &str,
    ) -> AppResult<Observation> {
        if reference.len() != 35
            || !reference.starts_with("ro_")
            || !reference[3..].bytes().all(|byte| byte.is_ascii_hexdigit())
        {
            return Err(unsupported());
        }
        let mut vault = self.vault.lock().map_err(|_| invalid())?;
        vault.reload()?;
        let raw = vault
            .get("@read-observation", reference)
            .ok_or_else(unsupported)?;
        let record: ObservationRecord = serde_json::from_str(raw).map_err(|_| unsupported())?;
        if record.account != account
            || record.binding != binding
            || record.expires <= util::unix_ms()
        {
            return Err(unsupported());
        }
        Ok(record.observation)
    }

    fn resolve_cursor(&self, handle: &str, account: &str, binding: &str) -> AppResult<String> {
        if handle.len() != 35
            || !handle.starts_with("rc_")
            || !handle[3..].bytes().all(|byte| byte.is_ascii_hexdigit())
        {
            return Err(invalid());
        }
        let mut vault = self.vault.lock().map_err(|_| invalid())?;
        vault.reload()?;
        let raw = vault.get("@read-cursor", handle).ok_or_else(unsupported)?;
        let record: CursorRecord = serde_json::from_str(raw).map_err(|_| unsupported())?;
        if record.account != account
            || record.binding != binding
            || record.expires <= util::unix_ms()
        {
            return Err(unsupported());
        }
        Ok(record.value)
    }

    fn store_cursor(&self, account: &str, binding: &str, value: String) -> AppResult<String> {
        if value.is_empty() || value.len() > 4096 || value.chars().any(char::is_control) {
            return Err(AppError::api("pagination_invalid", 400));
        }
        let handle = format!("rc_{}", util::id());
        let record = CursorRecord {
            account: account.into(),
            binding: binding.into(),
            value,
            expires: util::unix_ms() + CURSOR_TTL_MS,
        };
        let mut vault = self.vault.lock().map_err(|_| invalid())?;
        vault.set("@read-cursor", &handle, &serde_json::to_string(&record)?)?;
        Ok(handle)
    }
}

impl ReadPagination {
    fn validate(&self, request: &Prepared) -> AppResult<()> {
        let using_query = !self.query.is_empty();
        let using_variable = !self.variable.is_empty();
        let special_timeline_pointer = self.next == "$x_timeline_bottom";
        if using_query == using_variable
            || self.next.is_empty()
            || (!special_timeline_pointer && !self.next.starts_with('/'))
            || self.next.len() > 256
            || (!special_timeline_pointer && self.next.split('/').skip(1).any(str::is_empty))
        {
            return Err(invalid());
        }
        if using_query {
            if request.method != "GET"
                || request.url.query_pairs().any(|(key, _)| key == self.query)
            {
                return Err(invalid());
            }
        } else if !request.graphql
            || request.method != "POST"
            || !request
                .body
                .as_ref()
                .and_then(|body| body.get("variables"))
                .is_some_and(Value::is_object)
        {
            return Err(invalid());
        } else if request
            .body
            .as_ref()
            .and_then(|body| body.get("variables"))
            .and_then(Value::as_object)
            .is_some_and(|variables| {
                variables
                    .get(&self.variable)
                    .is_some_and(|value| !value.is_null())
            })
        {
            return Err(invalid());
        }
        for name in [&self.query, &self.variable] {
            if !name.is_empty()
                && (name.len() > 64
                    || !name
                        .bytes()
                        .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-')
                    || util::secret_field(name) && !is_cursor_name(name))
            {
                return Err(invalid());
            }
        }
        Ok(())
    }

    fn apply(&self, request: &mut Prepared, value: &str) -> AppResult<()> {
        if !self.query.is_empty() {
            request
                .url
                .query_pairs_mut()
                .append_pair(&self.query, value);
        } else {
            let body = request.body.as_mut().ok_or_else(invalid)?;
            let variables = body
                .get_mut("variables")
                .and_then(Value::as_object_mut)
                .ok_or_else(invalid)?;
            variables.insert(self.variable.clone(), Value::String(value.into()));
        }
        Ok(())
    }
}

fn credential_binding(account: &AccountView, credential: &Credential) -> AppResult<String> {
    Ok(util::hash(&serde_json::to_vec(&(
        account,
        &credential.origin,
        credential,
    ))?))
}

fn unique_query_keys(url: &url::Url) -> AppResult<BTreeSet<String>> {
    let mut keys = BTreeSet::new();
    for (key, _) in url.query_pairs() {
        if !keys.insert(key.into_owned()) {
            return Err(invalid());
        }
    }
    Ok(keys)
}

fn query_keys(url: &url::Url) -> AppResult<BTreeSet<String>> {
    unique_query_keys(url)
}

fn observation_matches(observed: &Observation, prepared: &Prepared) -> bool {
    (200..300).contains(&observed.status)
        && observed.method.eq_ignore_ascii_case(&prepared.method)
        && observed.path == prepared.url.path()
        && query_keys(&prepared.url).ok() == Some(observed.query.keys().cloned().collect())
        && body_schema_matches(observed, prepared)
}

fn body_schema_matches(observed: &Observation, prepared: &Prepared) -> bool {
    let expected: BTreeMap<String, String> = if let Some(body) = &prepared.body {
        let Some(object) = body.as_object() else {
            return false;
        };
        object
            .iter()
            .filter(|(key, _)| !util::secret_field(key))
            .map(|(key, value)| (key.clone(), json_kind(value).into()))
            .collect()
    } else if let Some(form) = &prepared.form {
        form.keys()
            .filter(|key| !util::secret_field(key))
            .map(|key| (key.clone(), "string".into()))
            .collect()
    } else {
        return observed.body.is_empty();
    };
    if expected.len() != observed.body.len() {
        return false;
    }
    expected.iter().all(|(key, kind)| {
        observed
            .body
            .get(key)
            .is_some_and(|observed_kind| kind_matches(observed_kind, kind))
    })
}

fn json_kind(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "bool",
        Value::Number(_) => "number",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
        Value::String(_) => "string",
    }
}

fn kind_matches(observed: &str, expected: &str) -> bool {
    observed == expected
        || (observed == "int" || observed == "float") && expected == "number"
        || observed == "json" && matches!(expected, "object" | "array")
}

fn looks_like_read_post(path: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    [
        "search", "query", "lookup", "list", "find", "count", "preview", "resolve", "fetch",
        "read", "filter", "check", "inspect", "graphql", "/gql",
    ]
    .iter()
    .any(|word| lower.contains(word))
}

fn contains_change_intent(value: &Value) -> bool {
    match value {
        Value::Object(fields) => fields.iter().any(|(key, child)| {
            (policy::INTENT_KEYS.contains(&key.to_ascii_lowercase().as_str())
                && contains_dangerous_value(child))
                || contains_change_intent(child)
        }),
        Value::Array(values) => values.iter().any(contains_change_intent),
        _ => false,
    }
}

fn contains_dangerous_value(value: &Value) -> bool {
    match value {
        Value::String(text) => policy::dangerous_intent(text),
        Value::Object(fields) => fields.values().any(contains_dangerous_value),
        Value::Array(values) => values.iter().any(contains_dangerous_value),
        _ => false,
    }
}

fn validate_graphql_query(query: &str) -> AppResult<()> {
    use graphql_parser::query::{Definition, OperationDefinition};
    let document = graphql_parser::parse_query::<&str>(query).map_err(|_| unsupported())?;
    let operations: Vec<_> = document
        .definitions
        .iter()
        .filter_map(|definition| match definition {
            Definition::Operation(operation) => Some(operation),
            _ => None,
        })
        .collect();
    if operations.len() != 1
        || operations.iter().any(|operation| {
            matches!(
                operation,
                OperationDefinition::Mutation(_) | OperationDefinition::Subscription(_)
            )
        })
    {
        return Err(unsupported());
    }
    Ok(())
}

fn screen_value(value: &Value, secrets: &[String]) -> AppResult<()> {
    let mut redacted = value.clone();
    util::redact(&mut redacted, &input_secrets(secrets));
    if redacted != *value {
        return Err(invalid());
    }
    Ok(())
}

fn input_secrets(secrets: &[String]) -> Vec<String> {
    secrets
        .iter()
        .filter(|secret| {
            secret.chars().count() >= 8
                || !serde_json::from_str::<Value>(secret).is_ok_and(|value| value.is_number())
        })
        .cloned()
        .collect()
}

fn pagination_value<'a>(value: &'a Value, path: &str) -> Option<&'a Value> {
    if path == "$x_timeline_bottom" {
        return find_bottom_cursor(value);
    }
    value.pointer(path)
}

fn find_bottom_cursor(value: &Value) -> Option<&Value> {
    fn visit<'a>(
        value: &'a Value,
        preferred: &mut Option<&'a Value>,
        fallback: &mut Option<&'a Value>,
    ) {
        match value {
            Value::Object(fields) => {
                if fields.get("cursorType").and_then(Value::as_str) == Some("Bottom")
                    && fields.get("value").is_some()
                {
                    let cursor = fields.get("value");
                    if fallback.is_none() {
                        *fallback = cursor;
                    }
                    if fields.get("entryType").and_then(Value::as_str)
                        == Some("TimelineTimelineCursor")
                    {
                        *preferred = cursor;
                    }
                }
                for child in fields.values() {
                    visit(child, preferred, fallback);
                }
            }
            Value::Array(values) => {
                for child in values {
                    visit(child, preferred, fallback);
                }
            }
            _ => {}
        }
    }
    let mut preferred = None;
    let mut fallback = None;
    visit(value, &mut preferred, &mut fallback);
    preferred.or(fallback)
}

fn scrub_x_timeline_cursors(value: &mut Value) {
    match value {
        Value::Object(fields) => {
            if fields.get("cursorType").and_then(Value::as_str) == Some("Bottom") {
                if let Some(cursor) = fields.get_mut("value") {
                    *cursor = Value::Null;
                }
            }
            fields.values_mut().for_each(scrub_x_timeline_cursors);
        }
        Value::Array(values) => values.iter_mut().for_each(scrub_x_timeline_cursors),
        _ => {}
    }
}

fn replace_string(value: &mut Value, needle: &str, replacement: &str) {
    match value {
        Value::String(text) => {
            if text.contains(needle) {
                *text = text.replace(needle, replacement);
            }
        }
        Value::Array(values) => values
            .iter_mut()
            .for_each(|child| replace_string(child, needle, replacement)),
        Value::Object(fields) => fields
            .values_mut()
            .for_each(|child| replace_string(child, needle, replacement)),
        _ => {}
    }
}

fn is_cursor_name(name: &str) -> bool {
    matches!(
        name.to_ascii_lowercase().replace(['-', '_'], "").as_str(),
        "cursor" | "pagetoken" | "nextpagetoken" | "continuationtoken"
    )
}
