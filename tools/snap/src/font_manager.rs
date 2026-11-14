//! Harness helpers for Settings › Font & Style's Font row and the Font manager.

use crate::Harness;
use slint::platform::{PointerEventButton, WindowEvent};
use slint::{ComponentHandle, Model};
use washi::settings::fonts::{default_row, model, RowSpec};

/// A row of the Font manager's list as the slip shows it.
#[derive(Clone, Debug, PartialEq)]
pub struct ManagerRow {
    pub name: String,
    pub subtitle: String,
    pub active: bool,
    pub locked: bool,
    pub tag: String,
}

impl Harness {
    fn manager_ui(&self) -> washi::note_window::SettingsWindow {
        self.app().settings().expect("Settings is not open").ui().clone_strong()
    }

    /// The Font row: the name of the Font in use and the summary under it.
    pub fn settings_font_row(&self) -> (String, String) {
        let ui = self.manager_ui();
        (ui.get_font_name().to_string(), ui.get_font_summary().to_string())
    }

    /// What the manage button is called to a screen reader.
    pub fn settings_manage_button_label(&self) -> String {
        self.manager_ui().get_manage_button_label().to_string()
    }

    /// Whether the manage button is filled dark.
    pub fn settings_manage_button_filled(&self) -> bool {
        self.manager_ui().get_manage_button_filled()
    }

    /// Whether the Font manager is open (or opening).
    pub fn settings_manager_open(&self) -> bool {
        self.manager_ui().get_manager_open()
    }

    /// How far out the slip is: 0 closed, 1 all the way out.
    pub fn settings_manager_progress(&self) -> f32 {
        self.manager_ui().get_manager_progress()
    }

    /// The slip's two headings: the list's and the install area's.
    pub fn settings_manager_titles(&self) -> (String, String) {
        let ui = self.manager_ui();
        (ui.get_manager_heading().to_string(), ui.get_manager_install_heading().to_string())
    }

    /// The rows the list shows, top to bottom.
    pub fn settings_manager_rows(&self) -> Vec<ManagerRow> {
        self.manager_ui()
            .get_manager_fonts()
            .iter()
            .map(|r| ManagerRow {
                name: r.name.to_string(),
                subtitle: r.subtitle.to_string(),
                active: r.active,
                locked: r.locked,
                tag: r.tag.to_string(),
            })
            .collect()
    }

    /// Give the list `n` rows (the Default font, then `n - 1` made-up ones), so it overflows.
    pub fn settings_manager_test_rows(&mut self, n: usize) {
        let mut rows = vec![default_row(true)];
        for i in 1..n {
            rows.push(RowSpec {
                name: format!("Test Font {i}"),
                family: "Shadows Into Light".into(),
                subtitle: "One style".into(),
                active: false,
                locked: false,
                tag: "Remove".into(),
                extra: 0.0,
            });
        }
        self.manager_ui().set_manager_fonts(model(&rows));
        self.settle();
    }

    /// How far the list is scrolled up, and how far it can go.
    pub fn settings_manager_scroll(&self) -> (f32, f32) {
        let ui = self.manager_ui();
        (ui.get_manager_scroll(), ui.get_manager_max_scroll())
    }

    /// The slip's bar: its opacity (0 hidden, 1 shown) and its thumb's centre, Settings window px.
    pub fn settings_manager_bar(&self) -> (f32, f32, f32) {
        let ui = self.manager_ui();
        (ui.get_manager_bar_opacity(), ui.get_manager_thumb_x(), ui.get_manager_thumb_y())
    }

    /// The slip's height, logical px.
    pub fn settings_manager_height(&self) -> f32 {
        self.manager_ui().get_manager_height()
    }

    /// A point in the middle of the list, beside the bar, Settings window px.
    pub fn settings_manager_list_middle(&self) -> (f32, f32) {
        let ui = self.manager_ui();
        (ui.get_manager_list_x(), ui.get_manager_list_y())
    }

    /// Move the pointer over the Settings window.
    pub fn settings_pointer_move(&mut self, x: f32, y: f32) {
        self.app().dispatch_settings_event(WindowEvent::PointerMoved { position: slint::LogicalPosition::new(x, y) });
        self.settle();
    }

    /// Press the left button where the pointer is moved to.
    pub fn settings_pointer_press(&mut self, x: f32, y: f32) {
        let position = slint::LogicalPosition::new(x, y);
        self.app().dispatch_settings_event(WindowEvent::PointerPressed { position, button: PointerEventButton::Left });
        self.settle();
    }

    pub fn settings_pointer_release(&mut self, x: f32, y: f32) {
        let position = slint::LogicalPosition::new(x, y);
        self.app().dispatch_settings_event(WindowEvent::PointerReleased { position, button: PointerEventButton::Left });
        self.settle();
    }
}
