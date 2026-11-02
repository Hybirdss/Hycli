use axum::{
    Json, Router,
    http::HeaderMap,
    routing::{get, post},
};
use hycli::util;
use serde_json::{Value, json};
use std::{
    path::Path,
    process::Stdio,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

fn command(root: &Path, args: &[&str]) -> tokio::process::Command {
    let mut command = tokio::process::Command::new(
        std::env::var_os("HYCLI_TEST_EXECUTABLE")
            .unwrap_or_else(|| env!("CARGO_BIN_EXE_hycli").into()),
    );
    command
        .kill_on_drop(true)
        .args(args)
        .env("HYCLI_DATA_DIR", root)
        .env("HYCLI_SPECS", root.join("specs"))
        .env("HYCLI_KB", root.join("kb.db"))
        .current_dir(env!("CARGO_MANIFEST_DIR"));
    command
}
struct CommandOutput {
    output: std::process::Output,
    label: String,
}
impl std::ops::Deref for CommandOutput {
    type Target = std::process::Output;
    fn deref(&self) -> &Self::Target {
        &self.output
    }
}
async fn input(root: &Path, args: &[&str], value: &str) -> CommandOutput {
    // Only fixture command/operation names, never stdin, --value, or its value.
    let label = args
        .iter()
        .filter(|arg| **arg != "--allow-local")
        .take(2)
        .copied()
        .collect::<Vec<_>>()
        .join(" ");
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
    let output = tokio::time::timeout(Duration::from_secs(45), child.wait_with_output())
        .await
        .unwrap()
        .unwrap();
    CommandOutput { output, label }
}
fn successful(output: CommandOutput) -> Value {
    assert!(
        output.status.success(),
        "command={} stdout={} stderr={}",
        output.label,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_no_secrets(&String::from_utf8_lossy(&output.stdout));
    assert_no_secrets(&String::from_utf8_lossy(&output.stderr));
    serde_json::from_slice(&output.stdout).unwrap()
}
fn assert_no_secrets(text: &str) {
    for value in [
        "cli-access-canary",
        "cli-rotated-canary",
        "cli-refresh-canary",
        "cli-new-refresh-canary",
        "cli-secret-canary",
    ] {
        assert!(!text.contains(value), "credential escaped output");
    }
}
fn grant() -> String {
    json!({"kind":"oauth_refresh","client_id":"cli-client-canary","client_secret":"cli-secret-canary","access_token":"cli-access-canary","refresh_token":"cli-refresh-canary","expires_at":chrono::Utc::now().timestamp()-60,"subject":"cli-subject-canary"}).to_string()
}
async fn install(root: &Path, spec: &hycli::spec::Spec) {
    std::fs::create_dir_all(root).unwrap();
    let path = root.join("definition.json");
    std::fs::write(&path, serde_json::to_vec(spec).unwrap()).unwrap();
    successful(
        input(
            root,
            &["--allow-local", "spec", "install", path.to_str().unwrap()],
            "",
        )
        .await,
    );
}
async fn configured(root: &Path, site: &str, auth: &str) -> bool {
    let site = successful(input(root, &["--allow-local", "describe", site], "").await);
    site["credentials"]
        .as_array()
        .unwrap()
        .iter()
        .find(|credential| credential["id"] == auth)
        .expect("credential exposed by describe")["configured"]
        .as_bool()
        .unwrap()
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
            assert_no_secrets(&line);
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
#[tokio::test]
async fn existing_auth_stdin_cli_and_mcp_use_the_same_rotating_grant() {
    let exchanges = Arc::new(AtomicUsize::new(0));
    let reads = Arc::new(AtomicUsize::new(0));
    let tokens = exchanges.clone();
    let resources = reads.clone();
    let app = Router::new()
        .route("/token", post(move |body: String| { let tokens=tokens.clone(); async move {
            tokens.fetch_add(1, Ordering::SeqCst);
            let fields = url::form_urlencoded::parse(body.as_bytes()).into_owned().collect::<Vec<_>>();
            assert_eq!(fields, vec![
                ("grant_type".into(), "refresh_token".into()),
                ("refresh_token".into(), "cli-refresh-canary".into()),
                ("client_id".into(), "cli-client-canary".into()),
                ("client_secret".into(), "cli-secret-canary".into()),
            ]);
            Json(json!({"access_token":"cli-rotated-canary","refresh_token":"cli-new-refresh-canary","expires_in":3600,"token_type":"Bearer","scope":"read"}))
        }}))
        .route("/identity", get(|headers: HeaderMap| async move { assert_eq!(headers["authorization"], "Bearer cli-rotated-canary"); Json(json!({"sub":"cli-subject-canary"})) }))
        .route("/items", get(move |headers: HeaderMap| { let resources=resources.clone(); async move {
            resources.fetch_add(1, Ordering::SeqCst);
            assert_eq!(headers["authorization"], "Bearer cli-rotated-canary");
            Json(json!({"items":["verified"],"key_echo":[{"cli-rotated-canary":"access key entry","cli-new-refresh-canary":"refresh key entry","cli-secret-canary":"secret key entry","[private]":"ordinary key entry","[private]#":"ordinary suffixed entry"}],"echo":"cli-rotated-canary cli-new-refresh-canary cli-client-canary cli-secret-canary cli-subject-canary"}))
        }}));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let server = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join(".cache/oauth-cli")
        .join(util::id());
    std::fs::create_dir_all(&root).unwrap();
    let definition = json!({"spec_version":3,"site":{"name":"oauth-cli","base_url":origin},"auth":[{"name":"api","kind":"header","header":"authorization","prefix":"Bearer ","origin":origin,"oauth_refresh":{"grant_ref":"one","token_endpoint":format!("{origin}/token"),"identity_endpoint":format!("{origin}/identity"),"subject_pointer":"/sub","resource":"fixture","scopes":["read"]}}],"operations":[{"name":"read-items","method":"GET","path":"/items","auth":"api","effect":"read","evidence":"Synthetic owned OAuth endpoint"}]});
    let path = root.join("definition.json");
    std::fs::write(&path, definition.to_string()).unwrap();
    successful(
        input(
            &root,
            &["--allow-local", "spec", "install", path.to_str().unwrap()],
            "",
        )
        .await,
    );
    let invalid = input(
        &root,
        &["auth", "set", "oauth-cli/api"],
        "{\"kind\":\"oauth_refresh\"}",
    )
    .await;
    assert!(!invalid.status.success());
    assert!(!root.join("store.json").exists());
    successful(input(&root, &["auth", "set", "oauth-cli/api"], &grant()).await);
    let argv = input(
        &root,
        &["auth", "set", "oauth-cli/api", "--value", &grant()],
        "",
    )
    .await;
    assert!(!argv.status.success());
    assert_no_secrets(&String::from_utf8_lossy(&argv.stderr));
    let result = successful(input(&root, &["--allow-local", "oauth-cli", "read-items"], "").await);
    assert!(result.to_string().contains("verified"));
    for retained in [
        "access key entry",
        "refresh key entry",
        "secret key entry",
        "ordinary key entry",
        "ordinary suffixed entry",
        "cli-client-canary",
        "cli-subject-canary",
    ] {
        assert!(
            result.to_string().contains(retained),
            "lost result field: {retained}"
        );
    }
    assert_eq!(exchanges.load(Ordering::SeqCst), 1);
    successful(input(&root, &["--allow-local", "oauth-cli", "read-items"], "").await);
    assert_eq!(exchanges.load(Ordering::SeqCst), 1);
    assert!(configured(&root, "oauth-cli", "api").await);
    assert_eq!(
        successful(input(&root, &["writes", "oauth-cli"], "").await)["writes"],
        false
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(root.join("store.json"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
    }
    successful(input(&root, &["auth", "set", "oauth-cli/api"], &grant()).await);
    let mut mcp = command(
        &root,
        &[
            "--allow-local",
            "mcp",
            "--sites-only",
            "--only-site",
            "oauth-cli",
        ],
    )
    .stdin(Stdio::piped())
    .stdout(Stdio::piped())
    .stderr(Stdio::null())
    .spawn()
    .unwrap();
    let mut stdin = mcp.stdin.take().unwrap();
    let mut stdout = BufReader::new(mcp.stdout.take().unwrap()).lines();
    rpc(&mut stdin, &mut stdout, 1, "initialize", json!({"protocolVersion":"2025-03-26","capabilities":{},"clientInfo":{"name":"oauth-fixture","version":"1"}})).await;
    stdin
        .write_all(b"{\"jsonrpc\":\"2.0\",\"method\":\"notifications/initialized\"}\n")
        .await
        .unwrap();
    let result = rpc(
        &mut stdin,
        &mut stdout,
        2,
        "tools/call",
        json!({"name":"oauth-cli_read-items","arguments":{}}),
    )
    .await;
    assert_ne!(result["isError"], true, "{result}");
    assert!(result.to_string().contains("verified"));
    for retained in [
        "access key entry",
        "refresh key entry",
        "secret key entry",
        "ordinary key entry",
        "ordinary suffixed entry",
        "cli-client-canary",
        "cli-subject-canary",
    ] {
        assert!(
            result.to_string().contains(retained),
            "lost result field: {retained}"
        );
    }
    assert_eq!(exchanges.load(Ordering::SeqCst), 2);
    assert_eq!(reads.load(Ordering::SeqCst), 3);
    mcp.kill().await.unwrap();
    mcp.wait().await.unwrap();
    successful(input(&root, &["auth", "rm", "oauth-cli/api"], "").await);
    let missing = input(&root, &["--allow-local", "oauth-cli", "read-items"], "").await;
    assert!(!missing.status.success());
    assert_eq!(reads.load(Ordering::SeqCst), 3);
    server.abort();
    std::fs::remove_dir_all(root).unwrap();
}

#[tokio::test]
async fn legacy_auth_fallback_survives_unrelated_runtime_failure() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join(".cache/oauth-cli")
        .join(util::id());
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("dashboard.json"), "invalid synthetic state").unwrap();
    assert!(!input(&root, &["describe"], "").await.status.success());
    successful(input(&root, &["auth", "set", "legacy/api"], "legacy-test-value").await);
    assert_eq!(
        successful(input(&root, &["auth", "ls"], "").await),
        json!(["legacy/api"])
    );
    let rejected = input(&root, &["auth", "set", "legacy/api"], &grant()).await;
    assert!(!rejected.status.success());
    assert_no_secrets(&String::from_utf8_lossy(&rejected.stderr));
    successful(input(&root, &["auth", "rm", "legacy/api"], "").await);
    assert_eq!(
        successful(input(&root, &["auth", "ls"], "").await),
        json!([])
    );
    std::fs::remove_dir_all(root).unwrap();
}

#[tokio::test]
async fn orphaned_grants_stay_listed_and_removable_without_affecting_siblings() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join(".cache/oauth-cli")
        .join(util::id());
    let definition = |site: &str, auth: &str, grant_ref: &str| {
        hycli::spec::parse(json!({"spec_version":3,"site":{"name":site,"base_url":"https://example.com"},"auth":[{"name":auth,"kind":"header","header":"authorization","prefix":"Bearer ","origin":"https://example.com","oauth_refresh":{"grant_ref":grant_ref,"token_endpoint":"https://example.com/token","identity_endpoint":"https://example.com/identity","subject_pointer":"/sub","resource":"fixture","scopes":["read"]}}],"operations":[{"name":"read-items","method":"GET","path":"/items","auth":auth,"effect":"read","evidence":"Synthetic OAuth definition; no network use"}]}).to_string().as_bytes()).unwrap()
    };
    let first = definition("orphan", "api", "one");
    let sibling = definition("sibling", "api", "one");
    for sp in [&first, &sibling] {
        install(&root, sp).await;
        successful(
            input(
                &root,
                &["auth", "set", &format!("{}/api", sp.site.name)],
                &grant(),
            )
            .await,
        );
    }
    // Removing the definition preserves credentials, but must not strand them.
    successful(input(&root, &["spec", "rm", "orphan"], "").await);
    let listed = successful(input(&root, &["auth", "ls"], "").await);
    assert_eq!(listed, json!(["orphan/api", "sibling/api"]));
    successful(input(&root, &["auth", "rm", "orphan/api"], "").await);
    assert_eq!(
        successful(input(&root, &["auth", "ls"], "").await),
        json!(["sibling/api"])
    );
    assert!(configured(&root, "sibling", "api").await);
    install(&root, &first).await;
    assert!(!configured(&root, "orphan", "api").await);

    // Same public reference, new grant key: removal must delete both generations.
    successful(input(&root, &["auth", "set", "orphan/api"], &grant()).await);
    let rebound = definition("orphan", "api", "two");
    install(&root, &rebound).await;
    successful(input(&root, &["auth", "set", "orphan/api"], &grant()).await);
    assert_eq!(
        successful(input(&root, &["auth", "ls"], "").await),
        json!(["orphan/api", "sibling/api"])
    );
    successful(input(&root, &["auth", "rm", "orphan/api"], "").await);
    for sp in [&first, &rebound] {
        install(&root, sp).await;
        assert!(!configured(&root, "orphan", "api").await);
    }

    // An auth rename keeps the old reference removable without deleting the new one.
    install(&root, &first).await;
    successful(input(&root, &["auth", "set", "orphan/api"], &grant()).await);
    let renamed = definition("orphan", "other", "two");
    install(&root, &renamed).await;
    successful(input(&root, &["auth", "set", "orphan/other"], &grant()).await);
    successful(input(&root, &["auth", "rm", "orphan/api"], "").await);
    assert_eq!(
        successful(input(&root, &["auth", "ls"], "").await),
        json!(["orphan/other", "sibling/api"])
    );
    assert!(configured(&root, "orphan", "other").await);
    // A reused private grant_ref must not let one public auth name delete another.
    install(&root, &rebound).await;
    successful(input(&root, &["auth", "rm", "orphan/api"], "").await);
    install(&root, &renamed).await;
    assert!(configured(&root, "orphan", "other").await);

    // A replacement legacy strategy may use a different private store key.
    let mut legacy = renamed.clone();
    legacy.auth[0].oauth_refresh = None;
    legacy.auth[0].value_from = "store:orphan/static-key".into();
    install(&root, &legacy).await;
    successful(input(&root, &["auth", "set", "orphan/other"], "legacy-value").await);
    successful(input(&root, &["auth", "rm", "orphan/other"], "").await);
    assert_eq!(
        successful(input(&root, &["auth", "ls"], "").await),
        json!(["sibling/api"])
    );
    install(&root, &renamed).await;
    assert!(!configured(&root, "orphan", "other").await);
    // Removal remains usable when unrelated runtime state cannot be opened.
    std::fs::write(root.join("dashboard.json"), "invalid synthetic state").unwrap();
    std::fs::remove_file(root.join("specs/sibling.yaml")).unwrap();
    successful(input(&root, &["auth", "rm", "sibling/api"], "").await);
    assert_eq!(
        successful(input(&root, &["auth", "ls"], "").await),
        json!([])
    );
    std::fs::remove_dir_all(root).unwrap();
}
