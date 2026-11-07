//! The encrypted notes database (SQLCipher, one connection owned by the UI thread).

use super::migrations;
use crate::platform::NoteId;
use rusqlite::{Connection, ErrorCode};
use std::path::Path;
use zeroize::Zeroizing;

#[derive(Debug)]
pub enum DbError {
    /// The key does not open this database (wrong key, or the file is damaged/tampered).
    WrongKey,
    Sqlite(rusqlite::Error),
}

impl std::fmt::Display for DbError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DbError::WrongKey => f.write_str(
                "Washi could not unlock your notes. The encryption key does not match the notes \
                 database, or the database file is damaged.",
            ),
            DbError::Sqlite(e) => write!(f, "The notes database reported an error: {e}"),
        }
    }
}
impl std::error::Error for DbError {}

impl From<rusqlite::Error> for DbError {
    fn from(e: rusqlite::Error) -> Self {
        DbError::Sqlite(e)
    }
}

pub struct NoteRow {
    pub id: NoteId,
    pub content: String,
    pub geometry: Option<Geometry>,
    pub always_on_top: bool,
}

/// Window position and size in logical pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Geometry {
    pub x: i32,
    pub y: i32,
    pub w: u32,
    pub h: u32,
}

/// A closed note as listed in the "Open note" menu.
pub struct ClosedNote {
    pub id: NoteId,
    /// The first part of the note's content, enough to build a title.
    pub preview: String,
}

pub struct Db {
    conn: Connection,
}

fn hex_key(raw: &[u8; 32]) -> Zeroizing<String> {
    let mut s = Zeroizing::new(String::with_capacity(67));
    s.push_str("x'");
    for b in raw {
        s.push_str(&format!("{b:02X}"));
    }
    s.push('\'');
    s
}

impl Db {
    /// Open (or create) the database with the raw key and apply pragmas and migrations.
    pub fn open(path: &Path, key: &[u8; 32], now_ms: i64) -> Result<Db, DbError> {
        let mut conn = Connection::open(path)?;
        conn.pragma_update(None, "key", hex_key(key).as_str())?;
        conn.pragma_update(None, "cipher_memory_security", "OFF")?;
        // Verifies the key: fails with NotADatabase on a wrong key or a damaged file.
        match conn.query_row("SELECT count(*) FROM sqlite_master", [], |_| Ok(())) {
            Ok(()) => {}
            Err(rusqlite::Error::SqliteFailure(e, _))
                if matches!(e.code, ErrorCode::NotADatabase | ErrorCode::DatabaseCorrupt) =>
            {
                return Err(DbError::WrongKey)
            }
            Err(e) => return Err(e.into()),
        }
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;
        conn.pragma_update(None, "temp_store", "MEMORY")?;
        conn.pragma_update(None, "cache_size", -512)?;
        conn.pragma_update(None, "secure_delete", "ON")?;
        migrations::migrate(&mut conn, now_ms)?;
        Ok(Db { conn })
    }

    pub fn schema_version(&self) -> Result<u32, DbError> {
        Ok(migrations::schema_version(&self.conn)?)
    }

    /// Hand SQLite's page and statement caches back to the heap (suspension).
    pub fn shrink_memory(&self) {
        if let Err(e) = self.conn.execute_batch("PRAGMA shrink_memory") {
            eprintln!("washi: shrink_memory failed: {e}");
        }
        self.conn.flush_prepared_statement_cache();
    }

    pub fn pragma_string(&self, name: &str) -> Result<String, DbError> {
        Ok(self.conn.query_row(&format!("PRAGMA {name}"), [], |r| {
            r.get::<_, rusqlite::types::Value>(0).map(|v| match v {
                rusqlite::types::Value::Integer(i) => i.to_string(),
                rusqlite::types::Value::Text(t) => t,
                other => format!("{other:?}"),
            })
        })?)
    }

