//! `mk task add | list | run | undo` (SPEC §9).

use mkrew_protocol::{
    Project, Request, SessionSpawnResult, Task, TaskCreateParams, TaskListParams, TaskRef,
    TaskRunParams, TaskStatus, TaskUndoParams, UndoPreview,
};
use ulid::Ulid;

use crate::client::Client;
use crate::session::{DEFAULT_SIZE, resolve_project};

/// Creates a Todo card and prints its id (alone, so scripts can capture it).
pub fn add(
    client: &mut Client,
    project: &str,
    title: String,
    prompt: Option<String>,
    agent: Option<String>,
) -> anyhow::Result<()> {
    let projects: Vec<Project> = client.call(Request::ProjectList)?;
    let project = resolve_project(&projects, project)?;
    let task: Task = client.call(Request::TaskCreate(TaskCreateParams {
        project_id: project.id,
        title,
        prompt: prompt.unwrap_or_default(),
        agent_profile: agent,
        position: None,
        worktree: false,
    }))?;
    println!("{}", task.id);
    Ok(())
}

pub fn list(
    client: &mut Client,
    project: Option<String>,
    status: Option<TaskStatus>,
) -> anyhow::Result<()> {
    let projects: Vec<Project> = client.call(Request::ProjectList)?;
    let project_id = match project {
        Some(key) => Some(resolve_project(&projects, &key)?.id),
        None => None,
    };
    let tasks: Vec<Task> = client.call(Request::TaskList(TaskListParams { project_id, status }))?;
    if tasks.is_empty() {
        println!("no tasks; add one with `mk task add <project> \"<title>\"`");
        return Ok(());
    }
    let project_name = |t: &Task| {
        projects
            .iter()
            .find(|p| p.id == t.project_id)
            .map_or_else(|| t.project_id.to_string(), |p| p.name.clone())
    };
    println!(
        "{:<26}  {:<11}  {:<10}  {:<12}  TITLE",
        "ID", "STATUS", "AGENT", "PROJECT"
    );
    for task in &tasks {
        // `!` = the last run exited non-zero.
        let mut status = status_name(task.status);
        if task.error_flag {
            status.push('!');
        }
        println!(
            "{:<26}  {:<11}  {:<10}  {:<12}  {}",
            task.id,
            status,
            task.agent_profile.as_deref().unwrap_or("(default)"),
            project_name(task),
            task.title
        );
    }
    Ok(())
}

/// Runs the task's agent and prints the new session id (alone, so scripts can capture it).
pub fn run(client: &mut Client, task_id: Ulid) -> anyhow::Result<()> {
    let (cols, rows) = crate::attach::terminal_size().unwrap_or(DEFAULT_SIZE);
    let spawned: SessionSpawnResult = client.call(Request::TaskRun(TaskRunParams {
        task_id,
        cols,
        rows,
    }))?;
    println!("{}", spawned.session_id);
    Ok(())
}

/// Prints what undoing the task discards; only `yes` actually discards it (ADR 0005).
pub fn undo(client: &mut Client, task_id: Ulid, yes: bool) -> anyhow::Result<()> {
    let preview: UndoPreview = client.call(Request::TaskUndoPreview(TaskRef { task_id }))?;
    print!("{}", render_preview(&preview));
    if is_empty(&preview) {
        return Ok(());
    }
    if !yes {
        println!("nothing discarded; run again with --yes to discard these changes");
        return Ok(());
    }
    let checkpoint = short(&preview.checkpoint).to_owned();
    client.call::<()>(Request::TaskUndo(TaskUndoParams {
        task_id,
        confirm: true,
        created: preview.created,
        fingerprint: preview.fingerprint,
    }))?;
    println!("undone: files restored to {checkpoint}");
    Ok(())
}

fn is_empty(preview: &UndoPreview) -> bool {
    preview.changed.is_empty() && preview.added.is_empty() && preview.created.is_empty()
}

fn render_preview(preview: &UndoPreview) -> String {
    let checkpoint = short(&preview.checkpoint);
    if is_empty(preview) {
        return format!("no changes since checkpoint {checkpoint}\n");
    }
    let mut out = format!("changes since checkpoint {checkpoint}:\n");
    for path in &preview.changed {
        out.push_str(&format!("  restore  {path}\n"));
    }
    for path in preview.added.iter().chain(&preview.created) {
        out.push_str(&format!("  delete   {path}\n"));
    }
    if !preview.diff.is_empty() {
        out.push('\n');
        out.push_str(&preview.diff);
        if !preview.diff.ends_with('\n') {
            out.push('\n');
        }
        if preview.diff_truncated {
            out.push_str("… diff cut (too large)\n");
        }
    }
    out
}

fn short(commit: &str) -> &str {
    commit.get(..12).unwrap_or(commit)
}

/// Parses a wire status name (`needs_input`, …) for `--status`.
pub fn parse_status(name: &str) -> Result<TaskStatus, String> {
    serde_json::from_value(serde_json::Value::String(name.to_owned())).map_err(|_| {
        format!("unknown status {name:?}; expected todo, running, needs_input, review or done")
    })
}

/// The wire name (`needs_input`, …).
fn status_name(status: TaskStatus) -> String {
    match serde_json::to_value(status) {
        Ok(serde_json::Value::String(name)) => name,
        _ => format!("{status:?}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn statuses_parse_by_wire_name() {
        assert_eq!(parse_status("needs_input"), Ok(TaskStatus::NeedsInput));
        assert_eq!(status_name(TaskStatus::NeedsInput), "needs_input");
        assert!(
            parse_status("NeedsInput")
                .unwrap_err()
                .contains("expected todo")
        );
    }

    #[test]
    fn undo_preview_lists_files_then_the_diff() {
        let mut preview = UndoPreview {
            checkpoint: "0123456789abcdef0123456789abcdef01234567".into(),
            fingerprint: String::new(),
            changed: vec![],
            added: vec![],
            created: vec![],
            diff: String::new(),
            diff_truncated: false,
        };
        assert_eq!(
            render_preview(&preview),
            "no changes since checkpoint 0123456789ab\n"
        );
        preview.changed = vec!["src/a.rs".into()];
        preview.added = vec!["staged.txt".into()];
        preview.created = vec!["new.txt".into()];
        preview.diff = "-old\n+new".into();
        preview.diff_truncated = true;
        assert_eq!(
            render_preview(&preview),
            "changes since checkpoint 0123456789ab:\n  restore  src/a.rs\n  delete   staged.txt\n\
             \x20 delete   new.txt\n\
             \n-old\n+new\n… diff cut (too large)\n"
        );
    }
}
