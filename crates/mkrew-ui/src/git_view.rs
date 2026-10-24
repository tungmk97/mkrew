//! Git view-model (SPEC §12B): the `git.status` of one workspace in the shown mode (Working
//! changes grouped Staged / Unstaged / Untracked, or Against base), the selected changed file
//! and its `git.diff` laid out as rows (hunk headers, lines, and "unchanged lines" gaps between
//! hunks), and the current hunk moved by `n` / `p`. `git.changed` re-asks the status and the
//! shown diff. Stage / unstage a file or hunk (`s` / `u`) and the discard confirmation; the
//! commit box and Fetch / Push live in [`crate::git_actions`]. The History section (`h`)
//! lists commits instead ([`crate::git_history`]); their files' diffs show here too.
//! Pure and tested headlessly; `main.rs` does the requests and renders it.

use std::collections::HashSet;
use std::ops::Range;
use std::rc::Rc;

use gpui::Keystroke;
use mkrew_core::settings::DiffMode;
use mkrew_protocol::{
    ChangeKind, ChangedFile, CommentContext, Commit, CommitDetail, DiffLine, DiffLineKind, Event,
    FileDiff, GitDiffParams, GitDiscardParams, GitLogParams, GitMode, GitSetViewedParams,
    GitShowParams, GitStageParams, GitStatus, GitStatusParams, HunkTarget, Request, Session,
    SessionFiles, SessionFilesParams, SessionState, ViewedFile,
};
use ulid::Ulid;

use crate::comments::{self, Comments, Selection};
use crate::git_actions::{CommitBox, Remotes};
use crate::git_graph::{self, GraphCell, GraphRow};
use crate::git_history::{Detail, History};
use crate::git_viewed::ViewedSet;
use crate::highlight::{self, Spans};
use crate::text_box::Input;

/// The group a changed file is listed in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Group {
    Staged,
    Unstaged,
    Untracked,
    /// Against-base mode's one list.
    Base,
    /// The files of the History section's selected commit.
    Commit,
    /// A task's or a turn's changes ([`Since`]).
    Since,
}

impl Group {
    pub fn title(self) -> &'static str {
        match self {
            Group::Staged => "Staged",
            Group::Unstaged => "Changes",
            Group::Untracked => "Untracked",
            Group::Base | Group::Since => "Changed files",
            Group::Commit => "Files",
        }
    }
}

/// A changed file in its group (the same path can be both staged and unstaged).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Target {
    pub group: Group,
    pub path: String,
    /// The commit whose change it is ([`Group::Commit`]), or the checkpoint (a commit or a
    /// tree) it is compared from ([`Group::Since`]).
    pub commit: Option<String>,
}

impl Target {
    /// The `git.diff` showing this file (of a task worktree, if `worktree_task_id`; of child
    /// repository `repo`).
    pub fn params(
        &self,
        project_id: Ulid,
        worktree_task_id: Option<Ulid>,
        repo: Option<String>,
    ) -> GitDiffParams {
        let commit = |group| self.commit.clone().filter(|_| self.group == group);
        GitDiffParams {
            project_id,
            worktree_task_id,
            repo,
            path: self.path.clone(),
            mode: if matches!(self.group, Group::Base | Group::Since) {
                GitMode::Base
            } else {
                GitMode::Working
            },
            staged: self.group == Group::Staged,
            commit: commit(Group::Commit),
            since: commit(Group::Since),
        }
    }
}

/// What the Git view lists: one of the two `git.status` modes, the History, or a task's or a
/// turn's changes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Section {
    Working,
    Base,
    History,
    /// A task's "View changes" or a turn's "Diff since" ([`GitView::since`]): from its
    /// checkpoint to the working tree.
    Since,
}

/// Whose changes a [`Since`] lists.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Origin {
    Task(Ulid),
    Turn { session_id: Ulid, n: u32 },
}

/// The changes since a checkpoint the Git view shows: a task's (SPEC §12B "Task changes") or
/// a turn's (§20C.3 "Since turn…").
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Since {
    pub origin: Origin,
    /// The header line saying what is compared.
    pub label: String,
    /// `task.checkpoint` (the HEAD before the task's run) or the turn's start tree.
    pub checkpoint: String,
}

impl Since {
    /// `Since abc1234 · task: Fix login`: the hash first, so a long title is cut, never what
    /// is compared.
    pub fn task(task_id: Ulid, title: &str, checkpoint: String) -> Since {
        Since {
            origin: Origin::Task(task_id),
            label: format!(
                "Since {} · task: {title}",
                crate::git_history::short_hash(&checkpoint)
            ),
            checkpoint,
        }
    }

    /// `Since turn 3 of claude #2`.
    pub fn turn(session: &Session, sessions: &[Session], n: u32, start_tree: String) -> Since {
        Since {
            origin: Origin::Turn {
                session_id: session.id,
                n,
            },
            label: format!("Since turn {n} of {}", session_label(session, sessions)),
            checkpoint: start_tree,
        }
    }
}

/// `session`'s name among `sessions`: its title or agent, numbered (`claude #2`, by start)
/// when another session of its workspace has the same name.
pub fn session_label(session: &Session, sessions: &[Session]) -> String {
    let name = crate::history::label(session);
    let mut namesakes: Vec<&Session> = sessions
        .iter()
        .filter(|s| s.project_id == session.project_id && crate::history::label(s) == name)
        .collect();
    if namesakes.len() < 2 {
        return name;
    }
    namesakes.sort_by_key(|s| (s.started_at, s.id));
    match namesakes.iter().position(|s| s.id == session.id) {
        Some(at) => format!("{name} #{}", at + 1),
        None => name,
    }
}

impl Section {
    fn of(mode: GitMode) -> Section {
        match mode {
            GitMode::Working => Section::Working,
            GitMode::Base => Section::Base,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Status {
    Loading,
    /// Boxed: far larger than the other variants (clippy `large_enum_variant`).
    Loaded(Box<GitStatus>),
    Failed(String),
}

/// One row of the changed-file list (a uniform list: every row has the same box).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ListRow {
    /// A group title with its file count (Working mode only).
    Header(Group, usize),
    File(Target, ChangedFile),
    /// A History commit; the selected one is expanded (its files follow it).
    Commit {
        commit: Commit,
        expanded: bool,
        /// Its row of the lane graph (SPEC §12B).
        graph: GraphRow,
    },
    /// Why the expanded commit lists no files (loading, failed, none changed).
    Note(String),
    /// A muted line under an empty Staged header (Working mode).
    Hint(&'static str),
}

/// One row of the diff viewer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiffRow {
    /// Unchanged lines between hunks that the diff does not show (collapsed).
    Gap(u32),
    /// The header of hunk number `.0`.
    Hunk(usize),
    Line(DiffLine),
}

/// One row of the Split diff viewer; indices point into [`Shown::rows`] (its `spans` and line
/// data are reused).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SplitRow {
    /// A gap or hunk-header row (`rows[.0]`), drawn across both halves.
    Span(usize),
    /// An unchanged line (`rows[.0]`): old number on the left, new number on the right.
    Same(usize),
    /// Removed line `old` on the left, added line `new` on the right. Both present = the line
    /// was modified; one `None` = the other half is empty padding.
    Change {
        old: Option<usize>,
        new: Option<usize>,
    },
}

/// The Split rows of `rows`. Inside a hunk a run of removed lines and the run of added lines
/// after it are paired in order and the shorter side is padded; unchanged lines are on both
/// sides; gap and hunk rows end a run and span both halves.
pub fn split_rows(rows: &[DiffRow]) -> Vec<SplitRow> {
    fn flush(out: &mut Vec<SplitRow>, removed: &mut Vec<usize>, added: &mut Vec<usize>) {
        for i in 0..removed.len().max(added.len()) {
            out.push(SplitRow::Change {
                old: removed.get(i).copied(),
                new: added.get(i).copied(),
            });
        }
        removed.clear();
        added.clear();
    }
    let mut out = Vec::with_capacity(rows.len());
    let (mut removed, mut added) = (Vec::new(), Vec::new());
    for (at, row) in rows.iter().enumerate() {
        match row {
            DiffRow::Line(line) if matches!(line.kind, DiffLineKind::Del) => {
                // A removed line after added ones starts the next change.
                if !added.is_empty() {
                    flush(&mut out, &mut removed, &mut added);
                }
                removed.push(at);
            }
            DiffRow::Line(line) if matches!(line.kind, DiffLineKind::Add) => added.push(at),
            DiffRow::Line(_) => {
                flush(&mut out, &mut removed, &mut added);
                out.push(SplitRow::Same(at));
            }
            DiffRow::Gap(_) | DiffRow::Hunk(_) => {
                flush(&mut out, &mut removed, &mut added);
                out.push(SplitRow::Span(at));
            }
        }
    }
    flush(&mut out, &mut removed, &mut added);
    out
}

/// A loaded diff and its rows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Shown {
    pub diff: FileDiff,
    pub rows: Vec<DiffRow>,
    /// The Split layout of `rows` ([`split_rows`]); never longer than `rows`.
    pub split: Vec<SplitRow>,
    /// Per row, the bytes of its (tab-expanded) text that differ from its partner when the line
    /// is half of a removed + added pair (see [`SplitRow::Change`], [`changed_ranges`]).
    pub words: Vec<Option<Range<usize>>>,
    /// Per row, the syntax spans of its line (tabs expanded; empty for other rows).
    pub spans: Vec<Spans>,
    /// Some line has syntax spans (its other text is then drawn as plain text).
    pub highlighted: bool,
    /// Digits of the largest line number (both gutter columns are this wide).
    pub number_width: usize,
}

