//! Serializes and parses lightweight HTML for the clipboard (`CF_HTML`).
//!
//! Restricts formatting to Washi's tool menu:
//! - Blocks: Title (h1), Heading (h2), ListItem (li), Body (p), Image (img).
//! - Inline marks: Bold (b), Italic (i), Underline (u), Strike (s).
//! - Paints: Ink color and Marker highlight via inline CSS.
//! - Images: Embedded PNG/JPEG data URIs (`data:image/...;base64,...`).

use crate::document::{floor_boundary, Block, Kind, Paint, Range, Style as Flags, PALETTE};
use crate::images::Rgba;
use base64::prelude::*;

#[derive(Debug, Clone, PartialEq)]
pub enum ParsedBlock {
    Title { text: String, marks: Vec<Flags>, paints: Vec<Paint> },
    Heading { text: String, marks: Vec<Flags>, paints: Vec<Paint> },
    Body { text: String, marks: Vec<Flags>, paints: Vec<Paint> },
    ListItem { text: String, marks: Vec<Flags>, paints: Vec<Paint> },
    Image { rgba: Rgba },
}

impl ParsedBlock {
    pub fn is_empty(&self) -> bool {
        match self {
            ParsedBlock::Title { text, .. }
            | ParsedBlock::Heading { text, .. }
            | ParsedBlock::Body { text, .. }
            | ParsedBlock::ListItem { text, .. } => text.is_empty(),
            ParsedBlock::Image { .. } => false,
        }
    }
}

fn escape_html(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            _ => out.push(c),
        }
    }
    out
}

