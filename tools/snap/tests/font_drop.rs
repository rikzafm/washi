//! Installing a font by dropping its file on the Settings window while the Font manager is open
//! (ticket 08). A real drag cannot be made here: the window system's three drag events are
//! faked at the seam that turns them into an install (`settings::font_drop`).

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

fn family(name: &str) -> String {
    read_package(&std::fs::read(fixture(name)).unwrap()).unwrap().family
}

fn started() -> Harness {
    let mut h = Harness::new();
    h.start().unwrap();
    h.place_note(0, Geometry { x: 600, y: 100, ..h.note_geometry(0) });
    h
}

fn open_settings(h: &mut Harness) {
    h.toggle_dropdown(0);
    h.invoke_dropdown_settings();
    h.advance_frames(ms(600));
    h.settings_click_tab(tab::STYLE);
    h.advance_frames(ms(600));
}

fn open_manager(h: &mut Harness) {
    open_settings(h);
    h.settings_style_click(0);
    h.advance_frames(ms(600));
    assert!(h.settings_manager_open());
}

fn names(h: &Harness) -> Vec<String> {
    h.settings_manager_rows().into_iter().map(|r| r.name).collect()
}

fn fonts_folders(h: &Harness) -> usize {
    std::fs::read_dir(h.fonts_dir()).map(|d| d.count()).unwrap_or(0)
}

const DEFAULT: &str = "Shadows Into Light";

#[test]
fn dropping_a_font_zip_on_the_open_manager_installs_it_like_the_picker() {
    let mut h = started();
    open_manager(&mut h);
    h.settings_drop_file(&fixture("Test_Sans.zip"));
    h.wait_for_font_installs();
    let rows = h.settings_manager_rows();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[1].name, family("Test_Sans.zip"));
    assert_eq!(rows[1].subtitle, "Variable: weight, width");
    assert!(rows[0].active, "a dropped font is not switched to");
    assert!(h.dialogs.errors.borrow().is_empty());
}

#[test]
fn a_dropped_file_that_is_not_a_font_gets_the_pickers_refusal() {
    let mut h = started();
    open_manager(&mut h);
    let junk = h.data_dir().join("notes.ttf");
    std::fs::write(&junk, b"this is not a font").unwrap();
    h.settings_drop_file(&junk);
    h.wait_for_font_installs();
    assert_eq!(*h.dialogs.errors.borrow(), vec!["This file is not a usable font.".to_string()]);
    assert_eq!(names(&h), [DEFAULT]);
    assert_eq!(fonts_folders(&h), 0);
}

#[test]
fn a_drop_while_the_manager_is_closed_does_nothing() {
    let mut h = started();
    open_settings(&mut h);
    assert!(!h.settings_manager_open());
    h.settings_drop_file(&fixture("Test_Sans.zip"));
    h.wait_for_font_installs();
    assert_eq!(fonts_folders(&h), 0);
    assert!(h.dialogs.errors.borrow().is_empty());
    // opening the manager afterwards shows only the default font
    h.settings_style_click(0);
    h.advance_frames(ms(600));
    assert_eq!(names(&h), [DEFAULT]);
}

#[test]
fn a_drop_with_no_settings_window_does_nothing() {
    let mut h = started();
    h.app().settings_file_drag(washi::settings::font_drop::FileDrag::Drop(fixture("Test_Sans.zip")));
    h.wait_for_font_installs();
    assert_eq!(fonts_folders(&h), 0);
}

#[test]
fn several_dropped_files_are_each_installed() {
    let mut h = started();
    open_manager(&mut h);
    h.settings_drop_file(&fixture("Test_Sans.zip"));
    h.settings_drop_file(&fixture("Test_Serif.zip"));
    h.wait_for_font_installs();
    assert_eq!(names(&h).len(), 3);
    assert!(h.dialogs.errors.borrow().is_empty());
}

#[test]
fn the_install_area_lights_up_while_a_file_is_held_over_the_window() {
    let mut h = started();
    open_manager(&mut h);
    assert!(!h.settings_manager_drop_hover());
    h.settings_hover_file();
    assert!(h.settings_manager_drop_hover());
    h.settings_cancel_file_hover();
    assert!(!h.settings_manager_drop_hover());
    h.settings_hover_file();
    h.settings_drop_file(&fixture("Test_Sans.zip"));
    assert!(!h.settings_manager_drop_hover(), "letting go ends the hover");
    h.wait_for_font_installs();
}

#[test]
fn a_file_held_over_the_window_while_the_manager_is_closed_lights_nothing() {
    let mut h = started();
    open_settings(&mut h);
    h.settings_hover_file();
    assert!(!h.settings_manager_drop_hover());
}

#[test]
fn closing_the_manager_while_a_file_is_held_ends_the_hover() {
    let mut h = started();
    open_manager(&mut h);
    h.settings_hover_file();
    h.settings_style_click(0);
    h.advance_frames(ms(600));
    assert!(!h.settings_manager_open());
    assert!(!h.settings_manager_drop_hover());
}

#[test]
fn the_drop_area_text_is_the_designed_one() {
    let mut h = started();
    open_manager(&mut h);
    assert_eq!(h.settings_manager_install_hint(), "Drop a Google Fonts .zip here, or click to browse");
}
