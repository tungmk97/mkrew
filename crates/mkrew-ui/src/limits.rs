//! The agents' usage limits (spec 2026-11-23 section 5.2, ADR 0106; the `agent.limits` path is
//! ADR 0102): the store that events and `limits.get` fill, and the chip the title bar draws from
//! it. Pure and tested headlessly; `shell/limits.rs` draws it.

use mkrew_protocol::{AgentLimits, Cli, Event, LimitWindow};

/// The newest limits per agent.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Limits {
    all: Vec<AgentLimits>,
}

impl Limits {
    /// Takes in `agent.limits`; returns whether anything changed.
    pub fn apply(&mut self, event: &Event) -> bool {
        match event {
            Event::AgentLimits(limits) => self.offer(limits.clone()),
            _ => false,
        }
    }

    /// The answer to `limits.get`, merged with the rule the daemon uses, so it never undoes a
    /// newer event that arrived first.
    pub fn load(&mut self, listed: Vec<AgentLimits>) -> bool {
        listed
            .into_iter()
            .fold(false, |changed, limits| self.offer(limits) | changed)
    }

    /// The new connection's list is about to replace these (events since the connect stay in
    /// the daemon's answer).
    pub fn clear(&mut self) {
        self.all.clear();
    }

    pub fn all(&self) -> &[AgentLimits] {
        &self.all
    }

    fn offer(&mut self, new: AgentLimits) -> bool {
        match self.all.iter_mut().find(|old| old.agent == new.agent) {
            Some(old) if new.supersedes(old) => {
                *old = new;
                true
            }
            Some(_) => false,
            None => {
                self.all.push(new);
                true
            }
        }
    }
}

/// How loud the chip is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Level {
    /// `theme.status_needs_input` (amber): 80 to 94%.
    Warn,
    /// `theme.danger_text`: 95% and more.
    Danger,
}

impl Level {
    /// `percent` is the shown, rounded value, so 79.5 reads "80%" and is amber. Below 80 there
    /// is nothing to say.
    pub fn of(percent: u32) -> Option<Level> {
        match percent {
            95.. => Some(Level::Danger),
            80..=94 => Some(Level::Warn),
            _ => None,
        }
    }
}

/// What the title bar draws: `text` after a warning icon, colored by `level`, with `tooltip`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Chip {
    /// `Claude 5h 84%`.
    pub text: String,
    pub level: Level,
    /// One line per live window with its reset time, then the click.
    pub tooltip: String,
}

/// One window whose number is still true.
struct Live<'a> {
    agent: &'static str,
    short: &'static str,
    long: &'static str,
    window: &'a LimitWindow,
    percent: u32,
}

impl Live<'_> {
    fn tip(&self, now_secs: i64, utc_offset: &dyn Fn(i64) -> i32) -> String {
        match self.window.resets_at {
            Some(resets) => format!(
                "{} {}: {}% used, resets {} ({})",
                self.agent,
                self.long,
                self.percent,
                local_clock(resets, now_secs, utc_offset),
                in_text(resets - now_secs)
            ),
            None => format!("{} {}: {}% used", self.agent, self.long, self.percent),
        }
    }
}

/// Claude's, then Codex's, 5-hour then weekly window, each one that has a number and has not
/// reset yet. Past its reset the number is stale: nothing until a new report.
fn live_windows(limits: &[AgentLimits], now_secs: i64) -> Vec<Live<'_>> {
    let mut live = Vec::new();
    for (cli, agent) in [(Cli::Claude, "Claude"), (Cli::Codex, "Codex")] {
        let Some(entry) = limits.iter().find(|l| l.agent == cli) else {
            continue;
        };
        for (short, long, window) in [
            ("5h", "5-hour limit", entry.five_hour.as_ref()),
            ("7d", "weekly limit", entry.weekly.as_ref()),
        ] {
            let Some(window) = window else {
                continue;
            };
            if window.used_percent.is_finite() && window.resets_at.is_none_or(|at| at > now_secs) {
                live.push(Live {
                    agent,
                    short,
                    long,
                    window,
                    percent: percent_of(window),
                });
            }
        }
    }
    live
}

