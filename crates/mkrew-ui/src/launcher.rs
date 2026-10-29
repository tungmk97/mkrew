//! This computer's window as the launcher (spec 2026-10-29 §4.5): one `mkrew --machine <name>`
//! child per machine, each child's state lines read on a thread, and a `quit` line on its stdin
//! to end it (the child asks about unsaved Files changes like on any window close). No GPUI
//! types.

use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Child, ChildStdin, Command as Process, Stdio};

use crate::launch::{self, Launch};
use crate::machines::MachineState;

/// What a machine window reads on its stdin, one per line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    /// Run the window's own quit path (it asks about unsaved changes first).
    Quit,
}

const QUIT_LINE: &str = "quit";

/// The commands in `reader` (unknown lines skipped). The end of the stream is no command: the
/// launcher is gone (crashed) and the window keeps working.
pub fn read_commands(reader: impl BufRead) -> impl Iterator<Item = Command> {
    reader
        .lines()
        .map_while(Result::ok)
        .filter_map(|line| (line.trim() == QUIT_LINE).then_some(Command::Quit))
}

/// Writes the `quit` line; `false` when it could not be delivered.
fn write_quit(to: &mut impl Write) -> bool {
    writeln!(to, "{QUIT_LINE}")
        .and_then(|()| to.flush())
        .is_ok()
}

/// The states a child's stdout reports (unknown lines skipped), then [`MachineState::Closed`]
/// when the stream ends: the child exited.
pub fn read_states(reader: impl BufRead) -> impl Iterator<Item = MachineState> {
    reader
        .lines()
        .map_while(Result::ok)
        .filter_map(|line| MachineState::parse(&line))
        .chain(std::iter::once(MachineState::Closed))
}

pub struct Launcher {
    exe: PathBuf,
    /// Started children by machine name.
    children: Vec<Started>,
    states: async_channel::Sender<(String, MachineState)>,
}

struct Started {
    name: String,
    child: Child,
    /// `None` once `quit` was sent (or could not be).
    stdin: Option<ChildStdin>,
}

impl Started {
    /// Sends `quit` once; `false` if it could not be delivered (the child is not listening).
    fn ask_quit(&mut self) -> bool {
        self.stdin
            .take()
            .is_none_or(|mut stdin| write_quit(&mut stdin))
    }
}

impl Launcher {
    /// The receiver gets every state change of every child, from the moment it is started.
    pub fn new(exe: PathBuf) -> (Self, async_channel::Receiver<(String, MachineState)>) {
        let (states, receiver) = async_channel::unbounded();
        let launcher = Self {
            exe,
            children: Vec::new(),
            states,
        };
        (launcher, receiver)
    }

    /// Starts `name`'s window unless one runs already (at the start of Mkrew a window of another
    /// launcher: left alone, not raised).
    pub fn start(&mut self, name: &str) {
        if !self.is_running(name) && !crate::desktop::instance_running(&instance(name)) {
            self.spawn(name);
        }
    }

    /// Raises `name`'s window if it runs (Windows: through the instance key), else starts it.
    pub fn open(&mut self, name: &str) {
        let id = instance(name);
        if self.is_running(name) || crate::desktop::instance_running(&id) {
            crate::desktop::raise_running_instance(&id, None);
        } else {
            self.spawn(name);
        }
    }

    /// Asks `name`'s window to quit (Remove): it asks about unsaved text, then exits by itself.
    pub fn close(&mut self, name: &str) {
        if let Some(at) = self.position(name) {
            self.children[at].ask_quit();
        }
    }

    /// Asks every window this launcher started to quit (Quit, update install).
    pub fn close_all(&mut self) {
        for started in &mut self.children {
            started.ask_quit();
        }
    }

    /// Whether any window this launcher started still runs (exited ones are dropped).
    pub fn any_running(&mut self) -> bool {
        self.children
            .retain_mut(|started| matches!(started.child.try_wait(), Ok(None)));
        !self.children.is_empty()
    }

