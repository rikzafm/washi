//! Resource use numbers for Settings › General (pure: sampling and formatting only).

use std::path::Path;
use std::time::Duration;

/// Turns a growing CPU-time counter into a share of the machine over the last interval.
#[derive(Default)]
pub struct CpuSampler {
    last: Option<(Duration, Duration)>,
}

impl CpuSampler {
    pub fn new() -> Self {
        CpuSampler { last: None }
    }

    /// CPU % of the whole machine since the previous sample (`None` on the first one).
    pub fn sample(&mut self, wall: Duration, cpu: Duration, processors: u32) -> Option<f32> {
        let prev = self.last.replace((wall, cpu))?;
        let dw = wall.saturating_sub(prev.0).as_secs_f32();
        if dw <= 0.0 {
            return None;
        }
        Some((cpu.saturating_sub(prev.1).as_secs_f32() / dw / processors.max(1) as f32 * 100.0).clamp(0.0, 100.0))
    }
}

pub fn fmt_bytes(b: u64) -> String {
    let mb = b as f64 / (1024.0 * 1024.0);
    if mb >= 10.0 {
        format!("{mb:.0} MB")
    } else if mb >= 1.0 {
        format!("{mb:.1} MB")
    } else {
        format!("{:.0} KB", b as f64 / 1024.0)
    }
}

pub fn fmt_cpu(p: Option<f32>) -> String {
    p.map_or("…".into(), |p| if p < 0.1 { "0%".into() } else { format!("{p:.1}%") })
}

/// The database on disk: the file plus its WAL (SQLCipher runs in WAL mode).
pub fn db_size(dir: &Path) -> u64 {
    ["notes.db", "notes.db-wal"].iter().filter_map(|n| std::fs::metadata(dir.join(n)).ok()).map(|m| m.len()).sum()
}

/// Shorten a long path in the middle so both the drive and the folder name stay visible.
pub fn elide_middle(path: &str, max_chars: usize) -> String {
    let chars: Vec<char> = path.chars().collect();
    if chars.len() <= max_chars || max_chars < 5 {
        return path.to_string();
    }
    let keep = max_chars - 1;
    let (head, tail) = (keep / 2, keep - keep / 2);
    format!("{}…{}", chars[..head].iter().collect::<String>(), chars[chars.len() - tail..].iter().collect::<String>())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ms(n: u64) -> Duration {
        Duration::from_millis(n)
    }

    #[test]
    fn first_sample_has_no_cpu_figure() {
        let mut s = CpuSampler::new();
        assert_eq!(s.sample(ms(0), ms(0), 4), None);
        assert_eq!(fmt_cpu(None), "…");
    }

    #[test]
    fn ten_ms_of_cpu_in_a_second_on_four_processors_is_a_quarter_percent() {
        let mut s = CpuSampler::new();
        s.sample(ms(1000), ms(50), 4);
        let p = s.sample(ms(2000), ms(60), 4).unwrap();
        assert!((p - 0.25).abs() < 1e-3, "{p}");
        assert_eq!(fmt_cpu(Some(1.26)), "1.3%");
    }

    #[test]
    fn cpu_is_clamped_and_a_stalled_clock_gives_none() {
        let mut s = CpuSampler::new();
        s.sample(ms(0), ms(0), 1);
        assert_eq!(s.sample(ms(1000), ms(5000), 1), Some(100.0));
        assert_eq!(s.sample(ms(1000), ms(6000), 1), None);
        assert_eq!(fmt_cpu(Some(0.02)), "0%");
    }

    #[test]
    fn bytes_format_at_the_edges() {
        assert_eq!(fmt_bytes(0), "0 KB");
        assert_eq!(fmt_bytes(512 * 1024), "512 KB");
        assert_eq!(fmt_bytes(1024 * 1024), "1.0 MB");
        assert_eq!(fmt_bytes(10 * 1024 * 1024 - 1), "10.0 MB");
        assert_eq!(fmt_bytes(10 * 1024 * 1024), "10 MB");
        assert_eq!(fmt_bytes(250 * 1024 * 1024), "250 MB");
    }

    #[test]
    fn db_size_adds_the_wal_and_ignores_missing_files() {
        let d = tempfile::tempdir().unwrap();
        assert_eq!(db_size(d.path()), 0);
        std::fs::write(d.path().join("notes.db"), vec![0u8; 100]).unwrap();
        std::fs::write(d.path().join("notes.db-wal"), vec![0u8; 20]).unwrap();
        std::fs::write(d.path().join("key.bin"), vec![0u8; 999]).unwrap();
        assert_eq!(db_size(d.path()), 120);
    }

    #[test]
    fn long_paths_elide_in_the_middle() {
        assert_eq!(elide_middle("C:\\short", 20), "C:\\short");
        let e = elide_middle("C:\\Users\\someone\\AppData\\Roaming\\Washi", 20);
        assert_eq!(e.chars().count(), 20);
        assert!(e.starts_with("C:\\Users") && e.ends_with("Washi") && e.contains('…'));
    }
}
