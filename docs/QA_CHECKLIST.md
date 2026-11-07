# Manual QA checklist (IMPLEMENTATION_PLAN 9.5, ticket 20)

Everything here needs a person at a real Windows 11 desktop: it covers what the headless harness,
the fake platform and the scripts cannot see. Tick a box only after doing the step on the release
build (`cargo build --release -p washi`, then `target\release\washi.exe`). Boxes are all unchecked on
purpose: nothing below was verified by a person when this file was written.

Tip: use a throwaway profile so your own notes stay untouched:
`$env:WASHI_DATA_DIR = "$env:TEMP\washi-qa"; .\target\release\washi.exe`

## Hover, resize, move, hint

- [ ] Hover: the toolbar fades in over the whole note, including over the text. It fades out when the pointer leaves, also when you leave through the tape area. The note never changes size.
- [ ] Keyboard focus alone (no hover) never shows the toolbar. A pressed toolbar button keeps it visible.
- [ ] Resize cursors: all 8 zones (4 edges, 4 corners, the invisible 8 px border) show the right cursor and resize in that direction (not covered by the headless tests: deferred from ticket 14).
- [ ] Minimum paper size is 360 x 360. Text re-wraps while resizing. The tilt stays 1.5 degrees and the tapes keep their size.
- [ ] Native move: dragging the pink tape and the top margin moves the window (drag the text and the toolbar: no move). Window snapping (Win+Arrow, drag to an edge) behaves.
- [ ] The scrollbar appears on hover only when the text overflows. The wheel scrolls the text, not the paper.
- [ ] Empty note: shows "Note Something...." and loses it on the first keystroke.
- [ ] Text sizes look right with the stepped (tilted) baseline at rest and level while editing (a known, accepted deviation: see Known limitations in `docs/RELEASE_NOTES.md`).

## Lifecycle, taskbar and app identity

- [ ] Each note has its own taskbar button, all grouped under one Washi icon (deferred from ticket 16). The icon is the taped-paper icon in the taskbar, the title bar area, Alt-Tab and the .exe in Explorer.
- [ ] The taskbar button title is the note's first line, or "Washi" when empty, and updates as you type.
- [ ] Single-instance handoff: with Washi running, start `washi.exe` again. A new note opens next to the newest one and the second process exits (check Task Manager: one `washi.exe`). Try also while all notes are minimized and while suspended.
- [ ] Close (Ctrl+W) hides the note without asking. It shows in "Open note >" of the others, newest first. Choosing it reopens it at its saved position and size.
- [ ] Closing the last note quits. Restarting reopens the notes that were open.
- [ ] Delete (Ctrl+D) asks first; both answers behave.
- [ ] Always on top toggles with a check mark, persists across restart. Start with Windows toggles with a check mark, is off by default, and really starts Washi after sign-in.
- [ ] Right-click anywhere on the paper or the tapes opens the native menu.

## Suspension and the efficiency leaf

- [ ] Minimize all notes: after about 10 s Task Manager (Details or Processes tab) shows the efficiency-mode leaf on Washi and its memory column is at most 5 MB (3 notes).
- [ ] Cover all notes with another maximized app: the same happens after about 10 s (deferred from ticket 18).
- [ ] Alt-Tab quickly between Washi and another app: nothing suspends.
- [ ] Restore, hover (while covered partially) or uncover a note: it wakes at once, looks unchanged (no flash, no blank frame), the leaf disappears.
- [ ] Event-driven wake: while suspended, Washi's CPU stays at 0 in Task Manager and restoring still wakes it without any polling timer (deferred from ticket 17).
- [ ] While suspended, typed-into and unsaved edits are not lost (edits are flushed before suspending).

## Input methods and text

- [ ] Japanese (Microsoft IME): composition shows inline in the note, candidates window opens next to the caret, commit inserts the text.
- [ ] Chinese (Pinyin): same.
- [ ] Emoji picker (Win+.) and emoji fallback rendering. Pasting rich text from a browser gives plain text.
- [ ] Clipboard: copy and paste in the text, paste a screenshot (inserts an image paragraph), "Export as Markdown..." writes the .md and PNGs and the .md opens correctly in a Markdown viewer.

## DPI and multi-monitor

- [ ] 100%, 125%, 150%, 200% display scale: the note looks right at each (the geometry is checked by `tools/capture.ps1`; look at the paper edge, tape and fonts).
- [ ] Drag a note between two monitors with different scale: it re-renders at the new scale without a blur or a size jump.
- [ ] A note saved on a monitor that is no longer there reopens on the primary work area.

## Kill and shutdown recovery

- [ ] Kill `washi.exe` in Task Manager while typing. On relaunch at most the last 500 ms of typing is lost and the notes open (no corruption error).
- [ ] Sign out and shut down with unsaved typing: after signing in again the text is there.
- [ ] Copy only `notes.db` (without `key.bin`) into a fresh profile folder and start Washi on it: a clear error message, not a crash.

## Human-only sign-off (nothing here can be done by a script)

- [ ] Product owner signs off the side-by-side review (`target/fidelity/index.html`, `flicker.gif`) at 100% and 400% for idle, hover and a large note (ticket 20, step 69).
- [ ] Live capture at 150% display scale passes every region (`pwsh tools/capture.ps1`). It was only run at the scale of the build machine so far: see `docs/PERF.md`.
- [ ] A clean Windows 11 VM runs the single `washi.exe` with nothing installed. (No installer or code-signing certificate exists yet: step 71 is open.)
- [ ] Compare memory with Windows Sticky Notes on the same machine, 3 notes (`pwsh tools/memprobe.ps1 -StickyNotes`).
