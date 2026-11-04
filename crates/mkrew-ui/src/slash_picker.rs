//! The `/` command picker (SPEC §20B.7, ADR 0094): typing `/` in the composer or the task
//! dialog's Prompt lists what typing `/` lists in the agent's own terminal. The daemon builds
//! the list (`slash.list`); this keeps the latest answer, the filter and the pick. Pure and
//! tested headlessly; `shell/slash.rs` renders it and reads keys with
//! [`crate::quick_open::key`].

use std::sync::atomic::{AtomicU64, Ordering};

use mkrew_protocol::{Cli, SlashItem, SlashListParams, SlashListResult, SlashSource};
use ulid::Ulid;

use crate::composer::Target;
use crate::palette::fuzzy;
use crate::quick_open::Key;
use crate::text_box::Input;

/// The name of omp's group row and the prefix of the skills it stands for.
const SKILL_PREFIX: &str = "skill:";

/// What one `slash.list` answers for, and the cache key: the agent profile, the project and the
/// task worktree (`None` = the project folder).
pub type SlashKey = (String, Ulid, Option<Ulid>);

/// One shown row: an index into [`SlashPicker::items`] and the matched chars of its name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    pub index: usize,
    pub matched: Vec<u32>,
}

/// Why no row is shown (the picker says so in one sentence).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Empty {
    /// No answer yet and nothing cached.
    Loading,
    /// The profile's CLI is unknown: Mkrew cannot list anything.
    UnknownCli,
    /// The CLI is known and every row is hidden or absent.
    NoCommands,
    /// Rows exist but the filter matches none.
    NoMatch,
}

/// The rows to show for `query`: with an empty one the CLI's own order; with one, the fuzzy
/// score on the name, ties in the CLI's order. In omp's list (it has a group row) the skills
/// stay behind their `skill:` row until the filter starts with `skill:`, except a skill whose
/// bare name starts with the filter while no other row's name does.
pub fn filter(items: &[SlashItem], query: &str) -> Vec<Row> {
    let query = query.trim();
    let grouped = items.iter().any(|item| item.group.is_some());
    let skill =
        |item: &SlashItem| grouped && item.group.is_none() && item.name.starts_with(SKILL_PREFIX);
    if query.is_empty() {
        return items
            .iter()
            .enumerate()
            .filter(|(_, item)| !skill(item))
            .map(|(index, _)| Row {
                index,
                matched: Vec::new(),
            })
            .collect();
    }
    let expanded = grouped
        && query
            .get(..SKILL_PREFIX.len())
            .is_some_and(|head| head.eq_ignore_ascii_case(SKILL_PREFIX));
    let mut found: Vec<(i64, Row)> = items
        .iter()
        .enumerate()
        // Expanded: the skills are listed, so their group row is redundant.
        .filter(|(_, item)| {
            if expanded {
                item.group.is_none()
            } else {
                !skill(item)
            }
        })
        .filter_map(|(index, item)| {
            let (score, matched) = fuzzy(query, &item.name)?;
            Some((score, Row { index, matched }))
        })
        .collect();
    if grouped && !expanded {
        let wanted = query.to_lowercase();
        let prefixed = |name: &str| name.to_lowercase().starts_with(&wanted);
        let taken = found.iter().any(|(_, row)| {
            items
                .get(row.index)
                .is_some_and(|item| prefixed(&item.name))
        });
        if !taken {
            let shift = u32::try_from(SKILL_PREFIX.len()).unwrap_or_default();
            for (index, item) in items.iter().enumerate().filter(|(_, item)| skill(item)) {
                let Some(bare) = item.name.strip_prefix(SKILL_PREFIX) else {
                    continue;
                };
                if prefixed(bare)
                    && let Some((score, matched)) = fuzzy(query, bare)
                {
                    let matched = matched.into_iter().map(|at| at + shift).collect();
                    found.push((score, Row { index, matched }));
                }
            }
        }
    }
    found.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.index.cmp(&b.1.index)));
    found.into_iter().map(|(_, row)| row).collect()
}

