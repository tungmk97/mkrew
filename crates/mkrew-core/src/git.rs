//! Pure parsers for `git` output and the hunk patch for `git apply --cached` (SPEC §12B).
//!
//! The daemon runs the commands; everything here only turns bytes into protocol types:
//! - [`parse_status`]: `git status --porcelain=v2 --branch -z`;
//! - [`parse_changes`]: `git diff --raw --numstat -z` (changed-file lists with +/− counts);
//! - [`auto_base`]: the auto-detected Git view base from `git for-each-ref`;
//! - [`parse_log`]: `git log -z --format=<LOG_FORMAT>` (history);
//! - [`parse_diff`]: `git diff` output with the default `a/`/`b/` prefixes (the daemon passes
//!   `--src-prefix=a/ --dst-prefix=b/` so a user's `diff.noprefix` cannot change it);
//! - [`hunk_patch`]: one hunk as a patch. Staging applies it with `git apply --cached`,
//!   unstaging applies the same patch (built from the staged diff) with `--cached -R`.

use mkrew_protocol::{
    ChangeKind, ChangedFile, Commit, DiffLine, DiffLineKind, FileDiff, GitStatus, Hunk,
};

/// Parses `git status --porcelain=v2 --branch -z` into the header and the
/// staged/unstaged/untracked lists of a [`GitStatus`]. Base fields stay at their defaults;
/// `added`/`removed` are `None` (status has no line counts). Ignored (`!`) entries are skipped.
pub fn parse_status(output: &[u8]) -> GitStatus {
    let mut status = GitStatus::default();
    let text = String::from_utf8_lossy(output);
    let mut records = text.split('\0');
    while let Some(record) = records.next() {
        if let Some(header) = record.strip_prefix("# ") {
            branch_header(&mut status, header);
        } else if let Some(rest) = record.strip_prefix("1 ") {
            // `1 XY sub mH mI mW hH hI path`
            if let Some((xy, path)) = fields(rest, 7) {
                changed(&mut status, xy, path, None);
            }
        } else if let Some(rest) = record.strip_prefix("2 ") {
            // `2 XY sub mH mI mW hH hI Xscore path`, then `origPath` as its own record.
            if let Some((xy, path)) = fields(rest, 8) {
                let old_path = records.next().map(str::to_owned);
                changed(&mut status, xy, path, old_path);
            }
        } else if let Some(rest) = record.strip_prefix("u ") {
            // `u XY sub m1 m2 m3 mW h1 h2 h3 path`
            if let Some((_, path)) = fields(rest, 9) {
                status
                    .unstaged
                    .push(file(path, None, ChangeKind::Conflicted));
            }
        } else if let Some(path) = record.strip_prefix("? ") {
            status
                .untracked
                .push(file(path, None, ChangeKind::Untracked));
        }
    }
    status
}

fn branch_header(status: &mut GitStatus, header: &str) {
    let Some((key, value)) = header.split_once(' ') else {
        return;
    };
    match key {
        "branch.oid" if value != "(initial)" => status.head = Some(value.to_owned()),
        "branch.head" if value != "(detached)" => status.branch = Some(value.to_owned()),
        "branch.upstream" => status.upstream = Some(value.to_owned()),
        "branch.ab" => {
            for part in value.split(' ') {
                if let Some(n) = part.strip_prefix('+') {
                    status.ahead = n.parse().unwrap_or(0);
                } else if let Some(n) = part.strip_prefix('-') {
                    status.behind = n.parse().unwrap_or(0);
                }
            }
        }
        _ => {}
    }
}

/// Splits the `count` space-separated fields before the path off `rest`; returns the first
/// one (`XY`) and the path (which may contain spaces).
fn fields(rest: &str, count: usize) -> Option<(&str, &str)> {
    let mut parts = rest.splitn(count + 1, ' ');
    let xy = parts.next()?;
    let path = parts.nth(count - 1)?;
    (xy.len() == 2 && !path.is_empty()).then_some((xy, path))
}

fn changed(status: &mut GitStatus, xy: &str, path: &str, old_path: Option<String>) {
    let mut codes = xy.chars();
    let (x, y) = (codes.next(), codes.next());
    if let Some(kind) = x.and_then(kind) {
        let old = matches!(kind, ChangeKind::Renamed | ChangeKind::Copied)
            .then(|| old_path.clone())
            .flatten();
        status.staged.push(file(path, old, kind));
    }
    if let Some(kind) = y.and_then(kind) {
        let old = matches!(kind, ChangeKind::Renamed | ChangeKind::Copied)
            .then_some(old_path)
            .flatten();
        status.unstaged.push(file(path, old, kind));
    }
}

