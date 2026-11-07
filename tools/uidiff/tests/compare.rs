//! `uidiff --compare` through its library seam, on the real reference mockups.
use std::path::PathBuf;
use uidiff::compare::{compare, Options};
use uidiff::{Image, Spec};

fn design_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../docs/design")
}

fn spec() -> Spec {
    serde_json::from_str(&std::fs::read_to_string(design_dir().join("spec.json")).unwrap()).unwrap()
}

fn mockup(name: &str) -> Image {
    Image::load_png(&design_dir().join(format!("reference/mockup-{name}.png"))).unwrap()
}

const DESK: [u8; 3] = [0xD9, 0xDD, 0xE1];

/// Copy of `img` with the pixels of `rect` (x0,y0,x1,y1) moved by (dx,dy); the vacated area becomes `fill`.
fn move_rect(img: &Image, rect: [usize; 4], dx: i64, dy: i64, fill: [u8; 3]) -> Image {
    let mut out = img.clone();
    for y in rect[1]..rect[3] {
        for x in rect[0]..rect[2] {
            out.set(x, y, fill);
        }
    }
    for y in rect[1]..rect[3] {
        for x in rect[0]..rect[2] {
            out.set((x as i64 + dx) as usize, (y as i64 + dy) as usize, img.get(x, y));
        }
    }
    out
}

fn shifted(img: &Image, dx: i64, dy: i64) -> Image {
    let mut out = Image::new(img.w, img.h, DESK);
    for y in 0..img.h {
        for x in 0..img.w {
            let (sx, sy) = (x as i64 - dx, y as i64 - dy);
            if sx >= 0 && sy >= 0 && (sx as usize) < img.w && (sy as usize) < img.h {
                out.set(x, y, img.get(sx as usize, sy as usize));
            }
        }
    }
    out
}

fn hints(c: &uidiff::compare::Comparison) -> String {
    c.rows.iter().filter_map(|r| r.hint.clone()).collect::<Vec<_>>().join("\n")
}

fn failing(c: &uidiff::compare::Comparison) -> Vec<String> {
    c.rows.iter().filter(|r| !r.pass).map(|r| format!("{}/{}", r.group, r.item)).collect()
}

#[test]
fn a_mockup_compared_with_itself_passes_with_zero_difference() {
    let spec = spec();
    for name in ["hover", "idle"] {
        let img = mockup(name);
        let c = compare(&spec, name, &img, &img, &Options::default()).unwrap();
        assert!(c.passed(), "{name}: failing rows {:?}", failing(&c));
        assert_eq!(c.diff_pixels, 0, "{name}");
        assert!(c.rows.iter().any(|r| r.group == "image"), "{name}: whole-image row present");
    }
}

#[test]
fn a_copy_shifted_by_two_pixels_fails_with_the_direction_to_move_it_back() {
    let spec = spec();
    let img = mockup("hover");
    let snap = shifted(&img, 2, 0); // everything 2 px to the right
    let c = compare(&spec, "hover", &img, &snap, &Options { register: false, ..Options::default() }).unwrap();
    assert!(!c.passed());
    assert!(c.rows.iter().any(|r| r.group == "paper" && !r.pass), "paper rows fail: {:?}", failing(&c));
    let h = hints(&c);
    assert!(h.contains("move left 2 px"), "hints were:\n{h}");

    let snap = shifted(&img, 0, -2); // everything 2 px up
    let c = compare(&spec, "hover", &img, &snap, &Options { register: false, ..Options::default() }).unwrap();
    assert!(hints(&c).contains("move down 2 px"), "hints were:\n{}", hints(&c));
}

#[test]
fn moving_only_the_title_by_three_pixels_fails_the_title_and_says_which_way() {
    let spec = spec();
    let img = mockup("hover");
    // the title's ink box is [145,164,402,221]; move a generous box around it 3 px down
    let snap = move_rect(&img, [130, 160, 440, 235], 0, 3, [0xFF, 0xFF, 0xFD]);
    let c = compare(&spec, "hover", &img, &snap, &Options::default()).unwrap();
    assert!(!c.passed());
    let bad = failing(&c);
    assert!(bad.iter().any(|b| b.starts_with("title/")), "failing rows {bad:?}");
    assert!(!bad.iter().any(|b| b.starts_with("paper/") || b.starts_with("toolbar/")), "only the title fails: {bad:?}");
    let h = hints(&c);
    assert!(h.contains("title") && h.contains("move up 3 px"), "hints were:\n{h}");
}

#[test]
fn registration_ignores_where_the_note_sits_in_the_snapshot() {
    let spec = spec();
    let img = mockup("hover");
    let snap = shifted(&img, 12, 7);
    let c = compare(&spec, "hover", &img, &snap, &Options::default()).unwrap();
    assert_eq!(c.shift, (-12, -7));
    assert!(c.passed(), "failing: {:?}", failing(&c));
}

#[test]
fn a_snapshot_with_no_paper_fails_the_paper_and_says_so() {
    let spec = spec();
    let img = mockup("hover");
    let snap = Image::new(img.w, img.h, DESK);
    let c = compare(&spec, "hover", &img, &snap, &Options::default()).unwrap();
    assert!(!c.passed());
    assert!(c.rows.iter().any(|r| r.group == "paper" && r.item == "found" && !r.pass));
}

#[test]
fn the_idle_mockups_caption_is_masked_so_it_does_not_count() {
    let spec = spec();
    let img = mockup("idle");
    // scribble inside the masked caption rect [128,180,360,220]: must not matter
    let mut snap = img.clone();
    for x in 140..300 {
        snap.set(x, 200, [0, 0, 0]);
    }
    let c = compare(&spec, "idle", &img, &snap, &Options::default()).unwrap();
    assert!(c.passed(), "failing: {:?}", failing(&c));
}
