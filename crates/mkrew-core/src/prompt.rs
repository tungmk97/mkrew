//! Prompt library (SPEC §20B.3): `<config>/mkrew/prompts/*.md`, rendered with [`render`].
//!
//! File stem = name; an optional first line `# Title` gives the title (else the name) and is
//! not part of the body. Three samples are seeded when the directory is missing, so a user who
//! deletes them all keeps an empty library. The Settings → Prompts page edits the folder with
//! [`create`], [`save`], [`rename`], [`delete`] and [`read`].

use std::io;
use std::path::{Path, PathBuf};

/// Samples written by [`load_or_seed`] when the prompts dir does not exist: (file, contents).
pub const SAMPLES: [(&str, &str); 3] = [
    (
        "review.md",
        "# Review\n\
         Review the changes in {file} for bugs, missed edge cases and unclear names.\n\
         List concrete problems first (with line numbers), then smaller suggestions.\n",
    ),
    (
        "explain.md",
        "# Explain\n\
         Explain what this code from {file} does, step by step, and why it is written this way:\n\
         \n\
         {selection}\n",
    ),
    (
        "fix-tests.md",
        "# Fix failing tests\n\
         Run the tests of {project}, find out why the failing ones fail and fix the code.\n\
         Only change a test when the test itself is wrong, and say so.\n",
    ),
];

/// One prompt of the library.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Prompt {
    /// File stem.
    pub name: String,
    /// `# Title` of the first line, else the name.
    pub title: String,
    /// The template, without the title line.
    pub body: String,
}

/// A prompt file that could not be read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReadError {
    pub file: PathBuf,
    pub message: String,
}

/// Result of loading the prompts dir: good prompts plus one error per unreadable file.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PromptLibrary {
    /// Sorted by name.
    pub prompts: Vec<Prompt>,
    /// Sorted by file.
    pub errors: Vec<ReadError>,
}

/// Values of the template variables; `None` = unavailable here (renders empty, listed).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PromptVars {
    /// Current file of Files/Git, project-relative.
    pub file: Option<String>,
    /// Picked lines, already formatted as quoted text.
    pub selection: Option<String>,
    /// Task title.
    pub task: Option<String>,
    /// Project name.
    pub project: Option<String>,
}

impl PromptVars {
    /// `None` for an unknown variable name or an unavailable value.
    fn get(&self, name: &str) -> Option<&str> {
        match name {
            "file" => self.file.as_deref(),
            "selection" => self.selection.as_deref(),
            "task" => self.task.as_deref(),
            "project" => self.project.as_deref(),
            _ => None,
        }
    }
}

/// A rendered template.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Rendered {
    pub text: String,
    /// Unknown or unavailable variables (rendered empty), in order of first use, no duplicates.
    pub missing: Vec<String>,
}

/// Replaces every `{name}` (`name` = ASCII letters, digits, `_`, `-`) with its value. Unknown or
/// unavailable variables render as empty and are listed in [`Rendered::missing`]. Any other
/// brace (`{}`, `{ a }`, a lone `{`) is kept as written; code like `format!("{name}")` in a
/// prompt is therefore blanked (and listed).
pub fn render(template: &str, vars: &PromptVars) -> Rendered {
    let mut out = Rendered::default();
    let mut rest = template;
    while let Some(open) = rest.find('{') {
        out.text.push_str(&rest[..open]);
        let after = &rest[open + 1..];
        let name_len = after
            .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_' || c == '-'))
            .unwrap_or(after.len());
        if name_len == 0 || !after[name_len..].starts_with('}') {
            out.text.push('{');
            rest = after;
            continue;
        }
        let name = &after[..name_len];
        match vars.get(name) {
            Some(value) => out.text.push_str(value),
            None => {
                if !out.missing.iter().any(|m| m == name) {
                    out.missing.push(name.to_owned());
                }
            }
        }
        rest = &after[name_len + 1..];
    }
    out.text.push_str(rest);
    out
}

