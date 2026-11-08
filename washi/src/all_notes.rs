//! All Notes ("Notes") window and lifecycle controller.

use crate::document::{Document, Kind, PALETTE, Style as DocStyle};
use crate::dropdown::{extract_title_and_preview, format_relative_timestamp};
use crate::note_window::{AllNoteItem, AllNotesWindow, PreviewRun};
use crate::platform::NoteId;
use crate::storage::AllNoteRow;
use crate::textflow;
use slint::ComponentHandle;
use std::cell::{Cell, RefCell};
use std::rc::Rc;

pub const ALL_NOTES_WIDTH: f32 = 603.0;
pub const ALL_NOTES_HEIGHT: f32 = 625.0;

pub const PREVIEW_PAPER_CENTER: (f64, f64) = (412.1, 311.6);
pub const PREVIEW_PAPER_SIZE: (f64, f64) = (334.8, 418.8);
pub const PREVIEW_TILT_DEG: f64 = 1.5;
const MAX_STEP_WIDTH: f64 = 28.0;
const MAX_PREVIEW_V: f64 = 365.0;

struct ViewState {
    rows: RefCell<Vec<AllNoteRow>>,
    now_ms: Cell<i64>,
}

pub fn format_status_line(row: &AllNoteRow, now_ms: i64) -> String {
    let rel = format_relative_timestamp(row.updated_at, now_ms);
    let rel_clean = rel.strip_prefix("· ").unwrap_or(&rel).trim();
    let rel_str = if rel_clean.is_empty() { "today" } else { rel_clean };
    if row.is_open {
        format!("Open on your desktop · edited {rel_str}")
    } else {
        format!("Closed · edited {rel_str}")
    }
}

fn layout_line_runs(
    text: &str,
    u0: f64,
    v: f64,
    size: f64,
    pitch: f64,
    bold: bool,
    italic: bool,
    color: slint::Color,
) -> Vec<PreviewRun> {
    textflow::line_pieces(text, size, MAX_STEP_WIDTH)
        .into_iter()
        .map(|(piece, at, w)| {
            let u = u0 + at;
            let (dx, dy) = textflow::tilt_offset(
                u + w / 2.0,
                v + pitch / 2.0,
                PREVIEW_PAPER_SIZE,
                PREVIEW_TILT_DEG,
            );
            PreviewRun {
                text: piece.into(),
                u: u as f32,
                v: v as f32,
                dx: dx as f32,
                dy: dy as f32,
                width: w as f32,
                size: size as f32,
                bold,
                italic,
                color,
            }
        })
        .collect()
}

fn wrap_text_lines(text: &str, size: f64, max_width: f64) -> Vec<String> {
    if text.is_empty() {
        return vec![String::new()];
    }
    let mut lines = Vec::new();
    let mut current = String::new();
    let mut current_w = 0.0;

    for word in text.split(' ') {
        let space_w = if current.is_empty() {
            0.0
        } else {
            textflow::line_width(" ", size)
        };
        let word_w = textflow::line_width(word, size);
        if !current.is_empty() && current_w + space_w + word_w > max_width + 1e-6 {
            lines.push(std::mem::take(&mut current));
            current_w = 0.0;
        }
        if current.is_empty() {
            current.push_str(word);
            current_w = word_w;
        } else {
            current.push(' ');
            current.push_str(word);
            current_w += space_w + word_w;
        }
    }
    if !current.is_empty() {
        lines.push(current);
    }
    if lines.is_empty() {
        lines.push(String::new());
    }
    lines
}

