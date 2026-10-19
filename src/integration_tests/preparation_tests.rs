use super::harness::*;
#[tokio::test]
async fn recon_mines_ssr_and_js_bundles_and_v2_pipeline_executes_preflight_and_derived_auth() {
    let (base, server) = host(
        Router::new()
            .route(
                "/",
                get(|| async {
                    (
                        [("content-type", "text/html; charset=utf-8")],
                        r#"<!doctype html><html><head><title>Synthetic Studio</title>
                        <meta name="csrf-token" content="csrf-live-777">
                        <script src="/_next/static/chunks/app.js"></script></head>
                        <body>
                        <form action="/api/v2/clips/quick-create" method="post">
                            <input name="title" type="text">
                            <input name="genre" type="text">
                        </form>
                        <script id="__NEXT_DATA__" type="application/json">{"buildId":"build-2026","props":{"pageProps":{"clips":[{"id":"clip-1","title":"First Song"}]}}}</script></body></html>"#,
                    )
                }),
            )
            .route(
                "/_next/static/chunks/app.js",
                get(|| async {
                    (
                        [("content-type", "application/javascript")],
                        r#"
                        export async function loadFeed() {
                            return fetch('/api/v2/feed');
                        }
                        export async function publishClip(id, payload) {
                            return axios.post(`/api/v2/clips/${id}/publish`, payload);
                        }
                        "#,
                    )
                }),
            )
            .route(
                "/api/v2/feed/{build_id}",
                get(
                    |axum::extract::Path(build_id): axum::extract::Path<String>,
                     headers: HeaderMap| async move {
                        let xsrf = headers
                            .get("x-xsrf-token")
                            .and_then(|v| v.to_str().ok())
                            .unwrap_or("");
                        let csrf = headers
                            .get("x-csrf-token")
                            .and_then(|v| v.to_str().ok())
                            .unwrap_or("");
                        if xsrf != "ct0-cookie-val"
                            || csrf != "csrf-live-777"
                            || build_id != "build-2026"
                        {
                            return (
                                StatusCode::UNAUTHORIZED,
                                [("content-type", "application/json")],
                                r#"{"error":"missing derived or preflight parameters"}"#.to_string(),
                            );
                        }
                        (
                            StatusCode::OK,
                            [("content-type", "application/json")],
                            format!(
                                ")]}}'\n{{\"data\":{{\"items\":[{{\"id\":\"feed-99\",\"build_id\":\"{build_id}\",\"echo_csrf\":\"csrf-live-777\",\"echo_xsrf\":\"ct0-cookie-val\"}}]}}}}"
                            ),
                        )
                    },
                ),
            ),
    )
    .await;

    let core = runtime();
    let (report, draft) = core
        .recon("studio", &format!("{base}/"), 8)
        .await
        .expect("deterministic recon");
    assert_eq!(report["title"], "Synthetic Studio");
    let bundle_routes = report["bundle_routes"].as_array().unwrap();
    assert!(
        bundle_routes.iter().any(|r| r == "GET /api/v2/feed"),
        "missing GET /api/v2/feed in {bundle_routes:?}"
    );
    assert!(
        bundle_routes
            .iter()
            .any(|r| r == "POST /api/v2/clips/{id}/publish"),
        "missing POST /api/v2/clips/{{id}}/publish in {bundle_routes:?}"
    );
    assert!(
        draft.operations.iter().any(|o| o.name == "read-next-data"),
        "missing read-next-data operation"
    );
    let form_write_op = draft
        .operations
        .iter()
        .find(|o| o.method == "POST" && o.path == "/api/v2/clips/quick-create")
        .expect("synthesized HTML form POST operation");
    assert_eq!(form_write_op.effect, Effect::Write);
    assert!(
        form_write_op
            .body
            .as_ref()
            .is_some_and(|b| b.form.contains_key("title") && b.form.contains_key("genre"))
    );

    let v2_spec = spec::parse(
        json!({
            "spec_version": 2,
            "site": {
                "name": "studio",
                "title": "Synthetic Studio",
                "base_url": base,
            },
            "auth": [
                {
                    "name": "xsrf",
                    "kind": "derived",
                    "header": "X-XSRF-Token",
                    "value_from": "cookie:ct0"
                }
            ],
            "operations": [
                {
                    "name": "ssr-clips",
                    "method": "GET",
                    "path": "/",
                    "effect": "read",
                    "evidence": "Synthetic SSR read",
                    "response": {
                        "format": "json",
                        "ssr_json": "__NEXT_DATA__",
                        "extract_pointer": "/props/pageProps/clips",
                        "required_pointers": ["/0/id"]
                    }
                },
                {
                    "name": "protected-feed",
                    "method": "GET",
                    "path": "/api/v2/feed/{{step.csrf.build_id}}",
                    "effect": "read",
                    "evidence": "Synthetic feed read",
                    "auth": "xsrf",
                    "preflight": [
                        {
                            "name": "csrf",
                            "path": "/",
                            "extract": {
                                "token": "css:meta[name='csrf-token']@content",
                                "build_id": "ssr:__NEXT_DATA__#/buildId"
                            }
                        }
                    ],
                    "headers": {
                        "X-CSRF-Token": "{{step.csrf.token}}"
                    },
                    "response": {
                        "format": "json",
                        "strip_prefix": ")]}'\n",
                        "extract_pointer": "/data/items",
                        "required_pointers": ["/0/id"]
                    }
                }
            ]
        })
        .to_string()
        .as_bytes(),
    )
    .unwrap();
    core.install(&v2_spec).unwrap();

    let ssr_res = core
        .run_op(&v2_spec, "ssr-clips", &BTreeMap::new())
        .await
        .unwrap()
        .0
        .body
        .unwrap();
    assert_eq!(ssr_res[0]["id"], "clip-1");

    let host_str = url::Url::parse(&base)
        .unwrap()
        .host_str()
        .unwrap()
        .to_string();
    let account = core
        .connect_account(
            "studio",
            &base,
            vec![crate::accounts::Cookie {
                name: "ct0".into(),
                value: "ct0-cookie-val".into(),
                domain: host_str,
                path: "/".into(),
                secure: false,
                http_only: false,
                host_only: true,
                expires: None,
                same_site: String::new(),
                store_id: String::new(),
                partition_key: None,
            }],
            "firefox",
            "default",
            "",
        )
        .unwrap();

    let feed_res = core
        .run_op_for_account(
            &v2_spec,
            "protected-feed",
            &BTreeMap::new(),
            Some(&account.id),
        )
        .await
        .unwrap()
        .0
        .body
        .unwrap();
    assert_eq!(feed_res[0]["id"], "feed-99");
    assert_eq!(feed_res[0]["build_id"], "build-2026");
    assert_eq!(feed_res[0]["echo_csrf"], "[private]");
    assert_eq!(feed_res[0]["echo_xsrf"], "[private]");

    let (encoded_url, _) = v2_spec
        .build_request(
            v2_spec.op("protected-feed").unwrap(),
            &BTreeMap::from([(
                "step.csrf.build_id".into(),
                "../../evil@attacker.com".into(),
            )]),
        )
        .unwrap();
    assert_eq!(
        encoded_url,
        format!("{base}/api/v2/feed/..%2F..%2Fevil%40attacker.com")
    );
    server.abort();
}