/// Splits an optional first line `# Title` from the body (blank lines after it are dropped too).
/// CRLF line endings become `\n` so a `\r` never reaches a terminal.
pub fn parse_prompt(name: &str, src: &str) -> Prompt {
    let src = src
        .strip_prefix('\u{feff}')
        .unwrap_or(src)
        .replace("\r\n", "\n");
    let (first, rest) = src.split_once('\n').unwrap_or((&src, ""));
    let heading = first.strip_prefix("# ").map(str::trim);
    match heading {
        Some(title) if !title.is_empty() => Prompt {
            name: name.to_owned(),
            title: title.to_owned(),
            body: rest.trim_start_matches('\n').to_owned(),
        },
        _ => Prompt {
            name: name.to_owned(),
            title: name.to_owned(),
            body: src.clone(),
        },
    }
}

/// Loads every `*.md` of `dir`. A missing dir is an empty library.
pub fn load_dir(dir: &Path) -> PromptLibrary {
    let mut lib = PromptLibrary::default();
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return lib,
        Err(e) => {
            lib.errors.push(io_error(dir, &e));
            return lib;
        }
    };
    for entry in entries {
        let path = match entry {
            Ok(entry) => entry.path(),
            Err(e) => {
                lib.errors.push(io_error(dir, &e));
                continue;
            }
        };
        let is_md = path
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("md"));
        if !is_md || !path.is_file() {
            continue;
        }
        let Some(name) = path.file_stem().and_then(|s| s.to_str()) else {
            lib.errors.push(ReadError {
                file: path.clone(),
                message: "file name is not valid UTF-8".into(),
            });
            continue;
        };
        match std::fs::read_to_string(&path) {
            Ok(src) => lib.prompts.push(parse_prompt(name, &src)),
            Err(e) => lib.errors.push(io_error(&path, &e)),
        }
    }
    lib.prompts.sort_by(|a, b| a.name.cmp(&b.name));
    lib.errors.sort_by(|a, b| a.file.cmp(&b.file));
    lib
}

/// Writes [`SAMPLES`] into `dir` if it does not exist yet, then loads it.
pub fn load_or_seed(dir: &Path) -> PromptLibrary {
    if !dir.exists()
        && let Err(e) = seed(dir)
    {
        let mut lib = load_dir(dir);
        lib.errors.push(io_error(dir, &e));
        return lib;
    }
    load_dir(dir)
}

fn seed(dir: &Path) -> io::Result<()> {
    std::fs::create_dir_all(dir)?;
    for (file, contents) in SAMPLES {
        std::fs::write(dir.join(file), contents)?;
    }
    Ok(())
}

fn io_error(path: &Path, e: &io::Error) -> ReadError {
    ReadError {
        file: path.to_path_buf(),
        message: e.to_string(),
    }
}

/// The longest prompt name, in characters.
pub const NAME_MAX: usize = 64;

/// Characters a file name cannot hold on Windows (and `/` on every system).
const FORBIDDEN: [char; 9] = ['/', '\\', ':', '*', '?', '"', '<', '>', '|'];

/// Names Windows keeps for devices, with any extension.
const RESERVED: [&str; 22] = [
    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
    "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
];

/// Why a prompt name is refused. Each message is the sentence the Prompts page shows under the
/// Name field.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum NameError {
    #[error("Give the prompt a name.")]
    Empty,
    #[error("A name has at most 64 characters.")]
    TooLong,
    #[error("A name cannot start with a dot.")]
    LeadingDot,
    #[error("A name cannot contain `{0}`.")]
    Forbidden(char),
    #[error("A name cannot contain control characters.")]
    Control,
    #[error("A name cannot end with a space or a dot.")]
    Trailing,
    #[error("\"{0}\" is a reserved name on Windows.")]
    Reserved(String),
    /// Carries the existing prompt's own spelling.
    #[error("A prompt named \"{0}\" exists already.")]
    Duplicate(String),
}

