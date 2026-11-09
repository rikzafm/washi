//! Settings › Appearance: app-wide look-and-feel preferences, kept in the database `meta` table.
//!
//! The tab only shows what is stored and reports choices: every value is read back from the database
//! after a write, so a failed write shows the truth. Writes are best effort: a failure is logged and
//! never blocks the UI.

use crate::note_window::SettingsWindow;
use crate::storage::Db;
use slint::ComponentHandle;
use std::rc::Rc;

pub use crate::storage::db::UI_SCALES;

/// The dropdown index of a stored scale (Large for anything that is not one of the choices).
pub fn ui_size_index(scale: f32) -> i32 {
    UI_SCALES.iter().position(|s| (s - scale).abs() < 1e-3).unwrap_or(UI_SCALES.len() - 1) as i32
}

pub struct Appearance {
    ui: slint::Weak<SettingsWindow>,
    db: Rc<Db>,
}

impl Appearance {
    pub fn new(ui: &SettingsWindow, db: Rc<Db>) -> Rc<Appearance> {
        let appearance = Rc::new(Appearance { ui: ui.as_weak(), db });
        let me = Rc::downgrade(&appearance);
        ui.on_set_ui_size(move |i| {
            if let Some(a) = me.upgrade() {
                a.set_ui_size(i);
            }
        });
        appearance.refresh();
        appearance
    }

    fn set_ui_size(&self, i: i32) {
        let Some(&scale) = UI_SCALES.get(i as usize) else { return };
        if let Err(e) = self.db.set_ui_scale(scale) {
            eprintln!("washi: could not save the UI size: {e}");
        }
        self.refresh();
    }

    /// Show what is stored.
    fn refresh(&self) {
        if let Some(ui) = self.ui.upgrade() {
            ui.set_ui_size(ui_size_index(self.db.ui_scale()));
        }
    }
}