#[tokio::test]
async fn desktop_shutdown_preserves_active_work_and_rejects_later_requests() {
    let core = runtime();
    let job = crate::state::Job::new("prepare", "en");
    core.state.add_job(job.clone()).unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let web = crate::dashboard::Web::new(core.clone(), port).unwrap();
    let launch = web.launch.clone();
    let shutdown = web.shutdown.clone();
    let server = tokio::spawn(async move {
        axum::serve(listener, crate::dashboard::router(web))
            .await
            .unwrap();
    });
    let client = reqwest::Client::builder().no_proxy().build().unwrap();
    let base = format!("http://127.0.0.1:{port}");
    let session = client
        .get(format!("{base}/api/session?launch={launch}"))
        .send()
        .await
        .unwrap();
    let cookie = session.headers()["set-cookie"]
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_owned();
    let csrf = session.json::<Value>().await.unwrap()["csrf"]
        .as_str()
        .unwrap()
        .to_owned();
    let request = || {
        client
            .post(format!("{base}/api/shutdown"))
            .header("cookie", &cookie)
            .header("x-hycli-csrf", &csrf)
    };
    assert_eq!(request().send().await.unwrap().status(), 409);
    assert!(!*shutdown.borrow());
    assert_eq!(core.state.job(&job.id).unwrap().status, "queued");
    core.state
        .update_job(&job.id, |job| job.status = "completed".into())
        .unwrap();
    assert_eq!(request().send().await.unwrap().status(), 200);
    assert!(*shutdown.borrow());
    assert_eq!(
        client
            .patch(format!("{base}/api/settings"))
            .header("cookie", &cookie)
            .header("x-hycli-csrf", &csrf)
            .json(&json!({"locale":"ko"}))
            .send()
            .await
            .unwrap()
            .status(),
        503
    );
    server.abort();
}

