use super::*;
use axum::{
    Json, Router,
    body::Bytes,
    http::HeaderMap,
    routing::{get, post},
};
use serde_json::json;
use std::sync::atomic::{AtomicUsize, Ordering};
use tokio::sync::Notify;

#[derive(Default)]
struct UploadFixture {
    received: Mutex<Vec<(HeaderMap, Vec<u8>)>>,
    leases: AtomicUsize,
    lease_headers: Mutex<Vec<HeaderMap>>,
    entered: Notify,
    proceed: Notify,
}

async fn host(router: Router) -> (String, tokio::task::JoinHandle<()>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    (
        origin,
        tokio::spawn(async move { axum::serve(listener, router).await.unwrap() }),
    )
}

async fn fixture(
    block_lease: bool,
    injected_path: String,
) -> (String, Arc<UploadFixture>, Vec<tokio::task::JoinHandle<()>>) {
    let state = Arc::new(UploadFixture::default());
    let sink = state.clone();
    let (target, upload_server) = host(Router::new().route(
        "/put",
        post(move |headers: HeaderMap, body: Bytes| {
            let state = sink.clone();
            async move {
                state
                    .received
                    .lock()
                    .unwrap()
                    .push((headers, body.to_vec()));
                Json(json!({"ok":true}))
            }
        }),
    ))
    .await;
    let lease = state.clone();
    let (source, lease_server) = host(Router::new().route(
        "/lease",
        get(move |headers: HeaderMap| {
            let state = lease.clone();
            let target = target.clone();
            let path = injected_path.clone();
            async move {
                state.leases.fetch_add(1, Ordering::SeqCst);
                let signature = headers
                    .get("x-echo-source")
                    .and_then(|value| value.to_str().ok())
                    .unwrap_or("target-signature-canary")
                    .to_owned();
                state.lease_headers.lock().unwrap().push(headers);
                state.entered.notify_one();
                if block_lease {
                    state.proceed.notified().await;
                }
                Json(json!({"base":target,"signature":signature,"file":path}))
            }
        }),
    ))
    .await;
    (source, state, vec![upload_server, lease_server])
}

fn definition(source: &str) -> Value {
    json!({"spec_version":3,"site":{"name":"upload-fixture","base_url":source},"operations":[{
        "name":"upload","method":"POST","path":"/put","base_url":"{{step.lease.base}}","derived_target":true,
        "effect":"write","evidence":"Synthetic upload fixture", "headers":{"X-Signed-Target":"{{step.lease.signature}}"},
        "preflight":[{"name":"lease","path":"/lease","extract":{"base":"json:/base","signature":"json:/signature","file":"json:/file"}}],
        "body":{"multipart":{"file":{"type":"file","required":false},"caption":{"type":"string","default":"fixture-caption"}}},
        "response":{"format":"json","required_pointers":["/ok"]}
    }]})
}
fn configured(value: Value) -> (Arc<Runtime>, Spec) {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join(".cache/upload-safety")
        .join(util::id());
    let core = Runtime::open(&root, None, None, false, true, false).unwrap();
    let sp = spec::parse(value.to_string().as_bytes()).unwrap();
    core.install(&sp).unwrap();
    core.set_writes(&sp.site.name, true).unwrap();
    (core, sp)
}
fn file(core: &Runtime, name: &str, content: &[u8]) -> String {
    let path = core.root.join(name);
    std::fs::write(&path, content).unwrap();
    path.to_string_lossy().into_owned()
}
fn args(path: String) -> BTreeMap<String, String> {
    BTreeMap::from([("file".into(), path)])
}
fn account(core: &Runtime, source: &str) -> String {
    let credential = crate::accounts::Credential {
        origin: source.into(),
        headers: BTreeMap::from([("X-Custom-Session".into(), "source-session-canary".into())]),
        ..Default::default()
    };
    core.register_browser_credential("upload-fixture", credential, "fixture", "fixture")
        .unwrap()
        .id
}
fn body_contains(body: &[u8], expected: &[u8]) -> bool {
    body.windows(expected.len()).any(|bytes| bytes == expected)
}
fn stop(servers: Vec<tokio::task::JoinHandle<()>>) {
    for server in servers {
        server.abort();
    }
}

