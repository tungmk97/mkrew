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

## CSV and TSV tables (addendum, 2026-11-25)

- **Context:** Agents answer with data as ```` ```csv ```` blocks in Chat; the user wants to read them as tables, as Mermaid blocks are read as diagrams.
- **Decision:** ```` ```csv ```` (`,`) and ```` ```tsv ```` (tab) blocks are read by a small RFC 4180 reader in `markdown.rs` (`markdown::delimited`), in `Shown::from_text` next to the diagrams (`Shown.tables`), and drawn by the preview as a table on the code card.
  The card label says `csv · N rows`; Copy copies the source.
  The first record is the head, short records are padded, all-number columns are right-aligned, and each column starts as wide as its longest line (4 to 40 characters, `theme::doc::COLUMN_CHAR` pixels each) and all shrink together on a narrow card, so a small table stays compact on the left.
  At most 200 body rows are drawn, then "N more rows".
  The cells are selectable text pieces like a Markdown table's (`preview_select` copies them tab-separated).
- **Alternatives rejected:**
  - The `csv` crate: a new dependency for about 40 lines of reading.
  - Equal column widths (the Markdown table look): an id column took a third of the card and a short table spread over the whole card.
  - A virtual list for long tables: agents print short tables; the cap keeps a pasted file cheap.
- **Consequences:**
  - A block with an unclosed quote or no rows shows its source with the reason, like a diagram that cannot be drawn.
  - Semicolon-separated files are not detected; they show as one column.
