//! `mk`: the Mkrew command-line client (SPEC §9).

mod client;
mod daemon;
mod project;

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use mkrew_core::Paths;

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
    }
}
