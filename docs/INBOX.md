# Inbox — bugs, enhancements, features

Write one line per item under **Items**: `- [ ] <what is wrong / what you want>`.
Optional tag at the start: `[bug]`, `[enh]`, `[feat]` (the skill guesses if missing).
Indented lines under an item are part of it (steps to reproduce, screenshots, ideas).

Then run the `mkrew-inbox` skill. It only reads `[ ]` items.

Markers: `[ ]` new · `[~]` planned (skill adds `→ plan: <file>`) · `[x]` done · `[-]` dropped.

## Items

- [x] In the git commit box: cannot Ctrl+A to select all to remove → plan: docs/superpowers/plans/2026-10-08-inbox-keys-fonts-view-rail.md Task 1
- [~] Add a feature to config font style, font size... for the app and for the terminal → plan: docs/superpowers/plans/2026-10-08-inbox-keys-fonts-view-rail.md Task 4, 5
- [~] I want more view on the main content, so the top bar (Board, Terminal...) should move somewhere else — I'm thinking the right bar. The Run button can move inline with the History, + Agent bar. So we clean up the top bar and get more space → plan: docs/superpowers/plans/2026-10-08-inbox-keys-fonts-view-rail.md Task 6
- [~] In terminal, Ctrl+C now cannot be used to copy text, please fix → plan: docs/superpowers/plans/2026-10-08-inbox-keys-fonts-view-rail.md Task 2
- [~] In terminal when typing e.g in OMP cannot Ctrl + Backspace to delete a text now just delete by character. And cannot Ctrl + A to select all to remove → plan: docs/superpowers/plans/2026-10-08-inbox-keys-fonts-view-rail.md Task 3
- [~] Should have icon the the agent that showing when agent inprogress, iddle, done, or ask me → plan: docs/superpowers/plans/2026-10-08-inbox-dots-drop-paste-redesign.md Task 1
- [~] Check if we don't have feature to drag drop a file into the terminal chat if not need to add in → plan: docs/superpowers/plans/2026-10-08-inbox-dots-drop-paste-redesign.md Task 2
- [x] Check if we can Ctrl+V to paste text or image into the terminal chat, if not need to add in → plan: docs/superpowers/plans/2026-10-08-inbox-dots-drop-paste-redesign.md Task 3
- [x] For the agent in the workspace do we have icon for each agent, if not yet then need to add in for better view → plan: docs/superpowers/plans/2026-10-08-inbox-dots-drop-paste-redesign.md Task 4
- [x] In case agent running, we're shoing circle loop icon, but I dont like that, I like dot rather than circle, can think a bout multiple dot aninamtion look beautifull → plan: docs/superpowers/plans/2026-10-08-inbox-dots-drop-paste-redesign.md Task 1
- [~] [feat] Use the new Mkrew logo everywhere (source: `icon/mkrew-1-lanes/`, picked `tile-dark.svg`) → plan: docs/superpowers/plans/2026-10-08-inbox-dots-drop-paste-redesign.md Task 11
  - Done: `mkrew.exe` icon (Explorer, window, taskbar, Alt+Tab) embedded via `crates/mkrew-ui/build.rs` + `resources/windows/mkrew.ico` (16–256 px).
  - Left: installer icon (`SetupIconFile=` in `packaging/mkrew.iss`, `.wxs` icon); `mk.exe` / `mkrew-daemon.exe` icons if wanted; logo (glyph) in the app (sidebar header / About); lockup in `README.md`.
  - Regenerate the `.ico` whenever the SVG changes (render 16,20,24,32,40,48,64,256 px).
- [~] I want to redesign the look of this GPUI desktop app so it feels modern, simple, and calm. This is a visual/UX refactor only: keep all existing behavior, actions, state, and data flow working exactly as they do now. → plan: docs/superpowers/plans/2026-10-08-inbox-dots-drop-paste-redesign.md Task 5–10
## Reference material
- `design/reference/theme.rs`, `design/reference/sidebar.rs`, `design/reference/app_shell.rs` are a reference implementation of the target design written against plain gpui primitives. Use them as the source of truth for layout, spacing, and colors, but adapt them to this codebase's real structs, actions, and GPUI version rather than pasting them in blindly.
- Screenshots of the current UI (if present) are in `design/reference/`.

## Before writing code
1. Explore the repo and find: the root window view, the workspace sidebar, the top bar (tabs, Run, History, + Agent, + Shell), the right-hand view rail (Board / Terminals / Docs / Files / Git), the empty "This tab has no panes" state, and the Inbox section.
2. Check which GPUI version we use (the `render` signature, `Context<Self>` vs `ViewContext`, `Application` vs `App`) and whether we have an icon/SVG asset setup or a `ui` component crate. Prefer real icons over unicode glyphs if they exist.
3. Find where colors are currently defined. If they are scattered, consolidate them.
4. Write a short plan that lists the files you'll touch and wait for my OK before making large changes.

## Target design

