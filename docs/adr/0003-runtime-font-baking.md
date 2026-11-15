# 0003: Fonts are baked into static faces at runtime and registered with Slint

Status: accepted (font manager, ticket 01: spikes S1 to S4). Spike code is in `spikes/fonts/` (own lockfile, not part of the workspace; run with `cargo run --release --bin s4_perf -- <font.ttf>` and so on).

## Context

Every Note is laid out by `textflow.rs`, which relies on three properties of the four embedded faces: Regular, Bold, Italic and Bold Italic share identical advances; there is no kerning or ligature shaping; and a blank Caret face with those advances exists for the edit fields. An arbitrary installed font (often a variable font) breaks all three. The font manager (see `.scratch/font-manager/spec.md`) therefore makes a Face set of five static TrueType faces from the Font in use and registers it with Slint at runtime. Four questions gated that plan; each was answered with a run, on Windows 11, Slint 1.18.1, software renderer, winit 0.30.13. Test fonts: Bahnschrift (variable, `wght` 300 to 700, `wdth` 75 to 100, 961 glyphs), Segoe UI (static, 5,395 glyphs), Arial (static, 4,652 glyphs). No Roboto Flex or Open Sans was on the machine; Bahnschrift is the closest in size.

## Decision

### S1: runtime registration works (yes)

`slint::fontique_011::shared_collection().register_fonts(Blob, Some(FontInfoOverride { family_name, weight, style, .. }))` (Cargo feature `unstable-fontique-011`) works under the software renderer, in `Text` and `TextInput`:

- Weight and style overrides pick the right face. With Segoe UI regular, bold and italic registered under one private family, `font-weight: 400 / 700` and `font-italic: true` drew three distinct renders, each as wide as that file's advances predict (ink 375 / 403 / 370 px against 381 / 410 / 373 px of advance at 40 px). Same in `TextInput`.
- Changing the family string on a live window re-shapes both `Text` and `TextInput`.
- **A window does not notice a font registered after the family string was set.** Setting `font-family: "X"`, drawing, registering "X", and drawing again changed nothing until the property was set to a different value. So: register first, then set the family; and give every bake a fresh family name (`Washi Text <n>`), which the plan already does. Never rely on re-registering under an existing name (it did change the render here, but that is incidental).
- `unregister_font(family_id, width, style, weight)` works; text in that family then falls back.
- Slint 1.18 with `std` has system fonts on: an unknown family falls back to the system default (Segoe UI here), not to the embedded Shadows Into Light. Not a problem for the Face set (its family always exists), but a test that wants to prove "the registered face is used" must register a font that is not the system fallback.
- The registration call takes the blob by `Arc<Vec<u8>>`; the five blobs are about 115 KB each for a 1,000-glyph font.

No fallback (per-window `register_font_from_memory`, files on disk) is needed. The risk that the feature is unstable stays: keep the calls in `fonts/registry.rs` only, and recheck after a Slint upgrade.

### S2: baking with skrifa and write-fonts works (yes)

`skrifa` 0.44 (outlines at a variation location through `OutlinePen`, advances through `glyph_metrics`, which applies HVAR) plus `write-fonts` 0.50 (`GlyfLocaBuilder`, `Head`, `Hhea`, `Hmtx`, `Maxp`, `Cmap::from_mappings`, `Name`, `Os2`, `Post` v3, `FontBuilder`) produce static faces with these tables only: `OS/2 cmap glyf head hhea hmtx loca maxp name post`. No GSUB, GPOS or kern.

- ttf-parser 0.25 and Slint both accept all five faces of every case tried: Bahnschrift at wght 600 / wdth 85 and at its defaults, Segoe UI sheared 8 degrees with the static Bold file as the bold source, and Segoe UI with double-strike bold.
- Advances: for all 961 glyphs, the baked `hmtx` advance equals ttf-parser's variable-font advance at the same location, 0 units apart. Bold, Italic, Bold Italic and Caret advances equal Regular's for every glyph (0 mismatches in all four cases). Covered by `spikes/fonts/tests/bake.rs`.
- Slint draws all four styles distinct; the Caret face draws 0 ink pixels in `Text` and `TextInput`.
- Details that mattered:
  - Composites are flattened by drawing through skrifa; the pen only needs `move/line/quad/close`. A cubic (CFF) outline is refused (P5).
  - A glyph's `hmtx` side bearing must be its `x_min` (0 when empty); the baker sets it from the glyph box.
  - Bold made from the weight axis is shifted by half the advance difference so it stays centred in Regular's advance. The same is done for a Bold file's outlines (glyph ids differ between files: map through the cmap).
  - `OS/2` weight class is passed through (Bahnschrift wght 600 plus 300 gave 900; skrifa clamped the outlines at the axis maximum 700). Register Bold with `FontWeight::new(700.0)` regardless of that value, since the CSS weight Slint asks for is 700.
  - Double-strike bold (+-em/64) adds 29% ink over Regular at 40 px on Segoe UI (3,944 against 3,058 dark pixels), against 27% for a weight-axis bold on Bahnschrift and 70% for Segoe UI's real Bold file (5,155): a subtle bold, acceptable as the last resort. It doubles the bold glyf size (2.4 MB for Segoe UI).
  - Slant on a font without a `slnt` axis is a shear `x += tan(deg) * y`; Italic adds a further 0.25.
