//! The session panel right of Terminals (SPEC §10, ADR 0038): its width, the Stop rule and
//! the text of the Session card. Pure; `main.rs` draws it.

use mkrew_protocol::{Session, SessionKind, SessionUsageEvent};
use ulid::Ulid;

use crate::git_summary::{GitSummary, RepoSummary};
use crate::lifecycle::{self, CloseAction};
use crate::usage::{self, UsageFields};
use crate::{history, status, theme};

/// `[ui] inspector_width` as drawn: kept between the limits.
pub fn saved_width(px: u16) -> f32 {
    f32::from(px).clamp(theme::INSPECTOR_MIN_WIDTH, theme::INSPECTOR_MAX_WIDTH)
}

/// The width while the edge is dragged to `pointer_x` (window coordinates): the panel is flush
/// with the right edge. At most half of the window, so the terminals keep room.
pub fn width_from_pointer(window_width: f32, pointer_x: f32) -> f32 {
    let max = theme::INSPECTOR_MAX_WIDTH
        .min(window_width / 2.0)
        .max(theme::INSPECTOR_MIN_WIDTH);
    (window_width - pointer_x).clamp(theme::INSPECTOR_MIN_WIDTH, max)
}

/// What the card's Stop does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StopAction {
    /// The session has exited: no Stop.
    Nothing,
    /// A live shell: stop it without asking.
    Kill,
    /// A running agent (or a session not listed yet): ask first.
    Ask,
}

/// Stop follows closing a pane (`lifecycle::close_action`), minus closing the pane: it closes
/// itself when the session exits (ADR 0033).
pub fn stop_action(session: Option<&Session>) -> StopAction {
    match lifecycle::close_action(session) {
        CloseAction::Remove => StopAction::Nothing,
        CloseAction::Kill => StopAction::Kill,
        CloseAction::Ask => StopAction::Ask,
    }
}

/// Buttons of the Stop confirmation; index 0 stops ([`crate::task_dialog::confirms_delete`]).
pub const STOP_ANSWERS: [&str; 2] = ["Stop session", "Cancel"];

/// The text of the Session card.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionCard {
    /// The agent profile id (the badge is drawn from the profile's icon).
    pub agent: String,
    pub title: String,
    /// `working · 2m`, `needs input`, `exited (1)`.
    pub state: String,
    /// `(label, value)` rows of the usage the agent reported.
    pub usage: Vec<(&'static str, String)>,
    /// The agent's transcript file, once the daemon located it.
    pub transcript: Option<String>,
}

pub fn session_card(
    session: &Session,
    usage: Option<&SessionUsageEvent>,
    now_ms: i64,
) -> SessionCard {
    SessionCard {
        agent: session.agent_profile.clone(),
        title: history::title(session).unwrap_or("Untitled").to_owned(),
        state: status::text(
            session.state,
            session.exit_code,
            session.state_changed_at,
            now_ms,
        ),
        usage: usage.map(usage::fields).map(usage_rows).unwrap_or_default(),
        transcript: usage.and_then(|u| u.transcript.clone()),
    }
}

/// The usage rows that have data, in card order.
pub fn usage_rows(fields: UsageFields) -> Vec<(&'static str, String)> {
    [
        ("Context", fields.context.map(|share| share.label)),
        ("Tokens", fields.tokens),
        ("Cache", fields.cache),
        ("Model", fields.model),
    ]
    .into_iter()
    .filter_map(|(label, value)| Some((label, value?)))
    .collect()
}

/// One repository with changes on the Changes card.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChangeLine {
    /// The repository as `git.repos` names it (`""` = the workspace folder): what Review diff
    /// opens.
    pub repo: String,
    /// The workspace name for the folder itself, else the child folder.
    pub label: String,
    /// `M 23`, when any path is modified.
    pub modified: Option<String>,
    /// `?3`, when any file is untracked.
    pub untracked: Option<String>,
}

fn change_line(repo: &RepoSummary, workspace: &str) -> ChangeLine {
    ChangeLine {
        repo: repo.repo.clone(),
        label: if repo.repo.is_empty() {
            workspace.to_owned()
        } else {
            repo.repo.clone()
        },
        modified: (repo.modified > 0).then(|| format!("M {}", repo.modified)),
        untracked: (repo.untracked > 0).then(|| format!("?{}", repo.untracked)),
    }
}

