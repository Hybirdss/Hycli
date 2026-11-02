//! Actual CLI one-off read contract against a synthetic local service only.
use axum::{
    Json, Router,
    extract::Query,
    http::{HeaderMap, StatusCode},
    routing::{get, post},
};
use hycli::{runtime::Runtime, util};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};
async fn command(root: &Path, args: &[&str]) -> std::process::Output {
    tokio::process::Command::new(
        std::env::var_os("HYCLI_TEST_EXECUTABLE")
            .unwrap_or_else(|| env!("CARGO_BIN_EXE_hycli").into()),
    )
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
fn ok(output: std::process::Output) -> Value {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}
#[tokio::test]
async fn account_read_cli_verifies_observes_reads_and_continues_without_sitespec() {
    let calls = Arc::new(AtomicUsize::new(0));
    let count = calls.clone();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let manifest = json!({"format":"apollo-persisted-query-manifest","version":1,"operations":[{"id":util::hash(b"query Items { items { id } }"),"body":"query Items { items { id } }","name":"Items","type":"query"}]});
    let served_manifest = manifest.clone();
    let app=Router::new()
        .route("/", get(|| async { axum::response::Html("<html><head><link rel='preload' as='fetch' type='application/json' href='/manifest.json'></head></html>") }))
        .route("/identity",get(|headers:HeaderMap|async move {if headers.get("cookie").and_then(|v|v.to_str().ok())==Some("sid=synthetic-read-secret"){(StatusCode::OK,Json(json!({"id":"person-one"})))}else{(StatusCode::UNAUTHORIZED,Json(json!({"error":"no session"})))}}))
        .route("/items",get(move |Query(query):Query<BTreeMap<String,String>>|{let count=count.clone();async move {count.fetch_add(1,Ordering::SeqCst);Json(json!({"items":["one"],"echo":"synthetic-read-secret","next":if query.contains_key("pageToken"){Value::Null}else{json!("cli-page-two-cursor")},"cursor_echo":query.get("pageToken")}))}}))
        .route("/graphql", post(|Json(body): Json<Value>| async move { Json(json!({"data":{"items":["query-result"]},"wire":body})) }))
        .route("/manifest.json", get(move || { let manifest = served_manifest.clone(); async move { Json(manifest) } }));
    let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join(".cache/account-read-cli")
        .join(util::id());
    let core = Runtime::open(&root, None, None, false, true, false).unwrap();
    let cookies = hycli::accounts::parse_cookies(
        &json!([{"name":"sid","value":"synthetic-read-secret","domain":"127.0.0.1","path":"/"}])
            .to_string(),
        &origin,
    )
    .unwrap();
    let account = core
        .connect_account("", &origin, cookies, "Fixture", "Synthetic", "")
        .unwrap();
    let identity = format!("{origin}/identity");
    ok(command(
        &root,
        &[
            "--allow-local",
            "accounts",
            "observe",
            &account.id,
            &identity,
        ],
    )
    .await);
    let recipe = root.join("recipe.json");
    std::fs::write(&recipe,json!({"identity_url":identity,"identity":{"user_id":"/id"},"evidence":"observed_browser_request"}).to_string()).unwrap();
    ok(command(
        &root,
        &[
            "--allow-local",
            "accounts",
            "verify",
            &account.id,
            "--recipe",
            recipe.to_str().unwrap(),
        ],
    )
    .await);
    let url = format!("{origin}/items");
    let observed = ok(command(
        &root,
        &["--allow-local", "accounts", "observe", &account.id, &url],
    )
    .await);
    let path = root.join("read.json");
    let mut request = json!({"version":1,"request":{"method":"GET","url":url},"evidence":{"observation":observed["read_evidence"]},"pagination":{"query":"pageToken","next":"/next"}});
    std::fs::write(&path, request.to_string()).unwrap();
    let first = ok(command(
        &root,
        &[
            "--allow-local",
            "accounts",
            "read",
            &account.id,
            "--request",
            path.to_str().unwrap(),
        ],
    )
    .await);
    assert_eq!(first["body"]["items"], json!(["one"]));
    assert_eq!(first["body"]["echo"], "[private]");
    assert!(!first.to_string().contains("cli-page-two-cursor"));
    let second = ok(command(
        &root,
        &[
            "--allow-local",
            "accounts",
            "read",
            &account.id,
            "--request",
            path.to_str().unwrap(),
            "--cursor",
            first["next_cursor"].as_str().unwrap(),
        ],
    )
    .await);
    assert!(second["next_cursor"].is_null());
    assert_eq!(second["body"]["cursor_echo"], "[private]");
    // Exercise both full-document and truly documentless query dispatch through the CLI.
    for (label, envelope, persisted) in [
        (
            "full-query",
            json!({"query":"query Items { items { id } }","operationName":"Items"}),
            false,
        ),
        (
            "persisted-query",
            json!({"extensions":{"persistedQuery":{"version":1,"sha256Hash":util::hash(b"query Items { items { id } }")}},"operationName":"Items"}),
            true,
        ),
    ] {
        let endpoint = format!("{origin}/graphql");
        let har = json!({"log":{"entries":[
            {"request":{"method":"POST","url":endpoint,"postData":{"mimeType":"application/json","text":envelope.to_string()}},"response":{"status":200,"content":{"mimeType":"application/json","text":"{}"}}},
            {"request":{"method":"GET","url":format!("{origin}/manifest.json")},"response":{"status":200,"content":{"mimeType":"application/json","text":manifest.to_string()}}}
        ]}});
        let capture = root.join(format!("{label}.har"));
        std::fs::write(&capture, har.to_string()).unwrap();
        let mut descriptor = json!({"version":1,"request":{"method":"POST","url":endpoint,"json":envelope},"evidence":{"har":{"path":format!("{label}.har"),"entry":0}}});
        if persisted {
            descriptor["persisted"] = json!({"format":"apollo","evidence":{"har":{"path":format!("{label}.har"),"entry":1}}});
        }
        let request_file = root.join(format!("{label}.json"));
        std::fs::write(&request_file, descriptor.to_string()).unwrap();
        let result = ok(command(
            &root,
            &[
                "--allow-local",
                "accounts",
                "read",
                &account.id,
                "--request",
                request_file.to_str().unwrap(),
            ],
        )
        .await);
        assert_eq!(result["body"]["data"]["items"], json!(["query-result"]));
        assert_eq!(result["body"]["wire"].get("query").is_none(), persisted);
    }
    let before = calls.load(Ordering::SeqCst);
    request["request"]["effect"] = json!("read");
    std::fs::write(&path, request.to_string()).unwrap();
    let denied = command(
        &root,
        &[
            "--allow-local",
            "accounts",
            "read",
            &account.id,
            "--request",
            path.to_str().unwrap(),
        ],
    )
    .await;
    assert!(!denied.status.success());
    assert_eq!(calls.load(Ordering::SeqCst), before);
    assert!(!String::from_utf8_lossy(&denied.stderr).contains("synthetic-read-secret"));
    assert!(core.spec(&account.site_id).is_err());
    server.abort();
    drop(core);
    std::fs::remove_dir_all(root).unwrap();
}
