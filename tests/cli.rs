//! Exercise the shipped command surface against a synthetic website, never a live account.
use axum::{
    Json, Router,
    extract::Query,
    http::{HeaderMap, StatusCode},
    routing::get,
};
use hycli::{runtime::Runtime, spec, util};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    path::Path,
    process::Output,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};

async fn cli(root: &Path, args: &[&str]) -> Output {
    let executable = std::env::var_os("HYCLI_TEST_EXECUTABLE")
        .unwrap_or_else(|| env!("CARGO_BIN_EXE_hycli").into());
    tokio::process::Command::new(executable)
        .args(args)
        .env("HYCLI_DATA_DIR", root)
        .env("HYCLI_TEST_DATA", root)
        .env("HYCLI_SPECS", root.join("specs"))
        .env("HYCLI_KB", root.join("kb.db"))
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .await
        .unwrap()
}
fn ok(out: Output) -> Value {
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice(&out.stdout).unwrap()
}

#[tokio::test]
async fn session_cli_inspects_observes_and_verifies_without_exposing_secrets() {
    use axum::response::IntoResponse;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let requests = Arc::new(AtomicUsize::new(0));
    let request_count = requests.clone();
    let unsafe_requests = Arc::new(AtomicUsize::new(0));
    let unsafe_count = unsafe_requests.clone();
    let response_mode = Arc::new(AtomicUsize::new(0));
    let mode = response_mode.clone();
    let router = Router::new()
        .route("/current-context",get(move |headers:HeaderMap,Query(query):Query<BTreeMap<String,String>>| {
            let mode=mode.clone();
            async move {
                if mode.load(Ordering::SeqCst)==2 {
                    return axum::response::Redirect::temporary("/context?variables=%7B%22action%22%3A%22delete%22%7D").into_response();
                }
                if headers.get("cookie").and_then(|v|v.to_str().ok()).is_some_and(|v|v.contains("sid=synthetic-session-secret")) {
                    let id=if mode.load(Ordering::SeqCst)==1 {json!("synthetic-session-secret")}
                        else if query.get("numeric").is_some_and(|value|value=="1") {json!(1)} else {json!("person-one")};
                    (StatusCode::OK,Json(json!({"id":id,"name":"Private Person","synthetic-session-secret":1}))).into_response()
                } else {(StatusCode::UNAUTHORIZED,Json(json!({"error":"sign in"}))).into_response()}
            }
        }))
        .route(
            "/public-profile",
            get(|| async { Json(json!({"id":"person-one","name":"Private Person"})) }),
        )
        .route(
            "/redirect-context",
            get(|| async {
                axum::response::Redirect::temporary(
                    "/context?variables=%7B%22action%22%3A%22delete%22%7D",
                )
            }),
        )
        .route(
            "/context",
            get(move || {
                let count = unsafe_count.clone();
                async move {
                    count.fetch_add(1, Ordering::SeqCst);
                    Json(json!({"id":"unexpected"}))
                }
            }),
        )
        .route(
            "/credential-echo",
            get(|headers: HeaderMap| async move {
                if headers
                    .get("cookie")
                    .and_then(|v| v.to_str().ok())
                    .is_some_and(|v| v.contains("sid=synthetic-session-secret"))
                {
                    (
                        StatusCode::OK,
                        Json(json!({"id":"synthetic-session-secret","name":"Private Person"})),
                    )
                } else {
                    (StatusCode::UNAUTHORIZED, Json(json!({"error":"sign in"})))
                }
            }),
        )
        .layer(axum::middleware::from_fn(
            move |request: axum::extract::Request, next: axum::middleware::Next| {
                let count = request_count.clone();
                async move {
                    count.fetch_add(1, Ordering::SeqCst);
                    next.run(request).await
                }
            },
        ));
    let server = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join(".cache/cli-tests")
        .join(util::id());
    let core = Runtime::open(&root, None, None, false, true, false).unwrap();
    let cookies = hycli::accounts::parse_cookies(
        &json!([{"name":"sid","value":"synthetic-session-secret","domain":"127.0.0.1","path":"/"},{"name":"showTips","value":"1","domain":"127.0.0.1","path":"/"},{"name":"user_id","value":"person-one","domain":"127.0.0.1","path":"/"}])
            .to_string(),
        &url,
    )
    .unwrap();
    let account = core
        .connect_account("", &url, cookies, "Fixture", "Manual session", "")
        .unwrap();
    let inspect = ok(cli(&root, &["accounts", "inspect", &account.id]).await);
    assert!(
        inspect["cookie_names"]
            .as_array()
            .unwrap()
            .contains(&json!("sid"))
    );
    assert!(
        inspect["cookie_names"]
            .as_array()
            .unwrap()
            .contains(&json!("showTips"))
    );
    assert!(!inspect.to_string().contains("synthetic-session-secret"));
    assert!(
        !core
            .state
            .account(&account.id)
            .unwrap()
            .unwrap()
            .verified_identity
    );

    let no_local = cli(
        &root,
        &[
            "accounts",
            "observe",
            &account.id,
            &format!("{url}/current-context"),
        ],
    )
    .await;
    assert!(!no_local.status.success());
    assert!(String::from_utf8_lossy(&no_local.stderr).contains("unsafe_target"));
    let missing = cli(&root, &["accounts", "inspect", "missing-account"]).await;
    assert!(!missing.status.success());
    assert!(String::from_utf8_lossy(&missing.stderr).contains("not_found"));
    let selected = ok(cli(
        &root,
        &[
            "accounts",
            "--site",
            &account.site_id,
            "inspect",
            &account.id,
        ],
    )
    .await);
    assert_eq!(selected["account_id"], account.id);
    let recipe_path = root.join("recipe.json");
    let recipe = json!({"identity_url":format!("{url}/current-context"),"identity":{"user_id":"/id","name":"/name"},"evidence":"observed_browser_request"});
    std::fs::write(&recipe_path, recipe.to_string()).unwrap();
    let recipe_file = recipe_path.to_str().unwrap();
    let unobserved = cli(
        &root,
        &[
            "--allow-local",
            "accounts",
            "verify",
            &account.id,
            "--recipe",
            recipe_file,
        ],
    )
    .await;
    assert!(!unobserved.status.success());
    assert!(String::from_utf8_lossy(&unobserved.stderr).contains("unsupported_evidence"));
    assert_eq!(requests.load(Ordering::SeqCst), 0);

    let observe = ok(cli(
        &root,
        &[
            "--allow-local",
            "accounts",
            "observe",
            &account.id,
            &format!("{url}/current-context"),
        ],
    )
    .await);
    assert_eq!(observe["observation"]["status"], 200);
    assert_eq!(observe["observation"]["response_shape"]["id"], "string");
    assert!(!observe.to_string().contains("person-one"));
    assert!(!observe.to_string().contains("Private Person"));
    assert!(!observe.to_string().contains("synthetic-session-secret"));
    assert!(
        !core
            .state
            .account(&account.id)
            .unwrap()
            .unwrap()
            .verified_identity
    );
    let verified = ok(cli(
        &root,
        &[
            "--allow-local",
            "accounts",
            "verify",
            &account.id,
            "--recipe",
            recipe_file,
        ],
    )
    .await);
    assert_eq!(verified["verified_identity"], true);
    assert_eq!(verified["user_id"], "person-one");
    assert!(!verified.to_string().contains("synthetic-session-secret"));
    let redirect = ok(cli(
        &root,
        &[
            "--allow-local",
            "accounts",
            "observe",
            &account.id,
            &format!("{url}/redirect-context"),
        ],
    )
    .await);
    assert_eq!(redirect["observation"]["status"], 307);
    assert_eq!(unsafe_requests.load(Ordering::SeqCst), 0);
    let before = requests.load(Ordering::SeqCst);
    for (suffix, expected_error) in [
        ("/graphql?extensions=%7B%7D", "unsafe_target"),
        (
            "/current-context?query=reference&payload=%7B%22query%22%3A%22mutation%20%7B%20erase%20%7D%22%7D",
            "unsafe_target",
        ),
        (
            "/current-context?query=reference&extensions=%7B%7D",
            "unsafe_target",
        ),
        ("/gql?extensions=%7B%7D", "unsafe_target"),
        (
            "/current-context?payload=%7B%22query%22%3A%22mutation%20%7B%20erase%20%7D%22%7D",
            "unsafe_target",
        ),
        (
            "/current-context?variables=%7B%22action%22%3A%22delete%22%7D",
            "unsafe_target",
        ),
        (
            "/current-context?variables=%7B%22action%22%3A%22delete%22%7D&variables=%7B%7D",
            "unsafe_target",
        ),
        (
            "/current-context?query=mutation%20%7B%20erase%20%7D",
            "unsafe_target",
        ),
        (
            "/graphql?query=fragment%20Part%20on%20User%20%7B%20id%20%7D",
            "unsafe_target",
        ),
        ("/current-context?token=synthetic-session-secret", "bad_url"),
        ("/logout", "unsafe_target"),
    ] {
        let rejected = cli(
            &root,
            &[
                "--allow-local",
                "accounts",
                "observe",
                &account.id,
                &format!("{url}{suffix}"),
            ],
        )
        .await;
        assert!(!rejected.status.success(), "allowed {suffix}");
        assert!(
            String::from_utf8_lossy(&rejected.stderr).contains(expected_error),
            "{suffix}: {}",
            String::from_utf8_lossy(&rejected.stderr)
        );
    }
    let other_origin = cli(
        &root,
        &[
            "--allow-local",
            "accounts",
            "observe",
            &account.id,
            "https://example.com/current-context",
        ],
    )
    .await;
    assert!(!other_origin.status.success());
    assert!(String::from_utf8_lossy(&other_origin.stderr).contains("unsafe_target"));
    let wrong_site = cli(
        &root,
        &[
            "accounts",
            "--site",
            "not-this-site",
            "inspect",
            &account.id,
        ],
    )
    .await;
    assert!(!wrong_site.status.success());
    assert!(String::from_utf8_lossy(&wrong_site.stderr).contains("not_found"));
    let mut unsupported = recipe.clone();
    unsupported["identity_url"] = json!(format!("{url}/current-context?scope=current"));
    std::fs::write(&recipe_path, unsupported.to_string()).unwrap();
    let wrong_query = cli(
        &root,
        &[
            "--allow-local",
            "accounts",
            "verify",
            &account.id,
            "--recipe",
            recipe_file,
        ],
    )
    .await;
    assert!(!wrong_query.status.success());
    assert!(String::from_utf8_lossy(&wrong_query.stderr).contains("unsupported_evidence"));
    unsupported = recipe.clone();
    unsupported["evidence"] = json!("invented-evidence");
    std::fs::write(&recipe_path, unsupported.to_string()).unwrap();
    let no_evidence = cli(
        &root,
        &[
            "--allow-local",
            "accounts",
            "verify",
            &account.id,
            "--recipe",
            recipe_file,
        ],
    )
    .await;
    assert!(!no_evidence.status.success());
    assert!(String::from_utf8_lossy(&no_evidence.stderr).contains("unsupported_evidence"));
    assert_eq!(requests.load(Ordering::SeqCst), before);
    ok(cli(
        &root,
        &[
            "--allow-local",
            "accounts",
            "observe",
            &account.id,
            &format!("{url}/public-profile"),
        ],
    )
    .await);
    let mut public = recipe.clone();
    public["identity_url"] = json!(format!("{url}/public-profile"));
    std::fs::write(&recipe_path, public.to_string()).unwrap();
    let rejected = cli(
        &root,
        &[
            "--allow-local",
            "accounts",
            "verify",
            &account.id,
            "--recipe",
            recipe_file,
        ],
    )
    .await;
    assert!(!rejected.status.success());
    assert!(String::from_utf8_lossy(&rejected.stderr).contains("identity_unavailable"));
    assert_eq!(
        ok(cli(&root, &["accounts", "inspect", &account.id]).await)["saved_recipe"]["identity_url"],
        recipe["identity_url"]
    );
    let plain_query = ok(cli(
        &root,
        &[
            "--allow-local",
            "accounts",
            "observe",
            &account.id,
            &format!("{url}/current-context?query=reference"),
        ],
    )
    .await);
    assert_eq!(plain_query["observation"]["status"], 200);
    let explicit_query = ok(cli(
        &root,
        &[
            "--allow-local",
            "accounts",
            "observe",
            &account.id,
            &format!("{url}/current-context?query=query%20%7B%20viewer%20%7B%20id%20%7D%20%7D"),
        ],
    )
    .await);
    assert_eq!(explicit_query["observation"]["status"], 200);
    // Preserve evidence for multiple query-key shapes at the same path.
    ok(cli(
        &root,
        &[
            "--allow-local",
            "accounts",
            "observe",
            &account.id,
            &format!("{url}/current-context?scope=one&fields=id"),
        ],
    )
    .await);
    ok(cli(
        &root,
        &[
            "--allow-local",
            "accounts",
            "observe",
            &account.id,
            &format!("{url}/current-context"),
        ],
    )
    .await);
    let mut with_query = recipe.clone();
    with_query["identity_url"] = json!(format!("{url}/current-context?fields=core&scope=current"));
    std::fs::write(&recipe_path, with_query.to_string()).unwrap();
    assert_eq!(
        ok(cli(
            &root,
            &[
                "--allow-local",
                "accounts",
                "verify",
                &account.id,
                "--recipe",
                recipe_file
            ]
        )
        .await)["verified_identity"],
        true
    );
    std::fs::write(&recipe_path, recipe.to_string()).unwrap();
    ok(cli(
        &root,
        &[
            "--allow-local",
            "accounts",
            "verify",
            &account.id,
            "--recipe",
            recipe_file,
        ],
    )
    .await);
    // Numeric identity must not collide with an unrelated one-character preference cookie.
    ok(cli(
        &root,
        &[
            "--allow-local",
            "accounts",
            "observe",
            &account.id,
            &format!("{url}/current-context?numeric=1"),
        ],
    )
    .await);
    let mut numeric = recipe.clone();
    numeric["identity_url"] = json!(format!("{url}/current-context?numeric=1"));
    std::fs::write(&recipe_path, numeric.to_string()).unwrap();
    let numeric_view = ok(cli(
        &root,
        &[
            "--allow-local",
            "accounts",
            "verify",
            &account.id,
            "--recipe",
            recipe_file,
        ],
    )
    .await);
    assert_eq!(numeric_view["user_id"], "1");
    assert_eq!(
        ok(cli(
            &root,
            &["--allow-local", "accounts", "refresh", &account.id]
        )
        .await)["user_id"],
        "1"
    );
    std::fs::write(&recipe_path, recipe.to_string()).unwrap();
    ok(cli(
        &root,
        &[
            "--allow-local",
            "accounts",
            "verify",
            &account.id,
            "--recipe",
            recipe_file,
        ],
    )
    .await);
    // A saved recipe is executable local state, but diagnostics must redact a secret prefix.
    let mut private_recipe = recipe.clone();
    private_recipe["headers"] = json!([{"header":"X-Context","source":"cookie","key":"sid","prefix":"synthetic-session-secret"}]);
    std::fs::write(&recipe_path, private_recipe.to_string()).unwrap();
    ok(cli(
        &root,
        &[
            "--allow-local",
            "accounts",
            "verify",
            &account.id,
            "--recipe",
            recipe_file,
        ],
    )
    .await);
    let inspected = ok(cli(&root, &["accounts", "inspect", &account.id]).await);
    assert!(!inspected.to_string().contains("synthetic-session-secret"));
    std::fs::write(&recipe_path, recipe.to_string()).unwrap();
    ok(cli(
        &root,
        &[
            "--allow-local",
            "accounts",
            "verify",
            &account.id,
            "--recipe",
            recipe_file,
        ],
    )
    .await);
    // Identity extraction must reject credentials before storing AccountView metadata.
    ok(cli(
        &root,
        &[
            "--allow-local",
            "accounts",
            "observe",
            &account.id,
            &format!("{url}/credential-echo"),
        ],
    )
    .await);
    let mut echoed = recipe.clone();
    echoed["identity_url"] = json!(format!("{url}/credential-echo"));
    std::fs::write(&recipe_path, echoed.to_string()).unwrap();
    let rejected_echo = cli(
        &root,
        &[
            "--allow-local",
            "accounts",
            "verify",
            &account.id,
            "--recipe",
            recipe_file,
        ],
    )
    .await;
    assert!(!rejected_echo.status.success());
    assert!(String::from_utf8_lossy(&rejected_echo.stderr).contains("identity_unavailable"));
    assert!(!String::from_utf8_lossy(&rejected_echo.stdout).contains("synthetic-session-secret"));
    let preserved = ok(cli(&root, &["accounts", "inspect", &account.id]).await);
    assert_eq!(
        preserved["saved_recipe"]["identity_url"],
        recipe["identity_url"]
    );
    // Bad recipe input is rejected before any network request.
    let before_invalid = requests.load(Ordering::SeqCst);
    let invalid = root.join("invalid-recipe.json");
    let no_file = cli(
        &root,
        &[
            "--allow-local",
            "accounts",
            "verify",
            &account.id,
            "--recipe",
            invalid.to_str().unwrap(),
        ],
    )
    .await;
    assert!(!no_file.status.success());
    assert!(String::from_utf8_lossy(&no_file.stderr).contains("data_invalid"));
    for data in [
        "{".to_string(),
        format!("{}{}", recipe, " ".repeat(128 * 1024)),
    ] {
        std::fs::write(&invalid, data).unwrap();
        let bad = cli(
            &root,
            &[
                "--allow-local",
                "accounts",
                "verify",
                &account.id,
                "--recipe",
                invalid.to_str().unwrap(),
            ],
        )
        .await;
        assert!(!bad.status.success());
        assert!(String::from_utf8_lossy(&bad.stderr).contains("data_invalid"));
    }
    assert_eq!(requests.load(Ordering::SeqCst), before_invalid);
    let refreshed = ok(cli(
        &root,
        &["--allow-local", "accounts", "refresh", &account.id],
    )
    .await);
    assert_eq!(refreshed["verified_identity"], true);
    // A previously verified endpoint cannot redirect refresh into a changed request.
    response_mode.store(2, Ordering::SeqCst);
    let redirected_refresh = cli(
        &root,
        &["--allow-local", "accounts", "refresh", &account.id],
    )
    .await;
    assert!(!redirected_refresh.status.success());
    assert_eq!(unsafe_requests.load(Ordering::SeqCst), 0);
    response_mode.store(1, Ordering::SeqCst);
    let unsafe_refresh = cli(
        &root,
        &["--allow-local", "accounts", "refresh", &account.id],
    )
    .await;
    assert!(!unsafe_refresh.status.success());
    assert!(String::from_utf8_lossy(&unsafe_refresh.stderr).contains("identity_unavailable"));
    assert!(!String::from_utf8_lossy(&unsafe_refresh.stdout).contains("synthetic-session-secret"));
    let listed = ok(cli(&root, &["accounts"]).await);
    let refreshed_view = listed
        .as_array()
        .unwrap()
        .iter()
        .find(|view| view["id"] == account.id)
        .unwrap();
    assert_eq!(refreshed_view["verified_identity"], false);
    assert!(refreshed_view.get("user_id").is_none());

    server.abort();
    drop(core);
    std::fs::remove_dir_all(root).unwrap();
}

