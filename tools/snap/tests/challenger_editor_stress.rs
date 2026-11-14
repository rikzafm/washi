//! Empirical Challenger Stress Tests for Milestone 2:
//! Note Editor Engine Extraction (Features EE-01 to EE-05).
//!
//! Adversarial stress test suites covering:
//! 1. Rapid burst typing, undo/redo cycles, interleaving formatting toggles (Bold/Italic/List).
//! 2. Multi-paragraph range selection with cross-block replacement, cutting, pasting, and edge cases.
//! 3. Image resize grip dragging with extreme coordinates (<0, >avail), alignment switches, aspect ratios.
//! 4. Stepped line layout, scale factor snapping, and settle animation invariants.

use slint::{ComponentHandle, Model};
use snap::fakes::FakeClipboard;
use std::rc::Rc;
use washi::document::{
    Align, Caret, Range,
    IMAGE_PCT_MAX, IMAGE_PCT_MIN,
};
use washi::editor::{
    Editor, FORMAT_BOLD, FORMAT_ITALIC, FORMAT_LIST, START,
};
use washi::images::Rgba;
use washi::note_window::{BlockKind, NoteWindow};
use washi::platform::ImageClipboard;
use washi::storage::Db;

// =========================================================================
// Test Harness Helpers
// =========================================================================

fn test_db() -> (tempfile::TempDir, Rc<Db>, i64) {
    let dir = tempfile::tempdir().expect("tempdir");
    let db = Db::open(&dir.path().join("test.db"), &[0x42; 32], 1000).expect("db open");
    let note_id = db.create_note(0).expect("create note in db");
    (dir, Rc::new(db), note_id)
}

fn test_editor(content: &str) -> (tempfile::TempDir, Rc<Editor>, Rc<FakeClipboard>, i64) {
    let (dir, db, note_id) = test_db();
    let clipboard = Rc::new(FakeClipboard::default());
    let editor = Rc::new(Editor::new(note_id, content, db, clipboard.clone()));
    (dir, editor, clipboard, note_id)
}

fn test_editor_with_ui(content: &str) -> (tempfile::TempDir, Rc<Editor>, NoteWindow, Rc<FakeClipboard>, i64) {
    snap::platform::install();
    let (dir, editor, clipboard, note_id) = test_editor(content);
    let ui = NoteWindow::new().expect("NoteWindow");
    ui.window().set_size(slint::LogicalSize::new(447.0, 468.0));
    let changed = Rc::new(|_| {});
    editor.bind_ui(&ui, changed, None, None, None);
    (dir, editor, ui, clipboard, note_id)
}

fn insert_test_image(db: &Db, note_id: i64, width: u32, height: u32) -> (i64, Rgba) {
    let rgba = Rgba {
        width,
        height,
        data: vec![200; (width * height * 4) as usize],
    };
    let png = washi::images::encode_png(&rgba).expect("encode png");
    let image_id = db.add_image(note_id, "image/png", width, height, &png).expect("add image");
    (image_id, rgba)
}

// =========================================================================
// Category 1: Rapid Editing Sequences, Burst Typing, Undo/Redo & Formatting
// =========================================================================

#[test]
fn stress_rapid_burst_typing_and_undo_redo_cycles() {
    let (_dir, editor, ui, _cb, _id) = test_editor_with_ui("Initial Title");
    editor.relayout(&ui);

    // Initial state check
    assert_eq!(editor.text(), "# Initial Title");
    editor.checkpoint();

    // 100 rapid sequential typing edits simulating keypresses
    for i in 1..=50 {
        let text = format!("Initial Title {i}");
        editor.edit(0, &text, text.len());
        if i % 10 == 0 {
            // Emulate 500ms pause checkpoint
            editor.checkpoint();
        }
    }
    assert_eq!(editor.text(), "# Initial Title 50");

    // 5 checkpoints were recorded; step back through all 5
    for i in (1..5).rev() {
        let caret = editor.history_step(false).caret().expect("undo snapshot available");
        assert_eq!(caret.block, 0);
        let expected = format!("# Initial Title {}", i * 10);
        assert_eq!(editor.text(), expected, "Undo step {i} matches checkpoint");
    }

    // Step back to initial state
    let caret = editor.history_step(false).caret().expect("undo to initial");
    assert_eq!(caret.block, 0);
    assert_eq!(editor.text(), "# Initial Title");

    // Redo all the way back to step 50
    for i in 1..=5 {
        let caret = editor.history_step(true).caret().expect("redo snapshot available");
        assert_eq!(caret.block, 0);
        let expected = format!("# Initial Title {}", i * 10);
        assert_eq!(editor.text(), expected, "Redo step {i} matches checkpoint");
    }

    // No further redo
    assert!(editor.history_step(true).caret().is_none());
}

