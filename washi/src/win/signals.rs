use crate::platform::{NoteId, WindowSignals, WindowState};
use std::cell::RefCell;
use std::collections::HashMap;
use windows_sys::Win32::Foundation::HWND;
use windows_sys::Win32::System::Threading::GetCurrentProcessId;
use windows_sys::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId, IsIconic};

/// Reads minimized/focus state from registered note HWNDs.
/// `covered` is always false until the suspend ticket adds the occlusion check (plan 7.1).
#[derive(Default)]
pub struct Win32Signals {
    hwnds: RefCell<HashMap<NoteId, isize>>,
}

impl Win32Signals {
    pub fn register(&self, note: NoteId, hwnd: isize) {
        self.hwnds.borrow_mut().insert(note, hwnd);
    }
    pub fn unregister(&self, note: NoteId) {
        self.hwnds.borrow_mut().remove(&note);
    }
}

impl WindowSignals for Win32Signals {
    fn window_state(&self, note: NoteId) -> WindowState {
        let Some(&h) = self.hwnds.borrow().get(&note) else { return WindowState::default() };
        let hwnd = h as HWND;
        // SAFETY: both calls tolerate stale handles (they just report false/null).
        unsafe {
            WindowState { minimized: IsIconic(hwnd) != 0, covered: false, focused: GetForegroundWindow() == hwnd }
        }
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
