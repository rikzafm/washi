//! Taskbar and app identity: window titles, "Always on top", "Start with Windows", the
//! second-launch handoff and the app icon. Taskbar grouping itself is in the manual QA list.

use snap::Harness;

#[test]
fn window_title_is_washi_when_empty_and_follows_the_first_line() {
    let mut h = Harness::new();
    h.start().unwrap();
    assert_eq!(h.window_title(0), "Washi Note");
    h.type_text(0, "shopping list");
    assert_eq!(h.window_title(0), "shopping list");
    h.press_enter(0);
    h.type_text(0, "milk");
    assert_eq!(h.window_title(0), "shopping list", "only the first line counts");
    for _ in 0..30 {
        h.press_backspace(0);
    }
    assert_eq!(h.note_text(0), "");
    assert_eq!(h.window_title(0), "Washi Note");
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
    h.new_note_from(0);
    h.right_click(0, None);
    assert!(!h.last_menu().pinned);
    assert!(!h.always_on_top(0) && !h.always_on_top(1));

    h.pick(&["Pin on top"]);
    assert!(h.always_on_top(0));
    assert!(!h.always_on_top(1), "the other note is unaffected");
    assert!(h.last_menu().pinned, "the open menu shows it at once");
    h.right_click(0, None);
    assert!(h.last_menu().pinned);
    h.right_click(1, None);
    assert!(!h.last_menu().pinned);

    h.right_click(0, Some(&["Pin on top"]));
    assert!(!h.always_on_top(0));
}

#[test]
fn always_on_top_is_restored_after_restart() {
    let mut h = Harness::new();
    h.start().unwrap();
    h.new_note_from(0);
    h.right_click(1, Some(&["Pin on top"]));
    h.restart().unwrap();
    assert_eq!(h.open_note_count(), 2);
    assert!(!h.always_on_top(0));
    assert!(h.always_on_top(1));
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
fn icon_renders_app_logo_at_every_size() {
    for size in washi::icon::SIZES.into_iter().chain([20, 64, 128]) {
        let px = washi::icon::rgba(size);
        assert_eq!(px.len() as u32, size * size * 4);
        let at = |x: u32, y: u32| -> [u8; 4] {
            let i = ((y * size + x) * 4) as usize;
            px[i..i + 4].try_into().unwrap()
        };
        // The four sharp outer corners are transparent outside the rounded rectangle
        assert_eq!(at(0, 0)[3], 0, "{size}px: outside corner is transparent");
        assert_eq!(at(size - 1, 0)[3], 0, "{size}px: outside top-right is transparent");
        assert_eq!(at(0, size - 1)[3], 0, "{size}px: outside bottom-left is transparent");
        assert_eq!(at(size - 1, size - 1)[3], 0, "{size}px: outside bottom-right is transparent");

        // The center contains the dark ink letter 'W'
        let center = at(size / 2, size / 2);
        assert!(
            center[0] < 60 && center[1] < 60 && center[2] < 70 && center[3] == 255,
            "{size}px: center is ink 'W', got {center:?}"
        );

        // Pink tape appears in the top-left area
        let has_pink = (0..size / 2).any(|y| {
            (0..size / 2).any(|x| {
                let p = at(x, y);
                p[0] > 200 && p[1] > 150 && p[1] < 220 && p[2] > 170 && p[2] < 230 && p[3] > 100
            })
        });
        assert!(has_pink, "{size}px: contains pink tape in top-left");

        // Light blue tape appears in the bottom-right area
        let has_blue = (size / 2..size).any(|y| {
            (size / 2..size).any(|x| {
                let p = at(x, y);
                p[0] < 200 && p[1] > 180 && p[2] > 200 && p[3] > 100
            })
        });
        assert!(has_blue, "{size}px: contains blue tape in bottom-right");

        // Paper is present inside the rounded frame
        let has_paper = (0..size).any(|y| {
            (0..size).any(|x| {
                let p = at(x, y);
                p[0] > 250 && p[1] > 250 && p[2] > 245 && p[3] == 255
            })
        });
        assert!(has_paper, "{size}px: contains paper");
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
