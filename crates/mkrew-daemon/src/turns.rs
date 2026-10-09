//! Persisted turns (SPEC §20C.1): one recorder task per session takes the turn snapshots,
//! writes the `turn` rows and announces each ended turn as `session.turn`. Jobs run in order,
//! so a turn's end never overtakes its start; the session task never waits for git.

use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use mkrew_core::turn::Turn;
use mkrew_protocol::{Event, SessionTurnEvent, TurnFile};
use tokio::sync::{mpsc, oneshot};
use ulid::Ulid;

use crate::git::Side;
use crate::session::Hub;
use crate::snapshot::turn_ref;

/// Turn refs of sessions that ended longer ago are deleted at startup.
const KEEP_REFS_MS: i64 = 7 * 24 * 60 * 60 * 1000;

pub enum Job {
    /// Turn `n` started at `at`; `reply` is dropped once the start snapshot is taken (a
    /// `hook.report` that started the turn is answered then).
    Start {
        n: u32,
        at: i64,
        reply: Option<oneshot::Sender<()>>,
    },
    /// The turn ended (`ended_at` set); `snapshot` = it ended done/idle/exited, so its end
    /// snapshot is taken now (a `needs_input` end gets the next turn's start snapshot).
    End { turn: Turn, snapshot: bool },
}

/// The ref keeping the end snapshot of turn `n` when no next turn started (yet).
fn turn_end_ref(session: Ulid, n: u32) -> String {
    format!("{}-end", turn_ref(session, n))
}

/// Records the turns of `session` (in `project`, running in `dir`) until `jobs` closes.
pub async fn record(
    hub: Arc<Hub>,
    session: Ulid,
    project: Ulid,
    dir: PathBuf,
    mut jobs: mpsc::UnboundedReceiver<Job>,
) {
    let mut recorder = Recorder {
        hub,
        session,
        project,
        dir: Arc::new(dir),
        in_git: None,
        started: None,
        unfinished: None,
    };
    while let Some(job) = jobs.recv().await {
        match job {
            Job::Start { n, at, reply } => {
                recorder.start(n, at).await;
                drop(reply);
            }
            Job::End { turn, snapshot } => recorder.end(turn, snapshot).await,
        }
    }
    // The session is gone: no next turn will bring the end snapshot.
    recorder.finish_unfinished().await;
}

struct Recorder {
    hub: Arc<Hub>,
    session: Ulid,
    project: Ulid,
    dir: Arc<PathBuf>,
    /// Whether `dir` is in a git repo; checked at the first snapshot.
    in_git: Option<bool>,
    /// The last started turn and its start snapshot.
    started: Option<(u32, Option<String>)>,
    /// The last turn when it ended at `needs_input` with a start snapshot: the next turn's
    /// start snapshot is its end snapshot.
    unfinished: Option<Turn>,
}

impl Recorder {
    async fn start(&mut self, n: u32, at: i64) {
        let tree = self.snapshot(turn_ref(self.session, n)).await;
        if let Some(turn) = self.unfinished.take() {
            let files = self.counted(&turn.files, self.start_tree(turn.n), tree.clone());
            self.save_end(&turn, tree.as_deref(), &files.await);
        }
        self.started = Some((n, tree.clone()));
        if let Err(e) = self
            .hub
            .store()
            .start_turn(self.session, n, at, tree.as_deref())
        {
            tracing::error!(id = %self.session, n, "saving the turn start failed: {e}");
        }
    }

    async fn end(&mut self, turn: Turn, snapshot: bool) {
        let start = self.start_tree(turn.n);
        let end = match (&start, snapshot) {
            (Some(_), true) => self.snapshot(turn_end_ref(self.session, turn.n)).await,
            _ => None,
        };
        let files = self.counted(&turn.files, start.clone(), end.clone()).await;
        self.save_end(&turn, end.as_deref(), &files);
        self.unfinished = (start.is_some() && !snapshot).then(|| turn.clone());
        let _ = self.hub.events.send(Event::SessionTurn(SessionTurnEvent {
            session_id: self.session,
            n: turn.n,
            started_at: turn.started_at,
            // Infallible: ended turns carry their end.
            ended_at: turn.ended_at.unwrap_or(turn.started_at),
            files,
        }));
    }

    async fn finish_unfinished(&mut self) {
        let Some(turn) = self.unfinished.take() else {
            return;
        };
        let end = self.snapshot(turn_end_ref(self.session, turn.n)).await;
        let files = self.counted(&turn.files, self.start_tree(turn.n), end.clone());
        self.save_end(&turn, end.as_deref(), &files.await);
    }

