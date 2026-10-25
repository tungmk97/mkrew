# Toast click through a `mkrew://` link (Windows)

- Date: 2026-10-25
- Status: design, waiting for review
- ADR: 0078 (written with the implementation, together with the SPEC update)

## Problem

On Windows a click on a Mkrew notification focuses the session only while the banner is on
screen (about 6 s). The daemon shows toasts through `notify-rust` under PowerShell's AUMID and
listens for the in-process `Activated` event. Windows raises that event only for the banner:

- banner timed out, click in the Notification Center (Win+N): no event (probe: `dismissed
  TimedOut` at 6.1 s, then a Notification Center click with the process alive gave nothing);
- toast sent straight to the Notification Center without a banner (Do Not Disturb, full screen,
  Windows quieting a rarely clicked app): no event;
- `notify-rust`'s `wait_for_response` also ends at the first event, so the waiting thread is gone
  after the timeout anyway.

Spike (2026-10-25): a toast with `activationType="protocol" launch="mkrewprobe://session/…"`,
clicked in the Notification Center after the banner timed out and after the process that showed
it had exited, made Windows run the registered handler with the full link.

## Goal

A click on a Mkrew toast, from the banner or the Notification Center, at any time, shows the
Mkrew window on that session's pane:

- UI running (visible, minimized or hidden to the tray): the window comes to the front on the pane;
- UI not running: Mkrew starts and opens the pane once the session list is loaded.

Not changed: Linux and macOS notifications, the `[notify]` policy and 30 s limit, the UI's
handling of `notification.clicked`, the installer, phone push.

## Design

### Link

`mkrew://session/<session ULID>?project=<project ULID>`

`mkrew_core::link` (new, pure, shared by daemon and UI): `session_link(session, project) ->
String` and `parse_session_link(&str) -> Option<(Ulid, Ulid)>`. The parser accepts exactly that
shape (scheme case-insensitive, one optional trailing `/` before `?`), rejects anything else.
Any web page can open a `mkrew://` link (the browser asks first); the worst a crafted link does
is show the window on a session, so no further check is needed.

### Daemon: the toast (Windows)

`OsNotifier::show` on Windows stops using `notify-rust` and the per-notice thread. It builds the
toast XML and shows it with the `windows` crate (0.62, already in the tree through
`tauri-winrt-notification`; new direct dependency with `UI_Notifications`, `Data_Xml_Dom`):

```xml
<toast activationType="protocol" launch="mkrew://session/…?project=…">
  <visual><binding template="ToastGeneric">
    <text>{project} · {name}</text><text>{Needs input|Done}</text>
  </binding></visual>
  <audio src="ms-winsoundevent:Notification.Default"/>   <!-- or <audio silent="true"/> -->
</toast>
```

- `toast_xml(&Notice) -> String` is pure: title and body XML-escaped, `launch` from
  `session_link`, audio from `notice.sound`.
- Shown with `ToastNotificationManager::CreateToastNotifierWithId(POWERSHELL_APP_ID)`: same
  header ("Windows PowerShell") as today; an own AUMID needs a Start Menu shortcut and is out of
  scope.
- The short-lived `notification` thread stays (WinRT calls block; the notify task is async) but
  ends right after `Show`: nothing waits for a click. A failure is a `tracing::warn!` like today.
- `keep_mta_alive` stays (WinRT factories are still cached).
- The click channel and `notification.clicked` stay for Linux; on Windows nothing emits the event
  any more (the link path replaces it). macOS unchanged.
- `notify-rust` moves to `[target.'cfg(not(windows))'.dependencies]`: Windows no longer uses it.

### Daemon: registering `mkrew://`

At startup, when the OS notifier is chosen (`MKREW_NOTIFY` unset) and `MKREW_SOCKET` is unset
(the user's own daemon, not a test or a dev daemon on its own pipe), and `mkrew.exe` exists next
to `mkrew-daemon.exe`, the daemon writes under `HKCU\Software\Classes\mkrew`:

- `(default)` = `URL:Mkrew`, `URL Protocol` = `""`;
- `shell\open\command\(default)` = `"<dir>\mkrew.exe" "%1"`.

Written every start (cheap, keeps the path right after a move or update). `windows-sys` gains
`Win32_System_Registry` (`RegSetKeyValueW`). The function takes the key path so the test can
write under a throwaway `HKCU\Software\MkrewTest-<pid>` and delete it. A failure is a warning;
toasts still show, clicks then do nothing.

Ceiling: an installed Mkrew and a dev build both on the default pipe overwrite each other; the
last daemon started owns the link.

### UI: opened by a link

`main` reads `std::env::args().nth(1)` and parses it with `parse_session_link` (an argument that
is not a session link is ignored, as today).

- **Running UI** (same exe, same socket): `raise_running_instance(socket, jump)` forwards the jump.
  `desktop::win32::raise` gets an `Option<(Ulid, Ulid)>`: without it, `PostMessageW(WM_OPEN)` as
  today; with it, `AllowSetForegroundWindow(<pid of the tray window>)` (the click gave this process
  the right to come forward; it passes it on) then `SendMessageTimeoutW(WM_COPYDATA)` with the 32
  bytes of both ULIDs and a fixed `dwData` tag. The tray window procedure turns a valid
  `WM_COPYDATA` into `TrayEvent::Notification { session, project }`; the shell handles it with the
  existing `notification_clicked` (show the window, `inbox::jump`, open the pane or the workspace).
  The second process then exits.
- **No running UI**: the jump goes into `Shell` as `pending_jump`. After the first successful list
  in `refresh` (after its own `activate_workspace`), `pending_jump.take()` calls
  `notification_clicked`, so the pane wins over the default workspace.

### Errors

| Case | Result |
|---|---|
| Registry write fails | warning in the daemon log; toast shows, click does nothing |
| Link malformed | UI starts or raises as a plain launch |
| Session gone or not listed | `inbox::jump` falls back to the workspace; unknown workspace: just the window |
| `SendMessageTimeout` fails (UI hung) | second process exits; same as a second launch today |

## Testing

- `mkrew_core::link`: round trip; rejects other schemes, other paths, bad ULIDs, a missing
  `project`, extra segments; accepts upper-case scheme and a trailing `/`.
- `toast_xml`: launch attribute equals `session_link`; `<`, `&`, `"` in names are escaped; sound on
  and off.
- Registry: write under the throwaway key, read the command back, delete the key (Windows only).
- Tray: extend `a_second_launch_reaches_the_tray_window`: `raise(key, Some(jump))` delivers
  `TrayEvent::Notification` with both ids.
- Smoke (manual, before commit): toast from the dev daemon on its default pipe, click in the
  Notification Center after the banner timed out, with the UI running, hidden to the tray, and
  closed.

## Docs

ADR 0078; SPEC §8 "Notifications" (click = `mkrew://` link on Windows, `notification.clicked`
elsewhere), §20J item 4 wording, §3 table row "OS notifications" (`notify-rust`, Windows toast via
`windows`).
