# Washi 0.1.0 release notes

Lightweight encrypted sticky notes for Windows. One exe, no installer needed.

## Performance (release build, 100% display scale; details in docs/PERF.md)

| | target | measured |
|---|---|---|
| 3 notes, active | <= 15 MB | 2.9 MB private WS (12.0 MB untrimmed) |
| 3 notes, suspended | <= 5 MB | 0.3 - 0.9 MB |
| per extra note | <= 2 MB | about 0 trimmed, 2.5 MB untrimmed (accepted) |
| startup | < 150 ms | median 147 ms (accepted) |
| wake | <= 50 ms | 9 - 18 ms |
| idle CPU, suspended | 0% | 0 |
| idle CPU, foreground | 0% | about 2% of a core, caret blink (accepted) |
| exe size | <= 8 MB, no DLLs | 12.96 MB, no DLLs (accepted) |

## Text color and highlight

- A seventh toolbar button, an "A" over a bar, opens a popover with ten ink colors and nine marker (highlight) colors, a caption and Clear. It works on a selection, or on what you type next at a caret. Each pick is one undo step; the popover stays open until Escape, a click elsewhere, or Clear. Arrow keys, Enter and Space drive it from the keyboard while the text keeps the caret.
- Colors are stored in the note as `<c=blue>` ... `</c>` and `<m=yellow>` ... `</m>` (names, not numbers). The closing tags are not mirrored: a note with both colors is stored as `<c=blue><m=yellow>body</c></m>`. Unknown names read as the default color. Export as Markdown writes the same tags.
- Copying text gives plain text; colors are not copied.
- Older builds show these tags as literal text (they escape `<` only when writing). Open a colored note only with this version or newer.

## Fonts and Font size

- Settings > Font & Style has a **Font** row: the Font in use in its own face, a summary of its Weight, Width and Slant, and a manage button that opens the **Font manager**, a taped slip beside the Settings page. It lists your fonts, picks the Font in use (one click), removes installed fonts, and installs new ones. Esc, the manage button, another Tab or closing Settings closes it.
- Install a font with "Install a font" (a file picker) or by dropping the file on the open Font manager. Take a Google Fonts `.zip` exactly as it downloads, or a single `.ttf`. Washi picks the variable font from the zip when there is one. Installing does not switch to the font; installing the same family again updates it and keeps its settings. Fonts stay on your machine (`%APPDATA%\Washi\fonts`) and are never sent anywhere.
- Shadows Into Light stays the default and cannot be removed. Removing the font in use switches back to it and deletes the removed font's files. If an installed font's files go missing, Washi starts in Shadows Into Light and tells you once.
- The Font in use's row opens a **Weight**, **Width** and **Slant** panel. Sliders snap (Weight in hundreds, Width at 75 / 100 / 125 %, Slant at 0 / 5 / 10 degrees), have a number box, and follow the keyboard. Only the axes a font has get a slider ("Not in this font" otherwise); Slant is offered for every font, using the font's own slant axis or a lean Washi makes. Every font remembers its own values, the Settings preview follows while you drag, and every note changes when you let go.
- The chosen font is the regular look of all text: every note and every screen. Bold and italic in a note are made by Washi on top of it (bold is heavier than your Weight, italic leans further than your Slant), so they work in every font, even one with a single style. Menus, Settings and All notes text shrink just enough (down to 70%) to fit a wide font; note text is never shrunk.
- **Font size** replaces Text size: a slider from 12 to 40 with snaps at 14, 18, 22, 26, 30 and 36, and a number box. 22 looks exactly like notes always have. It sets note text; titles, headings, line spacing, check boxes and bullets scale with it. Menus and Settings keep following Appearance > UI size. Open notes reflow at once and keep their scroll place as far as they can.
- A font is refused, with the reason in the error dialog, when it is not a font, is a `.otf` or any other font with CFF outlines, lacks basic Latin letters, is too large (zip over 64 MB, more than 600 entries, a font over 24 MB), uses unsupported compression, or is named like a font Washi ships.

Limits:

- CFF `.otf` fonts are not supported yet: use the `.ttf` (Google Fonts zips have them).
- Only Weight, Width and Slant are adjustable; other axes (optical size, grade) stay at the font's defaults. A font's own designed italic and true bold letter widths are not used: bold and italic are made by Washi and keep the regular letter widths.
- Letters a font lacks come from system fonts, as before.
- Fonts with more than about 4,000 glyphs (Arial, Segoe UI) make their five faces on the spot when picked or at startup, which takes 60 to 120 ms (measured in `docs/adr/0003-runtime-font-baking.md`); the planned "Regular first" split is not built.
- Not checked by a person yet: dropping a file from Explorer, the Font manager's look against the design, labels at every UI size with very wide fonts, the picture viewer (see `docs/QA_CHECKLIST.md`).
- The font feature uses unstable Slint APIs (runtime font registration, window drop events), pinned by `Cargo.lock`; a Slint upgrade must recheck them (ADR 0003).

## Known limitations

- Per-paragraph formatting only (no inline rich text), except text color and highlight, which are per character.
- Screen readers get names for the color swatches but not for the keyboard ring's position.
- The database key is tied to the Windows account (DPAPI) with no backup: a profile reset loses the notes.
- Task Manager shows the efficiency-mode leaf, not "Suspended".
- Windows only.
- Images are drawn level (not tilted with the paper); EXIF orientation is ignored.

## Known deviations from the mockups (accepted)

- Body font match IoU 0.825 accepted.
- Remaining rows in `docs/design/fidelity-baseline.txt` (hover:title, hover:body, hover:image, idle:image, invariants:fonts) stay baselined.
- The text stops 8 px above the toolbar buttons instead of running up against them.
- Heading and body lines are spaced about 1.45 em (the mockup's 1.22 to 1.28 em made the handwriting's lines touch).
- Text is tilted with a stepped baseline at rest and level while editing.
- Live capture was not run at 150%; human sign-off pending.

## Not verified here

Manual QA, product-owner sign-off, clean-VM run and installer: see `docs/QA_CHECKLIST.md` (all unchecked).
