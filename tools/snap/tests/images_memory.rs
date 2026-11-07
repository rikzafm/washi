//! Own test binary (own process) so no other test's allocations disturb the measurement: three
//! notes with pictures cost about 1 MB per picture while shown, and give it back when suspended.

use snap::Harness;
use std::time::Duration;
use washi::images::{encode_png, Rgba};

#[repr(C)]
struct ProcessMemoryCounters {
    cb: u32,
    page_fault_count: u32,
    peak_working_set_size: usize,
    working_set_size: usize,
    quota_peak_paged_pool_usage: usize,
    quota_paged_pool_usage: usize,
    quota_peak_non_paged_pool_usage: usize,
    quota_non_paged_pool_usage: usize,
    pagefile_usage: usize,
    peak_pagefile_usage: usize,
}

#[link(name = "kernel32")]
extern "system" {
    fn GetCurrentProcess() -> isize;
    fn K32GetProcessMemoryInfo(process: isize, counters: *mut ProcessMemoryCounters, cb: u32) -> i32;
}

/// Private bytes of this process (committed memory).
fn private_bytes() -> usize {
    let mut c: ProcessMemoryCounters = unsafe { std::mem::zeroed() };
    c.cb = std::mem::size_of::<ProcessMemoryCounters>() as u32;
    let ok = unsafe { K32GetProcessMemoryInfo(GetCurrentProcess(), &mut c, c.cb) };
    assert_ne!(ok, 0);
    c.pagefile_usage
}

fn photo(w: u32, h: u32) -> Rgba {
    let mut data = Vec::new();
    let mut seed = 7u32;
    for y in 0..h {
        for x in 0..w {
            seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
            let noise = ((seed >> 24) % 9) as i32 - 4;
            let c = |v: f32| (v as i32 + noise).clamp(0, 255) as u8;
            data.extend_from_slice(&[c(x as f32 * 255.0 / w as f32), c(y as f32 * 255.0 / h as f32), 120, 255]);
        }
    }
    Rgba { width: w, height: h, data }
}

#[test]
fn suspending_gives_the_decoded_pictures_back() {
    let dir = tempfile::tempdir().unwrap();
    let mut h = Harness::new();
    h.start().unwrap();
    h.press_ctrl(0, 'n');
    h.press_ctrl(0, 'n');
    let path = dir.path().join("p.png");
    std::fs::write(&path, encode_png(&photo(900, 700)).unwrap()).unwrap();
    for n in 0..3 {
        h.type_text(n, "note");
    }
    // warm up once (fonts, caches), then measure a note with and without its picture
    for n in 0..3 {
        h.choose_picture(&path);
        h.click_image_button(n);
        h.wait_for_images();
        h.render(n);
    }
    h.advance(Duration::from_millis(600));
    let decoded = h.decoded_image_bytes();
    assert!(decoded >= 3 * 900_000 && decoded <= 3 * 1_100_000, "{decoded} bytes decoded for 3 pictures");

    let awake = private_bytes();
    for n in 0..3 {
        h.minimize(n);
    }
    h.advance(Duration::from_secs(10));
    assert!(h.is_suspended());
    let suspended = private_bytes();
    let given_back = awake.saturating_sub(suspended);
    // the 3 decoded pictures are about 3 MB; allow for the allocator keeping some of it
    assert!(given_back >= 2 * 1024 * 1024, "suspending gave back {given_back} bytes (decoded: {decoded})");
}
