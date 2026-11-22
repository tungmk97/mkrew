//! The Chat mode's model (ADR 0104): what an agent pane draws for a transcript [`Page`], as
//! plain data. Pure and tested headlessly; `shell/chat_view.rs` draws it.

use std::collections::HashSet;

use mkrew_core::thread::{PlanStatus, PlanStep, Subagent, ToolCall};
use mkrew_protocol::SessionKind;
use ulid::Ulid;

use crate::docs::Shown;
use crate::icons::AgentLogo;
use crate::preview_select::{self, Point, Selection};
use crate::thread_view::{Page, PageItem, PageTurn};

/// Whether a pane can show Chat: an agent whose profile is a shipped one with a readable
/// transcript format (Claude, Codex, omp), or any agent whose transcript the daemon found.
pub fn available(kind: SessionKind, profile: &str, has_transcript: bool) -> bool {
    kind == SessionKind::Agent
        && (has_transcript
            || matches!(
                AgentLogo::of(profile),
                Some(AgentLogo::Claude | AgentLogo::Codex | AgentLogo::Omp)
            ))
}

/// The pane's mode: Chat when it can and its saved choice (else `[ui] chat_default`) says so.
pub fn effective(available: bool, saved: Option<bool>, default: bool) -> bool {
    available && saved.unwrap_or(default)
}

/// A row that opens and closes: (turn key, item index, call index; 0 for thinking).
pub type RowKey = (u64, usize, usize);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Row<'a> {
    /// Agent text: item `item` of the page turn.
    Text { item: usize },
    Thinking {
        key: RowKey,
        text: &'a str,
        open: bool,
    },
    Tool {
        key: RowKey,
        call: &'a ToolCall,
        open: bool,
    },
}

/// The mark before a plan step: an empty ring, a half ring, a check.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlanMark {
    Todo,
    Doing,
    Done,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanRow<'a> {
    pub text: &'a str,
    pub mark: PlanMark,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatTurn<'a> {
    /// `page.first_turn` + the turn's index: stable while the turn is kept.
    pub key: u64,
    pub prompt: &'a str,
    pub rows: Vec<Row<'a>>,
    /// Only on the newest turn that has a plan.
    pub plan: Option<Vec<PlanRow<'a>>>,
}

/// One subagent in the tray.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Chip<'a> {
    pub description: &'a str,
    pub subagent_type: &'a str,
    pub done: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuestionCard<'a> {
    pub text: &'a str,
    pub options: &'a [String],
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct View<'a> {
    /// Older turns exist but are not kept ("Earlier turns: Open transcript").
    pub earlier: bool,
    pub turns: Vec<ChatTurn<'a>>,
    /// The last turn's subagents, only while one of them runs.
    pub tray: Vec<Chip<'a>>,
    /// The last turn's question, only while it has no result.
    pub question: Option<QuestionCard<'a>>,
}

/// The view of `page` with the rows in `open` expanded. Borrows from the page: it is built on
/// every render, so it copies none of the transcript.
pub fn view<'a>(page: Option<&'a Page>, open: &HashSet<RowKey>) -> View<'a> {
    let Some(page) = page else {
        return View::default();
    };
    let plan_turn = page.turns.iter().rposition(|turn| turn.plan.is_some());
    let turns = page
        .turns
        .iter()
        .enumerate()
        .map(|(index, turn)| {
            let key = page.first_turn + index as u64;
            ChatTurn {
                key,
                prompt: &turn.prompt,
                rows: rows(key, turn, open),
                plan: if plan_turn == Some(index) {
                    turn.plan
                        .as_deref()
                        .filter(|steps| !steps.is_empty())
                        .map(plan_rows)
                } else {
                    None
                },
            }
        })
        .collect();
    let last = page.turns.last();
    View {
        earlier: page.truncated,
        turns,
        tray: last.map(|turn| tray(&turn.subagents)).unwrap_or_default(),
        question: last
            .and_then(|turn| turn.question.as_ref())
            .map(|question| QuestionCard {
                text: &question.text,
                options: &question.options,
            }),
    }
}