/// A prompt operation that did not happen.
#[derive(Debug, thiserror::Error)]
pub enum PromptError {
    #[error(transparent)]
    Name(#[from] NameError),
    #[error("There is no prompt named \"{0}\" any more.")]
    Missing(String),
    #[error("Cannot {action} {}: {source}", path.display())]
    Io {
        action: &'static str,
        path: PathBuf,
        source: io::Error,
    },
}

fn io_failure(action: &'static str, path: &Path, source: io::Error) -> PromptError {
    PromptError::Io {
        action,
        path: path.to_path_buf(),
        source,
    }
}

fn file(dir: &Path, name: &str) -> PathBuf {
    dir.join(format!("{name}.md"))
}

/// The rules for a name as a file stem, without looking at the folder. Every operation runs it
/// first, so a name can never leave `dir`.
fn check_format(name: &str) -> Result<(), NameError> {
    if name.trim().is_empty() {
        return Err(NameError::Empty);
    }
    if name.chars().count() > NAME_MAX {
        return Err(NameError::TooLong);
    }
    if name.starts_with('.') {
        return Err(NameError::LeadingDot);
    }
    for c in name.chars() {
        if c.is_control() {
            return Err(NameError::Control);
        }
        if FORBIDDEN.contains(&c) {
            return Err(NameError::Forbidden(c));
        }
    }
    if name.ends_with([' ', '.']) {
        return Err(NameError::Trailing);
    }
    // Windows reads `aux.v2.md` as the device `AUX`: the part before the first dot decides.
    let base = name.split('.').next().unwrap_or(name).trim_end();
    if RESERVED
        .iter()
        .any(|reserved| reserved.eq_ignore_ascii_case(base))
    {
        return Err(NameError::Reserved(name.to_owned()));
    }
    Ok(())
}

/// The stems of every `*.md` entry of `dir`; a folder that cannot be listed has none (the write
/// that follows reports the real error).
fn stems(dir: &Path) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    entries
        .filter_map(|entry| {
            let path = entry.ok()?.path();
            if !path
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("md"))
            {
                return None;
            }
            Some(path.file_stem()?.to_str()?.to_owned())
        })
        .collect()
}

/// Whether `name` can be the name of a prompt in `dir`. `renaming` is the prompt being renamed:
/// its own name is not a duplicate, so `review` may become `Review`. Names compare
/// case-insensitively; the error carries the other prompt's spelling.
pub fn validate_name(dir: &Path, name: &str, renaming: Option<&str>) -> Result<(), NameError> {
    check_format(name)?;
    let lower = name.to_lowercase();
    let taken = stems(dir)
        .into_iter()
        .find(|stem| stem.to_lowercase() == lower && Some(stem.as_str()) != renaming);
    match taken {
        Some(other) => Err(NameError::Duplicate(other)),
        None => Ok(()),
    }
}

/// Writes `text` as `dir/name.md` through a temp file in the same folder and a rename over the
/// target, so a crash never leaves half a prompt. The folder is created when missing. The temp
/// file ends in `.tmp`, which [`load_dir`] ignores.
fn write_atomic(dir: &Path, name: &str, text: &str) -> Result<(), PromptError> {
    std::fs::create_dir_all(dir).map_err(|e| io_failure("create", dir, e))?;
    let target = file(dir, name);
    let temp = dir.join(format!(".{name}.md.tmp"));
    let result = std::fs::write(&temp, text).and_then(|()| std::fs::rename(&temp, &target));
    if let Err(source) = result {
        let _ = std::fs::remove_file(&temp);
        return Err(io_failure("write", &target, source));
    }
    Ok(())
}

/// A new prompt; the name must be valid and unused. `body` is the whole file, `# Title` first
/// line included.
// ponytail: the duplicate check and the write are not one atomic step; a second writer between
// them (another Mkrew window, an editor) can be overwritten. One user, one folder; upgrade path:
// `create_new` on the final file.
pub fn create(dir: &Path, name: &str, body: &str) -> Result<(), PromptError> {
    validate_name(dir, name, None)?;
    write_atomic(dir, name, body)
}

/// Replaces the text of `name` (written again when the file went away meanwhile).
pub fn save(dir: &Path, name: &str, body: &str) -> Result<(), PromptError> {
    check_format(name)?;
    write_atomic(dir, name, body)
}

