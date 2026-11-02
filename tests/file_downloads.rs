use axum::{
    Router,
    body::Body,
    http::{Response, StatusCode},
    routing::{get, post},
};
use hycli::{
    artifacts::{ArtifactStore, CompletedArtifact, TRANSFER_CHUNK_BYTES},
    runtime::Runtime,
    util,
};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    process::Stdio,
    time::Duration,
};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
const PAYLOAD_SHA256: &str = "4e21f184e5bdb39d60fcc05e19f7fc94be6f3b029a57101b27e36beda24e1fc2";
const PAYLOAD: &[u8] = b"\x00\xff\xfe%PDF\r\nexact-byte-fixture";
fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join(".cache/file-downloads")
        .join(util::id())
}
fn assert_no_artifacts(root: &Path) {
    let directory = root.join("artifacts");
    assert!(!directory.exists() || std::fs::read_dir(directory).unwrap().next().is_none());
}
fn definition(origin: &str) -> hycli::spec::Spec {
    hycli::spec::parse(json!({"spec_version":3,"site":{"name":"files-fixture","base_url":origin},"operations":[{"name":"download","method":"GET","path":"/file","effect":"read","evidence":"Owned binary fixture","response":{"format":"file","file":{"media_types":["application/pdf"]}}},{"name":"json","method":"GET","path":"/json","effect":"read","evidence":"Owned JSON read fixture","response":{"format":"json"}}]}).to_string().as_bytes()).unwrap()
}
async fn server() -> (String, tokio::task::JoinHandle<()>) {
    let app = Router::new()
        .route(
            "/file",
            get(|| async {
                Response::builder()
                    .header("content-type", "application/pdf")
                    .header(
                        "content-disposition",
                        "attachment; filename=\"../../report.pdf\"",
                    )
                    .body(Body::from(PAYLOAD))
                    .unwrap()
            }),
        )
        .route(
            "/file",
            post(|| async {
                Response::builder()
                    .header("content-type", "application/pdf")
                    .body(Body::from(PAYLOAD))
                    .unwrap()
            }),
        )
        .route(
            "/upload",
            post(
                |headers: axum::http::HeaderMap, body: axum::body::Bytes| async move {
                    let boundary = headers["content-type"]
                        .to_str()
                        .unwrap()
                        .split("boundary=")
                        .nth(1)
                        .unwrap();
                    let marker = b"\r\n\r\n";
                    let start = body
                        .windows(marker.len())
                        .position(|v| v == marker)
                        .unwrap()
                        + marker.len();
                    let suffix = format!("\r\n--{boundary}--\r\n");
                    let bytes = &body[start..body.len() - suffix.len()];
                    assert_eq!(bytes, PAYLOAD);
                    axum::Json(json!({"sha256": util::hash(bytes), "byte_count": bytes.len()}))
                },
            ),
        )
        .route(
            "/json",
            get(|| async { axum::Json(json!({"items":[1,2]})) }),
        )
        .route(
            "/login",
            get(|| async {
                Response::builder()
                    .header("content-type", "text/html")
                    .body(Body::from("<form><input type=\"password\"></form>"))
                    .unwrap()
            }),
        )
        .route(
            "/denied",
            get(|| async { (StatusCode::FORBIDDEN, "denied") }),
        )
        .route(
            "/large",
            get(|| async {
                Response::builder()
                    .header("content-type", "application/pdf")
                    .header(
                        "content-length",
                        (1601u64 * TRANSFER_CHUNK_BYTES as u64).to_string(),
                    )
                    .body(Body::from_stream(futures_util::stream::iter(
                        (0..1601).map(|_| Ok::<_, std::io::Error>(vec![0u8; TRANSFER_CHUNK_BYTES])),
                    )))
                    .unwrap()
            }),
        )
        .route(
            "/chunked-large",
            get(|| async {
                Response::builder()
                    .header("content-type", "application/pdf")
                    .body(Body::from_stream(futures_util::stream::iter(
                        (0..1601).map(|_| Ok::<_, std::io::Error>(vec![0u8; TRANSFER_CHUNK_BYTES])),
                    )))
                    .unwrap()
            }),
        );
    let app = app.route(
        "/interrupted",
        get(|| async {
            Response::builder()
                .header("content-type", "application/pdf")
                .body(Body::from_stream(futures_util::stream::iter([
                    Ok::<_, std::io::Error>(PAYLOAD.to_vec()),
                    Err(std::io::Error::other("synthetic interrupted response")),
                ])))
                .unwrap()
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    (
        origin,
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() }),
    )
}
fn assert_artifact(value: Value) -> CompletedArtifact {
    let manifest: CompletedArtifact = serde_json::from_value(value).unwrap();
    assert_eq!(manifest.kind, "file");
    assert_eq!(manifest.status, "complete");
    let bytes = std::fs::read(&manifest.local_path).unwrap();
    assert_eq!(bytes, PAYLOAD);
    assert_eq!(manifest.sha256, PAYLOAD_SHA256);
    assert_eq!(manifest.byte_count, PAYLOAD.len() as u64);
    assert_eq!(manifest.content_trust, "untrusted");
    assert!(!manifest.filename.contains('/'));
    assert!(!manifest.filename.contains('\\'));
    manifest
}
#[tokio::test]
async fn explicit_execution_preserves_bytes_but_candidate_verification_does_not_persist() {
    let (origin, server) = server().await;
    let root = root();
    let core = Runtime::open(&root, None, None, false, true, false).unwrap();
    let sp = definition(&origin);
    assert!(sp.validate().is_empty(), "{:?}", sp.validate());
    hycli::policy::validate_spec(&sp).expect("file contract passes shared execution policy");
    let (verified, _) = core
        .verify_candidate_with_inputs(&sp, "download", "", BTreeMap::new())
        .await
        .unwrap();
    assert_eq!(verified.body.unwrap()["persisted"], false);
    assert_no_artifacts(&root);
    let (result, _) = core
        .run_op(&sp, "download", &BTreeMap::new())
        .await
        .unwrap();
    let manifest = assert_artifact(result.output().unwrap());
    let opened = ArtifactStore::open(&root)
        .unwrap()
        .open_completed(&manifest.artifact_id)
        .unwrap();
    let stream = opened.into_stream();
    futures_util::pin_mut!(stream);
    use futures_util::StreamExt;
    let mut received = Vec::new();
    while let Some(chunk) = stream.next().await {
        received.extend(chunk.unwrap());
    }
    assert_eq!(received, PAYLOAD);
    let (json, _) = core.run_op(&sp, "json", &BTreeMap::new()).await.unwrap();
    assert_eq!(json.output().unwrap(), json!({"items":[1,2]}));
    server.abort();
}
#[tokio::test]
async fn repeated_recorded_file_checks_do_not_publish_artifacts_or_create_jobs() {
    let (origin, server) = server().await;
    let root = root();
    let core = Runtime::open(&root, None, None, false, true, false).unwrap();
    let mut sp = definition(&origin);
    let mut write = sp.operations[0].clone();
    write.name = "write".into();
    write.method = "POST".into();
    write.effect = hycli::policy::Effect::Write;
    sp.operations.push(write);
    core.install(&sp).unwrap();
    core.state
        .update(|data| {
            data.sites.entry(sp.site.name.clone()).or_default().smoke =
                ["download", "json", "write"]
                    .into_iter()
                    .map(|action| hycli::smoke::ReadCheck {
                        action: action.into(),
                        inputs: BTreeMap::new(),
                    })
                    .collect();
            Ok(())
        })
        .unwrap();
    for _ in 0..3 {
        let report = core.check_site(&sp.site.name).await.unwrap();
        assert_eq!(report.verdict, "ok");
        assert_eq!(report.account, None);
        assert_eq!(report.checks.len(), 3);
        assert_eq!(report.checks[0].outcome, "ok");
        assert_eq!(report.checks[1].outcome, "ok");
        assert_eq!(report.checks[2].outcome, "skipped");
        assert_eq!(report.checks[2].error, "not_a_read");
        assert_no_artifacts(&root);
        assert!(core.state.read().unwrap().jobs.is_empty());
    }
    let (result, _) = core
        .run_op(&sp, "download", &BTreeMap::new())
        .await
        .unwrap();
    let manifest = assert_artifact(result.output().unwrap());
    assert_eq!(core.state.read().unwrap().jobs.len(), 1);
    let report = core.check_site(&sp.site.name).await.unwrap();
    assert_eq!(report.verdict, "ok");
    assert_eq!(
        std::fs::read_dir(root.join("artifacts")).unwrap().count(),
        1
    );
    assert_eq!(core.state.read().unwrap().jobs.len(), 1);
    assert_artifact(serde_json::to_value(manifest).unwrap());
    server.abort();
}

#[tokio::test]
async fn completed_action_persists_authoritative_artifact_separately_from_server_json() {
    let (origin, server) = server().await;
    let root = root();
    let core = Runtime::open(&root, None, None, false, true, false).unwrap();
    let sp = definition(&origin);
    core.install(&sp).unwrap();
    let request = core
        .request(&sp, "download", BTreeMap::new(), None)
        .unwrap();
    let receipt = core.start_action(request, None, "en").unwrap();
    let completed = tokio::time::timeout(Duration::from_secs(15), async {
        loop {
            let job = core.state.job(&receipt.id).unwrap();
            if !["queued", "running"].contains(&job.status.as_str()) {
                break job;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    assert_eq!(completed.status, "completed", "{}", completed.error_code);
    let artifact = completed.artifact.unwrap();
    assert_artifact(completed.result.unwrap());
    drop(core);
    let reopened = Runtime::open(&root, None, None, false, true, false).unwrap();
    assert_eq!(
        reopened.state.job(&receipt.id).unwrap().artifact,
        Some(artifact)
    );
    server.abort();
}

#[test]
fn file_expectations_reject_incompatible_contracts() {
    let mut sp = definition("https://example.test");
    sp.operations[0]
        .response
        .as_mut()
        .unwrap()
        .file
        .as_mut()
        .unwrap()
        .media_types = vec!["text/html".into()];
    sp.operations[0].model = "record".into();
    assert!(!sp.validate().is_empty());
    sp.operations[0].model.clear();
    sp.operations[0].transport = "browser-fetch".into();
    assert!(!sp.validate().is_empty());
    sp.operations[0].transport.clear();
    sp.operations[0]
        .response
        .as_mut()
        .unwrap()
        .file
        .as_mut()
        .unwrap()
        .media_types = vec!["*/*".into()];
    assert!(!sp.validate().is_empty());
}

#[tokio::test]
async fn failed_responses_never_publish_completed_artifacts() {
    let (origin, server) = server().await;
    for path in ["/login", "/denied", "/interrupted"] {
        let root = root();
        let core = Runtime::open(&root, None, None, false, true, false).unwrap();
        let mut sp = definition(&origin);
        sp.operations[0].path = path.into();
        assert!(
            core.run_op(&sp, "download", &BTreeMap::new())
                .await
                .is_err(),
            "{path}"
        );
        assert_no_artifacts(&root);
    }
    server.abort();
}
#[tokio::test]
async fn storage_failure_does_not_report_a_file_manifest() {
    let (origin, server) = server().await;
    let root = root();
    let core = Runtime::open(&root, None, None, false, true, false).unwrap();
    std::fs::write(root.join("artifacts"), b"existing unrelated file").unwrap();
    let result = core
        .run_op(&definition(&origin), "download", &BTreeMap::new())
        .await;
    assert!(result.is_err());
    assert_eq!(
        std::fs::read(root.join("artifacts")).unwrap(),
        b"existing unrelated file"
    );
    server.abort();
}

#[tokio::test]
async fn successful_write_with_failed_local_publication_is_not_safely_retryable() {
    let dispatched = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let counter = dispatched.clone();
    let handler = move || {
        let counter = counter.clone();
        async move {
            counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            Response::builder()
                .header("content-type", "application/pdf")
                .body(Body::from(PAYLOAD))
                .unwrap()
        }
    };
    let app = Router::new().route("/file", get(handler.clone()).post(handler));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    for write in [false, true] {
        let root = root();
        let core = Runtime::open(&root, None, None, false, true, false).unwrap();
        let mut sp = definition(&origin);
        if write {
            sp.operations[0].method = "POST".into();
            sp.operations[0].effect = hycli::policy::Effect::Write;
            core.install(&sp).unwrap();
            core.set_writes(&sp.site.name, true).unwrap();
        }
        std::fs::write(root.join("artifacts"), b"unrelated file").unwrap();
        let error = match core.run_op(&sp, "download", &BTreeMap::new()).await {
            Err(error) => error,
            Ok(_) => panic!("failed artifact publication must not report success"),
        };
        assert_eq!(error.public_code == "action_outcome_unknown", write);
        assert_eq!(
            std::fs::read(root.join("artifacts")).unwrap(),
            b"unrelated file"
        );
    }
    assert_eq!(dispatched.load(std::sync::atomic::Ordering::SeqCst), 2);
    server.abort();
}

#[tokio::test]
async fn nested_persistence_failure_preserves_the_remote_write_outcome() {
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };

    for write in [false, true] {
        let root = root();
        let core = Runtime::open(&root, None, None, false, true, false).unwrap();
        let db_path = core.state.db_path().to_owned();
        let dispatched = Arc::new(AtomicUsize::new(0));
        let counter = dispatched.clone();
        let handler = move || {
            let db_path = db_path.clone();
            let counter = counter.clone();
            async move {
                counter.fetch_add(1, Ordering::SeqCst);
                // Arm only after the request reached the service. Both the normal
                // completion and its fallback must fail, not pre-dispatch setup.
                rusqlite::Connection::open(db_path)
                    .unwrap()
                    .execute_batch(
                        "CREATE TRIGGER reject_action_completion
                         BEFORE UPDATE ON jobs
                         WHEN NEW.kind = 'action'
                           AND NEW.status IN ('completed', 'failed')
                         BEGIN
                           SELECT RAISE(ABORT, 'synthetic completion persistence failure');
                         END;",
                    )
                    .unwrap();
                Response::builder()
                    .header("content-type", "application/pdf")
                    .body(Body::from(PAYLOAD))
                    .unwrap()
            }
        };
        let route = if write { post(handler) } else { get(handler) };
        let app = Router::new().route("/file", route);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let origin = format!("http://{}", listener.local_addr().unwrap());
        let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let mut sp = definition(&origin);
        if write {
            sp.operations[0].method = "POST".into();
            sp.operations[0].effect = hycli::policy::Effect::Write;
        }
        // Installed operations use the observed execution path shared by CLI/MCP.
        core.install(&sp).unwrap();
        if write {
            core.set_writes(&sp.site.name, true).unwrap();
        }
        std::fs::write(root.join("artifacts"), b"unrelated file").unwrap();
        let result = tokio::time::timeout(
            Duration::from_secs(15),
            core.run_op(&sp, "download", &BTreeMap::new()),
        )
        .await
        .expect("the injected storage failure must not hang execution");
        let error = match result {
            Err(error) => error,
            Ok(_) => panic!("failed publication and job persistence must not report success"),
        };
        assert_eq!(
            error.public_code,
            if write {
                "action_outcome_unknown"
            } else {
                "internal"
            }
        );
        assert_eq!(dispatched.load(Ordering::SeqCst), 1);
        assert_eq!(
            std::fs::read(root.join("artifacts")).unwrap(),
            b"unrelated file"
        );
        let jobs = core.state.read().unwrap().jobs;
        assert_eq!(jobs.len(), 1);
        // Neither completion write can succeed while the trigger is armed.
        // A caller must still receive the unsafe-to-retry error for a POST.
        assert_eq!(jobs[0].status, "running");
        assert!(jobs[0].artifact.is_none());
        assert!(jobs[0].result.is_none());
        server.abort();
    }
}

#[tokio::test]
async fn cancellation_during_body_receipt_does_not_publish_a_partial_file() {
    use futures_util::StreamExt;
    let received = std::sync::Arc::new(tokio::sync::Notify::new());
    let signal = received.clone();
    let app = Router::new().route(
        "/file",
        get(move || {
            let signal = signal.clone();
            async move {
                signal.notify_one();
                Response::builder()
                    .header("content-type", "application/pdf")
                    .body(Body::from_stream(
                        futures_util::stream::once(async {
                            Ok::<_, std::io::Error>(PAYLOAD.to_vec())
                        })
                        .chain(futures_util::stream::pending()),
                    ))
                    .unwrap()
            }
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let root = root();
    let core = Runtime::open(&root, None, None, false, true, false).unwrap();
    let request = tokio::spawn(async move {
        core.run_op(&definition(&origin), "download", &BTreeMap::new())
            .await
    });
    tokio::time::timeout(Duration::from_secs(10), received.notified())
        .await
        .unwrap();
    request.abort();
    assert!(
        request
            .await
            .err()
            .expect("cancelled transfer")
            .is_cancelled()
    );
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if !root.join("artifacts").exists()
                || std::fs::read_dir(root.join("artifacts"))
                    .unwrap()
                    .next()
                    .is_none()
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    assert_no_artifacts(&root);
    server.abort();
}

fn command(root: &Path, args: &[&str]) -> tokio::process::Command {
    let mut cmd = tokio::process::Command::new(
        std::env::var_os("HYCLI_TEST_EXECUTABLE")
            .unwrap_or_else(|| env!("CARGO_BIN_EXE_hycli").into()),
    );
    cmd.args(args)
        .env("HYCLI_DATA_DIR", root)
        .env("HYCLI_SPECS", root.join("specs"))
        .env("HYCLI_KB", root.join("kb.db"));
    cmd
}
async fn rpc(
    stdin: &mut tokio::process::ChildStdin,
    stdout: &mut tokio::io::Lines<BufReader<tokio::process::ChildStdout>>,
    id: i64,
    method: &str,
    params: Value,
) -> Value {
    stdin
        .write_all(
            format!(
                "{}\n",
                json!({"jsonrpc":"2.0","id":id,"method":method,"params":params})
            )
            .as_bytes(),
        )
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(45), async {
        loop {
            let line = stdout.next_line().await.unwrap().expect("MCP closed");
            let value: Value = serde_json::from_str(&line).unwrap();
            if value["id"] == id {
                assert!(value.get("error").is_none(), "{value}");
                return value["result"].clone();
            }
        }
    })
    .await
    .unwrap()
}
#[tokio::test]
async fn actual_cli_and_mcp_return_files_consumable_by_the_next_process() {
    let (origin, server) = server().await;
    let root = root();
    let core = Runtime::open(&root, None, None, false, true, false).unwrap();
    let mut value = serde_json::to_value(definition(&origin)).unwrap();
    value["operations"].as_array_mut().unwrap().push(json!({"name":"upload","method":"POST","path":"/upload","effect":"write","evidence":"Owned binary roundtrip fixture","body":{"multipart":{"file":{"type":"file","required":true}}},"response":{"format":"json"}}));
    let sp = hycli::spec::parse(value.to_string().as_bytes()).unwrap();
    core.install(&sp).unwrap();
    core.set_writes(&sp.site.name, true).unwrap();
    drop(core);
    let output = tokio::time::timeout(
        Duration::from_secs(45),
        command(&root, &["--allow-local", "files-fixture", "download"]).output(),
    )
    .await
    .unwrap()
    .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let cli = assert_artifact(serde_json::from_slice(&output.stdout).unwrap());
    let mut process = command(
        &root,
        &[
            "--allow-local",
            "mcp",
            "--sites-only",
            "--only-site",
            "files-fixture",
        ],
    )
    .stdin(Stdio::piped())
    .stdout(Stdio::piped())
    .stderr(Stdio::null())
    .spawn()
    .unwrap();
    let mut stdin = process.stdin.take().unwrap();
    let mut stdout = BufReader::new(process.stdout.take().unwrap()).lines();
    rpc(&mut stdin,&mut stdout,1,"initialize",json!({"protocolVersion":"2025-03-26","capabilities":{},"clientInfo":{"name":"file-fixture","version":"1"}})).await;
    stdin
        .write_all(b"{\"jsonrpc\":\"2.0\",\"method\":\"notifications/initialized\"}\n")
        .await
        .unwrap();
    let result = rpc(
        &mut stdin,
        &mut stdout,
        2,
        "tools/call",
        json!({"name":"files-fixture_download","arguments":{}}),
    )
    .await;
    assert_ne!(result["isError"], true, "{result}");
    let envelope: Value =
        serde_json::from_str(result["content"][0]["text"].as_str().unwrap()).unwrap();
    let mcp = assert_artifact(envelope["artifact"].clone());
    assert_ne!(cli.artifact_id, mcp.artifact_id);
    let observed = Runtime::open(&root, None, None, false, true, false).unwrap();
    for artifact in [&cli, &mcp] {
        assert!(
            observed
                .state
                .read()
                .unwrap()
                .jobs
                .iter()
                .any(|job| job.status == "completed" && job.artifact.as_ref() == Some(artifact)),
            "CLI and MCP completion preserve the authoritative dashboard artifact marker"
        );
    }
    drop(observed);
    assert_eq!(
        std::fs::read(&cli.local_path).unwrap(),
        std::fs::read(&mcp.local_path).unwrap()
    );
    // Pass the returned host-local artifact path explicitly to the existing upload action.
    let uploaded = rpc(
        &mut stdin,
        &mut stdout,
        3,
        "tools/call",
        json!({"name":"files-fixture_upload","arguments":{"file":cli.local_path}}),
    )
    .await;
    assert_ne!(uploaded["isError"], true, "{uploaded}");
    assert!(uploaded.to_string().contains(PAYLOAD_SHA256), "{uploaded}");
    process.kill().await.unwrap();
    process.wait().await.unwrap();
    server.abort();
}

#[test]
fn old_job_schema_keeps_server_json_untrusted_and_unchanged() {
    let root = root();
    std::fs::create_dir_all(&root).unwrap();
    let path = root.join("state.json");
    let store = hycli::state::StateStore::open(&path).unwrap();
    let fake = json!({"kind":"file","version":1,"status":"complete","artifact_id":"0123456789abcdef0123456789abcdef","local_path":"/not-an-authorized-file","location_scope":"hycli_host","filename":"report.pdf","media_type":"application/pdf","byte_count":1,"sha256":"0".repeat(64),"created_at":"2026-01-01T00:00:00Z","provenance":{"site":"fixture","action":"read","source_origin":"https://example.test"},"content_trust":"untrusted"});
    let mut job = hycli::state::Job::new("action", "en");
    job.status = "completed".into();
    job.result = Some(fake.clone());
    let id = job.id.clone();
    store.add_job(job).unwrap();
    let db = store.db_path().to_owned();
    drop(store);
    rusqlite::Connection::open(&db)
        .unwrap()
        .execute("ALTER TABLE jobs DROP COLUMN artifact_json", [])
        .unwrap();
    let migrated = hycli::state::StateStore::open(&path).unwrap();
    let job = migrated.job(&id).unwrap();
    assert_eq!(job.result, Some(fake));
    assert!(job.artifact.is_none());
}

#[tokio::test]
async fn known_and_unknown_length_downloads_cross_both_old_ceilings() {
    use futures_util::StreamExt;
    use sha2::{Digest, Sha256};
    let (origin, server) = server().await;
    for path in ["/large", "/chunked-large"] {
        let root = root();
        let core = Runtime::open(&root, None, None, false, true, false).unwrap();
        let mut sp = definition(&origin);
        sp.operations[0].path = path.into();
        let (result, _) = core
            .run_op(&sp, "download", &BTreeMap::new())
            .await
            .unwrap();
        let file = result.artifact.unwrap();
        assert_eq!(file.byte_count, 1601 * TRANSFER_CHUNK_BYTES as u64);
        let opened = ArtifactStore::open(&root)
            .unwrap()
            .open_completed(&file.artifact_id)
            .unwrap();
        let stream = opened.into_stream();
        futures_util::pin_mut!(stream);
        let mut count = 0;
        let mut hash = Sha256::new();
        while let Some(part) = stream.next().await {
            let part = part.unwrap();
            assert!(part.len() <= TRANSFER_CHUNK_BYTES);
            count += part.len() as u64;
            hash.update(part);
        }
        assert_eq!(count, file.byte_count);
        assert_eq!(
            hash.finalize()
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<String>(),
            file.sha256
        );
        drop(core);
        std::fs::remove_dir_all(root).unwrap();
    }
    server.abort();
}

#[tokio::test]
async fn large_html_attachments_are_uninspected_not_falsely_scanned() {
    use futures_util::StreamExt;
    let app = Router::new()
        .route(
            "/file",
            get(|| async {
                let chunks = futures_util::stream::iter(
                    (0..145).map(|_| Ok::<_, std::io::Error>(vec![b' '; TRANSFER_CHUNK_BYTES])),
                )
                .chain(futures_util::stream::once(async {
                    Ok::<_, std::io::Error>(b"<form><input type=password></form>".to_vec())
                }));
                Response::builder()
                    .header("content-type", "text/html")
                    .header(
                        "content-disposition",
                        "attachment; filename=uninspected.html",
                    )
                    .body(Body::from_stream(chunks))
                    .unwrap()
            }),
        )
        .route(
            "/ordinary-login",
            get(|| async {
                Response::builder()
                    .header("content-type", "text/html")
                    .body(Body::from("<form><input type=password></form>"))
                    .unwrap()
            }),
        );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let root = root();
    let core = Runtime::open(&root, None, None, false, true, false).unwrap();
    let mut sp = definition(&origin);
    sp.operations[0]
        .response
        .as_mut()
        .unwrap()
        .file
        .as_mut()
        .unwrap()
        .media_types = vec!["text/html".into()];
    let (result, _) = core
        .run_op(&sp, "download", &BTreeMap::new())
        .await
        .unwrap();
    let file = result.artifact.unwrap();
    assert!(file.byte_count > 8 * 1024 * 1024);
    assert_eq!(file.content_trust, "untrusted");
    sp.operations[0].path = "/ordinary-login".into();
    assert!(
        core.run_op(&sp, "download", &BTreeMap::new())
            .await
            .is_err()
    );
    server.abort();
}

#[tokio::test]
async fn unsolicited_partial_responses_never_become_completed_files() {
    for (status, range) in [(206, false), (200, true)] {
        let app = Router::new().route(
            "/file",
            get(move || async move {
                let mut response = Response::builder()
                    .status(status)
                    .header("content-type", "application/pdf");
                if range {
                    response = response.header("content-range", "bytes 0-2/9");
                }
                response.body(Body::from("one")).unwrap()
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let origin = format!("http://{}", listener.local_addr().unwrap());
        let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let root = root();
        let core = Runtime::open(&root, None, None, false, true, false).unwrap();
        assert!(
            core.run_op(&definition(&origin), "download", &BTreeMap::new())
                .await
                .is_err()
        );
        assert_no_artifacts(&root);
        server.abort();
    }
}

#[tokio::test]
async fn completion_transaction_failure_cleans_published_file_and_never_lists_it() {
    let (origin, server) = server().await;
    let root = root();
    let core = Runtime::open(&root, None, None, false, true, false).unwrap();
    let sp = definition(&origin);
    core.install(&sp).unwrap();
    rusqlite::Connection::open(core.state.db_path())
        .unwrap()
        .execute_batch(
            "CREATE TRIGGER reject_file_completion BEFORE UPDATE ON jobs
         WHEN NEW.kind = 'action' AND NEW.status = 'completed'
         BEGIN SELECT RAISE(ABORT, 'synthetic completion failure'); END;",
        )
        .unwrap();
    assert!(
        core.run_op(&sp, "download", &BTreeMap::new())
            .await
            .is_err()
    );
    assert_no_artifacts(&root);
    for job in core.state.read().unwrap().jobs {
        assert_ne!(job.status, "completed");
        assert!(job.artifact.is_none());
    }
    server.abort();
}
