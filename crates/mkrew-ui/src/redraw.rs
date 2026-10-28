//! Which daemon events make the root `Shell` render again. A model (usage, turns, plan, ports,
//! queue, tasks, conflicts) always keeps its data; the root only redraws when the window draws
//! that data now. Anything that changes what is on screen (a view, workspace, tab or panel
//! switch, a dialog, window visibility) re-renders on its own, so data skipped while hidden is
//! read fresh when it is shown. The sidebar view observes the Shell, so a skipped notify skips
//! it too. Pure and tested headlessly; `Shell::shown` builds [`Shown`].

use mkrew_core::settings::SidebarMode;
use mkrew_protocol::{CommentContext, Event, Session};
use ulid::Ulid;

use crate::board::View;
use crate::inspector::PanelTab;

/// What the window draws now.
#[derive(Debug, PartialEq, Eq)]
pub struct Shown {
    pub workspace: Option<Ulid>,
    pub view: View,
    /// Sessions whose pane is drawn: the active tab's visible panes in the Terminals view,
    /// none while the window is hidden.
    pub panes: Vec<Ulid>,
    /// The pane keyboard input goes to (the full sidebar draws its plan bar).
    pub focused: Option<Ulid>,
    /// The right panel's tab while the panel is drawn.
    pub panel: Option<PanelTab>,
    /// The session the panel's Session tab describes.
    pub panel_session: Option<Ulid>,
    pub sidebar: SidebarMode,
    /// Some dialog is open.
    pub modal: bool,
}

/// Whether an event whose model reported "my data changed" must re-render the root. Events
/// owned by another handler (sidebar state, tabs, docs, git) answer `true`: they are never
/// decided here, and their handlers keep their own redraw rules.
pub fn event_needs_redraw(event: &Event, shown: &Shown, sessions: &[Session]) -> bool {
    // Dialogs read models of their own (compare, history, task editor, Ask AI).
    if shown.modal {
        return true;
    }
    let in_workspace = |project_id: Ulid| shown.workspace == Some(project_id);
    // A session not listed yet cannot be placed: redraw.
    let in_shown_workspace = |session_id: Ulid| {
        sessions
            .iter()
            .find(|s| s.id == session_id)
            .is_none_or(|s| in_workspace(s.project_id))
    };
    let on_pane = |id: Ulid| shown.panes.contains(&id);
    let in_session_tab =
        |id: Ulid| shown.panel == Some(PanelTab::Session) && shown.panel_session == Some(id);
    match event {
        // Pane status, the Session tab card, Board cards (their task's tokens).
        Event::SessionUsage(e) => {
            on_pane(e.session_id)
                || in_session_tab(e.session_id)
                || (shown.view == View::Board && in_shown_workspace(e.session_id))
        }
        // Pane status pill, `⧗ N queued` and the composer's queue.
        Event::SessionTurn(e) => on_pane(e.session_id),
        Event::SessionQueue(e) => on_pane(e.session_id),
        // Session tab Plan card, or the focused row's bar in the full sidebar.
        Event::SessionPlan(e) => {
            in_session_tab(e.session_id)
                || (shown.sidebar == SidebarMode::Full && shown.focused == Some(e.session_id))
        }
        // Pane titles and the Run pill (the shown workspace, every view), the RUN group of
        // every open workspace in the full sidebar.
        Event::SessionPorts(e) => {
            in_shown_workspace(e.session_id) || shown.sidebar == SidebarMode::Full
        }
        Event::TaskUpdated(task) => tasks_drawn(shown, task.project_id),
        Event::TaskDeleted(deleted) => tasks_drawn(shown, deleted.project_id),
        Event::ConflictDetected(_) => conflicts_need_redraw(shown),
        // Diff comments are drawn in the diff view, doc comments in Docs.
        Event::CommentUpdated(comment) => {
            in_workspace(comment.project_id)
                && matches!(
                    (shown.view, comment.context),
                    (View::Diff, CommentContext::Diff) | (View::Docs, CommentContext::Doc)
                )
        }
        // The Git tab's commit message box and the pane composer.
        Event::AiGenerated(_) => {
            shown.view == View::Terminals || shown.panel == Some(PanelTab::Git)
        }
        Event::SessionState(_)
        | Event::SessionExited(_)
        | Event::SessionUpdated(_)
        | Event::SessionTerminalTitle(_)
        | Event::ProjectUpdated(_)
        | Event::ProjectCloned(_)
        | Event::TabUpdated(_)
        | Event::SessionSnapshot(_)
        | Event::NotificationClicked(_)
        | Event::SessionEdited(_)
        | Event::DocChanged(_)
        | Event::GitChanged(_) => true,
    }
}

