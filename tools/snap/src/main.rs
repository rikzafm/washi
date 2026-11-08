//! `snap --fixture mockup --scale 1.5 --paper 363x433 --hover --out target/fidelity/hover.png`
//!
//! Renders the real note component headlessly (see the `snap` library) and writes a PNG on the desk
//! color at the mockup canvas size. Two runs give byte-identical files.
//!
//! --fixture <name|path>  text of the note: `docs/design/fixtures/<name>.md` or a file path (default mockup)
//! --fixtures-dir <dir>   where named fixtures live (default docs/design/fixtures)
//! --scale <n>            physical px per logical px (default 1.5)
//! --paper <WxH>          paper size in logical px (default 363x384)
//! --hover                move a real pointer over the note first
//! --editing              the note has the keyboard focus (level text, caret shown)
//! --caret <n>            the paragraph holding the caret (sets the toolbar's active buttons; default the title)
//! --canvas <WxH>         output size in px (default 720x840)
//! --desk <RRGGBB>        desk color (default D9DDE1)
//! --out <png>            output file (default snap.png)

use snap::shot::{snapshot, Options};
use std::path::PathBuf;

fn usage() -> ! {
    eprintln!("usage: snap [--fixture <name|path>] [--fixtures-dir <dir>] [--scale <n>] [--paper <WxH>] [--hover] [--editing] [--caret <n>] [--canvas <WxH>] [--desk <RRGGBB>] [--out <png>]");
    std::process::exit(2)
}

fn pair(s: &str) -> Result<(f64, f64), String> {
    let (w, h) = s.split_once('x').ok_or_else(|| format!("'{s}' is not WxH"))?;
    Ok((w.parse().map_err(|_| format!("'{s}' is not WxH"))?, h.parse().map_err(|_| format!("'{s}' is not WxH"))?))
}

fn run() -> Result<(), String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut o = Options::default();
    let (mut fixture, mut fixtures_dir, mut out) = ("mockup".to_string(), PathBuf::from("docs/design/fixtures"), PathBuf::from("snap.png"));
    let mut i = 0;
    let mut value = |i: &mut usize| -> Result<String, String> {
        *i += 1;
        args.get(*i).cloned().ok_or_else(|| format!("{} needs a value", args[*i - 1]))
    };
    while i < args.len() {
        match args[i].as_str() {
            "--fixture" => fixture = value(&mut i)?,
            "--fixtures-dir" => fixtures_dir = PathBuf::from(value(&mut i)?),
            "--scale" => o.scale = value(&mut i)?.parse().map_err(|_| "--scale needs a number")?,
            "--paper" => o.paper = pair(&value(&mut i)?)?,
            "--canvas" => {
                let (w, h) = pair(&value(&mut i)?)?;
                o.canvas = (w as u32, h as u32);
            }
            "--desk" => {
                let v = value(&mut i)?;
                let n = u32::from_str_radix(v.trim_start_matches('#'), 16).map_err(|_| "--desk needs RRGGBB")?;
                o.desk = [(n >> 16) as u8, (n >> 8) as u8, n as u8];
            }
            "--hover" => o.hover = true,
            "--editing" => o.editing = true,
            "--caret" => o.caret_block = Some(value(&mut i)?.parse().map_err(|_| "--caret needs a block number")?),
            "--out" => out = PathBuf::from(value(&mut i)?),
            _ => usage(),
        }
        i += 1;
    }
    let path = if PathBuf::from(&fixture).is_file() { PathBuf::from(&fixture) } else { fixtures_dir.join(format!("{fixture}.md")) };
    o.text = std::fs::read_to_string(&path).map_err(|e| format!("fixture {}: {e}", path.display()))?;
    // fixtures are checked in with the line endings of the platform; the note always holds \n
    o.text = o.text.replace("\r\n", "\n").trim_end_matches('\n').to_string();
    let shot = snapshot(&o)?;
    if let Some(dir) = out.parent().filter(|d| !d.as_os_str().is_empty()) {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    std::fs::write(&out, shot.png()).map_err(|e| format!("{}: {e}", out.display()))?;
    println!("{}", out.display());
    Ok(())
}

fn main() {
    if let Err(e) = run() {
        eprintln!("snap: {e}");
        std::process::exit(1);
    }
}
