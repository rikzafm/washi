//! Harness helpers for installing, picking and removing fonts through the Font manager.

use crate::Harness;
use slint::ComponentHandle;
use std::path::{Path, PathBuf};
use std::time::Duration;

impl Harness {
    /// The fonts folder of the data folder (it does not exist until a font is installed).
    pub fn fonts_dir(&self) -> PathBuf {
        self.data_dir().join("fonts")
    }

    /// What the file picker answers from now on (`None`: the user cancels).
    pub fn choose_font_file(&self, path: Option<&Path>) {
        *self.dialogs.open_path.borrow_mut() = path.map(|p| p.to_path_buf());
    }

    fn fonts_ui(&self) -> washi::note_window::SettingsWindow {
        self.app().settings().expect("Settings is not open").ui().clone_strong()
    }

    /// The "Install a font" area was pressed.
    pub fn settings_manager_click_install(&mut self) {
        self.fonts_ui().invoke_manager_install_clicked();
        self.settle();
    }

    /// Row `i` of the list was pressed.
    pub fn settings_manager_click_row(&mut self, i: i32) {
        self.fonts_ui().invoke_manager_row_clicked(i);
        self.settle();
    }

    /// Row `i`'s "Remove" was pressed.
    pub fn settings_manager_click_remove(&mut self, i: i32) {
        self.fonts_ui().invoke_manager_remove_clicked(i);
        self.settle();
    }

    /// Let the fonts being installed finish and the app take their results.
    pub fn wait_for_font_installs(&mut self) {
        for _ in 0..2000 {
            self.advance(Duration::from_millis(20));
            if self.app().pending_font_installs() == 0 {
                self.advance(Duration::ZERO);
                return;
            }
            std::thread::sleep(Duration::from_millis(2));
        }
        panic!("a font was still being installed after 4 s");
    }

    /// The family every open note draws its words in.
    pub fn note_font_family(&self, note: usize) -> String {
        self.app().with_note(self.id(note), |n| n.ui().global::<washi::note_window::Theme>().get_handwriting().to_string()).unwrap()
    }

    /// The family the open Settings page draws in.
    pub fn settings_font_family(&self) -> String {
        self.fonts_ui().global::<washi::note_window::Theme>().get_handwriting().to_string()
    }
}