fn kind(code: char) -> Option<ChangeKind> {
    Some(match code {
        'M' => ChangeKind::Modified,
        'T' => ChangeKind::TypeChanged,
        'A' => ChangeKind::Added,
        'D' => ChangeKind::Deleted,
        'R' => ChangeKind::Renamed,
        'C' => ChangeKind::Copied,
        'U' => ChangeKind::Conflicted,
        _ => return None,
    })
}

fn file(path: &str, old_path: Option<String>, kind: ChangeKind) -> ChangedFile {
    ChangedFile {
        path: path.to_owned(),
        old_path,
        kind,
        added: None,
        removed: None,
    }
}

/// Parses `git diff --raw --numstat -z` into the changed files with their line counts, in
/// output order. Counts stay `None` for binary files. Git prints every raw record first
/// (`:mode mode oid oid STATUS\0path\0`, renames/copies `R100\0old\0new\0`), then every
/// numstat record (`added\tremoved\tpath\0`, renames `added\tremoved\t\0old\0new\0`).
pub fn parse_changes(output: &[u8]) -> Vec<ChangedFile> {
    let text = String::from_utf8_lossy(output);
    let mut tokens = text.split('\0');
    let mut files: Vec<ChangedFile> = Vec::new();
    let mut index = std::collections::HashMap::new();
    while let Some(token) = tokens.next() {
        if token.is_empty() {
            continue;
        }
        if let Some(raw) = token.strip_prefix(':') {
            let code = raw.rsplit(' ').next().and_then(|s| s.chars().next());
            let Some(first) = tokens.next() else { break };
            let (old_path, path) = match code {
                Some('R' | 'C') => (Some(first.to_owned()), tokens.next().unwrap_or_default()),
                _ => (None, first),
            };
            let kind = code.and_then(kind).unwrap_or(ChangeKind::Modified);
            index.insert(path.to_owned(), files.len());
            files.push(file(path, old_path, kind));
        } else {
            let mut parts = token.splitn(3, '\t');
            let added = parts.next().and_then(|n| n.parse().ok());
            let removed = parts.next().and_then(|n| n.parse().ok());
            let mut path = parts.next().unwrap_or_default();
            if path.is_empty() {
                tokens.next();
                path = tokens.next().unwrap_or_default();
            }
            if let Some(&i) = index.get(path) {
                files[i].added = added;
                files[i].removed = removed;
            }
        }
    }
    files
}

/// Picks the auto-detected base (SPEC §12B: `origin/HEAD` → `main` → `master`) from
/// `git for-each-ref --format=%(refname)%09%(objectname)%09%(symref) refs/remotes/origin/HEAD
/// refs/heads/main refs/heads/master`. Returns the base name shown to the user (`origin/HEAD`
/// as its target, e.g. `origin/main`) and its commit.
pub fn auto_base(output: &str) -> Option<(String, String)> {
    let refs: Vec<(&str, &str, &str)> = output
        .lines()
        .filter_map(|line| {
            let mut parts = line.split('\t');
            Some((parts.next()?, parts.next()?, parts.next().unwrap_or("")))
        })
        .collect();
    let find = |name: &str| {
        refs.iter()
            .find(|(refname, oid, _)| *refname == name && !oid.is_empty())
    };
    if let Some((_, oid, symref)) = find("refs/remotes/origin/HEAD") {
        let name = symref
            .strip_prefix("refs/remotes/")
            .unwrap_or("origin/HEAD");
        return Some((name.to_owned(), (*oid).to_owned()));
    }
    ["main", "master"].into_iter().find_map(|branch| {
        find(&format!("refs/heads/{branch}"))
            .map(|(_, oid, _)| (branch.to_owned(), (*oid).to_owned()))
    })
}

/// The `git log --format` that [`parse_log`] reads (with `-z`): fields split by `0x1f`, the raw
/// message last so it may hold anything but NUL.
pub const LOG_FORMAT: &str = "%H%x1f%P%x1f%an%x1f%ae%x1f%at%x1f%B";

