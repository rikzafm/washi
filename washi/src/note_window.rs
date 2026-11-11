//! One open note: the Slint window plus its 500 ms autosave debounce.

use crate::document::{Align, Caret, Document, Range};
use crate::editor::Editor;
use crate::images::{Rgba, Stored};
use crate::platform::{Clock, ImageClipboard, NoteId, ResizeEdge, ScreenRect};
use crate::storage::{Db, Geometry};
use slint::{CloseRequestResponse, ComponentHandle, Timer, TimerMode};
use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;

slint::include_modules!();

pub use crate::editor::{
    block_data, ink_color, marker_color, rgb, same_row, swatch_marker, CURSOR_DEFAULT, CURSOR_RESIZE,
    CURSOR_TEXT, FORMAT_BOLD, FORMAT_ITALIC, FORMAT_LIST, FORMAT_STRIKE, FORMAT_UNDERLINE, GRIP,
    HINT, HISTORY_PAUSE, MULTI_CLICK, MULTI_CLICK_SLOP, AUTOSCROLL_STEP, REDO_KEY, START, UNDO_KEY,
};

/// Quiet period after the last edit before the note is written to the database.
pub const AUTOSAVE_DELAY: Duration = Duration::from_millis(500);

/// Quiet period after the last move or resize before the position and size are written. Its own
/// timer: typing never delays it, and dragging never delays the text.
pub const GEOMETRY_DELAY: Duration = Duration::from_millis(500);

/// How long the scrollbar stays shown after scrolling stops.
pub const SCROLLBAR_HOLD: Duration = Duration::from_millis(600);

/// A new note's Paper at UI size Large (`PaperTheme.paper-default-*`); its window adds the margins.
pub const DEFAULT_PAPER: (f32, f32) = (363.0, 384.0);

/// The window a new note opens with: the default Paper and its margins (logical px; on screen that
/// is times the UI size, which scales the whole window).
pub fn new_note_window_size() -> (u32, u32) {
    let m = 2.0 * crate::settings::anim::MARGIN;
    ((DEFAULT_PAPER.0 + m).round() as u32, (DEFAULT_PAPER.1 + m).round() as u32)
}

/// A minimized window reports a position around (-32000, -32000); that is not where the note lives.
fn is_minimized_position(x: i32, y: i32) -> bool {
    x <= -30000 && y <= -30000
}

struct GeometrySave {
    id: Rc<Cell<NoteId>>,
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
        if let Err(e) = self.db.save_geometry(self.id.get(), g) {
            eprintln!("washi: could not save the position of note {}: {e}", self.id.get());
            self.dirty.set(true);
        }
    }
}

