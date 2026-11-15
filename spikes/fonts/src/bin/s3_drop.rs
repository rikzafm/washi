//! S3: does a Slint winit window deliver dropped files to Rust through
//! `WinitWindowAccessor::on_winit_window_event`?
//!
//! A person's drag is simulated in-process: a CF_HDROP data object is made with the shell
//! (a hand-made IDataObject; the shell's own `SHCreateDataObject` was refused by winit's GetData
//! request, effect 0) and handed to the OLE drop target winit registered on the window
//! (found through the `OleDropTargetInterface` window property), calling DragEnter, DragOver and
//! Drop on it exactly as OLE's drag loop would.
//!
//! usage: s3_drop [path-of-an-existing-file]
use raw_window_handle::RawWindowHandle;
use slint::winit_030::{winit, EventResult, WinitWindowAccessor};
use slint::ComponentHandle;
use std::cell::RefCell;
use std::ffi::c_void;
use std::rc::Rc;
use washi_font_spikes::Probe;
use windows_sys::core::GUID;
use windows_sys::Win32::Foundation::HWND;
use windows_sys::Win32::System::Com::{FORMATETC, STGMEDIUM};
use windows_sys::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE};
use windows_sys::Win32::UI::WindowsAndMessaging::GetPropW;

/// A minimal IDataObject offering one CF_HDROP in an HGLOBAL (all winit's drop handler asks for).
#[repr(C)]
struct FileData {
    vtbl: *const DataVtbl,
    path: Vec<u16>,
}

#[repr(C)]
struct DataVtbl {
    query_interface: usize,
    add_ref: usize,
    release: usize,
    get_data: unsafe extern "system" fn(*mut FileData, *const FORMATETC, *mut STGMEDIUM) -> i32,
}

unsafe extern "system" fn get_data(this: *mut FileData, _f: *const FORMATETC, m: *mut STGMEDIUM) -> i32 {
    let me = &*this;
    let bytes = 20 + (me.path.len() + 2) * 2;
    let h = GlobalAlloc(GMEM_MOVEABLE, bytes);
    let p = GlobalLock(h) as *mut u8;
    std::ptr::write_bytes(p, 0, bytes);
    *(p as *mut u32) = 20; // pFiles
    *(p.add(16) as *mut u32) = 1; // fWide
    std::ptr::copy_nonoverlapping(me.path.as_ptr(), p.add(20) as *mut u16, me.path.len());
    GlobalUnlock(h);
    (*m).tymed = 1; // TYMED_HGLOBAL
    (*m).u.hGlobal = h;
    (*m).pUnkForRelease = std::ptr::null_mut();
    0
}

static DATA_VTBL: DataVtbl = DataVtbl { query_interface: 0, add_ref: 0, release: 0, get_data };

#[repr(C)]
#[derive(Clone, Copy)]
struct Pointl {
    x: i32,
    y: i32,
}

#[repr(C)]
struct DropTargetVtbl {
    query_interface: usize,
    add_ref: usize,
    release: usize,
    drag_enter: unsafe extern "system" fn(*mut c_void, *mut c_void, u32, Pointl, *mut u32) -> i32,
    drag_over: unsafe extern "system" fn(*mut c_void, u32, Pointl, *mut u32) -> i32,
    drag_leave: unsafe extern "system" fn(*mut c_void) -> i32,
    drop: unsafe extern "system" fn(*mut c_void, *mut c_void, u32, Pointl, *mut u32) -> i32,
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

fn main() {
    slint::BackendSelector::new().backend_name("winit".into()).renderer_name("software".into()).select().unwrap();
    let file = std::env::args().nth(1).unwrap_or_else(|| {
        let p = std::env::temp_dir().join("washi_s3_drop_me.zip");
        std::fs::write(&p, b"PK dummy").unwrap();
        p.to_string_lossy().into_owned()
    });
    println!("will drop: {file}");

    let p = Probe::new().unwrap();
    let got: Rc<RefCell<Vec<String>>> = Rc::default();
    let g2 = got.clone();
    p.window().on_winit_window_event(move |_w, ev| {
        use winit::event::WindowEvent as E;
        match ev {
            E::DroppedFile(path) => {
                println!("EVENT DroppedFile({})", path.display());
                g2.borrow_mut().push(format!("dropped {}", path.display()));
            }
            E::HoveredFile(path) => {
                println!("EVENT HoveredFile({})", path.display());
                g2.borrow_mut().push("hovered".into());
            }
            E::HoveredFileCancelled => println!("EVENT HoveredFileCancelled"),
            _ => {}
        }
        EventResult::Propagate
    });
    p.show().unwrap();

    let file2 = file.clone();
    let got2 = got.clone();
    let t = slint::Timer::default();
    t.start(slint::TimerMode::SingleShot, std::time::Duration::from_millis(800), move || {
        let probe_win = slint::Window::new; // silence unused
        let _ = probe_win;
        unsafe {
            // find the window
            let hwnd = {
                let weak = WEAK.with(|w| w.borrow().clone()).unwrap();
                let p = weak.upgrade().unwrap();
                let h = p.window().window_handle();
                use raw_window_handle::HasWindowHandle as _;
                match h.window_handle().unwrap().as_raw() {
                    RawWindowHandle::Win32(w) => w.hwnd.get() as HWND,
                    other => panic!("not win32: {other:?}"),
                }
            };
            let name = wide("OleDropTargetInterface");
            let target = GetPropW(hwnd, name.as_ptr()) as *mut *const DropTargetVtbl;
            println!("OleDropTargetInterface property: {}", if target.is_null() { "MISSING (winit did not register a drop target)" } else { "present" });
            if target.is_null() {
                slint::quit_event_loop().unwrap();
                return;
            }
            let mut obj = FileData { vtbl: &DATA_VTBL, path: wide(&file2).into_iter().take_while(|&c| c != 0).collect() };
            let data = &mut obj as *mut FileData as *mut c_void;
            let vt = &**target;
            let pt = Pointl { x: 300, y: 300 };
            let mut effect = 1u32; // DROPEFFECT_COPY
            let hr1 = (vt.drag_enter)(target as *mut c_void, data, 0, pt, &mut effect);
            let hr2 = (vt.drag_over)(target as *mut c_void, 0, pt, &mut effect);
            let hr3 = (vt.drop)(target as *mut c_void, data, 0, pt, &mut effect);
            println!("DragEnter {hr1:x} DragOver {hr2:x} Drop {hr3:x} effect {effect}");
        }
        let got3 = got2.clone();
        slint::Timer::single_shot(std::time::Duration::from_millis(300), move || {
            println!("RESULT events received: {:?}", got3.borrow());
            slint::quit_event_loop().unwrap();
        });
    });
    WEAK.with(|w| *w.borrow_mut() = Some(p.as_weak()));
    slint::run_event_loop().unwrap();
    drop(p);
}

thread_local! { static WEAK: RefCell<Option<slint::Weak<Probe>>> = const { RefCell::new(None) }; }
