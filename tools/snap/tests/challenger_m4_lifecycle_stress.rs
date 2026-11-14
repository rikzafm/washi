//! Empirical Challenger Adversarial Lifecycle & Stress Tests
//! Milestone 4: Final Verification & Hardening
//!
//! Stress tests challenging:
//! 1. Resource lifecycle & leak stress:
//!    - Rapid cycling: Open, switch notes, and dismiss All Notes 50+ times in a tight loop.
//!      Check `Rc::strong_count` and verify no leaked `Editor` or `AllNotesView` instances.
//!    - Rapid Image Viewer cycling: Open and close 50+ Image Viewers rapidly.
//!      Check tracking count and verify deferred deallocation (`slint::Timer::single_shot`)
//!      cleanly drains without memory leaks or handle leaks.
//!    - Editor undo/redo & typing timer stress: Simulate rapid burst typing, pause, undo, redo,
//!      and immediate drop. Verify that `history_timer` weak captures do not leak `Editor` instances.
//! 2. Boundary extremes & fault injection:
//!    - Extreme document inputs: Test `Editor` with empty text, 100,000+ character monolithic strings,
//!      text with rapid alternating formats and markers, and zero-byte or corrupt images.
//!    - Degenerate window placement: Test `ImageViewer` and `AllNotesView` placement with extreme screen coordinates
//!      (negative coordinates, zero-size work areas, multi-monitor boundaries where targets lie outside all monitors).
//!    - Re-entrancy & double-calls: Double-close on Image Viewer, double-dismiss on All Notes, calling `switch_to`
//!      or `select_note` with non-existent NoteId.

use slint::{ComponentHandle, Model};
use snap::fakes::*;
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::{Rc, Weak};
use std::time::Duration;
use washi::all_notes::{clamp_to_screen, AllNotesDelegate, AllNotesView};
use washi::app::{App, Env};
use washi::editor::{Editor, FORMAT_BOLD, FORMAT_ITALIC, FORMAT_LIST, FORMAT_STRIKE, FORMAT_UNDERLINE, START};
use washi::image_viewer::ImageViewer;
use washi::images::{self, Rgba};
use washi::note_window::{Note, NoteWindow};
use washi::placement::{center_over, find_work_area};
use washi::platform::{Clock, ImageClipboard, NoteId, ScreenRect, WindowControl};
use washi::storage::{Db, Geometry};

// =========================================================================
// Mocks and Test Helpers
// =========================================================================

#[derive(Default)]
struct MockClipboard {
    text: RefCell<Option<String>>,
}

impl ImageClipboard for MockClipboard {
    fn image(&self) -> Option<Rgba> {
        None
    }
    fn set_image(&self, _img: &Rgba) -> Result<(), String> {
        Ok(())
    }
    fn set_rich(&self, _payload: washi::platform::ClipboardPayload) -> Result<(), String> {
        Ok(())
    }
    fn html(&self) -> Option<String> {
        None
    }
    fn text(&self) -> Option<String> {
        self.text.borrow().clone()
    }
}

#[derive(Default)]
struct MockWindowControl {
    work_areas: RefCell<Vec<ScreenRect>>,
}

impl WindowControl for MockWindowControl {
    fn work_areas(&self) -> Vec<ScreenRect> {
        let wa = self.work_areas.borrow();
        if wa.is_empty() {
            vec![ScreenRect::new(0, 0, 1920, 1080)]
        } else {
            wa.clone()
        }
    }
    fn begin_move(&self, _id: NoteId, _hwnd: Option<isize>) {}
    fn begin_resize(&self, _id: NoteId, _hwnd: Option<isize>, _edge: washi::platform::ResizeEdge) {}
    fn bring_to_front(&self, _id: NoteId, _hwnd: Option<isize>) {}
    fn place_behind(&self, _id: NoteId, _hwnd: Option<isize>, _behind: Option<isize>) {}
    fn stacking_order(&self, ids: &[(i64, Option<isize>)]) -> Vec<i64> {
        ids.iter().map(|(id, _)| *id).collect()
    }
}

#[derive(Default)]
struct MockDelegate {
    geometries: RefCell<HashMap<NoteId, Geometry>>,
    texts: RefCell<HashMap<NoteId, String>>,
    visible: RefCell<HashMap<NoteId, bool>>,
    open_notes: RefCell<Vec<NoteId>>,
}

