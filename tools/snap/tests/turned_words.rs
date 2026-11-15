//! At rest the words near the view are pictures of themselves turned onto the tilted baseline
//! (`washi::tilt_text`), put on whole screen pixels; words far out of sight have none, the
//! pictures follow the view when the note scrolls, and a suspended note keeps the ones it shows.

use slint::Model;
use snap::Harness;
use std::time::Duration;

fn ms(n: u64) -> Duration {
    Duration::from_millis(n)
}

const CENTER: (f32, f32) = (220.0, 200.0);

fn long_text() -> String {
    let mut t = String::from("# Long note");
    for i in 1..=60 {
        t.push_str(&format!("\nLine {i} of a long note that keeps going so the text has to scroll: a few more words."));
    }
    t
}

fn note(text: &str) -> Harness {
    let mut h = Harness::new();
    h.start().unwrap();
    h.set_note_text(0, text);
    let _ = h.render(0);
    h.advance(ms(5));
    h
}

/// Every word as (level y, picture x, picture y, picture width, picture height), logical px.
fn words(h: &Harness) -> Vec<(f32, f32, f32, f32, f32)> {
    h.app().with_note(h.id(0), |n| n.ui().get_runs().iter().map(|r| (r.y, r.img_x, r.img_y, r.img_w, r.img_h)).collect()).unwrap()
}

#[test]
fn the_words_in_view_are_pictures_on_whole_pixels_and_those_far_below_are_not() {
    let h = note(&long_text());
    let (_, paper_h) = h.paper_size(0);
    let all = words(&h);
    let in_view: Vec<_> = all.iter().filter(|w| w.0 < paper_h - 60.0).collect();
    assert!(in_view.len() > 20, "{} words in view", in_view.len());
    for w in &in_view {
        assert!(w.3 > 0.0 && w.4 > 0.0, "a word in view has a picture: {w:?}");
        // at 100% a logical px is a screen px: the picture is a little past a whole one, both ways
        for at in [w.1, w.2] {
            let off = at - at.floor();
            assert!((off - 0.13).abs() < 0.01, "on a whole pixel: {w:?}");
        }
        assert_eq!((w.3.fract(), w.4.fract()), (0.0, 0.0), "a whole number of pixels: {w:?}");
    }
    let far: Vec<_> = all.iter().filter(|w| w.0 > paper_h * 3.0).collect();
    assert!(far.len() > 20, "{} words far below", far.len());
    assert!(far.iter().all(|w| w.3 == 0.0), "words far below the view have no picture");
}

#[test]
fn the_pictures_follow_the_view_when_the_note_scrolls() {
    let mut h = note(&long_text());
    let (_, paper_h) = h.paper_size(0);
    h.pointer_move(0, CENTER.0, CENTER.1);
    h.wheel(0, CENTER.0, CENTER.1, -100000.0);
    h.advance(ms(5));
    let _ = h.render(0);
    let scroll = h.scroll_offset(0);
    assert!(scroll > paper_h * 3.0, "scrolled to the end: {scroll}");
    let all = words(&h);
    let in_view: Vec<_> = all.iter().filter(|w| w.0 > scroll + 60.0 && w.0 < scroll + paper_h - 120.0).collect();
    assert!(in_view.len() > 20, "{} words in view", in_view.len());
    assert!(in_view.iter().all(|w| w.3 > 0.0), "the words now in view have pictures");
    assert!(all.iter().filter(|w| w.0 < paper_h).all(|w| w.3 == 0.0), "the words left behind at the top have none");
}

#[test]
fn a_suspended_note_keeps_its_pictures_and_is_not_drawn_again() {
    let mut h = note("# Title\nSome words to draw on the tilted line.");
    let before = words(&h);
    assert!(before.iter().all(|w| w.3 > 0.0));
    assert!(washi::tilt_text::glyphs_kept() > 0);
    h.minimize(0);
    // the note lost the focus: let that settle and be drawn (it takes a few frames over some
    // seconds) before it is suspended
    for _ in 0..4 {
        h.advance(Duration::from_secs(1));
        let _ = h.repaint_pending(0);
    }
    h.advance(Duration::from_secs(10));
    assert!(h.is_suspended());
    assert!(!h.repaint_pending(0), "suspending must not trigger drawing");
    assert_eq!(words(&h), before, "the words keep the pictures of the frame on screen");
    assert_eq!(washi::tilt_text::glyphs_kept(), 0, "the glyphs drawn so far are forgotten");
    h.restore(0);
    h.advance(ms(50));
    let _ = h.render(0);
    assert_eq!(words(&h), before);
}
