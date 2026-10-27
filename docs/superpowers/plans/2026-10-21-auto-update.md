# In-app update Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A tag push publishes a Windows per-user setup and a macOS app; a running Mkrew offers the
new version in the title bar, installs it without admin, reopens, and the agents resume.

**Architecture:** Pure + thin-I/O module `crates/mkrew-ui/src/update.rs` (lib) holds release
parsing, version compare, asset pick, install detection, download and the per-OS apply step. The
GPUI side `crates/mkrew-ui/src/shell/update.rs` runs the check loop, draws the title bar button,
asks for confirmation and calls the lib. Agent survival is the existing daemon resume (SPEC §8).

**Tech Stack:** Rust, gpui, `ureq` 3 (rustls, already a dependency), `serde_json`, Inno Setup 6,
macOS `codesign`/`ditto`, GitHub Actions.

**Spec:** `docs/superpowers/specs/2026-10-21-auto-update-design.md`

## Global Constraints

- `RELEASES_API` = `https://api.github.com/repos/tungmk97/mkrew/releases/latest`.
- Asset names: Windows `mkrew-<v>-x64-setup.exe`, macOS `mkrew-<v>-macos-arm64.zip`.
- Check at window open, then every 6 h; HTTP timeout 10 s for the API.
- Installer args: `/VERYSILENT /SUPPRESSMSGBOXES /NORESTART /LAUNCH=1` + `/CURRENTUSER` or `/ALLUSERS`.
- Numbers: ADR **0071**, SPEC **§20K**. No `PROTOCOL_VERSION` bump, no migration.
- No new crate dependency. No test calls the network or runs an installer (AGENTS.md).
- UI: theme tokens only (`theme.rs`), skill `mkrew-ui-design`.
- No `unwrap`/`expect` outside tests. `anyhow` with context in the I/O functions.
- Git: stage explicit paths only; commit with `git -c user.email=tungnv975@gmail.com commit`.
  **Never commit** `.github/workflows/*`, `packaging/*`, `docs/superpowers/specs/*`, `Cargo.lock`:
  edit them, leave them unstaged, list them in the final report for the user.
- Verify: `"C:/Program Files/Git/bin/bash.exe" scripts/verify.sh` (plain `bash` is WSL).

## Review Focus

1. A release asset name with path parts (`../x-x64-setup.exe`) must not write outside the
   download folder → `safe_file_name` test in Task 2.
2. A pre-release or odd tag (`v0.2.0-rc1`, `latest`, `0.1`) must never be offered → `is_newer`
   tests in Task 1.
3. A download cut short must not be run → size check in `download`, error path in Task 2 (the
   check is one comparison; covered by review, no network test).
4. Unsaved Files edits when the user clicks Update must be asked about before the app quits →
   `ask_unsaved` guard in Task 4 (manual check in HUMAN_CHECKS).
5. macOS swap failure must leave the old app in place → `swap_bundle` rollback test in Task 2.

---

### Task 1: Pure update logic

**Files:**
- Create: `crates/mkrew-ui/src/update.rs`
- Modify: `crates/mkrew-ui/src/lib.rs` (add `pub mod update;` between `pub mod undo_dialog;` and `pub mod usage;`)

**Interfaces:**
- Produces:
  - `pub const RELEASES_API: &str`, `pub const CHECK_EVERY: Duration`
  - `pub struct Release { pub version: String, pub page_url: String, pub assets: Vec<Asset> }`
  - `pub struct Asset { pub name: String, pub url: String, pub size: u64 }`
  - `pub fn parse_release(json: &str) -> anyhow::Result<Release>`
  - `pub fn is_newer(latest: &str, current: &str) -> bool`
  - `pub fn pick_asset<'a>(os: &str, arch: &str, assets: &'a [Asset]) -> Option<&'a Asset>`
  - `pub enum InstallKind { Inno { per_user: bool }, MacApp { bundle: PathBuf }, Manual }`
  - `pub fn install_kind(os: &str, exe: &Path, local_app_data: Option<&Path>) -> InstallKind`
  - `pub fn current_install() -> InstallKind`
  - `pub fn installer_args(per_user: bool) -> [&'static str; 5]`
  - `pub fn prompt_detail(working: usize) -> String`

