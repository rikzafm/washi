//! The exit sequence: notes closed one after another within 30s before Washi exits come back
//! open on the next start, stacked as they were when the first of them closed.

use super::{App, Inner};
use crate::platform::NoteId;
use crate::storage::Geometry;
use std::rc::Rc;
use std::time::Duration;

/// A note closed within the exit window (30s) as a candidate for reopening if the app exits.
#[derive(Clone, Debug, PartialEq)]
pub struct ClosedCandidate {
    pub id: NoteId,
    pub closed_at: Duration,
    pub was_empty: bool,
    pub geometry: Option<Geometry>,
}

/// If notes are closed within this window, they are grouped as candidates for app exit.
pub const EXIT_WINDOW: Duration = Duration::from_secs(30);

/// The notes closed so far in the current sequence, and the stacking order (bottom to top) the
/// open notes had when the first of them closed. Knows no windows, database or timers.
#[derive(Default, Debug)]
pub(crate) struct ExitSequence {
    candidates: Vec<ClosedCandidate>,
    stacking: Vec<NoteId>,
}

/// What exiting now brings back: the notes to mark open again, and the stacking order to save.
#[derive(Debug, PartialEq)]
pub(crate) struct Restore {
    pub reopen: Vec<NoteId>,
    pub stacking: Vec<NoteId>,
}

impl ExitSequence {
    pub fn candidates(&self) -> &[ClosedCandidate] {
        &self.candidates
    }

    pub fn clear(&mut self) {
        self.candidates.clear();
        self.stacking.clear();
    }

    /// Note `id` closed at `now`. A close more than `EXIT_WINDOW` after the previous one starts a
    /// new sequence; a new sequence captures `stacking()` (only called then).
    pub fn record(&mut self, id: NoteId, now: Duration, was_empty: bool, geometry: Geometry, stacking: impl FnOnce() -> Vec<NoteId>) {
        if self.candidates.last().is_some_and(|last| now.saturating_sub(last.closed_at) > EXIT_WINDOW) {
            self.clear();
        }
        if self.candidates.is_empty() {
            self.stacking = stacking();
        }
        self.candidates.push(ClosedCandidate { id, closed_at: now, was_empty, geometry: Some(geometry) });
    }

    /// Note `id` is no longer a candidate (reopened or deleted). The sequence ends with its last one.
    pub fn remove(&mut self, id: NoteId) {
        self.candidates.retain(|c| c.id != id);
        if self.candidates.is_empty() {
            self.clear();
        }
    }

    /// The UI size changed: positions held in logical px are multiplied by `k` (old / new).
    pub fn scale_geometry(&mut self, k: f32) {
        for c in &mut self.candidates {
            c.geometry = c.geometry.map(|g| g.scaled(k));
        }
    }

    /// Washi exits with notes `open` still open: end the sequence and say what comes back. `None`
    /// when no note with text was closed in it (empty notes were discarded, not closed). The order
    /// is the captured one, then any note it does not know on top, open notes before candidates.
    pub fn finish(&mut self, open: &[NoteId]) -> Option<Restore> {
        let reopen: Vec<NoteId> = self.candidates.iter().filter(|c| !c.was_empty).map(|c| c.id).collect();
        let stacking = std::mem::take(&mut self.stacking);
        self.clear();
        if reopen.is_empty() {
            return None;
        }
        let mut all = open.to_vec();
        for &id in &reopen {
            if !all.contains(&id) {
                all.push(id);
            }
        }
        let mut ordered: Vec<NoteId> = stacking.into_iter().filter(|id| all.contains(id)).collect();
        for id in all {
            if !ordered.contains(&id) {
                ordered.push(id);
            }
        }
        Some(Restore { reopen, stacking: ordered })
    }
}

impl App {
    /// Notes closed within the exit window (30s) that would reopen if the app exits.
    pub fn exit_candidates(&self) -> Vec<ClosedCandidate> {
        self.0.exit.borrow().candidates().to_vec()
    }
}

impl Inner {
    /// Ends the exit sequence without restoring anything, and stops its timer.
    fn clear_exit_sequence(&self) {
        self.exit.borrow_mut().clear();
        self.exit_timer.stop();
    }

    /// Records a closed note as an exit sequence candidate; the sequence ends 30s after the last close.
    pub(super) fn record_closed_candidate(self: &Rc<Self>, id: NoteId, was_empty: bool, geometry: Geometry) {
        let now = self.env.clock.monotonic();
        // taken out so the stacking query runs with no borrow held
        let mut exit = self.exit.take();
        exit.record(id, now, was_empty, geometry, || self.current_stacking());
        *self.exit.borrow_mut() = exit;
        let weak = Rc::downgrade(self);
        self.exit_timer.start(slint::TimerMode::SingleShot, EXIT_WINDOW, move || {
            if let Some(inner) = weak.upgrade() {
                inner.clear_exit_sequence();
            }
        });
    }

