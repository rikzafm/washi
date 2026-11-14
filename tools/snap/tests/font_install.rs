//! Installing, picking and removing fonts (Settings › Font & Style › Font manager): the package is
//! read off the UI thread and kept in the fonts folder, a click on a row makes it the Font in use in
//! every open note, Remove deletes it, and the choice is back after a restart (before the first
//! window). The native file picker is faked; everything below it is real.

use snap::{tab, Harness};
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

/// The family a fixture installs as.
fn family(name: &str) -> String {
    read_package(&std::fs::read(fixture(name)).unwrap()).unwrap().family
}

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

fn close_settings(h: &mut Harness) {
    h.settings_close();
    h.advance_frames(ms(600));
}

fn install(h: &mut Harness, fixture_name: &str) {
    h.choose_font_file(Some(&fixture(fixture_name)));
    h.settings_manager_click_install();
    h.wait_for_font_installs();
}

fn names(h: &Harness) -> Vec<String> {
    h.settings_manager_rows().into_iter().map(|r| r.name).collect()
}

fn active(h: &Harness) -> Vec<bool> {
    h.settings_manager_rows().into_iter().map(|r| r.active).collect()
}

/// The resting words without the tilt, which settles over time: (text, x, y, width).
fn level(runs: &[(String, f32, f32, f32, f32, f32)]) -> Vec<(String, f32, f32, f32)> {
    runs.iter().map(|r| (r.0.clone(), r.1, r.2, r.5)).collect()
}

fn installed_folders(h: &Harness) -> Vec<String> {
    let Ok(dir) = std::fs::read_dir(h.fonts_dir()) else { return Vec::new() };
    let mut v: Vec<String> = dir.map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
    v.sort();
    v
}

const DEFAULT: &str = "Shadows Into Light";
const TEXT: &str = "# Title\nthe quick brown fox jumps over the lazy dog and keeps running far away from here";

// ---- installing ----

#[test]
fn installing_a_zip_adds_a_row_with_its_subtitle_and_does_not_make_it_the_font_in_use() {
    let mut h = started();
    open_manager(&mut h);
    install(&mut h, "Test_Sans.zip");
    let rows = h.settings_manager_rows();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[1].name, family("Test_Sans.zip"));
    assert_eq!(rows[1].subtitle, "Variable: weight, width");
    assert_eq!((rows[1].locked, rows[1].tag.as_str(), rows[1].active), (false, "Remove", false));
    assert!(rows[0].active, "installing does not switch fonts");
    assert_eq!(h.settings_font_row().0, DEFAULT);
    assert_eq!(h.note_font_family(0), DEFAULT);
    assert!(h.dialogs.errors.borrow().is_empty());
}

#[test]
fn a_static_font_says_one_style_and_the_new_row_is_scrolled_into_view() {
    let mut h = started();
    open_manager(&mut h);
    install(&mut h, "Test_Serif.zip");
    assert_eq!(h.settings_manager_rows()[1].subtitle, "One style");
    let (scroll, max) = h.settings_manager_scroll();
    assert!(scroll >= 0.0 && scroll <= max.max(0.0));
}

#[test]
fn a_font_goes_into_the_fonts_folder_and_nothing_else_is_written_outside_it() {
    let mut h = started();
    open_manager(&mut h);
    let before: Vec<String> = std::fs::read_dir(h.data_dir()).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
    install(&mut h, "Test_Sans.zip");
    install(&mut h, "Test_Serif.zip");
    let mut after: Vec<String> = std::fs::read_dir(h.data_dir()).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
    after.sort();
    let mut expected = before;
    expected.push("fonts".into());
    expected.sort();
    after.retain(|n| !n.starts_with("notes.db-")); // the database's own journal files come and go
    expected.retain(|n| !n.starts_with("notes.db-"));
    assert_eq!(after, expected);
    let folders = installed_folders(&h);
    assert_eq!(folders.len(), 2);
    assert!(folders.iter().all(|f| !f.ends_with(".tmp") && !f.ends_with(".old")), "{folders:?}");
}