impl Shown {
    /// Lays out and syntax-highlights `diff` (slow on big diffs: run it off the UI thread).
    pub fn new(diff: FileDiff) -> Shown {
        let rows = rows(&diff);
        let mut lines = highlight::diff_spans(&diff.path, &diff.hunks)
            .into_iter()
            .flatten();
        // `rows` lists every hunk line once, in order.
        let spans: Vec<Spans> = rows
            .iter()
            .map(|row| match row {
                DiffRow::Line(_) => lines.next().unwrap_or_default(),
                DiffRow::Gap(_) | DiffRow::Hunk(_) => Vec::new(),
            })
            .collect();
        let highlighted = spans.iter().any(|line| !line.is_empty());
        let largest = diff
            .hunks
            .iter()
            .flat_map(|hunk| &hunk.lines)
            .flat_map(|line| [line.old_line, line.new_line])
            .flatten()
            .max()
            .unwrap_or(1);
        let split = split_rows(&rows);
        // Rows of a removed + added pair: the changed token of each gets an inline highlight.
        let mut words = vec![None; rows.len()];
        for row in &split {
            if let SplitRow::Change {
                old: Some(old),
                new: Some(new),
            } = *row
                && let (DiffRow::Line(removed), DiffRow::Line(added)) = (&rows[old], &rows[new])
                && let Some((gone, came)) = changed_ranges(
                    &removed.text.replace('\t', highlight::TAB),
                    &added.text.replace('\t', highlight::TAB),
                )
            {
                words[old] = Some(gone).filter(|range| !range.is_empty());
                words[new] = Some(came).filter(|range| !range.is_empty());
            }
        }
        Shown {
            split,
            words,
            number_width: largest.to_string().len(),
            diff,
            rows,
            spans,
            highlighted,
        }
    }

    /// The number of rows the list draws in `mode`.
    pub fn list_len(&self, mode: DiffMode) -> usize {
        match mode {
            DiffMode::Unified => self.rows.len(),
            DiffMode::Split => self.split.len(),
        }
    }

    /// The list row of hunk `hunk`'s header in `mode` (what n / p scroll to).
    pub fn hunk_row(&self, hunk: usize, mode: DiffMode) -> Option<usize> {
        let at = self
            .rows
            .iter()
            .position(|row| *row == DiffRow::Hunk(hunk))?;
        match mode {
            DiffMode::Unified => Some(at),
            DiffMode::Split => self.split.iter().position(|row| *row == SplitRow::Span(at)),
        }
    }

    /// What a diff comment on new-side lines `selection` quotes: the shown new-side lines in
    /// it (added and unchanged ones, in order), trimmed; `None` when none is shown.
    pub fn quote(&self, selection: Selection) -> Option<String> {
        self.new_side_text(selection)
            .map(|text| text.trim().to_owned())
    }

    /// The shown new-side lines in `selection` as they are (indent kept), for "Send
    /// selection" (§20B.4); `None` when none is shown.
    pub fn new_side_text(&self, selection: Selection) -> Option<String> {
        let lines: Vec<&str> = self
            .rows
            .iter()
            .filter_map(|row| match row {
                DiffRow::Line(line) => line
                    .new_line
                    .filter(|&n| selection.contains(n, n))
                    .map(|_| line.text.as_str()),
                DiffRow::Gap(_) | DiffRow::Hunk(_) => None,
            })
            .collect();
        (!lines.is_empty()).then(|| lines.join("\n"))
    }
}

/// The selected file's diff.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Diff {
    /// No file selected.
    None,
    Loading,
    Shown(Rc<Shown>),
    Failed(String),
}

/// Git view keys (SPEC §13 "Git").
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    /// ↑ / k: previous file.
    Up,
    /// ↓ / j: next file.
    Down,
    /// n: next hunk.
    NextHunk,
    /// p: previous hunk.
    PreviousHunk,
    /// b: Working changes ↔ Against base (from History: back to the one it left).
    ToggleMode,
    /// h: History ↔ the changes it left.
    ToggleHistory,
    /// s / u: stage / unstage the selected file, or its current hunk (sent by the shell, see
    /// [`GitView::stage_key`]).
    Stage(Stage),
}

/// Staging moves a file (or hunk) from Unstaged / Untracked to Staged, unstaging back.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage {
    Stage,
    Unstage,
}

impl Stage {
    /// What a file of `group` can do; Against-base, commit and task files can do neither.
    pub fn of(group: Group) -> Option<Stage> {
        match group {
            Group::Unstaged | Group::Untracked => Some(Stage::Stage),
            Group::Staged => Some(Stage::Unstage),
            Group::Base | Group::Commit | Group::Since => None,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Stage::Stage => "Stage",
            Stage::Unstage => "Unstage",
        }
    }
}

/// A `git.stage` / `git.unstage` of a file or one hunk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StageRequest {
    pub stage: Stage,
    pub params: GitStageParams,
}

impl StageRequest {
    pub fn request(self) -> Request {
        match self.stage {
            Stage::Stage => Request::GitStage(self.params),
            Stage::Unstage => Request::GitUnstage(self.params),
        }
    }
}

/// The discard confirmation of one Unstaged / Untracked file; the dialog shows the file's diff
/// (the view's shown diff once the file is selected) as what is lost.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Discard {
    pub project_id: Ulid,
    pub worktree_task_id: Option<Ulid>,
    pub repo: Option<String>,
    pub target: Target,
    /// The same path also has staged changes (they stay).
    pub also_staged: bool,
    pub busy: bool,
    pub error: Option<String>,
}