    /// Removes an exit candidate if present; ends the sequence if none remain.
    pub(super) fn remove_exit_candidate(&self, id: NoteId) {
        let mut exit = self.exit.borrow_mut();
        exit.remove(id);
        if exit.candidates().is_empty() {
            self.exit_timer.stop();
        }
    }

    /// Stacking order (bottom to top) of the currently open notes.
    fn current_stacking(&self) -> Vec<NoteId> {
        let Ok(notes) = self.notes.try_borrow() else { return Vec::new() };
        let windows: Vec<(NoteId, Option<isize>)> = notes.iter().map(|n| (n.id(), n.hwnd())).collect();
        drop(notes);
        self.env.window_control.stacking_order(&windows)
    }

    /// Remember which note window is above which, for the next start.
    pub(super) fn save_stacking(&self) {
        let order = self.current_stacking();
        if let Err(e) = self.db.save_stacking(&order) {
            eprintln!("washi: could not save the stacking order of the notes: {e}");
        }
    }

    /// Restore notes closed during the exit sequence (or session end) back to open in the DB.
    pub(super) fn restore_exit_candidates(&self) {
        let open: Vec<NoteId> = self.notes.try_borrow().map(|n| n.iter().map(|n| n.id()).collect()).unwrap_or_default();
        let restore = self.exit.borrow_mut().finish(&open);
        self.exit_timer.stop();
        let Some(restore) = restore else { return };
        for &id in &restore.reopen {
            if let Err(e) = self.db.set_open(id, true) {
                eprintln!("washi: could not mark note {id} open on exit: {e}");
            }
        }
        if let Err(e) = self.db.save_stacking(&restore.stacking) {
            eprintln!("washi: could not save stacking order on exit: {e}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const G: Geometry = Geometry { x: 10, y: 20, w: 300, h: 200 };

    fn secs(s: u64) -> Duration {
        Duration::from_secs(s)
    }

    fn ids(seq: &ExitSequence) -> Vec<NoteId> {
        seq.candidates().iter().map(|c| c.id).collect()
    }

    #[test]
    fn closes_within_the_window_accumulate() {
        let mut seq = ExitSequence::default();
        seq.record(1, secs(0), false, G, || vec![1, 2, 3]);
        seq.record(2, secs(20), false, G, || panic!("stacking is captured once per sequence"));
        seq.record(3, secs(45), false, G, || panic!("the window counts from the previous close"));
        assert_eq!(ids(&seq), [1, 2, 3]);
    }

    #[test]
    fn a_close_after_the_window_starts_a_new_sequence() {
        let mut seq = ExitSequence::default();
        seq.record(1, secs(0), false, G, || vec![1, 2]);
        seq.record(2, secs(31), false, G, || vec![2]);
        assert_eq!(ids(&seq), [2]);
        assert_eq!(seq.finish(&[]), Some(Restore { reopen: vec![2], stacking: vec![2] }));
    }

    #[test]
    fn removing_the_last_candidate_ends_the_sequence() {
        let mut seq = ExitSequence::default();
        seq.record(1, secs(0), false, G, || vec![1, 2]);
        seq.record(2, secs(1), false, G, Vec::new);
        seq.remove(1);
        assert_eq!(ids(&seq), [2]);
        seq.remove(2);
        assert!(seq.candidates().is_empty());
        // the old stacking is gone: the next close captures afresh
        seq.record(3, secs(2), false, G, || vec![3]);
        assert_eq!(seq.finish(&[]).unwrap().stacking, [3]);
    }

    #[test]
    fn finish_brings_back_only_notes_with_text() {
        let mut seq = ExitSequence::default();
        seq.record(1, secs(0), true, G, || vec![1, 2]);
        seq.record(2, secs(1), false, G, Vec::new);
        assert_eq!(seq.finish(&[]), Some(Restore { reopen: vec![2], stacking: vec![2] }));
        assert!(seq.candidates().is_empty());
    }

    #[test]
    fn finish_with_only_empty_notes_restores_nothing_and_clears() {
        let mut seq = ExitSequence::default();
        seq.record(1, secs(0), true, G, || vec![1]);
        assert_eq!(seq.finish(&[]), None);
        assert!(seq.candidates().is_empty());
    }

    #[test]
    fn finish_keeps_the_captured_order_and_puts_unknown_notes_on_top() {
        let mut seq = ExitSequence::default();
        // bottom to top when the first note closed: 3, 1, 2
        seq.record(2, secs(0), false, G, || vec![3, 1, 2]);
        seq.record(1, secs(1), false, G, Vec::new);
        // note 5 opened during the sequence and is still open; note 3 is gone (deleted)
        let restore = seq.finish(&[5]).unwrap();
        assert_eq!(restore.reopen, [2, 1]);
        assert_eq!(restore.stacking, [1, 2, 5]);
    }

    #[test]
    fn scale_geometry_carries_positions_to_the_new_ui_size() {
        let mut seq = ExitSequence::default();
        seq.record(1, secs(0), false, G, Vec::new);
        seq.scale_geometry(0.5);
        assert_eq!(seq.candidates()[0].geometry, Some(G.scaled(0.5)));
    }
}