/// The Changes card: a line per repository with changes, in `git.repos` order. `Some(empty)` =
/// every repository is clean; `None` = hide the card (no repository, or no summary loaded).
pub fn changes_card(summary: Option<&GitSummary>, workspace: &str) -> Option<Vec<ChangeLine>> {
    let summary = summary.filter(|s| !s.repos.is_empty())?;
    Some(
        summary
            .repos
            .iter()
            .filter(|repo| repo.changed() > 0)
            .map(|repo| change_line(repo, workspace))
            .collect(),
    )
}

/// The session whose thread the panel should read (ADR 0057): an open panel and a focused
/// agent session with a native id. `None` = no reading. An exited session stays wanted while
/// it is shown; the poll task does one last read after the exit and stops.
pub fn wanted_thread(open: bool, session: Option<&Session>) -> Option<(Ulid, String)> {
    let session = session.filter(|s| open && s.kind == SessionKind::Agent)?;
    Some((session.id, session.native_session_id.clone()?))
}

#[cfg(test)]
mod tests {
    use mkrew_protocol::{SessionState, StateSource};

    use super::*;
    use crate::usage::ContextShare;

    fn session(kind: SessionKind, state: SessionState) -> Session {
        Session {
            id: Ulid::new(),
            task_id: None,
            project_id: Ulid::new(),
            agent_profile: "claude".into(),
            kind,
            title: None,
            terminal_title: None,
            color: None,
            pinned: false,
            position: 0.0,
            native_session_id: None,
            state,
            state_source: StateSource::Daemon,
            state_changed_at: 0,
            exit_code: None,
            cols: 80,
            rows: 24,
            started_at: 0,
            ended_at: None,
            plan_path: None,
        }
    }

    #[test]
    fn the_saved_width_is_kept_between_the_limits() {
        assert_eq!(saved_width(300), 300.0);
        assert_eq!(saved_width(0), theme::INSPECTOR_MIN_WIDTH);
        assert_eq!(saved_width(65_535), theme::INSPECTOR_MAX_WIDTH);
    }

    #[test]
    fn dragging_the_edge_sets_the_width_from_the_pointer() {
        // The panel is flush with the window's right edge: width = window − pointer.
        assert_eq!(width_from_pointer(1200.0, 900.0), 300.0);
        assert_eq!(
            width_from_pointer(1200.0, 1190.0),
            theme::INSPECTOR_MIN_WIDTH
        );
        assert_eq!(
            width_from_pointer(1200.0, 100.0),
            theme::INSPECTOR_MAX_WIDTH
        );
        // Never more than half of a narrow window (but never below the minimum).
        assert_eq!(width_from_pointer(700.0, 0.0), 350.0);
        assert_eq!(width_from_pointer(400.0, 0.0), theme::INSPECTOR_MIN_WIDTH);
    }

    #[test]
    fn only_a_running_agent_asks_before_stopping() {
        let agent = session(SessionKind::Agent, SessionState::NeedsInput);
        let shell = session(SessionKind::Shell, SessionState::Idle);
        let exited = session(SessionKind::Agent, SessionState::Exited);
        assert_eq!(stop_action(Some(&agent)), StopAction::Ask);
        assert_eq!(stop_action(Some(&shell)), StopAction::Kill);
        assert_eq!(stop_action(Some(&exited)), StopAction::Nothing);
        // Just spawned, not listed yet: ask, like closing its pane.
        assert_eq!(stop_action(None), StopAction::Ask);
    }

    #[test]
    fn the_card_names_the_session_and_its_state() {
        let mut working = session(SessionKind::Agent, SessionState::Working);
        working.state_changed_at = 1_000;
        let card = session_card(&working, None, 121_000);
        assert_eq!(card.agent, "claude");
        assert_eq!(card.title, "Untitled", "no title, no terminal title");
        assert_eq!(card.state, "working · 2m");
        assert!(card.usage.is_empty() && card.transcript.is_none());
        working.terminal_title = Some("Fix login".into());
        assert_eq!(session_card(&working, None, 0).title, "Fix login");
        working.title = Some("Mine".into());
        assert_eq!(session_card(&working, None, 0).title, "Mine");
    }

