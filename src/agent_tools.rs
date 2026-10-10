//! Provider-neutral preparation tools. The model chooses actions; the broker owns execution.
use crate::{
    apperr::{AppError, AppResult},
    environment::Environment,
    runtime::Runtime,
    state::Job,
    util,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

#[derive(Clone, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct ToolCall {
    pub tool: String,
    pub arguments: Value,
}

pub struct Context<'a> {
    pub job: &'a Job,
    pub environment: &'a Environment,
    pub links: &'a BTreeSet<String>,
    pub routes: &'a BTreeSet<(String, String)>,
    pub prefixes: &'a BTreeSet<String>,
    pub documents: &'a BTreeMap<String, String>,
}

#[derive(Default)]
pub struct Outcome {
    pub content: Value,
    pub read_url: Option<String>,
    pub links: BTreeSet<String>,
    pub routes: BTreeSet<(String, String)>,
    pub document: Option<(String, String)>,
    pub environment: Option<Environment>,
    pub browser_read: Option<String>,
}

pub fn catalog(environment: &Environment) -> Value {
    json!([
        {"tool":"environment.inspect","arguments":{},"purpose":"Refresh the actual OS, installed/running browsers, registered profiles and available capabilities. Paths are private handles."},
        {"tool":"web.read","arguments":{"url":"published URL"},"purpose":"Read one evidenced page, reference or script. A failed read is evidence: choose another source or browser rendering, rather than repeat it."},
        {"tool":"evidence.search","arguments":{"source_url":"previously read URL","query":"heading or relevant text","offset":0},"purpose":"Retrieve a relevant section from a large source, including content beyond the initial excerpt. Empty query uses character offset."},
        {"tool":"session.import","arguments":{"profile_id":"profile handle from environment"},"purpose":"Import only this website's cookies and origin storage from one discovered profile. Cookie presence is not verified login."},
        {"tool":"session.inspect","arguments":{"account_id":"candidate account handle","storage_key":"optional exact key to inspect its JSON shape","offset":0},"purpose":"List local key names and types, never values. Select storage_key to inspect nested JSON structure for a connection recipe. Use next_offset to read more keys."},
        {"tool":"session.observe","arguments":{"account_id":"candidate account handle","url":"published or observed same-origin GET URL"},"purpose":"Inspect a candidate session before identity verification: returns actual status, JSON structure or selectable HTML fields, and published links without private field values. Use this on the website entry or observed account page to construct a JSON or HTML identity recipe. Does not mark the account or operation verified. No browser engine required."},
        {"tool":"session.verify","arguments":{"account_id":"candidate account handle","recipe":{"identity_url":"evidenced current-account GET URL","headers":[{"header":"Authorization","source":"local_storage|cookie|captured_header","key":"observed local key","json_pointer":"optional JSON pointer","prefix":"documented prefix or empty string"}],"identity":{"user_id":"/id","email":"/email","name":"/name"},"identity_html":{"user_id":{"selector":"observed CSS selector","attribute":"observed attribute or empty for text"},"email":{"selector":"optional observed selector","attribute":"value"},"name":{"selector":"optional observed selector","attribute":""}},"evidence":"exact supporting source URL"}},"purpose":"Verify the current account against an anonymous baseline. For JSON use identity pointers and omit identity_html. For HTML use identity_html fields observed by session.observe or browser.render, and empty headers for cookie authentication. Each selector must match one nonempty value, and anonymous access must not expose an identity. Never infer login from a public profile or cookie alone. No browser engine required."},
        {"tool":"session.read","arguments":{"account_id":"verified account handle","url":"evidenced GET URL"},"purpose":"Check whether this verified browser session can actually read an evidenced route. Returns status and JSON structure without private leaf values. On success a matching operation may use auth kind=browser. A browser session never configures a separate API or bot key."},
        {"tool":"browser.render","available":environment.browsers.iter().any(|b| b.protocol=="chromium-cdp"),"arguments":{"url":"published page or app entry URL","engine_id":"available browser handle","account_id":"optional candidate account handle"},"purpose":"Render a page with a discovered engine, optionally restore this site's session, and observe read requests and account context. No clicks, form submission or test writes. Returns HTML text, published links, request shapes and observed authentication header names."}
    ])
}

fn argument<'a>(call: &'a ToolCall, key: &str) -> &'a str {
    call.arguments
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or("")
}

impl Context<'_> {
    fn url(&self, raw: &str) -> AppResult<url::Url> {
        let parsed = crate::net::validate_url(raw)?;
        if raw != self.job.url && !self.links.contains(raw) && !self.documents.contains_key(raw) {
            return Err(AppError::api("unsupported_evidence", 422));
        }
        Ok(parsed)
    }
    fn account(&self, core: &Runtime, id: &str) -> AppResult<()> {
        let view = core
            .state
            .read()?
            .accounts
            .get(id)
            .cloned()
            .ok_or_else(|| AppError::api("not_found", 404))?;
        if view.site_id != self.job.site_id
            || view.site_url != crate::runtime::origin(&self.job.url)?
        {
            return Err(AppError::api("unsafe_target", 400));
        }
        Ok(())
    }
}