#[tokio::test]
async fn selected_source_account_uploads_cross_origin_without_forwarding_session_headers() {
    let (source, state, servers) = fixture(false, String::new()).await;
    let (core, sp) = configured(definition(&source));
    let id = account(&core, &source);
    let input = args(file(&core, "explicit.bin", b"explicit-upload-canary"));
    let result = core
        .run_op_for_account(&sp, "upload", &input, Some(&id))
        .await
        .unwrap()
        .0;
    assert_eq!(result.status, 200);
    assert_eq!(
        state.lease_headers.lock().unwrap()[0]["x-custom-session"],
        "source-session-canary"
    );
    let received = state.received.lock().unwrap();
    assert_eq!(received.len(), 1);
    assert!(body_contains(&received[0].1, b"explicit-upload-canary"));
    assert!(!received[0].0.contains_key("x-custom-session"));
    assert_eq!(received[0].0["x-signed-target"], "target-signature-canary");
    drop(received);
    stop(servers);
}

#[tokio::test]
async fn accountless_header_credentials_and_defaults_do_not_leave_source_origin() {
    let (source, state, servers) = fixture(false, String::new()).await;
    let mut value = definition(&source);
    value["auth"] = json!([{"name":"api","kind":"header","header":"X-CuStOm-Api-Key","value_from":"store:upload-fixture/api","origin":source}]);
    value["operations"][0]["auth"] = json!("api");
    value["defaults"] = json!({"headers":{"X-Source-Default":"source-default-canary"}});
    let (core, sp) = configured(value);
    core.set_website_credential("upload-fixture", "api", Some("source-api-canary"))
        .unwrap();
    core.run_op(
        &sp,
        "upload",
        &args(file(&core, "explicit.bin", b"explicit-upload-canary")),
    )
    .await
    .unwrap();
    assert_eq!(
        state.lease_headers.lock().unwrap()[0]["x-custom-api-key"],
        "source-api-canary"
    );
    let received = state.received.lock().unwrap();
    assert_eq!(received.len(), 1);
    assert!(!received[0].0.contains_key("x-custom-api-key"));
    assert!(!received[0].0.contains_key("x-source-default"));
    assert_eq!(received[0].0["x-signed-target"], "target-signature-canary");
    assert!(body_contains(&received[0].1, b"explicit-upload-canary"));
    drop(received);
    stop(servers);
}

#[tokio::test]
async fn account_rotation_disconnect_or_selection_change_during_preflight_prevents_final_upload() {
    for change in ["rotate", "disconnect", "select-another"] {
        let (source, state, servers) = fixture(true, String::new()).await;
        let (core, sp) = configured(definition(&source));
        let id = account(&core, &source);
        let input = args(file(&core, "explicit.bin", b"must-not-upload"));
        let pending_core = core.clone();
        let pending_id = id.clone();
        core.select_account("upload-fixture", &id, &source).unwrap();
        let pending = tokio::spawn(async move {
            pending_core
                .run_op_for_account(
                    &sp,
                    "upload",
                    &input,
                    if change == "select-another" {
                        None
                    } else {
                        Some(&pending_id)
                    },
                )
                .await
        });
        tokio::time::timeout(std::time::Duration::from_secs(5), state.entered.notified())
            .await
            .unwrap();
        if change == "disconnect" {
            core.remove_account(&id).unwrap();
        } else if change == "select-another" {
            let other = account(&core, &source);
            core.select_account("upload-fixture", &other, &source)
                .unwrap();
        } else {
            core.update_credential(&id, |credential| {
                credential
                    .headers
                    .insert("X-Custom-Session".into(), "rotated-session-canary".into());
                Ok(())
            })
            .unwrap();
        }
        state.proceed.notify_one();
        let error = pending
            .await
            .unwrap()
            .err()
            .expect("changed source authority must fail");
        assert_eq!(
            error.public_code,
            if change == "disconnect" {
                "cancelled"
            } else {
                "auth_required"
            },
            "{change}"
        );
        assert_eq!(state.leases.load(Ordering::SeqCst), 1);
        assert!(state.received.lock().unwrap().is_empty());
        stop(servers);
    }
}

