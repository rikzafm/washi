//! The stand-in Slint platform: no screen, no event loop. Windows are software-rendered
//! on demand and time is whatever the test says it is.
//!
//! A stand-in window behaves like a native one in the ways the app depends on: it remembers its
//! position (the app saves it), and it never becomes smaller than the minimum size the Slint
//! component asks for (Windows enforces that for a real window).

use slint::platform::software_renderer::{RepaintBufferType, SoftwareRenderer};
use slint::platform::{Platform, Renderer, WindowAdapter, WindowProperties};
use slint::{PhysicalPosition, PhysicalSize, Window, WindowPosition, WindowSize};
use std::cell::{Cell, RefCell};
use std::rc::{Rc, Weak};
use std::time::Duration;

thread_local! {
    static INSTALLED: Cell<bool> = const { Cell::new(false) };
    static CLIPBOARD: RefCell<Option<String>> = const { RefCell::new(None) };
    static NOW: Rc<Cell<Duration>> = Rc::new(Cell::new(Duration::ZERO));
    static WINDOWS: RefCell<Vec<Weak<StandInWindow>>> = const { RefCell::new(Vec::new()) };
}

/// One note's window on the stand-in platform.
pub struct StandInWindow {
    window: Window,
    renderer: SoftwareRenderer,
    needs_redraw: Cell<bool>,
    size: Cell<PhysicalSize>,
    position: Cell<PhysicalPosition>,
    /// Minimum size (logical px) the component asked for.
    min: Cell<Option<(f32, f32)>>,
    /// Preferred size (logical px) the component asked for: what a native window opens with
    /// when nobody chose a size.
    preferred: Cell<Option<(f32, f32)>>,
    size_chosen: Cell<bool>,
    /// The scale of the monitor the window is on (its DPI / 96).
    monitor: Cell<f32>,
}

impl StandInWindow {
    fn new() -> Rc<Self> {
        Rc::new_cyclic(|w: &Weak<Self>| StandInWindow {
            window: Window::new(w.clone()),
            renderer: SoftwareRenderer::new_with_repaint_buffer_type(RepaintBufferType::NewBuffer),
            needs_redraw: Cell::new(false),
            size: Cell::new(PhysicalSize::new(0, 0)),
            position: Cell::new(PhysicalPosition::new(0, 0)),
            min: Cell::new(None),
            preferred: Cell::new(None),
            size_chosen: Cell::new(false),
            monitor: Cell::new(1.0),
        })
    }

    /// Draw if the window asked for a repaint (see `MinimalSoftwareWindow::draw_if_needed`).
    pub fn draw_if_needed(&self, render: impl FnOnce(&SoftwareRenderer)) -> bool {
        if self.needs_redraw.replace(false) {
            render(&self.renderer);
            true
        } else {
            false
        }
    }

    /// The smallest size (physical px) this window may have.
    pub fn min_size(&self) -> PhysicalSize {
        let s = self.window.scale_factor();
        let (w, h) = self.min.get().unwrap_or((0.0, 0.0));
        PhysicalSize::new((w * s).ceil() as u32, (h * s).ceil() as u32)
    }

    /// What the operating system does at the end of a drag: place and size the window (the size
    /// is held to the minimum, as Windows does) and tell Slint about the new size.
    pub fn os_place(&self, position: PhysicalPosition, size: PhysicalSize) {
        self.position.set(position);
        self.set_size(WindowSize::Physical(size));
    }
}

impl StandInWindow {
    /// The user dragged the window onto a monitor at `scale` (its DPI / 96), as Windows and winit
    /// report it: Slint is told the monitor's scale, then Windows resizes the window by the ratio
    /// of the two DPIs (its suggested rect) at the same place, and Slint gets the new size. The
    /// minimum Windows holds it to is the one it was last given (screen px, from before the move).
    pub fn os_move_to_monitor(&self, scale: f32) {
        let k = scale / self.monitor.replace(scale);
        let (s, min) = (self.size.get(), self.min_size());
        self.window.dispatch_event(slint::platform::WindowEvent::ScaleFactorChanged { scale_factor: scale });
        let size = PhysicalSize::new(
            ((s.width as f32 * k).round() as u32).max(min.width),
            ((s.height as f32 * k).round() as u32).max(min.height),
        );
        self.size.set(size);
        self.window.dispatch_event(slint::platform::WindowEvent::Resized { size: size.to_logical(scale) });
    }
}

