# In-app update (Windows per-user, macOS) — design

- Date: 2026-10-21
- Source: brainstorming with the user. Chosen: approach A (own `update.rs` on the existing
  `ureq`, no updater crate); repo `tungmk97/mkrew` made public (no token); Windows = Inno setup
  `.exe` only (MSI dropped, it needs admin); macOS = ad-hoc signed `Mkrew.app` in a `.zip`;
  agents survive through the existing daemon resume (SPEC §8, ADR 0017).
- Numbers: ADR 0071 (0070 is reserved by the open-port-in-Edge spec), SPEC §20K. No protocol
  change, no migration.

## Problem

A new version means building on the machine, copying files and restarting by hand. Releases are
created as drafts, so nothing outside GitHub sees them, and the macOS package is a bare `bin/`
folder in a tar.gz. The MSI installs per machine and needs admin.

## Goal

Push a tag `vX.Y.Z` → CI builds and publishes the Windows setup and the macOS app. A running
Mkrew notices the new version, the user clicks once, Mkrew installs it without admin (Windows
per-user install), reopens, and every agent pane resumes its conversation in the same place.

## Facts this relies on (checked 2026-10-21)

- `GET https://api.github.com/repos/tungmk97/mkrew` → 200 (public). `…/releases/latest` → 404
  until the first non-draft release exists.
- `packaging/mkrew.iss`: `PrivilegesRequired=lowest`, per-user install in
  `%LOCALAPPDATA%\Programs\Mkrew`, `/CURRENTUSER` / `/ALLUSERS` / `/VERYSILENT` work, `AppId`
  fixed, `PrepareToInstall` runs `mk daemon stop --only-from <bin>` (agents resume on the next
  daemon start), `CloseApplications=yes`, `[Run]` launch is `skipifsilent`. Inno writes
  `{app}\unins000.exe`.
- Daemon start resumes every `interrupted` agent session that is a leaf of a tab layout, and
  tasks in running/needs_input (SPEC §8). The UI spawns the daemon when none answers
  (`connect_or_spawn`). Plain shells are not resumed; run commands restart only with
  `run_autostart`.
- `mkrew-ui` already depends on `ureq` 3 (rustls).
- Toasts (§14) auto-hide after ~4 s and carry no buttons; confirmations use the in-window
  `window.prompt` (`shell/confirm_prompt.rs`). The title bar already hosts a polled icon
  (phone access, `shell/phone_access.rs`).

## Design

### 1. Release CI (`.github/workflows/release.yml`, committed by the user)

- Trigger unchanged: tag `v*` push (+ `workflow_dispatch` dry run, artifacts only).
- First step of every job: fail when the tag without `v` differs from `version` in the root
  `Cargo.toml`.
- Windows job: drop WiX/dotnet; install Inno Setup (`choco install innosetup -y`), run
  `pwsh scripts/package-setup.ps1` → `dist/mkrew-<v>-x64-setup.exe`.
- macOS job (`macos-latest`, arm64): `bash scripts/package.sh --app` builds
  `dist/mkrew-<v>-macos-arm64.zip` (section 2).
- Linux job: unchanged `.tar.gz`.
- Publish: `gh release create <tag> dist/* --generate-notes --title "Mkrew <tag>"` — **no
  `--draft`**, so `releases/latest` returns it at once.
- Release routine (README): bump `version` in `Cargo.toml` → commit → `git tag vX.Y.Z` → push
  the commit and the tag. `ci.yml` (push to master = verify only) is unchanged.

### 2. macOS app bundle (`scripts/package.sh --app`)

- On macOS only: `Mkrew.app/Contents/MacOS/{mkrew,mk,mkrew-daemon}`,
  `Contents/Resources/agents/*.toml`, `Contents/Info.plist` (`CFBundleIdentifier`
  `dev.mkrew.app`, `CFBundleExecutable` `mkrew`, `CFBundleShortVersionString` and
  `CFBundleVersion` = the version, `LSMinimumSystemVersion` 12.0, `NSHighResolutionCapable`).
