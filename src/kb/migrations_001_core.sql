-- 001_core: hycli 지식베이스 v1 원장
-- 철학: 실행이 곧 기록, 기록이 곧 그래프, 그래프가 곧 판단.

CREATE TABLE IF NOT EXISTS node (
  id         INTEGER PRIMARY KEY,
  site       TEXT NOT NULL,
  kind       TEXT NOT NULL,
  key        TEXT NOT NULL,
  props      TEXT NOT NULL DEFAULT '{}',
  created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
  updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
  UNIQUE (site, kind, key)
);
CREATE INDEX IF NOT EXISTS idx_node_site_kind ON node(site, kind);

CREATE TABLE IF NOT EXISTS edge (
  src        INTEGER NOT NULL REFERENCES node(id) ON DELETE CASCADE,
  dst        INTEGER NOT NULL REFERENCES node(id) ON DELETE CASCADE,
  rel        TEXT NOT NULL,
  props      TEXT NOT NULL DEFAULT '{}',
  created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
  PRIMARY KEY (src, dst, rel)
);
CREATE INDEX IF NOT EXISTS idx_edge_dst ON edge(dst, rel);

CREATE TABLE IF NOT EXISTS attempt (
  node_id     INTEGER PRIMARY KEY REFERENCES node(id) ON DELETE CASCADE,
  stage       TEXT NOT NULL DEFAULT '',
  action      TEXT NOT NULL,
  verdict     TEXT NOT NULL,
  status_code INTEGER,
  defense     TEXT NOT NULL DEFAULT '',
  tls_profile TEXT NOT NULL DEFAULT '',
  cost_cents  REAL NOT NULL DEFAULT 0,
  duration_ms INTEGER NOT NULL DEFAULT 0,
  loop_id     TEXT NOT NULL DEFAULT ''
);
CREATE INDEX IF NOT EXISTS idx_attempt_action ON attempt(action, verdict);
CREATE INDEX IF NOT EXISTS idx_attempt_defense ON attempt(defense, verdict);

CREATE TABLE IF NOT EXISTS gap (
  id         INTEGER PRIMARY KEY,
  site       TEXT NOT NULL,
  rule       TEXT NOT NULL,
  subject    TEXT NOT NULL,
  state      TEXT NOT NULL DEFAULT 'open',
  priority   REAL NOT NULL DEFAULT 0.5,
  reason     TEXT NOT NULL DEFAULT '',
  created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
  updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
  UNIQUE (site, rule, subject)
);
CREATE INDEX IF NOT EXISTS idx_gap_open ON gap(site, state, priority DESC);

CREATE TABLE IF NOT EXISTS artifact (
  id         INTEGER PRIMARY KEY,
  site       TEXT NOT NULL,
  kind       TEXT NOT NULL,
  path       TEXT NOT NULL,
  sha256     TEXT NOT NULL UNIQUE,
  bytes      INTEGER NOT NULL DEFAULT 0,
  created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
);

CREATE TABLE IF NOT EXISTS finding (
  node_id    INTEGER PRIMARY KEY REFERENCES node(id) ON DELETE CASCADE,
  statement  TEXT NOT NULL,
  scope      TEXT NOT NULL DEFAULT 'site',
  confidence REAL NOT NULL DEFAULT 0.8
);

CREATE VIRTUAL TABLE IF NOT EXISTS finding_fts USING fts5(statement, content='finding', content_rowid='node_id');
CREATE TRIGGER IF NOT EXISTS finding_fts_ai AFTER INSERT ON finding BEGIN
  INSERT INTO finding_fts(rowid, statement) VALUES (new.node_id, new.statement);
END;
CREATE TRIGGER IF NOT EXISTS finding_fts_ad AFTER DELETE ON finding BEGIN
  INSERT INTO finding_fts(finding_fts, rowid, statement) VALUES ('delete', old.node_id, old.statement);
END;
CREATE TRIGGER IF NOT EXISTS finding_fts_au AFTER UPDATE OF statement ON finding BEGIN
  INSERT INTO finding_fts(finding_fts, rowid, statement) VALUES ('delete', old.node_id, old.statement);
  INSERT INTO finding_fts(rowid, statement) VALUES (new.node_id, new.statement);
END;
