//! End-to-end functional and visual fidelity snapshot tests for the All Notes ("Notes") window.

use snap::Harness;
use std::path::Path;
use std::time::Duration;

const ENDFIELD_CONTENT: &str = "# Endfield Note\n## Thermite Cutter : POG & Akekuri\nCurrently POG no Weapon : Ideal for him are Never Rest or White Night Nova. The last can be get in a box";

/// Set up a `Harness` with the exact 6 notes from the reference mockups and open `AllNotesWindow`
/// with `"Endfield Note"` selected.
fn setup_mockup_all_notes() -> Harness {
    let mut h = Harness::new();
    h.start().unwrap();

    // Create 4 closed notes from oldest to newest so `updated_at DESC` orders them:
    // 1. Meeting ideas, 2. Birthday gifts, 3. Reading list, 4. Trip to Kyoto
    let closed_titles = [
        "# Trip to Kyoto",
        "# Reading list",
        "# Birthday gifts",
        "# Meeting ideas",
    ];

    // Note 0 starts as the first closed note
    h.set_note_text(0, closed_titles[0]);
    h.advance(Duration::from_secs(1));

    for title in &closed_titles[1..] {
        h.right_click(0, Some(&["New note"]));
        let last = h.open_note_count() - 1;
        h.set_note_text(last, title);
        h.advance(Duration::from_secs(1));
    }

    // Create the 2 open notes: "Grocery run" and "Endfield Note"
    h.right_click(0, Some(&["New note"]));
    let grocery_idx = h.open_note_count() - 1;
    h.set_note_text(grocery_idx, "# Grocery run\nmilk");
    let grocery_id = h.note_id(grocery_idx);
    h.advance(Duration::from_secs(1));

    h.right_click(0, Some(&["New note"]));
    let endfield_idx = h.open_note_count() - 1;
    h.set_note_text(endfield_idx, ENDFIELD_CONTENT);
    let endfield_id = h.note_id(endfield_idx);
    h.advance(Duration::from_secs(1));

    // Close the first 4 notes (always closing index 0 four times)
    for _ in 0..4 {
        h.right_click(0, Some(&["Close"]));
    }
    assert_eq!(h.open_note_count(), 2);

    // Order open notes so Endfield Note is on top of the stack (listed 1st under OPEN)
    h.set_stacking(vec![grocery_id, endfield_id]);

    // Open All Notes from Endfield Note's dropdown menu
    let endfield_pos = (0..h.open_note_count())
        .find(|&i| h.note_id(i) == endfield_id)
        .unwrap();
    h.toggle_dropdown(endfield_pos);
    h.invoke_dropdown_all_notes();
    assert!(h.is_all_notes_open());
    assert_eq!(h.all_notes_selected_id(), Some(endfield_id));

    h
}

fn load_png_rgb(path: &Path) -> (u32, u32, Vec<u8>) {
    let file = std::fs::File::open(path).unwrap_or_else(|e| panic!("open {}: {e}", path.display()));
    let decoder = png::Decoder::new(file);
    let mut reader = decoder.read_info().expect("read png info");
    let mut buf = vec![0u8; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buf).expect("decode png frame");
    let (w, h) = (info.width, info.height);
    let rgb = match info.color_type {
        png::ColorType::Rgb => buf[..(w * h * 3) as usize].to_vec(),
        png::ColorType::Rgba => {
            let mut out = Vec::with_capacity((w * h * 3) as usize);
            for chunk in buf[..(w * h * 4) as usize].chunks_exact(4) {
                out.extend_from_slice(&[chunk[0], chunk[1], chunk[2]]);
            }
            out
        }
        other => panic!("unsupported png color type: {other:?}"),
    };
    (w, h, rgb)
}

fn write_png_rgb(path: &Path, width: u32, height: u32, rgb: &[u8]) {
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let mut out = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut out, width, height);
        enc.set_color(png::ColorType::Rgb);
        enc.set_depth(png::BitDepth::Eight);
        let mut w = enc.write_header().expect("png header");
        w.write_image_data(rgb).expect("png data");
    }
    std::fs::write(path, out).expect("write png file");
}