/// Parses `git log -z --format=<LOG_FORMAT>` into commits, in output order. Records with
/// missing fields are skipped; the summary is the message's first line, the body the rest
/// without surrounding blank lines.
pub fn parse_log(output: &[u8]) -> Vec<Commit> {
    String::from_utf8_lossy(output)
        .split('\0')
        .filter_map(|record| {
            let mut fields = record.trim_start_matches('\n').splitn(6, '\x1f');
            let hash = fields.next()?.to_owned();
            let parents = fields.next()?.split(' ').filter(|p| !p.is_empty());
            let parents = parents.map(str::to_owned).collect();
            let author_name = fields.next()?.to_owned();
            let author_email = fields.next()?.to_owned();
            let seconds: i64 = fields.next()?.parse().ok()?;
            let message = fields.next()?;
            let (summary, body) = message.split_once('\n').unwrap_or((message, ""));
            Some(Commit {
                hash,
                parents,
                author_name,
                author_email,
                authored_at: seconds.saturating_mul(1000),
                summary: summary.trim_end().to_owned(),
                body: body.trim_matches(['\n', '\r']).trim_end().to_owned(),
            })
        })
        .collect()
}

/// Parses `git diff` output into one [`FileDiff`] per file, in output order. Lenient:
/// unknown lines and combined (`diff --cc`, merge conflict) sections are skipped. `too_large`
/// is never set here. A deleted file's `path` is its old path. The caller decodes the output
/// (lossy for non-UTF-8 content, whose hunk patches then fail to apply).
pub fn parse_diff(output: &str) -> Vec<FileDiff> {
    let mut files: Vec<FileDiff> = Vec::new();
    // False inside a section that is not `diff --git` (or before the first one).
    let mut in_file = false;
    let mut lines = output.split_inclusive('\n').peekable();
    while let Some(raw) = lines.next() {
        let line = raw.trim_end_matches('\n').trim_end_matches('\r');
        if let Some(names) = line.strip_prefix("diff --git ") {
            let (old, new) = header_paths(names).unwrap_or_default();
            files.push(FileDiff {
                path: new,
                old_path: Some(old),
                ..FileDiff::default()
            });
            in_file = true;
            continue;
        }
        if line.starts_with("diff ") {
            in_file = false;
        }
        let (true, Some(diff)) = (in_file, files.last_mut()) else {
            continue;
        };
        if let Some(path) = line.strip_prefix("rename from ") {
            diff.old_path = Some(unquote(path));
        } else if let Some(path) = line.strip_prefix("rename to ") {
            diff.path = unquote(path);
        } else if let Some(path) = line.strip_prefix("--- ") {
            if let Some(path) = side_path(path, "a/") {
                diff.old_path = Some(path);
            }
        } else if let Some(path) = line.strip_prefix("+++ ") {
            match side_path(path, "b/") {
                Some(path) => diff.path = path,
                // Deleted file: keep the old path as the file's path.
                None => diff.path = diff.old_path.clone().unwrap_or_default(),
            }
        } else if line.starts_with("Binary files ") || line == "GIT binary patch" {
            diff.binary = true;
        } else if line.starts_with("@@ ")
            && let Some(hunk) = hunk(line, &mut lines)
        {
            diff.added += count(&hunk, DiffLineKind::Add);
            diff.removed += count(&hunk, DiffLineKind::Del);
            diff.hunks.push(hunk);
        }
    }
    for diff in &mut files {
        if diff.old_path.as_deref() == Some(diff.path.as_str()) {
            diff.old_path = None;
        }
    }
    files
}

fn count(hunk: &Hunk, kind: DiffLineKind) -> u32 {
    hunk.lines.iter().filter(|line| line.kind == kind).count() as u32
}

