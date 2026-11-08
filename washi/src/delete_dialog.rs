//! Confirmation dialog for deleting a note.
//!
//! Replaces the native Windows confirmation dialog with a themed Slint window
//! featuring washi tape, handwriting font, and tactile text actions.

use crate::note_window::DeleteNoteDialog;
use crate::platform::NoteId;
use slint::ComponentHandle;
use std::rc::Rc;

/// Sizing metrics of the delete dialog window (logical px).
pub const DIALOG_WIDTH: f32 = 392.0;
pub const DIALOG_HEIGHT: f32 = 252.0;

/// Calculate the top-left position of the delete dialog so it is centered directly
/// over the note being deleted, clamped within the work area if known.
pub fn calculate_dialog_position(
    note_geom: (i32, i32, u32, u32),
    work_area: Option<(f32, f32, f32, f32)>,
) -> (f32, f32) {
    let (nx, ny, nw, nh) = note_geom;
    let mut x = nx as f32 + (nw as f32 - DIALOG_WIDTH) / 2.0;
    let mut y = ny as f32 + (nh as f32 - DIALOG_HEIGHT) / 2.0;

    if let Some((wax, way, waw, wah)) = work_area {
        if x + DIALOG_WIDTH > wax + waw {
            x = wax + waw - DIALOG_WIDTH;
        }
        if x < wax {
            x = wax;
        }
        if y + DIALOG_HEIGHT > way + wah {
            y = way + wah - DIALOG_HEIGHT;
        }
        if y < way {
            y = way;
        }
    }

    (x, y)
}

fn hwnd_of(ui: &DeleteNoteDialog) -> Option<isize> {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    let window_handle = ui.window().window_handle();
    let handle = window_handle.window_handle().ok()?;
    match handle.as_raw() {
        RawWindowHandle::Win32(w) => Some(w.hwnd.get()),
        _ => None,
    }
}

/// Controller managing a live `DeleteNoteDialog` window.
pub struct DeleteDialog {
    note_id: NoteId,
    ui: DeleteNoteDialog,
}

impl DeleteDialog {
    pub fn open(
        note_id: NoteId,
        position: (f32, f32),
        owner_hwnd: Option<isize>,
        on_keep: Rc<dyn Fn(NoteId)>,
        on_confirm: Rc<dyn Fn(NoteId)>,
        on_start_move: Rc<dyn Fn(Option<isize>)>,
    ) -> Result<Self, slint::PlatformError> {
        let ui = DeleteNoteDialog::new()?;
        ui.window().set_position(slint::LogicalPosition::new(position.0, position.1));

        {
            let on_keep = on_keep.clone();
            ui.on_keep(move || {
                on_keep(note_id);
            });
        }
        {
            let on_confirm = on_confirm.clone();
            ui.on_confirm_delete(move || {
                on_confirm(note_id);
            });
        }
        {
            let on_keep = on_keep.clone();
            ui.window().on_close_requested(move || {
                on_keep(note_id);
                slint::CloseRequestResponse::HideWindow
            });
        }

        let dialog = Self { note_id, ui };

        {
            // Look the handle up at drag time: it does not exist until the window is shown.
            let weak = dialog.ui.as_weak();
            dialog.ui.on_start_move(move || {
                if let Some(ui) = weak.upgrade() {
                    on_start_move(hwnd_of(&ui));
                }
            });
        }

        dialog.ui.show()?;

        #[cfg(target_os = "windows")]
        if let Some(owner) = owner_hwnd {
            dialog.set_owner(owner);
        }
        dialog.ensure_on_top();

        Ok(dialog)
    }

    pub fn note_id(&self) -> NoteId {
        self.note_id
    }

    pub fn ui(&self) -> &DeleteNoteDialog {
        &self.ui
    }

    pub fn invoke_keep(&self) {
        self.ui.invoke_keep();
    }

    pub fn invoke_confirm_delete(&self) {
        self.ui.invoke_confirm_delete();
    }

    pub fn hwnd(&self) -> Option<isize> {
        hwnd_of(&self.ui)
    }

    pub fn set_owner(&self, owner_hwnd: isize) {
        #[cfg(target_os = "windows")]
        if let Some(hwnd) = self.hwnd() {
            use windows_sys::Win32::Foundation::HWND;
            use windows_sys::Win32::UI::WindowsAndMessaging::{SetWindowLongPtrW, GWLP_HWNDPARENT};
            unsafe {
                SetWindowLongPtrW(
                    hwnd as HWND,
                    GWLP_HWNDPARENT,
                    owner_hwnd as isize,
                );
            }
        }
    }

    pub fn ensure_on_top(&self) {
        #[cfg(target_os = "windows")]
        if let Some(hwnd) = self.hwnd() {
            use windows_sys::Win32::Foundation::HWND;
            use windows_sys::Win32::UI::WindowsAndMessaging::{
                IsWindowVisible, SetWindowPos, HWND_TOPMOST, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE,
            };
            unsafe {
                let hwnd = hwnd as HWND;
                if IsWindowVisible(hwnd) != 0 {
                    SetWindowPos(
                        hwnd,
                        HWND_TOPMOST,
                        0,
                        0,
                        0,
                        0,
                        SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
                    );
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    #[test]
    fn dialog_position_centers_over_note() {
        let note_geom = (100, 100, 500, 400);
        let pos = calculate_dialog_position(note_geom, None);
        // Note center: (100 + 250, 100 + 200) = (350, 300)
        // Dialog top-left: (350 - 196, 300 - 126) = (154, 174)
        assert_eq!(pos, (154.0, 174.0));
    }

    #[test]
    fn dialog_position_clamps_to_work_area() {
        let note_geom = (0, 0, 300, 300);
        let work_area = Some((50.0, 50.0, 1920.0, 1080.0));
        let pos = calculate_dialog_position(note_geom, work_area);
        // Center would put top-left at (150 - 240, 150 - 170) = (-90, -20)
        // Clamped to work area (50, 50)
        assert_eq!(pos, (50.0, 50.0));
    }

    #[test]
    fn delete_dialog_callbacks_and_properties() {
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

        let dialog = DeleteDialog::open(42, (100.0, 100.0), None, on_keep, on_confirm, on_start_move)
            .expect("DeleteDialog::open");

        assert_eq!(dialog.note_id(), 42);
        assert_eq!(dialog.ui().get_title_text(), "Delete this note?");
        assert_eq!(dialog.ui().get_message_text(), "You can't get it back.");

        dialog.invoke_keep();
        assert!(kept.get());

        dialog.invoke_confirm_delete();
        assert!(confirmed.get());
    }
}

