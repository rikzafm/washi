//! `seed --data-dir <dir> --notes <N> [--text-file <file>]`
//!
//! Fills a throwaway Washi data folder with N open notes, encrypted with a real DPAPI-wrapped key,
//! so `tools/memprobe.ps1` can launch the release build on N windows (`WASHI_DATA_DIR=<dir>`).

use std::path::PathBuf;
use washi::platform::Clock;
use washi::storage::{keystore, Db};
use washi::win::{DpapiProtector, SystemClock};

fn run() -> Result<(), String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (mut dir, mut notes, mut text) = (None::<PathBuf>, 1usize, "# Endfield Note\n## **Thermite Cutter : POG & Akekuri**\nCurrently POG no Weapon : Ideal for him are Never Rest or White Night Nova. The last can be get in a box".to_string());
    let mut i = 0;
    while i < args.len() {
        let v = args.get(i + 1).ok_or_else(|| format!("{} needs a value", args[i]))?;
        match args[i].as_str() {
            "--data-dir" => dir = Some(PathBuf::from(v)),
            "--notes" => notes = v.parse().map_err(|_| "--notes needs a number")?,
            "--text-file" => text = std::fs::read_to_string(v).map_err(|e| format!("{v}: {e}"))?.replace("\r\n", "\n").trim_end_matches('\n').to_string(),
            other => return Err(format!("unknown argument {other}")),
        }
        i += 2;
    }
    let dir = dir.ok_or("usage: seed --data-dir <dir> --notes <N> [--text-file <file>]")?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let clock = SystemClock::new();
    let now = clock.unix_millis();
    let key = keystore::load_or_create(&dir.join("key.bin"), &DpapiProtector).map_err(|e| e.to_string())?;
    let db = Db::open(&dir.join("notes.db"), &key, now).map_err(|e| e.to_string())?;
    for _ in 0..notes {
        let id = db.create_note(now).map_err(|e| e.to_string())?;
        db.save_content(id, &text, now).map_err(|e| e.to_string())?;
    }
    println!("seeded {notes} note(s) in {}", dir.display());
    Ok(())
}

fn main() {
    if let Err(e) = run() {
        eprintln!("seed: {e}");
        std::process::exit(1);
    }
}
