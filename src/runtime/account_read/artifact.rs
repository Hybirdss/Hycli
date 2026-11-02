//! Offline persisted-operation evidence. These are format parsers, never a JS
//! runtime or an inference from operation names. Supported source contracts:
//! https://www.apollographql.com/docs/graphos/platform/security/persisted-queries
//! https://relay.dev/docs/guides/persisted-queries/
//! https://github.com/facebook/relay/blob/main/packages/relay-runtime/util/RelayConcreteNode.js
use std::collections::{BTreeMap, BTreeSet};

use graphql_parser::query::{Definition, OperationDefinition};
use serde_json::Value;

use super::strict_json;
use crate::apperr::{AppError, AppResult};

#[derive(Debug)]
pub(super) struct VerifiedOperation {
    pub document: Option<String>,
    pub name: String,
}

fn unsupported() -> AppError {
    AppError::api("unsupported_evidence", 422)
}

pub(super) fn parse(
    format: &str,
    bytes: &[u8],
    id: &str,
    operation_name: Option<&str>,
    client_name: Option<&str>,
    apq: bool,
) -> AppResult<VerifiedOperation> {
    if bytes.len() > 1024 * 1024 || id.is_empty() {
        return Err(unsupported());
    }
    match format {
        "apollo" | "apollo-persisted-query-manifest" => {
            apollo(bytes, id, operation_name, client_name, apq)
        }
        "relay" | "relay-json" => relay_map(bytes, id, operation_name, apq),
        "relay-artifact" | "relay-concrete-request" => {
            relay_artifact(bytes, id, operation_name, apq)
        }
        _ => Err(unsupported()),
    }
}

fn apollo(
    bytes: &[u8],
    id: &str,
    operation_name: Option<&str>,
    client_name: Option<&str>,
    apq: bool,
) -> AppResult<VerifiedOperation> {
    let value = strict_json::parse(bytes)?;
    let root = value.as_object().ok_or_else(unsupported)?;
    if root.get("format").and_then(Value::as_str) != Some("apollo-persisted-query-manifest")
        || root.get("version").and_then(Value::as_u64) != Some(1)
    {
        return Err(unsupported());
    }
    let entries = root
        .get("operations")
        .and_then(Value::as_array)
        .ok_or_else(unsupported)?;
    let mut keys = BTreeSet::new();
    let mut selected = None;
    for entry in entries {
        let object = entry.as_object().ok_or_else(unsupported)?;
        let entry_id = object
            .get("id")
            .and_then(Value::as_str)
            .ok_or_else(unsupported)?;
        let client = match object.get("clientName") {
            None | Some(Value::Null) => None,
            Some(Value::String(value)) => Some(value.as_str()),
            _ => return Err(unsupported()),
        };
        // Reject even identical duplicates: a manifest must map each key once.
        if entry_id.is_empty() || !keys.insert((entry_id, client)) {
            return Err(unsupported());
        }
        if entry_id == id && client == client_name {
            selected = Some(object);
        }
    }
    let operation = selected.ok_or_else(unsupported)?;
    if operation.get("type").and_then(Value::as_str) != Some("query") {
        return Err(unsupported());
    }
    let name = operation
        .get("name")
        .and_then(Value::as_str)
        .ok_or_else(unsupported)?;
    if operation_name.is_some_and(|requested| requested != name) {
        return Err(unsupported());
    }
    let body = operation
        .get("body")
        .and_then(Value::as_str)
        .ok_or_else(unsupported)?;
    verify_document(body, id, Some(name), apq)
}

fn relay_map(
    bytes: &[u8],
    id: &str,
    name: Option<&str>,
    apq: bool,
) -> AppResult<VerifiedOperation> {
    let value = strict_json::parse(bytes)?;
    let mapping = value.as_object().ok_or_else(unsupported)?;
    // This format is specifically ID -> full GraphQL document, not arbitrary
    // JSON containing query-looking strings or a reversed body -> ID map.
    if mapping.values().any(|value| !value.is_string()) {
        return Err(unsupported());
    }
    let body = mapping
        .get(id)
        .and_then(Value::as_str)
        .ok_or_else(unsupported)?;
    verify_document(body, id, name, apq)
}

