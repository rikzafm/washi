//! Weight, Width and Slant of the Font in use (Settings › Font & Style › Font manager, the Axis
//! panel under the expanded row): sliders for the axes the font has, Slant for every font, snap
//! rules, values saved per font, the preview following a drag, and the notes changing on release.

use snap::{axis, tab, Harness};
use std::path::{Path, PathBuf};
use std::time::Duration;
use washi::fonts::read_package;
use washi::storage::Geometry;

fn ms(n: u64) -> Duration {
    Duration::from_millis(n)
}

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/fonts").join(name)
}

const DEFAULT: &str = "Shadows Into Light";
const TEXT: &str = "# Title\nthe quick brown fox jumps over the lazy dog and keeps running far away from here";

fn started() -> Harness {
    let mut h = Harness::new();
    h.start().unwrap();
    h.place_note(0, Geometry { x: 600, y: 100, ..h.note_geometry(0) });
    h
}

fn open_manager(h: &mut Harness) {
    h.toggle_dropdown(0);
    h.invoke_dropdown_settings();
    h.advance_frames(ms(600));
    h.settings_click_tab(tab::STYLE);
    h.advance_frames(ms(600));
    h.settings_style_click(0);
    h.advance_frames(ms(600));
    assert!(h.settings_manager_open());
}

fn install(h: &mut Harness, name: &str) {
    h.choose_font_file(Some(&fixture(name)));
    h.settings_manager_click_install();
    h.wait_for_font_installs();
}

/// Settings open with Test Sans (weight and width axes) installed and in use.
fn with_sans() -> Harness {
    let mut h = started();
    open_manager(&mut h);
    install(&mut h, "Test_Sans.zip");
    h.settings_manager_click_row(1);
    h.advance(ms(40));
    h
}

fn the_package(name: &str) -> washi::fonts::FontPackage {
    read_package(&std::fs::read(fixture(name)).unwrap()).unwrap()
}

/// Every multiple of `every` inside `min..=max`.
fn multiples(every: i32, min: i32, max: i32) -> Vec<i32> {
    (min..=max).filter(|v| v % every == 0).collect()
}

// ---- which sliders there are ----

#[test]
fn the_default_font_has_no_weight_or_width_but_a_slant_slider_of_0_to_12_degrees() {
    let mut h = started();
    open_manager(&mut h);
    let axes = h.settings_axes();
    assert_eq!(axes.len(), 3, "the Font in use is expanded");
    assert_eq!((axes[0].label.as_str(), axes[0].present), ("Weight", false));
    assert_eq!((axes[1].label.as_str(), axes[1].present), ("Width", false));
    let s = &axes[2];
    assert_eq!((s.label.as_str(), s.present, s.min, s.max, s.value, s.unit.as_str()), ("Slant", true, 0, 12, 0, "°"));
    assert_eq!(s.snaps, [0, 5, 10]);
}

#[test]
fn a_variable_font_has_weight_and_width_sliders_with_the_snaps_inside_its_range() {
    let h = with_sans();
    let pkg = the_package("Test_Sans.zip");
    let (w, wd) = (pkg.axes.weight.unwrap(), pkg.axes.width.unwrap());
    let axes = h.settings_axes();
    let weight = &axes[0];
    assert!(weight.present);
    assert_eq!((weight.min, weight.max, weight.value), (w.min.round() as i32, w.max.round() as i32, w.default.round() as i32));
    assert_eq!(weight.snaps, multiples(100, weight.min, weight.max), "every hundred inside the range");
    assert!(!weight.snaps.is_empty());
    assert_eq!((weight.pull, weight.step), (20, 10));
    let width = &axes[1];
    assert!(width.present);
    assert_eq!((width.min, width.max, width.value), (wd.min.round() as i32, wd.max.round() as i32, wd.default.round() as i32));
    let expected: Vec<i32> = [50, 75, 100, 125, 150].into_iter().filter(|v| (width.min..=width.max).contains(v)).collect();
    assert_eq!(width.snaps, expected);
    assert_eq!((width.pull, width.unit.as_str()), (2, "%"));
    let slant = &axes[2];
    assert_eq!((slant.present, slant.min, slant.max), (true, 0, 12), "no slant axis: sheared by Washi");
}