struct Autosave {
    id: Rc<Cell<NoteId>>,
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
        if let Err(e) = self.db.save_content(self.id.get(), ui.get_text().as_str(), self.clock.unix_millis()) {
            eprintln!("washi: autosave of note {} failed: {e}", self.id.get());
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

pub fn app_icon() -> slint::Image {
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
    /// `image` is the picture block under the pointer, if any.
    ContextMenu { at: (i32, i32), image: Option<usize> },
    /// The mouse moved over the note (wakes a suspended app).
    PointerMoved,
    /// The note's canvas was pressed with the pointer.
    PointerDown,
    /// The pink tape or top margin was pressed: drag the window.
    StartMove,
    /// A resize zone was pressed.
    StartResize(ResizeEdge),
    New,
    Close,
    Delete,
    /// The toolbar's image button: the app asks for a picture to add.
    InsertImage,
    /// A double click on the picture of this block: the app shows it in a window of its own.
    OpenImage(usize),
    /// The top-right `...` button was clicked: open or toggle the dropdown menu.
    OpenDropdown,
    /// The note's text content was edited.
    TextChanged,
}

pub struct Note {
    /// The note shown. Shared with the window's callbacks: the All Notes editor swaps it in place.
    id: Rc<Cell<NoteId>>,
    ui: NoteWindow,
    editor: Rc<Editor>,
    autosave: Rc<Autosave>,
    timer: Rc<Timer>,
    geometry: Rc<GeometrySave>,
    geometry_timer: Rc<Timer>,
    scroll_timer: Rc<Timer>,
    on_event: Rc<dyn Fn(NoteId, NoteEvent)>,
    paste_image: Rc<dyn Fn(NoteId) -> bool>,
    /// False for the All Notes editor: its window is wider than the note (the sidebar's strip), so
    /// it must never write its position as the note's.
    saves_geometry: Cell<bool>,
    /// The window's scale: the monitor's times the UI size.
    scale: Rc<crate::ui_scale::WindowScale>,
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
        clipboard: Rc<dyn ImageClipboard>,
    ) -> Result<Note, slint::PlatformError> {
        let ui = NoteWindow::new()?;
        let editor = Rc::new(Editor::new(id, content, db.clone(), clipboard));
        editor.set_clock(clock.clone());
        let id_cell = Rc::new(Cell::new(id));
        let autosave = Rc::new(Autosave { id: id_cell.clone(), db, clock, ui: ui.as_weak(), dirty: Cell::new(false) });
        let timer = Rc::new(Timer::default());
        let scroll_timer = Rc::new(Timer::default());

        let on_note_changed = {
            let (autosave, timer, on_event, id) = (autosave.clone(), timer.clone(), on_event.clone(), id_cell.clone());
            Rc::new(move |_focus: Option<Caret>| {
                autosave.dirty.set(true);
                let a = autosave.clone();
                timer.start(TimerMode::SingleShot, AUTOSAVE_DELAY, move || a.flush());
                on_event(id.get(), NoteEvent::TextChanged);
            })
        };

        let pointer_down_hook = {
            let (on_event, id) = (on_event.clone(), id_cell.clone());
            Some(Rc::new(move || on_event(id.get(), NoteEvent::PointerDown)) as Rc<dyn Fn()>)
        };

        let open_image_hook = {
            let (on_event, id) = (on_event.clone(), id_cell.clone());
            Some(Rc::new(move |block| on_event(id.get(), NoteEvent::OpenImage(block))) as Rc<dyn Fn(usize)>)
        };

        let paste_image_hook = {
            let (on_paste, id) = (paste_image.clone(), id_cell.clone());
            Some(Rc::new(move || on_paste(id.get())) as Rc<dyn Fn() -> bool>)
        };

        editor.bind_ui(&ui, on_note_changed, pointer_down_hook, open_image_hook, paste_image_hook);

        ui.set_window_title(window_title(content).into());
        ui.set_pinned(always_on_top);
        ui.set_app_icon(app_icon());

        let scale = Rc::new(crate::ui_scale::WindowScale::attach(ui.window()));
        {
            let (weak, scale, weak_editor) = (ui.as_weak(), scale.clone(), Rc::downgrade(&editor));
            ui.on_scale_factor_changed(move || {
                if let (Some(ui), Some(editor)) = (weak.upgrade(), weak_editor.upgrade()) {
                    if scale.resync(ui.window(), false) {
                        editor.relayout(&ui);
                    }
                }
            });
        }

        if let Some(g) = geometry {
            let sf = scale.factor();
            ui.window().set_size(slint::PhysicalSize::new((g.w as f32 * sf).round() as u32, (g.h as f32 * sf).round() as u32));
            ui.window().set_position(slint::PhysicalPosition::new((g.x as f32 * sf).round() as i32, (g.y as f32 * sf).round() as i32));
        }

        {
            let (weak, scroll_timer) = (ui.as_weak(), scroll_timer.clone());
            ui.on_scroll_activity(move || {
                let weak = weak.clone();
                scroll_timer.start(TimerMode::SingleShot, SCROLLBAR_HOLD, move || {
                    if let Some(ui) = weak.upgrade() {
                        ui.set_scrolling(false);
                    }
                });
            });
        }

        {
            let (on_event, id) = (on_event.clone(), id_cell.clone());
            ui.window().on_close_requested(move || {
                on_event(id.get(), NoteEvent::WindowClosed);
                CloseRequestResponse::HideWindow
            });
        }

        {
            let (on_event, weak, weak_editor, id) = (on_event.clone(), ui.as_weak(), Rc::downgrade(&editor), id_cell.clone());
            ui.on_context_menu(move |x, y| {
                let (Some(ui), Some(editor)) = (weak.upgrade(), weak_editor.upgrade()) else { return };
                let w = ui.window();
                let scale = w.scale_factor();
                let origin = w.position();
                let at = (origin.x + (x * scale) as i32, origin.y + (y * scale) as i32);
                let image = editor.take_right_click().and_then(|p| editor.image_at(&ui, p));
                if let Some(block) = image {
                    let c = Caret { block, offset: 0 };
                    editor.set_selection(&ui, Range { anchor: c, focus: c });
                }
                on_event(id.get(), NoteEvent::ContextMenu { at, image });
            });
        }

        let forward = |ev: NoteEvent| {
            let (on_event, id) = (on_event.clone(), id_cell.clone());
            move || on_event(id.get(), ev)
        };

        ui.on_pointer_moved(forward(NoteEvent::PointerMoved));
        ui.on_start_move(forward(NoteEvent::StartMove));
        {
            let (on_event, id) = (on_event.clone(), id_cell.clone());
            ui.on_start_resize(move |code| {
                if let Some(edge) = ResizeEdge::from_code(code) {
                    on_event(id.get(), NoteEvent::StartResize(edge));
                }
            });
        }
        ui.on_new_note(forward(NoteEvent::New));
        ui.on_close_note(forward(NoteEvent::Close));
        ui.on_insert_image(forward(NoteEvent::InsertImage));
        ui.on_delete_note(forward(NoteEvent::Delete));
        ui.on_open_dropdown(forward(NoteEvent::OpenDropdown));

        ui.show()?;
        if scale.resync(ui.window(), true) {
            editor.relayout(&ui);
        }
        let geometry = Rc::new(GeometrySave { id: id_cell.clone(), db: autosave.db.clone(), ui: ui.as_weak(), last: Cell::new(None), dirty: Cell::new(false) });
        geometry.current();

        Ok(Note {
            id: id_cell,
            ui,
            editor,
            autosave,
            timer,
            geometry,
            geometry_timer: Rc::new(Timer::default()),
            scroll_timer,
            on_event,
            paste_image,
            saves_geometry: Cell::new(true),
            scale,
        })
    }

    pub fn editor(&self) -> &Rc<Editor> {
        &self.editor
    }

    pub fn insert_position(&self) -> usize {
        self.editor.insert_position(&self.ui)
    }

    pub fn add_image(&self, stored: &Stored, after: usize) -> Result<(), crate::storage::DbError> {
        self.editor.add_image(stored, after)?;
        Ok(())
    }

    pub fn image_state(&self, block: usize) -> Option<(u8, Align)> {
        self.editor.image_state(block)
    }

    pub fn set_image_pct(&self, block: usize, pct: u8) {
        self.editor.set_image_pct(block, pct);
    }

    pub fn set_image_align(&self, block: usize, align: Align) {
        self.editor.set_image_align(block, align);
    }

    pub fn delete_image(&self, block: usize) {
        self.editor.delete_image(block);
    }

    pub fn image_rgba(&self, block: usize) -> Option<Rgba> {
        self.editor.image_rgba(block)
    }

    pub fn copy_image(&self, block: usize) -> bool {
        self.editor.copy_image(block)
    }

    pub fn cut_image(&self, block: usize) -> bool {
        self.editor.cut_image(block).is_some()
    }

    pub fn paste(&self) -> bool {
        if self.editor.paste() {
            return true;
        }
        (self.paste_image)(self.id.get())
    }

    pub fn paste_image(&self) -> bool {
        if self.editor.paste_image() {
            return true;
        }
        (self.paste_image)(self.id.get())
    }

    pub fn decoded_image_bytes(&self) -> usize {
        self.editor.decoded_image_bytes()
    }

    pub fn free_images(&self) {
        self.editor.free_images();
    }

    pub fn prune_images(&self) {
        if let Err(e) = self.editor.prune_images() {
            eprintln!("washi: could not remove unused pictures of note {}: {e}", self.id.get());
        }
    }

    pub fn geometry_changed(&self) {
        if self.scale.resync(self.ui.window(), false) {
            self.editor.relayout(&self.ui);
        }
        if !self.saves_geometry.get() {
            return;
        }
        self.geometry.dirty.set(true);
        let g = self.geometry.clone();
        self.geometry_timer.start(TimerMode::SingleShot, GEOMETRY_DELAY, move || g.flush());
    }

    pub fn screen_center(&self) -> (f32, f32) {
        let (pos, size) = (self.ui.window().position(), self.ui.window().size());
        (pos.x as f32 + size.width as f32 / 2.0, pos.y as f32 + size.height as f32 / 2.0)
    }

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
        self.id.get()
    }

