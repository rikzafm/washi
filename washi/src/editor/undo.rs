//! Undo and redo: snapshots of the document in the note's history.

use super::*;

/// One state in a note's history: the document, and where the caret was when it was recorded
/// (for a step, where the step was made). Two snapshots are the same state when their documents
/// are: moving the caret is no step.
#[derive(Clone, Debug)]
pub struct Snapshot {
    pub doc: Document,
    pub caret: Caret,
}

impl PartialEq for Snapshot {
    fn eq(&self, other: &Self) -> bool {
        self.doc == other.doc
    }
}

/// What one undo or redo did, and where it puts the caret.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HistoryStep {
    /// There was no step to take.
    Nothing,
    /// The step changed only blocks' kinds (a tick, a check or list toggle): the caret goes back
    /// to where it was when the step was made.
    Restored(Caret),
    /// The step changed text: the caret goes to the end of the first block it changed.
    Edited(Caret),
}

impl HistoryStep {
    /// Where to put the caret down, `None` when there was no step.
    pub fn caret(self) -> Option<Caret> {
        match self {
            HistoryStep::Nothing => None,
            HistoryStep::Restored(c) | HistoryStep::Edited(c) => Some(c),
        }
    }
}

impl Editor {
    fn snapshot(&self) -> Snapshot {
        Snapshot { doc: self.doc().clone(), caret: self.selection.get().focus }
    }

    pub fn checkpoint(&self) {
        let now = self.snapshot();
        self.history.borrow_mut().record(now);
    }

    pub fn structural(&self, before: Document) {
        let now = self.snapshot();
        let mut h = self.history.borrow_mut();
        if !h.record(Snapshot { doc: before, caret: now.caret }) {
            // the state before the step was already the newest: it was recorded with the caret
            // of its own time, so it takes the caret where the step is made
            h.current_mut().caret = now.caret;
        }
        h.record(now);
    }

    /// Undo (or redo) one step and say where the caret goes (see [`HistoryStep`]).
    pub fn history_step(&self, redo: bool) -> HistoryStep {
        self.typing.set(None);
        self.typing_paint.set(None);
        self.checkpoint();
        let target = {
            let mut h = self.history.borrow_mut();
            match if redo { h.redo() } else { h.undo() } {
                Some(s) => s.clone(),
                None => return HistoryStep::Nothing,
            }
        };
        let mut doc = self.doc_mut();
        let kinds_only = doc.len() == target.doc.len()
            && doc.blocks().iter().zip(target.doc.blocks()).all(|(a, b)| Block { kind: b.kind, ..a.clone() } == *b);
        let at = doc.blocks().iter().zip(target.doc.blocks()).take_while(|(a, b)| a == b).count();
        let block = at.min(target.doc.len() - 1);
        let offset = target.doc.blocks()[block].text.len();
        *doc = target.doc;
        if kinds_only {
            HistoryStep::Restored(target.caret)
        } else {
            HistoryStep::Edited(Caret { block, offset })
        }
    }

    pub fn can_undo(&self) -> bool {
        self.history.borrow().can_undo()
    }

    pub fn can_redo(&self) -> bool {
        self.history.borrow().can_redo()
    }
}
