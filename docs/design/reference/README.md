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