#[test]
fn a_file_that_is_not_a_font_is_refused_with_its_reason_and_writes_nothing() {
    let mut h = started();
    open_manager(&mut h);
    let junk = h.data_dir().join("notes.ttf");
    std::fs::write(&junk, b"this is not a font").unwrap();
    h.choose_font_file(Some(&junk));
    h.settings_manager_click_install();
    h.wait_for_font_installs();
    assert_eq!(*h.dialogs.errors.borrow(), vec!["This file is not a usable font.".to_string()]);
    assert_eq!(names(&h), [DEFAULT]);
    assert!(installed_folders(&h).is_empty());
}

#[test]
fn a_file_the_picker_cannot_read_is_reported_and_cancelling_the_picker_does_nothing() {
    let mut h = started();
    open_manager(&mut h);
    h.choose_font_file(None);
    h.settings_manager_click_install();
    h.wait_for_font_installs();
    assert!(h.dialogs.errors.borrow().is_empty(), "cancelled");
    h.choose_font_file(Some(&h.data_dir().join("gone.zip")));
    h.settings_manager_click_install();
    h.wait_for_font_installs();
    assert_eq!(h.dialogs.errors.borrow().len(), 1);
    assert_eq!(names(&h), [DEFAULT]);
}

#[test]
fn installing_a_family_again_replaces_it_instead_of_adding_a_second_row() {
    let mut h = started();
    open_manager(&mut h);
    install(&mut h, "Test_Serif.zip");
    h.settings_manager_click_row(1);
    install(&mut h, "Test_Serif.zip");
    assert_eq!(names(&h).len(), 2);
    assert_eq!(installed_folders(&h).len(), 1);
    assert_eq!(active(&h), [false, true], "the reinstalled font stays in use");
    assert!(h.note_font_family(0).starts_with("Washi Text"));
}

#[test]
fn installed_fonts_survive_a_restart() {
    let mut h = started();
    open_manager(&mut h);
    install(&mut h, "Test_Sans.zip");
    install(&mut h, "Test_Serif.zip");
    h.restart().unwrap();
    open_manager(&mut h);
    assert_eq!(names(&h), [DEFAULT.to_string(), family("Test_Sans.zip"), family("Test_Serif.zip")]);
    assert!(h.dialogs.errors.borrow().is_empty());
}

// ---- picking ----

#[test]
fn picking_a_font_changes_every_open_note_its_line_breaks_and_the_font_row() {
    let mut h = started();
    h.new_note_from(0);
    h.advance(ms(20));
    for n in 0..2 {
        h.set_note_text(n, TEXT);
    }
    let before: Vec<_> = (0..2).map(|n| h.text_runs(n).into_iter().map(|r| (r.0, r.1, r.2)).collect::<Vec<_>>()).collect();
    open_manager(&mut h);
    install(&mut h, "Test_Sans.zip");
    h.settings_manager_click_row(1);
    h.advance(ms(40));
    for n in 0..2 {
        assert!(h.note_font_family(n).starts_with("Washi Text"), "note {n}: {}", h.note_font_family(n));
        let now: Vec<_> = h.text_runs(n).into_iter().map(|r| (r.0, r.1, r.2)).collect();
        assert_ne!(now, before[n], "note {n} breaks its lines elsewhere");
    }
    assert_eq!(h.settings_font_family(), h.note_font_family(0), "the Settings page draws in it too");
    assert_eq!(h.settings_font_row().0, family("Test_Sans.zip"));
    assert_eq!(active(&h), [false, true]);
    assert!(h.dialogs.errors.borrow().is_empty());
}

