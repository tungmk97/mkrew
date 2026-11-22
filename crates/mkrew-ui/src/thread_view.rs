//! The session panel's Thread section (ADR 0057): turns built by
//! [`mkrew_core::thread::ThreadReader`] from the transcript bytes the daemon returns for
//! `session.transcript`, as the [`Page`] the panel renders. Feeding runs off the UI thread; the
//! caller polls about once a second.

use mkrew_core::UsageFormat;
use mkrew_core::thread::{Item, PlanStep, Question, Subagent, ThreadReader, ToolCall};

use crate::docs::Shown;

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
    pub plan: Option<Vec<PlanStep>>,
    pub question: Option<Question>,
    pub subagents: Vec<Subagent>,
}

pub enum PageItem {
    Text(Shown),
    /// A thinking block (the Chat view shows it, the Thread section does not).
    Thinking(String),
    Tools(Vec<ToolCall>),
}

impl Page {
    /// The newest `count` turns, and how many older kept turns that leaves out.
    pub fn newest(&self, count: usize) -> (usize, &[PageTurn]) {
        let skipped = self.turns.len().saturating_sub(count);
        (skipped, &self.turns[skipped..])
    }
}

/// The reader of one shown session's transcript, fed with what `session.transcript` returns.
pub struct ThreadFile {
    format: UsageFormat,
    max_turns: usize,
    reader: ThreadReader,
}

// The panel moves the reader to a background executor.
const _: fn() = || {
    fn send<T: Send>() {}
    send::<ThreadFile>();
    send::<Page>();
};

impl ThreadFile {
    pub fn new(format: &UsageFormat, max_turns: usize) -> Result<Self, String> {
        Ok(Self {
            format: format.clone(),
            max_turns,
            reader: ThreadReader::new(format, max_turns)?,
        })
    }

    /// Where the next `session.transcript` read starts.
    pub fn offset(&self) -> u64 {
        self.reader.offset()
    }

    /// Feeds `data`, which starts at `start` in the file; true if the turns changed. A chunk
    /// that does not continue where the reader stands (the file was rewritten, or the first
    /// look starts mid-file) starts the reader over at `start`.
    pub fn apply(&mut self, start: u64, data: &[u8]) -> bool {
        let mut changed = false;
        if start != self.reader.offset() {
            // The format was accepted in `new`.
            if let Ok(reader) = ThreadReader::new(&self.format, self.max_turns) {
                self.reader = reader;
                changed = true;
            }
            if start > 0 {
                self.reader.start_mid_file(start);
            }
        }
        for chunk in data.chunks(CHUNK) {
            changed |= self.reader.feed(chunk);
        }
        changed
    }

    /// The turns as the panel shows them.
    // ponytail: re-parses every kept turn's Markdown and clones every kept call on each change.
    // Bound: <= 50 turns x 50 detailed calls x ~12 KB (the reader's `DETAILED_CALLS`) copied
    // per build, off the UI thread. Upgrade path: a revision per turn in the reader and
    // `Arc<PageTurn>` reuse of the turns that did not change.
    pub fn page(&self) -> Page {
        Page {
            turns: self
                .reader
                .turns()
                .iter()
                .map(|turn| PageTurn {
                    prompt: clamp_prompt(&turn.prompt),
                    items: turn
                        .items
                        .iter()
                        .map(|item| match item {
                            Item::Text(text) => PageItem::Text(Shown::from_text(text)),
                            Item::Thinking(text) => PageItem::Thinking(text.clone()),
                            Item::Tools(calls) => PageItem::Tools(calls.clone()),
                        })
                        .collect(),
                    plan: turn.plan.clone(),
                    question: turn.question.clone(),
                    subagents: turn.subagents.clone(),
                })
                .collect(),
            truncated: self.reader.truncated(),
            first_turn: self.reader.first_turn(),
        }
    }
}

/// Characters of a prompt shown on a card.
const PROMPT_MAX: usize = 500;

