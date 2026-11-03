# 0092 - Markdown Split view and an interactive preview

- Date: 2026-11-03
- Status: accepted
- Milestone: none (human request 2026-11-03: "view and edit Markdown side by side, resize both sides; make the preview nicer")

## Context

A `.md` file in the Editor showed either the rendered preview or the source (`Preview | Source`, ADR 0083).
Editing meant switching back and forth, and the preview was built only on the switch to Preview.
The preview was read-only: links were not clickable (ADR 0021 skipped them), task boxes could not be ticked, code blocks could not be copied in one go, and GitHub alerts and YAML front matter showed as a plain quote and as stray text.
The human works with Markdown files a lot.

## Decision

- The Editor header switch has three icon segments: Preview (eye), Split (columns) and Source (code), each with a tooltip and its key.
  Docs keeps its two text segments.
- Split shows the editor on the left and the live page on the right, with a draggable divider between them.
  - The divider keeps each side between 20 % and 80 %; a double-click puts it back at half and half.
    The source's share is saved as `[ui] md_split` (percent, default 50) when the drag ends.
  - The page is built again after each edit, off the UI thread, one build at a time per file: an edit made while a page builds is built once that build lands.
  - The page follows the editor's scroll: the block shown for the editor's top line is put at the top of the page, part way into it when the editor is inside a long block.
    Scrolling the page does not move the editor.
  - A press on the page moves the editor's caret to that block's first line, scrolled into view only when it is off screen; that scroll is not followed back, so the page does not jump under the pointer.
  - The page has its own keyboard focus (selection, Ctrl+C, Ctrl+A); the ✎ block edit works as in Preview.
  - Find and "Open at a line" keep Split; they still switch Preview to Source.
- The mode a file is switched to becomes the mode the next Markdown file opens in, saved as `[ui] md_mode` (`preview`, `split` or `source`, default `preview`; also on Settings → Appearance).
- Ctrl+Shift+V stays Preview ↔ Source (Split goes to Preview); Ctrl+Alt+V (`toggle_file_split`) toggles Split ↔ Source. The palette lists both.
- The preview, in the Editor and in Docs:
  - Links are clickable (this revises ADR 0021's "skipped: clickable links"): `http`, `https` and `mailto` open in the browser; `#heading` scrolls to the heading with that GitHub anchor; a relative path opens that file of the folder in the Editor (`/path` from the folder's root, `#L12` at a line).
    Other schemes and paths leaving the folder open nothing and say so in a toast.
    A press on a link starts no selection, block pick or caret move.
    Links in a thread message or an answer (not on a file's page) resolve against the folder root and have no headings to scroll to.
  - GitHub alerts (`> [!NOTE]`, `[!TIP]`, `[!IMPORTANT]`, `[!WARNING]`, `[!CAUTION]`) show with a colored bar and an icon and title in that color.
  - YAML front matter shows as a bordered card of `key value` rows.
    It is only a `---` first line closed by a `---` or `...` line: pulldown-cmark's own metadata blocks start at any block, and would swallow a document's `---` rules and the text between them.
  - Code blocks show "Copy" on hover at the top-right, left of the ✎ corner on the Files preview.
- In the Editor's preview (Preview and Split), a click on a task box ticks or unticks it in the source: one undo step, unsaved like any edit, the caret and selection kept, refused with a toast while the page is of an older text.

## Rejected

- Two-way scroll sync: the page and the editor would chase each other while either scrolls.
- Rebuilding the page on a debounce timer: one build at a time already bounds the work, and a timer adds latency to every edit.
- A WebView or HTML preview: rejected by ADR 0001 and ADR 0021.
- Opening `#heading` links to other files at that heading: the file opens at the top; add it when someone needs it.
- Three text segments in the Editor header: with a long file name and the caret position they squeeze the name out of a narrow header.

## Consequences

- `FileTab.mode` is `MdMode` from `mkrew-core` (it is also the setting); the Docs view keeps its own two-mode `docs::Mode`.
- `markdown::BlockKind::Quote` carries the alert kind; `BlockKind::FrontMatter` is new.
- The preview dispatches `OpenLink`, `ToggleTask` and `CopyCode` actions to the window, like `ExpandDiagram`.
- SPEC §12A, §13 and the `[ui]` settings list say so.