- [ ] **Step 1: Write the failing tests** — at the end of `update.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    const JSON: &str = r#"{
        "tag_name": "v0.1.7",
        "html_url": "https://github.com/tungmk97/mkrew/releases/tag/v0.1.7",
        "assets": [
            {"name": "mkrew-0.1.7-x64-setup.exe", "browser_download_url": "https://x/a.exe", "size": 10},
            {"name": "mkrew-0.1.7-macos-arm64.zip", "browser_download_url": "https://x/b.zip", "size": 20},
            {"name": "mkrew-0.1.7-linux-x86_64.tar.gz", "browser_download_url": "https://x/c", "size": 30}
        ]
    }"#;

    #[test]
    fn parses_release_and_strips_v() {
        let release = parse_release(JSON).unwrap();
        assert_eq!(release.version, "0.1.7");
        assert_eq!(release.page_url, "https://github.com/tungmk97/mkrew/releases/tag/v0.1.7");
        assert_eq!(release.assets.len(), 3);
        assert_eq!(release.assets[1].url, "https://x/b.zip");
        assert_eq!(release.assets[1].size, 20);
        let bare = parse_release(r#"{"tag_name":"0.2.0","html_url":"u"}"#).unwrap();
        assert_eq!(bare.version, "0.2.0");
        assert!(bare.assets.is_empty());
        assert!(parse_release("not json").is_err());
    }

    #[test]
    fn newer_compares_numbers_and_refuses_odd_tags() {
        assert!(is_newer("0.1.10", "0.1.9"));
        assert!(is_newer("1.0.0", "0.9.9"));
        assert!(!is_newer("0.1.5", "0.1.5"));
        assert!(!is_newer("0.1.4", "0.1.5"));
        assert!(!is_newer("0.2.0-rc1", "0.1.5"));
        assert!(!is_newer("0.2", "0.1.5"));
        assert!(!is_newer("0.2.0.1", "0.1.5"));
        assert!(!is_newer("latest", "0.1.5"));
        assert!(!is_newer("0.2.0", "dev"));
    }

    #[test]
    fn picks_the_asset_of_this_platform() {
        let assets = parse_release(JSON).unwrap().assets;
        assert_eq!(pick_asset("windows", "x86_64", &assets).unwrap().name, "mkrew-0.1.7-x64-setup.exe");
        assert_eq!(pick_asset("macos", "aarch64", &assets).unwrap().name, "mkrew-0.1.7-macos-arm64.zip");
        assert!(pick_asset("macos", "x86_64", &assets).is_none());
        assert!(pick_asset("linux", "x86_64", &assets).is_none());
        assert!(pick_asset("windows", "x86_64", &[]).is_none());
    }

    #[test]
    fn inno_install_is_detected_by_its_uninstaller() {
        let tmp = tempfile::tempdir().unwrap();
        let local = tmp.path().join("Local");
        let app = local.join("Programs").join("Mkrew");
        std::fs::create_dir_all(app.join("bin")).unwrap();
        let exe = app.join("bin").join("mkrew.exe");
        // No uninstaller yet: a dev build or an MSI install.
        assert_eq!(install_kind("windows", &exe, Some(&local)), InstallKind::Manual);
        std::fs::write(app.join("unins000.exe"), b"").unwrap();
        assert_eq!(install_kind("windows", &exe, Some(&local)), InstallKind::Inno { per_user: true });
        let other = tmp.path().join("Elsewhere");
        assert_eq!(install_kind("windows", &exe, Some(&other)), InstallKind::Inno { per_user: false });
        assert_eq!(install_kind("windows", &exe, None), InstallKind::Inno { per_user: false });
    }

    #[test]
    fn mac_app_is_detected_by_the_bundle_path() {
        let tmp = tempfile::tempdir().unwrap();
        let bundle = tmp.path().join("Mkrew.app");
        let exe = bundle.join("Contents").join("MacOS").join("mkrew");
        assert_eq!(install_kind("macos", &exe, None), InstallKind::MacApp { bundle });
        let dev = tmp.path().join("target").join("release").join("mkrew");
        assert_eq!(install_kind("macos", &dev, None), InstallKind::Manual);
        assert_eq!(install_kind("linux", &exe, None), InstallKind::Manual);
    }

    #[test]
    fn prompt_names_working_agents() {
        assert!(!prompt_detail(0).contains("working"));
        assert!(prompt_detail(1).contains("1 agent is working; its current turn stops."));
        assert!(prompt_detail(3).contains("3 agents are working; their current turn stops."));
        assert!(prompt_detail(0).contains("Agent panes resume their conversation."));
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p mkrew-ui --lib update::`
Expected: FAIL to compile (`parse_release` etc. not found).

- [ ] **Step 3: Write the implementation** — top of `update.rs`:

```rust
//! In-app update from GitHub Releases (SPEC §20K, ADR 0071): what is new, which file fits this
//! install, and how to put it in place. Pure parts first; the I/O below is thin.

use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Context, Result, bail};
use serde::Deserialize;

/// The newest published (non-draft, non-pre-release) release of the public repo.
pub const RELEASES_API: &str = "https://api.github.com/repos/tungmk97/mkrew/releases/latest";
/// How often a running UI asks again (one request; GitHub allows 60/h unauthenticated).
pub const CHECK_EVERY: Duration = Duration::from_secs(6 * 60 * 60);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Release {
    /// `tag_name` without a leading `v`.
    pub version: String,
    pub page_url: String,
    pub assets: Vec<Asset>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Asset {
    pub name: String,
    #[serde(rename = "browser_download_url")]
    pub url: String,
    pub size: u64,
}

#[derive(Deserialize)]
struct RawRelease {
    tag_name: String,
    html_url: String,
    #[serde(default)]
    assets: Vec<Asset>,
}

pub fn parse_release(json: &str) -> Result<Release> {
    let raw: RawRelease = serde_json::from_str(json).context("unexpected GitHub release answer")?;
    Ok(Release {
        version: raw.tag_name.trim_start_matches('v').to_owned(),
        page_url: raw.html_url,
        assets: raw.assets,
    })
}

/// Exactly `a.b.c`, all numbers; anything else (pre-release, build suffix, two parts) is `None`.
fn numeric(version: &str) -> Option<[u64; 3]> {
    let mut parts = version.split('.');
    let mut out = [0; 3];
    for slot in &mut out {
        *slot = parts.next()?.parse().ok()?;
    }
    parts.next().is_none().then_some(out)
}

/// `latest` is offered only when both are plain `a.b.c` and it is strictly greater.
pub fn is_newer(latest: &str, current: &str) -> bool {
    matches!((numeric(latest), numeric(current)), (Some(l), Some(c)) if l > c)
}

/// The asset this OS/arch installs in place; none = only the release page helps.
pub fn pick_asset<'a>(os: &str, arch: &str, assets: &'a [Asset]) -> Option<&'a Asset> {
    let suffix = match (os, arch) {
        ("windows", "x86_64") => "-x64-setup.exe",
        ("macos", "aarch64") => "-macos-arm64.zip",
        _ => return None,
    };
    assets.iter().find(|asset| asset.name.ends_with(suffix))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InstallKind {
    /// Installed by the Inno setup (`<app>\unins000.exe` next to `bin\`).
    Inno { per_user: bool },
    /// Running from `<bundle>/Contents/MacOS/mkrew`.
    MacApp { bundle: PathBuf },
    /// Dev build, old MSI install, Linux: the update button opens the release page.
    Manual,
}

pub fn install_kind(os: &str, exe: &Path, local_app_data: Option<&Path>) -> InstallKind {
    match os {
        "windows" => {
            let Some(app) = exe.parent().and_then(Path::parent) else {
                return InstallKind::Manual;
            };
            if !app.join("unins000.exe").is_file() {
                return InstallKind::Manual;
            }
            // Case-insensitive: Windows paths differ in drive-letter case between APIs.
            let lower = |path: &Path| path.to_string_lossy().to_lowercase();
            let per_user = local_app_data.is_some_and(|dir| lower(exe).starts_with(&lower(dir)));
            InstallKind::Inno { per_user }
        }
        "macos" => {
            let macos = exe.parent();
            let contents = macos.and_then(Path::parent);
            let bundle = contents.and_then(Path::parent);
            match (macos, contents, bundle) {
                (Some(m), Some(c), Some(b))
                    if m.file_name() == Some("MacOS".as_ref())
                        && c.file_name() == Some("Contents".as_ref())
                        && b.extension() == Some("app".as_ref()) =>
                {
                    InstallKind::MacApp { bundle: b.to_path_buf() }
                }
                _ => InstallKind::Manual,
            }
        }
        _ => InstallKind::Manual,
    }
}

/// [`install_kind`] of the running executable.
pub fn current_install() -> InstallKind {
    let Ok(exe) = std::env::current_exe() else {
        return InstallKind::Manual;
    };
    let local = std::env::var_os("LOCALAPPDATA").map(PathBuf::from);
    install_kind(std::env::consts::OS, &exe, local.as_deref())
}

/// Inno setup switches: silent, reopen Mkrew after (`[Run]` `LaunchAfterUpdate`), same scope.
pub fn installer_args(per_user: bool) -> [&'static str; 5] {
    let scope = if per_user { "/CURRENTUSER" } else { "/ALLUSERS" };
    ["/VERYSILENT", "/SUPPRESSMSGBOXES", "/NORESTART", "/LAUNCH=1", scope]
}

/// The confirmation's detail line; `working` = agent sessions in `working`.
pub fn prompt_detail(working: usize) -> String {
    let mut text = String::from("Mkrew closes, installs and reopens. Agent panes resume their conversation.");
    match working {
        0 => {}
        1 => text.push_str(" 1 agent is working; its current turn stops."),
        n => text.push_str(&format!(" {n} agents are working; their current turn stops.")),
    }
    text.push_str(" Shells and run commands without autostart do not come back.");
    text
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p mkrew-ui --lib update::`
Expected: PASS (6 tests).