    /// Whether the child of `name` still runs (an exited one is dropped).
    pub fn is_running(&mut self, name: &str) -> bool {
        let Some(at) = self.position(name) else {
            return false;
        };
        if matches!(self.children[at].child.try_wait(), Ok(None)) {
            return true;
        }
        self.children.swap_remove(at);
        false
    }

    fn position(&self, name: &str) -> Option<usize> {
        (self.children.iter()).position(|started| crate::machines::same_name(&started.name, name))
    }

    fn spawn(&mut self, name: &str) {
        let child = Process::new(&self.exe)
            .arg("--machine")
            .arg(name)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn();
        let mut child = match child {
            Ok(child) => child,
            Err(error) => {
                tracing::warn!(machine = name, "cannot start the machine window: {error}");
                let state = MachineState::Unreachable(format!("cannot start its window: {error}"));
                let _ = self.states.try_send((name.to_owned(), state));
                return;
            }
        };
        let _ = self
            .states
            .try_send((name.to_owned(), MachineState::Connecting));
        if let Some(stdout) = child.stdout.take() {
            let (states, owner) = (self.states.clone(), name.to_owned());
            let reader = std::thread::Builder::new()
                .name("mkrew-machine-state".into())
                .spawn(move || {
                    for state in read_states(BufReader::new(stdout)) {
                        if states.send_blocking((owner.clone(), state)).is_err() {
                            return;
                        }
                    }
                });
            if let Err(error) = reader {
                tracing::warn!(machine = name, "cannot read the machine window: {error}");
            }
        }
        let stdin = child.stdin.take();
        self.children.push(Started {
            name: name.to_owned(),
            child,
            stdin,
        });
    }
}

/// Whichever way this window ends, its machine windows are asked to quit. They finish their own
/// questions and exit by themselves: only a child that cannot be asked (its stdin is broken) is
/// killed, the one last resort. ponytail: no kill after a grace time, it would also kill a
/// window that still asks "Save changes?".
impl Drop for Launcher {
    fn drop(&mut self) {
        for mut started in self.children.drain(..) {
            if !started.ask_quit() {
                // Already gone is fine: the goal is no unreachable process left.
                let _ = started.child.kill();
                let _ = started.child.wait();
            }
        }
    }
}

/// The instance id of a machine's window (the key its single-instance check uses).
fn instance(name: &str) -> String {
    let launch = Launch::Machine {
        name: name.to_owned(),
        jump: None,
    };
    launch::instance_id(&launch, "")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_childs_stdout_lines_become_states_and_eof_means_closed() {
        let text = b"connecting\nunreachable Permission denied\nnoise\nconnected\n";
        let states: Vec<MachineState> = read_states(&text[..]).collect();
        assert_eq!(
            states,
            [
                MachineState::Connecting,
                MachineState::Unreachable("Permission denied".into()),
                MachineState::Connected,
                MachineState::Closed,
            ]
        );
    }

    #[test]
    fn the_quit_line_is_read_back_and_the_end_of_the_stream_is_ignored() {
        let mut sent = Vec::new();
        assert!(write_quit(&mut sent));
        let commands: Vec<Command> = read_commands(&sent[..]).collect();
        assert_eq!(commands, [Command::Quit]);
        // Noise is skipped; EOF alone (the launcher crashed) is no command.
        assert_eq!(read_commands(&b"hello\n"[..]).count(), 0);
        assert_eq!(read_commands(&b""[..]).count(), 0);
        assert_eq!(read_commands(&b" quit \r\n"[..]).count(), 1);
    }

    #[test]
    fn a_closed_pipe_is_a_failed_quit() {
        struct Broken;
        impl Write for Broken {
            fn write(&mut self, _: &[u8]) -> std::io::Result<usize> {
                Err(std::io::ErrorKind::BrokenPipe.into())
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }
        assert!(!write_quit(&mut Broken));
    }
}