/// Reads one hunk body; stops once the header's line counts are used up (plus a trailing
/// `\ No newline at end of file`).
fn hunk<'a>(
    header: &str,
    lines: &mut std::iter::Peekable<impl Iterator<Item = &'a str>>,
) -> Option<Hunk> {
    let (old_start, old_lines, new_start, new_lines, title) = hunk_header(header)?;
    let mut hunk = Hunk {
        old_start,
        old_lines,
        new_start,
        new_lines,
        header: title.to_owned(),
        lines: Vec::new(),
    };
    let (mut old, mut new) = (old_lines, new_lines);
    let (mut old_line, mut new_line) = (old_start, new_start);
    while let Some(raw) = lines.peek() {
        let body = raw.strip_suffix('\n').unwrap_or(raw);
        if body.starts_with('\\') {
            if let Some(last) = hunk.lines.last_mut() {
                last.no_newline = true;
            }
            lines.next();
            continue;
        }
        if old == 0 && new == 0 {
            break;
        }
        let (kind, text) = match body.chars().next() {
            Some('+') if new > 0 => (DiffLineKind::Add, &body[1..]),
            Some('-') if old > 0 => (DiffLineKind::Del, &body[1..]),
            Some(' ') if old > 0 && new > 0 => (DiffLineKind::Context, &body[1..]),
            // Some tools drop the space of an empty context line.
            None if old > 0 && new > 0 => (DiffLineKind::Context, ""),
            _ => break,
        };
        let (text, crlf) = match text.strip_suffix('\r') {
            Some(text) => (text, true),
            None => (text, false),
        };
        let (old_no, new_no) = match kind {
            DiffLineKind::Add => (None, Some(new_line)),
            DiffLineKind::Del => (Some(old_line), None),
            DiffLineKind::Context => (Some(old_line), Some(new_line)),
        };
        if old_no.is_some() {
            old -= 1;
            old_line += 1;
        }
        if new_no.is_some() {
            new -= 1;
            new_line += 1;
        }
        hunk.lines.push(DiffLine {
            kind,
            text: text.to_owned(),
            old_line: old_no,
            new_line: new_no,
            no_newline: false,
            crlf,
        });
        lines.next();
    }
    Some(hunk)
}

/// `@@ -a[,b] +c[,d] @@[ title]` → (a, b, c, d, title); a missing count means 1.
fn hunk_header(line: &str) -> Option<(u32, u32, u32, u32, &str)> {
    let rest = line.strip_prefix("@@ -")?;
    let (ranges, title) = rest.split_once(" @@")?;
    let (old, new) = ranges.split_once(" +")?;
    let range = |range: &str| -> Option<(u32, u32)> {
        match range.split_once(',') {
            Some((start, count)) => Some((start.parse().ok()?, count.parse().ok()?)),
            None => Some((range.parse().ok()?, 1)),
        }
    };
    let (old_start, old_lines) = range(old)?;
    let (new_start, new_lines) = range(new)?;
    Some((
        old_start,
        old_lines,
        new_start,
        new_lines,
        title.strip_prefix(' ').unwrap_or(title),
    ))
}

/// The two paths of `diff --git a/<old> b/<new>`. Unquoted names with spaces are ambiguous;
/// equal names (no rename) are split in the middle, otherwise at the first ` b/`. Renames are
/// corrected later by `rename from`/`rename to` or the `---`/`+++` lines.
fn header_paths(names: &str) -> Option<(String, String)> {
    if names.starts_with('"') {
        let (old, rest) = quoted(names)?;
        let rest = rest.strip_prefix(' ')?;
        let new = if rest.starts_with('"') {
            quoted(rest)?.0
        } else {
            rest.to_owned()
        };
        return Some((
            old.strip_prefix("a/")?.into(),
            new.strip_prefix("b/")?.into(),
        ));
    }
    let names = names.strip_prefix("a/")?;
    if let Some(at) = names.find(" \"b/") {
        let new = quoted(&names[at + 1..])?.0;
        return Some((names[..at].to_owned(), new.strip_prefix("b/")?.to_owned()));
    }
    // `<name> b/<name>`: the name is (len - 3) / 2 bytes long.
    let half = names.len().saturating_sub(3) / 2;
    if names.len() == 2 * half + 3
        && names.is_char_boundary(half)
        && names[half..].starts_with(" b/")
        && names[..half] == names[half + 3..]
    {
        return Some((names[..half].to_owned(), names[half + 3..].to_owned()));
    }
    let at = names.find(" b/")?;
    Some((names[..at].to_owned(), names[at + 3..].to_owned()))
}

/// The path of a `---`/`+++` line without its prefix; `None` for `/dev/null`.
fn side_path(path: &str, prefix: &str) -> Option<String> {
    // Git appends a tab after names that contain spaces.
    let path = unquote(path.strip_suffix('\t').unwrap_or(path));
    path.strip_prefix(prefix).map(str::to_owned)
}

fn unquote(path: &str) -> String {
    match quoted(path) {
        Some((path, "")) => path,
        _ => path.to_owned(),
    }
}

