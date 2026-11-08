//! Dropdown menu floating window and lifecycle controller.
//!
//! Manages the frameless secondary `DropdownMenuWindow` anchored relative to
//! a note's top-right "..." button.

use crate::note_window::DropdownMenuWindow;
use crate::platform::NoteId;
use slint::ComponentHandle;
use std::rc::Rc;

/// A rectangle in logical coordinates.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl Rect {
    pub fn new(x: f32, y: f32, w: f32, h: f32) -> Self {
        Self { x, y, w, h }
    }

    pub fn right(&self) -> f32 {
        self.x + self.w
    }

    pub fn bottom(&self) -> f32 {
        self.y + self.h
    }
}

/// Sizing and layout metrics for the dropdown popup.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DropdownMetrics {
    pub card_width: f32,
    pub card_height: f32,
    pub shadow_margin: f32,
    pub gap: f32,
}

impl Default for DropdownMetrics {
    fn default() -> Self {
        Self {
            card_width: 260.0,
            card_height: 320.0,
            shadow_margin: 16.0,
            gap: 4.0,
        }
    }
}

/// Calculate the top-left (x, y) coordinates of the dropdown window in logical screen coordinates.
///
/// Anchors the card's right edge to the button's right edge, and places it immediately below
/// the button (`button.bottom() + gap`). If it overflows the bottom of the work area, flips above
/// the button. Clamps horizontally and vertically within the work area if provided.
/// The resulting position is offset by `shadow_margin` so the card aligns with the trigger.
pub fn calculate_dropdown_position(
    button_rect: Rect,
    metrics: &DropdownMetrics,
    work_area: Option<Rect>,
) -> (f32, f32) {
    let mut card_x = button_rect.right() - metrics.card_width;
    let mut card_y = button_rect.bottom() + metrics.gap;

    if let Some(wa) = work_area {
        // If placing below overflows the bottom of work area:
        if card_y + metrics.card_height > wa.bottom() {
            let candidate_y = button_rect.y - metrics.card_height - metrics.gap;
            if candidate_y >= wa.y {
                card_y = candidate_y;
            } else {
                card_y = (wa.bottom() - metrics.card_height).max(wa.y);
            }
        }

        // Clamp card horizontally:
        if card_x + metrics.card_width > wa.right() {
            card_x = wa.right() - metrics.card_width;
        }
        if card_x < wa.x {
            card_x = wa.x;
        }
    }

    (card_x - metrics.shadow_margin, card_y - metrics.shadow_margin)
}

/// Controller managing a live `DropdownMenuWindow`.
pub struct DropdownMenu {
    note_id: NoteId,
    ui: DropdownMenuWindow,
}

impl DropdownMenu {
    pub fn open(
        note_id: NoteId,
        position: (f32, f32),
        on_dismiss: Rc<dyn Fn(NoteId)>,
    ) -> Result<Self, slint::PlatformError> {
        let ui = DropdownMenuWindow::new()?;
        ui.window().set_position(slint::LogicalPosition::new(position.0, position.1));

        {
            let on_dismiss = on_dismiss.clone();
            ui.on_dismiss(move || {
                on_dismiss(note_id);
            });
        }
        {
            let on_dismiss = on_dismiss.clone();
            ui.window().on_close_requested(move || {
                on_dismiss(note_id);
                slint::CloseRequestResponse::HideWindow
            });
        }

        ui.show()?;
        Ok(Self { note_id, ui })
    }

    pub fn note_id(&self) -> NoteId {
        self.note_id
    }

    pub fn ui(&self) -> &DropdownMenuWindow {
        &self.ui
    }

    pub fn pinned(&self) -> bool {
        self.ui.get_pinned()
    }

