//! Cluster observed HAR traffic into endpoint templates and draft SiteSpecs.
use std::collections::BTreeMap;

use serde::Serialize;

use crate::apperr;
pub mod har;
use crate::policy;
use crate::spec;
use crate::util;

#[derive(Debug, Clone, Serialize)]
pub struct Cluster {
    pub method: String,
    pub template: String,
    pub count: i64,
    pub last: u16,
    pub is_json: bool,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub query: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub json_body: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub form_body: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub required_pointers: Vec<String>,
    #[serde(default)]
    pub needs_browser_auth: bool,
}

fn infer_scalar_kind(value: &str) -> &'static str {
    let trimmed = value.trim();
    if trimmed.eq_ignore_ascii_case("true") || trimmed.eq_ignore_ascii_case("false") {
        "bool"
    } else if !trimmed.is_empty() && trimmed.parse::<i64>().is_ok() {
        "int"
    } else if !trimmed.is_empty() && trimmed.parse::<f64>().is_ok() {
        "float"
    } else {
        "string"
    }
}

fn infer_json_kind(value: &serde_json::Value) -> &'static str {
    match value {
        serde_json::Value::Bool(_) => "bool",
        serde_json::Value::Number(n) if n.is_i64() || n.is_u64() => "int",
        serde_json::Value::Number(_) => "float",
        serde_json::Value::Array(_) | serde_json::Value::Object(_) => "json",
        _ => "string",
    }
}

pub fn from_har(f: &har::File) -> Vec<Cluster> {
    let mut order: Vec<String> = Vec::new();
    let mut groups: std::collections::HashMap<String, Cluster> = Default::default();
    for e in &f.log.entries {
        let Ok(url) = url::Url::parse(&e.request.url) else {
            continue;
        };
        let path = url.path().to_owned();
        if static_asset(&path) {
            continue;
        }
        let method = e.request.method.trim().to_ascii_uppercase();
        if method.is_empty() {
            continue;
        }
        let tpl = templify(&path);
        let key = format!("{method} {tpl}");
        let c = groups.entry(key.clone()).or_insert_with(|| {
            order.push(key.clone());
            Cluster {
                method: method.clone(),
                template: tpl.clone(),
                count: 0,
                last: 0,
                is_json: false,
                query: BTreeMap::new(),
                json_body: BTreeMap::new(),
                form_body: BTreeMap::new(),
                required_pointers: Vec::new(),
                needs_browser_auth: false,
            }
        });
        c.count += 1;
        c.last = e.response.status;
        for (qk, qv) in url.query_pairs() {
            let name = qk.trim();
            if !name.is_empty() && !util::secret_field(name) && c.query.len() < 16 {
                c.query
                    .entry(name.to_string())
                    .or_insert_with(|| infer_scalar_kind(&qv).to_string());
            }
        }
        for q in &e.request.query_string {
            let name = q.name.trim();
            if !name.is_empty() && !util::secret_field(name) && c.query.len() < 16 {
                c.query
                    .entry(name.to_string())
                    .or_insert_with(|| infer_scalar_kind(&q.value).to_string());
            }
        }
        for h in &e.request.headers {
            let lower = h.name.trim().to_ascii_lowercase();
            if matches!(
                lower.as_str(),
                "cookie" | "authorization" | "x-csrf-token" | "x-xsrf-token" | "x-super-properties"
            ) && !h.value.trim().is_empty()
            {
                c.needs_browser_auth = true;
            }
        }
        if let Some(post) = &e.request.post_data {
            if let Ok(serde_json::Value::Object(obj)) =
                serde_json::from_str::<serde_json::Value>(&post.text)
            {
                for (bk, bv) in obj.iter().take(16) {
                    if !bk.is_empty() && !util::secret_field(bk) {
                        c.json_body
                            .entry(bk.clone())
                            .or_insert_with(|| infer_json_kind(bv).to_string());
                    }
                }
            } else if !post.params.is_empty() {
                for p in post.params.iter().take(16) {
                    let name = p.name.trim();
                    if !name.is_empty() && !util::secret_field(name) {
                        c.form_body
                            .entry(name.to_string())
                            .or_insert_with(|| infer_scalar_kind(&p.value).to_string());
                    }
                }
            } else if post.mime_type.contains("x-www-form-urlencoded") && !post.text.is_empty() {
                for (fk, fv) in url::form_urlencoded::parse(post.text.as_bytes()).take(16) {
                    let name = fk.trim();
                    if !name.is_empty() && !util::secret_field(name) {
                        c.form_body
                            .entry(name.to_string())
                            .or_insert_with(|| infer_scalar_kind(&fv).to_string());
                    }
                }
            }
        }
        let body_trimmed = e.response.content.text.trim_start();
        let looks_json = e.response.content.mime_type.contains("json")
            || body_trimmed.starts_with('{')
            || body_trimmed.starts_with('[');
        if looks_json {
            c.is_json = true;
            if (200..300).contains(&e.response.status)
                && c.required_pointers.is_empty()
                && !body_trimmed.is_empty()
            {
                if let Ok(val) = serde_json::from_str::<serde_json::Value>(body_trimmed) {
                    c.required_pointers = match &val {
                        serde_json::Value::Object(obj) => obj
                            .keys()
                            .take(4)
                            .map(|k| format!("/{}", k.replace('~', "~0").replace('/', "~1")))
                            .collect(),
                        serde_json::Value::Array(arr) if !arr.is_empty() => {
                            vec!["/0".to_string()]
                        }
                        _ => Vec::new(),
                    };
                }
            }
        }
    }
    let mut out: Vec<Cluster> = order.iter().filter_map(|k| groups.remove(k)).collect();
    out.sort_by(|a, b| {
        b.is_json
            .cmp(&a.is_json)
            .then(b.count.cmp(&a.count))
            .then(a.template.cmp(&b.template))
    });
    out
}

