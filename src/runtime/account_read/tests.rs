use super::*;
use crate::session_recipe::{ConnectionRecipe, IdentityMapping};
use axum::{
    Json, Router,
    extract::Query,
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    routing::{get, post},
};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

struct Fixture {
    core: Arc<Runtime>,
    id: String,
    origin: String,
    root: std::path::PathBuf,
    calls: Arc<AtomicUsize>,
    mode: Arc<AtomicUsize>,
    artifact_calls: Arc<AtomicUsize>,
    root_calls: Arc<AtomicUsize>,
    server: tokio::task::JoinHandle<()>,
}
impl Drop for Fixture {
    fn drop(&mut self) {
        self.server.abort();
        let _ = std::fs::remove_dir_all(&self.root);
    }
}
impl Fixture {
    async fn new() -> Self {
        let calls = Arc::new(AtomicUsize::new(0));
        let mode = Arc::new(AtomicUsize::new(0));
        let artifact_calls = Arc::new(AtomicUsize::new(0));
        let root_calls = Arc::new(AtomicUsize::new(0));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let origin = format!("http://{}", listener.local_addr().unwrap());
        let count = calls.clone();
        let current = mode.clone();
        let app=Router::new()
            .route("/",get({let calls=root_calls.clone();let mode=mode.clone();let source=origin.clone();move ||{let calls=calls.clone();let mode=mode.clone();let source=source.clone();async move {
                calls.fetch_add(1,Ordering::SeqCst);
                let edge = match mode.load(Ordering::SeqCst) {
                    7 => r#"<meta http-equiv='Content-Security-Policy' content="script-src 'none'"><script src='/artifact.js'></script>"#,
                    8 => "<script nomodule src='/artifact.js'></script>",
                    9 => "<link rel='modulepreload' href='/artifact.js'>",
                    10 => "<script integrity='sha256-unverified' src='/artifact.js'></script>",
                    13 => r#"<meta http-equiv='Content-Security-Policy' content="script-src 'self'"><script src='/artifact.js'></script>"#,
                    _ => "<script src='/artifact.js'></script>",
                };
                let mut response=axum::response::Html(format!("<html><head><link rel='preload' as='fetch' type='application/json' href='/manifest.json'><link rel='preload' as='fetch' type='application/json' href='/relay.json'>{edge}</head></html>")).into_response();
                if mode.load(Ordering::SeqCst)==6 { response.headers_mut().append("content-security-policy", "script-src 'self'".parse().unwrap()); response.headers_mut().append("content-security-policy", "script-src 'none'".parse().unwrap()); }
                if mode.load(Ordering::SeqCst)==11 { response.headers_mut().append("content-security-policy", "default-src 'none'; script-src 'self'".parse().unwrap()); }
                if mode.load(Ordering::SeqCst)==12 { response.headers_mut().append("content-security-policy", format!("script-src {source}").parse().unwrap()); }
                if mode.load(Ordering::SeqCst)==14 { response.headers_mut().append("content-security-policy-report-only", "script-src 'none'".parse().unwrap()); }
                response
            }}}))
            .route("/identity",get(|headers:HeaderMap|async move {if headers.get("cookie").and_then(|h|h.to_str().ok())==Some("sid=synthetic-private-session") || headers.get("x-custom-key").and_then(|h|h.to_str().ok())==Some("custom-header-session") {(StatusCode::OK,Json(json!({"id":"person-one","name":"Synthetic"}))).into_response()}else{(StatusCode::UNAUTHORIZED,Json(json!({"error":"unauthorized"}))).into_response()}}))
            .route("/items",get(move |Query(query):Query<BTreeMap<String,String>>,headers:HeaderMap|{let count=count.clone();let mode=current.clone();async move {
                count.fetch_add(1,Ordering::SeqCst);
                assert_eq!(headers.get("cookie").unwrap(),"sid=synthetic-private-session");
                match mode.load(Ordering::SeqCst) {
                    15 => (StatusCode::FORBIDDEN,Json(json!({"error":{"code":"insufficient_scope","message":"private-provider-detail"},"token":"synthetic-private-session"}))).into_response(),
                    1 => (StatusCode::UNAUTHORIZED,Json(json!({"error":"expired"}))).into_response(),
                    2 => axum::response::Redirect::temporary("/mutate").into_response(),
                    3 => (StatusCode::TOO_MANY_REQUESTS,[("retry-after","1")],Json(json!({"error":"limited"}))).into_response(),
                    4 => (StatusCode::SERVICE_UNAVAILABLE,Json(json!({"error":"retry"}))).into_response(),
                    5 => {tokio::time::sleep(Duration::from_millis(200)).await;Json(json!({"items":[1],"next":"late-private-cursor"})).into_response()},
                    _ => Json(json!({"items":[1],"echo":"synthetic-private-session","next":if query.get("pageToken").is_some(){Value::Null}else{json!("continuation-page-two")},"cursor_echo":query.get("pageToken")})).into_response()
                }
            }}))
            .route("/graphql",post({let calls=calls.clone();move |Json(body):Json<Value>|{let calls=calls.clone();async move {calls.fetch_add(1,Ordering::SeqCst);if body.pointer("/variables/error")==Some(&Value::Bool(true)){Json(json!({"data":null,"errors":[{"message":"synthetic-private-session"}]}))}else{Json(json!({"data":{"items":[1],"next":if body.pointer("/variables/after").is_some_and(|v|!v.is_null()){Value::Null}else{json!("graphql-next-page")}},"echo":body}))}}}}))
            .route("/get-query",get({let calls=calls.clone();move |Query(query):Query<BTreeMap<String,String>>|{let calls=calls.clone();async move {calls.fetch_add(1,Ordering::SeqCst);Json(json!({"data":{"id":"get-result"},"echo":query}))}}}))
            .route("/form-query",post({let calls=calls.clone();move |axum::Form(form):axum::Form<BTreeMap<String,String>>|{let calls=calls.clone();async move {calls.fetch_add(1,Ordering::SeqCst);Json(json!({"data":{"id":"form-result"},"echo":form}))}}}))
            .route("/header-items",get(|headers:HeaderMap|async move {assert_eq!(headers.get("x-custom-key").unwrap(), "custom-header-session");Json(json!({"items":["custom-header"]}))}))
            .route("/echo-prior",get(||async {Json(json!({"previous":"continuation-page-two"}))}))
            .route("/array",get(||async {Json(json!([1,2]))}))
            .route("/large",get({let calls=calls.clone();move ||{let calls=calls.clone();async move {calls.fetch_add(1,Ordering::SeqCst);Json(json!({"large":"x".repeat(2*1024*1024)}))}}}))
            .route("/relay",post({let calls=calls.clone();move |Json(body):Json<Value>|{let calls=calls.clone();async move {calls.fetch_add(1,Ordering::SeqCst);Json(json!({"data":{"id":"one"},"echo":body}))}}}))
            .route("/manifest.json",get({let calls=artifact_calls.clone();move ||{let calls=calls.clone();async move {calls.fetch_add(1,Ordering::SeqCst);Json(apollo_manifest())}}}))
            .route("/unlinked.json",get({let calls=artifact_calls.clone();move ||{let calls=calls.clone();async move {calls.fetch_add(1,Ordering::SeqCst);Json(apollo_manifest())}}}))
            .route("/relay.json",get({let calls=artifact_calls.clone();move ||{let calls=calls.clone();async move {calls.fetch_add(1,Ordering::SeqCst);Json(json!({apollo_id():"query Items { items { id } }"}))}}}))
            .route("/artifact.js",get({let calls=artifact_calls.clone();move ||{let calls=calls.clone();async move {calls.fetch_add(1,Ordering::SeqCst);([( "content-type","application/javascript")],relay_artifact())}}}))
            .route("/mutate",get(||async {panic!("forbidden redirect target reached");#[allow(unreachable_code)] "never"}));
        let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join(".cache/account-read-tests")
            .join(util::id());
        let core = Runtime::open(&root, None, None, false, true, false).unwrap();
        let credential:Credential=serde_json::from_value(json!({"origin":origin,"cookies":[{"name":"sid","value":"synthetic-private-session","domain":"127.0.0.1","path":"/"}]})).unwrap();
        let account = core
            .register_browser_credential("fixture", credential, "Fixture", "Synthetic")
            .unwrap();
        let recipe = ConnectionRecipe {
            identity_url: format!("{origin}/identity"),
            identity: IdentityMapping {
                user_id: "/id".into(),
                name: "/name".into(),
                ..Default::default()
            },
            evidence: "observed_browser_request".into(),
            ..Default::default()
        };
        core.verify_session_recipe(&account.id, &recipe)
            .await
            .unwrap();
        Self {
            core,
            id: account.id,
            origin,
            root,
            calls,
            mode,
            artifact_calls,
            root_calls,
            server,
        }
    }
    fn descriptor(
        &self,
        name: &str,
        method: &str,
        path: &str,
        body: Option<Value>,
    ) -> std::path::PathBuf {
        let request = json!({"method":method,"url":format!("{}{path}",self.origin),"json":body});
        let har = json!({"log":{"entries":[{"request":{"method":method,"url":format!("{}{path}",self.origin),"headers":[],"postData":body.as_ref().map(|body|json!({"mimeType":"application/json","text":serde_json::to_string(body).unwrap()}))},"response":{"status":200,"content":{"mimeType":"application/json","text":"{}"}}}]}});
        let mut har = har;
        if body.is_none() {
            har.pointer_mut("/log/entries/0/request")
                .unwrap()
                .as_object_mut()
                .unwrap()
                .remove("postData");
        }
        std::fs::write(
            self.root.join(format!("{name}.har")),
            serde_json::to_vec(&har).unwrap(),
        )
        .unwrap();
        let descriptor = json!({"version":1,"request":request,"evidence":{"har":{"path":format!("{name}.har"),"entry":0}}});
        let path = self.root.join(format!("{name}.json"));
        std::fs::write(&path, serde_json::to_vec(&descriptor).unwrap()).unwrap();
        path
    }
    fn amend(&self, path: &Path, f: impl FnOnce(&mut Value)) {
        let mut descriptor: Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
        f(&mut descriptor);
        std::fs::write(path, serde_json::to_vec(&descriptor).unwrap()).unwrap();
    }
    fn artifact(&self, path: &Path, format: &str, url: &str, body: &str, mime: &str) {
        let har = json!({"log":{"entries":[{"request":{"method":"GET","url":format!("{}{url}",self.origin)},"response":{"status":200,"content":{"mimeType":mime,"text":body}}}]}});
        std::fs::write(
            self.root.join("artifact.har"),
            serde_json::to_vec(&har).unwrap(),
        )
        .unwrap();
        self.amend(
            path,
            |value| {
                value["persisted"] =
                    json!({"format":format,"evidence":{"har":{"path":"artifact.har","entry":0}}})
            },
        );
    }
}
fn apollo_id() -> String {
    util::hash(b"query Items { items { id } }")
}
fn apq_envelope() -> Value {
    json!({"extensions":{"persistedQuery":{"version":1,"sha256Hash":apollo_id()}},"operationName":"Items","variables":{}})
}
fn apollo_manifest() -> Value {
    json!({"format":"apollo-persisted-query-manifest","version":1,"operations":[{"id":apollo_id(),"body":"query Items { items { id } }","name":"Items","type":"query"}]})
}
fn relay_artifact() -> &'static str {
    "const node = {kind: 'Request', params: {id: 'relay-opaque', name: 'Items', operationKind: 'query', text: null}}; module.exports = node;"
}

#[tokio::test]
async fn observed_rest_reads_without_sitespec_and_scopes_private_continuations() {
    let f = Fixture::new().await;
    let observed = f
        .core
        .observe_account_session(&f.id, &format!("{}/items", f.origin), false)
        .await
        .unwrap();
    let path = f.descriptor("rest", "GET", "/items", None);
    f.amend(&path, |value| {
        value["evidence"] = json!({"observation":observed["read_evidence"]});
        value["pagination"] = json!({"query":"pageToken","next":"/next"});
    });
    let first = f
        .core
        .read_account_request(&f.id, &path, None)
        .await
        .unwrap();
    assert_eq!(first["body"]["items"], json!([1]));
    assert_eq!(first["body"]["echo"], "[private]");
    assert_eq!(first["body"]["next"], Value::Null);
    let handle = first["next_cursor"].as_str().unwrap();
    assert!(handle.starts_with("hc_"));
    assert!(!first.to_string().contains("continuation-page-two"));
    let second = f
        .core
        .read_account_request(&f.id, &path, Some(handle))
        .await
        .unwrap();
    assert!(second["next_cursor"].is_null());
    assert_eq!(second["body"]["cursor_echo"], "[private]");
    let before = f.calls.load(Ordering::SeqCst);
    f.amend(&path, |value| value["pagination"]["next"] = json!("/other"));
    assert_eq!(
        f.core
            .read_account_request(&f.id, &path, Some(handle))
            .await
            .unwrap_err()
            .public_code,
        "pagination_expired"
    );
    assert_eq!(f.calls.load(Ordering::SeqCst), before);
    assert!(f.core.spec("fixture").is_err());
}
#[tokio::test]
async fn graphql_documents_and_documentless_artifacts_execute_only_proven_queries() {
    let f = Fixture::new().await;
    let path=f.descriptor("graphql","POST","/graphql",Some(json!({"query":"query Items($after: String) { items { id } }","operationName":"Items","variables":{}})));
    f.amend(&path, |v| {
        v["pagination"] = json!({"variable":"after","next":"/data/next"})
    });
    let first = f
        .core
        .read_account_request(&f.id, &path, None)
        .await
        .unwrap();
    let second = f
        .core
        .read_account_request(&f.id, &path, first["next_cursor"].as_str())
        .await
        .unwrap();
    assert!(second["next_cursor"].is_null());
    assert!(!second.to_string().contains("graphql-next-page"));
    let persisted = f.descriptor("apollo", "POST", "/graphql", Some(apq_envelope()));
    f.artifact(
        &persisted,
        "apollo",
        "/manifest.json",
        &serde_json::to_string(&apollo_manifest()).unwrap(),
        "application/json",
    );
    let response = f
        .core
        .read_account_request(&f.id, &persisted, None)
        .await
        .unwrap();
    assert!(
        response["body"]["echo"].get("query").is_none(),
        "document must not be added to wire request"
    );
    let relay = f.descriptor(
        "relay",
        "POST",
        "/relay",
        Some(json!({"doc_id":"relay-opaque","variables":{}})),
    );
    f.artifact(
        &relay,
        "relay-artifact",
        "/artifact.js",
        relay_artifact(),
        "application/javascript",
    );
    let response = f
        .core
        .read_account_request(&f.id, &relay, None)
        .await
        .unwrap();
    assert_eq!(response["body"]["data"]["id"], "one");
    assert!(response["body"]["echo"].get("query").is_none());
}
#[tokio::test]
async fn ambiguous_mutating_mismatched_and_stale_requests_do_not_dispatch() {
    let f = Fixture::new().await;
    for body in [
        json!({"query":"mutation Erase { erase }"}),
        json!({"query":"query A { a } query B { b }"}),
        json!({"query":"query A { a }","operationName":"B"}),
        json!({"query":"query A($x: String) { a }","variables":{"nested":{"action":"delete"}}}),
        json!({"doc_id":"unknown"}),
        json!({"query":"query A { a }","extra":"unverified"}),
    ] {
        let path = f.descriptor("denied", "POST", "/graphql", Some(body));
        assert!(
            f.core
                .read_account_request(&f.id, &path, None)
                .await
                .is_err()
        );
    }
    assert_eq!(f.calls.load(Ordering::SeqCst), 0);
    let path = f.descriptor("mismatch", "GET", "/items", None);
    f.amend(&path, |v| {
        v["request"]["url"] = json!(format!("{}/items?scope=other", f.origin))
    });
    assert!(
        f.core
            .read_account_request(&f.id, &path, None)
            .await
            .is_err()
    );
    assert_eq!(f.calls.load(Ordering::SeqCst), 0);
    let path = f.descriptor("stale", "GET", "/items", None);
    f.core
        .update_credential(&f.id, |credential| {
            credential
                .headers
                .insert("x-csrf-token".into(), "new-session-key".into());
            Ok(())
        })
        .unwrap();
    assert_eq!(
        f.core
            .read_account_request(&f.id, &path, None)
            .await
            .unwrap_err()
            .public_code,
        "auth_required"
    );
    assert_eq!(f.calls.load(Ordering::SeqCst), 0);
}
#[tokio::test]
async fn redirects_graphql_errors_rate_limits_and_expiry_are_not_success() {
    let f = Fixture::new().await;
    let path = f.descriptor("status", "GET", "/items", None);
    f.mode.store(2, Ordering::SeqCst);
    assert_eq!(
        f.core
            .read_account_request(&f.id, &path, None)
            .await
            .unwrap_err()
            .public_code,
        "read_redirect"
    );
    let error = f.descriptor(
        "query-error",
        "POST",
        "/graphql",
        Some(json!({"query":"query Items { items { id } }","variables":{"error":true}})),
    );
    assert_eq!(
        f.core
            .read_account_request(&f.id, &error, None)
            .await
            .unwrap_err()
            .public_code,
        "query_failed"
    );
    f.mode.store(3, Ordering::SeqCst);
    assert_eq!(
        f.core
            .read_account_request(&f.id, &path, None)
            .await
            .unwrap_err()
            .public_code,
        "site_rate_limited"
    );
    tokio::time::sleep(Duration::from_secs(2)).await;
    f.mode.store(1, Ordering::SeqCst);
    assert_eq!(
        f.core
            .read_account_request(&f.id, &path, None)
            .await
            .unwrap_err()
            .public_code,
        "auth_required"
    );
    assert!(
        !f.core
            .state
            .account(&f.id)
            .unwrap()
            .unwrap()
            .verified_identity
    );
}

#[tokio::test]
async fn changed_authority_blocks_transport_retry_and_late_result_publication() {
    for mode in [4, 5] {
        let f = Fixture::new().await;
        f.mode.store(mode, Ordering::SeqCst);
        let path = f.descriptor("race", "GET", "/items", None);
        f.amend(&path, |v| {
            v["pagination"] = json!({"query":"pageToken","next":"/next"})
        });
        let core = f.core.clone();
        let id = f.id.clone();
        let running =
            tokio::spawn(async move { core.read_account_request(&id, &path, None).await });
        tokio::time::timeout(Duration::from_secs(5), async {
            while f.calls.load(Ordering::SeqCst) == 0 {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .unwrap();
        f.core
            .update_credential(&f.id, |credential| {
                credential
                    .headers
                    .insert("x-csrf-token".into(), "changed-private-key".into());
                Ok(())
            })
            .unwrap();
        assert_eq!(
            running.await.unwrap().unwrap_err().public_code,
            "auth_required"
        );
        assert_eq!(
            f.calls.load(Ordering::SeqCst),
            1,
            "stale authority must not retry"
        );
        assert!(
            f.core.pagination_values().unwrap().is_empty(),
            "late response must not publish cursors"
        );
    }
}
#[test]
fn request_secret_screen_checks_decoded_urls_and_variable_keys() {
    let secrets = vec!["synthetic-private-session".into()];
    for request in [
        json!({"method":"GET","url":"https://example.test/items?q=synthetic%2Dprivate%2Dsession"}),
        json!({"method":"GET","url":"https://example.test/synthetic%252Dprivate%252Dsession"}),
        json!({"method":"POST","url":"https://example.test/graphql","json":{"query":"query Items { items }","variables":{"synthetic-private-session":"value"}}}),
    ] {
        assert!(
            screen_request_secrets(&serde_json::from_value(request).unwrap(), &secrets).is_err()
        );
    }
}
#[test]
fn strict_read_boundary_rejects_duplicate_and_ambiguous_encoded_selectors() {
    for url in [
        "https://example.test/items?action=doThing",
        "https://example.test/items?filter=%7B%22action%22%3A%22delete%22%2C%22action%22%3A%22list%22%7D",
        "https://example.test/items?q=a&q=b",
    ] {
        let request: request::ReadRequest =
            serde_json::from_value(json!({"method":"GET","url":url})).unwrap();
        assert!(
            request
                .prepare()
                .and_then(|r| r.validate_read(None))
                .is_err()
        );
    }
}

#[test]
fn saved_override_headers_do_not_change_the_checked_request() {
    let request: request::ReadRequest =
        serde_json::from_value(json!({"method":"GET","url":"https://example.test/items"})).unwrap();
    let credential:Credential=serde_json::from_value(json!({"origin":"https://example.test","cookies":[{"name":"sid","value":"synthetic-only","domain":"example.test"}],"headers":{"x-original-url":"/delete","x-rewrite-url":"/delete","x-http-method-override":"DELETE"}})).unwrap();
    let headers = request
        .prepare()
        .unwrap()
        .headers(&credential, None)
        .unwrap();
    assert!(!headers.iter().any(|(name, _)| name.contains("override")
        || name.contains("original-url")
        || name.contains("rewrite-url")));
}

#[tokio::test]
async fn graphql_get_form_post_and_relay_document_map_keep_observed_wire_shape() {
    let f = Fixture::new().await;
    let mut url = url::Url::parse(&format!("{}/get-query", f.origin)).unwrap();
    url.query_pairs_mut()
        .append_pair("query", "query Items { items { id } }")
        .append_pair("operationName", "Items");
    let path = f.descriptor(
        "get-query",
        "GET",
        &format!("{}?{}", url.path(), url.query().unwrap()),
        None,
    );
    assert_eq!(
        f.core
            .read_account_request(&f.id, &path, None)
            .await
            .unwrap()["body"]["data"]["id"],
        "get-result"
    );
    let form = f.descriptor(
        "form-query",
        "POST",
        "/form-query",
        Some(json!({"query":"query Items { items { id } }"})),
    );
    f.amend(&form, |v| {
        v["request"].as_object_mut().unwrap().remove("json");
        v["request"]["form"] = json!({"query":"query Items { items { id } }","variables":"{}"});
    });
    let capture = f.root.join("form-query.har");
    let mut har: Value = serde_json::from_slice(&std::fs::read(&capture).unwrap()).unwrap();
    har["log"]["entries"][0]["request"]["postData"] = json!({"mimeType":"application/x-www-form-urlencoded","params":[{"name":"query","value":"query Items { items { id } }"},{"name":"variables","value":"{}"}]});
    std::fs::write(capture, har.to_string()).unwrap();
    assert_eq!(
        f.core
            .read_account_request(&f.id, &form, None)
            .await
            .unwrap()["body"]["data"]["id"],
        "form-result"
    );
    let relay = f.descriptor("relay-map", "POST", "/relay", Some(apq_envelope()));
    f.artifact(
        &relay,
        "relay",
        "/relay.json",
        &json!({apollo_id():"query Items { items { id } }"}).to_string(),
        "application/json",
    );
    let output = f
        .core
        .read_account_request(&f.id, &relay, None)
        .await
        .unwrap();
    assert!(output["body"]["echo"].get("query").is_none());
}
#[tokio::test]
async fn response_limit_and_cross_account_cursor_replay_are_bounded() {
    let f = Fixture::new().await;
    let large = f.descriptor("large", "GET", "/large", None);
    assert_eq!(
        f.core
            .read_account_request(&f.id, &large, None)
            .await
            .unwrap_err()
            .public_code,
        "too_large"
    );
    let path = f.descriptor("cursor-account", "GET", "/items", None);
    f.amend(&path, |v| {
        v["pagination"] = json!({"query":"pageToken","next":"/next"})
    });
    let first = f
        .core
        .read_account_request(&f.id, &path, None)
        .await
        .unwrap();
    let credential = f.core.credential(&f.id).unwrap();
    let recipe = credential.recipe.clone().unwrap();
    let other = f
        .core
        .register_browser_credential("other-fixture", credential, "Fixture", "Other")
        .unwrap();
    f.core
        .verify_session_recipe(&other.id, &recipe)
        .await
        .unwrap();
    let before = f.calls.load(Ordering::SeqCst);
    assert_eq!(
        f.core
            .read_account_request(&other.id, &path, first["next_cursor"].as_str())
            .await
            .unwrap_err()
            .public_code,
        "pagination_expired"
    );
    assert_eq!(f.calls.load(Ordering::SeqCst), before);
}

#[tokio::test]
async fn credential_material_is_rejected_before_artifact_or_target_dispatch() {
    let f = Fixture::new().await;
    f.core
        .vault
        .lock()
        .unwrap()
        .set("fixture", "synthetic-numeric-test", "123456789012345")
        .unwrap();
    for case in 0..6 {
        let path = f.descriptor(
            &format!("secret-screen-{case}"),
            "POST",
            "/graphql",
            Some(apq_envelope()),
        );
        f.artifact(
            &path,
            "apollo",
            "/manifest.json",
            &apollo_manifest().to_string(),
            "application/json",
        );
        match case {
            0 => f.amend(&path, |v| {
                v["request"]["url"] = json!(format!(
                    "{}/graphql?q=synthetic%2Dprivate%2Dsession",
                    f.origin
                ))
            }),
            1 => f.amend(&path, |v| {
                v["request"]["json"]["variables"]["synthetic-private-session"] = json!("value")
            }),
            5 => f.amend(&path, |v| {
                v["request"]["json"]["variables"]["id"] = json!(123456789012345u64)
            }),
            4 => {
                f.amend(
                    &path,
                    |v| {
                        v["pagination"] =
                            json!({"variable":"synthetic-private-session","next":"/data/next"})
                    },
                )
            }
            2 | 3 => {
                let source = f.root.join("artifact.har");
                let mut har: Value =
                    serde_json::from_slice(&std::fs::read(&source).unwrap()).unwrap();
                har["log"]["entries"][0]["request"]["url"] = json!(if case == 2 {
                    format!(
                        "{}/manifest.json?q=synthetic%252Dprivate%252Dsession",
                        f.origin
                    )
                } else {
                    format!(
                        "{}/manifest.json?synthetic%2Dprivate%2Dsession=value",
                        f.origin
                    )
                });
                std::fs::write(source, har.to_string()).unwrap();
            }
            _ => unreachable!(),
        }
        assert!(
            f.core
                .read_account_request(&f.id, &path, None)
                .await
                .is_err()
        );
        assert_eq!(
            f.artifact_calls.load(Ordering::SeqCst),
            0,
            "case {case} fetched an artifact"
        );
        assert_eq!(
            f.root_calls.load(Ordering::SeqCst),
            0,
            "case {case} fetched application HTML"
        );
        assert_eq!(
            f.calls.load(Ordering::SeqCst),
            0,
            "case {case} dispatched a target request"
        );
    }
}

#[tokio::test]
async fn observation_references_support_json_array_responses() {
    let f = Fixture::new().await;
    let observed = f
        .core
        .observe_account_session(&f.id, &format!("{}/array", f.origin), false)
        .await
        .unwrap();
    let path = f.descriptor("array", "GET", "/array", None);
    f.amend(&path, |v| {
        v["evidence"] = json!({"observation":observed["read_evidence"]})
    });
    assert_eq!(
        f.core
            .read_account_request(&f.id, &path, None)
            .await
            .unwrap()["body"],
        json!([1, 2])
    );
}

#[tokio::test]
async fn hash_bound_apq_uses_local_evidence_without_public_asset_requirements() {
    let f = Fixture::new().await;
    for source in ["/unlinked.json", "/manifest.json?build=current"] {
        let path = f.descriptor("local-apq-har", "POST", "/graphql", Some(apq_envelope()));
        f.artifact(
            &path,
            "apollo",
            source,
            &apollo_manifest().to_string(),
            "application/json",
        );
        assert_eq!(
            f.core
                .read_account_request(&f.id, &path, None)
                .await
                .unwrap()["body"]["data"]["items"],
            json!([1])
        );
        assert_eq!(f.root_calls.load(Ordering::SeqCst), 0);
        assert_eq!(f.artifact_calls.load(Ordering::SeqCst), 0);
    }
    let path = f.descriptor("local-apq-file", "POST", "/graphql", Some(apq_envelope()));
    std::fs::write(
        f.root.join("operations.json"),
        apollo_manifest().to_string(),
    )
    .unwrap();
    f.amend(&path, |value| {
        value["persisted"] = json!({"format":"apollo","evidence":{"path":"operations.json"}})
    });
    assert!(
        f.core
            .read_account_request(&f.id, &path, None)
            .await
            .is_ok()
    );
    let before = f.calls.load(Ordering::SeqCst);
    let mut changed = apollo_manifest();
    changed["operations"][0]["body"] = json!("query Items { other { id } }");
    std::fs::write(f.root.join("operations.json"), changed.to_string()).unwrap();
    assert_eq!(
        f.core
            .read_account_request(&f.id, &path, None)
            .await
            .unwrap_err()
            .public_code,
        "unsupported_evidence"
    );
    assert_eq!(f.calls.load(Ordering::SeqCst), before);
    assert!(
        f.core
            .state
            .account(&f.id)
            .unwrap()
            .unwrap()
            .verified_identity
    );
}

#[tokio::test]
async fn evidenced_persisted_get_keeps_exact_selector_and_rejects_changes() {
    let f = Fixture::new().await;
    let query = url::form_urlencoded::Serializer::new(String::new())
        .append_pair("extensions", &apq_envelope()["extensions"].to_string())
        .append_pair("operationName", "Items")
        .finish();
    let path = f.descriptor("persisted-get", "GET", &format!("/get-query?{query}"), None);
    f.artifact(
        &path,
        "apollo",
        "/manifest.json",
        &apollo_manifest().to_string(),
        "application/json",
    );
    let response = f
        .core
        .read_account_request(&f.id, &path, None)
        .await
        .unwrap();
    let echoed: Value =
        serde_json::from_str(response["body"]["echo"]["extensions"].as_str().unwrap()).unwrap();
    assert_eq!(echoed, apq_envelope()["extensions"]);
    assert!(response["body"]["echo"].get("query").is_none());
    let before = f.calls.load(Ordering::SeqCst);
    f.amend(&path, |v| {
        v["request"]["url"] = json!(format!(
            "{}/get-query?queryId=other-operation&operationName=Items",
            f.origin
        ))
    });
    assert!(
        f.core
            .read_account_request(&f.id, &path, None)
            .await
            .is_err()
    );
    assert_eq!(f.calls.load(Ordering::SeqCst), before);
}

#[test]
fn verified_recipe_custom_header_is_not_rejected_by_name_heuristics() {
    let request: request::ReadRequest =
        serde_json::from_value(json!({"method":"GET","url":"https://example.test/items"})).unwrap();
    let credential: Credential = serde_json::from_value(json!({
        "origin":"https://example.test",
        "cookies":[],
        "headers":{"x-custom-key":"synthetic-only"},
        "recipe":{"identity_url":"https://example.test/identity","headers":[{"header":"x-custom-key","source":"captured_header","key":"x-custom-key"}],"identity":{"user_id":"/id"},"evidence":"observed_browser_request"}
    })).unwrap();
    let headers = request
        .prepare()
        .unwrap()
        .headers(&credential, None)
        .unwrap();
    assert!(
        headers
            .iter()
            .any(|(name, value)| name == "x-custom-key" && value == "synthetic-only")
    );
}

#[tokio::test]
async fn current_header_only_recipe_reads_without_name_heuristic_failure() {
    let f = Fixture::new().await;
    let credential: Credential = serde_json::from_value(
        json!({"origin":f.origin,"cookies":[],"headers":{"x-custom-key":"custom-header-session"}}),
    )
    .unwrap();
    let account = f
        .core
        .register_browser_credential("fixture", credential, "Fixture", "Header only")
        .unwrap();
    let recipe: ConnectionRecipe = serde_json::from_value(json!({"identity_url":format!("{}/identity",f.origin),"headers":[{"header":"x-custom-key","source":"captured_header","key":"x-custom-key"}],"identity":{"user_id":"/id"},"evidence":"observed_browser_request"})).unwrap();
    f.core
        .verify_session_recipe(&account.id, &recipe)
        .await
        .unwrap();
    let path = f.descriptor("header-only", "GET", "/header-items", None);
    assert_eq!(
        f.core
            .read_account_request(&account.id, &path, None)
            .await
            .unwrap()["body"]["items"],
        json!(["custom-header"])
    );
}

#[tokio::test]
async fn expired_cursor_fails_before_target_dispatch() {
    let f = Fixture::new().await;
    let path = f.descriptor("expired-cursor", "GET", "/items", None);
    f.amend(&path, |v| {
        v["pagination"] = json!({"query":"pageToken","next":"/next"})
    });
    let first = f
        .core
        .read_account_request(&f.id, &path, None)
        .await
        .unwrap();
    let handle = first["next_cursor"].as_str().unwrap();
    {
        let mut vault = f.core.vault.lock().unwrap();
        vault
            .pagination_transaction(|records| {
                let record = records.get_mut(&format!("@pagination/{handle}")).unwrap();
                let mut value: Value = serde_json::from_str(record).unwrap();
                value["expires"] = json!(0);
                *record = value.to_string();
                Ok(())
            })
            .unwrap();
    }
    let before = f.calls.load(Ordering::SeqCst);
    assert_eq!(
        f.core
            .read_account_request(&f.id, &path, Some(handle))
            .await
            .unwrap_err()
            .public_code,
        "pagination_expired"
    );
    assert_eq!(f.calls.load(Ordering::SeqCst), before);
}

#[test]
fn known_numeric_credentials_do_not_cross_wire_or_output() {
    let secrets = vec!["123456789012345".to_string()];
    let mut value = json!({"variables":{"id":123456789012345u64}});
    assert!(
        screen_value_secrets(&value, &secrets).is_err(),
        "numeric secret must be rejected before any source or target request"
    );
    redact_keys(&mut value, &secrets).unwrap();
    util::redact(&mut value, &secrets);
    assert!(
        !value.to_string().contains("123456789012345"),
        "numeric secret must not be published"
    );
}

#[tokio::test]
async fn earlier_private_cursor_echoes_are_scrubbed_without_poisoning_continuation() {
    let f = Fixture::new().await;
    let page = f.descriptor("private-page", "GET", "/items", None);
    f.amend(&page, |v| {
        v["pagination"] = json!({"query":"pageToken","next":"/next"})
    });
    let first = f
        .core
        .read_account_request(&f.id, &page, None)
        .await
        .unwrap();
    let echo = f.descriptor("prior-echo", "GET", "/echo-prior", None);
    let output = f
        .core
        .read_account_request(&f.id, &echo, None)
        .await
        .unwrap();
    assert_eq!(output["body"]["previous"], "[private]");
    let second = f
        .core
        .read_account_request(&f.id, &page, first["next_cursor"].as_str())
        .await
        .unwrap();
    assert!(second["next_cursor"].is_null());
    assert!(
        f.core
            .state
            .account(&f.id)
            .unwrap()
            .unwrap()
            .verified_identity
    );
}

#[tokio::test]
async fn json_artifacts_cannot_authorize_opaque_ids_or_change_verified_login() {
    let f = Fixture::new().await;
    let original = f.core.account_read_snapshot(&f.id).unwrap().binding;
    for format in ["apollo", "relay"] {
        for selector in ["queryId", "doc_id", "id"] {
            // Even an ID deliberately equal to a benign document hash is not APQ.
            let path = f.descriptor(
                "opaque-json",
                "POST",
                "/graphql",
                Some(json!({selector:apollo_id(),"operationName":"Items"})),
            );
            f.artifact(
                &path,
                format,
                "/manifest.json",
                &apollo_manifest().to_string(),
                "application/json",
            );
            assert_eq!(
                f.core
                    .read_account_request(&f.id, &path, None)
                    .await
                    .unwrap_err()
                    .public_code,
                "unsupported_evidence"
            );
            f.amend(&path, |v| v["persisted"]["apq"] = json!(true));
            assert!(
                f.core
                    .read_account_request(&f.id, &path, None)
                    .await
                    .is_err()
            );
        }
    }
    assert_eq!(f.root_calls.load(Ordering::SeqCst), 0);
    assert_eq!(f.artifact_calls.load(Ordering::SeqCst), 0);
    assert_eq!(f.calls.load(Ordering::SeqCst), 0);
    assert_eq!(
        f.core.account_read_snapshot(&f.id).unwrap().binding,
        original
    );
    let normal = f.descriptor("still-logged-in", "GET", "/items", None);
    assert!(
        f.core
            .read_account_request(&f.id, &normal, None)
            .await
            .is_ok()
    );
}

#[tokio::test]
async fn opaque_relay_requires_an_unambiguous_executable_deployment_edge() {
    let f = Fixture::new().await;
    let original = f.core.account_read_snapshot(&f.id).unwrap().binding;
    let path = f.descriptor(
        "opaque-relay",
        "POST",
        "/relay",
        Some(json!({"doc_id":"relay-opaque","operationName":"Items"})),
    );
    f.artifact(
        &path,
        "relay-artifact",
        "/artifact.js",
        relay_artifact(),
        "application/javascript",
    );
    for mode in 6..=10 {
        f.mode.store(mode, Ordering::SeqCst);
        assert_eq!(
            f.core
                .read_account_request(&f.id, &path, None)
                .await
                .unwrap_err()
                .public_code,
            "unsupported_evidence",
            "mode {mode}"
        );
        assert_eq!(f.artifact_calls.load(Ordering::SeqCst), 0, "mode {mode}");
        assert_eq!(f.calls.load(Ordering::SeqCst), 0, "mode {mode}");
        assert_eq!(
            f.core.account_read_snapshot(&f.id).unwrap().binding,
            original
        );
    }
    for mode in [0, 11, 12, 13, 14] {
        f.mode.store(mode, Ordering::SeqCst);
        assert_eq!(
            f.core
                .read_account_request(&f.id, &path, None)
                .await
                .unwrap()["body"]["data"]["id"],
            "one",
            "mode {mode}"
        );
    }
}

#[test]
fn short_auth_cookie_does_not_conflict_with_local_descriptor_metadata() {
    let secrets = vec!["1".to_string()];
    let descriptor = json!({"version":1,"request":{"method":"GET","url":"https://example.test/items"},"evidence":{"har":{"path":"capture.har","entry":1}}});
    assert!(screen_metadata_secrets(&descriptor, &secrets).is_ok());
    assert!(screen_value_secrets(&descriptor["request"], &secrets).is_ok());
    let mut ordinary = json!({"variables":{"page":1,"limit":10,"id":1}});
    assert!(screen_value_secrets(&ordinary, &secrets).is_ok());
    let original = ordinary.clone();
    redact_keys(&mut ordinary, &secrets).unwrap();
    util::redact(&mut ordinary, &secrets);
    assert_eq!(ordinary, original);
    let input_secrets = read_input_secrets(&secrets);
    let request: request::ReadRequest = serde_json::from_value(
        json!({"method":"GET","url":"https://example.test/items?page=1&limit=10"}),
    )
    .unwrap();
    assert!(screen_request_secrets(&request, &input_secrets).is_ok());
    assert!(super::super::pagination::safe_value(
        "public-page-123",
        &input_secrets
    ));
}

#[test]
fn nested_read_selector_uses_shared_small_stack_graphql_bound() {
    std::thread::Builder::new()
        .name("account-read-selector-one-mib".into())
        .stack_size(1024 * 1024)
        .spawn(|| {
            for depth in [32, 33] {
                let document = format!(
                    "query Q {{ {}id{} }}",
                    "child { ".repeat(depth - 1),
                    "}".repeat(depth - 1)
                );
                let mut url = url::Url::parse("https://example.test/items").unwrap();
                url.query_pairs_mut()
                    .append_pair("filter", &json!({"query":document}).to_string());
                let request: request::ReadRequest =
                    serde_json::from_value(json!({"method":"GET","url":url.as_str()})).unwrap();
                let prepared = request.prepare().unwrap();
                assert!(!prepared.graphql);
                assert!(prepared.screen_normalized_secrets(&[]).is_ok());
                assert_eq!(prepared.validate_read(None).is_ok(), depth == 32);
            }
        })
        .unwrap()
        .join()
        .unwrap();
}

#[test]
fn normalized_protocol_values_are_screened_before_evidence_loading() {
    let secrets = vec![
        "123456789012345".to_string(),
        "synthetic-private-session".to_string(),
    ];
    for encoded in [
        r#"{"id":1.23456789012345e14}"#,
        r#"{"id":"synthetic\u002dprivate\u002dsession"}"#,
    ] {
        let form: request::ReadRequest = serde_json::from_value(json!({
            "method":"POST", "url":"https://example.test/graphql",
            "form":{"query":"query Q($id: ID) { id }", "variables":encoded}
        }))
        .unwrap();
        assert!(screen_request_secrets(&form, &secrets).is_ok());
        assert!(
            form.prepare()
                .unwrap()
                .screen_normalized_secrets(&secrets)
                .is_err()
        );
        for (path, key) in [("/graphql", "variables"), ("/items", "filter")] {
            let mut url = url::Url::parse(&format!("https://example.test{path}")).unwrap();
            if key == "variables" {
                url.query_pairs_mut()
                    .append_pair("query", "query Q($id: ID) { id }");
            }
            url.query_pairs_mut().append_pair(key, encoded);
            let request: request::ReadRequest =
                serde_json::from_value(json!({"method":"GET","url":url.as_str()})).unwrap();
            assert!(screen_request_secrets(&request, &secrets).is_ok());
            assert!(
                request
                    .prepare()
                    .unwrap()
                    .screen_normalized_secrets(&secrets)
                    .is_err()
            );
        }
    }
}

#[test]
fn graphql_literal_normalization_screens_values_not_comments_or_public_constants() {
    let secrets = vec![
        "synthetic-private-session".to_string(),
        "123456789012345".to_string(),
        "1".to_string(),
    ];
    let input_secrets = read_input_secrets(&secrets);
    for document in [
        r#"query Q { item(id:"synthetic\u002dprivate\u002dsession") { id } }"#,
        r#"query Q($id:ID="synthetic\u002dprivate\u002dsession") { item(id:$id) { id } }"#,
        r#"query Q { item(filter:{ids:[1.23456789012345e14]}) { id } }"#,
        r#"query Q { item @hint(value:"synthetic\u002dprivate\u002dsession") { id } }"#,
    ] {
        let request: request::ReadRequest = serde_json::from_value(
            json!({"method":"POST","url":"https://example.test/graphql","json":{"query":document}}),
        )
        .unwrap();
        assert!(screen_request_secrets(&request, &input_secrets).is_ok());
        assert!(
            request
                .prepare()
                .unwrap()
                .screen_normalized_secrets(&input_secrets)
                .is_err()
        );
    }
    let document = r#"# synthetic\u002dprivate\u002dsession
query Q { item(page:1,limit:10,label:"public") { id } }"#;
    let request: request::ReadRequest = serde_json::from_value(
        json!({"method":"POST","url":"https://example.test/graphql","json":{"query":document}}),
    )
    .unwrap();
    assert!(
        request
            .prepare()
            .unwrap()
            .screen_normalized_secrets(&input_secrets)
            .is_ok()
    );
}

#[tokio::test]
async fn ordinary_api_denial_preserves_account_and_allows_a_healthy_read() {
    let f = Fixture::new().await;
    let path = f.descriptor("denied", "GET", "/items", None);
    let before = f.core.account_read_snapshot(&f.id).unwrap().binding;
    f.mode.store(15, Ordering::SeqCst);
    let error = f
        .core
        .read_account_request(&f.id, &path, None)
        .await
        .unwrap_err();
    assert_eq!(error.public_code, "api_denied");
    assert_eq!(error.http_status, 403);
    assert_eq!(
        error.public_error()["error"]["diagnostic"],
        json!({"upstream_status":403,"defense":"none","provider_reason":"insufficient_scope"})
    );
    assert!(
        !error
            .public_error()
            .to_string()
            .contains("private-provider-detail")
    );
    assert_eq!(f.core.account_read_snapshot(&f.id).unwrap().binding, before);
    assert_eq!(
        f.core.state.account(&f.id).unwrap().unwrap().status,
        "connected"
    );
    f.mode.store(0, Ordering::SeqCst);
    assert!(
        f.core
            .read_account_request(&f.id, &path, None)
            .await
            .is_ok()
    );
}
