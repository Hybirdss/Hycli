//! Credential values stay behind the local executor. No command exports them.
use crate::{
    apperr::{AppError, AppResult},
    util,
};
use aes_gcm::{Aes256Gcm, KeyInit, Nonce, aead::Aead};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

#[derive(Serialize, Deserialize)]
struct VaultFile {
    version: u8,
    protection: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    nonce: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    ciphertext: String,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    values: BTreeMap<String, String>,
}

pub struct Store {
    path: PathBuf,
    data: BTreeMap<String, String>,
    key: Option<Vec<u8>>,
    protection: String,
    keyring_allowed: bool,
}

impl Store {
    pub fn open(path: &Path) -> AppResult<Self> {
        Self::open_with_keyring(path, true)
    }
    pub fn open_with_keyring(path: &Path, keyring_allowed: bool) -> AppResult<Self> {
        let mut store = Self {
            path: path.to_owned(),
            data: BTreeMap::new(),
            key: None,
            protection: "file_permissions_only".into(),
            keyring_allowed,
        };
        match util::read_bounded(path, 8 * 1024 * 1024) {
            Ok(bytes) => {
                let value: serde_json::Value = serde_json::from_slice(&bytes)?;
                if value.get("version").is_some() {
                    let file: VaultFile = serde_json::from_value(value)?;
                    if file.version != 1 {
                        return Err(AppError::api("data_invalid", 400));
                    }
                    if file.protection == "os_keyring" {
                        let key = store.existing_key()?;
                        let cipher = Aes256Gcm::new_from_slice(&key)
                            .map_err(|_| AppError::api("credentials_invalid", 401))?;
                        let nonce = STANDARD
                            .decode(file.nonce)
                            .map_err(|_| AppError::api("data_invalid", 400))?;
                        if nonce.len() != 12 {
                            return Err(AppError::api("data_invalid", 400));
                        }
                        let ciphertext = STANDARD
                            .decode(file.ciphertext)
                            .map_err(|_| AppError::api("data_invalid", 400))?;
                        let plaintext = cipher
                            .decrypt(Nonce::from_slice(&nonce), ciphertext.as_ref())
                            .map_err(|_| AppError::api("credentials_invalid", 401))?;
                        store.data = serde_json::from_slice(&plaintext)?;
                        store.key = Some(key);
                        store.protection = "os_keyring".into();
                    } else if file.protection == "file_permissions_only" {
                        store.data = file.values;
                    } else {
                        return Err(AppError::api("data_invalid", 400));
                    }
                } else {
                    // Upgrade the original CLI's private store on its next write.
                    store.data = serde_json::from_value(value)?;
                }
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    if std::fs::metadata(path)?.permissions().mode() & 0o777 != 0o600 {
                        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
                    }
                }
            }
            Err(error) if !path.exists() => {
                let _ = error;
            }
            Err(error) => return Err(error),
        }
        Ok(store)
    }
    fn keyring_entry(&self) -> AppResult<keyring::Entry> {
        if !self.keyring_allowed {
            return Err(AppError::api("credentials_invalid", 401));
        }
        let absolute = std::fs::canonicalize(self.path.parent().unwrap_or(Path::new(".")))
            .unwrap_or_else(|_| self.path.parent().unwrap_or(Path::new(".")).to_owned());
        let account = format!(
            "master-key-v1-{}",
            &util::hash(absolute.to_string_lossy().as_bytes())[..24]
        );
        keyring::Entry::new("hycli.credential-vault", &account)
            .map_err(|_| AppError::api("credentials_invalid", 401))
    }
    fn existing_key(&self) -> AppResult<Vec<u8>> {
        let encoded = self
            .keyring_entry()?
            .get_password()
            .map_err(|_| AppError::api("credentials_invalid", 401))?;
        let key = STANDARD
            .decode(encoded)
            .map_err(|_| AppError::api("credentials_invalid", 401))?;
        if key.len() != 32 {
            return Err(AppError::api("credentials_invalid", 401));
        }
        Ok(key)
    }
    fn prepare_key(&mut self) {
        if self.key.is_some() || !self.keyring_allowed {
            return;
        }
        if let Some(parent) = self.path.parent() {
            if util::private_dir(parent).is_err() {
                return;
            }
        }
        if let Ok(entry) = self.keyring_entry() {
            match entry.get_password() {
                Ok(encoded) => {
                    if let Ok(key) = STANDARD.decode(encoded) {
                        if key.len() == 32 {
                            self.key = Some(key);
                        }
                    }
                }
                Err(keyring::Error::NoEntry) => {
                    let key: [u8; 32] = rand::random();
                    if entry.set_password(&STANDARD.encode(key)).is_ok() {
                        self.key = Some(key.to_vec());
                    }
                }
                Err(_) => {}
            }
        }
        if self.key.is_some() {
            self.protection = "os_keyring".into();
        }
    }
    pub(crate) fn reload(&mut self) -> AppResult<()> {
        *self = Self::open_with_keyring(&self.path, self.keyring_allowed)?;
        Ok(())
    }
    pub(crate) fn get(&self, site: &str, key: &str) -> Option<&str> {
        self.data.get(&format!("{site}/{key}")).map(String::as_str)
    }
    pub(crate) fn values_for_redaction(&self) -> Vec<String> {
        let mut values: Vec<String> = self
            .data
            .values()
            .filter(|s| !s.is_empty())
            .cloned()
            .collect();
        for (name, value) in &self.data {
            if name.starts_with("accounts/") {
                if let Ok(credential) = serde_json::from_str::<crate::accounts::Credential>(value) {
                    values.extend(credential.redaction_values());
                }
            }
        }
        values
    }
    pub fn contains(&self, site: &str, key: &str) -> bool {
        self.data.contains_key(&format!("{site}/{key}"))
    }
    pub fn protection(&self) -> &str {
        &self.protection
    }
    pub fn keys(&self) -> Vec<String> {
        self.data.keys().cloned().collect()
    }
    pub fn set(&mut self, site: &str, key: &str, value: &str) -> AppResult<()> {
        if site.is_empty() || key.is_empty() || value.len() > 2 * 1024 * 1024 {
            return Err(AppError::api("bad_request", 400));
        }
        let _lock = util::file_lock(&self.path.with_extension("vault-lock"), false)?;
        self.reload()?;
        let name = format!("{site}/{key}");
        let previous = self.data.insert(name.clone(), value.into());
        if let Err(error) = self.save() {
            if let Some(old) = previous {
                self.data.insert(name, old);
            } else {
                self.data.remove(&name);
            }
            return Err(error);
        }
        Ok(())
    }
    pub fn delete(&mut self, site: &str, key: &str) -> AppResult<()> {
        let _lock = util::file_lock(&self.path.with_extension("vault-lock"), false)?;
        self.reload()?;
        let name = format!("{site}/{key}");
        let previous = self.data.remove(&name);
        if let Err(error) = self.save() {
            if let Some(old) = previous {
                self.data.insert(name, old);
            }
            return Err(error);
        }
        Ok(())
    }
    fn save(&mut self) -> AppResult<()> {
        self.prepare_key();
        let file = if let Some(key) = &self.key {
            let cipher =
                Aes256Gcm::new_from_slice(key).map_err(|_| AppError::api("internal", 500))?;
            let nonce: [u8; 12] = rand::random();
            let plaintext = serde_json::to_vec(&self.data)?;
            let ciphertext = cipher
                .encrypt(Nonce::from_slice(&nonce), plaintext.as_ref())
                .map_err(|_| AppError::api("internal", 500))?;
            VaultFile {
                version: 1,
                protection: "os_keyring".into(),
                nonce: STANDARD.encode(nonce),
                ciphertext: STANDARD.encode(ciphertext),
                values: BTreeMap::new(),
            }
        } else {
            VaultFile {
                version: 1,
                protection: "file_permissions_only".into(),
                nonce: String::new(),
                ciphertext: String::new(),
                values: self.data.clone(),
            }
        };
        util::atomic_file(&self.path, &serde_json::to_vec(&file)?)
    }
}
