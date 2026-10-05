//! Per-session status display (SPEC §11): spinner, elapsed/idle time, tab urgency, hover text
//! and the pane border. Pure; `main.rs` renders the result with [`crate::sidebar::status_display`]
//! icons and [`crate::sidebar::status_color`] colors.

use std::time::{SystemTime, UNIX_EPOCH};

use mkrew_protocol::{Session, SessionState, StateSource};
use ulid::Ulid;

use crate::sidebar::status_display;
use crate::theme::{self, Theme};

/// Frames of the small spinner shown for `working`, one per [`theme::STATUS_TICK`].
pub const SPINNER: [&str; 4] = ["◐", "◓", "◑", "◒"];

/// Unix ms now (0 if the clock is before 1970).
pub fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
        .unwrap_or(0)
}

/// The status icon at `now_ms`: a spinner frame while working, else the state's icon.
pub fn icon(state: SessionState, now_ms: i64) -> &'static str {
    if state == SessionState::Working {
        let step = theme::STATUS_TICK.as_millis().max(1) as i64;
        SPINNER[(now_ms.max(0) / step) as usize % SPINNER.len()]
    } else {
        status_display(state).icon
    }
}

/// A short duration: `12s`, `4m`, `1h 5m`, `3h`.
pub fn format_duration(ms: i64) -> String {
    let secs = ms.max(0) / 1000;
    match secs {
        0..60 => format!("{secs}s"),
        60..3600 => format!("{}m", secs / 60),
        _ => match (secs / 3600, secs % 3600 / 60) {
            (hours, 0) => format!("{hours}h"),
            (hours, minutes) => format!("{hours}h {minutes}m"),
        },
    }
}

/// Elapsed time while working, idle time while idle; nothing for other states.
pub fn time(state: SessionState, changed_at: i64, now_ms: i64) -> Option<String> {
    matches!(state, SessionState::Working | SessionState::Idle)
        .then(|| format_duration(now_ms - changed_at))
}

/// The status text: `working · 2m`, `needs input`, `exited (1)`.
pub fn text(state: SessionState, exit_code: Option<i32>, changed_at: i64, now_ms: i64) -> String {
    let label = status_display(state).label;
    match (state, exit_code, time(state, changed_at, now_ms)) {
        (SessionState::Exited, Some(code), _) => format!("{label} ({code})"),
        (_, _, Some(time)) => format!("{label} · {time}"),
        _ => label.to_owned(),
    }
}

/// What decided a state, as shown on hover.
pub fn source_label(source: StateSource) -> &'static str {
    match source {
        StateSource::Hook => "hook",
        StateSource::Screen => "screen",
        StateSource::Daemon => "daemon",
    }
}

/// Hover text of a status: what decided it and when, e.g. `needs input · from hook · 12s ago`.
pub fn hover(state: SessionState, source: StateSource, changed_at: i64, now_ms: i64) -> String {
    format!(
        "{} · from {} · {} ago",
        status_display(state).label,
        source_label(source),
        format_duration(now_ms - changed_at)
    )
}

/// Rank for the tab label: needs_input > done > working > idle > exited > the rest.
fn urgency(state: SessionState) -> u8 {
    match state {
        SessionState::NeedsInput => 5,
        SessionState::Done => 4,
        SessionState::Working => 3,
        SessionState::Idle => 2,
        SessionState::Exited => 1,
        SessionState::Starting | SessionState::Unknown => 0,
    }
}

/// The session in the most urgent state among a tab's panes `ids` (the first one on a tie;
/// unknown ids are skipped; `None` without known panes).
pub fn most_urgent<'a>(sessions: &'a [Session], ids: &[Ulid]) -> Option<&'a Session> {
    ids.iter()
        .filter_map(|id| sessions.iter().find(|s| s.id == *id))
        .rev()
        .max_by_key(|s| urgency(s.state))
}

/// Whether the status display changed between `last_ms` and `now_ms` (checked every
/// [`theme::STATUS_TICK`]): the spinner turns while a session works; an idle time changes
/// once per second, then once per minute.
pub fn redraw_due(sessions: &[Session], last_ms: i64, now_ms: i64) -> bool {
    sessions.iter().any(|s| match s.state {
        SessionState::Working => true,
        SessionState::Idle => {
            format_duration(last_ms - s.state_changed_at)
                != format_duration(now_ms - s.state_changed_at)
        }
        _ => false,
    })
}

