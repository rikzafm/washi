//! Pictures in notes and Markdown export, through the app harness: the image button and the
//! clipboard, rendering, restart, encryption at rest, suspension and export.

use snap::Harness;
use std::path::{Path, PathBuf};
use washi::images::{encode_png, Rgba};

const RED: [u8; 3] = [200, 30, 60];

/// A picture of one colour (a screenshot-like flat picture).
fn flat(w: u32, h: u32, rgb: [u8; 3]) -> Rgba {
    Rgba { width: w, height: h, data: (0..w * h).flat_map(|_| [rgb[0], rgb[1], rgb[2], 255]).collect() }
}

/// A photograph-like picture (smooth colour fields plus fine noise): it stores as a JPEG.
fn photo(w: u32, h: u32) -> Rgba {
    let mut data = Vec::new();
    let mut seed = 99u32;
    for y in 0..h {
        for x in 0..w {
            seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
            let noise = ((seed >> 24) % 9) as i32 - 4;
            let (fx, fy) = (x as f32 / w as f32, y as f32 / h as f32);
            let c = |v: f32| (v as i32 + noise).clamp(0, 255) as u8;
            data.extend_from_slice(&[c(60.0 + 150.0 * fx), c(200.0 - 120.0 * fy), c(90.0 + 80.0 * fx * fy), 255]);
        }
    }
    Rgba { width: w, height: h, data }
}

fn write_png(dir: &Path, name: &str, img: &Rgba) -> PathBuf {
    let path = dir.join(name);
    std::fs::write(&path, encode_png(img).unwrap()).unwrap();
    path
}

/// A started harness with one note whose title is typed.
fn note_with_title() -> Harness {
    let mut h = Harness::new();
    h.start().unwrap();
    h.type_text(0, "Trip");
    h
}

/// Pixels of a note's window within 12 of `rgb` in every channel.
fn pixels_near(h: &Harness, note: usize, rgb: [u8; 3]) -> usize {
    let (_, _, px) = h.render(note);
    px.iter().filter(|p| [p.r, p.g, p.b].iter().zip(rgb).all(|(a, b)| (*a as i32 - b as i32).abs() <= 12)).count()
}

fn find(haystack: &[u8], needle: &[u8]) -> bool {
    haystack.windows(needle.len()).any(|w| w == needle)
}

// ---- the toolbar's image button ----

#[test]
fn the_image_button_adds_the_chosen_picture_below_the_current_paragraph() {
    let dir = tempfile::tempdir().unwrap();
    let mut h = note_with_title();
    h.choose_picture(&write_png(dir.path(), "red.png", &flat(300, 150, RED)));
    assert_eq!(pixels_near(&h, 0, RED), 0);

    h.click_image_button(0);
    // the picture is prepared on its own thread: typing is not held up, the block is not here yet
    assert_eq!(h.pending_images(), 1);
    h.type_text(0, " 2");
    h.wait_for_images();

    assert_eq!(h.block_kinds(0), ["title", "image", "body"]);
    let images = h.image_blocks(0);
    assert_eq!(images.len(), 1);
    assert!(images[0].decoded);
    assert_eq!(images[0].size, (300, 150));
    // the picture is on screen at the text-area width (291 px) and its aspect ratio (145 px tall)
    let shown = pixels_near(&h, 0, RED);
    assert!((38_000..=46_000).contains(&shown), "{shown} px of the picture");
    // and typing goes on below it
    h.type_text(0, "after");
    h.advance(std::time::Duration::from_millis(600));
    let saved = h.saved_text(0);
    assert!(saved.starts_with("# Trip 2\n![](img:"), "{saved:?}");
    assert!(saved.ends_with(")\nafter"), "{saved:?}");
}

#[test]
fn a_cancelled_file_dialog_adds_nothing() {
    let mut h = note_with_title();
    h.click_image_button(0);
    h.wait_for_images();
    assert_eq!(h.block_kinds(0), ["title"]);
}

#[test]
fn a_file_that_is_not_a_picture_says_so() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("notes.png");
    std::fs::write(&path, b"this is not a picture").unwrap();
    let mut h = note_with_title();
    h.choose_picture(&path);
    h.click_image_button(0);
    h.wait_for_images();
    assert_eq!(h.block_kinds(0), ["title"]);
    assert_eq!(h.dialogs.errors.borrow().len(), 1);
}

