//! One open note: the Slint window plus its 500 ms autosave debounce.

use crate::platform::{Clock, NoteId};
use crate::storage::Db;
use slint::{CloseRequestResponse, ComponentHandle, Timer, TimerMode};
use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;

slint::include_modules!();

/// Quiet period after the last edit before the note is written to the database.
pub const AUTOSAVE_DELAY: Duration = Duration::from_millis(500);

struct Autosave {
    id: NoteId,
    db: Rc<Db>,
    clock: Rc<dyn Clock>,
    ui: slint::Weak<NoteWindow>,
    dirty: Cell<bool>,
}

impl Autosave {
    fn flush(&self) {
        if !self.dirty.replace(false) {
            return;
        }
        let Some(ui) = self.ui.upgrade() else { return };
        if let Err(e) = self.db.save_content(self.id, ui.get_text().as_str(), self.clock.unix_millis()) {
            eprintln!("washi: autosave of note {} failed: {e}", self.id);
            self.dirty.set(true);
        }
    }
}

pub struct Note {
    id: NoteId,
    ui: NoteWindow,
    autosave: Rc<Autosave>,
    timer: Rc<Timer>,
}

impl Note {
    /// Create and show the window. `on_close` runs when the user closes the window.
    pub fn open(
        id: NoteId,
        content: &str,
        db: Rc<Db>,
        clock: Rc<dyn Clock>,
        on_close: Rc<dyn Fn(NoteId)>,
    ) -> Result<Note, slint::PlatformError> {
        let ui = NoteWindow::new()?;
        ui.set_text(content.into());
        let autosave = Rc::new(Autosave { id, db, clock, ui: ui.as_weak(), dirty: Cell::new(false) });
        let timer = Rc::new(Timer::default());
        {
            let (autosave, timer) = (autosave.clone(), timer.clone());
            ui.on_edited(move |_| {
                autosave.dirty.set(true);
                let a = autosave.clone();
                timer.start(TimerMode::SingleShot, AUTOSAVE_DELAY, move || a.flush());
            });
        }
        ui.window().on_close_requested(move || {
            on_close(id);
            CloseRequestResponse::HideWindow
        });
        ui.show()?;
        Ok(Note { id, ui, autosave, timer })
    }

    pub fn id(&self) -> NoteId {
        self.id
    }

    pub fn ui(&self) -> &NoteWindow {
        &self.ui
    }

    pub fn text(&self) -> String {
        self.ui.get_text().to_string()
    }

    /// Save now if there are unsaved edits (used on close and quit).
    pub fn flush(&self) {
        self.timer.stop();
        self.autosave.flush();
    }
}