#[test]
fn stress_interleaving_text_editing_with_formatting_toggles() {
    let (_dir, editor, ui, _cb, _id) = test_editor_with_ui("Title\nFirst line\nSecond line");
    editor.relayout(&ui);

    // 1. Select "First" in block 1 (offsets 0..5) and toggle bold
    editor.set_selection(&ui, Range { anchor: Caret { block: 1, offset: 0 }, focus: Caret { block: 1, offset: 5 } });
    assert!(editor.format(FORMAT_BOLD).changed());
    assert_eq!(editor.text(), "# Title\n**First** line\nSecond line");

    // 2. Select "line" in block 1 (offsets 6..10) and toggle italic
    editor.set_selection(&ui, Range { anchor: Caret { block: 1, offset: 6 }, focus: Caret { block: 1, offset: 10 } });
    assert!(editor.format(FORMAT_ITALIC).changed());
    assert_eq!(editor.text(), "# Title\n**First** *line*\nSecond line");

    // 3. Toggle List on second line
    editor.set_selection(&ui, Range { anchor: Caret { block: 2, offset: 0 }, focus: Caret { block: 2, offset: 0 } });
    assert!(editor.format(FORMAT_LIST).changed());
    assert_eq!(editor.text(), "# Title\n**First** *line*\n- Second line");

    // 4. Undo list, undo italic, undo bold
    let c = editor.history_step(false).caret().expect("undo list");
    editor.notify_change(Some(c));
    assert_eq!(editor.text(), "# Title\n**First** *line*\nSecond line");

    let c = editor.history_step(false).caret().expect("undo italic");
    editor.notify_change(Some(c));
    assert_eq!(editor.text(), "# Title\n**First** line\nSecond line");

    let c = editor.history_step(false).caret().expect("undo bold");
    editor.notify_change(Some(c));
    assert_eq!(editor.text(), "# Title\nFirst line\nSecond line");
}

#[test]
fn stress_redo_clearing_on_new_edit() {
    let (_dir, editor, _ui, _cb, _id) = test_editor_with_ui("Base Note");
    editor.checkpoint();

    editor.edit(0, "Base Note A", 11);
    editor.checkpoint();

    editor.edit(0, "Base Note B", 11);
    editor.checkpoint();

    // Undo twice back to Base Note
    editor.history_step(false);
    editor.history_step(false);
    assert_eq!(editor.text(), "# Base Note");
    assert!(editor.can_redo(), "Redo stack must be non-empty");

    // A new edit must invalidate the redo branch
    editor.edit(0, "Base Note Branch C", 18);
    editor.checkpoint();
    assert_eq!(editor.text(), "# Base Note Branch C");
    assert!(!editor.can_redo(), "New edit must discard redo history");
}

// =========================================================================
// Category 2: Multi-Paragraph Range Selection, Cross-Block Ops & Edge Cases
// =========================================================================

