//! The app: opens the encrypted store, shows the open notes, and owns their lifecycle.

use crate::note_window::Note;
use crate::platform::*;
use crate::storage::{keystore, Db, DbError, KeyStoreError};
use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::rc::Rc;
use std::time::Duration;

/// Everything Washi takes from the outside world. Real Windows pieces in `crate::win`,
/// fakes in the `snap` harness.
#[derive(Clone)]
pub struct Env {
    /// Folder holding `notes.db` and `key.bin` (`%APPDATA%\Washi` in the real app).
    pub data_dir: PathBuf,
    pub protector: Rc<dyn KeyProtector>,
    pub clock: Rc<dyn Clock>,
    pub signals: Rc<dyn WindowSignals>,
    pub power: Rc<dyn ProcessPower>,
    pub menu: Rc<dyn ContextMenu>,
    pub dialogs: Rc<dyn Dialogs>,
}

#[derive(Debug)]
pub enum StartError {
    Key(KeyStoreError),
    Db(DbError),
    Io(std::io::Error),
    Ui(slint::PlatformError),
}

impl std::fmt::Display for StartError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StartError::Key(e) => e.fmt(f),
            StartError::Db(e) => e.fmt(f),
            StartError::Io(e) => write!(f, "Washi could not prepare its data folder: {e}"),
            StartError::Ui(e) => write!(f, "Washi could not open a note window: {e}"),
        }
    }
}
impl std::error::Error for StartError {}

struct Inner {
    env: Env,
    db: Rc<Db>,
    notes: RefCell<Vec<Note>>,
    quit: Cell<bool>,
}

#[derive(Clone)]
pub struct App(Rc<Inner>);

impl App {
    pub fn start(env: Env) -> Result<App, StartError> {
        std::fs::create_dir_all(&env.data_dir).map_err(StartError::Io)?;
        let now = env.clock.unix_millis();
        let key = keystore::load_or_create(&env.data_dir.join("key.bin"), &*env.protector).map_err(StartError::Key)?;
        let db = Db::open(&env.data_dir.join("notes.db"), &key, now).map_err(StartError::Db)?;
        drop(key); // zeroized: the raw key is wiped as soon as the database is unlocked
        let db = Rc::new(db);

        let mut rows = db.open_notes().map_err(StartError::Db)?;
        if rows.is_empty() {
            let id = db.create_note(now).map_err(StartError::Db)?;
            rows.push(crate::storage::NoteRow { id, content: String::new() });
        }
        let app = App(Rc::new(Inner { env, db, notes: RefCell::new(Vec::new()), quit: Cell::new(false) }));
        for row in rows {
            let note = app.open_note(row.id, &row.content)?;
            app.0.notes.borrow_mut().push(note);
        }
        Ok(app)
    }

    fn open_note(&self, id: NoteId, content: &str) -> Result<Note, StartError> {
        let weak = Rc::downgrade(&self.0);
        let on_close: Rc<dyn Fn(NoteId)> = Rc::new(move |id| {
            if let Some(inner) = weak.upgrade() {
                App(inner).window_close_requested(id);
            }
        });
        Note::open(id, content, self.0.db.clone(), self.0.env.clock.clone(), on_close).map_err(StartError::Ui)
    }

    pub fn env(&self) -> &Env {
        &self.0.env
    }

    pub fn note_ids(&self) -> Vec<NoteId> {
        self.0.notes.borrow().iter().map(|n| n.id()).collect()
    }

    pub fn with_note<R>(&self, id: NoteId, f: impl FnOnce(&Note) -> R) -> Option<R> {
        self.0.notes.borrow().iter().find(|n| n.id() == id).map(f)
    }

    /// The text last written to the database for a note.
    pub fn saved_text(&self, id: NoteId) -> Option<String> {
        self.0.db.content(id).ok().flatten()
    }

    pub fn schema_version(&self) -> u32 {
        self.0.db.schema_version().unwrap_or(0)
    }

    pub fn pragma(&self, name: &str) -> String {
        self.0.db.pragma_string(name).unwrap_or_default()
    }

    pub fn has_quit(&self) -> bool {
        self.0.quit.get()
    }

    /// The user closed a note window: save it now, mark it closed, drop the window afterwards.
    fn window_close_requested(&self, id: NoteId) {
        self.mark_closed(id);
        let app = self.clone();
        slint::Timer::single_shot(Duration::ZERO, move || app.remove_note(id));
    }

    /// Close a note immediately (saves first). Closing the last note quits Washi.
    pub fn close_note(&self, id: NoteId) {
        self.mark_closed(id);
        self.remove_note(id);
    }

    fn mark_closed(&self, id: NoteId) {
        self.with_note(id, |n| n.flush());
        if let Err(e) = self.0.db.set_open(id, false) {
            eprintln!("washi: could not mark note {id} closed: {e}");
        }
    }

    fn remove_note(&self, id: NoteId) {
        let removed = {
            let mut notes = self.0.notes.borrow_mut();
            let before = notes.len();
            notes.retain(|n| n.id() != id);
            before != notes.len()
        };
        if removed && self.0.notes.borrow().is_empty() {
            self.0.quit.set(true);
            let _ = slint::quit_event_loop();
        }
    }

    /// Save everything immediately. Notes stay open, so the next start reopens them.
    pub fn quit(&self) {
        for n in self.0.notes.borrow().iter() {
            n.flush();
        }
        self.0.quit.set(true);
    }
}
