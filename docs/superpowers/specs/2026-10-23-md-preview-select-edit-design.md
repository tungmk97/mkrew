# Select, copy and block edit in the Files Markdown preview — design

Date: 2026-10-23 · Status: approved in chat, awaiting spec review · ADR: 0072 (number to re-check)

## Intent

The user reads `.md` files in the Files view preview. Today the preview cannot be selected,
copied or edited (SPEC §12A: "editing only happens there", i.e. in Source). The user wants to:

- select text freely across blocks with the mouse and copy it (Ctrl+C), as in a browser;
- edit a passage without leaving the preview.

Success: drag-select across a paragraph, a table and a list, Ctrl+C, paste into Notepad →
readable plain text (table cells tab-separated). Hover a block → ✎ → edit its Markdown in
place → Ctrl+Enter → the preview shows the change, the file is unsaved (●), Ctrl+S saves,
Ctrl+Z in Source undoes it in one step.

## Decisions (from the user)

| Topic | Decision |
|---|---|
| Scope | Files view preview only. Docs view unchanged (keeps block picking for comments). |
| Copy | Free character selection across blocks; Ctrl+C copies the **rendered text**. |
| Edit | Per **top-level block**, in place: ✎ on hover → multi-line field with the block's source; Ctrl+Enter applies, Esc / click outside cancels. |
| Approach | A: pure selection model + GPUI `TextLayout` hit testing; renderer kept. |
| Execution | Implementation runs in a git worktree. |

Defaults chosen by the agent (not asked): Ctrl+A selects the page; double-click selects a
word; Esc clears the selection; no autoscroll while dragging past the edge.

Out of scope: editing a single list item or table cell, autosave, Docs view selection/edit,
autoscroll during a drag, triple-click.

## Part 1 — Selection model (pure)

New module `crates/mkrew-ui/src/preview_select.rs`, built on `markdown.rs`.

**Pieces.** Each top-level block flattens into ordered text pieces, in the exact order
`preview::block` draws its `StyledText`s:

| Block | Pieces | Separator when copied |
|---|---|---|
| Heading, Paragraph | 1 (`inline(runs).0`) | — |
| List | per item: its child blocks' pieces (recursive); prefix `- ` / `N. ` / `[x] ` / `[ ] ` added on copy only | `\n` |
| Quote | child blocks' pieces (recursive) | `\n` |
| Table | one per cell, head row first | `\t` between cells, `\n` between rows |
| Code | one per code line | `\n` |
| Mermaid diagram, Rule, Html | none (not selectable) | — |

List markers are drawn separately, so they are never selectable.

**Types.**
```rust
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Point { pub block: usize, pub piece: usize, pub byte: usize }
pub struct Selection { pub anchor: Point, pub head: Point }
```

**Functions.**
- `flat(block, shown) -> Flat { text, pieces: Vec<Range<usize>> }`: the block's copied text
  and where each piece sits in it; computed on demand (copy, select all, double-click), never
  per frame — drawing needs only `range_in`.
- `Selection::range_in(block, piece) -> Option<Range<usize>>`: the selected bytes of one
  piece (end `usize::MAX` = to its end), for highlighting.
- `copy_text(shown, &Selection) -> String`: the selected text with piece separators and list
  prefixes; `\n` between blocks.
- `select_all(shown) -> Option<Selection>`; `word_at(shown, Point) -> Selection` (run of the
  same `text_box::class`, no new dependency).

**Invariants.** `byte` is always on a char boundary (Vietnamese text never split). The
selection is cleared whenever the page is rebuilt (edit applied, file re-read, another
file), since block indices may change.

## Part 2 — Rendering and mouse (`preview.rs`, Files only)

`preview::page(...)` gains `select: Option<Select>`; Docs passes `None` (no change).

```rust
pub struct Select {
    pub sel: Option<Selection>,
    pub hits: Rc<RefCell<HashMap<(usize, usize), TextLayout>>>, // (block, piece) → layout
    pub on_select: Rc<dyn Fn(Selection, &mut Window, &mut App)>,
}
```

- `hits` lives in `Page`, cleared before each list paint; it holds only on-screen blocks.
- `text(runs, theme)` and `code_line` take `selected: Option<Range<usize>>`, merged into the
  highlights with background `theme.terminal_selection_bg` (dark and light already defined).
- Each selectable `StyledText` registers `styled.layout().clone()` under `(block, piece)`.
  `block()` / `list_item()` thread a `&mut usize` piece counter in the same order as
  `pieces()`.