#[tokio::test]
async fn session_cli_does_not_store_new_short_credential_sources_as_identity() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let router = Router::new()
        .route(
            "/profile",
            get(|headers: HeaderMap| async move {
                if headers.get("x-context").and_then(|v| v.to_str().ok()) == Some("ABC123") {
                    (StatusCode::OK, Json(json!({"id":"short-account"})))
                } else {
                    (StatusCode::UNAUTHORIZED, Json(json!({"error":"sign in"})))
                }
            }),
        )
        .route(
            "/echo",
            get(|headers: HeaderMap| async move {
                if headers.get("x-context").and_then(|v| v.to_str().ok()) == Some("ABC123") {
                    (StatusCode::OK, Json(json!({"id":"ABC123"})))
                } else {
                    (StatusCode::UNAUTHORIZED, Json(json!({"error":"sign in"})))
                }
            }),
        );
    let server = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join(".cache/cli-tests")
        .join(util::id());
    let core = Runtime::open(&root, None, None, false, true, false).unwrap();
    let cookies = hycli::accounts::parse_cookies(
        &json!([{"name":"ctx","value":"ABC123","domain":"127.0.0.1","path":"/"}]).to_string(),
        &url,
    )
    .unwrap();
    let account = core
        .connect_account("", &url, cookies, "Fixture", "Short source", "")
        .unwrap();
    ok(cli(
        &root,
        &[
            "--allow-local",
            "accounts",
            "observe",
            &account.id,
            &format!("{url}/profile"),
        ],
    )
    .await);
    let file = root.join("recipe.json");
    let mut recipe = json!({"identity_url":format!("{url}/profile"),"headers":[{"header":"X-Context","source":"cookie","key":"ctx","prefix":""}],"identity":{"user_id":"/id"},"evidence":"observed_browser_request"});
    std::fs::write(&file, recipe.to_string()).unwrap();
    assert_eq!(
        ok(cli(
            &root,
            &[
                "--allow-local",
                "accounts",
                "verify",
                &account.id,
                "--recipe",
                file.to_str().unwrap()
            ]
        )
        .await)["user_id"],
        "short-account"
    );
    // Keep the actual captured header but no old recipe source that could mask the new-source bug.
    recipe["headers"] = json!([]);
    std::fs::write(&file, recipe.to_string()).unwrap();
    ok(cli(
        &root,
        &[
            "--allow-local",
            "accounts",
            "verify",
            &account.id,
            "--recipe",
            file.to_str().unwrap(),
        ],
    )
    .await);
    ok(cli(
        &root,
        &[
            "--allow-local",
            "accounts",
            "observe",
            &account.id,
            &format!("{url}/echo"),
        ],
    )
    .await);
    for (source, key) in [("cookie", "ctx"), ("captured_header", "X-cOnTeXt")] {
        let mut candidate = recipe.clone();
        candidate["identity_url"] = json!(format!("{url}/echo"));
        candidate["headers"] =
            json!([{"header":"X-Context","source":source,"key":key,"prefix":""}]);
        std::fs::write(&file, candidate.to_string()).unwrap();
        let rejected = cli(
            &root,
            &[
                "--allow-local",
                "accounts",
                "verify",
                &account.id,
                "--recipe",
                file.to_str().unwrap(),
            ],
        )
        .await;
        assert!(
            !rejected.status.success(),
            "accepted {source}:{key} credential echo"
        );
        assert!(String::from_utf8_lossy(&rejected.stderr).contains("identity_unavailable"));
        let accounts = ok(cli(&root, &["accounts"]).await);
        assert!(!accounts.to_string().contains("ABC123"));
        assert_eq!(
            accounts
                .as_array()
                .unwrap()
                .iter()
                .find(|view| view["id"] == account.id)
                .unwrap()["user_id"],
            "short-account"
        );
        assert_eq!(
            ok(cli(&root, &["accounts", "inspect", &account.id]).await)["saved_recipe"]["identity_url"],
            recipe["identity_url"]
        );
    }
    server.abort();
    drop(core);
    std::fs::remove_dir_all(root).unwrap();
}

