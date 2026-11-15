//! Binary-size probe: baseline + the baker, the Slint font registration and a zip reader.
use slint::fontique_011::{fontique, shared_collection};
use std::io::Read;
use washi_font_spikes::bake::*;
use washi_font_spikes::*;

fn main() {
    install_headless();
    let rig = Rig::new();
    let args: Vec<String> = std::env::args().collect();
    let font = std::fs::read(&args[1]).unwrap();
    let set = bake(&font, Axes { wght: Some(500.0), wdth: None, slnt: None }, BoldSource::Axis(300.0), "Washi Text 1", "Washi Caret 1");
    let mut c = shared_collection();
    for b in [set.regular, set.bold, set.italic, set.bold_italic, set.caret] {
        c.register_fonts(fontique::Blob::new(std::sync::Arc::new(b)), None);
    }
    if let Some(z) = args.get(2) {
        let mut a = zip::ZipArchive::new(std::fs::File::open(z).unwrap()).unwrap();
        for i in 0..a.len() {
            let mut f = a.by_index(i).unwrap();
            let mut v = Vec::new();
            f.read_to_end(&mut v).unwrap();
            println!("{} {}", f.name(), v.len());
        }
    }
    println!("{}", rig.render().ink_count(0, 200));
}
