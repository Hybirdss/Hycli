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
        oauth_refresh: None,
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
                identity_proof: String::new(),
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

#[tokio::test]
async fn auth_declaration_diagnostics_preserve_intentional_public_writes() {
    let seen = Arc::new(AtomicUsize::new(0));
    let counted = seen.clone();
    let (url, server) = host(Router::new().route(
        "/items",
        post(move |headers: HeaderMap| {
            let counted = counted.clone();
            async move {
                if !headers.contains_key("cookie") && !headers.contains_key("authorization") {
                    counted.fetch_add(1, Ordering::SeqCst);
                }
                Json(json!({"ok":true}))
            }
        }),
    ))
    .await;
    let core = runtime();
    let sp = spec_for(&url);
    core.install(&sp).unwrap();
    let inputs = BTreeMap::from([("title".into(), "Synthetic item".into())]);
    assert_eq!(
        core.run_op(&sp, "create-item", &inputs)
            .await
            .err()
            .unwrap()
            .public_code,
        "writes_disabled"
    );
    assert_eq!(seen.load(Ordering::SeqCst), 0);
    allow_changes(&core, "fixture");
    let outcome = core.run_op(&sp, "create-item", &inputs).await;
    server.abort();
    assert!(
        outcome.is_ok(),
        "intentional public write must remain supported"
    );
    assert_eq!(seen.load(Ordering::SeqCst), 1);
}

#[test]
fn explicit_binding_is_local_exact_origin_and_invalidated_by_source_context() {
    let core = runtime();
    let sp = spec_for("https://example.test");
    core.install(&sp).unwrap();
    let account = core.connect_account("", "https://example.test", serde_json::from_value(json!([{"name":"sid","value":"synthetic-only","domain":"example.test","path":"/","secure":true}])).unwrap(), "Fixture", "Profile A", "").unwrap();
    core.state
        .update(|d| {
            let a = d.accounts.get_mut(&account.id).unwrap();
            a.verified_identity = true;
            a.user_id = "synthetic-person".into();
            // Legacy persisted references must not become cross-ID authorization.
            d.sites.get_mut("fixture").unwrap().account_id = account.id.clone();
            Ok(())
        })
        .unwrap();
    assert!(
        core.request(&sp, "read-items", BTreeMap::new(), Some(&account.id))
            .is_err()
    );
    assert!(
        core.select_account("fixture", &account.id, "https://example.test")
            .is_err(),
        "legacy verified booleans cannot mint binding"
    );
    let expected = core.credential(&account.id).unwrap();
    core.publish_verified_identity(
        &account.id,
        &expected,
        expected.clone(),
        crate::accounts::Identity {
            user_id: "synthetic-person".into(),
            ..Default::default()
        },
    )
    .unwrap();
    core.select_account("fixture", &account.id, "https://example.test")
        .unwrap();
    let request = core
        .request(&sp, "read-items", BTreeMap::new(), None)
        .unwrap();
    assert_eq!(request.account_id, account.id);
    assert_eq!(
        core.state.account(&account.id).unwrap().unwrap().site_id,
        account.site_id
    );
    let exported = serde_yaml::to_string(&core.spec("fixture").unwrap()).unwrap();
    assert!(!exported.contains("account_binding"));
    assert!(!exported.contains("synthetic-person"));
    assert!(!exported.contains("synthetic-only"));
    let mut sibling = sp.clone();
    sibling.site.base_url = "https://sibling.example.test".into();
    assert!(
        core.request(&sibling, "read-items", BTreeMap::new(), Some(&account.id))
            .is_err()
    );
    core.update_credential(&account.id, |c| {
        c.source_profile = "a-different-profile".into();
        Ok(())
    })
    .unwrap();
    assert!(
        core.request(&sp, "read-items", BTreeMap::new(), None)
            .is_err()
    );
    assert!(
        core.browser_account_for("fixture", "https://example.test")
            .is_err()
    );
    core.select_account("fixture", "", "https://example.test")
        .unwrap();
    assert_eq!(
        core.browser_account_for("fixture", "https://example.test")
            .unwrap(),
        ""
    );
}

#[test]
fn stale_selected_preparation_cannot_publish_or_overwrite_new_selection() {
    let core = runtime();
    let original = spec_for("https://example.test");
    core.install(&original).unwrap();
    // Explicit anonymous is also a choice and must be pinned against replacement.
    core.select_account("fixture", "", "https://example.test")
        .unwrap();
    let before = core
        .selection_snapshot("fixture", "https://example.test")
        .unwrap()
        .unwrap();
    core.select_account("fixture", "", "https://example.test")
        .unwrap();
    let newer = core
        .state
        .site("fixture")
        .unwrap()
        .unwrap()
        .account_selection_revision;
    let mut replacement = original.clone();
    replacement.operations.clear();
    assert!(core.install_selected(&replacement, Some(&before)).is_err());
    assert_eq!(
        core.spec("fixture").unwrap().operations.len(),
        original.operations.len()
    );
    assert_eq!(
        core.state
            .site("fixture")
            .unwrap()
            .unwrap()
            .account_selection_revision,
        newer
    );
}

