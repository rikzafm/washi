//! The shortcuts the Shortcuts tab lists. `snap`'s `shortcuts` test presses every row on a real
//! note, so the table cannot drift from the bindings in `note.slint`.

use crate::note_window::ShortcutRow;
use slint::{ModelRc, SharedString, VecModel};

/// The table as the Slint tab's model.
pub fn model() -> ModelRc<ShortcutRow> {
    let rows: Vec<ShortcutRow> = SHORTCUTS
        .iter()
        .map(|s| ShortcutRow {
            label: s.label.into(),
            keys: ModelRc::new(VecModel::from(s.keys.iter().map(|k| SharedString::from(*k)).collect::<Vec<_>>())),
        })
        .collect();
    ModelRc::new(VecModel::from(rows))
}

pub struct Shortcut {
    pub label: &'static str,
    pub keys: &'static [&'static str],
}

pub const SHORTCUTS: &[Shortcut] = &[
    Shortcut { label: "New note", keys: &["Ctrl", "N"] },
    Shortcut { label: "Close note", keys: &["Ctrl", "W"] },
    Shortcut { label: "Delete note", keys: &["Ctrl", "D"] },
    Shortcut { label: "Bold", keys: &["Ctrl", "B"] },
    Shortcut { label: "Italic", keys: &["Ctrl", "I"] },
    Shortcut { label: "Underline", keys: &["Ctrl", "U"] },
    Shortcut { label: "Strikethrough", keys: &["Ctrl", "Shift", "X"] },
    Shortcut { label: "List", keys: &["Ctrl", "Shift", "L"] },
    Shortcut { label: "Select all", keys: &["Ctrl", "A"] },
    Shortcut { label: "Undo", keys: &["Ctrl", "Z"] },
    Shortcut { label: "Redo", keys: &["Ctrl", "Y"] },
];
