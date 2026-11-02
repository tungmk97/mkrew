//! `port.list` / `port.stop` (spec 2026-11-01, ADR 0090): the listening TCP ports of the
//! machine and the workspace, task worktree and session each one belongs to. The decision
//! ([`attribute`]) is pure; the OS reads are [`scan`] and [`kill_tree`].

use std::collections::{HashMap, HashSet};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use std::path::{Path, PathBuf};

use mkrew_protocol::PortRow;
use ulid::Ulid;

/// How far up the parent chain a port's process is followed.
const MAX_DEPTH: usize = 64;

/// One listening socket as the OS reports it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Socket {
    pub port: u16,
    pub addr: IpAddr,
    pub pid: u32,
}

/// One process of the OS table. `cwd` is read only for listening processes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Proc {
    pub name: String,
    pub parent: Option<u32>,
    /// Start time (seconds since the epoch): a "parent" that started after its child is a
    /// reused pid, not the real parent.
    pub start: u64,
    pub cwd: Option<PathBuf>,
}

/// What a port under a live session is labelled with; keyed by the session's root pid.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Owner {
    pub session_id: Ulid,
    pub project_id: Ulid,
    pub worktree_task_id: Option<Ulid>,
    pub branch: Option<String>,
    pub session_title: Option<String>,
    pub run_command: Option<String>,
}

/// A workspace folder or a task worktree, for processes under no session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Folder {
    pub path: PathBuf,
    pub project_id: Ulid,
    pub worktree_task_id: Option<Ulid>,
    pub branch: Option<String>,
}

/// What the daemon knows: live sessions by root pid, folders, workspace order, its own pid.
#[derive(Debug, Clone, Default)]
pub(crate) struct Known {
    pub owners: HashMap<u32, Owner>,
    pub folders: Vec<Folder>,
    pub order: HashMap<Ulid, usize>,
    pub daemon_pid: u32,
}

/// What the OS reported.
#[derive(Debug, Clone, Default)]
pub(crate) struct Table {
    pub sockets: Vec<Socket>,
    pub processes: HashMap<u32, Proc>,
}

enum Walk<'a> {
    Session(&'a Owner),
    Daemon,
    Neither,
}

/// One row per `(port, pid)`, labelled and sorted (spec §4.2).
pub(crate) fn attribute(table: &Table, known: &Known) -> Vec<PortRow> {
    let mut best: HashMap<(u16, u32), IpAddr> = HashMap::new();
    for socket in &table.sockets {
        best.entry((socket.port, socket.pid))
            .and_modify(|addr| {
                if rank(socket.addr) < rank(*addr) {
                    *addr = socket.addr;
                }
            })
            .or_insert(socket.addr);
    }
    let mut rows: Vec<PortRow> = best
        .into_iter()
        .filter_map(|((port, pid), addr)| {
            let process = table.processes.get(&pid);
            let mut row = PortRow {
                port,
                addr: addr.to_string(),
                pid,
                process: process.map_or_else(String::new, |p| p.name.clone()),
                project_id: None,
                worktree_task_id: None,
                branch: None,
                session_id: None,
                session_title: None,
                run_command: None,
            };
            match walk(table, known, pid) {
                Walk::Daemon => return None,
                Walk::Session(owner) => {
                    row.project_id = Some(owner.project_id);
                    row.worktree_task_id = owner.worktree_task_id;
                    row.branch.clone_from(&owner.branch);
                    row.session_id = Some(owner.session_id);
                    row.session_title.clone_from(&owner.session_title);
                    row.run_command.clone_from(&owner.run_command);
                }
                Walk::Neither => {
                    let cwd = process.and_then(|p| p.cwd.as_deref());
                    if let Some(found) = cwd.and_then(|cwd| folder_of(&known.folders, cwd)) {
                        row.project_id = Some(found.project_id);
                        row.worktree_task_id = found.worktree_task_id;
                        row.branch.clone_from(&found.branch);
                    }
                }
            }
            Some(row)
        })
        .collect();
    rows.sort_by_key(|row| sort_key(row, known));
    rows
}

/// Every descendant of `root`, each after its own children (the kill order).
// ponytail: O(n²) over the process table (a few hundred entries); index by parent if it shows.
pub(crate) fn descendants(processes: &HashMap<u32, Proc>, root: u32) -> Vec<u32> {
    let mut out = Vec::new();
    collect(processes, root, &mut out, &mut HashSet::from([root]));
    out
}

/// `mkrew/fix-login-01J` -> `fix-login-01J`; other branches unchanged.
pub(crate) fn short_branch(branch: &str) -> String {
    branch.strip_prefix("mkrew/").unwrap_or(branch).to_owned()
}

