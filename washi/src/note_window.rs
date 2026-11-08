//! One open note: the Slint window plus its 500 ms autosave debounce.

use crate::platform::{Clock, NoteId, ResizeEdge, ScreenRect};
use crate::storage::{Db, Geometry};
use slint::{CloseRequestResponse, ComponentHandle, Timer, TimerMode};
use crate::document::{Block, Caret, Document, Kind, Range, Style as Flags};
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

/// The note's document, shared between the window's model and the stored text.
struct Doc {
    doc: RefCell<Document>,
    model: Rc<VecModel<BlockData>>,
    runs: Rc<VecModel<RunData>>,
    rules: Rc<VecModel<RuleData>>,
    /// The wash of the note-level selection, one level box per line it touches.
    sel_rects: Rc<VecModel<SelRect>>,
    /// The note-level selection shown by `sel_rects` (for now only a select-all), if any.
    range: Cell<Option<Range>>,
    /// The block that last held the caret: what the toolbar and the format shortcuts act on.
    caret: Cell<usize>,
    /// The caret's byte offset in that block and the selection's other end (equal: no selection).
    selection: Cell<(usize, usize)>,
    /// A format button pressed with nothing selected, outside a word: the style the next text
    /// typed at that caret takes. Gone once the caret moves elsewhere.
    typing: Cell<Option<(Caret, Flags)>>,
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
            range: Cell::new(None),
            caret: Cell::new(0),
            selection: Cell::new((0, 0)),
            typing: Cell::new(None),
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
        let (mut runs, rules) = textflow::layout_all_with_images(doc.blocks(), self.paper.get(), &tokens, &|b| self.images.size(b));
        ui.set_content_height(textflow::extent_with_images(doc.blocks(), self.paper.get().0, &tokens, &|b| self.images.size(b)).1 as f32);
        self.rules.set_vec(
            rules
                .iter()
                .map(|r| RuleData { block: r.block as i32, x: r.x as f32, y: r.y as f32, dx: r.dx as f32, dy: r.dy as f32, width: r.width as f32, height: r.height as f32 })
                .collect::<Vec<_>>(),
        );
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
        self.sel_rects.set_vec(
            rects.iter().map(|r| SelRect { x: r.x as f32, y: r.y as f32, width: r.width as f32, height: r.height as f32 }).collect::<Vec<_>>(),
        );
    }

    /// Ctrl+A: the note-level selection becomes the whole note.
    fn select_all(&self, ui: &NoteWindow) {
        self.range.set(Some(self.doc.borrow().all()));
        self.fill_sel_rects(&self.doc.borrow(), &Self::tokens(ui));
    }

    /// The select-all ended: no wash.
    fn clear_selection(&self) {
        self.range.set(None);
        self.sel_rects.set_vec(vec![]);
    }

    fn text(&self) -> String {
        self.doc.borrow().to_text()
    }

    /// What a key does over a select-all (its text): 1 a modifier key alone, so the selection stays;
    /// 2 Backspace, Delete, Enter or a character replaced the whole note (the caret to place is
    /// returned too); 0 any other key.
    fn all_key(&self, key: &str) -> (i32, Option<Caret>) {
        use slint::platform::Key;
        let mut chars = key.chars();
        let (Some(c), None) = (chars.next(), chars.next()) else { return (0, None) };
        // Shift, Control, Alt, AltGr, CapsLock, Meta and their right-hand twins
        if ('\u{10}'..='\u{18}').contains(&c) {
            return (1, None);
        }
        let is = |k: Key| c == char::from(k);
        let text = if is(Key::Backspace) || is(Key::Delete) || is(Key::Return) {
            String::new()
        } else if !c.is_control() && !('\u{f700}'..='\u{f8ff}').contains(&c) {
            c.to_string()
        } else {
            return (0, None);
        };
        (2, Some(self.replace_all(&text)))
    }

    /// The whole note becomes `text`; the caret after it.
    fn replace_all(&self, text: &str) -> Caret {
        let before = self.doc.borrow().clone();
        let caret = self.doc.borrow_mut().replace_all(text);
        self.structural(before);
        caret
    }

    /// The caret's block, kept within the document.
    fn caret_block(&self) -> usize {
        self.caret.get().min(self.doc.borrow().len().saturating_sub(1))
    }

    /// The style the next text typed at the caret takes: one a format button asked for there, or
    /// that of the text before it.
    fn typing_style(&self) -> Flags {
        let i = self.caret_block();
        let (cursor, _) = self.selection.get();
        match self.typing.get() {
            Some((c, style)) if c == (Caret { block: i, offset: cursor }) => style,
            _ => self.doc.borrow().blocks().get(i).map_or(Flags::NONE, |b| b.style_for_insert(cursor)),
        }
    }

    /// Toolbar button or shortcut `what` (0 bold, 1 italic, 2 underline, 3 strike) on the selected
    /// text: the whole note after a select-all, else the selection in the caret's paragraph, else
    /// the word holding the caret; outside a word, on what is typed next. 4 (list) toggles the
    /// caret's paragraph. True when the document changed.
    fn format(&self, what: i32, all_selected: bool) -> bool {
        let i = self.caret_block();
        let flag = match what {
            0 => Flags::BOLD,
            1 => Flags::ITALIC,
            2 => Flags::UNDERLINE,
            3 => Flags::STRIKE,
            4 => {
                let before = self.doc.borrow().clone();
                let done = self.doc.borrow_mut().toggle_list(i);
                if done {
                    self.structural(before);
                }
                return done;
            }
            _ => return false,
        };
        let (cursor, anchor) = self.selection.get();
        let before = self.doc.borrow().clone();
        let done = {
            let mut doc = self.doc.borrow_mut();
            if all_selected {
                doc.toggle_style_all(flag)
            } else if cursor != anchor {
                doc.toggle_style(i, cursor, anchor, flag)
            } else if let Some((start, end)) = doc.word_at(i, cursor) {
                doc.toggle_style(i, start, end, flag)
            } else {
                false
            }
        };
        if done {
            self.typing.set(None);
            self.structural(before);
        } else if !all_selected && cursor == anchor {
            let style = self.typing_style().toggled(flag);
            self.typing.set(Some((Caret { block: i, offset: cursor }, style)));
        }
        done
    }

    /// Show the formats of the selection (or of what is typed at the caret) on the toolbar buttons.
    fn show_formats(&self, ui: &NoteWindow) {
        let style = {
            let doc = self.doc.borrow();
            let (cursor, anchor) = self.selection.get();
            match doc.blocks().get(self.caret_block()) {
                _ if ui.get_text_all_selected() => doc.common_style_all(),
                Some(b) if cursor != anchor => b.common_style(cursor, anchor),
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
    }

    /// The caret moved to `offset` of block `block`, the selection's other end at `anchor`. A style
    /// asked for at another place is forgotten.
    fn caret_moved(&self, block: usize, offset: usize, anchor: usize) {
        self.caret.set(block);
        self.selection.set((offset, anchor));
        if self.typing.get().is_some_and(|(c, _)| c != (Caret { block, offset }) || offset != anchor) {
            self.typing.set(None);
        }
    }

    /// Ask the block at `c` to take the keyboard focus with the caret there, and scroll it into view.
    fn focus_at(&self, ui: &NoteWindow, c: Caret) {
        ui.set_focus_block(c.block as i32);
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
            doc.edit_text(i, text, cursor, typing);
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

    /// Enter, Backspace, Delete, Up or Down in block `i` at `offset`. `Some((caret, changed))` when
    /// the note takes the key (the document changed, or only the caret moves to another block);
    /// `None` when the field keeps it.
    fn key(&self, i: usize, key: &str, offset: usize, ui: &NoteWindow) -> Option<(Caret, bool)> {
        use slint::platform::Key;
        let before = self.doc.borrow().clone();
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
        let doc = Doc::new(NoteImages::new(id, db.clone()));
        doc.load(content);
        ui.set_blocks(slint::ModelRc::from(doc.model.clone()));
        ui.set_runs(slint::ModelRc::from(doc.runs.clone()));
        ui.set_rules(slint::ModelRc::from(doc.rules.clone()));
        ui.set_sel_rects(slint::ModelRc::from(doc.sel_rects.clone()));
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
                        doc.caret.set(c.block);
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
                if doc.format(what, ui.get_text_all_selected()) {
                    changed(None);
                } else {
                    // a style for what is typed next
                    doc.show_formats(&ui);
                }
            });
        }
        {
            let (doc, weak) = (doc.clone(), ui.as_weak());
            ui.on_focus_entered(move |i| {
                doc.caret.set(i.max(0) as usize);
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
            ui.on_select_cleared(move || doc.clear_selection());
        }
        {
            let doc = doc.clone();
            ui.on_all_text(move || doc.doc.borrow().plain_text().into());
        }
        {
            let (changed, doc) = (changed.clone(), doc.clone());
            ui.on_replace_all(move |text| {
                let caret = doc.replace_all(text.as_str());
                changed(Some(caret));
            });
        }
        {
            let (changed, doc) = (changed.clone(), doc.clone());
            ui.on_all_key(move |key| {
                let (taken, caret) = doc.all_key(key.as_str());
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
                doc.caret_moved(block, offset, anchor.max(0) as usize);
                if let Some(ui) = weak.upgrade() {
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
        self.doc.caret.set(0);
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
