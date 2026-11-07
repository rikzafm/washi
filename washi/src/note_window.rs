//! One open note: the Slint window plus its 500 ms autosave debounce.

use crate::platform::{Clock, NoteId};
use crate::storage::{Db, Geometry};
use slint::{CloseRequestResponse, ComponentHandle, Timer, TimerMode};
use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;

slint::include_modules!();

/// Quiet period after the last edit before the note is written to the database.
pub const AUTOSAVE_DELAY: Duration = Duration::from_millis(500);

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

/// A note's title: its first non-blank line, cut to about 40 characters. `None` for an empty note.
pub fn note_title(content: &str) -> Option<String> {
    const MAX: usize = 40;
    let line = content
        .split(['\n', '\r', '\u{2028}', '\u{2029}'])
        .map(str::trim)
        .find(|l| !l.is_empty())?;
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
    New,
    Close,
    Delete,
}

pub struct Note {
    id: NoteId,
    ui: NoteWindow,
    autosave: Rc<Autosave>,
    timer: Rc<Timer>,
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
    ) -> Result<Note, slint::PlatformError> {
        let ui = NoteWindow::new()?;
        ui.set_text(content.into());
        ui.set_window_title(window_title(content).into());
        ui.set_pinned(always_on_top);
        ui.set_app_icon(app_icon());
        if let Some(g) = geometry {
            ui.window().set_size(slint::LogicalSize::new(g.w as f32, g.h as f32));
            ui.window().set_position(slint::LogicalPosition::new(g.x as f32, g.y as f32));
        }
        let autosave = Rc::new(Autosave { id, db, clock, ui: ui.as_weak(), dirty: Cell::new(false) });
        let timer = Rc::new(Timer::default());
        {
            let (autosave, timer, weak) = (autosave.clone(), timer.clone(), ui.as_weak());
            ui.on_edited(move |text| {
                if let Some(ui) = weak.upgrade() {
                    ui.set_window_title(window_title(&text).into());
                }
                autosave.dirty.set(true);
                let a = autosave.clone();
                timer.start(TimerMode::SingleShot, AUTOSAVE_DELAY, move || a.flush());
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
            let on_event = on_event.clone();
            ui.on_new_note(move || on_event(id, NoteEvent::New));
        }
        {
            let on_event = on_event.clone();
            ui.on_close_note(move || on_event(id, NoteEvent::Close));
        }
        ui.on_delete_note(move || on_event(id, NoteEvent::Delete));
        ui.show()?;
        Ok(Note { id, ui, autosave, timer })
    }

    pub fn id(&self) -> NoteId {
        self.id
    }

    pub fn ui(&self) -> &NoteWindow {
        &self.ui
    }

    /// Where the window is and how big, in logical pixels.
    pub fn geometry(&self) -> Geometry {
        let w = self.ui.window();
        let scale = w.scale_factor();
        let (p, s) = (w.position(), w.size());
        Geometry {
            x: (p.x as f32 / scale).round() as i32,
            y: (p.y as f32 / scale).round() as i32,
            w: (s.width as f32 / scale).round() as u32,
            h: (s.height as f32 / scale).round() as u32,
        }
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

    pub fn text(&self) -> String {
        self.ui.get_text().to_string()
    }

    /// Save now if there are unsaved edits (used on close and quit).
    pub fn flush(&self) {
        self.timer.stop();
        self.autosave.flush();
    }
}

impl Drop for Note {
    /// Release the window (and with it the native window and its buffers).
    fn drop(&mut self) {
        let _ = self.ui.hide();
    }
}
