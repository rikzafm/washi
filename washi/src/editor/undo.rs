//! Undo and redo: snapshots of the document in the note's history.

use super::*;

impl Editor {
    pub fn checkpoint(&self) {
        self.history.borrow_mut().record(self.doc().clone());
    }

    pub fn structural(&self, before: Document) {
        let mut h = self.history.borrow_mut();
        h.record(before);
        h.record(self.doc().clone());
    }

    /// Undo (or redo) one step: the caret to put down after it, `None` when there is no step.
    pub fn step(&self, redo: bool) -> Option<Caret> {
        self.history_step(redo).map(|caret| caret.unwrap_or(self.selection.get().focus))
    }

    /// Undo (or redo) one step. `None` when there is no step; `Some(None)` when the step changed
    /// only blocks' kinds (a tick, a check or list toggle): the selection stays where it is. A step
    /// that changed text puts the caret at the end of the first block it changed.
    pub fn history_step(&self, redo: bool) -> Option<Option<Caret>> {
        self.typing.set(None);
        self.typing_paint.set(None);
        self.checkpoint();
        let target = {
            let mut h = self.history.borrow_mut();
            if redo { h.redo() } else { h.undo() }?.clone()
        };
        let mut doc = self.doc_mut();
        let kinds_only = doc.len() == target.len()
            && doc.blocks().iter().zip(target.blocks()).all(|(a, b)| Block { kind: b.kind, ..a.clone() } == *b);
        let at = doc.blocks().iter().zip(target.blocks()).take_while(|(a, b)| a == b).count();
        let block = at.min(target.len() - 1);
        let offset = target.blocks()[block].text.len();
        *doc = target;
        Some((!kinds_only).then_some(Caret { block, offset }))
    }

    pub fn can_undo(&self) -> bool {
        self.history.borrow().can_undo()
    }

    pub fn can_redo(&self) -> bool {
        self.history.borrow().can_redo()
    }
}
