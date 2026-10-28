# 0027 — Calm redesign: toolbar, theme modes, new tokens

- Date: 2026-10-08
- Status: accepted
- Milestone: inbox 2026-10-08
- Supersedes: 0023
- Amended by: 0034 (the toolbar became part of the custom title bar)

## Context

The user asked for a modern, simple and calm look, with more room for content, as a visual/UX
refactor only (inbox 2026-10-08, reference files in `design/`). It asks for a top toolbar, a
System/Light/Dark mode that persists, a quieter sidebar, a close button per tab and a helpful
empty state. ADR 0023 had just moved the view switcher into a rail on the right.

## Options

1. Keep the right rail (ADR 0023) and only restyle.
2. A 44 px toolbar at the top of the main area, as the reference shows (views as a segmented
   control, Run, theme toggle); the rail goes.

## Decision

Option 2.

- `Theme` is a GPUI global (`cx.theme()`); `[ui] theme = "system" | "light" | "dark"` picks the
  mode, `system` follows the OS live. The toolbar toggle and `ToggleTheme` cycle
  System → Light → Dark and write the key, keeping the rest of `settings.toml` byte for byte; a
  file that does not parse is not written (the palette lists why).
- `ToggleTheme` has the default `Ctrl+Shift+J`: every command needs a default (keymap tests);
  the key was unused.
- Colors stay `u32` tokens (`rgb(theme.x)`), so views keep their call style. New tokens:
  `sidebar`, `card`, `hover`, `selected`, `divider`, `text_dim`, `subtle` (graphics only),
  `accent_text`, `accent_bg`, `on_accent`, `on_accent_divider`, `warning_bg`, `success_bg`,
  `tint_alpha`, `scrim` (was the `DIALOG_SCRIM` const).
- Layout heights are rems of the 13 px body (`theme::rems_px`), so they grow with
  `[ui] font_size` (ADR 0024) instead of clipping.

Values changed from `design/theme.rs` (contrast tests):

| Token | Reference | Used | Why |
| --- | --- | --- | --- |
| dark `accent` | #7C83FF | #7C8CFF (kept) | the reference fails 4.5:1 for code/links on `surface-raised` |
| light `accent` | #5B63F0 | #4F5BD5 (kept) | the reference fails 4.5:1 on `surface` |
| dark `text_muted` | #8B8F9A | #9A9CA6 (kept) | the reference fails 4.5:1 on `selected` |
| light `text_muted` | #6B7080 | #5F6473 | the reference fails 4.5:1 on `hover` |
| dark `on_accent` | #FFFFFF | #17181C | white on the light dark-mode accent is 2.98:1 |
| light `surface_raised` | #FFFFFF | #EEF0F3 | the selected code line must differ from `bg` |
| derived `pane_focus_border`, `terminal_selection_bg` | — | re-mixed into the new `bg` | SPEC §14 rule |

## Consequences

SPEC §10 (layout), §11 (sidebar rows), §13 (Toggle theme), §14 (tokens, theme modes) and §20.0
(`[ui] theme`) change. The rail of ADR 0023 is gone.

- The ⋯ menu button shows on every workspace row on hover (always on the active row).
- A selected session name is no longer accent-colored: `selected` fill + medium weight.
- + Agent / + Shell live in the toolbar in every view and switch to Terminals first.
- Status glyphs stay wherever no text label sits beside them (tab label, §11).
- Not matched from the reference, and why:
  - the single pulsing dot became three dots stepping on the 500 ms status tick (a GPUI
    animation redraws the whole window every frame); ADR 0086 later made the working mark a still colored dot too;
  - SVG icons became Unicode glyphs (no icon asset pipeline);
  - the empty state's `Ctrl+Shift+S` chip shows the real `NewShellTab` key (Ctrl+Shift+T), and
    the chips name the new-tab shortcuts while the buttons add a pane to the current tab;
  - the System glyph is ◐ (monitor 🖵 is not in every Windows UI font);
  - the session row keeps its combined status text (`working · 2m`) instead of a separate
    right-aligned time.