    fn start_tree(&self, n: u32) -> Option<String> {
        self.started
            .as_ref()
            .filter(|(started, _)| *started == n)
            .and_then(|(_, tree)| tree.clone())
    }

    fn save_end(&self, turn: &Turn, end_tree: Option<&str>, files: &[TurnFile]) {
        let ended_at = turn.ended_at.unwrap_or(turn.started_at);
        if let Err(e) = self
            .hub
            .store()
            .end_turn(self.session, turn.n, ended_at, end_tree, files)
        {
            tracing::error!(id = %self.session, n = turn.n, "saving the turn end failed: {e}");
        }
    }

    /// The working state's tree, kept by `ref_name`; `None` outside git or on failure (e.g. a
    /// locked file makes `git add` fail: that snapshot is skipped).
    async fn snapshot(&mut self, ref_name: String) -> Option<String> {
        let hub = Arc::clone(&self.hub);
        let (project, dir, in_git) = (self.project, Arc::clone(&self.dir), self.in_git);
        let taken = tokio::task::spawn_blocking(move || {
            let in_git = match in_git {
                Some(in_git) => in_git,
                None => hub.git.repo(&dir).is_ok_and(|repo| repo.is_some()),
            };
            let tree = in_git.then(|| hub.snapshots.take(&*hub.git, project, &dir, &ref_name));
            (in_git, tree)
        })
        .await;
        match taken {
            Ok((in_git, tree)) => {
                self.in_git = Some(in_git);
                tree?
                    .inspect_err(
                        |e| tracing::warn!(id = %self.session, "turn snapshot skipped: {e}"),
                    )
                    .ok()
            }
            Err(e) => {
                tracing::error!(id = %self.session, "turn snapshot panicked: {e}");
                None
            }
        }
    }

    /// `paths` with their lines added/removed between the two trees (when both exist).
    async fn counted(
        &self,
        paths: &BTreeSet<String>,
        start: Option<String>,
        end: Option<String>,
    ) -> Vec<TurnFile> {
        let mut counts = HashMap::new();
        if let (Some(start), Some(end)) = (start, end) {
            let hub = Arc::clone(&self.hub);
            let dir = Arc::clone(&self.dir);
            let changes = tokio::task::spawn_blocking(move || {
                hub.git.changes(&dir, Side::Between(&start, &end))
            })
            .await;
            match changes {
                Ok(Ok(changes)) => {
                    counts = changes
                        .into_iter()
                        .map(|c| (c.path, (c.added, c.removed)))
                        .collect();
                }
                Ok(Err(e)) => tracing::warn!(id = %self.session, "turn line counts failed: {e}"),
                Err(e) => tracing::error!(id = %self.session, "turn line counts panicked: {e}"),
            }
        }
        paths
            .iter()
            .map(|path| {
                let (added, removed) = counts.get(path).copied().unwrap_or_default();
                TurnFile {
                    path: path.clone(),
                    added,
                    removed,
                }
            })
            .collect()
    }
}

