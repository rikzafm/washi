//! Standalone, deep document editor engine.
//!
//! Encapsulates Document AST state, History<Document> undo/redo snapshots, textflow layout
//! and stepped line geometry, ADR-0002 note-level selection wash rects and marker wash overlays,
//! image resize grips, drag math, hit testing, multi-click selection, Slint view models, and
//! UI event binding.
//!
//! Where things live: this file holds the `Editor` state, its construction, loading and switching
//! notes, and pushing the document into the view models. The rest is split by concern:
//!
//! - `bind`: wiring every Slint callback of a note window to the editor
//! - `layout`: theme tokens, the stepped line layout and the caret box
//! - `selection`: the note-level wash (ADR 0002), caret motion, hit testing, focus and reveal
//! - `editing`: typing, Enter/Backspace/Delete, styles, ink and marker
//! - `images`: picture selection, resize grips, size, alignment, copy and cut
//! - `pointer`: presses, drags, multi-click and autoscroll
//! - `undo`: history snapshots, undo and redo
//! - `clipboard`: copying, cutting and pasting ranges, rich text and pictures
//!
//! Fields only the editor uses are private; the `pub` ones are read by the window, All Notes and
//! the snap tests.

mod bind;
mod clipboard;
mod editing;
mod images;
mod layout;
mod pointer;
mod selection;
mod undo;
#[cfg(test)]
mod tests;

use crate::caret_motion;
use crate::document::{
    clamp_palette, floor_boundary, Align, Block, Caret, Document, Kind, Paint, Range,
    Style as Flags, IMAGE_PCT_MAX, IMAGE_PCT_MIN, PALETTE, PALETTE_MAX,
};
use crate::history::History;
use crate::images::{Rgba, Stored};
use crate::note_images::NoteImages;
use crate::note_window::{
    window_title, BlockData, BlockKind, KeyMods, MarkerData, NoteWindow, PaletteEntry, PickerPalette, RuleData, RunData,
    SelRect, Theme,
};
use crate::platform::{Clock, ImageClipboard, NoteId};
use crate::storage::{Db, DbError};
use crate::textflow::{self, Style, Tokens};
use slint::{ComponentHandle, Model, Timer, TimerMode, VecModel};
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

pub const CURSOR_TEXT: i32 = 0;
pub const CURSOR_DEFAULT: i32 = 1;
pub const CURSOR_RESIZE: i32 = 2;

pub const FORMAT_BOLD: i32 = 0;
pub const FORMAT_ITALIC: i32 = 1;
pub const FORMAT_UNDERLINE: i32 = 2;
pub const FORMAT_STRIKE: i32 = 3;
pub const FORMAT_LIST: i32 = 4;

/// Typing is snapshotted once it pauses this long; structural changes are snapshotted at once.
pub const HISTORY_PAUSE: Duration = Duration::from_millis(500);

/// Two presses count as a double (triple) click when they come this soon after each other...
pub const MULTI_CLICK: Duration = Duration::from_millis(500);
/// ...and this close (logical px).
pub const MULTI_CLICK_SLOP: f32 = 4.0;

/// While a drag is held outside the text area, the note scrolls this often.
pub const AUTOSCROLL_STEP: Duration = Duration::from_millis(40);

/// The shown text of an empty note.
pub const HINT: &str = "Note Something....";

pub const START: Caret = Caret { block: 0, offset: 0 };

/// Side of a picture's resize grip, logical px.
pub const GRIP: f64 = 10.0;

pub fn rgb(c: u32) -> slint::Color {
    slint::Color::from_rgb_u8((c >> 16) as u8, (c >> 8) as u8, c as u8)
}

/// The text color of palette entry `i` (the default ink for 0 and for an index past the table).
pub fn ink_color(i: u8) -> slint::Color {
    rgb(PALETTE.get(i as usize).unwrap_or(&PALETTE[0]).ink)
}

/// A swatch's marker as a color: see-through when it has none.
pub fn swatch_marker(marker: Option<u32>) -> slint::Color {
    marker.map_or(slint::Color::from_argb_u8(0, 0, 0, 0), rgb)
}