impl AllNotesDelegate for MockDelegate {
    fn desktop_note_geometry(&self, id: NoteId) -> Option<Geometry> {
        self.geometries.borrow().get(&id).copied()
    }
    fn desktop_note_hwnd(&self, _id: NoteId) -> Option<isize> {
        Some(100)
    }
    fn set_desktop_note_visible(&self, id: NoteId, visible: bool) {
        self.visible.borrow_mut().insert(id, visible);
    }
    fn park_desktop_note(&self, id: NoteId, at: (i32, i32)) {
        if let Some(g) = self.geometries.borrow_mut().get_mut(&id) {
            g.x = at.0;
            g.y = at.1;
        }
    }
    fn restore_desktop_note(&self, id: NoteId, _at: Geometry, home: Geometry) {
        if let Some(g) = self.geometries.borrow_mut().get_mut(&id) {
            *g = home;
        }
    }
    fn desktop_note_text(&self, id: NoteId) -> Option<String> {
        self.texts.borrow().get(&id).cloned()
    }
    fn set_desktop_note_text(&self, id: NoteId, text: &str) {
        self.texts.borrow_mut().insert(id, text.to_string());
    }
    fn open_desktop_note(&self, id: NoteId) {
        let mut open = self.open_notes.borrow_mut();
        if !open.contains(&id) {
            open.push(id);
        }
    }
    fn close_desktop_note(&self, id: NoteId) {
        self.open_notes.borrow_mut().retain(|&x| x != id);
    }
    fn create_new_note(&self, _from: Option<NoteId>) -> Option<NoteId> {
        None
    }
    fn delete_note(&self, _id: NoteId) {}
    fn export_note(&self, _id: NoteId) {}
    fn flush_open_notes(&self) {}
    fn toggle_dropdown(&self, _id: NoteId) {}
    fn dismiss_dropdown(&self) {}
    fn poke_suspend(&self) {}
    fn is_settings_host(&self, _id: NoteId) -> bool {
        false
    }
}

fn create_test_db() -> (tempfile::TempDir, Rc<Db>) {
    let dir = tempfile::tempdir().expect("tempdir");
    let db = Db::open(&dir.path().join("test.db"), &[0x55; 32], 1000).expect("db open");
    (dir, Rc::new(db))
}

fn make_test_env(dir: &tempfile::TempDir) -> Env {
    Env {
        data_dir: dir.path().to_path_buf(),
        protector: Rc::new(FakeProtector::new(1)),
        clock: Rc::new(FakeClock::new(Rc::new(Cell::new(Duration::from_secs(10))))),
        signals: Rc::new(FakeSignals::default()),
        power: Rc::new(FakePower::default()),
        startup: Rc::new(FakeStartup::default()),
        instance: Rc::new(FakeInstance::default()),
        dialogs: Rc::new(FakeDialogs::default()),
        clipboard: Rc::new(FakeClipboard::default()),
        window_control: Rc::new(FakeWindowControl::default()),
        stats: Rc::new(FakeStats::default()),
        shell: Rc::new(FakeShell::default()),
    }
}

fn make_test_rgba(w: u32, h: u32) -> Rgba {
    Rgba {
        width: w,
        height: h,
        data: vec![255u8; (w * h * 4) as usize],
    }
}

// =========================================================================
// Suite 1: Resource Lifecycle & Leak Stress
// =========================================================================