/// The shown whole percentage: clamped to 0-100 and rounded.
fn percent_of(window: &LimitWindow) -> u32 {
    window.used_percent.clamp(0.0, 100.0).round() as u32
}

/// The chip for the title bar: the live window with the highest percent, only from 80% on; a
/// tie goes to the one that resets later (a window without a reset time loses a tie). `None`
/// when nothing is at 80%, so the chip never shows a value it does not have. `utc_offset(at)` is
/// the local offset in seconds at the Unix time `at` (for reset times).
pub fn chip(
    limits: &[AgentLimits],
    now_secs: i64,
    utc_offset: impl Fn(i64) -> i32,
) -> Option<Chip> {
    let live = live_windows(limits, now_secs);
    let (level, top) = live
        .iter()
        .filter_map(|w| Some((Level::of(w.percent)?, w)))
        .max_by_key(|(_, w)| (w.percent, w.window.resets_at.unwrap_or(i64::MIN)))?;
    let mut lines: Vec<String> = live.iter().map(|w| w.tip(now_secs, &utc_offset)).collect();
    lines.push("Click: open Settings".to_owned());
    Some(Chip {
        text: format!("{} {} {}%", top.agent, top.short, top.percent),
        level,
        tooltip: lines.join("\n"),
    })
}

/// `in 50 min`, `in 2 h 10 min`, `in 3 d 4 h` (rounded up to the minute); `now` when due.
pub fn in_text(secs: i64) -> String {
    if secs <= 0 {
        return "now".to_owned();
    }
    let mins = (secs + 59) / 60;
    if mins < 60 {
        return format!("in {mins} min");
    }
    if mins < 48 * 60 {
        return match (mins / 60, mins % 60) {
            (hours, 0) => format!("in {hours} h"),
            (hours, rest) => format!("in {hours} h {rest} min"),
        };
    }
    match (mins / 1440, mins % 1440 / 60) {
        (days, 0) => format!("in {days} d"),
        (days, hours) => format!("in {days} d {hours} h"),
    }
}

/// The local clock time of Unix time `at`: `04:07` on `now`'s local day, else `Fri 14:41`.
/// `utc_offset(t)` is the local offset in seconds east of UTC at Unix time `t`; `now`'s day
/// uses the offset at `now`, so a daylight saving change between the two does not move it.
pub fn local_clock(at: i64, now: i64, utc_offset: &dyn Fn(i64) -> i32) -> String {
    let local = at + i64::from(utc_offset(at));
    let today = (now + i64::from(utc_offset(now))).div_euclid(86_400);
    let (day, rest) = (local.div_euclid(86_400), local.rem_euclid(86_400));
    let clock = format!("{:02}:{:02}", rest / 3600, rest % 3600 / 60);
    if day == today {
        return clock;
    }
    // Day 0 (1970-01-01) was a Thursday.
    const DAYS: [&str; 7] = ["Thu", "Fri", "Sat", "Sun", "Mon", "Tue", "Wed"];
    format!("{} {clock}", DAYS[day.rem_euclid(7) as usize])
}

#[cfg(test)]
mod tests {
    use super::*;
    use mkrew_protocol::{Cli, LimitWindow};

    const NOW: i64 = 1_778_519_153;

    fn plus_seven(_: i64) -> i32 {
        25_200
    }

    fn window(percent: f32, resets_in: Option<i64>) -> Option<LimitWindow> {
        Some(LimitWindow {
            used_percent: percent,
            resets_at: resets_in.map(|s| NOW + s),
        })
    }

    fn limits(agent: Cli, five: Option<LimitWindow>, weekly: Option<LimitWindow>) -> AgentLimits {
        AgentLimits {
            agent,
            five_hour: five,
            weekly,
            observed_at: NOW,
        }
    }

