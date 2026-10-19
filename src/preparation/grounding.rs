use super::models::Source;
use crate::{
    apperr::{AppError, AppResult},
    policy,
    runtime::{self, Runtime},
    spec::Spec,
    state::Job,
    util,
};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet, VecDeque};

impl Runtime {
    #[allow(clippy::too_many_arguments)]
    pub(super) async fn ingest_round_reads(
        &self,
        job: &Job,
        initial: &url::Url,
        round: usize,
        account_id: &str,
        queue: &mut Vec<String>,
        seen: &mut BTreeSet<String>,
        sources: &mut Vec<Source>,
        api_bases: &mut BTreeSet<String>,
        routes: &mut BTreeSet<(String, String)>,
        links: &mut BTreeSet<String>,
        api_routes: &mut BTreeSet<(String, String)>,
        evidence: &mut BTreeSet<String>,
        api_prefixes: &mut BTreeSet<String>,
        documents: &mut BTreeMap<String, String>,
        auth_requirements: &mut AuthRequirements,
        icon_candidates: &mut Vec<String>,
    ) -> AppResult<()> {
        let round_limit = if round == 0 { 6 } else { 4 };
        let mut reads: VecDeque<_> = std::mem::take(queue)
            .into_iter()
            .take(round_limit)
            .collect();
        let mut round_reads = 0;
        while let Some(raw) = reads.pop_front() {
            if round_reads >= round_limit {
                break;
            }
            round_reads += 1;
            if seen.len() >= 48 || !seen.insert(raw.clone()) {
                continue;
            }
            let parsed = crate::net::validate_url(&raw)?;
            let account = if crate::net::same_site_origin(&parsed, initial) {
                account_id
            } else {
                ""
            };
            self.job_note(&job.id, "reading", "", 0, "")?;
            let response = self.read_evidence_page(&raw, account).await;
            let response = match response {
                Ok(r) => r,
                Err(e) => {
                    sources.push(Source {
                        url: raw.clone(),
                        kind: "Read failed; choose a different available approach".into(),
                        content: json!({"error":e.public_code}),
                    });
                    continue;
                }
            };
            if !response.successful() {
                sources.push(Source {
                    url: raw.clone(),
                    kind: "Read failed; choose a different available approach".into(),
                    content: json!({
                        "status": response.status,
                        "defense": response.defense,
                        "error": if response.status == 401 {
                            "auth_required"
                        } else if response.status == 429 {
                            "site_rate_limited"
                        } else if response.status == 403 {
                            "blocked_by_site"
                        } else {
                            "request_failed"
                        }
                    }),
                });
                continue;
            }
            let text = String::from_utf8_lossy(&response.bytes);
            let base = crate::net::validate_url(&response.url)?;
            let is_js = response.header("content-type").contains("javascript")
                || base.path().ends_with(".js");
            api_bases.insert(base.origin().ascii_serialization());
            if !is_js {
                routes.insert(("GET".into(), base.path().into()));
            }
            api_bases.extend(crate::discovery::api_bases(&base, &text));
            api_bases.extend(crate::discovery::bundle_api_bases(&base, &text));
            links.extend(crate::discovery::header_links(
                &base,
                &response.header("link"),
            ));
            if !is_js {
                links.extend(crate::discovery::published_links(&base, &text));
            }
            let mut script_links = crate::discovery::bundle_urls(&base, &text);
            script_links.sort_by_key(|u| {
                let lower = u.to_ascii_lowercase();
                if lower.contains("polyfill")
                    || lower.contains("vendor")
                    || lower.contains("runtime")
                    || lower.contains("framework")
                {
                    1usize
                } else {
                    0usize
                }
            });
            links.extend(script_links.iter().cloned());
            let vis_text = if is_js {
                util::redact_text(&text.chars().take(120_000).collect::<String>())
            } else {
                runtime::visible_text(&text)
            };
            let documented = crate::discovery::documented_routes(&vis_text);
            api_routes.extend(documented.iter().cloned());
            routes.extend(documented);
            let js_routes = crate::discovery::bundle_routes(&text);
            for (_, path) in &js_routes {
                evidence.insert(path.clone());
            }
            routes.extend(js_routes.iter().cloned());
            api_prefixes.extend(crate::discovery::api_prefixes(initial, &text));
            let mut cached = json!(vis_text.clone());
            let secrets = if account_id.is_empty() {
                vec![]
            } else {
                self.credential(account_id)?.redaction_values()
            };
            util::redact(&mut cached, &secrets);
            documents.insert(base.to_string(), cached.as_str().unwrap_or("").to_string());
            let is_json = response
                .json()
                .or_else(|| crate::discovery::api_schema(&text));
            if let Some(mut doc) = is_json {
                let secrets = if account_id.is_empty() {
                    vec![]
                } else {
                    self.credential(account_id)?.redaction_values()
                };
                let is_schema = doc.get("openapi").is_some() || doc.get("swagger").is_some();
                if is_schema {
                    auth_requirements.extend(documented_auth(&doc, &job.site_id));
                }
                util::redact(&mut doc, &secrets);
                if is_schema {
                    if let Some(paths) = doc.get("paths").and_then(Value::as_object) {
                        for (path, methods) in paths {
                            evidence.insert(path.clone());
                            if let Some(methods) = methods.as_object() {
                                for method in methods.keys().filter(|m| {
                                    ["get", "head", "post", "put", "patch", "delete"]
                                        .contains(&m.as_str())
                                }) {
                                    routes.insert((method.to_uppercase(), path.clone()));
                                    api_routes.insert((method.to_uppercase(), path.clone()));
                                }
                            }
                        }
                    }
                    sources.push(Source {
                        url: base.origin().ascii_serialization() + base.path(),
                        kind: "API documentation".into(),
                        content: {
                            let mut doc = compact_schema(doc);
                            doc["documented_header_auth"] = json!(
                                auth_requirements
                                    .iter()
                                    .map(|((method, path), auth)| {
                                        json!({
                                            "method": method,
                                            "path": path,
                                            "auth": auth,
                                            "supported": auth.is_some()
                                        })
                                    })
                                    .collect::<Vec<_>>()
                            );
                            doc
                        },
                    });
                } else {
                    sources.push(Source {
                        url: base.origin().ascii_serialization() + base.path(),
                        kind: "Observed response".into(),
                        content: doc,
                    });
                }
            } else {
                let (found, paths) = if is_js {
                    (BTreeSet::new(), BTreeSet::new())
                } else {
                    observed_links(&base, &text)
                };
                if round == 0 && raw == job.url {
                    *icon_candidates = crate::site_images::candidates(&base, &text);
                    let chosen_scripts: Vec<_> = script_links.iter().take(2).cloned().collect();
                    let published: Vec<_> =
                        crate::discovery::ranked(links.iter().chain(found.iter()))
                            .into_iter()
                            .filter(|u| !chosen_scripts.contains(u))
                            .take(3)
                            .collect();
                    reads.extend(published);
                    reads.extend(chosen_scripts);
                }
                links.extend(found);
                evidence.extend(paths.iter().cloned());
                let mut doc_routes = crate::discovery::documented_routes(&vis_text);
                doc_routes.extend(js_routes);
                let mut content = if is_js {
                    json!({
                        "text": vis_text.chars().take(32_000).collect::<String>(),
                        "documented_routes": doc_routes,
                        "api_bases": crate::discovery::bundle_api_bases(&base, &text)
                    })
                } else {
                    let forms_val = crate::html::forms(&text);
                    if let Some(arr) = forms_val.as_array() {
                        for f in arr {
                            let method = f
                                .get("method")
                                .and_then(Value::as_str)
                                .unwrap_or("GET")
                                .to_ascii_uppercase();
                            let action = f.get("action").and_then(Value::as_str).unwrap_or("");
                            if let Ok(action_url) = base.join(action) {
                                if crate::net::same_site_origin(&action_url, &base) {
                                    let form_path = if action_url.path().is_empty() {
                                        "/".to_string()
                                    } else {
                                        action_url.path().to_string()
                                    };
                                    evidence.insert(form_path.clone());
                                    routes.insert((method.clone(), form_path.clone()));
                                    doc_routes.insert((method, form_path));
                                }
                            }
                        }
                    }
                    let ssr_keys: Vec<_> =
                        crate::html::extract_ssr_map(&text).into_keys().collect();
                    json!({
                        "text": vis_text.chars().take(32_000).collect::<String>(),
                        "documented_routes": doc_routes,
                        "ssr_keys": ssr_keys,
                        "ssr_payloads": crate::html::ssr_payloads(&text),
                        "forms": forms_val,
                        "html_structure": crate::html::structure(&text),
                        "observed_paths": paths.into_iter().take(120).collect::<Vec<_>>()
                    })
                };
                let secrets = if account_id.is_empty() {
                    vec![]
                } else {
                    self.credential(account_id)?.redaction_values()
                };
                util::redact(&mut content, &secrets);
                sources.push(Source {
                    url: base.origin().ascii_serialization() + base.path(),
                    kind: if is_js {
                        "Published JavaScript"
                    } else {
                        "Webpage"
                    }
                    .into(),
                    content,
                });
            }
            if !is_js {
                evidence.insert(base.path().to_string());
            }
            self.job_note(&job.id, "read", "", sources.len(), "")?;
            self.job_progress(&job.id, "connect", 1)?;
            // Promote newly discovered high-priority documentation links without evicting Round 0 JS bundles.
            if round == 0 && round_reads > 1 && round_reads < round_limit {
                let doc_candidates: Vec<_> = crate::discovery::ranked(links.iter().filter(|url| {
                    !seen.contains(*url) && crate::discovery::documentation_url(url)
                }))
                .into_iter()
                .take(2)
                .collect();
                for doc_url in doc_candidates.into_iter().rev() {
                    if !reads.contains(&doc_url) {
                        reads.push_front(doc_url);
                    }
                }
                reads.truncate(round_limit - round_reads);
            }
        }
        for source in sources {
            compact_value(&mut source.content, 0);
        }
        Ok(())
    }
}

