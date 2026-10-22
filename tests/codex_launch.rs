//! Native synthetic app-server. This test executable is not a public CLI target.
use hycli::ai::codex::Codex;
use serde_json::{Value, json};
use std::{
    io::{BufRead, Write},
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

fn main() {
    if std::env::args().nth(1).as_deref() == Some("app-server") {
        app_server();
        return;
    }
    let runtime = tokio::runtime::Runtime::new().unwrap();
    if std::env::args().nth(1).as_deref() == Some("automatic-probe") {
        let workspace = PathBuf::from(std::env::args_os().nth(2).unwrap());
        runtime.block_on(async {
            // A failed PATH lookup must never launch a real app-bundled provider.
            let adapter = Codex::with_binary(workspace.clone(), PathBuf::from("codex"));
            assert!(adapter.available());
            assert_eq!(adapter.refresh_account().await.unwrap(), None);
            assert_eq!(
                adapter
                    .complete("fixture", "synthetic", "automatic")
                    .await
                    .unwrap(),
                "automatic"
            );
            drop(adapter);
            assert_children_exit(&records(&workspace)).await;
        });
        return;
    }
    runtime.block_on(async {
        run().await;
        lifecycle().await;
    });
}

fn app_server() {
    let pid = std::process::id();
    let args: Vec<_> = std::env::args().skip(1).collect();
    std::fs::write(
        format!("child-{pid}.json"),
        serde_json::to_vec(&json!({"pid":pid,"args":args,"exe":std::env::current_exe().unwrap()}))
            .unwrap(),
    )
    .unwrap();
    let mut initialized = false;
    let mut login_sequence = 0;
    for line in std::io::stdin().lock().lines() {
        let request: Value = serde_json::from_str(&line.unwrap()).unwrap();
        let method = request["method"].as_str().unwrap();
        if method == "initialized" {
            initialized = true;
            continue;
        }
        if method == "initialize" && Path::new("hang-initialize").exists() {
            std::thread::sleep(Duration::from_secs(120));
            return;
        }
        assert!(method == "initialize" || initialized);
        let mut requests = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open("requests.jsonl")
            .unwrap();
        writeln!(requests, "{request}").unwrap();
        let marker = method.replace('/', "_");
        std::fs::write(format!("observed-{marker}"), b"").unwrap();
        let deadline = std::time::Instant::now() + Duration::from_secs(15);
        while Path::new(&format!("hold-{marker}")).exists() {
            assert!(
                std::time::Instant::now() < deadline,
                "Synthetic request was not released"
            );
            std::thread::sleep(Duration::from_millis(10));
        }
        let scenario: Value = std::fs::read("scenario.json")
            .ok()
            .map(|bytes| serde_json::from_slice(&bytes).unwrap())
            .unwrap_or(json!({}));
        if method == "account/read" {
            if let Ok(bytes) = std::fs::read("completion.json") {
                std::fs::remove_file("completion.json").unwrap();
                let params: Value = serde_json::from_slice(&bytes).unwrap();
                send(json!({"method":"account/login/completed","params":params}));
            }
            if scenario["account"] == "closed" {
                return;
            }
        }
        if (method == "account/read" && scenario["account"] == "fail")
            || (method == "model/list" && scenario["models"] == "fail")
        {
            send(
                json!({"id":request["id"],"error":{"code":-32000,"message":"private synthetic provider detail"}}),
            );
            continue;
        }
        let result = match method {
            "initialize" => json!({}),
            "account/read" => match scenario["account"].as_str() {
                Some("signed_in") => {
                    json!({"account":{"type":"chatgpt","email":"fixture@example.invalid"}})
                }
                Some("malformed") => json!({}),
                _ => json!({"account":null}),
            },
            "account/login/start" => {
                login_sequence += 1;
                let id = format!("synthetic-{pid}-{login_sequence}");
                if scenario["early_completion"] == true {
                    send(
                        json!({"method":"account/login/completed","params":{"loginId":id,"success":true}}),
                    );
                }
                json!({"type":"chatgpt","loginId":id,"authUrl":format!("https://auth.openai.com/authorize?fixture={id}")})
            }
            "account/login/cancel" => json!({"status":"canceled"}),
            "model/list" => json!({"data":[{"model":"fixture-model"}],"nextCursor":null}),
            "config/read" => json!({"config":{"mcp_servers":{}}}),
            "thread/start" => {
                assert_eq!(request["params"]["sandbox"], "read-only");
                assert_eq!(request["params"]["approvalPolicy"], "never");
                json!({"thread":{"id":"synthetic-thread"}})
            }
            "mcpServerStatus/list" => json!({"data":[],"nextCursor":null}),
            "turn/start" => json!({"turn":{"id":"synthetic-turn"}}),
            "thread/archive" => json!({}),
            _ => panic!("Unexpected synthetic method {method}"),
        };
        send(json!({"id":request["id"],"result":result}));
        if method == "turn/start" {
            send(
                json!({"method":"item/completed","params":{"threadId":"synthetic-thread","turnId":"synthetic-turn","item":{"type":"agentMessage","phase":"final_answer","text":request["params"]["input"][0]["text"]}}}),
            );
            send(
                json!({"method":"turn/completed","params":{"threadId":"synthetic-thread","turn":{"id":"synthetic-turn","status":"completed"}}}),
            );
        }
    }
}

fn send(value: Value) {
    println!("{value}");
    std::io::stdout().flush().unwrap();
}

struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let root = std::env::current_dir()
            .unwrap()
            .join(".cache")
            .join(format!("codex-launch-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        Self(root)
    }
    fn binary(&self) -> PathBuf {
        let binary = self.0.join(if cfg!(windows) {
            "provider 한 글 with spaces.exe"
        } else {
            "provider 한 글 with spaces"
        });
        std::fs::copy(std::env::current_exe().unwrap(), &binary).unwrap();
        binary
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

async fn run() {
    let fixture = Fixture::new();
    let binary = fixture.binary();
    let relative = binary
        .strip_prefix(std::env::current_dir().unwrap())
        .unwrap()
        .to_owned();
    let workspace = fixture.0.join("different workspace 한 글");
    let adapter = Codex::with_binary(workspace.clone(), relative);
    assert!(adapter.available());
    assert_eq!(adapter.cached_account(), None);
    let account = adapter.refresh_account().await;
    assert!(
        account.is_ok(),
        "relative executable must launch independently of provider workspace: {account:?}"
    );
    assert_eq!(
        account.unwrap(),
        None,
        "Executable discovery must not imply signed-in status"
    );
    let (first, second) = tokio::join!(
        adapter.complete("fixture", "synthetic", "first"),
        adapter.complete("fixture", "synthetic", "second"),
    );
    assert_eq!(first.unwrap(), "first");
    assert_eq!(second.unwrap(), "second");
    let before_reuse = records(&workspace).len();
    assert_eq!(
        adapter
            .complete("fixture", "synthetic", "pooled")
            .await
            .unwrap(),
        "pooled"
    );
    assert_eq!(records(&workspace).len(), before_reuse);
    let children = records(&workspace);
    assert!(
        children.len() >= 2,
        "Control and inference need separate children"
    );
    for child in &children {
        assert_selected_binary(child, &binary);
        let args = child["args"].as_array().unwrap();
        assert_eq!(args[0], "app-server");
        for value in [
            "approval_policy=\"never\"",
            "sandbox_mode=\"read-only\"",
            "web_search=\"disabled\"",
            "notify=[]",
            "tools.view_image=false",
        ] {
            assert!(
                args.windows(2)
                    .any(|pair| pair[0] == "-c" && pair[1] == value)
            );
        }
        assert!(
            args.windows(2)
                .any(|pair| pair[0] == "--disable" && pair[1] == "shell_tool")
        );
    }
    drop(adapter);
    assert_children_exit(&children).await;
    println!("relative binary with different workspace: PASS");

    let retry_binary = fixture.0.join(if cfg!(windows) {
        "installed-later.exe"
    } else {
        "installed-later"
    });
    let retry = Codex::with_binary(fixture.0.join("retry-workspace"), retry_binary.clone());
    assert!(!retry.available());
    std::fs::copy(&binary, &retry_binary).unwrap();
    assert!(
        retry.available(),
        "Installation must not require a Hycli restart"
    );
    assert_eq!(retry.refresh_account().await.unwrap(), None);
    drop(retry);
    assert_children_exit(&records(&fixture.0.join("retry-workspace"))).await;

    let path_dir = fixture.0.join("PATH 한 글 with spaces");
    std::fs::create_dir(&path_dir).unwrap();
    let path_binary = path_dir.join(if cfg!(windows) { "codex.exe" } else { "codex" });
    std::fs::copy(&binary, &path_binary).unwrap();
    let auto_workspace = fixture.0.join("automatic-workspace");
    let status = tokio::process::Command::new(std::env::current_exe().unwrap())
        .arg("automatic-probe")
        .arg(&auto_workspace)
        .env("PATH", &path_dir)
        .kill_on_drop(true)
        .status()
        .await
        .unwrap();
    assert!(status.success());
    for child in records(&auto_workspace) {
        assert_selected_binary(&child, &path_binary);
    }
    println!("isolated native PATH discovery and launch: PASS");

    let control_dir = fixture.0.join("hung-control");
    let inference_dir = fixture.0.join("hung-inference");
    for workspace in [&control_dir, &inference_dir] {
        std::fs::create_dir_all(workspace).unwrap();
        std::fs::write(workspace.join("hang-initialize"), b"").unwrap();
    }
    let control = Codex::with_binary(control_dir.clone(), binary.clone());
    let inference = Codex::with_binary(inference_dir.clone(), binary);
    let result = tokio::time::timeout(Duration::from_secs(50), async {
        tokio::join!(
            control.refresh_account(),
            inference.complete("fixture", "synthetic", "never reached"),
        )
    })
    .await
    .expect("Both control and inference initialization must be bounded");
    assert_eq!(result.0.unwrap_err().public_code, "timeout");
    assert_eq!(result.1.unwrap_err().public_code, "timeout");
    let hung_children: Vec<_> = records(&control_dir)
        .into_iter()
        .chain(records(&inference_dir))
        .collect();
    assert_eq!(hung_children.len(), 2);
    assert_children_exit(&hung_children).await;
    for workspace in [&control_dir, &inference_dir] {
        std::fs::remove_file(workspace.join("hang-initialize")).unwrap();
    }
    assert_eq!(control.refresh_account().await.unwrap(), None);
    assert_eq!(
        inference
            .complete("fixture", "synthetic", "recovered")
            .await
            .unwrap(),
        "recovered"
    );
    drop(control);
    drop(inference);
    assert_children_exit(&records(&control_dir)).await;
    assert_children_exit(&records(&inference_dir)).await;
    println!("hung control and inference initialize, child cleanup and retry: PASS");
}

fn assert_selected_binary(child: &Value, expected: &Path) {
    let executable = PathBuf::from(child["exe"].as_str().unwrap());
    assert!(executable.is_absolute());
    assert_eq!(
        executable.canonicalize().unwrap(),
        expected.canonicalize().unwrap()
    );
}

fn records(workspace: &Path) -> Vec<Value> {
    std::fs::read_dir(workspace)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| {
            path.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("child-")
        })
        .map(|path| serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap())
        .collect()
}

async fn assert_children_exit(children: &[Value]) {
    let pids: Vec<_> = children
        .iter()
        .map(|child| sysinfo::Pid::from_u32(child["pid"].as_u64().unwrap() as u32))
        .collect();
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let mut system = sysinfo::System::new();
            system.refresh_processes_specifics(
                sysinfo::ProcessesToUpdate::Some(&pids),
                true,
                sysinfo::ProcessRefreshKind::nothing(),
            );
            if pids.iter().all(|pid| system.process(*pid).is_none()) {
                break;
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    })
    .await
    .expect("Dropped and timed-out app-server children must exit");
}

struct DashboardFixture {
    core: Arc<hycli::runtime::Runtime>,
    workspace: PathBuf,
    client: reqwest::Client,
    origin: String,
    server: tokio::task::JoinHandle<()>,
}
impl DashboardFixture {
    async fn new(root: &Path, binary: &Path) -> Self {
        let mut core = hycli::runtime::Runtime::open(root, None, None, false, true, false).unwrap();
        let workspace = root.join("provider-workspace");
        Arc::get_mut(&mut core).unwrap().ai.codex =
            Arc::new(Codex::with_binary(workspace.clone(), binary.to_owned()));
        std::fs::create_dir_all(&workspace).unwrap();
        let listener = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
            .await
            .unwrap();
        let port = listener.local_addr().unwrap().port();
        let web = hycli::dashboard::Web::new(core.clone(), port).unwrap();
        #[cfg(feature = "dashboard-test-support")]
        let ticket = web.test_bootstrap_ticket();
        // The identical historical harness uses the baseline's public API.
        // Current fixtures must redeem a test-issued ticket through the real route.
        #[cfg(not(feature = "dashboard-test-support"))]
        let (cookie, csrf) = (format!("hycli_session={}", web.session), web.csrf.clone());
        let server = tokio::spawn(async move {
            axum::serve(listener, hycli::dashboard::router(web))
                .await
                .unwrap();
        });
        #[cfg(feature = "dashboard-test-support")]
        let (cookie, csrf) = {
            let response = reqwest::Client::builder()
                .no_proxy()
                .build()
                .unwrap()
                .post(format!("http://127.0.0.1:{port}/api/bootstrap"))
                .json(&json!({"ticket": ticket}))
                .send()
                .await
                .unwrap();
            assert_eq!(response.status(), 200);
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
            (cookie, csrf)
        };
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert(reqwest::header::COOKIE, cookie.parse().unwrap());
        headers.insert("x-hycli-csrf", csrf.parse().unwrap());
        let client = reqwest::Client::builder()
            .default_headers(headers)
            .timeout(Duration::from_secs(20))
            .build()
            .unwrap();
        Self {
            core,
            workspace,
            client,
            origin: format!("http://127.0.0.1:{port}"),
            server,
        }
    }
    fn scenario(&self, value: Value) {
        hycli::util::atomic_file(
            &self.workspace.join("scenario.json"),
            &serde_json::to_vec(&value).unwrap(),
        )
        .unwrap();
    }
    fn complete(&self, id: &str, success: bool) {
        hycli::util::atomic_file(&self.workspace.join("completion.json"),
            &serde_json::to_vec(&json!({"loginId":id,"success":success,"error":"private synthetic provider detail"})).unwrap()).unwrap();
    }
    fn request(&self, method: &str, path: &str) -> reqwest::RequestBuilder {
        self.client.request(
            reqwest::Method::from_bytes(method.as_bytes()).unwrap(),
            format!("{}{path}", self.origin),
        )
    }
    async fn send(&self, method: &str, path: &str, body: Value, status: u16) -> Value {
        response_json(
            self.request(method, path).json(&body).send().await.unwrap(),
            status,
        )
        .await
    }
    async fn account(&self, id: Option<&str>, status: u16) -> Value {
        let path = id
            .map(|id| format!("/api/providers/codex/account?login_id={id}"))
            .unwrap_or_else(|| "/api/providers/codex/account".to_owned());
        self.send("GET", &path, Value::Null, status).await
    }
    async fn login(&self) -> String {
        let response = self
            .send("POST", "/api/providers/codex/login", Value::Null, 200)
            .await;
        assert!(
            response["auth_url"]
                .as_str()
                .unwrap()
                .starts_with("https://auth.openai.com/")
        );
        response["login_id"].as_str().unwrap().to_owned()
    }
    async fn cancel(&self, id: &str) {
        self.send(
            "POST",
            "/api/providers/codex/login/cancel",
            json!({"login_id":id}),
            200,
        )
        .await;
    }
    async fn disconnect(&self) {
        self.send("DELETE", "/api/providers/codex", Value::Null, 200)
            .await;
    }
    fn configured(&self) -> bool {
        self.core
            .state
            .read()
            .unwrap()
            .providers
            .get("codex")
            .is_some_and(|p| p.connected)
    }
    fn view(&self) -> Value {
        serde_json::to_value(
            self.core
                .providers()
                .unwrap()
                .into_iter()
                .find(|p| p.provider.id == "codex")
                .unwrap(),
        )
        .unwrap()
    }
    fn requests(&self, method: &str) -> Vec<Value> {
        std::fs::read_to_string(self.workspace.join("requests.jsonl"))
            .unwrap_or_default()
            .lines()
            .map(|line| serde_json::from_str::<Value>(line).unwrap())
            .filter(|request| request["method"] == method)
            .collect()
    }
    fn hold(&self, method: &str) {
        let marker = method.replace('/', "_");
        let _ = std::fs::remove_file(self.workspace.join(format!("observed-{marker}")));
        std::fs::write(self.workspace.join(format!("hold-{marker}")), b"").unwrap();
    }
    async fn observed(&self, method: &str) {
        let path = self
            .workspace
            .join(format!("observed-{}", method.replace('/', "_")));
        tokio::time::timeout(Duration::from_secs(10), async {
            while !path.exists() {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("Synthetic provider did not receive the request");
    }
    fn login_revision(&self) -> u64 {
        let conn = rusqlite::Connection::open_with_flags(
            self.core.state.db_path(),
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
        )
        .unwrap();
        let revision: String = conn
            .query_row(
                "SELECT value FROM state_meta WHERE key = 'provider_revision:codex'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        revision.parse().unwrap()
    }
    async fn received_login_intent_after(&self, revision: u64) {
        // A fixture-only receipt barrier, not a scheduling sleep: the queued
        // handler must have persisted its intent before Disconnect is issued.
        tokio::time::timeout(Duration::from_secs(10), async {
            while self.login_revision() == revision {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("Queued login did not record its request intent");
    }
    fn release(&self, method: &str) {
        std::fs::remove_file(
            self.workspace
                .join(format!("hold-{}", method.replace('/', "_"))),
        )
        .unwrap();
    }
    async fn stop(self) {
        let children = records(&self.workspace);
        self.server.abort();
        drop(self.client);
        let _ = self.server.await;
        drop(self.core);
        assert_children_exit(&children).await;
    }
}
async fn response_json(response: reqwest::Response, status: u16) -> Value {
    let actual = response.status().as_u16();
    let body: Value = response.json().await.unwrap();
    assert_eq!(actual, status, "Unexpected response: {body}");
    assert!(
        !body
            .to_string()
            .contains("private synthetic provider detail")
    );
    body
}

async fn lifecycle() {
    let fixture = Fixture::new();
    let binary = fixture.binary();
    let app = DashboardFixture::new(&fixture.0.join("lifecycle"), &binary).await;
    assert_eq!(app.view()["verification"], "unknown");
    assert_eq!(app.view()["available"], true);
    assert!(!app.configured());

    app.scenario(json!({"account":"signed_in"}));
    let passive = app.account(None, 200).await;
    assert_eq!(passive["connected"], true);
    assert_eq!(passive["verification"], "verified");
    assert!(
        !app.configured(),
        "Passive verification must not activate a provider"
    );
    assert!(
        app.core
            .state
            .read()
            .unwrap()
            .settings
            .default_provider
            .is_empty()
    );
    assert!(app.requests("model/list").is_empty());
    app.send("POST", "/api/providers/codex/check", Value::Null, 200)
        .await;
    assert!(app.configured());
    assert_eq!(
        app.core.state.read().unwrap().settings.default_provider,
        "codex"
    );

    app.core
        .state
        .update(|data| {
            let provider = data.providers.get_mut("codex").unwrap();
            provider.model = "remembered-model".into();
            provider.models = vec!["remembered-model".into()];
            data.settings.default_provider = "anthropic".into();
            Ok(())
        })
        .unwrap();
    let saved = app.core.state.read().unwrap().providers["codex"].clone();
    app.scenario(json!({"account":"signed_in","models":"fail"}));
    let checked = app
        .send("POST", "/api/providers/codex/check", Value::Null, 200)
        .await;
    assert_eq!(checked["models_error"], "ai_unavailable");
    assert!(app.core.state.read().unwrap().providers["codex"] == saved);
    assert_eq!(app.view()["verification"], "verified");
    assert_eq!(
        app.core.state.read().unwrap().settings.default_provider,
        "anthropic"
    );

    for mode in ["fail", "malformed", "closed"] {
        app.scenario(json!({"account":mode}));
        app.account(None, 502).await;
        let view = app.view();
        assert_eq!(view["connected"], true);
        assert_eq!(view["verification"], "check_failed");
        assert_eq!(view["account"], "fixture@example.invalid");
        assert_eq!(
            view["verification_error"],
            if mode == "malformed" {
                "ai_response_invalid"
            } else {
                "ai_unavailable"
            }
        );
        assert!(app.core.state.read().unwrap().providers["codex"] == saved);
        let config = app.core.config("codex").unwrap();
        assert_eq!(config.model, "remembered-model");
        assert_eq!(
            app.core
                .ai
                .complete(&config, "synthetic", "still usable")
                .await
                .unwrap(),
            "still usable"
        );
    }
    app.scenario(json!({"account":"signed_out"}));
    assert_eq!(app.account(None, 200).await["verification"], "signed_out");
    app.send("POST", "/api/providers/codex/check", Value::Null, 401)
        .await;
    assert!(app.core.state.read().unwrap().providers["codex"] == saved);
    assert_eq!(app.view()["connected"], true);
    let dismissed = app.login().await;
    app.cancel(&dismissed).await;
    assert!(app.core.state.read().unwrap().providers["codex"] == saved);
    assert_eq!(
        app.core.state.read().unwrap().settings.default_provider,
        "anthropic"
    );
    app.disconnect().await;
    app.scenario(json!({"account":"signed_in"}));
    app.account(None, 200).await;
    assert!(
        !app.configured(),
        "Disconnect must survive a later passive verification"
    );
    println!("passive verification, model-list failure, configured inference and disconnect: PASS");

    let first = app.login().await;
    let pending = app.account(Some(&first), 200).await;
    assert_eq!(pending["connected"], true);
    assert_eq!(
        pending["login_status"], "pending",
        "An existing account is not completion evidence"
    );
    assert!(!app.configured());
    app.complete("unrelated-login", true);
    assert_eq!(
        app.account(Some(&first), 200).await["login_status"],
        "pending"
    );
    app.complete(&first, false);
    assert_eq!(
        app.account(Some(&first), 200).await["login_status"],
        "failed"
    );
    assert!(!app.configured());

    app.scenario(json!({"account":"signed_out","early_completion":true}));
    let completed = app.login().await;
    let unverified = app.account(Some(&completed), 200).await;
    assert_eq!(unverified["login_status"], "completed");
    assert_eq!(unverified["connected"], false);
    assert!(!app.configured());
    app.scenario(json!({"account":"signed_in","models":"fail"}));
    let verified = app.account(Some(&completed), 200).await;
    assert_eq!(verified["login_status"], "completed");
    assert_eq!(verified["models_error"], "ai_unavailable");
    assert!(app.configured());
    assert_eq!(app.view()["verification"], "verified");
    assert_eq!(
        app.core.state.read().unwrap().settings.default_provider,
        "anthropic"
    );
    app.disconnect().await;
    app.account(Some(&completed), 409).await;
    assert!(!app.configured());

    app.scenario(json!({"account":"signed_in"}));
    let older = app.login().await;
    let newer = app.login().await;
    let cancelled = app.requests("account/login/cancel");
    assert_eq!(cancelled.last().unwrap()["params"]["loginId"], older);
    app.complete(&older, true);
    assert_eq!(
        app.account(Some(&newer), 200).await["login_status"],
        "pending"
    );
    app.cancel(&older).await;
    app.cancel("not-owned").await;
    assert_eq!(app.requests("account/login/cancel").len(), cancelled.len());
    assert_eq!(
        app.account(Some(&newer), 200).await["login_status"],
        "pending"
    );
    app.cancel(&newer).await;
    assert_eq!(
        app.requests("account/login/cancel").last().unwrap()["params"]["loginId"],
        newer
    );
    app.account(Some(&newer), 409).await;
    assert!(!app.configured());

    app.hold("account/login/start");
    let request = app.request("POST", "/api/providers/codex/login");
    let starting = tokio::spawn(async move { request.send().await.unwrap() });
    app.observed("account/login/start").await;
    app.disconnect().await;
    app.release("account/login/start");
    let started = response_json(starting.await.unwrap(), 200).await;
    let closed_id = started["login_id"].as_str().unwrap();
    app.cancel(closed_id).await;
    assert_eq!(
        app.requests("account/login/cancel").last().unwrap()["params"]["loginId"],
        closed_id
    );
    app.account(Some(closed_id), 409).await;
    println!("owned completion evidence, replacement, scoped cancel and late start cleanup: PASS");

    for (method, path, body) in [
        ("POST", "/api/providers/codex/check", Value::Null),
        (
            "PUT",
            "/api/providers/codex",
            json!({"model":"new-model","default":true}),
        ),
    ] {
        app.hold("model/list");
        let request = app.request(method, path).json(&body);
        let updating = tokio::spawn(async move { request.send().await.unwrap() });
        app.observed("model/list").await;
        app.disconnect().await;
        app.release("model/list");
        response_json(updating.await.unwrap(), 409).await;
        assert!(
            !app.configured(),
            "Late Save/Check must not undo Disconnect"
        );
        assert!(
            !app.core
                .vault
                .lock()
                .unwrap()
                .contains("providers", "codex")
        );
        assert_eq!(
            app.core.state.read().unwrap().settings.default_provider,
            "anthropic"
        );
    }
    for cancel_instead in [true, false] {
        let id = app.login().await;
        app.complete(&id, true);
        app.hold("model/list");
        let request = app.request(
            "GET",
            &format!("/api/providers/codex/account?login_id={id}"),
        );
        let completing = tokio::spawn(async move { request.send().await.unwrap() });
        app.observed("model/list").await;
        if cancel_instead {
            let cancellations = app.requests("account/login/cancel").len();
            app.cancel(&id).await;
            assert_eq!(
                app.requests("account/login/cancel").len(),
                cancellations,
                "A completed attempt is invalidated locally, not sent to login/cancel"
            );
        } else {
            app.disconnect().await;
        }
        app.release("model/list");
        response_json(completing.await.unwrap(), 409).await;
        assert!(
            !app.configured(),
            "Late login model refresh must not undo Cancel/Disconnect"
        );
    }
    // The CLI opens a separate Runtime for auth removal. Its durable intent
    // must invalidate work already awaiting a model response in the dashboard.
    let other =
        hycli::runtime::Runtime::open(&app.core.root, None, None, false, true, false).unwrap();
    app.hold("account/login/start");
    let request = app.request("POST", "/api/providers/codex/login");
    let first_start = tokio::spawn(async move { request.send().await.unwrap() });
    app.observed("account/login/start").await;
    let first_revision = app.login_revision();
    let starts = app.requests("account/login/start").len();
    let cancellations = app.requests("account/login/cancel").len();
    let request = app.request("POST", "/api/providers/codex/login");
    let queued_start = tokio::spawn(async move { request.send().await.unwrap() });
    app.received_login_intent_after(first_revision).await;
    other.remove_provider("codex").unwrap();
    app.release("account/login/start");
    let first_started = response_json(first_start.await.unwrap(), 200).await;
    response_json(queued_start.await.unwrap(), 409).await;
    assert_eq!(
        app.requests("account/login/start").len(),
        starts,
        "A queued pre-Disconnect request must not start a new login"
    );
    assert_eq!(
        app.requests("account/login/cancel").len(),
        cancellations,
        "Rejected admission must not cancel existing ownership"
    );
    assert!(!app.configured());
    let stale_id = first_started["login_id"].as_str().unwrap();
    app.account(Some(stale_id), 409).await;
    let fresh_id = app.login().await;
    let cancellations = app.requests("account/login/cancel").len();
    app.cancel(stale_id).await;
    assert_eq!(app.requests("account/login/cancel").len(), cancellations);
    assert_eq!(
        app.account(Some(&fresh_id), 200).await["login_status"],
        "pending"
    );
    app.cancel(&fresh_id).await;
    println!("queued login intent remains older than cross-process Disconnect: PASS");
    for from_login in [false, true] {
        let path = if from_login {
            let id = app.login().await;
            app.complete(&id, true);
            format!("/api/providers/codex/account?login_id={id}")
        } else {
            "/api/providers/codex/check".to_owned()
        };
        app.hold("model/list");
        let request = app.request(if from_login { "GET" } else { "POST" }, &path);
        let updating = tokio::spawn(async move { request.send().await.unwrap() });
        app.observed("model/list").await;
        other.remove_provider("codex").unwrap();
        app.release("model/list");
        response_json(updating.await.unwrap(), 409).await;
        assert!(
            !app.configured(),
            "A second Runtime's disconnect must invalidate delayed activation"
        );
    }
    drop(other);
    let lost = app.login().await;
    app.scenario(json!({"account":"closed"}));
    app.account(Some(&lost), 502).await;
    app.account(Some(&lost), 409).await;
    assert!(!app.configured());
    assert!(
        app.requests("account/logout").is_empty(),
        "Disconnect/Cancel never log out Codex"
    );
    println!(
        "in-flight save/check/login rejection after disconnect and lost callback ownership: PASS"
    );
    app.scenario(json!({"account":"signed_in"}));
    app.send("POST", "/api/providers/codex/check", Value::Null, 200)
        .await;
    app.stop().await;
    // A fresh process has no verification evidence. Its saved selection survives
    // both restart and the official executable becoming unavailable.
    let restarted = DashboardFixture::new(
        &fixture.0.join("lifecycle"),
        &fixture.0.join("missing-provider"),
    )
    .await;
    let view = restarted.view();
    assert_eq!(view["connected"], true);
    assert_eq!(view["available"], false);
    assert_eq!(view["verification"], "unknown");
    assert!(view.get("verification_error").is_none());
    assert_eq!(view["account"], "");
    assert!(restarted.core.config("codex").is_ok());
    restarted.account(None, 503).await;
    assert_eq!(restarted.view()["verification_error"], "codex_missing");
    assert!(restarted.configured());
    restarted.stop().await;
    println!("configured restart and missing executable are independent of verification: PASS");
}
