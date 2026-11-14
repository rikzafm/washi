//! The Washi app logo and icon: a rounded paper sheet with dark outline (#2B2F36),
//! paper background (#FFFFFD), pink washi tape (#F7BFCD) across the top-left,
//! light blue washi tape (#B1D9E5) across the bottom-right, and a hand-drawn 'W'
//! in the center (#2B2F36).
//!
//! No image decoder dependencies so `build.rs` includes this very file to embed the icon
//! in `washi.exe`, and the app uses it for its windows and taskbar buttons.

/// Sizes embedded in the .exe (plan section 7).
pub const SIZES: [u32; 5] = [16, 24, 32, 48, 256];

pub const PAPER: [u8; 4] = [0xFF, 0xFF, 0xFD, 0xFF];
pub const BORDER: [u8; 4] = [0x2B, 0x2F, 0x36, 0xFF];
pub const TAPE_PINK: [u8; 4] = [0xF7, 0xBF, 0xCD, 0xFF];
pub const TAPE_BLUE: [u8; 4] = [0xB1, 0xD9, 0xE5, 0xFF];
pub const INK: [u8; 4] = [0x2B, 0x2F, 0x36, 0xFF];

const ICON_16: &[u8] = include_bytes!("assets/icon_16.rgba");
const ICON_24: &[u8] = include_bytes!("assets/icon_24.rgba");
const ICON_32: &[u8] = include_bytes!("assets/icon_32.rgba");
const ICON_48: &[u8] = include_bytes!("assets/icon_48.rgba");
const ICON_64: &[u8] = include_bytes!("assets/icon_64.rgba");
const ICON_128: &[u8] = include_bytes!("assets/icon_128.rgba");
const ICON_256: &[u8] = include_bytes!("assets/icon_256.rgba");
const PNG_256: &[u8] = include_bytes!("assets/icon_256.png");

/// Bilinear interpolation downsampler for arbitrary target sizes.
fn resample(src: &[u8], src_w: u32, src_h: u32, dst_w: u32, dst_h: u32) -> Vec<u8> {
    if dst_w == 0 || dst_h == 0 {
        return Vec::new();
    }
    let mut out = vec![0u8; (dst_w * dst_h * 4) as usize];
    let x_ratio = (src_w as f32) / (dst_w as f32);
    let y_ratio = (src_h as f32) / (dst_h as f32);
    for dy in 0..dst_h {
        let sy = (dy as f32 + 0.5) * y_ratio - 0.5;
        let y0 = (sy.floor() as i32).clamp(0, src_h as i32 - 1) as u32;
        let y1 = (y0 + 1).min(src_h - 1);
        let fy = (sy - sy.floor()).clamp(0.0, 1.0);
        for dx in 0..dst_w {
            let sx = (dx as f32 + 0.5) * x_ratio - 0.5;
            let x0 = (sx.floor() as i32).clamp(0, src_w as i32 - 1) as u32;
            let x1 = (x0 + 1).min(src_w - 1);
            let fx = (sx - sx.floor()).clamp(0.0, 1.0);

            let w00 = (1.0 - fx) * (1.0 - fy);
            let w10 = fx * (1.0 - fy);
            let w01 = (1.0 - fx) * fy;
            let w11 = fx * fy;

            let i00 = ((y0 * src_w + x0) * 4) as usize;
            let i10 = ((y0 * src_w + x1) * 4) as usize;
            let i01 = ((y1 * src_w + x0) * 4) as usize;
            let i11 = ((y1 * src_w + x1) * 4) as usize;

            let dst_i = ((dy * dst_w + dx) * 4) as usize;
            for c in 0..4 {
                let v = src[i00 + c] as f32 * w00
                    + src[i10 + c] as f32 * w10
                    + src[i01 + c] as f32 * w01
                    + src[i11 + c] as f32 * w11;
                out[dst_i + c] = v.round().clamp(0.0, 255.0) as u8;
            }
        }
    }
    out
}

/// RGBA pixels, top row first, straight (not premultiplied) alpha.
pub fn rgba(size: u32) -> Vec<u8> {
    match size {
        16 => ICON_16.to_vec(),
        24 => ICON_24.to_vec(),
        32 => ICON_32.to_vec(),
        48 => ICON_48.to_vec(),
        64 => ICON_64.to_vec(),
        128 => ICON_128.to_vec(),
        256 => ICON_256.to_vec(),
        _ => resample(ICON_256, 256, 256, size, size),
    }
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

/// zlib stream using one fixed-Huffman block.
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
    if size == 256 {
        return PNG_256.to_vec();
    }
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
    resource(&mut out, 16, 1, &version_info()); // RT_VERSION: Task Manager's app name comes from here
    out
}

/// A VS_VERSIONINFO block naming the app "Washi Note" (Task Manager shows FileDescription).
fn version_info() -> Vec<u8> {
    fn u16s(s: &str) -> Vec<u8> {
        s.encode_utf16().chain(std::iter::once(0)).flat_map(|c| c.to_le_bytes()).collect()
    }
    fn pad(v: &mut Vec<u8>) {
        while v.len() % 4 != 0 {
            v.push(0);
        }
    }
    // One block: length, value length, type, key, padding, value, padding, children.
    fn block(key: &str, kind: u16, value_len: u16, value: &[u8], children: &[u8]) -> Vec<u8> {
        let mut b = vec![0u8; 6];
        b.extend(u16s(key));
        pad(&mut b);
        b.extend_from_slice(value);
        pad(&mut b);
        b.extend_from_slice(children);
        let len = b.len() as u16;
        b[0..2].copy_from_slice(&len.to_le_bytes());
        b[2..4].copy_from_slice(&value_len.to_le_bytes());
        b[4..6].copy_from_slice(&kind.to_le_bytes());
        b
    }
    let text = |key: &str, value: &str| {
        let v = u16s(value);
        let mut b = block(key, 1, (v.len() / 2) as u16, &v, &[]);
        pad(&mut b);
        b
    };
    let mut strings = Vec::new();
    for (k, v) in [
        ("CompanyName", "Washi"),
        ("FileDescription", "Washi Note"),
        ("FileVersion", "0.1.0.0"),
        ("InternalName", "washi"),
        ("OriginalFilename", "washi.exe"),
        ("ProductName", "Washi Note"),
        ("ProductVersion", "0.1.0.0"),
    ] {
        strings.extend(text(k, v));
    }
    let mut table = block("040904B0", 1, 0, &[], &strings);
    pad(&mut table);
    let mut sfi = block("StringFileInfo", 1, 0, &[], &table);
    pad(&mut sfi);
    let mut var = block("Translation", 0, 4, &[0x09, 0x04, 0xB0, 0x04], &[]);
    pad(&mut var);
    let mut vfi = block("VarFileInfo", 1, 0, &[], &var);
    pad(&mut vfi);
    let mut fixed = Vec::new();
    for v in [0xFEEF04BDu32, 0x0001_0000, 0x0000_0001, 0, 0x0000_0001, 0, 0x3F, 0, 4, 1, 0, 0, 0] {
        fixed.extend_from_slice(&v.to_le_bytes());
    }
    let mut children = sfi;
    children.extend(vfi);
    block("VS_VERSION_INFO", 0, 52, &fixed, &children)
}
