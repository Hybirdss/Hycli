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
        effective_inputs: &mut BTreeMap<String, String>,
        secrets: &mut Vec<String>,
    ) -> AppResult<()> {
        for step in &op.preflight {
            let step_url = if !step.op.is_empty() {
                let pre_op = sp.op(&step.op)?;
                let mut step_args = effective_inputs.clone();
                step_args.extend(step.args.clone());
                let (u, _) = sp.build_request(pre_op, &step_args)?;
                u
            } else {
                let base = if step.base_url.is_empty() {
                    sp.operation_base(op)
                } else {
                    step.base_url.as_str()
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
            let r = self
                .read_page_profile(&step_url, account_id, &sp.defaults.tls_profile)
                .await?;
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
                            self.credential(account_id).ok().and_then(|c| {
                                c.cookies
                                    .into_iter()
                                    .find(|ck| ck.name == cookie_name)
                                    .map(|ck| ck.value)
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
                    effective_inputs.insert(format!("step.{}.{var}", step.name), val.clone());
                    effective_inputs.entry(var.clone()).or_insert(val);
                }
            }
        }
        Ok(())
    }
}
