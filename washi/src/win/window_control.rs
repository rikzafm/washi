use crate::platform::{NoteId, ResizeEdge, ScreenRect, WindowControl};
use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, SIZE, WPARAM};
use windows_sys::Win32::Graphics::Dwm::{DwmFlush, DwmSetWindowAttribute, DWMWA_CLOAK};
use windows_sys::Win32::Graphics::Gdi::{
    BitBlt, CreateCompatibleDC, CreateDIBSection, DeleteDC, DeleteObject, EnumDisplayMonitors, GetDC, GetMonitorInfoW, RedrawWindow, ReleaseDC,
    ScreenToClient, SelectObject, AC_SRC_ALPHA, AC_SRC_OVER, BITMAPINFO, BITMAPINFOHEADER, BLENDFUNCTION, DIB_RGB_COLORS, HDC, HMONITOR, MONITORINFO,
    RDW_INVALIDATE, RDW_UPDATENOW, SRCCOPY,
};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::ReleaseCapture;
use windows_sys::Win32::UI::Shell::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DestroyWindow, GetCursorPos, GetTopWindow, GetWindow, GetWindowLongW, GetWindowRect, PostMessageW, SetWindowPos,
    UpdateLayeredWindow, GWL_EXSTYLE, GW_HWNDNEXT, HTBOTTOM, HTBOTTOMLEFT, HTBOTTOMRIGHT, HTCAPTION, HTLEFT, HTRIGHT, HTTOP, HTTOPLEFT, HTTOPRIGHT,
    HWND_TOP, HWND_TOPMOST, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_SHOWWINDOW, ULW_ALPHA, WINDOWPOS, WM_LBUTTONUP, WM_NCDESTROY,
    WM_NCLBUTTONDOWN, WM_PAINT, WM_SIZE, WM_WINDOWPOSCHANGED, WM_WINDOWPOSCHANGING, WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST,
    WS_EX_TRANSPARENT, WS_POPUP,
};

thread_local! {
    /// The window Windows is moving or sizing for the user right now (0: none).
    static DRAGGED: std::cell::Cell<isize> = const { std::cell::Cell::new(0) };
    /// Whether that drag moves the window's top left corner (a top or left edge is held).
    static MOVES_ORIGIN: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    /// The window shown in place of `DRAGGED` meanwhile (0: none). See "The stand-in" below.
    static STAND_IN: std::cell::Cell<isize> = const { std::cell::Cell::new(0) };
}

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
    let dragged = DRAGGED.with(|d| d.get()) == hwnd as isize;
    match msg {
        WM_NCLBUTTONDOWN if wparam == HTCAPTION as usize || (HTLEFT as usize..=HTBOTTOMRIGHT as usize).contains(&wparam) => {
            DRAGGED.with(|d| d.set(hwnd as isize));
            MOVES_ORIGIN.with(|m| m.set(moves_origin(wparam)));
            let result = DefSubclassProc(hwnd, msg, wparam, lparam);
            DRAGGED.with(|d| d.set(0));
            stand_in_close(hwnd);
            let mut at = POINT { x: 0, y: 0 };
            GetCursorPos(&mut at);
            ScreenToClient(hwnd, &mut at);
            let client = (((at.y as u32) & 0xFFFF) << 16 | ((at.x as u32) & 0xFFFF)) as LPARAM;
            PostMessageW(hwnd, WM_LBUTTONUP, 0, client);
            result
        }
        // The first step of a resize that moves the top left corner, before the window changes: from
        // here on the stand-in is what the user sees.
        WM_WINDOWPOSCHANGING if dragged && MOVES_ORIGIN.with(|m| m.get()) && (*(lparam as *const WINDOWPOS)).flags & SWP_NOSIZE == 0 => {
            stand_in_open(hwnd);
            DefSubclassProc(hwnd, msg, wparam, lparam)
        }
        // A step of the user's resize. Slint has the new size once the window's own procedure has
        // run, but what follows from it (the `changed width` handlers: the words laid out for the new
        // paper) only runs on the next turn of the event loop, after the paint Windows asks for now.
        // Every step was therefore painted twice, first with the words still laid out for the old
        // size, and each paint is a full software repaint: the note flickered and lagged the pointer.
        // Running the handlers here lets that one paint draw the new layout. Only inside the size
        // loop of this window, where no Slint code is on the stack (the loop was started by a posted
        // message).
        WM_SIZE if dragged => {
            let result = DefSubclassProc(hwnd, msg, wparam, lparam);
            slint::platform::update_timers_and_animations();
            // Paint the new size now, inside the step: the stand-in takes its picture from this paint.
            RedrawWindow(hwnd, std::ptr::null(), std::ptr::null_mut(), RDW_INVALIDATE | RDW_UPDATENOW);
            result
        }
        // The window has painted (a resize step, or anything else it draws meanwhile, like the
        // caret): show that picture.
        WM_PAINT if dragged => {
            let result = DefSubclassProc(hwnd, msg, wparam, lparam);
            stand_in_update(hwnd);
            result
        }
        // A step that only moved the window (no paint follows): the stand-in goes along.
        WM_WINDOWPOSCHANGED if dragged => {
            let result = DefSubclassProc(hwnd, msg, wparam, lparam);
            if (*(lparam as *const WINDOWPOS)).flags & SWP_NOSIZE != 0 {
                stand_in_update(hwnd);
            }
            result
        }
        WM_NCDESTROY => {
            RemoveWindowSubclass(hwnd, Some(drag_subclass_proc), DRAG_SUBCLASS_ID);
            DefSubclassProc(hwnd, msg, wparam, lparam)
        }
        _ => DefSubclassProc(hwnd, msg, wparam, lparam),
    }
}

