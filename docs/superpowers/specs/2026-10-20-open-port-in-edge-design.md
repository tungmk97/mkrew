# Open a running port in Edge — design

- Date: 2026-10-20
- Source: brainstorming with the user. Chosen: no embedded browser (performance, ADR 0021);
  Edge profile per workspace (A); setting in Run setup (1); normal tab, not `--app` (1);
  workspace button opens the first port (1).

## Problem

Testing a workspace's web app means opening a browser by hand, typing `localhost:<port>`, and
logging in again whenever logins of different projects collide in one browser profile.

## Goal

One click from Mkrew opens the running port in Edge. A workspace can opt in to its own Edge
profile, so its logins, cookies and storage are remembered and kept apart from other
workspaces. Mkrew's own RAM and CPU do not change (no webview; SPEC §15 budgets untouched).

## Spike result (2026-10-20, this machine)

- Edge: `HKLM\SOFTWARE\Microsoft\Windows\CurrentVersion\App Paths\msedge.exe` →
  `C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe`. `msedge` is **not** on `PATH`,
  so `Command::new("msedge")` fails; the path must be read from App Paths.
- `msedge --profile-directory="Mkrew spike" about:blank` created
  `%LOCALAPPDATA%\Microsoft\Edge\User Data\Mkrew spike`, but Edge named the profile
  **"Profile 2"** in `Local State`. The folder name is ours, the display name is not. The user
  can rename it in Edge; Mkrew does not edit `Local State`.
- Login kept after closing and reopening Edge: standard Chromium profile behavior, checked by
  the human (HUMAN_CHECKS).

## Design

1. **Setting.** `Project.browser_profile: bool` (default `false`). Run setup dialog gets a
   checkbox below `Start with the daemon`: `Open in a separate Edge profile (remembers logins)`.
   Saved with the dialog through `project.update` (`ProjectUpdateParams.browser_profile:
   Option<bool>`), exactly like `run_autostart`.
2. **Launch.** A pure function builds the command, a thin one spawns it:
   - off: `msedge http://localhost:<port>` (Edge opens a tab in its current window / last
     used profile);
   - on: `msedge --profile-directory=Mkrew-<project id> http://localhost:<port>`. The ULID, not
     the name: renaming the workspace must not lose the logins, and the id has no spaces or
     quotes.
   - Edge path: Windows reads App Paths (`HKLM`, then `HKCU`); macOS
     `/Applications/Microsoft Edge.app/Contents/MacOS/Microsoft Edge`; Linux `microsoft-edge`
     on `PATH`.
   - Edge not found or the spawn fails: `cx.open_url(url)` (default browser, as Ctrl+Click on a
     URL does today) and, when the profile setting was on, `toast_info` "Microsoft Edge not
     found — opened in the default browser". The spawned process is not waited for.
3. **Buttons** (theme tokens only, `Icon::ExternalLink` or the closest existing icon, tooltip
   `Open :<port> in Edge`):
   - **RUN rows** (`shell/sidebar.rs` run rows, and the Run menu rows in `shell/run.rs`): one
     button per port of a live command, next to Restart / Stop.
   - **Workspace card header:** one button while `Summary::ports()` is non-empty; it opens
     `ports()[0]` (first port in Run setup order). Tooltip names that port.
   - No live port → no button.
4. **Storage and protocol.** Migration `0024_project_browser_profile.sql`
   (`ALTER TABLE project ADD COLUMN browser_profile INTEGER NOT NULL DEFAULT 0`);
   `PROJECT_COLUMNS`, `RawProject`, `ProjectChanges`, the UPDATE in `mkrew-store`; daemon
   `project.update` passes it through; `PROTOCOL_VERSION` 47 → 48 (`Project` and
   `ProjectUpdateParams` gain a field). ADR 0070 records the choice (Edge only, no embedded
   browser, profile per workspace id). SPEC §5 (migration list), §7 (`project.update` row),
   §10 (Run setup dialog, RUN rows, workspace card) updated in the same commit.

## Out of scope (YAGNI)

- An embedded browser / WebView2 pane (ADR 0021 stands).
- Other browsers than Edge; a browser picker.
- A port picker on the workspace button (the RUN rows already open any port).
- `--app` windows; URLs other than `http://localhost:<port>` (paths, https).
- Deleting the Edge profile when a workspace is removed (the user deletes it in Edge).
- Sharing the profile with agents (Playwright / remote debugging).
- A `mk project update --browser-profile` CLI switch.

## Error cases

1. Edge missing (Linux/macOS without Edge, or uninstalled): default browser opens, toast says
   why when the profile was wanted. Never a silent no-op.
2. Port closes between render and click: Edge shows its own "can't reach" page; Mkrew does not
   re-check.
3. Old daemon (protocol 47) with a new UI: the existing `version_mismatch` path.

## Testing

- Unit (mkrew-ui): the command builder — off → `[url]`; on → `[--profile-directory=Mkrew-<id>,
  url]`; the URL is `http://localhost:<port>`.
- Unit (mkrew-ui): the workspace button target is the first port in config order
  (`Summary::ports()` already pinned; one assert that the button uses it).
- Store: `browser_profile` round-trips through `update_project` and survives reopen;
  migration default `false` for existing rows.
- Protocol: `ProjectUpdateParams` parses `browser_profile`; `Project` serializes it.
- Run setup: `begin()` sends `browser_profile: Some(<checkbox>)`.
- No test starts Edge or opens a URL (AGENTS.md). HUMAN_CHECKS "To verify": with the box off a
  click opens a tab in the current Edge; with it on, log in, close Edge, click again → still
  logged in, and a second workspace has its own login; workspace button opens the first port;
  buttons vanish when the command stops; dark + light.
