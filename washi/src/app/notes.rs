//! Note lifecycle: create, reopen, activate, close, delete, export, quit.

use super::{App, NEW_NOTE_OFFSET};
use crate::note_window::note_title;
use crate::platform::NoteId;
use crate::storage::{Geometry, NoteRow};
use crate::suspend::SuspendHost;
use slint::ComponentHandle;
use std::time::Duration;

impl App {
    /// Close a note: save it, remember where it was, mark it closed and release its window.
    /// Closing the last open note quits Washi.
    pub fn close_note(&self, id: NoteId) {
        self.dismiss_dropdown_if(id);
        self.dismiss_delete_dialog_if(id);
        let Some((was_empty, geometry)) = self.with_note(id, |n| (n.text().trim().is_empty(), n.geometry())) else {
            return;
        };
        self.0.record_closed_candidate(id, was_empty, geometry);

        // an empty note is not worth keeping: closing it discards it instead of saving it
        if was_empty && self.0.db.delete_note(id).is_ok() {
            self.0.save_stacking();
            self.remove_note(id);
            if let Some(v) = self.all_notes() {
                v.on_desktop_note_closed(id);
            }
            return;
        }
        self.with_note(id, |n| {
            n.flush();
            n.prune_images();
            if let Err(e) = self.0.db.save_geometry(id, n.geometry()) {
                eprintln!("washi: could not save the position of note {id}: {e}");
            }
        });
        self.0.save_stacking();
        if let Err(e) = self.0.db.set_open(id, false) {
            eprintln!("washi: could not mark note {id} closed: {e}");
        }
        self.remove_note(id);
        if let Some(v) = self.all_notes() {
            v.on_desktop_note_closed(id);
        }
    }

    /// Delete a note for good from the database and remove its window.
    pub fn confirm_delete_note(&self, id: NoteId) {
        self.0.remove_exit_candidate(id);
        if let Err(e) = self.0.db.delete_note(id) {
            self.0.env.dialogs.error(&format!("Washi could not delete the note: {e}"));
            return;
        }
        self.remove_note(id);
        if let Some(v) = self.all_notes() {
            v.on_note_deleted(id);
        }
    }

    pub(super) fn create_new_note(&self, from: Option<NoteId>) -> Option<NoteId> {
        // Offset from the note it is opened from, but at the new-note size for the UI size (not that
        // note's own size).
        let (w, h) = crate::note_window::new_note_window_size();
        let geometry = from
            .and_then(|from_id| self.with_note(from_id, |n| n.geometry()))
            .map(|g| Geometry { x: g.x + NEW_NOTE_OFFSET, y: g.y + NEW_NOTE_OFFSET, w, h });
        let id = match self.0.db.create_note(self.0.env.clock.unix_millis()) {
            Ok(id) => id,
            Err(e) => {
                self.0.env.dialogs.error(&format!("Washi could not create a note: {e}"));
                return None;
            }
        };
        if let Some(g) = geometry {
            let _ = self.0.db.save_geometry(id, g);
        }
        let on_top = self.0.db.apply_new_note_defaults(id);
        if let Err(e) = self.open_row(&NoteRow { id, content: String::new(), geometry, always_on_top: on_top }) {
            self.0.env.dialogs.error(&e.to_string());
            return None;
        }
        Some(id)
    }

    /// Open a fresh empty note. If an open note exists, offsets from it.
    pub fn new_note(&self) {
        let from = self.dropdown().map(|d| d.note_id()).or_else(|| self.0.notes.borrow().last().map(|n| n.id()));
        if let Some(new_id) = self.create_new_note(from) {
            if let Some(v) = self.all_notes() {
                v.on_desktop_note_opened(new_id);
            }
        }
    }

    /// Open an empty note, offset from `from`.
    pub fn new_note_from(&self, from: NoteId) {
        if let Some(new_id) = self.create_new_note(Some(from)) {
            if let Some(v) = self.all_notes() {
                v.on_desktop_note_opened(new_id);
            }
        }
    }

