//! Everything Washi gets from Windows sits behind these interfaces. The Windows
//! implementations live in `crate::win`; test fakes live in the `snap` harness crate.

use std::path::PathBuf;
use std::time::Duration;
pub use zeroize::Zeroizing;

/// Stable id of a note (the `notes.id` row).
pub type NoteId = i64;

/// Wraps and unwraps the database key with a secret tied to the OS user (DPAPI on Windows).
pub trait KeyProtector {
    /// Produce a fresh random 256-bit key, or fail if the OS random generator does.
    fn generate_key(&self) -> Result<Zeroizing<[u8; 32]>, ProtectError>;
    fn protect(&self, raw: &[u8]) -> Result<Vec<u8>, ProtectError>;
    /// Must fail (never return garbage) if `wrapped` was altered. The key material is wiped when
    /// the result is dropped.
    fn unprotect(&self, wrapped: &[u8]) -> Result<Zeroizing<Vec<u8>>, ProtectError>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProtectError(pub String);

impl std::fmt::Display for ProtectError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for ProtectError {}

/// Wall-clock and monotonic time. The fake lets tests advance time.
pub trait Clock {
    /// Milliseconds since the Unix epoch (stored in `created_at` / `updated_at`).
    fn unix_millis(&self) -> i64;
    /// Monotonic time since app start (drives Slint timers in the harness).
    fn monotonic(&self) -> Duration;
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct WindowState {
    pub minimized: bool,
    pub covered: bool,
    pub focused: bool,
}

/// Something the platform noticed about Washi's windows, delivered on the UI thread.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SignalEvent {
    /// A note minimized, restored, gained or lost focus, or Washi moved to or from the background.
    StateChanged,
    /// A note was moved or resized.
    Geometry(NoteId),
    /// Windows asked a note to paint (for example because it was uncovered).
    Paint(NoteId),
}

/// Window-state signals used by the suspend controller.
pub trait WindowSignals {
    fn window_state(&self, note: NoteId) -> WindowState;
    fn app_in_background(&self) -> bool;
    /// A note's native window now exists / is gone. Fakes ignore this.
    fn attach(&self, _note: NoteId, _hwnd: isize) {}
    fn detach(&self, _note: NoteId) {}
    /// Deliver [`SignalEvent`]s to `notify` (on the UI thread, never from inside a window
    /// procedure) so a suspended app wakes without polling. Fakes ignore this: tests call the
    /// app directly.
    fn set_notifier(&self, _notify: std::rc::Rc<dyn Fn(SignalEvent)>) {}
    /// Windows is ending the session (shutdown, restart, sign-out). `handler` runs on the UI
    /// thread, synchronously inside the platform's message, and must finish its saving before it
    /// returns: the process may be gone right after. Fakes keep it for tests to call.
    fn set_session_end_handler(&self, _handler: std::rc::Rc<dyn Fn()>) {}
}

/// Efficiency mode and memory release (suspension, a later ticket).
pub trait ProcessPower {
    fn set_efficiency_mode(&self, on: bool);
    fn release_memory(&self);
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct MenuItem {
    pub id: u32,
    pub label: String,
    pub checked: bool,
    pub enabled: bool,
    pub children: Vec<MenuItem>,
}

/// The right-click menu. Returns the chosen item id, or `None` if dismissed.
pub trait ContextMenu {
    fn show(&self, items: &[MenuItem], at: (i32, i32)) -> Option<u32>;
}

/// The "Start with Windows" setting: a per-user registration that launches Washi at sign-in
/// (the `Run` registry value on Windows).
pub trait StartupRegistration {
    fn is_enabled(&self) -> bool;
    fn set_enabled(&self, on: bool);
}

/// What a launch found when it looked for an already running Washi.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Launch {
    /// This process is the one running Washi.
    First,
    /// Washi was already running and has been asked to open a new note; this process should exit.
    HandedOff,
}

/// One running Washi per user. A later launch hands off to the first.
pub trait SingleInstance {
    fn acquire(&self) -> Launch;
    /// First instance only: `on_request` runs on the UI thread each time a later launch asks
    /// for a new note.
    fn listen(&self, on_request: std::rc::Rc<dyn Fn()>);
}

/// A rectangle on the desktop in physical pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScreenRect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

impl ScreenRect {
    pub fn new(x: i32, y: i32, w: i32, h: i32) -> Self {
        ScreenRect { x, y, w, h }
    }
    pub fn right(&self) -> i32 {
        self.x + self.w
    }
    pub fn bottom(&self) -> i32 {
        self.y + self.h
    }
    /// The overlap with `other`, or `None` when they do not overlap.
    pub fn intersect(&self, other: &ScreenRect) -> Option<ScreenRect> {
        let (x, y) = (self.x.max(other.x), self.y.max(other.y));
        let (r, b) = (self.right().min(other.right()), self.bottom().min(other.bottom()));
        (r > x && b > y).then(|| ScreenRect::new(x, y, r - x, b - y))
    }
}

/// Which edge or corner of a note the user grabbed to resize it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResizeEdge {
    North,
    NorthEast,
    East,
    SouthEast,
    South,
    SouthWest,
    West,
    NorthWest,
}