#[tokio::test]
async fn successful_operation_preserves_account_identity_proof_and_selection() {
    use axum::extract::Query;

    let (url, server) = host(Router::new().route(
        "/account",
        get(|Query(query): Query<BTreeMap<String, String>>| async move {
            Json(json!({
                "id": "operation-person",
                "name": "Operation Person",
                "items": ["result"],
                "next": if query.contains_key("pageToken") {
                    Value::Null
                } else {
                    json!("synthetic-next-page")
                },
                "cursor_echo": query.get("pageToken")
            }))
        }),
    ))
    .await;
    let sp = spec::parse(
        json!({
            "spec_version": 3,
            "site": {"name": "fixture", "base_url": url},
            "operations": [{
                "name": "read-account", "method": "GET", "path": "/account",
                "effect": "read", "evidence": "Synthetic operation fixture",
                "model": "result",
                "pagination": {"input": "cursor", "query": "pageToken", "next": "/next"}
            }],
            "models": {"result": {"person": {"path": "name", "type": "string"}}}
        })
        .to_string()
        .as_bytes(),
    )
    .unwrap();
    for verified in [false, true] {
        let core = runtime();
        core.install(&sp).unwrap();
        let credential = serde_json::from_value(json!({
            "origin": url,
            "cookies": [{"name": "sid", "value": "synthetic-operation-session",
                "domain": "127.0.0.1", "path": "/"}]
        }))
        .unwrap();
        let account = core
            .register_browser_credential(
                if verified { "source" } else { "fixture" },
                credential,
                "Fixture",
                "Synthetic",
            )
            .unwrap();
        if verified {
            let expected = core.credential(&account.id).unwrap();
            core.publish_verified_identity(
                &account.id,
                &expected,
                expected.clone(),
                crate::accounts::Identity {
                    user_id: "verified-person".into(),
                    name: "Verified Person".into(),
                    ..Default::default()
                },
            )
            .unwrap();
        }
        core.select_account("fixture", &account.id, &url).unwrap();
        let before = core.state.read().unwrap();
        assert_eq!(before.accounts[&account.id].verified_identity, verified);
        let before_credential = serde_json::to_value(core.credential(&account.id).unwrap()).unwrap();
        let mut inputs = BTreeMap::new();
        for page in 0..2 {
            let result = core.run_op(&sp, "read-account", &inputs).await.unwrap().0;
            assert_eq!(result.status, 200);
            assert_eq!(result.body.as_ref().unwrap()["id"], "operation-person");
            assert_eq!(result.body.as_ref().unwrap()["items"], json!(["result"]));
            assert_eq!(result.mapped, Some(json!({"person": "Operation Person"})));
            assert!(result.paginated);
            assert!(
                !serde_json::to_string(&result)
                    .unwrap()
                    .contains("synthetic-next-page")
            );
            assert_eq!(
                result.clone().output().unwrap(),
                json!({"data": {"person": "Operation Person"}, "next_cursor": result.next_cursor})
            );
            if page == 0 {
                inputs.insert("cursor".into(), result.next_cursor.unwrap());
            } else {
                assert!(result.next_cursor.is_none());
                assert_eq!(result.body.as_ref().unwrap()["cursor_echo"], "[private]");
            }
            let after = core.state.read().unwrap();
            let mut current_account = after.accounts[&account.id].clone();
            assert!(!current_account.checked_at.is_empty());
            current_account.checked_at = before.accounts[&account.id].checked_at.clone();
            assert_eq!(
                serde_json::to_value(current_account).unwrap(),
                serde_json::to_value(&before.accounts[&account.id]).unwrap()
            );
            assert_eq!(after.identity_generations, before.identity_generations);
            assert_eq!(
                serde_json::to_value(core.credential(&account.id).unwrap()).unwrap(),
                before_credential
            );
            assert_eq!(
                after.sites["fixture"].account_id,
                before.sites["fixture"].account_id
            );
            assert!(
                after.sites["fixture"].account_binding == before.sites["fixture"].account_binding
            );
            assert_eq!(
                after.sites["fixture"].account_selection_revision,
                before.sites["fixture"].account_selection_revision
            );
            assert_eq!(
                core.credential(&account.id)
                    .unwrap()
                    .proves_identity(
                        &after.accounts[&account.id],
                        after
                            .identity_generations
                            .get(&account.id)
                            .map(String::as_str)
                            .unwrap_or(""),
                    )
                    .unwrap(),
                verified
            );
        }
    }
    server.abort();
}

#[test]
fn verified_identity_publication_is_bound_to_exact_private_snapshot() {
    let core = runtime();
    let account = core.connect_account("", "https://example.test", serde_json::from_value(json!([{"name":"sid","value":"synthetic-a","domain":"example.test","path":"/","secure":true}])).unwrap(), "Fixture", "A", "").unwrap();
    let before = core.credential(&account.id).unwrap();
    let identity = crate::accounts::Identity {
        user_id: "person-a".into(),
        ..Default::default()
    };
    let verified = core
        .publish_verified_identity(&account.id, &before, before.clone(), identity.clone())
        .unwrap();
    let proven = core.credential(&account.id).unwrap();
    assert!(
        proven
            .proves_identity(
                &verified,
                &core.state.read().unwrap().identity_generations[&account.id]
            )
            .unwrap()
    );
    assert!(
        !serde_json::to_string(&verified)
            .unwrap()
            .contains("identity_proof")
    );
    core.update_credential(&account.id, |c| {
        c.cookies[0].value = "synthetic-b".into();
        Ok(())
    })
    .unwrap();
    assert!(
        core.publish_verified_identity(&account.id, &proven, proven.clone(), identity)
            .is_err()
    );
    assert!(
        core.invalidate_identity_if_current(&account.id, &proven, true)
            .is_err()
    );
    assert_eq!(
        core.credential(&account.id).unwrap().cookies[0].value,
        "synthetic-b"
    );
    assert!(
        !core
            .credential(&account.id)
            .unwrap()
            .proves_identity(
                &verified,
                &core.state.read().unwrap().identity_generations[&account.id]
            )
            .unwrap()
    );
}

