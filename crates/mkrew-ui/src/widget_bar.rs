//! The widget bar's model (spec 2026-11-22, ADR 0102): which widgets show, their text and
//! colors, which drop when the window is narrow and how a hide toggle edits `widgets_hidden`.
//! Pure and tested headlessly; `shell/widget_bar.rs` draws it.

use mkrew_protocol::{AgentLimits, Cli, Event, LimitWindow};

use crate::git_summary::GitSummary;

/// The four widgets of the bar.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum WidgetId {
    Folder,
    Git,
    Limits,
    Version,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    Left,
    Right,
}

impl WidgetId {
    /// Bar order: folder and git on the left, limits and version on the right.
    pub const ALL: [WidgetId; 4] = [
        WidgetId::Folder,
        WidgetId::Git,
        WidgetId::Limits,
        WidgetId::Version,
    ];

    /// The id kept in `[ui] widgets_hidden`.
    pub fn id(self) -> &'static str {
        match self {
            WidgetId::Folder => "folder",
            WidgetId::Git => "git",
            WidgetId::Limits => "limits",
            WidgetId::Version => "version",
        }
    }

    /// The name in the bar's right-click menu.
    pub fn title(self) -> &'static str {
        match self {
            WidgetId::Folder => "Folder",
            WidgetId::Git => "Git branch",
            WidgetId::Limits => "Agent limits",
            WidgetId::Version => "Version",
        }
    }

    pub fn side(self) -> Side {
        match self {
            WidgetId::Folder | WidgetId::Git => Side::Left,
            WidgetId::Limits | WidgetId::Version => Side::Right,
        }
    }
}

/// How loud a part of a label is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Level {
    /// Muted text.
    Muted,
    /// `theme.status_needs_input` (amber): a limit at 80% or more.
    Warn,
    /// `theme.danger_text`: a limit at 95% or more.
    Danger,
}

impl Level {
    /// `percent` is the shown, rounded value, so 79.5 reads "80%" and is amber.
    pub fn of(percent: u32) -> Self {
        match percent {
            95.. => Level::Danger,
            80..=94 => Level::Warn,
            _ => Level::Muted,
        }
    }
}

/// What a click on a widget does; the shell runs it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Click {
    /// The workspace folder in the file explorer.
    OpenFolder,
    /// Opens Git; with Git already open, the next changed file (wrapping).
    Git,
    /// Settings at the Integrations pages.
    OpenLimits,
    /// The update check.
    CheckUpdate,
}

/// A run of label text in one color.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Part {
    pub text: String,
    pub level: Level,
}

impl Part {
    fn muted(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            level: Level::Muted,
        }
    }
}

/// One widget to draw: its label (after the icon of its id), tooltip and click.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Widget {
    pub id: WidgetId,
    pub parts: Vec<Part>,
    pub tooltip: String,
    pub click: Click,
}

impl Widget {
    /// The label as one line (parts joined by a space).
    pub fn label(&self) -> String {
        self.parts
            .iter()
            .map(|part| part.text.as_str())
            .collect::<Vec<_>>()
            .join(" ")
    }
}

/// What the daemon told the bar: the newest limits per agent.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct WidgetData {
    limits: Vec<AgentLimits>,
}

impl WidgetData {
    /// Takes in `agent.limits`; returns whether anything changed.
    pub fn apply(&mut self, event: &Event) -> bool {
        match event {
            Event::AgentLimits(limits) => self.offer(limits.clone()),
            _ => false,
        }
    }

    /// The answer to `limits.get`, merged with the rule the daemon uses, so it never undoes a
    /// newer event that arrived first.
    pub fn load_limits(&mut self, listed: Vec<AgentLimits>) -> bool {
        listed
            .into_iter()
            .fold(false, |changed, limits| self.offer(limits) | changed)
    }

    /// The new connection's list is about to replace the limits (events since the connect
    /// stay in the daemon's answer).
    pub fn clear_limits(&mut self) {
        self.limits.clear();
    }

