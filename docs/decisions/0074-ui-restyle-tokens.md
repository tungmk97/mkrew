# 0074 — UI restyle: new tokens, radius scale, shared controls

- Date: 2026-10-24
- Status: accepted (SPEC §14)
- Milestone: none (user request 2026-10-24)

## Context

The user restyled the app screen by screen (title bar, sidebar top, Board, Terminals, Files, Git)
with a new dark palette, a finer radius scale, taller controls and shared components. The sidebar
session list (workspace cards, session rows, RUN groups) was explicitly kept as it was.

## Decision

- Dark palette: chrome #0F1013 (title bar, sidebar), panel #121418 (`surface`), content #15171B
  (`bg`), raised #1C1F24 (`card`), selected #2A2E35, border #23262C, divider #1E2126, terminal
  #0B0C0E, text #ECEDEF / #A3A8B0 / #7D838C (`text_subtle`, the dimmest text allowed), accent
  #8E8CF5, info #6AA6F2, success #57C78A, attention #E4A452, danger #EF7B6C. Tints are the same hue
  at ~14 % (`Theme::tint`). The light palette follows the same roles (chosen by the agent, every
  pair keeps the existing contrast tests).
- The pre-restyle palette stays as `Theme::classic` and is used only by the sidebar session list
  and the compact rail, so they look exactly as before.
- Radius 5 (area chips, key hints) · 6 (chips, segments, tree rows) · 8 (buttons, inputs, rows) ·
  9 (segmented container) · 10 (cards) · 12 (dialogs) · 999 (pills); 4 stays for old badges.
- Heights: controls 30, inputs 32, title bar 48, tab strip 44, Git repo strip 42.
- Shared controls in `shell/widgets.rs`: segmented control, primary / secondary / ghost /
  destructive buttons, 30 px icon button, search box, status pill, tinted chip, overline, key hint.
  Overlines are upper case (the old "no all-caps text" rule is lifted for them only).
- Fonts: Inter when installed and `[ui] font_family` is unset, else the system font (not
  bundled); JetBrains Mono first in the terminal / code font list.
- Syntax: a `Key` token for JSON / YAML mapping keys, drawn in `info_soft` (#93BCF6) in the Git
  diff, the Files viewer and Docs code blocks.

## Consequences

- Every button, input and key hint in the app (dialogs too) takes the new look, since they share
  the widgets.
- A machine without Inter keeps the system font; bundling it is a later choice.
- Letter-spacing (0.06 em on overlines) is not available in GPUI and is skipped.
