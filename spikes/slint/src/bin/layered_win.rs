//! Option A prototype: Slint software renderer -> premultiplied BGRA DIB -> UpdateLayeredWindow.
//! No winit, no softbuffer. One buffer per window, freeable while minimized.
//! usage: layered_win [--notes N] [--ui note|bare|raw] [--x X --y Y] [--scale S]
//!        [--release-on-minimize] [--minimize-after S] [--quit-after S]
#![cfg(windows)]
use slint::platform::software_renderer::MinimalSoftwareWindow;
use slint::platform::{PointerEventButton, WindowAdapter, WindowEvent};
use slint::{ComponentHandle, LogicalPosition, PhysicalSize};
use std::cell::RefCell;
use std::ptr::{null, null_mut};
use std::rc::Rc;
use washi_spikes::{platform, Bare, Note};
use windows_sys::Win32::Foundation::*;
use windows_sys::Win32::Graphics::Gdi::*;
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::HiDpi::*;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::*;
use windows_sys::Win32::UI::WindowsAndMessaging::*;

const WM_MOUSELEAVE: u32 = 0x02A3;
fn arg(name: &str) -> Option<String> {
    let a: Vec<String> = std::env::args().collect();
    a.iter().position(|x| x == name).and_then(|i| a.get(i + 1).cloned())
}
fn flag(name: &str) -> bool { std::env::args().any(|x| x == name) }
fn wide(s: &str) -> Vec<u16> { s.encode_utf16().chain(Some(0)).collect() }

struct Dib { dc: HDC, bmp: HBITMAP, old: HGDIOBJ, bits: *mut platform::Bgra }
struct NoteWin {
    hwnd: HWND,
    win: Option<Rc<MinimalSoftwareWindow>>, // None in raw mode
    dib: RefCell<Option<Dib>>,
    px: (u32, u32),
    scale: f32,
    tracking: RefCell<bool>,
    minimized: RefCell<bool>,
    release_on_min: bool,
    _ui: Box<dyn std::any::Any>,
}
thread_local! { static WINS: RefCell<Vec<Rc<NoteWin>>> = RefCell::new(vec![]); }

fn new_dib(w: u32, h: u32) -> Dib {
    unsafe {
        let screen = GetDC(null_mut());
        let dc = CreateCompatibleDC(screen);
        let mut bi: BITMAPINFO = std::mem::zeroed();
        bi.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
        bi.bmiHeader.biWidth = w as i32;
        bi.bmiHeader.biHeight = -(h as i32);
        bi.bmiHeader.biPlanes = 1;
        bi.bmiHeader.biBitCount = 32;
        let mut bits: *mut core::ffi::c_void = null_mut();
        let bmp = CreateDIBSection(dc, &bi, DIB_RGB_COLORS, &mut bits, null_mut(), 0);
        let old = SelectObject(dc, bmp);
        ReleaseDC(null_mut(), screen);
        Dib { dc, bmp, old, bits: bits as *mut platform::Bgra }
    }
}
fn free_dib(d: Dib) { unsafe { SelectObject(d.dc, d.old); DeleteObject(d.bmp); DeleteDC(d.dc); } }

impl NoteWin {
    fn present(&self) {
        let mut slot = self.dib.borrow_mut();
        if *self.minimized.borrow() { return; }
        let (w, h) = self.px;
        if slot.is_none() { *slot = Some(new_dib(w, h)); if let Some(win) = &self.win { win.set_size(PhysicalSize::new(w, h)); } }
        let d = slot.as_ref().unwrap();
        let buf = unsafe { std::slice::from_raw_parts_mut(d.bits, (w * h) as usize) };
        let mut drew = false;
        match &self.win {
            Some(win) => { drew = win.draw_if_needed(|r| { r.render(buf, w as usize); }); }
            None => {
                // raw mode: draw a 50%-alpha red probe + opaque white square, no Slint
                if buf[0].a == 0 && buf[(w * 30 + 30) as usize].a == 0 {
                    for y in 0..h { for x in 0..w {
                        let inside = x > 30 && x < w - 30 && y > 30 && y < h - 30;
                        buf[(y * w + x) as usize] = if inside { platform::Bgra { b: 255, g: 255, r: 255, a: 255 } } else { platform::Bgra::default() };
                    } }
                    drew = true;
                }
            }
        }
        if drew {
            unsafe {
                let size = SIZE { cx: w as i32, cy: h as i32 };
                let src = POINT { x: 0, y: 0 };
                let bf = BLENDFUNCTION { BlendOp: AC_SRC_OVER as u8, BlendFlags: 0, SourceConstantAlpha: 255, AlphaFormat: AC_SRC_ALPHA as u8 };
                UpdateLayeredWindow(self.hwnd, null_mut(), null(), &size, d.dc, &src, 0, &bf, ULW_ALPHA);
            }
        }
    }
}

