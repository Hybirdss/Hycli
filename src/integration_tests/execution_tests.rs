use super::harness::*;
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
    // Newly installed sites start with writes off until confirmed once for that CLI.
    assert!(names(&mcp).contains(&"fixture_read-items".to_string()));
    assert!(!names(&mcp).contains(&"fixture_create-item".to_string()));
    let Err(error) = core.run_op(&sp, "create-item", &args).await else {
        panic!("change ran while read-only")
    };
    assert_eq!(error.public_code, "writes_disabled");
    assert_eq!(count.load(Ordering::SeqCst), 0);

    // Enabling writes once for this CLI exposes write tools and allows direct execution.
    core.set_writes(&sp.site.name, true).unwrap();
    assert!(names(&mcp).contains(&"fixture_create-item".to_string()));
    assert_eq!(
        core.run_op(&sp, "create-item", &args)
            .await
            .unwrap()
            .0
            .status,
        200
    );
    assert_eq!(count.load(Ordering::SeqCst), 1);

    let req = core
        .request(&sp, "create-item", args.clone(), None)
        .unwrap();
    let review = core.approval(req, "en").unwrap();
    let request = core.policy.review(&review.id).unwrap().request.unwrap();

    // Turning changes off stops both direct write calls and any approval issued while they were on.
    core.set_writes(&sp.site.name, false).unwrap();
    assert!(!names(&mcp).contains(&"fixture_create-item".to_string()));
    let Err(error) = core.run_op(&sp, "create-item", &args).await else {
        panic!("change ran after being turned off")
    };
    assert_eq!(error.public_code, "writes_disabled");
    let Err(error) = core.execute(request, Some(&review.id)).await else {
        panic!("change ran after being turned off")
    };
    assert_eq!(error.public_code, "writes_disabled");
    assert_eq!(count.load(Ordering::SeqCst), 1);
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

#[tokio::test]
async fn write_path_commit_barrier_and_pid_aware_recovery_keep_state_consistent() {
    let preflight_ok = Arc::new(AtomicUsize::new(0));
    let pf = preflight_ok.clone();
    let writes_hit = Arc::new(AtomicUsize::new(0));
    let wh = writes_hit.clone();
    let (base, server) = host(
        Router::new()
            .route(
                "/boot",
                get(move || {
                    let pf = pf.clone();
                    async move {
                        if pf.load(Ordering::SeqCst) == 0 {
                            (StatusCode::SERVICE_UNAVAILABLE, "boot down")
                        } else {
                            (StatusCode::OK, "<meta name=\"csrf\" content=\"tok-1\">")
                        }
                    }
                }),
            )
            .route(
                "/items",
                post(move || {
                    let wh = wh.clone();
                    async move {
                        wh.fetch_add(1, Ordering::SeqCst);
                        Json(json!({"created": true}))
                    }
                }),
            ),
    )
    .await;

    let core = runtime();
    let sp = spec::parse(
        json!({
            "spec_version": 2,
            "site": { "name": "fixture", "base_url": base },
            "operations": [
                {
                    "name": "create-item",
                    "method": "POST",
                    "path": "/items",
                    "effect": "write",
                    "evidence": "Synthetic write with preflight",
                    "preflight": [
                        {
                            "name": "boot",
                            "path": "/boot",
                            "extract": { "csrf": "css:meta[name='csrf']@content" }
                        }
                    ],
                    "headers": { "x-csrf": "{{step.boot.csrf}}" },
                    "body": { "json": { "title": { "type": "string", "required": true } } }
                }
            ]
        })
        .to_string()
        .as_bytes(),
    )
    .unwrap();
    core.install(&sp).unwrap();
    allow_changes(&core, "fixture");

    let req = core
        .request(
            &sp,
            "create-item",
            BTreeMap::from([("title".into(), "Task 1".into())]),
            None,
        )
        .unwrap();
    let review = core.approval(req.clone(), "en").unwrap();

    // 1. Preflight fails before the commit barrier: approval must NOT be consumed and server receives 0 writes.
    assert!(core.execute(req.clone(), Some(&review.id)).await.is_err());
    assert_eq!(writes_hit.load(Ordering::SeqCst), 0);
    assert!(core.policy.review(&review.id).is_ok());

    // 2. Once preflight succeeds, execution crosses the commit barrier and consumes the approval.
    preflight_ok.store(1, Ordering::SeqCst);
    assert_eq!(
        core.execute(req.clone(), Some(&review.id))
            .await
            .unwrap()
            .0
            .status,
        200
    );
    assert_eq!(writes_hit.load(Ordering::SeqCst), 1);
    assert!(core.policy.review(&review.id).is_err());

    // 3. Pre-dispatch write job cancelled before commit barrier is cleanly "cancelled", whereas a dispatched direct write is "action_outcome_unknown".
    let mut pre_dispatch_write = crate::state::Job::new("action", "en");
    pre_dispatch_write.effect = Effect::Write;
    pre_dispatch_write.dispatch_phase = crate::state::DispatchPhase::PreDispatch;
    let pre_job = core
        .spawn_job(pre_dispatch_write, std::future::pending())
        .unwrap();
    core.cancel_job(&pre_job.id).unwrap();
    let cancelled_pre = core.state.job(&pre_job.id).unwrap();
    assert_eq!(cancelled_pre.status, "cancelled");
    assert_eq!(cancelled_pre.error_code, "cancelled");

    let mut dispatched_direct_write = crate::state::Job::new("action", "en");
    dispatched_direct_write.effect = Effect::Write;
    dispatched_direct_write.dispatch_phase = crate::state::DispatchPhase::Dispatched;
    let disp_job = core
        .spawn_job(dispatched_direct_write, std::future::pending())
        .unwrap();
    core.cancel_job(&disp_job.id).unwrap();
    let cancelled_disp = core.state.job(&disp_job.id).unwrap();
    assert_eq!(cancelled_disp.status, "failed");
    assert_eq!(cancelled_disp.error_code, "action_outcome_unknown");

    // 4. Multi-process PID-aware recovery: a job owned by a dead PID is recovered, while a job owned by a live external PID (e.g. parent PID 1 or live process) is untouched on open_with_recovery(_, false).
    let mut dead_pid_job = crate::state::Job::new("action", "en");
    dead_pid_job.status = "running".into();
    dead_pid_job.effect = Effect::Write;
    dead_pid_job.dispatch_phase = crate::state::DispatchPhase::Dispatched;
    dead_pid_job.owner_pid = i32::MAX as u32;
    core.state.add_job(dead_pid_job.clone()).unwrap();

    let reopened =
        crate::state::StateStore::open_with_recovery(&core.root.join("dashboard.json"), false)
            .unwrap();
    let recovered = reopened.job(&dead_pid_job.id).unwrap();
    assert_eq!(recovered.status, "failed");
    assert_eq!(recovered.error_code, "action_outcome_unknown");
    server.abort();
}

