---
name: mkrew-cleanup
description: Use when asked to clean up, "dọn dẹp", free disk space, or delete build junk (target/, *.locked-old.exe, stale worktrees, logs) in the Mkrew repo, or when a build fails with "no space"/LNK1318.
---

# Mkrew cleanup

Delete only files that dev work generated and that the next build or test run can make again.
The script does the work. It deletes a path only if git ignores the path and no file under it
is tracked. It never stops a process.

## Steps

1. Dry run. It prints each group with its size and deletes nothing:
   `sh .claude/skills/mkrew-cleanup/cleanup.sh --stale 7`
   Add `--full` to see the size of a full `cargo clean` as well.
2. Show the user a table: group | paths | size | cost of deleting.
3. Delete:
   - Only `junk` was found, and the user asked to clean: run it with `--apply`.
   - `stale` or `full` was found: the user picks the level first (C: is nearly full, and a
     rebuild costs the user time; the user must agree before build output goes). Then run the
     same command with `--apply`.
4. Lines like `in use, skipped: …` mean a running process holds the file. Report them.
   Do not stop the process.

| Level | Deletes | Cost |
|---|---|---|
| (default) `junk` | `*.locked-old*` exes, `target/{tmp/*,idx,measure,verify.log}`, `*.pending-snap`, `.mkrew-build/`, `.superpowers-baseline.log`, empty `.worktrees/*` folders that git does not list as worktrees | none |
| `--stale DAYS` | files and folders in `target*/<profile>/{deps,build,.fingerprint,incremental}` that were not changed for DAYS days, in the main repo and in every worktree | cargo builds again any unit that is still in use |
| `--full` | every `target/` (main repo + `.worktrees/*/target`) | full rebuild, 10+ min |

## Never delete (the script does not touch these)

- Tracked files, and untracked files that git does not ignore (for example new `docs/superpowers/plans/*.md`).
- `.claude/settings.local.json`, `.superpowers/` (SDD ledgers), `dist/` (installers the user hands out).
- Worktrees that `git worktree list` shows. To remove one, use finishing-a-development-branch.
- `.worktrees/*` folders that git does not list but that still hold files. The script prints
  them as `keep`. Tell the user. They may hold unsaved work.
- Files outside the repo (`%TEMP%`, `%APPDATA%\mkrew`, the installed app).
