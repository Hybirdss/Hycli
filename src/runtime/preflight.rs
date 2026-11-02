use crate::{
    apperr::{AppError, AppResult},
    spec::{Operation, Spec},
    util,
};
use serde_json::Value;
use std::collections::BTreeMap;

use super::Runtime;

impl Runtime {
    pub(crate) async fn run_preflight_steps(
        &self,
        sp: &Spec,
        op: &Operation,
        account_id: &str,
        credential_snapshot: Option<&crate::accounts::Credential>,
        effective_inputs: &mut BTreeMap<String, String>,
        secrets: &mut Vec<String>,
        check_authority: &(dyn Fn() -> AppResult<()> + Sync),
    ) -> AppResult<BTreeMap<String, String>> {
        let mut extracted_values = BTreeMap::new();
        for step in &op.preflight {
            let step_url = if !step.op.is_empty() {
                let pre_op = sp.op(&step.op)?;
                let mut step_args = effective_inputs.clone();
                step_args.extend(step.args.clone());
                let (u, _) = sp.build_request(pre_op, &step_args)?;
                u
            } else {
                let base = if !step.base_url.is_empty() {
                    step.base_url.as_str()
                } else if op.derived_target {
                    // The op's own base is a derived (lease) target, so a step
                    // without an explicit base fetches the lease from the site.
                    sp.site.base_url.as_str()
                } else {
                    sp.operation_base(op)
                };
                format!(
                    "{}/{}",
                    base.trim_end_matches('/'),
                    step.path.trim_start_matches('/')
                )
            };
            let parsed = crate::net::validate_url(&step_url)?;
            if !crate::net::same_site_origin(
                &parsed,
                &crate::net::validate_url(sp.site.source_url())?,
            ) {
                return Err(AppError::api("unsafe_target", 400));
            }
            self.ensure_credential_snapshot(
                &sp.site.name,
                &step_url,
                account_id,
                credential_snapshot,
            )?;
            check_authority()?;
            let upload =
                op.derived_target || op.body.as_ref().is_some_and(|b| !b.multipart.is_empty());
            let header_auth = sp
                .find_auth(&op.auth)
                .is_some_and(|auth| matches!(auth.kind.as_str(), "header" | "derived"))
                || sp.defaults.headers.keys().any(|name| {
                    ![
                        "accept",
                        "accept-language",
                        "user-agent",
                        "content-type",
                        "content-md5",
                    ]
                    .iter()
                    .any(|public| name.eq_ignore_ascii_case(public))
                });
            let r = if upload && header_auth {
                if !crate::policy::safe_read_url(&parsed) {
                    return Err(AppError::api("approval_required", 409));
                }
                // The lease belongs to the source authority, not to the eventual
                // upload destination. Do not send destination header templates.
                let mut source_op = op.clone();
                source_op.headers.clear();
                source_op.body = None;
                let (headers, _, _) =
                    self.headers_snapshot(sp, &source_op, &step_url, account_id)?;
                self.network
                    .fetch_guarded(
                        crate::net::Request {
                            url: step_url.clone(),
                            method: "GET".into(),
                            headers,
                            body: None,
                            account_id: account_id.into(),
                            limit: 2 * 1024 * 1024,
                            timeout: std::time::Duration::from_secs(30),
                            read_only: true,
                            follow_redirects: true,
                            tls_profile: sp.defaults.tls_profile.clone(),
                            multipart: None,
                        },
                        None,
                        check_authority,
                    )
                    .await?
            } else {
                self.read_page_checked(
                    &step_url,
                    account_id,
                    &sp.defaults.tls_profile,
                    credential_snapshot,
                    || {
                        check_authority()?;
                        self.ensure_credential_snapshot(
                            &sp.site.name,
                            &step_url,
                            account_id,
                            credential_snapshot,
                        )
                    },
                )
                .await?
            };
            if !r.successful() {
                return Err(AppError::api(
                    if r.status == 401 {
                        "auth_required"
                    } else {
                        "operation_failed"
                    },
                    r.status,
                ));
            }
            let text = String::from_utf8_lossy(&r.bytes);
            let json_doc = r.json();
            let ssr_map = crate::html::extract_ssr_map(&text);
            for (var, expr) in &step.extract {
                let extracted: Option<String> = if let Some(sel) = expr.strip_prefix("css:") {
                    let (selector, attribute) = sel.split_once('@').unwrap_or((sel, ""));
                    let field = crate::html::Field {
                        selector: selector.to_string(),
                        attribute: attribute.to_string(),
                    };
                    let doc = scraper::Html::parse_document(&text);
                    field.extract(&doc)
                } else if let Some(ssr_spec) = expr.strip_prefix("ssr:") {
                    let (key, pointer) = ssr_spec.split_once('#').unwrap_or((ssr_spec, ""));
                    ssr_map.get(key).and_then(|doc| {
                        crate::html::resolve_pointer(doc, pointer).and_then(|v| match v {
                            Value::String(s) => Some(s.clone()),
                            Value::Number(n) => Some(n.to_string()),
                            _ => None,
                        })
                    })
                } else if let Some(hdr) = expr.strip_prefix("header:") {
                    let h = r.header(hdr);
                    if h.is_empty() {
                        None
                    } else {
                        Some(h.to_string())
                    }
                } else if let Some(cookie_name) = expr.strip_prefix("cookie:") {
                    let from_set_cookie = r.cookie_value(cookie_name);
                    from_set_cookie.or_else(|| {
                        if account_id.is_empty() {
                            None
                        } else {
                            credential_snapshot.and_then(|c| {
                                c.cookies
                                    .iter()
                                    .find(|ck| ck.name == cookie_name)
                                    .map(|ck| ck.value.clone())
                            })
                        }
                    })
                } else if expr.starts_with('/') || expr.starts_with("json:/") {
                    let ptr = expr.strip_prefix("json:").unwrap_or(expr);
                    json_doc.as_ref().and_then(|doc| {
                        crate::html::resolve_pointer(doc, ptr).and_then(|v| match v {
                            Value::String(s) => Some(s.clone()),
                            Value::Number(n) => Some(n.to_string()),
                            _ => None,
                        })
                    })
                } else {
                    None
                };
                if let Some(val) = extracted.filter(|s| !s.is_empty()) {
                    if expr.starts_with("cookie:")
                        || expr.starts_with("header:")
                        || util::secret_field(var)
                    {
                        secrets.push(val.clone());
                    }
                    extracted_values.insert(format!("step.{}.{var}", step.name), val.clone());
                    effective_inputs.insert(format!("step.{}.{var}", step.name), val.clone());
                    effective_inputs.entry(var.clone()).or_insert(val);
                }
            }
        }
        Ok(extracted_values)
    }
}