#[test]
fn private_identity_proof_rejects_partial_publication_and_rolls_back_state_failure() {
    let core = runtime();
    let account = core.connect_account("", "https://example.test", serde_json::from_value(json!([{"name":"sid","value":"synthetic-a","domain":"example.test","path":"/","secure":true}])).unwrap(), "Fixture", "A", "").unwrap();
    let before = core.credential(&account.id).unwrap();
    let mut future_view = account.clone();
    crate::accounts::apply_identity(
        &mut future_view,
        crate::accounts::Identity {
            user_id: "future-person".into(),
            ..Default::default()
        },
    );
    let mut partial = before.clone();
    partial.identity_proof = partial
        .identity_proof_for(&future_view, "future-generation")
        .unwrap();
    assert!(
        !partial.proves_identity(&account, "old-generation").unwrap(),
        "vault-first crash cannot verify the old state"
    );
    assert!(
        partial
            .proves_identity(&future_view, "future-generation")
            .unwrap()
    );
    assert!(
        !partial
            .proves_identity(&future_view, "old-generation")
            .unwrap(),
        "generation alone rejects the same-identity crash window"
    );
    let mut private_data = core.state.read().unwrap();
    private_data
        .identity_generations
        .insert(account.id.clone(), "private-generation".into());
    let encoded = serde_json::to_string(&private_data).unwrap();
    assert!(!encoded.contains("private-generation"));
    let decoded: crate::state::Data = serde_json::from_str(&encoded).unwrap();
    assert!(decoded.identity_generations.is_empty());
    let sql = rusqlite::Connection::open(core.state.db_path()).unwrap();
    sql.execute_batch("CREATE TRIGGER reject_identity BEFORE UPDATE ON accounts BEGIN SELECT RAISE(FAIL, 'synthetic failure'); END;").unwrap();
    assert!(
        core.publish_verified_identity(
            &account.id,
            &before,
            before.clone(),
            crate::accounts::Identity {
                user_id: "future-person".into(),
                ..Default::default()
            }
        )
        .is_err()
    );
    assert_eq!(
        serde_json::to_vec(&core.credential(&account.id).unwrap()).unwrap(),
        serde_json::to_vec(&before).unwrap()
    );
    assert!(
        !core
            .state
            .account(&account.id)
            .unwrap()
            .unwrap()
            .verified_identity
    );
    sql.execute_batch("DROP TRIGGER reject_identity;").unwrap();
    let mut forged = before;
    forged.identity_proof = "supplied-claim".into();
    let imported = core
        .register_browser_credential("source", forged, "Fixture", "A")
        .unwrap();
    assert!(
        core.credential(&imported.id)
            .unwrap()
            .identity_proof
            .is_empty()
    );
}

#[test]
fn later_explicit_choice_blocks_preparation_started_without_selection() {
    let core = runtime();
    let original = spec_for("https://example.test");
    core.install(&original).unwrap();
    let before_choice = core
        .preparation_selection_snapshot("fixture", "https://example.test")
        .unwrap();
    core.select_account("fixture", "", "https://example.test")
        .unwrap();
    let mut stale = original.clone();
    stale.site.title = "Stale preparation".into();
    assert!(core.install_selected(&stale, Some(&before_choice)).is_err());
    assert_eq!(
        Runtime::spec_hash(&core.spec("fixture").unwrap()).unwrap(),
        Runtime::spec_hash(&original).unwrap()
    );
}

#[tokio::test]
async fn refresh_identity_proof_requires_private_snapshot_and_anonymous_baseline() {
    let (url, server) = host(
        Router::new()
            .route(
                "/account",
                get(|headers: HeaderMap| async move {
                    if headers
                        .get("cookie")
                        .is_some_and(|v| v == "sid=synthetic-session")
                    {
                        (StatusCode::OK, Json(json!({"username":"private-person"})))
                    } else {
                        (
                            StatusCode::UNAUTHORIZED,
                            Json(json!({"error":"signed_out"})),
                        )
                    }
                }),
            )
            .route(
                "/users/me",
                get(|| async { Json(json!({"username":"public-person"})) }),
            ),
    )
    .await;
    let core = runtime();
    let credential: crate::accounts::Credential = serde_json::from_value(json!({"origin":url,"cookies":[{"name":"sid","value":"synthetic-session","domain":"127.0.0.1","path":"/"}],"identity_source":format!("{url}/account")})).unwrap();
    let account = core
        .register_browser_credential("fixture", credential, "Fixture", "Synthetic")
        .unwrap();
    let view = core.refresh_account(&account.id).await.unwrap();
    let state = core.state.read().unwrap();
    assert!(
        core.credential(&account.id)
            .unwrap()
            .proves_identity(
                &view,
                state
                    .identity_generations
                    .get(&account.id)
                    .map(String::as_str)
                    .unwrap_or("")
            )
            .unwrap()
    );
    core.update_credential(&account.id, |c| {
        c.identity_source = format!("{url}/users/me");
        Ok(())
    })
    .unwrap();
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
}

