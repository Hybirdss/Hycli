use crate::{
    accounts::AccountView,
    apperr::{AppError, AppResult},
    util,
};
use rusqlite::{Connection, OptionalExtension, Transaction, TransactionBehavior, params};
use std::collections::{BTreeMap, BTreeSet};

use super::{
    StateStore,
    models::{
        Data, Job, JobAgent, JobNote, MAX_JOBS_FINISHED, MAX_JOBS_TOTAL, MAX_PAYLOAD_BYTES,
        ProviderSettings, Settings, SiteMeta, WebsiteUnderstanding,
    },
    schema::{JobRow, SiteRow},
};

impl StateStore {
    pub fn has_job_result(&self, id: &str) -> AppResult<bool> {
        let conn = self
            .conn
            .lock()
            .map_err(|_| AppError::api("internal", 500))?;
        let exists: Option<i64> = conn
            .query_row(
                "SELECT 1 FROM job_results WHERE job_id = ?1",
                params![id],
                |row| row.get(0),
            )
            .optional()?;
        Ok(exists.is_some())
    }

    pub fn mark_job_dispatched(&self, id: &str) -> AppResult<()> {
        let conn = self
            .conn
            .lock()
            .map_err(|_| AppError::api("internal", 500))?;
        conn.execute(
            "UPDATE jobs SET dispatch_phase = 'dispatched', updated_at = ?2 WHERE id = ?1",
            params![id, util::now()],
        )?;
        Ok(())
    }

    pub fn settings(&self) -> AppResult<Settings> {
        let conn = self
            .conn
            .lock()
            .map_err(|_| AppError::api("internal", 500))?;
        Self::load_settings(&conn)
    }

    pub fn site(&self, id: &str) -> AppResult<Option<SiteMeta>> {
        let conn = self
            .conn
            .lock()
            .map_err(|_| AppError::api("internal", 500))?;
        let row: Option<SiteRow> = conn
            .query_row(
                "SELECT workflows_complete, pinned, title, created_at, updated_at, last_used,
                        account_id, descriptions_json, verified_json, identity_paths_json,
                        observations_json, connection_recipe_json, preparation_json,
                        icon_type, icon_source, writes, smoke_json
                 FROM sites WHERE id = ?1",
                params![id],
                |r| SiteRow::from_row(r, 0),
            )
            .optional()?;
        row.map(SiteRow::into_meta).transpose()
    }

    pub fn site_writes(&self, id: &str) -> AppResult<bool> {
        let conn = self
            .conn
            .lock()
            .map_err(|_| AppError::api("internal", 500))?;
        let writes: Option<i64> = conn
            .query_row("SELECT writes FROM sites WHERE id = ?1", params![id], |r| {
                r.get(0)
            })
            .optional()?;
        Ok(writes.unwrap_or(0) != 0)
    }

    pub fn account(&self, id: &str) -> AppResult<Option<AccountView>> {
        let conn = self
            .conn
            .lock()
            .map_err(|_| AppError::api("internal", 500))?;
        let data_json: Option<String> = conn
            .query_row(
                "SELECT data_json FROM accounts WHERE id = ?1",
                params![id],
                |r| r.get(0),
            )
            .optional()?;
        data_json
            .as_deref()
            .map(serde_json::from_str)
            .transpose()
            .map_err(Into::into)
    }

    pub fn accounts(&self) -> AppResult<BTreeMap<String, AccountView>> {
        let conn = self
            .conn
            .lock()
            .map_err(|_| AppError::api("internal", 500))?;
        Self::load_accounts(&conn)
    }

    pub fn latest_understanding(&self, site_id: &str) -> AppResult<Option<WebsiteUnderstanding>> {
        let conn = self
            .conn
            .lock()
            .map_err(|_| AppError::api("internal", 500))?;
        let raw: Option<String> = conn
            .query_row(
                "SELECT understanding_json FROM jobs
                 WHERE site_id = ?1 AND understanding_json IS NOT NULL
                 ORDER BY ord ASC, rowid DESC
                 LIMIT 1",
                params![site_id],
                |r| r.get(0),
            )
            .optional()?;
        raw.as_deref()
            .map(serde_json::from_str)
            .transpose()
            .map_err(Into::into)
    }

