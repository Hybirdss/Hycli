//! AI-authored connection recipes reference local values; they never contain those values.
use crate::{
    accounts::{AccountView, Credential, Identity},
    apperr::{AppError, AppResult},
    runtime::Runtime,
    util,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeMap;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct HeaderSource {
    pub header: String,
    pub source: String,
    pub key: String,
    pub json_pointer: String,
    pub prefix: String,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct IdentityMapping {
    pub user_id: String,
    pub email: String,
    pub name: String,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct HtmlIdentityMapping {
    pub user_id: crate::html::Field,
    pub email: crate::html::Field,
    pub name: crate::html::Field,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ConnectionRecipe {
    pub identity_url: String,
    pub headers: Vec<HeaderSource>,
    pub identity: IdentityMapping,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub identity_html: Option<HtmlIdentityMapping>,
    pub evidence: String,
}

pub fn allowed_header(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 80
        && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
        && ![
            "cookie",
            "host",
            "origin",
            "referer",
            "content-length",
            "transfer-encoding",
            "connection",
            "proxy-authorization",
            "proxy-connection",
        ]
        .contains(&name.to_ascii_lowercase().as_str())
        && !name.to_ascii_lowercase().starts_with("sec-")
}

fn scalar(value: &Value) -> Option<String> {
    match value {
        Value::String(s) => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        _ => None,
    }
}

impl ConnectionRecipe {
    pub fn resolve_headers(&self, credential: &Credential) -> AppResult<BTreeMap<String, String>> {
        let url = crate::net::validate_url(&self.identity_url)?;
        if url.origin().ascii_serialization() != credential.origin
            || !crate::policy::safe_read_url(&url)
            || self.headers.len() > 8
        {
            return Err(AppError::api("unsafe_target", 400));
        }
        let mut headers = BTreeMap::new();
        for rule in &self.headers {
            if !allowed_header(&rule.header)
                || rule.prefix.len() > 64
                || rule.prefix.contains(['\r', '\n'])
            {
                return Err(AppError::api("data_invalid", 400));
            }
            let raw = match rule.source.as_str() {
                "local_storage" => credential.local_storage.get(&rule.key).cloned(),
                "cookie" => credential
                    .cookies
                    .iter()
                    .find(|c| c.name == rule.key && crate::accounts::cookie_matches(c, &url))
                    .map(|c| c.value.clone()),
                "captured_header" => credential
                    .headers
                    .iter()
                    .find(|(name, _)| name.eq_ignore_ascii_case(&rule.key))
                    .map(|(_, value)| value.clone()),
                _ => None,
            }
            .ok_or_else(|| AppError::api("auth_required", 401))?;
            let value = if rule.json_pointer.is_empty() {
                // Browser storage commonly stores either a raw string or a JSON string.
                match serde_json::from_str::<Value>(&raw) {
                    Ok(Value::String(s)) => s,
                    Ok(Value::Null) => String::new(),
                    _ => raw,
                }
            } else {
                let decoded: Value =
                    serde_json::from_str(&raw).map_err(|_| AppError::api("data_invalid", 400))?;
                decoded
                    .pointer(&rule.json_pointer)
                    .and_then(scalar)
                    .ok_or_else(|| AppError::api("auth_required", 401))?
            };
            if value.is_empty() || value.len() > 16384 || value.contains(['\r', '\n']) {
                return Err(AppError::api("auth_required", 401));
            }
            headers.insert(
                rule.header.to_ascii_lowercase(),
                format!("{}{value}", rule.prefix),
            );
        }
        Ok(headers)
    }

    pub fn extract_identity(&self, response: &Value) -> Option<Identity> {
        if !response.is_object() {
            return None;
        }
        let field = |pointer: &str| {
            if !pointer.starts_with('/') {
                return String::new();
            }
            response
                .pointer(pointer)
                .and_then(scalar)
                .filter(|v| !v.is_empty() && v.len() <= 320 && !v.contains(['\n', '\r']))
                .unwrap_or_default()
        };
        let result = Identity {
            user_id: field(&self.identity.user_id),
            email: field(&self.identity.email),
            name: field(&self.identity.name),
        };
        if result.user_id.is_empty() && result.email.is_empty() {
            None
        } else {
            Some(result)
        }
    }

    pub fn identity_from_response(&self, response: &crate::net::Response) -> Option<Identity> {
        if let Some(mapping) = &self.identity_html {
            if !response.header("content-type").contains("html") {
                return None;
            }
            let document = scraper::Html::parse_document(&String::from_utf8_lossy(&response.bytes));
            let identity = Identity {
                user_id: mapping.user_id.extract(&document).unwrap_or_default(),
                email: mapping.email.extract(&document).unwrap_or_default(),
                name: mapping.name.extract(&document).unwrap_or_default(),
            };
            if identity.user_id.is_empty() && identity.email.is_empty() {
                None
            } else {
                Some(identity)
            }
        } else {
            response
                .json()
                .as_ref()
                .and_then(|value| self.extract_identity(value))
        }
    }
}

/// Keys and type information only; no leaf values, lengths, hashes or token fragments.
pub(crate) fn shape(value: &Value, depth: usize) -> Value {
    if depth > 4 {
        return json!("nested");
    }
    match value {
        Value::Object(items) => Value::Object(
            items
                .iter()
                .take(30)
                .map(|(k, v)| {
                    let key = if k.len() > 80
                        || k.contains(['\n', '\r', '@', '/', ':'])
                        || k.chars().next().is_some_and(|c| c.is_ascii_digit())
                    {
                        "[dynamic key]".into()
                    } else {
                        k.clone()
                    };
                    (key, shape(v, depth + 1))
                })
                .collect(),
        ),
        Value::Array(_) => json!("array"),
        Value::String(_) => json!("string"),
        Value::Null => json!("null"),
        Value::Bool(_) => json!("bool"),
        Value::Number(_) => json!("number"),
    }
}

impl Runtime {
    pub(crate) fn inspect_session_key(
        &self,
        account: &str,
        origin: &str,
        key: &str,
        offset: usize,
    ) -> AppResult<Value> {
        let credential = self.credential(account)?;
        if credential.origin != origin {
            return Err(AppError::api("unsafe_target", 400));
        }
        let keys: Vec<_> = credential
            .local_storage
            .keys()
            .filter(|key| key.len() <= 100 && !key.contains(['\r', '\n']))
            .collect();
        let storage_keys: Vec<_> = keys
            .iter()
            .skip(offset)
            .take(250)
            .map(|key| {
                let kind = match serde_json::from_str::<Value>(&credential.local_storage[*key]) {
                    Ok(Value::Null) => "null",
                    Ok(Value::String(_)) => "JSON string",
                    Ok(Value::Object(_)) => "object",
                    Ok(Value::Array(_)) => "array",
                    Ok(_) => "scalar",
                    Err(_) => "string",
                };
                format!("{key}: {kind}")
            })
            .collect();
        let selected = credential.local_storage.get(key).map(|value| {
            serde_json::from_str::<Value>(value)
                .ok()
                .map(|v| shape(&v, 0))
                .unwrap_or(json!("string"))
        });
        Ok(
            json!({"account_id":account,"origin":origin,"storage_keys":storage_keys.join("\n"),"total_keys":keys.len(),"next_offset":if offset+250<keys.len(){Some(offset+250)}else{None},"selected_key":key,"selected_shape":selected,"cookie_names":credential.cookies.iter().map(|c| &c.name).collect::<Vec<_>>(),"captured_headers":credential.headers.keys().collect::<Vec<_>>(),"saved_recipe":credential.recipe,"values_exposed":false}),
        )
    }

    /// The preparation broker must additionally verify that identity_url is a documented or
    /// observed GET. This function performs the actual scoped request and persists only success.
    pub(crate) async fn verify_session_recipe(
        &self,
        account: &str,
        recipe: &ConnectionRecipe,
    ) -> AppResult<AccountView> {
        let mut credential = self.credential(account)?;
        let resolved = recipe.resolve_headers(&credential)?;
        credential.headers.extend(resolved);
        self.policy
            .resume_auth(&format!("{}|{account}", credential.origin))?;
        let response = self
            .network
            .fetch(crate::net::Request {
                url: recipe.identity_url.clone(),
                method: "GET".into(),
                headers: credential.request_headers(&recipe.identity_url)?,
                body: None,
                account_id: account.into(),
                limit: 1024 * 1024,
                timeout: std::time::Duration::from_secs(25),
                read_only: true,
                follow_redirects: false,
                tls_profile: "standard".into(),
            })
            .await?;
        if !response.successful() {
            return Err(AppError::api(
                if response.status == 401 || response.status == 403 {
                    "auth_required"
                } else {
                    "request_failed"
                },
                422,
            ));
        }
        let identity = recipe
            .identity_from_response(&response)
            .ok_or_else(|| AppError::api("identity_unavailable", 422))?;
        // Do not label an arbitrary public profile endpoint as the signed-in account.
        let baseline_scope = format!("{account}:identity-baseline");
        // An unauthenticated 401 is the expected baseline result, not a lost user session.
        self.policy
            .resume_auth(&format!("{}|{baseline_scope}", credential.origin))?;
        let anonymous = self
            .network
            .fetch(crate::net::Request {
                url: recipe.identity_url.clone(),
                method: "GET".into(),
                headers: vec![],
                body: None,
                account_id: baseline_scope,
                limit: 1024 * 1024,
                timeout: std::time::Duration::from_secs(25),
                read_only: true,
                follow_redirects: false,
                tls_profile: "standard".into(),
            })
            .await?;
        if anonymous.successful() && recipe.identity_from_response(&anonymous).is_some() {
            return Err(AppError::api("identity_unavailable", 422));
        }
        credential.identity_source = recipe.identity_url.clone();
        credential.recipe = Some(recipe.clone());
        self.vault
            .lock()
            .map_err(|_| AppError::api("internal", 500))?
            .set("accounts", account, &serde_json::to_string(&credential)?)?;
        let mut view = self
            .state
            .read()?
            .accounts
            .get(account)
            .cloned()
            .ok_or_else(|| AppError::api("not_found", 404))?;
        crate::accounts::apply_identity(&mut view, identity);
        view.checked_at = util::now();
        self.state.update(|data| {
            data.accounts.insert(account.into(), view.clone());
            let site = data.sites.entry(view.site_id.clone()).or_default();
            site.connection_recipe = Some(recipe.clone());
            if !site.identity_paths.contains(&recipe.identity_url) {
                site.identity_paths.push(recipe.identity_url.clone());
            }
            Ok(())
        })?;
        self.changed();
        Ok(view)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn html_session_identity_is_observed_verified_and_rechecked_without_browser_engine() {
        use axum::{Router, http::HeaderMap, response::Html, routing::get};
        use std::sync::{
            Arc,
            atomic::{AtomicBool, Ordering},
        };
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let origin = format!("http://{}", listener.local_addr().unwrap());
        let active = Arc::new(AtomicBool::new(true));
        let current = active.clone();
        let signed_in = "<html><head><meta name='current-person' content='private-person'></head><body>Signed in<a href='/public-person'>Profile</a></body></html>";
        let router = Router::new()
            .route("/", get(move |headers: HeaderMap| { let current = current.clone(); async move {
                if current.load(Ordering::SeqCst) && headers.get("cookie").and_then(|v|v.to_str().ok()).is_some_and(|v|v.contains("sid=synthetic-private")) {
                    Html(signed_in)
                } else { Html("<html><head><meta name='current-person' content=''></head><body>Sign in</body></html>") }
            }}))
            .route("/public-person", get(move || async move { Html(signed_in) }));
        let server = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
        let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join(".cache/tests")
            .join(util::id());
        let core = Runtime::open(&root, None, None, false, true, false).unwrap();
        let credential: Credential = serde_json::from_value(json!({"origin":origin,"cookies":[{"name":"sid","value":"synthetic-private","domain":"127.0.0.1","path":"/"}]})).unwrap();
        let account = core
            .register_browser_credential("fixture", credential, "Firefox", "Custom profile")
            .unwrap();
        let mut job = crate::state::Job::new("prepare", "en");
        job.site_id = "fixture".into();
        job.url = format!("{origin}/");
        let environment = crate::environment::Environment {
            os: "fixture".into(),
            architecture: "fixture".into(),
            browsers: vec![],
            profiles: vec![],
            capabilities: vec![],
        };
        let context = crate::agent_tools::Context {
            job: &job,
            environment: &environment,
            links: &Default::default(),
            routes: &Default::default(),
            prefixes: &Default::default(),
            documents: &Default::default(),
        };
        let observed = core
            .preparation_tool(
                &crate::agent_tools::ToolCall {
                    tool: "session.observe".into(),
                    arguments: json!({"account_id":account.id,"url":job.url}),
                },
                &context,
            )
            .await
            .unwrap();
        assert_eq!(observed.content["format"], "html");
        assert!(!observed.content.to_string().contains("private-person"));
        assert!(!observed.content.to_string().contains("synthetic-private"));
        assert!(observed.browser_read.is_none());
        assert!(!core.state.read().unwrap().accounts[&account.id].verified_identity);
        let field = crate::html::Field {
            selector: "meta[name=\"current-person\"]".into(),
            attribute: "content".into(),
        };
        let recipe = ConnectionRecipe {
            identity_url: job.url.clone(),
            identity_html: Some(HtmlIdentityMapping {
                user_id: field.clone(),
                ..Default::default()
            }),
            evidence: job.url.clone(),
            ..Default::default()
        };
        let documents = BTreeMap::from([observed.document.unwrap()]);
        let context = crate::agent_tools::Context {
            job: &job,
            environment: &environment,
            links: &observed.links,
            routes: &observed.routes,
            prefixes: &Default::default(),
            documents: &documents,
        };
        let verified = core
            .preparation_tool(
                &crate::agent_tools::ToolCall {
                    tool: "session.verify".into(),
                    arguments: json!({"account_id":account.id,"recipe":recipe}),
                },
                &context,
            )
            .await
            .unwrap();
        assert_eq!(verified.content["verified"], true);
        // A published link is enough evidence for a read; it need not first be
        // rediscovered through a browser engine's network log.
        let published_url = format!("{origin}/public-person");
        let read = core
            .preparation_tool(
                &crate::agent_tools::ToolCall {
                    tool: "session.read".into(),
                    arguments: json!({"account_id":account.id,"url":published_url}),
                },
                &context,
            )
            .await
            .unwrap();
        assert_eq!(read.browser_read.as_deref(), Some(published_url.as_str()));
        assert!(
            read.routes
                .contains(&("GET".into(), "/public-person".into()))
        );
        let guessed = core.preparation_tool(&crate::agent_tools::ToolCall {
            tool: "session.read".into(),
            arguments: json!({"account_id":account.id,"url":format!("{origin}/unpublished")}),
        }, &context).await.err().unwrap();
        assert_eq!(guessed.public_code, "unsupported_evidence");
        assert!(
            core.refresh_account(&account.id)
                .await
                .unwrap()
                .verified_identity
        );
        let operation: crate::spec::Operation = serde_json::from_value(json!({"name":"current","method":"GET","path":"/","response":{"format":"html","required_html_fields":[field]}})).unwrap();
        assert!(operation.matches_response(&core.read_page(&job.url, &account.id).await.unwrap()));
        assert!(!operation.matches_response(&core.read_page(&job.url, "").await.unwrap()));
        let mut unguarded = operation.clone();
        unguarded
            .response
            .as_mut()
            .unwrap()
            .required_html_fields
            .clear();
        assert!(!unguarded.matches_response(&core.read_page(&job.url, "").await.unwrap()));
        let mut public = recipe;
        public.identity_url = format!("{origin}/public-person");
        assert_eq!(
            core.verify_session_recipe(&account.id, &public)
                .await
                .err()
                .unwrap()
                .public_code,
            "identity_unavailable"
        );
        active.store(false, Ordering::SeqCst);
        assert_eq!(
            core.refresh_account(&account.id)
                .await
                .err()
                .unwrap()
                .public_code,
            "identity_unavailable"
        );
        assert!(!core.state.read().unwrap().accounts[&account.id].verified_identity);
        server.abort();
        drop(core);
        std::fs::remove_dir_all(root).unwrap();
    }
    #[tokio::test]
    async fn firefox_storage_recipe_verifies_nonstandard_identity_and_rejects_public_profiles() {
        use axum::{
            Json, Router,
            http::{HeaderMap, StatusCode},
            routing::get,
        };
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let origin = format!("http://{}", listener.local_addr().unwrap());
        let identity = json!({"context":{"person":{"key":"42","display":"Synthetic Person"}}});
        let public = identity.clone();
        let router = Router::new()
            .route(
                "/api-items",
                get(|headers: HeaderMap| async move {
                    assert_eq!(
                        headers.get("x-session").and_then(|h| h.to_str().ok()),
                        Some("Bot synthetic-api")
                    );
                    Json(json!({"actor":"api"}))
                }),
            )
            .route(
                "/current-context",
                get(move |headers: HeaderMap| {
                    let identity = identity.clone();
                    async move {
                        if headers.get("x-session").and_then(|h| h.to_str().ok())
                            == Some("Session synthetic-private")
                        {
                            (StatusCode::OK, Json(identity))
                        } else {
                            (
                                StatusCode::UNAUTHORIZED,
                                Json(json!({"error":"auth required"})),
                            )
                        }
                    }
                }),
            )
            .route(
                "/public-person",
                get(move || {
                    let public = public.clone();
                    async move { Json(public) }
                }),
            );
        let server = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
        let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join(".cache/tests")
            .join(util::id());
        let core = Runtime::open(&root, None, None, false, true, false).unwrap();
        let credential: Credential = serde_json::from_value(json!({"origin":origin,"cookies":[],"local_storage":{"custom-account-state":"{\"auth\":{\"value\":\"synthetic-private\"}}"}})).unwrap();
        let account = core
            .register_browser_credential("fixture", credential, "Firefox", "Custom profile")
            .unwrap();
        let recipe = ConnectionRecipe {
            identity_url: format!("{origin}/current-context"),
            headers: vec![HeaderSource {
                header: "X-Session".into(),
                source: "local_storage".into(),
                key: "custom-account-state".into(),
                json_pointer: "/auth/value".into(),
                prefix: "Session ".into(),
            }],
            identity: IdentityMapping {
                user_id: "/context/person/key".into(),
                name: "/context/person/display".into(),
                ..Default::default()
            },
            evidence: format!("{origin}/docs"),
            ..Default::default()
        };
        let inspection = core
            .inspect_session_key(&account.id, &origin, "", 0)
            .unwrap();
        assert!(!inspection.to_string().contains("synthetic-private"));
        let verified = core
            .verify_session_recipe(&account.id, &recipe)
            .await
            .unwrap();
        assert!(verified.verified_identity);
        assert_eq!(verified.user_id, "42");
        assert_eq!(
            core.browser_account_for("fixture", &origin).unwrap(),
            account.id
        );
        let mut job = crate::state::Job::new("prepare", "en");
        job.site_id = "fixture".into();
        job.url = origin.clone();
        let environment = crate::environment::Environment {
            os: "fixture".into(),
            architecture: "fixture".into(),
            browsers: vec![],
            profiles: vec![],
            capabilities: vec![],
        };
        let routes = std::collections::BTreeSet::from([("GET".into(), "/current-context".into())]);
        let context = crate::agent_tools::Context {
            job: &job,
            environment: &environment,
            links: &Default::default(),
            routes: &routes,
            prefixes: &Default::default(),
            documents: &Default::default(),
        };
        let read = core
            .preparation_tool(
                &crate::agent_tools::ToolCall {
                    tool: "session.read".into(),
                    arguments: json!({"account_id":account.id,"url":recipe.identity_url}),
                },
                &context,
            )
            .await
            .unwrap();
        assert_eq!(read.content["status"], 200);
        assert_eq!(
            read.content["response_shape"]["context"]["person"]["key"],
            "string"
        );
        assert!(!read.content.to_string().contains("Synthetic Person"));
        assert_eq!(
            read.browser_read.as_deref(),
            Some(recipe.identity_url.as_str())
        );
        let spec = crate::spec::parse(json!({"spec_version":1,"site":{"name":"fixture","base_url":origin},"auth":[{"name":"browser-session","kind":"browser"},{"name":"api-key","kind":"header","header":"X-Session","prefix":"Bot ","value_from":"store:fixture/api-key"}],"operations":[{"name":"current","method":"GET","path":"/current-context","auth":"browser-session","effect":"read","evidence":"actual verified session read","response":{"format":"json"}},{"name":"api-items","method":"GET","path":"/api-items","auth":"api-key","effect":"read","evidence":"fixture reference"}]}).to_string().as_bytes()).unwrap();
        core.install(&spec).unwrap();
        assert!(
            !core.sites("en").unwrap()[0].credentials[0].configured,
            "A browser session is not a configured API key"
        );
        assert_eq!(
            core.request(&spec, "api-items", Default::default(), Some(&account.id))
                .err()
                .unwrap()
                .public_code,
            "auth_required"
        );
        let request = core
            .request(&spec, "current", Default::default(), Some(&account.id))
            .unwrap();
        assert_eq!(core.execute(request, None).await.unwrap().0.status, 200);
        core.set_website_credential("fixture", "api-key", Some("synthetic-api"))
            .unwrap();
        let request = core
            .request(&spec, "api-items", Default::default(), Some(&account.id))
            .unwrap();
        assert_eq!(core.execute(request, None).await.unwrap().0.status, 200);
        let refreshed = core.refresh_account(&account.id).await.unwrap();
        assert!(refreshed.verified_identity);
        let saved = core.credential(&account.id).unwrap();
        assert_eq!(
            saved
                .request_headers(&recipe.identity_url)
                .unwrap()
                .iter()
                .find(|(k, _)| k == "x-session")
                .unwrap()
                .1,
            "Session synthetic-private"
        );
        assert!(
            saved
                .request_headers("https://other.example/current-context")
                .is_err()
        );
        let mut public_recipe = recipe.clone();
        public_recipe.identity_url = format!("{origin}/public-person");
        assert_eq!(
            core.verify_session_recipe(&account.id, &public_recipe)
                .await
                .err()
                .unwrap()
                .public_code,
            "identity_unavailable"
        );
        assert_eq!(
            core.credential(&account.id)
                .unwrap()
                .recipe
                .unwrap()
                .identity_url,
            recipe.identity_url
        );
        server.abort();
        drop(core);
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn recipes_resolve_private_values_without_site_specific_names_or_origins() {
        let c: Credential = serde_json::from_value(json!({"origin":"https://example.com","cookies":[],"local_storage":{"custom-account-state":"{\"auth\":{\"value\":\"synthetic-private\"}}"}})).unwrap();
        let recipe = ConnectionRecipe {
            identity_url: "https://example.com/current-context".into(),
            headers: vec![HeaderSource {
                header: "X-Session".into(),
                source: "local_storage".into(),
                key: "custom-account-state".into(),
                json_pointer: "/auth/value".into(),
                prefix: "Session ".into(),
            }],
            identity: IdentityMapping {
                user_id: "/context/person/key".into(),
                name: "/context/person/display".into(),
                ..Default::default()
            },
            ..Default::default()
        };
        assert_eq!(
            recipe.resolve_headers(&c).unwrap()["x-session"],
            "Session synthetic-private"
        );
        assert_eq!(
            recipe
                .extract_identity(&json!({"context":{"person":{"key":"42","display":"Example"}}}))
                .unwrap()
                .user_id,
            "42"
        );
        assert!(
            !serde_json::to_string(&recipe)
                .unwrap()
                .contains("synthetic-private")
        );
        let mut other = recipe;
        other.identity_url = "https://other.example/current-context".into();
        assert!(other.resolve_headers(&c).is_err());
    }
}
