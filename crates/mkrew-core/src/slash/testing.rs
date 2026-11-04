//! A fake home and repo in a tempdir for the scanner tests.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use super::{Os, ScanRoots};

pub(super) struct World {
    _tmp: tempfile::TempDir,
    pub home: PathBuf,
    pub repo: PathBuf,
    pub managed: PathBuf,
}

/// `home/`, `managed/` and `repo/` (with a `.git` folder) in a new tempdir.
pub(super) fn world() -> World {
    let tmp = tempfile::tempdir().unwrap();
    let (home, repo, managed) = (
        tmp.path().join("home"),
        tmp.path().join("repo"),
        tmp.path().join("managed"),
    );
    fs::create_dir_all(repo.join(".git")).unwrap();
    World {
        _tmp: tmp,
        home,
        repo,
        managed,
    }
}

impl World {
    pub fn roots(&self, folder: &Path) -> ScanRoots {
        ScanRoots {
            folder: folder.to_path_buf(),
            home: self.home.clone(),
            managed: vec![self.managed.clone()],
            os: Os::Linux,
            env: BTreeMap::new(),
        }
    }
}

/// Writes `text` to `root/rel`, creating the folders.
pub(super) fn put(root: &Path, rel: &str, text: &str) {
    let path = root.join(rel);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, text).unwrap();
}

/// A markdown file with `description` as front matter.
pub(super) fn doc(description: &str) -> String {
    format!("---\ndescription: {description}\n---\nBody\n")
}

/// A skill file with `name` and `description`.
pub(super) fn skill(name: &str, description: &str) -> String {
    format!("---\nname: {name}\ndescription: {description}\n---\nBody\n")
}
