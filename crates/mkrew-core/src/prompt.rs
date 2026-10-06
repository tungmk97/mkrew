//! Prompt library (SPEC §20B.3): `<config>/mkrew/prompts/*.md`, rendered with [`render`].
//!
//! File stem = name; an optional first line `# Title` gives the title (else the name) and is
//! not part of the body. Three samples are seeded when the directory is missing, so a user who
//! deletes them all keeps an empty library.

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
pub struct PromptError {
    pub file: PathBuf,
    pub message: String,
}

/// Result of loading the prompts dir: good prompts plus one error per unreadable file.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PromptLibrary {
    /// Sorted by name.
    pub prompts: Vec<Prompt>,
    /// Sorted by file.
    pub errors: Vec<PromptError>,
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
            lib.errors.push(PromptError {
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

fn io_error(path: &Path, e: &io::Error) -> PromptError {
    PromptError {
        file: path.to_path_buf(),
        message: e.to_string(),
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
}
