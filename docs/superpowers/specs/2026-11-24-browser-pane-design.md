# Browser pane - Design

- Date: 2026-11-24
- Status: design approved in chat by the user (2026-11-24), section by section; this file waits for the user's review.
- Branch and worktree: `feat/browser-pane` in `.worktrees/browser-pane`, from `master` at `20a2202` (after the Tern panes merge).
- ADR: `docs/decisions/0109-browser-pane.md`.
  `master` has 0108; `feat/omp-tsp` (unmerged) holds 0107.
- Protocol: 60 -> 61.
  `master` is at 60; `feat/omp-tsp` is at 59 and not rebased, so it needs a number above 60 when it merges.
  Re-check both numbers when this branch merges; take the next free ones if they moved.
- Migration: none (the tab layout is a JSON column).
- Amends ADR 0108 (a fourth leaf kind) and the link handling of SPEC §10 and §12 (web links open in Mkrew).

## 1. Goal

The user wants a web browser inside Mkrew, like Tern's browser block.
A web link in an agent's chat or terminal opens a browser pane split to the right of the active pane, and the `+` row offers a new browser.

The user wants it for both of these:

- Looking at the app an agent just started (`http://localhost:3000`): reload often, keep the pane across a restart.
- Reading docs, PRs and issues an agent links to.

Success means:

- Clicking a web link in a Chat message, or Ctrl+Click (Cmd+Click on macOS) on a web link in a terminal, shows the page in a pane on the right of the pane the user was in, without leaving Mkrew.
- A second link in the same tab loads in that same browser pane.
- Hovering `+` shows a Browser icon in the row; clicking it opens a new tab with a browser, dragging it onto a pane edge splits that pane there.
- A browser pane has back, forward, reload, an address field, "Open in system browser" and close.
- A browser pane comes back after a restart, at the same place, on the page it last showed.
- Palette, dialogs, menus and drags are never hidden behind the page.
- On Linux, and where the OS web view is missing, links open in the system browser as today.

## 2. What the user said, and what was assumed

Said by the user (request and brainstorm answers):

- Open a browser inside Mkrew; when an agent's chat has a link, open a new pane split to the right of the current active pane.
- Use: both the dev app an agent runs and links to docs / PRs.
- Build it after the Tern panes branch merged (it has).
- Add Browser to the `+` row in the tab strip (added after the first request).
- Embed the OS web view with `wry` as a native child window (approach 1 of 3; CEF offscreen and an external window glued to the pane were rejected).
- Look at how Tern does it and do the same where it fits.
- Linux opens links in the system browser; Windows and macOS embed the page.

Assumed (stated in chat, not contradicted):

- One browser pane per tab takes links: a link loads in the tab's browser pane if it has one, else it splits right of the active pane.
  Tern opens a new block per link instead; the user's answer to that question was "ok, continue" with this option as the default.
- Ctrl+Click in the terminal and link hints follow the same rule as Chat links.
- No setting to choose between Mkrew and the system browser; the toolbar has "Open in system browser", and Mkrew falls back to the system browser when it cannot show the page.
- The address field follows Tern's rule (section 6.2), searching with Google.
- Back / forward history is not kept across a restart; cookies and logins are.

Seen in Tern (research, 2026-11-24), and taken:

- The OS web view (WebView2 on Windows, WKWebView on macOS) hosted as a native child of a pane, opened beside the focused pane.
- A browser from the `+` menu lands in its own tab.
- The address rule, "Open in default browser", and the page's own keys for reload, inspector and zoom.

Seen in Tern, and not taken (non-goals, section 12): picture in picture cards, agents driving the page, an element picker, screenshots into the prompt.

## 3. Data model

### 3.1 Leaf (`crates/mkrew-ui/src/split.rs`)

| Leaf | JSON | Key (`PaneKey`) |
|---|---|---|
| `Leaf::Web { id, url }` | `{"web": "https://example.com/", "id": "<ulid>"}` | `PaneKey::Web(Ulid)` |

