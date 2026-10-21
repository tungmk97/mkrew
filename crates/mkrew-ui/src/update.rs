//! In-app update from GitHub Releases (SPEC §20K, ADR 0071): what is new, which file fits this
//! install, and how to put it in place. Pure parts first; the I/O below is thin.

use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Context, Result};
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
    let local = std::env::var_os("LOCALAPPDATA").map(PathBuf::from);
    install_kind(std::env::consts::OS, &exe, local.as_deref())
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
        let local = tmp.path().join("Local");
        let app = local.join("Programs").join("Mkrew");
        std::fs::create_dir_all(app.join("bin")).unwrap();
        let exe = app.join("bin").join("mkrew.exe");
        // No uninstaller yet: a dev build or an MSI install.
        assert_eq!(
            install_kind("windows", &exe, Some(&local)),
            InstallKind::Manual
        );
        std::fs::write(app.join("unins000.exe"), b"").unwrap();
        assert_eq!(
            install_kind("windows", &exe, Some(&local)),
            InstallKind::Inno { per_user: true }
        );
        let other = tmp.path().join("Elsewhere");
        assert_eq!(
            install_kind("windows", &exe, Some(&other)),
            InstallKind::Inno { per_user: false }
        );
        assert_eq!(
            install_kind("windows", &exe, None),
            InstallKind::Inno { per_user: false }
        );
    }

    #[test]
    fn mac_app_is_detected_by_the_bundle_path() {
        let tmp = tempfile::tempdir().unwrap();
        let bundle = tmp.path().join("Mkrew.app");
        let exe = bundle.join("Contents").join("MacOS").join("mkrew");
        assert_eq!(
            install_kind("macos", &exe, None),
            InstallKind::MacApp { bundle }
        );
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
