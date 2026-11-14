//! Windows implementations of the `crate::platform` interfaces.

mod clipboard;
mod dialogs;
mod dpapi;
mod instance;
mod process;
mod signals;
mod startup;
mod system;
mod window_control;

pub use clipboard::Win32Clipboard;
pub use dialogs::Win32Dialogs;
pub use dpapi::DpapiProtector;
pub use instance::Win32Instance;
pub use process::Win32Power;
pub use signals::Win32Signals;
pub use startup::Win32Startup;
pub use system::{Win32Shell, Win32Stats};
pub use window_control::Win32WindowControl;

use crate::platform::Clock;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

pub(crate) fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

pub struct SystemClock {
    start: Instant,
}

impl SystemClock {
    pub fn new() -> Self {
        SystemClock { start: Instant::now() }
    }
}

impl Default for SystemClock {
    fn default() -> Self {
        Self::new()
    }
}

impl Clock for SystemClock {
    fn unix_millis(&self) -> i64 {
        SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0)
    }
    fn monotonic(&self) -> Duration {
        self.start.elapsed()
    }
}

/// Give a window the Washi logo (the .exe's icon resource, id 1) for its taskbar button, thumbnail
/// and Alt+Tab entry. Slint's own `icon` property does not reach these for our frameless windows.
pub fn set_window_icon(hwnd: isize) {
    use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows_sys::Win32::UI::WindowsAndMessaging::*;
    // SAFETY: plain Win32 calls; the icons are shared (LR_SHARED), so they need no cleanup.
    unsafe {
        let module = GetModuleHandleW(std::ptr::null());
        for (kind, cx, cy) in [
            (ICON_BIG, GetSystemMetrics(SM_CXICON), GetSystemMetrics(SM_CYICON)),
            (ICON_SMALL, GetSystemMetrics(SM_CXSMICON), GetSystemMetrics(SM_CYSMICON)),
        ] {
            let icon = LoadImageW(module, 1 as _, IMAGE_ICON, cx, cy, LR_SHARED);
            if !icon.is_null() {
                SendMessageW(hwnd as _, WM_SETICON, kind as usize, icon as isize);
                // The taskbar's preview list falls back to the window class icon for its title row.
                let slot = if kind == ICON_BIG { GCLP_HICON } else { GCLP_HICONSM };
                SetClassLongPtrW(hwnd as _, slot, icon as isize);
            }
        }
    }
}

/// Group every note's taskbar button under one Washi identity. Call once at startup, before
/// any window exists.
pub fn set_app_identity() {
    let id = wide("Washi.Notes");
    // SAFETY: NUL-terminated string, copied by the call.
    unsafe { windows_sys::Win32::UI::Shell::SetCurrentProcessExplicitAppUserModelID(id.as_ptr()) };
}
