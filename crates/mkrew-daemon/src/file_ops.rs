//! `fs.create` and `fs.ignore` (spec 2026-10-29 §5): New file, New folder and Add to .gitignore of the Files tree, done by the daemon so every window (local or another computer's) works the same.

use std::fs;
use std::io::{ErrorKind, Write};
use std::path::{Component, Path, PathBuf};

/// `rel` (`/`-separated, relative to `base`) below `base` with native separators. An empty
/// `rel` is `base` itself, without a trailing separator.
fn join_relative(base: PathBuf, rel: &str) -> PathBuf {
    rel.split('/')
        .filter(|part| !part.is_empty())
        .fold(base, |full, part| full.join(part))
}

/// Creates `name` in `dir` (`/`-separated, relative to `root`; `""` = `root`): an empty file,
/// or a folder when `folder`. `name` may hold `/` (or `\`): missing folders on the way are
/// created. An existing entry, an empty name and `.` / `..` / drive parts are refused, so
/// nothing is made outside `root`. Returns the new entry relative to `root`; an error carries
/// its [`ErrorKind`] (`AlreadyExists` for an existing entry) and a message for the user.
pub(crate) fn create(
    root: &Path,
    dir: &str,
    name: &str,
    folder: bool,
) -> Result<String, (ErrorKind, String)> {
    let parts: Vec<&str> = name
        .split(['/', '\\'])
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .collect();
    let plain = |part: &&str| {
        let mut components = Path::new(part).components();
        matches!(components.next(), Some(Component::Normal(_))) && components.next().is_none()
    };
    if parts.is_empty() || !parts.iter().all(plain) {
        return Err((
            ErrorKind::InvalidInput,
            format!("\"{}\" is not a valid name", name.trim()),
        ));
    }
    let rel = std::iter::once(dir)
        .filter(|dir| !dir.is_empty())
        .chain(parts)
        .collect::<Vec<_>>()
        .join("/");
    let path = join_relative(root.to_path_buf(), &rel);
    let made = path
        .parent()
        .map_or(Ok(()), fs::create_dir_all)
        .and_then(|()| {
            if folder {
                fs::create_dir(&path)
            } else {
                fs::File::create_new(&path).map(drop)
            }
        });
    match made {
        Ok(()) => Ok(rel),
        Err(error) => Err((error.kind(), format!("Cannot create {rel}: {error}"))),
    }
}

/// Adds `rel` (`/`-separated, relative to `root`) to `root/.gitignore` as a pattern matching
/// only that entry (`/rel`, `/rel/` for a folder), creating the file if needed and keeping its
/// line endings. A pattern already listed is not added twice.
pub(crate) fn ignore(root: &Path, rel: &str, is_dir: bool) -> Result<(), String> {
    let mut pattern = String::from("/");
    for c in rel.chars() {
        if matches!(c, '*' | '?' | '[' | '\\') {
            pattern.push('\\');
        }
        pattern.push(c);
    }
    if is_dir {
        pattern.push('/');
    }
    let file = root.join(".gitignore");
    let failed = |error: std::io::Error| format!("Cannot update .gitignore: {error}");
    let text = match fs::read_to_string(&file) {
        Ok(text) => text,
        Err(error) if error.kind() == ErrorKind::NotFound => String::new(),
        Err(error) => return Err(failed(error)),
    };
    if text.lines().any(|line| line.trim_end() == pattern) {
        return Ok(());
    }
    let eol = if text.contains("\r\n") { "\r\n" } else { "\n" };
    let lead = if text.is_empty() || text.ends_with('\n') {
        ""
    } else {
        eol
    };
    fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&file)
        .and_then(|mut out| write!(out, "{lead}{pattern}{eol}"))
        .map_err(failed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_makes_nested_entries_and_refuses_escapes_and_existing() {
        let root = tempfile::tempdir().unwrap();
        let root = root.path();
        fs::create_dir(root.join("src")).unwrap();
        assert_eq!(create(root, "src", "a/b.rs", false).unwrap(), "src/a/b.rs");
        assert!(root.join("src/a/b.rs").is_file());
        assert_eq!(
            create(root, "", " docs\\notes ", true).unwrap(),
            "docs/notes"
        );
        assert!(root.join("docs/notes").is_dir());
        let (kind, _) = create(root, "src", "a/b.rs", false).unwrap_err();
        assert_eq!(kind, ErrorKind::AlreadyExists);
        for bad in ["", " / ", "..", "a/../../x", "."] {
            assert!(create(root, "src", bad, false).is_err(), "{bad:?}");
        }
        #[cfg(windows)]
        assert!(create(root, "", "C:/x", false).is_err());
    }

    #[test]
    fn ignore_appends_an_anchored_escaped_line_once() {
        let root = tempfile::tempdir().unwrap();
        let root = root.path();
        ignore(root, "build", true).unwrap();
        assert_eq!(
            fs::read_to_string(root.join(".gitignore")).unwrap(),
            "/build/\n"
        );
        fs::write(root.join(".gitignore"), "target\r\n*.log").unwrap();
        ignore(root, "a b/x[1]*.txt", false).unwrap();
        ignore(root, "a b/x[1]*.txt", false).unwrap();
        assert_eq!(
            fs::read_to_string(root.join(".gitignore")).unwrap(),
            "target\r\n*.log\r\n/a b/x\\[1]\\*.txt\r\n"
        );
    }
}
