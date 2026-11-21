//! `fs.search`: ripgrep's engine over the files `fs.find` lists (Polish spec 2026-11-21 §8.3).
//! Blocking; the server runs it in `spawn_blocking`.
//!
//! The query is compiled once and the walk is parallel (`ignore`): each worker reads a text
//! file whole (at most 2 MiB, no NUL in its head) and searches the bytes, so there is nothing
//! to memory-map. The walk stops at `limit` hits and as soon as a newer search of the same
//! connection bumped the generation.
// ponytail: no index; add a persistent trigram index if search on big repos measurably lags

use std::io::Read as _;
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Mutex, PoisonError};

use grep_matcher::Matcher as _;
use grep_regex::{RegexMatcher, RegexMatcherBuilder};
use grep_searcher::SearcherBuilder;
use grep_searcher::sinks::Lossy;
use ignore::WalkState;
use mkrew_protocol::{FsSearchResult, SearchHit};

use crate::docs::MAX_BYTES;
use crate::files::{has_nul, relative, walker};

/// Longest preview line, in characters.
const PREVIEW_CHARS: usize = 200;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The query is not a valid pattern; the text says what is wrong.
    #[error("invalid search pattern: {0}")]
    Pattern(String),
    /// A newer search of the same connection replaced this one.
    #[error("a newer search replaced this one")]
    Cancelled,
}

/// The hits of `query` in the files under `root`, ordered by path then line, at most `limit`
/// (`truncated` when more exist). `regex: false` makes the query plain text; either way an
/// all-lowercase query ignores case. Returns [`Error::Cancelled`] once `generation` is no
/// longer `mine`.
pub fn search(
    root: &Path,
    query: &str,
    regex: bool,
    limit: usize,
    generation: &AtomicU64,
    mine: u64,
) -> Result<FsSearchResult, Error> {
    let matcher = RegexMatcherBuilder::new()
        .case_smart(true)
        .fixed_strings(!regex)
        .build(query)
        .map_err(|error| Error::Pattern(error.to_string()))?;
    let hits = Mutex::new(Vec::new());
    let found = AtomicUsize::new(0);
    let truncated = AtomicBool::new(false);
    // Copy: every worker's visitor takes its own.
    let cancelled = move || generation.load(Ordering::Relaxed) != mine;
    walker(root).build_parallel().run(|| {
        let mut searcher = SearcherBuilder::new().line_number(true).build();
        let (matcher, hits, found, truncated) = (&matcher, &hits, &found, &truncated);
        Box::new(move |entry| {
            if cancelled() || truncated.load(Ordering::Relaxed) {
                return WalkState::Quit;
            }
            let Ok(entry) = entry else {
                return WalkState::Continue;
            };
            if !entry.file_type().is_some_and(|kind| kind.is_file()) {
                return WalkState::Continue;
            }
            let Some(path) = relative(root, entry.path()) else {
                return WalkState::Continue;
            };
            let Some(bytes) = read_text(entry.path()) else {
                return WalkState::Continue;
            };
            let mut local = Vec::new();
            let sink = Lossy(|line, text| {
                // The (limit + 1)th hit proves there are more.
                if found.fetch_add(1, Ordering::Relaxed) >= limit {
                    truncated.store(true, Ordering::Relaxed);
                    return Ok(false);
                }
                local.push(hit(&path, line, text, matcher));
                Ok(true)
            });
            // A file that cannot be searched is skipped, never fatal.
            let _ = searcher.search_slice(matcher, &bytes, sink);
            hits.lock()
                .unwrap_or_else(PoisonError::into_inner)
                .extend(local);
            WalkState::Continue
        })
    });
    if cancelled() {
        return Err(Error::Cancelled);
    }
    let mut hits = hits.into_inner().unwrap_or_else(PoisonError::into_inner);
    hits.sort_by(|a, b| (&a.path, a.line).cmp(&(&b.path, b.line)));
    Ok(FsSearchResult {
        hits,
        truncated: truncated.load(Ordering::Relaxed),
    })
}

/// The bytes of `path`, or `None` when it cannot be read, is over [`MAX_BYTES`] or looks
/// binary (a NUL in its head, like `fs.read`).
fn read_text(path: &Path) -> Option<Vec<u8>> {
    let file = std::fs::File::open(path).ok()?;
    if file.metadata().ok()?.len() > MAX_BYTES {
        return None;
    }
    let mut bytes = Vec::new();
    file.take(MAX_BYTES + 1).read_to_end(&mut bytes).ok()?;
    (bytes.len() as u64 <= MAX_BYTES && !has_nul(&bytes)).then_some(bytes)
}

