//! Harness helpers for the Settings Paper's height cap and the scrolling of a Tab past it.

use crate::Harness;
use slint::platform::WindowEvent;
use slint::ComponentHandle;

impl Harness {
    fn height_ui(&self) -> washi::note_window::SettingsWindow {
        self.app().settings().expect("Settings is not open").ui().clone_strong()
    }

    /// The Paper's height as the window has it now (the page minus its margins), logical px.
    pub fn settings_paper_height(&self) -> u32 {
        let (_, _, _, h) = self.settings_window_geometry().expect("Settings is not open");
        h - 2 * washi::settings::anim::MARGIN as u32
    }

    /// On the Appearance Tab: turn both "Same for both sidebars" switches off (Theme, then the
    /// Sidebar group), so each Sidebar gets its own rows and the Tab grows past the Tallest tab.
    pub fn settings_split_appearance(&mut self) {
        /// The switches' ids, as `hit-x` in appearance-tab.slint defines them.
        const THEME_SWITCH: i32 = 40;
        const LAYOUT_SWITCH: i32 = 4;
        for id in [THEME_SWITCH, LAYOUT_SWITCH] {
            self.settings_appearance_click(id);
        }
    }

    /// The Settings height option the Appearance Tab shows: 0 Tallest tab, 1 Screen limit.
    pub fn settings_height_mode(&self) -> i32 {
        self.height_ui().get_settings_height()
    }

    /// The label on the closed Settings height dropdown, and the hint under the row.
    pub fn settings_height_label(&self) -> (String, String) {
        let ui = self.height_ui();
        (ui.get_settings_height_label().to_string(), ui.get_settings_height_hint().to_string())
    }

    /// How far the shown Tab's content is scrolled up (0 at the top), and how far it can go.
    pub fn settings_scroll(&self) -> (f32, f32) {
        let ui = self.height_ui();
        (ui.get_content_scroll(), ui.get_max_content_scroll())
    }

    /// Turn the mouse wheel by `dy` (negative scrolls down) over the middle of the Paper.
    pub fn settings_wheel(&mut self, dy: f32) {
        let (x, y) = self.settings_paper_middle();
        self.settings_wheel_at(x, y, dy);
    }

    /// Turn the mouse wheel by `dy` at a Settings window position.
    pub fn settings_wheel_at(&mut self, x: f32, y: f32, dy: f32) {
        let position = slint::LogicalPosition::new(x, y);
        self.app().dispatch_settings_event(WindowEvent::PointerMoved { position });
        self.app().dispatch_settings_event(WindowEvent::PointerScrolled { position, delta_x: 0.0, delta_y: dy });
        self.settle();
    }

    /// The middle of the Paper, Settings window px.
    pub fn settings_paper_middle(&self) -> (f32, f32) {
        let ui = self.height_ui();
        let (_, _, w, h) = self.settings_native_window_geometry().unwrap();
        let strip = ui.get_strip();
        (strip + (w as f32 - strip) / 2.0, h as f32 / 2.0)
    }

    /// Move the pointer to a Settings window position (no button).
    pub fn settings_pointer_to(&mut self, x: f32, y: f32) {
        let position = slint::LogicalPosition::new(x, y);
        self.app().dispatch_settings_event(WindowEvent::PointerMoved { position });
        self.settle();
    }

    /// Opacity of the Settings scrollbar (0 hidden, 1 shown).
    pub fn settings_scrollbar_opacity(&self) -> f32 {
        self.height_ui().get_scrollbar_opacity()
    }

    /// The centre of the Settings scrollbar's thumb, Settings window px.
    pub fn settings_scrollbar_thumb(&self) -> (f32, f32) {
        let ui = self.height_ui();
        (ui.get_scrollbar_thumb_x(), ui.get_scrollbar_thumb_y())
    }

    /// Press the left button at `from`, move to `to` and let go (Settings window px).
    pub fn settings_drag(&mut self, from: (f32, f32), to: (f32, f32)) {
        let button = slint::platform::PointerEventButton::Left;
        let at = |p: (f32, f32)| slint::LogicalPosition::new(p.0, p.1);
        self.app().dispatch_settings_event(WindowEvent::PointerMoved { position: at(from) });
        self.app().dispatch_settings_event(WindowEvent::PointerPressed { position: at(from), button });
        for i in 1..=4 {
            let t = i as f32 / 4.0;
            let p = (from.0 + (to.0 - from.0) * t, from.1 + (to.1 - from.1) * t);
            self.app().dispatch_settings_event(WindowEvent::PointerMoved { position: at(p) });
        }
        self.app().dispatch_settings_event(WindowEvent::PointerReleased { position: at(to), button });
        self.settle();
    }
}
