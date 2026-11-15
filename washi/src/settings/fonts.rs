//! Settings › Font & Style › the Font row and the Font manager's list (see `ui/font-manager.slint`).
//!
//! The Slint side draws whatever it is given: the Font row's name and summary
//! (`SettingsWindow::set_font_name` / `set_font_summary`) and one [`FontListRow`] per Font in the
//! slip (`set_manager_fonts`), each with the `top` and `height` it takes in the list. [`model`]
//! stacks [`RowSpec`]s into that list, so a row taller than the rest (an expanded one) only says how
//! much `extra` room it needs. Pressing a row, its "Remove" and the install area arrive as the
//! window's `on_manager_row_clicked`, `on_manager_remove_clicked` and `on_manager_install_clicked`.
//! The slip itself (open, close, its strip in the window) is `SettingsView::toggle_manager`.

use crate::fonts::{AxisRanges, AxisValues, DEFAULT_TEXT_FAMILY, SHEAR_MAX_DEG};
use crate::note_window::{AxisRow, FontListRow, SettingsWindow};
use slint::{ModelRc, VecModel};

/// A row of the list with nothing extra under it, in logical px.
pub const ROW_H: f32 = 58.0;

/// One line of the Axis panel and the room around the three of them, logical px.
pub const AXIS_H: f32 = 48.0;
pub const PANEL_PAD: f32 = 22.0;
/// The room the Axis panel takes under the Font in use's row (`RowSpec::extra`).
pub const PANEL_H: f32 = 3.0 * AXIS_H + PANEL_PAD;

/// Axis numbers of the panel's lines (what `manager-axis-moved` / `-committed` carry).
pub const AXIS_WEIGHT: i32 = 0;
pub const AXIS_WIDTH: i32 = 1;
pub const AXIS_SLANT: i32 = 2;

/// One line of the Axis panel: a slider over whole numbers, or "Not in this font".
#[derive(Clone, Debug, PartialEq)]
pub struct AxisSpec {
    pub label: &'static str,
    /// The font has this axis (Slant: always).
    pub present: bool,
    pub min: i32,
    pub max: i32,
    pub value: i32,
    /// What an arrow key moves the slider by, and how near a snap point a drag lands on it.
    pub step: i32,
    pub pull: i32,
    pub snaps: Vec<i32>,
    pub unit: &'static str,
}

/// The snap points of the Width slider: every one of these inside the font's range.
const WIDTH_SNAPS: [i32; 5] = [50, 75, 100, 125, 150];
const SLANT_SNAPS: [i32; 3] = [0, 5, 10];
/// The slider draws at most this many snap points (`PSlider`).
const MAX_SNAPS: usize = 12;

/// The Axis panel of a font with `axes` at `values`: Weight, Width and Slant lines. Weight and Width
/// are only there when the font has the axis. Slant is always: 0 to the font's own slant axis
/// maximum, or 0 to 12 degrees when Washi shears it.
pub fn axis_panel(axes: &AxisRanges, values: AxisValues) -> Vec<AxisSpec> {
    let whole = |v: f32| v.round() as i32;
    let weight = match axes.weight {
        Some(r) => {
            let (min, max) = (whole(r.min), whole(r.max));
            let value = whole(values.weight.unwrap_or(r.default)).clamp(min, max);
            let snaps = (min..=max).filter(|v| v % 100 == 0).take(MAX_SNAPS).collect();
            AxisSpec { label: "Weight", present: true, min, max, value, step: 10, pull: 20, snaps, unit: "" }
        }
        None => absent("Weight"),
    };
    let width = match axes.width {
        Some(r) => {
            let (min, max) = (whole(r.min), whole(r.max));
            let value = whole(values.width.unwrap_or(r.default)).clamp(min, max);
            let snaps = WIDTH_SNAPS.into_iter().filter(|v| (min..=max).contains(v)).collect();
            AxisSpec { label: "Width", present: true, min, max, value, step: 1, pull: 2, snaps, unit: "%" }
        }
        None => absent("Width"),
    };
    let (max, default) = match axes.slant {
        Some(r) => (whole(r.max).max(1), r.default),
        None => (SHEAR_MAX_DEG as i32, 0.0),
    };
    let slant = AxisSpec {
        label: "Slant",
        present: true,
        min: 0,
        max,
        value: whole(values.slant.unwrap_or(default)).clamp(0, max),
        step: 1,
        pull: 1,
        snaps: SLANT_SNAPS.into_iter().filter(|v| *v <= max).collect(),
        unit: "°",
    };
    vec![weight, width, slant]
}

