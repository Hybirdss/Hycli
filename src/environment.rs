//! Discover local capabilities from OS/application evidence, never a developer machine's paths.
use crate::{browser_discovery::BrowserProfile, util};
use serde::Serialize;
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    path::{Path, PathBuf},
    sync::{Condvar, Mutex, OnceLock},
    time::{Duration, Instant},
};

#[derive(Clone, Serialize)]
pub struct BrowserEngine {
    pub id: String,
    pub name: String,
    pub protocol: String,
    pub discovered_from: String,
    #[serde(skip)]
    pub executable: PathBuf,
}

#[derive(Clone, Serialize)]
pub struct Environment {
    pub discovery_pending: bool,
    pub os: String,
    pub architecture: String,
    pub browsers: Vec<BrowserEngine>,
    pub profiles: Vec<BrowserProfile>,
    pub capabilities: Vec<Value>,
}

#[derive(Default)]
struct Inventory {
    engines: BTreeMap<PathBuf, BrowserEngine>,
    roots: Vec<(String, PathBuf)>,
    profiles: BTreeMap<PathBuf, BrowserProfile>,
    active_roots: BTreeSet<PathBuf>,
}

fn family(name: &str) -> Option<&'static str> {
    let name = name.to_ascii_lowercase();
    if name.contains("firefox") || name.contains("librewolf") {
        Some("Firefox")
    } else if name.contains("msedge")
        || name.contains("microsoft edge")
        || name.contains("microsoft-edge")
        || name == "edge"
    {
        Some("Edge")
    } else if name.contains("brave") {
        Some("Brave")
    } else if name.contains("chromium") {
        Some("Chromium")
    } else if name.contains("chrome") {
        Some("Chrome")
    } else if name.contains("vivaldi") || name.contains("opera") {
        Some("Chromium")
    } else {
        None
    }
}

fn family_for_path(path: &Path) -> String {
    path.ancestors()
        .take(5)
        .find_map(|p| p.file_name().and_then(|n| family(&n.to_string_lossy())))
        .unwrap_or("Chromium")
        .into()
}

fn argument(args: &[std::ffi::OsString], name: &str) -> Option<PathBuf> {
    for (i, arg) in args.iter().enumerate() {
        let value = arg.to_string_lossy();
        let path = if value == name {
            args.get(i + 1).map(|s| s.to_string_lossy().into_owned())
        } else {
            value.strip_prefix(&format!("{name}=")).map(str::to_owned)
        };
        if let Some(path) = path.filter(|p| !p.is_empty()) {
            return Some(PathBuf::from(path.trim_matches('"')));
        }
    }
    None
}

impl Inventory {
    fn engine(&mut self, path: PathBuf, source: &str, known_family: Option<&str>) {
        if !path.is_file() {
            return;
        }
        let name = known_family.map(str::to_owned).or_else(|| {
            path.file_name()
                .and_then(|name| family(&name.to_string_lossy()).map(str::to_owned))
        });
        let Some(name) = name else {
            return;
        };
        let path = std::fs::canonicalize(&path).unwrap_or(path);
        if source == "user_configuration" {
            self.engines.remove(&path);
        }
        self.engines
            .entry(path.clone())
            .or_insert_with(|| BrowserEngine {
                id: format!(
                    "browser-{}",
                    &util::hash(path.to_string_lossy().as_bytes())[..16]
                ),
                protocol: if name == "Firefox" {
                    "firefox"
                } else {
                    "chromium-cdp"
                }
                .into(),
                name,
                discovered_from: source.into(),
                executable: path,
            });
    }

    fn profile(&mut self, path: PathBuf, browser: &str, label: Option<&str>) {
        if !path.is_dir() {
            return;
        }
        let path = std::fs::canonicalize(&path).unwrap_or(path);
        self.profiles
            .entry(path.clone())
            .or_insert_with(|| BrowserProfile {
                id: util::hash(path.to_string_lossy().as_bytes())[..16].into(),
                browser: browser.into(),
                profile: label
                    .filter(|s| !s.trim().is_empty())
                    .map(str::to_owned)
                    .unwrap_or_else(|| {
                        path.file_name()
                            .unwrap_or_default()
                            .to_string_lossy()
                            .into_owned()
                    }),
                running: self.active_roots.iter().any(|root| path.starts_with(root)),
                last_modified_ms: ["Preferences", "prefs.js"]
                    .iter()
                    .filter_map(|name| {
                        std::fs::metadata(path.join(name))
                            .ok()?
                            .modified()
                            .ok()?
                            .duration_since(std::time::UNIX_EPOCH)
                            .ok()
                            .map(|v| v.as_millis() as u64)
                    })
                    .max()
                    .unwrap_or(0),
                path,
            });
    }

