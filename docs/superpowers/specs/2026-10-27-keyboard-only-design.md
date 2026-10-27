# Keyboard-only operation - design

- Date: 2026-10-27
- Status: split into 4 waves approved in chat, awaiting spec review
- ADR to write with wave 1: `docs/decisions/0085-keyboard-only-operation.md` (covers all four waves; each later wave appends its section)
- SPEC sections touched: §13 (shortcuts), §14 (focus ring), §12 / §12B (line picking), §10 (sidebar, terminal)

## Goal

Every action a user does often can be done without the mouse.
Today about 110 commands have shortcuts and every dialog closes with Esc.
But an audit of `crates/mkrew-ui` (2026-10-27) found four root causes that leave many actions mouse-only:

|#|Root cause|Effect|
|---|---|---|
|A|Context menus open only on right-click; an open menu has no focus, no ↑/↓, no Enter, no Esc (`widgets.rs` `menu_panel` / `menu_item`)|About 20 menu items (Close others, Pin, colors, default agent / shell, New file / folder, Copy path) have no keyboard path; the palette entry "Workspace: Open workspace menu" is a dead end|
|B|Buttons, chips and checkboxes never take focus (`button()`, `icon_button()`, `segment()` have no `track_focus`); Tab in dialogs moves between text fields only|Task dialog agent chips and "Run in a worktree", Ask chips, Phone access, Fan-out picker, Run setup Browse / Add / Remove, worktree / undo / discard confirmations|
|C|Lines and terminal text can be picked only with the mouse|`Ctrl+Alt+M` (comment) and `Ctrl+Shift+L` (send selection) do nothing without a prior click; terminal copy needs a drag; terminal links open only with Ctrl+Click|
|D|Single actions without any command|Approve card, comment Reply / Resolve, toasts, inspector Stop, Jira, sidebar chips, tab reorder, and more (wave 4 list)|

Success: a user can go through the daily loop with the keyboard alone: switch workspace, open a session, answer, review the diff, comment, approve the card, commit.
The same holds for every action listed in this spec.
Each wave ships separately and is useful on its own.

## Principles (all waves)

- Reuse what exists: `Command` + `keymap.toml` for bound keys, `palette::Extra` for palette-only actions, GPUI tab stops for focus order, `alacritty_terminal` vi mode for terminal selection.
- New default keys must be unique across all commands (the `Keymap::parse` duplicate check ignores context, and defaults are asserted unique).
- New default keys avoid plain `Ctrl+<letter>` outside the Files view (SPEC §13) and avoid `Ctrl+Alt+<letter>` where a terminal has focus (AltGr).
- Keys that only mean something inside a mode (menu navigation, copy mode, link hints) are not in `keymap.toml`, like the keys of the peek and the confirm prompt today.
- A view-context key that a text field or the editor also uses (Shift+↑/↓, `a`, `w`, Alt+Enter) acts only while the view's list / tree / diff has the keyboard.
  While a text field, a comment draft or the Files editor has it, the key stays with that field, as `v` does in the Git commit box today.
- Global keys that now reach no terminal: `Shift+F10`, `Alt+0`, `Ctrl+Shift+PgUp / PgDn`; SPEC §13 lists them next to the existing note about Alt+Shift+A / S and F2.
- Destructive actions are never the default focus and never run on a bare Enter that opened the dialog.
  They run only when the keyboard was moved onto them first (Tab / arrows) and then Enter or Space is pressed.
- Every focusable element shows a visible focus ring from `theme.rs` tokens, only while keyboard focused (`focus_visible`), in both themes.
- Tooltips and menus show the shortcut; the palette lists every new action.

## Wave 1 - Keyboard context menus (root cause A)

### Open

New `Command::ContextMenu`, name `context_menu`, default `shift-f10`, no context (global).
The Windows Menu key arrives from GPUI as `menu`; users can bind it in Settings → Keyboard.
It opens the menu a right-click would open on the element that has the keyboard:

