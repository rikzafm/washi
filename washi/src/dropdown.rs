//! Dropdown menu floating window and lifecycle controller.
//!
//! Manages the frameless secondary `DropdownMenuWindow` anchored relative to
//! a note's top-right "..." button.

use crate::document::Document;
use crate::note_window::DropdownMenuWindow;
use crate::platform::NoteId;
use slint::ComponentHandle;
use std::rc::Rc;

/// Format a relative timestamp like `· today`, `· yesterday`, or `· last week`.
pub fn format_relative_timestamp(updated_at_ms: i64, now_ms: i64) -> String {
    use chrono::{Local, TimeZone};
    let updated = match Local.timestamp_millis_opt(updated_at_ms).single() {
        Some(dt) => dt,
        None => return String::new(),
    };
    let now = match Local.timestamp_millis_opt(now_ms).single() {
        Some(dt) => dt,
        None => return String::new(),
    };

    let updated_date = updated.date_naive();
    let now_date = now.date_naive();
    let diff_days = (now_date - updated_date).num_days();

    if diff_days <= 0 {
        "· today".to_string()
    } else if diff_days == 1 {
        "· yesterday".to_string()
    } else if diff_days >= 7 && diff_days < 14 {
        "· last week".to_string()
    } else {
        format!("· {}", updated.format("%b %-d"))
    }
}

/// Longest preview snippet shown in a menu row, in characters (the design cuts at 22).
pub const PREVIEW_CHARS: usize = 22;

/// Collapse every run of whitespace (spaces, tabs, line breaks) to one space.
fn collapse_whitespace(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Cut a snippet to [`PREVIEW_CHARS`] characters and mark the cut with an ellipsis.
pub fn shorten_preview(preview: &str) -> String {
    let flat = collapse_whitespace(preview);
    if flat.chars().count() <= PREVIEW_CHARS {
        return flat;
    }
    let cut: String = flat.chars().take(PREVIEW_CHARS).collect();
    format!("{}…", cut.trim_end())
}

/// Extract title and preview snippet from raw note content.
pub fn extract_title_and_preview(content: &str) -> (String, String) {
    let doc = Document::parse(content);
    let blocks = doc.blocks();
    let title = blocks
        .first()
        .map(|b| collapse_whitespace(&b.text))
        .filter(|t| !t.is_empty())
        .unwrap_or_else(|| "Untitled".to_string());

    let preview = blocks
        .iter()
        .skip(1)
        .map(|b| collapse_whitespace(&b.text))
        .find(|t| !t.is_empty())
        .unwrap_or_default();

    (title, preview)
}

/// A rectangle in logical coordinates.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl Rect {
    pub fn new(x: f32, y: f32, w: f32, h: f32) -> Self {
        Self { x, y, w, h }
    }

    pub fn right(&self) -> f32 {
        self.x + self.w
    }

    pub fn bottom(&self) -> f32 {
        self.y + self.h
    }
}

/// Sizing and layout metrics for the dropdown popup.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DropdownMetrics {
    pub card_width: f32,
    pub card_height: f32,
    pub shadow_margin: f32,
    pub gap: f32,
}

/// Height of the compact card (header 50 + action bar 72), logical px.
pub const COMPACT_CARD_HEIGHT: f32 = 128.0;

impl DropdownMetrics {
    pub fn compact() -> Self {
        Self { card_height: COMPACT_CARD_HEIGHT, ..Self::default() }
    }

    pub fn scaled(&self, scale: f32) -> Self {
        Self {
            card_width: self.card_width * scale,
            card_height: self.card_height * scale,
            shadow_margin: self.shadow_margin * scale,
            gap: self.gap * scale,
        }
    }
}

impl Default for DropdownMetrics {
    fn default() -> Self {
        Self {
            card_width: 268.0,
            card_height: 320.0,
            shadow_margin: 16.0,
            gap: 4.0,
        }
    }
}

