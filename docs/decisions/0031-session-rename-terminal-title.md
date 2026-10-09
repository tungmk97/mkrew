# 0031 — Rename a pane (session.rename) and name it from its terminal title

- Date: 2026-10-09
- Status: accepted
- Milestone: inbox 2026-10-09

## Context

The user wants a shortcut to rename a pane and, if cheap, an automatic name taken from the
current work. A pane is one session. `session.title` (nullable, persisted) already labels it in
the pane header, sidebar, Inbox, palette, notifications and resume. But it was set only at spawn,
and no request changed it. `run:<name>` titles mark Run sessions (ADR 0018).

## Options

1. Rename the session: `session.rename` + `session.updated`. The name follows the session
   everywhere and survives a daemon restart (resume copies it).
2. A per-pane label in the tab layout JSON: a second naming system that nothing outside the tab
   sees.
3. Reuse the tab rename (Ctrl+Shift+R): it names the tab, not the pane.

## Decision

Option 1. A live session is renamed through its task, because the task owns the row and saves
it on every state change. An exited session is renamed in the store. A blank title clears the
name. Run sessions and `run:` titles are refused, so Run keeps finding its sessions. The
shortcut is F2, the Windows rename key: TUIs lose F2, and users can rebind it. A double-click
on the header title does the same.

Automatic name: the daemon's emulator records OSC 0/2 titles. For agent sessions, the actor
drops leading glyphs (Claude's spinner), caps the title at 60 characters, dedupes it and
announces `session.terminal_title`. It ignores paths and glyph-only titles (the current name
stays); a reset or empty title clears the name. The title is kept in daemon memory only and
returned as `terminal_title` in `session.list`. Display order everywhere: user title > terminal
title > agent profile. A rename always wins, and clearing the name shows the terminal title
again. Shells are left out, because their titles are paths and commands. Rejected: the first
prompt (typed straight into the PTY, so Mkrew never sees it) and an `ai.generate` summary (one
CLI run per pane).

## Consequences

Protocol: a `session.rename` request, a `session.updated` event (the whole Session) and a
version bump. SPEC: §7 rows, the §11 header wording and a §13 row. No migration, because the
column exists. The optional `mk session rename` stays out until someone asks for it.

`Session` gains `terminal_title` (on the wire only, never stored), and there is a new event `session.terminal_title` with a second protocol bump.
