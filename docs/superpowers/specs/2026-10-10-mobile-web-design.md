# Mkrew web: control agents from a phone — Design

- Date: 2026-10-10
- Status: agreed in chat (brainstorming), UI mockups in `docs/design/mobile-web/`
- Plan: `docs/superpowers/plans/2026-10-10-mobile-web.md`

## Intent

The user leaves the PC that runs Mkrew and still wants to keep work moving from an iPhone (and
any other browser on the same tailnet, e.g. Chrome on another laptop):

1. See which sessions need them (needs input / done) and how the others are doing.
2. Open one session, read its terminal, answer (type text, quick keys like `y`, Enter, Esc,
   arrows, Ctrl+C).
3. Start a new agent (or shell) in a workspace and give it a prompt.

Nothing else: no board, Files, Git, Docs review, splits or settings on the phone. No push
notifications (the user opens the page to look). One user, one PC.

Success: away from home, the user opens the Home Screen icon, sees "Needs you", taps the
session, reads the question, answers, and the agent continues on the PC. Or picks a workspace,
picks Claude, types a prompt, and the agent starts on the PC.

## Constraints (said by the user)

- iPhone (Safari, "Add to Home Screen"); also any browser in the tailnet.
- Already uses Tailscale (free Personal plan covers this: MagicDNS, HTTPS certificates,
  `tailscale serve`).
- Work happens in a separate git worktree.

## Decisions

| Topic | Decision | Why |
| --- | --- | --- |
| Client | A web page (vanilla HTML/CSS/ES modules + vendored xterm.js 6), embedded in `mk` | One codebase for iPhone and desktop browsers, no App Store, no Mac/Xcode, no Node build |
| Server | New CLI command `mk web` (tokio, `tokio-tungstenite`); starts the daemon if none runs | The daemon keeps "no network listeners"; the user starts the listener on purpose. The phone may be the only way in after a reboot |
| Listen | `127.0.0.1:8787` by default (`--listen` to change) | `tailscale serve --bg 8787` gives `https://<pc>.<tailnet>.ts.net` with a real certificate, tailnet only. Mkrew does no TLS |
| Auth | 32 random bytes, hex, in `<config>/web_token`; required by `/ws` and `/check` (`?t=`); `--new-token` replaces it | localhost is reachable by every local process; the daemon socket is owner-only. Static files are not secret |
| Link | `mk web` prints `https://<MagicDNS name>/#t=<token>` (name from `tailscale status --json`, or `--host`) | The token sits in the fragment, never in a request line or log. The page keeps `#t=` in the URL so the Home Screen bookmark (separate storage on iOS) carries it |
| Protocol | The browser speaks SPEC §7 JSON (text messages). Each WebSocket gets one async daemon connection; `mk web` greets the daemon itself and refuses `hello` and `session.resize` from the browser | Reuses every existing request; the daemon is unchanged; no `PROTOCOL_VERSION` bump |
| Screen paint | Responses/events carrying a `snapshot` gain `ansi` (from `mkrew_protocol::paint::paint`, the `mk attach` painter; a `session.snapshot` event is painted without history) | The browser writes bytes into xterm.js; no second painter in JS |
| Output | Binary WebSocket message = the 26-character session id + raw PTY bytes | No base64; the page compares the prefix with the open session's id |
| Backpressure | At most 64 messages wait for a slow browser; then the relay stops reading the daemon, which drops queued output and sends `session.snapshot` (SPEC §7) | Memory stays bounded on a slow phone link |
| Resize | The web client never sends `session.resize`; the terminal keeps the session's columns and shrinks its font to fit; the page can be pinch-zoomed | A phone width (~45 cols) would reflow the agent and break the desktop view |
| New agent | Agent + prompt → `task.create` + `task.run` (120×30; the task shows on the desktop board). Shell, or no prompt → `session.spawn` (120×30), then `session.send` for a shell prompt | `task.run` delivers the prompt per the profile's `prompt_mode`; typing into a just-started TUI can be lost |
| Session list | Sessions from `session.list` that are not exited, plus ones that exit while the page is open; "Needs you" = needs_input or done; groups per non-archived workspace, pinned first | Matches the desktop sidebar rules (§11, ADR 0033) without a "viewed" state |
| Reconnect | `/check` first (401 → token screen), then WebSocket; retries 1, 2, 4, 8, then every 10 s; banner "Can't reach Mkrew on <machine>. Retrying…"; an open session re-attaches | Mockups `sessions-disconnected`, `token` |
| Look | The mockups' CSS: desktop tokens (SPEC §14), dark/light via `prefers-color-scheme`, 44 pt targets, safe areas | User-approved design |

## Out of scope (YAGNI)

Push notifications, QR code in the terminal, a web manifest/icon, starting `mk web` from the
daemon, several PCs, task board, files/git/docs, resizing sessions from the phone.

## Spec changes (same change as the code)

- §1 Non-goals: "No mobile app" → no native mobile app; a web client over the user's tailnet
  (`mk web`, ADR NNNN).
- §9: `mk web [--listen ADDR] [--host NAME] [--new-token]`.
- §15 Security: the daemon has no network listener; `mk web` listens on 127.0.0.1 by default and
  every WebSocket needs the token.
- §19 backlog: drop "remote control via web/phone" (done).
- ADR NNNN (next free number ≥ 0046 at execution time).

## Testing

- Rust unit tests: token create/load/compare, MagicDNS name parsing, request-head parsing,
  `ansi` enrichment (attach response painted; snapshot event painted without history; other
  messages unchanged).
- Rust integration test (`crates/mkrew-cli/tests/cli.rs`): real `mk web` + daemon + fake agent:
  static files, 401/204/404, WebSocket `session.list`, attach (screen shows the agent's
  question, output frames carry the session id), `session.input` answers, `session.exited`
  arrives.
- `node --test crates/mkrew-cli/web/` for the pure JS helpers (not part of `scripts/verify`).
- Browser smoke with the fake agent (list, open, answer, new agent, token screen, disconnect);
  iPhone checks in HUMAN_CHECKS.
