//! `fs.create` and `fs.ignore` (spec 2026-10-29 §5): New file, New folder and Add to .gitignore of the Files tree, done by the daemon so every window (local or another computer's) works the same.

use std::fs;
use std::io::{ErrorKind, Write};
use std::path::{Path, PathBuf};

use chrono::NaiveDateTime;
use mkrew_core::notes::{self, NotesFolder};

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
    let plain = |part: &&str| notes::is_plain(std::ffi::OsStr::new(part));
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

/// The most names `new_note` tries before it gives up (`note-<time>.md`, `-2`, ... `-1000`).
const NOTE_TRIES: u32 = 1000;

/// An empty note named for local time `at` in `folder`: below `root` for a relative folder,
/// where it says for an absolute one; missing folders are made. A taken name moves on to the
/// next number (`create` never overwrites). Returns the note relative to `root` when it is
/// inside it, else as an absolute `/`-separated path. An error carries its [`ErrorKind`] and a
/// message for the user that names the folder.
pub(crate) fn new_note(
    root: &Path,
    folder: &NotesFolder,
    at: NaiveDateTime,
) -> Result<String, (ErrorKind, String)> {
    let (base, dir) = match folder {
        NotesFolder::Relative(dir) => (root, dir.as_str()),
        NotesFolder::Absolute(path) => (path.as_path(), ""),
    };
    // Make the folder first: a file standing where a folder should be answers `AlreadyExists`
    // on Windows, which the loop below would mistake for a taken name.
    let place = join_relative(base.to_path_buf(), dir);
    if let Err(error) = fs::create_dir_all(&place) {
        let kind = match error.kind() {
            ErrorKind::AlreadyExists => ErrorKind::NotADirectory,
            kind => kind,
        };
        return Err((
            kind,
            format!(
                "Cannot create the notes folder {}: {error}",
                place.display()
            ),
        ));
    }
    for attempt in 1..=NOTE_TRIES {
        match create(base, dir, &notes::file_name(at, attempt), false) {
            Ok(made) => return Ok(answer(root, folder, &made)),
            Err((ErrorKind::AlreadyExists, _)) => {}
            Err((kind, message)) => {
                return Err((
                    kind,
                    format!("{message} (notes folder {})", place.display()),
                ));
            }
        }
    }
    Err((
        ErrorKind::AlreadyExists,
        format!("No free note name in the notes folder after {NOTE_TRIES} tries"),
    ))
}

/// `made` (relative to the folder `new_note` used) as the client sees it. The root is the
/// project's canonical path (on-disk case, no 8.3 names, no `..`), the folder is spelled as the
/// user typed it, so both are canonicalized before the prefix test (the folder exists now).
fn answer(root: &Path, folder: &NotesFolder, made: &str) -> String {
    let NotesFolder::Absolute(path) = folder else {
        return made.to_owned();
    };
    let canonical = |path: &Path| dunce::canonicalize(path).unwrap_or_else(|_| path.to_owned());
    let inside = canonical(path)
        .strip_prefix(canonical(root))
        .map(Path::to_path_buf)
        .ok();
    match inside {
        Some(inside) if inside.as_os_str().is_empty() => made.to_owned(),
        Some(inside) => format!("{}/{made}", inside.to_string_lossy().replace('\\', "/")),
        None => format!("{}/{made}", path.to_string_lossy().replace('\\', "/")),
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

    use chrono::NaiveDate;

    fn at() -> chrono::NaiveDateTime {
        NaiveDate::from_ymd_opt(2026, 11, 23)
            .unwrap()
            .and_hms_opt(9, 5, 7)
            .unwrap()
    }

    fn slash(path: &Path) -> String {
        path.to_string_lossy().replace('\\', "/")
    }

    #[test]
    fn new_note_numbers_a_taken_name_and_never_overwrites() {
        let root = tempfile::tempdir().unwrap();
        let root = root.path();
        let folder = NotesFolder::Relative(String::new());
        let made: Vec<String> = (0..3)
            .map(|_| new_note(root, &folder, at()).unwrap())
            .collect();
        assert_eq!(
            made,
            [
                "note-20261123-090507.md",
                "note-20261123-090507-2.md",
                "note-20261123-090507-3.md"
            ]
        );
        for name in &made {
            assert_eq!(fs::read(root.join(name)).unwrap(), b"");
        }
        fs::write(root.join("note-20261123-090507.md"), "mine").unwrap();
        assert_eq!(
            new_note(root, &folder, at()).unwrap(),
            "note-20261123-090507-4.md"
        );
        assert_eq!(
            fs::read_to_string(root.join("note-20261123-090507.md")).unwrap(),
            "mine"
        );
    }

    #[test]
    fn new_note_makes_a_relative_folder_and_answers_relative() {
        let root = tempfile::tempdir().unwrap();
        let folder = NotesFolder::Relative("notes/inbox".into());
        let made = new_note(root.path(), &folder, at()).unwrap();
        assert_eq!(made, "notes/inbox/note-20261123-090507.md");
        assert!(root.path().join(&made).is_file());
    }

    #[test]
    fn new_note_answers_relative_inside_the_root_and_absolute_outside_it() {
        let root = tempfile::tempdir().unwrap();
        let elsewhere = tempfile::tempdir().unwrap();

        let inside = root.path().join("docs").join("notes");
        let made = new_note(root.path(), &NotesFolder::Absolute(inside.clone()), at()).unwrap();
        assert_eq!(made, "docs/notes/note-20261123-090507.md");
        assert!(inside.join("note-20261123-090507.md").is_file());

        let outside = elsewhere.path().join("a").join("b");
        let made = new_note(root.path(), &NotesFolder::Absolute(outside.clone()), at()).unwrap();
        assert_eq!(made, format!("{}/note-20261123-090507.md", slash(&outside)));
        assert!(
            !made.contains('\\'),
            "an absolute answer uses `/` only: {made}"
        );
        assert!(outside.join("note-20261123-090507.md").is_file());

        let again = new_note(root.path(), &NotesFolder::Absolute(outside.clone()), at()).unwrap();
        assert_eq!(
            again,
            format!("{}/note-20261123-090507-2.md", slash(&outside))
        );
    }

    #[test]
    fn new_note_answers_relative_for_a_folder_spelled_differently_from_the_root() {
        // The project's root is canonical, the user's folder is not: `..` here, 8.3 names or
        // another case on Windows.
        let base = tempfile::tempdir().unwrap();
        let root = dunce::canonicalize(base.path()).unwrap();
        fs::create_dir(root.join("docs")).unwrap();
        let folder = NotesFolder::Absolute(root.join("docs").join("..").join("notes"));
        let made = new_note(&root, &folder, at()).unwrap();
        assert_eq!(made, "notes/note-20261123-090507.md");
        assert!(root.join("notes/note-20261123-090507.md").is_file());
    }

    #[test]
    fn new_note_in_a_folder_that_cannot_exist_is_an_error_that_names_the_folder() {
        let root = tempfile::tempdir().unwrap();
        let blocker = root.path().join("blocker");
        fs::write(&blocker, "a file, not a folder").unwrap();
        let folder = NotesFolder::Absolute(blocker.join("notes"));
        let (kind, message) = new_note(root.path(), &folder, at()).unwrap_err();
        assert_ne!(kind, ErrorKind::AlreadyExists);
        assert!(
            message.contains("notes folder") && message.contains("blocker"),
            "{message}"
        );
        assert!(blocker.is_file());
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
