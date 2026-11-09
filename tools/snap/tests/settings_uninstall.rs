//! Settings › General › Uninstall row: confirm step, launching the uninstaller, exiting cleanly.

use snap::{tab, Harness};
use std::path::PathBuf;
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

fn uninstaller() -> PathBuf {
    PathBuf::from(r"C:\Program Files\Washi\unins000.exe")
}

#[test]
fn uninstall_disabled_without_uninstaller() {
    let mut h = Harness::new();
    h.start().unwrap();
    open_settings(&mut h);
    assert!(!h.settings_uninstall_available());

    h.settings_press_uninstall();
    assert!(!h.settings_uninstall_confirming(), "a disabled button asks nothing");
    assert!(h.shell.launched.borrow().is_empty());
}

#[test]
fn uninstall_asks_first_and_cancel_goes_back() {
    let mut h = Harness::new();
    h.start().unwrap();
    *h.shell.uninstaller.borrow_mut() = Some(uninstaller());
    open_settings(&mut h);
    assert!(h.settings_uninstall_available());
    assert!(!h.settings_uninstall_confirming());

    h.settings_press_uninstall();
    assert!(h.settings_uninstall_confirming());
    assert!(h.shell.launched.borrow().is_empty(), "nothing runs before the confirm");

    h.settings_press_uninstall_cancel();
    assert!(!h.settings_uninstall_confirming());
    assert!(h.shell.launched.borrow().is_empty());
    assert!(!h.has_quit());
}

#[test]
fn uninstall_confirmed_launches_the_uninstaller_and_exits() {
    let mut h = Harness::new();
    h.start().unwrap();
    *h.shell.uninstaller.borrow_mut() = Some(uninstaller());
    open_settings(&mut h);
    h.settings_press_uninstall();
    h.settings_press_uninstall_confirm();

    assert_eq!(*h.shell.launched.borrow(), vec![uninstaller()]);
    assert!(h.has_quit(), "Washi exits so the uninstaller can remove it");
}

#[test]
fn uninstall_confirm_with_the_uninstaller_gone_does_nothing() {
    let mut h = Harness::new();
    h.start().unwrap();
    *h.shell.uninstaller.borrow_mut() = Some(uninstaller());
    open_settings(&mut h);
    h.settings_press_uninstall();
    *h.shell.uninstaller.borrow_mut() = None;
    h.settings_press_uninstall_confirm();

    assert!(h.shell.launched.borrow().is_empty());
    assert!(!h.has_quit());
}

#[test]
fn uninstall_launch_failure_is_reported_and_washi_stays() {
    let mut h = Harness::new();
    h.start().unwrap();
    *h.shell.uninstaller.borrow_mut() = Some(uninstaller());
    *h.shell.launch_error.borrow_mut() = Some("denied".into());
    open_settings(&mut h);
    h.settings_press_uninstall();
    h.settings_press_uninstall_confirm();

    assert!(!h.has_quit());
    let errors = h.dialogs.errors.borrow();
    assert_eq!(errors.len(), 1);
    assert!(errors[0].contains("denied"));
}

#[test]
fn confirm_resets_on_tab_change() {
    let mut h = Harness::new();
    h.start().unwrap();
    *h.shell.uninstaller.borrow_mut() = Some(uninstaller());
    open_settings(&mut h);
    h.settings_press_uninstall();
    assert!(h.settings_uninstall_confirming());

    h.settings_click_tab(tab::STYLE);
    h.advance_frames(ms(600));
    h.settings_click_tab(tab::GENERAL);
    h.advance_frames(ms(600));
    assert!(!h.settings_uninstall_confirming());
}