#[tokio::test]
async fn preflight_cannot_supply_or_replace_a_local_file_argument() {
    for explicit in [false, true] {
        let private_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join(".cache/upload-safety")
            .join(util::id());
        std::fs::create_dir_all(&private_root).unwrap();
        let private_path = private_root.join("private.bin");
        std::fs::write(&private_path, b"private-file-must-not-leave").unwrap();
        let (source, state, servers) =
            fixture(false, private_path.to_string_lossy().into_owned()).await;
        let (core, sp) = configured(definition(&source));
        let input = if explicit {
            args(file(&core, "chosen.bin", b"explicit-file-canary"))
        } else {
            BTreeMap::new()
        };
        core.run_op(&sp, "upload", &input).await.unwrap();
        let received = state.received.lock().unwrap();
        assert_eq!(received.len(), 1);
        assert!(!body_contains(
            &received[0].1,
            b"private-file-must-not-leave"
        ));
        assert_eq!(
            body_contains(&received[0].1, b"explicit-file-canary"),
            explicit
        );
        assert_eq!(body_contains(&received[0].1, b"filename="), explicit);
        assert!(body_contains(&received[0].1, b"fixture-caption"));
        drop(received);
        stop(servers);
    }
}

#[tokio::test]
async fn missing_required_file_and_browser_transport_fail_before_preflight() {
    for transport in ["", "browser", "browser-fetch"] {
        let (source, state, servers) = fixture(false, String::new()).await;
        let mut value = definition(&source);
        value["operations"][0]["transport"] = json!(transport);
        value["operations"][0]["body"]["multipart"]["file"]["required"] = json!(true);
        if transport == "browser" {
            let (core, _) = configured(definition(&source));
            let invalid = spec::parse(value.to_string().as_bytes()).unwrap();
            assert!(core.install(&invalid).is_err());
            assert_eq!(state.leases.load(Ordering::SeqCst), 0);
            assert!(state.received.lock().unwrap().is_empty());
            stop(servers);
            continue;
        }
        let (core, sp) = configured(value);
        let input = if transport.is_empty() {
            BTreeMap::new()
        } else {
            args(file(&core, "chosen.bin", b"explicit-file-canary"))
        };
        let error = core
            .run_op(&sp, "upload", &input)
            .await
            .err()
            .expect("unsupported or incomplete upload must fail");
        if !transport.is_empty() {
            assert_eq!(error.public_code, "unsupported_transport");
        }
        assert_eq!(state.leases.load(Ordering::SeqCst), 0);
        assert!(state.received.lock().unwrap().is_empty());
        stop(servers);
    }
}

#[test]
fn all_file_defaults_are_rejected_even_when_optional() {
    for required in [false, true] {
        let mut value = definition("https://example.test");
        value["operations"][0]["body"]["multipart"]["file"] =
            json!({"type":"file","required":required,"default":"private.bin"});
        let sp = spec::parse(value.to_string().as_bytes()).unwrap();
        assert!(crate::policy::validate_spec(&sp).is_err());
        assert!(
            sp.build_multipart(&sp.operations[0], &BTreeMap::new())
                .is_err()
        );
    }
}

#[tokio::test]
async fn direct_network_dispatch_rejects_read_only_files_and_browser_multipart() {
    let (source, state, servers) = fixture(false, String::new()).await;
    let (core, _) = configured(definition(&source));
    let request = |read_only| crate::net::Request {
        url: format!("{source}/lease"),
        method: "POST".into(),
        headers: vec![],
        body: None,
        account_id: String::new(),
        limit: 4096,
        timeout: std::time::Duration::from_secs(5),
        read_only,
        follow_redirects: false,
        tls_profile: String::new(),
        multipart: Some(vec![crate::spec::MultipartPart::File {
            name: "file".into(),
            path: core
                .root
                .join("does-not-exist.bin")
                .to_string_lossy()
                .into_owned(),
        }]),
    };
    let error = core
        .network
        .fetch(request(true))
        .await
        .err()
        .expect("read-only file must fail before file I/O");
    assert_eq!(error.public_code, "approval_required");
    let credential = crate::accounts::Credential {
        origin: source.clone(),
        ..Default::default()
    };
    let error = core
        .network
        .fetch_browser_in_page(&credential, &request(false))
        .await
        .err()
        .expect("browser multipart must fail before connecting to a browser");
    assert_eq!(error.public_code, "unsupported_transport");
    assert_eq!(state.leases.load(Ordering::SeqCst), 0);
    assert!(state.received.lock().unwrap().is_empty());
    stop(servers);
}