#[test]
fn resume_auth_exact_scope_and_atomic_credential_update_preserve_isolation() {
    let core = runtime();

    // 1. Exact origin & account matching on strategy_gate cleanup:
    // Resuming https://example.com|acc-1 must NOT clear https://example.com.evil.org|acc-1 or https://example.com|acc-2.
    let k_target =
        policy::Policy::strategy_key("GET", "https://example.com/api/me", "http", "acc-1");
    let k_other_acc =
        policy::Policy::strategy_key("GET", "https://example.com/api/me", "http", "acc-2");
    let k_sibling_domain = policy::Policy::strategy_key(
        "GET",
        "https://example.com.evil.org/api/me",
        "http",
        "acc-1",
    );

    core.policy
        .record_blocked_strategy(&k_target, "auth_required", 60_000)
        .unwrap();
    core.policy
        .record_blocked_strategy(&k_other_acc, "auth_required", 60_000)
        .unwrap();
    core.policy
        .record_blocked_strategy(&k_sibling_domain, "challenge_required", 60_000)
        .unwrap();

    core.policy
        .resume_auth("https://example.com|acc-1")
        .unwrap();
    assert_eq!(core.policy.is_strategy_blocked(&k_target).unwrap(), None);
    assert_eq!(
        core.policy.is_strategy_blocked(&k_other_acc).unwrap(),
        Some("auth_required".into())
    );
    assert_eq!(
        core.policy.is_strategy_blocked(&k_sibling_domain).unwrap(),
        Some("challenge_required".into())
    );

    // Origin-wide resume clears all accounts on https://example.com, still leaving https://example.com.evil.org intact.
    core.policy.resume("https://example.com").unwrap();
    assert_eq!(core.policy.is_strategy_blocked(&k_other_acc).unwrap(), None);
    assert_eq!(
        core.policy.is_strategy_blocked(&k_sibling_domain).unwrap(),
        Some("challenge_required".into())
    );

    // 2. Atomic credential read-modify-write via Runtime::update_credential:
    let account = core
        .register_browser_credential(
            "fixture",
            crate::accounts::Credential {
                origin: "https://example.com".into(),
                cookies: serde_json::from_value(json!([
                    {"name":"sid","value":"v1","domain":"example.com","path":"/"}
                ]))
                .unwrap(),
                local_storage: BTreeMap::new(),
                headers: BTreeMap::new(),
                recipe: None,
                identity_source: String::new(),
                browser_session: String::new(),
                extension_origin: String::new(),
                browser_session_expires_ms: 0,
                source_profile: String::new(),
            },
            "Chrome",
            "Default",
        )
        .unwrap();
    let (updated, prev_len) = core
        .update_credential(&account.id, |cred| {
            let prev = cred.headers.len();
            cred.headers.insert("x-custom".into(), "token-123".into());
            cred.identity_source = "https://example.com/me".into();
            Ok(prev)
        })
        .unwrap();
    assert_eq!(prev_len, 0);
    assert_eq!(
        updated.headers.get("x-custom").map(String::as_str),
        Some("token-123")
    );
    let reloaded = core.credential(&account.id).unwrap();
    assert_eq!(reloaded.identity_source, "https://example.com/me");
    assert_eq!(reloaded.cookies.len(), 1);
}
