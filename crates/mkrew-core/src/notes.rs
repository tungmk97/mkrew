//! New note helpers (Tern panes spec 2026-11-23 section 7.2), pure: the file name of a note and
//! what `[notes] folder` means. The daemon creates the file (`fs.new_note`).

use std::path::{Component, Path, PathBuf};

use chrono::{Datelike, NaiveDateTime, Timelike};

/// What `[notes] folder` names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NotesFolder {
    /// Below the workspace root: `/`-separated, no `.` or `..` parts; `""` is the root itself.
    Relative(String),
    /// A full path, used as is for every workspace: no `..`, no `.`, no trailing separator.
    Absolute(PathBuf),
}

/// Why `[notes] folder` names no place. The text is for the user.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum FolderError {
    #[error("[notes] folder {setting:?}: ~ is not expanded; write the full path instead")]
    Tilde { setting: String },
    #[error(
        "[notes] folder {setting:?}: use a full path with its drive, or a path below the workspace root"
    )]
    HalfPath { setting: String },
    #[error("[notes] folder {setting:?} must not contain \"..\"; write the full path instead")]
    ParentDir { setting: String },
    #[error(
        "[notes] folder {setting:?} has a drive part {name:?} inside the path; write the full path instead"
    )]
    DrivePart { setting: String, name: String },
}

/// One ordinary name: alone it parses as a single normal component (`C:` does not). On Windows
/// a `C:` or `C:x` in the middle of a path reads as a name, but joining it replaces the base.
pub fn is_plain(name: &std::ffi::OsStr) -> bool {
    let mut parts = Path::new(name).components();
    matches!(parts.next(), Some(Component::Normal(_))) && parts.next().is_none()
}

/// `[notes] folder` as the user typed it. Spaces around it are trimmed; `/` and `\` both
/// separate; `.` parts and a trailing separator are dropped. `..` and a leading `~` (nothing
/// expands it) are refused, and so is a path that is neither relative nor full (`/x` or `C:x`
/// on Windows). The error's text is for the user.
pub fn folder(setting: &str) -> Result<NotesFolder, FolderError> {
    let setting = setting.trim();
    if setting.starts_with('~') {
        return Err(FolderError::Tilde {
            setting: setting.to_owned(),
        });
    }
    let path = PathBuf::from(setting.replace('\\', "/"));
    let absolute = path.is_absolute();
    let half = !absolute
        && (path.has_root()
            || path
                .components()
                .any(|part| matches!(part, Component::Prefix(_))));
    if half {
        return Err(FolderError::HalfPath {
            setting: setting.to_owned(),
        });
    }
    let mut clean = PathBuf::new();
    for part in path.components() {
        match part {
            Component::CurDir => {}
            Component::ParentDir => {
                return Err(FolderError::ParentDir {
                    setting: setting.to_owned(),
                });
            }
            Component::Normal(name) if !is_plain(name) => {
                return Err(FolderError::DrivePart {
                    setting: setting.to_owned(),
                    name: name.to_string_lossy().into_owned(),
                });
            }
            part => clean.push(part),
        }
    }
    Ok(if absolute {
        NotesFolder::Absolute(clean)
    } else {
        NotesFolder::Relative(clean.to_string_lossy().replace('\\', "/"))
    })
}