    fn running_apps(&mut self) {
        use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, System, UpdateKind};
        let mut system = System::new();
        system.refresh_processes_specifics(
            ProcessesToUpdate::All,
            true,
            ProcessRefreshKind::nothing()
                .with_exe(UpdateKind::OnlyIfNotSet)
                .with_cmd(UpdateKind::OnlyIfNotSet)
                .without_environ()
                .without_tasks(),
        );
        for process in system.processes().values() {
            let Some(executable) = process.exe() else {
                continue;
            };
            let Some(browser) = executable
                .file_name()
                .and_then(|name| family(&name.to_string_lossy()))
            else {
                continue;
            };
            self.engine(executable.to_owned(), "running_application", Some(browser));
            // Inspect only profile-location flags. Process environments and other arguments
            // are never retained or returned to a model.
            if let Some(root) =
                argument(process.cmd(), "--user-data-dir").filter(|p| p.is_absolute())
            {
                if !root.join(".hycli-session").is_file() {
                    self.active_roots
                        .insert(std::fs::canonicalize(&root).unwrap_or(root.clone()));
                    self.roots.push((browser.into(), root));
                }
            }
            if browser == "Firefox" {
                if let Some(profile) = argument(process.cmd(), "-profile")
                    .or_else(|| argument(process.cmd(), "--profile"))
                    .filter(|p| p.is_absolute())
                {
                    self.active_roots
                        .insert(std::fs::canonicalize(&profile).unwrap_or(profile.clone()));
                    self.profile(profile, browser, None);
                }
            }
        }
    }

    fn path_apps(&mut self) {
        if let Some(executable) =
            std::env::var_os("HYCLI_BROWSER_EXECUTABLE").or_else(|| std::env::var_os("CHROME"))
        {
            self.engine(
                PathBuf::from(executable),
                "user_configuration",
                Some("Chromium"),
            );
        }
        if let Some(paths) = std::env::var_os("PATH") {
            for root in std::env::split_paths(&paths) {
                let Ok(entries) = std::fs::read_dir(root) else {
                    continue;
                };
                for entry in entries.filter_map(Result::ok).take(8192) {
                    let name = entry.file_name();
                    let name = name.to_string_lossy();
                    if !name.contains("driver")
                        && !name.contains("crash")
                        && family(&name).is_some()
                    {
                        self.engine(entry.path(), "executable_search_path", family(&name));
                    }
                }
            }
        }
    }

    fn registered_apps(&mut self, dirs: &directories::BaseDirs) {
        #[cfg(target_os = "linux")]
        {
            let mut roots = vec![dirs.data_dir().to_path_buf()];
            let system = std::env::var_os("XDG_DATA_DIRS")
                .unwrap_or_else(|| "/usr/local/share:/usr/share".into());
            roots.extend(std::env::split_paths(&system));
            for root in roots {
                let Ok(entries) = std::fs::read_dir(root.join("applications")) else {
                    continue;
                };
                for entry in entries.filter_map(Result::ok).take(2048) {
                    if entry.path().extension().is_none_or(|e| e != "desktop") {
                        continue;
                    }
                    let Ok(bytes) = util::read_bounded(&entry.path(), 64 * 1024) else {
                        continue;
                    };
                    let text = String::from_utf8_lossy(&bytes);
                    let label = text
                        .lines()
                        .find_map(|l| l.strip_prefix("Name="))
                        .unwrap_or("");
                    let Some(browser) =
                        family(label).or_else(|| family(&entry.file_name().to_string_lossy()))
                    else {
                        continue;
                    };
                    if let Some(command) = text.lines().find_map(|l| l.strip_prefix("Exec=")) {
                        if let Some(executable) =
                            executable_word(command).and_then(|p| which::which(p).ok())
                        {
                            self.engine(executable, "application_registration", Some(browser));
                        }
                    }
                }
            }
        }
        #[cfg(target_os = "macos")]
        {
            // Standard application containers, then bundle metadata; no browser-specific path.
            for root in [
                PathBuf::from("/Applications"),
                dirs.home_dir().join("Applications"),
            ] {
                let Ok(entries) = std::fs::read_dir(root) else {
                    continue;
                };
                for entry in entries.filter_map(Result::ok).take(2048) {
                    let Some(browser) = family(&entry.file_name().to_string_lossy()) else {
                        continue;
                    };
                    let plist = entry.path().join("Contents/Info.plist");
                    if let Some(binary) = metadata_command(
                        "plutil",
                        &[
                            "-extract",
                            "CFBundleExecutable",
                            "raw",
                            "-o",
                            "-",
                            &plist.to_string_lossy(),
                        ],
                    ) {
                        self.engine(
                            entry.path().join("Contents/MacOS").join(binary.trim()),
                            "application_registration",
                            Some(browser),
                        );
                    }
                }
            }
        }
        #[cfg(windows)]
        {
            let _ = dirs;
            for hive in ["HKCU", "HKLM"] {
                for key in [
                    format!("{hive}\\SOFTWARE\\Clients\\StartMenuInternet"),
                    format!("{hive}\\SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\App Paths"),
                ] {
                    let Some(text) = metadata_command("reg", &["query", &key, "/s"]) else {
                        continue;
                    };
                    for line in text.lines() {
                        if let Some((_, command)) = line.split_once("REG_SZ") {
                            if let Some(path) = executable_word(command.trim()) {
                                self.engine(path, "application_registration", None);
                            }
                        }
                    }
                }
            }
        }
    }

    fn configuration_roots(&mut self, dirs: &directories::BaseDirs) {
        for path in [
            dirs.config_dir(),
            dirs.config_local_dir(),
            dirs.data_local_dir(),
        ] {
            self.roots.push((String::new(), path.to_owned()));
        }
        // Legacy application-owned configuration and container directories may be outside XDG.
        // Discover their contents by browser database/registration markers, not directory names.
        if let Ok(entries) = std::fs::read_dir(dirs.home_dir()) {
            for entry in entries.filter_map(Result::ok).take(1024) {
                let name = entry.file_name().to_string_lossy().into_owned();
                if name.starts_with('.')
                    && entry.file_type().is_ok_and(|t| t.is_dir())
                    && ![".cache", ".git", ".cargo", ".rustup", ".npm", ".local"]
                        .contains(&name.as_str())
                {
                    self.roots.push((String::new(), entry.path()));
                }
            }
        }
        for key in ["SNAP_USER_DATA", "SNAP_USER_COMMON", "FLATPAK_USER_DIR"] {
            if let Some(path) = std::env::var_os(key) {
                self.roots.push((String::new(), path.into()));
            }
        }
    }

    #[cfg(test)]
    fn scan_profiles(&mut self) {
        self.scan_profiles_with(|_| {});
    }

    fn scan_profiles_with(&mut self, publish: impl Fn(&Self)) {
        let mut queue: VecDeque<_> = std::mem::take(&mut self.roots)
            .into_iter()
            .map(|(browser, path)| (browser, path, 0))
            .collect();
        let mut visited = BTreeSet::new();
        let mut inspected = 0;
        while let Some((hint, root, depth)) = queue.pop_front() {
            publish(self);
            if inspected >= 12_000 {
                break;
            }
            if depth > 7 || !root.is_dir() {
                continue;
            }
            let root = std::fs::canonicalize(&root).unwrap_or(root);
            if !visited.insert(root.clone()) {
                continue;
            }
            inspected += 1;
            if root.join(".hycli-session").is_file() {
                continue;
            }
            if root.join("profiles.ini").is_file() {
                if let Some(profiles) = crate::browser_discovery::firefox_profiles(&root) {
                    for path in profiles {
                        self.profile(path, "Firefox", None);
                    }
                }
                continue;
            }
            if root.join("Local State").is_file() {
                let browser = if hint.is_empty() {
                    family_for_path(&root)
                } else {
                    hint.clone()
                };
                if let Ok(bytes) = util::read_bounded(&root.join("Local State"), 16 * 1024 * 1024) {
                    if let Ok(data) = serde_json::from_slice::<Value>(&bytes) {
                        if let Some(profiles) = data
                            .pointer("/profile/info_cache")
                            .and_then(Value::as_object)
                        {
                            for (relative, value) in profiles {
                                let path = Path::new(relative);
                                if path.components().count() == 1 && !relative.starts_with('.') {
                                    self.profile(
                                        root.join(path),
                                        &browser,
                                        value.get("name").and_then(Value::as_str),
                                    );
                                }
                            }
                        }
                    }
                }
            }
            if root.join("prefs.js").is_file()
                && (root.join("cookies.sqlite").is_file() || root.join("storage").is_dir())
            {
                self.profile(root, "Firefox", None);
                continue;
            }
            if root.join("Preferences").is_file()
                && (root.join("Network/Cookies").is_file()
                    || root.join("Cookies").is_file()
                    || root.join("Local Storage").is_dir())
            {
                let browser = if hint.is_empty() {
                    family_for_path(&root)
                } else {
                    hint.clone()
                };
                self.profile(root, &browser, None);
                continue;
            }
            let Ok(entries) = std::fs::read_dir(&root) else {
                continue;
            };
            for entry in entries.filter_map(Result::ok).take(1024) {
                if !entry.file_type().is_ok_and(|t| t.is_dir()) {
                    continue;
                }
                let name = entry.file_name().to_string_lossy().to_ascii_lowercase();
                if [
                    "cache",
                    "code cache",
                    "gpucache",
                    "node_modules",
                    "target",
                    ".git",
                    "extensions",
                    "crash reports",
                    "backup",
                    "backups",
                ]
                .contains(&name.as_str())
                    || name.contains("-backup")
                {
                    continue;
                }
                queue.push_back((hint.clone(), entry.path(), depth + 1));
            }
        }
    }
}

