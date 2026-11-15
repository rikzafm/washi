use super::*;

const MOCKUP: &str = "# Endfield Note\n## **Thermite Cutter : POG & Akekuri**\nCurrently POG no Weapon";

fn texts(d: &Document) -> Vec<&str> {
    d.blocks().iter().map(|b| b.text.as_str()).collect()
}
fn kinds(d: &Document) -> Vec<Kind> {
    d.blocks().iter().map(|b| b.kind).collect()
}
fn doc(blocks: &[(Kind, &str)]) -> Document {
    Document::from_blocks(blocks.iter().map(|&(k, t)| Block::new(k, t)).collect())
}

#[test]
fn hash_or_dash_then_space_at_the_start_of_a_body_makes_a_heading_or_list_item() {
    let mut d = doc(&[(Kind::Title, "t"), (Kind::Body, "# ")]);
    assert_eq!(d.auto_format(1, "#", 2), Some(Caret { block: 1, offset: 0 }));
    assert_eq!((d.blocks()[1].kind, d.blocks()[1].text.as_str()), (Kind::Heading, ""));
    let mut d = doc(&[(Kind::Title, "t"), (Kind::Body, "- milk")]);
    assert_eq!(d.auto_format(1, "-milk", 2), Some(Caret { block: 1, offset: 0 }));
    assert_eq!((d.blocks()[1].kind, d.blocks()[1].text.as_str()), (Kind::ListItem, "milk"));
    let mut d = doc(&[(Kind::Title, "t"), (Kind::Body, "* eggs")]);
    assert_eq!(d.auto_format(1, "*eggs", 2), Some(Caret { block: 1, offset: 0 }));
    assert_eq!((d.blocks()[1].kind, d.blocks()[1].text.as_str()), (Kind::ListItem, "eggs"));
}

#[test]
fn auto_format_is_predicted_from_the_text_as_edit_text_writes_it() {
    // `edit_text` turns a line break into a space, so "-" + line break at the caret 2 is "- "
    let mut d = doc(&[(Kind::Title, "t"), (Kind::Body, "")]);
    assert!(d.would_auto_format(1, "", "-\n", 2));
    d.edit_text(1, "-\n", 2, None, None);
    assert_eq!(d.auto_format(1, "", 2), Some(Caret { block: 1, offset: 0 }));
}

#[test]
fn the_markers_do_nothing_elsewhere() {
    let mut d = doc(&[(Kind::Title, "# "), (Kind::Body, "a # "), (Kind::Body, "- x")]);
    assert_eq!(d.auto_format(0, "#", 2), None, "the title stays the title");
    assert_eq!(d.auto_format(1, "a #", 4), None);
    assert_eq!(d.auto_format(2, "- x", 2), None, "already there before the edit");
    assert_eq!(d.auto_format(9, "", 2), None);
}

// ---- the format ----

#[test]
fn the_mockup_fixture_is_title_heading_body() {
    let d = Document::parse(MOCKUP);
    assert_eq!(kinds(&d), [Kind::Title, Kind::Heading, Kind::Body]);
    assert_eq!(texts(&d), ["Endfield Note", "Thermite Cutter : POG & Akekuri", "Currently POG no Weapon"]);
    assert!(d.blocks()[1].marks.iter().all(|m| m.contains(Style::BOLD)));
    assert_eq!(d.to_text(), MOCKUP);
}

#[test]
fn every_kind_has_its_line() {
    let text = "# t\n## **h**\nb\n- item\n![](img:42)\n**bold** *it* ~~s~~";
    let d = Document::parse(text);
    assert_eq!(kinds(&d), [Kind::Title, Kind::Heading, Kind::Body, Kind::ListItem, Kind::Image, Kind::Body]);
    assert_eq!(d.blocks()[3].text, "item");
    assert_eq!(d.blocks()[4].image_id, Some(42));
    // markers inside a line style the words between them
    let last = &d.blocks()[5];
    assert_eq!(last.text, "bold it s");
    assert_eq!(last.spans(), [(0, 4, Style::BOLD), (4, 5, Style::NONE), (5, 7, Style::ITALIC), (7, 8, Style::NONE), (8, 9, Style::STRIKE)]);
    assert_eq!(d.to_text(), text);
    assert_eq!(Document::parse(&d.to_text()), d);
}

#[test]
fn style_flags_wrap_the_whole_block() {
    let all = Style::BOLD.toggled(Style::ITALIC).toggled(Style::UNDERLINE).toggled(Style::STRIKE);
    let d = Document::from_blocks(vec![Block::new(Kind::Title, "t"), Block::new(Kind::Body, "x").with_style(all)]);
    assert_eq!(d.to_text(), "# t\n<u>~~***x***~~</u>");
    assert_eq!(Document::parse(&d.to_text()), d);
}

#[test]
fn an_empty_note_is_stored_as_no_text() {
    assert_eq!(Document::new().to_text(), "");
    assert_eq!(Document::parse(&Document::new().to_text()), Document::new());
}

#[test]
fn plain_text_of_older_notes_is_a_title_and_body_lines() {
    let d = Document::parse("shopping list\nmilk\n\neggs");
    assert_eq!(kinds(&d), [Kind::Title, Kind::Body, Kind::Body, Kind::Body]);
    assert_eq!(texts(&d), ["shopping list", "milk", "", "eggs"]);
    assert_eq!(Document::parse("a\r\nb").blocks()[0].text, "a");
}

#[test]
fn the_first_block_is_always_the_title_and_empty_text_is_one_empty_title() {
    for s in ["", "- x", "## y", "![](img:3)", "\n\n"] {
        assert_eq!(Document::parse(s).blocks()[0].kind, Kind::Title, "{s:?}");
    }
    assert!(Document::parse("").is_empty());
    assert!(!Document::parse("a").is_empty());
    assert!(!Document::parse("\n").is_empty());
    assert_eq!(Document::from_blocks(vec![Block::image(1)]).blocks()[0].kind, Kind::Title);
}

#[test]
fn text_that_looks_like_markers_survives() {
    for s in ["# x", "## **y**", "- z", "![](img:1)", "**b**", "*i*", "~~s~~", "<u>u</u>", "\\", "\\*", "\\\\n", "-", "#", "", "*", "**", "****", "a*", "[ ] x", "[x] x", "[ ] ", "[ ]", "\\[ ] x"] {
        for kind in [Kind::Title, Kind::Heading, Kind::Body, Kind::ListItem, Kind::Check, Kind::Checked] {
            for style in 0..16 {
                let d = Document::from_blocks(vec![Block::new(Kind::Title, ""), Block::new(kind, s).with_style(Style::from_bits(style))]);
                assert_eq!(Document::parse(&d.to_text()), d, "{kind:?} {s:?} {style}: {:?}", d.to_text());
                let t = Document::from_blocks(vec![Block::new(kind, s).with_style(Style::from_bits(style))]);
                assert_eq!(Document::parse(&t.to_text()), t, "title {s:?} {style}");
            }
        }
    }
}

struct Rng(u64);
impl Rng {
    fn next(&mut self, n: usize) -> usize {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        ((self.0 >> 33) as usize) % n
    }
}

fn random_text(r: &mut Rng) -> String {
    const PIECES: [&str; 25] = ["a", "b", " ", "*", "**", "~~", "~", "<u>", "</u>", "<", "\\", "#", "## ", "- ", "!", "![](img:", ")", "7", "é", "日本", "\u{1F600}", "-", "[ ] ", "[x] ", "[ ]"];
    (0..r.next(8)).map(|_| PIECES[r.next(PIECES.len())]).collect()
}

#[test]
fn property_every_document_round_trips() {
    let mut r = Rng(1);
    for _ in 0..5000 {
        let blocks = (0..1 + r.next(6))
            .map(|_| {
                let b = match r.next(8) {
                    6 => Block::new(Kind::Check, &random_text(&mut r)),
                    7 => Block::new(Kind::Checked, &random_text(&mut r)),
                    0 => Block::new(Kind::Heading, &random_text(&mut r)),
                    1 => Block::new(Kind::ListItem, &random_text(&mut r)),
                    2 => Block::image(r.next(100) as i64 - 50),
                    3 => Block::new(Kind::Title, &random_text(&mut r)),
                    _ => Block::new(Kind::Body, &random_text(&mut r)),
                };
                let mut b = b.with_style(Style::from_bits(r.next(16) as u8));
                if r.next(2) == 0 {
                    // a style per character
                    let starts: Vec<usize> = b.text.char_indices().map(|(i, _)| i).collect();
                    for (k, &i) in starts.iter().enumerate() {
                        let end = starts.get(k + 1).copied().unwrap_or(b.text.len());
                        let s = Style::from_bits(r.next(16) as u8);
                        b.marks[i..end].fill(s);
                    }
                }
                b
            })
            .collect();
        let d = Document::from_blocks(blocks);
        assert_eq!(Document::parse(&d.to_text()), d, "{:?}", d.to_text());
    }
}