    pub fn read(&self) -> AppResult<Data> {
        let mut conn = self
            .conn
            .lock()
            .map_err(|_| AppError::api("internal", 500))?;
        let tx = conn.transaction_with_behavior(TransactionBehavior::Deferred)?;
        Self::load_data(&tx)
    }

    pub fn update<T>(&self, apply: impl FnOnce(&mut Data) -> AppResult<T>) -> AppResult<T> {
        let mut conn = self
            .conn
            .lock()
            .map_err(|_| AppError::api("internal", 500))?;
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let before = Self::load_data(&tx)?;
        let mut next = before.clone();
        let result = apply(&mut next)?;
        if next.jobs.len() > MAX_JOBS_TOTAL {
            let mut finished = 0;
            next.jobs.retain(|j| {
                if ["running", "queued"].contains(&j.status.as_str()) {
                    true
                } else {
                    finished += 1;
                    finished <= MAX_JOBS_FINISHED
                }
            });
        }
        Self::save_diff(&tx, &before, &next)?;
        tx.commit()?;
        Ok(result)
    }

    pub fn add_job(&self, mut job: Job) -> AppResult<()> {
        let mut conn = self
            .conn
            .lock()
            .map_err(|_| AppError::api("internal", 500))?;
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let duplicate = match job.kind.as_str() {
            "prepare" => tx
                .query_row(
                    "SELECT 1 FROM jobs
                     WHERE status IN ('running', 'queued') AND kind = 'prepare' AND site_id = ?1
                     LIMIT 1",
                    params![job.site_id],
                    |r| r.get::<_, i64>(0),
                )
                .optional()?
                .is_some(),
            "describe" => tx
                .query_row(
                    "SELECT 1 FROM jobs
                     WHERE status IN ('running', 'queued') AND kind = 'describe'
                       AND site_id = ?1 AND locale = ?2
                     LIMIT 1",
                    params![job.site_id, job.locale],
                    |r| r.get::<_, i64>(0),
                )
                .optional()?
                .is_some(),
            "summary" => tx
                .query_row(
                    "SELECT 1 FROM jobs
                     WHERE status IN ('running', 'queued') AND kind = 'summary' AND parent_id = ?1
                     LIMIT 1",
                    params![job.parent_id],
                    |r| r.get::<_, i64>(0),
                )
                .optional()?
                .is_some(),
            _ => false,
        };
        if duplicate {
            return Err(AppError::api("job_running", 409));
        }
        let next_ord: i64 =
            tx.query_row("SELECT COALESCE(MIN(ord) - 1, 0) FROM jobs", [], |r| {
                r.get(0)
            })?;
        Self::insert_or_replace_job(&tx, next_ord, &mut job)?;
        Self::trim_excess_jobs(&tx)?;
        tx.commit()?;
        Ok(())
    }

    pub fn job(&self, id: &str) -> AppResult<Job> {
        let mut conn = self
            .conn
            .lock()
            .map_err(|_| AppError::api("internal", 500))?;
        let tx = conn.transaction_with_behavior(TransactionBehavior::Deferred)?;
        Self::load_single_job(&tx, id, true)
    }

    pub fn update_job(&self, id: &str, apply: impl FnOnce(&mut Job)) -> AppResult<()> {
        let mut conn = self
            .conn
            .lock()
            .map_err(|_| AppError::api("internal", 500))?;
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let mut job = Self::load_single_job(&tx, id, false)?;
        let before_agents = job.agents.clone();
        let before_notes = job.notes.clone();
        apply(&mut job);
        job.updated_at = util::now();
        Self::update_job_row(&tx, &mut job, &before_agents, &before_notes)?;
        tx.commit()?;
        Ok(())
    }

