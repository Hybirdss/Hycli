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
            for (key, child) in object.iter_mut() {
                if secret_field(key) {
                    *child = serde_json::Value::String("[private]".into());
                } else {
                    redact(child, secrets);
                }
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
