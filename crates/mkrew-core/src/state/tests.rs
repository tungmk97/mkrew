use super::*;

use SessionState::*;

/// Fake clock: a fixed origin plus millisecond offsets.
struct Clock(Instant);

impl Clock {
    fn at(&self, ms: u64) -> Instant {
        self.0 + Duration::from_millis(ms)
    }
}

fn clock() -> Clock {
    Clock(Instant::now())
}

fn config(mode: StateMode) -> StateConfig {
    StateConfig {
        mode,
        idle_after_ms: 3000,
        needs_input_patterns: vec![r"(?i)do you want to".into(), r"(?i)\(y/n\)".into()],
    }
}

fn machine(mode: StateMode) -> StateMachine {
    StateMachine::new(&config(mode)).unwrap()
}

const BUSY: &[&str] = &["Compiling mkrew v0.1.0", "  building..."];
const PROMPT: &[&str] = &["Wrote 3 files.", "Do you want to proceed? (y/n)", "", "   "];

#[test]
fn starts_in_starting_and_stays_without_output() {
    let c = clock();
    let mut m = machine(StateMode::Screen);
    assert_eq!(m.state(), Starting);
    assert_eq!(m.tick(c.at(60_000), &[]), None);
    assert_eq!(m.state(), Starting);
    assert_eq!(m.next_deadline(), None);
}

#[test]
fn output_then_quiet_goes_working_then_idle() {
    let c = clock();
    let mut m = machine(StateMode::Screen);
    assert_eq!(m.on_output(c.at(0), BUSY), Some(Working));
    assert_eq!(m.on_output(c.at(1000), BUSY), None);
    assert_eq!(m.next_deadline(), Some(c.at(4000)));
    assert_eq!(m.tick(c.at(3999), BUSY), None);
    assert_eq!(m.tick(c.at(4000), BUSY), Some(Idle));
    assert_eq!(m.next_deadline(), None);
    assert_eq!(m.on_output(c.at(5000), BUSY), Some(Working));
}

#[test]
fn prompt_on_screen_is_needs_input_immediately() {
    let c = clock();
    let mut m = machine(StateMode::Screen);
    m.on_output(c.at(0), BUSY);
    assert_eq!(m.on_output(c.at(100), PROMPT), Some(NeedsInput));
    // Stays needs_input while the prompt is visible, even after idle_after_ms.
    assert_eq!(m.tick(c.at(10_000), PROMPT), None);
    assert_eq!(m.state(), NeedsInput);
}

#[test]
fn answered_prompt_returns_to_working_then_idle() {
    let c = clock();
    let mut m = machine(StateMode::Screen);
    m.on_output(c.at(0), PROMPT);
    // TUI redraw: the prompt is gone from the screen.
    assert_eq!(m.on_output(c.at(500), BUSY), Some(Working));
    assert_eq!(m.tick(c.at(3500), BUSY), Some(Idle));
}

#[test]
fn prompt_appearing_without_new_output_is_detected_on_tick() {
    let c = clock();
    let mut m = machine(StateMode::Screen);
    m.on_output(c.at(0), BUSY);
    assert_eq!(m.tick(c.at(200), PROMPT), Some(NeedsInput));
}

#[test]
fn patterns_only_match_the_screen_tail() {
    let c = clock();
    let mut m = machine(StateMode::Screen);
    let mut screen = vec!["Do you want to proceed? (y/n)"];
    screen.extend(std::iter::repeat_n("log line", PATTERN_TAIL_LINES));
    assert_eq!(m.on_output(c.at(0), &screen), Some(Working));
    screen.pop();
    assert_eq!(m.on_output(c.at(10), &screen), Some(NeedsInput));
}

