# Fonts

Chosen by measurement (ticket 03), not by eye. `uidiff --font-match` renders the mockup strings with Slint's
software renderer (Slint 1.18.1, the renderer the app uses) in every candidate font, and scores each against
the hover mockup by ink-mask IoU.

## Chosen fonts

| Role | Font file (`fonts/`) | IoU | Size (logical px, at scale 1.5) | Runner-up |
|---|---|---|---|---|
| title "Endfield Note" | `ShadowsIntoLight.ttf` | **0.891** | 37.3 | Shadows Into Light Two 0.244 |
| heading "Thermite Cutter : POG &" / "Akekuri" | `ShadowsIntoLight-Bold.ttf` (derived, see below) | **0.914** | 24.0 | Caveat Brush 0.404 |
| body (4 lines) | `ShadowsIntoLight.ttf` | **0.825** (lines 0.816, 0.811, 0.842, 0.853) | 22.65 | Shadows Into Light Two 0.337 |
| toolbar **B** | `Gelasio-Bold.ttf` | **0.915** | 23.4 | STIX Two Text Bold 0.907 |
| toolbar *I* | `NotoSerif-Italic.ttf` | **0.860** | 22.8 | PT Serif Italic 0.856 |
| toolbar U | `NotoSerif-Regular.ttf` | **0.869** | 22.7 | Gelasio Regular 0.853 |
| toolbar ab | `Gelasio-Regular.ttf` | **0.853** | 23.1 | PT Serif Regular 0.756 |

All roles but the body clear the 0.85 gate. **The body is at 0.825, under the gate.** The identity of its font is
not in doubt (the next candidate scores 0.34), and the title, which uses the same font file, scores 0.891. The gap is
the rasterizer, not the font choice: the body strokes are about 1.4 mockup px wide, and the mockup was rasterized by a
browser at sub-pixel glyph positions while Slint snaps glyphs differently, so 1-px fringes along every stroke cost
about 0.15 of IoU. The overlay shows near-perfect glyph shapes and spacing. Ink areas agree within 2 %
(`UIDIFF_FONTMATCH_DUMP=<dir>` writes red/blue/purple overlays). More search (finer size, spacing and phase grids,
per-line tilt) did not move it, and thickening the face made it worse (0.793 at +0.008 em).
Decision for later steps: treat 0.80 as the body floor; Step 24 tunes size, line height and width against the
rendered snapshot, not against this score.

### What the "Shadows Into Light" family is here

* The title and body font is **Shadows Into Light** (the original, not "Two": 0.891 vs 0.244).
* The heading is **Shadows Into Light with a synthetic bold**: the mockup's browser thickened it. No bold face of
  any shortlisted font matches (best: Caveat Brush 0.404, Kalam 700 0.324), while the emboldened original scores 0.914.
  The Slint renderer cannot fake bold, so `ShadowsIntoLight-Bold.ttf` is a **real face made by growing every
  outline by 1/24 em** (Skia's synthetic-bold constant for small sizes; scanned 0.025 to 0.06 em, flat optimum
  0.042 to 0.046 em, 0.91). Made with `tools/fonts.py embolden`.
* There is no italic face of the family. The recorded **italic companion** is the same font sheared by 0.25
  (`ShadowsIntoLight-Italic.ttf`, and `ShadowsIntoLight-BoldItalic.ttf` for bold plus italic), the slant a browser's
  synthetic italic uses. Made with `tools/fonts.py slant`. This is for italic text in note blocks; it is not
  scored against the mockup (the mockup has no italic text).

So the app has all four faces for text: Regular, Bold, Italic, Bold Italic, each a real file. All four have the
same advances (emboldening and shearing keep them), so a line breaks in the same places whatever the styles of
its characters.

### The edit fields' face (`WashiCaret.ttf`)

Bold, italic, underline and strike apply to any stretch of text, but a Slint `TextInput` draws its whole text in
one face. So the note draws the words itself while editing too (from its own layout, each piece in its face), and
the fields lay their text out in **Washi Caret**: Shadows Into Light with every outline removed and every advance
kept. Its ascent and descent are cut down to 861 and 100 units (0.94 em together, under every line pitch of the
note; the heading and body pitch are 34 and 33 px, about 1.45 em), because a field draws its caret as tall as the larger of the line pitch and ascent + descent: with the
handwriting's own 1.61 em the caret stuck out of the body lines and was cut at the field's edges, off-center on
a paragraph's first and last line. `caret-line-em` in `washi/ui/theme.slint` is that sum over the units per em. The field still places the caret, draws the selection and breaks the lines (and handles the
input method) exactly over the drawn words, but shows no ink of its own. The caret itself is drawn by the note,
as the box of its line in the note's own layout (`caret_box` in `washi/src/textflow.rs`, the blink in
`washi/ui/text.slint`): a field cuts its caret off at its own right edge, where a line that fills the field
ends, and it takes only a byte offset, which where a line wraps without a space is the start of the next line. While an input method composes, the
field switches to the real face for the moment. Made with
`python tools/fonts.py blank fonts/ShadowsIntoLight.ttf fonts/WashiCaret.ttf "Washi Caret"` (OFL 1.1, renamed as
a derivative).

### Toolbar glyphs

