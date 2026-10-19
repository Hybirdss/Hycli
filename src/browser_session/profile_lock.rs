use crate::{accounts::Credential, util};

pub(super) fn discover_live_cdp_endpoint(credential: &Credential) -> Option<String> {
    for env_key in ["HYCLI_CDP", "CHROME_CDP"] {
        if let Ok(val) = std::env::var(env_key) {
            let trimmed = val.trim();
            if !trimmed.is_empty() {
                return Some(trimmed.to_string());
            }
        }
    }
    if !credential.source_profile.is_empty() {
        let profile_path = std::path::Path::new(&credential.source_profile);
        let mut candidates = vec![profile_path.join("DevToolsActivePort")];
        if let Some(parent) = profile_path.parent() {
            candidates.push(parent.join("DevToolsActivePort"));
        }
        for port_file in candidates {
            if let Ok(bytes) = util::read_bounded(&port_file, 4096) {
                if let Ok(text) = String::from_utf8(bytes) {
                    let mut lines = text.lines();
                    if let (Some(port_str), Some(ws_path)) = (lines.next(), lines.next()) {
                        if let Ok(port) = port_str.trim().parse::<u16>() {
                            let ws = ws_path.trim();
                            if ws.starts_with('/') {
                                return Some(format!("ws://127.0.0.1:{port}{ws}"));
                            }
                        }
                    }
                }
            }
        }
    }
    None
}

pub(super) fn acquire_profile_dir(
    origin: &str,
) -> Result<
    (
        std::path::PathBuf,
        Option<std::fs::File>,
        Option<crate::browser_storage::Snapshot>,
    ),
    (),
> {
    use sha2::{Digest, Sha256};
    let base = std::env::var_os("HYCLI_TEST_DATA")
        .map(std::path::PathBuf::from)
        .or_else(|| {
            directories::ProjectDirs::from("", "", "hycli").map(|d| d.cache_dir().to_path_buf())
        })
        .or_else(|| Some(crate::platform::data_dir()));
    if let Some(root) = base {
        let mut hasher = Sha256::new();
        hasher.update(origin.as_bytes());
        let key: String = hasher
            .finalize()
            .iter()
            .take(8)
            .map(|b| format!("{b:02x}"))
            .collect();
        let warm_dir = root.join("browser-profiles").join(key);
        if util::private_dir(&warm_dir).is_ok() {
            let lock_path = warm_dir.join(".hycli-profile.lock");
            if let Ok(lock) = util::file_lock(&lock_path, true) {
                let singleton = warm_dir.join("SingletonLock");
                let live_owner = std::fs::read_link(&singleton)
                    .ok()
                    .and_then(|target| {
                        target
                            .to_str()
                            .and_then(|s| s.rsplit_once('-'))
                            .and_then(|(_, p)| p.parse::<u32>().ok())
                    })
                    .is_some_and(util::is_pid_alive);
                if !live_owner {
                    for name in ["SingletonLock", "SingletonSocket", "SingletonCookie"] {
                        let _ = std::fs::remove_file(warm_dir.join(name));
                    }
                    return Ok((warm_dir, Some(lock), None));
                }
            }
        }
    }
    let snapshot = crate::browser_storage::Snapshot::new()?;
    let path = snapshot.0.clone();
    Ok((path, None, Some(snapshot)))
}
