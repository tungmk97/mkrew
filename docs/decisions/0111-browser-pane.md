# 0111 - Browser pane: a web page as a fourth kind of leaf, shown by the OS web view

- Date: 2026-11-24
- Status: accepted (amends ADR 0108: leaf kinds; amends SPEC sections 10 and 12: web links open in Mkrew)
- Milestone: none (human request 2026-11-24, spec `docs/superpowers/specs/2026-11-24-browser-pane-design.md`, plan `docs/superpowers/plans/2026-11-24-browser-pane.md`)
- Numbers: this branch was written against ADR 0108 and protocol 60, then moved to 0111 and protocol 62 when master took 0107 to 0110 and protocol 61.

## Context

A link in an agent's chat or terminal opens the system browser, and the user loses the place they were in.
Tern shows the page in a pane next to the agent, and the user wants the same: look at the app an agent just started on `localhost:3000`, and read the docs, PRs and issues an agent links to.
ADR 0108 made a pane a typed leaf of the split tree, so a page can be one more leaf.
The page itself needs a web engine, and a GPUI window has none.

## Decision

- **A fourth leaf.**
  A leaf can be `{"web": "<url>", "id": "<ulid>"}` (`Leaf::Web`, key `PaneKey::Web(id)`); `web` is `""` before the first page.
  `id` is required, so two panes can show the same URL, and a leaf without it is dropped with a warning like any bad leaf.
  The layout stays untagged JSON in `terminal_tab.layout`, so there is no migration.
  The daemon reads only session leaves and keeps the others, so it needs no code change, only tests.
- **The OS web view, as a native child of the GPUI window.**
  `wry` 0.57 (WebView2 on Windows, WKWebView on macOS), built with `build_as_child` on the window's handle, below a toolbar drawn by GPUI.
  It is a dependency for Windows and macOS only, so Linux CI installs nothing new.
  A native view paints above everything GPUI draws, so Mkrew hides it off screen (another tab or view, a zoomed neighbour) and under every overlay (palette, dialog, menu, the `+` menu, a peek popover, any drag).
  A title card with the page's title and URL takes its place.
- **Links.**
  `http` and `https` links of Chat, Markdown previews, Docs, terminal Ctrl+Click and link hints go to `Shell::open_web`.
  The tab shown in the window loads the link in its first browser pane; with none, the link splits right of the pane it came from.
  One browser pane per tab takes links; Tern opens a new block per link instead.
- **Not available.**
  Linux and Windows without the WebView2 Runtime use the system browser, with one toast per run naming the reason.
  Those machines have no Browser item in the `+` row, `Alt+Shift+B` does nothing, and the palette offers no new browser pane, address, back, forward or reload; "Open in system browser" stays on a browser pane.
  A browser pane already in a layout stays and shows the reason and "Open in system browser".
  If building a view fails where the Runtime exists, the pane shows the error and the same button; the next link or Enter in the address field tries again.
- **Every window move rebuilds the page.**
  A tab torn off, moved back, or a pane dragged to another window rebuilds the view in the new window on the same URL, so back / forward history is lost.
  wry 0.57 `reparent` keeps the first parent for a child view's focus target (`focus_parent` would raise the old window), and macOS would need `unsafe` for its `NSWindow`, which the crate denies.
  The Shell notices a move at the start of each window's render, by comparing the window a page was built in with the window that shows its leaf.
- **Keys given back by an init script.**
  A click in the page gives it the keyboard, so GPUI never sees the keys.
  An initialization script posts focus over IPC, which focuses the pane in Mkrew, and posts every combo of a global binding or of the Browser context (`Ctrl+L`) that holds Ctrl, Alt or Cmd, or is an F-key, and swallows it.
  Text keys (Ctrl / Cmd with A, C, V, X, Y, Z), keys without a modifier and a lone Alt stay in the page.
  A lone Alt is made harmless in the script too: an unhandled Alt keyup from WebView2 reaches the host window, where `DefWindowProc` enters the Win32 system menu (`SC_KEYMENU`), which captures the mouse and eats the next keys.
  So `Alt+←` and `Alt+→` (pane focus) do not move page history; the toolbar, the mouse buttons and the palette do.
  The key list follows the keymap (ADR 0073).