The glyphs are a Times-like serif (the mockup's source used a system serif). The real Times New Roman scores
B 0.931, I 0.929, U 0.857, ab 0.873 in the same harness, so these roles are near the ceiling the method allows.
Times New Roman cannot be shipped, so the top-scoring OFL faces were chosen per role. PT Serif Regular scored
0.874 for U but carries a Reserved Font Name that a subset would have to drop; Noto Serif Regular (0.869) was
used instead. The underline of U and the strike-through of ab are drawn by the app, so those rows are ignored in
the score (for the reference and the candidate alike). The toolbar icons for the list and the image are not fonts.

## Candidates

All OFL 1.1. Shortlist from the plan: Shadows Into Light, Shadows Into Light Two, Reenie Beanie, Indie Flower
(thin); Gochi Hand, Kalam 700, Patrick Hand, Caveat Brush (bold). Also scored: Kalam Regular, the derived Shadows
faces, and for the serif glyphs Tinos (a Times clone), Crimson Text, PT Serif, Cardo, Gentium Book Plus, Old
Standard TT, Noto Serif, STIX Two Text and Gelasio. Full results (IoU, best first; every role is scored against
every non-italic candidate, glyph roles only against their face kind):

| Candidate | title | heading | body |
|---|---|---|---|
| Shadows Into Light | **0.891** | 0.596 | **0.825** |
| Shadows Into Light, emboldened | 0.588 | **0.914** | 0.582 |
| Shadows Into Light Two | 0.244 | 0.392 | 0.337 |
| Caveat Brush | 0.221 | 0.404 | 0.204 |
| Patrick Hand | 0.200 | 0.361 | 0.226 |
| Gochi Hand | 0.207 | 0.261 | 0.184 |
| Kalam 700 | 0.179 | 0.324 | 0.193 |
| Kalam 400 | 0.164 | 0.302 | 0.164 |
| Reenie Beanie | 0.173 | 0.207 | 0.152 |
| Indie Flower | 0.138 | 0.210 | 0.162 |

| Candidate | B (bold) | I (italic) | U | ab |
|---|---|---|---|---|
| Gelasio | **0.915** | 0.777 | 0.853 | **0.853** |
| STIX Two Text | 0.907 | 0.848 | 0.725 | 0.751 |
| Noto Serif | 0.811 | **0.860** | 0.869 | 0.678 |
| PT Serif | 0.844 | 0.856 | 0.874 | 0.756 |
| Gentium Book Plus | 0.854 | 0.750 | 0.829 | 0.745 |
| Tinos | 0.840 | 0.832 | 0.747 | 0.737 |
| Crimson Text | 0.818 | 0.757 | 0.787 | 0.727 |
| Old Standard TT | 0.741 | 0.809 | 0.767 | 0.649 |
| Cardo | 0.791 | 0.748 | 0.462 | 0.640 |

The candidates are not committed (only the winners are): `fetch` downloads them, `subset` subsets them, and the
derived Shadows faces are made with `embolden` and `slant` from the subset original (commands below). Scores of
the top three per role are from a finer search than the rest, so lower ranks are slightly pessimistic.

## Method

1. **Reference.** The hover mockup region is leveled (resampled back by the measured 1.45 degree text tilt, 1.5 for
   the toolbar), then thresholded to an ink mask where coverage reaches 0.5. Multi-line roles are cut into lines
   at the emptiest row between the measured baselines.
2. **Candidate.** One line (or glyph) rendered black on white with the Slint software renderer at scale factor 1
   (mockup px = renderer px), thresholded at 0.5.
3. **Size.** First scaled so the ink heights are equal (the "measured ink height"), then the font size is searched
   +-8 %.
4. **Alignment.** The candidate is moved over the reference: the ink bounding boxes are centered, then +-5 px, then a
   sub-pixel phase grid (1/8 px, 1/16 px for the front runners), letter spacing +-0.5 px, and a +-0.15 degree tilt
   per line. These absorb the browser's sub-pixel placement. For handwriting, letter spacing came out at 0 (the
   advances agree) and the tilt offsets are within 0.15 degrees.
5. **Score.** IoU of the two masks; a role's score is the ink-weighted mean of its lines.

Fonts are registered with the renderer one file per family name (the file stem), weight 400, so a candidate is
always chosen by file. Bold and italic are baked into the files.

## Files

* `fonts/*.ttf`: the winners, subset to Latin (U+0020-007E), Latin-1 (U+00A0-00FF) and general punctuation
  (U+2010-2027, U+2030-203A), plus the euro, trademark and minus signs; hinting dropped. Each under 80 KB.
  Characters outside the subset fall back to system fonts.
* `fonts/OFL-*.txt`: the license texts (SIL OFL 1.1) of the four families. The derived Shadows faces are covered
  by the Shadows Into Light license (no Reserved Font Name); Gelasio and Noto Serif are subsets of static
  instances cut from the variable fonts.
* `tools/fonts.py`: fetch, subset, embolden, slant.
* `tools/uidiff/tests/font_match.rs`: regression on the committed fonts (winners and score floors).

Commands:

```
python tools/fonts.py fetch                                   # raw candidates -> target/font-raw
python tools/fonts.py subset target/font-raw target/font-candidates
python tools/fonts.py embolden target/font-candidates/ShadowsIntoLight.ttf target/font-candidates/ShadowsIntoLight-Bold.ttf 0.0416667 "Shadows Into Light"
python tools/fonts.py slant target/font-candidates/ShadowsIntoLight.ttf target/font-candidates/ShadowsIntoLight-Italic.ttf 0.25 "Shadows Into Light" plain
python tools/fonts.py slant target/font-candidates/ShadowsIntoLight-Bold.ttf target/font-candidates/ShadowsIntoLight-BoldItalic.ttf 0.25 "Shadows Into Light" bold
cargo run --release --manifest-path tools/uidiff/Cargo.toml -- --font-match --fonts target/font-candidates
cargo run --release --manifest-path tools/uidiff/Cargo.toml -- --font-match               # committed fonts/
```

Needs `pip install fonttools skia-pathops`.
