//! Shared limits and single-use approvals for dashboard, CLI and MCP execution.
use crate::{
    apperr::{AppError, AppResult},
    spec::{Operation, Spec},
    util,
};
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, path::Path, sync::Mutex, time::Duration};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Effect {
    Read,
    Write,
    #[default]
    Unknown,
}
impl Effect {
    pub(crate) fn combine(self, other: Self) -> Self {
        match (self, other) {
            (Self::Write, _) | (_, Self::Write) => Self::Write,
            (Self::Unknown, _) | (_, Self::Unknown) => Self::Unknown,
            _ => Self::Read,
        }
    }
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Read => "read",
            Self::Write => "write",
            Self::Unknown => "unknown",
        }
    }
}

/// An operation's semantics matter even when its transport says GET.
pub fn dangerous_intent(value: &str) -> bool {
    let decoded =
        url::form_urlencoded::parse(format!("x={}", value.replace('+', "%2B")).as_bytes())
            .next()
            .map(|(_, v)| v.into_owned())
            .unwrap_or_else(|| value.into());
    let camel = regex::Regex::new(r"([a-z0-9])([A-Z])")
        .expect("constant")
        .replace_all(&decoded, "$1-$2");
    let lower = camel.to_ascii_lowercase();
    let words: Vec<_> = lower.split(|c: char| !c.is_ascii_alphanumeric()).collect();
    const CHANGES: &[&str] = &[
        "add",
        "edit",
        "change",
        "toggle",
        "commit",
        "upsert",
        "redeem",
        "confirm",
        "authorize",
        "login",
        "signin",
        "logout",
        "logoff",
        "signout",
        "delete",
        "remove",
        "destroy",
        "erase",
        "purge",
        "drop",
        "create",
        "insert",
        "update",
        "patch",
        "submit",
        "send",
        "publish",
        "unpublish",
        "upload",
        "transfer",
        "purchase",
        "checkout",
        "pay",
        "vote",
        "like",
        "unlike",
        "follow",
        "unfollow",
        "subscribe",
        "unsubscribe",
        "revoke",
        "reset",
        "activate",
        "deactivate",
        "enable",
        "disable",
        "execute",
        "trigger",
        "accept",
        "reject",
        "approve",
        "invite",
        "cancel",
        "terminate",
        "stop",
        "restart",
        "save",
        "markread",
        "markseen",
        "increment",
    ];
    words.iter().any(|w| CHANGES.contains(w))
        || ["sign-out", "log-out", "mark-as-read", "mark-as-seen"]
            .iter()
            .any(|w| lower.contains(w))
}
pub fn operation_effect(op: &Operation, args: &BTreeMap<String, String>) -> Effect {
    if op
        .body
        .as_ref()
        .is_some_and(|body| body.multipart.values().any(|p| p.kind == "file"))
    {
        return Effect::Write;
    }
    let method = op.method.to_ascii_uppercase();
    if dangerous_intent(&op.path) || dangerous_intent(&op.name) {
        return Effect::Write;
    }
    if ["PUT", "PATCH", "DELETE"].contains(&method.as_str()) {
        return Effect::Write;
    }
    if op.effect == Effect::Write {
        return Effect::Write;
    }
    let mut resolved = BTreeMap::new();
    for (key, param) in op
        .params
        .iter()
        .chain(op.query.iter())
        .chain(op.body.iter().flat_map(|b| b.inputs()))
    {
        if let Some(value) = &param.default {
            resolved.insert(key.clone(), util::plain(value));
        }
    }
    resolved.extend(args.clone());
    for (key, value) in &resolved {
        if INTENT_KEYS.contains(&key.to_ascii_lowercase().as_str()) && dangerous_intent(value) {
            return Effect::Write;
        }
    }
    for key in op.params.keys() {
        if resolved
            .get(key)
            .is_some_and(|value| dangerous_intent(value))
        {
            return Effect::Write;
        }
    }
    for value in resolved.values() {
        if serde_json::from_str::<serde_json::Value>(value)
            .ok()
            .is_some_and(|value| nested_change_intent(&value))
        {
            return Effect::Write;
        }
    }
    // Parse every candidate GraphQL document, regardless of the endpoint's name.
    // Conservatively require review for any mutation, subscription or ambiguous document.
    if method == "POST" || method == "GET" {
        if let Some(query) = resolved.get("query") {
            if let Some(effect) =
                graphql_document_effect(query, op.path.to_ascii_lowercase().contains("graphql"))
            {
                return if effect == Effect::Read
                    && (op.effect != Effect::Read || op.evidence.is_empty())
                {
                    Effect::Unknown
                } else {
                    effect
                };
            }
        } else if op.path.to_ascii_lowercase().contains("graphql") {
            return Effect::Unknown;
        }
    }
    if method == "GET" || method == "HEAD" || method == "OPTIONS" {
        return if op.effect == Effect::Read && !op.evidence.is_empty() {
            Effect::Read
        } else {
            Effect::Unknown
        };
    }
    if method == "POST" && op.effect == Effect::Read && !op.evidence.is_empty() {
        // Explicitly documented search/query/lookup operations may use POST.
        let lower = format!("{} {}", op.name, op.path).to_ascii_lowercase();
        if [
            "search", "query", "lookup", "list", "find", "count", "preview", "resolve", "get",
            "fetch", "read", "filter", "check", "inspect",
        ]
        .iter()
        .any(|word| {
            lower
                .split(|c: char| !c.is_ascii_alphanumeric())
                .any(|p| p == *word)
        }) {
            return Effect::Read;
        }
    }
    Effect::Unknown
}

/// Known overrides can change the effective method or target at an intermediary.
/// Their server-specific interpretation is not evidence of a supported read.
pub(crate) fn request_headers_effect<'a>(
    headers: impl IntoIterator<Item = (&'a str, &'a str)>,
) -> Effect {
    if headers.into_iter().any(|(name, _)| {
        matches!(
            name.to_ascii_lowercase().as_str(),
            "x-http-method-override"
                | "x-http-method"
                | "x-method-override"
                | "x-original-url"
                | "x-rewrite-url"
        )
    }) {
        Effect::Unknown
    } else {
        Effect::Read
    }
}

/// Classify the request that will actually be sent, not just its declared inputs.
/// Read evidence never overrides a write or an opaque protocol envelope.
pub(crate) fn rendered_request_effect(
    op: &Operation,
    args: &BTreeMap<String, String>,
    url: &str,
    body: Option<&[u8]>,
) -> Effect {
    let mut effect = operation_effect(op, args).combine(request_headers_effect(
        op.headers
            .iter()
            .map(|(name, value)| (name.as_str(), value.as_str())),
    ));
    let Ok(url) = url::Url::parse(url) else {
        return effect.combine(Effect::Unknown);
    };
    if dangerous_intent(url.path()) {
        return Effect::Write;
    }
    let opaque_field = op
        .pagination
        .as_ref()
        .map(|pagination| pagination.query.as_str());
    let (query, duplicate) =
        encoded_fields(url.query().unwrap_or_default().as_bytes(), opaque_field);
    let (query_effect, mut document) = payload_effect(&query);
    effect = effect.combine(query_effect);
    if duplicate {
        effect = effect.combine(Effect::Unknown);
    }
    if let Some(body) = body {
        let value = if op.body.as_ref().is_some_and(|body| !body.form.is_empty()) {
            let (value, duplicate) = encoded_fields(body, None);
            if duplicate {
                effect = effect.combine(Effect::Unknown);
            }
            Ok(value)
        } else {
            serde_json::from_slice(body)
        };
        match value {
            Ok(value) => {
                let (body_effect, body_document) = payload_effect(&value);
                effect = effect.combine(body_effect);
                document |= body_document;
            }
            Err(_) => effect = effect.combine(Effect::Unknown),
        }
    }
    let path = url.path().to_ascii_lowercase();
    if (path.contains("graphql") || path.ends_with("/gql")) && !document {
        effect = effect.combine(Effect::Unknown);
    }
    effect
}

