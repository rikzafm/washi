//! Connected components on a boolean mask.

/// The largest 8-connected component of `member` (w x h, row-major), as a mask. Ties go to the
/// component that starts first in raster order. None if the mask is empty.
pub fn largest_component(member: &[bool], w: usize, h: usize) -> Option<Vec<bool>> {
    let mut label = vec![0usize; w * h];
    let mut sizes = vec![0usize];
    for start in 0..w * h {
        if !member[start] || label[start] != 0 {
            continue;
        }
        let id = sizes.len();
        let mut stack = vec![start];
        label[start] = id;
        let mut n = 0;
        while let Some(p) = stack.pop() {
            n += 1;
            let (px, py) = ((p % w) as i64, (p / w) as i64);
            for dy in -1..=1i64 {
                for dx in -1..=1i64 {
                    let (nx, ny) = (px + dx, py + dy);
                    if nx < 0 || ny < 0 || nx >= w as i64 || ny >= h as i64 {
                        continue;
                    }
                    let q = ny as usize * w + nx as usize;
                    if member[q] && label[q] == 0 {
                        label[q] = id;
                        stack.push(q);
                    }
                }
            }
        }
        sizes.push(n);
    }
    let (best, _) = sizes.iter().enumerate().skip(1).max_by_key(|(i, n)| (**n, std::cmp::Reverse(*i)))?;
    Some(label.iter().map(|&l| l == best).collect())
}