#[tokio::test]
async fn stale_dashboard_refresh_cannot_relabel_reconnected_identity() {
    let entered = Arc::new(tokio::sync::Notify::new());
    let release = Arc::new(tokio::sync::Notify::new());
    let entered_handler = entered.clone();
    let release_handler = release.clone();
    let (url, server) = host(Router::new().route(
        "/account",
        get(move |headers: HeaderMap| {
            let entered = entered_handler.clone();
            let release = release_handler.clone();
            async move {
                if headers
                    .get("cookie")
                    .is_some_and(|v| v == "sid=synthetic-a")
                {
                    entered.notify_one();
                    release.notified().await;
                    (StatusCode::OK, Json(json!({"username":"person-a"})))
                } else {
                    (
                        StatusCode::UNAUTHORIZED,
                        Json(json!({"error":"signed_out"})),
                    )
                }
            }
        }),
    ))
    .await;
    let core = runtime();
    let credential: crate::accounts::Credential = serde_json::from_value(json!({"origin":url,"cookies":[{"name":"sid","value":"synthetic-a","domain":"127.0.0.1","path":"/"}],"identity_source":format!("{url}/account")})).unwrap();
    let account = core
        .register_browser_credential("fixture", credential, "Fixture", "Synthetic")
        .unwrap();
    let pending_core = core.clone();
    let id = account.id.clone();
    let pending = tokio::spawn(async move { pending_core.refresh_account(&id).await });
    tokio::time::timeout(std::time::Duration::from_secs(5), entered.notified())
        .await
        .unwrap();
    core.update_credential(&account.id, |c| {
        c.cookies[0].value = "synthetic-b".into();
        Ok(())
    })
    .unwrap();
    let replacement = core.credential(&account.id).unwrap();
    let current = core
        .publish_verified_identity(
            &account.id,
            &replacement,
            replacement.clone(),
            crate::accounts::Identity {
                user_id: "person-b".into(),
                ..Default::default()
            },
        )
        .unwrap();
    release.notify_one();
    assert_eq!(
        pending.await.unwrap().err().unwrap().public_code,
        "conflict"
    );
    let state = core.state.read().unwrap();
    assert_eq!(state.accounts[&account.id].user_id, "person-b");
    assert!(
        core.credential(&account.id)
            .unwrap()
            .proves_identity(&current, &state.identity_generations[&account.id])
            .unwrap()
    );
    server.abort();
}

#[tokio::test]
async fn bound_account_dispatch_uses_selected_snapshot_and_rejects_stale_authority() {
    let calls = Arc::new(AtomicUsize::new(0));
    let counted = calls.clone();
    let (url, server) = host(Router::new().route(
        "/items",
        get(move |headers: HeaderMap| {
            let calls = counted.clone();
            async move {
                calls.fetch_add(1, Ordering::SeqCst);
                assert_eq!(headers.get("cookie").unwrap(), "sid=synthetic-bound");
                Json(json!({"items":[]}))
            }
        }),
    ))
    .await;
    let core = runtime();
    let spec = spec_for(&url);
    core.install(&spec).unwrap();
    let account = core
        .connect_account(
            "",
            &url,
            serde_json::from_value(
                json!([{"name":"sid","value":"synthetic-bound","domain":"127.0.0.1","path":"/"}]),
            )
            .unwrap(),
            "Fixture",
            "Synthetic",
            "",
        )
        .unwrap();
    let expected = core.credential(&account.id).unwrap();
    core.publish_verified_identity(
        &account.id,
        &expected,
        expected.clone(),
        crate::accounts::Identity {
            user_id: "synthetic-person".into(),
            ..Default::default()
        },
    )
    .unwrap();
    core.select_account("fixture", &account.id, &url).unwrap();
    let stale = core
        .request(&spec, "read-items", BTreeMap::new(), None)
        .unwrap();
    core.select_account("fixture", "", &url).unwrap();
    assert!(core.execute(stale, None).await.is_err());
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    core.select_account("fixture", &account.id, &url).unwrap();
    let response = core
        .run_op(&spec, "read-items", &BTreeMap::new())
        .await
        .unwrap();
    assert_eq!(response.0.status, 200);
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    core.update_credential(&account.id, |c| {
        c.cookies[0].value = "synthetic-replacement".into();
        Ok(())
    })
    .unwrap();
    assert!(
        core.run_op(&spec, "read-items", &BTreeMap::new())
            .await
            .is_err()
    );
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    server.abort();
}