/// 1.1 Rapid Cycling: Open, switch notes, and dismiss All Notes 60 times in a tight loop.
/// Verifies that Rc::strong_count drops to zero and no Editor or AllNotesView leaks.
#[test]
fn test_all_notes_rapid_open_switch_dismiss_leak_stress() {
    snap::platform::install();
    let (_dir, db) = create_test_db();
    let clock: Rc<dyn Clock> = Rc::new(FakeClock::new(Rc::new(Cell::new(Duration::from_secs(10)))));
    let wc: Rc<dyn WindowControl> = Rc::new(MockWindowControl::default());
    let clip: Rc<dyn ImageClipboard> = Rc::new(MockClipboard::default());
    let delegate = Rc::new(MockDelegate::default());

    // Create 3 notes in DB
    let id1 = db.create_note(1000).expect("note 1");
    let id2 = db.create_note(1001).expect("note 2");
    let id3 = db.create_note(1002).expect("note 3");
    db.save_content(id1, "# Note One\nContent for one", 1000).unwrap();
    db.save_content(id2, "# Note Two\nContent for two", 1001).unwrap();
    db.save_content(id3, "# Note Three\nContent for three", 1002).unwrap();

    delegate.geometries.borrow_mut().insert(id1, Geometry { x: 100, y: 100, w: 400, h: 500 });
    delegate.geometries.borrow_mut().insert(id2, Geometry { x: 150, y: 150, w: 400, h: 500 });
    delegate.geometries.borrow_mut().insert(id3, Geometry { x: 200, y: 200, w: 400, h: 500 });

    const CYCLES: usize = 60;
    for cycle in 0..CYCLES {
        let weak_view: Weak<AllNotesView>;
        let weak_editor: Weak<Editor>;

        {
            let view = AllNotesView::open(
                id1,
                db.clone(),
                clock.clone(),
                slint::Image::default(),
                wc.clone(),
                clip.clone(),
                delegate.clone(),
                Rc::new(|| {}),
            ).expect("AllNotesView::open must succeed");

            weak_view = Rc::downgrade(&view);
            weak_editor = Rc::downgrade(view.editor());

            assert_eq!(view.editor_note_id(), id1);
            assert_eq!(Rc::strong_count(&view), 1, "Only local handle holds AllNotesView");
            assert_eq!(Rc::strong_count(view.editor()), 2, "AllNotesView and NoteWindow UI callbacks hold Editor");

            // Rapidly switch between notes
            view.select_note(id2);
            assert_eq!(view.editor_note_id(), id2);

            view.select_note(id3);
            assert_eq!(view.editor_note_id(), id3);

            view.select_note(id1);
            assert_eq!(view.editor_note_id(), id1);

            // Edit text to trigger autosave and history timers
            view.set_editor_text(&format!("# Note One\nCycle {cycle} edit"));

            // Dismiss view
            view.close();
            drop(view);
        }

        // View must drop immediately upon handle drop
        assert!(
            weak_view.upgrade().is_none(),
            "Cycle {cycle}: AllNotesView leaked! strong_count = {}",
            weak_view.strong_count()
        );
        assert_eq!(weak_view.strong_count(), 0);

        // Advance time past SWAP_OVERLAP (40ms) and HISTORY_PAUSE (500ms)
        snap::platform::advance(Duration::from_millis(600));

        // Editor must be deallocated cleanly without circular reference leak
        assert!(
            weak_editor.upgrade().is_none(),
            "Cycle {cycle}: Editor leaked! strong_count = {}",
            weak_editor.strong_count()
        );
        assert_eq!(weak_editor.strong_count(), 0);
    }
}

