# Performance log

Written by tools/memprobe.ps1. Private WS is what Task Manager shows as active private working set.
Budget: active, 3 notes, private WS <= 15 MB (CI-enforced) and not above Sticky Notes. Suspended 3 notes <= 5 MB (not built yet).

| date | commit | app | notes | state | private WS MB | commit MB | WS MB | vs budget |
|---|---|---|---|---|---|---|---|---|
| 2026-11-07 10:15 | 4b39b9c | washi | 1 | active | 4 | 4.76 | 24.59 |  |
| 2026-11-07 10:15 | 4b39b9c | washi | 3 | active | 5.62 | 6.38 | 26.31 | ok |

## Final pass (ticket 20, commit after 216f76b)

Release build, Windows 11, 100% display scale (a 150% display was not available, so the framebuffers
measured here are about 0.84 MB per note, not 1.9 MB). Tool: `tools/memprobe.ps1 -Suspended -Startup -CpuSec 60`.
"Private WS" is the Task Manager "active private working set". The user reviewed the numbers and accepted the
misses below; no further optimization was done.

| target | measured | verdict |
|---|---|---|
| G1 3 notes active, private WS <= 15 MB | 2.9 MB (after the one-shot trim, see below); 12.0 MB with `-NoTrim`. Commit 13.3 MB | met |
| G1s 3 notes suspended <= 5 MB | 0.3 - 0.9 MB (1 and 10 notes: 0.3 - 0.9 MB) | met |
| G2 <= 2 MB per extra note | trimmed: about 0 MB (idle pages are trimmed); untrimmed `-NoTrim`: 2.5 - 2.6 MB (1 to 10 notes 6.7 -> 28 MB); commit 2.7 MB | missed untrimmed, accepted |
| G3 startup < 150 ms | median 147 ms (140 - 165 ms) from process start until all 3 note windows exist; the first frame follows on the next loop turn | borderline, accepted |
| G4 idle CPU 0%, suspended | 0 ms in 60 s | met |
| G4 idle CPU 0%, foreground | 1.17 s of CPU in 60 s (about 2% of one core: the focused text caret blinks) | missed, accepted |
| G4 idle CPU, background and visible | 0.2 - 0.47 s in 60 s (the 5 s cover check) | accepted |
| G4w wake <= 50 ms | 9 - 18 ms (restore to the next event-loop turn after waking) | met |
| G5 exe <= 8 MB, no DLLs | 12.96 MB, no DLLs. Not cut (product decision) | missed, accepted |

Logged changes (none from the size cut list):

| # | change | effect |
|---|---|---|
| 1 | Note windows are attached to the platform signals once Slint has created them (they did not exist at open time). Before this the real app never suspended | suspension works: 12 MB -> 0.3 - 0.9 MB |
| 2 | One-shot `HeapCompact` + `EmptyWorkingSet` 1.5 s after the last note opened, never while suspended (`WASHI_NO_TRIM=1` / `-NoTrim` disables it) | active 3 notes 12.0 -> 2.9 MB private WS (commit unchanged). Pages needed later fault back in |
| 3 | Focus and foreground changes no longer restart the 10 s suspend grace | suspends 10 s after a minimize, not up to 15 s |
| 4 | Cover check rejects hidden, minimized and non-overlapping windows before the DWM query | cheaper background tick |

Not done: live capture at other display scales (display here is 100%), 150% sign-off.
Washi is single-instance per user: memprobe waits while another washi.exe runs, otherwise launches hand off.

Known measurement caveat: `-StickyNotes` was not run (not installed or not driven here).

## Ticket 20 pass (commit after 216f76b, release build, this machine: 100% display scale, 4 monitors)

Measured with `tools/memprobe.ps1 -Suspended -Startup -CpuSec 60`. Private WS = Task Manager "active private working set".

