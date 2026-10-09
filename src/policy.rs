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
        "new",
        "edit",
        "change",
        "toggle",
        "clear",
        "empty",
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
        "start",
        "stop",
        "restart",
        "set",
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
        if ["action", "operation", "op", "method", "command", "mode"]
            .contains(&key.to_ascii_lowercase().as_str())
            && dangerous_intent(value)
        {
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
            use graphql_parser::query::{Definition, OperationDefinition};
            match graphql_parser::parse_query::<&str>(query) {
                Ok(document) => {
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
                    if operations.iter().any(|operation| {
                        matches!(
                            operation,
                            OperationDefinition::Mutation(_) | OperationDefinition::Subscription(_)
                        )
                    }) {
                        return Effect::Write;
                    }
                    if operations.len() == 1 && op.effect == Effect::Read && !op.evidence.is_empty()
                    {
                        return Effect::Read;
                    }
                    return Effect::Unknown;
                }
                Err(_)
                    if op.path.to_ascii_lowercase().contains("graphql")
                        || query.trim_start().starts_with(['{', '#'])
                        || ["query", "mutation", "subscription", "fragment"]
                            .iter()
                            .any(|word| query.trim_start().starts_with(word)) =>
                {
                    return Effect::Unknown;
                }
                Err(_) => {} // A plain search string is not a GraphQL document.
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
            "search", "query", "lookup", "list", "find", "count", "preview", "resolve",
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

fn nested_change_intent(value: &serde_json::Value) -> bool {
    match value {
        serde_json::Value::Object(object) => object.iter().any(|(key, value)| {
            (["action", "operation", "op", "method", "command", "mode"]
                .contains(&key.to_ascii_lowercase().as_str())
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
        let conn = Connection::open(path)?;
        conn.busy_timeout(Duration::from_secs(5))?;
        conn.execute_batch("PRAGMA journal_mode=WAL;
            CREATE TABLE IF NOT EXISTS approvals(id TEXT PRIMARY KEY, digest TEXT NOT NULL, request TEXT NOT NULL, review TEXT NOT NULL, state TEXT NOT NULL, expires_ms INTEGER NOT NULL);
            CREATE TABLE IF NOT EXISTS request_gate(scope TEXT PRIMARY KEY, lease TEXT NOT NULL DEFAULT '', lease_until INTEGER NOT NULL DEFAULT 0, next_at INTEGER NOT NULL DEFAULT 0, cooldown INTEGER NOT NULL DEFAULT 0, paused TEXT NOT NULL DEFAULT '');
            CREATE TABLE IF NOT EXISTS request_event(scope TEXT NOT NULL, at INTEGER NOT NULL);
            CREATE INDEX IF NOT EXISTS request_event_scope_time ON request_event(scope,at);")?;
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
            "SELECT review FROM approvals WHERE state='pending' AND expires_ms>?1 ORDER BY rowid",
        )?;
        let rows = statement.query_map([util::unix_ms()], |r| r.get::<_, String>(0))?;
        let mut result = Vec::new();
        for row in rows {
            result.push(serde_json::from_str(&row?)?);
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
        review.request = Some(serde_json::from_str(&request)?);
        Ok(review)
    }
    /// Only the browser approval handler calls this after authenticating its session.
    /// Consumption happens before any network request, including failed writes.
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
    pub fn claim(&self, id: &str) -> AppResult<()> {
        let changed=self.conn.lock().map_err(|_|AppError::api("internal",500))?.execute("UPDATE approvals SET state='approved' WHERE id=?1 AND state='pending' AND expires_ms>?2",params![id,util::unix_ms()])?;
        if changed != 1 {
            return Err(AppError::api("approval_expired", 409));
        }
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
                let (lease_until, next_at, cooldown, paused): (i64, i64, i64, String) = tx.query_row("SELECT lease_until,next_at,cooldown,paused FROM request_gate WHERE scope=?1", [scope], |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?)))?;
                let now = util::unix_ms();
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
                    tx.execute(
                        "UPDATE request_gate SET lease=?2,lease_until=?3,next_at=?4 WHERE scope=?1",
                        params![scope, token, now + 120_000, now + self.interval_ms],
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
    pub fn release(
        &self,
        scope: &str,
        token: &str,
        status: u16,
        retry_after: Option<Duration>,
        challenge: bool,
    ) -> AppResult<()> {
        let conn = self
            .conn
            .lock()
            .map_err(|_| AppError::api("internal", 500))?;
        let pause = if status == 401 {
            "auth_required"
        } else if status == 403 || challenge {
            "blocked_by_site"
        } else {
            ""
        };
        let cooldown = if status == 429 || retry_after.is_some() {
            util::unix_ms()
                + retry_after
                    .unwrap_or(Duration::from_secs(60))
                    .as_millis()
                    .min(i64::MAX as u128) as i64
        } else {
            0
        };
        conn.execute("UPDATE request_gate SET lease='',lease_until=0,cooldown=MAX(cooldown,?3),paused=?4 WHERE scope=?1 AND lease=?2", params![scope,token,cooldown,pause])?;
        Ok(())
    }
    pub fn resume_auth(&self, scope: &str) -> AppResult<()> {
        self.conn
            .lock()
            .map_err(|_| AppError::api("internal", 500))?
            .execute(
                "UPDATE request_gate SET paused='' WHERE scope=?1 AND paused='auth_required'",
                [scope],
            )?;
        Ok(())
    }
    pub fn resume(&self, scope: &str) -> AppResult<()> {
        self.conn
            .lock()
            .map_err(|_| AppError::api("internal", 500))?
            .execute("UPDATE request_gate SET paused='' WHERE scope=?1", [scope])?;
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
        if !["", "http", "browser"].contains(&op.transport.as_str())
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
        let destination = crate::net::validate_url(sp.operation_base(op))?;
        if destination.query().is_some() || destination.fragment().is_some() {
            return Err(AppError::api("bad_url", 400));
        }
        if let Some(auth) = sp.find_auth(&op.auth) {
            if auth.kind == "header" {
                let scope = crate::net::validate_url(if auth.origin.is_empty() {
                    &sp.site.base_url
                } else {
                    &auth.origin
                })?;
                if destination.origin() != scope.origin() {
                    return Err(AppError::api("unsafe_target", 400));
                }
            } else if auth.kind == "browser"
                && destination.origin() != crate::net::validate_url(sp.site.source_url())?.origin()
            {
                return Err(AppError::api("unsafe_target", 400));
            }
        }
        if let Some(response) = &op.response {
            if !["json", "html", "text", "xml", "empty"].contains(&response.format.as_str())
                || response.required_pointers.len() > 30
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
            if util::secret_field(key)
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
        if auth.kind == "header" {
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
            && !auth
                .value_from
                .starts_with(&format!("store:{}/", sp.site.name))
        {
            return Err(AppError::api("data_invalid", 400));
        }
    }
    Ok(())
}

/// The same semantic check is used for observation URLs and every redirect.
pub fn safe_read_url(url: &url::Url) -> bool {
    !dangerous_intent(url.path())
        && !url.query_pairs().any(|(key, value)| {
            util::secret_field(&key)
                || dangerous_intent(&key)
                || (["action", "operation", "op", "command", "method", "mode"]
                    .contains(&key.to_ascii_lowercase().as_str())
                    && dangerous_intent(&value))
        })
}
