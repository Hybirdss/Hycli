//! On-demand background dashboard. The instance file is a discovery hint, never a PID to kill.
use crate::{
    apperr::{self, AppResult},
    platform, util,
};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::Duration,
};

const INSTANCE: &str = "dashboard-instance.json";

#[derive(Clone, Serialize, Deserialize)]
pub struct Instance {
    pub port: u16,
    pub id: String,
}

impl Instance {
    pub fn url(&self) -> String {
        format!("http://127.0.0.1:{}", self.port)
    }
}

pub struct Registration {
    path: PathBuf,
    id: String,
}
impl Registration {
    // Call only while holding Runtime's exclusive dashboard-process.lock.
    pub fn publish(root: &Path, instance: &Instance) -> AppResult<Self> {
        let path = root.join(INSTANCE);
        util::atomic_file(&path, &serde_json::to_vec(instance)?)?;
        Ok(Self {
            path,
            id: instance.id.clone(),
        })
    }
}
impl Drop for Registration {
    fn drop(&mut self) {
        if read_instance(&self.path).is_some_and(|i| i.id == self.id) {
            let _ = fs::remove_file(&self.path);
        }
    }
}

fn read_instance(path: &Path) -> Option<Instance> {
    let instance: Instance = serde_json::from_slice(&util::read_bounded(path, 4096).ok()?).ok()?;
    (instance.port != 0
        && instance.id.len() == 32
        && instance.id.bytes().all(|c| c.is_ascii_hexdigit()))
    .then_some(instance)
}

fn client() -> AppResult<reqwest::Client> {
    reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(2))
        .build()
        .map_err(|e| apperr::runtime(e, "Cannot connect to Hycli", "Try opening Hycli again."))
}

async fn session(instance: &Instance) -> Option<(String, String)> {
    let response = client()
        .ok()?
        .get(format!("{}/api/session", instance.url()))
        .send()
        .await
        .ok()?;
    if !response.status().is_success() {
        return None;
    }
    let cookie = response
        .headers()
        .get(reqwest::header::SET_COOKIE)?
        .to_str()
        .ok()?
        .split(';')
        .next()?
        .to_owned();
    let value: serde_json::Value = response.json().await.ok()?;
    if value["instance_id"].as_str()? != instance.id {
        return None;
    }
    Some((cookie, value["csrf"].as_str()?.to_owned()))
}

pub async fn running(root: &Path) -> Option<Instance> {
    let instance = read_instance(&root.join(INSTANCE))?;
    session(&instance).await?;
    Some(instance)
}

fn open_browser(instance: &Instance) -> AppResult<()> {
    crate::dashboard::open_url(&instance.url()).map_err(|error| {
        apperr::runtime(
            error,
            "Hycli is running, but the browser could not be opened",
            format!("Open {} in your browser.", instance.url()),
        )
    })
}