    pub fn set_saves_geometry(&self, on: bool) {
        self.saves_geometry.set(on);
    }

    pub fn set_content_opacity(&self, o: f32) {
        self.ui.set_content_opacity(o);
    }

    pub fn set_visible(&self, visible: bool) {
        if visible {
            let _ = self.ui.show();
        } else {
            let _ = self.ui.hide();
        }
    }

    pub fn place(&self, g: Geometry) {
        if self.geometry.current() != Some(g) {
            let sf = self.scale.factor();
            self.ui.window().set_size(slint::PhysicalSize::new((g.w as f32 * sf).round() as u32, (g.h as f32 * sf).round() as u32));
            self.ui.window().set_position(slint::PhysicalPosition::new((g.x as f32 * sf).round() as i32, (g.y as f32 * sf).round() as i32));
        }
        self.geometry_changed();
    }

    pub fn selection(&self) -> Range {
        self.editor.selection()
    }

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

    pub fn picture_at(&self, block: usize) -> Option<(slint::Image, (u32, u32))> {
        self.editor.picture_at(block)
    }

    pub fn set_ui_scale(&self, ui: f32, k: f32) {
        self.geometry.last.set(self.geometry.last.get().map(|g| g.scaled(k)));
        self.scale.set_ui(self.ui.window(), ui);
        self.editor.relayout(&self.ui);
        if self.saves_geometry.get() {
            self.geometry.current();
        }
    }

