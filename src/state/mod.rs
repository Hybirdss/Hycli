mod migration;
mod models;
mod queries;
mod recovery;
mod schema;

use crate::{apperr::AppResult, util};
use rusqlite::Connection;
use std::{
    path::{Path, PathBuf},
    sync::Mutex,
    time::Duration,
};

pub use models::*;

pub struct StateStore {
    pub(super) legacy_path: PathBuf,
    pub(super) db_path: PathBuf,
    pub(super) conn: Mutex<Connection>,
}

impl StateStore {
    pub fn open(path: &Path) -> AppResult<Self> {
        Self::open_with_recovery(path, true)
    }

    pub fn open_with_recovery(path: &Path, recover: bool) -> AppResult<Self> {
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                util::private_dir(parent)?;
            }
        }
        let db_path = path.with_extension("db");
        let legacy_path = if path.extension().is_some_and(|ext| ext == "json") {
            path.to_owned()
        } else {
            path.with_extension("json")
        };
        let conn = Connection::open(&db_path)?;
        conn.busy_timeout(Duration::from_secs(5))?;
        #[cfg(unix)]
        if db_path.exists() {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&db_path, std::fs::Permissions::from_mode(0o600));
        }
        schema::init_schema(&conn)?;
        let store = Self {
            legacy_path,
            db_path,
            conn: Mutex::new(conn),
        };
        store.migrate_legacy_json_if_needed()?;
        store.recover_interrupted_jobs(recover)?;
        Ok(store)
    }

    pub fn db_path(&self) -> &Path {
        &self.db_path
    }
}