    pub fn limits(&self) -> &[AgentLimits] {
        &self.limits
    }

    fn offer(&mut self, new: AgentLimits) -> bool {
        match self.limits.iter_mut().find(|old| old.agent == new.agent) {
            Some(old) if new.supersedes(old) => {
                *old = new;
                true
            }
            Some(_) => false,
            None => {
                self.limits.push(new);
                true
            }
        }
    }
}

/// Everything [`widget_list`] reads.
pub struct Input<'a> {
    /// The shown workspace's name and root folder.
    pub workspace: Option<(&'a str, &'a str)>,
    /// Its Git summary (already polled for the title bar).
    pub git: Option<&'a GitSummary>,
    pub data: &'a WidgetData,
    /// `env!("CARGO_PKG_VERSION")`.
    pub version: &'a str,
    /// `[ui] widgets_hidden`; unknown ids are ignored.
    pub hidden: &'a [String],
    pub now_secs: i64,
}

/// The widgets to draw, in bar order. A widget with nothing to show is absent, never "0%".
/// `utc_offset(at)` is the local offset in seconds at the Unix time `at` (for reset times).
pub fn widget_list(input: &Input<'_>, utc_offset: impl Fn(i64) -> i32) -> Vec<Widget> {
    WidgetId::ALL
        .into_iter()
        .filter(|id| !is_hidden(input.hidden, *id))
        .filter_map(|id| match id {
            WidgetId::Folder => folder(input),
            WidgetId::Git => git(input),
            WidgetId::Limits => limits(input, &utc_offset),
            WidgetId::Version => Some(version(input)),
        })
        .collect()
}

fn folder(input: &Input<'_>) -> Option<Widget> {
    let (name, path) = input.workspace?;
    Some(Widget {
        id: WidgetId::Folder,
        parts: vec![Part::muted(name)],
        tooltip: format!("{path}\nClick: open the folder in the file explorer"),
        click: Click::OpenFolder,
    })
}

fn git(input: &Input<'_>) -> Option<Widget> {
    let summary = input.git?;
    // The count is its own part, so a long branch name ellipsizes without hiding it.
    let mut parts = vec![Part::muted(summary.branch_label()?)];
    let changed = summary.changed();
    if changed > 0 {
        parts.extend([Part::muted("·"), Part::muted(changed.to_string())]);
    }
    let tooltip = summary.branch_tooltip().unwrap_or_default();
    Some(Widget {
        id: WidgetId::Git,
        parts,
        tooltip: format!("{tooltip}\nClick: open Git, again for the next changed file"),
        click: Click::Git,
    })
}

/// `Claude 5h 42% · 7d 18% | Codex 5h 10% · 7d 3%`: one group per agent that has a live window.
fn limits(input: &Input<'_>, utc_offset: &dyn Fn(i64) -> i32) -> Option<Widget> {
    let mut parts: Vec<Part> = Vec::new();
    let mut tips: Vec<String> = Vec::new();
    for (agent, name) in [(Cli::Claude, "Claude"), (Cli::Codex, "Codex")] {
        let Some(limits) = input.data.limits().iter().find(|l| l.agent == agent) else {
            continue;
        };
        let windows: Vec<(&str, &str, &LimitWindow)> = [
            ("5h", "5-hour limit", limits.five_hour.as_ref()),
            ("7d", "weekly limit", limits.weekly.as_ref()),
        ]
        .into_iter()
        .filter_map(|(short, long, window)| Some((short, long, window?)))
        // Past its reset the number is stale: nothing until a new report.
        .filter(|(_, _, w)| {
            w.used_percent.is_finite() && w.resets_at.is_none_or(|at| at > input.now_secs)
        })
        .collect();
        if windows.is_empty() {
            continue;
        }
        if !parts.is_empty() {
            parts.push(Part::muted("|"));
        }
        parts.push(Part::muted(name));
        for (at, (short, long, window)) in windows.into_iter().enumerate() {
            if at > 0 {
                parts.push(Part::muted("·"));
            }
            let percent = percent_of(window);
            parts.push(Part {
                text: format!("{short} {percent}%"),
                level: Level::of(percent),
            });
            tips.push(match window.resets_at {
                Some(resets) => format!(
                    "{name} {long}: {percent}% used, resets {} ({})",
                    local_clock(resets, input.now_secs, utc_offset),
                    in_text(resets - input.now_secs)
                ),
                None => format!("{name} {long}: {percent}% used"),
            });
        }
    }
    if parts.is_empty() {
        return None;
    }
    tips.push("Click: open Settings".to_owned());
    Some(Widget {
        id: WidgetId::Limits,
        parts,
        tooltip: tips.join("\n"),
        click: Click::OpenLimits,
    })
}