#[test]
fn stress_multi_paragraph_range_replacement_and_cross_block_ops() {
    let text = "# Document Title\nParagraph Alpha\n## Heading Beta\n- List Item Gamma\nParagraph Delta";
    let (_dir, editor, ui, cb, _id) = test_editor_with_ui(text);
    editor.relayout(&ui);

    // Cross-block selection: from middle of Paragraph Alpha (block 1, offset 10)
    // to middle of List Item Gamma (block 3, offset 10)
    let cross_range = Range {
        anchor: Caret { block: 1, offset: 10 },
        focus: Caret { block: 3, offset: 10 },
    };
    editor.set_selection(&ui, cross_range);

    // Verification of ADR-0002 wash invariants
    assert!(!ui.get_wash(), "cross-block selection disables single-field wash");
    assert!(editor.sel_rects.row_count() >= 3, "wash rects created for spanned lines");

    // Cut range
    let caret = editor.cut_range(&(cb.clone() as Rc<dyn ImageClipboard>)).expect("cut succeeds");
    editor.notify_change(Some(caret));

    // Check clipboard contains cut plain text
    let plain = cb.text().expect("plain text on clipboard");
    assert!(plain.contains("Alpha\nHeading Beta\nList Item"));

    // Check merged document structure:
    // Block 1 keeps its kind (Body) and merges start of Alpha with tail of Gamma
    assert_eq!(editor.text(), "# Document Title\nParagraph Gamma\nParagraph Delta");
    assert_eq!(caret, Caret { block: 1, offset: 10 });

    // Paste back replacement text
    let paste_caret = editor.replace_range("[INJECTED] ");
    editor.notify_change(Some(paste_caret));
    assert_eq!(editor.text(), "# Document Title\nParagraph [INJECTED] Gamma\nParagraph Delta");
    assert_eq!(paste_caret, Caret { block: 1, offset: 21 });

    // Undo restores the original multi-block state
    let c = editor.history_step(false).caret().expect("undo paste");
    editor.notify_change(Some(c));
    assert_eq!(editor.text(), "# Document Title\nParagraph Gamma\nParagraph Delta");

    let c = editor.history_step(false).caret().expect("undo cut");
    editor.notify_change(Some(c));
    assert_eq!(editor.text(), text);
}

#[test]
fn stress_range_operations_with_empty_paragraphs_and_slivers() {
    // Document with multiple consecutive empty paragraphs
    let text = "# Title\n\n\nParagraph\n\n";
    let (_dir, editor, ui, _cb, _id) = test_editor_with_ui(text);
    editor.relayout(&ui);

    // Select all across empty paragraphs
    editor.select_all(&ui);
    assert!(!ui.get_wash());

    // In ADR-0002, empty paragraphs get sliver rects so the selection highlight is visible
    let rects = &editor.sel_rects;
    assert!(rects.row_count() >= 4, "Every block including empty ones has selection wash");
    for i in 0..rects.row_count() {
        let r = rects.row_data(i).unwrap();
        assert!(r.width > 0.0, "Sliver or line rect width must be > 0 (found {})", r.width);
        assert!(r.height > 0.0, "Sliver or line rect height must be > 0 (found {})", r.height);
    }

    // Replace all with a single character
    let caret = editor.replace_range("Z");
    editor.notify_change(Some(caret));
    assert_eq!(editor.text(), "# Z");
    assert_eq!(caret, Caret { block: 0, offset: 1 });

    // Undo restores all empty paragraphs
    let c = editor.history_step(false).caret().expect("undo replace all");
    editor.notify_change(Some(c));
    assert_eq!(editor.text(), text);
}

#[test]
fn stress_unicode_emoji_and_multibyte_utf8_boundaries() {
    // Complex unicode emojis: crab (4 bytes), rocket (4 bytes), party (4 bytes),
    // ZWJ sequence family (👨‍👩‍👧‍👦 = 25 bytes), CJK characters, Arabic RTL
    let text = "# 🦀 Rustaceans 🎉\n🚀 Rocket to 👨‍👩‍👧‍👦 family\n日本語と中文\nمرحبا بالعالم";
    let (_dir, editor, ui, _cb, _id) = test_editor_with_ui(text);
    editor.relayout(&ui);

    // Clamping on non-char-boundary offsets:
    // "🦀" is bytes 0..4 in "# 🦀..." -> "# " is 0..2, "🦀" is 2..6.
    // Caret at offset 3 or 4 or 5 must clamp without panic.
    let unaligned = Range {
        anchor: Caret { block: 0, offset: 3 },
        focus: Caret { block: 0, offset: 5 },
    };
    let clamped = editor.clamp(unaligned);
    assert!(editor.doc().blocks()[0].text.is_char_boundary(clamped.anchor.offset));
    assert!(editor.doc().blocks()[0].text.is_char_boundary(clamped.focus.offset));

    // Selection across block 0 and block 1 with emojis
    editor.set_selection(&ui, Range {
        anchor: Caret { block: 0, offset: 2 }, // before crab
        focus: Caret { block: 1, offset: 15 }, // after rocket
    });
    assert!(!ui.get_wash());

    // Replace range with emoji
    let c = editor.replace_range("✨🌟✨");
    editor.notify_change(Some(c));
    assert!(editor.text().contains("✨🌟✨"));

    // Step undo restores original unicode text
    let c = editor.history_step(false).caret().expect("undo replace");
    editor.notify_change(Some(c));
    assert_eq!(editor.text(), text);
}

