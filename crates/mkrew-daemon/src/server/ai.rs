//! `ai.generate` (SPEC §6 `oneshot`, §12B, §20B.1; ADR 0016, 0061): one-shot text from the
//! user's own agent CLI. Mkrew never calls an LLM API: it runs the profile's non-interactive
//! command in the project folder (`ask`: the scratch folder, outside every project) with the
//! prompt on stdin and broadcasts its stdout as `ai.generated`.

use std::borrow::Cow;
use std::path::Path;
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;

use mkrew_protocol::{AiGenerateParams, AiGeneratedEvent, AiKind, ErrorCode, Event, Outcome};
use tokio::io::AsyncWriteExt;

use super::{Daemon, ok};
use crate::git::{GitBackend, Side};
use crate::launch;

/// Longest an agent may take to answer.
const TIMEOUT: Duration = Duration::from_secs(120);
/// Diff bytes sent to the agent; lockfiles and generated code can be huge.
// ponytail: plain byte cap; summarize per file if long diffs give poor messages.
const MAX_DIFF_BYTES: usize = 64 * 1024;

impl Daemon {
    /// `ai.generate`: checks what it can right away, then runs the agent in the background (this
    /// connection keeps serving requests) and broadcasts `ai.generated`.
    pub(super) async fn ai_generate(&self, params: AiGenerateParams) -> Outcome {
        let ask = params.kind == AiKind::Ask;
        if params.kind != AiKind::CommitMessage && params.input.trim().is_empty() {
            let message = if ask {
                "ai.generate needs a question"
            } else {
                "ai.generate needs a prompt to improve"
            };
            return Outcome::error(ErrorCode::InvalidRequest, message);
        }
        let (profile_id, dir) = if ask {
            let Some(profile_id) = params.agent_profile.clone() else {
                let message = "ai.generate ask needs an agent_profile";
                return Outcome::error(ErrorCode::InvalidRequest, message);
            };
            let dir = self.paths.scratch_dir();
            if let Err(error) = std::fs::create_dir_all(&dir) {
                let message = format!("cannot create {}: {error}", dir.display());
                return Outcome::error(ErrorCode::Internal, message);
            }
            (profile_id, dir)
        } else {
            let Some(project_id) = params.project_id else {
                let message = "ai.generate needs a project_id";
                return Outcome::error(ErrorCode::InvalidRequest, message);
            };
            let target = match self
                .target(project_id, params.worktree_task_id, params.repo.as_deref())
                .await
            {
                Ok(target) => target,
                Err(outcome) => return outcome,
            };
            let Some(profile_id) = params
                .agent_profile
                .clone()
                .or_else(|| target.project.default_agent.clone())
            else {
                let message = format!("set a default agent for {} first", target.project.name);
                return Outcome::error(ErrorCode::InvalidState, message);
            };
            (profile_id, target.dir)
        };
        let profile = match self.load_profile(&profile_id) {
            Ok(profile) => profile,
            Err(outcome) => return outcome,
        };
        let (runs, key, example) = match (ask, params.web) {
            (false, _) => (profile.oneshot_command(), "oneshot", "claude -p"),
            (true, false) => (profile.ask_command(false), "ask", "claude -p"),
            (true, true) => (
                profile.ask_command(true),
                "ask_web",
                "claude -p --tools WebSearch",
            ),
        };
        let Some(runs) = runs else {
            let file = self.paths.agents_dir().join(format!("{profile_id}.toml"));
            let message = format!(
                "agent profile {profile_id} has no `{key}` command: add a line like \
                 `{key} = \"{example}\"` to {}",
                file.display()
            );
            return Outcome::error(ErrorCode::InvalidState, message);
        };
        let builder = match launch::command(&runs) {
            Ok(builder) => builder,
            Err(error) => return Outcome::error(ErrorCode::InvalidState, error.to_string()),
        };
        let Some(command) = child_command(&builder, &dir) else {
            let message = format!("agent profile {profile_id} has an empty `{key}` command");
            return Outcome::error(ErrorCode::InvalidState, message);
        };
        let (hub, git) = (Arc::clone(&self.hub), Arc::clone(&self.git));
        let (generation_id, project_id) = (params.generation_id, params.project_id);
        tokio::spawn(async move {
            let (kind, input) = (params.kind, params.input);
            let prompt = tokio::task::spawn_blocking(move || prompt(&*git, &dir, kind, input))
                .await
                .map_err(|error| format!("cannot build the prompt: {error}"))
                .and_then(|prompt| prompt);
            let result = match prompt {
                Ok(prompt) => run(command, &prompt, TIMEOUT).await,
                Err(error) => Err(error),
            }
            .and_then(|stdout| {
                // An answer is shown as Markdown: its code blocks stay.
                clean_output(&stdout, !ask).ok_or_else(|| "the agent printed nothing".to_owned())
            });
            if let Err(error) = &result {
                tracing::info!(%generation_id, "ai.generate failed: {error}");
            }
            let (text, error) = match result {
                Ok(text) => (Some(text), None),
                Err(error) => (None, Some(error)),
            };
            let event = AiGeneratedEvent {
                generation_id,
                project_id,
                text,
                error,
            };
            let _ = hub.events.send(Event::AiGenerated(event));
        });
        ok(&())
    }
}