    pub fn set_pinned(&self, pinned: bool) {
        self.ui.set_pinned(pinned);
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

    pub fn close(&mut self) {
        let _ = self.ui.hide();
    }
}

impl Drop for DropdownMenu {
    fn drop(&mut self) {
        let _ = self.ui.hide();
    }
}

#[cfg(test)]
pub(crate) fn init_test_platform() {
    struct TestPlatform;
    impl slint::platform::Platform for TestPlatform {
        fn create_window_adapter(&self) -> Result<Rc<dyn slint::platform::WindowAdapter>, slint::PlatformError> {
            Ok(slint::platform::software_renderer::MinimalSoftwareWindow::new(
                slint::platform::software_renderer::RepaintBufferType::NewBuffer,
            ))
        }
    }
    let _ = slint::platform::set_platform(Box::new(TestPlatform));
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    #[test]
    fn normal_positioning_anchors_below_and_right_aligned() {
        let metrics = DropdownMetrics::default();
        let button = Rect::new(500.0, 100.0, 28.0, 24.0);
        let work_area = Some(Rect::new(0.0, 0.0, 1920.0, 1080.0));

        let (win_x, win_y) = calculate_dropdown_position(button, &metrics, work_area);
        // button.right() = 528.0
        // card_x = 528.0 - 260.0 = 268.0
        // card_y = 100.0 + 24.0 + 4.0 = 128.0
        // win_x = card_x - 16.0 = 252.0
        // win_y = card_y - 16.0 = 112.0
        assert_eq!((win_x, win_y), (252.0, 112.0));
    }

    #[test]
    fn flips_above_when_overflowing_bottom() {
        let metrics = DropdownMetrics::default();
        let button = Rect::new(500.0, 900.0, 28.0, 24.0);
        let work_area = Some(Rect::new(0.0, 0.0, 1920.0, 1080.0));

        let (win_x, win_y) = calculate_dropdown_position(button, &metrics, work_area);
        // Candidate below: 900 + 24 + 4 + 320 = 1248 > 1080.
        // Candidate above: 900 - 320 - 4 = 576.0 (>= 0).
        // win_x = 252.0
        // win_y = 576.0 - 16.0 = 560.0
        assert_eq!((win_x, win_y), (252.0, 560.0));
    }

    #[test]
    fn clamps_to_work_area_left_edge() {
        let metrics = DropdownMetrics::default();
        let button = Rect::new(100.0, 100.0, 28.0, 24.0);
        let work_area = Some(Rect::new(0.0, 0.0, 1920.0, 1080.0));

        let (win_x, win_y) = calculate_dropdown_position(button, &metrics, work_area);
        // card_right = 128.0 -> card_x = 128 - 260 = -132 < 0. Clamped to 0.
        // win_x = 0 - 16.0 = -16.0
        // win_y = 112.0
        assert_eq!((win_x, win_y), (-16.0, 112.0));
    }

    #[test]
    fn clamps_to_work_area_right_edge() {
        let metrics = DropdownMetrics::default();
        let button = Rect::new(850.0, 100.0, 28.0, 24.0);
        let work_area = Some(Rect::new(0.0, 0.0, 800.0, 600.0));

        let (win_x, _win_y) = calculate_dropdown_position(button, &metrics, work_area);
        // card_x clamped to 800 - 260 = 540.
        // win_x = 540 - 16 = 524.0
        assert_eq!(win_x, 524.0);
    }

    #[test]
    fn no_work_area_calculates_without_clamping() {
        let metrics = DropdownMetrics::default();
        let button = Rect::new(100.0, 900.0, 28.0, 24.0);

        let (win_x, win_y) = calculate_dropdown_position(button, &metrics, None);
        assert_eq!((win_x, win_y), (-148.0, 912.0));
    }

    #[test]
    fn flips_and_clamps_when_work_area_is_small() {
        let metrics = DropdownMetrics::default();
        let button = Rect::new(100.0, 100.0, 28.0, 24.0);
        let work_area = Some(Rect::new(0.0, 0.0, 800.0, 200.0));
        let (_win_x, win_y) = calculate_dropdown_position(button, &metrics, work_area);
        assert_eq!(win_y, -16.0);
    }

    #[test]
    fn dropdown_menu_window_creation_and_dismissal() {
        init_test_platform();
        let ui = DropdownMenuWindow::new().expect("DropdownMenuWindow::new");
        assert!(!ui.get_pinned());
        ui.set_pinned(true);
        assert!(ui.get_pinned());

        let dismissed = Rc::new(Cell::new(false));
        let dismissed_clone = dismissed.clone();
        ui.on_dismiss(move || {
            dismissed_clone.set(true);
        });

        ui.invoke_dismiss();
        assert!(dismissed.get());
    }

    #[test]
    fn dropdown_menu_window_callbacks_and_pinned_property() {
        init_test_platform();
        let ui = DropdownMenuWindow::new().expect("DropdownMenuWindow::new");
        assert!(!ui.get_pinned());
        ui.set_pinned(true);
        assert!(ui.get_pinned());

        let pin_toggled = Rc::new(Cell::new(false));
        let pt = pin_toggled.clone();
        ui.on_toggle_pin(move || {
            pt.set(true);
        });
        ui.invoke_toggle_pin();
        assert!(pin_toggled.get());

        let note_deleted = Rc::new(Cell::new(false));
        let nd = note_deleted.clone();
        ui.on_delete_note(move || {
            nd.set(true);
        });
        ui.invoke_delete_note();
        assert!(note_deleted.get());

        let new_created = Rc::new(Cell::new(false));
        let nc = new_created.clone();
        ui.on_new_note(move || {
            nc.set(true);
        });
        ui.invoke_new_note();
        assert!(new_created.get());
    }

    #[test]
    fn dropdown_menu_pinned_state_helpers() {
        init_test_platform();
        let menu = DropdownMenu::open(
            99,
            (100.0, 200.0),
            Rc::new(|_| ()),
        )
        .expect("DropdownMenu::open");

        assert!(!menu.pinned());
        menu.set_pinned(true);
        assert!(menu.pinned());
        menu.set_pinned(false);
        assert!(!menu.pinned());
    }

    #[test]
    fn dropdown_menu_lifecycle_and_note_id() {
        init_test_platform();
        let dismissed = Rc::new(Cell::new(false));
        let dismissed_clone = dismissed.clone();
        let menu = DropdownMenu::open(
            42,
            (100.0, 200.0),
            Rc::new(move |id| {
                if id == 42 {
                    dismissed_clone.set(true);
                }
            }),
        )
        .expect("DropdownMenu::open");

        assert_eq!(menu.note_id(), 42);
        menu.ui().invoke_dismiss();
        assert!(dismissed.get());
    }
}