/// One hit: the line trimmed and cut to [`PREVIEW_CHARS`], and where the matcher matches in
/// what is kept (byte offsets, so a cut never splits a character or leaves a stale range).
fn hit(path: &str, line: u64, text: &str, matcher: &RegexMatcher) -> SearchHit {
    let preview: String = text.trim().chars().take(PREVIEW_CHARS).collect();
    let mut ranges = Vec::new();
    let to_u32 = |at: usize| u32::try_from(at).unwrap_or(u32::MAX);
    // An error here only means no highlight.
    let _ = matcher.find_iter(preview.as_bytes(), |found| {
        if found.start() < found.end() {
            ranges.push((to_u32(found.start()), to_u32(found.end())));
        }
        true
    });
    SearchHit {
        path: path.to_owned(),
        line: u32::try_from(line).unwrap_or(u32::MAX),
        preview,
        ranges,
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;
    use std::sync::atomic::AtomicU64;

    use super::*;

    fn write(root: &Path, path: &str, bytes: &[u8]) {
        let file = root.join(path);
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(file, bytes).unwrap();
    }

    /// A folder that looks like a repository (the walk reads `.gitignore` only inside one).
    fn repo(files: &[(&str, &str)]) -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join(".git")).unwrap();
        for (path, text) in files {
            write(dir.path(), path, text.as_bytes());
        }
        dir
    }

    fn run(root: &Path, query: &str, regex: bool, limit: usize) -> Result<FsSearchResult, Error> {
        search(root, query, regex, limit, &AtomicU64::new(1), 1)
    }

    fn places(result: &FsSearchResult) -> Vec<(&str, u32)> {
        result
            .hits
            .iter()
            .map(|h| (h.path.as_str(), h.line))
            .collect()
    }

    #[test]
    fn gitignore_is_respected_and_paths_are_relative_with_slashes() {
        let dir = repo(&[
            (".gitignore", "out/\n"),
            ("src/deep/a.rs", "a needle\n"),
            ("out/b.rs", "needle\n"),
        ]);
        let result = run(dir.path(), "needle", false, 10).unwrap();
        assert_eq!(places(&result), [("src/deep/a.rs", 1)]);
        assert!(!result.truncated);
    }

    #[test]
    fn binary_and_big_files_are_skipped() {
        let dir = repo(&[("text.txt", "needle\n")]);
        write(dir.path(), "image.bin", b"needle\0\x01\x02");
        let mut big = b"needle\n".to_vec();
        big.resize(usize::try_from(MAX_BYTES).unwrap() + 1, b'x');
        write(dir.path(), "big.txt", &big);
        let result = run(dir.path(), "needle", false, 10).unwrap();
        assert_eq!(places(&result), [("text.txt", 1)]);
    }

    #[test]
    fn the_limit_stops_the_search_and_says_so() {
        let text = "needle\n".repeat(10);
        let dir = repo(&[("a.txt", text.as_str()), ("b.txt", text.as_str())]);
        let cut = run(dir.path(), "needle", false, 5).unwrap();
        assert_eq!(cut.hits.len(), 5);
        assert!(cut.truncated);
        let exact = run(dir.path(), "needle", false, 20).unwrap();
        assert_eq!(exact.hits.len(), 20);
        assert!(!exact.truncated, "exactly the limit is not truncated");
    }

    #[test]
    fn an_all_lowercase_query_ignores_case() {
        let dir = repo(&[("a.txt", "Needle\nneedle\n")]);
        assert_eq!(run(dir.path(), "needle", false, 10).unwrap().hits.len(), 2);
        assert_eq!(run(dir.path(), "Needle", false, 10).unwrap().hits.len(), 1);
    }

    #[test]
    fn a_query_is_literal_unless_regex_is_on() {
        let dir = repo(&[("a.txt", "a.c\nabc\nf(x)\n")]);
        let literal = run(dir.path(), "a.c", false, 10).unwrap();
        assert_eq!(places(&literal), [("a.txt", 1)]);
        let regex = run(dir.path(), "a.c", true, 10).unwrap();
        assert_eq!(places(&regex), [("a.txt", 1), ("a.txt", 2)]);
        // A paren is text, not a broken pattern.
        let paren = run(dir.path(), "f(", false, 10).unwrap();
        assert_eq!(places(&paren), [("a.txt", 3)]);
    }

    #[test]
    fn an_invalid_regex_names_the_problem() {
        let dir = repo(&[("a.txt", "x\n")]);
        let Err(Error::Pattern(message)) = run(dir.path(), "(", true, 10) else {
            panic!("an open paren is not a regex");
        };
        assert!(!message.is_empty());
    }

    #[test]
    fn previews_are_trimmed_cut_and_carry_byte_ranges() {
        let long = format!("needle {}", "é".repeat(300));
        let dir = repo(&[("a.rs", "    let needle = 1;\r\n"), ("b.rs", long.as_str())]);
        let result = run(dir.path(), "needle", false, 10).unwrap();
        assert_eq!(result.hits[0].preview, "let needle = 1;");
        assert_eq!(result.hits[0].ranges, [(4, 10)]);
        assert_eq!(result.hits[1].preview.chars().count(), 200);
        assert_eq!(result.hits[1].ranges, [(0, 6)]);
    }

    #[test]
    fn a_newer_search_stops_this_one() {
        let dir = repo(&[("a.txt", "needle\n")]);
        let generation = AtomicU64::new(2);
        let stopped = search(dir.path(), "needle", false, 10, &generation, 1);
        assert!(matches!(stopped, Err(Error::Cancelled)));
        assert!(search(dir.path(), "needle", false, 10, &generation, 2).is_ok());
    }
}
