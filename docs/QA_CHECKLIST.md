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
- [ ] Japanese IME in a field that was focused from code (a click, an arrow key or Shift+arrow, which move the focus between paragraph fields without the user clicking in the field): composition starts inline, candidates open at the caret, commit inserts. With a selection across paragraphs, starting to compose replaces the selected range first (the selected text is gone, the composition begins at the join).
- [ ] Screen readers (Narrator, NVDA) see only the selection of the focused paragraph field, not a selection across paragraphs. Accepted for v1 (ADR 0002); not a bug to fix before release.
- [ ] Note shortcuts (Ctrl+B/I/U/N/W/D/Z/Y, Ctrl+Shift+X/L) work with a non-Latin layout active (Japanese IME off/on, Russian, Greek). Known limitation: Slint key events carry only the produced text, no physical key. Washi accepts both the letter and the control code Windows sends for Ctrl+letter (for example U+0002 for Ctrl+B), which covers layouts that still produce control codes; a layout that reports its own letter (for example "и" for Ctrl+B) cannot trigger the shortcut. The toolbar buttons and the right-click menu always work.
- [ ] Emoji picker (Win+.) and emoji fallback rendering. Pasting rich text from a browser gives plain text.
- [ ] Clipboard: copy and paste in the text, paste a screenshot (inserts an image paragraph), "Export as Markdown..." writes the .md and PNGs and the .md opens correctly in a Markdown viewer.

## Text color and highlight (text-color-and-highlight spec, ticket 05)

Unchecked on purpose, like the rest of this file. Run on the release build.