/// 1.2 Rapid Image Viewer Cycling: Open and close 60+ Image Viewers rapidly.
/// Verifies tracking collection cleanly drains and deferred deallocation (slint::Timer::single_shot)
/// drains without memory leaks or handle leaks.
#[test]
fn test_image_viewer_rapid_cycling_deferred_deallocation_drain() {
    snap::platform::install();
    let wc: Rc<dyn WindowControl> = Rc::new(MockWindowControl::default());
    let clip: Rc<dyn ImageClipboard> = Rc::new(MockClipboard::default());
    let test_rgba = make_test_rgba(200, 150);

    let viewers: Rc<RefCell<Vec<Rc<ImageViewer>>>> = Rc::new(RefCell::new(Vec::new()));

    // Phase A: 60 sequential open and close cycles
    const SEQUENTIAL_CYCLES: usize = 60;
    for i in 0..SEQUENTIAL_CYCLES {
        let slot: Rc<RefCell<Option<Weak<ImageViewer>>>> = Rc::new(RefCell::new(None));
        let slot_close = slot.clone();
        let viewers_ref = viewers.clone();

        let on_close = Rc::new(move || {
            if let Some(weak_v) = slot_close.borrow_mut().take() {
                if let Some(v) = weak_v.upgrade() {
                    viewers_ref.borrow_mut().retain(|other| !Rc::ptr_eq(other, &v));
                    // Deferred deallocation via Slint single_shot timer
                    slint::Timer::single_shot(Duration::ZERO, move || drop(v));
                }
            }
        });

        let viewer = ImageViewer::open(
            slint::Image::default(),
            (200, 150),
            &format!("Viewer {i}"),
            Some(ScreenRect::new(100, 100, 300, 300)),
            wc.clone(),
            clip.clone(),
            Some(test_rgba.clone()),
            slint::Image::default(),
            on_close,
        ).expect("ImageViewer::open must succeed");

        let v_rc = Rc::new(viewer);
        let weak_v = Rc::downgrade(&v_rc);
        *slot.borrow_mut() = Some(weak_v.clone());
        viewers.borrow_mut().push(v_rc.clone());

        assert_eq!(viewers.borrow().len(), 1, "One active viewer tracked");

        // Close viewer
        v_rc.close();
        drop(v_rc);

        // Immediate removal from tracking list
        assert_eq!(viewers.borrow().len(), 0, "Viewer must be removed from tracking immediately upon close");

        // Advance timers to trigger deferred single_shot drop
        snap::platform::advance(Duration::from_millis(10));

        // Verify viewer is completely deallocated
        assert!(
            weak_v.upgrade().is_none(),
            "Sequential cycle {i}: ImageViewer leaked! strong_count = {}",
            weak_v.strong_count()
        );
        assert_eq!(weak_v.strong_count(), 0);
    }

    // Phase B: Burst of 60 viewers opened simultaneously, then all closed in rapid succession
    const BURST_COUNT: usize = 60;
    let mut burst_weaks = Vec::with_capacity(BURST_COUNT);

    for i in 0..BURST_COUNT {
        let slot: Rc<RefCell<Option<Weak<ImageViewer>>>> = Rc::new(RefCell::new(None));
        let slot_close = slot.clone();
        let viewers_ref = viewers.clone();

        let on_close = Rc::new(move || {
            if let Some(weak_v) = slot_close.borrow_mut().take() {
                if let Some(v) = weak_v.upgrade() {
                    viewers_ref.borrow_mut().retain(|other| !Rc::ptr_eq(other, &v));
                    slint::Timer::single_shot(Duration::ZERO, move || drop(v));
                }
            }
        });

        let viewer = ImageViewer::open(
            slint::Image::default(),
            (200, 150),
            &format!("Burst Viewer {i}"),
            Some(ScreenRect::new(200, 200, 300, 300)),
            wc.clone(),
            clip.clone(),
            Some(test_rgba.clone()),
            slint::Image::default(),
            on_close,
        ).expect("Burst ImageViewer::open must succeed");

        let v_rc = Rc::new(viewer);
        let weak_v = Rc::downgrade(&v_rc);
        *slot.borrow_mut() = Some(weak_v.clone());
        burst_weaks.push(weak_v);
        viewers.borrow_mut().push(v_rc);
    }

    assert_eq!(viewers.borrow().len(), BURST_COUNT, "All 60 burst viewers tracked");

    // Close all 60 viewers in rapid succession without advancing time between closes
    let all_viewers: Vec<Rc<ImageViewer>> = viewers.borrow().clone();
    for v in all_viewers {
        v.close();
    }

    // Tracking list should now be completely empty
    assert_eq!(viewers.borrow().len(), 0, "All viewers removed from tracking list");

    // Drain deferred single_shot timers
    snap::platform::advance(Duration::from_millis(20));

    // Verify all 60 viewers have cleanly dropped
    for (i, weak) in burst_weaks.iter().enumerate() {
        assert!(
            weak.upgrade().is_none(),
            "Burst viewer {i} leaked after drain! strong_count = {}",
            weak.strong_count()
        );
        assert_eq!(weak.strong_count(), 0);
    }
}

