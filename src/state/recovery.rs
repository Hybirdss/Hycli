use crate::{
    apperr::{AppError, AppResult},
    util,
};
use rusqlite::TransactionBehavior;

use super::StateStore;

impl StateStore {
    pub(super) fn recover_interrupted_jobs(&self, recover_all_local: bool) -> AppResult<()> {
        let mut conn = self
            .conn
            .lock()
            .map_err(|_| AppError::api("internal", 500))?;
        let current_pid = std::process::id();
        let candidates: Vec<(String, u32)> = {
            let mut stmt = conn.prepare(
                "SELECT id, owner_pid FROM jobs WHERE status IN ('queued', 'running') ORDER BY ord ASC",
            )?;
            let rows = stmt.query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)? as u32))
            })?;
            rows.collect::<Result<Vec<_>, _>>()?
        };
        let ids: Vec<String> = candidates
            .into_iter()
            .filter_map(|(id, pid)| {
                let should_recover = if recover_all_local {
                    pid == 0 || pid == current_pid || !util::is_pid_alive(pid)
                } else {
                    pid != 0 && pid != current_pid && !util::is_pid_alive(pid)
                };
                should_recover.then_some(id)
            })
            .collect();
        if ids.is_empty() {
            return Ok(());
        }
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        for id in ids {
            if let Ok(mut job) = Self::load_single_job(&tx, &id, false) {
                if !["queued", "running"].contains(&job.status.as_str()) {
                    continue;
                }
                let before_agents = job.agents.clone();
                let before_notes = job.notes.clone();
                job.mark_interrupted("interrupted");
                Self::update_job_row(&tx, &mut job, &before_agents, &before_notes)?;
            }
        }
        tx.commit()?;
        Ok(())
    }
}
