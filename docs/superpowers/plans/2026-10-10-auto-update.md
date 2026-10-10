# In-app Update Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** When a new Mkrew release is published on GitHub, the open app shows "Update to vX" in the title bar. One click downloads the Inno setup, installs it silently, restarts Mkrew, and agent sessions resume their conversations.

**Architecture:**
- The daemon owns the update: a new `crates/mkrew-daemon/src/update.rs` asks the GitHub Releases API through `curl.exe` at start and every hour. On `update.apply` it downloads the setup and checks its sha256. If asked, it waits until no agent is `working`. Then it broadcasts `installing` and stops exactly like `daemon.stop` (`Hub::shutdown` marks agents interrupted). Once every session is down, it starts `cmd /c start /wait setup … & start mkrew.exe`.
- The protocol gains `update.status`, `update.apply` and the event `update.changed`.
- The UI shows a title-bar button from a pure view-model (`crates/mkrew-ui/src/update.rs`). A confirm dialog appears when agents work or shells are live. The UI quits on `installing`, so the setup can replace `mkrew.exe`. The new UI starts the new daemon, and that daemon resumes the interrupted agents (existing code).

**Tech Stack:** Rust 2024, tokio, serde/serde_json, `sha2 0.10` (already in `Cargo.lock`, new direct dependency of `mkrew-daemon`), system `curl` (Windows 10 1803+, all CI runners), GPUI (`mkrew-ui`).

**Spec:** `docs/superpowers/specs/2026-10-10-auto-update-design.md` (in the **main checkout**, see Workspace). Task 3 copies the rules into `docs/SPEC.md` §8 and adds the ADR; Task 1 adds the protocol rows to §7; Task 5 adds the title-bar button to §10.

## Workspace (separate git worktree)

All tasks run in their own git worktree on branch `feat/auto-update`, never in the main checkout on `master`. The main checkout holds the user's uncommitted edits (`docs/INBOX.md`, other plans). A worktree starts from a clean `HEAD`, so `git add <path>` can never pick them up.

Setup, once, before Task 1, from the main checkout (`C:/Users/NguyenTung/Workspace/PersonalProject/mkrew-build-kit/mkrew`):

1. `git check-ignore -q .worktrees` must succeed (it does today: `.gitignore:9 /.worktrees/`).
2. Check free space on C: (`Get-PSDrive C` or `df -h /c`). The worktree gets its own `target/`. If less than 35 GB is free, stop and ask the user. Never delete anything in the main checkout's `target/`.
3. `git worktree add .worktrees/auto-update -b feat/auto-update`
4. This plan and the spec are untracked files in the main checkout. Read them from there; do not copy them into the worktree.
5. Baseline in the worktree: `cargo test --workspace -j 4`. If it fails, report the failures and ask before Task 1.

Rules for every task:

- Run every command with `.worktrees/auto-update` as the working directory. All paths below are relative to the worktree root. `sh scripts/sp.sh …` also runs from there, so the SDD ledger lives in the worktree's `.superpowers/sdd/`.
- Commit on `feat/auto-update` only. Never push.
- When all tasks are done, use superpowers:finishing-a-development-branch. Merging into `master` needs the user's OK; after the merge, `git worktree remove .worktrees/auto-update` and `git branch -d feat/auto-update`.

## Global Constraints

- Release repo: `tungmk97/mkrew` (private). Token env var: `MKREW_GITHUB_TOKEN`. No token → the feature is off and silent (no error, no button).
- Setup asset name: `mkrew-<x.y.z>-x64-setup.exe` (Inno `OutputBaseFilename`, `packaging/mkrew.iss:29`). The CI MSI is never used.
- Silent install flags, exactly: `/VERYSILENT /SUPPRESSMSGBOXES /NORESTART` then `/ALLUSERS` (install folder under `%ProgramFiles%`/`%ProgramW6432%`) or `/CURRENTUSER`.
- Updates run only on Windows, for an installed build: `unins000.exe` exists in the parent of the daemon exe's folder (`{app}\bin\mkrew-daemon.exe` → `{app}\unins000.exe`).
- "Idle" means no session with `kind == Agent && state == Working` (store state). Shells never block; `needs_input`/`idle`/`done` do not block.
- Check at daemon start, then every hour (`tokio::time::interval`, `MissedTickBehavior::Delay`, like `backup.rs`).
- No test may call the network or a real agent (SPEC §16). Update tests use `file://` URLs served by `curl` from a temp folder.
- No `unwrap`/`expect` outside tests. `thiserror`/`anyhow` per SPEC §17; the daemon module may use `anyhow` (it already depends on it).
- Do not touch `packaging/`, `.github/workflows/`, `docs/BUILD_VI.md`, `scripts/package-setup.ps1`, or the `x11`/`wayland` line in `crates/mkrew-ui/Cargo.toml`.
- `PROTOCOL_VERSION` (`crates/mkrew-protocol/src/lib.rs`): bump by exactly one in Task 1. It is 37 today; `docs/superpowers/plans/2026-10-10-inbox-perf.md` also plans 37 → 38. If the worktree's value is already 38, go to 39.
- ADR number: 0041–0045 are taken or reserved by other plans. In Task 3 run `ls docs/decisions` and take the next free number ≥ 0046; call it `NNNN` below and use it in every reference.
- `crates/mkrew-ui/src/main.rs` is ~20k lines and another plan may split it. Find code by the **symbol names** given here (`render_title_bar`, `render_git_discard`, `modal_open`, `dispatch`, the disconnect block after `while let Ok(first) = incoming.recv().await`), never by line number.
- Shell: run `cargo` and `git` directly; Git Bash scripts with `sh`; in the omp shell `bash` is WSL, never use it. Build with `-j 4`.
- Commits: after each task, commit only that task's files (`git add <paths>`, never `-A` or `.`). Conventional Commits. Never push.
- GPUI: never call `.hover()` twice on one element (`button()` already sets it). Never update the `Shell` entity from inside its own `update`.

## Review Focus

1. **A dev build (`cargo run`, `target\debug`) offers or runs an update and installs over the user's real install.** Expected: off unless `unins000.exe` is beside `bin\`; `update.status` is `up_to_date` and `update.apply` is refused with `invalid_state`. Pinned by `update_is_off_for_a_dev_build` (Task 3, `crates/mkrew-daemon/tests/daemon.rs`).
2. **A truncated or tampered download gets executed silently.** Expected: a digest mismatch deletes the file, the state becomes `failed` with "sha256" in the message, the daemon does not stop, and a retry is allowed. Pinned by `download_rejects_a_wrong_digest` (Task 2) and `a_corrupt_download_fails_and_can_be_retried` (Task 3).
3. **"Update when idle" installs while an agent is still mid-turn, or never installs because a shell is busy.** Expected: waits while any agent is `working`, ignores shells, installs as soon as the last agent leaves `working`. Pinned by `idle_waits_for_working_agents_only` (Task 3).
4. **A Program Files (all users) install is updated with `/CURRENTUSER` and a second copy appears under `%LOCALAPPDATA%`.** Expected: `/ALLUSERS` for any folder under `%ProgramFiles%` (case-insensitive), `/CURRENTUSER` otherwise; an unset variable matches nothing. Pinned by `program_files_installs_are_per_machine` (Task 2).
5. **Offline, rate limit (`403`), or an HTML error page on the hourly check shows an error or flips the state.** Expected: logged only, state unchanged; a non-JSON body is an error, not a panic. Pinned by `a_failed_check_changes_nothing` (Task 3) and `only_a_newer_release_with_the_windows_setup_counts` (Task 2).

Not testable headless (goes to HUMAN_CHECKS in Task 5): the UI quits before the daemon stops (so "Restart Mkrew to reconnect" never shows), the setup really replaces the files, the UI comes back, agents resume.

---

### Task 1: Protocol — `update.status`, `update.apply`, `update.changed`

**Files:**
- Modify: `crates/mkrew-protocol/src/message.rs` (`Request` enum end, `Event` enum end, new types after `SessionLinkPlanParams`, tests in `mod tests`)
- Modify: `crates/mkrew-protocol/src/lib.rs` (`PROTOCOL_VERSION`)
- Modify: `crates/mkrew-ui/src/sidebar.rs` (the exhaustive `match event` in `Sidebar::apply` that ends with `| Event::AiGenerated(_) => Change::None,`)
- Modify: `docs/SPEC.md` §7 (request table after the `session.plan` request row; event table after the `session.plan` event row)

**Interfaces:**
- Produces (all `pub`, re-exported by `mkrew_protocol::*`):
  - `enum UpdateStatus { UpToDate, Available { version: String }, Downloading { version: String }, WaitingIdle { version: String }, Installing { version: String }, Failed { version: String, message: String } }`, `Default` = `UpToDate`, derives `Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize`, serde `tag = "state"`, `rename_all = "snake_case"`.
  - `impl UpdateStatus { pub fn version(&self) -> Option<&str> }`.
  - `enum UpdateWhen { Now, Idle }`: derives `Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize`, `snake_case`.
  - `struct UpdateApplyParams { pub when: UpdateWhen }`.
  - `Request::UpdateStatus` (wire `update.status`, no params), `Request::UpdateApply(UpdateApplyParams)` (wire `update.apply`), `Event::UpdateChanged(UpdateStatus)` (wire `update.changed`).

- [ ] **Step 1: Write the failing test**

In `crates/mkrew-protocol/src/message.rs`, inside `mod tests` (next to `request_wire_shape_matches_spec`, which uses the same `round_trip` helper and `json!`):

