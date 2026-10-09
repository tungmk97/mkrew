//! Git view-model (SPEC §12B): the `git.status` of one workspace in the shown mode (Working
//! changes grouped Staged / Unstaged / Untracked, or Against base), the selected changed file
//! and its `git.diff` laid out as rows (hunk headers, lines, and "unchanged lines" gaps between
//! hunks), and the current hunk moved by `n` / `p`. `git.changed` re-asks the status and the
//! shown diff. Stage / unstage a file or hunk (`s` / `u`) and the discard confirmation; the
//! commit box and Fetch / Push live in [`crate::git_actions`]. The History section (`h`)
//! lists commits instead ([`crate::git_history`]); their files' diffs show here too.
//! Pure and tested headlessly; `main.rs` does the requests and renders it.

use std::rc::Rc;

use gpui::Keystroke;
use mkrew_protocol::{
    ChangeKind, ChangedFile, CommentContext, Commit, CommitDetail, DiffLine, Event, FileDiff,
    GitDiffParams, GitDiscardParams, GitLogParams, GitMode, GitShowParams, GitStageParams,
    GitStatus, GitStatusParams, HunkTarget, Request, Session, SessionFiles, SessionFilesParams,
    SessionState,
};
use ulid::Ulid;

use crate::comments::{self, Comments, Selection};
use crate::git_actions::{CommitBox, Remotes};
use crate::git_history::{Detail, History};
use crate::highlight::{self, Spans};

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
            Group::Unstaged => "Unstaged",
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
    /// The `git.diff` showing this file (of a task worktree, if `worktree_task_id`).
    pub fn params(&self, project_id: Ulid, worktree_task_id: Option<Ulid>) -> GitDiffParams {
        let commit = |group| self.commit.clone().filter(|_| self.group == group);
        GitDiffParams {
            project_id,
            worktree_task_id,
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
    },
    /// Why the expanded commit lists no files (loading, failed, none changed).
    Note(String),
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

/// A loaded diff and its rows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Shown {
    pub diff: FileDiff,
    pub rows: Vec<DiffRow>,
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
        Shown {
            number_width: largest.to_string().len(),
            diff,
            rows,
            spans,
            highlighted,
        }
    }

    /// The row index of hunk `hunk`'s header.
    pub fn hunk_row(&self, hunk: usize) -> Option<usize> {
        self.rows.iter().position(|row| *row == DiffRow::Hunk(hunk))
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
        }
        self.refresh_error = None;
        Requests {
            status: true,
            ..Requests::default()
        }
    }

    /// The changed files of the shown section with their diff targets, in list order (Staged,
    /// Unstaged, Untracked; in the History, the selected commit's), kept by "Changed by".
    pub fn files(&self) -> Vec<(Target, ChangedFile)> {
        if let Some(history) = &self.history {
            return history
                .files()
                .iter()
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
        let Status::Loaded(status) = &self.status else {
            return vec![];
        };
        let checkpoint = self.since.as_ref().map(|since| since.checkpoint.clone());
        let kept = self.kept();
        let group = |group: Group, files: &[ChangedFile]| {
            files
                .iter()
                .filter(|file| kept.is_none_or(|kept| kept.contains(&file.path)))
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
            for commit in history.commits() {
                let expanded = history.selected.as_ref() == Some(&commit.hash);
                rows.push(ListRow::Commit {
                    commit: commit.clone(),
                    expanded,
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
        // `files` lists the groups one after another (Staged, Unstaged, Untracked).
        let counts: Vec<(Group, usize)> = files
            .chunk_by(|(a, _), (b, _)| a.group == b.group)
            .map(|members| (members[0].0.group, members.len()))
            .collect();
        let mut rows = Vec::with_capacity(files.len() + counts.len());
        let mut files = files.into_iter();
        for (group, count) in counts {
            rows.push(ListRow::Header(group, count));
            rows.extend(
                files
                    .by_ref()
                    .take(count)
                    .map(|(target, file)| ListRow::File(target, file)),
            );
        }
        rows
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

    /// The next / previous file; in the History, past the expanded commit's first / last file
    /// the previous / next commit (its first file once listed).
    fn step_file(&mut self, by: isize) -> Requests {
        let files = self.files();
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
        self.hunk
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
    /// show: picked lines, a draft, threads, or why they cannot be listed (a diff keeps its
    /// full width otherwise).
    pub fn comments_shown(&self) -> bool {
        let listed = match &self.comments.list {
            comments::List::Loaded(list) => !list.is_empty(),
            comments::List::Failed(_) => true,
            comments::List::Loading => false,
        };
        self.selected.is_some()
            && self.can_comment()
            && (listed || self.comments.selection.is_some() || self.comments.draft.is_some())
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

/// "+3 −1" style counts of a changed file; `None` = binary or not counted.
pub fn counts(file: &ChangedFile) -> Option<(u32, u32)> {
    Some((file.added?, file.removed?))
}

/// "↑2 ↓1" for a branch ahead / behind its upstream; `None` when in sync.
pub fn ahead_behind(status: &GitStatus) -> Option<String> {
    let mut parts = Vec::new();
    if status.ahead > 0 {
        parts.push(format!("↑{}", status.ahead));
    }
    if status.behind > 0 {
        parts.push(format!("↓{}", status.behind));
    }
    (!parts.is_empty()).then(|| parts.join(" "))
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
mod tests {
    use super::*;
    use crate::highlight::Token;
    use mkrew_protocol::{ChangeKind, DiffLineKind, GitChangedEvent, Hunk};

    fn file(path: &str, kind: ChangeKind) -> ChangedFile {
        ChangedFile {
            path: path.into(),
            old_path: None,
            kind,
            added: Some(1),
            removed: Some(0),
        }
    }

    fn working() -> GitStatus {
        GitStatus {
            branch: Some("main".into()),
            staged: vec![file("a.rs", ChangeKind::Modified)],
            unstaged: vec![
                file("a.rs", ChangeKind::Modified),
                file("b.rs", ChangeKind::Deleted),
            ],
            untracked: vec![file("new.txt", ChangeKind::Untracked)],
            ..GitStatus::default()
        }
    }

    fn target(group: Group, path: &str) -> Target {
        Target {
            group,
            path: path.into(),
            commit: None,
        }
    }

    fn line(kind: DiffLineKind, old: Option<u32>, new: Option<u32>) -> DiffLine {
        DiffLine {
            kind,
            text: "x".into(),
            old_line: old,
            new_line: new,
            no_newline: false,
            crlf: false,
        }
    }

    fn hunk(old_start: u32, old_lines: u32, new_start: u32, new_lines: u32) -> Hunk {
        Hunk {
            old_start,
            old_lines,
            new_start,
            new_lines,
            header: String::new(),
            lines: vec![line(
                DiffLineKind::Context,
                Some(old_start),
                Some(new_start),
            )],
        }
    }

    fn diff(hunks: Vec<Hunk>) -> FileDiff {
        FileDiff {
            path: "a.rs".into(),
            hunks,
            ..FileDiff::default()
        }
    }

    /// [`diff`] laid out, as `diff_loaded` gets it.
    fn laid_out(hunks: Vec<Hunk>) -> Shown {
        Shown::new(diff(hunks))
    }

    /// A Working view with [`working`] loaded and its first file selected.
    fn loaded() -> GitView {
        let mut view = GitView::new(Ulid::new(), None);
        assert!(view.begin_status().is_some());
        let requests = view.status_loaded(Ok(working()));
        assert_eq!(requests.diff, Some(target(Group::Staged, "a.rs")));
        view
    }

    #[test]
    fn files_are_grouped_and_the_first_is_selected() {
        let view = loaded();
        let rows = view.list_rows();
        let labels: Vec<String> = rows
            .iter()
            .map(|row| match row {
                ListRow::Header(group, count) => format!("{} {count}", group.title()),
                ListRow::File(_, file) => file.path.clone(),
                ListRow::Commit { .. } | ListRow::Note(_) => unreachable!("not in History"),
            })
            .collect();
        assert_eq!(
            labels,
            [
                "Staged 1",
                "a.rs",
                "Unstaged 2",
                "a.rs",
                "b.rs",
                "Untracked 1",
                "new.txt"
            ]
        );
        assert_eq!(view.diff, Diff::Loading);
    }

    #[test]
    fn diff_params_follow_the_group() {
        let project = Ulid::new();
        let staged = target(Group::Staged, "a").params(project, None);
        assert!(staged.staged);
        assert_eq!(staged.mode, GitMode::Working);
        assert!(!target(Group::Untracked, "a").params(project, None).staged);
        assert_eq!(
            target(Group::Base, "a").params(project, None).mode,
            GitMode::Base
        );
        assert_eq!(target(Group::Base, "a").params(project, None).since, None);
    }

    #[test]
    fn a_status_asked_while_one_is_in_flight_runs_after_it() {
        let mut view = GitView::new(Ulid::new(), None);
        assert!(view.begin_status().is_some());
        assert!(view.begin_status().is_none());
        assert!(view.status_loaded(Ok(GitStatus::default())).status);
        assert!(view.begin_status().is_some());
        let requests = view.status_loaded(Ok(GitStatus::default()));
        assert_eq!(requests, Requests::default());
        assert_eq!(view.selected, None);
    }

    #[test]
    fn the_selection_follows_its_file_across_groups_or_falls_back_to_the_first() {
        let mut view = loaded();
        view.select(target(Group::Untracked, "new.txt"));
        // `new.txt` got staged: the selection moves with it and its diff is read.
        let mut status = working();
        status.untracked.clear();
        status.staged.push(file("new.txt", ChangeKind::Added));
        view.begin_status();
        let requests = view.status_loaded(Ok(status.clone()));
        assert_eq!(requests.diff, Some(target(Group::Staged, "new.txt")));
        // Unchanged: nothing to read.
        view.begin_status();
        assert_eq!(view.status_loaded(Ok(status)), Requests::default());
        // Gone: the first file.
        view.begin_status();
        let mut status = working();
        status.untracked.clear();
        let requests = view.status_loaded(Ok(status));
        assert_eq!(requests.diff, Some(target(Group::Staged, "a.rs")));
        // Nothing changed any more: nothing selected.
        view.begin_status();
        view.status_loaded(Ok(GitStatus::default()));
        assert_eq!(view.selected, None);
        assert_eq!(view.diff, Diff::None);
    }

    #[test]
    fn a_failed_status_says_so_but_a_failed_refresh_keeps_the_list() {
        let mut view = loaded();
        view.begin_status();
        view.begin_status();
        // A refresh fails (asked again meanwhile): the list stays, the error is noted and the
        // status is asked again.
        assert!(view.status_loaded(Err("index.lock".into())).status);
        assert!(matches!(view.status, Status::Loaded(_)));
        assert_eq!(view.refresh_error.as_deref(), Some("index.lock"));
        assert!(view.selected.is_some());
        view.begin_status();
        view.status_loaded(Ok(working()));
        assert_eq!(view.refresh_error, None);
        // Nothing loaded yet: the failure is shown.
        view.toggle_mode();
        view.begin_status();
        assert_eq!(
            view.status_loaded(Err("no base".into())),
            Requests::default()
        );
        assert_eq!(view.status, Status::Failed("no base".into()));
        assert_eq!(view.selected, None);
    }

    #[test]
    fn toggling_the_mode_reloads_and_drops_answers_for_the_old_mode() {
        let mut view = loaded();
        view.begin_status();
        let requests = view.key(Key::ToggleMode);
        assert_eq!(view.mode, GitMode::Base);
        assert!(requests.status);
        assert_eq!(view.status, Status::Loading);
        assert_eq!(view.selected, None);
        // The Working answer in flight is dropped and the status asked again…
        assert!(view.begin_status().is_none());
        assert!(view.status_loaded(Ok(working())).status);
        assert_eq!(view.status, Status::Loading);
        // …and the Base answer lists `against_base` without group titles.
        let params = view.begin_status().unwrap();
        assert_eq!(params.mode, GitMode::Base);
        let status = GitStatus {
            base_ref: Some("main".into()),
            against_base: vec![file("x.rs", ChangeKind::Added)],
            ..GitStatus::default()
        };
        let requests = view.status_loaded(Ok(status));
        assert_eq!(requests.diff, Some(target(Group::Base, "x.rs")));
        assert_eq!(
            view.list_rows(),
            [ListRow::File(
                target(Group::Base, "x.rs"),
                file("x.rs", ChangeKind::Added)
            )]
        );
    }

    fn task_changes(checkpoint: &str) -> Since {
        Since::task(Ulid::new(), "Fix login", checkpoint.into())
    }

    fn since(files: &[&str]) -> GitStatus {
        GitStatus {
            merge_base: Some("abc".into()),
            against_base: files
                .iter()
                .map(|path| file(path, ChangeKind::Modified))
                .collect(),
            ..GitStatus::default()
        }
    }

    #[test]
    fn a_tasks_changes_list_and_diff_since_its_checkpoint() {
        let mut view = loaded();
        let task = task_changes("abc");
        let requests = view.show_since(task.clone());
        assert!(requests.status);
        assert_eq!(view.section(), Section::Since);
        assert_eq!(
            (view.status.clone(), view.selected.clone()),
            (Status::Loading, None)
        );
        assert_eq!(view.show_since(task.clone()), Requests::default());
        let params = view.begin_status().unwrap();
        assert_eq!(
            (params.mode, params.since.as_deref()),
            (GitMode::Base, Some("abc"))
        );
        assert!(!view.commit_box_shown());

        let requests = view.status_loaded(Ok(since(&["a.rs"])));
        let shown = Target {
            group: Group::Since,
            path: "a.rs".into(),
            commit: Some("abc".into()),
        };
        assert_eq!(requests.diff, Some(shown.clone()));
        let diff = shown.params(view.project_id, None);
        assert_eq!(diff.mode, GitMode::Base);
        assert_eq!((diff.commit, diff.since.as_deref()), (None, Some("abc")));
        assert_eq!(Stage::of(Group::Since), None);
        assert!(view.discard(&shown).is_none());

        // The History goes back to the task's kept list; the mode key leaves the task.
        view.toggle_history();
        assert_eq!(view.section(), Section::History);
        assert!(view.since.is_some());
        let requests = view.toggle_history();
        assert_eq!((view.section(), requests.status), (Section::Since, true));
        assert!(matches!(view.status, Status::Loaded(_)));
        view.toggle_mode();
        assert_eq!(
            (view.section(), view.since.clone()),
            (Section::Working, None)
        );
        assert_eq!(view.status, Status::Loading);
    }

    #[test]
    fn answers_for_another_task_or_against_base_are_dropped() {
        let mut view = GitView::new(Ulid::new(), None);
        view.show(Section::Base);
        view.begin_status();
        // Against base was in flight: dropped, the task's status asked again.
        view.show_since(task_changes("abc"));
        assert!(view.status_loaded(Ok(since(&["base.rs"]))).status);
        assert_eq!(view.status, Status::Loading);
        view.begin_status();
        // Another task meanwhile: its own checkpoint.
        view.show_since(task_changes("def"));
        assert_eq!(view.status, Status::Loading);
        assert!(view.status_loaded(Ok(since(&["old.rs"]))).status);
        assert_eq!(view.begin_status().unwrap().since.as_deref(), Some("def"));
        view.status_loaded(Ok(since(&["new.rs"])));
        assert_eq!(view.files()[0].0.commit.as_deref(), Some("def"));
        // Against base from a task: no checkpoint any more.
        view.show(Section::Base);
        assert_eq!(view.begin_status().unwrap().since, None);
    }

    #[test]
    fn the_commit_box_shows_with_working_changes_or_a_draft() {
        let mut view = GitView::new(Ulid::new(), None);
        assert!(!view.commit_box_shown(), "still loading");
        let mut view_loaded = loaded();
        assert!(view_loaded.commit_box_shown());
        view_loaded.key(Key::ToggleMode);
        assert!(!view_loaded.commit_box_shown(), "against base");

        view.begin_status();
        view.status_loaded(Ok(GitStatus::default()));
        assert!(!view.commit_box_shown(), "nothing changed, nothing typed");
        view.commit.insert("WIP");
        assert!(view.commit_box_shown(), "a draft stays reachable");
    }

    #[test]
    fn diffs_are_read_once_coalesced_and_dropped_when_stale() {
        let mut view = loaded();
        let a = target(Group::Staged, "a.rs");
        assert!(view.begin_diff());
        assert_eq!(view.select(a.clone()), None, "already selected");
        // Another file is selected while the read is in flight: read once that answers.
        let b = target(Group::Unstaged, "b.rs");
        assert_eq!(view.select(b.clone()), Some(b.clone()));
        assert!(!view.begin_diff());
        assert_eq!(view.diff_loaded(&a, Ok(laid_out(vec![]))), Some(b.clone()));
        assert_eq!(view.diff, Diff::Loading, "a's answer is not shown");
        assert!(view.begin_diff());
        assert_eq!(view.diff_loaded(&b, Err("gone".into())), None);
        assert_eq!(view.diff, Diff::Failed("gone".into()));
        // Selecting a failed diff again retries.
        assert_eq!(view.select(b.clone()), Some(b));
    }

    #[test]
    fn keys_move_files_and_hunks() {
        let mut view = loaded();
        assert_eq!(
            view.key(Key::Down).diff,
            Some(target(Group::Unstaged, "a.rs"))
        );
        view.key(Key::Down);
        view.key(Key::Down);
        assert_eq!(view.key(Key::Down).diff, None, "stays on the last file");
        assert_eq!(view.selected, Some(target(Group::Untracked, "new.txt")));
        assert_eq!(
            view.key(Key::Up).diff,
            Some(target(Group::Unstaged, "b.rs"))
        );

        // Hunks: none shown yet → nothing; then first / last and clamped.
        view.key(Key::NextHunk);
        assert_eq!(view.hunk, None);
        let shown = target(Group::Unstaged, "b.rs");
        view.begin_diff();
        view.diff_loaded(
            &shown,
            Ok(laid_out(vec![hunk(1, 1, 1, 1), hunk(20, 1, 20, 1)])),
        );
        view.key(Key::PreviousHunk);
        assert_eq!(view.hunk, Some(1));
        view.key(Key::NextHunk);
        assert_eq!(view.hunk, Some(1));
        view.key(Key::PreviousHunk);
        view.key(Key::PreviousHunk);
        assert_eq!(view.hunk, Some(0));
        // A refreshed diff with fewer hunks drops a hunk that no longer exists.
        view.key(Key::NextHunk);
        view.begin_diff();
        view.diff_loaded(&shown, Ok(laid_out(vec![hunk(1, 1, 1, 1)])));
        assert_eq!(view.hunk, None);
        // Another file clears it.
        view.key(Key::NextHunk);
        view.key(Key::Up);
        assert_eq!(view.hunk, None);
    }

    #[test]
    fn git_changed_refreshes_the_status_and_the_shown_diff() {
        let mut view = GitView::new(Ulid::new(), None);
        let changed = |project_id| {
            Event::GitChanged(GitChangedEvent {
                project_id,
                worktree_task_id: None,
            })
        };
        assert_eq!(
            view.apply(&changed(view.project_id)),
            Requests::default(),
            "never shown"
        );
        view.begin_status();
        view.status_loaded(Ok(working()));
        assert_eq!(view.apply(&changed(Ulid::new())), Requests::default());
        let in_worktree = Event::GitChanged(GitChangedEvent {
            project_id: view.project_id,
            worktree_task_id: Some(Ulid::new()),
        });
        assert_eq!(view.apply(&in_worktree), Requests::default());
        assert_eq!(
            view.apply(&changed(view.project_id)),
            Requests {
                status: true,
                diff: Some(target(Group::Staged, "a.rs")),
                ..Requests::default()
            }
        );
    }

    #[test]
    fn a_worktree_view_asks_its_worktree_and_follows_only_its_changes() {
        let task_id = Some(Ulid::new());
        let mut view = GitView::new(Ulid::new(), task_id);
        assert_eq!(view.begin_status().unwrap().worktree_task_id, task_id);
        view.status_loaded(Ok(working()));
        let project_id = view.project_id;
        let changed = |worktree_task_id| {
            Event::GitChanged(GitChangedEvent {
                project_id,
                worktree_task_id,
            })
        };
        assert_eq!(view.apply(&changed(None)), Requests::default());
        assert_eq!(view.apply(&changed(Some(Ulid::new()))), Requests::default());
        assert!(view.apply(&changed(task_id)).status);
        let staged = target(Group::Staged, "a.rs");
        let untracked = target(Group::Untracked, "new.txt");
        assert_eq!(staged.params(project_id, task_id).worktree_task_id, task_id);
        let change = view.file_change(&staged).unwrap();
        assert_eq!(change.params.worktree_task_id, task_id);
        let discard = view.discard(&untracked).unwrap();
        assert_eq!(discard.params().worktree_task_id, task_id);
        assert_eq!(view.show_params("abc".into()).worktree_task_id, task_id);
    }

    #[test]
    fn rows_collapse_unchanged_lines_between_hunks() {
        let shown = Shown::new(diff(vec![
            hunk(1, 3, 1, 4),
            hunk(10, 2, 11, 2),
            // Pure insertion after old line 20.
            hunk(20, 0, 21, 1),
        ]));
        let kinds: Vec<String> = shown
            .rows
            .iter()
            .map(|row| match row {
                DiffRow::Gap(n) => format!("gap {n}"),
                DiffRow::Hunk(i) => format!("hunk {i}"),
                DiffRow::Line(_) => "line".into(),
            })
            .collect();
        assert_eq!(
            kinds,
            [
                "hunk 0", "line", "gap 6", "hunk 1", "line", "gap 9", "hunk 2", "line"
            ]
        );
        assert_eq!(shown.hunk_row(1), Some(3));
        assert_eq!(shown.hunk_row(3), None);
        assert_eq!(shown.number_width, 2, "line 21 is the largest");
        // A first hunk further down starts with a gap.
        assert_eq!(rows(&diff(vec![hunk(5, 2, 5, 2)]))[0], DiffRow::Gap(4));
    }

    #[test]
    fn line_rows_carry_their_syntax_spans() {
        let mut first = hunk(1, 1, 1, 1);
        first.lines[0].text = "// one".into();
        let mut second = hunk(10, 1, 10, 1);
        second.lines[0].text = "fn two() {}".into();
        let shown = Shown::new(diff(vec![first, second]));
        assert_eq!(shown.spans.len(), shown.rows.len());
        assert!(shown.highlighted);
        assert!(
            !Shown::new(diff(vec![hunk(1, 1, 1, 1)])).highlighted,
            "\"x\" has no tokens"
        );
        // hunk 0, line, gap, hunk 1, line.
        assert!(shown.spans[0].is_empty() && shown.spans[2].is_empty());
        assert_eq!(shown.spans[1], [(0..6, Token::Comment)]);
        assert!(
            shown.spans[4].contains(&(0..2, Token::Keyword)),
            "{:?}",
            shown.spans[4]
        );
    }

    fn paths(request: &StageRequest) -> (Stage, Vec<String>, Option<u32>) {
        let hunk = request.params.hunk.as_ref().map(|target| {
            assert_eq!(target.path, "a.rs");
            target.hunk.old_start
        });
        (request.stage, request.params.paths.clone(), hunk)
    }

    #[test]
    fn files_stage_and_unstage_by_path_from_their_group() {
        let view = loaded();
        let staged = view.file_change(&target(Group::Staged, "a.rs")).unwrap();
        assert_eq!(paths(&staged), (Stage::Unstage, vec!["a.rs".into()], None));
        assert!(matches!(staged.request(), Request::GitUnstage(_)));
        let untracked = view
            .file_change(&target(Group::Untracked, "new.txt"))
            .unwrap();
        assert_eq!(untracked.stage, Stage::Stage);
        assert!(matches!(untracked.request(), Request::GitStage(_)));
        assert_eq!(view.file_change(&target(Group::Unstaged, "gone.rs")), None);
        assert_eq!(view.file_change(&target(Group::Base, "a.rs")), None);

        // A staged rename unstages both paths.
        let mut view = GitView::new(Ulid::new(), None);
        view.begin_status();
        let mut status = working();
        status.staged = vec![ChangedFile {
            old_path: Some("old.rs".into()),
            ..file("new.rs", ChangeKind::Renamed)
        }];
        view.status_loaded(Ok(status));
        let renamed = view.file_change(&target(Group::Staged, "new.rs")).unwrap();
        assert_eq!(renamed.params.paths, ["new.rs", "old.rs"]);
    }

    #[test]
    fn keys_stage_the_current_hunk_of_a_modified_file_else_the_file() {
        let mut view = loaded();
        let a = target(Group::Staged, "a.rs");
        // The selection is staged: `s` does nothing, `u` unstages the file.
        assert_eq!(view.stage_key(Stage::Stage), None);
        let file = view.stage_key(Stage::Unstage).unwrap();
        assert_eq!(paths(&file), (Stage::Unstage, vec!["a.rs".into()], None));
        // With a current hunk, only that hunk.
        view.begin_diff();
        view.diff_loaded(&a, Ok(laid_out(vec![hunk(1, 1, 1, 1), hunk(20, 1, 20, 1)])));
        view.key(Key::PreviousHunk);
        let current = view.stage_key(Stage::Unstage).unwrap();
        assert_eq!(paths(&current), (Stage::Unstage, vec![], Some(20)));
        assert_eq!(view.hunk_change(5), None);
        // A deleted file goes by path even with a current hunk.
        let b = target(Group::Unstaged, "b.rs");
        view.select(b.clone());
        view.begin_diff();
        view.diff_loaded(&b, Ok(laid_out(vec![hunk(1, 1, 0, 0)])));
        view.key(Key::NextHunk);
        assert_eq!(view.hunk_change(0), None);
        let file = view.stage_key(Stage::Stage).unwrap();
        assert_eq!(paths(&file), (Stage::Stage, vec!["b.rs".into()], None));
    }

    #[test]
    fn one_stage_at_a_time_and_failures_are_shown() {
        let mut view = loaded();
        assert!(view.begin_stage());
        assert!(!view.begin_stage());
        assert_eq!(view.stage_key(Stage::Unstage), None, "in flight");
        assert_eq!(
            view.staged(Err("patch does not apply".into())),
            Requests::default()
        );
        assert_eq!(view.stage_error.as_deref(), Some("patch does not apply"));
        assert!(view.begin_stage());
        assert_eq!(view.stage_error, None);
        assert_eq!(
            view.staged(Ok(())),
            Requests {
                status: true,
                diff: Some(target(Group::Staged, "a.rs")),
                ..Requests::default()
            }
        );
    }

    #[test]
    fn only_unstaged_and_untracked_files_can_be_discarded() {
        let mut view = loaded();
        assert_eq!(view.discard(&target(Group::Staged, "a.rs")), None);
        assert_eq!(view.discard(&target(Group::Unstaged, "gone.rs")), None);
        let a = view.discard(&target(Group::Unstaged, "a.rs")).unwrap();
        assert!(a.also_staged);
        assert!(a.sentence().contains("Its staged changes stay."));
        let params = a.params();
        assert!(params.confirm);
        assert_eq!(params.paths, ["a.rs"]);
        let b = view.discard(&target(Group::Unstaged, "b.rs")).unwrap();
        assert!(!b.also_staged);
        let new = view.discard(&target(Group::Untracked, "new.txt")).unwrap();
        assert!(new.sentence().contains("is deleted"));
        // What is lost: the shown diff, once the file is selected and its diff loaded.
        let target_b = target(Group::Unstaged, "b.rs");
        assert_eq!(view.shown_diff(&target_b), None);
        view.select(target_b.clone());
        view.begin_diff();
        view.diff_loaded(&target_b, Ok(laid_out(vec![hunk(1, 1, 0, 0)])));
        assert!(view.shown_diff(&target_b).is_some());
        assert_eq!(view.shown_diff(&target(Group::Staged, "a.rs")), None);
    }

    #[test]
    fn labels() {
        let mut status = GitStatus::default();
        assert_eq!(ahead_behind(&status), None);
        status.ahead = 2;
        status.behind = 1;
        assert_eq!(ahead_behind(&status).as_deref(), Some("↑2 ↓1"));
        let mut binary = file("x", ChangeKind::Modified);
        assert_eq!(counts(&binary), Some((1, 0)));
        binary.added = None;
        assert_eq!(counts(&binary), None);
    }

    fn commit(hash: &str) -> Commit {
        Commit {
            hash: hash.into(),
            parents: vec![],
            author_name: "Ann".into(),
            author_email: "ann@example.com".into(),
            authored_at: 0,
            summary: format!("commit {hash}"),
            body: String::new(),
        }
    }

    fn detail(hash: &str, paths: &[&str]) -> CommitDetail {
        CommitDetail {
            commit: commit(hash),
            files: paths
                .iter()
                .map(|path| file(path, ChangeKind::Modified))
                .collect(),
        }
    }

    fn commit_target(hash: &str, path: &str) -> Target {
        Target {
            group: Group::Commit,
            path: path.into(),
            commit: Some(hash.into()),
        }
    }

    /// [`loaded`] switched to the History, with commits `b` (x, y) and `a` (z) listed and
    /// `b`'s first file selected.
    fn history() -> GitView {
        let mut view = loaded();
        let requests = view.key(Key::ToggleHistory);
        assert!(requests.log);
        assert_eq!(view.section(), Section::History);
        assert_eq!(view.selected, None);
        let params = view.begin_log().unwrap();
        assert_eq!(params.limit, 50);
        let requests = view.logged(Ok(vec![commit("b"), commit("a")]));
        assert_eq!(requests.show.as_deref(), Some("b"));
        let requests = view.commit_shown("b", Ok(detail("b", &["x", "y"])));
        assert_eq!(requests.diff, Some(commit_target("b", "x")));
        view
    }

    fn history_labels(view: &GitView) -> Vec<String> {
        view.list_rows()
            .iter()
            .map(|row| match row {
                ListRow::Commit { commit, expanded } => format!("{} {expanded}", commit.hash),
                ListRow::File(target, _) => format!("  {}", target.path),
                ListRow::Note(note) => format!("  {note}"),
                ListRow::Header(group, _) => group.title().to_owned(),
            })
            .collect()
    }

    #[test]
    fn the_history_lists_commits_with_the_selected_ones_files() {
        let mut view = history();
        assert_eq!(history_labels(&view), ["b true", "  x", "  y", "a false"]);
        assert!(!view.commit_box_shown());
        // The commit's diff asks for that commit's file; nothing stages it.
        let params = commit_target("b", "x").params(view.project_id, None);
        assert_eq!(params.commit.as_deref(), Some("b"));
        assert_eq!(view.stage_key(Stage::Stage), None);
        assert_eq!(view.discard(&commit_target("b", "x")), None);
        // Another commit: loading, then its first file.
        assert_eq!(view.select_commit("a".into()).as_deref(), Some("a"));
        assert_eq!(view.selected, None);
        assert_eq!(history_labels(&view), ["b false", "a true", "  Loading…"]);
        // A late answer for `b` is dropped.
        assert_eq!(
            view.commit_shown("b", Ok(detail("b", &["x"]))),
            Requests::default()
        );
        let requests = view.commit_shown("a", Ok(detail("a", &[])));
        assert_eq!(requests, Requests::default());
        assert_eq!(
            history_labels(&view),
            ["b false", "a true", "  No files changed."]
        );
    }

    #[test]
    fn moving_past_a_commits_files_moves_to_the_next_commit() {
        let mut view = history();
        assert_eq!(view.key(Key::Up), Requests::default(), "already first");
        assert_eq!(view.key(Key::Down).diff, Some(commit_target("b", "y")));
        let requests = view.key(Key::Down);
        assert_eq!(requests.show.as_deref(), Some("a"));
        assert_eq!(view.selected, None);
        view.commit_shown("a", Ok(detail("a", &["z"])));
        assert_eq!(view.key(Key::Down), Requests::default(), "last commit");
        assert_eq!(view.key(Key::Up).show.as_deref(), Some("b"));
    }

    #[test]
    fn the_history_refreshes_its_log_and_goes_back_to_the_changes_it_left() {
        let mut view = history();
        assert_eq!(
            view.apply(&Event::GitChanged(GitChangedEvent {
                project_id: view.project_id,
                worktree_task_id: None,
            })),
            Requests {
                status: true,
                log: true,
                ..Requests::default()
            }
        );
        // The status only feeds the header: the selection stays.
        view.begin_status();
        assert_eq!(view.status_loaded(Ok(working())), Requests::default());
        assert_eq!(view.selected, Some(commit_target("b", "x")));
        // A new commit on top: the selection stays.
        view.begin_log();
        let requests = view.logged(Ok(vec![commit("c"), commit("b"), commit("a")]));
        assert_eq!(requests, Requests::default());
        assert_eq!(view.selected, Some(commit_target("b", "x")));
        // `b` back to Working changes: the kept status lists at once, its refresh selects.
        let requests = view.key(Key::ToggleMode);
        assert_eq!(view.section(), Section::Working);
        assert!(requests.status);
        assert!(matches!(view.status, Status::Loaded(_)));
        view.begin_status();
        let requests = view.status_loaded(Ok(working()));
        assert_eq!(requests.diff, Some(target(Group::Staged, "a.rs")));
        // From Against base, History and back return to Against base.
        view.show(Section::Base);
        view.toggle_history();
        assert_eq!(view.begin_log().map(|params| params.path), Some(None));
        view.toggle_history();
        assert_eq!(view.section(), Section::Base);
        // A late log answer after leaving is dropped.
        assert_eq!(view.logged(Ok(vec![commit("a")])), Requests::default());
        // Left and re-entered while a log is in flight: no second log until it answers, then
        // one more (the answer may predate re-entering).
        view.toggle_history();
        assert!(view.begin_log().is_some());
        view.toggle_history();
        view.toggle_history();
        assert_eq!(view.begin_log(), None);
        let requests = view.logged(Ok(vec![commit("a")]));
        assert!(requests.log);
        assert!(view.begin_log().is_some());
    }

    #[test]
    fn a_status_refresh_in_the_history_keeps_a_failed_logs_error() {
        let mut view = history();
        view.begin_log();
        view.logged(Err("index.lock".into()));
        assert_eq!(view.refresh_error.as_deref(), Some("index.lock"));
        view.begin_status();
        view.status_loaded(Ok(working()));
        assert_eq!(view.refresh_error.as_deref(), Some("index.lock"));
        view.begin_log();
        view.logged(Ok(vec![commit("b")]));
        assert_eq!(view.refresh_error, None);
    }

    fn text_line(kind: DiffLineKind, old: Option<u32>, new: Option<u32>, text: &str) -> DiffLine {
        DiffLine {
            text: text.into(),
            ..line(kind, old, new)
        }
    }

    #[test]
    fn diff_comments_quote_the_picked_new_side_lines() {
        let mut first = hunk(1, 2, 1, 2);
        first.lines = vec![
            text_line(DiffLineKind::Context, Some(1), Some(1), "  fn a() {"),
            text_line(DiffLineKind::Del, Some(2), None, "old();"),
            text_line(DiffLineKind::Add, None, Some(2), "new();  "),
        ];
        let mut second = hunk(9, 1, 9, 1);
        second.lines = vec![text_line(DiffLineKind::Add, None, Some(9), "end();")];
        let shown = laid_out(vec![first, second]);
        let pick = |start, end| Selection { start, end };
        assert_eq!(
            shown.quote(pick(1, 2)).as_deref(),
            Some("fn a() {\nnew();"),
            "removed lines are not new-side lines"
        );
        assert_eq!(
            shown.quote(pick(2, 9)).as_deref(),
            Some("new();  \nend();"),
            "only shown lines"
        );
        assert_eq!(shown.quote(pick(4, 8)), None);
        assert_eq!(
            shown.new_side_text(pick(1, 2)).as_deref(),
            Some("  fn a() {\nnew();  "),
            "sent selections keep the indent"
        );

        let mut view = loaded();
        assert_eq!(view.begin_comments().as_deref(), Some("a.rs"));
        assert_eq!(view.begin_comments(), None, "same file");
        view.comments.listed("a.rs", Ok(vec![]));
        assert!(
            !view.comments_shown(),
            "nothing to show: the diff keeps its width"
        );
        view.comments.pick(2, 2, false);
        assert!(view.comments_shown());
        assert!(!view.start_comment(), "diff still loading");
        assert!(view.begin_diff());
        view.diff_loaded(&target(Group::Staged, "a.rs"), Ok(shown));
        assert!(view.start_comment());
        let draft = view.comments.draft.as_ref().unwrap();
        assert_eq!(draft.quote, "new();");
        assert_eq!(view.comments.context, CommentContext::Diff);

        // The same file's other diff keeps its threads but not the picked lines.
        view.comments.draft = None;
        view.comments.pick(2, 2, false);
        view.select(target(Group::Unstaged, "a.rs"));
        assert_eq!(view.begin_comments(), None, "same file: listed already");
        assert_eq!(view.comments.selection, None);

        // In the History no line is commented on.
        view.comments.pick(2, 2, false);
        view.toggle_history();
        assert!(!view.comments_shown() && !view.start_comment());
        assert_eq!(view.begin_comments(), None, "not listed in the History");
        view.toggle_history();
        assert_eq!(
            view.comments.selection,
            Some(Selection { start: 2, end: 2 })
        );

        // Another file starts its comments over.
        view.select(target(Group::Unstaged, "b.rs"));
        assert_eq!(view.begin_comments().as_deref(), Some("b.rs"));
        assert_eq!(view.comments.selection, None);
        assert!(!view.comments_shown());
    }

    fn session(id: u128, task: Option<u128>, started_at: i64, state: SessionState) -> Session {
        use mkrew_protocol::{SessionKind, StateSource};
        Session {
            id: Ulid(id),
            task_id: task.map(Ulid),
            project_id: Ulid(1),
            agent_profile: "fake".into(),
            kind: SessionKind::Agent,
            title: None,
            native_session_id: None,
            state,
            state_source: StateSource::Screen,
            state_changed_at: 0,
            exit_code: None,
            cols: 80,
            rows: 24,
            started_at,
            ended_at: None,
        }
    }

    #[test]
    fn send_to_agent_offers_the_tasks_newest_session_first() {
        let (idle, exited) = (SessionState::Idle, SessionState::Exited);
        let mut sessions = vec![
            session(1, Some(7), 1, idle),
            session(2, Some(7), 5, idle),
            session(3, None, 9, idle),
        ];
        let mut view = GitView::new(Ulid(1), None);
        assert_eq!(view.since_session(&sessions), None, "not a task's changes");
        view.show_since(Since::task(Ulid(7), "Fix it", "abc".into()));
        assert_eq!(view.since_session(&sessions), Some(Ulid(2)));
        sessions[1].state = exited;
        assert_eq!(view.since_session(&sessions), Some(Ulid(1)), "live first");
        sessions[0].state = exited;
        assert_eq!(view.since_session(&sessions), Some(Ulid(2)), "else newest");
        view.toggle_history();
        assert_eq!(view.since_session(&sessions), None, "in the History");
        // A turn's changes: its own session, even exited.
        view.show_since(Since::turn(&sessions[2], &sessions, 3, "tree".into()));
        assert_eq!(view.since_session(&sessions), Some(Ulid(3)));
    }

    #[test]
    fn a_turns_changes_list_and_diff_since_its_start_tree() {
        let sessions = [
            session(2, None, 5, SessionState::Idle),
            session(1, None, 1, SessionState::Idle),
        ];
        let turn = Since::turn(&sessions[0], &sessions, 3, "tree3".into());
        assert_eq!(turn.label, "Since turn 3 of fake #2");
        let mut view = GitView::new(Ulid(1), None);
        view.show_since(turn);
        assert_eq!(view.section(), Section::Since);
        let params = view.begin_status().unwrap();
        assert_eq!(
            (params.mode, params.since.as_deref()),
            (GitMode::Base, Some("tree3"))
        );
        let requests = view.status_loaded(Ok(since(&["a.rs"])));
        let diff = requests.diff.unwrap().params(view.project_id, None);
        assert_eq!(
            (diff.mode, diff.since.as_deref()),
            (GitMode::Base, Some("tree3"))
        );
        assert_eq!(
            Since::task(Ulid(7), "Fix it", "0123456789abcdef".into()).label,
            "Since 0123456 · task: Fix it"
        );
    }

    #[test]
    fn sessions_are_numbered_only_among_namesakes_of_the_workspace() {
        let idle = SessionState::Idle;
        let mut other = session(4, None, 0, idle);
        other.project_id = Ulid(9);
        let mut titled = session(5, None, 2, idle);
        titled.title = Some("review".into());
        let sessions = [
            session(3, None, 7, idle),
            session(2, None, 3, idle),
            other,
            titled,
        ];
        let label = |i: usize| session_label(&sessions[i], &sessions);
        assert_eq!(label(0), "fake #2");
        assert_eq!(label(1), "fake #1");
        assert_eq!(label(2), "fake", "alone in its workspace");
        assert_eq!(label(3), "review");
    }

    #[test]
    fn changed_by_keeps_only_the_sessions_files() {
        let idle = SessionState::Idle;
        let sessions = [session(1, None, 1, idle), session(2, None, 2, idle)];
        let mut view = loaded();
        let (params, late) = view.begin_session_files(CHANGED_BY_WINDOW_MS + 5);
        assert_eq!(params.since_ms, 5, "the last 24 h");
        let (_, serial) = view.begin_session_files(0);
        let stale = SessionFiles::from([(Ulid(9), vec!["a.rs".into()])]);
        view.session_files_loaded(late, stale);
        assert!(
            view.changers(&sessions).is_empty(),
            "a late answer is dropped"
        );
        let loaded = view.session_files_loaded(
            serial,
            SessionFiles::from([
                (Ulid(2), vec!["b.rs".into(), "gone.rs".into()]),
                (Ulid(1), vec!["new.txt".into()]),
                (Ulid(3), vec!["a.rs".into()]),
                (Ulid(4), vec![]),
                (Ulid(5), vec!["a.rs".into()]),
            ]),
        );
        assert_eq!(loaded, Requests::default(), "All still lists every file");
        let labels: Vec<String> = view
            .changers(&sessions)
            .into_iter()
            .map(|changer| changer.label)
            .collect();
        let line = format!("Changed by: All | {}", labels.join(" | "));
        insta::assert_snapshot!(line);

        // The selected a.rs is filtered out: the first kept file is selected.
        let requests = view.filter_changed_by(Some(Ulid(2)));
        assert_eq!(requests.diff, Some(target(Group::Unstaged, "b.rs")));
        let paths: Vec<String> = view.files().into_iter().map(|(t, _)| t.path).collect();
        assert_eq!(paths, ["b.rs"]);
        assert_eq!(view.changed_by_label(&sessions).as_deref(), Some("fake #2"));
        assert_eq!(view.filter_changed_by(Some(Ulid(2))), Requests::default());

        // Its files are gone from `session.files`: back to All.
        let (_, serial) = view.begin_session_files(0);
        view.session_files_loaded(
            serial,
            SessionFiles::from([(Ulid(1), vec!["new.txt".into()])]),
        );
        assert_eq!(view.changed_by.session, None);
        assert_eq!(view.files().len(), 4);

        // Nothing kept: nothing selected.
        view.filter_changed_by(Some(Ulid(1)));
        let (_, serial) = view.begin_session_files(0);
        view.session_files_loaded(serial, SessionFiles::from([(Ulid(1), vec!["x.rs".into()])]));
        assert!(view.files().is_empty());
        assert_eq!(view.selected, None);

        // The History lists commits: no filter there.
        view.toggle_history();
        assert!(view.changers(&sessions).is_empty());
    }

    #[test]
    fn keys_are_unmodified_arrows_and_j_k() {
        let parse = |keys: &str| key(&Keystroke::parse(keys).unwrap());
        assert_eq!(parse("up"), Some(Key::Up));
        assert_eq!(parse("k"), Some(Key::Up));
        assert_eq!(parse("down"), Some(Key::Down));
        assert_eq!(parse("j"), Some(Key::Down));
        // Keymap commands now (they would act twice otherwise).
        for keys in ["n", "p", "b", "s", "u"] {
            assert_eq!(parse(keys), None, "{keys}");
        }
        assert_eq!(parse("ctrl-k"), None);
        assert_eq!(parse("x"), None);
    }
}
