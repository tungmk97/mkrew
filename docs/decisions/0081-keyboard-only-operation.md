# 0081 - Keyboard-only operation

- Date: 2026-10-27
- Status: accepted
- Milestone: none (user request: every action done often must work without the mouse)

## Context

An audit of `crates/mkrew-ui` (2026-10-27) found four root causes that leave many actions mouse-only.
A: context menus open only on right-click and an open menu has no focus, no arrows, no Enter, no Esc.
B: buttons, chips and checkboxes never take focus.
C: lines and terminal text can be picked only with the mouse.
D: single actions have no command.
Design: `docs/superpowers/specs/2026-10-27-keyboard-only-design.md`.
The work ships in four waves; each wave appends its section here.

## Options

1. A key handler per menu: twelve copies of the same rules, and a click and a key can diverge.
2. One menu model (`menu.rs`, pure and tested) and one renderer in the shell; each menu only builds rows. (Chosen for wave 1.)
3. GPUI tab stops inside menus: arrows, type-ahead and swatches are not tab-stop behavior.

## Decision

### Wave 1 - Keyboard context menus

- Every popover menu (tab, layout, pane, share, workspace, file, file tab, Run, workspace switcher, "+ New", Run with, fan-out, Board card, and the Git By:, More and Repositories popovers) is a list of `MenuRow`s drawn by one renderer.
- A click and a key call the same action, so they cannot diverge.
- Every open menu takes the keyboard (`menu_focus`, key context `Menu`), opened by mouse or keyboard.
- Keys: ↑/↓ and Home/End select enabled rows, ←/→ move on a swatch row, Enter and Space run the row, a letter selects the next row starting with it, Esc closes.
- A held key does nothing, so the key that opened a menu cannot pick a row.
- Closing (Esc, an action, a click outside, any bound command) gives the keyboard back to what had it before.
- New command `context_menu` (`Shift+F10`, global) opens the menu a right-click would open on what has the keyboard (terminal pane, Files cursor row, active file tab, sidebar filter row, selected Board card), else the workspace menu under the title bar's switcher.
  Pressed again it closes the menu.
- Board keys are shielded while a menu is open: `Keymap::key_bindings` adds `NoAction` bindings in context `Menu` for plain and Shift-only keys of context-limited commands.
- The workspace menu floats at an anchor instead of dropping inside the sidebar row.
- The Run menu's inline restart / stop icons become rows (a running command has a "Stop" row).
- The Board card gets a menu with its hover actions (Approve, Undo…, Changes, Merge back…, Remove worktree…, Compare…, Run with…, Run with several agents…); the hover buttons stay.
- Repositories search: ↑/↓ move the selection through the filtered rows; Enter shows the selected one.

## Consequences

- SPEC §13 (Shift+F10, menu keys), §10 (popover menus, card menu) change.
- `Shift+F10` no longer reaches a focused terminal; rebind `context_menu` in Settings → Keyboard if a program needs it.
- The Windows Menu key arrives as `menu`: bind it in Settings → Keyboard.
- Waves 2 to 4 (focusable dialog controls, picking by keyboard, single actions) append their sections.
