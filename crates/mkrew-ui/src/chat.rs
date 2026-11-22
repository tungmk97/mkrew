//! The Chat mode's model (ADR 0104): what an agent pane draws for a transcript [`Page`], as
//! plain data. Pure and tested headlessly; `shell/chat_view.rs` draws it.

use std::collections::HashSet;

use mkrew_core::thread::{PlanStatus, PlanStep, Subagent, ToolResult};

use crate::thread_view::{Page, PageItem, PageTurn};

/// A row that opens and closes: (turn key, item index, call index; 0 for thinking).
pub type RowKey = (u64, usize, usize);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Row {
    /// Agent text: item `item` of the page turn.
    Text { item: usize },
    Thinking {
        key: RowKey,
        text: String,
        open: bool,
    },
    Tool {
        key: RowKey,
        name: String,
        target: String,
        input: String,
        result: Option<ToolResult>,
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
pub struct PlanRow {
    pub text: String,
    pub mark: PlanMark,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatTurn {
    /// `page.first_turn` + the turn's index: stable while the turn is kept.
    pub key: u64,
    pub prompt: String,
    pub rows: Vec<Row>,
    /// Only on the newest turn that has a plan.
    pub plan: Option<Vec<PlanRow>>,
}

/// One subagent in the tray.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Chip {
    pub description: String,
    pub subagent_type: String,
    pub done: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuestionCard {
    pub text: String,
    pub options: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct View {
    /// Older turns exist but are not kept ("Earlier turns: Open transcript").
    pub earlier: bool,
    pub turns: Vec<ChatTurn>,
    /// The last turn's subagents, only while one of them runs.
    pub tray: Vec<Chip>,
    /// The last turn's question, only while it has no result.
    pub question: Option<QuestionCard>,
}

/// The view of `page` with the rows in `open` expanded.
pub fn view(page: Option<&Page>, open: &HashSet<RowKey>) -> View {
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
                prompt: turn.prompt.clone(),
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
                text: question.text.clone(),
                options: question.options.clone(),
            }),
    }
}

fn rows(turn_key: u64, turn: &PageTurn, open: &HashSet<RowKey>) -> Vec<Row> {
    let mut rows = Vec::new();
    for (item, entry) in turn.items.iter().enumerate() {
        match entry {
            PageItem::Text(_) => rows.push(Row::Text { item }),
            PageItem::Thinking(text) => {
                let key = (turn_key, item, 0);
                rows.push(Row::Thinking {
                    key,
                    text: text.clone(),
                    open: open.contains(&key),
                });
            }
            PageItem::Tools(calls) => {
                rows.extend(calls.iter().enumerate().map(|(call, c)| {
                    let key = (turn_key, item, call);
                    Row::Tool {
                        key,
                        name: c.name.clone(),
                        target: c.target.clone(),
                        input: c.input.clone(),
                        result: c.result.clone(),
                        open: open.contains(&key),
                    }
                }));
            }
        }
    }
    rows
}

fn plan_rows(steps: &[PlanStep]) -> Vec<PlanRow> {
    steps
        .iter()
        .map(|step| PlanRow {
            text: step.text.clone(),
            mark: match step.status {
                PlanStatus::Pending => PlanMark::Todo,
                PlanStatus::InProgress => PlanMark::Doing,
                PlanStatus::Done => PlanMark::Done,
            },
        })
        .collect()
}

/// Every subagent of the turn while at least one still runs; none once all are done.
fn tray(subagents: &[Subagent]) -> Vec<Chip> {
    if subagents.iter().all(|subagent| subagent.done) {
        return Vec::new();
    }
    subagents
        .iter()
        .map(|subagent| Chip {
            description: subagent.description.clone(),
            subagent_type: subagent.subagent_type.clone(),
            done: subagent.done,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use mkrew_core::UsageFormat;
    use mkrew_core::thread::CHAT_TURNS;

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

    fn labels(turn: &ChatTurn) -> Vec<String> {
        let state = |open: &bool| if *open { "open" } else { "shut" };
        turn.rows
            .iter()
            .map(|row| match row {
                Row::Text { item } => format!("text {item}"),
                Row::Thinking { open, .. } => format!("thinking {}", state(open)),
                Row::Tool {
                    name,
                    target,
                    open,
                    result,
                    ..
                } => format!(
                    "{name} {target} {} {}",
                    state(open),
                    match result {
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
        let v = view(Some(&page(&text)), &HashSet::new());
        let plans: Vec<_> = v.turns.iter().map(|t| t.plan.clone()).collect();
        assert_eq!(
            plans,
            [
                None,
                Some(vec![PlanRow {
                    text: "Step".into(),
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
        let v = view(Some(&page(CLAUDE_AGENTS)), &HashSet::new());
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
        let pending = view(Some(&page(&head(CLAUDE_AGENTS, 7))), &HashSet::new());
        assert_eq!(
            pending.question,
            Some(QuestionCard {
                text: "Which database?".into(),
                options: vec!["SQLite".into(), "Postgres".into()]
            })
        );
        // The whole fixture: q1 is still unanswered but is not the last turn; q2 is answered.
        let all = view(Some(&page(CLAUDE_AGENTS)), &HashSet::new());
        assert_eq!(all.question, None);
    }

    #[test]
    fn the_tray_shows_while_a_subagent_runs_and_goes_when_all_are_done() {
        let running = view(Some(&page(&head(CLAUDE_AGENTS, 7))), &HashSet::new());
        let chip = |d: &str, t: &str, done| Chip {
            description: d.into(),
            subagent_type: t.into(),
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
        assert!(view(Some(&page(&done)), &HashSet::new()).tray.is_empty());
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
}
