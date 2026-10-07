# 0014 — Turn restore writes only the project folder's changed paths

- Date: 2026-10-07
- Status: accepted
- Milestone: M11

## Context

SPEC §20C.2 said restore = `git read-tree` of the turn's start tree into a temp index +
`git checkout-index -a -f`. A turn snapshot covers the whole repo, but a project may be a
subfolder of it (§20C.1). `-a` would rewrite every file of the repo — including files outside
the project folder that other sessions or the user changed — and touch unchanged files' mtimes.

## Options

1. `checkout-index -a -f` as written — simple, but restores outside the project folder.
2. `checkout-index -f --stdin` with only the paths that differ between the start tree and a
   snapshot of now, limited to the project folder (`git diff --relative`) — touches exactly what
   the preview lists.

## Decision

Option 2. The preview (`turn.restore_preview`) and the restore compute the same list (start
tree vs. a fresh unreferenced snapshot, relative to the project folder), so restore writes back
exactly the changed/deleted files the user confirmed and deletes the confirmed created ones.

## Consequences

- SPEC §20C.2 updated.
- Directories created during the turn may be left empty after their files are deleted.
- A created file the agent `git add`-ed is deleted from disk but stays in the real index
  (the index is never touched); `git status` then shows it as deleted.
- Accepted races (as for `task.undo`): the `working` check and the restore are not atomic, and a
  turn-start snapshot can run between the fingerprint check and the checkout.
