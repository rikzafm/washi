//! Face sets at the font engine's interface: `bake` and `bake_regular`. No window.
//! Expected values come from the fixture generator (`tools/make_test_fonts.py`: every glyph is a
//! box with a hole, so heights, widths and advances are known numbers), from ttf-parser's own
//! reading of the variable font (HVAR), from the bundled Default font, and from the spec.

use washi::fonts::{bake, bake_regular, package_from_font, read_package, AxisValues, FaceSet, FontPackage};

fn fixture(name: &str) -> Vec<u8> {
    let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../tools/snap/tests/fixtures/fonts").join(name);
    std::fs::read(&p).unwrap_or_else(|e| panic!("{}: {e}", p.display()))
}

const TABLES_ALLOWED: [&[u8; 4]; 10] =
    [b"OS/2", b"cmap", b"glyf", b"head", b"hhea", b"hmtx", b"loca", b"maxp", b"name", b"post"];

fn faces(set: &FaceSet) -> [(&'static str, &Vec<u8>); 5] {
    [("regular", &set.regular), ("bold", &set.bold), ("italic", &set.italic), ("bold italic", &set.bold_italic), ("caret", &set.caret)]
}

fn glyph_ids(f: &ttf_parser::Face) -> impl Iterator<Item = ttf_parser::GlyphId> {
    (0..f.number_of_glyphs()).map(ttf_parser::GlyphId)
}

fn adv(f: &ttf_parser::Face, g: ttf_parser::GlyphId) -> u16 {
    f.glyph_hor_advance(g).unwrap()
}

fn sans() -> FontPackage {
    read_package(&fixture("Test_Sans.zip")).unwrap()
}

fn source_at(bytes: &[u8], wght: Option<f32>, wdth: Option<f32>) -> ttf_parser::Face<'_> {
    let mut f = ttf_parser::Face::parse(bytes, 0).unwrap();
    if let Some(w) = wght {
        f.set_variation(ttf_parser::Tag::from_bytes(b"wght"), w).unwrap();
    }
    if let Some(w) = wdth {
        f.set_variation(ttf_parser::Tag::from_bytes(b"wdth"), w).unwrap();
    }
    f
}

/// The ink box (x_min, y_min, x_max, y_max) of the glyph for `c`.
fn bbox(bytes: &[u8], c: char) -> (i16, i16, i16, i16) {
    let f = ttf_parser::Face::parse(bytes, 0).unwrap();
    let r = f.glyph_bounding_box(f.glyph_index(c).unwrap()).expect("has ink");
    (r.x_min, r.y_min, r.x_max, r.y_max)
}

fn advance_of(bytes: &[u8], c: char) -> i32 {
    let f = ttf_parser::Face::parse(bytes, 0).unwrap();
    adv(&f, f.glyph_index(c).unwrap()) as i32
}

#[test]
fn all_five_faces_parse_and_share_regulars_advances_at_several_axis_locations() {
    let pkg = sans();
    for (w, wd) in [(None, None), (Some(300.0), Some(75.0)), (Some(800.0), Some(100.0)), (Some(550.0), Some(90.0)), (Some(400.0), Some(75.0))] {
        let set = bake(&pkg, AxisValues { weight: w, width: wd, slant: None }).expect("bakes");
        let src = source_at(&pkg.regular, w, wd);
        let regular = ttf_parser::Face::parse(&set.regular, 0).unwrap();
        for g in glyph_ids(&src) {
            // the source's interpolated advance, rounded to a unit
            assert!((adv(&regular, g) as i32 - adv(&src, g) as i32).abs() <= 1, "{w:?} {wd:?} glyph {}", g.0);
        }
        for (name, bytes) in faces(&set) {
            let f = ttf_parser::Face::parse(bytes, 0).unwrap_or_else(|e| panic!("{name} does not parse: {e:?}"));
            assert_eq!(f.number_of_glyphs(), regular.number_of_glyphs(), "{name}");
            for g in glyph_ids(&regular) {
                assert_eq!(adv(&f, g), adv(&regular, g), "{name} glyph {} at {w:?} {wd:?}", g.0);
            }
            let t = f.tables();
            assert!(t.gsub.is_none() && t.gpos.is_none() && t.kern.is_none(), "{name}");
            for rec in f.raw_face().table_records {
                assert!(TABLES_ALLOWED.contains(&&rec.tag.to_bytes()), "{name} has table {:?}", rec.tag);
            }
        }
    }
}

#[test]
fn a_static_font_gives_five_faces_with_the_sources_advances() {
    for name in ["Test_Serif.zip", "Test_Plain.zip"] {
        let pkg = read_package(&fixture(name)).unwrap();
        let set = bake(&pkg, AxisValues::default()).expect("bakes");
        let src = ttf_parser::Face::parse(&pkg.regular, 0).unwrap();
        for (face, bytes) in faces(&set) {
            let f = ttf_parser::Face::parse(bytes, 0).unwrap();
            for g in glyph_ids(&src) {
                assert_eq!(adv(&f, g), adv(&src, g), "{name} {face} glyph {}", g.0);
            }
        }
    }
}

