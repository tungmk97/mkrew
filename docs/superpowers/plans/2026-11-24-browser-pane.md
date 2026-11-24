# Browser pane Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A web browser pane inside Mkrew: web links in Chat and terminals open in a browser pane split right of the active pane, the `+` row offers a new browser, and the pane comes back after a restart.

**Architecture:** A fourth split-tree leaf `Leaf::Web { web, id }` (key `PaneKey::Web(Ulid)`) is stored in the tab layout like files and the diff; the daemon already keeps leaves without a `session` key.
The page is the OS web view (`wry`: WebView2 on Windows, WKWebView on macOS) built as a native child of the GPUI window and kept over the pane's bounds by a custom GPUI `Element`; the Shell hides it whenever the pane is not on screen or an overlay is open.
Pure logic (address rule, key forwarding, IPC parsing, where a link goes) lives in the `mkrew-ui` library and is unit tested; Linux and machines without WebView2 fall back to the system browser.

**Tech Stack:** Rust (edition 2024), gpui-pre 0.3.7, wry 0.57 (Windows and macOS only), serde / serde_json, async-channel, raw-window-handle 0.6 (Windows).

**Spec:** `docs/superpowers/specs/2026-11-24-browser-pane-design.md` (committed on this branch). Executors read it and this plan.

## Global Constraints

- Work in the worktree `C:/Users/NguyenTung/Workspace/PersonalProject/mkrew-build-kit/mkrew/.worktrees/browser-pane` on branch `feat/browser-pane`. Give every omp tool path as an absolute path or one starting with `.worktrees/browser-pane/`, and set `cwd` for `bash`. Never touch the main checkout (another session has uncommitted "Seamless panes" work there) or other worktrees.
- Disk: C: had 3.1 GB free on 2026-11-24. Before the first build, ask the user to run the `mkrew-cleanup` skill; build only with `CARGO_TARGET_DIR=C:/Users/NguyenTung/Workspace/PersonalProject/mkrew-build-kit/mkrew/target` (shared target, no new 13 GB folder) and `-j 4`.
- Commit with `git -c user.email=tungnv975@gmail.com commit -m "<Conventional Commit>"`, only the files of the task, never `git add -A`, never push, never rewrite history.
- Verify at the end with `"C:/Program Files/Git/bin/bash.exe" scripts/verify.sh` (plain `bash` is WSL). Intermediate tasks run `cargo fmt --all`, `cargo clippy -p mkrew-ui --all-targets -j 4 -- -D warnings` and their focused tests.
- Numbers: `PROTOCOL_VERSION` 60 -> 61; ADR `0110-browser-pane.md` (0107 is on `feat/omp-tsp`, 0109 is uncommitted "Seamless panes" work). Re-check both before Task 7 and at merge time.
- `wry = { version = "0.57", features = ["devtools"] }` only under `[target.'cfg(any(windows, target_os = "macos"))'.dependencies]` of `crates/mkrew-ui/Cargo.toml`. Linux builds no web view.
- `unsafe_code = "deny"` holds: no `unsafe`. macOS moves a page to another window by rebuilding its view (URL kept, history lost); Windows uses `WebViewExtWindows::reparent`.
- Keys: `SplitBrowser` PC `alt-shift-b`, mac `cmd-alt-b`, global. `BrowserAddress` PC `ctrl-l`, mac `cmd-l`, `BROWSER_CONTEXT`. `BrowserBack`, `BrowserForward`, `BrowserReload`, `BrowserOpenExternal` unbound, `BROWSER_CONTEXT`. `BROWSER_CONTEXT = "Browser"`.
- Address rule (spec 6.2): empty -> nothing; `http`, `https`, `file`, `about` scheme kept; `localhost` / IPv4 / `[IPv6]` (optional port, path) -> `http://`; no space and a dot in the host -> `https://`; else `https://www.google.com/search?q=<percent-encoded>`.
- Web leaf JSON: `{"web": "<url>", "id": "<ulid>"}`; `""` = not loaded yet.
- Copy: "New browser", "Type an address or paste a link", "Open in system browser", "Web pages open in your browser: <reason>", "Linux opens web pages in your browser", "The Microsoft Edge WebView2 Runtime is not installed".
- UI rules (SPEC §14, skill `mkrew-ui-design`): colors, spacing, radius and font sizes only from `theme.rs` tokens; hover / press / focus only through `shell/widgets.rs` widgets or `.pressable(Look::..)`; dark and light themes; tooltips through `tooltip(..)` with the shortcut; every new action is in the palette.
- No `unwrap`/`expect` outside tests unless provably infallible (comment why). `tracing` for logs. No test builds a web view, runs an agent or touches the network. Docs: no em dash, one sentence per line in long Markdown. Never edit CHANGELOG.md.
- GPUI gotchas: `track_focus` grabs focus on any mouse down; `.hover()` at most once per element; `match self.store().x() { … self.store() … }` deadlocks, bind first.

## Review Focus

Input classes the spec implies but its tests do not name, most likely first. Each has a test in the task named in brackets.

- Text typed in the address field: surrounding spaces, `localhost:5173/x`, `127.0.0.1`, `[::1]:3000`, `example.com:8080`, `foo` (no dot), `a.b c` (a space), `javascript:alert(1)`, `C:\x.html`. Expect: trimmed; only the four schemes are kept, `javascript:` and a drive letter become a search; a word without a dot is a search. [Task 2]
- Keys pressed while a page has the keyboard: Ctrl+C / V / X / A / Z / Y and plain letters in a page text field. Expect: they stay with the page even if a user keymap binds them globally; only modified global combos (and `ctrl-l` / `cmd-l`) come back to Mkrew. [Task 2]
- Hand-broken or future web leaves in a stored layout: `{"web": 3}`, `{"web": "x"}` (no id), `{"web": "x", "id": "bad"}`, the same id twice. Expect: the bad leaf is dropped with a warning and the rest loads; the same id twice is `LayoutError::Duplicate`. [Task 1]
- A link clicked while its tab's browser pane is hidden by a zoom, or while another tab (or a detached window's tab) has a browser pane. Expect: it loads in the browser pane of the tab shown in the window clicked in (ending the zoom), never in another tab's. [Task 5]
- A web leaf that leaves every layout (pane closed, tab closed, workspace removed) or changes window. Expect: its native view is dropped or moved, never left painting over the window. [Task 3]

---

## Task order

| # | Task | Depends on |
|---|---|---|
| 1 | Web leaf, `PaneKey::Web`, protocol 61, every match arm, daemon tests | - |
| 2 | Pure browser library: address rule, forwarded keys, init script, IPC, availability | - |
| 3 | Native view: `wry`, `WebSurface` element, `Browser` state, events, visibility, overlays, window moves, focus | 1, 2 |
| 4 | Toolbar, address field, empty and unavailable states, browser commands, palette | 3 |
| 5 | Links open in the browser pane | 1, 4 |
| 6 | `+` row Browser, `SplitBrowser`, `▾` row | 4 |
| 7 | ADR 0110, SPEC, HUMAN_CHECKS, PROGRESS, smoke, screenshots, final verify | 1-6 |

Each task ends with fmt, clippy `-D warnings`, its tests green and a commit.

---

### Task 1: Web leaf, `PaneKey::Web`, protocol 61, match arms, daemon tests

**Files:**
- Modify: `crates/mkrew-ui/src/split.rs` (`PaneKey`, `Leaf`, `chat_of`, tests)
- Modify: `crates/mkrew-ui/src/tabs.rs` (`PaneIcon::Web`, `tab_icons`, `web_label`), `crates/mkrew-ui/src/tabs/tests.rs`
- Create: `crates/mkrew-ui/src/shell/browser.rs` (Shell side: `Browser`, `sync_browsers`, `render_web_pane`, `close_web_pane`, `web_project`)
- Modify: `crates/mkrew-ui/src/shell.rs` (module list), `crates/mkrew-ui/src/main.rs` (`browsers` field), `crates/mkrew-ui/src/shell/connection.rs` (field init)
- Modify: `crates/mkrew-ui/src/shell/pane_render.rs` (content arm, chip icon, chip label), `shell/tabs.rs` (`close_focused_pane`), `shell/pane_drop.rs` (`pane_title`, `drop_leaf`), `shell/hosts.rs` (window title label), `shell/detached_render.rs` (label), `shell/focus.rs` (`sync_panes` calls `sync_browsers`)
- Modify: `crates/mkrew-protocol/src/lib.rs`, `crates/mkrew-protocol/src/message/tests.rs`
- Modify: `crates/mkrew-daemon/src/tabs.rs` (tests only)

**Interfaces:**
- Produces: `PaneKey::Web(Ulid)`; `Leaf::Web { web: String, id: Ulid }`; `Leaf::web(url) -> Leaf` (fresh id); `PaneIcon::Web`; `tabs::web_label(&Tab) -> Option<Ulid>`; `Shell.browsers: HashMap<Ulid, Browser>`; `Browser { page: browser::Page }` (Task 3 adds fields); `Shell::web_project(Ulid) -> Option<Ulid>`; `Shell::close_web_pane(Ulid, &mut Window, &mut Context<Shell>)`; `Shell::render_web_pane(project_id, id, is_focused, theme, window, cx) -> AnyElement`; `Shell::sync_browsers(&mut self, cx)`.
- Consumes from Task 2: `browser::Page` (`Page::new(url)`, `page.label() -> &str`, `page.url`). If Task 2 is not merged yet, do Task 2 Step 1-4 first (it has no dependency).

- [ ] **Step 1: Write the failing split tests** (`split.rs` tests module, next to `the_three_leaf_kinds_round_trip`)

```rust
    #[test]
    fn a_web_leaf_round_trips_and_has_its_id_as_key() {
        let id = Ulid::new();
        let stored = json!({"web": "https://example.com/", "id": id.to_string()});
        let leaf: Leaf = serde_json::from_value(stored.clone()).unwrap();
        assert_eq!(leaf.key(), PaneKey::Web(id));
        assert!(leaf.has_key(&PaneKey::Web(id)));
        assert!(!leaf.has_key(&PaneKey::Web(Ulid::new())));
        assert_eq!(serde_json::to_value(&leaf).unwrap(), stored);
        // Not loaded yet: an empty URL.
        let empty = json!({"web": "", "id": id.to_string()});
        let node = Node::from_json(&empty).unwrap().expect("a layout");
        assert_eq!(Node::to_json(Some(&node)), empty);
        assert_eq!(PaneKey::Web(id).session(), None);
        // A fresh leaf gets a fresh id.
        assert_ne!(Leaf::web("a").key(), Leaf::web("a").key());
    }

    #[test]
    fn broken_web_leaves_are_dropped_and_the_same_id_twice_is_refused() {
        let [a] = ids();
        let load = |value: serde_json::Value| Node::from_json(&value).unwrap();
        for bad in [
            json!({"web": 3, "id": Ulid::new().to_string()}),
            json!({"web": "https://x.dev"}),
            json!({"web": "https://x.dev", "id": "bad"}),
        ] {
            assert_eq!(load(halves_json("vertical", pane_json(a), bad)), Some(Node::pane(a)));
        }
        let id = Ulid::new().to_string();
        let twice = halves_json(
            "vertical",
            json!({"web": "https://a.dev", "id": id}),
            json!({"web": "https://b.dev", "id": id}),
        );
        assert!(matches!(
            Node::from_json(&twice),
            Err(LayoutError::Duplicate(PaneKey::Web(_)))
        ));
    }
```

- [ ] **Step 2: Run them to see them fail**

Run: `cargo test -p mkrew-ui --lib split::tests::a_web_leaf -j 4`
Expected: compile error, `no variant named Web`.

- [ ] **Step 3: Add the variant and the leaf** (`split.rs`)

```rust
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum PaneKey {
    Session(Ulid),
    File(FileKey),
    Diff,
    /// A browser pane (ADR 0110), by its leaf's own id: two panes may show one URL.
    Web(Ulid),
}

impl PaneKey {
    /// The session of a session pane.
    pub fn session(&self) -> Option<Ulid> {
        match self {
            PaneKey::Session(id) => Some(*id),
            PaneKey::File(_) | PaneKey::Diff | PaneKey::Web(_) => None,
        }
    }
}
```

In `enum Leaf`, after `Diff { diff: bool },`:

```rust
    /// A browser pane (ADR 0110): `web` is the page it shows (`""` before the first load),
    /// `id` names the pane.
    Web { web: String, id: Ulid },
```

In `impl Leaf`:

```rust
    /// A new browser pane on `url` (`""`: the address field waits for one).
    pub fn web(url: impl Into<String>) -> Leaf {
        Leaf::Web { web: url.into(), id: Ulid::new() }
    }
```

`key()` gets `Leaf::Web { id, .. } => PaneKey::Web(*id),`; `has_key()` gets `(Leaf::Web { id, .. }, PaneKey::Web(key)) => id == key,` before `_ => false`; `chat_of` (about line 494) gets `Leaf::File { .. } | Leaf::Diff { .. } | Leaf::Web { .. } => None,`.

- [ ] **Step 4: Run the split tests**

Run: `cargo test -p mkrew-ui --lib split:: -j 4`
Expected: compile errors in `tabs.rs` and `shell/*` only appear for the binary; the lib tests of `split` PASS. If `cargo test --lib` stops on `tabs.rs:979` (exhaustive `pane_icon`), do Step 5 first.

- [ ] **Step 5: Tab icon and label** (`tabs.rs`)

```rust
pub enum PaneIcon {
    /// An agent pane; the profile id (its badge is looked up by the caller).
    Agent(String),
    Shell,
    Run,
    /// A file pane; its stored path (the caller draws the file icon).
    File(String),
    /// The diff pane.
    Diff,
    /// A browser pane.
    Web,
}
```