fn encoded_fields(encoded: &[u8], opaque_field: Option<&str>) -> (serde_json::Value, bool) {
    let mut fields = serde_json::Map::new();
    let mut duplicate = false;
    for (key, value) in url::form_urlencoded::parse(encoded) {
        // A continuation is opaque data unless its actual field is a protocol selector.
        // Query itself is a document/search string, not JSON serialization.
        let protocol = matches!(
            key.to_ascii_lowercase().as_str(),
            "query" | "variables" | "extensions" | "operationname" | "queryid" | "persistedquery"
        ) || INTENT_KEYS.contains(&key.to_ascii_lowercase().as_str());
        let value = if key == "query" || (opaque_field == Some(key.as_ref()) && !protocol) {
            serde_json::Value::String(value.into_owned())
        } else {
            serde_json::from_str(&value)
                .unwrap_or_else(|_| serde_json::Value::String(value.into_owned()))
        };
        match fields.entry(key.into_owned()) {
            serde_json::map::Entry::Vacant(entry) => {
                entry.insert(value);
            }
            serde_json::map::Entry::Occupied(mut entry) => {
                // Repeated ordinary filters are data. Repeated protocol selectors
                // are ambiguous; retain every value so nested intents are not lost.
                duplicate |= protocol;
                let previous = std::mem::replace(entry.get_mut(), serde_json::Value::Null);
                entry.insert(serde_json::Value::Array(vec![previous, value]));
            }
        }
    }
    (serde_json::Value::Object(fields), duplicate)
}

/// Inspect structured request envelopes; arbitrary data strings are not commands.
/// The boolean identifies a classified GraphQL candidate, not validated read authority.
pub(crate) fn payload_effect(value: &serde_json::Value) -> (Effect, bool) {
    use serde_json::Value;
    if nested_change_intent(value) {
        return (Effect::Write, false);
    }
    match value {
        Value::Object(fields) => {
            let envelope = fields.keys().any(|key| {
                matches!(
                    key.to_ascii_lowercase().as_str(),
                    "operationname" | "queryid" | "persistedquery"
                )
            }) || fields
                .get("extensions")
                .and_then(Value::as_object)
                .is_some_and(|fields| {
                    fields
                        .keys()
                        .any(|key| key.eq_ignore_ascii_case("persistedQuery"))
                });
            let query = fields.get("query").and_then(Value::as_str);
            let query_effect = query.and_then(|query| graphql_document_effect(query, envelope));
            let mut effect = query_effect.unwrap_or(if envelope {
                Effect::Unknown
            } else {
                Effect::Read
            });
            let mut document = query_effect.is_some();
            for (key, value) in fields {
                // Variables and extensions belong to this document, not a second
                // executable GraphQL envelope. Nested change intents were checked above.
                if query_effect.is_some() && matches!(key.as_str(), "variables" | "extensions") {
                    continue;
                }
                let (nested, nested_document) = payload_effect(value);
                effect = effect.combine(nested);
                document |= nested_document;
            }
            (effect, document)
        }
        Value::Array(values) => {
            values
                .iter()
                .fold((Effect::Read, false), |(effect, document), value| {
                    let (nested, nested_document) = payload_effect(value);
                    (effect.combine(nested), document || nested_document)
                })
        }
        _ => (Effect::Read, false),
    }
}

/// Count structure outside GraphQL comments, normal strings and block strings
/// before entering the recursive GraphQL parser. This limits depth only; the
/// parser remains responsible for syntax, including unterminated strings.
pub(crate) fn graphql_depth_is_bounded(body: &str) -> bool {
    let bytes = body.as_bytes();
    let mut at = 0;
    let mut depth = 0usize;
    while at < bytes.len() {
        match bytes[at] {
            b'#' => {
                while at < bytes.len() && !matches!(bytes[at], b'\r' | b'\n') {
                    at += 1;
                }
                continue;
            }
            b'"' => {
                let block = bytes[at..].starts_with(b"\"\"\"");
                at += if block { 3 } else { 1 };
                let mut closed = false;
                while at < bytes.len() {
                    if block && bytes[at..].starts_with(b"\\\"\"\"") {
                        // Block strings escape only a triple quote, not an
                        // arbitrary following character or another backslash.
                        at += 4;
                    } else if !block && bytes[at] == b'\\' {
                        at += 2;
                    } else if block && bytes[at..].starts_with(b"\"\"\"") {
                        at += 3;
                        closed = true;
                        break;
                    } else if !block && bytes[at] == b'"' {
                        at += 1;
                        closed = true;
                        break;
                    } else {
                        at += 1;
                    }
                }
                if !closed {
                    return true; // Malformed syntax ends here; no later structure can execute.
                }
                continue;
            }
            b'{' | b'[' | b'(' => {
                depth += 1;
                // graphql-parser's own limit (50) still overflows a 1 MiB
                // thread stack in an unoptimized build. Keep our measured
                // smaller structural limit before entering that parser.
                if depth > 32 {
                    return false;
                }
            }
            b'}' | b']' | b')' => {
                let Some(next) = depth.checked_sub(1) else {
                    return true; // The parser rejects this unmatched closer before any suffix.
                };
                depth = next;
            }
            _ => {}
        }
        at += 1;
    }
    true
}

/// One bounded entry into graphql-parser for every production read classifier.
/// None is an unknown/unsupported document, never authority to dispatch.
pub(crate) fn parse_graphql_bounded(
    query: &str,
) -> Option<graphql_parser::query::Document<'_, &str>> {
    if !graphql_depth_is_bounded(query) {
        return None;
    }
    graphql_parser::parse_query::<&str>(query).ok()
}

fn graphql_document_effect(query: &str, required: bool) -> Option<Effect> {
    use graphql_parser::query::{Definition, OperationDefinition};
    // A limit failure is unknown even if a permitted lexical prefix (such as a
    // BOM) prevents the fallback name heuristic from recognizing the document.
    if !graphql_depth_is_bounded(query) {
        return Some(Effect::Unknown);
    }
    match parse_graphql_bounded(query) {
        Some(document) => {
            let operations: Vec<_> = document
                .definitions
                .iter()
                .filter_map(|definition| {
                    if let Definition::Operation(operation) = definition {
                        Some(operation)
                    } else {
                        None
                    }
                })
                .collect();
            Some(
                if operations.iter().any(|operation| {
                    matches!(
                        operation,
                        OperationDefinition::Mutation(_) | OperationDefinition::Subscription(_)
                    )
                }) {
                    Effect::Write
                } else if operations.len() == 1 {
                    Effect::Read
                } else {
                    Effect::Unknown
                },
            )
        }
        None if required
            || query.trim_start().starts_with(['{', '#'])
            || ["query", "mutation", "subscription", "fragment"]
                .iter()
                .any(|word| query.trim_start().starts_with(word)) =>
        {
            Some(Effect::Unknown)
        }
        None => None,
    }
}

pub const INTENT_KEYS: &[&str] = &["action", "operation", "op", "method", "command", "mode"];

