use crate::apperr::{self, AppResult};

use super::{
    request::path_vars,
    types::{AuthStrategy, Issue, Param, SPEC_VERSION, Spec},
};

pub fn parse(data: &[u8]) -> AppResult<Spec> {
    let mut s: Spec = serde_yaml::from_slice(data).map_err(|e| {
        apperr::spec(
            format!("Failed to parse SiteSpec YAML ({e})"),
            "Check YAML syntax.",
        )
    })?;
    if s.spec_version == 0 {
        s.spec_version = SPEC_VERSION;
    }
    if s.defaults.timeout.is_empty() {
        s.defaults.timeout = "15s".into();
    }
    if s.defaults.tls_profile.is_empty() {
        s.defaults.tls_profile = "go".into();
    }
    if s.auth.is_empty() {
        s.auth.push(AuthStrategy {
            oauth_refresh: None,
            name: "public".into(),
            kind: "none".into(),
            header: String::new(),
            prefix: String::new(),
            value_from: String::new(),
            origin: String::new(),
        });
    }
    Ok(s)
}

impl Spec {
    pub fn validate(&self) -> Vec<Issue> {
        let mut out: Vec<Issue> = Vec::new();
        {
            fn add(out: &mut Vec<Issue>, field: &str, detail: String, remedy: &str) {
                out.push(Issue {
                    field: field.into(),
                    detail,
                    remedy: remedy.into(),
                });
            }

            if self.spec_version != SPEC_VERSION && self.spec_version != 2 && self.spec_version != 3
            {
                add(
                    &mut out,
                    "spec_version",
                    format!("supported: 1|2|3, got: {}", self.spec_version),
                    "Set spec_version to 1, 2 or 3",
                );
            }
            if self.spec_version != 3 && self.operations.iter().any(|op| op.pagination.is_some()) {
                add(
                    &mut out,
                    "spec_version",
                    "Explicit private pagination requires SiteSpec v3".into(),
                    "Set spec_version to 3 for an evidenced supported pagination contract, or remove pagination metadata; never expose the raw continuation as an ordinary input.",
                );
            }
            if self.site.name.is_empty() {
                add(
                    &mut out,
                    "site.name",
                    "empty".into(),
                    "Site identifier is required",
                );
            }
            if self.site.base_url.is_empty() {
                add(
                    &mut out,
                    "site.base_url",
                    "empty".into(),
                    "Provide an https:// absolute URL",
                );
            } else if !self.site.base_url.starts_with("http") {
                add(
                    &mut out,
                    "site.base_url",
                    "missing http(s) scheme".into(),
                    "",
                );
            }

            let mut auths = std::collections::HashSet::new();
            let mut grants = std::collections::HashSet::new();
            for a in &self.auth {
                if let Some(binding) = &a.oauth_refresh {
                    if a.name.is_empty()
                        || !grants.insert(&binding.grant_ref)
                        || !a.value_from.is_empty()
                        || binding.check().is_err()
                        || a.kind != "header"
                        || !a.header.eq_ignore_ascii_case("authorization")
                        || a.prefix != "Bearer "
                        || a.origin.is_empty()
                    {
                        add(
                            &mut out,
                            &format!("auth.{}.oauth_refresh", a.name),
                            "Invalid explicit OAuth refresh binding".into(),
                            "Use pinned endpoints, a stable subject pointer and a private grant reference with Bearer header auth.",
                        );
                    }
                    if self
                        .operations
                        .iter()
                        .filter(|op| op.auth == a.name)
                        .any(|op| {
                            op.effect != crate::policy::Effect::Read
                                || !matches!(op.transport.as_str(), "" | "http")
                                || !op.preflight.is_empty()
                        })
                    {
                        add(
                            &mut out,
                            &format!("auth.{}.oauth_refresh", a.name),
                            "OAuth refresh requires direct read-only operations without preflight"
                                .into(),
                            "Keep browser sessions and write operations separate.",
                        );
                    }
                }

                if !auths.insert(a.name.clone()) {
                    add(
                        &mut out,
                        &format!("auth.{}", a.name),
                        "duplicate auth strategy name".into(),
                        "",
                    );
                }
                match a.kind.as_str() {
                    "none" | "browser" => {}
                    "header" => {
                        if a.header.is_empty() {
                            add(
                                &mut out,
                                &format!("auth.{}.header", a.name),
                                "missing header name for kind=header".into(),
                                "",
                            );
                        }
                        if a.oauth_refresh.is_none() && !a.value_from.starts_with("store:") {
                            add(
                                &mut out,
                                &format!("auth.{}.value_from", a.name),
                                "must reference store:<site>/<key>".into(),
                                "Never embed raw secrets in a SiteSpec",
                            );
                        }
                    }
                    "derived" => {
                        if a.header.is_empty() {
                            add(
                                &mut out,
                                &format!("auth.{}.header", a.name),
                                "missing header name for kind=derived".into(),
                                "",
                            );
                        }
                        if !["cookie:", "storage:", "header:"]
                            .iter()
                            .any(|p| a.value_from.starts_with(p))
                        {
                            add(
                                &mut out,
                                &format!("auth.{}.value_from", a.name),
                                "must reference cookie:<name>, storage:<key>[#<pointer>], or header:<name>".into(),
                                "Example: cookie:ct0",
                            );
                        }
                    }
                    other => add(
                        &mut out,
                        &format!("auth.{}.kind", a.name),
                        format!("unsupported auth kind: {other}"),
                        "none|header|browser|derived",
                    ),
                }
            }
            let models: std::collections::HashSet<&String> = self.models.keys().collect();
            let op_names: std::collections::HashSet<&String> =
                self.operations.iter().map(|o| &o.name).collect();

            for op in &self.operations {
                let id = format!("operations.{}", op.name);
                if op.method.is_empty() {
                    add(
                        &mut out,
                        &format!("{id}.method"),
                        "empty".into(),
                        "GET|POST|PUT|PATCH|DELETE",
                    );
                }
                if !op.path.starts_with('/') && !(op.derived_target && op.path.is_empty()) {
                    add(
                        &mut out,
                        &format!("{id}.path"),
                        format!("must start with /: {}", op.path),
                        "",
                    );
                }
                if !["", "http", "browser-fetch", "browser"].contains(&op.transport.as_str()) {
                    add(
                        &mut out,
                        &format!("{id}.transport"),
                        format!("unsupported transport: {}", op.transport),
                        "http|browser-fetch|browser",
                    );
                }
                if let Some(expected) = &op.response {
                    if expected.format == "file" {
                        let valid_types = expected.file.as_ref().is_some_and(|file| {
                            !file.media_types.is_empty()
                                && file.media_types.len() <= 16
                                && file.media_types.iter().all(|media| {
                                    media.len() <= 127
                                        && media.matches('/').count() == 1
                                        && media == &media.to_ascii_lowercase()
                                        && media.split_once('/').is_some_and(|(kind, subtype)| {
                                            !kind.is_empty()
                                                && !subtype.is_empty()
                                                && media.bytes().all(|b| {
                                                    b.is_ascii_alphanumeric()
                                                        || b"/!#$&^_.+-".contains(&b)
                                                })
                                        })
                                })
                        });
                        if !valid_types
                            || op.pagination.is_some()
                            || !op.model.is_empty()
                            || !expected.required_pointers.is_empty()
                            || !expected.required_html_fields.is_empty()
                            || expected.html.is_some()
                            || !expected.strip_prefix.is_empty()
                            || !expected.ssr_json.is_empty()
                            || !expected.extract_pointer.is_empty()
                            || !["", "http"].contains(&op.transport.as_str())
                        {
                            add(&mut out, &format!("{id}.response"),
                                "File responses require explicit media types and HTTP transport, without extraction, models or pagination".into(),
                                "Set response.format: file and response.file.media_types to observed exact media types.");
                        }
                    } else if expected.file.is_some() {
                        add(
                            &mut out,
                            &format!("{id}.response.file"),
                            "File expectations require format: file".into(),
                            "Remove file or use format: file.",
                        );
                    }
                }
                if !op.auth.is_empty() && !auths.contains(&op.auth) {
                    add(
                        &mut out,
                        &format!("{id}.auth"),
                        format!("undefined auth strategy: {}", op.auth),
                        "Define in auth block",
                    );
                }
                if !op.model.is_empty() && !models.contains(&op.model) {
                    add(
                        &mut out,
                        &format!("{id}.model"),
                        format!("undefined model: {}", op.model),
                        "Define in models block",
                    );
                }
                for step in &op.preflight {
                    let valid_target = (!step.op.is_empty() && op_names.contains(&step.op))
                        || (!step.path.is_empty() && step.path.starts_with('/'));
                    if step.name.is_empty() || !valid_target {
                        add(
                            &mut out,
                            &format!("{id}.preflight"),
                            format!("invalid preflight step: {}", step.name),
                            "Reference a defined operation or a path starting with '/'",
                        );
                    }
                }
                let path_names = path_vars(&op.path);
                for n in &path_names {
                    if !op.params.contains_key(n) {
                        add(
                            &mut out,
                            &format!("{id}.params"),
                            format!("undeclared path parameter {{{n}}}"),
                            "Declare in params",
                        );
                    }
                }
                for n in op.params.keys() {
                    if !path_names.contains(n) {
                        add(
                            &mut out,
                            &format!("{id}.params.{n}"),
                            "parameter not present in path template".into(),
                            &format!("Use {{{n}}} in path or move to query"),
                        );
                    }
                }
                let mut counts: std::collections::BTreeMap<&String, usize> = Default::default();
                for k in op.params.keys() {
                    *counts.entry(k).or_default() += 1;
                }
                for k in op.query.keys() {
                    *counts.entry(k).or_default() += 1;
                }
                if let Some(b) = &op.body {
                    let encodings = [
                        !b.json.is_empty() || b.json_template.is_some(),
                        !b.form.is_empty(),
                        !b.multipart.is_empty(),
                    ]
                    .iter()
                    .filter(|used| **used)
                    .count();
                    if encodings > 1 {
                        add(
                            &mut out,
                            &format!("{id}.body"),
                            "Choose one body encoding".into(),
                            "Use body.json/json_template, body.form, or body.multipart, not more than one.",
                        );
                    }
                    for (k, _) in b.inputs() {
                        *counts.entry(k).or_default() += 1;
                    }
                }
                for (n, c) in &counts {
                    if *c > 1 {
                        add(
                            &mut out,
                            id.as_str(),
                            format!("input name conflict: {n}"),
                            "Keep input names unique across params/query/body",
                        );
                    }
                }
                let check = |where_: &str,
                             ps: &std::collections::BTreeMap<String, Param>,
                             out: &mut Vec<Issue>| {
                    for (n, p) in ps {
                        match p.kind.as_str() {
                            "string" | "int" | "float" | "bool" | "json" => {}
                            other => out.push(Issue {
                                field: format!("{where_}.{n}.type"),
                                detail: format!("unsupported type: {other}"),
                                remedy: "string|int|float|bool|json".into(),
                            }),
                        }
                        if p.required && p.default.is_some() {
                            out.push(Issue {
                                field: format!("{where_}.{n}"),
                                detail: "required parameter cannot also define a default".into(),
                                remedy: "Use required or default, not both".into(),
                            });
                        }
                    }
                };
                check(&format!("{id}.params"), &op.params, &mut out);
                check(&format!("{id}.query"), &op.query, &mut out);
                if let Some(b) = &op.body {
                    check(&format!("{id}.body.json"), &b.json, &mut out);
                    check(&format!("{id}.body.form"), &b.form, &mut out);
                    for (n, p) in &b.multipart {
                        match p.kind.as_str() {
                            "string" | "int" | "float" | "bool" | "json" | "file" => {}
                            other => out.push(Issue {
                                field: format!("{id}.body.multipart.{n}.type"),
                                detail: format!("unsupported type: {other}"),
                                remedy: "string|int|float|bool|json|file".into(),
                            }),
                        }
                        if (p.required || p.kind == "file") && p.default.is_some() {
                            out.push(Issue {
                                field: format!("{id}.body.multipart.{n}"),
                                detail: "file inputs and required parameters cannot define defaults".into(),
                                remedy: "Provide file inputs explicitly; omit defaults for required parameters".into(),
                            });
                        }
                    }
                }
            }
        }
        out
    }
}