    pub fn create_note(&self, now_ms: i64) -> Result<NoteId, DbError> {
        self.conn.execute(
            "INSERT INTO notes(content, created_at, updated_at) VALUES ('', ?1, ?1)",
            [now_ms],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn save_content(&self, id: NoteId, content: &str, now_ms: i64) -> Result<(), DbError> {
        self.conn.execute(
            "UPDATE notes SET content = ?1, updated_at = ?2 WHERE id = ?3",
            rusqlite::params![content, now_ms, id],
        )?;
        Ok(())
    }

    pub fn content(&self, id: NoteId) -> Result<Option<String>, DbError> {
        use rusqlite::OptionalExtension;
        Ok(self
            .conn
            .query_row("SELECT content FROM notes WHERE id = ?1", [id], |r| r.get(0))
            .optional()?)
    }

    pub fn set_open(&self, id: NoteId, open: bool) -> Result<(), DbError> {
        self.conn
            .execute("UPDATE notes SET is_open = ?1 WHERE id = ?2", rusqlite::params![open as i64, id])?;
        Ok(())
    }

    const ROW: &'static str = "SELECT id, content, x, y, w, h, always_on_top FROM notes";

    fn row(r: &rusqlite::Row) -> rusqlite::Result<NoteRow> {
        let geometry = match (r.get::<_, Option<i32>>(2)?, r.get::<_, Option<i32>>(3)?, r.get::<_, Option<u32>>(4)?, r.get::<_, Option<u32>>(5)?) {
            (Some(x), Some(y), Some(w), Some(h)) => Some(Geometry { x, y, w, h }),
            _ => None,
        };
        Ok(NoteRow { id: r.get(0)?, content: r.get(1)?, geometry, always_on_top: r.get::<_, i64>(6)? != 0 })
    }

    pub fn set_always_on_top(&self, id: NoteId, on: bool) -> Result<(), DbError> {
        self.conn
            .execute("UPDATE notes SET always_on_top = ?1 WHERE id = ?2", rusqlite::params![on as i64, id])?;
        Ok(())
    }

    /// Notes with `is_open = 1`, oldest first.
    pub fn open_notes(&self) -> Result<Vec<NoteRow>, DbError> {
        let mut stmt = self.conn.prepare(&format!("{} WHERE is_open = 1 ORDER BY id", Self::ROW))?;
        let rows = stmt.query_map([], Self::row)?.collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    pub fn note(&self, id: NoteId) -> Result<Option<NoteRow>, DbError> {
        use rusqlite::OptionalExtension;
        Ok(self.conn.query_row(&format!("{} WHERE id = ?1", Self::ROW), [id], Self::row).optional()?)
    }

    /// Closed notes, most recently edited first.
    pub fn closed_notes(&self) -> Result<Vec<ClosedNote>, DbError> {
        let mut stmt = self.conn.prepare(
            "SELECT id, substr(content, 1, 200) FROM notes WHERE is_open = 0 ORDER BY updated_at DESC, id DESC",
        )?;
        let rows = stmt
            .query_map([], |r| Ok(ClosedNote { id: r.get(0)?, preview: r.get(1)? }))?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    pub fn save_geometry(&self, id: NoteId, g: Geometry) -> Result<(), DbError> {
        self.conn.execute(
            "UPDATE notes SET x = ?1, y = ?2, w = ?3, h = ?4 WHERE id = ?5",
            rusqlite::params![g.x, g.y, g.w, g.h, id],
        )?;
        Ok(())
    }

    /// Permanently remove a note and its images. `secure_delete` zero-fills the freed pages and
    /// the checkpoint empties the write-ahead log, so nothing of the note is left in the files.
    pub fn delete_note(&self, id: NoteId) -> Result<(), DbError> {
        self.conn.execute("DELETE FROM note_images WHERE note_id = ?1", [id])?;
        self.conn.execute("DELETE FROM notes WHERE id = ?1", [id])?;
        self.conn.query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |_| Ok(()))?;
        Ok(())
    }
}
