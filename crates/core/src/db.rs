use std::collections::HashMap;
use std::path::Path;

use rusqlite::{params, Connection, OptionalExtension};

use crate::error::Result;
use crate::model::{FoundProject, StoredArtifact, StoredProject};
use crate::util::{normalize_path, path_key};

pub struct Database {
    conn: Connection,
}

impl Database {
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let conn = Connection::open(path)?;
        conn.execute_batch(
            "
            PRAGMA journal_mode = WAL;
            PRAGMA foreign_keys = ON;
            CREATE TABLE IF NOT EXISTS projects (
                id INTEGER PRIMARY KEY,
                root_path TEXT NOT NULL,
                path TEXT NOT NULL UNIQUE,
                name TEXT NOT NULL,
                ecosystems TEXT NOT NULL,
                ecosystem_label TEXT NOT NULL,
                last_touched INTEGER,
                git_dirty INTEGER NOT NULL DEFAULT 0,
                protected INTEGER NOT NULL DEFAULT 0,
                threshold_days INTEGER,
                allow_unsafe INTEGER NOT NULL DEFAULT 0,
                open_reason TEXT,
                lockfile TEXT,
                scanned_at INTEGER NOT NULL
            );
            CREATE TABLE IF NOT EXISTS artifacts (
                id INTEGER PRIMARY KEY,
                project_id INTEGER NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
                rel_path TEXT NOT NULL,
                rule_id TEXT NOT NULL,
                pattern TEXT NOT NULL,
                size_bytes INTEGER NOT NULL,
                kind TEXT NOT NULL,
                ignored INTEGER NOT NULL,
                lock_ok INTEGER NOT NULL,
                UNIQUE(project_id, rel_path)
            );
            CREATE TABLE IF NOT EXISTS sweeps (
                id INTEGER PRIMARY KEY,
                started_at INTEGER NOT NULL,
                finished_at INTEGER,
                trigger TEXT NOT NULL,
                dry_run INTEGER NOT NULL,
                bytes_freed INTEGER NOT NULL,
                projects_cleaned INTEGER NOT NULL,
                errors INTEGER NOT NULL,
                skipped INTEGER NOT NULL,
                summary TEXT,
                skipped_summary TEXT
            );
            CREATE TABLE IF NOT EXISTS sweep_items (
                id INTEGER PRIMARY KEY,
                sweep_id INTEGER NOT NULL REFERENCES sweeps(id) ON DELETE CASCADE,
                project_path TEXT NOT NULL,
                project_name TEXT NOT NULL,
                rel_path TEXT NOT NULL,
                size_bytes INTEGER NOT NULL,
                result TEXT NOT NULL,
                error TEXT
            );
            CREATE TABLE IF NOT EXISTS exclusions (
                project_path TEXT NOT NULL,
                rel_path TEXT NOT NULL,
                PRIMARY KEY (project_path, rel_path)
            );
            CREATE TABLE IF NOT EXISTS kv (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL
            );
            ",
        )?;
        Ok(Self { conn })
    }

    pub fn replace_root(&mut self, root: &Path, projects: &[FoundProject], scanned_at: i64) -> Result<()> {
        let root_text = normalize_path(root).display().to_string();
        let flags = self.flags_for_root(&root_text)?;
        let tx = self.conn.transaction()?;
        tx.execute("DELETE FROM projects WHERE root_path = ?1", params![root_text])?;
        for project in projects {
            let path = normalize_path(&project.path).display().to_string();
            let key = path_key(Path::new(&path));
            let (saved_protected, allow_unsafe) = flags.get(&key).copied().unwrap_or((false, false));
            let ecosystems = serde_json::to_string(&project.ecosystems).unwrap_or_else(|_| "[]".into());
            tx.execute(
                "INSERT INTO projects (
                    root_path, path, name, ecosystems, ecosystem_label, last_touched, git_dirty,
                    protected, threshold_days, allow_unsafe, open_reason, lockfile, scanned_at
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
                params![
                    root_text,
                    path,
                    project.name,
                    ecosystems,
                    project.ecosystem_label,
                    project.last_touched,
                    project.git_dirty as i64,
                    (project.protected || saved_protected) as i64,
                    project.threshold_days.map(|days| days as i64),
                    allow_unsafe as i64,
                    project.open_reason,
                    project.lockfile,
                    scanned_at,
                ],
            )?;
            let project_id = tx.last_insert_rowid();
            for artifact in &project.artifacts {
                tx.execute(
                    "INSERT INTO artifacts (
                        project_id, rel_path, rule_id, pattern, size_bytes, kind, ignored, lock_ok
                    ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                    params![
                        project_id,
                        artifact.rel_path,
                        artifact.rule_id,
                        artifact.pattern,
                        artifact.size_bytes as i64,
                        artifact.kind,
                        artifact.ignored as i64,
                        artifact.lock_ok as i64,
                    ],
                )?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    fn flags_for_root(&self, root: &str) -> Result<HashMap<String, (bool, bool)>> {
        let mut stmt = self.conn.prepare(
            "SELECT path, protected, allow_unsafe FROM projects WHERE root_path = ?1",
        )?;
        let rows = stmt.query_map(params![root], |row| {
            let path: String = row.get(0)?;
            let protected: i64 = row.get(1)?;
            let allow: i64 = row.get(2)?;
            Ok((path_key(Path::new(&path)), (protected != 0, allow != 0)))
        })?;
        let mut map = HashMap::new();
        for row in rows {
            let (key, flags) = row?;
            map.insert(key, flags);
        }
        Ok(map)
    }

    pub fn delete_projects_outside(&self, roots: &[String]) -> Result<()> {
        let mut stmt = self.conn.prepare("SELECT DISTINCT root_path FROM projects")?;
        let existing: Vec<String> = stmt
            .query_map([], |row| row.get(0))?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        for root in existing {
            let keep = roots.iter().any(|candidate| path_key(Path::new(candidate)) == path_key(Path::new(&root)));
            if !keep {
                self.conn.execute("DELETE FROM projects WHERE root_path = ?1", params![root])?;
            }
        }
        Ok(())
    }

    pub fn load_projects(&self) -> Result<Vec<StoredProject>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, root_path, path, name, ecosystems, ecosystem_label, last_touched, git_dirty,
                    protected, threshold_days, allow_unsafe, open_reason, lockfile
             FROM projects ORDER BY name COLLATE NOCASE",
        )?;
        let rows = stmt.query_map([], |row| {
            let ecosystems: String = row.get(4)?;
            Ok(StoredProject {
                id: row.get(0)?,
                root_path: row.get(1)?,
                path: row.get(2)?,
                name: row.get(3)?,
                ecosystems: serde_json::from_str(&ecosystems).unwrap_or_default(),
                ecosystem_label: row.get(5)?,
                last_touched: row.get(6)?,
                git_dirty: row.get::<_, i64>(7)? != 0,
                protected: row.get::<_, i64>(8)? != 0,
                threshold_days: row.get::<_, Option<i64>>(9)?.map(|value| value as u32),
                allow_unsafe: row.get::<_, i64>(10)? != 0,
                open_reason: row.get(11)?,
                lockfile: row.get(12)?,
                artifacts: Vec::new(),
            })
        })?;
        let mut projects = Vec::new();
        for row in rows {
            projects.push(row?);
        }
        let mut artifacts = self.conn.prepare(
            "SELECT project_id, rel_path, rule_id, pattern, size_bytes, kind, ignored, lock_ok
             FROM artifacts",
        )?;
        let artifact_rows = artifacts.query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                StoredArtifact {
                    rel_path: row.get(1)?,
                    rule_id: row.get(2)?,
                    pattern: row.get(3)?,
                    size_bytes: row.get::<_, i64>(4)? as u64,
                    kind: row.get(5)?,
                    ignored: row.get::<_, i64>(6)? != 0,
                    lock_ok: row.get::<_, i64>(7)? != 0,
                },
            ))
        })?;
        let mut by_project: HashMap<i64, Vec<StoredArtifact>> = HashMap::new();
        for row in artifact_rows {
            let (id, artifact) = row?;
            by_project.entry(id).or_default().push(artifact);
        }
        for project in &mut projects {
            if let Some(items) = by_project.remove(&project.id) {
                project.artifacts = items;
            }
        }
        Ok(projects)
    }

    pub fn set_protected(&self, path: &Path, protected: bool) -> Result<bool> {
        let changed = self.conn.execute(
            "UPDATE projects SET protected = ?1 WHERE path = ?2 COLLATE NOCASE",
            params![protected as i64, normalize_path(path).display().to_string()],
        )?;
        Ok(changed > 0)
    }

    pub fn zero_artifact(&self, project_path: &str, rel_path: &str) -> Result<()> {
        self.conn.execute(
            "UPDATE artifacts SET size_bytes = 0
             WHERE rel_path = ?2 AND project_id = (SELECT id FROM projects WHERE path = ?1)",
            params![project_path, rel_path],
        )?;
        Ok(())
    }

    pub fn set_allow_unsafe(&self, path: &Path, allow: bool) -> Result<bool> {
        let changed = self.conn.execute(
            "UPDATE projects SET allow_unsafe = ?1 WHERE path = ?2 COLLATE NOCASE",
            params![allow as i64, normalize_path(path).display().to_string()],
        )?;
        Ok(changed > 0)
    }

    pub fn exclusions(&self) -> Result<Vec<(String, String)>> {
        let mut stmt = self.conn.prepare("SELECT project_path, rel_path FROM exclusions")?;
        let rows = stmt.query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?;
        rows.collect::<std::result::Result<Vec<_>, _>>().map_err(Into::into)
    }

    pub fn add_exclusions(&self, items: &[(String, String)]) -> Result<()> {
        for (path, rel) in items {
            self.conn.execute(
                "INSERT OR IGNORE INTO exclusions (project_path, rel_path) VALUES (?1, ?2)",
                params![path, rel],
            )?;
        }
        Ok(())
    }

    pub fn kv_get(&self, key: &str) -> Result<Option<String>> {
        self.conn
            .query_row("SELECT value FROM kv WHERE key = ?1", params![key], |row| row.get(0))
            .optional()
            .map_err(Into::into)
    }

    pub fn kv_set(&self, key: &str, value: &str) -> Result<()> {
        self.conn.execute(
            "INSERT INTO kv (key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![key, value],
        )?;
        Ok(())
    }

    pub fn insert_sweep(
        &self,
        started_at: i64,
        finished_at: i64,
        trigger: &str,
        bytes_freed: u64,
        projects_cleaned: u32,
        errors: u32,
        skipped: u32,
        summary: &str,
        skipped_summary: &str,
        items: &[(String, String, String, u64, String, Option<String>)],
    ) -> Result<i64> {
        self.conn.execute(
            "INSERT INTO sweeps (
                started_at, finished_at, trigger, dry_run, bytes_freed, projects_cleaned, errors, skipped, summary, skipped_summary
             ) VALUES (?1, ?2, ?3, 0, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                started_at,
                finished_at,
                trigger,
                bytes_freed as i64,
                projects_cleaned,
                errors,
                skipped,
                summary,
                skipped_summary,
            ],
        )?;
        let id = self.conn.last_insert_rowid();
        for (path, name, rel, size, result, error) in items {
            self.conn.execute(
                "INSERT INTO sweep_items (sweep_id, project_path, project_name, rel_path, size_bytes, result, error)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![id, path, name, rel, *size as i64, result, error],
            )?;
        }
        Ok(id)
    }

    pub fn sweep_rows(&self) -> Result<Vec<SweepRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, started_at, trigger, bytes_freed, projects_cleaned, errors, skipped, summary, skipped_summary
             FROM sweeps WHERE dry_run = 0 ORDER BY started_at DESC LIMIT 200",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(SweepRow {
                id: row.get(0)?,
                started_at: row.get(1)?,
                trigger: row.get(2)?,
                bytes_freed: row.get::<_, i64>(3)? as u64,
                projects_cleaned: row.get(4)?,
                errors: row.get(5)?,
                skipped: row.get(6)?,
                summary: row.get::<_, Option<String>>(7)?.unwrap_or_default(),
                skipped_summary: row.get::<_, Option<String>>(8)?.unwrap_or_default(),
            })
        })?;
        rows.collect::<std::result::Result<Vec<_>, _>>().map_err(Into::into)
    }

    pub fn sweep_items_for_csv(&self) -> Result<Vec<CsvItem>> {
        let mut stmt = self.conn.prepare(
            "SELECT s.started_at, s.trigger, i.project_name, i.project_path, i.rel_path, i.size_bytes, i.result, i.error
             FROM sweep_items i JOIN sweeps s ON s.id = i.sweep_id
             WHERE s.dry_run = 0
             ORDER BY s.started_at DESC, i.id",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(CsvItem {
                started_at: row.get(0)?,
                trigger: row.get(1)?,
                project_name: row.get(2)?,
                project_path: row.get(3)?,
                rel_path: row.get(4)?,
                size_bytes: row.get::<_, i64>(5)? as u64,
                result: row.get(6)?,
                error: row.get(7)?,
            })
        })?;
        rows.collect::<std::result::Result<Vec<_>, _>>().map_err(Into::into)
    }

    pub fn recent_cleaned(&self) -> Result<Vec<RecentCleanRow>> {
        let Some(sweep_id) = self
            .conn
            .query_row(
                "SELECT id FROM sweeps WHERE dry_run = 0 AND bytes_freed > 0 ORDER BY started_at DESC LIMIT 1",
                [],
                |row| row.get::<_, i64>(0),
            )
            .optional()?
        else {
            return Ok(Vec::new());
        };
        let mut stmt = self.conn.prepare(
            "SELECT project_name, project_path, rel_path, size_bytes FROM sweep_items
             WHERE sweep_id = ?1 AND result = 'deleted' ORDER BY size_bytes DESC",
        )?;
        let rows = stmt.query_map(params![sweep_id], |row| {
            Ok(RecentCleanRow {
                name: row.get(0)?,
                path: row.get(1)?,
                rel_path: row.get(2)?,
                size_bytes: row.get::<_, i64>(3)? as u64,
            })
        })?;
        rows.collect::<std::result::Result<Vec<_>, _>>().map_err(Into::into)
    }

    pub fn freed_since(&self, since: i64) -> Result<(u64, u32)> {
        let (bytes, count): (i64, i64) = self.conn.query_row(
            "SELECT COALESCE(SUM(bytes_freed), 0), COUNT(*) FROM sweeps WHERE dry_run = 0 AND started_at >= ?1",
            params![since],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        Ok((bytes as u64, count as u32))
    }

    pub fn freed_all(&self) -> Result<u64> {
        let bytes: i64 = self.conn.query_row(
            "SELECT COALESCE(SUM(bytes_freed), 0) FROM sweeps WHERE dry_run = 0",
            [],
            |row| row.get(0),
        )?;
        Ok(bytes as u64)
    }

    pub fn project_counts_by_root(&self) -> Result<HashMap<String, u32>> {
        let mut stmt = self.conn.prepare("SELECT root_path, COUNT(*) FROM projects GROUP BY root_path")?;
        let rows = stmt.query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)? as u32)))?;
        let mut map = HashMap::new();
        for row in rows {
            let (root, count) = row?;
            map.insert(path_key(Path::new(&root)), count);
        }
        Ok(map)
    }
}

#[derive(Debug, Clone)]
pub struct SweepRow {
    pub id: i64,
    pub started_at: i64,
    pub trigger: String,
    pub bytes_freed: u64,
    pub projects_cleaned: u32,
    pub errors: u32,
    pub skipped: u32,
    pub summary: String,
    pub skipped_summary: String,
}

#[derive(Debug, Clone)]
pub struct CsvItem {
    pub started_at: i64,
    pub trigger: String,
    pub project_name: String,
    pub project_path: String,
    pub rel_path: String,
    pub size_bytes: u64,
    pub result: String,
    pub error: Option<String>,
}

#[derive(Debug, Clone)]
pub struct RecentCleanRow {
    pub name: String,
    pub path: String,
    pub rel_path: String,
    pub size_bytes: u64,
}
