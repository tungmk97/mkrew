# Port manager Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** One place (a dialog, a palette command, a Run menu row and `mk ports`) that lists every listening TCP port of the daemon's machine with its workspace, task worktree and pane, and stops it.

**Architecture:** The daemon scans on request (`port.list`): `listeners` gives the listening sockets with their pid, `sysinfo` the process table; a pure `attribute` function walks each pid up to a live session's root pid (or the daemon's own pid, which hides the row) and falls back to a cwd-under-a-workspace match.
`port.stop` re-scans, then stops a run session through its session kill, or kills the process and its descendants.
The UI polls `port.list` every 2 s only while its dialog is open; the CLI calls the same two requests.

**Tech Stack:** Rust workspace (tokio daemon, gpui UI, clap CLI), `listeners` 0.6 (new), `sysinfo` 0.31 (already in the tree).

**Spec:** `docs/superpowers/specs/2026-11-01-port-manager-design.md`

## Global Constraints

- Work in the worktree `.worktrees/port-manager` (branch `feat/port-manager`); commit with `git -c user.email=tungnv975@gmail.com commit`, Conventional Commits, no co-author line.
- `PROTOCOL_VERSION` goes from 50 to 51.
- New dependencies, daemon only: `listeners = "0.6"`, `sysinfo = { version = "0.31", default-features = false, features = ["system"] }`.
- No `unwrap`/`expect` outside tests unless provably infallible (comment why).
- No test calls the network or a real agent; tests listen on `127.0.0.1:0` only.
- UI: every color, spacing, radius and font size from `crates/mkrew-ui/src/theme.rs`; new tokens `PORTS_DIALOG_WIDTH`, `PORTS_LIST_HEIGHT`, `PORTS_TICK` (2 s). No hex colors or raw px numbers in `shell/ports.rs`.
- Never the em dash character in code, docs or UI text; use `-`.
- Markdown docs: one sentence per line.
- Verify: `"C:/Program Files/Git/bin/bash.exe" scripts/verify.sh` (fmt check, clippy `-D warnings`, tests). Fast loop: `cargo test -p <crate> <filter>`.
- `cargo fmt --all` may touch other agents' files: commit only the files of the task.
- Look up crate APIs (`listeners`, `sysinfo` 0.31, gpui) in their source before relying on a name in this plan; the plan's names were read on 2026-11-01.

## Review Focus

1. A server started by an agent that detached from its shell (parent gone): it must still show under its workspace through the cwd match. Pinned by `a_reused_parent_pid_falls_back_to_the_cwd` (Task 2).
2. Stopping a port typed in a shell pane must not close the pane. Pinned by `stopping_a_shell_pane_port_keeps_the_pane` (Task 3).
3. A pid reused between list and stop must not be killed: `port.stop` re-checks `(port, pid)`. Pinned by `a_stale_pid_is_not_found` (Task 3).
4. The `mk web` server and the daemon itself must never be listed or stopped. Pinned by `the_daemon_and_its_helpers_are_hidden` (Task 2).
5. `/w/application` must not match the folder `/w/app`, and a worktree inside its project folder must win over the project. Pinned by `cwd_picks_the_longest_folder` (Task 2).

---

### Task 1: Protocol: `port.list`, `port.stop`, `PortRow`

**Files:**
- Modify: `crates/mkrew-protocol/src/message.rs` (enum `Request`, after `SessionPorts` at about line 287; new structs after `SessionPortsEvent` at about line 1644)
- Modify: `crates/mkrew-protocol/src/lib.rs:16`
- Test: `crates/mkrew-protocol/src/message/tests.rs`
- Modify: `docs/SPEC.md` (§7 request table after the `session.ports` row, line 549; protocol log after line 661)

**Interfaces:**
- Produces: `mkrew_protocol::PortRow`, `mkrew_protocol::PortStopParams`, `Request::PortList`, `Request::PortStop(PortStopParams)`, `PROTOCOL_VERSION == 51`.

- [ ] **Step 1: Write the failing tests** in `message/tests.rs`.

In the `vec![...]` of `every_m1_request_round_trips`, after `Request::SessionPorts(...)`:

```rust
        Request::PortList,
        Request::PortStop(PortStopParams {
            port: 3000,
            pid: 1234,
        }),
```

New test after `param_less_requests_omit_params`:

```rust
/// Spec 2026-11-01: a row outside Mkrew carries no workspace keys on the wire.
#[test]
fn port_rows_leave_out_missing_owners() {
    let outside = PortRow {
        port: 5432,
        addr: "0.0.0.0".into(),
        pid: 40,
        process: "postgres".into(),
        project_id: None,
        worktree_task_id: None,
        branch: None,
        session_id: None,
        session_title: None,
        run_command: None,
    };
    let wire = serde_json::to_value(&outside).unwrap();
    assert_eq!(
        wire,
        json!({"port": 5432, "addr": "0.0.0.0", "pid": 40, "process": "postgres"})
    );
    assert_eq!(serde_json::from_value::<PortRow>(wire).unwrap(), outside);
    let message = Message::Request {
        id: 2,
        request: Request::PortList,
    };
    assert_eq!(
        round_trip(&message),
        json!({"type": "request", "id": 2, "method": "port.list"})
    );
}
```

- [ ] **Step 2: Run to see it fail**

Run: `cargo test -p mkrew-protocol port_rows`
Expected: compile error, `PortRow` / `Request::PortList` not found.

- [ ] **Step 3: Implement**

In `enum Request`, right after `SessionPorts(ProjectRef)`:

```rust
    /// Returns `PortRow[]`: every listening TCP port of the daemon's machine, with the
    /// workspace, task worktree and session that own it when Mkrew knows them (spec 2026-11-01).
    #[serde(rename = "port.list")]
    PortList,
    /// Stops the process holding `port` if that is still `pid`: a run command's session ends,
    /// any other process is killed with its descendants. Returns `null`; `not_found` when `pid`
    /// no longer holds `port`, `invalid_state` when the OS refuses (spec 2026-11-01).
    #[serde(rename = "port.stop")]
    PortStop(PortStopParams),
```

After `SessionPortsEvent`:

```rust
/// One listening TCP port (`port.list`, spec 2026-11-01). `project_id` is empty for a process
/// outside every workspace; `session_id` is set when the process runs under a live session.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PortRow {
    pub port: u16,
    /// The listening address, `0.0.0.0` first when the process listens on several.
    pub addr: String,
    pub pid: u32,
    /// The process name as the OS reports it (`node.exe`).
    pub process: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_id: Option<Ulid>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub worktree_task_id: Option<Ulid>,
    /// The task worktree's branch without `mkrew/`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<Ulid>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_title: Option<String>,
    /// The run command's name when the session is a run command (`web`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run_command: Option<String>,
}

/// `port.stop` (spec 2026-11-01): both, so a pid the OS reused is never killed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PortStopParams {
    pub port: u16,
    pub pid: u32,
}
```

`lib.rs`: `pub const PROTOCOL_VERSION: u32 = 51;`

- [ ] **Step 4: Run the tests**

Run: `cargo test -p mkrew-protocol`
Expected: PASS.

- [ ] **Step 5: SPEC**

After the `session.ports` row of the request table:

```markdown
| `port.list` | – | `PortRow[]`: every listening TCP port of the daemon's machine (`port`, `addr`, `pid`, `process`, `project_id?`, `worktree_task_id?`, `branch?` without `mkrew/`, `session_id?`, `session_title?`, `run_command?`); a pid under a live session takes that session's workspace, worktree and run command; else a cwd inside a workspace or task worktree folder (the longest wins) takes it; the daemon and its own children (`mk web`) are left out (spec 2026-11-01, ADR 0090) |
| `port.stop` | `port`, `pid` | ok - lists again first; `not_found` when `pid` no longer holds `port`; a run command's session is killed like `project.run_stop`; any other process is killed with its descendants, never its parents, so a shell pane or an agent stays; `invalid_state` when the OS refuses |
```

After the `Protocol 50 ...` line, a blank line, then:

```markdown
Protocol 51 (port manager, spec 2026-11-01): requests `port.list` and `port.stop`.
```

- [ ] **Step 6: Commit**

```bash
git add crates/mkrew-protocol docs/SPEC.md
git -c user.email=tungnv975@gmail.com commit -m "feat(protocol): port.list and port.stop (protocol 51)"
```

If `cargo build --workspace` fails because the daemon's `match request` has no wildcard arm, that is fixed by Task 3 Step 6; tasks 1-3 are one reviewable unit for the workspace build, each with its own crate-level tests.

---

### Task 2: Daemon: pure port attribution

**Files:**
- Create: `crates/mkrew-daemon/src/ports.rs`
- Modify: `crates/mkrew-daemon/src/lib.rs` (`mod ports;` after `mod plan;`)

