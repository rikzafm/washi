//! The right-click menu of a note: a frameless paper card at the pointer (see `ui/context-menu.slint`).
//!
//! The card opens with its top-left corner at the click. Near the right or bottom edge of the screen it
//! opens to the left of the pointer, or above it (`placement::place_card`). Opening the Tape color or
//! Recently closed list makes the card taller, and it is put back on screen again.

use crate::placement::place_card;
use crate::platform::{NoteId, ScreenRect};
use crate::popup::Popup;
use crate::ui_scale::WindowScale;
use slint::ComponentHandle;
use std::rc::Rc;

pub use crate::note_window::{ClosedItem, ContextMenuWindow};

/// Width of the card, logical px.
pub const CARD_WIDTH: f32 = 212.0;
/// Transparent margin around the card that holds its shadow, logical px.
pub const SHADOW_MARGIN: f32 = 16.0;

/// Controller of a live `ContextMenuWindow`.
pub struct ContextMenu {
    note_id: NoteId,
    popup: Popup<ContextMenuWindow>,
}

fn put_on_screen(ui: &ContextMenuWindow, scale: &WindowScale, at: (i32, i32), areas: &[ScreenRect]) {
    let sf = scale.factor();
    let margin = (SHADOW_MARGIN * sf).round() as i32;
    let card = ((CARD_WIDTH * sf).round() as i32, (ui.get_card_height() * sf).round() as i32);
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
        let popup = Popup::new(ContextMenuWindow::new()?);
        let ui = popup.ui();
        let areas: Rc<Vec<ScreenRect>> = Rc::new(work_areas.to_vec());
        {
            let (weak, scale, areas) = (ui.as_weak(), popup.scale().clone(), areas.clone());
            ui.on_layout_changed(move || {
                if let Some(ui) = weak.upgrade() {
                    put_on_screen(&ui, &scale, at, &areas);
                }
            });
        }
        {
            let on_dismiss = on_dismiss.clone();
            ui.on_dismiss(move || on_dismiss(note_id));
        }
        ui.set_chip(crate::settings::tape::menu_chip_shape());

        put_on_screen(ui, popup.scale(), at, &areas);
        popup.show(owner_hwnd, move || on_dismiss(note_id))?;
        // the scale may have changed with the first show: place the card again at the real size
        put_on_screen(popup.ui(), popup.scale(), at, &areas);
        Ok(Self { note_id, popup })
    }

    /// Lift the menu back above every other window (a note that was just pinned is topmost too).
    pub fn ensure_on_top(&self) {
        self.popup.raise();
    }

    /// Keep lifting the menu for a moment. The note's new topmost state lands at a moment we can't
    /// see (it differs between notes), so a few fixed raises can miss it and the menu shows covered.
    /// Raising every few milliseconds puts it back within a frame whenever that happens.
    pub fn keep_on_top_briefly(self: &Rc<Self>) {
        self.ensure_on_top();
        let menu = Rc::downgrade(self);
        let ticks = std::cell::Cell::new(0u32);
        let timer = Rc::new(slint::Timer::default());
        let handle = timer.clone();
        timer.start(slint::TimerMode::Repeated, std::time::Duration::from_millis(4), move || {
            ticks.set(ticks.get() + 1);
            match menu.upgrade() {
                Some(m) if ticks.get() <= 50 => m.ensure_on_top(),
                _ => handle.stop(),
            }
        });
    }

    pub fn note_id(&self) -> NoteId {
        self.note_id
    }

    pub fn ui(&self) -> &ContextMenuWindow {
        self.popup.ui()
    }

    pub fn scale(&self) -> &Rc<WindowScale> {
        self.popup.scale()
    }

    pub fn close(&self) {
        self.popup.close();
    }

    pub fn hwnd(&self) -> Option<isize> {
        self.popup.hwnd()
    }

    pub fn position(&self) -> (i32, i32) {
        self.popup.position()
    }

    pub fn size(&self) -> (u32, u32) {
        self.popup.size()
    }
}