impl Discard {
    /// What confirming does, in one sentence.
    pub fn sentence(&self) -> String {
        let path = &self.target.path;
        if self.target.group == Group::Untracked {
            return format!(
                "“{path}” is deleted. It was never committed, so it cannot be recovered."
            );
        }
        let staged = if self.also_staged {
            " Its staged changes stay."
        } else {
            ""
        };
        format!(
            "The unstaged changes to “{path}” below are discarded and cannot be recovered.{staged}"
        )
    }

    /// The `git.discard` to send (only ever after the user confirmed).
    pub fn params(&self) -> GitDiscardParams {
        GitDiscardParams {
            project_id: self.project_id,
            worktree_task_id: self.worktree_task_id,
            repo: self.repo.clone(),
            paths: vec![self.target.path.clone()],
            confirm: true,
        }
    }
}

/// The discard dialog shows at most this many diff rows.
pub const DISCARD_PREVIEW_ROWS: usize = 500;

/// What a change asks of the shell: a `git.status` of the shown mode, a `git.diff`, the
/// History's `git.log`, a commit's `git.show`.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Requests {
    pub status: bool,
    pub diff: Option<Target>,
    pub log: bool,
    pub show: Option<String>,
}

/// How far back "Changed by" looks for the sessions that changed files (SPEC §20C.4).
pub const CHANGED_BY_WINDOW_MS: i64 = 24 * 60 * 60 * 1000;

/// The "Repo" chips (ADR 0029): `(repo, label)` per repository of `git.repos`, `""` (the
/// workspace folder) labelled `workspace`; none with fewer than two (the row is hidden).
pub fn repo_choices<'a>(repos: &'a [String], workspace: &'a str) -> Vec<(Option<String>, &'a str)> {
    if repos.len() < 2 {
        return Vec::new();
    }
    repos
        .iter()
        .map(|repo| match repo.as_str() {
            "" => (None, workspace),
            name => (Some(name.to_owned()), name),
        })
        .collect()
}

/// The repository to show once `git.repos` answered: the shown one while it is listed, else
/// the workspace folder when it is one (listed first as `""`), else the first child one.
/// `None` = the workspace folder.
pub fn pick_repo(shown: Option<&str>, repos: &[String]) -> Option<String> {
    if let Some(shown) = shown
        && repos.iter().any(|repo| repo == shown)
    {
        return Some(shown.to_owned());
    }
    repos.first().filter(|first| !first.is_empty()).cloned()
}

/// The "Changed by" filter: `session.files` of the last 24 h, and the session whose files are
/// kept (`None` = All).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ChangedBy {
    pub files: SessionFiles,
    pub session: Option<Ulid>,
    /// The last `session.files` asked for (an older, late answer is dropped).
    asked: u64,
}

/// One "Changed by" choice.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Changer {
    pub session_id: Ulid,
    /// [`session_label`], or `ended session` (numbered when several) once it is no longer
    /// listed.
    pub label: String,
}

/// One workspace's Git view.
#[derive(Debug)]
pub struct GitView {
    pub project_id: Ulid,
    /// The task worktree shown instead of the project folder (SPEC §20E.1).
    pub worktree_task_id: Option<Ulid>,
    /// The child repository shown (ADR 0029); `None` = the workspace folder's. Set by
    /// `Shell::retarget_git` only: a repo change is a fresh view.
    pub repo: Option<String>,
    pub mode: GitMode,
    pub status: Status,
    pub selected: Option<Target>,
    pub diff: Diff,
    /// The current hunk of the shown diff (`n` / `p`).
    pub hunk: Option<usize>,
    /// Why the last "Open in Zed" failed.
    pub zed_error: Option<String>,
    /// Why the last refresh of the shown (kept) status failed.
    pub refresh_error: Option<String>,
    /// Why the last stage / unstage failed.
    pub stage_error: Option<String>,
    /// The commit message draft.
    pub commit: CommitBox,
    /// Fetch / Push.
    pub remotes: Remotes,
    /// The History section, while shown (`mode` is the changes it goes back to).
    pub history: Option<History>,
    /// The task's or turn's changes listed (`mode` is then [`GitMode::Base`]); kept while the
    /// History is shown, so leaving it goes back to them.
    pub since: Option<Since>,
    /// The "Changed by" filter (SPEC §20C.4).
    pub changed_by: ChangedBy,
    /// The diff comments of the selected file (anchored to new-side lines).
    pub comments: Comments,
    /// The side panel's "Filter files" text: keeps the files whose path matches it (fuzzy).
    pub filter: Input,
    /// Working groups folded in the list: their header stays (with its count), their files
    /// are not listed and ↑ ↓ skip them.
    pub collapsed: HashSet<Group>,
    /// The files marked as viewed (SPEC §12B, ADR 0065): the daemon's last answer, plus the
    /// user's toggles since. Shared with the list's render closure.
    pub viewed: Rc<ViewedSet>,
    /// The viewed file whose collapsed diff "Show diff" opened.
    unfolded: Option<Target>,
    /// Serial of the last `git.viewed` asked for or toggle made: an older answer is dropped.
    viewed_asked: u64,
    /// The diff `comments.selection` was picked on.
    commented: Option<Target>,
    /// A stage / unstage is in flight (another is not sent meanwhile).
    staging: bool,
    /// A status was ever asked for (the view loads when first shown).
    started: bool,
    /// The mode (and task checkpoint) of the `git.status` in flight, and whether another was
    /// asked for meanwhile.
    loading: Option<(GitMode, Option<String>)>,
    load_again: bool,
    /// A `git.diff` is in flight, and another was asked for meanwhile.
    reading: bool,
    read_again: bool,
    /// A History `git.log` is in flight (kept here: it outlives leaving the History), and
    /// another was asked for meanwhile.
    logging: bool,
    log_again: bool,
}

impl GitView {
    pub fn new(project_id: Ulid, worktree_task_id: Option<Ulid>) -> GitView {
        GitView {
            project_id,
            worktree_task_id,
            repo: None,
            mode: GitMode::Working,
            status: Status::Loading,
            selected: None,
            diff: Diff::None,
            hunk: None,
            zed_error: None,
            refresh_error: None,
            stage_error: None,
            commit: CommitBox::default(),
            remotes: Remotes::default(),
            history: None,
            since: None,
            changed_by: ChangedBy::default(),
            comments: Comments::new(CommentContext::Diff),
            filter: Input::default(),
            collapsed: HashSet::new(),
            viewed: Rc::new(ViewedSet::default()),
            unfolded: None,
            viewed_asked: 0,
            commented: None,
            staging: false,
            started: false,
            loading: None,
            load_again: false,
            reading: false,
            read_again: false,
            logging: false,
            log_again: false,
        }
    }

    /// What the view shows: the task worktree and the repository (late answers for another
    /// are dropped).
    pub fn at(&self) -> (Option<Ulid>, Option<String>) {
        (self.worktree_task_id, self.repo.clone())
    }

    /// A `git.status` is about to be sent: its params, or `None` when one is in flight already
    /// (it is sent again once that one answers).
    pub fn begin_status(&mut self) -> Option<GitStatusParams> {
        self.started = true;
        if self.loading.is_some() {
            self.load_again = true;
            return None;
        }
        let listed = self.listed();
        self.loading = Some(listed.clone());
        Some(GitStatusParams {
            project_id: self.project_id,
            worktree_task_id: self.worktree_task_id,
            repo: self.repo.clone(),
            mode: listed.0,
            since: listed.1,
        })
    }

    /// The `git.status` the shown changes come from: the mode and the checkpoint.
    fn listed(&self) -> (GitMode, Option<String>) {
        let since = self.since.as_ref().map(|since| since.checkpoint.clone());
        (self.mode, since)
    }