fn verify_document(
    body: &str,
    id: &str,
    name: Option<&str>,
    apq: bool,
) -> AppResult<VerifiedOperation> {
    check_graphql_depth(body)?;
    let document = crate::policy::parse_graphql_bounded(body).ok_or_else(unsupported)?;
    let mut operation = None;
    let mut fragments = BTreeSet::new();
    for definition in document.definitions {
        match definition {
            Definition::Operation(OperationDefinition::Query(query)) if operation.is_none() => {
                operation = Some(query.name.ok_or_else(unsupported)?);
            }
            Definition::Fragment(fragment) if fragments.insert(fragment.name) => {}
            _ => return Err(unsupported()),
        }
    }
    let found_name = operation.ok_or_else(unsupported)?;
    if name.is_some_and(|requested| requested != found_name)
        || (apq && crate::util::hash(body.as_bytes()) != id)
    {
        return Err(unsupported());
    }
    Ok(VerifiedOperation {
        document: Some(body.to_owned()),
        name: found_name.to_owned(),
    })
}

pub(super) fn check_graphql_depth(body: &str) -> AppResult<()> {
    if crate::policy::graphql_depth_is_bounded(body) {
        Ok(())
    } else {
        Err(unsupported())
    }
}

fn relay_artifact(
    bytes: &[u8],
    id: &str,
    name: Option<&str>,
    apq: bool,
) -> AppResult<VerifiedOperation> {
    let source = std::str::from_utf8(bytes).map_err(|_| unsupported())?;
    let tokens = tokenize(source)?;
    let root = JsParser {
        tokens,
        offset: 0,
        bindings: BTreeSet::new(),
    }
    .module()?;
    let root = root.object().ok_or_else(unsupported)?;
    if root.get("kind").and_then(JsValue::string) != Some("Request") {
        return Err(unsupported());
    }
    let params = if let Some(params) = root.get("params") {
        // Mixing old top-level selectors and modern params is ambiguous.
        if ["id", "name", "operationKind", "text"]
            .iter()
            .any(|key| root.contains_key(*key))
        {
            return Err(unsupported());
        }
        params.object().ok_or_else(unsupported)?
    } else {
        root
    };
    let found_name = params
        .get("name")
        .and_then(JsValue::string)
        .ok_or_else(unsupported)?;
    if params.get("id").and_then(JsValue::string) != Some(id)
        || params.get("operationKind").and_then(JsValue::string) != Some("query")
        || !graphql_name(found_name)
        || name.is_some_and(|requested| requested != found_name)
    {
        return Err(unsupported());
    }
    match params.get("text") {
        Some(JsValue::Literal(Value::Null)) if !apq => Ok(VerifiedOperation {
            document: None,
            name: found_name.to_owned(),
        }),
        Some(JsValue::Literal(Value::String(body))) => {
            verify_document(body, id, Some(found_name), apq)
        }
        _ => Err(unsupported()),
    }
}

fn graphql_name(name: &str) -> bool {
    let mut bytes = name.bytes();
    bytes
        .next()
        .is_some_and(|byte| byte.is_ascii_alphabetic() || byte == b'_')
        && bytes.all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
}

#[derive(Debug, PartialEq)]
enum Token {
    Word(String),
    String(String),
    Number(String),
    Punct(char),
}

