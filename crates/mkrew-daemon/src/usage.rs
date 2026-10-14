//! Token usage of agent sessions (SPEC §20E.3). A session whose profile has `[usage]` gets a
//! [`track`] task: at each turn end it finds the agent's transcript by its native session id,
//! reads the bytes appended since the last read and records and announces `session.usage`.

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use mkrew_core::profile::{UsageConfig, UsageFormat};
use mkrew_core::usage::{Usage, UsageReader};
use mkrew_protocol::{Event, SessionUsageEvent};
use tokio::sync::mpsc;
use ulid::Ulid;

use crate::session::Hub;

/// `session_event` kind of the recorded usage.
pub const EVENT_KIND: &str = "usage";

/// The folder the agent keeps its transcripts under: `CLAUDE_CONFIG_DIR` or `~/.claude`,
/// `CODEX_HOME` or `~/.codex`, `PI_CODING_AGENT_DIR` or `~/.omp/agent` (omp).
fn root(format: &UsageFormat) -> Option<PathBuf> {
    let (var, dir) = match format {
        UsageFormat::ClaudeJsonl => ("CLAUDE_CONFIG_DIR", ".claude"),
        UsageFormat::CodexJsonl => ("CODEX_HOME", ".codex"),
        UsageFormat::OmpJsonl => ("PI_CODING_AGENT_DIR", ".omp/agent"),
        UsageFormat::Unknown(_) => return None,
    };
    match std::env::var_os(var).filter(|v| !v.is_empty()) {
        Some(root) => Some(PathBuf::from(root)),
        None => std::env::home_dir().map(|home| home.join(dir)),
    }
}

/// The transcript of agent session `id` under `root`: `projects/*/<id>.jsonl` (Claude) or
/// `sessions/**/*<id>*.jsonl` (Codex, omp). An id that could leave the folder finds nothing.
fn locate(format: &UsageFormat, root: &Path, id: &str) -> Option<PathBuf> {
    let safe = |c: char| c.is_ascii_alphanumeric() || c == '-' || c == '_';
    if id.is_empty() || !id.chars().all(safe) {
        return None;
    }
    match format {
        UsageFormat::ClaudeJsonl => {
            let name = format!("{id}.jsonl");
            let mut dirs: Vec<PathBuf> = std::fs::read_dir(root.join("projects"))
                .ok()?
                .filter_map(|e| Some(e.ok()?.path()))
                .collect();
            dirs.sort();
            dirs.into_iter()
                .map(|dir| dir.join(&name))
                .find(|path| path.is_file())
        }
        UsageFormat::CodexJsonl | UsageFormat::OmpJsonl => {
            let mut found = Vec::new();
            let mut stack = vec![root.join("sessions")];
            while let Some(dir) = stack.pop() {
                let Ok(entries) = std::fs::read_dir(&dir) else {
                    continue;
                };
                for entry in entries.flatten() {
                    let path = entry.path();
                    let Ok(kind) = entry.file_type() else {
                        continue;
                    };
                    let name = entry.file_name();
                    let name = name.to_string_lossy();
                    if kind.is_dir() {
                        stack.push(path);
                    } else if name.ends_with(".jsonl") && name.contains(id) {
                        found.push(path);
                    }
                }
            }
            found.sort();
            found.pop()
        }
        UsageFormat::Unknown(_) => None,
    }
}

/// The transcript of agent session `native` under the agent's own folder (SPEC §20E.3).
/// The UI's thread view uses the same lookup as usage (ADR 0057).
pub fn transcript_path(format: &UsageFormat, native: &str) -> Option<PathBuf> {
    locate(format, &root(format)?, native)
}

/// The reader of one session's current transcript.
struct Tracker {
    format: UsageFormat,
    root: Option<PathBuf>,
    /// The native session id the reader belongs to; a new id starts a new transcript.
    native: Option<String>,
    path: Option<PathBuf>,
    reader: UsageReader,
}

impl Tracker {
    fn new(format: UsageFormat, root: Option<PathBuf>) -> Result<Self, String> {
        let reader = UsageReader::new(&format)?;
        Ok(Self {
            format,
            root,
            native: None,
            path: None,
            reader,
        })
    }

    /// Reads what was appended to the transcript of `native` and returns the totals so far;
    /// `None` until the transcript exists and has a usage line.
    fn read(&mut self, native: &str) -> Option<Usage> {
        if self.native.as_deref() != Some(native) {
            self.restart();
            self.native = Some(native.to_owned());
        }
        if self.path.is_none() {
            self.path = locate(&self.format, self.root.as_deref()?, native);
        }
        let path = self.path.clone()?;
        match self.read_appended(&path) {
            Ok(usage) => usage,
            Err(e) => {
                tracing::debug!(path = %path.display(), "reading the transcript failed: {e}");
                // Moved or deleted: look for it again at the next turn end.
                self.path = None;
                self.reader.usage()
            }
        }
    }