/// Calculate the top-left (x, y) coordinates of the dropdown window in logical screen coordinates.
///
/// Anchors the card's top-left near the button (offset slightly left by ~14px so the button
/// sits neatly above the "JUMP TO" header), and places it immediately below the button (`button.bottom() + gap`).
/// If it overflows the bottom of the work area, flips above the button.
/// Clamps horizontally and vertically within the work area if provided.
/// The resulting position is offset by `shadow_margin` so the card aligns with the trigger.
pub fn calculate_dropdown_position(
    button_rect: Rect,
    metrics: &DropdownMetrics,
    work_area: Option<Rect>,
) -> (f32, f32) {
    let offset_x = 14.0 * (metrics.card_width / 268.0);
    let mut card_x = button_rect.x - offset_x;
    let mut card_y = button_rect.bottom() + metrics.gap;

    if let Some(wa) = work_area {
        // If placing below overflows the bottom of work area:
        if card_y + metrics.card_height > wa.bottom() {
            let candidate_y = button_rect.y - metrics.card_height - metrics.gap;
            if candidate_y >= wa.y {
                card_y = candidate_y;
            } else {
                card_y = (wa.bottom() - metrics.card_height).max(wa.y);
            }
        }

        // Clamp card horizontally:
        if card_x + metrics.card_width > wa.right() {
            card_x = wa.right() - metrics.card_width;
        }
        if card_x < wa.x {
            card_x = wa.x;
        }
    }

    (card_x - metrics.shadow_margin, card_y - metrics.shadow_margin)
}

/// Controller managing a live `DropdownMenuWindow`.
pub struct DropdownMenu {
    note_id: NoteId,
    ui: DropdownMenuWindow,
}

impl DropdownMenu {
    pub fn open(
        note_id: NoteId,
        position: (f32, f32),
        owner_hwnd: Option<isize>,
        on_dismiss: Rc<dyn Fn(NoteId)>,
    ) -> Result<Self, slint::PlatformError> {
        Self::open_with(note_id, position, owner_hwnd, on_dismiss, false)
    }

    /// The card of the All Notes editor: no jump list and no "All notes" button, so it is shorter.
    pub fn open_compact(
        note_id: NoteId,
        position: (f32, f32),
        owner_hwnd: Option<isize>,
        on_dismiss: Rc<dyn Fn(NoteId)>,
    ) -> Result<Self, slint::PlatformError> {
        Self::open_with(note_id, position, owner_hwnd, on_dismiss, true)
    }

    fn open_with(
        note_id: NoteId,
        position: (f32, f32),
        owner_hwnd: Option<isize>,
        on_dismiss: Rc<dyn Fn(NoteId)>,
        compact: bool,
    ) -> Result<Self, slint::PlatformError> {
        let ui = DropdownMenuWindow::new()?;
        let scale = crate::ui_scale::WindowScale::attach(ui.window());
        if compact {
            ui.set_compact(true);
            ui.set_card_height(COMPACT_CARD_HEIGHT);
        }
        ui.window().set_position(slint::PhysicalPosition::new(position.0.round() as i32, position.1.round() as i32));

        {
            let on_dismiss = on_dismiss.clone();
            ui.on_dismiss(move || {
                on_dismiss(note_id);
            });
        }
        {
            let on_dismiss = on_dismiss.clone();
            ui.window().on_close_requested(move || {
                on_dismiss(note_id);
                slint::CloseRequestResponse::HideWindow
            });
        }

        ui.show()?;
        scale.resync(ui.window(), true);
        let menu = Self { note_id, ui };

        #[cfg(target_os = "windows")]
        if let Some(owner) = owner_hwnd {
            menu.set_owner(owner);
        }
        menu.ensure_on_top();

        Ok(menu)
    }

