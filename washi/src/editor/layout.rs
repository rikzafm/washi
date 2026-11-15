//! Layout: the theme's text tokens, the stepped line layout pushed into the view models, and the caret box.

use super::*;

/// A turned word's picture sits this far (physical px) past the whole pixel it belongs to. The
/// renderer rounds every picture's place after adding what the note is moved by (its margins,
/// the scroll); with the pictures a little off the pixel edge, that rounding goes the same way
/// for all of them and they stay in step with each other.
const TURNED_NUDGE: f64 = 0.13;

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
            font_epoch: crate::fonts::current().epoch,
            check_indent: th.get_check_indent() as f64,
            check_box: crate::fonts::SizeTokens::at(crate::fonts::size()).check_box,
        }
    }

    pub fn default_tokens(&self) -> Tokens {
        let scale = self.scale_factor.get();
        let sz = crate::fonts::SizeTokens::at(crate::fonts::size());
        let style = |size: f64, pitch: f64| Style {
            size,
            pitch: (pitch * scale).round() / scale,
        };
        Tokens {
            inset_left: 32.0,
            inset_right: 40.0,
            inset_top: 38.2,
            heading_gap: sz.heading_gap,
            body_gap: sz.body_gap,
            title: style(sz.title_size, sz.title_pitch),
            heading: style(sz.heading_size, sz.heading_pitch),
            body: style(sz.body_size, sz.body_pitch),
            tilt_deg: 1.45,
            max_run: 28.0,
            letter_spacing: 0.0,
            font_epoch: crate::fonts::current().epoch,
            check_indent: sz.check_indent,
            check_box: sz.check_box,
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
        // The words and bars near the view are also drawn turned onto the tilted baseline
        // (`tilt_text`): a point of each goes through the tilt (a word's pen start on the baseline,
        // a bar's centre), and the picture is put on the whole screen pixel that point falls in.
        let scale = ui.window().scale_factor() as f64;
        let window = self.turned_window(ui);
        let mut words = self.words.borrow_mut();
        words.begin();
        // a bar cut into pieces is one picture, carried by its first piece: (x, y) of the bar that has one
        let mut whole: Option<(f64, f64)> = None;
        let rule_rows = rules
            .iter()
            .map(|r| {
                let mut row = RuleData {
                    block: r.block as i32,
                    x: r.x as f32,
                    y: r.y as f32,
                    dx: r.dx as f32,
                    dy: r.dy as f32,
                    width: r.width as f32,
                    height: r.height as f32,
                    done: r.done,
                    img: Default::default(),
                    img_x: 0.0,
                    img_y: 0.0,
                    img_w: 0.0,
                    img_h: 0.0,
                    joined: false,
                };
                if r.x != r.bar.0 {
                    row.joined = whole == Some((r.bar.0, r.y));
                    return row;
                }
                whole = None;
                if r.y + r.height < window.0 || r.y > window.1 {
                    return row;
                }
                let (x, y) = (r.bar.0 + r.bar.1 / 2.0, r.y + r.height / 2.0);
                let (dx, dy) = textflow::tilt_offset(x, y, layout.paper(), tokens.tilt_deg);
                let origin = ((x + dx) * scale, (y + dy) * scale);
                // (never thinner than a screen pixel, like the level bar)
                if let Some(d) = words.bar(r.bar.1 * scale, (r.height * scale).max(1.0), tokens.tilt_deg, crate::tilt_text::phase_of(origin)) {
                    let at = crate::tilt_text::pixel_of(origin);
                    row.img = d.image;
                    row.img_x = (((at.0 + d.left as i64) as f64 + TURNED_NUDGE) / scale) as f32;
                    row.img_y = (((at.1 + d.top as i64) as f64 + TURNED_NUDGE) / scale) as f32;
                    row.img_w = (d.width as f64 / scale) as f32;
                    row.img_h = (d.height as f64 / scale) as f32;
                    whole = Some((r.bar.0, r.y));
                }
                row
            })
            .collect::<Vec<_>>();
        sync_rows(&self.rules, rule_rows);
        sync_rows(&self.indents, (0..layout.block_count()).map(|b| layout.indent(b) as f32).collect::<Vec<_>>());
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
        let hint_layout = hint.then(|| Layout::build(&[Block::new(Kind::Title, HINT)], self.paper.get(), &tokens, &|_| None));
        let shown: &Layout = hint_layout.as_ref().unwrap_or(&layout);
        if hint {
            runs = shown.runs().to_vec();
        }
        let rows = runs
            .iter()
            .map(|r| {
                let mut row = RunData {
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
                    img: Default::default(),
                    img_x: 0.0,
                    img_y: 0.0,
                    img_w: 0.0,
                    img_h: 0.0,
                };
                if r.y + r.pitch < window.0 || r.y > window.1 {
                    return row;
                }
                let (x, y) = (r.x, r.y + shown.run_baseline(r));
                let (dx, dy) = textflow::tilt_offset(x, y, shown.paper(), tokens.tilt_deg);
                let origin = ((x + dx) * scale, (y + dy) * scale);
                let look = crate::tilt_text::Look { bold: r.bold, italic: r.italic, size: r.size * scale, spacing: shown.run_spacing(r) * scale, tilt_deg: tokens.tilt_deg };
                if let Some(d) = words.get(&**shown.font(), &r.text, &look, crate::tilt_text::phase_of(origin)) {
                    let at = crate::tilt_text::pixel_of(origin);
                    row.img = d.image;
                    row.img_x = (((at.0 + d.left as i64) as f64 + TURNED_NUDGE) / scale) as f32;
                    row.img_y = (((at.1 + d.top as i64) as f64 + TURNED_NUDGE) / scale) as f32;
                    row.img_w = (d.width as f64 / scale) as f32;
                    row.img_h = (d.height as f64 / scale) as f32;
                }
                row
            })
            .collect::<Vec<_>>();
        words.end();
        drop(words);
        sync_rows(&self.runs, rows);
        drop(doc);
    }

    /// The stretch of the layout (top, bottom) whose words are drawn turned: the view and half a
    /// paper above and below it. It stays put while the view is well inside it, so scrolling
    /// redraws words only now and then; [`Self::scrolled`] moves it.
    fn turned_window(&self, ui: &NoteWindow) -> (f64, f64) {
        let (scroll, h) = (ui.get_scroll_y() as f64, self.paper.get().1);
        if !self.turned_window_holds(ui) {
            self.words_window.set((scroll - h * 0.5, scroll + h * 1.5));
        }
        self.words_window.get()
    }

    /// Whether the view (and a tenth of a paper around it) is inside the stretch drawn turned.
    fn turned_window_holds(&self, ui: &NoteWindow) -> bool {
        let (scroll, h) = (ui.get_scroll_y() as f64, self.paper.get().1);
        let (top, bottom) = self.words_window.get();
        scroll - h * 0.1 >= top && scroll + h * 1.1 <= bottom && bottom - top <= h * 2.0 + 1.0
    }

    /// The note scrolled: draw the turned words that came near the view.
    pub fn scrolled(&self, ui: &NoteWindow) {
        if !self.turned_window_holds(ui) {
            self.lay_out(ui);
        }
    }

    /// The note is suspended: forget the glyphs drawn so far (every note shares them, and they are
    /// made again as needed). The words' own pictures stay: taking them from the rows would have the
    /// window draw itself again, and a suspended note must not (it keeps its last frame).
    pub(super) fn free_words(&self) {
        crate::tilt_text::forget_glyphs();
    }

    /// The bytes of the turned words' pictures (tests).
    pub fn turned_word_bytes(&self) -> usize {
        self.words.borrow().bytes()
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
