# 0002: Selection is a note-level range that Rust owns

Status: accepted (cross-paragraph selection, tickets 01 to 06). Supersedes the "Selection across paragraphs" line in the Out of Scope list of the sticky-notes spec.

## Context

Every paragraph is its own Slint `TextInput`. A press in a field grabs the pointer for that field, and Slint clamps a drag's cursor to that field's text, so a drag could never leave its paragraph and Shift+arrow stopped at the paragraph's edge. The only selection that spanned paragraphs was Ctrl+A, a special case (`all-selected`) with its own wash and its own key handling.

## Decision

1. **The selection is a range of two carets**, `Range { anchor, focus }`, each a `Caret { block, offset }`. `Doc::selection` holds it; `Doc::set_selection(ui, range)` is the one way to change it. Select-all is the range from the start of the note to its end.
2. **The note owns the pointer.** A transparent `TouchArea` over the text layer turns press, move and release into `pointer_down`, `pointer_drag` and `pointer_up`. Rust finds the caret under the pointer with `textflow::hit`. Click, Shift+click, double and triple click are rebuilt there.
3. **Fields keep what Slint does well**: the caret, keyboard input and the IME. A range inside one block is handed to that block's field (`set-selection-offsets`), which draws its own wash. A range across blocks is washed by the note (`selection_rects`), and the field holding the focus shows no selection of its own, so nothing is washed twice.
4. **Shift+arrows** that would cross a paragraph's edge (Left at its start, Right at its end, Up on its first line, Down on its last), and every Shift+arrow while the range already spans blocks, go to Rust, which moves the focus end of the range. Shift+Ctrl+Home and End move it to the note's ends. Inside one block, the field keeps handling Shift+keys itself.
5. **Drag auto-scroll**: while the button is down and the pointer is above or below the text view, a timer scrolls the note and extends the range to the visible edge, so the range keeps growing with the pointer held still.

## Consequences

- Screen readers see only each field's own selection, not the note's range (accepted for v1; listed in QA_CHECKLIST).
- Composing with an input method still happens in the focused field. Whether a field focused from code keeps the IME was checked in the harness only; a person on Windows with a Japanese IME has to confirm it.
- The pointer overlay must not cover the tapes and edge grips; the existing drag harness tests guard that.
- Every edit over a range is one undo step.