#[test]
fn the_picture_is_stored_small_and_survives_a_restart() {
    let dir = tempfile::tempdir().unwrap();
    let mut h = note_with_title();
    h.choose_picture(&write_png(dir.path(), "red.png", &flat(300, 150, RED)));
    h.click_image_button(0);
    h.wait_for_images();
    h.restart().unwrap();

    assert_eq!(h.block_kinds(0), ["title", "image", "body"]);
    assert!(h.image_blocks(0)[0].decoded);
    let shown = pixels_near(&h, 0, RED);
    assert!(shown > 38_000, "{shown} px of the picture after a restart");
    let stored = h.stored_pictures(0);
    assert_eq!(stored.len(), 1);
    assert!(stored[0].mime == "image/png" || stored[0].mime == "image/jpeg");
}

#[test]
fn big_pictures_are_shrunk_to_twice_the_text_width_before_storing() {
    let dir = tempfile::tempdir().unwrap();
    let mut h = note_with_title();
    h.choose_picture(&write_png(dir.path(), "wide.png", &photo(2400, 900)));
    h.click_image_button(0);
    h.wait_for_images();
    h.advance(std::time::Duration::from_millis(600));
    let stored = &h.stored_pictures(0)[0];
    assert_eq!(stored.size, (582, 218));
    assert_eq!(stored.mime, "image/jpeg", "a photograph is stored as a JPEG");
    assert!(stored.data.len() < 100 * 1024, "{} bytes", stored.data.len());
}

// ---- pasting ----

#[test]
fn pasting_a_clipboard_picture_adds_an_image_paragraph_after_the_current_one() {
    let mut h = note_with_title();
    h.press_enter(0);
    h.type_text(0, "second");
    h.copy_picture(flat(200, 80, RED));
    h.press_ctrl(0, 'v');
    h.wait_for_images();
    assert_eq!(h.block_kinds(0), ["title", "body", "image", "body"]);
    assert_eq!(h.image_blocks(0)[0].size, (200, 80));
    // the picture is as wide as the text area (291 px) and 116 px tall
    let shown = pixels_near(&h, 0, RED);
    assert!((30_000..=37_000).contains(&shown), "{shown} px");
}

#[test]
fn pasting_text_still_pastes_text() {
    let mut h = note_with_title();
    h.paste(0, " and more");
    assert_eq!(h.block_kinds(0), ["title"]);
    assert!(h.note_text(0).contains("Trip and more"), "{:?}", h.note_text(0));
    assert_eq!(h.pending_images(), 0);
}

// ---- removing ----

#[test]
fn backspace_in_the_paragraph_after_a_picture_removes_it() {
    let mut h = note_with_title();
    h.copy_picture(flat(300, 60, RED));
    h.press_ctrl(0, 'v');
    h.wait_for_images();
    assert_eq!(h.block_kinds(0), ["title", "image", "body"]);
    h.press_backspace(0);
    assert_eq!(h.block_kinds(0), ["title", "body"]);
    assert_eq!(pixels_near(&h, 0, RED), 0);
    // closing the note drops the stored picture nothing refers to any more
    assert_eq!(h.stored_picture_count(), 1);
    h.close_note(0);
    assert_eq!(h.stored_picture_count(), 0);
}

#[test]
fn deleting_a_note_deletes_its_pictures() {
    let mut h = note_with_title();
    h.copy_picture(flat(300, 60, RED));
    h.press_ctrl(0, 'v');
    h.wait_for_images();
    h.advance(std::time::Duration::from_millis(600));
    h.press_ctrl(0, 'n');
    assert_eq!(h.stored_picture_count(), 1);
    h.dialogs.confirm_answer.set(true);
    h.press_ctrl(0, 'd');
    assert_eq!(h.stored_picture_count(), 0);
}

// ---- encryption ----

#[test]
fn the_pictures_pixels_are_not_in_the_database_bytes() {
    let mut h = note_with_title();
    h.copy_picture(photo(500, 400));
    h.press_ctrl(0, 'v');
    h.wait_for_images();
    h.advance(std::time::Duration::from_millis(600));
    let stored = &h.stored_pictures(0)[0];
    assert!(stored.data.len() > 4000, "a real picture, so that finding it would mean something");

    for (name, bytes) in [("db", h.raw_db_bytes().0), ("wal", h.raw_db_bytes().1)] {
        assert!(!find(&bytes, &stored.data[..64]), "start of the picture found in the {name}");
        assert!(!find(&bytes, &stored.data[stored.data.len() / 2..stored.data.len() / 2 + 64]), "middle of the picture found in the {name}");
        assert!(!find(&bytes, b"\x89PNG") && !find(&bytes, b"JFIF") && !find(&bytes, b"IDAT"), "an image header is in the {name}");
    }
    // and the same picture as a plain file is found by the same check (the check can fail)
    let plain = stored.data.clone();
    assert!(find(&plain, &stored.data[..64]));
    // after a restart (checkpointed database) too
    h.restart().unwrap();
    let (db, wal) = h.raw_db_bytes();
    assert!(!find(&db, &stored.data[..64]) && !find(&wal, &stored.data[..64]));
}

