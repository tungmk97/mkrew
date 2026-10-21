//! Release packaging (SPEC §4): the setup .exe and the tar.gz ship `mkrew`, `mk`, the daemon
//! and every sample agent profile.

// Helpers outside `#[test]` fns are not covered by `allow-unwrap-in-tests`.
#![allow(clippy::unwrap_used)]

use std::path::{Path, PathBuf};

const BINS: [&str; 3] = ["mkrew", "mk", "mkrew-daemon"];

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

#[cfg(unix)]
fn sample_profiles() -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(repo_root().join("agents"))
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|n| n.ends_with(".toml"))
        .collect();
    names.sort();
    assert!(names.len() >= 5, "{names:?}");
    names
}

/// A silent in-app update (`/LAUNCH=1`, ADR 0071) reopens Mkrew as the logged-in user (an
/// all-users setup runs elevated; the app, daemon and agents must not); a normal silent install
/// does not reopen it.
#[test]
fn setup_exe_relaunches_after_an_in_app_update() {
    let iss = std::fs::read_to_string(repo_root().join("packaging/mkrew.iss")).unwrap();
    assert!(iss.contains(
        r#"Filename: "{app}\bin\mkrew.exe"; Flags: nowait runasoriginaluser; Check: LaunchAfterUpdate"#
    ));
    assert!(iss.contains("function LaunchAfterUpdate: Boolean;"));
    assert!(iss.contains("ExpandConstant('{param:LAUNCH|0}') = '1'"));
}

/// The setup .exe ships the same files and lets the user pick per-user (no admin) or all users.
#[test]
fn setup_exe_lists_binaries_profiles_and_both_scopes() {
    let iss = std::fs::read_to_string(repo_root().join("packaging/mkrew.iss")).unwrap();
    for bin in BINS {
        let source = format!(r#"Source: "{{#BinDir}}\{bin}.exe"; DestDir: "{{app}}\bin""#);
        assert!(iss.contains(&source), "missing {source}");
    }
    assert!(iss.contains(r#"Source: "{#AgentsDir}\*.toml"; DestDir: "{app}\agents""#));
    assert!(iss.contains("PrivilegesRequired=lowest"));
    assert!(iss.contains("PrivilegesRequiredOverridesAllowed=dialog commandline"));
    assert!(iss.contains("DefaultDirName={autopf}\\Mkrew"));
}

// `package.sh` targets Linux/macOS; on Windows `bash` may resolve to WSL (System32 first).
#[cfg(unix)]
#[test]
fn tarball_layout() {
    use std::process::Command;

    let tmp = tempfile::tempdir().unwrap();
    let bin_dir = tmp.path().join("bin");
    let out = tmp.path().join("out");
    std::fs::create_dir(&bin_dir).unwrap();
    for bin in BINS {
        std::fs::write(bin_dir.join(bin), "fake").unwrap();
    }
    let output = Command::new("bash")
        .arg(repo_root().join("scripts/package.sh"))
        .args(["--no-build", "--bin-dir"])
        .arg(&bin_dir)
        .arg("--out")
        .arg(&out)
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    let archive = String::from_utf8(output.stdout).unwrap().trim().to_owned();
    let name = Path::new(&archive)
        .file_name()
        .unwrap()
        .to_str()
        .unwrap()
        .strip_suffix(".tar.gz")
        .unwrap()
        .to_owned();
    assert!(
        name.starts_with(&format!("mkrew-{}-", env!("CARGO_PKG_VERSION"))),
        "{name}"
    );

    let listing = Command::new("tar")
        .arg("-tzf")
        .arg(&archive)
        .output()
        .unwrap();
    assert!(listing.status.success(), "{listing:?}");
    let mut files: Vec<String> = String::from_utf8(listing.stdout)
        .unwrap()
        .lines()
        .filter(|l| !l.ends_with('/'))
        .map(str::to_owned)
        .collect();
    files.sort();

    let mut expected = vec![format!("{name}/README.md")];
    expected.extend(
        sample_profiles()
            .iter()
            .map(|p| format!("{name}/agents/{p}")),
    );
    expected.extend(BINS.iter().map(|b| format!("{name}/bin/{b}")));
    expected.sort();
    assert_eq!(files, expected);
}