#[test]
fn a_single_style_font_says_not_in_this_font_for_weight_and_width_and_still_has_a_slant_slider() {
    let mut h = started();
    open_manager(&mut h);
    install(&mut h, "Test_Serif.zip");
    h.settings_manager_click_row(1);
    let axes = h.settings_axes();
    assert_eq!((axes[0].present, axes[1].present, axes[2].present), (false, false, true));
}

#[test]
fn a_font_with_its_own_slant_axis_ranges_from_0_to_that_axis_maximum() {
    let mut h = started();
    open_manager(&mut h);
    install(&mut h, "TestSlant-VariableFont_slnt.ttf");
    h.settings_manager_click_row(1);
    let max = the_package("TestSlant-VariableFont_slnt.ttf").axes.slant.unwrap().max.round() as i32;
    let s = &h.settings_axes()[2];
    assert_eq!((s.min, s.max), (0, max));
    assert!(s.snaps.iter().all(|v| [0, 5, 10].contains(v) && *v <= max));
}

#[test]
fn only_the_font_in_use_is_expanded() {
    let mut h = with_sans();
    assert!(!h.settings_axes().is_empty());
    let rows = h.app().settings().unwrap().ui().get_manager_fonts();
    use slint::Model;
    let expanded: Vec<bool> = rows.iter().map(|r| r.axes.row_count() > 0).collect();
    assert_eq!(expanded, [false, true], "the Default font collapsed when Test Sans was picked");
    h.settings_manager_click_row(0);
    let expanded: Vec<bool> = h.app().settings().unwrap().ui().get_manager_fonts().iter().map(|r| r.axes.row_count() > 0).collect();
    assert_eq!(expanded, [true, false]);
}

// ---- saved per font, shown in the Font row ----

#[test]
fn the_font_rows_summary_follows_the_saved_values() {
    let mut h = with_sans();
    let (_, before) = h.settings_font_row();
    assert!(before.contains('%'), "{before}");
    h.settings_axis_committed(axis::WEIGHT, 700);
    h.settings_axis_committed(axis::WIDTH, 80);
    h.settings_axis_committed(axis::SLANT, 5);
    assert_eq!(h.settings_font_row().1, "700 · 80% · 5°");
}

#[test]
fn typed_values_outside_the_range_clamp() {
    let mut h = with_sans();
    let w = h.settings_axes()[0].clone();
    h.settings_axis_committed(axis::WEIGHT, 100000);
    assert!(h.settings_font_row().1.starts_with(&format!("{} ·", w.max)), "{}", h.settings_font_row().1);
    h.settings_axis_committed(axis::WEIGHT, -5);
    assert!(h.settings_font_row().1.starts_with(&format!("{} ·", w.min)), "{}", h.settings_font_row().1);
    h.settings_axis_committed(axis::SLANT, 99);
    assert!(h.settings_font_row().1.ends_with("12°"), "{}", h.settings_font_row().1);
}

#[test]
fn each_font_keeps_its_own_values_across_switching_and_restarts() {
    let mut h = with_sans();
    h.settings_axis_committed(axis::WEIGHT, 700);
    h.settings_axis_committed(axis::SLANT, 10);
    install(&mut h, "Test_Serif.zip");
    h.settings_manager_click_row(2);
    h.settings_axis_committed(axis::SLANT, 5);
    h.settings_manager_click_row(0);
    h.settings_axis_committed(axis::SLANT, 10);
    assert_eq!(h.settings_font_row().1, "10°", "the Default font's slant");

    h.settings_manager_click_row(1);
    let a = h.settings_axes();
    assert_eq!((a[0].value, a[2].value), (700, 10), "Test Sans: as left");
    h.settings_manager_click_row(2);
    assert_eq!(h.settings_axes()[2].value, 5, "Test Serif: its own");

    h.settings_close();
    h.advance_frames(ms(600));
    h.restart().unwrap();
    open_manager(&mut h);
    h.settings_manager_click_row(1);
    let a = h.settings_axes();
    assert_eq!((a[0].value, a[2].value), (700, 10));
    h.settings_manager_click_row(0);
    assert_eq!(h.settings_axes()[2].value, 10, "the Default font's slant is saved in its own setting");
}

#[test]
fn the_default_font_at_slant_0_is_the_bundled_faces_again() {
    let mut h = started();
    h.advance(ms(20));
    open_manager(&mut h);
    assert_eq!(h.note_font_family(0), DEFAULT);
    h.settings_axis_committed(axis::SLANT, 5);
    h.advance(ms(40));
    assert!(h.note_font_family(0).starts_with("Washi Text"), "a sheared Shadows Into Light is baked");
    assert_eq!(h.settings_font_row().1, "5°");
    h.settings_axis_committed(axis::SLANT, 0);
    h.advance(ms(40));
    assert_eq!(h.note_font_family(0), DEFAULT, "at default values the bundled faces are used unchanged");
}