- [ ] **Step 5: Commit**

```bash
git add crates/mkrew-ui/src/update.rs crates/mkrew-ui/src/lib.rs
git -c user.email=tungnv975@gmail.com commit -m "feat(ui): parse GitHub releases and pick the update for this install"
```

---

### Task 2: Update I/O — fetch, download, apply

**Files:**
- Modify: `crates/mkrew-ui/src/update.rs` (append after `prompt_detail`, before `mod tests`)

**Interfaces:**
- Consumes: Task 1 types.
- Produces:
  - `pub fn fetch_latest() -> Result<Option<Release>>` (`None` = 404, nothing published)
  - `pub fn download(asset: &Asset, dir: &Path) -> Result<PathBuf>`
  - `pub fn apply(kind: &InstallKind, file: &Path) -> Result<()>` (after `Ok`, the caller quits)
  - `pub fn clean_leftovers(kind: &InstallKind)`
  - `fn safe_file_name(name: &str) -> Result<&str>`, `fn old_bundle(bundle: &Path) -> PathBuf`,
    `fn swap_bundle(bundle: &Path, new_app: &Path) -> Result<()>`

- [ ] **Step 1: Write the failing tests** — append inside `mod tests`:

```rust
    #[test]
    fn asset_names_with_path_parts_are_refused() {
        assert_eq!(safe_file_name("mkrew-0.1.7-x64-setup.exe").unwrap(), "mkrew-0.1.7-x64-setup.exe");
        assert!(safe_file_name("../evil-x64-setup.exe").is_err());
        assert!(safe_file_name("a/b.zip").is_err());
        assert!(safe_file_name(r"a\b.zip").is_err());
        assert!(safe_file_name("").is_err());
        assert!(safe_file_name("..").is_err());
    }

    #[test]
    fn swap_puts_the_new_bundle_in_place_and_keeps_the_old() {
        let tmp = tempfile::tempdir().unwrap();
        let bundle = tmp.path().join("Mkrew.app");
        let new_app = tmp.path().join(".mkrew-update").join("Mkrew.app");
        std::fs::create_dir_all(&bundle).unwrap();
        std::fs::write(bundle.join("v"), "old").unwrap();
        std::fs::create_dir_all(&new_app).unwrap();
        std::fs::write(new_app.join("v"), "new").unwrap();
        swap_bundle(&bundle, &new_app).unwrap();
        assert_eq!(std::fs::read_to_string(bundle.join("v")).unwrap(), "new");
        assert_eq!(std::fs::read_to_string(old_bundle(&bundle).join("v")).unwrap(), "old");
    }

    #[test]
    fn failed_swap_restores_the_old_bundle() {
        let tmp = tempfile::tempdir().unwrap();
        let bundle = tmp.path().join("Mkrew.app");
        std::fs::create_dir_all(&bundle).unwrap();
        std::fs::write(bundle.join("v"), "old").unwrap();
        let missing = tmp.path().join("nope").join("Mkrew.app");
        assert!(swap_bundle(&bundle, &missing).is_err());
        assert_eq!(std::fs::read_to_string(bundle.join("v")).unwrap(), "old");
        assert!(!old_bundle(&bundle).exists());
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p mkrew-ui --lib update::`
Expected: FAIL to compile (`safe_file_name`, `swap_bundle`, `old_bundle` not found).

- [ ] **Step 3: Write the implementation.** Before writing the download, check the ureq 3.4
  docs (docs.rs `ureq::Body::as_reader` / `into_reader`) for a size cap: if `as_reader()` caps
  the body, use `body_mut().with_config().limit(asset.size + 1).reader()` instead. Do not guess.