#[tokio::test]
async fn read_and_search_labels_cannot_bypass_file_write_approval() {
    for name in ["read-file", "search"] {
        let (source, state, servers) = fixture(false, String::new()).await;
        let mut value = definition(&source);
        value["operations"][0]["name"] = json!(name);
        value["operations"][0]["effect"] = json!("read");
        let (core, sp) = configured(value);
        let input = args(
            core.root
                .join("does-not-exist.bin")
                .to_string_lossy()
                .into_owned(),
        );
        let error = core
            .run_op(&sp, name, &input)
            .await
            .err()
            .expect("a file transfer is a write regardless of its label");
        assert_eq!(error.public_code, "approval_required");
        assert_eq!(state.leases.load(Ordering::SeqCst), 0);
        assert!(state.received.lock().unwrap().is_empty());
        stop(servers);
    }
}

#[tokio::test]
async fn same_origin_upload_preserves_selected_account_authentication() {
    let received = Arc::new(Mutex::new(Vec::<(HeaderMap, Vec<u8>)>::new()));
    let sink = received.clone();
    let (source, server) = host(Router::new().route(
        "/put",
        post(move |headers: HeaderMap, body: Bytes| {
            let sink = sink.clone();
            async move {
                sink.lock().unwrap().push((headers, body.to_vec()));
                Json(json!({"ok":true}))
            }
        }),
    ))
    .await;
    let mut value = definition(&source);
    value["operations"][0]["base_url"] = json!(source);
    value["operations"][0]["preflight"] = json!([]);
    value["defaults"] = json!({"headers":{"Content-Type":"application/json"}});
    value["operations"][0]["headers"] = json!({"content-type":"application/octet-stream"});
    let (core, sp) = configured(value);
    let id = account(&core, &source);
    core.run_op_for_account(
        &sp,
        "upload",
        &args(file(&core, "explicit.bin", b"same-origin-file-canary")),
        Some(&id),
    )
    .await
    .unwrap();
    let received = received.lock().unwrap();
    assert_eq!(received.len(), 1);
    assert_eq!(received[0].0["x-custom-session"], "source-session-canary");
    assert_eq!(received[0].0.get_all("content-type").iter().count(), 1);
    assert!(
        received[0].0["content-type"]
            .to_str()
            .unwrap()
            .starts_with("multipart/form-data; boundary=")
    );
    assert!(body_contains(&received[0].1, b"same-origin-file-canary"));
    drop(received);
    server.abort();
}

#[tokio::test]
async fn account_snapshot_is_rechecked_after_network_gate_wait() {
    let hits = Arc::new(AtomicUsize::new(0));
    let counted = hits.clone();
    let (target, server) = host(Router::new().route(
        "/put",
        post(move || {
            counted.fetch_add(1, Ordering::SeqCst);
            async { Json(json!({"ok":true})) }
        }),
    ))
    .await;
    let source = "http://127.0.0.1:1";
    let (core, _) = configured(definition(source));
    let id = account(&core, source);
    let snapshot = core.credential(&id).unwrap();
    let scope = format!("{target}|*");
    let token = core.policy.acquire(&scope).await.unwrap();
    let entered = Arc::new(Notify::new());
    let pending_entered = entered.clone();
    let pending_core = core.clone();
    let pending_id = id.clone();
    let path = file(&core, "explicit.bin", b"must-not-upload-after-rotation");
    let pending = tokio::spawn(async move {
        let check = || {
            pending_core.ensure_credential_snapshot(
                "upload-fixture",
                source,
                &pending_id,
                Some(&snapshot),
            )?;
            pending_entered.notify_one();
            Ok(())
        };
        pending_core
            .network
            .fetch_guarded(
                crate::net::Request {
                    url: format!("{target}/put"),
                    method: "POST".into(),
                    headers: vec![],
                    body: None,
                    account_id: pending_id.clone(),
                    limit: 4096,
                    timeout: std::time::Duration::from_secs(5),
                    read_only: false,
                    follow_redirects: false,
                    tls_profile: String::new(),
                    multipart: Some(vec![crate::spec::MultipartPart::File {
                        name: "file".into(),
                        path,
                    }]),
                },
                None,
                &check,
            )
            .await
    });
    tokio::time::timeout(std::time::Duration::from_secs(5), entered.notified())
        .await
        .unwrap();
    assert_eq!(hits.load(Ordering::SeqCst), 0);
    core.update_credential(&id, |credential| {
        credential
            .headers
            .insert("X-Custom-Session".into(), "rotated-session-canary".into());
        Ok(())
    })
    .unwrap();
    core.policy
        .release(&scope, &token, 200, None, false)
        .unwrap();
    let error = pending
        .await
        .unwrap()
        .err()
        .expect("changed authority after initial check must prevent dispatch");
    assert_eq!(error.public_code, "auth_required");
    assert_eq!(hits.load(Ordering::SeqCst), 0);
    server.abort();
}