/// 1.3 Editor undo/redo & typing timer stress:
/// Simulate rapid burst typing, pause, undo, redo, and immediate drop.
/// Verifies that history_timer weak captures do not leak Editor instances.
#[test]
fn test_editor_undo_redo_typing_timer_weak_capture_lifecycle() {
    snap::platform::install();
    let (_dir, db) = create_test_db();
    let clip: Rc<dyn ImageClipboard> = Rc::new(MockClipboard::default());
    let note_id = db.create_note(1000).expect("create note");

    let weak_editor: Weak<Editor>;

    {
        let note = Note::open(
            note_id,
            "# Initial Title\nFirst paragraph.",
            db.clone(),
            Rc::new(FakeClock::new(Rc::new(Cell::new(Duration::from_secs(10))))),
            None,
            false,
            Rc::new(|_, _| {}),
            Rc::new(|_| false),
            clip.clone(),
        ).expect("Note::open");

        let editor = note.editor().clone();
        weak_editor = Rc::downgrade(&editor);

        // Burst typing: simulate 20 rapid keystroke edits
        for i in 0..20 {
            note.ui().invoke_block_edited(1, format!("Paragraph burst typed {i}").into(), 10 + i);
        }

        // Advance 600ms past the 500ms history timer pause to trigger checkpoint
        snap::platform::advance(Duration::from_millis(600));

        // Verify history state: undo must be available
        assert!(editor.history.borrow().can_undo(), "Checkpoint must have been created");

        // Step undo and redo
        let undo_caret = editor.history_step(false).caret();
        assert!(undo_caret.is_some(), "Undo must succeed");

        let redo_caret = editor.history_step(true).caret();
        assert!(redo_caret.is_some(), "Redo must succeed");

        // Rapid typing again to start the history timer anew
        note.ui().invoke_block_edited(1, "Second burst right before drop".into(), 15);

        // Drop Note and NoteWindow immediately while timer is armed and pending!
        drop(note);
        drop(editor);
    }

    // Editor should be deallocated immediately (weak capture in timer)
    assert!(
        weak_editor.upgrade().is_none(),
        "Editor must deallocate immediately on drop even with active history timer! strong_count = {}",
        weak_editor.strong_count()
    );
    assert_eq!(weak_editor.strong_count(), 0);

    // Advance mock time past the scheduled timer:
    // the timer closure executes against a dead weak pointer without panicking!
    snap::platform::advance(Duration::from_millis(600));

    assert!(weak_editor.upgrade().is_none());
}

// =========================================================================
// Suite 2: Boundary Extremes & Fault Injection
// =========================================================================

/// 2.1 Extreme document inputs:
/// - Empty text `""`
/// - 120,000+ character monolithic string
/// - Rapid alternating formats and markers
#[test]
fn test_extreme_document_inputs_empty_monolithic_and_formatting() {
    snap::platform::install();
    let (_dir, db) = create_test_db();
    let clip: Rc<dyn ImageClipboard> = Rc::new(MockClipboard::default());
    let note_id = db.create_note(1000).expect("create note");

    let ui = NoteWindow::new().expect("NoteWindow");
    ui.window().set_size(slint::LogicalSize::new(447.0, 468.0));

    // A) Empty text
    {
        let editor = Rc::new(Editor::new(note_id, "", db.clone(), clip.clone()));
        editor.bind_ui(&ui, Rc::new(|_| {}), None, None, None);

        assert_eq!(editor.text(), "");
        assert_eq!(editor.model.row_count(), 1, "Empty doc has 1 empty block row");
        assert_eq!(editor.selection().focus, START);

        // Formatting toggles on empty document must not panic
        assert!(!editor.format(FORMAT_BOLD).changed());
        assert!(!editor.format(FORMAT_ITALIC).changed());
        assert!(!editor.format(FORMAT_UNDERLINE).changed());
        assert!(!editor.format(FORMAT_STRIKE).changed());
        assert!(!editor.format(FORMAT_LIST).changed());

        // Undo/redo on empty document
        assert!(editor.history_step(false).caret().is_none());
        assert!(editor.history_step(true).caret().is_none());

        editor.relayout(&ui);
    }

    // B) 120,000-character monolithic string (no newlines)
    {
        let mono_120k: String = format!("# {}", "A".repeat(120_000));
        let editor = Rc::new(Editor::new(note_id, &mono_120k, db.clone(), clip.clone()));
        editor.bind_ui(&ui, Rc::new(|_| {}), None, None, None);

        assert_eq!(editor.model.row_count(), 1);
        assert_eq!(editor.text().len(), 120_002);

        editor.relayout(&ui);

        // Edit at beginning
        editor.edit(0, &format!("PREFIX{}", &mono_120k[2..102]), 6);
        assert!(editor.text().starts_with("# PREFIX"));

        // Edit at end
        let end_idx = editor.text().len();
        editor.edit(0, &format!("{}SUFFIX", editor.text()), end_idx + 6);
        assert!(editor.text().ends_with("SUFFIX"));

        // Verify undo
        editor.checkpoint();
        let _ = editor.history_step(false).caret();
    }

    // C) Rapid alternating formats and markers (1,000 tag pairs)
    {
        let mut complex_doc = String::from("# Complex Markdown Header\n");
        for i in 0..200 {
            complex_doc.push_str(&format!(
                "**bold_{i}** *italic_{i}* ~~strike_{i}~~ <u>under_{i}</u> <c=red>red_{i}</c> <m=blue>mark_{i}</m> "
            ));
        }

        let editor = Rc::new(Editor::new(note_id, &complex_doc, db.clone(), clip.clone()));
        editor.bind_ui(&ui, Rc::new(|_| {}), None, None, None);

        editor.relayout(&ui);
        assert!(editor.runs.row_count() > 500, "Must generate rich styled runs");
        assert!(editor.marker_data.row_count() > 0, "Must calculate marker highlights");

        // Select all to verify ADR-0002 marker wash overlays under selection
        editor.select_all(&ui);
        assert!(editor.marker_wash.row_count() > 0, "Must calculate marker wash overlays under selection");

        // Format change across document
        let _ = editor.format(FORMAT_BOLD);
        editor.relayout(&ui);
    }
}

