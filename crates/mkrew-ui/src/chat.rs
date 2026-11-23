//! The Chat mode's model (ADR 0104): what an agent pane draws for a transcript [`Page`], as
//! plain data. Pure and tested headlessly; `shell/chat_view.rs` draws it.

use std::collections::HashSet;

use mkrew_core::thread::{PlanStatus, PlanStep, Subagent, ToolCall, short_model};
use mkrew_protocol::{Recap, SessionKind, SessionUsageEvent};
use ulid::Ulid;

use crate::docs::Shown;
use crate::git_summary::GitSummary;
use crate::icons::AgentLogo;
use crate::preview_select::{self, Point, Selection};
use crate::thread_view::{Page, PageItem, PageTurn};
use crate::{theme, usage};

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

/// The `[ui] chat_default` to write after the user picked Chat (`picked_chat`) or Terminal for a
/// pane while the default is `current`: the pick when it differs, else nothing to write.
pub fn default_after_pick(current: bool, picked_chat: bool) -> Option<bool> {
    (current != picked_chat).then_some(picked_chat)
}

/// A row that opens and closes: (turn key, item index, call index; 0 for thinking,
/// `usize::MAX` for an activity row).
pub type RowKey = (u64, usize, usize);

/// A row of a turn.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Row<'a> {
    /// Agent text: item `item` of the page turn.
    Text { item: usize },
    /// Thinking and tool calls between two texts (or the turn's ends) as one row (ADR 0105).
    Activity {
        /// `(turn key, first item, usize::MAX)`: never the key of one of its children.
        key: RowKey,
        /// Thinking time of the children that have one, summed.
        thinking_ms: Option<u64>,
        counts: Counts,
        /// The italic part of the label: "Thought for 4s", "Thought", or "Working…".
        thought: Option<String>,
        /// "3 commands, 2 reads"; empty while "Working…".
        summary: String,
        children: Vec<Child<'a>>,
        open: bool,
    },
    /// omp's recap after the last turn (see [`recap_shown`]).
    Recap { text: &'a str },
}

/// A row inside an open activity row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Child<'a> {
    /// Opens to its text unless the text is empty (Claude's redacted thinking).
    Thinking {
        key: RowKey,
        text: &'a str,
        ms: Option<u64>,
        open: bool,
    },
    Tool {
        key: RowKey,
        call: &'a ToolCall,
        open: bool,
    },
}

/// Tool calls of one activity row by kind.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Counts {
    pub commands: usize,
    pub reads: usize,
    pub edits: usize,
    pub web: usize,
    pub agents: usize,
    pub other: usize,
}

impl Counts {
    /// Counts one call of the tool `name` (the agents' names, in any case).
    pub fn add(&mut self, name: &str) {
        let slot = match name.to_ascii_lowercase().as_str() {
            "bash" | "shell" | "shell_command" | "exec_command" => &mut self.commands,
            "read" | "view" | "grep" | "glob" | "find" | "search" => &mut self.reads,
            "edit" | "write" | "multiedit" | "apply_patch" | "ast_edit" => &mut self.edits,
            "web_search" | "websearch" | "webfetch" | "fetch" | "browser" => &mut self.web,
            "task" | "agent" => &mut self.agents,
            _ => &mut self.other,
        };
        *slot += 1;
    }

    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }

    /// `3 commands, 2 reads, 1 edit`: in that order, a kind without calls left out.
    pub fn label(&self) -> String {
        [
            (self.commands, "command", "commands"),
            (self.reads, "read", "reads"),
            (self.edits, "edit", "edits"),
            (self.web, "web call", "web calls"),
            (self.agents, "agent", "agents"),
            (self.other, "tool", "tools"),
        ]
        .into_iter()
        .filter(|(n, ..)| *n > 0)
        .map(|(n, one, many)| format!("{n} {}", if n == 1 { one } else { many }))
        .collect::<Vec<_>>()
        .join(", ")
    }
}

/// `Thought for 4s` (at least 1 s), or `Thinking` when the block has no time.
pub fn thinking_label(ms: Option<u64>) -> String {
    match ms {
        Some(ms) => format!("Thought for {}", format_elapsed(ms.max(1000))),
        None => "Thinking".to_owned(),
    }
}

/// The italic part of an activity row that is not running: the thinking time, or just
/// `Thought` when the row holds thinking without a time and no tool calls.
fn thought_label(thinking: bool, ms: Option<u64>, counts: &Counts) -> Option<String> {
    match ms {
        Some(_) => Some(thinking_label(ms)),
        None if thinking && counts.is_empty() => Some("Thought".to_owned()),
        None => None,
    }
}

/// `42s`, `2m 46s`, `1h 03m`.
pub fn format_elapsed(ms: u64) -> String {
    let secs = ms / 1000;
    match secs {
        0..=59 => format!("{secs}s"),
        60..=3599 => format!("{}m {:02}s", secs / 60, secs % 60),
        _ => format!("{}h {:02}m", secs / 3600, secs % 3600 / 60),
    }
}

/// `950`, `1.2K`, `74K`, `1.5M`: tokens on a chip.
pub fn format_k(n: u64) -> String {
    match n {
        0..=999 => n.to_string(),
        1_000..=9_999 => format!("{}.{}K", n / 1000, n % 1000 / 100),
        10_000..=999_999 => format!("{}K", n / 1000),
        _ => format!("{}.{}M", n / 1_000_000, n % 1_000_000 / 100_000),
    }
}

/// The recap goes after the last turn when the agent is not working and the recap is not older
/// than that turn's prompt (`recap.at` is in seconds, `started_at` in milliseconds; a prompt
/// without a time shows any recap).
pub fn recap_shown(recap: &Recap, last_started_at: Option<i64>, working: bool) -> bool {
    !working && last_started_at.is_none_or(|started| recap.at >= started / 1000)
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
    /// Every running subagent of the kept turns, see [`tray`].
    pub tray: Vec<Chip<'a>>,
    /// The last turn's question, only while it has no result.
    pub question: Option<QuestionCard<'a>>,
}

