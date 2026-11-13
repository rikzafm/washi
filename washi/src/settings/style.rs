//! Settings › Font & Style: the Letter spacing row, the one control on the tab that is saved and
//! reaches the notes (the others only change the tab's preview, see `ui/style-tab.slint`).

use crate::app::Env;
use crate::note_window::SettingsWindow;
use crate::storage::db::Choice;
use crate::storage::{Db, LetterSpacing};
use slint::ComponentHandle;
use std::rc::Rc;

/// Show the saved Letter spacing and save a new choice, then hand its em to `changed` (open notes
/// lay their text out again).
pub fn wire(ui: &SettingsWindow, env: Env, db: Rc<Db>, changed: Rc<dyn Fn(f32)>) {
    ui.set_style_spacing(db.letter_spacing().index());
    let weak = ui.as_weak();
    ui.on_set_letter_spacing(move |i| {
        let Some(spacing) = LetterSpacing::ALL.get(i as usize).copied() else { return };
        match db.set_letter_spacing(spacing) {
            Ok(()) => changed(spacing.em()),
            Err(e) => env.dialogs.error(&format!("Washi could not save that setting: {e}")),
        }
        if let Some(ui) = weak.upgrade() {
            ui.set_style_spacing(db.letter_spacing().index());
        }
    });
}
