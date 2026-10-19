use crate::apperr::AppResult;
use rusqlite::{Connection, Row};
use serde_json::Value;

use super::models::{DispatchPhase, Job, JobAgent, JobNote, SiteMeta};

#[derive(PartialEq, Eq)]
pub(super) struct SiteRow {
    pub(super) workflows_complete: Option<i64>,
    pub(super) pinned: i64,
    pub(super) title: String,
    pub(super) created_at: String,
    pub(super) updated_at: String,
    pub(super) last_used: String,
    pub(super) account_id: String,
    pub(super) descriptions_json: String,
    pub(super) verified_json: String,
    pub(super) identity_paths_json: String,
    pub(super) observations_json: String,
    pub(super) connection_recipe_json: Option<String>,
    pub(super) preparation_json: Option<String>,
    pub(super) icon_type: String,
    pub(super) icon_source: String,
    pub(super) writes: i64,
    pub(super) smoke_json: String,
}
impl SiteRow {
    pub(super) fn from_row(r: &Row<'_>, offset: usize) -> rusqlite::Result<Self> {
        Ok(Self {
            workflows_complete: r.get(offset)?,
            pinned: r.get(offset + 1)?,
            title: r.get(offset + 2)?,
            created_at: r.get(offset + 3)?,
            updated_at: r.get(offset + 4)?,
            last_used: r.get(offset + 5)?,
            account_id: r.get(offset + 6)?,
            descriptions_json: r.get(offset + 7)?,
            verified_json: r.get(offset + 8)?,
            identity_paths_json: r.get(offset + 9)?,
            observations_json: r.get(offset + 10)?,
            connection_recipe_json: r.get(offset + 11)?,
            preparation_json: r.get(offset + 12)?,
            icon_type: r.get(offset + 13)?,
            icon_source: r.get(offset + 14)?,
            writes: r.get(offset + 15)?,
            smoke_json: r.get(offset + 16)?,
        })
    }
    pub(super) fn from_meta(meta: &SiteMeta) -> AppResult<Self> {
        Ok(Self {
            workflows_complete: meta.workflows_complete.map(i64::from),
            pinned: i64::from(meta.pinned),
            title: meta.title.clone(),
            created_at: meta.created_at.clone(),
            updated_at: meta.updated_at.clone(),
            last_used: meta.last_used.clone(),
            account_id: meta.account_id.clone(),
            descriptions_json: serde_json::to_string(&meta.descriptions)?,
            verified_json: serde_json::to_string(&meta.verified)?,
            identity_paths_json: serde_json::to_string(&meta.identity_paths)?,
            observations_json: serde_json::to_string(&meta.observations)?,
            connection_recipe_json: meta
                .connection_recipe
                .as_ref()
                .map(serde_json::to_string)
                .transpose()?,
            preparation_json: meta
                .preparation
                .as_ref()
                .map(serde_json::to_string)
                .transpose()?,
            icon_type: meta.icon_type.clone(),
            icon_source: meta.icon_source.clone(),
            writes: i64::from(meta.writes),
            smoke_json: serde_json::to_string(&meta.smoke)?,
        })
    }
    pub(super) fn into_meta(self) -> AppResult<SiteMeta> {
        Ok(SiteMeta {
            workflows_complete: self.workflows_complete.map(|v| v != 0),
            pinned: self.pinned != 0,
            title: self.title,
            created_at: self.created_at,
            updated_at: self.updated_at,
            last_used: self.last_used,
            account_id: self.account_id,
            descriptions: serde_json::from_str(&self.descriptions_json)?,
            verified: serde_json::from_str(&self.verified_json)?,
            identity_paths: serde_json::from_str(&self.identity_paths_json)?,
            observations: serde_json::from_str(&self.observations_json)?,
            connection_recipe: self
                .connection_recipe_json
                .as_deref()
                .map(serde_json::from_str)
                .transpose()?,
            preparation: self
                .preparation_json
                .as_deref()
                .map(serde_json::from_str)
                .transpose()?,
            icon_type: self.icon_type,
            icon_source: self.icon_source,
            writes: self.writes != 0,
            smoke: serde_json::from_str(&self.smoke_json)?,
        })
    }
}

