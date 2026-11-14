use snap::Harness;
use std::time::Duration;

fn ms(n: u64) -> Duration { Duration::from_millis(n) }

#[test]
fn probe() {
    let text = include_str!("washi_fixes_probe.md").replace("\r\n", "\n");
    let text = text.trim_end();
    for &(scroll_bottom) in &[false, true] {
        let mut h = Harness::new();
        h.start().unwrap();
        h.resize_note(0, 300, 300);
        h.set_note_text(0, text);
        let _ = h.render(0);
        h.advance(ms(5));
        let (w, hh) = h.note_size(0);
        if scroll_bottom { h.wheel(0, 220.0, 200.0, -1000.0); h.advance(ms(5)); }
        let off0 = h.scroll_offset(0);
        eprintln!("== bottom {scroll_bottom} size {w}x{hh} off {off0} view {} thumb {:?}", h.text_view_height(0), h.scrollbar_thumb(0));
        let mut ys: Vec<f32> = h.text_runs(0).iter().map(|r| r.2).collect();
        ys.sort_by(|a, b| a.partial_cmp(b).unwrap()); ys.dedup_by(|a, b| (*a - *b).abs() < 0.01);
        for &ly in &ys {
            let y = 42.0 + ly - off0 + 12.0;
            if y < 50.0 || y > hh as f32 - 90.0 { continue; }
            let mut line = String::new();
            for x in (300..=400).step_by(4) {
                // focus somewhere visible first: click start of the visible line
                if scroll_bottom { h.wheel(0, 220.0, 200.0, -1000.0); } else { h.wheel(0, 220.0, 200.0, 1000.0); }
                h.advance(ms(5));
                h.click(0, 100.0, y);
                h.advance(ms(100));
                let before = h.selection(0);
                h.click(0, x as f32, y);
                h.advance(ms(400));
                let s = h.selection(0);
                let f = h.text_focused_block(0);
                let ok = f >= 0 && f as usize == s.1 .0 && s.1 .0 == before.1 .0 && s != before;
                line.push(if ok { '.' } else if f < 0 { 'X' } else if s == before { '=' } else { '?' });
                if !ok { eprintln!("  y {y} x {x} before {before:?} after {s:?} focus {f} off {}", h.scroll_offset(0)); }
            }
            eprintln!("y {y:6.1} {line}");
        }
    }
}