In `tab_icons`'s `pane_icon` closure add `PaneKey::Web(_) => Some(PaneIcon::Web),`.

After `diff_label`:

```rust
/// The browser pane whose page names the tab: the focused pane is a browser and the tab keeps
/// its default name (a name the user gave the tab always wins), like [`diff_label`]. The
/// caller looks the page title up.
pub fn web_label(tab: &Tab) -> Option<Ulid> {
    match tab.focused()? {
        PaneKey::Web(id) if has_default_title(tab) => Some(*id),
        _ => None,
    }
}
```

Test (`tabs/tests.rs`):

```rust
#[test]
fn a_tab_whose_focused_pane_is_a_browser_is_named_by_it() {
    let project = Ulid::new();
    let (a, web) = (Ulid::new(), Ulid::new());
    let tree = json!({"split": "vertical", "ratio": 0.5, "children": [
        {"session": a.to_string()}, {"web": "https://x.dev", "id": web.to_string()}
    ]});
    let mut tabs = Tabs::new(project);
    tabs.load(vec![wire(project, "Terminal", 1.0, tree.clone())]);
    assert_eq!(web_label(tabs.active().unwrap()), None, "a session has the focus");
    assert!(tabs.focus(&PaneKey::Web(web)));
    let tab = tabs.active().unwrap();
    assert_eq!(web_label(tab), Some(web));
    assert_eq!(tab_icons(tab, &[], false).panes.last(), Some(&PaneIcon::Web));
    let mut named = Tabs::new(project);
    named.load(vec![wire(project, "Review", 1.0, tree)]);
    assert!(named.focus(&PaneKey::Web(web)));
    assert_eq!(web_label(named.active().unwrap()), None, "the user's name wins");
}
```

Run: `cargo test -p mkrew-ui --lib tabs:: -j 4` - Expected: PASS.

- [ ] **Step 6: Shell side, part 1** - create `crates/mkrew-ui/src/shell/browser.rs`

```rust
//! Browser panes (ADR 0110): the Shell's page state per web leaf, and the pane's body.

use crate::*;
use mkrew_ui::browser::Page;

/// One browser pane's live state, keyed by its leaf id in `Shell.browsers`.
pub(crate) struct Browser {
    pub(crate) page: Page,
}

impl Shell {
    /// The workspace whose tabs hold browser pane `id`.
    pub(crate) fn web_project(&self, id: Ulid) -> Option<Ulid> {
        let key = PaneKey::Web(id);
        (self.tabs.iter())
            .find(|(_, tabs)| tabs.tab_with(&key).is_some())
            .map(|(project, _)| *project)
    }

    /// Every web leaf of every layout has its state, and only those: a closed pane's page goes.
    pub(crate) fn sync_browsers(&mut self) {
        let mut leaves: HashMap<Ulid, String> = HashMap::new();
        for (project, tabs) in &self.tabs {
            // Tabs still loading list no leaves yet: nothing may be dropped.
            if self.loading_tabs.contains(project) {
                return;
            }
            for tab in tabs.tabs() {
                let Some(layout) = tab.layout() else { continue };
                for key in tab.keys() {
                    if let (PaneKey::Web(id), Some(Leaf::Web { web, .. })) =
                        (&key, layout.leaf_of(&key))
                    {
                        leaves.insert(*id, web.clone());
                    }
                }
            }
        }
        self.browsers.retain(|id, _| leaves.contains_key(id));
        for (id, url) in leaves {
            self.browsers.entry(id).or_insert_with(|| Browser { page: Page::new(url) });
        }
    }

    /// Closes browser pane `id` (its tab goes with its last pane).
    pub(crate) fn close_web_pane(
        &mut self,
        id: Ulid,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let Some(project_id) = self.web_project(id) else {
            return;
        };
        let write = (self.tabs.get_mut(&project_id)).and_then(|tabs| tabs.close_leaf(&PaneKey::Web(id)));
        if let Some(write) = write {
            self.save_layout(project_id, write, window, cx);
        }
        self.sync_panes(window, cx);
        cx.notify();
    }

    /// The body of browser pane `id`: the empty state until Task 3 draws the page.
    pub(crate) fn render_web_pane(
        &self,
        _project_id: Ulid,
        _id: Ulid,
        _is_focused: bool,
        theme: &Theme,
        _window: &Window,
        _cx: &mut Context<'_, Self>,
    ) -> gpui::AnyElement {
        empty_state(theme, None, None, "Type an address or paste a link".into(), None)
            .size_full()
            .into_any_element()
    }
}
```

Register `pub(crate) mod browser;` in the module list of `crates/mkrew-ui/src/shell.rs` (alphabetical, it lists one file per area). In `main.rs` next to `documents: HashMap<Ulid, Documents>,` add:

```rust
    /// Browser panes' page state, by leaf id (ADR 0110); follows the layouts (`sync_browsers`).
    browsers: HashMap<Ulid, shell::browser::Browser>,
```

and `browsers: HashMap::new(),` next to `documents: HashMap::new(),` in `Shell::new` (`shell/connection.rs`). In `sync_panes` (`shell/focus.rs`), right after `self.sync_documents(window, cx);`, add `self.sync_browsers();`.

- [ ] **Step 7: Every match arm** (the compiler lists them; these are all of them)

`shell/pane_render.rs` content match (about line 604):

```rust
            PaneKey::Diff => self.render_diff_pane(project_id, theme, window, cx),
            PaneKey::Web(id) => self.render_web_pane(project_id, *id, is_focused, theme, window, cx),
```

`shell/pane_render.rs` chip icons (about line 251), after the `PaneIcon::Diff` arm:

```rust
                PaneIcon::Web => icon_svg(Icon::Globe, theme::ICON_SIZE_SMALL, theme.text_muted)
                    .into_any_element(),
```

The chip label chain (`pane_render.rs` about lines 46-49 and 206-208), `shell/hosts.rs` about 63-68 and `shell/detached_render.rs` about 173-180 each get, after `.or_else(|| diff_label(tab))`:

```rust
            .or_else(|| web_label(tab).and_then(|id| self.browsers.get(&id)).map(|b| b.page.label()))
```

and `web_label` joins the `use` of `diff_label` in `main.rs` (about lines 113-117).

`shell/tabs.rs` `close_focused_pane`, before `None => {}`:

```rust
            Some(PaneKey::Web(id)) => self.close_web_pane(id, window, cx),
```

`shell/pane_drop.rs` `pane_title`, after `PaneKey::Diff => "Diff".into(),`:

```rust
            PaneKey::Web(id) => (self.browsers.get(id))
                .map_or("New browser", |browser| browser.page.label())
                .to_owned()
                .into(),
```

`shell/pane_drop.rs` `drop_leaf`, before `Leaf::Session { .. } =>`:

```rust
            Leaf::Web { web, id } => {
                self.place_leaf(project_id, Leaf::Web { web, id }, place, window, cx);
            }
```

`Icon::Globe` comes from Task 4 Step 1; do that step now if it is not in yet (add `Globe => "globe",` between `GitBranch` and `History` in `icons.rs` and create `icon/lucide/globe.svg`, content in Task 4 Step 1).

- [ ] **Step 8: Protocol 61**

`crates/mkrew-protocol/src/lib.rs:16`: `pub const PROTOCOL_VERSION: u32 = 61;`
`crates/mkrew-protocol/src/message/tests.rs`: rename and update the pin:

```rust
#[test]
fn the_browser_pane_protocol_is_version_61() {
    assert_eq!(crate::PROTOCOL_VERSION, 61);
}
```

- [ ] **Step 9: Daemon tests** (`crates/mkrew-daemon/src/tabs.rs` tests; no code change)

```rust
    fn web(id: Ulid) -> Value {
        json!({"web": "https://x.dev", "id": id.to_string()})
    }

    #[test]
    fn web_leaves_are_kept_like_file_leaves() {
        let [a, page] = [Ulid::new(), Ulid::new()];
        assert!(layout_sessions(&web(page)).is_empty());
        assert_eq!(layout_sessions(&split("vertical", leaf(a), web(page))), [a]);
        // The session goes, the browser pane stays and its tab with it.
        assert_eq!(without_session(&split("vertical", leaf(a), web(page)), a), Some(web(page)));
        // A web leaf whose id is the session's id is still a web leaf.
        assert_eq!(without_session(&web(a), a), None);
    }
```

- [ ] **Step 10: Build, lint, test**

Run: `cargo fmt --all`, `cargo clippy -p mkrew-ui -p mkrew-protocol -p mkrew-daemon --all-targets -j 4 -- -D warnings`, `cargo test -p mkrew-ui --lib -j 4 split:: tabs::`, `cargo test -p mkrew-protocol -j 4`, `cargo test -p mkrew-daemon --lib tabs:: -j 4`.
Expected: all PASS.

- [ ] **Step 11: Commit**

```bash
git add crates/mkrew-ui/src/split.rs crates/mkrew-ui/src/tabs.rs crates/mkrew-ui/src/tabs/tests.rs crates/mkrew-ui/src/shell/browser.rs crates/mkrew-ui/src/shell.rs crates/mkrew-ui/src/main.rs crates/mkrew-ui/src/shell/connection.rs crates/mkrew-ui/src/shell/pane_render.rs crates/mkrew-ui/src/shell/tabs.rs crates/mkrew-ui/src/shell/pane_drop.rs crates/mkrew-ui/src/shell/hosts.rs crates/mkrew-ui/src/shell/detached_render.rs crates/mkrew-ui/src/shell/focus.rs crates/mkrew-protocol/src/lib.rs crates/mkrew-protocol/src/message/tests.rs crates/mkrew-daemon/src/tabs.rs
git -c user.email=tungnv975@gmail.com commit -m "feat(ui): web leaf in the split tree, protocol 61"
```

---

### Task 2: Pure browser library

**Files:**
- Create: `crates/mkrew-ui/src/browser/mod.rs`, `crates/mkrew-ui/src/browser/address.rs`, `crates/mkrew-ui/src/browser/keys.rs`
- Modify: `crates/mkrew-ui/src/lib.rs` (`pub mod browser;` between `pub mod board;`-area entries, alphabetical)

**Interfaces:**
- Produces:
  - `browser::Page { pub url: String, pub title: String, pub loading: bool }`, `Page::new(url: impl Into<String>) -> Page`, `Page::label(&self) -> &str`.
  - `browser::PageEvent { Started, Finished(String), Title(String), Open(String), Focus, Key(String) }`.
  - `browser::parse_ipc(body: &str) -> Option<PageEvent>`.
  - `browser::Unavailable { Linux, NoRuntime, Failed(String) }` with `Display`; `browser::available() -> Result<(), Unavailable>` (cached per run).
  - `browser::address::normalize(input: &str) -> Option<String>`.
  - `browser::keys::forwarded(keymap: &Keymap) -> Vec<String>`, `browser::keys::key_name(&Keystroke) -> String`, `browser::keys::init_script(keys: &[String]) -> String`, `browser::keys::set_keys_script(keys: &[String]) -> String`.
  - `keymap::BROWSER_CONTEXT` (added here, used by Task 4).

- [ ] **Step 1: Failing tests for the address rule** (`browser/address.rs`)

```rust
#[cfg(test)]
mod tests {
    use super::normalize;

    #[test]
    fn the_address_rule() {
        let n = |s| normalize(s);
        assert_eq!(n("   "), None);
        assert_eq!(n("https://docs.rs/wry"), Some("https://docs.rs/wry".into()));
        assert_eq!(n("  HTTP://X.dev "), Some("HTTP://X.dev".into()));
        assert_eq!(n("file:///C:/a.html"), Some("file:///C:/a.html".into()));
        assert_eq!(n("about:blank"), Some("about:blank".into()));
        assert_eq!(n("localhost"), Some("http://localhost".into()));
        assert_eq!(n("localhost:5173/path"), Some("http://localhost:5173/path".into()));
        assert_eq!(n("127.0.0.1:8080"), Some("http://127.0.0.1:8080".into()));
        assert_eq!(n("[::1]:3000"), Some("http://[::1]:3000".into()));
        assert_eq!(n("example.com"), Some("https://example.com".into()));
        assert_eq!(n("docs.rs/wry"), Some("https://docs.rs/wry".into()));
        assert_eq!(n("example.com:8080"), Some("https://example.com:8080".into()));
        let search = |q: &str| Some(format!("https://www.google.com/search?q={q}"));
        assert_eq!(n("foo"), search("foo"));
        assert_eq!(n("two words"), search("two+words"));
        assert_eq!(n("a.b c"), search("a.b+c"));
        assert_eq!(n("javascript:alert(1)"), search("javascript%3Aalert%281%29"));
        assert_eq!(n(r"C:\x.html"), search("C%3A%5Cx.html"));
        assert_eq!(n("c++ & rust?"), search("c%2B%2B+%26+rust%3F"));
    }
}
```

- [ ] **Step 2: Implement `normalize`** (`browser/address.rs`)