/// The chip at the right of a row.
pub fn source_label(source: &SlashSource) -> String {
    match source {
        SlashSource::BuiltIn => "built-in".to_owned(),
        SlashSource::Project => "project".to_owned(),
        SlashSource::User => "user".to_owned(),
        SlashSource::Managed => "managed".to_owned(),
        SlashSource::Plugin(name) => format!("plugin: {name}"),
        SlashSource::Profile => "profile".to_owned(),
    }
}

/// The footer note for files the scan could not read.
pub fn unreadable_note(count: usize) -> String {
    match count {
        1 => "1 file could not be read".to_owned(),
        count => format!("{count} files could not be read"),
    }
}

/// The open picker.
#[derive(Debug, Clone)]
pub struct SlashPicker {
    /// Where the pick is typed.
    pub target: Target,
    /// Index into [`Self::rows`] of the pick.
    pub selected: usize,
    scope: SlashKey,
    query: Input,
    items: Vec<SlashItem>,
    cli: Option<Cli>,
    /// Files the scan could not read, one line each.
    errors: Vec<String>,
    /// Why the last `slash.list` failed; the rows of an earlier answer stay.
    error: Option<String>,
    /// An answer or cached rows were seen: an empty list is then a fact, not a wait.
    ready: bool,
    /// Private: every change of `items` or `query` must filter again.
    rows: Vec<Row>,
    /// Number of the last request sent; older answers are dropped.
    sent: u64,
}

impl SlashPicker {
    /// A picker for `scope` showing the `cached` rows of an earlier answer at once (`None` =
    /// loading). The caller sends [`Self::request`].
    pub fn new(target: Target, scope: SlashKey, cached: Option<Vec<SlashItem>>) -> Self {
        let mut picker = Self {
            target,
            selected: 0,
            scope,
            query: Input::default(),
            items: Vec::new(),
            cli: None,
            errors: Vec::new(),
            error: None,
            ready: false,
            rows: Vec::new(),
            sent: 0,
        };
        picker.show(cached);
        picker
    }

    /// Takes `cached` as the rows (`None` = loading) and forgets what the last answer said.
    fn show(&mut self, cached: Option<Vec<SlashItem>>) {
        self.ready = cached.is_some();
        self.items = cached.unwrap_or_default();
        self.cli = None;
        self.errors.clear();
        self.error = None;
        self.refilter();
    }

    fn refilter(&mut self) {
        self.rows = filter(&self.items, self.query.as_str());
        self.selected = 0;
    }

    /// The agent changed (the dialog's Agent chips): the rows of `scope` replace these, the
    /// filter text stays. The caller sends a new [`Self::request`]; answers to the old one are
    /// dropped.
    pub fn retarget(&mut self, scope: SlashKey, cached: Option<Vec<SlashItem>>) {
        self.scope = scope;
        self.show(cached);
    }

    pub fn scope(&self) -> &SlashKey {
        &self.scope
    }

    pub fn query(&self) -> &Input {
        &self.query
    }

    /// A press or drag at byte `at` of the filter (see `Input::click`).
    pub fn click_query(&mut self, at: usize, extend: bool, clicks: usize) {
        self.query.click(at, extend, clicks);
    }

    pub fn items(&self) -> &[SlashItem] {
        &self.items
    }

    pub fn rows(&self) -> &[Row] {
        &self.rows
    }

    /// The CLI of the last answer (`None` before one, and when the CLI is unknown).
    pub fn cli(&self) -> Option<Cli> {
        self.cli
    }

    /// Why the last request failed.
    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    /// Files the last answer could not read.
    pub fn unreadable(&self) -> &[String] {
        &self.errors
    }

