//! Per-provider API-key storage for the E28 free-provider pool
//! (`free_pool::FreeProvider`) — distinct from both `providers.rs`'s
//! single shared key per provider and `provider_keys.rs`'s per-agent
//! labeled keys, because pool providers are keyed by `(platform, key_id)`
//! with no agent attribution at all (a pool key belongs to the pool
//! engine, not to any one agent). Reuses the same registry-row/keychain
//! split those two modules already establish: this table only ever holds
//! metadata, the actual key value lives in the OS keychain under
//! `secret_name(platform, key_id)`, set separately via
//! `single_core::secrets::SecretStore` by the caller (mirrors
//! `provider_keys.rs::add`'s doc comment).
//!
//! Storage is SQLite (the shared runtime db, opened by the caller via
//! `crate::state`-equivalent in `single-runtime`), not a TOML file —
//! `notes.rs` is the precedent for a `single-core` module owning a table
//! in that shared db via `ensure_schema(conn)` + plain rusqlite CRUD.

use anyhow::{Context, Result};
use rusqlite::{params, Connection, OptionalExtension};

/// `"pool-key:{platform}:{key_id}"` — a namespace distinct from
/// `providers.rs`'s `"provider:{name}"` and `provider_keys.rs`'s
/// `"provider-key:{provider}:{label}"`, so all three registries can
/// coexist in the OS keychain without ever colliding.
pub fn secret_name(platform: &str, key_id: &str) -> String {
    format!("pool-key:{platform}:{key_id}")
}

#[derive(Debug, Clone, PartialEq)]
pub struct PoolProviderKey {
    pub platform: String,
    pub key_id: String,
    pub secret_ref: String,
    pub added_at: String,
    pub last_validated_at: Option<String>,
    pub valid: bool,
    pub disabled: bool,
}

pub fn ensure_schema(conn: &Connection) -> Result<()> {
    conn.execute(
        "CREATE TABLE IF NOT EXISTS pool_provider_keys (
            platform TEXT NOT NULL,
            key_id TEXT NOT NULL,
            secret_ref TEXT NOT NULL,
            added_at TEXT NOT NULL,
            last_validated_at TEXT,
            valid INTEGER NOT NULL DEFAULT 0,
            disabled INTEGER NOT NULL DEFAULT 0,
            PRIMARY KEY (platform, key_id)
        )",
        (),
    )?;
    Ok(())
}

/// Registers (or re-registers) one key's metadata row. The caller stores
/// the actual secret value separately via `secrets::SecretStore` under
/// `secret_name(platform, key_id)` — this only writes the registry row,
/// same separation `provider_keys.rs::add` documents.
pub fn add(conn: &Connection, platform: &str, key_id: &str) -> Result<()> {
    let secret_ref = secret_name(platform, key_id);
    let added_at = chrono::Utc::now().to_rfc3339();
    conn.execute(
        "INSERT INTO pool_provider_keys (platform, key_id, secret_ref, added_at, last_validated_at, valid, disabled)
         VALUES (?1, ?2, ?3, ?4, NULL, 0, 0)
         ON CONFLICT(platform, key_id) DO UPDATE SET secret_ref = excluded.secret_ref",
        params![platform, key_id, secret_ref, added_at],
    )
    .context("inserting pool provider key")?;
    Ok(())
}

pub fn list(conn: &Connection, platform: Option<&str>) -> Result<Vec<PoolProviderKey>> {
    let mut sql = String::from("SELECT platform, key_id, secret_ref, added_at, last_validated_at, valid, disabled FROM pool_provider_keys");
    if platform.is_some() {
        sql.push_str(" WHERE platform = ?1");
    }
    sql.push_str(" ORDER BY platform, key_id");

    let mut stmt = conn.prepare(&sql)?;
    let rows = if let Some(platform) = platform {
        stmt.query_map(params![platform], row_to_key)?.collect::<rusqlite::Result<Vec<_>>>()
    } else {
        stmt.query_map((), row_to_key)?.collect::<rusqlite::Result<Vec<_>>>()
    };
    rows.context("collecting pool provider keys")
}

