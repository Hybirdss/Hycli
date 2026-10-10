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
    tokio::process::Command::new(env!("CARGO_BIN_EXE_hycli"))
        .args(args)
        .env("HYCLI_DATA_DIR", root)
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
        {"name":"create-item","method":"POST","path":"/items","effect":"write","evidence":"Synthetic documentation"},
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
            );
        }
    }
    assert_eq!(
        reads.load(Ordering::SeqCst),
        0,
        "help must not send a website request"
    );
    let metadata = ok(cli(&root, &["describe", "sample", "search"]).await);
    assert!(
        metadata["inputs"]
            .as_array()
            .unwrap()
            .iter()
            .any(|i| i["id"] == "q" && i["required"] == true)
    );
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
    std::fs::write(root.join("specs/broken.yaml"), "site: [broken").unwrap();
    let listed = cli(&root, &["spec", "ls"]).await;
    assert!(String::from_utf8_lossy(&listed.stderr).contains("cannot load broken"));
    assert!(!cli(&root, &["broken", "--help"]).await.status.success());
    server.abort();
    drop(core);
    let _ = std::fs::remove_dir_all(&root);
}
