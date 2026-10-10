//! Synthetic fixtures only: never contact or modify a live website.
use crate::{
    policy::{self, Effect},
    runtime::Runtime,
    spec, util,
};
use axum::{
    Json, Router,
    http::{HeaderMap, StatusCode},
    routing::{get, post},
};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};
fn root() -> PathBuf {
    std::env::var_os("HYCLI_TEST_DATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(".cache/tests"))
        .join(util::id())
}
fn runtime() -> Arc<Runtime> {
    Runtime::open(&root(), None, None, false, true, false).unwrap()
}
async fn host(router: Router) -> (String, tokio::task::JoinHandle<()>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = format!("http://{}", listener.local_addr().unwrap());
    let task = tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });
    (address, task)
}
fn allow_changes(core: &Runtime, site: &str) {
    core.state
        .update(|d| {
            d.sites.entry(site.into()).or_default().writes = true;
            Ok(())
        })
        .unwrap();
}
fn spec_for(origin: &str) -> spec::Spec {
    spec::parse(json!({"spec_version":1,"site":{"name":"fixture","title":"Fixture website","base_url":origin},"operations":[{"name":"read-items","method":"GET","path":"/items","effect":"read","evidence":"Synthetic fixture documentation"},{"name":"create-item","method":"POST","path":"/items","effect":"write","evidence":"Synthetic fixture documentation","body":{"json":{"title":{"type":"string","required":true}}}}]}).to_string().as_bytes()).unwrap()
}
#[test]
fn defaults_and_disguised_methods_never_make_changes_look_read_only() {
    for path in [
        "/cart/add",
        "/api/deleteItem",
        "/api/%64elete",
        "/logout",
        "/toggle",
    ] {
        let mut op = spec_for("https://example.com").operations.remove(0);
        op.path = path.into();
        assert_eq!(
            policy::operation_effect(&op, &BTreeMap::new()),
            Effect::Write,
            "{path}"
        );
    }
    let mut op = spec_for("https://example.com").operations.remove(0);
    op.path = "/search".into();
    op.name = "search-items".into();
    op.method = "POST".into();
    op.body = Some(spec::Body {
        json: BTreeMap::from([(
            "operation".into(),
            spec::Param {
                kind: "string".into(),
                required: false,
                default: Some(json!("delete")),
            },
        )]),
        ..Default::default()
    });
    assert_eq!(
        policy::operation_effect(&op, &BTreeMap::new()),
        Effect::Write
    );
    for raw in [
        "https://example.com/api?mode=delete",
        "https://example.com/api?action=logout",
        "https://example.com/api?csrf=opaque",
    ] {
        let u = url::Url::parse(raw).unwrap();
        assert!(!policy::safe_read_url(&u), "{raw}");
    }
}
#[test]
fn opaque_credentials_are_removed_from_nested_results() {
    let mut value = json!({"token":"opaque-token-1234","authToken":"opaque-auth-5678","csrf":"csrf-opaque-value","cookies":[{"name":"sid","value":"unseen-cookie-value"}],"headers":[{"name":"Authorization","value":"opaque-header"}],"echo":"short x credential","useful":{"name":"Mina","count":3}});
    util::redact(&mut value, &["x".into()]);
    let text = value.to_string();
    for private in [
        "opaque-token-1234",
        "opaque-auth-5678",
        "csrf-opaque-value",
        "unseen-cookie-value",
        "opaque-header",
        "short x credential",
    ] {
        assert!(!text.contains(private), "{private}");
    }
    assert_eq!(value["useful"]["count"], 3);
    assert_eq!(value["useful"]["name"], "Mina");
    let mut evidence = json!({"text":"environment version v10 token=long-private-value","engine":"browser-engine-101"});
    util::redact(
        &mut evidence,
        &["en".into(), "1".into(), "long-private-value".into()],
    );
    assert!(
        evidence["text"]
            .as_str()
            .unwrap()
            .starts_with("environment version v10")
    );
    assert_eq!(evidence["engine"], "browser-engine-101");
}