/// Build a 3-panel `[Mockup | Rendered | Diff (4x amplified)]` side-by-side comparison image in memory.
fn build_side_by_side_rgb(width: u32, height: u32, mockup_rgb: &[u8], rendered_rgb: &[u8]) -> (u32, u32, Vec<u8>) {
    let sep = 4u32;
    let out_w = width * 3 + sep * 2;
    let mut out = vec![0x2Bu8; (out_w * height * 3) as usize];
    for y in 0..height {
        for x in 0..width {
            let src_i = ((y * width + x) * 3) as usize;
            let m = [mockup_rgb[src_i], mockup_rgb[src_i + 1], mockup_rgb[src_i + 2]];
            let r = [rendered_rgb[src_i], rendered_rgb[src_i + 1], rendered_rgb[src_i + 2]];
            let d = [
                ((m[0] as i32 - r[0] as i32).unsigned_abs() * 4).min(255) as u8,
                ((m[1] as i32 - r[1] as i32).unsigned_abs() * 4).min(255) as u8,
                ((m[2] as i32 - r[2] as i32).unsigned_abs() * 4).min(255) as u8,
            ];

            let dst_m = ((y * out_w + x) * 3) as usize;
            out[dst_m..dst_m + 3].copy_from_slice(&m);

            let dst_r = ((y * out_w + (width + sep + x)) * 3) as usize;
            out[dst_r..dst_r + 3].copy_from_slice(&r);

            let dst_d = ((y * out_w + (width * 2 + sep * 2 + x)) * 3) as usize;
            out[dst_d..dst_d + 3].copy_from_slice(&d);
        }
    }
    (out_w, height, out)
}