**Interfaces:**
- Consumes: `mkrew_protocol::PortRow` (Task 1).
- Produces (`pub(crate)` in `crate::ports`):
  - `struct Socket { port: u16, addr: IpAddr, pid: u32 }`
  - `struct Proc { name: String, parent: Option<u32>, start: u64, cwd: Option<PathBuf> }`
  - `struct Owner { session_id, project_id: Ulid, worktree_task_id: Option<Ulid>, branch: Option<String>, session_title: Option<String>, run_command: Option<String> }`
  - `struct Folder { path: PathBuf, project_id: Ulid, worktree_task_id: Option<Ulid>, branch: Option<String> }`
  - `struct Known { owners: HashMap<u32, Owner>, folders: Vec<Folder>, order: HashMap<Ulid, usize>, daemon_pid: u32 }` (`owners` keyed by the session's root pid)
  - `struct Table { sockets: Vec<Socket>, processes: HashMap<u32, Proc> }`
  - `fn attribute(table: &Table, known: &Known) -> Vec<PortRow>`
  - `fn descendants(processes: &HashMap<u32, Proc>, root: u32) -> Vec<u32>` (children after their own children)
  - `fn short_branch(branch: &str) -> String`

- [ ] **Step 1: Write the test module** at the bottom of the new `crates/mkrew-daemon/src/ports.rs`:

```rust
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
        assert_eq!((row.port, row.pid, row.process.as_str()), (3000, NODE, "node.exe"));
        assert_eq!(row.session_id, Some(session()));
        assert_eq!(row.project_id, Some(project()));
        assert_eq!(row.run_command.as_deref(), Some("web"));
        assert_eq!(row.session_title.as_deref(), Some("run:web"));
    }

    #[test]
    fn the_daemon_and_its_helpers_are_hidden() {
        let rows = attribute(&table(vec![any_v4(8787, MK_WEB), any_v4(9000, DAEMON)]), &known());
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
        assert_eq!(at("/w/application"), (None, None, None), "a name prefix is no folder");
    }

    #[test]
    fn both_stacks_collapse_into_one_row_preferring_ipv4_any() {
        let rows = attribute(&table(vec![any_v6(3000, NODE), any_v4(3000, NODE)]), &known());
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].addr, "0.0.0.0");
    }

    #[test]
    fn a_reused_parent_pid_falls_back_to_the_cwd() {
        // The shell's pid now belongs to a process that started after node: not its parent.
        let mut reused = table(vec![any_v4(3000, NODE)]);
        reused.processes.insert(SHELL, process("cmd.exe", Some(DAEMON), 400));
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
        let sockets = [(9000, 71), (5000, 72), (3000, 73), (4000, 74), (1000, POSTGRES)]
            .map(|(port, pid)| any_v4(port, pid))
            .to_vec();
        let mut t = table(sockets);
        for (pid, cwd) in [(71, "/w/api"), (72, "/w/app/.worktrees/fix"), (73, "/w/app"), (74, "/w/app")] {
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
    fn branches_lose_the_mkrew_prefix() {
        assert_eq!(short_branch("mkrew/fix-login-01J"), "fix-login-01J");
        assert_eq!(short_branch("feature/x"), "feature/x");
    }
}
```

- [ ] **Step 2: Run to see it fail**

Run: `cargo test -p mkrew-daemon --lib ports::`
Expected: compile errors (items missing).

- [ ] **Step 3: Implement** above the test module:

```rust
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
    (order, row.worktree_task_id.is_some(), row.branch.clone(), row.port, row.pid)
}
```

`lib.rs`: `mod ports;` after `mod plan;`.
Until Task 3 calls these from the server, `cargo clippy` reports them unused; that is expected between Task 2 and Task 3 and no `allow` attribute is added.

- [ ] **Step 4: Run the tests**

Run: `cargo test -p mkrew-daemon --lib ports::`
Expected: 10 tests PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/mkrew-daemon/src/ports.rs crates/mkrew-daemon/src/lib.rs
git -c user.email=tungnv975@gmail.com commit -m "feat(daemon): attribute listening ports to sessions and folders"
```

---

### Task 3: Daemon: OS scan, kill, requests, fake listener, ADR

**Files:**
- Modify: `crates/mkrew-daemon/Cargo.toml`
- Modify: `crates/mkrew-daemon/src/ports.rs` (`scan`, `kill_tree`)
- Modify: `crates/mkrew-daemon/src/session.rs` (`struct Live` at line 200, the `Live { .. }` literal at line 519, new `Hub::live_pids` next to `live_count` at line 362)
- Create: `crates/mkrew-daemon/src/server/ports.rs`
- Modify: `crates/mkrew-daemon/src/server.rs` (`mod ports;` after `mod git;`, line 44; two arms next to `Request::SessionPorts`, line 454)
- Modify: `crates/mkrew-fakeagent/src/main.rs` (`--listen`)
- Create: `crates/mkrew-daemon/tests/daemon/ports.rs`; Modify: `crates/mkrew-daemon/tests/daemon/main.rs`
- Create: `docs/decisions/0090-port-manager.md`

**Interfaces:**
- Consumes: Task 1 types; Task 2 `crate::ports::*`.
- Produces: `Hub::live_pids(&self) -> Vec<(Ulid, u32)>`, `ports::scan(known: Known) -> anyhow::Result<Vec<PortRow>>`, `ports::kill_tree(pid: u32) -> Result<(), String>`, handlers for `port.list` / `port.stop`, `mkrew-fakeagent --listen`.

- [ ] **Step 1: Fake listener** in `crates/mkrew-fakeagent/src/main.rs`.

Test first, in its `tests` module:

```rust
    #[test]
    fn listen_is_a_flag() {
        assert!(parse_args(args(&["--listen"])).unwrap().listen);
        assert!(!parse_args(args(&[])).unwrap().listen);
    }
```

Then: `listen: bool` in `Args`; in `parse_args`, before the `arg.starts_with("--")` catch-all:

```rust
        } else if arg == "--listen" {
            parsed.listen = true;
```

`USAGE` and the module doc gain `[--listen]` and the line `` `--listen` binds `127.0.0.1:0`, prints `Listening on http://127.0.0.1:<port>/` and waits until killed (a dev server for the port tests). ``
In `real_main`, right after `parse_args`:

```rust
    if args.listen {
        return listen();
    }
```

```rust
/// `--listen`: a stand-in dev server for the daemon's port tests; never returns.
fn listen() -> Result<i32> {
    let listener = std::net::TcpListener::bind("127.0.0.1:0")?;
    let mut out = io::stdout().lock();
    writeln!(out, "Listening on http://127.0.0.1:{}/", listener.local_addr()?.port())?;
    out.flush()?;
    loop {
        // Connections are dropped at once: only the open port matters.
        let _ = listener.accept();
    }
}
```

Run: `cargo test -p mkrew-fakeagent listen_is_a_flag` - PASS.

- [ ] **Step 2: Dependencies** in `crates/mkrew-daemon/Cargo.toml` `[dependencies]`, alphabetical. After `interprocess`:

```toml
# `port.list` (spec 2026-11-01): listening sockets with their pid on Windows, Linux and macOS
# (ADR 0090).
listeners = "0.6"
```

After `serde_json`:

```toml
# `port.list` / `port.stop`: parent pid, start time, cwd and kill of other processes; 0.31 is
# already in the tree (mkrew-ui, spike-term).
sysinfo = { version = "0.31", default-features = false, features = ["system"] }
```

Run: `cargo build -p mkrew-daemon`; `Cargo.lock` must still list one `sysinfo` (0.31.4).
Read the fetched `listeners` 0.6 `src/lib.rs`: the plan uses `listeners::get_all() -> Result<HashSet<Listener>, Box<dyn Error>>`, `Listener { process: Process { pid, name, .. }, socket: SocketAddr, protocol: Protocol::TCP, state: SocketState::Listen }`.

- [ ] **Step 3: Write the failing daemon tests** `crates/mkrew-daemon/tests/daemon/ports.rs`:

```rust
//! `port.list` / `port.stop` (spec 2026-11-01) against real processes on `127.0.0.1:0`.

use super::*;

/// The command line that starts the fake dev server.
fn listener_line() -> String {
    format!("\"{}\" --listen", built_bin("mkrew-fakeagent").display())
}

fn port_list(client: &mut Client) -> Vec<PortRow> {
    client.call(Request::PortList).into_result().unwrap()
}

/// Waits until `session_id` holds a listening port and returns its row.
fn port_of(client: &mut Client, session_id: Ulid) -> PortRow {
    let deadline = Instant::now() + TIMEOUT;
    loop {
        if let Some(row) = port_list(client)
            .into_iter()
            .find(|r| r.session_id == Some(session_id))
        {
            return row;
        }
        assert!(Instant::now() < deadline, "no port for session {session_id}");
        std::thread::sleep(Duration::from_millis(200));
    }
}

/// Waits until nothing holds `port`.
fn wait_closed(client: &mut Client, port: u16) {
    let deadline = Instant::now() + TIMEOUT;
    while port_list(client).iter().any(|r| r.port == port) {
        assert!(Instant::now() < deadline, ":{port} still listens");
        std::thread::sleep(Duration::from_millis(200));
    }
}

fn stop(client: &mut Client, row: &PortRow) -> Outcome {
    client.call(Request::PortStop(PortStopParams {
        port: row.port,
        pid: row.pid,
    }))
}

#[test]
fn a_run_command_port_is_listed_and_stopped_with_its_session() {
    let env = Env::new();
    let repo = env.repo();
    let (_daemon, mut client) = env.start();
    let project = client.add_project(&repo);
    let web = run_command("web", ".", &listener_line());
    client
        .call(set_runs(&project, vec![web], None))
        .into_result::<Project>()
        .unwrap();
    let started: RunStarted = client.call(run_start(&project, None)).into_result().unwrap();
    let id = started.sessions[0].session_id;

    let row = port_of(&mut client, id);
    assert_eq!(row.project_id, Some(project.id));
    assert_eq!(row.run_command.as_deref(), Some("web"));
    assert_eq!(row.worktree_task_id, None);
    // Linux shortens process names to 15 characters.
    assert!(row.process.starts_with("mkrew-fakeagen"), "{}", row.process);

    stop(&mut client, &row).into_result::<()>().unwrap();
    client.wait_state(id, SessionState::Exited);
    wait_closed(&mut client, row.port);
    client.stop();
}

#[test]
fn stopping_a_shell_pane_port_keeps_the_pane() {
    let env = Env::new();
    let repo = env.repo();
    let (_daemon, mut client) = env.start();
    let project = client.add_project(&repo);
    let shell = client
        .call(spawn_params(&project, SessionKind::Shell, None))
        .into_result::<SessionSpawnResult>()
        .unwrap()
        .session_id;
    client
        .input(shell, format!("{}\r", listener_line()).as_bytes())
        .into_result::<()>()
        .unwrap();

    let row = port_of(&mut client, shell);
    assert_eq!(row.project_id, Some(project.id));
    assert_eq!(row.run_command, None);

    stop(&mut client, &row).into_result::<()>().unwrap();
    wait_closed(&mut client, row.port);
    assert_ne!(client.session(shell).state, SessionState::Exited, "the pane stays");
    client.stop();
}

#[test]
fn a_stale_pid_is_not_found() {
    let env = Env::new();
    let (_daemon, mut client) = env.start();
    let outcome = client.call(Request::PortStop(PortStopParams {
        port: 1,
        pid: u32::MAX - 1,
    }));
    assert_eq!(error_code(outcome), ErrorCode::NotFound);
    client.stop();
}
```

`tests/daemon/main.rs`: `mod ports;` in the module list (after `mod io;`); `PortRow, PortStopParams` in the `use mkrew_protocol::{...}` block; `Instant` in the `std::time` import if it is not there.

Run: `cargo test -p mkrew-daemon --test daemon ports::`
Expected: FAIL (no handler yet).

- [ ] **Step 4: Session root pids** in `session.rs`.

`struct Live` gains:

```rust
    /// The PTY child's pid (the shell or agent itself), for `port.list`'s parent walk.
    pid: Option<u32>,
```

In the `Live { .. }` literal (line 519) add `pid,`: the `let pid = child.process_id();` of line 431 is in scope and `Option<u32>` is `Copy`, so the later `Actor { pid, .. }` still compiles.
Next to `live_count`:

```rust
    /// `(session id, root pid)` of every live session whose PTY reported a pid (`port.list`).
    pub fn live_pids(&self) -> Vec<(Ulid, u32)> {
        lock(&self.live)
            .iter()
            .filter_map(|(id, live)| Some((*id, live.pid?)))
            .collect()
    }
```

- [ ] **Step 5: OS layer**, appended to `ports.rs` above the tests:

```rust
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

/// Kills `pid` and its descendants (children first), never its parents. Blocking.
pub(crate) fn kill_tree(pid: u32) -> Result<(), String> {
    use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate};
    let mut system = sysinfo::System::new();
    system.refresh_processes_specifics(ProcessesToUpdate::All, ProcessRefreshKind::new());
    let processes: HashMap<u32, Proc> = system
        .processes()
        .iter()
        .map(|(id, p)| (id.as_u32(), proc_of(p, None)))
        .collect();
    for child in descendants(&processes, pid) {
        if let Some(process) = system.process(Pid::from_u32(child)) {
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
```

- [ ] **Step 6: Handlers** `crates/mkrew-daemon/src/server/ports.rs`:

```rust
//! `port.list` / `port.stop` (spec 2026-11-01, ADR 0090).

use std::collections::HashMap;
use std::path::PathBuf;

use mkrew_core::run;
use mkrew_protocol::{ErrorCode, Outcome, PortRow, PortStopParams, SessionState};

use super::{Daemon, ok, store_outcome};
use crate::ports::{self, Folder, Known, Owner};
use crate::session::Command;

impl Daemon {
    pub(super) async fn port_list(&self) -> Outcome {
        match self.port_rows().await {
            Ok(rows) => ok(&rows),
            Err(outcome) => outcome,
        }
    }

    /// Lists again, then acts only when `pid` still holds `port`.
    pub(super) async fn port_stop(&self, params: PortStopParams) -> Outcome {
        let rows = match self.port_rows().await {
            Ok(rows) => rows,
            Err(outcome) => return outcome,
        };
        let Some(row) = rows
            .into_iter()
            .find(|r| r.port == params.port && r.pid == params.pid)
        else {
            let message = format!("port :{} is no longer held by pid {}", params.port, params.pid);
            return Outcome::error(ErrorCode::NotFound, message);
        };
        if let (Some(session_id), Some(name)) = (row.session_id, row.run_command.as_deref()) {
            tracing::info!(%session_id, name, port = row.port, "stopping a run command from the port list");
            self.hub.command(session_id, Command::Kill { force: false });
            return ok(&());
        }
        tracing::info!(pid = row.pid, port = row.port, process = row.process, "stopping a port's process");
        match tokio::task::spawn_blocking(move || ports::kill_tree(params.pid)).await {
            Ok(Ok(())) => ok(&()),
            Ok(Err(reason)) => Outcome::error(ErrorCode::InvalidState, reason),
            Err(e) => Outcome::error(ErrorCode::Internal, format!("port stop: {e}")),
        }
    }

    async fn port_rows(&self) -> Result<Vec<PortRow>, Outcome> {
        let known = self.port_known()?;
        match tokio::task::spawn_blocking(move || ports::scan(known)).await {
            Ok(Ok(rows)) => Ok(rows),
            Ok(Err(e)) => Err(Outcome::error(ErrorCode::Internal, format!("{e:#}"))),
            Err(e) => Err(Outcome::error(ErrorCode::Internal, format!("port scan: {e}"))),
        }
    }

    /// Live sessions by root pid, workspace and worktree folders, workspace order. The live
    /// map is read and released before the store is locked: no nested locks.
    fn port_known(&self) -> Result<Known, Outcome> {
        let pids = self.hub.live_pids();
        let read = {
            let store = self.store();
            store.list_projects().and_then(|projects| {
                let tasks = store.list_tasks(None, None)?;
                let sessions = store.list_sessions(None)?;
                Ok((projects, tasks, sessions))
            })
        };
        let (projects, tasks, sessions) = read.map_err(|e| store_outcome::<()>(Err(e)))?;
        let projects: Vec<_> = projects.into_iter().filter(|p| !p.archived).collect();
        let worktree = |task_id: Option<_>| {
            tasks
                .iter()
                .find(|t| Some(t.id) == task_id && t.worktree_path.is_some())
        };
        let owners = pids
            .into_iter()
            .filter_map(|(session_id, pid)| {
                let session = sessions
                    .iter()
                    .find(|s| s.id == session_id && s.state != SessionState::Exited)?;
                let task = worktree(session.task_id);
                Some((
                    pid,
                    Owner {
                        session_id,
                        project_id: session.project_id,
                        worktree_task_id: task.map(|t| t.id),
                        branch: task.and_then(|t| t.branch.as_deref()).map(ports::short_branch),
                        session_title: session.title.clone(),
                        run_command: run::command_name(session).map(str::to_owned),
                    },
                ))
            })
            .collect();
        let mut folders: Vec<Folder> = projects
            .iter()
            .map(|p| Folder {
                path: PathBuf::from(&p.path),
                project_id: p.id,
                worktree_task_id: None,
                branch: None,
            })
            .collect();
        folders.extend(tasks.iter().filter_map(|t| {
            Some(Folder {
                path: PathBuf::from(t.worktree_path.as_ref()?),
                project_id: t.project_id,
                worktree_task_id: Some(t.id),
                branch: t.branch.as_deref().map(ports::short_branch),
            })
        }));
        let order: HashMap<_, _> = projects.iter().enumerate().map(|(i, p)| (p.id, i)).collect();
        Ok(Known {
            owners,
            folders,
            order,
            daemon_pid: std::process::id(),
        })
    }
}
```

`server.rs`: `mod ports;` after `mod git;`; in `match request`, next to `Request::SessionPorts`:

```rust
            Request::PortList => (self.port_list().await, After::Continue),
            Request::PortStop(params) => (self.port_stop(params).await, After::Continue),
```

`store_outcome` and `ok` are imported by `server/run.rs` through `use super::{Daemon, Start, ok, store_outcome};`, so they are reachable from the new child module the same way.

- [ ] **Step 7: Run the tests**

Run: `cargo test -p mkrew-daemon --test daemon ports::` then `cargo test -p mkrew-daemon --lib ports::`
Expected: all PASS.
If, on Windows, the run-command test sees the session exit but the port stays open, the existing run Stop does not reach the shell's child; report it as a finding instead of widening the kill (it is the Run menu's existing behaviour).

