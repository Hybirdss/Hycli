use serde_json::{Map, Value};

use crate::apperr::{self, AppResult};

use super::types::{AuthStrategy, Operation, Param, Spec};

impl Spec {
    pub fn operation_base<'a>(&'a self, op: &'a Operation) -> &'a str {
        if op.base_url.is_empty() {
            &self.site.base_url
        } else {
            &op.base_url
        }
    }

    pub fn find_auth(&self, name: &str) -> Option<&AuthStrategy> {
        self.auth.iter().find(|a| a.name == name)
    }

    pub fn op(&self, name: &str) -> AppResult<&Operation> {
        self.operations
            .iter()
            .find(|o| o.name == name)
            .ok_or_else(|| {
                let names: Vec<&str> = self.operations.iter().map(|o| o.name.as_str()).collect();
                apperr::spec(
                    format!("Unknown operation: {name}"),
                    format!("Available: {}", names.join(", ")),
                )
            })
    }

    /// Build the request URL and body from operation definition and input arguments.
    pub fn build_request(
        &self,
        op: &Operation,
        args: &std::collections::BTreeMap<String, String>,
    ) -> AppResult<(String, Option<Vec<u8>>)> {
        let mut path = interpolate_path_template(&op.path, args);
        for n in path_vars(&op.path) {
            let p = op
                .params
                .get(&n)
                .ok_or_else(|| apperr::spec(format!("Undeclared path parameter: {n}"), ""))?;
            let mut v = coerce_str(p, &n, args, true)?;
            if v.contains("{{") {
                v = interpolate_template_str(&v, &std::collections::BTreeMap::new(), args);
            }
            path = path.replace(&format!("{{{n}}}"), &urlencode_path(&v));
        }
        let mut url_s = format!(
            "{}/{}",
            self.operation_base(op).trim_end_matches('/'),
            path.trim_start_matches('/')
        );

        let empty_resolved = std::collections::BTreeMap::new();
        let mut q = FormEncoder::default();
        for (name, p) in &op.query {
            let present = args.contains_key(name);
            if !present && p.default.is_none() {
                if p.required {
                    return Err(apperr::usage(
                        format!("Missing required input: {name}"),
                        "Provide the input shown by describe.",
                    ));
                }
                continue;
            }
            let mut v = coerce_str(p, name, args, true)?;
            if v.contains("{{") {
                v = interpolate_template_str(&v, &empty_resolved, args);
            }
            q.append_pair(name, &v);
        }
        let qs = q.finish();
        if !qs.is_empty() {
            url_s.push('?');
            url_s.push_str(&qs);
        }

        let mut body = None;
        if let Some(b) = &op.body {
            if !b.form.is_empty() {
                let mut form = FormEncoder::default();
                for (name, p) in &b.form {
                    if !args.contains_key(name) && p.default.is_none() {
                        if p.required {
                            return Err(apperr::usage(
                                format!("Missing required input: {name}"),
                                "Use the action's --help for its inputs.",
                            ));
                        }
                        continue;
                    }
                    let mut v = coerce_str(p, name, args, true)?;
                    if v.contains("{{") {
                        v = interpolate_template_str(&v, &empty_resolved, args);
                    }
                    form.append_pair(name, &v);
                }
                body = Some(form.finish().into_bytes());
            } else if let Some(template) = &b.json_template {
                let mut resolved_values = std::collections::BTreeMap::new();
                for (name, p) in &b.json {
                    let present = args.contains_key(name);
                    if !present && p.default.is_none() {
                        if p.required {
                            return Err(apperr::spec(
                                format!("Missing required argument: --{name}"),
                                format!("Example: --{name} <{}>", p.kind),
                            ));
                        }
                        continue;
                    }
                    resolved_values.insert(name.clone(), coerce_value(p, name, args)?);
                }
                let rendered = render_json_template(template, &resolved_values, args);
                body = Some(serde_json::to_vec(&rendered).unwrap_or_default());
            } else if !b.json.is_empty() {
                let mut obj = Map::new();
                for (name, p) in &b.json {
                    let present = args.contains_key(name);
                    if !present && p.default.is_none() {
                        if p.required {
                            return Err(apperr::spec(
                                format!("Missing required argument: --{name}"),
                                format!("Example: --{name} <{}>", p.kind),
                            ));
                        }
                        continue;
                    }
                    let mut val = coerce_value(p, name, args)?;
                    if let Value::String(s) = &val {
                        if s.contains("{{") {
                            val = Value::String(interpolate_template_str(s, &empty_resolved, args));
                        }
                    }
                    obj.insert(name.clone(), val);
                }
                body = Some(serde_json::to_vec(&Value::Object(obj)).unwrap_or_default());
            }
        }
        Ok((url_s, body))
    }
}

pub(super) fn path_vars(path: &str) -> Vec<String> {
    let mut stripped = String::with_capacity(path.len());
    let mut rest = path;
    while let Some(start) = rest.find("{{") {
        stripped.push_str(&rest[..start]);
        let after = &rest[start + 2..];
        if let Some(end) = after.find("}}") {
            rest = &after[end + 2..];
        } else {
            stripped.push_str(&rest[start..]);
            rest = "";
        }
    }
    stripped.push_str(rest);

    let mut out = Vec::new();
    let mut cur = String::new();
    let mut in_var = false;
    for c in stripped.chars() {
        match c {
            '{' => {
                in_var = true;
                cur.clear();
            }
            '}' => {
                in_var = false;
                if !cur.is_empty() {
                    out.push(cur.clone());
                }
            }
            _ if in_var => cur.push(c),
            _ => {}
        }
    }
    out
}