// =========================================================================
// Category 3: Image Operations Stress Testing
// =========================================================================

#[test]
fn stress_image_resize_grip_dragging_extreme_coordinates() {
    let (dir, db, note_id) = test_db();
    let (image_id, _) = insert_test_image(&db, note_id, 300, 150);

    let content = format!("# Title\n![](img:{image_id})\nAfter text");
    snap::platform::install();
    let clipboard = Rc::new(FakeClipboard::default());
    let editor = Rc::new(Editor::new(note_id, &content, db, clipboard));
    let ui = NoteWindow::new().expect("NoteWindow");
    ui.window().set_size(slint::LogicalSize::new(447.0, 468.0));
    editor.bind_ui(&ui, Rc::new(|_| {}), None, None, None);
    editor.relayout(&ui);

    // Select the image block (block 1)
    editor.set_selection(&ui, Range { anchor: Caret { block: 1, offset: 0 }, focus: Caret { block: 1, offset: 0 } });
    assert_eq!(editor.handles.row_count(), 2);
    let grip_row = editor.handles.row_data(1).unwrap();
    let grip_center = (grip_row.x + grip_row.width / 2.0, grip_row.y + grip_row.height / 2.0);

    // Grab grip
    assert!(editor.grab_grip(&ui, grip_center));

    // 1. Extreme negative coordinate: (-10000.0, -10000.0)
    let resized = editor.grip_drag(&ui, (-10000.0, -10000.0)).expect("grip_drag handled");
    assert!(resized);
    let (pct, _) = editor.image_state(1).unwrap();
    assert_eq!(pct, IMAGE_PCT_MIN, "Negative extreme coordinate clamps to IMAGE_PCT_MIN (10)");

    // 2. Extreme positive coordinate: (50000.0, 50000.0)
    let resized = editor.grip_drag(&ui, (50000.0, 50000.0)).expect("grip_drag handled");
    assert!(resized);
    let (pct, _) = editor.image_state(1).unwrap();
    assert_eq!(pct, IMAGE_PCT_MAX, "Positive extreme coordinate clamps to IMAGE_PCT_MAX (100)");

    // 3. Exact 0.0 coordinate
    let _ = editor.grip_drag(&ui, (0.0, 100.0));
    let (pct, _) = editor.image_state(1).unwrap();
    assert_eq!(pct, IMAGE_PCT_MIN, "0.0 x coordinate clamps to minimum");

    // 4. Release grip commits undo snapshot
    assert!(editor.release_grip());
    assert!(editor.history_step(false).caret().is_some(), "Undo reverts resize");
    let (pct_original, _) = editor.image_state(1).unwrap();
    assert_eq!(pct_original, IMAGE_PCT_MAX, "Original size restored");
    drop(dir);
}

