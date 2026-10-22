//! Native synthetic app-server. This test executable is not a public CLI target.
use hycli::ai::codex::Codex;
use serde_json::{Value, json};
use std::{
    io::{BufRead, Write},
    path::{Path, PathBuf},
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
    runtime.block_on(run());
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
        let result = match method {
            "initialize" => json!({}),
            "account/read" => json!({"account":null}),
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