fn collect(processes: &HashMap<u32, Proc>, pid: u32, out: &mut Vec<u32>, seen: &mut HashSet<u32>) {
    let Some(parent) = processes.get(&pid) else {
        return;
    };
    for (&child, process) in processes {
        if process.parent == Some(pid) && process.start >= parent.start && seen.insert(child) {
            collect(processes, child, out, seen);
            out.push(child);
        }
    }
}

fn walk<'a>(table: &Table, known: &'a Known, pid: u32) -> Walk<'a> {
    let mut seen = HashSet::new();
    let mut current = pid;
    for _ in 0..MAX_DEPTH {
        if let Some(owner) = known.owners.get(&current) {
            return Walk::Session(owner);
        }
        if current == known.daemon_pid {
            return Walk::Daemon;
        }
        if !seen.insert(current) {
            break;
        }
        let Some(process) = table.processes.get(&current) else {
            break;
        };
        let Some(parent) = process.parent else {
            break;
        };
        match table.processes.get(&parent) {
            Some(up) if up.start <= process.start => current = parent,
            _ => break,
        }
    }
    Walk::Neither
}

fn rank(addr: IpAddr) -> u8 {
    if addr == IpAddr::V4(Ipv4Addr::UNSPECIFIED) {
        0
    } else if addr == IpAddr::V6(Ipv6Addr::UNSPECIFIED) {
        1
    } else {
        2
    }
}

fn folder_of<'a>(folders: &'a [Folder], cwd: &Path) -> Option<&'a Folder> {
    folders
        .iter()
        .filter(|folder| cwd.starts_with(&folder.path))
        .max_by_key(|folder| folder.path.components().count())
}

fn sort_key(row: &PortRow, known: &Known) -> (usize, bool, Option<String>, u16, u32) {
    let order = row.project_id.map_or(usize::MAX, |id| {
        known.order.get(&id).copied().unwrap_or(usize::MAX - 1)
    });
    (
        order,
        row.worktree_task_id.is_some(),
        row.branch.clone(),
        row.port,
        row.pid,
    )
}

/// Reads the OS and labels its listening TCP ports. Blocking: call from `spawn_blocking`.
pub(crate) fn scan(mut known: Known) -> anyhow::Result<Vec<PortRow>> {
    let found =
        listeners::get_all().map_err(|e| anyhow::anyhow!("read the listening sockets: {e}"))?;
    let sockets: Vec<Socket> = found
        .into_iter()
        .filter(|l| {
            l.protocol == listeners::Protocol::TCP && l.state == listeners::SocketState::Listen
        })
        .map(|l| Socket {
            port: l.socket.port(),
            addr: l.socket.ip(),
            pid: l.process.pid,
        })
        .collect();
    let listening: HashSet<u32> = sockets.iter().map(|s| s.pid).collect();
    // Canonical on both sides: the real case and no `\\?\` (dunce), like the process cwd.
    for folder in &mut known.folders {
        if let Ok(path) = dunce::canonicalize(&folder.path) {
            folder.path = path;
        }
    }
    let table = Table {
        processes: read_processes(&listening),
        sockets,
    };
    Ok(attribute(&table, &known))
}

/// `pid`'s descendants (children first), then `pid`; an error when that would take the
/// daemon down with it, since `pid` is the daemon or one of its ancestors.
pub(crate) fn kill_order(
    processes: &HashMap<u32, Proc>,
    pid: u32,
    daemon_pid: u32,
) -> Result<Vec<u32>, String> {
    let mut order = descendants(processes, pid);
    if pid == daemon_pid || order.contains(&daemon_pid) {
        return Err(format!(
            "pid {pid} started the Mkrew daemon; stop it outside Mkrew"
        ));
    }
    order.push(pid);
    Ok(order)
}

/// Kills `pid` and its descendants (children first), never its parents or the daemon. Blocking.
pub(crate) fn kill_tree(pid: u32, daemon_pid: u32) -> Result<(), String> {
    use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate};
    let mut system = sysinfo::System::new();
    system.refresh_processes_specifics(ProcessesToUpdate::All, ProcessRefreshKind::new());
    let processes: HashMap<u32, Proc> = system
        .processes()
        .iter()
        .map(|(id, p)| (id.as_u32(), proc_of(p, None)))
        .collect();
    let order = kill_order(&processes, pid, daemon_pid)?;
    for child in &order[..order.len() - 1] {
        if let Some(process) = system.process(Pid::from_u32(*child)) {
            process.kill();
        }
    }
    match system.process(Pid::from_u32(pid)) {
        Some(process) if !process.kill() => Err(format!(
            "the OS refused to stop pid {pid} (access denied or a system process)"
        )),
        // Killed, or gone meanwhile: the port is free either way.
        _ => Ok(()),
    }
}