/// Pane border color: amber while the session needs input, the faint accent when focused
/// among several panes, else the plain border.
pub fn pane_border(
    theme: &Theme,
    state: Option<SessionState>,
    several: bool,
    focused: bool,
) -> u32 {
    if state == Some(SessionState::NeedsInput) {
        theme.status_needs_input
    } else if several && focused {
        theme.pane_focus_border
    } else {
        theme.border
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SEC: i64 = 1000;

    #[test]
    fn durations_are_short() {
        assert_eq!(format_duration(0), "0s");
        assert_eq!(format_duration(-5 * SEC), "0s");
        assert_eq!(format_duration(59 * SEC + 999), "59s");
        assert_eq!(format_duration(60 * SEC), "1m");
        assert_eq!(format_duration(3599 * SEC), "59m");
        assert_eq!(format_duration(3600 * SEC), "1h");
        assert_eq!(format_duration(3900 * SEC), "1h 5m");
    }

    #[test]
    fn working_and_idle_show_their_time() {
        let now = 1_000 * SEC;
        assert_eq!(
            text(SessionState::Working, None, now - 90 * SEC, now),
            "working · 1m"
        );
        assert_eq!(
            text(SessionState::Idle, None, now - 12 * SEC, now),
            "idle · 12s"
        );
        assert_eq!(
            text(SessionState::NeedsInput, None, now - 12 * SEC, now),
            "needs input"
        );
        assert_eq!(text(SessionState::Exited, Some(1), 0, now), "exited (1)");
        assert_eq!(text(SessionState::Exited, None, 0, now), "exited");
        assert_eq!(time(SessionState::Done, 0, now), None);
    }

    #[test]
    fn spinner_turns_only_while_working() {
        let step = theme::STATUS_TICK.as_millis() as i64;
        let frames: Vec<&str> = (0..5)
            .map(|i| icon(SessionState::Working, i * step))
            .collect();
        assert_eq!(frames, ["◐", "◓", "◑", "◒", "◐"]);
        assert_eq!(icon(SessionState::Idle, step), "○");
        assert_eq!(icon(SessionState::NeedsInput, 3 * step), "?");
    }

    fn session(state: SessionState, changed_at: i64) -> Session {
        Session {
            id: Ulid::new(),
            task_id: None,
            project_id: Ulid::new(),
            agent_profile: "fake".into(),
            kind: mkrew_protocol::SessionKind::Agent,
            title: None,
            native_session_id: None,
            state,
            state_source: StateSource::Daemon,
            state_changed_at: changed_at,
            exit_code: None,
            cols: 80,
            rows: 24,
            started_at: 0,
            ended_at: None,
        }
    }

    #[test]
    fn tab_shows_the_most_urgent_state() {
        use SessionState::*;
        let urgent = |states: &[SessionState]| {
            let sessions: Vec<Session> = states.iter().map(|s| session(*s, 0)).collect();
            let mut ids: Vec<Ulid> = sessions.iter().map(|s| s.id).collect();
            ids.push(Ulid::new()); // not (yet) known: skipped
            most_urgent(&sessions, &ids).map(|s| s.state)
        };
        assert_eq!(urgent(&[Idle, NeedsInput, Done]), Some(NeedsInput));
        assert_eq!(urgent(&[Working, Done, Idle]), Some(Done));
        assert_eq!(urgent(&[Exited, Idle, Working]), Some(Working));
        assert_eq!(urgent(&[Exited, Idle]), Some(Idle));
        assert_eq!(urgent(&[Starting, Exited]), Some(Exited));
        assert_eq!(urgent(&[]), None);

        // A tie picks the first pane.
        let (a, b) = (session(Done, 0), session(Done, 0));
        assert_eq!(
            most_urgent(&[a.clone(), b.clone()], &[b.id, a.id]),
            Some(&b)
        );
    }

    #[test]
    fn hover_names_the_source_and_age() {
        assert_eq!(
            hover(SessionState::NeedsInput, StateSource::Hook, 0, 12 * SEC),
            "needs input · from hook · 12s ago"
        );
        assert_eq!(
            hover(SessionState::Idle, StateSource::Screen, 0, 120 * SEC),
            "idle · from screen · 2m ago"
        );
    }

    #[test]
    fn needs_input_highlights_the_pane_border() {
        for theme in [Theme::dark(), Theme::light()] {
            let needs = Some(SessionState::NeedsInput);
            assert_eq!(
                pane_border(&theme, needs, false, false),
                theme.status_needs_input
            );
            assert_eq!(
                pane_border(&theme, needs, true, true),
                theme.status_needs_input
            );
            let working = Some(SessionState::Working);
            assert_eq!(
                pane_border(&theme, working, true, true),
                theme.pane_focus_border
            );
            assert_eq!(pane_border(&theme, working, false, true), theme.border);
            assert_eq!(pane_border(&theme, None, true, false), theme.border);
        }
    }

    #[test]
    fn redraws_only_when_the_shown_status_changes() {
        let tick = theme::STATUS_TICK.as_millis() as i64;
        let still = [
            session(SessionState::Done, 0),
            session(SessionState::NeedsInput, 0),
            session(SessionState::Exited, 0),
        ];
        assert!(!redraw_due(&still, 0, 600 * SEC));

        // The spinner turns on every tick.
        let working = [session(SessionState::Working, 0)];
        assert!(redraw_due(&working, 10 * SEC, 10 * SEC + tick));

        // Idle: each second under a minute, then only when the minute changes.
        let idle = [session(SessionState::Idle, 0)];
        assert!(redraw_due(&idle, 4 * SEC + 600, 5 * SEC + 100));
        assert!(!redraw_due(&idle, 5 * SEC + 100, 5 * SEC + 600));
        assert!(!redraw_due(&idle, 90 * SEC, 90 * SEC + tick));
        assert!(redraw_due(&idle, 119 * SEC + 800, 120 * SEC + 300));
    }
}
