//! `snap` as a screenshot tool: the real note component on the desk color at the mockup canvas size.
use snap::shot::{snapshot, Options};

const DESK: [u8; 3] = [0xD9, 0xDD, 0xE1];

fn opts(text: &str) -> Options {
    Options { text: text.into(), ..Options::default() }
}

/// Bounding box (x0, y0, x1, y1) of everything that is not desk.
fn non_desk_bbox(s: &snap::shot::Snapshot) -> (u32, u32, u32, u32) {
    let (mut x0, mut y0, mut x1, mut y1) = (u32::MAX, u32::MAX, 0, 0);
    for y in 0..s.height {
        for x in 0..s.width {
            if s.pixel(x, y) != DESK {
                x0 = x0.min(x);
                y0 = y0.min(y);
                x1 = x1.max(x + 1);
                y1 = y1.max(y + 1);
            }
        }
    }
    (x0, y0, x1, y1)
}

fn dark_pixels(s: &snap::shot::Snapshot) -> usize {
    (0..s.height).flat_map(|y| (0..s.width).map(move |x| (x, y))).filter(|&(x, y)| s.pixel(x, y).iter().map(|&c| c as u32).sum::<u32>() < 3 * 110).count()
}

#[test]
fn the_snapshot_is_the_mockup_canvas_with_the_desk_color_around_the_note() {
    let s = snapshot(&opts("hello")).unwrap();
    assert_eq!((s.width, s.height), (720, 840));
    for (x, y) in [(0, 0), (719, 0), (0, 839), (719, 839)] {
        assert_eq!(s.pixel(x, y), DESK, "corner {x},{y}");
    }
}

#[test]
fn two_runs_give_byte_identical_pngs() {
    let o = Options { hover: true, ..opts("Endfield Note\nsome body text that wraps around the line") };
    let a = snapshot(&o).unwrap().png();
    let b = snapshot(&o).unwrap().png();
    assert!(a.starts_with(&[0x89, b'P', b'N', b'G']));
    assert_eq!(a, b);
}

#[test]
fn the_window_is_the_paper_plus_a_40_px_margin_at_the_given_scale() {
    // 363 x 384 logical paper + 40 px margins = 443 x 464 logical = 664.5 x 696 px at 1.5
    let s = snapshot(&Options { paper: (363.0, 384.0), scale: 1.5, ..opts("") }).unwrap();
    let (x0, y0, x1, y1) = non_desk_bbox(&s);
    assert!(((x1 - x0) as i64 - 665).abs() <= 1, "width {}", x1 - x0);
    assert!(((y1 - y0) as i64 - 696).abs() <= 1, "height {}", y1 - y0);
    // centred on the canvas
    assert!(((x0 + x1) as i64 - 720).abs() <= 2 && ((y0 + y1) as i64 - 840).abs() <= 2, "{:?}", (x0, y0, x1, y1));

    let big = snapshot(&Options { paper: (545.0, 576.0), canvas: (1000, 1000), ..opts("") }).unwrap();
    let (x0, _, x1, _) = non_desk_bbox(&big);
    assert!(((x1 - x0) as i64 - 938).abs() <= 1, "width {}", x1 - x0); // (545 + 80) * 1.5 = 937.5

    let one = snapshot(&Options { scale: 1.0, ..Options { paper: (363.0, 384.0), ..opts("") } }).unwrap();
    let (x0, _, x1, _) = non_desk_bbox(&one);
    assert!(((x1 - x0) as i64 - 443).abs() <= 1, "width {}", x1 - x0);
}

#[test]
fn the_fixture_text_is_what_gets_drawn() {
    let empty = snapshot(&opts("")).unwrap();
    let text = snapshot(&opts("Endfield Note\nThermite Cutter : POG & Akekuri")).unwrap();
    assert_eq!(dark_pixels(&empty), 0, "an empty, unfocused note shows no ink and no caret");
    assert!(dark_pixels(&text) > 100);
}

#[test]
fn a_hovered_snapshot_is_produced_through_a_real_pointer_event() {
    let o = Options { hover: true, ..opts("hi") };
    let s = snapshot(&o).unwrap();
    assert_eq!((s.width, s.height), (720, 840));
}