- [ ] **Step 8: ADR** `docs/decisions/0090-port-manager.md`:

```markdown
# 0090 - Port manager: OS socket table, parent walk, subtree kill

- Date: 2026-11-01
- Status: accepted
- Spec: docs/superpowers/specs/2026-11-01-port-manager-design.md

## Context

Several worktrees each run a dev server, and the Run pill shows only the ports its own run commands print (SPEC §10).
The user wants every listening port of the machine, with its workspace, worktree and pane, and a Stop.

## Decision

- The daemon reads the listening TCP sockets with the `listeners` crate (MIT; Windows, Linux and macOS) instead of three hand-written readers (`GetExtendedTcpTable`, `/proc/net/tcp`, `libproc`).
- `sysinfo` 0.31, already in the tree, gives the parent pid, start time and cwd of processes and kills them.
- A port belongs to the first live session root pid met walking up from its pid; meeting the daemon's pid first hides it (the daemon and `mk web`).
  A parent that started after its child is a reused pid and ends the walk.
  Without a session, the process cwd inside a workspace or task worktree folder (the longest wins) gives the workspace.
- `port.stop` lists again and acts only when the same pid still holds the port.
  A run command stops through its session kill; any other process is killed with its descendants, never its parents.
- No background scan and no event: clients ask (`port.list`) while they show the ports.

## Consequences

- One new crate in the daemon.
- A process of another user, or an elevated one, may hide its cwd: it shows outside Mkrew.
- A killed shell child leaves the pane open at its prompt.
```