    /// The `git.status` answer. An answer for other changes than the shown ones (another mode
    /// or task) is dropped (and the status asked again). The selection stays on the same file
    /// if it is still listed (in its group, else in another), else moves to the first file; a
    /// moved selection asks for its diff. A failed refresh of a loaded status keeps it (e.g. git's index lock while
    /// an agent commits) and only notes the error in [`Self::refresh_error`].
    pub fn status_loaded(&mut self, loaded: Result<GitStatus, String>) -> Requests {
        let listed = self.loading.take();
        let again = std::mem::take(&mut self.load_again);
        if listed != Some(self.listed()) {
            return Requests {
                status: true,
                ..Requests::default()
            };
        }
        let status = match loaded {
            Ok(status) => status,
            Err(error) if matches!(self.status, Status::Loaded(_)) => {
                self.refresh_error = Some(error);
                return Requests {
                    status: again,
                    ..Requests::default()
                };
            }
            Err(error) => {
                self.status = Status::Failed(error);
                if self.history.is_none() {
                    self.select_none();
                }
                return Requests {
                    status: again,
                    ..Requests::default()
                };
            }
        };
        self.status = Status::Loaded(Box::new(status));
        // The History lists commits; the status only feeds the header there.
        if self.history.is_some() {
            return Requests {
                status: again,
                ..Requests::default()
            };
        }
        self.refresh_error = None;
        Requests {
            status: again,
            diff: self.reselect(),
            ..Requests::default()
        }
    }

    /// Keeps the selection on the same file if it is still listed (in its group, else in
    /// another), else moves it to the first file; returns a moved selection (its diff is read).
    fn reselect(&mut self) -> Option<Target> {
        let files = self.files();
        let kept = self.selected.as_ref().and_then(|selected| {
            files
                .iter()
                .find(|(target, _)| target == selected)
                .or_else(|| {
                    files
                        .iter()
                        .find(|(target, _)| target.path == selected.path)
                })
        });
        let next = kept
            .or_else(|| files.first())
            .map(|(target, _)| target.clone());
        match next {
            Some(next) if Some(&next) == self.selected.as_ref() => None,
            Some(next) => self.select(next),
            None => {
                self.select_none();
                None
            }
        }
    }

    /// A `session.files` asking which sessions changed files in the last 24 h before `now_ms`
    /// is about to be sent: its params and its serial (given back with its answer).
    pub fn begin_session_files(&mut self, now_ms: i64) -> (SessionFilesParams, u64) {
        self.changed_by.asked += 1;
        let params = SessionFilesParams {
            project_id: self.project_id,
            since_ms: now_ms - CHANGED_BY_WINDOW_MS,
        };
        (params, self.changed_by.asked)
    }

    /// The `session.files` answer to request `serial` (dropped when a newer one was asked
    /// for): a filtered session without changed files any more falls back to All; the listed
    /// files (and so the selection) may change.
    pub fn session_files_loaded(&mut self, serial: u64, files: SessionFiles) -> Requests {
        if serial != self.changed_by.asked {
            return Requests::default();
        }
        self.changed_by.files = files;
        let session = self.changed_by.session;
        let gone = session.is_some_and(|id| {
            self.changed_by
                .files
                .get(&id)
                .is_none_or(|paths| paths.is_empty())
        });
        if gone {
            self.changed_by.session = None;
        }
        self.filtered()
    }

    /// "Changed by": keeps only `session`'s files (`None` = All).
    pub fn filter_changed_by(&mut self, session: Option<Ulid>) -> Requests {
        if self.changed_by.session == session {
            return Requests::default();
        }
        self.changed_by.session = session;
        self.filtered()
    }

    /// The listed files changed with the filter: the selection follows.
    fn filtered(&mut self) -> Requests {
        if self.history.is_some() || !matches!(self.status, Status::Loaded(_)) {
            return Requests::default();
        }
        Requests {
            diff: self.reselect(),
            ..Requests::default()
        }
    }

    /// The "Changed by" choices: the sessions with changed files, oldest first (none in the
    /// History, where the filter does not apply).
    pub fn changers(&self, sessions: &[Session]) -> Vec<Changer> {
        if self.history.is_some() {
            return vec![];
        }
        let mut changers: Vec<(i64, Changer)> = self
            .changed_by
            .files
            .iter()
            .filter(|(_, paths)| !paths.is_empty())
            .map(|(&session_id, _)| {
                let session = sessions.iter().find(|s| s.id == session_id);
                let label = session.map_or_else(
                    || "ended session".to_owned(),
                    |session| session_label(session, sessions),
                );
                let started = session.map_or(i64::MAX, |session| session.started_at);
                (started, Changer { session_id, label })
            })
            .collect();
        changers.sort_by_key(|(started, changer)| (*started, changer.session_id));
        // Several ended sessions (listed last, by id) are told apart by number.
        let ended = changers.iter().filter(|(started, _)| *started == i64::MAX);
        if ended.count() > 1 {
            let unlisted = changers
                .iter_mut()
                .filter(|(started, _)| *started == i64::MAX);
            for (at, (_, changer)) in unlisted.enumerate() {
                changer.label = format!("ended session #{}", at + 1);
            }
        }
        changers.into_iter().map(|(_, changer)| changer).collect()
    }

    /// The label of the filtered session, while filtering (its [`Changer`] label).
    pub fn changed_by_label(&self, sessions: &[Session]) -> Option<String> {
        let id = self.changed_by.session?;
        let changed = self
            .changed_by
            .files
            .get(&id)
            .is_some_and(|paths| !paths.is_empty());
        if self.history.is_some() || !changed {
            return None;
        }
        match sessions.iter().find(|s| s.id == id) {
            Some(session) => Some(session_label(session, sessions)),
            // An ended session's number depends on the other ended ones.
            None => self
                .changers(sessions)
                .into_iter()
                .find(|changer| changer.session_id == id)
                .map(|changer| changer.label),
        }
    }

    fn select_none(&mut self) {
        self.selected = None;
        self.diff = Diff::None;
        self.hunk = None;
    }

    /// The listed section.
    pub fn section(&self) -> Section {
        if self.history.is_some() {
            Section::History
        } else {
            self.changes()
        }
    }

    /// The changes section the History goes back to.
    fn changes(&self) -> Section {
        if self.since.is_some() {
            Section::Since
        } else {
            Section::of(self.mode)
        }
    }

    /// Lists `section` (nothing if it is listed already): the status of a changes mode, or
    /// the History's log. [`Section::Since`] goes back to the kept changes (else Against
    /// base); [`Self::show_since`] picks them.
    pub fn show(&mut self, section: Section) -> Requests {
        if section == self.section() {
            return Requests::default();
        }
        self.select_none();
        if section == Section::History {
            self.history = Some(History::default());
            return Requests {
                log: true,
                ..Requests::default()
            };
        }
        let (mode, since) = match section {
            Section::Working | Section::History => (GitMode::Working, None),
            Section::Base => (GitMode::Base, None),
            Section::Since => (GitMode::Base, self.since.clone()),
        };
        self.list(mode, since)
    }

    /// Lists the changes since a checkpoint (a Review card's "View changes", a turn's "Diff
    /// since").
    pub fn show_since(&mut self, since: Since) -> Requests {
        if self.section() == Section::Since && self.since.as_ref() == Some(&since) {
            return Requests::default();
        }
        self.select_none();
        self.list(GitMode::Base, Some(since))
    }