| measurement | result | target |
|---|---|---|
| active, 1 / 3 / 10 notes (after the one-shot trim) | 2.7 / 2.9 / 3.0 MB private WS (commit 7.8 / 13.2 / 30.7 MB) | 3 notes <= 15 MB |
| active, untrimmed (`-NoTrim`, heap as allocated) | 6.7 / 12.1 / 28.0 MB | 3 notes <= 15 MB, stretch 12 |
| per extra note, untrimmed / trimmed | about 2.3 MB / about 0.03 MB (commit grows about 2.5 MB per note) | <= 2 MB |
| suspended, 1 / 3 / 10 notes | 0.64 / 0.29 / 2.1 MB | 3 notes <= 5 MB |
| startup: process start to all 3 windows shown | 136-184 ms, median about 142 ms (first frame is drawn on the next loop turn, not measured) | < 150 ms (marginal) |
| wake: restore to the next event-loop turn after waking | 7-15 ms (does not include the DWM present) | <= 50 ms |
| idle CPU 60 s, 3 notes, suspended | 0 ms | 0 |
| idle CPU 60 s, 3 notes, active | 0.5-1.3 % of a core, background or foreground (focused note's caret blink, cover check). Not 0 | 0 |
| release exe | 13.59 MB, no DLLs | <= 8 MB: NOT MET |

Cuts and findings, in order:

1. The real app never suspended: note HWNDs did not exist when the notes were opened, so no minimize was ever seen. Fixed (attach after the event loop creates the windows; windows appear about 110 ms after the run loop starts).
2. Focus or foreground changes during the grace period restarted the 10 s. Now only changes in minimized/covered state do (test added).
3. One-shot compact + working-set trim 1.5 s after the last note opens (not in the planned cut order; it is what takes active memory from 12.1 to 2.9 MB private WS). Pages needed while idle fault back in on use, so a note being used will read higher; commit is unchanged. `WASHI_NO_TRIM` / `-NoTrim` measures without it. The product owner should decide whether this counts for G1.
4. Cover check no longer queries DWM for windows that do not overlap the note.
5. Planned order (font scan, buffer type, layered platform, accessibility, SQLite cache): not needed for G1/G1s. The buffer type is chosen inside Slint's winit backend and cannot be changed without forking it.

Binary size (G5, not met, product decision: measured only): `.text` 7.3 MB and `.rdata` 5.3 MB. About 3 MB of `.rdata` is ICU data pulled by Slint's `std` feature (parley/icu_segmenter), about 1 MB is OpenSSL for SQLCipher. Experiments: `opt-level = "z"` 13.07 MB; plus no `accessibility` 12.88 MB. Reaching 8 MB needs a fork or patch of Slint/parley.

Not done: Sticky Notes side-by-side (`-StickyNotes`), live capture at 150% (this machine is 100% and display scale was not changed).

## Settings transition (ticket 02 of the Settings page)

The spec said "about 300 ms"; after the real app felt delayed and choppy the user asked for quick and snappy,
so the numbers below replace it.

Cause of "choppy and delayed", found by reading the code (not by a real-display measurement):

1. **Timer grid.** The frame timer was a 16 ms Slint timer. On Windows timers fire on a ~15.6 ms grid unless
   a finer resolution is requested (nothing in the app called timeBeginPeriod), so a 16 ms timer fires every
   ~31 ms (about 30 fps, 6-8 frames per transition), and as Slint timers run after rendering, frames also slip.
2. **Sequential and long.** Open was a 90 ms text fade, a swap frame, then a 240 ms grow (about 350 ms before
   the page was usable); close ended with another 90 ms fade; a tab switch was 200 ms in-out cubic (slow at the
   start, so it felt late).
3. The per-frame cost (one SetWindowPos of a large layered window, relayout and software repaint of the page and
   its paper shadow) is real but was not the first-order problem and is unchanged.

Now: Settings shows at once on the note's Paper (no pre-fade; the note's text sits hidden beneath the opaque
Paper), grows over **190 ms** ease-out (at rest about 200 ms after the press); close is 190 ms plus a 40 ms text
fade-in (about 250 ms); a tab switch is **130 ms** ease-out (height tween and the out-then-in content fade run
together). The frame timer is **8 ms** and holds the 1 ms system timer resolution (timeBeginPeriod and
timeEndPeriod, paired) only while a transition runs, so it is released at rest. The frame work is
SettingsView::step, bracketed by the `settings-frame` trace mark (WASHI_TRACE).

