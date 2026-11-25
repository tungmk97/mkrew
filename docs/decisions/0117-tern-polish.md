# 0117 - Tern polish: neutral grounds, four text tiers, layers and Tern themes

- Date: 2026-11-25
- Status: accepted (amends ADR 0116: composer pill ring -> fill, round Send/Stop -> key chip and Stop text button, Markdown line height 1.5 -> chat 1.6 / docs 1.65; ADR 0105: accent bullet -> `text_subtle`, accent headings -> `text`; ADR 0097: glass tint wash -> `glass_hi` line; ADR 0085: focus ring -> accent plus halo; ADR 0078: built-in list gains Tern Dark / Tern Light, `ThemeEntry` takes a recipe)
- Milestone: none (human request 2026-11-25; spec `docs/superpowers/specs/2026-11-25-tern-polish-design.md`)

## Context

Next to Tern, Mkrew's chat now reads as calmly (ADR 0116) but the rest of the app does not.
Measured from Tern's CSS (six gap reports):

- Mkrew's greys lean blue and are not ordered: `surface #121418` is darker than `bg #15171B`; Tern's `page < panel < card < raise` are neutral and strictly ordered.
- `text_dim #D5D8DD` is almost `text`, and `text_subtle == status_idle`; Tern has four tiers with fixed jobs (t1 `#EDEDED`, t2 `#A1A1A1`, t3 `#8A8A92`, t4 `#55555C`).
- Hover, pressed, divider and ring colors are hand-picked hexes; Tern derives all of them from one alpha ladder (7/10/15/24 % white, 6/9/14/22 % ink).
- Floating panels carry an accent wash (ADR 0097) and one two-layer shadow; Tern has a 1 px top highlight and separate pop / modal / composer shadows.
- The sidebar session list keeps a second, pre-restyle palette (`Theme::classic`) that clashes with any retune.

## Decision

- **Retune Mkrew Dark / Light** (grounds, lines, text tiers; accent family, status, brand, diff hues, syntax, ANSI, workspace colors kept). Values in SPEC §14. `pane_gap` stays at 1.06:1 minimum; its comment is corrected.
- **New tokens:** `layer[4]`, `code_bg`, `composer_fill`, `glass_hi`. `glass_tint`, `GLASS_TINT_*`, `glass_gradient`, `Theme::classic`, `classic_dark/light`, the "sidebar text" rule and `shadow_float` are removed. Shadows are `shadow_pop`, `shadow_modal`, `shadow_composer`. `radius::COMPOSER` is renamed `radius::PANEL`.
- **Floating panels:** fill = `surface_raised` (dark) / `surface` (light) at the contrast-loop alpha, `glass_border = layer[1]`, a 1 px `glass_hi` line inside the top edge (`widgets::top_highlight`), scrim black 55 % (dark) / `pane_gap` 50 % (light).
- **New contrast rules:** "meta text" (`text_subtle` 4.5:1 on bg, surface, card, terminal_bg, surface_raised, composer_fill), "accent text" (`accent_text` on `accent_bg`), "glass lift" (dark glass is lighter than `bg`), "menu pick" (`accent_bg` differs from the glass fill; replaces the unsatisfiable spec wording "`selected` differs from `surface_raised`").
- **Derived themes** (`from_palette`, theme files) get the layers, `code_bg`, `composer_fill`, a real t2/t3/t4 split and `sidebar = pane_gap`.
- **Tern Dark / Tern Light** are built-ins: the retuned Mkrew theme with Tern's blue accent, status, selection and ANSI colors. `ThemeEntry` takes a `Recipe` (`Mkrew`, `Tern`, `Palette(Palette)`); defaults stay Mkrew Dark / Light.
- Components (shared controls, chat, composer, files, chrome, settings, git) follow in the same change; see the spec.

## Consequences

- `the_mkrew_themes_never_change` snapshots are re-accepted under this ADR; two new snapshots pin the Tern themes.
- Theme files keep loading (palette-only); derived themes change look because their tiers and layers are now derived.
- Mkrew Light `git_modified` / `git_untracked` move to `#8F5410` (4.5:1 on the new `selected`).
- No setting migration: `THEME_NAMES` only grows.

## Alternatives

- Only retune Mkrew, no Tern themes: rejected; the user chose both (Q1 = B).
- Keep `classic` for the sidebar: rejected; a second palette defeats the retune.
- Copy Tern's backdrop blur, gradient buttons, glows and animations: GPUI cannot blur, ADR 0086 bans continuous animation; see spec §11.
- Pane tiles with gaps and radius: rejected (Q2).