/// Whether a drag on this hit-test code moves the window's top left corner.
fn moves_origin(hit: usize) -> bool {
    [HTLEFT, HTTOP, HTTOPLEFT, HTTOPRIGHT, HTBOTTOMLEFT].iter().any(|h| *h as usize == hit)
}

// The stand-in.
//
// When the user drags a top or left edge, every step moves the window's top left corner and changes
// its size. Windows shows a window's new place at once but its new picture only a frame later (the
// picture goes to the compositor on its own schedule), so for one frame the old picture sat at the
// new corner: measured on screen, the far edge of the paper jumped by the step and back, on about
// one step in four even with a release build painting inside the step. Nothing makes a normal
// window's move and paint land together. A layered window can: UpdateLayeredWindow sets place, size
// and picture in one call.
//
// So for the length of such a drag the real window is cloaked (hidden by the compositor; it still
// sizes, lays out and paints as usual) and a layered window on top of it shows each picture the real
// window has painted, at the real window's place. Windows still runs the size loop on the real
// window, so the minimum size, the cursor and the messages are what they always were. Drags of the
// right or bottom edge leave the corner alone and need none of this.

/// Start showing the stand-in for `hwnd` (once per drag) and cloak `hwnd`.
unsafe fn stand_in_open(hwnd: HWND) {
    if STAND_IN.with(|s| s.get()) != 0 {
        return;
    }
    let class: Vec<u16> = "Static\0".encode_utf16().collect();
    let topmost = GetWindowLongW(hwnd, GWL_EXSTYLE) as u32 & WS_EX_TOPMOST;
    // Never takes the pointer, the focus or a taskbar button.
    let ex_style = WS_EX_LAYERED | WS_EX_TRANSPARENT | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE | topmost;
    let stand_in =
        CreateWindowExW(ex_style, class.as_ptr(), std::ptr::null(), WS_POPUP, 0, 0, 0, 0, std::ptr::null_mut(), std::ptr::null_mut(), std::ptr::null_mut(), std::ptr::null());
    if stand_in.is_null() {
        return; // the drag goes on with the real window in view
    }
    STAND_IN.with(|s| s.set(stand_in as isize));
    stand_in_update(hwnd);
    let order = if topmost != 0 { HWND_TOPMOST } else { HWND_TOP };
    SetWindowPos(stand_in, order, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_SHOWWINDOW);
    cloak(hwnd, true);
}

/// Show `hwnd` again and drop its stand-in (if there is one).
unsafe fn stand_in_close(hwnd: HWND) {
    let stand_in = STAND_IN.with(|s| s.replace(0)) as HWND;
    if stand_in.is_null() {
        return;
    }
    cloak(hwnd, false);
    // Two frames with both on screen (the same picture at the same place), so that the stand-in is
    // never gone before the real window is back.
    DwmFlush();
    DwmFlush();
    DestroyWindow(stand_in);
}

