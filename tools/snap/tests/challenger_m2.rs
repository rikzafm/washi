//! Empirical Challenger Test Suite for Milestone 2: Note Editor Engine Extraction
//! Tests headless isolation, memory lifecycle, reference cycle freedom, and AllNotes interaction.

use slint::Model;
use snap::Harness;
use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;
use washi::document::{Align, Caret, Range};
use washi::editor::Editor;
use washi::images::{encode_png, Rgba};
use washi::note_window::Note;
use washi::platform::{ClipboardPayload, Clock, ImageClipboard};
use washi::storage::Db;

#[derive(Default)]
struct MockClipboard {
    payload: RefCell<ClipboardPayload>,
}

impl ImageClipboard for MockClipboard {
    fn image(&self) -> Option<Rgba> {
        self.payload.borrow().image.clone()
    }
    fn set_image(&self, img: &Rgba) -> Result<(), String> {
        self.payload.borrow_mut().image = Some(img.clone());
        Ok(())
    }
    fn set_rich(&self, payload: ClipboardPayload) -> Result<(), String> {
        *self.payload.borrow_mut() = payload;
        Ok(())
    }
    fn html(&self) -> Option<String> {
        self.payload.borrow().html.clone()
    }
    fn text(&self) -> Option<String> {
        self.payload.borrow().plain.clone()
    }
}

struct SimpleClock;
impl Clock for SimpleClock {
    fn unix_millis(&self) -> i64 {
        1_000_000
    }
    fn monotonic(&self) -> Duration {
        Duration::from_secs(10)
    }
}

fn create_test_db() -> (tempfile::TempDir, Rc<Db>) {
    let dir = tempfile::tempdir().expect("tempdir");
    let db = Db::open(&dir.path().join("challenger.db"), &[0x55; 32], 1000).expect("db open");
    (dir, Rc::new(db))
}

fn create_sample_image(w: u32, h: u32, fill: u8) -> Rgba {
    Rgba {
        width: w,
        height: h,
        data: vec![fill; (w * h * 4) as usize],
    }
}

// =========================================================================
// 1. Headless Isolation Verification
// =========================================================================

#[test]
fn challenge_headless_editor_operations_without_ui() {
    let (_dir, db) = create_test_db();
    let clip: Rc<dyn ImageClipboard> = Rc::new(MockClipboard::default());
    let note_id = db.create_note(0).expect("create note");

    let initial_content = "# Title Heading\nFirst line of body text.\n- List item 1\n- List item 2";
    let editor = Editor::new(note_id, initial_content, db.clone(), clip.clone());

    // Headless initial verification
    assert_eq!(editor.id(), note_id);
    assert_eq!(editor.text(), initial_content);
    assert!(!editor.is_empty());
    assert_eq!(editor.model.row_count(), 4);

    // Headless editing: change body text
    let caret = editor.edit(1, "Updated body text without UI.", 29);
    assert!(caret.is_none(), "pure text edit within block");
    editor.checkpoint();
    assert_eq!(editor.text(), "# Title Heading\nUpdated body text without UI.\n- List item 1\n- List item 2");

    // Headless undo/redo
    assert!(editor.can_undo());
    let undo_caret = editor.step(false).expect("undo step");
    assert_eq!(undo_caret.block, 1);
    assert_eq!(editor.text(), initial_content);

    let redo_caret = editor.step(true).expect("redo step");
    assert_eq!(redo_caret.block, 1);
    assert_eq!(editor.text(), "# Title Heading\nUpdated body text without UI.\n- List item 1\n- List item 2");

    // Headless format toggling
    editor.set_caret(Caret { block: 1, offset: 0 });
    let bold_range = Range {
        anchor: Caret { block: 1, offset: 0 },
        focus: Caret { block: 1, offset: 7 },
    };
    editor.selection.set(bold_range);
    assert!(editor.format(washi::editor::FORMAT_BOLD));
    assert!(editor.text().contains("**Updated**"));

    // Headless paint application (ink 2, marker 1)
    let paint_range = Range {
        anchor: Caret { block: 1, offset: 8 },
        focus: Caret { block: 1, offset: 12 },
    };
    editor.selection.set(paint_range);
    assert!(editor.apply_paint(Some(2), Some(1)));
    let text_with_paint = editor.text();
    assert!(text_with_paint.contains("<c=") && text_with_paint.contains("<m="), "serialized document contains ink and marker tags: {text_with_paint}");

    // Headless clipboard copy & replace
    assert!(editor.copy_range(&clip));
    assert!(clip.text().is_some());

    let rep_caret = editor.replace_range("REPLACED");
    assert_eq!(rep_caret.block, 1);
    assert!(editor.text().contains("REPLACED"));

    // Headless notify_change (must not crash when ui is None)
    editor.notify_change(Some(Caret { block: 1, offset: 5 }));

    // Headless paper size & scale manipulation
    editor.set_paper_size(500.0, 600.0);
    assert_eq!(editor.paper.get(), (500.0, 600.0));
    editor.set_scale_factor(1.5);
    assert_eq!(editor.scale_factor.get(), 1.5);

    // Headless layout tokens check
    let tokens = editor.default_tokens();
    assert_eq!(tokens.inset_left, 32.0);
    assert!(tokens.body.pitch > 0.0);
}

