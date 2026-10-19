# 0052 — Mkrew web: watch and answer sessions from a phone over the tailnet

- Date: 2026-10-12
- Status: accepted (amended by ADR 0053)
- Milestone: none (outside the milestone loop; user request: an app on the phone to keep work
  moving while away from the PC)

## Context

- SPEC §1 listed "No mobile app" and §15 "no network listeners". The user wants to see which
  sessions need them, answer agents and start new ones from an iPhone, and already uses
  Tailscale.
- The daemon owns all state and every needed action exists in §7 (`project.list`,
  `agent.profiles`, `session.list/attach/detach/input/send/spawn`, `task.create/run`).
- omp's `/collab` covers omp sessions only, one link per session, and cannot start sessions.

## Options

1. A native iOS app: needs a Mac, signing, the App Store or 7-day sideloading; a second UI stack.
2. A TCP listener in the daemon: a network surface in the process that owns every agent.
3. `mk web`: a command the user starts, serving a small page and relaying its WebSocket to the
   daemon's local socket; `tailscale serve` publishes it with HTTPS inside the tailnet. (Chosen.)

## Decision

- `mk web [--listen 127.0.0.1:8787] [--host NAME] [--new-token]` (§9) starts the daemon if none
  runs, prints `https://<MagicDNS name>/#t=<token>` and serves `index.html`, `app.css`,
  `app.mjs`, `model.mjs`, `apple-touch-icon.png` (the Home Screen icon, 180 px, from
  `icon/mkrew-1-lanes/tile-dark.svg` drawn full-bleed) and xterm.js 6 (vendored, MIT) from the
  binary.
- Token: 32 random bytes as lowercase hex in `<config>/web_token` (owner-only on Unix), required
  by `/check` and `/ws`; `--new-token` replaces it. It travels in the URL fragment of the printed
  link, so it is not in any request line except the page's own `/check` and `/ws` calls.
- Relay: one async daemon connection per WebSocket; `mk web` sends `hello` itself and refuses
  `hello` and `session.resize` from the browser; responses and `session.snapshot` events with a
  snapshot gain `ansi` (`mkrew_protocol::paint`); output frames go out as binary messages
  (26-character session id + PTY bytes); at most 64 messages wait for a slow browser. A reply
  to a browser `session.history` request is preceded by one output message holding the newest
  512 KiB of the session's log (`mkrew_core::log_tail`), or replaced by an `internal` error when
  the log cannot be read: the page replays sessions that ended (2026-10-13).
- The page (mockups in `docs/design/mobile-web/`): Sessions ("Needs you" + one group per
  workspace), Session (terminal scaled to fit, quick keys Enter/Esc/↑/↓/Tab/y/n/Ctrl+C, reply
  box = `session.send`), New agent (agent + prompt → task + run; shell → spawn + send), token
  screen, reconnect banner. It never resizes a session. Added 2026-10-13: a Stop button
  (`session.kill`), chips of the last 5 one-line replies (`localStorage` `mkrew.replies`), and
  sessions that ended in the last 24 h (listed below the live ones, opened as a log replay).

## Consequences

- SPEC §1, §9, §15, §19 and the §20 intro change. No protocol version change, no migration.
- Whoever has the token and reaches the port controls the agents: keep `--listen` on
  127.0.0.1, publish only with `tailscale serve` (never Funnel); `--new-token` revokes links.
- `mk web` must keep running on the PC (amended by ADR 0053: the daemon runs it; README: Task Scheduler runs `mk.exe daemon start` at logon).
- Not done: push notifications, several PCs, board/files/git/docs on the phone.
- Amended 2026-10-13: a view button on the session screen cycles **Fit** (font fits the width), **Zoom** (the session's rows fill the height; the box pans sideways) and **Read** (the terminal's text, soft-wrapped rows joined, box side borders dropped, wrapped to the phone width; plain text, no colours), kept in `localStorage`; the session size is still never changed. Read shows what the page's terminal holds: on the alternate screen (full-screen agents) that is the current screen only. The page keeps 10,000 lines of live scrollback like the desktop pane; an attach still brings at most the daemon's 200 lines. A Home Screen app fills `screen` height when no field is focused (iOS 26 visual viewport is short by the top safe area, WebKit bug 301108).
- Amended 2026-10-13: xterm 6 has no touch scrolling, so the page scrolls on a one-finger vertical drag itself, like the desktop wheel: the history, or arrow keys for a full-screen program without mouse mode.
- Amended 2026-10-13: ended sessions are no longer listed (as on the desktop sidebar, ADR 0033); only the open session keeps its ended screen. The New agent sheet keeps at least 48 px + 24 px clear at the top, since some iPhones report `env(safe-area-inset-top)` as 0 while the page draws under the notch.
- Amended 2026-10-15: for the same reason, the Home Screen app (`display-mode: standalone`) keeps the sessions header and session top bar at least 60 px below the top (notch / Dynamic Island); a Safari tab, which never draws under the status bar, uses the reported inset only.
- Amended 2026-10-19: iOS 27 blurs the top of the page, in the Home Screen app far below the status bar. A `position: fixed` strip in the page background colour covers the top (`max(top inset, 6px)` high, full width); iOS then paints the status bar that colour and drops the blur (Modern Web Weekly #80).