/// The marker color of palette entry `i` (see-through for none).
pub fn marker_color(i: u8) -> slint::Color {
    swatch_marker(PALETTE.get(i as usize).and_then(|s| s.marker))
}

/// Whether two rows of the block model are the same.
pub fn same_row(a: &BlockData, b: &BlockData) -> bool {
    a.kind == b.kind
        && a.text == b.text
        && a.bold == b.bold
        && (a.img_w, a.img_h) == (b.img_w, b.img_h)
        && (a.img_pct, a.img_align) == (b.img_pct, b.img_align)
        && a.img.size() == b.img.size()
        && (a.img.size().width == 0 || a.img == b.img)
}

pub fn block_data(b: &Block) -> BlockData {
    let kind = match b.kind {
        Kind::Title => BlockKind::Title,
        Kind::Heading => BlockKind::Heading,
        Kind::Body => BlockKind::Body,
        Kind::ListItem => BlockKind::ListItem,
        Kind::Image => BlockKind::Image,
    };
    let img_align = match b.align {
        Align::Left => 0,
        Align::Center => 1,
        Align::Right => 2,
    };
    BlockData {
        kind,
        text: b.text.as_str().into(),
        bold: b.kind == Kind::Heading,
        italic: false,
        img: Default::default(),
        img_turned: Default::default(),
        img_w: 0,
        img_h: 0,
        img_pct: b.image_pct as i32,
        img_align,
    }
}

/// The note's wrapped lines, measured for vertical caret motion.
struct NoteLines<'a> {
    doc: &'a Document,
    width: f64,
    tokens: &'a Tokens,
}

impl caret_motion::Lines for NoteLines<'_> {
    fn line_at(&self, block: usize, offset: usize, line_end: bool) -> caret_motion::LineAt {
        let at = textflow::caret_line_on(&self.doc.blocks()[block], offset, line_end, self.width, self.tokens);
        caret_motion::LineAt { line: at.line, lines: at.lines, x: at.x }
    }

    fn offset_on(&self, block: usize, line: usize, x: f64) -> (usize, bool) {
        textflow::offset_on_line(&self.doc.blocks()[block], line, x, self.width, self.tokens)
    }
}

/// The pointer's press in progress: what a drag extends, by characters, words or paragraphs.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PointerState {
    /// The left button is down.
    pub down: bool,
    /// 1 click: by characters, 2: by words, 3: by paragraphs.
    pub clicks: u8,
    /// Where the press started: the anchor of a character drag, the first word or paragraph of the others.
    pub origin: Range,
    pub last_press: Option<(Duration, (f32, f32))>,
    /// The last place of a drag: x in the layer, y in the text area (outside it: scrolling).
    pub drag: (f32, f32),
}

impl Default for PointerState {
    fn default() -> Self {
        Self {
            down: false,
            clicks: 0,
            origin: Range { anchor: START, focus: START },
            last_press: None,
            drag: (0.0, 0.0),
        }
    }
}

#[derive(Clone, Copy, Default)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn unix_millis(&self) -> i64 {
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0)
    }
    fn monotonic(&self) -> Duration {
        static EPOCH: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();
        EPOCH.get_or_init(std::time::Instant::now).elapsed()
    }
}

pub struct Editor {
    id: Cell<NoteId>,
    pub db: Rc<Db>,
    clipboard: Rc<dyn ImageClipboard>,
    pub doc: RefCell<Document>,
    pub history: RefCell<History<Document>>,
    pub images: NoteImages,

    // Slint Model backing stores
    pub model: Rc<VecModel<BlockData>>,
    pub runs: Rc<VecModel<RunData>>,
    pub rules: Rc<VecModel<RuleData>>,
    pub sel_rects: Rc<VecModel<SelRect>>,
    pub handles: Rc<VecModel<SelRect>>,
    markers: RefCell<Vec<textflow::Marker>>,
    pub marker_data: Rc<VecModel<MarkerData>>,
    pub marker_wash: Rc<VecModel<SelRect>>,

