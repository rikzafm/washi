//! Harness helpers for Settings › Appearance and for the Settings Sidebar's Tab list.

use crate::Harness;
use slint::{ComponentHandle, Model};
use slint::platform::WindowEvent;

/// The Settings Tabs in Sidebar order, as `settings_click_tab` takes them.
pub mod tab {
    pub const GENERAL: i32 = 0;
    pub const APPEARANCE: i32 = 1;
    pub const STYLE: i32 = 2;
    pub const SHORTCUTS: i32 = 3;
    pub const PRIVACY: i32 = 4;
    /// Every Tab, in order.
    pub const ALL: [i32; 5] = [GENERAL, APPEARANCE, STYLE, SHORTCUTS, PRIVACY];
}

impl Harness {
    fn appearance_ui(&self) -> washi::note_window::SettingsWindow {
        self.app().settings().expect("Settings is not open").ui().clone_strong()
    }

    /// The Tab labels in the Sidebar, top to bottom.
    pub fn settings_tab_names(&self) -> Vec<String> {
        self.appearance_ui().get_tab_names().iter().map(|s| s.to_string()).collect()
    }

    /// The title drawn at the top of the Paper (the shown Tab's name).
    pub fn settings_title(&self) -> String {
        let ui = self.appearance_ui();
        ui.get_tab_names().row_data(ui.get_shown_tab() as usize).map(|s| s.to_string()).unwrap_or_default()
    }

    /// Click the Appearance control `id` (see `hit-x` in appearance-tab.slint).
    /// A control scrolled out of the Paper is first wheeled into view, as a user would.
    pub fn settings_appearance_click(&mut self, id: i32) {
        let ui = self.appearance_ui();
        let y = ui.invoke_appearance_hit_y(id);
        let (top, bottom) = (ui.get_view_top(), ui.get_view_bottom());
        if y > bottom - 30.0 {
            self.settings_wheel(-(y - (bottom - 80.0)));
        } else if y < top + 30.0 {
            self.settings_wheel(top + 80.0 - y);
        }
        let position = slint::LogicalPosition::new(ui.invoke_appearance_hit_x(id), ui.invoke_appearance_hit_y(id));
        let button = slint::platform::PointerEventButton::Left;
        self.app().dispatch_settings_event(WindowEvent::PointerMoved { position });
        self.app().dispatch_settings_event(WindowEvent::PointerPressed { position, button });
        self.app().dispatch_settings_event(WindowEvent::PointerReleased { position, button });
        self.settle();
    }

    /// The UI size the Appearance Tab shows as chosen: 0 Small, 1 Medium, 2 Large.
    pub fn settings_ui_size(&self) -> i32 {
        self.appearance_ui().get_ui_size()
    }

    /// The label on the closed UI size dropdown.
    pub fn settings_ui_size_label(&self) -> String {
        self.appearance_ui().get_ui_size_label().to_string()
    }

    /// Which Appearance dropdown is open (-1 none, 0 UI size).
    pub fn settings_appearance_open_list(&self) -> i32 {
        self.appearance_ui().get_appearance_open_list()
    }
}
