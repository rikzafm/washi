//! Harness helpers for files dragged onto the Settings window (what winit reports as
//! `HoveredFile`, `HoveredFileCancelled` and `DroppedFile` in the real app).

use crate::Harness;
use std::path::Path;
use washi::settings::font_drop::FileDrag;

impl Harness {
    /// A file is held over the Settings window.
    pub fn settings_hover_file(&mut self) {
        self.app().settings_file_drag(FileDrag::Hover);
        self.settle();
    }

    /// The held file left the Settings window again.
    pub fn settings_cancel_file_hover(&mut self) {
        self.app().settings_file_drag(FileDrag::Cancel);
        self.settle();
    }

    /// `path` was dropped on the Settings window.
    pub fn settings_drop_file(&mut self, path: &Path) {
        self.app().settings_file_drag(FileDrag::Drop(path.to_path_buf()));
        self.settle();
    }

    /// The text of the Font manager's install area.
    pub fn settings_manager_install_hint(&self) -> String {
        self.app().settings().expect("Settings is not open").ui().get_manager_install_hint().to_string()
    }

    /// The Font manager's install area shows that a file is over it.
    pub fn settings_manager_drop_hover(&self) -> bool {
        self.app().settings().expect("Settings is not open").ui().get_manager_install_hover()
    }
}
