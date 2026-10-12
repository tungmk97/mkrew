# 0051 — Icon buttons draw Lucide SVG icons

- Date: 2026-10-12
- Status: accepted (reverses ADR 0027's "SVG icons became Unicode glyphs" for icon buttons)

## Context

Icon-only buttons showed Unicode glyphs (`◷ ◧ ◫ ⋯ ⌕ ⇤ ✦ …`) in the UI font. Each glyph came from
a different fallback font, with different weights, sizes and baselines; some looked wrong on
Windows. The human asked for better-looking icons from an existing set, not hand-drawn ones.

## Decision

- Lucide (`lucide-static` 1.55.0, ISC; license in `icon/lucide/LICENSE`): 49 SVGs vendored in
  `icon/lucide/`, embedded with `include_bytes!` by `crates/mkrew-ui/src/icons.rs` (`Icon`) and
  served to gpui by `icons::Assets` (`Application::with_assets`).
- Drawn with `gpui::svg()` as a monochrome mask in a theme token color (`icon_svg`), so one file
  serves dark and light. Sizes `ICON_SIZE` 16 (icon-only buttons) and `ICON_SIZE_SMALL` 14
  (among text, rows, window controls, status bar), scaled with `[ui] font_size`. Stroke width is
  Lucide's 2.
- In scope: icon-only buttons, toggles, window controls, row actions, row chevrons; Close pane
  becomes an icon-only `square-x`. A 14 px icon goes before the text of frequent toolbar,
  header, card and empty-state actions: Share, Turns…, Show all panes, `+ New ▾`, idle Run and
  its `▾`, the workspace switcher `▾`, New agent / New shell / Add workspace / New task / Open
  prompts folder, card Undo / Changes / Compare / Run with… / Several agents…, Comment, Send to
  agent, Open in Zed, Review in Docs, Save, Find, Discard…, Push, Generate, `By: … ▾`, Open
  transcript, Link plan… / Unlink, Prompts…, Run setup Duplicate / Remove / Add command / Up.
- Text only: dialog footers, Retry, empty-state navigation, comment thread actions, Replace,
  Reload / Keep mine, Restore before / Diff since, row actions (Stage, Review diff), the Ignored
  chip (ADR 0050), danger buttons; status icons (§11), marks inside messages, menu checkmarks,
  the live Run label, agent badges and the pin. An icon on every button is noise (§14).
- Adding an icon: download `https://unpkg.com/lucide-static@1.55.0/icons/<name>.svg` into
  `icon/lucide/` and add one `Variant => "<name>",` line to the `icons!` list.

## Alternatives rejected

- `gpui-kit` (gpui-component) `Icon`: the same Lucide set behind a whole component library and its
  asset crate.
- Segoe Fluent Icons font: Windows 11 only; Linux has no such font.
- Better Unicode glyphs: still one font per glyph, still uneven.

## Consequences

The binary grows by about 25 KB. An SVG gpui cannot load draws nothing (blank button): the
`icons` test checks that every `Icon` is served and is a 24 px Lucide SVG.
