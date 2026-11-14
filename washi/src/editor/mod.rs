//! Standalone, deep document editor engine.
//!
//! Encapsulates Document AST state, History<Snapshot> undo/redo snapshots, textflow layout
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

pub use editing::Formatted;
pub use undo::{HistoryStep, Snapshot};
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
    window_title, BlockData, BlockKind, CheckData, KeyMods, MarkerData, NoteWindow, PaletteEntry, PickerPalette, RuleData, RunData,
    SelRect, Theme,
};
use crate::platform::{Clock, ImageClipboard, NoteId};
use crate::storage::{Db, DbError};
use crate::textflow::{self, Layout, Style, Tokens};
use slint::{ComponentHandle, Model, Timer, TimerMode, VecModel};
use std::cell::{Cell, Ref, RefCell, RefMut};
use std::rc::Rc;
use std::time::Duration;

pub const CURSOR_TEXT: i32 = 0;
pub const CURSOR_DEFAULT: i32 = 1;
pub const CURSOR_RESIZE: i32 = 2;
/// Over a check box: the pointing hand.
pub const CURSOR_POINTER: i32 = 3;

pub const FORMAT_BOLD: i32 = 0;
pub const FORMAT_ITALIC: i32 = 1;
pub const FORMAT_UNDERLINE: i32 = 2;
pub const FORMAT_STRIKE: i32 = 3;
pub const FORMAT_LIST: i32 = 4;
pub const FORMAT_CHECK: i32 = 5;

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

fn block_kind(kind: Kind) -> BlockKind {
    match kind {
        Kind::Title => BlockKind::Title,
        Kind::Heading => BlockKind::Heading,
        Kind::Body => BlockKind::Body,
        Kind::ListItem => BlockKind::ListItem,
        Kind::Check => BlockKind::Check,
        Kind::Checked => BlockKind::Checked,
        Kind::Image => BlockKind::Image,
    }
}

fn image_align(align: Align) -> i32 {
    match align {
        Align::Left => 0,
        Align::Center => 1,
        Align::Right => 2,
    }
}

/// The picture ids of a document, in order.
fn image_ids(doc: &Document) -> Vec<i64> {
    doc.blocks().iter().filter_map(|b| b.image_id).collect()
}

pub fn block_data(b: &Block) -> BlockData {
    let kind = block_kind(b.kind);
    let img_align = image_align(b.align);
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
    layout: &'a Layout,
}

impl caret_motion::Lines for NoteLines<'_> {
    fn line_at(&self, block: usize, offset: usize, line_end: bool) -> caret_motion::LineAt {
        let at = self.layout.caret_line(block, offset, line_end).expect("the caret's block exists");
        // x counts from the text area's left edge, so Up and Down keep their column across an indented item
        caret_motion::LineAt { line: at.line, lines: at.lines, x: at.x + self.layout.indent(block) }
    }

    fn offset_on(&self, block: usize, line: usize, x: f64) -> (usize, bool) {
        self.layout.offset_on_line(block, line, x - self.layout.indent(block)).expect("the caret's block exists")
    }
}

/// What a cached layout was built from: when any of it differs, the layout is stale.
#[derive(Clone, PartialEq)]
struct LayoutKey {
    doc_rev: u64,
    paper: (f64, f64),
    tokens: Tokens,
    images_gen: u64,
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

/// One note's editing state: its document, selection, history and the models the window shows.
///
/// The note's geometry is one cached [`Layout`] (see [`Self::layout`]), shared by every question
/// about where text is: the caret box, the pointer, the selection wash, a picture's rect, scrolling.
/// It is rebuilt on the first question after any of these changed, and not otherwise:
/// - the document, which retires it in [`Self::doc_mut`], the only way to write the document, so a
///   write cannot forget to;
/// - the paper size;
/// - the tokens (font sizes, pitches, insets, letter spacing, tilt);
/// - the size of a picture, once its pixels are known (a picture added, replaced or loaded).
///
/// Caret motion, selection changes and the pointer change none of these and never rebuild it.
pub struct Editor {
    id: Cell<NoteId>,
    pub db: Rc<Db>,
    clipboard: Rc<dyn ImageClipboard>,
    /// The note's text. Private: writes go through [`Self::doc_mut`], which retires the cached layout.
    doc: RefCell<Document>,
    /// Counts the writes to `doc`; part of the layout's key.
    doc_rev: Cell<u64>,
    /// The note's geometry as last built, with what it was built from.
    layout: RefCell<Option<(LayoutKey, Rc<Layout>)>>,
    /// Cache hits so far; every so often a debug build checks one against a fresh build.
    #[cfg(debug_assertions)]
    layout_hits: Cell<u32>,
    /// The picture ids of the document as `images` last learned them (see [`Self::sync_model`]).
    image_ids: RefCell<Vec<i64>>,
    pub history: RefCell<History<Snapshot>>,
    pub images: NoteImages,

    // Slint Model backing stores
    pub model: Rc<VecModel<BlockData>>,
    pub runs: Rc<VecModel<RunData>>,
    pub rules: Rc<VecModel<RuleData>>,
    pub checks: Rc<VecModel<CheckData>>,
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
        let checks = Rc::new(VecModel::default());
        let sel_rects = Rc::new(VecModel::default());
        let handles = Rc::new(VecModel::default());
        let marker_data = Rc::new(VecModel::default());
        let marker_wash = Rc::new(VecModel::default());

        let editor = Editor {
            id: Cell::new(id),
            db,
            clipboard,
            doc: RefCell::new(parsed.clone()),
            doc_rev: Cell::new(0),
            layout: RefCell::new(None),
            #[cfg(debug_assertions)]
            layout_hits: Cell::new(0),
            image_ids: RefCell::new(image_ids(&parsed)),
            history: RefCell::new(History::new(Snapshot { doc: parsed.clone(), caret: START })),
            images,
            model,
            runs,
            rules,
            checks,
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
        self.lay_out(ui);
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
        self.doc().to_text()
    }