pub fn layout_preview_runs(content: &str) -> Vec<PreviewRun> {
    let doc = Document::parse(content);
    let blocks = doc.blocks();
    let default_ink = slint::Color::from_rgb_u8(0x2B, 0x2F, 0x36);

    let mut runs = Vec::new();
    let mut v = 48.0;
    let mut prev_group: Option<Kind> = None;

    for b in blocks {
        if b.kind == Kind::Image {
            continue;
        }
        let grp = if b.kind == Kind::ListItem { Kind::Body } else { b.kind };
        if let Some(prev) = prev_group {
            if prev != grp {
                v += if grp == Kind::Heading { 10.0 } else { 12.0 };
            }
        }
        prev_group = Some(grp);

        let (u0, size, pitch, avail, default_bold) = match b.kind {
            Kind::Title => (25.6, 33.2, 46.0, 285.0, false),
            Kind::Heading => (24.2, 16.8, 22.0, 290.0, true),
            Kind::Body | Kind::ListItem | Kind::Image => (24.8, 15.42, 22.0, 275.0, false),
        };

        let block_text = if b.kind == Kind::ListItem && !b.text.is_empty() {
            format!("• {}", b.text)
        } else {
            b.text.clone()
        };

        let common_style = b.common_style(0, b.text.len());
        let bold = default_bold || common_style.contains(DocStyle::BOLD);
        let italic = common_style.contains(DocStyle::ITALIC);
        let ink_color = b
            .common_ink(0, b.text.len())
            .and_then(|idx| PALETTE.get(idx as usize))
            .map(|sw| {
                slint::Color::from_rgb_u8(
                    ((sw.ink >> 16) & 0xFF) as u8,
                    ((sw.ink >> 8) & 0xFF) as u8,
                    (sw.ink & 0xFF) as u8,
                )
            })
            .unwrap_or(default_ink);

        let lines = wrap_text_lines(&block_text, size, avail);
        for line in lines {
            if v >= MAX_PREVIEW_V {
                return runs;
            }
            if !line.is_empty() {
                runs.extend(layout_line_runs(
                    &line, u0, v, size, pitch, bold, italic, ink_color,
                ));
            }
            v += pitch;
        }
    }

    runs
}

fn apply_selected_note(ui: &AllNotesWindow, rows: &[AllNoteRow], selected_id: Option<NoteId>, now_ms: i64) {
    match selected_id.and_then(|id| rows.iter().find(|r| r.id == id)) {
        Some(row) => {
            ui.set_selected_note_id(row.id as i32);
            ui.set_selected_note_open(row.is_open);
            let status = format_status_line(row, now_ms);
            let status_runs = layout_line_runs(
                &status,
                24.5,
                26.0,
                12.65,
                20.0,
                false,
                false,
                slint::Color::from_rgb_u8(0x5B, 0x61, 0x6B),
            );
            let preview_runs = layout_preview_runs(&row.content);
            ui.set_status_text(status.into());
            ui.set_status_runs(slint::ModelRc::from(Rc::new(slint::VecModel::from(status_runs))));
            ui.set_preview_runs(slint::ModelRc::from(Rc::new(slint::VecModel::from(preview_runs))));
        }
        None => {
            ui.set_selected_note_id(-1);
            ui.set_selected_note_open(false);
            ui.set_status_text("".into());
            ui.set_status_runs(slint::ModelRc::default());
            ui.set_preview_runs(slint::ModelRc::default());
        }
    }
}

pub struct AllNotesView {
    ui: AllNotesWindow,
    state: Rc<ViewState>,
}

impl AllNotesView {
    pub fn open(
        initial_selected_id: NoteId,
        rows: &[AllNoteRow],
        now_ms: i64,
        on_close: Rc<dyn Fn()>,
    ) -> Result<Self, slint::PlatformError> {
        let ui = AllNotesWindow::new()?;
        let state = Rc::new(ViewState {
            rows: RefCell::new(rows.to_vec()),
            now_ms: Cell::new(now_ms),
        });

        {
            let on_close = on_close.clone();
            ui.on_close_window(move || {
                on_close();
            });
        }
        {
            let on_close = on_close.clone();
            ui.window().on_close_requested(move || {
                on_close();
                slint::CloseRequestResponse::HideWindow
            });
        }
        {
            let weak_ui = ui.as_weak();
            let state = state.clone();
            ui.on_select_note(move |id| {
                if let Some(ui) = weak_ui.upgrade() {
                    ui.set_confirming_delete(false);
                    apply_selected_note(
                        &ui,
                        &state.rows.borrow(),
                        (id >= 0).then_some(id as NoteId),
                        state.now_ms.get(),
                    );
                }
            });
        }

        let view = Self { ui, state };
        view.refresh(rows, Some(initial_selected_id), now_ms);
        view.ui.show()?;
        Ok(view)
    }

    pub fn select_note(&self, id: NoteId) {
        self.ui.set_confirming_delete(false);
        apply_selected_note(
            &self.ui,
            &self.state.rows.borrow(),
            Some(id),
            self.state.now_ms.get(),
        );
    }

    pub fn invoke_select_note(&self, id: NoteId) {
        self.ui.set_confirming_delete(false);
        self.ui.invoke_select_note(id as i32);
    }