#[tokio::test]
async fn session_cli_rejects_sibling_origins_and_mismatched_saved_sources() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join(".cache/cli-tests")
        .join(util::id());
    let core = Runtime::open(&root, None, None, false, true, false).unwrap();
    let account = core
        .connect_account(
            "",
            "https://app.example.test",
            hycli::accounts::parse_cookies(&json!([{"name":"sid","value":"synthetic-origin-check","domain":"app.example.test","path":"/"}]).to_string(), "https://app.example.test").unwrap(),
            "Fixture",
            "Origin",
            "",
        )
        .unwrap();
    let sibling = cli(
        &root,
        &[
            "accounts",
            "observe",
            &account.id,
            "https://api.example.test/current-context",
        ],
    )
    .await;
    assert!(!sibling.status.success());
    assert!(String::from_utf8_lossy(&sibling.stderr).contains("unsafe_target"));
    let wrong_site = cli(
        &root,
        &[
            "accounts",
            "--site",
            "different-owner",
            "inspect",
            &account.id,
        ],
    )
    .await;
    assert!(!wrong_site.status.success());
    assert!(String::from_utf8_lossy(&wrong_site.stderr).contains("not_found"));
    core.update_credential(&account.id, |credential| {
        credential.origin = "https://other.example.test".into();
        Ok(())
    })
    .unwrap();
    for args in [
        vec!["accounts", "inspect", &account.id],
        vec![
            "accounts",
            "observe",
            &account.id,
            "https://app.example.test/current-context",
        ],
        vec!["accounts", "refresh", &account.id],
    ] {
        let rejected = cli(&root, &args).await;
        assert!(!rejected.status.success());
        assert!(String::from_utf8_lossy(&rejected.stderr).contains("unsafe_target"));
    }
    drop(core);
    std::fs::remove_dir_all(root).unwrap();
}