impl ResizeEdge {
    /// The zone codes of `resize-zones.slint`: 0 is north, then clockwise.
    pub fn from_code(code: i32) -> Option<ResizeEdge> {
        use ResizeEdge::*;
        Some(match code {
            0 => North,
            1 => NorthEast,
            2 => East,
            3 => SouthEast,
            4 => South,
            5 => SouthWest,
            6 => West,
            7 => NorthWest,
            _ => return None,
        })
    }

    pub fn moves_left(self) -> bool {
        matches!(self, ResizeEdge::West | ResizeEdge::NorthWest | ResizeEdge::SouthWest)
    }
    pub fn moves_right(self) -> bool {
        matches!(self, ResizeEdge::East | ResizeEdge::NorthEast | ResizeEdge::SouthEast)
    }
    pub fn moves_top(self) -> bool {
        matches!(self, ResizeEdge::North | ResizeEdge::NorthEast | ResizeEdge::NorthWest)
    }
    pub fn moves_bottom(self) -> bool {
        matches!(self, ResizeEdge::South | ResizeEdge::SouthEast | ResizeEdge::SouthWest)
    }
}

/// Native window dragging and the screen layout. The user's pointer is already down when
/// `begin_*` is called; the platform runs the drag until the button is released and keeps the
/// window within its minimum size.
pub trait WindowControl {
    /// Start dragging a note's window by its caption (`hwnd` is its native handle, if any).
    fn begin_move(&self, note: NoteId, hwnd: Option<isize>);
    fn begin_resize(&self, note: NoteId, hwnd: Option<isize>, edge: ResizeEdge);
    /// Work areas (screens minus taskbars) of all connected monitors, primary first.
    fn work_areas(&self) -> Vec<ScreenRect>;
    /// The given notes (id and native handle, if any) ordered by their windows' stacking on the
    /// desktop, bottom first. Notes the platform cannot place keep their given order, below the rest.
    fn stacking_order(&self, notes: &[(NoteId, Option<isize>)]) -> Vec<NoteId>;
}

/// What to put on the clipboard when copying rich or visual content.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ClipboardPayload {
    pub plain: Option<String>,
    pub html: Option<String>,
    pub image: Option<crate::images::Rgba>,
}

/// The picture and rich text on the clipboard (Ctrl+C, Ctrl+X, Ctrl+V).
pub trait ImageClipboard {
    /// A bitmap on the clipboard, only when no text is there too (copying text from a page or
    /// spreadsheet often puts a bitmap beside it; that paste is text).
    fn image(&self) -> Option<crate::images::Rgba>;
    /// Put a picture on the clipboard (CF_DIB and CF_HTML).
    fn set_image(&self, img: &crate::images::Rgba) -> Result<(), String>;
    /// Put rich content on the clipboard (CF_HTML, CF_UNICODETEXT, and optional CF_DIB).
    fn set_rich(&self, payload: ClipboardPayload) -> Result<(), String>;
    /// HTML from the clipboard, if any.
    fn html(&self) -> Option<String>;
    /// Plain text from the clipboard, if any.
    fn text(&self) -> Option<String>;
}

/// Message boxes and file pickers.
pub trait Dialogs {
    fn confirm(&self, message: &str) -> bool;
    fn error(&self, message: &str);
    fn pick_open_file(&self, filter: &str) -> Option<PathBuf>;
    fn pick_save_file(&self, suggested_name: &str, filter: &str) -> Option<PathBuf>;
}