fn interpolate_path_template(
    template: &str,
    raw_args: &std::collections::BTreeMap<String, String>,
) -> String {
    let mut out = String::new();
    let mut rest = template;
    while let Some(start) = rest.find("{{") {
        out.push_str(&rest[..start]);
        let after_start = &rest[start + 2..];
        if let Some(end) = after_start.find("}}") {
            let var = after_start[..end].trim();
            if let Some(raw) = raw_args.get(var) {
                out.push_str(&urlencode_path(raw));
            }
            rest = &after_start[end + 2..];
        } else {
            out.push_str(&rest[start..]);
            rest = "";
        }
    }
    out.push_str(rest);
    out
}

fn coerce_str(
    p: &Param,
    name: &str,
    args: &std::collections::BTreeMap<String, String>,
    allow_default: bool,
) -> AppResult<String> {
    match args.get(name) {
        Some(v) => {
            check_type(p, name, v)?;
            Ok(v.clone())
        }
        None => {
            if let Some(d) = &p.default {
                if allow_default {
                    return Ok(value_to_plain_string(d));
                }
            }
            if p.required {
                return Err(apperr::spec(
                    format!("Missing required argument: --{name}"),
                    format!("Example: --{name} <{}>", p.kind),
                ));
            }
            Ok(String::new())
        }
    }
}

fn coerce_value(
    p: &Param,
    name: &str,
    args: &std::collections::BTreeMap<String, String>,
) -> AppResult<Value> {
    match args.get(name) {
        Some(v) => {
            check_type(p, name, v)?;
            Ok(match p.kind.as_str() {
                "int" => v
                    .parse::<i64>()
                    .map(Value::from)
                    .map_err(|e| apperr::spec(format!("Failed to parse --{name}: {e}"), ""))?,
                "float" => v
                    .parse::<f64>()
                    .map(Value::from)
                    .map_err(|e| apperr::spec(format!("Failed to parse --{name}: {e}"), ""))?,
                "bool" => v
                    .parse::<bool>()
                    .map(Value::from)
                    .map_err(|e| apperr::spec(format!("Failed to parse --{name}: {e}"), ""))?,
                "json" => serde_json::from_str(v).map_err(|_| {
                    apperr::usage(
                        format!("Invalid structured input: {name}"),
                        "Provide valid JSON.",
                    )
                })?,
                _ => Value::String(v.clone()),
            })
        }
        None => p
            .default
            .clone()
            .ok_or_else(|| apperr::spec(format!("Missing required argument: --{name}"), "")),
    }
}

fn check_type(p: &Param, name: &str, v: &str) -> AppResult<()> {
    let ok = match p.kind.as_str() {
        "int" => v.parse::<i64>().is_ok(),
        "float" => v.parse::<f64>().is_ok_and(|n| n.is_finite()),
        "bool" => v.parse::<bool>().is_ok(),
        "json" => serde_json::from_str::<Value>(v).is_ok(),
        _ => true,
    };
    if ok {
        Ok(())
    } else {
        Err(apperr::spec(
            format!("--{name} must be {}: {v}", p.kind),
            "",
        ))
    }
}

fn value_to_plain_string(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

fn urlencode_path(s: &str) -> String {
    if s == "." {
        return "%2E".into();
    }
    if s == ".." {
        return "%2E%2E".into();
    }
    let mut out = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// Minimal form encoder without external crate dependencies.
#[derive(Default)]
pub struct FormEncoder(String);

impl FormEncoder {
    pub fn append_pair(&mut self, k: &str, v: &str) {
        if !self.0.is_empty() {
            self.0.push('&');
        }
        self.0.push_str(&form_encode(k));
        self.0.push('=');
        self.0.push_str(&form_encode(v));
    }
    pub fn finish(self) -> String {
        self.0
    }
}

fn form_encode(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' | b'*' => {
                out.push(b as char)
            }
            b' ' => out.push('+'),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

pub fn render_json_template(
    template: &Value,
    resolved_values: &std::collections::BTreeMap<String, Value>,
    raw_args: &std::collections::BTreeMap<String, String>,
) -> Value {
    match template {
        Value::Object(map) => Value::Object(
            map.iter()
                .map(|(k, v)| {
                    (
                        k.clone(),
                        render_json_template(v, resolved_values, raw_args),
                    )
                })
                .collect(),
        ),
        Value::Array(items) => Value::Array(
            items
                .iter()
                .map(|v| render_json_template(v, resolved_values, raw_args))
                .collect(),
        ),
        Value::String(s) => {
            if let Some(var) = s
                .strip_prefix("{{")
                .and_then(|rest| rest.strip_suffix("}}"))
            {
                let var = var.trim();
                if !var.contains("{{") {
                    if let Some(val) = resolved_values.get(var) {
                        return val.clone();
                    }
                    if let Some(raw) = raw_args.get(var) {
                        return Value::String(raw.clone());
                    }
                }
            }
            Value::String(interpolate_template_str(s, resolved_values, raw_args))
        }
        other => other.clone(),
    }
}

pub fn interpolate_template_str(
    template: &str,
    resolved_values: &std::collections::BTreeMap<String, Value>,
    raw_args: &std::collections::BTreeMap<String, String>,
) -> String {
    let mut out = String::new();
    let mut rest = template;
    while let Some(start) = rest.find("{{") {
        out.push_str(&rest[..start]);
        let after_start = &rest[start + 2..];
        if let Some(end) = after_start.find("}}") {
            let var = after_start[..end].trim();
            if let Some(raw) = raw_args.get(var) {
                out.push_str(raw);
            } else if let Some(val) = resolved_values.get(var) {
                out.push_str(&value_to_plain_string(val));
            }
            rest = &after_start[end + 2..];
        } else {
            out.push_str(&rest[start..]);
            rest = "";
        }
    }
    out.push_str(rest);
    out
}
