//! The Washi app icon, drawn from spec values: a white sheet of paper with a pink tape strip
//! across its top. No image files and no dependencies, so `build.rs` includes this very file
//! to embed the icon in `washi.exe`, and the app uses it for its windows and taskbar buttons.

/// Sizes embedded in the .exe (plan section 7).
pub const SIZES: [u32; 5] = [16, 24, 32, 48, 256];

pub const PAPER: [u8; 4] = [0xFF, 0xFF, 0xFF, 0xFF];
/// Thin outline so the white sheet stays visible on a light taskbar.
pub const EDGE: [u8; 4] = [0xD0, 0xD0, 0xD0, 0xFF];
/// `tape-pink` from the design tokens.
pub const TAPE: [u8; 4] = [0xF7, 0xBF, 0xCD, 0xFF];

/// Where things sit in a `size` x `size` icon: (paper left/top, paper right/bottom exclusive,
/// tape left, tape right exclusive, tape top, tape bottom exclusive).
pub fn layout(size: u32) -> (u32, u32, u32, u32, u32, u32) {
    let s = size as f32;
    let m = (s * 0.125).round() as u32;
    let tape_top = (s * 0.22).round() as u32;
    let tape_h = ((s * 0.2).round() as u32).max(2);
    (m, size - m, (s * 0.25).round() as u32, (s * 0.75).round() as u32, tape_top, tape_top + tape_h)
}

/// RGBA pixels, top row first, straight (not premultiplied) alpha.
pub fn rgba(size: u32) -> Vec<u8> {
    let (lo, hi, tl, tr, tt, tb) = layout(size);
    let edge = (size / 32).max(1);
    let mut out = vec![0u8; (size * size * 4) as usize];
    for y in 0..size {
        for x in 0..size {
            let px = if x < lo || x >= hi || y < lo || y >= hi {
                [0, 0, 0, 0]
            } else if x >= tl && x < tr && y >= tt && y < tb {
                TAPE
            } else if x < lo + edge || x >= hi - edge || y < lo + edge || y >= hi - edge {
                EDGE
            } else {
                PAPER
            };
            let i = ((y * size + x) * 4) as usize;
            out[i..i + 4].copy_from_slice(&px);
        }
    }
    out
}

// ---- encoding for the .exe resource (build time) ----

fn crc32(data: &[u8]) -> u32 {
    let mut crc = !0u32;
    for &b in data {
        crc ^= b as u32;
        for _ in 0..8 {
            crc = if crc & 1 != 0 { (crc >> 1) ^ 0xEDB8_8320 } else { crc >> 1 };
        }
    }
    !crc
}

fn adler32(data: &[u8]) -> u32 {
    let (mut a, mut b) = (1u32, 0u32);
    for &d in data {
        a = (a + d as u32) % 65521;
        b = (b + a) % 65521;
    }
    (b << 16) | a
}

struct Bits {
    out: Vec<u8>,
    acc: u32,
    n: u32,
}

impl Bits {
    fn put(&mut self, value: u32, count: u32) {
        self.acc |= value << self.n;
        self.n += count;
        while self.n >= 8 {
            self.out.push(self.acc as u8);
            self.acc >>= 8;
            self.n -= 8;
        }
    }
    /// Huffman codes go out most significant bit first.
    fn code(&mut self, code: u32, len: u32) {
        let mut rev = 0;
        for i in 0..len {
            rev |= ((code >> i) & 1) << (len - 1 - i);
        }
        self.put(rev, len);
    }
    fn literal(&mut self, v: u32) {
        match v {
            0..=143 => self.code(0x30 + v, 8),
            144..=255 => self.code(0x190 + v - 144, 9),
            256..=279 => self.code(v - 256, 7),
            _ => self.code(0xC0 + v - 280, 8),
        }
    }
}

/// zlib stream using one fixed-Huffman block. Matches only look back one pixel (4 bytes), which
/// is all a flat-coloured icon needs to shrink to a few KB.
fn zlib(data: &[u8]) -> Vec<u8> {
    const BASE: [u32; 29] = [3, 4, 5, 6, 7, 8, 9, 10, 11, 13, 15, 17, 19, 23, 27, 31, 35, 43, 51, 59, 67, 83, 99, 115, 131, 163, 195, 227, 258];
    const EXTRA: [u32; 29] = [0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 3, 4, 4, 4, 4, 5, 5, 5, 5, 0];
    let mut w = Bits { out: vec![0x78, 0x01], acc: 0, n: 0 };
    w.put(1, 1); // final block
    w.put(1, 2); // fixed Huffman
    let mut i = 0;
    while i < data.len() {
        let mut run = 0;
        if i >= 4 {
            while run < 258 && i + run < data.len() && data[i + run] == data[i + run - 4] {
                run += 1;
            }
        }
        if run >= 3 {
            let k = BASE.iter().rposition(|&b| b as usize <= run).unwrap();
            w.literal(257 + k as u32);
            w.put(run as u32 - BASE[k], EXTRA[k]);
            w.code(3, 5); // distance 4
            i += run;
        } else {
            w.literal(data[i] as u32);
            i += 1;
        }
    }
    w.literal(256);
    if w.n > 0 {
        let b = w.acc as u8;
        w.out.push(b);
    }
    w.out.extend_from_slice(&adler32(data).to_be_bytes());
    w.out
}