```rust
const API_TIMEOUT: Duration = Duration::from_secs(10);
/// A setup is ~30 MB; slow links get time, a dead one still ends.
const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(15 * 60);
const USER_AGENT: &str = concat!("mkrew/", env!("CARGO_PKG_VERSION"));

fn agent(timeout: Duration) -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_global(Some(timeout))
        .http_status_as_error(false)
        .build()
        .into()
}

/// The newest release; `None` while nothing is published (GitHub answers 404).
pub fn fetch_latest() -> Result<Option<Release>> {
    let mut response = agent(API_TIMEOUT)
        .get(RELEASES_API)
        .header("User-Agent", USER_AGENT)
        .header("Accept", "application/vnd.github+json")
        .call()
        .context("cannot reach GitHub")?;
    let status = response.status().as_u16();
    if status == 404 {
        return Ok(None);
    }
    let body = response.body_mut().read_to_string().context("cannot read GitHub's answer")?;
    if status != 200 {
        bail!("GitHub answered {status}");
    }
    parse_release(&body).map(Some)
}

/// The asset's name as a plain file name (it comes from the network).
fn safe_file_name(name: &str) -> Result<&str> {
    let plain = !name.is_empty()
        && name != "."
        && name != ".."
        && !name.contains(['/', '\\', ':']);
    if plain { Ok(name) } else { bail!("refusing the asset name {name:?}") }
}

/// Downloads `asset` into `dir`; the byte count must match the release's `size`.
pub fn download(asset: &Asset, dir: &Path) -> Result<PathBuf> {
    let path = dir.join(safe_file_name(&asset.name)?);
    std::fs::create_dir_all(dir).with_context(|| format!("create {}", dir.display()))?;
    let mut response = agent(DOWNLOAD_TIMEOUT)
        .get(&asset.url)
        .header("User-Agent", USER_AGENT)
        .call()
        .with_context(|| format!("cannot download {}", asset.name))?;
    let status = response.status().as_u16();
    if status != 200 {
        bail!("download of {} answered {status}", asset.name);
    }
    let mut file = std::fs::File::create(&path).with_context(|| format!("create {}", path.display()))?;
    let written = std::io::copy(&mut response.body_mut().as_reader(), &mut file)
        .with_context(|| format!("download {}", asset.name))?;
    if written != asset.size {
        bail!("download of {} stopped at {written} of {} bytes", asset.name, asset.size);
    }
    Ok(path)
}

/// `Mkrew.app` → `Mkrew.app.old`, next to it.
fn old_bundle(bundle: &Path) -> PathBuf {
    let mut name = bundle.file_name().unwrap_or_default().to_os_string();
    name.push(".old");
    bundle.with_file_name(name)
}

/// Where a macOS update is unpacked: beside the bundle, so the swap is a same-volume rename.
fn unpack_dir(bundle: &Path) -> PathBuf {
    bundle.with_file_name(".mkrew-update")
}

/// Moves `bundle` aside to [`old_bundle`] and `new_app` into its place; on failure the old one
/// is put back.
fn swap_bundle(bundle: &Path, new_app: &Path) -> Result<()> {
    let old = old_bundle(bundle);
    let _ = std::fs::remove_dir_all(&old);
    std::fs::rename(bundle, &old).with_context(|| format!("move {} aside", bundle.display()))?;
    if let Err(error) = std::fs::rename(new_app, bundle) {
        let _ = std::fs::rename(&old, bundle);
        return Err(error).with_context(|| format!("put the new app at {}", bundle.display()));
    }
    Ok(())
}

fn run(command: &mut std::process::Command, what: &str) -> Result<()> {
    let status = command.status().with_context(|| format!("run {what}"))?;
    if !status.success() {
        bail!("{what} failed ({status})");
    }
    Ok(())
}

/// Puts the downloaded `file` in place. After `Ok` the caller quits the UI: on Windows the
/// detached setup stops the daemon, installs and reopens Mkrew; on macOS the new app is already
/// in place and opened.
pub fn apply(kind: &InstallKind, file: &Path) -> Result<()> {
    match kind {
        InstallKind::Inno { per_user } => {
            let mut command = std::process::Command::new(file);
            command.args(installer_args(*per_user));
            #[cfg(windows)]
            {
                use std::os::windows::process::CommandExt;
                // DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP: the setup outlives the UI.
                command.creation_flags(0x0000_0008 | 0x0000_0200);
            }
            command.spawn().context("start the installer")?;
            Ok(())
        }
        InstallKind::MacApp { bundle } => {
            let dir = unpack_dir(bundle);
            let _ = std::fs::remove_dir_all(&dir);
            run(
                std::process::Command::new("/usr/bin/ditto").arg("-x").arg("-k").arg(file).arg(&dir),
                "ditto",
            )?;
            let bin = bundle.join("Contents").join("MacOS");
            // Returns once the daemon released its lock; agents resume on the next start.
            run(
                std::process::Command::new(bin.join("mk"))
                    .args(["daemon", "stop", "--only-from"])
                    .arg(&bin),
                "mk daemon stop",
            )?;
            swap_bundle(bundle, &dir.join("Mkrew.app"))?;
            let _ = std::fs::remove_dir_all(&dir);
            std::process::Command::new("/usr/bin/open")
                .arg("-n")
                .arg(bundle)
                .spawn()
                .context("open the new Mkrew")?;
            Ok(())
        }
        InstallKind::Manual => bail!("this install cannot update itself"),
    }
}

/// Removes what a previous macOS update left beside the bundle.
pub fn clean_leftovers(kind: &InstallKind) {
    if let InstallKind::MacApp { bundle } = kind {
        let _ = std::fs::remove_dir_all(old_bundle(bundle));
        let _ = std::fs::remove_dir_all(unpack_dir(bundle));
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p mkrew-ui --lib update::` then `cargo clippy -p mkrew-ui --all-targets -- -D warnings`
Expected: PASS (9 tests), clippy clean.

- [ ] **Step 5: Commit**

