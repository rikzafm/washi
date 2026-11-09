//! Harness helpers for Settings › General (kept apart from the main harness file).

use crate::Harness;

impl Harness {
    fn general_ui(&self) -> washi::note_window::SettingsWindow {
        use slint::ComponentHandle;
        self.app().settings().expect("Settings is not open").ui().clone_strong()
    }

    /// The Start with Windows switch as drawn.
    pub fn settings_start_with_windows(&self) -> bool {
        self.general_ui().get_start_with_windows()
    }

    pub fn settings_press_start_with_windows(&mut self) {
        self.general_ui().invoke_toggle_startup();
        self.settle();
    }

    /// The Always on top switch as drawn.
    pub fn settings_new_notes_on_top(&self) -> bool {
        self.general_ui().get_new_notes_on_top()
    }

    pub fn settings_press_always_on_top(&mut self) {
        self.general_ui().invoke_toggle_on_top();
        self.settle();
    }

    pub fn settings_press_open_install(&mut self) {
        self.general_ui().invoke_open_install_dir();
    }

    pub fn settings_press_open_data(&mut self) {
        self.general_ui().invoke_open_data_dir();
    }

    /// The four Resource use tiles: memory, CPU, database, open notes.
    pub fn settings_stats(&self) -> [String; 4] {
        let ui = self.general_ui();
        [ui.get_stat_memory().to_string(), ui.get_stat_cpu().to_string(), ui.get_stat_db().to_string(), ui.get_stat_notes().to_string()]
    }

    /// The Installed / Database folder rows' paths as drawn.
    pub fn settings_folders(&self) -> [String; 2] {
        let ui = self.general_ui();
        [ui.get_install_dir().to_string(), ui.get_data_dir().to_string()]
    }

    /// The data folder the app runs on.
    pub fn data_dir(&self) -> std::path::PathBuf {
        self.app().env().data_dir.clone()
    }
}
