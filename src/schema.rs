use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct History {
    pub id: String,
    pub timestamp: i64,
    pub duration: i64,
    pub exit: i32,
    pub command: String,
    pub cwd: String,
    pub session: String,
    pub hostname: String,
    pub deleted_at: Option<i64>,
}

impl History {
    pub fn from_row(row: &rusqlite::Row) -> rusqlite::Result<Self> {
        Ok(Self {
            id: row.get("id")?,
            timestamp: row.get("timestamp")?,
            duration: row.get("duration")?,
            exit: row.get("exit")?,
            command: row.get("command")?,
            cwd: row.get("cwd")?,
            session: row.get("session")?,
            hostname: row.get("hostname")?,
            deleted_at: row.get("deleted_at")?,
        })
    }
}

#[derive(Debug, Clone, Default)]
pub struct SearchFilter {
    pub query: Option<String>,
    pub cwd: Option<String>,
    #[allow(dead_code)]
    pub session: Option<String>,
    pub hostname: Option<String>,
    pub exit: Option<i32>,
    pub limit: usize,
    pub offset: usize,
}

impl SearchFilter {
    pub fn new(limit: usize) -> Self {
        Self { limit, ..Default::default() }
    }
}
