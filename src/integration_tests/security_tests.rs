use super::harness::*;
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
    // Candidate verification during preparation must never execute a write.
    assert!(
        core.verify_candidate_with_inputs(&sp, "create-item", "", args.clone())
            .await
            .is_err()
    );
    assert_eq!(count.load(Ordering::SeqCst), 0);
    // Direct execution runs when writes are enabled.
    assert_eq!(
        core.run_op(&sp, "create-item", &args)
            .await
            .unwrap()
            .0
            .status,
        200
    );
    assert_eq!(count.load(Ordering::SeqCst), 1);
    // Explicit dashboard approval tokens remain single-use and bound to exact inputs.
    let req = core
        .request(&sp, "create-item", args.clone(), None)
        .unwrap();
    let review = core.approval(req, "en").unwrap();
    let request = core.policy.review(&review.id).unwrap().request.unwrap();
    let mut changed = request.clone();
    changed
        .inputs
        .insert("title".into(), "A different task".into());
    assert!(core.execute(changed, Some(&review.id)).await.is_err());
    assert_eq!(count.load(Ordering::SeqCst), 1);
    assert_eq!(
        core.execute(request.clone(), Some(&review.id))
            .await
            .unwrap()
            .0
            .status,
        200
    );
    assert_eq!(count.load(Ordering::SeqCst), 2);
    assert!(core.execute(request, Some(&review.id)).await.is_err());
    assert_eq!(count.load(Ordering::SeqCst), 2);
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
