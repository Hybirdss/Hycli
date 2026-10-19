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
