use crate::occlusion::{fully_covered, Above, Rect};
use crate::platform::{NoteId, SignalEvent, WindowSignals, WindowState};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::sync::Mutex;
use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows_sys::Win32::Graphics::Dwm::{DwmGetWindowAttribute, DWMWA_CLOAKED};
use windows_sys::Win32::System::Threading::GetCurrentProcessId;
use windows_sys::Win32::UI::Shell::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    GetForegroundWindow, GetWindow, GetWindowRect, GetWindowThreadProcessId, IsIconic, IsWindowVisible, GW_HWNDPREV,
    WM_ACTIVATE, WM_ACTIVATEAPP, WM_MOVE, WM_NCDESTROY, WM_PAINT, WM_SIZE,
};

const SUBCLASS_ID: usize = 0x5741_5348; // "WASH"

thread_local! {
    static NOTIFIER: RefCell<Option<Rc<dyn Fn(SignalEvent)>>> = const { RefCell::new(None) };
}

static STATE_QUEUED: AtomicBool = AtomicBool::new(false);
static PAINT_QUEUED: AtomicBool = AtomicBool::new(false);
static LAST_PAINT_NOTE: AtomicI64 = AtomicI64::new(0);

/// Reads minimized, covered and focus state from registered note HWNDs, and hooks their window
/// procedures so minimize, restore, activation and paint requests arrive as events.
#[derive(Default)]
pub struct Win32Signals {
    hwnds: RefCell<HashMap<NoteId, isize>>,
}

impl Win32Signals {
    pub fn register(&self, note: NoteId, hwnd: isize) {
        self.hwnds.borrow_mut().insert(note, hwnd);
        // SAFETY: the hwnd is a live window of this thread; the subclass procedure only
        // posts to the event loop and forwards to the previous procedure.
        unsafe {
            SetWindowSubclass(hwnd as HWND, Some(subclass_proc), SUBCLASS_ID, note as usize);
        }
    }

    pub fn unregister(&self, note: NoteId) {
        if let Some(h) = self.hwnds.borrow_mut().remove(&note) {
            // SAFETY: tolerates a window that is already gone.
            unsafe {
                RemoveWindowSubclass(h as HWND, Some(subclass_proc), SUBCLASS_ID);
            }
        }
    }
}

/// Runs inside the window procedure, so it must not touch the app: it only queues a call
/// for the event loop (coalesced) and lets Windows carry on.
unsafe extern "system" fn subclass_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM, _id: usize, note: usize) -> LRESULT {
    match msg {
        WM_SIZE | WM_MOVE | WM_ACTIVATE | WM_ACTIVATEAPP => {
            if msg == WM_SIZE || msg == WM_ACTIVATE || msg == WM_ACTIVATEAPP {
                post(SignalEvent::StateChanged);
            }
            // a minimized window "moves" to (-32000, -32000): not a place worth saving
            if (msg == WM_SIZE || msg == WM_MOVE) && IsIconic(hwnd) == 0 {
                post_geometry(note as NoteId);
            }
        }
        WM_PAINT => post(SignalEvent::Paint(note as NoteId)),
        WM_NCDESTROY => {
            RemoveWindowSubclass(hwnd, Some(subclass_proc), SUBCLASS_ID);
        }
        _ => {}
    }
    DefSubclassProc(hwnd, msg, wparam, lparam)
}

/// Notes that moved or resized and have not been reported yet (coalesced per note).
static GEOMETRY_PENDING: Mutex<Vec<NoteId>> = Mutex::new(Vec::new());

fn post_geometry(note: NoteId) {
    let first = {
        let Ok(mut pending) = GEOMETRY_PENDING.lock() else { return };
        if pending.contains(&note) {
            return;
        }
        pending.push(note);
        pending.len() == 1
    };
    if first {
        let _ = slint::invoke_from_event_loop(|| {
            let notes = GEOMETRY_PENDING.lock().map(|mut p| std::mem::take(&mut *p)).unwrap_or_default();
            let notify = NOTIFIER.with(|n| n.borrow().clone());
            if let Some(notify) = notify {
                for note in notes {
                    notify(SignalEvent::Geometry(note));
                }
            }
        });
    }
}

