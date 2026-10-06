//! Params, results and events of the Files & Git requests (SPEC §7 `fs.*`/`git.*`, §12A, §12B).
//!
//! Paths are relative to the project root with `/` separators. Commit ids are full hex
//! hashes.

use serde::{Deserialize, Serialize};
use ulid::Ulid;

/// Params of requests that only name a project (`git.fetch`, `git.push`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectRef {
    pub project_id: Ulid,
}

// ---- git ----

/// What the Git view compares (SPEC §12B "Two modes").
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GitMode {
    /// Uncommitted changes: Staged / Unstaged / Untracked.
    #[default]
    Working,
    /// Everything on this branch since `git merge-base HEAD <base>`, up to the working tree.
    Base,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GitStatusParams {
    pub project_id: Ulid,
    #[serde(default)]
    pub mode: GitMode,
    /// [`GitMode::Base`] only: compare from this commit instead of the merge-base (a task's
    /// "View changes" from `task.checkpoint`); `merge_base` is then this commit.
    #[serde(default)]
    pub since: Option<String>,
}

/// `git.status` result: the Git view header plus the changed files of the requested mode.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct GitStatus {
    /// `None` = detached HEAD.
    pub branch: Option<String>,
    /// `None` = no commit yet.
    pub head: Option<String>,
    /// e.g. `origin/main`; `None` = no upstream configured.
    pub upstream: Option<String>,
    pub ahead: u32,
    pub behind: u32,
    /// The base used for comparison: `project.base_ref` if set, else the auto-detected one
    /// (`origin/HEAD` → `main` → `master`); `None` = no base found.
    pub base_ref: Option<String>,
    /// True when `base_ref` was auto-detected (`project.base_ref` is `null`).
    pub base_auto: bool,
    /// `git merge-base HEAD <base_ref>`; only computed in [`GitMode::Base`].
    pub merge_base: Option<String>,
    /// [`GitMode::Working`]: changes in the index.
    pub staged: Vec<ChangedFile>,
    /// [`GitMode::Working`]: changes in the working tree not staged, incl. conflicted files.
    pub unstaged: Vec<ChangedFile>,
    /// [`GitMode::Working`]: untracked files (not ignored).
    pub untracked: Vec<ChangedFile>,
    /// [`GitMode::Base`]: changes from `merge_base` to the working tree.
    pub against_base: Vec<ChangedFile>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChangeKind {
    Added,
    Modified,
    Deleted,
    Renamed,
    Copied,
    /// File type changed (e.g. file ↔ symlink).
    TypeChanged,
    Untracked,
    /// Unmerged (merge conflict).
    Conflicted,
}

/// One entry of a changed-file list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChangedFile {
    pub path: String,
    /// The source path of a rename or copy.
    #[serde(default)]
    pub old_path: Option<String>,
    pub kind: ChangeKind,
    /// Added lines; `None` = binary or not counted.
    #[serde(default)]
    pub added: Option<u32>,
    /// Removed lines; `None` = binary or not counted.
    #[serde(default)]
    pub removed: Option<u32>,
}

/// `git.diff`: the diff of one file, computed on demand.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GitDiffParams {
    pub project_id: Ulid,
    pub path: String,
    #[serde(default)]
    pub mode: GitMode,
    /// [`GitMode::Working`] only: the staged (index vs HEAD) side instead of the unstaged one.
    #[serde(default)]
    pub staged: bool,
    /// The diff this commit introduced (history); `mode`/`staged` are then ignored.
    #[serde(default)]
    pub commit: Option<String>,
    /// [`GitMode::Base`] only: the diff from this commit to the working tree instead of from
    /// the merge-base (a task's "View changes").
    #[serde(default)]
    pub since: Option<String>,
}

/// `git.diff` result.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct FileDiff {
    pub path: String,
    #[serde(default)]
    pub old_path: Option<String>,
    /// Binary file: no hunks.
    pub binary: bool,
    /// The diff is over 1 MB: only the counts are sent, no hunks (UI offers "Open in Zed").
    pub too_large: bool,
    pub added: u32,
    pub removed: u32,
    pub hunks: Vec<Hunk>,
}

