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
    raw_hash: String,
    #[cfg(test)]
    pub(crate) oauth_fail_after: Option<(usize, bool)>,
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
            raw_hash: String::new(),
            #[cfg(test)]
            oauth_fail_after: None,
        };
        store.load_if_changed()?;
        Ok(store)
    }
    #[cfg(test)]
    pub(crate) fn open_oauth_test(path: &Path, key: [u8; 32]) -> AppResult<Self> {
        let mut store = Self {
            path: path.to_owned(),
            data: BTreeMap::new(),
            key: Some(key.to_vec()),
            protection: "os_keyring".into(),
            keyring_allowed: true,
            raw_hash: String::new(),
            oauth_fail_after: None,
        };
        store.load_if_changed()?;
        Ok(store)
    }
    fn load_if_changed(&mut self) -> AppResult<()> {
        match util::read_bounded(&self.path, 8 * 1024 * 1024) {
            Ok(bytes) => {
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    if std::fs::metadata(&self.path)?.permissions().mode() & 0o777 != 0o600 {
                        std::fs::set_permissions(
                            &self.path,
                            std::fs::Permissions::from_mode(0o600),
                        )?;
                    }
                }
                let next_hash = util::hash(&bytes);
                if !self.raw_hash.is_empty() && self.raw_hash == next_hash {
                    return Ok(());
                }
                let value: serde_json::Value = serde_json::from_slice(&bytes)?;
                if value.get("version").is_some() {
                    let file: VaultFile = serde_json::from_value(value)?;
                    if file.version != 1 {
                        return Err(AppError::api("data_invalid", 400));
                    }
                    if file.protection == "os_keyring" {
                        let nonce = STANDARD
                            .decode(file.nonce)
                            .map_err(|_| AppError::api("data_invalid", 400))?;
                        if nonce.len() != 12 {
                            return Err(AppError::api("data_invalid", 400));
                        }
                        let ciphertext = STANDARD
                            .decode(file.ciphertext)
                            .map_err(|_| AppError::api("data_invalid", 400))?;
                        let decrypt_with = |key_bytes: &[u8]| -> AppResult<Vec<u8>> {
                            let cipher = Aes256Gcm::new_from_slice(key_bytes)
                                .map_err(|_| AppError::api("credentials_invalid", 401))?;
                            cipher
                                .decrypt(Nonce::from_slice(&nonce), ciphertext.as_ref())
                                .map_err(|_| AppError::api("credentials_invalid", 401))
                        };
                        let (plaintext, active_key) = match &self.key {
                            Some(cached_key) if self.keyring_allowed => {
                                match decrypt_with(cached_key) {
                                    Ok(plain) => (plain, cached_key.clone()),
                                    Err(_) => {
                                        let fresh = self.existing_key()?;
                                        (decrypt_with(&fresh)?, fresh)
                                    }
                                }
                            }
                            _ => {
                                let fresh = self.existing_key()?;
                                (decrypt_with(&fresh)?, fresh)
                            }
                        };
                        self.data = serde_json::from_slice(&plaintext)?;
                        self.key = Some(active_key);
                        self.protection = "os_keyring".into();
                    } else if file.protection == "file_permissions_only" {
                        self.data = file.values;
                        self.key = None;
                        self.protection = "file_permissions_only".into();
                    } else {
                        return Err(AppError::api("data_invalid", 400));
                    }
                } else {
                    // Upgrade the original CLI's private store on its next write.
                    self.data = serde_json::from_value(value)?;
                    self.key = None;
                    self.protection = "file_permissions_only".into();
                }
                self.raw_hash = next_hash;
            }
            Err(error) if !self.path.exists() => {
                let _ = error;
                self.data.clear();
                self.raw_hash.clear();
            }
            Err(error) => return Err(error),
        }
        Ok(())
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
        self.load_if_changed()
    }
    pub(crate) fn get(&self, site: &str, key: &str) -> Option<&str> {
        self.data.get(&format!("{site}/{key}")).map(String::as_str)
    }
    pub(crate) fn entries(&self) -> &BTreeMap<String, String> {
        &self.data
    }
    pub fn contains(&self, site: &str, key: &str) -> bool {
        self.data.contains_key(&format!("{site}/{key}"))
    }
    pub fn protection(&self) -> &str {
        &self.protection
    }
    pub fn keys(&self) -> Vec<String> {
        self.data
            .iter()
            .filter_map(|(key, value)| {
                if key.starts_with("@oauth/") {
                    crate::runtime::oauth::credential_ref(value)
                } else if key.starts_with("@pagination/")
                    || key.starts_with("@read-observation/")
                    || key.starts_with("@capture/")
                {
                    None
                } else {
                    Some(key.clone())
                }
            })
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .collect()
    }
    pub fn set(&mut self, site: &str, key: &str, value: &str) -> AppResult<()> {
        if site == "@oauth" || site.is_empty() || key.is_empty() || value.len() > 2 * 1024 * 1024 {
            return Err(AppError::api("bad_request", 400));
        }
        let _lock = util::file_lock(&self.path.with_extension("vault-lock"), false)?;
        self.reload()?;
        let name = format!("{site}/{key}");
        let snapshot = self.data.clone();
        if self.data.get(&name).map(String::as_str) != Some(value) {
            self.invalidate_pagination(site, key);
            if site == "accounts" {
                self.data.remove(&format!("@capture/{key}"));
            }
        }
        self.data.insert(name.clone(), value.into());
        if let Err(error) = self.save() {
            self.data = snapshot;
            return Err(error);
        }
        Ok(())
    }
    pub fn delete(&mut self, site: &str, key: &str) -> AppResult<()> {
        self.delete_website_credential(site, key, None, None)
    }
    pub(crate) fn delete_website_credential(
        &mut self,
        site: &str,
        key: &str,
        current_oauth_key: Option<&str>,
        legacy_key: Option<&str>,
    ) -> AppResult<()> {
        if site == "@oauth" {
            return Err(AppError::api("bad_request", 400));
        }
        let _lock = util::file_lock(&self.path.with_extension("vault-lock"), false)?;
        self.reload()?;
        let name = format!("{site}/{key}");
        let snapshot = self.data.clone();
        let current_oauth_name = current_oauth_key.map(|key| format!("@oauth/{key}"));
        let legacy_name = legacy_key.map(|key| format!("{site}/{key}"));
        self.data.retain(|stored_key, value| {
            stored_key != &name
                && !(site == "accounts" && stored_key == &format!("@capture/{key}"))
                && legacy_name.as_ref() != Some(stored_key)
                && !(current_oauth_name.as_ref() == Some(stored_key)
                    && crate::runtime::oauth::credential_ref(value).is_none())
                && !(stored_key.starts_with("@oauth/")
                    && crate::runtime::oauth::credential_ref(value).as_ref() == Some(&name))
        });
        if self.data != snapshot {
            self.invalidate_pagination(site, key);
        }
        if let Err(error) = self.save() {
            self.data = snapshot;
            return Err(error);
        }
        Ok(())
    }
    fn invalidate_pagination(&mut self, site: &str, key: &str) {
        self.data.retain(|name, value| {
            if !name.starts_with("@pagination/") {
                return true;
            }
            let Ok(record) = serde_json::from_str::<serde_json::Value>(value) else {
                return false;
            };
            if site == "accounts" {
                record["account"].as_str() != Some(key)
            } else {
                record["site"].as_str() != Some(site)
            }
        });
    }
    /// One cross-process transaction for private continuation records and pruning.
    pub(crate) fn pagination_transaction<T>(
        &mut self,
        f: impl FnOnce(&mut BTreeMap<String, String>) -> AppResult<T>,
    ) -> AppResult<T> {
        let _lock = util::file_lock(&self.path.with_extension("vault-lock"), false)?;
        self.reload()?;
        let previous = self.data.clone();
        let mut entries: BTreeMap<_, _> = self
            .data
            .iter()
            .filter(|(k, _)| k.starts_with("@pagination/"))
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        let result = f(&mut entries)?;
        self.data.retain(|k, _| !k.starts_with("@pagination/"));
        self.data.extend(entries);
        if self.data != previous {
            if let Err(error) = self.save() {
                self.data = previous;
                return Err(error);
            }
        }
        Ok(result)
    }

    pub(crate) fn oauth_replace(
        &mut self,
        key: &str,
        expected: Option<&str>,
        replacement: Option<&str>,
    ) -> AppResult<()> {
        let _lock = util::file_lock(&self.path.with_extension("vault-lock"), false)?;
        self.reload()?;
        if self.get("@oauth", key) != expected {
            return Err(AppError::api("conflict", 409));
        }
        #[cfg(test)]
        let fault = match self.oauth_fail_after {
            Some((0, after)) => {
                self.oauth_fail_after = None;
                Some(after)
            }
            Some((remaining, after)) => {
                self.oauth_fail_after = Some((remaining - 1, after));
                None
            }
            None => None,
        };
        #[cfg(test)]
        if fault == Some(false) {
            return Err(AppError::api("internal", 500));
        }
        let name = format!("@oauth/{key}");
        match replacement {
            Some(value) => {
                self.data.insert(name, value.into());
            }
            None => {
                self.data.remove(&name);
            }
        }
        let committed = self.save();
        #[cfg(test)]
        let committed = committed.and_then(|()| {
            if fault == Some(true) {
                Err(AppError::api("internal", 500))
            } else {
                Ok(())
            }
        });
        if let Err(error) = committed {
            // Rename can succeed before directory sync fails. Disk, not the old
            // in-memory snapshot, owns the outcome of an uncertain commit.
            self.raw_hash.clear();
            self.reload()?;
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
        let bytes = serde_json::to_vec(&file)?;
        util::atomic_file(&self.path, &bytes)?;
        self.raw_hash = util::hash(&bytes);
        self.protection = file.protection;
        Ok(())
    }
}