/// 2.2 Fault injection: Zero-byte and corrupt images.
/// Verifies graceful rejection without crashes or corrupt state.
#[test]
fn test_corrupt_and_zero_byte_image_injection() {
    // 1. Zero-byte image decode
    let zero_res = images::decode(&[]);
    assert!(zero_res.is_err(), "Zero-byte image decode must fail gracefully");

    // 2. Corrupted PNG header
    let corrupt_header = [0x89, 0x50, 0x4E, 0x47, 0x00, 0x00, 0x00, 0xFF, 0xEE];
    let corrupt_res = images::decode(&corrupt_header);
    assert!(corrupt_res.is_err(), "Corrupt PNG header decode must fail gracefully");

    // 3. Truncated random payload
    let random_noise = vec![0x42u8; 1024];
    let noise_res = images::decode(&random_noise);
    assert!(noise_res.is_err(), "Random noise decode must fail gracefully");

    // 4. Editor NoteImages fault resilience
    let (_dir, db) = create_test_db();
    let note_id = db.create_note(1000).expect("create note");
    let clip: Rc<dyn ImageClipboard> = Rc::new(MockClipboard::default());

    // Insert corrupt image into DB directly
    let corrupt_img_id = db.add_image(note_id, "image/png", 100, 100, &corrupt_header).expect("add image");

    let doc_with_corrupt = format!("# Image Note\n![](img:{corrupt_img_id})\nSome text below");
    let editor = Rc::new(Editor::new(note_id, &doc_with_corrupt, db.clone(), clip));

    // Sync model must not panic when decoding corrupt image
    editor.sync_model();

    // Verify block was created and has default image fallback
    assert_eq!(editor.model.row_count(), 3);
    let img_block = editor.model.row_data(1).expect("image block");
    assert_eq!(img_block.img_w, 100);
    assert_eq!(img_block.img_h, 100);
}