```rust
//! The browser pane's address field (spec 6.2): what a typed text loads.

/// The URL `input` loads, `None` for blank input. A kept scheme, a local address over http,
/// a host name over https, anything else a Google search.
pub fn normalize(input: &str) -> Option<String> {
    let text = input.trim();
    if text.is_empty() {
        return None;
    }
    if let Some((scheme, _)) = text.split_once(':')
        && ["http", "https", "file", "about"].contains(&scheme.to_ascii_lowercase().as_str())
    {
        return Some(text.to_owned());
    }
    if !text.contains(char::is_whitespace) {
        let host = host_of(text);
        if is_local(host) {
            return Some(format!("http://{text}"));
        }
        if host.contains('.') && !host.contains(['\\', ':']) {
            return Some(format!("https://{text}"));
        }
    }
    Some(format!("https://www.google.com/search?q={}", query(text)))
}

/// `text` up to the first `/`, without its port (a bracketed IPv6 address keeps its brackets).
fn host_of(text: &str) -> &str {
    let authority = text.split('/').next().unwrap_or(text);
    if authority.starts_with('[') {
        return authority.split_inclusive(']').next().unwrap_or(authority);
    }
    authority.split(':').next().unwrap_or(authority)
}

fn is_local(host: &str) -> bool {
    host.eq_ignore_ascii_case("localhost")
        || host.parse::<std::net::Ipv4Addr>().is_ok()
        || host
            .strip_prefix('[')
            .and_then(|h| h.strip_suffix(']'))
            .is_some_and(|h| h.parse::<std::net::Ipv6Addr>().is_ok())
}

/// `application/x-www-form-urlencoded` of `text`: space as `+`, unreserved bytes kept.
fn query(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for byte in text.bytes() {
        match byte {
            b' ' => out.push('+'),
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(char::from(byte));
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}
```

Note: `example.com:8080` has host `example.com` (dot) -> https; `C:\x.html` has host `C` -> search; `javascript:alert(1)` has host `javascript` -> search.

Run: `cargo test -p mkrew-ui --lib browser::address -j 4` - Expected: PASS.

- [ ] **Step 3: Page, events, IPC, availability** (`browser/mod.rs`)

```rust
//! Browser panes (spec 2026-11-24, ADR 0110): what the Shell keeps per page, what a page
//! reports, and whether this machine can show pages at all. Pure, no gpui, except
//! `native` (Windows and macOS), which wraps the `wry` view.

pub mod address;
pub mod keys;

use std::fmt;
use std::sync::OnceLock;

/// The page a browser pane shows.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Page {
    /// The address loaded or loading; `""` before the first one.
    pub url: String,
    pub title: String,
    pub loading: bool,
}

impl Page {
    pub fn new(url: impl Into<String>) -> Page {
        Page { url: url.into(), ..Page::default() }
    }

    /// What names the pane: its title, else its URL, else "New browser".
    pub fn label(&self) -> &str {
        [self.title.as_str(), self.url.as_str()]
            .into_iter()
            .find(|text| !text.trim().is_empty())
            .unwrap_or("New browser")
    }
}

/// What a page tells the Shell (wry callbacks and the init script's IPC messages).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PageEvent {
    Started,
    /// Loaded; the page's URL.
    Finished(String),
    Title(String),
    /// The page asked for a new window (`target=_blank`, `window.open`): load it here.
    Open(String),
    /// The page took the keyboard or a click.
    Focus,
    /// A Mkrew key combo pressed in the page (`key_name` form).
    Key(String),
}

/// An IPC message of the init script: `{"t":"focus"}` or `{"t":"key","k":"alt-shift-b"}`.
pub fn parse_ipc(body: &str) -> Option<PageEvent> {
    let value: serde_json::Value = serde_json::from_str(body).ok()?;
    match value.get("t")?.as_str()? {
        "focus" => Some(PageEvent::Focus),
        "key" => Some(PageEvent::Key(value.get("k")?.as_str()?.to_owned())),
        _ => None,
    }
}

/// Why pages open in the system browser instead.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Unavailable {
    Linux,
    NoRuntime,
    Failed(String),
}

impl fmt::Display for Unavailable {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Unavailable::Linux => f.write_str("Linux opens web pages in your browser"),
            Unavailable::NoRuntime => {
                f.write_str("The Microsoft Edge WebView2 Runtime is not installed")
            }
            Unavailable::Failed(error) => f.write_str(error),
        }
    }
}

/// Whether this machine shows pages in Mkrew; checked once per run.
pub fn available() -> Result<(), Unavailable> {
    static CHECK: OnceLock<Result<(), Unavailable>> = OnceLock::new();
    CHECK.get_or_init(check).clone()
}

#[cfg(windows)]
fn check() -> Result<(), Unavailable> {
    wry::webview_version().map(drop).map_err(|_| Unavailable::NoRuntime)
}

#[cfg(target_os = "macos")]
fn check() -> Result<(), Unavailable> {
    Ok(())
}

#[cfg(not(any(windows, target_os = "macos")))]
fn check() -> Result<(), Unavailable> {
    Err(Unavailable::Linux)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_page_is_named_by_its_title_then_its_url() {
        let mut page = Page::new("");
        assert_eq!(page.label(), "New browser");
        page.url = "https://x.dev/".into();
        assert_eq!(page.label(), "https://x.dev/");
        page.title = "  ".into();
        assert_eq!(page.label(), "https://x.dev/", "a blank title does not count");
        page.title = "X".into();
        assert_eq!(page.label(), "X");
    }

    #[test]
    fn ipc_messages_parse_and_noise_is_ignored() {
        assert_eq!(parse_ipc(r#"{"t":"focus"}"#), Some(PageEvent::Focus));
        assert_eq!(
            parse_ipc(r#"{"t":"key","k":"alt-shift-b"}"#),
            Some(PageEvent::Key("alt-shift-b".into()))
        );
        for noise in ["", "{", r#"{"t":"key"}"#, r#"{"t":3}"#, r#"{"t":"other"}"#, "[]"] {
            assert_eq!(parse_ipc(noise), None, "{noise}");
        }
    }

    #[test]
    fn the_reasons_read_as_the_spec_says() {
        assert_eq!(Unavailable::Linux.to_string(), "Linux opens web pages in your browser");
        assert_eq!(
            Unavailable::NoRuntime.to_string(),
            "The Microsoft Edge WebView2 Runtime is not installed"
        );
    }
}
```

The `wry` dependency for `check` on Windows is added in Task 3 Step 1; until then, gate the Windows `check` on `cfg(any())` is not allowed - instead add the `wry` line of Task 3 Step 1 to `Cargo.toml` in this task (it is only a dependency line).

- [ ] **Step 4: Run**

Run: `cargo test -p mkrew-ui --lib browser:: -j 4` - Expected: PASS.

- [ ] **Step 5: Failing tests for forwarded keys** (`browser/keys.rs`)

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::keymap::{Keymap, Os};
    use gpui::Keystroke;

    #[test]
    fn key_names_put_modifiers_in_one_order() {
        let name = |keys| key_name(&Keystroke::parse(keys).unwrap());
        assert_eq!(name("shift-alt-b"), "alt-shift-b");
        assert_eq!(name("cmd-alt-b"), "alt-cmd-b");
        assert_eq!(name("ctrl-shift-p"), "ctrl-shift-p");
        assert_eq!(name("f11"), "f11");
    }

    #[test]
    fn the_page_gives_back_global_combos_and_keeps_its_text_keys() {
        let keymap = Keymap::defaults_for(Os::Windows);
        let keys = forwarded(&keymap);
        assert!(keys.contains(&"alt-shift-b".to_owned()), "SplitBrowser");
        assert!(keys.contains(&"alt-left".to_owned()), "Focus left is global");
        assert!(keys.contains(&"ctrl-l".to_owned()), "BrowserAddress");
        // Terminal and editor bindings stay with the page.
        assert!(!keys.iter().any(|k| k == "ctrl-shift-c"), "terminal Copy");
        assert!(!keys.iter().any(|k| k == "ctrl-s"), "editor Save");
        // Text editing keys never leave the page, even if bound globally.
        for text in ["ctrl-c", "ctrl-v", "ctrl-x", "ctrl-a", "ctrl-z", "ctrl-y"] {
            assert!(!keys.iter().any(|k| k == text), "{text}");
        }
        // An unmodified key never leaves the page (except F-keys).
        assert!(keys.iter().all(|k| k.contains('-') || k.starts_with('f')));
    }

    #[test]
    fn the_scripts_carry_the_key_list() {
        let keys = ["alt-shift-b".to_owned()];
        let script = init_script(&keys);
        assert!(script.contains(r#"["alt-shift-b"]"#));
        assert!(script.contains("window.ipc.postMessage"));
        assert!(set_keys_script(&keys).contains(r#"["alt-shift-b"]"#));
    }
}
```

- [ ] **Step 6: Implement** (`browser/keys.rs`; add `pub const BROWSER_CONTEXT: &str = "Browser";` with a doc comment "Key context of a browser pane (ADR 0110)." in `keymap.rs` after `EDITOR_CONTEXT`)

```rust
//! The keys a page gives back to Mkrew (spec 7.4): while a web view has the keyboard GPUI
//! sees nothing, so an init script catches Mkrew's combos and posts them over IPC.

use crate::keymap::{BROWSER_CONTEXT, Keymap};
use gpui::Keystroke;

/// `keystroke` as the init script names a `keydown`: `ctrl-`, `alt-`, `shift-`, `cmd-`, then
/// the key, so both sides agree whatever order the keymap wrote.
pub fn key_name(keystroke: &Keystroke) -> String {
    let m = &keystroke.modifiers;
    let mut name = String::new();
    for (on, prefix) in [(m.control, "ctrl-"), (m.alt, "alt-"), (m.shift, "shift-"), (m.platform, "cmd-")] {
        if on {
            name.push_str(prefix);
        }
    }
    name.push_str(&keystroke.key);
    name
}

/// Ctrl / Cmd + these edit text in a page's fields: they never leave the page.
const TEXT_KEYS: [&str; 6] = ["a", "c", "v", "x", "y", "z"];

/// The combos a page posts back: every global binding and every browser pane binding that
/// holds Ctrl, Alt or Cmd (or is an F-key), minus the text editing keys.
pub fn forwarded(keymap: &Keymap) -> Vec<String> {
    let mut keys: Vec<String> = keymap
        .bindings()
        .iter()
        .filter(|(command, _)| matches!(command.context(), None | Some(BROWSER_CONTEXT)))
        .map(|(_, keystroke)| keystroke)
        .filter(|keystroke| {
            let m = &keystroke.modifiers;
            let function = keystroke.key.starts_with('f') && keystroke.key.len() > 1;
            let modified = m.control || m.alt || m.platform;
            let text = (m.control || m.platform) && !m.alt && !m.shift
                && TEXT_KEYS.contains(&keystroke.key.as_str());
            (modified || function) && !text
        })
        .map(key_name)
        .collect();
    keys.sort();
    keys.dedup();
    keys
}

/// Runs in every page before its own scripts: posts focus, and posts and swallows the keys.
pub fn init_script(keys: &[String]) -> String {
    format!(
        r#"(() => {{
  const post = (m) => window.ipc.postMessage(JSON.stringify(m));
  window.__mkrewKeys = new Set({keys});
  const codes = {{ ArrowLeft: "left", ArrowRight: "right", ArrowUp: "up", ArrowDown: "down",
    Enter: "enter", Escape: "escape", Space: "space", Tab: "tab", Backspace: "backspace",
    Delete: "delete", Home: "home", End: "end", PageUp: "pageup", PageDown: "pagedown",
    Equal: "=", Minus: "-", Comma: ",", Period: ".", Slash: "/", Backslash: "\\",
    BracketLeft: "[", BracketRight: "]", Semicolon: ";", Quote: "'", Backquote: "`" }};
  const key = (code) => code.startsWith("Key") ? code.slice(3).toLowerCase()
    : code.startsWith("Digit") ? code.slice(5)
    : /^F\d+$/.test(code) ? code.toLowerCase()
    : codes[code];
  addEventListener("mousedown", () => post({{ t: "focus" }}), true);
  addEventListener("focusin", () => post({{ t: "focus" }}), true);
  addEventListener("keydown", (e) => {{
    const k = key(e.code);
    if (!k) return;
    const name = (e.ctrlKey ? "ctrl-" : "") + (e.altKey ? "alt-" : "")
      + (e.shiftKey ? "shift-" : "") + (e.metaKey ? "cmd-" : "") + k;
    if (!window.__mkrewKeys.has(name)) return;
    e.preventDefault();
    e.stopPropagation();
    post({{ t: "key", k: name }});
  }}, true);
}})();"#,
        keys = json_list(keys)
    )
}

/// Replaces the key list in a live page (the keymap changed, ADR 0073).
pub fn set_keys_script(keys: &[String]) -> String {
    format!("window.__mkrewKeys = new Set({});", json_list(keys))
}

fn json_list(keys: &[String]) -> String {
    serde_json::to_string(keys).unwrap_or_else(|_| "[]".to_owned())
}
```

`Keymap::bindings()` may not exist publicly: if `Keymap { bindings }` has no accessor, add to `keymap.rs`:

```rust
    /// Every binding, in `Command::all()` order.
    pub fn bindings(&self) -> &[(Command, Keystroke)] {
        &self.bindings
    }