/// The shown whole percentage: clamped to 0-100 and rounded.
fn percent_of(window: &LimitWindow) -> u32 {
    window.used_percent.clamp(0.0, 100.0).round() as u32
}

fn version(input: &Input<'_>) -> Widget {
    Widget {
        id: WidgetId::Version,
        parts: vec![Part::muted(format!("v{}", input.version))],
        tooltip: format!("Mkrew {}\nClick: check for updates", input.version),
        click: Click::CheckUpdate,
    }
}

// ponytail: widths are estimated from the character count, not measured; the bar drops whole
// widgets, so a few pixels of error only move the moment a widget drops. Measure with the
// text system if a font makes it visibly early or late.
const CHAR_EM: f32 = 0.55;
const ICON_EM: f32 = 1.6;
const WIDGET_PAD_EM: f32 = 0.6;
const GAP_EM: f32 = 0.9;
const BAR_PAD_EM: f32 = 1.2;
/// The folder and branch labels ellipsize past this (`theme::WIDGET_BAR_LABEL_MAX_WIDTH`).
const LABEL_MAX_CHARS: usize = 20;

fn widget_width(widget: &Widget, em: f32) -> f32 {
    let chars: usize = widget
        .parts
        .iter()
        .map(|p| p.text.chars().count() + 1)
        .sum();
    let chars = match widget.id {
        WidgetId::Folder | WidgetId::Git => chars.min(LABEL_MAX_CHARS),
        _ => chars,
    };
    (ICON_EM + chars as f32 * CHAR_EM + WIDGET_PAD_EM) * em
}

/// The estimated width of `list` laid out in the bar, in px at `em` px per em.
pub fn list_width(list: &[Widget], em: f32) -> f32 {
    let widgets: f32 = list.iter().map(|w| widget_width(w, em)).sum();
    let gaps = GAP_EM * em * list.len().saturating_sub(1) as f32;
    widgets + gaps + BAR_PAD_EM * em
}

/// Drops the limits widget while `list` is wider than `available` px. Folder, git and version
/// stay.
pub fn fit(mut list: Vec<Widget>, available: f32, em: f32) -> Vec<Widget> {
    if list_width(&list, em) > available {
        list.retain(|widget| widget.id != WidgetId::Limits);
    }
    list
}

pub fn is_hidden(hidden: &[String], id: WidgetId) -> bool {
    hidden.iter().any(|h| h == id.id())
}

