//! `mkrew://` links (ADR 0079). A Windows toast opens `mkrew://session/<session>?project=<project>`;
//! `mkrew.exe` started with it shows that session's pane (in the running UI, or a new one).

use ulid::Ulid;

/// The URL scheme registered for Mkrew on Windows.
pub const SCHEME: &str = "mkrew";

/// A parsed [`session_link`]. `machine` names the machine the session runs on (None: this computer).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionLink {
    pub session: Ulid,
    pub project: Ulid,
    pub machine: Option<String>,
}

/// The link a toast of `session` (in workspace `project`, on `machine`) opens.
pub fn session_link(session: Ulid, project: Ulid, machine: Option<&str>) -> String {
    let mut link = format!("{SCHEME}://session/{session}?project={project}");
    if let Some(machine) = machine {
        link.push_str("&machine=");
        for byte in machine.bytes() {
            if byte.is_ascii_alphanumeric() || b"-._~".contains(&byte) {
                link.push(byte as char);
            } else {
                link.push_str(&format!("%{byte:02X}"));
            }
        }
    }
    link
}

/// The [`SessionLink`] in `link`; `None` for anything else. The scheme is
/// case-insensitive and one `/` may follow the session id (Windows may pass either);
/// surrounding whitespace is ignored. Only `machine` may follow `project`.
pub fn parse_session_link(link: &str) -> Option<SessionLink> {
    let (scheme, rest) = link.trim().split_once("://")?;
    if !scheme.eq_ignore_ascii_case(SCHEME) {
        return None;
    }
    let (session, query) = rest.strip_prefix("session/")?.split_once('?')?;
    let session = session.strip_suffix('/').unwrap_or(session);
    let query = query.strip_prefix("project=")?;
    let (project, machine) = match query.split_once("&machine=") {
        Some((project, machine)) => (project, Some(decode(machine)?)),
        None => (query, None),
    };
    Some(SessionLink {
        session: Ulid::from_string(session).ok()?,
        project: Ulid::from_string(project).ok()?,
        machine,
    })
}

/// Strict percent-decoding: a raw `&`, a bad `%`, non-UTF-8 bytes or an empty result give `None`.
fn decode(text: &str) -> Option<String> {
    let mut bytes = Vec::with_capacity(text.len());
    let mut input = text.bytes();
    while let Some(byte) = input.next() {
        match byte {
            b'&' => return None,
            b'%' => {
                let hex = [input.next()?, input.next()?];
                bytes.push(u8::from_str_radix(std::str::from_utf8(&hex).ok()?, 16).ok()?);
            }
            _ => bytes.push(byte),
        }
    }
    String::from_utf8(bytes)
        .ok()
        .filter(|text| !text.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_session_link_round_trips() {
        let (session, project) = (Ulid::new(), Ulid::new());
        let link = session_link(session, project, None);
        assert_eq!(link, format!("mkrew://session/{session}?project={project}"));
        let parsed = parse_session_link(&link).unwrap();
        assert_eq!(
            (parsed.session, parsed.project, parsed.machine),
            (session, project, None)
        );
    }

    #[test]
    fn a_machine_link_round_trips_and_old_links_have_no_machine() {
        let (session, project) = (Ulid::new(), Ulid::new());
        let link = session_link(session, project, Some("Home PC & co"));
        assert_eq!(
            link,
            format!("mkrew://session/{session}?project={project}&machine=Home%20PC%20%26%20co")
        );
        let parsed = parse_session_link(&link).unwrap();
        assert_eq!((parsed.session, parsed.project), (session, project));
        assert_eq!(parsed.machine.as_deref(), Some("Home PC & co"));
        let old =
            parse_session_link(&format!("mkrew://session/{session}?project={project}")).unwrap();
        assert_eq!(old.machine, None);
        assert!(
            parse_session_link(&format!(
                "mkrew://session/{session}?project={project}&machine="
            ))
            .is_none()
        );
        assert!(
            parse_session_link(&format!(
                "mkrew://session/{session}?project={project}&machine=%zz"
            ))
            .is_none()
        );
    }

    #[test]
    fn the_forms_windows_may_pass_are_accepted() {
        let (session, project) = (Ulid::new(), Ulid::new());
        for link in [
            format!("MKREW://session/{session}?project={project}"),
            format!("mkrew://session/{session}/?project={project}"),
            format!("  mkrew://session/{session}?project={project} "),
        ] {
            let parsed = parse_session_link(&link).unwrap();
            assert_eq!(
                (parsed.session, parsed.project),
                (session, project),
                "{link}"
            );
            assert_eq!(parsed.machine, None, "{link}");
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
            format!("mkrew://session/{session}?project={project}&machine=a&x=1"),
            format!("mkrew://session/not-a-ulid?project={project}"),
            format!("mkrew://session/{session}?project=zz"),
        ] {
            assert_eq!(parse_session_link(&link), None, "{link}");
        }
    }
}