- `write_fonts::read` is behind its `read` feature, which Washi does not need: take `CurvePoint` from `skrifa::raw::tables::glyf`. `ttf-parser` needs the `variable-fonts` feature only to cross-check in tests.

### S3: dropped files reach Rust through winit's event (yes)

`slint::winit_030::WinitWindowAccessor::on_winit_window_event` (Cargo feature `unstable-winit-030`) receives `WindowEvent::DroppedFile(PathBuf)` and `HoveredFile` / `HoveredFileCancelled` for a Slint window. winit registers its own OLE drop target when the window is created (found as the `OleDropTargetInterface` property), so no Win32 fallback (`DragAcceptFiles`, subclass) is needed. Drop mechanism for ticket 08: **the winit event**; the Win32 fallback is not to be written.

Limits of the check: the drag was simulated in-process by calling the window's drop target (DragEnter, DragOver, Drop) with a hand-made data object carrying one `CF_HDROP`, because the run had no pointer to drag with. It proves the Slint to winit to Rust path, not Explorer's drag loop; a person has to drag a real `.zip` onto the real Settings window once (QA checklist, ticket 10), including with the Settings window's own window styles. Notes: the handler fires for hover as well, so the slip can light up its drop zone on `HoveredFile`; the callback returns `EventResult::Propagate`; it runs on the UI thread, so reading the archive must move off it (spec).

### S4: speed and size (yes, with a limit)

Release build (`opt-level = "s"`, fat LTO, as the product), median of 11 runs, all five faces, one thread:

| font | glyphs | axis bold | double-strike bold | Regular only |
|---|---|---|---|---|
| Bahnschrift (variable) | 961 | 11.1 ms | 14.3 ms | 3.8 ms |
| Arial (static) | 4,652 | 61.6 ms | 74.2 ms | 17.1 ms |
| Segoe UI (static) | 5,395 | 72.5 ms | 121.7 ms | 23.5 ms |

The pass condition (a font of Roboto Flex's size, about 1,000 glyphs, under 60 ms) is met with a wide margin (about 11 ms). Fonts of 4,500 glyphs and more take 60 to 120 ms, so the plan's own fallback is adopted for them: apply Regular first (the live-drag path is Regular only, 4 to 24 ms) and make the other four faces on the next loop turn. Cheap further cuts if needed: bake only glyphs the cmap reaches, and skip glyphs above U+2FFF. Slider dragging (one Regular bake per 30 ms) is fine up to about 1,000 glyphs (3.8 ms) and borderline for 5,000 (24 ms).

Binary: the same Slint window program with and without the baker, the Slint registration call and a zip reader (`zip` 2 with `default-features = false, features = ["deflate"]`) is 7,039,488 against 7,385,088 bytes: **+345,600 bytes (about 0.34 MB)**, against a 12.96 to 13.59 MB release `washi.exe` in `docs/PERF.md` (+2.6%). Startup with the Default font is unchanged by construction, not by measurement: P6 keeps the bundled faces when the Default font has default settings, so no bake, no registration and no new code runs on that path; the dependency code is only linked in. Re-measure with `tools/memprobe.ps1` once ticket 03 lands. Memory: one Face set is about 0.5 MB for a 1,000-glyph font, 4.5 MB for a 5,000-glyph one (double-strike), held only for the font in use.

## New dependencies and unstable features

- `skrifa` 0.44 and `write-fonts` 0.50 (`read-fonts` 0.41 and `font-types` are already in `Cargo.lock` through Slint; `write-fonts` adds `kurbo`, `indexmap`, `log`). Direct dependencies of `washi`.
- `zip` 2 with `default-features = false, features = ["deflate"]` (ticket 05). It brought `zopfli`, `arbitrary` and a second `bitflags` into the spike lockfile; check `cargo tree` when adding it and keep `deflate-zopfli` and other write features off.
- Slint features `unstable-fontique-011` (S1) and `unstable-winit-030` (S3), both on the `slint` dependency of `washi`. Pinned by `Cargo.lock`. `fontique` 0.11.1 is reached only through `slint::fontique_011::fontique` (no direct dependency).
- `ttf-parser` is already a dependency; no new feature is needed in the product.

## Consequences

- The plan in section 4 stands as written; the order of registration and the fresh family per bake are now requirements, not choices.
- Ticket 03 must set the family string after registering the Face set, on every window (and popups at creation).
- Ticket 08 listens to `DroppedFile` and `HoveredFile` through `on_winit_window_event` on the Settings window; the Win32 route is dropped from the plan.
- Fonts over about 4,000 glyphs apply Regular first and the rest one loop turn later.
- Tests that prove a registered face is used must not use the system default font (Segoe UI on Windows) as the registered font.
- A Slint upgrade must rerun `spikes/fonts` (`s1_register`, `s2_bake`, `s3_drop`) before the lock file moves.
- Startup with the Default font and the Face set times were re-measured on the finished feature (ticket 10): see "Font manager" in `docs/PERF.md`. Startup is unchanged; Arial and Segoe UI still take 66 to 88 ms to bake, and the "Regular first" fallback above was not built.