    /// The `slash.list` for the scope as it is now, with its number for [`Self::answered`].
    pub fn request(&mut self) -> (u64, SlashListParams) {
        static SENT: AtomicU64 = AtomicU64::new(0);
        self.sent = SENT.fetch_add(1, Ordering::Relaxed) + 1;
        self.error = None;
        let (profile_id, project_id, worktree_task_id) = self.scope.clone();
        (
            self.sent,
            SlashListParams {
                profile_id,
                project_id,
                worktree_task_id,
            },
        )
    }

    /// The answer to request `number`; kept only if no newer request was sent (a stale one is
    /// dropped). Rows that did not change keep the pick; new rows put it back on the best
    /// match. A failure keeps the rows already shown. Returns `true` when the answer was kept.
    pub fn answered(&mut self, number: u64, answer: Result<SlashListResult, String>) -> bool {
        if number != self.sent {
            return false;
        }
        match answer {
            Ok(result) => {
                let changed = result.items != self.items;
                (self.ready, self.cli, self.errors, self.error) =
                    (true, result.cli, result.errors, None);
                if changed {
                    self.items = result.items;
                    self.refilter();
                }
            }
            Err(error) => self.error = Some(error),
        }
        true
    }

    /// Applies a key to the filter and the pick; Enter, Tab and Esc are the caller's
    /// ([`Self::accept`], [`Self::escaped`]).
    pub fn apply(&mut self, key: Key) {
        match key {
            Key::Move(by) => self.step(by),
            Key::Edit(key) => {
                if self.query.edit(&key) {
                    self.refilter();
                }
            }
            Key::Insert(text) => self.insert(&text),
            Key::Close | Key::Open | Key::Paste => {}
        }
    }

    /// Types or pastes `text` into the filter (line breaks dropped: a name is one line).
    pub fn insert(&mut self, text: &str) {
        let line: String = text.chars().filter(|c| *c != '\r' && *c != '\n').collect();
        if self.query.insert(&line) {
            self.refilter();
        }
    }

    /// Moves the pick by `by`, wrapping around the rows.
    pub fn step(&mut self, by: i32) {
        let count = self.rows.len();
        if count == 0 {
            return;
        }
        let count = i64::try_from(count).unwrap_or(i64::MAX);
        let at = i64::try_from(self.selected).unwrap_or(0);
        self.selected = usize::try_from((at + i64::from(by)).rem_euclid(count)).unwrap_or(0);
    }

    /// Picks row `index` (a click).
    pub fn pick(&mut self, index: usize) {
        if index < self.rows.len() {
            self.selected = index;
        }
    }

    /// Enter or Tab: the text to type for the pick, `/name `. A group row types nothing: it
    /// sets the filter to `skill:` and the skills are listed. `None` when nothing is picked.
    pub fn accept(&mut self) -> Option<String> {
        let item = self.items.get(self.rows.get(self.selected)?.index)?;
        if item.group.is_some() {
            self.query.clear();
            self.query.insert(SKILL_PREFIX);
            self.refilter();
            return None;
        }
        Some(format!("/{} ", item.name))
    }

    /// Esc: the `/` that opened the picker and what was typed after it.
    pub fn escaped(&self) -> String {
        format!("/{}", self.query.as_str())
    }

    /// Why no row is shown, if none is. A failed request with no rows is not an empty state:
    /// the error line says it.
    pub fn empty(&self) -> Option<Empty> {
        if !self.rows.is_empty() || self.error.is_some() {
            return None;
        }
        if !self.ready {
            return Some(Empty::Loading);
        }
        if self.items.is_empty() {
            return Some(if self.cli.is_none() {
                Empty::UnknownCli
            } else {
                Empty::NoCommands
            });
        }
        Some(Empty::NoMatch)
    }
}

#[cfg(test)]
mod tests {
    use crate::task_dialog::DialogKey;

    use super::*;

    fn item(name: &str) -> SlashItem {
        SlashItem {
            name: name.into(),
            description: format!("About {name}"),
            argument_hint: None,
            source: SlashSource::BuiltIn,
            group: None,
        }
    }

