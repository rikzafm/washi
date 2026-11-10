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

/// An image as stored: encoded bytes plus what the database knows about them.
pub struct StoredImage {
    pub mime: String,
    pub width: u32,
    pub height: u32,
    pub data: Vec<u8>,
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

    /// Where an open note goes when All Notes switches away from it (Settings › General): beside the
    /// All Notes window when there is room (true, the default) or straight behind it (false).
    pub fn park_notes_beside(&self) -> bool {
        self.conn
            .query_row("SELECT value FROM meta WHERE key='park_notes'", [], |r| r.get::<_, String>(0))
            .map(|v| v != "back")
            .unwrap_or(true)
    }

    pub fn set_park_notes_beside(&self, beside: bool) -> Result<(), DbError> {
        self.conn.execute(
            "INSERT INTO meta(key, value) VALUES ('park_notes', ?1)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            [if beside { "side" } else { "back" }],
        )?;
        Ok(())
    }

    /// The global default for new notes' Always on top (Settings › General). Off when never set.
    pub fn new_notes_on_top(&self) -> bool {
        self.conn
            .query_row("SELECT value FROM meta WHERE key='new_notes_on_top'", [], |r| r.get::<_, String>(0))
            .map(|v| v == "1")
            .unwrap_or(false)
    }

    pub fn set_new_notes_on_top(&self, on: bool) -> Result<(), DbError> {
        self.conn.execute(
            "INSERT INTO meta(key, value) VALUES ('new_notes_on_top', ?1)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            [if on { "1" } else { "0" }],
        )?;
        Ok(())
    }

    /// A stored app-wide preference (`meta` table), if it was ever set.
    fn meta(&self, key: &str) -> Option<String> {
        self.conn.query_row("SELECT value FROM meta WHERE key = ?1", [key], |r| r.get::<_, String>(0)).ok()
    }

    /// Store an app-wide preference (`meta` table), replacing any earlier value.
    pub(crate) fn set_meta(&self, key: &str, value: &str) -> Result<(), DbError> {
        self.conn.execute(
            "INSERT INTO meta(key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            [key, value],
        )?;
        Ok(())
    }

    /// The UI size (Settings › Appearance) as a scale factor, one of `UI_SCALES`. Large (1.0) when
    /// never set or when the stored value is not one of the choices.
    pub fn ui_scale(&self) -> f32 {
        self.meta("ui_scale")
            .and_then(|v| v.trim().parse::<f32>().ok())
            .and_then(|v| UI_SCALES.iter().copied().find(|s| (s - v).abs() < 1e-3))
            .unwrap_or(1.0)
    }

    pub fn set_ui_scale(&self, scale: f32) -> Result<(), DbError> {
        self.set_meta("ui_scale", &format!("{scale:.2}"))
    }

    /// Apply the new-note defaults to a note just created and return whether it starts Always on
    /// top. The preference write is best effort: a failure is logged and never stops a note opening.
    pub fn apply_new_note_defaults(&self, id: NoteId) -> bool {
        let on_top = self.new_notes_on_top();
        if on_top {
            if let Err(e) = self.set_always_on_top(id, true) {
                eprintln!("washi: could not save always-on-top for new note {id}: {e}");
            }
        }
        on_top
    }

    pub fn set_always_on_top(&self, id: NoteId, on: bool) -> Result<(), DbError> {
        self.conn
            .execute("UPDATE notes SET always_on_top = ?1 WHERE id = ?2", rusqlite::params![on as i64, id])?;
        Ok(())
    }

