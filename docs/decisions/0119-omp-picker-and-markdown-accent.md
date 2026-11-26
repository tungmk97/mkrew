# 0119 - omp's picker sheet, and accent Markdown headings and bullets

- Date: 2026-11-26
- Status: accepted (amends ADR 0107 and ADR 0117; SPEC §11, §14)
- Spec: `docs/superpowers/specs/2026-11-26-tsp-picker-design.md`
- Protocol: no change (63). Migration: none.

## Context

With omp 19.2.1, every sheet omp opens over its page (the model switcher, `/agents`, the agent hub, sessions, the session tree, `/copy`, history search, logins, plugins, extensions, the hook selector) is one `picker` node under `layer` (TSP 19 `TspPickerProps`).
Mkrew did not know the kind, so a picker was drawn generically: its first text-like prop as one muted line, then its children, which are only the preview of the selected item.
The model switcher showed "Search, @ for roles..." and nothing else; `/agents` showed the selected agent's whole system prompt and no list.

ADR 0117 drew Markdown headings in `text` and bullets in `text_subtle`; the user found omp's answers harder to scan than in Tern, which colors both, and asked for the color back.

## Decision

- **Picker.** The pane draws a `picker` as Tern does (`shell/tsp_view/picker.rs`, model `mkrew_ui::tsp_picker`): a glass sheet sized by `size` (`md`, `lg`, `screen`) or rising from the composer (`anchor: "composer"`), the head (icon, title, subtitle, the search text omp keeps with its caret, the `12 of 1,604` count, head actions, Esc), the scope column or `rail`, tabs, column heads, the list (`rows`, `cards`, `timeline`, `tree`) with marks, icons, dots, hits, badges, chips and right-aligned facts, the preview (the picker's children) beside or below the list, the chip `strip`, the `level` slider, the action bar with keycaps, the `confirm` bar, and the loading, error and empty states.
  omp owns the keys (they reach it through the PTY), the filtering and the selection.
  A press sends what omp's own terminal sends: `select` on an item, `activate` on the selected one (or a double press), `action` with `act` `scope`, `tab`, `strip` or `level` and a `value`, or the action's id; Esc is `act: "close"`.
  Only the rows on screen are laid out (a `gpui::list`; the model switcher holds about 150 items, a full catalogue 1,600).
- **Item patches.** omp sends `items` once and patches it with `itemsAdd` (upsert by id) and `itemsDel`; `Document::set` folds them into `items` of a `picker` node and does not keep them as props.
- **Markdown color.** Headings and list bullets are `accent` in the Chat view, the omp pane and the Docs preview (ADR 0105's look, undoing that part of ADR 0117).
- Icon names omp uses in its sheets map to Lucide icons in `tsp::icon`; a `provider:<id>` mark draws its initials.
- **Images.** An `image` in the transcript is drawn whole at its own size, scaled down to the transcript's column when wider (`tsp::image_size`, the size read from the blob's header); before, it was cut at 256 px.

## Consequences

- The model switcher, `/agents` and omp's other sheets can be used with the mouse.
- The sheet has no animation (Tern's sliding selection bar, blinking caret, heated level track): the status loop would have to redraw the window while one is open (ADR 0086).
- Fact columns that do not fit beside a 200 px label hide lowest `priority` first, as in Tern; the label is cut only after the detail.
- An `elapsed` fact shows its age when omp sent it and does not count up.
- Rows have one height per layout and density.

## Rejected

- Draw omp's own Markdown colors (`mdHeading`, `mdListBullet`) from its theme palette, as Tern does: omp 19.2.1 never sends the palette over TSP (`packages/tui/src/native/connection.ts` `send` takes `TspVerb`, which has no `t`; a live session's snapshot had none), so Tern must resolve omp's theme by itself.
  Doing the same would mean reading omp's settings and carrying a copy of omp's built-in themes.
- Keep drawing pickers generically: the main sheets of omp 19 cannot be used.
- Filter in Mkrew: omp republishes `order` and `hits` per key; a second filter would disagree with omp's selection.