```bash
git add crates/mkrew-ui/src/update.rs
git -c user.email=tungnv975@gmail.com commit -m "feat(ui): download and apply an update (Inno setup, macOS app swap)"
```

---

### Task 3: Packaging — relaunching setup, no MSI, macOS app bundle

**Files:**
- Modify: `packaging/mkrew.iss` (`[Run]`, `[Code]`) — **do not commit**
- Delete: `packaging/mkrew.wxs` — **do not commit** (user commits)
- Delete: `scripts/package.ps1`
- Modify: `scripts/package.sh` (`--app`)
- Modify: `crates/mkrew-cli/tests/packaging.rs`
- Modify: `README.md:27-74`

**Interfaces:**
- Produces: `dist/mkrew-<v>-x64-setup.exe` relaunching with `/LAUNCH=1`;
  `bash scripts/package.sh --app` → `dist/mkrew-<v>-macos-arm64.zip` (macOS only).

- [ ] **Step 1: Write the failing test** — in `packaging.rs`, delete `installer_lists_binaries_and_profiles`
  (the MSI goes away) and add:

```rust
/// A silent in-app update (`/LAUNCH=1`, ADR 0071) reopens Mkrew; a normal silent install does not.
#[test]
fn setup_exe_relaunches_after_an_in_app_update() {
    let iss = std::fs::read_to_string(repo_root().join("packaging/mkrew.iss")).unwrap();
    assert!(iss.contains(
        r#"Filename: "{app}\bin\mkrew.exe"; Flags: nowait; Check: LaunchAfterUpdate"#
    ));
    assert!(iss.contains("function LaunchAfterUpdate: Boolean;"));
    assert!(iss.contains("ExpandConstant('{param:LAUNCH|0}') = '1'"));
}
```

  Also change the file's doc comment line 1 to `//! Release packaging (SPEC §4): the setup .exe and the tar.gz ship \`mkrew\`, \`mk\`, the daemon`.

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p mkrew-cli --test packaging`
Expected: FAIL `setup_exe_relaunches_after_an_in_app_update`.

- [ ] **Step 3: Implement**

  `packaging/mkrew.iss` `[Run]` becomes:

```
[Run]
Filename: "{app}\bin\mkrew.exe"; Description: "{cm:LaunchProgram,Mkrew}"; Flags: nowait postinstall skipifsilent
; In-app update (ADR 0071): the UI runs setup with /VERYSILENT /LAUNCH=1 and quits; reopen it.
Filename: "{app}\bin\mkrew.exe"; Flags: nowait; Check: LaunchAfterUpdate
```

  In `[Code]`, before `function PrepareToInstall`:

```pascal
{ True for the silent setup started by Mkrew's own update (/LAUNCH=1). }
function LaunchAfterUpdate: Boolean;
begin
  Result := WizardSilent and (ExpandConstant('{param:LAUNCH|0}') = '1');
end;
```

  Delete `packaging/mkrew.wxs` and `scripts/package.ps1`.

  `scripts/package.sh`: header usage line → `# Usage: scripts/package.sh [--no-build] [--bin-dir DIR] [--out DIR] [--app]`
  and add after line 3: `# --app (macOS): dist/mkrew-<version>-macos-<arch>.zip holding an ad-hoc signed Mkrew.app.`
  Add `app=0` after `build=1`, the case `--app) app=1 ;;`, and after the `cargo build` block
  (before `stage="$(mktemp -d)"`… keep `stage`/`trap`, insert right after the `trap` line):

```bash
if [ "$app" = 1 ]; then
  [ "$(uname -s)" = Darwin ] || { echo "package: --app needs macOS" >&2; exit 1; }
  bundle="$stage/Mkrew.app"
  mkdir -p "$bundle/Contents/MacOS" "$bundle/Contents/Resources/agents"
  for bin in mkrew mk mkrew-daemon; do
    cp "$bin_dir/$bin" "$bundle/Contents/MacOS/$bin"
    chmod 755 "$bundle/Contents/MacOS/$bin"
  done
  cp "$root"/agents/*.toml "$bundle/Contents/Resources/agents/"
  cat > "$bundle/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleIdentifier</key><string>dev.mkrew.app</string>
  <key>CFBundleName</key><string>Mkrew</string>
  <key>CFBundleExecutable</key><string>mkrew</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>$version</string>
  <key>CFBundleVersion</key><string>$version</string>
  <key>LSMinimumSystemVersion</key><string>12.0</string>
  <key>NSHighResolutionCapable</key><true/>
</dict>
</plist>
PLIST
  # Ad-hoc signature (ADR 0071): no Developer ID; first open needs right-click → Open.
  codesign --force --deep -s - "$bundle"
  mkdir -p "$out"
  out="$(cd "$out" && pwd)"
  zip="$out/mkrew-$version-macos-$(uname -m).zip"
  rm -f "$zip"
  ditto -c -k --keepParent "$bundle" "$zip"
  echo "$zip"
  exit 0
fi
```

  README: delete the "### Windows (MSI)" section (lines 40-45). Replace the "### Linux / macOS
  (tar.gz)" heading by "### Linux (tar.gz)" and add before it:

```markdown
### macOS (app)

Unzip `mkrew-<version>-macos-arm64.zip` and drag `Mkrew.app` to `/Applications`. The app is not
signed with an Apple Developer ID: the first time, right-click it → Open → Open (or run
`xattr -dr com.apple.quarantine /Applications/Mkrew.app`). For `mk` in a terminal:
`ln -s /Applications/Mkrew.app/Contents/MacOS/mk /usr/local/bin/mk`.

### Updates

Mkrew checks GitHub for a new release when it opens and every 6 hours. A new version shows an
arrow button in the title bar: click it, confirm, and Mkrew installs it, reopens, and agent panes
resume their conversation (shells do not come back). Windows needs no admin for a "for me only"
install. An old MSI install cannot update itself: uninstall it and run the setup once.
```

  Replace lines 72-74 ("Release packages: …") with:

```markdown
All three land in `~/.cargo/bin`, which rustup already puts on your `PATH`. Release packages:
`bash scripts/package.sh` (tar.gz; `--app` on macOS for the app zip) or
`pwsh scripts/package-setup.ps1` (setup .exe, needs Inno Setup 6).

### Releasing

Bump `version` in the root `Cargo.toml`, commit, then `git tag vX.Y.Z` and push the commit and
the tag. The Release workflow builds the Windows setup, the macOS app and the Linux tar.gz and
publishes the release; running copies of Mkrew offer it within 6 hours.
```

  Grep the repo for other `package.ps1` / `mkrew.wxs` / `MSI` mentions
  (`docs/`, `scripts/`, `.claude/skills/`) and update each (skill `mkrew-build-setup`,
  `docs/BUILD_VI.md`, `docs/REDEPLOY_VI.md` included — those two docs are edited but not
  committed).

- [ ] **Step 4: Run to verify it passes**

Run: `cargo test -p mkrew-cli --test packaging`
Expected: PASS. Then build the setup locally: `pwsh scripts/package-setup.ps1 -NoBuild`
(needs `target/release` binaries; skip if absent and say so in the report).

- [ ] **Step 5: Commit** (only these paths; `packaging/*` and the two `_VI.md` docs stay unstaged)

```bash
git add crates/mkrew-cli/tests/packaging.rs scripts/package.sh README.md
git rm scripts/package.ps1
git -c user.email=tungnv975@gmail.com commit -m "feat(packaging): macOS app bundle, drop the MSI, relaunch after in-app update"
```

---

### Task 4: UI — check loop, title bar button, palette, confirm and apply

**Files:**
- Create: `icon/lucide/circle-arrow-up.svg`
- Modify: `crates/mkrew-ui/src/icons.rs` (variant `CircleArrowUp => "circle-arrow-up",`, alphabetical)
- Create: `crates/mkrew-ui/src/shell/update.rs`
- Modify: `crates/mkrew-ui/src/shell.rs` (`mod update;` in the mod list)
- Modify: `crates/mkrew-ui/src/main.rs:311` (`struct Shell` fields)
- Modify: `crates/mkrew-ui/src/shell/connection.rs` (`Shell::new`: field init + start loop, next to the phone poll at `:148-162`)
- Modify: `crates/mkrew-ui/src/shell/chrome.rs:174` (`.children(self.render_update_button(theme, cx))` before the phone button)
- Modify: `crates/mkrew-ui/src/palette.rs` (`Extra::CheckUpdates`, `ALL` count +1, label, availability)
- Modify: `crates/mkrew-ui/src/shell/navigation.rs:482-483` and `:553-554` (dispatch)
- Test: `crates/mkrew-ui/src/palette/tests.rs`

**Interfaces:**
- Consumes: `mkrew_ui::update::{Release, InstallKind, current_install, pick_asset, prompt_detail, fetch_latest, download, apply, clean_leftovers, is_newer, CHECK_EVERY}`.
- Produces: `Shell::check_updates(manual: bool, window, cx)`, `Shell::render_update_button(theme, cx)`, `Shell::start_update(window, cx)`; fields `update: Option<mkrew_ui::update::Release>`, `updating: bool`.

- [ ] **Step 1: Write the failing palette test** — in `palette/tests.rs`, next to the Phone access
  assertions (`:315-339`), add the same shape for the new entry:

```rust
#[test]
fn check_updates_is_a_global_palette_entry() {
    let entry = Extra::CheckUpdates;
    assert_eq!(entry.title(), "Global: Check for updates");
    assert!(Extra::ALL.contains(&entry));
}
```

  Match the existing test's accessors (if titles are read through a palette entry list rather
  than `Extra::title()`, use that same path; read `palette/tests.rs:315-339` first).

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p mkrew-ui --lib palette::`
Expected: FAIL to compile (`Extra::CheckUpdates` not found).

- [ ] **Step 3: Implement**

  `icon/lucide/circle-arrow-up.svg` (Lucide; match the attribute layout of `icon/lucide/arrow-up.svg`):

```svg
<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="10"/><path d="m16 12-4-4-4 4"/><path d="M12 16V8"/></svg>
```

  `palette.rs`: add `CheckUpdates` to `enum Extra` after `PhoneAccess`, to `ALL` (bump `[Self; 17]`
  to `18`), label `Self::CheckUpdates => "Global: Check for updates",`, availability next to
  `Self::OpenSettings | Self::PhoneAccess => true,` → `Self::OpenSettings | Self::PhoneAccess | Self::CheckUpdates => true,`.

  `navigation.rs` both matches:

```rust
            palette::Action::Extra(Extra::CheckUpdates) => self.check_updates(true, window, cx),
