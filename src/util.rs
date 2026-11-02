use crate::apperr::{AppError, AppResult};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::Path,
};

pub fn id() -> String {
    uuid::Uuid::new_v4().simple().to_string()
}
pub fn now() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}
pub fn unix_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}
pub fn hash(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}
pub fn valid_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value.as_bytes()[0].is_ascii_lowercase()
        && value
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}
pub fn private_dir(path: &Path) -> AppResult<()> {
    fs::create_dir_all(path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
}
pub fn atomic_file(path: &Path, data: &[u8]) -> AppResult<()> {
    let parent = path
        .parent()
        .ok_or_else(|| AppError::api("bad_request", 400))?;
    private_dir(parent)?;
    let temporary = parent.join(format!(".hycli-{}", id()));
    let result = (|| {
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&temporary)?;
        file.write_all(data)?;
        file.sync_all()?;
        drop(file);
        fs::rename(&temporary, path)?;
        #[cfg(unix)]
        {
            let directory = fs::File::open(parent)?;
            directory.sync_all()?;
        }
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}
pub fn read_bounded(path: &Path, limit: usize) -> AppResult<Vec<u8>> {
    use std::io::Read;
    let file = fs::File::open(path)?;
    if file.metadata()?.len() > limit as u64 {
        return Err(AppError::api("too_large", 413));
    }
    let mut result = Vec::new();
    file.take(limit as u64 + 1).read_to_end(&mut result)?;
    if result.len() > limit {
        return Err(AppError::api("too_large", 413));
    }
    Ok(result)
}
pub fn plain(value: &serde_json::Value) -> String {
    value
        .as_str()
        .map(str::to_owned)
        .unwrap_or_else(|| value.to_string())
}
pub fn constant_eq(a: &str, b: &str) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.bytes()
        .zip(b.bytes())
        .fold(0u8, |acc, (x, y)| acc | (x ^ y))
        == 0
}

// This is applied before results enter a model, log, export, or dashboard response.
pub fn redact(value: &mut serde_json::Value, secrets: &[String]) {
    match value {
        serde_json::Value::Object(object) => {
            if object
                .get("name")
                .or_else(|| object.get("key"))
                .and_then(serde_json::Value::as_str)
                .is_some_and(secret_field)
            {
                if let Some(value) = object.get_mut("value") {
                    *value = serde_json::Value::String("[private]".into());
                }
            }
            let mut renamed = Vec::new();
            for (key, mut child) in std::mem::take(object) {
                if secret_field(&key) {
                    child = serde_json::Value::String("[private]".into());
                } else {
                    redact(&mut child, secrets);
                }
                let mut clean = serde_json::Value::String(key.clone());
                redact(&mut clean, secrets);
                let clean = clean.as_str().expect("redacted string").to_owned();
                if clean == key {
                    object.insert(key, child);
                } else {
                    renamed.push((clean, child));
                }
            }
            // Reserve unchanged names first. Keep every entry when different secrets
            // collapse to one name, without overwriting a legitimate API property.
            let mut next_suffix = std::collections::BTreeMap::new();
            for (base, child) in renamed {
                let mut key = base.clone();
                let next = next_suffix.entry(base.clone()).or_insert(1usize);
                while object.contains_key(&key) {
                    // The underscore keeps short numeric secrets from matching the
                    // generated index on subsequent redaction passes.
                    key = format!("{base}#_{next}");
                    *next += 1;
                }
                object.insert(key, child);
            }
        }
        serde_json::Value::Array(array) => {
            for child in array {
                redact(child, secrets);
            }
        }
        serde_json::Value::String(text) => {
            for secret in secrets {
                // A preference such as "en" or "1" must not corrupt every word or handle.
                // Short values are still removed when they are the complete scalar;
                // credential-named fields are always redacted above.
                if secret.chars().count() >= 8 {
                    *text = text.replace(secret, "[private]");
                } else if !secret.is_empty() {
                    let word = |c: char| c.is_alphanumeric() || c == '_';
                    let mut cleaned = String::new();
                    let mut copied = 0;
                    for (start, _) in text.match_indices(secret) {
                        let end = start + secret.len();
                        if text[..start].chars().next_back().is_none_or(|c| !word(c))
                            && text[end..].chars().next().is_none_or(|c| !word(c))
                        {
                            cleaned.push_str(&text[copied..start]);
                            cleaned.push_str("[private]");
                            copied = end;
                        }
                    }
                    cleaned.push_str(&text[copied..]);
                    *text = cleaned;
                }
            }
            *text = redact_text(text);
        }
        _ => {}
    }
}
pub fn secret_field(key: &str) -> bool {
    let lower = key.to_ascii_lowercase().replace(['-', '_'], "");
    [
        "tokens",
        "cookies",
        "session",
        "auth",
        "token",
        "authtoken",
        "csrf",
        "xsrf",
        "credential",
        "credentials",
        "password",
        "passwd",
        "authorization",
        "cookie",
        "setcookie",
        "accesstoken",
        "refreshtoken",
        "idtoken",
        "csrftoken",
        "xsrftoken",
        "apikey",
        "secret",
        "clientsecret",
        "sessiontoken",
        "sessionid",
        "bearer",
    ]
    .iter()
    .any(|word| lower == *word || lower.ends_with(word))
}
pub fn redact_text(text: &str) -> String {
    use std::sync::LazyLock;
    static TOKENS: LazyLock<regex::Regex> = LazyLock::new(|| {
        regex::Regex::new(r#"(?i)(?:bearer\s+[A-Za-z0-9._~+/=-]+|(?:sk-|xai-)[A-Za-z0-9_-]{16,}|eyJ[A-Za-z0-9_-]{8,}\.[A-Za-z0-9_-]+\.[A-Za-z0-9_-]+)"#).expect("constant pattern")
    });
    static ASSIGNMENTS: LazyLock<regex::Regex> = LazyLock::new(|| {
        regex::Regex::new(r#"(?i)((?:[a-z_-]*token|api[_-]?key|csrf|xsrf|password|passwd|secret|authorization|cookie|session[_-]?id)["']?\s*[:=]\s*)["']?[^\s"'<>,;&}]+["']?"#).expect("constant pattern")
    });
    ASSIGNMENTS
        .replace_all(&TOKENS.replace_all(text, "[private]"), "${1}[private]")
        .into_owned()
}

/// Model output may be a typed tool proposal or SiteSpec. Sanitize string contents
/// before serialization so a textual header example cannot consume JSON quotes.
/// Schema keys such as `auth` contain references, not credential values.
pub fn redact_generated_text(text: &str) -> String {
    fn strings(value: &mut serde_json::Value) {
        match value {
            serde_json::Value::String(text) => *text = redact_text(text),
            serde_json::Value::Array(values) => values.iter_mut().for_each(strings),
            serde_json::Value::Object(values) => values.values_mut().for_each(strings),
            _ => {}
        }
    }
    match serde_json::from_str::<serde_json::Value>(text) {
        Ok(mut value) => {
            strings(&mut value);
            value.to_string()
        }
        Err(_) => redact_text(text),
    }
}

pub fn file_lock(path: &Path, nonblocking: bool) -> AppResult<fs::File> {
    if let Some(parent) = path.parent() {
        private_dir(parent)?;
    }
    let mut options = OpenOptions::new();
    options.read(true).write(true).create(true).truncate(false);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let file = options.open(path)?;
    if nonblocking {
        file.try_lock()
            .map_err(|_| AppError::api("conflict", 409))?;
    } else {
        file.lock()?;
    }
    Ok(file)
}

pub fn is_pid_alive(pid: u32) -> bool {
    if pid == 0 {
        return false;
    }
    if pid == std::process::id() {
        return true;
    }
    #[cfg(target_os = "linux")]
    {
        let Ok(stat) = std::fs::read_to_string(format!("/proc/{pid}/stat")) else {
            return false;
        };
        if let Some((_, rest)) = stat.rsplit_once(") ")
            && let Some(state_ch) = rest.chars().next()
        {
            return !matches!(state_ch, 'Z' | 'X' | 'x');
        }
        return true;
    }
    #[cfg(not(target_os = "linux"))]
    {
        use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System};
        let target = Pid::from_u32(pid);
        let mut system = System::new();
        system.refresh_processes_specifics(
            ProcessesToUpdate::Some(&[target]),
            true,
            ProcessRefreshKind::nothing(),
        );
        system.process(target).is_some()
    }
}

#[cfg(test)]
mod redaction_tests {
    use super::redact;
    use serde_json::json;

    #[test]
    fn secret_keys_are_redacted_recursively_without_losing_colliding_entries() {
        let secrets = vec![
            "access-canary".into(),
            "refresh-canary".into(),
            "client-secret-canary".into(),
        ];
        let mut value = json!({"nested":[{
            "access-canary": {"refresh-canary": "access-canary"},
            "refresh-canary": "second entry",
            "client-secret-canary": "third entry",
            "[private]": "original entry",
            "[private]#_1": "original suffixed entry",
            "subject": "subject-canary", "client_id": "client-canary",
            "access_token": "hidden regardless of known secrets"
        }]});
        redact(&mut value, &secrets);
        let text = value.to_string();
        for secret in &secrets {
            assert!(!text.contains(secret));
        }
        let fields = value["nested"][0].as_object().unwrap();
        assert_eq!(fields.len(), 8);
        assert_eq!(fields["[private]"], "original entry");
        assert_eq!(fields["[private]#_1"], "original suffixed entry");
        assert_eq!(fields["subject"], "subject-canary");
        assert_eq!(fields["client_id"], "client-canary");
        assert_eq!(fields["access_token"], "[private]");
        assert!(fields.values().any(|v| v == "second entry"));
        assert!(fields.values().any(|v| v == "third entry"));
        assert!(
            fields
                .values()
                .any(|v| v == &json!({"[private]":"[private]"}))
        );
        let once = value.clone();
        redact(&mut value, &secrets);
        assert_eq!(value, once);
    }

    #[test]
    fn many_colliding_keys_keep_all_entries_with_bounded_names() {
        let secrets: Vec<String> = (0..1000)
            .map(|i| format!("credential-canary-{i:04}"))
            .collect();
        let mut fields = serde_json::Map::new();
        for (i, secret) in secrets.iter().enumerate() {
            fields.insert(secret.clone(), json!(i));
        }
        fields.insert("[private]".into(), json!("reserved"));
        fields.insert("[private]#_1".into(), json!("reserved suffix"));
        let mut value = serde_json::Value::Object(fields);
        redact(&mut value, &secrets);
        let fields = value.as_object().unwrap();
        assert_eq!(fields.len(), 1002);
        assert_eq!(fields["[private]"], "reserved");
        assert_eq!(fields["[private]#_1"], "reserved suffix");
        assert!(fields.keys().all(|key| key.len() <= 16));
        assert!(value.to_string().len() < 25_000);
        let mut retained: Vec<_> = fields
            .values()
            .filter_map(serde_json::Value::as_u64)
            .collect();
        retained.sort_unstable();
        assert_eq!(retained, (0..1000).collect::<Vec<_>>());
        let once = value.clone();
        redact(&mut value, &secrets);
        assert_eq!(value, once);
    }

    #[test]
    fn key_matching_preserves_short_secret_word_boundaries_and_normal_names() {
        let mut value = json!({"en":1,"en-US":2,"tenant":3,"en_US":4,"1":5,"v1":6,"":7,"subject":"person","client_id":"client"});
        redact(&mut value, &["en".into(), "1".into(), "".into()]);
        assert_eq!(value.as_object().unwrap().len(), 9);
        assert!(value.get("en").is_none());
        assert!(value.get("1").is_none());
        assert_eq!(value["[private]-US"], 2);
        assert_eq!(value["tenant"], 3);
        assert_eq!(value["en_US"], 4);
        assert_eq!(value["v1"], 6);
        assert_eq!(value[""], 7);
        assert_eq!(value["subject"], "person");
        assert_eq!(value["client_id"], "client");
        let once = value.clone();
        redact(&mut value, &["en".into(), "1".into(), "".into()]);
        assert_eq!(value, once);
    }
}