#[tokio::test]
async fn source_cookie_cannot_escape_through_derived_query_or_generated_text() {
    for destination in ["query", "text", "form", "json", "json-template"] {
        let (source, state, servers) = fixture(false, String::new()).await;
        let mut value = definition(&source);
        value["operations"][0]["preflight"][0]["extract"]["session"] = json!("cookie:sid");
        if destination == "query" {
            value["operations"][0]["query"] =
                json!({"lease_proof":{"type":"string","default":"{{step.lease.session}}"}});
        } else if destination == "text" {
            value["operations"][0]["body"]["multipart"]["caption"]["default"] =
                json!("{{step.lease.session}}");
        } else if destination == "json-template" {
            value["operations"][0]["body"] =
                json!({"json_template":{"lease_proof":"{{step.lease.session}}"}});
        } else {
            value["operations"][0]["body"] = json!({destination:{"lease_proof":{"type":"string","default":"{{step.lease.session}}"}}});
        }
        let (core, sp) = configured(value);
        let id = account(&core, &source);
        core.update_credential(&id, |credential| {
            credential.cookies.push(
                serde_json::from_value(
                    json!({"name":"sid","value":"source-cookie-canary","domain":"127.0.0.1"}),
                )
                .unwrap(),
            );
            Ok(())
        })
        .unwrap();
        let input = if ["query", "text"].contains(&destination) {
            args(file(&core, "explicit.bin", b"explicit-upload-canary"))
        } else {
            BTreeMap::new()
        };
        let error = core
            .run_op_for_account(&sp, "upload", &input, Some(&id))
            .await
            .err()
            .expect("source-cookie-derived destination data must be rejected");
        assert_eq!(error.public_code, "unsafe_target");
        assert_eq!(state.leases.load(Ordering::SeqCst), 1);
        assert!(state.received.lock().unwrap().is_empty());
        stop(servers);
    }
}

#[tokio::test]
async fn missing_or_invalid_lease_destination_never_dispatches_upload() {
    for expression in ["json:/missing", "json:/signature"] {
        let (source, state, servers) = fixture(false, String::new()).await;
        let mut value = definition(&source);
        value["operations"][0]["preflight"][0]["extract"]["base"] = json!(expression);
        let (core, sp) = configured(value);
        let input = args(file(
            &core,
            "explicit.bin",
            b"must-not-upload-without-target",
        ));
        assert!(core.run_op(&sp, "upload", &input).await.is_err());
        assert_eq!(state.leases.load(Ordering::SeqCst), 1);
        assert!(state.received.lock().unwrap().is_empty());
        stop(servers);
    }
}