    pub fn set_owner(&self, owner_hwnd: isize) {
        #[cfg(target_os = "windows")]
        if let Some(hwnd) = self.hwnd() {
            use windows_sys::Win32::Foundation::HWND;
            use windows_sys::Win32::UI::WindowsAndMessaging::{SetWindowLongPtrW, GWLP_HWNDPARENT};
            unsafe {
                SetWindowLongPtrW(
                    hwnd as HWND,
                    GWLP_HWNDPARENT,
                    owner_hwnd as isize,
                );
            }
        }
    }

    pub fn ensure_on_top(&self) {
        #[cfg(target_os = "windows")]
        if let Some(hwnd) = self.hwnd() {
            use windows_sys::Win32::Foundation::HWND;
            use windows_sys::Win32::UI::WindowsAndMessaging::{
                IsWindowVisible, SetWindowPos, HWND_TOPMOST, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE,
            };
            unsafe {
                let hwnd = hwnd as HWND;
                if IsWindowVisible(hwnd) != 0 {
                    SetWindowPos(
                        hwnd,
                        HWND_TOPMOST,
                        0,
                        0,
                        0,
                        0,
                        SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
                    );
                }
            }
        }
    }

    pub fn note_id(&self) -> NoteId {
        self.note_id
    }

    pub fn ui(&self) -> &DropdownMenuWindow {
        &self.ui
    }

    pub fn pinned(&self) -> bool {
        self.ui.get_pinned()
    }

    pub fn set_pinned(&self, pinned: bool) {
        self.ui.set_pinned(pinned);
    }

    pub fn hwnd(&self) -> Option<isize> {
        use raw_window_handle::{HasWindowHandle, RawWindowHandle};
        let window_handle = self.ui.window().window_handle();
        let handle = window_handle.window_handle().ok()?;
        match handle.as_raw() {
            RawWindowHandle::Win32(w) => Some(w.hwnd.get()),
            _ => None,
        }
    }

    pub fn close(&self) {
        let _ = self.ui.hide();
    }

    pub fn invoke_toggle_pin(&self) {
        self.ui.invoke_toggle_pin();
    }

    pub fn invoke_delete_note(&self) {
        self.ui.invoke_delete_note();
    }

    pub fn invoke_close_note(&self) {
        self.ui.invoke_close_note();
    }

    pub fn invoke_new_note(&self) {
        self.ui.invoke_new_note();
    }

    pub fn invoke_all_notes(&self) {
        self.ui.invoke_all_notes();
    }

    pub fn invoke_settings(&self) {
        self.ui.invoke_settings();
    }

    /// Whether the card offers Settings (not the compact one).
    pub fn settings_visible(&self) -> bool {
        self.ui.get_settings_visible()
    }

    pub fn invoke_select_note(&self, id: NoteId) {
        self.ui.invoke_select_note(id as i32);
    }

    pub fn invoke_dismiss(&self) {
        self.ui.invoke_dismiss();
    }

    pub fn notes(&self) -> Vec<crate::note_window::DropdownNoteItem> {
        use slint::Model;
        self.ui.get_notes().iter().collect()
    }

    pub fn position(&self) -> (f32, f32) {
        let pos = self.ui.window().position();
        let scale = self.ui.window().scale_factor();
        (pos.x as f32 / scale, pos.y as f32 / scale)
    }

    pub fn size(&self) -> (f32, f32) {
        let size = self.ui.window().size();
        let scale = self.ui.window().scale_factor();
        (size.width as f32 / scale, size.height as f32 / scale)
    }

    pub fn dispatch_event(&self, event: slint::platform::WindowEvent) {
        self.ui.window().dispatch_event(event);
    }
}

impl Drop for DropdownMenu {
    fn drop(&mut self) {
        let _ = self.ui.hide();
    }
}

