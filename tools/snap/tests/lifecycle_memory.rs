//! Own test binary (own process) so no other test's allocations disturb the measurement.

use snap::Harness;
use std::time::Duration;

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

/// Private bytes of this process (committed memory), which does not wobble like the working set.
fn private_bytes() -> usize {
    let mut c: ProcessMemoryCounters = unsafe { std::mem::zeroed() };
    c.cb = std::mem::size_of::<ProcessMemoryCounters>() as u32;
    let ok = unsafe { K32GetProcessMemoryInfo(GetCurrentProcess(), &mut c, c.cb) };
    assert_ne!(ok, 0);
    c.pagefile_usage
}

#[test]
fn opening_and_closing_ten_notes_returns_memory_to_where_it_was() {
    let mut h = Harness::new();
    h.start().unwrap();
    // Warm up once so one-time allocations (fonts, caches) are not counted.
    for _ in 0..10 {
        h.press_ctrl(0, 'n');
        h.type_text(1, "warm up");
        h.advance(Duration::from_millis(600));
        h.press_ctrl(1, 'w');
    }
    let windows = h.window_count();
    let before = private_bytes();

    for i in 0..10 {
        h.press_ctrl(0, 'n');
        h.type_text(1, &format!("note number {i}"));
        h.advance(Duration::from_millis(600));
        h.press_ctrl(1, 'w');
    }

    let after = private_bytes();
    assert_eq!(h.window_count(), windows, "closed windows are released");
    let grown = after.saturating_sub(before);
    assert!(grown < 512 * 1024, "memory grew by {grown} bytes");
}