#[tokio::test]
async fn documented_api_origin_does_not_inherit_homepage_cookies_and_html_is_not_api_success() {
    let (api, server) = host(
        Router::new()
            .route(
                "/items",
                get(|headers: HeaderMap| async move {
                    assert!(headers.get("cookie").is_none());
                    Json(json!({"items":[]}))
                }),
            )
            .route(
                "/fallback",
                get(|| async { axum::response::Html("<html>Sign in</html>") }),
            ),
    )
    .await;
    let core = runtime();
    let mut spec = spec_for(&api);
    spec.site.source_url = "https://app.example.com".into();
    spec.operations[0].response = Some(spec::ResponseExpectation {
        format: "json".into(),
        required_pointers: vec!["/items".into()],
        ..Default::default()
    });
    core.install(&spec).unwrap();
    let cookies=crate::accounts::parse_cookies(r#"[{"name":"sid","value":"synthetic-homepage-secret","domain":"app.example.com","path":"/","secure":true}]"#, &spec.site.source_url).unwrap();
    let account = core
        .connect_account(
            "fixture",
            &spec.site.source_url,
            cookies,
            "Firefox",
            "Personal",
            "",
        )
        .unwrap();
    let request = core
        .request(&spec, "read-items", BTreeMap::new(), None)
        .unwrap();
    assert!(request.account_id.is_empty());
    assert!(
        core.request(&spec, "read-items", BTreeMap::new(), Some(&account.id))
            .is_err()
    );
    core.execute(request, None).await.unwrap();
    assert_eq!(
        core.state.read().unwrap().sites["fixture"]
            .verified
            .get("read-items"),
        Some(&true)
    );
    spec.operations[0].path = "/fallback".into();
    core.install(&spec).unwrap();
    let request = core
        .request(&spec, "read-items", BTreeMap::new(), None)
        .unwrap();
    assert_eq!(
        core.execute(request, None).await.err().unwrap().public_code,
        "response_mismatch"
    );
    assert_eq!(
        core.state.read().unwrap().sites["fixture"]
            .verified
            .get("read-items"),
        Some(&false)
    );
    server.abort();
}

#[tokio::test]
async fn mixed_web_and_api_actions_execute_with_separate_credential_origins() {
    let (web, web_server) = host(Router::new().route(
        "/account",
        get(|headers: HeaderMap| async move {
            assert!(headers.get("authorization").is_none());
            if headers
                .get("cookie")
                .and_then(|v| v.to_str().ok())
                .is_some_and(|v| v.contains("sid=synthetic-browser"))
            {
                (StatusCode::OK, Json(json!({"id":"fixture-person"})))
            } else {
                (
                    StatusCode::UNAUTHORIZED,
                    Json(json!({"error":"auth_required"})),
                )
            }
        }),
    ))
    .await;
    let (api, api_server) = host(
        Router::new()
            .route(
                "/public",
                get(|headers: HeaderMap| async move {
                    assert!(headers.get("cookie").is_none());
                    assert!(headers.get("authorization").is_none());
                    Json(json!({"items":[]}))
                }),
            )
            .route(
                "/private",
                get(|headers: HeaderMap| async move {
                    assert!(headers.get("cookie").is_none());
                    assert_eq!(
                        headers.get("authorization").unwrap(),
                        "Bearer synthetic-api"
                    );
                    Json(json!({"items":[]}))
                }),
            ),
    )
    .await;
    let core = runtime();
    let spec = crate::spec::parse(json!({"spec_version":1,"site":{"name":"fixture","base_url":api,"source_url":web},"auth":[{"name":"browser","kind":"browser"},{"name":"api-key","kind":"header","header":"Authorization","prefix":"Bearer ","origin":api,"value_from":"store:fixture/api-key"}],"operations":[{"name":"account","method":"GET","path":"/account","base_url":web,"auth":"browser","effect":"read","evidence":"observed current account","response":{"format":"json","required_pointers":["/id"]}},{"name":"public","method":"GET","path":"/public","effect":"read","evidence":"published public API"},{"name":"private","method":"GET","path":"/private","auth":"api-key","effect":"read","evidence":"published protected API"}]}).to_string().as_bytes()).unwrap();
    core.install(&spec).unwrap();
    let cookies = crate::accounts::parse_cookies(
        r#"[{"name":"sid","value":"synthetic-browser","domain":"127.0.0.1","path":"/"}]"#,
        &web,
    )
    .unwrap();
    let account = core
        .connect_account("fixture", &web, cookies, "Firefox", "Profile", "")
        .unwrap();
    let recipe = crate::session_recipe::ConnectionRecipe {
        identity_url: format!("{web}/account"),
        identity: crate::session_recipe::IdentityMapping {
            user_id: "/id".into(),
            ..Default::default()
        },
        ..Default::default()
    };
    core.verify_session_recipe(&account.id, &recipe)
        .await
        .unwrap();
    core.browser_account_for("fixture", &web).unwrap();
    core.state
        .update(|data| {
            data.sites.get_mut("fixture").unwrap().preparation = Some(Default::default());
            Ok(())
        })
        .unwrap();
    let browser = core
        .request(&spec, "account", Default::default(), Some(&account.id))
        .unwrap();
    assert_eq!(core.execute(browser, None).await.unwrap().0.status, 200);
    let public = core
        .request(&spec, "public", Default::default(), None)
        .unwrap();
    assert!(public.account_id.is_empty());
    assert_eq!(core.execute(public, None).await.unwrap().0.status, 200);
    assert_eq!(
        core.sites("en").unwrap()[0].status,
        "ready",
        "A successful user-supplied read resolves an earlier preparation with no executable inputs"
    );
    core.set_website_credential("fixture", "api-key", Some("synthetic-api"))
        .unwrap();
    let private = core
        .request(&spec, "private", Default::default(), None)
        .unwrap();
    assert!(private.account_id.is_empty());
    assert_eq!(core.execute(private, None).await.unwrap().0.status, 200);
    let sites = core.sites("en").unwrap();
    assert!(
        sites[0]
            .actions
            .iter()
            .find(|a| a.id == "account")
            .unwrap()
            .accepts_account
    );
    assert!(
        !sites[0]
            .actions
            .iter()
            .find(|a| a.id == "public")
            .unwrap()
            .accepts_account
    );
    assert!(
        !sites[0]
            .actions
            .iter()
            .find(|a| a.id == "private")
            .unwrap()
            .accepts_account
    );
    web_server.abort();
    api_server.abort();
}
#[tokio::test]
async fn an_anonymous_identity_failure_does_not_block_public_reads() {
    let (url, server) = host(
        Router::new()
            .route(
                "/private",
                get(|| async {
                    (
                        StatusCode::UNAUTHORIZED,
                        [("server", "cloudflare"), ("cf-ray", "synthetic")],
                        Json(json!({"error":"auth required"})),
                    )
                }),
            )
            .route("/articles", get(|| async { Json(json!({"items":[]})) })),
    )
    .await;
    let core = runtime();
    let private = core.read_page(&format!("{url}/private"), "").await.unwrap();
    assert_eq!(private.status, 401);
    assert_eq!(private.defense, "none");
    assert!(
        core.read_page(&format!("{url}/articles"), "")
            .await
            .unwrap()
            .successful()
    );
    server.abort();
}
#[tokio::test]
async fn a_change_needs_exact_single_use_approval_and_is_never_tested() {
    let count = Arc::new(AtomicUsize::new(0));
    let c = count.clone();
    let (url, server) = host(Router::new().route(
        "/items",
        post(move || {
            let c = c.clone();
            async move {
                c.fetch_add(1, Ordering::SeqCst);
                Json(json!({"created":true}))
            }
        }),
    ))
    .await;
    let core = runtime();
    let sp = spec_for(&url);
    core.install(&sp).unwrap();
    allow_changes(&core, &sp.site.name);
    let args = BTreeMap::from([("title".into(), "A concrete task".into())]);
    assert!(core.run_op(&sp, "create-item", &args).await.is_err());
    assert_eq!(count.load(Ordering::SeqCst), 0);
    let review = core.policy.pending().unwrap().remove(0);
    let request = core.policy.review(&review.id).unwrap().request.unwrap();
    let mut changed = request.clone();
    changed
        .inputs
        .insert("title".into(), "A different task".into());
    assert!(core.execute(changed, Some(&review.id)).await.is_err());
    assert_eq!(count.load(Ordering::SeqCst), 0);
    assert_eq!(
        core.execute(request.clone(), Some(&review.id))
            .await
            .unwrap()
            .0
            .status,
        200
    );
    assert_eq!(count.load(Ordering::SeqCst), 1);
    assert!(core.execute(request, Some(&review.id)).await.is_err());
    assert_eq!(count.load(Ordering::SeqCst), 1);
    server.abort();
}
#[tokio::test]
async fn approval_is_invalidated_when_the_spec_changes() {
    let core = runtime();
    let mut sp = spec_for("http://127.0.0.1:1");
    core.install(&sp).unwrap();
    allow_changes(&core, &sp.site.name);
    let request = core
        .request(
            &sp,
            "create-item",
            BTreeMap::from([("title".into(), "One".into())]),
            None,
        )
        .unwrap();
    let review = core.approval(request.clone(), "en").unwrap();
    sp.operations[1].path = "/different-items".into();
    core.install(&sp).unwrap();
    allow_changes(&core, &sp.site.name);
    let error = core.execute(request, Some(&review.id)).await.err().unwrap();
    assert_eq!(error.public_code, "approval_expired");
}
#[tokio::test]
async fn cookie_echo_is_scrubbed_and_never_exported() {
    let (url,server)=host(Router::new().route("/items",get(|headers:HeaderMap|async move{assert_eq!(headers.get("cookie").unwrap(),"sid=synthetic-private-cookie");Json(json!({"items":[{"title":"Useful result"}],"echo":"synthetic-private-cookie","token":"opaque-new-token"}))}))).await;
    let core = runtime();
    let sp = spec_for(&url);
    core.install(&sp).unwrap();
    let u = url::Url::parse(&url).unwrap();
    let cookies=crate::accounts::parse_cookies(&json!([{"name":"sid","value":"synthetic-private-cookie","domain":u.host_str().unwrap(),"path":"/"}]).to_string(),&url).unwrap();
    core.connect_account("fixture", &url, cookies, "Test browser", "Test profile", "")
        .unwrap();
    let (result, _) = core
        .run_op(&sp, "read-items", &BTreeMap::new())
        .await
        .unwrap();
    let text = serde_json::to_string(&result).unwrap();
    assert!(!text.contains("synthetic-private-cookie"));
    assert!(!text.contains("opaque-new-token"));
    assert!(text.contains("Useful result"));
    let d = serde_json::to_string(&core.state.read().unwrap()).unwrap();
    assert!(!d.contains("synthetic-private-cookie"));
    server.abort();
}
#[tokio::test]
async fn rate_limit_pauses_further_calls_and_write_failures_are_not_retried() {
    let hits = Arc::new(AtomicUsize::new(0));
    let h = hits.clone();
    let (url, server) = host(Router::new().route(
        "/items",
        get(move || {
            let h = h.clone();
            async move {
                h.fetch_add(1, Ordering::SeqCst);
                (
                    StatusCode::TOO_MANY_REQUESTS,
                    [("retry-after", "60")],
                    "Please wait",
                )
            }
        }),
    ))
    .await;
    let core = runtime();
    let sp = spec_for(&url);
    core.install(&sp).unwrap();
    allow_changes(&core, &sp.site.name);
    assert_eq!(
        core.run_op(&sp, "read-items", &BTreeMap::new())
            .await
            .err()
            .unwrap()
            .public_code,
        "site_rate_limited"
    );
    assert!(
        core.run_op(&sp, "read-items", &BTreeMap::new())
            .await
            .is_err()
    );
    assert_eq!(hits.load(Ordering::SeqCst), 1);
    server.abort();
    let hits = Arc::new(AtomicUsize::new(0));
    let h = hits.clone();
    let (url, server) = host(Router::new().route(
        "/items",
        post(move || {
            let h = h.clone();
            async move {
                h.fetch_add(1, Ordering::SeqCst);
                StatusCode::SERVICE_UNAVAILABLE
            }
        }),
    ))
    .await;
    let core = runtime();
    let sp = spec_for(&url);
    core.install(&sp).unwrap();
    allow_changes(&core, &sp.site.name);
    let request = core
        .request(
            &sp,
            "create-item",
            BTreeMap::from([("title".into(), "One".into())]),
            None,
        )
        .unwrap();
    let review = core.approval(request.clone(), "en").unwrap();
    assert_eq!(
        core.execute(request, Some(&review.id))
            .await
            .unwrap()
            .0
            .status,
        503
    );
    assert_eq!(hits.load(Ordering::SeqCst), 1);
    server.abort();
}
#[tokio::test]
async fn redirects_cannot_turn_a_read_into_a_change() {
    let hits = Arc::new(AtomicUsize::new(0));
    let h = hits.clone();
    let (url, server) = host(
        Router::new()
            .route(
                "/items",
                get(|| async { (StatusCode::FOUND, [("location", "/api?mode=delete")]) }),
            )
            .route(
                "/api",
                get(move || {
                    let h = h.clone();
                    async move {
                        h.fetch_add(1, Ordering::SeqCst);
                        "changed"
                    }
                }),
            ),
    )
    .await;
    let core = runtime();
    let sp = spec_for(&url);
    core.install(&sp).unwrap();
    let error = core
        .run_op(&sp, "read-items", &BTreeMap::new())
        .await
        .err()
        .unwrap();
    assert_eq!(error.public_code, "request_failed");
    assert_eq!(hits.load(Ordering::SeqCst), 0);
    server.abort();
}
#[tokio::test]
async fn loopback_api_requires_session_origin_and_csrf() {
    let core = runtime();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let web = crate::dashboard::Web::new(core, port).unwrap();
    let launch = web.launch.clone();
    let router = crate::dashboard::router(web);
    let server = tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });
    let base = format!("http://127.0.0.1:{port}");
    let client = reqwest::Client::new();
    assert_eq!(
        client
            .get(format!("{base}/api/state"))
            .send()
            .await
            .unwrap()
            .status(),
        401
    );
    // A plain local request cannot mint the session that approves changes.
    for address in [
        format!("{base}/api/session"),
        format!("{base}/api/session?launch=guess"),
    ] {
        assert_eq!(client.get(address).send().await.unwrap().status(), 401);
    }
    let session = client
        .get(format!("{base}/api/session?launch={launch}"))
        .send()
        .await
        .unwrap();
    let cookie = session
        .headers()
        .get("set-cookie")
        .unwrap()
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_string();
    let token = session.json::<Value>().await.unwrap()["csrf"]
        .as_str()
        .unwrap()
        .to_string();
    assert_eq!(
        client
            .patch(format!("{base}/api/settings"))
            .header("cookie", &cookie)
            .json(&json!({"locale":"ko"}))
            .send()
            .await
            .unwrap()
            .status(),
        401
    );
    assert_eq!(
        client
            .patch(format!("{base}/api/settings"))
            .header("cookie", &cookie)
            .header("x-hycli-csrf", &token)
            .header("origin", "https://attacker.example")
            .json(&json!({"locale":"ko"}))
            .send()
            .await
            .unwrap()
            .status(),
        403
    );
    assert_eq!(
        client
            .patch(format!("{base}/api/settings"))
            .header("cookie", &cookie)
            .header("x-hycli-csrf", &token)
            .json(&json!({"locale":"ko"}))
            .send()
            .await
            .unwrap()
            .status(),
        200
    );
    server.abort();
}
#[test]
fn plaintext_fallback_is_explicit_and_private() {
    let path = root().join("vault.json");
    let mut vault = crate::store::Store::open_with_keyring(&path, false).unwrap();
    vault.set("site", "key", "synthetic-key").unwrap();
    assert_eq!(vault.protection(), "file_permissions_only");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
    let loaded = crate::store::Store::open_with_keyring(&path, false).unwrap();
    assert!(loaded.contains("site", "key"));
}
#[test]
fn stale_process_state_does_not_overwrite_another_process_update() {
    let path = root().join("state.json");
    let a = crate::state::StateStore::open_with_recovery(&path, false).unwrap();
    let b = crate::state::StateStore::open_with_recovery(&path, false).unwrap();
    a.update(|d| {
        d.settings.locale = "ja".into();
        Ok(())
    })
    .unwrap();
    b.update(|d| {
        d.settings.theme = "dark".into();
        Ok(())
    })
    .unwrap();
    let result = a.read().unwrap();
    assert_eq!(result.settings.locale, "ja");
    assert_eq!(result.settings.theme, "dark");
}