    /// Lists the changes of `mode` (and `since`, base mode only) instead of the History.
    fn list(&mut self, mode: GitMode, since: Option<Since>) -> Requests {
        let listed = self.listed();
        self.history = None;
        self.mode = mode;
        self.since = since;
        // Back to the same changes: the kept status lists at once; its refresh selects a file.
        if listed != self.listed() {
            self.status = Status::Loading;
            // Another list has its own marks; the answer for it is asked after its status.
            self.viewed = Rc::new(ViewedSet::default());
            self.viewed_asked += 1;
        }
        self.refresh_error = None;
        Requests {
            status: true,
            ..Requests::default()
        }
    }

    /// The changed files of the shown section with their diff targets, in list order (Staged,
    /// Unstaged, Untracked; in the History, the selected commit's), kept by "Changed by" and
    /// by the "Filter files" text.
    pub fn files(&self) -> Vec<(Target, ChangedFile)> {
        if let Some(history) = &self.history {
            return history
                .files()
                .iter()
                .filter(|file| self.matches(&file.path))
                .map(|file| {
                    let target = Target {
                        group: Group::Commit,
                        path: file.path.clone(),
                        commit: history.selected.clone(),
                    };
                    (target, file.clone())
                })
                .collect();
        }
        self.status_files(true)
    }

    /// The changed files of the loaded status in list order. `narrowed`: only those that pass
    /// "Changed by" and the "Filter files" text (what the list shows); else every file of the
    /// section (what `Viewed N/M` counts).
    fn status_files(&self, narrowed: bool) -> Vec<(Target, ChangedFile)> {
        let Status::Loaded(status) = &self.status else {
            return vec![];
        };
        let checkpoint = self.since.as_ref().map(|since| since.checkpoint.clone());
        let kept = self.kept();
        let group = |group: Group, files: &[ChangedFile]| {
            files
                .iter()
                .filter(|file| !narrowed || kept.is_none_or(|kept| kept.contains(&file.path)))
                .filter(|file| !narrowed || self.matches(&file.path))
                .map(|file| {
                    let target = Target {
                        group,
                        path: file.path.clone(),
                        commit: checkpoint.clone().filter(|_| group == Group::Since),
                    };
                    (target, file.clone())
                })
                .collect::<Vec<_>>()
        };
        match (self.mode, &self.since) {
            (GitMode::Working, _) => [
                group(Group::Staged, &status.staged),
                group(Group::Unstaged, &status.unstaged),
                group(Group::Untracked, &status.untracked),
            ]
            .concat(),
            (GitMode::Base, None) => group(Group::Base, &status.against_base),
            (GitMode::Base, Some(_)) => group(Group::Since, &status.against_base),
        }
    }

    /// "Changed by": only the filtered session's files (`None` = All).
    fn kept(&self) -> Option<&[String]> {
        self.changed_by.session.map(|id| {
            self.changed_by
                .files
                .get(&id)
                .map_or(&[][..], Vec::as_slice)
        })
    }

    /// Whether `path` passes the "Filter files" text (an empty filter keeps every path).
    fn matches(&self, path: &str) -> bool {
        self.filter.is_empty() || crate::palette::fuzzy(&self.filter, path).is_some()
    }

    /// Whether the commit box is on screen: Working changes of a loaded status, with changes
    /// listed or a draft typed. (Asked every frame: no [`Self::files`] copy.)
    pub fn commit_box_shown(&self) -> bool {
        if self.section() != Section::Working {
            return false;
        }
        let Status::Loaded(status) = &self.status else {
            return false;
        };
        let kept = self.kept();
        let any_file = [&status.staged, &status.unstaged, &status.untracked]
            .into_iter()
            .flatten()
            .any(|file| kept.is_none_or(|kept| kept.contains(&file.path)));
        any_file || !self.commit.text.text().is_empty()
    }

    /// The changed-file list: in Working mode each non-empty group under its title; in the
    /// History the commits, the selected one followed by its files (or why there are none).
    pub fn list_rows(&self) -> Vec<ListRow> {
        let mut files = self.files();
        if let Some(history) = &self.history {
            let mut rows = Vec::new();
            for (commit, graph) in history.commits().iter().zip(&history.graph) {
                let expanded = history.selected.as_ref() == Some(&commit.hash);
                rows.push(ListRow::Commit {
                    commit: commit.clone(),
                    expanded,
                    graph: graph.clone(),
                });
                if !expanded {
                    continue;
                }
                match &history.detail {
                    Detail::Loaded(listed) if listed.is_empty() => {
                        rows.push(ListRow::Note("No files changed.".into()));
                    }
                    // Only the expanded commit lists its files: they move in once.
                    Detail::Loaded(_) => rows.extend(
                        files
                            .drain(..)
                            .map(|(target, file)| ListRow::File(target, file)),
                    ),
                    Detail::None | Detail::Loading => rows.push(ListRow::Note("Loading…".into())),
                    // The diff panel shows the error and Retry.
                    Detail::Failed(_) => {
                        rows.push(ListRow::Note("✕ Cannot list its files.".into()));
                    }
                }
            }
            return rows;
        }
        if self.mode == GitMode::Base {
            return files
                .into_iter()
                .map(|(target, file)| ListRow::File(target, file))
                .collect();
        }
        if files.is_empty() {
            return Vec::new();
        }
        // `files` lists the groups one after another (Staged, Unstaged, Untracked). Staged is
        // always listed, so where staged changes go stays visible.
        let mut counts: Vec<(Group, usize)> = files
            .chunk_by(|(a, _), (b, _)| a.group == b.group)
            .map(|members| (members[0].0.group, members.len()))
            .collect();
        if counts
            .first()
            .is_none_or(|(group, _)| *group != Group::Staged)
        {
            counts.insert(0, (Group::Staged, 0));
        }
        let mut rows = Vec::with_capacity(files.len() + counts.len() + 1);
        let mut files = files.into_iter();
        for (group, count) in counts {
            rows.push(ListRow::Header(group, count));
            let listed = files.by_ref().take(count);
            if self.collapsed.contains(&group) {
                listed.for_each(drop);
                continue;
            }
            if count == 0 {
                rows.push(ListRow::Hint("Nothing staged. Stage a file or a hunk."));
            }
            rows.extend(listed.map(|(target, file)| ListRow::File(target, file)));
        }
        rows
    }

    /// Width of the History's graph column in lanes (0 outside the History).
    pub fn graph_lanes(&self) -> usize {
        self.history
            .as_ref()
            .map_or(0, |history| git_graph::lanes(&history.graph))
    }

    /// Expands History commit `hash`; returns it when its files have to be read.
    pub fn select_commit(&mut self, hash: String) -> Option<String> {
        let show = self.history.as_mut()?.select(hash);
        if show.is_some() {
            self.select_none();
        }
        show
    }

    /// The History's `git.log` is about to be sent: its params, or `None` when one is in
    /// flight already (or the History is not shown).
    pub fn begin_log(&mut self) -> Option<GitLogParams> {
        self.history.as_ref()?;
        if self.logging {
            self.log_again = true;
            return None;
        }
        self.logging = true;
        Some(GitLogParams {
            project_id: self.project_id,
            worktree_task_id: self.worktree_task_id,
            repo: self.repo.clone(),
            limit: GitLogParams::DEFAULT_LIMIT,
            path: None,
        })
    }

    /// The `git.log` answer (dropped if the History was left meanwhile). In the History the
    /// log owns [`Self::refresh_error`] (a status refresh does not clear it there).
    pub fn logged(&mut self, loaded: Result<Vec<Commit>, String>) -> Requests {
        self.logging = false;
        let again = std::mem::take(&mut self.log_again);
        let Some(history) = &mut self.history else {
            return Requests::default();
        };
        let logged = history.logged(loaded);
        if logged.show.is_some() || history.selected.is_none() {
            self.select_none();
        }
        self.refresh_error = logged.error;
        Requests {
            log: again,
            show: logged.show,
            ..Requests::default()
        }
    }