- `id` is a fresh ULID per browser pane, so two panes can show the same URL; `validate` rejects the same `id` twice, as for any key.
- `url` is the page the pane shows, or `""` for a pane that has not loaded anything yet (the empty state, section 5.3).
- A web leaf without `id`, or with an `id` that is not a ULID, does not parse and is dropped with a `warn!`, as ADR 0108 does for every bad leaf.
- No required key of `Leaf::Web` is a required key of another leaf, so `#[serde(untagged)]` stays unambiguous.
- `PaneKey::session()` is `None` for `Web`; `chat_of` / `set_chat` do not touch it.
- `Leaf::Web` exists on every OS, so a Linux UI keeps a layout that a Windows UI wrote (section 8.3).

### 3.2 Browser state (UI only)

- `Shell.browsers: HashMap<Ulid, Browser>`, keyed by the leaf `id`.
- `Browser` holds the leaf's live page: the native web view (Windows and macOS only), the window it belongs to, title, loading flag, `can_go_back` / `can_go_forward` when the platform reports them, and the address field's text while it is edited.
- A `Browser` is created when its leaf is drawn the first time with a non-empty `url`, and dropped when its leaf leaves every layout of every window.
- After a restart only `url` comes back (from the layout); the page loads again.

### 3.3 Daemon and protocol

- `crates/mkrew-daemon/src/tabs.rs` needs no code change: `cut()` and `layout_sessions()` read only leaves with a `session` key, and a tab that holds no session leaf stays (ADR 0108).
- New daemon tests prove it for a web leaf: a web leaf survives `cut()` of its sibling session, and a tab whose layout is one web leaf survives `drop_panes` at startup.
- `PROTOCOL_VERSION` 60 -> 61: an older UI would drop the unknown leaf and write the layout back without it.
- SPEC §5: leaves are "sessions, files, the diff or web pages".

## 4. Opening a browser pane

### 4.1 Links

Today `open_preview_link` (`shell/files_split.rs`, Chat messages, Markdown previews, Docs) and `open_terminal_link` (`shell/navigation.rs`, Ctrl+Click and link hints) call `cx.open_url` for a web link.
Both call `Shell::open_web(url, from, window, cx)` instead, for `http` and `https` only.
`mailto:` and every other scheme still go to `cx.open_url`.

`open_web` decides with a pure function in `crates/mkrew-ui/src/tabs.rs`:

```rust
pub enum WebPlace {
    /// The tab already has a web leaf: load the URL there and focus it.
    Load(Ulid),
    /// Split right of this pane (the tab's focused pane, or the pane the link was in).
    Split(PaneKey),
    /// No tab to split (a link outside the Terminals view): a new tab.
    NewTab,
}

pub fn web_place(tab: Option<&Tab>, from: Option<&PaneKey>) -> WebPlace;
```

- The tab is the one shown in the window the click happened in (`tab_scope(window)`): the main window's front tab, or a detached window's tab.
- `Load` takes the first web leaf of the tab in reading order (left to right, then top to bottom, as `rects` returns them).
- `Split` anchors on the pane the link was in when it is known (a terminal link carries its session; a Chat message knows its session), else the tab's focused pane.
- `NewTab` is used when no tab is shown (a link in the Docs view): the browser opens as a new tab of the workspace and the Terminals view comes to the front, as a file does from Docs (ADR 0108).
- Loading into an existing leaf focuses it; if another pane of the tab is zoomed, the zoom ends first.
- Placement goes through `place_leaf(project, leaf, place, window, cx)` (`shell/places.rs`) with `Place::Split(key, Edge::Right)` or `Place::NewTab`, so the layout is saved the same way as for files.
- When the browser is not available (section 8), `open_web` calls `cx.open_url` and nothing else; the first time per run it shows a `toast_info` naming the reason ("Web pages open in your browser: <reason>").

### 4.2 `+` row (`crates/mkrew-ui/src/plus_row.rs`, `shell/plus_row.rs`)