- [ ] **Step 9: Commit**

```bash
git add Cargo.lock crates/mkrew-daemon crates/mkrew-fakeagent docs/decisions/0090-port-manager.md
git -c user.email=tungnv975@gmail.com commit -m "feat(daemon): port.list and port.stop over the OS socket table"
```

---

### Task 4: CLI `mk ports`

**Files:**
- Create: `crates/mkrew-cli/src/ports.rs`
- Modify: `crates/mkrew-cli/src/main.rs` (`enum Command`, lines 30-105; `mod ports;`; dispatch, lines 323-393)
- Test: `crates/mkrew-cli/tests/cli.rs`
- Modify: `docs/SPEC.md` §9 CLI block (after the `mk project stop` line, 734)

**Interfaces:**
- Consumes: `Request::PortList`, `Request::PortStop(PortStopParams)`, `PortRow`, `Request::ProjectList` -> `Vec<Project>`.
- Produces: `mk ports [--json]`, `mk ports stop <port> [--pid <pid>]`.

- [ ] **Step 1: Write the failing tests**

Bottom of `crates/mkrew-cli/src/ports.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn row(port: u16, pid: u32) -> PortRow {
        PortRow {
            port,
            addr: "0.0.0.0".into(),
            pid,
            process: "node".into(),
            project_id: None,
            worktree_task_id: None,
            branch: None,
            session_id: None,
            session_title: None,
            run_command: None,
        }
    }

    #[test]
    fn the_holder_is_the_one_pid_on_the_port() {
        assert_eq!(holder(&[row(3000, 7), row(4000, 8)], 3000).unwrap(), 7);
        assert_eq!(
            holder(&[row(4000, 8)], 3000).unwrap_err().to_string(),
            "nothing listens on :3000"
        );
        assert_eq!(
            holder(&[row(3000, 9), row(3000, 7)], 3000).unwrap_err().to_string(),
            "several processes listen on :3000 (pids 7, 9); pick one with --pid"
        );
    }

    #[test]
    fn the_table_names_workspace_and_place() {
        let id = Ulid(1);
        let mut in_worktree = row(5173, 12);
        in_worktree.project_id = Some(id);
        in_worktree.branch = Some("fix".into());
        in_worktree.run_command = Some("web".into());
        assert_eq!(
            table(&[in_worktree, row(5432, 40)], &[(id, "app".to_owned())]),
            "PORT   WORKSPACE  WHERE  SOURCE  PID\n\
             :5173  app        fix    web     12\n\
             :5432  -          -      node    40\n"
        );
    }
}
```

`tests/cli.rs` (add `use std::io::BufRead;` and `use std::path::Path;` if missing):

```rust
#[test]
fn ports_lists_and_stops_a_listening_process() {
    let env = Env::new();
    env.ok(&["daemon", "start"]);
    let fake = Path::new(env!("CARGO_BIN_EXE_mk"))
        .with_file_name(format!("mkrew-fakeagent{}", std::env::consts::EXE_SUFFIX));
    let mut server = std::process::Command::new(fake)
        .arg("--listen")
        .stdout(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    let mut line = String::new();
    std::io::BufReader::new(server.stdout.take().unwrap())
        .read_line(&mut line)
        .unwrap();
    let port: u16 = line
        .trim()
        .trim_start_matches("Listening on http://127.0.0.1:")
        .trim_end_matches('/')
        .parse()
        .unwrap();

    let listed: Vec<serde_json::Value> =
        serde_json::from_str(&env.ok(&["ports", "--json"])).unwrap();
    let mine = listed
        .iter()
        .find(|r| r["port"] == port)
        .expect("the fake server is listed");
    assert_eq!(mine["pid"], server.id());
    assert!(mine.get("project_id").is_none(), "outside Mkrew");

    let stopped = env.ok(&["ports", "stop", &port.to_string()]);
    assert_eq!(stopped.trim(), format!("stopped pid {} on :{port}", server.id()));
    server.wait().unwrap();
    assert!(!env.mk(&["ports", "stop", &port.to_string()]).status.success());
    env.ok(&["daemon", "stop"]);
}
```

Run: `cargo test -p mkrew-cli ports` - compile errors.

- [ ] **Step 2: Implement** `crates/mkrew-cli/src/ports.rs` (use the same `Client` import path as `src/project.rs`):

