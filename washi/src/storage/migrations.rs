//! Ordered, versioned schema migrations. `meta.schema_version` records the last applied one.

use rusqlite::{Connection, OptionalExtension};

/// `MIGRATIONS[n]` upgrades schema version `n` to `n + 1`.
pub const MIGRATIONS: &[&str] = &[
    // v1
    "CREATE TABLE meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);
     CREATE TABLE notes (
       id            INTEGER PRIMARY KEY,
       content       TEXT    NOT NULL,
       is_open       INTEGER NOT NULL DEFAULT 1,
       always_on_top INTEGER NOT NULL DEFAULT 0,
       x INTEGER, y INTEGER, w INTEGER, h INTEGER,
       z_order       INTEGER NOT NULL DEFAULT 0,
       tape_style    INTEGER NOT NULL DEFAULT 0,
       created_at    INTEGER NOT NULL,
       updated_at    INTEGER NOT NULL
     );
     CREATE TABLE note_images (
       id       INTEGER PRIMARY KEY,
       note_id  INTEGER NOT NULL REFERENCES notes(id) ON DELETE CASCADE,
       mime     TEXT    NOT NULL,
       width    INTEGER NOT NULL, height INTEGER NOT NULL,
       data     BLOB    NOT NULL
     );
     CREATE INDEX idx_notes_open ON notes(is_open);",
];

pub fn current_version() -> u32 {
    MIGRATIONS.len() as u32
}

pub fn schema_version(conn: &Connection) -> rusqlite::Result<u32> {
    let has_meta: bool = conn.query_row(
        "SELECT count(*) > 0 FROM sqlite_master WHERE type='table' AND name='meta'",
        [],
        |r| r.get(0),
    )?;
    if !has_meta {
        return Ok(0);
    }
    let v: Option<String> = conn
        .query_row("SELECT value FROM meta WHERE key='schema_version'", [], |r| r.get(0))
        .optional()?;
    Ok(v.and_then(|s| s.parse().ok()).unwrap_or(0))
}

/// Apply every pending migration, each inside its own transaction.
pub fn migrate(conn: &mut Connection, now_ms: i64) -> rusqlite::Result<()> {
    let from = schema_version(conn)? as usize;
    for (n, sql) in MIGRATIONS.iter().enumerate().skip(from) {
        let tx = conn.transaction()?;
        tx.execute_batch(sql)?;
        if n == 0 {
            tx.execute("INSERT INTO meta(key, value) VALUES ('created_at', ?1)", [now_ms.to_string()])?;
        }
        tx.execute(
            "INSERT INTO meta(key, value) VALUES ('schema_version', ?1)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            [(n + 1).to_string()],
        )?;
        tx.commit()?;
    }
    Ok(())
}