#[test]
fn multiline_patterns_see_joined_tail() {
    let cfg = StateConfig {
        needs_input_patterns: vec![r"Allow\?\n❯ 1\. Yes".into()],
        ..config(StateMode::Screen)
    };
    let c = clock();
    let mut m = StateMachine::new(&cfg).unwrap();
    assert_eq!(
        m.on_output(c.at(0), &["Allow?", "❯ 1. Yes", "  2. No"]),
        Some(NeedsInput)
    );
}

#[test]
fn no_patterns_never_needs_input() {
    let cfg = StateConfig {
        needs_input_patterns: vec![],
        ..config(StateMode::Screen)
    };
    let c = clock();
    let mut m = StateMachine::new(&cfg).unwrap();
    assert_eq!(m.on_output(c.at(0), PROMPT), Some(Working));
    assert_eq!(m.tick(c.at(3000), PROMPT), Some(Idle));
}

#[test]
fn invalid_pattern_is_an_error() {
    let cfg = StateConfig {
        needs_input_patterns: vec!["(unclosed".into()],
        ..config(StateMode::Screen)
    };
    assert!(StateMachine::new(&cfg).is_err());
}

#[test]
fn exit_is_terminal() {
    let c = clock();
    let mut m = machine(StateMode::HookScreen);
    m.on_output(c.at(0), BUSY);
    assert_eq!(m.on_exit(), Some(Exited));
    assert_eq!(m.on_exit(), None);
    assert_eq!(m.on_output(c.at(10), PROMPT), None);
    assert_eq!(m.tick(c.at(10_000), BUSY), None);
    assert_eq!(m.on_hook(c.at(10_000), HookEvent::Busy), None);
    assert_eq!(m.state(), Exited);
}

#[test]
fn exit_from_starting() {
    let mut m = machine(StateMode::Screen);
    assert_eq!(m.on_exit(), Some(Exited));
}

#[test]
fn hook_events_take_priority_for_ten_seconds() {
    let c = clock();
    let mut m = machine(StateMode::HookScreen);
    m.on_output(c.at(0), BUSY);
    assert_eq!(m.on_hook(c.at(1000), HookEvent::Done), Some(Done));
    // Screen activity inside the window is ignored.
    assert_eq!(m.on_output(c.at(2000), PROMPT), None);
    assert_eq!(m.tick(c.at(10_999), PROMPT), None);
    assert_eq!(m.state(), Done);
    // After the window the screen is trusted again.
    assert_eq!(m.tick(c.at(11_000), PROMPT), Some(NeedsInput));
}

#[test]
fn done_is_not_downgraded_to_idle_by_quiet_screen() {
    let c = clock();
    let mut m = machine(StateMode::HookScreen);
    m.on_hook(c.at(0), HookEvent::Done);
    assert_eq!(m.tick(c.at(60_000), BUSY), None);
    assert_eq!(m.state(), Done);
}

#[test]
fn hook_events_map_to_states() {
    let c = clock();
    let mut m = machine(StateMode::Hook);
    assert_eq!(m.on_hook(c.at(0), HookEvent::Busy), Some(Working));
    assert_eq!(m.on_hook(c.at(1), HookEvent::NeedsInput), Some(NeedsInput));
    assert_eq!(m.on_hook(c.at(2), HookEvent::Done), Some(Done));
}

#[test]
fn screen_mode_ignores_hooks() {
    let c = clock();
    let mut m = machine(StateMode::Screen);
    assert_eq!(m.on_hook(c.at(0), HookEvent::Done), None);
    assert_eq!(m.on_output(c.at(1), BUSY), Some(Working));
}

#[test]
fn hook_mode_ignores_screen_but_leaves_starting() {
    let c = clock();
    let mut m = machine(StateMode::Hook);
    assert_eq!(m.on_output(c.at(0), PROMPT), Some(Unknown));
    assert_eq!(m.on_output(c.at(1), PROMPT), None);
    assert_eq!(m.tick(c.at(60_000), PROMPT), None);
    assert_eq!(m.next_deadline(), None);
    assert_eq!(m.on_hook(c.at(60_001), HookEvent::Busy), Some(Working));
    assert_eq!(m.next_deadline(), None);
}

