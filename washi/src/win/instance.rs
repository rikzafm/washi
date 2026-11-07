use super::wide;
use crate::platform::{Launch, SingleInstance};
use std::cell::RefCell;
use std::rc::Rc;
use windows_sys::Win32::Foundation::{CloseHandle, GetLastError, ERROR_ALREADY_EXISTS, WAIT_OBJECT_0};
use windows_sys::Win32::System::Threading::{CreateEventW, CreateMutexW, SetEvent, WaitForSingleObject, INFINITE};

const MUTEX_NAME: &str = "Local\\Washi-SingleInstance";
const EVENT_NAME: &str = "Local\\Washi-NewNote";

thread_local! {
    /// Set on the UI thread by `listen`; the waiter thread asks the event loop to run it.
    static ON_REQUEST: RefCell<Option<Rc<dyn Fn()>>> = const { RefCell::new(None) };
}

/// The named "open a new note" event (created if it does not exist yet), or `None` if Windows
/// refused. The caller owns the handle and must close it.
fn new_note_event() -> Option<usize> {
    let name = wide(EVENT_NAME);
    // SAFETY: auto-reset, initially unsignalled; opens the existing event if there is one.
    let handle = unsafe { CreateEventW(std::ptr::null(), 0, 0, name.as_ptr()) };
    if handle.is_null() {
        // SAFETY: plain query.
        eprintln!("washi: could not open the new-note event (error {})", unsafe { GetLastError() });
        None
    } else {
        Some(handle as usize)
    }
}

/// A named mutex marks the running Washi; a named event carries "open a new note" to it.
pub struct Win32Instance;

impl SingleInstance for Win32Instance {
    fn acquire(&self) -> Launch {
        let name = wide(MUTEX_NAME);
        // SAFETY: the mutex handle is deliberately kept for the life of the process: it is what
        // marks this process as the running Washi.
        let (mutex, error) = unsafe {
            let mutex = CreateMutexW(std::ptr::null(), 0, name.as_ptr());
            (mutex, GetLastError())
        };
        if mutex.is_null() {
            // Cannot tell whether another Washi runs: better a second window than no Washi.
            eprintln!("washi: could not create the single-instance mutex (error {error}); starting anyway");
            return Launch::First;
        }
        if error != ERROR_ALREADY_EXISTS {
            return Launch::First;
        }
        // SAFETY: this process only borrows the existing mutex to learn it exists.
        unsafe { CloseHandle(mutex) };
        if let Some(event) = new_note_event() {
            // SAFETY: `event` is a valid event handle that is closed right after signalling.
            unsafe {
                SetEvent(event as _);
                CloseHandle(event as _);
            }
        }
        Launch::HandedOff
    }

    fn listen(&self, on_request: Rc<dyn Fn()>) {
        ON_REQUEST.with(|c| *c.borrow_mut() = Some(on_request));
        let Some(event) = new_note_event() else {
            eprintln!("washi: later launches cannot ask this Washi for a new note");
            return;
        };
        std::thread::spawn(move || {
            loop {
                // SAFETY: the event handle is owned by this thread until it returns.
                let waited = unsafe { WaitForSingleObject(event as _, INFINITE) };
                if waited != WAIT_OBJECT_0 {
                    eprintln!("washi: waiting for new-note requests failed ({waited:#x}); later launches can no longer open a note");
                    break;
                }
                let _ = slint::invoke_from_event_loop(|| {
                    let f = ON_REQUEST.with(|c| c.borrow().clone());
                    if let Some(f) = f {
                        f();
                    }
                });
            }
            // SAFETY: last use of the handle.
            unsafe { CloseHandle(event as _) };
        });
    }
}