    // Editor state
    selection: Cell<Range>,
    pub range: Cell<Option<Range>>,
    /// The caret that sits at the end of a line whose next line starts at the same offset (a wrap
    /// without a space), when the caret is there; any other caret is drawn where its offset starts.
    line_end: Cell<Option<Caret>>,
    /// The column Up and Down keep to (x in the line, with the caret they left it at), so the caret
    /// comes back to it after a shorter line.
    goal_x: Cell<Option<(Caret, f64)>>,
    /// The caret as last shown: x, top and bottom of its line box, or none.
    shown_caret: Cell<Option<(f32, f32, f32)>>,
    /// The text stays tilted while editing (Settings › General): the note washes every selection
    /// itself, tilted, and the field's own wash is off.
    tilted: Cell<bool>,
    pointer: Cell<PointerState>,
    typing: Cell<Option<(Caret, Flags)>>,
    typing_paint: Cell<Option<(Caret, Paint)>>,
    pub paper: Cell<(f64, f64)>,
    pub scale_factor: Cell<f64>,
    grip: RefCell<Option<(usize, Document)>>,
    right_click: Cell<Option<(f32, f32)>>,

    // Timers
    history_timer: Rc<Timer>,
    autoscroll: Rc<Timer>,

    // Bound UI reference & change callback
    ui: RefCell<Option<slint::Weak<NoteWindow>>>,
    on_changed: RefCell<Option<Rc<dyn Fn(Option<Caret>)>>>,
    clock: RefCell<Rc<dyn Clock>>,
}

/// Bring `model` to `rows`, touching only the rows that differ. A relayout (every step of a resize)
/// mostly leaves the words where they are: resetting the model made Slint build every word's element
/// again, which cost several times the layout itself.
fn sync_rows<T: Clone + PartialEq + 'static>(model: &VecModel<T>, rows: Vec<T>) {
    let had = model.row_count();
    if had == 0 || rows.is_empty() {
        model.set_vec(rows);
        return;
    }
    for (i, row) in rows.iter().enumerate().take(had) {
        if model.row_data(i).as_ref() != Some(row) {
            model.set_row_data(i, row.clone());
        }
    }
    for i in (rows.len()..had).rev() {
        model.remove(i);
    }
    for row in rows.into_iter().skip(had) {
        model.push(row);
    }
}

impl Editor {
    /// Create a new Editor instance initialized with content.
    pub fn new(id: NoteId, content: &str, db: Rc<Db>, clipboard: Rc<dyn ImageClipboard>) -> Self {
        let parsed = Document::parse(content);
        let images = NoteImages::new(id, db.clone());
        images.load(&parsed);

        let model = Rc::new(VecModel::default());
        let runs = Rc::new(VecModel::default());
        let rules = Rc::new(VecModel::default());
        let sel_rects = Rc::new(VecModel::default());
        let handles = Rc::new(VecModel::default());
        let marker_data = Rc::new(VecModel::default());
        let marker_wash = Rc::new(VecModel::default());

        let editor = Editor {
            id: Cell::new(id),
            db,
            clipboard,
            doc: RefCell::new(parsed.clone()),
            history: RefCell::new(History::new(parsed.clone())),
            images,
            model,
            runs,
            rules,
            sel_rects,
            handles,
            markers: RefCell::new(vec![]),
            marker_data,
            marker_wash,
            selection: Cell::new(Range { anchor: START, focus: START }),
            range: Cell::new(None),
            line_end: Cell::new(None),
            goal_x: Cell::new(None),
            shown_caret: Cell::new(None),
            tilted: Cell::new(false),
            pointer: Cell::new(PointerState::default()),
            typing: Cell::new(None),
            typing_paint: Cell::new(None),
            paper: Cell::new((363.0, 433.0)),
            scale_factor: Cell::new(1.0),
            grip: RefCell::new(None),
            right_click: Cell::new(None),
            history_timer: Rc::new(Timer::default()),
            autoscroll: Rc::new(Timer::default()),
            ui: RefCell::new(None),
            on_changed: RefCell::new(None),
            clock: RefCell::new(Rc::new(SystemClock)),
        };
        editor.load(content);
        editor
    }


