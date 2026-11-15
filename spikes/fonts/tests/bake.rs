//! Seam: `bake()`. Expected advances come from ttf-parser's own variable-font (HVAR) reading, an
//! independent source of truth. Skipped when the system has no Bahnschrift (variable, wght+wdth).
use washi_font_spikes::bake::*;

#[test]
fn baked_faces_keep_the_variable_fonts_advances_at_the_location() {
    let Ok(src) = std::fs::read("C:/Windows/Fonts/bahnschrift.ttf") else { return };
    let set = bake(&src, Axes { wght: Some(600.0), wdth: Some(85.0), slnt: None }, BoldSource::Axis(300.0), "Washi Text 1", "Washi Caret 1");
    let mut vf = ttf_parser::Face::parse(&src, 0).unwrap();
    vf.set_variation(ttf_parser::Tag::from_bytes(b"wght"), 600.0).unwrap();
    vf.set_variation(ttf_parser::Tag::from_bytes(b"wdth"), 85.0).unwrap();
    for face in [&set.regular, &set.bold, &set.italic, &set.bold_italic, &set.caret] {
        let f = ttf_parser::Face::parse(face, 0).expect("ttf-parser accepts the baked face");
        assert!(f.tables().gsub.is_none() && f.tables().gpos.is_none() && f.tables().kern.is_none());
        for g in 0..vf.number_of_glyphs() {
            let want = vf.glyph_hor_advance(ttf_parser::GlyphId(g)).unwrap() as i32;
            let got = f.glyph_hor_advance(ttf_parser::GlyphId(g)).unwrap() as i32;
            assert!((want - got).abs() <= 1, "glyph {g}: {got} vs {want}");
        }
    }
}
