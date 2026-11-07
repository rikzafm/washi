//! The stand-in Slint platform: no screen, no event loop. Windows are software-rendered
//! on demand and time is whatever the test says it is.

use slint::platform::software_renderer::{MinimalSoftwareWindow, RepaintBufferType};
use slint::platform::{Platform, WindowAdapter};
use slint::PhysicalSize;
use std::cell::{Cell, RefCell};
use std::rc::{Rc, Weak};
use std::time::Duration;

pub const WINDOW_SIZE: (u32, u32) = (431, 451);

thread_local! {
    static INSTALLED: Cell<bool> = const { Cell::new(false) };
    static NOW: Rc<Cell<Duration>> = Rc::new(Cell::new(Duration::ZERO));
    static WINDOWS: RefCell<Vec<Weak<MinimalSoftwareWindow>>> = const { RefCell::new(Vec::new()) };
}

struct StandIn;

impl Platform for StandIn {
    fn create_window_adapter(&self) -> Result<Rc<dyn WindowAdapter>, slint::PlatformError> {
        let w = MinimalSoftwareWindow::new(RepaintBufferType::NewBuffer);
        w.set_size(PhysicalSize::new(WINDOW_SIZE.0, WINDOW_SIZE.1));
        WINDOWS.with(|ws| ws.borrow_mut().push(Rc::downgrade(&w)));
        Ok(w)
    }

    fn duration_since_start(&self) -> Duration {
        NOW.with(|n| n.get())
    }
}

/// Install the stand-in platform on this thread (idempotent).
pub fn install() {
    if INSTALLED.with(|i| i.replace(true)) {
        return;
    }
    slint::platform::set_platform(Box::new(StandIn)).expect("a Slint platform was already installed on this thread");
}

/// Shared handle to the fake monotonic time of this thread.
pub fn time_handle() -> Rc<Cell<Duration>> {
    NOW.with(|n| n.clone())
}

/// Advance time and fire due Slint timers.
pub fn advance(by: Duration) {
    NOW.with(|n| n.set(n.get() + by));
    slint::platform::update_timers_and_animations();
}

/// Live windows in creation order.
pub fn windows() -> Vec<Rc<MinimalSoftwareWindow>> {
    WINDOWS.with(|ws| {
        let mut ws = ws.borrow_mut();
        ws.retain(|w| w.strong_count() > 0);
        ws.iter().filter_map(|w| w.upgrade()).collect()
    })
}
