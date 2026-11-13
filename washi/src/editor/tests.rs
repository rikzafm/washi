use super::*;
use crate::document::{Caret, Range};
use crate::platform::{ClipboardPayload, ImageClipboard};
use crate::storage::Db;
use slint::ComponentHandle;
use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

#[derive(Default)]
struct DummyClipboard {
    payload: RefCell<ClipboardPayload>,
}

impl ImageClipboard for DummyClipboard {
    fn image(&self) -> Option<crate::images::Rgba> {
        self.payload.borrow().image.clone()
    }
    fn set_image(&self, img: &crate::images::Rgba) -> Result<(), String> {
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

fn test_db() -> (tempfile::TempDir, Rc<Db>) {
    let dir = tempfile::tempdir().expect("tempdir");
    let db = Db::open(&dir.path().join("test.db"), &[0x42; 32], 1000).expect("db open");
    (dir, Rc::new(db))
}

fn test_editor(content: &str) -> (tempfile::TempDir, Rc<Editor>) {
    let (dir, db) = test_db();
    let clipboard = Rc::new(DummyClipboard::default());
    let editor = Rc::new(Editor::new(1, content, db, clipboard));
    (dir, editor)
}

fn test_editor_with_ui(content: &str) -> (tempfile::TempDir, Rc<Editor>, NoteWindow) {
    crate::popup::init_test_platform();
    let (dir, editor) = test_editor(content);
    let ui = NoteWindow::new().expect("NoteWindow");
    ui.window().set_size(slint::LogicalSize::new(447.0, 468.0));
    let changed = Rc::new(|_| {});
    editor.bind_ui(&ui, changed, None, None, None);
    (dir, editor, ui)
}

#[test]
fn test_editor_initialization_and_text_roundtrip() {
    let text = "# Shopping List\nMilk and eggs\n- Bread\n- Butter";
    let (_dir, editor) = test_editor(text);
    assert_eq!(editor.text(), text);
    assert_eq!(editor.model.row_count(), 4);
    assert_eq!(editor.model.row_data(0).unwrap().kind, BlockKind::Title);
    assert_eq!(editor.model.row_data(1).unwrap().kind, BlockKind::Body);
    assert_eq!(editor.model.row_data(2).unwrap().kind, BlockKind::ListItem);
    assert_eq!(editor.model.row_data(3).unwrap().kind, BlockKind::ListItem);
}

#[test]
fn test_editor_undo_redo_history() {
    let (_dir, editor) = test_editor("Initial text");
    editor.checkpoint();
    editor.edit(0, "Modified text", 13);
    editor.checkpoint();
    assert_eq!(editor.text(), "# Modified text");

    // Undo steps back to initial state
    let caret = editor.step(false).expect("undo snapshot available");
    assert_eq!(editor.text(), "# Initial text");
    assert_eq!(caret.block, 0);

    // Redo steps forward to modified state
    let caret = editor.step(true).expect("redo snapshot available");
    assert_eq!(editor.text(), "# Modified text");
    assert_eq!(caret.block, 0);

    // Further redo is None
    assert!(editor.step(true).is_none());
}

#[test]
fn test_editor_adr0002_selection_wash_rects() {
    let text = "# First Line\nSecond Line\nThird Line";
    let (_dir, editor, ui) = test_editor_with_ui(text);
    editor.relayout(&ui);

    // Select all creates cross-block selection wash
    editor.select_all(&ui);
    assert_eq!(editor.selection(), Range { anchor: Caret { block: 0, offset: 0 }, focus: Caret { block: 2, offset: 10 } });
    assert!(editor.sel_rects.row_count() >= 3, "one wash box per line across 3 blocks");
    assert!(!ui.get_wash(), "note-level wash active: individual field wash suppressed");

    // Single block selection clears note wash rects and restores field wash
    let single_range = Range {
        anchor: Caret { block: 1, offset: 0 },
        focus: Caret { block: 1, offset: 6 },
    };
    editor.set_tilted(&ui, false);
    editor.set_selection(&ui, single_range);
    assert_eq!(editor.selection(), Range { anchor: Caret { block: 1, offset: 0 }, focus: Caret { block: 1, offset: 6 } });
    assert_eq!(editor.sel_rects.row_count(), 0, "no note-level wash for single block");
    assert!(ui.get_wash(), "field wash restored for single block selection");

    // With tilted editing the note washes it too (the field's level wash is off): one box for the line
    editor.set_tilted(&ui, true);
    assert_eq!(editor.sel_rects.row_count(), 1, "tilted editing: the note washes a selection inside a block");
    assert!(ui.get_wash(), "it is still a selection inside one block");
    editor.set_selection(&ui, Range { anchor: Caret { block: 1, offset: 3 }, focus: Caret { block: 1, offset: 3 } });
    assert_eq!(editor.sel_rects.row_count(), 0, "a caret has no wash");
}

#[test]
fn test_editor_multi_click_pointer_expansion() {
    let text = "# Title\nFirst word second\nThird line";
    let (_dir, editor, ui) = test_editor_with_ui(text);
    editor.relayout(&ui);

    // Hit point in the middle of "First" in block 1
    let r = editor.runs.iter().find(|r| r.block == 1).expect("block 1 run");
    let at = (r.x + r.width / 2.0, r.y + 4.0);

    // 1st click: collapsed caret
    editor.pointer_down(&ui, at, false, Duration::from_millis(100));
    let Range { anchor, focus } = editor.selection();
    assert_eq!(anchor, focus, "single click collapses caret");

    // 2nd click within 500ms: word selection
    editor.pointer_down(&ui, at, false, Duration::from_millis(250));
    let Range { anchor, focus } = editor.selection();
    assert_ne!(anchor, focus, "double click expands to word range");
    assert_eq!(anchor.block, 1, "anchor in block 1");
    assert_eq!(focus.block, 1, "focus in block 1");

    // 3rd click within 500ms: paragraph selection
    editor.pointer_down(&ui, at, false, Duration::from_millis(400));
    let Range { anchor, focus } = editor.selection();
    assert_eq!(anchor, Caret { block: 1, offset: 0 }, "triple click anchor at start of paragraph");
    assert_eq!(focus, Caret { block: 1, offset: "First word second".len() }, "triple click focus at end of paragraph");
}

#[test]
fn test_double_click_word_selection_at_boundaries() {
    let text = "# Title\nFirst  second...third";
    let (_dir, editor, _ui) = test_editor_with_ui(text);

    // Block 1 text: "First  second...third"
    // "First" is 0..5
    // Beginning of "First" (offset 0)
    let r0 = editor.unit_at(Caret { block: 1, offset: 0 }, 2);
    assert_eq!(r0, Range { anchor: Caret { block: 1, offset: 0 }, focus: Caret { block: 1, offset: 5 } });

    // Trailing boundary of "First" before whitespace (offset 5)
    let r5 = editor.unit_at(Caret { block: 1, offset: 5 }, 2);
    assert_eq!(r5, Range { anchor: Caret { block: 1, offset: 0 }, focus: Caret { block: 1, offset: 5 } });

    // Double-clicking inside whitespace (offset 6) collapses
    let r6 = editor.unit_at(Caret { block: 1, offset: 6 }, 2);
    assert_eq!(r6, Range { anchor: Caret { block: 1, offset: 6 }, focus: Caret { block: 1, offset: 6 } });

    // Beginning of "second" (offset 7)
    let r7 = editor.unit_at(Caret { block: 1, offset: 7 }, 2);
    assert_eq!(r7, Range { anchor: Caret { block: 1, offset: 7 }, focus: Caret { block: 1, offset: 13 } });

    // Trailing boundary of "second" before punctuation (offset 13)
    let r13 = editor.unit_at(Caret { block: 1, offset: 13 }, 2);
    assert_eq!(r13, Range { anchor: Caret { block: 1, offset: 7 }, focus: Caret { block: 1, offset: 13 } });

    // Double-clicking inside punctuation (offset 14) collapses
    let r14 = editor.unit_at(Caret { block: 1, offset: 14 }, 2);
    assert_eq!(r14, Range { anchor: Caret { block: 1, offset: 14 }, focus: Caret { block: 1, offset: 14 } });

    // Trailing boundary of "third" at EOF (offset 21)
    let r21 = editor.unit_at(Caret { block: 1, offset: 21 }, 2);
    assert_eq!(r21, Range { anchor: Caret { block: 1, offset: 16 }, focus: Caret { block: 1, offset: 21 } });
}

#[test]
fn test_formatting_toggle_unaffected_at_boundaries() {
    let (_dir, editor, ui) = test_editor_with_ui("title\nbody text");
    editor.relayout(&ui);

    // Caret collapsed at end of "body" (block 1, offset 4)
    editor.set_selection(&ui, Range {
        anchor: Caret { block: 1, offset: 4 },
        focus: Caret { block: 1, offset: 4 },
    });

    // Toggle bold when collapsed at word boundary does NOT style "body", applies to typing style
    assert!(!editor.format(0));
    assert_eq!(editor.text(), "# title\nbody text");
    assert!(editor.typing_style().contains(Flags::BOLD));

    // Caret collapsed inside "body" (block 1, offset 2) styles the word
    editor.set_selection(&ui, Range {
        anchor: Caret { block: 1, offset: 2 },
        focus: Caret { block: 1, offset: 2 },
    });
    assert!(editor.format(0));
    assert_eq!(editor.text(), "# title\n**body** text");
}

#[test]
fn test_editor_image_resize_grip_and_drag_math() {
    let (dir, db) = test_db();
    let note_id = db.create_note(0).expect("create note");
    // Insert a test image into DB
    let flat_rgba = crate::images::Rgba {
        width: 200,
        height: 100,
        data: vec![255; 200 * 100 * 4],
    };
    let png = crate::images::encode_png(&flat_rgba).expect("encode png");
    let image_id = db.add_image(note_id, "image/png", 200, 100, &png).expect("add image");

    let content = format!("# Title\n![](img:{image_id})\nAfter text");
    crate::popup::init_test_platform();
    let clipboard = Rc::new(DummyClipboard::default());
    let editor = Rc::new(Editor::new(note_id, &content, db, clipboard));
    let ui = NoteWindow::new().expect("NoteWindow");
    ui.window().set_size(slint::LogicalSize::new(447.0, 468.0));
    editor.bind_ui(&ui, Rc::new(|_| {}), None, None, None);
    editor.relayout(&ui);

    // Select the image block (block index 1)
    let img_caret = Caret { block: 1, offset: 0 };
    editor.set_selection(&ui, Range { anchor: img_caret, focus: img_caret });

    // Handles model must contain 2 rects: image frame and resize grip
    assert_eq!(editor.handles.row_count(), 2, "selected image shows frame and grip");
    let grip_row = editor.handles.row_data(1).unwrap();
    let grip_center = (grip_row.x + grip_row.width / 2.0, grip_row.y + grip_row.height / 2.0);

    // Cursor kinds
    assert_eq!(editor.cursor_kind_at(&ui, grip_center), CURSOR_RESIZE);
    assert_eq!(editor.cursor_kind_at(&ui, (100.0, grip_row.y - 30.0)), CURSOR_DEFAULT);

    // Grab grip
    assert!(editor.grab_grip(&ui, grip_center));

    // Drag grip inwards to resize smaller
    let dragged = editor.grip_drag(&ui, (grip_center.0 - 50.0, grip_center.1));
    assert!(dragged.is_some());

    // Release grip commits undo checkpoint
    assert!(editor.release_grip());
    assert!(editor.step(false).is_some(), "undo steps back through grip drag");
    drop(dir);
}

#[test]
fn test_editor_formatting_toggle() {
    let (_dir, editor, ui) = test_editor_with_ui("title\nbody text");
    editor.relayout(&ui);

    // Select "body" in block 1: offsets 0..4
    let range = Range {
        anchor: Caret { block: 1, offset: 0 },
        focus: Caret { block: 1, offset: 4 },
    };
    editor.set_selection(&ui, range);

    // Toggle Bold (0)
    assert!(editor.format(0));
    assert_eq!(editor.text(), "# title\n**body** text");

    // Toggle Italic (1)
    assert!(editor.format(1));
    assert_eq!(editor.text(), "# title\n***body*** text");

    // Toggle List (4)
    assert!(editor.format(4));
    assert!(editor.text().contains("- "));
}

#[test]
fn test_editor_color_palette_and_marker_wash() {
    let (_dir, editor, ui) = test_editor_with_ui("# Header\nColored text here");
    editor.relayout(&ui);

    // Select "Colored" in block 1
    let range = Range {
        anchor: Caret { block: 1, offset: 0 },
        focus: Caret { block: 1, offset: 7 },
    };
    editor.set_selection(&ui, range);

    // Apply ink 1 and marker 2
    assert!(editor.apply_paint(Some(1), Some(2)));
    editor.relayout(&ui);

    // Check markers model is populated
    assert!(editor.marker_data.row_count() > 0, "marker wash created for highlighted text");

    // Select all to verify marker wash under selection
    editor.select_all(&ui);
    assert!(editor.marker_wash.row_count() > 0, "marker wash overlays tinted under selection");
}

#[test]
fn test_editor_range_keys_replace_and_cut_copy() {
    let (_dir, editor, ui) = test_editor_with_ui("# Title\nFirst paragraph\nSecond paragraph");
    editor.relayout(&ui);

    // Cross-block selection from block 1 offset 6 to block 2 offset 6
    let range = Range {
        anchor: Caret { block: 1, offset: 6 },
        focus: Caret { block: 2, offset: 6 },
    };
    editor.set_selection(&ui, range);

    // Copy range
    assert!(editor.copy_range(&editor.clipboard));
    assert!(editor.clipboard.text().unwrap().contains("paragraph\nSecond"));

    // Replace range with key
    let (code, caret) = editor.range_key("X");
    assert_eq!(code, 2, "range replaced");
    assert!(caret.is_some());
    assert_eq!(editor.text(), "# Title\nFirst X paragraph");

    // Undo restores original text
    editor.step(false);
    assert_eq!(editor.text(), "# Title\nFirst paragraph\nSecond paragraph");
}

#[test]
fn test_editor_multi_block_single_line_paste_with_clipboard_abstraction() {
    let (_dir, editor, ui) = test_editor_with_ui("Block 0\nBlock 1\nBlock 2");
    editor.relayout(&ui);

    // Cross-block selection from block 0 offset 3 to block 2 offset 3
    let range = Range {
        anchor: Caret { block: 0, offset: 3 },
        focus: Caret { block: 2, offset: 3 },
    };
    editor.set_selection(&ui, range);
    assert!(editor.range.get().is_some(), "multi-block range selection active");

    // Inject single-line plain text into clipboard abstraction (DummyClipboard)
    editor.clipboard.set_rich(ClipboardPayload {
        plain: Some("REPLACED".to_string()),
        ..Default::default()
    }).unwrap();

    // Invoke paste-range callback
    let handled = ui.invoke_paste_range();
    assert!(handled, "paste-range handled single-line clipboard content");

    // Selected range cleanly replaced
    assert_eq!(editor.text(), "# BloREPLACEDck 2");
    assert!(editor.range.get().is_none(), "range cleared after paste");

    // Undo restores original multi-block text
    editor.step(false);
    assert_eq!(editor.text(), "# Block 0\nBlock 1\nBlock 2");

    // Redo re-applies paste replacement
    editor.step(true);
    assert_eq!(editor.text(), "# BloREPLACEDck 2");
}

/// Where each motion lands is the caret motion module's to test; here only that the note takes
/// the keys a block forwards, on a text block and on an image, and leaves the rest to the field.
#[test]
fn a_block_forwards_navigation_keys_to_the_note() {
    use crate::note_window::KeyMods;
    use slint::platform::Key;
    let key = |k: Key| char::from(k).to_string();
    let (_dir, db) = test_db();
    let note_id = db.create_note(0).expect("create note");
    let rgba = crate::images::Rgba { width: 200, height: 100, data: vec![255; 200 * 100 * 4] };
    let png = crate::images::encode_png(&rgba).expect("encode png");
    let img = db.add_image(note_id, "image/png", 200, 100, &png).expect("add image");
    let content = format!("First\n![](img:{img})\nMiddle");
    crate::popup::init_test_platform();
    let editor = Rc::new(Editor::new(note_id, &content, db, Rc::new(DummyClipboard::default())));
    let ui = NoteWindow::new().expect("NoteWindow");
    ui.window().set_size(slint::LogicalSize::new(447.0, 468.0));
    editor.bind_ui(&ui, Rc::new(|_| {}), None, None, None);
    editor.relayout(&ui);
    let press = |k: &str, shift, control, block, offset| {
        ui.invoke_block_key(block, k.into(), KeyMods { shift, control, selecting: false }, offset)
    };
    // 0 "First", 1 image, 2 "Middle"
    for shift in [false, true] {
        assert!(press(&key(Key::DownArrow), shift, false, 0, 2), "down, shift={shift}");
        assert!(press(&key(Key::UpArrow), shift, false, 2, 3), "up, shift={shift}");
        assert!(press(&key(Key::Home), shift, false, 2, 3), "home, shift={shift}");
        assert!(press(&key(Key::End), shift, false, 2, 3), "end, shift={shift}");
        assert!(press(&key(Key::Home), shift, true, 2, 3), "ctrl+home, shift={shift}");
        assert!(press(&key(Key::End), shift, true, 0, 3), "ctrl+end, shift={shift}");
        assert!(press(&key(Key::DownArrow), shift, false, 1, 0), "down off an image, shift={shift}");
        assert!(press(&key(Key::LeftArrow), shift, false, 1, 0), "left off an image, shift={shift}");
    }
    let on_image = Caret { block: 1, offset: 0 };
    editor.set_selection(&ui, Range { anchor: on_image, focus: on_image });
    assert!(!press(&key(Key::Home), false, false, 1, 0), "an image has no line to go to the edge of");
    assert!(!press(&key(Key::LeftArrow), false, false, 0, 2), "a step inside a paragraph is the field's");
    assert!(!press(&key(Key::LeftArrow), false, true, 0, 2), "ctrl+arrow is the field's");
    assert!(!press("a", false, false, 0, 2), "typing is the field's");
}
