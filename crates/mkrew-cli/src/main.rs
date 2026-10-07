//! `mk`: the Mkrew command-line client (SPEC §9).

mod attach;
mod client;
mod comment;
mod daemon;
mod hook;
mod integration;
mod project;
mod session;
mod task;

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use mkrew_core::Paths;
use mkrew_protocol::{CommentId, SessionState, TaskStatus};
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
    /// Manage and run board tasks.
    Task {
        #[command(subcommand)]
        command: TaskCommand,
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
    /// Read and answer Markdown review comments.
    Comment {
        #[command(subcommand)]
        command: CommentCommand,
    },
    /// Install, remove or check the hooks that report an agent's state to Mkrew.
    Integration {
        #[command(subcommand)]
        command: IntegrationCommand,
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
    /// Print a session's raw output log (also for exited sessions).
    History { session_id: Ulid },
}

#[derive(Subcommand)]
enum TaskCommand {
    /// Add a card to the project's Todo column and print its id.
    Add {
        /// Project id or name.
        project: String,
        title: String,
        /// What the agent is asked to do (delivered per the profile's `prompt_mode`).
        #[arg(long)]
        prompt: Option<String>,
        /// Agent profile id (default: the project's default agent at run time).
        #[arg(long)]
        agent: Option<String>,
    },
    /// List tasks.
    List {
        /// Only tasks of this project (id or name).
        #[arg(long)]
        project: Option<String>,
        /// Only tasks in this column: todo, running, needs_input, review or done.
        #[arg(long, value_parser = task::parse_status)]
        status: Option<TaskStatus>,
    },
    /// Start the task's agent in the project folder and print the session id.
    Run { task_id: Ulid },
    /// Show what undoing the task discards (files + diff since its checkpoint); `--yes`
    /// restores the files and deletes the ones it created.
    Undo {
        task_id: Ulid,
        /// Really discard the changes.
        #[arg(long)]
        yes: bool,
    },
}

#[derive(Subcommand)]
enum CommentCommand {
    /// List review threads (default project: the current session's, else the current
    /// directory's).
    List {
        /// Only comments on this file (relative to the current directory or project root).
        #[arg(long)]
        file: Option<String>,
        /// Project id or name.
        #[arg(long)]
        project: Option<String>,
    },
    /// Answer a review comment; inside a Mkrew session the reply is the agent's and marks
    /// the comment addressed.
    Reply { comment_id: CommentId, text: String },
}

#[derive(Subcommand)]
enum IntegrationCommand {
    /// Add Mkrew's hooks to the agent's config (backs the file up first; omp/copilot: one Mkrew-owned file).
    Install { agent: integration::Agent },
    /// Remove Mkrew's hooks from the agent's config, keeping everything else.
    Uninstall { agent: integration::Agent },
    /// Show whether the hooks are installed (exit code 1 if not).
    Status { agent: integration::Agent },
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
    /// Change project settings.
    #[command(group(clap::ArgGroup::new("change").required(true).multiple(true)))]
    Update {
        /// Project id or name.
        project: String,
        /// Git view base to compare against, e.g. `develop` or `origin/main`.
        #[arg(long, group = "change", conflicts_with = "auto_base")]
        base: Option<String>,
        /// Auto-detect the base again (`origin/HEAD` → `main` → `master`).
        #[arg(long, group = "change")]
        auto_base: bool,
        /// Run the workspace's run commands after the daemon starts.
        #[arg(long, group = "change", value_enum)]
        run_autostart: Option<project::Switch>,
    },
    /// Run the workspace's run commands (all, or one by name); prints `<name> <session id>`.
    Run {
        /// Project id or name.
        project: String,
        /// One command's name (default: all).
        name: Option<String>,
    },
    /// Stop, then run again the workspace's run commands (all, or one by name).
    Restart {
        /// Project id or name.
        project: String,
        /// One command's name (default: all).
        name: Option<String>,
    },
    /// Stop the workspace's run commands (all, or one by name).
    Stop {
        /// Project id or name.
        project: String,
        /// One command's name (default: all).
        name: Option<String>,
    },
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
    // Only touches the agents' config files.
    if let Command::Integration { command } = cli.command {
        let (action, agent) = match command {
            IntegrationCommand::Install { agent } => (integration::Action::Install, agent),
            IntegrationCommand::Uninstall { agent } => (integration::Action::Uninstall, agent),
            IntegrationCommand::Status { agent } => (integration::Action::Status, agent),
        };
        return integration::run(action, agent);
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
                ProjectCommand::Update {
                    project,
                    base,
                    auto_base,
                    run_autostart,
                } => project::update(
                    &mut client,
                    &project,
                    // clap rejects `--base` with `--auto-base`.
                    project::Changes {
                        base: (base.is_some() || auto_base).then_some(base),
                        run_autostart: run_autostart.map(|s| s == project::Switch::On),
                    },
                ),
                ProjectCommand::Run { project, name } => {
                    project::run(&mut client, &project, name, false)
                }
                ProjectCommand::Restart { project, name } => {
                    project::run(&mut client, &project, name, true)
                }
                ProjectCommand::Stop { project, name } => {
                    project::stop(&mut client, &project, name)
                }
            }
            .map(|()| ExitCode::SUCCESS)
        }
        Command::Task { command } => {
            let mut client = project::connect(&paths)?;
            match command {
                TaskCommand::Add {
                    project,
                    title,
                    prompt,
                    agent,
                } => task::add(&mut client, &project, title, prompt, agent),
                TaskCommand::List { project, status } => task::list(&mut client, project, status),
                TaskCommand::Run { task_id } => task::run(&mut client, task_id),
                TaskCommand::Undo { task_id, yes } => task::undo(&mut client, task_id, yes),
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
                SessionCommand::History { session_id } => session::history(&mut client, session_id),
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
        Command::Comment { command } => {
            let mut client = project::connect(&paths)?;
            match command {
                CommentCommand::List { file, project } => comment::list(&mut client, project, file),
                CommentCommand::Reply { comment_id, text } => {
                    comment::reply(&mut client, comment_id, text)
                }
            }
            .map(|()| ExitCode::SUCCESS)
        }
        Command::Hook { .. } | Command::Integration { .. } => unreachable!("handled above"),
    }
}
