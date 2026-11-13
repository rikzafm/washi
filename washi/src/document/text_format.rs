//! The stored text form of a [`Document`]: reading (`parse_line` and the readers it uses) and
//! writing (`write_block`) the Markdown subset described in the parent module, plus the reader of
//! the older format. Pure functions over [`Block`]; the document itself stays in the parent.

use super::{palette_index, palette_name, Align, Block, Kind, Paint, Style, IMAGE_PCT_MAX, IMAGE_PCT_MIN};

/// Characters that start a marker somewhere in the old format; a text starting with one was escaped.
const SPECIAL: [char; 7] = ['\\', '*', '~', '<', '#', '-', '!'];
/// Characters that start an inline marker (or an escape): escaped wherever they are in the text.
const INLINE: [char; 4] = ['\\', '*', '~', '<'];
/// Characters that start a block marker: escaped as the first character of a line.
const LINE_START: [char; 3] = ['#', '-', '!'];

/// What follows `- ` on the stored line of a check item.
const CHECK_MARKERS: [(&str, Kind); 2] = [("[ ] ", Kind::Check), ("[x] ", Kind::Checked)];

// ---- the format of older versions: markers around the whole text, only its first character escaped ----

fn unescape(text: &str) -> String {
    let mut chars = text.chars();
    if chars.next() == Some('\\') {
        if let Some(c) = chars.next() {
            if SPECIAL.contains(&c) {
                return text[1..].to_string();
            }
        }
    }
    text.to_string()
}

fn unwrap_inline(mut s: &str) -> (Style, String) {
    let mut style = Style::NONE;
    for (open, close, flag) in [("<u>", "</u>", Style::UNDERLINE), ("~~", "~~", Style::STRIKE), ("**", "**", Style::BOLD), ("*", "*", Style::ITALIC)] {
        if s.len() >= open.len() + close.len() && s.starts_with(open) && s.ends_with(close) {
            s = &s[open.len()..s.len() - close.len()];
            style = style.with(flag, true);
        }
    }
    (style, unescape(s))
}

// ---- the format: a marker wherever a style changes ----

/// The markers that change the style from `from` to `to`: those that end first (innermost
/// first), then those that begin (outermost first). Each flag at most once, so a run of `*` is
/// never ambiguous: `***` is bold and italic.
fn markers(out: &mut String, from: Style, to: Style) {
    for (flag, close) in [(Style::ITALIC, "*"), (Style::BOLD, "**"), (Style::STRIKE, "~~"), (Style::UNDERLINE, "</u>")] {
        if from.contains(flag) && !to.contains(flag) {
            out.push_str(close);
        }
    }
    for (flag, open) in [(Style::UNDERLINE, "<u>"), (Style::STRIKE, "~~"), (Style::BOLD, "**"), (Style::ITALIC, "*")] {
        if to.contains(flag) && !from.contains(flag) {
            out.push_str(open);
        }
    }
}

/// The color markers that change the paint from `from` to `to`: an ink or marker that changes
/// ends (`</c>`, `</m>`) and then begins (`<c=name>`, `<m=name>`).
fn paint_markers(out: &mut String, from: Paint, to: Paint) {
    for (a, b, tag) in [(from.ink, to.ink, 'c'), (from.marker, to.marker, 'm')] {
        if a != b {
            if a != 0 {
                out.push_str(&format!("</{tag}>"));
            }
            if b != 0 {
                out.push_str(&format!("<{tag}={}>", palette_name(b)));
            }
        }
    }
}

/// A block as one stored line: its kind's marker and then its text, or the whole picture line.
pub(super) fn write_block(b: &Block) -> String {
    let mut out = String::new();
    match b.kind {
        Kind::Title => out.push_str("# "),
        Kind::Heading => out.push_str("## "),
        Kind::ListItem => {
            out.push_str("- ");
            // a bullet whose text starts like a check marker must not be read as a check item
            if CHECK_MARKERS.iter().any(|(m, _)| b.text.starts_with(m) && write_inline(b).starts_with(m)) {
                out.push('\\');
            }
        }
        Kind::Check => out.push_str("- [ ] "),
        Kind::Checked => out.push_str("- [x] "),
        Kind::Body => {}
        Kind::Image => {
            out.push_str(&format!("![](img:{}", b.image_id.unwrap_or(0)));
            if b.image_pct != IMAGE_PCT_MAX || b.align != Align::Left {
                let a = match b.align {
                    Align::Left => 'l',
                    Align::Center => 'c',
                    Align::Right => 'r',
                };
                out.push_str(&format!(",{},{a}", b.image_pct.clamp(IMAGE_PCT_MIN, IMAGE_PCT_MAX)));
            }
            out.push(')');
            return out;
        }
    }
    out.push_str(&write_inline(b));
    out
}

/// A block's text with its styles, as stored.
pub(super) fn write_inline(b: &Block) -> String {
    let mut out = String::new();
    let (mut cur, mut cur_paint) = (Style::NONE, Paint::default());
    for (start, end, style, paint) in b.runs() {
        markers(&mut out, cur, style);
        paint_markers(&mut out, cur_paint, paint);
        for c in b.text[start..end].chars() {
            if INLINE.contains(&c) || (out.is_empty() && LINE_START.contains(&c)) {
                out.push('\\');
            }
            out.push(c);
        }
        cur = style;
        cur_paint = paint;
    }
    markers(&mut out, cur, Style::NONE);
    paint_markers(&mut out, cur_paint, Paint::default());
    out
}

