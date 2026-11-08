//! One open note: the Slint window plus its 500 ms autosave debounce.

use crate::platform::{Clock, NoteId, ResizeEdge, ScreenRect};
use crate::storage::{Db, Geometry};
use slint::{CloseRequestResponse, ComponentHandle, Timer, TimerMode};
use crate::document::{floor_boundary, Block, Caret, Document, Kind, Paint, Range, Style as Flags, clamp_palette, PALETTE, PALETTE_MAX};
use crate::history::History;
use crate::images::Stored;
use crate::note_images::NoteImages;
use crate::textflow::{self, Style, Tokens};
use slint::{Model, VecModel};
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

slint::include_modules!();

/// Quiet period after the last edit before the note is written to the database.
pub const AUTOSAVE_DELAY: Duration = Duration::from_millis(500);

/// Quiet period after the last move or resize before the position and size are written. Its own
/// timer: typing never delays it, and dragging never delays the text.
pub const GEOMETRY_DELAY: Duration = Duration::from_millis(500);

/// A minimized window reports a position around (-32000, -32000); that is not where the note lives.
fn is_minimized_position(x: i32, y: i32) -> bool {
    x <= -30000 && y <= -30000
}

struct GeometrySave {
    id: NoteId,
    db: Rc<Db>,
    ui: slint::Weak<NoteWindow>,
    /// The last geometry seen while the window was in a normal state.
    last: Cell<Option<Geometry>>,
    dirty: Cell<bool>,
}

impl GeometrySave {
    /// Where the window is and how big, in logical pixels; the last good value while minimized.
    fn current(&self) -> Option<Geometry> {
        let ui = self.ui.upgrade()?;
        let w = ui.window();
        let scale = w.scale_factor();
        let (p, s) = (w.position(), w.size());
        if is_minimized_position(p.x, p.y) {
            return self.last.get();
        }
        let g = Geometry {
            x: (p.x as f32 / scale).round() as i32,
            y: (p.y as f32 / scale).round() as i32,
            w: (s.width as f32 / scale).round() as u32,
            h: (s.height as f32 / scale).round() as u32,
        };
        self.last.set(Some(g));
        Some(g)
    }

    fn flush(&self) {
        if !self.dirty.replace(false) {
            return;
        }
        let Some(g) = self.current() else { return };
        if let Err(e) = self.db.save_geometry(self.id, g) {
            eprintln!("washi: could not save the position of note {}: {e}", self.id);
            self.dirty.set(true);
        }
    }
}

struct Autosave {
    id: NoteId,
    db: Rc<Db>,
    clock: Rc<dyn Clock>,
    ui: slint::Weak<NoteWindow>,
    dirty: Cell<bool>,
}

impl Autosave {
    fn flush(&self) {
        if !self.dirty.replace(false) {
            return;
        }
        let Some(ui) = self.ui.upgrade() else { return };
        if let Err(e) = self.db.save_content(self.id, ui.get_text().as_str(), self.clock.unix_millis()) {
            eprintln!("washi: autosave of note {} failed: {e}", self.id);
            self.dirty.set(true);
        }
    }
}

/// A note's title: its first non-blank text, cut to about 40 characters. `None` for an empty note.
pub fn note_title(content: &str) -> Option<String> {
    const MAX: usize = 40;
    let doc = Document::parse(content);
    let line = doc.blocks().iter().map(|b| b.text.trim()).find(|l| !l.is_empty())?;
    if line.chars().count() <= MAX {
        return Some(line.into());
    }
    let cut: String = line.chars().take(MAX).collect();
    Some(format!("{}…", cut.trim_end()))
}

/// Window title: the note's title, or "Washi" when the note is empty.
pub fn window_title(content: &str) -> String {
    note_title(content).unwrap_or_else(|| "Washi".into())
}

fn app_icon() -> slint::Image {
    const SIZE: u32 = 48;
    let mut buf = slint::SharedPixelBuffer::<slint::Rgba8Pixel>::new(SIZE, SIZE);
    for (dst, src) in buf.make_mut_bytes().chunks_mut(4).zip(crate::icon::rgba(SIZE).chunks(4)) {
        dst.copy_from_slice(src);
    }
    slint::Image::from_rgba8(buf)
}

/// What a note window asks the app to do.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum NoteEvent {
    /// The user closed the window itself (taskbar, Alt+F4).
    WindowClosed,
    /// Right click; `at` is in screen pixels.
    ContextMenu { at: (i32, i32) },
    /// The mouse moved over the note (wakes a suspended app).
    PointerMoved,
    /// The pink tape or top margin was pressed: drag the window.
    StartMove,
    /// A resize zone was pressed.
    StartResize(ResizeEdge),
    New,
    Close,
    Delete,
    /// The toolbar's image button: the app asks for a picture to add.
    InsertImage,
}

const START: Caret = Caret { block: 0, offset: 0 };

/// Two presses count as a double (triple) click when they come this soon after each other...
const MULTI_CLICK: Duration = Duration::from_millis(500);
/// ...and this close (logical px).
const MULTI_CLICK_SLOP: f32 = 4.0;
/// While a drag is held outside the text area, the note scrolls this often.
const AUTOSCROLL_STEP: Duration = Duration::from_millis(40);

/// The pointer's press in progress: what a drag extends, by characters, words or paragraphs.
#[derive(Clone, Copy)]
struct Pointer {
    /// The left button is down.
    down: bool,
    /// 1 click: by characters, 2: by words, 3: by paragraphs.
    clicks: u8,
    /// Where the press started: the anchor of a character drag, the first word or paragraph of the others.
    origin: Range,
    last_press: Option<(Duration, (f32, f32))>,
    /// The last place of a drag: x in the layer, y in the text area (outside it: scrolling).
    drag: (f32, f32),
}

impl Default for Pointer {
    fn default() -> Pointer {
        Pointer { down: false, clicks: 0, origin: Range { anchor: START, focus: START }, last_press: None, drag: (0.0, 0.0) }
    }
}

/// The note's document, shared between the window's model and the stored text.
struct Doc {
    doc: RefCell<Document>,
    model: Rc<VecModel<BlockData>>,
    runs: Rc<VecModel<RunData>>,
    rules: Rc<VecModel<RuleData>>,
    /// The wash of the note-level selection, one level box per line it touches.
    sel_rects: Rc<VecModel<SelRect>>,
    /// The markers behind colored text as last laid out, what they are drawn from, and the boxes of
    /// the wash that meet them.
    markers: RefCell<Vec<textflow::Marker>>,
    marker_data: Rc<VecModel<MarkerData>>,
    marker_wash: Rc<VecModel<SelRect>>,
    /// The note-level range that `sel_rects` washes (a selection across blocks), if any.
    range: Cell<Option<Range>>,
    /// The selection: where it started and where the caret is now (the same caret: nothing
    /// selected). The focus block is what the toolbar and the format shortcuts act on.
    selection: Cell<Range>,
    /// The pointer press in progress (a drag grows the selection), and the clicks before it.
    pointer: Cell<Pointer>,
    /// A format button pressed with nothing selected, outside a word: the style the next text
    /// typed at that caret takes. Gone once the caret moves elsewhere.
    typing: Cell<Option<(Caret, Flags)>>,
    /// A color picked with nothing selected: the paint the next text typed at that caret takes.
    typing_paint: Cell<Option<(Caret, Paint)>>,
    /// The paper's size in logical px (window minus margins), for the stepped layout.
    paper: Cell<(f64, f64)>,
    /// Sizes and decoded pixels of the note's pictures.
    images: NoteImages,
    /// Undo and redo (ticket 12): the snapshots of this note's document.
    history: RefCell<History<Document>>,
}