fn rows<'a>(turn_key: u64, turn: &'a PageTurn, open: &HashSet<RowKey>) -> Vec<Row<'a>> {
    let mut rows = Vec::new();
    for (item, entry) in turn.items.iter().enumerate() {
        match entry {
            PageItem::Text(_) => rows.push(Row::Text { item }),
            PageItem::Thinking(text) => {
                let key = (turn_key, item, 0);
                rows.push(Row::Thinking {
                    key,
                    text,
                    open: open.contains(&key),
                });
            }
            PageItem::Tools(calls) => {
                rows.extend(calls.iter().enumerate().map(|(index, call)| {
                    let key = (turn_key, item, index);
                    Row::Tool {
                        key,
                        call,
                        open: open.contains(&key),
                    }
                }));
            }
        }
    }
    rows
}

fn plan_rows(steps: &[PlanStep]) -> Vec<PlanRow<'_>> {
    steps
        .iter()
        .map(|step| PlanRow {
            text: &step.text,
            mark: match step.status {
                PlanStatus::Pending => PlanMark::Todo,
                PlanStatus::InProgress => PlanMark::Doing,
                PlanStatus::Done => PlanMark::Done,
            },
        })
        .collect()
}

/// Every subagent of the turn while at least one still runs; none once all are done.
fn tray(subagents: &[Subagent]) -> Vec<Chip<'_>> {
    if subagents.iter().all(|subagent| subagent.done) {
        return Vec::new();
    }
    subagents
        .iter()
        .map(|subagent| Chip {
            description: &subagent.description,
            subagent_type: &subagent.subagent_type,
            done: subagent.done,
        })
        .collect()
}

/// The transcripts to read: the panel's, then each Chat pane's; one per session (the first
/// key of a session wins), so the panel and the pane never read the same file twice.
pub fn wanted_reads(
    panel: Option<(Ulid, String)>,
    chats: Vec<(Ulid, String)>,
) -> Vec<(Ulid, String)> {
    let mut wanted: Vec<(Ulid, String)> = Vec::new();
    for key in panel.into_iter().chain(chats) {
        if !wanted.iter().any(|(session, _)| *session == key.0) {
            wanted.push(key);
        }
    }
    wanted
}

/// The scroll offset after moving `delta` px down (negative: up): gpui offsets are 0 at the top
/// and `-max_y` at the end.
pub fn scrolled(offset_y: f32, max_y: f32, delta: f32) -> f32 {
    (offset_y - delta).clamp(-max_y.max(0.0), 0.0)
}

/// Where Tab goes from the chat page: through its `count` keyboard stops, then back to the
/// composer; Shift+Tab goes the other way, from the first stop to the list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TabTo {
    Stop(usize),
    List,
    Composer,
}

/// The target of Tab (`forward`) or Shift+Tab from stop `at` (`None`: the list itself).
pub fn tab_to(count: usize, at: Option<usize>, forward: bool) -> TabTo {
    match (at, forward) {
        (None, true) if count > 0 => TabTo::Stop(0),
        (Some(i), true) if i + 1 < count => TabTo::Stop(i + 1),
        (None | Some(_), true) | (None, false) => TabTo::Composer,
        (Some(0), false) => TabTo::List,
        (Some(i), false) => TabTo::Stop(i - 1),
    }
}

/// A message: (turn key, item index); a selection lives inside one of them.
pub type MsgKey = (u64, usize);

/// A press at `at` in message `key`: a caret, Shift extends the selection of the same message,
/// a double click takes the word (as the Files preview does).
pub fn press(
    current: Option<(MsgKey, Selection)>,
    key: MsgKey,
    shown: &Shown,
    at: Point,
    extend: bool,
    clicks: usize,
) -> (MsgKey, Selection) {
    let selection = match current {
        _ if clicks == 2 => preview_select::word_at(shown, at),
        Some((was, selection)) if extend && was == key => Selection {
            head: at,
            ..selection
        },
        _ => Selection::caret(at),
    };
    (key, selection)
}

/// The text of message `key` in `page`; `None` when the page no longer has it as text.
pub fn text_of(page: &Page, key: MsgKey) -> Option<&Shown> {
    let index = usize::try_from(key.0.checked_sub(page.first_turn)?).ok()?;
    match page.turns.get(index)?.items.get(key.1)? {
        PageItem::Text(shown) => Some(shown),
        _ => None,
    }
}