pub fn mark_validated(conn: &Connection, platform: &str, key_id: &str, valid: bool) -> Result<()> {
    let now = chrono::Utc::now().to_rfc3339();
    conn.execute(
        "UPDATE pool_provider_keys SET last_validated_at = ?1, valid = ?2 WHERE platform = ?3 AND key_id = ?4",
        params![now, valid as i64, platform, key_id],
    )
    .context("marking pool provider key validated")?;
    Ok(())
}

pub fn disable(conn: &Connection, platform: &str, key_id: &str) -> Result<()> {
    conn.execute(
        "UPDATE pool_provider_keys SET disabled = 1 WHERE platform = ?1 AND key_id = ?2",
        params![platform, key_id],
    )
    .context("disabling pool provider key")?;
    Ok(())
}

pub fn is_disabled(conn: &Connection, platform: &str, key_id: &str) -> Result<bool> {
    let disabled: Option<i64> = conn
        .query_row(
            "SELECT disabled FROM pool_provider_keys WHERE platform = ?1 AND key_id = ?2",
            params![platform, key_id],
            |row| row.get(0),
        )
        .optional()
        .context("querying pool provider key disabled state")?;
    Ok(disabled.unwrap_or(0) != 0)
}

fn row_to_key(row: &rusqlite::Row) -> rusqlite::Result<PoolProviderKey> {
    Ok(PoolProviderKey {
        platform: row.get("platform")?,
        key_id: row.get("key_id")?,
        secret_ref: row.get("secret_ref")?,
        added_at: row.get("added_at")?,
        last_validated_at: row.get("last_validated_at")?,
        valid: row.get::<_, i64>("valid")? != 0,
        disabled: row.get::<_, i64>("disabled")? != 0,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_conn() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        ensure_schema(&conn).unwrap();
        conn
    }

    #[test]
    fn add_list_mark_validated_roundtrip() {
        let conn = test_conn();
        add(&conn, "groq", "default").unwrap();

        let keys = list(&conn, Some("groq")).unwrap();
        assert_eq!(keys.len(), 1);
        assert_eq!(keys[0].secret_ref, "pool-key:groq:default");
        assert!(!keys[0].valid);
        assert!(keys[0].last_validated_at.is_none());

        mark_validated(&conn, "groq", "default", true).unwrap();
        let keys = list(&conn, Some("groq")).unwrap();
        assert!(keys[0].valid);
        assert!(keys[0].last_validated_at.is_some());
    }

    #[test]
    fn secret_name_uses_the_pool_key_namespace() {
        assert_eq!(secret_name("groq", "default"), "pool-key:groq:default");
    }

    #[test]
    fn add_is_idempotent_by_platform_and_key_id() {
        let conn = test_conn();
        add(&conn, "groq", "default").unwrap();
        add(&conn, "groq", "default").unwrap();
        assert_eq!(list(&conn, Some("groq")).unwrap().len(), 1);
    }

    #[test]
    fn list_with_no_platform_returns_every_key() {
        let conn = test_conn();
        add(&conn, "groq", "default").unwrap();
        add(&conn, "nvidia", "default").unwrap();
        assert_eq!(list(&conn, None).unwrap().len(), 2);
    }

    #[test]
    fn disable_and_is_disabled_round_trip() {
        let conn = test_conn();
        add(&conn, "groq", "default").unwrap();
        assert!(!is_disabled(&conn, "groq", "default").unwrap());
        disable(&conn, "groq", "default").unwrap();
        assert!(is_disabled(&conn, "groq", "default").unwrap());
    }

    #[test]
    fn is_disabled_is_false_for_an_unknown_key() {
        let conn = test_conn();
        assert!(!is_disabled(&conn, "no-such-platform", "default").unwrap());
    }
}
