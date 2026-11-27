# 0121 - Switch an omp pane between TSP and omp's terminal UI

- Date: 2026-11-27
- Status: accepted (amends ADR 0107; SPEC §7, §10, §11)
- Protocol: 64 (`session.relaunch`). Migration: none.

## Context

An omp pane opened from `+` runs with TSP: Mkrew draws omp's native surface.
An omp typed by hand in a shell pane draws its own terminal UI, because every PTY other than a TSP profile's gets `PI_TUI_NATIVE=0`.
The user wants to move one pane between the two views with a button or a shortcut.
omp reads `PI_TUI_NATIVE` once, when it starts; nothing in TSP or its environment changes the renderer of a running omp (spec 2026-11-23, section 12, left this out of scope).

## Decision

- **Relaunch.** The switch restarts omp: `session.relaunch {session_id, tsp}` stops the live session (the normal terminate, force after 5 s), waits up to 10 s for its exit, and resumes its conversation like `session.resume`, with the profile's `tsp` replaced by the one asked for.
  The resumed session takes the old pane's place in its tab.
- **The pane keeps the choice.** The daemon writes `"tsp": true|false` on the pane's layout leaf before the stop.
  Every resume (`session.relaunch`, History's Resume, the startup resume) reads that flag from the leaf of the session it resumes, so the view survives a daemon restart.
  A conversation resumed from History into a new pane uses the profile again.
- **The pane stays.** A session that exits leaves its tab (ADR 0033); a session being relaunched is marked in the hub (`kept_panes`), so its pane stays for the resumed one.
  When the resume fails the pane leaves as after any exit, and the error is a toast.
- **Refused before anything stops** for a shell, an exited session, a session whose agent never reported its id or whose profile has no `resume`, and while the agent is `working` (its turn would be cut off).
- **UI.** The pane's existing `Terminal | Chat` toggle (`MessageSquare`), `Ctrl+Alt+T` and the palette entry "Agent: Toggle chat view" are the switch on an omp pane whose session has TSP on, or whose leaf says `tsp: false`: accent while the TSP view shows.
  Such a pane has no Chat mode (ADR 0104), so the key has one meaning per pane.
  The switch shows only for a live session with a native id; a second press while a relaunch is in flight does nothing.

## Consequences

- A switch costs omp's start time (about 2 to 10 s) and closes whatever omp had open (a picker, a half-typed prompt in omp's own editor).
- Panes of profiles with `tsp = false`, Claude Code and Codex keep the Chat view as before.
- An omp started by hand in a shell pane is still not switchable: Mkrew does not know its conversation; open it from `+` (or History) instead.

## Rejected

- A new command with its own key: the user chose `Ctrl+Alt+T`, and a pane never has both Chat and the omp switch.
- Let the UI stop the session and call `session.resume`: the pane would leave its tab at the exit, and two requests can interleave with other clients.
- Store the choice in a session column: the layout leaf already holds per-pane modes (`chat`), and the choice belongs to the pane, not to one run of omp.