/// The selected text as shown (Ctrl+C); `None` for no, an empty or a stale selection.
pub fn copy_text(page: &Page, selecting: Option<(MsgKey, Selection)>) -> Option<String> {
    let (key, selection) = selecting?;
    let text = preview_select::copy_text(text_of(page, key)?, &selection);
    (!text.is_empty()).then_some(text)
}

#[cfg(test)]
mod tests {
    use super::*;
    use mkrew_core::UsageFormat;
    use mkrew_core::thread::CHAT_TURNS;

    use crate::docs::Shown;
    use crate::preview_select::{Point, Selection};
    use crate::thread_view::ThreadFile;

    const CLAUDE_CHAT: &str =
        include_str!("../../mkrew-core/tests/fixtures/thread/claude_chat.jsonl");
    const CLAUDE_AGENTS: &str =
        include_str!("../../mkrew-core/tests/fixtures/thread/claude_agents.jsonl");

    fn page(jsonl: &str) -> Page {
        let mut file = ThreadFile::new(&UsageFormat::ClaudeJsonl, CHAT_TURNS).unwrap();
        file.apply(0, jsonl.as_bytes());
        file.page()
    }

    /// The first `count` lines.
    fn head(text: &str, count: usize) -> String {
        text.lines()
            .take(count)
            .map(|line| format!("{line}\n"))
            .collect()
    }

    #[test]
    fn one_read_per_session_and_the_panels_wins() {
        let [a, b] = [Ulid::new(), Ulid::new()];
        let key = |id, native: &str| (id, native.to_owned());
        assert_eq!(wanted_reads(None, vec![]), []);
        assert_eq!(
            wanted_reads(Some(key(a, "n1")), vec![key(b, "n2"), key(a, "other")]),
            [key(a, "n1"), key(b, "n2")]
        );
        assert_eq!(
            wanted_reads(None, vec![key(b, "n2"), key(b, "n3")]),
            [key(b, "n2")]
        );
    }

    fn labels(turn: &ChatTurn<'_>) -> Vec<String> {
        let state = |open: &bool| if *open { "open" } else { "shut" };
        turn.rows
            .iter()
            .map(|row| match row {
                Row::Text { item } => format!("text {item}"),
                Row::Thinking { open, .. } => format!("thinking {}", state(open)),
                Row::Tool { call, open, .. } => format!(
                    "{} {} {} {}",
                    call.name,
                    call.target,
                    state(open),
                    match &call.result {
                        Some(r) if r.is_error => "error",
                        Some(_) => "ok",
                        None => "pending",
                    }
                ),
            })
            .collect()
    }

    #[test]
    fn rows_follow_the_items_with_one_row_per_call_and_their_open_state() {
        let page = page(CLAUDE_CHAT);
        let shut = view(Some(&page), &HashSet::new());
        assert_eq!(
            labels(&shut.turns[0]),
            [
                "thinking shut",
                "text 1",
                "Bash cargo test shut error",
                "Read src/lib.rs shut ok",
                "text 3"
            ]
        );
        let open: HashSet<RowKey> = [(0, 2, 0), (0, 0, 0)].into_iter().collect();
        let opened = view(Some(&page), &open);
        assert_eq!(
            labels(&opened.turns[0]),
            [
                "thinking open",
                "text 1",
                "Bash cargo test open error",
                "Read src/lib.rs shut ok",
                "text 3"
            ]
        );
        assert_eq!(shut.turns[0].key, 0);
        assert!(!shut.earlier);
    }

    fn todo_turn(prompt: &str, id: &str, status: &str) -> String {
        format!(
            "{{\"type\":\"user\",\"message\":{{\"role\":\"user\",\"content\":\"{prompt}\"}}}}\n\
             {{\"type\":\"assistant\",\"message\":{{\"role\":\"assistant\",\"content\":[{{\"type\":\"tool_use\",\"id\":\"{id}\",\"name\":\"TodoWrite\",\"input\":{{\"todos\":[{{\"content\":\"Step\",\"status\":\"{status}\"}}]}}}}]}}}}\n"
        )
    }