fn find(hwnd: HWND) -> Option<Rc<NoteWin>> { WINS.with(|w| w.borrow().iter().find(|n| n.hwnd == hwnd).cloned()) }

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    if let Some(n) = find(hwnd) {
        let pos = || { let x = (lp & 0xffff) as i16 as f32 / n.scale; let y = ((lp >> 16) & 0xffff) as i16 as f32 / n.scale; LogicalPosition::new(x, y) };
        match msg {
            WM_MOUSEMOVE => {
                if !*n.tracking.borrow() {
                    *n.tracking.borrow_mut() = true;
                    let mut t = TRACKMOUSEEVENT { cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32, dwFlags: TME_LEAVE, hwndTrack: hwnd, dwHoverTime: 0 };
                    TrackMouseEvent(&mut t);
                }
                if let Some(w) = &n.win { w.dispatch_event(WindowEvent::PointerMoved { position: pos() }); }
                return 0;
            }
            WM_MOUSELEAVE => {
                *n.tracking.borrow_mut() = false;
                if let Some(w) = &n.win { w.dispatch_event(WindowEvent::PointerExited); }
                return 0;
            }
            WM_LBUTTONDOWN => { SetCapture(hwnd); if let Some(w) = &n.win { w.dispatch_event(WindowEvent::PointerPressed { position: pos(), button: PointerEventButton::Left }); } return 0; }
            WM_LBUTTONUP => { ReleaseCapture(); if let Some(w) = &n.win { w.dispatch_event(WindowEvent::PointerReleased { position: pos(), button: PointerEventButton::Left }); } return 0; }
            WM_SIZE => {
                let kind = wp as u32;
                if kind == SIZE_MINIMIZED {
                    *n.minimized.borrow_mut() = true;
                    if n.release_on_min { if let Some(d) = n.dib.borrow_mut().take() { free_dib(d); } println!("RELEASED dib"); }
                } else if *n.minimized.borrow() {
                    *n.minimized.borrow_mut() = false;
                    if let Some(w) = &n.win { w.window().request_redraw(); }
                    // buffer recreated + fully repainted on next present(); raw mode clears via fresh zeroed DIB
                    n.present();
                    println!("RESTORED");
                }
                return 0;
            }
            WM_NCHITTEST => return HTCLIENT as LRESULT,
            WM_DESTROY => { PostQuitMessage(0); return 0; }
            _ => {}
        }
    }
    DefWindowProcW(hwnd, msg, wp, lp)
}

fn main() {
    unsafe { SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2); }
    let n: i32 = arg("--notes").and_then(|s| s.parse().ok()).unwrap_or(1);
    let x0: i32 = arg("--x").and_then(|s| s.parse().ok()).unwrap_or(100);
    let y0: i32 = arg("--y").and_then(|s| s.parse().ok()).unwrap_or(100);
    let scale: f32 = arg("--scale").and_then(|s| s.parse().ok()).unwrap_or_else(|| unsafe { GetDpiForSystem() as f32 / 96.0 });
    let ui_kind = arg("--ui").unwrap_or_else(|| "note".into());
    let release = flag("--release-on-minimize");
    if ui_kind != "raw" { platform::install(); }
    let cls = wide("WashiSpikeLayered");
    unsafe {
        let hinst = GetModuleHandleW(null());
        let wc = WNDCLASSW { style: 0, lpfnWndProc: Some(wndproc), cbClsExtra: 0, cbWndExtra: 0, hInstance: hinst, hIcon: null_mut(), hCursor: LoadCursorW(null_mut(), IDC_ARROW), hbrBackground: null_mut(), lpszMenuName: null(), lpszClassName: cls.as_ptr() };
        RegisterClassW(&wc);
        for i in 0..n {
            let (w, h) = ((431.0 * scale) as u32, (451.0 * scale) as u32);
            let hwnd = CreateWindowExW(WS_EX_LAYERED | WS_EX_APPWINDOW | WS_EX_TOPMOST, cls.as_ptr(), wide("washi layered spike").as_ptr(),
                WS_POPUP | WS_MINIMIZEBOX | WS_SYSMENU, x0 + i * (w as i32 + 20), y0, w as i32, h as i32, null_mut(), null_mut(), hinst, null());
            let (win, ui): (Option<Rc<MinimalSoftwareWindow>>, Box<dyn std::any::Any>) = match ui_kind.as_str() {
                "raw" => (None, Box::new(())),
                "bare" => { let win = platform::next_window(); let ui = Bare::new().unwrap(); ui.show().unwrap(); (Some(win), Box::new(ui)) }
                _ => {
                    let win = platform::next_window(); let ui = Note::new().unwrap();
                    ui.set_probe(!flag("--no-probe")); ui.show().unwrap();
                    (Some(win), Box::new(ui))
                }
            };
            if let Some(win) = &win {
                win.dispatch_event(WindowEvent::ScaleFactorChanged { scale_factor: scale });
                win.set_size(PhysicalSize::new(w, h));
            }
            let nw = Rc::new(NoteWin { hwnd, win, dib: RefCell::new(None), px: (w, h), scale, tracking: RefCell::new(false), minimized: RefCell::new(false), release_on_min: release, _ui: ui });
            WINS.with(|v| v.borrow_mut().push(nw.clone()));
            nw.present();
            ShowWindow(hwnd, SW_SHOWNOACTIVATE);
            nw.present();
        }
        let t0 = std::time::Instant::now();
        let min_after = arg("--minimize-after").and_then(|s| s.parse::<u64>().ok());
        let quit_after = arg("--quit-after").and_then(|s| s.parse::<u64>().ok());
        let mut minimized_done = false;
        let mut msg: MSG = std::mem::zeroed();
        'outer: loop {
            while PeekMessageW(&mut msg, null_mut(), 0, 0, PM_REMOVE) != 0 {
                if msg.message == WM_QUIT { break 'outer; }
                TranslateMessage(&msg); DispatchMessageW(&msg);
            }
            if platform_active() { slint::platform::update_timers_and_animations(); }
            WINS.with(|w| for n in w.borrow().iter() { n.present(); });
            let el = t0.elapsed().as_secs();
            if let Some(m) = min_after { if !minimized_done && el >= m { minimized_done = true; WINS.with(|w| for n in w.borrow().iter() { ShowWindow(n.hwnd, SW_MINIMIZE); }); } }
            if let Some(q) = quit_after { if el >= q { break; } }
            MsgWaitForMultipleObjects(0, null(), 0, 16, QS_ALLINPUT);
        }
    }
}
fn platform_active() -> bool { WINS.with(|w| w.borrow().iter().any(|n| n.win.is_some())) }




