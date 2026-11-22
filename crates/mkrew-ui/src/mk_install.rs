//! "Global: Install mk command" (macOS app, SPEC §20L, ADR 0100): a symlink to the bundle's
//! `mk` in a folder on `PATH`, so Terminal finds `mk` (the `mk` of the app is the one that
//! speaks the app's daemon protocol).

use std::ffi::OsStr;
use std::io;
use std::path::{Path, PathBuf};

/// The folders tried in order: `/usr/local/bin` (on every Mac's default `PATH`; writable
/// without admin only where Homebrew made it so), then `~/.local/bin` (always writable; the
/// user adds it to `PATH`).
pub fn link_dirs(home: Option<&Path>) -> Vec<PathBuf> {
    let mut dirs = vec![PathBuf::from("/usr/local/bin")];
    dirs.extend(home.map(|home| home.join(".local").join("bin")));
    dirs
}

/// What an install made.
#[derive(Debug, PartialEq, Eq)]
pub struct Linked {
    /// The link, `<folder>/mk`.
    pub link: PathBuf,
    /// Its folder is on `PATH` already.
    pub on_path: bool,
    /// An older link of ours in an earlier folder could not be replaced and still shadows this
    /// one in Terminal; the toast names it and the elevated `ln -sf` that fixes it.
    pub shadowed_by: Option<PathBuf>,
}

/// Links `target` as `mk` in the first of `dirs` that takes it; `path` (the `PATH` string) says
/// whether that folder is searched. An error lists why each folder refused.
pub fn install(target: &Path, dirs: &[PathBuf], path: Option<&OsStr>) -> Result<Linked, String> {
    let mut refused = Vec::new();
    let mut shadowed_by = None;
    for dir in dirs {
        match link_in(target, dir) {
            Ok(link) => {
                let on_path = path.is_some_and(|path| {
                    std::env::split_paths(path).any(|searched| searched == *dir)
                });
                return Ok(Linked {
                    link,
                    on_path,
                    shadowed_by,
                });
            }
            Err(why) => {
                // An older link of ours that could not be replaced (root-owned): it keeps
                // winning in Terminal when its folder comes first on `PATH`.
                if shadowed_by.is_none() && is_stale_ours(&dir.join("mk"), target) {
                    shadowed_by = Some(dir.join("mk"));
                }
                refused.push(why);
            }
        }
    }
    if refused.is_empty() {
        return Err("no folder to link into".to_owned());
    }
    Err(refused.join("; "))
}

/// `link` is a symlink to some bundle's `mk` other than `target` (a stale link of ours).
fn is_stale_ours(link: &Path, target: &Path) -> bool {
    std::fs::read_link(link).is_ok_and(|to| to != target && to.ends_with("Contents/MacOS/mk"))
}

fn link_in(target: &Path, dir: &Path) -> Result<PathBuf, String> {
    let link = dir.join("mk");
    let fail = |what: &str, error: io::Error| format!("{what} {}: {error}", dir.display());
    std::fs::create_dir_all(dir).map_err(|error| fail("cannot create", error))?;
    match std::fs::symlink_metadata(&link) {
        Ok(meta) if meta.file_type().is_symlink() => {
            let to = std::fs::read_link(&link);
            // Made already (maybe by the root command of the error hint): leave it, so a
            // root-owned link stays and no second one is made.
            if to.as_deref().is_ok_and(|to| to == target) {
                return Ok(link);
            }
            // A stale link of an older bundle is replaced; a link to something that is not
            // the bundle's `mk` is somebody else's.
            let ours = to.is_ok_and(|to| to.ends_with("Contents/MacOS/mk"));
            if !ours {
                return Err(format!("{} links elsewhere", link.display()));
            }
            std::fs::remove_file(&link)
                .map_err(|error| fail("cannot replace the link in", error))?;
        }
        Ok(_) => return Err(format!("{} exists and is not a link", link.display())),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(fail("cannot look in", error)),
    }
    symlink(target, &link).map_err(|error| fail("cannot link in", error))?;
    Ok(link)
}

#[cfg(unix)]
fn symlink(target: &Path, link: &Path) -> io::Result<()> {
    std::os::unix::fs::symlink(target, link)
}