```rust
//! `mk ports`: every listening TCP port of the daemon's machine, and `mk ports stop`
//! (spec 2026-11-01, SPEC §9).

use std::fmt::Write as _;

use anyhow::bail;
use mkrew_protocol::{PortRow, PortStopParams, Project, Request};
use ulid::Ulid;

use crate::client::Client;

pub fn list(client: &mut Client, json: bool) -> anyhow::Result<()> {
    let rows: Vec<PortRow> = client.call(Request::PortList)?;
    if json {
        println!("{}", serde_json::to_string_pretty(&rows)?);
        return Ok(());
    }
    if rows.is_empty() {
        println!("no ports are listening");
        return Ok(());
    }
    let projects: Vec<Project> = client.call(Request::ProjectList)?;
    let names: Vec<(Ulid, String)> = projects.into_iter().map(|p| (p.id, p.name)).collect();
    print!("{}", table(&rows, &names));
    Ok(())
}

pub fn stop(client: &mut Client, port: u16, pid: Option<u32>) -> anyhow::Result<()> {
    let pid = match pid {
        Some(pid) => pid,
        None => holder(&client.call::<Vec<PortRow>>(Request::PortList)?, port)?,
    };
    let () = client.call(Request::PortStop(PortStopParams { port, pid }))?;
    println!("stopped pid {pid} on :{port}");
    Ok(())
}

/// The one pid listening on `port`.
fn holder(rows: &[PortRow], port: u16) -> anyhow::Result<u32> {
    let mut pids: Vec<u32> = rows.iter().filter(|r| r.port == port).map(|r| r.pid).collect();
    pids.sort_unstable();
    pids.dedup();
    match pids.as_slice() {
        [] => bail!("nothing listens on :{port}"),
        [pid] => Ok(*pid),
        many => {
            let list: Vec<String> = many.iter().map(u32::to_string).collect();
            bail!(
                "several processes listen on :{port} (pids {}); pick one with --pid",
                list.join(", ")
            )
        }
    }
}

/// `PORT  WORKSPACE  WHERE  SOURCE  PID`; every column but the last padded to its widest
/// cell; `-` for none.
fn table(rows: &[PortRow], projects: &[(Ulid, String)]) -> String {
    let header = ["PORT", "WORKSPACE", "WHERE", "SOURCE", "PID"].map(str::to_owned);
    let cells: Vec<[String; 5]> = rows
        .iter()
        .map(|r| {
            let workspace = r
                .project_id
                .and_then(|id| projects.iter().find(|(p, _)| *p == id))
                .map_or_else(|| "-".to_owned(), |(_, name)| name.clone());
            let place = match (&r.branch, r.project_id) {
                (Some(branch), _) => branch.clone(),
                (None, Some(_)) => "main".to_owned(),
                (None, None) => "-".to_owned(),
            };
            let source = r
                .run_command
                .clone()
                .or_else(|| r.session_title.clone())
                .unwrap_or_else(|| r.process.clone());
            [format!(":{}", r.port), workspace, place, source, r.pid.to_string()]
        })
        .collect();
    let all: Vec<&[String; 5]> = std::iter::once(&header).chain(&cells).collect();
    let width = |i: usize| all.iter().map(|c| c[i].chars().count()).max().unwrap_or(0);
    let (w0, w1, w2, w3) = (width(0), width(1), width(2), width(3));
    let mut out = String::new();
    for c in all {
        // Writing to a String cannot fail.
        let _ = writeln!(out, "{:<w0$}  {:<w1$}  {:<w2$}  {:<w3$}  {}", c[0], c[1], c[2], c[3], c[4]);
    }
    out
}
```

`main.rs`: `mod ports;`; in `enum Command`:

```rust
    /// Listening TCP ports of the daemon's machine with their workspace; `stop` ends one.
    Ports {
        #[command(subcommand)]
        command: Option<PortsCommand>,
        /// Print the rows as JSON (`port.list`).
        #[arg(long)]
        json: bool,
    },
```

next to `SessionCommand`:

```rust
#[derive(Subcommand)]
enum PortsCommand {
    /// Stops the process listening on a port (a run command's session ends).
    Stop {
        port: u16,
        /// The pid to stop when several processes listen on the port.
        #[arg(long)]
        pid: Option<u32>,
    },
}
```

dispatch, next to `Command::Session`:

```rust
        Command::Ports { command, json } => {
            let mut client = project::connect(&paths)?;
            match command {
                None => ports::list(&mut client, json),
                Some(PortsCommand::Stop { port, pid }) => ports::stop(&mut client, port, pid),
            }
            .map(|()| ExitCode::SUCCESS)
        }
```

- [ ] **Step 3: Run the tests**

Run: `cargo test -p mkrew-cli ports` and `cargo test -p mkrew-cli --test cli ports_lists_and_stops`
Expected: PASS.

- [ ] **Step 4: SPEC §9 CLI block**, after the `mk project stop` line:

```
mk ports [--json]                      # listening TCP ports of the daemon's machine: port, workspace, main or worktree branch, source, pid
mk ports stop <port> [--pid <pid>]     # stops the process on that port (a run command's session ends); --pid when several listen
```

- [ ] **Step 5: Commit**

```bash
git add crates/mkrew-cli docs/SPEC.md
git -c user.email=tungnv975@gmail.com commit -m "feat(cli): mk ports and mk ports stop"
```

---

### Task 5: UI model: groups, labels, selection, Stop question

**Files:**
- Create: `crates/mkrew-ui/src/ports.rs`
- Modify: `crates/mkrew-ui/src/lib.rs` (`pub mod ports;`, alphabetical)

**Interfaces:**
- Consumes: `PortRow`, `Project`, `ErrorBody`, `ErrorCode` (`mkrew_protocol`).
- Produces (`mkrew_ui::ports`): `OUTSIDE`, `Group { title, rows }`, `groups(&[PortRow], &[Project]) -> Vec<Group>`, `place(&PortRow) -> String`, `source(&PortRow) -> String`, `asks_first(&PortRow) -> bool`, `question(&PortRow) -> String`, `stop_failure(u32, &ErrorBody) -> Option<String>`, `Key { Close, Move(isize), Open, Stop }`, `key(&Keystroke) -> Option<Key>`, `PortsDialog { rows, loaded, error, selected: Option<(u16, u32)>, confirm: Option<PortRow> }` with `begin_poll() -> bool`, `listed(Result<Vec<PortRow>, String>)`, `select_by(isize, &[Project])`, `selected_row() -> Option<&PortRow>`.

- [ ] **Step 1: Write the failing tests** at the bottom of `ports.rs`.
Read `Project` (`crates/mkrew-protocol/src/message.rs:497-533`) first: if it has no `Default`, build the test projects with `serde_json::from_value(json!({...}))` carrying its required fields instead of `..Project::default()`.

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn project(id: u128, name: &str) -> Project {
        Project {
            id: Ulid(id),
            name: name.into(),
            ..Project::default()
        }
    }

    fn row(port: u16, pid: u32, project_id: Option<u128>) -> PortRow {
        PortRow {
            port,
            addr: "0.0.0.0".into(),
            pid,
            process: "node.exe".into(),
            project_id: project_id.map(Ulid),
            worktree_task_id: None,
            branch: None,
            session_id: None,
            session_title: None,
            run_command: None,
        }
    }

    #[test]
    fn groups_follow_the_sidebar_and_end_outside() {
        let projects = [project(2, "api"), project(1, "app")];
        let mut worktree = row(5000, 3, Some(1));
        worktree.branch = Some("fix".into());
        let rows = [
            row(9000, 1, None),
            row(4000, 2, Some(1)),
            worktree,
            row(8080, 4, Some(2)),
            row(7000, 5, Some(99)),
        ];
        let shown: Vec<(String, Vec<u16>)> = groups(&rows, &projects)
            .into_iter()
            .map(|g| (g.title, g.rows.iter().map(|r| r.port).collect()))
            .collect();
        assert_eq!(
            shown,
            [
                ("API".to_owned(), vec![8080]),
                ("APP".to_owned(), vec![4000, 5000]),
                (OUTSIDE.to_owned(), vec![7000, 9000]),
            ]
        );
    }

    #[test]
    fn labels_say_where_and_what() {
        let mut run = row(5173, 7, Some(1));
        run.branch = Some("fix".into());
        run.session_id = Some(Ulid(3));
        run.session_title = Some("run:web".into());
        run.run_command = Some("web".into());
        assert_eq!((place(&run), source(&run)), ("⎇ fix".to_owned(), "web".to_owned()));
        let main = row(3000, 8, Some(1));
        assert_eq!((place(&main), source(&main)), ("main".to_owned(), "node.exe".to_owned()));
        assert_eq!(place(&row(1, 1, None)), "");
    }

    #[test]
    fn only_a_run_command_stops_without_asking() {
        let mut run = row(5173, 7, Some(1));
        run.session_id = Some(Ulid(3));
        run.run_command = Some("web".into());
        assert!(!asks_first(&run));
        let mut pane = row(3000, 8, Some(1));
        pane.session_id = Some(Ulid(4));
        assert!(asks_first(&pane));
        assert_eq!(question(&pane), "Stop node.exe (pid 8) on :3000? The pane stays open.");
        assert_eq!(question(&row(5432, 9, None)), "Stop node.exe (pid 9) on :5432?");
    }

    #[test]
    fn a_vanished_port_is_no_error() {
        let gone = ErrorBody {
            code: ErrorCode::NotFound,
            message: "gone".into(),
        };
        assert_eq!(stop_failure(4, &gone), None);
        let refused = ErrorBody {
            code: ErrorCode::InvalidState,
            message: "access denied".into(),
        };
        assert_eq!(
            stop_failure(4, &refused).as_deref(),
            Some("Can't stop pid 4: access denied")
        );
    }

    #[test]
    fn selection_moves_in_shown_order_and_survives_a_refresh() {
        let projects = [project(1, "app")];
        let mut dialog = PortsDialog::default();
        assert!(dialog.begin_poll());
        assert!(!dialog.begin_poll(), "one poll at a time");
        dialog.listed(Ok(vec![row(9000, 1, None), row(3000, 2, Some(1))]));
        dialog.select_by(1, &projects);
        assert_eq!(dialog.selected, Some((3000, 2)), "first shown row");
        dialog.select_by(1, &projects);
        assert_eq!(dialog.selected, Some((9000, 1)));
        dialog.select_by(1, &projects);
        assert_eq!(dialog.selected, Some((9000, 1)), "stays on the last");
        dialog.confirm = Some(row(9000, 1, None));
        assert!(dialog.begin_poll());
        dialog.listed(Ok(vec![row(3000, 2, Some(1))]));
        assert_eq!(dialog.selected, None, "its port went away");
        assert_eq!(dialog.confirm, None);
        dialog.listed(Err("daemon busy".into()));
        assert_eq!(dialog.rows.len(), 1, "rows stay on an error");
        assert_eq!(dialog.error.as_deref(), Some("daemon busy"));
    }
}
```

Run: `cargo test -p mkrew-ui --lib ports::` - compile errors.

- [ ] **Step 2: Implement** `crates/mkrew-ui/src/ports.rs`:

```rust
//! The Ports dialog's model (spec 2026-11-01): `port.list` rows grouped by workspace, the
//! selection and the Stop question. Pure; `shell/ports.rs` renders it.