/// Convert a range of document blocks into plain text and an HTML fragment.
pub fn to_html_and_text(
    blocks: &[Block],
    range: Range,
    get_rgba: &dyn Fn(&Block) -> Option<Rgba>,
) -> (String, String) {
    let (s, e) = range.ordered();
    let from_block = s.block.min(blocks.len().saturating_sub(1));
    let to_block = e.block.min(blocks.len().saturating_sub(1));

    let mut plain_lines = Vec::new();
    let mut html = String::new();
    let mut in_list = false;

    for i in from_block..=to_block {
        let b = &blocks[i];
        if b.kind == Kind::Image {
            if in_list {
                html.push_str("</ul>\n");
                in_list = false;
            }
            if let Some(rgba) = get_rgba(b) {
                if let Ok(png) = crate::images::encode_png(&rgba) {
                    let b64 = BASE64_STANDARD.encode(&png);
                    html.push_str(&format!(
                        r#"<p><img src="data:image/png;base64,{b64}" width="{}" height="{}" /></p>"#,
                        rgba.width, rgba.height
                    ));
                    html.push('\n');
                }
            }
            continue;
        }

        let text = &b.text;
        let from_offset = if i == s.block { floor_boundary(text, s.offset) } else { 0 };
        let to_offset = if i == e.block { floor_boundary(text, e.offset) } else { text.len() };
        let slice = &text[from_offset..to_offset.max(from_offset)];
        plain_lines.push(slice.to_string());

        if b.kind == Kind::ListItem {
            if !in_list {
                html.push_str("<ul>\n");
                in_list = true;
            }
        } else if in_list {
            html.push_str("</ul>\n");
            in_list = false;
        }

        let tag = match b.kind {
            Kind::Title => "h1",
            Kind::Heading => "h2",
            Kind::ListItem => "li",
            Kind::Body => "p",
            Kind::Image => unreachable!(),
        };

        html.push('<');
        html.push_str(tag);
        html.push('>');

        if from_offset >= to_offset {
            html.push_str("<br>");
        } else {
            // Render styled text inside block
            let mut char_idx = from_offset;
            while char_idx < to_offset {
                let next_boundary = text[char_idx..].chars().next().map_or(to_offset, |c| char_idx + c.len_utf8());
                let style = b.marks.get(char_idx).copied().unwrap_or(Flags::NONE);
                let paint = b.paints.get(char_idx).copied().unwrap_or_default();

                // Group identical style & paint runs
                let mut run_end = next_boundary;
                while run_end < to_offset {
                    let st = b.marks.get(run_end).copied().unwrap_or(Flags::NONE);
                    let pt = b.paints.get(run_end).copied().unwrap_or_default();
                    if st != style || pt != paint {
                        break;
                    }
                    let len = text[run_end..].chars().next().map_or(1, |c| c.len_utf8());
                    run_end += len;
                }

                let run_text = escape_html(&text[char_idx..run_end]);

                // Open inline tags
                let mut close_tags = Vec::new();
                if paint.ink != 0 {
                    let hex = PALETTE[paint.ink.min(PALETTE.len() as u8 - 1) as usize].ink;
                    html.push_str(&format!(r#"<span style="color:#{:06X}">"#, hex));
                    close_tags.push("</span>");
                }
                if paint.marker != 0 {
                    if let Some(hex) = PALETTE[paint.marker.min(PALETTE.len() as u8 - 1) as usize].marker {
                        html.push_str(&format!(r#"<span style="background-color:#{:06X}">"#, hex));
                        close_tags.push("</span>");
                    }
                }
                if style.contains(Flags::BOLD) {
                    html.push_str("<b>");
                    close_tags.push("</b>");
                }
                if style.contains(Flags::ITALIC) {
                    html.push_str("<i>");
                    close_tags.push("</i>");
                }
                if style.contains(Flags::UNDERLINE) {
                    html.push_str("<u>");
                    close_tags.push("</u>");
                }
                if style.contains(Flags::STRIKE) {
                    html.push_str("<s>");
                    close_tags.push("</s>");
                }

                html.push_str(&run_text);

                for c in close_tags.into_iter().rev() {
                    html.push_str(c);
                }

                char_idx = run_end;
            }
        }

        html.push_str("</");
        html.push_str(tag);
        html.push_str(">\n");
    }

    if in_list {
        html.push_str("</ul>\n");
    }

    (plain_lines.join("\n"), html)
}

fn hex_color(val: &str) -> Option<[u8; 3]> {
    let s = val.trim().trim_start_matches('#');
    if s.len() == 6 {
        let r = u8::from_str_radix(&s[0..2], 16).ok()?;
        let g = u8::from_str_radix(&s[2..4], 16).ok()?;
        let b = u8::from_str_radix(&s[4..6], 16).ok()?;
        Some([r, g, b])
    } else if s.len() == 3 {
        let r = u8::from_str_radix(&s[0..1], 16).ok()? * 17;
        let g = u8::from_str_radix(&s[1..2], 16).ok()? * 17;
        let b = u8::from_str_radix(&s[2..3], 16).ok()? * 17;
        Some([r, g, b])
    } else {
        None
    }
}

fn rgb_func(val: &str) -> Option<[u8; 3]> {
    let s = val.trim();
    let inner = s.strip_prefix("rgb(")?.strip_suffix(')')?;
    let parts: Vec<&str> = inner.split(',').collect();
    if parts.len() >= 3 {
        let r = parts[0].trim().parse::<u8>().ok()?;
        let g = parts[1].trim().parse::<u8>().ok()?;
        let b = parts[2].trim().parse::<u8>().ok()?;
        Some([r, g, b])
    } else {
        None
    }
}

fn parse_color(val: &str) -> Option<[u8; 3]> {
    hex_color(val).or_else(|| rgb_func(val))
}

fn closest_ink(rgb: [u8; 3]) -> u8 {
    let mut best = (0u8, i64::MAX);
    for (i, swatch) in PALETTE.iter().enumerate() {
        let r = ((swatch.ink >> 16) & 0xFF) as i64;
        let g = ((swatch.ink >> 8) & 0xFF) as i64;
        let b = (swatch.ink & 0xFF) as i64;
        let dist = (r - rgb[0] as i64).pow(2) + (g - rgb[1] as i64).pow(2) + (b - rgb[2] as i64).pow(2);
        if dist < best.1 {
            best = (i as u8, dist);
        }
    }
    best.0
}

fn closest_marker(rgb: [u8; 3]) -> u8 {
    let mut best = (0u8, i64::MAX);
    for (i, swatch) in PALETTE.iter().enumerate() {
        if let Some(m) = swatch.marker {
            let r = ((m >> 16) & 0xFF) as i64;
            let g = ((m >> 8) & 0xFF) as i64;
            let b = (m & 0xFF) as i64;
            let dist = (r - rgb[0] as i64).pow(2) + (g - rgb[1] as i64).pow(2) + (b - rgb[2] as i64).pow(2);
            if dist < best.1 {
                best = (i as u8, dist);
            }
        }
    }
    best.0
}

/// Parse HTML fragment into Washi parsed blocks.
pub fn from_html(html: &str) -> Vec<ParsedBlock> {
    let mut blocks: Vec<ParsedBlock> = Vec::new();
    let mut cur_text = String::new();
    let mut cur_marks = Vec::new();
    let mut cur_paints = Vec::new();

    let mut bold = 0usize;
    let mut italic = 0usize;
    let mut underline = 0usize;
    let mut strike = 0usize;
    let mut style_stack: Vec<(Option<u8>, Option<u8>)> = Vec::new();

    #[derive(Clone, Copy)]
    struct BlockFrame {
        kind: Kind,
        children_emitted: usize,
    }
    let mut block_stack: Vec<BlockFrame> = Vec::new();
    let mut last_was_br = false;

    let flush_text = |blocks: &mut Vec<ParsedBlock>,
                      block_stack: &mut [BlockFrame],
                      cur_text: &mut String,
                      cur_marks: &mut Vec<Flags>,
                      cur_paints: &mut Vec<Paint>,
                      kind: Kind| {
        if !cur_text.is_empty() {
            let text = std::mem::take(cur_text);
            let marks = std::mem::take(cur_marks);
            let paints = std::mem::take(cur_paints);
            match kind {
                Kind::Title => blocks.push(ParsedBlock::Title { text, marks, paints }),
                Kind::Heading => blocks.push(ParsedBlock::Heading { text, marks, paints }),
                Kind::ListItem => blocks.push(ParsedBlock::ListItem { text, marks, paints }),
                Kind::Body | Kind::Image => blocks.push(ParsedBlock::Body { text, marks, paints }),
            }
            if let Some(f) = block_stack.last_mut() {
                f.children_emitted += 1;
            }
        }
    };

    let push_empty_block = |blocks: &mut Vec<ParsedBlock>,
                            block_stack: &mut [BlockFrame],
                            kind: Kind| {
        match kind {
            Kind::Title => blocks.push(ParsedBlock::Title { text: String::new(), marks: vec![], paints: vec![] }),
            Kind::Heading => blocks.push(ParsedBlock::Heading { text: String::new(), marks: vec![], paints: vec![] }),
            Kind::ListItem => blocks.push(ParsedBlock::ListItem { text: String::new(), marks: vec![], paints: vec![] }),
            Kind::Body | Kind::Image => blocks.push(ParsedBlock::Body { text: String::new(), marks: vec![], paints: vec![] }),
        }
        if let Some(f) = block_stack.last_mut() {
            f.children_emitted += 1;
        }
    };

    let mut i = 0;
    let bytes = html.as_bytes();
    while i < bytes.len() {
        if bytes[i] == b'<' {
            let tag_start = i;
            if let Some(tag_end) = bytes[tag_start..].iter().position(|&b| b == b'>').map(|p| tag_start + p) {
                let tag_str = &html[tag_start + 1..tag_end].trim();
                let is_close = tag_str.starts_with('/');
                let clean_str = tag_str.trim_matches(|c: char| c == '/' || c.is_whitespace());
                let tag_name = clean_str.split_whitespace().next().unwrap_or("").trim_end_matches('/').to_lowercase();

                // Skip invisible content in <head>, <style>, <script>
                if !is_close && (tag_name == "style" || tag_name == "script" || tag_name == "head") {
                    let close_pattern = format!("</{}", tag_name);
                    if let Some(pos) = html[tag_end + 1..].to_lowercase().find(&close_pattern) {
                        if let Some(close_gt) = html[tag_end + 1 + pos..].find('>') {
                            i = tag_end + 1 + pos + close_gt + 1;
                            continue;
                        }
                    }
                }

                match tag_name.as_str() {
                    "b" | "strong" => {
                        if is_close { bold = bold.saturating_sub(1); } else { bold += 1; }
                    }
                    "i" | "em" => {
                        if is_close { italic = italic.saturating_sub(1); } else { italic += 1; }
                    }
                    "u" | "ins" => {
                        if is_close { underline = underline.saturating_sub(1); } else { underline += 1; }
                    }
                    "s" | "strike" | "del" => {
                        if is_close { strike = strike.saturating_sub(1); } else { strike += 1; }
                    }
                    "span" => {
                        if is_close {
                            style_stack.pop();
                        } else {
                            let mut ink = None;
                            let mut marker = None;
                            // Search style="..."
                            if let Some(idx) = tag_str.find("style=") {
                                let after = &tag_str[idx + 6..];
                                let quote = after.chars().next().unwrap_or('"');
                                let style_val = after.strip_prefix(quote).and_then(|s| s.split(quote).next()).unwrap_or("");
                                for decl in style_val.split(';') {
                                    let mut kv = decl.split(':');
                                    let prop = kv.next().unwrap_or("").trim().to_lowercase();
                                    let val = kv.next().unwrap_or("").trim();
                                    if prop == "color" {
                                        if let Some(rgb) = parse_color(val) {
                                            ink = Some(closest_ink(rgb));
                                        }
                                    } else if prop == "background-color" || prop == "background" {
                                        if let Some(rgb) = parse_color(val) {
                                            marker = Some(closest_marker(rgb));
                                        }
                                    }
                                }
                            }
                            style_stack.push((ink, marker));
                        }
                    }
                    "mark" => {
                        if is_close {
                            style_stack.pop();
                        } else {
                            style_stack.push((None, Some(4))); // default yellow marker
                        }
                    }
                    "h1" | "h2" | "h3" | "h4" | "h5" | "h6" | "li" | "p" | "div" => {
                        let kind = match tag_name.as_str() {
                            "h1" => Kind::Title,
                            "h2" | "h3" | "h4" | "h5" | "h6" => Kind::Heading,
                            "li" => Kind::ListItem,
                            _ => Kind::Body,
                        };

                        if !is_close {
                            if cur_text.trim().is_empty() {
                                cur_text.clear();
                                cur_marks.clear();
                                cur_paints.clear();
                            } else {
                                let prev_kind = block_stack.last().map_or(Kind::Body, |f| f.kind);
                                flush_text(&mut blocks, &mut block_stack, &mut cur_text, &mut cur_marks, &mut cur_paints, prev_kind);
                            }
                            block_stack.push(BlockFrame { kind, children_emitted: 0 });
                        } else {
                            if cur_text.trim().is_empty() {
                                cur_text.clear();
                                cur_marks.clear();
                                cur_paints.clear();
                            }
                            if let Some(frame) = block_stack.pop() {
                                if !cur_text.is_empty() {
                                    flush_text(&mut blocks, &mut block_stack, &mut cur_text, &mut cur_marks, &mut cur_paints, frame.kind);
                                    if let Some(parent) = block_stack.last_mut() {
                                        parent.children_emitted += frame.children_emitted;
                                    }
                                } else if frame.children_emitted == 0 {
                                    push_empty_block(&mut blocks, &mut block_stack, frame.kind);
                                } else if let Some(parent) = block_stack.last_mut() {
                                    parent.children_emitted += frame.children_emitted;
                                }
                            }
                        }
                        last_was_br = false;
                    }
                    "br" => {
                        let kind = block_stack.last().map_or(Kind::Body, |f| f.kind);
                        if !cur_text.is_empty() {
                            flush_text(&mut blocks, &mut block_stack, &mut cur_text, &mut cur_marks, &mut cur_paints, kind);
                            last_was_br = true;
                        } else if last_was_br {
                            push_empty_block(&mut blocks, &mut block_stack, kind);
                            last_was_br = true;
                        } else {
                            last_was_br = true;
                        }
                    }
                    "img" => {
                        // Extract src attribute
                        if let Some(src_idx) = tag_str.find("src=") {
                            let after = &tag_str[src_idx + 4..];
                            let quote = after.chars().next().unwrap_or('"');
                            let src_val = after.strip_prefix(quote).and_then(|s| s.split(quote).next()).unwrap_or("");
                            if let Some(b64) = src_val.strip_prefix("data:image/").and_then(|s| s.split(";base64,").nth(1)) {
                                if let Ok(data) = BASE64_STANDARD.decode(b64.trim()) {
                                    if let Ok(rgba) = crate::images::decode(&data) {
                                        let kind = block_stack.last().map_or(Kind::Body, |f| f.kind);
                                        flush_text(&mut blocks, &mut block_stack, &mut cur_text, &mut cur_marks, &mut cur_paints, kind);
                                        blocks.push(ParsedBlock::Image { rgba });
                                        if let Some(f) = block_stack.last_mut() {
                                            f.children_emitted += 1;
                                        }
                                        last_was_br = false;
                                    }
                                }
                            }
                        }
                    }
                    _ => {}
                }

                i = tag_end + 1;
                continue;
            }
        }

        // Handle text character or entity
        if bytes[i] == b'&' {
            if let Some(semi) = bytes[i..].iter().position(|&b| b == b';').map(|p| i + p) {
                let entity = &html[i..=semi];
                let decoded = match entity {
                    "&amp;" => Some('&'),
                    "&lt;" => Some('<'),
                    "&gt;" => Some('>'),
                    "&quot;" => Some('"'),
                    "&#39;" | "&apos;" => Some('\''),
                    "&nbsp;" => Some(' '),
                    _ => None,
                };
                if let Some(c) = decoded {
                    let mut flags = Flags::NONE;
                    if bold > 0 { flags = flags.with(Flags::BOLD, true); }
                    if italic > 0 { flags = flags.with(Flags::ITALIC, true); }
                    if underline > 0 { flags = flags.with(Flags::UNDERLINE, true); }
                    if strike > 0 { flags = flags.with(Flags::STRIKE, true); }

                    let mut paint = Paint::default();
                    for &(ref ink, ref marker) in &style_stack {
                        if let Some(m) = marker { paint.marker = *m; }
                        if let Some(k) = ink { paint.ink = *k; }
                    }

                    cur_text.push(c);
                    for _ in 0..c.len_utf8() {
                        cur_marks.push(flags);
                        cur_paints.push(paint);
                    }
                    last_was_br = false;
                    i = semi + 1;
                    continue;
                }
            }
        }

        // Normal UTF-8 char
        let c = html[i..].chars().next().unwrap_or(' ');
        if c != '\r' && c != '\n' {
            let mut flags = Flags::NONE;
            if bold > 0 { flags = flags.with(Flags::BOLD, true); }
            if italic > 0 { flags = flags.with(Flags::ITALIC, true); }
            if underline > 0 { flags = flags.with(Flags::UNDERLINE, true); }
            if strike > 0 { flags = flags.with(Flags::STRIKE, true); }

            let mut paint = Paint::default();
            for &(ref ink, ref marker) in &style_stack {
                if let Some(m) = marker { paint.marker = *m; }
                if let Some(k) = ink { paint.ink = *k; }
            }

            cur_text.push(c);
            for _ in 0..c.len_utf8() {
                cur_marks.push(flags);
                cur_paints.push(paint);
            }
            last_was_br = false;
        }

        i += c.len_utf8();
    }

    if cur_text.trim().is_empty() {
        cur_text.clear();
        cur_marks.clear();
        cur_paints.clear();
    }
    if !cur_text.is_empty() {
        let kind = block_stack.last().map_or(Kind::Body, |f| f.kind);
        flush_text(&mut blocks, &mut block_stack, &mut cur_text, &mut cur_marks, &mut cur_paints, kind);
    }
    blocks
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::{Align, Caret};

    #[test]
    fn to_html_and_text_serializes_formatting_and_headings() {
        let mut b1 = Block::new(Kind::Title, "Note Title");
        b1.marks = vec![Flags::BOLD; 10];
        let mut b2 = Block::new(Kind::Body, "Bold and blue");
        let mut marks = vec![Flags::NONE; 13];
        marks[0..4].fill(Flags::BOLD);
        b2.marks = marks;
        let mut paints = vec![Paint::default(); 13];
        paints[9..13].fill(Paint { ink: 6, marker: 0 }); // blue
        b2.paints = paints;

        let blocks = vec![b1, b2];
        let range = Range {
            anchor: Caret { block: 0, offset: 0 },
            focus: Caret { block: 1, offset: 13 },
        };
        let (plain, html) = to_html_and_text(&blocks, range, &|_| None);
        assert_eq!(plain, "Note Title\nBold and blue");
        assert!(html.contains("<h1><b>Note Title</b></h1>"));
        assert!(html.contains("<b>Bold</b> and <span style=\"color:#3A6D96\">blue</span>"));
    }

    #[test]
    fn from_html_parses_formatting_and_colors() {
        let html = "<h1><b>Title</b></h1><p><b>Bold</b> and <i>italic</i> and <u>underline</u> and <s>strike</s> and <span style=\"color:#3A6D96\">blue</span></p>";
        let parsed = from_html(html);
        assert_eq!(parsed.len(), 2);
        match &parsed[0] {
            ParsedBlock::Title { text, marks, .. } => {
                assert_eq!(text, "Title");
                assert!(marks.iter().all(|m| m.contains(Flags::BOLD)));
            }
            _ => panic!("expected Title"),
        }
        match &parsed[1] {
            ParsedBlock::Body { text, marks, paints } => {
                assert_eq!(text, "Bold and italic and underline and strike and blue");
                assert!(marks[0].contains(Flags::BOLD));
                assert_eq!(paints[text.len() - 2], Paint { ink: 6, marker: 0 }); // blue
            }
            _ => panic!("expected Body"),
        }
    }

    #[test]
    fn image_data_uri_roundtrips_through_html() {
        let rgba = Rgba {
            width: 2,
            height: 2,
            data: vec![
                200, 30, 60, 255, 0, 0, 0, 255,
                0, 255, 0, 255,   255, 255, 255, 255,
            ],
        };
        let block = Block {
            kind: Kind::Image,
            text: String::new(),
            marks: vec![],
            paints: vec![],
            image_id: Some(1),
            image_pct: 100,
            align: Align::Left,
        };
        let blocks = vec![block];
        let range = Range { anchor: Caret { block: 0, offset: 0 }, focus: Caret { block: 0, offset: 0 } };
        let (_, html) = to_html_and_text(&blocks, range, &|_| Some(rgba.clone()));
        assert!(html.contains("<img src=\"data:image/png;base64,"));

        let parsed = from_html(&html);
        assert_eq!(parsed.len(), 1);
        match &parsed[0] {
            ParsedBlock::Image { rgba: parsed_rgba } => {
                assert_eq!((parsed_rgba.width, parsed_rgba.height), (rgba.width, rgba.height));
                assert_eq!(parsed_rgba.data, rgba.data);
            }
            _ => panic!("expected Image block"),
        }
    }

    #[test]
    fn to_html_and_text_serializes_empty_paragraphs_as_br() {
        let b1 = Block::new(Kind::Title, "Header");
        let b2 = Block::new(Kind::Body, "");
        let b3 = Block::new(Kind::Body, "Content");
        let blocks = vec![b1, b2, b3];
        let range = Range {
            anchor: Caret { block: 0, offset: 0 },
            focus: Caret { block: 2, offset: 7 },
        };
        let (plain, html) = to_html_and_text(&blocks, range, &|_| None);
        assert_eq!(plain, "Header\n\nContent");
        assert!(html.contains("<h1>Header</h1>"));
        assert!(html.contains("<p><br></p>"));
        assert!(html.contains("<p>Content</p>"));
    }

    fn body(text: &str) -> ParsedBlock {
        ParsedBlock::Body {
            text: text.to_string(),
            marks: vec![Flags::NONE; text.len()],
            paints: vec![Paint::default(); text.len()],
        }
    }

    #[test]
    fn from_html_parses_empty_paragraphs_and_line_breaks() {
        // Standard <p><br></p>
        let p1 = from_html("<p>Hello</p><p><br></p><p>World</p>");
        assert_eq!(p1.len(), 3);
        assert_eq!(p1[0], body("Hello"));
        assert_eq!(p1[1], body(""));
        assert_eq!(p1[2], body("World"));

        // Empty <p></p>
        let p2 = from_html("<p>Hello</p><p></p><p>World</p>");
        assert_eq!(p2.len(), 3);
        assert_eq!(p2[0], body("Hello"));
        assert_eq!(p2[1], body(""));
        assert_eq!(p2[2], body("World"));

        // Double <br> inside paragraph: <p>Hello<br><br>World</p>
        let p3 = from_html("<p>Hello<br><br>World</p>");
        assert_eq!(p3.len(), 3);
        assert_eq!(p3[0], body("Hello"));
        assert_eq!(p3[1], body(""));
        assert_eq!(p3[2], body("World"));

        // Double <br> outside tags: Hello<br><br>World
        let p4 = from_html("Hello<br><br>World");
        assert_eq!(p4.len(), 3);
        assert_eq!(p4[0], body("Hello"));
        assert_eq!(p4[1], body(""));
        assert_eq!(p4[2], body("World"));

        // Single <br> does not emit extra empty block: Hello<br>World
        let p5 = from_html("Hello<br>World");
        assert_eq!(p5.len(), 2);
        assert_eq!(p5[0], body("Hello"));
        assert_eq!(p5[1], body("World"));

        // Non-breaking space in empty paragraph: <p>&nbsp;</p>
        let p6 = from_html("<p>Hello</p><p>&nbsp;</p><p>World</p>");
        assert_eq!(p6.len(), 3);
        assert_eq!(p6[0], body("Hello"));
        assert_eq!(p6[1], body(""));
        assert_eq!(p6[2], body("World"));

        // Nested wrapper: <div><p>Hello</p></div>
        let p7 = from_html("<div><p>Hello</p></div>");
        assert_eq!(p7.len(), 1);
        assert_eq!(p7[0], body("Hello"));

        // Single empty paragraph: <p><br></p>
        let p8 = from_html("<p><br></p>");
        assert_eq!(p8.len(), 1);
        assert_eq!(p8[0], body(""));
    }

    #[test]
    fn from_html_skips_style_and_script_tags() {
        let html = "<style>body { color: red; }</style><p>Visible</p><script>console.log(1);</script>";
        let parsed = from_html(html);
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0], body("Visible"));
    }
}