/// The view of `page` with the rows in `open` expanded. Borrows from the page: it is built on
/// every render, so it copies none of the transcript. `working`: the agent is working, so the
/// newest turn's last activity row says "Working…" and no recap shows; `recap`: omp's newest.
pub fn view<'a>(
    page: Option<&'a Page>,
    open: &HashSet<RowKey>,
    recap: Option<&'a Recap>,
    working: bool,
) -> View<'a> {
    let Some(page) = page else {
        return View::default();
    };
    let plan_turn = page.turns.iter().rposition(|turn| turn.plan.is_some());
    let newest = page.turns.len().checked_sub(1);
    let turns = page
        .turns
        .iter()
        .enumerate()
        .map(|(index, turn)| {
            let key = page.first_turn + index as u64;
            let is_newest = Some(index) == newest;
            let mut turn_rows = rows(key, turn, open, working && is_newest);
            if is_newest
                && let Some(recap) = recap.filter(|r| recap_shown(r, turn.started_at, working))
            {
                turn_rows.push(Row::Recap { text: &recap.text });
            }
            ChatTurn {
                key,
                prompt: &turn.prompt,
                rows: turn_rows,
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
        tray: tray(page),
        question: last
            .and_then(|turn| turn.question.as_ref())
            .map(|question| QuestionCard {
                text: &question.text,
                options: &question.options,
            }),
    }
}

/// A run of thinking and tool calls: becomes one [`Row::Activity`].
#[derive(Default)]
struct Group<'a> {
    first: Option<usize>,
    children: Vec<Child<'a>>,
    counts: Counts,
    thinking: bool,
    thinking_ms: Option<u64>,
}

impl<'a> Group<'a> {
    fn thinking(
        &mut self,
        turn_key: u64,
        item: usize,
        text: &'a str,
        ms: Option<u64>,
        open: &HashSet<RowKey>,
    ) {
        let key = (turn_key, item, 0);
        self.first.get_or_insert(item);
        self.thinking = true;
        if let Some(ms) = ms {
            self.thinking_ms = Some(self.thinking_ms.unwrap_or(0) + ms);
        }
        self.children.push(Child::Thinking {
            key,
            text,
            ms,
            open: !text.is_empty() && open.contains(&key),
        });
    }

    fn tools(&mut self, turn_key: u64, item: usize, calls: &'a [ToolCall], open: &HashSet<RowKey>) {
        self.first.get_or_insert(item);
        for (index, call) in calls.iter().enumerate() {
            let key = (turn_key, item, index);
            self.counts.add(&call.name);
            self.children.push(Child::Tool {
                key,
                call,
                open: open.contains(&key),
            });
        }
    }

    /// Ends the run (nothing when it is empty). `running`: the turn runs and this row is its
    /// last.
    fn flush(
        &mut self,
        turn_key: u64,
        open: &HashSet<RowKey>,
        running: bool,
        rows: &mut Vec<Row<'a>>,
    ) {
        let group = std::mem::take(self);
        let Some(first) = group.first else {
            return;
        };
        let key = (turn_key, first, usize::MAX);
        let (thought, summary) = if running {
            (Some("Working…".to_owned()), String::new())
        } else {
            (
                thought_label(group.thinking, group.thinking_ms, &group.counts),
                group.counts.label(),
            )
        };
        rows.push(Row::Activity {
            key,
            thinking_ms: group.thinking_ms,
            counts: group.counts,
            thought,
            summary,
            children: group.children,
            open: open.contains(&key),
        });
    }
}

fn rows<'a>(
    turn_key: u64,
    turn: &'a PageTurn,
    open: &HashSet<RowKey>,
    running: bool,
) -> Vec<Row<'a>> {
    let mut rows = Vec::new();
    let mut group = Group::default();
    for (item, entry) in turn.items.iter().enumerate() {
        match entry {
            PageItem::Text(_) => {
                group.flush(turn_key, open, false, &mut rows);
                rows.push(Row::Text { item });
            }
            PageItem::Thinking { text, ms } => group.thinking(turn_key, item, text, *ms, open),
            PageItem::Tools(calls) => group.tools(turn_key, item, calls, open),
        }
    }
    group.flush(turn_key, open, running, &mut rows);
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

/// A subagent that still runs: one of the newest turn, or a background one of an older turn
/// (a foreground call of an older turn that never got its result was cut off, not running).
fn runs(subagent: &Subagent, newest_turn: bool) -> bool {
    !subagent.done && (newest_turn || subagent.background)
}

/// Whether a subagent of the kept turns still runs.
pub fn subagent_runs(page: &Page) -> bool {
    let last = page.turns.len().saturating_sub(1);
    page.turns
        .iter()
        .enumerate()
        .any(|(index, turn)| (turn.subagents.iter()).any(|subagent| runs(subagent, index == last)))
}

/// Whether `text` is a tool or thinking body that was cut (the reader's marker ends it).
pub fn body_cut(text: &str) -> bool {
    text.ends_with(mkrew_core::thread::CUT)
}

/// While a subagent runs: the running ones of the kept turns, and the newest turn's finished
/// ones beside them. Nothing once none runs.
fn tray(page: &Page) -> Vec<Chip<'_>> {
    if !subagent_runs(page) {
        return Vec::new();
    }
    let last = page.turns.len().saturating_sub(1);
    page.turns
        .iter()
        .enumerate()
        .flat_map(|(index, turn)| {
            let newest = index == last;
            turn.subagents
                .iter()
                .filter(move |subagent| newest || runs(subagent, false))
        })
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