#[test]
fn challenge_headless_image_operations() {
    let (_dir, db) = create_test_db();
    let clip: Rc<dyn ImageClipboard> = Rc::new(MockClipboard::default());
    let note_id = db.create_note(0).expect("create note");

    let img = create_sample_image(120, 80, 180);
    let png = encode_png(&img).expect("encode png");
    let image_id = db.add_image(note_id, "image/png", 120, 80, &png).expect("add image");

    let content = format!("# Image Note\n![](img:{image_id})\nText below");
    let editor = Editor::new(note_id, &content, db.clone(), clip.clone());

    assert_eq!(editor.model.row_count(), 3);
    assert_eq!(editor.image_state(1), Some((100, Align::Left)));

    // Modify image pct and alignment headlessly
    editor.set_image_pct(1, 60);
    editor.set_image_align(1, Align::Center);
    assert_eq!(editor.image_state(1), Some((60, Align::Center)));

    // Decoded bytes check
    let bytes = editor.decoded_image_bytes();
    assert!(bytes > 0, "image should have decoded pixel bytes");

    // Headless delete image
    let del_ok = editor.delete_image(1);
    assert!(del_ok);
    assert_eq!(editor.model.row_count(), 2);

    // Headless undo image deletion
    let caret = editor.step(false);
    assert!(caret.is_some());
    assert_eq!(editor.doc.borrow().len(), 3);
    editor.sync_model();
    assert_eq!(editor.model.row_count(), 3);
    assert_eq!(editor.image_state(1), Some((60, Align::Center)));
}

// =========================================================================
// 2. Reference Cycle & Memory Lifecycle Resistance
// =========================================================================

#[test]
fn challenge_editor_deallocation_when_note_dropped_without_typing() {
    snap::platform::install();
    let (_dir, db) = create_test_db();
    let clip: Rc<dyn ImageClipboard> = Rc::new(MockClipboard::default());
    let note_id = db.create_note(0).expect("create note");

    let weak_editor;
    {
        let note = Note::open(
            note_id,
            "# Transient Note\nSome content to be dropped",
            db.clone(),
            Rc::new(SimpleClock),
            None,
            false,
            Rc::new(|_, _| {}),
            Rc::new(|_| false),
            clip.clone(),
        ).expect("Note::open");

        weak_editor = Rc::downgrade(note.editor());
        assert!(weak_editor.upgrade().is_some(), "Editor is alive while Note is open");
    }

    // After dropping Note without edits, weak_editor must not upgrade
    assert!(weak_editor.upgrade().is_none(), "Editor must be deallocated when Note is dropped without active edits");
}

