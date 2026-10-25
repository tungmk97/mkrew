# 0078 — Theme picker and built-in themes

- Date: 2026-10-25
- Status: accepted
- Amends: 0027 (theme modes)

## Context

The user asked to change themes like VS Code / Zed. Mkrew had two palettes (dark, light) and a
System / Light / Dark mode.

## Options

1. Built-in themes only, each from ~20 palette colors, the rest derived. (Chosen.)
2. Every theme written token by token (~85 tokens × 10): long, and drifts when a token is added.
3. Theme files (TOML) or VS Code / Zed imports: not asked for (YAGNI).

## Decision

- 12 themes (`theme::THEMES`; names in `mkrew_core::settings::THEME_NAMES`). Mkrew Dark / Light
  are `Theme::dark()` / `Theme::light()` unchanged (golden snapshot test).
- `[ui] light_theme` / `dark_theme` name the theme of each slot (Zed's model); `[ui] theme` still
  picks the slot. Unknown names fall back to the slot's Mkrew theme.
- `Theme::from_palette` derives the tokens; UI text colors are blended toward the text color until
  they read ≥ 4.5:1 (`readable`), so some published colors come out lighter / darker. Terminal
  ANSI colors are as published. Status, workspace and QR colors and the classic sidebar palette
  stay Mkrew's.
- "Theme: Select theme…" (`Ctrl+Alt+J`; the keymap has no chords) reuses the command palette with
  live preview (↓ / ↑ preview a theme). Esc restores the previous colors; the modal does not close
  on an outside click. Enter or a click on a row saves to the slot shown (only `dark_theme` /
  `light_theme`).
- Settings → Appearance has "Light mode theme" / "Dark mode theme" rows (searchable list,
  `Kind::List`, the font pick generalised).

## Consequences

- Programs drawing 24-bit colors (Claude Code's and omp's own themes) keep their colors; ANSI 0–15
  and the default fg/bg follow the theme.
- Not done: answering OSC 10/11 color queries (INBOX).
- Palette changes made for contrast tests (theme, field, old → new):

  | Theme | Field | Old → New |
  |---|---|---|
  | One Dark | fg | ABB2BF → BDC3CD |
  | One Dark | surface | 2C313A → 292D35 |
  | Dracula | surface | 343746 → 242631 |
  | Gruvbox Dark | surface | 32302F → 222222 |
  | Gruvbox Light | chrome | EBDBB2 → EEDFB6 |
  | Gruvbox Light | surface | F2E5BC → FCF3CF |
  | Catppuccin Latte | surface | E6E9EF → F7F8FA |
  | Solarized Light | fg | 586E75 → 4A5C62 |
  | Solarized Light | surface | F5EFDC → FEFBF1 |
  | One Light | surface | F2F2F2 → FCFCFC |
  | GitHub Light | surface | F6F8FA → FDFEFE |

  Four light themes got a darker `accent` so accent text reads ≥ 4.5:1 on the background:

  | Theme | Accent old → new |
  |---|---|
  | One Light | 4078F2 → 3564C9 |
  | GitHub Light | 0969DA → 0965D1 |
  | Catppuccin Latte | 1E66F5 → 1C5EE1 |
  | Solarized Light | 268BD2 → 1E6FA8 |