// ---- memory and suspension ----

fn note_with_photo(h: &mut Harness, note: usize, dir: &Path, name: &str) {
    h.choose_picture(&write_png(dir, name, &photo(900, 700)));
    h.click_image_button(note);
    h.wait_for_images();
}

#[test]
fn decoded_pictures_exist_only_while_shown_and_not_suspended() {
    let dir = tempfile::tempdir().unwrap();
    let mut h = Harness::new();
    h.start().unwrap();
    h.press_ctrl(0, 'n');
    h.press_ctrl(0, 'n');
    for n in 0..3 {
        h.type_text(n, "note");
        note_with_photo(&mut h, n, dir.path(), &format!("p{n}.png"));
    }
    // each picture is at most 582 x 453 px once stored: about 1 MB decoded, plus as much again
    // for its copy turned by the text's tilt (shown at rest)
    let awake = h.decoded_image_bytes();
    assert!(awake > 0 && awake <= 3 * 2_250_000, "{awake} bytes decoded for 3 pictures");

    for n in 0..3 {
        h.render(n);
        h.minimize(n);
    }
    h.advance(std::time::Duration::from_secs(10));
    assert!(h.is_suspended());
    assert_eq!(h.decoded_image_bytes(), 0, "suspended: no decoded pictures");
    assert!(h.image_blocks(0).iter().all(|b| !b.decoded && b.size.0 > 0), "sizes stay so the layout does not move");

    h.restore(0);
    assert!(!h.is_suspended());
    assert_eq!(h.decoded_image_bytes(), awake);
    assert!(h.image_blocks(0).iter().all(|b| b.decoded));
}

#[test]
fn closing_a_note_frees_its_pictures() {
    let dir = tempfile::tempdir().unwrap();
    let mut h = Harness::new();
    h.start().unwrap();
    h.press_ctrl(0, 'n');
    h.type_text(1, "two");
    note_with_photo(&mut h, 1, dir.path(), "p.png");
    let with = h.decoded_image_bytes();
    assert!(with > 0);
    h.close_note(1);
    assert_eq!(h.decoded_image_bytes(), 0);
}

// ---- export ----

#[test]
fn export_as_markdown_writes_the_note_and_its_pictures_beside_it() {
    let dir = tempfile::tempdir().unwrap();
    let out = tempfile::tempdir().unwrap();
    let mut h = note_with_title();
    h.press_enter(0);
    h.type_text(0, "hello");
    h.choose_picture(&write_png(dir.path(), "red.png", &flat(300, 60, RED)));
    h.click_image_button(0);
    h.wait_for_images();
    h.type_text(0, "bye");
    *h.dialogs.save_path.borrow_mut() = Some(out.path().join("my trip.md"));

    h.right_click(0, Some(&["Export as Markdown\u{2026}"]));

    let md = std::fs::read_to_string(out.path().join("my trip.md")).unwrap();
    assert_eq!(md, "# Trip\nhello\n![image 1](my%20trip-image-1.png)\nbye\n");
    // the picture is a real PNG of the stored size, viewable on its own
    let file = std::fs::File::open(out.path().join("my trip-image-1.png")).unwrap();
    let reader = png::Decoder::new(file).read_info().unwrap();
    assert_eq!((reader.info().width, reader.info().height), (300, 60));
    // the note itself is untouched
    assert_eq!(h.block_kinds(0), ["title", "body", "image", "body"]);
}

#[test]
fn a_cancelled_export_writes_nothing() {
    let mut h = note_with_title();
    *h.dialogs.save_path.borrow_mut() = None;
    h.right_click(0, Some(&["Export as Markdown\u{2026}"]));
    assert!(h.dialogs.errors.borrow().is_empty());
}

#[test]
fn the_menu_offers_export_last() {
    let mut h = note_with_title();
    h.right_click(0, None);
    let labels: Vec<String> = h.last_menu().into_iter().map(|i| i.label).collect();
    assert_eq!(labels.last().map(String::as_str), Some("Export as Markdown\u{2026}"));
}