    #[test]
    fn usage_rows_skip_what_has_no_data() {
        let fields = UsageFields {
            context: Some(ContextShare {
                label: "ctx 62%".into(),
                warning: false,
                hover: String::new(),
            }),
            tokens: Some("12.3k tok".into()),
            cache: None,
            model: Some("opus-4".into()),
        };
        assert_eq!(
            usage_rows(fields),
            [
                ("Context", "ctx 62%".to_owned()),
                ("Tokens", "12.3k tok".to_owned()),
                ("Model", "opus-4".to_owned())
            ]
        );
        assert!(usage_rows(UsageFields::default()).is_empty());
    }

    #[test]
    fn the_card_carries_the_transcript_of_its_usage() {
        let working = session(SessionKind::Agent, SessionState::Working);
        let event = SessionUsageEvent {
            session_id: working.id,
            input: 1,
            output: 1,
            cache_read: 0,
            cache_write: 0,
            context: 1,
            context_window: None,
            model: None,
            transcript: Some("/t/s1.jsonl".into()),
        };
        let card = session_card(&working, Some(&event), 0);
        assert_eq!(card.transcript.as_deref(), Some("/t/s1.jsonl"));
        assert!(!card.usage.is_empty(), "tokens at least");
    }

    fn repo(name: &str, modified: usize, untracked: usize) -> RepoSummary {
        RepoSummary {
            repo: name.into(),
            branch: Some("main".into()),
            head: Some("1a2b3c4d".into()),
            modified,
            untracked,
        }
    }

    fn lines(repos: Vec<RepoSummary>) -> Vec<(String, String, Option<String>, Option<String>)> {
        changes_card(Some(&GitSummary { repos }), "api")
            .unwrap()
            .into_iter()
            .map(|l| (l.repo, l.label, l.modified, l.untracked))
            .collect()
    }

    #[test]
    fn one_repo_is_one_line_named_after_the_workspace() {
        assert_eq!(
            lines(vec![repo("", 23, 3)]),
            [(
                "".into(),
                "api".into(),
                Some("M 23".into()),
                Some("?3".into())
            )]
        );
    }

    #[test]
    fn a_zero_part_is_left_out() {
        assert_eq!(
            lines(vec![repo("", 2, 0)]),
            [("".into(), "api".into(), Some("M 2".into()), None)]
        );
        assert_eq!(
            lines(vec![repo("", 0, 5)]),
            [("".into(), "api".into(), None, Some("?5".into()))]
        );
    }

    #[test]
    fn only_repos_with_changes_get_a_line_in_repo_order() {
        // A root without `.git` lists just its children; clean ones have no line.
        let got = lines(vec![repo("fe", 1, 0), repo("be", 0, 0), repo("ops", 0, 2)]);
        let labels: Vec<&str> = got.iter().map(|l| l.1.as_str()).collect();
        assert_eq!(labels, ["fe", "ops"]);
        assert_eq!(
            got[1].0, "ops",
            "Review diff opens the child repo by its name"
        );
    }

    #[test]
    fn the_card_says_clean_or_hides() {
        // Repos, none changed: a card with no line ("Working tree clean").
        assert_eq!(
            changes_card(
                Some(&GitSummary {
                    repos: vec![repo("", 0, 0)]
                }),
                "api"
            ),
            Some(Vec::new())
        );
        // No repository at all, or nothing loaded yet: no card.
        assert_eq!(changes_card(Some(&GitSummary::default()), "api"), None);
        assert_eq!(changes_card(None, "api"), None);
    }

    #[test]
    fn the_thread_is_read_only_for_a_shown_agent_with_a_native_id() {
        let mut agent = session(SessionKind::Agent, SessionState::Working);
        assert_eq!(wanted_thread(true, Some(&agent)), None, "no native id yet");
        agent.native_session_id = Some("n1".into());
        assert_eq!(
            wanted_thread(true, Some(&agent)),
            Some((agent.id, "n1".into()))
        );
        assert_eq!(wanted_thread(false, Some(&agent)), None, "panel closed");
        assert_eq!(wanted_thread(true, None), None, "nothing focused");
        let mut shell = session(SessionKind::Shell, SessionState::Working);
        shell.native_session_id = Some("n1".into());
        assert_eq!(
            wanted_thread(true, Some(&shell)),
            None,
            "shells have no transcript"
        );
    }
}