#[tokio::test]
async fn session_cli_browser_diagnostics_fail_closed_without_requests_or_state_changes() {
    let requests = Arc::new(AtomicUsize::new(0));
    let count = requests.clone();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let router = Router::new().route(
        "/account",
        get(move || {
            let count = count.clone();
            async move {
                count.fetch_add(1, Ordering::SeqCst);
                Json(json!({"id":"synthetic"}))
            }
        }),
    );
    let server = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join(".cache/cli-tests")
        .join(util::id());
    let core = Runtime::open(&root, None, None, false, true, false).unwrap();
    let cookies = hycli::accounts::parse_cookies(
        &json!([{"name":"sid","value":"synthetic-session","domain":"127.0.0.1","path":"/"}])
            .to_string(),
        &url,
    )
    .unwrap();
    let account = core
        .connect_account("", &url, cookies, "Fixture", "Diagnostic", "")
        .unwrap();
    let before_credential = serde_json::to_value(core.credential(&account.id).unwrap()).unwrap();
    let before_accounts = ok(cli(&root, &["accounts"]).await);
    let rejected = cli(
        &root,
        &[
            "--allow-local",
            "accounts",
            "observe",
            &account.id,
            &format!("{url}/account"),
            "--browser",
        ],
    )
    .await;
    assert!(!rejected.status.success());
    assert!(String::from_utf8_lossy(&rejected.stderr).contains("browser_diagnostic_unsupported"));
    assert_eq!(requests.load(Ordering::SeqCst), 0);
    assert_eq!(
        serde_json::to_value(core.credential(&account.id).unwrap()).unwrap(),
        before_credential
    );
    assert_eq!(ok(cli(&root, &["accounts"]).await), before_accounts);
    // The supported alternative remains functional after the rejected mode.
    let observed = ok(cli(
        &root,
        &[
            "--allow-local",
            "accounts",
            "observe",
            &account.id,
            &format!("{url}/account"),
        ],
    )
    .await);
    assert_eq!(observed["observation"]["status"], 200);
    assert_eq!(requests.load(Ordering::SeqCst), 1);
    server.abort();
    drop(core);
    std::fs::remove_dir_all(root).unwrap();
}

