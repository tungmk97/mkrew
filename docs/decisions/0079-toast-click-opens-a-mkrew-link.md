# 0079 — A Windows toast click opens a `mkrew://` link

- Date: 2026-10-25
- Status: accepted (SPEC §3, §7, §8, §20A)
- Milestone: none (bug report 2026-10-25)

## Context

Toasts were shown through `notify-rust` under PowerShell's AUMID, and a click was read from the
in-process `Activated` event. Windows raises it only while the banner is on screen (about 6 s):
a click in the Notification Center, or on a toast Windows sent there without a banner (Do Not
Disturb, full screen, a rarely clicked app), never reached Mkrew. `wait_for_response` also ended
at the banner's timeout.

## Decision

- Windows toasts are built by the daemon (WinRT through `windows` 0.62) with
  `activationType="protocol"` and `launch="mkrew://session/<session>?project=<project>"`, still
  under PowerShell's AUMID. Nothing waits for a click; `notify-rust` stays for Linux and macOS.
- At start, the user's daemon (OS notifier on, on the default pipe — `MKREW_SOCKET` unset or
  equal to it, as the UI sets it — and `mkrew.exe` beside it) writes
  `HKCU\Software\Classes\mkrew` → `"<dir>\mkrew.exe" "%1"`.
- `mkrew.exe <link>`: when a UI of the same exe and daemon runs, it grants it the foreground
  (`AllowSetForegroundWindow`) and sends both ids by `WM_COPYDATA` to its tray window, which
  calls the existing `notification_clicked`; a running UI that does not take them (an older
  build, hung) is only asked to show its window. Otherwise the new UI opens the pane after its
  first session list.

## Consequences

- A click works from the banner and the Notification Center, with the UI visible, hidden to the
  tray or closed.
- Any program or web page can open a `mkrew://` link; the most it does is show a session.
- An installed Mkrew and a dev build both on the default pipe overwrite the link; the last
  daemon started owns it. Uninstalling leaves the key (it then points at a missing exe).
- `notification.clicked` is now emitted on Linux only.
- Not done: an own AUMID (needs a Start Menu shortcut with a toast activator CLSID).