```

(If one exists with another name or shape, use it and adapt the filter.) The JS key names match gpui's: letters lower case, digits, `left` / `right` / `up` / `down`, `f1`..`f12`, punctuation as the character.

- [ ] **Step 7: Run, lint, commit**

Run: `cargo test -p mkrew-ui --lib browser:: -j 4`, `cargo clippy -p mkrew-ui --all-targets -j 4 -- -D warnings`.
Expected: PASS.

```bash
git add crates/mkrew-ui/src/browser crates/mkrew-ui/src/lib.rs crates/mkrew-ui/src/keymap.rs crates/mkrew-ui/Cargo.toml Cargo.lock
git -c user.email=tungnv975@gmail.com commit -m "feat(ui): browser pane library (address rule, page keys, IPC, availability)"
```

---

### Task 3: Native view

**Files:**
- Modify: `crates/mkrew-ui/Cargo.toml` (wry, if not added in Task 2; `objc2-app-kit` needs no change: no macOS reparent)
- Create: `crates/mkrew-ui/src/browser/native.rs` (Windows and macOS), `crates/mkrew-ui/src/browser/no_native.rs` (other OS)
- Modify: `crates/mkrew-ui/src/browser/mod.rs` (module wiring), `crates/mkrew-ui/src/shell/browser.rs`, `crates/mkrew-ui/src/shell/connection.rs` (event channel and task), `crates/mkrew-ui/src/main.rs` (`Render::render` calls `prepare_browsers`; `page_events` field), `crates/mkrew-ui/src/shell/detached_render.rs` (`render_host` calls `prepare_browsers`), `crates/mkrew-ui/src/shell/workspace_actions.rs` (`set_keymap` pushes keys), `crates/mkrew-ui/src/shell/pane_render.rs` (browser key context and focus)

**Interfaces:**
- Consumes: Task 1 (`Browser`, `sync_browsers`, `web_project`), Task 2 (`Page`, `PageEvent`, `parse_ipc`, `available`, `keys::*`).
- Produces:
  - `browser::NativeView` with `build(window: &Window, id: Ulid, url: &str, keys: &[String], dark: bool, data_dir: Option<&Path>, events: async_channel::Sender<(Ulid, PageEvent)>) -> Result<NativeView, String>`, `load(&self, url)`, `script(&self, js)`, `set_shown(&self, bool)`, `set_dark(&self, bool)`, `focus(&self)`, `focus_parent(&self)`, `move_to(&self, window: &Window) -> bool` (false: rebuild), `surface(&self, show: bool) -> WebSurface`.
  - `Browser { page, native: Option<NativeView>, host: Option<HostId>, error: Option<String>, focus: FocusHandle, address: Input, address_focus: FocusHandle, page_keys: bool, editing: bool }`.
  - `Shell::prepare_browsers(&mut self, host: HostId, window: &mut Window, cx: &mut Context<Shell>)`, `Shell::browser_overlay(&self, cx: &App) -> bool`, `Shell::page_event(&mut self, id, PageEvent, &mut Window, &mut Context<Shell>)`, `Shell::navigate(&mut self, id, url: String, &mut Window, &mut Context<Shell>)`.

- [ ] **Step 1: Dependency** (`crates/mkrew-ui/Cargo.toml`, new section after the macOS one)

```toml
# Browser panes (ADR 0110): the OS web view (WebView2, WKWebView) as a child of the window.
# Linux shows pages in the system browser and builds none of it.
[target.'cfg(any(windows, target_os = "macos"))'.dependencies]
wry = { version = "0.57", features = ["devtools"] }
```

Run: `cargo build -p mkrew-ui -j 4` (with the shared `CARGO_TARGET_DIR`). Expected: builds; `Cargo.lock` gains `wry`, `webview2-com` (Windows).

- [ ] **Step 2: The no-view stand-in** (`browser/no_native.rs`; it keeps every call site free of `cfg`)

```rust
//! No web view on this OS (Linux): a `NativeView` cannot exist, so every method is unreachable.

use super::PageEvent;
use gpui::Window;
use std::path::Path;
use ulid::Ulid;

pub enum NativeView {}
pub enum WebSurface {}

impl NativeView {
    pub fn build(
        _window: &Window,
        _id: Ulid,
        _url: &str,
        _keys: &[String],
        _dark: bool,
        _data_dir: Option<&Path>,
        _events: async_channel::Sender<(Ulid, PageEvent)>,
    ) -> Result<NativeView, String> {
        Err(super::Unavailable::Linux.to_string())
    }
    pub fn load(&self, _url: &str) { match *self {} }
    pub fn script(&self, _js: &str) { match *self {} }
    pub fn set_shown(&self, _shown: bool) { match *self {} }
    pub fn set_dark(&self, _dark: bool) { match *self {} }
    pub fn focus(&self) { match *self {} }
    pub fn focus_parent(&self) { match *self {} }
    pub fn move_to(&self, _window: &Window) -> bool { match *self {} }
    pub fn surface(&self, _show: bool) -> WebSurface { match *self {} }
}

impl gpui::IntoElement for WebSurface {
    type Element = gpui::Empty;
    fn into_element(self) -> gpui::Empty { match self {} }
}
```

In `browser/mod.rs`:

```rust
#[cfg(any(windows, target_os = "macos"))]
mod native;
#[cfg(not(any(windows, target_os = "macos")))]
#[path = "no_native.rs"]
mod native;
pub use native::{NativeView, WebSurface};
```

- [ ] **Step 3: The view and its element** (`browser/native.rs`)

```rust
//! The page of a browser pane: a `wry` web view (WebView2 / WKWebView) made a child of the
//! GPUI window, kept over the pane by [`WebSurface`] (modeled on longbridge `gpui-webview`,
//! which pins another gpui). A native view paints above everything GPUI draws and stays on
//! screen until hidden: the Shell hides it (spec 7.2).

use super::{PageEvent, keys, parse_ipc};
use gpui::{
    App, Bounds, ElementId, GlobalElementId, Hitbox, HitboxBehavior, InspectorElementId,
    IntoElement, LayoutId, MouseDownEvent, Pixels, Size, Style, Window,
};
use std::cell::Cell;
use std::path::Path;
use std::rc::Rc;
use ulid::Ulid;
use wry::dpi::{LogicalPosition, LogicalSize};
use wry::{NewWindowResponse, PageLoadEvent, Rect, WebContext, WebViewBuilder};

pub struct NativeView {
    view: Rc<wry::WebView>,
    shown: Rc<Cell<bool>>,
    dark: Cell<bool>,
}

impl NativeView {
    /// Builds the page of browser pane `id` on `url` as a child of `window` (hidden until its
    /// surface is drawn). Events go to `events` with the pane's id.
    pub fn build(
        window: &Window,
        id: Ulid,
        url: &str,
        keys: &[String],
        dark: bool,
        data_dir: Option<&Path>,
        events: async_channel::Sender<(Ulid, PageEvent)>,
    ) -> Result<NativeView, String> {
        let mut context = WebContext::new(data_dir.map(Path::to_path_buf));
        let send = move |event: PageEvent| {
            let _ = events.try_send((id, event));
        };
        let (on_load, on_title, on_open, on_ipc) =
            (send.clone(), send.clone(), send.clone(), send);
        let builder = WebViewBuilder::new_with_web_context(&mut context)
            .with_url(url)
            .with_visible(false)
            .with_devtools(true)
            .with_initialization_script(keys::init_script(keys))
            .with_on_page_load_handler(move |event, url| {
                on_load(match event {
                    PageLoadEvent::Started => PageEvent::Started,
                    PageLoadEvent::Finished => PageEvent::Finished(url),
                })
            })
            .with_document_title_changed_handler(move |title| on_title(PageEvent::Title(title)))
            .with_new_window_req_handler(move |url, _| {
                on_open(PageEvent::Open(url));
                NewWindowResponse::Deny
            })
            .with_ipc_handler(move |request| {
                if let Some(event) = parse_ipc(request.body()) {
                    on_ipc(event);
                }
            });
        #[cfg(windows)]
        let builder = {
            use wry::WebViewBuilderExtWindows as _;
            builder.with_theme(theme(dark))
        };
        let view = builder.build_as_child(window).map_err(|error| error.to_string())?;
        Ok(NativeView { view: Rc::new(view), shown: Rc::new(Cell::new(false)), dark: Cell::new(dark) })
    }

    pub fn load(&self, url: &str) {
        if let Err(error) = self.view.load_url(url) {
            tracing::warn!("browser: load {url}: {error}");
        }
    }

    pub fn script(&self, js: &str) {
        if let Err(error) = self.view.evaluate_script(js) {
            tracing::warn!("browser: script: {error}");
        }
    }

    /// Shows or hides the page; hiding gives the keyboard back to the window first.
    pub fn set_shown(&self, shown: bool) {
        if self.shown.get() == shown {
            return;
        }
        if !shown {
            let _ = self.view.focus_parent();
        }
        if let Err(error) = self.view.set_visible(shown) {
            tracing::warn!("browser: set_visible({shown}): {error}");
        }
        self.shown.set(shown);
    }

    /// Follows Mkrew's theme (`prefers-color-scheme`); macOS follows the app appearance itself.
    pub fn set_dark(&self, dark: bool) {
        if self.dark.replace(dark) == dark {
            return;
        }
        #[cfg(windows)]
        {
            use wry::WebViewExtWindows as _;
            let _ = self.view.set_theme(theme(dark));
        }
    }

    pub fn focus(&self) {
        let _ = self.view.focus();
    }

    pub fn focus_parent(&self) {
        let _ = self.view.focus_parent();
    }

    /// Moves the page into `window` keeping its state (Windows). `false`: this OS cannot,
    /// rebuild it there (macOS needs `unsafe` for its `NSWindow`, which this crate denies).
    pub fn move_to(&self, window: &Window) -> bool {
        #[cfg(windows)]
        {
            use raw_window_handle::{HasWindowHandle, RawWindowHandle};
            use wry::WebViewExtWindows as _;
            if let Ok(RawWindowHandle::Win32(handle)) =
                HasWindowHandle::window_handle(window).map(|handle| handle.as_raw())
            {
                return self.view.reparent(handle.hwnd.get()).is_ok();
            }
            false
        }
        #[cfg(not(windows))]
        {
            let _ = window;
            false
        }
    }

    /// The element that keeps the page over its bounds; `show` false while an overlay is open.
    pub fn surface(&self, show: bool) -> WebSurface {
        WebSurface { view: Rc::clone(&self.view), shown: Rc::clone(&self.shown), show }
    }
}

impl Drop for NativeView {
    fn drop(&mut self) {
        self.set_shown(false);
    }
}

#[cfg(windows)]
fn theme(dark: bool) -> wry::Theme {
    if dark { wry::Theme::Dark } else { wry::Theme::Light }
}

/// Fills its parent; keeps the native page over its bounds (clipped to the content mask),
/// blocks GPUI's mouse under it, and gives the keyboard back to the window on a click outside.
pub struct WebSurface {
    view: Rc<wry::WebView>,
    shown: Rc<Cell<bool>>,
    show: bool,
}

impl IntoElement for WebSurface {
    type Element = Self;

    fn into_element(self) -> Self {
        self
    }
}

impl gpui::Element for WebSurface {
    type RequestLayoutState = ();
    type PrepaintState = Option<Hitbox>;

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, ()) {
        let style = Style { size: Size::full(), flex_grow: 1., flex_shrink: 1., ..Style::default() };
        (window.request_layout(style, [], cx), ())
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut (),
        window: &mut Window,
        _cx: &mut App,
    ) -> Option<Hitbox> {
        let visible = bounds.intersect(&window.content_mask().bounds);
        if !self.show || visible.is_empty() {
            hide(&self.view, &self.shown);
            return None;
        }
        let rect = Rect {
            position: LogicalPosition::new(f64::from(f32::from(visible.origin.x)), f64::from(f32::from(visible.origin.y))).into(),
            size: LogicalSize::new(f64::from(f32::from(visible.size.width)), f64::from(f32::from(visible.size.height))).into(),
        };
        if let Err(error) = self.view.set_bounds(rect) {
            tracing::warn!("browser: set_bounds: {error}");
            hide(&self.view, &self.shown);
            return None;
        }
        if !self.shown.get() {
            let _ = self.view.set_visible(true);
            self.shown.set(true);
        }
        Some(window.insert_hitbox(visible, HitboxBehavior::BlockMouse))
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        _bounds: Bounds<Pixels>,
        _request_layout: &mut (),
        hitbox: &mut Option<Hitbox>,
        window: &mut Window,
        _cx: &mut App,
    ) {
        let Some(bounds) = hitbox.as_ref().map(|hitbox| hitbox.bounds) else {
            return;
        };
        let view = Rc::clone(&self.view);
        window.on_mouse_event(move |event: &MouseDownEvent, _, _, _| {
            if !bounds.contains(&event.position) {
                let _ = view.focus_parent();
            }
        });
    }
}

fn hide(view: &wry::WebView, shown: &Cell<bool>) {
    if shown.replace(false) {
        let _ = view.focus_parent();
        let _ = view.set_visible(false);
    }
}
```

If `gpui::Empty` is not exported by gpui 0.3.7, use any unit element type the crate exports for the stand-in's `IntoElement`; its body is unreachable.
`raw-window-handle` is already a Windows dependency of `mkrew-ui`.
The `wry` callbacks are `Fn + 'static` and run on the UI thread (Windows dispatches them through the parent window's message loop), so `try_send` on the unbounded channel never blocks.

- [ ] **Step 4: Browser state and the event task** (`shell/browser.rs`, `main.rs`, `shell/connection.rs`)

Grow `Browser` and its creation in `sync_browsers`:

```rust
pub(crate) struct Browser {
    pub(crate) page: Page,
    /// The page's view (Windows and macOS), built the first time the pane is drawn with a URL.
    pub(crate) native: Option<NativeView>,
    /// The window `native` is a child of.
    pub(crate) host: Option<HostId>,
    /// Why `native` could not be built (spec 5.3); the pane offers the system browser.
    pub(crate) error: Option<String>,
    /// The pane's keyboard (`BROWSER_CONTEXT`).
    pub(crate) focus: FocusHandle,
    pub(crate) address: Input,
    pub(crate) address_focus: FocusHandle,
    /// The page has the keyboard (its last IPC was a focus).
    pub(crate) page_keys: bool,
}
```