    fn items(names: &[&str]) -> Vec<SlashItem> {
        names.iter().map(|name| item(name)).collect()
    }

    /// omp: a `skill:` group row standing for two skills, and the skills themselves.
    fn omp_items() -> Vec<SlashItem> {
        vec![
            item("help"),
            SlashItem {
                group: Some(2),
                description: "2 skills".into(),
                ..item("skill:")
            },
            item("skill:lint"),
            item("skill:test"),
            item("review"),
        ]
    }

    fn names<'a>(items: &'a [SlashItem], rows: &[Row]) -> Vec<&'a str> {
        rows.iter()
            .map(|row| items[row.index].name.as_str())
            .collect()
    }

    fn shown(picker: &SlashPicker) -> Vec<&str> {
        names(picker.items(), picker.rows())
    }

    fn answer(cli: Option<Cli>, items: Vec<SlashItem>) -> Result<SlashListResult, String> {
        Ok(SlashListResult {
            cli,
            items,
            errors: Vec::new(),
        })
    }

    fn scope() -> SlashKey {
        ("claude".into(), Ulid::new(), None)
    }

    fn type_in(picker: &mut SlashPicker, text: &str) {
        for c in text.chars() {
            picker.apply(Key::Insert(c.to_string()));
        }
    }

    #[test]
    fn an_empty_filter_keeps_the_clis_own_order() {
        let list = items(&["init", "compact", "clear"]);
        let rows = filter(&list, "");
        assert_eq!(names(&list, &rows), ["init", "compact", "clear"]);
        assert!(rows.iter().all(|row| row.matched.is_empty()));
        assert_eq!(
            names(&list, &filter(&list, "  ")),
            ["init", "compact", "clear"]
        );
    }

    #[test]
    fn a_filter_ranks_by_fuzzy_score_and_ties_keep_the_clis_order() {
        let list = items(&["review", "preview", "rename", "remember", "model"]);
        let rows = filter(&list, "re");
        // `preview` starts later in the word, so it scores lower although the CLI lists it
        // second; the three that start with `re` tie and keep the CLI's order.
        assert_eq!(
            names(&list, &rows),
            ["review", "rename", "remember", "preview"]
        );
        assert_eq!(rows[0].matched, vec![0, 1]);
        assert!(names(&list, &filter(&list, "zz")).is_empty());
    }

    #[test]
    fn the_skill_group_row_expands_when_the_filter_starts_with_skill() {
        let list = omp_items();
        assert_eq!(
            names(&list, &filter(&list, "")),
            ["help", "skill:", "review"],
            "the skills are behind their row"
        );
        assert_eq!(
            names(&list, &filter(&list, "skill:")),
            ["skill:lint", "skill:test"]
        );
        assert_eq!(names(&list, &filter(&list, "SKILL:te")), ["skill:test"]);
        // `sk` matches the group row's own name, so no skill shows on its bare name.
        assert_eq!(names(&list, &filter(&list, "sk")), ["skill:"]);
    }

    #[test]
    fn a_single_skill_shows_on_its_bare_name_when_no_other_row_starts_with_it() {
        let list = omp_items();
        let rows = filter(&list, "lin");
        assert_eq!(names(&list, &rows), ["skill:lint"]);
        assert_eq!(rows[0].matched, vec![6, 7, 8], "shifted past `skill:`");
        assert_eq!(names(&list, &filter(&list, "he")), ["help"]);
        assert!(
            names(&list, &filter(&list, "int")).is_empty(),
            "a bare name must start with the filter"
        );
    }

    #[test]
    fn without_a_group_row_skill_names_are_plain_rows() {
        // Claude: a command file `commands/skill/x.md` is named `skill:x`.
        let list = items(&["help", "skill:x"]);
        assert_eq!(names(&list, &filter(&list, "")), ["help", "skill:x"]);
    }

    #[test]
    fn a_pick_types_the_name_and_escape_types_what_was_typed() {
        let mut picker = SlashPicker::new(
            Target::TaskDialog,
            scope(),
            Some(items(&["review", "init"])),
        );
        assert_eq!(picker.accept().as_deref(), Some("/review "));
        picker.step(1);
        assert_eq!(picker.accept().as_deref(), Some("/init "));
        picker.step(1);
        assert_eq!(picker.selected, 0, "wraps");
        picker.pick(1);
        assert_eq!(picker.selected, 1);
        picker.pick(9);
        assert_eq!(picker.selected, 1, "out of range is ignored");

        type_in(&mut picker, "ini");
        assert_eq!(picker.escaped(), "/ini");
        assert_eq!(picker.accept().as_deref(), Some("/init "));
        type_in(&mut picker, "zz");
        assert_eq!(picker.empty(), Some(Empty::NoMatch));
        assert_eq!(picker.accept(), None);
        assert_eq!(picker.escaped(), "/inizz");
        picker.apply(Key::Edit(DialogKey::Backspace));
        picker.apply(Key::Edit(DialogKey::Backspace));
        assert_eq!(picker.empty(), None);
    }

    #[test]
    fn a_group_row_sets_the_filter_and_types_nothing() {
        let mut picker = SlashPicker::new(Target::TaskDialog, scope(), Some(omp_items()));
        picker.step(1);
        assert_eq!(shown(&picker)[picker.selected], "skill:");
        assert_eq!(picker.accept(), None, "nothing is typed");
        assert_eq!(picker.query().as_str(), "skill:");
        assert_eq!(shown(&picker), ["skill:lint", "skill:test"]);
        assert_eq!(picker.selected, 0);
        assert_eq!(picker.escaped(), "/skill:");
        assert_eq!(picker.accept().as_deref(), Some("/skill:lint "));
    }

    #[test]
    fn only_the_answer_to_the_latest_request_is_shown() {
        let mut picker = SlashPicker::new(Target::TaskDialog, scope(), None);
        let (first, params) = picker.request();
        assert_eq!(params.profile_id, "claude");
        assert_eq!(picker.empty(), Some(Empty::Loading));
        // Try again: a newer request supersedes the first.
        let (second, _) = picker.request();
        assert!(!picker.answered(first, answer(Some(Cli::Claude), items(&["old"]))));
        assert!(picker.items().is_empty());
        assert!(picker.answered(second, answer(Some(Cli::Claude), items(&["new"]))));
        assert_eq!(shown(&picker), ["new"]);

        // Numbers are unique across pickers: an answer meant for a closed one never lands in
        // the next.
        let mut next = SlashPicker::new(Target::TaskDialog, scope(), None);
        let (third, _) = next.request();
        assert!(third > second);
        assert!(!next.answered(second, answer(Some(Cli::Claude), items(&["late"]))));
        assert!(next.items().is_empty());
    }

    #[test]
    fn an_agent_change_replaces_the_rows_and_keeps_the_filter() {
        let project = Ulid::new();
        let claude: SlashKey = ("claude".into(), project, None);
        let mut picker =
            SlashPicker::new(Target::TaskDialog, claude, Some(items(&["review", "init"])));
        type_in(&mut picker, "re");
        assert_eq!(shown(&picker), ["review"]);
        let (claude_request, _) = picker.request();

        picker.retarget(
            ("codex".into(), project, None),
            Some(items(&["resume", "model"])),
        );
        assert_eq!(picker.query().as_str(), "re", "the filter stays");
        assert_eq!(shown(&picker), ["resume"]);
        let (codex_request, params) = picker.request();
        assert_eq!(
            (params.profile_id.as_str(), params.project_id),
            ("codex", project)
        );
        assert!(
            !picker.answered(
                claude_request,
                answer(Some(Cli::Claude), items(&["review"]))
            ),
            "claude's late answer"
        );
        assert!(picker.answered(
            codex_request,
            answer(Some(Cli::Codex), items(&["resume", "model", "review"]))
        ));
        assert_eq!(
            shown(&picker),
            ["resume", "review"],
            "a tie keeps the CLI's order"
        );

        // Nothing cached for the next agent: loading, still filtered by `re`.
        picker.retarget(("omp".into(), project, None), None);
        assert_eq!(picker.query().as_str(), "re");
        assert_eq!(picker.empty(), Some(Empty::Loading));
        assert_eq!(picker.cli(), None);
    }

    #[test]
    fn the_empty_states_say_why_nothing_is_listed() {
        let mut picker = SlashPicker::new(Target::TaskDialog, scope(), None);
        assert_eq!(picker.empty(), Some(Empty::Loading));

        let (number, _) = picker.request();
        picker.answered(number, Err("agent profile x not found".into()));
        assert_eq!(picker.empty(), None, "the error line says it");
        assert_eq!(picker.error(), Some("agent profile x not found"));
        let (number, _) = picker.request();
        assert_eq!(picker.error(), None, "Try again clears it");
        assert_eq!(picker.empty(), Some(Empty::Loading));

        picker.answered(number, answer(None, Vec::new()));
        assert_eq!(picker.empty(), Some(Empty::UnknownCli));
        let (number, _) = picker.request();
        picker.answered(number, answer(Some(Cli::Omp), Vec::new()));
        assert_eq!(picker.empty(), Some(Empty::NoCommands));
        let (number, _) = picker.request();
        picker.answered(number, answer(Some(Cli::Omp), items(&["model"])));
        assert_eq!(picker.empty(), None);
        type_in(&mut picker, "zz");
        assert_eq!(picker.empty(), Some(Empty::NoMatch));
    }

    #[test]
    fn cached_rows_show_at_once_and_a_failed_refresh_keeps_them() {
        let mut picker =
            SlashPicker::new(Target::TaskDialog, scope(), Some(items(&["a", "b", "c"])));
        assert_eq!(shown(&picker), ["a", "b", "c"], "no wait for the daemon");
        assert_eq!(picker.empty(), None);
        picker.step(2);

        let (number, _) = picker.request();
        picker.answered(number, Err("boom".into()));
        assert_eq!(shown(&picker), ["a", "b", "c"]);
        assert_eq!(picker.error(), Some("boom"));

        // The same rows again: the pick stays; different rows: back to the best match.
        let (number, _) = picker.request();
        picker.answered(number, answer(Some(Cli::Claude), items(&["a", "b", "c"])));
        assert_eq!((picker.selected, picker.error()), (2, None));
        assert_eq!(picker.cli(), Some(Cli::Claude));
        let (number, _) = picker.request();
        picker.answered(
            number,
            answer(Some(Cli::Claude), items(&["a", "b", "c", "d"])),
        );
        assert_eq!(picker.selected, 0);
    }

    #[test]
    fn unreadable_files_are_counted_for_the_footer() {
        let mut picker = SlashPicker::new(Target::TaskDialog, scope(), None);
        let (number, _) = picker.request();
        picker.answered(
            number,
            Ok(SlashListResult {
                cli: Some(Cli::Claude),
                items: items(&["init"]),
                errors: vec!["a.md: cannot read".into(), "b.md: cannot read".into()],
            }),
        );
        assert_eq!(picker.unreadable().len(), 2);
        assert_eq!(unreadable_note(1), "1 file could not be read");
        assert_eq!(unreadable_note(3), "3 files could not be read");
    }

    #[test]
    fn the_chip_names_the_source() {
        let label = |source| source_label(&source);
        assert_eq!(label(SlashSource::BuiltIn), "built-in");
        assert_eq!(label(SlashSource::Project), "project");
        assert_eq!(label(SlashSource::User), "user");
        assert_eq!(label(SlashSource::Managed), "managed");
        assert_eq!(label(SlashSource::Plugin("acme".into())), "plugin: acme");
        assert_eq!(label(SlashSource::Profile), "profile");
    }
}
