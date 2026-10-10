//! Links in a terminal row (SPEC §10): `http(s)://` URLs and file paths (relative or absolute,
//! with an optional `:line` or `:line:col`) under the pointer. Pure and tested headlessly. The
//! pane reads ONE row of its cached grid while Ctrl / Cmd is held and calls [`link_at`], then
//! [`resolve`] checks that a path is a file of the pane's folder; nothing runs while output is
//! parsed or painted.

use std::path::Path;

/// Stands for the second cell of a wide char in a row of chars (one char per cell), so that a
/// column is an index.
pub const SPACER: char = '\0';

/// What a link points to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    Url(String),
    /// A path as typed (not checked yet) and its 1-based line.
    Path {
        path: String,
        line: Option<u32>,
    },
}

/// A link and the cells it covers: `start..end`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Link {
    pub start: usize,
    pub end: usize,
    pub target: Target,
}

/// Ends a link on both sides: blanks and the quotes and angle brackets that wrap one.
fn breaks(c: char) -> bool {
    c.is_whitespace() || matches!(c, '"' | '\'' | '`' | '<' | '>' | '|')
}

/// The link under cell `col` of `cells`, if any.
pub fn link_at(cells: &[char], col: usize) -> Option<Link> {
    if breaks(*cells.get(col)?) {
        return None;
    }
    let mut start = col;
    while start > 0 && !breaks(cells[start - 1]) {
        start -= 1;
    }
    let mut end = col + 1;
    while end < cells.len() && !breaks(cells[end]) {
        end += 1;
    }
    let link = url(cells, start, end).or_else(|| path(cells, start, end))?;
    (link.start <= col && col < link.end).then_some(link)
}

fn text(cells: &[char]) -> String {
    cells.iter().filter(|c| **c != SPACER).collect()
}

fn starts_with_scheme(cells: &[char]) -> bool {
    ["http://", "https://"].iter().any(|scheme| {
        cells.len() >= scheme.len()
            && (cells.iter().zip(scheme.chars()))
                .all(|(cell, expected)| cell.eq_ignore_ascii_case(&expected))
    })
}

/// The URL inside the token `start..end` (a Markdown `[label](https://…)` token holds one).
fn url(cells: &[char], start: usize, end: usize) -> Option<Link> {
    let scheme = (0..end - start).find(|&i| starts_with_scheme(&cells[start + i..end]))?;
    let start = start + scheme;
    let end = trim_end(cells, start, end);
    let text = text(&cells[start..end]);
    // Something after "://".
    (text.len() > text.find("://")? + 3).then_some(Link {
        start,
        end,
        target: Target::Url(text),
    })
}

/// The path inside the token `start..end`.
fn path(cells: &[char], start: usize, end: usize) -> Option<Link> {
    let mut start = start;
    while start < end && matches!(cells[start], '(' | '[' | '{') {
        start += 1;
    }
    // "src/a.rs(12)": a bracket ends the path.
    let end = (start..end)
        .find(|&i| matches!(cells[i], '(' | '[' | '{'))
        .unwrap_or(end);
    let end = trim_end(cells, start, end);
    let typed = text(&cells[start..end]);
    let (path, line) = split_line(&typed);
    looks_like_path(path).then(|| Link {
        start,
        end,
        target: Target::Path {
            path: path.to_owned(),
            line,
        },
    })
}

/// `end` moved left over what ends a sentence rather than a link: `.,;:!?` and a closing
/// bracket without its opening one inside the link.
fn trim_end(cells: &[char], start: usize, mut end: usize) -> usize {
    let unbalanced = |end: usize, open: char, close: char| {
        let token = &cells[start..end];
        token.iter().filter(|c| **c == close).count() > token.iter().filter(|c| **c == open).count()
    };
    while end > start {
        let drop = match cells[end - 1] {
            '.' | ',' | ';' | ':' | '!' | '?' => true,
            ')' => unbalanced(end, '(', ')'),
            ']' => unbalanced(end, '[', ']'),
            '}' => unbalanced(end, '{', '}'),
            _ => false,
        };
        if !drop {
            break;
        }
        end -= 1;
    }
    end
}

/// `text` without a trailing `:line` or `:line:col`, and the line.
fn split_line(text: &str) -> (&str, Option<u32>) {
    let Some((rest, last)) = number_suffix(text) else {
        return (text, None);
    };
    match number_suffix(rest) {
        Some((path, line)) => (path, Some(line)),
        None => (rest, Some(last)),
    }
}