/// The agent's program as a plain child process (no PTY) in `dir`, pipes for all three
/// streams, no console window. `None` if the command line is empty.
fn child_command(
    builder: &portable_pty::CommandBuilder,
    dir: &Path,
) -> Option<std::process::Command> {
    let (program, args) = builder.get_argv().split_first()?;
    let mut command = std::process::Command::new(program);
    command
        .args(args)
        .envs(builder.iter_extra_env_as_str())
        .current_dir(dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // The daemon has no console; without this the agent would open one.
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    Some(command)
}

/// What the agent is asked. Runs git, so call it off the async threads.
fn prompt(git: &dyn GitBackend, dir: &Path, kind: AiKind, input: String) -> Result<String, String> {
    let failed = |error: std::io::Error| format!("git failed: {error}");
    match kind {
        AiKind::Ask => Ok(input),
        AiKind::EnhancePrompt => Ok(enhance_prompt(&input)),
        AiKind::CommitMessage => {
            let staged = git.file_diff(dir, ".", Side::Staged).map_err(failed)?;
            if !staged.trim().is_empty() {
                return commit_prompt_from(&staged, "", &[]);
            }
            let unstaged = git.file_diff(dir, ".", Side::Unstaged).map_err(failed)?;
            let untracked = git.untracked_files(dir).map_err(failed)?;
            commit_prompt_from("", &unstaged, &untracked)
        }
    }
}

/// The commit-message prompt for the staged diff, else the unstaged diff plus new files.
fn commit_prompt_from(
    staged: &str,
    unstaged: &str,
    untracked: &[String],
) -> Result<String, String> {
    let diff = if staged.trim().is_empty() {
        let mut diff = unstaged.to_owned();
        for path in untracked {
            diff.push_str(&format!("new file: {path}\n"));
        }
        diff
    } else {
        staged.to_owned()
    };
    if diff.trim().is_empty() {
        return Err("No changes to describe.".to_owned());
    }
    Ok(format!(
        "Write a git commit message for the changes below, following Conventional Commits: a \
         first line `type(scope): summary` (type is one of feat, fix, docs, style, refactor, \
         perf, test, build, ci, chore; scope optional; summary in the imperative mood, at most \
         72 characters, no final period), then, only if it helps explain why, a blank line and \
         a short body wrapped at 72 characters. Reply with the message only: no quotes, no code \
         fence, no comments. Do not run tools or change files.\n\n{}",
        truncated(&diff, MAX_DIFF_BYTES)
    ))
}

fn enhance_prompt(input: &str) -> String {
    format!(
        "Rewrite the prompt below so a coding agent can act on it well. Keep the author's \
         intent and language. Make the goal, constraints and expected result explicit, and name \
         the relevant files, functions or commands. You may read files in the current directory \
         to get these right, but do not change any file and do not run commands that change \
         anything. Reply with the improved prompt only: no preamble, no code fence.\n\n{input}"
    )
}

/// `text` cut to at most `max` bytes on a char boundary, with a note when cut.
fn truncated(text: &str, max: usize) -> Cow<'_, str> {
    if text.len() <= max {
        return Cow::Borrowed(text);
    }
    let mut end = max;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    Cow::Owned(format!("{}\n[diff truncated]\n", &text[..end]))
}