Fixed-large-window fallback (window at its final bounds, Paper animating inside it): NOT built. Washi uses the
software renderer (ADR 0001), so there is no GPU or vsync compositing to gain; a fixed transparent window would
make every frame repaint and present the whole large layered surface and needs a window shape to keep the empty
areas click-through. Reconsider only if the measured frame cost on a real display is still too high.

Regression check against main (94ae630): this branch adds no timer or work to the other animations. `Hooks::now`
is just the clock's monotonic read, and the Settings timers (frame timer, 1 Hz stats) exist only while Settings is
open (stats only on General). The one shared change is note.slint: the note's text, scrollbar, toolbar and menu
button now multiply by `content-opacity` (always 1 outside the Settings transition). Whether the software renderer
treats an opacity bound to exactly 1 as free (no layer) was NOT measured; if the dropdown or All notes still feel
slower than on main, look there first.

Measured vs not: **frame cost and smoothness on a real display are NOT measured** (headless harness only).
Verified in tests: open at rest within 230 ms and close within 260 ms landing exactly on the host's geometry, tab
height tween ease-out and done within 140 ms, Settings shown from the first instant (overlap, not sequence), the
timer resolution hold is balanced and the frame timer stops at rest, tweens are wall-clock based. To close this
out: run a release build with WASHI_TRACE set, open and close Settings and switch tabs at 100% and 150% display
scale, and record the worst `settings-frame` interval here.

## Settings General tab: 1 Hz stats timer

While (and only while) the General tab is showing, a 1 s repeating timer refreshes Resource use. Each
tick reads process stats (private working set via GetProcessMemoryInfo, CPU time via GetProcessTimes),
the database file metadata (size of the database files, no query), the open-note count, and the Start
with Windows registry value. It stops when another tab is shown or Settings closes, so it adds nothing
to the closed-Settings idle figures above.

Against goal G4 (idle CPU 0%): the cost is a few cheap syscalls and a handful of property sets per
second, but it is not zero, and the repainting window wakes the event loop every tick. That is accepted
while General is on screen (a deliberate, user-opened view); the CPU figure it shows includes its own
cost. Idle CPU of Washi with Settings open on General has NOT been measured, and the transition frame
cost (above) is still NOT yet measured.


## Layout cache

Baseline before the layout cache work (.scratch/layout-cache, ticket 01). Commit 992e711 (plus the wrap counter and `layout_bench`), release build (`cargo run --release -p snap --bin layout_bench`, 200 inputs per cell), Windows 11, Intel i5-13600K, 32 GB. Headless: this is layout and the app's reaction to one input, not Slint's repaint. `wraps` is `textflow::wrap` calls per input (one per text block per whole-note wrap); the same counts are pinned in `tools/snap/tests/layout_wraps.rs`. The pointer row runs on the fixture plus a pasted 300x150 picture, selected.

| note | operation | median ms | p95 ms | wraps |
|---|---|---|---|---|
| mockup.md (3 blocks) | typed character | 0.167 | 0.251 | 36 |
| mockup.md | Down arrow | 0.079 | 0.087 | 24 |
| mockup.md | pointer over image | 0.028 | 0.035 | 12 |
| mockup.md | 10 px resize | 0.053 | 0.056 | 10 |
| long.md | typed character | 1.251 | 1.428 | 156 |
| long.md | Down arrow | 0.405 | 0.474 | 96 |
| long.md | pointer over image | 0.169 | 0.189 | 48 |
| long.md | 10 px resize | 0.516 | 0.624 | 46 |
| 50 paragraphs | typed character | 9.155 | 10.380 | 506 |
| 50 paragraphs | Down arrow | 2.433 | 2.710 | 306 |
| 50 paragraphs | pointer over image | 1.102 | 1.513 | 153 |
| 50 paragraphs | 10 px resize | 7.029 | 8.365 | 151 |
### After ticket 03 (editor owns one cached `Layout`)

