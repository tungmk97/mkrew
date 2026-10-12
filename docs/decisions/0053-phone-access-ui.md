# 0053 — Phone access from the Mkrew UI

- Date: 2026-10-12
- Status: accepted (reverses the "starting `mk web` from the daemon" non-goal and the "keep `mk web`
  running with Task Scheduler" advice of ADR 0052)
- Milestone: none (outside the milestone loop; user request: no typing `mk web` or `tailscale serve`)

## Context

ADR 0052 needs the user to start `mk web` by hand (or with Task Scheduler) and to run
`tailscale serve --bg 8787` once. The user wants to do all of it from the Mkrew UI: see whether phone
access is on, switch it on and off, set up Tailscale when it is missing, and move the link to the
iPhone without typing.

## Options

1. The UI starts `mk web` as its own child: it dies with the UI; phone access would stop when the
   window is closed.
2. A listener inside the daemon: a network surface in the process that owns every agent (rejected
   in ADR 0052).
3. The daemon supervises `mk web` as a child while `[web] enabled = true` in `settings.toml`; the UI
   writes the key and reads the state from `daemon.status`. (Chosen.)

## Decision

- `[web] enabled` (bool, default `false`) is the only setting. The UI writes it
  (`mkrew_core::settings::write_web_enabled`, keeps the other sections and comments); the daemon
  reacts through its settings watcher: it spawns `mk web` (windowless, `kill_on_drop`, stderr read),
  restarts a failed child after 1, 2, 4 … 60 s (a 60 s run resets the wait), kills it when disabled.
  `mk.exe` not found → `error`, looked up again every 60 s. The daemon process still has no
  network listener: the socket is in the `mk web` child (SPEC §15).
- `daemon.status` gains `web: {enabled, running, error?}`; `PROTOCOL_VERSION` 39 → 40.
- The UI runs `tailscale serve status --json` (parsed in `mkrew_core::tailscale`, shared with `mk web`)
  and offers **Set up** (`tailscale serve --bg 8787`) only when the proxy is missing and a MagicDNS name
  exists. `mkrew-core` therefore runs the `tailscale` CLI (every run `CREATE_NO_WINDOW`); SPEC §4's
  "no I/O beyond files" says so.
- UI: palette `Global: Phone access…` and a Lucide `smartphone` title bar icon (accent while running)
  open the Phone access dialog (SPEC §14). It polls `daemon.status` and Tailscale about once a second
  while open (background executor, never in render); the icon reads `daemon.status` every 5 s. The
  link `https://<MagicDNS name>/#t=<token>` (token from `<config>/web_token`) is shown as a QR code
  (`qrcodegen`, SVG drawn with `gpui::img`; dark modules on a light panel in both themes: tokens
  `qr_dark` / `qr_light`) and can be copied. It appears only in this dialog.
- Not in the UI (YAGNI): a new-token button (use `mk web --new-token`), choosing port or listen
  address, starting Tailscale or signing in, notifications, several PCs.

## Consequences

- The README setup becomes: phone icon → Start (→ Set up the first time) → scan the QR. Phone access
  stays on across reboots and with the UI closed, because the daemon owns the child.
- A hand-started `mk web` (or a Task Scheduler entry) still works but competes for port 8787: the
  dialog then shows `Not running: <bind error>. Retrying…`.
- Stop kills the child: open phone sockets drop at once.
- The UI cannot show a toast (none exists); a failed Start / Stop is an error row in the dialog.
