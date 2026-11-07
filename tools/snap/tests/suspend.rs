//! Suspending when every note is minimized, and waking again, tested through the app harness
//! with the fake clock.

use snap::Harness;
use std::time::{Duration, Instant};

fn ms(n: u64) -> Duration {
    Duration::from_millis(n)
}

fn secs(n: u64) -> Duration {
    Duration::from_secs(n)
}

fn three_notes() -> Harness {
    let mut h = Harness::new();
    h.start().unwrap();
    h.press_ctrl(0, 'n');
    h.press_ctrl(0, 'n');
    assert_eq!(h.open_note_count(), 3);
    h
}

fn minimize_all(h: &mut Harness) {
    for n in 0..h.open_note_count() {
        h.minimize(n);
    }
}

/// A harness whose notes are all minimized and suspended, with the last frames drawn first.
fn suspended() -> Harness {
    let mut h = three_notes();
    for n in 0..3 {
        h.render(n);
    }
    minimize_all(&mut h);
    h.advance(secs(10));
    assert!(h.is_suspended());
    h
}

// ---- when to suspend ----

#[test]
fn not_suspended_at_9_seconds_suspended_at_10() {
    let mut h = three_notes();
    minimize_all(&mut h);
    h.advance(secs(9));
    assert!(!h.is_suspended());
    assert!(!h.power.efficiency.get());
    h.advance(ms(999));
    assert!(!h.is_suspended());
    h.advance(ms(1));
    assert!(h.is_suspended());
    assert!(h.power.efficiency.get());
}

#[test]
fn never_suspends_while_a_note_is_not_minimized() {
    let mut h = three_notes();
    h.minimize(0);
    h.minimize(1);
    h.advance(secs(60));
    assert!(!h.is_suspended());
}

#[test]
fn restoring_during_the_grace_period_cancels_it() {
    let mut h = three_notes();
    minimize_all(&mut h);
    h.advance(secs(5));
    h.restore(1);
    h.advance(secs(60));
    assert!(!h.is_suspended());
}

#[test]
fn quick_minimize_and_restore_never_suspends() {
    let mut h = three_notes();
    for _ in 0..20 {
        minimize_all(&mut h);
        h.advance(secs(6));
        h.restore(0);
        h.advance(secs(6));
    }
    assert!(!h.is_suspended());
    // Each minimize starts a fresh 10 s, not what is left of an old one.
    minimize_all(&mut h);
    h.advance(secs(9));
    assert!(!h.is_suspended());
    h.advance(secs(1));
    assert!(h.is_suspended());
}

#[test]
fn a_new_note_during_the_grace_period_cancels_it() {
    let mut h = three_notes();
    minimize_all(&mut h);
    h.advance(secs(5));
    h.press_ctrl(0, 'n'); // opens a visible note
    h.advance(secs(60));
    assert!(!h.is_suspended());
}

#[test]
fn any_change_during_the_grace_period_restarts_the_10_seconds() {
    let mut h = three_notes();
    minimize_all(&mut h);
    h.advance(secs(6));
    // Close one minimized note: still all minimized, but the picture changed.
    h.close_note(2);
    h.advance(secs(6));
    assert!(!h.is_suspended(), "12 s after the first minimize, 6 s after the change");
    h.advance(secs(4));
    assert!(h.is_suspended());
}

// ---- what suspending does ----

#[test]
fn pending_edits_and_geometry_are_saved_before_suspending() {
    let mut h = three_notes();
    minimize_all(&mut h);
    h.advance(ms(9_800));
    h.type_text(0, "last words"); // autosave is still waiting for its 500 ms
    h.resize_note(0, 300, 320);
    h.advance(ms(200));
    assert!(h.is_suspended());
    assert_eq!(h.saved_text(0), "last words");
    let id = h.note_id(0);
    let g = h.saved_geometry(id).unwrap();
    assert_eq!((g.w, g.h), (300, 320));
}

#[test]
fn suspending_releases_memory_and_then_turns_efficiency_mode_on() {
    let h = suspended();
    assert_eq!(*h.power.log.borrow(), ["release_memory", "efficiency_on"]);
}

#[test]
fn a_suspended_app_has_no_timers_at_all() {
    let mut h = three_notes();
    h.type_text(0, "x"); // an autosave timer is running
    minimize_all(&mut h);
    h.advance(secs(9));
    h.advance(secs(1));
    assert!(h.is_suspended());
    assert_eq!(h.next_timer_due(), None, "nothing scheduled, so the loop sleeps until a message");
}

#[test]
fn notes_look_unchanged_while_suspended() {
    let mut h = three_notes();
    h.type_text(1, "still here");
    h.advance(ms(600));
    for n in 0..3 {
        h.render(n);
    }
    minimize_all(&mut h);
    h.advance(secs(10));
    assert!(h.is_suspended());
    assert_eq!(h.note_text(1), "still here");
    // The last frame stays on screen: nothing asked the windows to draw anything else.
    for n in 0..3 {
        assert!(!h.repaint_pending(n), "suspending must not trigger drawing");
    }
}

// ---- waking ----

fn assert_awake_with_full_repaint(h: &mut Harness) {
    assert!(!h.is_suspended());
    assert!(!h.power.efficiency.get(), "efficiency mode off");
    assert_eq!(h.power.log.borrow().last().map(String::as_str), Some("efficiency_off"));
    for n in 0..h.open_note_count() {
        assert!(h.repaint_pending(n), "note {n} gets a full repaint");
    }
}

#[test]
fn restoring_a_note_wakes_the_app() {
    let mut h = suspended();
    h.restore(1);
    assert_awake_with_full_repaint(&mut h);
}

#[test]
fn focusing_a_note_wakes_the_app() {
    let mut h = suspended();
    h.focus(2);
    assert_awake_with_full_repaint(&mut h);
}

#[test]
fn hovering_a_note_wakes_the_app() {
    let mut h = suspended();
    h.hover(0);
    assert_awake_with_full_repaint(&mut h);
}

#[test]
fn a_repaint_request_wakes_the_app() {
    let mut h = suspended();
    h.paint_request(0);
    assert_awake_with_full_repaint(&mut h);
}

#[test]
fn waking_restarts_timers_so_edits_are_saved_again() {
    let mut h = suspended();
    h.restore(0);
    h.type_text(0, "back again");
    h.advance(ms(600));
    assert_eq!(h.saved_text(0), "back again");
}

#[test]
fn the_first_frame_after_waking_is_drawn_within_50_ms() {
    let mut h = suspended();
    let started = Instant::now();
    h.restore(0);
    let drawn = h.draw_pending(0);
    let took = started.elapsed();
    assert!(drawn, "the first frame was drawn");
    eprintln!("wake-to-first-frame: {took:?}");
    assert!(took < ms(50), "wake to first frame took {took:?}");
}

#[test]
fn a_woken_app_that_is_still_minimized_suspends_again_after_10_seconds() {
    let mut h = suspended();
    h.hover(0);
    assert!(!h.is_suspended());
    h.advance(secs(9));
    assert!(!h.is_suspended());
    h.advance(secs(1));
    assert!(h.is_suspended());
}

#[test]
fn a_woken_app_can_suspend_again_after_minimizing_again() {
    let mut h = suspended();
    h.restore(0);
    h.restore(1);
    h.restore(2);
    h.advance(secs(30));
    assert!(!h.is_suspended());
    minimize_all(&mut h);
    h.advance(secs(10));
    assert!(h.is_suspended());
    assert_eq!(h.power.log.borrow().iter().filter(|e| *e == "efficiency_on").count(), 2);
}