/// The status chips of a Chat composer (ADR 0105); a field with no data is `None`: its chip is
/// hidden, never shown as 0.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ComposerStats {
    /// Short model name ([`short_model`]).
    pub model: Option<String>,
    pub effort: Option<String>,
    pub permission: Option<String>,
    pub branch: Option<BranchChip>,
    /// Tokens in the context, and the window when known.
    pub context: Option<(u64, Option<u64>)>,
    pub tok_s: Option<u32>,
    pub elapsed_ms: Option<u64>,
    /// The local wall-clock time (`09:44:27`) the newest turn ended; `None` while working.
    pub ended_clock: Option<String>,
}

/// The branch chip: `name` and `*changed` files.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BranchChip {
    pub name: String,
    pub changed: usize,
}

/// `09:44:27`: the local wall-clock time of Unix time `at` (seconds). `utc_offset(t)` is the
/// local offset in seconds east of UTC at `t`.
pub fn clock_hms(at: i64, utc_offset: &dyn Fn(i64) -> i32) -> String {
    let rest = (at + i64::from(utc_offset(at))).rem_euclid(86_400);
    format!(
        "{:02}:{:02}:{:02}",
        rest / 3600,
        rest % 3600 / 60,
        rest % 60
    )
}

/// The chips of the session behind `page`. Model and context come from the transcript's
/// `SessionMeta`, else from the usage event; the profile's context window wins over the
/// transcript's. tok/s and elapsed time are the newest turn's: elapsed runs to `now` (epoch ms)
/// while `working`; the end clock shows once the turn ended (local time via `utc_offset`).
/// The branch shows only when the session runs at the workspace root.
pub fn composer_stats(
    page: Option<&Page>,
    usage: Option<&SessionUsageEvent>,
    git: Option<&GitSummary>,
    at_root: bool,
    working: bool,
    now: i64,
    utc_offset: &dyn Fn(i64) -> i32,
) -> ComposerStats {
    let meta = page.map(|page| &page.meta);
    let last = page.and_then(|page| page.turns.last());
    let model = meta
        .and_then(|meta| meta.model.as_deref())
        .or_else(|| usage.and_then(|usage| usage.model.as_deref()))
        .map(short_model);
    let used = meta.and_then(|meta| meta.context).or_else(|| {
        usage
            .map(|usage| usage.context)
            .filter(|context| *context > 0)
    });
    let window = usage
        .and_then(|usage| usage.context_window)
        .or_else(|| meta.and_then(|meta| meta.context_window));
    let tok_s = last
        .filter(|turn| turn.generation_ms > 0 && turn.output_tokens > 0)
        .map(|turn| {
            let rate = (turn.output_tokens * 1000 + turn.generation_ms / 2) / turn.generation_ms;
            u32::try_from(rate).unwrap_or(u32::MAX)
        });
    let elapsed_ms = last.and_then(|turn| {
        let start = turn.started_at?;
        let end = if working { now } else { turn.ended_at? };
        Some(u64::try_from((end - start).max(0)).unwrap_or(0))
    });
    ComposerStats {
        model,
        effort: meta.and_then(|meta| meta.effort.clone()),
        permission: meta.and_then(|meta| meta.permission.clone()),
        branch: git.filter(|_| at_root).and_then(|git| {
            Some(BranchChip {
                name: git.branch_label()?,
                changed: git.changed(),
            })
        }),
        context: used.map(|used| (used, window)),
        tok_s,
        elapsed_ms,
        ended_clock: last
            .filter(|_| !working)
            .and_then(|turn| turn.ended_at)
            .map(|ended| clock_hms(ended / 1000, utc_offset)),
    }
}

/// The stats of the chip row in order: `74K`, `102 tok/s`, `2m 46s`; those known.
pub fn stat_parts(stats: &ComposerStats) -> Vec<String> {
    let mut parts = Vec::new();
    if let Some((used, _)) = stats.context {
        parts.push(format_k(used));
    }
    if let Some(rate) = stats.tok_s {
        parts.push(format!("{rate} tok/s"));
    }
    if let Some(ms) = stats.elapsed_ms {
        parts.push(format_elapsed(ms));
    }
    parts
}

/// The thin context meter above the composer card.
#[derive(Debug, Clone, PartialEq)]
pub struct Meter {
    /// Used share, whole percent (can pass 100).
    pub percent: u64,
    /// Used share for the filled track, 0 to 1.
    pub fraction: f32,
    /// From `usage::WARNING_PERCENT` on.
    pub warning: bool,
    /// The window as `usage::format_tokens` writes it.
    pub window_label: String,
}

/// `None` without a window.
pub fn context_meter(used: u64, window: u64) -> Option<Meter> {
    if window == 0 {
        return None;
    }
    let percent = used.saturating_mul(100) / window;
    Some(Meter {
        percent,
        fraction: (used as f32 / window as f32).clamp(0.0, 1.0),
        warning: percent >= usage::WARNING_PERCENT,
        window_label: usage::format_tokens(window),
    })
}

/// Longest session title the placeholder shows.
const PLACEHOLDER_MAX: usize = 60;

/// The composer's placeholder: the transcript's title, else the pane's, in quotes; else
/// `Message the agent…`.
pub fn composer_placeholder(transcript_title: Option<&str>, pane_title: Option<&str>) -> String {
    let title = [transcript_title, pane_title]
        .into_iter()
        .flatten()
        .map(str::trim)
        .find(|title| !title.is_empty());
    match title {
        Some(title) if title.chars().count() > PLACEHOLDER_MAX => {
            let cut: String = title.chars().take(PLACEHOLDER_MAX - 1).collect();
            format!("\"{cut}…\"")
        }
        Some(title) => format!("\"{title}\""),
        None => "Message the agent…".to_owned(),
    }
}

/// Which chips of the Chat composer show.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Chips {
    pub model: bool,
    pub effort: bool,
    pub permission: bool,
    pub branch: bool,
    pub stats: bool,
    /// The local time the last turn ended.
    pub clock: bool,
}

