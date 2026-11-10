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

    /// Which Appearance dropdown is open (-1 none, 0 UI size, 10 the first Theme list, 11 the Notes
    /// sidebar's).
    pub fn settings_appearance_open_list(&self) -> i32 {
        self.appearance_ui().get_appearance_open_list()
    }

    /// The Theme row's "Same for both sidebars" switch as drawn.
    pub fn settings_theme_shared(&self) -> bool {
        self.appearance_ui().get_theme_shared()
    }

    /// The Theme dropdowns shown, top to bottom, as (row label, chosen option): one while the switch
    /// is on, the Settings sidebar's and the Notes sidebar's while it is off.
    pub fn settings_theme_rows(&self) -> Vec<(String, String)> {
        let ui = self.appearance_ui();
        let rows = ui.get_theme_rows();
        let labels = ui.get_theme_labels();
        rows.iter().zip(labels.iter()).map(|(r, l)| (r.to_string(), l.to_string())).collect()
    }

    /// The All notes window drawn at its own size onto the Settings desk color (its margins and a
    /// Transparent Sidebar show the desk).
    pub fn render_all_notes_on_desk(&self) -> Option<crate::shot::Snapshot> {
        use slint::platform::software_renderer::PremultipliedRgbaColor;
        use slint::platform::WindowAdapter;
        let (_, _, pw, ph) = self.all_notes_window_geometry()?;
        let adapter = crate::platform::windows().into_iter().rev().find(|w| {
            let s = w.window().size();
            s.width == pw && s.height == ph
        })?;
        let mut buf = vec![PremultipliedRgbaColor::default(); (pw * ph) as usize];
        adapter.window().request_redraw();
        adapter.draw_if_needed(|r| {
            r.render(&mut buf, pw as usize);
        });
        Some(crate::settings_shots::on_desk(pw, ph, buf))
    }

    // ---- UI size: what the screen shows (physical px, the stand-in monitor is at 100%) ----

    /// A note's window on screen (physical px): what the user sees, whatever the UI size.
    pub fn note_screen_size(&self, note: usize) -> (u32, u32) {
        let s = self.app().with_note(self.note_id(note), |n| n.ui().window().size()).unwrap();
        (s.width, s.height)
    }

    /// How many screen pixels one logical px of a note covers (1 at Large on a 100% monitor).
    pub fn note_drawing_scale(&self, note: usize) -> f32 {
        self.app().with_note(self.note_id(note), |n| n.ui().window().scale_factor()).unwrap()
    }

    /// The Settings window on screen (physical px).
    pub fn settings_screen_size(&self) -> Option<(u32, u32)> {
        let s = self.app().settings()?.ui().window().size();
        Some((s.width, s.height))
    }

    /// The open dropdown menu's window on screen (physical px).
    pub fn dropdown_screen_size(&self) -> Option<(u32, u32)> {
        self.app().dropdown_screen_size()
    }

    /// How many screen pixels one logical px of the Settings window covers.
    pub fn settings_drawing_scale(&self) -> Option<f32> {
        Some(self.app().settings()?.ui().window().scale_factor())
    }

    /// The delete dialog's window on screen (physical px), and how many screen pixels one of its
    /// logical px covers.
    pub fn delete_dialog_screen(&self) -> Option<((u32, u32), f32)> {
        let d = self.app().delete_dialog()?;
        let w = d.ui().window();
        Some(((w.size().width, w.size().height), w.scale_factor()))
    }

    /// The user drags the Settings window onto a monitor at `scale` (DPI / 96).
    pub fn move_settings_to_monitor(&mut self, scale: f32) {
        let ui = self.appearance_ui();
        Self::os_move_to_monitor(ui.window(), scale);
        self.settle();
    }

    /// The user drags the delete dialog onto a monitor at `scale` (DPI / 96).
    pub fn move_delete_dialog_to_monitor(&mut self, scale: f32) {
        let d = self.app().delete_dialog().expect("no delete dialog");
        Self::os_move_to_monitor(d.ui().window(), scale);
        self.settle();
    }

    /// The user drags note `n` onto a monitor at `scale` (DPI / 96); the window procedure then
    /// reports the move, as for any move.
    pub fn move_note_to_monitor(&mut self, n: usize, scale: f32) {
        let id = self.note_id(n);
        let ui = self.app().with_note(id, |n| n.ui().clone_strong()).unwrap();
        Self::os_move_to_monitor(ui.window(), scale);
        self.app().geometry_changed(id);
        self.settle();
    }

    /// How many screen pixels one logical px of each open picture viewer covers, oldest first.
    pub fn viewer_drawing_scales(&self) -> Vec<f32> {
        self.app().viewer_scale_factors()
    }

    /// The pitch scale used for text layout on note `n`.
    pub fn note_pitch_scale(&self, n: usize) -> f32 {
        let id = self.note_id(n);
        self.app().with_note(id, |n| n.ui().get_pitch_scale()).unwrap()
    }

    fn os_move_to_monitor(window: &slint::Window, scale: f32) {
        let adapter = crate::platform::windows()
            .into_iter()
            .find(|w| std::ptr::eq(slint::platform::WindowAdapter::window(&**w), window))
            .expect("not a stand-in window");
        adapter.os_move_to_monitor(scale);
    }
}