#[test]
fn stress_image_alignment_switches_and_drag_geometry() {
    let (dir, db, note_id) = test_db();
    let (image_id, _) = insert_test_image(&db, note_id, 400, 200);

    let content = format!("# Title\n![](img:{image_id},50,l)\nEnd");
    snap::platform::install();
    let clipboard = Rc::new(FakeClipboard::default());
    let editor = Rc::new(Editor::new(note_id, &content, db, clipboard));
    let ui = NoteWindow::new().expect("NoteWindow");
    ui.window().set_size(slint::LogicalSize::new(447.0, 468.0));
    editor.bind_ui(&ui, Rc::new(|_| {}), None, None, None);
    editor.relayout(&ui);

    // Select image
    editor.set_selection(&ui, Range { anchor: Caret { block: 1, offset: 0 }, focus: Caret { block: 1, offset: 0 } });

    // Test Align::Left: Grip is at bottom right
    editor.set_image_align(1, Align::Left);
    editor.relayout(&ui);
    let grip_left = editor.handles.row_data(1).unwrap();
    let img_rect_left = editor.handles.row_data(0).unwrap();
    assert!((grip_left.x - (img_rect_left.x + img_rect_left.width - 10.0)).abs() < 1.0, "Left align grip is at right edge");

    // Test Align::Right: Grip is at bottom left
    editor.set_image_align(1, Align::Right);
    editor.relayout(&ui);
    let grip_right = editor.handles.row_data(1).unwrap();
    let img_rect_right = editor.handles.row_data(0).unwrap();
    assert!((grip_right.x - img_rect_right.x).abs() < 1.0, "Right align grip is at left edge");

    // Test Align::Center: Grip is at bottom right
    editor.set_image_align(1, Align::Center);
    editor.relayout(&ui);
    let grip_center = editor.handles.row_data(1).unwrap();
    let img_rect_center = editor.handles.row_data(0).unwrap();
    assert!((grip_center.x - (img_rect_center.x + img_rect_center.width - 10.0)).abs() < 1.0, "Center align grip is at right edge");

    // Dragging right for Right-aligned image reduces its percentage
    editor.set_image_align(1, Align::Right);
    editor.set_image_pct(1, 80);
    editor.relayout(&ui);
    let grip_r = editor.handles.row_data(1).unwrap();
    assert!(editor.grab_grip(&ui, (grip_r.x + 5.0, grip_r.y + 5.0)));
    // Drag to the right (increasing x) -> should decrease width
    editor.grip_drag(&ui, (grip_r.x + 50.0, grip_r.y + 5.0));
    let (pct, _) = editor.image_state(1).unwrap();
    assert!(pct < 80, "Moving grip inwards (right) on Right-aligned image shrinks it (pct={pct})");
    editor.release_grip();
    drop(dir);
}

#[test]
fn stress_image_aspect_ratio_consistency() {
    let (dir, db, note_id) = test_db();
    // Test diverse aspect ratios:
    let ratios = [
        (400, 200, 2.0),   // Landscape 2:1
        (150, 300, 0.5),   // Portrait 1:2
        (250, 250, 1.0),   // Square 1:1
        (1000, 50, 20.0),  // Ultra-wide
        (50, 1000, 0.05),  // Ultra-tall
    ];

    snap::platform::install();
    let clipboard = Rc::new(FakeClipboard::default());

    for (w, h, expected_ratio) in ratios {
        let (image_id, _) = insert_test_image(&db, note_id, w, h);
        let content = format!("# Title\n![](img:{image_id})\nEnd");
        let editor = Rc::new(Editor::new(note_id, &content, db.clone(), clipboard.clone()));
        let ui = NoteWindow::new().expect("NoteWindow");
        ui.window().set_size(slint::LogicalSize::new(447.0, 468.0));
        editor.bind_ui(&ui, Rc::new(|_| {}), None, None, None);

        // Test every percentage from 10% to 100% in 15% steps
        for pct in (10..=100).step_by(15) {
            editor.set_image_pct(1, pct);
            editor.relayout(&ui);
            let doc = editor.doc();
            let b = &doc.blocks()[1];
            let avail = editor.paper.get().0 - 32.0 - 40.0;
            let (_, img_w, img_h) = b.image_box(avail, (w, h));
            let computed_ratio = img_w / img_h;
            assert!(
                (computed_ratio - expected_ratio).abs() < 1e-4,
                "Aspect ratio preserved for {w}x{h} at {pct}%: expected {expected_ratio}, got {computed_ratio}"
            );
        }
    }
    drop(dir);
}

