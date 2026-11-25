# 0113 - Mkrew edits the omp composer itself

- Date: 2026-11-25
- Status: accepted (amends ADR 0107 and SPEC section 10, omp native surface)
- Milestone: none (human request 2026-11-25)

## Context

The composer of an omp pane drew omp's editor text, and every key went to omp through the PTY.
The human reported that the field did not feel like a text field:

- The caret was not visible: it was a 1 px wide `div` with no height in a centered row.
- Every typed letter came back only after a PTY round trip and an omp frame.
- Ctrl+A moved to the line start (omp's Emacs key), Ctrl+C interrupted, Ctrl+Z did nothing useful, and there was no selection.
- A skill token was its own flex child, so a long prompt wrapped it into a narrow column at the left of the text.
- A pasted image showed its full preview in the card.

The reference (section 5, `edit`) lets a terminal edit the editor text locally and tell omp each change; omp applies it with `applyHostEdit`, and its completion list follows host edits.

## Decision

- While omp's focus is on the composer's `editor` node, no modal is open, the surface is not read-only and the editor is not in vim mode, Mkrew keeps the field itself: `tsp::LocalEditor` holds a `text_box::Input` (caret, selection, undo, as every other Mkrew field).
- Typing, Backspace and Delete (and their word forms), caret moves, Shift selection, Ctrl+A, Ctrl+Z, Ctrl+Y, Ctrl+C and Ctrl+X with a selection, Ctrl+V with text, and Shift+Enter (a new line) change the field at once and send one `edit` event: the changed middle as one splice, with the caret, in UTF-16 units of the text omp has.
- Enter, Tab, Esc, Ctrl+Enter, the arrows while omp shows its completion list, Up on the first line and Down on the last (omp's history), Ctrl+C and Ctrl+X without a selection, an image paste and every other modified key go to omp through the PTY as before.
  The field's caret is sent first, so the key acts where the caret is.
- omp's drawn text is reconciled with the field: a text Mkrew sent and omp has not drawn yet is skipped; any other text is omp's own change (a completion, the history, a send that emptied it) and replaces the field as one undo step.
- The editor is drawn as one wrapping text (the `prompt` lead, omp's decorations while omp has the same text, the selection, a painted caret, the ghost), so a skill stays inline with the prompt. Plain `editor` and `input` nodes are drawn the same way, with omp's caret.
- An attachment chip is a small accent `#n` chip; a click opens its content (the image, the pasted text) in a panel above it.
- The daemon accepts `edit` (not `undo` or `send`); PROTOCOL_VERSION 63.

## Consequences

- A key typed in the few milliseconds after Enter or a completion key, before omp's frame, can be lost: omp refuses the `edit` (its text length differs) and its next frame replaces the field.
- omp's large-paste collapse (`[Paste #1 +30 lines]`) does not happen for text pasted with Ctrl+V; the text is typed as it is.
- The same field model (`text_box::Input`, `field::caret_over`, `field::highlight_runs`, `field::caret_mouse`) is the one to use for any new text input.
