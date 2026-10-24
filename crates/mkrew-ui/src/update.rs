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

/// `program_files` = the machine's Program Files folders: an Inno install inside one is "for all
/// users", anywhere else (`%LOCALAPPDATA%\Programs`, a custom folder) "for me only".
pub fn install_kind(os: &str, exe: &Path, program_files: &[PathBuf]) -> InstallKind {
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
            let all_users = program_files
                .iter()
                .any(|dir| lower(exe).starts_with(&lower(dir)));
            InstallKind::Inno {
                per_user: !all_users,
            }
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
                    InstallKind::MacApp {
                        bundle: b.to_path_buf(),
                    }
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
    let program_files: Vec<PathBuf> = ["ProgramFiles", "ProgramW6432", "ProgramFiles(x86)"]
        .into_iter()
        .filter_map(std::env::var_os)
        .map(PathBuf::from)
        .collect();
    install_kind(std::env::consts::OS, &exe, &program_files)
}

/// Inno setup switches: silent, reopen Mkrew after (`[Run]` `LaunchAfterUpdate`), same scope.
pub fn installer_args(per_user: bool) -> [&'static str; 5] {
    let scope = if per_user {
        "/CURRENTUSER"
    } else {
        "/ALLUSERS"
    };
    [
        "/VERYSILENT",
        "/SUPPRESSMSGBOXES",
        "/NORESTART",
        "/LAUNCH=1",
        scope,
    ]
}

/// The confirmation's detail line; `working` = agent sessions in `working`.
pub fn prompt_detail(working: usize) -> String {
    let mut text =
        String::from("Mkrew closes, installs and reopens. Agent panes resume their conversation.");
    match working {
        0 => {}
        1 => text.push_str(" 1 agent is working; its current turn stops."),
        n => text.push_str(&format!(
            " {n} agents are working; their current turn stops."
        )),
    }
    text.push_str(" Shells and run commands without autostart do not come back.");
    text
}

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
    let body = response
        .body_mut()
        .read_to_string()
        .context("cannot read GitHub's answer")?;
    if status != 200 {
        bail!("GitHub answered {status}");
    }
    parse_release(&body).map(Some)
}