- **DirectComposition off on Windows.**
  `GPUI_DISABLE_DIRECT_COMPOSITION=1` is set at the top of `main()` unless the user set it.
  gpui-pre-windows 0.3.7 draws through a topmost DirectComposition visual that covers child windows, so the page was invisible.
  The whole app now draws through the HWND swap chain path; resize flicker or a performance change is possible.
  Mkrew strips the variable from the environment of the daemon, Zed and the installer when Mkrew set it itself, so a GPUI-based Zed or a relaunched Mkrew does not take it for a user setting.
- **First build blocks input briefly.**
  The first build of a page pumps messages inside GPUI's borrow (wry `wait_with_pump`), so a click or key in the first ~400 ms may be dropped, with `RefCell already borrowed` in the log.
  Any GPUI callback holds the `App` borrow, so building outside it needs a change in GPUI.
  Accepted.
- **Protocol 62.**
  An older UI would drop the unknown leaf and write the layout back without it, so a version check must refuse the pairing.
- **Storage and theme.**
  The page follows Mkrew's theme on Windows (`prefers-color-scheme`), and the app appearance on macOS.
  Windows keeps its data in `<data>/mkrew/WebView2`, shared by all panes and apart from the system browser; macOS uses the app's default store.
  Cookies and logins survive a restart; back / forward history does not.

## Alternatives

- **CEF offscreen (`wef`).**
  Pixels would come into GPUI, so overlays, clipping and rounded corners would work without hiding anything.
  It adds about 1 GB to the app and a second engine to ship and update.
  Rejected.
- **An external browser window glued to the pane.**
  Needs a window that follows the pane's bounds, z-order and focus across virtual desktops, monitors and detached windows, and a browser the user has not set up this way.
  Rejected.
- **The `gpui-webview` crate.**
  It does what the native child does, but pins `gpui-pre =0.3.9` and Mkrew pins `=0.3.7`.
  `WebSurface` is modeled on it instead.
- **A new pane per link (Tern).**
  Ten links would be ten panes.
  One link-taking pane per tab keeps the layout, at the cost of replacing the page the user was reading; the toolbar's `+` and `Alt+Shift+B` make more.
- **`reparent` on a window move.**
  Keeps history, but it left the keyboard with the old window (see Decision), and macOS needs `unsafe`.
  Rejected for now; revisit when wry fixes the focus parent.

## Consequences

- Protocol 62: a UI from before it cannot connect to the new daemon.
  After the update the user restarts the daemon.
- Tab tear-off, Move back and a pane drag across windows reload the page, so a form half filled in is lost.
- While an overlay or a drag is open, the page shows as a title card, not a picture of it (`ponytail:` a capture through WebView2 `CapturePreview` or `WKWebView.takeSnapshot` if it looks jarring).
- The dim of an unfocused pane, and anything else GPUI paints above a pane, covers only the toolbar: GPUI cannot paint above the native page.
  Tooltips of other panes that reach over a page are covered by it.
- The square page covers the pane's rounded bottom corners; the focused stripe sits on the toolbar's top edge, so it stays visible.
- Windows draws the whole app without DirectComposition.
  A user who sets `GPUI_DISABLE_DIRECT_COMPOSITION` themselves keeps their value, and Mkrew does not remove it from children.
- The keys the init script gives back come from `e.code`, so a non-QWERTY layout may match a combo at the US key position.
- Checked by smoke on Windows: link, `+`, load, hiding under overlays and tabs, tear-off, restart, Stop.
  Not checked by hand: page-focused key forwarding, a lone Alt, a click into the page while editing the address, a page that autofocuses next to a terminal, Move back, drags, the first ~400 ms of a build, macOS, and Windows without the Runtime.
  They are in `docs/HUMAN_CHECKS.md`.
- Not built: picture in picture cards, agents driving the page, an element picker, a browser on Linux, back / forward across a restart, a setting that sends links to the system browser, find in page, bookmarks, a tab strip inside the pane, a download panel (spec section 12).
- SPEC: sections 5, 7, 10, 13 and the new 20M.