    #[test]
    fn the_plan_card_is_on_the_newest_turn_that_has_a_plan() {
        let text = todo_turn("one", "a", "pending")
            + &todo_turn("two", "b", "in_progress")
            + "{\"type\":\"user\",\"message\":{\"role\":\"user\",\"content\":\"three\"}}\n";
        let page = page(&text);
        let v = view(Some(&page), &HashSet::new());
        let plans: Vec<_> = v.turns.iter().map(|t| t.plan.clone()).collect();
        assert_eq!(
            plans,
            [
                None,
                Some(vec![PlanRow {
                    text: "Step",
                    mark: PlanMark::Doing
                }]),
                None
            ]
        );
    }

    #[test]
    fn an_emptied_checklist_on_the_newest_plan_turn_shows_no_card() {
        let step = PlanStep {
            text: "Step".into(),
            status: PlanStatus::Pending,
        };
        let turn = |plan| PageTurn {
            prompt: String::new(),
            items: Vec::new(),
            plan,
            question: None,
            subagents: Vec::new(),
        };
        let page = Page {
            turns: vec![turn(Some(vec![step])), turn(Some(Vec::new())), turn(None)],
            truncated: false,
            first_turn: 0,
        };
        let v = view(Some(&page), &HashSet::new());
        assert!(v.turns.iter().all(|t| t.plan.is_none()));
    }

    #[test]
    fn plan_marks_follow_the_statuses() {
        let agents = page(CLAUDE_AGENTS);
        let v = view(Some(&agents), &HashSet::new());
        let marks: Vec<_> = v.turns[0]
            .plan
            .as_ref()
            .unwrap()
            .iter()
            .map(|r| r.mark)
            .collect();
        assert_eq!(marks, [PlanMark::Done, PlanMark::Done, PlanMark::Doing]);
    }

    #[test]
    fn a_question_shows_only_while_pending_and_only_for_the_last_turn() {
        let pending_page = page(&head(CLAUDE_AGENTS, 7));
        let pending = view(Some(&pending_page), &HashSet::new());
        assert_eq!(
            pending.question,
            Some(QuestionCard {
                text: "Which database?",
                options: &["SQLite".to_owned(), "Postgres".to_owned()]
            })
        );
        // The whole fixture: q1 is still unanswered but is not the last turn; q2 is answered.
        let all_page = page(CLAUDE_AGENTS);
        let all = view(Some(&all_page), &HashSet::new());
        assert_eq!(all.question, None);
    }

    #[test]
    fn the_tray_shows_while_a_subagent_runs_and_goes_when_all_are_done() {
        let running_page = page(&head(CLAUDE_AGENTS, 7));
        let running = view(Some(&running_page), &HashSet::new());
        let chip = |d, t, done| Chip {
            description: d,
            subagent_type: t,
            done,
        };
        assert_eq!(
            running.tray,
            [
                chip("Explore the repo", "Explore", true),
                chip("Review the diff", "reviewer", false)
            ]
        );
        let done = head(CLAUDE_AGENTS, 7)
            + "{\"type\":\"user\",\"message\":{\"role\":\"user\",\"content\":[{\"type\":\"tool_result\",\"tool_use_id\":\"s2\",\"content\":\"ok\"}]}}\n";
        let done_page = page(&done);
        assert!(view(Some(&done_page), &HashSet::new()).tray.is_empty());
        // The whole fixture: s2 never got a result, but it is not in the last turn.
        let all_page = page(CLAUDE_AGENTS);
        assert!(view(Some(&all_page), &HashSet::new()).tray.is_empty());
    }

    #[test]
    fn no_page_or_no_turns_is_an_empty_view_and_truncation_is_reported() {
        assert_eq!(view(None, &HashSet::new()), View::default());
        let empty = Page {
            turns: Vec::new(),
            truncated: true,
            first_turn: 3,
        };
        let v = view(Some(&empty), &HashSet::new());
        assert!(v.turns.is_empty() && v.earlier);
    }

    #[test]
    fn only_agents_with_a_readable_transcript_can_chat() {
        use SessionKind::{Agent, Shell};
        assert!(available(Agent, "claude", false));
        assert!(available(Agent, "codex", false));
        assert!(available(Agent, "omp", false));
        assert!(!available(Agent, "copilot", false), "no readable format");
        assert!(
            !available(Agent, "my-agent", false),
            "unknown until its transcript is found"
        );
        assert!(
            available(Agent, "my-agent", true),
            "a copied profile whose transcript is found"
        );
        assert!(!available(Shell, "claude", true), "a shell pane");
    }

