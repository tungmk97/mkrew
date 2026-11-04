//! File scanning shared by the CLI scanners: the repo root rule, linked worktrees and
//! reading skill and command files (spec 2026-11-04 §5).

use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};

use super::front_matter::{self, FrontMatter};

/// Recursion limit for `**/*.md`, so a symlink loop ends.
const MAX_DEPTH: usize = 8;

/// The folders of a project scan: `folder`, then each parent, up to and including the repo
/// root, which is the nearest of them holding `.git` (a folder for a clone, a file for a linked
/// worktree). Without a repo it is `folder` alone: the scan never climbs past a folder that is
/// not inside a repo.
pub(super) fn project_dirs(folder: &Path) -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    for dir in folder.ancestors() {
        dirs.push(dir.to_path_buf());
        if dir.join(".git").exists() {
            return dirs;
        }
    }
    vec![folder.to_path_buf()]
}

/// `folder` seen from the main checkout when its repo root is a linked worktree (its `.git` is a
/// file `gitdir: <main>/.git/worktrees/<name>` and that folder has a `commondir` file): the same
/// relative folder inside the main checkout. `None` for a normal clone, a bare repo or an
/// unreadable link.
pub(super) fn main_checkout_folder(folder: &Path) -> Option<PathBuf> {
    let root = project_dirs(folder).pop()?;
    let link = fs::read_to_string(root.join(".git")).ok()?;
    let gitdir = Path::new(link.lines().next()?.strip_prefix("gitdir:")?.trim());
    let gitdir = root.join(gitdir); // an absolute `gitdir` replaces `root`
    let common = fs::read_to_string(gitdir.join("commondir")).ok()?;
    let common = lexical(&gitdir.join(common.trim()));
    if common.file_name()? != ".git" {
        return None;
    }
    let relative = folder.strip_prefix(&root).ok()?;
    Some(common.parent()?.join(relative))
}

/// `path` with `.` and `..` resolved by text only: no symlinks are followed and nothing is read
/// from disk, so `\\?\` or other prefixes are kept as they are. A `..` at the root is dropped.
fn lexical(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for part in path.components() {
        match part {
            Component::ParentDir => {
                out.pop();
            }
            Component::CurDir => {}
            other => out.push(other),
        }
    }
    out
}

/// Whether `path` is `base` or below it, compared by text with `.` and `..` resolved; ignoring
/// case on Windows.
pub(super) fn is_within(path: &Path, base: &Path) -> bool {
    let (path, base) = (lexical(path), lexical(base));
    if cfg!(windows) {
        let fold = |p: &Path| PathBuf::from(p.to_string_lossy().to_lowercase());
        return fold(&path).starts_with(fold(&base));
    }
    path.starts_with(base)
}

/// A skill or command file, read once.
pub(super) struct Doc {
    /// A skill's folder name, or a command's path below its folder with `:` between the parts
    /// and `.md` removed (`frontend/component.md` is `frontend:component`).
    pub key: String,
    pub fm: FrontMatter,
    first_line: Option<String>,
}

impl Doc {
    /// The one-line description: `description`, else `when_to_use`, else the first non-empty
    /// body line, cut to `cut` characters plus `...` when given. Line breaks become spaces.
    pub fn description(&self, cut: Option<usize>) -> String {
        let declared = self
            .fm
            .description
            .as_ref()
            .or(self.fm.when_to_use.as_ref());
        if let Some(text) = declared {
            return text.split_whitespace().collect::<Vec<_>>().join(" ");
        }
        let line = self.first_line.as_deref().unwrap_or_default();
        match cut {
            Some(n) if line.chars().count() > n => {
                format!("{}...", line.chars().take(n).collect::<String>())
            }
            _ => line.to_owned(),
        }
    }
}

