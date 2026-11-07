//! The `uidiff` command line as CI and the fidelity script use it: exit codes, report files,
//! and the baseline of known failures.
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use uidiff::report::{judge, parse_baseline};
use uidiff::Image;

fn design_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../docs/design")
}

fn hover_png() -> PathBuf {
    design_dir().join("reference/mockup-hover.png")
}

fn scratch(name: &str) -> PathBuf {
    let d = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(name);
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn uidiff(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_uidiff")).args(args).arg("--spec").arg(design_dir().join("spec.json")).output().unwrap()
}

fn s(p: &Path) -> &str {
    p.to_str().unwrap()
}

/// The hover mockup with its title moved `dy` px down.
fn title_moved(dy: i64, to: &Path) {
    let img = Image::load_png(&hover_png()).unwrap();
    let mut out = img.clone();
    for y in 160..235usize {
        for x in 130..440usize {
            out.set(x, y, [0xFF, 0xFF, 0xFD]);
        }
    }
    for y in 160..235usize {
        for x in 130..440usize {
            out.set(x, (y as i64 + dy) as usize, img.get(x, y));
        }
    }
    out.save_png(to).unwrap();
}

#[test]
fn compare_of_a_mockup_with_itself_exits_zero_and_writes_every_report_file() {
    let out = scratch("cli-self");
    let o = uidiff(&["--compare", s(&hover_png()), s(&hover_png()), "--mockup", "hover", "--out", s(&out)]);
    let text = String::from_utf8_lossy(&o.stdout);
    assert!(o.status.success(), "stdout:\n{text}\nstderr:\n{}", String::from_utf8_lossy(&o.stderr));
    assert!(text.contains("PASS") && !text.contains("FAIL"), "{text}");
    for f in ["heatmap.png", "overlay.png", "side-by-side.png", "flicker.gif", "report.md", "result.json"] {
        assert!(out.join(f).is_file(), "{f} written");
    }
    // the GIF really is a GIF, the PNGs really decode
    assert!(std::fs::read(out.join("flicker.gif")).unwrap().starts_with(b"GIF89a"));
    Image::load_png(&out.join("heatmap.png")).unwrap();
}

#[test]
fn compare_exits_non_zero_when_the_title_moved_three_pixels_and_writes_a_crop() {
    let out = scratch("cli-title");
    let snap = out.join("snap.png");
    title_moved(3, &snap);
    let o = uidiff(&["--compare", s(&hover_png()), s(&snap), "--mockup", "hover", "--out", s(&out.join("r"))]);
    assert!(!o.status.success());
    let text = String::from_utf8_lossy(&o.stdout);
    assert!(text.contains("FAIL") && text.contains("move up 3 px"), "{text}");
    assert!(out.join("r/crop-title.png").is_file(), "a 4x crop of the failing title");
    assert!(std::fs::read_to_string(out.join("r/report.md")).unwrap().contains("title"));
}

#[test]
fn a_baseline_allows_known_failures_but_not_new_ones_and_demands_that_fixed_ones_are_removed() {
    let out = scratch("cli-baseline");
    let snap = out.join("snap.png");
    title_moved(3, &snap);
    let base = out.join("baseline.txt");

    std::fs::write(&base, "# known\nhover:title\n").unwrap();
    let o = uidiff(&["--compare", s(&hover_png()), s(&snap), "--mockup", "hover", "--baseline", s(&base)]);
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stdout));
    assert!(String::from_utf8_lossy(&o.stdout).contains("FAIL (known)"));

    std::fs::write(&base, "hover:title\n").unwrap();
    let o = uidiff(&["--compare", s(&hover_png()), s(&snap), "--mockup", "hover", "--baseline", s(&base)]);
    // the whole-image row may or may not trip for a 3 px title move; the title must stay allowed
    let text = String::from_utf8_lossy(&o.stdout);
    assert!(!text.contains("REGRESSIONS in title"), "{text}");

    std::fs::write(&base, "hover:title\nhover:toolbar\n").unwrap();
    let o = uidiff(&["--compare", s(&hover_png()), s(&snap), "--mockup", "hover", "--baseline", s(&base)]);
    assert!(!o.status.success(), "toolbar passes but is listed");
    assert!(String::from_utf8_lossy(&o.stdout).contains("remove them from the baseline: hover:toolbar"));
}

#[test]
fn judge_sorts_groups_into_regressions_known_and_stale() {
    use uidiff::compare::Row;
    let row = |group: &str, pass: bool| Row { group: group.into(), item: "x".into(), expected: String::new(), actual: String::new(), delta: String::new(), tol: String::new(), pass, hint: None };
    let rows = vec![row("paper", true), row("title", false), row("title", true), row("body", false), row("toolbar", true)];
    let b = parse_baseline("# comment\nhover:body  # not done yet\nhover:toolbar\nother:paper\n\nhover:gone\n");
    let v = judge("hover", &rows, &b);
    assert_eq!(v.regressions, vec!["title"]);
    assert_eq!(v.known, vec!["body"]);
    assert_eq!(v.stale, vec!["toolbar", "gone"]);
    assert!(!v.ok());
}

#[test]
fn the_index_page_gathers_the_reports() {
    let out = scratch("cli-index");
    let o = uidiff(&["--compare", s(&hover_png()), s(&hover_png()), "--mockup", "hover", "--out", s(&out.join("hover"))]);
    assert!(o.status.success());
    let o = uidiff(&["--index", s(&out)]);
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    let html = std::fs::read_to_string(out.join("index.html")).unwrap();
    assert!(html.contains("hover/side-by-side.png") && html.contains("OK"));
}
