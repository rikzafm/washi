//! Pointer presses, drags, multi-click and autoscroll.

use super::*;

impl Editor {
    pub fn pointer_down(&self, ui: &NoteWindow, at: (f32, f32), shift: bool, now: Duration) {
        let mut p = self.pointer.get();
        let again = p.last_press.is_some_and(|(t, (x, y))| {
            now.saturating_sub(t) <= MULTI_CLICK && (x - at.0).abs() <= MULTI_CLICK_SLOP && (y - at.1).abs() <= MULTI_CLICK_SLOP
        });
        p.clicks = if again { p.clicks % 3 + 1 } else { 1 };
        p.last_press = Some((now, at));
        p.down = true;
        let (hit, line_end) = self.hit_on_line(ui, at);
        let range = match p.clicks {
            1 if shift => Range { anchor: self.selection.get().anchor, focus: hit },
            1 => Range { anchor: hit, focus: hit },
            _ => self.unit_at(hit, p.clicks),
        };
        p.origin = range;
        self.pointer.set(p);
        self.select_with(ui, range, caret_motion::Held { line_end, goal_x: None });
    }

    pub fn pointer_drag(&self, ui: &NoteWindow, at: (f32, f32)) {
        let p = self.pointer.get();
        if !p.down {
            return;
        }
        let hit = self.hit(ui, at);
        let range = if p.clicks == 1 {
            Range { anchor: p.origin.anchor, focus: hit }
        } else {
            let (first_start, first_end) = p.origin.ordered();
            let (start, end) = self.unit_at(hit, p.clicks).ordered();
            if hit < first_start {
                Range { anchor: first_end, focus: start }
            } else {
                Range { anchor: first_start, focus: if end >= first_end { end } else { first_end } }
            }
        };
        if range != self.selection.get() {
            self.set_selection(ui, range);
        }
    }

    pub fn pointer_up(&self) {
        let mut p = self.pointer.get();
        p.down = false;
        self.pointer.set(p);
    }

    pub fn drag_to(&self, ui: &NoteWindow, at: (f32, f32)) -> bool {
        let view_y = at.1 + ui.get_text_layer_y();
        let mut p = self.pointer.get();
        p.drag = (at.0, view_y);
        self.pointer.set(p);
        self.pointer_drag(ui, at);
        view_y < 0.0 || view_y > ui.get_text_view_height()
    }

    pub fn autoscroll(&self, ui: &NoteWindow) -> bool {
        let p = self.pointer.get();
        let (x, view_y) = p.drag;
        let view = ui.get_text_view_height();
        let over = if view_y < 0.0 { view_y } else if view_y > view { view_y - view } else { 0.0 };
        if !p.down || over == 0.0 {
            return false;
        }
        let step = (over.abs() * 0.25).clamp(3.0, 40.0).copysign(over);
        let max = (ui.get_content_height() - view).max(0.0);
        ui.set_scroll_y((ui.get_scroll_y() + step).clamp(0.0, max));
        self.pointer_drag(ui, (x, view_y.clamp(0.0, view) - ui.get_text_layer_y()));
        true
    }

    pub fn set_right_click(&self, at: Option<(f32, f32)>) {
        self.right_click.set(at);
    }

    pub fn take_right_click(&self) -> Option<(f32, f32)> {
        self.right_click.take()
    }
}