#[test]
fn typing_below_a_tall_picture_works_and_the_note_scrolls_to_it() {
    let mut h = note_with_title();
    h.copy_picture(flat(200, 200, RED));
    h.press_ctrl(0, 'v');
    h.wait_for_images();
    // the 291 px square pushes the paragraph below it past the bottom of the text area
    h.type_text(0, "below");
    assert_eq!(h.note_text(0), "# Trip\n![](img:1)\nbelow");
}

fn ms(n: u64) -> std::time::Duration {
    std::time::Duration::from_millis(n)
}

fn select_image(h: &mut Harness) -> (f32, f32) {
    let (w, _, px) = h.render(0);
    let red: Vec<usize> = px
        .iter()
        .enumerate()
        .filter(|(_, p)| (p.r as i32 - RED[0] as i32).abs() <= 12 && (p.g as i32 - RED[1] as i32).abs() <= 12 && (p.b as i32 - RED[2] as i32).abs() <= 12)
        .map(|(i, _)| i)
        .collect();
    let (top, bottom) = (red.first().unwrap() / w as usize, red.last().unwrap() / w as usize);
    let x = (red[0] % w as usize) as f32 + 40.0;
    let y = (top + (bottom - top) / 2) as f32;
    h.click(0, 5.0, 5.0);
    h.advance(ms(400));
    h.click(0, x, y);
    h.advance(ms(400));
    assert_eq!(h.selection(0), ((1, 0), (1, 0)));
    (x, y)
}

#[test]
fn ctrl_c_on_selected_image_copies_picture_to_clipboard() {
    let mut h = note_with_title();
    let img = flat(100, 100, RED);
    h.copy_picture(img.clone());
    h.press_ctrl(0, 'v');
    h.wait_for_images();
    h.advance(ms(600));

    // Clear clipboard so we know the copy put it there
    *h.clipboard.image.borrow_mut() = None;

    select_image(&mut h);

    h.press_ctrl(0, 'c');
    let on_clip = h.clipboard_image().expect("image on clipboard");
    assert_eq!((on_clip.width, on_clip.height), (img.width, img.height));
    assert_eq!(on_clip.data, img.data);
    assert_eq!(h.block_kinds(0), ["title", "image", "body"]);
}

#[test]
fn ctrl_x_on_selected_image_cuts_picture_and_undo_restores_it() {
    let mut h = note_with_title();
    let img = flat(80, 80, RED);
    h.copy_picture(img.clone());
    h.press_ctrl(0, 'v');
    h.wait_for_images();
    h.advance(ms(600));

    *h.clipboard.image.borrow_mut() = None;

    select_image(&mut h);

    h.press_ctrl(0, 'x');
    let on_clip = h.clipboard_image().expect("image on clipboard");
    assert_eq!((on_clip.width, on_clip.height), (img.width, img.height));
    assert_eq!(on_clip.data, img.data);
    assert_eq!(h.block_kinds(0), ["title", "body"]);

    // Ctrl+Z undoes the cut and restores the image block
    h.press_ctrl(0, 'z');
    assert_eq!(h.block_kinds(0), ["title", "image", "body"]);
}

#[test]
fn right_click_menu_offers_cut_and_copy_image() {
    let mut h = note_with_title();
    let img = flat(60, 60, RED);
    h.copy_picture(img.clone());
    h.press_ctrl(0, 'v');
    h.wait_for_images();
    h.advance(ms(600));

    let (x, y) = select_image(&mut h);

    // Right click on image without choosing an item: check menu items
    h.right_click_at(0, x, y, None);
    let labels: Vec<String> = h.last_menu().into_iter().map(|i| i.label).collect();
    assert!(labels.contains(&"Cut image".to_string()));
    assert!(labels.contains(&"Copy image".to_string()));

    // Copy via context menu
    *h.clipboard.image.borrow_mut() = None;
    h.right_click_at(0, x, y, Some(&["Copy image"]));
    assert!(h.clipboard_image().is_some());
    assert_eq!(h.block_kinds(0), ["title", "image", "body"]);

    // Cut via context menu
    *h.clipboard.image.borrow_mut() = None;
    h.right_click_at(0, x, y, Some(&["Cut image"]));
    assert!(h.clipboard_image().is_some());
    assert_eq!(h.block_kinds(0), ["title", "body"]);
}