    /// Notes with `is_open = 1`, bottom of the stack first (then oldest first).
    pub fn open_notes(&self) -> Result<Vec<NoteRow>, DbError> {
        let mut stmt = self.conn.prepare(&format!("{} WHERE is_open = 1 ORDER BY z_order, id", Self::ROW))?;
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

    /// Remember the stacking of the open notes: `bottom_to_top[0]` is the lowest window.
    pub fn save_stacking(&self, bottom_to_top: &[NoteId]) -> Result<(), DbError> {
        let mut stmt = self.conn.prepare("UPDATE notes SET z_order = ?1 WHERE id = ?2")?;
        for (rank, id) in bottom_to_top.iter().enumerate() {
            stmt.execute(rusqlite::params![rank as i64, id])?;
        }
        Ok(())
    }

    pub fn save_geometry(&self, id: NoteId, g: Geometry) -> Result<(), DbError> {
        self.conn.execute(
            "UPDATE notes SET x = ?1, y = ?2, w = ?3, h = ?4 WHERE id = ?5",
            rusqlite::params![g.x, g.y, g.w, g.h, id],
        )?;
        Ok(())
    }

    /// Store an encoded image for a note; returns the id the note text refers to (`![](img:ID)`).
    pub fn add_image(&self, note: NoteId, mime: &str, width: u32, height: u32, data: &[u8]) -> Result<i64, DbError> {
        self.conn.execute(
            "INSERT INTO note_images(note_id, mime, width, height, data) VALUES (?1, ?2, ?3, ?4, ?5)",
            rusqlite::params![note, mime, width, height, data],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    /// A stored image, if there is one with this id.
    pub fn image(&self, id: i64) -> Result<Option<StoredImage>, DbError> {
        use rusqlite::OptionalExtension;
        Ok(self
            .conn
            .query_row("SELECT mime, width, height, data FROM note_images WHERE id = ?1", [id], |r| {
                Ok(StoredImage { mime: r.get(0)?, width: r.get(1)?, height: r.get(2)?, data: r.get(3)? })
            })
            .optional()?)
    }

    /// Width and height of a stored image, without reading its bytes.
    pub fn image_size(&self, id: i64) -> Result<Option<(u32, u32)>, DbError> {
        use rusqlite::OptionalExtension;
        Ok(self
            .conn
            .query_row("SELECT width, height FROM note_images WHERE id = ?1", [id], |r| Ok((r.get(0)?, r.get(1)?)))
            .optional()?)
    }

    pub fn image_count(&self) -> Result<usize, DbError> {
        Ok(self.conn.query_row("SELECT count(*) FROM note_images", [], |r| r.get::<_, i64>(0))? as usize)
    }

    /// Remove the images of a note that its text no longer refers to (an image that was deleted
    /// from the text stays stored until now, so that undo can bring it back).
    pub fn prune_images(&self, note: NoteId, keep: &[i64]) -> Result<(), DbError> {
        let mut stmt = self.conn.prepare("SELECT id FROM note_images WHERE note_id = ?1")?;
        let ids = stmt.query_map([note], |r| r.get::<_, i64>(0))?.collect::<Result<Vec<_>, _>>()?;
        for id in ids.into_iter().filter(|id| !keep.contains(id)) {
            self.conn.execute("DELETE FROM note_images WHERE id = ?1", [id])?;
        }
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

    /// Other notes ordered by most recently updated first, excluding `exclude_id`.
    pub fn recent_notes(&self, exclude_id: NoteId) -> Result<Vec<RecentNote>, DbError> {
        let mut stmt = self.conn.prepare(
            "SELECT id, content, tape_style, updated_at, is_open FROM notes WHERE id != ?1 ORDER BY updated_at DESC, id DESC",
        )?;
        let rows = stmt
            .query_map([exclude_id], |r| {
                Ok(RecentNote {
                    id: r.get(0)?,
                    content: r.get(1)?,
                    tape_style: r.get(2)?,
                    updated_at: r.get(3)?,
                    is_open: r.get::<_, i64>(4)? != 0,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// All notes (open first ordered by `z_order DESC, updated_at DESC, id DESC`, then closed ordered by `updated_at DESC, id DESC`).
    pub fn all_notes(&self) -> Result<Vec<AllNoteRow>, DbError> {
        let mut stmt = self.conn.prepare(
            "SELECT id, content, updated_at, is_open, z_order FROM notes
             ORDER BY is_open DESC, CASE WHEN is_open != 0 THEN z_order ELSE 0 END DESC, updated_at DESC, id DESC",
        )?;
        let rows = stmt
            .query_map([], |r| {
                Ok(AllNoteRow {
                    id: r.get(0)?,
                    content: r.get(1)?,
                    updated_at: r.get(2)?,
                    is_open: r.get::<_, i64>(3)? != 0,
                    z_order: r.get(4)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AllNoteRow {
    pub id: NoteId,
    pub content: String,
    pub updated_at: i64,
    pub is_open: bool,
    pub z_order: i64,
}

/// The UI size choices (Settings › Appearance: Small, Medium, Large), as the scale each one saves.
pub const UI_SCALES: [f32; 3] = [0.75, 0.80, 1.00];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecentNote {
    pub id: NoteId,
    pub content: String,
    pub tape_style: i64,
    pub updated_at: i64,
    /// The note has a window on the desktop right now (the menu marks these with a tape).
    pub is_open: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_db() -> (tempfile::TempDir, Db) {
        let dir = tempfile::tempdir().unwrap();
        let db = Db::open(&dir.path().join("test.db"), &[0x42; 32], 1000).unwrap();
        (dir, db)
    }

    #[test]
    fn new_note_defaults_follow_the_preference() {
        let (_dir, db) = test_db();
        let a = db.create_note(1).unwrap();
        assert!(!db.apply_new_note_defaults(a));
        assert!(!db.open_notes().unwrap().iter().any(|r| r.always_on_top));
        db.set_new_notes_on_top(true).unwrap();
        let b = db.create_note(2).unwrap();
        assert!(db.apply_new_note_defaults(b));
        let rows = db.open_notes().unwrap();
        assert!(rows.iter().find(|r| r.id == b).unwrap().always_on_top);
        assert!(!rows.iter().find(|r| r.id == a).unwrap().always_on_top);
    }

    #[test]
    fn new_notes_on_top_is_off_until_set_and_survives_reopening() {
        let (dir, db) = test_db();
        assert!(!db.new_notes_on_top());
        db.set_new_notes_on_top(true).unwrap();
        assert!(db.new_notes_on_top());
        drop(db);
        let db = Db::open(&dir.path().join("test.db"), &[0x42; 32], 2000).unwrap();
        assert!(db.new_notes_on_top());
        db.set_new_notes_on_top(false).unwrap();
        assert!(!db.new_notes_on_top());
    }

    #[test]
    fn ui_scale_is_large_until_set_survives_reopening_and_falls_back_on_nonsense() {
        let (dir, db) = test_db();
        assert_eq!(db.ui_scale(), 1.0, "never set: Large");
        db.set_ui_scale(0.75).unwrap();
        assert_eq!(db.ui_scale(), 0.75);
        drop(db);
        let db = Db::open(&dir.path().join("test.db"), &[0x42; 32], 2000).unwrap();
        assert_eq!(db.ui_scale(), 0.75, "kept across a restart");
        db.set_ui_scale(0.8).unwrap();
        assert_eq!(db.ui_scale(), 0.8);
        for junk in ["", "big", "NaN", "inf", "-1", "0", "0.5", "1.5"] {
            db.set_meta("ui_scale", junk).unwrap();
            assert_eq!(db.ui_scale(), 1.0, "{junk:?} reads as Large");
        }
    }

    #[test]
    fn recent_notes_excludes_active_and_orders_by_updated_at_desc() {
        let (_dir, db) = test_db();
        let id1 = db.create_note(1000).unwrap();
        db.save_content(id1, "Note 1", 1000).unwrap();

        let id2 = db.create_note(2000).unwrap();
        db.save_content(id2, "Note 2", 2000).unwrap();

        let id3 = db.create_note(3000).unwrap();
        db.save_content(id3, "Note 3", 3000).unwrap();

        let recent = db.recent_notes(id2).unwrap();
        assert_eq!(recent.len(), 2);
        assert_eq!(recent[0].id, id3);
        assert_eq!(recent[0].content, "Note 3");
        assert_eq!(recent[0].updated_at, 3000);
        assert_eq!(recent[1].id, id1);
        assert_eq!(recent[1].content, "Note 1");
        assert_eq!(recent[1].updated_at, 1000);
    }

    #[test]
    fn all_notes_returns_open_then_closed_ordered_appropriately() {
        let (_dir, db) = test_db();
        let id1 = db.create_note(1000).unwrap();
        db.save_content(id1, "Open Low Z", 1000).unwrap();

        let id2 = db.create_note(2000).unwrap();
        db.save_content(id2, "Closed Old", 2000).unwrap();
        db.set_open(id2, false).unwrap();

        let id3 = db.create_note(1500).unwrap();
        db.save_content(id3, "Open High Z", 1500).unwrap();
        db.save_stacking(&[id1, id3]).unwrap();

        let id4 = db.create_note(3000).unwrap();
        db.save_content(id4, "Closed New", 3000).unwrap();
        db.set_open(id4, false).unwrap();

        let all = db.all_notes().unwrap();
        assert_eq!(all.len(), 4);
        assert_eq!(all[0].id, id3);
        assert_eq!(all[0].content, "Open High Z");
        assert_eq!(all[0].updated_at, 1500);
        assert!(all[0].is_open);
        assert_eq!(all[0].z_order, 1);

        assert_eq!(all[1].id, id1);
        assert_eq!(all[1].content, "Open Low Z");
        assert_eq!(all[1].updated_at, 1000);
        assert!(all[1].is_open);
        assert_eq!(all[1].z_order, 0);

        assert_eq!(all[2].id, id4);
        assert_eq!(all[2].content, "Closed New");
        assert_eq!(all[2].updated_at, 3000);
        assert!(!all[2].is_open);

        assert_eq!(all[3].id, id2);
        assert_eq!(all[3].content, "Closed Old");
        assert_eq!(all[3].updated_at, 2000);
        assert!(!all[3].is_open);
    }
}