#[tokio::test]
async fn full_signed_lease_url_keeps_its_query_and_upload_body() {
    let received = Arc::new(Mutex::new(Vec::<(String, Vec<u8>)>::new()));
    let sink = received.clone();
    let (target, upload_server) = host(Router::new().route(
        "/put",
        post(move |uri: axum::extract::OriginalUri, body: Bytes| {
            let sink = sink.clone();
            async move {
                sink.lock()
                    .unwrap()
                    .push((uri.0.to_string(), body.to_vec()));
                Json(json!({"ok":true}))
            }
        }),
    ))
    .await;
    let lease_url = format!("{target}/put?signature=signed%2Bvalue%2Fpart&expires=9999999999");
    let (source, lease_server) = host(Router::new().route(
        "/lease",
        get(move || {
            let lease_url = lease_url.clone();
            async move { Json(json!({"base":lease_url,"signature":"target-signature-canary"})) }
        }),
    ))
    .await;
    let mut value = definition(&source);
    value["operations"][0]["path"] = json!("");
    let (core, sp) = configured(value);
    core.run_op(
        &sp,
        "upload",
        &args(file(&core, "chosen.bin", b"signed-url-upload-canary")),
    )
    .await
    .unwrap();
    let received = received.lock().unwrap();
    assert_eq!(received.len(), 1);
    assert_eq!(
        received[0].0,
        "/put?signature=signed%2Bvalue%2Fpart&expires=9999999999"
    );
    assert!(body_contains(&received[0].1, b"signed-url-upload-canary"));
    drop(received);
    lease_server.abort();
    upload_server.abort();
}

#[tokio::test]
async fn explicit_text_is_literal_and_static_checksum_is_retained_cross_origin() {
    let (source, state, servers) = fixture(false, String::new()).await;
    let mut value = definition(&source);
    value["operations"][0]["preflight"][0]["extract"]["session"] = json!("cookie:sid");
    value["operations"][0]["headers"]["Content-MD5"] = json!("qw7dH5Zodkp3lBwI/oUozg==");
    let (core, sp) = configured(value);
    let id = account(&core, &source);
    core.update_credential(&id, |credential| {
        credential.cookies.push(
            serde_json::from_value(
                json!({"name":"sid","value":"source-cookie-canary","domain":"127.0.0.1"}),
            )
            .unwrap(),
        );
        Ok(())
    })
    .unwrap();
    let mut input = args(file(&core, "explicit.bin", b"explicit-upload-canary"));
    input.insert("caption".into(), "{{step.lease.session}}".into());
    core.run_op_for_account(&sp, "upload", &input, Some(&id))
        .await
        .unwrap();
    let received = state.received.lock().unwrap();
    assert_eq!(received.len(), 1);
    assert_eq!(received[0].0["content-md5"], "qw7dH5Zodkp3lBwI/oUozg==");
    assert!(body_contains(&received[0].1, b"{{step.lease.session}}"));
    assert!(!body_contains(&received[0].1, b"source-cookie-canary"));
    assert!(body_contains(&received[0].1, b"explicit-upload-canary"));
    drop(received);
    stop(servers);
}

#[tokio::test]
async fn upload_approval_cannot_be_reused_for_a_different_file() {
    let (source, state, servers) = fixture(false, String::new()).await;
    let (core, sp) = configured(definition(&source));
    let original = core
        .request(
            &sp,
            "upload",
            args(file(&core, "approved.bin", b"approved-file-canary")),
            None,
        )
        .unwrap();
    let approval = core.approval(original.clone(), "en").unwrap();
    let changed = core
        .request(
            &sp,
            "upload",
            args(file(&core, "different.bin", b"different-file-canary")),
            None,
        )
        .unwrap();
    let error = core
        .execute(changed, Some(&approval.id))
        .await
        .err()
        .expect("approval is bound to the exact file argument");
    assert_eq!(error.public_code, "approval_expired");
    assert_eq!(state.leases.load(Ordering::SeqCst), 0);
    assert!(state.received.lock().unwrap().is_empty());
    core.execute(original.clone(), Some(&approval.id))
        .await
        .unwrap();
    let received = state.received.lock().unwrap();
    assert_eq!(received.len(), 1);
    assert!(body_contains(&received[0].1, b"approved-file-canary"));
    assert!(!body_contains(&received[0].1, b"different-file-canary"));
    drop(received);
    let error = core
        .execute(original, Some(&approval.id))
        .await
        .err()
        .expect("consumed approval cannot send the same file twice");
    assert_eq!(error.public_code, "approval_expired");
    assert_eq!(state.leases.load(Ordering::SeqCst), 1);
    assert_eq!(state.received.lock().unwrap().len(), 1);
    stop(servers);
}

