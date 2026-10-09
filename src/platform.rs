//! Persistent application data; explicit overrides keep CLI, dashboard and MCP aligned.
use std::path::PathBuf;

pub fn data_dir() -> PathBuf {
    if let Some(path) = env_path("HYCLI_DATA_DIR") {
        return path;
    }
    if let Some(path) = env_path("XDG_DATA_HOME") {
        return path.join("hycli");
    }
    let home = env_path("HOME").or_else(|| env_path("USERPROFILE"));
    // Keep an existing installation in place after upgrading the platform defaults.
    if let Some(home) = &home {
        let legacy = home.join(".local/share/hycli");
        if legacy.is_dir() {
            return legacy;
        }
    }
    if cfg!(windows) {
        if let Some(path) = env_path("LOCALAPPDATA").or_else(|| env_path("APPDATA")) {
            return path.join("hycli");
        }
        if let Some(home) = home {
            return home.join("AppData/Local/hycli");
        }
    } else if cfg!(target_os = "macos") {
        if let Some(home) = home {
            return home.join("Library/Application Support/hycli");
        }
    } else if let Some(home) = home {
        return home.join(".local/share/hycli");
    }
    PathBuf::from(".hycli")
}
fn env_path(name: &str) -> Option<PathBuf> {
    std::env::var_os(name)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
}
pub fn resolve_kb(flag: Option<&str>) -> PathBuf {
    flag.map(PathBuf::from)
        .or_else(|| env_path("HYCLI_KB"))
        .unwrap_or_else(|| data_dir().join("kb.db"))
}
pub fn specs_dir() -> PathBuf {
    env_path("HYCLI_SPECS").unwrap_or_else(|| data_dir().join("specs"))
}
pub fn store_path() -> PathBuf {
    data_dir().join("store.json")
}
