//! Per-session state machine and send-queue policy (SPEC §6, §8).
//!
//! Pure: no clock, no I/O. The daemon passes `now` and the last lines of the session's emulated
//! screen into every call, so tests drive it with a fake clock.

use std::collections::VecDeque;
use std::time::{Duration, Instant};

use mkrew_protocol::SessionState;
use regex::Regex;

use crate::profile::{StateConfig, StateMode};

/// After a hook event, screen detection is suppressed for this long (SPEC §6).
pub const HOOK_PRIORITY: Duration = Duration::from_secs(10);

/// How many trailing non-blank screen lines `needs_input_patterns` are matched against.
/// Small on purpose: an answered prompt must scroll out of this window for the session to
/// leave `needs_input`. The daemon passes the screen up to the cursor row.
pub const PATTERN_TAIL_LINES: usize = 4;

/// What an agent hook reported. Hooks arrive in M3; the machine already honours them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HookEvent {
    Busy,
    NeedsInput,
    Done,
}

/// State detection for one session.
#[derive(Debug)]
pub struct StateMachine {
    mode: StateMode,
    idle_after: Duration,
    patterns: Vec<Regex>,
    state: SessionState,
    last_output: Option<Instant>,
    last_hook: Option<Instant>,
}

impl StateMachine {
    /// Compiles the profile's patterns. They were validated at parse time, so this only fails
    /// for a hand-built [`StateConfig`].
    pub fn new(config: &StateConfig) -> Result<Self, regex::Error> {
        let patterns = config
            .needs_input_patterns
            .iter()
            .map(|p| Regex::new(p))
            .collect::<Result<_, _>>()?;
        Ok(Self {
            mode: config.mode,
            idle_after: Duration::from_millis(config.idle_after_ms),
            patterns,
            state: SessionState::Starting,
            last_output: None,
            last_hook: None,
        })
    }

    pub fn state(&self) -> SessionState {
        self.state
    }

    /// The PTY produced output; `screen` is the emulated screen after feeding it.
    /// Returns the new state if it changed.
    pub fn on_output(&mut self, now: Instant, screen: &[&str]) -> Option<SessionState> {
        self.last_output = Some(now);
        if !self.screen_active(now) {
            // Hook-only profiles still leave `starting` once the process shows signs of life.
            if self.state == SessionState::Starting && !self.uses_screen() {
                return self.set(SessionState::Unknown);
            }
            return None;
        }
        let next = if self.needs_input(screen) {
            SessionState::NeedsInput
        } else {
            SessionState::Working
        };
        self.set(next)
    }

    /// Periodic check with the current screen; detects quiet periods and prompts.
    /// Only `working` decays to `idle`: `needs_input` and `done` change through output or hooks.
    /// A hook `Busy` is trusted for [`HOOK_PRIORITY`] only; after that a quiet screen means idle.
    pub fn tick(&mut self, now: Instant, screen: &[&str]) -> Option<SessionState> {
        if !self.screen_active(now) {
            return None;
        }
        if self.needs_input(screen) {
            return self.set(SessionState::NeedsInput);
        }
        let quiet = self
            .last_output
            .is_some_and(|t| now.saturating_duration_since(t) >= self.idle_after);
        if quiet && self.state == SessionState::Working {
            return self.set(SessionState::Idle);
        }
        None
    }

    /// When the daemon should call [`tick`](Self::tick) next to notice the idle transition.
    /// Never earlier than the end of the hook-priority window, so ticking at it makes progress.
    pub fn next_deadline(&self) -> Option<Instant> {
        if self.state != SessionState::Working || !self.uses_screen() {
            return None;
        }
        let idle_at = self.last_output? + self.idle_after;
        let hook_end = self.last_hook.map(|t| t + HOOK_PRIORITY);
        Some(hook_end.map_or(idle_at, |h| h.max(idle_at)))
    }

    /// An agent hook fired. Ignored unless the profile's mode includes hooks.
    pub fn on_hook(&mut self, now: Instant, event: HookEvent) -> Option<SessionState> {
        if self.mode == StateMode::Screen || self.state == SessionState::Exited {
            return None;
        }
        self.last_hook = Some(now);
        self.set(match event {
            HookEvent::Busy => SessionState::Working,
            HookEvent::NeedsInput => SessionState::NeedsInput,
            HookEvent::Done => SessionState::Done,
        })
    }

    /// The process exited. Terminal: every later event is ignored.
    pub fn on_exit(&mut self) -> Option<SessionState> {
        self.set(SessionState::Exited)
    }

    fn uses_screen(&self) -> bool {
        self.mode != StateMode::Hook
    }

    fn screen_active(&self, now: Instant) -> bool {
        let hook_recent = self.mode != StateMode::Screen
            && self
                .last_hook
                .is_some_and(|t| now.saturating_duration_since(t) < HOOK_PRIORITY);
        self.uses_screen() && !hook_recent && self.state != SessionState::Exited
    }

    fn needs_input(&self, screen: &[&str]) -> bool {
        if self.patterns.is_empty() {
            return false;
        }
        let mut tail: Vec<&str> = screen
            .iter()
            .rev()
            .map(|l| l.trim_end())
            .filter(|l| !l.is_empty())
            .take(PATTERN_TAIL_LINES)
            .collect();
        tail.reverse();
        let text = tail.join("\n");
        self.patterns.iter().any(|p| p.is_match(&text))
    }

    fn set(&mut self, next: SessionState) -> Option<SessionState> {
        if self.state == next || self.state == SessionState::Exited {
            return None;
        }
        self.state = next;
        Some(next)
    }
}

/// Whether a `session.send` may be written to the PTY in this state (SPEC §8).
/// `unknown` delivers too: otherwise a session we cannot read would never get input.
pub fn can_deliver(state: SessionState) -> bool {
    matches!(
        state,
        SessionState::Idle | SessionState::Done | SessionState::NeedsInput | SessionState::Unknown
    )
}

/// Messages waiting for the session to become ready. Delivers one message at a time: after a
/// pop, the next one waits for PTY output (the agent reacting), which also lets the state
/// machine see the session busy again before the next delivery check.
#[derive(Debug, Default)]
pub struct SendQueue {
    pending: VecDeque<String>,
    in_flight: bool,
}

impl SendQueue {
    pub fn push(&mut self, text: String) {
        self.pending.push_back(text);
    }

    /// The PTY produced output: a delivered message has been picked up.
    pub fn on_output(&mut self) {
        self.in_flight = false;
    }

    /// The next message to write, if `state` allows delivery.
    pub fn pop(&mut self, state: SessionState) -> Option<String> {
        if self.in_flight || !can_deliver(state) {
            return None;
        }
        let text = self.pending.pop_front()?;
        self.in_flight = true;
        Some(text)
    }

    pub fn len(&self) -> usize {
        self.pending.len()
    }

    pub fn is_empty(&self) -> bool {
        self.pending.is_empty()
    }
}

#[cfg(test)]
mod tests;