#[tokio::test]
async fn resilience_against_false_403_lockout_multi_cookie_preflight_and_embedded_data() {
    let html_map = crate::html::extract_ssr_map(
        r#"<html><body><script type="application/json" data-target="react-app.embeddedData">{"payload":{"plan":"free","limit":2000}}</script></body></html>"#,
    );
    assert_eq!(
        html_map["react-app.embeddedData"]["payload"]["plan"],
        "free"
    );

    let routes =
        crate::discovery::bundle_routes(r#"fetch("/_api/blogEntries;blogId=123&limit=20");"#);
    assert!(routes.contains(&("GET".into(), "/_api/blogEntries".into())));

    let (base, server) = host(
        Router::new()
            .route(
                "/api/forbidden-subresource",
                get(|| async {
                    (
                        StatusCode::FORBIDDEN,
                        [
                            ("server", "cloudflare"),
                            ("cf-ray", "synthetic-ray"),
                            ("content-type", "application/json"),
                        ],
                        Json(json!({"error":"insufficient_scope"})),
                    )
                }),
            )
            .route(
                "/boot",
                get(|| async {
                    let mut headers = HeaderMap::new();
                    headers.append(
                        axum::http::header::SET_COOKIE,
                        "theme=dark; Path=/".parse().unwrap(),
                    );
                    headers.append(
                        axum::http::header::SET_COOKIE,
                        "xsrf_rot=rotated-second-cookie; Path=/".parse().unwrap(),
                    );
                    (headers, axum::response::Html("<html>ok</html>"))
                }),
            )
            .route(
                "/api/items",
                get(|headers: HeaderMap| async move {
                    assert_eq!(
                        headers.get("sec-fetch-mode").and_then(|v| v.to_str().ok()),
                        Some("cors")
                    );
                    assert!(headers.get("referer").is_some());
                    assert_eq!(
                        headers.get("x-xsrf-token").and_then(|v| v.to_str().ok()),
                        Some("rotated-second-cookie")
                    );
                    assert_eq!(
                        headers.get("authorization").and_then(|v| v.to_str().ok()),
                        Some("Bearer nested-jwt-value")
                    );
                    Json(json!({"items":[{"id":"ok"}]}))
                }),
            ),
    )
    .await;

    let core = runtime();
    let sp = spec::parse(
        json!({
            "spec_version": 2,
            "site": { "name": "resilience", "base_url": base },
            "auth": [
                {
                    "name": "nested-jwt",
                    "kind": "derived",
                    "header": "Authorization",
                    "prefix": "Bearer ",
                    "value_from": "storage:persist:auth#/state/token"
                }
            ],
            "operations": [
                {
                    "name": "forbidden-check",
                    "method": "GET",
                    "path": "/api/forbidden-subresource",
                    "auth": "nested-jwt",
                    "effect": "read",
                    "evidence": "Synthetic forbidden endpoint"
                },
                {
                    "name": "list-items",
                    "method": "GET",
                    "path": "/api/items",
                    "auth": "nested-jwt",
                    "effect": "read",
                    "evidence": "Synthetic protected items",
                    "preflight": [
                        {
                            "name": "boot",
                            "path": "/boot",
                            "extract": {
                                "xsrf": "cookie:xsrf_rot"
                            }
                        }
                    ],
                    "headers": {
                        "x-xsrf-token": "{{step.boot.xsrf}}"
                    },
                    "response": {
                        "format": "json",
                        "required_pointers": ["/items/0/id"]
                    }
                }
            ]
        })
        .to_string()
        .as_bytes(),
    )
    .unwrap();
    core.install(&sp).unwrap();

    let cred = crate::accounts::Credential {
        origin: base.clone(),
        local_storage: BTreeMap::from([(
            "persist:auth".into(),
            serde_json::to_string(&"{\"state\":{\"token\":\"nested-jwt-value\"}}").unwrap(),
        )]),
        ..Default::default()
    };
    let acct = core
        .register_browser_credential("resilience", cred, "Chromium", "Default")
        .unwrap();

    // 1. Hitting an application 403 behind Cloudflare headers must not brick the gate or expire the account.
    let first_err = core
        .run_op_for_account(&sp, "forbidden-check", &BTreeMap::new(), Some(&acct.id))
        .await
        .err()
        .unwrap();
    assert_eq!(first_err.http_status, 403);
    assert_eq!(
        core.state.read().unwrap().accounts[&acct.id].status,
        "connected"
    );

    // 2. Subsequent operation with multi-Set-Cookie preflight and double-encoded storage token succeeds.
    let (second_res, _) = core
        .run_op_for_account(&sp, "list-items", &BTreeMap::new(), Some(&acct.id))
        .await
        .unwrap();
    assert_eq!(second_res.status, 200);
    server.abort();
}

#[tokio::test]
async fn strategy_circuit_breaker_isolates_challenged_transport_without_origin_lockout() {
    let (base, server) = host(
        Router::new()
            .route(
                "/api/challenged",
                get(|| async {
                    (
                        StatusCode::FORBIDDEN,
                        [
                            ("server", "cloudflare"),
                            ("content-type", "text/html; charset=utf-8"),
                        ],
                        "<html><title>Just a moment...</title><script src=\"/cdn-cgi/challenge-platform/h/b/orchestrate/chl_page/v1\"></script></html>",
                    )
                }),
            )
            .route(
                "/api/healthy",
                get(|| async { Json(json!({"ok": true, "rows": [{"id": 1}]})) }),
            ),
    )
    .await;

    let core = runtime();
    let sp = spec::parse(
        json!({
            "spec_version": 2,
            "site": { "name": "adaptive-site", "base_url": base },
            "operations": [
                {
                    "name": "challenged-read",
                    "method": "GET",
                    "path": "/api/challenged",
                    "effect": "read",
                    "evidence": "Challenged endpoint"
                },
                {
                    "name": "browser-fetch-op",
                    "method": "GET",
                    "path": "/api/healthy",
                    "transport": "browser-fetch",
                    "effect": "read",
                    "evidence": "In-page browser fetch endpoint"
                },
                {
                    "name": "healthy-read",
                    "method": "GET",
                    "path": "/api/healthy",
                    "effect": "read",
                    "evidence": "Healthy endpoint on same origin",
                    "response": {
                        "format": "json",
                        "required_pointers": ["/rows/0/id"]
                    }
                }
            ]
        })
        .to_string()
        .as_bytes(),
    )
    .unwrap();
    assert!(sp.validate().is_empty());
    policy::validate_spec(&sp).unwrap();
    core.install(&sp).unwrap();

    // 1. Hitting a Cloudflare challenge on /api/challenged records the blocked direct strategy.
    let err = core
        .run_op_for_account(&sp, "challenged-read", &BTreeMap::new(), None)
        .await
        .err()
        .unwrap();
    assert_eq!(err.public_code, "blocked_by_site");

    let challenged_url = format!("{}/api/challenged", base.trim_end_matches('/'));
    let blocked_key = policy::Policy::strategy_key("GET", &challenged_url, "direct", "");
    assert_eq!(
        core.policy
            .is_strategy_blocked(&blocked_key)
            .unwrap()
            .as_deref(),
        Some("cloudflare")
    );

    // 2. Origin-wide gate is NOT paused: another operation on the same host succeeds immediately.
    let (res, _) = core
        .run_op_for_account(&sp, "healthy-read", &BTreeMap::new(), None)
        .await
        .unwrap();
    assert_eq!(res.status, 200);
    server.abort();
}

#[test]
fn har_capture_synthesizes_rich_v2_spec_with_query_body_auth_and_pointers() {
    let raw_har = json!({
        "log": {
            "entries": [
                {
                    "request": {
                        "method": "GET",
                        "url": "https://api.example.com/v1/projects/42/issues?page=1&open=true&api_key=secret-ignore",
                        "headers": [
                            { "name": "Cookie", "value": "session=abc" }
                        ],
                        "queryString": [
                            { "name": "page", "value": "1" },
                            { "name": "open", "value": "true" },
                            { "name": "api_key", "value": "secret-ignore" }
                        ]
                    },
                    "response": {
                        "status": 200,
                        "content": {
                            "mimeType": "application/json",
                            "text": "{\"items\":[{\"id\":\"iss-1\"}],\"total\":1}"
                        }
                    }
                },
                {
                    "request": {
                        "method": "POST",
                        "url": "https://api.example.com/v1/projects/42/issues",
                        "headers": [
                            { "name": "X-CSRF-Token", "value": "csrf-123" }
                        ],
                        "postData": {
                            "mimeType": "application/json",
                            "text": "{\"title\":\"Fix bug\",\"priority\":2,\"password\":\"redact\"}"
                        }
                    },
                    "response": {
                        "status": 201,
                        "content": {
                            "mimeType": "application/json",
                            "text": "{\"id\":\"iss-2\",\"ok\":true}"
                        }
                    }
                }
            ]
        }
    });
    let parsed = crate::har::parse(raw_har.to_string().as_bytes()).unwrap();
    let clusters = crate::capture::from_har(&parsed);
    assert_eq!(clusters.len(), 2);

    let draft = crate::capture::draft_spec(
        "example",
        "Example Tracker",
        "https://api.example.com",
        &clusters,
        20,
    )
    .unwrap();
    assert_eq!(draft.spec_version, 2);
    assert!(draft.validate().is_empty());
    policy::validate_spec(&draft).unwrap();
    assert!(
        draft
            .auth
            .iter()
            .any(|a| a.name == "browser" && a.kind == "browser")
    );

    let get_op = draft
        .operations
        .iter()
        .find(|o| o.method == "GET")
        .expect("GET operation");
    assert_eq!(get_op.effect, policy::Effect::Read);
    assert_eq!(get_op.auth, "browser");
    assert!(get_op.params.contains_key("id0"));
    assert_eq!(
        get_op.query.get("page").map(|p| p.kind.as_str()),
        Some("int")
    );
    assert_eq!(
        get_op.query.get("open").map(|p| p.kind.as_str()),
        Some("bool")
    );
    assert!(!get_op.query.contains_key("api_key"));
    assert!(
        get_op
            .response
            .as_ref()
            .is_some_and(|r| r.required_pointers.contains(&"/items".to_string()))
    );

    let post_op = draft
        .operations
        .iter()
        .find(|o| o.method == "POST")
        .expect("POST operation");
    assert_eq!(post_op.effect, policy::Effect::Write);
    assert_eq!(post_op.auth, "browser");
    let body = post_op.body.as_ref().expect("POST body");
    assert_eq!(
        body.json.get("title").map(|p| p.kind.as_str()),
        Some("string")
    );
    assert_eq!(
        body.json.get("priority").map(|p| p.kind.as_str()),
        Some("int")
    );
    assert!(!body.json.contains_key("password"));
}