    #[test]
    fn effective_mode_prefers_the_saved_choice_then_the_default_and_never_without_chat() {
        assert!(effective(true, Some(true), false));
        assert!(
            !effective(true, Some(false), true),
            "Terminal chosen against the default"
        );
        assert!(
            effective(true, None, true),
            "the default applies when nothing is saved"
        );
        assert!(!effective(true, None, false));
        assert!(
            !effective(false, Some(true), true),
            "an incapable pane is always Terminal"
        );
        assert!(!effective(false, None, true));
    }

    #[test]
    fn scrolling_stays_between_the_top_and_the_end() {
        // Offsets are 0 at the top and -max at the end (gpui's scroll offsets are negative).
        assert_eq!(scrolled(0.0, 300.0, 48.0), -48.0);
        assert_eq!(scrolled(-290.0, 300.0, 48.0), -300.0, "stops at the end");
        assert_eq!(scrolled(-10.0, 300.0, -48.0), 0.0, "stops at the top");
        assert_eq!(scrolled(0.0, 0.0, 48.0), 0.0, "nothing to scroll");
        // Home and End pass the extreme deltas.
        assert_eq!(scrolled(-100.0, 300.0, f32::MIN), 0.0, "Home");
        assert_eq!(scrolled(-100.0, 300.0, f32::MAX), -300.0, "End");
    }

    #[test]
    fn tab_goes_through_the_stops_then_to_the_composer() {
        use TabTo::*;
        assert_eq!(tab_to(2, None, true), Stop(0));
        assert_eq!(tab_to(2, Some(0), true), Stop(1));
        assert_eq!(tab_to(2, Some(1), true), Composer);
        assert_eq!(tab_to(0, None, true), Composer, "no stops");
        assert_eq!(tab_to(2, Some(1), false), Stop(0));
        assert_eq!(tab_to(2, Some(0), false), List);
        assert_eq!(tab_to(2, None, false), Composer);
    }

    fn at(byte: usize) -> Point {
        Point {
            block: 0,
            piece: 0,
            byte,
        }
    }

    #[test]
    fn a_press_puts_a_caret_shift_extends_in_the_same_message_and_a_double_click_takes_the_word() {
        let shown = Shown::from_text("hello world");
        let key = (0, 1);
        let (_, caret) = press(None, key, &shown, at(3), false, 1);
        assert_eq!(caret, Selection::caret(at(3)));
        let (_, extended) = press(Some((key, caret)), key, &shown, at(8), true, 1);
        assert_eq!((extended.anchor, extended.head), (at(3), at(8)));
        // Shift in another message starts over instead of mixing two messages.
        let (other, fresh) = press(Some((key, caret)), (0, 3), &shown, at(5), true, 1);
        assert_eq!((other, fresh), ((0, 3), Selection::caret(at(5))));
        let (_, word) = press(None, key, &shown, at(1), false, 2);
        assert_eq!(word.range_in(0, 0), Some(0..5), "hello");
    }

    #[test]
    fn a_stale_selection_is_ignored_after_the_page_changes() {
        let page = page(CLAUDE_CHAT);
        // Item 1 of turn 0 is the agent's text; item 0 is thinking; turn 7 does not exist.
        assert!(text_of(&page, (0, 1)).is_some());
        assert!(
            text_of(&page, (0, 0)).is_none(),
            "thinking is not selectable text"
        );
        assert!(text_of(&page, (0, 9)).is_none(), "no such item");
        assert!(text_of(&page, (7, 1)).is_none(), "no such turn");
        let whole = Selection {
            anchor: at(0),
            head: at(7),
        };
        assert_eq!(
            copy_text(&page, Some(((0, 1), whole))).as_deref(),
            Some("Running")
        );
        assert_eq!(copy_text(&page, Some(((7, 1), whole))), None);
        assert_eq!(
            copy_text(&page, Some(((0, 1), Selection::caret(at(2))))),
            None,
            "an empty selection copies nothing"
        );
        assert_eq!(copy_text(&page, None), None);
    }
}