#[test]
fn picking_the_default_font_puts_everything_back_exactly() {
    let mut h = started();
    h.set_note_text(0, TEXT);
    let before = h.text_runs(0);
    open_manager(&mut h);
    install(&mut h, "Test_Sans.zip");
    h.settings_manager_click_row(1);
    h.advance(ms(40));
    h.settings_manager_click_row(0);
    h.advance(ms(40));
    assert_eq!(h.note_font_family(0), DEFAULT);
    assert_eq!(level(&h.text_runs(0)), level(&before));
    assert_eq!(h.settings_font_row(), (DEFAULT.to_string(), "One style".to_string()));
    assert_eq!(active(&h), [true, false]);
}

#[test]
fn a_font_picked_now_is_what_a_note_opened_later_uses() {
    let mut h = started();
    open_manager(&mut h);
    install(&mut h, "Test_Serif.zip");
    h.settings_manager_click_row(1);
    h.new_note_from(0);
    h.advance(ms(20));
    assert_eq!(h.note_font_family(1), h.note_font_family(0));
    assert!(h.note_font_family(1).starts_with("Washi Text"));
}

#[test]
fn the_font_in_use_is_saved_and_is_loaded_before_the_first_window() {
    let mut h = started();
    open_manager(&mut h);
    install(&mut h, "Test_Serif.zip");
    h.settings_manager_click_row(1);
    h.set_note_text(0, TEXT);
    let runs = h.text_runs(0);
    h.restart().unwrap();
    assert!(h.note_font_family(0).starts_with("Washi Text"), "{}", h.note_font_family(0));
    assert_eq!(level(&h.text_runs(0)), level(&runs), "the first window is already laid out in it");
    open_manager(&mut h);
    assert_eq!(active(&h), [false, true]);
    assert_eq!(h.settings_font_row().0, family("Test_Serif.zip"));
    assert!(h.dialogs.errors.borrow().is_empty());
}

// ---- removing ----

#[test]
fn removing_a_font_deletes_its_row_and_its_files_and_keeps_the_font_in_use() {
    let mut h = started();
    open_manager(&mut h);
    install(&mut h, "Test_Sans.zip");
    install(&mut h, "Test_Serif.zip");
    h.settings_manager_click_row(2);
    h.settings_manager_click_remove(1);
    assert_eq!(names(&h), [DEFAULT.to_string(), family("Test_Serif.zip")]);
    assert_eq!(active(&h), [false, true], "the one in use is still in use");
    assert_eq!(installed_folders(&h).len(), 1);
    h.restart().unwrap();
    open_manager(&mut h);
    assert_eq!(names(&h).len(), 2);
}

#[test]
fn removing_the_font_in_use_switches_to_the_default_first_and_it_stays_so_after_a_restart() {
    let mut h = started();
    h.set_note_text(0, TEXT);
    let before = h.text_runs(0);
    open_manager(&mut h);
    install(&mut h, "Test_Sans.zip");
    h.settings_manager_click_row(1);
    h.settings_manager_click_remove(1);
    h.advance(ms(40));
    assert_eq!(names(&h), [DEFAULT]);
    assert_eq!(active(&h), [true]);
    assert_eq!(h.note_font_family(0), DEFAULT);
    assert_eq!(level(&h.text_runs(0)), level(&before));
    assert_eq!(h.settings_font_row().0, DEFAULT);
    assert!(installed_folders(&h).is_empty(), "its files are gone");
    h.restart().unwrap();
    assert_eq!(h.note_font_family(0), DEFAULT);
    assert!(h.dialogs.errors.borrow().is_empty(), "nothing to report: it was removed on purpose");
}

#[test]
fn the_default_font_cannot_be_removed() {
    let mut h = started();
    open_manager(&mut h);
    h.settings_manager_click_remove(0);
    assert_eq!(names(&h), [DEFAULT]);
    assert_eq!(h.note_font_family(0), DEFAULT);
}

// ---- startup fallbacks ----