    /// The `git.show` of commit `hash`: if it is the expanded commit, its first file's diff
    /// is shown (unless a file of it is selected already).
    pub fn commit_shown(&mut self, hash: &str, loaded: Result<CommitDetail, String>) -> Requests {
        let Some(history) = &mut self.history else {
            return Requests::default();
        };
        if !history.shown(hash, loaded) || self.selected.is_some() {
            return Requests::default();
        }
        let first = self.files().into_iter().next();
        Requests {
            diff: first.and_then(|(target, _)| self.select(target)),
            ..Requests::default()
        }
    }

    /// The `git.show` of `hash`.
    pub fn show_params(&self, hash: String) -> GitShowParams {
        GitShowParams {
            project_id: self.project_id,
            worktree_task_id: self.worktree_task_id,
            repo: self.repo.clone(),
            commit: hash,
        }
    }

    /// Shows `target`'s diff. Returns it when it has to be read: another file, or the same
    /// one whose diff failed (a retry).
    pub fn select(&mut self, target: Target) -> Option<Target> {
        if self.selected.as_ref() == Some(&target) && !matches!(self.diff, Diff::Failed(_)) {
            return None;
        }
        self.selected = Some(target.clone());
        self.diff = Diff::Loading;
        self.hunk = None;
        self.unfolded = None;
        self.zed_error = None;
        self.stage_error = None;
        Some(target)
    }

    /// A `git.diff` is about to be sent; `false` = one is in flight already (the selected
    /// file's diff is read again once that one answers).
    pub fn begin_diff(&mut self) -> bool {
        if self.reading {
            self.read_again = true;
            return false;
        }
        self.reading = true;
        true
    }

    /// The `git.diff` answer for `target` (laid out by [`Shown::new`]); shown if `target` is
    /// still selected (the current hunk stays if it still exists). Returns the file to read
    /// again (asked for meanwhile).
    pub fn diff_loaded(
        &mut self,
        target: &Target,
        loaded: Result<Shown, String>,
    ) -> Option<Target> {
        self.reading = false;
        let again = std::mem::take(&mut self.read_again);
        if self.selected.as_ref() == Some(target) {
            self.diff = match loaded {
                Ok(shown) => {
                    if self.hunk.is_some_and(|hunk| hunk >= shown.diff.hunks.len()) {
                        self.hunk = None;
                    }
                    Diff::Shown(Rc::new(shown))
                }
                Err(error) => Diff::Failed(error),
            };
        }
        if again { self.selected.clone() } else { None }
    }

    /// Switches Working changes ↔ Against base (from a task's changes: Working; from the
    /// History: back to the changes it left): the list and diff load again.
    pub fn toggle_mode(&mut self) -> Requests {
        self.show(match self.section() {
            Section::Working => Section::Base,
            Section::Base | Section::Since => Section::Working,
            Section::History => self.changes(),
        })
    }

    /// Switches to the History, or back to the changes it left.
    pub fn toggle_history(&mut self) -> Requests {
        self.show(match self.section() {
            Section::History => self.changes(),
            Section::Working | Section::Base | Section::Since => Section::History,
        })
    }

    /// Moves the file selection or the current hunk, or switches the section.
    pub fn key(&mut self, key: Key) -> Requests {
        match key {
            Key::Up => self.step_file(-1),
            Key::Down => self.step_file(1),
            Key::NextHunk => {
                self.step_hunk(true);
                Requests::default()
            }
            Key::PreviousHunk => {
                self.step_hunk(false);
                Requests::default()
            }
            Key::ToggleMode => self.toggle_mode(),
            Key::ToggleHistory => self.toggle_history(),
            Key::Stage(_) => Requests::default(),
        }
    }

    /// The files ↑ ↓ move between: the listed ones outside folded groups.
    fn steppable(&self) -> Vec<(Target, ChangedFile)> {
        let mut files = self.files();
        files.retain(|(target, _)| !self.collapsed.contains(&target.group));
        files
    }

    /// The selected file's place among the files ↑ ↓ move between (1-based), their count and
    /// the file's change: the diff toolbar's status badge and "1 / 6".
    pub fn file_position(&self) -> Option<(usize, usize, ChangeKind)> {
        let selected = self.selected.as_ref()?;
        let files = self.steppable();
        let at = files.iter().position(|(target, _)| target == selected)?;
        Some((at + 1, files.len(), files[at].1.kind))
    }

    /// Folds or unfolds a Working group of the list.
    pub fn toggle_collapsed(&mut self, group: Group) {
        if !self.collapsed.remove(&group) {
            self.collapsed.insert(group);
        }
    }

    /// "Stage all" / "Unstage all" on a group header: one `git.stage` / `git.unstage` of every
    /// listed file of `group` (a rename also takes its old path, like [`Self::file_change`]).
    pub fn group_change(&self, group: Group) -> Option<StageRequest> {
        let stage = Stage::of(group)?;
        let paths: Vec<String> = self
            .files()
            .into_iter()
            .filter(|(target, _)| target.group == group)
            .flat_map(|(_, file)| [Some(file.path), file.old_path])
            .flatten()
            .collect();
        if paths.is_empty() {
            return None;
        }
        Some(StageRequest {
            stage,
            params: GitStageParams {
                project_id: self.project_id,
                worktree_task_id: self.worktree_task_id,
                repo: self.repo.clone(),
                paths,
                hunk: None,
            },
        })
    }

    /// The next / previous file; in the History, past the expanded commit's first / last file
    /// the previous / next commit (its first file once listed).
    fn step_file(&mut self, by: isize) -> Requests {
        let files = self.steppable();
        let at = self
            .selected
            .as_ref()
            .and_then(|selected| files.iter().position(|(target, _)| target == selected));
        let past_end = match at {
            Some(at) => at.checked_add_signed(by).is_none_or(|to| to >= files.len()),
            None => files.is_empty(),
        };
        if past_end && self.history.is_some() {
            let show = self.history.as_mut().and_then(|history| history.step(by));
            if show.is_some() {
                self.select_none();
            }
            return Requests {
                show,
                ..Requests::default()
            };
        }
        if files.is_empty() {
            return Requests::default();
        }
        let to = match at {
            Some(at) => at.saturating_add_signed(by).min(files.len() - 1),
            None => 0,
        };
        let target = files[to].0.clone();
        Requests {
            diff: self.select(target),
            ..Requests::default()
        }
    }

    /// The next / previous hunk of the shown diff (from none: the first / last); stops at
    /// the ends.
    fn step_hunk(&mut self, forward: bool) {
        let Diff::Shown(shown) = &self.diff else {
            return;
        };
        if self.diff_collapsed() {
            return;
        }
        let count = shown.diff.hunks.len();
        if count == 0 {
            return;
        }
        self.hunk = Some(match (self.hunk, forward) {
            (None, true) => 0,
            (None, false) => count - 1,
            (Some(hunk), true) => (hunk + 1).min(count - 1),
            (Some(hunk), false) => hunk.saturating_sub(1),
        });
    }

    /// Follows `git.changed` of this workspace (sent for every file-watcher batch).
    pub fn apply(&mut self, event: &Event) -> Requests {
        let Event::GitChanged(changed) = event else {
            return Requests::default();
        };
        // A task worktree's changes (SPEC §20E.1) are only that worktree's.
        if changed.project_id != self.project_id
            || changed.worktree_task_id != self.worktree_task_id
            || !self.started
        {
            return Requests::default();
        }
        self.refresh()
    }

