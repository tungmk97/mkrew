# Composer polish: icon glyphs, image thumbnails, no workspace stripe

- Date: 2026-11-25
- Status: approved by the human (2026-11-25, "approve trước các suggestion")
- Branch: `fix/composer-polish`

## Requests

1. A window per workspace (ADR 0115) draws a 3 px stripe of the workspace color along the top. The human does not want it.
2. In the omp composer, the placeholder that suggests the next message (`“<icon> TITLE: …”`) sometimes starts with a box: a glyph that did not render.
3. A pasted image shows in the composer as a `#1`, `#2` chip. Tern shows a small preview of the image above the field instead.
4. After sending, the image is drawn large (up to 256 px wide) in the message. It should stay small.

## Findings

- Request 2 reproduced in the smoke sandbox with a resumed omp session: omp's placeholder is `“\u{F03D8} TERN: …”`.
  `push_runs` already cuts private-use glyphs into runs in the terminal font, whose fallbacks list the bundled "Symbols Nerd Font Mono".
  But gpui's DirectWrite text system (`gpui-pre-windows` `direct_write.rs`, `layout_line`) sets the font fallback only from the first run's font for the whole line.
  The placeholder's first run is `“` in the UI font, which has no fallbacks, so the glyph falls back to the system fonts and draws as a box.
  It is "sometimes" because the box only shows when UI text comes before the glyph in the same line.
- The same bug hits Markdown text (`preview::text`): omp writes a user prompt's image references as `<icon> #1`, which drew as `▯ #1` in the sent message.
- Fallback mappings resolve family names in the system font collection only, so the bundled font is not reachable as a fallback on a machine without an installed Nerd Font.
- omp sends composer attachments as `card{role:"omp.composer.chip", head:"#n"}` holding an `image` (or the pasted text).
  A sent prompt's images are `row{role:"omp.user.images"}` of `image{alt:"#n", title:"#n", w, h, href?}` with no `max`, so Mkrew drew them at `TSP_IMAGE_MAX_WIDTH`.

## Design

- Glyphs: a run made only of private-use glyphs names the bundled icon font itself (`theme::ICON_FONT`, the first of `TERMINAL_GLYPH_FALLBACKS`), so no fallback is needed.
  - TSP text (`push_runs`): the glyph runs `tsp::with_icon_glyphs` cuts out get `ICON_FONT`.
  - Markdown (`preview::text`): each glyph gets a highlight of its own (`gpui::combine_highlights`) so it is a run of its own, and a font override to `ICON_FONT`; code spans keep the terminal font around their glyphs (`font_overrides`, unit tested).
- Thumbnails: a new token `TSP_THUMB_SIZE` (56 px). `thumbnail()` draws a square tile, the image cropped to fill (`ObjectFit::Cover`), `radius::MEDIUM`, a 1 px `border`.
  - Composer: a chip whose card holds an image draws the thumbnail instead of `#n`; hover turns the border `text_subtle`; the click still opens the preview panel with the full image. Text attachments keep the `#n` chip.
  - Sent prompt: an `image` whose parent has role `omp.user.images` draws the thumbnail; its title (`#n`) and click (open the file) stay.
- Stripe: removed from `render_space`; the OS title, the tabs and the switcher already name the workspace. ADR 0115 amended, SPEC §10 updated.

## Out of scope

- Claude and Codex Chat composers: a pasted image is typed as a path there, no chip.
- Fixing gpui's per-run fallback: the dependency is a registry crate; naming the font is enough for Mkrew's glyphs.

## Verification

- Unit test `icon_glyphs_get_the_icon_font_and_cut_a_code_span_around_them`.
- Smoke sandbox (`scripts/smoke.ps1`) with an omp session resumed from a copy of a real session holding two pasted images: the placeholder glyph, the `#n` glyphs, the thumbnails in the sent message, a pasted image in the composer, a window per workspace, dark and light.