/// Tokenize comments and strings before recognizing any fields. Backticks,
/// regex literals, non-ASCII identifiers and executable expressions are not
/// part of the supported generated-artifact subset.
fn tokenize(source: &str) -> AppResult<Vec<Token>> {
    let bytes = source.as_bytes();
    let mut result = Vec::new();
    let mut at = 0;
    while at < bytes.len() {
        let byte = bytes[at];
        if let Some(width) = js_line_terminator(&bytes[at..]) {
            at += width;
            continue;
        }
        if byte.is_ascii_whitespace() {
            at += 1;
            continue;
        }
        if bytes[at..].starts_with(b"//") {
            while at < bytes.len() && js_line_terminator(&bytes[at..]).is_none() {
                at += 1;
            }
            continue;
        }
        if bytes[at..].starts_with(b"/*") {
            at += 2;
            let start = at;
            while at + 1 < bytes.len() && !bytes[at..].starts_with(b"*/") {
                at += 1;
            }
            if at + 1 >= bytes.len() || at < start {
                return Err(unsupported());
            }
            at += 2;
            continue;
        }
        if byte == b'"' || byte == b'\'' {
            let start = at;
            at += 1;
            loop {
                if at >= bytes.len() {
                    return Err(unsupported());
                }
                if bytes[at] == byte {
                    at += 1;
                    break;
                }
                if bytes[at] == b'\\' {
                    at += 1;
                }
                at += 1;
            }
            let raw = &source[start..at];
            let decoded = if byte == b'"' {
                serde_json::from_str::<String>(raw).map_err(|_| unsupported())?
            } else {
                let mut json = String::from("\"");
                let mut chars = raw[1..raw.len() - 1].chars();
                while let Some(ch) = chars.next() {
                    match ch {
                        '\\' => {
                            let escaped = chars.next().ok_or_else(unsupported)?;
                            if escaped == '\'' {
                                json.push('\'');
                            } else {
                                json.push('\\');
                                json.push(escaped);
                            }
                        }
                        '"' => json.push_str("\\\""),
                        _ => json.push(ch),
                    }
                }
                json.push('"');
                serde_json::from_str::<String>(&json).map_err(|_| unsupported())?
            };
            result.push(Token::String(decoded));
        } else if byte.is_ascii_alphabetic() || byte == b'_' || byte == b'$' {
            let start = at;
            at += 1;
            while at < bytes.len()
                && (bytes[at].is_ascii_alphanumeric() || matches!(bytes[at], b'_' | b'$'))
            {
                at += 1;
            }
            result.push(Token::Word(source[start..at].to_owned()));
        } else if byte.is_ascii_digit() || byte == b'-' {
            let start = at;
            at += 1;
            while at < bytes.len()
                && (bytes[at].is_ascii_digit()
                    || matches!(bytes[at], b'.' | b'e' | b'E' | b'+' | b'-'))
            {
                at += 1;
            }
            result.push(Token::Number(source[start..at].to_owned()));
        } else if byte.is_ascii_punctuation() && !matches!(byte, b'`' | b'\\' | b'/') {
            result.push(Token::Punct(char::from(byte)));
            at += 1;
        } else {
            return Err(unsupported());
        }
    }
    Ok(result)
}

// ECMAScript line comments end at LF, CR, LINE SEPARATOR, or PARAGRAPH
// SEPARATOR. Skipping only LF would conceal executable selector overrides.
fn js_line_terminator(bytes: &[u8]) -> Option<usize> {
    if matches!(bytes.first(), Some(b'\n' | b'\r')) {
        Some(1)
    } else if bytes.starts_with(b"\xe2\x80\xa8") || bytes.starts_with(b"\xe2\x80\xa9") {
        Some(3)
    } else {
        None
    }
}

#[derive(Debug)]
enum JsValue {
    Literal(Value),
    Object(BTreeMap<String, JsValue>),
    // Array contents are parsed and checked but never selectors.
    Array,
    Reference,
}
impl JsValue {
    fn object(&self) -> Option<&BTreeMap<String, JsValue>> {
        match self {
            Self::Object(value) => Some(value),
            _ => None,
        }
    }
    fn string(&self) -> Option<&str> {
        match self {
            Self::Literal(Value::String(value)) => Some(value),
            _ => None,
        }
    }
}

struct JsParser {
    tokens: Vec<Token>,
    offset: usize,
    bindings: BTreeSet<String>,
}

impl JsParser {
    fn punct(&mut self, value: char) -> bool {
        if self.tokens.get(self.offset) == Some(&Token::Punct(value)) {
            self.offset += 1;
            true
        } else {
            false
        }
    }
    fn word(&mut self, value: &str) -> bool {
        if matches!(self.tokens.get(self.offset), Some(Token::Word(found)) if found == value) {
            self.offset += 1;
            true
        } else {
            false
        }
    }
    fn require_punct(&mut self, value: char) -> AppResult<()> {
        if self.punct(value) {
            Ok(())
        } else {
            Err(unsupported())
        }
    }
    fn require_word(&mut self, value: &str) -> AppResult<()> {
        if self.word(value) {
            Ok(())
        } else {
            Err(unsupported())
        }
    }
    fn identifier(&mut self) -> AppResult<String> {
        match self.tokens.get(self.offset) {
            Some(Token::Word(value)) => {
                let value = value.clone();
                self.offset += 1;
                Ok(value)
            }
            _ => Err(unsupported()),
        }
    }
    fn string(&mut self) -> AppResult<String> {
        match self.tokens.get(self.offset) {
            Some(Token::String(value)) => {
                let value = value.clone();
                self.offset += 1;
                Ok(value)
            }
            _ => Err(unsupported()),
        }
    }