pub(super) struct JobRow {
    pub(super) id: String,
    pub(super) kind: String,
    pub(super) site_id: String,
    pub(super) site_title: String,
    pub(super) url: String,
    pub(super) intent: String,
    pub(super) understanding_json: Option<String>,
    pub(super) action_id: String,
    pub(super) action_title: String,
    pub(super) provider: String,
    pub(super) locale: String,
    pub(super) account_id: String,
    pub(super) status: String,
    pub(super) phase: String,
    pub(super) error_code: String,
    pub(super) found_json: String,
    pub(super) started_at: String,
    pub(super) updated_at: String,
    pub(super) finished_at: String,
    pub(super) summary: String,
    pub(super) approval_id: String,
    pub(super) progress_done: usize,
    pub(super) progress_total: usize,
    pub(super) parent_id: String,
    pub(super) effect: String,
    pub(super) dispatch_phase: String,
    pub(super) owner_pid: u32,
}
impl JobRow {
    pub(super) fn from_row(r: &Row<'_>) -> rusqlite::Result<Self> {
        Ok(Self {
            id: r.get(0)?,
            kind: r.get(1)?,
            site_id: r.get(2)?,
            site_title: r.get(3)?,
            url: r.get(4)?,
            intent: r.get(5)?,
            understanding_json: r.get(6)?,
            action_id: r.get(7)?,
            action_title: r.get(8)?,
            provider: r.get(9)?,
            locale: r.get(10)?,
            account_id: r.get(11)?,
            status: r.get(12)?,
            phase: r.get(13)?,
            error_code: r.get(14)?,
            found_json: r.get(15)?,
            started_at: r.get(16)?,
            updated_at: r.get(17)?,
            finished_at: r.get(18)?,
            summary: r.get(19)?,
            approval_id: r.get(20)?,
            progress_done: r.get::<_, i64>(21)? as usize,
            progress_total: r.get::<_, i64>(22)? as usize,
            parent_id: r.get(23)?,
            effect: r.get(24)?,
            dispatch_phase: r.get(25)?,
            owner_pid: r.get::<_, i64>(26)? as u32,
        })
    }
    pub(super) fn into_job(
        self,
        agents: Vec<JobAgent>,
        notes: Vec<JobNote>,
        result: Option<Value>,
    ) -> AppResult<Job> {
        let effect = match self.effect.as_str() {
            "write" => crate::policy::Effect::Write,
            "unknown" => crate::policy::Effect::Unknown,
            _ => crate::policy::Effect::Read,
        };
        Ok(Job {
            id: self.id,
            kind: self.kind,
            site_id: self.site_id,
            site_title: self.site_title,
            url: self.url,
            intent: self.intent,
            understanding: self
                .understanding_json
                .as_deref()
                .map(serde_json::from_str)
                .transpose()?,
            action_id: self.action_id,
            action_title: self.action_title,
            provider: self.provider,
            locale: self.locale,
            account_id: self.account_id,
            status: self.status,
            phase: self.phase,
            error_code: self.error_code,
            found: serde_json::from_str(&self.found_json)?,
            started_at: self.started_at,
            updated_at: self.updated_at,
            finished_at: self.finished_at,
            result,
            summary: self.summary,
            approval_id: self.approval_id,
            progress_done: self.progress_done,
            progress_total: self.progress_total,
            agents,
            notes,
            parent_id: self.parent_id,
            effect,
            dispatch_phase: DispatchPhase::from_str(&self.dispatch_phase),
            owner_pid: self.owner_pid,
        })
    }
}