/// Renames `old` to `new`; refused when another prompt has that name in any case. A new case of
/// the same name is allowed.
pub fn rename(dir: &Path, old: &str, new: &str) -> Result<(), PromptError> {
    check_format(old)?;
    validate_name(dir, new, Some(old))?;
    if old == new {
        return Ok(());
    }
    let from = file(dir, old);
    std::fs::rename(&from, file(dir, new)).map_err(|source| {
        if source.kind() == io::ErrorKind::NotFound {
            PromptError::Missing(old.to_owned())
        } else {
            io_failure("rename", &from, source)
        }
    })
}

/// Deletes the prompt file.
pub fn delete(dir: &Path, name: &str) -> Result<(), PromptError> {
    check_format(name)?;
    let path = file(dir, name);
    std::fs::remove_file(&path).map_err(|source| {
        if source.kind() == io::ErrorKind::NotFound {
            PromptError::Missing(name.to_owned())
        } else {
            io_failure("delete", &path, source)
        }
    })
}

/// The whole file of `name` as the editor shows it: without a byte order mark, with `\n` line
/// breaks, the `# Title` line included.
pub fn read(dir: &Path, name: &str) -> Result<String, PromptError> {
    check_format(name)?;
    let path = file(dir, name);
    match std::fs::read_to_string(&path) {
        Ok(text) => Ok(text
            .strip_prefix('\u{feff}')
            .unwrap_or(&text)
            .replace("\r\n", "\n")),
        Err(source) if source.kind() == io::ErrorKind::NotFound => {
            Err(PromptError::Missing(name.to_owned()))
        }
        Err(source) => Err(io_failure("read", &path, source)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vars() -> PromptVars {
        PromptVars {
            file: Some("src/main.rs".into()),
            selection: None,
            task: Some("Fix login".into()),
            project: Some("mkrew".into()),
        }
    }

    #[test]
    fn render_substitutes_known_vars() {
        let r = render("{task} in {project}: look at {file}.", &vars());
        assert_eq!(r.text, "Fix login in mkrew: look at src/main.rs.");
        assert!(r.missing.is_empty());
    }

    #[test]
    fn render_unknown_and_unavailable_render_empty_and_are_listed_once() {
        let r = render("a{selection}b{nope}c{selection}d", &vars());
        assert_eq!(r.text, "abcd");
        assert_eq!(r.missing, ["selection", "nope"]);
    }

    #[test]
    fn render_keeps_braces_that_are_not_variables() {
        let r = render("fn x() {} { a } {a b} {open", &vars());
        assert_eq!(r.text, "fn x() {} { a } {a b} {open");
        assert!(r.missing.is_empty());
    }

    #[test]
    fn render_does_not_expand_values() {
        let vars = PromptVars {
            selection: Some("{file} {task}".into()),
            ..PromptVars::default()
        };
        let r = render("{selection}", &vars);
        assert_eq!(r.text, "{file} {task}");
        assert!(r.missing.is_empty());
    }

    #[test]
    fn render_handles_multibyte_text() {
        let r = render("é{project}ü{", &vars());
        assert_eq!(r.text, "émkrewü{");
    }

    #[test]
    fn parse_title_line() {
        let p = parse_prompt("review", "# Review it \r\n\r\nBody {file}\n");
        assert_eq!(p.title, "Review it");
        assert_eq!(p.body, "Body {file}\n");
    }

    #[test]
    fn parse_normalizes_crlf_in_the_body() {
        let p = parse_prompt("x", "line one\r\nline two\r\n");
        assert_eq!(p.body, "line one\nline two\n");
        let p = parse_prompt("x", "# T\r\nA\r\nB\r\n");
        assert_eq!(p.body, "A\nB\n");
    }

    #[test]
    fn parse_without_title_uses_the_name() {
        let p = parse_prompt("x", "Do it\n# not a title\n");
        assert_eq!(p.title, "x");
        assert_eq!(p.body, "Do it\n# not a title\n");
        let p = parse_prompt("y", "#NoSpace\n");
        assert_eq!(p.title, "y");
        assert_eq!(p.body, "#NoSpace\n");
    }

    #[test]
    fn seeds_samples_when_dir_is_missing() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("prompts");
        let lib = load_or_seed(&dir);
        assert!(lib.errors.is_empty(), "{:?}", lib.errors);
        let names: Vec<_> = lib.prompts.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(names, ["explain", "fix-tests", "review"]);
        let titles: Vec<_> = lib.prompts.iter().map(|p| p.title.as_str()).collect();
        assert_eq!(titles, ["Explain", "Fix failing tests", "Review"]);
        // Every sample only uses known variables.
        let all = PromptVars {
            file: Some("f".into()),
            selection: Some("s".into()),
            task: Some("t".into()),
            project: Some("p".into()),
        };
        for p in &lib.prompts {
            assert!(render(&p.body, &all).missing.is_empty(), "{}", p.name);
        }
    }

    #[test]
    fn does_not_reseed_an_existing_dir() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("prompts");
        std::fs::create_dir(&dir).unwrap();
        assert_eq!(load_or_seed(&dir), PromptLibrary::default());

        std::fs::write(dir.join("mine.md"), "# Mine\nhello").unwrap();
        std::fs::write(dir.join("notes.txt"), "ignored").unwrap();
        let lib = load_or_seed(&dir);
        assert_eq!(
            lib.prompts,
            [Prompt {
                name: "mine".into(),
                title: "Mine".into(),
                body: "hello".into(),
            }]
        );
    }

    #[test]
    fn missing_dir_without_seeding_is_empty() {
        let tmp = tempfile::tempdir().unwrap();
        assert_eq!(load_dir(&tmp.path().join("nope")), PromptLibrary::default());
    }

    #[test]
    fn prompts_path_that_is_a_file_is_an_error_not_a_panic() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("prompts");
        std::fs::write(&dir, "not a dir").unwrap();
        let lib = load_or_seed(&dir);
        assert!(lib.prompts.is_empty());
        assert_eq!(lib.errors.len(), 1, "{:?}", lib.errors);
        assert_eq!(lib.errors[0].file, dir);
    }

    #[test]
    fn md_directories_are_skipped() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::create_dir(tmp.path().join("x.md")).unwrap();
        assert_eq!(load_dir(tmp.path()), PromptLibrary::default());
    }

    /// A prompts folder inside a fresh tempdir.
    fn library_dir() -> (tempfile::TempDir, PathBuf) {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("prompts");
        std::fs::create_dir(&dir).unwrap();
        (tmp, dir)
    }

    /// The file names of `dir`, sorted.
    fn files(dir: &Path) -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(dir)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().into_string().unwrap())
            .collect();
        names.sort();
        names
    }

    #[test]
    fn create_writes_the_file_and_the_library_reads_it_back() {
        let (_tmp, dir) = library_dir();
        create(&dir, "mine", "# Mine\nhello {file}\n").unwrap();
        assert_eq!(files(&dir), ["mine.md"]);
        assert_eq!(
            load_dir(&dir).prompts,
            [Prompt {
                name: "mine".into(),
                title: "Mine".into(),
                body: "hello {file}\n".into(),
            }]
        );
    }

    #[test]
    fn create_and_save_make_a_missing_folder_and_validation_accepts_it() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("not").join("yet");
        assert_eq!(validate_name(&dir, "mine", None), Ok(()));
        create(&dir, "mine", "x").unwrap();
        assert_eq!(files(&dir), ["mine.md"]);
        let other = tmp.path().join("other");
        save(&other, "kept", "typed text").unwrap();
        assert_eq!(read(&other, "kept").unwrap(), "typed text");
    }

    #[test]
    fn create_refuses_a_duplicate_in_any_case_and_keeps_the_first_file() {
        let (_tmp, dir) = library_dir();
        create(&dir, "Review", "first").unwrap();
        let result = create(&dir, "review", "second");
        assert!(
            matches!(&result, Err(PromptError::Name(NameError::Duplicate(other))) if other == "Review"),
            "{result:?}"
        );
        assert_eq!(files(&dir), ["Review.md"]);
        assert_eq!(read(&dir, "Review").unwrap(), "first");
        // A folder named like a prompt counts too: creating over it could not work.
        std::fs::create_dir(dir.join("Folder.md")).unwrap();
        assert_eq!(
            validate_name(&dir, "folder", None),
            Err(NameError::Duplicate("Folder".into()))
        );
    }

    #[test]
    fn every_invalid_name_is_refused_with_its_own_reason() {
        let (_tmp, dir) = library_dir();
        let long = "a".repeat(65);
        let wide = "é".repeat(65);
        let refused: Vec<(&str, NameError)> = vec![
            ("", NameError::Empty),
            ("   ", NameError::Empty),
            (long.as_str(), NameError::TooLong),
            (wide.as_str(), NameError::TooLong),
            (".hidden", NameError::LeadingDot),
            ("..", NameError::LeadingDot),
            ("a\tb", NameError::Control),
            ("a\u{7}", NameError::Control),
            ("a ", NameError::Trailing),
            ("a.", NameError::Trailing),
            ("CON", NameError::Reserved("CON".into())),
            ("nul", NameError::Reserved("nul".into())),
            ("Com1", NameError::Reserved("Com1".into())),
            ("LPT9", NameError::Reserved("LPT9".into())),
            ("aux.v2", NameError::Reserved("aux.v2".into())),
        ];
        for (name, expected) in refused {
            assert_eq!(validate_name(&dir, name, None), Err(expected), "{name:?}");
        }
        for c in ['/', '\\', ':', '*', '?', '"', '<', '>', '|'] {
            assert_eq!(
                validate_name(&dir, &format!("a{c}b"), None),
                Err(NameError::Forbidden(c)),
                "{c}"
            );
        }
        // The limit counts characters, not bytes; near misses are fine.
        let ok_long = "a".repeat(64);
        let ok_wide = "é".repeat(64);
        for name in [
            "review",
            "Fix tests",
            "COM0",
            "console",
            "name.with.dots",
            "ünï cødé 名前",
            "-dash",
            ok_long.as_str(),
            ok_wide.as_str(),
        ] {
            assert_eq!(validate_name(&dir, name, None), Ok(()), "{name:?}");
        }
    }

    #[test]
    fn a_refused_name_writes_nothing_and_cannot_escape_the_folder() {
        let (tmp, dir) = library_dir();
        let victim = tmp.path().join("victim.md");
        std::fs::write(&victim, "keep").unwrap();
        for evil in ["../victim", "..\\victim", "sub/x", ".."] {
            assert!(
                matches!(create(&dir, evil, "x"), Err(PromptError::Name(_))),
                "{evil}"
            );
            assert!(
                matches!(save(&dir, evil, "x"), Err(PromptError::Name(_))),
                "{evil}"
            );
            assert!(
                matches!(delete(&dir, evil), Err(PromptError::Name(_))),
                "{evil}"
            );
            assert!(
                matches!(read(&dir, evil), Err(PromptError::Name(_))),
                "{evil}"
            );
            assert!(
                matches!(rename(&dir, evil, "ok"), Err(PromptError::Name(_))),
                "{evil}"
            );
            assert!(
                matches!(rename(&dir, "ok", evil), Err(PromptError::Name(_))),
                "{evil}"
            );
        }
        assert_eq!(std::fs::read_to_string(&victim).unwrap(), "keep");
        assert!(files(&dir).is_empty());
        assert_eq!(files(tmp.path()), ["prompts", "victim.md"]);
    }

    #[test]
    fn save_replaces_the_text_and_leaves_no_temp_file() {
        let (_tmp, dir) = library_dir();
        create(&dir, "x", "old").unwrap();
        save(&dir, "x", "# New\nnew text").unwrap();
        assert_eq!(read(&dir, "x").unwrap(), "# New\nnew text");
        assert_eq!(files(&dir), ["x.md"]);
        // A prompt deleted meanwhile is written again: the typed text is not lost.
        delete(&dir, "x").unwrap();
        save(&dir, "x", "typed").unwrap();
        assert_eq!(read(&dir, "x").unwrap(), "typed");
    }

    #[test]
    fn a_failed_save_leaves_neither_a_temp_file_nor_a_changed_target() {
        let (_tmp, dir) = library_dir();
        // The target is a folder: the final rename cannot succeed.
        std::fs::create_dir(dir.join("x.md")).unwrap();
        let result = save(&dir, "x", "body");
        match &result {
            Err(PromptError::Io { .. }) => {
                assert!(!result.as_ref().unwrap_err().to_string().is_empty());
            }
            other => panic!("{other:?}"),
        }
        assert_eq!(files(&dir), ["x.md"]);
    }

    #[test]
    fn rename_moves_the_file_and_keeps_its_text() {
        let (_tmp, dir) = library_dir();
        create(&dir, "one", "# One\nbody\n").unwrap();
        rename(&dir, "one", "two").unwrap();
        assert_eq!(files(&dir), ["two.md"]);
        assert_eq!(read(&dir, "two").unwrap(), "# One\nbody\n");
        rename(&dir, "two", "two").unwrap();
        assert_eq!(files(&dir), ["two.md"], "the same name is a no-op");
    }

    #[test]
    fn rename_refuses_an_existing_target_in_any_case() {
        let (_tmp, dir) = library_dir();
        create(&dir, "a", "text a").unwrap();
        create(&dir, "b", "text b").unwrap();
        let result = rename(&dir, "a", "B");
        assert!(
            matches!(&result, Err(PromptError::Name(NameError::Duplicate(other))) if other == "b"),
            "{result:?}"
        );
        assert_eq!(files(&dir), ["a.md", "b.md"]);
        assert_eq!(read(&dir, "a").unwrap(), "text a");
        assert_eq!(read(&dir, "b").unwrap(), "text b");
    }

    #[test]
    fn rename_to_a_new_case_of_its_own_name_works() {
        let (_tmp, dir) = library_dir();
        create(&dir, "review", "text").unwrap();
        // Its own name does not count as a duplicate while it is the one being renamed.
        assert_eq!(validate_name(&dir, "REVIEW", Some("review")), Ok(()));
        assert_eq!(
            validate_name(&dir, "REVIEW", None),
            Err(NameError::Duplicate("review".into()))
        );
        rename(&dir, "review", "Review").unwrap();
        assert_eq!(files(&dir), ["Review.md"]);
        assert_eq!(read(&dir, "Review").unwrap(), "text");
    }

    #[test]
    fn rename_and_delete_of_a_missing_prompt_say_so() {
        let (tmp, dir) = library_dir();
        let result = rename(&dir, "ghost", "x");
        assert!(
            matches!(&result, Err(PromptError::Missing(name)) if name == "ghost"),
            "{result:?}"
        );
        let result = delete(&dir, "ghost");
        assert!(
            matches!(&result, Err(PromptError::Missing(name)) if name == "ghost"),
            "{result:?}"
        );
        let gone = tmp.path().join("no-folder");
        assert!(matches!(delete(&gone, "x"), Err(PromptError::Missing(_))));
        assert!(matches!(
            rename(&gone, "x", "y"),
            Err(PromptError::Missing(_))
        ));
        assert!(matches!(read(&gone, "x"), Err(PromptError::Missing(_))));
    }

    #[test]
    fn delete_removes_only_that_file() {
        let (_tmp, dir) = library_dir();
        create(&dir, "a", "x").unwrap();
        create(&dir, "b", "y").unwrap();
        delete(&dir, "a").unwrap();
        assert_eq!(files(&dir), ["b.md"]);
    }

    #[test]
    fn read_gives_the_whole_file_with_unix_line_breaks() {
        let (_tmp, dir) = library_dir();
        std::fs::write(dir.join("t.md"), "\u{feff}# T\r\nA\r\n").unwrap();
        assert_eq!(read(&dir, "t").unwrap(), "# T\nA\n");
    }

    #[test]
    fn the_page_shows_the_error_sentences() {
        // One sentence pinned per error area: a name problem and a vanished prompt.
        assert_eq!(
            NameError::Duplicate("Review".into()).to_string(),
            "A prompt named \"Review\" exists already."
        );
        assert_eq!(
            PromptError::Missing("x".into()).to_string(),
            "There is no prompt named \"x\" any more."
        );
        // The other reasons only need to say something.
        for e in [
            NameError::Empty,
            NameError::TooLong,
            NameError::LeadingDot,
            NameError::Forbidden('*'),
            NameError::Control,
            NameError::Trailing,
            NameError::Reserved("CON".into()),
        ] {
            assert!(!e.to_string().is_empty(), "{e:?}");
        }
    }
}