    #[test]
    fn the_store_keeps_the_newest_limits_and_reports_changes() {
        let mut store = Limits::default();
        let old = limits(Cli::Claude, window(10.0, Some(60)), None);
        let newer = AgentLimits {
            observed_at: NOW + 1,
            ..limits(Cli::Claude, window(20.0, Some(60)), None)
        };
        assert!(store.apply(&Event::AgentLimits(old.clone())));
        assert!(!store.apply(&Event::AgentLimits(old.clone())), "a repeat");
        assert!(store.apply(&Event::AgentLimits(newer.clone())));
        assert!(
            !store.apply(&Event::AgentLimits(old.clone())),
            "an older event"
        );
        assert!(
            !store.load(vec![old]),
            "a late answer never undoes a newer event"
        );
        assert_eq!(store.all(), [newer]);
        assert!(
            !store.apply(&Event::SessionState(mkrew_protocol::SessionStateEvent {
                session_id: ulid::Ulid::nil(),
                state: mkrew_protocol::SessionState::Idle,
                source: mkrew_protocol::StateSource::Daemon,
                changed_at: 0,
            }))
        );
        store.clear();
        assert!(store.all().is_empty());
    }

    #[test]
    fn durations_read_as_minutes_hours_and_days() {
        for (secs, text) in [
            (0, "now"),
            (-30, "now"),
            (1, "in 1 min"),
            (60, "in 1 min"),
            (61, "in 2 min"),
            (3_000, "in 50 min"),
            (7_200, "in 2 h"),
            (7_800, "in 2 h 10 min"),
            (172_800, "in 2 d"),
            (190_800, "in 2 d 5 h"),
            (259_200, "in 3 d"),
        ] {
            assert_eq!(in_text(secs), text, "{secs}");
        }
    }

    #[test]
    fn reset_times_are_local_and_name_the_day_when_it_is_not_today() {
        // 2026-05-12 00:05 local (UTC+7) now; the 5-hour window resets 04:07 the same day.
        assert_eq!(local_clock(1_778_533_631, NOW, &plus_seven), "04:07");
        // Three days on: Friday.
        assert_eq!(
            local_clock(1_778_571_719 + 3 * 86_400, NOW, &plus_seven),
            "Fri 14:41"
        );
        // The same instant in UTC is still the previous day for `now`.
        assert_eq!(local_clock(1_778_533_631, NOW, &|_| 0), "21:07");
        assert_eq!(local_clock(0, 0, &|_| 0), "00:00");
    }

    /// `now` is judged by the offset at `now`: a clock that moves forward an hour between `now`
    /// (23:59 on day 999) and the reset (01:30 on day 1000) still calls the reset tomorrow.
    #[test]
    fn today_uses_the_offset_at_now_across_a_daylight_saving_change() {
        let change = 1_000 * 86_400;
        let offset = |t: i64| if t < change { 0 } else { 3_600 };
        assert_eq!(
            local_clock(change + 1_800, change - 60, &offset),
            "Wed 01:30"
        );
    }

    fn chip_of(list: &[AgentLimits]) -> Option<Chip> {
        chip(list, NOW, plus_seven)
    }

    fn text_of(list: &[AgentLimits]) -> Option<String> {
        chip_of(list).map(|chip| chip.text)
    }

    #[test]
    fn the_chip_is_amber_from_80_and_red_from_95_on_the_rounded_percent() {
        let at = |percent: f32| chip_of(&[limits(Cli::Claude, window(percent, Some(600)), None)]);
        assert_eq!(at(79.4), None, "79% is below the line");
        assert_eq!(at(-5.0), None);
        assert_eq!(at(f32::NAN), None, "not a number: nothing to show");
        for (percent, text, level) in [
            (79.5, "Claude 5h 80%", Level::Warn),
            (80.0, "Claude 5h 80%", Level::Warn),
            (94.4, "Claude 5h 94%", Level::Warn),
            (94.5, "Claude 5h 95%", Level::Danger),
            (100.0, "Claude 5h 100%", Level::Danger),
            (150.0, "Claude 5h 100%", Level::Danger),
        ] {
            let chip = at(percent).unwrap_or_else(|| panic!("{percent} shows a chip"));
            assert_eq!((chip.text.as_str(), chip.level), (text, level), "{percent}");
        }
    }

