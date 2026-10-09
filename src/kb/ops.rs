//! 원장 연산. 모든 쓰기는 append 지향, 스키마는 마이그레이션으로만 진화.
use std::path::Path;
use std::sync::Mutex;

use rusqlite::{Connection, OptionalExtension, params};
use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::apperr::{self, AppResult};

const MIGRATION_001: &str = include_str!("migrations_001_core.sql");

pub struct Kb {
    conn: Mutex<Connection>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Node {
    pub id: i64,
    pub site: String,
    pub kind: String,
    pub key: String,
    pub props: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct Edge {
    pub src: i64,
    pub dst: i64,
    pub rel: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct Gap {
    pub id: i64,
    pub site: String,
    pub rule: String,
    pub subject: String,
    pub state: String,
    pub priority: f64,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct Finding {
    pub node_id: i64,
    pub site: String,
    pub statement: String,
    pub scope: String,
    pub confidence: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct Issue {
    pub severity: String,
    pub kind: String,
    pub site: String,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct StatRow {
    pub action: String,
    pub defense: String,
    pub count: i64,
    pub pass_rate: f64,
}

#[derive(Debug, Clone, Default)]
pub struct AttemptInput {
    pub site: String,
    pub stage: String,
    pub action: String,
    pub verdict: String,
    pub target_kind: String,
    pub target_key: String,
    pub status: i64,
    pub defense: String,
    pub tls_profile: String,
    pub duration_ms: i64,
    pub cost_cents: f64,
    pub loop_id: String,
    pub props: Option<serde_json::Value>,
}

impl Kb {
    pub fn open(path: &Path) -> AppResult<Self> {
        let conn = Connection::open(path)
            .map_err(|e| apperr::runtime(e, "KB 열기 실패", "경로/권한 확인"))?;
        conn.pragma_update(None, "journal_mode", "WAL").ok();
        conn.pragma_update(None, "foreign_keys", "ON").ok();
        conn.busy_timeout(std::time::Duration::from_millis(5000))
            .ok();
        let kb = Self {
            conn: Mutex::new(conn),
        };
        kb.migrate()?;
        Ok(kb)
    }

    fn migrate(&self) -> AppResult<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS schema_migrations (
                version INTEGER PRIMARY KEY, name TEXT NOT NULL,
                applied_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')));",
        )
        .map_err(|e| apperr::runtime(e, "마이그레이션 테이블 생성 실패", ""))?;
        let applied: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM schema_migrations WHERE version=1",
                [],
                |r| r.get(0),
            )
            .unwrap_or(0);
        if applied == 0 {
            conn.execute_batch(MIGRATION_001)
                .map_err(|e| apperr::runtime(e, "마이그레이션 001 적용 실패", ""))?;
            conn.execute(
                "INSERT INTO schema_migrations(version, name) VALUES(1, '001_core')",
                [],
            )
            .map_err(|e| apperr::runtime(e, "마이그레이션 기록 실패", ""))?;
        }
        Ok(())
    }

    pub fn schema_version(&self) -> i64 {
        self.conn
            .lock()
            .unwrap()
            .query_row(
                "SELECT COALESCE(MAX(version),0) FROM schema_migrations",
                [],
                |r| r.get(0),
            )
            .unwrap_or(0)
    }

    pub fn ensure_node(
        &self,
        site: &str,
        kind: &str,
        key: &str,
        props: &serde_json::Value,
    ) -> AppResult<i64> {
        let props = if props.is_null() {
            "{}".to_string()
        } else {
            props.to_string()
        };
        let conn = self.conn.lock().unwrap();
        conn.query_row(
            "INSERT INTO node(site, kind, key, props) VALUES(?1,?2,?3,?4)
             ON CONFLICT(site, kind, key) DO UPDATE SET props=excluded.props,
               updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now')
             RETURNING id",
            params![site, kind, key, props],
            |r| r.get(0),
        )
        .map_err(|e| apperr::runtime(e, "노드 upsert 실패", ""))
    }

    pub fn node(&self, site: &str, kind: &str, key: &str) -> AppResult<Option<Node>> {
        let conn = self.conn.lock().unwrap();
        conn.query_row(
            "SELECT id, site, kind, key, props, created_at, updated_at
             FROM node WHERE site=?1 AND kind=?2 AND key=?3",
            params![site, kind, key],
            |r| {
                Ok(Node {
                    id: r.get(0)?,
                    site: r.get(1)?,
                    kind: r.get(2)?,
                    key: r.get(3)?,
                    props: r.get(4)?,
                    created_at: r.get(5)?,
                    updated_at: r.get(6)?,
                })
            },
        )
        .optional()
        .map_err(|e| apperr::runtime(e, "노드 조회 실패", ""))
    }

    pub fn link(&self, src: i64, dst: i64, rel: &str) -> AppResult<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO edge(src, dst, rel) VALUES(?,?,?)
             ON CONFLICT(src, dst, rel) DO NOTHING",
            params![src, dst, rel],
        )
        .map(|_| ())
        .map_err(|e| apperr::runtime(e, "엣지 upsert 실패", "src/dst 노드 존재 확인"))
    }

    pub fn query_nodes(&self, site: &str, kind: &str, limit: i64) -> AppResult<Vec<Node>> {
        let limit = if limit <= 0 || limit > 1000 {
            200
        } else {
            limit
        };
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, site, kind, key, props, created_at, updated_at
             FROM node WHERE site=?1 AND (?2='*' OR kind=?2)
             ORDER BY updated_at DESC LIMIT ?3",
        )?;
        let rows = stmt
            .query_map(params![site, kind, limit], |r| {
                Ok(Node {
                    id: r.get(0)?,
                    site: r.get(1)?,
                    kind: r.get(2)?,
                    key: r.get(3)?,
                    props: r.get(4)?,
                    created_at: r.get(5)?,
                    updated_at: r.get(6)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| apperr::runtime(e, "노드 목록 실패", ""))?;
        Ok(rows)
    }

    pub fn subgraph(&self, root_id: i64, depth: i64) -> AppResult<(Vec<Node>, Vec<Edge>)> {
        let depth = if depth <= 0 || depth > 6 { 2 } else { depth };
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "WITH RECURSIVE walk(id, d) AS (
                SELECT ?1, 0
                UNION
                SELECT CASE WHEN e.src=w.id THEN e.dst ELSE e.src END, w.d+1
                FROM walk w JOIN edge e ON e.src=w.id OR e.dst=w.id
                WHERE w.d < ?2
             )
             SELECT DISTINCT id FROM walk",
        )?;
        let ids: Vec<i64> = stmt
            .query_map(params![root_id, depth], |r| r.get(0))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| apperr::runtime(e, "그래프 순회 실패", ""))?;
        let mut nodes = Vec::new();
        let mut edges = Vec::new();
        let mut seen = std::collections::HashSet::new();
        for id in ids {
            if let Some(n) = conn.query_row(
                "SELECT id, site, kind, key, props, created_at, updated_at FROM node WHERE id=?1",
                params![id],
                |r| Ok(Node {
                    id: r.get(0)?, site: r.get(1)?, kind: r.get(2)?, key: r.get(3)?,
                    props: r.get(4)?, created_at: r.get(5)?, updated_at: r.get(6)?,
                }),
            ).optional()? {
                nodes.push(n);
            }
            let mut es = conn.prepare("SELECT src, dst, rel FROM edge WHERE src=?1 OR dst=?1")?;
            let rows = es.query_map(params![id], |r| {
                Ok(Edge {
                    src: r.get(0)?,
                    dst: r.get(1)?,
                    rel: r.get(2)?,
                })
            })?;
            for e in rows {
                let e = e?;
                let k = format!("{}|{}|{}", e.src, e.dst, e.rel);
                if seen.insert(k) {
                    edges.push(e);
                }
            }
        }
        Ok((nodes, edges))
    }

    /// 시도 1건 기록 — 대상 노드가 지정되면 "targets" 엣지까지. 실행이 곧 기록.
    pub fn record_attempt(&self, in_: &AttemptInput) -> AppResult<i64> {
        let key = format!(
            "{}|{}|{}",
            in_.action,
            if in_.target_key.is_empty() {
                "-"
            } else {
                &in_.target_key
            },
            chrono_key()
        );
        let mut props = in_.props.clone().unwrap_or(serde_json::json!({}));
        if let Some(obj) = props.as_object_mut() {
            obj.insert("verdict".into(), serde_json::json!(in_.verdict));
            if in_.status != 0 {
                obj.insert("status".into(), serde_json::json!(in_.status));
            }
            if !in_.defense.is_empty() {
                obj.insert("defense".into(), serde_json::json!(in_.defense));
            }
        }
        let id = self.ensure_node(&in_.site, "attempt", &key, &props)?;
        {
            let conn = self.conn.lock().unwrap();
            conn.execute(
                "INSERT INTO attempt(node_id, stage, action, verdict, status_code, defense, tls_profile, cost_cents, duration_ms, loop_id)
                 VALUES(?,?,?,?,?,?,?,?,?,?)",
                params![
                    id, in_.stage, in_.action, in_.verdict,
                    if in_.status == 0 { None } else { Some(in_.status) },
                    in_.defense, in_.tls_profile, in_.cost_cents, in_.duration_ms, in_.loop_id,
                ],
            ).map_err(|e| apperr::runtime(e, "attempt 기록 실패", ""))?;
        }
        if !in_.target_kind.is_empty() && !in_.target_key.is_empty() {
            let tid = self.ensure_node(
                &in_.site,
                &in_.target_kind,
                &in_.target_key,
                &serde_json::Value::Null,
            )?;
            self.link(id, tid, "targets")?;
        }
        Ok(id)
    }

    pub fn add_gap(
        &self,
        site: &str,
        rule: &str,
        subject: &str,
        reason: &str,
        priority: f64,
    ) -> AppResult<i64> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO gap(site, rule, subject, reason, priority) VALUES(?,?,?,?,?)
             ON CONFLICT(site, rule, subject) DO UPDATE SET reason=excluded.reason,
               priority=excluded.priority, updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now')",
            params![site, rule, subject, reason, priority],
        )
        .map_err(|e| apperr::runtime(e, "갭 upsert 실패", ""))?;
        conn.query_row(
            "SELECT id FROM gap WHERE site=?1 AND rule=?2 AND subject=?3",
            params![site, rule, subject],
            |r| r.get(0),
        )
        .map_err(|e| apperr::runtime(e, "갭 조회 실패", ""))
    }

    pub fn gaps(&self, site: &str, state: &str) -> AppResult<Vec<Gap>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, site, rule, subject, state, priority, reason FROM gap
             WHERE site=?1 AND (?2='' OR state=?2) ORDER BY priority DESC, id ASC",
        )?;
        let rows = stmt
            .query_map(params![site, state], |r| {
                Ok(Gap {
                    id: r.get(0)?,
                    site: r.get(1)?,
                    rule: r.get(2)?,
                    subject: r.get(3)?,
                    state: r.get(4)?,
                    priority: r.get(5)?,
                    reason: r.get(6)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| apperr::runtime(e, "갭 목록 실패", ""))?;
        Ok(rows)
    }

    /// 다음 행동 1개 — 최우선 열린 갭. AI는 매 턴 이걸 묻는다.
    pub fn next(&self, site: &str) -> AppResult<Option<Gap>> {
        let conn = self.conn.lock().unwrap();
        conn.query_row(
            "SELECT id, site, rule, subject, state, priority, reason FROM gap
             WHERE site=?1 AND state='open' ORDER BY priority DESC, id ASC LIMIT 1",
            params![site],
            |r| {
                Ok(Gap {
                    id: r.get(0)?,
                    site: r.get(1)?,
                    rule: r.get(2)?,
                    subject: r.get(3)?,
                    state: r.get(4)?,
                    priority: r.get(5)?,
                    reason: r.get(6)?,
                })
            },
        )
        .optional()
        .map_err(|e| apperr::runtime(e, "next 조회 실패", ""))
    }

    pub fn set_gap_state(&self, id: i64, state: &str) -> AppResult<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE gap SET state=?1, updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE id=?2",
            params![state, id],
        )
        .map(|_| ())
        .map_err(|e| apperr::runtime(e, "갭 상태 변경 실패", ""))
    }

    pub fn add_finding(
        &self,
        site: &str,
        statement: &str,
        confidence: f64,
        evidence: &[i64],
    ) -> AppResult<i64> {
        if statement.trim().is_empty() {
            return Err(apperr::spec(
                "교훈 문장이 비어 있다",
                "관찰 가능한 사실 문장으로 작성",
            ));
        }
        let conf = if confidence <= 0.0 || confidence > 1.0 {
            0.8
        } else {
            confidence
        };
        let mut hasher = Sha256::new();
        hasher.update(statement.as_bytes());
        let key = hasher.finalize()[..8]
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>();
        let id = self.ensure_node(
            site,
            "finding",
            &key,
            &serde_json::json!({"statement": statement}),
        )?;
        {
            let conn = self.conn.lock().unwrap();
            conn.execute(
                "INSERT INTO finding(node_id, statement, scope, confidence) VALUES(?,?,?,?)
                 ON CONFLICT(node_id) DO UPDATE SET statement=excluded.statement, confidence=excluded.confidence",
                params![id, statement, "site", conf],
            ).map_err(|e| apperr::runtime(e, "finding 기록 실패", ""))?;
        }
        for ev in evidence {
            self.link(id, *ev, "evidenced_by")?;
        }
        Ok(id)
    }

    pub fn search_findings(&self, site: &str, q: &str, limit: i64) -> AppResult<Vec<Finding>> {
        let limit = if limit <= 0 || limit > 200 { 50 } else { limit };
        let conn = self.conn.lock().unwrap();
        if q.trim().is_empty() {
            let mut stmt = conn.prepare(
                "SELECT f.node_id, n.site, f.statement, f.scope, f.confidence
                 FROM finding f JOIN node n ON n.id=f.node_id
                 WHERE (?1='' OR n.site=?1) ORDER BY f.node_id DESC LIMIT ?2",
            )?;
            let rows = stmt
                .query_map(params![site, limit], map_finding)?
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| apperr::runtime(e, "", ""))?;
            return Ok(rows);
        }
        let terms: Vec<String> = q
            .split_whitespace()
            .map(|t| format!("\"{}\"", t.replace('"', "")))
            .collect();
        let match_q = terms.join(" ");
        let mut stmt = conn.prepare(
            "SELECT f.node_id, n.site, f.statement, f.scope, f.confidence
             FROM finding_fts ft
             JOIN finding f ON f.node_id = ft.rowid
             JOIN node n ON n.id = f.node_id
             WHERE finding_fts MATCH ?1 AND (?2='' OR n.site=?2)
             ORDER BY rank LIMIT ?3",
        )?;
        let rows = stmt
            .query_map(params![match_q, site, limit], map_finding)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| apperr::runtime(e, "", ""))?;
        Ok(rows)
    }

    pub fn register_artifact(
        &self,
        site: &str,
        kind: &str,
        path: &str,
    ) -> AppResult<(i64, String)> {
        let bytes = std::fs::read(path)
            .map_err(|e| apperr::runtime(e, format!("아티팩트 파일 없음: {path}"), "경로 확인"))?;
        let mut h = Sha256::new();
        h.update(&bytes);
        let sum = h
            .finalize()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>();
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO artifact(site, kind, path, sha256, bytes) VALUES(?,?,?,?,?)
             ON CONFLICT(sha256) DO NOTHING",
            params![site, kind, path, sum, bytes.len() as i64],
        )
        .map_err(|e| apperr::runtime(e, "아티팩트 등록 실패", ""))?;
        let id = conn
            .query_row(
                "SELECT id FROM artifact WHERE sha256=?1",
                params![sum],
                |r| r.get(0),
            )
            .map_err(|e| apperr::runtime(e, "아티팩트 조회 실패", ""))?;
        Ok((id, sum))
    }

    pub fn stats(&self, site: &str) -> AppResult<Vec<StatRow>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT a.action, a.defense, a.verdict, COUNT(*) as c
             FROM attempt a JOIN node n ON n.id=a.node_id
             WHERE (?1='' OR n.site=?1)
             GROUP BY a.action, a.defense, a.verdict ORDER BY a.action, a.defense",
        )?;
        let rows = stmt
            .query_map(params![site], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, i64>(3)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| apperr::runtime(e, "집계 실패", ""))?;
        let mut groups: Vec<(String, String, i64, f64)> = Vec::new();
        for (action, defense, verdict, c) in rows {
            let g = groups.iter_mut().find(|g| g.0 == action && g.1 == defense);
            match g {
                Some(g) => {
                    g.2 += c;
                    if verdict == "pass" {
                        g.3 += c as f64;
                    }
                }
                None => groups.push((
                    action,
                    defense,
                    c,
                    if verdict == "pass" { c as f64 } else { 0.0 },
                )),
            }
        }
        Ok(groups
            .into_iter()
            .map(|(action, defense, count, pass)| StatRow {
                pass_rate: if count > 0 { pass / count as f64 } else { 0.0 },
                action,
                defense,
                count,
            })
            .collect())
    }

    /// 무결성 감사: 시도 없는 대상 · 증거 없는 교훈 · 실종 아티팩트.
    pub fn fsck(&self, site: &str) -> AppResult<Vec<Issue>> {
        let conn = self.conn.lock().unwrap();
        let mut out: Vec<Issue> = Vec::new();
        {
            let mut stmt = conn.prepare(
                "SELECT n.site, n.kind, n.key FROM node n
                 WHERE n.kind IN ('endpoint','url') AND (?1='' OR n.site=?1)
                 AND NOT EXISTS (
                   SELECT 1 FROM edge e
                   JOIN node a ON a.id=e.src JOIN attempt t ON t.node_id=a.id
                   WHERE e.dst=n.id AND e.rel='targets'
                 )",
            )?;
            let rows = stmt.query_map(params![site], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                ))
            })?;
            for row in rows {
                let (s, k, key) = row?;
                out.push(Issue {
                    severity: "warn".into(),
                    kind: "attempt-less-target".into(),
                    site: s,
                    detail: format!("{k} {key} 에 대한 시도가 없다"),
                });
            }
        }
        {
            let mut stmt = conn.prepare(
                "SELECT n.site, n.key FROM node n
                 JOIN finding f ON f.node_id=n.id
                 WHERE (?1='' OR n.site=?1)
                 AND NOT EXISTS (SELECT 1 FROM edge e WHERE e.src=n.id AND e.rel='evidenced_by')",
            )?;
            let rows = stmt.query_map(params![site], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
            })?;
            for row in rows {
                let (s, key) = row?;
                out.push(Issue {
                    severity: "error".into(),
                    kind: "evidence-less-finding".into(),
                    site: s,
                    detail: format!("finding {key} 에 증거 엣지가 없다"),
                });
            }
        }
        {
            let mut stmt =
                conn.prepare("SELECT site, path, sha256 FROM artifact WHERE (?1='' OR site=?1)")?;
            let rows = stmt.query_map(params![site], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                ))
            })?;
            for row in rows {
                let (s, path, sum) = row?;
                if !std::path::Path::new(&path).exists() {
                    let short: String = sum.chars().take(8).collect();
                    out.push(Issue {
                        severity: "error".into(),
                        kind: "missing-artifact".into(),
                        site: s,
                        detail: format!("{path} ({short}) 파일이 디스크에 없다"),
                    });
                }
            }
        }
        Ok(out)
    }
}

fn map_finding(r: &rusqlite::Row) -> rusqlite::Result<Finding> {
    Ok(Finding {
        node_id: r.get(0)?,
        site: r.get(1)?,
        statement: r.get(2)?,
        scope: r.get(3)?,
        confidence: r.get(4)?,
    })
}

fn chrono_key() -> u128 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0)
}