    /// The status and the selected file's diff again (shown values stay until they answer);
    /// in the History the status (for the header) and the log (a commit's diff never changes).
    /// Also used when the view is shown again, since hidden views do not follow `git.changed`.
    pub fn refresh(&mut self) -> Requests {
        if self.history.is_some() {
            return Requests {
                status: true,
                log: true,
                ..Requests::default()
            };
        }
        Requests {
            status: true,
            diff: self.selected.clone(),
            ..Requests::default()
        }
    }

    /// The listed file `target` stands for (the same lookup as [`Self::files`], without
    /// copying the list: the render asks it every frame).
    pub fn file(&self, target: &Target) -> Option<&ChangedFile> {
        let (files, commit, kept) = if let Some(history) = &self.history {
            if target.group != Group::Commit {
                return None;
            }
            (history.files(), history.selected.as_ref(), None)
        } else {
            let Status::Loaded(status) = &self.status else {
                return None;
            };
            let files = match (self.mode, &self.since, target.group) {
                (GitMode::Working, _, Group::Staged) => &status.staged,
                (GitMode::Working, _, Group::Unstaged) => &status.unstaged,
                (GitMode::Working, _, Group::Untracked) => &status.untracked,
                (GitMode::Base, None, Group::Base) | (GitMode::Base, Some(_), Group::Since) => {
                    &status.against_base
                }
                _ => return None,
            };
            let checkpoint = self
                .since
                .as_ref()
                .map(|since| &since.checkpoint)
                .filter(|_| target.group == Group::Since);
            (files.as_slice(), checkpoint, self.kept())
        };
        if target.commit.as_ref() != commit {
            return None;
        }
        files
            .iter()
            .filter(|file| kept.is_none_or(|kept| kept.contains(&file.path)))
            .find(|file| file.path == target.path)
    }

    /// Stages / unstages the whole listed file `target` (new, deleted and untracked files can
    /// only go by path). A rename also takes its old path (else its deletion stays staged).
    pub fn file_change(&self, target: &Target) -> Option<StageRequest> {
        let stage = Stage::of(target.group)?;
        let file = self.file(target)?;
        Some(StageRequest {
            stage,
            params: GitStageParams {
                project_id: self.project_id,
                worktree_task_id: self.worktree_task_id,
                repo: self.repo.clone(),
                paths: [Some(file.path.clone()), file.old_path.clone()]
                    .into_iter()
                    .flatten()
                    .collect(),
                hunk: None,
            },
        })
    }

    /// Stages / unstages hunk `hunk` of the shown diff. Only modified files go by hunk: the
    /// daemon's patch has no file-mode headers (new / deleted / renamed files go by path).
    pub fn hunk_change(&self, hunk: usize) -> Option<StageRequest> {
        let selected = self.selected.as_ref()?;
        let stage = Stage::of(selected.group)?;
        if self.file(selected)?.kind != ChangeKind::Modified {
            return None;
        }
        let Diff::Shown(shown) = &self.diff else {
            return None;
        };
        let hunk = shown.diff.hunks.get(hunk)?.clone();
        Some(StageRequest {
            stage,
            params: GitStageParams {
                project_id: self.project_id,
                worktree_task_id: self.worktree_task_id,
                repo: self.repo.clone(),
                paths: vec![],
                hunk: Some(HunkTarget {
                    path: selected.path.clone(),
                    hunk,
                }),
            },
        })
    }

    /// `s` / `u`: the current hunk if there is one (and it can go by hunk), else the selected
    /// file; nothing if the selected file cannot go that way or a change is in flight.
    pub fn stage_key(&self, stage: Stage) -> Option<StageRequest> {
        let selected = self.selected.as_ref()?;
        if self.staging || Stage::of(selected.group) != Some(stage) {
            return None;
        }
        // A collapsed diff hides its hunks: `s` / `u` take the whole file then.
        self.hunk
            .filter(|_| !self.diff_collapsed())
            .and_then(|hunk| self.hunk_change(hunk))
            .or_else(|| self.file_change(selected))
    }

    /// A stage / unstage is about to be sent; `false` = one is in flight already.
    pub fn begin_stage(&mut self) -> bool {
        if self.staging {
            return false;
        }
        self.staging = true;
        self.stage_error = None;
        true
    }

    /// The stage / unstage answered: on success the status and diff are read again.
    pub fn staged(&mut self, result: Result<(), String>) -> Requests {
        self.staging = false;
        match result {
            Ok(()) => self.refresh(),
            Err(error) => {
                self.stage_error = Some(error);
                Requests::default()
            }
        }
    }

    /// Whether the shown list can ask which files are viewed: loaded changes, not the
    /// History (a commit's files are never marked).
    pub fn wants_viewed(&self) -> bool {
        self.history.is_none() && matches!(self.status, Status::Loaded(_))
    }

    /// A `git.viewed` is about to be sent: the params of the shown `git.status`, and its serial
    /// (given back with the answer).
    pub fn begin_viewed(&mut self) -> (GitStatusParams, u64) {
        self.viewed_asked += 1;
        let (mode, since) = self.listed();
        let params = GitStatusParams {
            project_id: self.project_id,
            worktree_task_id: self.worktree_task_id,
            repo: self.repo.clone(),
            mode,
            since,
        };
        (params, self.viewed_asked)
    }

    /// The `git.viewed` answer: kept unless a newer ask or a toggle came meanwhile, or other
    /// changes (`asked` = mode and checkpoint) are listed now. A failed ask keeps the last
    /// set (the shell never calls this for it).
    pub fn viewed_loaded(
        &mut self,
        serial: u64,
        asked: (GitMode, Option<String>),
        files: Vec<ViewedFile>,
    ) {
        if serial != self.viewed_asked || asked != self.listed() {
            return;
        }
        self.viewed = Rc::new(ViewedSet::new(files));
    }

    /// Marks `target` viewed, or clears the mark: shown at once; returns the `git.set_viewed`
    /// to send. `None` for a History file.
    pub fn toggle_viewed(&mut self, target: &Target) -> Option<GitSetViewedParams> {
        if target.group == Group::Commit || self.history.is_some() {
            return None;
        }
        let viewed = !self.viewed.contains(target.group, &target.path);
        Rc::make_mut(&mut self.viewed).set(target.group, &target.path, viewed);
        // An answer to an older `git.viewed` no longer holds.
        self.viewed_asked += 1;
        self.unfolded = None;
        // The hunk belongs to the diff that is about to collapse.
        if viewed && self.selected.as_ref() == Some(target) {
            self.hunk = None;
        }
        Some(GitSetViewedParams {
            diff: target.params(self.project_id, self.worktree_task_id, self.repo.clone()),
            viewed,
        })
    }

    /// The selected file is viewed and its diff is shown collapsed ("Show diff" opens it).
    pub fn diff_collapsed(&self) -> bool {
        self.selected.as_ref().is_some_and(|selected| {
            self.viewed.contains(selected.group, &selected.path)
                && self.unfolded.as_ref() != Some(selected)
        })
    }

    /// "Show diff": opens the selected viewed file's diff (until another file is selected).
    pub fn unfold(&mut self) {
        self.unfolded = self.selected.clone();
    }

    /// `(viewed, listed)` for the header's `Viewed N/M`: every file of the shown list, whatever
    /// "Changed by" and the file filter hide; `None` in the History or with nothing listed.
    pub fn viewed_counter(&self) -> Option<(usize, usize)> {
        if !self.wants_viewed() {
            return None;
        }
        let (viewed, listed) = self.viewed.count(&self.status_files(false));
        (listed > 0).then_some((viewed, listed))
    }