#[test]
fn property_malformed_text_never_panics_and_settles() {
    let mut r = Rng(7);
    for _ in 0..5000 {
        let s: String = (0..r.next(6)).map(|_| if r.next(4) == 0 { "\n".to_string() } else { random_text(&mut r) }).collect();
        let d = Document::parse(&s);
        assert_eq!(d.blocks()[0].kind, Kind::Title);
        let again = Document::parse(&d.to_text());
        assert_eq!(again, d, "{s:?}");
    }
    for s in ["![](img:", "![](img:999999999999999999999)", "![](img:x)", "<u>", "</u>", "<u></u>", "~~", "* ", "\\", "# ", "## **", "\r", "\u{0}"] {
        let _ = Document::parse(s);
        let _ = Document::parse(&format!("t\n{s}"));
    }
}

// ---- Enter ----

#[test]
fn enter_splits_the_paragraph_at_the_caret() {
    let mut d = doc(&[(Kind::Title, "t"), (Kind::Body, "hello world")]);
    assert_eq!(d.split(1, 5), Caret { block: 2, offset: 0 });
    assert_eq!(texts(&d), ["t", "hello", " world"]);
    assert_eq!(kinds(&d), [Kind::Title, Kind::Body, Kind::Body]);
}

#[test]
fn enter_at_the_boundaries_makes_an_empty_block() {
    let mut d = doc(&[(Kind::Title, "t"), (Kind::Body, "ab")]);
    d.split(1, 2);
    assert_eq!(texts(&d), ["t", "ab", ""]);
    d.split(1, 0);
    assert_eq!(texts(&d), ["t", "", "ab", ""]);
    // an offset inside a character or past the end is clamped
    let mut e = doc(&[(Kind::Title, "é")]);
    e.split(0, 1);
    assert_eq!(texts(&e), ["", "é"]);
    e.split(1, 99);
    assert_eq!(texts(&e), ["", "é", ""]);
}

#[test]
fn enter_after_title_or_heading_starts_body() {
    let mut d = doc(&[(Kind::Title, "ti"), (Kind::Heading, "he")]);
    d.toggle_style(0, 0, 2, Style::BOLD);
    d.split(0, 2);
    assert_eq!(kinds(&d), [Kind::Title, Kind::Body, Kind::Heading]);
    assert!(d.blocks()[1].marks.is_empty());
    d.split(2, 2);
    assert_eq!(kinds(&d), [Kind::Title, Kind::Body, Kind::Heading, Kind::Body]);
    // the middle of a title: the title keeps its head, the rest is body
    let mut e = doc(&[(Kind::Title, "abcd")]);
    e.split(0, 2);
    assert_eq!((kinds(&e), texts(&e)), (vec![Kind::Title, Kind::Body], vec!["ab", "cd"]));
}

#[test]
fn enter_in_a_list_continues_it_and_on_an_empty_item_ends_it() {
    let mut d = doc(&[(Kind::Title, "t"), (Kind::ListItem, "milk")]);
    d.toggle_style(1, 2, 4, Style::ITALIC);
    d.split(1, 2);
    assert_eq!(kinds(&d), [Kind::Title, Kind::ListItem, Kind::ListItem]);
    assert_eq!(d.blocks()[2].spans(), [(0, 2, Style::ITALIC)], "the text after the caret keeps its style");
    d.set_text(2, "");
    assert_eq!(d.split(2, 0), Caret { block: 2, offset: 0 });
    assert_eq!(kinds(&d), [Kind::Title, Kind::ListItem, Kind::Body]);
    assert_eq!(d.len(), 3);
}

#[test]
fn enter_on_an_image_adds_body_after_it() {
    let mut d = doc(&[(Kind::Title, "t")]);
    d.insert_image(0, 5);
    assert_eq!(d.split(1, 0), Caret { block: 2, offset: 0 });
    assert_eq!(kinds(&d), [Kind::Title, Kind::Image, Kind::Body, Kind::Body]);
}

// ---- Backspace / Delete ----

#[test]
fn backspace_at_the_start_merges_into_the_previous_block() {
    let mut d = doc(&[(Kind::Title, "t"), (Kind::Heading, "head"), (Kind::Body, "body")]);
    assert_eq!(d.backspace(2, 0), Some(Caret { block: 1, offset: 4 }));
    assert_eq!((kinds(&d), texts(&d)), (vec![Kind::Title, Kind::Heading], vec!["t", "headbody"]));
    assert_eq!(d.backspace(1, 0), Some(Caret { block: 0, offset: 1 }));
    assert_eq!((kinds(&d), texts(&d)), (vec![Kind::Title], vec!["theadbody"]));
}

#[test]
fn backspace_does_nothing_inside_a_block_and_the_title_cannot_be_merged_away() {
    let mut d = doc(&[(Kind::Title, "t"), (Kind::Body, "b")]);
    assert_eq!(d.backspace(1, 1), None);
    assert_eq!(d.backspace(0, 0), None);
    assert_eq!(d.backspace(9, 0), None);
    assert_eq!(d.len(), 2);
    // an empty body right after the title merges into it, never the other way round
    let mut e = doc(&[(Kind::Title, "t"), (Kind::Body, "")]);
    e.backspace(1, 0);
    assert_eq!((kinds(&e), texts(&e)), (vec![Kind::Title], vec!["t"]));
}

#[test]
fn backspace_at_the_start_of_a_list_item_removes_the_bullet_first() {
    let mut d = doc(&[(Kind::Title, "t"), (Kind::Body, "a"), (Kind::ListItem, "")]);
    assert_eq!(d.backspace(2, 0), Some(Caret { block: 2, offset: 0 }));
    assert_eq!((kinds(&d), texts(&d)), (vec![Kind::Title, Kind::Body, Kind::Body], vec!["t", "a", ""]));
    // the next press merges as usual
    assert_eq!(d.backspace(2, 0), Some(Caret { block: 1, offset: 1 }));
    assert_eq!(kinds(&d), [Kind::Title, Kind::Body]);
    // a non-empty item keeps its text
    let mut e = doc(&[(Kind::Title, "t"), (Kind::ListItem, "milk")]);
    assert_eq!(e.backspace(1, 0), Some(Caret { block: 1, offset: 0 }));
    assert_eq!((kinds(&e), texts(&e)), (vec![Kind::Title, Kind::Body], vec!["t", "milk"]));
}

#[test]
fn backspace_after_an_image_deletes_the_image() {
    let mut d = doc(&[(Kind::Title, "t")]);
    d.insert_image(0, 9);
    d.set_text(2, "x");
    assert_eq!(d.backspace(2, 0), Some(Caret { block: 1, offset: 0 }));
    assert_eq!((kinds(&d), texts(&d)), (vec![Kind::Title, Kind::Body], vec!["t", "x"]));
}

#[test]
fn delete_at_the_end_merges_the_next_block_in() {
    let mut d = doc(&[(Kind::Title, "ab"), (Kind::Heading, "cd"), (Kind::ListItem, "ef")]);
    assert_eq!(d.delete(0, 2), Some(Caret { block: 0, offset: 2 }));
    assert_eq!((kinds(&d), texts(&d)), (vec![Kind::Title, Kind::ListItem], vec!["abcd", "ef"]));
    assert_eq!(d.delete(0, 1), None);
    assert_eq!(d.delete(1, 2), None, "the last block has nothing after it");
    assert_eq!(d.delete(7, 0), None);
}

#[test]
fn delete_before_an_image_deletes_the_image() {
    let mut d = doc(&[(Kind::Title, "t")]);
    d.insert_image(0, 9);
    assert_eq!(d.delete(0, 1), Some(Caret { block: 0, offset: 1 }));
    assert_eq!(kinds(&d), [Kind::Title, Kind::Body]);
}

// ---- styles, lists, images ----