- [ ] The "A" button (fifth of seven): its letter shows the selection's ink and its bar the marker. The button fill is on while the popover is open or the selection has a color. Known: the bar is near-invisible for a pale marker on the button's fill (`#F0F0EE`); judge by eye whether that is acceptable.
- [ ] Popover look against the claude.ai canvas "B · Ink dots" (https://claude.ai/artifact/66MmBXsWquEdiGhr2BFA6x): tilt follows the paper (1.5 degrees), the tail's tip sits on the center of the "A", the shadow is soft, the caption is in the handwriting font, the rings are right. The `uidiff` comparison against the board was never run (needs the board's PNG): do it by eye.
- [ ] Popover close paths: Escape, a click in the text, a click on another toolbar button, the pink tape or a resize zone, resizing, minimising, clicking another note or app (the note loses focus), typing over the selection, Clear. Opening it never blurs the text or collapses the selection.
- [ ] Keyboard: with the popover open, Left/Right walk a row, Up/Down switch rows, Enter or Space picks, Escape closes. The ring is blue (`#6FA2F5`, 2 px, 2 px off the swatch), also on the chosen swatch. The caret and selection stay in the text the whole time. Enter or Space before any arrow key types a space (and closes the popover).
- [ ] Accessibility: swatches carry the names `Text color Blue`, `Highlight Yellow`, `No highlight`, and Clear is `Clear color` (set through Slint's `accessible-label`). Check in Narrator or Accessibility Insights that they are exposed. Not verified: Slint shows them as buttons only when the popover is open, and the keyboard walk above is a Washi-level focus ring, not screen-reader focus (Narrator will not announce the swatch the ring is on). Accepted for v1.
- [ ] Colored text at rest (tilted, stepped baseline) and while editing (level): ink and marker both show, a marker is one rounded bar per line with no hairline seams at piece boundaries, over a heading and the title too. A selected highlighted word is the marker tinted by the blue wash, not hidden.
- [ ] Picking with a collapsed caret colors only what is typed next (nothing changes in the existing text).
- [ ] Undo and redo step back through ink and marker picks one at a time.
- [ ] Copy and paste: copying colored text and pasting elsewhere (Notepad, a browser, another note) gives plain text; colors are not copied.
- [ ] IME composition (Microsoft Japanese or Pinyin) over a colored range: the composing text is drawn in the ink, the marker stays behind it, the candidate window opens at the caret, commit keeps or drops the color as the typed-text rule says (the committed text takes the color of the character before it).
- [ ] Palette is in sync: the popover's names and colors are the stored ones (pick every swatch once and check `notes.db` text via Export as Markdown). The popover reads `document::PALETTE` directly, so there is one table.
- [ ] Contrast: the lowest ink-on-marker pair over the ten inks and nine markers is **3.62:1** (Orange ink `#B8581A` on Purple marker `#E8DEEE`; Orange on Blue, Brown and Orange markers are next, up to 3.68). Every pair passes the spec's 3:1 (large handwriting) but 57 of the 90 pairs are under 4.5:1, so small non-handwriting text would not pass. Look at Orange on Purple, Orange on Blue and Yellow ink on its markers at 100% scale in the handwriting font and confirm they read. Every ink on the paper (`#FFFFFD`) is at least 4.71:1 (lowest: Orange).
- [ ] Older build: open a note saved with colors in 0.1.0 without the feature: the `<c=blue>` tags show as text (accepted, see the release notes).

## Settings page (real desktop)

- [ ] Open/close transition at 100% and 150% display scale: no tearing or flicker, and after closing, the window is exactly on the note's original position and size (compare before/after).
- [ ] The tab height tween (switching tabs) is smooth and ends at the right height with no clipped rows.
- [ ] Start with Windows: the General toggle and the right-click menu check mark agree. Flip it in the menu while General is visible: the toggle follows within a second. Flip it in General: the menu agrees on the next open.
- [ ] Resource use tiles show plausible memory, CPU, database size and open-note count, update about once a second while General is showing, and stop when you switch tab (CPU in Task Manager returns to idle).
- [ ] Always on top default: turn it on, create a new note (Ctrl+N and the menu): it starts on top. Turn it off: new notes do not. Existing notes keep their own setting.
- [ ] Uninstall: with a real `unins000.exe` beside `washi.exe`, the row is enabled; Cancel does nothing; Confirm starts the uninstaller and Washi exits with notes saved. Without it the row is disabled.
- [ ] Shortcuts tab: every shortcut listed does what it says with the real keys on a note, and nothing real is missing from the list.

## Font & Style > Fonts (font-manager spec, ticket 10)

Settings > Font & Style, then the manage button on the Font row opens the Font manager. Use a throwaway profile (see the tip above) so installed fonts do not pile up in your own `%APPDATA%\Washi\fonts`. Nothing below was run by a person when this section was written: the checks the harness could do are in `tools/snap/tests/font_*.rs`, `ui_font.rs` and `active_font.rs`.

- [ ] First run: the Font row shows "Shadows Into Light" in its own face with a short summary line, and a 48 px manage button. The Font manager lists only Shadows Into Light, marked as the Font in use, with no Remove. Notes and every screen look exactly as before this feature (Font size 22).
- [ ] The manage button fills dark while the Font manager is open and goes back to outlined when it closes. The slip comes out to the right of the Paper with a tape strip, tilted like the Paper, and moves with the Settings page when you drag it. Esc closes the slip first and Settings second. Switching Tab or closing Settings closes it.
- [ ] Settings near the right edge of the screen (and on the right-most monitor): opening the slip shifts the Settings page left so the slip is fully on screen; closing it moves the page back.
- [ ] Install the real `Open_Sans.zip` from Google Fonts with "Install a font" (file picker). It appears in the list, scrolled into view, named "Open Sans" in its own face, with a subtitle like "Variable: weight, width" (and a summary line such as "400 · 100% · 0°" on the Font row once it is in use). Installing does not switch to it: notes stay in Shadows Into Light until you click its row. Click it: every open note and the Settings preview change at once, the row is marked as in use and expands with the Weight, Width and Slant panel.
- [ ] Install a Roboto Flex zip (variable, many axes): only Weight, Width and Slant are offered, each limited to the font's range. Pick it; text is wide but nothing in notes, menus or Settings is cut off (see the overflow check below).
- [ ] Install a script font (for example a Google Fonts script face with a single style): its row says "One style" (the Font row summary too). Weight and Width rows read "Not in this font"; Slant is a slider (0 to 12 degrees, sheared by Washi). Bold and italic in a note still show (bold is a double strike, italic leans further).
- [ ] Axis sliders: Weight snaps in hundreds (dots under the track), Width at 75 / 100 / 125 %, Slant at 0 / 5 / 10 degrees. Arrow keys move, Home and End jump. A number typed in the box commits on Enter or leaving the box, clamps to the range, Up and Down step it, Esc puts the old value back. While dragging, the Settings preview follows; the open notes change when you let go. Switch to another font and back: each font kept its own values.
- [ ] A `.otf` (CFF outlines) is refused with the error dialog "This font uses OpenType (CFF) outlines, which Washi does not support yet. Use the .ttf version." Nothing is added to the list. A text file renamed to `.ttf` says "This file is not a usable font."
- [ ] Reinstall the same zip: no duplicate row; the font keeps its Weight, Width and Slant. If it was the font in use, the notes redraw with the new files.
- [ ] Remove the font in use: the row goes, every note switches to Shadows Into Light at once, and `%APPDATA%\Washi\fonts\<id>` is deleted from disk. Removing a font that is not in use leaves the notes alone.
- [ ] Restart with a deleted fonts folder: pick an installed font, quit Washi, delete `%APPDATA%\Washi\fonts`, start again. Washi starts in Shadows Into Light, shows one message naming the missing font, the row is gone from the list, and the next start shows no message.
- [ ] The choice, its Axis values, the Default font's Slant and the Font size survive a restart.
- [ ] Font size: the slider runs 12 to 40, snaps at 14, 18, 22, 26, 30 and 36, and its box accepts any whole number in range (out of range is clamped). Check 12, 22 and 40 in a note with a title, heading, bullets, numbers, check items, bold and italic: the text, check boxes, bullets and spacing scale together, lines wrap correctly and the caret and selection sit on the letters. A note scrolled down stays near the same spot after a change. Menus, Settings and All notes text do not change with Font size.
- [ ] All three UI sizes (Appearance > UI size) with the Default font, Open Sans and Roboto Flex: every label in Settings, the Dropdown menu, the right-click menu, All notes and the Delete dialog fits its box (see gap b below).
- [ ] The Font manager list scrolls with the wheel when there are more fonts than fit (install four or five). Its scroll bar stays invisible until you scroll, hover or drag it, like a note's, and dragging it works. The title and the "Install a font" area do not scroll.
- [ ] Picture viewer: open a note's picture in the viewer with a non-default font in use; its caption lines use the Font in use (gap d below).

Known gaps from the implementers (the headless harness could not cover these; do them by hand and note the result):

- [ ] (a) Real drag and drop, once, from Explorer onto the open Settings window with the Font manager open. The harness cannot produce winit drop events (ADR 0003 S3 only simulated one in a spike). Check: holding a file over the window darkens the install area; dropping a Google Fonts `.zip` installs it; dropping a non-font file shows the picker's error; a drop with the Font manager closed does nothing; dropping two files at once installs both in turn.
- [ ] (b) Overflow at all three UI sizes with a wide font (Roboto Flex at its widest Width) and a script font: no label in Settings, Dropdown menu, right-click menu, All notes or the Delete dialog overflows or is clipped. The automated overflow test (spec: Whole UI) was not written; the UI fit factor (0.70 to 1.0) is only unit-tested.
- [ ] (c) The Font manager layout was built from the spec text, not from the design. Compare it with the artboard "C2 · Axes inside the fonts slip" on the canvas "Washi Font & Style Settings" (https://claude.ai/artifact/JUkiNFbsq93MqBDSEgyecA; the artboard is not in the repo): slip size and tilt, tape, row height, Axis panel spacing, snap dots, number boxes, scroll bar, install area.
- [ ] (d) The picture viewer takes the Font in use (it sets the Theme families when it opens, `image_viewer.rs`), but no test opens it with a non-default font.
- [ ] (e) Large fonts (more than about 4,000 glyphs, such as Segoe UI or Arial): picking one, or starting Washi with one in use, makes all five faces on the UI thread. ADR 0003 measured 60 to 120 ms for that; the "Regular first, other four on the next loop turn" fallback it adopted is NOT implemented. Pick Arial or Segoe UI (copy from `C:\Windows\Fonts`) and judge whether the pause is noticeable; dragging an Axis slider on such a font remakes Regular only (17 to 24 ms per step).

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