    pub fn geometry(&self) -> Geometry {
        self.geometry.current().unwrap_or(Geometry { x: 0, y: 0, w: 0, h: 0 })
    }

    pub fn always_on_top(&self) -> bool {
        self.ui.get_pinned()
    }

    pub fn set_always_on_top(&self, on: bool) {
        self.ui.set_pinned(on);
    }

    pub fn is_dropdown_open(&self) -> bool {
        self.ui.get_dropdown_open()
    }

    pub fn set_dropdown_open(&self, open: bool) {
        self.ui.set_dropdown_open(open);
    }

    pub fn menu_button_rect(&self) -> (f32, f32, f32, f32) {
        (
            self.ui.get_menu_button_x(),
            self.ui.get_menu_button_y(),
            self.ui.get_menu_button_width(),
            self.ui.get_menu_button_height(),
        )
    }

    pub fn menu_button_opacity(&self) -> f32 {
        self.ui.get_menu_button_opacity()
    }

    pub fn menu_button_shown(&self) -> bool {
        self.ui.get_menu_button_shown()
    }

    pub fn title(&self) -> String {
        self.ui.get_window_title().to_string()
    }

    pub fn set_text(&self, text: &str) {
        self.scroll_timer.stop();
        self.editor.set_text(&self.ui, text);
        self.autosave.dirty.set(true);
        (self.on_event)(self.id.get(), NoteEvent::TextChanged);
    }

    pub fn switch_to(&self, id: NoteId, content: &str) {
        if id == self.id.get() {
            return;
        }
        self.flush_text();
        self.scroll_timer.stop();
        self.id.set(id);
        self.editor.switch_to(&self.ui, id, content);
    }

    pub fn set_autofocus(&self, on: bool) {
        self.ui.set_autofocus(on);
        if !on {
            self.ui.invoke_focus_keys();
        }
    }

    pub fn focus_start(&self) {
        self.editor.focus_start(&self.ui);
    }

    pub fn text(&self) -> String {
        self.ui.get_text().to_string()
    }