// ---- dragging and releasing ----

#[test]
fn dragging_remakes_only_regular_and_only_settings_follows_then_release_updates_every_window() {
    let mut h = with_sans();
    h.new_note_from(0);
    h.advance(ms(20));
    for n in 0..2 {
        h.set_note_text(n, TEXT);
    }
    let note_family = h.note_font_family(0);
    let note_runs: Vec<_> = h.text_runs(0).into_iter().map(|r| (r.0, r.1, r.2)).collect();
    let settings_before = h.settings_font_family();
    assert_eq!(settings_before, note_family);

    h.settings_axis_moved(axis::WEIGHT, 600);
    let dragging = h.settings_font_family();
    assert_ne!(dragging, settings_before, "the Settings window follows the drag");
    assert_eq!(h.note_font_family(0), note_family, "the notes wait for the release");
    assert_eq!(h.note_font_family(1), note_family);

    // about one remake per 30 ms: a second move right away waits, the last value is not lost
    h.settings_axis_moved(axis::WEIGHT, 650);
    assert_eq!(h.settings_font_family(), dragging);
    h.advance(ms(40));
    let later = h.settings_font_family();
    assert_ne!(later, dragging, "the value that waited was made after the pause");
    assert_eq!(h.note_font_family(0), note_family);

    h.settings_axis_committed(axis::WEIGHT, 650);
    h.advance(ms(40));
    let now = h.note_font_family(0);
    assert_ne!(now, note_family, "release makes the full Face set");
    assert_eq!(h.note_font_family(1), now, "every window updates");
    assert_eq!(h.settings_font_family(), now);
    let runs: Vec<_> = h.text_runs(0).into_iter().map(|r| (r.0, r.1, r.2)).collect();
    assert_ne!(runs, note_runs, "the lines are laid out in the new weight");
}

#[test]
fn a_move_that_changes_nothing_after_the_release_does_not_remake_the_faces() {
    let mut h = with_sans();
    let family = h.note_font_family(0);
    let w = h.settings_axes()[0].value;
    h.settings_axis_committed(axis::WEIGHT, w);
    h.advance(ms(40));
    assert_eq!(h.note_font_family(0), family, "a click that left the value alone is not a change");
}

// ---- bold and italic sit on top of the values ----

#[test]
fn with_non_default_weight_width_and_slant_bold_and_italic_runs_keep_the_caret_on_the_letters() {
    let mut h = with_sans();
    h.set_tilted_editing(false);
    h.settings_axis_committed(axis::WEIGHT, 650);
    h.settings_axis_committed(axis::WIDTH, 80);
    h.settings_axis_committed(axis::SLANT, 10);
    h.advance(ms(60));
    h.settings_close();
    h.advance_frames(ms(600));
    h.set_note_text(0, "# A **bold** title\nplain *italic text* and ***both of them*** and plain again for a long line that wraps");
    h.advance(ms(60));
    let mut checked = 0;
    for (block, (kind, text)) in h.blocks(0).iter().enumerate() {
        if kind == "image" {
            continue;
        }
        let offsets: Vec<usize> = (0..=text.len()).filter(|&o| text.is_char_boundary(o)).collect();
        for (i, &offset) in offsets.iter().enumerate() {
            if i % 3 != 0 && offset != text.len() {
                continue;
            }
            h.place_caret_at(0, block, offset);
            let (left, top, right, bottom) = h.drawn_caret(0).unwrap_or_else(|| panic!("no caret at block {block} offset {offset}"));
            let (x, line_top, line_bottom) = h.laid_out_caret(0, block, offset);
            let at = format!("block {block} offset {offset}: caret x {left}..{right} y {top}..{bottom}, line x {x} y {line_top}..{line_bottom}");
            assert!(right - left <= 3 && (left as f32 - x).abs() <= 1.0, "{at}");
            assert!((top as f32 - line_top).abs() <= 1.0 && (bottom as f32 - line_bottom).abs() <= 1.0, "{at}");
            checked += 1;
        }
    }
    assert!(checked > 20, "{checked} positions");
}
