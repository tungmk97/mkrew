//! Markdown review (SPEC §12): which files are reviewable, re-anchoring comments after the file
//! changed, and the prompt typed into an agent session. Pure.

use globset::{GlobBuilder, GlobSet, GlobSetBuilder};
use mkrew_protocol::{CommentContext, DocComment};

/// `review_globs` of a workspace that has none configured.
pub const DEFAULT_REVIEW_GLOBS: [&str; 3] = ["*.md", "docs/**/*.md", ".claude/plans/**/*.md"];

/// Where a comment's passage is in the current file content.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Anchor {
    /// First and last line, 1-based, inclusive.
    Lines(u32, u32),
    /// The passage is gone.
    Outdated,
}

/// Finds `quote` in `content`, ignoring differences in whitespace (line breaks included), and
/// returns the lines of the match whose first line is nearest to the old `start` (the earlier
/// one on a tie). Without a quote the old lines are kept while the file is still long enough
/// (and they are a valid range: `1 <= start <= end`).
pub fn reanchor(content: &str, start: u32, end: u32, quote: &str) -> Anchor {
    let quote = collapse(quote);
    if quote.is_empty() {
        let lines = u32::try_from(content.lines().count()).unwrap_or(u32::MAX);
        return if 1 <= start && start <= end && end <= lines {
            Anchor::Lines(start, end)
        } else {
            Anchor::Outdated
        };
    }
    let (text, lines) = collapse_with_lines(content);
    let len = quote.len();
    let mut best: Option<(u32, u32)> = None;
    let mut from = 0;
    // Overlapping matches count too (`x x` in `x x x`), unlike `match_indices`.
    while let Some(offset) = text[from..].find(&quote) {
        let at = from + offset;
        // `quote` has no leading/trailing space, so both ends sit on a kept character.
        let found = (lines[at], lines[at + len - 1]);
        if best.is_none_or(|(s, _)| found.0.abs_diff(start) < s.abs_diff(start)) {
            best = Some(found);
        }
        from = at + text[at..].chars().next().map_or(1, char::len_utf8);
    }
    match best {
        Some((s, e)) => Anchor::Lines(s, e),
        None => Anchor::Outdated,
    }
}

