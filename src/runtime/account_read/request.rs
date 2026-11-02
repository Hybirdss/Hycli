use super::*;
use crate::policy::Effect;
use crate::spec::{Body, Operation};

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ReadRequest {
    pub method: String,
    pub url: String,
    #[serde(default)]
    pub json: Option<Value>,
    #[serde(default)]
    pub form: Option<BTreeMap<String, String>>,
    #[serde(default)]
    pub apollo_client_name: Option<String>,
}
#[derive(Clone)]
pub(super) struct Prepared {
    pub method: String,
    pub url: url::Url,
    pub graphql: bool,
    pub envelope: Option<Value>,
    pub persisted_id: Option<String>,
    pub semantic_document: Option<String>,
    pub query_cursor: Option<(String, String)>,
    form: bool,
    base_envelope: Option<Value>,
}
fn fields(url: &url::Url) -> AppResult<BTreeMap<String, String>> {
    let mut result = BTreeMap::new();
    for (key, value) in url.query_pairs() {
        if result
            .insert(key.into_owned(), value.into_owned())
            .is_some()
        {
            return Err(invalid());
        }
    }
    Ok(result)
}
fn decode_fields(fields: &BTreeMap<String, String>) -> AppResult<Value> {
    let mut result = serde_json::Map::new();
    for (key, value) in fields {
        let value = if ["variables", "extensions"].contains(&key.as_str()) {
            strict_json::parse(value.as_bytes())?
        } else {
            Value::String(value.clone())
        };
        result.insert(key.clone(), value);
    }
    Ok(Value::Object(result))
}
impl ReadRequest {
    pub fn prepare(&self) -> AppResult<Prepared> {
        if !["GET", "POST"].contains(&self.method.as_str())
            || self.json.is_some() && self.form.is_some()
        {
            return Err(unsupported());
        }
        let url = net::validate_url(&self.url)?;
        // Fragments are not request provenance; reject instead of silently normalizing them.
        if url::Url::parse(&self.url)
            .map_err(|_| invalid())?
            .fragment()
            .is_some()
        {
            return Err(invalid());
        }
        let query = fields(&url)?;
        if self.method == "GET" && (self.json.is_some() || self.form.is_some()) {
            return Err(invalid());
        }
        if self.method == "POST" && !query.is_empty() {
            return Err(unsupported());
        }
        let graphql = self.method == "POST"
            || url.path().to_ascii_lowercase().contains("graphql")
            || url.path().ends_with("/gql")
            || query.keys().any(|k| {
                ["operationName", "queryId", "doc_id", "extensions"].contains(&k.as_str())
            })
            || query.get("query").is_some_and(|q| {
                policy::parse_graphql_bounded(q).is_some()
                    || q.trim_start().starts_with(['{', '#'])
                    || ["query", "mutation", "subscription", "fragment"]
                        .iter()
                        .any(|prefix| q.trim_start().starts_with(prefix))
            });
        if let Some(name) = &self.apollo_client_name {
            if !graphql || name.is_empty() || name.len() > 128 || name.chars().any(char::is_control)
            {
                return Err(invalid());
            }
        }
        let envelope = if graphql {
            Some(if self.method == "GET" {
                decode_fields(&query)?
            } else if let Some(value) = &self.json {
                value.clone()
            } else if let Some(form) = &self.form {
                decode_fields(form)?
            } else {
                return Err(unsupported());
            })
        } else {
            None
        };
        let mut result = Prepared {
            method: self.method.clone(),
            url,
            graphql,
            base_envelope: envelope.clone(),
            envelope,
            persisted_id: None,
            semantic_document: None,
            query_cursor: None,
            form: self.form.is_some(),
        };
        if graphql {
            result.validate_envelope()?;
        }
        Ok(result)
    }
}
impl Prepared {
    fn validate_envelope(&mut self) -> AppResult<()> {
        let object = self
            .envelope
            .as_ref()
            .and_then(Value::as_object)
            .ok_or_else(unsupported)?;
        if object.keys().any(|key| {
            ![
                "query",
                "operationName",
                "variables",
                "extensions",
                "queryId",
                "doc_id",
                "id",
            ]
            .contains(&key.as_str())
        }) {
            return Err(unsupported());
        }
        if object
            .get("variables")
            .is_some_and(|v| !v.is_null() && !v.is_object())
        {
            return Err(invalid());
        }
        if object.get("query").is_some_and(|v| !v.is_string())
            || object
                .get("operationName")
                .is_some_and(|v| !v.is_string() && !v.is_null())
        {
            return Err(invalid());
        }
        let mut ids = Vec::new();
        for key in ["queryId", "doc_id", "id"] {
            if let Some(value) = object.get(key) {
                ids.push(
                    value
                        .as_str()
                        .filter(|s| !s.is_empty())
                        .ok_or_else(invalid)?
                        .to_string(),
                );
            }
        }
        if let Some(extensions) = object.get("extensions") {
            let extensions = extensions.as_object().ok_or_else(invalid)?;
            if extensions.len() != 1 {
                return Err(unsupported());
            }
            let persisted = extensions
                .get("persistedQuery")
                .and_then(Value::as_object)
                .ok_or_else(unsupported)?;
            if persisted.len() != 2 || persisted.get("version").and_then(Value::as_u64) != Some(1) {
                return Err(unsupported());
            }
            ids.push(
                persisted
                    .get("sha256Hash")
                    .and_then(Value::as_str)
                    .filter(|s| !s.is_empty())
                    .ok_or_else(invalid)?
                    .to_string(),
            );
        }
        if ids.len() > 1
            || ids
                .first()
                .is_some_and(|id| id.len() > 256 || id.chars().any(char::is_control))
        {
            return Err(unsupported());
        }
        self.persisted_id = ids.pop();
        if let Some(document) = object.get("query").and_then(Value::as_str) {
            validate_document(
                document,
                object.get("operationName").and_then(Value::as_str),
            )?;
            if let Some(id) = &self.persisted_id {
                if !object.contains_key("extensions") || *id != util::hash(document.as_bytes()) {
                    return Err(unsupported());
                }
                self.persisted_id = None;
            }
            self.semantic_document = Some(document.into());
        } else if self.persisted_id.is_none() {
            return Err(unsupported());
        }
        Ok(())
    }
    pub fn uses_apq_protocol(&self) -> bool {
        self.persisted_id.is_some()
            && self
                .envelope
                .as_ref()
                .is_some_and(|value| value.get("extensions").is_some())
    }
    pub fn persisted_selector(&self) -> AppResult<(String, Option<String>)> {
        // A manifest must not be used to relabel a full, contradictory document.
        if self.semantic_document.is_some() {
            return Err(unsupported());
        }
        Ok((
            self.persisted_id.clone().ok_or_else(unsupported)?,
            self.envelope
                .as_ref()
                .and_then(|v| v.get("operationName"))
                .and_then(Value::as_str)
                .map(str::to_string),
        ))
    }
    fn decoded_selectors(&self) -> AppResult<Value> {
        let query = fields(&self.url)?;
        let mut selectors = serde_json::Map::new();
        for (key, value) in query {
            let value = if value.trim_start().starts_with(['{', '[']) && key != "query" {
                strict_json::parse(value.as_bytes())?
            } else {
                Value::String(value)
            };
            selectors.insert(key, value);
        }
        Ok(Value::Object(selectors))
    }
    pub fn screen_normalized_secrets(&self, secrets: &[String]) -> AppResult<()> {
        // Form/GET protocol JSON has been decoded. Screen these canonical values
        // before any evidence fetch, while preserving exact original wire equality.
        if let Some(envelope) = &self.envelope {
            screen_value_secrets(envelope, secrets)?;
            screen_protocol_documents(envelope, secrets)?;
        }
        let selectors = self.decoded_selectors()?;
        screen_value_secrets(&selectors, secrets)?;
        screen_protocol_documents(&selectors, secrets)
    }
    pub fn validate_read(&self, documentless_name: Option<&str>) -> AppResult<()> {
        if policy::dangerous_intent(self.url.path())
            || self
                .url
                .query_pairs()
                .any(|(key, _)| util::secret_field(&key) || policy::dangerous_intent(&key))
        {
            return Err(unsupported());
        }
        if ambiguous_selectors(&self.decoded_selectors()?)
            || self.envelope.as_ref().is_some_and(ambiguous_selectors)
        {
            return Err(unsupported());
        }
        if let Some(name) = documentless_name {
            if self.persisted_id.is_none()
                || self.semantic_document.is_some()
                || policy::dangerous_intent(name)
            {
                return Err(unsupported());
            }
            let variables = self
                .envelope
                .as_ref()
                .and_then(|v| v.get("variables"))
                .unwrap_or(&Value::Null);
            if policy::nested_change_intent(variables) {
                return Err(unsupported());
            }
            return Ok(());
        }
        let mut semantic = self.envelope.clone();
        let mut args = BTreeMap::new();
        if let Some(document) = &self.semantic_document {
            validate_document(
                document,
                semantic
                    .as_ref()
                    .and_then(|v| v.get("operationName"))
                    .and_then(Value::as_str),
            )?;
            args.insert("query".into(), document.clone());
            let object = semantic
                .as_mut()
                .and_then(Value::as_object_mut)
                .ok_or_else(invalid)?;
            // These exact selectors have already been resolved by the independently sourced artifact.
            for key in ["extensions", "queryId", "doc_id", "id"] {
                object.remove(key);
            }
            object.insert("query".into(), Value::String(document.clone()));
        }
        let mut url = self.url.clone();
        let body = if self.graphql && self.method == "GET" {
            url.set_query(None);
            append_envelope(&mut url, semantic.as_ref().ok_or_else(invalid)?)?;
            None
        } else if let Some(value) = semantic {
            Some(serde_json::to_vec(&value)?)
        } else {
            None
        };
        if let Some((key, value)) = &self.query_cursor {
            // Only a private, already resolved continuation may use a sensitive token field.
            url.query_pairs_mut().append_pair(
                if util::secret_field(key) {
                    "cursor"
                } else {
                    key
                },
                value,
            );
        }
        let op = Operation {
            name: "account-read".into(),
            method: self.method.clone(),
            path: self.url.path().into(),
            effect: Effect::Read,
            evidence: "exact observed account request".into(),
            body: body.as_ref().map(|_| Body::default()),
            ..Default::default()
        };
        if policy::rendered_request_effect(&op, &args, url.as_str(), body.as_deref())
            != Effect::Read
        {
            return Err(unsupported());
        }
        Ok(())
    }
    pub fn ensure_cursor_absent(&self, pagination: &ReadPagination) -> AppResult<()> {
        if !pagination.query.is_empty() {
            if self
                .url
                .query_pairs()
                .any(|(key, _)| key == pagination.query)
            {
                return Err(invalid());
            }
        } else if self
            .envelope
            .as_ref()
            .and_then(|e| e.get("variables"))
            .and_then(|v| v.get(&pagination.variable))
            .is_some_and(|v| !v.is_null())
        {
            return Err(invalid());
        }
        Ok(())
    }
    pub fn apply_cursor(&mut self, pagination: &ReadPagination, value: &str) -> AppResult<()> {
        if !pagination.query.is_empty() {
            self.query_cursor = Some((pagination.query.clone(), value.into()));
        } else {
            let envelope = self
                .envelope
                .as_mut()
                .and_then(Value::as_object_mut)
                .ok_or_else(invalid)?;
            let variables = envelope.entry("variables").or_insert_with(|| json!({}));
            if variables.is_null() {
                *variables = json!({});
            }
            variables
                .as_object_mut()
                .ok_or_else(invalid)?
                .insert(pagination.variable.clone(), Value::String(value.into()));
            if self.method == "GET" {
                self.url.set_query(None);
                append_envelope(&mut self.url, self.envelope.as_ref().unwrap())?;
            }
        }
        Ok(())
    }
    pub fn clone_without_cursor(&self) -> Self {
        let mut result = self.clone();
        result.query_cursor = None;
        result.envelope = result.base_envelope.clone();
        if result.graphql && result.method == "GET" {
            result.url.set_query(None);
            let _ = append_envelope(&mut result.url, result.envelope.as_ref().unwrap());
        }
        result
    }
    pub fn wire_body(&self) -> AppResult<Option<Vec<u8>>> {
        if self.method == "GET" {
            return Ok(None);
        }
        let envelope = self.envelope.as_ref().ok_or_else(invalid)?;
        if self.form {
            let mut encoder = url::form_urlencoded::Serializer::new(String::new());
            for (key, value) in envelope.as_object().ok_or_else(invalid)? {
                encoder.append_pair(key, &field_text(key, value)?);
            }
            Ok(Some(encoder.finish().into_bytes()))
        } else {
            Ok(Some(serde_json::to_vec(envelope)?))
        }
    }
    pub fn headers(
        &self,
        credential: &Credential,
        client: Option<&str>,
    ) -> AppResult<Vec<(String, String)>> {
        let mut headers = credential.request_headers(self.url.as_str())?;
        // Do not allow captured method-override or transport framing headers to change the approved semantics.
        headers.retain(|(key, _)| {
            ![
                "content-type",
                "accept",
                "apollographql-client-name",
                "x-http-method-override",
                "x-method-override",
                "x-http-method",
                "x-original-url",
                "x-rewrite-url",
            ]
            .contains(&key.to_ascii_lowercase().as_str())
        });
        headers.push(("accept".into(), "application/json".into()));
        if self.method == "POST" {
            headers.push((
                "content-type".into(),
                if self.form {
                    "application/x-www-form-urlencoded"
                } else {
                    "application/json"
                }
                .into(),
            ));
        }
        if let Some(client) = client {
            headers.push(("apollographql-client-name".into(), client.into()));
        }
        if !headers.iter().any(|(name, _)| {
            name == "cookie"
                || crate::util::secret_field(name)
                || crate::accounts::browser_header(name)
                || credential.recipe.as_ref().is_some_and(|recipe| {
                    recipe
                        .headers
                        .iter()
                        .any(|rule| rule.header.eq_ignore_ascii_case(name))
                })
        }) {
            return Err(AppError::api("auth_required", 401));
        }
        Ok(headers)
    }
}
fn field_text(key: &str, value: &Value) -> AppResult<String> {
    if ["variables", "extensions"].contains(&key) {
        Ok(serde_json::to_string(value)?)
    } else {
        value.as_str().map(str::to_string).ok_or_else(invalid)
    }
}
fn append_envelope(url: &mut url::Url, value: &Value) -> AppResult<()> {
    for (key, value) in value.as_object().ok_or_else(invalid)? {
        url.query_pairs_mut()
            .append_pair(key, &field_text(key, value)?);
    }
    Ok(())
}
fn validate_document(document: &str, name: Option<&str>) -> AppResult<()> {
    use graphql_parser::query::{Definition, OperationDefinition};
    let document = policy::parse_graphql_bounded(document).ok_or_else(unsupported)?;
    let operations: Vec<_> = document
        .definitions
        .iter()
        .filter_map(|d| {
            if let Definition::Operation(op) = d {
                Some(op)
            } else {
                None
            }
        })
        .collect();
    if operations.len() != 1 {
        return Err(unsupported());
    }
    let actual = match operations[0] {
        OperationDefinition::Query(q) => q.name,
        OperationDefinition::SelectionSet(_) => None,
        _ => return Err(unsupported()),
    };
    if name.is_some() && name != actual || actual.is_some_and(policy::dangerous_intent) {
        return Err(unsupported());
    }
    Ok(())
}