use gpui::Keystroke;
use mkrew_protocol::{ErrorBody, ErrorCode, PortRow, Project};
use ulid::Ulid;

/// Heading of the ports no shown workspace owns.
pub const OUTSIDE: &str = "OUTSIDE MKREW";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Group {
    pub title: String,
    pub rows: Vec<PortRow>,
}

/// One group per workspace with ports, in `projects` (sidebar) order, main folder before
/// worktrees, then by port; the rest (no workspace, or one not shown) last under [`OUTSIDE`].
pub fn groups(rows: &[PortRow], projects: &[Project]) -> Vec<Group> {
    let mut groups: Vec<Group> = projects
        .iter()
        .filter_map(|project| {
            let mut mine: Vec<PortRow> = rows
                .iter()
                .filter(|r| r.project_id == Some(project.id))
                .cloned()
                .collect();
            mine.sort_by(|a, b| {
                (a.branch.is_some(), &a.branch, a.port, a.pid)
                    .cmp(&(b.branch.is_some(), &b.branch, b.port, b.pid))
            });
            (!mine.is_empty()).then(|| Group {
                title: project.name.to_uppercase(),
                rows: mine,
            })
        })
        .collect();
    let shown = |id: Option<Ulid>| id.is_some_and(|id| projects.iter().any(|p| p.id == id));
    let mut outside: Vec<PortRow> = rows.iter().filter(|r| !shown(r.project_id)).cloned().collect();
    if !outside.is_empty() {
        outside.sort_by_key(|r| (r.port, r.pid));
        groups.push(Group {
            title: OUTSIDE.to_owned(),
            rows: outside,
        });
    }
    groups
}

/// `⎇ <branch>` in a task worktree, `main` in the workspace folder, empty outside.
pub fn place(row: &PortRow) -> String {
    match (&row.branch, row.project_id) {
        (Some(branch), _) => format!("⎇ {branch}"),
        (None, Some(_)) => "main".to_owned(),
        (None, None) => String::new(),
    }
}

/// The run command, else the session title, else the process name.
pub fn source(row: &PortRow) -> String {
    row.run_command
        .clone()
        .or_else(|| row.session_title.clone())
        .unwrap_or_else(|| row.process.clone())
}

/// Every Stop asks first except a run command's (it stops like ■ in the Run menu).
pub fn asks_first(row: &PortRow) -> bool {
    row.run_command.is_none()
}

pub fn question(row: &PortRow) -> String {
    let ask = format!("Stop {} (pid {}) on :{}?", row.process, row.pid, row.port);
    if row.session_id.is_some() {
        format!("{ask} The pane stays open.")
    } else {
        ask
    }
}

/// `not_found` means the port went away meanwhile: the next list shows it, no error.
pub fn stop_failure(pid: u32, body: &ErrorBody) -> Option<String> {
    (body.code != ErrorCode::NotFound).then(|| format!("Can't stop pid {pid}: {}", body.message))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    Close,
    Move(isize),
    Open,
    Stop,
}

pub fn key(keystroke: &Keystroke) -> Option<Key> {
    let modified = keystroke.modifiers.modified();
    match keystroke.key.as_str() {
        "escape" => Some(Key::Close),
        "up" if !modified => Some(Key::Move(-1)),
        "down" if !modified => Some(Key::Move(1)),
        "enter" if !modified => Some(Key::Open),
        "delete" if !modified => Some(Key::Stop),
        _ => None,
    }
}

/// The open dialog. A row is known by `(port, pid)` across refreshes.
#[derive(Debug, Default)]
pub struct PortsDialog {
    pub rows: Vec<PortRow>,
    /// A first answer came (until then the dialog says "Loading…").
    pub loaded: bool,
    pub error: Option<String>,
    pub selected: Option<(u16, u32)>,
    /// The row whose Stop question shows in the footer.
    pub confirm: Option<PortRow>,
    polling: bool,
}

impl PortsDialog {
    /// `false` while a `port.list` is in flight: that tick skips.
    pub fn begin_poll(&mut self) -> bool {
        !std::mem::replace(&mut self.polling, true)
    }

    pub fn listed(&mut self, listed: Result<Vec<PortRow>, String>) {
        self.polling = false;
        self.loaded = true;
        match listed {
            Ok(rows) => {
                self.rows = rows;
                self.error = None;
            }
            Err(error) => self.error = Some(error),
        }
        let alive = |rows: &[PortRow], port: u16, pid: u32| {
            rows.iter().any(|r| r.port == port && r.pid == pid)
        };
        if self.selected.is_some_and(|(port, pid)| !alive(&self.rows, port, pid)) {
            self.selected = None;
        }
        if self.confirm.as_ref().is_some_and(|r| !alive(&self.rows, r.port, r.pid)) {
            self.confirm = None;
        }
    }

    /// Moves the selection through the rows in shown order, without wrapping. With no
    /// selection, down picks the first row and up the last.
    pub fn select_by(&mut self, offset: isize, projects: &[Project]) {
        let order: Vec<(u16, u32)> = groups(&self.rows, projects)
            .into_iter()
            .flat_map(|g| g.rows)
            .map(|r| (r.port, r.pid))
            .collect();
        let Some(last) = order.len().checked_sub(1) else {
            return;
        };
        let at = match self.selected.and_then(|id| order.iter().position(|o| *o == id)) {
            Some(at) => at.saturating_add_signed(offset).min(last),
            None if offset < 0 => last,
            None => 0,
        };
        self.selected = Some(order[at]);
    }

    pub fn selected_row(&self) -> Option<&PortRow> {
        let (port, pid) = self.selected?;
        self.rows.iter().find(|r| r.port == port && r.pid == pid)
    }
}
```

`lib.rs`: `pub mod ports;` (alphabetical).

- [ ] **Step 3: Run the tests**

Run: `cargo test -p mkrew-ui --lib ports::`
Expected: 5 tests PASS.

- [ ] **Step 4: Commit**

```bash
git add crates/mkrew-ui/src/ports.rs crates/mkrew-ui/src/lib.rs
git -c user.email=tungnv975@gmail.com commit -m "feat(ui): ports dialog model"
```

---

### Task 6: UI: Ports dialog, palette command, Run menu row

Read `skill://mkrew-ui-design` before this task (repo rule for any `crates/mkrew-ui` change).

**Files:**
- Create: `crates/mkrew-ui/src/shell/ports.rs`
- Modify: `crates/mkrew-ui/src/shell.rs` (`mod ports;`)
- Modify: `crates/mkrew-ui/src/main.rs` (field after `phone_open`, line 519-522; `mkrew_ui` imports, lines 46-65; overlay after `render_phone_access`, lines 1013-1016)
- Modify: `crates/mkrew-ui/src/shell/connection.rs` (`ports: None,` next to `phone_open: false,` at line 306; `self.ports = None;` next to `self.phone_open = false;` at line 781)
- Modify: `crates/mkrew-ui/src/shell/focus.rs:7-28` (`modal_open`)
- Modify: `crates/mkrew-ui/src/theme.rs`
- Modify: `crates/mkrew-ui/src/palette.rs`, `crates/mkrew-ui/src/palette/tests.rs`, `crates/mkrew-ui/src/shell/navigation.rs` (line 668)
- Modify: `crates/mkrew-ui/src/shell/run.rs` (end of `run_menu_rows`, line 538)
- Modify: `docs/SPEC.md` §10 (after line 1062), `docs/HUMAN_CHECKS.md`

**Interfaces:**
- Consumes: Task 5 `mkrew_ui::ports::*`; `Request::PortList`, `Request::PortStop`.
- Produces: `Shell::open_ports(&mut self, window: &mut Window, cx: &mut Context<'_, Self>)`.

- [ ] **Step 1: Palette test first** in `palette/tests.rs`, after `check_updates_is_listed_without_a_workspace`:

```rust
#[test]
fn ports_is_listed_without_a_workspace_and_in_a_machine_window() {
    let none = Context {
        workspace: false,
        workspaces: 0,
        tabs: 0,
        machine: true,
        ..context(View::Terminals)
    };
    let listed = entries(&Keymap::defaults(), &none, &[], &[], &[]);
    let entry = listed
        .iter()
        .find(|e| e.action == Action::Extra(Extra::Ports))
        .expect("listed without a workspace");
    assert_eq!(entry.title, "Ports: Show…");
}
```

In `fn extras`, add `Extra::Ports` to `SEPARATE` (`[Extra; 9]` -> `[Extra; 10]`).

- [ ] **Step 2: Palette** (`palette.rs`): variant `Ports,` after `RunShowTab,`; `Self::Ports,` after `Self::RunShowTab,` in `ALL` (`[Self; 41]` -> `[Self; 42]`); in `title()` `Self::Ports => "Ports: Show…",`; in `available()` `Self::Ports => true,`.
`shell/navigation.rs`, with the workspace-free arms next to `Extra::PhoneAccess` (line 668):

```rust
            palette::Action::Extra(Extra::Ports) => self.open_ports(window, cx),
```

- [ ] **Step 3: Theme tokens** (`theme.rs`), next to `TURNS_LIST_HEIGHT`:

```rust
/// The Ports dialog (spec 2026-11-01): its width and the height its rows scroll past.
pub const PORTS_DIALOG_WIDTH: f32 = 640.0;
pub const PORTS_LIST_HEIGHT: f32 = 400.0;
```

next to `PHONE_TICK`:

```rust
/// While the Ports dialog is open, `port.list` is asked on every tick (spec 2026-11-01).
pub const PORTS_TICK: Duration = Duration::from_secs(2);
```

- [ ] **Step 4: Shell module** `crates/mkrew-ui/src/shell/ports.rs`:

```rust
//! The Ports dialog (spec 2026-11-01): every listening port of the daemon's machine by
//! workspace, a jump to its pane, and Stop.

use mkrew_protocol::{PortRow, PortStopParams};
use mkrew_ui::ports::{self, PortsDialog};

use crate::*;

impl Shell {
    /// Opens the dialog and asks `port.list` now and every [`theme::PORTS_TICK`] until it
    /// closes; nothing runs while it is closed.
    pub(crate) fn open_ports(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        if self.connection.is_none() {
            return;
        }
        self.close_menus();
        self.ports = Some(PortsDialog::default());
        self.dialog_serial += 1;
        let serial = self.dialog_serial;
        window.focus(&self.dialog_focus, cx);
        cx.notify();
        self.poll_ports(window, cx);
        cx.spawn_in(window, async move |this, cx| {
            loop {
                cx.background_executor().timer(theme::PORTS_TICK).await;
                let open = this.update_in(cx, |shell, window, cx| {
                    let open = shell.ports.is_some() && shell.dialog_serial == serial;
                    if open {
                        shell.poll_ports(window, cx);
                    }
                    open
                });
                if !matches!(open, Ok(true)) {
                    return;
                }
            }
        })
        .detach();
    }

    pub(crate) fn close_ports(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        self.ports = None;
        self.focus_pane(window, cx);
        cx.notify();
    }

    fn poll_ports(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        let Some(connection) = self.connection.clone() else {
            return;
        };
        let Some(dialog) = self.ports.as_mut() else {
            return;
        };
        if !dialog.begin_poll() {
            return;
        }
        let listed = connection.call::<Vec<PortRow>>(Request::PortList);
        cx.spawn_in(window, async move |this, cx| {
            let listed = listed.await.map_err(|error| format!("{error:#}"));
            let _ = this.update(cx, |shell, cx| {
                if let Some(dialog) = shell.ports.as_mut() {
                    dialog.listed(listed);
                    cx.notify();
                }
            });
        })
        .detach();
    }

    /// ■ or Delete on a row: a run command stops at once, any other row asks in the footer.
    fn ask_port_stop(&mut self, row: PortRow, window: &mut Window, cx: &mut Context<'_, Self>) {
        if ports::asks_first(&row) {
            if let Some(dialog) = self.ports.as_mut() {
                dialog.confirm = Some(row);
            }
            cx.notify();
        } else {
            self.stop_port(row.port, row.pid, window, cx);
        }
    }

    fn stop_port(&mut self, port: u16, pid: u32, window: &mut Window, cx: &mut Context<'_, Self>) {
        let Some(connection) = self.connection.clone() else {
            return;
        };
        if let Some(dialog) = self.ports.as_mut() {
            dialog.confirm = None;
        }
        cx.notify();
        let stopped = connection.request(Request::PortStop(PortStopParams { port, pid }));
        cx.spawn_in(window, async move |this, cx| {
            let failure = match stopped.await {
                Ok(Outcome::Result(_)) => None,
                Ok(Outcome::Error(body)) => ports::stop_failure(pid, &body),
                Err(error) => Some(format!("Can't stop pid {pid}: {error:#}")),
            };
            let _ = this.update_in(cx, |shell, window, cx| {
                if let Some(dialog) = shell.ports.as_mut() {
                    dialog.error = failure;
                }
                shell.poll_ports(window, cx);
            });
        })
        .detach();
    }

    /// A row with a session: its workspace and pane; the dialog closes.
    fn go_to_port(&mut self, row: &PortRow, window: &mut Window, cx: &mut Context<'_, Self>) {
        let Some(session) = row.session_id else {
            return;
        };
        self.ports = None;
        self.open_session(session, window, cx);
        cx.notify();
    }

    pub(crate) fn ports_key_down(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        if self.dialog_nav(event, true, window, cx, |_, _| {}) {
            return;
        }
        cx.stop_propagation();
        let Some(dialog) = self.ports.as_mut() else {
            return;
        };
        let asking = dialog.confirm.is_some();
        match ports::key(&event.keystroke) {
            Some(ports::Key::Close) if asking => {
                dialog.confirm = None;
                cx.notify();
            }
            Some(ports::Key::Close) => self.close_ports(window, cx),
            // While the footer asks, only Tab, its buttons and Esc act.
            _ if asking => {}
            Some(ports::Key::Move(offset)) => {
                dialog.select_by(offset, self.sidebar.projects());
                cx.notify();
            }
            Some(ports::Key::Open) => {
                if let Some(row) = dialog.selected_row().cloned() {
                    self.go_to_port(&row, window, cx);
                }
            }
            Some(ports::Key::Stop) => {
                if let Some(row) = dialog.selected_row().cloned() {
                    self.ask_port_stop(row, window, cx);
                }
            }
            None => {}
        }
    }

    pub(crate) fn render_ports(&self, theme: &Theme, cx: &mut Context<'_, Self>) -> gpui::AnyElement {
        let Some(dialog) = &self.ports else {
            return div().into_any_element();
        };
        let hover = theme.hover;
        let mut list = div()
            .id("ports-list")
            .flex()
            .flex_col()
            .gap(px(space::XS))
            .max_h(px(theme::PORTS_LIST_HEIGHT))
            .overflow_y_scroll();
        let mut index = 0usize;
        for group in ports::groups(&dialog.rows, self.sidebar.projects()) {
            list = list.child(
                div()
                    .pt(px(space::S))
                    .text_size(font_size::OVERLINE)
                    .font_weight(FontWeight(weight::SEMIBOLD))
                    .text_color(rgb(theme.text_muted))
                    .child(group.title),
            );
            for row in group.rows {
                let selected = dialog.selected == Some((row.port, row.pid));
                let clickable = row.session_id.is_some();
                let go = row.clone();
                let stop = row.clone();
                list = list.child(
                    div()
                        .id(SharedString::from(format!("port-row-{index}")))
                        .flex()
                        .items_center()
                        .gap(px(space::M))
                        .px(px(space::S))
                        .py(px(space::XS))
                        .rounded(px(radius::SMALL))
                        .text_size(font_size::BODY)
                        .when(selected, |el| el.bg(rgb(theme.surface_raised)))
                        .when(!selected && clickable, |el| {
                            el.cursor_pointer().hover(move |style| style.bg(rgb(hover)))
                        })
                        .on_click(cx.listener(move |shell, _, window, cx| {
                            shell.go_to_port(&go, window, cx);
                        }))
                        .child(
                            div()
                                .w(px(space::XXL * 2.0))
                                .font_weight(FontWeight(weight::MEDIUM))
                                .text_color(rgb(theme.text))
                                .child(format!(":{}", row.port)),
                        )
                        .child(div().text_color(rgb(theme.text_dim)).child(ports::place(&row)))
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .truncate()
                                .text_color(rgb(theme.text))
                                .child(ports::source(&row)),
                        )
                        .child(
                            div()
                                .text_size(font_size::META)
                                .text_color(rgb(theme.text_muted))
                                .child(format!("pid {}", row.pid)),
                        )
                        .child(
                            div()
                                .id(SharedString::from(format!("port-stop-{index}")))
                                .px(px(space::XS))
                                .rounded(px(radius::SMALL))
                                .cursor_pointer()
                                .text_color(rgb(theme.danger_text))
                                .hover(|style| style.bg(rgb(theme.danger_bg)))
                                .tooltip(tooltip(theme, "Stop", "Delete"))
                                .on_click(cx.listener(move |shell, _, window, cx| {
                                    cx.stop_propagation();
                                    shell.ask_port_stop(stop.clone(), window, cx);
                                }))
                                .child("■"),
                        ),
                );
                index += 1;
            }
        }
        let muted = |text: &'static str| div().text_color(rgb(theme.text_muted)).child(text);
        let body = if !dialog.loaded {
            muted("Loading…").into_any_element()
        } else if dialog.rows.is_empty() {
            muted("No ports are listening.").into_any_element()
        } else {
            list.into_any_element()
        };
        let error = dialog.error.clone().map(|text| {
            div()
                .flex()
                .gap(px(space::XS))
                .text_size(font_size::META)
                .child(div().text_color(rgb(theme.status_exited)).child("✕"))
                .child(div().text_color(rgb(theme.text)).child(text))
        });
        let footer = match &dialog.confirm {
            Some(row) => {
                let (port, pid) = (row.port, row.pid);
                div()
                    .flex()
                    .items_center()
                    .gap(px(space::S))
                    .child(div().flex_1().text_color(rgb(theme.text)).child(ports::question(row)))
                    .child(
                        button("port-cancel", "Cancel", theme, false)
                            .focus_stop(theme)
                            .tooltip(tooltip(theme, "Keep it running", "Esc"))
                            .on_click(cx.listener(|shell, _, _, cx| {
                                if let Some(dialog) = shell.ports.as_mut() {
                                    dialog.confirm = None;
                                }
                                cx.notify();
                            })),
                    )
                    .child(
                        danger_button("port-stop", "Stop", theme)
                            .focus_stop(theme)
                            .on_click(cx.listener(move |shell, _, window, cx| {
                                shell.stop_port(port, pid, window, cx);
                            })),
                    )
            }
            None => div().flex().justify_end().child(
                button("ports-close", "Close", theme, false)
                    .focus_stop(theme)
                    .tooltip(tooltip(theme, "Close", "Esc"))
                    .on_click(cx.listener(|shell, _, window, cx| shell.close_ports(window, cx))),
            ),
        };
        let panel = div()
            .id("ports-dialog")
            .w(px(theme::PORTS_DIALOG_WIDTH))
            .flex()
            .flex_col()
            .gap(px(space::M))
            .p(px(space::XL))
            .rounded(px(radius::LARGE))
            .border(px(theme::BORDER_WIDTH))
            .border_color(rgb(theme.border))
            .bg(rgb(theme.surface))
            .shadow_md()
            .font_family(theme.ui_font_family.clone())
            .dialog_root(
                &self.dialog_focus,
                false,
                cx.listener(Self::dialog_guard),
                cx.listener(Self::ports_key_down),
            )
            .child(
                div()
                    .text_size(font_size::CARD_TITLE)
                    .font_weight(FontWeight(weight::SEMIBOLD))
                    .text_color(rgb(theme.text))
                    .child("Ports"),
            )
            .children(error)
            .child(body)
            .child(footer);
        Self::modal_shade("ports-shade", panel, cx)
    }
}
```

