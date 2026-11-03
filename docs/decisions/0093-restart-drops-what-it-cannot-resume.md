# 0093 - A restart drops the panes it cannot bring back

- Date: 2026-11-03
- Status: accepted
- Milestone: none (human request 2026-11-03: "after a restart the terminals do not come back, which is fine, but the red exited tab with an empty terminal is useless; clean the tab and the RUN / Terminal entries in the sidebar")

## Context

A session cut off by the daemon stop (or still live at startup after a reboot) keeps its pane, so that the resume at the next start puts the agent back in the same place (ADR 0017, ADR 0033).
Only agents with a native session id and a `resume` command come back.
Shells, run commands and other agents stayed in their tabs as a red dot over "✕ Session exited.", and a run command's RUN row stayed red.
Nothing could be done with these panes but close them one by one.

## Decision

- At startup, after the resumes, every session the stop interrupted and nothing brought back leaves its tabs (`Hub::drop_panes`).
  A resumed pane already names its new session, so it is not touched.
- A tab left without panes by this closes, unless it is pinned.
  A tab that was empty before stays.
  A workspace left without tabs gets a fresh "Terminal" tab from the UI, as before; autostarted run commands get their run tab again.
- Interrupted shells, run commands included, are hidden as by `session.remove` (ADR 0019): they cannot be resumed, so they leave History and the RUN row shows the command as stopped.
- An interrupted agent stays listed, so History can still resume it by hand (ADR 0055).
- Archived workspaces are left alone: their panes wait for the workspace to be added again.
- No protocol change.

## Consequences

- After a reboot the tab bar shows only what runs again.
- A shell's scrollback from before the restart is no longer reachable from History.
- SPEC §8 (Resume) says so.
- Test: `restart_clears_the_panes_it_cannot_bring_back` (daemon).
