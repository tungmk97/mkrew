# In-app update — design

Date: 2026-10-10. Agreed in chat (Vietnamese) with the user; this file records it.

## Intent

When a new Mkrew version is released on GitHub, every user who has the app open sees it and can
install it with one click. After the update everything looks as before (workspaces, tabs, panes,
agent conversations) without the user rebuilding anything by hand.

## What is and is not possible (evaluated with the user)

- Notice + one-click install: possible.
- UI state: kept. Layout (`terminal_tab`) and pins are stored; the UI reattaches on start.
- **The daemon must restart**: Windows locks running `.exe` files, and the daemon owns every
  ConPTY. Agent sessions resume their conversation through the existing interrupted-session
  resume (SPEC §8, ADR 0017); a turn that was running is cut. Shell and `run:` panes do not
  come back. Zero-loss update (a separate PTY host process) was offered and **not chosen**.

## Decisions

|Topic|Decision|
|---|---|
|Approach|Option 1: restart + existing auto-resume, with "update when idle".|
|Release source|Private repo `tungmk97/mkrew`; GitHub Releases API `releases/latest` (published, non-draft, non-prerelease).|
|Auth|Each machine sets `MKREW_GITHUB_TOKEN` (fine-grained, `Contents: read-only`) at user level. No token = feature off, silently.|
|Installer|The Inno setup `mkrew-<x.y.z>-x64-setup.exe` the maintainer builds locally (skill `mkrew-build-setup`) and uploads to the release. Not the CI MSI: it is a different product (other UpgradeCode, Program Files, per-machine only) and would install a second copy.|
|Install|Silent: `/VERYSILENT /SUPPRESSMSGBOXES /NORESTART` plus the scope of the running install (`/ALLUSERS` for a Program Files install — UAC prompt; else `/CURRENTUSER`). Then the UI is started again.|
|Integrity|GitHub's asset `digest` (`sha256:…`) is checked when present; a mismatch deletes the file and fails.|
|When agents work|Click → if no agent is working and no shell/run pane is live, install at once. Otherwise a dialog lists what is cut: **Update now** / **Update when idle** / **Cancel**. "When idle" = no agent session in `working`; `needs_input` / `idle` do not block. *(Question 4 was not answered; this is the recommended default — change it here if wrong.)*|
|Who checks|The daemon (one check per machine, it must stop itself anyway): at start and every hour.|
|Dev builds|Off: updates only run for an installed build (`unins000.exe` in the folder above `bin\`) on Windows.|
|HTTP|`curl.exe` (in Windows since 10 1803); no new HTTP crate in the daemon.|

## Flow

```mermaid
sequenceDiagram
    participant D as daemon
    participant G as GitHub API
    participant U as UI
    D->>G: GET releases/latest (hourly, Bearer token)
    D-->>U: update.changed {available, version}
    U->>U: title bar "Update to vX"
    U->>D: update.apply {when: now|idle}
    D->>G: download setup asset, check sha256
    D-->>U: update.changed {waiting_idle} (if agents work)
    D-->>U: update.changed {installing}
    U->>U: quit
    D->>D: stop like daemon.stop (agents marked interrupted)
    D->>D: start cmd: setup /VERYSILENT … & start mkrew.exe
    Note over D,U: new UI starts the new daemon; it resumes interrupted agents
```

## Protocol

- `update.status` → `UpdateStatus`.
- `update.apply { when: "now" | "idle" }` → `null`; `invalid_state` when nothing is ready.
- Event `update.changed` (`UpdateStatus`).
- `UpdateStatus` = `{state: up_to_date}` | `available {version}` | `downloading {version}` |
  `waiting_idle {version}` | `installing {version}` | `failed {version, message}`.

## Not in scope

Zero-loss update (PTY host), release notes in the app, cancelling "update when idle" (restart
the daemon to cancel), CI building the Inno setup, macOS/Linux updates, code signing.
