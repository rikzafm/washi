//! Confirmation dialog for deleting a note.
//!
//! Replaces the native Windows confirmation dialog with a themed Slint window
//! featuring washi tape, handwriting font, and tactile text actions. The window's lifecycle is the
//! [`Popup`]'s; this module is its content and its centred placement.

use crate::note_window::DeleteNoteDialog;
use crate::platform::{NoteId, ScreenRect};
use crate::popup::{hwnd_of, Popup};
use slint::ComponentHandle;
use std::rc::Rc;

/// Sizing metrics of the delete dialog window (logical px).
pub const DIALOG_WIDTH: f32 = 392.0;
pub const DIALOG_HEIGHT: f32 = 252.0;

/// Controller managing a live `DeleteNoteDialog` window.
pub struct DeleteDialog {
    note_id: NoteId,
    popup: Popup<DeleteNoteDialog>,
}

impl DeleteDialog {
    pub fn open(
        note_id: NoteId,
        target: ScreenRect,
        work_areas: &[ScreenRect],
        owner_hwnd: Option<isize>,
        on_keep: Rc<dyn Fn(NoteId)>,
        on_confirm: Rc<dyn Fn(NoteId)>,
        on_start_move: Rc<dyn Fn(Option<isize>)>,
    ) -> Result<Self, slint::PlatformError> {
        let popup = Popup::new(DeleteNoteDialog::new()?);
        let ui = popup.ui();
        crate::fonts::current().apply(&ui.global::<crate::note_window::Theme>());
        let scale_factor = popup.scale().factor();
        let dw = (DIALOG_WIDTH * scale_factor).round() as i32;
        let dh = (DIALOG_HEIGHT * scale_factor).round() as i32;
        let rect = crate::placement::center_over(target, (dw, dh), work_areas);
        ui.window().set_position(slint::PhysicalPosition::new(rect.x, rect.y));

        {
            let on_keep = on_keep.clone();
            ui.on_keep(move || on_keep(note_id));
        }
        ui.on_confirm_delete(move || on_confirm(note_id));
        {
            // Look the handle up at drag time: it does not exist until the window is shown.
            let weak = ui.as_weak();
            ui.on_start_move(move || {
                if let Some(ui) = weak.upgrade() {
                    on_start_move(hwnd_of(ui.window()));
                }
            });
        }

        popup.show(owner_hwnd, move || on_keep(note_id))?;
        Ok(Self { note_id, popup })
    }

    pub fn note_id(&self) -> NoteId {
        self.note_id
    }

    pub fn ui(&self) -> &DeleteNoteDialog {
        self.popup.ui()
    }

    pub fn confirm(&self) {
        self.popup.ui().invoke_confirm_delete();
    }

    pub fn cancel(&self) {
        self.popup.ui().invoke_keep();
    }

    pub fn dispatch_event(&self, event: slint::platform::WindowEvent) {
        self.popup.dispatch_event(event);
    }

    pub fn hwnd(&self) -> Option<isize> {
        self.popup.hwnd()
    }

    pub fn close(&self) {
        self.popup.close();
    }

    pub fn position(&self) -> (f32, f32) {
        self.popup.logical_position()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    #[test]
    fn delete_dialog_controller_lifecycle_placement_and_semantic_actions() {
        let kept = Rc::new(Cell::new(false));
        let confirmed = Rc::new(Cell::new(false));

        let k = kept.clone();
        let on_keep = Rc::new(move |_id| {
            k.set(true);
        });

        let c = confirmed.clone();
        let on_confirm = Rc::new(move |_id| {
            c.set(true);
        });

        let on_start_move = Rc::new(|_| {});

        let target = ScreenRect::new(100, 100, 500, 400);
        let work_areas = [ScreenRect::new(0, 0, 1920, 1080)];

        let dialog = DeleteDialog::open(42, target, &work_areas, None, on_keep, on_confirm, on_start_move)
            .expect("DeleteDialog::open");

        assert_eq!(dialog.note_id(), 42);
        assert_eq!(dialog.ui().get_title_text(), "Delete this note?");
        assert_eq!(dialog.ui().get_message_text(), "You can't get it back.");

        // Target center: (100 + 250, 100 + 200) = (350, 300)
        // Dialog size: 392 x 252 (at 100% scale)
        // Expected top-left: (350 - 196, 300 - 126) = (154, 174)
        let (x, y) = dialog.position();
        assert_eq!((x.round() as i32, y.round() as i32), (154, 174));

        // Semantic cancel()
        dialog.cancel();
        assert!(kept.get());

        // Semantic confirm()
        dialog.confirm();
        assert!(confirmed.get());
    }
}