/// 2.3 Degenerate window placement:
/// Extreme screen coordinates, negative coordinates, zero-size work areas,
/// and targets outside all monitors.
#[test]
fn test_degenerate_window_placement() {
    // A) Negative coordinates
    let neg_wa = vec![ScreenRect::new(-3840, -1080, 1920, 1080)];
    let far_neg_target = ScreenRect::new(-10000, -10000, 300, 400);

    let found = find_work_area((-5000, -5000), &neg_wa);
    assert_eq!(found, neg_wa.first());

    let placed = center_over(far_neg_target, (400, 500), &neg_wa);
    assert!(placed.x >= neg_wa[0].x);
    assert!(placed.right() <= neg_wa[0].right());
    assert!(placed.y >= neg_wa[0].y);
    assert!(placed.bottom() <= neg_wa[0].bottom());

    let clamped = clamp_to_screen(Geometry { x: -5000, y: -5000, w: 400, h: 500 }, &neg_wa);
    assert_eq!(clamped.x, neg_wa[0].x);
    assert_eq!(clamped.y, neg_wa[0].y);

    // B) Zero-size work areas
    let zero_wa = vec![ScreenRect::new(0, 0, 0, 0)];
    let placed_zero = center_over(ScreenRect::new(100, 100, 200, 200), (300, 300), &zero_wa);
    assert_eq!(placed_zero.w, 300);
    assert_eq!(placed_zero.h, 300);

    let clamped_zero = clamp_to_screen(Geometry { x: 100, y: 100, w: 300, h: 300 }, &zero_wa);
    assert_eq!(clamped_zero.x, 0);
    assert_eq!(clamped_zero.y, 0);

    // Empty work area list: must not panic or index out of bounds
    let empty_wa: Vec<ScreenRect> = vec![];
    let placed_empty = center_over(ScreenRect::new(50, 50, 100, 100), (200, 200), &empty_wa);
    assert_eq!(placed_empty.w, 200);
    assert_eq!(placed_empty.h, 200);

    let clamped_empty = clamp_to_screen(Geometry { x: 50, y: 50, w: 200, h: 200 }, &empty_wa);
    assert_eq!(clamped_empty.x, 50);
    assert_eq!(clamped_empty.y, 50);

    // C) Multi-monitor boundary extremes
    let multi_wa = vec![
        ScreenRect::new(-1920, 0, 1920, 1080),  // Left monitor
        ScreenRect::new(0, 0, 1920, 1080),       // Primary monitor
        ScreenRect::new(1920, -200, 2560, 1440), // High-res Right monitor
    ];

    // Target far outside all monitors (positive extreme)
    let far_pos_target = ScreenRect::new(100_000, 100_000, 400, 400);
    let placed_far_pos = center_over(far_pos_target, (500, 600), &multi_wa);
    assert!(multi_wa.iter().any(|wa| placed_far_pos.x >= wa.x && placed_far_pos.x < wa.right()));

    // Target far outside all monitors (negative extreme)
    let far_neg_target_2 = ScreenRect::new(-100_000, -100_000, 400, 400);
    let placed_far_neg = center_over(far_neg_target_2, (500, 600), &multi_wa);
    assert_eq!(placed_far_neg.w, 500);
    assert_eq!(placed_far_neg.h, 600);

    // Target right on monitor boundary seam
    let seam_target = ScreenRect::new(1920, 0, 100, 100);
    let placed_seam = center_over(seam_target, (400, 400), &multi_wa);
    assert_eq!(placed_seam.w, 400);
    assert_eq!(placed_seam.h, 400);
}

