# 0021 — Docs preview rendering

- Date: 2026-10-07
- Status: accepted
- Milestone: none (outside the milestone loop; workspace/files/docs fixes)

## Context

The user found the Docs preview hard to read: the UI's 13 px body with default line height,
h3–h6 at body size, proportional inline code, unhighlighted 12 px code blocks, a left-aligned
column. SPEC §12 said "No syntax highlighting in v1". The user also wants Mermaid diagrams.

## Decision

- Doc typography tokens (`theme::doc`): body 14 px / line height 1.6, headings
  24/20/17/15/14/14 px / 1.3 (h1–h2 underlined), code 13 px / 1.45, a 20 px list marker column;
  the preview is a centered column of at most 720 px.
- Inline code uses the monospaced (terminal) font.
- Fenced code blocks are highlighted by syntect by their fence language, with the same token
  colors as the Files viewer (`highlight::fenced`; `ts` reads as JavaScript, which the bundled set
  lacks); unknown languages and over-limit blocks stay plain.
- `doc.read` answers are parsed and highlighted once, on the background executor (`Shown::new`
  builds `Shown.code`, keyed by a block's first source line), not on the UI thread.

## Consequences

SPEC §12 and §14 updated. Opening a doc no longer blocks the UI on parsing. Skipped: clickable
links, inline images, per-depth bullet glyphs, horizontal scrolling of code (long lines wrap).

## Mermaid diagrams (addendum)

- **Decision:** ```` ```mermaid ```` blocks are drawn by `mermaid-rs-renderer` 0.3.1
  (`default-features = false`; pure Rust, MIT) to SVG in the theme's colors, one layout per
  appearance (some layouts bake the colors in), in `Shown::new` (off the UI thread;
  `Shown.diagrams`), shown with `gpui::img`.
- **Alternatives rejected:**
  - mermaid-cli/Node: needs a browser, which breaks "lightweight" and §1.
  - A WebView: not GPUI.
  - Showing the source only: that is what the user complained about.
- **Consequences:**
  - One more crate, with its own fontdb. It loads the system fonts once, which takes seconds in
    debug. Measure the UI memory against the 150 MB budget in the manual check.
  - Blocks that do not parse or use an unsupported diagram type show their source with the
    reason.