/// One `@@ -old_start,old_lines +new_start,new_lines @@ header` block.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Hunk {
    pub old_start: u32,
    pub old_lines: u32,
    pub new_start: u32,
    pub new_lines: u32,
    /// Text after the closing `@@` (often the enclosing function); may be empty.
    #[serde(default)]
    pub header: String,
    pub lines: Vec<DiffLine>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DiffLineKind {
    Context,
    Add,
    Del,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiffLine {
    pub kind: DiffLineKind,
    /// Without the leading ` `/`+`/`-` and without the line ending.
    pub text: String,
    /// 1-based; `None` for added lines.
    pub old_line: Option<u32>,
    /// 1-based; `None` for deleted lines.
    pub new_line: Option<u32>,
    /// Followed by `\ No newline at end of file`.
    #[serde(default)]
    pub no_newline: bool,
    /// The line ended with `\r\n` (the `\r` is not in `text`); kept so a hunk patch for
    /// `git apply` reproduces the file exactly.
    #[serde(default)]
    pub crlf: bool,
}

/// `git.stage` / `git.unstage`: whole files (`paths`) or one hunk.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GitStageParams {
    pub project_id: Ulid,
    #[serde(default)]
    pub paths: Vec<String>,
    #[serde(default)]
    pub hunk: Option<HunkTarget>,
}

/// A hunk as shown by `git.diff`; the daemon builds the patch for `git apply --cached [-R]`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HunkTarget {
    pub path: String,
    pub hunk: Hunk,
}

/// `git.discard`: destructive; `invalid_request` unless `confirm` is true.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GitDiscardParams {
    pub project_id: Ulid,
    pub paths: Vec<String>,
    #[serde(default)]
    pub confirm: bool,
}

/// `git.commit` of the staged changes (no amend in v1).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GitCommitParams {
    pub project_id: Ulid,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GitCommitResult {
    pub commit: String,
}

/// `git.log`: newest first.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GitLogParams {
    pub project_id: Ulid,
    #[serde(default = "GitLogParams::default_limit")]
    pub limit: u32,
    /// Only commits touching this path.
    #[serde(default)]
    pub path: Option<String>,
}

impl GitLogParams {
    /// History length shown by the Git view (SPEC §12B).
    pub const DEFAULT_LIMIT: u32 = 50;

    fn default_limit() -> u32 {
        Self::DEFAULT_LIMIT
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Commit {
    pub hash: String,
    pub parents: Vec<String>,
    pub author_name: String,
    pub author_email: String,
    /// Unix ms.
    pub authored_at: i64,
    /// First line of the message.
    pub summary: String,
    /// The rest of the message; may be empty.
    #[serde(default)]
    pub body: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GitShowParams {
    pub project_id: Ulid,
    pub commit: String,
}

/// `git.show` result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommitDetail {
    pub commit: Commit,
    pub files: Vec<ChangedFile>,
}

/// `git.set_base`: `None` (JSON `null`) = auto-detect.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GitSetBaseParams {
    pub project_id: Ulid,
    #[serde(default)]
    pub base_ref: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GitChangedEvent {
    pub project_id: Ulid,
}

// ---- fs ----

/// `fs.list`: the direct children of one folder (the tree loads lazily).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FsListParams {
    pub project_id: Ulid,
    /// `""` = the project root.
    #[serde(default)]
    pub dir: String,
    #[serde(default)]
    pub show_ignored: bool,
}

/// Git marker next to a file name (SPEC §12A).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GitMarker {
    Modified,
    Added,
    Deleted,
    Untracked,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FsEntry {
    pub name: String,
    pub path: String,
    pub is_dir: bool,
    /// Git-ignored (only listed with `show_ignored`).
    #[serde(default)]
    pub ignored: bool,
    /// Files only.
    #[serde(default)]
    pub marker: Option<GitMarker>,
    /// Folders only: something below has a git marker.
    #[serde(default)]
    pub has_changes: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FsReadParams {
    pub project_id: Ulid,
    pub path: String,
}

/// `fs.read` result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FsContent {
    pub path: String,
    /// `None` when `binary` or `too_large`.
    pub content: Option<String>,
    /// Bytes.
    pub size: u64,
    pub binary: bool,
    /// Over the 2 MB viewer limit.
    pub too_large: bool,
    /// Syntax name guessed from the extension (e.g. `rust`); `None` = plain text.
    pub language: Option<String>,
}

/// `fs.find`: quick open over the project's files.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FsFindParams {
    pub project_id: Ulid,
    pub query: String,
    #[serde(default = "FsFindParams::default_limit")]
    pub limit: u32,
}