```rust
    #[test]
    fn update_wire_shapes() {
        let apply = round_trip(&Message::Request {
            id: 3,
            request: Request::UpdateApply(UpdateApplyParams {
                when: UpdateWhen::Idle,
            }),
        });
        assert_eq!(
            apply,
            json!({"type": "request", "id": 3, "method": "update.apply", "params": {"when": "idle"}})
        );
        let status = round_trip(&Message::Request {
            id: 4,
            request: Request::UpdateStatus,
        });
        assert_eq!(
            status,
            json!({"type": "request", "id": 4, "method": "update.status"})
        );
        let failed = round_trip(&Message::Event(Event::UpdateChanged(UpdateStatus::Failed {
            version: "0.2.0".into(),
            message: "offline".into(),
        })));
        assert_eq!(
            failed,
            json!({"type": "event", "event": "update.changed",
                   "data": {"state": "failed", "version": "0.2.0", "message": "offline"}})
        );
        let current = round_trip(&Message::Event(Event::UpdateChanged(UpdateStatus::UpToDate)));
        assert_eq!(current["data"], json!({"state": "up_to_date"}));
        let waiting = UpdateStatus::WaitingIdle {
            version: "0.2.0".into(),
        };
        assert_eq!(waiting.version(), Some("0.2.0"));
        assert_eq!(UpdateStatus::default().version(), None);
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p mkrew-protocol -j 4 update_wire_shapes`
Expected: compile error, `no variant named UpdateApply` / `cannot find type UpdateStatus`.

- [ ] **Step 3: Write minimal implementation**

At the end of `pub enum Request` (after `SessionPlan(ProjectRef),`):

```rust
    /// The in-app update state (SPEC §8 "Update"); returns `UpdateStatus`.
    #[serde(rename = "update.status")]
    UpdateStatus,
    /// Installs the available update now or once no agent is working; returns `null`,
    /// `invalid_state` when no update is ready or one is already installing.
    #[serde(rename = "update.apply")]
    UpdateApply(UpdateApplyParams),
```

At the end of `pub enum Event` (after `SessionPlan(SessionPlanEvent),`):

```rust
    /// The in-app update state changed (SPEC §8 "Update"). On `installing` clients quit: the
    /// daemon stops and the setup replaces the binaries.
    #[serde(rename = "update.changed")]
    UpdateChanged(UpdateStatus),
```

After `pub struct SessionLinkPlanParams { … }`:

```rust
/// `update.apply` (SPEC §8 "Update").
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UpdateApplyParams {
    pub when: UpdateWhen,
}

/// When `update.apply` installs: at once, or once no agent session is `working`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UpdateWhen {
    Now,
    Idle,
}

/// The in-app update state (SPEC §8 "Update"): `update.status`, `update.changed`.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum UpdateStatus {
    /// No newer release, or updates are off (not Windows, not an installed build, no token).
    #[default]
    UpToDate,
    Available { version: String },
    Downloading { version: String },
    /// Downloaded; installs once no agent session is working.
    WaitingIdle { version: String },
    /// The daemon is stopping to run the setup.
    Installing { version: String },
    /// The download or its check failed; `update.apply` retries.
    Failed { version: String, message: String },
}

impl UpdateStatus {
    /// The release this state is about; `None` when up to date.
    pub fn version(&self) -> Option<&str> {
        match self {
            Self::UpToDate => None,
            Self::Available { version }
            | Self::Downloading { version }
            | Self::WaitingIdle { version }
            | Self::Installing { version }
            | Self::Failed { version, .. } => Some(version),
        }
    }
}
```

In `crates/mkrew-protocol/src/lib.rs`: `pub const PROTOCOL_VERSION: u32 = 38;` (or current + 1, see Global Constraints).

In `crates/mkrew-ui/src/sidebar.rs`, `Sidebar::apply`'s exhaustive `match event`, extend the last arm:

```rust
            | Event::SessionPlan(_)
            | Event::AiGenerated(_)
            | Event::UpdateChanged(_) => Change::None,
```

If `cargo check --workspace` reports another non-exhaustive `match` on `Event` or `Request`, add the new variants there with the arm that ignores events (`Change::None`, `false`, `{}`), never a new behavior.

In `docs/SPEC.md` §7, request table, after the `` | `session.plan` | `project_id` | … `` row:

```markdown
| `update.status` | – | `UpdateStatus` `{state, version?, message?}`: `up_to_date` (also when updates are off) · `available` · `downloading` · `waiting_idle` · `installing` · `failed` (§8 "Update") |
| `update.apply` | `when`: `now` \| `idle` | ok — downloads the available release, then installs at once or once no agent is `working`; `now` while `waiting_idle` installs at once; `failed` retries; `invalid_state` when nothing is ready or it is already downloading/installing |
```

Event table, after the `` | `session.plan` | the checklist … `` row:

```markdown
| `update.changed` | the in-app update state changed (`UpdateStatus`, see `update.status`); on `installing` the UI quits, the daemon stops like `daemon.stop` and runs the setup (§8 "Update") |
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p mkrew-protocol -j 4 && cargo check --workspace --all-targets -j 4`
Expected: PASS; workspace compiles.

- [ ] **Step 5: Commit**

```bash
cargo fmt --all
cargo clippy -p mkrew-protocol -p mkrew-ui --all-targets -j 4 -- -D warnings
git add crates/mkrew-protocol/src/message.rs crates/mkrew-protocol/src/lib.rs crates/mkrew-ui/src/sidebar.rs docs/SPEC.md
git commit -m "feat(protocol): update.status, update.apply and update.changed"
```

---

### Task 2: Daemon — release source (check, download, digest, install line)

**Files:**
- Create: `crates/mkrew-daemon/src/update.rs`
- Modify: `crates/mkrew-daemon/src/lib.rs` (`mod update;` in alphabetical order among the `mod` lines)
- Modify: `crates/mkrew-daemon/Cargo.toml` (`[dependencies]`: `sha2 = "0.10"`)
- Modify: `Cargo.lock` (only the `mkrew-daemon` dependency line `cargo` adds; `sha2 0.10.9` is already locked)

**Interfaces:**
- Consumes: nothing from Task 1 yet.
- Produces (crate-private, used by Task 3 in the same file):
  - `type Version = (u64, u64, u64);` `fn parse_version(tag: &str) -> Option<Version>`
  - `#[derive(Debug, Clone, PartialEq, Eq)] struct Release { version: String, asset: String, url: String, digest: Option<String> }`
  - `fn newer(json: &str, current: Version) -> anyhow::Result<Option<Release>>`
  - `#[derive(Debug, Clone)] struct Source { api: String, token: String, current: Version }` with `fn latest(&self) -> anyhow::Result<Option<Release>>` and `fn download(&self, release: &Release, dir: &Path) -> anyhow::Result<PathBuf>`
  - `fn install_line(setup: &Path, install_dir: &Path, all_users: bool) -> String`
  - `fn all_users(install_dir: &Path, program_files: &[PathBuf]) -> bool`
  - `fn hex(bytes: &[u8]) -> String`
  - consts `REPO = "tungmk97/mkrew"`, `ENV_TOKEN = "MKREW_GITHUB_TOKEN"`, `API = "https://api.github.com"`

Check before Step 3: `Cargo.lock` and `.github/workflows/` must have no uncommitted changes in the worktree (`git status --short`). If they do, stop and ask.

- [ ] **Step 1: Write the failing tests**

Create `crates/mkrew-daemon/src/update.rs` with only the test module first. The pure helpers are tested directly. `latest`/`download` run the real `curl` against `file://` URLs in a temp folder laid out like the API.

