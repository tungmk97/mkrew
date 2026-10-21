# 0071 — In-app update from GitHub Releases

- Date: 2026-10-21
- Status: accepted (SPEC §4, §14, §20K; supersedes the unimplemented 2026-10-10 auto-update
  design that needed a token for a private repo)
- Milestone: none (user request 2026-10-21)

## Context

A new version meant building on each machine, copying files and restarting by hand. Releases
were created as drafts, so nothing outside GitHub saw them. The MSI installed per machine and
needed admin; the macOS package was a bare `bin/` folder in a tar.gz. The user wants: a tag push
builds Windows and macOS, a running Mkrew offers the update, one click installs it without admin
on Windows, and the agents keep their conversations.

## Decision

- Source: `GET https://api.github.com/repos/tungmk97/mkrew/releases/latest` (the repo was made
  public, so no token), at window open and every 6 h, plus palette "Global: Check for updates".
  Only a plain `a.b.c` tag strictly greater than the running version is offered.
- Own module `mkrew_ui::update` on the `ureq` already in the tree; no updater crate (they replace
  binaries and know nothing about the daemon or resume).
- Windows: the Inno setup is the only installer (`packaging/mkrew.wxs`, `scripts/package.ps1`
  removed). The UI downloads `mkrew-<v>-x64-setup.exe`, runs it detached with
  `/VERYSILENT /SUPPRESSMSGBOXES /NORESTART /LAUNCH=1` and the running install's scope
  (`/CURRENTUSER` under `%LOCALAPPDATA%`, else `/ALLUSERS`), and quits. Setup stops the daemon
  (existing `PrepareToInstall`), installs, and a `[Run]` entry checked by `LaunchAfterUpdate`
  reopens Mkrew.
- macOS: `scripts/package.sh --app` builds an ad-hoc signed `Mkrew.app` zipped with `ditto`
  (no Developer ID, no notarization). The UI unzips it beside the bundle, runs the bundle's
  `mk daemon stop --only-from <Contents/MacOS>`, renames the bundle to `.old` (put back if the
  next move fails), moves the new one in, `open -n`, and quits. Leftovers go at the next start.
- Agents continue through the existing daemon resume (SPEC §8, ADR 0017): the new daemon resumes
  every interrupted agent pane in its conversation. PTYs live in the daemon, so the processes
  themselves cannot survive a daemon replacement.
- Integrity: HTTPS plus the byte count of the asset's `size`; no checksum or signature.
- Releases are published directly (no `--draft`), and the workflow refuses a tag that differs
  from the `Cargo.toml` version.

## Consequences

- An update stops running turns, plain shells and run commands without autostart.
- Old MSI installs (no `unins000.exe`), dev builds, Linux and Intel Macs get the release page
  instead of an in-place install. An MSI user uninstalls it and runs the setup once.
- macOS first install needs right-click → Open once; in-app downloads carry no quarantine flag.
- No checksum: a compromised GitHub account or release could ship anything. Add a `SHA256SUMS`
  asset and check it when the app has other users.