/// Conflict warnings are drawn on Board cards and in pane status. Also asked after
/// `Conflicts::prune`, which can clear a warning on any event.
pub fn conflicts_need_redraw(shown: &Shown) -> bool {
    shown.modal || matches!(shown.view, View::Board | View::Terminals)
}

/// Tasks are read by the Board, the pane status folder and the worktree chooser of the
/// Files and Git tabs of the shown workspace.
fn tasks_drawn(shown: &Shown, project_id: Ulid) -> bool {
    shown.workspace == Some(project_id)
        && (matches!(shown.view, View::Board | View::Terminals)
            || matches!(shown.panel, Some(PanelTab::Files | PanelTab::Git)))
}

#[cfg(test)]
mod tests {
    use mkrew_protocol::{
        AiGeneratedEvent, CommentId, CommentStatus, ConflictDetectedEvent, DocComment,
        SessionExitedEvent, SessionKind, SessionPlanEvent, SessionPortsEvent, SessionQueueEvent,
        SessionState, SessionStateEvent, SessionTurnEvent, SessionUsageEvent, StateSource,
        TaskDeletedEvent,
    };

    use super::*;

    /// `a` is the focused pane of the shown workspace, `b` lives in a hidden tab of it, `c` in
    /// another workspace.
    struct World {
        ws: Ulid,
        other: Ulid,
        sessions: Vec<Session>,
    }

    impl World {
        fn a(&self) -> Ulid {
            self.sessions[0].id
        }
        fn b(&self) -> Ulid {
            self.sessions[1].id
        }
        fn c(&self) -> Ulid {
            self.sessions[2].id
        }
    }

    fn session(project_id: Ulid) -> Session {
        Session {
            id: Ulid::new(),
            task_id: None,
            project_id,
            agent_profile: "claude".to_owned(),
            kind: SessionKind::Agent,
            title: None,
            terminal_title: None,
            color: None,
            native_session_id: None,
            state: SessionState::Working,
            state_source: StateSource::Daemon,
            state_changed_at: 0,
            exit_code: None,
            cols: 80,
            rows: 24,
            started_at: 0,
            ended_at: None,
            pinned: false,
            position: 0.0,
            plan_path: None,
        }
    }

    fn world() -> World {
        let (ws, other) = (Ulid::new(), Ulid::new());
        World {
            ws,
            other,
            sessions: vec![session(ws), session(ws), session(other)],
        }
    }

    /// Terminals view of `ws`, `a` focused, no right panel, compact rail.
    fn terminals(w: &World) -> Shown {
        Shown {
            workspace: Some(w.ws),
            view: View::Terminals,
            panes: vec![w.a()],
            focused: Some(w.a()),
            panel: None,
            panel_session: Some(w.a()),
            sidebar: SidebarMode::Compact,
            modal: false,
        }
    }

    /// Editor view of `ws`: no pane drawn, no panel, no sidebar rows.
    fn nothing_shown(w: &World) -> Shown {
        Shown {
            workspace: Some(w.ws),
            view: View::Editor,
            panes: Vec::new(),
            focused: None,
            panel: None,
            panel_session: Some(w.a()),
            sidebar: SidebarMode::Hidden,
            modal: false,
        }
    }

    fn usage(id: Ulid) -> Event {
        Event::SessionUsage(SessionUsageEvent {
            session_id: id,
            input: 1,
            output: 1,
            cache_read: 0,
            cache_write: 0,
            context: 0,
            context_window: None,
            model: None,
            transcript: None,
        })
    }

    fn turn(id: Ulid) -> Event {
        Event::SessionTurn(SessionTurnEvent {
            session_id: id,
            n: 1,
            started_at: 0,
            ended_at: 1,
            files: Vec::new(),
        })
    }

    fn queue(id: Ulid) -> Event {
        Event::SessionQueue(SessionQueueEvent {
            session_id: id,
            items: Vec::new(),
        })
    }

    fn plan(id: Ulid) -> Event {
        Event::SessionPlan(SessionPlanEvent {
            session_id: id,
            name: "plan".into(),
            done: 1,
            total: 2,
            items: Vec::new(),
            error: None,
        })
    }

    fn ports(id: Ulid) -> Event {
        Event::SessionPorts(SessionPortsEvent {
            session_id: id,
            ports: vec![8080],
        })
    }

