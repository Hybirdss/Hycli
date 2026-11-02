use axum::{
    Json, Router,
    body::Bytes,
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    routing::{get, post},
};
use hycli::util;
use serde_json::{Value, json};
use std::{
    path::Path,
    process::Stdio,
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

const FILE_BYTES: &[u8] = b"explicit-cli-upload-canary";
const FILE_SHA256: &str = "cb554815d03cf7051b490609cfd190f3cb5a2a6776cea8467e7ca8cab6c301c6";

fn command(root: &Path, args: &[&str]) -> tokio::process::Command {
    let mut command = tokio::process::Command::new(
        std::env::var_os("HYCLI_TEST_EXECUTABLE")
            .unwrap_or_else(|| env!("CARGO_BIN_EXE_hycli").into()),
    );
    command
        .args(args)
        .env("HYCLI_DATA_DIR", root)
        .env("HYCLI_TEST_DATA", root)
        .env("HYCLI_SPECS", root.join("specs"))
        .env("HYCLI_KB", root.join("kb.db"))
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .kill_on_drop(true);
    command
}
async fn input(root: &Path, args: &[&str], value: &str) -> std::process::Output {
    let mut child = command(root, args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(value.as_bytes())
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(45), child.wait_with_output())
        .await
        .unwrap()
        .unwrap()
}
fn successful(output: std::process::Output) -> Value {
    assert!(
        output.status.success(),
        "stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    for bytes in [&output.stdout, &output.stderr] {
        assert!(!String::from_utf8_lossy(bytes).contains("source-cli-api-canary"));
    }
    serde_json::from_slice(&output.stdout).unwrap()
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
            let line = stdout
                .next_line()
                .await
                .unwrap()
                .expect("MCP closed before response");
            assert!(!line.contains("source-cli-api-canary"));
            let response: Value = serde_json::from_str(&line).unwrap();
            if response["id"] == id {
                assert!(response.get("error").is_none(), "{response}");
                return response["result"].clone();
            }
        }
    })
    .await
    .unwrap()
}
async fn host(router: Router) -> (String, tokio::task::JoinHandle<()>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    (
        origin,
        tokio::spawn(async move { axum::serve(listener, router).await.unwrap() }),
    )
}