#[tokio::test]
async fn recon_respects_explicit_anonymous_and_selected_account() {
    let seen = Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
    let entry_seen = seen.clone();
    let bundle_seen = seen.clone();
    let (url, server) = host(
        Router::new()
            .route(
                "/",
                get(move |headers: HeaderMap| {
                    let seen = entry_seen.clone();
                    async move {
                        seen.lock().unwrap().push(
                            headers
                                .get("cookie")
                                .and_then(|v| v.to_str().ok())
                                .unwrap_or("")
                                .into(),
                        );
                        axum::response::Html("<html><script src='/bundle.js'></script></html>")
                    }
                }),
            )
            .route(
                "/bundle.js",
                get(move |headers: HeaderMap| {
                    let seen = bundle_seen.clone();
                    async move {
                        seen.lock().unwrap().push(
                            headers
                                .get("cookie")
                                .and_then(|v| v.to_str().ok())
                                .unwrap_or("")
                                .into(),
                        );
                        "fetch('/items')"
                    }
                }),
            ),
    )
    .await;
    let core = runtime();
    core.install(&spec_for(&url)).unwrap();
    for value in ["synthetic-a", "synthetic-b"] {
        let account = core
            .connect_account(
                "",
                &url,
                serde_json::from_value(
                    json!([{"name":"sid","value":value,"domain":"127.0.0.1","path":"/"}]),
                )
                .unwrap(),
                "Fixture",
                value,
                "",
            )
            .unwrap();
        if value == "synthetic-b" {
            let expected = core.credential(&account.id).unwrap();
            core.publish_verified_identity(
                &account.id,
                &expected,
                expected.clone(),
                crate::accounts::Identity {
                    user_id: "person-b".into(),
                    ..Default::default()
                },
            )
            .unwrap();
            core.select_account("fixture", &account.id, &url).unwrap();
        }
    }
    let selected = core.state.site("fixture").unwrap().unwrap().account_id;
    core.select_account("fixture", "", &url).unwrap();
    core.recon("fixture", &url, 1).await.unwrap();
    assert!(!seen.lock().unwrap().is_empty());
    assert!(
        seen.lock().unwrap().iter().all(String::is_empty),
        "explicit anonymous recon must not choose another account"
    );
    seen.lock().unwrap().clear();
    core.select_account("fixture", &selected, &url).unwrap();
    core.recon("fixture", &url, 1).await.unwrap();
    assert!(seen.lock().unwrap().iter().all(|v| v == "sid=synthetic-b"));
    server.abort();
}

