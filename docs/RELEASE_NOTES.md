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
- Text is tilted with a stepped baseline at rest and level while editing.
- Live capture was not run at 150%; human sign-off pending.

## Not verified here

Manual QA, product-owner sign-off, clean-VM run and installer: see `docs/QA_CHECKLIST.md` (all unchecked).
