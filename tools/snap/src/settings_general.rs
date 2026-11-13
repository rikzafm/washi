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

    /// The Keep text tilted while editing switch as drawn.
    pub fn settings_tilted_editing(&self) -> bool {
        self.general_ui().get_tilted_editing()
    }

    pub fn settings_press_tilted_editing(&mut self) {
        self.general_ui().invoke_toggle_tilted_editing();
        self.settle();
    }

    /// Make every open note keep its text tilted while editing (true) or settle it flat (false),
    /// without going through Settings or the database.
    pub fn set_tilted_editing(&mut self, on: bool) {
        self.app().set_tilted_editing(on);
        self.settle();
    }

    /// Lay every note's text out with `em` of extra space after each letter, as Settings › Font &
    /// Style › Letter spacing does once it has saved the choice.
    pub fn set_letter_spacing(&mut self, em: f32) {
        self.app().set_letter_spacing(em);
        self.settle();
    }

    pub fn settings_press_open_install(&mut self) {
        self.general_ui().invoke_open_install_dir();
    }

    pub fn settings_press_open_data(&mut self) {
        self.general_ui().invoke_open_data_dir();
    }

    /// The Uninstall row's button is enabled (an uninstaller exists).
    pub fn settings_uninstall_available(&self) -> bool {
        self.general_ui().get_uninstall_available()
    }

    /// The Uninstall row is asking "Remove Washi Note from this computer?".
    pub fn settings_uninstall_confirming(&self) -> bool {
        self.general_ui().get_uninstall_confirming()
    }

    pub fn settings_press_uninstall(&mut self) {
        self.general_ui().invoke_uninstall_clicked();
        self.settle();
    }

    pub fn settings_press_uninstall_cancel(&mut self) {
        self.general_ui().invoke_uninstall_cancel();
        self.settle();
    }

    pub fn settings_press_uninstall_confirm(&mut self) {
        self.general_ui().invoke_uninstall_confirm();
        self.settle();
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
