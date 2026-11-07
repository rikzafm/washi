//! Undo and redo for one note: a ring of snapshots (ticket 12).
//!
//! `History` knows nothing about notes or time. The note records a state when typing pauses and
//! after every structural change; `undo` and `redo` hand back the state to show.

use std::collections::VecDeque;

/// How many undo steps a note keeps; the oldest are dropped first.
pub const LIMIT: usize = 50;

pub struct History<T> {
    undo: VecDeque<T>,
    current: T,
    redo: Vec<T>,
}

impl<T: Clone + PartialEq> History<T> {
    pub fn new(initial: T) -> History<T> {
        History { undo: VecDeque::new(), current: initial, redo: Vec::new() }
    }

    /// The last recorded state.
    pub fn current(&self) -> &T {
        &self.current
    }

    /// Make `state` the newest snapshot. A state equal to the current one is no step; a new step
    /// clears the redo steps. True when a step was added.
    pub fn record(&mut self, state: T) -> bool {
        if state == self.current {
            return false;
        }
        self.undo.push_back(std::mem::replace(&mut self.current, state));
        if self.undo.len() > LIMIT {
            self.undo.pop_front();
        }
        self.redo.clear();
        true
    }

    /// Step back: the previous snapshot, now current. `None` when there is none.
    pub fn undo(&mut self) -> Option<&T> {
        let prev = self.undo.pop_back()?;
        self.redo.push(std::mem::replace(&mut self.current, prev));
        Some(&self.current)
    }

    /// Step forward again after an undo.
    pub fn redo(&mut self) -> Option<&T> {
        let next = self.redo.pop()?;
        self.undo.push_back(std::mem::replace(&mut self.current, next));
        Some(&self.current)
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn with(states: &[&str]) -> History<String> {
        let mut h = History::new(states[0].to_string());
        for s in &states[1..] {
            h.record(s.to_string());
        }
        h
    }

    #[test]
    fn undo_steps_back_through_the_recorded_states_in_reverse_order() {
        let mut h = with(&["a", "b", "c"]);
        assert_eq!(h.undo().map(String::as_str), Some("b"));
        assert_eq!(h.undo().map(String::as_str), Some("a"));
        assert_eq!(h.undo(), None);
        assert_eq!(h.current(), "a");
    }

    #[test]
    fn redo_replays_the_undone_states_in_order() {
        let mut h = with(&["a", "b", "c"]);
        h.undo();
        h.undo();
        assert_eq!(h.redo().map(String::as_str), Some("b"));
        assert_eq!(h.redo().map(String::as_str), Some("c"));
        assert_eq!(h.redo(), None);
    }

    #[test]
    fn a_new_step_after_an_undo_clears_the_redo_steps() {
        let mut h = with(&["a", "b", "c"]);
        h.undo();
        assert!(h.can_redo());
        assert!(h.record("d".into()));
        assert!(!h.can_redo());
        assert_eq!(h.undo().map(String::as_str), Some("b"));
    }

    #[test]
    fn recording_the_current_state_again_is_not_a_step() {
        let mut h = with(&["a", "b"]);
        h.undo();
        assert!(!h.record("a".into()));
        assert!(h.can_redo(), "no step, so the redo steps stay");
        assert!(!h.can_undo());
    }

    #[test]
    fn it_keeps_fifty_steps_and_drops_the_oldest_first() {
        let mut h = History::new(0u32);
        for n in 1..=60 {
            h.record(n);
        }
        let mut seen = vec![];
        while let Some(&s) = h.undo() {
            seen.push(s);
        }
        assert_eq!(seen.len(), LIMIT);
        assert_eq!(seen.first(), Some(&59));
        assert_eq!(seen.last(), Some(&10));
    }
}
