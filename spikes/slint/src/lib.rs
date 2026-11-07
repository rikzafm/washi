//! Shared spike plumbing: Slint UI modules and a minimal software-renderer platform
//! that renders into premultiplied BGRA (what UpdateLayeredWindow wants).
slint::include_modules!();

pub mod platform {
    use slint::platform::software_renderer::{
        MinimalSoftwareWindow, PremultipliedRgbaColor, RepaintBufferType, TargetPixel,
    };
    use slint::platform::{Platform, WindowAdapter};
    use std::cell::RefCell;
    use std::rc::Rc;

    #[derive(Clone, Copy, Default)]
    #[repr(C)]
    pub struct Bgra { pub b: u8, pub g: u8, pub r: u8, pub a: u8 }

    impl TargetPixel for Bgra {
        fn blend(&mut self, c: PremultipliedRgbaColor) {
            let ia = 255 - c.alpha as u32;
            self.b = (c.blue as u32 + self.b as u32 * ia / 255) as u8;
            self.g = (c.green as u32 + self.g as u32 * ia / 255) as u8;
            self.r = (c.red as u32 + self.r as u32 * ia / 255) as u8;
            self.a = (c.alpha as u32 + self.a as u32 * ia / 255) as u8;
        }
        fn from_rgb(r: u8, g: u8, b: u8) -> Self { Bgra { b, g, r, a: 255 } }
        fn background() -> Self { Bgra { b: 0, g: 0, r: 0, a: 0 } }
    }

    thread_local! { static PENDING: RefCell<Option<Rc<MinimalSoftwareWindow>>> = RefCell::new(None); }

    struct SpikePlatform { start: std::time::Instant }
    impl Platform for SpikePlatform {
        fn create_window_adapter(&self) -> Result<Rc<dyn WindowAdapter>, slint::PlatformError> {
            PENDING.with(|p| p.borrow_mut().take())
                .map(|w| w as Rc<dyn WindowAdapter>)
                .ok_or_else(|| slint::PlatformError::Other("no pending window".into()))
        }
        fn duration_since_start(&self) -> std::time::Duration { self.start.elapsed() }
    }

    pub fn install() {
        slint::platform::set_platform(Box::new(SpikePlatform { start: std::time::Instant::now() })).unwrap();
    }
    /// Create the window the *next* `Component::new()` will adopt.
    pub fn next_window() -> Rc<MinimalSoftwareWindow> {
        let w = MinimalSoftwareWindow::new(RepaintBufferType::ReusedBuffer);
        PENDING.with(|p| *p.borrow_mut() = Some(w.clone()));
        w
    }
}