/// Documented API operations that no prepared action implements, as "METHOD /path".
pub(super) fn uncovered_routes(
    documented: &BTreeSet<(String, String)>,
    spec: Option<&Spec>,
    prefixes: &BTreeSet<String>,
) -> Vec<String> {
    documented
        .iter()
        .filter(|(method, path)| {
            !spec.is_some_and(|spec| {
                spec.operations.iter().any(|op| {
                    let full = url::Url::parse(spec.operation_base(op))
                        .map(|base| format!("{}{}", base.path().trim_end_matches('/'), op.path))
                        .unwrap_or_default();
                    op.method.eq_ignore_ascii_case(method)
                        && (crate::discovery::route_matches(&op.path, path, prefixes)
                            || crate::discovery::route_matches(&full, path, prefixes))
                })
            })
        })
        .map(|(method, path)| format!("{method} {path}"))
        .collect()
}

pub(super) fn verification_key(
    spec: &Spec,
    operation: &crate::spec::Operation,
) -> AppResult<String> {
    Ok(util::hash(&serde_json::to_vec(&(
        spec.operation_base(operation),
        operation,
        &spec.defaults,
        spec.find_auth(&operation.auth),
    ))?))
}

pub(super) type AuthRequirements = BTreeMap<(String, String), Option<crate::spec::AuthStrategy>>;