pub fn png(size: u32) -> Vec<u8> {
    let pixels = rgba(size);
    let mut raw = Vec::with_capacity(pixels.len() + size as usize);
    for row in pixels.chunks((size * 4) as usize) {
        raw.push(0); // filter: none
        raw.extend_from_slice(row);
    }
    let mut out = b"\x89PNG\r\n\x1a\n".to_vec();
    let mut chunk = |kind: &[u8; 4], body: &[u8]| {
        out.extend_from_slice(&(body.len() as u32).to_be_bytes());
        let mut c = kind.to_vec();
        c.extend_from_slice(body);
        out.extend_from_slice(&c);
        out.extend_from_slice(&crc32(&c).to_be_bytes());
    };
    let mut ihdr = Vec::new();
    ihdr.extend_from_slice(&size.to_be_bytes());
    ihdr.extend_from_slice(&size.to_be_bytes());
    ihdr.extend_from_slice(&[8, 6, 0, 0, 0]);
    chunk(b"IHDR", &ihdr);
    chunk(b"IDAT", &zlib(&raw));
    chunk(b"IEND", &[]);
    out
}

/// An icon image as stored inside a Windows icon resource: PNG for 256 px, a bottom-up 32-bit
/// bitmap (plus an empty AND mask) for the small sizes.
pub fn icon_image(size: u32) -> Vec<u8> {
    if size >= 256 {
        return png(size);
    }
    let mut out = Vec::new();
    for v in [40u32, size, size * 2] {
        out.extend_from_slice(&v.to_le_bytes());
    }
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&32u16.to_le_bytes());
    out.extend_from_slice(&[0u8; 24]);
    let pixels = rgba(size);
    for row in pixels.chunks((size * 4) as usize).rev() {
        for p in row.chunks(4) {
            out.extend_from_slice(&[p[2], p[1], p[0], p[3]]);
        }
    }
    let mask_row = (size.div_ceil(32) * 4) as usize;
    out.resize(out.len() + mask_row * size as usize, 0);
    out
}

/// A 32-bit COFF `.res` file holding every icon size and the icon group (resource id 1, which
/// Windows uses as the .exe's icon). The MSVC linker takes it as-is, so no `rc.exe` is needed.
pub fn res_file() -> Vec<u8> {
    fn resource(out: &mut Vec<u8>, kind: u16, id: u16, data: &[u8]) {
        out.extend_from_slice(&(data.len() as u32).to_le_bytes());
        out.extend_from_slice(&32u32.to_le_bytes());
        for v in [0xFFFFu16, kind, 0xFFFF, id] {
            out.extend_from_slice(&v.to_le_bytes());
        }
        out.extend_from_slice(&0u32.to_le_bytes()); // data version
        out.extend_from_slice(&0x0030u16.to_le_bytes()); // movable, pure
        out.extend_from_slice(&0u16.to_le_bytes()); // language neutral
        out.extend_from_slice(&[0u8; 8]); // version, characteristics
        out.extend_from_slice(data);
        while out.len() % 4 != 0 {
            out.push(0);
        }
    }
    let mut out = Vec::new();
    resource(&mut out, 0, 0, &[]); // mandatory empty first entry
    let mut group = Vec::new();
    group.extend_from_slice(&[0, 0, 1, 0]);
    group.extend_from_slice(&(SIZES.len() as u16).to_le_bytes());
    for (n, &size) in SIZES.iter().enumerate() {
        let image = icon_image(size);
        let id = n as u16 + 1;
        resource(&mut out, 3, id, &image);
        let dim = if size >= 256 { 0 } else { size as u8 };
        group.extend_from_slice(&[dim, dim, 0, 0]);
        group.extend_from_slice(&1u16.to_le_bytes());
        group.extend_from_slice(&32u16.to_le_bytes());
        group.extend_from_slice(&(image.len() as u32).to_le_bytes());
        group.extend_from_slice(&id.to_le_bytes());
    }
    resource(&mut out, 14, 1, &group);
    out
}