    /// Feeds what was appended since the reader's offset, in chunks: the first read of a long
    /// transcript would otherwise sit in memory whole.
    fn read_appended(&mut self, path: &Path) -> std::io::Result<Option<Usage>> {
        let mut file = File::open(path)?;
        if file.metadata()?.len() < self.reader.offset() {
            // Rewritten from scratch: count again.
            let path = self.path.take();
            self.restart();
            self.path = path;
        }
        file.seek(SeekFrom::Start(self.reader.offset()))?;
        let mut chunk = vec![0; 1024 * 1024];
        loop {
            match file.read(&mut chunk) {
                Ok(0) => return Ok(self.reader.usage()),
                Ok(n) => {
                    self.reader.feed(&chunk[..n]);
                }
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
                Err(e) => return Err(e),
            }
        }
    }

    /// The transcript located for the current native session, if any.
    fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    fn restart(&mut self) {
        self.path = None;
        // The format was accepted in `new`.
        if let Ok(reader) = UsageReader::new(&self.format) {
            self.reader = reader;
        }
    }
}

/// The usage task of agent session `session` and the sender of its turn ends (the native
/// session id at each end); the warning to log instead when the format is unknown.
pub fn track(
    hub: Arc<Hub>,
    session: Ulid,
    config: &UsageConfig,
) -> Result<
    (
        mpsc::UnboundedSender<String>,
        impl Future<Output = ()> + Send + 'static,
    ),
    String,
> {
    let tracker = Tracker::new(config.format.clone(), root(&config.format))?;
    let (ends, rx) = mpsc::unbounded_channel();
    Ok((ends, run(hub, session, config.context_window, tracker, rx)))
}