fn templify(path: &str) -> String {
    let mut idx = 0;
    let segs: Vec<String> = path
        .split('/')
        .map(|seg| {
            if seg.is_empty() {
                return seg.to_string();
            }
            if is_dynamic(seg) {
                let s = format!("{{id{idx}}}");
                idx += 1;
                s
            } else {
                seg.to_string()
            }
        })
        .collect();
    segs.join("/")
}

fn is_dynamic(seg: &str) -> bool {
    let digits = seg.bytes().all(|b| b.is_ascii_digit()) && !seg.is_empty();
    let uuid = seg.len() == 36 && seg.matches('-').count() == 4;
    let hex_long = seg.len() >= 16 && seg.bytes().all(|b| b.is_ascii_hexdigit());
    digits || uuid || hex_long
}

fn static_asset(path: &str) -> bool {
    [
        ".js", ".css", ".png", ".jpg", ".jpeg", ".gif", ".svg", ".woff", ".woff2", ".ico", ".map",
    ]
    .iter()
    .any(|ext| path.to_lowercase().ends_with(ext))
}

pub fn slugify(c: &Cluster) -> String {
    let p: String = c
        .template
        .chars()
        .map(|ch| match ch {
            '{' | '}' | '/' => '-',
            other => other,
        })
        .collect();
    let p = p.trim_matches('-').to_lowercase();
    let p: String = p
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric() || *ch == '-')
        .collect();
    let p = if p.is_empty() { "root".to_string() } else { p };
    format!("{}-{}", c.method.to_lowercase(), p)
}