impl FsFindParams {
    pub const DEFAULT_LIMIT: u32 = 50;

    fn default_limit() -> u32 {
        Self::DEFAULT_LIMIT
    }
}

/// One `fs.find` result, best first.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FsMatch {
    pub path: String,
    pub score: u32,
    /// Char indices of `path` that matched the query (for highlighting).
    pub indices: Vec<u32>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Event, Message, Outcome, Request};
    use serde_json::json;

    fn sid() -> Ulid {
        Ulid::from_parts(1_700_000_000_000, 7)
    }

    fn round_trip(message: &Message) -> serde_json::Value {
        let value = serde_json::to_value(message).unwrap();
        let back: Message = serde_json::from_value(value.clone()).unwrap();
        assert_eq!(&back, message);
        value
    }

    fn hunk() -> Hunk {
        Hunk {
            old_start: 1,
            old_lines: 2,
            new_start: 1,
            new_lines: 2,
            header: "fn main()".into(),
            lines: vec![
                DiffLine {
                    kind: DiffLineKind::Context,
                    text: "a".into(),
                    old_line: Some(1),
                    new_line: Some(1),
                    no_newline: false,
                    crlf: false,
                },
                DiffLine {
                    kind: DiffLineKind::Del,
                    text: "b".into(),
                    old_line: Some(2),
                    new_line: None,
                    no_newline: false,
                    crlf: false,
                },
                DiffLine {
                    kind: DiffLineKind::Add,
                    text: "c".into(),
                    old_line: None,
                    new_line: Some(2),
                    no_newline: true,
                    crlf: true,
                },
            ],
        }
    }

    #[test]
    fn every_files_and_git_request_round_trips() {
        let project_id = sid();
        let requests = vec![
            Request::GitStatus(GitStatusParams {
                project_id,
                mode: GitMode::Base,
                since: Some("abc".into()),
            }),
            Request::GitDiff(GitDiffParams {
                project_id,
                path: "src/a.rs".into(),
                mode: GitMode::Working,
                staged: true,
                commit: Some("abc".into()),
                since: None,
            }),
            Request::GitStage(GitStageParams {
                project_id,
                paths: vec!["a".into()],
                hunk: None,
            }),
            Request::GitUnstage(GitStageParams {
                project_id,
                paths: vec![],
                hunk: Some(HunkTarget {
                    path: "a".into(),
                    hunk: hunk(),
                }),
            }),
            Request::GitDiscard(GitDiscardParams {
                project_id,
                paths: vec!["a".into()],
                confirm: true,
            }),
            Request::GitCommit(GitCommitParams {
                project_id,
                message: "feat: x\n\nbody".into(),
            }),
            Request::GitLog(GitLogParams {
                project_id,
                limit: 10,
                path: Some("a".into()),
            }),
            Request::GitShow(GitShowParams {
                project_id,
                commit: "abc".into(),
            }),
            Request::GitSetBase(GitSetBaseParams {
                project_id,
                base_ref: Some("develop".into()),
            }),
            Request::GitFetch(ProjectRef { project_id }),
            Request::GitPush(ProjectRef { project_id }),
            Request::FsList(FsListParams {
                project_id,
                dir: "src".into(),
                show_ignored: true,
            }),
            Request::FsRead(FsReadParams {
                project_id,
                path: "src/a.rs".into(),
            }),
            Request::FsFind(FsFindParams {
                project_id,
                query: "mai".into(),
                limit: 20,
            }),
        ];
        let methods = [
            "git.status",
            "git.diff",
            "git.stage",
            "git.unstage",
            "git.discard",
            "git.commit",
            "git.log",
            "git.show",
            "git.set_base",
            "git.fetch",
            "git.push",
            "fs.list",
            "fs.read",
            "fs.find",
        ];
        assert_eq!(requests.len(), methods.len());
        for (id, (request, method)) in requests.into_iter().zip(methods).enumerate() {
            let value = round_trip(&Message::Request {
                id: id as u64,
                request,
            });
            assert_eq!(value["method"], method);
        }
    }

    #[test]
    fn optional_params_default_when_missing() {
        let id = sid().to_string();
        let status: GitStatusParams = serde_json::from_value(json!({"project_id": id})).unwrap();
        assert_eq!(status.mode, GitMode::Working);
        let diff: GitDiffParams =
            serde_json::from_value(json!({"project_id": id, "path": "a"})).unwrap();
        assert_eq!(
            (diff.mode, diff.staged, diff.commit),
            (GitMode::Working, false, None)
        );
        let discard: GitDiscardParams =
            serde_json::from_value(json!({"project_id": id, "paths": ["a"]})).unwrap();
        assert!(!discard.confirm);
        let log: GitLogParams = serde_json::from_value(json!({"project_id": id})).unwrap();
        assert_eq!((log.limit, log.path), (50, None));
        let stage: GitStageParams = serde_json::from_value(json!({"project_id": id})).unwrap();
        assert_eq!((stage.paths.len(), stage.hunk), (0, None));
        let list: FsListParams = serde_json::from_value(json!({"project_id": id})).unwrap();
        assert_eq!((list.dir.as_str(), list.show_ignored), ("", false));
        let find: FsFindParams =
            serde_json::from_value(json!({"project_id": id, "query": "x"})).unwrap();
        assert_eq!(find.limit, 50);
        // `null` and a missing base both mean auto-detect.
        let auto: GitSetBaseParams =
            serde_json::from_value(json!({"project_id": id, "base_ref": null})).unwrap();
        assert_eq!(auto.base_ref, None);
        let missing: GitSetBaseParams = serde_json::from_value(json!({"project_id": id})).unwrap();
        assert_eq!(missing, auto);
    }

    #[test]
    fn results_and_event_wire_shapes() {
        let status = GitStatus {
            branch: Some("feature".into()),
            head: Some("abc".into()),
            upstream: Some("origin/feature".into()),
            ahead: 2,
            behind: 1,
            base_ref: Some("origin/main".into()),
            base_auto: true,
            merge_base: None,
            staged: vec![ChangedFile {
                path: "new.rs".into(),
                old_path: Some("old.rs".into()),
                kind: ChangeKind::Renamed,
                added: Some(3),
                removed: Some(1),
            }],
            unstaged: vec![],
            untracked: vec![ChangedFile {
                path: "x.bin".into(),
                old_path: None,
                kind: ChangeKind::Untracked,
                added: None,
                removed: None,
            }],
            against_base: vec![],
        };
        let value = serde_json::to_value(&status).unwrap();
        assert_eq!(value["staged"][0]["kind"], "renamed");
        assert_eq!(value["untracked"][0]["kind"], "untracked");
        let decoded: GitStatus = Outcome::ok(&status).unwrap().into_result().unwrap();
        assert_eq!(decoded, status);

        let diff = FileDiff {
            path: "a.rs".into(),
            added: 1,
            removed: 1,
            hunks: vec![hunk()],
            ..FileDiff::default()
        };
        let value = serde_json::to_value(&diff).unwrap();
        assert_eq!(value["hunks"][0]["lines"][1]["kind"], "del");
        assert_eq!(value["hunks"][0]["lines"][1]["new_line"], json!(null));
        let decoded: FileDiff = Outcome::ok(&diff).unwrap().into_result().unwrap();
        assert_eq!(decoded, diff);

        let detail = CommitDetail {
            commit: Commit {
                hash: "abc".into(),
                parents: vec!["def".into()],
                author_name: "A".into(),
                author_email: "a@example.com".into(),
                authored_at: 1,
                summary: "feat: x".into(),
                body: String::new(),
            },
            files: status.staged.clone(),
        };
        let decoded: CommitDetail = Outcome::ok(&detail).unwrap().into_result().unwrap();
        assert_eq!(decoded, detail);

        let content = FsContent {
            path: "a.png".into(),
            content: None,
            size: 10,
            binary: true,
            too_large: false,
            language: None,
        };
        let decoded: FsContent = Outcome::ok(&content).unwrap().into_result().unwrap();
        assert_eq!(decoded, content);
        let entry: FsEntry =
            serde_json::from_value(json!({"name": "src", "path": "src", "is_dir": true})).unwrap();
        assert_eq!(
            (entry.ignored, entry.marker, entry.has_changes),
            (false, None, false)
        );

        let changed = round_trip(&Message::Event(Event::GitChanged(GitChangedEvent {
            project_id: sid(),
        })));
        assert_eq!(changed["event"], "git.changed");
        assert_eq!(changed["data"]["project_id"], sid().to_string());
    }
}