#[test]
fn toggling_a_style_flips_one_flag_of_the_selection() {
    let mut d = doc(&[(Kind::Title, "t"), (Kind::Body, "one two")]);
    assert!(d.toggle_style(1, 4, 7, Style::BOLD));
    assert_eq!(d.blocks()[1].spans(), [(0, 4, Style::NONE), (4, 7, Style::BOLD)]);
    assert!(d.toggle_style(1, 7, 2, Style::UNDERLINE), "the ends can come in either order");
    assert_eq!(d.blocks()[1].spans(), [(0, 2, Style::NONE), (2, 4, Style::UNDERLINE), (4, 7, Style::BOLD.with(Style::UNDERLINE, true))]);
    // part of the selection is bold: the whole of it becomes bold; all of it is: none of it stays
    assert!(d.toggle_style(1, 0, 7, Style::BOLD));
    assert_eq!(d.blocks()[1].common_style(0, 7), Style::BOLD);
    assert!(d.toggle_style(1, 0, 7, Style::BOLD));
    assert_eq!(d.blocks()[1].common_style(0, 7), Style::NONE);
    assert_eq!(d.to_text(), "# t\non<u>e two</u>");
    assert!(!d.toggle_style(1, 3, 3, Style::BOLD), "nothing selected");
    assert!(d.toggle_style(0, 0, 1, Style::ITALIC), "a title can be styled");
    assert!(!d.toggle_style(5, 0, 1, Style::BOLD));
    d.insert_image(1, 1);
    assert!(!d.toggle_style(2, 0, 1, Style::BOLD), "an image has no text to style");
}

#[test]
fn the_list_button_over_a_range_makes_every_paragraph_a_list_item_or_none() {
    let mut d = doc(&[(Kind::Title, "t"), (Kind::Body, "a"), (Kind::ListItem, "b"), (Kind::Body, "c")]);
    let r = Range { anchor: Caret { block: 3, offset: 0 }, focus: Caret { block: 0, offset: 1 } };
    assert!(d.toggle_list_range(r));
    assert_eq!(d.blocks().iter().map(|b| b.kind).collect::<Vec<_>>(), [Kind::Title, Kind::ListItem, Kind::ListItem, Kind::ListItem]);
    assert!(d.toggle_list_range(r));
    assert_eq!(d.blocks().iter().map(|b| b.kind).collect::<Vec<_>>(), [Kind::Title, Kind::Body, Kind::Body, Kind::Body]);
    let only_title = Range { anchor: Caret { block: 0, offset: 0 }, focus: Caret { block: 0, offset: 1 } };
    assert!(!d.toggle_list_range(only_title));
}

#[test]
fn a_select_all_styles_the_whole_note() {
    let mut d = doc(&[(Kind::Title, "t"), (Kind::Body, "ab")]);
    d.toggle_style(1, 0, 1, Style::BOLD);
    assert_eq!(d.common_style_all(), Style::NONE);
    assert!(d.toggle_style_all(Style::BOLD));
    assert_eq!(d.common_style_all(), Style::BOLD);
    assert!(d.toggle_style_all(Style::BOLD));
    assert_eq!(d.common_style_all(), Style::NONE);
    assert!(!Document::new().toggle_style_all(Style::BOLD));
}

#[test]
fn the_word_at_the_caret() {
    let d = doc(&[(Kind::Title, "héllo wörld, x")]);
    assert_eq!(d.word_at(0, 2), Some((0, 6)));
    assert_eq!(d.word_at(0, 9), Some((7, 13)));
    assert_eq!(d.word_at(0, 0), Some((0, 6)), "at the start of a word selects the word");
    assert_eq!(d.word_at(0, 6), Some((0, 6)), "at the end of a word before space selects the word");
    assert_eq!(d.word_at(0, 7), Some((7, 13)), "at start of second word selects it");
    assert_eq!(d.word_at(0, 13), Some((7, 13)), "trailing boundary before punctuation selects word");
    assert_eq!(d.word_at(0, 14), None, "in whitespace/punctuation does not select word");
    assert_eq!(d.word_at(0, 15), Some((15, 16)), "start of single-letter word");
    assert_eq!(d.word_at(0, 16), Some((15, 16)), "at end of single-letter word at EOF");
}

#[test]
fn word_at_boundaries_and_whitespace() {
    let d = doc(&[(Kind::Body, "word1   word2...word3")]);
    // Beginning of word1
    assert_eq!(d.word_at(0, 0), Some((0, 5)));
    // Trailing boundary of word1 before whitespace
    assert_eq!(d.word_at(0, 5), Some((0, 5)));
    // Inside multiple whitespace
    assert_eq!(d.word_at(0, 6), None);
    assert_eq!(d.word_at(0, 7), None);
    // Beginning of word2
    assert_eq!(d.word_at(0, 8), Some((8, 13)));
    // Trailing boundary before punctuation
    assert_eq!(d.word_at(0, 13), Some((8, 13)));
    // Inside punctuation
    assert_eq!(d.word_at(0, 14), None);
    assert_eq!(d.word_at(0, 15), None);
    // Beginning of word3 after punctuation
    assert_eq!(d.word_at(0, 16), Some((16, 21)));
    // Trailing boundary of word3 at EOF
    assert_eq!(d.word_at(0, 21), Some((16, 21)));
}

// ---- styles while editing ----

#[test]
fn typed_text_takes_the_style_before_it() {
    let mut d = doc(&[(Kind::Title, "ab")]);
    d.toggle_style(0, 0, 1, Style::BOLD);
    // "a|b" -> "ax|b": after the bold a
    d.edit_text(0, "axb", 2, None, None);
    assert_eq!(d.blocks()[0].spans(), [(0, 2, Style::BOLD), (2, 3, Style::NONE)]);
    // at the start: the style of what follows
    d.edit_text(0, "yaxb", 1, None, None);
    assert_eq!(d.blocks()[0].spans(), [(0, 3, Style::BOLD), (3, 4, Style::NONE)]);
    // with a style asked for (a toolbar button with nothing selected)
    d.edit_text(0, "yaxbz", 5, Some(Style::ITALIC), None);
    assert_eq!(d.blocks()[0].spans(), [(0, 3, Style::BOLD), (3, 4, Style::NONE), (4, 5, Style::ITALIC)]);
}

#[test]
fn the_caret_settles_where_repeated_letters_were_typed() {
    let mut d = doc(&[(Kind::Title, "aa")]);
    d.toggle_style(0, 1, 2, Style::BOLD);
    // the plain a was doubled: the caret after the first a
    d.edit_text(0, "aaa", 1, None, None);
    assert_eq!(d.blocks()[0].spans(), [(0, 2, Style::NONE), (2, 3, Style::BOLD)]);
    // a selection replaced
    d.edit_text(0, "aZ", 2, None, None);
    assert_eq!(d.blocks()[0].text, "aZ");
    // a deletion
    d.edit_text(0, "Z", 0, None, None);
    assert_eq!(d.blocks()[0].text, "Z");
    assert_eq!(d.blocks()[0].marks.len(), 1);
}

#[test]
fn merges_and_pastes_keep_the_styles() {
    let mut d = doc(&[(Kind::Title, "t"), (Kind::Body, "ab"), (Kind::Body, "cd")]);
    d.toggle_style(2, 0, 2, Style::ITALIC);
    d.backspace(2, 0);
    assert_eq!(d.blocks()[1].spans(), [(0, 2, Style::NONE), (2, 4, Style::ITALIC)]);
    d.insert_text(1, 4, 4, "x\ny");
    assert_eq!(d.blocks()[1].spans(), [(0, 2, Style::NONE), (2, 5, Style::ITALIC)]);
    assert_eq!(d.blocks()[2].spans(), [(0, 1, Style::ITALIC)]);
    d.delete(1, 5);
    assert_eq!(d.blocks()[1].text, "abcdxy");
    assert_eq!(d.blocks()[1].marks.len(), 6);
}

// ---- inline styles in the stored text ----

#[test]
fn styled_words_are_stored_with_markers_where_the_style_changes() {
    let mut d = doc(&[(Kind::Title, "t"), (Kind::Body, "plain bold both under")]);
    d.toggle_style(1, 6, 15, Style::BOLD);
    d.toggle_style(1, 11, 15, Style::ITALIC);
    d.toggle_style(1, 16, 21, Style::UNDERLINE);
    assert_eq!(d.to_text(), "# t\nplain **bold *both*** <u>under</u>");
    assert_eq!(Document::parse(&d.to_text()), d);
    let mut s = doc(&[(Kind::Title, r"a*b~c<d\e")]);
    s.toggle_style(0, 1, 4, Style::STRIKE);
    assert_eq!(s.to_text(), r"# a~~\*b\~~~c\<d\\e");
    assert_eq!(Document::parse(&s.to_text()), s);
}

