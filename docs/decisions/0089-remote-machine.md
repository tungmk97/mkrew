# 0089 - Work on another computer: one window per machine over SSH and `mk proxy`

- Date: 2026-10-29
- Status: accepted
- Milestone: none (INBOX 15)

## Context

The user works from another computer on the home computer's workspaces (INBOX 15).
The agents and the daemon live on the home computer, and the daemon must keep running when the other computer leaves.
The daemon has only an owner-only local socket and no network listener (SPEC §15, ADR 0052).

## Options

1. A TCP listener in the daemon - pros: simple client / cons: a network surface in the process that owns every agent (ADR 0052); rejected.
2. `mk web` relay over Tailscale - pros: exists / cons: the user wants the desktop UI as the client; rejected.
3. One window switching between machines (old plan 2026-10-10) - cons: the user wants every machine ready, no switching; rejected.
4. One window per machine, each its own process, over SSH and `mk proxy` - pros: no new listener, SSH does auth and encryption, one code path for every machine / cons: needs an SSH server and key on the other computer; chosen.

## Decision

Option 4, as in the design spec §2:
- Client: the desktop UI, not `mk web`.
- Machines in the UI: one window per computer, no switch button.
- Network: Tailscale or plain SSH reachability.
- Transport: the window runs `ssh <host> mk proxy`; the remote daemon keeps its owner-only local socket.
- Startup: opening Mkrew opens this computer's window and one window per added machine.
- Unreachable at startup: the window opens, says why, and retries with backoff.
- Connection lost: the window keeps its layout and last screen, blocks input, reconnects and re-attaches.
- Process model: one UI process per machine (`mkrew-ui --machine <name>`), spawned by this computer's UI process.
- Tray: one icon, from this computer's process.
- Adding machines: Settings → Machines, plus palette commands.
- Version mismatch: a clear message naming both versions; no automatic update.
- Daemon settings of the other computer: edited on that computer.

## Consequences

- New `mk proxy` (hidden): joins stdin/stdout to the daemon socket and starts the daemon first; stdout carries protocol bytes only.
- The Unix daemon start uses `setsid`, so an SSH hangup never reaches it.
- The Windows start breaks away from the SSH session's job object when the job allows it, else falls back to a plain detach.
- Protocol 50 adds four requests used by every window, this computer's included.
- Every window keeps its screen and reconnects, also when the local daemon restarts.
- The notifications of a machine window come from that window.
- A daemon started by `mk proxy` inherits the SSH session's environment, which may lack the agents' `PATH`; keep the home daemon started by the home login.
- SPEC §3, §9, §15 and §19, README "Work on another computer" and HUMAN_CHECKS follow.
- ssh runs with `-o ConnectTimeout=10`: without it an unreachable host would hold "Connecting…" for the OS TCP timeout (minutes).
  A link that needs more than 10 s to connect fails and retries.
- The keep-screen banner reads "Connection to <name> lost. Next try in N s." (N fixed per attempt), has **Try now** unless an attempt runs, and a second line with the status detail; one connection attempt runs at a time.
  A version mismatch or unreachable host after a loss stays in the banner, so the kept screen and its unsaved text survive an update of the other computer.
- Files tabs and Docs drafts with unsaved changes survive a reconnect (spec §8 wins over the plan's "clear at once").
- A dialog confirm while disconnected toasts "Not connected to <name>. Reconnecting…"; other input is blocked by the banner state.
- The machine chip and banner add no theme tokens; they reuse `well`, `warning-bg`, `divider`, the status colors and `TITLE_CHIP_MAX_WIDTH`.
- Settings → Machines lists, opens, removes and adds machines; a broken `machines.toml` shows its error and is never written, and is re-read while the tab is open.
  The tray shows `<name> - <short state>`.
- Machine windows type paths for Add workspace, Clone into and Link plan, refuse local-only actions with a toast, and show notifications on this computer with a link that names the machine.