/// Cross-origin uploads start with an empty header set. Only operation headers
/// whose dynamic values came from non-cookie lease fields may cross the boundary.
pub(crate) fn destination_headers(
    op: &Operation,
    inputs: &BTreeMap<String, String>,
    source_secrets: &[String],
) -> AppResult<Vec<(String, String)>> {
    let mut headers = Vec::new();
    for (name, template) in &op.headers {
        if name.eq_ignore_ascii_case("cookie") || name.eq_ignore_ascii_case("host") {
            return Err(AppError::api("unsafe_target", 400));
        }
        let has_lease_value = validate_destination_template(op, template, inputs)?;
        // Static protocol metadata is portable; static credentials are not.
        if !has_lease_value
            && !["accept", "content-type", "content-md5"]
                .iter()
                .any(|header| name.eq_ignore_ascii_case(header))
        {
            return Err(AppError::api("unsafe_target", 400));
        }
        let value = crate::spec::interpolate_template_str(template, &BTreeMap::new(), inputs);
        if source_secrets
            .iter()
            .any(|secret| !secret.is_empty() && value.contains(secret))
        {
            return Err(AppError::api("unsafe_target", 400));
        }
        headers.push((name.clone(), value));
    }
    Ok(headers)
}

pub(crate) fn source_credential_expression(expression: &str) -> bool {
    expression.starts_with("cookie:")
        || expression.strip_prefix("header:").is_some_and(|header| {
            [
                "cookie",
                "set-cookie",
                "authorization",
                "proxy-authorization",
            ]
            .iter()
            .any(|name| header.eq_ignore_ascii_case(name))
        })
}