```
```rust
            Extra::CheckUpdates => self.check_updates(true, window, cx),
```

  `main.rs` `struct Shell`, after `connection`:

```rust
    /// A newer published release (SPEC §20K), shown as the title bar update button.
    update: Option<mkrew_ui::update::Release>,
    /// The update is downloading / being put in place: the button ignores clicks.
    updating: bool,
```

  `connection.rs` `Shell::new`: init `update: None, updating: false,` in the struct literal, and
  after the phone poll loop:

```rust
        // In-app update (SPEC §20K): ask GitHub now and every `CHECK_EVERY`; clear what a
        // previous macOS update left beside the app.
        cx.background_executor()
            .spawn(async { mkrew_ui::update::clean_leftovers(&mkrew_ui::update::current_install()) })
            .detach();
        cx.spawn_in(window, async move |this, cx| {
            loop {
                if this
                    .update_in(cx, |shell, window, cx| shell.check_updates(false, window, cx))
                    .is_err()
                {
                    return;
                }
                cx.background_executor().timer(mkrew_ui::update::CHECK_EVERY).await;
            }
        })
        .detach();
```

  `crates/mkrew-ui/src/shell/update.rs`:

```rust
//! In-app update (SPEC §20K, ADR 0071): the check, the title bar button and the apply step.

