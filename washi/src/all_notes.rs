//! All Notes ("Notes") window and lifecycle controller.

use crate::dropdown::extract_title_and_preview;
use crate::note_window::{AllNoteItem, AllNotesWindow};
use crate::platform::NoteId;
use crate::storage::AllNoteRow;
use slint::ComponentHandle;
use std::rc::Rc;

pub const ALL_NOTES_WIDTH: f32 = 603.0;
pub const ALL_NOTES_HEIGHT: f32 = 625.0;

pub struct AllNotesView {
    ui: AllNotesWindow,
}

impl AllNotesView {
    pub fn open(
        initial_selected_id: NoteId,
        rows: &[AllNoteRow],
        on_close: Rc<dyn Fn()>,
    ) -> Result<Self, slint::PlatformError> {
        let ui = AllNotesWindow::new()?;

        {
            let on_close = on_close.clone();
            ui.on_close_window(move || {
                on_close();
            });
        }
        {
            let on_close = on_close.clone();
            ui.window().on_close_requested(move || {
                on_close();
                slint::CloseRequestResponse::HideWindow
            });
        }

        let view = Self { ui };
        view.refresh(rows, Some(initial_selected_id));
        view.ui.show()?;
        Ok(view)
    }

    pub fn refresh(&self, rows: &[AllNoteRow], preferred_selected_id: Option<NoteId>) {
        let mut open_items = Vec::new();
        let mut closed_items = Vec::new();

        for r in rows {
            let (title, _) = extract_title_and_preview(&r.content);
            let item = AllNoteItem {
                id: r.id as i32,
                title: title.into(),
                is_open: r.is_open,
            };
            if r.is_open {
                open_items.push(item);
            } else {
                closed_items.push(item);
            }
        }

        self.ui.set_open_count(open_items.len() as i32);
        self.ui.set_closed_count(closed_items.len() as i32);
        self.ui
            .set_open_notes(slint::ModelRc::from(Rc::new(slint::VecModel::from(open_items))));
        self.ui
            .set_closed_notes(slint::ModelRc::from(Rc::new(slint::VecModel::from(closed_items))));

        let current_selected = self.selected_note_id();
        let target_id = preferred_selected_id
            .filter(|&id| rows.iter().any(|r| r.id == id))
            .or_else(|| current_selected.filter(|&id| rows.iter().any(|r| r.id == id)))
            .or_else(|| rows.first().map(|r| r.id));

        match target_id {
            Some(id) => {
                self.ui.set_selected_note_id(id as i32);
                let is_open = rows.iter().find(|r| r.id == id).is_some_and(|r| r.is_open);
                self.ui.set_selected_note_open(is_open);
            }
            None => {
                self.ui.set_selected_note_id(-1);
                self.ui.set_selected_note_open(false);
            }
        }
    }

    pub fn ui(&self) -> &AllNotesWindow {
        &self.ui
    }

    pub fn selected_note_id(&self) -> Option<NoteId> {
        let id = self.ui.get_selected_note_id();
        (id >= 0).then_some(id as NoteId)
    }

    pub fn open_count(&self) -> usize {
        self.ui.get_open_count().max(0) as usize
    }

    pub fn closed_count(&self) -> usize {
        self.ui.get_closed_count().max(0) as usize
    }

    pub fn open_notes(&self) -> Vec<AllNoteItem> {
        use slint::Model;
        self.ui.get_open_notes().iter().collect()
    }

    pub fn closed_notes(&self) -> Vec<AllNoteItem> {
        use slint::Model;
        self.ui.get_closed_notes().iter().collect()
    }

    pub fn confirming_delete(&self) -> bool {
        self.ui.get_confirming_delete()
    }

    pub fn close(&self) {
        let _ = self.ui.hide();
    }

    pub fn hwnd(&self) -> Option<isize> {
        use raw_window_handle::{HasWindowHandle, RawWindowHandle};
        let window_handle = self.ui.window().window_handle();
        let handle = window_handle.window_handle().ok()?;
        match handle.as_raw() {
            RawWindowHandle::Win32(w) => Some(w.hwnd.get()),
            _ => None,
        }
    }

    pub fn dispatch_event(&self, event: slint::platform::WindowEvent) {
        self.ui.window().dispatch_event(event);
    }
}

impl Drop for AllNotesView {
    fn drop(&mut self) {
        let _ = self.ui.hide();
    }
}
