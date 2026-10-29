//! The machines a window can open (spec 2026-10-29 §4): `<config>/mkrew/machines.toml`,
//! read and written by the UI only, the ssh command for one machine, advice for ssh's usual
//! failures, and the state lines a machine window sends to the window that started it.

use std::path::Path;

use serde::{Deserialize, Serialize};

pub const MACHINES_FILE: &str = "machines.toml";
/// The name of this computer's window; no machine may take it.
pub const THIS_COMPUTER: &str = "This computer";
const MAX_NAME: usize = 40;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Machine {
    pub name: String,
    pub host: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mk: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Machines {
    pub list: Vec<Machine>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Field {
    Name,
    Host,
    Mk,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Invalid {
    pub field: Field,
    pub message: String,
}

#[derive(Default, Serialize, Deserialize)]
struct File {
    #[serde(default, rename = "machine")]
    machine: Vec<Machine>,
}

impl Machines {
    /// `Ok(empty)` when the file is missing; `Err(text with file and line)` when broken.
    pub fn load(config_dir: &Path) -> Result<Self, String> {
        let path = config_dir.join(MACHINES_FILE);
        let text = match std::fs::read_to_string(&path) {
            Ok(text) => text,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Self::default()),
            Err(e) => return Err(format!("{}: {e}", path.display())),
        };
        // `toml`'s error names the line and column.
        let file: File = toml::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))?;
        let mut machines = Self::default();
        for machine in file.machine {
            let name = machine.name.clone();
            machines
                .add(machine)
                .map_err(|e| format!("{}: machine {name:?}: {}", path.display(), e.message))?;
        }
        Ok(machines)
    }

    pub fn save(&self, config_dir: &Path) -> std::io::Result<()> {
        std::fs::create_dir_all(config_dir)?;
        let text = toml::to_string(&File {
            machine: self.list.clone(),
        })
        .map_err(std::io::Error::other)?;
        std::fs::write(config_dir.join(MACHINES_FILE), text)
    }

    /// Trims, validates against the others, appends.
    pub fn add(&mut self, machine: Machine) -> Result<(), Invalid> {
        let name = machine.name.trim().to_owned();
        let host = machine.host.trim().to_owned();
        let mk = machine
            .mk
            .map(|mk| mk.trim().to_owned())
            .filter(|mk| !mk.is_empty());
        let invalid = |field, message: &str| {
            Err(Invalid {
                field,
                message: message.into(),
            })
        };
        if name.is_empty() {
            return invalid(Field::Name, "Enter a name.");
        }
        if name.chars().count() > MAX_NAME {
            return invalid(Field::Name, "Use at most 40 characters.");
        }
        if name.eq_ignore_ascii_case(THIS_COMPUTER) || self.get(&name).is_some() {
            return invalid(Field::Name, "That name is already used.");
        }
        if host.is_empty() {
            return invalid(
                Field::Host,
                "Enter the SSH host (name, user@host or a ~/.ssh/config alias).",
            );
        }
        if host.starts_with('-') || host.chars().any(char::is_whitespace) {
            return invalid(
                Field::Host,
                "A host has no spaces and does not start with \"-\".",
            );
        }
        if mk.as_deref().is_some_and(|mk| mk.starts_with('-')) {
            return invalid(Field::Mk, "A path does not start with \"-\".");
        }
        self.list.push(Machine { name, host, mk });
        Ok(())
    }

    pub fn remove(&mut self, name: &str) -> bool {
        let before = self.list.len();
        self.list.retain(|m| !m.name.eq_ignore_ascii_case(name));
        self.list.len() != before
    }

    pub fn get(&self, name: &str) -> Option<&Machine> {
        self.list.iter().find(|m| m.name.eq_ignore_ascii_case(name))
    }
}

/// `ssh` and its arguments for `machine` (spec §4.3).
pub fn ssh_command(machine: &Machine) -> (String, Vec<String>) {
    let mut args: Vec<String> = [
        "-T",
        "-o",
        "ConnectTimeout=10",
        "-o",
        "BatchMode=yes",
        "-o",
        "StrictHostKeyChecking=accept-new",
        "-o",
        "ServerAliveInterval=15",
        "-o",
        "ServerAliveCountMax=3",
        "--",
    ]
    .map(String::from)
    .into();
    args.push(machine.host.clone());
    args.push(machine.mk.clone().unwrap_or_else(|| "mk".into()));
    args.push("proxy".into());
    ("ssh".into(), args)
}