/// The asset's name as a plain file name (it comes from the network).
fn safe_file_name(name: &str) -> Result<&str> {
    let plain = !name.is_empty() && name != "." && name != ".." && !name.contains(['/', '\\', ':']);
    if plain {
        Ok(name)
    } else {
        bail!("refusing the asset name {name:?}")
    }
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
    let mut file =
        std::fs::File::create(&path).with_context(|| format!("create {}", path.display()))?;
    // The limit makes a longer body an error instead of an endless write. One byte over the
    // size: ureq's limit reader errors on the read after `limit` bytes even at end of body, so
    // `limit(size)` failed every complete download; a longer body still fails the check below.
    let mut body = response
        .body_mut()
        .with_config()
        .limit(asset.size + 1)
        .reader();
    let written =
        std::io::copy(&mut body, &mut file).with_context(|| format!("download {}", asset.name))?;
    if written != asset.size {
        bail!(
            "download of {} stopped at {written} of {} bytes",
            asset.name,
            asset.size
        );
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
                std::process::Command::new("/usr/bin/ditto")
                    .args(["-x", "-k"])
                    .arg(file)
                    .arg(&dir),
                "ditto",
            )?;
            let bin = bundle.join("Contents").join("MacOS");
            // The daemon on our socket is this app's. Plain `stop`: `--only-from` reads `/proc`,
            // which macOS lacks (it would leave the old daemon running). Returns once the daemon
            // released its lock; agents resume on the next start.
            run(
                std::process::Command::new(bin.join("mk")).args(["daemon", "stop"]),
                "mk daemon stop",
            )?;
            swap_bundle(bundle, &dir.join("Mkrew.app"))?;
            let _ = std::fs::remove_dir_all(&dir);
            // Started directly (not `open`), so the new app and its daemon keep this process's
            // environment (`PATH` with `claude`/`codex` when Mkrew was started from a terminal).
            let mut relaunch = std::process::Command::new(bin.join("mkrew"));
            #[cfg(unix)]
            {
                use std::os::unix::process::CommandExt;
                // Its own process group: it outlives this UI.
                relaunch.process_group(0);
            }
            relaunch.spawn().context("open the new Mkrew")?;
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

    /// Serves `body` once on localhost (no network) and returns the asset pointing at it.
    fn serve(body: &'static [u8], size: u64) -> Asset {
        use std::io::{Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = [0u8; 1024];
            let _ = stream.read(&mut request);
            let head = format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            );
            stream.write_all(head.as_bytes()).unwrap();
            stream.write_all(body).unwrap();
        });
        Asset {
            name: "setup.exe".into(),
            url: format!("http://127.0.0.1:{port}/setup.exe"),
            size,
        }
    }

    #[test]
    fn a_complete_download_succeeds_and_a_longer_body_fails() {
        let dir = tempfile::tempdir().unwrap();
        let path = download(&serve(b"0123456789", 10), dir.path()).unwrap();
        assert_eq!(std::fs::read(path).unwrap(), b"0123456789");
        let error = download(&serve(b"0123456789AB", 10), dir.path()).unwrap_err();
        assert!(format!("{error:#}").contains("download"), "{error:#}");
    }

    #[test]
    fn asset_names_with_path_parts_are_refused() {
        assert_eq!(
            safe_file_name("mkrew-0.1.7-x64-setup.exe").unwrap(),
            "mkrew-0.1.7-x64-setup.exe"
        );
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
        assert_eq!(
            std::fs::read_to_string(old_bundle(&bundle).join("v")).unwrap(),
            "old"
        );
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

    #[test]
    fn parses_release_and_strips_v() {
        let release = parse_release(JSON).unwrap();
        assert_eq!(release.version, "0.1.7");
        assert_eq!(
            release.page_url,
            "https://github.com/tungmk97/mkrew/releases/tag/v0.1.7"
        );
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
        assert_eq!(
            pick_asset("windows", "x86_64", &assets).unwrap().name,
            "mkrew-0.1.7-x64-setup.exe"
        );
        assert_eq!(
            pick_asset("macos", "aarch64", &assets).unwrap().name,
            "mkrew-0.1.7-macos-arm64.zip"
        );
        assert!(pick_asset("macos", "x86_64", &assets).is_none());
        assert!(pick_asset("linux", "x86_64", &assets).is_none());
        assert!(pick_asset("windows", "x86_64", &[]).is_none());
    }

    #[test]
    fn inno_install_is_detected_by_its_uninstaller() {
        let tmp = tempfile::tempdir().unwrap();
        let program_files = tmp.path().join("Program Files");
        let custom = tmp.path().join("Apps").join("Mkrew");
        std::fs::create_dir_all(custom.join("bin")).unwrap();
        let exe = custom.join("bin").join("mkrew.exe");
        // No uninstaller yet: a dev build or an MSI install.
        assert_eq!(
            install_kind("windows", &exe, std::slice::from_ref(&program_files)),
            InstallKind::Manual
        );
        // "For me only" in a custom folder is still per-user: no UAC, no second copy.
        std::fs::write(custom.join("unins000.exe"), b"").unwrap();
        assert_eq!(
            install_kind("windows", &exe, std::slice::from_ref(&program_files)),
            InstallKind::Inno { per_user: true }
        );
        let all_users = program_files.join("Mkrew");
        std::fs::create_dir_all(all_users.join("bin")).unwrap();
        std::fs::write(all_users.join("unins000.exe"), b"").unwrap();
        let exe = all_users.join("bin").join("mkrew.exe");
        assert_eq!(
            install_kind("windows", &exe, &[tmp.path().join("x86"), program_files]),
            InstallKind::Inno { per_user: false }
        );
    }

    #[test]
    fn mac_app_is_detected_by_the_bundle_path() {
        let tmp = tempfile::tempdir().unwrap();
        let bundle = tmp.path().join("Mkrew.app");
        let exe = bundle.join("Contents").join("MacOS").join("mkrew");
        assert_eq!(
            install_kind("macos", &exe, &[]),
            InstallKind::MacApp { bundle }
        );
        let dev = tmp.path().join("target").join("release").join("mkrew");
        assert_eq!(install_kind("macos", &dev, &[]), InstallKind::Manual);
        assert_eq!(install_kind("linux", &exe, &[]), InstallKind::Manual);
    }

    #[test]
    fn prompt_names_working_agents() {
        assert!(!prompt_detail(0).contains("working"));
        assert!(prompt_detail(1).contains("1 agent is working; its current turn stops."));
        assert!(prompt_detail(3).contains("3 agents are working; their current turn stops."));
        assert!(prompt_detail(0).contains("Agent panes resume their conversation."));
    }
}