/// The file name of a note made at local time `at`: `note-YYYYMMDD-HHMMSS.md`; try 2, 3, ...
/// (the name was taken) add `-2`, `-3`, ... before `.md`.
pub fn file_name(at: NaiveDateTime, attempt: u32) -> String {
    let stamp = format!(
        "{:04}{:02}{:02}-{:02}{:02}{:02}",
        at.year(),
        at.month(),
        at.day(),
        at.hour(),
        at.minute(),
        at.second()
    );
    if attempt <= 1 {
        format!("note-{stamp}.md")
    } else {
        format!("note-{stamp}-{attempt}.md")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;
    use std::path::PathBuf;

    /// A full path on this OS, `/`-separated.
    const FULL: &str = if cfg!(windows) { "C:/notes" } else { "/notes" };

    fn relative(text: &str) -> NotesFolder {
        NotesFolder::Relative(text.into())
    }

    fn full() -> NotesFolder {
        NotesFolder::Absolute(PathBuf::from(FULL))
    }

    #[test]
    fn a_note_is_named_by_local_time_and_numbered_from_the_second_try() {
        let at = NaiveDate::from_ymd_opt(2026, 11, 23)
            .unwrap()
            .and_hms_opt(9, 5, 7)
            .unwrap();
        assert_eq!(file_name(at, 1), "note-20261123-090507.md");
        assert_eq!(file_name(at, 2), "note-20261123-090507-2.md");
        assert_eq!(file_name(at, 13), "note-20261123-090507-13.md");
    }

    #[test]
    fn empty_and_dot_mean_the_root() {
        for text in ["", "   ", ".", "./", " ./. "] {
            assert_eq!(folder(text), Ok(relative("")), "{text:?}");
        }
    }

    #[test]
    fn a_relative_folder_is_trimmed_and_uses_slashes_without_a_trailing_one() {
        for (text, want) in [
            ("notes", "notes"),
            ("notes/", "notes"),
            ("  inbox  ", "inbox"),
            ("a\\b", "a/b"),
            ("a/b\\", "a/b"),
            ("./a/./b/", "a/b"),
        ] {
            assert_eq!(folder(text), Ok(relative(want)), "{text:?}");
        }
    }

    #[test]
    fn dot_dot_is_refused_with_a_reason() {
        for text in [
            "..",
            "../x",
            "a/../b",
            "a/b/..",
            "..\\x",
            &format!("{FULL}/../x"),
        ] {
            let message = folder(text).unwrap_err().to_string();
            assert!(message.contains(".."), "{text:?}: {message}");
        }
    }

    #[cfg(windows)]
    #[test]
    fn a_drive_part_inside_the_path_is_refused() {
        // `C:` after the first part reads as a name, and joining it would replace the base.
        for text in [
            "notes/C:",
            "a/D:/x",
            "C:/notes/D:/x",
            "C:/notes/sub/D:/x",
            "a\\C:x",
        ] {
            let message = folder(text).unwrap_err().to_string();
            assert!(message.contains("drive"), "{text:?}: {message}");
        }
    }

    #[test]
    fn a_tilde_is_not_expanded_and_the_message_asks_for_the_full_path() {
        for text in ["~", "~/notes", "~\\notes", " ~/notes "] {
            let message = folder(text).unwrap_err().to_string();
            assert!(
                message.contains('~') && message.contains("full path"),
                "{text:?}: {message}"
            );
        }
    }

    #[test]
    fn a_full_path_is_trimmed_and_keeps_no_trailing_separator() {
        for text in [
            FULL.to_owned(),
            format!("{FULL}/"),
            format!("  {FULL}/  "),
            format!("{FULL}\\"),
            format!("{FULL}/./"),
        ] {
            assert_eq!(folder(&text), Ok(full()), "{text:?}");
        }
        assert_eq!(
            folder(&format!("{FULL}/a/b/")),
            Ok(NotesFolder::Absolute(PathBuf::from(format!("{FULL}/a/b"))))
        );
    }

    #[cfg(windows)]
    #[test]
    fn windows_full_paths_take_either_slash_and_half_paths_are_refused() {
        assert_eq!(
            folder("C:\\notes\\"),
            Ok(NotesFolder::Absolute(PathBuf::from("C:\\notes")))
        );
        assert_eq!(
            folder("c:/Users/me/notes/"),
            Ok(NotesFolder::Absolute(PathBuf::from("c:/Users/me/notes")))
        );
        // A root without a drive and a drive without a root name no fixed place.
        for text in ["/notes", "\\notes", "C:notes"] {
            let message = folder(text).unwrap_err().to_string();
            assert!(message.contains("full path"), "{text:?}: {message}");
        }
    }

    #[test]
    fn a_refused_folder_names_its_reason_as_a_variant() {
        assert!(matches!(folder("~/n"), Err(FolderError::Tilde { .. })));
        assert!(matches!(
            folder("a/../b"),
            Err(FolderError::ParentDir { .. })
        ));
    }
}
