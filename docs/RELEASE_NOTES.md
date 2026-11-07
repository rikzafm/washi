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

## Known limitations

- Per-paragraph formatting only (no inline rich text).
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
