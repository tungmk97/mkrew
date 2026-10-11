# 0047 — The Files editor's change gutter compares with the git index

- Date: 2026-10-11
- Status: accepted

## Context

The Files editor (SPEC §12A, ADR 0028) showed no git changes. A gutter (as in Zed, VS Code,
Tern) marks added, changed and removed lines. It must follow unsaved typing, so the existing
`git.diff` (a diff of the file on disk) is not enough: the UI needs the text to compare with.

## Decision

- `fs.read` gets an opt-in `index_text: bool`; when set, `FsContent.index_text` carries the file's
  text in the **index** (`git cat-file blob :./<path>` after `git ls-files`), from the child
  repository holding the file (ADR 0029). Untracked, conflicted, not UTF-8, over 2 MB or outside
  a repository: `None`, and no gutter.
- The UI diffs the editor text with it (`similar`, Myers, 100 ms deadline) on the background
  executor, in the job that re-highlights 150 ms after typing stops; the marks live in
  `highlight::Code` and are drawn only on lines whose text matches that highlight.
- The index, not HEAD: the gutter shows what "Working changes" (unstaged) shows in the Git
  view, so staging a hunk clears its marks.

## Alternatives rejected

- `git.diff` per save: no marks for unsaved text; lines shift while typing.
- A separate `git.index_text` request: a second round trip and handler for every open.
- HEAD as the base: staged lines stay marked while the Git view calls them staged.

## Consequences

Protocol bump (two `#[serde(default)]` fields). Each `fs.read` of the shown file runs
`git ls-files` + `git cat-file` (Files view only; git still runs only for a view on screen).
Untracked files show the tree's `?` and no bars.
