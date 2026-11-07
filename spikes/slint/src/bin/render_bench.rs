//! Headless render benchmark + edge-quality PNGs for the tilted paper (software renderer).
use slint::platform::WindowEvent;
use slint::{ComponentHandle, PhysicalSize};
use washi_spikes::{platform, Note};

fn main() {
    platform::install();
    let out = std::env::args().nth(1).unwrap_or_else(|| ".".into());
    let win = platform::next_window();
    let ui = Note::new().unwrap();
    ui.show().unwrap();
    for scale in [1.0f32, 1.5] {
        win.dispatch_event(WindowEvent::ScaleFactorChanged { scale_factor: scale });
        // (label, shadows, n, step_px, cached-image fallback)
        for (label, shadows, n, step, cached) in [("8x1px", true, 8, 1.0f32, false), ("4x2px", true, 4, 2.0, false), ("3x3px", true, 3, 3.0, false), ("none", false, 8, 1.0, false), ("cached-image", false, 8, 1.0, true)] {
            ui.set_shadows(shadows); ui.set_shadow_n(n); ui.set_shadow_step(step);
            ui.set_use_cached(false);
            if cached {
                // fallback: a pre-rendered shadow bitmap (default size, this scale) stretched to the window
                let (cw, ch) = ((431.0 * scale) as u32, (451.0 * scale) as u32);
                ui.set_shadows(true); ui.set_shadow_n(8); ui.set_shadow_step(1.0);
                win.set_size(PhysicalSize::new(cw, ch));
                let mut cb = vec![platform::Bgra::default(); (cw * ch) as usize];
                win.draw_if_needed(|r| { r.render(&mut cb, cw as usize); });
                let mut sb = slint::SharedPixelBuffer::<slint::Rgba8Pixel>::new(cw, ch);
                for (d, s) in sb.make_mut_slice().iter_mut().zip(cb.iter()) { *d = slint::Rgba8Pixel { r: s.r, g: s.g, b: s.b, a: s.a }; }
                ui.set_cached(slint::Image::from_rgba8_premultiplied(sb));
                ui.set_use_cached(true); ui.set_shadows(false);
            }
            // static frame at default size -> png
            let (lw, lh) = (431.0f32, 451.0f32);
            let (pw, ph) = ((lw * scale) as u32, (lh * scale) as u32);
            win.set_size(PhysicalSize::new(pw, ph));
            let mut buf = vec![platform::Bgra::default(); (pw * ph) as usize];
            win.draw_if_needed(|r| { r.render(&mut buf, pw as usize); });
            if label == "8x1px" { write_png(&format!("{out}/paper_{scale}.png"), &buf, pw, ph); }
            // live resize sweep: every frame a new size => full repaint
            let (mw, mh) = ((900.0 * scale) as u32, (900.0 * scale) as u32);
            let mut big = vec![platform::Bgra::default(); (mw * mh) as usize];
            let mut times = vec![];
            for f in 0..400u32 {
                let t = (f as f32 / 400.0 * std::f32::consts::TAU).sin() * 0.5 + 0.5; // 0..1..0
                let w = ((371.0 + 60.0 + t * 400.0) * scale) as u32;
                let h = ((391.0 + 60.0 + t * 350.0) * scale) as u32;
                win.set_size(PhysicalSize::new(w, h));
                let t0 = std::time::Instant::now();
                win.draw_if_needed(|r| { r.render(&mut big, w as usize); });
                times.push(t0.elapsed().as_secs_f64() * 1000.0);
            }
            times.sort_by(|a, b| a.partial_cmp(b).unwrap());
            let mean: f64 = times.iter().sum::<f64>() / times.len() as f64;
            println!(
                "scale={scale} shadow={label} live-resize frames={} mean={:.2}ms p50={:.2} p95={:.2} max={:.2}",
                times.len(), mean, times[times.len() / 2], times[times.len() * 95 / 100], times[times.len() - 1]
            );
        }
    }
}

fn write_png(path: &str, buf: &[platform::Bgra], w: u32, h: u32) {
    // composite over a busy diagonal-stripe + colour background so alpha edges are visible
    let mut rgb = Vec::with_capacity((w * h * 3) as usize);
    for y in 0..h {
        for x in 0..w {
            let p = buf[(y * w + x) as usize];
            let stripe = (((x + y) / 6) % 2) as u32;
            let bg = if stripe == 0 { [60u32, 130, 200] } else { [220, 170, 60] };
            let ia = 255 - p.a as u32;
            rgb.push((p.r as u32 + bg[0] * ia / 255) as u8);
            rgb.push((p.g as u32 + bg[1] * ia / 255) as u8);
            rgb.push((p.b as u32 + bg[2] * ia / 255) as u8);
        }
    }
    let f = std::fs::File::create(path).unwrap();
    let mut enc = png::Encoder::new(std::io::BufWriter::new(f), w, h);
    enc.set_color(png::ColorType::Rgb);
    enc.set_depth(png::BitDepth::Eight);
    enc.write_header().unwrap().write_image_data(&rgb).unwrap();
    // also dump alpha-only stats for the margin corner (must be 0 where nothing is drawn)
    let a00 = buf[0].a;
    println!("{path}: corner alpha={a00}");
}