/// Reads `path`; a failure is one line in `errors` and the file is skipped.
pub(super) fn read_doc(path: &Path, key: String, errors: &mut Vec<String>) -> Option<Doc> {
    let text = match fs::read_to_string(path) {
        Ok(text) => text,
        Err(e) => {
            errors.push(format!("{}: {e}", path.display()));
            return None;
        }
    };
    Some(doc_from_text(key, &text))
}

/// The document `text` (front matter and body) under `key`.
pub(super) fn doc_from_text(key: String, text: &str) -> Doc {
    let (fm, body) = front_matter::parse(text);
    Doc {
        key,
        fm: fm.unwrap_or_default(),
        first_line: front_matter::first_line(body).map(str::to_owned),
    }
}

/// The entries of `dir` by name; a missing folder is empty, any other failure is one line in
/// `errors`.
pub(super) fn entries(dir: &Path, errors: &mut Vec<String>) -> Vec<(String, PathBuf)> {
    let read = match fs::read_dir(dir) {
        Ok(read) => read,
        Err(e) if is_absent(&e) => return Vec::new(),
        Err(e) => {
            errors.push(format!("{}: {e}", dir.display()));
            return Vec::new();
        }
    };
    let mut found = Vec::new();
    for entry in read {
        match entry {
            Ok(e) => found.push((e.file_name().to_string_lossy().into_owned(), e.path())),
            Err(e) => errors.push(format!("{}: {e}", dir.display())),
        }
    }
    found.sort();
    found
}

/// Metadata of `path` (following symlinks); a path that is gone is silent, any other failure is
/// one line in `errors`.
fn stat(path: &Path, errors: &mut Vec<String>) -> Option<fs::Metadata> {
    match fs::metadata(path) {
        Ok(meta) => Some(meta),
        Err(e) if is_absent(&e) => None,
        Err(e) => {
            errors.push(format!("{}: {e}", path.display()));
            None
        }
    }
}

fn is_absent(e: &io::Error) -> bool {
    matches!(
        e.kind(),
        io::ErrorKind::NotFound | io::ErrorKind::NotADirectory
    )
}

/// `dir/<folder>/SKILL.md` for every folder, in folder order.
pub(super) fn skill_docs(dir: &Path, errors: &mut Vec<String>) -> Vec<Doc> {
    let mut docs = Vec::new();
    for (name, path) in entries(dir, errors) {
        let file = path.join("SKILL.md");
        if stat(&file, errors).is_some_and(|m| m.is_file()) {
            docs.extend(read_doc(&file, name, errors));
        }
    }
    docs
}

/// The `*.md` files of `dir`, and with `recursive` those of its sub folders, in name order.
pub(super) fn command_docs(dir: &Path, recursive: bool, errors: &mut Vec<String>) -> Vec<Doc> {
    let mut docs = Vec::new();
    walk(dir, &mut Vec::new(), recursive, errors, &mut docs);
    docs
}

fn walk(
    dir: &Path,
    parents: &mut Vec<String>,
    recursive: bool,
    errors: &mut Vec<String>,
    docs: &mut Vec<Doc>,
) {
    for (name, path) in entries(dir, errors) {
        let Some(meta) = stat(&path, errors) else {
            continue;
        };
        if meta.is_dir() {
            if recursive && parents.len() < MAX_DEPTH {
                parents.push(name);
                walk(&path, parents, recursive, errors, docs);
                parents.pop();
            }
        } else if let Some(stem) = markdown_stem(&name).filter(|_| meta.is_file()) {
            let key = parents
                .iter()
                .map(String::as_str)
                .chain([stem])
                .collect::<Vec<_>>();
            docs.extend(read_doc(&path, key.join(":"), errors));
        }
    }
}

fn markdown_stem(name: &str) -> Option<&str> {
    let stem = name.len().checked_sub(3).filter(|&n| n > 0)?;
    (name.is_char_boundary(stem) && name[stem..].eq_ignore_ascii_case(".md")).then(|| &name[..stem])
}

#[cfg(test)]
mod tests;
