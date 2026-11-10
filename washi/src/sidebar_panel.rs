//! The Sidebar's shape for the Rust side, shared by All notes and Settings (logical px). The
//! numbers are `SidebarPanel`'s in ui/sidebar-panel.slint; the harness checks that the two agree.

/// The strip the Sidebar adds left of the Paper (`SidebarPanel.strip`).
pub const STRIP: f32 = 282.0;
/// The panel's width (`SidebarPanel.width`).
pub const PANEL_W: f32 = 270.0;
/// The panel's centre in the strip (`SidebarPanel.cx`).
pub const PANEL_CX: f32 = 143.0;
