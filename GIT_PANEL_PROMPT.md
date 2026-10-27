# Prompt: move Git into the right panel (sub-project 2)

Paste the block below into a new agent session once the Files right panel has shipped.

```text
Use the brainstorming skill (architectural path).

Goal: remove the top-level Git view and move Git into the right panel, the same way the
Files view was moved (spec: docs/superpowers/specs/2026-10-27-files-right-panel-design.md).

What already exists after sub-project 1:
- The right panel is one column with tabs (`Files | Session`), one toggle, one width
  (`[ui] inspector_width`), shown beside the center area.
- Clicking a file in the panel's tree opens it in the center `Editor` view; the view bar shows
  `Editor` only while files are open.

Wanted (like VS Code's Source Control):
- A `Git` tab in the right panel: Staged / Unstaged change lists, the commit box, push and
  branch actions, the session filter.
- Clicking a changed file shows its diff in the center area (unified / split, hunks, diff
  comments), the way a tree click shows the editor.
- The `Git` entry leaves the view bar; the `ShowGit` shortcut and palette commands follow.

Open questions to settle with the human (one at a time):
1. Where History (commit list + lane graph) lives: the panel tab, or the center area.
2. Whether the narrow panel (240-560 px) fits the change rows, commit box, hunk actions and
   the session filter, or some move to the center diff header.
3. Where diff comment threads and "Send to agent" sit.
4. Whether the center diff is its own view (`Diff`, shown only while a diff is open) or shares
   the `Editor` view's tabs.

Read first: docs/SPEC.md §10, §12A, §12B; crates/mkrew-ui/src/shell/git*.rs;
crates/mkrew-ui/src/git_view.rs; the Files right panel spec above.
```