    /// Synchronize view models and editor UI elements following a document modification.
    pub fn apply_change(&self, ui: &NoteWindow, focus: Option<Caret>) {
        let text = self.text();
        self.sync_model();
        self.relayout(ui);
        ui.set_text(text.as_str().into());
        ui.set_window_title(window_title(&text).into());
        if let Some(c) = focus {
            self.set_caret(c);
            self.end_range(ui);
        }
        self.show_formats(ui);
        if let Some(c) = focus {
            self.focus_at(ui, c);
        }
        self.show_caret(ui);
    }

    /// Notify change across UI and external callbacks.
    pub fn notify_change(&self, focus: Option<Caret>) {
        if let Some(ui) = self.ui.borrow().as_ref().and_then(|w| w.upgrade()) {
            self.apply_change(&ui, focus);
        } else {
            self.sync_model();
            if let Some(c) = focus {
                self.set_caret(c);
            }
        }
        if let Some(cb) = self.on_changed.borrow().as_ref() {
            cb(focus);
        }
    }

    pub fn set_clock(&self, clock: Rc<dyn Clock>) {
        *self.clock.borrow_mut() = clock;
    }

    pub fn monotonic_time(&self) -> Duration {
        self.clock.borrow().monotonic()
    }

    // --- Document State & Content ---

    pub fn id(&self) -> NoteId {
        self.id.get()
    }

    pub fn text(&self) -> String {
        self.doc.borrow().to_text()
    }

    pub fn is_empty(&self) -> bool {
        self.doc.borrow().is_empty()
    }

    /// Replace everything (opening a note, or text set from outside).
    pub fn load(&self, content: &str) {
        let parsed = Document::parse(content);
        self.images.load(&parsed);
        self.model.set_vec(parsed.blocks().iter().map(|b| self.block_data(b)).collect::<Vec<_>>());
        *self.history.borrow_mut() = History::new(parsed.clone());
        *self.doc.borrow_mut() = parsed;
    }

    /// Set text with active UI reset.
    pub fn set_text(&self, ui: &NoteWindow, text: &str) {
        ui.set_text_focused_block(-1);
        ui.set_focus_block(-1);
        ui.set_scrolling(false);
        self.load(text);
        self.set_caret(START);
        self.end_range(ui);
        self.show_formats(ui);
        self.relayout(ui);
        ui.set_text(self.text().as_str().into());
        ui.set_window_title(window_title(text).into());
    }

    /// Switch active note to a new id and content.
    pub fn switch_to(&self, ui: &NoteWindow, id: NoteId, content: &str) {
        self.history_timer.stop();
        ui.invoke_focus_keys();
        self.id.set(id);
        self.images.retarget(id);
        self.typing.set(None);
        self.typing_paint.set(None);
        self.grip.replace(None);
        self.right_click.set(None);
        self.pointer.set(PointerState::default());
        ui.set_text_focused_block(-1);
        ui.set_focus_block(-1);
        ui.set_scrolling(false);
        ui.set_scroll_y(0.0);
        self.load(content);
        self.set_caret(START);
        self.end_range(ui);
        self.show_formats(ui);
        self.relayout(ui);
        ui.set_text(self.text().as_str().into());
        ui.set_window_title(window_title(content).into());
        ui.set_switch_serial(ui.get_switch_serial() + 1);
    }

    /// Make the window's blocks match the document, touching only rows that differ.
    pub fn sync_model(&self) {
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

    /// A block as the window shows it.
    pub fn block_data(&self, b: &Block) -> BlockData {
        let (img_w, img_h) = self.images.size(b).unwrap_or((0, 0));
        BlockData {
            img: self.images.pixels(b),
            img_turned: self.images.turned(b),
            img_w: img_w as i32,
            img_h: img_h as i32,
            ..block_data(b)
        }
    }

    // --- Lifecycle ---

    pub fn pause(&self) {
        self.history_timer.stop();
        self.autoscroll.stop();
    }

    pub fn resume(&self) {
        self.thaw_images();
    }
}

impl Drop for Editor {
    fn drop(&mut self) {
        self.history_timer.stop();
        self.autoscroll.stop();
    }
}