pub(crate) fn nested_change_intent(value: &serde_json::Value) -> bool {
    match value {
        serde_json::Value::Object(object) => object.iter().any(|(key, value)| {
            (INTENT_KEYS.contains(&key.to_ascii_lowercase().as_str())
                && value.as_str().is_some_and(dangerous_intent))
                || nested_change_intent(value)
        }),
        serde_json::Value::Array(array) => array.iter().any(nested_change_intent),
        _ => false,
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ExecutionRequest {
    pub site_id: String,
    pub action_id: String,
    #[serde(default)]
    pub account_id: String,
    #[serde(default)]
    pub inputs: BTreeMap<String, String>,
    pub spec_hash: String,
    #[serde(default)]
    pub credential_hash: String,
}
impl ExecutionRequest {
    pub fn digest(&self) -> AppResult<String> {
        Ok(util::hash(&serde_json::to_vec(self)?))
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ReviewInput {
    pub label: String,
    pub value: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Approval {
    pub id: String,
    pub site_id: String,
    pub site_title: String,
    pub action_id: String,
    pub action_title: String,
    pub account_id: String,
    pub account_label: String,
    pub inputs: Vec<ReviewInput>,
    pub expires_at: String,
    pub status: String,
    #[serde(skip)]
    pub request: Option<ExecutionRequest>,
}

const REQUEST_LEASE_MS: i64 = 120_000;

pub struct Policy {
    conn: Mutex<Connection>,
    interval_ms: i64,
    budget: i64,
}
impl Policy {
    pub fn open(path: &Path) -> AppResult<Self> {
        Self::with_limits(path, 1200, 60)
    }
    pub fn with_limits(path: &Path, interval_ms: i64, budget: i64) -> AppResult<Self> {
        if let Some(parent) = path.parent() {
            util::private_dir(parent)?;
        }
        // Bootstrap completes before Runtime opens its other stores, so this guard
        // never nests with their startup locks or survives on the Policy value.
        let _startup = util::file_lock(&path.with_extension("db.startup.lock"), false)?;
        let conn = Connection::open(path)?;
        conn.busy_timeout(Duration::from_secs(5))?;
        conn.execute_batch("PRAGMA journal_mode=WAL;
            CREATE TABLE IF NOT EXISTS approvals(id TEXT PRIMARY KEY, digest TEXT NOT NULL, request TEXT NOT NULL, review TEXT NOT NULL, state TEXT NOT NULL, expires_ms INTEGER NOT NULL);
            CREATE TABLE IF NOT EXISTS request_gate(scope TEXT PRIMARY KEY, lease TEXT NOT NULL DEFAULT '', lease_until INTEGER NOT NULL DEFAULT 0, next_at INTEGER NOT NULL DEFAULT 0, cooldown INTEGER NOT NULL DEFAULT 0, paused TEXT NOT NULL DEFAULT '');
            CREATE TABLE IF NOT EXISTS request_event(scope TEXT NOT NULL, at INTEGER NOT NULL);
            CREATE INDEX IF NOT EXISTS request_event_scope_time ON request_event(scope,at);
            CREATE TABLE IF NOT EXISTS strategy_gate(strategy_key TEXT PRIMARY KEY, reason TEXT NOT NULL DEFAULT '', until_ms INTEGER NOT NULL DEFAULT 0, failures INTEGER NOT NULL DEFAULT 0);
            CREATE TABLE IF NOT EXISTS origin_transport(origin TEXT PRIMARY KEY, preferred_transport TEXT NOT NULL DEFAULT '', updated_ms INTEGER NOT NULL DEFAULT 0);")?;
        Ok(Self {
            conn: Mutex::new(conn),
            interval_ms: interval_ms.max(0),
            budget: budget.max(1),
        })
    }
    pub fn request_approval(
        &self,
        request: ExecutionRequest,
        site_title: String,
        action_title: String,
        account_label: String,
        inputs: Vec<ReviewInput>,
    ) -> AppResult<Approval> {
        let expires_ms = util::unix_ms() + 5 * 60 * 1000;
        let review = Approval {
            id: util::id(),
            site_id: request.site_id.clone(),
            site_title,
            action_id: request.action_id.clone(),
            action_title,
            account_id: request.account_id.clone(),
            account_label,
            inputs,
            expires_at: chrono::DateTime::from_timestamp_millis(expires_ms)
                .expect("current timestamp")
                .to_rfc3339(),
            status: "pending".into(),
            request: Some(request.clone()),
        };
        let conn = self
            .conn
            .lock()
            .map_err(|_| AppError::api("internal", 500))?;
        conn.execute("INSERT INTO approvals(id,digest,request,review,state,expires_ms) VALUES(?1,?2,?3,?4,'pending',?5)", params![review.id, request.digest()?, serde_json::to_string(&request)?, serde_json::to_string(&review)?, expires_ms])?;
        Ok(review)
    }
    pub fn pending(&self) -> AppResult<Vec<Approval>> {
        let conn = self
            .conn
            .lock()
            .map_err(|_| AppError::api("internal", 500))?;
        let mut statement = conn.prepare(
            "SELECT review, state FROM approvals WHERE state='pending' AND expires_ms>?1 ORDER BY rowid",
        )?;
        let rows = statement.query_map([util::unix_ms()], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
        })?;
        let mut result = Vec::new();
        for row in rows {
            let (raw, state) = row?;
            let mut item: Approval = serde_json::from_str(&raw)?;
            item.status = state;
            result.push(item);
        }
        Ok(result)
    }
    pub fn review(&self, id: &str) -> AppResult<Approval> {
        let conn = self
            .conn
            .lock()
            .map_err(|_| AppError::api("internal", 500))?;
        let row: Option<(String, String, String, i64)> = conn
            .query_row(
                "SELECT review, request, state, expires_ms FROM approvals WHERE id=?1",
                [id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
            )
            .optional()?;
        let (review, request, state, expires) =
            row.ok_or_else(|| AppError::api("not_found", 404))?;
        if state != "pending" || expires <= util::unix_ms() {
            return Err(AppError::api("approval_expired", 409));
        }
        let mut review: Approval = serde_json::from_str(&review)?;
        review.status = state;
        review.request = Some(serde_json::from_str(&request)?);
        Ok(review)
    }
    pub(crate) fn verify_consumable(&self, id: &str, request: &ExecutionRequest) -> AppResult<()> {
        let conn = self
            .conn
            .lock()
            .map_err(|_| AppError::api("internal", 500))?;
        let exists: Option<i64> = conn
            .query_row(
                "SELECT 1 FROM approvals WHERE id=?1 AND digest=?2 AND state IN ('pending','approved') AND expires_ms>?3",
                params![id, request.digest()?, util::unix_ms()],
                |r| r.get(0),
            )
            .optional()?;
        if exists.is_none() {
            return Err(AppError::api("approval_expired", 409));
        }
        Ok(())
    }
    /// Only the browser approval handler calls this after authenticating its session.
    /// Consumption happens at the commit barrier right before the network request is dispatched.
    pub(crate) fn consume(&self, id: &str, request: &ExecutionRequest) -> AppResult<()> {
        let conn = self
            .conn
            .lock()
            .map_err(|_| AppError::api("internal", 500))?;
        let affected = conn.execute("UPDATE approvals SET state='consumed' WHERE id=?1 AND digest=?2 AND state IN ('pending','approved') AND expires_ms>?3", params![id, request.digest()?, util::unix_ms()])?;
        if affected != 1 {
            return Err(AppError::api("approval_expired", 409));
        }
        Ok(())
    }
    pub fn reject(&self, id: &str) -> AppResult<()> {
        self.conn
            .lock()
            .map_err(|_| AppError::api("internal", 500))?
            .execute(
                "UPDATE approvals SET state='rejected' WHERE id=?1 AND state='pending'",
                [id],
            )?;
        Ok(())
    }
    pub fn reject_claimed_or_pending(&self, id: &str) -> AppResult<()> {
        self.conn
            .lock()
            .map_err(|_| AppError::api("internal", 500))?
            .execute(
                "UPDATE approvals SET state='rejected' WHERE id=?1 AND state IN ('pending','approved')",
                [id],
            )?;
        Ok(())
    }
    pub fn claim(&self, id: &str) -> AppResult<()> {
        let changed=self.conn.lock().map_err(|_|AppError::api("internal",500))?.execute("UPDATE approvals SET state='approved' WHERE id=?1 AND state='pending' AND expires_ms>?2",params![id,util::unix_ms()])?;
        if changed != 1 {
            return Err(AppError::api("approval_expired", 409));
        }
        Ok(())
    }
    pub fn unclaim(&self, id: &str) -> AppResult<()> {
        self.conn
            .lock()
            .map_err(|_| AppError::api("internal", 500))?
            .execute(
                "UPDATE approvals SET state='pending' WHERE id=?1 AND state='approved' AND expires_ms>?2",
                params![id, util::unix_ms()],
            )?;
        Ok(())
    }
    pub fn invalidate(&self, site: Option<&str>, account: Option<&str>) -> AppResult<()> {
        self.conn.lock().map_err(|_|AppError::api("internal",500))?.execute("UPDATE approvals SET state='rejected' WHERE state IN ('pending','approved') AND ((?1 IS NOT NULL AND json_extract(request,'$.site_id')=?1) OR (?2 IS NOT NULL AND json_extract(request,'$.account_id')=?2))",params![site,account])?;
        Ok(())
    }
    pub async fn acquire(&self, scope: &str) -> AppResult<String> {
        let start = util::unix_ms();
        loop {
            let token = util::id();
            let wait = {
                let mut conn = self
                    .conn
                    .lock()
                    .map_err(|_| AppError::api("internal", 500))?;
                let tx =
                    conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
                tx.execute(
                    "INSERT INTO request_gate(scope) VALUES(?1) ON CONFLICT(scope) DO NOTHING",
                    [scope],
                )?;
                let (lease_until, next_at, mut cooldown, mut paused): (i64, i64, i64, String) = tx.query_row("SELECT lease_until,next_at,cooldown,paused FROM request_gate WHERE scope=?1", [scope], |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?)))?;
                let now = util::unix_ms();
                if !paused.is_empty() && (cooldown == 0 || now >= cooldown) {
                    tx.execute(
                        "UPDATE request_gate SET paused='',cooldown=0 WHERE scope=?1",
                        [scope],
                    )?;
                    paused.clear();
                    cooldown = 0;
                }
                if !paused.is_empty() {
                    return Err(AppError::api(
                        &paused,
                        if paused == "auth_required" { 401 } else { 429 },
                    ));
                }
                if cooldown > now {
                    return Err(AppError::api("rate_limited", 429));
                }
                let count: i64 = tx.query_row(
                    "SELECT COUNT(*) FROM request_event WHERE scope=?1 AND at>?2",
                    params![scope, now - 600_000],
                    |r| r.get(0),
                )?;
                if count >= self.budget {
                    return Err(AppError::api("rate_limited", 429));
                }
                let wait = (lease_until.max(next_at) - now).max(0);
                if wait == 0 {
                    let jitter = if self.interval_ms >= 500 {
                        ((now as u64 ^ token.as_bytes().first().copied().unwrap_or(0) as u64) % 180)
                            as i64
                    } else {
                        0
                    };
                    tx.execute(
                        "UPDATE request_gate SET lease=?2,lease_until=?3,next_at=?4 WHERE scope=?1",
                        params![
                            scope,
                            token,
                            now + REQUEST_LEASE_MS,
                            now + self.interval_ms + jitter
                        ],
                    )?;
                    tx.execute(
                        "INSERT INTO request_event(scope,at) VALUES(?1,?2)",
                        params![scope, now],
                    )?;
                    tx.execute("DELETE FROM request_event WHERE at<?1", [now - 3_600_000])?;
                }
                tx.commit()?;
                wait
            };
            if wait == 0 {
                return Ok(token);
            }
            if util::unix_ms() - start > 45_000 {
                return Err(AppError::api("timeout", 408));
            }
            tokio::time::sleep(Duration::from_millis(wait.min(500) as u64)).await;
        }
    }
    /// Extend only an unexpired lease still owned by this exact token. Never
    /// reacquire or revive an expired lease: the caller must stop its attempt.
    pub(crate) fn renew(&self, scope: &str, token: &str) -> AppResult<()> {
        let mut conn = self
            .conn
            .lock()
            .map_err(|_| AppError::api("internal", 500))?;
        // Get the SQLite write lock before sampling time. Otherwise a busy
        // database could let a stale pre-wait timestamp revive an expired lease.
        let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let now = util::unix_ms();
        let changed = tx.execute(
            "UPDATE request_gate SET lease_until=?3 WHERE scope=?1 AND lease=?2 AND lease<>'' AND lease_until>?4",
            params![scope, token, now + REQUEST_LEASE_MS, now],
        )?;
        if changed != 1 {
            return Err(AppError::api("request_lease_lost", 409));
        }
        tx.commit()?;
        Ok(())
    }
    pub fn release(
        &self,
        scope: &str,
        token: &str,
        status: u16,
        retry_after: Option<Duration>,
        _challenge: bool,
    ) -> AppResult<()> {
        let conn = self
            .conn
            .lock()
            .map_err(|_| AppError::api("internal", 500))?;
        let cooldown = if status == 429 || retry_after.is_some() {
            util::unix_ms()
                + retry_after
                    .unwrap_or(Duration::from_secs(60))
                    .as_millis()
                    .min(i64::MAX as u128) as i64
        } else {
            0
        };
        conn.execute("UPDATE request_gate SET lease='',lease_until=0,cooldown=MAX(cooldown,?3),paused='' WHERE scope=?1 AND lease=?2", params![scope,token,cooldown])?;
        Ok(())
    }
    pub fn strategy_key(method: &str, raw_url: &str, transport: &str, account: &str) -> String {
        let norm_url = url::Url::parse(raw_url)
            .map(|u| format!("{}{}", u.origin().ascii_serialization(), u.path()))
            .unwrap_or_else(|_| raw_url.to_string());
        let norm_transport = if transport.is_empty() || transport == "direct" {
            "http"
        } else {
            transport
        };
        format!(
            "{}|{}|{}|{}",
            method.to_ascii_uppercase(),
            norm_url,
            norm_transport,
            account
        )
    }
    pub fn is_strategy_blocked(&self, key: &str) -> AppResult<Option<String>> {
        let conn = self
            .conn
            .lock()
            .map_err(|_| AppError::api("internal", 500))?;
        let now = util::unix_ms();
        let row: Option<(String, i64)> = conn
            .query_row(
                "SELECT reason, until_ms FROM strategy_gate WHERE strategy_key=?1",
                [key],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        match row {
            Some((reason, until_ms)) if until_ms > now && !reason.is_empty() => Ok(Some(reason)),
            Some(_) => {
                let _ = conn.execute("DELETE FROM strategy_gate WHERE strategy_key=?1", [key]);
                Ok(None)
            }
            None => Ok(None),
        }
    }
    pub fn record_blocked_strategy(&self, key: &str, reason: &str, ttl_ms: i64) -> AppResult<()> {
        let conn = self
            .conn
            .lock()
            .map_err(|_| AppError::api("internal", 500))?;
        let until_ms = util::unix_ms() + ttl_ms.max(1_000);
        conn.execute(
            "INSERT INTO strategy_gate(strategy_key, reason, until_ms, failures) VALUES(?1, ?2, ?3, 1)
             ON CONFLICT(strategy_key) DO UPDATE SET reason=excluded.reason, until_ms=excluded.until_ms, failures=strategy_gate.failures+1",
            params![key, reason, until_ms],
        )?;
        Ok(())
    }
    pub fn clear_strategy(&self, key: &str) -> AppResult<()> {
        let conn = self
            .conn
            .lock()
            .map_err(|_| AppError::api("internal", 500))?;
        conn.execute("DELETE FROM strategy_gate WHERE strategy_key=?1", [key])?;
        Ok(())
    }
    pub fn record_preferred_transport(&self, origin: &str, transport: &str) -> AppResult<()> {
        let conn = self
            .conn
            .lock()
            .map_err(|_| AppError::api("internal", 500))?;
        conn.execute(
            "INSERT INTO origin_transport(origin, preferred_transport, updated_ms) VALUES(?1, ?2, ?3)
             ON CONFLICT(origin) DO UPDATE SET preferred_transport=excluded.preferred_transport, updated_ms=excluded.updated_ms",
            params![origin, transport, util::unix_ms()],
        )?;
        Ok(())
    }
    pub fn preferred_transport(&self, origin: &str) -> AppResult<Option<String>> {
        let conn = self
            .conn
            .lock()
            .map_err(|_| AppError::api("internal", 500))?;
        let now = util::unix_ms();
        let row: Option<(String, i64)> = conn
            .query_row(
                "SELECT preferred_transport, updated_ms FROM origin_transport WHERE origin=?1",
                [origin],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        match row {
            Some((transport, updated_ms))
                if !transport.is_empty() && now - updated_ms <= 3_600_000 =>
            {
                Ok(Some(transport))
            }
            _ => Ok(None),
        }
    }
    fn clear_strategies_for_scope(conn: &Connection, scope: &str) -> AppResult<()> {
        let (origin, account) = match scope.split_once('|') {
            Some((origin, account)) => (origin.trim_end_matches('/'), Some(account)),
            None => (scope.trim_end_matches('/'), None),
        };
        if origin.is_empty() {
            return Ok(());
        }
        let mut stmt = conn.prepare("SELECT strategy_key FROM strategy_gate")?;
        let keys: Vec<String> = stmt
            .query_map([], |r| r.get(0))?
            .collect::<Result<Vec<_>, _>>()?;
        for key in keys {
            let mut parts = key.splitn(4, '|');
            let _method = parts.next();
            let Some(url_str) = parts.next() else {
                continue;
            };
            let _transport = parts.next();
            let Some(key_account) = parts.next() else {
                continue;
            };
            let same_origin = url_str == origin
                || url_str
                    .strip_prefix(origin)
                    .is_some_and(|rest| rest.starts_with('/'));
            let same_account = account.is_none_or(|a| a == "*" || key_account == a);
            if same_origin && same_account {
                conn.execute("DELETE FROM strategy_gate WHERE strategy_key=?1", [key])?;
            }
        }
        Ok(())
    }
    pub fn resume_auth(&self, scope: &str) -> AppResult<()> {
        let conn = self
            .conn
            .lock()
            .map_err(|_| AppError::api("internal", 500))?;
        conn.execute(
            "UPDATE request_gate SET paused='',cooldown=0 WHERE scope=?1 AND paused IN ('auth_required','blocked_by_site')",
            [scope],
        )?;
        Self::clear_strategies_for_scope(&conn, scope)?;
        Ok(())
    }
    pub fn resume(&self, scope: &str) -> AppResult<()> {
        let conn = self
            .conn
            .lock()
            .map_err(|_| AppError::api("internal", 500))?;
        if scope.contains('|') {
            conn.execute(
                "UPDATE request_gate SET paused='',cooldown=0 WHERE scope=?1",
                [scope],
            )?;
        } else {
            let origin = scope.trim_end_matches('/');
            conn.execute(
                "UPDATE request_gate SET paused='',cooldown=0 WHERE scope=?1 OR scope LIKE ?1 || '|%'",
                [origin],
            )?;
        }
        Self::clear_strategies_for_scope(&conn, scope)?;
        Ok(())
    }
}

pub fn validate_spec(sp: &Spec) -> AppResult<()> {
    if !util::valid_id(&sp.site.name) || !sp.validate().is_empty() || sp.operations.len() > 200 {
        return Err(AppError::api("data_invalid", 400));
    }
    let base = url::Url::parse(&sp.site.base_url).map_err(|_| AppError::api("bad_url", 400))?;
    if !sp.site.source_url.is_empty() {
        crate::net::validate_url(&sp.site.source_url)?;
    }
    if !["https", "http"].contains(&base.scheme())
        || !base.username().is_empty()
        || base.password().is_some()
        || base.host_str().is_none()
        || base.query().is_some()
        || base.fragment().is_some()
    {
        return Err(AppError::api("bad_url", 400));
    }
    let mut names = std::collections::HashSet::new();
    for op in &sp.operations {
        crate::spec::validate_pagination(sp, op)?;
        if !["", "http", "browser-fetch", "browser"].contains(&op.transport.as_str())
            || (op.transport == "browser"
                && (op.method != "GET"
                    || op.body.is_some()
                    || operation_effect(op, &Default::default()) != Effect::Read
                    || op
                        .response
                        .as_ref()
                        .is_none_or(|response| response.format != "html")
                    || sp
                        .find_auth(&op.auth)
                        .is_some_and(|auth| auth.kind == "header")))
        {
            return Err(AppError::api("data_invalid", 400));
        }
        // A templated base (for example a derived_target upload whose target
        // comes from a preflight/lease response) is resolved at runtime and
        // cannot be checked as a static URL here. A non-derived op with a
        // templated base is still rejected at execution by the origin gate.
        if !sp.operation_base(op).contains("{{") {
            let destination = crate::net::validate_url(sp.operation_base(op))?;
            if destination.query().is_some() || destination.fragment().is_some() {
                return Err(AppError::api("bad_url", 400));
            }
            let authority = if op.derived_target {
                crate::net::validate_url(&sp.site.base_url)?
            } else {
                destination.clone()
            };
            if let Some(auth) = sp.find_auth(&op.auth) {
                if auth.kind == "header" {
                    let scope = crate::net::validate_url(if auth.origin.is_empty() {
                        &sp.site.base_url
                    } else {
                        &auth.origin
                    })?;
                    if authority.origin() != scope.origin() {
                        return Err(AppError::api("unsafe_target", 400));
                    }
                } else if matches!(auth.kind.as_str(), "browser" | "derived") {
                    let scope = crate::net::validate_url(if auth.origin.is_empty() {
                        sp.site.source_url()
                    } else {
                        &auth.origin
                    })?;
                    if !crate::net::same_site_origin(&authority, &scope) {
                        return Err(AppError::api("unsafe_target", 400));
                    }
                }
            }
        }
        if let Some(response) = &op.response {
            if !["json", "html", "text", "xml", "empty", "file"].contains(&response.format.as_str())
                || response.required_pointers.len() > 30
                || response.strip_prefix.len() > 128
                || response.ssr_json.len() > 128
                || (!response.extract_pointer.is_empty()
                    && (!response.extract_pointer.starts_with('/')
                        || response.extract_pointer.len() > 512))
                || response
                    .html
                    .as_ref()
                    .is_some_and(|html| response.format != "html" || !html.valid())
                || response.required_html_fields.len() > 30
                || response.required_html_fields.iter().any(|field| {
                    field.selector.len() > 500
                        || field.attribute.len() > 80
                        || scraper::Selector::parse(&field.selector).is_err()
                })
                || response
                    .required_pointers
                    .iter()
                    .any(|p| !p.is_empty() && (!p.starts_with('/') || p.len() > 512))
            {
                return Err(AppError::api("data_invalid", 400));
            }
        }
        if op.preflight.len() > 4 {
            return Err(AppError::api("data_invalid", 400));
        }
        for step in &op.preflight {
            if !util::valid_id(&step.name)
                || !["", "GET", "HEAD"].contains(&step.method.as_str())
                || step.path.starts_with("//")
                || step.path.contains(['\\', '\r', '\n', '#'])
            {
                return Err(AppError::api("data_invalid", 400));
            }
            if !step.base_url.is_empty() {
                let step_dest = crate::net::validate_url(&step.base_url)?;
                if step_dest.query().is_some() || step_dest.fragment().is_some() {
                    return Err(AppError::api("bad_url", 400));
                }
            }
        }
        for (name, param) in op
            .params
            .iter()
            .chain(op.query.iter())
            .chain(op.body.iter().flat_map(|b| b.inputs()))
        {
            if util::secret_field(name)
                || param
                    .default
                    .as_ref()
                    .is_some_and(|v| util::redact_text(&util::plain(v)) != util::plain(v))
            {
                return Err(AppError::api("data_invalid", 400));
            }
        }
        if !util::valid_id(&op.name)
            || !names.insert(op.name.clone())
            || op.path.starts_with("//")
            || op.path.contains(['\\', '\r', '\n', '#', '?'])
            || !["GET", "HEAD", "OPTIONS", "POST", "PUT", "PATCH", "DELETE"]
                .contains(&op.method.as_str())
        {
            return Err(AppError::api("data_invalid", 400));
        }
        for (key, value) in sp.defaults.headers.iter().chain(op.headers.iter()) {
            let is_step_template = step_template_with_scheme(value);
            if (util::secret_field(key) && !is_step_template)
                || [
                    "host",
                    "content-length",
                    "transfer-encoding",
                    "connection",
                    "proxy-authorization",
                ]
                .contains(&key.to_ascii_lowercase().as_str())
                || key.contains(['\r', '\n'])
                || value.contains(['\r', '\n'])
            {
                return Err(AppError::api("data_invalid", 400));
            }
        }
    }
    let mut credential_origins = std::collections::BTreeMap::new();
    for auth in &sp.auth {
        if !auth.origin.is_empty() {
            let scope = crate::net::validate_url(&auth.origin)?;
            if scope.path() != "/" || scope.query().is_some() || scope.fragment().is_some() {
                return Err(AppError::api("bad_url", 400));
            }
        }
        if auth.kind == "header" && auth.oauth_refresh.is_none() {
            let scope = crate::net::validate_url(if auth.origin.is_empty() {
                &sp.site.base_url
            } else {
                &auth.origin
            })?
            .origin()
            .ascii_serialization();
            if credential_origins
                .insert(auth.value_from.clone(), scope.clone())
                .is_some_and(|previous| previous != scope)
            {
                return Err(AppError::api("unsafe_target", 400));
            }
        }
        if auth.header.contains(['\r', '\n'])
            || auth.prefix.contains(['\r', '\n'])
            || [
                "host",
                "content-length",
                "transfer-encoding",
                "connection",
                "proxy-authorization",
            ]
            .contains(&auth.header.to_ascii_lowercase().as_str())
        {
            return Err(AppError::api("data_invalid", 400));
        }
        if auth.kind == "header"
            && auth.oauth_refresh.is_none()
            && !auth
                .value_from
                .starts_with(&format!("store:{}/", sp.site.name))
        {
            return Err(AppError::api("data_invalid", 400));
        }
        if auth.kind == "derived"
            && (!crate::session_recipe::allowed_header(&auth.header)
                || !(auth.value_from.starts_with("cookie:")
                    || auth.value_from.starts_with("storage:")
                    || auth.value_from.starts_with("header:")))
        {
            return Err(AppError::api("data_invalid", 400));
        }
    }
    Ok(())
}

/// The same semantic check is used for observation URLs and every redirect.
pub fn safe_read_url(url: &url::Url) -> bool {
    if dangerous_intent(url.path())
        || url.query_pairs().any(|(key, value)| {
            util::secret_field(&key)
                || dangerous_intent(&key)
                || (INTENT_KEYS.contains(&key.to_ascii_lowercase().as_str())
                    && dangerous_intent(&value))
        })
    {
        return false;
    }
    // Preflight reads and redirects are separate requests with their own URL
    // semantics, not covered by the originating operation's effect check.
    let operation = Operation {
        name: "bounded-read".into(),
        method: "GET".into(),
        path: url.path().into(),
        effect: Effect::Read,
        evidence: "bounded GET request contract".into(),
        ..Default::default()
    };
    let inputs = url
        .query_pairs()
        .map(|(key, value)| (key.into_owned(), value.into_owned()))
        .collect();
    rendered_request_effect(&operation, &inputs, url.as_str(), None) == Effect::Read
}

/// Strict URL semantics for bounded manual session diagnostic GETs.
/// Opaque persisted GraphQL operations need a separate, evidenced execution contract.
pub(crate) fn safe_session_read_url(url: &url::Url) -> bool {
    if !safe_read_url(url) {
        return false;
    }
    let mut args = BTreeMap::new();
    for (key, value) in url.query_pairs() {
        if args.insert(key.into_owned(), value.into_owned()).is_some() {
            return false;
        }
    }
    let path = url.path().to_ascii_lowercase();
    let graphql = path.contains("graphql")
        || path.ends_with("/gql")
        || args.keys().any(|key| {
            matches!(
                key.to_ascii_lowercase().as_str(),
                "operationname" | "queryid" | "extensions"
            )
        });
    fn graphql_envelope(value: &serde_json::Value) -> bool {
        match value {
            serde_json::Value::Object(fields) => fields.iter().any(|(key, value)| {
                matches!(
                    key.to_ascii_lowercase().as_str(),
                    "query" | "operationname" | "queryid" | "extensions" | "persistedquery"
                ) || graphql_envelope(value)
            }),
            serde_json::Value::Array(values) => values.iter().any(graphql_envelope),
            _ => false,
        }
    }
    let visible_document = args
        .get("query")
        .is_some_and(|query| parse_graphql_bounded(query).is_some());
    if (graphql && !visible_document)
        || args.values().any(|value| {
            serde_json::from_str::<serde_json::Value>(value)
                .ok()
                .is_some_and(|value| graphql_envelope(&value))
        })
    {
        return false;
    }
    let operation = Operation {
        name: "session-diagnostic".into(),
        method: "GET".into(),
        path: url.path().into(),
        effect: Effect::Read,
        evidence: "explicit diagnostic GET".into(),
        ..Default::default()
    };
    operation_effect(&operation, &args) == Effect::Read
}

/// A manual session diagnostic cannot acquire authority over sibling origins.
pub(crate) fn safe_session_request(url: &url::Url, origin: &url::Url, method: &str) -> bool {
    url.origin() == origin.origin()
        && ["GET", "HEAD", "OPTIONS"].contains(&method)
        && safe_session_read_url(url)
}

/// A credential header value is acceptable only when it is one preflight step
/// template, optionally preceded by a single alphabetic auth scheme such as
/// `Bearer `. The scheme is public protocol text, never a secret.
fn step_template_with_scheme(value: &str) -> bool {
    let value = value.trim();
    let bare = |t: &str| t.starts_with("{{step.") && t.ends_with("}}");
    if bare(value) {
        return true;
    }
    value.split_once(' ').is_some_and(|(scheme, rest)| {
        !scheme.is_empty()
            && scheme.chars().all(|c| c.is_ascii_alphabetic())
            && bare(rest)
            && rest.matches("{{").count() == 1
    })
}

#[cfg(test)]
mod step_template_scheme_tests {
    use super::step_template_with_scheme;

    #[test]
    fn accepts_bare_or_scheme_prefixed_step_template() {
        assert!(step_template_with_scheme("{{step.s.accessToken}}"));
        assert!(step_template_with_scheme("Bearer {{step.s.accessToken}}"));
    }

    #[test]
    fn rejects_static_or_mixed_values() {
        assert!(!step_template_with_scheme("Bearer abc123"));
        assert!(!step_template_with_scheme("Bearer abc{{step.s.t}}"));
        assert!(!step_template_with_scheme("Bear3r {{step.s.t}}"));
        assert!(!step_template_with_scheme(
            "Bearer {{step.s.a}}x{{step.s.b}}"
        ));
        assert!(!step_template_with_scheme("Bearer {{input}}"));
    }
}

#[cfg(test)]
mod session_request_tests {
    #[test]
    fn diagnostics_reject_sibling_origins_redirect_targets_and_writes() {
        let origin = url::Url::parse("https://app.example.test").unwrap();
        for target in [
            "https://api.example.test/profile",
            "http://app.example.test/profile",
            "https://app.example.test:444/profile",
            "https://app.example.test/profile?action=delete",
        ] {
            assert!(!super::safe_session_request(
                &url::Url::parse(target).unwrap(),
                &origin,
                "GET"
            ));
        }
        assert!(!super::safe_session_request(&origin, &origin, "POST"));
        assert!(super::safe_session_request(
            &origin.join("profile").unwrap(),
            &origin,
            "GET"
        ));
    }
}

#[cfg(test)]
mod rendered_request_tests {
    use super::{Effect, operation_effect, rendered_request_effect};
    use serde_json::{Value, json};
    use std::collections::BTreeMap;

    // Pure serialization and policy tests: no runtime, credentials, or network.
    fn template_spec(body: Value) -> crate::spec::Spec {
        crate::spec::parse(
            json!({
                "spec_version": 2,
                "site": {"name": "fixture", "base_url": "https://fixture.example.test"},
                "operations": [{
                    "name": "read-items", "method": "POST", "path": "/api",
                    "effect": "read", "evidence": "Synthetic fixture contract",
                    "body": {"json_template": body}
                }]
            })
            .to_string()
            .as_bytes(),
        )
        .unwrap()
    }

    fn effect(spec: &crate::spec::Spec, args: &BTreeMap<String, String>) -> Effect {
        let op = &spec.operations[0];
        let (url, body) = spec.build_request(op, args).unwrap();
        rendered_request_effect(op, args, &url, body.as_deref())
    }

    #[test]
    fn rendered_preflight_and_redirect_urls_check_graphql_semantics() {
        let mut url = url::Url::parse("https://fixture.example.test/context").unwrap();
        url.query_pairs_mut()
            .append_pair("query", "mutation { sampleChange }");
        assert!(!super::safe_read_url(&url));
        url.set_query(None);
        url.query_pairs_mut().append_pair(
            "extensions",
            r#"{"persistedQuery":{"sha256Hash":"synthetic-reference"}}"#,
        );
        assert!(!super::safe_read_url(&url));
    }

    #[test]
    fn rendered_preflight_graphql_routes_preserve_visible_read_documents() {
        for path in ["/graphql", "/gql"] {
            let mut url = url::Url::parse(&format!("https://fixture.example.test{path}")).unwrap();
            url.query_pairs_mut()
                .append_pair("query", "query { items { id } }");
            assert!(super::safe_read_url(&url), "visible read at {path}");
            url.set_query(None);
            url.query_pairs_mut()
                .append_pair("query", "mutation { sampleChange }");
            assert!(!super::safe_read_url(&url));
            url.set_query(None);
            url.query_pairs_mut().append_pair(
                "extensions",
                r#"{"persistedQuery":{"sha256Hash":"synthetic-reference"}}"#,
            );
            assert!(!super::safe_read_url(&url));
        }
    }

    #[test]
    fn rendered_preflight_and_redirect_urls_preserve_ordinary_reads() {
        for path in [
            "/entry",
            "/context?format=json",
            "/items?tag=blue&tag=large",
        ] {
            let url = url::Url::parse(&format!("https://fixture.example.test{path}")).unwrap();
            assert!(super::safe_read_url(&url));
        }
        for (key, value) in [
            ("query", "query { items { id } }"),
            ("query", "winter coats"),
            ("search", "mutation { sampleChange }"),
        ] {
            let mut url = url::Url::parse("https://fixture.example.test/context").unwrap();
            url.query_pairs_mut().append_pair(key, value);
            assert!(super::safe_read_url(&url));
        }
    }

    #[test]
    fn rendered_request_override_headers_require_review() {
        for name in [
            "X-HTTP-Method-Override",
            "x-http-method",
            "X-Method-Override",
            "X-Original-URL",
            "x-Rewrite-uRL",
        ] {
            let mut spec = template_spec(json!({"search": "sample item"}));
            spec.operations[0]
                .headers
                .insert(name.into(), "synthetic-selector".into());
            assert_eq!(effect(&spec, &BTreeMap::new()), Effect::Unknown, "{name}");
        }
    }

    #[test]
    fn rendered_headers_check_merged_and_separate_read_requests() {
        let defaults = BTreeMap::from([("X-Rewrite-URL".to_owned(), "/other".to_owned())]);
        let operation = BTreeMap::from([("Accept".to_owned(), "application/json".to_owned())]);
        let merged: BTreeMap<_, _> = defaults.iter().chain(operation.iter()).collect();
        let effect = super::request_headers_effect(
            merged
                .into_iter()
                .map(|(name, value)| (name.as_str(), value.as_str())),
        );
        assert_eq!(effect, Effect::Unknown);
        assert_eq!(Effect::Write.combine(effect), Effect::Write);
        // Separate preflight reads use the identical helper, not the operation's headers.
        assert_eq!(
            super::request_headers_effect([("x-HTTP-method-OVERRIDE", "{{method}}")]),
            Effect::Unknown
        );
        assert_eq!(
            super::request_headers_effect([("X-HTTP-Method-Override", "GET")]),
            Effect::Unknown
        );
        assert_eq!(
            super::request_headers_effect([("Accept", "text/html")]),
            Effect::Read
        );
    }

    #[test]
    fn rendered_request_ordinary_headers_remain_read_only() {
        let mut spec = template_spec(json!({"search": "sample item"}));
        for name in ["Accept", "Content-Type", "X-Request-ID", "X-CSRF-Token"] {
            spec.operations[0]
                .headers
                .insert(name.into(), "mutation-shaped data".into());
        }
        assert_eq!(effect(&spec, &BTreeMap::new()), Effect::Read);
    }

    #[test]
    fn rendered_request_query_template_preserves_read_control() {
        let document = "query { items { id } }";
        let spec = template_spec(json!({"query": document}));
        let op = &spec.operations[0];
        let (url, body) = spec.build_request(op, &BTreeMap::new()).unwrap();
        assert_eq!(url, "https://fixture.example.test/api");
        let value: Value = serde_json::from_slice(body.as_ref().unwrap()).unwrap();
        assert_eq!(value, json!({"query": document}));
        assert_eq!(
            rendered_request_effect(op, &BTreeMap::new(), &url, body.as_deref()),
            Effect::Read
        );
    }

    #[test]
    fn rendered_request_mutation_template_requires_review() {
        let spec = template_spec(json!({"query": "mutation { sampleChange }"}));
        assert_eq!(effect(&spec, &BTreeMap::new()), Effect::Write);
    }

    #[test]
    fn rendered_request_nested_intent_template_requires_review() {
        let spec = template_spec(json!({"payload": {"operation": "update"}}));
        assert_eq!(effect(&spec, &BTreeMap::new()), Effect::Write);
    }

    #[test]
    fn rendered_request_opaque_graphql_envelope_requires_review() {
        for template in [
            json!({"extensions": {"persistedQuery": {"version": 1, "sha256Hash": "synthetic-reference"}}}),
            json!({"payload": {"queryId": "synthetic-reference"}}),
            json!({"operationName": "Items"}),
        ] {
            assert_eq!(
                effect(&template_spec(template), &BTreeMap::new()),
                Effect::Unknown
            );
        }
    }

    #[test]
    fn rendered_request_data_strings_and_ordinary_search_remain_read_only() {
        for template in [
            json!({"search": "mutation { sampleChange }"}),
            json!({"text": "{\"operation\":\"update\"}"}),
            json!({"query": "winter coats", "limit": 20}),
            json!({"filter": {"query": "blue shoes"}, "extensions": {"theme": "dark"}}),
            json!({"query": "query { item(search: \"mutation\") { id } }"}),
            json!({"query": "# mutation is only a comment\n{ items { id } }"}),
            json!({"query": "query Items($query: String) { items(search: $query) { id } }", "variables": {"query": "mutation { sampleChange }"}}),
            json!({"query": "query { items { id } }", "extensions": {"persistedQuery": {"version": 1, "sha256Hash": "synthetic-reference"}}}),
        ] {
            let spec = template_spec(template.clone());
            assert_eq!(effect(&spec, &BTreeMap::new()), Effect::Read, "{template}");
        }
    }

    #[test]
    fn rendered_request_nested_and_batched_documents_are_classified() {
        for template in [
            json!({"payload": {"query": "mutation { sampleChange }"}}),
            json!([{"query": "query { items { id } }"}, {"query": "mutation { sampleChange }"}]),
            json!({"query": "subscription { updates }"}),
        ] {
            assert_eq!(
                effect(&template_spec(template), &BTreeMap::new()),
                Effect::Write
            );
        }
        assert_eq!(
            effect(
                &template_spec(
                    json!([{"query": "query { items { id } }"}, {"query": "query { count }"}])
                ),
                &BTreeMap::new()
            ),
            Effect::Read
        );
        for document in [
            "query A { items { id } } query B { count }",
            "query { items(",
            "fragment Item on Item { id }",
        ] {
            assert_eq!(
                effect(&template_spec(json!({"query": document})), &BTreeMap::new()),
                Effect::Unknown
            );
        }
    }

    #[test]
    fn rendered_request_interpolated_documents_and_intents_are_rechecked() {
        let spec = template_spec(json!({"query": "{{document}}"}));
        for (document, expected) in [
            ("query { items { id } }", Effect::Read),
            ("mutation { sampleChange }", Effect::Write),
        ] {
            assert_eq!(
                effect(
                    &spec,
                    &BTreeMap::from([("document".into(), document.into())])
                ),
                expected
            );
        }
        let spec = template_spec(json!({"payload": {"mode": "{{step.context.mode}}"}}));
        assert_eq!(effect(&spec, &BTreeMap::new()), Effect::Read);
        assert_eq!(
            effect(
                &spec,
                &BTreeMap::from([("step.context.mode".into(), "update".into())])
            ),
            Effect::Write
        );
    }

    #[test]
    fn rendered_request_form_and_url_values_are_checked_after_interpolation() {
        let mut spec = template_spec(json!({}));
        spec.operations[0].body = Some(
            serde_json::from_value(
                json!({"form": {"query": {"type": "string", "default": "{{document}}"}}}),
            )
            .unwrap(),
        );
        let args = BTreeMap::from([("document".into(), "mutation { sampleChange }".into())]);
        assert_eq!(effect(&spec, &args), Effect::Write);
        spec.operations[0].method = "GET".into();
        spec.operations[0].query = spec.operations[0].body.take().unwrap().form;
        assert_eq!(effect(&spec, &args), Effect::Write);
        let args = BTreeMap::from([("document".into(), "query { items { id } }".into())]);
        // The declared unresolved query remains Unknown even when the final document is a read.
        assert_eq!(effect(&spec, &args), Effect::Unknown);
        let args = BTreeMap::from([("query".into(), "query { items { id } }".into())]);
        assert_eq!(effect(&spec, &args), Effect::Read);
        spec.operations[0].query = serde_json::from_value(json!({"extensions": {"type": "string", "default": "{\"persistedQuery\":{\"sha256Hash\":\"synthetic-reference\"}}"}})).unwrap();
        assert_eq!(effect(&spec, &BTreeMap::new()), Effect::Unknown);
    }

    #[test]
    fn rendered_request_path_and_declared_authority_cannot_be_downgraded() {
        let mut spec = template_spec(json!({"query": "query { items { id } }"}));
        spec.operations[0].path = "/{{route}}".into();
        assert_eq!(
            effect(&spec, &BTreeMap::from([("route".into(), "update".into())])),
            Effect::Write
        );
        spec.operations[0].path = "/api".into();
        spec.operations[0].effect = Effect::Write;
        assert_eq!(effect(&spec, &BTreeMap::new()), Effect::Write);
        spec.operations[0].effect = Effect::Unknown;
        assert_eq!(effect(&spec, &BTreeMap::new()), Effect::Unknown);
        spec.operations[0].effect = Effect::Read;
        spec.operations[0].evidence.clear();
        assert_eq!(effect(&spec, &BTreeMap::new()), Effect::Unknown);
    }

    #[test]
    fn rendered_request_ambiguous_or_invalid_wire_data_stays_unknown() {
        let spec = template_spec(json!({}));
        let op = &spec.operations[0];
        assert_eq!(
            rendered_request_effect(
                op,
                &BTreeMap::new(),
                "https://fixture.example.test/api?query=%7Bitems%7D&query=%7Bcount%7D",
                None
            ),
            Effect::Unknown
        );
        assert_eq!(
            rendered_request_effect(
                op,
                &BTreeMap::new(),
                "https://fixture.example.test/gql",
                None
            ),
            Effect::Unknown
        );
        assert_eq!(
            rendered_request_effect(
                op,
                &BTreeMap::new(),
                "https://fixture.example.test/api",
                Some(b"not JSON")
            ),
            Effect::Unknown
        );
        assert_eq!(operation_effect(op, &BTreeMap::new()), Effect::Read);
    }
}

#[cfg(test)]
mod bounded_graphql_tests {
    use super::*;

    #[test]
    fn shared_graphql_entry_bounds_read_classification_on_one_mib_stack() {
        std::thread::Builder::new()
            .name("shared-graphql-one-mib".into())
            .stack_size(1024 * 1024)
            .spawn(|| {
                for depth in [32, 33] {
                    let query = format!(
                        "query Q {{ {}id{} }}",
                        "child { ".repeat(depth - 1),
                        "}".repeat(depth - 1)
                    );
                    let expected = if depth == 32 {
                        Effect::Read
                    } else {
                        Effect::Unknown
                    };
                    assert_eq!(graphql_document_effect(&query, true), Some(expected));
                    assert_eq!(
                        graphql_document_effect(&format!("\u{feff}{query}"), false),
                        Some(expected)
                    );
                    assert_eq!(
                        payload_effect(&serde_json::json!({"filter":{"query":query}})).0,
                        expected
                    );
                    let mut url = url::Url::parse("https://example.test/graphql").unwrap();
                    url.query_pairs_mut().append_pair("query", &query);
                    assert_eq!(safe_read_url(&url), depth == 32);
                    assert_eq!(safe_session_read_url(&url), depth == 32);
                    let operation = Operation {
                        method: "GET".into(),
                        path: "/graphql".into(),
                        ..Default::default()
                    };
                    assert_eq!(
                        operation.graphql_query(&BTreeMap::from([("query".into(), query)])),
                        depth == 32
                    );
                }
                assert_eq!(graphql_document_effect("ordinary text", false), None);
                assert_eq!(graphql_document_effect("ordinary \"search", false), None);
                assert_eq!(
                    graphql_document_effect("query Q { id }", true),
                    Some(Effect::Read)
                );
            })
            .unwrap()
            .join()
            .unwrap();
    }
}

#[cfg(test)]
mod transfer_lease_tests {
    use super::*;

    #[tokio::test]
    async fn renewal_requires_the_current_unexpired_token_and_never_revives_it() {
        let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join(".cache/transfer-lease-tests")
            .join(util::id());
        let policy = Policy::with_limits(&root.join("policy.db"), 0, 100).unwrap();
        let scope = "https://fixture.test|account";
        let token = policy.acquire(scope).await.unwrap();
        policy.renew(scope, &token).unwrap();
        assert_eq!(
            policy.renew(scope, "wrong-token").unwrap_err().public_code,
            "request_lease_lost"
        );
        policy
            .conn
            .lock()
            .unwrap()
            .execute(
                "UPDATE request_gate SET lease_until=?2 WHERE scope=?1",
                params![scope, util::unix_ms() - 1],
            )
            .unwrap();
        assert_eq!(
            policy.renew(scope, &token).unwrap_err().public_code,
            "request_lease_lost"
        );
        let replacement = policy.acquire(scope).await.unwrap();
        assert_ne!(replacement, token);
        assert!(policy.renew(scope, &token).is_err());
        // Dropping the old owner must not release a replacement lease.
        policy.release(scope, &token, 0, None, false).unwrap();
        policy.renew(scope, &replacement).unwrap();
        policy.release(scope, &replacement, 0, None, false).unwrap();
        assert!(policy.renew(scope, &replacement).is_err());
        drop(policy);
        std::fs::remove_dir_all(root).unwrap();
    }
}
