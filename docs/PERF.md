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