fn absent(label: &'static str) -> AxisSpec {
    AxisSpec { label, present: false, min: 0, max: 0, value: 0, step: 1, pull: 1, snaps: Vec::new(), unit: "" }
}

/// The Axis values a slider released (or a number typed) at `value` makes, the other axes as they
/// were. A value outside the axis's range is clamped into it; `None` if the font has no such axis.
pub fn with_axis(axes: &AxisRanges, values: AxisValues, axis: i32, value: i32) -> Option<AxisValues> {
    let line = axis_panel(axes, values).into_iter().nth(usize::try_from(axis).ok()?)?;
    if !line.present {
        return None;
    }
    let v = Some(value.clamp(line.min, line.max) as f32);
    let mut out = values;
    match axis {
        AXIS_WEIGHT => out.weight = v,
        AXIS_WIDTH => out.width = v,
        _ => out.slant = v,
    }
    Some(out)
}

/// What one row of the Font manager's list shows.
#[derive(Clone, Debug, PartialEq)]
pub struct RowSpec {
    /// The family name, drawn in `family` (the Font's own face).
    pub name: String,
    pub family: String,
    /// "One style", "Variable: weight, width, slant".
    pub subtitle: String,
    /// The Font in use: its radio dot is filled.
    pub active: bool,
    /// Cannot be removed (the Default font): shows a lock and its `tag`.
    pub locked: bool,
    /// At the row's right: "Default" or "Remove".
    pub tag: String,
    /// Room under the row's own `ROW_H` (an expanded row's Axis panel), logical px.
    pub extra: f32,
    /// The Axis panel under the row (the Font in use only; empty otherwise).
    pub axes: Vec<AxisSpec>,
}

/// The Default font's row (Shadows Into Light, bundled).
pub fn default_row(active: bool) -> RowSpec {
    RowSpec {
        name: DEFAULT_TEXT_FAMILY.into(),
        family: DEFAULT_TEXT_FAMILY.into(),
        subtitle: "One style".into(),
        active,
        locked: true,
        tag: "Default".into(),
        extra: 0.0,
        axes: Vec::new(),
    }
}

/// The rows stacked top to bottom, as the slip's list model.
pub fn model(rows: &[RowSpec]) -> ModelRc<FontListRow> {
    let mut top = 0.0;
    let list: Vec<FontListRow> = rows
        .iter()
        .map(|r| {
            let height = ROW_H + r.extra;
            let row = FontListRow {
                name: r.name.as_str().into(),
                family: r.family.as_str().into(),
                subtitle: r.subtitle.as_str().into(),
                active: r.active,
                locked: r.locked,
                tag: r.tag.as_str().into(),
                top,
                height,
                axes: ModelRc::new(VecModel::from(r.axes.iter().map(axis_row).collect::<Vec<_>>())),
            };
            top += height;
            row
        })
        .collect();
    ModelRc::new(VecModel::from(list))
}

fn axis_row(a: &AxisSpec) -> AxisRow {
    AxisRow {
        label: a.label.into(),
        present: a.present,
        min: a.min,
        max: a.max,
        value: a.value,
        step: a.step,
        pull: a.pull,
        snaps: ModelRc::new(VecModel::from(a.snaps.clone())),
        unit: a.unit.into(),
    }
}

/// The slip's title block above the list and its install area under it (`ManagerMetrics`).
pub const HEAD_H: f32 = 71.0;
pub const FOOT_H: f32 = 118.0;

/// Show the Default font as the Font in use (Settings has just opened; the app then calls [`show`]).
pub fn wire(ui: &SettingsWindow) {
    let row = default_row(true);
    ui.set_font_name(row.name.as_str().into());
    ui.set_font_summary(row.subtitle.as_str().into());
    ui.set_manager_fonts(model(&[row]));
}

/// A font's list subtitle: which axes it has, or that it has one style.
pub fn subtitle(axes: &AxisRanges) -> String {
    let names: Vec<&str> = [(axes.weight, "weight"), (axes.width, "width"), (axes.slant, "slant")]
        .into_iter()
        .filter_map(|(a, n)| a.map(|_| n))
        .collect();
    if names.is_empty() {
        "One style".into()
    } else {
        format!("Variable: {}", names.join(", "))
    }
}

/// The Font row's summary of the Font in use: its Axis values ("400 · 100% · 0°"), for the axes it
/// has (and a slant that was set), or "One style".
pub fn summary(axes: &AxisRanges, values: AxisValues) -> String {
    let mut parts = Vec::new();
    if let Some(r) = axes.weight {
        parts.push(format!("{}", values.weight.unwrap_or(r.default).round()));
    }
    if let Some(r) = axes.width {
        parts.push(format!("{}%", values.width.unwrap_or(r.default).round()));
    }
    let slant = values.slant.or(axes.slant.map(|r| r.default)).unwrap_or(0.0);
    if axes.slant.is_some() || slant != 0.0 {
        parts.push(format!("{}°", slant.round()));
    }
    if parts.is_empty() {
        "One style".into()
    } else {
        parts.join(" · ")
    }
}

