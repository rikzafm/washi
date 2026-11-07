//! The "fully covered" check (plan 7.1), as pure geometry. The Windows side lists the
//! top-level windows above a note in z-order and hands them to [`fully_covered`].

/// A screen rectangle, `right` and `bottom` exclusive.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Rect {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

impl Rect {
    pub fn new(left: i32, top: i32, right: i32, bottom: i32) -> Self {
        Rect { left, top, right, bottom }
    }

    fn is_empty(&self) -> bool {
        self.right <= self.left || self.bottom <= self.top
    }

    /// The parts of `self` that `cut` does not cover (up to four rectangles).
    fn minus(self, cut: Rect) -> Vec<Rect> {
        let overlap = Rect::new(self.left.max(cut.left), self.top.max(cut.top), self.right.min(cut.right), self.bottom.min(cut.bottom));
        if overlap.is_empty() {
            return vec![self];
        }
        [
            Rect::new(self.left, self.top, self.right, overlap.top),
            Rect::new(self.left, overlap.bottom, self.right, self.bottom),
            Rect::new(self.left, overlap.top, overlap.left, overlap.bottom),
            Rect::new(overlap.right, overlap.top, self.right, overlap.bottom),
        ]
        .into_iter()
        .filter(|r| !r.is_empty())
        .collect()
    }
}

/// A top-level window above the note in z-order.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Above {
    pub rect: Rect,
    pub visible: bool,
    /// Hidden by DWM (another virtual desktop, a suspended UWP app): it covers nothing.
    pub cloaked: bool,
    pub minimized: bool,
}

/// True when nothing of `note` remains after subtracting every window that is visible,
/// not cloaked and not minimized. Windows below the note are simply not passed in.
pub fn fully_covered(note: Rect, above: &[Above]) -> bool {
    let mut left = if note.is_empty() { vec![] } else { vec![note] };
    for w in above.iter().filter(|w| w.visible && !w.cloaked && !w.minimized) {
        left = left.into_iter().flat_map(|r| r.minus(w.rect)).collect();
    }
    left.is_empty()
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOTE: Rect = Rect { left: 100, top: 100, right: 400, bottom: 400 };

    fn win(l: i32, t: i32, r: i32, b: i32) -> Above {
        Above { rect: Rect::new(l, t, r, b), visible: true, cloaked: false, minimized: false }
    }

    #[test]
    fn one_big_window_above_covers_the_note() {
        assert!(fully_covered(NOTE, &[win(0, 0, 1000, 800)]));
    }

    #[test]
    fn exactly_the_same_rectangle_covers_it() {
        assert!(fully_covered(NOTE, &[win(100, 100, 400, 400)]));
    }

    #[test]
    fn several_windows_together_cover_it() {
        assert!(fully_covered(NOTE, &[win(0, 0, 250, 800), win(250, 0, 1000, 250), win(200, 250, 1000, 800)]));
    }

    #[test]
    fn a_window_leaving_a_strip_does_not_cover_it() {
        assert!(!fully_covered(NOTE, &[win(0, 0, 1000, 399)]));
        assert!(!fully_covered(NOTE, &[win(101, 0, 1000, 800)]));
    }

    #[test]
    fn a_hole_in_the_middle_is_not_covered() {
        // Four windows around a 10x10 gap.
        let around = [win(0, 0, 1000, 200), win(0, 210, 1000, 800), win(0, 0, 200, 800), win(210, 0, 1000, 800)];
        assert!(!fully_covered(NOTE, &around));
    }

    #[test]
    fn no_windows_above_means_not_covered() {
        assert!(!fully_covered(NOTE, &[]));
    }

    #[test]
    fn a_cloaked_window_above_covers_nothing() {
        let cloaked = Above { cloaked: true, ..win(0, 0, 1000, 800) };
        assert!(!fully_covered(NOTE, &[cloaked]));
    }

    #[test]
    fn a_minimized_window_above_covers_nothing() {
        let minimized = Above { minimized: true, ..win(0, 0, 1000, 800) };
        assert!(!fully_covered(NOTE, &[minimized]));
    }

    #[test]
    fn an_invisible_window_above_covers_nothing() {
        let hidden = Above { visible: false, ..win(0, 0, 1000, 800) };
        assert!(!fully_covered(NOTE, &[hidden]));
    }

    #[test]
    fn a_cloaked_window_does_not_stop_a_real_one_covering() {
        let cloaked = Above { cloaked: true, ..win(0, 0, 50, 50) };
        assert!(fully_covered(NOTE, &[cloaked, win(0, 0, 1000, 800)]));
    }

    #[test]
    fn a_window_elsewhere_covers_nothing() {
        assert!(!fully_covered(NOTE, &[win(500, 500, 900, 900)]));
    }
}