/// Parses a C-style quoted name at the start of `text` (git's `core.quotePath` output) →
/// (name, rest after the closing quote).
fn quoted(text: &str) -> Option<(String, &str)> {
    let body = text.strip_prefix('"')?;
    let mut bytes = Vec::new();
    let mut chars = body.char_indices();
    while let Some((at, c)) = chars.next() {
        match c {
            '"' => {
                return Some((
                    String::from_utf8_lossy(&bytes).into_owned(),
                    &body[at + 1..],
                ));
            }
            '\\' => {
                let (_, escape) = chars.next()?;
                let byte = match escape {
                    'a' => 7,
                    'b' => 8,
                    't' => b'\t',
                    'n' => b'\n',
                    'v' => 11,
                    'f' => 12,
                    'r' => b'\r',
                    '0'..='3' => {
                        let (_, b) = chars.next()?;
                        let (_, c) = chars.next()?;
                        let octal: String = [escape, b, c].iter().collect();
                        u8::from_str_radix(&octal, 8).ok()?
                    }
                    other => {
                        let mut buf = [0; 4];
                        bytes.extend_from_slice(other.encode_utf8(&mut buf).as_bytes());
                        continue;
                    }
                };
                bytes.push(byte);
            }
            _ => {
                let mut buf = [0; 4];
                bytes.extend_from_slice(c.encode_utf8(&mut buf).as_bytes());
            }
        }
    }
    None
}

/// One hunk of `path` as a patch for `git apply --cached` (stage) or `git apply --cached -R`
/// (unstage, with a hunk of the staged diff). Line endings (`crlf`) and missing final newlines
/// are reproduced exactly. Renames are not encoded: both sides name `path`.
pub fn hunk_patch(path: &str, hunk: &Hunk) -> String {
    let name = quote(path);
    let mut patch = format!(
        "diff --git {a} {b}\n--- {a}\n+++ {b}\n@@ -{},{} +{},{} @@\n",
        hunk.old_start,
        hunk.old_lines,
        hunk.new_start,
        hunk.new_lines,
        a = prefixed("a/", &name),
        b = prefixed("b/", &name),
    );
    for line in &hunk.lines {
        patch.push(match line.kind {
            DiffLineKind::Context => ' ',
            DiffLineKind::Add => '+',
            DiffLineKind::Del => '-',
        });
        patch.push_str(&line.text);
        if line.crlf {
            patch.push('\r');
        }
        patch.push('\n');
        if line.no_newline {
            patch.push_str("\\ No newline at end of file\n");
        }
    }
    patch
}

/// `prefix` + name, keeping a quoted name quoted (`"a/x\ty"`).
fn prefixed(prefix: &str, name: &str) -> String {
    match name.strip_prefix('"') {
        Some(rest) => format!("\"{prefix}{rest}"),
        None => format!("{prefix}{name}"),
    }
}

