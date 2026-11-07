//! The note text as styled blocks: one block per line of the stored Markdown subset.
//!
//! A block keeps the markers around its text (`## **` ... `**`) so that serializing gives back
//! exactly what was parsed, whatever the user typed.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Title,
    Heading,
    Body,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Block {
    pub kind: Kind,
    prefix: String,
    suffix: String,
    pub text: String,
}

/// Parse the stored text. The first block is always the title; there is always at least one block.
pub fn parse(content: &str) -> Vec<Block> {
    content.split('\n').enumerate().map(|(i, line)| parse_line(i, line)).collect()
}

/// The stored text of the blocks (inverse of [`parse`]).
pub fn serialize(blocks: &[Block]) -> String {
    let mut out = String::new();
    for (i, b) in blocks.iter().enumerate() {
        if i > 0 {
            out.push('\n');
        }
        out.push_str(&b.prefix);
        out.push_str(&b.text);
        out.push_str(&b.suffix);
    }
    out
}

impl Block {
    /// The user edited the block's text; the markers stay.
    pub fn set_text(&mut self, text: &str) {
        self.text = text.to_string();
    }
}

fn parse_line(index: usize, line: &str) -> Block {
    let (mut prefix, mut rest, heading) = if let Some(r) = line.strip_prefix("## ") {
        ("## ".to_string(), r, true)
    } else if let Some(r) = line.strip_prefix("# ") {
        ("# ".to_string(), r, true)
    } else {
        (String::new(), line, false)
    };
    let mut suffix = String::new();
    if heading && rest.len() >= 4 && rest.starts_with("**") && rest.ends_with("**") {
        prefix.push_str("**");
        suffix.push_str("**");
        rest = &rest[2..rest.len() - 2];
    }
    let kind = if index == 0 {
        Kind::Title
    } else if heading {
        Kind::Heading
    } else {
        Kind::Body
    };
    Block { kind, prefix, suffix, text: rest.to_string() }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MOCKUP: &str = "# Endfield Note\n## **Thermite Cutter : POG & Akekuri**\nCurrently POG no Weapon";

    #[test]
    fn mockup_content_is_title_heading_body() {
        let b = parse(MOCKUP);
        assert_eq!(b.iter().map(|b| b.kind).collect::<Vec<_>>(), [Kind::Title, Kind::Heading, Kind::Body]);
        assert_eq!(b[0].text, "Endfield Note");
        assert_eq!(b[1].text, "Thermite Cutter : POG & Akekuri");
        assert_eq!(b[2].text, "Currently POG no Weapon");
    }

    #[test]
    fn empty_note_is_one_empty_title_block() {
        let b = parse("");
        assert_eq!(b.len(), 1);
        assert_eq!(b[0].kind, Kind::Title);
        assert_eq!(b[0].text, "");
    }

    #[test]
    fn round_trips_whatever_the_text_is() {
        for s in [MOCKUP, "", "plain", "a\n\nb\n", "## **\n**", "## ****", "# \n## x", "**bold**", "x\n# y\n## **z**"] {
            assert_eq!(serialize(&parse(s)), s, "{s:?}");
        }
    }

    #[test]
    fn plain_first_line_is_the_title() {
        let b = parse("shopping list\nmilk");
        assert_eq!(b[0].kind, Kind::Title);
        assert_eq!(b[1].kind, Kind::Body);
    }

    #[test]
    fn editing_keeps_the_markers() {
        let mut b = parse(MOCKUP);
        b[1].set_text("New heading");
        assert!(serialize(&b).contains("\n## **New heading**\n"));
    }
}
