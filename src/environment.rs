//! Discover local capabilities from OS/application evidence, never a developer machine's paths.
use crate::util;
use serde::Serialize;
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    path::{Path, PathBuf},
    sync::{Condvar, Mutex, OnceLock},
    time::{Duration, Instant},
};

#[derive(Clone, Default, Serialize)]
pub struct BrowserProfile {
    pub id: String,
    pub browser: String,
    pub profile: String,
    pub running: bool,
    pub last_modified_ms: u64,
    #[serde(skip)]
    pub(crate) path: PathBuf,
}

pub(crate) fn firefox_profiles(root: &Path) -> Option<Vec<PathBuf>> {
    let ini = [
        root.join("profiles.ini"),
        root.parent()?.join("profiles.ini"),
    ]
    .into_iter()
    .find(|path| path.is_file())?;
    let text = String::from_utf8(util::read_bounded(&ini, 64 * 1024).ok()?).ok()?;
    let mut profiles = vec![];
    let mut profile = false;
    let mut path = String::new();
    let mut relative = true;
    let mut finish = |profile: bool, path: &str, relative: bool| {
        if profile && !path.is_empty() {
            profiles.push(if relative {
                ini.parent().unwrap().join(path)
            } else {
                PathBuf::from(path)
            });
        }
    };
    for line in text.lines().chain(std::iter::once("[End]")) {
        let line = line.trim();
        if line.starts_with('[') && line.ends_with(']') {
            finish(profile, &path, relative);
            path.clear();
            relative = true;
            profile = line.starts_with("[Profile");
        } else if let Some(value) = line.strip_prefix("Path=") {
            path = value.trim().into();
        } else if let Some(value) = line.strip_prefix("IsRelative=") {
            relative = value.trim() != "0";
        }
    }
    Some(profiles)
}

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
    if name.contains("headless")
        || name.contains("driver")
        || name.contains("crash")
        || name.contains("agent")
        || name.starts_with("scoped_dir")
        || name.contains("-backup")
    {
        return None;
    }
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
    } else if name.contains("vivaldi")
        || name.contains("opera")
        || name.contains("whale")
        || name == "arc"
        || name == "arc.app"
        || name == "arc.exe"
        || name == "arc browser"
    {
        Some("Chromium")
    } else {
        None
    }
}