    pub fn invoke_new_note(&self) {
        self.ui.set_confirming_delete(false);
        self.ui.invoke_new_note();
    }

    pub fn invoke_open_or_focus_note(&self) {
        self.ui.set_confirming_delete(false);
        self.ui.invoke_open_or_focus_note();
    }

    pub fn invoke_close_note(&self) {
        self.ui.set_confirming_delete(false);
        self.ui.invoke_close_note();
    }

    pub fn invoke_export_note(&self) {
        self.ui.invoke_export_note();
    }

    pub fn invoke_request_delete(&self) {
        if self.selected_note_id().is_some() {
            self.ui.set_confirming_delete(true);
            self.ui.invoke_request_delete();
        }
    }

    pub fn invoke_cancel_delete(&self) {
        self.ui.set_confirming_delete(false);
        self.ui.invoke_cancel_delete();
    }

    pub fn invoke_confirm_delete(&self) {
        self.ui.invoke_confirm_delete();
    }

    pub fn refresh(&self, rows: &[AllNoteRow], preferred_selected_id: Option<NoteId>, now_ms: i64) {
        *self.state.rows.borrow_mut() = rows.to_vec();
        self.state.now_ms.set(now_ms);

        let mut open_items = Vec::new();
        let mut closed_items = Vec::new();

        for r in rows {
            let (title, _) = extract_title_and_preview(&r.content);
            let item = AllNoteItem {
                id: r.id as i32,
                title: title.into(),
                is_open: r.is_open,
            };
            if r.is_open {
                open_items.push(item);
            } else {
                closed_items.push(item);
            }
        }

        self.ui.set_open_count(open_items.len() as i32);
        self.ui.set_closed_count(closed_items.len() as i32);
        self.ui
            .set_open_notes(slint::ModelRc::from(Rc::new(slint::VecModel::from(open_items))));
        self.ui
            .set_closed_notes(slint::ModelRc::from(Rc::new(slint::VecModel::from(closed_items))));

        let current_selected = self.selected_note_id();
        let target_id = preferred_selected_id
            .filter(|&id| rows.iter().any(|r| r.id == id))
            .or_else(|| current_selected.filter(|&id| rows.iter().any(|r| r.id == id)))
            .or_else(|| rows.first().map(|r| r.id));

        apply_selected_note(&self.ui, rows, target_id, now_ms);
    }

    pub fn ui(&self) -> &AllNotesWindow {
        &self.ui
    }

    pub fn selected_note_id(&self) -> Option<NoteId> {
        let id = self.ui.get_selected_note_id();
        (id >= 0).then_some(id as NoteId)
    }

    pub fn open_count(&self) -> usize {
        self.ui.get_open_count().max(0) as usize
    }

    pub fn closed_count(&self) -> usize {
        self.ui.get_closed_count().max(0) as usize
    }

    pub fn open_notes(&self) -> Vec<AllNoteItem> {
        use slint::Model;
        self.ui.get_open_notes().iter().collect()
    }

    pub fn closed_notes(&self) -> Vec<AllNoteItem> {
        use slint::Model;
        self.ui.get_closed_notes().iter().collect()
    }

    pub fn status_text(&self) -> String {
        self.ui.get_status_text().to_string()
    }

    pub fn preview_runs(&self) -> Vec<PreviewRun> {
        use slint::Model;
        self.ui.get_preview_runs().iter().collect()
    }

    pub fn confirming_delete(&self) -> bool {
        self.ui.get_confirming_delete()
    }