#[tokio::test]
async fn bound_readonly_preparation_tools_preserve_source_ownership() {
    let (url, server) = host(Router::new().route(
        "/",
        get(|headers: HeaderMap| async move {
            assert_eq!(headers.get("cookie").unwrap(), "sid=synthetic-bound");
            Json(json!({"ok":true}))
        }),
    ))
    .await;
    let core = runtime();
    core.install(&spec_for(&url)).unwrap();
    let account = core
        .connect_account(
            "",
            &url,
            serde_json::from_value(
                json!([{"name":"sid","value":"synthetic-bound","domain":"127.0.0.1","path":"/"}]),
            )
            .unwrap(),
            "Fixture",
            "Synthetic",
            "",
        )
        .unwrap();
    let expected = core.credential(&account.id).unwrap();
    core.publish_verified_identity(
        &account.id,
        &expected,
        expected.clone(),
        crate::accounts::Identity {
            user_id: "person".into(),
            ..Default::default()
        },
    )
    .unwrap();
    core.select_account("fixture", &account.id, &url).unwrap();
    let before = serde_json::to_vec(&core.credential(&account.id).unwrap()).unwrap();
    let mut job = crate::state::Job::new("prepare", "en");
    job.site_id = "fixture".into();
    job.url = format!("{url}/");
    job.account_selection = Some(
        core.preparation_selection_snapshot("fixture", &url)
            .unwrap(),
    );
    let environment = crate::environment::Environment {
        discovery_pending: false,
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
    for tool in ["session.inspect", "session.observe"] {
        core.preparation_tool(
            &crate::agent_tools::ToolCall {
                tool: tool.into(),
                arguments: json!({"account_id":account.id,"url":job.url}),
            },
            &context,
        )
        .await
        .unwrap();
    }
    assert_eq!(
        serde_json::to_vec(&core.credential(&account.id).unwrap()).unwrap(),
        before
    );
    assert_eq!(
        core.state.account(&account.id).unwrap().unwrap().site_id,
        account.site_id
    );
    assert!(
        core.preparation_tool(
            &crate::agent_tools::ToolCall {
                tool: "session.verify".into(),
                arguments: json!({"account_id":account.id})
            },
            &context
        )
        .await
        .is_err()
    );
    server.abort();
}

#[tokio::test]
async fn browser_failure_rechecks_binding_before_http_retry() {
    let calls = Arc::new(AtomicUsize::new(0));
    let counted = calls.clone();
    let (url, server) = host(Router::new().route(
        "/items",
        get(move || {
            let calls = counted.clone();
            async move {
                calls.fetch_add(1, Ordering::SeqCst);
                "ok"
            }
        }),
    ))
    .await;
    let core = runtime();
    core.install(&spec_for(&url)).unwrap();
    let account = core
        .connect_account(
            "",
            &url,
            serde_json::from_value(
                json!([{"name":"sid","value":"synthetic-bound","domain":"127.0.0.1","path":"/"}]),
            )
            .unwrap(),
            "Fixture",
            "Synthetic",
            "",
        )
        .unwrap();
    let expected = core.credential(&account.id).unwrap();
    core.publish_verified_identity(
        &account.id,
        &expected,
        expected.clone(),
        crate::accounts::Identity {
            user_id: "person".into(),
            ..Default::default()
        },
    )
    .unwrap();
    core.select_account("fixture", &account.id, &url).unwrap();
    let snapshot = core.credential(&account.id).unwrap();
    let endpoint = format!("{url}/items");
    let request = crate::net::Request {
        url: endpoint.clone(),
        method: "GET".into(),
        headers: snapshot.request_headers(&endpoint).unwrap(),
        body: None,
        account_id: account.id.clone(),
        limit: 4096,
        timeout: std::time::Duration::from_secs(5),
        read_only: true,
        follow_redirects: false,
        tls_profile: "standard".into(),
        multipart: None,
    };
    let response = core
        .retry_read_after_browser(
            async {
                core.select_account("fixture", "", &url).unwrap();
                Err(crate::apperr::AppError::api("browser_unavailable", 422))
            },
            request.clone(),
            true,
            || core.ensure_credential_snapshot("fixture", &endpoint, &account.id, Some(&snapshot)),
        )
        .await;
    assert_eq!(response.err().unwrap().public_code, "auth_required");
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    core.select_account("fixture", &account.id, &url).unwrap();
    assert!(
        core.retry_read_after_browser(
            async { Err(crate::apperr::AppError::api("browser_unavailable", 422)) },
            request,
            true,
            || core.ensure_credential_snapshot("fixture", &endpoint, &account.id, Some(&snapshot))
        )
        .await
        .unwrap()
        .successful()
    );
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    server.abort();
}

#[tokio::test]
async fn normal_refresh_rejects_secret_echo_before_identity_publication() {
    let (url, server) = host(Router::new().route(
        "/account",
        get(|headers: HeaderMap| async move {
            if headers.get("cookie").is_some() {
                (StatusCode::OK, Json(json!({"username":"abc"})))
            } else {
                (
                    StatusCode::UNAUTHORIZED,
                    Json(json!({"error":"signed_out"})),
                )
            }
        }),
    ))
    .await;
    let core = runtime();
    let account = core
        .connect_account(
            "",
            &url,
            serde_json::from_value(
                json!([{"name":"sid","value":"abc","domain":"127.0.0.1","path":"/"}]),
            )
            .unwrap(),
            "Fixture",
            "Synthetic",
            &format!("{url}/account"),
        )
        .unwrap();
    assert_eq!(
        core.refresh_account(&account.id)
            .await
            .err()
            .unwrap()
            .public_code,
        "identity_unavailable"
    );
    let state = core.state.read().unwrap();
    assert!(!state.accounts[&account.id].verified_identity);
    assert!(state.accounts[&account.id].user_id.is_empty());
    assert!(!state.identity_generations.contains_key(&account.id));
    server.abort();
}

#[tokio::test]
async fn preparation_recon_stops_before_bundles_after_selection_changes() {
    let entered = Arc::new(tokio::sync::Notify::new());
    let release = Arc::new(tokio::sync::Notify::new());
    let start = entered.clone();
    let resume = release.clone();
    let bundles = Arc::new(AtomicUsize::new(0));
    let counted = bundles.clone();
    let (url, server) = host(
        Router::new()
            .route(
                "/",
                get(move |headers: HeaderMap| {
                    let entered = start.clone();
                    let release = resume.clone();
                    async move {
                        assert_eq!(headers.get("cookie").unwrap(), "sid=synthetic-bound");
                        entered.notify_one();
                        release.notified().await;
                        axum::response::Html("<html><script src='/bundle.js'></script></html>")
                    }
                }),
            )
            .route(
                "/bundle.js",
                get(move || {
                    let calls = counted.clone();
                    async move {
                        calls.fetch_add(1, Ordering::SeqCst);
                        "fetch('/items')"
                    }
                }),
            ),
    )
    .await;
    let core = runtime();
    core.install(&spec_for(&url)).unwrap();
    let account = core
        .connect_account(
            "",
            &url,
            serde_json::from_value(
                json!([{"name":"sid","value":"synthetic-bound","domain":"127.0.0.1","path":"/"}]),
            )
            .unwrap(),
            "Fixture",
            "Synthetic",
            "",
        )
        .unwrap();
    let expected = core.credential(&account.id).unwrap();
    core.publish_verified_identity(
        &account.id,
        &expected,
        expected.clone(),
        crate::accounts::Identity {
            user_id: "person".into(),
            ..Default::default()
        },
    )
    .unwrap();
    core.select_account("fixture", &account.id, &url).unwrap();
    let mut job = crate::state::Job::new("prepare", "en");
    job.site_id = "fixture".into();
    job.url = url.clone();
    job.account_selection = Some(
        core.preparation_selection_snapshot("fixture", &url)
            .unwrap(),
    );
    let pending_core = core.clone();
    let pending_url = url.clone();
    let pending =
        tokio::spawn(async move { pending_core.recon_for_job(&job, &pending_url, 1).await });
    tokio::time::timeout(std::time::Duration::from_secs(5), entered.notified())
        .await
        .unwrap();
    core.select_account("fixture", "", &url).unwrap();
    release.notify_one();
    assert_eq!(
        pending.await.unwrap().err().unwrap().public_code,
        "auth_required"
    );
    assert_eq!(bundles.load(Ordering::SeqCst), 0);
    server.abort();
}

#[test]
fn mcp_listing_survives_metadata_removal_after_snapshot() {
    for remove_site in [false, true] {
        let core = runtime();
        let definition = spec_for("https://fixture.test");
        core.install(&definition).unwrap();
        let mut unrelated = definition.clone();
        unrelated.site.name = "unrelated".into();
        core.install(&unrelated).unwrap();
        let snapshot = core.sites("en").unwrap();
        if remove_site {
            core.remove_site("fixture").unwrap();
        } else {
            let mut replacement = definition.clone();
            replacement.operations.retain(|op| op.name != "read-items");
            core.install(&replacement).unwrap();
        }
        let mcp = crate::guarded_mcp::Server {
            core,
            sites_only: false,
            only_site: None,
        };
        let names: Vec<_> = mcp
            .tools_from_snapshot(snapshot)
            .unwrap()
            .into_iter()
            .map(|tool| tool.name.to_string())
            .collect();
        assert!(names.contains(&"fixture_read-items".into()));
        assert!(names.contains(&"unrelated_read-items".into()));
        assert!(names.contains(&"hycli_result".into()));
    }
}

#[tokio::test]
async fn identity_refresh_permission_denial_preserves_session_but_401_expires_it() {
    use std::sync::atomic::AtomicU16;
    let status = Arc::new(AtomicU16::new(200));
    let response_status = status.clone();
    let (url, server) = host(Router::new().route(
        "/account",
        get(move |headers: HeaderMap| {
            let status = response_status.clone();
            async move {
                if headers
                    .get("cookie")
                    .is_some_and(|v| v == "sid=synthetic-session")
                {
                    (
                        StatusCode::from_u16(status.load(Ordering::SeqCst)).unwrap(),
                        Json(json!({"id":"private-person", "name":"Private Person"})),
                    )
                } else {
                    (
                        StatusCode::UNAUTHORIZED,
                        Json(json!({"error":"signed_out"})),
                    )
                }
            }
        }),
    ))
    .await;
    let core = runtime();
    for recipe_refresh in [false, true] {
        status.store(200, Ordering::SeqCst);
        let credential: crate::accounts::Credential = serde_json::from_value(json!({"origin":url,"cookies":[{"name":"sid","value":"synthetic-session","domain":"127.0.0.1","path":"/"}],"identity_source":format!("{url}/account")})).unwrap();
        let account = core
            .register_browser_credential("fixture", credential, "Fixture", "Synthetic")
            .unwrap();
        if recipe_refresh {
            let recipe = crate::session_recipe::ConnectionRecipe {
                identity_url: format!("{url}/account"),
                identity: crate::session_recipe::IdentityMapping {
                    user_id: "/id".into(),
                    ..Default::default()
                },
                evidence: "observed_browser_request".into(),
                ..Default::default()
            };
            core.observe_account_session(&account.id, &recipe.identity_url, false)
                .await
                .unwrap();
            core.verify_session_recipe(&account.id, &recipe)
                .await
                .unwrap();
        } else {
            core.refresh_account(&account.id).await.unwrap();
        }
        let before_credential =
            serde_json::to_value(core.credential(&account.id).unwrap()).unwrap();
        let before_account =
            serde_json::to_value(core.state.account(&account.id).unwrap()).unwrap();
        let generation = core.state.read().unwrap().identity_generations[&account.id].clone();
        status.store(403, Ordering::SeqCst);
        let code = if recipe_refresh {
            core.refresh_account_session(&account.id)
                .await
                .err()
                .map(|e| e.public_code)
        } else {
            core.refresh_account(&account.id)
                .await
                .err()
                .map(|e| e.public_code)
        };
        assert_eq!(
            code.as_deref(),
            Some("request_failed"),
            "403 is not proof of logout"
        );
        assert_eq!(
            serde_json::to_value(core.credential(&account.id).unwrap()).unwrap(),
            before_credential
        );
        assert_eq!(
            serde_json::to_value(core.state.account(&account.id).unwrap()).unwrap(),
            before_account
        );
        assert_eq!(
            core.state.read().unwrap().identity_generations[&account.id],
            generation
        );
        // The same account can be verified again without importing or signing in again.
        status.store(200, Ordering::SeqCst);
        if recipe_refresh {
            core.refresh_account_session(&account.id).await.unwrap();
        } else {
            core.refresh_account(&account.id).await.unwrap();
        }
        status.store(401, Ordering::SeqCst);
        if recipe_refresh {
            assert_eq!(
                core.refresh_account_session(&account.id)
                    .await
                    .err()
                    .unwrap()
                    .public_code,
                "auth_required"
            );
        } else {
            assert_eq!(
                core.refresh_account(&account.id).await.unwrap().status,
                "expired"
            );
        }
        let expired = core.state.account(&account.id).unwrap().unwrap();
        assert_eq!(expired.status, "expired");
        assert!(!expired.verified_identity);
        assert!(
            core.credential(&account.id)
                .unwrap()
                .identity_proof
                .is_empty()
        );
    }
    server.abort();
}

#[tokio::test]
async fn multipart_upload_sends_the_file_bytes_and_text_field() {
    let received: Arc<std::sync::Mutex<Option<Vec<u8>>>> = Arc::new(std::sync::Mutex::new(None));
    let sink = received.clone();
    let (url, server) = host(Router::new().route(
        "/upload",
        post(move |body: axum::body::Bytes| {
            let sink = sink.clone();
            async move {
                *sink.lock().unwrap() = Some(body.to_vec());
                Json(json!({"ok": true}))
            }
        }),
    ))
    .await;

    let core = runtime();
    let sp = spec::parse(
        json!({"spec_version":1,"site":{"name":"fixture","title":"Fixture","base_url":url},"operations":[
            {"name":"upload","method":"POST","path":"/upload","effect":"write","evidence":"Synthetic multipart upload endpoint",
             "body":{"multipart":{"avatar":{"type":"file","required":true},"caption":{"type":"string","required":true}}},
             "response":{"format":"json","required_pointers":["/ok"]}}
        ]})
        .to_string()
        .as_bytes(),
    )
    .unwrap();
    core.install(&sp).unwrap();
    allow_changes(&core, "fixture");

    let dir = root();
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("avatar.gif");
    std::fs::write(&file, b"GIF89a-synthetic-bytes").unwrap();

    let mut args = BTreeMap::new();
    args.insert("avatar".to_string(), file.to_string_lossy().into_owned());
    args.insert("caption".to_string(), "hello there".to_string());
    core.run_op(&sp, "upload", &args).await.unwrap();

    let body = received
        .lock()
        .unwrap()
        .clone()
        .expect("server received the upload");
    let contains = |needle: &[u8]| body.windows(needle.len()).any(|w| w == needle);
    assert!(
        contains(b"GIF89a-synthetic-bytes"),
        "file bytes not in multipart body"
    );
    assert!(
        contains(b"hello there"),
        "caption text not in multipart body"
    );
    assert!(contains(b"name=\"avatar\""), "file field name missing");
    assert!(contains(b"filename="), "file part had no filename");
    assert!(contains(b"name=\"caption\""), "text field name missing");
    server.abort();
}

#[tokio::test]
async fn derived_target_allows_cross_origin_upload_but_a_plain_op_is_blocked() {
    let got: Arc<std::sync::Mutex<Option<Vec<u8>>>> = Arc::new(std::sync::Mutex::new(None));
    let sink = got.clone();
    let (upload_url, upload_server) = host(Router::new().route(
        "/put",
        post(move |body: axum::body::Bytes| {
            let sink = sink.clone();
            async move {
                *sink.lock().unwrap() = Some(body.to_vec());
                Json(json!({"ok": true}))
            }
        }),
    ))
    .await;
    let lease_base = upload_url.clone();
    let (site_url, lease_server) = host(Router::new().route(
        "/lease",
        get(move || {
            let lease_base = lease_base.clone();
            async move { Json(json!({"base": lease_base})) }
        }),
    ))
    .await;

    let op_json = |derived: bool| {
        json!({"spec_version":1,"site":{"name":"fixture","title":"Fixture","base_url":site_url},"operations":[
            {"name":"upload","method":"POST","path":"/put","base_url":"{{step.lease.base}}","derived_target":derived,
             "effect":"write","evidence":"Synthetic derived cross-origin upload",
             "preflight":[{"name":"lease","path":"/lease","extract":{"base":"json:/base"}}],
             "body":{"multipart":{"file":{"type":"file","required":true}}},
             "response":{"format":"json","required_pointers":["/ok"]}}
        ]})
    };

    let dir = root();
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("pic.png");
    std::fs::write(&file, b"synthetic-png-bytes").unwrap();
    let mut args = BTreeMap::new();
    args.insert("file".to_string(), file.to_string_lossy().into_owned());

    // Opt-in derived_target: the cross-origin (different port) upload is allowed.
    let core = runtime();
    let sp = spec::parse(op_json(true).to_string().as_bytes()).unwrap();
    core.install(&sp).unwrap();
    allow_changes(&core, "fixture");
    core.run_op(&sp, "upload", &args).await.unwrap();
    let body = got
        .lock()
        .unwrap()
        .clone()
        .expect("cross-origin upload received");
    assert!(
        body.windows(19).any(|w| w == b"synthetic-png-bytes"),
        "file bytes not delivered cross-origin"
    );

    // Without the opt-in, the same cross-origin target is rejected and never hit.
    *got.lock().unwrap() = None;
    let core2 = runtime();
    let sp2 = spec::parse(op_json(false).to_string().as_bytes()).unwrap();
    core2.install(&sp2).unwrap();
    allow_changes(&core2, "fixture");
    let outcome = core2.run_op(&sp2, "upload", &args).await;
    assert!(
        outcome.is_err(),
        "a plain op must not reach a cross-origin target"
    );
    assert!(
        got.lock().unwrap().is_none(),
        "cross-origin server must not be hit"
    );

    upload_server.abort();
    lease_server.abort();
}