#[test]
fn challenge_editor_circular_reference_leak_on_typing() {
    snap::platform::install();
    let (_dir, db) = create_test_db();
    let clip: Rc<dyn ImageClipboard> = Rc::new(MockClipboard::default());
    let note_id = db.create_note(0).expect("create note");

    let weak_editor;
    {
        let note = Note::open(
            note_id,
            "# Pending Timer Note\nInitial text",
            db.clone(),
            Rc::new(SimpleClock),
            None,
            false,
            Rc::new(|_, _| {}),
            Rc::new(|_| false),
            clip.clone(),
        ).expect("Note::open");

        weak_editor = Rc::downgrade(note.editor());

        // Trigger on_block_edited callback on UI (which sets history_timer with editor_clone)
        note.ui().invoke_block_edited(1, "Typed text".into(), 10);

        // Note is closed/dropped here
    }

    // Advance mock time past the 500ms history debounce timer
    snap::platform::advance(Duration::from_millis(600));

    // Due to the reference cycle in washi/src/editor.rs:304 (editor_clone: Rc<Editor> captured in history_timer),
    // weak_editor CANNOT be deallocated and remains leaked permanently in memory!
    assert!(
        weak_editor.upgrade().is_none(),
        "CRITICAL LEAK: Editor failed to deallocate after window close due to circular reference cycle in history_timer (strong_count = {})",
        weak_editor.upgrade().map(|e| Rc::strong_count(&e)).unwrap_or(0)
    );
}

#[test]
fn challenge_repeated_note_lifecycle_memory_stability() {
    let mut h = Harness::new();
    h.start().unwrap();

    // Repeatedly create, edit, and close notes 20 times
    for i in 0..20 {
        h.press_ctrl(0, 'n');
        let idx = h.open_note_count() - 1;
        h.type_text(idx, &format!("Rapid cycle iteration {i}\nBody line"));
        h.advance(Duration::from_millis(600));
        h.press_ctrl(idx, 'w');
    }

    // Verify window count returned to baseline
    assert_eq!(h.open_note_count(), 1, "Only initial root note remains open");
}

#[test]
fn challenge_editor_switch_to_lifecycle() {
    snap::platform::install();
    let (_dir, db) = create_test_db();
    let clip: Rc<dyn ImageClipboard> = Rc::new(MockClipboard::default());
    let id1 = db.create_note(0).expect("note 1");
    let id2 = db.create_note(0).expect("note 2");

    let note = Note::open(
        id1,
        "# Note 1 Content",
        db.clone(),
        Rc::new(SimpleClock),
        None,
        false,
        Rc::new(|_, _| {}),
        Rc::new(|_| false),
        clip.clone(),
    ).expect("Note::open");

    assert_eq!(note.editor().id(), id1);
    assert_eq!(note.editor().text(), "# Note 1 Content");

    // Switch note 1 to note 2
    note.switch_to(id2, "# Note 2 Switched Content\nNew line");
    assert_eq!(note.editor().id(), id2);
    assert_eq!(note.editor().text(), "# Note 2 Switched Content\nNew line");
    assert_eq!(note.editor().model.row_count(), 2);

    // Switch back to note 1
    note.switch_to(id1, "# Note 1 Restored");
    assert_eq!(note.editor().id(), id1);
    assert_eq!(note.editor().text(), "# Note 1 Restored");
}