    /// The note's text, to read.
    pub fn doc(&self) -> Ref<'_, Document> {
        self.doc.borrow()
    }

    /// The note's text, to write: the only way to change it, which retires the cached layout. Drop
    /// the guard before asking for the [`Self::layout`]: a layout cannot be built from a document
    /// that is being changed. It retires the layout when called, not when something changes, so a
    /// write that changes nothing rebuilds it needlessly: deliberate, it is safe and cheap to be wrong that way.
    pub fn doc_mut(&self) -> RefMut<'_, Document> {
        self.doc_rev.set(self.doc_rev.get() + 1);
        self.doc.borrow_mut()
    }

    /// The note's geometry in the window's tokens, built again only when the document, the paper,
    /// the tokens or the pictures' sizes changed since the last call.
    pub fn layout(&self, ui: &NoteWindow) -> Rc<Layout> {
        self.layout_with(&self.tokens(ui))
    }

    /// [`Self::layout`] with the given tokens (without a window, see [`Self::default_tokens`]).
    pub fn layout_with(&self, tokens: &Tokens) -> Rc<Layout> {
        let key = LayoutKey { doc_rev: self.doc_rev.get(), paper: self.paper.get(), tokens: *tokens, images_gen: self.images.generation() };
        let cached = self.layout.borrow().as_ref().filter(|(k, _)| *k == key).map(|(_, l)| l.clone());
        if let Some(layout) = cached {
            #[cfg(debug_assertions)]
            {
                // now and then, build from scratch and compare: a stale layout shows as another height
                let hits = self.layout_hits.get().wrapping_add(1);
                self.layout_hits.set(hits);
                if hits % 1024 == 0 {
                    let fresh = Layout::build(self.doc().blocks(), key.paper, tokens, &|b| self.images.size(b));
                    debug_assert_eq!(fresh.content_height(), layout.content_height(), "the cached layout is stale");
                    debug_assert_eq!(fresh.runs().len(), layout.runs().len(), "the cached layout is stale");
                    debug_assert_eq!(fresh.block_count(), layout.block_count(), "the cached layout is stale");
                }
            }
            return layout;
        }
        let built = Rc::new(Layout::build(self.doc().blocks(), key.paper, tokens, &|b| self.images.size(b)));
        *self.layout.borrow_mut() = Some((key, built.clone()));
        built
    }

    /// Forget the cached layout (the note is suspended); the next question builds it again.
    pub fn drop_layout(&self) {
        *self.layout.borrow_mut() = None;
    }

    pub fn is_empty(&self) -> bool {
        self.doc().is_empty()
    }

    /// Replace everything (opening a note, or text set from outside).
    pub fn load(&self, content: &str) {
        let parsed = Document::parse(content);
        self.images.load(&parsed);
        *self.image_ids.borrow_mut() = image_ids(&parsed);
        self.model.set_vec(parsed.blocks().iter().map(|b| self.block_data(b)).collect::<Vec<_>>());
        *self.history.borrow_mut() = History::new(Snapshot { doc: parsed.clone(), caret: START });
        *self.doc_mut() = parsed;
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
        let doc = self.doc();
        // the pictures are learned again only when the note's set of them changed (the sizes they
        // are laid out with only change then, and the decoded pixels are freed and thawed elsewhere)
        // (a picture that did not resolve, a failed decode, is tried again at the next sync)
        let ids = image_ids(&doc);
        if *self.image_ids.borrow() != ids || !self.images.knows(&ids) {
            self.images.load(&doc);
            *self.image_ids.borrow_mut() = ids;
        }
        // rows are built only for the span between the rows that are already right at both ends
        let blocks = doc.blocks();
        let old_len = self.model.row_count();
        let same = |row: usize, block: usize| self.model.row_data(row).is_some_and(|r| self.shows(&r, &blocks[block]));
        let most = old_len.min(blocks.len());
        let prefix = (0..most).take_while(|&k| same(k, k)).count();
        let suffix = (0..most - prefix).take_while(|&k| same(old_len - 1 - k, blocks.len() - 1 - k)).count();
        let (old_mid, new_mid) = (old_len - prefix - suffix, blocks.len() - prefix - suffix);
        for k in 0..old_mid.min(new_mid) {
            self.model.set_row_data(prefix + k, self.block_data(&blocks[prefix + k]));
        }
        for _ in new_mid..old_mid {
            self.model.remove(prefix + new_mid);
        }
        for k in old_mid..new_mid {
            self.model.insert(prefix + k, self.block_data(&blocks[prefix + k]));
        }
    }

    /// Whether `row` of the window's blocks already shows `b`: what [`same_row`] says of it and
    /// `block_data(b)`, without building that.
    fn shows(&self, row: &BlockData, b: &Block) -> bool {
        let (img_w, img_h) = self.images.size(b).unwrap_or((0, 0));
        let img = self.images.pixels(b);
        row.kind == block_kind(b.kind)
            && row.text == b.text.as_str()
            && row.bold == (b.kind == Kind::Heading)
            && (row.img_w, row.img_h) == (img_w as i32, img_h as i32)
            && (row.img_pct, row.img_align) == (b.image_pct as i32, image_align(b.align))
            && row.img.size() == img.size()
            && (img.size().width == 0 || row.img == img)
    }

    /// A block as the window shows it.
    pub fn block_data(&self, b: &Block) -> BlockData {
        crate::trace::count(crate::trace::counts::Count::BlockData);
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