Same machine, benchmark and method as the baseline above, release build. The editor keeps one `Layout` keyed by document revision, paper size, tokens and picture sizes, so one input costs at most one build (one `wrap` per text block: mockup.md 3, long.md 15, 50 paragraphs 50) and caret motion and the pointer cost none.

| note | operation | median ms | p95 ms | wraps | before (median ms / wraps) |
|---|---|---|---|---|---|
| mockup.md (3 blocks) | typed character | 0.119 | 0.148 | 3 | 0.167 / 36 |
| mockup.md | Down arrow | 0.053 | 0.069 | 0 | 0.079 / 24 |
| mockup.md | pointer over image | 0.015 | 0.016 | 0 | 0.028 / 12 |
| mockup.md | 10 px resize | 0.048 | 0.050 | 3 | 0.053 / 10 |
| long.md | typed character | 0.870 | 1.006 | 15 | 1.251 / 156 |
| long.md | Down arrow | 0.141 | 0.170 | 0 | 0.405 / 96 |
| long.md | pointer over image | 0.046 | 0.047 | 0 | 0.169 / 48 |
| long.md | 10 px resize | 0.470 | 0.579 | 15 | 0.516 / 46 |
| 50 paragraphs | typed character | 6.627 | 7.432 | 50 | 9.155 / 506 |
| 50 paragraphs | Down arrow | 0.679 | 0.830 | 0 | 2.433 / 306 |
| 50 paragraphs | pointer over image | 0.237 | 0.275 | 0 | 1.102 / 153 |
| 50 paragraphs | 10 px resize | 6.473 | 7.113 | 50 | 7.029 / 151 |

What is left of a keystroke is the single build (still wraps every block; ticket 05 would cut that to the edited block) plus the draw-list rows.


### After ticket 04 (edit-path cleanups)

Same machine and method, release build. A typed character no longer copies the `Document` (only an auto-formatting one does), `apply_change` computes the caret box once, and `sync_model` builds a window row only for the changed block and loads pictures only when the note's set of pictures changed (pinned in `tools/snap/tests/edit_path.rs`). On a 50 paragraph note that is 50 rows built before and 1 now, and no picture reload; the wrap counts are unchanged.

| note | operation | median ms | p95 ms | wraps | after ticket 03 (median ms) |
|---|---|---|---|---|---|
| mockup.md (3 blocks) | typed character | 0.116 | 0.152 | 3 | 0.119 |
| mockup.md | Down arrow | 0.052 | 0.059 | 0 | 0.053 |
| mockup.md | pointer over image | 0.015 | 0.015 | 0 | 0.015 |
| mockup.md | 10 px resize | 0.049 | 0.067 | 3 | 0.048 |
| long.md | typed character | 0.852 | 1.013 | 15 | 0.870 |
| long.md | Down arrow | 0.140 | 0.171 | 0 | 0.141 |
| long.md | pointer over image | 0.045 | 0.051 | 0 | 0.046 |
| long.md | 10 px resize | 0.475 | 0.585 | 15 | 0.470 |
| 50 paragraphs | typed character | 6.700 | 7.418 | 50 | 6.627 |
| 50 paragraphs | Down arrow | 0.674 | 0.824 | 0 | 0.679 |
| 50 paragraphs | pointer over image | 0.236 | 0.285 | 0 | 0.237 |
| 50 paragraphs | 10 px resize | 6.557 | 7.432 | 50 | 6.473 |

The keystroke time did not move: the work removed (a document copy, a second caret box, 49 row builds) is microseconds next to the rest of the keystroke, which this benchmark shows is not the layout either. `layout_bench` now also times one `Layout::build` against the typed character it is part of:

| note | Layout::build median ms | typed character median ms | share |
|---|---|---|---|
| mockup.md | 0.012 | 0.117 | 10% |
| long.md | 0.136 | 0.848 | 16% |
| 50 paragraphs | 0.928 | 6.665 | 14% |

So a wrap cache (ticket 05) could save at most about 10 to 16% of a keystroke here (less, as it would still lay out the edited block).
