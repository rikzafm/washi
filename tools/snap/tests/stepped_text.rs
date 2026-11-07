//! Ticket 08: at rest the words sit on the paper's tilted baseline (upright glyphs, stepped per
//! word); while editing the note is level, settling in about 120 ms.
use snap::Harness;
use std::time::Duration;

fn ms(n: u64) -> Duration {
    Duration::from_millis(n)
}

const MOCKUP: &str = "# Endfield Note\n## **Thermite Cutter : POG & Akekuri**\nCurrently POG no Weapon : Ideal for him are Never Rest or White Night Nova. The last can be get in a box";

/// Let Slint's change handlers see a focus change, then read once: that starts the settle
/// animation (a drawn frame does both in the real app).
fn begin(h: &mut Harness) -> f32 {
    h.advance(ms(1));
    h.text_tilt_scale(0)
}

fn at_rest() -> Harness {
    let mut h = Harness::new();
    h.start().unwrap();
    h.set_note_text(0, MOCKUP);
    let _ = h.render(0); // drawing creates the text fields
    h.advance(ms(5));
    h.set_window_active(0, false);
    begin(&mut h);
    h.advance(ms(500));
    let _ = h.text_tilt_scale(0); // a drawn frame would observe the end of the animation
    h
}

#[test]
fn at_rest_each_word_sits_on_the_tilted_baseline() {
    let h = at_rest();
    assert_eq!(h.text_tilt_scale(0), 1.0);
    let runs = h.text_runs(0);
    assert!(runs.len() > 20, "{} words", runs.len());
    // the second body line: neighbours step along tan(1.45 deg) per px
    let slope = 1.45f32.to_radians().tan();
    let ys: Vec<f32> = runs.iter().map(|r| r.2).collect();
    let body_first = ys.iter().cloned().fold(f32::MIN, f32::max); // lowest line = last body line
    let line_y = ys.iter().cloned().filter(|&y| y < body_first - 29.0 && y > 140.0).fold(f32::MIN, f32::max);
    let line: Vec<_> = runs.iter().filter(|r| r.2 == line_y).collect();
    assert!(line.len() >= 4, "{} words on the line", line.len());
    for w in line.windows(2) {
        let anchor = |r: &&(String, f32, f32, f32, f32, f32)| r.1 + r.5 / 2.0 + r.3;
        let got = ((w[1].2 + w[1].4) - (w[0].2 + w[0].4)) / (anchor(&w[1]) - anchor(&w[0]));
        assert!((got - slope).abs() < 0.002, "step slope {got} vs {slope}");
    }
}

#[test]
fn editing_settles_the_text_flat_in_about_120_ms_and_rest_brings_the_tilt_back() {
    let mut h = at_rest();
    h.set_window_active(0, true);
    let mut seen = vec![begin(&mut h)];
    for _ in 0..14 {
        h.advance(ms(10));
        seen.push(h.text_tilt_scale(0));
    }
    assert!(seen[0] > 0.9, "settling has only just begun: {:?}", seen);
    assert!(seen.windows(2).all(|w| w[1] <= w[0]), "monotone: {seen:?}");
    assert!(seen.iter().any(|&s| s > 0.05 && s < 0.95), "passes through the middle: {seen:?}");
    assert_eq!(seen[13], 0.0, "flat within about 130 ms: {seen:?}");

    h.set_window_active(0, false);
    let mut back = vec![begin(&mut h)];
    for _ in 0..14 {
        h.advance(ms(10));
        back.push(h.text_tilt_scale(0));
    }
    assert!(back[0] < 0.1 && back.windows(2).all(|w| w[1] >= w[0]), "{back:?}");
    assert_eq!(back[13], 1.0, "tilted again within about 130 ms: {back:?}");
}

#[test]
fn the_words_follow_the_text_while_typing() {
    let mut h = at_rest();
    h.set_window_active(0, true);
    h.advance(ms(200));
    let before = h.text_runs(0).len();
    h.type_text(0, " more words here");
    assert!(h.text_runs(0).len() > before);
}