- `codesign --force --deep -s - Mkrew.app` (ad-hoc; no Developer ID, no notarization).
- `ditto -c -k --keepParent Mkrew.app mkrew-<v>-macos-arm64.zip`.
- First install (README): unzip, drag to `/Applications`, right-click → Open once (Gatekeeper
  "unidentified developer"), or `xattr -dr com.apple.quarantine /Applications/Mkrew.app`.
  In-app updates download with `ureq`, which sets no quarantine flag, so they open without it.
- The daemon and `mk web` are found next to the running exe (`sibling_exe`), which inside the
  bundle is `Contents/MacOS`. `mk` on PATH is optional: README shows
  `ln -s /Applications/Mkrew.app/Contents/MacOS/mk /usr/local/bin/mk`.

### 3. Windows installer: drop the MSI, relaunch after a silent install

- Delete `packaging/mkrew.wxs`, `scripts/package.ps1`, the MSI test in
  `crates/mkrew-cli/tests/packaging.rs`, and the README "Windows (MSI)" section; SPEC §4 tree
  comments and M7 checklist say "setup .exe".
- `mkrew.iss` `[Run]` gains a second entry without `skipifsilent`:
  `Filename: "{app}\bin\mkrew.exe"; Flags: nowait; Check: LaunchAfterUpdate`, where
  `LaunchAfterUpdate` = `WizardSilent and (ExpandConstant('{param:LAUNCH|0}') = '1')`.

### 4. Update check (`crates/mkrew-ui/src/update.rs`, pure + thin I/O)

- Pure: `parse_release(json) -> Release { version, page_url, assets[{name, url, size}] }`
  (`tag_name` without a leading `v`); `is_newer(latest, current)` comparing numeric `a.b.c`
  (any pre-release/build suffix → not newer, never offered); `pick_asset(os, arch, assets)`:
  Windows x86_64 → `*-x64-setup.exe`, macOS aarch64 → `*-macos-arm64.zip`, else none;
  `install_kind(exe_path, env) -> Inno { per_user } | MacApp { bundle } | Manual`.
- `install_kind`: Windows → `Inno` when `<exe dir>/../unins000.exe` exists, `per_user` when the
  exe lies under `%LOCALAPPDATA%`; macOS → `MacApp` when the exe path is
  `<X>.app/Contents/MacOS/mkrew`; everything else (dev build in `target/`, old MSI install,
  Linux, no matching asset) → `Manual`.
- Thin I/O: `fetch_latest()` — `ureq` GET `releases/latest`, header
  `User-Agent: mkrew/<version>`, 10 s timeout, on the background executor.
- Schedule: at window open, then every 6 h. 404, network or parse failure → no update shown,
  one `tracing::warn`. Palette command "Global: Check for updates" runs it now and answers with
  `toast_info` ("Mkrew <v> is up to date" / "Mkrew <new> is available") or `toast_error` (the
  failure).

### 5. UI (theme tokens only; skill mkrew-ui-design)

Adjustment to the approved chat design: toasts auto-hide and have no buttons (§14), so the
lasting "update available" signal is a title bar button; the toast only announces it once.

- When a newer version is known: a title bar icon button (`icon_button_in`, like the phone
  icon) right before the phone icon, Lucide `circle-arrow-up` in `accent-text`, tooltip
  `Update to Mkrew <new> (you have <current>)`; plus one `toast_info`
  "Mkrew <new> is available" when first found. The button stays until restart; no "skip this
  version". Unsaved Files edits are asked about first (`ask_unsaved`), like quitting.
- Click → `window.prompt` (§14): title `Update to Mkrew <new>?`, detail "Mkrew closes, installs
  and reopens. Agent panes resume their conversation." + when ≥ 1 session is `working`:
  "N agents are working; their current turn stops." + "Shells and run commands without
  autostart do not come back." Buttons Cancel / **Update**.
- `Manual` kind: the button's click opens the release page (`cx.open_url(page_url)`), no prompt.
- While downloading: the button is dimmed (`opacity::DISABLED`), ignores clicks, tooltip
  `Downloading Mkrew <new>…`; failure → `toast_error` with the reason, button back to normal.

### 6. Apply (thin, per OS)