- Pointer over text: `cursor_text` (I-beam).

| Mouse | Effect |
|---|---|
| Press | Find the layout in `hits` whose `bounds()` contains the point; byte via `index_for_position` (`Err(i)` → nearest `i`); `anchor = head`. Press on empty space clears the selection. |
| Drag (left held) | Update `head`. Off any text: the nearest visible piece vertically (byte 0 above, end below). |
| Shift+press | Keep `anchor`, move `head`. |
| Double-click | Select the word (`word_at`). |
| Wheel during drag | The list scrolls; the next move updates `head`. No edge autoscroll. |

Risk: the gpui-pre 0.3.7 `TextLayout` API (`bounds`, `index_for_position`) must be checked
against the crate source before coding (AGENTS.md: never guess crate APIs).

## Part 3 — Keyboard and block edit

**Keyboard.** The preview page tracks `file_editor_focus` with its own handler
`file_preview_key_down` (the editor and its handler are not drawn in Preview):

| Key | Effect |
|---|---|
| `EditKey::Copy` (Ctrl+C) | `copy_text` to the clipboard; nothing selected → nothing. |
| `EditKey::SelectAll` (Ctrl+A) | Select the page. |
| Esc | Clear the selection. |
| Any other edit key (typing, Paste, Cut, Delete…) | Ignored: the hidden editor must not change. |

A press in the preview focuses `file_editor_focus`. The edit field stops presses and drags
from reaching the page (they would steal its keyboard and move the page selection).

**Block edit**, through the existing `preview::Wrap` hook (Files passes `None` today):
- New state in `FilesView`: `block_edit: Option<BlockEdit { block: usize, lines: Lines,
  stamp: Stamp, text: TextBox }>` (`TextBox` = the card follow-up field's type).
- The Files `Wrap` adds a ✎ button (Lucide `pencil`, added) at the block's right, shown on
  hover (`group_hover`). The block being edited is replaced by a `caret_field` (caret, click,
  drag-select) + `typing_input` (IME), max height `theme::PROMPT_FIELD_HEIGHT`, then scrolls.
  Both get `_for(&Entity<Shell>)` variants, since the `Wrap` closure has no `Context`.
- Open: text = `editor.lines()[start-1..end].join("\n")`; store the stamp; focus the field.
- **Ctrl+Enter:** if the editor's stamp (text id + version) differs from the stored one →
  refuse, toast "The file changed; edit again", the field stays so its text can be copied.
  Otherwise `editor.replace((start-1,0)..(end-1,len), text)` (one undo step) → `file_edited`
  (●) → `build_preview` → close the field, clear the selection. Empty text leaves one blank
  line where the block was (undoable). ✎ is refused while the shown page is of an older text.
- **Cancel:** Esc or click outside (`on_mouse_down_out`).
- One edit at a time: ✎ on another block cancels the open one. Switching to Source,
  opening another file or a page rebuild closes the field; its text is discarded.

## Part 4 — Tests and documents

**Tests** (no real agent, no network):

| Where | What |
|---|---|
| `preview_select.rs` | Copy across paragraph, table (`\t`/`\n`), nested list (prefixes); reversed selection; Vietnamese char boundaries; selection spanning a diagram and a rule; `word_at`; `select_all`. |
| `preview.rs` | `with_selection` (cuts highlights, fills plain text, clamps), `nearest`. Renderer/model piece counts: `debug_assert_eq!` per block at draw time (no gpui test harness in the repo). |
| `files_view/tests.rs` | Apply replaces the lines, one undo step; empty text leaves a blank line; stamp mismatch refused, text unchanged; Source / another file closes the field; rebuild clears the selection; preview actions leave the editor text unchanged. |

**Documents** (same commit as the code):
- ADR `docs/decisions/0072-files-preview-select-copy-block-edit.md` (re-check the number;
  0070 is absent, 0071 is the latest).
- SPEC §12A: replace "editing only happens there" with the selection, copy, ✎ and
  Ctrl+Enter rules. SPEC §12: state that Docs keeps block picking (character selection in
  Docs stays backlog).
- `docs/HUMAN_CHECKS.md`: dark and light — selection color, ✎ on hover, edit field;
  double-click word; paste a copied table into Notepad shows tab columns.
- No migration, no protocol change, no new dependency.

**Verify:** `"C:/Program Files/Git/bin/bash.exe" scripts/verify.sh`, then run the app on a
`.md` file with a table, a list and a Mermaid block.