fn number_suffix(text: &str) -> Option<(&str, u32)> {
    let (head, digits) = text.rsplit_once(':')?;
    if head.is_empty() || digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    Some((head, digits.parse().ok()?))
}

/// A path has a separator, or a name with an extension (`Cargo.toml`) and no more colons
/// than a drive's; `v1.2`, `1.5` and `key:value` are not paths.
fn looks_like_path(path: &str) -> bool {
    let body = match path.as_bytes() {
        [drive, b':', sep, ..] if drive.is_ascii_alphabetic() && matches!(sep, b'\\' | b'/') => {
            &path[2..]
        }
        _ => path,
    };
    if body.is_empty() || body.contains(':') {
        return false;
    }
    if body.contains(['/', '\\']) {
        return body.chars().any(char::is_alphanumeric);
    }
    body.rsplit_once('.').is_some_and(|(_, ext)| {
        (1..=8).contains(&ext.len())
            && ext.chars().all(|c| c.is_ascii_alphanumeric())
            && ext.chars().any(|c| c.is_ascii_alphabetic())
    })
}

/// The Files path (`/` separators, relative to `base`) of the file that `path` names, typed
/// in a terminal running in `base`; `None` unless it exists, is a file, and lies inside
/// `base` (a folder, a missing file or a file elsewhere is no link). Reads the disk: call it
/// when the pointer moves to another cell, never per frame.
pub fn resolve(base: &Path, path: &str) -> Option<String> {
    let typed = Path::new(path);
    let full = if typed.is_absolute() {
        typed.to_path_buf()
    } else {
        base.join(typed)
    };
    let full = full.canonicalize().ok()?;
    if !full.is_file() {
        return None;
    }
    let rel = full.strip_prefix(base.canonicalize().ok()?).ok()?;
    let parts: Vec<_> = rel
        .components()
        .map(|part| part.as_os_str().to_string_lossy())
        .collect();
    Some(parts.join("/"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chars(row: &str) -> Vec<char> {
        row.chars().collect()
    }

    /// The link under the first char of `needle` in the ASCII `row`.
    fn target(row: &str, needle: &str) -> Option<Target> {
        let col = row[..row.find(needle)?].chars().count();
        link_at(&chars(row), col).map(|link| link.target)
    }

    fn url(text: &str) -> Target {
        Target::Url(text.into())
    }

    fn path(text: &str, line: Option<u32>) -> Target {
        Target::Path {
            path: text.into(),
            line,
        }
    }

    #[test]
    fn urls_keep_their_query_and_lose_trailing_punctuation() {
        assert_eq!(
            target("see https://example.com/a?b=1&c=2#top, then", "https"),
            Some(url("https://example.com/a?b=1&c=2#top"))
        );
        assert_eq!(
            target("(https://example.com/x).", "https"),
            Some(url("https://example.com/x"))
        );
        assert_eq!(
            target("http://localhost:5173/", "http"),
            Some(url("http://localhost:5173/"))
        );
        // A bracket pair inside the URL stays.
        let wiki = "https://en.wikipedia.org/wiki/Rust_(language)";
        assert_eq!(target(wiki, "https"), Some(url(wiki)));
        // Markdown: the link is the URL, not its label.
        let row = "[docs](https://x.dev/a)";
        assert_eq!(target(row, "https"), Some(url("https://x.dev/a")));
        assert_eq!(target(row, "docs"), None);
        // The scheme alone is nothing.
        assert_eq!(target("https:// x", "https"), None);
    }

    #[test]
    fn paths_carry_an_optional_line_and_column() {
        assert_eq!(
            target("at ./src/a.rs:3:5 now", "./src"),
            Some(path("./src/a.rs", Some(3)))
        );
        assert_eq!(
            target("crates/x/src/main.rs:123", "crates"),
            Some(path("crates/x/src/main.rs", Some(123)))
        );
        assert_eq!(
            target("open Cargo.toml please", "Cargo"),
            Some(path("Cargo.toml", None))
        );
        assert_eq!(
            target("/home/u/a.rs:7", "/home"),
            Some(path("/home/u/a.rs", Some(7)))
        );
    }

    #[test]
    fn windows_paths_keep_their_drive_colon() {
        assert_eq!(
            target(r"C:\x\y.rs:12", "C:"),
            Some(path(r"C:\x\y.rs", Some(12)))
        );
        assert_eq!(
            target(r"error in C:\x\y.rs:12:5: bad", "C:"),
            Some(path(r"C:\x\y.rs", Some(12)))
        );
        assert_eq!(target("C:/x/y.rs", "C:"), Some(path("C:/x/y.rs", None)));
        // Anywhere in the token is the link.
        assert_eq!(target(r"C:\x\y.rs", "y.rs"), Some(path(r"C:\x\y.rs", None)));
    }

    #[test]
    fn quotes_brackets_and_punctuation_are_not_part_of_the_path() {
        assert_eq!(
            target(r#"read "src/a.rs" and"#, "src"),
            Some(path("src/a.rs", None))
        );
        assert_eq!(
            target("(src/a.rs:3)", "src"),
            Some(path("src/a.rs", Some(3)))
        );
        // `file(12)` is not detected as a line: the path before the bracket is the link.
        assert_eq!(
            target("src/a.rs(12): error", "src"),
            Some(path("src/a.rs", None))
        );
        assert_eq!(
            target("see `./a.rs`, or", "./a"),
            Some(path("./a.rs", None))
        );
        assert_eq!(target("[src/a.rs]", "src"), Some(path("src/a.rs", None)));
        assert_eq!(target("in src/a.rs.", "src"), Some(path("src/a.rs", None)));
    }

    #[test]
    fn a_link_covers_its_cells_wherever_the_pointer_is() {
        let row = chars("see src/a.rs:3 now");
        let expected = Link {
            start: 4,
            end: 14,
            target: path("src/a.rs", Some(3)),
        };
        for col in 4..14 {
            assert_eq!(link_at(&row, col), Some(expected.clone()), "col {col}");
        }
        for col in [0, 2, 3, 14, 15, 17, 40] {
            assert_eq!(link_at(&row, col), None, "col {col}");
        }
    }

    #[test]
    fn words_numbers_and_flags_are_not_links() {
        for row in [
            "hello",
            "v1.2",
            "1.5",
            "10:30:45",
            "--flag",
            "key:value",
            "a-b",
            "...",
            ".",
        ] {
            assert_eq!(link_at(&chars(row), 0), None, "{row}");
        }
    }

    #[test]
    fn wide_chars_take_two_cells_and_none_of_the_text() {
        // "日本 src/a.rs": every wide char is followed by its spacer cell.
        let row = [
            '日', SPACER, '本', SPACER, ' ', 's', 'r', 'c', '/', 'a', '.', 'r', 's',
        ];
        assert_eq!(
            link_at(&row, 11),
            Some(Link {
                start: 5,
                end: 13,
                target: path("src/a.rs", None)
            })
        );
        let name = ['日', SPACER, '.', 'r', 's'];
        assert_eq!(
            link_at(&name, 0),
            Some(Link {
                start: 0,
                end: 5,
                target: path("日.rs", None)
            })
        );
        // The spacer of a wide char is the char.
        assert_eq!(link_at(&name, 1), link_at(&name, 0));
    }

    #[test]
    fn only_existing_files_inside_the_folder_resolve() {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path().join("proj");
        std::fs::create_dir_all(base.join("src")).unwrap();
        std::fs::write(base.join("src").join("a.rs"), "x").unwrap();
        std::fs::write(dir.path().join("outside.txt"), "x").unwrap();
        assert_eq!(resolve(&base, "src/a.rs").as_deref(), Some("src/a.rs"));
        assert_eq!(
            resolve(&base, "./src/../src/a.rs").as_deref(),
            Some("src/a.rs")
        );
        let absolute = base.join("src").join("a.rs");
        assert_eq!(
            resolve(&base, absolute.to_str().unwrap()).as_deref(),
            Some("src/a.rs")
        );
        // A folder, a missing file and a file outside the folder are not links.
        assert_eq!(resolve(&base, "src"), None);
        assert_eq!(resolve(&base, "src/missing.rs"), None);
        assert_eq!(resolve(&base, "../outside.txt"), None);
        let outside = dir.path().join("outside.txt");
        assert_eq!(resolve(&base, outside.to_str().unwrap()), None);
    }
}