/// `hidden` with `id` hidden, or shown again when it was hidden. Every other id, known or
/// not, stays where it is.
pub fn toggled(hidden: &[String], id: WidgetId) -> Vec<String> {
    if is_hidden(hidden, id) {
        return hidden.iter().filter(|h| *h != id.id()).cloned().collect();
    }
    let mut out = hidden.to_vec();
    out.push(id.id().to_owned());
    out
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
    use crate::git_summary::RepoSummary;

    const NOW: i64 = 1_778_519_153;

    fn plus_seven(_: i64) -> i32 {
        25_200
    }

    fn repo(branch: Option<&str>, modified: usize, untracked: usize) -> RepoSummary {
        RepoSummary {
            repo: String::new(),
            branch: branch.map(str::to_owned),
            head: Some("1a2b3c4d5e6f".into()),
            modified,
            untracked,
        }
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

    fn data(list: Vec<AgentLimits>) -> WidgetData {
        let mut data = WidgetData::default();
        data.load_limits(list);
        data
    }

    fn list(
        git: Option<&GitSummary>,
        data: &WidgetData,
        hidden: &[String],
        workspace: Option<(&str, &str)>,
    ) -> Vec<Widget> {
        widget_list(
            &Input {
                workspace,
                git,
                data,
                version: "0.5.3",
                hidden,
                now_secs: NOW,
            },
            plus_seven,
        )
    }

    fn ids(list: &[Widget]) -> Vec<WidgetId> {
        list.iter().map(|w| w.id).collect()
    }

    fn full() -> Vec<Widget> {
        let git = GitSummary {
            repos: vec![repo(Some("master"), 2, 1)],
        };
        let data = data(vec![
            limits(
                Cli::Claude,
                window(42.4, Some(7_800)),
                window(18.0, Some(3 * 86_400)),
            ),
            limits(
                Cli::Codex,
                window(10.0, Some(4 * 3600)),
                window(3.0, Some(6 * 86_400)),
            ),
        ]);
        list(Some(&git), &data, &[], Some(("api", "C:/work/api")))
    }

    #[test]
    fn without_data_only_the_version_shows() {
        let none = list(None, &WidgetData::default(), &[], None);
        assert_eq!(ids(&none), [WidgetId::Version]);
        assert_eq!(none[0].label(), "v0.5.3");
    }

    #[test]
    fn a_full_state_lists_the_widgets_in_bar_order() {
        let list = full();
        assert_eq!(ids(&list), WidgetId::ALL);
        let labels: Vec<String> = list.iter().map(Widget::label).collect();
        assert_eq!(
            labels,
            [
                "api",
                "master · 3",
                "Claude 5h 42% · 7d 18% | Codex 5h 10% · 7d 3%",
                "v0.5.3"
            ]
        );
        let clicks: Vec<Click> = list.iter().map(|w| w.click).collect();
        assert_eq!(
            clicks,
            [
                Click::OpenFolder,
                Click::Git,
                Click::OpenLimits,
                Click::CheckUpdate
            ]
        );
        assert_eq!(WidgetId::Folder.side(), Side::Left);
        assert_eq!(WidgetId::Git.side(), Side::Left);
        assert_eq!(WidgetId::Limits.side(), Side::Right);
    }

    #[test]
    fn tooltips_carry_the_full_value_and_the_click() {
        let list = full();
        assert_eq!(
            list[0].tooltip,
            "C:/work/api\nClick: open the folder in the file explorer"
        );
        assert!(
            list[1]
                .tooltip
                .starts_with("(root) \u{2014} master · 1a2b3c4\n"),
            "{}",
            list[1].tooltip
        );
        assert!(
            list[1]
                .tooltip
                .ends_with("Click: open Git, again for the next changed file")
        );
        let limits = &list[2].tooltip;
        assert!(
            limits.contains("Claude 5-hour limit: 42% used, resets 02:15 (in 2 h 10 min)"),
            "{limits}"
        );
        assert!(
            limits.contains("Claude weekly limit: 18% used, resets Fri 00:05 (in 3 d)"),
            "{limits}"
        );
        assert!(limits.ends_with("Click: open Settings"), "{limits}");
        assert_eq!(list[3].tooltip, "Mkrew 0.5.3\nClick: check for updates");
    }

    #[test]
    fn the_git_widget_counts_changes_and_names_several_repositories() {
        let one = |summary: GitSummary| {
            list(Some(&summary), &WidgetData::default(), &[], None)
                .iter()
                .find(|w| w.id == WidgetId::Git)
                .map(Widget::label)
        };
        assert_eq!(
            one(GitSummary {
                repos: vec![repo(Some("master"), 0, 0)]
            })
            .as_deref(),
            Some("master")
        );
        assert_eq!(
            one(GitSummary {
                repos: vec![repo(None, 1, 0)]
            })
            .as_deref(),
            Some("detached · 1")
        );
        assert_eq!(
            one(GitSummary {
                repos: vec![repo(Some("a"), 1, 0), repo(Some("b"), 0, 2)]
            })
            .as_deref(),
            Some("2 repos · 3")
        );
        assert_eq!(one(GitSummary::default()), None, "no repository, no widget");
    }

    /// The UI ellipsizes only the first part of the Git widget, so the count must not be in it.
    #[test]
    fn the_change_count_is_not_part_of_the_branch_name() {
        let summary = GitSummary {
            repos: vec![repo(Some("feature/long"), 2, 1)],
        };
        let widgets = list(Some(&summary), &WidgetData::default(), &[], None);
        let git = widgets.iter().find(|w| w.id == WidgetId::Git).unwrap();
        let texts: Vec<&str> = git.parts.iter().map(|p| p.text.as_str()).collect();
        assert_eq!(texts, ["feature/long", "·", "3"]);
    }

    #[test]
    fn limit_colors_follow_the_shown_percent() {
        let level_of = |percent: f32| {
            let data = data(vec![limits(Cli::Claude, window(percent, Some(600)), None)]);
            let widgets = list(None, &data, &[], None);
            let widget = widgets.iter().find(|w| w.id == WidgetId::Limits).unwrap();
            widget
                .parts
                .iter()
                .find(|p| p.text.starts_with("5h"))
                .unwrap()
                .clone()
        };
        for (percent, text, level) in [
            (79.4, "5h 79%", Level::Muted),
            (79.5, "5h 80%", Level::Warn),
            (94.4, "5h 94%", Level::Warn),
            (94.5, "5h 95%", Level::Danger),
            (100.0, "5h 100%", Level::Danger),
            (150.0, "5h 100%", Level::Danger),
            (-5.0, "5h 0%", Level::Muted),
        ] {
            let part = level_of(percent);
            assert_eq!((part.text.as_str(), part.level), (text, level), "{percent}");
        }
    }

    #[test]
    fn a_window_past_its_reset_shows_nothing_until_a_new_report() {
        let label = |five: Option<LimitWindow>, weekly: Option<LimitWindow>| {
            let data = data(vec![limits(Cli::Claude, five, weekly)]);
            list(None, &data, &[], None)
                .iter()
                .find(|w| w.id == WidgetId::Limits)
                .map(Widget::label)
        };
        assert_eq!(
            label(window(50.0, Some(-1)), window(18.0, Some(60))).as_deref(),
            Some("Claude 7d 18%")
        );
        assert_eq!(
            label(window(50.0, Some(0)), window(18.0, Some(60))).as_deref(),
            Some("Claude 7d 18%"),
            "now is past"
        );
        assert_eq!(
            label(window(50.0, Some(-1)), window(18.0, Some(-5))),
            None,
            "both past: absent"
        );
        assert_eq!(
            label(window(50.0, None), None).as_deref(),
            Some("Claude 5h 50%"),
            "no reset time: kept"
        );
        assert_eq!(label(None, None), None);
    }

    #[test]
    fn only_claude_and_codex_are_listed() {
        let data = data(vec![limits(Cli::Omp, window(50.0, Some(60)), None)]);
        assert_eq!(ids(&list(None, &data, &[], None)), [WidgetId::Version]);
        let data = self::data(vec![limits(Cli::Codex, window(50.0, Some(60)), None)]);
        let widgets = list(None, &data, &[], None);
        assert_eq!(widgets[0].label(), "Codex 5h 50%");
    }

    #[test]
    fn hidden_widgets_are_left_out_and_unknown_ids_are_ignored() {
        let hidden = vec!["git".to_owned(), "clock".to_owned(), "limits".to_owned()];
        let widgets = {
            let git = GitSummary {
                repos: vec![repo(Some("m"), 0, 0)],
            };
            let data = data(vec![limits(Cli::Claude, window(1.0, Some(60)), None)]);
            list(Some(&git), &data, &hidden, Some(("api", "/api")))
        };
        assert_eq!(ids(&widgets), [WidgetId::Folder, WidgetId::Version]);
        assert!(is_hidden(&hidden, WidgetId::Git));
        assert!(!is_hidden(&hidden, WidgetId::Folder));
    }

    #[test]
    fn toggling_keeps_ids_this_build_does_not_know_in_order() {
        let hidden = vec!["clock".to_owned(), "git".to_owned()];
        assert_eq!(toggled(&hidden, WidgetId::Git), ["clock"]);
        assert_eq!(
            toggled(&hidden, WidgetId::Limits),
            ["clock", "git", "limits"]
        );
        assert_eq!(
            toggled(&toggled(&hidden, WidgetId::Limits), WidgetId::Limits),
            hidden
        );
        assert_eq!(toggled(&[], WidgetId::Version), ["version"]);
    }

    #[test]
    fn a_narrow_window_drops_limits() {
        let list = full();
        let em = 13.0;
        let all = list_width(&list, em);
        assert_eq!(fit(list.clone(), f32::MAX, em), list);
        let no_limits: Vec<Widget> = list
            .iter()
            .filter(|w| w.id != WidgetId::Limits)
            .cloned()
            .collect();
        assert!(list_width(&no_limits, em) < all);
        assert_eq!(fit(list.clone(), all - 1.0, em), no_limits);
        // Folder, git and version stay however narrow.
        assert_eq!(
            ids(&fit(list, 0.0, em)),
            [WidgetId::Folder, WidgetId::Git, WidgetId::Version]
        );
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

    #[test]
    fn the_folder_and_git_sit_left_and_the_rest_right() {
        let sides: Vec<Side> = WidgetId::ALL.into_iter().map(WidgetId::side).collect();
        assert_eq!(sides, [Side::Left, Side::Left, Side::Right, Side::Right]);
    }

    #[test]
    fn widget_data_keeps_the_newest_limits_and_reports_changes() {
        let mut data = WidgetData::default();
        let old = limits(Cli::Claude, window(10.0, Some(60)), None);
        let newer = AgentLimits {
            observed_at: NOW + 1,
            ..limits(Cli::Claude, window(20.0, Some(60)), None)
        };
        assert!(data.apply(&Event::AgentLimits(old.clone())));
        assert!(!data.apply(&Event::AgentLimits(old.clone())), "a repeat");
        assert!(data.apply(&Event::AgentLimits(newer.clone())));
        assert!(
            !data.apply(&Event::AgentLimits(old.clone())),
            "an older event"
        );
        assert!(
            !data.load_limits(vec![old]),
            "a late answer never undoes a newer event"
        );
        assert_eq!(data.limits(), [newer]);

        assert!(
            !data.apply(&Event::SessionState(mkrew_protocol::SessionStateEvent {
                session_id: ulid::Ulid::nil(),
                state: mkrew_protocol::SessionState::Idle,
                source: mkrew_protocol::StateSource::Daemon,
                changed_at: 0,
            }))
        );
        data.clear_limits();
        assert!(data.limits().is_empty());
    }

    #[test]
    fn every_widget_has_a_menu_row_state_and_a_title() {
        let hidden = vec!["limits".to_owned(), "clock".to_owned()];
        let rows: Vec<(&str, bool)> = WidgetId::ALL
            .into_iter()
            .map(|id| (id.title(), !is_hidden(&hidden, id)))
            .collect();
        assert_eq!(
            rows,
            [
                ("Folder", true),
                ("Git branch", true),
                ("Agent limits", false),
                ("Version", true)
            ]
        );
    }
}
