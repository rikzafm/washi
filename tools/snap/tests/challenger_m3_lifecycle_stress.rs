//! Empirical Challenger Lifecycle & Reference Topology Stress Tests
//! Milestone 3: All Notes Seam Encapsulation
//!
//! Specifically tests:
//! 1. Memory leak and reference cycle verification: confirm that closing AllNotesView deallocates
//!    AllNotesView, Editor, and all Slint timers (strong/weak count topologies).
//! 2. 50-cycle repeated open/edit/switch/dismiss lifecycle leak test.
//! 3. Rapid note switching (100 switches across 10 notes) with document state integrity verification.
//! 4. Active note deletion until empty list and recreation from empty state.
//! 5. Timer safety: queued timers firing after AllNotesView deallocation.

use snap::Harness;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::time::Duration;
use washi::all_notes::{AllNotesDelegate, AllNotesView};
use washi::platform::{Clock, ImageClipboard, NoteId, ScreenRect, WindowControl};
use washi::storage::{Db, Geometry};

#[derive(Default)]
struct DummyClipboard {
    text: RefCell<Option<String>>,
}

impl ImageClipboard for DummyClipboard {
    fn image(&self) -> Option<washi::images::Rgba> {
        None
    }
    fn set_image(&self, _img: &washi::images::Rgba) -> Result<(), String> {
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

struct StaticClock;
impl Clock for StaticClock {
    fn unix_millis(&self) -> i64 {
        1_000_000
    }
    fn monotonic(&self) -> Duration {
        Duration::from_secs(10)
    }
}

#[derive(Default)]
struct StaticWindowControl {
    work_areas: Vec<ScreenRect>,
}

impl WindowControl for StaticWindowControl {
    fn work_areas(&self) -> Vec<ScreenRect> {
        if self.work_areas.is_empty() {
            vec![ScreenRect::new(0, 0, 1920, 1080)]
        } else {
            self.work_areas.clone()
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
struct MockSeamDelegate {
    geometries: RefCell<HashMap<NoteId, Geometry>>,
    texts: RefCell<HashMap<NoteId, String>>,
    visible: RefCell<HashMap<NoteId, bool>>,
    open_notes: RefCell<Vec<NoteId>>,
}

impl AllNotesDelegate for MockSeamDelegate {
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
        self.geometries.borrow_mut().insert(id, home);
    }
    fn desktop_note_text(&self, id: NoteId) -> Option<String> {
        self.texts.borrow().get(&id).cloned()
    }
    fn set_desktop_note_text(&self, id: NoteId, text: &str) {
        self.texts.borrow_mut().insert(id, text.to_string());
    }
    fn flush_open_notes(&self) {}
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
    fn delete_note(&self, id: NoteId) {
        self.open_notes.borrow_mut().retain(|&x| x != id);
        self.texts.borrow_mut().remove(&id);
        self.geometries.borrow_mut().remove(&id);
    }
    fn export_note(&self, _id: NoteId) {}
    fn toggle_dropdown(&self, _id: NoteId) {}
    fn dismiss_dropdown(&self) {}
    fn poke_suspend(&self) {}
    fn is_settings_host(&self, _id: NoteId) -> bool {
        false
    }
}

fn create_temp_db() -> (tempfile::TempDir, Rc<Db>) {
    let dir = tempfile::tempdir().expect("tempdir");
    let db = Db::open(&dir.path().join("challenger_lifecycle.db"), &[0x88; 32], 1000).expect("db open");
    (dir, Rc::new(db))
}

// -----------------------------------------------------------------------------
// Test 1: Strict Strong/Weak Reference Topology & Deallocation
// -----------------------------------------------------------------------------
#[test]
fn test_strict_all_notes_and_editor_deallocation() {
    snap::platform::install();
    let (_dir, db) = create_temp_db();
    let clock: Rc<dyn Clock> = Rc::new(StaticClock);
    let wc: Rc<dyn WindowControl> = Rc::new(StaticWindowControl::default());
    let clip: Rc<dyn ImageClipboard> = Rc::new(DummyClipboard::default());
    let delegate = Rc::new(MockSeamDelegate::default());

    let note_id = db.create_note(1000).expect("create note");
    db.save_content(note_id, "# Note 1\nDirect topology test", 1000).unwrap();
    delegate.geometries.borrow_mut().insert(note_id, Geometry { x: 200, y: 200, w: 400, h: 500 });
    delegate.texts.borrow_mut().insert(note_id, "# Note 1\nDirect topology test".to_string());

    let weak_view;
    let weak_editor;
    {
        let view = AllNotesView::open(
            note_id,
            db.clone(),
            clock.clone(),
            slint::Image::default(),
            wc.clone(),
            clip.clone(),
            delegate.clone(),
            Rc::new(|| {}),
        ).expect("AllNotesView::open");

        weak_view = Rc::downgrade(&view);
        weak_editor = Rc::downgrade(view.editor());

        // Check initial state
        assert_eq!(view.editor_note_id(), note_id);
        assert_eq!(view.editor_text(), "# Note 1\nDirect topology test");

        // Type to trigger autosave debounce timer (500ms) and Editor history timer
        view.set_editor_text("# Note 1\nEdited text while open");

        // Check strong counts while alive
        assert_eq!(Rc::strong_count(&view), 1, "Only local handle should hold AllNotesView strongly");

        // Close view
        view.close();

        // Drop local view handle
        drop(view);
    }

    // AllNotesView must drop immediately on handle drop (no cycles keeping view alive)
    assert!(
        weak_view.upgrade().is_none(),
        "AllNotesView leaked! Strong count: {}",
        weak_view.upgrade().map(|v| Rc::strong_count(&v)).unwrap_or(0)
    );
    assert_eq!(weak_view.strong_count(), 0, "AllNotesView strong count must be 0");

    // Advance time past SWAP_OVERLAP (40ms) and HISTORY_PAUSE (500ms) to ensure Slint timers fire and release NoteWindow
    snap::platform::advance(Duration::from_millis(600));

    // Editor must also drop completely
    assert!(
        weak_editor.upgrade().is_none(),
        "Editor leaked! Strong count: {}",
        weak_editor.upgrade().map(|e| Rc::strong_count(&e)).unwrap_or(0)
    );
    assert_eq!(weak_editor.strong_count(), 0, "Editor strong count must be 0");
}

// -----------------------------------------------------------------------------
// Test 2: 50-Cycle Repeated Open/Edit/Switch/Close Lifecycle Leak Stress
// -----------------------------------------------------------------------------
#[test]
fn test_50_cycles_direct_all_notes_open_edit_close_leak_stress() {
    snap::platform::install();
    let (_dir, db) = create_temp_db();
    let clock: Rc<dyn Clock> = Rc::new(StaticClock);
    let wc: Rc<dyn WindowControl> = Rc::new(StaticWindowControl::default());
    let clip: Rc<dyn ImageClipboard> = Rc::new(DummyClipboard::default());
    let delegate = Rc::new(MockSeamDelegate::default());

    // Create 3 notes
    let mut ids = Vec::new();
    for i in 0..3 {
        let id = db.create_note(1000 + i).expect("create note");
        db.save_content(id, &format!("# Note {i}\nBody {i}"), 1000 + i).unwrap();
        delegate.geometries.borrow_mut().insert(id, Geometry { x: 100 + (i as i32 * 50), y: 100, w: 400, h: 500 });
        delegate.texts.borrow_mut().insert(id, format!("# Note {i}\nBody {i}"));
        ids.push(id);
    }

    let mut weak_views = Vec::new();
    let mut weak_editors = Vec::new();

    for cycle in 0..50 {
        let host = ids[cycle % ids.len()];
        let view = AllNotesView::open(
            host,
            db.clone(),
            clock.clone(),
            slint::Image::default(),
            wc.clone(),
            clip.clone(),
            delegate.clone(),
            Rc::new(|| {}),
        ).expect("open");

        weak_views.push(Rc::downgrade(&view));
        weak_editors.push(Rc::downgrade(view.editor()));

        // Switch to next note
        let target = ids[(cycle + 1) % ids.len()];
        view.select_note(target);
        assert_eq!(view.editor_note_id(), target);

        // Edit text
        view.set_editor_text(&format!("# Note {target}\nEdited in cycle {cycle}"));

        // Close and drop
        view.close();
        drop(view);

        // Advance past SWAP_OVERLAP timer
        snap::platform::advance(Duration::from_millis(50));
    }

    // Advance past all debounces
    snap::platform::advance(Duration::from_millis(1000));

    // Verify all 50 views and editors were deallocated with ZERO leaks
    for (i, weak) in weak_views.iter().enumerate() {
        assert!(
            weak.upgrade().is_none(),
            "AllNotesView cycle {i} leaked (strong_count = {})",
            weak.upgrade().map(|v| Rc::strong_count(&v)).unwrap_or(0)
        );
    }
    for (i, weak) in weak_editors.iter().enumerate() {
        assert!(
            weak.upgrade().is_none(),
            "Editor cycle {i} leaked (strong_count = {})",
            weak.upgrade().map(|e| Rc::strong_count(&e)).unwrap_or(0)
        );
    }
}

// -----------------------------------------------------------------------------
// Test 3: Rapid Note Switching (100 switches across 10 notes)
// -----------------------------------------------------------------------------
#[test]
fn test_rapid_100_note_switches_document_integrity() {
    let mut h = Harness::new();
    h.start().unwrap();

    let mut note_ids = Vec::new();
    h.set_note_text(0, "# Master Note 0\nText 0");
    note_ids.push(h.note_id(0));

    for i in 1..10 {
        h.new_note_from(0);
        let idx = h.open_note_count() - 1;
        h.set_note_text(idx, &format!("# Master Note {i}\nText {i}"));
        note_ids.push(h.note_id(idx));
        h.advance(Duration::from_millis(40));
    }

    // Open All Notes
    h.toggle_dropdown(0);
    h.invoke_dropdown_all_notes();
    assert!(h.is_all_notes_open());
    h.advance(Duration::from_millis(100));

    let mut current_texts: HashMap<NoteId, String> = HashMap::new();
    for &id in &note_ids {
        let pos = (0..h.open_note_count()).find(|&i| h.note_id(i) == id).unwrap();
        current_texts.insert(id, h.note_text(pos));
    }

    // Perform 100 rapid note switches
    for step in 0..100 {
        let pick_id = note_ids[step % note_ids.len()];
        h.invoke_all_notes_select_note(pick_id);

        assert_eq!(h.all_notes_selected_id(), Some(pick_id));
        assert_eq!(h.all_notes_editor_note_id(), Some(pick_id));

        // Verify text matches expected
        let displayed = h.all_notes_editor_text().unwrap();
        let expected = current_texts.get(&pick_id).unwrap();
        assert_eq!(&displayed, expected, "Mismatch before edit at step {step}");

        // Interleave edits every 4 steps
        if step % 4 == 0 {
            let updated = format!("# Master Note (Step {step})\nUpdated payload at {step}");
            h.set_all_notes_editor_text(&updated);
            current_texts.insert(pick_id, updated);
        }
    }

    // Debounce autosave
    h.advance(Duration::from_millis(600));

    // Verify all 10 notes have correct data in All Notes
    for &id in &note_ids {
        h.invoke_all_notes_select_note(id);
        let actual = h.all_notes_editor_text().unwrap();
        let expected = current_texts.get(&id).unwrap();
        assert_eq!(&actual, expected, "Note {id} corrupted after rapid switches");
    }

    // Dismiss All Notes
    h.press_all_notes_escape();
    h.advance(Duration::from_millis(100));
    assert!(!h.is_all_notes_open());

    // Verify all 10 desktop notes also match
    for &id in &note_ids {
        let pos = (0..h.open_note_count()).find(|&i| h.note_id(i) == id).unwrap();
        let desktop_actual = h.note_text(pos);
        let expected = current_texts.get(&id).unwrap();
        assert_eq!(&desktop_actual, expected, "Desktop note {id} out of sync");
    }
}

// -----------------------------------------------------------------------------
// Test 4: Active Note Deletion until Empty and Recreation Stress
// -----------------------------------------------------------------------------
#[test]
fn test_delete_active_notes_until_empty_and_recreate() {
    let mut h = Harness::new();
    h.start().unwrap();

    // Create 3 notes
    h.set_note_text(0, "# Initial A\nBody A");
    h.new_note_from(0);
    h.set_note_text(1, "# Initial B\nBody B");
    h.new_note_from(0);
    h.set_note_text(2, "# Initial C\nBody C");
    h.advance(Duration::from_millis(50));

    h.toggle_dropdown(0);
    h.invoke_dropdown_all_notes();
    assert!(h.is_all_notes_open());
    h.advance(Duration::from_millis(100));

    // Delete 1st note while active
    h.invoke_all_notes_request_delete();
    h.invoke_all_notes_confirm_delete();
    h.advance(Duration::from_millis(50));
    assert_eq!(h.all_notes_open_count(), Some(2));
    assert!(h.all_notes_selected_id().is_some());

    // Delete 2nd note while active
    h.invoke_all_notes_request_delete();
    h.invoke_all_notes_confirm_delete();
    h.advance(Duration::from_millis(50));
    assert_eq!(h.all_notes_open_count(), Some(1));
    assert!(h.all_notes_selected_id().is_some());

    // Delete 3rd (last) note while active
    h.invoke_all_notes_request_delete();
    h.invoke_all_notes_confirm_delete();
    h.advance(Duration::from_millis(50));

    // List is now completely empty
    assert_eq!(h.all_notes_open_count(), Some(0));
    assert_eq!(h.all_notes_closed_count(), Some(0));
    assert_eq!(h.all_notes_selected_id(), None);

    // Create new note from empty All Notes view
    h.invoke_all_notes_new_note();
    h.advance(Duration::from_millis(100));

    assert_eq!(h.all_notes_open_count(), Some(1));
    let brand_new_id = h.all_notes_selected_id().expect("brand new note selected");
    assert_eq!(h.all_notes_editor_note_id(), Some(brand_new_id));

    // Type text into the new note
    h.set_all_notes_editor_text("# Reborn Note\nCreated in empty view");
    h.advance(Duration::from_millis(600));

    // Dismiss All Notes
    h.press_all_notes_escape();
    h.advance(Duration::from_millis(100));
    assert!(!h.is_all_notes_open());

    assert_eq!(h.open_note_count(), 1);
    assert_eq!(h.note_text(0), "# Reborn Note\nCreated in empty view");
}

// -----------------------------------------------------------------------------
// Test 5: Timer Safety: Drop View with Pending Timers
// -----------------------------------------------------------------------------
#[test]
fn test_timer_safety_pending_timers_do_not_panic_or_corrupt_after_drop() {
    snap::platform::install();
    let (_dir, db) = create_temp_db();
    let clock: Rc<dyn Clock> = Rc::new(StaticClock);
    let wc: Rc<dyn WindowControl> = Rc::new(StaticWindowControl::default());
    let clip: Rc<dyn ImageClipboard> = Rc::new(DummyClipboard::default());
    let delegate = Rc::new(MockSeamDelegate::default());

    let id = db.create_note(1000).expect("create note");
    db.save_content(id, "# Timer Note\nInitial text", 1000).unwrap();
    delegate.geometries.borrow_mut().insert(id, Geometry { x: 100, y: 100, w: 400, h: 500 });
    delegate.texts.borrow_mut().insert(id, "# Timer Note\nInitial text".to_string());

    let weak_view;
    let weak_editor;
    {
        let view = AllNotesView::open(
            id,
            db.clone(),
            clock.clone(),
            slint::Image::default(),
            wc.clone(),
            clip.clone(),
            delegate.clone(),
            Rc::new(|| {}),
        ).expect("open");

        weak_view = Rc::downgrade(&view);
        weak_editor = Rc::downgrade(view.editor());

        // Queue autosave timer (500ms) and history timer
        view.set_editor_text("# Timer Note\nBurst edit 1");
        view.set_editor_text("# Timer Note\nBurst edit 2");

        // Immediately close and drop WITHOUT waiting for 500ms timer to fire!
        view.close();
        drop(view);
    }

    // At this moment, timers are queued in the Slint mock timer queue.
    // Advancing time must safely fire or cancel the timers without panicking or accessing dropped memory!
    snap::platform::advance(Duration::from_millis(100));
    snap::platform::advance(Duration::from_millis(500));
    snap::platform::advance(Duration::from_millis(1000));

    assert!(weak_view.upgrade().is_none(), "View must be deallocated");
    assert!(weak_editor.upgrade().is_none(), "Editor must be deallocated");
}