/// Startup (SPEC §20C.1): deletes `refs/mkrew/turns/<sid>/*` of sessions that ended more than
/// 7 days before `now`, then forgets their trees so this never repeats. Blocking.
pub fn delete_expired_refs(hub: &Hub, now: i64) {
    let expired = match hub.store().sessions_with_expired_turns(now - KEEP_REFS_MS) {
        Ok(expired) => expired,
        Err(e) => {
            tracing::error!("listing expired turn refs failed: {e}");
            return;
        }
    };
    for (session, project) in expired {
        let path = hub.store().get_project(project).map(|p| p.path);
        let deleted = match path {
            Ok(path) => {
                let prefix = format!("refs/mkrew/turns/{session}/");
                hub.git
                    .delete_refs(Path::new(&path), &prefix)
                    .map_err(|e| e.to_string())
            }
            Err(e) => Err(e.to_string()),
        };
        match deleted.and_then(|()| {
            hub.store()
                .forget_turn_trees(session)
                .map_err(|e| e.to_string())
        }) {
            Ok(()) => tracing::info!(%session, "deleted expired turn refs"),
            Err(e) => tracing::warn!(%session, "expired turn refs not deleted: {e}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::process::Command;

    use mkrew_protocol::{Session, SessionKind, SessionState, StateSource};
    use mkrew_store::Store;

    use super::*;

    fn git(dir: &Path, args: &[&str]) -> String {
        let output = Command::new("git")
            .arg("-C")
            .arg(dir)
            .args(["-c", "user.name=t", "-c", "user.email=t@t"])
            .args(["-c", "commit.gpgsign=false"])
            .args(args)
            .env_remove("GIT_DIR")
            .env_remove("GIT_WORK_TREE")
            .env_remove("GIT_INDEX_FILE")
            .output()
            .unwrap();
        assert!(output.status.success(), "git {args:?} failed");
        String::from_utf8(output.stdout).unwrap().trim().to_owned()
    }

    /// A hub (database in `base`) over a project in `<base>/repo` (a git repo with `a.txt`
    /// committed when `in_git`) and one agent session in it.
    fn hub(base: &tempfile::TempDir, in_git: bool) -> (PathBuf, Arc<Hub>, Ulid, Ulid) {
        let base = dunce::canonicalize(base.path()).unwrap();
        let dir = &base.join("repo");
        std::fs::create_dir(dir).unwrap();
        if in_git {
            git(dir, &["init", "-q", "-b", "main"]);
            git(dir, &["config", "core.autocrlf", "false"]);
            std::fs::write(dir.join("a.txt"), "1\n2\n").unwrap();
            git(dir, &["add", "a.txt"]);
            git(dir, &["commit", "-q", "-m", "init"]);
        }
        let store = Store::open(&base.join("mkrew.db")).unwrap();
        let project = store.add_project(dir, Some("repo")).unwrap().id;
        let session = Ulid::new();
        store
            .insert_session(&Session {
                id: session,
                task_id: None,
                project_id: project,
                agent_profile: "claude".into(),
                kind: SessionKind::Agent,
                title: None,
                terminal_title: None,
                native_session_id: None,
                state: SessionState::Working,
                state_source: StateSource::Daemon,
                state_changed_at: 0,
                exit_code: None,
                cols: 80,
                rows: 24,
                started_at: 0,
                ended_at: None,
            })
            .unwrap();
        (dir.clone(), Arc::new(Hub::new(store)), project, session)
    }

    fn ended(n: u32, started_at: i64, ended_at: i64, files: &[&str]) -> Turn {
        Turn {
            n,
            started_at,
            ended_at: Some(ended_at),
            files: files.iter().map(|f| f.to_string()).collect(),
        }
    }

    struct Running {
        jobs: mpsc::UnboundedSender<Job>,
        task: tokio::task::JoinHandle<()>,
    }

    fn run(hub: &Arc<Hub>, project: Ulid, session: Ulid, dir: &Path) -> Running {
        let (jobs, rx) = mpsc::unbounded_channel();
        let task = tokio::spawn(record(
            Arc::clone(hub),
            session,
            project,
            dir.to_path_buf(),
            rx,
        ));
        Running { jobs, task }
    }

    impl Running {
        /// Starts turn `n` and waits for its start snapshot.
        async fn start(&self, n: u32, at: i64) {
            let (reply, done) = oneshot::channel();
            let reply = Some(reply);
            self.jobs.send(Job::Start { n, at, reply }).unwrap();
            assert!(done.await.is_err(), "the reply is dropped, never sent");
        }

        fn end(&self, turn: Turn, snapshot: bool) {
            self.jobs.send(Job::End { turn, snapshot }).unwrap();
        }

        async fn finish(self) {
            drop(self.jobs);
            self.task.await.unwrap();
        }
    }

    fn file(path: &str, added: Option<u32>, removed: Option<u32>) -> TurnFile {
        TurnFile {
            path: path.into(),
            added,
            removed,
        }
    }

    #[tokio::test]
    async fn a_turn_in_git_records_both_snapshots_and_line_counts() {
        let dir = tempfile::tempdir().unwrap();
        let (root, hub, project, session) = hub(&dir, true);
        let mut events = hub.events.subscribe();
        let recorder = run(&hub, project, session, &root);

        recorder.start(1, 10).await;
        // The start snapshot is taken (and its ref written) before the reply.
        let start = git(&root, &["rev-parse", &turn_ref(session, 1)]);
        assert_eq!(git(&root, &["ls-tree", "--name-only", &start]), "a.txt");
        std::fs::write(root.join("a.txt"), "1\nx\ny\n").unwrap();
        std::fs::write(root.join("b.txt"), "b\n").unwrap();
        recorder.end(ended(1, 10, 20, &["a.txt", "b.txt", "gone.txt"]), true);
        recorder.finish().await;

        let turn = hub.store().get_turn(session, 1).unwrap();
        assert_eq!(turn.start_tree.as_deref(), Some(start.as_str()));
        let end = turn.end_tree.clone().unwrap();
        assert_eq!(git(&root, &["rev-parse", &turn_end_ref(session, 1)]), end);
        assert_eq!(turn.ended_at, Some(20));
        let files = vec![
            file("a.txt", Some(2), Some(1)),
            file("b.txt", Some(1), Some(0)),
            // Attributed but unchanged between the trees: no counts.
            file("gone.txt", None, None),
        ];
        assert_eq!(turn.files, files);
        let Ok(Event::SessionTurn(event)) = events.recv().await else {
            panic!("no session.turn");
        };
        assert_eq!((event.n, event.started_at, event.ended_at), (1, 10, 20));
        assert_eq!(event.files, files);
        // Nothing the user owns moved: no branch, no staged change.
        assert_eq!(
            git(&root, &["for-each-ref", "refs/heads"]).lines().count(),
            1
        );
        assert_eq!(git(&root, &["diff", "--cached", "--name-only"]), "");
    }

    #[tokio::test]
    async fn a_needs_input_end_gets_the_next_start_snapshot() {
        let dir = tempfile::tempdir().unwrap();
        let (root, hub, project, session) = hub(&dir, true);
        let recorder = run(&hub, project, session, &root);

        recorder.start(1, 10).await;
        std::fs::write(root.join("a.txt"), "1\n2\n3\n").unwrap();
        recorder.end(ended(1, 10, 20, &["a.txt"]), false);
        recorder.start(2, 30).await;
        recorder.finish().await;

        let first = hub.store().get_turn(session, 1).unwrap();
        let second = hub.store().get_turn(session, 2).unwrap();
        assert!(second.start_tree.is_some());
        assert_eq!(first.end_tree, second.start_tree);
        assert_eq!(first.ended_at, Some(20));
        assert_eq!(first.files, [file("a.txt", Some(1), Some(0))]);
        assert!(
            git(&root, &["for-each-ref", "refs/mkrew"])
                .lines()
                .all(|r| !r.ends_with("-end")),
            "no end ref when the next start keeps the tree"
        );
    }

    #[tokio::test]
    async fn a_needs_input_end_is_snapshotted_when_the_session_closes() {
        let dir = tempfile::tempdir().unwrap();
        let (root, hub, project, session) = hub(&dir, true);
        let recorder = run(&hub, project, session, &root);

        recorder.start(1, 10).await;
        std::fs::write(root.join("a.txt"), "1\n").unwrap();
        recorder.end(ended(1, 10, 20, &["a.txt"]), false);
        recorder.finish().await;

        let turn = hub.store().get_turn(session, 1).unwrap();
        let end = turn.end_tree.unwrap();
        assert_eq!(git(&root, &["rev-parse", &turn_end_ref(session, 1)]), end);
        assert_eq!(turn.ended_at, Some(20));
        assert_eq!(turn.files, [file("a.txt", Some(0), Some(1))]);
    }

    #[tokio::test]
    async fn turns_outside_git_have_no_trees_or_counts() {
        let dir = tempfile::tempdir().unwrap();
        let (root, hub, project, session) = hub(&dir, false);
        let recorder = run(&hub, project, session, &root);

        recorder.start(1, 10).await;
        std::fs::write(root.join("a.txt"), "a\n").unwrap();
        recorder.end(ended(1, 10, 20, &["a.txt"]), true);
        recorder.finish().await;

        let turn = hub.store().get_turn(session, 1).unwrap();
        assert_eq!((turn.start_tree, turn.end_tree), (None, None));
        assert_eq!(turn.files, [file("a.txt", None, None)]);
    }

    #[tokio::test]
    async fn refs_of_sessions_ended_over_7_days_ago_are_deleted_once() {
        let dir = tempfile::tempdir().unwrap();
        let (root, hub, project, old) = hub(&dir, true);
        let recorder = run(&hub, project, old, &root);
        recorder.start(1, 10).await;
        recorder.end(ended(1, 10, 20, &[]), true);
        recorder.finish().await;
        let now = 100 * KEEP_REFS_MS;
        {
            let store = hub.store();
            let mut session = store.get_session(old).unwrap();
            session.ended_at = Some(now - KEEP_REFS_MS - 1);
            store.update_session(&session).unwrap();
        }
        // A recent session keeps its refs.
        let recent = turn_ref(Ulid::new(), 1);
        git(&root, &["update-ref", &recent, "HEAD"]);

        delete_expired_refs(&hub, now);
        assert_eq!(
            git(
                &root,
                &["for-each-ref", "--format=%(refname)", "refs/mkrew"]
            ),
            recent
        );
        let turn = hub.store().get_turn(old, 1).unwrap();
        assert_eq!((turn.start_tree, turn.end_tree), (None, None));
        assert!(
            hub.store()
                .sessions_with_expired_turns(now - KEEP_REFS_MS)
                .unwrap()
                .is_empty()
        );
    }
}