    pub fn pause(&self) {
        self.timer.stop();
        self.geometry_timer.stop();
        self.scroll_timer.stop();
        self.editor.pause();
        self.ui.set_suspended(true);
        self.ui.window().dispatch_event(slint::platform::WindowEvent::WindowActiveChanged(false));
    }

    pub fn resume(&self, focused: bool) {
        self.editor.thaw_images();
        self.editor.sync_model();
        self.ui.set_scrolling(false);
        self.ui.set_suspended(false);
        self.ui.set_text_resync(self.ui.get_text_resync() + 1);
        if focused {
            self.ui.window().dispatch_event(slint::platform::WindowEvent::WindowActiveChanged(true));
        }
        self.ui.window().request_redraw();
    }

    pub fn flush_text(&self) {
        self.timer.stop();
        self.autosave.flush();
    }

    pub fn flush(&self) {
        self.flush_text();
        self.geometry_timer.stop();
        self.geometry.flush();
    }
}

impl Drop for Note {
    fn drop(&mut self) {
        self.timer.stop();
        self.geometry_timer.stop();
        self.scroll_timer.stop();
        self.editor.pause();
        let _ = self.ui.hide();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn note_trigger_button_interface_and_dropdown_state() {
        crate::dropdown::init_test_platform();
        let ui = NoteWindow::new().expect("NoteWindow::new");
        ui.window().set_size(slint::LogicalSize::new(447.0, 468.0));

        // Initially without dropdown open or hover, button is not shown or active
        assert!(!ui.get_dropdown_open());
        assert!(!ui.get_menu_button_shown());
        assert!(!ui.get_menu_button_active());
        assert!(!ui.get_menu_button_highlighted());

        // Setting dropdown_open to true activates visual state and keeps button shown
        ui.set_dropdown_open(true);
        assert!(ui.get_dropdown_open());
        assert!(ui.get_menu_button_shown());
        assert!(ui.get_menu_button_active());
        assert!(ui.get_menu_button_highlighted());

        // Setting dropdown_open back to false deactivates it
        ui.set_dropdown_open(false);
        assert!(!ui.get_dropdown_open());
        assert!(!ui.get_menu_button_shown());
        assert!(!ui.get_menu_button_active());
        assert!(!ui.get_menu_button_highlighted());

        // Position: top-right corner of paper
        let (x, y, w, h) = (
            ui.get_menu_button_x(),
            ui.get_menu_button_y(),
            ui.get_menu_button_width(),
            ui.get_menu_button_height(),
        );
        assert!(w >= 20.0);
        assert!(h >= 20.0);
        assert!(x > 300.0 && x + w <= 447.0, "x={x}, w={w} should be in top-right");
        assert!(y >= 30.0 && y < 100.0, "y={y} should be in top strip");

        let clicked = Rc::new(Cell::new(false));
        let clicked_clone = clicked.clone();
        ui.on_open_dropdown(move || {
            clicked_clone.set(true);
        });

        ui.invoke_open_dropdown();
        assert!(clicked.get());
    }

    #[test]
    fn note_event_open_dropdown_and_helpers() {
        let ev = NoteEvent::OpenDropdown;
        assert_eq!(ev, NoteEvent::OpenDropdown);
    }

    #[test]
    fn note_editor_text_cursor_kinds() {
        crate::dropdown::init_test_platform();
        let ui = NoteWindow::new().expect("NoteWindow::new");
        ui.window().set_size(slint::LogicalSize::new(447.0, 468.0));

        // Default text-cursor-kind is CURSOR_TEXT (text I-beam)
        assert_eq!(ui.get_text_cursor_kind(), CURSOR_TEXT);

        // Setting to CURSOR_DEFAULT (default arrow, e.g. over image) and CURSOR_RESIZE (nwse-resize, over grip)
        ui.set_text_cursor_kind(CURSOR_DEFAULT);
        assert_eq!(ui.get_text_cursor_kind(), CURSOR_DEFAULT);

        ui.set_text_cursor_kind(CURSOR_RESIZE);
        assert_eq!(ui.get_text_cursor_kind(), CURSOR_RESIZE);

        ui.set_text_cursor_kind(CURSOR_TEXT);
        assert_eq!(ui.get_text_cursor_kind(), CURSOR_TEXT);
    }
}