/// The answer without blank edges and, if `unfence`, a surrounding ``` fence; `None` if
/// nothing is left.
fn clean_output(stdout: &str, unfence: bool) -> Option<String> {
    let text = stdout.trim();
    let text = text
        .strip_prefix("```")
        .filter(|_| unfence)
        .and_then(|rest| rest.split_once('\n'))
        .and_then(|(_, body)| body.trim_end().strip_suffix("```"))
        .map_or(text, str::trim);
    (!text.is_empty()).then(|| text.to_owned())
}

/// Runs `command` with `prompt` on stdin; its stdout is the answer. Killed after `timeout`.
async fn run(
    command: std::process::Command,
    prompt: &str,
    timeout: Duration,
) -> Result<String, String> {
    let mut command = tokio::process::Command::from(command);
    command.kill_on_drop(true);
    let mut child = command
        .spawn()
        .map_err(|error| format!("cannot start the agent: {error}"))?;
    if let Some(mut stdin) = child.stdin.take() {
        let prompt = prompt.to_owned();
        // Dropping `stdin` after the write sends EOF. An agent that exits without reading
        // everything breaks the pipe; that is not an error.
        tokio::spawn(async move {
            let _ = stdin.write_all(prompt.as_bytes()).await;
        });
    }
    let output = tokio::time::timeout(timeout, child.wait_with_output())
        .await
        .map_err(|_| format!("the agent did not answer within {} s", timeout.as_secs()))?
        .map_err(|error| format!("the agent failed: {error}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let last: Vec<&str> = stderr.trim().lines().rev().take(3).collect();
        let last: Vec<&str> = last.into_iter().rev().collect();
        return Err(format!(
            "the agent exited with {}: {}",
            output.status,
            last.join(" / ")
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    #[test]
    fn long_diffs_are_cut_on_a_char_boundary_with_a_note() {
        let diff = "é".repeat(10); // 20 bytes
        let cut = truncated(&diff, 5);
        assert!(cut.starts_with("éé") && !cut.starts_with("ééé"), "{cut}");
        assert!(cut.ends_with("[diff truncated]\n"));
        assert_eq!(truncated("short", 5), "short");
    }

    #[test]
    fn answers_lose_blank_edges_and_a_code_fence() {
        let fenced = "\n```text\nfeat: x\n\nbody\n```\n";
        assert_eq!(
            clean_output(fenced, true).as_deref(),
            Some("feat: x\n\nbody")
        );
        // `ask`: a whole-answer code block is the answer.
        assert_eq!(
            clean_output(fenced, false).as_deref(),
            Some("```text\nfeat: x\n\nbody\n```")
        );
        assert_eq!(
            clean_output("  fix: y  \n", true).as_deref(),
            Some("fix: y")
        );
        assert_eq!(clean_output("```\n```", true), None);
        assert_eq!(clean_output(" \n ", false), None);
    }

    #[test]
    fn commit_prompt_falls_back_to_unstaged_and_new_files() {
        let prompt = commit_prompt_from("", "-old\n+new\n", &["n.txt".into()]).unwrap();
        assert!(
            prompt.contains("+new") && prompt.contains("new file: n.txt"),
            "{prompt}"
        );
        let staged = commit_prompt_from("+staged\n", "+unstaged\n", &[]).unwrap();
        assert!(staged.contains("+staged") && !staged.contains("+unstaged"));
        assert_eq!(
            commit_prompt_from(" ", "", &[]),
            Err("No changes to describe.".to_owned())
        );
    }

    /// An agent that never answers is stopped; the user gets a reason, not a spinner forever.
    #[tokio::test]
    async fn run_stops_an_agent_that_does_not_answer() {
        let mut sleeper = if cfg!(windows) {
            let mut c = std::process::Command::new("powershell");
            c.args(["-NoProfile", "-Command", "Start-Sleep -Seconds 30"]);
            c
        } else {
            let mut c = std::process::Command::new("sleep");
            c.arg("30");
            c
        };
        sleeper
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());
        let started = std::time::Instant::now();
        let error = run(sleeper, "hi", Duration::from_millis(300))
            .await
            .unwrap_err();
        assert!(error.contains("did not answer"), "{error}");
        assert!(started.elapsed() < Duration::from_secs(10));
    }
}