    fn comment(project_id: Ulid, context: CommentContext) -> Event {
        Event::CommentUpdated(Box::new(DocComment {
            id: CommentId(Ulid::new()),
            project_id,
            file_path: "a.rs".into(),
            context,
            parent_id: None,
            author: DocComment::USER.into(),
            body: "x".into(),
            start_line: Some(1),
            end_line: Some(1),
            quote: None,
            status: CommentStatus::Open,
            sent_to: None,
            sent_snapshot: None,
            created_at: 0,
            updated_at: 0,
        }))
    }

    fn redraws(w: &World, shown: &Shown, event: &Event) -> bool {
        event_needs_redraw(event, shown, &w.sessions)
    }

    /// A visible pane shows its context share; a session of a hidden tab or another workspace
    /// shows nothing. The Session tab and the Board are the other two readers.
    #[test]
    fn usage_redraws_for_a_visible_pane_the_session_tab_or_the_board() {
        let w = world();
        let mut s = terminals(&w);
        assert!(redraws(&w, &s, &usage(w.a())), "visible pane");
        assert!(!redraws(&w, &s, &usage(w.b())), "hidden tab");
        assert!(!redraws(&w, &s, &usage(w.c())), "other workspace");
        s.panes.push(w.b());
        assert!(redraws(&w, &s, &usage(w.b())), "split pane");

        let mut s = nothing_shown(&w);
        assert!(!redraws(&w, &s, &usage(w.a())), "Editor draws no usage");
        s.panel = Some(PanelTab::Session);
        assert!(
            redraws(&w, &s, &usage(w.a())),
            "Session tab of the panel session"
        );
        assert!(
            !redraws(&w, &s, &usage(w.b())),
            "Session tab shows one session"
        );
        s.panel = Some(PanelTab::Git);
        assert!(!redraws(&w, &s, &usage(w.a())), "Git tab draws no usage");

        s.panel = None;
        s.view = View::Board;
        assert!(
            redraws(&w, &s, &usage(w.b())),
            "card of the shown workspace"
        );
        assert!(
            !redraws(&w, &s, &usage(w.c())),
            "another workspace's board is not drawn"
        );
    }

    #[test]
    fn turns_and_queues_need_a_visible_pane() {
        let w = world();
        let s = terminals(&w);
        for event in [turn(w.a()), queue(w.a())] {
            assert!(redraws(&w, &s, &event), "{event:?}");
        }
        for event in [turn(w.b()), queue(w.b()), turn(w.c()), queue(w.c())] {
            assert!(!redraws(&w, &s, &event), "{event:?}");
        }
    }

    /// The plan is drawn for one session: in the Session tab, or as the focused row's bar of
    /// the full sidebar (the rail has no bar).
    #[test]
    fn plan_needs_the_session_tab_or_the_focused_row_of_the_full_sidebar() {
        let w = world();
        let mut s = terminals(&w);
        assert!(!redraws(&w, &s, &plan(w.a())), "rail, panel closed");
        s.panel = Some(PanelTab::Session);
        assert!(redraws(&w, &s, &plan(w.a())));
        assert!(!redraws(&w, &s, &plan(w.b())), "not the panel session");
        s.panel = Some(PanelTab::Files);
        assert!(!redraws(&w, &s, &plan(w.a())), "another tab");
        s.sidebar = SidebarMode::Full;
        assert!(redraws(&w, &s, &plan(w.a())), "the selected row's bar");
        assert!(
            !redraws(&w, &s, &plan(w.b())),
            "an unselected row has no bar"
        );
    }

    /// Ports are in the Run pill of the shown workspace (every view) and in the RUN group of
    /// the full sidebar (every open workspace); the rail draws neither.
    #[test]
    fn ports_show_in_the_run_pill_and_the_full_sidebars_run_group() {
        let w = world();
        let mut s = nothing_shown(&w);
        assert!(
            redraws(&w, &s, &ports(w.b())),
            "Run pill of the shown workspace"
        );
        assert!(
            !redraws(&w, &s, &ports(w.c())),
            "other workspace, no sidebar rows"
        );
        s.sidebar = SidebarMode::Compact;
        assert!(!redraws(&w, &s, &ports(w.c())), "the rail has no RUN group");
        s.sidebar = SidebarMode::Full;
        assert!(redraws(&w, &s, &ports(w.c())), "its workspace's RUN group");
        assert!(
            redraws(&w, &nothing_shown(&w), &ports(Ulid::new())),
            "unknown session"
        );
    }