#[test]
fn stress_image_deletion_cutting_and_undo_invariants() {
    let (dir, db, note_id) = test_db();
    let (img1, _) = insert_test_image(&db, note_id, 200, 100);
    let (img2, _) = insert_test_image(&db, note_id, 300, 150);

    let content = format!("# Title\n![](img:{img1})\nMiddle Text\n![](img:{img2})\nBottom");
    snap::platform::install();
    let clipboard = Rc::new(FakeClipboard::default());
    let editor = Rc::new(Editor::new(note_id, &content, db, clipboard.clone()));
    let ui = NoteWindow::new().expect("NoteWindow");
    ui.window().set_size(slint::LogicalSize::new(447.0, 468.0));
    editor.bind_ui(&ui, Rc::new(|_| {}), None, None, None);
    editor.relayout(&ui);

    assert_eq!(editor.model.row_count(), 5);

    // 1. Cut the first image (block 1)
    let caret = editor.cut_image(1).expect("cut image succeeds");
    assert!(clipboard.has_image(), "Clipboard holds image pixels");
    assert_eq!(editor.model.row_count(), 4);
    assert_eq!(caret, Caret { block: 1, offset: 0 });

    // 2. Undo restores the image
    let c = editor.history_step(false).caret().expect("undo cut");
    editor.notify_change(Some(c));
    assert_eq!(editor.model.row_count(), 5);
    assert_eq!(editor.model.row_data(1).unwrap().kind, BlockKind::Image);

    // 3. Delete second image (block 3) via Backspace on block 3
    let before = editor.doc().clone();
    let caret = editor.image_key(3, &char::from(slint::platform::Key::Backspace).to_string(), before);
    assert!(caret.is_some());
    editor.notify_change(caret);
    assert_eq!(editor.model.row_count(), 4);

    // 4. Undo restores second image
    let c = editor.history_step(false).caret().expect("undo delete");
    editor.notify_change(Some(c));
    assert_eq!(editor.text(), content);
    assert_eq!(editor.model.row_count(), 5);
    drop(dir);
}

// =========================================================================
// Category 4: Stepped Line Layout, Settle Animation, & Scale Factor Invariants
// =========================================================================

#[test]
fn stress_stepped_line_layout_invariants_across_scale_factors() {
    let content = "# Dynamic Title\n## Section Heading\nBody text with **bold** and *italic* styling.\n- List item\nAnother body line.";
    let (_dir, editor, ui, _cb, _id) = test_editor_with_ui(content);

    let test_scales = [0.5, 0.75, 1.0, 1.25, 1.5, 1.75, 2.0, 2.5, 3.0];

    for scale in test_scales {
        ui.window().dispatch_event(slint::platform::WindowEvent::ScaleFactorChanged { scale_factor: scale as f32 });
        editor.relayout(&ui);

        // 1. Pitch scale matches window scale factor
        assert_eq!(ui.get_pitch_scale(), scale as f32);

        // 2. Content height is strictly positive and finite
        let height = ui.get_content_height();
        assert!(height > 0.0 && !height.is_nan() && height.is_finite(), "Content height finite at scale {scale}");

        // 3. Stepped line layout run indices and geometries
        let runs = &editor.runs;
        assert!(runs.row_count() >= 5);
        for i in 0..runs.row_count() {
            let r = runs.row_data(i).unwrap();
            assert!(r.block >= 0 && (r.block as usize) < editor.doc().len(), "Run block index in valid range");
            assert!(r.pitch > 0.0, "Run pitch must be strictly positive");
            assert!(r.size > 0.0, "Run size must be strictly positive");
            assert!(!r.x.is_nan() && !r.y.is_nan(), "Coordinates must not be NaN");
            assert!(!r.dx.is_nan() && !r.dy.is_nan(), "Stepped offsets must not be NaN");

            // Verify scale snapping invariant: pitch * scale is rounded to integer
            let scaled_pitch = (r.pitch as f64) * scale;
            assert!(
                (scaled_pitch - scaled_pitch.round()).abs() < 1e-4,
                "Pitch {} snapped to physical pixel boundary at scale {scale}", r.pitch
            );
        }

        // 4. Stepped rule geometries
        let rules = &editor.rules;
        for i in 0..rules.row_count() {
            let rule = rules.row_data(i).unwrap();
            assert!(rule.width >= 0.0 && rule.height >= 0.0);
            assert!(!rule.x.is_nan() && !rule.y.is_nan());
            assert!(!rule.dx.is_nan() && !rule.dy.is_nan());
        }
    }
}

