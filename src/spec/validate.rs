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

            if self.spec_version != SPEC_VERSION && self.spec_version != 2 {
                add(
                    &mut out,
                    "spec_version",
                    format!("supported: 1|2, got: {}", self.spec_version),
                    "Set spec_version to 1 or 2",
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
            for a in &self.auth {
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
                        if !a.value_from.starts_with("store:") {
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
                if !op.path.starts_with('/') {
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
                    if (!b.json.is_empty() || b.json_template.is_some()) && !b.form.is_empty() {
                        add(
                            &mut out,
                            &format!("{id}.body"),
                            "Choose one body encoding".into(),
                            "Use body.json/json_template or body.form, not both.",
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
                }
            }
        }
        out
    }
}