impl WindowAdapter for StandInWindow {
    fn window(&self) -> &Window {
        &self.window
    }
    fn renderer(&self) -> &dyn Renderer {
        &self.renderer
    }
    fn size(&self) -> PhysicalSize {
        self.size.get()
    }
    fn set_visible(&self, visible: bool) -> Result<(), slint::PlatformError> {
        if visible && !self.size_chosen.get() {
            if let Some((w, h)) = self.preferred.get() {
                self.set_size(WindowSize::Logical(slint::LogicalSize::new(w, h)));
            }
        }
        Ok(())
    }
    fn set_size(&self, size: WindowSize) {
        self.size_chosen.set(true);
        let sf = self.window.scale_factor();
        let min = self.min_size();
        // a logical size lands on whole screen px rounded, as on Windows (Slint's own conversion
        // truncates)
        let p = match size {
            WindowSize::Logical(l) => PhysicalSize::new((l.width * sf).round() as u32, (l.height * sf).round() as u32),
            WindowSize::Physical(p) => p,
        };
        let p = PhysicalSize::new(p.width.max(min.width), p.height.max(min.height));
        self.size.set(p);
        self.window.dispatch_event(slint::platform::WindowEvent::Resized { size: p.to_logical(sf) });
    }
    fn position(&self) -> Option<PhysicalPosition> {
        Some(self.position.get())
    }
    fn set_position(&self, position: WindowPosition) {
        let sf = self.window.scale_factor();
        self.position.set(match position {
            WindowPosition::Logical(l) => PhysicalPosition::new((l.x * sf).round() as i32, (l.y * sf).round() as i32),
            WindowPosition::Physical(p) => p,
        });
    }
    fn request_redraw(&self) {
        self.needs_redraw.set(true);
    }
    fn update_window_properties(&self, properties: WindowProperties<'_>) {
        let c = properties.layout_constraints();
        self.min.set(c.min.map(|m| (m.width, m.height)));
        self.preferred.set(Some((c.preferred.width, c.preferred.height)));
    }
}

struct StandIn;

impl Platform for StandIn {
    fn create_window_adapter(&self) -> Result<Rc<dyn WindowAdapter>, slint::PlatformError> {
        let w = StandInWindow::new();
        WINDOWS.with(|ws| ws.borrow_mut().push(Rc::downgrade(&w)));
        Ok(w)
    }

    fn set_clipboard_text(&self, text: &str, clipboard: slint::platform::Clipboard) {
        if clipboard == slint::platform::Clipboard::DefaultClipboard {
            CLIPBOARD.with(|c| *c.borrow_mut() = Some(text.to_string()));
        }
    }

    fn clipboard_text(&self, clipboard: slint::platform::Clipboard) -> Option<String> {
        if clipboard == slint::platform::Clipboard::DefaultClipboard {
            CLIPBOARD.with(|c| c.borrow().clone())
        } else {
            None
        }
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
pub fn windows() -> Vec<Rc<StandInWindow>> {
    WINDOWS.with(|ws| {
        let mut ws = ws.borrow_mut();
        ws.retain(|w| w.strong_count() > 0);
        ws.iter().filter_map(|w| w.upgrade()).collect()
    })
}

/// Put text on the stand-in clipboard.
pub fn set_clipboard(text: &str) {
    CLIPBOARD.with(|c| *c.borrow_mut() = Some(text.to_string()));
}

/// The text on the stand-in clipboard.
pub fn clipboard() -> Option<String> {
    CLIPBOARD.with(|c| c.borrow().clone())
}
