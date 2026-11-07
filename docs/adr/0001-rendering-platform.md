# 0001: Rendering platform: winit + Slint software renderer, transparent window

Status: accepted (Phase 0 spikes, ticket 01). Spike code: `spikes/` (throwaway, not part of the product).

## Decision

1. **Backend: winit + Slint software renderer (softbuffer).** The custom layered-window platform (ticket 04, IMPLEMENTATION_PLAN §3.3 Option A) is **not needed**. Per-pixel alpha works on Windows 11 with the stock winit backend.
2. **Buffer release: rely on minimize.** The winit backend frees its framebuffer when the window is minimized (size 0), so no custom release code is needed. `Window::hide()` does not free anything and also drops the taskbar button, so it is not used.
3. **Paper: a live `Path`** computed from bound size and the constant 1.5° tilt. **Shadow: 4 stacked translucent `Path` layers, 2 px growth each**, not 8 x 1 px. The cached-image shadow is the pre-approved fallback if resize pacing proves poor in QA.
4. **Hover: a parent `TouchArea`'s `has-hover` is enough.** No Rust-side fallback.
5. **Accessibility: on by default** (`accessibility` cargo feature), re-checked once a screen reader is exercised.
6. **SQLCipher: `bundled-sqlcipher-vendored-openssl` works** once Strawberry Perl is first on PATH. Cold release build 5 min 24 s; spike exe 4.01 MB (see below).

## Method

Machine: Windows 11 Pro 10.0.26200, 100% system scale, MSVC 14.44, rustc 1.99, Slint 1.18.1 with `backend-winit`, `renderer-software`, `software-renderer-systemfonts`. Release profile as in plan §3.1 (`opt-level=s`, fat LTO, `panic=abort`, strip). 150% DPI is emulated (`SLINT_SCALE_FACTOR=1.5` for winit, `--scale 1.5` for the layered prototype), which gives the same framebuffer size as a real 150% display.