pub(super) fn init_schema(conn: &Connection) -> AppResult<()> {
    conn.execute_batch(
        "PRAGMA journal_mode = WAL;
         PRAGMA synchronous = NORMAL;
         PRAGMA foreign_keys = ON;

         CREATE TABLE IF NOT EXISTS state_meta (
             key TEXT PRIMARY KEY,
             value TEXT NOT NULL
         );

         CREATE TABLE IF NOT EXISTS settings (
             id INTEGER PRIMARY KEY CHECK (id = 1),
             locale TEXT NOT NULL DEFAULT 'en',
             theme TEXT NOT NULL DEFAULT 'light',
             reduce_motion INTEGER NOT NULL DEFAULT 0,
             notifications INTEGER NOT NULL DEFAULT 0,
             default_provider TEXT NOT NULL DEFAULT '',
             engine_path TEXT NOT NULL DEFAULT ''
         );

         CREATE TABLE IF NOT EXISTS providers (
             id TEXT PRIMARY KEY,
             model TEXT NOT NULL DEFAULT '',
             connected INTEGER NOT NULL DEFAULT 0,
             models_json TEXT NOT NULL DEFAULT '[]',
             checked_at TEXT NOT NULL DEFAULT ''
         );

         CREATE TABLE IF NOT EXISTS sites (
             id TEXT PRIMARY KEY,
             workflows_complete INTEGER,
             pinned INTEGER NOT NULL DEFAULT 0,
             title TEXT NOT NULL DEFAULT '',
             created_at TEXT NOT NULL DEFAULT '',
             updated_at TEXT NOT NULL DEFAULT '',
             last_used TEXT NOT NULL DEFAULT '',
             account_id TEXT NOT NULL DEFAULT '',
             descriptions_json TEXT NOT NULL DEFAULT '{}',
             verified_json TEXT NOT NULL DEFAULT '{}',
             identity_paths_json TEXT NOT NULL DEFAULT '[]',
             observations_json TEXT NOT NULL DEFAULT '[]',
             connection_recipe_json TEXT,
             preparation_json TEXT,
             icon_type TEXT NOT NULL DEFAULT '',
             icon_source TEXT NOT NULL DEFAULT '',
             writes INTEGER NOT NULL DEFAULT 0,
             smoke_json TEXT NOT NULL DEFAULT '[]'
         );

         CREATE TABLE IF NOT EXISTS accounts (
             id TEXT PRIMARY KEY,
             data_json TEXT NOT NULL
         );

         CREATE TABLE IF NOT EXISTS jobs (
             ord INTEGER NOT NULL DEFAULT 0,
             id TEXT PRIMARY KEY,
             kind TEXT NOT NULL,
             site_id TEXT NOT NULL DEFAULT '',
             site_title TEXT NOT NULL DEFAULT '',
             url TEXT NOT NULL DEFAULT '',
             intent TEXT NOT NULL DEFAULT '',
             understanding_json TEXT,
             action_id TEXT NOT NULL DEFAULT '',
             action_title TEXT NOT NULL DEFAULT '',
             provider TEXT NOT NULL DEFAULT '',
             locale TEXT NOT NULL DEFAULT 'en',
             account_id TEXT NOT NULL DEFAULT '',
             status TEXT NOT NULL DEFAULT 'queued',
             phase TEXT NOT NULL DEFAULT 'queued',
             error_code TEXT NOT NULL DEFAULT '',
             found_json TEXT NOT NULL DEFAULT '[]',
             started_at TEXT NOT NULL DEFAULT '',
             updated_at TEXT NOT NULL DEFAULT '',
             finished_at TEXT NOT NULL DEFAULT '',
             summary TEXT NOT NULL DEFAULT '',
             approval_id TEXT NOT NULL DEFAULT '',
             progress_done INTEGER NOT NULL DEFAULT 0,
             progress_total INTEGER NOT NULL DEFAULT 5,
             parent_id TEXT NOT NULL DEFAULT '',
             effect TEXT NOT NULL DEFAULT 'read',
             dispatch_phase TEXT NOT NULL DEFAULT 'pending',
             owner_pid INTEGER NOT NULL DEFAULT 0
         );
         CREATE INDEX IF NOT EXISTS idx_jobs_ord ON jobs(ord ASC);
         CREATE INDEX IF NOT EXISTS idx_jobs_status_kind_site
             ON jobs(status, kind, site_id, locale, parent_id);
         CREATE INDEX IF NOT EXISTS idx_jobs_site_status
             ON jobs(site_id, status);

         CREATE TABLE IF NOT EXISTS job_agents (
             job_id TEXT NOT NULL REFERENCES jobs(id) ON DELETE CASCADE,
             idx INTEGER NOT NULL,
             id TEXT NOT NULL,
             status TEXT NOT NULL DEFAULT '',
             found INTEGER NOT NULL DEFAULT 0,
             started_at TEXT NOT NULL DEFAULT '',
             finished_at TEXT NOT NULL DEFAULT '',
             PRIMARY KEY (job_id, idx)
         );

         CREATE TABLE IF NOT EXISTS job_notes (
             job_id TEXT NOT NULL REFERENCES jobs(id) ON DELETE CASCADE,
             seq INTEGER NOT NULL,
             at TEXT NOT NULL,
             code TEXT NOT NULL,
             message TEXT NOT NULL,
             agent TEXT NOT NULL DEFAULT '',
             count INTEGER NOT NULL DEFAULT 0,
             PRIMARY KEY (job_id, seq)
         );

         CREATE TABLE IF NOT EXISTS job_results (
             job_id TEXT PRIMARY KEY REFERENCES jobs(id) ON DELETE CASCADE,
             result_json BLOB NOT NULL
         );",
    )?;
    for ddl in [
        "ALTER TABLE jobs ADD COLUMN effect TEXT NOT NULL DEFAULT 'read'",
        "ALTER TABLE jobs ADD COLUMN dispatch_phase TEXT NOT NULL DEFAULT 'pending'",
        "ALTER TABLE jobs ADD COLUMN owner_pid INTEGER NOT NULL DEFAULT 0",
    ] {
        let _ = conn.execute(ddl, []);
    }
    Ok(())
}