`sync_browsers` takes `cx: &mut Context<'_, Self>` now and inserts:

```rust
            self.browsers.entry(id).or_insert_with(|| {
                let mut address = Input::default();
                address.insert(&url);
                Browser {
                    page: Page::new(url),
                    native: None,
                    host: None,
                    error: None,
                    focus: cx.focus_handle(),
                    address,
                    address_focus: cx.focus_handle(),
                    page_keys: false,
                }
            });
```

(update the `sync_panes` call to `self.sync_browsers(cx);`).

`main.rs` field next to `browsers`:

```rust
    /// Page events of every browser pane (`wry` callbacks), read by one task (`page_event`).
    page_events: async_channel::Sender<(Ulid, mkrew_ui::browser::PageEvent)>,
```

In `Shell::new` (`shell/connection.rs`), before the struct literal:

```rust
        let (page_events, page_inbox) = async_channel::unbounded();
        cx.spawn_in(window, async move |this, cx| {
            while let Ok((id, event)) = page_inbox.recv().await {
                if this.update_in(cx, |shell, window, cx| shell.page_event(id, event, window, cx)).is_err() {
                    break;
                }
            }
        })
        .detach();
```

and `page_events,` in the literal.

- [ ] **Step 5: Page events and navigation** (`shell/browser.rs`)

```rust
    /// What a page reported (spec 6.1, 7.4).
    pub(crate) fn page_event(
        &mut self,
        id: Ulid,
        event: PageEvent,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let Some(browser) = self.browsers.get_mut(&id) else {
            return;
        };
        match event {
            PageEvent::Started => browser.page.loading = true,
            PageEvent::Title(title) => browser.page.title = title,
            PageEvent::Open(url) => {
                if let Some(native) = &browser.native {
                    native.load(&url);
                }
            }
            PageEvent::Finished(url) => {
                browser.page.loading = false;
                if !browser.editing {
                    browser.address.clear();
                    browser.address.insert(&url);
                }
                browser.page.url = url.clone();
                self.store_url(id, url, window, cx);
            }
            PageEvent::Focus => {
                browser.page_keys = true;
                if let Some(project) = self.web_project(id) {
                    self.focus_pane_key(project, &PaneKey::Web(id));
                }
            }
            PageEvent::Key(name) => {
                let command = gpui::Keystroke::parse(&name)
                    .ok()
                    .and_then(|keystroke| self.keymap.command_for(&keystroke));
                if let (Some(command), Some(host)) = (command, browser.host) {
                    self.in_window(host, cx, move |shell, window, cx| {
                        shell.run(&Run(command), window, cx);
                    });
                }
            }
        }
        cx.notify();
    }

    /// Saves the page's URL in its leaf (one `tab.set_layout` per finished load).
    fn store_url(&mut self, id: Ulid, url: String, window: &mut Window, cx: &mut Context<'_, Self>) {
        let Some(project_id) = self.web_project(id) else {
            return;
        };
        let write = (self.tabs.get_mut(&project_id)).and_then(|tabs| {
            tabs.update_leaf(&PaneKey::Web(id), |leaf| {
                if let Leaf::Web { web, .. } = leaf {
                    *web = url;
                }
            })
        });
        if let Some(write) = write {
            self.save_layout(project_id, write, window, cx);
        }
    }

    /// Loads `url` in browser pane `id`: in its page, or (no page yet) as the leaf's URL, which
    /// builds the page on the next draw.
    pub(crate) fn navigate(&mut self, id: Ulid, url: String, window: &mut Window, cx: &mut Context<'_, Self>) {
        let Some(browser) = self.browsers.get_mut(&id) else {
            return;
        };
        browser.editing = false;
        browser.address.clear();
        browser.address.insert(&url);
        browser.page.url = url.clone();
        match &browser.native {
            Some(native) => native.load(&url),
            None => self.store_url(id, url, window, cx),
        }
        cx.notify();
    }
```

Add `pub(crate) editing: bool,` (the address field is being edited) to `Browser` and `editing: false` to its creation.

- [ ] **Step 6: Build, show, hide, move** (`shell/browser.rs`)

```rust
    /// An overlay above the panes is open: every page hides so it shows (spec 7.2).
    /// ponytail: one rule for every window; a palette in one window hides pages in all.
    pub(crate) fn browser_overlay(&self, cx: &App) -> bool {
        self.modal_open()
            || self.menu_open()
            || self.peek.is_some()
            || self.drop_hover.is_some()
            || cx.has_active_drag()
    }

    /// Runs at the start of `host`'s render: builds the pages of this window's shown web leaves,
    /// moves the ones that changed window, and hides every page of this window not on screen.
    pub(crate) fn prepare_browsers(&mut self, host: HostId, window: &mut Window, cx: &mut Context<'_, Self>) {
        if self.browsers.is_empty() {
            return;
        }
        let shown: Vec<Ulid> = self.shown_browsers(host, window);
        let focused = self.focused_key(window);
        let dark = theme::slot_is_dark(cx.theme().mode, theme::os_dark(window));
        let keys = mkrew_ui::browser::keys::forwarded(&self.keymap);
        let data_dir = self.paths.data_dir.join("WebView2");
        let usable = mkrew_ui::browser::available().is_ok();
        for (id, browser) in &mut self.browsers {
            let here = shown.contains(id);
            if here && browser.host != Some(host) {
                let moved = (browser.native.as_ref()).is_some_and(|native| native.move_to(window));
                if !moved {
                    browser.native = None;
                }
                browser.host = Some(host);
            }
            if here && usable && browser.native.is_none() && browser.error.is_none() && !browser.page.url.is_empty() {
                match NativeView::build(window, *id, &browser.page.url, &keys, dark, Some(&data_dir), self.page_events.clone()) {
                    Ok(native) => browser.native = Some(native),
                    Err(error) => {
                        tracing::warn!(%id, "browser: cannot build the page: {error}");
                        browser.error = Some(error);
                    }
                }
            }
            let Some(native) = &browser.native else { continue };
            if browser.host == Some(host) && !here {
                native.set_shown(false);
            }
            if here {
                native.set_dark(dark);
                // The keyboard left the page's pane: give it back to the window.
                if browser.page_keys && focused != Some(PaneKey::Web(*id)) {
                    native.focus_parent();
                    browser.page_keys = false;
                }
            }
        }
    }

    /// The web leaves `host` shows now: its tab's shown panes (a zoom shows one), none while
    /// the main window shows another view.
    fn shown_browsers(&self, host: HostId, window: &Window) -> Vec<Ulid> {
        let Some((project, tab_id)) = self.tab_scope(window) else {
            return Vec::new();
        };
        debug_assert_eq!(self.host_of(window), host);
        (self.tabs.get(&project).and_then(|tabs| tabs.tab(tab_id)))
            .map(|tab| {
                tab.shown()
                    .into_iter()
                    .filter_map(|key| match key {
                        PaneKey::Web(id) => Some(id),
                        _ => None,
                    })
                    .collect()
            })
            .unwrap_or_default()
    }
```

Call it at the start of both renders:
- `main.rs` `impl Render for Shell`, right after `self.ensure_diff_views();`: `self.prepare_browsers(HostId::Main, window, cx);`
- `shell/detached_render.rs` `render_host`, right after `self.ensure_diff_views();`: `self.prepare_browsers(HostId::Detached(key), window, cx);`

A tab hidden by a tab switch, a view switch or a zoom is not in `shown`, so its page hides; a closed pane's `Browser` is dropped by `sync_browsers` and `NativeView::drop` hides it.

- [ ] **Step 7: The pane body draws the page** (`shell/browser.rs` `render_web_pane`)

```rust
    pub(crate) fn render_web_pane(
        &self,
        _project_id: Ulid,
        id: Ulid,
        _is_focused: bool,
        theme: &Theme,
        _window: &Window,
        cx: &mut Context<'_, Self>,
    ) -> gpui::AnyElement {
        let Some(browser) = self.browsers.get(&id) else {
            return message(theme, None, "Loading…".into(), None).size_full().into_any_element();
        };
        let body = match (&browser.native, &browser.error) {
            (Some(native), _) if !self.browser_overlay(cx) => native.surface(true).into_any_element(),
            (Some(native), _) => div()
                .size_full()
                .child(native.surface(false))
                .child(self.page_card(browser, theme))
                .into_any_element(),
            (None, Some(error)) => self.unavailable_body(id, error, theme, cx),
            (None, None) => match mkrew_ui::browser::available() {
                Err(reason) => self.unavailable_body(id, &reason.to_string(), theme, cx),
                Ok(()) => empty_state(theme, None, None, "Type an address or paste a link".into(), None)
                    .size_full()
                    .into_any_element(),
            },
        };
        div().size_full().flex().flex_col().bg(rgb(theme.surface)).child(body).into_any_element()
    }

    /// Where the page is while an overlay hides it: its title and URL.
    fn page_card(&self, browser: &Browser, theme: &Theme) -> gpui::Div {
        empty_state(theme, None, None, browser.page.label().to_owned().into(), Some(browser.page.url.clone().into()))
            .absolute()
            .inset_0()
    }

    /// The page cannot be shown here (spec 5.3): the reason and the system browser.
    fn unavailable_body(&self, id: Ulid, reason: &str, theme: &Theme, cx: &mut Context<'_, Self>) -> gpui::AnyElement {
        div()
            .size_full()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap(px(space::M))
            .child(message(theme, None, reason.to_owned().into(), None))
            .child(
                button("web-open-external", "Open in system browser", theme, false)
                    .on_click(cx.listener(move |shell, _, _, cx| shell.open_external(id, cx))),
            )
            .into_any_element()
    }

    /// The page in the system browser.
    pub(crate) fn open_external(&mut self, id: Ulid, cx: &mut Context<'_, Self>) {
        if let Some(url) = (self.browsers.get(&id)).map(|b| b.page.url.clone()).filter(|url| !url.is_empty()) {
            cx.open_url(&url);
        }
    }
```

If `empty_state`'s sentence parameter type does not accept a `String`, convert with `SharedString::from`.

`shell/pane_render.rs` `pane_frame`, next to the `EDITOR_CONTEXT` line (about 617-620):

```rust
            .when(matches!(key, PaneKey::Web(_)), |el| el.key_context(BROWSER_CONTEXT))
            .when_some(key_web_focus, |el, focus| el.track_focus(focus))
```

with, before `let frame = div()`:

```rust
        let key_web_focus = match &key {
            PaneKey::Web(id) => self.browsers.get(id).map(|browser| &browser.focus),
            _ => None,
        };
```

`shell/focus.rs` `focus_pane`, before the `focused_diff` branch:

```rust
        // A browser pane keeps the keyboard on its pane (the page takes it on a click).
        if let Some(PaneKey::Web(id)) = self.focused_key(window)
            && let Some(browser) = self.browsers.get(&id)
        {
            window.focus(&browser.focus, cx);
            return;
        }
```

- [ ] **Step 8: Keymap changes reach live pages** (`shell/workspace_actions.rs` `set_keymap`, after `self.keymap = keymap;`)

```rust
        let keys = mkrew_ui::browser::keys::forwarded(&self.keymap);
        let script = mkrew_ui::browser::keys::set_keys_script(&keys);
        for native in self.browsers.values().filter_map(|b| b.native.as_ref()) {
            native.script(&script);
        }
```

- [ ] **Step 9: A test for the leaf bookkeeping** (`tabs/tests.rs`; `sync_browsers` follows `Tab::keys`, so the guarantee lives there)

```rust
#[test]
fn closing_a_browser_pane_or_its_tab_drops_its_key() {
    let project = Ulid::new();
    let (a, web) = (Ulid::new(), Ulid::new());
    let tree = json!({"split": "vertical", "ratio": 0.5, "children": [
        {"session": a.to_string()}, {"web": "https://x.dev", "id": web.to_string()}
    ]});
    let mut tabs = Tabs::new(project);
    tabs.load(vec![wire(project, "Terminal", 1.0, tree)]);
    assert!(tabs.tab_with(&PaneKey::Web(web)).is_some());
    assert!(tabs.close_leaf(&PaneKey::Web(web)).is_some());
    assert!(tabs.tab_with(&PaneKey::Web(web)).is_none(), "the pane left every layout");
    assert_eq!(tabs.active().unwrap().keys(), [k(a)]);
}
```

- [ ] **Step 10: Build, lint, smoke the page**

Run: `cargo fmt --all`, `cargo clippy -p mkrew-ui --all-targets -j 4 -- -D warnings`, `cargo test -p mkrew-ui --lib -j 4 tabs:: browser::`.
No smoke here: nothing in the UI makes a browser pane before Task 6; the page is smoked in Task 6 Step 6.
Expected: build and tests PASS.

- [ ] **Step 11: Commit**

```bash
git add crates/mkrew-ui/Cargo.toml Cargo.lock crates/mkrew-ui/src/browser crates/mkrew-ui/src/shell/browser.rs crates/mkrew-ui/src/main.rs crates/mkrew-ui/src/shell/connection.rs crates/mkrew-ui/src/shell/detached_render.rs crates/mkrew-ui/src/shell/workspace_actions.rs crates/mkrew-ui/src/shell/pane_render.rs crates/mkrew-ui/src/shell/focus.rs crates/mkrew-ui/src/tabs/tests.rs
git -c user.email=tungnv975@gmail.com commit -m "feat(ui): browser pane page (wry child view, events, visibility, overlays, window moves)"
```

---

### Task 4: Toolbar, address field, states, browser commands

