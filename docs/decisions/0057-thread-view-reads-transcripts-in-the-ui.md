# 0057 — The thread view reads transcripts in the UI

- Date: 2026-10-14
- Status: accepted (extends ADR 0038's session panel)
- Milestone: none (outside the milestone loop; user request, spec
  `docs/superpowers/specs/2026-10-14-thread-view-design.md`)

## Context

Users read an agent session by scrolling TUI frames in the pane. The agent already writes the
conversation as a transcript file (the same file the usage cards read).

## Decision

The UI reads the agent's transcript itself, with `mkrew_core::thread::ThreadReader`, finding the
file through `mkrew_daemon::transcript_path` (the lookup usage already uses). The session panel
shows the result as a Thread section.

- A 1 s poll runs only while the panel is open and shows an agent session with a native id and a
  profile with `[usage]`. Each tick is one `metadata` call and reads only the appended bytes, off
  the UI thread. Closing the panel or focusing another pane drops the task.
- Bounds: the first read takes the last 4 MiB (from the first line break in that window); at most
  20 turns are kept. Older turns give "Earlier turns: Open transcript".
- Thinking, tool output and sub-agent lines are not shown.
- No protocol change, no daemon change, no new dependency.

## Alternatives

- **The daemon streams turns over IPC:** rejected. It needs protocol and daemon work, and it
  would parse every session's transcript even when nobody looks.
- **A file watcher:** rejected. It has more moving parts than one `metadata` call per second.

## Consequences

- The UI depends on the agents' file formats, as usage already does; a new format is a new
  `ThreadReader` case. Codex prompts come from `user_message` events.
- Every change re-parses the Markdown of at most 20 turns; cache by text if it ever shows.
