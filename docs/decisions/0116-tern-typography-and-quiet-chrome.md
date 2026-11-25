# 0116 - Bundled Geist, quieter chrome and one running mark per role (Tern look)

- Date: 2026-11-25
- Status: accepted (amends ADR 0051: Lucide stroke 2 → 1.5; ADR 0086: adds the orbit mark, still on the status tick; ADR 0106: tab chip look; ADR 0105: composer look)
- Milestone: none (human request 2026-11-25; spec `docs/superpowers/specs/2026-11-25-ui-typography-design.md`)

## Context

Side by side with Tern (stencil.so), Mkrew reads less clearly.
Measured on the same screen at 200 %:

- Mkrew asked for Inter but did not bundle it; the machine had only `Inter 18pt SemiBold`, so the UI fell back to Segoe UI.
  Tern bundles Geist (OFL) and sets 14 px / 1.5.
- Body rows in an omp pane peaked at luminance 137/255 against Tern's 204: a column of `text_muted` "Thought for 1s" rows, because omp hides finished tool calls and every assistant turn left only its thinking head.
- Six icon sizes, Lucide's stroke 2, brand-coloured logos and a status glyph in every tab chip.
- Up to five loading-dot marks inside one working omp pane.
- omp's composer placeholder showed a box: omp's private-use icon is drawn in the terminal font, and no installed Nerd Font matched `TERMINAL_GLYPH_FALLBACKS`.

## Decision

- **Fonts.** Geist Regular, Italic, Medium and SemiBold (OFL, `font/geist/`) and Symbols Nerd Font Mono (MIT, `font/nerd-symbols/`) are embedded and registered with `TextSystem::add_fonts` before the font list is read.
  `PREFERRED_UI_FONT` is Geist; `[ui] font_family` still wins when set.
  The Nerd Font symbols are the first glyph fallback already listed, so omp icons draw on every machine.
- **Type.** Every window root sets the UI font and line height 1.5 (`UI_LINE_HEIGHT`); Markdown body line height 1.6 → 1.5.
- **Icons.** `icons::Assets` serves Lucide with stroke 1.5 (`ICON_STROKE`).
  Icon sizes collapse to 16 (icon buttons), 14 (among text) and 12 (chevrons, tab and pin icons).
- **Running marks.** Two kinds, both stepped on `STATUS_TICK` (no continuous animation, ADR 0086):
  - three dots: only the dock's working row (the turn is running), `spinner` nodes of style `dots`, and short app loads (starting a pane, Jira, voice);
  - the orbit mark (a 12 px ring at 22 % with a 3 px dot moving one of 8 places per tick): `spinner` nodes of other styles, a progress of unknown amount.
  Everything else that used dots shows a still mark: the model pill none, a working checklist phase and the todo hud the active todo mark, a running subagent chip a still status dot.
- **Tabs.** A chip is 26 px, META, regular weight, one pane icon in a neutral color; its status recolors that icon (working / needs input) instead of adding an hourglass or bell.
- **Composer.** Effort moves into the model pill (glyph only); the pill is a ring with `text`; facts and usage are `text_subtle`; the context meter is a 1 px neutral line with no title; the send button is neutral and inverts on hover (never accent); Stop is a filled circle with a square; the working row loses its second Stop; the placeholder has no icon and no italic.
- **Chat list.** A thinking head is META italic `text_subtle` with the chevron after the label; a run of adjacent collapsed thinking rows (hidden rows between them skipped) shows as one row with the summed time, opened by a click.
  Inline code is `text` on a faint fill; table headers are `text_muted`.

## Consequences

- The binary grows by about 3.1 MB of fonts.
- Theme files keep working: only built-in token values and constants change.
- If registering the bundled fonts fails, the UI logs it and falls back to the system font.

## Alternatives rejected

- Inter bundled: Tern's clarity comes with Geist's narrower, taller shapes; Inter is wider at the same size.
- A continuous spinner animation (Tern's 0.9 s orbit): repaints the whole window every frame (ADR 0086).
- Collapsing thinking rows in the daemon: the UI owns how a list reads; the TSP tree stays omp's.