/// The process table; `cwd` read (and made canonical) only for `cwd_for`.
fn read_processes(cwd_for: &HashSet<u32>) -> HashMap<u32, Proc> {
    use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, UpdateKind};
    let mut system = sysinfo::System::new();
    system.refresh_processes_specifics(ProcessesToUpdate::All, ProcessRefreshKind::new());
    let wanted: Vec<Pid> = cwd_for.iter().map(|&pid| Pid::from_u32(pid)).collect();
    system.refresh_processes_specifics(
        ProcessesToUpdate::Some(&wanted),
        ProcessRefreshKind::new().with_cwd(UpdateKind::Always),
    );
    system
        .processes()
        .iter()
        .map(|(id, p)| {
            let cwd = cwd_for
                .contains(&id.as_u32())
                .then(|| p.cwd())
                .flatten()
                .filter(|cwd| !cwd.as_os_str().is_empty())
                .and_then(|cwd| dunce::canonicalize(cwd).ok());
            (id.as_u32(), proc_of(p, cwd))
        })
        .collect()
}

fn proc_of(process: &sysinfo::Process, cwd: Option<PathBuf>) -> Proc {
    Proc {
        name: process.name().to_string_lossy().into_owned(),
        parent: process.parent().map(|pid| pid.as_u32()),
        start: process.start_time(),
        cwd,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DAEMON: u32 = 10;
    const SHELL: u32 = 20;
    const NODE: u32 = 30;
    const POSTGRES: u32 = 40;
    const MK_WEB: u32 = 50;

    fn project() -> Ulid {
        Ulid(1)
    }
    fn task() -> Ulid {
        Ulid(2)
    }
    fn session() -> Ulid {
        Ulid(3)
    }
    fn other_project() -> Ulid {
        Ulid(4)
    }

    fn process(name: &str, parent: Option<u32>, start: u64) -> Proc {
        Proc {
            name: name.into(),
            parent,
            start,
            cwd: None,
        }
    }
    fn any_v4(port: u16, pid: u32) -> Socket {
        Socket {
            port,
            addr: IpAddr::V4(Ipv4Addr::UNSPECIFIED),
            pid,
        }
    }
    fn any_v6(port: u16, pid: u32) -> Socket {
        Socket {
            port,
            addr: IpAddr::V6(Ipv6Addr::UNSPECIFIED),
            pid,
        }
    }

    /// daemon(10) -> run session shell(20) -> node(30); postgres(40) outside; mk web(50).
    fn table(sockets: Vec<Socket>) -> Table {
        Table {
            sockets,
            processes: HashMap::from([
                (DAEMON, process("mkrew-daemon", Some(1), 100)),
                (SHELL, process("cmd.exe", Some(DAEMON), 200)),
                (NODE, process("node.exe", Some(SHELL), 300)),
                (POSTGRES, process("postgres", Some(1), 50)),
                (MK_WEB, process("mk.exe", Some(DAEMON), 150)),
            ]),
        }
    }

    fn folder(path: &str, project_id: Ulid, worktree: Option<(Ulid, &str)>) -> Folder {
        Folder {
            path: PathBuf::from(path),
            project_id,
            worktree_task_id: worktree.map(|(id, _)| id),
            branch: worktree.map(|(_, branch)| branch.to_owned()),
        }
    }

    fn known() -> Known {
        Known {
            owners: HashMap::from([(
                SHELL,
                Owner {
                    session_id: session(),
                    project_id: project(),
                    worktree_task_id: None,
                    branch: None,
                    session_title: Some("run:web".into()),
                    run_command: Some("web".into()),
                },
            )]),
            folders: vec![
                folder("/w/app", project(), None),
                folder("/w/app/.worktrees/fix", project(), Some((task(), "fix"))),
                folder("/w/api", other_project(), None),
            ],
            order: HashMap::from([(project(), 0), (other_project(), 1)]),
            daemon_pid: DAEMON,
        }
    }

    fn with_cwd(mut table: Table, pid: u32, cwd: &str) -> Table {
        if let Some(p) = table.processes.get_mut(&pid) {
            p.cwd = Some(PathBuf::from(cwd));
        }
        table
    }

    #[test]
    fn a_port_under_a_session_takes_its_workspace_and_run_command() {
        let rows = attribute(&table(vec![any_v4(3000, NODE)]), &known());
        assert_eq!(rows.len(), 1);
        let row = &rows[0];
        assert_eq!(
            (row.port, row.pid, row.process.as_str()),
            (3000, NODE, "node.exe")
        );
        assert_eq!(row.session_id, Some(session()));
        assert_eq!(row.project_id, Some(project()));
        assert_eq!(row.run_command.as_deref(), Some("web"));
        assert_eq!(row.session_title.as_deref(), Some("run:web"));
    }

    #[test]
    fn the_daemon_and_its_helpers_are_hidden() {
        let rows = attribute(
            &table(vec![any_v4(8787, MK_WEB), any_v4(9000, DAEMON)]),
            &known(),
        );
        assert!(rows.is_empty(), "{rows:?}");
    }

    #[test]
    fn a_process_in_no_folder_is_outside() {
        let rows = attribute(&table(vec![any_v4(5432, POSTGRES)]), &known());
        assert_eq!(rows[0].project_id, None);
        assert_eq!(rows[0].session_id, None);
        assert_eq!(rows[0].process, "postgres");
    }

    #[test]
    fn cwd_picks_the_longest_folder() {
        let at = |cwd: &str| {
            let t = with_cwd(table(vec![any_v4(5432, POSTGRES)]), POSTGRES, cwd);
            let row = attribute(&t, &known()).remove(0);
            (row.project_id, row.worktree_task_id, row.branch)
        };
        assert_eq!(
            at("/w/app/.worktrees/fix/web"),
            (Some(project()), Some(task()), Some("fix".into()))
        );
        assert_eq!(at("/w/app/src"), (Some(project()), None, None));
        assert_eq!(
            at("/w/application"),
            (None, None, None),
            "a name prefix is no folder"
        );
    }

    #[test]
    fn both_stacks_collapse_into_one_row_preferring_ipv4_any() {
        let rows = attribute(
            &table(vec![any_v6(3000, NODE), any_v4(3000, NODE)]),
            &known(),
        );
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].addr, "0.0.0.0");
    }

    #[test]
    fn a_reused_parent_pid_falls_back_to_the_cwd() {
        // The shell's pid now belongs to a process that started after node: not its parent.
        let mut reused = table(vec![any_v4(3000, NODE)]);
        reused
            .processes
            .insert(SHELL, process("cmd.exe", Some(DAEMON), 400));
        let rows = attribute(&with_cwd(reused, NODE, "/w/app/.worktrees/fix"), &known());
        assert_eq!(rows[0].session_id, None);
        assert_eq!(rows[0].worktree_task_id, Some(task()));
    }

    #[test]
    fn a_parent_cycle_ends_the_walk() {
        let mut cyclic = table(vec![any_v4(4000, 60)]);
        cyclic.processes.insert(60, process("a", Some(61), 500));
        cyclic.processes.insert(61, process("b", Some(60), 500));
        assert_eq!(attribute(&cyclic, &known())[0].project_id, None);
    }

    #[test]
    fn rows_sort_by_workspace_then_main_before_worktrees_then_port() {
        let sockets = [
            (9000, 71),
            (5000, 72),
            (3000, 73),
            (4000, 74),
            (1000, POSTGRES),
        ]
        .map(|(port, pid)| any_v4(port, pid))
        .to_vec();
        let mut t = table(sockets);
        for (pid, cwd) in [
            (71, "/w/api"),
            (72, "/w/app/.worktrees/fix"),
            (73, "/w/app"),
            (74, "/w/app"),
        ] {
            t.processes.insert(pid, process("x", Some(1), 600));
            t = with_cwd(t, pid, cwd);
        }
        let ports: Vec<u16> = attribute(&t, &known()).iter().map(|r| r.port).collect();
        assert_eq!(ports, [3000, 4000, 5000, 9000, 1000]);
    }

    #[test]
    fn descendants_come_children_first_and_skip_reused_pids() {
        let mut processes = table(vec![]).processes;
        processes.insert(31, process("child", Some(NODE), 310));
        processes.insert(32, process("grandchild", Some(31), 320));
        processes.insert(33, process("older", Some(NODE), 290));
        assert_eq!(descendants(&processes, NODE), [32, 31]);
        assert!(descendants(&processes, POSTGRES).is_empty());
    }

    #[test]
    fn kill_order_never_reaches_the_daemon() {
        let mut processes = table(vec![]).processes;
        processes.insert(1, process("ide", None, 10));
        processes.insert(32, process("grandchild", Some(NODE), 320));
        assert!(kill_order(&processes, 1, DAEMON).is_err());
        assert!(kill_order(&processes, DAEMON, DAEMON).is_err());
        assert_eq!(kill_order(&processes, NODE, DAEMON), Ok(vec![32, NODE]));
    }

    #[test]
    fn branches_lose_the_mkrew_prefix() {
        assert_eq!(short_branch("mkrew/fix-login-01J"), "fix-login-01J");
        assert_eq!(short_branch("feature/x"), "feature/x");
    }
}