/// `text` with every whitespace run turned into one space, trimmed.
fn collapse(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// [`collapse`] of `content`, plus the 1-based line of every byte of the result.
fn collapse_with_lines(content: &str) -> (String, Vec<u32>) {
    let mut text = String::with_capacity(content.len());
    let mut lines = Vec::with_capacity(content.len());
    for (index, line) in content.lines().enumerate() {
        let number = u32::try_from(index + 1).unwrap_or(u32::MAX);
        for word in line.split_whitespace() {
            if !text.is_empty() {
                // The space before a word that starts a line belongs to that line; matches
                // never start or end on a space, so it does not matter which.
                text.push(' ');
                lines.push(number);
            }
            text.push_str(word);
            lines.extend(std::iter::repeat_n(number, word.len()));
        }
    }
    (text, lines)
}

/// The prompt that asks a session to address the review comments on `file` (SPEC §12
/// template). `comments` holds thread roots, in order, and may hold their replies too: those
/// follow their comment so a resent thread carries the conversation so far. Diff comments
/// (SPEC §12B) ask to fix the code instead, which may touch other files too.
pub fn prompt(file: &str, comments: &[DocComment]) -> String {
    let roots = || comments.iter().filter(|c| c.parent_id.is_none());
    let code = roots().any(|c| c.context == CommentContext::Diff);
    let (task, scope, quoted) = if code {
        (
            "Fix the code in",
            "Line numbers are of the current file.",
            "code",
        )
    } else {
        ("Update", "Only edit this file.", "passage")
    };
    let mut out = format!(
        "{task} {file} according to the review comments below.\n\
         {scope} When done, for EACH comment run:\n  \
         mk comment reply <id> \"<what you changed, or why you did not>\"\n\n"
    );
    for root in roots() {
        out.push_str(&format!("[{}] {}", root.id, lines_label(root)));
        if let Some(quote) = root
            .quote
            .as_deref()
            .map(collapse)
            .filter(|q| !q.is_empty())
        {
            out.push_str(&format!(", {quoted} \"{quote}\""));
        }
        out.push_str(":\n");
        push_indented(&mut out, "", "  ", &root.body);
        // Replies whose root is not in `comments` are left out.
        for reply in comments.iter().filter(|c| c.parent_id == Some(root.id)) {
            let first = format!("{} replied: ", reply.author);
            push_indented(&mut out, &first, "    ", &reply.body);
        }
    }
    // No trailing newline: whoever types the prompt submits it.
    out.truncate(out.trim_end().len());
    out
}

fn lines_label(comment: &DocComment) -> String {
    match (comment.start_line, comment.end_line) {
        (Some(s), Some(e)) if s != e => format!("lines {s}–{e}"),
        (Some(s), _) => format!("line {s}"),
        _ => "whole file".to_owned(),
    }
}

/// Appends `body`: its first line indented by two spaces and `prefix`, the other (non-empty)
/// lines by `rest`.
fn push_indented(out: &mut String, prefix: &str, rest: &str, body: &str) {
    for (index, line) in body.trim().lines().enumerate() {
        let line = line.trim_end();
        if index == 0 {
            out.push_str("  ");
            out.push_str(prefix);
        } else if !line.is_empty() {
            out.push_str(rest);
        }
        out.push_str(line);
        out.push('\n');
    }
}

/// A compiled set of review globs, matched against project-relative `/`-separated paths.
/// `*` and `?` stay within one path segment; `**` spans any number of them.
#[derive(Debug, Clone)]
pub struct ReviewGlobs(GlobSet);

impl ReviewGlobs {
    /// Fails on the first invalid pattern.
    pub fn new<S: AsRef<str>>(patterns: &[S]) -> Result<Self, globset::Error> {
        let mut set = GlobSetBuilder::new();
        for pattern in patterns {
            set.add(
                GlobBuilder::new(pattern.as_ref())
                    .literal_separator(true)
                    .build()?,
            );
        }
        Ok(Self(set.build()?))
    }

    /// The workspace's globs, or [`DEFAULT_REVIEW_GLOBS`] when it has none.
    pub fn for_project(review_globs: Option<&[String]>) -> Result<Self, globset::Error> {
        match review_globs {
            Some(patterns) => Self::new(patterns),
            None => Self::new(&DEFAULT_REVIEW_GLOBS),
        }
    }

    /// `relative_path` is relative to the project root (no leading `./`).
    pub fn matches(&self, relative_path: &str) -> bool {
        self.0.is_match(relative_path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PLAN: &str = "# Plan\n\
        \n\
        Use Redis for\n\
        caching of   sessions.\n\
        \n\
        ## Steps\n\
        Use Redis for caching of sessions.\n";

    /// A comment from its wire JSON (core has no `ulid` dependency to build ids with).
    fn comment(
        id: &str,
        parent: Option<&str>,
        lines: Option<(u32, u32)>,
        quote: Option<&str>,
        author: &str,
        body: &str,
    ) -> DocComment {
        serde_json::from_value(serde_json::json!({
            "id": id,
            "project_id": "01JX0000000000000000000000",
            "file_path": "docs/plan.md",
            "parent_id": parent,
            "author": author,
            "body": body,
            "start_line": lines.map(|l| l.0),
            "end_line": lines.map(|l| l.1),
            "quote": quote,
            "status": "open",
            "sent_to": null,
            "sent_snapshot": null,
            "created_at": 0,
            "updated_at": 0,
        }))
        .unwrap()
    }

    #[test]
    fn reanchor_follows_a_moved_passage_across_reflowed_whitespace() {
        // Written on lines 3–4, then two lines were inserted above and the passage reflowed.
        let content = format!("intro\nmore\n{PLAN}");
        assert_eq!(
            reanchor(&content, 3, 4, "Use Redis for caching of sessions."),
            Anchor::Lines(5, 6)
        );
        // Whitespace inside the quote does not matter either.
        assert_eq!(
            reanchor(PLAN, 3, 4, "  Redis\n for  caching "),
            Anchor::Lines(3, 4)
        );
    }

    #[test]
    fn reanchor_picks_the_match_nearest_to_the_old_lines() {
        assert_eq!(reanchor(PLAN, 3, 4, "Use Redis"), Anchor::Lines(3, 3));
        assert_eq!(reanchor(PLAN, 6, 6, "Use Redis"), Anchor::Lines(7, 7));
        // A tie goes to the earlier match.
        assert_eq!(reanchor(PLAN, 5, 5, "Use Redis"), Anchor::Lines(3, 3));
        // Overlapping matches are candidates too.
        assert_eq!(reanchor("x\nx\nx\n", 2, 3, "x x"), Anchor::Lines(2, 3));
    }

    #[test]
    fn reanchor_marks_a_deleted_passage_outdated() {
        assert_eq!(reanchor(PLAN, 3, 4, "Use Postgres"), Anchor::Outdated);
        assert_eq!(reanchor("", 1, 1, "Use Redis"), Anchor::Outdated);
        // Whitespace is collapsed, not dropped: a missing space is a different quote.
        assert_eq!(reanchor(PLAN, 3, 3, "Redisfor"), Anchor::Outdated);
    }

    #[test]
    fn reanchor_without_a_quote_keeps_lines_that_still_exist() {
        assert_eq!(reanchor(PLAN, 2, 7, " \n"), Anchor::Lines(2, 7));
        assert_eq!(reanchor(PLAN, 2, 8, ""), Anchor::Outdated);
        assert_eq!(reanchor(PLAN, 0, 1, ""), Anchor::Outdated);
        assert_eq!(reanchor(PLAN, 3, 2, ""), Anchor::Outdated);
    }

    #[test]
    fn reanchor_handles_crlf_and_non_ascii() {
        let content = "a\r\nnaïve — café\r\nb\r\n";
        assert_eq!(reanchor(content, 9, 9, "— café"), Anchor::Lines(2, 2));
    }

    #[test]
    fn prompt_matches_the_spec_template() {
        let comments = [
            comment(
                "c_01JX0000000000000000000001",
                None,
                Some((12, 14)),
                Some("Use Redis for caching"),
                DocComment::USER,
                "Why not an in-memory cache first? Is Redis overkill?",
            ),
            comment(
                "c_01JX0000000000000000000002",
                None,
                Some((40, 40)),
                Some("M3: deploy"),
                DocComment::USER,
                "Add a rollback step.",
            ),
        ];
        insta::assert_snapshot!(prompt("docs/plan.md", &comments));
    }

    #[test]
    fn prompt_carries_replies_and_multi_line_text() {
        let root = "c_01JX0000000000000000000001";
        let comments = [
            comment(
                root,
                None,
                Some((3, 4)),
                Some("Use Redis for\n  caching"),
                DocComment::USER,
                "Two points:\n\n- cost\n- ops\n",
            ),
            comment(
                "c_01JX0000000000000000000003",
                Some(root),
                None,
                None,
                "claude",
                "Kept Redis, see §2.",
            ),
            comment(
                "c_01JX0000000000000000000004",
                Some(root),
                None,
                None,
                DocComment::USER,
                "Then add a fallback:\nan in-memory LRU.",
            ),
        ];
        insta::assert_snapshot!(prompt("docs/plan.md", &comments));
    }

    #[test]
    fn prompt_for_diff_comments_asks_to_fix_the_code() {
        let mut root = comment(
            "c_01JX0000000000000000000001",
            None,
            Some((3, 4)),
            Some("fn add(a: i32, b: i32) -> i32 {\n    a - b"),
            DocComment::USER,
            "This subtracts.",
        );
        root.file_path = "src/lib.rs".to_owned();
        root.context = CommentContext::Diff;
        insta::assert_snapshot!(prompt("src/lib.rs", &[root]));
    }

    #[test]
    fn default_globs_match_root_docs_and_plans_only() {
        let globs = ReviewGlobs::for_project(None).unwrap();
        for path in [
            "README.md",
            "docs/a.md",
            "docs/x/y/a.md",
            ".claude/plans/p.md",
        ] {
            assert!(globs.matches(path), "{path}");
        }
        for path in ["src/notes.md", "README.txt", "docs/a.rs", "plans/p.md"] {
            assert!(!globs.matches(path), "{path}");
        }
    }

    #[test]
    fn project_globs_replace_the_defaults_and_bad_ones_fail() {
        let custom = vec!["specs/*.md".to_owned()];
        let globs = ReviewGlobs::for_project(Some(&custom)).unwrap();
        assert!(globs.matches("specs/a.md"));
        assert!(!globs.matches("README.md"));
        assert!(ReviewGlobs::new(&["[a-"]).is_err());
    }
}