### Theme (dark + light mode)
Use `design/reference/theme.rs` as the model. It defines a `Theme` struct stored as a GPUI global, with `Theme::dark()` and `Theme::light()` palettes, a `ThemeMode` (System / Light / Dark), and an `ActiveTheme` trait so views can call `cx.theme()`.

Requirements:
- Every color in every view must come from `cx.theme()`. No hardcoded hex values in view code, and no `const` color values. Read the theme once at the top of `render` (`let t = cx.theme().clone();` or borrow it) and pass it into helper functions that build elements.
- Note that `design/reference/sidebar.rs` and `app_shell.rs` still use the old `const` colors (SIDEBAR, BG, TEXT, ...). Map each one to the matching `Theme` field (SIDEBAR → `t.sidebar`, BG → `t.bg`, etc.), and replace the old `tint(color, alpha)` calls for workspace avatars with `t.tint(color)`.
- Startup: call `theme::init(ThemeMode::System, window, cx)` before the root view renders. Load the user's saved mode if we have settings persistence; otherwise default to System.
- In the root view, subscribe with `cx.observe_window_appearance(window, ...)` and call `theme::sync_with_system`, so System mode follows OS changes live without a restart.
- Add a theme toggle as a ghost icon button in the toolbar, left of History. It cycles System → Light → Dark, and its icon or tooltip shows the current mode (monitor / sun / moon). If we have a settings screen, also add a "Theme" select there. Persist the chosen mode if settings persistence exists.
- If we have a keymap, add an action `ToggleTheme` (no default binding needed).
- The light palette must keep the same hierarchy as dark: the sidebar a shade off from the main bg, very soft dividers, the active segment reading as a raised white pill with a border, and the accent-filled Run and New agent buttons with white text.
- Check contrast in light mode: muted text on the sidebar and status labels must stay readable. Adjust the light values in `theme.rs` if something is too faint, and tell me what you changed.

Add to Verify:
- Switch between System, Light, and Dark at runtime and confirm every surface updates immediately, with no leftover dark pieces in light mode or the reverse.
- Change the OS appearance while in System mode and confirm the app follows.
- Check both themes with these states visible: hover, selected session, active tab, working pulse, the Inbox in both its empty and non-empty states, and the empty-state buttons.

### Layout
`[Sidebar 280px] | [Toolbar 44px / Tab strip 38px (Terminals only) / Content]`
- Remove the right-hand vertical rail entirely.

### Sidebar
- Header: "Workspaces" (12px, medium, muted) on the left and a + icon button on the right. Height 44px so it lines up with the toolbar.
- Workspace row (30px): chevron, an 18px rounded avatar with the first letter (tinted bg plus the workspace's color), and the name. If any sessions are working, show an accent pill with a pulsing dot and the count. The ⋯ button appears only on row hover (group_hover).
- Sessions are indented under a 1px left guide line. Session row (28px): status dot, name, status label, and elapsed time right-aligned.
  - Working: accent dot that pulses (opacity 0.35↔1, 1.4s).
  - Idle: hollow ring.
  - Needs input: amber dot.
- The selected session gets the selected background and a medium-weight name.
- Inbox: a card pinned at the bottom with an icon tile. When empty, show a green check and "Nothing needs you right now."; otherwise show an amber "!" and "N waiting for you".

### Toolbar (44px, bottom divider)
- Left: a segmented control with Board / Terminals / Docs / Files / Git, replacing the old right rail. The active segment gets the active-segment bg, text color, and medium weight.
- Right: History, + Agent, and + Shell as ghost buttons (muted text, hover bg), then Run as a split button: accent "▶ Run" plus a ▾ menu half separated by a faint white divider.

### Tab strip (38px, Terminals view only)
- The active tab is a raised pill (surface bg plus border). Inactive tabs show muted text with a hover bg.
- Tab close ✕ is always visible on the active tab and only on hover for the others. Clicking it must not activate the tab.
- Keep the existing new-tab "+" behavior.

### Empty state
Centered, with a max width of about 320px:
- A 44px icon tile
- A 15px medium title, "No panes in this tab"
- A muted line: "Start an agent or a shell, or pick a session from the sidebar."
- A primary "New agent" button and a secondary "New shell" button, each with a keyboard-shortcut chip. Use the real keybindings from the keymap; show ⌘ on macOS and Ctrl elsewhere.

Other views with no content should use the same empty-state component.

## Constraints
- Do not change business logic, persistence, or the action and keybinding definitions; only the views and styling.
- Factor out reusable pieces (ghost/primary/secondary button, kbd chip, empty state, status dot) instead of duplicating them.
- Keep text from wrapping in narrow spots (truncate with an ellipsis).
- Respect the existing focus and keyboard navigation.

## Verify
- Run `cargo check` and `cargo clippy` after each major step and fix all errors and warnings you introduced.
- Run the app and confirm the following still work: switching views, creating, closing, and switching tabs, Run and its menu, + Agent, + Shell, History, expanding and collapsing workspaces, selecting sessions, and the inbox states.
- Finish with a short summary of what changed, file by file, plus anything you couldn't match from the reference and why.
```
