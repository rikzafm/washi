//! Minimal RGB image: PNG load plus the sampling helpers the measurements need.

pub type Rgb = [u8; 3];

#[derive(Clone)]
pub struct Image {
    pub w: usize,
    pub h: usize,
    px: Vec<Rgb>,
}

impl Image {
    pub fn new(w: usize, h: usize, fill: Rgb) -> Self {
        Image { w, h, px: vec![fill; w * h] }
    }

    pub fn get(&self, x: usize, y: usize) -> Rgb {
        self.px[y * self.w + x]
    }

    pub fn set(&mut self, x: usize, y: usize, c: Rgb) {
        self.px[y * self.w + x] = c;
    }

    pub fn lum(&self, x: usize, y: usize) -> f64 {
        lum(self.get(x, y))
    }

    /// Bilinear luminance at continuous coordinates (pixel i covers [i, i+1), center i+0.5).
    pub fn sample_lum(&self, fx: f64, fy: f64) -> f64 {
        let (gx, gy) = (fx - 0.5, fy - 0.5);
        let (x0, y0) = (gx.floor(), gy.floor());
        let (tx, ty) = (gx - x0, gy - y0);
        let at = |x: f64, y: f64| {
            let xi = (x.max(0.0) as usize).min(self.w - 1);
            let yi = (y.max(0.0) as usize).min(self.h - 1);
            self.lum(xi, yi)
        };
        let top = at(x0, y0) * (1.0 - tx) + at(x0 + 1.0, y0) * tx;
        let bot = at(x0, y0 + 1.0) * (1.0 - tx) + at(x0 + 1.0, y0 + 1.0) * tx;
        top * (1.0 - ty) + bot * ty
    }

    /// Copy of the rect [x0,y0,x1,y1) clipped to the image (at least 1x1).
    pub fn crop(&self, r: [i64; 4]) -> Image {
        let x0 = r[0].clamp(0, self.w as i64 - 1) as usize;
        let y0 = r[1].clamp(0, self.h as i64 - 1) as usize;
        let x1 = (r[2].clamp(0, self.w as i64) as usize).max(x0 + 1);
        let y1 = (r[3].clamp(0, self.h as i64) as usize).max(y0 + 1);
        let mut out = Image::new(x1 - x0, y1 - y0, [0, 0, 0]);
        for y in y0..y1 {
            for x in x0..x1 {
                out.set(x - x0, y - y0, self.get(x, y));
            }
        }
        out
    }

    /// Nearest-neighbour enlargement.
    pub fn zoom(&self, k: usize) -> Image {
        let mut out = Image::new(self.w * k, self.h * k, [0, 0, 0]);
        for y in 0..out.h {
            for x in 0..out.w {
                out.set(x, y, self.get(x / k, y / k));
            }
        }
        out
    }

    /// `self` and `other` next to each other with a gap.
    pub fn beside(&self, other: &Image, gap: usize, gap_color: Rgb) -> Image {
        let mut out = Image::new(self.w + gap + other.w, self.h.max(other.h), gap_color);
        for y in 0..self.h {
            for x in 0..self.w {
                out.set(x, y, self.get(x, y));
            }
        }
        for y in 0..other.h {
            for x in 0..other.w {
                out.set(self.w + gap + x, y, other.get(x, y));
            }
        }
        out
    }

    /// Row-major RGB bytes.
    pub fn rgb_bytes(&self) -> Vec<u8> {
        self.px.iter().flat_map(|c| c.iter().copied()).collect()
    }

    pub fn save_png(&self, path: &std::path::Path) -> Result<(), String> {
        let file = std::fs::File::create(path).map_err(|e| format!("{}: {e}", path.display()))?;
        let mut enc = png::Encoder::new(std::io::BufWriter::new(file), self.w as u32, self.h as u32);
        enc.set_color(png::ColorType::Rgb);
        enc.set_depth(png::BitDepth::Eight);
        let mut w = enc.write_header().map_err(|e| e.to_string())?;
        w.write_image_data(&self.rgb_bytes()).map_err(|e| e.to_string())
    }

    pub fn load_png(path: &std::path::Path) -> Result<Image, String> {
        let file = std::fs::File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
        let mut decoder = png::Decoder::new(file);
        decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
        let mut reader = decoder.read_info().map_err(|e| e.to_string())?;
        let mut buf = vec![0; reader.output_buffer_size()];
        let info = reader.next_frame(&mut buf).map_err(|e| e.to_string())?;
        let channels = match info.color_type {
            png::ColorType::Rgb => 3,
            png::ColorType::Rgba => 4,
            png::ColorType::Grayscale => 1,
            png::ColorType::GrayscaleAlpha => 2,
            png::ColorType::Indexed => return Err("indexed PNG not expanded".into()),
        };
        let (w, h) = (info.width as usize, info.height as usize);
        let mut img = Image::new(w, h, [0, 0, 0]);
        for y in 0..h {
            for x in 0..w {
                let i = (y * w + x) * channels;
                let c = if channels >= 3 { [buf[i], buf[i + 1], buf[i + 2]] } else { [buf[i]; 3] };
                img.set(x, y, c);
            }
        }
        Ok(img)
    }
}

pub fn lum(c: Rgb) -> f64 {
    0.299 * c[0] as f64 + 0.587 * c[1] as f64 + 0.114 * c[2] as f64
}

/// Largest per-channel difference between two colors.
pub fn max_diff(a: Rgb, b: Rgb) -> u8 {
    (0..3).map(|i| a[i].abs_diff(b[i])).max().unwrap()
}

/// Channel spread (max - min): near 0 for grays, large for tape colors.
pub fn saturation(c: Rgb) -> u8 {
    *c.iter().max().unwrap() - *c.iter().min().unwrap()
}

pub fn parse_hex(s: &str) -> Result<Rgb, String> {
    let h = s.trim_start_matches('#');
    if h.len() != 6 {
        return Err(format!("bad color {s}"));
    }
    let v = u32::from_str_radix(h, 16).map_err(|e| format!("bad color {s}: {e}"))?;
    Ok([(v >> 16) as u8, (v >> 8) as u8, v as u8])
}

pub fn to_hex(c: Rgb) -> String {
    format!("#{:02X}{:02X}{:02X}", c[0], c[1], c[2])
}

/// Round to 3 decimals so printed results are stable.
pub fn r3(v: f64) -> f64 {
    let r = (v * 1000.0).round() / 1000.0;
    if r == 0.0 {
        0.0
    } else {
        r
    }
}