```rust
#[cfg(test)]
mod tests {
    use sha2::Digest as _;

    use super::*;

    /// `file://` URL of `path` (curl reads local files; no network).
    pub(super) fn file_url(path: &Path) -> String {
        let path = path.to_string_lossy().replace('\\', "/").replace(' ', "%20");
        if path.starts_with('/') {
            format!("file://{path}")
        } else {
            format!("file:///{path}")
        }
    }

    /// Lays out `<dir>/api/repos/<REPO>/releases/latest` like GitHub: release `tag` with the
    /// setup asset holding `body`; `digest` = `None` → the correct sha256.
    pub(super) fn publish(dir: &Path, tag: &str, body: &[u8], digest: Option<String>) -> Source {
        let api = dir.join("api");
        let releases = api.join("repos").join("tungmk97").join("mkrew").join("releases");
        std::fs::create_dir_all(&releases).unwrap();
        let asset = dir.join("asset.bin");
        std::fs::write(&asset, body).unwrap();
        let version = tag.trim_start_matches('v');
        let digest = digest.unwrap_or_else(|| format!("sha256:{}", hex(&Sha256::digest(body))));
        let json = serde_json::json!({
            "tag_name": tag,
            "assets": [{
                "name": format!("mkrew-{version}-x64-setup.exe"),
                "url": file_url(&asset),
                "digest": digest,
            }],
        });
        std::fs::write(releases.join("latest"), json.to_string()).unwrap();
        Source {
            api: file_url(&api),
            token: "test-token".into(),
            current: (0, 1, 0),
        }
    }

    #[test]
    fn versions_are_three_numbers() {
        assert_eq!(parse_version("v0.2.10"), Some((0, 2, 10)));
        assert_eq!(parse_version("1.0.0"), Some((1, 0, 0)));
        assert_eq!(parse_version("v1.2"), None);
        assert_eq!(parse_version("v1.2.3-beta"), None);
        assert_eq!(parse_version("v1.2.3.4"), None);
        assert!(
            parse_version("v0.10.0") > parse_version("v0.9.9"),
            "numeric order, not text order"
        );
    }

    #[test]
    fn only_a_newer_release_with_the_windows_setup_counts() {
        let json = |tag: &str, asset: &str| {
            serde_json::json!({
                "tag_name": tag,
                "assets": [{"name": asset, "url": "https://x/1", "digest": "sha256:ab"}],
            })
            .to_string()
        };
        let current = (0, 1, 0);
        assert_eq!(
            newer(&json("v0.2.0", "mkrew-0.2.0-x64-setup.exe"), current).unwrap(),
            Some(Release {
                version: "0.2.0".into(),
                asset: "mkrew-0.2.0-x64-setup.exe".into(),
                url: "https://x/1".into(),
                digest: Some("sha256:ab".into()),
            })
        );
        let same = json("v0.1.0", "mkrew-0.1.0-x64-setup.exe");
        assert_eq!(newer(&same, current).unwrap(), None, "same version");
        let older = json("v0.0.9", "mkrew-0.0.9-x64-setup.exe");
        assert_eq!(newer(&older, current).unwrap(), None, "older");
        let msi = json("v0.2.0", "mkrew-0.2.0-x64.msi");
        assert_eq!(newer(&msi, current).unwrap(), None, "no Inno setup");
        let odd = json("nightly", "mkrew-nightly-x64-setup.exe");
        assert_eq!(newer(&odd, current).unwrap(), None, "not a version");
        assert!(newer("<html>rate limited</html>", current).is_err());
    }

    #[test]
    fn the_setup_runs_silently_in_the_installs_scope_then_the_ui_starts() {
        let setup = Path::new(r"C:\Data\updates\mkrew-0.2.0-x64-setup.exe");
        let dir = Path::new(r"C:\Users\me\AppData\Local\Programs\Mkrew");
        assert_eq!(
            install_line(setup, dir, false),
            r#"/d /c start "" /wait "C:\Data\updates\mkrew-0.2.0-x64-setup.exe" /VERYSILENT /SUPPRESSMSGBOXES /NORESTART /CURRENTUSER & start "" "C:\Users\me\AppData\Local\Programs\Mkrew\bin\mkrew.exe""#
        );
        assert!(install_line(setup, dir, true).contains(" /NORESTART /ALLUSERS & "));
    }

    #[test]
    fn program_files_installs_are_per_machine() {
        let program_files = [PathBuf::from(r"C:\Program Files"), PathBuf::new()];
        assert!(all_users(Path::new(r"c:\program files\Mkrew"), &program_files));
        let per_user = Path::new(r"C:\Users\me\AppData\Local\Programs\Mkrew");
        assert!(!all_users(per_user, &program_files));
        assert!(
            !all_users(per_user, &[PathBuf::new()]),
            "an unset variable matches nothing"
        );
    }

    #[test]
    fn latest_and_download_read_the_api_layout() {
        let dir = tempfile::tempdir().unwrap();
        let source = publish(dir.path(), "v0.2.0", b"setup bytes", None);
        let release = source.latest().unwrap().expect("a newer release");
        assert_eq!(release.version, "0.2.0");
        let setup = source.download(&release, &dir.path().join("updates")).unwrap();
        assert_eq!(setup, dir.path().join("updates").join("mkrew-0.2.0-x64-setup.exe"));
        assert_eq!(std::fs::read(&setup).unwrap(), b"setup bytes");
    }

    #[test]
    fn download_rejects_a_wrong_digest() {
        let dir = tempfile::tempdir().unwrap();
        let wrong = format!("sha256:{}", hex(&Sha256::digest(b"other")));
        let source = publish(dir.path(), "v0.2.0", b"setup bytes", Some(wrong));
        let release = source.latest().unwrap().unwrap();
        let updates = dir.path().join("updates");
        let error = source.download(&release, &updates).unwrap_err();
        assert!(format!("{error:#}").contains("sha256"), "{error:#}");
        assert!(!updates.join(&release.asset).exists(), "a bad file is deleted");
    }

    #[test]
    fn a_missing_api_is_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let mut source = publish(dir.path(), "v0.2.0", b"x", None);
        source.api = file_url(&dir.path().join("missing"));
        assert!(source.latest().is_err());
    }
}
```

Add `mod update;` to `crates/mkrew-daemon/src/lib.rs` and `sha2 = "0.10"` to `crates/mkrew-daemon/Cargo.toml` `[dependencies]` (alphabetical, after `serde_json`).

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p mkrew-daemon --lib -j 4 update::`
Expected: compile errors (`cannot find function parse_version`, …).

- [ ] **Step 3: Write minimal implementation**

Put this above the test module in `crates/mkrew-daemon/src/update.rs`:

```rust
//! In-app update (SPEC §8 "Update", ADR NNNN). The daemon asks GitHub for the latest release
//! of [`REPO`] (private: the token comes from [`ENV_TOKEN`]) through `curl`, downloads the Inno
//! setup on `update.apply` and checks its sha256. Then, in the order of [`install_line`], it
//! runs the setup silently and starts the UI again.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use anyhow::{Context as _, bail};
use serde::Deserialize;
use sha2::{Digest as _, Sha256};

/// The GitHub repository whose releases are installed.
pub const REPO: &str = "tungmk97/mkrew";
/// A GitHub token that can read [`REPO`]'s releases; unset = updates off.
pub const ENV_TOKEN: &str = "MKREW_GITHUB_TOKEN";
const API: &str = "https://api.github.com";

/// `major.minor.patch`.
type Version = (u64, u64, u64);

/// `v1.2.3` or `1.2.3`; anything else (pre-release, two parts) is not an update.
fn parse_version(tag: &str) -> Option<Version> {
    let mut parts = tag.strip_prefix('v').unwrap_or(tag).split('.');
    let mut next = || parts.next()?.parse::<u64>().ok();
    let version = (next()?, next()?, next()?);
    parts.next().is_none().then_some(version)
}

/// A release newer than the running build, with its Windows setup.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Release {
    /// `x.y.z`.
    version: String,
    /// The setup's file name, `mkrew-x.y.z-x64-setup.exe`.
    asset: String,
    /// The asset's API URL (download with `Accept: application/octet-stream`).
    url: String,
    /// GitHub's `sha256:<hex>`; absent on assets uploaded before June 2025.
    digest: Option<String>,
}

#[derive(Deserialize)]
struct LatestJson {
    tag_name: String,
    #[serde(default)]
    assets: Vec<AssetJson>,
}

#[derive(Deserialize)]
struct AssetJson {
    name: String,
    url: String,
    #[serde(default)]
    digest: Option<String>,
}

/// The release in `json` (a `releases/latest` body) when it is newer than `current` and has
/// the Inno setup; `None` otherwise.
fn newer(json: &str, current: Version) -> anyhow::Result<Option<Release>> {
    let latest: LatestJson = serde_json::from_str(json).context("read releases/latest")?;
    let Some(found) = parse_version(&latest.tag_name).filter(|v| *v > current) else {
        return Ok(None);
    };
    let version = format!("{}.{}.{}", found.0, found.1, found.2);
    let asset = format!("mkrew-{version}-x64-setup.exe");
    let Some(file) = latest.assets.into_iter().find(|a| a.name == asset) else {
        tracing::warn!("release {} has no {asset}", latest.tag_name);
        return Ok(None);
    };
    Ok(Some(Release {
        version,
        asset,
        url: file.url,
        digest: file.digest,
    }))
}

/// Where releases come from: the GitHub API (tests: a `file://` folder laid out like it).
#[derive(Debug, Clone)]
struct Source {
    api: String,
    token: String,
    /// The running build's version.
    current: Version,
}

impl Source {
    fn latest(&self) -> anyhow::Result<Option<Release>> {
        let url = format!("{}/repos/{REPO}/releases/latest", self.api);
        let body = curl(&self.token, "application/vnd.github+json", &url, None)?;
        newer(&String::from_utf8_lossy(&body), self.current)
    }

    /// Downloads the setup into `dir` and checks its digest; a file that fails is deleted.
    fn download(&self, release: &Release, dir: &Path) -> anyhow::Result<PathBuf> {
        std::fs::create_dir_all(dir).with_context(|| format!("create {}", dir.display()))?;
        let file = dir.join(&release.asset);
        curl(&self.token, "application/octet-stream", &release.url, Some(&file))?;
        let checked = match &release.digest {
            Some(digest) => check_digest(&file, digest),
            None => Ok(()),
        };
        if let Err(e) = checked {
            let _ = std::fs::remove_file(&file);
            return Err(e);
        }
        Ok(file)
    }
}

/// Runs `curl` (shipped with Windows since 10 1803): the body goes to `out` or is returned.
/// `-f` makes HTTP errors fail; on the redirect of a download to another host curl drops the
/// `Authorization` header by itself.
fn curl(token: &str, accept: &str, url: &str, out: Option<&Path>) -> anyhow::Result<Vec<u8>> {
    let max_time = if out.is_some() { "900" } else { "30" };
    let mut command = Command::new("curl");
    command
        .args(["-fsSL", "--connect-timeout", "20", "--max-time", max_time])
        .args(["-H", &format!("Authorization: Bearer {token}")])
        .args(["-H", &format!("Accept: {accept}")])
        .args(["-H", "X-GitHub-Api-Version: 2022-11-28", "-A", "mkrew"])
        .stdin(Stdio::null());
    if let Some(out) = out {
        command.arg("-o").arg(out);
    }
    command.arg(url);
    no_window(&mut command);
    let output = command.output().context("run curl")?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("curl {url}: {}", stderr.trim());
    }
    Ok(output.stdout)
}

fn no_window(command: &mut Command) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // The daemon has no console; without this every curl call would open one.
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    #[cfg(not(windows))]
    let _ = command;
}