/// One line of advice for ssh's stderr, if a known failure.
pub fn hint(stderr: &str, machine: &Machine) -> Option<String> {
    let text = stderr.to_lowercase();
    let has = |needle: &str| text.contains(needle);
    let name = &machine.name;
    if has("permission denied") {
        Some(format!(
            "Add this computer's SSH key to {name} (ssh-copy-id, or its authorized_keys)."
        ))
    } else if has("host key verification failed") || has("remote host identification has changed") {
        Some(format!(
            "{name}'s host key changed. If that is expected, remove the old key with `ssh-keygen -R {}`.",
            machine.host
        ))
    } else if has("mk") && (has("not found") || has("not recognized") || has("no such file")) {
        Some(format!(
            "Mkrew's `mk` is not on {name}'s PATH for SSH. Set the mk path for this machine in Settings → Machines."
        ))
    } else if has("connection refused") {
        Some(format!(
            "{name} answers but runs no SSH server. Start it there (OpenSSH Server, Remote Login, openssh-server)."
        ))
    } else if has("could not resolve hostname") || has("timed out") || has("no route to host") {
        Some(format!(
            "{name} is offline or not reachable from here (Tailscale up? Same network?)."
        ))
    } else {
        None
    }
}

/// What a machine window tells the launcher on its stdout, one per line (spec §4.5).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MachineState {
    Connecting,
    Connected,
    Reconnecting,
    Unreachable(String),
    Version(String),
    /// Set by the launcher only, when the child exited.
    Closed,
}

impl MachineState {
    pub fn line(&self) -> String {
        let one_line = |text: &str| text.replace(['\r', '\n'], " ");
        match self {
            Self::Connecting => "connecting".into(),
            Self::Connected => "connected".into(),
            Self::Reconnecting => "reconnecting".into(),
            Self::Unreachable(text) => format!("unreachable {}", one_line(text)),
            Self::Version(text) => format!("version {}", one_line(text)),
            Self::Closed => "closed".into(),
        }
    }

    pub fn parse(line: &str) -> Option<Self> {
        let line = line.trim_end_matches(['\r', '\n']);
        match line {
            "connecting" => return Some(Self::Connecting),
            "connected" => return Some(Self::Connected),
            "reconnecting" => return Some(Self::Reconnecting),
            "closed" => return Some(Self::Closed),
            _ => {}
        }
        if let Some(text) = line.strip_prefix("unreachable ") {
            Some(Self::Unreachable(text.into()))
        } else {
            line.strip_prefix("version ")
                .map(|text| Self::Version(text.into()))
        }
    }

    /// "Connected", "Not reachable: …", … for the Machines page and the tray.
    pub fn label(&self) -> String {
        match self {
            Self::Connecting => "Connecting…".into(),
            Self::Connected => "Connected".into(),
            Self::Reconnecting => "Reconnecting…".into(),
            Self::Unreachable(text) => format!("Not reachable: {text}"),
            Self::Version(text) => format!("Version differs: {text}"),
            Self::Closed => "Window closed".into(),
        }
    }

    /// [`Self::label`] without the reason: one short line for the tray menu.
    pub fn short_label(&self) -> &'static str {
        match self {
            Self::Connecting => "Connecting…",
            Self::Connected => "Connected",
            Self::Reconnecting => "Reconnecting…",
            Self::Unreachable(_) => "Not reachable",
            Self::Version(_) => "Version differs",
            Self::Closed => "Window closed",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_short_label_drops_the_reason_and_the_long_one_keeps_it() {
        let cases = [
            (MachineState::Connecting, "Connecting…", "Connecting…"),
            (MachineState::Connected, "Connected", "Connected"),
            (MachineState::Reconnecting, "Reconnecting…", "Reconnecting…"),
            (
                MachineState::Unreachable("Permission denied".into()),
                "Not reachable",
                "Not reachable: Permission denied",
            ),
            (
                MachineState::Version("protocol 49".into()),
                "Version differs",
                "Version differs: protocol 49",
            ),
            (MachineState::Closed, "Window closed", "Window closed"),
        ];
        for (state, short, long) in cases {
            assert_eq!(state.short_label(), short);
            assert_eq!(state.label(), long);
        }
    }

    fn machine(name: &str, host: &str) -> Machine {
        Machine {
            name: name.into(),
            host: host.into(),
            mk: None,
        }
    }

    #[test]
    fn validation_refuses_options_spaces_duplicates_and_the_reserved_name() {
        let mut machines = Machines::default();
        machines.add(machine(" Home PC ", " home-pc ")).unwrap();
        assert_eq!(machines.list[0], machine("Home PC", "home-pc"), "trimmed");
        let refused = |m: Machine| machines.clone().add(m).unwrap_err().field;
        assert_eq!(refused(machine("", "x")), Field::Name);
        assert_eq!(refused(machine(&"n".repeat(41), "x")), Field::Name);
        assert_eq!(
            refused(machine("home pc", "other")),
            Field::Name,
            "case-insensitive duplicate"
        );
        assert_eq!(refused(machine("This computer", "x")), Field::Name);
        assert_eq!(refused(machine("A", "")), Field::Host);
        assert_eq!(refused(machine("A", "-oProxyCommand=calc")), Field::Host);
        assert_eq!(refused(machine("A", "user@host evil")), Field::Host);
        let mut bad_mk = machine("A", "h");
        bad_mk.mk = Some("-x".into());
        assert_eq!(refused(bad_mk), Field::Mk);
        let mut blank_mk = machine("A", "h");
        blank_mk.mk = Some("  ".into());
        machines.add(blank_mk).unwrap();
        assert_eq!(machines.get("a").unwrap().mk, None, "blank mk = default");
    }

    #[test]
    fn save_writes_load_reads_and_a_missing_file_is_empty() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(Machines::load(dir.path()).unwrap(), Machines::default());
        let mut machines = Machines::default();
        machines.add(machine("Home PC", "home-pc")).unwrap();
        let mut ubuntu = machine("Ubuntu", "me@box");
        ubuntu.mk = Some("~/.local/opt/mkrew/bin/mk".into());
        machines.add(ubuntu).unwrap();
        machines.save(dir.path()).unwrap();
        assert_eq!(Machines::load(dir.path()).unwrap(), machines);
        assert!(machines.remove("home pc"));
        assert!(!machines.remove("nope"));
    }