#[test]
fn delivery_policy() {
    for s in [Idle, Done, NeedsInput, Unknown] {
        assert!(can_deliver(s), "{s:?}");
    }
    for s in [Starting, Working, Exited] {
        assert!(!can_deliver(s), "{s:?}");
    }
}

#[test]
fn queue_holds_while_working_and_delivers_on_idle() {
    let mut q = SendQueue::default();
    q.push("y".into());
    assert_eq!(q.pop(Working), None);
    assert_eq!(q.pop(Starting), None);
    assert_eq!(q.len(), 1);
    assert_eq!(q.pop(Idle).as_deref(), Some("y"));
    assert!(q.is_empty());
}

#[test]
fn queue_delivers_one_message_at_a_time() {
    let mut q = SendQueue::default();
    q.push("first".into());
    q.push("second".into());
    assert_eq!(q.pop(NeedsInput).as_deref(), Some("first"));
    // No output yet: the agent has not picked it up, so the second waits.
    assert_eq!(q.pop(NeedsInput), None);
    q.on_output();
    assert_eq!(q.pop(Working), None);
    assert_eq!(q.pop(Done).as_deref(), Some("second"));
}

#[test]
fn queue_never_delivers_to_exited() {
    let mut q = SendQueue::default();
    q.push("y".into());
    assert_eq!(q.pop(Exited), None);
    assert_eq!(q.len(), 1);
}

#[test]
fn machine_and_queue_together() {
    let c = clock();
    let mut m = machine(StateMode::Screen);
    let mut q = SendQueue::default();
    m.on_output(c.at(0), BUSY);
    q.push("y".into());
    q.push("next".into());
    assert_eq!(q.pop(m.state()), None);
    m.on_output(c.at(100), PROMPT);
    assert_eq!(q.pop(m.state()).as_deref(), Some("y"));
    // Line-based prompt: the echoed answer keeps the prompt in the tail for a while.
    let answered = ["Do you want to proceed? (y/n) y", "step 1"];
    m.on_output(c.at(200), &answered);
    q.on_output();
    assert_eq!(m.state(), NeedsInput);
    // Delivery is not stuck: the prompt is still showing, so the next message goes out.
    assert_eq!(q.pop(m.state()).as_deref(), Some("next"));
    let later = [
        "Do you want to proceed? (y/n) y",
        "step 1",
        "step 2",
        "step 3",
        "step 4",
    ];
    assert_eq!(m.on_output(c.at(300), &later), Some(Working));
    assert_eq!(m.tick(c.at(3300), &later), Some(Idle));
}

#[test]
fn deadline_is_never_in_the_past_after_a_noop_tick() {
    let c = clock();
    // Prompt visible: needs_input has no deadline (only output clears it).
    let mut m = machine(StateMode::Screen);
    m.on_output(c.at(0), PROMPT);
    assert_eq!(m.tick(c.at(10_000), PROMPT), None);
    assert_eq!(m.next_deadline(), None);

    // Hook window: the deadline waits for it to end.
    let mut m = machine(StateMode::HookScreen);
    m.on_output(c.at(0), BUSY);
    m.on_hook(c.at(1000), HookEvent::Busy);
    let deadline = m.next_deadline().unwrap();
    assert_eq!(deadline, c.at(11_000));
    assert_eq!(m.tick(c.at(3000), BUSY), None);
    assert_eq!(m.tick(deadline, BUSY), Some(Idle));
}

#[test]
fn hook_busy_decays_to_idle_after_the_priority_window() {
    let c = clock();
    let mut m = machine(StateMode::HookScreen);
    m.on_output(c.at(0), BUSY);
    m.on_hook(c.at(0), HookEvent::Busy);
    assert_eq!(m.tick(c.at(9_999), BUSY), None);
    assert_eq!(m.tick(c.at(10_000), BUSY), Some(Idle));
}