/// Validate generated request text by its source, never by scanning user files
/// or recursively expanding caller-provided text.
pub(crate) fn validate_generated_request(
    op: &Operation,
    explicit: &BTreeMap<String, String>,
    effective: &BTreeMap<String, String>,
    extracted: &BTreeMap<String, String>,
    source_secrets: &[String],
) -> AppResult<()> {
    struct Provenance<'a> {
        op: &'a Operation,
        explicit: &'a BTreeMap<String, String>,
        extracted: &'a BTreeMap<String, String>,
        secrets: &'a [String],
    }
    impl Provenance<'_> {
        fn literal(&self, text: &str) -> AppResult<()> {
            if self
                .secrets
                .iter()
                .any(|secret| !secret.is_empty() && text.contains(secret))
            {
                return Err(AppError::api("unsafe_target", 400));
            }
            Ok(())
        }
        fn reference(&self, key: &str, depth: usize) -> AppResult<()> {
            if depth > 8 {
                return Err(AppError::api("unsafe_target", 400));
            }
            if self.explicit.contains_key(key) {
                return Ok(());
            }
            let sources: Vec<_> = self
                .op
                .preflight
                .iter()
                .flat_map(|step| {
                    step.extract.iter().filter_map(move |(field, expression)| {
                        let qualified = format!("step.{}.{field}", step.name);
                        ((key == qualified || key == field.as_str())
                            && self.extracted.contains_key(&qualified))
                        .then_some((qualified, expression))
                    })
                })
                .collect();
            if !sources.is_empty() {
                if sources.len() != 1 || source_credential_expression(sources[0].1) {
                    return Err(AppError::api("unsafe_target", 400));
                }
                return self.literal(&self.extracted[&sources[0].0]);
            }
            let param = self
                .op
                .params
                .get(key)
                .or_else(|| self.op.query.get(key))
                .or_else(|| {
                    self.op.body.as_ref().and_then(|body| {
                        body.inputs()
                            .find(|(name, _)| name.as_str() == key)
                            .map(|(_, param)| param)
                    })
                });
            if let Some(value) = param.and_then(|p| p.default.as_ref()) {
                return self.template(&util::plain(value), depth + 1);
            }
            Err(AppError::api("unsafe_target", 400))
        }
        fn template(&self, template: &str, depth: usize) -> AppResult<()> {
            let mut rest = template;
            while let Some(start) = rest.find("{{") {
                self.literal(&rest[..start])?;
                let after = &rest[start + 2..];
                let end = after
                    .find("}}")
                    .ok_or_else(|| AppError::api("unsafe_target", 400))?;
                self.reference(after[..end].trim(), depth)?;
                rest = &after[end + 2..];
            }
            self.literal(rest)
        }
        fn json(&self, value: &Value) -> AppResult<()> {
            match value {
                Value::String(text) => self.template(text, 0),
                Value::Array(values) => {
                    for value in values {
                        self.json(value)?;
                    }
                    Ok(())
                }
                Value::Object(values) => {
                    for value in values.values() {
                        self.json(value)?;
                    }
                    Ok(())
                }
                _ => Ok(()),
            }
        }
    }
    let provenance = Provenance {
        op,
        explicit,
        extracted,
        secrets: source_secrets,
    };
    for (name, param) in op
        .params
        .iter()
        .chain(op.query.iter())
        .chain(op.body.iter().flat_map(|body| body.inputs()))
    {
        if param.kind != "file"
            && !explicit.contains_key(name)
            && (effective.contains_key(name) || param.default.is_some())
        {
            provenance.reference(name, 0)?;
        }
    }
    if let Some(template) = op
        .body
        .as_ref()
        .and_then(|body| body.json_template.as_ref())
    {
        provenance.json(template)?;
    }
    Ok(())
}