#[tokio::test]
async fn configured_auth_is_secret_even_with_metadata_header_names_or_template_braces() {
    for (header, secret) in [
        ("Accept", "target-signature-canary"),
        ("User-Agent", "target-signature-canary"),
        ("Content-Type", "target-signature-canary"),
        ("X-Echo-Source", "opaque{{credential}}value"),
    ] {
        let (source, state, servers) = fixture(false, String::new()).await;
        let mut value = definition(&source);
        value["auth"] = json!([{"name":"api","kind":"header","header":header,"value_from":"store:upload-fixture/api","origin":source}]);
        value["operations"][0]["auth"] = json!("api");
        let (core, sp) = configured(value);
        core.set_website_credential("upload-fixture", "api", Some(secret))
            .unwrap();
        let outcome = core
            .run_op(
                &sp,
                "upload",
                &args(file(&core, "explicit.bin", b"not-dispatched")),
            )
            .await;
        assert_eq!(outcome.err().unwrap().public_code, "unsafe_target");
        assert_eq!(state.leases.load(Ordering::SeqCst), 1);
        assert!(state.received.lock().unwrap().is_empty());
        stop(servers);
    }
}

#[tokio::test]
async fn same_origin_preflight_preserves_literal_credentials_and_resolves_declared_headers() {
    for header in [
        "Authorization",
        "X-Custom-Key",
        "User-Agent",
        "X-Custom-Session",
    ] {
        let received = Arc::new(Mutex::new(Vec::<(HeaderMap, Bytes)>::new()));
        let sink = received.clone();
        let (source, server) = host(
            Router::new()
                .route(
                    "/lease",
                    get(|| async {
                        Json(json!({"signature":"lease-header-canary","credential":"must-not-expand"}))
                    }),
                )
                .route(
                    "/put",
                    post(move |headers: HeaderMap, body: Bytes| {
                        let sink = sink.clone();
                        async move {
                            // Success requires consuming the entire streaming multipart request.
                            sink.lock().unwrap().push((headers, body));
                            Json(json!({"ok":true}))
                        }
                    }),
                ),
        )
        .await;
        let secret = "opaque{{credential}}value";
        let mut value = definition(&source);
        value["operations"][0]["base_url"] = json!(source);
        value["operations"][0]["headers"] = json!({"X-Lease":"{{step.lease.signature}}"});
        value["defaults"] = json!({"headers":{"X-Default-Lease":"{{step.lease.signature}}"}});
        let selected = header == "X-Custom-Session";
        if !selected {
            value["auth"] = json!([{"name":"api","kind":"header","header":header,"prefix":"Bearer ","value_from":"store:upload-fixture/api","origin":source}]);
            value["operations"][0]["auth"] = json!("api");
        }
        let (core, sp) = configured(value);
        let account_id = if selected {
            Some(
                core.register_browser_credential(
                    "upload-fixture",
                    crate::accounts::Credential {
                        origin: source.clone(),
                        headers: BTreeMap::from([(header.into(), secret.into())]),
                        ..Default::default()
                    },
                    "fixture",
                    "fixture",
                )
                .unwrap()
                .id,
            )
        } else {
            core.set_website_credential("upload-fixture", "api", Some(secret))
                .unwrap();
            None
        };
        core.run_op_for_account(
            &sp,
            "upload",
            &args(file(&core, "explicit.bin", b"literal-credential-upload")),
            account_id.as_deref(),
        )
        .await
        .unwrap();
        let received = received.lock().unwrap();
        assert_eq!(received.len(), 1);
        let (headers, body) = &received[0];
        assert!(body_contains(
            body,
            b"name=\"file\"; filename=\"explicit.bin\""
        ));
        assert!(body_contains(
            body,
            b"\r\n\r\nliteral-credential-upload\r\n"
        ));
        let boundary = headers["content-type"]
            .to_str()
            .unwrap()
            .split("boundary=")
            .nth(1)
            .unwrap();
        assert!(body.ends_with(format!("\r\n--{boundary}--\r\n").as_bytes()));
        assert_eq!(
            headers[header].to_str().unwrap(),
            if selected {
                secret.to_owned()
            } else {
                format!("Bearer {secret}")
            }
        );
        assert_eq!(headers["x-lease"], "lease-header-canary");
        assert_eq!(headers["x-default-lease"], "lease-header-canary");
        drop(received);
        server.abort();
    }
}
