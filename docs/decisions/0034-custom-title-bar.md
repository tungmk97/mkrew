# 0034 — Custom title bar replaces the OS title bar and the toolbar

- Date: 2026-10-10
- Status: accepted
- Milestone: inbox 2026-10-10
- Amends: 0027

## Context

ADR 0027 put a 44 px toolbar (views, History, + Agent, + Shell, Run) under the OS title bar.
Together they take ~76 px of height, the logo sits in the sidebar header, and the toolbar shows
neither the workspace nor its git state. The inbox asks for one bar: logo, workspace switcher,
branch, views, History, + Agent, + Shell, Run and drawn window controls. A folder holding
several repositories (ADR 0029) must show a branch and a `Git N` badge too.

## Options

1. Keep the OS title bar and only add the branch and the switcher to the toolbar — saves no
   height; the OS bar stays a dead strip.
2. One custom title bar on every platform, with client-side decorations on Linux — Linux needs
   resize edges, shadows, tiling insets and a window menu drawn by us, and X11 without a
   compositor falls back to the OS bar anyway; none of it can be tested here.
3. A custom title bar on Windows and macOS, the OS bar on Linux — the platforms GPUI gives
   everything for: on Windows `HTCAPTION` / `HTMAXBUTTON` / `HTMINBUTTON` / `HTCLOSE` through
   `WindowControlArea`, on macOS the native traffic lights.

## Decision

Option 3.

- The bar is 44 px (`TITLE_BAR_HEIGHT`) and holds, left to right: logo, workspace name ▾
  (a dropdown of the workspaces and "Add workspace…"), the branch chip, the views with the
  `Git N` badge, a drag area, the theme toggle, History, + Agent, + Shell, Run and the window
  controls. It shows without a workspace too (logo, drag area, controls), so the window can always
  be moved and closed.
- Windows: `TitlebarOptions.appears_transparent = true`. The drag area is a
  `WindowControlArea::Drag` (system move, double-click maximizes or restores). ─ □ ✕ are
  `WindowControlArea::Min` / `Max` / `Close` without click handlers: the native hit test
  (`HTMAXBUTTON`) is what shows the Windows 11 Snap Layouts flyout on □ and runs the action. The
  drag area is a sibling of the controls, never their parent (the first registered area wins a
  hit test). Resize edges come from GPUI's frame hit test and the system frame. The controls
  are not drawn in full screen (F11).
- macOS: `appears_transparent = true` keeps the native traffic lights (72 px reserved at the
  left); the drag area calls `Window::start_window_move` and, on a double-click,
  `Window::titlebar_double_click`; `app_owns_titlebar_drag = true`.
- Linux: the OS title bar stays and our bar is a toolbar below it (no drawn controls, no drag
  handling). Client-side decorations are not worth an untestable ~150 lines now.
- The minimum window size is 1100 × 560 so the bar never overflows; the workspace name and the
  branch truncate first.
- The branch chip reads a cached `GitSummary` of the shown workspace (1 repo: `⎇ master`; N
  repos: `⎇ N repos`, tooltip lists each; a root without `.git` but with child repos is never blank).
  It is loaded on a workspace switch and on `git.changed` of that workspace only: no polling,
  nothing for hidden workspaces.
- The sidebar header loses the logo and shrinks to 30 px (`WORKSPACE_ROW_HEIGHT`).
- Run controls are green (Start), amber (Restart) and red (Stop) in the bar and in the menu.

## Consequences

- SPEC §10 (layout diagram, "Title bar" bullet, Run button) and §14 (theme toggle) change; ADR
  0027's toolbar is part of the title bar now (0027 gets an "Amended by" line).
- Pressed (mouse-down) styling of ─ □ ✕ on Windows is the system's: only hover is drawn.
- Under the top 1–2 px of the Close button the system resize edge (`HTTOPRIGHT`) wins.
- A Linux user gets two stacked bars (OS bar + toolbar); `window_decorations` can switch to
  client-side later without touching the bar's content.
- Dialogs and their shade are drawn inside the sidebar | main row, below the bar: a shade over the
  bar would block the window-control hit areas, so the window stays movable and closable while a
  dialog is open. The bar's other controls are covered by an inert layer while a dialog is open
  (a modal keeps the mouse as it keeps the keyboard).
- Tests: the bar itself is checked by hand (HUMAN_CHECKS); the git summary behind the chip and
  badge is unit tested (`git_summary`).
