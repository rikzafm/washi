//! Confirmation dialog for deleting a note.
//!
//! Replaces the native Windows confirmation dialog with a themed Slint window
//! featuring washi tape, handwriting font, and tactile text actions.

use crate::note_window::DeleteNoteDialog;
use crate::platform::{NoteId, ScreenRect};
use slint::ComponentHandle;
use std::rc::Rc;

/// Sizing metrics of the delete dialog window (logical px).
pub const DIALOG_WIDTH: f32 = 392.0;
pub const DIALOG_HEIGHT: f32 = 252.0;

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
    /// The window's scale: the monitor's times the UI size.
    scale: Rc<crate::ui_scale::WindowScale>,
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
        let ui = DeleteNoteDialog::new()?;
        let scale = Rc::new(crate::ui_scale::WindowScale::attach(ui.window()));
        {
            // moved to a monitor with another DPI: put the UI size back on top
            let (weak, scale) = (ui.as_weak(), scale.clone());
            ui.on_scale_factor_changed(move || {
                if let Some(ui) = weak.upgrade() {
                    scale.resync(ui.window(), false);
                }
            });
        }
        let scale_factor = scale.factor();
        let dw = (DIALOG_WIDTH * scale_factor).round() as i32;
        let dh = (DIALOG_HEIGHT * scale_factor).round() as i32;
        let rect = crate::placement::center_over(target, (dw, dh), work_areas);
        ui.window().set_position(slint::PhysicalPosition::new(rect.x, rect.y));

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

        let dialog = Self { note_id, ui, scale };

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
        dialog.scale.resync(dialog.ui.window(), true);

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

    pub fn confirm(&self) {
        self.ui.invoke_confirm_delete();
    }

    pub fn cancel(&self) {
        self.ui.invoke_keep();
    }

    pub fn dispatch_event(&self, event: slint::platform::WindowEvent) {
        self.ui.window().dispatch_event(event);
    }

    pub fn hwnd(&self) -> Option<isize> {
        hwnd_of(&self.ui)
    }

    pub fn position(&self) -> (f32, f32) {
        let pos = self.ui.window().position();
        let scale = self.ui.window().scale_factor();
        (pos.x as f32 / scale, pos.y as f32 / scale)
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