/// Read what [`write_inline`] writes. `None` for anything it would not have written exactly (text
/// from an older version), which is then read the old way.
fn read_inline(s: &str) -> Option<(String, Vec<Style>, Vec<Paint>)> {
    let (mut text, mut marks, mut cur) = (String::new(), vec![], Style::NONE);
    let (mut paints, mut paint) = (vec![], Paint::default());
    // the stored line without the color tags of unknown names, which are dropped
    let mut clean = String::new();
    let mut rest = s;
    while let Some(c) = rest.chars().next() {
        let (flag, len, on) = if c == '\\' {
            let n = rest[1..].chars().next().filter(|n| INLINE.contains(n) || LINE_START.contains(n))?;
            text.push(n);
            marks.extend(std::iter::repeat_n(cur, n.len_utf8()));
            paints.extend(std::iter::repeat_n(paint, n.len_utf8()));
            clean.push_str(&rest[..1 + n.len_utf8()]);
            rest = &rest[1 + n.len_utf8()..];
            continue;
        } else if let Some((tag, len, to)) = color_tag(rest) {
            let field = if tag == 'c' { &mut paint.ink } else { &mut paint.marker };
            // a closing tag with nothing open and a tag of an unknown name are dropped
            if to.is_some() || (rest.starts_with("</") && *field != 0) {
                clean.push_str(&rest[..len]);
            } else if *field != 0 {
                // an unknown name switches the color off, as the writer would
                clean.push_str(&format!("</{tag}>"));
            }
            *field = to.unwrap_or(0);
            rest = &rest[len..];
            continue;
        } else if rest.starts_with("<u>") {
            (Style::UNDERLINE, 3, true)
        } else if rest.starts_with("</u>") {
            (Style::UNDERLINE, 4, false)
        } else if rest.starts_with("~~") {
            (Style::STRIKE, 2, !cur.contains(Style::STRIKE))
        } else if rest.starts_with("**") {
            (Style::BOLD, 2, !cur.contains(Style::BOLD))
        } else if c == '*' {
            (Style::ITALIC, 1, !cur.contains(Style::ITALIC))
        } else {
            text.push(c);
            marks.extend(std::iter::repeat_n(cur, c.len_utf8()));
            paints.extend(std::iter::repeat_n(paint, c.len_utf8()));
            clean.push(c);
            rest = &rest[c.len_utf8()..];
            continue;
        };
        cur = cur.with(flag, on);
        clean.push_str(&rest[..len]);
        rest = &rest[len..];
    }
    let b = Block { kind: Kind::Body, text, marks, paints, image_id: None, image_pct: IMAGE_PCT_MAX, align: Align::Left };
    (write_inline(&b) == clean).then_some((b.text, b.marks, b.paints))
}

/// A color tag at the start of `s`: which (`'c'` ink, `'m'` marker), its length, and the palette
/// index it switches to (`None` for a closing tag or an unknown name, both meaning default).
fn color_tag(s: &str) -> Option<(char, usize, Option<u8>)> {
    for tag in ['c', 'm'] {
        if s.starts_with(&format!("</{tag}>")) {
            return Some((tag, 4, None));
        }
        if let Some(r) = s.strip_prefix(&format!("<{tag}=")) {
            let end = r.find('>')?;
            let name = &r[..end];
            if name.contains('<') {
                return None;
            }
            return Some((tag, 4 + end, palette_index(name)));
        }
    }
    None
}

/// A block of `kind` from the stored rest of its line (after the kind's marker).
fn read_block(kind: Kind, rest: &str) -> Block {
    match read_inline(rest) {
        Some((text, marks, paints)) => Block { kind, text, marks, paints, image_id: None, image_pct: IMAGE_PCT_MAX, align: Align::Left },
        None => {
            let (style, text) = unwrap_inline(rest);
            Block::new(kind, &text).with_style(style)
        }
    }
}

/// `![](img:ID)`, or `![](img:ID,PCT,l|c|r)` for a picture that is resized or aligned.
fn image_line(line: &str) -> Option<Block> {
    let inner = line.strip_prefix("![](img:")?.strip_suffix(')')?;
    let mut parts = inner.split(',');
    let mut b = Block::image(parts.next()?.parse().ok()?);
    if let Some(pct) = parts.next().and_then(|p| p.parse::<u8>().ok()) {
        b.image_pct = pct.clamp(IMAGE_PCT_MIN, IMAGE_PCT_MAX);
    }
    b.align = match parts.next() {
        Some("c") => Align::Center,
        Some("r") => Align::Right,
        _ => Align::Left,
    };
    Some(b)
}

pub(super) fn parse_line(index: usize, line: &str) -> Block {
    if index == 0 {
        // The title: `# ` (or `## `, older notes) and the rest, or the whole line of a plain note.
        let rest = line.strip_prefix("# ").or_else(|| line.strip_prefix("## ")).unwrap_or(line);
        return read_block(Kind::Title, rest);
    }
    if let Some(b) = image_line(line) {
        return b;
    }
    let (kind, rest) = if let Some(r) = line.strip_prefix("## ").or_else(|| line.strip_prefix("# ")) {
        (Kind::Heading, r)
    } else if let Some(r) = line.strip_prefix("- ") {
        match CHECK_MARKERS.iter().find_map(|&(m, k)| r.strip_prefix(m).map(|rest| (k, rest))) {
            Some((kind, rest)) => (kind, rest),
            // the escaped form of a bullet that starts like a check marker
            None => (Kind::ListItem, r.strip_prefix('\\').filter(|s| CHECK_MARKERS.iter().any(|(m, _)| s.starts_with(m))).unwrap_or(r)),
        }
    } else {
        (Kind::Body, line)
    };
    read_block(kind, rest)
}