fn validate_destination_template(
    op: &Operation,
    template: &str,
    inputs: &BTreeMap<String, String>,
) -> AppResult<bool> {
    let mut rest = template;
    let mut has_lease_value = false;
    while let Some(start) = rest.find("{{") {
        let after = &rest[start + 2..];
        let Some(end) = after.find("}}") else {
            return Err(AppError::api("unsafe_target", 400));
        };
        let key = after[..end].trim();
        let expressions: Vec<_> = op
            .preflight
            .iter()
            .flat_map(|step| {
                step.extract.iter().filter_map(move |(field, expr)| {
                    (key == format!("step.{}.{field}", step.name)).then_some(expr)
                })
            })
            .collect();
        let trusted = expressions.len() == 1
            && expressions
                .iter()
                .all(|expr| !source_credential_expression(expr));
        if !trusted || !inputs.contains_key(key) {
            return Err(AppError::api("unsafe_target", 400));
        }
        has_lease_value = true;
        rest = &after[end + 2..];
    }
    Ok(has_lease_value)
}

pub(crate) fn validate_destination_url(
    sp: &Spec,
    op: &Operation,
    inputs: &BTreeMap<String, String>,
    source_secrets: &[String],
    url: &str,
) -> AppResult<()> {
    validate_destination_template(op, sp.operation_base(op), inputs)?;
    validate_destination_template(op, &op.path, inputs)?;
    for param in op.query.values() {
        if let Some(value) = &param.default {
            validate_destination_template(op, &util::plain(value), inputs)?;
        }
    }
    // Form decoding also covers percent-encoded secrets in paths and query
    // values. A signed lease URL is retained byte-for-byte unless it echoes a
    // source credential; file contents are never inspected here.
    let echoes_source = |value: &str| {
        source_secrets
            .iter()
            .any(|secret| !secret.is_empty() && value.contains(secret))
    };
    if echoes_source(url)
        || url::form_urlencoded::parse(url.as_bytes())
            .any(|(key, value)| echoes_source(&key) || echoes_source(&value))
    {
        return Err(AppError::api("unsafe_target", 400));
    }
    Ok(())
}

#[cfg(test)]
mod destination_header_tests {
    use super::*;
    use serde_json::json;

    fn operation(header: &str, expression: &str) -> Operation {
        serde_json::from_value(json!({
            "name":"upload", "method":"POST", "path":"/put", "effect":"write",
            "headers":{"X-Signed-Target":header},
            "preflight":[{"name":"lease","path":"/lease","extract":{"signature":expression}}]
        }))
        .unwrap()
    }

    #[test]
    fn destination_signature_is_preserved_but_cookie_alias_and_echo_are_rejected() {
        let inputs = BTreeMap::from([("step.lease.signature".into(), "target-signature".into())]);
        let op = operation("{{step.lease.signature}}", "json:/signature");
        assert_eq!(
            destination_headers(&op, &inputs, &["source-secret".into()]).unwrap(),
            vec![("X-Signed-Target".into(), "target-signature".into())]
        );
        for expression in [
            "cookie:session",
            "header:Set-Cookie",
            "header:Authorization",
        ] {
            assert!(
                destination_headers(
                    &operation("{{step.lease.signature}}", expression),
                    &inputs,
                    &[]
                )
                .is_err()
            );
        }
        assert!(destination_headers(&op, &inputs, &["target-signature".into()]).is_err());
        assert!(
            destination_headers(&operation("{{signature}}", "json:/signature"), &inputs, &[])
                .is_err()
        );
        assert!(
            destination_headers(
                &operation("literal-secret", "json:/signature"),
                &inputs,
                &[]
            )
            .is_err()
        );
    }
}
