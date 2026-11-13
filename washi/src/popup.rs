//! The window every popup (Dropdown menu, right-click menu, Delete dialog) is drawn in.
//!
//! A [`Popup`] owns the lifecycle the three share: it is drawn at the window's own scale (the monitor's
//! times the UI size, put back on top when the window moves to another DPI), it is owned by a Note's
//! window so it stays above it, it is raised above other windows, and it reports when the platform
//! asks to close it. A popup's module keeps only what makes it that popup: its content, where it goes
//! (see `placement`) and what its buttons do.
//!
//! The Win32 calls live here once. Off Windows, and with no owner, they do nothing.

use crate::ui_scale::WindowScale;
use slint::ComponentHandle;
use std::rc::Rc;

/// A Slint window a [`Popup`] can hold: one that tells when its scale factor changes.
pub trait PopupContent: ComponentHandle + 'static {
    fn watch_scale_factor(&self, f: Box<dyn FnMut()>);
}

macro_rules! popup_content {
    ($($window:ty),* $(,)?) => {$(
        impl PopupContent for $window {
            fn watch_scale_factor(&self, f: Box<dyn FnMut()>) {
                self.on_scale_factor_changed(f);
            }
        }
    )*};
}

popup_content!(
    crate::note_window::DropdownMenuWindow,
    crate::note_window::ContextMenuWindow,
    crate::note_window::DeleteNoteDialog,
);

/// The native handle of a window, once it is shown.
pub fn hwnd_of(window: &slint::Window) -> Option<isize> {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    let window_handle = window.window_handle();
    let handle = window_handle.window_handle().ok()?;
    match handle.as_raw() {
        RawWindowHandle::Win32(w) => Some(w.hwnd.get()),
        _ => None,
    }
}

/// A live popup window. Hidden when dropped.
pub struct Popup<C: PopupContent> {
    ui: C,
    scale: Rc<WindowScale>,
}

impl<C: PopupContent> Popup<C> {
    /// Hold `ui` at the app's UI size. Place and size it with [`Popup::scale`] before [`Popup::show`].
    pub fn new(ui: C) -> Self {
        let scale = Rc::new(WindowScale::attach(ui.window()));
        {
            // moved to a monitor with another DPI: put the UI size back on top
            let (weak, scale) = (ui.as_weak(), scale.clone());
            ui.watch_scale_factor(Box::new(move || {
                if let Some(ui) = weak.upgrade() {
                    scale.resync(ui.window(), false);
                }
            }));
        }
        Self { ui, scale }
    }

    pub fn ui(&self) -> &C {
        &self.ui
    }

    pub fn window(&self) -> &slint::Window {
        self.ui.window()
    }

    pub fn scale(&self) -> &Rc<WindowScale> {
        &self.scale
    }

    /// Show the popup above `owner` (a Note's window), keeping the position and logical size it was
    /// given. `on_close` runs when the platform asks to close it.
    pub fn show(&self, owner: Option<isize>, on_close: impl Fn() + 'static) -> Result<(), slint::PlatformError> {
        self.ui.window().on_close_requested(move || {
            on_close();
            slint::CloseRequestResponse::HideWindow
        });
        self.ui.show()?;
        self.scale.resync(self.ui.window(), true);
        if let Some(owner) = owner {
            self.set_owner(owner);
        }
        self.raise();
        Ok(())
    }

    pub fn close(&self) {
        let _ = self.ui.hide();
    }

    pub fn hwnd(&self) -> Option<isize> {
        hwnd_of(self.ui.window())
    }

    pub fn dispatch_event(&self, event: slint::platform::WindowEvent) {
        self.ui.window().dispatch_event(event);
    }

    /// Position in physical px.
    pub fn position(&self) -> (i32, i32) {
        let p = self.ui.window().position();
        (p.x, p.y)
    }

    /// Position in logical px at the window's own scale factor.
    pub fn logical_position(&self) -> (f32, f32) {
        let pos = self.ui.window().position();
        let scale = self.ui.window().scale_factor();
        (pos.x as f32 / scale, pos.y as f32 / scale)
    }

    /// Size in physical px.
    pub fn size(&self) -> (u32, u32) {
        let s = self.ui.window().size();
        (s.width, s.height)
    }

    /// Size in logical px at the window's own scale factor.
    pub fn logical_size(&self) -> (f32, f32) {
        let size = self.ui.window().size();
        let scale = self.ui.window().scale_factor();
        (size.width as f32 / scale, size.height as f32 / scale)
    }

