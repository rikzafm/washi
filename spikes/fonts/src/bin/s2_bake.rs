//! S2: baked static faces are accepted by ttf-parser and Slint, advances equal the variable font's.
use slint::fontique_011::{fontique, shared_collection};
use slint::SharedString;
use washi_font_spikes::bake::*;
use washi_font_spikes::*;

use fontique::{Blob, FontInfoOverride, FontStyle, FontWeight};

fn reg(bytes: &[u8], family: &str, weight: f32, italic: bool) {
    let mut c = shared_collection();
    let r = c.register_fonts(
        Blob::new(std::sync::Arc::new(bytes.to_vec())),
        Some(FontInfoOverride {
            family_name: Some(family),
            weight: Some(FontWeight::new(weight)),
            style: Some(if italic { FontStyle::Italic } else { FontStyle::Normal }),
            ..Default::default()
        }),
    );
    assert!(!r.is_empty(), "Slint/fontique refused {family} w{weight} italic={italic}");
}

fn tables(bytes: &[u8]) -> Vec<String> {
    let f = ttf_parser::RawFace::parse(bytes, 0).unwrap();
    f.table_records.into_iter().map(|r| r.tag.to_string()).collect()
}

fn check(name: &str, src: &[u8], axes: Axes, bold: BoldSource, rig: &Rig, n: u32) {
    let fam = format!("Washi Text {n}");
    let cfam = format!("Washi Caret {n}");
    let t = std::time::Instant::now();
    let set = bake(src, axes, bold, &fam, &cfam);
    println!("== {name}: baked in {:.1} ms; sizes reg {} bold {} ital {} bi {} caret {} bytes", t.elapsed().as_secs_f64() * 1000.0,
        set.regular.len(), set.bold.len(), set.italic.len(), set.bold_italic.len(), set.caret.len());
    println!("   tables: {:?}", tables(&set.regular));
    for (nm, b) in [("regular", &set.regular), ("bold", &set.bold), ("italic", &set.italic), ("bolditalic", &set.bold_italic), ("caret", &set.caret)] {
        let f = ttf_parser::Face::parse(b, 0).unwrap_or_else(|e| panic!("ttf-parser rejects {nm}: {e}"));
        let mism = (0..f.number_of_glyphs()).filter(|&g| f.glyph_hor_advance(ttf_parser::GlyphId(g)) != Some(set.advances[g as usize])).count();
        println!("   {nm:10} parses; {} glyphs; advance mismatches vs regular = {mism}; weight {}", f.number_of_glyphs(), f.weight().to_number());
        assert_eq!(mism, 0);
    }
    // advances equal the variable font's at that location (ttf-parser with HVAR as the second opinion)
    let mut vf = ttf_parser::Face::parse(src, 0).unwrap();
    if let Some(w) = axes.wght {
        vf.set_variation(ttf_parser::Tag::from_bytes(b"wght"), w);
    }
    if let Some(w) = axes.wdth {
        vf.set_variation(ttf_parser::Tag::from_bytes(b"wdth"), w);
    }
    let mut bad = 0;
    let mut maxd = 0i32;
    for g in 0..vf.number_of_glyphs() {
        let a = vf.glyph_hor_advance(ttf_parser::GlyphId(g)).unwrap_or(0) as i32;
        let d = (a - set.advances[g as usize] as i32).abs();
        maxd = maxd.max(d);
        if d > 1 {
            bad += 1;
        }
    }
    println!("   advances vs ttf-parser variable (HVAR) at the location: {bad} differ by >1 unit (max diff {maxd} units)");
    let rf = ttf_parser::Face::parse(&set.regular, 0).unwrap();
    println!("   GSUB {} GPOS {} kern {}", rf.tables().gsub.is_some(), rf.tables().gpos.is_some(), rf.tables().kern.is_some());

    // Slint accepts them: register and draw
    reg(&set.regular, &fam, 400.0, false);
    reg(&set.bold, &fam, 700.0, false);
    reg(&set.italic, &fam, 400.0, true);
    reg(&set.bold_italic, &fam, 700.0, true);
    reg(&set.caret, &cfam, 400.0, false);
    let p = &rig.probe;
    let text = "Hamburgefonstiv 123";
    p.set_fam(SharedString::from(fam.as_str()));
    let mut hs = vec![];
    for (nm, w, it, file) in [("reg", 400, false, &set.regular), ("bold", 700, false, &set.bold), ("ital", 400, true, &set.italic), ("bi", 700, true, &set.bold_italic)] {
        p.set_wt(w);
        p.set_ital(it);
        let s = rig.render();
        let (a, b) = s.ink_x(TOP.0, TOP.1).unwrap_or((0, 0));
        let (c, d) = s.ink_x(BOTTOM.0, BOTTOM.1).unwrap_or((0, 0));
        println!("   slint {nm:5}: Text ink {a}..{b} (w {}), TextInput ink w {}; advance sum {:.1} px at 40px; ink count {}",
            b - a, d - c, advance_sum(file, text, 40.0), s.ink_count(TOP.0, TOP.1));
        hs.push(s.hash(TOP.0, TOP.1));
        s.save_pgm(&format!("s2_{}_{nm}.pgm", name.replace(' ', "_")));
    }
    println!("   four styles distinct: {}", {
        let mut v = hs.clone();
        v.sort();
        v.dedup();
        v.len() == 4
    });
    // caret face draws nothing
    p.set_fam(SharedString::from(cfam.as_str()));
    p.set_wt(400);
    p.set_ital(false);
    let s = rig.render();
    let (c0, c1) = s.ink_x(BOTTOM.0, BOTTOM.1).unwrap_or((0, 0));
    println!("   caret face ink pixels: Text {} TextInput {} (want 0)", s.ink_count(TOP.0, TOP.1), s.ink_count(BOTTOM.0, BOTTOM.1));
    let _ = (c0, c1);
    std::fs::write(format!("s2_{}_regular.ttf", name.replace(' ', "_")), &set.regular).unwrap();
}

fn main() {
    install_headless();
    let fonts = "C:/Windows/Fonts/";
    let bah = std::fs::read(format!("{fonts}bahnschrift.ttf")).unwrap();
    let seg = std::fs::read(format!("{fonts}segoeui.ttf")).unwrap();
    let segb = std::fs::read(format!("{fonts}segoeuib.ttf")).unwrap();
    let rig = Rig::new();
    let f = skrifa::FontRef::new(&bah).unwrap();
    use skrifa::MetadataProvider;
    for a in f.axes().iter() {
        println!("bahnschrift axis {} {}..{} default {}", a.tag(), a.min_value(), a.max_value(), a.default_value());
    }
    check("bahn w600 wd85", &bah, Axes { wght: Some(600.0), wdth: Some(85.0), slnt: None }, BoldSource::Axis(300.0), &rig, 1);
    check("bahn default", &bah, Axes::default(), BoldSource::Axis(300.0), &rig, 2);
    check("segoe static, sheared 8deg, bold file", &seg, Axes { wght: None, wdth: None, slnt: Some(8.0) }, BoldSource::File(&segb), &rig, 3);
    check("segoe static, double strike", &seg, Axes::default(), BoldSource::DoubleStrike, &rig, 4);
}
