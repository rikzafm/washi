# 0002: Selection is a note-level range that Rust owns

Status: accepted (cross-paragraph selection, tickets 01 to 06). Supersedes the "Selection across paragraphs" line in the Out of Scope list of the sticky-notes spec.

## Context

Every paragraph is its own Slint `TextInput`. A press in a field grabs the pointer for that field, and Slint clamps a drag's cursor to that field's text, so a drag could never leave its paragraph and Shift+arrow stopped at the paragraph's edge. The only selection that spanned paragraphs was Ctrl+A, a special case (`all-selected`) with its own wash and its own key handling.

## Decision

1. **The selection is a range of two carets**, `Range { anchor, focus }`, each a `Caret { block, offset }`. `Editor::selection` holds it; `Editor::set_selection(ui, range)` is the one way to change it. Select-all is the range from the start of the note to its end. Every motion and pointer change goes through it; the caret motions that carry the side of a wrap and the goal column, and the pointer, use a private variant, `select_with`, which `set_selection` itself calls with nothing carried, so there is one code path. Two other writers exist on purpose and do not go through `set_selection`, because they must not wash or focus: `set_range` (no window, for headless use and edits that refocus themselves) and `caret_moved` (the field already holds the selection it reports, and washing and focusing it again would fight the field). The field is private; all three end in one private `store`, the only assignment of the selection. So "no code outside `set_selection` assigns the selection" holds at the level of `store`, not of `set_selection` alone.
2. **The note owns the pointer.** A transparent `TouchArea` over the text layer turns press, move and release into `pointer_down`, `pointer_drag` and `pointer_up`. Rust finds the caret under the pointer with `textflow::hit`. Click, Shift+click, double and triple click are rebuilt there.
3. **Fields keep what Slint does well**: the caret, keyboard input and the IME. A range inside one block is handed to that block's field (`set-selection-offsets`), which draws its own wash. A range across blocks is washed by the note (`selection_rects`), and the field holding the focus shows no selection of its own, so nothing is washed twice.
4. **Caret motion is the note's.** Every arrow, Home and End key (with or without Shift, and Ctrl+Home and End for the note's ends) goes to Rust's `caret_motion` module, which moves the caret or the focus end of the range; the block files forward every key they do not take themselves (Ctrl+A, Ctrl+Z and Ctrl+Y, paste and the copy and cut of a picture, and Alt or Meta chords, which they reject) with its typed modifiers, and keep no table of which keys are motions or where a paragraph's edge is. The field still moves itself for a Left or Right (Shift or not) that steps inside its paragraph, and for Ctrl with an arrow. Originally the field handled every Shift+key inside one block; Shift+Up, Down, Home and End inside one paragraph now go to the note too, so wrapped lines, the goal column and the end of a wrapped line follow one rule everywhere. Ctrl+Home and Ctrl+End go to the note's start and end, and Home, End, Up and Down over a selection inside one paragraph are the note's and collapse it by the module's rule (the edge the key points at, or the focus's line edge). A key that moves nothing on an image, or a plain Up or Down at the note's ends, is left to the field.
5. **Drag auto-scroll**: while the button is down and the pointer is above or below the text view, a timer scrolls the note and extends the range to the visible edge, so the range keeps growing with the pointer held still.

## Consequences

- Screen readers see only each field's own selection, not the note's range (accepted for v1; listed in QA_CHECKLIST).
- Composing with an input method still happens in the focused field. Whether a field focused from code keeps the IME was checked in the harness only; a person on Windows with a Japanese IME has to confirm it.
- The pointer overlay must not cover the tapes and edge grips; the existing drag harness tests guard that.
- Every edit over a range is one undo step.
