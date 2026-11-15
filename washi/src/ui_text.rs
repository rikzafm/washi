//! The Settings page's text, turned onto the tilt of its Paper.
//!
//! The software renderer cannot turn text, so a label drawn by a `Text` is level and lands on whole
//! pixels: along a tilted line it steps up and down. `PText` (ui/settings-widgets.slint) therefore asks
//! [`lines`] for its text: the words are laid out here (wrapped, aligned, elided, as the `Text` would),
//! each line is drawn turned by the tilt (`tilt_text`, the notes' engine) and handed back as a picture
//! to show 1:1, tinted with the text's ink. A face this module has no outlines for (the Font manager
//! while a slider drags, a font that failed to parse) gives no lines, and `PText` shows the level text.

use crate::fonts::{self, DEFAULT_TEXT_FAMILY};
use crate::note_window::{SettingsWindow, Turned, TurnedLine, TurnSpec};
use crate::tilt_text::{self, FaceSource, Look, Words};
use slint::{ComponentHandle, ModelRc, VecModel};
use std::cell::RefCell;
use unicode_linebreak::{linebreaks, BreakOpportunity};

/// Where a picture lands against the pixel its origin is in (see `editor::layout::TURNED_NUDGE`).
const NUDGE: f64 = 0.13;

/// Turned lines sit this much higher than the baseline the face's own ascent gives, which is where the
/// mockup's browser puts them (the fidelity test pins it; the level `Text` is a pixel lower than that
/// by itself, hence `PText.nudge`).
const BASELINE_SHIFT: f64 = -1.0;

/// The pictures kept: the same label is asked for again on every frame of an animation.
const MAX_KEPT: usize = 1500;

thread_local! {
    static WORDS: RefCell<Words> = RefCell::new(Words::default());
}

/// Let the Settings window's `PText`s ask for turned lines.
pub fn wire(ui: &SettingsWindow) {
    ui.global::<Turned>().on_lines(|spec| ModelRc::new(VecModel::from(lines(&spec))));
}

/// Forget the pictures (the font changed, or the window was suspended).
pub fn forget() {
    WORDS.with(|w| w.borrow_mut().clear());
}

/// The face `family` names, if there are outlines for it: the font in use, or a font's preview face.
fn source(family: &str) -> Option<std::rc::Rc<dyn FaceSource>> {
    let active = fonts::current();
    if family == active.text_family || family == DEFAULT_TEXT_FAMILY {
        return Some(active);
    }
    fonts::registry::preview_face(family).map(|f| f as std::rc::Rc<dyn FaceSource>)
}

/// One line of text: what it says and how wide it is, px.
struct Line {
    text: String,
    width: f64,
}

struct Measure<'a> {
    face: &'a ttf_parser::Face<'a>,
    k: f64,
    spacing: f64,
}

impl Measure<'_> {
    fn width(&self, s: &str) -> f64 {
        s.chars().map(|c| self.advance(c)).sum()
    }

    fn advance(&self, c: char) -> f64 {
        let g = self.face.glyph_index(c).unwrap_or(ttf_parser::GlyphId(0));
        self.face.glyph_hor_advance(g).unwrap_or(0) as f64 * self.k + self.spacing
    }
}

/// `text` cut into lines no wider than `max` px (`wrap`: 0 never, 1 at the line-break opportunities,
/// 2 anywhere), each with its width. A line that is too wide and may not be cut is left whole.
fn break_lines(text: &str, m: &Measure, max: f64, wrap: i32) -> Vec<Line> {
    let mut out = vec![];
    for para in text.split('\n') {
        if wrap == 0 || max <= 0.0 {
            out.push(Line { text: para.trim_end().to_string(), width: m.width(para.trim_end()) });
            continue;
        }
        // the pieces of the paragraph a line may end after (or, for `wrap == 2`, every character)
        let mut ends: Vec<usize> = if wrap == 2 {
            para.char_indices().map(|(i, c)| i + c.len_utf8()).collect()
        } else {
            linebreaks(para).filter(|(_, o)| *o == BreakOpportunity::Allowed).map(|(i, _)| i).collect()
        };
        if ends.last() != Some(&para.len()) {
            ends.push(para.len());
        }
        let (mut start, mut last_ok) = (0usize, 0usize);
        for &end in &ends {
            let candidate = para[start..end].trim_end();
            if m.width(candidate) > max && last_ok > start {
                let line = para[start..last_ok].trim_end();
                out.push(Line { text: line.to_string(), width: m.width(line) });
                start = last_ok;
            }
            last_ok = end;
        }
        let rest = para[start..].trim_end();
        out.push(Line { text: rest.to_string(), width: m.width(rest) });
    }
    out
}