#[test]
fn all_notes_end_to_end_workflow() {
    let mut h = setup_mockup_all_notes();

    // 1. Verify counts and sidebar sections match the 6 fixture notes
    assert_eq!(h.all_notes_open_count(), Some(2));
    assert_eq!(h.all_notes_closed_count(), Some(4));

    let open_items = h.all_notes_open_items();
    let closed_items = h.all_notes_closed_items();
    assert_eq!(
        open_items.iter().map(|i| i.title.as_str()).collect::<Vec<_>>(),
        vec!["Endfield Note", "Grocery run"]
    );
    assert_eq!(
        closed_items.iter().map(|i| i.title.as_str()).collect::<Vec<_>>(),
        vec!["Meeting ideas", "Birthday gifts", "Reading list", "Trip to Kyoto"]
    );

    let endfield_id = open_items[0].id as i64;
    let grocery_id = open_items[1].id as i64;
    let meeting_id = closed_items[0].id as i64;

    // 2. Selecting a closed note by clicking its sidebar row at (80, 340) previews it
    h.click_all_notes(80.0, 340.0);
    assert_eq!(h.all_notes_selected_id(), Some(meeting_id));
    assert_eq!(h.all_notes_status_text().as_deref(), Some("Closed · edited today"));

    // 3. Clicking the Open/Focus button at (279, 489) reopens the closed note onto the desktop
    h.click_all_notes(279.0, 489.0);
    assert_eq!(h.open_note_count(), 3);
    assert_eq!(h.all_notes_open_count(), Some(3));
    assert_eq!(h.all_notes_closed_count(), Some(3));
    assert_eq!(h.all_notes_selected_id(), Some(meeting_id));
    assert_eq!(
        h.all_notes_status_text().as_deref(),
        Some("Open on your desktop · edited today")
    );

    // 4. Clicking the Close (×) button at (316, 490) closes the note's desktop window and moves it back to CLOSED
    h.click_all_notes(316.0, 490.0);
    assert_eq!(h.open_note_count(), 2);
    assert_eq!(h.all_notes_open_count(), Some(2));
    assert_eq!(h.all_notes_closed_count(), Some(4));
    assert_eq!(h.all_notes_selected_id(), Some(meeting_id));
    assert_eq!(h.all_notes_status_text().as_deref(), Some("Closed · edited today"));

    // 5. Select open note "Endfield Note" by clicking row 0 at (80, 234) and click Open/Focus to bring to front
    h.click_all_notes(80.0, 234.0);
    assert_eq!(h.all_notes_selected_id(), Some(endfield_id));
    let log_before = h.control.log.borrow().len();
    h.click_all_notes(279.0, 489.0);
    assert!(
        h.control.log.borrow()[log_before..].contains(&snap::WindowCommand::BringToFront(endfield_id)),
        "clicking Open/Focus on open note brings it to front"
    );

    // 6. Clicking Export (↓) button at (352, 491) exports the selected note as Markdown
    let export_dir = tempfile::tempdir().unwrap();
    let md_path = export_dir.path().join("endfield.md");
    *h.dialogs.save_path.borrow_mut() = Some(md_path.clone());
    h.click_all_notes(352.0, 491.0);
    let exported = std::fs::read_to_string(&md_path).expect("exported markdown file");
    assert!(exported.contains("# Endfield Note"));
    assert!(exported.contains("## Thermite Cutter : POG & Akekuri"));

    // 7. Clicking Delete (🗑) button at (388, 492) enters inline delete confirmation
    assert_eq!(h.all_notes_confirming_delete(), Some(false));
    h.click_all_notes(388.0, 492.0);
    assert_eq!(h.all_notes_confirming_delete(), Some(true));

    // 8. Clicking "Keep" button at (456, 495) cancels delete confirmation
    h.click_all_notes(456.0, 495.0);
    assert_eq!(h.all_notes_confirming_delete(), Some(false));
    assert!(!h.is_deleted(endfield_id));

    // 9. Pressing Escape while confirming delete cancels confirmation without closing All Notes
    h.click_all_notes(388.0, 492.0);
    assert_eq!(h.all_notes_confirming_delete(), Some(true));
    h.press_all_notes_escape();
    assert_eq!(h.all_notes_confirming_delete(), Some(false));
    assert!(h.is_all_notes_open());

    // 10. Clicking "Delete" button at (519, 496) permanently deletes the note and selects next note
    h.click_all_notes(388.0, 492.0);
    assert_eq!(h.all_notes_confirming_delete(), Some(true));
    h.click_all_notes(519.0, 496.0);
    assert_eq!(h.all_notes_confirming_delete(), Some(false));
    assert!(h.is_deleted(endfield_id));
    assert_eq!(h.open_note_count(), 1);
    assert_eq!(h.all_notes_open_count(), Some(1));
    assert_eq!(h.all_notes_selected_id(), Some(grocery_id));

    // 11. Clicking "+ New note" button in the sidebar creates a new note and selects it
    // With 1 open note and 4 closed notes, new-button-cy = (225 + 1*38) + 46.0 + 4*38 = 461.0
    h.click_all_notes(84.0, 461.0);
    assert_eq!(h.open_note_count(), 2);
    assert_eq!(h.all_notes_open_count(), Some(2));

    // 12. Pressing Escape when not confirming delete closes All Notes window
    h.press_all_notes_escape();
    assert!(!h.is_all_notes_open());
}

fn mean_abs_error(a: &[u8], b: &[u8]) -> f64 {
    assert_eq!(a.len(), b.len());
    let sum: u64 = a
        .iter()
        .zip(b.iter())
        .map(|(&x, &y)| (x as i32 - y as i32).unsigned_abs() as u64)
        .sum();
    sum as f64 / a.len() as f64
}

fn ink_bbox(rgb: &[u8], width: u32, x0: u32, x1: u32, y0: u32, y1: u32, thresh: u8) -> (i32, i32, i32, i32) {
    let (mut min_x, mut max_x) = (i32::MAX, i32::MIN);
    let (mut min_y, mut max_y) = (i32::MAX, i32::MIN);
    for y in y0..y1 {
        for x in x0..x1 {
            let i = ((y * width + x) * 3) as usize;
            let (r, g, b) = (rgb[i], rgb[i + 1], rgb[i + 2]);
            if r < thresh && g < thresh && b < thresh.saturating_add(10) {
                min_x = min_x.min(x as i32);
                max_x = max_x.max(x as i32);
                min_y = min_y.min(y as i32);
                max_y = max_y.max(y as i32);
            }
        }
    }
    assert!(min_x <= max_x, "no ink found in region x={x0}..{x1}, y={y0}..{y1}");
    (min_x, max_x, min_y, max_y)
}

