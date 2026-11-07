# Performance log

Written by tools/memprobe.ps1. Private WS is what Task Manager shows as active private working set.
Budget: active, 3 notes, private WS <= 15 MB (CI-enforced) and not above Sticky Notes. Suspended 3 notes <= 5 MB (not built yet).

| date | commit | app | notes | state | private WS MB | commit MB | WS MB | vs budget |
|---|---|---|---|---|---|---|---|---|
| 2026-11-07 10:15 | 4b39b9c | washi | 1 | active | 4 | 4.76 | 24.59 |  |
| 2026-11-07 10:15 | 4b39b9c | washi | 3 | active | 5.62 | 6.38 | 26.31 | ok |