|Keyboard is in|Menu|Anchored at|
|---|---|---|
|A terminal pane|its pane menu (`render_pane_menu`)|the pane header's bottom-left|
|The Files tree|the file menu of the cursor row (`render_file_menu`)|the cursor row's bottom-left|
|The Files editor / preview|the active file tab's menu|the active file tab's bottom-left|
|The sidebar filter with a highlighted row|that row's workspace menu or pane menu|the row's bottom-left|
|The Board with a selected card|the new card menu (below)|the card's bottom-left|
|Anything else (no owner)|the active workspace's menu|the title bar's workspace switcher|

The anchor uses the owner's bounds recorded during prepaint.
When the owner was not painted this frame, the anchor falls back to the title bar's workspace switcher.
Pressing `context_menu` while a menu is open closes it (toggle).

### Navigate

One model serves all twelve menus listed in `Shell::close_menus` (tab, layout, pane, share, workspace, file, file tab, run, workspace switcher, "+ New", run with, fan-out) and the Git "By:" and Repositories popovers:

```rust
/// One row of a popover menu (crates/mkrew-ui/src/menu.rs, pure, unit-tested).
pub enum MenuRow<A> {
    Item { label: SharedString, keys: Option<String>, checked: bool, enabled: bool, action: A },
    /// A row of color swatches: ←/→ move between them, Enter picks.
    Swatches { current: usize, count: usize, enabled: bool, action: A },
    Separator,
    /// A non-selectable muted line (Loading agents…, an error).
    Note(SharedString),
}
```

In the binary, `A` is `Rc<dyn Fn(&mut Shell, usize, &mut Window, &mut Context<Shell>)>`; the `usize` is the swatch index, 0 for items.
Each `render_*_menu` builds `Vec<MenuRow<A>>` and passes it to one renderer.
The renderer draws `menu_item` rows, separators, notes and swatches, and wires both the click and the keyboard to the same `action`.
So a menu item's click and its Enter can never diverge.

A pure function decides what a key does (like `confirm::key`):

|Key|Effect|
|---|---|
|↓ / ↑|select the next / previous enabled `Item` or `Swatches` row, wrapping; skips separators, notes, disabled rows|
|Home / End|first / last enabled row|
|← / →|inside a `Swatches` row: previous / next swatch|
|Enter / Space|run the selected row's action, then close the menu (multi-select rows of the fan-out picker toggle and stay open; Enter on its Run row runs)|
|Esc|close the menu and give the keyboard back to where it was|
|A letter|select the next enabled row whose label starts with it (type-ahead, one letter)|
|A held (auto-repeat) key|nothing, so the key that opened the menu cannot pick a row|

Every open menu takes the keyboard through one `menu_focus: FocusHandle` on `Shell` with key context `Menu`, whether it was opened by mouse or keyboard.
Opening by keyboard selects the first enabled row; opening by mouse selects nothing until an arrow is pressed.
Closing (Esc, an action, a click outside, any bound command) refocuses the element that had the keyboard before the menu opened.
This also fixes Esc falling through to the terminal while a menu is open.

Git Repositories keeps its search box.
↑/↓ in the box move the selection through the filtered rows (today Enter always takes the first match).

### Card menu (new)

The Board card's hover buttons become a menu so the keyboard reaches them.
Rows: Approve, Undo…, Changes, Merge back…, Remove worktree…, Compare…, Run with…, Run with several agents…, each shown only when its button would be.
The hover buttons stay; their clicks call the same functions.

### Effects

- The palette entries "Workspace: Set default agent…" and "Workspace: Open workspace menu" stop being dead ends.
- Files: New file…, New folder…, Copy path, Add to .gitignore, Open in File Explorer / Zed of the cursor row.
- File tabs: Close others / to the right / all, Pin, Copy path.
- Panes: Pin pane, Change color, Mark unread.
- Workspaces: Default agent, Default shell, Color, Run setup.
- Run with and fan-out pickers (fan-out: Space ticks an agent, Enter on Run starts).

## Wave 2 - Focusable controls in dialogs (root cause B)

### Model