unsafe fn cloak(hwnd: HWND, on: bool) {
    let value: i32 = on as i32;
    DwmSetWindowAttribute(hwnd, DWMWA_CLOAK as u32, &value as *const i32 as *const _, std::mem::size_of::<i32>() as u32);
}

/// Put what `hwnd` has painted into its stand-in, at `hwnd`'s place and size, in one step.
unsafe fn stand_in_update(hwnd: HWND) {
    let stand_in = STAND_IN.with(|s| s.get()) as HWND;
    let mut rect: RECT = std::mem::zeroed();
    if stand_in.is_null() || GetWindowRect(hwnd, &mut rect) == 0 {
        return;
    }
    let (w, h) = (rect.right - rect.left, rect.bottom - rect.top);
    if w <= 0 || h <= 0 {
        return;
    }
    let screen = GetDC(std::ptr::null_mut());
    let picture = CreateCompatibleDC(screen);
    let mut info: BITMAPINFO = std::mem::zeroed();
    info.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
    info.bmiHeader.biWidth = w;
    info.bmiHeader.biHeight = -h; // top down
    info.bmiHeader.biPlanes = 1;
    info.bmiHeader.biBitCount = 32;
    let mut bits = std::ptr::null_mut();
    let bitmap = CreateDIBSection(picture, &info, DIB_RGB_COLORS, &mut bits, std::ptr::null_mut(), 0);
    if !bitmap.is_null() {
        let previous = SelectObject(picture, bitmap);
        // The window has no frame (its client area is the whole window) and paints premultiplied
        // alpha, which is what a layered window takes.
        let painted = GetDC(hwnd);
        BitBlt(picture, 0, 0, w, h, painted, 0, 0, SRCCOPY);
        ReleaseDC(hwnd, painted);
        let at = POINT { x: rect.left, y: rect.top };
        let size = SIZE { cx: w, cy: h };
        let from = POINT { x: 0, y: 0 };
        let blend = BLENDFUNCTION { BlendOp: AC_SRC_OVER as u8, BlendFlags: 0, SourceConstantAlpha: 255, AlphaFormat: AC_SRC_ALPHA as u8 };
        UpdateLayeredWindow(stand_in, screen, &at, &size, picture, &from, 0, &blend, ULW_ALPHA);
        SelectObject(picture, previous);
        DeleteObject(bitmap);
    }
    DeleteDC(picture);
    ReleaseDC(std::ptr::null_mut(), screen);
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

/// Move and resize a window in one step (physical px), without activating it or changing its
/// z-order. Two separate calls (position, then size) would show one frame with the old size at the
/// new place.
pub fn set_window_bounds(hwnd: isize, x: i32, y: i32, w: i32, h: i32) {
    use windows_sys::Win32::UI::WindowsAndMessaging::{SetWindowPos, SWP_NOACTIVATE, SWP_NOOWNERZORDER, SWP_NOZORDER};
    // SAFETY: a plain call on a window of this thread; a stale handle just fails. Never SWP_NOREDRAW:
    // the frame must repaint.
    unsafe {
        SetWindowPos(hwnd as HWND, std::ptr::null_mut(), x, y, w, h, SWP_NOZORDER | SWP_NOACTIVATE | SWP_NOOWNERZORDER);
    }
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

    fn place_behind(&self, _note: NoteId, hwnd: Option<isize>, above: Option<isize>) {
        let (Some(h), Some(above)) = (hwnd, above) else { return };
        // SAFETY: plain window calls; a stale handle makes them fail harmlessly.
        unsafe {
            use windows_sys::Win32::UI::WindowsAndMessaging::{SetForegroundWindow, SetWindowPos, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE};
            SetWindowPos(h as HWND, above as HWND, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
            SetForegroundWindow(above as HWND);
        }
    }

    fn fine_timer(&self, on: bool) {
        use windows_sys::Win32::Media::{timeBeginPeriod, timeEndPeriod};
        // SAFETY: plain calls; paired by the caller (SettingsView).
        unsafe {
            if on {
                timeBeginPeriod(1);
            } else {
                timeEndPeriod(1);
            }
        }
    }

    fn set_bounds(&self, hwnd: Option<isize>, x: i32, y: i32, w: i32, h: i32) -> bool {
        let Some(hwnd) = hwnd else { return false };
        set_window_bounds(hwnd, x, y, w, h);
        true
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