/// Build a validated starter SiteSpec from clustered HAR endpoints.
pub fn draft_spec(
    site_name: &str,
    title: &str,
    base_url: &str,
    clusters: &[Cluster],
    limit: usize,
) -> AppResultSpec {
    let limit = if limit == 0 || limit > 100 { 20 } else { limit };
    let selected = &clusters[..clusters.len().min(limit)];
    let any_browser_auth = selected.iter().any(|c| c.needs_browser_auth);
    let mut auth = vec![spec::AuthStrategy {
        name: "public".into(),
        kind: "none".into(),
        header: String::new(),
        prefix: String::new(),
        value_from: String::new(),
        origin: String::new(),
    }];
    if any_browser_auth {
        auth.push(spec::AuthStrategy {
            name: "browser".into(),
            kind: "browser".into(),
            header: String::new(),
            prefix: String::new(),
            value_from: String::new(),
            origin: String::new(),
        });
    }
    let mut sp = spec::Spec {
        spec_version: 2,
        site: spec::SiteInfo {
            source_url: String::new(),
            name: site_name.into(),
            title: title.into(),
            base_url: base_url.into(),
        },
        defaults: Default::default(),
        auth,
        operations: Vec::new(),
        models: Default::default(),
        health: None,
    };
    sp.defaults.timeout = "15s".into();
    sp.defaults.tls_profile = "go".into();

    let mut used = std::collections::HashSet::new();
    for c in selected {
        let mut name = slugify(c);
        while used.contains(&name) {
            name.push_str("-x");
        }
        used.insert(name.clone());
        let method_upper = c.method.to_ascii_uppercase();
        let (effect, evidence) = if matches!(method_upper.as_str(), "GET" | "HEAD" | "OPTIONS")
            && !policy::dangerous_intent(&c.template)
            && !policy::dangerous_intent(&name)
        {
            (
                policy::Effect::Read,
                format!(
                    "Observed {} read request in HAR traffic (last status {}).",
                    method_upper, c.last
                ),
            )
        } else if matches!(method_upper.as_str(), "POST" | "PUT" | "PATCH" | "DELETE")
            || policy::dangerous_intent(&c.template)
        {
            (
                policy::Effect::Write,
                format!(
                    "Observed {} state-changing request in HAR traffic (last status {}).",
                    method_upper, c.last
                ),
            )
        } else {
            (
                policy::Effect::Unknown,
                "Observed browser request; effect has not been established.".into(),
            )
        };
        let mut op = spec::Operation {
            transport: String::new(),
            name,
            desc: format!(
                "capture draft ({} observations, last status {})",
                c.count, c.last
            ),
            method: method_upper,
            path: c.template.clone(),
            base_url: String::new(),
            auth: if c.needs_browser_auth {
                "browser".into()
            } else {
                "public".into()
            },
            preflight: Vec::new(),
            params: Default::default(),
            query: Default::default(),
            headers: Default::default(),
            body: None,
            model: String::new(),
            effect,
            evidence,
            response: if c.is_json {
                Some(spec::ResponseExpectation {
                    format: "json".into(),
                    required_pointers: c.required_pointers.clone(),
                    ..Default::default()
                })
            } else {
                None
            },
        };
        let mut seen_inputs = std::collections::HashSet::new();
        for seg in c.template.split('/') {
            if seg.starts_with('{') {
                let n = seg.trim_matches(|ch| ch == '{' || ch == '}');
                if !n.is_empty() && !util::secret_field(n) && seen_inputs.insert(n.to_string()) {
                    op.params.insert(
                        n.to_string(),
                        spec::Param {
                            kind: "string".into(),
                            required: true,
                            default: None,
                        },
                    );
                }
            }
        }
        for (qk, kind) in &c.query {
            if !util::secret_field(qk) && seen_inputs.insert(qk.clone()) {
                op.query.insert(
                    qk.clone(),
                    spec::Param {
                        kind: kind.clone(),
                        required: false,
                        default: None,
                    },
                );
            }
        }
        if !c.json_body.is_empty() {
            let mut json_map = BTreeMap::new();
            for (bk, kind) in &c.json_body {
                if !util::secret_field(bk) && seen_inputs.insert(bk.clone()) {
                    json_map.insert(
                        bk.clone(),
                        spec::Param {
                            kind: kind.clone(),
                            required: false,
                            default: None,
                        },
                    );
                }
            }
            if !json_map.is_empty() {
                op.body = Some(spec::Body {
                    json: json_map,
                    json_template: None,
                    form: BTreeMap::new(),
                });
            }
        } else if !c.form_body.is_empty() {
            let mut form_map = BTreeMap::new();
            for (fk, kind) in &c.form_body {
                if !util::secret_field(fk) && seen_inputs.insert(fk.clone()) {
                    form_map.insert(
                        fk.clone(),
                        spec::Param {
                            kind: kind.clone(),
                            required: false,
                            default: None,
                        },
                    );
                }
            }
            if !form_map.is_empty() {
                op.body = Some(spec::Body {
                    json: BTreeMap::new(),
                    json_template: None,
                    form: form_map,
                });
            }
        }
        sp.operations.push(op);
    }
    let issues = sp.validate();
    if !issues.is_empty() {
        return Err(apperr::spec(
            format!("Draft validation failed: {issues:?}"),
            "Edit draft.yaml manually.",
        ));
    }
    Ok(sp)
}

pub type AppResultSpec = apperr::AppResult<spec::Spec>;