**Files:**
- Create: `icon/lucide/globe.svg`, `icon/lucide/arrow-left.svg`, `icon/lucide/arrow-right.svg` (unless Task 1 added `globe`)
- Modify: `crates/mkrew-ui/src/icons.rs`, `crates/mkrew-ui/src/keymap.rs`, `crates/mkrew-ui/src/palette.rs`, `crates/mkrew-ui/src/palette/tests.rs`, `crates/mkrew-ui/src/shell/browser.rs`, `crates/mkrew-ui/src/shell/sessions.rs` (`run` arms), `crates/mkrew-ui/src/shell/detached_actions.rs` (`run_detached` arms), `crates/mkrew-ui/src/detached.rs` (`handles`), `crates/mkrew-ui/src/shell/navigation.rs` (palette context `web_pane`)

**Interfaces:**
- Consumes: Task 3 `Browser`, `navigate`, `open_external`, `NativeView::{load, script, focus}`.
- Produces: `Command::{BrowserAddress, BrowserBack, BrowserForward, BrowserReload, BrowserOpenExternal}` (Task 6 adds `SplitBrowser`); `palette::Context.web_pane: bool`; `Shell::focused_web(&Window) -> Option<Ulid>`; `Shell::focus_address(Ulid, &mut Window, &mut Context<Shell>)`; `Shell::browser_command(Command, &mut Window, &mut Context<Shell>)`.

- [ ] **Step 1: Icons** (`icons.rs`: `ArrowLeft => "arrow-left",` and `ArrowRight => "arrow-right",` in alphabetical place near `ArrowDown`; `Globe => "globe",` between `GitBranch` and `History`)

`icon/lucide/globe.svg`:

```svg
<!-- @license lucide-static v1.55.0 - ISC -->
<svg
  class="lucide lucide-globe"
  xmlns="http://www.w3.org/2000/svg"
  width="24"
  height="24"
  viewBox="0 0 24 24"
  fill="none"
  stroke="currentColor"
  stroke-width="2"
  stroke-linecap="round"
  stroke-linejoin="round"
>
  <circle cx="12" cy="12" r="10" />
  <path d="M12 2a14.5 14.5 0 0 0 0 20 14.5 14.5 0 0 0 0-20" />
  <path d="M2 12h20" />
</svg>
```

`icon/lucide/arrow-left.svg`: same header with `class="lucide lucide-arrow-left"` and the body `<path d="m12 19-7-7 7-7" />` `<path d="M19 12H5" />`.
`icon/lucide/arrow-right.svg`: `class="lucide lucide-arrow-right"`, body `<path d="M5 12h14" />` `<path d="m12 5 7 7-7 7" />`.

Run: `cargo test -p mkrew-ui --lib icons:: -j 4` - Expected: PASS (`every_icon_is_served_and_is_a_lucide_svg`).

- [ ] **Step 2: Failing keymap tests** (`keymap.rs` tests)

```rust
    #[test]
    fn browser_pane_keys() {
        let keymap = Keymap::defaults_for(Os::Windows);
        assert_eq!(keymap.command_for(&keystroke("ctrl-l")), Some(Command::BrowserAddress));
        assert_eq!(Command::BrowserAddress.context(), Some(BROWSER_CONTEXT));
        assert_eq!(Command::BrowserAddress.default_keys_for(Os::Mac), "cmd-l");
        for command in [
            Command::BrowserBack,
            Command::BrowserForward,
            Command::BrowserReload,
            Command::BrowserOpenExternal,
        ] {
            assert_eq!(command.context(), Some(BROWSER_CONTEXT), "{command:?}");
            assert_eq!(keymap.label(command), "", "{command:?} is palette only");
        }
        assert_eq!(Command::BrowserOpenExternal.name(), "browser_open_external");
    }
```

- [ ] **Step 3: The commands** (`keymap.rs`)

Enum, after `SplitShell`:

```rust
    /// Focus the browser pane's address field (ADR 0110).
    BrowserAddress,
    BrowserBack,
    BrowserForward,
    BrowserReload,
    /// The browser pane's page in the system browser.
    BrowserOpenExternal,
```

`all()`: add the five after `Command::SplitShell,`. `name()`: `Command::BrowserAddress => "browser_address".into(),`, `Command::BrowserBack => "browser_back".into(),`, `Command::BrowserForward => "browser_forward".into(),`, `Command::BrowserReload => "browser_reload".into(),`, `Command::BrowserOpenExternal => "browser_open_external".into(),`. `mac_keys`: `Command::BrowserAddress => "cmd-l",`. `pc_keys`: `Command::BrowserAddress => "ctrl-l".into(),`; the other four return "no key" the way other palette-only commands do in `pc_keys` / `mac_keys` (find one, e.g. a command without a default, and copy its arm shape). `context()`: add

```rust
            Command::BrowserAddress
            | Command::BrowserBack
            | Command::BrowserForward
            | Command::BrowserReload
            | Command::BrowserOpenExternal => Some(BROWSER_CONTEXT),
```

`shares_context`: a browser pane and a terminal or file pane never have the keyboard at once:

```rust
            (Some(TERMINAL_CONTEXT), Some(EDITOR_CONTEXT))
                | (Some(EDITOR_CONTEXT), Some(TERMINAL_CONTEXT))
                | (Some(BROWSER_CONTEXT), Some(TERMINAL_CONTEXT | EDITOR_CONTEXT))
                | (Some(TERMINAL_CONTEXT | EDITOR_CONTEXT), Some(BROWSER_CONTEXT))
```

The existing test near line 1745 ("plain Ctrl+letter only in EDITOR_CONTEXT") will fail on `ctrl-l`: extend its allowed contexts to `EDITOR_CONTEXT | BROWSER_CONTEXT` and say why in its comment ("a browser pane's address key, like a browser's").

Run: `cargo test -p mkrew-ui --lib keymap:: -j 4` - Expected: PASS.

- [ ] **Step 4: Palette** (`palette.rs`)

`Context` gets:

```rust
    /// A browser pane is focused: its commands act on it.
    pub web_pane: bool,
```

`title()`:

```rust
        Command::BrowserAddress => "Browser: Focus address".into(),
        Command::BrowserBack => "Browser: Back".into(),
        Command::BrowserForward => "Browser: Forward".into(),
        Command::BrowserReload => "Browser: Reload".into(),
        Command::BrowserOpenExternal => "Browser: Open in system browser".into(),
```

`available()`:

```rust
        Command::BrowserAddress
        | Command::BrowserBack
        | Command::BrowserForward
        | Command::BrowserReload
        | Command::BrowserOpenExternal => context.web_pane,
```

Every `palette::Context { .. }` literal (in `shell/navigation.rs` about line 335, and the test helpers in `palette/tests.rs`) gets `web_pane`; in navigation.rs: `web_pane: self.focused_web(window).is_some(),`.

