//! The session panel's Thread section (ADR 0057): reads the shown session's transcript file
//! (the last [`WINDOW`] bytes first, then what was appended) through
//! [`mkrew_core::thread::ThreadReader`], and builds the [`Page`] the panel renders.
//! Runs off the UI thread; the caller polls [`ThreadFile::read`] about once a second.

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use mkrew_core::UsageFormat;
use mkrew_core::thread::{Item, ThreadReader, ToolCall};

use crate::docs::Shown;

/// Bytes read on the first look at a transcript (its end).
pub const WINDOW: u64 = 4 * 1024 * 1024;
const CHUNK: usize = 1024 * 1024;

/// What the panel shows: the kept turns, with text parsed as Markdown.
pub struct Page {
    pub turns: Vec<PageTurn>,
    /// Earlier turns exist but are not shown ("Earlier turns: Open transcript").
    pub truncated: bool,
    /// Key of `turns[0]` (see `ThreadReader::first_turn`), for per-turn UI state.
    pub first_turn: u64,
}

pub struct PageTurn {
    pub prompt: String,
    pub items: Vec<PageItem>,
}

pub enum PageItem {
    Text(Shown),
    Tools(Vec<ToolCall>),
}

/// The reader of one shown session's transcript.
pub struct ThreadFile {
    format: UsageFormat,
    locate: Box<dyn Fn() -> Option<PathBuf> + Send>,
    path: Option<PathBuf>,
    reader: ThreadReader,
}

// The panel moves the reader to a background executor.
const _: fn() = || {
    fn send<T: Send>() {}
    send::<ThreadFile>();
    send::<Page>();
};

impl ThreadFile {
    pub fn new(
        format: &UsageFormat,
        locate: Box<dyn Fn() -> Option<PathBuf> + Send>,
    ) -> Result<Self, String> {
        Ok(Self {
            format: format.clone(),
            locate,
            path: None,
            reader: ThreadReader::new(format)?,
        })
    }

    /// Reads what was appended since the last read; true if the turns changed. A missing or
    /// unreadable file keeps the turns and is located again at the next read.
    pub fn read(&mut self) -> bool {
        if self.path.is_none() {
            self.path = (self.locate)();
        }
        let Some(path) = self.path.clone() else {
            return false;
        };
        match self.read_appended(&path) {
            Ok(changed) => changed,
            Err(error) => {
                tracing::debug!(path = %path.display(), "reading the transcript failed: {error}");
                self.path = None;
                false
            }
        }
    }

    fn read_appended(&mut self, path: &Path) -> std::io::Result<bool> {
        let mut file = File::open(path)?;
        let len = file.metadata()?.len();
        let mut changed = false;
        if len < self.reader.offset() {
            // Rewritten from scratch: start over (the format was accepted in `new`).
            if let Ok(reader) = ThreadReader::new(&self.format) {
                self.reader = reader;
                changed = true;
            }
        }
        if self.reader.offset() == 0 && len > WINDOW {
            self.reader.start_mid_file(len - WINDOW);
        }
        file.seek(SeekFrom::Start(self.reader.offset()))?;
        let mut chunk = vec![0; CHUNK];
        loop {
            match file.read(&mut chunk) {
                Ok(0) => return Ok(changed),
                Ok(n) => changed |= self.reader.feed(&chunk[..n]),
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
                Err(e) => return Err(e),
            }
        }
    }

    /// The turns as the panel shows them.
    // ponytail: re-parses every kept turn's Markdown on each change (at most 20 turns); cache
    // by text if it ever shows in a profile.
    pub fn page(&self) -> Page {
        Page {
            turns: self
                .reader
                .turns()
                .iter()
                .map(|turn| PageTurn {
                    prompt: turn.prompt.clone(),
                    items: turn
                        .items
                        .iter()
                        .map(|item| match item {
                            Item::Text(text) => PageItem::Text(Shown::from_text(text)),
                            Item::Tools(calls) => PageItem::Tools(calls.clone()),
                        })
                        .collect(),
                })
                .collect(),
            truncated: self.reader.truncated(),
            first_turn: self.reader.first_turn(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn prompt(text: &str) -> String {
        format!(
            "{{\"type\":\"message\",\"message\":{{\"role\":\"user\",\"content\":[{{\"type\":\"text\",\"text\":\"{text}\"}}]}}}}\n"
        )
    }

    fn reader_at(path: &std::path::Path) -> ThreadFile {
        let path = path.to_owned();
        ThreadFile::new(&UsageFormat::OmpJsonl, Box::new(move || Some(path.clone()))).unwrap()
    }

    fn prompts(file: &ThreadFile) -> Vec<String> {
        file.page().turns.into_iter().map(|t| t.prompt).collect()
    }

    #[test]
    fn reads_only_what_was_appended_and_restarts_when_rewritten() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("t.jsonl");
        let mut file = reader_at(&path);
        assert!(!file.read(), "no file yet");

        std::fs::write(&path, prompt("one")).unwrap();
        assert!(file.read());
        assert!(!file.read(), "nothing new");
        std::fs::write(&path, prompt("one") + &prompt("two")).unwrap();
        assert!(file.read());
        assert_eq!(prompts(&file), ["one", "two"]);

        // Rewritten shorter (e.g. a new conversation in the same file): read again.
        std::fs::write(&path, prompt("fresh")).unwrap();
        assert!(file.read());
        assert_eq!(prompts(&file), ["fresh"]);
    }

    /// A deleted file keeps what is shown; it is located again at the next read.
    #[test]
    fn a_missing_file_keeps_the_turns() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("t.jsonl");
        let mut file = reader_at(&path);
        std::fs::write(&path, prompt("one")).unwrap();
        assert!(file.read());
        std::fs::remove_file(&path).unwrap();
        assert!(!file.read());
        assert_eq!(prompts(&file), ["one"]);
        std::fs::write(&path, prompt("one") + &prompt("two")).unwrap();
        assert!(file.read());
        assert_eq!(prompts(&file), ["one", "two"]);
    }

    /// A long file is read from its last WINDOW bytes only, and says turns are missing.
    #[test]
    fn a_long_file_starts_at_the_window() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("t.jsonl");
        let filler = "x".repeat(1024);
        let mut text = prompt("old");
        while (text.len() as u64) < WINDOW + 64 * 1024 {
            text.push_str(&format!("{{\"type\":\"pad\",\"p\":\"{filler}\"}}\n"));
        }
        text.push_str(&prompt("new"));
        std::fs::write(&path, &text).unwrap();
        let mut file = reader_at(&path);
        assert!(file.read());
        assert!(file.page().truncated);
        assert_eq!(prompts(&file), ["new"]);
    }

    #[test]
    fn page_parses_text_as_markdown() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("t.jsonl");
        let answer = "{\"type\":\"message\",\"message\":{\"role\":\"assistant\",\"content\":[{\"type\":\"text\",\"text\":\"# Done\"}]}}\n";
        std::fs::write(&path, prompt("go") + answer).unwrap();
        let mut file = reader_at(&path);
        file.read();
        let page = file.page();
        let PageItem::Text(shown) = &page.turns[0].items[0] else {
            panic!("text expected");
        };
        assert!(!shown.blocks.is_empty());
    }
}