// Only structural authentication metadata is retained. Examples, bearer values,
// client secrets and OAuth URLs never enter this descriptor.
pub(super) fn documented_auth(doc: &Value, site: &str) -> AuthRequirements {
    let schemas = doc
        .pointer("/components/securitySchemes")
        .or_else(|| doc.get("securityDefinitions"));
    let mut out = BTreeMap::new();
    let Some(paths) = doc.get("paths").and_then(Value::as_object) else {
        return out;
    };
    for (path, item) in paths {
        for method in ["get", "head", "options", "post", "put", "patch", "delete"] {
            let Some(operation) = item.get(method) else {
                continue;
            };
            let Some(requirements) = operation
                .get("security")
                .or_else(|| doc.get("security"))
                .and_then(Value::as_array)
            else {
                continue;
            };
            if requirements.is_empty()
                || requirements
                    .iter()
                    .any(|value| value.as_object().is_some_and(|o| o.is_empty()))
            {
                continue;
            }
            let strategy = requirements.iter().find_map(|requirement| {
                let requirement = requirement.as_object()?;
                if requirement.len() != 1 {
                    return None;
                } // Multiple simultaneous keys need a richer contract.
                let name = requirement.keys().next()?;
                let schema = schemas?.get(name)?;
                let (header, prefix) = match (
                    schema["type"].as_str(),
                    schema["scheme"].as_str(),
                    schema["in"].as_str(),
                ) {
                    (Some("http"), Some("bearer"), _) => ("Authorization", "Bearer "),
                    (Some("apiKey"), _, Some("header")) => (schema["name"].as_str()?, ""),
                    _ => return None,
                };
                if header.len() > 80
                    || header.is_empty()
                    || !header
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b == b'-')
                    || util::redact_text(header) != header
                {
                    return None;
                }
                let id = format!("website-key-{}", &util::hash(name.as_bytes())[..8]);
                Some(crate::spec::AuthStrategy {
                    name: id.clone(),
                    kind: "header".into(),
                    header: header.into(),
                    prefix: prefix.into(),
                    value_from: format!("store:{site}/{id}"),
                    origin: String::new(),
                })
            });
            out.insert((method.to_ascii_uppercase(), path.clone()), strategy);
        }
    }
    out
}