use gpui::{Context, InteractiveElement, PromptLevel, StatefulInteractiveElement, Styled, Window, prelude::FluentBuilder};
use mkrew_protocol::{SessionKind, SessionState};
use mkrew_ui::update::{self, InstallKind, Release};

use crate::icons::Icon;
use crate::shell::widgets::{icon_button_in, tooltip};
use crate::theme::{Theme, opacity};
use crate::{AfterSave, Shell};

const CURRENT: &str = env!("CARGO_PKG_VERSION");
/// `window.prompt` answers; Cancel is also Esc.
const ANSWERS: [&str; 2] = ["Update", "Cancel"];

impl Shell {
    /// Asks GitHub for the newest release. `manual` (palette) always answers with a toast.
    pub(crate) fn check_updates(&mut self, manual: bool, window: &mut Window, cx: &mut Context<'_, Self>) {
        cx.spawn_in(window, async move |this, cx| {
            let result = cx.background_executor().spawn(async { update::fetch_latest() }).await;
            let _ = this.update(cx, |shell, cx| shell.finish_check(manual, result, cx));
        })
        .detach();
    }

    fn finish_check(&mut self, manual: bool, result: anyhow::Result<Option<Release>>, cx: &mut Context<'_, Self>) {
        match result {
            Ok(Some(release)) if update::is_newer(&release.version, CURRENT) => {
                let fresh = self.update.as_ref().is_none_or(|known| known.version != release.version);
                if fresh || manual {
                    self.toast_info(format!("Mkrew {} is available", release.version), cx);
                }
                self.update = Some(release);
            }
            Ok(_) if manual => self.toast_info(format!("Mkrew {CURRENT} is up to date"), cx),
            Ok(_) => {}
            Err(error) => {
                tracing::warn!("update check: {error:#}");
                if manual {
                    self.toast_error(format!("Update check failed: {error:#}"), cx);
                }
            }
        }
        cx.notify();
    }

    pub(crate) fn render_update_button(&self, theme: &Theme, cx: &mut Context<'_, Self>) -> Option<gpui::Stateful<gpui::Div>> {
        let release = self.update.as_ref()?;
        let text = if self.updating {
            format!("Downloading Mkrew {}…", release.version)
        } else {
            format!("Update to Mkrew {} (you have {CURRENT})", release.version)
        };
        Some(
            icon_button_in("update", Icon::CircleArrowUp, theme.accent_text, theme)
                .tooltip(tooltip(theme, &text, ""))
                .when(self.updating, |el| el.opacity(opacity::DISABLED))
                .when(!self.updating, |el| {
                    el.on_click(cx.listener(|shell, _, window, cx| shell.start_update(window, cx)))
                }),
        )
    }