#[test]
fn a_deleted_fonts_folder_falls_back_to_the_default_font_with_one_message() {
    let mut h = started();
    open_manager(&mut h);
    install(&mut h, "Test_Serif.zip");
    h.settings_manager_click_row(1);
    h.quit();
    std::fs::remove_dir_all(h.fonts_dir()).unwrap();
    h.restart().unwrap();
    assert_eq!(h.note_font_family(0), DEFAULT);
    assert_eq!(h.dialogs.errors.borrow().len(), 1, "{:?}", h.dialogs.errors.borrow());
    assert!(h.dialogs.errors.borrow()[0].contains(&family("Test_Serif.zip")));
    h.restart().unwrap();
    assert_eq!(h.dialogs.errors.borrow().len(), 1, "reported once, not at every start");
    open_manager(&mut h);
    assert_eq!(names(&h), [DEFAULT], "the row whose folder is gone is dropped");
}

#[test]
fn a_broken_font_file_falls_back_to_the_default_font_with_one_message() {
    let mut h = started();
    open_manager(&mut h);
    install(&mut h, "Test_Serif.zip");
    h.settings_manager_click_row(1);
    h.quit();
    let folder = std::fs::read_dir(h.fonts_dir()).unwrap().next().unwrap().unwrap().path();
    std::fs::write(folder.join("regular.ttf"), b"damaged").unwrap();
    h.restart().unwrap();
    assert_eq!(h.note_font_family(0), DEFAULT);
    assert_eq!(h.dialogs.errors.borrow().len(), 1);
    h.restart().unwrap();
    assert_eq!(h.dialogs.errors.borrow().len(), 1, "reported once");
    assert_eq!(h.note_font_family(0), DEFAULT);
}

#[test]
fn an_unused_font_whose_folder_is_gone_is_dropped_without_a_message() {
    let mut h = started();
    open_manager(&mut h);
    install(&mut h, "Test_Sans.zip");
    install(&mut h, "Test_Serif.zip");
    h.quit();
    let first = installed_folders(&h).remove(0);
    std::fs::remove_dir_all(h.fonts_dir().join(first)).unwrap();
    h.restart().unwrap();
    assert!(h.dialogs.errors.borrow().is_empty());
    open_manager(&mut h);
    assert_eq!(names(&h).len(), 2);
}

// ---- text stays aligned in the picked font ----

/// The caret of every `step`th position, against the line the layout gives it: with the wide fixture
/// font at font sizes 12, 22 and 40, in a note of bold and italic runs.
#[test]
fn the_caret_stays_on_its_line_in_a_picked_font_at_sizes_12_22_and_40() {
    const NOTE: &str = "# A heading in the font\nplain **bold words** and *italic words* and ***both*** wrap over the lines of a narrow note\n- [ ] a check item that is long enough to wrap onto a second line of the note\n1. a numbered item that is long enough to wrap onto a second line of the note";
    for size in [12u32, 22, 40] {
        let mut h = started();
        h.set_tilted_editing(false);
        open_manager(&mut h);
        install(&mut h, "Test_Sans.zip");
        h.settings_manager_click_row(1);
        close_settings(&mut h);
        h.app().set_font_size(size);
        h.set_note_text(0, NOTE);
        let mut checked = 0;
        for (block, (_, text)) in h.blocks(0).iter().enumerate() {
            for offset in (0..=text.len()).filter(|&o| text.is_char_boundary(o)).step_by(5) {
                h.place_caret_at(0, block, offset);
                let (left, top, right, bottom) = h.drawn_caret(0).unwrap_or_else(|| panic!("size {size}: no caret at block {block} offset {offset}"));
                let (x, line_top, line_bottom) = h.laid_out_caret(0, block, offset);
                let at = format!("size {size} block {block} offset {offset}: caret x {left}..{right} y {top}..{bottom}, line x {x} y {line_top}..{line_bottom}");
                assert!(right - left <= 3 && (left as f32 - x).abs() <= 1.0, "{at}");
                assert!((top as f32 - line_top).abs() <= 1.0 && (bottom as f32 - line_bottom).abs() <= 1.0, "{at}");
                checked += 1;
            }
        }
        assert!(checked > 20, "{checked} positions at size {size}");
    }
}