impl Runtime {
    pub(crate) async fn preparation_tool(
        self: &Arc<Self>,
        call: &ToolCall,
        context: &Context<'_>,
    ) -> AppResult<Outcome> {
        let mut outcome = Outcome::default();
        match call.tool.as_str() {
            "environment.inspect" => {
                let environment = tokio::task::spawn_blocking(crate::environment::inspect)
                    .await
                    .map_err(|_| AppError::api("internal", 500))?;
                outcome.content = json!(environment);
                outcome.environment = Some(environment);
            }
            "web.read" => {
                let url = context.url(argument(call, "url"))?;
                outcome.read_url = Some(url.to_string());
                outcome.content = json!({"queued":true,"url":url.as_str()});
            }
            "evidence.search" => {
                let source = argument(call, "source_url");
                let document = context
                    .documents
                    .get(source)
                    .ok_or_else(|| AppError::api("unsupported_evidence", 422))?;
                let query = argument(call, "query");
                let offset = call
                    .arguments
                    .get("offset")
                    .and_then(Value::as_u64)
                    .unwrap_or(0) as usize;
                let start = if query.is_empty() {
                    offset
                } else {
                    let lower = document.to_lowercase();
                    lower
                        .find(&query.to_lowercase())
                        .map(|at| lower[..at].chars().count().saturating_sub(1200))
                        .unwrap_or(offset)
                };
                outcome.content = json!({"source_url":source,"offset":start,"text":document.chars().skip(start).take(18_000).collect::<String>(),"total_characters":document.chars().count()});
            }
            "session.import" => {
                let profile = argument(call, "profile_id");
                if !context.environment.profiles.iter().any(|p| p.id == profile) {
                    return Err(AppError::api("browser_unavailable", 422));
                }
                let site = if self.spec(&context.job.site_id).is_ok() {
                    context.job.site_id.as_str()
                } else {
                    ""
                };
                let search = self
                    .import_browser_accounts(
                        site,
                        &context.job.url,
                        profile,
                        false,
                        Some(context.environment.profiles.clone()),
                    )
                    .await?;
                let imported: Vec<_> = search
                    .accounts
                    .iter()
                    .filter(|account| {
                        self.credential(&account.id)
                            .is_ok_and(|credential| credential.source_profile == profile)
                    })
                    .cloned()
                    .collect();
                outcome.content = json!({"profile_id":profile,"profiles_checked":search.profiles_checked,"profiles_unavailable":search.profiles_unavailable,"accounts":imported,"needs_choice":search.needs_choice,"login_url":search.login_url});
            }
            "session.inspect" => {
                let account = argument(call, "account_id");
                context.account(self, account)?;
                outcome.content = self.inspect_session_key(
                    account,
                    &crate::runtime::origin(&context.job.url)?,
                    argument(call, "storage_key"),
                    call.arguments
                        .get("offset")
                        .and_then(Value::as_u64)
                        .unwrap_or(0) as usize,
                )?;
            }
            "session.observe" => {
                let account = argument(call, "account_id");
                context.account(self, account)?;
                let url = crate::net::validate_url(argument(call, "url"))?;
                if url.origin().ascii_serialization() != crate::runtime::origin(&context.job.url)?
                    || (context.url(url.as_str()).is_err()
                        && !context.routes.iter().any(|(method, path)| {
                            method == "GET"
                                && crate::discovery::route_matches(
                                    url.path(),
                                    path,
                                    context.prefixes,
                                )
                        }))
                {
                    return Err(AppError::api("unsupported_evidence", 422));
                }
                let response = self.read_page(url.as_str(), account).await?;
                let text = String::from_utf8_lossy(&response.bytes);
                let body = response.json();
                let html = response.header("content-type").contains("html");
                outcome.content = json!({"url":response.url,"status":response.status,"successful":response.successful(),"format":if body.is_some(){"json"}else if html{"html"}else{"text"},"response_shape":body.as_ref().map(|body|crate::session_recipe::shape(body,0)),"html_structure":if html{crate::html::structure(&text)}else{json!([])},"html_fields":if html{crate::html::fields(&text)}else{json!([])},"values_exposed":false});
                if response.successful() {
                    let final_url = crate::net::validate_url(&response.url)?;
                    outcome
                        .routes
                        .insert(("GET".into(), final_url.path().into()));
                    outcome.links = crate::discovery::published_links(&final_url, &text);
                    outcome.document = Some((response.url.clone(), outcome.content.to_string()));
                }
            }
            "session.read" => {
                let account = argument(call, "account_id");
                context.account(self, account)?;
                if !self
                    .state
                    .read()?
                    .accounts
                    .get(account)
                    .is_some_and(|view| view.verified_identity && view.status == "connected")
                {
                    return Err(AppError::api("auth_required", 401));
                }
                let url = crate::net::validate_url(argument(call, "url"))?;
                if url.origin().ascii_serialization() != crate::runtime::origin(&context.job.url)?
                    || (context.url(url.as_str()).is_err()
                        && !context.routes.iter().any(|(method, path)| {
                            method == "GET"
                                && crate::discovery::route_matches(
                                    url.path(),
                                    path,
                                    context.prefixes,
                                )
                        }))
                {
                    return Err(AppError::api("unsupported_evidence", 422));
                }
                let response = self.read_page(url.as_str(), account).await?;
                let json_body = response.json();
                if matches!(response.status, 401 | 403) && response.defense == "none" {
                    // A route may require a different credential. Recheck the known
                    // identity before treating that denial as loss of the whole session.
                    let _ = self.refresh_account(account).await;
                }
                if response.successful() {
                    outcome.browser_read = Some(url.to_string());
                    outcome.routes.insert(("GET".into(), url.path().into()));
                }
                outcome.content = json!({"url":url.as_str(),"status":response.status,"successful":response.successful(),"format":if json_body.is_some(){"json"}else if response.header("content-type").contains("html"){"html"}else{"text"},"response_shape":json_body.as_ref().map(|body|crate::session_recipe::shape(body,0)),"html_structure":if response.header("content-type").contains("html"){crate::html::structure(&String::from_utf8_lossy(&response.bytes))}else{json!([])},"html_fields":if response.header("content-type").contains("html"){crate::html::fields(&String::from_utf8_lossy(&response.bytes))}else{json!([])},"authentication":"browser","values_exposed":false});
            }
            "session.verify" => {
                let account = argument(call, "account_id");
                context.account(self, account)?;
                let recipe: crate::session_recipe::ConnectionRecipe = serde_json::from_value(
                    call.arguments.get("recipe").cloned().unwrap_or(Value::Null),
                )
                .map_err(|_| AppError::api("data_invalid", 400))?;
                let identity = crate::net::validate_url(&recipe.identity_url)?;
                let supported = context.routes.iter().any(|(method, path)| {
                    method == "GET"
                        && crate::discovery::route_matches(identity.path(), path, context.prefixes)
                });
                if !supported
                    || (!context.documents.contains_key(&recipe.evidence)
                        && recipe.evidence != "observed_browser_request")
                {
                    return Err(AppError::api("unsupported_evidence", 422));
                }
                let account = self.verify_session_recipe(account, &recipe).await?;
                outcome.browser_read = Some(recipe.identity_url.clone());
                outcome.content = json!({"account":account,"verified":true});
            }
            "browser.render" => {
                let entry = context.url(argument(call, "url"))?;
                let engine_id = argument(call, "engine_id");
                let engine = context
                    .environment
                    .browsers
                    .iter()
                    .find(|b| b.id == engine_id && b.protocol == "chromium-cdp")
                    .ok_or_else(|| AppError::api("browser_unavailable", 422))?;
                let account_id = argument(call, "account_id");
                let mut credential = if account_id.is_empty() {
                    serde_json::from_value(
                        json!({"origin":entry.origin().ascii_serialization(),"cookies":[]}),
                    )?
                } else {
                    context.account(self, account_id)?;
                    self.credential(account_id)?
                };
                let capture = crate::browser_session::capture_with_engine(
                    &credential,
                    entry.as_str(),
                    self.network.allow_local,
                    engine,
                )
                .await?;
                let final_url = crate::net::validate_url(&capture.final_url)?;
                outcome.links = crate::discovery::published_links(&final_url, &capture.html);
                outcome.routes.extend(
                    capture
                        .observations
                        .iter()
                        .map(|o| (o.method.clone(), o.path.clone())),
                );
                if (200..300).contains(&capture.status) {
                    outcome
                        .routes
                        .insert(("GET".into(), final_url.path().into()));
                }
                if !account_id.is_empty() {
                    credential.headers.extend(capture.headers.clone());
                    if capture.identity.is_some() {
                        credential.identity_source = capture.identity_url.clone();
                    }
                    self.vault
                        .lock()
                        .map_err(|_| AppError::api("internal", 500))?
                        .set("accounts", account_id, &serde_json::to_string(&credential)?)?;
                    self.state.update(|data| {
                        let meta = data.sites.entry(context.job.site_id.clone()).or_default();
                        for observation in &capture.observations {
                            if !meta.observations.iter().any(|old| {
                                old.method == observation.method && old.path == observation.path
                            }) {
                                meta.observations.push(observation.clone());
                            }
                        }
                        Ok(())
                    })?;
                    if capture.identity.is_some() {
                        let _ = self.refresh_account(account_id).await;
                    }
                }
                if (200..300).contains(&capture.status)
                    && !account_id.is_empty()
                    && self
                        .state
                        .read()?
                        .accounts
                        .get(account_id)
                        .is_some_and(|account| {
                            account.verified_identity && account.status == "connected"
                        })
                {
                    outcome.browser_read = Some(final_url.to_string());
                }
                let mut text = json!(crate::runtime::visible_text(&capture.html));
                let mut secrets = credential.redaction_values();
                secrets.extend(capture.headers.values().cloned());
                util::redact(&mut text, &secrets);
                let text = text.as_str().unwrap_or("").to_string();
                outcome.document = Some((final_url.to_string(), text.clone()));
                outcome.content = json!({"url":final_url.as_str(),"status":capture.status,"text":text.chars().take(20_000).collect::<String>(),"html_structure":crate::html::structure(&capture.html),"html_fields":crate::html::fields(&capture.html),"observations":capture.observations,"authenticated_requests":capture.authenticated_requests,"authentication_headers":capture.headers.keys().collect::<Vec<_>>(),"identity_verified":!account_id.is_empty() && self.state.read()?.accounts.get(account_id).is_some_and(|a| a.verified_identity)});
            }
            _ => return Err(AppError::api("unsupported_tool", 422)),
        }
        Ok(outcome)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn importing_one_profile_never_reports_legacy_accounts_as_its_result() {
        let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join(".cache/tests")
            .join(util::id());
        let core = Runtime::open(&root, None, None, false, false, false).unwrap();
        let origin = "https://example.com";
        let site_id = crate::runtime::site_id_for(&url::Url::parse(origin).unwrap());
        let old = core
            .register_browser_credential(
                &site_id,
                serde_json::from_value(
                    json!({"origin":origin,"cookies":[],"source_profile":"old-profile"}),
                )
                .unwrap(),
                "Firefox",
                "Legacy profile",
            )
            .unwrap();
        let profile_path = root.join("custom-profile");
        let storage = profile_path.join("storage/default/https+++example.com/ls");
        std::fs::create_dir_all(&storage).unwrap();
        let db = rusqlite::Connection::open(storage.join("data.sqlite")).unwrap();
        db.execute_batch("CREATE TABLE database(origin TEXT); INSERT INTO database VALUES ('https://example.com'); CREATE TABLE data(key TEXT, utf16_length INTEGER, conversion_type INTEGER, compression_type INTEGER, value BLOB);").unwrap();
        db.execute(
            "INSERT INTO data VALUES ('session-key',?1,1,0,?2)",
            rusqlite::params![17, "synthetic-session".as_bytes()],
        )
        .unwrap();
        drop(db);
        let environment = Environment {
            discovery_pending: false,
            os: "fixture".into(),
            architecture: "fixture".into(),
            browsers: vec![],
            profiles: vec![
                crate::browser_discovery::BrowserProfile {
                    id: "requested-profile".into(),
                    browser: "Firefox".into(),
                    profile: "Custom".into(),
                    path: profile_path,
                    ..Default::default()
                },
                crate::browser_discovery::BrowserProfile {
                    id: "empty-profile".into(),
                    browser: "Firefox".into(),
                    path: root.join("empty-profile"),
                    ..Default::default()
                },
            ],
            capabilities: vec![],
        };
        let mut job = Job::new("prepare", "en");
        job.site_id = site_id;
        job.url = origin.into();
        let context = Context {
            job: &job,
            environment: &environment,
            links: &Default::default(),
            routes: &Default::default(),
            prefixes: &Default::default(),
            documents: &Default::default(),
        };
        let result = core
            .preparation_tool(
                &ToolCall {
                    tool: "session.import".into(),
                    arguments: json!({"profile_id":"requested-profile"}),
                },
                &context,
            )
            .await
            .unwrap();
        let accounts = result.content["accounts"].as_array().unwrap();
        assert_eq!(accounts.len(), 1);
        assert_ne!(accounts[0]["id"], old.id);
        assert_eq!(accounts[0]["profile"], "Custom");
        assert!(!result.content.to_string().contains("synthetic-session"));
        let empty = core
            .preparation_tool(
                &ToolCall {
                    tool: "session.import".into(),
                    arguments: json!({"profile_id":"empty-profile"}),
                },
                &context,
            )
            .await
            .unwrap();
        assert_eq!(empty.content["accounts"].as_array().unwrap().len(), 0);
        assert_eq!(core.state.read().unwrap().accounts.len(), 2);
        drop(core);
        std::fs::remove_dir_all(root).unwrap();
    }
}
