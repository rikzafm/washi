//! Ink-mask IoU with alignment search.

/// A binary ink mask.
#[derive(Clone, Debug)]
pub struct Bitmap {
    pub w: usize,
    pub h: usize,
    pub on: Vec<bool>,
}

impl Bitmap {
    pub fn from_fn(w: usize, h: usize, f: impl Fn(usize, usize) -> bool) -> Self {
        let mut on = Vec::with_capacity(w * h);
        for y in 0..h {
            for x in 0..w {
                on.push(f(x, y));
            }
        }
        Bitmap { w, h, on }
    }

    pub fn count(&self) -> usize {
        self.on.iter().filter(|&&b| b).count()
    }

    pub fn get(&self, x: i64, y: i64) -> bool {
        x >= 0 && y >= 0 && (x as usize) < self.w && (y as usize) < self.h && self.on[y as usize * self.w + x as usize]
    }

    /// Bounding box [x0, y0, x1, y1) of the ink.
    pub fn bbox(&self) -> Option<(i64, i64, i64, i64)> {
        let (mut x0, mut y0, mut x1, mut y1) = (i64::MAX, i64::MAX, i64::MIN, i64::MIN);
        for y in 0..self.h {
            for x in 0..self.w {
                if self.on[y * self.w + x] {
                    x0 = x0.min(x as i64);
                    y0 = y0.min(y as i64);
                    x1 = x1.max(x as i64 + 1);
                    y1 = y1.max(y as i64 + 1);
                }
            }
        }
        (x1 > x0).then_some((x0, y0, x1, y1))
    }

    fn points(&self) -> Vec<(i64, i64)> {
        let mut v = vec![];
        for y in 0..self.h {
            for x in 0..self.w {
                if self.on[y * self.w + x] {
                    v.push((x as i64, y as i64));
                }
            }
        }
        v
    }
}

/// IoU of `a` and `b` after moving `b` by every offset within `radius` px of the position that
/// puts the centres of the two ink bounding boxes on top of each other. Returns the best IoU and
/// the offset (dx, dy) applied to `b`.
pub fn align_iou(a: &Bitmap, b: &Bitmap, radius: i64) -> (f64, (i64, i64)) {
    align_iou_ignoring(a, b, radius, &[])
}

/// Like [`align_iou`], but rows of `a` flagged in `ignore_rows` (e.g. an underline the app draws
/// itself) count for neither mask. The alignment still centres the ink bounding boxes.
pub fn align_iou_ignoring(a: &Bitmap, b: &Bitmap, radius: i64, ignore_rows: &[bool]) -> (f64, (i64, i64)) {
    let ignored = |y: i64| y >= 0 && ignore_rows.get(y as usize).copied().unwrap_or(false);
    let kept = Bitmap::from_fn(a.w, a.h, |x, y| a.on[y * a.w + x] && !ignored(y as i64));
    let (Some(ba), Some(bb)) = (kept.bbox(), b.bbox()) else { return (0.0, (0, 0)) };
    let (cax, cay) = ((ba.0 + ba.2) / 2, (ba.1 + ba.3) / 2);
    let (cbx, cby) = ((bb.0 + bb.2) / 2, (bb.1 + bb.3) / 2);
    let (base_x, base_y) = (cax - cbx, cay - cby);
    let na = a.points().iter().filter(|&&(_, y)| !ignored(y)).count() as f64;
    let pts = b.points();
    let mut best = (-1.0, (0, 0));
    for dy in base_y - radius..=base_y + radius {
        for dx in base_x - radius..=base_x + radius {
            let (mut inter, mut nb) = (0.0, 0.0);
            for &(x, y) in &pts {
                if ignored(y + dy) {
                    continue;
                }
                nb += 1.0;
                if a.get(x + dx, y + dy) {
                    inter += 1.0;
                }
            }
            let iou = inter / (na + nb - inter);
            if iou > best.0 {
                best = (iou, (dx, dy));
            }
        }
    }
    best
}
