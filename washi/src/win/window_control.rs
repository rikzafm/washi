use crate::platform::{NoteId, ResizeEdge, ScreenRect, WindowControl};
use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows_sys::Win32::Graphics::Gdi::{EnumDisplayMonitors, GetMonitorInfoW, ScreenToClient, HDC, HMONITOR, MONITORINFO};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::ReleaseCapture;
use windows_sys::Win32::UI::Shell::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    GetCursorPos, GetTopWindow, GetWindow, PostMessageW, GW_HWNDNEXT, HTBOTTOM, HTBOTTOMLEFT, HTBOTTOMRIGHT, HTCAPTION, HTLEFT, HTRIGHT, HTTOP, HTTOPLEFT,
    HTTOPRIGHT, WM_LBUTTONUP, WM_NCDESTROY, WM_NCLBUTTONDOWN,
};

const DRAG_SUBCLASS_ID: usize = 0x5744_5247; // "WDRG"

/// `MONITORINFOF_PRIMARY`
const MONITORINFOF_PRIMARY: u32 = 1;

/// Native drag through the non-client machinery: what a title bar or a sizing border does.
/// Windows runs the move or size loop itself, honors the window's minimum size, and sends the
/// window its usual move and size messages.
pub struct Win32WindowControl;

fn hit_test_code(edge: ResizeEdge) -> u32 {
    match edge {
        ResizeEdge::North => HTTOP,
        ResizeEdge::NorthEast => HTTOPRIGHT,
        ResizeEdge::East => HTRIGHT,
        ResizeEdge::SouthEast => HTBOTTOMRIGHT,
        ResizeEdge::South => HTBOTTOM,
        ResizeEdge::SouthWest => HTBOTTOMLEFT,
        ResizeEdge::West => HTLEFT,
        ResizeEdge::NorthWest => HTTOPLEFT,
    }
}

/// Hand the pointer, which is already down on the window, over to Windows as a press on `hit`.
fn start_native_drag(hwnd: Option<isize>, hit: u32) {
    let Some(hwnd) = hwnd else { return };
    // SAFETY: plain calls on a window of this thread. The message is posted (not sent), so the
    // modal loop starts after Slint's callback has returned. Installing the subclass again on the
    // same window only replaces its (unused) reference data.
    unsafe {
        SetWindowSubclass(hwnd as HWND, Some(drag_subclass_proc), DRAG_SUBCLASS_ID, 0);
        let mut at = POINT { x: 0, y: 0 };
        GetCursorPos(&mut at);
        ReleaseCapture();
        let lparam = (((at.y as u32) & 0xFFFF) << 16 | ((at.x as u32) & 0xFFFF)) as LPARAM;
        PostMessageW(hwnd as HWND, WM_NCLBUTTONDOWN, hit as usize, lparam);
    }
}

/// On every window that hands a press to Windows (notes and picture viewers alike). Windows' move
/// or size loop runs inside the WM_NCLBUTTONDOWN call and swallows the button-up, so neither winit
/// nor Slint ever learns the button was released: the grip keeps the pointer grab and eats every
/// later click (each one starting another move or resize). Once the loop is over, post the missing
/// button-up.
unsafe extern "system" fn drag_subclass_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM, _id: usize, _data: usize) -> LRESULT {
    match msg {
        WM_NCLBUTTONDOWN if wparam == HTCAPTION as usize || (HTLEFT as usize..=HTBOTTOMRIGHT as usize).contains(&wparam) => {
            let result = DefSubclassProc(hwnd, msg, wparam, lparam);
            let mut at = POINT { x: 0, y: 0 };
            GetCursorPos(&mut at);
            ScreenToClient(hwnd, &mut at);
            let client = (((at.y as u32) & 0xFFFF) << 16 | ((at.x as u32) & 0xFFFF)) as LPARAM;
            PostMessageW(hwnd, WM_LBUTTONUP, 0, client);
            result
        }
        WM_NCDESTROY => {
            RemoveWindowSubclass(hwnd, Some(drag_subclass_proc), DRAG_SUBCLASS_ID);
            DefSubclassProc(hwnd, msg, wparam, lparam)
        }
        _ => DefSubclassProc(hwnd, msg, wparam, lparam),
    }
}

unsafe extern "system" fn collect(monitor: HMONITOR, _dc: HDC, _rect: *mut RECT, out: LPARAM) -> i32 {
    let out = &mut *(out as *mut Vec<(bool, ScreenRect)>);
    let mut info: MONITORINFO = std::mem::zeroed();
    info.cbSize = std::mem::size_of::<MONITORINFO>() as u32;
    if GetMonitorInfoW(monitor, &mut info) != 0 {
        let w = info.rcWork;
        out.push((info.dwFlags & MONITORINFOF_PRIMARY != 0, ScreenRect::new(w.left, w.top, w.right - w.left, w.bottom - w.top)));
    }
    1
}

impl WindowControl for Win32WindowControl {
    fn begin_move(&self, _note: NoteId, hwnd: Option<isize>) {
        start_native_drag(hwnd, HTCAPTION);
    }

    fn begin_resize(&self, _note: NoteId, hwnd: Option<isize>, edge: ResizeEdge) {
        start_native_drag(hwnd, hit_test_code(edge));
    }

    fn bring_to_front(&self, _note: NoteId, hwnd: Option<isize>) {
        if let Some(h) = hwnd {
            unsafe {
                use windows_sys::Win32::UI::WindowsAndMessaging::{IsIconic, SetForegroundWindow, ShowWindow, SW_RESTORE};
                let hwnd = h as HWND;
                if IsIconic(hwnd) != 0 {
                    ShowWindow(hwnd, SW_RESTORE);
                }
                SetForegroundWindow(hwnd);
            }
        }
    }

    fn stacking_order(&self, notes: &[(NoteId, Option<isize>)]) -> Vec<NoteId> {
        // Walk the desktop's top-level windows front to back, noting where each note's window is.
        let mut front_to_back: Vec<NoteId> = Vec::new();
        // SAFETY: plain window queries; a stale handle just ends the walk.
        unsafe {
            let mut w = GetTopWindow(std::ptr::null_mut());
            while !w.is_null() {
                if let Some((id, _)) = notes.iter().find(|(_, h)| *h == Some(w as isize)) {
                    front_to_back.push(*id);
                }
                w = GetWindow(w, GW_HWNDNEXT);
            }
        }
        let mut out: Vec<NoteId> = notes.iter().map(|(id, _)| *id).filter(|id| !front_to_back.contains(id)).collect();
        out.extend(front_to_back.into_iter().rev());
        out
    }

    fn work_areas(&self) -> Vec<ScreenRect> {
        let mut found: Vec<(bool, ScreenRect)> = Vec::new();
        // SAFETY: the callback only pushes to the vector for the duration of the call.
        unsafe {
            EnumDisplayMonitors(std::ptr::null_mut(), std::ptr::null(), Some(collect), &mut found as *mut _ as LPARAM);
        }
        // primary first
        found.sort_by_key(|(primary, _)| !*primary);
        found.into_iter().map(|(_, r)| r).collect()
    }
}