- `NewKind::Browser` and `RowItem::Browser` are added; the row is Agent (when a default exists), Shell, Note, Browser, then `▾` More.
- Label "New browser", glyph `Icon::Globe` (Lucide `globe`, added to `icons.rs` and `assets/icons/lucide/`).
- Click: a new tab with an empty web leaf (`url = ""`) right after the active tab, the address field focused.
  In a detached window, click splits right of the focused pane, as the other items do there.
- Drag (`DraggedNew`): onto a pane edge, an empty web leaf at that edge; onto empty tab strip space, a new tab.
- The item is not in the row when the browser is not available (section 8).
- The `▾` full menu gets "New browser" too.

### 4.3 Keys and palette

| Command | Windows / Linux | macOS | Context | Palette label |
|---|---|---|---|---|
| `SplitBrowser` | `Alt+Shift+B` | `Cmd+Alt+B` | global | Browser: Split right |
| `BrowserAddress` | `Ctrl+L` | `Cmd+L` | browser pane | Browser: Focus address |
| `BrowserBack` | none | none | browser pane | Browser: Back |
| `BrowserForward` | none | none | browser pane | Browser: Forward |
| `BrowserReload` | none | none | browser pane | Browser: Reload |
| `BrowserOpenExternal` | none | none | browser pane | Browser: Open in system browser |