/// Where to scroll the list so the row at `top` (`height` tall) is in a view `view` tall, scrolled
/// to `scroll` now: stay put when it already is, else the least movement. At most `max`.
pub fn scroll_into_view(scroll: f32, top: f32, height: f32, view: f32, max: f32) -> f32 {
    let want = if top < scroll {
        top
    } else if top + height > scroll + view {
        top + height - view
    } else {
        scroll
    };
    want.clamp(0.0, max.max(0.0))
}

/// Draw the Font row (`name`, `summary`) and the list, and if `scroll_to` names a row (an index into
/// `rows`) scroll it into view.
pub fn show(ui: &SettingsWindow, rows: &[RowSpec], name: &str, summary: &str, scroll_to: Option<usize>) {
    ui.set_font_name(name.into());
    ui.set_font_summary(summary.into());
    ui.set_manager_fonts(model(rows));
    let Some(i) = scroll_to.filter(|&i| i < rows.len()) else { return };
    let top: f32 = rows[..i].iter().map(|r| ROW_H + r.extra).sum();
    let list_h: f32 = rows.iter().map(|r| ROW_H + r.extra).sum();
    let view = ui.get_manager_height() - HEAD_H - FOOT_H;
    let target = scroll_into_view(ui.get_manager_scroll(), top, ROW_H + rows[i].extra, view, list_h - view);
    ui.set_manager_scroll(target);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fonts::AxisRange;
    use slint::Model;

    fn range(min: f32, default: f32, max: f32) -> Option<AxisRange> {
        Some(AxisRange { min, default, max })
    }

    #[test]
    fn the_subtitle_names_the_axes_a_font_has_or_says_one_style() {
        assert_eq!(subtitle(&AxisRanges::default()), "One style");
        let both = AxisRanges { weight: range(300.0, 400.0, 800.0), width: range(75.0, 100.0, 100.0), slant: None };
        assert_eq!(subtitle(&both), "Variable: weight, width");
        let slant = AxisRanges { slant: range(0.0, 0.0, 15.0), ..Default::default() };
        assert_eq!(subtitle(&slant), "Variable: slant");
    }

    #[test]
    fn the_summary_reads_the_axis_values_or_their_defaults() {
        let axes = AxisRanges { weight: range(300.0, 400.0, 800.0), width: range(75.0, 100.0, 100.0), slant: None };
        assert_eq!(summary(&axes, AxisValues::default()), "400 · 100%");
        assert_eq!(summary(&axes, AxisValues { weight: Some(650.0), width: Some(87.5), slant: Some(5.0) }), "650 · 88% · 5°");
        assert_eq!(summary(&AxisRanges::default(), AxisValues::default()), "One style");
        assert_eq!(summary(&AxisRanges::default(), AxisValues { slant: Some(10.0), ..Default::default() }), "10°", "a sheared font shows its lean");
    }

    #[test]
    fn scrolling_to_a_row_moves_the_list_the_least_and_stays_within_its_range() {
        // view 200 tall, rows 64 tall
        assert_eq!(scroll_into_view(0.0, 64.0, 64.0, 200.0, 300.0), 0.0, "already in view");
        assert_eq!(scroll_into_view(0.0, 256.0, 64.0, 200.0, 300.0), 120.0, "its bottom edge meets the view's");
        assert_eq!(scroll_into_view(200.0, 64.0, 64.0, 200.0, 300.0), 64.0, "above the view: its top edge");
        assert_eq!(scroll_into_view(0.0, 900.0, 64.0, 200.0, 300.0), 300.0, "never past the end");
        assert_eq!(scroll_into_view(50.0, 0.0, 64.0, 500.0, -5.0), 0.0, "nothing to scroll");
    }

    #[test]
    fn rows_stack_with_their_extra_room() {
        let mut open = default_row(true);
        open.extra = 100.0;
        let list = model(&[open, default_row(false), default_row(false)]);
        let tops: Vec<(f32, f32)> = list.iter().map(|r| (r.top, r.height)).collect();
        assert_eq!(tops, vec![(0.0, ROW_H + 100.0), (ROW_H + 100.0, ROW_H), (2.0 * ROW_H + 100.0, ROW_H)]);
    }
}