    pub(super) fn load_settings(conn: &Connection) -> AppResult<Settings> {
        let settings = conn
            .query_row(
                "SELECT locale, theme, reduce_motion, notifications, default_provider, engine_path
                 FROM settings WHERE id = 1",
                [],
                |r| {
                    Ok(Settings {
                        locale: r.get(0)?,
                        theme: r.get(1)?,
                        reduce_motion: r.get::<_, i64>(2)? != 0,
                        notifications: r.get::<_, i64>(3)? != 0,
                        default_provider: r.get(4)?,
                        engine_path: r.get(5)?,
                    })
                },
            )
            .optional()?
            .unwrap_or_default();
        Ok(settings)
    }

    pub(super) fn load_accounts(conn: &Connection) -> AppResult<BTreeMap<String, AccountView>> {
        let mut accounts = BTreeMap::new();
        let mut stmt = conn.prepare("SELECT id, data_json FROM accounts ORDER BY id ASC")?;
        let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
        for row in rows {
            let (id, data_json) = row?;
            accounts.insert(id, serde_json::from_str(&data_json)?);
        }
        Ok(accounts)
    }

    pub(super) fn load_data(tx: &Transaction<'_>) -> AppResult<Data> {
        let settings = Self::load_settings(tx)?;

        let mut providers = BTreeMap::new();
        {
            let mut stmt = tx.prepare(
                "SELECT id, model, connected, models_json, checked_at FROM providers ORDER BY id ASC",
            )?;
            let rows = stmt.query_map([], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, i64>(2)? != 0,
                    r.get::<_, String>(3)?,
                    r.get::<_, String>(4)?,
                ))
            })?;
            for row in rows {
                let (id, model, connected, models_json, checked_at) = row?;
                providers.insert(
                    id,
                    ProviderSettings {
                        model,
                        connected,
                        models: serde_json::from_str(&models_json)?,
                        checked_at,
                    },
                );
            }
        }

        let mut sites = BTreeMap::new();
        {
            let mut stmt = tx.prepare(
                "SELECT id, workflows_complete, pinned, title, created_at, updated_at, last_used,
                        account_id, descriptions_json, verified_json, identity_paths_json,
                        observations_json, connection_recipe_json, preparation_json,
                        icon_type, icon_source, writes, smoke_json
                 FROM sites ORDER BY id ASC",
            )?;
            let rows = stmt.query_map([], |r| {
                Ok((r.get::<_, String>(0)?, SiteRow::from_row(r, 1)?))
            })?;
            for row in rows {
                let (id, site_row) = row?;
                sites.insert(id, site_row.into_meta()?);
            }
        }

        let accounts = Self::load_accounts(tx)?;

        let mut agents_by_job: BTreeMap<String, Vec<JobAgent>> = BTreeMap::new();
        {
            let mut stmt = tx.prepare(
                "SELECT job_id, id, status, found, started_at, finished_at
                 FROM job_agents ORDER BY job_id ASC, idx ASC",
            )?;
            let rows = stmt.query_map([], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    JobAgent {
                        id: r.get(1)?,
                        status: r.get(2)?,
                        found: r.get::<_, i64>(3)? as usize,
                        started_at: r.get(4)?,
                        finished_at: r.get(5)?,
                    },
                ))
            })?;
            for row in rows {
                let (job_id, agent) = row?;
                agents_by_job.entry(job_id).or_default().push(agent);
            }
        }

        let mut notes_by_job: BTreeMap<String, Vec<JobNote>> = BTreeMap::new();
        {
            let mut stmt = tx.prepare(
                "SELECT job_id, seq, at, code, message, agent, count
                 FROM job_notes ORDER BY job_id ASC, seq ASC",
            )?;
            let rows = stmt.query_map([], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    JobNote {
                        seq: r.get::<_, i64>(1)? as u64,
                        at: r.get(2)?,
                        code: r.get(3)?,
                        message: r.get(4)?,
                        agent: r.get(5)?,
                        count: r.get::<_, i64>(6)? as usize,
                    },
                ))
            })?;
            for row in rows {
                let (job_id, note) = row?;
                notes_by_job.entry(job_id).or_default().push(note);
            }
        }

        let mut jobs = Vec::new();
        {
            let mut stmt = tx.prepare(
                "SELECT id, kind, site_id, site_title, url, intent, understanding_json,
                        action_id, action_title, provider, locale, account_id, status, phase,
                        error_code, found_json, started_at, updated_at, finished_at, summary,
                        approval_id, progress_done, progress_total, parent_id,
                        effect, dispatch_phase, owner_pid
                 FROM jobs ORDER BY ord ASC, rowid DESC",
            )?;
            let rows = stmt.query_map([], JobRow::from_row)?;
            for row in rows {
                let job_row = row?;
                let agents = agents_by_job.remove(&job_row.id).unwrap_or_default();
                let notes = notes_by_job.remove(&job_row.id).unwrap_or_default();
                jobs.push(job_row.into_job(agents, notes, None)?);
            }
        }

        Ok(Data {
            settings,
            providers,
            sites,
            accounts,
            jobs,
        })
    }

    pub(super) fn load_single_job(
        tx: &Transaction<'_>,
        id: &str,
        with_result: bool,
    ) -> AppResult<Job> {
        let job_row = tx
            .query_row(
                "SELECT id, kind, site_id, site_title, url, intent, understanding_json,
                        action_id, action_title, provider, locale, account_id, status, phase,
                        error_code, found_json, started_at, updated_at, finished_at, summary,
                        approval_id, progress_done, progress_total, parent_id,
                        effect, dispatch_phase, owner_pid
                 FROM jobs WHERE id = ?1",
                params![id],
                JobRow::from_row,
            )
            .optional()?
            .ok_or_else(|| AppError::api("not_found", 404))?;

        let mut agents = Vec::new();
        {
            let mut stmt = tx.prepare(
                "SELECT id, status, found, started_at, finished_at
                 FROM job_agents WHERE job_id = ?1 ORDER BY idx ASC",
            )?;
            let rows = stmt.query_map(params![id], |r| {
                Ok(JobAgent {
                    id: r.get(0)?,
                    status: r.get(1)?,
                    found: r.get::<_, i64>(2)? as usize,
                    started_at: r.get(3)?,
                    finished_at: r.get(4)?,
                })
            })?;
            for agent in rows {
                agents.push(agent?);
            }
        }

        let mut notes = Vec::new();
        {
            let mut stmt = tx.prepare(
                "SELECT seq, at, code, message, agent, count
                 FROM job_notes WHERE job_id = ?1 ORDER BY seq ASC",
            )?;
            let rows = stmt.query_map(params![id], |r| {
                Ok(JobNote {
                    seq: r.get::<_, i64>(0)? as u64,
                    at: r.get(1)?,
                    code: r.get(2)?,
                    message: r.get(3)?,
                    agent: r.get(4)?,
                    count: r.get::<_, i64>(5)? as usize,
                })
            })?;
            for note in rows {
                notes.push(note?);
            }
        }

        let result = if with_result {
            let raw: Option<Vec<u8>> = tx
                .query_row(
                    "SELECT result_json FROM job_results WHERE job_id = ?1",
                    params![id],
                    |r| r.get(0),
                )
                .optional()?;
            match raw {
                Some(bytes) => Some(serde_json::from_slice(&bytes)?),
                None => None,
            }
        } else {
            None
        };

        job_row.into_job(agents, notes, result)
    }

    pub(super) fn save_diff(tx: &Transaction<'_>, before: &Data, next: &Data) -> AppResult<()> {
        if next.settings != before.settings {
            tx.execute(
                "INSERT INTO settings (id, locale, theme, reduce_motion, notifications, default_provider, engine_path)
                 VALUES (1, ?1, ?2, ?3, ?4, ?5, ?6)
                 ON CONFLICT(id) DO UPDATE SET
                     locale = excluded.locale,
                     theme = excluded.theme,
                     reduce_motion = excluded.reduce_motion,
                     notifications = excluded.notifications,
                     default_provider = excluded.default_provider,
                     engine_path = excluded.engine_path",
                params![
                    next.settings.locale,
                    next.settings.theme,
                    i64::from(next.settings.reduce_motion),
                    i64::from(next.settings.notifications),
                    next.settings.default_provider,
                    next.settings.engine_path,
                ],
            )?;
        }

        for id in before.providers.keys() {
            if !next.providers.contains_key(id) {
                tx.execute("DELETE FROM providers WHERE id = ?1", params![id])?;
            }
        }
        for (id, provider) in &next.providers {
            if before.providers.get(id) != Some(provider) {
                tx.execute(
                    "INSERT INTO providers (id, model, connected, models_json, checked_at)
                     VALUES (?1, ?2, ?3, ?4, ?5)
                     ON CONFLICT(id) DO UPDATE SET
                         model = excluded.model,
                         connected = excluded.connected,
                         models_json = excluded.models_json,
                         checked_at = excluded.checked_at",
                    params![
                        id,
                        provider.model,
                        i64::from(provider.connected),
                        serde_json::to_string(&provider.models)?,
                        provider.checked_at,
                    ],
                )?;
            }
        }

        for id in before.sites.keys() {
            if !next.sites.contains_key(id) {
                tx.execute("DELETE FROM sites WHERE id = ?1", params![id])?;
            }
        }
        for (id, meta) in &next.sites {
            let new_row = SiteRow::from_meta(meta)?;
            let changed = match before.sites.get(id) {
                Some(old_meta) => SiteRow::from_meta(old_meta)? != new_row,
                None => true,
            };
            if changed {
                tx.execute(
                    "INSERT INTO sites (
                         id, workflows_complete, pinned, title, created_at, updated_at, last_used,
                         account_id, descriptions_json, verified_json, identity_paths_json,
                         observations_json, connection_recipe_json, preparation_json,
                         icon_type, icon_source, writes, smoke_json
                     ) VALUES (
                         ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18
                     )
                     ON CONFLICT(id) DO UPDATE SET
                         workflows_complete = excluded.workflows_complete,
                         pinned = excluded.pinned,
                         title = excluded.title,
                         created_at = excluded.created_at,
                         updated_at = excluded.updated_at,
                         last_used = excluded.last_used,
                         account_id = excluded.account_id,
                         descriptions_json = excluded.descriptions_json,
                         verified_json = excluded.verified_json,
                         identity_paths_json = excluded.identity_paths_json,
                         observations_json = excluded.observations_json,
                         connection_recipe_json = excluded.connection_recipe_json,
                         preparation_json = excluded.preparation_json,
                         icon_type = excluded.icon_type,
                         icon_source = excluded.icon_source,
                         writes = excluded.writes,
                         smoke_json = excluded.smoke_json",
                    params![
                        id,
                        new_row.workflows_complete,
                        new_row.pinned,
                        new_row.title,
                        new_row.created_at,
                        new_row.updated_at,
                        new_row.last_used,
                        new_row.account_id,
                        new_row.descriptions_json,
                        new_row.verified_json,
                        new_row.identity_paths_json,
                        new_row.observations_json,
                        new_row.connection_recipe_json,
                        new_row.preparation_json,
                        new_row.icon_type,
                        new_row.icon_source,
                        new_row.writes,
                        new_row.smoke_json,
                    ],
                )?;
            }
        }

        for id in before.accounts.keys() {
            if !next.accounts.contains_key(id) {
                tx.execute("DELETE FROM accounts WHERE id = ?1", params![id])?;
            }
        }
        for (id, account) in &next.accounts {
            let new_json = serde_json::to_string(account)?;
            let changed = match before.accounts.get(id) {
                Some(old_acc) => serde_json::to_string(old_acc)? != new_json,
                None => true,
            };
            if changed {
                tx.execute(
                    "INSERT INTO accounts (id, data_json) VALUES (?1, ?2)
                     ON CONFLICT(id) DO UPDATE SET data_json = excluded.data_json",
                    params![id, new_json],
                )?;
            }
        }

        let next_ids: BTreeSet<&str> = next.jobs.iter().map(|j| j.id.as_str()).collect();
        for old_job in &before.jobs {
            if !next_ids.contains(old_job.id.as_str()) {
                tx.execute("DELETE FROM jobs WHERE id = ?1", params![old_job.id])?;
            }
        }

        let before_map: BTreeMap<&str, (usize, &Job)> = before
            .jobs
            .iter()
            .enumerate()
            .map(|(idx, j)| (j.id.as_str(), (idx, j)))
            .collect();

        for (idx, job) in next.jobs.iter().enumerate() {
            let ord = idx as i64;
            match before_map.get(job.id.as_str()) {
                Some((old_idx, old_job)) if *old_idx == idx && *old_job == job => {}
                Some((_, old_job)) if *old_job == job => {
                    tx.execute(
                        "UPDATE jobs SET ord = ?1 WHERE id = ?2",
                        params![ord, job.id],
                    )?;
                }
                Some((_, old_job)) => {
                    let mut cloned = job.clone();
                    Self::upsert_job_with_ord(
                        tx,
                        ord,
                        &mut cloned,
                        Some(&old_job.agents),
                        Some(&old_job.notes),
                    )?;
                }
                None => {
                    let mut cloned = job.clone();
                    Self::insert_or_replace_job(tx, ord, &mut cloned)?;
                }
            }
        }
        Ok(())
    }

    fn insert_or_replace_job(tx: &Transaction<'_>, ord: i64, job: &mut Job) -> AppResult<()> {
        Self::upsert_job_with_ord(tx, ord, job, None, None)
    }

    pub(super) fn update_job_row(
        tx: &Transaction<'_>,
        job: &mut Job,
        before_agents: &[JobAgent],
        before_notes: &[JobNote],
    ) -> AppResult<()> {
        let ord: i64 =
            tx.query_row("SELECT ord FROM jobs WHERE id = ?1", params![job.id], |r| {
                r.get(0)
            })?;
        Self::upsert_job_with_ord(tx, ord, job, Some(before_agents), Some(before_notes))
    }

    fn upsert_job_with_ord(
        tx: &Transaction<'_>,
        ord: i64,
        job: &mut Job,
        before_agents: Option<&[JobAgent]>,
        before_notes: Option<&[JobNote]>,
    ) -> AppResult<()> {
        let pending_result = job.result.take();
        let understanding_json = job
            .understanding
            .as_ref()
            .map(serde_json::to_string)
            .transpose()?;
        let found_json = serde_json::to_string(&job.found)?;
        let row_bytes = understanding_json.as_ref().map_or(0, String::len)
            + found_json.len()
            + job.summary.len()
            + job.intent.len();
        if row_bytes > MAX_PAYLOAD_BYTES {
            return Err(AppError::api("too_large", 413));
        }

        let effect_str = match job.effect {
            crate::policy::Effect::Read => "read",
            crate::policy::Effect::Write => "write",
            crate::policy::Effect::Unknown => "unknown",
        };
        tx.execute(
            "INSERT INTO jobs (
                 ord, id, kind, site_id, site_title, url, intent, understanding_json,
                 action_id, action_title, provider, locale, account_id, status, phase,
                 error_code, found_json, started_at, updated_at, finished_at, summary,
                 approval_id, progress_done, progress_total, parent_id,
                 effect, dispatch_phase, owner_pid
             ) VALUES (
                 ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15,
                 ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24, ?25,
                 ?26, ?27, ?28
             )
             ON CONFLICT(id) DO UPDATE SET
                 ord = excluded.ord,
                 kind = excluded.kind,
                 site_id = excluded.site_id,
                 site_title = excluded.site_title,
                 url = excluded.url,
                 intent = excluded.intent,
                 understanding_json = excluded.understanding_json,
                 action_id = excluded.action_id,
                 action_title = excluded.action_title,
                 provider = excluded.provider,
                 locale = excluded.locale,
                 account_id = excluded.account_id,
                 status = excluded.status,
                 phase = excluded.phase,
                 error_code = excluded.error_code,
                 found_json = excluded.found_json,
                 started_at = excluded.started_at,
                 updated_at = excluded.updated_at,
                 finished_at = excluded.finished_at,
                 summary = excluded.summary,
                 approval_id = excluded.approval_id,
                 progress_done = excluded.progress_done,
                 progress_total = excluded.progress_total,
                 parent_id = excluded.parent_id,
                 effect = excluded.effect,
                 dispatch_phase = CASE
                     WHEN jobs.dispatch_phase = 'dispatched' THEN 'dispatched'
                     ELSE excluded.dispatch_phase
                 END,
                 owner_pid = CASE
                     WHEN excluded.owner_pid != 0 THEN excluded.owner_pid
                     ELSE jobs.owner_pid
                 END",
            params![
                ord,
                job.id,
                job.kind,
                job.site_id,
                job.site_title,
                job.url,
                job.intent,
                understanding_json,
                job.action_id,
                job.action_title,
                job.provider,
                job.locale,
                job.account_id,
                job.status,
                job.phase,
                job.error_code,
                found_json,
                job.started_at,
                job.updated_at,
                job.finished_at,
                job.summary,
                job.approval_id,
                job.progress_done as i64,
                job.progress_total as i64,
                job.parent_id,
                effect_str,
                job.dispatch_phase.as_str(),
                job.owner_pid as i64,
            ],
        )?;

        if let Some(value) = pending_result {
            let bytes = serde_json::to_vec(&value)?;
            if bytes.len() > MAX_PAYLOAD_BYTES {
                return Err(AppError::api("too_large", 413));
            }
            tx.execute(
                "INSERT INTO job_results (job_id, result_json) VALUES (?1, ?2)
                 ON CONFLICT(job_id) DO UPDATE SET result_json = excluded.result_json",
                params![job.id, bytes],
            )?;
        }

        if before_agents.is_none_or(|old| old != job.agents.as_slice()) {
            tx.execute("DELETE FROM job_agents WHERE job_id = ?1", params![job.id])?;
            for (idx, agent) in job.agents.iter().enumerate() {
                tx.execute(
                    "INSERT INTO job_agents (job_id, idx, id, status, found, started_at, finished_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                    params![
                        job.id,
                        idx as i64,
                        agent.id,
                        agent.status,
                        agent.found as i64,
                        agent.started_at,
                        agent.finished_at,
                    ],
                )?;
            }
        }

        match before_notes {
            Some(old) if old == job.notes.as_slice() => {}
            Some(old)
                if job.notes.len() > old.len()
                    && job.notes[..old.len()] == *old
                    && job.notes.windows(2).all(|w| w[0].seq < w[1].seq) =>
            {
                for note in &job.notes[old.len()..] {
                    tx.execute(
                        "INSERT INTO job_notes (job_id, seq, at, code, message, agent, count)
                         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                        params![
                            job.id,
                            note.seq as i64,
                            note.at,
                            note.code,
                            note.message,
                            note.agent,
                            note.count as i64,
                        ],
                    )?;
                }
            }
            _ => {
                tx.execute("DELETE FROM job_notes WHERE job_id = ?1", params![job.id])?;
                let mut prev_seq = 0u64;
                for note in &job.notes {
                    let seq = if note.seq > prev_seq {
                        note.seq
                    } else {
                        prev_seq + 1
                    };
                    prev_seq = seq;
                    tx.execute(
                        "INSERT INTO job_notes (job_id, seq, at, code, message, agent, count)
                         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                        params![
                            job.id,
                            seq as i64,
                            note.at,
                            note.code,
                            note.message,
                            note.agent,
                            note.count as i64,
                        ],
                    )?;
                }
            }
        }

        Ok(())
    }

    fn trim_excess_jobs(tx: &Transaction<'_>) -> AppResult<()> {
        let total: i64 = tx.query_row("SELECT COUNT(*) FROM jobs", [], |r| r.get(0))?;
        if (total as usize) <= MAX_JOBS_TOTAL {
            return Ok(());
        }
        tx.execute(
            "DELETE FROM jobs WHERE id IN (
                 SELECT id FROM jobs
                 WHERE status NOT IN ('running', 'queued')
                 ORDER BY ord ASC, rowid DESC
                 LIMIT -1 OFFSET ?1
             )",
            params![MAX_JOBS_FINISHED as i64],
        )?;
        Ok(())
    }
}