    /// The discard confirmation of the listed Unstaged / Untracked file `target`.
    pub fn discard(&self, target: &Target) -> Option<Discard> {
        if !matches!(target.group, Group::Unstaged | Group::Untracked) {
            return None;
        }
        self.file(target)?;
        let also_staged = self.file(&Target {
            group: Group::Staged,
            path: target.path.clone(),
            commit: None,
        });
        Some(Discard {
            project_id: self.project_id,
            worktree_task_id: self.worktree_task_id,
            repo: self.repo.clone(),
            target: target.clone(),
            also_staged: also_staged.is_some(),
            busy: false,
            error: None,
        })
    }

    /// The shown diff of `target`, if it is the selected file and its diff is loaded (what a
    /// discard of it loses).
    pub fn shown_diff(&self, target: &Target) -> Option<Rc<Shown>> {
        match &self.diff {
            Diff::Shown(shown) if self.selected.as_ref() == Some(target) => Some(Rc::clone(shown)),
            _ => None,
        }
    }

    /// The selected file's diff comments start over when it is another file: returns its path
    /// when `comment.list` has to be sent. Picked lines go with another diff of the same file
    /// too (staged vs unstaged numbers differ). The History keeps the comments it left.
    pub fn begin_comments(&mut self) -> Option<String> {
        if !self.can_comment() {
            return None;
        }
        let target = self.selected.clone()?;
        if self.commented.as_ref() != Some(&target) {
            self.comments.selection = None;
        }
        let path = target.path.clone();
        self.commented = Some(target);
        self.comments.begin_list(&path)
    }

    /// Diff lines can be commented on (and threads show) outside the History: a past
    /// commit's line numbers are not the current file's.
    pub fn can_comment(&self) -> bool {
        self.section() != Section::History
    }

    /// Opens a draft for a diff comment on the picked new-side lines, quoting them from the
    /// shown diff; `false` = nothing picked, no diff shown, or in the History.
    pub fn start_comment(&mut self) -> bool {
        if !self.can_comment() {
            return false;
        }
        let quoted = match (&self.diff, self.comments.selection) {
            (Diff::Shown(shown), Some(selection)) => shown.quote(selection),
            (Diff::Shown(_), None) => None,
            _ => return false,
        };
        self.comments.start_quoted(quoted)
    }

    /// The comment margin shows beside the diff of a selected file while it has something to
    /// show: a draft, threads, or why they cannot be listed (a diff keeps its full width
    /// otherwise; picked lines get the inline selection bar).
    pub fn comments_shown(&self) -> bool {
        let listed = match &self.comments.list {
            comments::List::Loaded(list) => !list.is_empty(),
            comments::List::Failed(_) => true,
            comments::List::Loading => false,
        };
        self.selected.is_some() && self.can_comment() && (listed || self.comments.draft.is_some())
    }

    /// The session "Send to agent" offers first: in a turn's changes, its session; in a task's,
    /// the task's newest live session, else its newest one (the dialog falls back to the
    /// newest live session then).
    pub fn since_session(&self, sessions: &[Session]) -> Option<Ulid> {
        if self.section() != Section::Since {
            return None;
        }
        let task_id = match self.since.as_ref()?.origin {
            Origin::Task(task_id) => task_id,
            Origin::Turn { session_id, .. } => return Some(session_id),
        };
        sessions
            .iter()
            .filter(|session| session.task_id == Some(task_id))
            .max_by_key(|session| (session.state != SessionState::Exited, session.started_at))
            .map(|session| session.id)
    }
}

/// The graph column of row `index` of [`GitView::list_rows`]: a commit's own row; for the
/// files or the note listed under a commit, its lanes running on. `None` outside the History.
pub fn graph_cell(rows: &[ListRow], index: usize) -> Option<GraphCell> {
    match rows.get(index)? {
        ListRow::Commit { graph, .. } => Some(GraphCell::Commit(graph.clone())),
        ListRow::File(..) | ListRow::Note(_) => {
            rows[..index].iter().rev().find_map(|row| match row {
                ListRow::Commit { graph, .. } => Some(GraphCell::Through(graph.below())),
                ListRow::Header(..) => Some(GraphCell::Through(Vec::new())),
                _ => None,
            })
        }
        ListRow::Header(..) | ListRow::Hint(_) => None,
    }
    .filter(|cell| !matches!(cell, GraphCell::Through(lanes) if lanes.is_empty()))
}

/// The diff viewer rows of `diff`: each hunk's header then its lines, with a gap row for the
/// unchanged lines between two hunks (and before the first one; none after the last, as the
/// file's length is not known).
pub fn rows(diff: &FileDiff) -> Vec<DiffRow> {
    let mut rows = Vec::new();
    // Old-side lines before / through a hunk (`-N,0` names the line it comes after).
    let before = |old_start: u32, old_lines: u32| {
        if old_lines == 0 {
            old_start
        } else {
            old_start.saturating_sub(1)
        }
    };
    let mut through = 0;
    for (index, hunk) in diff.hunks.iter().enumerate() {
        let start = before(hunk.old_start, hunk.old_lines);
        let gap = start.saturating_sub(through);
        if gap > 0 {
            rows.push(DiffRow::Gap(gap));
        }
        through = start + hunk.old_lines;
        rows.push(DiffRow::Hunk(index));
        rows.extend(hunk.lines.iter().cloned().map(DiffRow::Line));
    }
    rows
}

/// The bytes that differ between a removed line and the added line paired with it: what is
/// left of each after their common prefix and suffix (on char boundaries). `None` when they
/// share neither, so the whole line would light up. A range is empty when that side only lost
/// (or gained) text.
pub fn changed_ranges(old: &str, new: &str) -> Option<(Range<usize>, Range<usize>)> {
    let common = |a: &str, b: &str, from_end: bool| -> usize {
        let same = |(a, b): &(char, char)| a == b;
        let len = |(a, _): (char, char)| a.len_utf8();
        if from_end {
            a.chars()
                .rev()
                .zip(b.chars().rev())
                .take_while(same)
                .map(len)
                .sum()
        } else {
            a.chars().zip(b.chars()).take_while(same).map(len).sum()
        }
    };
    let prefix = common(old, new, false);
    let suffix = common(&old[prefix..], &new[prefix..], true);
    (prefix + suffix > 0).then(|| (prefix..old.len() - suffix, prefix..new.len() - suffix))
}

/// "+3 −1" style counts of a changed file; `None` = binary or not counted.
pub fn counts(file: &ChangedFile) -> Option<(u32, u32)> {
    Some((file.added?, file.removed?))
}

/// The branch card's upstream line: "origin/main · 2 commits ahead", "… · up to date",
/// "… · 2 ahead, 1 behind", or "No upstream".
pub fn tracking(status: &GitStatus) -> String {
    let Some(upstream) = &status.upstream else {
        return "No upstream".to_owned();
    };
    let commits = |n: u32| if n == 1 { "commit" } else { "commits" };
    let state = match (status.ahead, status.behind) {
        (0, 0) => "up to date".to_owned(),
        (ahead, 0) => format!("{ahead} {} ahead", commits(ahead)),
        (0, behind) => format!("{behind} {} behind", commits(behind)),
        (ahead, behind) => format!("{ahead} ahead, {behind} behind"),
    };
    format!("{upstream} · {state}")
}

/// The file-moving key `keystroke` stands for (unmodified arrows, j / k). The other Git keys
/// are keymap commands (`keymap::GIT_CONTEXT`).
pub fn key(keystroke: &Keystroke) -> Option<Key> {
    if keystroke.modifiers.modified() {
        return None;
    }
    match keystroke.key.as_str() {
        "up" | "k" => Some(Key::Up),
        "down" | "j" => Some(Key::Down),
        _ => None,
    }
}

#[cfg(test)]
mod tests;