Memory is read with `spikes/tools/memprobe.ps1` after a 10 s idle. **Private working set** is `WorkingSetPrivate` (what Task Manager's "Memory (active private working set)" shows); commit is private bytes. Total working set is higher because it counts shared DLL pages; it is not the goal metric. The spike windows draw the paper UI (or nothing, for "bare") at 431 x 451 logical (371 x 391 paper plus a 30 px margin). No SQLite, no embedded fonts, no images, so add the real app's own cost on top.

## Measurements

### Transparency (`spikes/tools/transparency_probe.ps1`)

A topmost striped reference window sits behind the spike window; the screen is captured (with `CAPTUREBLT`) before and after.

| Probe | Expected | winit + softbuffer | layered prototype |
|---|---|---|---|
| empty margin pixel | equals backdrop (218,165,32) | (218,165,32) | (218,165,32) |
| 50% red square over backdrop | (236,82,16) | (237,82,16) | (237,82,16) |
| paper | white | (255,255,255) | (255,255,255) |
| shadow below paper | backdrop darkened | (39,72,100) | (40,74,103) |

Both backends give correct per-pixel alpha. Not tested: whether fully transparent margin pixels are click-through (probably not, on either backend); that matters for the §3.2 window shell and belongs to that ticket.

### Memory (private working set / commit, MB, idle)

| Scenario | 1 window | 3 windows |
|---|---|---|
| winit + software, bare window, 100% | 2.85 / 3.43 | 4.34 / 4.93 |
| winit + software, paper UI, 100% | 3.11 / 3.93 | 4.94 / 5.78 |
| winit + software, paper UI, **150%** | 4.59 / 5.41 | **7.59-7.95 / 8.4-8.6** |
| layered + Slint, bare, 100% | 2.00 / 2.46 | 3.50 / 3.96 |
| layered + Slint, paper UI, 100% | 2.38 / 2.90 | 3.93 / 4.52 |
| layered + Slint, paper UI, **150%** | 3.11 / 4.11 | **6.55 / 7.11** |
| layered, no Slint (raw DIB), 100% | 1.91 / 2.36 | 3.39 / 3.86 |
| layered, no Slint (raw DIB), 150% | 2.83 / 3.29 | 6.16 / 6.64 |
| Windows Sticky Notes 6.1.4.0 (home window only, no notes opened) | 19.2 / 81.5 | not measured |

G1 (≤ 15 MB, stretch 12 MB with 3 notes): both backends pass with room (winit about 8 MB, layered about 6.5 MB at 150%). The layered path saves about 1 MB for 3 notes, which does not justify 1-1.5k lines of Win32 code. Sticky Notes with 3 notes could not be measured without driving its UI; its idle home window alone is 19 MB, so Washi is well under it.

### Buffer release while minimized (3 notes, 150%, measured 16 s after minimizing; private WS / commit, MB)

| Scenario | Before | After minimize |
|---|---|---|
| winit, `set_minimized(true)` | 7.95 / 8.60 | **2.50 / 3.16** |
| winit, `hide()` | 7.98 / 8.64 | 7.98 / 8.64 (no change) |
| layered, minimize, keep DIB | 6.98 / 7.58 | 6.98 / 7.58 (no change) |
| layered, minimize, free DIB | 6.55 / 7.12 | **1.55 / 2.11** |
| layered raw, free DIB | 6.18 / 6.66 | 1.17 / 1.66 |

The winit drop is commit as well as working set (about 5.4 MB for 3 notes, one 1.75 MB buffer each), so it is a real free, not just a working-set trim. G1s (≤ 5 MB with 3 suspended notes) is met by both: 2.5 MB (winit) and 1.55 MB (layered). The spike does not yet suspend Slint's own caches; the plan's §7.1 suspension work still applies.

### Hover through children (`spikes/tools/hover_probe.ps1`)

With the real cursor moved over the window, the parent `TouchArea.has-hover` goes true when the pointer is over the child `TextInput`, stays true while moving within it, and goes false on leaving the window. (One brief true then false was logged when the cursor first landed on the window's margin edge; harmless, but re-check with the real toolbar.)

### Tilted paper and shadow (`spikes/slint/src/bin/render_bench.rs`, headless software renderer)

Edge quality of the 1.5° rounded-rect `Path` is acceptable at 100% and 150%: smooth anti-aliased edges and corners, no visible stair-stepping (inspected over a striped backdrop). Live-resize cost is a full repaint every frame (400 frames, window growing and shrinking between 431 x 451 and about 831 x 801 logical):

| Shadow approach | 100% mean / p95 (ms) | 150% mean / p95 (ms) |
|---|---|---|
| 8 stacked paths x 1 px | 11.1 / 18.9 | 25.4 / 45.0 |
| **4 stacked paths x 2 px** | 6.3 / 10.8 | 15.0 / 26.2 |
| 3 stacked paths x 3 px | 5.0 / 8.7 | 12.9 / 23.6 |
| no shadow (paper only) | 1.6 / 2.7 | 3.7 / 6.5 |
| cached shadow image (fallback) | 2.4 / 4.2 | 6.1 / 10.0 |

The paper `Path` itself is cheap; the stacked shadow dominates. 8 layers misses a 16.7 ms frame at 150%. Idle repaints are partial (caret blink), so this cost only applies to first paint and live resize. Whether the 4 x 2 px shadow matches the mockup ring was not checked (no `uidiff` yet); that comparison belongs to the paper ticket, which may move to 3 x 3 px or the cached image.

### Accessibility feature cost

| | exe | 3 notes, 150%: private WS / commit |
|---|---|---|
| without | 7.03 MB | 7.95 / 8.58 |
| with `accessibility` | 7.27 MB (+0.24 MB) | 7.83 / 8.68 (no meaningful change) |

Total working set rose (22.9 to 30.4 MB for 3 notes), but that is shared pages, not the goal metric. The cost is under the plan's 1 MB threshold, so it is on by default. A screen reader attaching will add more; unmeasured.

### Binary size and build time

| | exe (release) |
|---|---|
| winit + Slint software spike | 7.03 MB |
| layered prototype, no winit | 5.49 MB |

Clean build of the winit spike: about 97 s without accessibility, 95 s with. These sizes include the spike's extras (system fonts, `usvg`/`png` pulled in by Slint features) and are an upper bound for the product.

### SQLCipher (measured in ticket 05)

`rusqlite 0.32` with `bundled-sqlcipher-vendored-openssl` (`spikes/sqlcipher/`). With Git's Cygwin Perl first on PATH the build fails, because `openssl-src`'s `Configure` rejects it ("This Perl version ... for x86_64-cygwin"). With **Strawberry Perl (`C:\Strawberry\perl\bin`) first on PATH it builds** from a normal shell; no NASM, so OpenSSL is built `no-asm`.

| | Result |
|---|---|
| Clean release build (cold `target/`, all deps, vendored OpenSSL) | 5 min 24 s |
| Incremental rebuild of the spike after a source edit | about 2 s |
| `spike-sqlcipher.exe` (release, same profile) | 4,105,216 bytes (3.91 MiB) |
| SQLCipher version | 4.5.7 community |
| File contains the plaintext marker | no |
| File starts with `SQLite format 3` | no |
| Opening without the key fails | yes |

So SQLCipher adds about 4 MB to the exe (the spike has nothing else), within G5 together with the 7 MB Slint spike if sizes stack, and the build cost is a one-time 5 minutes. A fix to the spike was needed: `cipher_version` is a pragma, not a SQL function.

## Consequences

- Ticket 04 (custom layered-window platform) is dropped. The prototype stays in `spikes/slint/src/bin/layered_win.rs` as a verified fallback (alpha, mouse, free-on-minimize work; no keyboard, IME, DPI-change or cursor code).
- The window shell uses winit APIs: `drag_window`, `drag_resize_window`, min size, `set_minimized`.
- The note component draws paper and shadow from properties; `spikes/slint/ui/spike.slint` has a working formulation of the tilted rounded-rect path.
- Open follow-ups: click-through of transparent margins; wake time after restore (G4w); real fonts and SQLite memory; shadow fidelity against the mockup.

## Reproduce

```
cd spikes/slint
cargo build --release --features "winit bench"          # winit_win, render_bench
cargo build --release                                    # layered_win only (no winit)
target/release/render_bench.exe <outdir>                 # paper PNGs + live-resize timings
powershell -File ../tools/transparency_probe.ps1 -Exe target/release/winit_win.exe
powershell -File ../tools/hover_probe.ps1 -Exe target/release/winit_win.exe
powershell -File ../tools/memprobe.ps1 -Label x -Exe target/release/winit_win.exe -ExeArgs "--notes 3"
```

Add `-AfterMinimizeSec 16` and `--minimize-after 11` for the suspend scenarios, and set `SLINT_SCALE_FACTOR=1.5` for winit at 150%. Building with `--features winit` unifies features, so build `layered_win` alone when comparing binary size. Build from an environment where `cl` is on PATH (vcvars64).