    pub fn close(&self) {
        let _ = self.ui.hide();
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

    pub fn dispatch_event(&self, event: slint::platform::WindowEvent) {
        self.ui.window().dispatch_event(event);
    }
}

impl Drop for AllNotesView {
    fn drop(&mut self) {
        let _ = self.ui.hide();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dropdown::init_test_platform;
    use std::cell::Cell;

    fn sample_rows() -> Vec<AllNoteRow> {
        vec![
            AllNoteRow {
                id: 1,
                content: "Endfield Note\nThermite Cutter".into(),
                updated_at: 1_000,
                is_open: true,
                z_order: 2,
            },
            AllNoteRow {
                id: 2,
                content: "Grocery run\nMilk and eggs".into(),
                updated_at: 900,
                is_open: true,
                z_order: 1,
            },
            AllNoteRow {
                id: 3,
                content: "Meeting ideas\nQ4 roadmap".into(),
                updated_at: 800,
                is_open: false,
                z_order: 0,
            },
            AllNoteRow {
                id: 4,
                content: "Birthday gifts\nBooks".into(),
                updated_at: 700,
                is_open: false,
                z_order: 0,
            },
        ]
    }

    #[test]
    fn select_note_updates_selected_id_open_status_and_resets_confirming_delete() {
        init_test_platform();
        let rows = sample_rows();
        let view = AllNotesView::open(1, &rows, 1_000, Rc::new(|| {})).expect("AllNotesView::open");

        assert_eq!(view.selected_note_id(), Some(1));
        assert!(view.ui().get_selected_note_open());

        view.ui().set_confirming_delete(true);
        assert!(view.confirming_delete());

        // Select a closed note (id = 3)
        view.invoke_select_note(3);
        assert_eq!(view.selected_note_id(), Some(3));
        assert!(!view.ui().get_selected_note_open());
        assert!(!view.confirming_delete(), "selecting a note resets confirming-delete");

        // Select an open note (id = 2)
        view.invoke_select_note(2);
        assert_eq!(view.selected_note_id(), Some(2));
        assert!(view.ui().get_selected_note_open());
    }

    #[test]
    fn new_note_callback_resets_confirming_delete_and_invokes_handler() {
        init_test_platform();
        let rows = sample_rows();
        let view = AllNotesView::open(1, &rows, 1_000, Rc::new(|| {})).expect("AllNotesView::open");

        let new_called = Rc::new(Cell::new(false));
        let nc = new_called.clone();
        view.ui().on_new_note(move || {
            nc.set(true);
        });

        view.ui().set_confirming_delete(true);
        view.invoke_new_note();
        assert!(new_called.get());
        assert!(!view.confirming_delete());
    }

    #[test]
    fn status_text_and_preview_runs_match_mockup_line_breaks_and_update_on_selection() {
        init_test_platform();
        let day_ms = 24 * 60 * 60 * 1000;
        let now_ms = 1_700_000_000_000i64;
        let mockup_content = "# Endfield Note\n## Thermite Cutter : POG & Akekuri\nCurrently POG no Weapon : Ideal for him are Never Rest or White Night Nova. The last can be get in a box";
        let rows = vec![
            AllNoteRow {
                id: 1,
                content: mockup_content.into(),
                updated_at: now_ms,
                is_open: true,
                z_order: 2,
            },
            AllNoteRow {
                id: 2,
                content: "# Meeting ideas\nQ4 roadmap".into(),
                updated_at: now_ms - day_ms,
                is_open: false,
                z_order: 0,
            },
        ];
        let view = AllNotesView::open(1, &rows, now_ms, Rc::new(|| {})).expect("AllNotesView::open");

        assert_eq!(view.status_text(), "Open on your desktop · edited today");

        let runs = view.preview_runs();
        assert!(!runs.is_empty(), "preview_runs should not be empty");

        // Group runs by v coordinate to reconstruct wrapped lines
        let mut lines_by_v: Vec<(f32, Vec<String>, bool, f32)> = Vec::new();
        for r in &runs {
            if let Some(last) = lines_by_v.last_mut().filter(|l| (l.0 - r.v).abs() < 0.5) {
                last.1.push(r.text.to_string());
            } else {
                lines_by_v.push((r.v, vec![r.text.to_string()], r.bold, r.size));
            }
        }
        let line_strings: Vec<String> = lines_by_v.iter().map(|(_, pieces, _, _)| pieces.concat()).collect();
        assert_eq!(
            line_strings,
            vec![
                "Endfield Note",
                "Thermite Cutter : POG & Akekuri",
                "Currently POG no Weapon : Ideal for him are",
                "Never Rest or White Night Nova. The last can",
                "be get in a box",
            ]
        );
        // Title at v=48, size=33
        assert!((lines_by_v[0].0 - 48.0).abs() < 1.0);
        assert!((lines_by_v[0].3 - 33.0).abs() < 0.5);
        // Heading at v=104, bold=true
        assert!((lines_by_v[1].0 - 104.0).abs() < 1.0);
        assert!(lines_by_v[1].2);
        // Body lines at v=138, 160, 182, size=15.5
        assert!((lines_by_v[2].0 - 138.0).abs() < 1.0);
        assert!((lines_by_v[3].0 - 160.0).abs() < 1.0);
        assert!((lines_by_v[4].0 - 182.0).abs() < 1.0);

        // Selecting closed note 2 updates status text and preview runs
        view.invoke_select_note(2);
        assert_eq!(view.status_text(), "Closed · edited yesterday");
        let closed_runs = view.preview_runs();
        let closed_text: String = closed_runs.iter().map(|r| r.text.as_str()).collect();
        assert!(closed_text.contains("Meeting ideas"));
        assert!(closed_text.contains("Q4 roadmap"));
    }

    #[test]
    fn sidebar_renders_headers_tilted_selection_pill_and_handles_row_and_new_note_clicks() {
        let win = slint::platform::software_renderer::MinimalSoftwareWindow::new(
            slint::platform::software_renderer::RepaintBufferType::NewBuffer,
        );
        struct RenderPlatform(Rc<slint::platform::software_renderer::MinimalSoftwareWindow>);
        impl slint::platform::Platform for RenderPlatform {
            fn create_window_adapter(&self) -> Result<Rc<dyn slint::platform::WindowAdapter>, slint::PlatformError> {
                Ok(self.0.clone())
            }
        }
        let _ = slint::platform::set_platform(Box::new(RenderPlatform(win.clone())));

        let rows = vec![
            AllNoteRow { id: 1, content: "Endfield Note\nBody".into(), updated_at: 1000, is_open: true, z_order: 2 },
            AllNoteRow { id: 2, content: "Grocery run\nBody".into(), updated_at: 900, is_open: true, z_order: 1 },
            AllNoteRow { id: 3, content: "Meeting ideas\nBody".into(), updated_at: 800, is_open: false, z_order: 0 },
            AllNoteRow { id: 4, content: "Birthday gifts\nBody".into(), updated_at: 700, is_open: false, z_order: 0 },
            AllNoteRow { id: 5, content: "Reading list\nBody".into(), updated_at: 600, is_open: false, z_order: 0 },
            AllNoteRow { id: 6, content: "Trip to Kyoto\nBody".into(), updated_at: 500, is_open: false, z_order: 0 },
        ];
        let view = AllNotesView::open(1, &rows, 1_000, Rc::new(|| {})).expect("AllNotesView::open");
        win.set_size(slint::PhysicalSize::new(603, 625));

        let render_pixels = || {
            win.request_redraw();
            let mut buf = vec![slint::Rgb8Pixel::default(); 603 * 625];
            win.draw_if_needed(|r| {
                r.render(&mut buf, 603);
            });
            buf
        };

        let buf = render_pixels();
        let px = |buf: &[slint::Rgb8Pixel], x: usize, y: usize| buf[y * 603 + x];
        let has_ink_in = |buf: &[slint::Rgb8Pixel], x0: usize, x1: usize, y0: usize, y1: usize| {
            (y0..=y1).any(|y| (x0..=x1).any(|x| {
                let p = px(buf, x, y);
                p.r < 140 && p.g < 140 && p.b < 150
            }))
        };

        // 1. "Notes" heading ink at x=[33..91], y=[123..147]
        assert!(has_ink_in(&buf, 33, 91, 123, 147), "Notes heading should render dark ink");
        // 2. "2 open · 4 closed" subtitle ink at x=[33..116], y=[166..181]
        assert!(has_ink_in(&buf, 33, 116, 166, 181), "subtitle should render ink");
        // 3. "OPEN" header ink at x=[39..64], y=[199..209]
        assert!(has_ink_in(&buf, 39, 64, 199, 209), "OPEN section header should render ink");
        // 4. "CLOSED" header ink at x=[41..81], y=[305..314]
        assert!(has_ink_in(&buf, 41, 81, 305, 314), "CLOSED section header should render ink");

        // 5. Selected row 0 ("Endfield Note") has white pill (#FFFFFD) around (160, 234)
        let pill_px = px(&buf, 160, 234);
        assert!(
            pill_px.r >= 250 && pill_px.g >= 250 && pill_px.b >= 250,
            "selected row 0 should have white pill at (160, 234), got {pill_px:?}"
        );
        // Unselected row 1 ("Grocery run") at (160, 276) has desk background (#D6DBE2 = 214, 219, 226)
        let desk_px = px(&buf, 160, 276);
        assert_eq!((desk_px.r, desk_px.g, desk_px.b), (214, 219, 226));

        // 6. "+ New note" button at center (83.5, 498.5) has dark pill (#2B2F36 = 43, 47, 54)
        let has_dark_pill = (488..=508).any(|y| (45..=120).any(|x| {
            let p = px(&buf, x, y);
            p.r < 60 && p.g < 65 && p.b < 75
        }));
        assert!(has_dark_pill, "+ New note button should render #2B2F36 dark pill");

        // 7. Clicking closed row "Meeting ideas" at (80, 340) selects note 3 and resets confirming-delete
        view.ui().set_confirming_delete(true);
        let click = |x: f32, y: f32| {
            let position = slint::LogicalPosition::new(x, y);
            let button = slint::platform::PointerEventButton::Left;
            view.dispatch_event(slint::platform::WindowEvent::PointerMoved { position });
            view.dispatch_event(slint::platform::WindowEvent::PointerPressed { position, button });
            view.dispatch_event(slint::platform::WindowEvent::PointerReleased { position, button });
        };
        click(80.0, 340.0);
        assert_eq!(view.selected_note_id(), Some(3));
        assert!(!view.confirming_delete());

        let buf2 = render_pixels();
        let closed_pill_px = px(&buf2, 160, 340);
        assert!(
            closed_pill_px.r >= 250 && closed_pill_px.g >= 250 && closed_pill_px.b >= 250,
            "selected closed row should now have white pill at (160, 340), got {closed_pill_px:?}"
        );

        // 8. Clicking "+ New note" button at (83.5, 498.5) invokes new-note callback
        let new_clicked = Rc::new(Cell::new(false));
        let nc = new_clicked.clone();
        view.ui().on_new_note(move || {
            nc.set(true);
        });
        view.ui().set_confirming_delete(true);
        click(83.5, 498.5);
        assert!(new_clicked.get(), "clicking + New note button should fire new-note callback");
        assert!(!view.confirming_delete());
    }

    #[test]
    fn preview_card_renders_tilted_paper_shadow_tapes_and_typography() {
        let win = slint::platform::software_renderer::MinimalSoftwareWindow::new(
            slint::platform::software_renderer::RepaintBufferType::NewBuffer,
        );
        struct RenderPlatform(Rc<slint::platform::software_renderer::MinimalSoftwareWindow>);
        impl slint::platform::Platform for RenderPlatform {
            fn create_window_adapter(&self) -> Result<Rc<dyn slint::platform::WindowAdapter>, slint::PlatformError> {
                Ok(self.0.clone())
            }
        }
        let _ = slint::platform::set_platform(Box::new(RenderPlatform(win.clone())));

        let mockup_content = "# Endfield Note\n## Thermite Cutter : POG & Akekuri\nCurrently POG no Weapon : Ideal for him are Never Rest or White Night Nova. The last can be get in a box";
        let rows = vec![AllNoteRow {
            id: 1,
            content: mockup_content.into(),
            updated_at: 1_000,
            is_open: true,
            z_order: 1,
        }];
        let _view = AllNotesView::open(1, &rows, 1_000, Rc::new(|| {})).expect("AllNotesView::open");
        win.set_size(slint::PhysicalSize::new(603, 625));

        win.request_redraw();
        let mut buf = vec![slint::Rgb8Pixel::default(); 603 * 625];
        win.draw_if_needed(|r| {
            r.render(&mut buf, 603);
        });

        let px = |x: usize, y: usize| buf[y * 603 + x];

        // 1. Paper center (411, 311) is #FFFFFD
        let center_px = px(411, 311);
        assert!(
            center_px.r >= 252 && center_px.g >= 252 && center_px.b >= 250,
            "paper center (411, 311) should be #FFFFFD, got {center_px:?}"
        );

        // 2. Soft shadow below bottom edge of paper around (411, 526) is darker than desk (#D6DBE2 = 214, 219, 226)
        let shadow_px = px(411, 526);
        assert!(
            shadow_px.r < 214 && shadow_px.g < 219 && shadow_px.b < 226,
            "paper shadow at (411, 526) should be darker than desk, got {shadow_px:?}"
        );

        // 3. Pink tape around (281, 111) has warm pink pixels (r > b + 10)
        let has_pink_tape = (105..=117).any(|y| (265..=295).any(|x| {
            let p = px(x, y);
            p.r as i32 > p.b as i32 + 10 && p.r < 250
        }));
        assert!(has_pink_tape, "pink tape should render around (281, 111)");

        // 4. Blue tape around (545, 519) has cool blue pixels (b > r + 10)
        let has_blue_tape = (512..=525).any(|y| (530..=560).any(|x| {
            let p = px(x, y);
            p.b as i32 > p.r as i32 + 10 && p.r < 245
        }));
        assert!(has_blue_tape, "blue tape should render around (545, 519)");

        // 5. Status line ink around x=[273..450], y=[128..148]
        let has_status_ink = (128..=148).any(|y| (273..=450).any(|x| {
            let p = px(x, y);
            p.r < 160 && p.g < 165 && p.b < 175
        }));
        assert!(has_status_ink, "status line should render ink in top band of preview card");

        // 6. Title ("Endfield Note") ink around x=[273..430], y=[155..190]
        let has_title_ink = (155..=190).any(|y| (273..=430).any(|x| {
            let p = px(x, y);
            p.r < 100 && p.g < 100 && p.b < 110
        }));
        assert!(has_title_ink, "title should render dark ink on preview card");

        // 7. Heading ("Thermite Cutter : POG & Akekuri") ink around x=[272..480], y=[208..228]
        let has_heading_ink = (208..=228).any(|y| (272..=480).any(|x| {
            let p = px(x, y);
            p.r < 100 && p.g < 100 && p.b < 110
        }));
        assert!(has_heading_ink, "heading should render dark ink on preview card");

        // 8. Three body lines ink around y=[240..260], y=[262..282], y=[284..304]
        for (line_idx, (y0, y1)) in [(240usize, 260usize), (262, 282), (284, 304)].into_iter().enumerate() {
            let has_line_ink = (y0..=y1).any(|y| (270..=520).any(|x| {
                let p = px(x, y);
                p.r < 100 && p.g < 100 && p.b < 110
            }));
            assert!(has_line_ink, "body line {line_idx} should render dark ink in y=[{y0}..{y1}]");
        }
    }

    #[test]
    fn bottom_bar_renders_normal_actions_and_delete_confirmation_and_handles_clicks() {
        let win = slint::platform::software_renderer::MinimalSoftwareWindow::new(
            slint::platform::software_renderer::RepaintBufferType::NewBuffer,
        );
        struct RenderPlatform(Rc<slint::platform::software_renderer::MinimalSoftwareWindow>);
        impl slint::platform::Platform for RenderPlatform {
            fn create_window_adapter(&self) -> Result<Rc<dyn slint::platform::WindowAdapter>, slint::PlatformError> {
                Ok(self.0.clone())
            }
        }
        let _ = slint::platform::set_platform(Box::new(RenderPlatform(win.clone())));

        let rows = sample_rows();
        let view = AllNotesView::open(1, &rows, 1_000, Rc::new(|| {})).expect("AllNotesView::open");
        win.set_size(slint::PhysicalSize::new(603, 625));

        let render_pixels = || {
            win.request_redraw();
            let mut buf = vec![slint::Rgb8Pixel::default(); 603 * 625];
            win.draw_if_needed(|r| {
                r.render(&mut buf, 603);
            });
            buf
        };
        let px = |buf: &[slint::Rgb8Pixel], x: usize, y: usize| buf[y * 603 + x];
        let click = |x: f32, y: f32| {
            let position = slint::LogicalPosition::new(x, y);
            let button = slint::platform::PointerEventButton::Left;
            view.dispatch_event(slint::platform::WindowEvent::PointerMoved { position });
            view.dispatch_event(slint::platform::WindowEvent::PointerPressed { position, button });
            view.dispatch_event(slint::platform::WindowEvent::PointerReleased { position, button });
            // Move pointer away so hover state doesn't linger
            view.dispatch_event(slint::platform::WindowEvent::PointerMoved {
                position: slint::LogicalPosition::new(10.0, 10.0),
            });
        };

        let buf = render_pixels();

        // 1. Open/Focus button center is around (279, 489) in window coordinates:
        //    Since note 1 is open, its pill background is #F0F0EE (240, 240, 238) at corner (269, 479)
        let active_pill_px = px(&buf, 269, 479);
        assert!(
            (active_pill_px.r as i32 - 240).abs() <= 4
                && (active_pill_px.g as i32 - 240).abs() <= 4
                && (active_pill_px.b as i32 - 238).abs() <= 4,
            "Open note should have #F0F0EE active pill on button 1 at (269, 479), got {active_pill_px:?}"
        );

        // 2. Delete trash icon around (389, 492) renders crimson ink (#8A1F2B: r > g + 40)
        let has_trash_ink = (483..=501).any(|y| (380..=398).any(|x| {
            let p = px(&buf, x, y);
            p.r as i32 > p.g as i32 + 40 && p.g < 150
        }));
        assert!(has_trash_ink, "Delete trash icon should render crimson #8A1F2B stroke");

        // 3. Selecting a closed note (id = 3) removes the #F0F0EE active pill from button 1
        view.invoke_select_note(3);
        let buf_closed = render_pixels();
        let inactive_pill_px = px(&buf_closed, 269, 479);
        assert!(
            inactive_pill_px.r >= 252 && inactive_pill_px.g >= 252 && inactive_pill_px.b >= 250,
            "Closed note should have #FFFFFD paper background at (269, 479), got {inactive_pill_px:?}"
        );
        view.invoke_select_note(1);

        // 4. Clicking each of the 4 normal buttons fires its callback
        let open_called = Rc::new(Cell::new(0));
        let close_called = Rc::new(Cell::new(0));
        let export_called = Rc::new(Cell::new(0));
        let req_del_called = Rc::new(Cell::new(0));
        let cancel_del_called = Rc::new(Cell::new(0));
        let confirm_del_called = Rc::new(Cell::new(0));

        {
            let c = open_called.clone();
            view.ui().on_open_or_focus_note(move || c.set(c.get() + 1));
        }
        {
            let c = close_called.clone();
            view.ui().on_close_note(move || c.set(c.get() + 1));
        }
        {
            let c = export_called.clone();
            view.ui().on_export_note(move || c.set(c.get() + 1));
        }
        {
            let c = req_del_called.clone();
            view.ui().on_request_delete(move || c.set(c.get() + 1));
        }
        {
            let c = cancel_del_called.clone();
            view.ui().on_cancel_delete(move || c.set(c.get() + 1));
        }
        {
            let c = confirm_del_called.clone();
            view.ui().on_confirm_delete(move || c.set(c.get() + 1));
        }

        click(279.0, 489.0);
        assert_eq!(open_called.get(), 1, "clicking button 1 fires open-or-focus-note");

        click(316.0, 490.0);
        assert_eq!(close_called.get(), 1, "clicking button 2 fires close-note");

        click(352.0, 491.0);
        assert_eq!(export_called.get(), 1, "clicking button 3 fires export-note");

        assert!(!view.confirming_delete());
        click(389.0, 492.0);
        assert_eq!(req_del_called.get(), 1, "clicking button 4 fires request-delete");
        assert!(view.confirming_delete(), "clicking button 4 enters confirming-delete state");

        // 5. In confirming-delete state, "Delete for good?", "Keep" pill (#F0F0EE), and "Delete" pill (#8A1F2B) render
        let buf_del = render_pixels();
        let has_prompt_ink = (480..=505).any(|y| (265..=370).any(|x| {
            let p = px(&buf_del, x, y);
            p.r < 100 && p.g < 100 && p.b < 110
        }));
        assert!(has_prompt_ink, "Delete confirmation prompt 'Delete for good?' should render dark ink");

        let has_crimson_pill = (485..=505).any(|y| (500..=540).any(|x| {
            let p = px(&buf_del, x, y);
            (p.r as i32 - 138).abs() <= 10 && (p.g as i32 - 31).abs() <= 10 && (p.b as i32 - 43).abs() <= 10
        }));
        assert!(has_crimson_pill, "Delete confirmation pill should render #8A1F2B fill");

        // 6. Clicking "Keep" at (457, 495) exits confirming-delete and fires cancel-delete
        click(457.0, 495.0);
        assert!(!view.confirming_delete(), "clicking Keep resets confirming-delete");
        assert_eq!(cancel_del_called.get(), 1, "clicking Keep fires cancel-delete");

        // 7. Re-enter confirming-delete and click "Delete" at (520, 496)
        view.invoke_request_delete();
        assert!(view.confirming_delete());
        click(520.0, 496.0);
        assert_eq!(confirm_del_called.get(), 1, "clicking Delete pill fires confirm-delete");
    }
}