- `SplitBrowser` puts an empty web leaf right of the focused pane, with the address field focused; keymap names `split_browser`, `browser_address`, `browser_back`, `browser_forward`, `browser_reload`, `browser_open_external`.
- The browser pane's key context is `BROWSER_CONTEXT = "Browser"` (in `keymap.rs`, next to `EDITOR_CONTEXT`).
- `Ctrl+Shift+B` (Tern's key) is not used: it is `ToggleSidebar` in Mkrew.
- `SplitBrowser` and the browser commands are not offered (palette and keys) when the browser is not available.
- While the page has the keyboard, the page's own keys work (F5, Ctrl+R, F12, Ctrl+wheel and Ctrl+= zoom), except the keys Mkrew takes back (section 7.4).
  Mouse back / forward buttons go to the page.

## 5. Pane look

### 5.1 Frame

- A browser pane uses the same `pane_frame` as every pane: stripe, unfocused dim, badges, drag strip, drop overlay.
- The dim and the rounded corner masks cover the toolbar only: GPUI cannot paint above the native page.
- Tab chip: `PaneIcon::Web` (the globe) and the page title, or the host while there is no title, or "New browser" for an empty leaf.

### 5.2 Toolbar

One row at the top of the pane, the height and padding of the file pane's header:

- `←` back, `→` forward (disabled when the platform says there is nothing to go to; always enabled where it does not say), `⟳` reload (`✕` stop while loading).
- The address field (`INPUT` height), filling the row: shows the URL; on focus selects all; Enter loads the normalized address (section 6.2); Esc restores the current URL and gives the keyboard back to the page.
- `↗` Open in system browser (`cx.open_url` of the current URL).
- `✕` close the pane (same as Close pane).
- A 2 px accent line under the row while the page loads (between the platform's load started and finished events), no animation when reduce motion is on.
- Every button is a `quiet_button` with hover, focus, active and disabled states, and a tooltip with its shortcut when it has one.
- Colors, spacing, radius and sizes come from `theme.rs` tokens only.

### 5.3 Empty and unavailable states

- An empty leaf (`url = ""`) has no web view yet: below the toolbar it shows the Mkrew empty state "Type an address or paste a link" on `surface`, and the address field has the keyboard.
- A pane whose web view cannot be created (section 8) shows, below the toolbar, the reason and an "Open in system browser" button; the leaf stays in the layout.
- Load errors (no server on `localhost:3000`, DNS failure) show the platform's own error page inside the web view; Mkrew adds nothing.

## 6. Pages

### 6.1 Navigation events

`wry` callbacks push events into a channel; a `cx.spawn` task on the UI side reads it and updates the `Browser` and the leaf.

| Event (`wry`) | Effect |
|---|---|
| `with_on_page_load_handler` Started | loading on |
| `with_on_page_load_handler` Finished | loading off; the leaf's `url` becomes the page URL and the layout is saved (`save_layout`) if it changed |
| `with_document_title_changed_handler` | title (tab chip, tooltip) |
| `with_new_window_req_handler` | the request is refused and its URL loads in the same pane |
| `with_ipc_handler` | focus and keys (section 7.4) |

- Saving on load finished only (not on every redirect hop) keeps `tab.set_layout` traffic to one request per page.

### 6.2 Address field (`crates/mkrew-ui/src/browser/address.rs`, pure, no gpui)

`normalize(input: &str) -> Option<String>`, after trimming:

1. Empty: `None` (nothing loads).
2. A URL with a scheme (`https://…`, `http://…`, `file:///…`, `about:…`): kept as typed.
3. `localhost`, `localhost:<port>`, an IPv4 or bracketed IPv6 address, with or without a port and a path: `http://` in front.
4. No space and a dot in the host part (`example.com`, `docs.rs/wry`): `https://` in front.
5. Anything else: `https://www.google.com/search?q=<input, percent encoded>`.

### 6.3 Storage

- Windows: one `WebContext` with the data folder `<local data>/mkrew/WebView2` (`%LOCALAPPDATA%\mkrew\WebView2`), shared by every browser pane, so cookies and logins survive a restart and do not mix with the system browser.
- macOS: the default persistent `WKWebsiteDataStore` of the app.
- Devtools are on (`with_devtools(true)`), so F12 opens the platform inspector.
- The page follows Mkrew's theme (`with_theme`, and `set_theme` when the theme changes on Windows), so `prefers-color-scheme` matches.
- Downloads, permission prompts (camera, location) and file pickers behave as the platform web view does by default.

## 7. The native web view (`crates/mkrew-ui/src/browser/surface.rs`)

Built only with `cfg(any(windows, target_os = "macos"))`.

### 7.1 Creation and bounds

- `WebSurface` is a GPUI `Element` drawn below the toolbar, modeled on longbridge `gpui-webview` (that crate cannot be used: it pins `gpui-pre =0.3.9`, Mkrew pins `=0.3.7`).
- The first draw of a leaf with a URL builds the view with `WebViewBuilder::build_as_child(window)` on the GPUI window's handle (`HasWindowHandle`, already used by `desktop/win32.rs`).
- `prepaint` calls `set_bounds` with the element's bounds in logical pixels (`wry` scales by the window's DPI) and inserts a `HitboxBehavior::BlockMouse` hitbox, so GPUI does not take clicks meant for the page.
- Bounds are clipped to the window's content mask; a view with no visible area is hidden.

### 7.2 Shown or hidden

A native view paints above everything GPUI draws and stays on screen until hidden, so Mkrew hides it explicitly.

At the start of each window's render, the Shell computes the window's shown set: the web leaves of the tab shown in that window's Terminals view, minus the panes hidden by a zoom.
Every `Browser` of that window outside the set gets `set_visible(false)`.
A view in the set is shown by its `WebSurface` prepaint, unless an overlay is open.

An overlay is open when any of these holds for that window:

- `modal_open()` (dialogs, palette, pickers, Settings),
- `menu_open()` (tab, pane, workspace, file and card menus),
- the `+` menu (`new_menu`),
- a peek popover,
- a drag in progress (pane, tab, `+` item, Files row, OS files), so the drop overlay can be seen.

While an overlay is open the page's area shows the page title and URL on `surface` instead.
`ponytail:` a plain card instead of a picture of the page; take a capture (WebView2 `CapturePreview`, `WKWebView.takeSnapshot`) if the switch looks jarring.
Tooltips of other panes that reach over a page are covered by it; accepted.

### 7.3 Moving between windows

- When a web leaf moves to another window (tab tear-off, Move back, Move all back, a pane dragged across windows, a detached window closing), its view is moved with `reparent` (`WebViewExtWindows::reparent(hwnd)`, `WebViewExtMacOS::reparent(ns_window)`), so the page keeps its state.
- Minimizing a window hides its child views with it (the OS does that).

### 7.4 Focus and keys

- A click in the page gives the page the keyboard; GPUI does not see the click.
- An initialization script (`with_initialization_script`) posts `{"t":"focus"}` over IPC on `mousedown` and `focusin`, and the Shell then focuses that pane (`focus_pane_key`), so the stripe, the tab and the palette's pane commands follow.
- The same script receives the key combos Mkrew takes back: every binding with the global context, plus the browser pane context's bindings (`Ctrl+L`).
  On a match it calls `preventDefault` and `stopPropagation` and posts `{"t":"key","k":"<keystroke>"}`; the Shell runs the bound command as if GPUI had received the key.
  The list is passed as JSON when the view is built and replaced (`evaluate_script`) when the keymap changes (ADR 0073).
- So `Alt+←` / `Alt+→` (global `Focus` left / right in Mkrew) move pane focus, not page history; page history is the toolbar, the mouse buttons and the palette.
- When focus moves to another pane or an overlay opens, the Shell calls `focus_parent()`, so the keyboard comes back to GPUI.

## 8. Where the browser is not available

`browser::available() -> Result<(), Unavailable>`, computed once per run:

| Case | Reason shown |
|---|---|
| Linux (any display server) | "Linux opens web pages in your browser" |
| Windows without the WebView2 runtime (`wry::webview_version()` fails) | "The Microsoft Edge WebView2 Runtime is not installed" |
| Building a view failed at run time | the `wry` error text |

### 8.1 Effects

- Links: `cx.open_url`, one toast per run (section 4.1).
- No Browser item in the `+` row, no `SplitBrowser` key or palette entry.
- A web leaf that is already in a layout shows the unavailable state (section 5.3) and stays.

### 8.2 Linux

- `wry` is a dependency for Windows and macOS only: `[target.'cfg(any(windows, target_os = "macos"))'.dependencies] wry = { version = "0.57", features = ["devtools"] }` in `crates/mkrew-ui/Cargo.toml`.
  CI on Linux installs nothing new.
- `browser/address.rs` and the leaf, placement and toolbar code build on every OS; only `browser/surface.rs` is `cfg`-gated.

### 8.3 Remote machines

A layout comes from the daemon the UI is connected to, so a Linux UI on a Windows daemon over SSH can meet a web leaf: it shows the unavailable state, and "Open in system browser" still works.
A `localhost` page is the UI machine's localhost, not the remote machine's; forwarding a remote port stays the user's job (non-goal).

## 9. Code layout

| File | Change |
|---|---|
| `crates/mkrew-ui/src/split.rs` | `Leaf::Web`, `PaneKey::Web`, every `match` on them |
| `crates/mkrew-ui/src/tabs.rs` | `WebPlace`, `web_place`, `PaneIcon::Web` |
| `crates/mkrew-ui/src/browser/mod.rs` (new) | `Browser`, `available()`, events |
| `crates/mkrew-ui/src/browser/address.rs` (new) | `normalize` |
| `crates/mkrew-ui/src/browser/surface.rs` (new, Windows and macOS) | `WebSurface` element, `wry` builder, init script |
| `crates/mkrew-ui/src/shell/browser.rs` (new) | `open_web`, the event task, the shown set, overlays, toolbar render, commands |
| `crates/mkrew-ui/src/shell/pane_render.rs` | the `Web` arm of the pane content |
| `crates/mkrew-ui/src/shell/files_split.rs`, `shell/navigation.rs` | web links call `open_web` |
| `crates/mkrew-ui/src/plus_row.rs`, `shell/plus_row.rs` | the Browser item |
| `crates/mkrew-ui/src/keymap.rs` | the commands of section 4.3, `BROWSER_CONTEXT` |
| `crates/mkrew-ui/src/icons.rs`, `assets/icons/lucide/globe.svg` | `Icon::Globe` |
| `crates/mkrew-ui/Cargo.toml` | `wry` for Windows and macOS |
| `crates/mkrew-protocol/src/lib.rs` | `PROTOCOL_VERSION = 61` |
| `crates/mkrew-daemon/src/tabs.rs` | tests only |

Also every `match` that today lists `Session | File | Diff` (`shell/tabs.rs` close pane, `shell/pane_drop.rs` drag title and `drop_leaf`), and the wildcard matches that decide Preview, focus and hosts (`documents.rs`, `shell/hosts.rs`, `shell/focus.rs`), get a deliberate `Web` behavior: a web leaf is never a preview tab, has no host, and focuses like a file pane.

## 10. Testing

No test builds a real web view or touches the network.

Permanent unit tests:

- `address::normalize`: each rule of section 6.2, plus `localhost:5173/path`, `127.0.0.1:8080`, `[::1]:3000`, `docs.rs/wry`, `two words`, `  ` and a percent-encoded search.
- `split`: a web leaf round-trips through JSON unchanged; the same `id` twice in a tab is `LayoutError::Duplicate`; a web leaf without `id` is dropped and its split collapses to the sibling.
- `tabs::web_place`: a tab with a web leaf gives `Load` with the first one in reading order; a tab without one gives `Split` of the link's pane, or of the focused pane when the link has none; no tab gives `NewTab`.
- `plus_row_items`: Browser comes after Note and before More when the browser is available, and is absent when it is not.
- `keymap`: `Alt+Shift+B` / `Cmd+Alt+B` are `SplitBrowser`; `Ctrl+L` is `BrowserAddress` only in `BROWSER_CONTEXT`; the key list passed to a page holds the global bindings and the browser bindings and no terminal or editor binding.
- Daemon `tabs.rs`: the two tests of section 3.3.

Smoke run (`scripts/smoke.ps1`, Windows), with screenshots in `docs/screenshots/browser-pane-<view>-<theme>.png`:

1. A fake agent prints a URL in Chat; clicking it opens a browser pane on the right of that pane; a second URL loads in the same pane.
2. Ctrl+Click on a URL in a terminal does the same.
3. `+` -> Browser opens a new tab with the address field focused; typing `localhost:<port of a local test server>` loads it.
4. Opening the palette, a menu, and dragging a pane each hide the page and show the title card; closing them shows the page again.
5. Switching tabs and views hides the page; coming back shows it.
6. Tearing the tab off into a detached window keeps the page (no reload); moving it back too.
7. Quitting and starting Mkrew brings the pane back on the same URL.
8. Dark and light theme screenshots of the toolbar, the empty state and the title card.

`docs/HUMAN_CHECKS.md` gets: "Browser pane on macOS (open, keys, reparent, theme)", "Browser pane on Windows without the WebView2 runtime (toast, unavailable state)", and "check the browser pane visually (dark / light)".

## 11. Docs

- ADR 0109 "Browser pane": wry child view over CEF and an external window; one link-taking browser pane per tab; Linux falls back to the system browser; protocol 61.
- SPEC §5 (web leaf JSON), §7 (`tab.set_layout` leaves, protocol 61 line), §10 (links open in a browser pane), §13 (the commands of section 4.3), and a new §20M "Browser pane" with sections 4 to 8 in short.
- `docs/PROGRESS.md` note for the next session.

## 12. Non-goals

- Picture in picture browser cards.
- Agents reading or driving the page (snapshots, clicks, console logs), an element picker, a screenshot sent to the composer.
- A browser on Linux, or forwarding a remote machine's ports.
- Keeping back / forward history across a restart.
- A setting that sends links to the system browser.
- Find in page, bookmarks, a tab strip inside the pane, a download panel.