fn family_for_path(path: &Path) -> Option<String> {
    path.ancestors()
        .take(5)
        .find_map(|p| p.file_name().and_then(|n| family(&n.to_string_lossy())))
        .map(str::to_owned)
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
        if matches!(source, "user_configuration" | "cloakbrowser_stealth") {
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

    fn cloakbrowser_apps(&mut self, cache_roots: &[PathBuf]) {
        if let Some(override_bin) = std::env::var_os("CLOAKBROWSER_BINARY_PATH") {
            let bin = PathBuf::from(override_bin);
            if bin.is_file() {
                self.engine(bin, "cloakbrowser_stealth", Some("CloakBrowser"));
            }
        }
        let mut candidates: Vec<(Vec<u32>, bool, PathBuf)> = Vec::new();
        for root in cache_roots {
            let Ok(entries) = std::fs::read_dir(root) else {
                continue;
            };
            for entry in entries.filter_map(Result::ok).take(128) {
                let dir_name = entry.file_name().to_string_lossy().into_owned();
                let Some(ver_raw) = dir_name.strip_prefix("chromium-") else {
                    continue;
                };
                let (ver_str, is_pro) = match ver_raw.strip_suffix("-pro") {
                    Some(v) => (v, true),
                    None => (ver_raw, false),
                };
                let version: Vec<u32> = ver_str
                    .split('.')
                    .filter_map(|part| part.parse::<u32>().ok())
                    .collect();
                if version.is_empty() {
                    continue;
                }
                let dir_path = entry.path();
                for rel in [
                    "chrome",
                    "chrome.exe",
                    "chrome-win/chrome.exe",
                    "chrome-win64/chrome.exe",
                    "chrome-win32/chrome.exe",
                    "Chromium.app/Contents/MacOS/Chromium",
                    "Google Chrome for Testing.app/Contents/MacOS/Google Chrome for Testing",
                    "Chrome.app/Contents/MacOS/Chrome",
                    "chrome-mac/Chromium.app/Contents/MacOS/Chromium",
                    "chrome-mac-arm64/Google Chrome for Testing.app/Contents/MacOS/Google Chrome for Testing",
                    "chrome-mac-x64/Google Chrome for Testing.app/Contents/MacOS/Google Chrome for Testing",
                ] {
                    let bin = dir_path.join(rel);
                    if bin.is_file() {
                        candidates.push((version.clone(), is_pro, bin));
                        break;
                    }
                }
            }
        }
        candidates.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| b.1.cmp(&a.1)));
        if let Some((_, _, best_bin)) = candidates.into_iter().next() {
            self.engine(best_bin, "cloakbrowser_stealth", Some("CloakBrowser"));
        }
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
        let mut cloak_roots = Vec::new();
        if let Some(custom) = std::env::var_os("CLOAKBROWSER_CACHE_DIR") {
            cloak_roots.push(PathBuf::from(custom));
        }
        if let Some(dirs) = directories::BaseDirs::new() {
            cloak_roots.push(dirs.home_dir().join(".cloakbrowser"));
            cloak_roots.push(dirs.data_local_dir().join("cloakbrowser"));
            cloak_roots.push(dirs.cache_dir().join("cloakbrowser"));
        }
        self.cloakbrowser_apps(&cloak_roots);
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
            // Standard application containers, then bundle metadata; check direct bundle stem before spawning plutil.
            for root in [
                PathBuf::from("/Applications"),
                dirs.home_dir().join("Applications"),
            ] {
                let Ok(entries) = std::fs::read_dir(root) else {
                    continue;
                };
                for entry in entries.filter_map(Result::ok).take(2048) {
                    let file_name = entry.file_name().to_string_lossy().into_owned();
                    let Some(browser) = family(&file_name) else {
                        continue;
                    };
                    let macos_dir = entry.path().join("Contents/MacOS");
                    if let Some(stem) = file_name.strip_suffix(".app") {
                        let direct = macos_dir.join(stem);
                        if direct.is_file() {
                            self.engine(direct, "application_registration", Some(browser));
                            continue;
                        }
                        let lower_direct = macos_dir.join(stem.to_ascii_lowercase());
                        if lower_direct.is_file() {
                            self.engine(lower_direct, "application_registration", Some(browser));
                            continue;
                        }
                    }
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
                            macos_dir.join(binary.trim()),
                            "application_registration",
                            Some(browser),
                        );
                    }
                }
            }
        }
        #[cfg(windows)]
        {
            for env_key in ["ProgramFiles", "ProgramFiles(x86)", "LOCALAPPDATA"] {
                if let Some(base) = std::env::var_os(env_key).map(PathBuf::from) {
                    for (rel, fam) in [
                        ("Google\\Chrome\\Application\\chrome.exe", "Chrome"),
                        ("Microsoft\\Edge\\Application\\msedge.exe", "Edge"),
                        (
                            "BraveSoftware\\Brave-Browser\\Application\\brave.exe",
                            "Brave",
                        ),
                        ("Chromium\\Application\\chrome.exe", "Chromium"),
                        ("Vivaldi\\Application\\vivaldi.exe", "Chromium"),
                        ("Naver\\Naver Whale\\Application\\whale.exe", "Chromium"),
                        ("Mozilla Firefox\\firefox.exe", "Firefox"),
                    ] {
                        let candidate = base.join(rel);
                        if candidate.is_file() {
                            self.engine(candidate, "application_registration", Some(fam));
                        }
                    }
                }
            }
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
        let mut base_dirs = Vec::new();
        for path in [
            dirs.config_dir(),
            dirs.config_local_dir(),
            dirs.data_local_dir(),
        ] {
            if !base_dirs.contains(&path) {
                base_dirs.push(path);
            }
        }
        for base in &base_dirs {
            for (rel, fam) in [
                ("Google/Chrome/User Data", "Chrome"),
                ("Google/Chrome", "Chrome"),
                ("google-chrome", "Chrome"),
                ("Microsoft/Edge/User Data", "Edge"),
                ("Microsoft Edge", "Edge"),
                ("microsoft-edge", "Edge"),
                ("BraveSoftware/Brave-Browser/User Data", "Brave"),
                ("BraveSoftware/Brave-Browser", "Brave"),
                ("Chromium/User Data", "Chromium"),
                ("Chromium", "Chromium"),
                ("chromium", "Chromium"),
                ("Vivaldi/User Data", "Chromium"),
                ("Vivaldi", "Chromium"),
                ("Arc/User Data", "Chromium"),
                ("Naver/Naver Whale/User Data", "Chromium"),
                ("Naver/Whale", "Chromium"),
                ("Mozilla/Firefox", "Firefox"),
                ("Firefox", "Firefox"),
            ] {
                let candidate = base.join(rel);
                if candidate.is_dir() {
                    self.roots.push((fam.into(), candidate));
                }
            }
        }
        for path in base_dirs {
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
            let before_profiles = self.profiles.len();
            if root.join(".hycli-session").is_file() {
                continue;
            }
            if root.join("profiles.ini").is_file() {
                if let Some(profiles) = firefox_profiles(&root) {
                    for path in profiles {
                        self.profile(path, "Firefox", None);
                    }
                }
                if self.profiles.len() != before_profiles {
                    publish(self);
                }
                continue;
            }
            if root.join("Local State").is_file() {
                let browser = if hint.is_empty() {
                    family_for_path(&root)
                } else {
                    Some(hint.clone())
                };
                if let Some(browser) = browser {
                    if let Ok(bytes) =
                        util::read_bounded(&root.join("Local State"), 16 * 1024 * 1024)
                    {
                        if let Ok(data) = serde_json::from_slice::<Value>(&bytes) {
                            if let Some(profiles) = data
                                .pointer("/profile/info_cache")
                                .and_then(Value::as_object)
                            {
                                for (relative, value) in profiles {
                                    let path = Path::new(relative);
                                    if path.components().count() == 1 && !relative.starts_with('.')
                                    {
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
                    if self.profiles.len() != before_profiles {
                        publish(self);
                    }
                }
            }
            if root.join("prefs.js").is_file()
                && (root.join("cookies.sqlite").is_file() || root.join("storage").is_dir())
            {
                self.profile(root, "Firefox", None);
                if self.profiles.len() != before_profiles {
                    publish(self);
                }
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
                    Some(hint.clone())
                };
                if let Some(browser) = browser {
                    self.profile(root, &browser, None);
                    if self.profiles.len() != before_profiles {
                        publish(self);
                    }
                }
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
                    "caches",
                    "code cache",
                    "gpucache",
                    "node_modules",
                    "target",
                    ".git",
                    "extensions",
                    "crash reports",
                    "backup",
                    "backups",
                    "partitions",
                    "headless",
                    "puppeteer",
                    "playwright",
                    "cypress",
                    "logs",
                    "containers",
                    "group containers",
                    "mobilesync",
                    "clouddocs",
                    "xcode",
                    "docker",
                    "steam",
                    "packages",
                    "temp",
                    "programs",
                    "windows",
                ]
                .contains(&name.as_str())
                    || name.contains("-backup")
                    || name.contains("headless")
                    || name.starts_with("scoped_dir")
                    || name.starts_with("launch-preview")
                {
                    continue;
                }
                let next_hint = if hint.is_empty() {
                    family(&name).unwrap_or("").to_owned()
                } else {
                    hint.clone()
                };
                let is_browser_dir = !next_hint.is_empty()
                    || [
                        "mozilla",
                        "bravesoftware",
                        "google",
                        "microsoft",
                        "naver",
                        "arc",
                        "default",
                    ]
                    .iter()
                    .any(|k| name.contains(k))
                    || name.starts_with("profile ");
                if is_browser_dir {
                    queue.push_front((next_hint, entry.path(), depth + 1));
                } else {
                    queue.push_back((next_hint, entry.path(), depth + 1));
                }
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
    let mut command = Command::new(which::which(binary).ok()?);
    command
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    let mut child = command.spawn().ok()?;
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
                "cloakbrowser_stealth" => 1,
                "running_application" => 2,
                "application_registration" => 3,
                _ => 4,
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
        assert_eq!(family("chromium-headless"), None);
    }
    #[test]
    fn ignores_electron_and_headless_ephemeral_directories_during_broad_scan() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join(".cache/tests")
            .join(util::id());
        let real_chromium = root.join("chromium/Default");
        std::fs::create_dir_all(real_chromium.join("Local Storage")).unwrap();
        std::fs::write(real_chromium.join("Preferences"), "{}").unwrap();

        let electron_cursor = root.join("Cursor");
        std::fs::create_dir_all(electron_cursor.join("Local Storage")).unwrap();
        std::fs::write(electron_cursor.join("Preferences"), "{}").unwrap();

        let headless_ephemeral = root.join("chromium-headless/scoped_dir123/Default");
        std::fs::create_dir_all(headless_ephemeral.join("Local Storage")).unwrap();
        std::fs::write(headless_ephemeral.join("Preferences"), "{}").unwrap();

        let mut inventory = Inventory::default();
        inventory.roots.push((String::new(), root.clone()));
        inventory.scan_profiles();
        assert_eq!(inventory.profiles.len(), 1);
        let profile = inventory.profiles.into_values().next().unwrap();
        assert_eq!(profile.browser, "Chromium");
        assert_eq!(profile.profile, "Default");
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn discovers_and_prioritizes_cloakbrowser_stealth_binary() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join(".cache/tests")
            .join(util::id());
        let older = root.join("chromium-145.0.7632.109.2/chrome");
        let newer = root.join("chromium-146.0.7680.177.5/chrome");
        let stock = root.join("system-bin/chromium");
        std::fs::create_dir_all(older.parent().unwrap()).unwrap();
        std::fs::create_dir_all(newer.parent().unwrap()).unwrap();
        std::fs::create_dir_all(stock.parent().unwrap()).unwrap();
        std::fs::write(&older, b"#!/bin/sh\n").unwrap();
        std::fs::write(&newer, b"#!/bin/sh\n").unwrap();
        std::fs::write(&stock, b"#!/bin/sh\n").unwrap();

        let mut inventory = Inventory::default();
        inventory.engine(stock.clone(), "executable_search_path", Some("Chromium"));
        inventory.cloakbrowser_apps(std::slice::from_ref(&root));
        let snapshot = inventory_snapshot(&inventory, false);
        assert_eq!(snapshot.browsers.len(), 2);
        assert_eq!(snapshot.browsers[0].name, "CloakBrowser");
        assert_eq!(snapshot.browsers[0].discovered_from, "cloakbrowser_stealth");
        assert_eq!(snapshot.browsers[0].protocol, "chromium-cdp");
        assert_eq!(
            snapshot.browsers[0].executable,
            std::fs::canonicalize(&newer).unwrap_or(newer)
        );
        assert_eq!(snapshot.browsers[1].name, "Chromium");
        std::fs::remove_dir_all(root).unwrap();
    }
}