fn executable_word(command: &str) -> Option<PathBuf> {
    let command = command.trim();
    if let Some(quoted) = command.strip_prefix('"') {
        quoted.split_once('"').map(|(path, _)| PathBuf::from(path))
    } else if let Some(end) = command.to_ascii_lowercase().find(".exe") {
        // Windows App Paths values can be an unquoted absolute executable with spaces.
        Some(PathBuf::from(&command[..end + 4]))
    } else {
        command.split_whitespace().next().map(PathBuf::from)
    }
}

#[cfg(any(windows, target_os = "macos"))]
fn metadata_command(binary: &str, args: &[&str]) -> Option<String> {
    use std::{
        io::Read,
        process::{Command, Stdio},
        time::{Duration, Instant},
    };
    let mut child = Command::new(which::which(binary).ok()?)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let stdout = child.stdout.take()?;
    let (sender, receiver) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let mut bytes = vec![];
        let _ = stdout.take(4 * 1024 * 1024).read_to_end(&mut bytes);
        let _ = sender.send(bytes);
    });
    let deadline = Instant::now() + Duration::from_secs(4);
    loop {
        if let Some(status) = child.try_wait().ok()? {
            if !status.success() {
                return None;
            }
            break;
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            return None;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    String::from_utf8(receiver.recv_timeout(Duration::from_secs(1)).ok()?).ok()
}

// One shared discovery worker avoids multiplying slow filesystem scans. A stalled
// profile or mount must not hold up website preparation or browser rendering.
struct Discovery {
    snapshot: Environment,
    running: bool,
    completed: Option<Instant>,
}
static DISCOVERY: OnceLock<(Mutex<Discovery>, Condvar)> = OnceLock::new();

pub fn inspect() -> Environment {
    let (lock, changed) = DISCOVERY.get_or_init(|| {
        (
            Mutex::new(Discovery {
                snapshot: inventory_snapshot(&Inventory::default(), true),
                running: false,
                completed: None,
            }),
            Condvar::new(),
        )
    });
    let mut state = lock.lock().unwrap_or_else(|error| error.into_inner());
    if state
        .completed
        .is_some_and(|at| at.elapsed() < Duration::from_secs(30))
    {
        return state.snapshot.clone();
    }
    if !state.running {
        state.running = true;
        state.snapshot.discovery_pending = true;
        std::thread::spawn(move || {
            let publish = |inventory: &Inventory| {
                let mut state = lock.lock().unwrap_or_else(|error| error.into_inner());
                state.snapshot = inventory_snapshot(inventory, true);
                changed.notify_all();
            };
            let mut inventory = Inventory::default();
            inventory.path_apps();
            publish(&inventory);
            if let Some(dirs) = directories::BaseDirs::new() {
                inventory.registered_apps(&dirs);
                publish(&inventory);
                inventory.configuration_roots(&dirs);
            }
            inventory.running_apps();
            publish(&inventory);
            // Active profiles should be inspected before the broad configuration scan.
            inventory
                .roots
                .sort_by_key(|(_, path)| !inventory.active_roots.contains(path));
            inventory.scan_profiles_with(publish);
            let mut state = lock.lock().unwrap_or_else(|error| error.into_inner());
            state.snapshot = inventory_snapshot(&inventory, false);
            state.running = false;
            state.completed = Some(Instant::now());
            changed.notify_all();
        });
    }
    let (state, _) = changed
        .wait_timeout_while(state, Duration::from_secs(2), |state| state.running)
        .unwrap_or_else(|error| error.into_inner());
    state.snapshot.clone()
}

fn inventory_snapshot(inventory: &Inventory, pending: bool) -> Environment {
    let mut browsers: Vec<_> = inventory.engines.values().cloned().collect();
    browsers.sort_by_key(|b| {
        (
            match b.discovered_from.as_str() {
                "user_configuration" => 0,
                "running_application" => 1,
                "application_registration" => 2,
                _ => 3,
            },
            b.name.clone(),
            b.id.clone(),
        )
    });
    let mut profiles: Vec<_> = inventory.profiles.values().cloned().collect();
    profiles.sort_by_key(|p| {
        (
            std::cmp::Reverse(p.running),
            std::cmp::Reverse(p.last_modified_ms),
            p.id.clone(),
        )
    });
    Environment {
        discovery_pending: pending,
        os: std::env::consts::OS.into(),
        architecture: std::env::consts::ARCH.into(),
        capabilities: vec![
            json!({"name":"web.read","available":true}),
            json!({"name":"browser.render","available":browsers.iter().any(|b| b.protocol == "chromium-cdp"),"engines":browsers.iter().filter(|b| b.protocol == "chromium-cdp").map(|b| &b.id).collect::<Vec<_>>()}),
            json!({"name":"session.inspect","available":!profiles.is_empty(),"sources":["site_cookies","origin_local_storage"]}),
            json!({"name":"session.verify","available":true,"secret_values_exposed":false}),
            json!({"name":"browser.companion","available":true,"connection_required":true}),
        ],
        browsers,
        profiles,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn registration_finds_custom_profile_names_and_never_exports_paths() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join(".cache/tests")
            .join(util::id());
        std::fs::create_dir_all(root.join("my everyday session")).unwrap();
        std::fs::write(root.join("Local State"), r#"{"profile":{"info_cache":{"my everyday session":{"name":"Personal"},"../outside":{"name":"Invalid"}}}}"#).unwrap();
        let mut inventory = Inventory::default();
        inventory.roots.push(("Edge".into(), root.clone()));
        inventory.scan_profiles();
        assert_eq!(inventory.profiles.len(), 1);
        let profile = inventory.profiles.into_values().next().unwrap();
        assert_eq!(profile.browser, "Edge");
        assert_eq!(profile.profile, "Personal");
        assert!(
            !serde_json::to_string(&profile)
                .unwrap()
                .contains(&root.to_string_lossy().to_string())
        );
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn profile_flags_and_registered_executables_keep_spaces() {
        let args = vec![
            "browser".into(),
            "--user-data-dir=/custom place/profile store".into(),
        ];
        assert_eq!(
            argument(&args, "--user-data-dir"),
            Some(PathBuf::from("/custom place/profile store"))
        );
        assert_eq!(
            executable_word("\"D:\\Portable Apps\\Browser\\chrome.exe\" --other"),
            Some(PathBuf::from("D:\\Portable Apps\\Browser\\chrome.exe"))
        );
        assert_eq!(
            executable_word("D:\\Portable Apps\\Browser\\chrome.exe"),
            Some(PathBuf::from("D:\\Portable Apps\\Browser\\chrome.exe"))
        );
        assert_eq!(family("microsoft-edge-stable"), Some("Edge"));
    }
}