#[cfg(test)]
pub(crate) fn init_test_platform() {
    struct TestPlatform;
    impl slint::platform::Platform for TestPlatform {
        fn create_window_adapter(&self) -> Result<Rc<dyn slint::platform::WindowAdapter>, slint::PlatformError> {
            Ok(slint::platform::software_renderer::MinimalSoftwareWindow::new(
                slint::platform::software_renderer::RepaintBufferType::NewBuffer,
            ))
        }
    }
    let _ = slint::platform::set_platform(Box::new(TestPlatform));
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    #[test]
    fn normal_positioning_anchors_below_and_near_button_left() {
        let metrics = DropdownMetrics::default();
        let button = Rect::new(500.0, 100.0, 36.0, 26.0);
        let work_area = Some(Rect::new(0.0, 0.0, 1920.0, 1080.0));

        let (win_x, win_y) = calculate_dropdown_position(button, &metrics, work_area);
        // button.x = 500.0
        // card_x = 500.0 - 14.0 = 486.0
        // card_y = 100.0 + 26.0 + 4.0 = 130.0
        // win_x = 486.0 - 16.0 = 470.0
        // win_y = 130.0 - 16.0 = 114.0
        assert_eq!((win_x, win_y), (470.0, 114.0));
    }

    #[test]
    fn flips_above_when_overflowing_bottom() {
        let metrics = DropdownMetrics::default();
        let button = Rect::new(500.0, 900.0, 36.0, 26.0);
        let work_area = Some(Rect::new(0.0, 0.0, 1920.0, 1080.0));

        let (win_x, win_y) = calculate_dropdown_position(button, &metrics, work_area);
        // Candidate below: 900 + 26 + 4 + 320 = 1250 > 1080.
        // Candidate above: 900 - 320 - 4 = 576.0 (>= 0).
        // win_x = 470.0
        // win_y = 576.0 - 16.0 = 560.0
        assert_eq!((win_x, win_y), (470.0, 560.0));
    }

    #[test]
    fn clamps_to_work_area_left_edge() {
        let metrics = DropdownMetrics::default();
        let button = Rect::new(5.0, 100.0, 36.0, 26.0);
        let work_area = Some(Rect::new(0.0, 0.0, 1920.0, 1080.0));

        let (win_x, win_y) = calculate_dropdown_position(button, &metrics, work_area);
        // card_x = 5 - 14 = -9 < 0. Clamped to 0.
        // win_x = 0 - 16.0 = -16.0
        // win_y = 100 + 26 + 4 - 16 = 114.0
        assert_eq!((win_x, win_y), (-16.0, 114.0));
    }

    #[test]
    fn clamps_to_work_area_right_edge() {
        let metrics = DropdownMetrics::default();
        let button = Rect::new(700.0, 100.0, 36.0, 26.0);
        let work_area = Some(Rect::new(0.0, 0.0, 800.0, 600.0));

        let (win_x, _win_y) = calculate_dropdown_position(button, &metrics, work_area);
        // card_x would be 700 - 14 = 686. 686 + 268 = 954 > 800.
        // card_x clamped to 800 - 268 = 532.
        // win_x = 532 - 16 = 516.0
        assert_eq!(win_x, 516.0);
    }

    #[test]
    fn no_work_area_calculates_without_clamping() {
        let metrics = DropdownMetrics::default();
        let button = Rect::new(10.0, 900.0, 36.0, 26.0);

        let (win_x, win_y) = calculate_dropdown_position(button, &metrics, None);
        // card_x = 10 - 14 = -4.0
        // win_x = -4.0 - 16.0 = -20.0
        // win_y = 900 + 26 + 4 - 16 = 914.0
        assert_eq!((win_x, win_y), (-20.0, 914.0));
    }

    #[test]
    fn flips_and_clamps_when_work_area_is_small() {
        let metrics = DropdownMetrics::default();
        let button = Rect::new(100.0, 100.0, 28.0, 24.0);
        let work_area = Some(Rect::new(0.0, 0.0, 800.0, 200.0));
        let (_win_x, win_y) = calculate_dropdown_position(button, &metrics, work_area);
        assert_eq!(win_y, -16.0);
    }

    #[test]
    fn dropdown_menu_window_creation_and_dismissal() {
        init_test_platform();
        let ui = DropdownMenuWindow::new().expect("DropdownMenuWindow::new");
        assert!(!ui.get_pinned());
        ui.set_pinned(true);
        assert!(ui.get_pinned());

        let dismissed = Rc::new(Cell::new(false));
        let dismissed_clone = dismissed.clone();
        ui.on_dismiss(move || {
            dismissed_clone.set(true);
        });

        ui.invoke_dismiss();
        assert!(dismissed.get());
    }

    #[test]
    fn dropdown_menu_window_callbacks_and_pinned_property() {
        init_test_platform();
        let ui = DropdownMenuWindow::new().expect("DropdownMenuWindow::new");
        assert!(!ui.get_pinned());
        ui.set_pinned(true);
        assert!(ui.get_pinned());

        let pin_toggled = Rc::new(Cell::new(false));
        let pt = pin_toggled.clone();
        ui.on_toggle_pin(move || {
            pt.set(true);
        });
        ui.invoke_toggle_pin();
        assert!(pin_toggled.get());

        let note_deleted = Rc::new(Cell::new(false));
        let nd = note_deleted.clone();
        ui.on_delete_note(move || {
            nd.set(true);
        });
        ui.invoke_delete_note();
        assert!(note_deleted.get());

        let new_created = Rc::new(Cell::new(false));
        let nc = new_created.clone();
        ui.on_new_note(move || {
            nc.set(true);
        });
        ui.invoke_new_note();
        assert!(new_created.get());

        let all_notes_clicked = Rc::new(Cell::new(false));
        let anc = all_notes_clicked.clone();
        ui.on_all_notes(move || {
            anc.set(true);
        });
        ui.invoke_all_notes();
        assert!(all_notes_clicked.get());
    }

    #[test]
    fn dropdown_menu_pinned_state_helpers() {
        init_test_platform();
        let menu = DropdownMenu::open(
            99,
            (100.0, 200.0),
            None,
            Rc::new(|_| ()),
        )
        .expect("DropdownMenu::open");

        assert!(!menu.pinned());
        menu.set_pinned(true);
        assert!(menu.pinned());
        menu.set_pinned(false);
        assert!(!menu.pinned());
        menu.set_owner(0);
        menu.ensure_on_top();
    }

    #[test]
    fn dropdown_menu_lifecycle_and_note_id() {
        init_test_platform();
        let dismissed = Rc::new(Cell::new(false));
        let dismissed_clone = dismissed.clone();
        let menu = DropdownMenu::open(
            42,
            (100.0, 200.0),
            None,
            Rc::new(move |id| {
                if id == 42 {
                    dismissed_clone.set(true);
                }
            }),
        )
        .expect("DropdownMenu::open");

        assert_eq!(menu.note_id(), 42);
        let all_notes_called = Rc::new(Cell::new(false));
        let anc = all_notes_called.clone();
        menu.ui().on_all_notes(move || {
            anc.set(true);
        });
        menu.invoke_all_notes();
        assert!(all_notes_called.get());
        menu.ui().invoke_dismiss();
        assert!(dismissed.get());
    }

    #[test]
    fn relative_timestamp_formatting() {
        use chrono::{Local, NaiveDate, NaiveDateTime, NaiveTime, TimeZone};
        let date_oct_14 = NaiveDate::from_ymd_opt(2026, 10, 14).unwrap();
        let time = NaiveTime::from_hms_opt(12, 0, 0).unwrap();
        let dt_oct_14 = NaiveDateTime::new(date_oct_14, time);
        let ms_oct_14 = Local.from_local_datetime(&dt_oct_14).unwrap().timestamp_millis();

        let dt_oct_14_later = NaiveDateTime::new(date_oct_14, NaiveTime::from_hms_opt(15, 30, 0).unwrap());
        let ms_today = Local.from_local_datetime(&dt_oct_14_later).unwrap().timestamp_millis();
        assert_eq!(format_relative_timestamp(ms_oct_14, ms_today), "· today");

        let date_oct_15 = NaiveDate::from_ymd_opt(2026, 10, 15).unwrap();
        let dt_oct_15 = NaiveDateTime::new(date_oct_15, time);
        let ms_yesterday = Local.from_local_datetime(&dt_oct_15).unwrap().timestamp_millis();
        assert_eq!(format_relative_timestamp(ms_oct_14, ms_yesterday), "· yesterday");

        let date_oct_17 = NaiveDate::from_ymd_opt(2026, 10, 17).unwrap();
        let dt_oct_17 = NaiveDateTime::new(date_oct_17, time);
        let ms_oct_17 = Local.from_local_datetime(&dt_oct_17).unwrap().timestamp_millis();
        assert_eq!(format_relative_timestamp(ms_oct_14, ms_oct_17), "· Oct 14");
    }

    #[test]
    fn note_title_and_preview_extraction() {
        let content = "Project Ideas\nFirst thought about UI\nSecond idea";
        let (title, preview) = extract_title_and_preview(content);
        assert_eq!(title, "Project Ideas");
        assert_eq!(preview, "First thought about UI");

        let content_md = "# Shopping List\n- Milk\n- Eggs";
        let (title, preview) = extract_title_and_preview(content_md);
        assert_eq!(title, "Shopping List");
        assert_eq!(preview, "Milk");

        let content_no_title = "\nJust a note without title";
        let (title, preview) = extract_title_and_preview(content_no_title);
        assert_eq!(title, "Untitled");
        assert_eq!(preview, "Just a note without title");

        let (title, preview) = extract_title_and_preview("");
        assert_eq!(title, "Untitled");
        assert_eq!(preview, "");
    }

    #[test]
    fn preview_is_flattened_and_cut_with_an_ellipsis() {
        assert_eq!(shorten_preview("short snippet"), "short snippet");
        // Exactly 22 characters stay whole.
        assert_eq!(shorten_preview("First thought about UI"), "First thought about UI");
        assert_eq!(shorten_preview("Ask who owns the launch checklist"), "Ask who owns the launc…");
        // Runs of spaces, tabs and line breaks collapse to a single space.
        assert_eq!(shorten_preview("a   b\t\tc\n d"), "a b c d");
        assert_eq!(
            shorten_preview("sdfsdf sdf v        libopenssl_sys-bfe51"),
            "sdfsdf sdf v libopenss…"
        );
    }

    #[test]
    fn title_and_preview_collapse_whitespace() {
        let (title, preview) = extract_title_and_preview("a  \t b\nx    y");
        assert_eq!(title, "a b");
        assert_eq!(preview, "x y");
    }

    #[test]
    fn dropdown_menu_notes_model_and_select_callback() {
        init_test_platform();
        let ui = DropdownMenuWindow::new().expect("DropdownMenuWindow::new");

        let selected_id = Rc::new(Cell::new(None));
        let sel = selected_id.clone();
        ui.on_select_note(move |id| {
            sel.set(Some(id));
        });

        let items = vec![
            crate::note_window::DropdownNoteItem {
                id: 10,
                title: "First Note".into(),
                preview: "Snippet of note".into(),
                timestamp: "· today".into(),
                has_tape: true,
            },
            crate::note_window::DropdownNoteItem {
                id: 20,
                title: "Second Note".into(),
                preview: "Another preview".into(),
                timestamp: "· yesterday".into(),
                has_tape: false,
            },
        ];
        ui.set_notes(slint::ModelRc::from(Rc::new(slint::VecModel::from(items))));

        ui.invoke_select_note(10);
        assert_eq!(selected_id.get(), Some(10));
    }
}
