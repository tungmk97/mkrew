# Inbox — bugs, enhancements, features

Write one line per item under **Items**: `- [ ] <what is wrong / what you want>`.
Optional tag at the start: `[bug]`, `[enh]`, `[feat]` (the skill guesses if missing).
Indented lines under an item are part of it (steps to reproduce, screenshots, ideas).

Then run the `mkrew-inbox` skill. It only reads `[ ]` items.

Markers: `[ ]` new · `[~]` planned (skill adds `→ plan: <file>`) · `[x]` done · `[-]` dropped.

## Items

- [x] In the git commit box: cannot Ctrl+A to select all to remove → plan: docs/superpowers/plans/2026-10-08-inbox-keys-fonts-view-rail.md Task 1
- [x] Add a feature to config font style, font size... for the app and for the terminal → plan: docs/superpowers/plans/2026-10-08-inbox-keys-fonts-view-rail.md Task 4, 5
- [x] I want more view on the main content, so the top bar (Board, Terminal...) should move somewhere else — I'm thinking the right bar. The Run button can move inline with the History, + Agent bar. So we clean up the top bar and get more space → plan: docs/superpowers/plans/2026-10-08-inbox-keys-fonts-view-rail.md Task 6
- [x] In terminal, Ctrl+C now cannot be used to copy text, please fix → plan: docs/superpowers/plans/2026-10-08-inbox-keys-fonts-view-rail.md Task 2
- [x] In terminal when typing e.g in OMP cannot Ctrl + Backspace to delete a text now just delete by character. And cannot Ctrl + A to select all to remove → plan: docs/superpowers/plans/2026-10-08-inbox-keys-fonts-view-rail.md Task 3 (Ctrl+A stays line start; clear with Ctrl+U)
- [~] Should have icon the the agent that showing when agent inprogress, iddle, done, or ask me → plan: docs/superpowers/plans/2026-10-08-inbox-status-drop-badges.md Task 1
- [x] Check if we don't have feature to drag drop a file into the terminal chat if not need to add in → plan: docs/superpowers/plans/2026-10-08-inbox-status-drop-badges.md Task 2 (already built in T10.8; manual check in HUMAN_CHECKS)
- [x] For the agent in the workspace do we have icon for each agent, if not yet then → plan: docs/superpowers/plans/2026-10-08-inbox-status-drop-badges.md Task 3