use anyhow::Result;
use rusqlite::{params, Connection};
use std::path::Path;
use std::sync::{Arc, Mutex};

use crate::config::Config;
use crate::schema::{History, SearchFilter};

#[derive(Clone)]
pub struct Database {
    conn: Arc<Mutex<Connection>>,
}

impl Database {
    pub fn open() -> Result<Self> {
        Self::open_at(&Config::db_path())
    }

    pub fn open_at(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let conn = Connection::open(path)?;
        conn.busy_timeout(std::time::Duration::from_secs(5))?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;

        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS history (
                id TEXT PRIMARY KEY,
                timestamp INTEGER NOT NULL,
                duration INTEGER NOT NULL,
                exit INTEGER NOT NULL,
                command TEXT NOT NULL,
                cwd TEXT NOT NULL,
                session TEXT NOT NULL,
                hostname TEXT NOT NULL,
                deleted_at INTEGER
            );
            CREATE INDEX IF NOT EXISTS idx_cmd_ts ON history(command, timestamp);
            CREATE INDEX IF NOT EXISTS idx_ts_cwd_cmd ON history(timestamp, cwd, command);
            CREATE UNIQUE INDEX IF NOT EXISTS idx_dedup ON history(timestamp, cwd, command);",
        )?;

        Ok(Self { conn: Arc::new(Mutex::new(conn)) })
    }

    pub fn insert(&self, h: &History) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        // Retry on SQLITE_BUSY — WAL mode serializes writes, so under heavy
        // concurrent load (multiple terminal sessions) we may need to wait.
        for attempt in 0..10 {
            match conn.execute(
                "INSERT OR IGNORE INTO history
                    (id, timestamp, duration, exit, command, cwd, session, hostname, deleted_at)
                 VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9)",
                params![h.id, h.timestamp, h.duration, h.exit, h.command, h.cwd, h.session, h.hostname, h.deleted_at],
            ) {
                Ok(_) => return Ok(()),
                Err(rusqlite::Error::SqliteFailure(e, _))
                    if e.code == rusqlite::ffi::ErrorCode::DatabaseBusy =>
                {
                    std::thread::sleep(std::time::Duration::from_millis(10 * (attempt + 1)));
                    continue;
                }
                Err(e) => return Err(e.into()),
            }
        }
        anyhow::bail!("database busy after 10 retries")
    }

    pub fn search(&self, filter: &SearchFilter) -> Result<Vec<History>> {
        let conn = self.conn.lock().unwrap();
        let mut sql = String::from(
            "SELECT id,timestamp,duration,exit,command,cwd,session,hostname,deleted_at
             FROM history WHERE deleted_at IS NULL",
        );
        let mut param_values: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
        let mut idx = 1;

        if let Some(ref q) = filter.query {
            sql.push_str(&format!(" AND command LIKE ?{idx}"));
            param_values.push(Box::new(format!("%{q}%")));
            idx += 1;
        }
        if let Some(ref cwd) = filter.cwd {
            sql.push_str(&format!(" AND cwd = ?{idx}"));
            param_values.push(Box::new(cwd.clone()));
            idx += 1;
        }
        if let Some(ref host) = filter.hostname {
            sql.push_str(&format!(" AND hostname = ?{idx}"));
            param_values.push(Box::new(host.clone()));
            idx += 1;
        }
        if let Some(exit) = filter.exit {
            sql.push_str(&format!(" AND exit = ?{idx}"));
            param_values.push(Box::new(exit));
            idx += 1;
        }
        let _ = idx;

        sql.push_str(" ORDER BY timestamp DESC");
        sql.push_str(&format!(" LIMIT {} OFFSET {}", filter.limit, filter.offset));

        let refs: Vec<&dyn rusqlite::types::ToSql> = param_values.iter().map(|p| p.as_ref()).collect();
        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt
            .query_map(refs.as_slice(), |row| History::from_row(row))?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    pub fn count(&self) -> Result<usize> {
        let conn = self.conn.lock().unwrap();
        Ok(conn.query_row("SELECT COUNT(*) FROM history WHERE deleted_at IS NULL", [], |r| r.get(0))?)
    }

    pub fn soft_delete(&self, id: &str, deleted_at: i64) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute("UPDATE history SET deleted_at=?1 WHERE id=?2", params![deleted_at, id])?;
        Ok(())
    }

    /// Delete the N oldest entries (by timestamp). Used for automatic pruning.
    pub fn prune_oldest(&self, count: usize) -> Result<usize> {
        let conn = self.conn.lock().unwrap();
        let deleted = conn.execute(
            "DELETE FROM history WHERE id IN (
                SELECT id FROM history WHERE deleted_at IS NULL
                ORDER BY timestamp ASC LIMIT ?1
            )",
            params![count],
        )?;
        Ok(deleted)
    }
}