    /// The footer's Inbox count, the sidebar dots and `Viewed` depend on these: never skipped.
    #[test]
    fn state_changes_redraw_even_when_nothing_else_is_shown() {
        let w = world();
        let s = nothing_shown(&w);
        let state = Event::SessionState(SessionStateEvent {
            session_id: w.c(),
            state: SessionState::NeedsInput,
            source: StateSource::Hook,
            changed_at: 1,
        });
        let exited = Event::SessionExited(SessionExitedEvent {
            session_id: w.c(),
            exit_code: Some(0),
        });
        assert!(redraws(&w, &s, &state));
        assert!(redraws(&w, &s, &exited));
    }

    /// Dialogs read models we did not audit (compare, history, task editor): while one is open
    /// every change redraws.
    #[test]
    fn an_open_dialog_keeps_every_change_redrawing() {
        let w = world();
        let mut s = nothing_shown(&w);
        assert!(!redraws(&w, &s, &usage(w.c())));
        s.modal = true;
        assert!(redraws(&w, &s, &usage(w.c())));
        assert!(conflicts_need_redraw(&s));
    }

    /// Tasks are read by the Board, the pane folder and the worktree chooser of the Files and
    /// Git tabs: not by the Session tab, the Editor or the diff themselves.
    #[test]
    fn task_changes_redraw_the_views_that_read_tasks() {
        let w = world();
        let event = |project_id| {
            Event::TaskDeleted(TaskDeletedEvent {
                task_id: Ulid::new(),
                project_id,
            })
        };
        let mut s = terminals(&w);
        for view in [View::Board, View::Terminals] {
            s.view = view;
            assert!(redraws(&w, &s, &event(w.ws)), "{view:?}");
            assert!(
                !redraws(&w, &s, &event(w.other)),
                "{view:?}: another workspace"
            );
        }
        s.view = View::Editor;
        assert!(!redraws(&w, &s, &event(w.ws)), "Editor, no panel");
        s.panel = Some(PanelTab::Session);
        assert!(!redraws(&w, &s, &event(w.ws)), "Session tab");
        for tab in [PanelTab::Files, PanelTab::Git] {
            s.panel = Some(tab);
            assert!(redraws(&w, &s, &event(w.ws)), "{tab:?} tab chooser");
        }
        s.view = View::Docs;
        s.panel = None;
        assert!(!redraws(&w, &s, &event(w.ws)));
    }

    #[test]
    fn conflicts_show_on_the_board_and_in_pane_headers_only() {
        let w = world();
        let detected = Event::ConflictDetected(ConflictDetectedEvent {
            project_id: w.ws,
            path: "src/a.rs".into(),
            session_ids: vec![w.a(), w.b()],
            task_ids: Vec::new(),
        });
        let mut s = terminals(&w);
        for view in [View::Board, View::Terminals] {
            s.view = view;
            assert!(
                conflicts_need_redraw(&s) && redraws(&w, &s, &detected),
                "{view:?}"
            );
        }
        for view in [View::Docs, View::Editor, View::Diff] {
            s.view = view;
            assert!(
                !conflicts_need_redraw(&s) && !redraws(&w, &s, &detected),
                "{view:?}"
            );
        }
    }

    /// Diff comments are drawn in the diff view, doc comments in Docs, both of the shown
    /// workspace; an AI answer goes to the Git tab's commit box or a pane composer.
    #[test]
    fn comments_and_ai_answers_follow_the_view_that_draws_them() {
        let w = world();
        let mut s = nothing_shown(&w);
        s.view = View::Diff;
        assert!(redraws(&w, &s, &comment(w.ws, CommentContext::Diff)));
        assert!(
            !redraws(&w, &s, &comment(w.other, CommentContext::Diff)),
            "other workspace"
        );
        assert!(
            !redraws(&w, &s, &comment(w.ws, CommentContext::Doc)),
            "Doc comment in Diff"
        );
        s.view = View::Docs;
        assert!(redraws(&w, &s, &comment(w.ws, CommentContext::Doc)));
        assert!(!redraws(&w, &s, &comment(w.ws, CommentContext::Diff)));
        s.view = View::Terminals;
        assert!(!redraws(&w, &s, &comment(w.ws, CommentContext::Diff)));

        let ai = Event::AiGenerated(AiGeneratedEvent {
            generation_id: Ulid::new(),
            project_id: Some(w.ws),
            text: Some("x".into()),
            error: None,
        });
        assert!(redraws(&w, &s, &ai), "pane composer");
        s.view = View::Editor;
        assert!(!redraws(&w, &s, &ai));
        s.panel = Some(PanelTab::Git);
        assert!(redraws(&w, &s, &ai), "commit box in the Git tab");
    }
}
