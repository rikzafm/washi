//! Layout: the theme's text tokens, the stepped line layout pushed into the view models, and the caret box.

use super::*;

impl Editor {
    pub fn tokens(&self, ui: &NoteWindow) -> Tokens {
        let th = ui.global::<Theme>();
        let scale = ui.window().scale_factor() as f64;
        let style = |size: f32, pitch: f32| Style {
            size: size as f64,
            pitch: (pitch as f64 * scale).round() / scale,
        };
        Tokens {
            inset_left: th.get_text_inset_left() as f64,
            inset_right: th.get_text_inset_right() as f64,
            inset_top: th.get_text_inset_top() as f64,
            heading_gap: th.get_heading_gap() as f64,
            body_gap: th.get_body_gap() as f64,
            title: style(th.get_title_size(), th.get_title_pitch()),
            heading: style(th.get_heading_size(), th.get_heading_pitch()),
            body: style(th.get_body_size(), th.get_body_pitch()),
            tilt_deg: th.get_text_tilt() as f64,
            max_run: th.get_step_width() as f64,
            letter_spacing: th.get_letter_spacing_em() as f64,
        }
    }

    pub fn default_tokens(&self) -> Tokens {
        let scale = self.scale_factor.get();
        let style = |size: f64, pitch: f64| Style {
            size,
            pitch: (pitch * scale).round() / scale,
        };
        Tokens {
            inset_left: 32.0,
            inset_right: 40.0,
            inset_top: 38.2,
            heading_gap: 10.0,
            body_gap: 8.1,
            title: style(37.3, 60.0),
            heading: style(24.0, 29.3),
            body: style(22.65, 29.1),
            tilt_deg: 1.45,
            max_run: 28.0,
            letter_spacing: 0.0,
        }
    }

    /// Lay the note out again and show it, caret included.
    pub fn relayout(&self, ui: &NoteWindow) {
        self.lay_out(ui);
        self.show_caret(ui);
    }

    /// [`Self::relayout`] without the caret, for a caller that moves the caret next and shows it itself.
    pub(super) fn lay_out(&self, ui: &NoteWindow) {
        ui.set_pitch_scale(ui.window().scale_factor());
        let tokens = self.tokens(ui);
        let layout = self.layout_with(&tokens);
        let doc = self.doc();
        let mut runs = layout.runs().to_vec();
        let (rules, markers) = (layout.rules(), layout.markers().to_vec());
        ui.set_content_height(layout.content_height() as f32);
        sync_rows(&self.rules, 
            rules
                .iter()
                .map(|r| RuleData {
                    block: r.block as i32,
                    x: r.x as f32,
                    y: r.y as f32,
                    dx: r.dx as f32,
                    dy: r.dy as f32,
                    width: r.width as f32,
                    height: r.height as f32,
                    done: r.done,
                })
                .collect::<Vec<_>>(),
        );
        sync_rows(&self.checks,
            layout
                .checks()
                .iter()
                .map(|c| CheckData {
                    block: c.block as i32,
                    x: c.x as f32,
                    y: c.y as f32,
                    size: c.size as f32,
                    dx: c.dx as f32,
                    dy: c.dy as f32,
                    checked: c.checked,
                })
                .collect::<Vec<_>>(),
        );
        sync_rows(&self.marker_data, 
            markers
                .iter()
                .map(|m| MarkerData {
                    x: m.x as f32,
                    y: m.y as f32,
                    width: m.width as f32,
                    height: m.height as f32,
                    color: marker_color(m.marker),
                })
                .collect::<Vec<_>>(),
        );
        *self.markers.borrow_mut() = markers;
        self.fill_sel_rects(&doc, &layout);
        let hint = doc.is_empty();
        if hint {
            runs = Layout::build(&[Block::new(Kind::Title, HINT)], self.paper.get(), &tokens, &|_| None).runs().to_vec();
        }
        sync_rows(&self.runs, 
            runs.iter()
                .map(|r| RunData {
                    block: if hint { -2 } else { r.block as i32 },
                    text: r.text.as_str().into(),
                    x: r.x as f32,
                    y: r.y as f32,
                    dx: r.dx as f32,
                    dy: r.dy as f32,
                    width: r.width as f32,
                    size: r.size as f32,
                    pitch: r.pitch as f32,
                    bold: r.bold,
                    italic: r.italic,
                    ink: ink_color(r.ink),
                    done: r.done,
                    hint,
                })
                .collect::<Vec<_>>(),
        );
        drop(doc);
    }

    /// Tell the note where to draw the caret: the box of its line in the app's own layout, or
    /// nowhere while something is selected or a picture has the focus.
    pub fn show_caret(&self, ui: &NoteWindow) {
        let sel = self.selection.get();
        let at = if sel.anchor == sel.focus && self.range.get().is_none() {
            let c = sel.focus;
            self.layout(ui)
                .caret_box(c.block, c.offset, self.line_end.get() == Some(c))
                .map(|(x, top, bottom)| (x as f32, top as f32, bottom as f32))
        } else {
            None
        };
        if at == self.shown_caret.get() {
            return;
        }
        self.shown_caret.set(at);
        ui.set_caret_block(if at.is_some() { self.selection.get().focus.block as i32 } else { -1 });
        if let Some((x, top, bottom)) = at {
            ui.set_caret(SelRect { x, y: top, width: 0.0, height: bottom - top });
            ui.set_caret_serial(ui.get_caret_serial().wrapping_add(1));
        }
    }

    pub fn set_paper_size(&self, width: f64, height: f64) {
        self.paper.set((width, height));
    }

    pub fn set_scale_factor(&self, scale: f64) {
        self.scale_factor.set(scale);
    }
}