/// Typing is snapshotted once it pauses this long; structural changes are snapshotted at once.
pub const HISTORY_PAUSE: Duration = Duration::from_millis(500);

/// The keys the block field sends for Ctrl+Z and Ctrl+Y (see ui/block.slint).
const UNDO_KEY: &str = "\u{1a}";
const REDO_KEY: &str = "\u{19}";

fn rgb(c: u32) -> slint::Color {
    slint::Color::from_rgb_u8((c >> 16) as u8, (c >> 8) as u8, c as u8)
}

/// The text color of palette entry `i` (the default ink for 0 and for an index past the table).
pub fn ink_color(i: u8) -> slint::Color {
    rgb(PALETTE.get(i as usize).unwrap_or(&PALETTE[0]).ink)
}

/// The marker color of palette entry `i` (see-through for none).
pub fn marker_color(i: u8) -> slint::Color {
    swatch_marker(PALETTE.get(i as usize).and_then(|s| s.marker))
}

/// A swatch's marker as a color: see-through when it has none.
fn swatch_marker(marker: Option<u32>) -> slint::Color {
    marker.map_or(slint::Color::from_argb_u8(0, 0, 0, 0), rgb)
}

/// The shown text of an empty note.
pub const HINT: &str = "Note Something....";

impl Doc {
    fn new(images: NoteImages) -> Rc<Doc> {
        Rc::new(Doc {
            doc: RefCell::new(Document::new()),
            model: Rc::new(VecModel::default()),
            runs: Rc::new(VecModel::default()),
            rules: Rc::new(VecModel::default()),
            sel_rects: Rc::new(VecModel::default()),
            markers: RefCell::new(vec![]),
            marker_data: Rc::new(VecModel::default()),
            marker_wash: Rc::new(VecModel::default()),
            range: Cell::new(None),
            selection: Cell::new(Range { anchor: START, focus: START }),
            pointer: Cell::new(Pointer::default()),
            typing: Cell::new(None),
            typing_paint: Cell::new(None),
            paper: Cell::new((363.0, 433.0)),
            history: RefCell::new(History::new(Document::new())),
            images,
        })
    }

    /// A block as the window shows it.
    fn block_data(&self, b: &Block) -> BlockData {
        let (img_w, img_h) = self.images.size(b).unwrap_or((0, 0));
        BlockData { img: self.images.pixels(b), img_w: img_w as i32, img_h: img_h as i32, ..block_data(b) }
    }

    /// Replace everything (opening a note, or text set from outside).
    fn load(&self, content: &str) {
        let parsed = Document::parse(content);
        self.images.load(&parsed);
        self.model.set_vec(parsed.blocks().iter().map(|b| self.block_data(b)).collect::<Vec<_>>());
        *self.history.borrow_mut() = History::new(parsed.clone());
        *self.doc.borrow_mut() = parsed;
    }

    /// Snapshot the document as it is now (typing paused, or an undo is about to step back).
    fn checkpoint(&self) {
        self.history.borrow_mut().record(self.doc.borrow().clone());
    }

    /// A structural change just happened to the document that was `before`: snapshot the typing
    /// before it, then the change.
    fn structural(&self, before: Document) {
        let mut h = self.history.borrow_mut();
        h.record(before);
        h.record(self.doc.borrow().clone());
    }

    /// Ctrl+Z (`redo` false) or Ctrl+Y: show the previous or next snapshot. The caret to place,
    /// or `None` when there is no such snapshot.
    fn step(&self, redo: bool) -> Option<Caret> {
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

    fn tokens(ui: &NoteWindow) -> Tokens {
        let th = ui.global::<Theme>();
        // the edit fields step by whole physical pixels: the words must use the same pitch
        let scale = ui.window().scale_factor() as f64;
        let style = |size: f32, pitch: f32| Style { size: size as f64, pitch: (pitch as f64 * scale).round() / scale };
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
        }
    }

    /// Make the window's blocks match the document, touching only the rows that differ so the
    /// fields that stay keep their focus.
    fn sync_model(&self) {
        self.images.load(&self.doc.borrow());
        let new: Vec<BlockData> = self.doc.borrow().blocks().iter().map(|b| self.block_data(b)).collect();
        let old: Vec<BlockData> = self.model.iter().collect();
        let prefix = old.iter().zip(&new).take_while(|(a, b)| same_row(a, b)).count();
        let suffix = old[prefix..].iter().rev().zip(new[prefix..].iter().rev()).take_while(|(a, b)| same_row(a, b)).count();
        let (old_mid, new_mid) = (old.len() - prefix - suffix, new.len() - prefix - suffix);
        for k in 0..old_mid.min(new_mid) {
            self.model.set_row_data(prefix + k, new[prefix + k].clone());
        }
        for _ in new_mid..old_mid {
            self.model.remove(prefix + new_mid);
        }
        for k in old_mid..new_mid {
            self.model.insert(prefix + k, new[prefix + k].clone());
        }
    }