Use GPUI's tab stops, not a hand-written focus list: `FocusHandle::tab_stop(true)` + `tab_index`, `window.focus_next()` / `focus_prev()`, and `.focus_visible(..)` for the ring.
Handles of controls come from `window.use_keyed_state(element_id, ..)`, so a button needs no field on `Shell`.

- `button`, `secondary_button`, `quiet_button`, `danger_button`, `icon_button`, `segment` and the checkbox / chip rows get a focusable variant.
  The exact shape (a `.focusable(window, cx)` extension or an argument) is chosen in the plan after reading the call sites.
  It adds `track_focus`, the ring, and Enter / Space = click.
- Only controls inside a modal dialog are tab stops.
  Buttons in the sidebar, title bar and views are reached through commands, menus and the palette, so Tab inside a dialog never lands on a control behind the shade.
- Each dialog root binds Tab / Shift+Tab to `focus_next` / `focus_prev`.
  This replaces the hand-written Tab handling (task dialog Title ↔ Prompt, Run setup forward-only Tab, broadcast Tab = tick).
- Order: fields first (top to bottom), then options (chips, checkboxes), then buttons left to right; Cancel before the primary button.
- Space toggles a focused checkbox / chip; ←/→ move inside a chip group (agent picker, web on / off).
- Initial focus: the first text field, else the primary button, never a destructive button.
- The ring uses theme tokens (color, width, the control's radius).
  Missing tokens are added to `theme.rs`, with both themes' snapshots updated.

### Destructive confirmations (changes the `undo_dialog` rule)

Today Undo Discard, worktree Remove / Discard, Git Discard and Compare Pick take only a click ("discarding takes an explicit click", `undo_dialog.rs:282`).
New rule: they also run when the keyboard was moved onto the button (Tab) and Enter or Space is pressed.
They are never the initial focus, and the held-key guard of the confirm prompt applies (auto-repeat does nothing).
ADR 0085 records this change.

### Dialogs covered

- Task, Ask, Undo, Phone access, Send, Docs.
- Broadcast: ↑/↓ + Space ticks; Tab moves to the buttons.
- Add workspace: Open a folder…, Browse….
- Run setup: Browse…, Duplicate / Remove / Add command, Shift+Tab.
- Run setup picker: → enters a folder, ← goes to the parent, Enter picks the folder or the `.env` file under the cursor.
- Worktree: Confirm, Discard, Keep files, Git view, Remove others.
- Compare: Git view, Pick.
- Git discard, Git base (Auto).
- Settings: Reset of a shortcut and "Fix in Zed" become focusable.
- Turns: `d` on the selected turn = "Diff since".

The confirm prompt already works and is left as is.

## Wave 3 - Picking by keyboard (root cause C)

### Lines in Docs and the Git diff

New commands, context `Docs || Git`:

|Command|Name|Default|Effect|
|---|---|---|---|
|`PickDown`|`pick_down`|`shift-down`|extend the picked lines one line / block down|
|`PickUp`|`pick_up`|`shift-up`|shrink / extend one line / block up|

Start point when nothing is picked: Git = the first new-side line of the current hunk (`n` / `p` move it); Docs = the first line / block in view.
The picked range scrolls into view.
Esc clears the pick (Git already does; Docs gets the same).
Then `Ctrl+Alt+M` comments and `Ctrl+Shift+L` sends, as with a mouse pick.
The pick is the existing `comments::Selection`; no new selection type.

### Terminal copy mode

New `Command::CopyMode`, name `copy_mode`, context `Terminal`, default `alt-shift-c`.
It turns on `alacritty_terminal`'s vi mode (`Term::toggle_vi_mode`, `vi_motion`) on the focused pane's emulator.
While on, keys go to the UI, not the PTY, and the pane header shows "Copy mode" with the key hints:

|Key|Effect|
|---|---|
|←↓↑→ / h j k l|move the cursor|
|w / b / e, 0 / $, g / G, PgUp / PgDn|word, line start / end, top / bottom of history, page|
|v / Shift+V|start / switch to a character / line selection|
|y or Enter|copy the selection, leave copy mode|
|Ctrl+Shift+L|send the selection to an agent's composer (existing `SendSelection`)|
|Ctrl+Shift+F|open the find bar; its match moves the cursor|
|Esc|clear the selection, or leave copy mode when none|

Output arriving while copy mode is on does not move the view; the cursor stays on its text.
Leaving copy mode returns to the live screen.
The selection uses the existing `Selection` paths, so Ctrl+Shift+C also copies it.

### Terminal link hints

New `Command::LinkHints`, name `link_hints`, context `Terminal`, default `alt-shift-o`.
It scans the visible rows with `links::link_at` (same detection as Ctrl+Click) and draws a short label over each link (`a`, `s`, `d`, `f`, …, then two letters).
Typing a label opens that link exactly like Ctrl+Click; Esc or any other key cancels.
No links on screen: a tab bar notice "No links on screen.".

## Wave 4 - Single actions (root cause D)

### New bound commands

|Command|Name|Context|Default|Effect|
|---|---|---|---|---|
|`FocusSidebar`|`focus_sidebar`|global|`alt-0`|show the sidebar if hidden, focus its filter with the active row highlighted (today `/` works only when no terminal has focus)|
|`ApproveCard`|`approve_card`|Board|`a`|Approve the selected Review card|
|`NextComment` / `PreviousComment`|`next_comment` / `previous_comment`|Docs ‖ Git|`f8` / `shift-f8`|select the next / previous comment thread and scroll it into view|
|`ReplyComment`|`reply_comment`|Docs ‖ Git|`ctrl-alt-r`|open the reply field of the selected thread|
|`ResolveComment`|`resolve_comment`|Docs ‖ Git|`ctrl-alt-enter`|resolve / reopen the selected thread|
|`MoveTabLeft` / `MoveTabRight`|`move_tab_left` / `move_tab_right`|global|`ctrl-shift-pageup` / `ctrl-shift-pagedown`|move the active terminal tab (file tab in Files) one place, within its pinned / unpinned group|
|`GitToggleWrap`|`git_toggle_wrap`|Git|`w`|toggle Wrap of the diff|
|`OpenKept`|`open_kept`|Files|`alt-enter`|open the tree's cursor file in a kept tab (Enter stays transient)|

The Board's `Card(_)` keys also move and open rows of the Jira list / kanban (today `sessions.rs` ignores them while Jira is shown).
Pressing Show Board (`Ctrl+Shift+2`) while the Board is already shown and Jira is configured switches Tasks ↔ Jira.

Sidebar filter keys (no keymap entries, like its ↑/↓ today): Tab / Shift+Tab cycle the chips All / Working / Needs you; ← / → collapse / expand the highlighted workspace.

### New palette entries (`palette::Extra`)

|Title|Available when|
|---|---|
|Tab: Pin / Unpin|a terminal tab is active|
|Tab: Close others / Close tabs to the right|the active tab has such tabs|
|Pane: Pin / Unpin|a pane is focused|
|Pane: Reading width (on / off)|an agent pane is focused|
|Workspace: Set default shell…|a workspace is active (opens the workspace menu at that list)|
|Workspace: Move up / Move down|a workspace is active (same reorder as the drag)|
|Agent: Stop session…|a live session is focused (same as the inspector's Stop)|
|Agent: Open transcript|an agent pane with a transcript is focused|
|Agent: Link plan… / Unlink plan|an agent pane is focused|
|Agent: Improve prompt|the focused pane's composer is open|
|Run: Start / Restart / Stop `<command>`|one entry per run command of the active workspace, in Run menu order|
|Git: Change base…|Git view, Against base available|
|Git: Switch repository…|Git view with several repositories|
|Jira: All / Mine, List / Kanban, Refresh|Board shows Jira|
|Global: Copy last error|a toast with an error was shown this run|
|Global: Dismiss notifications|a toast is shown|
|Global: History…|always (same as the title bar button)|
|Global: Install update|an update is ready (same as the title bar button)|

### Tooltips

Fix tooltips that miss their key while the action has one: hunk Stage / Unstage (`git_diff.rs:813`), the Branch chip (`Ctrl+Shift+4`), the workspace switcher (`Alt+1…9`), card Approve (`A`).

## Out of scope

- Rename workspace: a missing feature, not a keyboard gap.
- Resizing the sidebar and the inspector by keyboard (`Ctrl+Alt+B` / `Ctrl+Shift+B` / `Ctrl+Shift+Y` cover hide / compact).
- Dropping files from the OS into a pane (paste and `@` cover it).
- Making sidebar, title bar and view buttons Tab stops (they are reached through commands, menus and the palette).
- Collapsing Git file groups by keyboard; Retry / Show all buttons of error states (rare; re-running the view's command covers them).

## Testing

- `menu.rs`: unit tests of the key function (wrap, skip disabled / separators / notes, swatches ←/→, type-ahead, held keys, fan-out multi-select), like `confirm.rs`.
- `keymap.rs`: the existing "defaults are unique" and template tests cover the new commands; a new test only if a context-specific rule is added.
- Line picking: unit tests on the start point and the extend / shrink boundaries (first / last line, hunk edges, a pick past the end after an edit).
- Copy mode: an emulator test that vi motion + `v` + `y` yields the expected text, and that output arriving in copy mode keeps the view offset.
- Link hints: a unit test of label assignment (order, two-letter labels after the single ones) and of matching typed labels.
- Palette: the existing availability tests get rows for the new `Extra` entries.
- Every wave: launch the app and do the wave's actions with the keyboard only, in dark and light.
  Anything not checked automatically goes into `docs/HUMAN_CHECKS.md` → "To verify".

## SPEC and docs changes

- §13: the new commands and defaults, the context menu key, menu navigation keys, copy mode and link hint keys, sidebar filter Tab / ←→, and the rule that every dialog control is reachable with Tab.
- §14: the focus ring tokens and the rule "focus ring only while keyboard focused".
- §12 / §12B: Shift+↑/↓ picks lines.
- §10: copy mode and link hints in the terminal; the card menu on the Board.
- ADR 0085: keyboard-only operation, including the destructive-confirmation change from wave 2.

## Refinements found while planning (2026-10-27)

The plans in `docs/superpowers/plans/2026-10-27-keyboard-wave-{1..4}-*.md` refine this spec as follows; where they differ, these lines win.

- Wave 1: `MenuRow::Item` holds a `MenuItem` struct (icon / agent badge / layout thumb / status dot lead, tone, tooltip) and a `Heading` row is added, because today's menus draw more than a label.
- Wave 1: an open menu shields the Board's single keys with `NoAction` bindings in key context `Menu`, since GPUI runs bindings before `on_key_down`.
- Wave 1: the workspace menu floats at an anchor at the window root instead of dropping down inside the sidebar row.
- Wave 1: the Run menu's inline ↻ / ■ / ▶ icons become rows; the Git "⋯" popover moves to the shared renderer too.
- Wave 2: the focus ring reuses `BORDER_WIDTH` and adds one `focus_ring` color token.
- Wave 2: a dialog without a text field starts with the keyboard on the dialog itself (Enter keeps its old meaning); no button gets first focus.
- Wave 2: Compare Pick only opens Merge back, so it is a plain Tab stop, not a destructive confirmation.
- Wave 2: in Broadcast, Space ticks only in list mode (entered with ↑/↓, left by typing), so a space can still be typed in the message.
- Wave 3: while copy mode or link hints are on, the emulator holds incoming output (capped at 1 MiB) so the view and the labels do not move.
- Wave 3: `Ctrl+Shift+L` does not send a Docs pick (sending stays Files / Git / terminal); a Docs pick is commented with `Ctrl+Alt+M`.
- Wave 4: `Alt+0` on the compact rail widens the sidebar, since the rail has no filter.
- Wave 4: ←/→ in the sidebar filter collapse / expand only while the filter is empty (else they move the caret).
- Wave 4: "Global: History…" needs an active workspace; "Agent: Unlink plan" shows only with a linked plan; "Git: Change base…" only in Against base.