#[test]
fn graphql_strings_and_comments_do_not_disguise_a_mutation() {
    let mut op = spec_for("https://example.com").operations.remove(0);
    op.name = "query-data".into();
    op.method = "POST".into();
    op.path = "/api".into();
    for query in [
        "query { item(search: \"#\") { id } } mutation { deleteAll }",
        "query { item(search: \"safe\") { id } }\nmutation M { change }",
        "subscription { updates }",
        "query A { item { id } } query B { item { id } }",
        "query { item(",
        "{ items { id } } broken syntax",
    ] {
        assert_ne!(
            policy::operation_effect(&op, &BTreeMap::from([("query".into(), query.into())])),
            Effect::Read
        );
    }
    for query in [
        "query { item(search: \"mutation\") { id } }",
        "# mutation is just a comment\n{ items { id } }",
    ] {
        assert_eq!(
            policy::operation_effect(&op, &BTreeMap::from([("query".into(), query.into())])),
            Effect::Read
        );
    }
}

#[tokio::test]
async fn browser_pairing_is_single_use_origin_bound_and_verifies_actual_identity() {
    let (website,site_server)=host(Router::new().route("/account",get(|headers:HeaderMap| async move {
        assert_eq!(headers.get("cookie").unwrap(),"sid=synthetic-bridge-cookie");
        Json(json!({"user":{"id":"fixture-user-42","email":"mina@example.test","name":"Mina"},"token":"synthetic-response-secret"}))
    }))).await;
    let core = runtime();
    core.install(&spec_for(&website)).unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let web = crate::dashboard::Web::new(core.clone(), port).unwrap();
    let launch = web.launch.clone();
    let router = crate::dashboard::router(web);
    let server = tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });
    let base = format!("http://127.0.0.1:{port}");
    let client = reqwest::Client::new();
    let session = client
        .get(format!("{base}/api/session?launch={launch}"))
        .send()
        .await
        .unwrap();
    let cookie = session
        .headers()
        .get("set-cookie")
        .unwrap()
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_string();
    let csrf = session.json::<Value>().await.unwrap()["csrf"]
        .as_str()
        .unwrap()
        .to_owned();
    let paired = client
        .post(format!("{base}/api/accounts/pair"))
        .header("cookie", &cookie)
        .header("x-hycli-csrf", csrf)
        .json(&json!({"site_id":"fixture","url":website}))
        .send()
        .await
        .unwrap()
        .json::<Value>()
        .await
        .unwrap();
    let origin = "chrome-extension://abcdefghijklmnopabcdefghijklmnop";
    assert_eq!(
        client
            .post(format!("{base}/bridge/pair"))
            .header("origin", "https://attacker.example")
            .json(&json!({"code":paired["code"]}))
            .send()
            .await
            .unwrap()
            .status(),
        403
    );
    let exchange = client
        .post(format!("{base}/bridge/pair"))
        .header("origin", origin)
        .json(&json!({"code":paired["code"]}))
        .send()
        .await
        .unwrap()
        .json::<Value>()
        .await
        .unwrap();
    assert_eq!(
        client
            .post(format!("{base}/bridge/pair"))
            .header("origin", "chrome-extension://another-extension")
            .json(&json!({"code":paired["code"]}))
            .send()
            .await
            .unwrap()
            .status(),
        400
    );
    let payload = json!({"token":exchange["token"],"cookies":[{"name":"sid","value":"synthetic-bridge-cookie","domain":"127.0.0.1","path":"/"}],"browser":"Synthetic browser","identity_source":format!("{website}/account")});
    let response = client
        .post(format!("{base}/bridge/connect"))
        .header("origin", origin)
        .json(&payload)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let connected = response.json::<Value>().await.unwrap();
    let id = connected["account_id"].as_str().unwrap();
    assert_eq!(
        client
            .post(format!("{base}/bridge/connect"))
            .header("origin", origin)
            .json(&payload)
            .send()
            .await
            .unwrap()
            .status(),
        400
    );
    let mut verified = false;
    for _ in 0..100 {
        let state = core.state.read().unwrap();
        if state.accounts[id].verified_identity {
            assert_eq!(state.accounts[id].email, "mina@example.test");
            verified = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    assert!(verified);
    let observation = json!({"token":connected["session_token"],"observations":[{"method":"GET","path":"/items","query":{"q":"string","token":"string"},"body":{},"status":200}]});
    assert_eq!(
        client
            .post(format!("{base}/bridge/observe"))
            .header("origin", "chrome-extension://another-extension")
            .json(&observation)
            .send()
            .await
            .unwrap()
            .status(),
        403
    );
    assert_eq!(
        client
            .post(format!("{base}/bridge/observe"))
            .header("origin", origin)
            .json(&observation)
            .send()
            .await
            .unwrap()
            .status(),
        200
    );
    let state = core.state.read().unwrap();
    assert!(
        state.sites["fixture"].observations[0]
            .query
            .contains_key("q")
    );
    assert!(
        !state.sites["fixture"].observations[0]
            .query
            .contains_key("token")
    );
    let logout = json!({"token":connected["session_token"],"cookies":[]});
    assert_eq!(
        client
            .post(format!("{base}/bridge/refresh"))
            .header("origin", "chrome-extension://another-extension")
            .json(&logout)
            .send()
            .await
            .unwrap()
            .status(),
        403
    );
    assert!(core.state.read().unwrap().accounts[id].verified_identity);
    assert_eq!(
        client
            .post(format!("{base}/bridge/refresh"))
            .header("origin", origin)
            .json(&logout)
            .send()
            .await
            .unwrap()
            .status(),
        200
    );
    let logged_out = core.state.read().unwrap().accounts[id].clone();
    assert_eq!(logged_out.status, "expired");
    assert!(!logged_out.verified_identity);
    assert!(logged_out.email.is_empty());
    let mut renewed = payload.clone();
    renewed["token"] = connected["session_token"].clone();
    assert_eq!(
        client
            .post(format!("{base}/bridge/refresh"))
            .header("origin", origin)
            .json(&renewed)
            .send()
            .await
            .unwrap()
            .status(),
        200
    );
    let mut reconnected = false;
    for _ in 0..200 {
        let account = core.state.read().unwrap().accounts[id].clone();
        if account.verified_identity && account.status == "connected" {
            assert_eq!(account.email, "mina@example.test");
            reconnected = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    assert!(
        reconnected,
        "renewed sign-in should reconnect the same account"
    );
    let exported = client
        .get(format!("{base}/api/state"))
        .header("cookie", cookie)
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    for secret in [
        "synthetic-bridge-cookie",
        "synthetic-response-secret",
        connected["session_token"].as_str().unwrap(),
    ] {
        assert!(!exported.contains(secret));
    }
    server.abort();
    site_server.abort();
}

#[test]
fn path_arguments_and_nested_controls_keep_changes_behind_review() {
    let mut op = spec_for("https://example.com").operations.remove(0);
    op.path = "/api/{route}".into();
    op.params.insert(
        "route".into(),
        spec::Param {
            kind: "string".into(),
            required: true,
            default: None,
        },
    );
    assert_eq!(
        policy::operation_effect(&op, &BTreeMap::from([("route".into(), "delete".into())])),
        Effect::Write
    );
    op.params.clear();
    op.path = "/search".into();
    op.method = "POST".into();
    op.name = "search-data".into();
    assert_eq!(
        policy::operation_effect(
            &op,
            &BTreeMap::from([("options".into(), r#"{"filter":{"mode":"delete"}}"#.into())])
        ),
        Effect::Write
    );
}

#[test]
fn removing_sites_and_accounts_invalidates_pending_approvals() {
    let core = runtime();
    let sp = spec_for("https://example.test");
    core.install(&sp).unwrap();
    allow_changes(&core, &sp.site.name);
    let req = core
        .request(
            &sp,
            "create-item",
            BTreeMap::from([("title".into(), "Fixture".into())]),
            None,
        )
        .unwrap();
    core.approval(req, "en").unwrap();
    assert_eq!(core.policy.pending().unwrap().len(), 1);
    core.remove_site("fixture").unwrap();
    assert!(core.policy.pending().unwrap().is_empty());
}

#[tokio::test]
async fn refreshing_without_observed_identity_does_not_claim_a_check() {
    let core = runtime();
    let url = "https://example.test";
    core.install(&spec_for(url)).unwrap();
    let cookies = crate::accounts::parse_cookies(
        r#"[{"name":"sid","value":"synthetic-session","domain":"example.test","path":"/"}]"#,
        url,
    )
    .unwrap();
    let account = core
        .connect_account("fixture", url, cookies, "Fixture browser", "", "")
        .unwrap();
    assert!(account.checked_at.is_empty());
    assert_eq!(
        core.refresh_account(&account.id)
            .await
            .err()
            .unwrap()
            .public_code,
        "identity_unavailable"
    );
    assert!(
        core.state.read().unwrap().accounts[&account.id]
            .checked_at
            .is_empty()
    );
}

#[tokio::test]
async fn approval_is_bound_to_the_credential_that_was_reviewed() {
    let core = runtime();
    let url = "http://127.0.0.1:1";
    let sp = spec_for(url);
    core.install(&sp).unwrap();
    allow_changes(&core, &sp.site.name);
    let cookies = crate::accounts::parse_cookies(
        r#"[{"name":"sid","value":"synthetic-account-one","domain":"127.0.0.1","path":"/"}]"#,
        url,
    )
    .unwrap();
    let account = core
        .connect_account("fixture", url, cookies, "Fixture browser", "", "")
        .unwrap();
    let request = core
        .request(
            &sp,
            "create-item",
            BTreeMap::from([("title".into(), "Fixture".into())]),
            Some(&account.id),
        )
        .unwrap();
    let review = core.approval(request.clone(), "en").unwrap();
    let mut credential = core.credential(&account.id).unwrap();
    credential.cookies[0].value = "synthetic-account-two".into();
    core.vault
        .lock()
        .unwrap()
        .set(
            "accounts",
            &account.id,
            &serde_json::to_string(&credential).unwrap(),
        )
        .unwrap();
    assert_eq!(
        core.execute(request, Some(&review.id))
            .await
            .err()
            .unwrap()
            .public_code,
        "approval_expired"
    );
}

#[test]
fn large_results_do_not_bloat_dashboard_snapshots_and_clear_with_history() {
    let folder = root();
    let path = folder.join("state.json");
    let state = crate::state::StateStore::open_with_recovery(&path, false).unwrap();
    let mut job = crate::state::Job::new("action", "en");
    job.id = "0123456789abcdef0123456789abcdef".into();
    job.status = "completed".into();
    job.result = Some(json!({"text":"x".repeat(2*1024*1024)}));
    let id = job.id.clone();
    state.add_job(job).unwrap();
    assert!(std::fs::metadata(&path).unwrap().len() < 8192);
    assert!(state.read().unwrap().jobs[0].result.is_none());
    assert_eq!(
        state.job(&id).unwrap().result.unwrap()["text"]
            .as_str()
            .unwrap()
            .len(),
        2 * 1024 * 1024
    );
    state
        .update(|data| {
            data.jobs.clear();
            Ok(())
        })
        .unwrap();
    assert!(!folder.join("results").join(format!("{id}.json")).exists());
}

#[test]
fn automatic_browser_account_selection_preserves_the_selected_person() {
    let core = runtime();
    let url = "https://example.com";
    core.install(&spec_for(url)).unwrap();
    allow_changes(&core, &spec_for(url).site.name);
    let cookies = || {
        crate::accounts::parse_cookies(
            r#"[{"name":"sid","value":"synthetic-session","domain":"example.com","path":"/"}]"#,
            url,
        )
        .unwrap()
    };
    let first = core
        .connect_account("fixture", url, cookies(), "Chrome", "Personal", "")
        .unwrap();
    let second = core
        .connect_account("fixture", url, cookies(), "Chrome", "Work", "")
        .unwrap();
    assert!(
        core.browser_account_for("fixture", url).unwrap().is_empty(),
        "cookies alone do not verify an account"
    );
    core.state
        .update(|state| {
            for (id, user) in [(&first.id, "person-one"), (&second.id, "person-two")] {
                crate::accounts::apply_identity(
                    state.accounts.get_mut(id).unwrap(),
                    crate::accounts::Identity {
                        user_id: user.into(),
                        ..Default::default()
                    },
                );
            }
            Ok(())
        })
        .unwrap();
    assert_eq!(core.browser_account_for("fixture", url).unwrap(), first.id);
    core.state
        .update(|state| {
            state.accounts.get_mut(&first.id).unwrap().status = "expired".into();
            Ok(())
        })
        .unwrap();
    assert!(
        core.browser_account_for("fixture", url).unwrap().is_empty(),
        "expired selected account must not switch to another person"
    );
    core.state
        .update(|state| {
            state.sites.get_mut("fixture").unwrap().account_id.clear();
            state.accounts.get_mut(&first.id).unwrap().status = "connected".into();
            Ok(())
        })
        .unwrap();
    assert!(
        core.browser_account_for("fixture", url).unwrap().is_empty(),
        "ambiguous profiles require an account choice"
    );
    core.remove_account(&second.id).unwrap();
    assert_eq!(core.browser_account_for("fixture", url).unwrap(), first.id);
}

#[tokio::test]
async fn documentation_redirect_drops_the_original_session() {
    let seen = Arc::new(AtomicUsize::new(0));
    let counted = seen.clone();
    let (docs, docs_server) = host(Router::new().route(
        "/reference",
        get(move |headers: HeaderMap| {
            let counted = counted.clone();
            async move {
                assert!(!headers.contains_key("cookie"));
                assert!(!headers.contains_key("authorization"));
                counted.fetch_add(1, Ordering::SeqCst);
                "GET /items"
            }
        }),
    ))
    .await;
    let target = format!("{docs}/reference");
    let (home, home_server) = host(Router::new().route(
        "/docs",
        get(move || {
            let target = target.clone();
            async move { axum::response::Redirect::temporary(&target) }
        }),
    ))
    .await;
    let core = runtime();
    let cookie = crate::accounts::parse_cookies(r#"[{"name":"sid","value":"synthetic-cross-origin-secret","domain":"127.0.0.1","path":"/"}]"#, &home).unwrap();
    let account = core
        .connect_account("", &home, cookie, "fixture", "", "")
        .unwrap();
    let response = core
        .read_evidence_page(&format!("{home}/docs"), &account.id)
        .await
        .unwrap();
    assert_eq!(response.status, 200);
    assert_eq!(seen.load(Ordering::SeqCst), 1);
    docs_server.abort();
    home_server.abort();
}

#[tokio::test]
#[ignore = "requires installed Chromium; run explicitly for browser session regression"]
async fn browser_restores_storage_captures_identity_and_blocks_writes() {
    let writes = Arc::new(AtomicUsize::new(0));
    let counted = writes.clone();
    let (url, server) = host(
        Router::new()
            .route(
                "/login",
                get(|| async {
                    axum::response::Html(
                        r#"<script>
            const token = localStorage.getItem('site-session');
            fetch('/users/@me', {headers: {Authorization: 'Bearer '+token}});
            fetch('/items', {method:'POST',body:'must-not-run'});
        </script>"#,
                    )
                }),
            )
            .route("/app", get(|| async { axum::response::Html(r#"<h1>Workspace</h1><main id="items"></main><script>
                fetch('/users/@me', {headers: {Authorization: 'Bearer '+localStorage.getItem('site-session')}})
                .then(response => response.json()).then(user => { document.querySelector('#items').innerHTML = '<article><h2>'+user.username+'</h2></article>'; });
                fetch('/items', {method:'POST',body:'must-not-run'});
            </script>"#) }))
            .route(
                "/users/@me",
                get(|headers: HeaderMap| async move {
                    if headers
                        .get("authorization")
                        .is_some_and(|h| h == "Bearer synthetic-storage-secret")
                    {
                        (
                            StatusCode::OK,
                            Json(json!({"id":"browser-person", "username":"Fixture Person"})),
                        )
                    } else {
                        (
                            StatusCode::UNAUTHORIZED,
                            Json(json!({"message":"not signed in"})),
                        )
                    }
                }),
            )
            .route(
                "/items",
                post(move || {
                    let counted = counted.clone();
                    async move {
                        counted.fetch_add(1, Ordering::SeqCst);
                        "bad"
                    }
                }),
            ),
    )
    .await;
    let mut credential: crate::accounts::Credential = serde_json::from_value(json!({"origin":url,"cookies":[],"local_storage":{"site-session":"synthetic-storage-secret"}})).unwrap();
    let capture = crate::browser_session::capture(&credential, &format!("{url}/login"), true)
        .await
        .unwrap();
    assert_eq!(capture.identity.unwrap().user_id, "browser-person");
    assert_eq!(capture.identity_url, format!("{url}/users/@me"));
    assert_eq!(writes.load(Ordering::SeqCst), 0);
    credential.headers = capture.headers;
    credential.identity_source = capture.identity_url;
    let core = runtime();
    core.install(&spec_for(&url)).unwrap();
    let account = core
        .register_browser_credential("fixture", credential.clone(), "Chromium", "test")
        .unwrap();
    let verified = core.refresh_account(&account.id).await.unwrap();
    assert!(verified.verified_identity);
    assert_eq!(
        core.browser_account_for("fixture", &url).unwrap(),
        account.id
    );
    let browser_spec = spec::parse(json!({"site":{"name":"fixture","base_url":url},"auth":[{"name":"session","kind":"browser"}],"operations":[{"name":"workspace","method":"GET","path":"/app","transport":"browser","auth":"session","effect":"read","evidence":"Synthetic observed rendered workspace","response":{"format":"html","required_html_fields":[{"selector":"#items article h2"}],"html":{"items":"#items article","fields":{"title":{"selector":"h2","required":true}}}}}]}).to_string().as_bytes()).unwrap();
    core.install(&browser_spec).unwrap();
    let rendered = core
        .run_op_for_account(
            &browser_spec,
            "workspace",
            &BTreeMap::new(),
            Some(&account.id),
        )
        .await
        .unwrap()
        .0;
    assert_eq!(rendered.status, 200);
    assert_eq!(
        rendered.body.unwrap()["items"][0]["title"],
        "Fixture Person"
    );
    assert_eq!(writes.load(Ordering::SeqCst), 0);
    let duplicate = core
        .register_browser_credential("fixture", credential.clone(), "Chromium", "duplicate")
        .unwrap();
    core.refresh_account(&duplicate.id).await.unwrap();
    core.state
        .update(|data| {
            data.sites.get_mut("fixture").unwrap().account_id.clear();
            Ok(())
        })
        .unwrap();
    assert!(
        !core
            .browser_account_for("fixture", &url)
            .unwrap()
            .is_empty(),
        "one person in two profiles does not require a choice"
    );
    assert!(
        credential
            .request_headers("https://different.example/users/@me")
            .is_err()
    );
    let mut exported = json!({"echo":"Bearer synthetic-storage-secret"});
    util::redact(&mut exported, &credential.redaction_values());
    assert!(!exported.to_string().contains("synthetic-storage-secret"));
    server.abort();
}

#[tokio::test]
async fn website_keys_are_scoped_private_and_invalidate_pending_approvals() {
    let (url, server) = host(Router::new().route(
        "/items",
        get(|headers: HeaderMap| async move {
            assert_eq!(headers.get("x-api-key").unwrap(), "synthetic-website-key");
            Json(json!({"items":["private reference"],"echo":"synthetic-website-key"}))
        }),
    ))
    .await;
    let core = runtime();
    let mut sp = spec_for(&url);
    sp.auth.push(crate::spec::AuthStrategy {
        name: "website-key".into(),
        kind: "header".into(),
        header: "x-api-key".into(),
        prefix: String::new(),
        value_from: "store:fixture/api-key".into(),
        origin: String::new(),
    });
    for operation in &mut sp.operations {
        operation.auth = "website-key".into();
    }
    core.install(&sp).unwrap();
    allow_changes(&core, &sp.site.name);
    assert_eq!(core.sites("en").unwrap()[0].status, "needs_signin");
    assert_eq!(
        core.request(&sp, "read-items", BTreeMap::new(), Some(""))
            .err()
            .unwrap()
            .public_code,
        "auth_required"
    );
    assert!(
        core.set_website_credential("fixture", "not-declared", Some("value"))
            .is_err()
    );
    assert!(
        core.set_website_credential("fixture", "website-key", Some("header\r\ninjection"))
            .is_err()
    );
    core.set_website_credential("fixture", "website-key", Some("synthetic-website-key"))
        .unwrap();
    let sites = core.sites("en").unwrap();
    assert!(sites[0].credentials[0].configured);
    assert!(
        !serde_json::to_string(&sites)
            .unwrap()
            .contains("synthetic-website-key")
    );
    let (result, _) = core
        .run_op(&sp, "read-items", &BTreeMap::new())
        .await
        .unwrap();
    assert_eq!(result.status, 200);
    assert!(
        !serde_json::to_string(&result)
            .unwrap()
            .contains("synthetic-website-key")
    );
    let request = core
        .request(
            &sp,
            "create-item",
            BTreeMap::from([("title".into(), "One".into())]),
            Some(""),
        )
        .unwrap();
    let approval = core.approval(request, "en").unwrap();
    core.set_website_credential("fixture", "website-key", Some("rotated-key"))
        .unwrap();
    assert_eq!(
        core.policy.review(&approval.id).err().unwrap().public_code,
        "approval_expired"
    );
    core.set_website_credential("fixture", "website-key", None)
        .unwrap();
    assert!(!core.sites("en").unwrap()[0].credentials[0].configured);
    server.abort();
}

#[tokio::test]
async fn cancellation_and_restart_keep_dispatched_changes_uncertain() {
    let core = runtime();
    let mut write = crate::state::Job::new("action", "en");
    write.approval_id = "already-authorized".into();
    let job = core.spawn_job(write, std::future::pending()).unwrap();
    core.cancel_job(&job.id).unwrap();
    let stopped = core.state.job(&job.id).unwrap();
    assert_eq!(stopped.status, "failed");
    assert_eq!(stopped.error_code, "action_outcome_unknown");
    let read = core
        .spawn_job(
            crate::state::Job::new("action", "en"),
            std::future::pending(),
        )
        .unwrap();
    core.cancel_job(&read.id).unwrap();
    assert_eq!(core.state.job(&read.id).unwrap().status, "cancelled");
    let mut interrupted = crate::state::Job::new("action", "en");
    interrupted.status = "running".into();
    interrupted.approval_id = "claimed".into();
    core.state.add_job(interrupted.clone()).unwrap();
    let restored = crate::state::StateStore::open(&core.root.join("dashboard.json")).unwrap();
    assert_eq!(
        restored.job(&interrupted.id).unwrap().error_code,
        "action_outcome_unknown"
    );
    assert!(
        restored
            .job(&interrupted.id)
            .unwrap()
            .notes
            .iter()
            .any(|note| note.code == "interrupted")
    );
}

#[test]
fn duplicate_preparation_description_and_summary_are_rejected_atomically() {
    let path = root().join("dashboard.json");
    let state = crate::state::StateStore::open(&path).unwrap();
    for kind in ["prepare", "describe", "summary"] {
        let mut job = crate::state::Job::new(kind, "en");
        job.site_id = "fixture".into();
        job.parent_id = "parent-result".into();
        state.add_job(job.clone()).unwrap();
        job.id = util::id();
        assert_eq!(
            state.add_job(job.clone()).unwrap_err().public_code,
            "job_running"
        );
        if kind == "describe" {
            job.locale = "ko".into();
            state.add_job(job).unwrap();
        }
    }
}

#[tokio::test]
async fn form_requests_and_structured_html_reads_use_the_shared_executor() {
    let (base, server) = host(Router::new()
        .route("/search", post(|headers: HeaderMap, body: String| async move {
            assert_eq!(headers.get("content-type").unwrap(), "application/x-www-form-urlencoded");
            let form: BTreeMap<_, _> = url::form_urlencoded::parse(body.as_bytes()).into_owned().collect();
            assert_eq!(form.get("q").unwrap(), "space & 한글=ok");
            assert_eq!(form.get("limit").unwrap(), "2");
            Json(json!({"results":[{"id":1}]}))
        }))
        .route("/catalog", get(|| async { axum::response::Html(r#"<h1>Catalogue</h1><article class="book"><h2><a href="./one"> One title </a></h2><span class="price">£12</span></article><article class="book"><h2><a href="./two">Two title</a></h2><span class="price">£14</span></article><a class="next" href="?page=2">Next</a>"#) }))
        .route("/unexpected-html", get(|| async {axum::response::Html("<h1>Sign in</h1><input type='password'>")}))).await;
    let core = runtime();
    let definition = spec::parse(json!({"site":{"name":"fixture","base_url":base},"operations":[
        {"name":"search","method":"POST","path":"/search","effect":"read","evidence":"Synthetic form search documentation","headers":{"Content-Type":"application/json"},"body":{"form":{"q":{"required":true},"limit":{"type":"int","default":2}}},"response":{"format":"json","required_pointers":["/results"]}},
        {"name":"catalog","method":"GET","path":"/catalog","effect":"read","evidence":"Observed synthetic catalogue","response":{"format":"html","required_html_fields":[{"selector":"a.next","attribute":"href"}],"html":{"items":"article.book","fields":{"title":{"selector":"h2 a","required":true},"url":{"selector":"h2 a","attribute":"href","absolute_url":true},"price":{"selector":".price","required":true}},"page":{"next_url":{"selector":"a.next","attribute":"href","absolute_url":true}}}}}
    ]}).to_string().as_bytes()).unwrap();
    let old = spec_for(&base);
    core.install(&old).unwrap();
    let before = Runtime::spec_hash(&core.spec("fixture").unwrap()).unwrap();
    let candidate = core
        .verify_candidate_with_inputs(&definition, "catalog", "", BTreeMap::new())
        .await
        .unwrap()
        .0;
    assert_eq!(candidate.status, 200);
    assert_eq!(
        Runtime::spec_hash(&core.spec("fixture").unwrap()).unwrap(),
        before
    );
    assert!(
        !core.state.read().unwrap().sites["fixture"]
            .verified
            .contains_key("catalog")
    );
    assert!(
        core.verify_candidate_with_inputs(&old, "create-item", "", BTreeMap::new())
            .await
            .is_err()
    );
    let checked_form = core
        .verify_candidate_with_inputs(
            &definition,
            "search",
            "",
            BTreeMap::from([("q".into(), "space & 한글=ok".into())]),
        )
        .await
        .unwrap()
        .0;
    assert_eq!(checked_form.status, 200);
    assert_eq!(
        Runtime::spec_hash(&core.spec("fixture").unwrap()).unwrap(),
        before
    );
    assert!(
        core.verify_candidate_with_inputs(
            &old,
            "create-item",
            "",
            BTreeMap::from([("title".into(), "Never create during preparation".into())])
        )
        .await
        .is_err()
    );
    core.install(&definition).unwrap();
    let form = core
        .run_op(
            &definition,
            "search",
            &BTreeMap::from([("q".into(), "space & 한글=ok".into())]),
        )
        .await
        .unwrap()
        .0;
    assert_eq!(form.status, 200);
    let result = core
        .run_op(&definition, "catalog", &BTreeMap::new())
        .await
        .unwrap()
        .0
        .body
        .unwrap();
    assert_eq!(result["items"].as_array().unwrap().len(), 2);
    assert_eq!(result["items"][0]["title"], "One title");
    assert_eq!(result["items"][0]["url"], format!("{base}/one"));
    assert_eq!(result["next_url"], format!("{base}/catalog?page=2"));
    let mut broken = definition.clone();
    broken.operations[1].path = "/unexpected-html".into();
    core.install(&broken).unwrap();
    assert_eq!(
        core.run_op(&broken, "catalog", &BTreeMap::new())
            .await
            .err()
            .unwrap()
            .public_code,
        "response_mismatch"
    );
    server.abort();
}

#[tokio::test]
async fn graphql_errors_are_not_success_and_mutations_never_run_as_reads() {
    let calls = Arc::new(AtomicUsize::new(0));
    let counted = calls.clone();
    let (base, server) = host(Router::new().route("/api", post(move |Json(body): Json<Value>| {
        let counted = counted.clone();
        async move {
            counted.fetch_add(1, Ordering::SeqCst);
            if body["variables"]["fail"] == true {
                Json(json!({"data":{"items":null},"errors":[{"message":"Synthetic missing field"}]}))
            } else {
                Json(json!({"data":{"items":[{"id":"one"}]}}))
            }
        }
    }))).await;
    let core = runtime();
    let definition = spec::parse(json!({"site":{"name":"fixture","base_url":base},"operations":[{"name":"read-items","method":"POST","path":"/api","effect":"read","evidence":"Synthetic documented GraphQL endpoint","body":{"json":{"query":{"default":"query { items { id } }"},"variables":{"type":"json","default":{"fail":false}}}},"response":{"format":"json","required_pointers":["/data"]}}]}).to_string().as_bytes()).unwrap();
    core.install(&definition).unwrap();
    assert_eq!(
        core.run_op(&definition, "read-items", &BTreeMap::new())
            .await
            .unwrap()
            .0
            .body
            .unwrap()["data"]["items"][0]["id"],
        "one"
    );
    let failure = core
        .run_op(
            &definition,
            "read-items",
            &BTreeMap::from([("variables".into(), json!({"fail":true}).to_string())]),
        )
        .await
        .err()
        .unwrap();
    assert_eq!(failure.public_code, "query_failed");
    assert_eq!(
        core.state.read().unwrap().sites["fixture"]
            .verified
            .get("read-items"),
        Some(&false)
    );
    assert!(
        core.run_op(
            &definition,
            "read-items",
            &BTreeMap::from([("query".into(), "mutation { destroyAll }".into())])
        )
        .await
        .is_err()
    );
    assert_eq!(calls.load(Ordering::SeqCst), 2);
    server.abort();
}
#[tokio::test]
async fn websites_stay_read_only_until_the_user_allows_changes() {
    let count = Arc::new(AtomicUsize::new(0));
    let c = count.clone();
    let (url, server) = host(Router::new().route(
        "/items",
        post(move || {
            let c = c.clone();
            async move {
                c.fetch_add(1, Ordering::SeqCst);
                Json(json!({"created":true}))
            }
        }),
    ))
    .await;
    let core = runtime();
    let sp = spec_for(&url);
    core.install(&sp).unwrap();
    let args = BTreeMap::from([("title".into(), "A concrete task".into())]);
    let Err(error) = core.run_op(&sp, "create-item", &args).await else {
        panic!("change ran while read-only")
    };
    assert_eq!(error.public_code, "writes_disabled");
    assert!(core.policy.pending().unwrap().is_empty());
    let mcp = crate::guarded_mcp::Server {
        core: core.clone(),
        sites_only: false,
        only_site: None,
    };
    let names = |server: &crate::guarded_mcp::Server| -> Vec<String> {
        server
            .tools()
            .unwrap()
            .into_iter()
            .map(|tool| tool.name.to_string())
            .collect()
    };
    assert!(names(&mcp).contains(&"fixture_read-items".to_string()));
    assert!(!names(&mcp).contains(&"fixture_create-item".to_string()));
    allow_changes(&core, &sp.site.name);
    assert!(names(&mcp).contains(&"fixture_create-item".to_string()));
    assert!(core.run_op(&sp, "create-item", &args).await.is_err());
    let review = core.policy.pending().unwrap().remove(0);
    let request = core.policy.review(&review.id).unwrap().request.unwrap();
    // Turning changes off again also stops an approval issued while they were on.
    core.state
        .update(|d| {
            d.sites.get_mut(&sp.site.name).unwrap().writes = false;
            Ok(())
        })
        .unwrap();
    let Err(error) = core.execute(request, Some(&review.id)).await else {
        panic!("change ran after being turned off")
    };
    assert_eq!(error.public_code, "writes_disabled");
    assert_eq!(count.load(Ordering::SeqCst), 0);
    server.abort();
}
#[tokio::test]
async fn check_replays_recorded_reads_and_tells_sign_in_from_site_changes() {
    // 0 = healthy, 1 = signed out, 2 = changed response shape.
    let mode = Arc::new(AtomicUsize::new(0));
    let m = mode.clone();
    let (url, server) = host(
        Router::new()
            .route(
                "/items",
                get(move || {
                    let m = m.clone();
                    async move {
                        match m.load(Ordering::SeqCst) {
                            0 => (StatusCode::OK, Json(json!({"items":[{"id":"private-7"}]}))),
                            1 => (StatusCode::UNAUTHORIZED, Json(json!({}))),
                            _ => (StatusCode::OK, Json(json!({"results":[]}))),
                        }
                    }
                }),
            )
            .route(
                "/items/{id}",
                get(
                    |axum::extract::Path(id): axum::extract::Path<String>| async move {
                        Json(json!({"id":id}))
                    },
                ),
            ),
    )
    .await;
    let core = runtime();
    let sp = spec::parse(json!({"spec_version":1,"site":{"name":"fixture","base_url":url},"operations":[
        {"name":"list-items","method":"GET","path":"/items","effect":"read","evidence":"Synthetic fixture documentation","response":{"format":"json","required_pointers":["/items"]}},
        {"name":"read-item","method":"GET","path":"/items/{id}","effect":"read","evidence":"Synthetic fixture documentation","params":{"id":{"type":"string","required":true}},"response":{"format":"json","required_pointers":["/id"]}}
    ]}).to_string().as_bytes()).unwrap();
    core.install(&sp).unwrap();
    core.state
        .update(|d| {
            d.sites.entry("fixture".into()).or_default().smoke = vec![
                crate::smoke::ReadCheck {
                    action: "list-items".into(),
                    inputs: BTreeMap::new(),
                },
                crate::smoke::ReadCheck {
                    action: "read-item".into(),
                    inputs: BTreeMap::from([(
                        "id".into(),
                        "$result:list-items:/items/0/id".into(),
                    )]),
                },
            ];
            Ok(())
        })
        .unwrap();
    let report = core.check_site("fixture").await.unwrap();
    assert_eq!(report.verdict, "ok");
    assert_eq!(report.checks.len(), 2);
    assert!(report.checks.iter().all(|check| check.outcome == "ok"));
    mode.store(1, Ordering::SeqCst);
    assert_eq!(
        core.check_site("fixture").await.unwrap().verdict,
        "signed_out"
    );
    mode.store(2, Ordering::SeqCst);
    let report = core.check_site("fixture").await.unwrap();
    assert_eq!(report.verdict, "site_changed");
    assert_eq!(report.checks[1].outcome, "skipped");
    server.abort();
}