    #[test]
    fn a_broken_file_names_the_file_and_line_and_is_never_overwritten() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join(MACHINES_FILE);
        for broken in [
            "[[machine]]\nname = \"A\"\nhost = \n",
            "[[machine]]\nname = \"A\"\nhost = \"-oX\"\n",
            "[[machine]]\nname = \"A\"\nhost = \"h\"\n[[machine]]\nname = \"a\"\nhost = \"h2\"\n",
        ] {
            std::fs::write(&file, broken).unwrap();
            let error = Machines::load(dir.path()).unwrap_err();
            assert!(error.contains(MACHINES_FILE), "{error}");
            assert_eq!(std::fs::read_to_string(&file).unwrap(), broken);
        }
    }

    #[test]
    fn ssh_args_put_the_double_dash_before_the_host() {
        let (program, args) = ssh_command(&machine("Home PC", "me@home-pc"));
        assert_eq!(program, "ssh");
        assert_eq!(
            args,
            [
                "-T",
                "-o",
                "ConnectTimeout=10",
                "-o",
                "BatchMode=yes",
                "-o",
                "StrictHostKeyChecking=accept-new",
                "-o",
                "ServerAliveInterval=15",
                "-o",
                "ServerAliveCountMax=3",
                "--",
                "me@home-pc",
                "mk",
                "proxy",
            ]
        );
        let mut custom = machine("U", "box");
        custom.mk = Some("/opt/mkrew/bin/mk".into());
        let (_, args) = ssh_command(&custom);
        assert_eq!(args[args.len() - 2..], ["/opt/mkrew/bin/mk", "proxy"]);
    }

    #[test]
    fn hints_match_the_usual_ssh_failures() {
        let m = machine("Home PC", "home-pc");
        let hint = |text: &str| hint(text, &m).unwrap_or_default();
        assert!(hint("me@home-pc: Permission denied (publickey).").contains("SSH key"));
        assert!(hint("Host key verification failed.").contains("ssh-keygen -R home-pc"));
        assert!(
            hint("@@@ WARNING: REMOTE HOST IDENTIFICATION HAS CHANGED! @@@")
                .contains("ssh-keygen -R")
        );
        assert!(hint("bash: line 1: mk: command not found").contains("mk path"));
        assert!(
            hint("'mk' is not recognized as an internal or external command").contains("mk path")
        );
        assert!(
            hint("ssh: Could not resolve hostname home-pc: Name or service not known")
                .contains("offline")
        );
        assert!(
            hint("ssh: connect to host home-pc port 22: Connection timed out").contains("offline")
        );
        assert!(
            hint("ssh: connect to host home-pc port 22: Connection refused").contains("SSH server")
        );
        assert_eq!(hint("something else"), "");
    }

    #[test]
    fn state_lines_round_trip() {
        for state in [
            MachineState::Connecting,
            MachineState::Connected,
            MachineState::Reconnecting,
            MachineState::Unreachable("Permission denied (publickey).".into()),
            MachineState::Version("Home PC speaks protocol 51, this computer 50.".into()),
            MachineState::Closed,
        ] {
            let line = state.line();
            assert!(!line.contains('\n'), "{line}");
            assert_eq!(MachineState::parse(&line), Some(state));
        }
        assert_eq!(
            MachineState::Unreachable("a\nb".into()).line(),
            MachineState::Unreachable("a b".into()).line(),
            "newlines folded"
        );
        assert_eq!(MachineState::parse("garbage"), None);
    }
}
