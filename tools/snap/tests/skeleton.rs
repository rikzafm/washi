//! Walking-skeleton behavior, tested from the outside through the app harness.

use snap::Harness;
use std::time::Duration;

const CANARY: &str = "WASHI_CANARY_7f3a";

fn ms(n: u64) -> Duration {
    Duration::from_millis(n)
}

#[test]
fn first_run_shows_one_empty_note() {
    let mut h = Harness::new();
    h.start().unwrap();
    assert_eq!(h.open_note_count(), 1);
    assert_eq!(h.note_text(0), "");
    assert_eq!(h.schema_version(), 2);
}

#[test]
fn typed_text_is_back_after_restart() {
    let mut h = Harness::new();
    h.start().unwrap();
    h.type_text(0, "buy washi tape");
    h.advance(ms(500));
    h.restart().unwrap();
    assert_eq!(h.open_note_count(), 1);
    assert_eq!(h.note_text(0), "# buy washi tape");
}

#[test]
fn autosave_waits_500ms_after_the_last_edit() {
    let mut h = Harness::new();
    h.start().unwrap();
    h.type_text(0, "a");
    h.advance(ms(400));
    h.type_text(0, "b"); // restarts the debounce
    h.advance(ms(400));
    assert_eq!(h.saved_text(0), "", "800 ms after first edit but only 400 ms after the last");
    h.advance(ms(100));
    assert_eq!(h.saved_text(0), "# ab");
}

#[test]
fn closing_the_note_saves_immediately_and_quits() {
    let mut h = Harness::new();
    h.start().unwrap();
    h.type_text(0, "unsaved");
    h.close_note(0);
    assert!(h.has_quit(), "closing the last note quits Washi");
    assert_eq!(h.saved_text_of_only_note(), "# unsaved");
}

#[test]
fn quitting_saves_immediately_and_keeps_the_note_open() {
    let mut h = Harness::new();
    h.start().unwrap();
    h.type_text(0, "unsaved");
    h.quit();
    h.restart().unwrap();
    assert_eq!(h.note_text(0), "# unsaved");
}

#[test]
fn canary_never_reaches_disk_in_plaintext() {
    let mut h = Harness::new();
    h.start().unwrap();
    h.type_text(0, CANARY);
    h.advance(ms(500));

    // While running: the main file and the -wal exist and are opaque.
    let (db, wal) = h.raw_db_bytes();
    assert!(db.len() > 1024, "database file should exist");
    assert!(!wal.is_empty(), "the save should be sitting in the WAL");
    assert!(!contains(&db, CANARY.as_bytes()) && !contains(&wal, CANARY.as_bytes()));
    assert!(!db.starts_with(b"SQLite format 3\0"), "header must not be plaintext SQLite");

    // After a clean quit (checkpoint) too.
    h.quit();
    let (db, wal) = h.raw_db_bytes();
    assert!(!contains(&db, CANARY.as_bytes()) && !contains(&wal, CANARY.as_bytes()));
    assert!(!db.starts_with(b"SQLite format 3\0"));

    // ...and the text is really in there (so the checks above mean something).
    h.restart().unwrap();
    assert_eq!(h.note_text(0), format!("# {CANARY}"));
}

#[test]
fn database_uses_the_planned_settings() {
    let mut h = Harness::new();
    h.start().unwrap();
    assert_eq!(h.pragma("journal_mode"), "wal");
    assert_eq!(h.pragma("temp_store"), "2"); // MEMORY
    assert_eq!(h.pragma("secure_delete"), "1");
    assert_eq!(h.pragma("cache_size"), "-512");
}

#[test]
fn a_tampered_key_file_gives_a_clear_error() {
    let mut h = Harness::new();
    h.start().unwrap();
    h.quit();
    h.corrupt_key_file();
    let msg = h.start().err().expect("start must fail").to_string();
    assert!(msg.contains("encryption key"), "message was: {msg}");
}

#[test]
fn a_key_that_does_not_match_the_database_gives_a_clear_error() {
    let mut h = Harness::new();
    h.start().unwrap();
    h.quit();
    h.replace_key_with_another_valid_key();
    let msg = h.start().err().expect("start must fail").to_string();
    assert!(msg.contains("could not unlock"), "message was: {msg}");
}

#[test]
fn a_damaged_database_file_gives_a_clear_error() {
    let mut h = Harness::new();
    h.start().unwrap();
    h.quit();
    h.damage_database_header();
    let msg = h.start().err().expect("start must fail").to_string();
    assert!(msg.contains("could not unlock"), "message was: {msg}");
}

#[test]
fn typing_changes_the_rendered_pixels() {
    let mut h = Harness::new();
    h.start().unwrap();
    let (_, _, empty) = h.render(0);
    h.type_text(0, "Hello Washi");
    let (_, _, typed) = h.render(0);
    assert_ne!(empty, typed, "the typed text should be drawn on the paper");
}

fn contains(hay: &[u8], needle: &[u8]) -> bool {
    hay.windows(needle.len()).any(|w| w == needle)
}