#[test]
fn older_notes_keep_their_meaning() {
    // whole-line markers around text with a marker character inside
    let d = Document::parse("t\n**a*b**\n*x* and *");
    assert_eq!(d.blocks()[1].text, "a*b");
    assert_eq!(d.blocks()[1].spans(), [(0, 3, Style::BOLD)]);
    // an odd marker: read as before (the outer pair wraps the line)
    assert_eq!((d.blocks()[2].text.as_str(), d.blocks()[2].spans()), ("x* and ", vec![(0, 7, Style::ITALIC)]));
    // a backslash that escapes nothing, and a first-character escape
    let e = Document::parse("C:\\Users\n\\**b** c");
    assert_eq!(e.blocks()[0].text, r"C:\Users");
    assert_eq!(e.blocks()[1].text, "**b** c");
    for d in [d, e] {
        assert_eq!(Document::parse(&d.to_text()), d);
    }
}

#[test]
fn toggling_the_list_switches_body_and_list_item_only() {
    let mut d = doc(&[(Kind::Title, "t"), (Kind::Body, "b"), (Kind::Heading, "h")]);
    assert!(d.toggle_list(1));
    assert_eq!(d.blocks()[1].kind, Kind::ListItem);
    assert!(d.toggle_list(1));
    assert_eq!(d.blocks()[1].kind, Kind::Body);
    assert!(!d.toggle_list(0) && !d.toggle_list(2) && !d.toggle_list(9));
}

#[test]
fn inserting_an_image_goes_after_the_block_and_never_ends_the_note() {
    let mut d = doc(&[(Kind::Title, "t"), (Kind::Body, "b")]);
    assert_eq!(d.insert_image(0, 7), 1);
    assert_eq!(kinds(&d), [Kind::Title, Kind::Image, Kind::Body]);
    assert_eq!(d.len(), 3);
    assert_eq!(d.insert_image(2, 8), 3);
    assert_eq!(kinds(&d), [Kind::Title, Kind::Image, Kind::Body, Kind::Image, Kind::Body]);
    assert_eq!(d.insert_image(99, 9), 5, "past the end appends");
    assert_eq!(Document::parse(&d.to_text()), d);
}

// ---- paste ----

#[test]
fn a_single_line_paste_replaces_the_selection() {
    let mut d = doc(&[(Kind::Title, "hello world")]);
    assert_eq!(d.insert_text(0, 0, 5, "HEY"), Caret { block: 0, offset: 3 });
    assert_eq!(texts(&d), ["HEY world"]);
}

#[test]
fn a_multi_line_paste_is_split_into_paragraphs() {
    let mut d = doc(&[(Kind::Title, "t"), (Kind::Body, "ab|cd")]);
    let c = d.insert_text(1, 3, 3, "x\r\ny\nz");
    assert_eq!(c, Caret { block: 3, offset: 1 });
    assert_eq!(texts(&d), ["t", "ab|x", "y", "zcd"]);
    assert_eq!(kinds(&d), [Kind::Title, Kind::Body, Kind::Body, Kind::Body]);
}

#[test]
fn pasting_lines_into_the_title_or_a_list() {
    let mut d = doc(&[(Kind::Title, "")]);
    d.insert_text(0, 0, 0, "one\ntwo\n\nthree");
    assert_eq!((kinds(&d), texts(&d)), (vec![Kind::Title, Kind::Body, Kind::Body, Kind::Body], vec!["one", "two", "", "three"]));
    let mut e = doc(&[(Kind::Title, "t"), (Kind::ListItem, "a")]);
    e.insert_text(1, 1, 1, "\nb");
    assert_eq!(kinds(&e), [Kind::Title, Kind::ListItem, Kind::ListItem]);
    let c = e.insert_text(1, 0, 99, "\n");
    assert_eq!((c, texts(&e)), (Caret { block: 2, offset: 0 }, vec!["t", "", "", "b"]));
}

// ---- navigation helpers ----

#[test]
fn a_picture_remembers_its_size_and_place_in_the_stored_text() {
    let mut d = doc(&[(Kind::Title, "t")]);
    d.insert_image(0, 5);
    assert_eq!(d.to_text(), "# t\n![](img:5)\n");
    assert!(d.set_image_pct(1, 40));
    assert!(!d.set_image_pct(1, 40), "nothing changes");
    assert!(d.set_image_align(1, Align::Right));
    assert!(!d.set_image_align(0, Align::Right), "the title is no picture");
    assert_eq!(d.to_text(), "# t\n![](img:5,40,r)\n");
    let back = Document::parse(&d.to_text());
    assert_eq!((back.blocks()[1].image_pct, back.blocks()[1].align), (40, Align::Right));
    assert!(d.set_image_pct(1, 1));
    assert_eq!(d.blocks()[1].image_pct, IMAGE_PCT_MIN, "clamped");
    assert_eq!(Document::parse("# t\n![](img:5,500,c)").blocks()[1].image_pct, IMAGE_PCT_MAX);
    assert_eq!(Document::parse("# t\n![](img:5,x,y)").blocks()[1].image_pct, IMAGE_PCT_MAX);
}

#[test]
fn the_box_of_a_picture_follows_its_width_and_place() {
    let mut b = Block::image(1);
    assert_eq!(b.image_box(200.0, (100, 50)), (0.0, 200.0, 100.0));
    b.image_pct = 50;
    b.align = Align::Center;
    assert_eq!(b.image_box(200.0, (100, 50)), (50.0, 100.0, 50.0));
    b.align = Align::Right;
    assert_eq!(b.image_box(200.0, (100, 50)), (100.0, 100.0, 50.0));
}

#[test]
fn deleting_a_picture_leaves_a_text_block_for_the_caret() {
    let mut d = doc(&[(Kind::Title, "t")]);
    d.insert_image(0, 9);
    assert_eq!(d.delete_image(1), Some(Caret { block: 1, offset: 0 }));
    assert_eq!(kinds(&d), [Kind::Title, Kind::Body]);
    assert_eq!(d.delete_image(0), None, "the title is no picture");
    let mut e = Document::from_blocks(vec![Block::new(Kind::Title, "t"), Block::image(1)]);
    assert_eq!(e.delete_image(1), Some(Caret { block: 1, offset: 0 }));
    assert_eq!(kinds(&e), [Kind::Title, Kind::Body]);
}

#[test]
fn neighbors_skip_images() {
    let mut d = doc(&[(Kind::Title, "t"), (Kind::Body, "b")]);
    d.insert_image(0, 1);
    assert_eq!(d.next_text_block(0), Some(2));
    assert_eq!(d.prev_text_block(2), Some(0));
    assert_eq!(d.prev_text_block(0), None);
    assert_eq!(d.next_text_block(2), None);
}

#[test]
fn plain_text_is_the_lines_without_pictures() {
    let mut d = doc(&[(Kind::Title, "T"), (Kind::Body, "a"), (Kind::Body, "b")]);
    d.insert_image(1, 7);
    assert_eq!(d.plain_text(), "T\na\nb");
}

#[test]
fn replacing_everything_starts_over_with_the_text() {
    let mut d = doc(&[(Kind::Title, "T"), (Kind::Body, "a"), (Kind::Body, "b")]);
    let c = d.replace_all("x\ny");
    assert_eq!(texts(&d), ["x", "y"]);
    assert_eq!(c, Caret { block: 1, offset: 1 });
    let c = d.replace_all("");
    assert!(d.is_empty());
    assert_eq!(c, Caret { block: 0, offset: 0 });
}

#[test]
fn newlines_never_enter_block_text() {
    let mut d = Document::new();
    d.set_text(0, "a\nb");
    assert_eq!(d.blocks()[0].text, "a b");
    assert_eq!(Document::parse(&d.to_text()), d);
}

// ---- ranges ----

fn c(block: usize, offset: usize) -> Caret {
    Caret { block, offset }
}
fn r(a: Caret, f: Caret) -> Range {
    Range { anchor: a, focus: f }
}
/// Title "Title", body "alpha", image, body "omega", list item "tail".
fn five() -> Document {
    Document::from_blocks(vec![
        Block::new(Kind::Title, "Title"),
        Block::new(Kind::Body, "alpha"),
        Block::image(7),
        Block::new(Kind::Body, "omega"),
        Block::new(Kind::ListItem, "tail"),
    ])
}