/// `text` cut to [`PROMPT_MAX`] characters, with `…` when cut.
fn clamp_prompt(text: &str) -> String {
    match text.char_indices().nth(PROMPT_MAX) {
        Some((end, _)) => format!("{}…", &text[..end]),
        None => text.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mkrew_core::thread::{CHAT_TURNS, THREAD_TURNS};

    const CLAUDE_CHAT: &str =
        include_str!("../../mkrew-core/tests/fixtures/thread/claude_chat.jsonl");

    fn prompt(text: &str) -> String {
        format!(
            "{{\"type\":\"message\",\"message\":{{\"role\":\"user\",\"content\":[{{\"type\":\"text\",\"text\":\"{text}\"}}]}}}}\n"
        )
    }

    fn file() -> ThreadFile {
        ThreadFile::new(&UsageFormat::OmpJsonl, THREAD_TURNS).unwrap()
    }

    fn prompts(file: &ThreadFile) -> Vec<String> {
        file.page().turns.into_iter().map(|t| t.prompt).collect()
    }

    #[test]
    fn apply_feeds_contiguous_chunks_and_restarts_on_a_gap() {
        let mut file = file();
        assert!(file.apply(0, prompt("one").as_bytes()));
        assert!(!file.apply(file.offset(), b""), "nothing new");
        assert!(file.apply(file.offset(), prompt("two").as_bytes()));
        assert_eq!(prompts(&file), ["one", "two"]);
        // The daemon restarted from 0 (the file was rewritten shorter): start over.
        assert!(file.apply(0, prompt("fresh").as_bytes()));
        assert_eq!(prompts(&file), ["fresh"]);
    }

    #[test]
    fn a_first_chunk_from_the_middle_shows_the_tail_as_truncated() {
        let mut file = file();
        // The first line of a mid-file chunk is the end of a cut line: skipped.
        let cut = format!("end of a cut line\n{}", prompt("new"));
        assert!(file.apply(1234, cut.as_bytes()));
        assert!(file.page().truncated);
        // The next read continues from the end of that chunk: no restart.
        let two = prompt("two");
        assert!(file.apply(file.offset(), two.as_bytes()));
        assert_eq!(prompts(&file), ["new", "two"]);
    }

    #[test]
    fn page_parses_text_as_markdown() {
        let answer = "{\"type\":\"message\",\"message\":{\"role\":\"assistant\",\"content\":[{\"type\":\"text\",\"text\":\"# Done\"}]}}\n";
        let mut file = file();
        file.apply(0, (prompt("go") + answer).as_bytes());
        let page = file.page();
        let PageItem::Text(shown) = &page.turns[0].items[0] else {
            panic!("text expected");
        };
        assert!(!shown.blocks.is_empty());
    }

    #[test]
    fn long_prompts_are_cut_on_a_char_boundary() {
        assert_eq!(clamp_prompt("short"), "short");
        let exact = "é".repeat(PROMPT_MAX);
        assert_eq!(clamp_prompt(&exact), exact);
        let cut = clamp_prompt(&"é".repeat(PROMPT_MAX + 1));
        assert_eq!(cut.chars().count(), PROMPT_MAX + 1);
        assert!(cut.ends_with('…'));
    }

    #[test]
    fn the_page_carries_thinking_calls_with_results_and_the_turn_extras() {
        let mut file = ThreadFile::new(&UsageFormat::ClaudeJsonl, CHAT_TURNS).unwrap();
        file.apply(0, CLAUDE_CHAT.as_bytes());
        let page = file.page();
        let turn = &page.turns[0];
        assert!(matches!(&turn.items[0], PageItem::Thinking(t) if t == "Check the suite first."));
        let PageItem::Tools(calls) = &turn.items[2] else {
            panic!("tools expected");
        };
        assert_eq!(calls[0].result.as_ref().map(|r| r.is_error), Some(true));
        assert!(turn.plan.is_none() && turn.question.is_none() && turn.subagents.is_empty());
    }

    #[test]
    fn newest_leaves_out_the_older_kept_turns() {
        let mut file = ThreadFile::new(&UsageFormat::OmpJsonl, CHAT_TURNS).unwrap();
        for i in 0..25 {
            file.apply(file.offset(), prompt(&format!("p{i}")).as_bytes());
        }
        let page = file.page();
        let (skipped, shown) = page.newest(20);
        assert_eq!(
            (skipped, shown.len(), shown[0].prompt.as_str()),
            (5, 20, "p5")
        );
        let (skipped, shown) = page.newest(100);
        assert_eq!((skipped, shown.len()), (0, 25));
    }
}
