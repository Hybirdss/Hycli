use crate::{
    apperr::{AppError, AppResult},
    util,
};
use rusqlite::{OptionalExtension, TransactionBehavior};
use serde_json::Value;
use std::path::{Path, PathBuf};

use super::{
    StateStore,
    models::{Data, MAX_PAYLOAD_BYTES},
};

impl StateStore {
    pub(super) fn migrate_legacy_json_if_needed(&self) -> AppResult<()> {
        let mut conn = self
            .conn
            .lock()
            .map_err(|_| AppError::api("internal", 500))?;
        let already: Option<String> = conn
            .query_row(
                "SELECT value FROM state_meta WHERE key = 'migrated_from_json'",
                [],
                |row| row.get(0),
            )
            .optional()?;
        if already.as_deref() == Some("1") {
            if self.legacy_path.exists() {
                let migrated_backup = self.legacy_path.with_extension("json.migrated");
                let _ = std::fs::rename(&self.legacy_path, migrated_backup);
            }
            return Ok(());
        }
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let already_in_tx: Option<String> = tx
            .query_row(
                "SELECT value FROM state_meta WHERE key = 'migrated_from_json'",
                [],
                |row| row.get(0),
            )
            .optional()?;
        if already_in_tx.as_deref() == Some("1") {
            if self.legacy_path.exists() {
                let migrated_backup = self.legacy_path.with_extension("json.migrated");
                let _ = std::fs::rename(&self.legacy_path, migrated_backup);
            }
            return Ok(());
        }
        let mut migrated_results = Vec::new();
        let migrated_json = self.legacy_path.exists();
        if migrated_json {
            let bytes = util::read_bounded(&self.legacy_path, MAX_PAYLOAD_BYTES)?;
            let mut legacy: Data = serde_json::from_slice(&bytes)?;
            // Imported old state contains references, never new selection authority.
            for site in legacy.sites.values_mut() {
                site.account_binding = None;
                site.account_selection_revision.clear();
            }
            for job in &mut legacy.jobs {
                if job.result.is_none() {
                    if let Ok(result_file) = Self::legacy_result_path(&self.legacy_path, &job.id) {
                        if result_file.exists() {
                            if let Ok(raw) = util::read_bounded(&result_file, MAX_PAYLOAD_BYTES) {
                                if let Ok(val) = serde_json::from_slice::<Value>(&raw) {
                                    job.result = Some(val);
                                    migrated_results.push(result_file);
                                }
                            }
                        }
                    }
                }
            }
            let current = Self::load_data(&tx)?;
            Self::save_diff(&tx, &current, &legacy)?;
        }
        tx.execute(
            "INSERT OR REPLACE INTO state_meta (key, value) VALUES ('migrated_from_json', '1')",
            [],
        )?;
        tx.commit()?;
        for result_file in migrated_results {
            let _ = std::fs::remove_file(result_file);
        }
        if migrated_json {
            let migrated_backup = self.legacy_path.with_extension("json.migrated");
            let _ = std::fs::rename(&self.legacy_path, migrated_backup);
        }
        Ok(())
    }

    fn legacy_result_path(base_path: &Path, id: &str) -> AppResult<PathBuf> {
        if id.len() != 32
            || !id
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return Err(AppError::api("not_found", 404));
        }
        Ok(base_path
            .parent()
            .unwrap_or(Path::new("."))
            .join("results")
            .join(format!("{id}.json")))
    }
}
