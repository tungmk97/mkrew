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
  - `text` (the avatar letter) is adjusted until it reads on the workspace tints over `pane_gap`, `hover` and `surface_raised`; only a palette with a weak foreground changes.
  - `accent` is adjusted until it reads on `bg`, `surface` and `surface_raised` (the "code" rule); `accent_bg`, `selected_ring`, `pane_focus_stripe` and `accent_text` derive from the adjusted accent.
  - `text_subtle` is derived on the grounds of the "meta text" rule only (`bg`, `surface`, `surface_raised`, `composer_fill`); with the diff tints it collapses onto `text_muted`.
  - `accent_bg`, `pane_gap` and `composer_fill` join the shared grounds, so `accent_text` reads 4.5:1 on the pick fill and text reads on the page and the composer.
- **Tern Dark / Tern Light** are built-ins: the retuned Mkrew theme with Tern's blue accent, status, selection and ANSI colors. `ThemeEntry` takes a `Recipe` (`Mkrew`, `Tern`, `Palette(Palette)`); defaults stay Mkrew Dark / Light.
- **Popovers:** menus, tooltips, toasts and peek share `pop_surface` (glass, `layer[1]` ring, `shadow_pop`, `glass_hi`); picked rows are `accent_bg` plus a 2x14 bar (`with_pick`); the palette row is a `layer[0]` fill with an inset 2 px bar.
- **Controls:** focus is a 1 px accent border plus a 3 px halo at 18 % (amends 0085); `CONTROL_HEIGHT` 32; secondary buttons are raised fills with a ring; the switch is 36x22 with a white knob; input placeholders are t4 (`subtle`).
- **Pressables** hover to `layer[0]` and press to `layer[0]+layer[1]`; an element with its own fill flattens the layers over it (`pressable_on`); `quiet_button` has no hover argument.
- **Dialogs:** every dialog is a `dialog_frame` (icon tile head, body, foot band, primary with `kbd`); `error_note` replaces the `✕` rows; `DIALOG_WIDTH` 560; floating panels are `radius::PANEL`; selected rows use `with_pick`.
- **Settings:** 960x720 with a 232 px well navigation (an icon per page, groups in sentence case), a page head (`VIEW_TITLE` plus a one-sentence lead) inside the scroll, a 740 px content column, sentence-case section titles and 56 px rows.
- **Transcript rhythm (omp pane).** 16 px below every top-level row that draws something (`TSP_ROW_GAP`), at least 8 px inside an assistant turn, thinking rows 24 px high in BODY italic `text_subtle`, the working row 24 px in the text column.
- **Prompt bubble.** Tern's right-aligned bubble (`prompt_bubble`) in the omp pane and the Chat view; omp's head row moves under it on hover.
- **Chat Markdown.** Line height 1.6 for chat only (amends ADR 0116's 1.5; the UI stays 1.5), `text` headings and `text_subtle` bullets (amends ADR 0105's accent headings and dots), code cards sunken on `code_bg`, code lines 1.6 everywhere.
- **Tool rows.** No box around `omp.tool*` cards; their output sits on `code_bg` with a `layer[0]` ring.
- **Files tree.** Names `text_muted`, hover / cursor / opened `text`, ignored and dot files `text_subtle`; opened row = selected-row helper with an accent icon; 1 px `layer[1]` indent guides; icons by extension (`Icon::for_file`, six Lucide icons added from `lucide-static` 1.55.0: file-code, file-cog, file-image, file-json, file-spreadsheet, lock); git status = 6 px dot plus tinted name, the letter moves to the tooltip.
- **File pane header.** Breadcrumb instead of the full path; Save, Find, Open in Zed and Review in Docs are icon buttons; the unsaved mark is a dot on Save.
- **Docs flavor.** Column 960 → 720, body 14 → 15 at 1.5 → 1.65 (amends ADR 0116's 1.5), h1 32 / h2 22 with no rules, bullets `text_subtle` (amends ADR 0105's accent dot), ordered numbers terminal META `text_subtle`, quote gap 16, table header META `text_muted`, ring `layer[1]`, hr `my(XL)`.
- **Editor.** Gutter `text_subtle`, current number `text` (no accent), current line `row_hover` band (now a ground of the "code" contrast rule), caret `accent`, `code_line_height` 1.35 → 1.5. The accent only has to reach 3:1 on that band: it is the caret, a graphic (WCAG 1.4.11), and Tern Light's accent is 4.34:1 there.
- **Composer fit.** The chip-fit test counts the clock width and the cards' rings, so at 900 px with the branch chip shown the clock (the first part to drop) no longer fits; the stats still do. Without the branch chip both show.
- Components (shared controls, chat, composer, files, chrome, settings, git) follow in the same change; see the spec.

## Consequences

- `the_mkrew_themes_never_change` snapshots are re-accepted under this ADR; two new snapshots pin the Tern themes.
- Theme files keep loading (palette-only); derived themes change look because their tiers and layers are now derived.
- Mkrew Light `git_modified` / `git_untracked` move to `#8F5410` (4.5:1 on the new `selected`).
- No setting migration: `THEME_NAMES` only grows.
- The focus stripe among several panes is 1 px; needs input and a session colour stay 2 px.
- The sidebar sits on pane_gap without border_r; selected sidebar rows use the tint plus 2 px bar; the right panel's shown icon tab is accent_bg with an accent_text icon.
- Changed-line numbers keep at least 2:1 on their gutter, also in derived palettes (`diff_tone` raises the hue share until it does; the "diff numbers" contrast rule).
- A line on an accent-filled button is `accent_highlight`, not `glass_hi` (white at 100 % in light themes).
- Screenshots (smoke sandbox, fake agent, `docs/screenshots/`): `tern-polish-{split,files,menu,dialog,settings,git}-{mkrew-dark,mkrew-light,tern-dark,tern-light}.png` and `tern-polish-split-font18-mkrew-dark.png`. A fake agent has no Chat view and the sandbox has no omp, tracker or failing action, so the Chat view and toast shots were not taken.
- HUMAN_CHECKS "2026-11-26 Tern polish" lists what is left: omp pane transcript, tool rows, dock, composer fit, scroll-to-bottom button, Chat Markdown and composer, cards and lists (Jira), find bar and badges, and a second look at every screenshotted view on a real screen.

## Alternatives

- Only retune Mkrew, no Tern themes: rejected; the user chose both (Q1 = B).
- Keep `classic` for the sidebar: rejected; a second palette defeats the retune.
- Copy Tern's backdrop blur, gradient buttons, glows and animations: GPUI cannot blur, ADR 0086 bans continuous animation; see spec §11.
- Pane tiles with gaps and radius: rejected (Q2).
