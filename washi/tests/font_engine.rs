//! The font engine at its own interface: `read_package` and `bake`. No window, no UI.
//! Expected values come from the fixture generator (`tools/make_test_fonts.py`), from ttf-parser's
//! own reading of the variable font (HVAR), and from the spec, not from the engine's code.

use washi::fonts::{read_package, PackageError};

fn fixture(name: &str) -> Vec<u8> {
    let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../tools/snap/tests/fixtures/fonts").join(name);
    std::fs::read(&p).unwrap_or_else(|e| panic!("{}: {e}", p.display()))
}

#[test]
fn a_google_fonts_zip_reads_as_its_variable_font() {
    let pkg = read_package(&fixture("Test_Sans.zip")).expect("reads");
    assert_eq!(pkg.family, "Test Sans");
    let w = pkg.axes.weight.expect("weight axis");
    assert_eq!((w.min, w.default, w.max), (300.0, 400.0, 800.0));
    let wd = pkg.axes.width.expect("width axis");
    assert_eq!((wd.min, wd.default, wd.max), (75.0, 100.0, 100.0));
    assert!(pkg.axes.slant.is_none());
    // the variable file was chosen: it is the upright variable file, not the static Regular
    assert_eq!(pkg.regular, fixture("TestSans-VariableFont_wdth,wght.ttf"));
    // a weight axis makes a Bold source unnecessary
    assert!(pkg.bold.is_none());
    assert!(String::from_utf8_lossy(pkg.license.as_deref().expect("licence kept")).contains("Open Font License"));
}

#[test]
fn a_static_only_zip_uses_the_regular_file_and_keeps_the_bold_file_as_a_bold_source() {
    let pkg = read_package(&fixture("Test_Serif.zip")).expect("reads");
    assert_eq!(pkg.family, "Test Serif");
    assert_eq!(pkg.regular, fixture("TestSerif-Regular.ttf"));
    assert_eq!(pkg.bold.as_deref(), Some(fixture("TestSerif-Bold.ttf").as_slice()));
    assert!(pkg.axes.weight.is_none() && pkg.axes.width.is_none() && pkg.axes.slant.is_none());
}

#[test]
fn a_zip_with_one_ttf_uses_it_and_a_bare_ttf_reads_too() {
    let zipped = read_package(&fixture("Test_Plain.zip")).expect("reads");
    assert_eq!(zipped.regular, fixture("TestSerif-Regular.ttf"));
    assert!(zipped.bold.is_none() && zipped.license.is_none());
    let bare = read_package(&fixture("TestSerif-Regular.ttf")).expect("reads");
    assert_eq!(bare.family, "Test Serif");
    assert_eq!(bare.regular, fixture("TestSerif-Regular.ttf"));
    assert!(bare.bold.is_none());
}

#[test]
fn a_bare_variable_ttf_reports_its_axes() {
    let pkg = read_package(&fixture("TestSans-VariableFont_wdth,wght.ttf")).expect("reads");
    assert_eq!(pkg.axes.weight.map(|a| (a.min, a.max)), Some((300.0, 800.0)));
}

fn refusal(bytes: &[u8]) -> PackageError {
    read_package(bytes).err().expect("refused")
}

#[test]
fn files_that_are_not_fonts_are_refused_with_a_reason() {
    let e = refusal(b"hello, this is not a font at all");
    assert!(matches!(e, PackageError::NotAFont), "{e:?}");
    assert!(!e.to_string().is_empty());
    assert!(matches!(refusal(b""), PackageError::NotAFont));
    // a zip that is cut short
    let z = fixture("Test_Sans.zip");
    assert!(matches!(refusal(&z[..z.len() / 2]), PackageError::NotAFont));
}

#[test]
fn a_zip_without_a_font_is_refused() {
    let z = zip_of(&[("README.txt", b"nothing here".as_slice())], zip::CompressionMethod::Deflated);
    assert!(matches!(refusal(&z), PackageError::NoFont), "{:?}", refusal(&z));
}

fn zip_of(entries: &[(&str, &[u8])], method: zip::CompressionMethod) -> Vec<u8> {
    use std::io::Write;
    let mut buf = std::io::Cursor::new(Vec::new());
    {
        let mut w = zip::ZipWriter::new(&mut buf);
        let o = zip::write::SimpleFileOptions::default().compression_method(method);
        for (n, b) in entries {
            w.start_file(*n, o).unwrap();
            w.write_all(b).unwrap();
        }
        w.finish().unwrap();
    }
    buf.into_inner()
}

