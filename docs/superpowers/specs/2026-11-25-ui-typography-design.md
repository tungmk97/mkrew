# UI typography and quiet chrome (Tern look) - design

- Date: 2026-11-25
- Branch: `feat/ui-typography` (worktree `.worktrees/ui-typography`)
- ADR: 0116
- Approval: the user picked option 5 (fonts + type/icon system + placeholder box fix), Geist with the `[ui] font_family` override, Tern-like running marks (option 2), and asked to also bring the chat, tab and composer styling and behavior closer to Tern; he accepted the suggestions up front and asked for a report at the end.

## 1. Goal

Mkrew reads as clearly as Tern on the same screen: crisp text, quiet icons, one thing moving where something runs.
Success: a screenshot of the same views next to Tern shows body text at Tern's brightness, gray icons, one tab icon per chip, a composer row of at most model + a few quiet facts, and no wall of "Thought for 1s" rows.

Constraints kept: every value from `theme.rs`; spacing 4/8/12/16/24/32; dark and light themes; no continuous animation (ADR 0086, the status tick redraws); theme files keep working; the terminal font stays the user's.

## 2. Evidence

Tern's CSS was read out of `tern.exe` (Geist, `14px/1.5`, t1 `#ededed`, icons 16 px stroke 1.45).
Three comparison reports (Tern CSS line numbers, Mkrew functions, gap tables) drove sections 6-8.
A `tsp.snapshot` of a live omp pane showed why the omp list is gray: omp marks finished tool calls `hidden`, so each assistant turn leaves only its collapsed `omp.thinking` section (`took` in ms, head text "Thought for 1s").

## 3. Fonts (ADR 0116)

- `font/geist/` holds Geist Regular, Italic, Medium, SemiBold (OFL, `LICENSE`); `font/nerd-symbols/` holds Symbols Nerd Font Mono (MIT, `LICENSE`).
- `theme::BUNDLED_FONTS` embeds them; `main.rs` registers them with `cx.text_system().add_fonts` before `Fonts::from_settings` reads the font list. A failure is logged; the UI then uses the system font.
- `PREFERRED_UI_FONT = "Geist"`. `[ui] font_family` set → that font, as before.
- The omp placeholder box: omp's private-use icons already go to the terminal font with `TERMINAL_GLYPH_FALLBACKS`; its first entry, Symbols Nerd Font Mono, is now always present.

## 4. Type and icons

- Every window root (`render_space`, the detached window) sets the UI font family and `line_height(UI_LINE_HEIGHT = 1.5)`.
- `doc::LINE_HEIGHT` 1.6 → 1.5.
- Lucide stroke 2 → 1.5 (`ICON_STROKE`, rewritten in `icons::Assets::load`).
- Icon sizes: 16 icon buttons, 14 among text (`TREE_ICON_SIZE`, `BOARD_TYPE_ICON` now alias it), 12 chevrons / tab / pin (`PIN_ICON_SIZE` aliases `TAB_ICON_SIZE`).

## 5. Running marks

Two looks, both stepped on `STATUS_TICK`:

| Mark | Where |
|---|---|
| three dots (`loading_dots`) | the omp dock's working row only (with the elapsed time); `spinner` nodes with `style: "dots"`; transient app loads outside panes' content (starting a pane, Jira, voice, the `+` row) |
| orbit (`orbit_mark`, new) | `spinner` nodes of any other style (tone color, default `accent`); `progress` without a value |
| still marks | model pill: nothing; working checklist phase and the todo hud: the active todo mark (accent ring + dot); running subagent chip (Chat view): `status_dot(status_working)` |

`orbit_mark(now, color)`: a 12 px ring, 1.5 px line in `color` at 22 %, a 3 px dot at one of 8 places (`status::orbit_step`).

## 6. Tab chips (`pane_render::tab_chip`, `widgets::tab_frame`, `tabs::tab_icons`, `chrome.rs`, `plus_row.rs`)

