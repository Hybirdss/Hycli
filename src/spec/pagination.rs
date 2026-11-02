//! Explicit, versioned pagination contract; no global secret-name exceptions.
use super::{Operation, Spec};
use crate::{
    apperr::{AppError, AppResult},
    policy::{self, Effect},
    util,
};
use std::collections::BTreeMap;
fn invalid() -> AppError {
    AppError::api("data_invalid", 400)
}
fn continuation_name(name: &str) -> bool {
    matches!(
        name.to_ascii_lowercase().replace(['-', '_'], "").as_str(),
        "pagetoken" | "nextpagetoken" | "previouspagetoken" | "continuationtoken"
    )
}
pub(crate) fn validate_pagination(sp: &Spec, op: &Operation) -> AppResult<()> {
    let Some(p) = &op.pagination else {
        return Ok(());
    };
    if sp.spec_version != 3
        || op.method != "GET"
        || op.effect != Effect::Read
        || policy::operation_effect(op, &BTreeMap::new()) != Effect::Read
        || op.body.is_some()
        || !op.preflight.is_empty()
        || !["", "http"].contains(&op.transport.as_str())
        || !util::valid_id(&p.input)
        || util::secret_field(&p.input)
        || ["account", "hycli-account", "allow-local"].contains(&p.input.as_str())
        || p.query.is_empty()
        || p.query.len() > 64
        || !p
            .query
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'_' || c == b'-')
        || policy::dangerous_intent(&p.query)
        || (util::secret_field(&p.query) && !continuation_name(&p.query))
        || op.params.contains_key(&p.input)
        || op.query.contains_key(&p.input)
        || op.query.contains_key(&p.query)
        || op.path.contains(&format!("{{{}}}", p.input))
        || op
            .headers
            .values()
            .chain(sp.defaults.headers.values())
            .any(|v| v.contains(&p.input))
        || !p.next.starts_with('/')
        || p.next.len() > 256
        || p.next
            .split('/')
            .skip(1)
            .any(|s| s.is_empty() || (util::secret_field(s) && !continuation_name(s)))
        || op.response.as_ref().is_some_and(|r| {
            !r.ssr_json.is_empty() || r.html.is_some() || !r.extract_pointer.is_empty()
        })
    {
        return Err(invalid());
    }
    // Only RFC6901 escape forms are supported; no wildcard/implicit traversal.
    for part in p.next.split('/').skip(1) {
        let mut chars = part.chars();
        while let Some(c) = chars.next() {
            if c == '~' && !matches!(chars.next(), Some('0' | '1')) {
                return Err(invalid());
            }
        }
    }
    Ok(())
}