    /// Make `owner_hwnd` the owner of the popup window, so the popup stays above it.
    pub fn set_owner(&self, owner_hwnd: isize) {
        #[cfg(target_os = "windows")]
        if let Some(hwnd) = self.hwnd() {
            use windows_sys::Win32::Foundation::HWND;
            use windows_sys::Win32::UI::WindowsAndMessaging::{SetWindowLongPtrW, GWLP_HWNDPARENT};
            // SAFETY: both handles are live windows of this process.
            unsafe {
                SetWindowLongPtrW(hwnd as HWND, GWLP_HWNDPARENT, owner_hwnd);
            }
        }
        #[cfg(not(target_os = "windows"))]
        let _ = owner_hwnd;
    }

    /// Put the popup above every non-topmost window, without taking focus.
    pub fn raise(&self) {
        #[cfg(target_os = "windows")]
        if let Some(hwnd) = self.hwnd() {
            use windows_sys::Win32::Foundation::HWND;
            use windows_sys::Win32::UI::WindowsAndMessaging::{
                IsWindowVisible, SetWindowPos, HWND_TOPMOST, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE,
            };
            // SAFETY: `hwnd` is this popup's own live window.
            unsafe {
                let hwnd = hwnd as HWND;
                if IsWindowVisible(hwnd) != 0 {
                    SetWindowPos(hwnd, HWND_TOPMOST, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
                }
            }
        }
    }
}

impl<C: PopupContent> Drop for Popup<C> {
    fn drop(&mut self) {
        let _ = self.ui.hide();
    }
}

/// A stand-in platform for tests that open popups: windows that keep their position and size and draw
/// nothing. Set once per test thread; later calls are ignored.
#[cfg(test)]
pub(crate) fn init_test_platform() {
    use slint::platform::software_renderer::{RepaintBufferType, SoftwareRenderer};
    use slint::platform::{Platform, Renderer, WindowAdapter, WindowProperties};
    use slint::{WindowPosition, WindowSize};
    use std::cell::Cell;
    use std::rc::Weak;

    struct TestWindow {
        window: slint::Window,
        renderer: SoftwareRenderer,
        pos: Cell<slint::PhysicalPosition>,
        size: Cell<slint::PhysicalSize>,
    }

    impl TestWindow {
        fn new() -> Rc<Self> {
            Rc::new_cyclic(|w: &Weak<Self>| TestWindow {
                window: slint::Window::new(w.clone()),
                renderer: SoftwareRenderer::new_with_repaint_buffer_type(RepaintBufferType::NewBuffer),
                pos: Cell::new(slint::PhysicalPosition::new(0, 0)),
                size: Cell::new(slint::PhysicalSize::new(0, 0)),
            })
        }
    }

    impl WindowAdapter for TestWindow {
        fn window(&self) -> &slint::Window {
            &self.window
        }
        fn renderer(&self) -> &dyn Renderer {
            &self.renderer
        }
        fn position(&self) -> Option<slint::PhysicalPosition> {
            Some(self.pos.get())
        }
        fn set_position(&self, position: WindowPosition) {
            let sf = self.window.scale_factor();
            self.pos.set(match position {
                WindowPosition::Logical(l) => slint::PhysicalPosition::new((l.x * sf).round() as i32, (l.y * sf).round() as i32),
                WindowPosition::Physical(p) => p,
            });
        }
        fn size(&self) -> slint::PhysicalSize {
            self.size.get()
        }
        fn set_size(&self, size: WindowSize) {
            let sf = self.window.scale_factor();
            let p = match size {
                WindowSize::Logical(l) => slint::PhysicalSize::new((l.width * sf).round() as u32, (l.height * sf).round() as u32),
                WindowSize::Physical(p) => p,
            };
            self.size.set(p);
            self.window.dispatch_event(slint::platform::WindowEvent::Resized { size: p.to_logical(sf) });
        }
        fn set_visible(&self, _visible: bool) -> Result<(), slint::PlatformError> {
            Ok(())
        }
        fn request_redraw(&self) {}
        fn update_window_properties(&self, properties: WindowProperties<'_>) {
            let c = properties.layout_constraints();
            let sf = self.window.scale_factor();
            let p = slint::PhysicalSize::new((c.preferred.width * sf).round() as u32, (c.preferred.height * sf).round() as u32);
            self.size.set(p);
            self.window.dispatch_event(slint::platform::WindowEvent::Resized { size: p.to_logical(sf) });
        }
    }

    struct TestPlatform;
    impl Platform for TestPlatform {
        fn create_window_adapter(&self) -> Result<Rc<dyn WindowAdapter>, slint::PlatformError> {
            Ok(TestWindow::new())
        }
    }
    let _ = slint::platform::set_platform(Box::new(TestPlatform));
}