#[test]
fn ordered_puts_the_earlier_caret_first_whichever_way_it_was_dragged() {
    let (a, b) = (c(1, 3), c(3, 1));
    assert_eq!(r(a, b).ordered(), (a, b));
    assert_eq!(r(b, a).ordered(), (a, b));
    assert_eq!(r(c(2, 4), c(2, 1)).ordered(), (c(2, 1), c(2, 4)));
}

#[test]
fn replace_range_inside_one_block_is_insert_text() {
    let mut a = five();
    let mut b = five();
    let got = a.replace_range(r(c(1, 1), c(1, 4)), "X
Y");
    assert_eq!(got, b.insert_text(1, 1, 4, "X
Y"));
    assert_eq!(a, b);
    assert_eq!(texts(&a), ["Title", "aX", "Ya", "", "omega", "tail"]);
}

#[test]
fn replace_range_across_two_blocks_merges_them_and_keeps_the_first_kind() {
    let mut d = doc(&[(Kind::Title, "t"), (Kind::Heading, "Hello"), (Kind::ListItem, "world")]);
    let caret = d.replace_range(r(c(1, 2), c(2, 3)), "x");
    assert_eq!(texts(&d), ["t", "Hexld"]);
    assert_eq!(kinds(&d), [Kind::Title, Kind::Heading]);
    assert_eq!(caret, c(1, 3));
}

#[test]
fn delete_range_across_three_blocks_drops_the_image_and_the_blocks_between() {
    let mut d = five();
    let caret = d.delete_range(r(c(1, 2), c(3, 2)));
    assert_eq!(texts(&d), ["Title", "alega", "tail"]);
    assert_eq!(kinds(&d), [Kind::Title, Kind::Body, Kind::ListItem]);
    assert_eq!(caret, c(1, 2));
}

#[test]
fn a_reversed_range_behaves_like_the_ordered_one() {
    let mut a = five();
    let mut b = five();
    let ca = a.replace_range(r(c(3, 2), c(1, 2)), "Q");
    let cb = b.replace_range(r(c(1, 2), c(3, 2)), "Q");
    assert_eq!((a, ca), (b, cb));
}

#[test]
fn replace_range_from_the_start_to_the_end_of_the_note_is_replace_all() {
    let mut d = five();
    let caret = d.replace_range(r(c(0, 0), c(4, 4)), "x
y");
    assert_eq!(texts(&d), ["x", "y"]);
    assert_eq!(kinds(&d), [Kind::Title, Kind::Body]);
    assert_eq!(caret, c(1, 1));
    let mut d = five();
    assert_eq!(d.replace_range(r(c(4, 4), c(0, 0)), ""), c(0, 0));
    assert_eq!(texts(&d), [""]);
    assert!(d.is_empty());
}

#[test]
fn a_range_at_the_start_or_end_of_the_note() {
    let mut d = five();
    assert_eq!(d.delete_range(r(c(0, 0), c(0, 3))), c(0, 0));
    assert_eq!(d.blocks()[0].text, "le");
    let mut d = five();
    assert_eq!(d.delete_range(r(c(3, 2), c(4, 4))), c(3, 2));
    assert_eq!(texts(&d), ["Title", "alpha", "", "om"]);
    let mut d = five();
    assert_eq!(d.delete_range(r(c(0, 3), c(1, 0))), c(0, 3));
    assert_eq!(texts(&d)[..2], ["Titalpha", ""]);
}

#[test]
fn a_collapsed_range_inserts_text_and_deletes_nothing() {
    let mut d = five();
    assert_eq!(d.replace_range(r(c(1, 2), c(1, 2)), "--"), c(1, 4));
    assert_eq!(d.blocks()[1].text, "al--pha");
    let before = five();
    let mut d = five();
    assert_eq!(d.delete_range(r(c(1, 2), c(1, 2))), c(1, 2));
    assert_eq!(d, before);
}

#[test]
fn an_image_at_either_end_of_a_range_is_deleted_with_it() {
    let mut d = five();
    let caret = d.delete_range(r(c(1, 3), c(2, 0)));
    assert_eq!(texts(&d), ["Title", "alp", "omega", "tail"]);
    assert_eq!(caret, c(1, 3));
    let mut d = five();
    let caret = d.delete_range(r(c(2, 0), c(3, 2)));
    assert_eq!(texts(&d), ["Title", "alpha", "ega", "tail"]);
    assert_eq!(caret, c(2, 0));
    let mut d = doc(&[(Kind::Title, "t"), (Kind::Body, "a")]);
    d.insert_image(1, 3);
    let mut d2 = d.clone();
    // image to image with no text beside them leaves one empty body line
    d2.insert_image(2, 4);
    d2.delete_range(r(c(2, 0), c(3, 0)));
    assert!(d2.blocks().iter().all(|b| b.kind != Kind::Image));
}

#[test]
fn plain_range_joins_lines_with_newlines_and_skips_images() {
    let d = five();
    assert_eq!(d.plain_range(r(c(1, 2), c(4, 2))), "pha
omega
ta");
    assert_eq!(d.plain_range(r(c(4, 2), c(1, 2))), "pha
omega
ta");
    assert_eq!(d.plain_range(r(c(1, 1), c(1, 3))), "lp");
    assert_eq!(d.plain_range(r(c(1, 1), c(1, 1))), "");
    assert_eq!(d.plain_range(r(c(0, 0), c(4, 4))), d.plain_text());
}

#[test]
fn toggle_style_range_across_blocks_makes_everything_bold_if_only_part_is() {
    let mut d = Document::from_blocks(vec![
        Block::new(Kind::Title, "t"),
        Block::new(Kind::Body, "abc").with_style(Style::BOLD),
        Block::new(Kind::Body, "def"),
    ]);
    let range = r(c(1, 1), c(2, 2));
    assert_eq!(d.common_style_range(range), Style::NONE);
    assert!(d.toggle_style_range(range, Style::BOLD));
    assert_eq!(d.common_style_range(range), Style::BOLD);
    assert_eq!(d.blocks()[1].marks, [Style::BOLD; 3]);
    assert_eq!(d.blocks()[2].marks, [Style::BOLD, Style::BOLD, Style::NONE]);
    assert!(d.toggle_style_range(range, Style::BOLD));
    assert_eq!(d.blocks()[1].marks, [Style::BOLD, Style::NONE, Style::NONE]);
    assert_eq!(d.blocks()[2].marks, [Style::NONE; 3]);
}

#[test]
fn style_over_a_range_without_text_does_nothing() {
    let mut d = five();
    assert!(!d.toggle_style_range(r(c(1, 2), c(1, 2)), Style::BOLD));
    assert!(!d.toggle_style_range(r(c(2, 0), c(2, 0)), Style::BOLD));
    assert_eq!(d.common_style_range(r(c(1, 2), c(1, 2))), Style::NONE);
    let mut one = doc(&[(Kind::Title, "ab"), (Kind::Body, "cd")]);
    let mut same = one.clone();
    assert!(one.toggle_style_range(r(c(1, 1), c(1, 0)), Style::ITALIC));
    assert!(same.toggle_style(1, 0, 1, Style::ITALIC));
    assert_eq!(one, same);
}

// ---- color ----

fn colored(d: &Document, block: usize) -> Vec<(u8, u8)> {
    d.blocks()[block].paints.iter().map(|p| (p.ink, p.marker)).collect()
}

#[test]
fn colors_mixed_with_every_style_round_trip_on_one_line() {
    let mut d = doc(&[(Kind::Title, "t"), (Kind::Body, "plain bold ital under strike end")]);
    d.set_ink(1, 6, 15, 6);
    d.set_marker(1, 10, 21, 4);
    d.toggle_style(1, 6, 10, Style::BOLD);
    d.toggle_style(1, 11, 17, Style::ITALIC);
    d.toggle_style(1, 14, 25, Style::UNDERLINE);
    d.toggle_style(1, 19, 27, Style::STRIKE);
    let text = d.to_text();
    assert!(text.contains("<c=blue>") && text.contains("<m=yellow>") && text.contains("</c>") && text.contains("</m>"), "{text}");
    assert_eq!(Document::parse(&text), d, "{text}");
}

#[test]
fn a_line_without_color_markers_reads_and_writes_as_before() {
    let d = Document::parse("# t\n**a** *b* ~~c~~ <u>d</u> \\<c=blue>x");
    assert!(d.blocks()[1].paints.iter().all(|p| *p == Paint::default()));
    assert_eq!(d.blocks()[1].text, "a b c d <c=blue>x");
    assert_eq!(d.to_text(), "# t\n**a** *b* ~~c~~ <u>d</u> \\<c=blue>x");
}

#[test]
fn the_stored_markers_read_by_name() {
    let d = Document::parse("# t\nx <c=red>y<m=green>z</c> w</m>!");
    assert_eq!(d.blocks()[1].text, "x yz w!");
    assert_eq!(colored(&d, 1), [(0, 0), (0, 0), (9, 0), (9, 5), (0, 5), (0, 5), (0, 0)]);
}

#[test]
fn an_unknown_color_name_is_plain_text_color_and_the_tag_is_dropped() {
    let d = Document::parse("# t\na<c=chartreuse>b</c>c<m=none>d</m>**e**");
    assert_eq!(d.blocks()[1].text, "abcde");
    assert!(d.blocks()[1].paints.iter().all(|p| *p == Paint::default()));
    assert!(d.blocks()[1].marks[4].contains(Style::BOLD), "the rest of the line is still read the new way");
}

#[test]
fn property_colored_documents_round_trip() {
    let mut r = Rng(7);
    for _ in 0..3000 {
        let blocks = (0..1 + r.next(5))
            .map(|_| {
                let mut b = Block::new(Kind::Body, &random_text(&mut r));
                let starts: Vec<usize> = b.text.char_indices().map(|(i, _)| i).collect();
                for (k, &i) in starts.iter().enumerate() {
                    let end = starts.get(k + 1).copied().unwrap_or(b.text.len());
                    b.marks[i..end].fill(Style::from_bits(r.next(16) as u8));
                    b.paints[i..end].fill(Paint { ink: r.next(10) as u8, marker: r.next(10) as u8 });
                }
                b
            })
            .collect();
        let d = Document::from_blocks(blocks);
        assert_eq!(Document::parse(&d.to_text()), d, "{:?}", d.to_text());
    }
}

#[test]
fn a_range_over_three_blocks_takes_one_ink_and_a_mixed_range_has_no_common_color() {
    let mut d = doc(&[(Kind::Title, "t"), (Kind::Body, "abc"), (Kind::Body, "def"), (Kind::Body, "ghi")]);
    let range = r(c(1, 1), c(3, 2));
    assert_eq!(d.common_ink_range(range), Some(0));
    assert!(d.set_ink_range(range, 6));
    assert_eq!(d.common_ink_range(range), Some(6));
    assert_eq!(colored(&d, 1), [(0, 0), (6, 0), (6, 0)]);
    assert_eq!(colored(&d, 3), [(6, 0), (6, 0), (0, 0)]);
    assert_eq!(d.common_ink_range(r(c(1, 0), c(3, 3))), None);
    assert_eq!(d.common_marker_range(r(c(1, 0), c(3, 3))), Some(0));
    assert!(d.set_marker(1, 0, 1, 2));
    assert_eq!(d.common_marker_range(r(c(1, 0), c(3, 3))), None);
}

#[test]
fn setting_a_color_is_idempotent_and_clear_sets_default() {
    let mut d = doc(&[(Kind::Title, "t"), (Kind::Body, "abc")]);
    let all = d.all();
    assert!(d.set_ink_range(all, 3));
    assert!(!d.set_ink_range(all, 3), "picking the color the range has is a no-op, not a toggle");
    assert_eq!(d.common_ink_range(all), Some(3));
    assert!(d.set_marker_range(all, 5));
    assert!(d.set_ink_range(all, 0) && d.set_marker_range(all, 0));
    assert_eq!(colored(&d, 1), [(0, 0); 3]);
}

#[test]
fn color_over_an_empty_range_or_an_image_changes_nothing() {
    let mut d = Document::from_blocks(vec![Block::new(Kind::Title, "t"), Block::image(1), Block::new(Kind::Body, "ab")]);
    assert!(!d.set_ink_range(r(c(1, 0), c(1, 0)), 2));
    assert_eq!(d.common_ink_range(r(c(1, 0), c(1, 0))), None);
    assert!(d.set_ink_range(r(c(0, 0), c(2, 2)), 2), "an image in the range is skipped");
    assert_eq!(colored(&d, 2), [(2, 0); 2]);
    assert!(d.blocks()[1].paints.is_empty());
}

#[test]
fn typing_takes_the_paint_of_the_character_before_or_the_next_at_a_block_start() {
    let mut d = doc(&[(Kind::Title, "t"), (Kind::Body, "ab")]);
    d.set_ink(1, 0, 1, 6);
    d.set_marker(1, 0, 1, 4);
    d.edit_text(1, "aXb", 2, None, None);
    assert_eq!(colored(&d, 1), [(6, 4), (6, 4), (0, 0)]);
    d.edit_text(1, "YaXb", 1, None, None);
    assert_eq!(colored(&d, 1)[0], (6, 4), "at the start it takes the next character's");
    assert_eq!(d.blocks()[1].paint_for_insert(0), Paint { ink: 6, marker: 4 });
}

#[test]
fn paints_stay_as_long_as_the_text_through_every_operation() {
    let mut d = doc(&[(Kind::Title, "title"), (Kind::Body, "héllo wörld"), (Kind::Body, "second")]);
    d.set_ink_range(d.all(), 9);
    let ok = |d: &Document| {
        for b in d.blocks() {
            assert_eq!(b.paints.len(), b.text.len(), "{b:?}");
            assert_eq!(b.paints.len(), b.marks.len());
        }
    };
    d.split(1, 3);
    ok(&d);
    d.backspace(2, 0);
    ok(&d);
    d.delete(1, d.blocks()[1].text.len());
    ok(&d);
    d.insert_text(1, 2, 4, "a\nbc\nd");
    ok(&d);
    d.replace_range(r(c(0, 2), c(2, 1)), "zz");
    ok(&d);
    d.delete_range(r(c(0, 0), c(d.len() - 1, 1)));
    ok(&d);
    d.set_text(0, "new text");
    ok(&d);
    let mut e = doc(&[(Kind::Title, "t"), (Kind::Body, "# x")]);
    e.auto_format(1, "#x", 2);
    ok(&e);
}

#[test]
fn paints_survive_a_clone_for_undo_and_redo() {
    let mut d = doc(&[(Kind::Title, "t"), (Kind::Body, "abc")]);
    let before = d.clone();
    d.set_ink_range(d.all(), 2);
    let after = d.clone();
    assert_ne!(before, after);
    d = before.clone();
    assert_eq!(colored(&d, 1), [(0, 0); 3]);
    d = after;
    assert_eq!(colored(&d, 1), [(2, 0); 3]);
}

#[test]
fn the_palette_has_ten_entries_with_the_spec_colors() {
    assert_eq!(PALETTE.len(), 10);
    assert_eq!((PALETTE[0].name, PALETTE[0].ink, PALETTE[0].marker), ("Default", 0x2B2F36, None));
    assert_eq!((PALETTE[6].name, PALETTE[6].ink, PALETTE[6].marker), ("Blue", 0x3A6D96, Some(0xD3E5EF)));
    assert_eq!(PALETTE[9].marker, Some(0xFFE2DD));
}

#[test]
fn replace_range_rich_inserts_formatting_and_images() {
    use crate::clipboard_html::ParsedBlock;
    let mut d = doc(&[(Kind::Title, "Title"), (Kind::Body, "Hello world")]);
    let parsed = vec![
        ParsedBlock::Body {
            text: " beautiful".into(),
            marks: vec![Style::BOLD; 10],
            paints: vec![Paint { ink: 6, marker: 0 }; 10],
        },
        ParsedBlock::Image {
            rgba: crate::images::Rgba { width: 10, height: 10, data: vec![0; 400] },
        },
        ParsedBlock::ListItem {
            text: "item 1".into(),
            marks: vec![Style::NONE; 6],
            paints: vec![Paint::default(); 6],
        },
    ];
    let mut img_count = 0;
    let mut add_img = |_| {
        img_count += 1;
        Some(img_count)
    };
    // Paste into "Hello| world" (offset 5)
    let caret = d.replace_range_rich(Range { anchor: Caret { block: 1, offset: 5 }, focus: Caret { block: 1, offset: 5 } }, parsed, &mut add_img);
    assert_eq!(img_count, 1);
    assert_eq!(d.blocks()[1].text, "Hello beautiful");
    assert!(d.blocks()[1].marks[5..15].iter().all(|m| m.contains(Style::BOLD)));
    assert_eq!(d.blocks()[2].kind, Kind::Image);
    assert_eq!(d.blocks()[3].kind, Kind::ListItem);
    assert_eq!(d.blocks()[3].text, "item 1 world");
    assert_eq!(caret, Caret { block: 3, offset: 6 });
}

#[test]
fn replace_range_rich_preserves_empty_blocks() {
    use crate::clipboard_html::ParsedBlock;
    let mut d = doc(&[(Kind::Title, "Title"), (Kind::Body, "Start End")]);
    let parsed = vec![
        ParsedBlock::Body {
            text: " Part 1".into(),
            marks: vec![Style::NONE; 7],
            paints: vec![Paint::default(); 7],
        },
        ParsedBlock::Body {
            text: "".into(),
            marks: vec![],
            paints: vec![],
        },
        ParsedBlock::Body {
            text: "Part 2".into(),
            marks: vec![Style::NONE; 6],
            paints: vec![Paint::default(); 6],
        },
    ];
    let mut add_img = |_| None;
    let caret = d.replace_range_rich(Range { anchor: Caret { block: 1, offset: 5 }, focus: Caret { block: 1, offset: 5 } }, parsed, &mut add_img);
    assert_eq!(d.blocks().len(), 4);
    assert_eq!(d.blocks()[1].text, "Start Part 1");
    assert_eq!(d.blocks()[2].text, "");
    assert_eq!(d.blocks()[3].text, "Part 2 End");
    assert_eq!(caret, Caret { block: 3, offset: 6 });
}

// ---- check items ----

#[test]
fn check_items_have_their_stored_lines() {
    let text = "# t\n- [ ] open\n- [x] done\n- plain\n- \\[ ] bullet";
    let d = Document::parse(text);
    assert_eq!(kinds(&d), [Kind::Title, Kind::Check, Kind::Checked, Kind::ListItem, Kind::ListItem]);
    assert_eq!(texts(&d), ["t", "open", "done", "plain", "[ ] bullet"]);
    assert_eq!(d.to_text(), text);
}

#[test]
fn a_check_marker_and_space_at_the_start_of_a_body_makes_a_check_item() {
    for (typed, kind) in [("[] ", Kind::Check), ("[ ] ", Kind::Check), ("[x] ", Kind::Checked)] {
        let text = format!("{typed}milk");
        let mut d = doc(&[(Kind::Title, "t"), (Kind::Body, &text)]);
        assert_eq!(d.auto_format(1, &typed[..typed.len() - 1], typed.len()), Some(Caret { block: 1, offset: 0 }), "{typed:?}");
        assert_eq!((d.blocks()[1].kind, d.blocks()[1].text.as_str()), (kind, "milk"));
    }
    let mut d = doc(&[(Kind::Title, "t"), (Kind::Body, "[x] a")]);
    assert_eq!(d.auto_format(1, "[x] a", 4), None, "already there before the edit");
}

#[test]
fn enter_after_a_check_item_starts_an_unticked_one_and_on_an_empty_one_ends_the_list() {
    let mut d = doc(&[(Kind::Title, "t"), (Kind::Checked, "milk")]);
    assert_eq!(d.split(1, 4), Caret { block: 2, offset: 0 });
    assert_eq!(kinds(&d), [Kind::Title, Kind::Checked, Kind::Check]);
    d.split(2, 0);
    assert_eq!(kinds(&d), [Kind::Title, Kind::Checked, Kind::Body]);
}

#[test]
fn backspace_at_the_start_of_a_check_item_removes_its_box_first() {
    let mut d = doc(&[(Kind::Title, "t"), (Kind::Body, "a"), (Kind::Checked, "b")]);
    assert_eq!(d.backspace(2, 0), Some(Caret { block: 2, offset: 0 }));
    assert_eq!(kinds(&d), [Kind::Title, Kind::Body, Kind::Body]);
}

#[test]
fn the_check_button_turns_paragraphs_into_check_items_and_back() {
    let mut d = doc(&[(Kind::Title, "t"), (Kind::Body, "a"), (Kind::ListItem, "b"), (Kind::Checked, "c")]);
    let r = Range { anchor: Caret { block: 1, offset: 0 }, focus: Caret { block: 3, offset: 0 } };
    assert!(d.toggle_check_range(r));
    assert_eq!(kinds(&d), [Kind::Title, Kind::Check, Kind::Check, Kind::Checked]);
    assert!(d.toggle_check_range(r));
    assert_eq!(kinds(&d), [Kind::Title, Kind::Body, Kind::Body, Kind::Body]);
    assert!(!d.toggle_check_range(Range::within(0, 0, 0)), "the title is no check item");
}

#[test]
fn ticking_toggles_only_check_items() {
    let mut d = doc(&[(Kind::Title, "t"), (Kind::Check, "a"), (Kind::Body, "b")]);
    assert!(d.toggle_checked(1));
    assert_eq!(d.blocks()[1].kind, Kind::Checked);
    assert!(d.toggle_checked(1));
    assert_eq!(d.blocks()[1].kind, Kind::Check);
    assert!(!d.toggle_checked(2) && !d.toggle_checked(0) && !d.toggle_checked(9));
}

#[test]
fn numbered_items_count_from_one_in_each_unbroken_run() {
    let d = doc(&[
        (Kind::Title, "t"),
        (Kind::Numbered, "a"),
        (Kind::Numbered, "b"),
        (Kind::Body, "x"),
        (Kind::Numbered, "c"),
        (Kind::ListItem, "y"),
        (Kind::Numbered, "d"),
        (Kind::Numbered, "e"),
    ]);
    assert_eq!(d.numbers(), [0, 1, 2, 0, 1, 0, 1, 2]);
    assert_eq!((d.number(2), d.number(3), d.number(7)), (Some(2), None, Some(2)));
}

#[test]
fn one_dot_or_one_comma_then_space_at_the_start_of_a_body_makes_a_numbered_item() {
    for marker in ["1. ", "1, "] {
        let mut d = doc(&[(Kind::Title, "t"), (Kind::Body, &format!("{marker}milk"))]);
        assert_eq!(d.auto_format(1, &format!("{}milk", marker.trim_end()), 3), Some(Caret { block: 1, offset: 0 }), "{marker:?}");
        assert_eq!((d.blocks()[1].kind, d.blocks()[1].text.as_str()), (Kind::Numbered, "milk"));
    }
    // another number, the middle of a paragraph, or a paragraph that is no body: plain text
    let mut d = doc(&[(Kind::Title, "t"), (Kind::Body, "3. x")]);
    assert_eq!(d.auto_format(1, "3.x", 3), None);
    let mut d = doc(&[(Kind::Title, "t"), (Kind::ListItem, "1. x")]);
    assert_eq!(d.auto_format(1, "1.x", 3), None);
    let mut d = doc(&[(Kind::Title, "t"), (Kind::Body, "a 1. x")]);
    assert_eq!(d.auto_format(1, "a 1.x", 5), None);
}

#[test]
fn enter_continues_a_numbered_list_and_ends_it_on_an_empty_item_and_backspace_drops_the_number() {
    let mut d = doc(&[(Kind::Title, "t"), (Kind::Numbered, "milk")]);
    assert_eq!(d.split(1, 4), Caret { block: 2, offset: 0 });
    assert_eq!(kinds(&d), [Kind::Title, Kind::Numbered, Kind::Numbered]);
    assert_eq!(d.split(2, 0), Caret { block: 2, offset: 0 });
    assert_eq!(kinds(&d), [Kind::Title, Kind::Numbered, Kind::Body], "Enter on an empty item ends the list");
    let mut d = doc(&[(Kind::Title, "t"), (Kind::Numbered, "a"), (Kind::Numbered, "b")]);
    assert_eq!(d.backspace(2, 0), Some(Caret { block: 2, offset: 0 }));
    assert_eq!(kinds(&d), [Kind::Title, Kind::Numbered, Kind::Body]);
}

#[test]
fn a_list_type_picked_for_a_range_changes_only_its_paragraphs() {
    let mut d = doc(&[(Kind::Title, "t"), (Kind::ListItem, "a"), (Kind::Body, "b"), (Kind::Check, "c"), (Kind::Heading, "h"), (Kind::ListItem, "d")]);
    let r = Range { anchor: Caret { block: 1, offset: 0 }, focus: Caret { block: 4, offset: 1 } };
    assert!(d.set_list_range(r, Kind::Numbered));
    assert_eq!(kinds(&d), [Kind::Title, Kind::Numbered, Kind::Numbered, Kind::Numbered, Kind::Heading, Kind::ListItem]);
    // all of them already numbered: they all become body
    assert!(d.set_list_range(r, Kind::Numbered));
    assert_eq!(kinds(&d), [Kind::Title, Kind::Body, Kind::Body, Kind::Body, Kind::Heading, Kind::ListItem]);
    // clearing takes off bullets and numbers, and nothing else
    let mut d = doc(&[(Kind::Title, "t"), (Kind::ListItem, "a"), (Kind::Numbered, "b"), (Kind::Check, "c")]);
    assert!(d.clear_list_range(d.all()));
    assert_eq!(kinds(&d), [Kind::Title, Kind::Body, Kind::Body, Kind::Check]);
    assert!(!d.clear_list_range(d.all()));
}

#[test]
fn numbered_items_are_stored_with_their_numbers_and_a_body_that_looks_numbered_is_escaped() {
    let d = doc(&[(Kind::Title, "t"), (Kind::Numbered, "a"), (Kind::Numbered, "b"), (Kind::Body, "3. apples"), (Kind::Numbered, "c"), (Kind::ListItem, "1. x")]);
    let text = d.to_text();
    assert_eq!(text, "# t\n1. a\n2. b\n\\3. apples\n1. c\n- \\1. x");
    assert_eq!(Document::parse(&text), d);
    // any number reads as a numbered item, counted again
    let d = Document::parse("# t\n7. a\n12. b\n2024 plans\n3.no space");
    assert_eq!(kinds(&d), [Kind::Title, Kind::Numbered, Kind::Numbered, Kind::Body, Kind::Body]);
    assert_eq!(texts(&d), ["t", "a", "b", "2024 plans", "3.no space"]);
    assert_eq!(d.to_text(), "# t\n1. a\n2. b\n2024 plans\n3.no space");
}

// ---- lists that are moved in (Tab) ----

fn moved(d: &Document) -> Vec<bool> {
    d.indents()
}

fn caret_range(block: usize) -> Range {
    Range::within(block, 0, 0)
}

#[test]
fn tab_moves_the_whole_list_the_caret_is_in_and_no_other() {
    let mut d = doc(&[
        (Kind::Title, "t"),
        (Kind::ListItem, "a"),
        (Kind::Numbered, "b"),
        (Kind::Check, "c"),
        (Kind::Body, "x"),
        (Kind::ListItem, "d"),
    ]);
    assert!(d.set_indent_range(caret_range(2), true));
    assert_eq!(moved(&d), [false, true, true, true, false, false]);
    assert!(!d.set_indent_range(caret_range(2), true), "already there");
    assert!(!d.set_indent_range(caret_range(4), true), "a body paragraph is no list");
    assert!(d.set_indent_range(caret_range(3), false));
    assert_eq!(moved(&d), [false; 6]);
}

#[test]
fn tab_over_a_selection_moves_every_list_it_touches() {
    let mut d = doc(&[(Kind::Title, "t"), (Kind::ListItem, "a"), (Kind::Body, "x"), (Kind::ListItem, "b"), (Kind::Body, "y"), (Kind::ListItem, "c")]);
    let sel = Range { anchor: Caret { block: 1, offset: 0 }, focus: Caret { block: 3, offset: 0 } };
    assert!(d.set_indent_range(sel, true));
    assert_eq!(moved(&d), [false, true, false, true, false, false]);
}

#[test]
fn backspace_at_the_front_of_a_moved_in_item_moves_the_list_back_then_drops_the_marker() {
    let mut d = doc(&[(Kind::Title, "t"), (Kind::ListItem, "a"), (Kind::ListItem, "b")]);
    d.set_indent_range(caret_range(1), true);
    assert_eq!(d.backspace(2, 0), Some(Caret { block: 2, offset: 0 }));
    assert_eq!((moved(&d), d.blocks()[2].kind), (vec![false; 3], Kind::ListItem), "the list moved back, the marker stayed");
    assert_eq!(d.backspace(2, 0), Some(Caret { block: 2, offset: 0 }));
    assert_eq!(d.blocks()[2].kind, Kind::Body);
}

#[test]
fn enter_in_a_moved_in_list_continues_it_there_and_a_new_item_joins_the_list_next_to_it() {
    let mut d = doc(&[(Kind::Title, "t"), (Kind::ListItem, "ab")]);
    d.set_indent_range(caret_range(1), true);
    d.split(1, 1);
    assert_eq!(moved(&d), [false, true, true]);
    // a paragraph made a list item takes the position of the list above it, or else below it
    let mut d = doc(&[(Kind::Title, "t"), (Kind::ListItem, "a"), (Kind::Body, "b"), (Kind::Body, "c"), (Kind::ListItem, "d")]);
    d.set_indent_range(caret_range(1), true);
    assert!(d.set_list_range(caret_range(2), Kind::ListItem));
    assert_eq!(moved(&d), [false, true, true, false, false], "joined the list above, which stays where it was");
    assert!(d.set_list_range(caret_range(3), Kind::Numbered));
    assert_eq!(moved(&d), [false, true, true, true, true], "the list below it joins the list above, and follows its position");
}

#[test]
fn a_new_list_next_to_a_moved_in_list_does_not_move_it() {
    let mut d = doc(&[(Kind::Title, "t"), (Kind::Body, "a"), (Kind::ListItem, "b"), (Kind::ListItem, "c")]);
    d.set_indent_range(caret_range(2), true);
    assert!(d.set_list_range(caret_range(1), Kind::ListItem));
    assert_eq!(moved(&d), [false, true, true, true]);
}

#[test]
fn lists_joined_by_a_deletion_take_the_position_of_the_upper_one() {
    let mut d = doc(&[(Kind::Title, "t"), (Kind::ListItem, "a"), (Kind::Body, ""), (Kind::ListItem, "b")]);
    d.set_indent_range(caret_range(3), true);
    assert_eq!(moved(&d), [false, false, false, true]);
    d.backspace(2, 0);
    d.delete(1, 1);
    assert!(moved(&d).iter().all(|&m| !m), "{:?}", moved(&d));
}

#[test]
fn a_moved_in_list_round_trips_through_the_stored_text_in_every_kind() {
    let mut d = doc(&[
        (Kind::Title, "t"),
        (Kind::ListItem, "a"),
        (Kind::Numbered, "b"),
        (Kind::Check, "c"),
        (Kind::Body, "x"),
        (Kind::Numbered, "d"),
    ]);
    d.set_indent_range(caret_range(1), true);
    d.set_indent_range(caret_range(5), true);
    let text = d.to_text();
    assert_eq!(text, "# t\n    - a\n    1. b\n    - [ ] c\nx\n    1. d");
    assert_eq!(Document::parse(&text), d);
}

#[test]
fn a_body_that_starts_with_four_spaces_and_a_marker_stays_a_body() {
    for text in ["    - x", "    3. x", "    - [ ] x"] {
        let d = doc(&[(Kind::Title, "t"), (Kind::Body, text)]);
        let back = Document::parse(&d.to_text());
        assert_eq!(back, d, "{text:?} -> {:?}", d.to_text());
        assert_eq!(back.blocks()[1].kind, Kind::Body);
    }
    // an item whose text starts that way too
    let mut d = doc(&[(Kind::Title, "t"), (Kind::ListItem, "    - x")]);
    d.set_indent_range(caret_range(1), true);
    assert_eq!(Document::parse(&d.to_text()), d);
}

#[test]
fn old_notes_and_other_lines_read_as_before() {
    let d = Document::parse("# t\n    plain\n  - two spaces\n- a\n1. b");
    assert_eq!(moved(&d), [false; 5]);
    assert_eq!(d.blocks()[1].kind, Kind::Body);
    assert_eq!(d.blocks()[2].kind, Kind::Body);
    // a later item of a list is read at the position of the list's first
    let d = Document::parse("# t\n- a\n    - b");
    assert_eq!(moved(&d), [false, false, false]);
}
