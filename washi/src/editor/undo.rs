//! Undo and redo: snapshots of the document in the note's history.

use super::*;

impl Editor {
    pub fn checkpoint(&self) {
        self.history.borrow_mut().record(self.doc.borrow().clone());
    }

    pub fn structural(&self, before: Document) {
        let mut h = self.history.borrow_mut();
        h.record(before);
        h.record(self.doc.borrow().clone());
    }

    pub fn step(&self, redo: bool) -> Option<Caret> {
        self.typing.set(None);
        self.typing_paint.set(None);
        self.checkpoint();
        let target = {
            let mut h = self.history.borrow_mut();
            if redo { h.redo() } else { h.undo() }?.clone()
        };
        let mut doc = self.doc.borrow_mut();
        let at = doc.blocks().iter().zip(target.blocks()).take_while(|(a, b)| a == b).count();
        let block = at.min(target.len() - 1);
        let offset = target.blocks()[block].text.len();
        *doc = target;
        Some(Caret { block, offset })
    }

    pub fn can_undo(&self) -> bool {
        self.history.borrow().can_undo()
    }

    pub fn can_redo(&self) -> bool {
        self.history.borrow().can_redo()
    }
}
