# 0103 - Review flow: viewed then next, hunks across files, discard a hunk, send from a preview

- Date: 2026-11-22
- Status: accepted (amends SPEC sections 12A, 12B, 13 and the protocol table)
- Milestone: none (Tern-inspired update, sub-project 4; spec `docs/superpowers/specs/2026-11-22-review-flow-design.md`, plan `docs/superpowers/plans/2026-11-22-review-flow.md`)

## Context

Mkrew already had split and unified diffs, "Viewed N/M" with collapsed viewed files (ADR 0065), line comments sent to agents, per-hunk stage and unstage, Markdown Preview, Split and Source (ADR 0083, 0092) and "Send selection" from the editor, the diff and the terminal.
Reviewing an agent's work still took extra steps: marking a file viewed left the selection on it, `n` / `p` stopped at the ends of a file, a bad hunk could only be discarded with its whole file, and a selection in a Markdown preview or in Docs could not be sent.

## Decision

- `v` on a file that is not viewed marks it and selects the next file below it that is not viewed, wrapping to the top; with every file viewed the selection stays; `v` on a viewed file only clears the mark.
  It moves only when the toggled file is the selected one (a row's box never moves the selection).
- `n` past the last hunk selects the next file that is not viewed and its first hunk, `p` before the first hunk the previous such file and its last hunk; no wrap at the ends.
  The target hunk is pending until that file's diff is shown; a file without hunks keeps it, so the next key moves on.
  On a viewed file's folded diff the hunks are hidden, so `n` / `p` go straight to the next / previous file that is not viewed.
  In the History the keys never leave the commit's files.
- `git.discard` takes `hunk` (a `HunkTarget`) with `paths` empty; the daemon builds the same patch as for a hunk stage and runs `git apply -R` on the working tree.
  Protocol 56 (55 is the Widget bar's).
  The daemon refuses an untracked path with `invalid_request`; the UI offers the button only on a modified file of the Changes group, and `x` takes the current hunk, else the file.
  The confirm dialog lists that hunk's rows; its button stays disabled, and a confirm that still arrives is refused, if the reloaded diff no longer has that hunk.
  A failed discard reads the diff again, so the dialog and the page show the file as it is now.
- "Send selection" (Ctrl+Shift+L) sends, from a Markdown preview, the Markdown source of the blocks the selection touches with `path:a-b`, and from Docs its comment selection.
  In Preview the page's selection is asked first; in Split the editor's own selection first, then the page's; the editor's caret line stays the fallback.
- `c` stays "focus the commit box" and `Ctrl+Alt+M` stays "add comment".

## Alternatives

- Calling `step_file` from `step_hunk`: it cannot skip viewed files and steps to another commit in the History.
- Sending the rendered text of a preview selection: the agent should see the Markdown, with a line range it can open.
- Discarding by editing the file in the UI: loses the index-safe `git apply -R` and its stale-hunk check.
- Comments and viewed marks in the History: they need anchors on a commit, not on the working file, so they stay out of this change.

## Consequences

- A stale hunk fails with git's own text in the dialog (a toast if it was closed), and the diff reloads.
- A hand-made request naming a staged-only hunk of a clean file would apply; only the UI keeps that from happening.
- A hunk stage, unstage or discard whose line holds a line feed is refused with `invalid_request`, because it would add sections to the patch.
  A lone carriage return inside a line is fine: `git apply` splits on line feeds only.
- An older daemon is refused by the protocol version check.
- `docs/HUMAN_CHECKS.md` lists the review pass for a person to check in dark and light.