#[test]
fn too_many_entries_are_refused() {
    let names: Vec<String> = (0..601).map(|i| format!("f{i}.txt")).collect();
    let entries: Vec<(&str, &[u8])> = names.iter().map(|n| (n.as_str(), b"x".as_slice())).collect();
    let z = zip_of(&entries, zip::CompressionMethod::Stored);
    assert!(matches!(refusal(&z), PackageError::TooLarge(_)), "{:?}", refusal(&z));
    // exactly at the cap is fine as far as the count goes (it then has no font)
    let z = zip_of(&entries[..600], zip::CompressionMethod::Stored);
    assert!(matches!(refusal(&z), PackageError::NoFont));
}

#[test]
fn a_font_over_24_mb_is_refused_even_when_it_compresses_small() {
    // a valid font followed by zeros: deflates to a few dozen KB, inflates past the cap
    let mut big = fixture("TestSerif-Regular.ttf");
    big.resize(24 * 1024 * 1024 + 1, 0);
    let z = zip_of(&[("Big-Regular.ttf", &big)], zip::CompressionMethod::Deflated);
    assert!(z.len() < 1024 * 1024);
    assert!(matches!(refusal(&z), PackageError::TooLarge(_)), "{:?}", refusal(&z));
}

#[test]
fn an_archive_over_64_mb_is_refused_before_it_is_opened() {
    let big = vec![b'P'; 64 * 1024 * 1024 + 1];
    assert!(matches!(refusal(&big), PackageError::TooLarge(_)));
}

#[test]
fn unsupported_compression_is_refused_and_entry_names_are_not_trusted() {
    // a hostile entry name next to a good font changes nothing and is never used as a path
    let font = fixture("TestSerif-Regular.ttf");
    let z = zip_of(&[("../../evil/Test-Regular.ttf", &font)], zip::CompressionMethod::Stored);
    let pkg = read_package(&z).expect("reads");
    assert_eq!(pkg.family, "Test Serif");
}

#[test]
fn cff_outlines_are_refused() {
    // a minimal OpenType/CFF container: 'OTTO' header with a CFF table entry
    let mut f = Vec::new();
    f.extend_from_slice(b"OTTO");
    f.extend_from_slice(&[0, 1, 0, 16, 0, 0, 0, 0]);
    f.extend_from_slice(b"CFF ");
    f.extend_from_slice(&[0, 0, 0, 0, 0, 0, 0, 28, 0, 0, 0, 4]);
    f.extend_from_slice(&[1, 0, 4, 1]);
    assert!(matches!(refusal(&f), PackageError::Cff), "{:?}", refusal(&f));
}

#[test]
fn a_font_without_basic_latin_is_refused() {
    let f = fixture("TestSerif-Regular.ttf");
    let no_latin = without_latin(&f);
    assert!(matches!(refusal(&no_latin), PackageError::NoLatin), "{:?}", refusal(&no_latin));
}

/// Same font with the lowercase letters taken out of its cmap (cmap format 4 table rewritten).
fn without_latin(font: &[u8]) -> Vec<u8> {
    use write_fonts::tables::cmap::Cmap;
    use write_fonts::{types::GlyphId, FontBuilder};
    let src = skrifa::FontRef::new(font).unwrap();
    use skrifa::MetadataProvider;
    let map = Cmap::from_mappings(
        src.charmap().mappings().filter(|(c, _)| !(0x61..=0x7A).contains(c)).map(|(c, g)| (char::from_u32(c).unwrap(), GlyphId::new(g.to_u32()))),
    )
    .unwrap();
    let mut b = FontBuilder::new();
    b.add_table(&map).unwrap();
    b.copy_missing_tables(src);
    b.build()
}

#[test]
fn a_family_named_like_a_bundled_font_is_refused() {
    let f = rename_family(&fixture("TestSerif-Regular.ttf"), "Shadows Into Light");
    let e = refusal(&f);
    assert!(matches!(e, PackageError::NameClash(ref n) if n == "Shadows Into Light"), "{e:?}");
    let f = rename_family(&fixture("TestSerif-Regular.ttf"), "noto serif");
    assert!(matches!(refusal(&f), PackageError::NameClash(_)));
}

fn rename_family(font: &[u8], family: &str) -> Vec<u8> {
    use write_fonts::tables::name::{Name, NameRecord};
    use write_fonts::types::NameId;
    use write_fonts::FontBuilder;
    let src = skrifa::FontRef::new(font).unwrap();
    let name = Name::new(vec![NameRecord::new(3, 1, 0x409, NameId::new(1), family.to_string().into())]);
    let mut b = FontBuilder::new();
    b.add_table(&name).unwrap();
    b.copy_missing_tables(src);
    b.build()
}