#[test]
fn external_desktop_note_lifecycle_updates_open_all_notes_window_and_exits_when_all_closed() {
    let mut h = setup_mockup_all_notes();
    assert_eq!(h.all_notes_open_count(), Some(2));
    assert_eq!(h.all_notes_closed_count(), Some(4));

    let open_items = h.all_notes_open_items();
    let endfield_id = open_items[0].id as i64;
    assert_eq!(h.all_notes_selected_id(), Some(endfield_id));

    // 1. Editing an open note's text from the desktop updates AllNotesWindow's sidebar title & preview
    let endfield_idx = (0..h.open_note_count())
        .find(|&i| h.note_id(i) == endfield_id)
        .unwrap();
    h.set_note_text(endfield_idx, "# Updated Endfield\nNew body line");
    assert_eq!(h.all_notes_open_items()[0].title.as_str(), "Updated Endfield");
    let preview_str: String = h.all_notes_preview_runs().iter().map(|r| r.text.as_str()).collect();
    assert!(preview_str.contains("Updated Endfield"));
    assert!(preview_str.contains("New body line"));

    // 2. Creating a new note from a desktop note window updates AllNotesWindow's open count & list while keeping selection
    h.right_click(endfield_idx, Some(&["New note"]));
    assert_eq!(h.open_note_count(), 3);
    assert_eq!(h.all_notes_open_count(), Some(3));
    assert_eq!(h.all_notes_selected_id(), Some(endfield_id));
    let new_idx = h.open_note_count() - 1;
    h.set_note_text(new_idx, "# Fresh Desktop Note");
    assert!(
        h.all_notes_open_items()
            .iter()
            .any(|i| i.title.as_str() == "Fresh Desktop Note")
    );

    // 3. Closing all desktop notes while AllNotesWindow is open updates counts/lists and keeps the app alive
    while h.open_note_count() > 0 {
        h.close_note(0);
    }
    assert_eq!(h.all_notes_open_count(), Some(0));
    assert_eq!(h.all_notes_closed_count(), Some(7));
    assert!(!h.has_quit(), "app stays alive while AllNotesWindow is still open");

    // 4. Closing AllNotesWindow after all desktop notes are closed quits the app
    h.press_all_notes_escape();
    assert!(!h.is_all_notes_open());
    assert!(h.has_quit(), "closing AllNotesWindow when no desktop notes are open exits the app");
}