pub(super) fn apply_documented_auth(spec: &mut Spec, requirements: &AuthRequirements) {
    spec.operations.retain_mut(|operation| {
        if spec.auth.iter().any(|auth| {
            auth.name == operation.auth && matches!(auth.kind.as_str(), "browser" | "derived")
        }) {
            return true; // The broker required a successful read or an observed change with a verified session.
        }
        match requirements.get(&(
            operation.method.to_ascii_uppercase(),
            operation.path.clone(),
        )) {
            Some(Some(auth)) => {
                operation.auth = auth.name.clone();
                if !spec.auth.iter().any(|existing| existing.name == auth.name) {
                    spec.auth.push(auth.clone());
                }
                true
            }
            Some(None) => false, // Do not turn an unsupported protected operation into a public tool.
            None => true,
        }
    });
}

#[allow(clippy::too_many_arguments)]
pub(super) fn filter_worker_spec(
    sp: &mut Spec,
    initial: &url::Url,
    account_id: &str,
    api_bases: &BTreeSet<String>,
    api_prefixes: &BTreeSet<String>,
    routes: &BTreeSet<(String, String)>,
    evidence: &BTreeSet<String>,
    browser_reads: &BTreeSet<String>,
    observed_session_routes: &BTreeSet<(String, String)>,
    auth_requirements: &AuthRequirements,
) {
    let site_base = sp.site.base_url.clone();
    let spec_auth = sp.auth.clone();
    sp.operations.retain_mut(|op| {
        let method = op.method.to_uppercase();
        let path = &op.path;
        let operation_base = if op.base_url.is_empty() {
            &site_base
        } else {
            &op.base_url
        };
        if !api_bases.contains(operation_base.trim_end_matches('/')) {
            return false;
        }
        let is_session_auth = spec_auth.iter().any(|auth| {
            auth.name == op.auth && matches!(auth.kind.as_str(), "browser" | "derived")
        });
        if is_session_auth {
            let has_openapi_req = auth_requirements.contains_key(&(method.clone(), path.clone()));
            let proven_read = browser_reads.contains(&format!(
                "{}{}",
                operation_base.trim_end_matches('/'),
                path
            )) || browser_reads.iter().any(|read_url| {
                url::Url::parse(read_url)
                    .is_ok_and(|u| crate::discovery::route_matches(u.path(), path, api_prefixes))
            });
            let proven_session_route =
                observed_session_routes
                    .iter()
                    .any(|(observed_method, observed_path)| {
                        observed_method == &method
                            && crate::discovery::route_matches(observed_path, path, api_prefixes)
                    });
            if method == "GET" {
                if account_id.is_empty() && !proven_read {
                    return false;
                }
                if has_openapi_req && !proven_read {
                    if auth_requirements
                        .get(&(method.clone(), path.clone()))
                        .is_some_and(|req| req.is_some())
                    {
                        op.auth.clear();
                    } else {
                        return false;
                    }
                }
            } else if account_id.is_empty() || op.transport == "browser" {
                return false;
            } else if has_openapi_req && !proven_session_route {
                if auth_requirements
                    .get(&(method.clone(), path.clone()))
                    .is_some_and(|req| req.is_some())
                {
                    op.auth.clear();
                } else {
                    return false;
                }
            }
        }
        routes.iter().any(|(m, p)| {
            *m == method
                && (crate::discovery::route_matches(path, p, api_prefixes)
                    || url::Url::parse(operation_base).is_ok_and(|base| {
                        crate::discovery::route_matches(
                            &format!("{}{}", base.path().trim_end_matches('/'), path),
                            p,
                            api_prefixes,
                        )
                    }))
        }) || (method == "GET"
            && path != "/"
            && path != initial.path()
            && !crate::discovery::documentation_url(&format!(
                "{}{}",
                initial.origin().ascii_serialization(),
                path
            ))
            && supported_path(path, evidence))
    });
    apply_documented_auth(sp, auth_requirements);
    for auth in &mut sp.auth {
        if auth.kind == "header" && auth.origin.is_empty() {
            let origins: BTreeSet<_> = sp
                .operations
                .iter()
                .filter(|op| op.auth == auth.name)
                .filter_map(|op| {
                    runtime::origin(if op.base_url.is_empty() {
                        &sp.site.base_url
                    } else {
                        &op.base_url
                    })
                    .ok()
                })
                .collect();
            if origins.len() == 1 {
                auth.origin = origins.into_iter().next().unwrap();
            }
        }
    }
}