- Download the asset to `<temp>/mkrew-update/<asset name>` (streamed to file; byte count must
  equal the asset's `size`).
- **Windows (`Inno`)**: spawn detached
  `<setup> /VERYSILENT /SUPPRESSMSGBOXES /NORESTART /LAUNCH=1` + `/CURRENTUSER` (per-user) or
  `/ALLUSERS` (Program Files install: Windows shows UAC, the only admin case), then
  `quit_ui`. The installer stops the daemon (existing `PrepareToInstall`), replaces the files,
  relaunches `mkrew.exe`; the new UI spawns the new daemon, which resumes the agents.
- **macOS (`MacApp`)**: `ditto -x -k <zip> <bundle dir>/.mkrew-update` (same volume as the
  bundle, so the swap is a rename); run the bundle's plain `mk daemon stop` (`--only-from` reads
  `/proc`, absent on macOS; it returns once the daemon released its lock; non-zero → error,
  abort before touching the bundle); rename `<bundle>` → `<bundle>.old` (renamed back if the
  next move fails), move the new `Mkrew.app` to `<bundle>`, start `<bundle>/Contents/MacOS/mkrew`
  directly in its own process group (keeps `PATH`), `quit_ui`. At window open, a leftover
  `<bundle>.old` and `.mkrew-update` are removed. (Final review 2026-10-21; the Windows relaunch
  uses `runasoriginaluser`, scope = all users only under Program Files, and unsaved Files edits
  are asked again after the download.)
- Shared constant `RELEASES_API` = `https://api.github.com/repos/tungmk97/mkrew/releases/latest`.

### 7. Docs

- ADR 0071: in-app update via GitHub Releases + Inno silent install; MSI dropped; macOS ad-hoc
  bundle; no signature check (HTTPS only).
- SPEC §4 (packaging lines), §14 (title bar update button), new §20K (sections 4–6 in short),
  M7 checklist line.
- README: install (setup .exe, macOS app), release routine.
- HUMAN_CHECKS "To verify — 2026-10-21 in-app update" (see Testing).

## Out of scope (YAGNI)

- Checksums or signatures on downloads (HTTPS only; add a `SHA256SUMS` asset when the app has
  other users).
- "Skip this version", a setting to turn the check off, beta/pre-release channel.
- macOS Intel and Linux in-app update (they get the release page).
- Keeping agent processes alive across a daemon replacement (not possible: PTYs live in the
  daemon; resume covers the conversation).
- Developer ID signing / notarization, a DMG.

## Error cases

1. No release yet / offline / rate limit (60 req/h unauthenticated; we send ≤ 1 per 6 h):
   nothing shown; the manual check toasts the error.
2. Download fails or is short: `toast_error`, nothing installed, the app keeps running.
3. Windows installer fails after the UI quit: the old files stay (Inno rolls back); the user
   starts Mkrew from the Start Menu; the daemon resumes the agents as after any stop.
4. macOS daemon does not stop within 10 s: abort before touching the bundle, `toast_error`.
5. macOS swap fails: `<bundle>.old` renamed back, `toast_error`.
6. Old MSI install (Program Files, no `unins000.exe`): `Manual` → release page; README says to
   uninstall the MSI and run the setup once.

## Testing

- Unit (mkrew-ui `update.rs`): `parse_release` on a fixture JSON (tag with/without `v`, assets);
  `is_newer` (0.1.10 > 0.1.9, equal → false, older → false, `0.2.0-rc1` → false, garbage →
  false); `pick_asset` per OS/arch and none; `install_kind` with temp dirs (Inno per-user,
  Inno all-users, mac bundle path, dev `target/release`); Windows installer argument list.
- Packaging test (`crates/mkrew-cli/tests/packaging.rs`): `mkrew.iss` has the `LAUNCH` relaunch
  entry; the MSI test is removed with the MSI.
- No test calls GitHub or runs an installer (AGENTS.md).
- HUMAN_CHECKS: install the first release that has this feature (say `v0.1.6`) per-user, open 2
  agent panes (one working) and a shell, then publish `v0.1.7` → button + toast appear → prompt
  counts 1 working agent → Update → no UAC, Mkrew reopens at 0.1.7 (`mk --version`), both agent
  panes resume, the shell pane is gone; same on macOS from `/Applications`; dark + light;
  palette "Check for updates" when up to date.
