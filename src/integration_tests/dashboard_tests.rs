use super::harness::*;
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
    assert!(state.db_path().exists());
    let snapshot = state.read().unwrap();
    assert!(serde_json::to_vec(&snapshot).unwrap().len() < 8192);
    assert!(snapshot.jobs[0].result.is_none());
    assert!(state.has_job_result(&id).unwrap());
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
    assert!(!state.has_job_result(&id).unwrap());
    assert!(!folder.join("results").join(format!("{id}.json")).exists());
}

#[test]
fn legacy_json_state_and_results_migrate_atomically_to_sqlite() {
    let folder = root();
    let json_path = folder.join("dashboard.json");
    let results_dir = folder.join("results");
    std::fs::create_dir_all(&results_dir).unwrap();
    let job_id = "fedcba9876543210fedcba9876543210";
    let mut legacy = crate::state::Data::default();
    legacy.settings.locale = "ko".into();
    legacy.settings.theme = "dark".into();
    let mut site = crate::state::SiteMeta::default();
    site.title = "Example Site".into();
    site.writes = true;
    legacy.sites.insert("example".into(), site);
    let mut job = crate::state::Job::new("action", "ko");
    job.id = job_id.into();
    job.site_id = "example".into();
    job.status = "completed".into();
    job.note("started", "", 0, "");
    job.notes.push(crate::state::JobNote {
        seq: 0,
        at: crate::util::now(),
        code: "completed".into(),
        message: "Done".into(),
        agent: String::new(),
        count: 0,
    });
    legacy.jobs.push(job);
    crate::util::atomic_file(&json_path, &serde_json::to_vec(&legacy).unwrap()).unwrap();
    let result_file = results_dir.join(format!("{job_id}.json"));
    crate::util::atomic_file(
        &result_file,
        &serde_json::to_vec(&json!({"migrated": true, "items": [1, 2, 3]})).unwrap(),
    )
    .unwrap();

    let state = crate::state::StateStore::open_with_recovery(&json_path, false).unwrap();
    assert!(!json_path.exists());
    assert!(folder.join("dashboard.json.migrated").exists());
    assert!(!result_file.exists());
    let data = state.read().unwrap();
    assert_eq!(data.settings.locale, "ko");
    assert_eq!(data.settings.theme, "dark");
    assert!(state.site_writes("example").unwrap());
    assert_eq!(data.jobs.len(), 1);
    assert_eq!(data.jobs[0].notes.len(), 2);
    let full_job = state.job(job_id).unwrap();
    assert_eq!(full_job.result.unwrap()["migrated"], json!(true));

    state
        .update(|d| {
            d.settings.locale = "ja".into();
            Ok(())
        })
        .unwrap();
    crate::util::atomic_file(
        &json_path,
        &serde_json::to_vec(&crate::state::Data::default()).unwrap(),
    )
    .unwrap();
    let reopened = crate::state::StateStore::open_with_recovery(&json_path, false).unwrap();
    assert!(!json_path.exists());
    assert_eq!(reopened.settings().unwrap().locale, "ja");
    assert!(reopened.site_writes("example").unwrap());
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
async fn anthropic_registration_normalizes_inputs_and_preserves_saved_connection_on_failure() {
    let calls = Arc::new(AtomicUsize::new(0));
    let observed = calls.clone();
    let (origin, provider_server) = host(Router::new()
        .route("/models", get(move |headers: HeaderMap| {
            let calls = observed.clone();
            async move {
                calls.fetch_add(1, Ordering::SeqCst);
                assert!(["synthetic-registered-key", "synthetic-replacement-key"].contains(&headers["x-api-key"].to_str().unwrap()));
                assert_eq!(headers["anthropic-version"], "2023-06-01");
                Json(json!({"data":[{"id":"fixture-model"}],"has_more":false}))
            }
        }))
        .route("/messages", post(|headers: HeaderMap, Json(body): Json<Value>| async move {
            assert!(["synthetic-registered-key", "synthetic-replacement-key"].contains(&headers["x-api-key"].to_str().unwrap()));
            assert_eq!(body["system"], "Reply with OK only.");
            assert_eq!(body["messages"], json!([{"role":"user","content":"Connection check."}]));
            assert!(!body.to_string().contains("synthetic-registered-key"));
            let failure = match body["model"].as_str().unwrap() {
                "missing-model" => Some((404,"not_found_error")),
                "billing-model" => Some((402,"billing_error")),
                "permission-model" => Some((403,"permission_error")),
                "invalid-model" => Some((400,"invalid_request_error")),
                "fixture-model" => None,
                other => panic!("Unexpected unnormalized model: {other:?}"),
            };
            if let Some((status, kind)) = failure {
                return (StatusCode::from_u16(status).unwrap(), Json(json!({"type":"error","error":{"type":kind,"message":"synthetic-private-provider-body synthetic-registered-key"},"request_id":"synthetic-private-request"})));
            }
            (StatusCode::OK, Json(json!({"stop_reason":"end_turn","content":[{"type":"text","text":"OK"}]})))
        }))).await;
    let mut core = runtime();
    Arc::get_mut(&mut core)
        .unwrap()
        .ai
        .origins
        .insert("anthropic".into(), origin);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let web = crate::dashboard::Web::new(core.clone(), port).unwrap();
    let launch = web.launch.clone();
    let server = tokio::spawn(async move {
        axum::serve(listener, crate::dashboard::router(web))
            .await
            .unwrap();
    });
    let base = format!("http://127.0.0.1:{port}");
    let client = reqwest::Client::new();
    let response = client
        .get(format!("{base}/api/session?launch={launch}"))
        .send()
        .await
        .unwrap();
    let cookie = response.headers()["set-cookie"]
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_owned();
    let csrf = response.json::<Value>().await.unwrap()["csrf"]
        .as_str()
        .unwrap()
        .to_owned();
    let save = |body: Value| {
        client
            .put(format!("{base}/api/providers/anthropic"))
            .header("cookie", &cookie)
            .header("x-hycli-csrf", &csrf)
            .json(&body)
            .send()
    };
    let response = save(json!({"key":" \t synthetic-registered-key\r\n", "model":"  fixture-model \n", "default":true})).await.unwrap();
    assert!(
        response.status().is_success(),
        "{}",
        response.text().await.unwrap()
    );
    let saved = core.state.read().unwrap();
    assert_eq!(saved.providers["anthropic"].model, "fixture-model");
    assert!(saved.providers["anthropic"].connected);
    assert_eq!(
        core.vault.lock().unwrap().get("providers", "anthropic"),
        Some("synthetic-registered-key")
    );
    for (model, code) in [
        ("missing-model", "model_unavailable"),
        ("billing-model", "ai_billing_error"),
        ("permission-model", "ai_permission_denied"),
        ("invalid-model", "ai_request_invalid"),
    ] {
        let response =
            save(json!({"key":"synthetic-replacement-key","model":model,"default":false}))
                .await
                .unwrap();
        assert!(!response.status().is_success());
        let text = response.text().await.unwrap();
        let body: Value = serde_json::from_str(&text).unwrap();
        assert_eq!(body["error"]["code"], code);
        assert!(!text.contains("synthetic-private"));
        assert!(!text.contains("synthetic-registered-key"));
        let current = core.state.read().unwrap();
        assert!(current.providers == saved.providers);
        assert!(current.settings == saved.settings);
        assert_eq!(
            core.vault.lock().unwrap().get("providers", "anthropic"),
            Some("synthetic-registered-key")
        );
    }
    let before_calls = calls.load(Ordering::SeqCst);
    let response = save(json!({"key":"synthetic\nprivate", "model":"fixture-model"}))
        .await
        .unwrap();
    let text = response.text().await.unwrap();
    assert_eq!(
        serde_json::from_str::<Value>(&text).unwrap()["error"]["code"],
        "credentials_invalid"
    );
    assert!(!text.contains("synthetic"));
    assert_eq!(
        calls.load(Ordering::SeqCst),
        before_calls,
        "invalid credential must not reach provider"
    );
    assert!(core.state.read().unwrap().providers == saved.providers);
    assert_eq!(
        core.vault.lock().unwrap().get("providers", "anthropic"),
        Some("synthetic-registered-key")
    );
    server.abort();
    provider_server.abort();
}
