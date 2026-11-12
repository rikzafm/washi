//! The right-click menu of a note: a frameless paper card at the pointer (see `ui/context-menu.slint`).
//!
//! The card opens with its top-left corner at the click. Near the right or bottom edge of the screen it
//! opens to the left of the pointer, or above it. Opening the Tape color or Recently closed list makes
//! the card taller, and it is put back on screen again.

use crate::platform::{NoteId, ScreenRect};
use crate::ui_scale::WindowScale;
use slint::ComponentHandle;
use std::rc::Rc;

pub use crate::note_window::{ClosedItem, ContextMenuWindow};

/// Width of the card, logical px.
pub const CARD_WIDTH: f32 = 352.0;
/// Transparent margin around the card that holds its shadow, logical px.
pub const SHADOW_MARGIN: f32 = 16.0;

/// Where the card goes (physical px): its top-left corner at `at` unless it would leave `areas`.
/// `card` is the card's size without the shadow margin.
pub fn place_card(at: (i32, i32), card: (i32, i32), areas: &[ScreenRect]) -> (i32, i32) {
    let Some(area) = crate::placement::find_work_area(at, areas) else { return at };
    let (mut x, mut y) = at;
    if x + card.0 > area.right() {
        // like a Windows menu: to the left of the pointer when it fits there, else against the edge
        x = if at.0 - card.0 >= area.x { at.0 - card.0 } else { area.right() - card.0 };
    }
    if y + card.1 > area.bottom() {
        y = if at.1 - card.1 >= area.y { at.1 - card.1 } else { area.bottom() - card.1 };
    }
    (x.max(area.x), y.max(area.y))
}

/// Controller of a live `ContextMenuWindow`.
pub struct ContextMenu {
    note_id: NoteId,
    ui: ContextMenuWindow,
    scale: Rc<WindowScale>,
}

fn put_on_screen(ui: &ContextMenuWindow, scale: f32, at: (i32, i32), areas: &[ScreenRect]) {
    let margin = (SHADOW_MARGIN * scale).round() as i32;
    let card = ((CARD_WIDTH * scale).round() as i32, (ui.get_card_height() * scale).round() as i32);
    let (x, y) = place_card(at, card, areas);
    ui.window().set_size(slint::PhysicalSize::new((card.0 + 2 * margin) as u32, (card.1 + 2 * margin) as u32));
    ui.window().set_position(slint::PhysicalPosition::new(x - margin, y - margin));
}

impl ContextMenu {
    /// Open the menu of note `note_id` at screen position `at` (physical px). `on_dismiss` runs when the
    /// user closes the menu without choosing (Escape, a click outside).
    pub fn open(
        note_id: NoteId,
        at: (i32, i32),
        work_areas: &[ScreenRect],
        owner_hwnd: Option<isize>,
        on_dismiss: Rc<dyn Fn(NoteId)>,
    ) -> Result<Self, slint::PlatformError> {
        let ui = ContextMenuWindow::new()?;
        let scale = Rc::new(WindowScale::attach(ui.window()));
        let areas: Rc<Vec<ScreenRect>> = Rc::new(work_areas.to_vec());
        {
            let (weak, scale) = (ui.as_weak(), scale.clone());
            ui.on_scale_factor_changed(move || {
                if let Some(ui) = weak.upgrade() {
                    scale.resync(ui.window(), false);
                }
            });
        }
        {
            let (weak, scale, areas) = (ui.as_weak(), scale.clone(), areas.clone());
            ui.on_layout_changed(move || {
                if let Some(ui) = weak.upgrade() {
                    put_on_screen(&ui, scale.factor(), at, &areas);
                }
            });
        }
        {
            let on_dismiss = on_dismiss.clone();
            ui.on_dismiss(move || on_dismiss(note_id));
        }
        ui.window().on_close_requested(move || {
            on_dismiss(note_id);
            slint::CloseRequestResponse::HideWindow
        });
        ui.set_chip(crate::settings::tape::menu_chip_shape());

        put_on_screen(&ui, scale.factor(), at, &areas);
        ui.show()?;
        scale.resync(ui.window(), true);
        // the scale may have changed with the first show: place the card again at the real size
        put_on_screen(&ui, scale.factor(), at, &areas);
        let menu = Self { note_id, ui, scale };
        #[cfg(target_os = "windows")]
        if let Some(owner) = owner_hwnd {
            menu.set_owner(owner);
        }
        #[cfg(not(target_os = "windows"))]
        let _ = owner_hwnd;
        menu.ensure_on_top();
        Ok(menu)
    }

    pub fn note_id(&self) -> NoteId {
        self.note_id
    }

    pub fn ui(&self) -> &ContextMenuWindow {
        &self.ui
    }

    pub fn scale(&self) -> &Rc<WindowScale> {
        &self.scale
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

    /// Make the note the owner of the menu window, so the menu stays above it.
    #[cfg(target_os = "windows")]
    fn set_owner(&self, owner_hwnd: isize) {
        if let Some(hwnd) = self.hwnd() {
            use windows_sys::Win32::Foundation::HWND;
            use windows_sys::Win32::UI::WindowsAndMessaging::{SetWindowLongPtrW, GWLP_HWNDPARENT};
            // SAFETY: both handles are live windows of this process.
            unsafe {
                SetWindowLongPtrW(hwnd as HWND, GWLP_HWNDPARENT, owner_hwnd);
            }
        }
    }

    fn ensure_on_top(&self) {
        #[cfg(target_os = "windows")]
        if let Some(hwnd) = self.hwnd() {
            use windows_sys::Win32::Foundation::HWND;
            use windows_sys::Win32::UI::WindowsAndMessaging::{
                IsWindowVisible, SetWindowPos, HWND_TOPMOST, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE,
            };
            // SAFETY: `hwnd` is this menu's own live window.
            unsafe {
                let hwnd = hwnd as HWND;
                if IsWindowVisible(hwnd) != 0 {
                    SetWindowPos(hwnd, HWND_TOPMOST, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
                }
            }
        }
    }

    pub fn position(&self) -> (i32, i32) {
        let p = self.ui.window().position();
        (p.x, p.y)
    }

    pub fn size(&self) -> (u32, u32) {
        let s = self.ui.window().size();
        (s.width, s.height)
    }
}

impl Drop for ContextMenu {
    fn drop(&mut self) {
        let _ = self.ui.hide();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SCREEN: ScreenRect = ScreenRect { x: 0, y: 0, w: 1920, h: 1080 };

    #[test]
    fn the_card_opens_at_the_pointer() {
        assert_eq!(place_card((300, 200), (352, 194), &[SCREEN]), (300, 200));
    }

    #[test]
    fn near_the_right_edge_it_opens_to_the_left_of_the_pointer() {
        assert_eq!(place_card((1800, 200), (352, 194), &[SCREEN]), (1448, 200));
    }

    #[test]
    fn near_the_bottom_edge_it_opens_above_the_pointer() {
        assert_eq!(place_card((300, 1000), (352, 194), &[SCREEN]), (300, 806));
    }

    #[test]
    fn on_a_screen_too_small_it_is_pushed_against_the_edge() {
        let small = ScreenRect::new(0, 0, 300, 150);
        assert_eq!(place_card((100, 100), (352, 194), &[small]), (0, 0));
    }

    #[test]
    fn without_a_work_area_it_stays_at_the_pointer() {
        assert_eq!(place_card((-40, 900), (352, 194), &[]), (-40, 900));
    }
}
