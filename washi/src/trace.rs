//! Optional timing trace for `tools/memprobe.ps1` (startup and wake measurements).
//!
//! When the `WASHI_TRACE` environment variable names a file, [`mark`] appends one line per call:
//! `<unix epoch ms> <label>`. The probe stamps the same clock before it launches or restores a
//! window, so the difference is the time the app needed. Without the variable, `mark` is a single
//! cached lookup and does nothing else.

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

#[cfg(test)]
mod tests {
    #[test]
    fn line_is_epoch_then_label() {
        assert_eq!(super::line(1700000000123, "wake-begin"), "1700000000123 wake-begin\n");
    }
}
