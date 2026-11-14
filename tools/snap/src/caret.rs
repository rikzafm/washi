//! The caret as drawn against the caret as laid out: where the edit field paints its caret, and the
//! line box the app's own layout (src/textflow.rs) gives the same position.

use crate::harness::Harness;
use crate::platform;
use slint::ComponentHandle;
use std::time::Duration;

/// A box in a note's window, in screen px: left, top, right, bottom (the last two exclusive).
pub type PxBox = (u32, u32, u32, u32);

impl Harness {
    /// Put the caret at byte `offset` of `block` (nothing selected).
    pub fn place_caret_at(&mut self, note: usize, block: usize, offset: usize) {
        let id = self.id(note);
        let caret = washi::document::Caret { block, offset };
        self.app().with_note(id, |n| n.editor().set_selection(n.ui(), washi::document::Range { anchor: caret, focus: caret }));
        self.settle();
    }

    /// Move the note's window to a monitor at `scale` (its DPI / 96).
    pub fn move_to_monitor(&mut self, note: usize, scale: f32) {
        platform::windows().remove(note).os_move_to_monitor(scale);
        self.settle();
    }

    /// The text and kind ("title", "body", ...) of each of the note's blocks.
    pub fn blocks(&self, note: usize) -> Vec<(String, String)> {
        use slint::Model;
        self.app()
            .with_note(self.id(note), |n| n.ui().get_blocks().iter().map(|b| (format!("{:?}", b.kind).to_lowercase(), b.text.to_string())).collect())
            .unwrap()
    }

    /// Where the app's layout has the caret at byte `offset` of `block`, in the note's window in
    /// logical px: the caret's x, and the top and bottom of its line box.
    pub fn laid_out_caret(&self, note: usize, block: usize, offset: usize) -> (f32, f32, f32) {
        self.app()
            .with_note(self.id(note), |n| {
                let (ui, editor) = (n.ui(), n.editor());
                let layout = editor.layout(ui);
                let (top, bottom) = layout.caret_span(block, offset).expect("the block exists");
                // from the text area's left edge, past a check item's box
                let x = layout.caret_line(block, offset, false).expect("the block exists").x + layout.tokens().inset_left + layout.indent(block);
                let (margin, scroll) = (ui.get_window_margin() as f64, ui.get_scroll_y() as f64);
                ((margin + x) as f32, (margin + top - scroll) as f32, (margin + bottom - scroll) as f32)
            })
            .unwrap()
    }

    /// Where the note's edit fields end and where the app's layout ends, in logical px of the
    /// paper's frame: the bottom of the last field and the bottom of the last laid-out line. They are
    /// the same when every field wraps its text to as many lines as the layout does.
    pub fn fields_and_layout_bottom(&self, note: usize) -> (f32, f32) {
        // the fields are laid out for a frame: draw one, so their heights are the current text's
        let _ = self.render(note);
        self.app()
            .with_note(self.id(note), |n| (n.ui().get_fields_bottom(), n.editor().layout(n.ui()).content_height() as f32))
            .unwrap()
    }

    /// The center of the `i`th check box (in document order) in the note's window, logical px, as
    /// laid out level and scrolled: where a click ticks it.
    pub fn check_box_center(&self, note: usize, i: usize) -> (f32, f32) {
        self.app()
            .with_note(self.id(note), |n| {
                let (ui, editor) = (n.ui(), n.editor());
                let layout = editor.layout(ui);
                let b = layout.checks().get(i).expect("the check box exists").clone();
                let (margin, scroll) = (ui.get_window_margin() as f64, ui.get_scroll_y() as f64);
                ((margin + b.x + b.size / 2.0) as f32, (margin + b.y + b.size / 2.0 - scroll) as f32)
            })
            .unwrap()
    }

    /// Where the caret at byte `offset` of `block` should be drawn when the text stays tilted while
    /// editing: [`Self::laid_out_caret`] carried onto the paper's tilted baseline like a word is (the
    /// offset of the center of its line box), in the same terms: x, top and bottom in logical px.
    pub fn tilted_caret(&self, note: usize, block: usize, offset: usize) -> (f32, f32, f32) {
        let (x, top, bottom) = self.laid_out_caret(note, block, offset);
        self.app()
            .with_note(self.id(note), |n| {
                let (ui, editor) = (n.ui(), n.editor());
                let margin = ui.get_window_margin() as f64;
                let (pw, ph) = editor.paper.get();
                let tilt = editor.tokens(ui).tilt_deg;
                let scroll = ui.get_scroll_y() as f64;
                // the layer is the paper's frame, scrolled; the tilt is about the paper's center
                let (cx, cy) = (x as f64 - margin, (top + bottom) as f64 / 2.0 - margin + scroll);
                let (dx, dy) = washi::textflow::tilt_offset(cx, cy, (pw, ph), tilt);
                let (s, c) = tilt.to_radians().sin_cos();
                let (dx, dy) = (dx + scroll * s, dy + scroll * (1.0 - c));
                ((x as f64 + dx) as f32, (top as f64 + dy) as f32, (bottom as f64 + dy) as f32)
            })
            .unwrap()
    }

    /// Draw the note's window anew (whether it asked for a repaint or not): width, height, pixels.
    pub fn redraw(&mut self, note: usize) -> (u32, u32, Vec<slint::Rgb8Pixel>) {
        self.app().with_note(self.id(note), |n| n.ui().window().request_redraw());
        self.render(note)
    }

    /// The caret as the note's window draws it: the box of the pixels that change when the caret
    /// blinks off. `None` when nothing blinks (no field has the focus, or the caret is out of view).
    pub fn drawn_caret(&mut self, note: usize) -> Option<PxBox> {
        let shot = |h: &mut Harness| h.redraw(note);
        // a frame for the note to follow the caret (it scrolls to it when drawn); then the text
        // settles flat and the scrollbar that scrolling showed fades away again
        shot(self);
        self.advance(Duration::from_millis(1000));
        shot(self);
        let (w, _, first) = shot(self);
        let mut found: Option<PxBox> = None;
        for _ in 0..5 {
            self.advance(Duration::from_millis(250));
            let (_, _, frame) = shot(self);
            for (i, (a, b)) in first.iter().zip(&frame).enumerate() {
                if a != b {
                    let (x, y) = (i as u32 % w, i as u32 / w);
                    let f = found.get_or_insert((x, y, x + 1, y + 1));
                    *f = (f.0.min(x), f.1.min(y), f.2.max(x + 1), f.3.max(y + 1));
                }
            }
        }
        found
    }
}