    pub(crate) fn start_update(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        let Some(release) = self.update.clone() else { return };
        let kind = update::current_install();
        let asset = update::pick_asset(std::env::consts::OS, std::env::consts::ARCH, &release.assets).cloned();
        let (InstallKind::Inno { .. } | InstallKind::MacApp { .. }, Some(asset)) = (&kind, asset) else {
            cx.open_url(&release.page_url);
            return;
        };
        let again: AfterSave = Box::new(|shell, window, cx| shell.start_update(window, cx));
        if self.ask_unsaved(again, window, cx) {
            return;
        }
        let working = self
            .sidebar
            .sessions()
            .iter()
            .filter(|s| s.kind == SessionKind::Agent && s.state == SessionState::Working)
            .count();
        let detail = update::prompt_detail(working);
        let answer = window.prompt(
            PromptLevel::Warning,
            &format!("Update to Mkrew {}?", release.version),
            Some(detail.as_str()),
            &ANSWERS,
            cx,
        );
        cx.spawn_in(window, async move |this, cx| {
            if answer.await.ok() != Some(0) {
                return;
            }
            let _ = this.update(cx, |shell, cx| {
                shell.updating = true;
                cx.notify();
            });
            let dir = std::env::temp_dir().join("mkrew-update");
            let result = cx
                .background_executor()
                .spawn(async move { update::download(&asset, &dir).and_then(|file| update::apply(&kind, &file)) })
                .await;
            let _ = this.update_in(cx, |shell, window, cx| match result {
                Ok(()) => shell.quit_ui(window, cx),
                Err(error) => {
                    shell.updating = false;
                    shell.toast_error(format!("Update failed: {error:#}"), cx);
                }
            });
        })
        .detach();
    }
}
```

  Fix imports to match the crate (where `Icon`, `icon_button_in`, `tooltip`, `opacity`,
  `AfterSave`, `SessionKind` really live — copy the `use` lines of `shell/phone_access.rs` and
  `shell/files.rs`). `ask_unsaved` / `quit_ui` visibility is `pub(crate)` already.

  `chrome.rs:174`: `.children(self.render_update_button(theme, cx))` immediately before
  `.children(self.render_phone_button(theme, cx));`.

- [ ] **Step 4: Run tests + smoke**

Run: `cargo test -p mkrew-ui --lib palette::` → PASS; `cargo build -p mkrew-ui`.
Smoke: `cargo run -p mkrew-ui`, open the palette → "Global: Check for updates" → a toast
(`Mkrew 0.1.5 is up to date` while no release is published, or `… is available`). Report which.

- [ ] **Step 5: Commit**

```bash
git add icon/lucide/circle-arrow-up.svg crates/mkrew-ui/src/icons.rs crates/mkrew-ui/src/shell/update.rs crates/mkrew-ui/src/shell.rs crates/mkrew-ui/src/main.rs crates/mkrew-ui/src/shell/connection.rs crates/mkrew-ui/src/shell/chrome.rs crates/mkrew-ui/src/palette.rs crates/mkrew-ui/src/palette/tests.rs crates/mkrew-ui/src/shell/navigation.rs
git -c user.email=tungnv975@gmail.com commit -m "feat(ui): title bar update button, update check and in-app install"
```

---

### Task 5: Release workflow (edited, committed by the user)

**Files:**
- Modify: `.github/workflows/release.yml` — **do not commit**

- [ ] **Step 1: Replace the `package` job steps and the publish command** with:

```yaml
    steps:
      - run: git config --global core.autocrlf false
      - uses: actions/checkout@v7
      # The tag must name the version being built (README "Releasing").
      - if: startsWith(github.ref, 'refs/tags/v')
        shell: bash
        run: |
          want="$(tr -d '\r' < Cargo.toml | sed -n 's/^version = "\(.*\)"$/\1/p' | head -n 1)"
          [ "${GITHUB_REF_NAME#v}" = "$want" ] || { echo "tag $GITHUB_REF_NAME does not match Cargo.toml $want" >&2; exit 1; }
      - uses: dtolnay/rust-toolchain@stable
      - uses: Swatinem/rust-cache@v2
      # gpui's x11/wayland backends (mkrew-ui) link against these.
      - if: runner.os == 'Linux'
        run: sudo apt-get update && sudo apt-get install -y libxkbcommon-x11-dev libwayland-dev libx11-xcb-dev libfontconfig-dev libasound2-dev
      - if: runner.os == 'Windows'
        run: choco install innosetup -y --no-progress
      - if: runner.os == 'Windows'
        shell: pwsh
        run: ./scripts/package-setup.ps1
      - if: runner.os == 'macOS'
        shell: bash
        run: bash scripts/package.sh --app
      - if: runner.os == 'Linux'
        shell: bash
        run: bash scripts/package.sh
      - uses: actions/upload-artifact@v4
        with:
          name: mkrew-${{ github.ref_name }}-${{ matrix.os }}
          path: |
            dist/*-setup.exe
            dist/*.zip
            dist/*.tar.gz
          if-no-files-found: error
```

  Publish step: drop `--draft`:

```yaml
        run: >
          gh release create "${{ github.ref_name }}" dist/*
          --repo "${{ github.repository }}" --generate-notes
          --title "Mkrew ${{ github.ref_name }}"
```

  Header comment lines 1-2 → `# Release packages (SPEC §4, ADR 0071): Windows setup .exe, macOS Mkrew.app zip, Linux tar.gz.`
  and `# A \`v*\` tag push publishes them as the latest GitHub Release, which running Mkrew copies offer.`

- [ ] **Step 2: Check** the YAML parses: `python -c "import yaml,sys;yaml.safe_load(open('.github/workflows/release.yml'))"` (or skip if PyYAML is missing and say so). No commit.

---

### Task 6: Docs — ADR, SPEC, HUMAN_CHECKS, PROGRESS

**Files:**
- Create: `docs/decisions/0071-in-app-update.md` (re-check `docs/decisions/` first; take the next free number if 0071 is taken and use it everywhere)
- Modify: `docs/SPEC.md` (§4 tree lines 192-193, §14 title bar, new §20K after §20J, M7 line 1618)
- Modify: `docs/HUMAN_CHECKS.md`, `docs/PROGRESS.md`

- [ ] **Step 1: ADR** (template `docs/decisions/0000-template.md`): Context = manual redeploys, draft
  releases, MSI needs admin, macOS bare tar.gz. Decision = GitHub Releases `latest` of the public
  repo, `update.rs` on `ureq`; Windows Inno silent `/LAUNCH=1` (MSI dropped); macOS ad-hoc signed
  `Mkrew.app` zip swapped in place after `mk daemon stop`; agents continue through daemon resume;
  no checksum/signature (HTTPS only). Consequences = shells and running turns are lost on update;
  old MSI installs update by hand once; macOS first open needs right-click → Open.

- [ ] **Step 2: SPEC**
  - §4 tree: `scripts/ … package (setup .exe / tar.gz / macOS app)`, `packaging/ # mkrew.iss (Inno Setup 6 setup .exe)`.
  - §14, after the toasts bullet: `- Update button (§20K): Lucide \`circle-arrow-up\` in \`accent-text\` in the title bar before the phone icon while a newer release is known; dimmed (\`opacity::DISABLED\`) while downloading.`
  - M7 line 1618: `Windows setup .exe (per-user, no admin), macOS app zip, Linux tar.gz.`
  - New `### §20K In-app update (outside the milestones, ADR 0071)` with spec sections 4-6 in
    short: source URL, schedule, `is_newer` rule, assets, `InstallKind`, prompt text, Windows
    and macOS apply, what survives (SPEC §8 resume) and what does not.

- [ ] **Step 3: HUMAN_CHECKS** — new `### To verify — 2026-10-21 in-app update (ADR 0071)` with the
  checks from the spec "Testing" section plus: unsaved Files edit → Update asks Save / Don't save
  first; dev build (`cargo run`) → button opens the release page; Windows "for all users"
  install → UAC appears once. Under "Waiting for the human": commit `.github/workflows/release.yml`,
  `packaging/mkrew.iss`, the deletion of `packaging/mkrew.wxs`; push a `v<next>` tag.

- [ ] **Step 4: PROGRESS** — one line under the outside-the-loop notes:
  `- 2026-10-21 in-app update (plan docs/superpowers/plans/2026-10-21-auto-update.md, ADR 0071, no protocol/migration): mkrew_ui::update (GitHub latest, Inno /LAUNCH=1, macOS app swap), shell/update.rs title bar button + palette "Check for updates"; MSI dropped; package.sh --app. Manual checks in HUMAN_CHECKS "2026-10-21 in-app update". Outside the milestone loop.`

- [ ] **Step 5: Full verify, then commit**

Run: `"C:/Program Files/Git/bin/bash.exe" scripts/verify.sh`
Expected: fmt, clippy, tests all pass.

```bash
git add docs/decisions/0071-in-app-update.md docs/SPEC.md docs/HUMAN_CHECKS.md docs/PROGRESS.md
git -c user.email=tungnv975@gmail.com commit -m "docs: in-app update (ADR 0071, SPEC §20K)"
```