`px(space::XXL * 2.0)` follows `shell/run.rs:959`: a token expression, not a raw number.
Before building, check every name against its definition: `theme.hover`, `theme.surface_raised`, `theme.text_dim`, `theme.text_muted`, `theme.danger_text`, `theme.danger_bg`, `theme.status_exited` (`theme.rs`); `button`, `danger_button`, `tooltip`, `DialogControl` (`shell/widgets.rs`); `Outcome` and `Request` reachable through `crate::*` (else import from `mkrew_protocol`).

Wiring:
- `shell.rs`: `mod ports;` (alphabetical).
- `main.rs`: add `ports::PortsDialog` to the `mkrew_ui` imports; field after `phone_open`:
  ```rust
      /// The Ports dialog (spec 2026-11-01), while open.
      ports: Option<PortsDialog>,
  ```
  overlay after the phone access `.children(...)`:
  ```rust
              .children(self.ports.is_some().then(|| self.render_ports(&theme, cx)))
  ```
- `connection.rs`: `ports: None,` in the initializer; `self.ports = None;` in the disconnect reset.
- `focus.rs` `modal_open`: `|| self.ports.is_some()` after `|| self.phone_open`.

- [ ] **Step 5: Run menu row** in `shell/run.rs`, after the `Run setup…` row, before `rows`:

```rust
        rows.push(
            MenuItem::new(
                "All ports…",
                act(move |shell, window, cx| shell.open_ports(window, cx)),
            )
            .lead(Lead::Icon(Icon::Server))
            .row(),
        );
```

If an insta snapshot lists the Run menu rows, review it with `cargo insta review` and accept only when the sole change is the added row.

- [ ] **Step 6: Build and test**

Run: `cargo test -p mkrew-ui`
Expected: PASS, the palette test of Step 1 included.

- [ ] **Step 7: SPEC §10 and HUMAN_CHECKS**

SPEC §10, after the line ending `Restart / Stop act only while a run session lives; Start with nothing set up opens Run setup.` (line 1062):

```markdown
- **Ports dialog** (spec 2026-11-01, ADR 0090): every listening TCP port of the daemon's machine, opened by `Ports: Show…` in the palette and `All ports…` at the end of the Run menu.
  One group per workspace with ports (sidebar order), then `OUTSIDE MKREW`; a row shows `:port`, `main` or `⎇ <branch>`, the run command (else the pane title, else the process name), `pid N` and a red ■.
  A click on a row with a pane opens that pane and closes the dialog.
  ■ (or Delete on the selected row) stops a run command at once; any other row asks in the footer ("Stop node.exe (pid 1234) on :3000? The pane stays open.") with Cancel and a red Stop that runs only after Tab moved onto it; Esc cancels the question, then closes the dialog.
  `port.list` is asked when it opens, every 2 s while it is open and after each Stop; nothing runs while it is closed; it closes on disconnect.
  `↑` / `↓` select, Enter opens the pane.
```

Top of `## To verify (non-blocking)` in `docs/HUMAN_CHECKS.md`:

```markdown
### To verify - 2026-11-01 Ports dialog

- [ ] **Restart the daemon first** (protocol 51).
- [ ] **Look (dark + light):** `Ports: Show…` opens a 640 px dialog; groups by workspace in sidebar order, `OUTSIDE MKREW` last; each row `:port`, `main` / `⎇ branch`, source, muted `pid N`, a red ■ on the right; a long source truncates.
- [ ] **Run menu:** `All ports…` is the last row, under `Run setup…`, with the server icon.
- [ ] **Stop a run command:** ■ stops it at once; the Run pill updates and the row goes within 2 s.
- [ ] **Stop a shell pane server:** start a dev server in a shell pane, open Ports, ■ asks in the footer; Enter does nothing, Tab to Stop then Enter stops it; the pane stays at its prompt.
- [ ] **Outside Mkrew:** a server started in a plain terminal outside every workspace shows under `OUTSIDE MKREW`; started inside a workspace folder it shows under that workspace with no pane (a click does nothing).
- [ ] **Worktree:** a task worktree's dev server shows `⎇ <branch>` and a click opens its pane.
- [ ] **Keyboard:** `↑` / `↓` select, Enter opens the pane, Delete asks, Esc cancels the question, then closes.
```

- [ ] **Step 8: Full verify and smoke**

Run: `"C:/Program Files/Git/bin/bash.exe" scripts/verify.sh`
Expected: fmt, clippy `-D warnings` and every test pass.
Run: `git diff master -- crates/mkrew-ui | grep -nE "#[0-9a-fA-F]{6}|px\([0-9]"` - must print nothing.
Smoke: `cargo build`; stop the running daemon (`mk daemon stop`; a locked exe is renamed to `*.locked-old.exe`, never killed); start the UI; run a dev server as a run command and one in a shell pane; check `mk ports`, the dialog (dark and light), Stop on both, and `mk ports stop <port>` on a server outside Mkrew.

- [ ] **Step 9: Commit**

```bash
git add crates/mkrew-ui docs/SPEC.md docs/HUMAN_CHECKS.md
git -c user.email=tungnv975@gmail.com commit -m "feat(ui): Ports dialog from the palette and the Run menu"
```
