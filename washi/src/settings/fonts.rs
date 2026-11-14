//! Settings › Font & Style › the Font row and the Font manager's list (see `ui/font-manager.slint`).
//!
//! The Slint side draws whatever it is given: the Font row's name and summary
//! (`SettingsWindow::set_font_name` / `set_font_summary`) and one [`FontListRow`] per Font in the
//! slip (`set_manager_fonts`), each with the `top` and `height` it takes in the list. [`model`]
//! stacks [`RowSpec`]s into that list, so a row taller than the rest (an expanded one) only says how
//! much `extra` room it needs. Pressing a row, its "Remove" and the install area arrive as the
//! window's `on_manager_row_clicked`, `on_manager_remove_clicked` and `on_manager_install_clicked`.
//! The slip itself (open, close, its strip in the window) is `SettingsView::toggle_manager`.

use crate::fonts::DEFAULT_TEXT_FAMILY;
use crate::note_window::{FontListRow, SettingsWindow};
use slint::{ModelRc, VecModel};

/// A row of the list with nothing extra under it, in logical px.
pub const ROW_H: f32 = 64.0;

/// What one row of the Font manager's list shows.
#[derive(Clone, Debug, PartialEq)]
pub struct RowSpec {
    /// The family name, drawn in `family` (the Font's own face).
    pub name: String,
    pub family: String,
    /// "One style", "Variable: weight, width, slant".
    pub subtitle: String,
    /// The Font in use: its radio dot is filled.
    pub active: bool,
    /// Cannot be removed (the Default font): shows a lock and its `tag`.
    pub locked: bool,
    /// At the row's right: "Default" or "Remove".
    pub tag: String,
    /// Room under the row's own `ROW_H` (an expanded row's Axis panel), logical px.
    pub extra: f32,
}

/// The Default font's row (Shadows Into Light, bundled).
pub fn default_row(active: bool) -> RowSpec {
    RowSpec {
        name: DEFAULT_TEXT_FAMILY.into(),
        family: DEFAULT_TEXT_FAMILY.into(),
        subtitle: "One style".into(),
        active,
        locked: true,
        tag: "Default".into(),
        extra: 0.0,
    }
}

/// The rows stacked top to bottom, as the slip's list model.
pub fn model(rows: &[RowSpec]) -> ModelRc<FontListRow> {
    let mut top = 0.0;
    let list: Vec<FontListRow> = rows
        .iter()
        .map(|r| {
            let height = ROW_H + r.extra;
            let row = FontListRow {
                name: r.name.as_str().into(),
                family: r.family.as_str().into(),
                subtitle: r.subtitle.as_str().into(),
                active: r.active,
                locked: r.locked,
                tag: r.tag.as_str().into(),
                top,
                height,
            };
            top += height;
            row
        })
        .collect();
    ModelRc::new(VecModel::from(list))
}

/// Show the Font in use on the Font row and the Default font in the Font manager's list.
pub fn wire(ui: &SettingsWindow) {
    let row = default_row(true);
    ui.set_font_name(row.name.as_str().into());
    ui.set_font_summary(row.subtitle.as_str().into());
    ui.set_manager_fonts(model(&[row]));
}

#[cfg(test)]
mod tests {
    use super::*;
    use slint::Model;

    #[test]
    fn rows_stack_with_their_extra_room() {
        let mut open = default_row(true);
        open.extra = 100.0;
        let list = model(&[open, default_row(false), default_row(false)]);
        let tops: Vec<(f32, f32)> = list.iter().map(|r| (r.top, r.height)).collect();
        assert_eq!(tops, vec![(0.0, 164.0), (164.0, 64.0), (228.0, 64.0)]);
    }
}
