//! Find in the Files viewer (SPEC §12A): the query, its case-insensitive matches in the
//! opened file's lines, and the current one. Pure and tested headlessly; `main.rs` draws the
//! bar and `preview::code_view` the highlights.

use std::ops::Range;
use std::rc::Rc;

/// One match: the line (0-based) and the byte range in it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Match {
    pub line: usize,
    pub range: Range<usize>,
}

/// The matches to draw; cheap to clone into a render closure.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Hits {
    /// Sorted by line, then position.
    matches: Rc<[Match]>,
    current: Option<usize>,
}

impl Hits {
    /// The matches on `line` (byte ranges, left to right) and whether each is the current one.
    pub fn on_line(&self, line: usize) -> Vec<(Range<usize>, bool)> {
        let start = self.matches.partition_point(|m| m.line < line);
        self.matches[start..]
            .iter()
            .take_while(|m| m.line == line)
            .enumerate()
            .map(|(i, m)| (m.range.clone(), self.current == Some(start + i)))
            .collect()
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TextFind {
    pub query: String,
    hits: Hits,
}

impl TextFind {
    pub fn hits(&self) -> Hits {
        self.hits.clone()
    }

    pub fn current(&self) -> Option<&Match> {
        self.hits
            .current
            .and_then(|index| self.hits.matches.get(index))
    }

    /// "3 of 12", "No matches", or nothing while the query is empty.
    pub fn label(&self) -> String {
        match (self.query.is_empty(), self.hits.current) {
            (true, _) => String::new(),
            (false, None) => "No matches".to_owned(),
            (false, Some(index)) => format!("{} of {}", index + 1, self.hits.matches.len()),
        }
    }

    /// Searches `lines` again (after an edit of the query or a re-read of the file). The
    /// current match is the first one at or after the previous current position.
    pub fn search(&mut self, lines: &[String]) {
        let from = self.current().map_or((0, 0), |m| (m.line, m.range.start));
        let matches: Vec<Match> = lines
            .iter()
            .enumerate()
            .flat_map(|(line, text)| {
                find_in_line(text, &self.query)
                    .into_iter()
                    .map(move |range| Match { line, range })
            })
            .collect();
        let current = (!matches.is_empty()).then(|| {
            matches
                .iter()
                .position(|m| (m.line, m.range.start) >= from)
                .unwrap_or(0)
        });
        self.hits = Hits {
            matches: matches.into(),
            current,
        };
    }

    /// Enter / Shift+Enter: the next or previous match, wrapping around.
    pub fn step(&mut self, forward: bool) -> Option<&Match> {
        let len = self.hits.matches.len();
        let current = self.hits.current?;
        self.hits.current = Some(if forward {
            (current + 1) % len
        } else {
            (current + len - 1) % len
        });
        self.current()
    }
}

/// The byte ranges of `query` in `line`, ignoring case, left to right, not overlapping.
pub fn find_in_line(line: &str, query: &str) -> Vec<Range<usize>> {
    let mut found = Vec::new();
    if query.is_empty() {
        return found;
    }
    let mut from = 0;
    while let Some(c) = line[from..].chars().next() {
        match match_len(&line[from..], query) {
            Some(len) => {
                found.push(from..from + len);
                from += len;
            }
            None => from += c.len_utf8(),
        }
    }
    found
}

/// The byte length of the start of `text` that equals `query` ignoring case.
fn match_len(text: &str, query: &str) -> Option<usize> {
    let mut chars = text.char_indices();
    for want in query.chars() {
        let (_, got) = chars.next()?;
        if !got.to_lowercase().eq(want.to_lowercase()) {
            return None;
        }
    }
    Some(chars.next().map_or(text.len(), |(index, _)| index))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lines(text: &[&str]) -> Vec<String> {
        text.iter().map(|line| (*line).to_owned()).collect()
    }

    fn at(find: &TextFind) -> Option<(usize, usize)> {
        find.current().map(|m| (m.line, m.range.start))
    }

    #[test]
    fn matches_ignore_case_and_keep_byte_ranges_of_the_line() {
        // "Tiếng " is 8 bytes, "VIỆT" 6; lowercase 'ệ' is as long as 'Ệ'.
        assert_eq!(find_in_line("Tiếng VIỆT, việt", "việt"), [8..14, 16..22]);
        assert_eq!(find_in_line("aaaa", "aa"), [0..2, 2..4], "not overlapping");
        // 'İ' lowercases to two chars: no match, and ranges stay on char boundaries.
        let line = "İİi";
        let found = find_in_line(line, "i");
        assert_eq!((found.len(), found.first()), (1, Some(&(4..5))));
        assert!(found.iter().all(|r| line.get(r.clone()).is_some()));
        assert!(find_in_line("abc", "").is_empty());
    }

    #[test]
    fn enter_steps_through_matches_and_wraps() {
        let text = lines(&["fn a() {}", "let a = 1;", "", "a"]);
        let mut find = TextFind {
            query: "a".into(),
            ..TextFind::default()
        };
        find.search(&text);
        assert_eq!(find.label(), "1 of 3");
        assert_eq!(at(&find), Some((0, 3)));
        find.step(true);
        assert_eq!(at(&find), Some((1, 4)));
        find.step(true);
        assert_eq!(at(&find), Some((3, 0)));
        find.step(true);
        assert_eq!(at(&find), Some((0, 3)), "wraps to the first");
        find.step(false);
        assert_eq!(at(&find), Some((3, 0)), "and back to the last");
        assert_eq!(find.hits().on_line(1), [(4..5, false)]);
        assert_eq!(find.hits().on_line(3), [(0..1, true)]);
        assert!(find.hits().on_line(2).is_empty());
    }

    #[test]
    fn a_new_search_stays_at_or_after_the_current_match() {
        let text = lines(&["alpha", "beta", "alphabet"]);
        let mut find = TextFind {
            query: "al".into(),
            ..TextFind::default()
        };
        find.search(&text);
        find.step(true);
        assert_eq!(at(&find), Some((2, 0)));
        find.query.push('p');
        find.search(&text);
        assert_eq!(find.label(), "2 of 2", "typing on keeps the place");
        find.query = "zz".into();
        find.search(&text);
        assert_eq!(find.label(), "No matches");
        assert_eq!(find.current(), None);
        assert_eq!(find.step(true), None);
        find.query.clear();
        find.search(&text);
        assert_eq!(find.label(), "");
    }
}