/// 2.4 Re-entrancy & double-calls:
/// - Double-close on ImageViewer
/// - Double-dismiss on AllNotesView and App
/// - Calling select_note and switch_to with non-existent NoteId
#[test]
fn test_reentrancy_and_double_calls() {
    snap::platform::install();
    let wc: Rc<dyn WindowControl> = Rc::new(MockWindowControl::default());
    let clip: Rc<dyn ImageClipboard> = Rc::new(MockClipboard::default());
    let test_rgba = make_test_rgba(100, 100);

    // A) Double-close on ImageViewer
    {
        let close_calls = Rc::new(Cell::new(0));
        let close_calls_clone = close_calls.clone();
        let on_close = Rc::new(move || {
            close_calls_clone.set(close_calls_clone.get() + 1);
        });

        let viewer = ImageViewer::open(
            slint::Image::default(),
            (100, 100),
            "Double Close Test",
            None,
            wc.clone(),
            clip.clone(),
            Some(test_rgba),
            slint::Image::default(),
            on_close,
        ).expect("open");

        // First close call
        viewer.close();
        assert_eq!(close_calls.get(), 1);

        // Immediate second close call (must be safe and not crash)
        viewer.close();
        assert_eq!(close_calls.get(), 2);

        // UI invoke close viewer directly
        viewer.ui().invoke_close_viewer();
        assert_eq!(close_calls.get(), 3);

        // CloseRequested window event dispatch
        viewer.ui().window().dispatch_event(slint::platform::WindowEvent::CloseRequested);
        assert_eq!(close_calls.get(), 4);
    }

    // B) Double-dismiss on All Notes in App
    {
        let dir = tempfile::tempdir().expect("tempdir");
        let env = make_test_env(&dir);
        let app = App::start(env).expect("app start");

        // Open All Notes
        app.open_all_notes(1);
        assert!(app.all_notes().is_some());

        // First dismiss
        app.dismiss_all_notes();
        assert!(app.all_notes().is_none());

        // Immediate second dismiss (must be a clean no-op, no panic)
        app.dismiss_all_notes();
        assert!(app.all_notes().is_none());

        // Advance timers and third dismiss
        snap::platform::advance(Duration::from_millis(600));
        app.dismiss_all_notes();
        assert!(app.all_notes().is_none());
    }

    // C) Non-existent NoteId calls
    {
        let (_dir, db) = create_test_db();
        let clock: Rc<dyn Clock> = Rc::new(FakeClock::new(Rc::new(Cell::new(Duration::from_secs(10)))));
        let delegate = Rc::new(MockDelegate::default());

        let id1 = db.create_note(1000).expect("note 1");
        db.save_content(id1, "# Note 1\nHello", 1000).unwrap();
        delegate.geometries.borrow_mut().insert(id1, Geometry { x: 100, y: 100, w: 400, h: 500 });

        let view = AllNotesView::open(
            id1,
            db.clone(),
            clock.clone(),
            slint::Image::default(),
            wc.clone(),
            clip.clone(),
            delegate.clone(),
            Rc::new(|| {}),
        ).expect("AllNotesView::open");

        // select_note with non-existent positive ID
        view.select_note(999_999);
        // Must fallback safely without crashing
        assert_eq!(view.editor_note_id(), id1);

        // select_note with negative ID
        view.select_note(-1 as NoteId);
        assert_eq!(view.editor_note_id(), id1);

        // select_note with 0
        view.select_note(0);
        assert_eq!(view.editor_note_id(), id1);

        // open_note with non-existent ID
        view.open_note(888_888);

        // Direct Editor switch_to with non-existent NoteId
        let ui = NoteWindow::new().expect("NoteWindow");
        view.editor().switch_to(&ui, 777_777, "# Switched To Non-existent\nBody");
        assert_eq!(view.editor().id(), 777_777);
        assert_eq!(view.editor().text(), "# Switched To Non-existent\nBody");

        view.close();
    }
}

/// 2.5 Degenerate zero dimensions and empty DB states:
/// - Opening ImageViewer with (0, 0) picture size
/// - Opening AllNotesView when note database is completely empty
#[test]
fn test_degenerate_zero_dimensions_and_empty_db_states() {
    snap::platform::install();
    let wc: Rc<dyn WindowControl> = Rc::new(MockWindowControl::default());
    let clip: Rc<dyn ImageClipboard> = Rc::new(MockClipboard::default());

    // 1. ImageViewer with (0, 0) size
    let zero_viewer = ImageViewer::open(
        slint::Image::default(),
        (0, 0),
        "Zero Dimension Picture",
        None,
        wc.clone(),
        clip.clone(),
        None,
        slint::Image::default(),
        Rc::new(|| {}),
    ).expect("ImageViewer with (0, 0) must handle minimum clamping safely");

    let (vw, vh) = zero_viewer.window_size();
    assert!(vw >= 360, "Viewer window width must clamp to MIN_PAPER margin");
    assert!(vh >= 280, "Viewer window height must clamp to MIN_PAPER margin");
    zero_viewer.close();

    // 2. AllNotesView with empty database (0 notes)
    let (_dir, db) = create_test_db();
    let clock: Rc<dyn Clock> = Rc::new(FakeClock::new(Rc::new(Cell::new(Duration::from_secs(10)))));
    let delegate = Rc::new(MockDelegate::default());

    let empty_view = AllNotesView::open(
        0, // ID 0 when DB has no rows
        db.clone(),
        clock.clone(),
        slint::Image::default(),
        wc.clone(),
        clip.clone(),
        delegate.clone(),
        Rc::new(|| {}),
    ).expect("AllNotesView::open on empty DB must succeed without panicking");

    assert_eq!(empty_view.open_count(), 0);
    assert_eq!(empty_view.closed_count(), 0);
    assert_eq!(empty_view.selected_id(), None);
    empty_view.close();
}