impl Chips {
    /// The chips `stats` has data for.
    pub fn wanted(stats: &ComposerStats) -> Self {
        Self {
            model: stats.model.is_some(),
            effort: stats.effort.is_some(),
            permission: stats.permission.is_some(),
            branch: stats.branch.is_some(),
            stats: !stat_parts(stats).is_empty(),
            clock: stats.ended_clock.is_some(),
        }
    }
}

/// What of `want` fits in `room` px: chips drop from the right, in the order clock, stats,
/// permission, effort, branch; the model chip and the fixed part (send, ✦, Prompts) stay.
pub fn fit_chips(want: Chips, room: f32) -> Chips {
    let width = |chips: &Chips| {
        theme::COMPOSER_FIXED_WIDTH
            + [
                (chips.model, theme::COMPOSER_CHIP_MODEL_WIDTH),
                (chips.effort, theme::COMPOSER_CHIP_EFFORT_WIDTH),
                (chips.permission, theme::COMPOSER_CHIP_PERMISSION_WIDTH),
                (chips.branch, theme::COMPOSER_CHIP_BRANCH_WIDTH),
                (chips.stats, theme::COMPOSER_CHIP_STATS_WIDTH),
                (chips.clock, theme::COMPOSER_CHIP_CLOCK_WIDTH),
            ]
            .into_iter()
            .filter(|(shown, _)| *shown)
            .map(|(_, width)| width)
            .sum::<f32>()
    };
    let drops: [fn(&mut Chips); 5] = [
        |chips| chips.clock = false,
        |chips| chips.stats = false,
        |chips| chips.permission = false,
        |chips| chips.effort = false,
        |chips| chips.branch = false,
    ];
    let mut chips = want;
    for drop in drops {
        if width(&chips) <= room {
            break;
        }
        drop(&mut chips);
    }
    chips
}

