//! Taskbar and app identity: window titles, "Always on top", "Start with Windows", the
//! second-launch handoff and the app icon. Taskbar grouping itself is in the manual QA list.

use snap::Harness;

fn menu_item(h: &Harness, label: &str) -> washi::platform::MenuItem {
    h.last_menu().into_iter().find(|i| i.label == label).unwrap_or_else(|| panic!("no menu item {label:?}"))
}

#[test]
fn window_title_is_washi_when_empty_and_follows_the_first_line() {
    let mut h = Harness::new();
    h.start().unwrap();
    assert_eq!(h.window_title(0), "Washi");
    h.type_text(0, "shopping list");
    assert_eq!(h.window_title(0), "shopping list");
    h.press_enter(0);
    h.type_text(0, "milk");
    assert_eq!(h.window_title(0), "shopping list", "only the first line counts");
    for _ in 0..30 {
        h.press_backspace(0);
    }
    assert_eq!(h.note_text(0), "");
    assert_eq!(h.window_title(0), "Washi");
}

#[test]
fn window_title_is_restored_for_a_saved_note() {
    let mut h = Harness::new();
    h.start().unwrap();
    h.type_text(0, "plan the trip");
    h.restart().unwrap();
    assert_eq!(h.window_title(0), "plan the trip");
}

#[test]
fn always_on_top_is_off_by_default_and_toggles_per_note() {
    let mut h = Harness::new();
    h.start().unwrap();
    h.right_click(0, Some(&["New note"]));
    h.right_click(0, None);
    let item = menu_item(&h, "Always on top");
    assert!(!item.checked && item.enabled);
    assert!(!h.always_on_top(0) && !h.always_on_top(1));

    h.right_click(0, Some(&["Always on top"]));
    assert!(h.always_on_top(0));
    assert!(!h.always_on_top(1), "the other note is unaffected");
    h.right_click(0, None);
    assert!(menu_item(&h, "Always on top").checked);
    h.right_click(1, None);
    assert!(!menu_item(&h, "Always on top").checked);

    h.right_click(0, Some(&["Always on top"]));
    assert!(!h.always_on_top(0));
}

#[test]
fn always_on_top_is_restored_after_restart() {
    let mut h = Harness::new();
    h.start().unwrap();
    h.right_click(0, Some(&["New note"]));
    h.right_click(1, Some(&["Always on top"]));
    h.restart().unwrap();
    assert_eq!(h.open_note_count(), 2);
    assert!(!h.always_on_top(0));
    assert!(h.always_on_top(1));
}

#[test]
fn start_with_windows_is_off_by_default_and_toggles_the_registration() {
    let mut h = Harness::new();
    h.start().unwrap();
    h.right_click(0, None);
    assert!(!menu_item(&h, "Start with Windows").checked);
    assert!(!h.startup_registered());

    h.right_click(0, Some(&["Start with Windows"]));
    assert!(h.startup_registered());
    h.right_click(0, None);
    assert!(menu_item(&h, "Start with Windows").checked);

    h.restart().unwrap();
    assert!(h.startup_registered(), "the setting lives outside the app process");
    h.right_click(0, Some(&["Start with Windows"]));
    assert!(!h.startup_registered());
}

#[test]
fn second_launch_opens_a_new_note_in_the_running_app_and_exits() {
    let mut h = Harness::new();
    h.start().unwrap();
    h.type_text(0, "first");
    assert_eq!(h.open_note_count(), 1);

    assert!(h.launch_again(), "the second launch hands off and exits");
    assert_eq!(h.open_note_count(), 2);
    assert_eq!(h.note_text(0), "# first");
    assert_eq!(h.note_text(1), "");

    assert!(h.launch_again());
    assert_eq!(h.open_note_count(), 3);
}

#[test]
fn a_launch_after_the_app_quit_starts_normally() {
    let mut h = Harness::new();
    h.start().unwrap();
    h.close_note(0);
    assert!(h.has_quit());
    h.start().unwrap();
    assert_eq!(h.open_note_count(), 1);
}

#[test]
fn icon_is_white_paper_with_a_pink_tape_strip_at_every_size() {
    for size in washi::icon::SIZES.into_iter().chain([20, 64, 128]) {
        let px = washi::icon::rgba(size);
        assert_eq!(px.len() as u32, size * size * 4);
        let at = |x: u32, y: u32| -> [u8; 4] {
            let i = ((y * size + x) * 4) as usize;
            px[i..i + 4].try_into().unwrap()
        };
        let (lo, _hi, tl, tr, tt, tb) = washi::icon::layout(size);
        assert_eq!(at(0, 0)[3], 0, "{size}px: outside the sheet is transparent");
        assert_eq!(at((tl + tr) / 2, (tt + tb) / 2), [0xF7, 0xBF, 0xCD, 0xFF], "{size}px: tape");
        assert_eq!(at(size / 2, size * 7 / 10), [0xFF, 0xFF, 0xFF, 0xFF], "{size}px: paper");
        assert!(lo > 0 && tt > lo, "{size}px: tape sits inside the sheet");
    }
}

#[test]
fn icon_resource_holds_every_size() {
    let png = washi::icon::png(256);
    assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
    assert_eq!(u32::from_be_bytes(png[16..20].try_into().unwrap()), 256);
    assert!(png.len() < 20_000, "flat colours must compress: {} bytes", png.len());
    let res = washi::icon::res_file();
    assert_eq!(res.len() % 4, 0);
    assert!(res.len() < 40_000, "{} bytes", res.len());
}