    #[test]
    fn no_data_and_nothing_at_80_show_no_chip() {
        assert_eq!(chip_of(&[]), None);
        let calm = [
            limits(Cli::Claude, window(42.0, Some(600)), window(18.0, None)),
            limits(Cli::Codex, window(79.0, Some(600)), None),
        ];
        assert_eq!(chip_of(&calm), None);
        let omp = [limits(Cli::Omp, window(99.0, Some(600)), None)];
        assert_eq!(chip_of(&omp), None, "only Claude and Codex are listed");
    }

    #[test]
    fn the_highest_window_wins_across_agents() {
        let list = [
            limits(
                Cli::Claude,
                window(82.0, Some(600)),
                window(88.0, Some(600)),
            ),
            limits(Cli::Codex, window(91.0, Some(600)), window(10.0, None)),
        ];
        assert_eq!(text_of(&list).as_deref(), Some("Codex 5h 91%"));
        let chip = chip_of(&list).unwrap();
        assert_eq!(chip.level, Level::Warn);
    }

    #[test]
    fn a_tie_goes_to_the_window_that_resets_later() {
        let sooner = limits(Cli::Claude, window(90.0, Some(600)), None);
        let later = limits(Cli::Codex, None, window(90.0, Some(7_200)));
        for list in [
            [sooner.clone(), later.clone()],
            [later.clone(), sooner.clone()],
        ] {
            assert_eq!(text_of(&list).as_deref(), Some("Codex 7d 90%"));
        }
        let unknown = limits(Cli::Claude, window(90.0, None), None);
        let known = limits(Cli::Codex, window(90.0, Some(60)), None);
        assert_eq!(
            text_of(&[unknown, known]).as_deref(),
            Some("Codex 5h 90%"),
            "a known reset time beats an unknown one"
        );
    }

    #[test]
    fn a_window_past_its_reset_is_ignored_until_a_new_report() {
        let text = |five: Option<LimitWindow>, weekly: Option<LimitWindow>| {
            text_of(&[limits(Cli::Claude, five, weekly)])
        };
        assert_eq!(
            text(window(99.0, Some(-1)), window(85.0, Some(60))).as_deref(),
            Some("Claude 7d 85%"),
            "the expired 5h is not the top"
        );
        assert_eq!(
            text(window(99.0, Some(0)), window(85.0, Some(60))).as_deref(),
            Some("Claude 7d 85%"),
            "now is past"
        );
        assert_eq!(
            text(window(99.0, Some(-1)), window(90.0, Some(-5))),
            None,
            "both past: no chip"
        );
        assert_eq!(
            text(window(99.0, None), None).as_deref(),
            Some("Claude 5h 99%"),
            "no reset time: kept"
        );
        assert_eq!(text(None, None), None);
    }

    #[test]
    fn the_tooltip_lists_every_live_window_with_its_reset_time() {
        let list = [
            limits(Cli::Claude, window(84.0, Some(3_000)), window(40.0, None)),
            limits(
                Cli::Codex,
                window(10.0, Some(7_200)),
                window(95.0, Some(-1)),
            ),
        ];
        let chip = chip_of(&list).unwrap();
        assert_eq!(chip.text, "Claude 5h 84%", "the expired 95% is not the top");
        assert_eq!(
            chip.tooltip,
            "Claude 5-hour limit: 84% used, resets 00:55 (in 50 min)\n\
             Claude weekly limit: 40% used\n\
             Codex 5-hour limit: 10% used, resets 02:05 (in 2 h)\n\
             Click: open Settings"
        );
    }
}