    fn module(mut self) -> AppResult<JsValue> {
        if self.tokens.first() == Some(&Token::String("use strict".into())) {
            self.offset += 1;
            self.require_punct(';')?;
        }
        // Flow/TypeScript type-only declarations cannot alter the exported
        // value. Nothing else is skipped as arbitrary surrounding JavaScript.
        loop {
            let start = self.offset;
            if self.word("import") {
                self.require_word("type")?;
                self.type_import()?;
            } else if self.word("export") {
                if self.word("type") {
                    self.type_declaration()?;
                } else {
                    self.offset = start;
                    break;
                }
            } else if self.word("type") {
                self.type_declaration()?;
            } else {
                break;
            }
        }
        let root;
        if self.punct('{') {
            self.offset -= 1;
            root = self.value(0)?;
            self.punct(';');
        } else if self.word("module") {
            self.require_punct('.')?;
            self.require_word("exports")?;
            self.require_punct('=')?;
            root = self.value(0)?;
            self.punct(';');
        } else if self.word("export") {
            self.require_word("default")?;
            root = self.value(0)?;
            self.punct(';');
        } else {
            if !(self.word("const") || self.word("var") || self.word("let")) {
                return Err(unsupported());
            }
            let binding = self.identifier()?;
            if self.punct(':') {
                self.require_word("ConcreteRequest")?;
            }
            self.require_punct('=')?;
            if self.punct('(') {
                self.require_word("function")?;
                self.require_punct('(')?;
                self.require_punct(')')?;
                self.require_punct('{')?;
                while self.word("var") || self.word("const") || self.word("let") {
                    loop {
                        let helper = self.identifier()?;
                        if helper == binding || self.bindings.contains(&helper) {
                            return Err(unsupported());
                        }
                        self.require_punct('=')?;
                        self.value(0)?;
                        self.bindings.insert(helper);
                        if !self.punct(',') {
                            break;
                        }
                    }
                    self.require_punct(';')?;
                }
                self.require_word("return")?;
                root = self.value(0)?;
                self.punct(';');
                self.require_punct('}')?;
                self.require_punct(')')?;
                self.require_punct('(')?;
                self.require_punct(')')?;
            } else {
                root = self.value(0)?;
            }
            self.punct(';');
            self.footer(&binding)?;
        }
        if self.offset != self.tokens.len() {
            return Err(unsupported());
        }
        Ok(root)
    }

    fn type_import(&mut self) -> AppResult<()> {
        self.require_punct('{')?;
        self.require_word("ConcreteRequest")?;
        self.punct(',');
        self.require_punct('}')?;
        self.require_word("from")?;
        if self.string()? != "relay-runtime" {
            return Err(unsupported());
        }
        self.require_punct(';')
    }

    fn type_declaration(&mut self) -> AppResult<()> {
        // Require an actual type alias, not an expression beginning with the
        // contextual identifier `type` (such as `type = mutate();`).
        self.identifier()?;
        self.require_punct('=')?;
        let mut stack = Vec::new();
        while let Some(token) = self.tokens.get(self.offset) {
            self.offset += 1;
            if let Token::Punct(ch) = token {
                match ch {
                    ';' if stack.is_empty() => return Ok(()),
                    '{' | '[' | '(' => {
                        stack.push(*ch);
                        if stack.len() > 64 {
                            return Err(unsupported());
                        }
                    }
                    '}' | ']' | ')' => {
                        let expected = match ch {
                            '}' => '{',
                            ']' => '[',
                            _ => '(',
                        };
                        if stack.pop() != Some(expected) {
                            return Err(unsupported());
                        }
                    }
                    _ => {}
                }
            }
        }
        Err(unsupported())
    }

    fn footer(&mut self, binding: &str) -> AppResult<()> {
        // Relay adds a non-selector debug hash. No mutation of params, aliases,
        // computed properties, Object.assign, calls or conditional exports.
        let start = self.offset;
        let parenthesized = self.punct('(');
        if self.word(binding) {
            if parenthesized {
                if self.word("as") {
                    self.require_word("any")?;
                }
                self.require_punct(')')?;
            }
            self.require_punct('.')?;
            self.require_word("hash")?;
            self.require_punct('=')?;
            self.string()?;
            self.require_punct(';')?;
        } else {
            self.offset = start;
        }
        if self.word("module") {
            self.require_punct('.')?;
            self.require_word("exports")?;
            self.require_punct('=')?;
            self.require_word(binding)?;
            self.punct(';');
        } else if self.word("export") {
            self.require_word("default")?;
            self.require_word(binding)?;
            self.punct(';');
        }
        Ok(())
    }