Test (`palette/tests.rs`, using the file's context helper; if it is named differently, use it):

```rust
#[test]
fn browser_commands_need_a_focused_browser_pane() {
    let mut context = terminals_context();
    assert!(!commands(&context).contains(&Command::BrowserReload));
    context.web_pane = true;
    for command in [Command::BrowserAddress, Command::BrowserBack, Command::BrowserReload, Command::BrowserOpenExternal] {
        assert!(commands(&context).contains(&command), "{command:?}");
    }
}
```

`detached.rs` `handles`: add the five browser commands (a browser pane works in a detached window); update `detached/tests.rs` list the same way.

- [ ] **Step 5: Commands run** (`shell/browser.rs`, `shell/sessions.rs`, `shell/detached_actions.rs`)

```rust
    /// The focused pane of `window` if it is a browser pane.
    pub(crate) fn focused_web(&self, window: &Window) -> Option<Ulid> {
        match self.focused_key(window)? {
            PaneKey::Web(id) => Some(id),
            _ => None,
        }
    }

    pub(crate) fn focus_address(&mut self, id: Ulid, window: &mut Window, cx: &mut Context<'_, Self>) {
        let Some(browser) = self.browsers.get_mut(&id) else {
            return;
        };
        if let Some(native) = &browser.native {
            native.focus_parent();
        }
        browser.page_keys = false;
        browser.editing = true;
        browser.address.select_all();
        window.focus(&browser.address_focus, cx);
        cx.notify();
    }

    /// The browser pane commands, on the focused browser pane of `window`.
    pub(crate) fn browser_command(&mut self, command: Command, window: &mut Window, cx: &mut Context<'_, Self>) {
        let Some(id) = self.focused_web(window) else {
            return;
        };
        let script = match command {
            Command::BrowserAddress => return self.focus_address(id, window, cx),
            Command::BrowserOpenExternal => return self.open_external(id, cx),
            Command::BrowserBack => "history.back()",
            Command::BrowserForward => "history.forward()",
            Command::BrowserReload => "location.reload()",
            _ => return,
        };
        if let Some(native) = self.browsers.get(&id).and_then(|b| b.native.as_ref()) {
            native.script(script);
        }
    }
```

In `Shell::run` (`shell/sessions.rs`), next to the `SplitShell` arm:

```rust
            Command::BrowserAddress
            | Command::BrowserBack
            | Command::BrowserForward
            | Command::BrowserReload
            | Command::BrowserOpenExternal => self.browser_command(*command, window, cx),
```

and the same arm in `run_detached` (`shell/detached_actions.rs`). In the view guard of `run` (lines 333-358) add the five to the "act on panes that are not on screen" list (they `return` outside Terminals).

- [ ] **Step 6: The toolbar** (`shell/browser.rs`; `render_web_pane` puts it above the body)

```rust
    /// Back, forward, reload / stop, the address field, open in the system browser, close
    /// (spec 5.2), and the loading line under it.
    fn web_toolbar(&self, id: Ulid, browser: &Browser, window: &Window, theme: &Theme, cx: &mut Context<'_, Self>) -> gpui::Div {
        let has_page = browser.native.is_some();
        let nav = |name: &'static str, glyph: Icon, tip: &'static str, command: Command| {
            let button = if has_page {
                icon_button(name, glyph, theme).on_click(cx.listener(move |shell, _, window, cx| {
                    shell.browser_command(command, window, cx);
                }))
            } else {
                disabled_icon_button(name, glyph, theme)
            };
            button.tooltip(tooltip(theme, tip, self.keymap.label(command)))
        };
        let reload = if browser.page.loading {
            nav("web-stop", Icon::X, "Stop", Command::BrowserReload)
        } else {
            nav("web-reload", Icon::RotateCw, "Reload", Command::BrowserReload)
        };
        let focused = browser.address_focus.is_focused(window);
        let field = caret_field(
            "web-address",
            &browser.address,
            "Type an address or paste a link",
            focused,
            theme,
            cx,
            move |shell, caret| {
                if let Some(browser) = shell.browsers.get_mut(&id) {
                    browser.address.click(caret.at, caret.extend, caret.clicks);
                }
            },
        )
        .flex_1()
        .min_w_0()
        .h(theme::rems_px(theme::INPUT_HEIGHT))
        .track_focus(&browser.address_focus)
        .on_key_down(cx.listener(move |shell, event: &KeyDownEvent, window, cx| {
            shell.address_key_down(id, event, window, cx);
        }))
        .relative()
        .child(self.typing(&browser.address_focus, move |shell: &mut Shell, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<'_, Shell>| {
            shell.address_key_down(id, event, window, cx);
        }, cx))
        .on_click(cx.listener(move |shell, _, window, cx| shell.focus_address(id, window, cx)));
        let row = div()
            .flex_none()
            .flex()
            .items_center()
            .gap(px(space::S))
            .px(px(space::L))
            .py(px(space::S))
            .child(nav("web-back", Icon::ArrowLeft, "Back", Command::BrowserBack))
            .child(nav("web-forward", Icon::ArrowRight, "Forward", Command::BrowserForward))
            .child(reload)
            .child(field)
            .child(
                icon_button("web-external", Icon::ExternalLink, theme)
                    .tooltip(tooltip(theme, "Open in system browser", self.keymap.label(Command::BrowserOpenExternal)))
                    .on_click(cx.listener(move |shell, _, _, cx| shell.open_external(id, cx))),
            )
            .child(
                icon_button("web-close", Icon::X, theme)
                    .tooltip(tooltip(theme, "Close", self.keymap.label(Command::ClosePane)))
                    .on_click(cx.listener(move |shell, _, window, cx| shell.close_web_pane(id, window, cx))),
            );
        let line = div()
            .flex_none()
            .h(px(theme::PANE_ACCENT_WIDTH))
            .when(browser.page.loading, |el| el.bg(rgb(theme.accent)));
        div().flex_none().flex().flex_col().child(row).child(line)
    }

    /// Typing in the address field: edits, Enter loads, Esc restores and gives the page the keys.
    fn address_key_down(&mut self, id: Ulid, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<'_, Self>) {
        cx.stop_propagation();
        let keystroke = &event.keystroke;
        if let Some(edit) = field_edit(keystroke, cx) {
            if let Some(browser) = self.browsers.get_mut(&id) {
                browser.editing = true;
                edit.apply(&mut browser.address, cx);
            }
            cx.notify();
            return;
        }
        if keystroke.modifiers.modified() {
            return;
        }
        match keystroke.key.as_str() {
            "enter" => {
                let typed = self.browsers.get(&id).map(|b| b.address.as_str().to_owned()).unwrap_or_default();
                if let Some(url) = mkrew_ui::browser::address::normalize(&typed) {
                    self.navigate(id, url, window, cx);
                    if let Some(native) = self.browsers.get(&id).and_then(|b| b.native.as_ref()) {
                        native.focus();
                    }
                }
            }
            "escape" => {
                if let Some(browser) = self.browsers.get_mut(&id) {
                    browser.editing = false;
                    let url = browser.page.url.clone();
                    browser.address.clear();
                    browser.address.insert(&url);
                    match &browser.native {
                        Some(native) => native.focus(),
                        None => window.focus(&browser.focus, cx),
                    }
                }
                cx.notify();
            }
            _ => {}
        }
    }
```

`self.typing(handle, handler, cx)` takes the handler the way `file_find_key_down` is passed in `file_pane.rs`; if its parameter is a method path (`Self::xxx_key_down`) rather than a closure, add one method per field instead: store the edited pane id in a Shell field `address_editing: Option<Ulid>` set by `focus_address`, and write `fn web_address_key_down(&mut self, event, window, cx)` that reads it and calls `address_key_down`. Check `typing`'s signature in `shell/focus.rs:348` first and pick the shape it accepts.

`render_web_pane` becomes:

```rust
        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(rgb(theme.surface))
            .child(self.web_toolbar(id, browser, window, theme, cx))
            .child(div().flex_1().min_h_0().relative().child(body))
            .into_any_element()
```

(`render_web_pane` gets `window: &Window` from its caller already.)
An empty leaf focuses its address field when it is created: Task 6's creators call `focus_address`.

- [ ] **Step 7: Lint and tests**

Run: `cargo fmt --all`, `cargo clippy -p mkrew-ui --all-targets -j 4 -- -D warnings`, `cargo test -p mkrew-ui --lib -j 4 keymap:: palette:: detached:: icons:: browser::`.
Expected: PASS.

- [ ] **Step 8: No smoke yet**

Nothing in the UI creates a browser pane before Task 6 (`+` item, `Alt+Shift+B`); the toolbar, address field and commands are smoked in Task 6 Step 6, items 2 to 5.
Do not add a throwaway hook to get there earlier.

- [ ] **Step 9: Commit**

```bash
git add icon/lucide/globe.svg icon/lucide/arrow-left.svg icon/lucide/arrow-right.svg crates/mkrew-ui/src/icons.rs crates/mkrew-ui/src/keymap.rs crates/mkrew-ui/src/palette.rs crates/mkrew-ui/src/palette/tests.rs crates/mkrew-ui/src/detached.rs crates/mkrew-ui/src/detached/tests.rs crates/mkrew-ui/src/shell/browser.rs crates/mkrew-ui/src/shell/sessions.rs crates/mkrew-ui/src/shell/detached_actions.rs crates/mkrew-ui/src/shell/navigation.rs
git -c user.email=tungnv975@gmail.com commit -m "feat(ui): browser pane toolbar, address field and commands"
```

---

### Task 5: Links open in the browser pane

**Files:**
- Modify: `crates/mkrew-ui/src/tabs.rs` (`WebPlace`, `web_place`), `crates/mkrew-ui/src/tabs/tests.rs`
- Modify: `crates/mkrew-ui/src/preview.rs` (`OpenLink.session`), `crates/mkrew-ui/src/markdown.rs` (doc of `LinkTarget::Web`)
- Modify: `crates/mkrew-ui/src/shell/browser.rs` (`open_web`), `shell/files_split.rs` (`open_preview_link`), `shell/navigation.rs` (`open_terminal_link`)

**Interfaces:**
- Consumes: Task 1 `Leaf::web`, `PaneKey::Web`; Task 3 `navigate`; Task 4 `focus_address`.
- Produces: `tabs::WebPlace { Load(Ulid), Split(PaneKey), NewTab }`, `tabs::web_place(tab: Option<&Tab>, from: Option<&PaneKey>) -> WebPlace`; `Shell::open_web(url: String, from: Option<PaneKey>, window, cx)`; `preview::OpenLink { dest, on_page, session: Option<Ulid> }`.

- [ ] **Step 1: Failing tests** (`tabs/tests.rs`)

```rust
#[test]
fn a_link_loads_in_the_tabs_browser_pane_or_splits_beside_its_pane() {
    let project = Ulid::new();
    let (a, b, web) = (Ulid::new(), Ulid::new(), Ulid::new());
    let two = json!({"split": "vertical", "ratio": 0.5, "children": [pane(a), pane(b)]});
    let mut tabs = Tabs::new(project);
    tabs.load(vec![wire(project, "Terminal", 1.0, two)]);
    let tab = tabs.active().unwrap();
    // No browser pane: beside the pane the link is in, else beside the focused pane.
    assert_eq!(web_place(Some(tab), Some(&k(b))), WebPlace::Split(k(b)));
    assert_eq!(web_place(Some(tab), None), WebPlace::Split(tab.focused().unwrap().clone()));
    // A link from a pane of another tab splits the focused pane of this one.
    assert_eq!(web_place(Some(tab), Some(&k(Ulid::new()))), WebPlace::Split(tab.focused().unwrap().clone()));
    // No tab shown (Docs): a new tab.
    assert_eq!(web_place(None, Some(&k(a))), WebPlace::NewTab);
    // The tab has a browser pane: it takes the link, even while another pane is zoomed.
    let with_web = json!({"split": "vertical", "ratio": 0.5, "children": [
        pane(a), {"web": "https://x.dev", "id": web.to_string()}
    ]});
    let mut tabs = Tabs::new(project);
    tabs.load(vec![wire(project, "Terminal", 1.0, with_web)]);
    assert!(tabs.focus(&k(a)));
    assert!(tabs.toggle_zoom());
    assert_eq!(web_place(tabs.active(), Some(&k(a))), WebPlace::Load(web));
}

#[test]
fn the_first_browser_pane_in_tree_order_takes_the_link() {
    let project = Ulid::new();
    let (first, second) = (Ulid::new(), Ulid::new());
    let tree = json!({"split": "horizontal", "ratio": 0.5, "children": [
        {"web": "", "id": first.to_string()}, {"web": "", "id": second.to_string()}
    ]});
    let mut tabs = Tabs::new(project);
    tabs.load(vec![wire(project, "Terminal", 1.0, tree)]);
    assert_eq!(web_place(tabs.active(), None), WebPlace::Load(first));
}
```

- [ ] **Step 2: Implement** (`tabs.rs`)

```rust
/// Where a web link clicked in a tab goes (spec 4.1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WebPlace {
    /// The tab's browser pane (the first in tree order) loads it.
    Load(Ulid),
    /// A new browser pane right of this pane.
    Split(PaneKey),
    /// No tab is shown: a new tab.
    NewTab,
}

/// Where a link clicked in `tab` (`None`: no tab shown) goes; `from` is the pane it was in.
pub fn web_place(tab: Option<&Tab>, from: Option<&PaneKey>) -> WebPlace {
    let Some(tab) = tab else {
        return WebPlace::NewTab;
    };
    let keys = tab.keys();
    if let Some(id) = keys.iter().find_map(|key| match key {
        PaneKey::Web(id) => Some(*id),
        _ => None,
    }) {
        return WebPlace::Load(id);
    }
    match from.filter(|key| keys.contains(key)).or(tab.focused()) {
        Some(key) => WebPlace::Split(key.clone()),
        None => WebPlace::NewTab,
    }
}
```

Run: `cargo test -p mkrew-ui --lib tabs:: -j 4` - Expected: PASS.

- [ ] **Step 3: Chat links carry their session** (`preview.rs`)

```rust
pub struct OpenLink {
    pub dest: String,
    pub on_page: bool,
    /// The Chat pane the link is in (ADR 0110: a web link splits beside it).
    pub session: Option<Ulid>,
}
```

In `links()`'s dispatch (about line 391): `_ => window.dispatch_action(Box::new(OpenLink { dest, on_page, session: path_of }), cx),`. Fix every other `OpenLink { .. }` literal the compiler names with `session: None`.

`markdown.rs` `LinkTarget::Web` doc: `/// \`http\`, \`https\` (a browser pane, ADR 0110) or \`mailto\` (the system).`

- [ ] **Step 4: `open_web`** (`shell/browser.rs`)

```rust
    /// A web link clicked in `window` (spec 4.1): `http(s)` loads in the shown tab's browser
    /// pane, or a new one right of `from` (else the focused pane), or a new tab; anything else,
    /// and every link where pages cannot show, goes to the system.
    pub(crate) fn open_web(&mut self, url: String, from: Option<PaneKey>, window: &mut Window, cx: &mut Context<'_, Self>) {
        let web = url.get(..url.find(':').unwrap_or(0)).is_some_and(|scheme| {
            scheme.eq_ignore_ascii_case("http") || scheme.eq_ignore_ascii_case("https")
        });
        if !web {
            return cx.open_url(&url);
        }
        if let Err(reason) = mkrew_ui::browser::available() {
            if !self.browser_fallback_told {
                self.browser_fallback_told = true;
                self.toast_info(format!("Web pages open in your browser: {reason}"), cx);
            }
            return cx.open_url(&url);
        }
        let scope = self.tab_scope(window);
        let Some(project_id) = scope.map(|(project, _)| project).or(self.workspace) else {
            return cx.open_url(&url);
        };
        let tab = scope.and_then(|(project, tab_id)| self.tabs.get(&project)?.tab(tab_id));
        match web_place(tab, from.as_ref()) {
            WebPlace::Load(id) => {
                self.focus_pane_key(project_id, &PaneKey::Web(id));
                self.navigate(id, url, window, cx);
                self.sync_panes(window, cx);
            }
            WebPlace::Split(target) => {
                let place = documents::Place::Split(target, Edge::Right);
                self.place_leaf(project_id, Leaf::web(url), place, window, cx);
            }
            WebPlace::NewTab => {
                // Terminals show the panes: Board and Docs give way (as a file from Docs).
                self.views.insert(project_id, View::Terminals);
                if self.workspace != Some(project_id) {
                    self.activate_workspace(project_id, window, cx);
                }
                self.place_leaf(project_id, Leaf::web(url), documents::Place::NewTab, window, cx);
            }
        }
        cx.notify();
    }
```

Add `browser_fallback_told: bool` to `Shell` (`main.rs`, doc "The 'pages open in your browser' toast was shown this run (ADR 0110).") and `browser_fallback_told: false,` in `Shell::new`.
`focus_pane_key` ends a zoom on another pane (`Tab::fix_focus`).

- [ ] **Step 5: The two entry points**

`shell/files_split.rs` `open_preview_link`: `Some(LinkTarget::Web(url)) => self.open_web(url, action.session.map(PaneKey::Session), window, cx),`
`shell/navigation.rs` `open_terminal_link`: `OpenLink::Url(url) => self.open_web(url.clone(), Some(PaneKey::Session(session)), window, cx),` and its doc: "a URL opens in a browser pane (ADR 0110), a file as a file tab."

- [ ] **Step 6: Lint, test, smoke**

Run: `cargo fmt --all`, `cargo clippy -p mkrew-ui --all-targets -j 4 -- -D warnings`, `cargo test -p mkrew-ui --lib -j 4 tabs:: preview:: markdown::`.
Smoke (`scripts/smoke.ps1`): start a fake agent in the sandbox whose Chat shows a markdown link and whose terminal prints a URL (`mkrew-fakeagent` steps `output`); click the Chat link -> browser pane right of that pane; click a second link -> same pane loads it; Ctrl+Click the terminal URL -> same pane; a `mailto:` link opens the mail app.
Expected: as described; capture `shot` for Task 7.

- [ ] **Step 7: Commit**

```bash
git add crates/mkrew-ui/src/tabs.rs crates/mkrew-ui/src/tabs/tests.rs crates/mkrew-ui/src/preview.rs crates/mkrew-ui/src/markdown.rs crates/mkrew-ui/src/shell/browser.rs crates/mkrew-ui/src/shell/files_split.rs crates/mkrew-ui/src/shell/navigation.rs crates/mkrew-ui/src/main.rs crates/mkrew-ui/src/shell/connection.rs
git -c user.email=tungnv975@gmail.com commit -m "feat(ui): web links open in a browser pane beside the active pane"
```

---

### Task 6: `+` row Browser, `SplitBrowser`, `▾` row

**Files:**
- Modify: `crates/mkrew-ui/src/plus_row.rs` (+ tests), `crates/mkrew-ui/src/shell/plus_row.rs`, `crates/mkrew-ui/src/shell/chrome.rs` (`new_menu_rows`), `crates/mkrew-ui/src/keymap.rs` (+ tests), `crates/mkrew-ui/src/palette.rs`, `crates/mkrew-ui/src/palette/tests.rs`, `crates/mkrew-ui/src/shell/sessions.rs`, `crates/mkrew-ui/src/shell/detached_actions.rs`, `crates/mkrew-ui/src/detached.rs`, `crates/mkrew-ui/src/detached/tests.rs`, `crates/mkrew-ui/src/shell/browser.rs` (`new_browser`)

**Interfaces:**
- Consumes: Task 1 `Leaf::web`; Task 4 `focus_address`; `browser::available`.
- Produces: `NewKind::Browser`, `RowItem::Browser`, `plus_row_items(default_agent, shell_name, main, browser: bool)`, `Command::SplitBrowser`, `palette::Context.browser: bool`, `Shell::new_browser(at: Option<(Ulid, PaneKey, Edge)>, window, cx)`.

- [ ] **Step 1: Failing row tests** (`plus_row.rs`; update every existing call to `plus_row_items(.., true)` with a fourth argument `false` and keep their expectations)

```rust
    #[test]
    fn the_browser_item_comes_after_note_when_pages_can_show() {
        let items = plus_row_items(None, None, true, true);
        assert_eq!(
            items,
            [RowItem::Shell { name: None }, RowItem::Note, RowItem::Browser, RowItem::More]
        );
        assert!(!plus_row_items(None, None, true, false).contains(&RowItem::Browser));
        assert_eq!(RowItem::Browser.kind(), Some(NewKind::Browser));
        assert_eq!(RowItem::Browser.label(true), "New browser");
        assert_eq!(RowItem::Browser.command(true), None, "a tab has no key");
        assert_eq!(RowItem::Browser.command(false), Some(Command::SplitBrowser));
        // A detached window splits.
        assert!(plus_row_items(None, None, false, true).contains(&RowItem::Browser));
    }
```

- [ ] **Step 2: Implement** (`plus_row.rs`)

`NewKind`: `/// A browser pane (ADR 0110).` `Browser,`. `RowItem`: `Browser,` before `More`. `kind()`: `RowItem::Browser => Some(NewKind::Browser),`. `label()`: `RowItem::Browser => "New browser".to_owned(),`. `command()`:

```rust
            (RowItem::Browser, false) => Some(Command::SplitBrowser),
            (RowItem::Note, false) | (RowItem::Browser, true) | (RowItem::More, _) => None,
```

`plus_row_items`:

```rust
/// The items after `+`, in order. `main`: the main window, which also has `▾` (a detached
/// window has Split ▾ in its title bar and no new-tab menu). `browser`: pages can show here.
pub fn plus_row_items(
    default_agent: Option<&PaneAgent>,
    shell_name: Option<&str>,
    main: bool,
    browser: bool,
) -> Vec<RowItem> {
    let mut items = Vec::with_capacity(5);
    // ... agent, shell, note as today ...
    if browser {
        items.push(RowItem::Browser);
    }
    if main {
        items.push(RowItem::More);
    }
    items
}
```

- [ ] **Step 3: `SplitBrowser`** (`keymap.rs`; test first)

```rust
    #[test]
    fn split_browser_has_its_own_global_keys() {
        let keymap = Keymap::defaults_for(Os::Windows);
        assert_eq!(keymap.command_for(&keystroke("alt-shift-b")), Some(Command::SplitBrowser));
        assert_eq!(keymap.label(Command::SplitBrowser), "Alt+Shift+B");
        assert_eq!(Command::SplitBrowser.context(), None);
        assert_eq!(Command::SplitBrowser.default_keys_for(Os::Mac), "cmd-alt-b");
        assert_eq!(Command::SplitBrowser.name(), "split_browser");
    }
```

Enum after `SplitShell`: `/// Split the focused pane to the right with a new browser pane (ADR 0110).` `SplitBrowser,`. `all()` after `Command::SplitShell,`. `name()`: `"split_browser"`. `mac_keys`: `Command::SplitBrowser => "cmd-alt-b",`. `pc_keys`: `Command::SplitBrowser => "alt-shift-b".into(),`. Context: global (falls in `_ => None`).

`palette.rs`: `title()` -> `Command::SplitBrowser => "Panes: New browser pane".into(),`; `Context` gets `/// Pages can show in Mkrew (\`browser::available\`).` `pub browser: bool,`; `available()`: `Command::SplitBrowser => context.browser && context.workspace && context.tabs > 0,`; `shell/navigation.rs` context literal: `browser: mkrew_ui::browser::available().is_ok(),`; palette test helpers get `browser: true`. Test:

```rust
#[test]
fn split_browser_is_listed_only_where_pages_show() {
    let mut context = terminals_context();
    context.browser = true;
    assert!(commands(&context).contains(&Command::SplitBrowser));
    context.browser = false;
    assert!(!commands(&context).contains(&Command::SplitBrowser));
}
```

`detached.rs` `handles`: add `Command::SplitBrowser` next to `SplitShell`; `detached/tests.rs` list likewise.
The view guard of `run` (`shell/sessions.rs` 333-358): add `| Command::SplitBrowser` to the "shows the terminals" list.
`run` arm and `run_detached` arm: `Command::SplitBrowser => self.split_browser(window, cx),`.

- [ ] **Step 4: Creating a browser pane** (`shell/browser.rs`)

```rust
    /// A new empty browser pane with its address field focused: on an edge of a pane
    /// (`at`), or in a new tab (`None`). New tabs belong to the main window.
    pub(crate) fn new_browser(
        &mut self,
        at: Option<(Ulid, PaneKey, Edge)>,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let Some(project_id) = (at.as_ref().map(|(project, ..)| *project))
            .or_else(|| self.tab_scope(window).map(|(project, _)| project))
            .or(self.workspace)
        else {
            return;
        };
        let leaf = Leaf::web("");
        let Leaf::Web { id, .. } = leaf else { return };
        let place = match at {
            Some((_, key, edge)) => documents::Place::Split(key, edge),
            None => {
                if self.host_of(window) != HostId::Main {
                    return self.in_window(HostId::Main, cx, move |shell, window, cx| {
                        window.activate_window();
                        shell.new_browser(None, window, cx);
                    });
                }
                self.views.insert(project_id, View::Terminals);
                if self.workspace != Some(project_id) {
                    self.activate_workspace(project_id, window, cx);
                }
                documents::Place::NewTab
            }
        };
        self.place_leaf(project_id, leaf, place, window, cx);
        // A new tab exists once the daemon answered: focus the field then.
        self.in_window(self.host_of(window), cx, move |shell, window, cx| {
            shell.sync_panes(window, cx);
            shell.focus_address(id, window, cx);
        });
    }

    /// `SplitBrowser`: a browser pane right of the focused pane.
    pub(crate) fn split_browser(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        let at = self.tab_scope(window).zip(self.focused_key(window)).map(|((project, _), key)| (project, key, Edge::Right));
        if at.is_some() {
            self.new_browser(at, window, cx);
        }
    }
```

If `focus_address` runs before the tab exists (`NewTab` waits for `tab.create`), `browsers` has no entry yet and it returns: then the pane shows its empty state with the field unfocused; Ctrl+L focuses it. Prefer calling `focus_address` from the `create_tab_then` continuation if `place_leaf` grows a callback; do not add one just for this.

`shell/plus_row.rs` `start_new`: `(NewKind::Browser, at) => self.new_browser(at, window, cx),`. `new_row_item` glyph: `RowItem::Browser => icon_svg(Icon::Globe, theme::ICON_SIZE, theme.text_muted).into_any_element(),`. `render_new_tab_button`: `plus_row_items(default_agent(&agents), shell_name.as_deref(), main, mkrew_ui::browser::available().is_ok())`.

`shell/chrome.rs` `new_menu_rows`, after the "Shell" row and before "Empty tab":

```rust
        if mkrew_ui::browser::available().is_ok() {
            rows.push(
                MenuItem::new("Browser", act(|shell, window, cx| shell.new_browser(None, window, cx)))
                    .row(),
            );
        }
```

- [ ] **Step 5: Lint and tests**

Run: `cargo fmt --all`, `cargo clippy -p mkrew-ui --all-targets -j 4 -- -D warnings`, `cargo test -p mkrew-ui --lib -j 4 plus_row:: keymap:: palette:: detached::`.
Expected: PASS.

- [ ] **Step 6: Smoke** (`scripts/smoke.ps1`)

1. Hover `+`: the row shows the globe after the note; its tooltip is "New browser".
2. Click it: a new tab with a browser pane, the empty state, the address field focused (or Ctrl+L focuses it); type `localhost:<port>` of a local static server started for the smoke (`python -m http.server` in a temp folder) and Enter: the page loads.
3. Drag the globe onto a terminal pane's right edge: a browser pane splits there.
4. `Alt+Shift+B` on a terminal pane: a browser pane on its right.
5. Open the palette, a tab menu, drag a pane: the page hides and shows the title card each time; closing shows it again.
6. Switch tabs, switch to Board and back: the page hides and comes back.
7. Tear the tab off (Shift+F4): the page shows in the detached window without reloading (the typed state of a form field on the page stays); Move back: same.
8. Quit and start the sandbox app again: the browser pane comes back on the same URL.
9. `shot` dark and light: toolbar, empty state, title card.
Expected: as described. Fix anything that looks off before committing (AGENTS: pixel perfection).

- [ ] **Step 7: Commit**

```bash
git add crates/mkrew-ui/src/plus_row.rs crates/mkrew-ui/src/shell/plus_row.rs crates/mkrew-ui/src/shell/chrome.rs crates/mkrew-ui/src/keymap.rs crates/mkrew-ui/src/palette.rs crates/mkrew-ui/src/palette/tests.rs crates/mkrew-ui/src/shell/sessions.rs crates/mkrew-ui/src/shell/detached_actions.rs crates/mkrew-ui/src/detached.rs crates/mkrew-ui/src/detached/tests.rs crates/mkrew-ui/src/shell/browser.rs crates/mkrew-ui/src/shell/navigation.rs
git -c user.email=tungnv975@gmail.com commit -m "feat(ui): New browser in the + row and the menu, Alt+Shift+B splits a browser pane"
```

---

### Task 7: ADR, SPEC, docs, final verify

**Files:**
- Create: `docs/decisions/0110-browser-pane.md`
- Modify: `docs/SPEC.md` (§5 leaves, §7 `tab.set_layout` row and a "Protocol 61" line after "Protocol 60", §10 links, §13 shortcut table, new `### §20M Browser pane (outside the milestones, ADR 0110)` after §20L, its line in the table of contents if §20 subsections are listed there)
- Modify: `docs/HUMAN_CHECKS.md`, `docs/PROGRESS.md`
- Create: `docs/screenshots/browser-pane-toolbar-dark.png`, `-light.png`, `browser-pane-empty-dark.png`, `-light.png`, `browser-pane-card-dark.png`, `-light.png` (from the smoke runs)

- [ ] **Step 1: Re-check numbers**

Run: `git -C C:/Users/NguyenTung/Workspace/PersonalProject/mkrew-build-kit/mkrew ls-tree --name-only master docs/decisions/ | tail -3`, the same for `feat/omp-tsp`, and `git show master:crates/mkrew-protocol/src/lib.rs | grep PROTOCOL_VERSION`.
Expected: master ADRs end at 0108 or 0109 (Seamless panes) and protocol 60. If 0110 or protocol 61 is taken, take the next free ones and update the spec, Task 1's test and every doc line of this task.

- [ ] **Step 2: ADR 0110** (follow the shape of `docs/decisions/0108-tern-panes.md`: title, Date, Status, Milestone, Context, Decision, Alternatives, Consequences; one sentence per line)

Decision points to state: a fourth leaf `{"web", "id"}`; the OS web view through `wry` as a native child, hidden under overlays and off screen; one link-taking browser pane per tab; Linux and machines without WebView2 use the system browser; macOS rebuilds a page moved to another window; keys given back by an init script; protocol 61. Alternatives: CEF offscreen (`wef`, about 1 GB more), an external browser window glued to the pane, `gpui-webview` (pins gpui 0.3.9), a new pane per link (Tern). Amends ADR 0108 (leaf kinds) and the link handling of SPEC §10 / §12.

- [ ] **Step 3: SPEC**

- §5 leaf sentence: add `and {"web": "https://example.com/", "id": "<ulid>"} (a browser pane, ADR 0110; "" before its first page)`.
- §7 `tab.set_layout` row: "leaves are sessions, files, the diff or web pages".
- §7 history: `Protocol 61 (ADR 0110): a tab layout's leaves can be browser panes (\`{"web": "<url>", "id": "<ulid>"}\`).`
- §10: web links in Chat and terminals open in a browser pane (point to §20M).
- §13 table rows: `Alt+Shift+B` / `⌥⌘B` New browser pane (global); `Ctrl+L` / `⌘L` Focus address (browser pane).
- §20M: spec sections 4 to 8 in short (opening, look, pages, native view, availability), each rule one line.

- [ ] **Step 4: HUMAN_CHECKS and PROGRESS**

`docs/HUMAN_CHECKS.md` "To verify": "Browser pane on macOS (open from a link and from +, Cmd+L, Cmd+Alt+B, moving a tab to a detached window rebuilds the page, dark / light)", "Browser pane on Windows without the WebView2 Runtime (one toast, links open in the system browser, a restored browser pane shows the reason and Open in system browser)", "check the browser pane visually (dark / light): toolbar, empty state, title card under the palette".
`docs/PROGRESS.md` "Notes for the next session": one line on the browser pane (ADR 0110, protocol 61, non-goals in the spec section 12).

- [ ] **Step 5: Final verify**

Run: `"C:/Program Files/Git/bin/bash.exe" scripts/verify.sh` (with the shared `CARGO_TARGET_DIR`).
If it passes 600 s, run `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -j 4 -- -D warnings`, then `cargo test --workspace -j 4` with no time limit. Known flakes (memory): `spike-term grid_snapshots` in a workspace run (passes with `-p spike-term`), the archive test, the resume test's leaked fake agents.
Expected: green. Grep the branch diff for raw hex colors and raw px numbers outside `theme.rs`: `git diff master --stat` then `git diff master -- crates/mkrew-ui/src | rg "#[0-9a-fA-F]{6}|px\(\d"`; both must be empty (only tokens).

- [ ] **Step 6: Commit**

```bash
git add docs/decisions/0110-browser-pane.md docs/SPEC.md docs/HUMAN_CHECKS.md docs/PROGRESS.md docs/screenshots/browser-pane-*.png
git -c user.email=tungnv975@gmail.com commit -m "docs: browser pane ADR 0110, SPEC §20M, human checks, screenshots"
```

- [ ] **Step 7: Main checkout untouched**

Run: `git -C C:/Users/NguyenTung/Workspace/PersonalProject/mkrew-build-kit/mkrew status --short`.
Expected: only the other session's files ("Seamless panes"); if a file this branch touched shows there, report it and do not restore it.
