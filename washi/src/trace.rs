//! Optional tracing: timing marks for `tools/memprobe.ps1`, and work counters for the tests.
//!
//! Timing, for `tools/memprobe.ps1` (startup and wake measurements).
//!
//! When the `WASHI_TRACE` environment variable names a file, [`mark`] appends one line per call:
//! `<unix epoch ms> <label>`. The probe stamps the same clock before it launches or restores a
//! window, so the difference is the time the app needed. Without the variable, `mark` is a single
//! cached lookup and does nothing else.
//!
//! Counters ([`counts`], [`count`]): how often `Layout::build`, `textflow::wrap` and the other costly
//! steps of an input ran. They are kept only in tests and with the `trace` feature; elsewhere
//! [`count`] does nothing.

use std::io::Write;
use std::sync::OnceLock;
use std::time::{SystemTime, UNIX_EPOCH};

fn target() -> Option<&'static std::path::PathBuf> {
    static TARGET: OnceLock<Option<std::path::PathBuf>> = OnceLock::new();
    TARGET.get_or_init(|| std::env::var_os("WASHI_TRACE").map(Into::into)).as_ref()
}

/// One trace line for `label` at `epoch_ms`.
pub fn line(epoch_ms: u128, label: &str) -> String {
    format!("{epoch_ms} {label}\n")
}

/// Record that `label` just happened (no-op unless `WASHI_TRACE` is set).
pub fn mark(label: &str) {
    let Some(path) = target() else { return };
    let ms = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis()).unwrap_or(0);
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(path) {
        let _ = f.write_all(line(ms, label).as_bytes());
    }
}

/// Count one occurrence of `c` (a no-op unless built for tests or with the `trace` feature).
#[inline(always)]
pub fn count(c: counts::Count) {
    counts::bump(c);
}

/// Counts of the work one input causes, for the tests that pin the edit path. Per thread, so tests
/// running side by side do not count each other's work.
pub mod counts {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum Count {
        /// A `Document` was cloned.
        DocumentClone,
        /// A `Document` was written out as text.
        ToText,
        /// `Layout::caret_box` was asked.
        CaretBox,
        /// A window row (`BlockData`) was built for a block.
        BlockData,
        /// `NoteImages::load` ran.
        ImagesLoad,
        /// `Layout::build` ran.
        LayoutBuild,
        /// `textflow::wrap` ran (one block's text broken into lines).
        Wrap,
    }

    #[cfg(any(test, feature = "trace"))]
    const N: usize = 7;

    #[cfg(any(test, feature = "trace"))]
    thread_local! {
        static COUNTS: [std::cell::Cell<usize>; N] = const { [const { std::cell::Cell::new(0) }; N] };
    }

    #[cfg(any(test, feature = "trace"))]
    pub fn bump(c: Count) {
        COUNTS.with(|a| a[c as usize].set(a[c as usize].get() + 1));
    }

    #[cfg(not(any(test, feature = "trace")))]
    pub fn bump(_: Count) {}

    #[cfg(any(test, feature = "trace"))]
    pub fn get(c: Count) -> usize {
        COUNTS.with(|a| a[c as usize].get())
    }

    #[cfg(any(test, feature = "trace"))]
    pub fn reset() {
        COUNTS.with(|a| a.iter().for_each(|c| c.set(0)));
    }

    /// Start counting `c` from zero, leaving the others.
    #[cfg(any(test, feature = "trace"))]
    pub fn clear(c: Count) {
        COUNTS.with(|a| a[c as usize].set(0));
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn line_is_epoch_then_label() {
        assert_eq!(super::line(1700000000123, "wake-begin"), "1700000000123 wake-begin\n");
    }
}