fn ambiguous_selectors(value: &Value) -> bool {
    match value {
        Value::Object(fields) => fields.iter().any(|(key, value)| {
            if policy::INTENT_KEYS.contains(&key.to_ascii_lowercase().as_str()) {
                let Some(selector) = value.as_str() else {
                    return true;
                };
                let words = regex::Regex::new(r"([a-z0-9])([A-Z])")
                    .expect("constant")
                    .replace_all(selector, "$1-$2")
                    .to_ascii_lowercase();
                let first = words
                    .split(|c: char| !c.is_ascii_alphanumeric())
                    .next()
                    .unwrap_or("");
                if ![
                    "get", "read", "list", "search", "query", "lookup", "find", "fetch", "count",
                    "filter", "inspect", "check", "preview",
                ]
                .contains(&first)
                {
                    return true;
                }
            }
            ambiguous_selectors(value)
        }),
        Value::Array(values) => values.iter().any(ambiguous_selectors),
        _ => false,
    }
}

/// Decode only specified GraphQL literal syntax in protocol document fields.
/// Comments and GraphQL variables are data, not another executable envelope.
fn screen_protocol_documents(value: &Value, secrets: &[String]) -> AppResult<()> {
    match value {
        Value::Object(fields) => {
            let document = fields
                .get("query")
                .and_then(Value::as_str)
                .and_then(policy::parse_graphql_bounded);
            if let Some(document) = &document {
                screen_graphql_literals(document, secrets)?;
            }
            for (key, value) in fields {
                if document.is_some() && matches!(key.as_str(), "variables" | "extensions") {
                    continue;
                }
                screen_protocol_documents(value, secrets)?;
            }
        }
        Value::Array(values) => {
            for value in values {
                screen_protocol_documents(value, secrets)?;
            }
        }
        _ => {}
    }
    Ok(())
}
fn screen_graphql_literals<'a>(
    document: &graphql_parser::query::Document<'a, &'a str>,
    secrets: &[String],
) -> AppResult<()> {
    use graphql_parser::query as gql;
    fn literal<'a>(value: &gql::Value<'a, &'a str>) -> AppResult<Value> {
        Ok(match value {
            gql::Value::Variable(_) | gql::Value::Null => Value::Null,
            gql::Value::Int(value) => json!(value.as_i64().ok_or_else(invalid)?),
            gql::Value::Float(value) => {
                Value::Number(serde_json::Number::from_f64(*value).ok_or_else(invalid)?)
            }
            gql::Value::String(value) => Value::String(value.clone()),
            gql::Value::Boolean(value) => Value::Bool(*value),
            gql::Value::Enum(value) => Value::String((*value).into()),
            gql::Value::List(values) => {
                Value::Array(values.iter().map(literal).collect::<AppResult<_>>()?)
            }
            gql::Value::Object(fields) => Value::Object(
                fields
                    .iter()
                    .map(|(key, value)| Ok(((*key).to_string(), literal(value)?)))
                    .collect::<AppResult<_>>()?,
            ),
        })
    }
    fn arguments<'a>(
        args: &[(&'a str, gql::Value<'a, &'a str>)],
        secrets: &[String],
    ) -> AppResult<()> {
        for (_, value) in args {
            screen_value_secrets(&literal(value)?, secrets)?;
        }
        Ok(())
    }
    fn directives<'a>(values: &[gql::Directive<'a, &'a str>], secrets: &[String]) -> AppResult<()> {
        for value in values {
            arguments(&value.arguments, secrets)?;
        }
        Ok(())
    }
    fn selection<'a>(set: &gql::SelectionSet<'a, &'a str>, secrets: &[String]) -> AppResult<()> {
        for item in &set.items {
            match item {
                gql::Selection::Field(field) => {
                    arguments(&field.arguments, secrets)?;
                    directives(&field.directives, secrets)?;
                    selection(&field.selection_set, secrets)?;
                }
                gql::Selection::FragmentSpread(fragment) => {
                    directives(&fragment.directives, secrets)?
                }
                gql::Selection::InlineFragment(fragment) => {
                    directives(&fragment.directives, secrets)?;
                    selection(&fragment.selection_set, secrets)?;
                }
            }
        }
        Ok(())
    }
    for definition in &document.definitions {
        match definition {
            gql::Definition::Fragment(fragment) => {
                directives(&fragment.directives, secrets)?;
                selection(&fragment.selection_set, secrets)?;
            }
            gql::Definition::Operation(operation) => {
                let (variables, hints, set) = match operation {
                    gql::OperationDefinition::SelectionSet(set) => (&[][..], &[][..], set),
                    gql::OperationDefinition::Query(query) => (
                        &query.variable_definitions[..],
                        &query.directives[..],
                        &query.selection_set,
                    ),
                    gql::OperationDefinition::Mutation(query) => (
                        &query.variable_definitions[..],
                        &query.directives[..],
                        &query.selection_set,
                    ),
                    gql::OperationDefinition::Subscription(query) => (
                        &query.variable_definitions[..],
                        &query.directives[..],
                        &query.selection_set,
                    ),
                };
                for variable in variables {
                    if let Some(value) = &variable.default_value {
                        screen_value_secrets(&literal(value)?, secrets)?;
                    }
                }
                directives(hints, secrets)?;
                selection(set, secrets)?;
            }
        }
    }
    Ok(())
}
