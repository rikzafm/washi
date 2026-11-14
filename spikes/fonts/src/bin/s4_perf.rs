//! S4: how long does baking a five-face set take? (release build)
//! usage: s4_perf <font.ttf> [wght] [wdth]
use skrifa::raw::TableProvider;
use washi_font_spikes::bake::*;

fn med(v: &mut [f64]) -> f64 {
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    v[v.len() / 2]
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let bytes = std::fs::read(&a[1]).unwrap();
    let f = skrifa::FontRef::new(&bytes).unwrap();
    println!("font {} : {} bytes, {} glyphs", a[1], bytes.len(), f.maxp().unwrap().num_glyphs());
    let axes = Axes { wght: a.get(2).and_then(|s| s.parse().ok()), wdth: a.get(3).and_then(|s| s.parse().ok()), slnt: None };
    for (name, src) in [("axis bold", BoldSource::Axis(300.0)), ("double strike", BoldSource::DoubleStrike)] {
        let mut all = vec![];
        let mut reg = vec![];
        for i in 0..11 {
            let t = std::time::Instant::now();
            let set = bake(&bytes, axes, src, &format!("Washi Text {i}"), &format!("Washi Caret {i}"));
            all.push(t.elapsed().as_secs_f64() * 1000.0);
            std::hint::black_box(&set);
            let t = std::time::Instant::now();
            let r = bake_regular(&bytes, axes, &format!("Washi Text {i}"));
            reg.push(t.elapsed().as_secs_f64() * 1000.0);
            std::hint::black_box(&r);
        }
        println!(
            "bold={name}: five faces median {:.1} ms (min {:.1}, max {:.1}); regular only median {:.1} ms",
            med(&mut all.clone()),
            all.iter().cloned().fold(f64::MAX, f64::min),
            all.iter().cloned().fold(0.0, f64::max),
            med(&mut reg)
        );
    }
}
