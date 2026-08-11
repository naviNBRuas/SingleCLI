//! Minimal SQLite-backed state: a log of runtime events (agent detected,
//! integration applied, setup run). Phase 1 doesn't yet have task/session
//! state to persist — this exists so the storage seam and schema
//! versioning approach are real from day one instead of retrofitted in
//! Phase 4 when task state shows up.

use anyhow::{Context, Result};
use rusqlite::Connection;
use std::path::Path;

pub fn open(db_path: &Path) -> Result<Connection> {
    if let Some(parent) = db_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let conn = Connection::open(db_path).with_context(|| format!("opening {}", db_path.display()))?;
    conn.execute(
        "CREATE TABLE IF NOT EXISTS events (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            occurred_at TEXT NOT NULL,
            kind TEXT NOT NULL,
            detail TEXT NOT NULL
        )",
        (),
    )?;
    Ok(conn)
}

pub fn record_event(conn: &Connection, kind: &str, detail: &str) -> Result<()> {
    conn.execute(
        "INSERT INTO events (occurred_at, kind, detail) VALUES (?1, ?2, ?3)",
        (chrono::Utc::now().to_rfc3339(), kind, detail),
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn open_creates_schema_and_records_events() {
        let dir = tempfile::tempdir().unwrap();
        let conn = open(&dir.path().join("state").join("single.db")).unwrap();
        record_event(&conn, "setup", "ran dry-run").unwrap();
        let count: i64 = conn.query_row("SELECT COUNT(*) FROM events", (), |row| row.get(0)).unwrap();
        assert_eq!(count, 1);
    }
}