// Each worker returns a complete spec. Preserve its auth/model references and default
// headers when combining its operations with the other independently valid results.
pub(super) fn merge_worker_spec(
    existing: &mut Spec,
    incoming: Spec,
    role: &str,
) -> AppResult<BTreeMap<String, String>> {
    let mut next = existing.clone();
    let accepted = merge_worker_spec_inner(&mut next, incoming, role)?;
    *existing = next;
    Ok(accepted)
}

pub(super) fn merge_worker_spec_inner(
    existing: &mut Spec,
    mut incoming: Spec,
    role: &str,
) -> AppResult<BTreeMap<String, String>> {
    existing.spec_version = existing.spec_version.max(incoming.spec_version);
    let base = crate::net::validate_url(&existing.site.base_url)?;
    let other = crate::net::validate_url(&incoming.site.base_url)?;
    if base.origin() != other.origin() {
        // Preserve independently evidenced web/API destinations and their credential scope.
        for op in &mut incoming.operations {
            if op.base_url.is_empty() {
                op.base_url = incoming.site.base_url.clone();
            }
        }
        for auth in &mut existing.auth {
            if auth.kind == "header" && auth.origin.is_empty() {
                auth.origin = base.origin().ascii_serialization();
            }
        }
        for auth in &mut incoming.auth {
            if auth.kind == "header" && auth.origin.is_empty() {
                auth.origin = other.origin().ascii_serialization();
            }
        }
    }
    if base.origin() == other.origin()
        && base.path().trim_end_matches('/') != other.path().trim_end_matches('/')
    {
        // Normalize equivalent server prefixes before merging independently authored actions.
        for op in &mut existing.operations {
            if op.base_url.is_empty() {
                op.path = format!("{}{}", base.path().trim_end_matches('/'), op.path);
            }
        }
        for op in &mut incoming.operations {
            if op.base_url.is_empty() {
                op.path = format!("{}{}", other.path().trim_end_matches('/'), op.path);
            }
        }
        existing.site.base_url = base.origin().ascii_serialization();
    }
    let mut accepted = BTreeMap::new();
    for mut operation in incoming.operations {
        let original_name = operation.name.clone();
        let destination = if operation.base_url.is_empty() {
            &existing.site.base_url
        } else {
            &operation.base_url
        };
        if let Some(index) = existing.operations.iter().position(|old| {
            existing.operation_base(old).trim_end_matches('/') == destination.trim_end_matches('/')
                && old.method == operation.method
                && old.path == operation.path
                && (role != "request"
                    || (old.params == operation.params
                        && old.headers == operation.headers
                        && old.transport == operation.transport
                        && old.preflight == operation.preflight
                        && old.response == operation.response
                        && old.model == operation.model
                        && old.effect == operation.effect
                        && existing.find_auth(&old.auth)
                            == incoming
                                .auth
                                .iter()
                                .find(|auth| auth.name == operation.auth)))
                && ((role != "request" && old.name == operation.name)
                    || (old.query == operation.query && old.body == operation.body))
        }) {
            let incoming_browser = incoming.auth.iter().any(|auth| {
                auth.name == operation.auth && matches!(auth.kind.as_str(), "browser" | "derived")
            });
            let old = &existing.operations[index];
            let old_browser = existing.auth.iter().any(|auth| {
                auth.name == old.auth && matches!(auth.kind.as_str(), "browser" | "derived")
            });
            if role != "request"
                && incoming_browser
                && !old_browser
                && old.method == operation.method
                && old.path == operation.path
            {
                // A successfully checked browser/derived read supersedes a provisional API-key
                // version of that same operation, even when its name was already used.
                existing.operations.remove(index);
            } else {
                accepted.insert(original_name, old.name.clone());
                continue;
            }
        }
        if existing
            .operations
            .iter()
            .any(|old| old.name == operation.name)
        {
            operation.name = format!(
                "{role}-{}-{}",
                operation.name,
                &util::hash(
                    if role == "request" {
                        serde_json::to_vec(&(destination, &operation))?
                    } else {
                        format!("{destination}{} {}", operation.method, operation.path).into_bytes()
                    }
                    .as_slice()
                )[..8]
            );
        }
        // Resolve defaults before merging, so one worker's defaults cannot affect another.
        for old in &mut existing.operations {
            for (key, value) in &existing.defaults.headers {
                old.headers.entry(key.clone()).or_insert(value.clone());
            }
        }
        existing.defaults.headers.clear();
        for (key, value) in &incoming.defaults.headers {
            operation
                .headers
                .entry(key.clone())
                .or_insert(value.clone());
        }
        if !operation.auth.is_empty() {
            let auth = incoming
                .auth
                .iter()
                .find(|auth| auth.name == operation.auth)
                .ok_or_else(|| AppError::api("data_invalid", 400))?;
            let mut auth = auth.clone();
            if existing
                .auth
                .iter()
                .any(|old| old.name == auth.name && old != &auth)
            {
                auth.name = format!(
                    "{role}-{}-{}",
                    auth.name,
                    &util::hash(&serde_json::to_vec(&auth)?)[..12]
                );
            }
            operation.auth = auth.name.clone();
            if !existing.auth.iter().any(|old| old.name == auth.name) {
                existing.auth.push(auth);
            }
        }
        if !operation.model.is_empty() {
            let model = incoming
                .models
                .get(&operation.model)
                .ok_or_else(|| AppError::api("data_invalid", 400))?;
            if existing
                .models
                .get(&operation.model)
                .is_some_and(|old| old != model)
            {
                operation.model = format!(
                    "{role}-{}-{}",
                    operation.model,
                    &util::hash(&serde_json::to_vec(model)?)[..12]
                );
            }
            existing
                .models
                .entry(operation.model.clone())
                .or_insert_with(|| model.clone());
        }
        accepted.insert(original_name, operation.name.clone());
        existing.operations.push(operation);
    }
    policy::validate_spec(existing)?;
    Ok(accepted)
}

