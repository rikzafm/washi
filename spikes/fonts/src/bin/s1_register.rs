//! S1: runtime font registration through slint::fontique_011 under the software renderer.
use slint::fontique_011::{fontique, shared_collection};
use slint::SharedString;
use washi_font_spikes::*;

use fontique::{Blob, FontInfoOverride, FontStyle, FontWeight};

fn reg(bytes: Vec<u8>, family: &str, weight: Option<f32>, italic: bool) {
    let mut c = shared_collection();
    let r = c.register_fonts(
        Blob::new(std::sync::Arc::new(bytes)),
        Some(FontInfoOverride {
            family_name: Some(family),
            weight: weight.map(FontWeight::new),
            style: if italic { Some(FontStyle::Italic) } else { Some(FontStyle::Normal) },
            ..Default::default()
        }),
    );
    println!("  registered {family} w={weight:?} italic={italic}: {} families, {} infos", r.len(), r.iter().map(|x| x.1.len()).sum::<usize>());
}

fn main() {
    install_headless();
    let fonts = "C:/Windows/Fonts/";
    let seg = std::fs::read(format!("{fonts}segoeui.ttf")).unwrap();
    let segb = std::fs::read(format!("{fonts}segoeuib.ttf")).unwrap();
    let segi = std::fs::read(format!("{fonts}segoeuii.ttf")).unwrap();
    let bah = std::fs::read(format!("{fonts}bahnschrift.ttf")).unwrap();
    let text = "Hamburgefonstiv 123";

    let rig = Rig::new();
    let p = &rig.probe;

    // A. before registration: the family is unknown (fallback)
    p.set_fam(SharedString::from("Washi Text 1"));
    let before = rig.render();
    println!("A before registration: top ink {:?} hash {:x}", before.ink_x(TOP.0, TOP.1), before.hash(TOP.0, TOP.1));

    // B. register at runtime, in the already-open window, WITHOUT touching the property
    reg(seg.clone(), "Washi Text 1", Some(400.0), false);
    reg(segb.clone(), "Washi Text 1", Some(700.0), false);
    reg(segi.clone(), "Washi Text 1", Some(400.0), true);
    let after_noreset = rig.render();
    println!(
        "B registered, same live window, property untouched: top hash {:x} (changed: {})",
        after_noreset.hash(TOP.0, TOP.1),
        after_noreset.hash(TOP.0, TOP.1) != before.hash(TOP.0, TOP.1)
    );

    // C. weight / italic pick the right faces; compare to predicted advance widths
    p.set_fam(SharedString::from("Washi Text 1"));
    let mut sigs = Vec::new();
    for (name, w, it, file) in [("regular", 400, false, &seg), ("bold", 700, false, &segb), ("italic", 400, true, &segi)] {
        p.set_wt(w);
        p.set_ital(it);
        let s = rig.render();
        let (t0, t1) = s.ink_x(TOP.0, TOP.1).unwrap();
        let (b0, b1) = s.ink_x(BOTTOM.0, BOTTOM.1).unwrap();
        let want = advance_sum(file, text, 40.0);
        println!(
            "C {name:8} Text ink w={} (pred advance {:.1}) hash {:x} | TextInput ink w={} hash {:x}",
            t1 - t0, want, s.hash(TOP.0, TOP.1), b1 - b0, s.hash(BOTTOM.0, BOTTOM.1)
        );
        s.save_pgm(&format!("s1_{name}.pgm"));
        sigs.push((s.hash(TOP.0, TOP.1), s.hash(BOTTOM.0, BOTTOM.1)));
    }
    println!("C faces pairwise distinct (Text): {}", sigs[0].0 != sigs[1].0 && sigs[0].0 != sigs[2].0 && sigs[1].0 != sigs[2].0);
    println!("C faces pairwise distinct (TextInput): {}", sigs[0].1 != sigs[1].1 && sigs[0].1 != sigs[2].1 && sigs[1].1 != sigs[2].1);
    println!("C registered regular differs from pre-registration fallback: {}", sigs[0].0 != before.hash(TOP.0, TOP.1));

    // D. change the family on the live window: re-shapes
    p.set_wt(400);
    p.set_ital(false);
    reg(bah.clone(), "Washi Text 2", None, false);
    let r1 = rig.render();
    p.set_fam(SharedString::from("Washi Text 2"));
    let r2 = rig.render();
    let (a0, a1) = r2.ink_x(TOP.0, TOP.1).unwrap();
    println!(
        "D family change Text 1 -> Text 2: top hash changed {} ; ink w={} (pred advance for Bahnschrift default {:.1})",
        r1.hash(TOP.0, TOP.1) != r2.hash(TOP.0, TOP.1),
        a1 - a0,
        advance_sum(&bah, text, 40.0)
    );
    println!("D TextInput changed too: {}", r1.hash(BOTTOM.0, BOTTOM.1) != r2.hash(BOTTOM.0, BOTTOM.1));
    r2.save_pgm("s1_family2.pgm");

    // E. re-register a different font under an already-drawn family name, no new name: stale?
    reg(bah.clone(), "Washi Text 1", Some(400.0), false);
    p.set_fam(SharedString::from("Washi Text 1"));
    let e = rig.render();
    println!(
        "E same family name re-registered with other file: top hash {:x} vs regular-seg {:x} (identical to Segoe = stale cache or still-Segoe-first)",
        e.hash(TOP.0, TOP.1),
        sigs[0].0
    );

    // G. the honest "window already open" test: an unknown family on the system falls back to
    // Segoe UI (A above was a no-op for that reason), so register a font that is not the fallback.
    p.set_fam(SharedString::from("Washi Text 9"));
    let g0 = rig.render();
    reg(bah.clone(), "Washi Text 9", Some(400.0), false);
    let g1 = rig.render();
    println!(
        "G open window, family set before it exists, font registered later, property untouched: Text changed {}, TextInput changed {}",
        g1.hash(TOP.0, TOP.1) != g0.hash(TOP.0, TOP.1),
        g1.hash(BOTTOM.0, BOTTOM.1) != g0.hash(BOTTOM.0, BOTTOM.1)
    );
    p.set_fam(SharedString::from("Washi Text 9 "));
    p.set_fam(SharedString::from("Washi Text 9"));
    let g2 = rig.render();
    println!(
        "G after re-setting the property: equals Bahnschrift render {} (Text 2 hash {:x}, now {:x})",
        g2.hash(TOP.0, TOP.1) == r2.hash(TOP.0, TOP.1),
        r2.hash(TOP.0, TOP.1),
        g2.hash(TOP.0, TOP.1)
    );

    // F. unregister
    let mut c = shared_collection();
    let id = c.family_id("Washi Text 2");
    println!("F family_id(Washi Text 2) = {:?}", id.is_some());
    if let Some(id) = id {
        let ok = c.unregister_font(id, fontique::FontWidth::NORMAL, FontStyle::Normal, FontWeight::NORMAL);
        println!("F unregister_font ok={ok}");
        p.set_fam(SharedString::from("Washi Text 2"));
        let f = rig.render();
        println!("F after unregister: top hash {:x} (r2 {:x})", f.hash(TOP.0, TOP.1), r2.hash(TOP.0, TOP.1));
    }
}
