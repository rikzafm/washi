//! All Notes ("Notes") window and lifecycle controller.

use crate::dropdown::extract_title_and_preview;
use crate::note_window::{AllNoteItem, AllNotesWindow};
use crate::platform::NoteId;
use crate::storage::AllNoteRow;
use slint::ComponentHandle;
use std::rc::Rc;

pub const ALL_NOTES_WIDTH: f32 = 603.0;
pub const ALL_NOTES_HEIGHT: f32 = 625.0;

pub struct AllNotesView {
    ui: AllNotesWindow,
}

impl AllNotesView {
    pub fn open(
        initial_selected_id: NoteId,
        rows: &[AllNoteRow],
        on_close: Rc<dyn Fn()>,
    ) -> Result<Self, slint::PlatformError> {
        let ui = AllNotesWindow::new()?;

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
            ui.on_select_note(move |id| {
                if let Some(ui) = weak_ui.upgrade() {
                    use slint::Model;
                    ui.set_confirming_delete(false);
                    ui.set_selected_note_id(id);
                    let is_open = ui.get_open_notes().iter().any(|item| item.id == id);
                    ui.set_selected_note_open(is_open);
                }
            });
        }

        let view = Self { ui };
        view.refresh(rows, Some(initial_selected_id));
        view.ui.show()?;
        Ok(view)
    }

    pub fn select_note(&self, id: NoteId) {
        use slint::Model;
        self.ui.set_confirming_delete(false);
        self.ui.set_selected_note_id(id as i32);
        let is_open = self.ui.get_open_notes().iter().any(|item| item.id == id as i32);
        self.ui.set_selected_note_open(is_open);
    }

    pub fn invoke_select_note(&self, id: NoteId) {
        self.ui.set_confirming_delete(false);
        self.ui.invoke_select_note(id as i32);
    }

    pub fn invoke_new_note(&self) {
        self.ui.set_confirming_delete(false);
        self.ui.invoke_new_note();
    }

    pub fn refresh(&self, rows: &[AllNoteRow], preferred_selected_id: Option<NoteId>) {
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

        match target_id {
            Some(id) => {
                self.ui.set_selected_note_id(id as i32);
                let is_open = rows.iter().find(|r| r.id == id).is_some_and(|r| r.is_open);
                self.ui.set_selected_note_open(is_open);
            }
            None => {
                self.ui.set_selected_note_id(-1);
                self.ui.set_selected_note_open(false);
            }
        }
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
        let view = AllNotesView::open(1, &rows, Rc::new(|| {})).expect("AllNotesView::open");

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
        let view = AllNotesView::open(1, &rows, Rc::new(|| {})).expect("AllNotesView::open");

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
        let view = AllNotesView::open(1, &rows, Rc::new(|| {})).expect("AllNotesView::open");
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
}

