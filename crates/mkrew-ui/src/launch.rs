//! Which window a start of `mkrew` opens (spec 2026-10-29 §4.5): this computer's, or the one
//! of a machine from `machines.toml`.

use mkrew_core::link::parse_session_link;
use ulid::Ulid;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Launch {
    /// This computer's window, maybe jumping to a session.
    Local { jump: Option<(Ulid, Ulid)> },
    /// The window of machine `name`.
    Machine {
        name: String,
        jump: Option<(Ulid, Ulid)>,
    },
}

/// `args` without the program name: `[]`, `[<mkrew:// link>]`, `["--machine", name]`,
/// `["--machine", name, <link>]`; a link with `machine=` means that machine. Anything else is
/// ignored.
pub fn parse(args: &[String]) -> Launch {
    let mut machine = None;
    let mut link = None;
    let mut args = args.iter();
    while let Some(arg) = args.next() {
        if arg == "--machine" {
            machine = args.next().cloned();
        } else if link.is_none() {
            link = parse_session_link(arg);
        }
    }
    let jump = link.as_ref().map(|link| (link.session, link.project));
    match machine.or_else(|| link.and_then(|link| link.machine)) {
        Some(name) => Launch::Machine { name, jump },
        None => Launch::Local { jump },
    }
}

/// The single-instance key input for a launch: `machine:<name>` (lower case, like machine
/// names are compared) or, for this computer, the daemon socket.
pub fn instance_id(launch: &Launch, socket: &str) -> String {
    match launch {
        Launch::Local { .. } => socket.to_owned(),
        Launch::Machine { name, .. } => format!("machine:{}", name.to_lowercase()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arguments_pick_the_window() {
        let s = |v: &[&str]| v.iter().map(|a| a.to_string()).collect::<Vec<_>>();
        let (session, project) = (Ulid::new(), Ulid::new());
        assert_eq!(parse(&s(&[])), Launch::Local { jump: None });
        let local_link = mkrew_core::link::session_link(session, project, None);
        assert_eq!(
            parse(&s(&[&local_link])),
            Launch::Local {
                jump: Some((session, project))
            }
        );
        assert_eq!(
            parse(&s(&["--machine", "Home PC"])),
            Launch::Machine {
                name: "Home PC".into(),
                jump: None
            }
        );
        let machine_link = mkrew_core::link::session_link(session, project, Some("Home PC"));
        assert_eq!(
            parse(&s(&[&machine_link])),
            Launch::Machine {
                name: "Home PC".into(),
                jump: Some((session, project))
            }
        );
        assert_eq!(
            parse(&s(&["--machine", "Home PC", &local_link])),
            Launch::Machine {
                name: "Home PC".into(),
                jump: Some((session, project))
            }
        );
        // Garbage is ignored as before (the old code ignored a non-link argument).
        assert_eq!(parse(&s(&["whatever"])), Launch::Local { jump: None });
        assert_eq!(parse(&s(&["--machine"])), Launch::Local { jump: None });
    }

    #[test]
    fn instance_ids_differ_per_machine_and_keep_the_socket_for_this_computer() {
        let local = Launch::Local { jump: None };
        let home = Launch::Machine {
            name: "Home PC".into(),
            jump: None,
        };
        assert_eq!(
            instance_id(&local, r"\\.\pipe\mkrew-me"),
            r"\\.\pipe\mkrew-me"
        );
        assert_eq!(instance_id(&home, "x"), "machine:home pc");
    }
}
