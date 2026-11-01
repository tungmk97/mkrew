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
