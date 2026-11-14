//! "Export as Markdown...": a note as a plaintext `.md` file, its pictures as PNG files beside it.
//!
//! The user is choosing to take the note out of encryption, so this writes plain files. The text
//! is the note's own text format (a Markdown subset) with each picture reference replaced by a
//! link to the PNG written next to the file: `name-image-1.png`, `name-image-2.png`, ...

use crate::document::{Document, Kind};
use crate::images;
use crate::storage::Db;
use std::path::Path;

/// The file name offered in the save dialog: the note's title, with what Windows forbids removed.
pub fn suggested_name(title: &str) -> String {
    let clean: String = title.chars().map(|c| if c.is_control() || "\\/:*?\"<>|".contains(c) { ' ' } else { c }).collect();
    let clean = clean.split_whitespace().collect::<Vec<_>>().join(" ");
    let clean = clean.trim_end_matches('.').trim();
    format!("{}.md", if clean.is_empty() || clean == "Washi Note" { "Washi note" } else { clean })
}

/// The Markdown of `content` (a note's stored text); `image_file` names the file of each picture
/// (`None`: the picture is gone, its line is left out).
pub fn markdown(content: &str, image_file: &mut dyn FnMut(i64) -> Option<String>) -> String {
    let doc = Document::parse(content);
    let text = doc.to_text();
    let mut lines = Vec::new();
    let mut shown = 0;
    // one stored line per block
    for (line, block) in text.split('\n').zip(doc.blocks()) {
        if block.kind != Kind::Image {
            lines.push(line.to_string());
            continue;
        }
        if let Some(file) = block.image_id.and_then(|id| image_file(id)) {
            shown += 1;
            lines.push(format!("![image {shown}]({})", file.replace(' ', "%20")));
        }
    }
    // an image at the end of a note has an empty paragraph after it: not worth a blank line
    while lines.last().is_some_and(|l| l.is_empty()) {
        lines.pop();
    }
    let mut out = lines.join("\n");
    out.push('\n');
    out
}

/// Write the note as `path` (a `.md` file) and its pictures as PNG files in the same folder.
pub fn write(db: &Db, content: &str, path: &Path) -> std::io::Result<()> {
    let folder = path.parent().filter(|p| !p.as_os_str().is_empty()).unwrap_or(Path::new("."));
    let stem = path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| "note".into());
    let mut written = 0;
    let mut failure = None;
    let text = markdown(content, &mut |id| {
        let stored = db.image(id).ok().flatten()?;
        let png = if stored.mime == images::MIME_PNG {
            stored.data
        } else {
            images::encode_png(&images::decode(&stored.data).ok()?).ok()?
        };
        let file = format!("{stem}-image-{}.png", written + 1);
        match std::fs::write(folder.join(&file), png) {
            Ok(()) => {
                written += 1;
                Some(file)
            }
            Err(e) => {
                failure = Some(e);
                None
            }
        }
    });
    if let Some(e) = failure {
        return Err(e);
    }
    std::fs::write(path, text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn markdown_keeps_the_note_text_and_links_pictures() {
        let content = "# Trip\n## **Plan**\nsee below\n![](img:7)\n- bags\n![](img:8)";
        let md = markdown(content, &mut |id| Some(format!("trip-image-{}.png", id - 6)));
        assert_eq!(md, "# Trip\n## **Plan**\nsee below\n![image 1](trip-image-1.png)\n- bags\n![image 2](trip-image-2.png)\n");
    }

    #[test]
    fn colors_are_exported_as_the_note_stores_them() {
        // the export is the note's text format, not a rendering: ink and marker keep their markers
        let content = "# Trip\nsee <c=blue>blue</c> and <m=yellow>marked</m> and <c=red><m=green>both</c></m>";
        assert_eq!(markdown(content, &mut |_| None), format!("{content}\n"));
    }

    #[test]
    fn a_missing_picture_is_left_out_and_names_with_spaces_are_escaped() {
        let content = "# T\n![](img:1)\n![](img:2)\nend";
        let md = markdown(content, &mut |id| (id == 2).then(|| "my note-image-1.png".to_string()));
        assert_eq!(md, "# T\n![image 1](my%20note-image-1.png)\nend\n");
    }

    #[test]
    fn the_suggested_name_is_a_valid_file_name() {
        assert_eq!(suggested_name("Endfield Note"), "Endfield Note.md");
        assert_eq!(suggested_name("a/b: c?"), "a b c.md");
        assert_eq!(suggested_name("Washi Note"), "Washi note.md");
        assert_eq!(suggested_name(""), "Washi note.md");
    }
}