/// `digest` = `sha256:<hex>` (GitHub's asset `digest`).
fn check_digest(file: &Path, digest: &str) -> anyhow::Result<()> {
    let expected = digest
        .strip_prefix("sha256:")
        .with_context(|| format!("unsupported digest {digest:?}"))?;
    let mut reader =
        std::fs::File::open(file).with_context(|| format!("open {}", file.display()))?;
    let mut hasher = Sha256::new();
    std::io::copy(&mut reader, &mut hasher).with_context(|| format!("read {}", file.display()))?;
    if hex(&hasher.finalize()).eq_ignore_ascii_case(expected) {
        Ok(())
    } else {
        bail!("{} does not match its sha256 digest", file.display())
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// The `cmd` arguments that run the setup silently in the scope of the current install,
/// wait for it, then start the UI again. The UI also starts after a failed setup, so Mkrew
/// always comes back. `start` goes through the shell, so a `/ALLUSERS` setup can ask for UAC.
fn install_line(setup: &Path, install_dir: &Path, all_users: bool) -> String {
    let scope = if all_users { "/ALLUSERS" } else { "/CURRENTUSER" };
    format!(
        r#"/d /c start "" /wait "{}" /VERYSILENT /SUPPRESSMSGBOXES /NORESTART {scope} & start "" "{}\bin\mkrew.exe""#,
        setup.display(),
        install_dir.display(),
    )
}

/// An install under Program Files is per machine (`/ALLUSERS`); others are per user.
fn all_users(install_dir: &Path, program_files: &[PathBuf]) -> bool {
    let dir = install_dir.to_string_lossy().to_lowercase();
    program_files.iter().any(|root| {
        let root = root.to_string_lossy().to_lowercase();
        !root.is_empty() && dir.starts_with(&root)
    })
}
```

This task adds items that Task 3 uses: `API`, `ENV_TOKEN`, `install_line`, `all_users`. Until then, `cargo clippy` reports them as dead code. Add `#![allow(dead_code)] // ponytail: removed in Task 3` as the first line after the module doc comment. Task 3 deletes it.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p mkrew-daemon --lib -j 4 update::`
Expected: 7 tests PASS. If `latest_and_download_read_the_api_layout` fails with a curl "URL using bad/illegal format", print `file_url(...)` and fix `file_url` (Windows needs `file:///C:/…`).

- [ ] **Step 5: Commit**

```bash
cargo fmt --all
cargo clippy -p mkrew-daemon --all-targets -j 4 -- -D warnings
git add crates/mkrew-daemon/src/update.rs crates/mkrew-daemon/src/lib.rs crates/mkrew-daemon/Cargo.toml Cargo.lock
git commit -m "feat(daemon): find, download and verify the latest release setup"
```

---

### Task 3: Daemon — update state machine, `update.*` requests, stop-and-install

**Files:**
- Modify: `crates/mkrew-daemon/src/update.rs` (add `Updater`, remove the `#![allow(dead_code)]` line, more tests)
- Modify: `crates/mkrew-daemon/src/awake.rs` (`fn working_agents` → `pub(crate) fn working_agents`)
- Modify: `crates/mkrew-daemon/src/server.rs` (`Daemon` field `update`, `Daemon::new`, `Daemon::serve`, `handle` arms)
- Modify: `crates/mkrew-daemon/tests/daemon.rs` (one test)
- Create: `docs/decisions/NNNN-in-app-update.md`
- Modify: `docs/SPEC.md` §8 (new bullet after `- **Shutdown:** …`)

**Interfaces:**
- Consumes: Task 1 `UpdateStatus`, `UpdateWhen`, `UpdateApplyParams`, `Request::UpdateStatus`, `Request::UpdateApply`, `Event::UpdateChanged`; Task 2 `Source`, `Release`, `parse_version`, `install_line`, `all_users`, `API`, `ENV_TOKEN`.
- Produces:
  - `pub struct Updater` with `pub fn from_env(hub: Arc<Hub>, data_dir: &Path, shutdown: watch::Sender<bool>) -> Arc<Self>`, `fn new(hub: Arc<Hub>, source: Option<Source>, install_dir: PathBuf, all_users: bool, downloads: PathBuf, shutdown: watch::Sender<bool>, idle_poll: Duration) -> Arc<Self>`, `pub fn status(&self) -> UpdateStatus`, `pub async fn check(self: &Arc<Self>)`, `pub fn start(self: Arc<Self>) -> JoinHandle<()>`, `pub fn apply(self: &Arc<Self>, when: UpdateWhen) -> Result<(), String>`, `pub fn launch(&self)`.
  - `pub(crate) fn awake::working_agents(hub: &Hub) -> HashSet<Ulid>`.

- [ ] **Step 1: Write the failing tests**

Append to `mod tests` in `crates/mkrew-daemon/src/update.rs`. The fixture copies `awake.rs`'s `setup()`/`Setup::add` (same `Session` literal):

```rust
    use std::sync::Arc;
    use std::time::Duration;

    use mkrew_protocol::{Event, Session, SessionKind, SessionState, StateSource, UpdateStatus, UpdateWhen};
    use mkrew_store::Store;
    use tokio::sync::watch;
    use ulid::Ulid;

    use crate::session::Hub;

    struct Fixture {
        dir: tempfile::TempDir,
        hub: Arc<Hub>,
        project_id: Ulid,
    }

    fn fixture() -> Fixture {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(&dir.path().join("mkrew.db")).unwrap();
        let project_id = store.add_project(dir.path(), Some("repo")).unwrap().id;
        Fixture {
            hub: Arc::new(Hub::new(store)),
            project_id,
            dir,
        }
    }

    impl Fixture {
        fn add(&self, kind: SessionKind, state: SessionState) -> Ulid {
            let id = Ulid::new();
            self.hub
                .store()
                .insert_session(&Session {
                    id,
                    task_id: None,
                    project_id: self.project_id,
                    agent_profile: "claude".into(),
                    kind,
                    title: None,
                    terminal_title: None,
                    color: None,
                    native_session_id: None,
                    state,
                    state_source: StateSource::Daemon,
                    state_changed_at: 0,
                    exit_code: None,
                    cols: 80,
                    rows: 24,
                    started_at: 0,
                    ended_at: None,
                    pinned: false,
                    plan_path: None,
                })
                .unwrap();
            id
        }

        fn set_state(&self, id: Ulid, state: SessionState) {
            let store = self.hub.store();
            let mut session = store.get_session(id).unwrap();
            session.state = state;
            store.update_session(&session).unwrap();
        }

        fn updater(&self, source: Option<Source>) -> (Arc<Updater>, watch::Receiver<bool>) {
            let (shutdown, stopped) = watch::channel(false);
            let updater = Updater::new(
                Arc::clone(&self.hub),
                source,
                self.dir.path().join("install"),
                false,
                self.dir.path().join("updates"),
                shutdown,
                Duration::from_millis(10),
            );
            (updater, stopped)
        }

        /// An updater that already saw release v0.2.0 (setup = `b"setup"`).
        async fn available(&self) -> (Arc<Updater>, watch::Receiver<bool>) {
            let source = publish(self.dir.path(), "v0.2.0", b"setup", None);
            let made = self.updater(Some(source));
            made.0.check().await;
            assert_eq!(made.0.status(), UpdateStatus::Available { version: "0.2.0".into() });
            made
        }
    }

    /// Polls until `done` holds; curl is a real process, so allow 10 s.
    async fn until(updater: &Updater, done: impl Fn(&UpdateStatus) -> bool) -> UpdateStatus {
        let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
        loop {
            let status = updater.status();
            if done(&status) {
                return status;
            }
            assert!(tokio::time::Instant::now() < deadline, "stuck at {status:?}");
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }

    fn installing(status: &UpdateStatus) -> bool {
        matches!(status, UpdateStatus::Installing { .. })
    }

    fn waiting(status: &UpdateStatus) -> bool {
        matches!(status, UpdateStatus::WaitingIdle { .. })
    }

    #[tokio::test]
    async fn check_announces_a_newer_release_once() {
        let s = fixture();
        let source = publish(s.dir.path(), "v0.2.0", b"setup", None);
        let (updater, _stopped) = s.updater(Some(source));
        let mut events = s.hub.events.subscribe();
        updater.check().await;
        let available = UpdateStatus::Available { version: "0.2.0".into() };
        assert_eq!(updater.status(), available);
        assert_eq!(events.try_recv().unwrap(), Event::UpdateChanged(available));
        updater.check().await;
        assert!(events.try_recv().is_err(), "same release: no second event");
    }

    #[tokio::test]
    async fn a_failed_check_changes_nothing() {
        let s = fixture();
        let mut source = publish(s.dir.path(), "v0.2.0", b"setup", None);
        source.api = file_url(&s.dir.path().join("missing"));
        let (updater, _stopped) = s.updater(Some(source));
        let mut events = s.hub.events.subscribe();
        updater.check().await;
        assert_eq!(updater.status(), UpdateStatus::UpToDate);
        assert!(events.try_recv().is_err());
    }

    #[tokio::test]
    async fn apply_needs_an_available_update() {
        let s = fixture();
        let (updater, _stopped) = s.updater(None);
        assert!(updater.apply(UpdateWhen::Now).is_err());
        assert_eq!(updater.status(), UpdateStatus::UpToDate);
    }

    #[tokio::test]
    async fn now_installs_even_while_an_agent_works() {
        let s = fixture();
        s.add(SessionKind::Agent, SessionState::Working);
        let (updater, stopped) = s.available().await;
        updater.apply(UpdateWhen::Now).unwrap();
        until(&updater, installing).await;
        assert!(*stopped.borrow(), "the daemon was asked to stop");
        let setup = updater.lock().pending.clone().unwrap();
        assert_eq!(std::fs::read(&setup).unwrap(), b"setup");
        assert!(updater.apply(UpdateWhen::Now).is_err(), "already installing");
    }

    #[tokio::test]
    async fn idle_waits_for_working_agents_only() {
        let s = fixture();
        let agent = s.add(SessionKind::Agent, SessionState::Working);
        s.add(SessionKind::Shell, SessionState::Working);
        s.add(SessionKind::Agent, SessionState::NeedsInput);
        let (updater, stopped) = s.available().await;
        updater.apply(UpdateWhen::Idle).unwrap();
        until(&updater, waiting).await;
        tokio::time::sleep(Duration::from_millis(100)).await;
        assert!(waiting(&updater.status()), "still waiting: {:?}", updater.status());
        assert!(!*stopped.borrow());
        s.set_state(agent, SessionState::Idle);
        until(&updater, installing).await;
        assert!(*stopped.borrow());
    }

    #[tokio::test]
    async fn now_cuts_the_wait_for_idle() {
        let s = fixture();
        s.add(SessionKind::Agent, SessionState::Working);
        let (updater, stopped) = s.available().await;
        updater.apply(UpdateWhen::Idle).unwrap();
        until(&updater, waiting).await;
        updater.apply(UpdateWhen::Now).unwrap();
        until(&updater, installing).await;
        assert!(*stopped.borrow());
    }

    #[tokio::test]
    async fn a_corrupt_download_fails_and_can_be_retried() {
        let s = fixture();
        let wrong = format!("sha256:{}", hex(&Sha256::digest(b"other")));
        let source = publish(s.dir.path(), "v0.2.0", b"setup", Some(wrong));
        let (updater, stopped) = s.updater(Some(source));
        updater.check().await;
        updater.apply(UpdateWhen::Now).unwrap();
        let failed = until(&updater, |st| matches!(st, UpdateStatus::Failed { .. })).await;
        let UpdateStatus::Failed { message, .. } = failed else {
            unreachable!("matched above")
        };
        assert!(message.contains("sha256"), "{message}");
        let file = s.dir.path().join("updates").join("mkrew-0.2.0-x64-setup.exe");
        assert!(!file.exists(), "a bad setup is deleted");
        assert!(!*stopped.borrow());
        updater.check().await;
        assert!(
            matches!(updater.status(), UpdateStatus::Failed { .. }),
            "the hourly check keeps the failure visible"
        );
        assert!(updater.apply(UpdateWhen::Now).is_ok(), "retry is allowed");
    }
```

`publish` and `file_url` are the Task 2 helpers in the same `mod tests`; call them directly. Merge the `use` lines with the existing ones in `mod tests`; keep `use super::*;`.

In `crates/mkrew-daemon/tests/daemon.rs`, next to the other `daemon.stop` tests (add `UpdateApplyParams, UpdateStatus, UpdateWhen` to the file's `mkrew_protocol` import):

```rust
/// A daemon outside an installed build (here: `target\debug`) never offers or runs an update,
/// even with a token set: `cargo run` must not install over the user's real Mkrew.
#[test]
fn update_is_off_for_a_dev_build() {
    let env = Env::new();
    let (mut daemon, mut client) = env.start();
    let status = client
        .call(Request::UpdateStatus)
        .into_result::<UpdateStatus>()
        .unwrap();
    assert_eq!(status, UpdateStatus::UpToDate);
    let refused = client.call(Request::UpdateApply(UpdateApplyParams {
        when: UpdateWhen::Now,
    }));
    assert_eq!(error_code(refused), ErrorCode::InvalidState);
    client.call(Request::DaemonStop).into_result::<()>().unwrap();
    assert!(daemon.wait_exit().success());
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p mkrew-daemon --lib -j 4 update::`
Expected: compile errors (`cannot find type Updater`).

- [ ] **Step 3: Write minimal implementation**

In `crates/mkrew-daemon/src/update.rs`: delete the `#![allow(dead_code)]` line. Extend the module doc with: "[`Updater`] holds the state (`update.status`), checks at start and every hour, and on `update.apply` downloads, waits for idle agents if asked, then stops the daemon like `daemon.stop`; [`Updater::launch`] runs the setup once every session is down. Off unless Windows, an installed build (`unins000.exe` beside `bin\`) and [`ENV_TOKEN`]." Then add these imports and items (keep the Task 2 code):

```rust
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use mkrew_protocol::{Event, UpdateStatus, UpdateWhen};
use tokio::sync::watch;
use tokio::task::JoinHandle;

use crate::session::Hub;

/// How often GitHub is asked (monotonic; paused while the machine sleeps, like backups).
const EVERY: Duration = Duration::from_secs(60 * 60);
/// How often "update when idle" looks at the agents.
const IDLE_POLL: Duration = Duration::from_secs(2);

/// The update state shared by `update.*` requests and the background tasks.
pub struct Updater {
    hub: Arc<Hub>,
    /// `None` = updates off.
    source: Option<Source>,
    /// The install folder (`{app}`): the parent of the daemon's `bin\`.
    install_dir: PathBuf,
    all_users: bool,
    downloads: PathBuf,
    /// The daemon's `daemon.stop` signal.
    shutdown: watch::Sender<bool>,
    idle_poll: Duration,
    state: Mutex<State>,
}

#[derive(Default)]
struct State {
    status: UpdateStatus,
    release: Option<Release>,
    /// `update.apply` asked for `now` (also while waiting for idle).
    now: bool,
    /// The downloaded setup, run by [`Updater::launch`] after the sessions stopped.
    pending: Option<PathBuf>,
}

impl Updater {
    /// Updates are on for an installed Windows build with [`ENV_TOKEN`] set. The setup of the
    /// last update (in `<data>/updates`) is deleted.
    pub fn from_env(hub: Arc<Hub>, data_dir: &Path, shutdown: watch::Sender<bool>) -> Arc<Self> {
        let downloads = data_dir.join("updates");
        let _ = std::fs::remove_dir_all(&downloads);
        let install_dir = std::env::current_exe()
            .ok()
            .and_then(|exe| Some(exe.parent()?.parent()?.to_path_buf()))
            .unwrap_or_default();
        let installed = cfg!(windows) && install_dir.join("unins000.exe").is_file();
        let token = std::env::var(ENV_TOKEN)
            .ok()
            .map(|token| token.trim().to_owned())
            .filter(|token| installed && !token.is_empty());
        let source = token.and_then(|token| {
            Some(Source {
                api: API.into(),
                token,
                current: parse_version(env!("CARGO_PKG_VERSION"))?,
            })
        });
        if source.is_none() {
            tracing::info!("in-app update off: needs an installed Windows build and {ENV_TOKEN}");
        }
        let program_files: Vec<PathBuf> = ["ProgramFiles", "ProgramW6432"]
            .into_iter()
            .filter_map(std::env::var_os)
            .map(PathBuf::from)
            .collect();
        let all_users = all_users(&install_dir, &program_files);
        Self::new(hub, source, install_dir, all_users, downloads, shutdown, IDLE_POLL)
    }

    fn new(
        hub: Arc<Hub>,
        source: Option<Source>,
        install_dir: PathBuf,
        all_users: bool,
        downloads: PathBuf,
        shutdown: watch::Sender<bool>,
        idle_poll: Duration,
    ) -> Arc<Self> {
        Arc::new(Self {
            hub,
            source,
            install_dir,
            all_users,
            downloads,
            shutdown,
            idle_poll,
            state: Mutex::default(),
        })
    }

    fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Stores and broadcasts `status` (`update.changed`).
    fn set(&self, state: &mut State, status: UpdateStatus) {
        state.status = status.clone();
        let _ = self.hub.events.send(Event::UpdateChanged(status));
    }

    pub fn status(&self) -> UpdateStatus {
        self.lock().status.clone()
    }

    /// Asks for the latest release. A newer one becomes `available`. Errors (offline, rate
    /// limit, a bad body) are only logged, and the next check retries. A download, a wait or a
    /// failure is never replaced.
    pub async fn check(self: &Arc<Self>) {
        let Some(source) = self.source.clone() else {
            return;
        };
        let found = match tokio::task::spawn_blocking(move || source.latest()).await {
            Ok(Ok(found)) => found,
            Ok(Err(e)) => {
                tracing::warn!("update check failed: {e:#}");
                return;
            }
            Err(e) => {
                tracing::warn!("update check stopped: {e}");
                return;
            }
        };
        let Some(release) = found else {
            return;
        };
        let mut state = self.lock();
        let quiet = matches!(
            state.status,
            UpdateStatus::UpToDate | UpdateStatus::Available { .. }
        );
        if quiet && state.release.as_ref() != Some(&release) {
            let version = release.version.clone();
            state.release = Some(release);
            self.set(&mut state, UpdateStatus::Available { version });
        }
    }

    /// Checks at once, then every [`EVERY`], until aborted; ends at once when updates are off.
    pub fn start(self: Arc<Self>) -> JoinHandle<()> {
        tokio::spawn(async move {
            if self.source.is_none() {
                return;
            }
            let mut ticks = tokio::time::interval(EVERY);
            ticks.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
            loop {
                ticks.tick().await;
                self.check().await;
            }
        })
    }

    /// `update.apply`: downloads the available release, then installs at once (`now`) or when
    /// no agent is working (`idle`). `now` while waiting cuts the wait; `failed` retries.
    pub fn apply(self: &Arc<Self>, when: UpdateWhen) -> Result<(), String> {
        let mut state = self.lock();
        match state.status {
            UpdateStatus::WaitingIdle { .. } => {
                state.now |= when == UpdateWhen::Now;
                return Ok(());
            }
            UpdateStatus::Available { .. } | UpdateStatus::Failed { .. } => {}
            UpdateStatus::UpToDate => return Err("no update is available".into()),
            UpdateStatus::Downloading { .. } | UpdateStatus::Installing { .. } => {
                return Err("the update is already in progress".into());
            }
        }
        let Some(release) = state.release.clone() else {
            return Err("no update is available".into());
        };
        state.now = when == UpdateWhen::Now;
        let version = release.version.clone();
        self.set(&mut state, UpdateStatus::Downloading { version });
        drop(state);
        tokio::spawn(Arc::clone(self).install(release));
        Ok(())
    }

    async fn install(self: Arc<Self>, release: Release) {
        let Some(source) = self.source.clone() else {
            return;
        };
        let version = release.version.clone();
        let dir = self.downloads.clone();
        let setup = match tokio::task::spawn_blocking(move || source.download(&release, &dir)).await
        {
            Ok(Ok(setup)) => setup,
            Ok(Err(e)) => return self.fail(&version, format!("{e:#}")),
            Err(e) => return self.fail(&version, e.to_string()),
        };
        let mut waiting = false;
        loop {
            {
                let mut state = self.lock();
                if state.now || crate::awake::working_agents(&self.hub).is_empty() {
                    state.pending = Some(setup);
                    self.set(&mut state, UpdateStatus::Installing { version: version.clone() });
                    break;
                }
                if !waiting {
                    waiting = true;
                    self.set(&mut state, UpdateStatus::WaitingIdle { version: version.clone() });
                }
            }
            tokio::time::sleep(self.idle_poll).await;
        }
        tracing::info!("installing update {version}: stopping the daemon");
        self.shutdown.send_replace(true);
    }

    fn fail(&self, version: &str, message: String) {
        tracing::warn!("update {version} failed: {message}");
        let mut state = self.lock();
        let failed = UpdateStatus::Failed {
            version: version.into(),
            message,
        };
        self.set(&mut state, failed);
    }

    /// After every session stopped: starts the pending setup, detached (see [`install_line`]).
    pub fn launch(&self) {
        let Some(setup) = self.lock().pending.take() else {
            return;
        };
        let line = install_line(&setup, &self.install_dir, self.all_users);
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            let mut command = Command::new("cmd");
            command
                .raw_arg(&line)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null());
            crate::spawn::detach(&mut command);
            match command.spawn() {
                Ok(_) => tracing::info!("update setup started: cmd {line}"),
                Err(e) => tracing::error!("cannot start the update setup (cmd {line}): {e}"),
            }
        }
        #[cfg(not(windows))]
        tracing::warn!("in-app update installs only on Windows; skipped: cmd {line}");
    }
}
```

If the borrow checker rejects `state.pending = Some(setup)` inside the loop ("value moved in previous iteration"), move `setup` into an `Option` before the loop (`let mut setup = Some(setup);` … `state.pending = setup.take();`).

In `crates/mkrew-daemon/src/awake.rs`: `fn working_agents(hub: &Hub)` → `pub(crate) fn working_agents(hub: &Hub)`.

In `crates/mkrew-daemon/src/server.rs`:

1. `use crate::{awake, backup, plan, session_log, undo, update, worktree};`
2. `struct Daemon`: add a field after `shutdown`:
   ```rust
       /// In-app update (SPEC §8 "Update"); stops the daemon through `shutdown` to install.
       update: Arc<update::Updater>,
   ```
3. `Daemon::new`: create the stop signal first and hand a clone to the updater:
   ```rust
       pub fn new(store: Store, paths: Paths) -> Arc<Self> {
           let hub = Arc::new(Hub::new(store));
           let shutdown = watch::channel(false).0;
           let update =
               update::Updater::from_env(Arc::clone(&hub), &paths.data_dir, shutdown.clone());
           Arc::new(Self {
               git: Arc::clone(&hub.git),
               hub,
               settings: Arc::new(SettingsHolder::load(paths.settings_file())),
               paths,
               git_cache: Arc::default(),
               started_at: now_ms(),
               shutdown,
               update,
               run_starting: tokio::sync::Mutex::new(()),
           })
       }
   ```
4. `Daemon::serve`: after `let backups = backup::start(…);` add `let update_checks = Arc::clone(&self.update).start();`. After `backups.abort();` add `update_checks.abort();`. After `self.hub.shutdown().await;` add:
   ```rust
           // Every session is down (agents marked interrupted): the setup may replace the exes.
           self.update.launch();
   ```
   Also add to the `serve` doc comment: "checks for updates hourly; after the sessions stop, runs a pending update setup".
5. `handle`'s `match request`, next to `Request::DaemonStop`:
   ```rust
               Request::UpdateStatus => (ok(&self.update.status()), After::Continue),
               Request::UpdateApply(params) => {
                   let outcome = match self.update.apply(params.when) {
                       Ok(()) => ok(&()),
                       Err(message) => Outcome::error(ErrorCode::InvalidState, message),
                   };
                   (outcome, After::Continue)
               }
   ```

`docs/SPEC.md` §8, after the `- **Shutdown:** …` bullet:

```markdown
- **Update** (ADR NNNN): on an installed Windows build (`unins000.exe` beside `bin\`) with
  `MKREW_GITHUB_TOKEN` set, the daemon asks `GET /repos/tungmk97/mkrew/releases/latest` through
  `curl` at start and every hour. If the tag `vX.Y.Z` is newer than the running version and the
  release has the asset `mkrew-X.Y.Z-x64-setup.exe`, the state becomes `available`
  (`update.changed`). Errors are only logged. `update.apply` downloads the asset to
  `<data>/updates/` and checks GitHub's `sha256` digest (a mismatch deletes it → `failed`). With
  `idle`, it waits (`waiting_idle`) until no agent session is `working`; shells never block.
  Then it sends `installing` (clients quit) and stops like `daemon.stop`, so agents are marked
  interrupted and resume on the next start. Once every session is down, it starts
  `cmd /c start "" /wait <setup> /VERYSILENT /SUPPRESSMSGBOXES /NORESTART /ALLUSERS|/CURRENTUSER & start "" <app>\bin\mkrew.exe`
  (`/ALLUSERS` when the install is under Program Files). The new UI starts the new daemon.
  Shell and `run:` panes do not come back. Without a token, on other OSes, and in dev builds,
  the state stays `up_to_date`.
```

Create `docs/decisions/NNNN-in-app-update.md` (copy the shape of `docs/decisions/0000-template.md`; content):

```markdown
# NNNN — In-app update restarts the daemon and relies on resume

- Date: 2026-10-10
- Status: accepted

## Context

The user wants a "new version" notice in the open app and a one-click update that keeps
everything as it was. Windows locks running executables, and the daemon owns every ConPTY, so
replacing `mkrew-daemon.exe` ends every PTY. The repo `tungmk97/mkrew` is private, and releases
are Inno setups built locally (`mkrew-build-setup`).

## Decision

- The daemon checks GitHub Releases (`releases/latest`) at start and hourly with the user's
  `MKREW_GITHUB_TOKEN`, through `curl.exe` (no HTTP crate).
- `update.apply` downloads the Inno setup, verifies GitHub's sha256 digest, optionally waits until
  no agent is working, then stops like `daemon.stop` (agents marked interrupted, ADR 0017) and runs
  the setup silently in the install's scope, then starts the UI again.
- Off without a token, outside Windows, and for builds without `unins000.exe` (dev builds).

## Alternatives

- Separate PTY host process so the daemon can restart without losing PTYs: zero loss, but a new
  process boundary and protocol; offered and not chosen.
- Hand the ConPTY handles to a new daemon: `HPCON` is not transferable; rejected.
- CI MSI as the update payload: a different product (UpgradeCode, Program Files, per-machine) that
  would install a second copy; rejected.
- Embedding a read-only token in the binary: anyone can extract it; rejected.

## Consequences

A running turn is cut on "Update now". Shell and `run:` panes end. Releases must be published (not
draft) with the setup asset, and the tag must match the Cargo version. Each machine needs its own
token while the repo is private.
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p mkrew-daemon --lib -j 4 update:: && cargo test -p mkrew-daemon --test daemon -j 4 update_is_off_for_a_dev_build`
Expected: all `update::` tests (7 from Task 2 + 7 new) PASS, and the integration test PASSES.

- [ ] **Step 5: Commit**

```bash
cargo fmt --all
cargo clippy -p mkrew-daemon --all-targets -j 4 -- -D warnings
git add crates/mkrew-daemon/src/update.rs crates/mkrew-daemon/src/awake.rs crates/mkrew-daemon/src/server.rs crates/mkrew-daemon/tests/daemon.rs docs/SPEC.md docs/decisions/NNNN-in-app-update.md
git commit -m "feat(daemon): install updates from GitHub releases and resume agents"
```

---

### Task 4: UI view-model — title-bar button and restart impact

**Files:**
- Create: `crates/mkrew-ui/src/update.rs`
- Modify: `crates/mkrew-ui/src/lib.rs` (`pub mod update;`, alphabetical)

**Interfaces:**
- Consumes: Task 1 `UpdateStatus`, `mkrew_protocol::{Session, SessionKind, SessionState}`.
- Produces:
  - `pub struct Button { pub label: String, pub tooltip: String, pub enabled: bool }` (`Debug, Clone, PartialEq, Eq`)
  - `pub fn button(status: &UpdateStatus) -> Option<Button>`
  - `pub struct Impact { pub working: usize, pub shells: usize }` (`Debug, Clone, Copy, PartialEq, Eq, Default`) with `pub fn of(sessions: &[Session]) -> Self`, `pub fn is_none(&self) -> bool`, `pub fn offers_idle(&self, status: &UpdateStatus) -> bool`, `pub fn sentence(&self) -> String`

- [ ] **Step 1: Write the failing tests**

```rust
#[cfg(test)]
mod tests {
    use mkrew_protocol::StateSource;
    use ulid::Ulid;

    use super::*;

    fn session(kind: SessionKind, state: SessionState) -> Session {
        Session {
            id: Ulid::new(),
            task_id: None,
            project_id: Ulid::new(),
            agent_profile: "claude".into(),
            kind,
            title: None,
            terminal_title: None,
            color: None,
            native_session_id: None,
            state,
            state_source: StateSource::Daemon,
            state_changed_at: 0,
            exit_code: None,
            cols: 80,
            rows: 24,
            started_at: 0,
            ended_at: None,
            pinned: false,
            plan_path: None,
        }
    }

    fn v(version: &str) -> String {
        version.into()
    }

    #[test]
    fn the_button_follows_the_update_state() {
        assert_eq!(button(&UpdateStatus::UpToDate), None);
        let available = button(&UpdateStatus::Available { version: v("0.2.0") }).unwrap();
        assert_eq!(available.label, "Update to v0.2.0");
        assert!(available.enabled);
        let downloading = button(&UpdateStatus::Downloading { version: v("0.2.0") }).unwrap();
        assert!(!downloading.enabled, "no second click while downloading");
        let waiting = button(&UpdateStatus::WaitingIdle { version: v("0.2.0") }).unwrap();
        assert!(waiting.enabled, "a click can still install now");
        let installing = button(&UpdateStatus::Installing { version: v("0.2.0") }).unwrap();
        assert!(!installing.enabled);
        let failed = button(&UpdateStatus::Failed {
            version: v("0.2.0"),
            message: v("curl: (6) Could not resolve host"),
        })
        .unwrap();
        assert!(failed.enabled, "retry");
        assert!(failed.label.contains("failed"), "{}", failed.label);
        assert_eq!(failed.tooltip, "curl: (6) Could not resolve host");
    }

    #[test]
    fn impact_counts_working_agents_and_live_shells() {
        let sessions = [
            session(SessionKind::Agent, SessionState::Working),
            session(SessionKind::Agent, SessionState::Working),
            session(SessionKind::Agent, SessionState::Idle),
            session(SessionKind::Agent, SessionState::NeedsInput),
            session(SessionKind::Shell, SessionState::Idle),
            session(SessionKind::Shell, SessionState::Exited),
            session(SessionKind::Agent, SessionState::Exited),
        ];
        assert_eq!(Impact::of(&sessions), Impact { working: 2, shells: 1 });
        let calm = [session(SessionKind::Agent, SessionState::Idle)];
        assert!(Impact::of(&calm).is_none(), "idle agents resume: no question");
    }

    #[test]
    fn when_idle_is_offered_only_while_agents_work_and_not_already_waiting() {
        let busy = Impact { working: 1, shells: 0 };
        let shells = Impact { working: 0, shells: 2 };
        let available = UpdateStatus::Available { version: v("0.2.0") };
        let waiting = UpdateStatus::WaitingIdle { version: v("0.2.0") };
        assert!(busy.offers_idle(&available));
        assert!(!busy.offers_idle(&waiting));
        assert!(!shells.offers_idle(&available), "shells never get idle");
    }

    #[test]
    fn the_sentence_says_what_is_cut() {
        assert_eq!(
            Impact { working: 1, shells: 0 }.sentence(),
            "Mkrew restarts to install the update. 1 working agent stops mid-turn and resumes its conversation after the restart."
        );
        assert_eq!(
            Impact { working: 2, shells: 3 }.sentence(),
            "Mkrew restarts to install the update. 2 working agents stop mid-turn and resume their conversation after the restart. 3 shell or run panes close and do not come back."
        );
        assert_eq!(
            Impact { working: 0, shells: 1 }.sentence(),
            "Mkrew restarts to install the update. 1 shell or run pane closes and does not come back."
        );
    }
}
```

If the `Session` literal does not compile (a field was added since this plan), copy the field list from `crates/mkrew-daemon/src/awake.rs` `Setup::add`.

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p mkrew-ui --lib -j 4 update::`
Expected: compile errors (`cannot find function button`).

- [ ] **Step 3: Write minimal implementation**

Above the tests in `crates/mkrew-ui/src/update.rs`:

```rust
//! In-app update view-model (SPEC §8 "Update", §10 title bar): the title-bar button for the
//! daemon's [`UpdateStatus`] and what the restart would cut.

use mkrew_protocol::{Session, SessionKind, SessionState, UpdateStatus};

/// The title-bar button.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Button {
    pub label: String,
    pub tooltip: String,
    /// A click opens the confirmation (or installs at once).
    pub enabled: bool,
}

/// `None` = no button (up to date, or updates are off).
pub fn button(status: &UpdateStatus) -> Option<Button> {
    let (label, tooltip, enabled) = match status {
        UpdateStatus::UpToDate => return None,
        UpdateStatus::Available { version } => (
            format!("Update to v{version}"),
            "Install the new version; agents resume after the restart".to_owned(),
            true,
        ),
        UpdateStatus::Downloading { version } => (
            format!("Downloading v{version}…"),
            "Installs once the download is checked".to_owned(),
            false,
        ),
        UpdateStatus::WaitingIdle { version } => (
            format!("v{version} waits for idle agents"),
            "Installs when no agent is working; click to install now".to_owned(),
            true,
        ),
        UpdateStatus::Installing { version } => (
            format!("Installing v{version}…"),
            "Mkrew restarts in a moment".to_owned(),
            false,
        ),
        UpdateStatus::Failed { version, message } => (
            format!("Update to v{version} failed: retry"),
            message.clone(),
            true,
        ),
    };
    Some(Button {
        label,
        tooltip,
        enabled,
    })
}

/// What a daemon restart cuts, over the live (not exited) sessions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Impact {
    /// Agents mid-turn: cut, then resumed.
    pub working: usize,
    /// Shell and `run:` panes: gone for good.
    pub shells: usize,
}

impl Impact {
    pub fn of(sessions: &[Session]) -> Self {
        let mut impact = Self::default();
        for session in sessions.iter().filter(|s| s.state != SessionState::Exited) {
            match session.kind {
                SessionKind::Agent => {
                    impact.working += usize::from(session.state == SessionState::Working);
                }
                SessionKind::Shell => impact.shells += 1,
            }
        }
        impact
    }

    /// Nothing is cut: install without asking.
    pub fn is_none(&self) -> bool {
        self.working == 0 && self.shells == 0
    }

    /// "Update when idle" makes sense: agents work and the daemon is not already waiting.
    pub fn offers_idle(&self, status: &UpdateStatus) -> bool {
        self.working > 0 && !matches!(status, UpdateStatus::WaitingIdle { .. })
    }

    /// The dialog's body.
    pub fn sentence(&self) -> String {
        let mut text = "Mkrew restarts to install the update.".to_owned();
        match self.working {
            0 => {}
            1 => text.push_str(
                " 1 working agent stops mid-turn and resumes its conversation after the restart.",
            ),
            n => text.push_str(&format!(
                " {n} working agents stop mid-turn and resume their conversation after the restart."
            )),
        }
        match self.shells {
            0 => {}
            1 => text.push_str(" 1 shell or run pane closes and does not come back."),
            n => text.push_str(&format!(
                " {n} shell or run panes close and do not come back."
            )),
        }
        text
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p mkrew-ui --lib -j 4 update::`
Expected: 4 tests PASS.

- [ ] **Step 5: Commit**

```bash
cargo fmt --all
cargo clippy -p mkrew-ui --all-targets -j 4 -- -D warnings
git add crates/mkrew-ui/src/update.rs crates/mkrew-ui/src/lib.rs
git commit -m "feat(ui): update button and restart impact view-model"
```

---

### Task 5: UI shell — title-bar button, confirm dialog, quit on install

**Files:**
- Modify: `crates/mkrew-ui/src/main.rs` (`Shell` fields + `Shell::new` init, connect block, disconnect block, `dispatch`, `render_title_bar`, `modal_open`, `render`, new methods, `disabled_button` signature)
- Modify: `docs/SPEC.md` §10 (`**Title bar**` bullet)
- Modify: `docs/HUMAN_CHECKS.md` (new "To verify" block at the top of "To verify (non-blocking)")

**Interfaces:**
- Consumes: Task 1 `UpdateStatus`, `UpdateWhen`, `UpdateApplyParams`, `Request::UpdateStatus`, `Request::UpdateApply`, `Event::UpdateChanged`; Task 4 `mkrew_ui::update::{button, Impact}`.
- Produces: `Shell.update_status: UpdateStatus`, `Shell.update_dialog: Option<update::Impact>`, methods `load_update`, `set_update`, `open_update`, `close_update`, `update_key_down`, `apply_update`, `render_update_button`, `render_update_dialog`.

No headless test can drive GPUI views here (none exist in the repo). Task 4 tests the logic; this task is glue, checked by compile, clippy, the full suite and HUMAN_CHECKS.

- [ ] **Step 1: State and wiring**

Add imports in `main.rs`: `mkrew_protocol::{UpdateApplyParams, UpdateStatus, UpdateWhen}` (merge into the existing `mkrew_protocol` use) and `mkrew_ui::update` (merge into the existing `mkrew_ui::{…}` use).

`struct Shell`, next to `git_discard`:

```rust
    /// The daemon's in-app update state (SPEC §8 "Update"); drives the title-bar button.
    update_status: UpdateStatus,
    /// The "Install the update now?" dialog, with what the restart would cut.
    update_dialog: Option<update::Impact>,
```

`Shell::new`'s `Self { … }`: `update_status: UpdateStatus::UpToDate, update_dialog: None,`.

Connect block (`let loaded = this.update_in(cx, |shell, window, cx| { shell.connection = Some(connection); shell.refresh(window, cx); });`): add `shell.load_update(window, cx);` after `shell.refresh(window, cx);`.

Disconnect block (after `while let Ok(first) = incoming.recv().await`), next to `shell.git_discard = None;`: `shell.update_dialog = None; shell.update_status = UpdateStatus::UpToDate;`.

`dispatch`, after the `if let Event::AiGenerated(done) = event { … }` block:

```rust
                if let Event::UpdateChanged(status) = event {
                    self.set_update(status.clone(), cx);
                }
```

`modal_open`: append `|| self.update_dialog.is_some()`.

- [ ] **Step 2: Methods**

Add these to `impl Shell`, next to `open_git_discard`/`close_git_discard`/`confirm_git_discard`:

```rust
    /// `update.status` once connected; later changes arrive as `update.changed`.
    fn load_update(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        let Some(connection) = self.connection.clone() else {
            return;
        };
        let loaded = connection.request(Request::UpdateStatus);
        cx.spawn_in(window, async move |this, cx| {
            let status = match loaded.await {
                Ok(outcome) => outcome.into_result::<UpdateStatus>().map_err(|e| e.to_string()),
                Err(error) => Err(format!("{error:#}")),
            };
            match status {
                Ok(status) => {
                    let _ = this.update(cx, |shell, cx| shell.set_update(status, cx));
                }
                Err(error) => tracing::warn!("update.status failed: {error}"),
            }
        })
        .detach();
    }

    /// Shows `status`; on `installing` the app quits so the setup can replace `mkrew.exe`
    /// (the daemon is stopping; the setup starts the new UI).
    fn set_update(&mut self, status: UpdateStatus, cx: &mut Context<'_, Self>) {
        if matches!(status, UpdateStatus::Installing { .. }) {
            tracing::info!("update installing: quitting");
            cx.quit();
            return;
        }
        self.update_status = status;
        cx.notify();
    }

    /// The title-bar button: installs at once when the restart cuts nothing, else asks.
    fn open_update(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        let impact = update::Impact::of(self.sidebar.sessions());
        if impact.is_none() {
            self.apply_update(UpdateWhen::Now, window, cx);
            return;
        }
        self.update_dialog = Some(impact);
        self.dialog_serial += 1;
        window.focus(&self.dialog_focus, cx);
        cx.notify();
    }

    fn close_update(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        self.update_dialog = None;
        window.focus(&self.focus_handle, cx);
        cx.notify();
    }

    fn update_key_down(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        cx.stop_propagation();
        if undo_dialog::cancels(&event.keystroke) {
            self.close_update(window, cx);
        }
    }

    /// `update.apply`; progress and failures come back as `update.changed`.
    fn apply_update(&mut self, when: UpdateWhen, window: &mut Window, cx: &mut Context<'_, Self>) {
        self.close_update(window, cx);
        let Some(connection) = self.connection.clone() else {
            return;
        };
        let applied = connection.request(Request::UpdateApply(UpdateApplyParams { when }));
        cx.spawn_in(window, async move |_, _| match applied.await {
            Ok(Outcome::Result(_)) => {}
            Ok(Outcome::Error(body)) => tracing::warn!("update.apply refused: {}", body.message),
            Err(error) => tracing::warn!("update.apply failed: {error:#}"),
        })
        .detach();
    }
```

If `outcome.into_result` returns a non-`Display` error type, use `.map_err(|e| format!("{e:?}"))`. If `this.update(cx, …)` does not compile in the `spawn_in` closure, use `this.update_in(cx, |shell, _, cx| shell.set_update(status, cx))` like `confirm_git_discard` does.

- [ ] **Step 3: Title-bar button**

Widen `disabled_button`'s `label: &'static str` to `label: impl Into<SharedString>` and change `.child(label)` to `.child(label.into())`. Existing callers keep compiling.

Add:

```rust
    /// The title bar's update button (SPEC §10); `None` when up to date or updates are off.
    fn render_update_button(
        &self,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> Option<gpui::AnyElement> {
        let shown = update::button(&self.update_status)?;
        let tip = tooltip(theme, &shown.tooltip, "");
        Some(if shown.enabled {
            button("update", shown.label, theme, true)
                .flex_none()
                .tooltip(tip)
                .on_click(cx.listener(|shell, _, window, cx| shell.open_update(window, cx)))
                .into_any_element()
        } else {
            disabled_button("update", shown.label, theme, true)
                .flex_none()
                .tooltip(tip)
                .into_any_element()
        })
    }
```

In `render_title_bar`, the `right` div is built as `div().flex_none()…​.child(icon_button("toggle-theme", …))`. Insert `.children(self.render_update_button(theme, cx))` **before** `.child(icon_button("toggle-theme", …))`. It must sit outside the `if let Some(project_id) = workspace` block, so it shows without a workspace. If `tooltip(…)` is not `Clone`-free to move (it is used once here), keep as written. If `render_title_bar`'s `cx` is borrowed by an outer closure at that point, compute `let update_button = self.render_update_button(theme, cx);` before building `right` and pass `.children(update_button)`.

- [ ] **Step 4: Dialog**

Add `render_update_dialog`. Copy the panel styling of `render_git_discard` **verbatim** (width, padding, border, radius, shadow, font family, `track_focus(&self.dialog_focus)`, title/body text styles, button row, `Self::modal_shade`), changing only ids, texts, key handler and buttons:

```rust
    fn render_update_dialog(
        &self,
        impact: update::Impact,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> gpui::AnyElement {
        let version = self.update_status.version().unwrap_or_default().to_owned();
        let panel = div()
            .id("update-dialog")
            .w(px(theme::UNDO_DIALOG_WIDTH))
            .flex()
            .flex_col()
            .gap(px(space::M))
            .p(px(space::XL))
            .rounded(px(radius::LARGE))
            .border(px(theme::BORDER_WIDTH))
            .border_color(rgb(theme.border))
            .bg(rgb(theme.surface))
            .shadow_md()
            .font_family(theme.ui_font_family.clone())
            .track_focus(&self.dialog_focus)
            .on_key_down(cx.listener(Self::update_key_down))
            .child(
                div()
                    .text_size(font_size::CARD_TITLE)
                    .font_weight(FontWeight(weight::SEMIBOLD))
                    .text_color(rgb(theme.text))
                    .child(format!("Install Mkrew v{version} now?")),
            )
            .child(
                div()
                    .text_size(font_size::BODY)
                    .text_color(rgb(theme.text))
                    .child(impact.sentence()),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_end()
                    .gap(px(space::S))
                    .pt(px(space::S))
                    .child(
                        button("update-cancel", "Cancel", theme, false)
                            .tooltip(tooltip(theme, "Keep working; the button stays", "Esc"))
                            .on_click(cx.listener(|shell, _, window, cx| {
                                shell.close_update(window, cx);
                            })),
                    )
                    .when(impact.offers_idle(&self.update_status), |row| {
                        row.child(
                            button("update-idle", "Update when idle", theme, true)
                                .tooltip(tooltip(theme, "Install once no agent is working", ""))
                                .on_click(cx.listener(|shell, _, window, cx| {
                                    shell.apply_update(UpdateWhen::Idle, window, cx);
                                })),
                        )
                    })
                    .child(danger_button("update-now", "Update now", theme).on_click(
                        cx.listener(|shell, _, window, cx| {
                            shell.apply_update(UpdateWhen::Now, window, cx);
                        }),
                    )),
            );
        Self::modal_shade("update-shade", panel, cx)
    }
```

If `render_git_discard` uses different style calls than this sketch (e.g. scaled font sizes from the theme), follow `render_git_discard`; the theme is the source of truth (SPEC §14).

In `render`, next to `.children(self.git_discard.as_ref().map(|dialog| self.render_git_discard(dialog, &theme, cx)))`:

```rust
            .children(
                self.update_dialog
                    .map(|impact| self.render_update_dialog(impact, &theme, cx)),
            )
```

- [ ] **Step 5: Build, lint, full suite**

Run: `cargo clippy -p mkrew-ui --all-targets -j 4 -- -D warnings && cargo test -p mkrew-ui --lib -j 4`
Expected: clean; all `mkrew-ui` lib tests PASS.

- [ ] **Step 6: Docs**

`docs/SPEC.md` §10, in the `**Title bar**` bullet: before the sentence that describes the theme toggle (the title bar's right side), add:

```markdown
  When the daemon reports an update (§8 "Update"), an accent button **Update to vX.Y.Z** shows
  left of the theme toggle (also without a workspace): `Downloading…` and `Installing…` are faded
  and cannot be clicked; `vX waits for idle agents` and `Update to vX failed: retry` (tooltip =
  the error) can be. A click installs at once when no agent is working and no shell or run pane
  is live. Otherwise a dialog says what the restart cuts and offers **Cancel**, **Update when
  idle** (only while agents work and not already waiting) and **Update now**. On `installing` the
  app quits; the setup starts it again.
```

`docs/HUMAN_CHECKS.md`, insert at the top of "## To verify (non-blocking)" (after its italic intro line):

```markdown
### To verify — 2026-10-10 in-app update (ADR NNNN)

Setup once: set a fine-grained token that can read `tungmk97/mkrew` contents:
`setx MKREW_GITHUB_TOKEN github_pat_…`. Install the current build with the setup
(skill `mkrew-build-setup`; per user). Then bump the workspace version (e.g. 0.1.1), build that
setup, and publish a **non-draft** release:
`gh release create v0.1.1 dist\mkrew-0.1.1-x64-setup.exe --repo tungmk97/mkrew --title "Mkrew v0.1.1"`.

- [ ] **Notice:** open the installed Mkrew (restart it after `setx`). Within seconds the title bar
  shows `Update to v0.1.1` (dark and light). Without the token (or from `cargo run`) no button
  shows.
- [ ] **Now, nothing running:** with only idle agents, click → no dialog, the window closes, and
  Mkrew comes back on 0.1.1 by itself (`mk daemon status` shows the version). Tabs, panes and pins
  are as before, and each agent pane resumes its conversation.
- [ ] **Dialog:** start an agent on a long prompt and a shell pane, then click → the dialog names
  1 working agent and 1 shell pane. Esc / Cancel close it. **Update when idle** → the button
  reads `v0.1.1 waits for idle agents`, and the install starts by itself when the agent finishes.
  **Update now** → the turn is cut, and the agent resumes after the restart.
- [ ] **No reconnect message:** during the update the old window never shows "The daemon
  connection closed. Restart Mkrew to reconnect."
- [ ] **All users:** for an "Install for all users" setup, the update shows a UAC prompt and
  updates `C:\Program Files\Mkrew` (no second copy under `%LOCALAPPDATA%\Programs\Mkrew`).
- [ ] **Offline:** disconnect the network and start Mkrew → no button, no error;
  `%LOCALAPPDATA%\mkrew\logs` has "update check failed".
```

- [ ] **Step 7: Commit**

```bash
cargo fmt --all
git add crates/mkrew-ui/src/main.rs docs/SPEC.md docs/HUMAN_CHECKS.md
git commit -m "feat(ui): title-bar update button, confirm dialog, quit on install"
```

---

### Final: verify the branch

- [ ] Run `sh scripts/verify.sh` in the worktree (fmt check, clippy `-D warnings`, full tests). Fix anything red in the task that owns it, with a new commit.
- [ ] Smoke (dev build, proves the off path end to end) **without touching the user's running daemon**. The new protocol version would not talk to it anyway. In PowerShell, from the worktree: `$s = Join-Path $env:TEMP "mkrew-smoke"; $env:MKREW_DATA_DIR = "$s\data"; $env:MKREW_CONFIG_DIR = "$s\config"; $env:MKREW_SOCKET = '\\.\pipe\mkrew-smoke'; $env:MKREW_GITHUB_TOKEN = 'x'; $env:MKREW_LOG = 'debug'; cargo run -p mkrew-ui --bin mkrew -j 4`. The UI spawns the worktree's daemon on that pipe. Check that no update button shows, and that `$s\data` logs contain "in-app update off". Close the window, stop that daemon (`target\debug\mk.exe daemon stop` with the same env), and delete `$s`.
- [ ] Final review: dispatch `final-reviewer` on the whole branch diff against `master`, with this plan and its Review Focus.
- [ ] superpowers:finishing-a-development-branch (merge needs the user's OK).