pub(super) fn accounts_identity(u: &url::Url) -> bool {
    crate::accounts::identity_endpoint(u)
}

pub(super) fn supported_path(path: &str, evidence: &BTreeSet<String>) -> bool {
    if evidence.contains(path) {
        return true;
    }
    let pattern = regex::escape(path);
    let pattern = regex::Regex::new(r"\\\{\\\{[^}]+\\\}\\\}|\\\{[^}]+\\\}")
        .expect("constant")
        .replace_all(&pattern, "[^/]+?");
    regex::Regex::new(&format!("^{pattern}$"))
        .ok()
        .is_some_and(|re| evidence.iter().any(|e| re.is_match(e)))
}

pub(super) fn observed_links(base: &url::Url, text: &str) -> (BTreeSet<String>, BTreeSet<String>) {
    let links = crate::discovery::published_links(base, text);
    let paths = links
        .iter()
        .filter_map(|raw| url::Url::parse(raw).ok())
        .filter(|url| crate::net::same_site_origin(url, base))
        .map(|url| url.path().into())
        .collect();
    (links, paths)
}

pub(super) fn compact_schema(mut value: Value) -> Value {
    if let Some(o) = value.as_object_mut() {
        o.remove("examples");
    }
    if value.to_string().len() > 140_000 {
        if let Some(paths) = value.get_mut("paths").and_then(Value::as_object_mut) {
            while paths.len() > 40 {
                if let Some(key) = paths.keys().next_back().cloned() {
                    paths.remove(&key);
                } else {
                    break;
                }
            }
        }
    }
    value
}

pub(super) fn compact_value(value: &mut Value, depth: usize) {
    if depth > 14 {
        *value = json!("[nested structure]");
        return;
    }
    match value {
        Value::String(text) => {
            if text.chars().count() > 14000 {
                *text = text.chars().take(14000).collect::<String>() + "…";
            }
        }
        Value::Array(items) => {
            items.truncate(80);
            for item in items {
                compact_value(item, depth + 1);
            }
        }
        Value::Object(object) => {
            if object.len() > 100 {
                let remove: Vec<_> = object.keys().skip(100).cloned().collect();
                for key in remove {
                    object.remove(&key);
                }
            }
            for child in object.values_mut() {
                compact_value(child, depth + 1);
            }
        }
        _ => {}
    }
}
