//! `mkrew://` links (ADR 0078). A Windows toast opens `mkrew://session/<session>?project=<project>`;
//! `mkrew.exe` started with it shows that session's pane (in the running UI, or a new one).

use ulid::Ulid;

/// The URL scheme registered for Mkrew on Windows.
pub const SCHEME: &str = "mkrew";

/// The link a toast of `session` (in workspace `project`) opens.
pub fn session_link(session: Ulid, project: Ulid) -> String {
    format!("{SCHEME}://session/{session}?project={project}")
}

/// `(session, project)` of a [`session_link`]; `None` for anything else. The scheme is
/// case-insensitive and one `/` may follow the session id (Windows may pass either);
/// surrounding whitespace is ignored.
pub fn parse_session_link(link: &str) -> Option<(Ulid, Ulid)> {
    let (scheme, rest) = link.trim().split_once("://")?;
    if !scheme.eq_ignore_ascii_case(SCHEME) {
        return None;
    }
    let (session, query) = rest.strip_prefix("session/")?.split_once('?')?;
    let session = session.strip_suffix('/').unwrap_or(session);
    let project = query.strip_prefix("project=")?;
    Some((
        Ulid::from_string(session).ok()?,
        Ulid::from_string(project).ok()?,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_session_link_round_trips() {
        let (session, project) = (Ulid::new(), Ulid::new());
        let link = session_link(session, project);
        assert_eq!(link, format!("mkrew://session/{session}?project={project}"));
        assert_eq!(parse_session_link(&link), Some((session, project)));
    }

    #[test]
    fn the_forms_windows_may_pass_are_accepted() {
        let (session, project) = (Ulid::new(), Ulid::new());
        for link in [
            format!("MKREW://session/{session}?project={project}"),
            format!("mkrew://session/{session}/?project={project}"),
            format!("  mkrew://session/{session}?project={project} "),
        ] {
            assert_eq!(
                parse_session_link(&link),
                Some((session, project)),
                "{link}"
            );
        }
    }

    #[test]
    fn anything_else_is_rejected() {
        let (session, project) = (Ulid::new(), Ulid::new());
        for link in [
            String::new(),
            "mkrew://".into(),
            format!("http://session/{session}?project={project}"),
            format!("mkrew://task/{session}?project={project}"),
            format!("mkrew://session/{session}"),
            format!("mkrew://session/{session}?workspace={project}"),
            format!("mkrew://session/{session}/x?project={project}"),
            format!("mkrew://session/{session}?project={project}&x=1"),
            format!("mkrew://session/not-a-ulid?project={project}"),
            format!("mkrew://session/{session}?project=zz"),
        ] {
            assert_eq!(parse_session_link(&link), None, "{link}");
        }
    }
}