fn post(ev: SignalEvent) {
    let queued = match ev {
        SignalEvent::Geometry(note) => return post_geometry(note),
        SignalEvent::StateChanged => &STATE_QUEUED,
        SignalEvent::Paint(note) => {
            LAST_PAINT_NOTE.store(note, Ordering::Release);
            &PAINT_QUEUED
        }
    };
    if queued.swap(true, Ordering::AcqRel) {
        return; // one is already on its way
    }
    let _ = slint::invoke_from_event_loop(move || {
        queued.store(false, Ordering::Release);
        let ev = match ev {
            SignalEvent::StateChanged | SignalEvent::Geometry(_) => ev,
            SignalEvent::Paint(_) => SignalEvent::Paint(LAST_PAINT_NOTE.load(Ordering::Acquire)),
        };
        let notify = NOTIFIER.with(|n| n.borrow().clone());
        if let Some(notify) = notify {
            notify(ev);
        }
    });
}

/// The top-level windows above `hwnd` in z-order that could cover part of `note`, nearest first.
/// Cheap rejections come first (hidden, minimized, not overlapping the note): the DWM cloak query
/// is the slow call and a desktop easily has hundreds of windows.
fn windows_above(hwnd: HWND, note: Rect) -> Vec<Above> {
    let mut out = Vec::new();
    // SAFETY: plain window queries; stale handles just report failure.
    unsafe {
        let mut w = GetWindow(hwnd, GW_HWNDPREV);
        while !w.is_null() {
            let mut r = RECT { left: 0, top: 0, right: 0, bottom: 0 };
            let overlaps = IsWindowVisible(w) != 0
                && IsIconic(w) == 0
                && GetWindowRect(w, &mut r) != 0
                && r.left < note.right
                && r.right > note.left
                && r.top < note.bottom
                && r.bottom > note.top;
            if overlaps {
                let mut cloaked = 0u32;
                let hr = DwmGetWindowAttribute(w, DWMWA_CLOAKED as u32, &mut cloaked as *mut u32 as *mut _, 4);
                out.push(Above { rect: Rect::new(r.left, r.top, r.right, r.bottom), visible: true, cloaked: hr == 0 && cloaked != 0, minimized: false });
            }
            w = GetWindow(w, GW_HWNDPREV);
        }
    }
    out
}

fn is_covered(hwnd: HWND) -> bool {
    let mut r = RECT { left: 0, top: 0, right: 0, bottom: 0 };
    // SAFETY: plain query.
    if unsafe { GetWindowRect(hwnd, &mut r) } == 0 {
        return false;
    }
    let note = Rect::new(r.left, r.top, r.right, r.bottom);
    fully_covered(note, &windows_above(hwnd, note))
}

impl WindowSignals for Win32Signals {
    fn window_state(&self, note: NoteId) -> WindowState {
        let Some(&h) = self.hwnds.borrow().get(&note) else { return WindowState::default() };
        let hwnd = h as HWND;
        // SAFETY: both calls tolerate stale handles (they just report false/null).
        let (minimized, focused) = unsafe { (IsIconic(hwnd) != 0, GetForegroundWindow() == hwnd) };
        // Only meaningful (and only worth the walk) when Washi is in the background.
        let covered = !minimized && !focused && self.app_in_background() && is_covered(hwnd);
        WindowState { minimized, covered, focused }
    }

    fn attach(&self, note: NoteId, hwnd: isize) {
        self.register(note, hwnd);
    }

    fn detach(&self, note: NoteId) {
        self.unregister(note);
    }

    fn set_notifier(&self, notify: Rc<dyn Fn(SignalEvent)>) {
        NOTIFIER.with(|n| *n.borrow_mut() = Some(notify));
    }

    fn app_in_background(&self) -> bool {
        // SAFETY: simple queries.
        unsafe {
            let fg = GetForegroundWindow();
            if fg.is_null() {
                return true;
            }
            let mut pid = 0u32;
            GetWindowThreadProcessId(fg, &mut pid);
            pid != GetCurrentProcessId()
        }
    }
}
