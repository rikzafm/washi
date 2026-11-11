//! washi/src/editor.rs
//! Standalone, deep document editor engine.
//!
//! Encapsulates Document AST state, History<Document> undo/redo snapshots, textflow layout
//! and stepped line geometry, ADR-0002 note-level selection wash rects and marker wash overlays,
//! image resize grips, drag math, hit testing, multi-click selection, Slint view models, and
//! UI event binding.

use crate::document::{
    clamp_palette, floor_boundary, Align, Block, Caret, Document, Kind, Paint, Range,
    Style as Flags, IMAGE_PCT_MAX, IMAGE_PCT_MIN, PALETTE, PALETTE_MAX,
};
use crate::history::History;
use crate::images::{Rgba, Stored};
use crate::note_images::NoteImages;
use crate::note_window::{
    window_title, BlockData, BlockKind, MarkerData, NoteWindow, PaletteEntry, PickerPalette, RuleData, RunData,
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

/// The keys the block field sends for Ctrl+Z and Ctrl+Y (see ui/block.slint).
pub const UNDO_KEY: &str = "\u{1a}";
pub const REDO_KEY: &str = "\u{19}";

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

/// Landing caret on a target block: offset 0 if at start or an image block, else end of text.
fn block_boundary_caret(doc: &Document, block: usize, at_start: bool) -> Caret {
    let offset = if at_start || doc.blocks().get(block).is_some_and(|b| b.kind == Kind::Image) {
        0
    } else {
        doc.blocks().get(block).map_or(0, |b| b.text.len())
    };
    Caret { block, offset }
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
    pub id: Cell<NoteId>,
    pub db: Rc<Db>,
    pub clipboard: Rc<dyn ImageClipboard>,
    pub doc: RefCell<Document>,
    pub history: RefCell<History<Document>>,
    pub images: NoteImages,

    // Slint Model backing stores
    pub model: Rc<VecModel<BlockData>>,
    pub runs: Rc<VecModel<RunData>>,
    pub rules: Rc<VecModel<RuleData>>,
    pub sel_rects: Rc<VecModel<SelRect>>,
    pub handles: Rc<VecModel<SelRect>>,
    pub markers: RefCell<Vec<textflow::Marker>>,
    pub marker_data: Rc<VecModel<MarkerData>>,
    pub marker_wash: Rc<VecModel<SelRect>>,

    // Editor state
    pub selection: Cell<Range>,
    pub range: Cell<Option<Range>>,
    /// The caret that sits at the end of a line whose next line starts at the same offset (a wrap
    /// without a space), when the caret is there; any other caret is drawn where its offset starts.
    pub line_end: Cell<Option<Caret>>,
    /// The column Up and Down keep to (x in the line, with the caret they left it at), so the caret
    /// comes back to it after a shorter line.
    pub goal_x: Cell<Option<(Caret, f64)>>,
    /// The caret as last shown: x, top and bottom of its line box, or none.
    pub shown_caret: Cell<Option<(f32, f32, f32)>>,
    /// The text stays tilted while editing (Settings › General): the note washes every selection
    /// itself, tilted, and the field's own wash is off.
    pub tilted: Cell<bool>,
    pub pointer: Cell<PointerState>,
    pub typing: Cell<Option<(Caret, Flags)>>,
    pub typing_paint: Cell<Option<(Caret, Paint)>>,
    pub paper: Cell<(f64, f64)>,
    pub scale_factor: Cell<f64>,
    pub grip: RefCell<Option<(usize, Document)>>,
    pub right_click: Cell<Option<(f32, f32)>>,

    // Timers
    pub history_timer: Rc<Timer>,
    pub autoscroll: Rc<Timer>,

    // Bound UI reference & change callback
    pub ui: RefCell<Option<slint::Weak<NoteWindow>>>,
    pub on_changed: RefCell<Option<Rc<dyn Fn(Option<Caret>)>>>,
    pub clock: RefCell<Rc<dyn Clock>>,
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

    /// Bind this editor engine to a Slint NoteWindow component, wiring all models and callbacks.
    pub fn bind_ui(
        self: &Rc<Self>,
        ui: &NoteWindow,
        on_changed: Rc<dyn Fn(Option<Caret>)>,
        on_pointer_down: Option<Rc<dyn Fn()>>,
        on_open_image: Option<Rc<dyn Fn(usize)>>,
        on_paste_image: Option<Rc<dyn Fn() -> bool>>,
    ) {
        *self.ui.borrow_mut() = Some(ui.as_weak());
        *self.on_changed.borrow_mut() = Some(on_changed.clone());

        // Initialize PickerPalette entries
        let entries: Vec<PaletteEntry> = PALETTE
            .iter()
            .map(|s| PaletteEntry {
                name: s.name.into(),
                ink: rgb(s.ink),
                marker: swatch_marker(s.marker),
            })
            .collect();
        ui.global::<PickerPalette>().set_entries(slint::ModelRc::from(Rc::new(VecModel::from(entries))));

        // Attach Slint VecModels
        ui.set_blocks(slint::ModelRc::from(self.model.clone()));
        ui.set_runs(slint::ModelRc::from(self.runs.clone()));
        ui.set_rules(slint::ModelRc::from(self.rules.clone()));
        ui.set_sel_rects(slint::ModelRc::from(self.sel_rects.clone()));
        ui.set_image_handles(slint::ModelRc::from(self.handles.clone()));
        ui.set_markers(slint::ModelRc::from(self.marker_data.clone()));
        ui.set_marker_wash(slint::ModelRc::from(self.marker_wash.clone()));

        self.tilted.set(self.db.tilted_editing());
        ui.set_tilted_editing(self.tilted.get());
        self.relayout(ui);
        self.show_formats(ui);
        ui.set_text(self.text().as_str().into());

        // Helper closure to trigger change
        let editor_weak = Rc::downgrade(self);
        let changed = {
            let weak_ui = ui.as_weak();
            let editor_weak = editor_weak.clone();
            let on_changed = on_changed.clone();
            Rc::new(move |focus: Option<Caret>| {
                if let (Some(editor), Some(ui)) = (editor_weak.upgrade(), weak_ui.upgrade()) {
                    editor.apply_change(&ui, focus);
                }
                on_changed(focus);
            })
        };

        // Wire on_block_edited
        {
            let changed = changed.clone();
            let editor_weak = editor_weak.clone();
            ui.on_block_edited(move |i, block_text, cursor| {
                let Some(editor) = editor_weak.upgrade() else { return };
                let caret = editor.edit(i.max(0) as usize, block_text.as_str(), cursor.max(0) as usize);
                if caret.is_none() {
                    // typing: snapshot when it pauses
                    let weak_editor = editor_weak.clone();
                    editor.history_timer.start(TimerMode::SingleShot, HISTORY_PAUSE, move || {
                        if let Some(ed) = weak_editor.upgrade() {
                            ed.checkpoint();
                        }
                    });
                }
                changed(caret);
            });
        }

        // Wire on_format
        {
            let changed = changed.clone();
            let editor_weak = editor_weak.clone();
            let weak_ui = ui.as_weak();
            ui.on_format(move |what| {
                let (Some(editor), Some(ui)) = (editor_weak.upgrade(), weak_ui.upgrade()) else { return };
                if editor.format(what) {
                    changed(None);
                } else {
                    editor.show_formats(&ui);
                }
            });
        }

        // Wire color popover actions: apply ink / marker
        let apply: Rc<dyn Fn(Option<u8>, Option<u8>)> = {
            let changed = changed.clone();
            let editor_weak = editor_weak.clone();
            let weak_ui = ui.as_weak();
            Rc::new(move |ink, marker| {
                let (Some(editor), Some(ui)) = (editor_weak.upgrade(), weak_ui.upgrade()) else { return };
                if editor.apply_paint(ink, marker) {
                    changed(None);
                } else {
                    editor.show_formats(&ui);
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
        {
            let apply = apply.clone();
            ui.on_clear_color(move || apply(Some(0), Some(0)));
        }

        // Wire on_focus_entered
        {
            let editor_weak = editor_weak.clone();
            let weak_ui = ui.as_weak();
            ui.on_focus_entered(move |i| {
                let (Some(editor), Some(ui)) = (editor_weak.upgrade(), weak_ui.upgrade()) else { return };
                let block = i.max(0) as usize;
                if editor.selection.get().focus.block != block {
                    editor.set_caret(Caret { block, offset: 0 });
                }
                editor.show_formats(&ui);
            });
        }

        // Wire on_block_key
        {
            let changed = changed.clone();
            let editor_weak = editor_weak.clone();
            let weak_ui = ui.as_weak();
            ui.on_block_key(move |i, key, offset| {
                let (Some(editor), Some(ui)) = (editor_weak.upgrade(), weak_ui.upgrade()) else { return false };
                if key == UNDO_KEY || key == REDO_KEY {
                    if let Some(caret) = editor.step(key == REDO_KEY) {
                        changed(Some(caret));
                    }
                    return true;
                }
                if key.contains("shift+") {
                    return editor.extend(&ui, key.as_str());
                }
                let Some((caret, edited)) = editor.key(i.max(0) as usize, key.as_str(), offset.max(0) as usize, &ui) else { return false };
                if edited {
                    changed(Some(caret));
                } else {
                    // the field reports its caret only when the offset changes: the caret's side of
                    // a wrap can change without it
                    editor.selection.set(Range { anchor: caret, focus: caret });
                    editor.focus_at(&ui, caret);
                    editor.show_caret(&ui);
                }
                true
            });
        }

        // Wire on_select_all
        {
            let editor_weak = editor_weak.clone();
            let weak_ui = ui.as_weak();
            ui.on_select_all(move || {
                let (Some(editor), Some(ui)) = (editor_weak.upgrade(), weak_ui.upgrade()) else { return };
                editor.select_all(&ui);
            });
        }

        // Wire on_range_text
        {
            let editor_weak = editor_weak.clone();
            ui.on_range_text(move || {
                editor_weak.upgrade().map(|e| e.range_text().into()).unwrap_or_default()
            });
        }

        // Wire on_copy_range
        {
            let editor_weak = editor_weak.clone();
            ui.on_copy_range(move || {
                let Some(editor) = editor_weak.upgrade() else { return false };
                editor.copy_range(&editor.clipboard)
            });
        }

        // Wire on_cut_range
        {
            let changed = changed.clone();
            let editor_weak = editor_weak.clone();
            ui.on_cut_range(move || {
                let Some(editor) = editor_weak.upgrade() else { return false };
                if let Some(caret) = editor.cut_range(&editor.clipboard) {
                    changed(Some(caret));
                    true
                } else {
                    false
                }
            });
        }

        // Wire on_range_replace
        {
            let changed = changed.clone();
            let editor_weak = editor_weak.clone();
            ui.on_range_replace(move |text| {
                let Some(editor) = editor_weak.upgrade() else { return };
                let caret = editor.replace_range(text.as_str());
                changed(Some(caret));
            });
        }

        // Wire on_range_key
        {
            let changed = changed.clone();
            let editor_weak = editor_weak.clone();
            let weak_ui = ui.as_weak();
            ui.on_range_key(move |key, shift| {
                let (Some(editor), Some(ui)) = (editor_weak.upgrade(), weak_ui.upgrade()) else { return 0 };
                let (taken, caret) = editor.range_key(&ui, key.as_str(), shift);
                if let Some(c) = caret {
                    changed(Some(c));
                }
                taken
            });
        }

        // Wire on_caret_moved
        {
            let editor_weak = editor_weak.clone();
            let weak_ui = ui.as_weak();
            ui.on_caret_moved(move |block, offset, anchor| {
                let (Some(editor), Some(ui)) = (editor_weak.upgrade(), weak_ui.upgrade()) else { return };
                let (block, offset) = (block.max(0) as usize, offset.max(0) as usize);
                editor.caret_moved(&ui, block, offset, anchor.max(0) as usize);
                editor.show_formats(&ui);
                editor.reveal(&ui, block, offset);
            });
        }

        // Wire on_paper_resized
        {
            let editor_weak = editor_weak.clone();
            let weak_ui = ui.as_weak();
            ui.on_paper_resized(move |w, h| {
                let (Some(editor), Some(ui)) = (editor_weak.upgrade(), weak_ui.upgrade()) else { return };
                editor.paper.set((w as f64, h as f64));
                editor.relayout(&ui);
            });
        }

        // Wire on_text_right_click
        {
            let editor_weak = editor_weak.clone();
            ui.on_text_right_click(move |x, y| {
                if let Some(editor) = editor_weak.upgrade() {
                    editor.right_click.set(Some((x, y)));
                }
            });
        }

        // Wire on_pointer_down
        {
            let editor_weak = editor_weak.clone();
            let weak_ui = ui.as_weak();
            let on_pointer_down = on_pointer_down.clone();
            let on_open_image = on_open_image.clone();
            ui.on_pointer_down(move |x, y, shift| {
                let (Some(editor), Some(ui)) = (editor_weak.upgrade(), weak_ui.upgrade()) else { return };
                if let Some(cb) = on_pointer_down.as_ref() {
                    cb();
                }
                if editor.grab_grip(&ui, (x, y)) {
                    return;
                }
                let now = editor.monotonic_time();
                editor.pointer_down(&ui, (x, y), shift, now);
                if editor.pointer.get().clicks == 2 {
                    if let Some(block) = editor.image_at(&ui, (x, y)) {
                        if let Some(cb) = on_open_image.as_ref() {
                            cb(block);
                        }
                    }
                }
            });
        }

        // Wire on_pointer_drag
        {
            let changed = changed.clone();
            let editor_weak = editor_weak.clone();
            let weak_ui = ui.as_weak();
            let autoscroll = self.autoscroll.clone();
            ui.on_pointer_drag(move |x, y| {
                let (Some(editor), Some(ui)) = (editor_weak.upgrade(), weak_ui.upgrade()) else { return };
                if let Some(resized) = editor.grip_drag(&ui, (x, y)) {
                    if resized {
                        changed(None);
                    }
                    return;
                }
                if editor.drag_to(&ui, (x, y)) {
                    if !autoscroll.running() {
                        let weak_editor = editor_weak.clone();
                        let weak_ui = weak_ui.clone();
                        let timer_weak = Rc::downgrade(&autoscroll);
                        autoscroll.start(TimerMode::Repeated, AUTOSCROLL_STEP, move || {
                            let more = match (weak_editor.upgrade(), weak_ui.upgrade()) {
                                (Some(ed), Some(u)) => ed.autoscroll(&u),
                                _ => false,
                            };
                            if !more {
                                if let Some(t) = timer_weak.upgrade() {
                                    t.stop();
                                }
                            }
                        });
                    }
                } else {
                    autoscroll.stop();
                }
            });
        }

        // Wire on_pointer_up
        {
            let editor_weak = editor_weak.clone();
            let autoscroll = self.autoscroll.clone();
            ui.on_pointer_up(move || {
                if let Some(editor) = editor_weak.upgrade() {
                    editor.release_grip();
                    editor.pointer_up();
                }
                autoscroll.stop();
            });
        }

        // Wire on_pointer_motion
        {
            let editor_weak = editor_weak.clone();
            let weak_ui = ui.as_weak();
            ui.on_pointer_motion(move |x, y| {
                let (Some(editor), Some(ui)) = (editor_weak.upgrade(), weak_ui.upgrade()) else { return };
                let kind = editor.cursor_kind_at(&ui, (x, y));
                if ui.get_text_cursor_kind() != kind {
                    ui.set_text_cursor_kind(kind);
                }
            });
        }

        // Paste clipboard handler
        let paste_clipboard_fn = {
            let editor_weak = editor_weak.clone();
            let changed = changed.clone();
            Rc::new(move || -> bool {
                let Some(editor) = editor_weak.upgrade() else { return false };
                if editor.paste_rich_or_image() {
                    return true;
                }
                if let Some(text) = editor.clipboard.text() {
                    if text.contains('\n') || text.contains('\r') || editor.has_range() {
                        let caret = editor.replace_range(&text);
                        changed(Some(caret));
                        return true;
                    }
                }
                false
            })
        };

        ui.on_paste_range({
            let paste_clipboard_fn = paste_clipboard_fn.clone();
            move || paste_clipboard_fn()
        });

        {
            let paste_clipboard_fn = paste_clipboard_fn.clone();
            let on_paste_image = on_paste_image.clone();
            ui.on_paste_image(move || {
                if paste_clipboard_fn() {
                    return true;
                }
                if let Some(cb) = on_paste_image.as_ref() {
                    return cb();
                }
                false
            });
        }

        // Wire on_copy_image
        {
            let editor_weak = editor_weak.clone();
            ui.on_copy_image(move |i| {
                let Some(editor) = editor_weak.upgrade() else { return false };
                editor.copy_image(i.max(0) as usize)
            });
        }

        // Wire on_cut_image
        {
            let editor_weak = editor_weak.clone();
            ui.on_cut_image(move |i| {
                let Some(editor) = editor_weak.upgrade() else { return false };
                editor.cut_image(i.max(0) as usize).is_some()
            });
        }
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

    // --- Textflow Layout & Stepped Line Geometry ---

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
        }
    }

    pub fn relayout(&self, ui: &NoteWindow) {
        ui.set_pitch_scale(ui.window().scale_factor());
        let tokens = self.tokens(ui);
        let doc = self.doc.borrow();
        let (mut runs, rules, markers) = textflow::layout_all_with_images(doc.blocks(), self.paper.get(), &tokens, &|b| self.images.size(b));
        ui.set_content_height(textflow::extent_with_images(doc.blocks(), self.paper.get().0, &tokens, &|b| self.images.size(b)).1 as f32);
        self.rules.set_vec(
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
                })
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
        drop(doc);
        self.show_caret(ui);
    }

    /// Tell the note where to draw the caret: the box of its line in the app's own layout, or
    /// nowhere while something is selected or a picture has the focus.
    pub fn show_caret(&self, ui: &NoteWindow) {
        let sel = self.selection.get();
        let at = if sel.anchor == sel.focus && self.range.get().is_none() {
            let doc = self.doc.borrow();
            let c = sel.focus;
            textflow::caret_box(doc.blocks(), c.block, c.offset, self.line_end.get() == Some(c), self.paper.get().0, &self.tokens(ui), &|b| self.images.size(b))
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

    // --- ADR-0002 Note-Level Selection Wash Rects ---

    pub fn selection(&self) -> Range {
        self.selection.get()
    }

    /// Text now stays tilted while editing (true) or settles flat (false).
    pub fn set_tilted(&self, ui: &NoteWindow, on: bool) {
        self.tilted.set(on);
        ui.set_tilted_editing(on);
        if let Ok(doc) = self.doc.try_borrow() {
            self.fill_sel_rects(&doc, &self.tokens(ui));
        }
    }

    /// The selection the note washes itself: one across blocks always, and with tilted editing one
    /// inside a block too (the field's level wash is off then).
    fn washed(&self) -> Option<Range> {
        self.range.get().or_else(|| {
            let s = self.selection.get();
            (self.tilted.get() && s.anchor != s.focus).then_some(s)
        })
    }

    pub fn has_range(&self) -> bool {
        self.range.get().is_some()
    }

    pub fn set_selection(&self, ui: &NoteWindow, range: Range) {
        self.select(ui, range, false);
    }

    /// Make `range` the selection. With `line_end` (a caret only), the caret sits at the end of a
    /// line whose next line starts at the same offset.
    pub fn select(&self, ui: &NoteWindow, range: Range, line_end: bool) {
        let range = self.clamp(range);
        self.line_end.set((line_end && range.anchor == range.focus).then_some(range.focus));
        self.goal_x.set(None);
        let (start, end) = range.ordered();
        let across = start.block != end.block;
        self.selection.set(range);
        ui.set_wash(!across);
        if across {
            self.range.set(Some(range));
            self.fill_sel_rects(&self.doc.borrow(), &self.tokens(ui));
            self.focus_with(ui, range.focus, None);
        } else {
            self.range.set(None);
            // empty unless tilted editing washes a selection inside this block
            self.fill_sel_rects(&self.doc.borrow(), &self.tokens(ui));
            self.focus_with(ui, range.focus, Some(range.anchor.offset));
        }
        self.typing.set(None);
        self.typing_paint.set(None);
        self.show_formats(ui);
        self.show_caret(ui);
    }

    pub fn select_all(&self, ui: &NoteWindow) {
        let all = self.doc.borrow().all();
        self.set_selection(ui, all);
    }

    pub fn end_range(&self, ui: &NoteWindow) {
        self.range.set(None);
        self.sel_rects.set_vec(vec![]);
        self.marker_wash.set_vec(vec![]);
        ui.set_wash(true);
    }

    pub fn clamp(&self, range: Range) -> Range {
        let doc = self.doc.borrow();
        let fix = |c: Caret| {
            let block = c.block.min(doc.len().saturating_sub(1));
            let text = doc.blocks().get(block).map_or("", |b| b.text.as_str());
            Caret { block, offset: floor_boundary(text, c.offset) }
        };
        Range { anchor: fix(range.anchor), focus: fix(range.focus) }
    }

    pub fn fill_sel_rects(&self, doc: &Document, tokens: &Tokens) {
        let rects = match self.washed() {
            Some(range) if !doc.is_empty() => textflow::selection_rects(doc.blocks(), range, self.paper.get().0, tokens, &|b| self.images.size(b)),
            _ => vec![],
        };
        let rect = |r: &textflow::Rect| SelRect { x: r.x as f32, y: r.y as f32, width: r.width as f32, height: r.height as f32 };
        self.sel_rects.set_vec(rects.iter().map(rect).collect::<Vec<_>>());
        self.marker_wash.set_vec(textflow::marker_wash(&self.markers.borrow(), &rects).iter().map(rect).collect::<Vec<_>>());
        self.fill_handles(doc, tokens);
    }

    pub fn extend(&self, ui: &NoteWindow, key: &str) -> bool {
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
                block_boundary_caret(&doc, focus.block - 1, false)
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
                block_boundary_caret(&doc, focus.block + 1, true)
            } else {
                focus
            }
        } else {
            let up = is(Key::UpArrow);
            let tokens = self.tokens(ui);
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
                    None => Caret { block: focus.block, offset: if up { 0 } else { block.text.len() } },
                }
            }
        };
        drop(doc);
        self.set_selection(ui, Range { anchor: sel.anchor, focus: to });
        true
    }

    pub fn range_key(&self, ui: &NoteWindow, key: &str, shift: bool) -> (i32, Option<Caret>) {
        use slint::platform::Key;
        let mut chars = key.chars();
        let (Some(c), None) = (chars.next(), chars.next()) else { return (0, None) };
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

    pub fn replace_range(&self, text: &str) -> Caret {
        let range = self.clamp(self.selection.get());
        let before = self.doc.borrow().clone();
        let caret = self.doc.borrow_mut().replace_range(range, text);
        self.typing.set(None);
        self.typing_paint.set(None);
        self.structural(before);
        caret
    }

    pub fn range_text(&self) -> String {
        self.doc.borrow().plain_range(self.clamp(self.selection.get()))
    }

    pub fn hit(&self, ui: &NoteWindow, at: (f32, f32)) -> Caret {
        self.hit_on_line(ui, at).0
    }

    /// [`Self::hit`], and whether the caret is at the end of the line under the point where the
    /// next line starts at the same offset.
    pub fn hit_on_line(&self, ui: &NoteWindow, at: (f32, f32)) -> (Caret, bool) {
        let doc = self.doc.borrow();
        textflow::hit_on_line(doc.blocks(), (at.0 as f64, at.1 as f64), self.paper.get().0, &self.tokens(ui), &|b| self.images.size(b))
    }

    pub fn unit_at(&self, at: Caret, clicks: u8) -> Range {
        let doc = self.doc.borrow();
        let Some(b) = doc.blocks().get(at.block) else { return Range { anchor: at, focus: at } };
        let (start, end) = if clicks >= 3 {
            (0, b.text.len())
        } else {
            doc.word_at(at.block, at.offset).unwrap_or((at.offset, at.offset))
        };
        Range { anchor: Caret { block: at.block, offset: start }, focus: Caret { block: at.block, offset: end } }
    }

    pub fn caret_block(&self) -> usize {
        self.selection.get().focus.block.min(self.doc.borrow().len().saturating_sub(1))
    }

    pub fn set_caret(&self, c: Caret) {
        self.line_end.set(None);
        self.goal_x.set(None);
        self.selection.set(Range { anchor: c, focus: c });
    }

    pub fn caret_moved(&self, ui: &NoteWindow, block: usize, offset: usize, anchor: usize) {
        let now = self.selection.get();
        let (start, end) = now.ordered();
        if start.block != end.block && now.focus == (Caret { block, offset }) && anchor == offset {
            return;
        }
        let caret = Caret { block, offset };
        self.selection.set(Range { anchor: Caret { block, offset: anchor }, focus: caret });
        // the caret is where the note put it: it keeps its side of a wrap and its column
        if anchor != offset || self.line_end.get().is_some_and(|c| c != caret) {
            self.line_end.set(None);
        }
        if anchor != offset || self.goal_x.get().is_some_and(|(c, _)| c != caret) {
            self.goal_x.set(None);
        }
        if self.typing.get().is_some_and(|(c, _)| c != (Caret { block, offset }) || offset != anchor) {
            self.typing.set(None);
        }
        if self.typing_paint.get().is_some_and(|(c, _)| c != (Caret { block, offset }) || offset != anchor) {
            self.typing_paint.set(None);
        }
        if self.range.get().is_some() {
            self.end_range(ui);
        }
        // a selection the field made (Shift+arrows) is washed by the note too when tilted
        if self.tilted.get() {
            if let Ok(doc) = self.doc.try_borrow() {
                self.fill_sel_rects(&doc, &self.tokens(ui));
            }
        }
        self.show_caret(ui);
    }

    pub fn focus_at(&self, ui: &NoteWindow, c: Caret) {
        self.focus_with(ui, c, None);
    }

    pub fn focus_with(&self, ui: &NoteWindow, c: Caret, anchor: Option<usize>) {
        ui.set_focus_block(c.block as i32);
        ui.set_focus_anchor(anchor.map_or(-1, |a| a as i32));
        ui.set_focus_offset(c.offset as i32);
        ui.set_focus_serial(ui.get_focus_serial() + 1);
        self.reveal(ui, c.block, c.offset);
    }

    pub fn focus_start(&self, ui: &NoteWindow) {
        self.set_caret(START);
        self.focus_at(ui, START);
    }

    pub fn reveal(&self, ui: &NoteWindow, block: usize, offset: usize) {
        let tokens = self.tokens(ui);
        let doc = self.doc.borrow();
        let line_end = self.line_end.get() == Some(Caret { block, offset });
        let span = match textflow::caret_box(doc.blocks(), block, offset, line_end, self.paper.get().0, &tokens, &|b| self.images.size(b)) {
            Some((_, top, bottom)) => Some((top, bottom)),
            None => textflow::caret_span_with_images(doc.blocks(), block, offset, self.paper.get().0, &tokens, &|b| self.images.size(b)),
        };
        let Some((top, bottom)) = span else { return };
        let (_, content) = textflow::extent_with_images(doc.blocks(), self.paper.get().0, &tokens, &|b| self.images.size(b));
        let view = ui.get_text_view_height() as f64;
        let cur = ui.get_scroll_y() as f64;
        let mut to = if top <= tokens.inset_top + 0.5 { 0.0 } else if top < cur { top } else if bottom > cur + view { bottom - view } else { cur };
        to = to.min((content - view).max(0.0)).max(0.0);
        if (to - cur).abs() > 0.01 {
            ui.set_scroll_y(to as f32);
        }
    }

    // --- Text Editing & Formatting ---

    pub fn edit(&self, i: usize, text: &str, cursor: usize) -> Option<Caret> {
        if !text.contains(['\n', '\r']) {
            let before = self.doc.borrow().clone();
            let mut doc = self.doc.borrow_mut();
            let old = doc.blocks().get(i).map(|b| b.text.clone()).unwrap_or_default();
            let typing = self.typing.take().filter(|(c, _)| c.block == i).map(|(_, s)| s);
            let paint = self.typing_paint.take().filter(|(c, _)| c.block == i).map(|(_, p)| p);
            doc.edit_text(i, text, cursor, typing, paint);
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

    pub fn key(&self, i: usize, key: &str, offset: usize, ui: &NoteWindow) -> Option<(Caret, bool)> {
        use slint::platform::Key;
        let before = self.doc.borrow().clone();
        if before.blocks().get(i).is_some_and(|b| b.kind == Kind::Image) {
            return self.image_key(i, key, before);
        }
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
        if is(Key::LeftArrow) {
            if offset > 0 {
                return None;
            }
            if i > 0 {
                return Some((block_boundary_caret(&doc, i - 1, false), false));
            } else {
                return Some((Caret { block: 0, offset: 0 }, false));
            }
        }
        if is(Key::RightArrow) {
            let len = doc.blocks().get(i).map_or(0, |b| b.text.len());
            if offset < len {
                return None;
            }
            if i + 1 < doc.len() {
                return Some((block_boundary_caret(&doc, i + 1, true), false));
            } else {
                return Some((Caret { block: i, offset: len }, false));
            }
        }
        let up = is(Key::UpArrow);
        let (home, end) = (is(Key::Home), is(Key::End));
        if !up && !is(Key::DownArrow) && !home && !end {
            return None;
        }
        // The lines are the note's own (the field knows only a byte offset, which where a line
        // wraps without a space is both the end of one line and the start of the next).
        let here = Caret { block: i, offset };
        let tokens = self.tokens(ui);
        let width = self.paper.get().0;
        let block = doc.blocks().get(i)?;
        let at = textflow::caret_line_on(block, offset, self.line_end.get() == Some(here), width, &tokens);
        // where the caret lands: the block, the line in it and the x on that line
        let land = |target: usize, line: usize, x: f64| {
            let (to, line_end) = textflow::offset_on_line(&doc.blocks()[target], line, x, width, &tokens);
            let c = Caret { block: target, offset: to };
            self.line_end.set(line_end.then_some(c));
            c
        };
        if home || end {
            // past the end of any line: the paper's own width
            let c = land(i, at.line, if home { 0.0 } else { width });
            self.goal_x.set(None);
            return Some((c, false));
        }
        let x = self.goal_x.get().filter(|(c, _)| *c == here).map_or(at.x, |(_, x)| x);
        let c = if up {
            if at.line > 0 {
                land(i, at.line - 1, x)
            } else if i > 0 && doc.blocks()[i - 1].kind == Kind::Image {
                Caret { block: i - 1, offset: 0 }
            } else {
                land(doc.prev_text_block(i)?, usize::MAX, x)
            }
        } else if at.line + 1 < at.lines {
            land(i, at.line + 1, x)
        } else if i + 1 < doc.len() && doc.blocks()[i + 1].kind == Kind::Image {
            Caret { block: i + 1, offset: 0 }
        } else {
            land(doc.next_text_block(i)?, 0, x)
        };
        self.goal_x.set(Some((c, x)));
        Some((c, false))
    }

    pub fn typing_style(&self) -> Flags {
        let i = self.caret_block();
        let cursor = self.selection.get().focus.offset;
        match self.typing.get() {
            Some((c, style)) if c == (Caret { block: i, offset: cursor }) => style,
            _ => self.doc.borrow().blocks().get(i).map_or(Flags::NONE, |b| b.style_for_insert(cursor)),
        }
    }

    pub fn paint_at_caret(&self) -> Paint {
        let i = self.caret_block();
        let cursor = self.selection.get().focus.offset;
        match self.typing_paint.get() {
            Some((c, paint)) if c == (Caret { block: i, offset: cursor }) => paint,
            _ => self.doc.borrow().blocks().get(i).map_or(Paint::default(), |b| b.paint_for_insert(cursor)),
        }
    }

    pub fn format(&self, what: i32) -> bool {
        let i = self.caret_block();
        let flag = match what {
            FORMAT_BOLD => Flags::BOLD,
            FORMAT_ITALIC => Flags::ITALIC,
            FORMAT_UNDERLINE => Flags::UNDERLINE,
            FORMAT_STRIKE => Flags::STRIKE,
            FORMAT_LIST => {
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
            } else if let Some((start, end)) = doc.word_at(i, cursor).filter(|&(start, end)| cursor > start && cursor < end) {
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

    pub fn apply_paint(&self, ink: Option<u8>, marker: Option<u8>) -> bool {
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

    pub fn show_formats(&self, ui: &NoteWindow) {
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
        self.fill_handles(&doc, &self.tokens(ui));
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

    // --- Image Handling & Resize Grips ---

    pub fn selected_image(&self, doc: &Document) -> Option<usize> {
        let r = self.selection.get();
        (r.anchor == r.focus && doc.blocks().get(r.focus.block).is_some_and(|b| b.kind == Kind::Image)).then_some(r.focus.block)
    }

    pub fn grip_rect(b: &Block, r: textflow::Rect) -> textflow::Rect {
        let x = if b.align == Align::Right { r.x } else { r.x + r.width - GRIP };
        textflow::Rect { x, y: r.y + r.height - GRIP, width: GRIP, height: GRIP }
    }

    pub fn fill_handles(&self, doc: &Document, tokens: &Tokens) {
        let rows = self
            .selected_image(doc)
            .and_then(|i| {
                let r = textflow::image_rect(doc.blocks(), i, self.paper.get().0, tokens, &|b| self.images.size(b))?;
                Some([r, Self::grip_rect(&doc.blocks()[i], r)])
            })
            .map(|rs| rs.map(|r| SelRect { x: r.x as f32, y: r.y as f32, width: r.width as f32, height: r.height as f32 }).to_vec())
            .unwrap_or_default();
        self.handles.set_vec(rows);
    }

    pub fn grab_grip(&self, ui: &NoteWindow, at: (f32, f32)) -> bool {
        if !self.grip_at(ui, at) {
            return false;
        }
        let doc = self.doc.borrow();
        let Some(i) = self.selected_image(&doc) else { return false };
        *self.grip.borrow_mut() = Some((i, doc.clone()));
        true
    }

    pub fn grip_drag(&self, ui: &NoteWindow, at: (f32, f32)) -> Option<bool> {
        let block = self.grip.borrow().as_ref()?.0;
        let t = self.tokens(ui);
        let avail = self.paper.get().0 - t.inset_left - t.inset_right;
        let x = at.0 as f64;
        let mut doc = self.doc.borrow_mut();
        let w = match doc.blocks().get(block)?.align {
            Align::Left => x - t.inset_left,
            Align::Center => 2.0 * (x - (t.inset_left + avail / 2.0)),
            Align::Right => t.inset_left + avail - x,
        };
        let pct = (w / avail * 100.0).round().clamp(IMAGE_PCT_MIN as f64, IMAGE_PCT_MAX as f64) as u8;
        Some(doc.set_image_pct(block, pct))
    }

    pub fn release_grip(&self) -> bool {
        let Some((_, before)) = self.grip.borrow_mut().take() else { return false };
        if *self.doc.borrow() != before {
            self.structural(before);
        }
        true
    }

    pub fn image_at(&self, ui: &NoteWindow, at: (f32, f32)) -> Option<usize> {
        let block = self.hit(ui, at).block;
        let doc = self.doc.borrow();
        let r = textflow::image_rect(doc.blocks(), block, self.paper.get().0, &self.tokens(ui), &|b| self.images.size(b))?;
        let (x, y) = (at.0 as f64, at.1 as f64);
        (x >= r.x && x <= r.x + r.width && y >= r.y && y <= r.y + r.height).then_some(block)
    }

    pub fn grip_at(&self, ui: &NoteWindow, at: (f32, f32)) -> bool {
        let doc = self.doc.borrow();
        let Some(i) = self.selected_image(&doc) else { return false };
        let Some(r) = textflow::image_rect(doc.blocks(), i, self.paper.get().0, &self.tokens(ui), &|b| self.images.size(b)) else { return false };
        let g = Self::grip_rect(&doc.blocks()[i], r);
        let slop = 6.0;
        let (x, y) = (at.0 as f64, at.1 as f64);
        !(x < g.x - slop || x > g.x + g.width + slop || y < g.y - slop || y > g.y + g.height + slop)
    }

    pub fn cursor_kind_at(&self, ui: &NoteWindow, at: (f32, f32)) -> i32 {
        if self.grip_at(ui, at) {
            return CURSOR_RESIZE;
        }
        if self.image_at(ui, at).is_some() {
            return CURSOR_DEFAULT;
        }
        CURSOR_TEXT
    }

    pub fn image_key(&self, i: usize, key: &str, before: Document) -> Option<(Caret, bool)> {
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
            if i > 0 {
                Some((block_boundary_caret(&doc, i - 1, false), false))
            } else {
                None
            }
        } else if is(Key::DownArrow) || is(Key::RightArrow) {
            if i + 1 < doc.len() {
                Some((block_boundary_caret(&doc, i + 1, true), false))
            } else {
                None
            }
        } else {
            None
        }
    }

    pub fn insert_position(&self, ui: &NoteWindow) -> usize {
        let last = self.doc.borrow().len().saturating_sub(1);
        let focused = ui.get_text_focused_block();
        let at = if focused >= 0 { focused } else { ui.get_last_focused_block() };
        if at < 0 { last } else { (at as usize).min(last) }
    }

    pub fn add_image(&self, stored: &Stored, after: usize) -> Result<Caret, DbError> {
        let image_id = self.db.add_image(self.id.get(), stored.mime, stored.width, stored.height, &stored.data)?;
        let pixels = crate::images::decode(&stored.data).ok().map(|p| crate::note_images::to_slint(&p));
        self.images.add(image_id, (stored.width, stored.height), pixels);
        let before = self.doc.borrow().clone();
        let at = self.doc.borrow_mut().insert_image(after, image_id);
        self.structural(before);
        let caret = Caret { block: at + 1, offset: 0 };
        self.notify_change(Some(caret));
        Ok(caret)
    }

    pub fn image_state(&self, block: usize) -> Option<(u8, Align)> {
        self.doc.borrow().blocks().get(block).filter(|b| b.kind == Kind::Image).map(|b| (b.image_pct, b.align))
    }

    pub fn edit_image(&self, edit: impl FnOnce(&mut Document) -> Option<Option<Caret>>) -> bool {
        let before = self.doc.borrow().clone();
        let Some(caret) = edit(&mut self.doc.borrow_mut()) else { return false };
        self.structural(before);
        self.notify_change(caret);
        true
    }

    pub fn set_image_pct(&self, block: usize, pct: u8) -> bool {
        self.edit_image(|d| d.set_image_pct(block, pct).then_some(None))
    }

    pub fn set_image_align(&self, block: usize, align: Align) -> bool {
        self.edit_image(|d| d.set_image_align(block, align).then_some(None))
    }

    pub fn delete_image(&self, block: usize) -> bool {
        self.edit_image(|d| d.delete_image(block).map(Some))
    }

    pub fn image_rgba(&self, block: usize) -> Option<Rgba> {
        let doc = self.doc.borrow();
        let b = doc.blocks().get(block).filter(|b| b.kind == Kind::Image)?;
        self.images.rgba(b)
    }

    pub fn copy_image(&self, block: usize) -> bool {
        let Some(rgba) = self.image_rgba(block) else { return false };
        self.clipboard.set_image(&rgba).is_ok()
    }

    pub fn cut_image(&self, block: usize) -> Option<Caret> {
        let doc = self.doc.borrow();
        let b = doc.blocks().get(block).filter(|b| b.kind == Kind::Image)?;
        let rgba = self.images.rgba(b)?;
        drop(doc);
        if self.clipboard.set_image(&rgba).is_err() {
            return None;
        }
        let before = self.doc.borrow().clone();
        let caret = self.doc.borrow_mut().delete_image(block)?;
        self.structural(before);
        self.notify_change(Some(caret));
        Some(caret)
    }

    pub fn picture_at(&self, block: usize) -> Option<(slint::Image, (u32, u32))> {
        let doc = self.doc.borrow();
        let b = doc.blocks().get(block)?;
        let size = self.images.size(b)?;
        let pixels = self.images.pixels(b);
        (pixels.size().width > 0).then_some((pixels, size))
    }

    pub fn decoded_image_bytes(&self) -> usize {
        self.images.decoded_bytes()
    }

    pub fn free_images(&self) {
        self.images.free();
        self.sync_model();
    }

    pub fn thaw_images(&self) {
        self.images.thaw(&self.doc.borrow());
        self.sync_model();
    }

    pub fn prune_images(&self) -> Result<(), DbError> {
        let keep: Vec<i64> = self.doc.borrow().blocks().iter().filter_map(|b| b.image_id).collect();
        self.db.prune_images(self.id.get(), &keep)
    }

    // --- Pointer & Autoscroll ---

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
        self.select(ui, range, line_end);
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

    // --- Undo / Redo History ---

    pub fn checkpoint(&self) {
        self.history.borrow_mut().record(self.doc.borrow().clone());
    }

    pub fn structural(&self, before: Document) {
        let mut h = self.history.borrow_mut();
        h.record(before);
        h.record(self.doc.borrow().clone());
    }

    pub fn step(&self, redo: bool) -> Option<Caret> {
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

    pub fn can_undo(&self) -> bool {
        self.history.borrow().can_undo()
    }

    pub fn can_redo(&self) -> bool {
        self.history.borrow().can_redo()
    }

    // --- Clipboard ---

    pub fn copy_range(&self, clipboard: &Rc<dyn ImageClipboard>) -> bool {
        let doc = self.doc.borrow();
        let range = self.selection.get();
        if let Some(i) = self.selected_image(&doc) {
            if let Some(b) = doc.blocks().get(i) {
                if let Some(rgba) = self.images.rgba(b) {
                    return clipboard.set_image(&rgba).is_ok();
                }
            }
        }
        let (plain, html) = crate::clipboard_html::to_html_and_text(doc.blocks(), range, &|b| self.images.rgba(b));
        clipboard.set_rich(crate::platform::ClipboardPayload {
            plain: Some(plain),
            html: Some(html),
            image: None,
        }).is_ok()
    }

    pub fn cut_range(&self, clipboard: &Rc<dyn ImageClipboard>) -> Option<Caret> {
        let doc = self.doc.borrow();
        if let Some(i) = self.selected_image(&doc) {
            if let Some(b) = doc.blocks().get(i) {
                if let Some(rgba) = self.images.rgba(b) {
                    let _ = clipboard.set_image(&rgba);
                    drop(doc);
                    let before = self.doc.borrow().clone();
                    let caret = self.doc.borrow_mut().delete_image(i)?;
                    self.structural(before);
                    return Some(caret);
                }
            }
        }
        drop(doc);
        if !self.copy_range(clipboard) {
            return None;
        }
        Some(self.replace_range(""))
    }

    pub fn paste_rich_or_image(&self) -> bool {
        use crate::clipboard_html::ParsedBlock;
        if let Some(html) = self.clipboard.html() {
            let parsed = crate::clipboard_html::from_html(&html);
            let is_plain_single_body = parsed.len() == 1
                && matches!(&parsed[0], ParsedBlock::Body { marks, paints, .. } if marks.iter().all(|m| *m == Flags::NONE) && paints.iter().all(|p| *p == Paint::default()));
            if !parsed.is_empty() && !is_plain_single_body {
                let before = self.doc.borrow().clone();
                let range = self.selection.get();
                let db = self.db.clone();
                let images = &self.images;
                let id = self.id.get();
                let mut add_image = |rgba: Rgba| -> Option<i64> {
                    let (w, h) = (rgba.width, rgba.height);
                    let png = crate::images::encode_png(&rgba).ok()?;
                    let image_id = db.add_image(id, "image/png", w, h, &png).ok()?;
                    let pixels = Some(crate::note_images::to_slint(&rgba));
                    images.add(image_id, (w, h), pixels);
                    Some(image_id)
                };
                let caret = self.doc.borrow_mut().replace_range_rich(range, parsed, &mut add_image);
                self.structural(before);
                self.notify_change(Some(caret));
                return true;
            }
        }
        if let Some(rgba) = self.clipboard.image() {
            let after = {
                let last = self.doc.borrow().len().saturating_sub(1);
                let range = self.selection.get();
                range.focus.block.min(last)
            };
            let (w, h) = (rgba.width, rgba.height);
            if let Ok(png) = crate::images::encode_png(&rgba) {
                if let Ok(image_id) = self.db.add_image(self.id.get(), "image/png", w, h, &png) {
                    let pixels = Some(crate::note_images::to_slint(&rgba));
                    self.images.add(image_id, (w, h), pixels);
                    let before = self.doc.borrow().clone();
                    let at = self.doc.borrow_mut().insert_image(after, image_id);
                    self.structural(before);
                    self.notify_change(Some(Caret { block: at + 1, offset: 0 }));
                    return true;
                }
            }
        }
        false
    }

    pub fn paste(&self) -> bool {
        if self.paste_rich_or_image() {
            return true;
        }
        if let Some(text) = self.clipboard.text() {
            if !text.is_empty() {
                let caret = self.replace_range(&text);
                self.notify_change(Some(caret));
                return true;
            }
        }
        false
    }

    pub fn paste_image(&self) -> bool {
        if let Some(rgba) = self.clipboard.image() {
            let after = {
                let last = self.doc.borrow().len().saturating_sub(1);
                let range = self.selection.get();
                range.focus.block.min(last)
            };
            let (w, h) = (rgba.width, rgba.height);
            if let Ok(png) = crate::images::encode_png(&rgba) {
                if let Ok(image_id) = self.db.add_image(self.id.get(), "image/png", w, h, &png) {
                    let pixels = Some(crate::note_images::to_slint(&rgba));
                    self.images.add(image_id, (w, h), pixels);
                    let before = self.doc.borrow().clone();
                    let at = self.doc.borrow_mut().insert_image(after, image_id);
                    self.structural(before);
                    self.notify_change(Some(Caret { block: at + 1, offset: 0 }));
                    return true;
                }
            }
        }
        false
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::{Caret, Range};
    use crate::platform::{ClipboardPayload, ImageClipboard};
    use crate::storage::Db;
    use slint::ComponentHandle;
    use std::cell::RefCell;
    use std::rc::Rc;
    use std::time::Duration;

    #[derive(Default)]
    struct DummyClipboard {
        payload: RefCell<ClipboardPayload>,
    }

    impl ImageClipboard for DummyClipboard {
        fn image(&self) -> Option<crate::images::Rgba> {
            self.payload.borrow().image.clone()
        }
        fn set_image(&self, img: &crate::images::Rgba) -> Result<(), String> {
            self.payload.borrow_mut().image = Some(img.clone());
            Ok(())
        }
        fn set_rich(&self, payload: ClipboardPayload) -> Result<(), String> {
            *self.payload.borrow_mut() = payload;
            Ok(())
        }
        fn html(&self) -> Option<String> {
            self.payload.borrow().html.clone()
        }
        fn text(&self) -> Option<String> {
            self.payload.borrow().plain.clone()
        }
    }

    fn test_db() -> (tempfile::TempDir, Rc<Db>) {
        let dir = tempfile::tempdir().expect("tempdir");
        let db = Db::open(&dir.path().join("test.db"), &[0x42; 32], 1000).expect("db open");
        (dir, Rc::new(db))
    }

    fn test_editor(content: &str) -> (tempfile::TempDir, Rc<Editor>) {
        let (dir, db) = test_db();
        let clipboard = Rc::new(DummyClipboard::default());
        let editor = Rc::new(Editor::new(1, content, db, clipboard));
        (dir, editor)
    }

    fn test_editor_with_ui(content: &str) -> (tempfile::TempDir, Rc<Editor>, NoteWindow) {
        crate::dropdown::init_test_platform();
        let (dir, editor) = test_editor(content);
        let ui = NoteWindow::new().expect("NoteWindow");
        ui.window().set_size(slint::LogicalSize::new(447.0, 468.0));
        let changed = Rc::new(|_| {});
        editor.bind_ui(&ui, changed, None, None, None);
        (dir, editor, ui)
    }

    #[test]
    fn test_editor_initialization_and_text_roundtrip() {
        let text = "# Shopping List\nMilk and eggs\n- Bread\n- Butter";
        let (_dir, editor) = test_editor(text);
        assert_eq!(editor.text(), text);
        assert_eq!(editor.model.row_count(), 4);
        assert_eq!(editor.model.row_data(0).unwrap().kind, BlockKind::Title);
        assert_eq!(editor.model.row_data(1).unwrap().kind, BlockKind::Body);
        assert_eq!(editor.model.row_data(2).unwrap().kind, BlockKind::ListItem);
        assert_eq!(editor.model.row_data(3).unwrap().kind, BlockKind::ListItem);
    }

    #[test]
    fn test_editor_undo_redo_history() {
        let (_dir, editor) = test_editor("Initial text");
        editor.checkpoint();
        editor.edit(0, "Modified text", 13);
        editor.checkpoint();
        assert_eq!(editor.text(), "# Modified text");

        // Undo steps back to initial state
        let caret = editor.step(false).expect("undo snapshot available");
        assert_eq!(editor.text(), "# Initial text");
        assert_eq!(caret.block, 0);

        // Redo steps forward to modified state
        let caret = editor.step(true).expect("redo snapshot available");
        assert_eq!(editor.text(), "# Modified text");
        assert_eq!(caret.block, 0);

        // Further redo is None
        assert!(editor.step(true).is_none());
    }

    #[test]
    fn test_editor_adr0002_selection_wash_rects() {
        let text = "# First Line\nSecond Line\nThird Line";
        let (_dir, editor, ui) = test_editor_with_ui(text);
        editor.relayout(&ui);

        // Select all creates cross-block selection wash
        editor.select_all(&ui);
        assert_eq!(editor.selection(), Range { anchor: Caret { block: 0, offset: 0 }, focus: Caret { block: 2, offset: 10 } });
        assert!(editor.sel_rects.row_count() >= 3, "one wash box per line across 3 blocks");
        assert!(!ui.get_wash(), "note-level wash active: individual field wash suppressed");

        // Single block selection clears note wash rects and restores field wash
        let single_range = Range {
            anchor: Caret { block: 1, offset: 0 },
            focus: Caret { block: 1, offset: 6 },
        };
        editor.set_tilted(&ui, false);
        editor.set_selection(&ui, single_range);
        assert_eq!(editor.selection(), Range { anchor: Caret { block: 1, offset: 0 }, focus: Caret { block: 1, offset: 6 } });
        assert_eq!(editor.sel_rects.row_count(), 0, "no note-level wash for single block");
        assert!(ui.get_wash(), "field wash restored for single block selection");

        // With tilted editing the note washes it too (the field's level wash is off): one box for the line
        editor.set_tilted(&ui, true);
        assert_eq!(editor.sel_rects.row_count(), 1, "tilted editing: the note washes a selection inside a block");
        assert!(ui.get_wash(), "it is still a selection inside one block");
        editor.set_selection(&ui, Range { anchor: Caret { block: 1, offset: 3 }, focus: Caret { block: 1, offset: 3 } });
        assert_eq!(editor.sel_rects.row_count(), 0, "a caret has no wash");
    }

    #[test]
    fn test_editor_multi_click_pointer_expansion() {
        let text = "# Title\nFirst word second\nThird line";
        let (_dir, editor, ui) = test_editor_with_ui(text);
        editor.relayout(&ui);

        // Hit point in the middle of "First" in block 1
        let r = editor.runs.iter().find(|r| r.block == 1).expect("block 1 run");
        let at = (r.x + r.width / 2.0, r.y + 4.0);

        // 1st click: collapsed caret
        editor.pointer_down(&ui, at, false, Duration::from_millis(100));
        let Range { anchor, focus } = editor.selection();
        assert_eq!(anchor, focus, "single click collapses caret");

        // 2nd click within 500ms: word selection
        editor.pointer_down(&ui, at, false, Duration::from_millis(250));
        let Range { anchor, focus } = editor.selection();
        assert_ne!(anchor, focus, "double click expands to word range");
        assert_eq!(anchor.block, 1, "anchor in block 1");
        assert_eq!(focus.block, 1, "focus in block 1");

        // 3rd click within 500ms: paragraph selection
        editor.pointer_down(&ui, at, false, Duration::from_millis(400));
        let Range { anchor, focus } = editor.selection();
        assert_eq!(anchor, Caret { block: 1, offset: 0 }, "triple click anchor at start of paragraph");
        assert_eq!(focus, Caret { block: 1, offset: "First word second".len() }, "triple click focus at end of paragraph");
    }

    #[test]
    fn test_double_click_word_selection_at_boundaries() {
        let text = "# Title\nFirst  second...third";
        let (_dir, editor, _ui) = test_editor_with_ui(text);

        // Block 1 text: "First  second...third"
        // "First" is 0..5
        // Beginning of "First" (offset 0)
        let r0 = editor.unit_at(Caret { block: 1, offset: 0 }, 2);
        assert_eq!(r0, Range { anchor: Caret { block: 1, offset: 0 }, focus: Caret { block: 1, offset: 5 } });

        // Trailing boundary of "First" before whitespace (offset 5)
        let r5 = editor.unit_at(Caret { block: 1, offset: 5 }, 2);
        assert_eq!(r5, Range { anchor: Caret { block: 1, offset: 0 }, focus: Caret { block: 1, offset: 5 } });

        // Double-clicking inside whitespace (offset 6) collapses
        let r6 = editor.unit_at(Caret { block: 1, offset: 6 }, 2);
        assert_eq!(r6, Range { anchor: Caret { block: 1, offset: 6 }, focus: Caret { block: 1, offset: 6 } });

        // Beginning of "second" (offset 7)
        let r7 = editor.unit_at(Caret { block: 1, offset: 7 }, 2);
        assert_eq!(r7, Range { anchor: Caret { block: 1, offset: 7 }, focus: Caret { block: 1, offset: 13 } });

        // Trailing boundary of "second" before punctuation (offset 13)
        let r13 = editor.unit_at(Caret { block: 1, offset: 13 }, 2);
        assert_eq!(r13, Range { anchor: Caret { block: 1, offset: 7 }, focus: Caret { block: 1, offset: 13 } });

        // Double-clicking inside punctuation (offset 14) collapses
        let r14 = editor.unit_at(Caret { block: 1, offset: 14 }, 2);
        assert_eq!(r14, Range { anchor: Caret { block: 1, offset: 14 }, focus: Caret { block: 1, offset: 14 } });

        // Trailing boundary of "third" at EOF (offset 21)
        let r21 = editor.unit_at(Caret { block: 1, offset: 21 }, 2);
        assert_eq!(r21, Range { anchor: Caret { block: 1, offset: 16 }, focus: Caret { block: 1, offset: 21 } });
    }

    #[test]
    fn test_formatting_toggle_unaffected_at_boundaries() {
        let (_dir, editor, ui) = test_editor_with_ui("title\nbody text");
        editor.relayout(&ui);

        // Caret collapsed at end of "body" (block 1, offset 4)
        editor.set_selection(&ui, Range {
            anchor: Caret { block: 1, offset: 4 },
            focus: Caret { block: 1, offset: 4 },
        });

        // Toggle bold when collapsed at word boundary does NOT style "body", applies to typing style
        assert!(!editor.format(0));
        assert_eq!(editor.text(), "# title\nbody text");
        assert!(editor.typing_style().contains(Flags::BOLD));

        // Caret collapsed inside "body" (block 1, offset 2) styles the word
        editor.set_selection(&ui, Range {
            anchor: Caret { block: 1, offset: 2 },
            focus: Caret { block: 1, offset: 2 },
        });
        assert!(editor.format(0));
        assert_eq!(editor.text(), "# title\n**body** text");
    }

    #[test]
    fn test_editor_image_resize_grip_and_drag_math() {
        let (dir, db) = test_db();
        let note_id = db.create_note(0).expect("create note");
        // Insert a test image into DB
        let flat_rgba = crate::images::Rgba {
            width: 200,
            height: 100,
            data: vec![255; 200 * 100 * 4],
        };
        let png = crate::images::encode_png(&flat_rgba).expect("encode png");
        let image_id = db.add_image(note_id, "image/png", 200, 100, &png).expect("add image");

        let content = format!("# Title\n![](img:{image_id})\nAfter text");
        crate::dropdown::init_test_platform();
        let clipboard = Rc::new(DummyClipboard::default());
        let editor = Rc::new(Editor::new(note_id, &content, db, clipboard));
        let ui = NoteWindow::new().expect("NoteWindow");
        ui.window().set_size(slint::LogicalSize::new(447.0, 468.0));
        editor.bind_ui(&ui, Rc::new(|_| {}), None, None, None);
        editor.relayout(&ui);

        // Select the image block (block index 1)
        let img_caret = Caret { block: 1, offset: 0 };
        editor.set_selection(&ui, Range { anchor: img_caret, focus: img_caret });

        // Handles model must contain 2 rects: image frame and resize grip
        assert_eq!(editor.handles.row_count(), 2, "selected image shows frame and grip");
        let grip_row = editor.handles.row_data(1).unwrap();
        let grip_center = (grip_row.x + grip_row.width / 2.0, grip_row.y + grip_row.height / 2.0);

        // Cursor kinds
        assert_eq!(editor.cursor_kind_at(&ui, grip_center), CURSOR_RESIZE);
        assert_eq!(editor.cursor_kind_at(&ui, (100.0, grip_row.y - 30.0)), CURSOR_DEFAULT);

        // Grab grip
        assert!(editor.grab_grip(&ui, grip_center));

        // Drag grip inwards to resize smaller
        let dragged = editor.grip_drag(&ui, (grip_center.0 - 50.0, grip_center.1));
        assert!(dragged.is_some());

        // Release grip commits undo checkpoint
        assert!(editor.release_grip());
        assert!(editor.step(false).is_some(), "undo steps back through grip drag");
        drop(dir);
    }

    #[test]
    fn test_editor_formatting_toggle() {
        let (_dir, editor, ui) = test_editor_with_ui("title\nbody text");
        editor.relayout(&ui);

        // Select "body" in block 1: offsets 0..4
        let range = Range {
            anchor: Caret { block: 1, offset: 0 },
            focus: Caret { block: 1, offset: 4 },
        };
        editor.set_selection(&ui, range);

        // Toggle Bold (0)
        assert!(editor.format(0));
        assert_eq!(editor.text(), "# title\n**body** text");

        // Toggle Italic (1)
        assert!(editor.format(1));
        assert_eq!(editor.text(), "# title\n***body*** text");

        // Toggle List (4)
        assert!(editor.format(4));
        assert!(editor.text().contains("- "));
    }

    #[test]
    fn test_editor_color_palette_and_marker_wash() {
        let (_dir, editor, ui) = test_editor_with_ui("# Header\nColored text here");
        editor.relayout(&ui);

        // Select "Colored" in block 1
        let range = Range {
            anchor: Caret { block: 1, offset: 0 },
            focus: Caret { block: 1, offset: 7 },
        };
        editor.set_selection(&ui, range);

        // Apply ink 1 and marker 2
        assert!(editor.apply_paint(Some(1), Some(2)));
        editor.relayout(&ui);

        // Check markers model is populated
        assert!(editor.marker_data.row_count() > 0, "marker wash created for highlighted text");

        // Select all to verify marker wash under selection
        editor.select_all(&ui);
        assert!(editor.marker_wash.row_count() > 0, "marker wash overlays tinted under selection");
    }

    #[test]
    fn test_editor_range_keys_replace_and_cut_copy() {
        let (_dir, editor, ui) = test_editor_with_ui("# Title\nFirst paragraph\nSecond paragraph");
        editor.relayout(&ui);

        // Cross-block selection from block 1 offset 6 to block 2 offset 6
        let range = Range {
            anchor: Caret { block: 1, offset: 6 },
            focus: Caret { block: 2, offset: 6 },
        };
        editor.set_selection(&ui, range);

        // Copy range
        assert!(editor.copy_range(&editor.clipboard));
        assert!(editor.clipboard.text().unwrap().contains("paragraph\nSecond"));

        // Replace range with key
        let (code, caret) = editor.range_key(&ui, "X", false);
        assert_eq!(code, 2, "range replaced");
        assert!(caret.is_some());
        assert_eq!(editor.text(), "# Title\nFirst X paragraph");

        // Undo restores original text
        editor.step(false);
        assert_eq!(editor.text(), "# Title\nFirst paragraph\nSecond paragraph");
    }

    #[test]
    fn test_editor_horizontal_arrow_paragraph_navigation() {
        use slint::platform::Key;
        let left_key = char::from(Key::LeftArrow).to_string();
        let right_key = char::from(Key::RightArrow).to_string();

        let (_dir, editor, ui) = test_editor_with_ui("Title\nParagraph 1\nParagraph 2");
        editor.relayout(&ui);

        // 1. Pressing Left Arrow at offset 0 of paragraph N (N > 0) moves focus and caret to the end of paragraph N-1.
        let res = editor.key(1, &left_key, 0, &ui);
        assert_eq!(res, Some((Caret { block: 0, offset: 5 }, false)), "Left arrow at start of paragraph 1 moves to end of paragraph 0");

        let res = editor.key(2, &left_key, 0, &ui);
        assert_eq!(res, Some((Caret { block: 1, offset: 11 }, false)), "Left arrow at start of paragraph 2 moves to end of paragraph 1");

        // 2. Pressing Left Arrow within a paragraph (offset > 0) returns None so Slint handles intra-paragraph navigation.
        let res = editor.key(1, &left_key, 5, &ui);
        assert_eq!(res, None, "Left arrow mid-paragraph returns None");

        // 3. Pressing Left Arrow at offset 0 of paragraph 0 remains at the start of the document without error.
        let res = editor.key(0, &left_key, 0, &ui);
        assert_eq!(res, Some((Caret { block: 0, offset: 0 }, false)), "Left arrow at start of block 0 stays at start");

        // 4. Pressing Right Arrow at the end of paragraph N moves focus and caret to offset 0 of paragraph N+1.
        let res = editor.key(0, &right_key, 5, &ui);
        assert_eq!(res, Some((Caret { block: 1, offset: 0 }, false)), "Right arrow at end of paragraph 0 moves to start of paragraph 1");

        let res = editor.key(1, &right_key, 11, &ui);
        assert_eq!(res, Some((Caret { block: 2, offset: 0 }, false)), "Right arrow at end of paragraph 1 moves to start of paragraph 2");

        // 5. Pressing Right Arrow within a paragraph (offset < len) returns None so Slint handles intra-paragraph navigation.
        let res = editor.key(1, &right_key, 5, &ui);
        assert_eq!(res, None, "Right arrow mid-paragraph returns None");

        // 6. Pressing Right Arrow at the end of the last paragraph remains at the end of the document without error.
        let res = editor.key(2, &right_key, 11, &ui);
        assert_eq!(res, Some((Caret { block: 2, offset: 11 }, false)), "Right arrow at end of last paragraph stays at end");
    }

    #[test]
    fn test_editor_multi_block_single_line_paste_with_clipboard_abstraction() {
        let (_dir, editor, ui) = test_editor_with_ui("Block 0\nBlock 1\nBlock 2");
        editor.relayout(&ui);

        // Cross-block selection from block 0 offset 3 to block 2 offset 3
        let range = Range {
            anchor: Caret { block: 0, offset: 3 },
            focus: Caret { block: 2, offset: 3 },
        };
        editor.set_selection(&ui, range);
        assert!(editor.range.get().is_some(), "multi-block range selection active");

        // Inject single-line plain text into clipboard abstraction (DummyClipboard)
        editor.clipboard.set_rich(ClipboardPayload {
            plain: Some("REPLACED".to_string()),
            ..Default::default()
        }).unwrap();

        // Invoke paste-range callback
        let handled = ui.invoke_paste_range();
        assert!(handled, "paste-range handled single-line clipboard content");

        // Selected range cleanly replaced
        assert_eq!(editor.text(), "# BloREPLACEDck 2");
        assert!(editor.range.get().is_none(), "range cleared after paste");

        // Undo restores original multi-block text
        editor.step(false);
        assert_eq!(editor.text(), "# Block 0\nBlock 1\nBlock 2");

        // Redo re-applies paste replacement
        editor.step(true);
        assert_eq!(editor.text(), "# BloREPLACEDck 2");
    }

    #[test]
    fn test_editor_arrow_navigation_onto_and_off_images() {
        use slint::platform::Key;
        let up_key = char::from(Key::UpArrow).to_string();
        let down_key = char::from(Key::DownArrow).to_string();
        let left_key = char::from(Key::LeftArrow).to_string();
        let right_key = char::from(Key::RightArrow).to_string();

        let (_dir, db) = test_db();
        let note_id = db.create_note(0).expect("create note");
        let flat_rgba = crate::images::Rgba {
            width: 200,
            height: 100,
            data: vec![255; 200 * 100 * 4],
        };
        let png = crate::images::encode_png(&flat_rgba).expect("encode png");
        let img1 = db.add_image(note_id, "image/png", 200, 100, &png).expect("add image 1");
        let img2 = db.add_image(note_id, "image/png", 200, 100, &png).expect("add image 2");

        let content = format!("First\n![](img:{img1})\nMiddle\n![](img:{img2})\nLast");
        crate::dropdown::init_test_platform();
        let clipboard = Rc::new(DummyClipboard::default());
        let editor = Rc::new(Editor::new(note_id, &content, db, clipboard));
        let ui = NoteWindow::new().expect("NoteWindow");
        ui.window().set_size(slint::LogicalSize::new(447.0, 468.0));
        editor.bind_ui(&ui, Rc::new(|_| {}), None, None, None);
        editor.relayout(&ui);

        // Document structure:
        // 0: "First" (text, len 5)
        // 1: img1 (image)
        // 2: "Middle" (text, len 6)
        // 3: img2 (image)
        // 4: "Last" (text, len 4)

        // 1. Down Arrow at bottom line of paragraph preceding image focuses image
        let res = editor.key(0, &down_key, 0, &ui);
        assert_eq!(res, Some((Caret { block: 1, offset: 0 }, false)), "Down arrow from block 0 focuses image 1");

        // 2. Right Arrow at end of paragraph preceding image focuses image
        let res = editor.key(0, &right_key, 5, &ui);
        assert_eq!(res, Some((Caret { block: 1, offset: 0 }, false)), "Right arrow from end of block 0 focuses image 1");

        // 3. Up Arrow at top line of paragraph following image focuses image
        let res = editor.key(2, &up_key, 3, &ui);
        assert_eq!(res, Some((Caret { block: 1, offset: 0 }, false)), "Up arrow from block 2 focuses image 1");

        // 4. Left Arrow at offset 0 of paragraph immediately after image focuses image
        let res = editor.key(2, &left_key, 0, &ui);
        assert_eq!(res, Some((Caret { block: 1, offset: 0 }, false)), "Left arrow from start of block 2 focuses image 1");

        // 5. Exiting image 1 backwards (Up / Left) lands at end of block 0
        let res = editor.key(1, &up_key, 0, &ui);
        assert_eq!(res, Some((Caret { block: 0, offset: 5 }, false)), "Up arrow on image 1 lands at end of block 0");
        let res = editor.key(1, &left_key, 0, &ui);
        assert_eq!(res, Some((Caret { block: 0, offset: 5 }, false)), "Left arrow on image 1 lands at end of block 0");

        // 6. Exiting image 1 forwards (Down / Right) lands at offset 0 of block 2
        let res = editor.key(1, &down_key, 0, &ui);
        assert_eq!(res, Some((Caret { block: 2, offset: 0 }, false)), "Down arrow on image 1 lands at start of block 2");
        let res = editor.key(1, &right_key, 0, &ui);
        assert_eq!(res, Some((Caret { block: 2, offset: 0 }, false)), "Right arrow on image 1 lands at start of block 2");

        // Test navigation between adjacent image blocks
        let (_dir2, db2) = test_db();
        let note_id2 = db2.create_note(0).expect("create note 2");
        let img1_b = db2.add_image(note_id2, "image/png", 200, 100, &png).expect("add img");
        let img2_b = db2.add_image(note_id2, "image/png", 200, 100, &png).expect("add img");
        let content2 = format!("Before\n![](img:{img1_b})\n![](img:{img2_b})\nAfter");
        let editor2 = Rc::new(Editor::new(note_id2, &content2, db2, Rc::new(DummyClipboard::default())));
        let ui2 = NoteWindow::new().expect("NoteWindow");
        ui2.window().set_size(slint::LogicalSize::new(447.0, 468.0));
        editor2.bind_ui(&ui2, Rc::new(|_| {}), None, None, None);
        editor2.relayout(&ui2);

        // 0: "Before"
        // 1: img1
        // 2: img2
        // 3: "After"

        // Down from image 1 should move to image 2
        let res = editor2.key(1, &down_key, 0, &ui2);
        assert_eq!(res, Some((Caret { block: 2, offset: 0 }, false)), "Down arrow from image 1 moves to adjacent image 2");
        let res = editor2.key(1, &right_key, 0, &ui2);
        assert_eq!(res, Some((Caret { block: 2, offset: 0 }, false)), "Right arrow from image 1 moves to adjacent image 2");

        // Up from image 2 should move to image 1
        let res = editor2.key(2, &up_key, 0, &ui2);
        assert_eq!(res, Some((Caret { block: 1, offset: 0 }, false)), "Up arrow from image 2 moves to adjacent image 1");
        let res = editor2.key(2, &left_key, 0, &ui2);
        assert_eq!(res, Some((Caret { block: 1, offset: 0 }, false)), "Left arrow from image 2 moves to adjacent image 1");
    }
}