pub async fn open(
    port: u16,
    browser: bool,
    allow_local: bool,
    kb: Option<&str>,
) -> AppResult<Instance> {
    let root = std::path::absolute(platform::data_dir())?;
    util::private_dir(&root)?;
    // Serialize rapid double clicks without blocking a Tokio worker on a file lock.
    let deadline = tokio::time::Instant::now() + Duration::from_secs(60);
    let _launch = loop {
        match util::file_lock(&root.join("dashboard-launch.lock"), true) {
            Ok(lock) => break lock,
            Err(e) if e.public_code == "conflict" && tokio::time::Instant::now() < deadline => {
                tokio::time::sleep(Duration::from_millis(100)).await
            }
            Err(e) => return Err(e),
        }
    };
    let instance = if let Some(instance) = running(&root).await {
        instance
    } else {
        // A foreground instance may still be starting or completing shutdown. Wait for
        // its lock instead of spawning a second writer or treating its port as ours.
        loop {
            if let Some(instance) = running(&root).await {
                if browser {
                    open_browser(&instance)?;
                }
                return Ok(instance);
            }
            if util::file_lock(&root.join("dashboard-process.lock"), true).is_ok() {
                break;
            }
            if tokio::time::Instant::now() >= deadline {
                return Err(apperr::runtime_msg(
                    "Another Hycli instance is not responding",
                    "Close the older instance and open Hycli again.",
                ));
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        let log_path = root.join("dashboard.log");
        // Bound startup log growth. Contains diagnostics, never request bodies or credentials.
        if fs::metadata(&log_path).is_ok_and(|m| m.len() > 1024 * 1024) {
            let _ = fs::rename(&log_path, root.join("dashboard.previous.log"));
        }
        let mut options = fs::OpenOptions::new();
        options.create(true).append(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let log = options.open(&log_path)?;
        let mut command = Command::new(std::env::current_exe()?);
        command.args([
            "dashboard",
            "--no-open",
            "--auto-port",
            "--port",
            &port.to_string(),
        ]);
        if allow_local {
            command.arg("--allow-local");
        }
        if let Some(kb) = kb {
            command.arg("--kb").arg(std::path::absolute(kb)?);
        }
        // Pin resolved paths before changing cwd, including relative environment overrides.
        command
            .env("HYCLI_DATA_DIR", &root)
            .env("HYCLI_SPECS", std::path::absolute(platform::specs_dir())?)
            .env("HYCLI_KB", std::path::absolute(platform::resolve_kb(kb))?);
        command
            .current_dir(&root)
            .stdin(Stdio::null())
            .stdout(log.try_clone()?)
            .stderr(log);
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            command.process_group(0);
        }
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x08000000 | 0x00000200);
        }
        let mut child = command.spawn()?;
        loop {
            if let Some(instance) = running(&root).await {
                break instance;
            }
            if child.try_wait()?.is_some() {
                // A foreground launch may have won the runtime lock while we spawned.
                if let Some(instance) = running(&root).await {
                    break instance;
                }
                return Err(apperr::runtime_msg(
                    "Hycli could not start",
                    format!(
                        "See {}. Close any older Hycli instance and try again.",
                        log_path.display()
                    ),
                ));
            }
            if tokio::time::Instant::now() >= deadline {
                // This is our own child handle, never an unverified PID from disk.
                let _ = child.kill();
                let _ = child.wait();
                return Err(apperr::runtime_msg(
                    "Hycli startup timed out",
                    format!("See {}.", log_path.display()),
                ));
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    };
    if browser {
        open_browser(&instance)?;
    }
    Ok(instance)
}

pub async fn stop(root: &Path) -> AppResult<()> {
    let Some(instance) = running(root).await else {
        return Ok(());
    };
    let (cookie, csrf) = session(&instance)
        .await
        .ok_or_else(|| apperr::runtime_msg("Hycli stopped responding", "Try again."))?;
    let response = client()?
        .post(format!("{}/api/shutdown", instance.url()))
        .header(reqwest::header::COOKIE, cookie)
        .header("x-hycli-csrf", csrf)
        .send()
        .await
        .map_err(|e| apperr::runtime(e, "Cannot stop Hycli", "Try again."))?;
    if response.status() == reqwest::StatusCode::CONFLICT {
        return Err(apperr::usage(
            "Hycli still has active work",
            "Wait for it to finish or cancel it in Activity, then quit again.",
        ));
    }
    if !response.status().is_success() {
        return Err(apperr::runtime_msg(
            "Cannot stop Hycli",
            "Open the dashboard and try again.",
        ));
    }
    for _ in 0..100 {
        // Wait for the exclusive runtime lock, not just the HTTP socket, to be released.
        if util::file_lock(&root.join("dashboard-process.lock"), true).is_ok() {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    Err(apperr::runtime_msg(
        "Hycli is still shutting down",
        "Wait a moment and run hycli status.",
    ))
}