/// C-style quotes `path` when git would not read it back verbatim.
fn quote(path: &str) -> String {
    let needs = path
        .chars()
        .any(|c| c == '"' || c == '\\' || c.is_control())
        || path.starts_with(' ')
        || path.ends_with(' ');
    if !needs {
        return path.to_owned();
    }
    let mut out = String::from("\"");
    for c in path.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\t' => out.push_str("\\t"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            // Git reads octal escapes as bytes.
            c if c.is_control() => {
                let mut buf = [0; 4];
                for byte in c.encode_utf8(&mut buf).bytes() {
                    out.push_str(&format!("\\{byte:03o}"));
                }
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(path: &str, kind: ChangeKind) -> ChangedFile {
        file(path, None, kind)
    }

    #[test]
    fn log_records_split_message_into_summary_and_body() {
        let output = [
            "aaa\x1fbbb ccc\x1fAn Name\x1fa@x\x1f1700000000\x1fMerge it\n\nWhy:\r\n  because\n\n",
            "bbb\x1f\x1fB\x1fb@x\x1f1\x1fsubject only\n",
            "broken\x1fno fields",
            "",
        ]
        .join("\0");
        let commits = parse_log(output.as_bytes());
        assert_eq!(commits.len(), 2);
        assert_eq!(commits[0].hash, "aaa");
        assert_eq!(commits[0].parents, ["bbb", "ccc"]);
        assert_eq!(
            (
                commits[0].author_name.as_str(),
                commits[0].author_email.as_str()
            ),
            ("An Name", "a@x")
        );
        assert_eq!(commits[0].authored_at, 1_700_000_000_000);
        assert_eq!(commits[0].summary, "Merge it");
        assert_eq!(commits[0].body, "Why:\r\n  because");
        assert!(commits[1].parents.is_empty());
        assert_eq!(
            (commits[1].summary.as_str(), commits[1].body.as_str()),
            ("subject only", "")
        );
    }

    #[test]
    fn status_header_and_entries() {
        let output = [
            "# branch.oid 1234abcd",
            "# branch.head feature/x",
            "# branch.upstream origin/feature/x",
            "# branch.ab +3 -1",
            "1 M. N... 100644 100644 100644 aaa bbb src/staged.rs",
            "1 .M N... 100644 100644 100644 aaa aaa src/with space.rs",
            "1 MM N... 100644 100644 100644 aaa bbb both.rs",
            "1 A. N... 000000 100644 100644 000 bbb added.rs",
            "1 .D N... 100644 100644 000000 aaa aaa gone.rs",
            "1 .A N... 000000 000000 100644 000 000 intent.rs",
            "2 R. N... 100644 100644 100644 aaa aaa R100 new name.rs",
            "old name.rs",
            "u UU N... 100644 100644 100644 100644 a b c conflict.rs",
            "? untracked file.txt",
            "! ignored.log",
            "",
        ]
        .join("\0");
        let status = parse_status(output.as_bytes());
        assert_eq!(status.head.as_deref(), Some("1234abcd"));
        assert_eq!(status.branch.as_deref(), Some("feature/x"));
        assert_eq!(status.upstream.as_deref(), Some("origin/feature/x"));
        assert_eq!((status.ahead, status.behind), (3, 1));
        assert_eq!(
            status.staged,
            vec![
                entry("src/staged.rs", ChangeKind::Modified),
                entry("both.rs", ChangeKind::Modified),
                entry("added.rs", ChangeKind::Added),
                file(
                    "new name.rs",
                    Some("old name.rs".into()),
                    ChangeKind::Renamed
                ),
            ]
        );
        assert_eq!(
            status.unstaged,
            vec![
                entry("src/with space.rs", ChangeKind::Modified),
                entry("both.rs", ChangeKind::Modified),
                entry("gone.rs", ChangeKind::Deleted),
                entry("intent.rs", ChangeKind::Added),
                entry("conflict.rs", ChangeKind::Conflicted),
            ]
        );
        assert_eq!(
            status.untracked,
            vec![entry("untracked file.txt", ChangeKind::Untracked)]
        );
        assert!(status.against_base.is_empty() && status.base_ref.is_none());
    }

    #[test]
    fn status_of_a_fresh_or_detached_repo() {
        let fresh = parse_status(b"# branch.oid (initial)\0# branch.head main\0");
        assert_eq!((fresh.head, fresh.branch.as_deref()), (None, Some("main")));
        assert_eq!((fresh.upstream, fresh.ahead, fresh.behind), (None, 0, 0));
        let detached = parse_status(b"# branch.oid abc\0# branch.head (detached)\0");
        assert_eq!(
            (detached.head.as_deref(), detached.branch),
            (Some("abc"), None)
        );
        assert_eq!(parse_status(b""), GitStatus::default());
    }

    const MODIFIED: &str = "\
diff --git a/src/lib.rs b/src/lib.rs
index 1111111..2222222 100644
--- a/src/lib.rs
+++ b/src/lib.rs
@@ -1,4 +1,5 @@ fn main() {
 one
-two
+deux
+zwei
 three

@@ -10 +11,0 @@ impl X
-ten
";

    #[test]
    fn diff_hunks_with_line_numbers_and_counts() {
        let files = parse_diff(MODIFIED);
        assert_eq!(files.len(), 1);
        let diff = &files[0];
        assert_eq!(
            (diff.path.as_str(), diff.old_path.as_deref()),
            ("src/lib.rs", None)
        );
        assert_eq!((diff.added, diff.removed, diff.binary), (2, 2, false));
        assert_eq!(diff.hunks.len(), 2);
        let first = &diff.hunks[0];
        assert_eq!(
            (
                first.old_start,
                first.old_lines,
                first.new_start,
                first.new_lines
            ),
            (1, 4, 1, 5)
        );
        assert_eq!(first.header, "fn main() {");
        let rows: Vec<_> = first
            .lines
            .iter()
            .map(|l| (l.kind, l.text.as_str(), l.old_line, l.new_line))
            .collect();
        assert_eq!(
            rows,
            vec![
                (DiffLineKind::Context, "one", Some(1), Some(1)),
                (DiffLineKind::Del, "two", Some(2), None),
                (DiffLineKind::Add, "deux", None, Some(2)),
                (DiffLineKind::Add, "zwei", None, Some(3)),
                (DiffLineKind::Context, "three", Some(3), Some(4)),
                (DiffLineKind::Context, "", Some(4), Some(5)),
            ]
        );
        let second = &diff.hunks[1];
        assert_eq!(
            (
                second.old_start,
                second.old_lines,
                second.new_start,
                second.new_lines
            ),
            (10, 1, 11, 0)
        );
        assert_eq!(second.lines[0].old_line, Some(10));
    }

    #[test]
    fn diff_renames_new_deleted_and_binary_files() {
        let output = "\
diff --git a/old name.rs b/new name.rs
similarity index 90%
rename from old name.rs
rename to new name.rs
index 1..2 100644
--- a/old name.rs\t
+++ b/new name.rs\t
@@ -1 +1 @@
-a
+b
diff --git a/moved.txt b/dir/moved.txt
similarity index 100%
rename from moved.txt
rename to dir/moved.txt
diff --git a/new.rs b/new.rs
new file mode 100644
index 0000000..1
--- /dev/null
+++ b/new.rs
@@ -0,0 +1,2 @@
+x
+y
diff --git a/gone.rs b/gone.rs
deleted file mode 100644
--- a/gone.rs
+++ /dev/null
@@ -1 +0,0 @@
-z
diff --git a/img.png b/img.png
new file mode 100644
index 0000000..3
Binary files /dev/null and b/img.png differ
diff --git \"a/t\\303\\251st\\tx.rs\" \"b/t\\303\\251st\\tx.rs\"
--- \"a/t\\303\\251st\\tx.rs\"
+++ \"b/t\\303\\251st\\tx.rs\"
@@ -1 +1 @@
-1
+2
";
        let files = parse_diff(output);
        let summary: Vec<_> = files
            .iter()
            .map(|f| {
                (
                    f.path.as_str(),
                    f.old_path.as_deref(),
                    f.added,
                    f.removed,
                    f.binary,
                    f.hunks.len(),
                )
            })
            .collect();
        assert_eq!(
            summary,
            vec![
                ("new name.rs", Some("old name.rs"), 1, 1, false, 1),
                ("dir/moved.txt", Some("moved.txt"), 0, 0, false, 0),
                ("new.rs", None, 2, 0, false, 1),
                ("gone.rs", None, 0, 1, false, 1),
                ("img.png", None, 0, 0, true, 0),
                ("tést\tx.rs", None, 1, 1, false, 1),
            ]
        );
    }

    #[test]
    fn diff_keeps_crlf_and_missing_final_newline() {
        let output = "diff --git a/w.txt b/w.txt\n--- a/w.txt\n+++ b/w.txt\n\
@@ -1,2 +1,2 @@\n a\r\n-b\r\n\\ No newline at end of file\n+c\r\n+d\n\\ No newline at end of file\n";
        let files = parse_diff(output);
        let hunk = &files[0].hunks[0];
        // `+d` would exceed `+1,2`: the header counts end the hunk at `+c`.
        let rows: Vec<_> = hunk
            .lines
            .iter()
            .map(|l| (l.kind, l.text.as_str(), l.crlf, l.no_newline))
            .collect();
        assert_eq!(
            rows,
            vec![
                (DiffLineKind::Context, "a", true, false),
                (DiffLineKind::Del, "b", true, true),
                (DiffLineKind::Add, "c", true, false),
            ]
        );
    }

    #[test]
    fn hunk_patch_reproduces_the_diff_text() {
        let output = "diff --git a/w.txt b/w.txt\n--- a/w.txt\n+++ b/w.txt\n\
@@ -1,2 +1,2 @@ title\n a\r\n-b\n\\ No newline at end of file\n+c\n\\ No newline at end of file\n";
        let hunk = &parse_diff(output)[0].hunks[0];
        assert_eq!(
            hunk_patch("w.txt", hunk),
            "diff --git a/w.txt b/w.txt\n--- a/w.txt\n+++ b/w.txt\n@@ -1,2 +1,2 @@\n a\r\n-b\n\
\\ No newline at end of file\n+c\n\\ No newline at end of file\n"
        );
        let hunk = &parse_diff(MODIFIED)[0].hunks[1];
        let patch = hunk_patch("dir/a \"b\".rs", hunk);
        assert!(patch.starts_with(
            "diff --git \"a/dir/a \\\"b\\\".rs\" \"b/dir/a \\\"b\\\".rs\"\n--- \"a/dir/a"
        ));
        assert!(patch.ends_with("@@ -10,1 +11,0 @@\n-ten\n"));
        // The quoted name parses back.
        assert_eq!(parse_diff(&patch)[0].path, "dir/a \"b\".rs",);
    }

    #[test]
    fn combined_diff_sections_are_skipped() {
        let output = format!(
            "{MODIFIED}diff --cc conflict.rs\nindex 1,2..3\n--- a/conflict.rs\n+++ b/conflict.rs\n\
@@@ -1,1 -1,1 +1,5 @@@\n++<<<<<<< HEAD\n{}",
            MODIFIED.replace("src/lib.rs", "next.rs")
        );
        let files = parse_diff(&output);
        let paths: Vec<_> = files.iter().map(|f| f.path.as_str()).collect();
        assert_eq!(paths, ["src/lib.rs", "next.rs"]);
        assert_eq!(files[0], parse_diff(MODIFIED)[0]);
    }

    #[test]
    fn header_paths_split_ambiguous_names() {
        assert_eq!(
            header_paths("a/x b/y b/x b/y"),
            Some(("x b/y".into(), "x b/y".into()))
        );
        assert_eq!(
            header_paths("a/old b/new"),
            Some(("old".into(), "new".into()))
        );
        assert_eq!(quote("plain path.rs"), "plain path.rs");
        assert_eq!(quote("a\u{85}"), "\"a\\302\\205\"");
        assert_eq!(unquote(&quote("a\u{85}\t\"")), "a\u{85}\t\"");
        assert_eq!(unquote("\"a\\\\b\\001\""), "a\\b\u{1}");
    }

    #[test]
    fn raw_and_numstat_records_merge_into_counted_files() {
        let z = "0".repeat(7);
        let output = format!(
            ":100644 100644 {z} {z} M\0src/a b.rs\0\
:000000 100644 {z} {z} A\0new.rs\0\
:100644 100644 {z} {z} R087\0old.rs\0:odd.rs\0\
:100644 000000 {z} {z} D\0gone.bin\0\
3\t1\tsrc/a b.rs\0\
2\t0\tnew.rs\0\
1\t1\t\0old.rs\0:odd.rs\0\
-\t-\tgone.bin\0"
        );
        let counted =
            |path: &str, old: Option<&str>, kind, counts: Option<(u32, u32)>| ChangedFile {
                path: path.into(),
                old_path: old.map(Into::into),
                kind,
                added: counts.map(|c| c.0),
                removed: counts.map(|c| c.1),
            };
        assert_eq!(
            parse_changes(output.as_bytes()),
            [
                counted("src/a b.rs", None, ChangeKind::Modified, Some((3, 1))),
                counted("new.rs", None, ChangeKind::Added, Some((2, 0))),
                counted(":odd.rs", Some("old.rs"), ChangeKind::Renamed, Some((1, 1))),
                counted("gone.bin", None, ChangeKind::Deleted, None),
            ]
        );
        assert_eq!(parse_changes(b""), []);
    }

    #[test]
    fn auto_base_prefers_origin_head_then_main_then_master() {
        let origin = "refs/remotes/origin/HEAD\tc1\trefs/remotes/origin/trunk\n";
        let main = "refs/heads/main\tc2\t\n";
        let master = "refs/heads/master\tc3\t\n";
        let base = |s: &str| auto_base(s).map(|(n, c)| format!("{n}@{c}"));
        assert_eq!(
            base(&format!("{main}{master}{origin}")).as_deref(),
            Some("origin/trunk@c1")
        );
        assert_eq!(base(&format!("{master}{main}")).as_deref(), Some("main@c2"));
        assert_eq!(base(master).as_deref(), Some("master@c3"));
        // Other refs matched by the patterns' prefixes are ignored.
        assert_eq!(base("refs/heads/main/x\tc4\t\n"), None);
        assert_eq!(base(""), None);
    }
}