/// `line` cut to `max` px with an ellipsis, when it is wider.
fn elide(line: Line, m: &Measure, max: f64) -> Line {
    if line.width <= max {
        return line;
    }
    let dots = if m.face.glyph_index('…').is_some() { "…" } else { "..." };
    let room = max - m.width(dots);
    let mut text = String::new();
    let mut w = 0.0;
    for c in line.text.chars() {
        let a = m.advance(c);
        if w + a > room {
            break;
        }
        w += a;
        text.push(c);
    }
    let text = format!("{}{dots}", text.trim_end());
    Line { width: m.width(&text), text }
}

/// The lines of `spec`, drawn turned and placed (in the coordinates its frame is in).
pub fn lines(spec: &TurnSpec) -> Vec<TurnedLine> {
    let Some(font) = source(spec.family.as_str()) else { return vec![] };
    let bold = spec.bold;
    let face = font.face(bold, false);
    let size = spec.size as f64;
    if size <= 0.0 || spec.scale <= 0.0 {
        return vec![];
    }
    let k = size / face.units_per_em() as f64;
    let m = Measure { face, k, spacing: spec.spacing as f64 };
    let (asc, desc) = (face.ascender() as f64 * k, face.descender() as f64 * k);
    let pitch = (asc - desc) * spec.line_factor as f64;
    let baseline = asc + (pitch - (asc - desc)) / 2.0 + BASELINE_SHIFT;
    let width = spec.w as f64;

    let mut rows = break_lines(spec.text.as_str(), &m, width, spec.wrap);
    if spec.wrap == 0 && spec.elide && width > 0.0 {
        rows = rows.into_iter().map(|l| elide(l, &m, width)).collect();
    }
    let block = rows.len() as f64 * pitch;
    let top = match spec.valign {
        1 => (spec.h as f64 - block) / 2.0,
        2 => spec.h as f64 - block,
        _ => 0.0,
    };

    let f = &spec.pf.f;
    let tilt_deg = (f.sn as f64).atan2(f.cs as f64).to_degrees();
    let scale = spec.scale as f64;
    let look = Look { bold, italic: false, size: size * scale, spacing: spec.spacing as f64 * scale, tilt_deg };
    let mut out = vec![];
    WORDS.with(|words| {
        let mut words = words.borrow_mut();
        if words.len() > MAX_KEPT {
            words.clear();
        }
        for (i, row) in rows.iter().enumerate() {
            if row.text.is_empty() {
                continue;
            }
            let dx = match spec.align {
                1 => (width - row.width) / 2.0,
                2 => width - row.width,
                _ => 0.0,
            };
            // the pen's start on the line, in the paper's own axes, then through the tilt
            let (u, v) = (spec.a as f64 + dx - spec.pf.pw as f64 / 2.0, spec.b as f64 + top + i as f64 * pitch + baseline - spec.pf.ph as f64 / 2.0);
            let (sn, cs) = (f.sn as f64, f.cs as f64);
            let origin = ((f.cx as f64 + u * cs - v * sn) * scale, (f.cy as f64 + u * sn + v * cs) * scale);
            if let Some(d) = words.get(&*font, &row.text, &look, tilt_text::phase_of(origin)) {
                let at = tilt_text::pixel_of(origin);
                out.push(TurnedLine {
                    img: d.image,
                    x: (((at.0 + d.left as i64) as f64 + NUDGE) / scale) as f32,
                    y: (((at.1 + d.top as i64) as f64 + NUDGE) / scale) as f32,
                    w: (d.width as f64 / scale) as f32,
                    h: (d.height as f64 / scale) as f32,
                });
            }
        }
    });
    out
}
