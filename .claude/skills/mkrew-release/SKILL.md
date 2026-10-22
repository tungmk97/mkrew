---
name: mkrew-release
description: Cut a new Mkrew release - bump the workspace version, commit, tag vX.Y.Z, push, and watch the GitHub Release workflow. Use when the user says "release", "phát hành", "ra bản mới", "tag version", "/mkrew-release", or wants running Mkrew copies to get an update. Pushing master alone never releases; only this skill does.
---

# Release a new version

A push to `master` only runs CI. A `v*` tag push runs `.github/workflows/release.yml`: it checks
that the tag equals `[workspace.package] version` in `Cargo.toml`, builds the Windows setup, the
macOS app zip and the Linux tar.gz with `--locked`, and publishes the GitHub Release. Running copies
of Mkrew offer it within 6 hours (ADR 0071). So a release is: bump → commit → tag → push.

This skill is the user's explicit request to push; still confirm the version and the push (steps 2
and 6). Commit with `git -c user.email=tungnv975@gmail.com`. Stage only `Cargo.toml` and
`Cargo.lock`: other agents may have dirty files in the tree; leave them alone.

## 1. Preflight (stop and report on any failure)

```sh
git rev-parse --abbrev-ref HEAD                    # master
git fetch origin --tags
git status -sb | grep -m1 '##'                     # not "behind"; "ahead N" is fine (those go out too)
git diff --quiet HEAD -- Cargo.toml Cargo.lock     # exit 0: no pending edits in the two files
git describe --tags --abbrev=0 --match 'v*'        # last release, e.g. v0.1.6
grep -m1 '^version' Cargo.toml                     # must equal the last tag without "v"
```

Behind origin → tell the user to pull first (never pull/merge/rebase yourself).

## 2. Pick the version

Show the changes since the last tag:

```sh
git log --oneline <last-tag>..origin/master
git log --oneline origin/master..HEAD              # local commits that this push will publish
```

No commits since the last tag → nothing to release; stop. Otherwise ask with the `ask` tool:
patch (default) / minor / major, with the computed numbers (0.x: breaking changes still go minor).
The new tag must not exist: `git ls-remote --tags origin vX.Y.Z` prints nothing.

## 3. Verify

`"C:/Program Files/Git/bin/bash.exe" scripts/verify.sh` (plain `bash` is WSL). The release
workflow runs no tests, so a red verify blocks the release. Flaky daemon tests: rerun that crate
alone (`mkrew-verify` skill).

## 4. Bump

Edit only `version = "…"` under `[workspace.package]` in `Cargo.toml` (every crate inherits it).
Then refresh the workspace entries in the lock file without touching dependencies:

```sh
cargo update --workspace --offline
git diff --stat -- Cargo.toml Cargo.lock           # Cargo.toml 1 line, Cargo.lock only mkrew-*/spike-term
cargo metadata --locked --format-version 1 --no-deps >/dev/null   # exit 0 = CI's --locked will pass
```

## 5. Commit and tag

```sh
git add Cargo.toml Cargo.lock
git -c user.email=tungnv975@gmail.com commit -m "chore(release): vX.Y.Z"
git -c user.email=tungnv975@gmail.com tag -a vX.Y.Z -m "Mkrew vX.Y.Z"
```

## 6. Push (confirm first)

Ask: "Push master + tag vX.Y.Z lên origin? Bản release sẽ public." Yes →

```sh
git push origin master
git push origin vX.Y.Z
```

Commit first, then the tag, so the tagged commit is on `master`. No → leave the commit and tag
local and tell the user the two push commands.

## 7. Watch the workflow

The `gh` default account may not be `tungmk97`; pass the repo explicitly. Never `gh auth switch`
without asking.

```sh
gh run list -R tungmk97/mkrew --workflow release.yml -L 1
gh run watch <run-id> -R tungmk97/mkrew --exit-status       # async, timeout ~3600: 3 OS builds
gh release view vX.Y.Z -R tungmk97/mkrew                    # assets: *-setup.exe, *.zip, *.tar.gz
```

Failure → `gh run view <run-id> -R tungmk97/mkrew --log-failed`, report the cause. Never delete or
move a pushed tag (history rewrite, and running copies may already see it): fix on `master`, then
release the next patch version with this skill. Ask the user before re-running the job.

## 8. Report (in Vietnamese)

Version, commit hash, tag, workflow run URL and result, release URL with its assets, and what was
not checked (e.g. the update button in a running Mkrew appears within 6 hours, a manual check).