    fn value(&mut self, depth: usize) -> AppResult<JsValue> {
        if depth > 64 {
            return Err(unsupported());
        }
        if self.punct('{') {
            let mut fields = BTreeMap::new();
            if !self.punct('}') {
                loop {
                    let key = match self.tokens.get(self.offset) {
                        Some(Token::Word(key) | Token::String(key)) => key.clone(),
                        _ => return Err(unsupported()),
                    };
                    self.offset += 1;
                    self.require_punct(':')?;
                    let value = self.value(depth + 1)?;
                    if fields.insert(key, value).is_some() {
                        return Err(unsupported());
                    }
                    if self.punct('}') {
                        break;
                    }
                    self.require_punct(',')?;
                    if self.punct('}') {
                        break;
                    }
                }
            }
            return Ok(JsValue::Object(fields));
        }
        if self.punct('[') {
            if !self.punct(']') {
                loop {
                    self.value(depth + 1)?;
                    if self.punct(']') {
                        break;
                    }
                    self.require_punct(',')?;
                    if self.punct(']') {
                        break;
                    }
                }
            }
            return Ok(JsValue::Array);
        }
        if self.punct('(') {
            let value = self.value(depth + 1)?;
            self.require_punct(')')?;
            return Ok(value);
        }
        let value = match self.tokens.get(self.offset) {
            Some(Token::String(value)) => JsValue::Literal(Value::String(value.clone())),
            Some(Token::Number(value)) => {
                let number: Value = serde_json::from_str(value).map_err(|_| unsupported())?;
                if !number.is_number() {
                    return Err(unsupported());
                }
                JsValue::Literal(number)
            }
            Some(Token::Word(value)) if value == "null" => JsValue::Literal(Value::Null),
            Some(Token::Word(value)) if value == "true" => JsValue::Literal(Value::Bool(true)),
            Some(Token::Word(value)) if value == "false" => JsValue::Literal(Value::Bool(false)),
            Some(Token::Word(value)) if self.bindings.contains(value) => JsValue::Reference,
            _ => return Err(unsupported()),
        };
        self.offset += 1;
        Ok(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn manifest(operations: Value) -> Vec<u8> {
        serde_json::to_vec(&json!({
            "format": "apollo-persisted-query-manifest", "version": 1,
            "operations": operations,
        }))
        .unwrap()
    }

    fn entry(id: &str, body: &str, name: &str, kind: &str) -> Value {
        json!({"id":id,"body":body,"name":name,"type":kind})
    }

    fn relay(source: &str) -> AppResult<VerifiedOperation> {
        parse(
            "relay-artifact",
            source.as_bytes(),
            "registered-42",
            Some("ViewerQuery"),
            None,
            false,
        )
    }

    const MODERN: &str = r#"
        /** @generated SignedSource<<fixture>> */
        'use strict';
        /*:: import type { ConcreteRequest } from 'relay-runtime'; */
        const node/*: ConcreteRequest*/ = (function(){
            var v0 = [{kind: 'LocalArgument', name: 'id', defaultValue: null}],
                v1 = {alias: null, args: null, kind: 'ScalarField', name: 'id', storageKey: null};
            return {
                fragment: {argumentDefinitions: v0, kind: 'Fragment', name: 'ViewerQuery',
                           selections: [v1], type: 'Query', abstractKey: null},
                kind: 'Request',
                operation: {argumentDefinitions: (v0/*: any*/), kind: 'Operation',
                            name: 'ViewerQuery', selections: [(v1/*: any*/)]},
                params: {cacheID: 'not-a-persisted-id', id: 'registered-42', metadata: {},
                         name: 'ViewerQuery', operationKind: 'query', text: null},
            };
        })();
        (node/*: any*/).hash = 'debug-only';
        module.exports = node;
    "#;

    #[test]
    fn apollo_registered_id_is_not_assumed_to_be_a_hash() {
        let bytes = manifest(json!([entry(
            "registered-42",
            "query ViewerQuery { viewer { id } }",
            "ViewerQuery",
            "query"
        )]));
        let operation = parse("apollo", &bytes, "registered-42", None, None, false).unwrap();
        assert_eq!(operation.name, "ViewerQuery");
        assert!(operation.document.unwrap().contains("viewer"));
        assert!(parse("apollo", &bytes, "registered-42", None, None, true).is_err());
    }

    #[test]
    fn apollo_apq_verifies_exact_unmodified_document_bytes() {
        let body = "query ViewerQuery { viewer { id } }\n";
        let hash = crate::util::hash(body.as_bytes());
        let bytes = manifest(json!([entry(&hash, body, "ViewerQuery", "query")]));
        assert!(parse("apollo", &bytes, &hash, Some("ViewerQuery"), None, true).is_ok());
        let modified = manifest(json!([entry(&hash, body.trim(), "ViewerQuery", "query")]));
        assert!(parse("apollo", &modified, &hash, None, None, true).is_err());
    }

    #[test]
    fn apollo_matches_the_exact_client_scope() {
        let mut first = entry("same", "query ViewerQuery { a }", "ViewerQuery", "query");
        first["clientName"] = json!("web");
        let mut second = entry(
            "same",
            "mutation ViewerQuery { b }",
            "ViewerQuery",
            "mutation",
        );
        second["clientName"] = json!("mobile");
        let bytes = manifest(json!([first, second]));
        assert!(parse("apollo", &bytes, "same", None, Some("web"), false).is_ok());
        assert!(parse("apollo", &bytes, "same", None, Some("mobile"), false).is_err());
        assert!(parse("apollo", &bytes, "same", None, None, false).is_err());
        assert!(parse("apollo", &bytes, "same", None, Some("WEB"), false).is_err());
    }

    #[test]
    fn apollo_null_client_is_unscoped_and_duplicate_mapping_rejected() {
        let mut operation = entry("same", "query ViewerQuery { a }", "ViewerQuery", "query");
        operation["clientName"] = Value::Null;
        assert!(
            parse(
                "apollo",
                &manifest(json!([operation.clone()])),
                "same",
                None,
                None,
                false
            )
            .is_ok()
        );
        let bytes = manifest(json!([operation.clone(), operation]));
        assert!(parse("apollo", &bytes, "same", None, None, false).is_err());
    }

    #[test]
    fn apollo_rejects_format_name_type_and_document_conflicts() {
        for operation in [
            entry("x", "query Actual { a }", "Wrong", "query"),
            entry("x", "mutation ViewerQuery { a }", "ViewerQuery", "query"),
            entry("x", "query ViewerQuery { a }", "ViewerQuery", "mutation"),
            entry(
                "x",
                "query ViewerQuery { a } query Other { b }",
                "ViewerQuery",
                "query",
            ),
            entry(
                "x",
                "query ViewerQuery { a } mutation Other { b }",
                "ViewerQuery",
                "query",
            ),
            entry(
                "x",
                "subscription ViewerQuery { a }",
                "ViewerQuery",
                "query",
            ),
            entry("x", "{ a }", "ViewerQuery", "query"),
        ] {
            assert!(
                parse(
                    "apollo",
                    &manifest(json!([operation])),
                    "x",
                    None,
                    None,
                    false
                )
                .is_err()
            );
        }
        let mut value: Value = serde_json::from_slice(&manifest(json!([entry(
            "x",
            "query Q { a }",
            "Q",
            "query"
        )])))
        .unwrap();
        value["version"] = json!(2);
        assert!(
            parse(
                "apollo",
                &serde_json::to_vec(&value).unwrap(),
                "x",
                None,
                None,
                false
            )
            .is_err()
        );
    }

    #[test]
    fn relay_map_accepts_fragments_and_rejects_wrong_selector_or_mapping() {
        let source = br#"{"registered-42":"query ViewerQuery { viewer { ...User } } fragment User on User { id }"}"#;
        assert_eq!(
            parse("relay", source, "registered-42", None, None, false)
                .unwrap()
                .name,
            "ViewerQuery"
        );
        assert!(parse("relay", source, "registered-42", Some("Other"), None, false).is_err());
        assert!(parse("relay", source, "missing", None, None, false).is_err());
        assert!(
            parse(
                "relay",
                br#"{"x":{"body":"query Q { a }"}}"#,
                "x",
                None,
                None,
                false
            )
            .is_err()
        );
        assert!(
            parse(
                "relay",
                br#"{"x":"query Q { a }","x":"mutation Q { b }"}"#,
                "x",
                None,
                None,
                false
            )
            .is_err()
        );
    }

    #[test]
    fn relay_parses_realistic_modern_generated_module_without_executing_it() {
        let operation = relay(MODERN).unwrap();
        assert_eq!(operation.name, "ViewerQuery");
        assert!(operation.document.is_none());
    }

    #[test]
    fn relay_accepts_integration_fixture_literal() {
        let source = "const node = {kind: 'Request', params: {id: 'relay-opaque', name: 'Items', operationKind: 'query', text: null}}; module.exports = node;";
        let result = parse(
            "relay-artifact",
            source.as_bytes(),
            "relay-opaque",
            Some("Items"),
            None,
            false,
        )
        .unwrap();
        assert_eq!(result.name, "Items");
        assert!(result.document.is_none());
    }

    #[test]
    fn relay_supports_documented_legacy_explicit_shape_and_typescript_header() {
        let source = r#"
            import type { ConcreteRequest } from 'relay-runtime';
            export type ViewerQuery = { readonly data: { readonly id: string; }; };
            const node: ConcreteRequest = (function(){
                return { kind: 'Request', id: 'registered-42', name: 'ViewerQuery',
                         operationKind: 'query', text: null };
            })();
            (node as any).hash = 'debug-hash';
            export default node;
        "#;
        assert!(relay(source).is_ok());
    }

    #[test]
    fn relay_accepts_literal_module_export_and_included_text() {
        let source = r#"module.exports = {kind:'Request', params:{id:'registered-42',name:'ViewerQuery',operationKind:'query',text:'query ViewerQuery { viewer { id } }'}};"#;
        assert!(relay(source).unwrap().document.is_some());
        assert!(relay(&source.replace("query ViewerQuery {", "mutation ViewerQuery {")).is_err());
        assert!(
            parse(
                "relay-artifact",
                MODERN.as_bytes(),
                "registered-42",
                None,
                None,
                true
            )
            .is_err()
        );
    }

    #[test]
    fn relay_rejects_spreads_computed_getters_dynamic_values_and_duplicate_fields() {
        for changed in [
            MODERN.replace("params: {", "params: {...v1,"),
            MODERN.replace("params:", "['params']:"),
            MODERN.replace("params: {", "get params() { return {"),
            MODERN.replace("id: 'registered-42'", "id: v1"),
            MODERN.replace("id: 'registered-42'", "id: getId()"),
            MODERN.replace("id: 'registered-42'", "id: `registered-42`"),
            MODERN.replace(
                "operationKind: 'query'",
                "operationKind: condition ? 'query' : 'mutation'",
            ),
            MODERN.replace(
                "operationKind: 'query'",
                "operationKind: 'mutation', operationKind: 'query'",
            ),
            MODERN.replace("params: {", "params: v1, params: {"),
            MODERN.replace(
                "kind: 'Request'",
                "kind: 'Request', operationKind: 'mutation'",
            ),
            MODERN.replace("id: 'registered-42'", "id: null"),
            MODERN.replace("text: null", "text: unknown"),
            MODERN.replace("text: null", "unused: 'x'"),
        ] {
            assert!(relay(&changed).is_err(), "accepted unsupported structure");
        }
    }

    #[test]
    fn relay_rejects_selector_mutations_decoys_and_dynamic_exports() {
        for changed in [
            format!("{MODERN}\nnode.params.operationKind = 'mutation';"),
            MODERN.replace(
                "(node/*: any*/).hash = 'debug-only';",
                "node.params = replacement;",
            ),
            MODERN.replace("module.exports = node;", "module.exports = choose(node);"),
            MODERN.replace("module.exports = node;", "module.exports = other;"),
            MODERN.replace("return {", "if (false) return {"),
            format!("const decoy = '{{ kind: Request }}'; {MODERN}"),
            format!("/* {MODERN} */ module.exports = {{kind:'Mutation'}};"),
            format!("{MODERN}\n{MODERN}"),
            format!("type = runArbitraryCode(); {MODERN}"),
            format!("type.setter = 'mutation'; {MODERN}"),
        ] {
            assert!(relay(&changed).is_err());
        }
    }

    #[test]
    fn relay_literals_decode_escapes_before_comparison_and_ignore_comments() {
        let escaped = MODERN
            .replace("'registered-42'", "'registered-\\u0034\\u0032'")
            .replace("'query'", "'\\u0071uery'");
        assert!(relay(&escaped).is_ok());
        assert!(
            relay(&MODERN.replace(
                "params: {",
                "params: {/* id: 'other', operationKind: 'mutation' */"
            ))
            .is_ok()
        );
        assert!(relay(&MODERN.replace("'query'", "'mutation' /* 'query' */")).is_err());
        assert!(
            relay(&MODERN.replace(
                "id: 'registered-42'",
                "id: 'registered-42', '\\u0069d': 'other'"
            ))
            .is_err()
        );
    }

    #[test]
    fn relay_comments_cannot_hide_selector_overrides_after_any_js_line_terminator() {
        for terminator in ["\n", "\r", "\r\n", "\u{2028}", "\u{2029}"] {
            assert!(
                relay(&format!(
                    "{MODERN}// harmless{terminator}node.params.operationKind = 'mutation';"
                ))
                .is_err()
            );
            assert!(relay(&format!("// generated{terminator}{MODERN}")).is_ok());
        }
        assert!(
            relay(&format!(
                "{MODERN}// node.params.operationKind = 'mutation';"
            ))
            .is_ok()
        );
    }

    #[test]
    fn graphql_comments_end_at_cr_and_lf_before_the_depth_gate() {
        for terminator in ["\n", "\r", "\r\n"] {
            let body = format!(
                "# comment{terminator}query Q {{ {}id{} }}",
                "child { ".repeat(64),
                "}".repeat(64)
            );
            assert!(check_graphql_depth(&body).is_err());
            assert!(verify_document(&body, "x", None, false).is_err());
        }
        // Unlike ECMAScript, GraphQL comments do not end at U+2028/U+2029.
        for terminator in ["\u{2028}", "\u{2029}"] {
            assert!(check_graphql_depth(&format!("# comment{terminator}{{{{")).is_ok());
        }
    }

    #[test]
    fn graphql_depth_is_bounded_without_counting_strings_or_comments() {
        let body = format!(
            "query ViewerQuery {{ field(arg: \"{}\") }}",
            "}".repeat(200)
        );
        assert!(verify_document(&body, "x", None, false).is_ok());
        let deep = format!(
            "query Q {{ {}id{} }}",
            "nested(arg: \"}}}}\") { ".repeat(65),
            "}".repeat(65)
        );
        assert!(verify_document(&deep, "x", None, false).is_err());
        assert!(
            verify_document(
                "query Q { a } fragment X on T { a } fragment X on T { b }",
                "x",
                None,
                false
            )
            .is_err()
        );
    }

    #[test]
    fn artifact_bounds_and_errors_never_include_source_values() {
        assert!(
            parse(
                "apollo",
                &vec![b' '; 1024 * 1024 + 1],
                "x",
                None,
                None,
                false
            )
            .is_err()
        );
        let error = relay("const secret_token = 'private-value'; fail();").unwrap_err();
        assert_eq!(error.msg, "unsupported_evidence");
        assert!(!error.to_string().contains("private-value"));
        let nested = format!(
            "{{ kind:'Request', child: {}null{} }}",
            "[".repeat(66),
            "]".repeat(66)
        );
        assert!(relay(&nested).is_err());
    }

    #[test]
    fn bounded_parsers_fit_a_one_mib_thread_stack() {
        std::thread::Builder::new()
            .name("bounded-parsers-one-mib".into())
            .stack_size(1024 * 1024)
            .spawn(|| {
                eprintln!("1 MiB stack: JSON depth 64 and depth 65");
                let at_limit = format!("{}0{}", "[".repeat(64), "]".repeat(64));
                assert!(strict_json::parse(at_limit.as_bytes()).is_ok());
                let over_limit = format!("{}0{}", "[".repeat(65), "]".repeat(65));
                assert!(strict_json::parse(over_limit.as_bytes()).is_err());

                eprintln!("1 MiB stack: Relay static depth and size limits");
                let at_limit = format!(
                    "{{kind:'Request',params:{{id:'registered-42',name:'ViewerQuery',operationKind:'query',text:null}},child:{}null{}}}",
                    "[".repeat(63), "]".repeat(63),
                );
                assert!(relay(&at_limit).is_ok());
                let over_limit = format!(
                    "{{kind:'Request',params:{{id:'registered-42',name:'ViewerQuery',operationKind:'query',text:null}},child:{}null{}}}",
                    "[".repeat(64), "]".repeat(64),
                );
                assert!(relay(&over_limit).is_err());
                assert!(parse("relay-artifact", &vec![b' '; 1024 * 1024 + 1], "x", None, None, false).is_err());

                // Depth 50 overflowed a 1 MiB stack in the unoptimized
                // parser. Prove our actual accepted maximum and first denied
                // depth, with each GraphQL line-comment terminator.
                eprintln!("1 MiB stack: accepted GraphQL depth 32 and scanner-rejected 33; LF/CR/CRLF comments");
                for prefix in ["", "# comment\n", "# comment\r", "# comment\r\n"] {
                    for depth in [32, 33] {
                        let body = format!("{prefix}query ViewerQuery {{ {}id{} }}", "child { ".repeat(depth - 1), "}".repeat(depth - 1));
                        assert_eq!(check_graphql_depth(&body).is_ok(), depth <= 32);
                        assert_eq!(verify_document(&body, "x", None, false).is_ok(), depth <= 32);
                    }
                }
            })
            .unwrap()
            .join()
            .unwrap();
    }
}
