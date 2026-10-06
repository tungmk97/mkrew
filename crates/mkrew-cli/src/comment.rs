//! `mk comment list | reply` (SPEC §9, §12): agents read and answer review comments.
//!
//! Inside a Mkrew PTY, `MKREW_SESSION_ID` names the session: `list` uses its project and
//! `reply` is authored by its agent (the daemon moves the thread to `addressed`).

use std::path::{Path, PathBuf};

use anyhow::Context;
use mkrew_protocol::{
    CommentId, CommentListParams, CommentReplyParams, CommentStatus, DocComment, Project, Request,
    Session, SessionListParams,
};
use ulid::Ulid;

use crate::client::Client;
use crate::session::resolve_project;

const ENV_SESSION_ID: &str = "MKREW_SESSION_ID";

/// Prints the threads of `file` (or of every file), each root followed by its replies.
pub fn list(
    client: &mut Client,
    project: Option<String>,
    file: Option<String>,
) -> anyhow::Result<()> {
    let project = current_project(client, project)?;
    let file_path = match file {
        Some(f) => Some(project_relative(Path::new(&project.path), &cwd()?, &f)),
        None => None,
    };
    let comments: Vec<DocComment> = client.call(Request::CommentList(CommentListParams {
        project_id: project.id,
        file_path,
        context: None,
    }))?;
    if comments.is_empty() {
        println!("no review comments");
        return Ok(());
    }
    print!("{}", render(&comments));
    Ok(())
}

/// Replies to a thread; inside a Mkrew session the reply is the agent's.
pub fn reply(client: &mut Client, comment_id: CommentId, text: String) -> anyhow::Result<()> {
    let reply: DocComment = client.call(Request::CommentReply(CommentReplyParams {
        comment_id,
        body: text,
        mkrew_session_id: session_id()?,
    }))?;
    let thread = reply.parent_id.unwrap_or(reply.id);
    println!("replied to {thread}");
    Ok(())
}

/// `MKREW_SESSION_ID`, if set.
fn session_id() -> anyhow::Result<Option<Ulid>> {
    match std::env::var(ENV_SESSION_ID) {
        Ok(id) if !id.is_empty() => id
            .parse()
            .map(Some)
            .with_context(|| format!("{ENV_SESSION_ID}={id:?} is not a session id")),
        _ => Ok(None),
    }
}

/// `--project`, else the project of `MKREW_SESSION_ID`, else the project containing the
/// current directory.
fn current_project(client: &mut Client, key: Option<String>) -> anyhow::Result<Project> {
    let projects: Vec<Project> = client.call(Request::ProjectList)?;
    if let Some(key) = key {
        return resolve_project(&projects, &key);
    }
    if let Some(id) = session_id()? {
        let sessions: Vec<Session> =
            client.call(Request::SessionList(SessionListParams::default()))?;
        let session = sessions
            .iter()
            .find(|s| s.id == id)
            .with_context(|| format!("no session with id {id} ({ENV_SESSION_ID})"))?;
        return projects
            .into_iter()
            .find(|p| p.id == session.project_id)
            .with_context(|| format!("session {id} has no project"));
    }
    let cwd = cwd()?;
    projects
        .into_iter()
        .filter(|p| cwd.starts_with(&p.path))
        .max_by_key(|p| p.path.len())
        .context("the current directory is in no project; pass --project")
}

/// The current directory, canonical like project paths.
fn cwd() -> anyhow::Result<PathBuf> {
    std::env::current_dir()
        .and_then(dunce::canonicalize)
        .context("read the current directory")
}

/// `file` as given relative to the project root (`/` separators): resolved against `cwd`
/// when that lands inside the project, else taken as project-relative already.
fn project_relative(root: &Path, cwd: &Path, file: &str) -> String {
    let full = cwd.join(file);
    let relative = full
        .strip_prefix(root)
        .map_or_else(|_| file.to_owned(), |r| r.to_string_lossy().into_owned());
    relative.replace('\\', "/")
}

fn render(comments: &[DocComment]) -> String {
    let mut out = String::new();
    for root in comments.iter().filter(|c| c.parent_id.is_none()) {
        let lines = match (root.start_line, root.end_line) {
            (Some(s), Some(e)) if s == e => format!("line {s}"),
            (Some(s), Some(e)) => format!("lines {s}–{e}"),
            _ => "no lines".to_owned(),
        };
        out.push_str(&format!(
            "[{}] {} {}, {}",
            root.id,
            status_name(root.status),
            root.file_path,
            lines
        ));
        if let Some(quote) = &root.quote {
            out.push_str(&format!(", passage {quote:?}"));
        }
        out.push('\n');
        let replies = comments.iter().filter(|c| c.parent_id == Some(root.id));
        for comment in std::iter::once(root).chain(replies) {
            out.push_str(&format!("  {}: {}\n", comment.author, comment.body));
        }
    }
    out
}

/// The wire name (`addressed`, …).
fn status_name(status: CommentStatus) -> String {
    match serde_json::to_value(status) {
        Ok(serde_json::Value::String(name)) => name,
        _ => format!("{status:?}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn comment(id: CommentId, parent: Option<CommentId>, author: &str, body: &str) -> DocComment {
        DocComment {
            id,
            project_id: Ulid::nil(),
            file_path: "docs/plan.md".into(),
            context: Default::default(),
            parent_id: parent,
            author: author.into(),
            body: body.into(),
            start_line: parent.is_none().then_some(12),
            end_line: parent.is_none().then_some(14),
            quote: parent.is_none().then(|| "Use Redis".into()),
            status: CommentStatus::Addressed,
            sent_to: None,
            sent_snapshot: None,
            created_at: 0,
            updated_at: 0,
        }
    }

    #[test]
    fn threads_render_root_then_replies() {
        let root = CommentId(Ulid::from_parts(1, 1));
        let comments = [
            comment(root, None, "user", "Why Redis?"),
            comment(
                CommentId(Ulid::from_parts(2, 2)),
                Some(root),
                "claude",
                "Dropped it",
            ),
        ];
        assert_eq!(
            render(&comments),
            format!(
                "[{root}] addressed docs/plan.md, lines 12–14, passage \"Use Redis\"\n\
                 \x20 user: Why Redis?\n  claude: Dropped it\n"
            )
        );
    }

    #[test]
    fn files_are_made_project_relative() {
        let root = Path::new("/work/api");
        assert_eq!(
            project_relative(root, Path::new("/work/api/docs"), "plan.md"),
            "docs/plan.md"
        );
        assert_eq!(
            project_relative(root, Path::new("/elsewhere"), "docs/plan.md"),
            "docs/plan.md"
        );
        assert_eq!(
            project_relative(root, Path::new("/work/api"), r"docs\plan.md"),
            "docs/plan.md"
        );
    }
}
