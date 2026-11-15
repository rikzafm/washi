//! Settings › Font & Style: the Font size and Letter spacing rows, the controls on the tab that are
//! saved and reach the notes (the others only change the tab's preview, see `ui/style-tab.slint`).

use crate::app::Env;
use crate::note_window::SettingsWindow;
use crate::storage::db::Choice;
use crate::storage::{Db, LetterSpacing};
use slint::ComponentHandle;
use std::rc::Rc;

/// Show the saved Font size and Letter spacing and save new choices, then hand them to the hooks
/// (open notes lay their text out again). Dragging the size slider only moves the preview; the size
/// is saved when the slider is released or the number committed.
pub fn wire(ui: &SettingsWindow, env: Env, db: Rc<Db>, spacing_changed: Rc<dyn Fn(f32)>, size_changed: Rc<dyn Fn(u32)>) {
    ui.set_style_spacing(db.letter_spacing().index());
    ui.set_style_size(db.font_size() as i32);
    let weak = ui.as_weak();
    ui.on_set_letter_spacing({
        let (db, env, weak) = (db.clone(), env.clone(), weak.clone());
        move |i| {
            let Some(spacing) = LetterSpacing::ALL.get(i as usize).copied() else { return };
            match db.set_letter_spacing(spacing) {
                Ok(()) => spacing_changed(spacing.em()),
                Err(e) => env.dialogs.error(&format!("Washi could not save that setting: {e}")),
            }
            if let Some(ui) = weak.upgrade() {
                ui.set_style_spacing(db.letter_spacing().index());
            }
        }
    });
    ui.on_set_font_size(move |n| {
        let n = (n.max(0) as u32).clamp(*crate::fonts::FONT_SIZE_RANGE.start(), *crate::fonts::FONT_SIZE_RANGE.end());
        match db.set_font_size(n) {
            Ok(()) => size_changed(n),
            Err(e) => env.dialogs.error(&format!("Washi could not save that setting: {e}")),
        }
        if let Some(ui) = weak.upgrade() {
            ui.set_style_size(db.font_size() as i32);
        }
    });
}