/// The Files path (`/` separators, relative to the workspace folder `base`) a path pill names,
/// or `None` when it leaves the workspace (an absolute path elsewhere, or any `..`). Pure: it
/// reads no disk, so it works for a machine window too. Folder and drive case do not matter.
pub fn files_path(base: &str, path: &str) -> Option<String> {
    let path = path.replace('\\', "/");
    let path = path.strip_prefix("./").unwrap_or(&path);
    let clean = |rest: &str| {
        (!rest.is_empty() && !rest.split('/').any(|part| part == "..")).then(|| rest.to_owned())
    };
    let absolute = path.starts_with('/') || path.as_bytes().get(1) == Some(&b':');
    if !absolute {
        return clean(path);
    }
    let base = base.replace('\\', "/");
    let base = base.trim_end_matches('/');
    let inside = path
        .get(..base.len())
        .is_some_and(|head| head.eq_ignore_ascii_case(base));
    let rest = path.get(base.len()..).filter(|_| inside)?;
    clean(rest.strip_prefix('/')?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use mkrew_core::UsageFormat;
    use mkrew_core::thread::CHAT_TURNS;

    /// The whole label of an activity row: the italic part, then ` · ` and the counts.
    fn activity_label(thought: Option<&str>, summary: &str) -> String {
        match (thought, summary.is_empty()) {
            (Some(thought), true) => thought.to_owned(),
            (Some(thought), false) => format!("{thought} · {summary}"),
            (None, _) => summary.to_owned(),
        }
    }

    use crate::docs::Shown;
    use crate::preview_select::{Point, Selection};
    use crate::thread_view::ThreadFile;

    use mkrew_protocol::SessionUsageEvent;

    use crate::git_summary::{GitSummary, RepoSummary};
    use crate::thread_view::PageTurn;

    const CLAUDE_TIMING: &str =
        include_str!("../../mkrew-core/tests/fixtures/thread/claude_timing.jsonl");
    const CODEX_TIMING: &str =
        include_str!("../../mkrew-core/tests/fixtures/thread/codex_timing.jsonl");
    const OMP_TIMING: &str =
        include_str!("../../mkrew-core/tests/fixtures/thread/omp_timing.jsonl");
    /// 2026-01-01T00:00:00Z in epoch milliseconds.
    const T0: i64 = 1_767_225_600_000;

    fn page_in(format: UsageFormat, jsonl: &str) -> Page {
        let mut file = ThreadFile::new(&format, CHAT_TURNS).unwrap();
        file.apply(0, jsonl.as_bytes());
        file.page()
    }

    /// `super::composer_stats` in UTC.
    fn composer_stats(
        page: Option<&Page>,
        usage: Option<&SessionUsageEvent>,
        git: Option<&GitSummary>,
        at_root: bool,
        working: bool,
        now: i64,
    ) -> ComposerStats {
        super::composer_stats(page, usage, git, at_root, working, now, &|_| 0)
    }

    fn call(name: &str) -> ToolCall {
        ToolCall {
            id: String::new(),
            name: name.into(),
            target: String::new(),
            input: String::new(),
            result: None,
        }
    }

    /// A one-turn page of `items`.
    fn turn_page(items: Vec<PageItem>) -> Page {
        Page {
            turns: vec![PageTurn {
                items,
                ..PageTurn::default()
            }],
            ..Page::default()
        }
    }

    fn think(ms: Option<u64>) -> PageItem {
        PageItem::Thinking {
            text: "hm".into(),
            ms,
        }
    }

    fn words(text: &str) -> PageItem {
        PageItem::Text(Shown::from_text(text))
    }

    fn usage(context: u64, window: Option<u64>, model: Option<&str>) -> SessionUsageEvent {
        SessionUsageEvent {
            session_id: Ulid::new(),
            input: 0,
            output: 0,
            cache_read: 0,
            cache_write: 0,
            context,
            context_window: window,
            model: model.map(str::to_owned),
            transcript: None,
        }
    }

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

    const CLAUDE_BACKGROUND: &str =
        include_str!("../../mkrew-core/tests/fixtures/thread/claude_background.jsonl");

    #[test]
    fn the_tray_holds_every_running_subagent_of_the_kept_turns() {
        // A second prompt came before any notification: the three launched in turn 0 run.
        let before = page(&head(CLAUDE_BACKGROUND, 7));
        let v = view(Some(&before), &HashSet::new(), None, false);
        let names = |v: &View<'_>| -> Vec<String> {
            v.tray
                .iter()
                .map(|c| format!("{} {}", c.description, if c.done { "done" } else { "runs" }))
                .collect()
        };
        assert_eq!(
            names(&v),
            ["Review A runs", "Review B runs", "Review C runs"]
        );
        assert!(subagent_runs(&before));
        // After the notifications only Review B still runs.
        let after = page(CLAUDE_BACKGROUND);
        let v = view(Some(&after), &HashSet::new(), None, false);
        assert_eq!(names(&v), ["Review B runs"]);
        assert!(subagent_runs(&after));
        // Once it is notified too, the tray goes.
        let ended = format!(
            "{CLAUDE_BACKGROUND}{{\"type\":\"user\",\"message\":{{\"role\":\"user\",\"content\":\"<task-notification>\\n<task-id>a222</task-id>\\n<status>completed</status>\\n</task-notification>\"}}}}\n"
        );
        let ended = page(&ended);
        assert!(
            view(Some(&ended), &HashSet::new(), None, false)
                .tray
                .is_empty()
        );
        assert!(!subagent_runs(&ended));
    }

    #[test]
    fn a_body_ends_cut_with_the_marker() {
        assert!(body_cut("some text… (cut)"));
        assert!(!body_cut("some text"));
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
                Row::Activity {
                    thought,
                    summary,
                    open,
                    children,
                    ..
                } => format!(
                    "activity [{}] {} {}",
                    activity_label(thought.as_deref(), summary),
                    state(open),
                    children.len()
                ),
                Row::Recap { text } => format!("recap {text}"),
            })
            .collect()
    }

    #[test]
    fn thinking_and_tools_between_two_texts_are_one_activity_row() {
        let page = page(CLAUDE_CHAT);
        let shut = view(Some(&page), &HashSet::new(), None, false);
        assert_eq!(
            labels(&shut.turns[0]),
            [
                "activity [Thought] shut 1",
                "text 1",
                "activity [1 command, 1 read] shut 2",
                "text 3"
            ]
        );
        // Opening the second activity shows its calls; a call opens on its own key.
        let open: HashSet<RowKey> = [(0, 2, usize::MAX), (0, 2, 0)].into_iter().collect();
        let opened = view(Some(&page), &open, None, false);
        assert_eq!(
            labels(&opened.turns[0]),
            [
                "activity [Thought] shut 1",
                "text 1",
                "activity [1 command, 1 read] open 2",
                "text 3"
            ]
        );
        let Row::Activity { children, .. } = &opened.turns[0].rows[2] else {
            panic!("an activity row");
        };
        assert!(matches!(
            &children[0],
            Child::Tool {
                open: true,
                key: (0, 2, 0),
                ..
            }
        ));
        assert!(matches!(
            &children[1],
            Child::Tool {
                open: false,
                key: (0, 2, 1),
                ..
            }
        ));
        assert_eq!(shut.turns[0].key, 0);
        assert!(!shut.earlier);
    }

    #[test]
    fn activity_groups_cover_thinking_only_tools_only_mixed_and_the_turn_end() {
        let page = turn_page(vec![
            think(Some(4_000)),
            PageItem::Tools(vec![call("Bash"), call("bash"), call("Read")]),
            words("answer"),
            PageItem::Tools(vec![call("Edit")]),
            words("more"),
            think(Some(2_000)),
            think(Some(3_000)),
            PageItem::Tools(vec![call("mystery")]),
        ]);
        let v = view(Some(&page), &HashSet::new(), None, false);
        assert_eq!(
            labels(&v.turns[0]),
            [
                "activity [Thought for 4s · 2 commands, 1 read] shut 4",
                "text 2",
                "activity [1 edit] shut 1",
                "text 4",
                "activity [Thought for 5s · 1 tool] shut 3",
            ]
        );
        let Row::Activity {
            thinking_ms, key, ..
        } = &v.turns[0].rows[4]
        else {
            panic!("an activity row");
        };
        assert_eq!((*thinking_ms, *key), (Some(5_000), (0, 5, usize::MAX)));
    }

    #[test]
    fn counts_sort_tools_into_kinds_with_plural_rules() {
        let mut counts = Counts::default();
        for name in [
            "Bash",
            "shell_command",
            "exec_command",
            "Read",
            "Grep",
            "glob",
            "Edit",
            "MultiEdit",
            "apply_patch",
            "WebFetch",
            "web_search",
            "Task",
            "Agent",
            "TodoWrite",
        ] {
            counts.add(name);
        }
        assert_eq!(
            counts,
            Counts {
                commands: 3,
                reads: 3,
                edits: 3,
                web: 2,
                agents: 2,
                other: 1
            }
        );
        assert_eq!(
            counts.label(),
            "3 commands, 3 reads, 3 edits, 2 web calls, 2 agents, 1 tool"
        );
        let mut one = Counts::default();
        one.add("edit");
        assert_eq!(one.label(), "1 edit");
        assert!(Counts::default().is_empty());
    }

    #[test]
    fn a_redacted_thinking_row_does_not_open_and_a_long_thought_reads_in_minutes() {
        let page = turn_page(vec![PageItem::Thinking {
            text: String::new(),
            ms: Some(166_000),
        }]);
        let open: HashSet<RowKey> = [(0, 0, 0), (0, 0, usize::MAX)].into_iter().collect();
        let v = view(Some(&page), &open, None, false);
        let Row::Activity {
            children, thought, ..
        } = &v.turns[0].rows[0]
        else {
            panic!("an activity row");
        };
        assert_eq!(thought.as_deref(), Some("Thought for 2m 46s"));
        assert!(matches!(&children[0], Child::Thinking { open: false, .. }));
        assert_eq!(thinking_label(Some(500)), "Thought for 1s");
        assert_eq!(thinking_label(None), "Thinking");
    }

    #[test]
    fn the_running_turn_ends_with_a_working_row_only_while_its_last_row_is_activity() {
        let ends_in_activity = turn_page(vec![words("a"), PageItem::Tools(vec![call("Bash")])]);
        let working = view(Some(&ends_in_activity), &HashSet::new(), None, true);
        assert_eq!(
            labels(&working.turns[0]),
            ["text 0", "activity [Working…] shut 1"]
        );
        let idle = view(Some(&ends_in_activity), &HashSet::new(), None, false);
        assert_eq!(labels(&idle.turns[0])[1], "activity [1 command] shut 1");
        // The row is not last when text follows it; an older turn is never the running one.
        let ends_in_text = turn_page(vec![PageItem::Tools(vec![call("Bash")]), words("a")]);
        let v = view(Some(&ends_in_text), &HashSet::new(), None, true);
        assert_eq!(labels(&v.turns[0])[0], "activity [1 command] shut 1");
        let mut two = turn_page(vec![PageItem::Tools(vec![call("Bash")])]);
        two.turns.push(PageTurn::default());
        let v = view(Some(&two), &HashSet::new(), None, true);
        assert_eq!(labels(&v.turns[0]), ["activity [1 command] shut 1"]);
    }

    fn recap(at: i64) -> Recap {
        Recap {
            text: "Fixed the parser.".into(),
            at,
        }
    }

    #[test]
    fn the_recap_shows_only_when_fresh_and_idle() {
        let page = page(CLAUDE_TIMING);
        let start = T0 / 1000;
        let recap_last = |r: &Recap, working| {
            matches!(
                view(Some(&page), &HashSet::new(), Some(r), working).turns[0]
                    .rows
                    .last(),
                Some(Row::Recap {
                    text: "Fixed the parser."
                })
            )
        };
        let fresh = recap(start + 60);
        assert!(recap_last(&fresh, false));
        assert!(!recap_last(&fresh, true), "working");
        assert!(
            !recap_last(&recap(start - 1), false),
            "older than the prompt"
        );
        assert!(
            recap_shown(&recap(start), None, false),
            "a prompt without a time"
        );
        // It follows the last turn only, and an empty page has none.
        let v = view(None, &HashSet::new(), Some(&fresh), false);
        assert!(v.turns.is_empty());
    }

    #[test]
    fn elapsed_times_and_token_counts_read_short() {
        assert_eq!(format_elapsed(0), "0s");
        assert_eq!(format_elapsed(42_000), "42s");
        assert_eq!(format_elapsed(59_999), "59s");
        assert_eq!(format_elapsed(60_000), "1m 00s");
        assert_eq!(format_elapsed(166_000), "2m 46s");
        assert_eq!(format_elapsed(3_780_000), "1h 03m");
        assert_eq!(format_k(950), "950");
        assert_eq!(format_k(1_234), "1.2K");
        assert_eq!(format_k(74_000), "74K");
        assert_eq!(format_k(1_500_000), "1.5M");
    }

    #[test]
    fn composer_stats_come_from_the_meta_the_newest_turn_and_the_usage_event() {
        let claude = page_in(UsageFormat::ClaudeJsonl, CLAUDE_TIMING);
        let idle = composer_stats(Some(&claude), None, None, true, false, T0 + 99_000);
        assert_eq!(idle.model.as_deref(), Some("opus 5.5"));
        // The fixture's later `permission-mode` line (auto) wins over the prompt's.
        assert_eq!(idle.permission.as_deref(), Some("auto"));
        assert_eq!(idle.effort, None);
        assert_eq!(idle.context, Some((205, None)));
        // 60 tokens over 6 s of writing.
        assert_eq!(idle.tok_s, Some(10));
        // Finished: first line to last line of the turn.
        assert_eq!(idle.elapsed_ms, Some(12_000));
        // Running: from the prompt to now.
        let live = composer_stats(Some(&claude), None, None, true, true, T0 + 90_000);
        assert_eq!(live.elapsed_ms, Some(90_000));
        // The profile's window wins over the transcript's (Codex writes one).
        let codex = page_in(UsageFormat::CodexJsonl, CODEX_TIMING);
        let from_transcript = composer_stats(Some(&codex), None, None, true, false, T0);
        assert_eq!(from_transcript.context, Some((1_900, Some(258_400))));
        let event = usage(5, Some(200_000), None);
        let from_profile = composer_stats(Some(&codex), Some(&event), None, true, false, T0);
        assert_eq!(from_profile.context, Some((1_900, Some(200_000))));
        // 80 tokens over 3.5 s is 22.86 tok/s: rounded to 23; omp 22.5 rounds up to 23 too.
        assert_eq!(from_transcript.tok_s, Some(23));
        let omp = page_in(UsageFormat::OmpJsonl, OMP_TIMING);
        assert_eq!(
            composer_stats(Some(&omp), None, None, true, false, T0).tok_s,
            Some(23)
        );
        assert_eq!(
            composer_stats(Some(&omp), None, None, true, false, T0)
                .effort
                .as_deref(),
            Some("high")
        );
    }

    #[test]
    fn composer_stats_fall_back_to_the_usage_event_and_hide_what_is_unknown() {
        let nothing = composer_stats(None, None, None, true, false, T0);
        assert_eq!(nothing, ComposerStats::default());
        let event = usage(74_000, Some(200_000), Some("anthropic/claude-opus-5-5"));
        let from_event = composer_stats(None, Some(&event), None, true, false, T0);
        assert_eq!(from_event.model.as_deref(), Some("opus 5.5"));
        assert_eq!(from_event.context, Some((74_000, Some(200_000))));
        assert_eq!((from_event.tok_s, from_event.elapsed_ms), (None, None));
        // A usage event that has no context yet is no context.
        let empty = usage(0, Some(200_000), None);
        assert_eq!(
            composer_stats(None, Some(&empty), None, true, false, T0).context,
            None
        );
        // No tokens or no writing time: no tok/s.
        let mut flat = turn_page(vec![]);
        flat.turns[0].output_tokens = 10;
        assert_eq!(
            composer_stats(Some(&flat), None, None, true, false, T0).tok_s,
            None
        );
        flat.turns[0].generation_ms = 1_000;
        flat.turns[0].output_tokens = 0;
        assert_eq!(
            composer_stats(Some(&flat), None, None, true, false, T0).tok_s,
            None
        );
    }

    /// The card's chip room at a 900 px pane: the 760 px column less page and card padding.
    const ROOM_900: f32 = theme::CHAT_COLUMN_WIDTH - 2.0 * theme::space::XL - 2.0 * theme::space::M;

    #[test]
    fn omp_and_codex_sessions_show_their_stats_at_900_px() {
        let git = GitSummary {
            repos: vec![RepoSummary {
                repo: String::new(),
                branch: Some("master".into()),
                head: None,
                modified: 1,
                untracked: 0,
            }],
        };
        for (format, jsonl) in [
            (UsageFormat::OmpJsonl, OMP_TIMING),
            (UsageFormat::CodexJsonl, CODEX_TIMING),
        ] {
            let name = format!("{format:?}");
            let page = page_in(format, jsonl);
            let stats = composer_stats(Some(&page), None, Some(&git), true, false, T0);
            let want = Chips::wanted(&stats);
            assert!(want.stats && want.clock && want.branch, "{name}: {stats:?}");
            let shown = fit_chips(want, ROOM_900);
            assert!(shown.stats && shown.clock, "{name}: dropped {shown:?}");
            assert_eq!(stat_parts(&stats).len(), 3, "{name}: {stats:?}");
        }
    }

    #[test]
    fn the_branch_chip_shows_only_at_the_workspace_root() {
        let git = GitSummary {
            repos: vec![RepoSummary {
                repo: String::new(),
                branch: Some("main".into()),
                head: None,
                modified: 2,
                untracked: 1,
            }],
        };
        let at = |root| composer_stats(None, None, Some(&git), root, false, T0).branch;
        assert_eq!(
            at(true),
            Some(BranchChip {
                name: "main".into(),
                changed: 3
            })
        );
        assert_eq!(at(false), None, "a task worktree has no branch chip");
        assert_eq!(
            composer_stats(None, None, None, true, false, T0).branch,
            None
        );
    }

    #[test]
    fn a_pick_updates_the_default_only_when_it_differs() {
        assert_eq!(default_after_pick(false, true), Some(true));
        assert_eq!(default_after_pick(true, false), Some(false));
        assert_eq!(default_after_pick(true, true), None);
        assert_eq!(default_after_pick(false, false), None);
    }

    #[test]
    fn the_stats_text_lists_only_what_is_known() {
        let stats = ComposerStats {
            context: Some((74_000, Some(200_000))),
            tok_s: Some(102),
            elapsed_ms: Some(166_000),
            ..ComposerStats::default()
        };
        assert_eq!(stat_parts(&stats), ["74K", "102 tok/s", "2m 46s"]);
        assert!(stat_parts(&ComposerStats::default()).is_empty());
    }

    #[test]
    fn the_end_clock_is_the_local_time_the_last_turn_ended() {
        let utc = |_: i64| 0;
        assert_eq!(clock_hms(0, &utc), "00:00:00");
        assert_eq!(clock_hms(35_067, &utc), "09:44:27");
        // Seven hours east; a day boundary wraps.
        assert_eq!(clock_hms(35_067, &|_| 25_200), "16:44:27");
        assert_eq!(clock_hms(86_399, &|_| 3_600), "00:59:59");
        // West of UTC before the epoch still lands in 0..24 h.
        assert_eq!(clock_hms(60, &|_| -3_600), "23:01:00");

        let mut page = turn_page(vec![]);
        page.turns[0].ended_at = Some(T0 + 35_067_000);
        let at = |page: &Page, working, offset: i32| {
            super::composer_stats(Some(page), None, None, true, working, T0, &move |_| offset)
                .ended_clock
        };
        assert_eq!(at(&page, false, 0).as_deref(), Some("09:44:27"));
        assert_eq!(at(&page, false, 25_200).as_deref(), Some("16:44:27"));
        assert_eq!(at(&page, true, 0), None, "a working turn has not ended");
        page.turns[0].ended_at = None;
        assert_eq!(at(&page, false, 0), None);
    }

    #[test]
    fn the_context_meter_warns_from_the_usage_threshold() {
        assert_eq!(context_meter(10, 0), None);
        let calm = context_meter(74_000, 200_000).unwrap();
        assert_eq!((calm.percent, calm.warning), (37, false));
        assert_eq!(calm.window_label, "200.0k tok");
        let hot = context_meter(170_000, 200_000).unwrap();
        assert_eq!((hot.percent, hot.warning), (85, true));
        let over = context_meter(300_000, 200_000).unwrap();
        assert!(
            (over.fraction - 1.0).abs() < f32::EPSILON,
            "never past the track"
        );
    }

    #[test]
    fn the_placeholder_is_the_session_title_in_quotes() {
        assert_eq!(
            composer_placeholder(Some("Fix login"), Some("pane")),
            "\"Fix login\""
        );
        assert_eq!(
            composer_placeholder(None, Some(" pane title ")),
            "\"pane title\""
        );
        assert_eq!(composer_placeholder(Some("  "), None), "Message the agent…");
        assert_eq!(composer_placeholder(None, None), "Message the agent…");
        let long = composer_placeholder(Some(&"x".repeat(200)), None);
        assert_eq!(long.chars().count(), 60 + 2);
    }

    #[test]
    fn chips_drop_from_the_right_in_order() {
        use crate::theme::{
            COMPOSER_CHIP_BRANCH_WIDTH as BRANCH, COMPOSER_CHIP_EFFORT_WIDTH as EFFORT,
            COMPOSER_CHIP_MODEL_WIDTH as MODEL, COMPOSER_CHIP_PERMISSION_WIDTH as PERMISSION,
            COMPOSER_CHIP_STATS_WIDTH as STATS, COMPOSER_FIXED_WIDTH as FIXED,
        };
        let all = Chips {
            model: true,
            effort: true,
            permission: true,
            branch: true,
            stats: true,
            clock: true,
        };
        assert_eq!(fit_chips(all, 10_000.0), all);
        let room = FIXED + MODEL + EFFORT + PERMISSION + BRANCH + STATS;
        assert_eq!(
            fit_chips(all, room),
            Chips {
                clock: false,
                ..all
            }
        );
        // The clock goes first, then the stats.
        assert_eq!(
            fit_chips(all, room - 1.0),
            Chips {
                clock: false,
                stats: false,
                ..all
            }
        );
        assert_eq!(
            fit_chips(all, FIXED + MODEL + EFFORT + BRANCH),
            Chips {
                clock: false,
                stats: false,
                permission: false,
                ..all
            }
        );
        assert_eq!(
            fit_chips(all, FIXED + MODEL + BRANCH),
            Chips {
                clock: false,
                stats: false,
                permission: false,
                effort: false,
                ..all
            }
        );
        // The branch goes last of the four; the model chip never goes.
        let narrow = fit_chips(all, 0.0);
        assert_eq!(
            narrow,
            Chips {
                model: true,
                effort: false,
                permission: false,
                branch: false,
                stats: false,
                clock: false,
            }
        );
        // A chip that is not wanted costs nothing.
        let few = Chips {
            effort: false,
            permission: false,
            branch: false,
            stats: false,
            clock: false,
            ..all
        };
        assert_eq!(fit_chips(few, FIXED + MODEL), few);
    }

    #[test]
    fn files_paths_stay_inside_the_workspace() {
        let base = "C:\\Users\\me\\repo";
        assert_eq!(files_path(base, "src\\a.rs").as_deref(), Some("src/a.rs"));
        assert_eq!(files_path(base, "./src/a.rs").as_deref(), Some("src/a.rs"));
        assert_eq!(
            files_path(base, "C:\\Users\\me\\repo\\crates\\x.rs").as_deref(),
            Some("crates/x.rs")
        );
        assert_eq!(
            files_path(base, "c:/users/me/repo/crates/x.rs").as_deref(),
            Some("crates/x.rs"),
            "drive and folder case do not matter"
        );
        assert_eq!(
            files_path("/home/me/repo/", "/home/me/repo/a.rs").as_deref(),
            Some("a.rs")
        );
        assert_eq!(files_path(base, "C:\\Users\\me\\other\\a.rs"), None);
        assert_eq!(
            files_path(base, "C:\\Users\\me\\repository\\a.rs"),
            None,
            "a longer folder name"
        );
        assert_eq!(files_path(base, "..\\secret.rs"), None);
        assert_eq!(files_path(base, "src/../../x.rs"), None);
        assert_eq!(
            files_path(base, "C:\\Users\\me\\repo"),
            None,
            "the folder itself"
        );
        assert_eq!(files_path(base, ""), None);
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
        let v = view(Some(&page), &HashSet::new(), None, false);
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
            plan,
            ..PageTurn::default()
        };
        let page = Page {
            turns: vec![turn(Some(vec![step])), turn(Some(Vec::new())), turn(None)],
            truncated: false,
            first_turn: 0,
            ..Page::default()
        };
        let v = view(Some(&page), &HashSet::new(), None, false);
        assert!(v.turns.iter().all(|t| t.plan.is_none()));
    }

    #[test]
    fn plan_marks_follow_the_statuses() {
        let agents = page(CLAUDE_AGENTS);
        let v = view(Some(&agents), &HashSet::new(), None, false);
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
        let pending = view(Some(&pending_page), &HashSet::new(), None, false);
        assert_eq!(
            pending.question,
            Some(QuestionCard {
                text: "Which database?",
                options: &["SQLite".to_owned(), "Postgres".to_owned()]
            })
        );
        // The whole fixture: q1 is still unanswered but is not the last turn; q2 is answered.
        let all_page = page(CLAUDE_AGENTS);
        let all = view(Some(&all_page), &HashSet::new(), None, false);
        assert_eq!(all.question, None);
    }

    #[test]
    fn the_tray_shows_while_a_subagent_runs_and_goes_when_all_are_done() {
        let running_page = page(&head(CLAUDE_AGENTS, 7));
        let running = view(Some(&running_page), &HashSet::new(), None, false);
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
        assert!(
            view(Some(&done_page), &HashSet::new(), None, false)
                .tray
                .is_empty()
        );
        // The whole fixture: s2 never got a result, but it is a foreground call of an older turn.
        let all_page = page(CLAUDE_AGENTS);
        assert!(
            view(Some(&all_page), &HashSet::new(), None, false)
                .tray
                .is_empty()
        );
        assert!(!subagent_runs(&all_page));
    }

    #[test]
    fn no_page_or_no_turns_is_an_empty_view_and_truncation_is_reported() {
        assert_eq!(view(None, &HashSet::new(), None, false), View::default());
        let empty = Page {
            turns: Vec::new(),
            truncated: true,
            first_turn: 3,
            ..Page::default()
        };
        let v = view(Some(&empty), &HashSet::new(), None, false);
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
