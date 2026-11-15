//! Harness helpers for the Font manager's Axis panel (Weight, Width, Slant of the Font in use).

use crate::Harness;
use slint::{ComponentHandle, Model};

/// One line of the Axis panel as the slip shows it.
#[derive(Clone, Debug, PartialEq)]
pub struct AxisInfo {
    pub label: String,
    /// The font has this axis (else the line says "Not in this font").
    pub present: bool,
    pub min: i32,
    pub max: i32,
    pub value: i32,
    pub step: i32,
    pub pull: i32,
    pub snaps: Vec<i32>,
    pub unit: String,
}

/// Axis numbers of the panel's lines.
pub mod axis {
    pub const WEIGHT: i32 = 0;
    pub const WIDTH: i32 = 1;
    pub const SLANT: i32 = 2;
}

impl Harness {
    fn axes_ui(&self) -> washi::note_window::SettingsWindow {
        self.app().settings().expect("Settings is not open").ui().clone_strong()
    }

    /// The Axis panel of the Font in use (empty while no row is expanded).
    pub fn settings_axes(&self) -> Vec<AxisInfo> {
        let ui = self.axes_ui();
        let Some(row) = ui.get_manager_fonts().iter().find(|r| r.active) else { return Vec::new() };
        row.axes
            .iter()
            .map(|a| AxisInfo {
                label: a.label.to_string(),
                present: a.present,
                min: a.min,
                max: a.max,
                value: a.value,
                step: a.step,
                pull: a.pull,
                snaps: a.snaps.iter().collect(),
                unit: a.unit.to_string(),
            })
            .collect()
    }

    /// A slider of the panel is being dragged: it is now at `value` (`axis` is one of [`axis`]).
    pub fn settings_axis_moved(&mut self, axis: i32, value: i32) {
        self.axes_ui().invoke_manager_axis_moved(axis, value);
        self.settle();
    }

    /// A slider was released at `value`, or a number box committed it.
    pub fn settings_axis_committed(&mut self, axis: i32, value: i32) {
        self.axes_ui().invoke_manager_axis_committed(axis, value);
        self.settle();
    }
}
