//! Harness helpers for the Sidebars' Height and Anchor (Settings › Appearance › Sidebar): where each
//! Sidebar's panel is drawn against its Paper, measured on the rendered window.

use crate::Harness;
use slint::ComponentHandle;

/// The panel's centre column in the strip (`SidebarPanel.cx`): the tilted panel's top and bottom
/// edges cross it at their true height.
const PANEL_CX: usize = 143;
/// The transparent margin around a Paper (`PaperTheme.margin`).
const MARGIN: f32 = 42.0;

/// The first and last rows of the panel's fill (#D6DBE2, the Default theme) down its centre column:
/// its top and bottom edges, window px.
fn panel_span(w: u32, h: u32, rgb: &[u8]) -> Option<(f32, f32)> {
    let slate = |y: u32| {
        let i = (y as usize * w as usize + PANEL_CX) * 3;
        (rgb[i] as i32 - 0xD6).abs() <= 2 && (rgb[i + 1] as i32 - 0xDB).abs() <= 2 && (rgb[i + 2] as i32 - 0xE2).abs() <= 2
    };
    let ys: Vec<u32> = (0..h).filter(|&y| slate(y)).collect();
    Some((*ys.first()? as f32, *ys.last()? as f32 + 1.0))
}

/// Which Sidebar a Height or Anchor applies to, as the Appearance Tab numbers them.
pub mod sidebar {
    pub const SETTINGS: i32 = 0;
    pub const NOTES: i32 = 1;
}

impl Harness {
    /// The Settings Sidebar's panel as drawn: its top and bottom edge, window px.
    pub fn settings_sidebar_span(&self) -> (f32, f32) {
        let shot = self.render_settings();
        // The Settings shot is drawn over a desk that is not the panel's fill.
        panel_span(shot.width, shot.height, shot.rgb()).expect("the Settings Sidebar's panel is drawn")
    }

    /// The Settings Paper's top and bottom edge, window px.
    pub fn settings_paper_span(&self) -> (f32, f32) {
        let ui = self.app().settings().expect("Settings is not open").ui().clone_strong();
        let h = ui.window().size().height as f32 / ui.window().scale_factor();
        (MARGIN, h - MARGIN)
    }

    /// The All notes Sidebar's panel as drawn: its top and bottom edge, window px.
    pub fn all_notes_sidebar_span(&self) -> (f32, f32) {
        let (w, h, px) = self.render_all_notes().expect("All notes is not open");
        let rgb: Vec<u8> = px.iter().flat_map(|p| [p.r, p.g, p.b]).collect();
        panel_span(w, h, &rgb).expect("the All notes Sidebar's panel is drawn")
    }

    /// The All notes Paper's top and bottom edge, window px.
    pub fn all_notes_paper_span(&self) -> (f32, f32) {
        let (_, h, _) = self.render_all_notes().expect("All notes is not open");
        (MARGIN, h as f32 - MARGIN)
    }
}