#[test]
fn challenge_editor_immediate_deallocation_before_timer_fires() {
    snap::platform::install();
    let (_dir, db) = create_test_db();
    let clip: Rc<dyn ImageClipboard> = Rc::new(MockClipboard::default());
    let note_id = db.create_note(0).expect("create note");

    let weak_editor;
    {
        let note = Note::open(
            note_id,
            "# Immediate Drop Note\nInitial text",
            db.clone(),
            Rc::new(SimpleClock),
            None,
            false,
            Rc::new(|_, _| {}),
            Rc::new(|_| false),
            clip.clone(),
        ).expect("Note::open");

        weak_editor = Rc::downgrade(note.editor());
        note.ui().invoke_block_edited(1, "Typing before immediate drop".into(), 10);
        // Note dropped right here WITHOUT advancing time
    }

    // Immediately verify strong_count is 0 without advancing any time!
    assert!(
        weak_editor.upgrade().is_none(),
        "Editor must deallocate immediately upon Note drop before debounce timer fires (strong_count = {})",
        weak_editor.upgrade().map(|e| Rc::strong_count(&e)).unwrap_or(0)
    );
}

#[test]
fn challenge_editor_deallocation_after_timer_fired_and_retyping() {
    snap::platform::install();
    let (_dir, db) = create_test_db();
    let clip: Rc<dyn ImageClipboard> = Rc::new(MockClipboard::default());
    let note_id = db.create_note(0).expect("create note");

    let weak_editor;
    {
        let note = Note::open(
            note_id,
            "# Multi Burst Note\nInitial text",
            db.clone(),
            Rc::new(SimpleClock),
            None,
            false,
            Rc::new(|_, _| {}),
            Rc::new(|_| false),
            clip.clone(),
        ).expect("Note::open");

        weak_editor = Rc::downgrade(note.editor());

        // First edit burst
        note.ui().invoke_block_edited(1, "Burst 1".into(), 7);
        // Advance time to let timer fire and checkpoint
        snap::platform::advance(Duration::from_millis(600));

        // Second edit burst (starts timer again)
        note.ui().invoke_block_edited(1, "Burst 2".into(), 7);

        // Note dropped while second timer is pending
    }

    assert!(
        weak_editor.upgrade().is_none(),
        "Editor must deallocate after multi-burst edits and drop (strong_count = {})",
        weak_editor.upgrade().map(|e| Rc::strong_count(&e)).unwrap_or(0)
    );

    // Also advance past second timer to ensure mock scheduler doesn't panic on stale callbacks
    snap::platform::advance(Duration::from_millis(600));
}

#[test]
fn challenge_repeated_direct_note_open_edit_close_stress() {
    snap::platform::install();
    let (_dir, db) = create_test_db();
    let clip: Rc<dyn ImageClipboard> = Rc::new(MockClipboard::default());

    let mut weak_editors = Vec::new();

    for i in 0..50 {
        let note_id = db.create_note(0).expect("create note");
        let note = Note::open(
            note_id,
            &format!("# Stress Note {i}\nContent line 1"),
            db.clone(),
            Rc::new(SimpleClock),
            None,
            false,
            Rc::new(|_, _| {}),
            Rc::new(|_| false),
            clip.clone(),
        ).expect("Note::open");

        let weak = Rc::downgrade(note.editor());
        weak_editors.push(weak.clone());

        // Type several times
        note.ui().invoke_block_edited(1, format!("Edited {i}").into(), 8);
        if i % 3 == 0 {
            // Let some fire
            snap::platform::advance(Duration::from_millis(600));
        }

        // Drop note
        drop(note);

        // Ensure current editor dropped immediately
        assert!(
            weak.upgrade().is_none(),
            "Editor {i} leaked immediately after drop (strong_count = {})",
            weak.upgrade().map(|e| Rc::strong_count(&e)).unwrap_or(0)
        );
    }

    // Advance time and check all 50 weak references are None
    snap::platform::advance(Duration::from_millis(1000));
    for (i, weak) in weak_editors.iter().enumerate() {
        assert!(
            weak.upgrade().is_none(),
            "Editor {i} in repeated stress cycle leaked (strong_count = {})",
            weak.upgrade().map(|e| Rc::strong_count(&e)).unwrap_or(0)
        );
    }
}