    /// Reopen a closed note where it was.
    pub fn reopen_note(&self, id: NoteId) {
        if self.with_note(id, |_| ()).is_some() {
            return;
        }
        self.0.remove_exit_candidate(id);
        let Ok(Some(row)) = self.0.db.note(id) else { return };
        if self.0.db.set_open(id, true).is_ok() {
            if let Err(e) = self.open_row(&row) {
                self.0.env.dialogs.error(&e.to_string());
            }
            if let Some(v) = self.all_notes() {
                v.on_desktop_note_opened(id);
            }
        }
    }

    /// Raise an open note to front or reopen it if closed.
    pub fn activate_note(&self, id: NoteId) {
        let note = self.0.notes.borrow().iter().find(|n| n.id() == id).cloned();
        if let Some(note) = note {
            let _ = note.ui().show();
            let hwnd = note.hwnd();
            self.0.env.window_control.bring_to_front(id, hwnd);
            if let Some(v) = self.all_notes() {
                v.on_desktop_note_opened(id);
            }
        } else {
            self.reopen_note(id);
        }
    }

    /// Keep a note's window above the others (or not), and remember it.
    pub fn set_always_on_top(&self, id: NoteId, on: bool) {
        self.with_note(id, |n| n.set_always_on_top(on));
        if let Some(d) = self.dropdown() {
            if d.note_id() == id {
                d.set_pinned(on);
                d.ensure_on_top();
                let d_clone = d.clone();
                slint::Timer::single_shot(std::time::Duration::ZERO, move || {
                    d_clone.ensure_on_top();
                });
                let d_clone2 = d.clone();
                slint::Timer::single_shot(std::time::Duration::from_millis(20), move || {
                    d_clone2.ensure_on_top();
                });
            }
        }
        if let Err(e) = self.0.db.set_always_on_top(id, on) {
            eprintln!("washi: could not save always-on-top for note {id}: {e}");
        }
    }

    /// "Export as Markdown...": a save dialog, then the note as a plain .md file with its pictures
    /// as PNG files beside it.
    pub fn export_note(&self, id: NoteId) {
        let Some((text, title)) = self
            .with_note(id, |n| {
                n.flush();
                (n.text(), n.title())
            })
            .or_else(|| {
                let text = self.0.db.content(id).ok().flatten()?;
                let title = note_title(&text).unwrap_or_else(|| "Washi Note".to_string());
                Some((text, title))
            })
        else {
            return;
        };
        let Some(path) = self.0.env.dialogs.pick_save_file(&crate::export::suggested_name(&title), "Markdown|*.md") else { return };
        if let Err(e) = crate::export::write(&self.0.db, &text, &path) {
            self.0.env.dialogs.error(&format!("Washi could not export the note: {e}"));
        }
    }

    /// Take a note out of the app. Its window is released on the next turn of the event loop,
    /// because this may run inside one of that window's own callbacks.
    fn remove_note(&self, id: NoteId) {
        let closed = self.0.popups.dismiss_all_of(id);
        self.dropdown_closed(closed);
        let (note, now_empty) = {
            let mut notes = self.0.notes.borrow_mut();
            let at = notes.iter().position(|n| n.id() == id);
            (at.map(|i| notes.remove(i)), notes.is_empty())
        };
        if let Some(note) = note {
            self.0.env.signals.detach(id);
            slint::Timer::single_shot(Duration::ZERO, move || drop(note));
            self.0.suspend.evaluate();
            if now_empty && self.all_notes().is_none() {
                self.0.restore_exit_candidates();
                self.0.quit.set(true);
                let _ = slint::quit_event_loop();
            }
        }
    }

    /// Exit for the uninstaller that was just started: the same exit as closing the last note, so
    /// open notes come back if Washi is installed again.
    pub fn quit_for_uninstall(&self) {
        self.0.save_all();
        self.0.restore_exit_candidates();
        self.0.quit.set(true);
        let _ = slint::quit_event_loop();
    }

    /// Save everything immediately. Notes stay open, so the next start reopens them.
    pub fn quit(&self) {
        self.0.save_all();
        self.0.quit.set(true);
    }
}