- Height `TAB_CHIP_HEIGHT` 26, `px(space::M)`, icon-title gap `space::S`, `font_size::META`, weight regular in every state (rename field too).
- One pane icon per chip (the focused pane's); no `+N`.
- Icons (agent logos included) in `text_subtle`, `text_muted` on the active chip; no brand colors in chips.
- Status recolors that icon: working `status_working`, needs input `status_needs_input`; the hourglass / bell glyph goes. The chip's tooltip still names the state.
- Active fill: built-in dark themes' `tab_active` moves toward Tern's 10 % white over the bar (still passing `theme::problems`).
- Drop the rule between pinned and other tabs and the rule after the tab area; keep the one after the left group.
- `+` open: its row of items sits on one `hover` pill.

## 7. Composer and dock (`composer.rs`, `tsp/dock.rs`, `tsp_view.rs` dock functions)

- Effort: no separate chip; its glyph leads the model pill, the label goes in the pill's tooltip (`fit_bar` and widths updated).
- Model pill: `COMPOSER_PILL_HEIGHT`, a 1 px `border` ring, no fill, `text` at MEDIUM, chevron 12 `text_muted`; no loading dots.
- Facts and usage: `text_subtle` (icons too); colors omp put in spans stay; tone warning / error keep `status_needs_input` / `danger_text`.
- Context meter: `CONTEXT_METER_HEIGHT` 1, fill `text_muted` (tone colors past the thresholds), labels (percent, total) above the line, no session title; marks only while the meter is hovered.
- Send: `SEND_BUTTON_SIZE` 32, neutral fill, `text` glyph; hover inverts (`text` fill, `surface_raised` glyph); never accent; same look with or without text (disabled dims).
- Stop: a `text`-filled circle with a `STOP_GLYPH_SIZE` square in `surface_raised`; hover `text_muted`.
- Card: `radius::COMPOSER` 16, padding top M, bottom S, left L, right M.
- Placeholder: no sparkle icon, not italic, `text_subtle`.
- Working row: label `text_muted` at BODY, elapsed `text_subtle`; the ghost Stop goes (the card's round Stop is the one).
- Hud pill: no fill, `border` ring, segments with the `Chip` hover.
- The Chat view's composer shares `model_pill`, `meter_bar`, `round_button`, `placeholder_row` and gets the same look.

## 8. Chat list (`tsp_view.rs` list functions, `tsp.rs`, `chat_view.rs`, `preview.rs`)

- Thinking head (`omp.thinking*`): META, italic, `text_subtle` (hover `text_muted`), chevron 12 after the label.
- Thinking runs: top-level rows whose only shown content is one collapsed `omp.thinking` section, with only hidden rows between them, show as one head: "Thought for <sum of `took`>" (omp's rounding: whole seconds, at least 1 s, `format_duration`), or "Thought N times" when a member has no `took`.
  A click opens the run (each member as today); a click on the lead closes it. Pure grouping in `tsp.rs` with unit tests; UI-only open state like `todo_toggled`.
- Tool cards (`omp.tool.*` cards that are shown): chevron only on head hover.
- Inline code in chat Markdown: `text` on a faint fill (no `accent_text`); table header `text_muted` MEDIUM, row lines `divider`.
- Checklist: no dots (section 5); phase title `text` (`text_muted` when done).

## 9. Testing

- Unit: `orbit_step`; thinking-run grouping (runs across hidden rows, a shown tool or text breaks a run, sums, the "N times" fallback); `fit_bar` without the effort chip; `theme::problems` on every built-in theme after the token changes.
- Screens: `scripts/smoke.ps1` dark and light shots of a split with a fake agent pane, the tab bar, Settings → Appearance (font list shows Geist), saved as `docs/screenshots/ui-typography-<view>-<theme>.png`; an omp pane cannot run in the sandbox, so its dock and list go to `docs/HUMAN_CHECKS.md`.
