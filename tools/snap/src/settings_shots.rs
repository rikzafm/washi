//! Harness helpers for looking at the Settings page: rendering the window and measuring the Paper.

use crate::shot::Snapshot;
use crate::{platform, Harness};
use slint::platform::software_renderer::PremultipliedRgbaColor;
use slint::platform::WindowAdapter;
use slint::ComponentHandle;

/// The desk the reference images of the Settings page were made on.
pub const DESK: [u8; 3] = [0xD9, 0xDD, 0xE1];

impl Harness {
    /// The Settings window drawn at its own size onto the desk color (its margins are transparent).
    pub fn render_settings(&self) -> Snapshot {
        let ui = self.app().settings().expect("Settings is not open").ui().clone_strong();
        let size = ui.window().size();
        let (pw, ph) = (size.width, size.height);
        let adapter = platform::windows()
            .into_iter()
            .rev()
            .find(|w| w.window().size() == size)
            .expect("the Settings window");
        let mut buf = vec![PremultipliedRgbaColor::default(); (pw * ph) as usize];
        adapter.window().request_redraw();
        adapter.draw_if_needed(|r| {
            r.render(&mut buf, pw as usize);
        });
        let mut rgb = Vec::with_capacity((pw * ph * 3) as usize);
        for p in buf {
            let inv = 255 - p.alpha as u32;
            let over = |src: u8, desk: u8| (src as u32 + (desk as u32 * inv + 127) / 255).min(255) as u8;
            rgb.extend_from_slice(&[over(p.red, DESK[0]), over(p.green, DESK[1]), over(p.blue, DESK[2])]);
        }
        Snapshot::from_rgb(pw, ph, rgb)
    }

    /// Put the values the mockup shows on the General tab (switches, folders, Resource use, an
    /// uninstaller) on the open Settings window. Call it just before rendering: the page refreshes
    /// them from the machine while it runs.
    pub fn settings_show_mockup_values(&mut self) {
        let ui = self.app().settings().expect("Settings is not open").ui().clone_strong();
        ui.set_start_with_windows(true);
        ui.set_new_notes_on_top(false);
        ui.set_install_dir(r"C:\Users\you\AppData\Local\Programs\Washi".into());
        ui.set_data_dir(r"C:\Users\you\AppData\Roaming\Washi".into());
        ui.set_stat_memory("38 MB".into());
        ui.set_stat_cpu("0.1%".into());
        ui.set_stat_db("2.4 MB".into());
        ui.set_stat_notes("4".into());
        ui.set_uninstall_available(true);
    }

    /// Where each tab's content ends (paper px from the Paper's top edge), as laid out.
    /// Indexed by tab (see `snap::tab`).
    pub fn settings_content_bottoms(&self) -> [f32; 5] {
        let ui = self.app().settings().expect("Settings is not open").ui().clone_strong();
        let m = ui.get_content_bottoms();
        use slint::Model;
        std::array::from_fn(|i| m.row_data(i).unwrap())
    }

    /// Move the pointer over the Settings window's top-left corner (off every control).
    pub fn settings_pointer_away(&mut self) {
        let position = slint::LogicalPosition::new(3.0, 3.0);
        self.app().dispatch_settings_event(slint::platform::WindowEvent::PointerMoved { position });
    }

    /// The lowest point the Font or Tape color list reaches, from the Paper's top edge (logical px).
    pub fn settings_style_lists_bottom(&self) -> f32 {
        let ui = self.app().settings().expect("Settings is not open").ui().clone_strong();
        ui.get_style_lists_bottom()
    }

    /// Make the Settings window `by` px shorter than the Paper needs (as a window the app failed to
    /// size would be), to prove the overflow checks can fail.
    pub fn settings_squeeze_window(&mut self, by: f32) {
        let ui = self.app().settings().expect("Settings is not open").ui().clone_strong();
        let size = ui.window().size();
        ui.window().set_size(slint::PhysicalSize::new(size.width, size.height - by as u32));
    }

    /// The Paper height the tab needs, logical px.
    pub fn settings_paper_height_for(&self, tab: i32) -> f32 {
        let ui = self.app().settings().expect("Settings is not open").ui().clone_strong();
        ui.invoke_paper_height_for(tab)
    }
}
