//! Settings › General: Start with Windows, the new-note Always on top default, folders, Resource use.

use snap::{tab, Harness};
use std::time::Duration;
use washi::storage::Geometry;

fn ms(n: u64) -> Duration {
    Duration::from_millis(n)
}

fn open_settings(h: &mut Harness) {
    h.place_note(0, Geometry { x: 600, y: 100, ..h.note_geometry(0) });
    h.toggle_dropdown(0);
    h.invoke_dropdown_settings();
    h.advance_frames(ms(600));
    assert!(h.is_settings_open());
}

fn close_settings(h: &mut Harness) {
    h.settings_close();
    h.advance_frames(ms(600));
}

fn new_note(h: &mut Harness) -> usize {
    h.new_note_from(0);
    h.open_note_count() - 1
}

#[test]
fn start_with_windows_is_the_registry_value() {
    let mut h = Harness::new();
    h.start().unwrap();
    open_settings(&mut h);
    assert!(!h.settings_start_with_windows());

    h.settings_press_start_with_windows();
    assert!(h.startup.enabled.get(), "the registration was written");
    assert!(h.settings_start_with_windows());

    // changed elsewhere (the right-click menu flips the same value): Settings shows it when reopened
    close_settings(&mut h);
    h.startup.enabled.set(false);
    open_settings(&mut h);
    assert!(!h.settings_start_with_windows());

    h.settings_press_start_with_windows();
    h.settings_press_start_with_windows();
    assert!(!h.startup.enabled.get());
    assert!(!h.settings_start_with_windows());
}

#[test]
fn always_on_top_default_applies_to_new_notes_only_and_persists() {
    let mut h = Harness::new();
    h.start().unwrap();
    open_settings(&mut h);
    assert!(!h.settings_new_notes_on_top(), "off by default");

    h.settings_press_always_on_top();
    assert!(h.settings_new_notes_on_top());
    assert!(!h.always_on_top(0), "existing notes are not touched");
    close_settings(&mut h);

    let b = new_note(&mut h);
    assert!(h.always_on_top(b), "a new note starts pinned");
    assert!(!h.always_on_top(0), "the existing note stays as it was");

    h.restart().unwrap();
    assert!(!h.always_on_top(0));
    assert!(h.always_on_top(1), "the pin of the new note was saved");
    let c = new_note(&mut h);
    assert!(h.always_on_top(c), "the default survives a restart");
    open_settings(&mut h);
    assert!(h.settings_new_notes_on_top());

    h.settings_press_always_on_top();
    close_settings(&mut h);
    let d = new_note(&mut h);
    assert!(!h.always_on_top(d), "turned off again");
    assert!(h.always_on_top(c), "pins already given stay");
}

#[test]
fn the_first_note_of_a_fresh_profile_follows_the_default() {
    let mut h = Harness::new();
    h.start().unwrap();
    open_settings(&mut h);
    h.settings_press_always_on_top();
    close_settings(&mut h);
    h.close_note(0);
    h.restart().unwrap();
    assert!(h.always_on_top(0), "the note made for an empty profile is a new note");
}

#[test]
fn the_open_buttons_open_the_right_folders() {
    let mut h = Harness::new();
    h.start().unwrap();
    open_settings(&mut h);

    h.settings_press_open_install();
    h.settings_press_open_data();

    let opened = h.shell.opened.borrow().clone();
    assert_eq!(opened, vec![std::path::PathBuf::from(r"C:\Program Files\Washi"), h.data_dir()]);
    let [install, data] = h.settings_folders();
    assert_eq!(install, r"C:\Program Files\Washi");
    assert!(!data.is_empty());
}

#[test]
fn resource_use_shows_live_values() {
    let mut h = Harness::new();
    h.start().unwrap();
    h.stats.memory.set(64 * 1024 * 1024);
    open_settings(&mut h);
    let [mem, cpu, _db, notes] = h.settings_stats();
    assert_eq!(mem, "64 MB");
    assert_eq!(cpu, "…", "no CPU figure before the second sample");
    assert_eq!(notes, "1");

    // 40 ms of CPU over one second on 4 processors = 1.0%
    h.stats.cpu.set(h.stats.cpu.get() + ms(40));
    h.stats.memory.set(80 * 1024 * 1024);
    new_note(&mut h);
    h.advance_frames(ms(1000));
    let [mem, cpu, db, notes] = h.settings_stats();
    assert_eq!(mem, "80 MB");
    assert_eq!(cpu, "1.0%");
    assert_eq!(notes, "2");
    assert!(db.ends_with("KB") || db.ends_with("MB"), "{db}");
}

#[test]
fn stats_refresh_only_while_the_general_tab_shows() {
    let mut h = Harness::new();
    h.start().unwrap();
    open_settings(&mut h);
    let at_rest = h.stats.samples.get();
    assert!(at_rest >= 1, "a sample is taken as soon as General shows");

    h.advance_frames(ms(3000));
    assert_eq!(h.stats.samples.get(), at_rest + 3, "one a second");

    h.settings_click_tab(tab::SHORTCUTS);
    h.advance_frames(ms(300));
    let off = h.stats.samples.get();
    h.advance_frames(ms(3000));
    assert_eq!(h.stats.samples.get(), off, "no refresh on another tab");

    h.settings_click_tab(tab::GENERAL);
    h.advance_frames(ms(300));
    assert!(h.stats.samples.get() > off, "General refreshes again at once");
    let back = h.stats.samples.get();
    h.advance_frames(ms(2000));
    assert_eq!(h.stats.samples.get(), back + 2);

    close_settings(&mut h);
    let closed = h.stats.samples.get();
    h.advance_frames(ms(3000));
    assert_eq!(h.stats.samples.get(), closed, "no refresh once the page is closed");
}

#[test]
fn stats_stop_when_the_page_starts_closing() {
    let mut h = Harness::new();
    h.start().unwrap();
    open_settings(&mut h);
    h.settings_close();
    h.advance_frames(ms(100));
    let n = h.stats.samples.get();
    h.advance_frames(ms(3000));
    assert_eq!(h.stats.samples.get(), n);
}
