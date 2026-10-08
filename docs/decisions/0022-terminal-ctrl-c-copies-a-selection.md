# 0022 — Terminal Ctrl+C copies a selection

- Date: 2026-10-08
- Status: accepted
- Milestone: inbox 2026-10-08

## Context

The user expects Windows Terminal behavior: Ctrl+C copies selected terminal text. SPEC §13 only
intercepts keys bound in the keymap, so plain Ctrl+C always reached the agent as ^C and copying
needed Ctrl+Shift+C.

Separately, alacritty_terminal 0.26 drops a selection when the selected lines are erased
(`clear_line`, `swap_alt`, `clear_screen` in `term/mod.rs`), so a selection over a redrawing
spinner (omp) can vanish before it is copied. That is not changed here.

## Options

1. Ctrl+C copies when there is a selection, else ^C — what Windows Terminal does; a stale
   selection could swallow an intended interrupt unless typing clears it.
2. Keep Ctrl+Shift+C only, plus a hint — no surprise for ^C, but not what the user asked for.
3. Copy on mouse-up — copies without a key, but overwrites the clipboard on every drag.

## Decision

Option 1. Plain Ctrl+C (no Shift/Alt) with a selection copies it and clears it, also in an
exited pane. Anything sent to the session (keys, paste, dropped paths, wheel arrows) clears the
selection first, so a later Ctrl+C interrupts again. Ctrl+C stays unbound in the keymap; the
decision is `terminal::input::key_action`, used by `TerminalPane::key_down`; `input()` clears.

## Consequences

SPEC §13: the "only bound keys are intercepted" rule gets this one exception, and the Copy row
reads "Ctrl+Shift+C (or Ctrl+C while text is selected)".
