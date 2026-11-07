# Washi 0.1.0 release notes (draft)

Lightweight encrypted sticky notes for Windows. One `washi.exe`, no DLLs.

## Performance (release build; see docs/PERF.md for method and caveats)

| | measured | target |
|---|---|---|
| 3 notes active, private WS | 2.9 MB idle after trim (12.1 MB untrimmed) | <= 15 MB |
| 3 notes suspended | 0.3 MB | <= 5 MB |
| extra note | about 2.3 MB untrimmed | <= 2 MB (untrimmed misses) |
| startup to windows shown | about 142 ms median | < 150 ms |
| wake | 7-15 ms | <= 50 ms |
| idle CPU | 0 suspended; 0.5-1.3 % active | 0 |
| exe size | 13.59 MB | <= 8 MB (not met) |

## Known limitations

- Per-paragraph formatting only (no inline rich text).
- The encryption key is tied to the Windows account with no backup; a profile reset loses the notes.
- Task Manager shows efficiency mode (leaf) when suspended, not "Suspended".
- Windows only.
- Executable is 13.6 MB, above the 8 MB target (Slint text stack and SQLCipher/OpenSSL).
- Idle CPU is not exactly 0 while a note is visible (caret blink, cover check).
- Fidelity deviations accepted: body font IoU 0.825 (rasterizer difference), remaining rows in docs/design/fidelity-baseline.txt (hover title/body/image, idle image, invariants fonts), resting text uses a stepped baseline to fake the tilt (level while editing), images are drawn level, EXIF orientation ignored.
- Live capture comparison at 150% and the other scales was not run (build machine is at 100%).

## Not yet done (human)

See docs/QA_CHECKLIST.md: manual QA, product-owner sign-off, clean-VM run, installer and signing.
