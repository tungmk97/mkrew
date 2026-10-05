//! `mk`: the Mkrew command-line client (SPEC §9).

mod attach;
mod client;
mod daemon;
mod paint;
mod project;
mod session;

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use mkrew_core::Paths;
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
    }
}
