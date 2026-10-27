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

## Wave 2 - focusable controls in dialogs (2026-10-27)

### Decision

- The controls of a modal dialog are Tab stops through gpui's tab index.
  A control with `tab_index(0)` keeps its focus handle in its element state, so no field on `Shell` and no `use_keyed_state` is needed.
- The dialog's panel keeps `dialog_focus`.
  It is the first Tab stop when the dialog has text fields, so Shift+Tab can come back to them.
- Tab and Shift+Tab are handled in each dialog's `on_key_down` (`Shell::dialog_nav`), not bound in `keymap.toml`.
  gpui has no default Tab binding, and the key only means something inside a dialog.
- The text fields stay virtual (a `field` or `focus` value in the dialog model and one `dialog_focus` handle).
  Tab walks them first; at the end of that chain it moves on to the buttons, and from the last button it wraps back.
- A focused control takes Enter and Space (gpui clicks it on release) and every typed key.
  Esc, arrow keys and Ctrl combinations still reach the dialog.
- The ring is a 1 px border in `Theme.focus_ring` (the text color) over the control's own border, shown by `focus_visible` only after keyboard input.
- Chips of a row are each a Tab stop, and ← / → move between them.
- Initial focus is the dialog itself (or its first text field), where Enter keeps its old meaning.
  The spec said "else the primary button"; the primary button of several dialogs is destructive, so no button gets the first focus.
- Broadcast: Space checks the row the arrows picked; Space in the message types a space until an arrow key moves the keyboard to the list.
- A control that disappears while focused (a button swapped for its disabled twin) returns the keyboard to the dialog, so Esc keeps working.

### Destructive confirmations (replaces the rule of `undo_dialog.rs`)

Before: Undo Discard, worktree Remove and Discard files, and Git Discard took only a click ("discarding takes an explicit click").
Now: they also run when Tab moved the keyboard onto the button and Enter or Space is pressed.
They are never the first focus, and an auto-repeating Enter or Space is dropped before a control sees it (`dialog_focus::blocks_repeat`), like the held-key guard of the confirm prompt.
The daemon still re-checks every destructive request (previews and fingerprints), so nothing else changed.
Compare Pick only opens Merge back, so it is a plain Tab stop and not part of this rule.

### Consequences

- SPEC §13 and §14 describe the dialog keys and the ring.
- Turns: D shows the diff since the selected turn.
  Run setup: Shift+Tab walks back, and its picker takes ↑ ↓ → ← and Enter on the entry under the cursor.

## Wave 3 - Picking by keyboard

- Lines of a doc or diff are picked with `PickDown` / `PickUp` (`shift-down` / `shift-up`, key context `Docs || Git`) over the existing `comments::Selection`; no new selection type.
- `pick::step` is pure over "units": source lines (Docs source), top-level blocks (Docs preview) and new-side diff lines (Git).
  Nothing picked: the first line or block in view, in Git the first new-side line of the current hunk (`n` / `p` choose the hunk).
  Picked: Shift+↓ moves the end that was last moved, Shift+↑ shrinks it, a single line grows towards the key's direction.
  `Comments.head` remembers which end moves; a click resets it.
- Shift+↑/↓ stays with a focused text field, a comment draft or the Files editor: the shell's `run` already hands every command to them, and the Git key context sits on the file list only.
- Esc clears the pick in Docs (Git already did).
- Terminal copy mode is `alacritty_terminal`'s vi mode (`Term::toggle_vi_mode`, `vi_motion`) on the focused pane's emulator, `copy_mode` = `alt-shift-c`.
  While it is on, keys go to the UI and never to the PTY; the keys are not in `keymap.toml`.
  Entering copy mode clears a mouse selection, so `v` starts at the cursor.
  `y` / `Enter` copy the selection without the trailing newline of a line selection, so a paste into a shell never runs the line.
  Sending the selection (`Ctrl+Shift+L`) ends copy mode in the source pane; losing focus alone keeps it.
  The emulator holds incoming output back while copy mode (or link labels) is on, so the view and the cursor stay on their text; leaving applies it and returns to the live screen.
  Held output is bounded (1 MiB); a flood above that is applied at once.
- Link hints (`link_hints` = `alt-shift-o`) scan the visible rows with the same detection as Ctrl+Click and label each openable link: single letters first, then two letters, never a label that starts another.
  The picked link opens through the `OpenLink` event of Ctrl+Click.
  No link: a tab bar notice "No links on screen." that goes away after 3 seconds.
- Consequence: `Alt+Shift+C` and `Alt+Shift+O` no longer reach a focused terminal (`ESC C`, `ESC O`); rebind `copy_mode` / `link_hints` in Settings → Keyboard if a program needs them.
- Out of scope: `Ctrl+Shift+L` in Docs (it sends from Files, the Git diff and terminals only, as before).