#[tokio::test]
async fn installed_cli_help_inputs_accounts_and_failures_are_real_command_contracts() {
    let reads = Arc::new(AtomicUsize::new(0));
    let read_count = reads.clone();
    let router = Router::new()
        .route(
            "/items",
            get(move |Query(query): Query<BTreeMap<String, String>>| {
                let count = read_count.clone();
                async move {
                    count.fetch_add(1, Ordering::SeqCst);
                    Json(json!({"inputs":query}))
                }
            })
            .post(|Json(body): Json<Value>| async move {
                Json(json!({"created":true,"received":body}))
            }),
        )
        .route(
            "/account",
            get(|headers: HeaderMap| async move {
                let cookie = headers
                    .get("cookie")
                    .and_then(|v| v.to_str().ok())
                    .unwrap_or("");
                if cookie.contains("session=first-test-cookie") {
                    (
                        StatusCode::OK,
                        Json(json!({"id":"one","name":"First account"})),
                    )
                } else if cookie.contains("session=second-test-cookie") {
                    (
                        StatusCode::OK,
                        Json(json!({"id":"two","name":"Second account"})),
                    )
                } else {
                    (StatusCode::UNAUTHORIZED, Json(json!({"error":"sign in"})))
                }
            }),
        )
        .route("/expired", get(|| async { StatusCode::UNAUTHORIZED }))
        .route("/missing", get(|| async { StatusCode::NOT_FOUND }))
        .route(
            "/unexpected",
            get(|| async { axum::response::Html("<html><form>Sign in</form></html>") }),
        )
        .route(
            "/spa",
            get(|| async {
                axum::response::Html(
                    r#"<html><head><title>SPA CLI</title></head><body>
                    <script id="__NEXT_DATA__" type="application/json">{"props":{"pageProps":{"ok":true}}}</script>
                    <script>fetch('/items');</script>
                    </body></html>"#,
                )
            }),
        );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let server = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join(".cache/cli-tests")
        .join(util::id());
    let core = Runtime::open(&root, None, None, false, true, false).unwrap();
    // Global observation defaults must not become a positional website filter.
    assert_eq!(ok(cli(&root, &["describe"]).await)["sites"], json!([]));
    assert_eq!(ok(cli(&root, &["accounts"]).await), json!([]));
    let value = json!({"site":{"name":"sample","title":"Sample library","base_url":url},
    "auth":[{"name":"browser","kind":"browser"}],
    "operations":[
        {"name":"create-item","method":"POST","path":"/items","effect":"write","evidence":"Synthetic documentation","body":{"json":{"title":{"type":"string","required":true}}}},
        {"name":"search","desc":"Search the sample collection.","method":"GET","path":"/items","effect":"read","evidence":"Synthetic documentation", "query":{
            "q":{"type":"string","required":true}, "sort-order":{"type":"string"}, "sort_order":{"type":"string"},
            "include_archived":{"type":"bool"}, "page":{"type":"int","default":1}}},
        {"name":"read-account","method":"GET","path":"/account","auth":"browser","effect":"read","evidence":"Synthetic documentation"},
        {"name":"read-public","method":"GET","path":"/items","effect":"read","evidence":"Synthetic documentation"},
        {"name":"read-expired","method":"GET","path":"/expired","effect":"read","evidence":"Synthetic documentation"},
        {"name":"read-missing","method":"GET","path":"/missing","effect":"read","evidence":"Synthetic documentation"},
        {"name":"read-unexpected","method":"GET","path":"/unexpected","effect":"read","evidence":"Synthetic documentation","response":{"format":"json","required_pointers":["/items"]}}
    ]});
    core.install(&spec::parse(value.to_string().as_bytes()).unwrap())
        .unwrap();
    let one = core.connect_account("sample", &url, hycli::accounts::parse_cookies(&json!([{"name":"session","value":"first-test-cookie","domain":"127.0.0.1","path":"/"}]).to_string(), &url).unwrap(), "Fixture", "First", &format!("{url}/account")).unwrap();
    core.refresh_account(&one.id).await.unwrap();
    let two = core.connect_account("sample", &url, hycli::accounts::parse_cookies(&json!([{"name":"session","value":"second-test-cookie","domain":"127.0.0.1","path":"/"}]).to_string(), &url).unwrap(), "Fixture", "Second", &format!("{url}/account")).unwrap();
    core.refresh_account(&two.id).await.unwrap();
    assert_eq!(
        ok(cli(&root, &["describe"]).await)["sites"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    let accounts = ok(cli(&root, &["accounts"]).await);
    assert_eq!(accounts.as_array().unwrap().len(), 2);
    assert_eq!(
        ok(cli(&root, &["accounts", "--site", "sample"]).await),
        accounts
    );
    assert_eq!(
        ok(cli(&root, &["accounts", "--site", "missing"]).await),
        json!([])
    );
    for flags in [
        vec!["sample"],
        vec!["sample", "--help"],
        vec!["sample", "search", "--help"],
    ] {
        let output = cli(&root, &flags).await;
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let text = String::from_utf8(output.stdout).unwrap();
        assert!(text.contains("Sample library") && text.contains("Usage:"));
        if flags.len() == 3 {
            assert!(
                text.contains("--q <string>")
                    && text.contains("required;")
                    && text.contains("default: 1")
                    && text.contains("Example:\n  hycli sample search --q \"search-term\"")
            );
        } else {
            assert!(text.contains("What you can do:") && text.contains("Examples:"));
        }
    }
    assert_eq!(
        reads.load(Ordering::SeqCst),
        0,
        "help must not send a website request"
    );
    let site_desc = ok(cli(&root, &["describe", "sample"]).await);
    assert!(
        site_desc["user_guide"]["headline"]
            .as_str()
            .unwrap()
            .contains("Sample library")
    );
    assert_eq!(
        site_desc["user_guide"]["read_capabilities"]
            .as_array()
            .unwrap()
            .len(),
        6
    );
    assert_eq!(
        site_desc["user_guide"]["write_capabilities"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    let metadata = ok(cli(&root, &["describe", "sample", "search"]).await);
    assert!(
        metadata["inputs"]
            .as_array()
            .unwrap()
            .iter()
            .any(|i| i["id"] == "q" && i["required"] == true)
    );
    let typo_flag = cli(&root, &["sample", "search", "--sort-ordr", "desc"]).await;
    assert!(!typo_flag.status.success());
    assert!(String::from_utf8_lossy(&typo_flag.stderr).contains("Did you mean --sort-order?"));
    let typo_action = cli(&root, &["sample", "serch"]).await;
    assert!(!typo_action.status.success());
    assert!(String::from_utf8_lossy(&typo_action.stderr).contains("Did you mean 'search'?"));
    for invalid in [
        vec!["sample", "search", "--q"],
        vec!["sample", "search", "--q", "--page", "2"],
        vec!["sample", "search", "--q", "a", "--q", "b"],
        vec!["sample", "search", "--page", "invalid"],
    ] {
        assert!(!cli(&root, &invalid).await.status.success());
    }
    let output = ok(cli(
        &root,
        &[
            "sample",
            "search",
            "--allow-local",
            "--q=--literal",
            "--sort-order",
            "desc",
            "--sort_order",
            "asc",
            "--include-archived",
        ],
    )
    .await);
    assert_eq!(
        output["inputs"],
        json!({"q":"--literal","sort-order":"desc","sort_order":"asc","include_archived":"true","page":"1"})
    );
    let exact = ok(cli(
        &root,
        &[
            "--allow-local",
            "run",
            "sample",
            "search",
            "--arg",
            "q=hello=world",
            "--arg",
            "sort-order=desc",
        ],
    )
    .await);
    assert_eq!(exact["inputs"]["q"], "hello=world");
    let first = ok(cli(
        &root,
        &[
            "--allow-local",
            "run",
            "sample",
            "read-account",
            "--account",
            &one.id,
        ],
    )
    .await);
    let second = ok(cli(
        &root,
        &[
            "sample",
            "read-account",
            "--allow-local",
            "--hycli-account",
            &two.id,
        ],
    )
    .await);
    assert_eq!(first["id"], "one");
    assert_eq!(second["id"], "two");
    let refused_write = cli(
        &root,
        &[
            "sample",
            "create-item",
            "--allow-local",
            "--title",
            "New book",
        ],
    )
    .await;
    assert_eq!(refused_write.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&refused_write.stderr).contains("writes_disabled"));
    let allowed = ok(cli(&root, &["writes", "sample", "allow"]).await);
    assert_eq!(allowed["site"], "sample");
    assert_eq!(allowed["writes"], true);
    let created = ok(cli(
        &root,
        &[
            "sample",
            "create-item",
            "--allow-local",
            "--title",
            "New book",
        ],
    )
    .await);
    assert_eq!(created["created"], true);
    assert_eq!(created["received"]["title"], "New book");
    // Explicitly choose a public input-free read; the first definition is a write.
    let mut sp = core.spec("sample").unwrap();
    sp.health = Some(spec::Health {
        op: "read-public".into(),
    });
    core.install(&sp).unwrap();
    let report = ok(cli(&root, &["--allow-local", "check", "sample"]).await);
    assert_eq!(report[0]["checks"][0]["action"], "read-public");
    assert_eq!(report[0]["checks"][0]["outcome"], "ok");
    for (action, code, exit_code) in [
        ("read-missing", "request_failed", 1),
        ("read-unexpected", "response_mismatch", 1),
        ("read-expired", "auth_required", 3),
    ] {
        let output = cli(&root, &["--allow-local", "sample", action]).await;
        assert_eq!(
            output.status.code(),
            Some(exit_code),
            "{action}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(output.stdout.is_empty());
        assert!(String::from_utf8_lossy(&output.stderr).contains(code));
        let job = core
            .state
            .read()
            .unwrap()
            .jobs
            .into_iter()
            .find(|j| j.action_id == action)
            .unwrap();
        assert_eq!(job.status, "failed");
        assert!(
            !cli(&root, &["jobs", "show", &job.id, "--watch"])
                .await
                .status
                .success()
        );
    }
    let draft_path = root.join("spa-draft.yaml");
    let recon_out = ok(cli(
        &root,
        &[
            "--allow-local",
            "--site",
            "spa",
            "recon",
            &format!("{url}/spa"),
            "--draft",
            draft_path.to_str().unwrap(),
        ],
    )
    .await);
    assert_eq!(recon_out["site_id"], "spa");
    assert_eq!(recon_out["title"], "SPA CLI");
    assert!(
        recon_out["bundle_routes"]
            .as_array()
            .unwrap()
            .iter()
            .any(|r| r == "GET /items")
    );
    let validated = ok(cli(
        &root,
        &[
            "--allow-local",
            "spec",
            "validate",
            draft_path.to_str().unwrap(),
        ],
    )
    .await);
    assert_eq!(validated["status"], "ok");
    std::fs::write(root.join("specs/broken.yaml"), "site: [broken").unwrap();
    let listed = cli(&root, &["spec", "ls"]).await;
    assert!(String::from_utf8_lossy(&listed.stderr).contains("cannot load broken"));
    assert!(!cli(&root, &["broken", "--help"]).await.status.success());
    server.abort();
    drop(core);
    let _ = std::fs::remove_dir_all(&root);
}

#[tokio::test]
async fn feedback_is_advertised_and_composes_a_public_issue_url_offline() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join(".cache/cli-tests")
        .join(util::id());
    let help = cli(&root, &["--help"]).await;
    let text = String::from_utf8_lossy(&help.stdout);
    assert!(help.status.success(), "{}", text);
    assert!(
        text.contains("feedback"),
        "feedback must be listed in the command surface: {text}"
    );
    // Opening the URL is the only side effect, so the URL contract is checkable without a browser.
    let url = hycli::feedback::issue_url();
    assert!(
        url.starts_with("https://github.com/Hybirdss/Hycli/issues/new?"),
        "{url}"
    );
    assert!(url.contains(env!("CARGO_PKG_VERSION")), "{url}");
}

// Use a real child process and a synthetic launcher, never the developer's browser.
#[cfg(unix)]
#[tokio::test]
async fn feedback_keeps_json_and_reports_only_launcher_start() {
    use std::os::unix::fs::PermissionsExt;
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join(".cache/cli-tests/feedback path with spaces")
        .join(util::id());
    let bin = root.join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    let launcher = bin.join(if cfg!(target_os = "macos") {
        "open"
    } else {
        "xdg-open"
    });
    let capture = root.join("url.txt");
    let release = root.join("release");
    let done = root.join("done");
    for (script, requested) in [
        (None, false),
        (
            Some(
                "#!/bin/sh\nprintf '%s' \"$1\" > \"$HYCLI_FEEDBACK_CAPTURE\"\nprintf done > \"$HYCLI_FEEDBACK_DONE\"\nexit 0\n",
            ),
            true,
        ),
        (
            Some(
                "#!/bin/sh\nprintf '%s' \"$1\" > \"$HYCLI_FEEDBACK_CAPTURE\"\nprintf done > \"$HYCLI_FEEDBACK_DONE\"\nexit 3\n",
            ),
            true,
        ),
        (
            Some(
                "#!/bin/sh\nprintf '%s' \"$1\" > \"$HYCLI_FEEDBACK_CAPTURE\"\nwhile [ ! -e \"$HYCLI_FEEDBACK_RELEASE\" ]; do /bin/sleep 0.05; done\nprintf done > \"$HYCLI_FEEDBACK_DONE\"\n",
            ),
            true,
        ),
    ] {
        let _ = std::fs::remove_file(&capture);
        let _ = std::fs::remove_file(&release);
        let _ = std::fs::remove_file(&done);
        if let Some(script) = script {
            std::fs::write(&launcher, script).unwrap();
            std::fs::set_permissions(&launcher, std::fs::Permissions::from_mode(0o700)).unwrap();
        }
        let mut command = tokio::process::Command::new(env!("CARGO_BIN_EXE_hycli"));
        command
            .args(["--format", "json", "feedback"])
            .env("PATH", &bin)
            .env("HOME", &root)
            .env("HYCLI_DATA_DIR", root.join("data"))
            .env("HYCLI_FEEDBACK_CAPTURE", &capture)
            .env("HYCLI_FEEDBACK_RELEASE", &release)
            .env("HYCLI_FEEDBACK_DONE", &done)
            .kill_on_drop(true);
        let out = tokio::time::timeout(std::time::Duration::from_secs(5), command.output()).await;
        // Release even on failure so the synthetic opener cannot outlive this test indefinitely.
        std::fs::write(&release, b"done").unwrap();
        let out = out
            .expect("feedback must not wait for the browser to close")
            .unwrap();
        let result = ok(out);
        assert_eq!(result["launch_requested"], requested);
        assert!(
            result.get("opened").is_none(),
            "a launcher cannot attest that a page opened"
        );
        let url = result["url"].as_str().unwrap();
        let parsed = url::Url::parse(url).unwrap();
        assert_eq!(parsed.host_str(), Some("github.com"));
        assert_eq!(parsed.path(), "/Hybirdss/Hycli/issues/new");
        let query: std::collections::HashMap<_, _> = parsed.query_pairs().into_owned().collect();
        assert!(query["title"].contains(env!("CARGO_PKG_VERSION")));
        assert!(query["body"].contains(std::env::consts::OS));
        if requested {
            for _ in 0..100 {
                if done.exists() {
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            }
            assert!(
                done.exists(),
                "synthetic launcher must finish before cleanup"
            );
            assert_eq!(std::fs::read_to_string(&capture).unwrap(), url);
        }
    }
    let _ = std::fs::remove_dir_all(root);
}

#[cfg(windows)]
#[tokio::test]
async fn feedback_does_not_claim_a_page_opened_on_windows() {
    // Build a tiny native launcher that records arguments and exits unsuccessfully.
    // Copy the CLI beside it so Windows finds the synthetic launcher in the application
    // directory before its system directories. Keep SystemRoot and other OS settings intact.
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join(".cache/cli-tests/feedback path with spaces")
        .join(util::id());
    std::fs::create_dir_all(&root).unwrap();
    let mut compiler =
        tokio::process::Command::new(std::env::var_os("RUSTC").unwrap_or_else(|| "rustc".into()));
    compiler
        .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/feedback_opener.rs"))
        .arg("-o")
        .arg(root.join("rundll32.exe"));
    if let Some(target) = std::env::var_os("CARGO_BUILD_TARGET") {
        compiler.arg("--target").arg(target);
    }
    let compiled = tokio::time::timeout(
        std::time::Duration::from_secs(120),
        compiler.kill_on_drop(true).output(),
    )
    .await
    .unwrap()
    .unwrap();
    assert!(
        compiled.status.success(),
        "{}",
        String::from_utf8_lossy(&compiled.stderr)
    );
    let capture = root.join("url.txt");
    let binary = root.join("hycli.exe");
    std::fs::copy(env!("CARGO_BIN_EXE_hycli"), &binary).unwrap();
    let out = tokio::time::timeout(
        std::time::Duration::from_secs(10),
        tokio::process::Command::new(&binary)
            .args(["--format", "json", "feedback"])
            .current_dir(&root)
            .env("HYCLI_FEEDBACK_CAPTURE", &capture)
            .env("PATH", &root)
            .env("HYCLI_DATA_DIR", root.join("data"))
            .kill_on_drop(true)
            .output(),
    )
    .await
    .unwrap()
    .unwrap();
    let result = ok(out);
    assert_eq!(result["launch_requested"], true);
    assert!(result.get("opened").is_none());
    let url = url::Url::parse(result["url"].as_str().unwrap()).unwrap();
    assert_eq!(url.host_str(), Some("github.com"));
    assert_eq!(url.path(), "/Hybirdss/Hycli/issues/new");
    for _ in 0..500 {
        if std::fs::read_to_string(&capture)
            .is_ok_and(|args| args == format!("url.dll,FileProtocolHandler\n{url}"))
        {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    assert_eq!(
        std::fs::read_to_string(&capture).unwrap(),
        format!("url.dll,FileProtocolHandler\n{url}")
    );
    // The rejected launcher can still be exiting after its capture was written.
    for _ in 0..500 {
        if std::fs::remove_dir_all(&root).is_ok() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    assert!(!root.exists(), "synthetic Windows launcher did not exit");
}

#[tokio::test]
async fn explicit_pagination_explains_handles_and_version_mismatches() {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join(".cache/cli-tests")
        .join(util::id());
    std::fs::create_dir_all(&root).unwrap();
    let file = root.join("fixture.json");
    let mut value = json!({"spec_version":3,"site":{"name":"cursor-help","base_url":"https://example.com"},"operations":[{"name":"list-items","method":"GET","path":"/items","effect":"read","evidence":"Synthetic documented pagination","pagination":{"input":"page","query":"pageToken","next":"/nextPageToken"}}]});
    std::fs::write(&file, value.to_string()).unwrap();
    ok(cli(
        &root,
        &[
            "--format",
            "json",
            "spec",
            "install",
            file.to_str().unwrap(),
        ],
    )
    .await);
    let action = ok(cli(
        &root,
        &["--format", "json", "describe", "cursor-help", "list-items"],
    )
    .await);
    assert!(
        action["inputs"][0]["hint"]
            .as_str()
            .unwrap()
            .contains("next_cursor")
    );
    assert!(
        action["inputs"][0]["hint"]
            .as_str()
            .unwrap()
            .contains("15 minutes")
    );
    assert!(action["output"].as_str().unwrap().contains("next_cursor"));
    let help = cli(&root, &["cursor-help", "list-items", "--help"]).await;
    assert!(help.status.success());
    assert!(String::from_utf8_lossy(&help.stdout).contains("next_cursor"));
    value["spec_version"] = json!(2);
    std::fs::write(&file, value.to_string()).unwrap();
    let invalid = cli(
        &root,
        &[
            "--format",
            "json",
            "spec",
            "validate",
            file.to_str().unwrap(),
        ],
    )
    .await;
    assert!(!invalid.status.success());
    let issues: Value = serde_json::from_slice(&invalid.stdout).unwrap();
    assert!(issues["issues"].as_array().unwrap().iter().any(|issue| {
        issue["field"] == "spec_version"
            && issue["remedy"]
                .as_str()
                .is_some_and(|v| v.contains("spec_version to 3"))
    }));
}

// Exercise the shipped entry point, not just an async future-size estimate.
// Linux can reproduce the small default Windows main stack without changing
// the product's stack reservation or weakening the native Windows contract.
#[cfg(target_os = "linux")]
#[tokio::test]
async fn installed_cli_read_runs_on_a_one_mebibyte_main_stack() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let router = Router::new().route(
        "/items",
        get(|| async { Json(json!({"items":["synthetic-item"]})) }),
    );
    let server = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join(".cache/cli-tests")
        .join(util::id());
    let core = Runtime::open(&root, None, None, false, true, false).unwrap();
    core.install(&spec::parse(json!({"site":{"name":"bounded-stack","base_url":origin},"operations":[{"name":"read-items","method":"GET","path":"/items","effect":"read","evidence":"Owned synthetic fixture"}]}).to_string().as_bytes()).unwrap()).unwrap();
    let executable = std::env::var_os("HYCLI_TEST_EXECUTABLE")
        .unwrap_or_else(|| env!("CARGO_BIN_EXE_hycli").into());
    let out = tokio::process::Command::new("sh")
        .args([
            "-c",
            "ulimit -c 0; ulimit -s 1024 || exit; exec \"$@\"",
            "bounded-stack-fixture",
        ])
        .arg(executable)
        .args(["bounded-stack", "read-items", "--allow-local"])
        .env("HYCLI_DATA_DIR", &root)
        .env("HYCLI_TEST_DATA", &root)
        .env("HYCLI_SPECS", root.join("specs"))
        .env("HYCLI_KB", root.join("kb.db"))
        .output()
        .await
        .unwrap();
    assert_eq!(ok(out)["items"], json!(["synthetic-item"]));
    server.abort();
    drop(core);
    std::fs::remove_dir_all(root).unwrap();
}

#[tokio::test]
async fn jobs_show_preserves_provider_failure_codes_notes_and_exit_classes() {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join(".cache/provider-cli-tests")
        .join(util::id());
    let core = Runtime::open(&root, None, None, false, true, false).unwrap();
    for (code, exit) in [
        ("ai_response_incomplete", 1),
        ("ai_response_invalid", 1),
        ("ai_connection_failed", 1),
        ("rate_limited", 4),
    ] {
        let mut job = hycli::state::Job::new("prepare", "en");
        job.status = "failed".into();
        job.error_code = code.into();
        job.note(&format!("agent_failed_{code}"), "explore", 0, "");
        core.state.add_job(job.clone()).unwrap();
        let result = cli(&root, &["jobs", "show", &job.id]).await;
        assert_eq!(
            result.status.code(),
            Some(exit),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        let value: Value = serde_json::from_slice(&result.stdout).unwrap();
        assert_eq!(value["error_code"], code);
        assert_eq!(value["notes"][0]["code"], format!("agent_failed_{code}"));
        assert!(!value["notes"][0]["message"].as_str().unwrap().is_empty());
        assert!(!String::from_utf8_lossy(&result.stderr).contains("private-"));
    }
}
