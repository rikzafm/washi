//! The All notes window, and what it asks of the desktop notes.

use super::App;
use crate::all_notes::AllNotesView;
use crate::note_window::app_icon;
use crate::platform::NoteId;
use crate::storage::Geometry;
use slint::ComponentHandle;
use std::rc::Rc;

impl App {
    /// Clone the handle to the open All Notes view, if any.
    pub fn all_notes(&self) -> Option<Rc<AllNotesView>> {
        self.0.all_notes.borrow().clone()
    }

    /// Open All Notes from note `initial_selected_id`: its window grows to the left for the list.
    pub fn open_all_notes(&self, initial_selected_id: NoteId) {
        if let Some(v) = self.all_notes() {
            v.refresh_rows(Some(initial_selected_id));
            let _ = v.ui().show();
            self.0.env.window_control.bring_to_front(0, v.hwnd());
            return;
        }

        let weak = Rc::downgrade(&self.0);
        let on_close = Rc::new(move || {
            if let Some(inner) = weak.upgrade() {
                App(inner).dismiss_all_notes();
            }
        });

        let (db, clock, icon, window_control, clipboard) = (
            self.0.db.clone(),
            self.0.env.clock.clone(),
            app_icon(),
            self.0.env.window_control.clone(),
            self.0.env.clipboard.clone(),
        );

        match AllNotesView::open(
            initial_selected_id,
            db,
            clock,
            icon,
            window_control,
            clipboard,
            Rc::new(self.clone()),
            on_close,
        ) {
            Ok(view) => {
                *self.0.all_notes.borrow_mut() = Some(view);
            }
            Err(e) => eprintln!("washi: could not open All Notes: {e}"),
        }
    }

    /// Close All Notes: the window gives the sidebar's strip back and the host note keeps the
    /// place and size its paper has now.
    pub fn dismiss_all_notes(&self) {
        let Some(v) = self.0.all_notes.borrow_mut().take() else { return };
        let (host, paper) = (v.host_note_id(), v.paper_geometry());
        v.unpark_notes();
        v.close();
        if self.with_note(host, |n| {
            n.set_saves_geometry(true);
            n.place(paper);
        }).is_some() {
            let hwnd = self.with_note(host, |n| n.hwnd()).flatten();
            self.0.env.window_control.bring_to_front(host, hwnd);
        }
        if self.0.notes.borrow().is_empty() {
            self.0.restore_exit_candidates();
            self.0.quit.set(true);
            let _ = slint::quit_event_loop();
        }
    }

    /// Give the open All notes Sidebar the Height and Anchor Settings › Appearance stores for it.
    pub(super) fn apply_notes_sidebar_layout(&self) {
        if let Some(v) = self.all_notes() {
            v.apply_sidebar_layout();
        }
    }
}

impl crate::all_notes::AllNotesDelegate for App {
    fn desktop_note_geometry(&self, id: NoteId) -> Option<Geometry> {
        self.with_note(id, |n| n.geometry())
    }

    fn desktop_note_hwnd(&self, id: NoteId) -> Option<isize> {
        self.with_note(id, |n| n.hwnd()).flatten()
    }

    fn set_desktop_note_visible(&self, id: NoteId, visible: bool) {
        let show = visible && !self.settings().is_some_and(|s| s.host() == id);
        self.with_note(id, |n| n.set_visible(show));
    }

    fn park_desktop_note(&self, id: NoteId, at: (i32, i32)) {
        if self.settings().is_some_and(|s| s.host() == id) {
            return;
        }
        self.with_note(id, |n| {
            n.set_saves_geometry(false);
            n.ui().window().set_position(slint::LogicalPosition::new(at.0 as f32, at.1 as f32));
            n.set_visible(true);
        });
    }

    fn restore_desktop_note(&self, id: NoteId, at: Geometry, home: Geometry) {
        self.with_note(id, |n| {
            n.set_saves_geometry(true);
            let now = n.geometry();
            if (now.x, now.y) != (at.x, at.y) {
                n.geometry_changed();
            } else if at != home {
                n.place(home);
            }
        });
    }

    fn desktop_note_text(&self, id: NoteId) -> Option<String> {
        self.with_note(id, |n| n.text())
    }

    fn set_desktop_note_text(&self, id: NoteId, text: &str) {
        self.with_note(id, |n| {
            if n.text() != text {
                n.set_text(text);
            }
        });
    }

    fn flush_open_notes(&self) {
        for n in self.0.notes.borrow().iter() {
            n.flush_text();
        }
        self.0.save_stacking();
    }

    fn open_desktop_note(&self, id: NoteId) {
        self.activate_note(id);
    }

    fn close_desktop_note(&self, id: NoteId) {
        self.close_note(id);
    }

    fn create_new_note(&self, from: Option<NoteId>) -> Option<NoteId> {
        self.create_new_note(from)
    }

    fn delete_note(&self, id: NoteId) {
        self.confirm_delete_note(id);
    }

    fn export_note(&self, id: NoteId) {
        self.export_note(id);
    }

    fn toggle_dropdown(&self, _id: NoteId) {
        self.toggle_editor_dropdown();
    }

    fn dismiss_dropdown(&self) {
        self.dismiss_dropdown();
    }

    fn poke_suspend(&self) {
        self.0.suspend.poke();
    }

    fn is_settings_host(&self, id: NoteId) -> bool {
        self.settings().is_some_and(|s| s.host() == id)
    }
}
