# 0051 — Mkrew web: watch and answer sessions from a phone over the tailnet

- Date: 2026-10-12
- Status: accepted
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
  `app.mjs`, `model.mjs` and xterm.js 6 (vendored, MIT) from the binary.
- Token: 32 random bytes as lowercase hex in `<config>/web_token` (owner-only on Unix), required
  by `/check` and `/ws`; `--new-token` replaces it. It travels in the URL fragment of the printed
  link, so it is not in any request line except the page's own `/check` and `/ws` calls.
- Relay: one async daemon connection per WebSocket; `mk web` sends `hello` itself and refuses
  `hello` and `session.resize` from the browser; responses and `session.snapshot` events with a
  snapshot gain `ansi` (`mkrew_protocol::paint`); output frames go out as binary messages
  (26-character session id + PTY bytes); at most 64 messages wait for a slow browser.
- The page (mockups in `docs/design/mobile-web/`): Sessions ("Needs you" + one group per
  workspace), Session (terminal scaled to fit, quick keys Enter/Esc/↑/↓/Tab/y/n/Ctrl+C, reply
  box = `session.send`), New agent (agent + prompt → task + run; shell → spawn + send), token
  screen, reconnect banner. It never resizes a session.

## Consequences

- SPEC §1, §9, §15, §19 and the §20 intro change. No protocol version change, no migration.
- Whoever has the token and reaches the port controls the agents: keep `--listen` on
  127.0.0.1, publish only with `tailscale serve` (never Funnel); `--new-token` revokes links.
- `mk web` must keep running on the PC (README: Task Scheduler at logon).
- Not done: push notifications, several PCs, board/files/git/docs on the phone.