#[test]
fn axis_values_outside_the_range_are_clamped() {
    let pkg = sans();
    let over = bake(&pkg, AxisValues { weight: Some(5000.0), width: Some(1.0), slant: None }).unwrap();
    let edge = bake(&pkg, AxisValues { weight: Some(800.0), width: Some(75.0), slant: None }).unwrap();
    for c in ['H', 'o', ' '] {
        assert_eq!(advance_of(&over.regular, c), advance_of(&edge.regular, c));
    }
}

#[test]
fn the_caret_face_is_blank_with_the_cut_down_ascent_and_descent() {
    let set = bake(&sans(), AxisValues::default()).unwrap();
    let f = ttf_parser::Face::parse(&set.caret, 0).unwrap();
    assert_eq!(f.units_per_em(), 1000);
    for c in "Hxg 7".chars() {
        let g = f.glyph_index(c).unwrap();
        assert!(f.glyph_bounding_box(g).is_none(), "caret glyph {c:?} has ink");
    }
    // 861/1024 and 100/1024 of the em (the current Caret face), at 1000 units per em
    assert_eq!(f.ascender(), 841);
    assert_eq!(f.descender(), -98);
    // the text faces keep the font's own
    let r = ttf_parser::Face::parse(&set.regular, 0).unwrap();
    assert_eq!((r.ascender(), r.descender()), (900, -300));
}

#[test]
fn every_face_set_gets_a_fresh_private_family_name() {
    let pkg = sans();
    let (a, b) = (bake(&pkg, AxisValues::default()).unwrap(), bake(&pkg, AxisValues::default()).unwrap());
    assert!(a.text_family.starts_with("Washi Text ") && a.caret_family.starts_with("Washi Caret "));
    assert_ne!(a.text_family, b.text_family);
    assert_ne!(a.caret_family, b.caret_family);
    let family_of = |bytes: &[u8]| {
        let f = ttf_parser::Face::parse(bytes, 0).unwrap();
        f.names().into_iter().find(|n| n.name_id == 1).and_then(|n| n.to_string()).unwrap()
    };
    for (_, bytes) in [("r", &a.regular), ("b", &a.bold), ("i", &a.italic), ("bi", &a.bold_italic)] {
        assert_eq!(family_of(bytes), a.text_family);
    }
    assert_eq!(family_of(&a.caret), a.caret_family);
}

fn weight_class(bytes: &[u8]) -> u16 {
    ttf_parser::Face::parse(bytes, 0).unwrap().weight().to_number()
}

#[test]
fn bold_is_weight_plus_300_capped_at_the_axis_maximum() {
    let pkg = sans();
    let set = bake(&pkg, AxisValues { weight: Some(500.0), width: None, slant: None }).unwrap();
    assert_eq!((weight_class(&set.regular), weight_class(&set.bold)), (500, 800));
    let set = bake(&pkg, AxisValues { weight: Some(700.0), width: None, slant: None }).unwrap();
    assert_eq!(weight_class(&set.bold), 800, "capped at the font's maximum");
    // heavier, and centred in Regular's advance (the bold glyph is wider than the regular one)
    let (r, b) = (bbox(&set.regular, 'H'), bbox(&set.bold, 'H'));
    assert!(b.2 - b.0 > r.2 - r.0);
    assert_eq!(((b.0 as i32 + b.2 as i32) - (r.0 as i32 + r.2 as i32)).abs() <= 2, true, "centred");
}

#[test]
fn without_a_weight_axis_bold_comes_from_the_packages_bold_file() {
    let pkg = read_package(&fixture("Test_Serif.zip")).unwrap();
    let set = bake(&pkg, AxisValues::default()).unwrap();
    let bold_file = fixture("TestSerif-Bold.ttf");
    let (b, want) = (bbox(&set.bold, 'H'), bbox(&bold_file, 'H'));
    assert_eq!(b.2 - b.0, want.2 - want.0, "the Bold file's outline");
    let dx = (advance_of(&set.regular, 'H') - advance_of(&bold_file, 'H')) / 2;
    assert!((b.0 as i32 - (want.0 as i32 + dx)).abs() <= 1, "centred in Regular's advance");
    assert_eq!(advance_of(&set.bold, 'H'), advance_of(&set.regular, 'H'));
    assert_eq!(weight_class(&set.bold), 700);
}

#[test]
fn with_neither_a_weight_axis_nor_a_bold_file_bold_is_a_double_strike() {
    let pkg = read_package(&fixture("Test_Plain.zip")).unwrap();
    let set = bake(&pkg, AxisValues::default()).unwrap();
    let (r, b) = (bbox(&set.regular, 'H'), bbox(&set.bold, 'H'));
    // copies shifted by +-em/64 (15.6 units at 1000 per em)
    let grow = (b.2 - b.0) - (r.2 - r.0);
    assert!((30..=33).contains(&grow), "grew by {grow}");
    assert_eq!((b.1, b.3), (r.1, r.3), "no vertical change");
}