    /// Lay the words out again (text or paper size changed) from the tokens in ui/theme.slint.
    fn relayout(&self, ui: &NoteWindow) {
        ui.set_pitch_scale(ui.window().scale_factor());
        let tokens = Self::tokens(ui);
        let doc = self.doc.borrow();
        let (mut runs, rules, markers) = textflow::layout_all_with_images(doc.blocks(), self.paper.get(), &tokens, &|b| self.images.size(b));
        ui.set_content_height(textflow::extent_with_images(doc.blocks(), self.paper.get().0, &tokens, &|b| self.images.size(b)).1 as f32);
        self.rules.set_vec(
            rules
                .iter()
                .map(|r| RuleData { block: r.block as i32, x: r.x as f32, y: r.y as f32, dx: r.dx as f32, dy: r.dy as f32, width: r.width as f32, height: r.height as f32 })
                .collect::<Vec<_>>(),
        );
        self.marker_data.set_vec(
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
        self.fill_sel_rects(&doc, &tokens);
        let hint = doc.is_empty();
        if hint {
            runs = textflow::layout(&[Block::new(Kind::Title, HINT)], self.paper.get(), &tokens);
        }
        self.runs.set_vec(
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
                    hint,
                })
                .collect::<Vec<_>>(),
        );
    }

    /// Fill the wash model from the note-level range (empty when there is none).
    fn fill_sel_rects(&self, doc: &Document, tokens: &Tokens) {
        let rects = match self.range.get() {
            Some(range) if !doc.is_empty() => textflow::selection_rects(doc.blocks(), range, self.paper.get().0, tokens, &|b| self.images.size(b)),
            _ => vec![],
        };
        let rect = |r: &textflow::Rect| SelRect { x: r.x as f32, y: r.y as f32, width: r.width as f32, height: r.height as f32 };
        self.sel_rects.set_vec(rects.iter().map(rect).collect::<Vec<_>>());
        self.marker_wash.set_vec(textflow::marker_wash(&self.markers.borrow(), &rects).iter().map(rect).collect::<Vec<_>>());
    }

    /// Ctrl+A: the selection becomes the range from the start of the note to its end.
    fn select_all(&self, ui: &NoteWindow) {
        let all = self.doc.borrow().all();
        self.set_selection(ui, all);
    }

    /// A key moved the caret out of a selection across blocks (or the text was replaced): the note's
    /// wash is over and the fields draw selections again.
    fn end_range(&self, ui: &NoteWindow) {
        self.range.set(None);
        self.sel_rects.set_vec(vec![]);
        self.marker_wash.set_vec(vec![]);
        ui.set_wash(true);
    }

    /// Make `range` the selection. Inside one block (or collapsed) the block's own field holds and
    /// draws it; across blocks the note draws the wash (`sel_rects`) and the field with the caret
    /// shows none.
    fn set_selection(&self, ui: &NoteWindow, range: Range) {
        let range = self.clamp(range);
        let (start, end) = range.ordered();
        let across = start.block != end.block;
        self.selection.set(range);
        ui.set_wash(!across);
        if across {
            self.range.set(Some(range));
            self.fill_sel_rects(&self.doc.borrow(), &Self::tokens(ui));
            // the caret is in the field of `focus`, with nothing selected there
            self.focus_with(ui, range.focus, None);
        } else {
            self.range.set(None);
            self.sel_rects.set_vec(vec![]);
            self.marker_wash.set_vec(vec![]);
            self.focus_with(ui, range.focus, Some(range.anchor.offset));
        }
        self.typing.set(None);
        self.typing_paint.set(None);
        self.show_formats(ui);
    }

    /// `range` with both ends inside the document, on character boundaries.
    fn clamp(&self, range: Range) -> Range {
        let doc = self.doc.borrow();
        let fix = |c: Caret| {
            let block = c.block.min(doc.len().saturating_sub(1));
            let text = doc.blocks().get(block).map_or("", |b| b.text.as_str());
            Caret { block, offset: floor_boundary(text, c.offset) }
        };
        Range { anchor: fix(range.anchor), focus: fix(range.focus) }
    }

    /// A press at `at` (logical px in the text layer, level): `shift` extends the selection, a second
    /// and third press in a row select the word and the paragraph.
    fn pointer_down(&self, ui: &NoteWindow, at: (f32, f32), shift: bool, now: Duration) {
        let mut p = self.pointer.get();
        let again = p.last_press.is_some_and(|(t, (x, y))| {
            now.saturating_sub(t) <= MULTI_CLICK && (x - at.0).abs() <= MULTI_CLICK_SLOP && (y - at.1).abs() <= MULTI_CLICK_SLOP
        });
        p.clicks = if again { p.clicks % 3 + 1 } else { 1 };
        p.last_press = Some((now, at));
        p.down = true;
        let hit = self.hit(ui, at);
        let range = match p.clicks {
            1 if shift => Range { anchor: self.selection.get().anchor, focus: hit },
            1 => Range { anchor: hit, focus: hit },
            _ => self.unit_at(hit, p.clicks),
        };
        p.origin = range;
        self.pointer.set(p);
        self.set_selection(ui, range);
    }

    /// The pointer moved to `at` with the left button down: grow the selection.
    fn pointer_drag(&self, ui: &NoteWindow, at: (f32, f32)) {
        let p = self.pointer.get();
        if !p.down {
            return;
        }
        let hit = self.hit(ui, at);
        let range = if p.clicks == 1 {
            Range { anchor: p.origin.anchor, focus: hit }
        } else {
            // by whole words or paragraphs: the unit under the pointer joins the one pressed first
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

    fn pointer_up(&self) {
        let mut p = self.pointer.get();
        p.down = false;
        self.pointer.set(p);
    }

    /// The pointer moved to `at` (layer frame) with the button down: grow the selection, and say
    /// whether the pointer is outside the text area (the note is to scroll while it stays there).
    fn drag_to(&self, ui: &NoteWindow, at: (f32, f32)) -> bool {
        let view_y = at.1 + ui.get_text_layer_y();
        let mut p = self.pointer.get();
        p.drag = (at.0, view_y);
        self.pointer.set(p);
        self.pointer_drag(ui, at);
        view_y < 0.0 || view_y > ui.get_text_view_height()
    }

    /// One step of the scroll that goes on while a drag is held outside the text area: the note
    /// scrolls, and the range reaches the line at the area's edge. False when it is over.
    fn autoscroll(&self, ui: &NoteWindow) -> bool {
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

    /// Shift+`key` (`shift+<arrow>` or `ctrl+shift+<Home or End>`) from the focus end of the
    /// selection: that end moves, over a paragraph's edge if need be. False when the field keeps
    /// the key (inside one paragraph, with nothing across paragraphs).
    fn extend(&self, ui: &NoteWindow, key: &str) -> bool {
        use slint::platform::Key;
        let sel = self.selection.get();
        let across = sel.anchor.block != sel.focus.block;
        let focus = sel.focus;
        let doc = self.doc.borrow();
        let Some(block) = doc.blocks().get(focus.block) else { return false };
        let offset = focus.offset;
        let (key, ctrl) = match key.strip_prefix("ctrl+shift+") {
            Some(k) => (k, true),
            None => (key.strip_prefix("shift+").unwrap_or(key), false),
        };
        let is = |k: Key| key == char::from(k).to_string();
        let to = if ctrl {
            let all = doc.all();
            if is(Key::Home) { all.anchor } else { all.focus }
        } else if is(Key::LeftArrow) {
            if offset > 0 {
                if !across {
                    return false;
                }
                Caret { block: focus.block, offset: floor_boundary(&block.text, offset - 1) }
            } else if focus.block > 0 {
                let prev = focus.block - 1;
                Caret { block: prev, offset: doc.blocks()[prev].text.len() }
            } else {
                focus
            }
        } else if is(Key::RightArrow) {
            if offset < block.text.len() {
                if !across {
                    return false;
                }
                let mut o = offset + 1;
                while !block.text.is_char_boundary(o) {
                    o += 1;
                }
                Caret { block: focus.block, offset: o }
            } else if focus.block + 1 < doc.len() {
                Caret { block: focus.block + 1, offset: 0 }
            } else {
                focus
            }
        } else {
            let up = is(Key::UpArrow);
            let tokens = Self::tokens(ui);
            let width = self.paper.get().0;
            let at = textflow::caret_line(block, offset, width, &tokens);
            let edge = if up { at.line == 0 } else { at.line + 1 >= at.lines };
            if !edge {
                if !across {
                    return false;
                }
                let line = if up { at.line - 1 } else { at.line + 1 };
                Caret { block: focus.block, offset: textflow::offset_at(block, line, at.x, width, &tokens) }
            } else {
                let target = if up { doc.prev_text_block(focus.block) } else { doc.next_text_block(focus.block) };
                match target {
                    Some(t) => Caret { block: t, offset: textflow::offset_at(&doc.blocks()[t], if up { usize::MAX } else { 0 }, at.x, width, &tokens) },
                    // the first or last line of the note: the focus runs to its end
                    None => Caret { block: focus.block, offset: if up { 0 } else { block.text.len() } },
                }
            }
        };
        drop(doc);
        self.set_selection(ui, Range { anchor: sel.anchor, focus: to });
        true
    }

    fn hit(&self, ui: &NoteWindow, at: (f32, f32)) -> Caret {
        let doc = self.doc.borrow();
        textflow::hit(doc.blocks(), (at.0 as f64, at.1 as f64), self.paper.get().0, &Self::tokens(ui), &|b| self.images.size(b))
    }

    /// The word (`clicks` 2) or paragraph (3) holding `at`, as a range. Not on a word: just the caret.
    fn unit_at(&self, at: Caret, clicks: u8) -> Range {
        let doc = self.doc.borrow();
        let Some(b) = doc.blocks().get(at.block) else { return Range { anchor: at, focus: at } };
        let (start, end) = if clicks >= 3 {
            (0, b.text.len())
        } else {
            doc.word_at(at.block, at.offset).unwrap_or((at.offset, at.offset))
        };
        Range { anchor: Caret { block: at.block, offset: start }, focus: Caret { block: at.block, offset: end } }
    }

    fn text(&self) -> String {
        self.doc.borrow().to_text()
    }

    /// What a key does over a selection across blocks (its text): 1 taken without a change (a
    /// modifier key alone, or an arrow without Shift, which collapses the selection to its start or
    /// end); 2 Backspace, Delete, Enter or a character replaced the selection (the caret to place is
    /// returned too); 0 any other key (Shift+arrows are `extend`'s).
    fn range_key(&self, ui: &NoteWindow, key: &str, shift: bool) -> (i32, Option<Caret>) {
        use slint::platform::Key;
        let mut chars = key.chars();
        let (Some(c), None) = (chars.next(), chars.next()) else { return (0, None) };
        // Shift, Control, Alt, AltGr, CapsLock, Meta and their right-hand twins
        if ('\u{10}'..='\u{18}').contains(&c) {
            return (1, None);
        }
        let is = |k: Key| c == char::from(k);
        let (start, end) = self.selection.get().ordered();
        if is(Key::LeftArrow) || is(Key::UpArrow) || is(Key::RightArrow) || is(Key::DownArrow) {
            if shift {
                return (0, None);
            }
            let to = if is(Key::LeftArrow) || is(Key::UpArrow) { start } else { end };
            self.set_selection(ui, Range { anchor: to, focus: to });
            return (1, None);
        }
        let text = if is(Key::Backspace) || is(Key::Delete) {
            String::new()
        } else if is(Key::Return) {
            "\n".to_string()
        } else if !c.is_control() && !('\u{f700}'..='\u{f8ff}').contains(&c) {
            c.to_string()
        } else {
            return (0, None);
        };
        (2, Some(self.replace_range(&text)))
    }

    /// The selection becomes `text`; the caret after it.
    fn replace_range(&self, text: &str) -> Caret {
        let range = self.clamp(self.selection.get());
        let before = self.doc.borrow().clone();
        let caret = self.doc.borrow_mut().replace_range(range, text);
        self.typing.set(None);
        self.typing_paint.set(None);
        self.structural(before);
        caret
    }

    /// The selected text as plain lines.
    fn range_text(&self) -> String {
        self.doc.borrow().plain_range(self.clamp(self.selection.get()))
    }

    /// The caret's block, kept within the document.
    fn caret_block(&self) -> usize {
        self.selection.get().focus.block.min(self.doc.borrow().len().saturating_sub(1))
    }

    /// Put the caret at `c` without asking any field to take it (the field is about to report it).
    fn set_caret(&self, c: Caret) {
        self.selection.set(Range { anchor: c, focus: c });
    }

    /// The style the next text typed at the caret takes: one a format button asked for there, or
    /// that of the text before it.
    fn typing_style(&self) -> Flags {
        let i = self.caret_block();
        let cursor = self.selection.get().focus.offset;
        match self.typing.get() {
            Some((c, style)) if c == (Caret { block: i, offset: cursor }) => style,
            _ => self.doc.borrow().blocks().get(i).map_or(Flags::NONE, |b| b.style_for_insert(cursor)),
        }
    }

    /// The paint the next text typed at the caret takes: one a picked color asked for there, or
    /// that of the text before it.
    fn paint_at_caret(&self) -> Paint {
        let i = self.caret_block();
        let cursor = self.selection.get().focus.offset;
        match self.typing_paint.get() {
            Some((c, paint)) if c == (Caret { block: i, offset: cursor }) => paint,
            _ => self.doc.borrow().blocks().get(i).map_or(Paint::default(), |b| b.paint_for_insert(cursor)),
        }
    }

    /// A swatch picked (or Clear, which picks 0 for both): set the ink and/or the marker of the
    /// selection as one undo step; with a collapsed caret, of the text typed next. True when the
    /// document changed.
    fn apply_paint(&self, ink: Option<u8>, marker: Option<u8>) -> bool {
        let sel = self.clamp(self.selection.get());
        if sel.anchor == sel.focus {
            let mut paint = self.paint_at_caret();
            paint.ink = clamp_palette(ink.unwrap_or(paint.ink));
            paint.marker = clamp_palette(marker.unwrap_or(paint.marker));
            self.typing_paint.set(Some((sel.focus, paint)));
            return false;
        }
        let before = self.doc.borrow().clone();
        let done = {
            let mut doc = self.doc.borrow_mut();
            let a = ink.is_some_and(|i| doc.set_ink_range(sel, i));
            let b = marker.is_some_and(|m| doc.set_marker_range(sel, m));
            a | b
        };
        if done {
            self.structural(before);
        }
        done
    }

    /// Toolbar button or shortcut `what` (0 bold, 1 italic, 2 underline, 3 strike) on the selected
    /// text: the selection (across paragraphs too), else the word holding the caret; outside a
    /// word, on what is typed next. 4 (list) toggles every paragraph the selection touches. True
    /// when the document changed.
    fn format(&self, what: i32) -> bool {
        let i = self.caret_block();
        let flag = match what {
            0 => Flags::BOLD,
            1 => Flags::ITALIC,
            2 => Flags::UNDERLINE,
            3 => Flags::STRIKE,
            4 => {
                let before = self.doc.borrow().clone();
                let done = self.doc.borrow_mut().toggle_list_range(self.selection.get());
                if done {
                    self.structural(before);
                }
                return done;
            }
            _ => return false,
        };
        let sel = self.selection.get();
        let (cursor, collapsed) = (sel.focus.offset, sel.anchor == sel.focus);
        let before = self.doc.borrow().clone();
        let done = {
            let mut doc = self.doc.borrow_mut();
            if !collapsed {
                doc.toggle_style_range(sel, flag)
            } else if let Some((start, end)) = doc.word_at(i, cursor) {
                doc.toggle_style(i, start, end, flag)
            } else {
                false
            }
        };
        if done {
            self.typing.set(None);
            self.structural(before);
        } else if collapsed {
            let style = self.typing_style().toggled(flag);
            self.typing.set(Some((Caret { block: i, offset: cursor }, style)));
        }
        done
    }

    /// Show the formats of the selection (or of what is typed at the caret) on the toolbar buttons.
    fn show_formats(&self, ui: &NoteWindow) {
        let style = {
            let doc = self.doc.borrow();
            let sel = self.selection.get();
            match doc.blocks().get(self.caret_block()) {
                Some(_) if sel.anchor != sel.focus => doc.common_style_range(sel),
                Some(_) => {
                    drop(doc);
                    self.typing_style()
                }
                None => Flags::NONE,
            }
        };
        let doc = self.doc.borrow();
        let b = doc.blocks().get(self.caret_block());
        let heading = b.is_some_and(|b| b.kind == Kind::Heading);
        ui.set_fmt_bold(heading || style.contains(Flags::BOLD));
        ui.set_fmt_italic(style.contains(Flags::ITALIC));
        ui.set_fmt_underline(style.contains(Flags::UNDERLINE));
        ui.set_fmt_strike(style.contains(Flags::STRIKE));
        ui.set_fmt_list(b.is_some_and(|b| b.kind == Kind::ListItem));
        let sel = self.selection.get();
        let (ink, marker) = if b.is_none() {
            (Some(0), Some(0))
        } else if sel.anchor != sel.focus {
            (doc.common_ink_range(sel), doc.common_marker_range(sel))
        } else {
            drop(doc);
            let p = self.paint_at_caret();
            (Some(p.ink), Some(p.marker))
        };
        ui.set_fmt_ink(ink.map_or(-1, i32::from));
        ui.set_fmt_marker(marker.map_or(-1, i32::from));
    }

    /// The caret moved to `offset` of block `block`, the selection's other end at `anchor`. A style
    /// asked for at another place is forgotten.
    fn caret_moved(&self, ui: &NoteWindow, block: usize, offset: usize, anchor: usize) {
        let now = self.selection.get();
        let (start, end) = now.ordered();
        // the field reporting the caret that `set_selection` put in it, under a wash across blocks
        if start.block != end.block && now.focus == (Caret { block, offset }) && anchor == offset {
            return;
        }
        self.selection.set(Range { anchor: Caret { block, offset: anchor }, focus: Caret { block, offset } });
        if self.typing.get().is_some_and(|(c, _)| c != (Caret { block, offset }) || offset != anchor) {
            self.typing.set(None);
        }
        if self.typing_paint.get().is_some_and(|(c, _)| c != (Caret { block, offset }) || offset != anchor) {
            self.typing_paint.set(None);
        }
        if self.range.get().is_some() {
            // the caret left a selection across blocks (a key): the wash is over
            self.end_range(ui);
        }
    }

    /// Ask the block at `c` to take the keyboard focus with the caret there, and scroll it into view.
    fn focus_at(&self, ui: &NoteWindow, c: Caret) {
        self.focus_with(ui, c, None);
    }

    /// As `focus_at`, with the field's selection running from `anchor` (a byte offset) to the caret.
    fn focus_with(&self, ui: &NoteWindow, c: Caret, anchor: Option<usize>) {
        ui.set_focus_block(c.block as i32);
        ui.set_focus_anchor(anchor.map_or(-1, |a| a as i32));
        ui.set_focus_offset(c.offset as i32);
        ui.set_focus_serial(ui.get_focus_serial() + 1);
        self.reveal(ui, c.block, c.offset);
    }

    /// Scroll the least that brings the caret's line into the text area (the top of the note when
    /// the caret is on its first line).
    fn reveal(&self, ui: &NoteWindow, block: usize, offset: usize) {
        let tokens = Self::tokens(ui);
        let doc = self.doc.borrow();
        let Some((top, bottom)) = textflow::caret_span_with_images(doc.blocks(), block, offset, self.paper.get().0, &tokens, &|b| self.images.size(b)) else { return };
        let (_, content) = textflow::extent_with_images(doc.blocks(), self.paper.get().0, &tokens, &|b| self.images.size(b));
        let view = ui.get_text_view_height() as f64;
        let cur = ui.get_scroll_y() as f64;
        let mut to = if top <= tokens.inset_top + 0.5 { 0.0 } else if top < cur { top } else if bottom > cur + view { bottom - view } else { cur };
        to = to.min((content - view).max(0.0)).max(0.0);
        if (to - cur).abs() > 0.01 {
            ui.set_scroll_y(to as f32);
        }
    }

    /// Block `i` was edited to `text`, the caret now at `cursor`. Line breaks in it (a paste) make
    /// new blocks; then the caret to move to is returned.
    fn edit(&self, i: usize, text: &str, cursor: usize) -> Option<Caret> {
        if !text.contains(['\n', '\r']) {
            let before = self.doc.borrow().clone();
            let mut doc = self.doc.borrow_mut();
            let old = doc.blocks().get(i).map(|b| b.text.clone()).unwrap_or_default();
            // a style asked for with a format button applies to what is typed at its caret
            let typing = self.typing.take().filter(|(c, _)| c.block == i).map(|(_, s)| s);
            let paint = self.typing_paint.take().filter(|(c, _)| c.block == i).map(|(_, p)| p);
            doc.edit_text(i, text, cursor, typing, paint);
            // `# ` or `- ` typed at the start of a paragraph
            let caret = doc.auto_format(i, &old, cursor);
            drop(doc);
            if caret.is_some() {
                self.structural(before);
            }
            return caret;
        }
        let before = self.doc.borrow().clone();
        let mut doc = self.doc.borrow_mut();
        let old = match doc.blocks().get(i) {
            Some(b) if b.is_text() => b.text.clone(),
            _ => return None,
        };
        // the field now holds old[..start] + inserted + old[end..], the caret right after `inserted`
        let mut cursor = cursor.min(text.len());
        while !text.is_char_boundary(cursor) {
            cursor -= 1;
        }
        let mut end = old.len() - (text.len() - cursor).min(old.len());
        while !old.is_char_boundary(end) {
            end += 1;
        }
        let mut start = old.bytes().zip(text.bytes()).take_while(|(a, b)| a == b).count().min(end).min(cursor);
        while !old.is_char_boundary(start) || !text.is_char_boundary(start) {
            start -= 1;
        }
        let caret = doc.insert_text(i, start, end, &text[start..cursor]);
        drop(doc);
        self.structural(before);
        Some(caret)
    }

    /// A key while picture block `i` is selected (it holds the keyboard focus): Backspace or Delete
    /// removes it, the arrows move the caret to the text around it.
    fn image_key(&self, i: usize, key: &str, before: Document) -> Option<(Caret, bool)> {
        use slint::platform::Key;
        let is = |k: Key| key == char::from(k).to_string();
        if is(Key::Backspace) || is(Key::Delete) {
            let range = Range { anchor: Caret { block: i, offset: 0 }, focus: Caret { block: i + 1, offset: 0 } };
            let caret = self.doc.borrow_mut().replace_range(range, "");
            self.structural(before);
            return Some((caret, true));
        }
        let doc = self.doc.borrow();
        if is(Key::UpArrow) || is(Key::LeftArrow) {
            let p = doc.prev_text_block(i)?;
            Some((Caret { block: p, offset: doc.blocks()[p].text.len() }, false))
        } else if is(Key::DownArrow) || is(Key::RightArrow) {
            Some((Caret { block: doc.next_text_block(i)?, offset: 0 }, false))
        } else {
            None
        }
    }

    /// Enter, Backspace, Delete, Up or Down in block `i` at `offset`. `Some((caret, changed))` when
    /// the note takes the key (the document changed, or only the caret moves to another block);
    /// `None` when the field keeps it.
    fn key(&self, i: usize, key: &str, offset: usize, ui: &NoteWindow) -> Option<(Caret, bool)> {
        use slint::platform::Key;
        let before = self.doc.borrow().clone();
        if before.blocks().get(i).is_some_and(|b| b.kind == Kind::Image) {
            return self.image_key(i, key, before);
        }
        // what is typed after Enter goes on in the style of the text before the caret
        let carried = if self.typing.get().is_some_and(|(c, _)| c == (Caret { block: i, offset })) {
            self.typing.get().map(|(_, s)| s)
        } else {
            before.blocks().get(i).map(|b| b.style_for_insert(offset))
        };
        let mut doc = self.doc.borrow_mut();
        let is = |k: Key| key == char::from(k).to_string();
        let edit = if is(Key::Return) {
            let c = doc.split(i, offset);
            self.typing.set(carried.filter(|&s| s != Flags::NONE).map(|s| (c, s)));
            Some(c)
        } else if is(Key::Backspace) {
            doc.backspace(i, offset)
        } else if is(Key::Delete) {
            doc.delete(i, offset)
        } else {
            None
        };
        if let Some(c) = edit {
            drop(doc);
            self.structural(before);
            return Some((c, true));
        }
        if is(Key::Return) || is(Key::Backspace) || is(Key::Delete) {
            return None;
        }
        let up = is(Key::UpArrow);
        if !up && !is(Key::DownArrow) {
            return None;
        }
        let tokens = Self::tokens(ui);
        let width = self.paper.get().0;
        let at = textflow::caret_line(doc.blocks().get(i)?, offset, width, &tokens);
        let target = if up {
            if at.line > 0 {
                return None;
            }
            doc.prev_text_block(i)?
        } else {
            if at.line + 1 < at.lines {
                return None;
            }
            doc.next_text_block(i)?
        };
        let line = if up { usize::MAX } else { 0 };
        let to = textflow::offset_at(&doc.blocks()[target], line, at.x, width, &tokens);
        Some((Caret { block: target, offset: to }, false))
    }
}

/// Whether two rows of the block model are the same. (`BlockData`'s own equality is no use: Slint
/// never calls two empty images equal, so no row with one would ever match.)
fn same_row(a: &BlockData, b: &BlockData) -> bool {
    a.kind == b.kind
        && a.text == b.text
        && a.bold == b.bold
        && (a.img_w, a.img_h) == (b.img_w, b.img_h)
        && a.img.size() == b.img.size()
        && (a.img.size().width == 0 || a.img == b.img)
}

fn block_data(b: &Block) -> BlockData {
    let kind = match b.kind {
        Kind::Title => BlockKind::Title,
        Kind::Heading => BlockKind::Heading,
        Kind::Body => BlockKind::Body,
        Kind::ListItem => BlockKind::ListItem,
        Kind::Image => BlockKind::Image,
    };
    // the words are drawn by the note; the field's face only matters while an input method composes
    BlockData { kind, text: b.text.as_str().into(), bold: b.kind == Kind::Heading, italic: false, img: Default::default(), img_w: 0, img_h: 0 }
}

pub struct Note {
    id: NoteId,
    ui: NoteWindow,
    doc: Rc<Doc>,
    autosave: Rc<Autosave>,
    timer: Rc<Timer>,
    /// Snapshots typing once it pauses (stopped while suspended; the next undo snapshots anyway).
    history_timer: Rc<Timer>,
    geometry: Rc<GeometrySave>,
    geometry_timer: Rc<Timer>,
    /// Everything that changes the text ends here (see `open`).
    changed: Rc<dyn Fn(Option<Caret>)>,
}

impl Note {
    /// Create and show the window. `on_event` runs when the user acts on it.
    pub fn open(
        id: NoteId,
        content: &str,
        db: Rc<Db>,
        clock: Rc<dyn Clock>,
        geometry: Option<Geometry>,
        always_on_top: bool,
        on_event: Rc<dyn Fn(NoteId, NoteEvent)>,
        paste_image: Rc<dyn Fn(NoteId) -> bool>,
    ) -> Result<Note, slint::PlatformError> {
        let ui = NoteWindow::new()?;
        // the popover draws the same table the renderer and the storage read
        let entries: Vec<PaletteEntry> = PALETTE
            .iter()
            .map(|s| PaletteEntry {
                name: s.name.into(),
                ink: rgb(s.ink),
                marker: swatch_marker(s.marker),
            })
            .collect();
        ui.global::<PickerPalette>().set_entries(slint::ModelRc::from(Rc::new(VecModel::from(entries))));
        let doc = Doc::new(NoteImages::new(id, db.clone()));
        doc.load(content);
        ui.set_blocks(slint::ModelRc::from(doc.model.clone()));
        ui.set_runs(slint::ModelRc::from(doc.runs.clone()));
        ui.set_rules(slint::ModelRc::from(doc.rules.clone()));
        ui.set_sel_rects(slint::ModelRc::from(doc.sel_rects.clone()));
        ui.set_markers(slint::ModelRc::from(doc.marker_data.clone()));
        ui.set_marker_wash(slint::ModelRc::from(doc.marker_wash.clone()));
        doc.relayout(&ui);
        doc.show_formats(&ui);
        ui.set_text(doc.text().as_str().into());
        ui.set_window_title(window_title(content).into());
        ui.set_pinned(always_on_top);
        ui.set_app_icon(app_icon());
        if let Some(g) = geometry {
            ui.window().set_size(slint::LogicalSize::new(g.w as f32, g.h as f32));
            ui.window().set_position(slint::LogicalPosition::new(g.x as f32, g.y as f32));
        }
        let autosave = Rc::new(Autosave { id, db, clock, ui: ui.as_weak(), dirty: Cell::new(false) });
        let timer = Rc::new(Timer::default());
        let history_timer = Rc::new(Timer::default());
        let autoscroll = Rc::new(Timer::default());
        // everything that changes the text ends here: relayout, stored text, title, autosave
        let changed: Rc<dyn Fn(Option<Caret>)> = {
            let (autosave, timer, weak, doc) = (autosave.clone(), timer.clone(), ui.as_weak(), doc.clone());
            Rc::new(move |focus: Option<Caret>| {
                let text = doc.text();
                if let Some(ui) = weak.upgrade() {
                    doc.sync_model();
                    doc.relayout(&ui);
                    ui.set_text(text.as_str().into());
                    ui.set_window_title(window_title(&text).into());
                    if let Some(c) = focus {
                        doc.set_caret(c);
                        doc.end_range(&ui);
                    }
                    doc.show_formats(&ui);
                    if let Some(c) = focus {
                        doc.focus_at(&ui, c);
                    }
                }
                autosave.dirty.set(true);
                let a = autosave.clone();
                timer.start(TimerMode::SingleShot, AUTOSAVE_DELAY, move || a.flush());
            })
        };
        {
            let (changed, doc, history_timer) = (changed.clone(), doc.clone(), history_timer.clone());
            ui.on_block_edited(move |i, block_text, cursor| {
                let caret = doc.edit(i.max(0) as usize, block_text.as_str(), cursor.max(0) as usize);
                if caret.is_none() {
                    // typing: snapshot when it pauses
                    let doc = doc.clone();
                    history_timer.start(TimerMode::SingleShot, HISTORY_PAUSE, move || doc.checkpoint());
                }
                changed(caret);
            });
        }
        {
            let (changed, doc) = (changed.clone(), doc.clone());
            let weak = ui.as_weak();
            ui.on_format(move |what| {
                let Some(ui) = weak.upgrade() else { return };
                if doc.format(what) {
                    changed(None);
                } else {
                    // a style for what is typed next
                    doc.show_formats(&ui);
                }
            });
        }
        // a swatch or Clear in the color popover
        let apply: Rc<dyn Fn(Option<u8>, Option<u8>)> = {
            let (changed, doc, weak) = (changed.clone(), doc.clone(), ui.as_weak());
            Rc::new(move |ink, marker| {
                let Some(ui) = weak.upgrade() else { return };
                if doc.apply_paint(ink, marker) {
                    changed(None);
                } else {
                    // a color for what is typed next
                    doc.show_formats(&ui);
                }
            })
        };
        {
            let apply = apply.clone();
            ui.on_pick_ink(move |i| apply(Some(i.clamp(0, PALETTE_MAX as i32) as u8), None));
        }
        {
            let apply = apply.clone();
            ui.on_pick_marker(move |i| apply(None, Some(i.clamp(0, PALETTE_MAX as i32) as u8)));
        }
        ui.on_clear_color(move || apply(Some(0), Some(0)));
        {
            let (doc, weak) = (doc.clone(), ui.as_weak());
            ui.on_focus_entered(move |i| {
                // the field reports its caret and selection right after this
                let block = i.max(0) as usize;
                if doc.selection.get().focus.block != block {
                    doc.set_caret(Caret { block, offset: 0 });
                }
                if let Some(ui) = weak.upgrade() {
                    doc.show_formats(&ui);
                }
            });
        }
        {
            let (changed, doc, weak) = (changed.clone(), doc.clone(), ui.as_weak());
            ui.on_block_key(move |i, key, offset| {
                let Some(ui) = weak.upgrade() else { return false };
                if key == UNDO_KEY || key == REDO_KEY {
                    // taken even with nothing to step to: the field has no history of its own
                    if let Some(caret) = doc.step(key == REDO_KEY) {
                        changed(Some(caret));
                    }
                    return true;
                }
                if key.contains("shift+") {
                    return doc.extend(&ui, key.as_str());
                }
                let Some((caret, edited)) = doc.key(i.max(0) as usize, key.as_str(), offset.max(0) as usize, &ui) else { return false };
                if edited {
                    changed(Some(caret));
                } else {
                    // only the caret moves, to another block
                    doc.focus_at(&ui, caret);
                }
                true
            });
        }
        {
            let (doc, weak) = (doc.clone(), ui.as_weak());
            ui.on_select_all(move || {
                if let Some(ui) = weak.upgrade() {
                    doc.select_all(&ui);
                }
            });
        }
        {
            let doc = doc.clone();
            ui.on_range_text(move || doc.range_text().into());
        }
        {
            let (changed, doc) = (changed.clone(), doc.clone());
            ui.on_range_replace(move |text| {
                let caret = doc.replace_range(text.as_str());
                changed(Some(caret));
            });
        }
        {
            let (changed, doc, weak) = (changed.clone(), doc.clone(), ui.as_weak());
            ui.on_range_key(move |key, shift| {
                let Some(ui) = weak.upgrade() else { return 0 };
                let (taken, caret) = doc.range_key(&ui, key.as_str(), shift);
                if let Some(c) = caret {
                    changed(Some(c));
                }
                taken
            });
        }
        {
            let (doc, weak) = (doc.clone(), ui.as_weak());
            ui.on_caret_moved(move |block, offset, anchor| {
                let (block, offset) = (block.max(0) as usize, offset.max(0) as usize);
                if let Some(ui) = weak.upgrade() {
                    doc.caret_moved(&ui, block, offset, anchor.max(0) as usize);
                    doc.show_formats(&ui);
                    doc.reveal(&ui, block, offset);
                }
            });
        }
        {
            let (weak, doc) = (ui.as_weak(), doc.clone());
            ui.on_paper_resized(move |w, h| {
                doc.paper.set((w as f64, h as f64));
                if let Some(ui) = weak.upgrade() {
                    doc.relayout(&ui);
                }
            });
        }
        {
            let on_event = on_event.clone();
            ui.window().on_close_requested(move || {
                on_event(id, NoteEvent::WindowClosed);
                CloseRequestResponse::HideWindow
            });
        }
        {
            let (on_event, weak) = (on_event.clone(), ui.as_weak());
            ui.on_context_menu(move |x, y| {
                let Some(ui) = weak.upgrade() else { return };
                let w = ui.window();
                let scale = w.scale_factor();
                let origin = w.position();
                let at = (origin.x + (x * scale) as i32, origin.y + (y * scale) as i32);
                on_event(id, NoteEvent::ContextMenu { at });
            });
        }
        {
            let (doc, weak, clock) = (doc.clone(), ui.as_weak(), autosave.clock.clone());
            ui.on_pointer_down(move |x, y, shift| {
                if let Some(ui) = weak.upgrade() {
                    doc.pointer_down(&ui, (x, y), shift, clock.monotonic());
                }
            });
        }
        {
            let (doc, weak, scroll) = (doc.clone(), ui.as_weak(), autoscroll.clone());
            ui.on_pointer_drag(move |x, y| {
                let Some(ui) = weak.upgrade() else { return };
                if doc.drag_to(&ui, (x, y)) {
                    if !scroll.running() {
                        let (doc, weak, timer) = (doc.clone(), weak.clone(), Rc::downgrade(&scroll));
                        scroll.start(TimerMode::Repeated, AUTOSCROLL_STEP, move || {
                            let more = weak.upgrade().is_some_and(|ui| doc.autoscroll(&ui));
                            if !more {
                                if let Some(t) = timer.upgrade() {
                                    t.stop();
                                }
                            }
                        });
                    }
                } else {
                    scroll.stop();
                }
            });
        }
        {
            let (doc, scroll) = (doc.clone(), autoscroll.clone());
            ui.on_pointer_up(move || {
                doc.pointer_up();
                scroll.stop();
            });
        }
        // callbacks that only pass one event on to the app
        let forward = |ev: NoteEvent| {
            let on_event = on_event.clone();
            move || on_event(id, ev)
        };
        ui.on_pointer_moved(forward(NoteEvent::PointerMoved));
        ui.on_start_move(forward(NoteEvent::StartMove));
        {
            let on_event = on_event.clone();
            ui.on_start_resize(move |code| {
                if let Some(edge) = ResizeEdge::from_code(code) {
                    on_event(id, NoteEvent::StartResize(edge));
                }
            });
        }
        ui.on_new_note(forward(NoteEvent::New));
        ui.on_close_note(forward(NoteEvent::Close));
        ui.on_insert_image(forward(NoteEvent::InsertImage));
        ui.on_delete_note(forward(NoteEvent::Delete));
        ui.on_paste_image(move || paste_image(id));
        ui.show()?;
        let geometry = Rc::new(GeometrySave { id, db: autosave.db.clone(), ui: ui.as_weak(), last: Cell::new(None), dirty: Cell::new(false) });
        geometry.current(); // remember where the window opened
        Ok(Note { id, ui, doc, autosave, timer, history_timer, geometry, geometry_timer: Rc::new(Timer::default()), changed })
    }

    /// Where an image added now goes: after the block that has the caret (or last had it).
    pub fn insert_position(&self) -> usize {
        let last = self.doc.doc.borrow().len() - 1;
        let focused = self.ui.get_text_focused_block();
        let at = if focused >= 0 { focused } else { self.ui.get_last_focused_block() };
        if at < 0 { last } else { (at as usize).min(last) }
    }

    /// Put a stored picture into the note as a new block after block `after`, and move the caret to
    /// the paragraph below it. The picture is saved in the database now; the text that refers to
    /// it follows with the usual autosave.
    pub fn add_image(&self, stored: &Stored, after: usize) -> Result<(), crate::storage::DbError> {
        let image_id = self.autosave.db.add_image(self.id, stored.mime, stored.width, stored.height, &stored.data)?;
        let pixels = crate::images::decode(&stored.data).ok().map(|p| crate::note_images::to_slint(&p));
        self.doc.images.add(image_id, (stored.width, stored.height), pixels);
        let at = self.doc.doc.borrow_mut().insert_image(after, image_id);
        (self.changed)(Some(Caret { block: at + 1, offset: 0 }));
        Ok(())
    }

    /// Bytes of decoded pictures this note holds right now.
    pub fn decoded_image_bytes(&self) -> usize {
        self.doc.images.decoded_bytes()
    }

    /// Drop the decoded pictures (suspension); they are decoded again by [`Note::resume`].
    pub fn free_images(&self) {
        self.doc.images.free();
        self.doc.sync_model();
    }

    /// Delete stored pictures the text no longer shows (called when the note closes).
    pub fn prune_images(&self) {
        let keep: Vec<i64> = self.doc.doc.borrow().blocks().iter().filter_map(|b| b.image_id).collect();
        if let Err(e) = self.autosave.db.prune_images(self.id, &keep) {
            eprintln!("washi: could not remove unused pictures of note {}: {e}", self.id);
        }
    }

    /// The user (or the platform) moved or resized the window: save the new geometry after the
    /// geometry debounce.
    pub fn geometry_changed(&self) {
        self.geometry.dirty.set(true);
        let g = self.geometry.clone();
        self.geometry_timer.start(TimerMode::SingleShot, GEOMETRY_DELAY, move || g.flush());
    }

    /// A restored note must be reachable: if its top strip is on no monitor (it was saved on a
    /// monitor that is gone), move it onto the primary work area. `areas` are physical pixels.
    pub fn keep_on_screen(&self, areas: &[ScreenRect]) {
        let w = self.ui.window();
        let (p, s) = (w.position(), w.size());
        let margin = (self.ui.get_window_margin() * w.scale_factor()).round() as i32;
        let here = ScreenRect::new(p.x, p.y, s.width as i32, s.height as i32);
        if let Some(r) = crate::placement::recover(here, margin, areas) {
            w.set_position(slint::PhysicalPosition::new(r.x, r.y));
            if (r.w, r.h) != (here.w, here.h) {
                w.set_size(slint::PhysicalSize::new(r.w as u32, r.h as u32));
            }
            self.geometry_changed();
        }
    }

    pub fn id(&self) -> NoteId {
        self.id
    }

    /// The note's selection: where it started and where the caret is (the same caret: none).
    pub fn selection(&self) -> Range {
        self.doc.selection.get()
    }

    /// The native window handle, once the window exists (never in the test harness).
    pub fn hwnd(&self) -> Option<isize> {
        use raw_window_handle::{HasWindowHandle, RawWindowHandle};
        let window_handle = self.ui.window().window_handle();
        let handle = window_handle.window_handle().ok()?;
        match handle.as_raw() {
            RawWindowHandle::Win32(w) => Some(w.hwnd.get()),
            _ => None,
        }
    }

    pub fn ui(&self) -> &NoteWindow {
        &self.ui
    }

    /// Where the window is and how big, in logical pixels.
    pub fn geometry(&self) -> Geometry {
        self.geometry.current().unwrap_or(Geometry { x: 0, y: 0, w: 0, h: 0 })
    }

    pub fn always_on_top(&self) -> bool {
        self.ui.get_pinned()
    }

    pub fn set_always_on_top(&self, on: bool) {
        self.ui.set_pinned(on);
    }

    /// The title the window (and its taskbar button) currently shows.
    pub fn title(&self) -> String {
        self.ui.get_window_title().to_string()
    }

    /// Replace the note's text without typing it.
    pub fn set_text(&self, text: &str) {
        // the fields of the old blocks vanish without saying they lost the focus
        self.ui.set_text_focused_block(-1);
        self.ui.set_focus_block(-1);
        self.doc.load(text);
        self.doc.set_caret(START);
        self.doc.end_range(&self.ui);
        self.doc.show_formats(&self.ui);
        self.doc.relayout(&self.ui);
        self.ui.set_text(self.doc.text().as_str().into());
        self.ui.set_window_title(window_title(text).into());
    }

    pub fn text(&self) -> String {
        self.ui.get_text().to_string()
    }

    /// Suspension: stop this note's timers and its caret blink (a window that is not active
    /// does not blink). Unsaved edits must have been flushed first.
    pub fn pause(&self) {
        self.timer.stop();
        self.history_timer.stop();
        self.geometry_timer.stop();
        // the text keeps its look (editing or resting) while nobody can see it: no redraw
        self.ui.set_suspended(true);
        self.ui.window().dispatch_event(slint::platform::WindowEvent::WindowActiveChanged(false));
    }

    /// Waking: draw the whole note again; `focused` notes get their caret blink back.
    pub fn resume(&self, focused: bool) {
        self.doc.images.thaw(&self.doc.doc.borrow());
        self.doc.sync_model();
        self.ui.set_suspended(false);
        // fields that lost the focus while suspended say so now
        self.ui.set_text_resync(self.ui.get_text_resync() + 1);
        if focused {
            self.ui.window().dispatch_event(slint::platform::WindowEvent::WindowActiveChanged(true));
        }
        self.ui.window().request_redraw();
    }

    /// Save the text now if there are unsaved edits (focus lost); the position keeps its own debounce.
    pub fn flush_text(&self) {
        self.timer.stop();
        self.autosave.flush();
    }

    /// Save now if there are unsaved edits (used on close and quit).
    pub fn flush(&self) {
        self.flush_text();
        self.geometry_timer.stop();
        self.geometry.flush();
    }
}

impl Drop for Note {
    /// Release the window (and with it the native window and its buffers).
    fn drop(&mut self) {
        let _ = self.ui.hide();
    }
}