#[tokio::test]
async fn cli_and_mcp_upload_explicit_bytes_without_forwarding_source_credentials_or_remote_paths() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join(".cache/upload-cli")
        .join(util::id());
    std::fs::create_dir_all(&root).unwrap();
    let chosen = root.join("chosen.bin");
    std::fs::write(&chosen, FILE_BYTES).unwrap();
    let private = root.join("not-chosen.bin");
    std::fs::write(&private, b"private-local-canary-never-upload").unwrap();
    let received = Arc::new(Mutex::new(Vec::<(HeaderMap, Option<Vec<u8>>)>::new()));
    let sink = received.clone();
    let (target, target_server) = host(Router::new().route(
        "/put",
        post(move |headers: HeaderMap, body: Bytes| {
            let sink = sink.clone();
            async move {
                let text = std::str::from_utf8(&body).unwrap();
                assert!(!text.contains("private-local-canary-never-upload"));
                let content_type = headers["content-type"].to_str().unwrap();
                let boundary = content_type
                    .split("boundary=")
                    .nth(1)
                    .expect("multipart boundary");
                let file = text.split(&format!("--{boundary}")).find_map(|part| {
                    let (metadata, bytes) = part.split_once("\r\n\r\n")?;
                    if !metadata.contains("name=\"file\"") || !metadata.contains("filename=") {
                        return None;
                    }
                    Some(
                        bytes
                            .strip_suffix("\r\n")
                            .unwrap_or(bytes)
                            .as_bytes()
                            .to_vec(),
                    )
                });
                let digest = file.as_ref().map(|bytes| util::hash(bytes));
                let size = file.as_ref().map(|bytes| bytes.len());
                sink.lock().unwrap().push((headers, file));
                Json(json!({"ok":true,"file_sha256":digest,"file_size":size}))
            }
        }),
    ))
    .await;
    let leases = Arc::new(AtomicUsize::new(0));
    let source_hits = leases.clone();
    let injected_path = private.to_string_lossy().into_owned();
    let (source, source_server) = host(Router::new().route(
        "/lease",
        get(move |headers: HeaderMap| {
            let hits = source_hits.clone();
            let target = target.clone();
            let path = injected_path.clone();
            async move {
                hits.fetch_add(1, Ordering::SeqCst);
                if headers
                    .get("x-custom-api-key")
                    .is_none_or(|value| value != "source-cli-api-canary")
                {
                    return (
                        StatusCode::UNAUTHORIZED,
                        Json(json!({"error":"missing source credential"})),
                    )
                        .into_response();
                }
                Json(json!({"base":target,"signature":"target-cli-signature","file":path}))
                    .into_response()
            }
        }),
    ))
    .await;
    let definition = json!({"spec_version":3,"site":{"name":"upload-cli","base_url":source},"auth":[{"name":"api","kind":"header","header":"X-Custom-Api-Key","value_from":"store:upload-cli/api","origin":source}],"operations":[{
        "name":"upload","method":"POST","path":"/put","base_url":"{{step.lease.base}}","derived_target":true,"auth":"api","effect":"write","evidence":"Synthetic owned upload endpoint",
        "preflight":[{"name":"lease","path":"/lease","extract":{"base":"json:/base","signature":"json:/signature","file":"json:/file"}}],
        "headers":{"X-Signed-Target":"{{step.lease.signature}}"},
        "body":{"multipart":{"file":{"type":"file","required":false},"caption":{"type":"string","default":"synthetic-caption"}}},"response":{"format":"json","required_pointers":["/ok"]}
    }]});
    let definition_path = root.join("definition.json");
    std::fs::write(&definition_path, definition.to_string()).unwrap();
    successful(
        input(
            &root,
            &[
                "--allow-local",
                "spec",
                "install",
                definition_path.to_str().unwrap(),
            ],
            "",
        )
        .await,
    );
    successful(
        input(
            &root,
            &["auth", "set", "upload-cli/api"],
            "source-cli-api-canary",
        )
        .await,
    );
    let file_arg = format!("file={}", chosen.display());
    let rejected = input(
        &root,
        &[
            "--allow-local",
            "run",
            "upload-cli",
            "upload",
            "--arg",
            &file_arg,
        ],
        "",
    )
    .await;
    assert!(!rejected.status.success());
    assert!(String::from_utf8_lossy(&rejected.stderr).contains("writes_disabled"));
    assert_eq!(leases.load(Ordering::SeqCst), 0);
    assert!(received.lock().unwrap().is_empty());

    let mut mcp = command(
        &root,
        &[
            "--allow-local",
            "mcp",
            "--sites-only",
            "--only-site",
            "upload-cli",
        ],
    )
    .stdin(Stdio::piped())
    .stdout(Stdio::piped())
    .stderr(Stdio::null())
    .spawn()
    .unwrap();
    let mut stdin = mcp.stdin.take().unwrap();
    let mut stdout = BufReader::new(mcp.stdout.take().unwrap()).lines();
    rpc(&mut stdin, &mut stdout, 1, "initialize", json!({"protocolVersion":"2025-03-26","capabilities":{},"clientInfo":{"name":"upload-fixture","version":"1"}})).await;
    stdin
        .write_all(b"{\"jsonrpc\":\"2.0\",\"method\":\"notifications/initialized\"}\n")
        .await
        .unwrap();
    let rejected = rpc(
        &mut stdin,
        &mut stdout,
        2,
        "tools/call",
        json!({"name":"upload-cli_upload","arguments":{"file":chosen.to_str().unwrap()}}),
    )
    .await;
    assert_eq!(rejected["isError"], true, "{rejected}");
    assert!(rejected.to_string().contains("writes_disabled"));
    assert_eq!(leases.load(Ordering::SeqCst), 0);
    assert!(received.lock().unwrap().is_empty());
    successful(input(&root, &["writes", "upload-cli", "allow"], "").await);

    let explicit = successful(
        input(
            &root,
            &[
                "--allow-local",
                "run",
                "upload-cli",
                "upload",
                "--arg",
                &file_arg,
            ],
            "",
        )
        .await,
    );
    assert!(explicit.to_string().contains(FILE_SHA256), "{explicit}");
    let omitted =
        successful(input(&root, &["--allow-local", "run", "upload-cli", "upload"], "").await);
    assert!(omitted.to_string().contains("file_sha256"), "{omitted}");
    let explicit_mcp = rpc(
        &mut stdin,
        &mut stdout,
        3,
        "tools/call",
        json!({"name":"upload-cli_upload","arguments":{"file":chosen.to_str().unwrap()}}),
    )
    .await;
    assert_ne!(explicit_mcp["isError"], true, "{explicit_mcp}");
    assert!(
        explicit_mcp.to_string().contains(FILE_SHA256),
        "{explicit_mcp}"
    );
    let omitted_mcp = rpc(
        &mut stdin,
        &mut stdout,
        4,
        "tools/call",
        json!({"name":"upload-cli_upload","arguments":{}}),
    )
    .await;
    assert_ne!(omitted_mcp["isError"], true, "{omitted_mcp}");
    assert_eq!(leases.load(Ordering::SeqCst), 4);
    {
        let received = received.lock().unwrap();
        assert_eq!(received.len(), 4);
        for (index, (headers, file)) in received.iter().enumerate() {
            assert!(!headers.contains_key("x-custom-api-key"));
            assert!(!headers.contains_key("authorization"));
            assert!(!headers.contains_key("cookie"));
            assert_eq!(headers["x-signed-target"], "target-cli-signature");
            assert_eq!(
                file.as_deref(),
                if index % 2 == 0 {
                    Some(FILE_BYTES)
                } else {
                    None
                }
            );
        }
    }
    mcp.kill().await.unwrap();
    mcp.wait().await.unwrap();
    source_server.abort();
    target_server.abort();
}