#[test]
fn stress_paper_resize_extremes_and_invariants() {
    let content = "# Title Line\nLong paragraph that wraps many times across multiple lines under small paper sizes.";
    let (_dir, editor, ui, _cb, _id) = test_editor_with_ui(content);

    // Test extreme paper sizes:
    let paper_sizes = [
        (10.0, 10.0),       // Sub-inset paper width
        (80.0, 100.0),      // Very narrow paper
        (363.0, 433.0),     // Normal paper
        (1000.0, 1000.0),   // Large paper
        (10000.0, 5000.0),  // Ultra wide paper
    ];

    for (w, h) in paper_sizes {
        editor.set_paper_size(w, h);
        editor.relayout(&ui);

        let ch = ui.get_content_height();
        assert!(!ch.is_nan() && ch.is_finite() && ch >= 0.0, "Content height non-negative and finite at ({w}, {h})");

        // Verify select all generates valid non-negative rects
        editor.select_all(&ui);
        for i in 0..editor.sel_rects.row_count() {
            let r = editor.sel_rects.row_data(i).unwrap();
            assert!(r.width >= 0.0 && r.height >= 0.0, "Selection rect dimensions non-negative at ({w}, {h})");
            assert!(!r.x.is_nan() && !r.y.is_nan(), "Selection rect coords not NaN at ({w}, {h})");
        }
    }
}

#[test]
fn stress_editor_switch_to_note_settle_invariants() {
    let (_dir, editor, ui, _cb, _id) = test_editor_with_ui("# Note 1\nContent 1");
    editor.relayout(&ui);

    let initial_serial = ui.get_switch_serial();

    // Switch to note 2
    editor.switch_to(&ui, 2, "# Note 2\nLine A\nLine B\nLine C");
    assert_eq!(editor.id(), 2);
    assert_eq!(editor.text(), "# Note 2\nLine A\nLine B\nLine C");
    assert_eq!(ui.get_switch_serial(), initial_serial + 1, "Switch serial incremented");
    assert_eq!(ui.get_scroll_y(), 0.0, "Scroll reset on switch");
    assert_eq!(editor.selection(), Range { anchor: START, focus: START });
    assert!(ui.get_wash(), "Selection wash reset on switch");

    // Switch to note 3 (empty note)
    editor.switch_to(&ui, 3, "");
    assert_eq!(editor.id(), 3);
    assert!(editor.is_empty());
    assert_eq!(ui.get_switch_serial(), initial_serial + 2);
    // Hint runs generated for empty note
    assert_eq!(editor.runs.row_data(0).unwrap().block, -2, "Empty note has hint run with block -2");
}

#[test]
fn stress_editor_deallocation_no_circular_reference_in_history_timer() {
    snap::platform::install();
    let (_dir, db, note_id) = test_db();
    let clip: Rc<dyn ImageClipboard> = Rc::new(FakeClipboard::default());
    let weak_editor;
    {
        let note = washi::note_window::Note::open(
            note_id,
            "# Pending Timer Note\nInitial text",
            db.clone(),
            Rc::new(washi::editor::SystemClock),
            None,
            false,
            Rc::new(|_, _| {}),
            Rc::new(|_| false),
            clip.clone(),
        ).expect("Note::open");

        weak_editor = Rc::downgrade(note.editor());

        // Trigger on_block_edited callback on UI (which sets history_timer with editor_clone)
        note.ui().invoke_block_edited(1, "Typed text".into(), 10);
        // Note is dropped here without note.pause()
    }

    // Advance mock time past the 500ms history debounce timer
    snap::platform::advance(std::time::Duration::from_millis(600));

    // Editor must be deallocated cleanly without reference cycle leak
    assert!(
        weak_editor.upgrade().is_none(),
        "Editor leaked: failed to deallocate after window close due to circular reference in history_timer (strong_count = {})",
        weak_editor.upgrade().map(|e| Rc::strong_count(&e)).unwrap_or(0)
    );
}

