//! `mk`: the Mkrew command-line client (SPEC §9).

mod attach;
mod client;
mod daemon;
mod hook;
mod project;
mod session;

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use mkrew_core::Paths;
use mkrew_protocol::SessionState;
use ulid::Ulid;

#[derive(Parser)]
#[command(name = "mk", version, about = "Mkrew command-line client")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Start, stop or inspect the background daemon.
    Daemon {
        #[command(subcommand)]
        command: DaemonCommand,
    },
    /// Manage projects.
    Project {
        #[command(subcommand)]
        command: ProjectCommand,
    },
    /// List or start agent sessions.
    Session {
        #[command(subcommand)]
        command: SessionCommand,
    },
    /// Attach this terminal to a session; detach with Ctrl+] (the agent keeps running).
    Attach { session_id: Ulid },
    /// Type text into a session; queued while the agent is working.
    Send {
        session_id: Ulid,
        text: String,
        /// Do not press Enter after the text.
        #[arg(long)]
        no_submit: bool,
    },
    /// Wait until a session reaches one of the given states (or exits) and print it.
    Wait {
        session_id: Ulid,
        /// Comma-separated states, e.g. `idle,needs_input,done`.
        #[arg(long, required = true, value_delimiter = ',', value_parser = session::parse_state)]
        until: Vec<SessionState>,
        /// Give up after this many milliseconds (exit code 1).
        #[arg(long)]
        timeout: Option<u64>,
    },
    /// Report an agent hook event to the session (called by agent hooks, not by users).
    Hook {
        /// `claude`, `codex` or `fake`.
        agent: String,
        /// The event name (`codex`: the notify JSON instead).
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
}

#[derive(Subcommand)]
enum SessionCommand {
    /// List sessions.
    List {
        /// Only sessions of this project (id or name).
        #[arg(long)]
        project: Option<String>,
    },
    /// Start a session from an agent profile and print its id.
    Spawn {
        /// Project id or name.
        project: String,
        /// Agent profile id (file name in the agents directory, e.g. `claude`).
        #[arg(long)]
        agent: String,
    },
}

#[derive(Subcommand)]
enum DaemonCommand {
    /// Start the daemon detached (no-op if it is already running).
    Start,
    /// Stop the daemon and wait until it has exited.
    Stop,
    /// Show whether the daemon is running (exit code 1 if not).
    Status,
}

#[derive(Subcommand)]
enum ProjectCommand {
    /// Register a project directory.
    Add {
        path: PathBuf,
        /// Display name (default: the directory name).
        #[arg(long)]
        name: Option<String>,
    },
    /// List registered projects.
    List,
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match run(cli) {
        Ok(code) => code,
        Err(e) => {
            eprintln!("mk: {e:#}");
            ExitCode::FAILURE
        }
    }
}

fn run(cli: Cli) -> anyhow::Result<ExitCode> {
    // Hooks also run outside Mkrew, where nothing else may be set up.
    if let Command::Hook { agent, args } = cli.command {
        return hook::hook(agent, args);
    }
    let paths = Paths::from_env()?;
    match cli.command {
        Command::Daemon { command } => match command {
            DaemonCommand::Start => daemon::start(&paths),
            DaemonCommand::Stop => daemon::stop(&paths),
            DaemonCommand::Status => daemon::status(&paths),
        },
        Command::Project { command } => {
            let mut client = project::connect(&paths)?;
            match command {
                ProjectCommand::Add { path, name } => project::add(&mut client, &path, name),
                ProjectCommand::List => project::list(&mut client),
            }
            .map(|()| ExitCode::SUCCESS)
        }
        Command::Session { command } => {
            let mut client = project::connect(&paths)?;
            match command {
                SessionCommand::List { project } => session::list(&mut client, project),
                SessionCommand::Spawn { project, agent } => {
                    session::spawn(&mut client, &project, agent)
                }
            }
            .map(|()| ExitCode::SUCCESS)
        }
        Command::Attach { session_id } => attach::attach(&paths, session_id),
        Command::Send {
            session_id,
            text,
            no_submit,
        } => {
            let mut client = project::connect(&paths)?;
            session::send(&mut client, session_id, text, no_submit).map(|()| ExitCode::SUCCESS)
        }
        Command::Wait {
            session_id,
            until,
            timeout,
        } => {
            let mut client = project::connect(&paths)?;
            session::wait(&mut client, session_id, until, timeout).map(|()| ExitCode::SUCCESS)
        }
        Command::Hook { .. } => unreachable!("handled above"),
    }
}
