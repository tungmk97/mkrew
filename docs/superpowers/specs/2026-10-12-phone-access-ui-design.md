# Phone access from the Mkrew UI — Design

- Date: 2026-10-12
- Status: agreed in chat (brainstorming)
- Builds on: `docs/superpowers/specs/2026-10-10-mobile-web-design.md` (`mk web`, ADR 0052)
- Branch: `feat/mobile-web` (same worktree; merged together with `mk web`)

## Intent

The user does not want to type `mk web` or `tailscale serve` to reach Mkrew from the phone.
From the Mkrew UI they want to:

1. See whether phone access is on, and turn it on or off with one click. Off means the phone
   can no longer reach Mkrew.
2. See whether Tailscale already publishes Mkrew (`tailscale serve` for port 8787); if not, set
   it up with one click; if it already is, nothing to do.
3. Get the link onto the iPhone without typing: scan a QR code, or copy the link.

Phone access stays on across reboots once turned on, also when the UI is closed.

Success: the user opens Mkrew, clicks the phone icon, clicks Start (and Set up the first time),
scans the QR code with the iPhone camera, and the page opens. Later, Stop cuts the phone off.

## Decisions

| Topic | Decision | Why |
| --- | --- | --- |
| Who runs `mk web` | The daemon, as a supervised child, while `[web] enabled = true` in `settings.toml` | Survives reboots and a closed UI; the daemon already reloads settings live. The daemon itself still has no network listener: the socket lives in the `mk web` process (SPEC §15) |
| Switch | `[web] enabled` (bool, default `false`), the only key. The UI writes it; the daemon reacts through its existing settings watcher | No new request to toggle; the state is persisted where the user's other settings are |
| Status | `daemon.status` gains `web: { enabled, running, error? }`; `PROTOCOL_VERSION` 39 → 40 | The UI shows the real reason when `mk web` fails (e.g. port in use), not a guess |
| Child process | `mk.exe web`, found next to `mkrew-daemon.exe`, else on PATH; `CREATE_NO_WINDOW`, `kill_on_drop`, stdin null, stdout ignored, stderr read; the last non-empty stderr line is kept as `error` | Same pattern as `spawn::daemon_exe` and the agent children; dies with the daemon |
| Restart | While enabled, a child that exits is started again after 1, 2, 4, … 60 s; a child that ran for 60 s resets the wait to 1 s. `mk.exe` not found → `error`, no restart until settings change | Bounded retries; a stable run forgives earlier failures |
| Stop | `enabled = false` → the daemon kills the child; open phone sockets drop at once | "Stop" must cut access |
| Tailscale window flash | `mk web`'s `tailscale status --json` call uses `CREATE_NO_WINDOW` | A windowless parent would otherwise open a console per call |
| Entry points | Palette command "Global: Phone access…"; a Lucide `smartphone` icon button in the title bar (accent when running, muted otherwise; tooltip "Phone access: on" / "Phone access: off") | The palette lists every action (§14 rule 10); the icon shows the state at a glance |
| Dialog | Status row, Tailscale row, link block (below) | One small dialog, the existing dialog pattern |
| Tailscale check | The UI runs `tailscale serve status --json` (`CREATE_NO_WINDOW`) and looks for a handler `/` proxying to `http://127.0.0.1:8787`; **Set up** runs `tailscale serve --bg 8787` | The user asked for "check first; button only if needed" |
| Link | `https://<MagicDNS name>/#t=<token>`: token read from `<config>/web_token`, name from `tailscale status --json` (same parsing as `mk web`) | The UI builds the link itself; never parses `mk web` output |
| QR | `qrcodegen` crate (no dependencies), matrix drawn as an SVG shown with `gpui::img`; always dark modules on a light panel, colors `qr_dark` / `qr_light` in `theme.rs` | Phone cameras read dark-on-light reliably in both themes; no color literals in components |
| Refresh | While the dialog is open the UI re-reads `daemon.status` and the Tailscale state about once a second | Start / Stop / Set up show their effect without a manual refresh |
| Token rotation | Not in the UI; `mk web --new-token` stays the way to revoke old links | User chose QR + Copy only (YAGNI) |

## Dialog

Title: **Phone access**. Rows, top to bottom (status always icon + text, §11):

1. **Status** (from `daemon.status.web`):
   - `○ Off` — primary button **Start**.
   - `● On` — button **Stop**.
   - `✕ Not running: <error>. Retrying…` (enabled, child down; the daemon restarts it with
     backoff) — button **Stop**.
2. **Tailscale**:
   - `✓ Tailscale serve is set up` — no button.
   - `Tailscale doesn't publish Mkrew yet` — button **Set up**; its stderr shows below on failure.
   - `Install and sign in to Tailscale first` — no button (Tailscale missing or logged out).
3. **Link** (only while On and a MagicDNS name is known):
   - QR code of the link.
   - The link as text and a **Copy link** button.
   - Hint: "Scan with the iPhone camera, then Share → Add to Home Screen."
   - No `web_token` yet (first start) → "Starting…".

The link (it carries the token) appears only in this dialog: never in logs or tooltips.

## Error handling

| Case | Behaviour |
| --- | --- |
| `mk.exe` missing | `web.error = "mk.exe not found next to mkrew-daemon"`; no restart loop |
| `mk web` exits (port in use, …) | `error` = last stderr line; restart with backoff |
| `settings.toml` fails to parse | The daemon keeps its last settings (existing watcher); `settings_error` already reaches the UI |
| The UI cannot write `settings.toml` | Toast with the error; state unchanged |
| `tailscale serve` fails | Its stderr under the Tailscale row; Set up can be pressed again |
| UI/daemon protocol mismatch | Existing version-mismatch path |

## Out of scope (YAGNI)

New-token button, choosing the port or listen address in the UI, starting Tailscale or signing
in, notifications, several PCs. This reverses the earlier spec's "starting `mk web` from the
daemon" out-of-scope line (ADR 0053).

## Spec changes (same change as the code)

- SPEC settings section: `[web] enabled`.
- §7 / `daemon.status`: the `web` field; protocol 40.
- §14: the Phone access dialog and title-bar icon; `qr_dark` / `qr_light` tokens.
- §15 Security: the daemon may run `mk web` as a child when the user turned phone access on;
  the daemon process still has no network listener.
- ADR 0053.

## Testing

- Core unit: writing `[web] enabled` keeps the other sections and comments intact.
- Daemon unit: backoff schedule, reset after a 60 s run, last stderr line, enabled-change →
  start / stop decision.
- Daemon integration: a fake `mk` binary selected by a test-only env var (`MKREW_MK_EXE`):
  enabled → `daemon.status.web.running`; disabled → child killed; a fake that exits at once with a
  stderr line → that line in `error`.
- Protocol: round-trip of `DaemonStatusResult` with `web`.
- UI unit: parsing `tailscale serve status --json` (proxy to 8787, to another port, empty);
  building the link from token + name; the QR matrix for a link has the expected size and
  finder patterns.
- No test runs Tailscale or the network (parsers are pure functions).
- Smoke: the UI with isolated MKREW_* dirs: Start → running, Stop → not running, dialog in dark
  and light. HUMAN_CHECKS: scan the QR with a real iPhone; Stop while the phone page is open.