#[cfg(not(unix))]
fn symlink(_target: &Path, _link: &Path) -> io::Result<()> {
    Err(io::ErrorKind::Unsupported.into())
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use super::*;

    #[test]
    fn the_link_folders_are_the_system_one_then_the_users() {
        let home = Path::new("/Users/me");
        assert_eq!(
            link_dirs(Some(home)),
            [
                PathBuf::from("/usr/local/bin"),
                home.join(".local").join("bin")
            ]
        );
        assert_eq!(link_dirs(None), [PathBuf::from("/usr/local/bin")]);
    }

    #[cfg(unix)]
    mod on_disk {
        use super::*;

        fn bundle_mk(root: &Path) -> PathBuf {
            let target = root.join("Mkrew.app/Contents/MacOS/mk");
            std::fs::create_dir_all(target.parent().unwrap()).unwrap();
            std::fs::write(&target, b"mk").unwrap();
            target
        }

        #[test]
        fn it_links_into_the_first_folder_that_works_and_again_without_error() {
            let tmp = tempfile::tempdir().unwrap();
            let target = bundle_mk(tmp.path());
            let (first, second) = (tmp.path().join("bin1"), tmp.path().join("bin2"));
            let dirs = [first.clone(), second];
            let linked = install(&target, &dirs, None).unwrap();
            assert_eq!(linked.link, first.join("mk"));
            assert!(!linked.on_path);
            assert_eq!(std::fs::read_link(&linked.link).unwrap(), target);
            assert_eq!(
                install(&target, &dirs, None).unwrap(),
                linked,
                "twice is fine"
            );
        }

        #[test]
        fn on_path_says_whether_the_folder_is_already_searched() {
            let tmp = tempfile::tempdir().unwrap();
            let target = bundle_mk(tmp.path());
            let dir = tmp.path().join("bin");
            let path = std::env::join_paths([Path::new("/usr/bin"), dir.as_path()]).unwrap();
            let linked = install(&target, std::slice::from_ref(&dir), Some(&path)).unwrap();
            assert!(linked.on_path);
        }

        #[test]
        fn an_unusable_first_folder_falls_to_the_next() {
            let tmp = tempfile::tempdir().unwrap();
            let target = bundle_mk(tmp.path());
            let blocker = tmp.path().join("a-file");
            std::fs::write(&blocker, b"").unwrap();
            // A folder below a file cannot be created, as an unwritable /usr/local/bin cannot.
            let first = blocker.join("bin");
            let second = tmp.path().join("bin2");
            let linked = install(&target, &[first, second.clone()], None).unwrap();
            assert_eq!(linked.link, second.join("mk"));
        }

        #[test]
        fn a_real_file_or_a_foreign_link_named_mk_is_never_replaced() {
            let tmp = tempfile::tempdir().unwrap();
            let target = bundle_mk(tmp.path());
            let first = tmp.path().join("bin1");
            std::fs::create_dir_all(&first).unwrap();
            std::fs::write(first.join("mk"), b"mine").unwrap();
            let second = tmp.path().join("bin2");
            std::fs::create_dir_all(&second).unwrap();
            let elsewhere = tmp.path().join("other-tool");
            std::fs::write(&elsewhere, b"x").unwrap();
            std::os::unix::fs::symlink(&elsewhere, second.join("mk")).unwrap();

            let error = install(&target, &[first.clone(), second.clone()], None).unwrap_err();
            assert!(
                error.contains("not a link") && error.contains("links elsewhere"),
                "{error}"
            );
            assert_eq!(std::fs::read(first.join("mk")).unwrap(), b"mine");
            assert_eq!(std::fs::read_link(second.join("mk")).unwrap(), elsewhere);
        }

        #[test]
        fn a_stale_link_to_an_older_bundle_is_replaced() {
            let tmp = tempfile::tempdir().unwrap();
            let target = bundle_mk(tmp.path());
            let dir = tmp.path().join("bin");
            std::fs::create_dir_all(&dir).unwrap();
            std::os::unix::fs::symlink("/gone/Mkrew.app/Contents/MacOS/mk", dir.join("mk"))
                .unwrap();
            let linked = install(&target, std::slice::from_ref(&dir), None).unwrap();
            assert_eq!(std::fs::read_link(&linked.link).unwrap(), target);
        }

        #[test]
        fn a_link_already_at_the_target_is_left_alone() {
            use std::os::unix::fs::MetadataExt as _;
            let tmp = tempfile::tempdir().unwrap();
            let target = bundle_mk(tmp.path());
            let dir = tmp.path().join("bin");
            std::fs::create_dir_all(&dir).unwrap();
            std::os::unix::fs::symlink(&target, dir.join("mk")).unwrap();
            let before = std::fs::symlink_metadata(dir.join("mk")).unwrap().ino();
            let linked = install(&target, std::slice::from_ref(&dir), None).unwrap();
            assert_eq!(linked.link, dir.join("mk"));
            let after = std::fs::symlink_metadata(dir.join("mk")).unwrap().ino();
            assert_eq!(before, after, "not removed and made again");
        }

        #[test]
        fn a_link_to_some_other_file_named_mk_is_not_ours() {
            let tmp = tempfile::tempdir().unwrap();
            let target = bundle_mk(tmp.path());
            let dir = tmp.path().join("bin");
            std::fs::create_dir_all(&dir).unwrap();
            let theirs = tmp.path().join("other/mk");
            std::fs::create_dir_all(theirs.parent().unwrap()).unwrap();
            std::fs::write(&theirs, b"x").unwrap();
            std::os::unix::fs::symlink(&theirs, dir.join("mk")).unwrap();
            let error = install(&target, std::slice::from_ref(&dir), None).unwrap_err();
            assert!(error.contains("links elsewhere"), "{error}");
            assert_eq!(std::fs::read_link(dir.join("mk")).unwrap(), theirs);
        }

        #[test]
        fn nothing_to_link_into_is_an_error() {
            let error = install(Path::new("/x/mk"), &[], Some(OsStr::new("/usr/bin"))).unwrap_err();
            assert!(error.contains("no folder"), "{error}");
        }

        #[test]
        fn an_older_link_of_ours_is_stale_and_anything_else_is_not() {
            let tmp = tempfile::tempdir().unwrap();
            let target = bundle_mk(tmp.path());
            let old = tmp.path().join("old-link");
            std::os::unix::fs::symlink("/gone/Mkrew.app/Contents/MacOS/mk", &old).unwrap();
            assert!(is_stale_ours(&old, &target));
            let same = tmp.path().join("same-link");
            std::os::unix::fs::symlink(&target, &same).unwrap();
            assert!(!is_stale_ours(&same, &target), "already right");
            let other = tmp.path().join("other-link");
            std::os::unix::fs::symlink("/usr/bin/true", &other).unwrap();
            assert!(!is_stale_ours(&other, &target));
            assert!(!is_stale_ours(&tmp.path().join("none"), &target));
        }
    }
}