#[test]
fn all_notes_side_by_side_mockup_fidelity() {
    let mut h = setup_mockup_all_notes();

    // Render 1.0x normal state (603 x 625)
    h.set_all_notes_confirming_delete(false);
    let normal_1x = h.render_all_notes_canvas(1.0, (603, 625)).expect("render normal 1x");
    assert_eq!((normal_1x.width, normal_1x.height), (603, 625));

    // Render 1.6305x normal state on 1001 x 1024 desk canvas
    let normal_hi = h.render_all_notes_canvas(1.6305, (1001, 1024)).expect("render normal 1.63x");
    assert_eq!((normal_hi.width, normal_hi.height), (1001, 1024));

    // Render 1.0x delete confirmation state (603 x 625)
    h.set_all_notes_confirming_delete(true);
    let delete_1x = h.render_all_notes_canvas(1.0, (603, 625)).expect("render delete 1x");
    assert_eq!((delete_1x.width, delete_1x.height), (603, 625));

    // Load reference mockups
    let ref_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/design/reference");
    let (nw, nh, ref_normal_rgb) = load_png_rgb(&ref_dir.join("all-notes-normal-1x.png"));
    let (dw, dh, ref_delete_rgb) = load_png_rgb(&ref_dir.join("all-notes-delete.png"));
    assert_eq!((nw, nh), (603, 625));
    assert_eq!((dw, dh), (603, 625));

    // Build side-by-side comparison images in memory on every test run
    let (sbs_nw, sbs_nh, sbs_normal_rgb) = build_side_by_side_rgb(603, 625, &ref_normal_rgb, normal_1x.rgb());
    let (sbs_dw, sbs_dh, sbs_delete_rgb) = build_side_by_side_rgb(603, 625, &ref_delete_rgb, delete_1x.rgb());
    assert_eq!((sbs_nw, sbs_nh), (603 * 3 + 8, 625));
    assert_eq!((sbs_dw, sbs_dh), (603 * 3 + 8, 625));

    // Optionally dump rendered & side-by-side PNGs if WASHI_SNAP_OUT_DIR is set
    if let Some(out_dir) = std::env::var_os("WASHI_SNAP_OUT_DIR") {
        let out_dir = Path::new(&out_dir);
        write_png_rgb(&out_dir.join("rendered_normal.png"), 603, 625, normal_1x.rgb());
        write_png_rgb(&out_dir.join("rendered_delete.png"), 603, 625, delete_1x.rgb());
        write_png_rgb(&out_dir.join("side_by_side_normal.png"), sbs_nw, sbs_nh, &sbs_normal_rgb);
        write_png_rgb(&out_dir.join("side_by_side_delete.png"), sbs_dw, sbs_dh, &sbs_delete_rgb);
    }

    // 1. Assert overall Mean Absolute Error against reference mockups (< 3.0 out of 255)
    let mae_normal = mean_abs_error(&ref_normal_rgb, normal_1x.rgb());
    let mae_delete = mean_abs_error(&ref_delete_rgb, delete_1x.rgb());
    assert!(
        mae_normal < 3.0,
        "normal state MAE should be < 3.0, got {mae_normal:.3}"
    );
    assert!(
        mae_delete < 2.5,
        "delete state MAE should be < 2.5, got {mae_delete:.3}"
    );

    // 2. Assert ink bounding-box alignment (within 2px) for every text region in sidebar and preview card
    let regions = [
        ("Notes", 25, 110, 110, 155, 160),
        ("2 open · 4 closed", 25, 130, 160, 188, 175),
        ("OPEN", 30, 80, 192, 214, 175),
        ("Endfield Note (sidebar)", 35, 130, 222, 248, 160),
        ("Grocery run", 35, 130, 264, 290, 160),
        ("CLOSED", 35, 95, 298, 320, 175),
        ("Meeting ideas", 35, 135, 328, 358, 160),
        ("Birthday gifts", 35, 135, 366, 396, 160),
        ("Reading list", 35, 135, 404, 434, 160),
        ("Trip to Kyoto", 35, 135, 442, 472, 160),
        ("Status line", 260, 460, 125, 152, 185),
        ("Preview Title", 260, 445, 152, 195, 160),
        ("Preview Heading", 260, 495, 204, 232, 160),
        ("Body Line 1", 260, 538, 238, 263, 160),
        ("Body Line 2", 260, 548, 260, 286, 160),
        ("Body Line 3", 260, 380, 282, 308, 160),
        ("Delete for good?", 255, 370, 475, 505, 160),
        ("Keep text", 435, 475, 480, 505, 160),
    ];

    for (name, x0, x1, y0, y1, th) in regions {
        let b_ref = ink_bbox(&ref_delete_rgb, 603, x0, x1, y0, y1, th);
        let b_ren = ink_bbox(delete_1x.rgb(), 603, x0, x1, y0, y1, th);
        assert!(
            (b_ren.0 - b_ref.0).abs() <= 2 && (b_ren.2 - b_ref.2).abs() <= 2,
            "{name} top-left ink bbox mismatch: ref={b_ref:?}, ren={b_ren:?}"
        );
        assert!(
            (b_ren.1 - b_ref.1).abs() <= 3 && (b_ren.3 - b_ref.3).abs() <= 3,
            "{name} bottom-right ink bbox mismatch: ref={b_ref:?}, ren={b_ren:?}"
        );
    }
}