/// Reads the transcript of each native session id received (one per turn end) until `ends`
/// closes; changed totals are stored as a `usage` session event and broadcast.
async fn run(
    hub: Arc<Hub>,
    session: Ulid,
    context_window: Option<u64>,
    mut tracker: Tracker,
    mut ends: mpsc::UnboundedReceiver<String>,
) {
    let mut last: Option<Usage> = None;
    let mut last_path: Option<String> = None;
    while let Some(native) = ends.recv().await {
        let read = tokio::task::spawn_blocking(move || {
            let usage = tracker.read(&native);
            let warning = tracker.reader.take_warning();
            (tracker, usage, warning)
        })
        .await;
        let Ok((back, usage, warning)) = read else {
            return;
        };
        tracker = back;
        // A located (or moved) transcript is news even when the totals did not change.
        let path = tracker.path().map(|p| p.to_string_lossy().into_owned());
        if let Some(warning) = warning {
            tracing::warn!(id = %session, "token usage: {warning}");
        }
        let Some(usage) = usage.filter(|u| last.as_ref() != Some(u) || last_path != path) else {
            continue;
        };
        last = Some(usage.clone());
        last_path = path.clone();
        let event = SessionUsageEvent {
            session_id: session,
            input: usage.input,
            output: usage.output,
            cache_read: usage.cache_read,
            cache_write: usage.cache_write,
            context: usage.context,
            context_window,
            model: usage.model,
            transcript: path,
        };
        let payload = serde_json::to_value(&event).unwrap_or_default();
        let at = crate::server::now_ms();
        if let Err(e) = hub
            .store()
            .insert_session_event(session, EVENT_KIND, &payload, at)
        {
            tracing::error!(id = %session, "saving usage failed: {e}");
        }
        let _ = hub.events.send(Event::SessionUsage(event));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CLAUDE: &[u8] = include_bytes!("../../mkrew-core/tests/fixtures/usage/claude.jsonl");

    #[test]
    fn claude_transcripts_are_found_in_any_project_folder() {
        let root = tempfile::tempdir().unwrap();
        let dir = root.path().join("projects/C--work-repo");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("abc-1.jsonl"), "").unwrap();
        std::fs::write(dir.join("abc-12.jsonl"), "").unwrap();
        let found = locate(&UsageFormat::ClaudeJsonl, root.path(), "abc-1");
        assert_eq!(found, Some(dir.join("abc-1.jsonl")));
        assert_eq!(locate(&UsageFormat::ClaudeJsonl, root.path(), "abc"), None);
    }

    #[test]
    fn codex_transcripts_are_found_in_dated_folders() {
        let root = tempfile::tempdir().unwrap();
        let dir = root.path().join("sessions/2026/10/07");
        std::fs::create_dir_all(&dir).unwrap();
        let name = "rollout-2026-10-07T10-00-00-0199a-thread.jsonl";
        std::fs::write(dir.join(name), "").unwrap();
        std::fs::write(dir.join("rollout-other.jsonl"), "").unwrap();
        let found = locate(&UsageFormat::CodexJsonl, root.path(), "0199a-thread");
        assert_eq!(found, Some(dir.join(name)));
        assert_eq!(locate(&UsageFormat::CodexJsonl, root.path(), "nope"), None);
    }

    /// omp keeps `sessions/<cwd-slug>/<ts>_<id>.jsonl`; the slug is not derived, the id is
    /// searched. The sub-agent folder `<ts>_<id>/` (its files do not carry the id) is never it.
    #[test]
    fn omp_transcripts_are_found_in_slug_folders() {
        let root = tempfile::tempdir().unwrap();
        let dir = root.path().join("sessions/-Workspace-proj");
        let name = "2026-10-09T15-34-31-887Z_01a1214d-3acf.jsonl";
        let sub_agents = dir.join("2026-10-09T15-34-31-887Z_01a1214d-3acf");
        std::fs::create_dir_all(&sub_agents).unwrap();
        std::fs::write(dir.join(name), "").unwrap();
        std::fs::write(sub_agents.join("ScoutX.jsonl"), "").unwrap();
        std::fs::write(dir.join("2026-10-09T16-00-00-000Z_other.jsonl"), "").unwrap();
        let found = locate(&UsageFormat::OmpJsonl, root.path(), "01a1214d-3acf");
        assert_eq!(found, Some(dir.join(name)));
        assert_eq!(locate(&UsageFormat::OmpJsonl, root.path(), "nope"), None);
    }

    #[test]
    fn ids_that_could_leave_the_folder_find_nothing() {
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(root.path().join("projects/p")).unwrap();
        std::fs::write(root.path().join("projects/x.jsonl"), "").unwrap();
        for id in ["../x", "", "a/b", "a\\b"] {
            assert_eq!(
                locate(&UsageFormat::ClaudeJsonl, root.path(), id),
                None,
                "{id}"
            );
        }
    }

    #[test]
    fn each_read_takes_only_the_appended_bytes() {
        let root = tempfile::tempdir().unwrap();
        let dir = root.path().join("projects/p");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("s1.jsonl");
        let mut tracker =
            Tracker::new(UsageFormat::ClaudeJsonl, Some(root.path().to_owned())).unwrap();
        // No transcript yet.
        assert_eq!(tracker.read("s1"), None);

        let first = &CLAUDE[..CLAUDE.len() / 2];
        std::fs::write(&path, first).unwrap();
        let early = tracker.read("s1").unwrap();
        std::fs::write(&path, CLAUDE).unwrap();
        let all = tracker.read("s1").unwrap();
        assert!(early.output < all.output, "{early:?} {all:?}");
        let mut whole = UsageReader::new(&UsageFormat::ClaudeJsonl).unwrap();
        assert_eq!(Some(all.clone()), whole.feed(CLAUDE));
        assert_eq!(tracker.reader.offset(), CLAUDE.len() as u64);
        // Nothing new: same totals.
        assert_eq!(tracker.read("s1"), Some(all));

        // Rewritten shorter: read again from the start.
        std::fs::write(&path, first).unwrap();
        assert_eq!(tracker.read("s1"), Some(early));

        // Another native session (e.g. after `/clear`) has its own transcript.
        assert_eq!(tracker.read("s2"), None);
    }

    #[test]
    fn the_tracker_reports_the_transcript_it_located() {
        let root = tempfile::tempdir().unwrap();
        let dir = root.path().join("projects/p");
        std::fs::create_dir_all(&dir).unwrap();
        let mut tracker =
            Tracker::new(UsageFormat::ClaudeJsonl, Some(root.path().to_owned())).unwrap();
        assert_eq!(tracker.path(), None, "nothing located yet");
        std::fs::write(dir.join("s1.jsonl"), CLAUDE).unwrap();
        tracker.read("s1").unwrap();
        assert_eq!(tracker.path(), Some(dir.join("s1.jsonl").as_path()));
        // Another native session (e.g. after `/clear`) starts without a path.
        tracker.read("s2");
        assert_eq!(tracker.path(), None);
    }
}
