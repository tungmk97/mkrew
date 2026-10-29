//! This computer's window as the launcher (spec 2026-10-29 §4.5): one `mkrew --machine <name>`
//! child per machine, each child's state lines read on a thread. No GPUI types.

use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};

use crate::launch::{self, Launch};
use crate::machines::MachineState;

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
    children: Vec<(String, Child)>,
    states: async_channel::Sender<(String, MachineState)>,
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

    /// Ends `name`'s window (Remove).
    pub fn close(&mut self, name: &str) {
        if let Some(at) = self.position(name) {
            end(self.children.swap_remove(at).1);
        }
    }

    /// Ends every window this launcher started (Quit, update install).
    pub fn close_all(&mut self) {
        for (_, child) in self.children.drain(..) {
            end(child);
        }
    }

    /// Whether the child of `name` still runs (an exited one is dropped).
    pub fn is_running(&mut self, name: &str) -> bool {
        let Some(at) = self.position(name) else {
            return false;
        };
        if matches!(self.children[at].1.try_wait(), Ok(None)) {
            return true;
        }
        self.children.swap_remove(at);
        false
    }

    fn position(&self, name: &str) -> Option<usize> {
        (self.children.iter()).position(|(started, _)| started.eq_ignore_ascii_case(name))
    }

    fn spawn(&mut self, name: &str) {
        let child = Command::new(&self.exe)
            .arg("--machine")
            .arg(name)
            .stdin(Stdio::null())
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
        self.children.push((name.to_owned(), child));
    }
}

/// Whichever way this window ends, its machine windows go with it.
impl Drop for Launcher {
    fn drop(&mut self) {
        self.close_all();
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

fn end(mut child: Child) {
    // Already gone is fine: the goal is no process left.
    let _ = child.kill();
    let _ = child.wait();
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
}
