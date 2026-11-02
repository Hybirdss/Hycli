use crate::{
    apperr::AppResult,
    kb::AttemptInput,
    policy::{self, Effect},
    spec::{self, Spec},
    util,
};
use serde_json::{Value, json};
use std::collections::BTreeMap;

use super::{Runtime, humanize, sanitized, site_id_for, visible_text};

impl Runtime {
    /// Deterministic fast recon for external agents (CLI / MCP) without waiting on an AI loop.
    pub async fn recon(
        &self,
        site: &str,
        url: &str,
        max_bundles: usize,
    ) -> AppResult<(Value, Spec)> {
        self.recon_scoped(site, url, max_bundles, None).await
    }
    pub(crate) async fn recon_for_job(
        &self,
        job: &crate::state::Job,
        url: &str,
        max_bundles: usize,
    ) -> AppResult<(Value, Spec)> {
        self.recon_scoped(&job.site_id, url, max_bundles, Some(job))
            .await
    }
    async fn recon_scoped(
        &self,
        site: &str,
        url: &str,
        max_bundles: usize,
        job: Option<&crate::state::Job>,
    ) -> AppResult<(Value, Spec)> {
        let parsed = crate::net::validate_url(url)?;
        let site_id = if site.is_empty() || site == "default" {
            site_id_for(&parsed)
        } else {
            site.to_string()
        };
        let mut scope = job
            .cloned()
            .unwrap_or_else(|| crate::state::Job::new("recon", "en"));
        if job.is_none() {
            scope.site_id = site_id.clone();
            scope.url = url.into();
        }
        if scope.account_selection.is_none() {
            scope.account_selection =
                Some(self.preparation_selection_snapshot(&scope.site_id, &scope.url)?);
        }
        let selection = scope.account_selection.as_ref().unwrap();
        let data = self.state.read()?;
        let account_id = if !selection.revision.is_empty() {
            selection.account_id.clone()
        } else {
            data.sites
                .get(&site_id)
                .map(|m| &m.account_id)
                .filter(|id| {
                    data.accounts
                        .get(*id)
                        .is_some_and(|a| a.site_id == site_id && a.status == "connected")
                })
                .cloned()
                .unwrap_or_default()
        };
        let credential = if account_id.is_empty() {
            None
        } else {
            Some(self.job_credential(&scope, &account_id, &scope.url)?)
        };
        let check = || {
            self.ensure_job_credential_snapshot(
                &scope,
                &account_id,
                &scope.url,
                credential.as_ref(),
            )
        };
        let entry_account = if super::origin(url)? == super::origin(&scope.url)? {
            account_id.as_str()
        } else {
            ""
        };
        let r = self
            .read_page_checked(
                url,
                entry_account,
                "go",
                if entry_account.is_empty() {
                    None
                } else {
                    credential.as_ref()
                },
                &check,
            )
            .await?;
        let _ = self.kb.record_attempt(&AttemptInput {
            site: site_id.clone(),
            stage: "recon".into(),
            action: "recon_entry".into(),
            target_kind: "url".into(),
            target_key: r.url.clone(),
            status: r.status as i64,
            defense: r.defense.clone(),
            duration_ms: r.duration_ms,
            verdict: if r.successful() { "pass" } else { "fail" }.into(),
            ..Default::default()
        });
        let r_url = crate::net::validate_url(&r.url)?;
        let text = String::from_utf8_lossy(&r.bytes);
        let vis = visible_text(&text);
        let page_title = crate::html::Field {
            selector: "title".into(),
            attribute: String::new(),
        }
        .extract(&scraper::Html::parse_document(&text))
        .unwrap_or_default();
        let ssr_payloads = crate::html::ssr_payloads(&text);
        let ssr_map = crate::html::extract_ssr_map(&text);
        let doc_routes = crate::discovery::documented_routes(&vis);
        let mut bundle_routes = crate::discovery::bundle_routes(&text);
        let mut api_bases = crate::discovery::bundle_api_bases(&r_url, &text);
        let max_bundles = max_bundles.clamp(1, 24);
        let mut script_urls: Vec<String> = crate::discovery::bundle_urls(&r_url, &text)
            .into_iter()
            .collect();
        script_urls.sort_by_key(|u| {
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
        let mut scanned_bundles = Vec::new();
        for bundle_url in script_urls.into_iter().take(max_bundles) {
            let Ok(parsed_bundle) = crate::net::validate_url(&bundle_url) else {
                continue;
            };
            check()?;
            let bundle_account =
                if parsed_bundle.origin().ascii_serialization() == super::origin(&scope.url)? {
                    account_id.as_str()
                } else {
                    ""
                };
            if let Ok(br) = self
                .read_page_checked(
                    &bundle_url,
                    bundle_account,
                    "go",
                    if bundle_account.is_empty() {
                        None
                    } else {
                        credential.as_ref()
                    },
                    &check,
                )
                .await
            {
                if br.successful() {
                    let btext = String::from_utf8_lossy(&br.bytes);
                    for route in crate::discovery::bundle_routes(&btext) {
                        if bundle_routes.len() < 96 {
                            bundle_routes.insert(route);
                        }
                    }
                    for base in crate::discovery::bundle_api_bases(&r_url, &btext) {
                        if api_bases.len() < 16 {
                            api_bases.insert(base);
                        }
                    }
                    scanned_bundles.push(bundle_url);
                }
            }
        }
        let origin_str = r_url.origin().ascii_serialization();
        let entry_path = if r_url.path().is_empty() {
            "/".to_string()
        } else {
            r_url.path().to_string()
        };
        let mut entry_query = BTreeMap::new();
        for (k, _) in r_url.query_pairs() {
            if util::valid_id(&k) && !util::secret_field(&k) {
                entry_query.insert(
                    k.into_owned(),
                    spec::Param {
                        kind: "string".into(),
                        required: false,
                        default: None,
                    },
                );
            }
        }
        if (entry_path.contains("search") || entry_path.contains("buy-sell"))
            && !entry_query.contains_key("q")
        {
            entry_query.insert(
                "q".into(),
                spec::Param {
                    kind: "string".into(),
                    required: false,
                    default: None,
                },
            );
        }
        let primary_base = api_bases
            .iter()
            .find(|b| {
                url::Url::parse(b)
                    .ok()
                    .and_then(|u| u.host_str().map(|h| h.to_ascii_lowercase()))
                    .is_some_and(|h| {
                        [
                            "api.",
                            "api-",
                            "v1.",
                            "v2.",
                            "v3.",
                            "gql.",
                            "graphql.",
                            "gateway.",
                            "backend.",
                            "studio-api",
                        ]
                        .iter()
                        .any(|p| h.starts_with(p))
                    })
            })
            .cloned()
            .unwrap_or_else(|| origin_str.clone());
        let primary_ssr = crate::html::SSR_GLOBAL_KEYS
            .iter()
            .copied()
            .find(|k| ssr_map.contains_key(*k))
            .unwrap_or("");

        let mut form_query_map: BTreeMap<String, BTreeMap<String, spec::Param>> = BTreeMap::new();
        let mut form_body_map: BTreeMap<(String, String), BTreeMap<String, spec::Param>> =
            BTreeMap::new();
        if let Some(forms_arr) = crate::html::forms(&text).as_array() {
            for f in forms_arr {
                let method = f
                    .get("method")
                    .and_then(Value::as_str)
                    .unwrap_or("GET")
                    .to_ascii_uppercase();
                if !["GET", "POST", "PUT", "PATCH", "DELETE"].contains(&method.as_str()) {
                    continue;
                }
                let action = f.get("action").and_then(Value::as_str).unwrap_or("");
                let Ok(action_url) = r_url.join(action) else {
                    continue;
                };
                if action_url.origin() != r_url.origin() {
                    continue;
                }
                let mut qmap = BTreeMap::new();
                if let Some(inputs) = f.get("inputs").and_then(Value::as_array) {
                    for inp in inputs {
                        if let Some(name) = inp.get("name").and_then(Value::as_str) {
                            if util::valid_id(name) && !util::secret_field(name) {
                                qmap.insert(
                                    name.to_string(),
                                    spec::Param {
                                        kind: "string".into(),
                                        required: false,
                                        default: None,
                                    },
                                );
                            }
                        }
                    }
                }
                let clean_path = action_url.path().trim_end_matches('/');
                let norm_path = if clean_path.is_empty() {
                    "/".to_string()
                } else {
                    clean_path.to_string()
                };
                if method == "GET" {
                    form_query_map
                        .entry(norm_path.clone())
                        .or_default()
                        .extend(qmap.clone());
                    form_query_map
                        .entry(action_url.path().to_string())
                        .or_default()
                        .extend(qmap);
                } else {
                    bundle_routes.insert((method.clone(), norm_path.clone()));
                    form_body_map
                        .entry((method.clone(), norm_path))
                        .or_default()
                        .extend(qmap.clone());
                    form_body_map
                        .entry((method, action_url.path().to_string()))
                        .or_default()
                        .extend(qmap);
                }
            }
        }
        if let Some(ld) = ssr_map.get("ld+json") {
            let tpl_opt = ld
                .pointer("/potentialAction/target/urlTemplate")
                .or_else(|| ld.pointer("/potentialAction/0/target/urlTemplate"))
                .and_then(Value::as_str);
            if let Some(tpl) = tpl_opt {
                let safe_tpl = tpl.replace('{', "_").replace('}', "_");
                if let Ok(tpl_url) = r_url.join(&safe_tpl) {
                    if tpl_url.origin() == r_url.origin() {
                        let mut qmap = BTreeMap::new();
                        for (k, _) in tpl_url.query_pairs() {
                            if util::valid_id(&k) && !util::secret_field(&k) {
                                qmap.insert(
                                    k.into_owned(),
                                    spec::Param {
                                        kind: "string".into(),
                                        required: false,
                                        default: None,
                                    },
                                );
                            }
                        }
                        let clean_path = tpl_url.path().trim_end_matches('/');
                        let norm_path = if clean_path.is_empty() {
                            "/".to_string()
                        } else {
                            clean_path.to_string()
                        };
                        form_query_map
                            .entry(norm_path)
                            .or_default()
                            .extend(qmap.clone());
                        form_query_map
                            .entry(tpl_url.path().to_string())
                            .or_default()
                            .extend(qmap);
                    }
                }
            }
        }

        let inferred_html = crate::html::infer_html_extraction(&text, &r.url);
        let mut operations = Vec::new();
        let mut used_names = std::collections::BTreeSet::new();
        if primary_ssr.is_empty() {
            if let Some(ref ext) = inferred_html {
                used_names.insert("read-entry".into());
                operations.push(spec::Operation {
                    name: "read-entry".into(),
                    method: "GET".into(),
                    path: entry_path.clone(),
                    base_url: origin_str.clone(),
                    query: entry_query.clone(),
                    desc: "Extract structured item list from entry page".into(),
                    effect: Effect::Read,
                    evidence: format!("Inferred repeating DOM item selector `{}`", ext.items),
                    response: Some(spec::ResponseExpectation {
                        format: "html".into(),
                        html: Some(ext.clone()),
                        ..Default::default()
                    }),
                    ..Default::default()
                });
                for nav_path in crate::html::nav_list_routes(&text, &r.url) {
                    let slug = nav_path
                        .trim_matches('/')
                        .replace(|c: char| !c.is_ascii_alphanumeric(), "-")
                        .to_ascii_lowercase();
                    let op_name = format!("get-{slug}");
                    if util::valid_id(&op_name) && used_names.insert(op_name.clone()) {
                        operations.push(spec::Operation {
                            name: op_name,
                            method: "GET".into(),
                            path: nav_path.clone(),
                            base_url: origin_str.clone(),
                            desc: format!("Extract structured item list from {nav_path}"),
                            effect: Effect::Read,
                            evidence: format!(
                                "Observed same-origin navigation route {nav_path} sharing `{}` item layout",
                                ext.items
                            ),
                            response: Some(spec::ResponseExpectation {
                                format: "html".into(),
                                html: Some(ext.clone()),
                                ..Default::default()
                            }),
                            ..Default::default()
                        });
                    }
                }
            }
        }
        for ssr_key in crate::html::SSR_GLOBAL_KEYS
            .iter()
            .copied()
            .chain(std::iter::once("ld+json"))
        {
            if let Some(doc) = ssr_map.get(ssr_key) {
                let slug = if ssr_key == "__remixContext" {
                    "remix-context".to_string()
                } else {
                    ssr_key
                        .trim_matches('_')
                        .to_ascii_lowercase()
                        .replace(|c: char| !c.is_ascii_alphanumeric(), "-")
                };
                let op_name = format!("read-{slug}");
                if util::valid_id(&op_name) && used_names.insert(op_name.clone()) {
                    let required_pointers = if doc.get("props").is_some() {
                        vec!["/props".to_string()]
                    } else {
                        vec![]
                    };
                    operations.push(spec::Operation {
                        name: op_name,
                        method: "GET".into(),
                        path: entry_path.clone(),
                        base_url: origin_str.clone(),
                        query: entry_query.clone(),
                        desc: format!("Extract embedded {ssr_key} SSR state from entry page"),
                        effect: Effect::Read,
                        evidence: format!("Observed {ssr_key} payload in entry HTML"),
                        response: Some(spec::ResponseExpectation {
                            format: "json".into(),
                            ssr_json: ssr_key.into(),
                            required_pointers,
                            ..Default::default()
                        }),
                        ..Default::default()
                    });
                }
            }
        }
        if ssr_map.contains_key("__remixContext")
            && (text.contains("/kr/buy-sell") || text.contains("/kr/search"))
            && used_names.insert("search-buy-sell".into())
        {
            operations.push(spec::Operation {
                name: "search-buy-sell".into(),
                method: "GET".into(),
                path: "/kr/search/".into(),
                base_url: origin_str.clone(),
                query: BTreeMap::from([
                    (
                        "q".into(),
                        spec::Param {
                            kind: "string".into(),
                            required: false,
                            default: None,
                        },
                    ),
                    (
                        "in".into(),
                        spec::Param {
                            kind: "string".into(),
                            required: false,
                            default: None,
                        },
                    ),
                ]),
                desc: "Search secondhand buy-sell listings".into(),
                effect: Effect::Read,
                evidence: "Observed /kr/search/ Remix route and __remixContext in HTML".into(),
                response: Some(spec::ResponseExpectation {
                    format: "json".into(),
                    ssr_json: "__remixContext".into(),
                    extract_pointer: "/routeData/buySellArticles".into(),
                    ..Default::default()
                }),
                ..Default::default()
            });
        }
        let passive_observations = self
            .state
            .read()
            .ok()
            .and_then(|d| d.sites.get(&site_id).map(|m| m.observations.clone()))
            .unwrap_or_default();
        for obs in &passive_observations {
            let method = obs.method.to_ascii_uppercase();
            if !["GET", "POST", "PUT", "PATCH", "DELETE"].contains(&method.as_str())
                || !obs.path.starts_with('/')
            {
                continue;
            }
            let slug = obs
                .path
                .trim_matches('/')
                .split('/')
                .filter(|seg| !seg.is_empty() && !seg.starts_with('{'))
                .collect::<Vec<_>>()
                .join("-")
                .replace(|c: char| !c.is_ascii_alphanumeric(), "-")
                .to_ascii_lowercase();
            let op_name = format!(
                "{}-{}",
                method.to_ascii_lowercase(),
                if slug.is_empty() { "root" } else { &slug }
            )
            .chars()
            .take(48)
            .collect::<String>()
            .trim_end_matches('-')
            .to_string();
            if !util::valid_id(&op_name) || !used_names.insert(op_name.clone()) {
                continue;
            }
            let query: BTreeMap<String, spec::Param> = obs
                .query
                .iter()
                .filter(|(k, _)| util::valid_id(k) && !util::secret_field(k))
                .map(|(k, kind)| {
                    let norm_kind = match kind.as_str() {
                        "int" | "float" | "bool" | "json" => kind.as_str(),
                        "number" => "float",
                        _ => "string",
                    };
                    (
                        k.clone(),
                        spec::Param {
                            kind: norm_kind.into(),
                            required: false,
                            default: None,
                        },
                    )
                })
                .collect();
            let body = if !obs.body.is_empty() && method != "GET" {
                let mut json_fields = BTreeMap::new();
                for (k, kind) in &obs.body {
                    if util::valid_id(k) && !util::secret_field(k) && !query.contains_key(k) {
                        let norm_kind = match kind.as_str() {
                            "int" | "float" | "bool" | "json" => kind.as_str(),
                            "number" => "float",
                            "array" | "object" => "json",
                            _ => "string",
                        };
                        json_fields.insert(
                            k.clone(),
                            spec::Param {
                                kind: norm_kind.into(),
                                required: true,
                                default: None,
                            },
                        );
                    }
                }
                if json_fields.is_empty() {
                    None
                } else {
                    Some(spec::Body {
                        json: json_fields,
                        ..Default::default()
                    })
                }
            } else {
                None
            };
            let is_read = method == "GET" && !policy::dangerous_intent(&obs.path);
            operations.push(spec::Operation {
                name: op_name,
                method: method.clone(),
                path: obs.path.clone(),
                query,
                body,
                desc: format!("{method} {}", obs.path),
                effect: if is_read { Effect::Read } else { Effect::Write },
                evidence: format!(
                    "Observed live browser network request ({} {})",
                    method, obs.path
                ),
                response: Some(spec::ResponseExpectation {
                    format: "json".into(),
                    required_pointers: obs.response_pointers.clone(),
                    ..Default::default()
                }),
                ..Default::default()
            });
            if operations.len() >= 24 {
                break;
            }
        }
        for (method, path) in doc_routes.iter().chain(bundle_routes.iter()) {
            let slug = path
                .trim_matches('/')
                .split('/')
                .filter(|seg| !seg.is_empty() && !seg.starts_with('{'))
                .collect::<Vec<_>>()
                .join("-")
                .replace(|c: char| !c.is_ascii_alphanumeric(), "-")
                .to_ascii_lowercase();
            let op_name = format!(
                "{}-{}",
                method.to_ascii_lowercase(),
                if slug.is_empty() { "root" } else { &slug }
            )
            .chars()
            .take(48)
            .collect::<String>()
            .trim_end_matches('-')
            .to_string();
            if !util::valid_id(&op_name) || !used_names.insert(op_name.clone()) {
                continue;
            }
            let route_key = format!("{method} {path}");
            let _ = self.kb.ensure_node(
                &site_id,
                "endpoint",
                &route_key,
                &json!({"method": method, "path": path}),
            );
            let mut params = BTreeMap::new();
            for seg in path.split('/') {
                if let Some(var) = seg.strip_prefix('{').and_then(|s| s.strip_suffix('}')) {
                    if util::valid_id(var) {
                        params.insert(
                            var.to_string(),
                            spec::Param {
                                kind: "string".into(),
                                required: true,
                                default: None,
                            },
                        );
                    }
                }
            }
            let mut query = if method == "GET" {
                form_query_map.get(path).cloned().unwrap_or_default()
            } else {
                BTreeMap::new()
            };
            if method == "GET" && path.contains("search") && !query.contains_key("q") {
                query.insert(
                    "q".into(),
                    spec::Param {
                        kind: "string".into(),
                        required: false,
                        default: None,
                    },
                );
            }
            let body = if method != "GET" {
                form_body_map
                    .get(&(method.clone(), path.clone()))
                    .filter(|fields| !fields.is_empty())
                    .map(|fields| spec::Body {
                        form: fields.clone(),
                        ..Default::default()
                    })
            } else {
                None
            };
            let is_api_route = path.starts_with("/api/")
                || path.starts_with("/v1/")
                || path.starts_with("/v2/")
                || path.starts_with("/v3/")
                || path.contains("graphql")
                || path.ends_with("/gql")
                || path.starts_with("/rest/")
                || path.starts_with("/_next/data/")
                || path.ends_with(".json")
                || doc_routes.contains(&(method.clone(), path.clone()));
            let is_read =
                ["GET", "HEAD"].contains(&method.as_str()) && !policy::dangerous_intent(path);
            let response = if is_api_route {
                spec::ResponseExpectation {
                    format: "json".into(),
                    ..Default::default()
                }
            } else if !primary_ssr.is_empty() {
                spec::ResponseExpectation {
                    format: "json".into(),
                    ssr_json: primary_ssr.into(),
                    ..Default::default()
                }
            } else {
                spec::ResponseExpectation {
                    format: "text".into(),
                    ..Default::default()
                }
            };
            operations.push(spec::Operation {
                name: op_name,
                method: method.to_string(),
                path: path.to_string(),
                base_url: if path.starts_with("/api/") && primary_base != origin_str {
                    primary_base.clone()
                } else {
                    String::new()
                },
                params,
                query,
                body,
                desc: route_key.clone(),
                effect: if is_read { Effect::Read } else { Effect::Write },
                evidence: format!("Discovered {route_key} during deterministic recon"),
                response: Some(response),
                ..Default::default()
            });
            if operations.len() >= 24 {
                break;
            }
        }
        if !used_names.contains("read-entry") && (operations.is_empty() || primary_ssr.is_empty()) {
            operations.push(spec::Operation {
                name: "read-entry".into(),
                method: "GET".into(),
                path: entry_path,
                query: entry_query,
                desc: "Read entry page".into(),
                effect: Effect::Read,
                evidence: "Observed entry page during recon".into(),
                response: Some(spec::ResponseExpectation {
                    format: if r.header("content-type").contains("json") {
                        "json"
                    } else {
                        "text"
                    }
                    .into(),
                    ..Default::default()
                }),
                ..Default::default()
            });
        }
        let draft = Spec {
            spec_version: 2,
            site: spec::SiteInfo {
                name: site_id.clone(),
                title: if page_title.is_empty() {
                    humanize(&site_id)
                } else {
                    page_title.clone()
                },
                base_url: origin_str.clone(),
                source_url: r.url.clone(),
            },
            defaults: spec::Defaults {
                tls_profile: "go".into(),
                ..Default::default()
            },
            auth: vec![],
            models: BTreeMap::new(),
            operations,
            health: None,
        };
        let doc_route_strs: Vec<String> =
            doc_routes.iter().map(|(m, p)| format!("{m} {p}")).collect();
        let bundle_route_strs: Vec<String> = bundle_routes
            .iter()
            .map(|(m, p)| format!("{m} {p}"))
            .collect();
        check()?;
        let secrets = credential
            .as_ref()
            .map(|c| c.redaction_values())
            .unwrap_or_default();
        let report = sanitized(
            json!({
                "site_id": site_id,
                "url": r.url,
                "status": r.status,
                "defense": r.defense,
                "latency_ms": r.duration_ms,
                "title": page_title,
                "account_id": account_id,
                "api_bases": api_bases,
                "documented_routes": doc_route_strs,
                "bundle_routes": bundle_route_strs,
                "scanned_bundles": scanned_bundles,
                "ssr_payloads": ssr_payloads,
                "html_extraction": inferred_html,
                "forms": crate::html::forms(&text),
                "draft_operations": draft.operations.iter().map(|o| json!({
                    "name": o.name,
                    "method": o.method,
                    "path": o.path,
                    "base_url": draft.operation_base(o),
                })).collect::<Vec<_>>(),
            }),
            &secrets,
        );
        Ok((report, draft))
    }
}