#[test]
fn slant_on_a_font_without_a_slant_axis_is_a_shear_and_italic_adds_a_quarter() {
    let pkg = read_package(&fixture("Test_Plain.zip")).unwrap();
    assert!(pkg.axes.slant.is_none());
    let flat = bake(&pkg, AxisValues::default()).unwrap();
    let slanted = bake(&pkg, AxisValues { slant: Some(10.0), ..Default::default() }).unwrap();
    // 'H' is 700 tall: its top right corner moves by tan(10 deg) * 700 = 123.4
    let d = |a: &[u8], b: &[u8]| bbox(a, 'H').2 as f32 - bbox(b, 'H').2 as f32;
    assert!((d(&slanted.regular, &flat.regular) - 123.4).abs() <= 1.5);
    // Italic: a further 0.25 * 700 = 175 on top of the slant
    assert!((d(&flat.italic, &flat.regular) - 175.0).abs() <= 1.5);
    assert!((d(&slanted.italic, &slanted.regular) - 175.0).abs() <= 1.5);
    assert!((d(&slanted.bold_italic, &slanted.bold) - 175.0).abs() <= 1.5);
    // a shear never moves the advances
    assert_eq!(advance_of(&slanted.regular, 'H'), advance_of(&flat.regular, 'H'));
    // beyond 12 degrees is clamped to 12
    let max = bake(&pkg, AxisValues { slant: Some(12.0), ..Default::default() }).unwrap();
    let over = bake(&pkg, AxisValues { slant: Some(45.0), ..Default::default() }).unwrap();
    assert_eq!(bbox(&max.regular, 'H'), bbox(&over.regular, 'H'));
}

#[test]
fn a_font_with_a_slant_axis_uses_it_and_reports_the_lean_as_positive_degrees() {
    let pkg = read_package(&fixture("TestSlant-VariableFont_slnt.ttf")).unwrap();
    let s = pkg.axes.slant.expect("slant axis");
    assert_eq!((s.min, s.default, s.max), (0.0, 0.0, 10.0));
    let flat = bake(&pkg, AxisValues::default()).unwrap();
    let lean = bake(&pkg, AxisValues { slant: Some(10.0), ..Default::default() }).unwrap();
    // the designer's own lean (the fixture's second master shears by tan(10 deg))
    let d = bbox(&lean.regular, 'H').2 as f32 - bbox(&flat.regular, 'H').2 as f32;
    assert!((d - 123.4).abs() <= 2.5, "moved {d}");
    // italic is a further quarter on top
    let di = bbox(&lean.italic, 'H').2 as f32 - bbox(&lean.regular, 'H').2 as f32;
    assert!((di - 175.0).abs() <= 1.5);
}

#[test]
fn bake_regular_makes_only_the_regular_face_with_the_same_advances() {
    let pkg = sans();
    let v = AxisValues { weight: Some(650.0), width: Some(80.0), slant: Some(6.0) };
    let (one, all) = (bake_regular(&pkg, v).unwrap(), bake(&pkg, v).unwrap());
    let (a, b) = (ttf_parser::Face::parse(&one.face, 0).unwrap(), ttf_parser::Face::parse(&all.regular, 0).unwrap());
    for g in glyph_ids(&b) {
        assert_eq!(adv(&a, g), adv(&b, g));
    }
    assert_eq!(one.metrics, all.metrics);
    assert!(one.family.starts_with("Washi Text "));
}

#[test]
fn metrics_match_the_fixture_fonts_known_shapes() {
    // Test Sans: ascent 900 + descent 300 of 1000; space advance 520 + (32 % 5) * 40 = 600
    let m = bake(&sans(), AxisValues::default()).unwrap().metrics;
    assert!((m.line_em - 1.2).abs() < 1e-4);
    assert!((m.space_em - 0.6).abs() < 1e-4);
    assert!((m.descent_em - 0.18).abs() < 1e-4, "g, j, p, q, y go 180 below");
    assert_eq!(m.lean_em, 0.0, "every glyph starts right of its origin");
    // the Italic's top right corner (y = 700) goes 0.25 * 700 = 175 right of its box, whose right
    // edge is 60 inside the advance
    assert!((m.italic_overhang_em - 0.115).abs() < 1e-4, "{}", m.italic_overhang_em);
    assert!(m.avg_advance_em > 0.5 && m.avg_advance_em < 0.7);
    // narrower at width 75
    let narrow = bake(&sans(), AxisValues { width: Some(75.0), ..Default::default() }).unwrap().metrics;
    assert!(narrow.avg_advance_em < m.avg_advance_em * 0.85);
    assert!((narrow.space_em - 0.45).abs() < 1e-3);
}

#[test]
fn the_bundled_default_font_gives_todays_constants() {
    let bytes = std::fs::read(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../fonts/ShadowsIntoLight.ttf")).unwrap();
    // refused as an installed font (its name is bundled), usable as the default font's package
    assert!(read_package(&bytes).is_err());
    let pkg = package_from_font(bytes).unwrap();
    let set = bake(&pkg, AxisValues::default()).unwrap();
    assert!((set.metrics.space_em - 0.450195).abs() < 1e-4, "{}", set.metrics.space_em);
    assert!((set.metrics.line_em - 1.6064).abs() < 1e-4, "{}", set.metrics.line_em);
}
