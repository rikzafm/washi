# Reference mockups (frozen)

These two images are the source of truth for Washi's look. Never edit or re-export them.
`tools/uidiff` measures them, and `docs/design/spec.json` stores the measured values.
A test in `tools/uidiff` fails if either file changes.

| File | Canvas | SHA-256 |
|---|---|---|
| `mockup-hover.png` | 720 x 840 | `7462f9b41d65bd49c10f1e31ee486fc671289fe5f3b816f3484a37a402536c9b` |
| `mockup-idle.png` | 720 x 800 | `f97e2c3db4aea64b76cb2e73ffe5ada411390d5fc621f5b4ebce8cf2edf5b167` |

`mockup-hover.png` has the toolbar visible and is used for every region.
`mockup-idle.png` has no toolbar. Its gray caption line is leftover text and is masked out of comparisons.

## Settings page (renders of the mockup's HTML)

`settings-general.png`, `settings-style.png`, `settings-shortcuts.png` and `settings-privacy.png`, plus
`settings-style-font-list.png` and `settings-style-tape-list.png` (a dropdown open), are renders of the
"Setting Page" artboard (`V2.dc.html`): one window of the Settings page at 1x (886 px wide: the 282 px
Sidebar strip, the 520 px Paper and the 42 px margins; as tall as the Paper plus margins) on the desk
color `#D9DDE1`. The browser used the repo's bundled Shadows Into Light (the other fonts of the Font list
are not bundled), and the Tape color is the default Pink + blue. The Paper's heights in them are the
mockup's own: General 658, Font & Style 603, Shortcuts 767, Privacy 600.

`tools/snap/tests/settings_fidelity.rs` renders each tab with the real app and compares it to its image
on 8 px blocks of luminance (a browser and Slint place glyphs a pixel apart, so single pixels are not
compared). These images come from the mockup, not from the app; replace them only when the mockup changes.
