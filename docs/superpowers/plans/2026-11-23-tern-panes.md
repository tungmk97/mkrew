# Tern panes Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Panes that look like Tern's, files and the diff as tabs or panes next to the terminals (no Editor or Diff view), four-edge drag and drop for everything, and a `+` that expands into a row of icons (default agent, default terminal, new note, more).

**Architecture:** The split tree's leaves get a kind (`Leaf::Session`, `Leaf::File`, `Leaf::Diff`) and every tree and tab operation works on a `PaneKey`; layouts stay JSON in the daemon, which already ignores leaves without a `session`.
Open files move from `FilesView.tabs` into a pure per-workspace `Documents` store, with gpui view state in per-file `DocView`s, so file panes render inside `pane_frame` like terminals and the Editor view, the file tab strip and the Diff view are removed.
Notes are created by a new daemon request `fs.new_note` that resolves the daemon's own `[notes] folder` (protocol 60).

**Tech Stack:** Rust (edition 2024), gpui-pre 0.3.7, serde / serde_json / toml, tokio (daemon), insta (theme goldens only, written by hand: `cargo insta` is not installed).

**Spec:** `docs/superpowers/specs/2026-11-23-tern-panes-design.md` (committed with this plan). Executors read it and this plan.

## Global Constraints

- Work in the worktree `C:/Users/NguyenTung/Workspace/PersonalProject/mkrew-build-kit/mkrew/.worktrees/tern-panes` on branch `feat/tern-panes` (base master 0a8d404). Use it as `cwd`. Never touch the main checkout or other worktrees (other branches are running there).
- Commit with `git -c user.email=tungnv975@gmail.com commit -m "<Conventional Commit>"`, only the files of the task, never push, never rewrite history.
- Every cargo command runs with `-j 4` (or `CARGO_BUILD_JOBS=4`) from the worktree. C: is low on disk (about 22 GB free): set `CARGO_TARGET_DIR=C:/Users/NguyenTung/Workspace/PersonalProject/mkrew-build-kit/mkrew/target` so the worktree shares the main target folder instead of creating a new one. Do not kill processes you did not start; never touch the installed Mkrew.
- After any protocol or CLI change run `cargo build -p mkrew-cli -p mkrew-fakeagent --bins -j 4` before `cargo test -p mkrew-daemon --test daemon -j 2` (daemon tests use the prebuilt `mk.exe`).
- Verify with `"C:/Program Files/Git/bin/bash.exe" scripts/verify.sh` (plain `bash` is WSL). If it passes 600 s or rustc crashes on mkrew-ui test builds (STATUS_STACK_BUFFER_OVERRUN), run fmt check + clippy, then `cargo test --workspace -j 4`. Intermediate tasks run only their focused commands.
- `PROTOCOL_VERSION` 58 -> 60 (59 is reserved by unmerged `feat/omp-tsp`). ADR 0108 (0107 is reserved by `feat/omp-tsp`). No migration. Re-check these numbers before Task 9 and at merge time.
- One pane per file and one Diff leaf per workspace, across all tabs and windows.
- New tabs open right after the active tab (`Tabs::position_after_active`).
- Gap between panes and frame edge 8 px (`PANE_HALF_GAP = space::XS`); no pane border; top stripe 2 px (`PANE_ACCENT_WIDTH`), precedence needs input > session color > focused with two or more panes.
- `pane_gap` vs `terminal_bg` contrast at least 1.06:1 in every theme.
- `+` row: expands on hover or keyboard focus, 150 ms ease-out, collapses 150 ms after the pointer leaves; items default agent, default terminal, new note, `▾` (today's menu).
- Notes: `[notes] folder` (empty = workspace root, relative = under the root, absolute = as is); name `note-YYYYMMDD-HHMMSS.md` local time, `-2`, `-3`, ... on clash; created by `fs.new_note`; opened kept, source mode.
- `NewNote` keys: PC `ctrl-alt-n`, mac `cmd-alt-n`.
- UI rules (SPEC §14, skill `mkrew-ui-design`): colors, spacing (4/8/12/16/24/32), radius and font sizes only from `theme.rs` tokens; hover, press and focus only through widgets in `shell/widgets.rs` or `.pressable(Look::..)` + `.on_press(..)`, never `.hover(`, `.active(` or `.cursor_pointer()` elsewhere (`group_hover` is fine); dark and light themes; tooltips through `tooltip(..)` with the shortcut; every interactive element has hover, focus, active and disabled states.
- No `unwrap`/`expect` outside tests unless provably infallible (comment why). `thiserror` in libraries, `anyhow` in binaries, `tracing` for logs.
- No test runs a real agent or touches the network. No test reads source text. Docs: no em dash, one sentence per line in long Markdown. Never edit CHANGELOG.md.
- Clean cutover: removed types, functions, commands and tests are deleted, not aliased. Deviating from SPEC needs ADR 0108 and the SPEC update in the same branch (Task 9).
- GPUI gotchas: an element with `track_focus` grabs focus on any mouse down; set `.hover()` at most once per element; drops only reach the element under the pointer (every overlay above a pane must repeat the pane's `on_drop` handlers); OS file drops go through `drop_catcher` + `drop_hover`, not `on_drop`.

## Review Focus

Input classes the spec implies but its tests do not name, most likely first. Each has a test in the task named in brackets.

- Unsaved edits when a file leaf leaves every layout by any path (close pane, close tab, close others or to the right, close window, close workspace, quit, preview replacement), versus moves (drag, tear-off, merge) that must never ask. Expect: closes ask Save / Don't save / Cancel, moves never ask, an edited preview tab is kept and not replaced. [Task 4]
- Stored layouts from protocol 58 and hand-broken or future leaves (`{"file": 3}`, `{"canvas": 1}`). Expect: old layouts load unchanged, bad leaves are dropped with a warning and the rest of the tab loads, an all-bad layout gives an empty tab without a panic. [Task 1]
- Opening a file that is already open, from any entry point (tree, palette, terminal link with `:line`, chat pill), while it sits in another tab or a detached window. Expect: its pane is focused (its tab activated, its window raised) and the line shown; a second leaf is never created. [Task 1, Task 4]
- A tab that holds a session and a file when the session ends, during the run and across a daemon restart. Expect: the tab stays with the file leaf focused. [Task 1]
- Notes folder values a user types: trailing separators, forward slashes on Windows, a relative path with `..`, `~`, spaces around it, a missing drive, two notes in the same second. Expect: trimmed and normalized, `..` and `~` refused with a message, a missing drive answers an error and creates nothing, the second note gets `-2`. [Task 7]

---

## Task order

| # | Task | Depends on |
|---|---|---|
| 1 | Typed leaves (`Leaf`, `PaneKey`, `Edge`), key-typed split and tabs API, protocol 60, daemon tests (sub-tasks 1a, 1b, 1c) | - |
| 2 | Pane look | 1 |
| 3-4 | Documents store, file leaves, no Editor view (parts A, B, C) | 1, 2 |
| 5 | Diff leaf, no Diff view | 3-4 |
| 6 | Four-edge drops | 1-5 |
| 7 | Notes in core and daemon (and the UI `notes_folder` left by Task 6) | 6 |
| 8 | The `+` row and New note | 1-7 |
| 9 | ADR 0108, SPEC, HUMAN_CHECKS, smoke, screenshots, final verify | 1-8 |

Each task (and each sub-task or part with its own commit) ends with fmt, clippy `-D warnings` and its tests green.
Tasks 3 and 4 are one task: splitting them would need a throwaway file tab list.
Task 6 leaves `Shell::notes_folder()` answering `None` (only files inside the root open from an OS drop); Task 7 Step 18 gives it its real body.

---

### Task 1: Typed leaves (`Leaf`, `PaneKey`, `Edge`), key-typed split and tabs API, protocol 60, daemon tests

Worktree: `.worktrees/tern-panes` (branch `feat/tern-panes`).
All paths are relative to it.
Run every command from the worktree root.
Windows: run cargo directly, not through a bash script.
Use `-j 4`.

The task has three sub-tasks.
Each one ends with `cargo fmt --all`, clippy with `-D warnings`, its tests, and a commit.

| Sub-task | What | Compiles alone because |
|---|---|---|
| 1a | `PaneKey`, `Leaf`, `Edge` and their tests in `split.rs` | Pure additions to a library module; no caller changes. |
| 1b | `Node::Leaf`, key-typed `split.rs`, lenient `from_json`, `insert_at`, `Preset::build(&[Leaf])`, key-typed `tabs.rs`, every compile fix in `shell/*.rs`, `main.rs`, `tabs/tests.rs`, `tests/lifecycle.rs`, `hosts.rs::focused_key` | One atomic type migration: nothing in between compiles. |
| 1c | New `Tabs` API (`insert_leaf`, `update_leaf`, `preview_tab`, `position_after_active`), protocol 60, daemon unit tests | Additions on top of 1b. |

**Files (whole task)**

- Modify: `crates/mkrew-ui/src/split.rs` (1a: after line 26 and the test module; 1b: lines 1-8, 28-46, 131-185, 196-581, tests 646-1128).
- Modify: `crates/mkrew-ui/src/tabs.rs` (1b: lines 5-14, 19-133, 143-758 and 760-958; 1c: new fns after `close_pane`).
- Modify: `crates/mkrew-ui/src/tabs/tests.rs` (1b: site table in Step 1b.6; 1b and 1c: new tests at the end).
- Modify: `crates/mkrew-ui/src/main.rs:108`.
- Modify: `crates/mkrew-ui/src/shell/pane_drop.rs`, `pane_render.rs`, `hosts.rs`, `focus.rs`, `workspace.rs`, `menus.rs`, `chrome.rs`, `tabs.rs` (the shell one).
- Modify: `crates/mkrew-ui/tests/lifecycle.rs:25,160`.
- Modify: `crates/mkrew-protocol/src/lib.rs:16`, `crates/mkrew-protocol/src/message/tests.rs:1394-1397`, `crates/mkrew-protocol/src/message.rs:639`.
- Modify: `crates/mkrew-daemon/src/tabs.rs` (comments at lines 1-2, 19, 44, 53; tests at the end), `crates/mkrew-daemon/tests/daemon/tabs_tasks.rs` (one e2e test).

**Interfaces**

Produces (all signatures are the shared contract; extras are listed under "Additions to the contract" at the end of this task):

```rust
// crates/mkrew-ui/src/split.rs
pub enum PaneKey { Session(Ulid), File(String), Diff }
pub enum Leaf { Session { session: Ulid, chat: Option<bool> }, File { file: String, md: Option<MdMode>, preview: bool }, Diff { diff: bool } }
pub enum Edge { Left, Right, Top, Bottom }
pub enum Node { Leaf(Leaf), Split { split: Axis, ratio: f64, children: Box<[Node; 2]> } }
// crates/mkrew-ui/src/tabs.rs: Tab::{focused, zoomed, focused_session, keys, sessions, shown, shown_sessions, rects, contains},
// Tabs::{tab_of, tab_with, focus, focus_pane, insert_leaf, update_leaf, move_pane, merge_tab, close_leaf, preview_tab, position_after_active, arrange}
// crates/mkrew-ui/src/shell/hosts.rs: Shell::focused_key(&self, window: &Window) -> Option<PaneKey>
```

Consumes: nothing from other tasks.
`Tabs::visible()` keeps its signature and meaning (`Vec<Ulid>` of sessions on screen); it now returns `shown_sessions()` of the active tab, so attach logic, `on_screen`, `newest_unplaced` and the integration tests need no change.

---

#### Sub-task 1a: `PaneKey`, `Leaf`, `Edge`

### Step 1a.1: Write the failing tests

Append inside `mod tests` of `crates/mkrew-ui/src/split.rs`, after the last test (`splitting_a_pane_keeps_its_chat_flag`, ends at line 1127, before the closing `}` at line 1128):

```rust
    #[test]
    fn the_three_leaf_kinds_round_trip() {
        let id = Ulid::new();
        let session = json!({"session": id.to_string(), "chat": true});
        let file = json!({"file": "src/a.rs", "md": "split", "preview": true});
        let diff = json!({"diff": true});
        for stored in [session, file, diff] {
            let leaf: Leaf = serde_json::from_value(stored.clone()).unwrap();
            assert_eq!(serde_json::to_value(&leaf).unwrap(), stored);
        }
        // Optional keys stay out of the stored form.
        assert_eq!(
            serde_json::to_value(Leaf::session(id)).unwrap(),
            json!({"session": id.to_string()})
        );
        assert_eq!(
            serde_json::to_value(Leaf::file("a.md")).unwrap(),
            json!({"file": "a.md"})
        );
        assert_eq!(
            serde_json::to_value(Leaf::diff()).unwrap(),
            json!({"diff": true})
        );
    }

    #[test]
    fn a_leaf_has_the_key_of_its_kind() {
        let id = Ulid::new();
        assert_eq!(Leaf::session(id).key(), PaneKey::Session(id));
        assert_eq!(
            Leaf::file("src/a.rs").key(),
            PaneKey::File("src/a.rs".to_owned())
        );
        assert_eq!(Leaf::diff().key(), PaneKey::Diff);
        // The flags are not part of the key: one file has one key.
        let flagged = Leaf::File {
            file: "src/a.rs".into(),
            md: Some(mkrew_core::settings::MdMode::Split),
            preview: true,
        };
        assert_eq!(flagged.key(), Leaf::file("src/a.rs").key());
        assert_eq!(PaneKey::Session(id).session(), Some(id));
        assert_eq!(PaneKey::File("a".into()).session(), None);
        assert_eq!(PaneKey::Diff.session(), None);
    }

    #[test]
    fn the_nearest_edge_is_the_smallest_distance() {
        assert_eq!(Edge::nearest(0.9, 0.5), Edge::Right);
        assert_eq!(Edge::nearest(0.1, 0.5), Edge::Left);
        assert_eq!(Edge::nearest(0.5, 0.1), Edge::Top);
        assert_eq!(Edge::nearest(0.5, 0.9), Edge::Bottom);
        // Off-axis pointers still pick the closest side.
        assert_eq!(Edge::nearest(0.2, 0.3), Edge::Left);
        assert_eq!(Edge::nearest(0.7, 0.2), Edge::Top);
        assert_eq!(Edge::nearest(0.6, 0.9), Edge::Bottom);
        assert_eq!(Edge::nearest(0.8, 0.7), Edge::Right);
        // On the pane's own border.
        assert_eq!(Edge::nearest(0.0, 0.5), Edge::Left);
        assert_eq!(Edge::nearest(1.0, 0.5), Edge::Right);
    }

    #[test]
    fn edge_ties_go_right_then_bottom_then_left_then_top() {
        // The very middle: all four are equal.
        assert_eq!(Edge::nearest(0.5, 0.5), Edge::Right);
        // Right and Bottom (the bottom right corner), Right and Top.
        assert_eq!(Edge::nearest(0.75, 0.75), Edge::Right);
        assert_eq!(Edge::nearest(0.75, 0.25), Edge::Right);
        // Bottom and Left (the bottom left corner).
        assert_eq!(Edge::nearest(0.25, 0.75), Edge::Bottom);
        // Left and Top (the top left corner).
        assert_eq!(Edge::nearest(0.25, 0.25), Edge::Left);
        // Left and Right at the horizontal middle, Top clearly nearer.
        assert_eq!(Edge::nearest(0.5, 0.25), Edge::Top);
    }

    #[test]
    fn an_edge_knows_its_axis_and_whether_it_leads() {
        assert_eq!(Edge::Left.axis(), Axis::Vertical);
        assert_eq!(Edge::Right.axis(), Axis::Vertical);
        assert_eq!(Edge::Top.axis(), Axis::Horizontal);
        assert_eq!(Edge::Bottom.axis(), Axis::Horizontal);
        assert!(Edge::Left.leads());
        assert!(Edge::Top.leads());
        assert!(!Edge::Right.leads());
        assert!(!Edge::Bottom.leads());
    }
```

### Step 1a.2: Run the tests to see them fail

```
cargo test -p mkrew-ui --lib split -j 4
```

Expected: compile FAIL with `cannot find type `Leaf` in this scope` and `failed to resolve: use of undeclared type `Edge``.

### Step 1a.3: Implement the three types

In `crates/mkrew-ui/src/split.rs`, insert after the `Direction` enum (after line 26, before the `Node` doc comment at line 28):

```rust
/// What a pane of the tab shows, as the key that finds it: one session, one file (the stored
/// path), or the diff. A tab holds a key at most once, and so does a workspace for files and
/// the diff (ADR 0108).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum PaneKey {
    Session(Ulid),
    File(String),
    Diff,
}

impl PaneKey {
    /// The session of a session pane.
    pub fn session(&self) -> Option<Ulid> {
        match self {
            PaneKey::Session(id) => Some(*id),
            PaneKey::File(_) | PaneKey::Diff => None,
        }
    }
}

/// One pane of the split tree and what is saved on it. The JSON is untagged: the keys decide
/// the kind, so every layout of an older version (sessions only) still reads.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Leaf {
    /// A pane showing one session. `chat` is the pane's saved mode (ADR 0104): `Some(true)`
    /// Chat, `Some(false)` Terminal, absent = not set (`[ui] chat_default` decides).
    Session {
        session: Ulid,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        chat: Option<bool>,
    },
    /// A file of the workspace: its path relative to the root with `/` separators (an
    /// absolute path for a note in a notes folder outside it). `md` is the pane's own Markdown
    /// mode, absent = `[ui] md_mode`. `preview` marks the workspace's preview tab.
    File {
        file: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        md: Option<mkrew_core::settings::MdMode>,
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        preview: bool,
    },
    /// The workspace's diff. `diff` is always `true` on the wire.
    Diff { diff: bool },
}

impl Leaf {
    pub fn session(id: Ulid) -> Leaf {
        Leaf::Session {
            session: id,
            chat: None,
        }
    }

    pub fn file(path: impl Into<String>) -> Leaf {
        Leaf::File {
            file: path.into(),
            md: None,
            preview: false,
        }
    }

    pub fn diff() -> Leaf {
        Leaf::Diff { diff: true }
    }

    pub fn key(&self) -> PaneKey {
        match self {
            Leaf::Session { session, .. } => PaneKey::Session(*session),
            Leaf::File { file, .. } => PaneKey::File(file.clone()),
            Leaf::Diff { .. } => PaneKey::Diff,
        }
    }
}

/// The side of a pane a dropped leaf goes to: it takes that half of the pane.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Edge {
    Left,
    Right,
    Top,
    Bottom,
}

impl Edge {
    /// The edge nearest to the pointer at (`x`, `y`), fractions of the pane (0..1): the
    /// smallest of `x`, `1 - x`, `y`, `1 - y`. A tie goes Right, then Bottom, then Left, then
    /// Top, so the middle of a pane splits it to the right like the old drop did.
    pub fn nearest(x: f32, y: f32) -> Edge {
        [
            (1.0 - x, Edge::Right),
            (1.0 - y, Edge::Bottom),
            (x, Edge::Left),
            (y, Edge::Top),
        ]
        .into_iter()
        // `min_by` keeps the first of equal elements, which is the tie order above.
        .min_by(|a, b| a.0.total_cmp(&b.0))
        .map_or(Edge::Right, |(_, edge)| edge)
    }

    /// The split that holds the old pane and the new one.
    pub fn axis(self) -> Axis {
        match self {
            Edge::Left | Edge::Right => Axis::Vertical,
            Edge::Top | Edge::Bottom => Axis::Horizontal,
        }
    }

    /// The new leaf is the first child of the split (left of / above the old pane).
    pub fn leads(self) -> bool {
        matches!(self, Edge::Left | Edge::Top)
    }
}
```

### Step 1a.4: Run the tests to see them pass

```
cargo test -p mkrew-ui --lib split -j 4
```

Expected: PASS, including the four new tests and every old one (`Node` is untouched in this sub-task).

### Step 1a.5: Format, lint, commit

```
cargo fmt --all
cargo clippy -p mkrew-ui --all-targets -j 4 -- -D warnings
cargo test -p mkrew-ui --lib split -j 4
git add crates/mkrew-ui/src/split.rs
git -c user.email=tungnv975@gmail.com commit -m "feat(ui): PaneKey, Leaf and Edge types for typed panes"
```

---

#### Sub-task 1b: Migrate the split tree and the tabs to typed leaves

This sub-task is one atomic migration: `Node::Pane` goes, so `split.rs`, `tabs.rs`, the tests and every shell caller change together.
Nothing compiles between Step 1b.2 and Step 1b.8.
Do the steps in order and run the tests only at Step 1b.9.

### Step 1b.1: Write the new tests (they cannot compile yet)

**`crates/mkrew-ui/src/split.rs`**, inside `mod tests`, after the 1a tests.
First add these helpers right after `fn ids` (line 642-644):

```rust
    fn k(id: Ulid) -> PaneKey {
        PaneKey::Session(id)
    }

    fn leaves(ids: &[Ulid]) -> Vec<Leaf> {
        ids.iter().map(|id| Leaf::session(*id)).collect()
    }

    /// `Node::split` took a session; the tests keep that shape.
    fn split(node: &mut Node, target: Ulid, axis: Axis, new: Ulid) -> bool {
        node.split_with(&k(target), axis, Node::pane(new))
    }

    fn pane_json(id: Ulid) -> serde_json::Value {
        json!({"session": id.to_string()})
    }

    fn halves_json(
        axis: &str,
        first: serde_json::Value,
        second: serde_json::Value,
    ) -> serde_json::Value {
        json!({"split": axis, "ratio": 0.5, "children": [first, second]})
    }
```

Then add these tests at the end of the module:

```rust
    #[test]
    fn a_layout_with_all_three_leaf_kinds_round_trips() {
        let a = Ulid::new();
        let stored = json!({"split":"vertical","ratio":0.5,"children":[
            {"session": a.to_string(), "chat": true},
            {"split":"horizontal","ratio":0.6,"children":[
                {"file": "src/a.rs", "md": "split", "preview": true},
                {"diff": true}
            ]}
        ]});
        let node = Node::from_json(&stored).unwrap().unwrap();
        assert_eq!(
            node.keys(),
            [k(a), PaneKey::File("src/a.rs".into()), PaneKey::Diff]
        );
        assert_eq!(node.sessions(), [a]);
        assert_eq!(Node::to_json(Some(&node)), stored);
        // A file with no flags stores no flags.
        let plain = json!({"file": "README.md"});
        let node = Node::from_json(&plain).unwrap().unwrap();
        assert_eq!(Node::to_json(Some(&node)), plain);
    }

    #[test]
    fn a_protocol_58_layout_reads_unchanged() {
        let [a, b, c] = ids();
        // Sessions only, with and without the `chat` flag: what every stored layout looks like.
        let stored = json!({"split":"vertical","ratio":0.25,"children":[
            {"session": a.to_string(), "chat": false},
            {"split":"horizontal","ratio":0.5,"children":[
                {"session": b.to_string()}, {"session": c.to_string(), "chat": true}
            ]}
        ]});
        let node = Node::from_json(&stored).unwrap().unwrap();
        let expected = Node::Split {
            split: Axis::Vertical,
            ratio: 0.25,
            children: Box::new([
                Node::leaf(Leaf::Session {
                    session: a,
                    chat: Some(false),
                }),
                Node::Split {
                    split: Axis::Horizontal,
                    ratio: 0.5,
                    children: Box::new([
                        Node::pane(b),
                        Node::leaf(Leaf::Session {
                            session: c,
                            chat: Some(true),
                        }),
                    ]),
                },
            ]),
        };
        assert_eq!(node, expected);
        assert_eq!(Node::to_json(Some(&node)), stored);
        assert_eq!(node.chat_of(a), Some(false));
        assert_eq!(node.chat_of(b), None);
        assert_eq!(node.chat_of(c), Some(true));
    }

    #[test]
    fn leaves_that_do_not_parse_are_dropped_and_the_rest_loads() {
        let [a, b] = ids();
        let load = |value: serde_json::Value| Node::from_json(&value).unwrap();
        // One leaf of an unknown shape, one hand-broken: only those go.
        assert_eq!(
            load(halves_json("vertical", pane_json(a), json!({"canvas": 1}))),
            Some(Node::pane(a))
        );
        assert_eq!(
            load(halves_json("vertical", json!({"file": 3}), pane_json(a))),
            Some(Node::pane(a))
        );
        // `diff` is always true on the wire.
        assert_eq!(
            load(halves_json("vertical", json!({"diff": false}), pane_json(a))),
            Some(Node::pane(a))
        );
        // A split whose child is bad becomes the good child, at any depth.
        let deep = halves_json(
            "vertical",
            pane_json(a),
            halves_json(
                "horizontal",
                json!({"session": "not-a-ulid"}),
                halves_json("horizontal", pane_json(b), json!({"diff": false})),
            ),
        );
        assert_eq!(
            Node::to_json(load(deep).as_ref()),
            halves_json("vertical", pane_json(a), pane_json(b))
        );
        // Both children bad: the split is gone.
        assert_eq!(
            load(halves_json("vertical", json!({"canvas": 1}), json!({"file": 3}))),
            None
        );
    }

    #[test]
    fn a_root_that_is_entirely_bad_loads_as_an_empty_tab() {
        for bad in [
            json!({"canvas": 1}),
            json!({"file": 3}),
            json!({"diff": false}),
            json!({"session": "not-a-ulid"}),
            json!({}),
            json!([]),
            json!("pane"),
            json!(7),
        ] {
            assert_eq!(Node::from_json(&bad).unwrap(), None, "{bad}");
        }
    }

    #[test]
    fn a_malformed_split_or_ratio_is_still_an_error() {
        let [a, b] = ids();
        let three = json!({"split":"vertical","ratio":0.5,"children":[
            pane_json(a), pane_json(b), pane_json(a)
        ]});
        assert!(matches!(Node::from_json(&three), Err(LayoutError::Json(_))));
        let axis = json!({"split":"diagonal","ratio":0.5,"children":[pane_json(a), pane_json(b)]});
        assert!(matches!(Node::from_json(&axis), Err(LayoutError::Json(_))));
        let text = json!({"split":"vertical","ratio":"half","children":[pane_json(a), pane_json(b)]});
        assert!(matches!(Node::from_json(&text), Err(LayoutError::Json(_))));
        // The ratio is checked even when a child is dropped.
        let ratio = json!({"split":"vertical","ratio":1.5,"children":[
            pane_json(a), json!({"canvas": 1})
        ]});
        assert!(matches!(
            Node::from_json(&ratio),
            Err(LayoutError::Ratio(r)) if r == 1.5
        ));
    }

    #[test]
    fn the_same_key_twice_in_a_layout_is_refused() {
        let a = Ulid::new();
        let twice = halves_json("horizontal", pane_json(a), pane_json(a));
        assert!(matches!(
            Node::from_json(&twice),
            Err(LayoutError::Duplicate(key)) if key == k(a)
        ));
        // A file is one key whatever its flags.
        let files = halves_json(
            "vertical",
            json!({"file": "a.md", "preview": true}),
            json!({"file": "a.md", "md": "source"}),
        );
        assert!(matches!(
            Node::from_json(&files),
            Err(LayoutError::Duplicate(PaneKey::File(path))) if path == "a.md"
        ));
        let diffs = halves_json("vertical", json!({"diff": true}), json!({"diff": true}));
        assert!(matches!(
            Node::from_json(&diffs),
            Err(LayoutError::Duplicate(PaneKey::Diff))
        ));
        // Two different files are fine.
        let two = halves_json("vertical", json!({"file": "a.md"}), json!({"file": "b.md"}));
        assert!(Node::from_json(&two).unwrap().is_some());
    }

    #[test]
    fn insert_at_puts_the_leaf_on_each_of_the_four_edges() {
        let a = Ulid::new();
        let file = || Node::leaf(Leaf::file("src/a.rs"));
        let want = |axis: &str, first: serde_json::Value, second: serde_json::Value| {
            halves_json(axis, first, second)
        };
        let new = json!({"file": "src/a.rs"});
        for (edge, expected) in [
            (Edge::Right, want("vertical", pane_json(a), new.clone())),
            (Edge::Left, want("vertical", new.clone(), pane_json(a))),
            (Edge::Bottom, want("horizontal", pane_json(a), new.clone())),
            (Edge::Top, want("horizontal", new.clone(), pane_json(a))),
        ] {
            let mut node = Node::pane(a);
            assert!(node.insert_at(&k(a), edge, file()), "{edge:?}");
            assert_eq!(Node::to_json(Some(&node)), expected, "{edge:?}");
        }
    }

    #[test]
    fn insert_at_splits_an_inner_leaf_and_keeps_its_flags() {
        let [a, b] = ids();
        let mut node = Node::pane(a);
        assert!(split(&mut node, a, Axis::Vertical, b));
        assert!(node.set_chat(b, Some(true)));
        assert!(node.insert_at(&k(b), Edge::Top, Node::leaf(Leaf::diff())));
        assert_eq!(
            Node::to_json(Some(&node)),
            halves_json(
                "vertical",
                pane_json(a),
                halves_json(
                    "horizontal",
                    json!({"diff": true}),
                    json!({"session": b.to_string(), "chat": true})
                )
            )
        );
        assert_eq!(node.keys(), [k(a), PaneKey::Diff, k(b)]);
    }

    #[test]
    fn insert_at_refuses_a_missing_target_or_a_key_already_there() {
        let [a, b] = ids();
        let mut node = Node::pane(a);
        assert!(node.insert_at(&k(a), Edge::Right, Node::leaf(Leaf::file("x"))));
        let before = node.clone();
        // No such target.
        assert!(!node.insert_at(&k(b), Edge::Right, Node::leaf(Leaf::diff())));
        assert!(!node.insert_at(
            &PaneKey::File("other".into()),
            Edge::Left,
            Node::leaf(Leaf::diff())
        ));
        // The key is in the tree already, alone or inside a whole layout.
        assert!(!node.insert_at(&k(a), Edge::Left, Node::leaf(Leaf::file("x"))));
        assert!(!node.insert_at(&k(a), Edge::Left, Node::pane(a)));
        let layout = Node::Split {
            split: Axis::Vertical,
            ratio: 0.5,
            children: Box::new([Node::leaf(Leaf::diff()), Node::pane(a)]),
        };
        assert!(!node.insert_at(&k(a), Edge::Left, layout));
        assert_eq!(node, before, "nothing changed");
    }

    #[test]
    fn a_whole_layout_goes_in_on_the_leading_edge() {
        let [a, b, c] = ids();
        let mut node = Node::pane(a);
        let moved = Node::Split {
            split: Axis::Horizontal,
            ratio: 0.3,
            children: Box::new([Node::pane(b), Node::leaf(Leaf::file("x"))]),
        };
        assert!(node.insert_at(&k(a), Edge::Left, moved.clone()));
        assert_eq!(
            node,
            Node::Split {
                split: Axis::Vertical,
                ratio: 0.5,
                children: Box::new([moved, Node::pane(a)]),
            }
        );
        assert!(!node.contains(&k(c)));
    }

    #[test]
    fn file_and_diff_leaves_lay_out_close_move_and_resize_like_sessions() {
        // a | (x / diff)
        let a = Ulid::new();
        let x = PaneKey::File("x".into());
        let mut node = Node::pane(a);
        assert!(node.split_with(&k(a), Axis::Vertical, Node::leaf(Leaf::file("x"))));
        assert!(node.split_with(&x, Axis::Horizontal, Node::leaf(Leaf::diff())));
        let keys: Vec<PaneKey> = node.rects().into_iter().map(|(key, _)| key).collect();
        assert_eq!(keys, [k(a), x.clone(), PaneKey::Diff]);
        assert!(node.contains(&PaneKey::Diff));
        assert!(!node.contains(&PaneKey::File("y".into())));
        // Neighbours across kinds: a's right neighbour is the upper one of the column.
        assert_eq!(node.neighbour(&k(a), Direction::Right), Some(x.clone()));
        assert_eq!(node.neighbour(&x, Direction::Down), Some(PaneKey::Diff));
        assert_eq!(node.neighbour(&PaneKey::Diff, Direction::Left), Some(k(a)));
        // Resize by a file leaf moves the stacked divider.
        assert!(node.resize(&x, Direction::Down, 0.1));
        assert_ratios(&node, &[0.5, 0.6]);
        // Closing a leaf promotes the sibling and says which leaf takes the focus.
        let closed = node.clone().close(&x);
        assert_eq!(closed.focus, Some(PaneKey::Diff));
        assert_eq!(closed.remaining.unwrap().keys(), [k(a), PaneKey::Diff]);
        let closed = node.clone().close(&PaneKey::Diff);
        assert_eq!(closed.focus, Some(x.clone()));
        let closed = node.close(&k(a));
        assert_eq!(closed.focus, Some(x));
        assert_eq!(
            closed.remaining.unwrap().keys(),
            [PaneKey::File("x".into()), PaneKey::Diff]
        );
    }

    #[test]
    fn chat_flags_only_live_on_session_leaves() {
        let [a, other] = ids();
        let mut node = Node::pane(a);
        assert!(node.split_with(&k(a), Axis::Vertical, Node::leaf(Leaf::file("x"))));
        assert!(node.set_chat(a, Some(true)));
        assert_eq!(node.chat_of(a), Some(true));
        assert!(!node.set_chat(other, Some(true)), "not in the tree");
        assert_eq!(node.chat_of(other), None);
        // The file leaf has no chat flag and is not touched.
        assert_eq!(node.leaf_of(&PaneKey::File("x".into())), Some(&Leaf::file("x")));
    }

    #[test]
    fn leaf_mut_changes_a_leaf_in_place() {
        let a = Ulid::new();
        let mut node = Node::pane(a);
        assert!(node.split_with(&k(a), Axis::Vertical, Node::leaf(Leaf::file("x"))));
        let key = PaneKey::File("x".into());
        if let Some(Leaf::File { md, preview, .. }) = node.leaf_mut(&key) {
            *md = Some(mkrew_core::settings::MdMode::Source);
            *preview = true;
        }
        assert_eq!(
            Node::to_json(Some(&node))["children"][1],
            json!({"file": "x", "md": "source", "preview": true})
        );
        assert!(node.leaf_mut(&PaneKey::Diff).is_none());
        assert_eq!(node.leaves().len(), 2);
        assert!(node.any_leaf(|leaf| matches!(leaf, Leaf::File { preview: true, .. })));
        assert!(!node.any_leaf(|leaf| matches!(leaf, Leaf::Diff { .. })));
    }

    #[test]
    fn a_preset_builds_with_whole_leaves() {
        let a = Ulid::new();
        let file = Leaf::File {
            file: "README.md".into(),
            md: Some(mkrew_core::settings::MdMode::Split),
            preview: true,
        };
        let built = Preset::Columns2
            .build(&[Leaf::session(a), file.clone()])
            .unwrap();
        assert_eq!(
            Node::to_json(Some(&built)),
            halves_json(
                "vertical",
                pane_json(a),
                json!({"file": "README.md", "md": "split", "preview": true})
            )
        );
        assert_eq!(built.leaves(), [Leaf::session(a), file]);
    }
```

**Rewrite three existing tests in full** (they change behavior or use the old signatures heavily).

Replace `presets_build_their_trees_in_pane_order` (lines 677-710) with:

```rust
    #[test]
    fn presets_build_their_trees_in_pane_order() {
        let [a, b, c, d] = [Ulid::new(), Ulid::new(), Ulid::new(), Ulid::new()];
        let half = |axis: &str, first: serde_json::Value, second: serde_json::Value| json!({"split": axis, "ratio": 0.5, "children": [first, second]});
        let pane = |id: Ulid| json!({"session": id.to_string()});
        let json = |preset: Preset, sessions: &[Ulid]| {
            Node::to_json(preset.build(&leaves(sessions)).as_ref())
        };
        assert_eq!(
            json(Preset::Columns2, &[a, b]),
            half("vertical", pane(a), pane(b))
        );
        assert_eq!(
            json(Preset::Rows2, &[a, b]),
            half("horizontal", pane(a), pane(b))
        );
        assert_eq!(
            json(Preset::MainAndStack, &[a, b, c]),
            half("vertical", pane(a), half("horizontal", pane(b), pane(c)))
        );
        assert_eq!(
            json(Preset::Grid, &[a, b, c, d]),
            half(
                "vertical",
                half("horizontal", pane(a), pane(b)),
                half("horizontal", pane(c), pane(d))
            )
        );
        // Re-applied to its own panes (in tree order), a preset keeps every pane in place.
        let grid = Preset::Grid.build(&leaves(&[a, b, c, d])).unwrap();
        assert_eq!(Preset::Grid.build(&grid.leaves()), Some(grid));
        // Only the preset's number of panes.
        assert_eq!(Preset::Columns3.build(&leaves(&[a, b])), None);
        assert_eq!(Preset::Columns2.build(&leaves(&[a, b, c])), None);
    }
```

Replace `invalid_layouts_are_rejected` (lines 814-837) with:

```rust
    #[test]
    fn invalid_layouts_are_rejected() {
        let [a, b] = ids();
        let three = json!({"split":"vertical","ratio":0.5,"children":[
            {"session": a.to_string()}, {"session": b.to_string()}, {"session": a.to_string()}
        ]});
        assert!(matches!(Node::from_json(&three), Err(LayoutError::Json(_))));
        let ratio = json!({"split":"vertical","ratio":1.5,"children":[
            {"session": a.to_string()}, {"session": b.to_string()}
        ]});
        assert!(matches!(
            Node::from_json(&ratio),
            Err(LayoutError::Ratio(_))
        ));
        let twice = json!({"split":"horizontal","ratio":0.5,"children":[
            {"session": a.to_string()}, {"session": a.to_string()}
        ]});
        assert!(matches!(
            Node::from_json(&twice),
            Err(LayoutError::Duplicate(key)) if key == PaneKey::Session(a)
        ));
        // A leaf that is not a pane is dropped, not an error (ADR 0108): the tab opens empty.
        assert_eq!(
            Node::from_json(&json!({"session": "not-a-ulid"})).unwrap(),
            None
        );
        assert_eq!(Node::from_json(&json!("pane")).unwrap(), None);
    }
```

Replace `neighbour_moves_across_nested_splits` (lines 980-1008) with:

```rust
    #[test]
    fn neighbour_moves_across_nested_splits() {
        // ┌───┬───┐
        // │   │ b │
        // │ a ├───┤
        // │   │ c │
        // ├───┴───┤
        // │   d   │
        // └───────┘
        let [a, b, c, d] = ids();
        let mut node = Node::pane(a);
        split(&mut node, a, Axis::Horizontal, d);
        split(&mut node, a, Axis::Vertical, b);
        split(&mut node, b, Axis::Horizontal, c);

        assert_eq!(node.neighbour(&k(a), Direction::Right), Some(k(b)));
        assert_eq!(node.neighbour(&k(c), Direction::Left), Some(k(a)));
        assert_eq!(node.neighbour(&k(b), Direction::Left), Some(k(a)));
        assert_eq!(node.neighbour(&k(b), Direction::Down), Some(k(c)));
        assert_eq!(node.neighbour(&k(c), Direction::Down), Some(k(d)));
        assert_eq!(node.neighbour(&k(a), Direction::Down), Some(k(d)));
        assert_eq!(node.neighbour(&k(c), Direction::Up), Some(k(b)));
        // d spans both columns and overlaps a and c equally: the left one wins.
        assert_eq!(node.neighbour(&k(d), Direction::Up), Some(k(a)));
        assert_eq!(node.neighbour(&k(a), Direction::Left), None);
        assert_eq!(node.neighbour(&k(b), Direction::Up), None);
        assert_eq!(node.neighbour(&k(d), Direction::Down), None);
        assert_eq!(node.neighbour(&k(Ulid::new()), Direction::Up), None);
    }
```

**Mechanical rewrite of the other existing tests** (apply to the original line numbers of `split.rs`, before any edit shifts them; do it first with the file open, or use `grep -n` to find the same lines afterwards).

Rules:

| Rule | Old | New |
|---|---|---|
| R1 | `node.split(X, AX, Y)` | `split(&mut node, X, AX, Y)` (also inside `assert!(..)` and `assert!(!..)`) |
| R2 | `.close(X)` | `.close(&k(X))` |
| R3 | `.resize(X, ` | `.resize(&k(X), ` |
| R4 | `.neighbour(X, ` | `.neighbour(&k(X), ` |
| R5 | `.split_with(X, ` | `.split_with(&k(X), ` |
| R6 | expectation `Some(Y)` of `closed.focus` | `Some(k(Y))` |
| R7 | `Node::Pane { .. }` | `Node::Leaf(_)` |

| Original lines | Test | Change |
|---|---|---|
| 664-666 | `a_whole_layout_splits_in_beside_a_pane` | R5 on the three `split_with` calls: `&k(a)`, `&k(b)`, `&k(a)` |
| 736-738, 745-749 | `equalize_shares_each_run_of_splits_evenly` | R1 on 736, 737, 745, 746, 747; R3 on 748 (`node.resize(&k(a), Direction::Right, 0.3)`) |
| 750-753 | same | `let rects: std::collections::HashMap<PaneKey, Rect> = ...;` and the four indexings become `rects[&k(a)]`, `rects[&k(b)]` (twice), `rects[&k(c)]`, `rects[&k(d)]` |
| 761-762 | `dividers_and_set_ratio_address_splits_in_tree_order` | R1 |
| 843-844 | `split_puts_the_new_pane_right_or_below` | R1 |
| 860-861 | `split_of_unknown_target_or_existing_session_is_refused` | R1 on both (`!split(&mut node, c, Axis::Vertical, b)`, `!split(&mut node, a, Axis::Vertical, a)`) |
| 869-870, 873-874, 886-887 | `close_promotes_the_sibling_and_focuses_the_nearest_pane` | R1 on 869, 870; R2 on 873 and 886; R6 on 874 and 887 (`Some(k(b))`) |
| 904-905, 909-910 | `close_keeps_child_order_and_ratio` | R1; R2 (`node.close(&k(b))`); R6 (`Some(k(c))`) |
| 922, 925 | `close_the_last_pane_or_an_unknown_one` | R2: `Node::pane(a).close(&k(a))`, `Node::pane(a).close(&k(b))` |
| 944, 956, 968 | `rects_follow_ratios` | the expected tuples start `(k(a),`, `(k(b),`, `(k(c),` |
| 1012 | `ratios` helper | R7 |
| 1028-1029, 1032, 1034, 1037, 1038, 1041-1043 | `resize_moves_the_innermost_matching_divider` | R1 on 1028, 1029; R3 on every `node.resize(..)` and on `Node::pane(a).resize(&k(a), Direction::Left, 0.1)`; line 1042 `node.resize(&k(Ulid::new()), Direction::Left, 0.1)` |
| 1059-1063 | `resize_stops_at_the_minimum_share` | R1 on 1059; R3 on 1060, 1062, 1063 |
| 1082 | `neighbour_prefers_the_largest_overlap` | `assert_eq!(node.neighbour(&k(a), Direction::Right), Some(k(c)));` |
| 1106, 1116 | `set_chat_changes_only_that_pane_and_reports_a_change` | R1 on 1106; R2 on 1116 (`node.close(&k(b))`) |
| 1125 | `splitting_a_pane_keeps_its_chat_flag` | R1 |

Tests that need no change: `a_drop_leaning_right_splits_right_else_below` (the `drop_axis` test; Task 6 deletes it), `three_columns_are_equal`, `spec_example_round_trips`, `null_is_an_empty_tab`, `a_leaf_keeps_its_chat_flag_through_json_and_absent_stays_absent`.

**`crates/mkrew-ui/src/tabs/tests.rs`**: add these helpers right after the `pane` helper (line 660):

```rust
fn k(id: Ulid) -> PaneKey {
    PaneKey::Session(id)
}

fn file(path: &str) -> serde_json::Value {
    json!({"file": path})
}

fn file_key(path: &str) -> PaneKey {
    PaneKey::File(path.to_owned())
}

/// A file leaf with every optional key set.
fn full_file() -> serde_json::Value {
    json!({"file": "README.md", "md": "split", "preview": true})
}
```

Then append these tests at the end of the file (after the last test, line 1314):

```rust
#[test]
fn a_file_leaf_is_a_pane_but_not_a_session() {
    let project = Ulid::new();
    let a = Ulid::new();
    let mut tabs = Tabs::new(project);
    tabs.load(vec![wire(
        project,
        "one",
        1.0,
        split("vertical", pane(a), file("src/a.rs")),
    )]);
    let tab = tabs.active().unwrap();
    assert_eq!(tab.keys(), [k(a), file_key("src/a.rs")]);
    assert_eq!(tab.sessions(), [a]);
    assert_eq!(tab.focused(), Some(&k(a)), "the first leaf has the focus");
    assert!(tab.contains(&file_key("src/a.rs")));

    // With the file focused no session has the keyboard, and the zoom shows the file alone.
    assert!(tabs.focus(&file_key("src/a.rs")));
    let tab = tabs.active().unwrap();
    assert_eq!(tab.focused_session(), None);
    assert_eq!(tab.focused(), Some(&file_key("src/a.rs")));
    assert!(tabs.toggle_zoom());
    let tab = tabs.active().unwrap();
    assert_eq!(tab.shown(), [file_key("src/a.rs")]);
    assert!(tab.shown_sessions().is_empty());
    assert!(tabs.visible().is_empty(), "no session is on screen");
    assert_eq!(tab.rects().len(), 1);
    assert_eq!(tab.rects()[0].0, file_key("src/a.rs"));

    // Focusing the session ends the zoom: its terminal is on screen again.
    assert!(tabs.focus(&k(a)));
    assert_eq!(tabs.visible(), vec![a]);
}

#[test]
fn tab_with_finds_any_leaf_in_any_tab() {
    let project = Ulid::new();
    let a = Ulid::new();
    let one = wire(project, "one", 1.0, pane(a));
    let two = wire(
        project,
        "two",
        2.0,
        split("vertical", file("src/a.rs"), json!({"diff": true})),
    );
    let mut tabs = Tabs::new(project);
    tabs.load(vec![one.clone(), two.clone()]);
    assert_eq!(tabs.active().map(|t| t.id), Some(one.id));

    // A leaf of a tab that is not the active one (a detached window shows it from the same
    // `Tabs`, so this is also how a file in a detached window is found).
    assert_eq!(
        tabs.tab_with(&file_key("src/a.rs")).map(|t| t.id),
        Some(two.id)
    );
    assert_eq!(tabs.tab_with(&PaneKey::Diff).map(|t| t.id), Some(two.id));
    assert_eq!(tabs.tab_with(&k(a)).map(|t| t.id), Some(one.id));
    assert_eq!(tabs.tab_of(a).map(|t| t.id), Some(one.id));
    assert!(tabs.tab_with(&file_key("src/b.rs")).is_none());

    // `focus_pane` reaches a leaf of another tab without activating it.
    assert!(tabs.focus_pane(two.id, &PaneKey::Diff));
    assert_eq!(tabs.active().map(|t| t.id), Some(one.id));
    assert_eq!(
        tabs.tab(two.id).and_then(Tab::focused),
        Some(&PaneKey::Diff)
    );
    assert!(!tabs.focus_pane(one.id, &PaneKey::Diff), "not in that tab");
    // `focus` activates the tab that holds the leaf.
    assert!(tabs.focus(&file_key("src/a.rs")));
    assert_eq!(tabs.active().map(|t| t.id), Some(two.id));
}

#[test]
fn a_tab_loads_around_a_leaf_it_cannot_read() {
    let project = Ulid::new();
    let a = Ulid::new();
    let mut tabs = Tabs::new(project);
    tabs.load(vec![wire(
        project,
        "one",
        1.0,
        split("vertical", pane(a), json!({"canvas": 1})),
    )]);
    let tab = tabs.active().unwrap();
    assert_eq!(tab.layout(), Some(&Node::pane(a)));
    assert_eq!(tabs.visible(), vec![a]);
    // A layout that is bad all the way down is an empty tab, not a lost one.
    tabs.load(vec![wire(project, "two", 2.0, json!({"canvas": 1}))]);
    assert_eq!(tabs.tabs().len(), 1);
    assert_eq!(tabs.tabs()[0].layout(), None);
}

#[test]
fn a_session_that_goes_leaves_the_file_leaf_focused_and_the_tab() {
    let project = Ulid::new();
    let [a, b] = [Ulid::new(), Ulid::new()];
    let one = wire(project, "one", 1.0, split("vertical", pane(a), file("x")));
    let two = wire(project, "two", 2.0, pane(b));
    let mut tabs = Tabs::new(project);
    tabs.load(vec![one.clone(), two]);
    assert!(tabs.focus(&k(a)));

    // `a` is unknown to the daemon now.
    let writes = tabs.prune(|id| id == b);
    assert_eq!(writes.len(), 1);
    assert_eq!((writes[0].tab_id, &writes[0].layout), (one.id, &file("x")));
    let tab = tabs.tab(one.id).unwrap();
    assert_eq!(tab.focused(), Some(&file_key("x")));
    assert!(tab.sessions().is_empty());
    assert_eq!(tabs.tabs().len(), 2, "the tab stays");
    assert!(tabs.prune(|id| id == b).is_empty());
}

#[test]
fn close_leaf_closes_any_kind_and_moves_the_focus() {
    let project = Ulid::new();
    let a = Ulid::new();
    let mut tabs = Tabs::new(project);
    tabs.load(vec![wire(
        project,
        "one",
        1.0,
        split("vertical", pane(a), split("horizontal", file("x"), json!({"diff": true}))),
    )]);
    assert!(tabs.focus(&file_key("x")));
    let write = tabs.close_leaf(&file_key("x")).unwrap();
    assert_eq!(
        write.layout,
        split("vertical", pane(a), json!({"diff": true}))
    );
    assert_eq!(tabs.active().and_then(Tab::focused), Some(&PaneKey::Diff));
    assert_eq!(tabs.close_leaf(&file_key("x")), None, "already closed");
    tabs.close_leaf(&PaneKey::Diff).unwrap();
    let write = tabs.close_leaf(&k(a)).unwrap();
    assert_eq!(write.layout, serde_json::Value::Null);
    assert_eq!(tabs.active().and_then(Tab::focused), None);
}

#[test]
fn move_pane_carries_the_whole_leaf() {
    let project = Ulid::new();
    let [a, c] = [Ulid::new(), Ulid::new()];
    let one = wire(project, "one", 1.0, split("vertical", pane(a), full_file()));
    let two = wire(project, "two", 2.0, pane(c));
    let mut tabs = Tabs::new(project);
    tabs.load(vec![one.clone(), two.clone()]);

    // To another tab, on the left of `c`: `md` and `preview` travel with the file.
    let writes = tabs
        .move_pane(&file_key("README.md"), &k(c), Edge::Left)
        .unwrap();
    assert_eq!(writes.len(), 2);
    assert_eq!((writes[0].tab_id, &writes[0].layout), (one.id, &pane(a)));
    assert_eq!(
        (writes[1].tab_id, &writes[1].layout),
        (two.id, &split("vertical", full_file(), pane(c)))
    );
    assert_eq!(tabs.active().map(|t| t.id), Some(two.id));
    assert_eq!(
        tabs.tab(two.id).and_then(Tab::focused),
        Some(&file_key("README.md"))
    );
    assert_eq!(
        tabs.tab_with(&file_key("README.md")).map(|t| t.id),
        Some(two.id)
    );
    // Onto itself or onto a leaf that is in no tab: nothing moves.
    assert_eq!(tabs.move_pane(&file_key("README.md"), &file_key("README.md"), Edge::Top), None);
    assert_eq!(tabs.move_pane(&file_key("README.md"), &file_key("nope"), Edge::Top), None);
    assert_eq!(tabs.move_pane(&file_key("nope"), &k(c), Edge::Top), None);
}

#[test]
fn move_pane_moves_the_diff_within_a_tab_to_each_edge() {
    let project = Ulid::new();
    let a = Ulid::new();
    let diff = || json!({"diff": true});
    let tab = wire(project, "one", 1.0, split("vertical", pane(a), diff()));
    let mut tabs = Tabs::new(project);
    tabs.load(vec![tab.clone()]);
    for (edge, expected) in [
        (Edge::Top, split("horizontal", diff(), pane(a))),
        (Edge::Bottom, split("horizontal", pane(a), diff())),
        (Edge::Left, split("vertical", diff(), pane(a))),
        (Edge::Right, split("vertical", pane(a), diff())),
    ] {
        let writes = tabs.move_pane(&PaneKey::Diff, &k(a), edge).unwrap();
        assert_eq!(writes.len(), 1, "one tab, one write");
        assert_eq!(writes[0].layout, expected, "{edge:?}");
        assert_eq!(tabs.active().and_then(Tab::focused), Some(&PaneKey::Diff));
    }
}

#[test]
fn merge_tab_carries_whole_leaves_to_any_edge() {
    let project = Ulid::new();
    let a = Ulid::new();
    let one = wire(project, "one", 1.0, pane(a));
    let moved = split("vertical", full_file(), json!({"diff": true}));
    let two = wire(project, "two", 2.0, moved.clone());
    let mut tabs = Tabs::new(project);
    tabs.load(vec![one.clone(), two.clone()]);

    let [placed, emptied] = tabs.merge_tab(two.id, &k(a), Edge::Top).unwrap();
    assert_eq!(placed.tab_id, one.id);
    assert_eq!(placed.layout, split("horizontal", moved, pane(a)));
    assert_eq!((emptied.tab_id, emptied.layout), (two.id, serde_json::Value::Null));
    assert_eq!(tabs.active().map(|t| t.id), Some(one.id));
    assert_eq!(
        tabs.tab(one.id).and_then(Tab::focused),
        Some(&file_key("README.md")),
        "the first moved leaf has the focus"
    );
    // The same tab, or an empty one: nothing moves.
    assert_eq!(tabs.merge_tab(one.id, &k(a), Edge::Right), None);
    assert_eq!(tabs.merge_tab(two.id, &k(a), Edge::Right), None);
}

#[test]
fn arrange_carries_whole_leaves() {
    let project = Ulid::new();
    let a = Ulid::new();
    let tab = wire(project, "one", 1.0, split("vertical", pane(a), full_file()));
    let mut tabs = Tabs::new(project);
    tabs.load(vec![tab.clone()]);
    let write = tabs.arrange(tab.id, Preset::Rows2, &[]).unwrap();
    assert_eq!(write.layout, split("horizontal", pane(a), full_file()));
    // Three leaves need Columns3 or MainAndStack; a new session joins after the file.
    let c = Ulid::new();
    tabs.arrange(tab.id, Preset::MainAndStack, &[c]).unwrap();
    let tab = tabs.tab(tab.id).unwrap();
    assert_eq!(tab.keys(), [k(a), file_key("README.md"), k(c)]);
    assert_eq!(
        tab.layout().and_then(|node| node.leaf_of(&file_key("README.md"))),
        Some(&Leaf::File {
            file: "README.md".into(),
            md: Some(mkrew_core::settings::MdMode::Split),
            preview: true,
        })
    );
}
```

### Step 1b.2: Implement `split.rs`

Header, lines 1-8, replace with:

```rust
//! Split tree of a terminal tab (SPEC §5 `terminal_tab.layout`, §10): leaves are sessions, files
//! or the diff, inner nodes split their area in two. Pure; serializes to exactly the §5 JSON
//! shape.

use std::collections::HashSet;

use serde::{Deserialize, Serialize};
use ulid::Ulid;
```

`Node` enum, lines 28-46, replace with:

```rust
/// A node of the split tree.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(untagged)]
pub enum Node {
    Leaf(Leaf),
    /// `ratio` is the share of the first child.
    Split {
        split: Axis,
        ratio: f64,
        children: Box<[Node; 2]>,
    },
}

/// The wire shape of a split, read from a `serde_json::Value` so one bad child does not lose
/// the whole layout ([`Node::from_json`]).
#[derive(Deserialize)]
struct SplitShape {
    split: Axis,
    ratio: f64,
    children: [serde_json::Value; 2],
}
```

Add to `impl Leaf` (from 1a) two fns after `key`:

```rust
    /// Whether this leaf is the one `key` names (no allocation, unlike `key()`).
    pub fn has_key(&self, key: &PaneKey) -> bool {
        match (self, key) {
            (Leaf::Session { session, .. }, PaneKey::Session(id)) => session == id,
            (Leaf::File { file, .. }, PaneKey::File(path)) => file == path,
            (Leaf::Diff { .. }, PaneKey::Diff) => true,
            _ => false,
        }
    }

    /// A stored leaf, or `None` (with a warning) when it is of a shape this version does not
    /// know or is broken by hand. `{"diff": false}` is never written, so it is broken too.
    fn from_json(value: &serde_json::Value) -> Option<Leaf> {
        match Leaf::deserialize(value) {
            Ok(Leaf::Diff { diff: false }) => {
                tracing::warn!(%value, "dropping a layout leaf: `diff` is not true");
                None
            }
            Ok(leaf) => Some(leaf),
            Err(error) => {
                tracing::warn!(%value, "dropping a layout leaf that does not parse: {error}");
                None
            }
        }
    }
```

`Preset::build` and `Preset::rects`, lines 131-165, replace with:

```rust
    /// The tree with `leaves` as its panes in tree order ([`Node::leaves`]: left to right,
    /// top to bottom within a column); `None` unless there are exactly [`Self::panes`] of them.
    pub fn build(self, leaves: &[Leaf]) -> Option<Node> {
        if leaves.len() != self.panes() {
            return None;
        }
        let pane = |at: usize| Node::leaf(leaves[at].clone());
        let split = |split: Axis, ratio: f64, first: Node, second: Node| Node::Split {
            split,
            ratio,
            children: Box::new([first, second]),
        };
        let (side, stack) = (Axis::Vertical, Axis::Horizontal);
        Some(match self {
            Preset::Columns2 => split(side, 0.5, pane(0), pane(1)),
            Preset::Rows2 => split(stack, 0.5, pane(0), pane(1)),
            Preset::Columns3 => split(side, 1.0 / 3.0, pane(0), split(side, 0.5, pane(1), pane(2))),
            Preset::MainAndStack => split(side, 0.5, pane(0), split(stack, 0.5, pane(1), pane(2))),
            Preset::Grid => split(
                side,
                0.5,
                split(stack, 0.5, pane(0), pane(1)),
                split(stack, 0.5, pane(2), pane(3)),
            ),
        })
    }

    /// Each pane's area (fractions of the tab), for drawing the layout in the menu.
    pub fn rects(self) -> Vec<Rect> {
        let leaves: Vec<Leaf> = (0..self.panes() as u128)
            .map(|n| Leaf::session(Ulid(n)))
            .collect();
        self.build(&leaves)
            .map(|node| node.rects().into_iter().map(|(_, rect)| rect).collect())
            .unwrap_or_default()
    }
```

`LayoutError` and `Closed`, lines 168-185, replace with:

```rust
#[derive(Debug, thiserror::Error)]
pub enum LayoutError {
    #[error("invalid layout JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("split ratio {0} is outside (0, 1)")]
    Ratio(f64),
    #[error("pane {0:?} appears more than once")]
    Duplicate(PaneKey),
}

/// Result of [`Node::close`].
#[derive(Debug, Clone, PartialEq)]
pub struct Closed {
    /// What is left of the tree; `None` when the closed pane was the last one.
    pub remaining: Option<Node>,
    /// The pane that took the closed pane's place, to focus next.
    pub focus: Option<PaneKey>,
}
```

`impl Node`, lines 196-581, replace the whole block with:

```rust
impl Node {
    pub fn pane(session: Ulid) -> Self {
        Node::Leaf(Leaf::session(session))
    }

    pub fn leaf(leaf: Leaf) -> Self {
        Node::Leaf(leaf)
    }

    /// Parses a stored layout; JSON `null` is a tab without panes.
    /// A leaf that does not parse (an unknown shape, a broken value) is dropped with a warning
    /// and a split with one dropped child becomes the other child, so one bad leaf never
    /// loses the rest of the tab (ADR 0108); a layout with no good leaf is an empty tab.
    /// `Err` only for a split that is not a split (not two children, bad axis or ratio type),
    /// a ratio outside (0, 1), and a key that appears twice.
    pub fn from_json(value: &serde_json::Value) -> Result<Option<Node>, LayoutError> {
        if value.is_null() {
            return Ok(None);
        }
        let Some(node) = Node::parse(value)? else {
            return Ok(None);
        };
        let mut seen = HashSet::new();
        for key in node.keys() {
            if seen.contains(&key) {
                return Err(LayoutError::Duplicate(key));
            }
            seen.insert(key);
        }
        Ok(Some(node))
    }

    fn parse(value: &serde_json::Value) -> Result<Option<Node>, LayoutError> {
        if value.get("split").is_none() && value.get("children").is_none() {
            return Ok(Leaf::from_json(value).map(Node::Leaf));
        }
        let shape = SplitShape::deserialize(value)?;
        if !(shape.ratio > 0.0 && shape.ratio < 1.0) {
            return Err(LayoutError::Ratio(shape.ratio));
        }
        let [first, second] = &shape.children;
        Ok(match (Node::parse(first)?, Node::parse(second)?) {
            (Some(first), Some(second)) => Some(Node::Split {
                split: shape.split,
                ratio: shape.ratio,
                children: Box::new([first, second]),
            }),
            (one, other) => one.or(other),
        })
    }

    /// The stored form of `layout` (`null` without panes).
    pub fn to_json(layout: Option<&Node>) -> serde_json::Value {
        // Serializing this enum cannot fail: no maps with non-string keys, ratios are finite.
        serde_json::to_value(layout).unwrap_or(serde_json::Value::Null)
    }

    /// Calls `visit` on every leaf in tree order.
    fn walk<'a>(&'a self, visit: &mut impl FnMut(&'a Leaf)) {
        match self {
            Node::Leaf(leaf) => visit(leaf),
            Node::Split { children, .. } => children.iter().for_each(|child| child.walk(visit)),
        }
    }

    /// Keys of all panes, left-to-right / top-to-bottom in tree order.
    pub fn keys(&self) -> Vec<PaneKey> {
        let mut out = Vec::new();
        self.walk(&mut |leaf| out.push(leaf.key()));
        out
    }

    /// Every leaf in tree order, whole: what a preset rebuilds the tree from.
    pub fn leaves(&self) -> Vec<Leaf> {
        let mut out = Vec::new();
        self.walk(&mut |leaf| out.push(leaf.clone()));
        out
    }

    /// Session ids of the session panes, in tree order.
    pub fn sessions(&self) -> Vec<Ulid> {
        let mut out = Vec::new();
        self.walk(&mut |leaf| {
            if let Leaf::Session { session, .. } = leaf {
                out.push(*session);
            }
        });
        out
    }

    /// Whether any leaf satisfies `test`.
    pub fn any_leaf(&self, test: impl Fn(&Leaf) -> bool) -> bool {
        let mut found = false;
        self.walk(&mut |leaf| found |= test(leaf));
        found
    }

    pub fn contains(&self, key: &PaneKey) -> bool {
        self.leaf_of(key).is_some()
    }

    pub fn leaf_of(&self, key: &PaneKey) -> Option<&Leaf> {
        match self {
            Node::Leaf(leaf) => leaf.has_key(key).then_some(leaf),
            Node::Split { children, .. } => children.iter().find_map(|child| child.leaf_of(key)),
        }
    }

    pub fn leaf_mut(&mut self, key: &PaneKey) -> Option<&mut Leaf> {
        match self {
            Node::Leaf(leaf) => leaf.has_key(key).then_some(leaf),
            Node::Split { children, .. } => {
                children.iter_mut().find_map(|child| child.leaf_mut(key))
            }
        }
    }

    /// The mode saved on the pane of `session`; `None` = not set (or no such pane).
    pub fn chat_of(&self, session: Ulid) -> Option<bool> {
        match self.leaf_of(&PaneKey::Session(session))? {
            Leaf::Session { chat, .. } => *chat,
            Leaf::File { .. } | Leaf::Diff { .. } => None,
        }
    }

    /// Saves `value` on the pane of `session`; false if it is not in the tree or already has it.
    pub fn set_chat(&mut self, session: Ulid, value: Option<bool>) -> bool {
        match self.leaf_mut(&PaneKey::Session(session)) {
            Some(Leaf::Session { chat, .. }) => {
                let changed = *chat != value;
                *chat = value;
                changed
            }
            _ => false,
        }
    }

    fn first(&self) -> PaneKey {
        match self {
            Node::Leaf(leaf) => leaf.key(),
            Node::Split { children, .. } => children[0].first(),
        }
    }

    fn last(&self) -> PaneKey {
        match self {
            Node::Leaf(leaf) => leaf.key(),
            Node::Split { children, .. } => children[1].last(),
        }
    }

    /// Splits the pane of `target` in half along `axis`; `new` (one pane or a whole layout)
    /// goes right of / below it. Returns false (and changes nothing) if `target` is not in the
    /// tree or a pane of `new` already is.
    pub fn split_with(&mut self, target: &PaneKey, axis: Axis, new: Node) -> bool {
        self.place(target, axis, false, new)
    }

    /// Splits the pane of `target` 50 / 50 and puts `new` on `edge`'s side of it: left or
    /// above (the first child) for `Left` and `Top`, right or below for `Right` and `Bottom`.
    /// Returns false (and changes nothing) if `target` is not in the tree or a pane of `new`
    /// already is.
    pub fn insert_at(&mut self, target: &PaneKey, edge: Edge, new: Node) -> bool {
        self.place(target, edge.axis(), edge.leads(), new)
    }

    fn place(&mut self, target: &PaneKey, axis: Axis, lead: bool, new: Node) -> bool {
        if !self.contains(target) || new.keys().iter().any(|key| self.contains(key)) {
            return false;
        }
        self.place_inner(target, axis, lead, &mut Some(new))
    }

    /// `new` is taken by the pane of `target`.
    fn place_inner(
        &mut self,
        target: &PaneKey,
        axis: Axis,
        lead: bool,
        new: &mut Option<Node>,
    ) -> bool {
        match self {
            Node::Leaf(leaf) if leaf.has_key(target) => {
                let Some(new) = new.take() else {
                    return false;
                };
                // The split keeps the leaf as it is: its own mode, Markdown mode and flags.
                let old = Node::Leaf(leaf.clone());
                let children = if lead { [new, old] } else { [old, new] };
                *self = Node::Split {
                    split: axis,
                    ratio: 0.5,
                    children: Box::new(children),
                };
                true
            }
            Node::Leaf(_) => false,
            Node::Split { children, .. } => children
                .iter_mut()
                .any(|child| child.place_inner(target, axis, lead, new)),
        }
    }

    /// Removes the pane of `target`; its sibling takes the parent split's place. Closing a
    /// pane that is not in the tree leaves it unchanged.
    pub fn close(self, target: &PaneKey) -> Closed {
        if !self.contains(target) {
            return Closed {
                remaining: Some(self),
                focus: None,
            };
        }
        match self {
            Node::Leaf(_) => Closed {
                remaining: None,
                focus: None,
            },
            Node::Split {
                split,
                ratio,
                children,
            } => {
                let [first, second] = *children;
                let in_first = first.contains(target);
                let (closed, keep) = if in_first {
                    (first.close(target), second)
                } else {
                    (second.close(target), first)
                };
                match closed.remaining {
                    // The sibling replaces the split; focus its pane nearest to the closed one.
                    None => Closed {
                        focus: Some(if in_first { keep.first() } else { keep.last() }),
                        remaining: Some(keep),
                    },
                    Some(rest) => Closed {
                        remaining: Some(Node::Split {
                            split,
                            ratio,
                            children: Box::new(if in_first { [rest, keep] } else { [keep, rest] }),
                        }),
                        focus: closed.focus,
                    },
                }
            }
        }
    }

    /// Moves the divider of the innermost split around `target` whose axis matches
    /// `direction` (left/right: side-by-side splits, up/down: stacked) by `step` that way,
    /// keeping both children at least [`MIN_RATIO`]. Returns false if nothing changed.
    pub fn resize(&mut self, target: &PaneKey, direction: Direction, step: f64) -> bool {
        let (axis, delta) = match direction {
            Direction::Left => (Axis::Vertical, -step),
            Direction::Right => (Axis::Vertical, step),
            Direction::Up => (Axis::Horizontal, -step),
            Direction::Down => (Axis::Horizontal, step),
        };
        self.resize_inner(target, axis, delta) == Some(true)
    }

    /// `None` = no matching split around `target` in this subtree (an outer one may match).
    fn resize_inner(&mut self, target: &PaneKey, axis: Axis, delta: f64) -> Option<bool> {
        let Node::Split {
            split,
            ratio,
            children,
        } = self
        else {
            return None;
        };
        let child = children.iter_mut().find(|c| c.contains(target))?;
        if let Some(done) = child.resize_inner(target, axis, delta) {
            return Some(done);
        }
        if *split != axis {
            return None;
        }
        // Rounded so repeated steps do not drift (0.44999…) and the stored JSON stays short.
        let next = ((*ratio + delta) * 1e4).round() / 1e4;
        let next = next.clamp(MIN_RATIO, 1.0 - MIN_RATIO);
        let changed = (next - *ratio).abs() > EPSILON;
        *ratio = next;
        Some(changed)
    }

    /// Gives every pane of a run of same-axis splits the same share (`A | (B | C)` becomes
    /// thirds); a nested split of the other axis counts as one pane. Returns false if no
    /// ratio changed.
    pub fn equalize(&mut self) -> bool {
        let Node::Split {
            split,
            ratio,
            children,
        } = self
        else {
            return false;
        };
        let [first, second] = &mut **children;
        let (a, b) = (first.slots(*split) as f64, second.slots(*split) as f64);
        let next = a / (a + b);
        let changed = (next - *ratio).abs() > EPSILON;
        *ratio = next;
        // `|`, not `||`: both children are equalized.
        changed | first.equalize() | second.equalize()
    }

    /// Panes side by side along `axis` within this node.
    fn slots(&self, axis: Axis) -> usize {
        match self {
            Node::Split {
                split, children, ..
            } if *split == axis => children.iter().map(|c| c.slots(axis)).sum(),
            _ => 1,
        }
    }

    /// Sets the ratio of the `index`-th split in tree order (see [`Self::dividers`]), kept
    /// within [`MIN_RATIO`]. Returns false if there is no such split or nothing changed.
    pub fn set_ratio(&mut self, index: usize, ratio: f64) -> bool {
        // A tab with no size yet gives NaN; a NaN ratio would break the stored layout.
        if !ratio.is_finite() {
            return false;
        }
        let next = ((ratio * 1e4).round() / 1e4).clamp(MIN_RATIO, 1.0 - MIN_RATIO);
        self.set_ratio_inner(&mut { index }, next) == Some(true)
    }

    /// `left` counts down the splits still to skip; `None` = not in this subtree.
    fn set_ratio_inner(&mut self, left: &mut usize, next: f64) -> Option<bool> {
        let Node::Split {
            ratio, children, ..
        } = self
        else {
            return None;
        };
        if *left == 0 {
            let changed = (next - *ratio).abs() > EPSILON;
            *ratio = next;
            return Some(changed);
        }
        *left -= 1;
        children
            .iter_mut()
            .find_map(|child| child.set_ratio_inner(left, next))
    }

    /// Every split with its area, in tree order.
    pub fn dividers(&self) -> Vec<Divider> {
        let mut out = Vec::new();
        self.dividers_inner(Rect::WHOLE, &mut out);
        out
    }

    fn dividers_inner(&self, area: Rect, out: &mut Vec<Divider>) {
        if let Node::Split {
            split,
            ratio,
            children,
        } = self
        {
            out.push(Divider {
                index: out.len(),
                axis: *split,
                area,
                ratio: *ratio,
            });
            let (a, b) = halves(*split, *ratio, area);
            children[0].dividers_inner(a, out);
            children[1].dividers_inner(b, out);
        }
    }

    /// Every pane with its area, in tree order.
    pub fn rects(&self) -> Vec<(PaneKey, Rect)> {
        let mut out = Vec::new();
        self.rects_inner(Rect::WHOLE, &mut out);
        out
    }

    fn rects_inner(&self, area: Rect, out: &mut Vec<(PaneKey, Rect)>) {
        match self {
            Node::Leaf(leaf) => out.push((leaf.key(), area)),
            Node::Split {
                split,
                ratio,
                children,
            } => {
                let (a, b) = halves(*split, *ratio, area);
                children[0].rects_inner(a, out);
                children[1].rects_inner(b, out);
            }
        }
    }

    /// The pane next to `from` in `direction`: the closest pane on that side whose span
    /// overlaps `from`'s, preferring the largest overlap, then the top / left one.
    pub fn neighbour(&self, from: &PaneKey, direction: Direction) -> Option<PaneKey> {
        let rects = self.rects();
        let (_, origin) = rects.iter().find(|(key, _)| key == from)?;
        let origin = *origin;
        rects
            .iter()
            .filter(|(key, _)| key != from)
            .filter_map(|(key, rect)| {
                // (gap to the candidate, overlap on the other axis, position on the other axis)
                let (gap, overlap, along) = match direction {
                    Direction::Right => (
                        rect.x - (origin.x + origin.w),
                        overlap(origin.y, origin.h, rect.y, rect.h),
                        rect.y,
                    ),
                    Direction::Left => (
                        origin.x - (rect.x + rect.w),
                        overlap(origin.y, origin.h, rect.y, rect.h),
                        rect.y,
                    ),
                    Direction::Down => (
                        rect.y - (origin.y + origin.h),
                        overlap(origin.x, origin.w, rect.x, rect.w),
                        rect.x,
                    ),
                    Direction::Up => (
                        origin.y - (rect.y + rect.h),
                        overlap(origin.x, origin.w, rect.x, rect.w),
                        rect.x,
                    ),
                };
                (gap > -EPSILON && overlap > EPSILON).then_some((key, gap, overlap, along))
            })
            .min_by(|a, b| {
                let gap = if (a.1 - b.1).abs() <= EPSILON {
                    std::cmp::Ordering::Equal
                } else {
                    a.1.total_cmp(&b.1)
                };
                gap.then(b.2.total_cmp(&a.2)).then(a.3.total_cmp(&b.3))
            })
            .map(|(key, ..)| key.clone())
    }
}
```

`drop_axis` (lines 593-601) stays as it is: Task 6 deletes it.

Notes on the code above:

- `Node::split(target: Ulid, axis, new: Ulid)` is gone: no caller outside the tests used it (the grep of `\.split\(` over `crates/mkrew-ui/src` finds only `str::split` and `stream.split()`), and the tests use the `split` helper from Step 1b.1.
- `Node` no longer derives `Deserialize`: its only reader was `from_json`, and `Node::parse` plays the "custom deserialize" role (the tree is read as `serde_json::Value`, leaf by leaf).
- `Leaf::from_json` is private; `Leaf` still derives `Deserialize` (untagged) so a single leaf reads with `Leaf::deserialize(&value)`.

### Step 1b.3: Implement `tabs.rs`

Imports, lines 5-14, replace with:

```rust
use std::collections::VecDeque;

use mkrew_protocol::{
    Event, Session, SessionKind, SessionState, TabMoveParams, TabRenameParams, TabSetLayoutParams,
    TerminalTab,
};
use ulid::Ulid;

use crate::board::View;
use crate::split::{
    Axis, Direction, Divider, Edge, Leaf, Node, PaneKey, Preset, RESIZE_STEP, Rect,
};
```

`Tab` struct fields, lines 28-31, replace with:

```rust
    /// Local only (not persisted): the pane keyboard input goes to.
    focused: Option<PaneKey>,
    /// Local only: the pane filling the whole tab (zoom); always the focused pane.
    zoomed: Option<PaneKey>,
```

`impl Tab`, lines 58-133 (from `pub fn layout` through the end of `write`), replace with:

```rust
    pub fn layout(&self) -> Option<&Node> {
        self.layout.as_ref()
    }

    pub fn focused(&self) -> Option<&PaneKey> {
        self.focused.as_ref()
    }

    pub fn zoomed(&self) -> Option<&PaneKey> {
        self.zoomed.as_ref()
    }

    /// The focused pane's session; `None` when a file or the diff has the focus.
    pub fn focused_session(&self) -> Option<Ulid> {
        self.focused.as_ref()?.session()
    }

    /// Every pane in tree order, files and the diff included.
    pub fn keys(&self) -> Vec<PaneKey> {
        self.layout.as_ref().map(Node::keys).unwrap_or_default()
    }

    /// The session panes only, in tree order.
    pub fn sessions(&self) -> Vec<Ulid> {
        self.layout.as_ref().map(Node::sessions).unwrap_or_default()
    }

    /// Panes on screen: the zoomed pane alone, else every pane.
    pub fn shown(&self) -> Vec<PaneKey> {
        match &self.zoomed {
            Some(key) => vec![key.clone()],
            None => self.keys(),
        }
    }

    /// Sessions on screen: the sessions among [`Self::shown`] (the ones to attach).
    pub fn shown_sessions(&self) -> Vec<Ulid> {
        self.shown().iter().filter_map(PaneKey::session).collect()
    }

    /// Area of each pane on screen: the zoomed pane fills the tab.
    pub fn rects(&self) -> Vec<(PaneKey, Rect)> {
        match (&self.zoomed, &self.layout) {
            (Some(key), _) => vec![(key.clone(), Rect::WHOLE)],
            (None, Some(node)) => node.rects(),
            (None, None) => Vec::new(),
        }
    }

    /// Dividers drawn over the panes; none while a pane is zoomed.
    pub fn dividers(&self) -> Vec<Divider> {
        match (&self.zoomed, &self.layout) {
            (None, Some(node)) => node.dividers(),
            _ => Vec::new(),
        }
    }

    pub fn contains(&self, key: &PaneKey) -> bool {
        self.layout.as_ref().is_some_and(|node| node.contains(key))
    }

    /// Keeps the focus on a pane of the tab: the first pane if it was lost. A zoom ends when
    /// the focus leaves the zoomed pane.
    fn fix_focus(&mut self) {
        if !self.focused.as_ref().is_some_and(|key| self.contains(key)) {
            self.focused = self.keys().into_iter().next();
        }
        if self.zoomed != self.focused {
            self.zoomed = None;
        }
    }

    /// Records a local layout change and returns the request persisting it.
    fn write(&mut self) -> TabSetLayoutParams {
        let layout = Node::to_json(self.layout.as_ref());
        self.pending.push_back(layout.clone());
        TabSetLayoutParams {
            tab_id: self.id,
            layout,
        }
    }
```

`Tabs::load`, lines 158-175: the two copied fields become clones (the rest unchanged):

```rust
                if let Some(previous) = old.iter().find(|old| old.id == tab.id) {
                    tab.focused = previous.focused.clone();
                    tab.zoomed = previous.zoomed.clone();
                    tab.fix_focus();
                }
```

`Tabs::tab_of`, lines 251-254, replace with these two fns (and keep `chat_of` / `set_chat`, which call `tab_of`, unchanged):

```rust
    /// The tab whose layout holds `session`.
    pub fn tab_of(&self, session: Ulid) -> Option<&Tab> {
        self.tab_with(&PaneKey::Session(session))
    }

    /// The tab whose layout holds `key`, of any kind. One workspace holds a file or the diff
    /// in at most one tab (ADR 0108), main window and detached windows alike: they all draw
    /// from this one `Tabs`.
    pub fn tab_with(&self, key: &PaneKey) -> Option<&Tab> {
        self.tabs.iter().find(|tab| tab.contains(key))
    }
```

`Tabs::visible`, lines 409-413, replace with:

```rust
    /// Sessions shown on screen: the session panes of the active tab (only the zoomed one
    /// while a pane is zoomed). All others stay detached.
    pub fn visible(&self) -> Vec<Ulid> {
        self.active()
            .map(Tab::shown_sessions)
            .unwrap_or_default()
    }
```

`Tabs::focus` and `Tabs::focus_pane`, lines 415-425 and 441-450:

```rust
    /// Activates the tab holding `key` and focuses its pane; false if no tab holds it.
    pub fn focus(&mut self, key: &PaneKey) -> bool {
        let Some(tab_id) = self.tab_with(key).map(|tab| tab.id) else {
            return false;
        };
        self.active = Some(tab_id);
        if let Some(tab) = self.tab_mut(tab_id) {
            tab.focused = Some(key.clone());
            tab.fix_focus();
        }
        true
    }
```

```rust
    /// Focuses `key`'s pane in `tab_id` without making that tab active.
    pub fn focus_pane(&mut self, tab_id: Ulid, key: &PaneKey) -> bool {
        let Some(tab) = self.tab_mut(tab_id).filter(|tab| tab.contains(key)) else {
            return false;
        };
        tab.focused = Some(key.clone());
        tab.fix_focus();
        true
    }
```

`Tabs::focus_neighbour`, lines 467-482, replace with:

```rust
    /// Moves the focus in the active tab (ending a zoom); false if there is no pane that way.
    pub fn focus_neighbour(&mut self, direction: Direction) -> bool {
        let Some(tab) = self.active_mut() else {
            return false;
        };
        let next = match (&tab.layout, &tab.focused) {
            (Some(node), Some(from)) => node.neighbour(from, direction),
            _ => None,
        };
        let Some(next) = next else {
            return false;
        };
        tab.focused = Some(next);
        tab.fix_focus();
        true
    }
```

`Tabs::toggle_zoom`, lines 485-499: change the two statements after the `zoomed.is_some()` early return:

```rust
        if tab.keys().len() < 2 {
            return false;
        }
        tab.zoomed = tab.focused.clone();
        tab.zoomed.is_some()
```

`Tabs::resize_focused`, lines 502-513, replace with:

```rust
    pub fn resize_focused(&mut self, direction: Direction) -> Option<TabSetLayoutParams> {
        let tab = self.active_mut()?;
        if tab.zoomed.is_some() {
            return None;
        }
        let focused = tab.focused.as_ref()?;
        if !tab.layout.as_mut()?.resize(focused, direction, RESIZE_STEP) {
            return None;
        }
        Some(tab.write())
    }
```

`Tabs::split_focused_chat`, lines 546-567 (the body only):

```rust
    pub fn split_focused_chat(
        &mut self,
        axis: Axis,
        new: Ulid,
        chat: Option<bool>,
    ) -> Option<TabSetLayoutParams> {
        if self.tab_of(new).is_some() {
            return None;
        }
        let tab = self.active_mut()?;
        let leaf = Node::Leaf(Leaf::Session { session: new, chat });
        match (&mut tab.layout, &tab.focused) {
            (Some(node), Some(target)) => {
                if !node.split_with(target, axis, leaf) {
                    return None;
                }
            }
            (layout @ None, _) => *layout = Some(leaf),
            // `fix_focus` keeps a pane focused whenever the tab has one.
            (Some(_), None) => return None,
        }
        tab.focused = Some(PaneKey::Session(new));
        tab.fix_focus();
        Some(tab.write())
    }
```

`Tabs::move_pane`, `merge_tab`, `arrange`, `close_pane`, `prune`, lines 596-737, replace all five (docs included) with:

```rust
    /// Moves the pane of `key` next to the pane of `target` on `edge` (the moved leaf takes that
    /// half of the target), within its tab or from another one. The whole leaf travels: a file
    /// keeps its Markdown mode and preview flag, a session its chat flag. The moved pane is
    /// focused and its new tab activated. Returns the writes, the source tab's first when it
    /// differs; `None` if nothing moves (the same pane, or either is in no tab).
    pub fn move_pane(
        &mut self,
        key: &PaneKey,
        target: &PaneKey,
        edge: Edge,
    ) -> Option<Vec<TabSetLayoutParams>> {
        if key == target {
            return None;
        }
        let from = self.tab_with(key)?.id;
        let to = self.tab_with(target)?.id;
        let leaf = self.tab(from)?.layout.as_ref()?.leaf_of(key)?.clone();
        let mut writes = Vec::new();
        let source = self.tab_mut(from)?;
        let closed = source.layout.take()?.close(key);
        source.layout = closed.remaining;
        if source.focused.as_ref() == Some(key) {
            source.focused = closed.focus;
        }
        source.fix_focus();
        if from != to {
            writes.push(source.write());
        }
        let tab = self.tab_mut(to)?;
        // `target` is still in the tab (it is not `key`), which no longer holds `key`.
        let placed = (tab.layout.as_mut())
            .is_some_and(|node| node.insert_at(target, edge, Node::Leaf(leaf)));
        debug_assert!(placed, "the moved pane always fits next to its target");
        tab.focused = Some(key.clone());
        tab.fix_focus();
        writes.push(tab.write());
        self.active = Some(to);
        Some(writes)
    }

    /// Moves every pane of tab `source` next to the pane of `target` in another tab (as
    /// [`Self::move_pane`]), keeping their layout; `source` is left without panes, for the
    /// caller to close once both writes are saved. The target tab is activated with the first
    /// moved pane focused. Returns the writes, the target tab's first; `None` if nothing moves
    /// (`target` is in `source` or in no tab, or `source` has no panes).
    pub fn merge_tab(
        &mut self,
        source: Ulid,
        target: &PaneKey,
        edge: Edge,
    ) -> Option<[TabSetLayoutParams; 2]> {
        let to = self.tab_with(target)?.id;
        if to == source {
            return None;
        }
        let moved = self.tab(source)?.layout.clone()?;
        let first = moved.keys().into_iter().next()?;
        let tab = self.tab_mut(to)?;
        if !tab.layout.as_mut()?.insert_at(target, edge, moved) {
            return None;
        }
        tab.focused = Some(first);
        tab.fix_focus();
        let placed = tab.write();
        let emptied = self.tab_mut(source)?;
        emptied.layout = None;
        emptied.fix_focus();
        let emptied = emptied.write();
        self.active = Some(to);
        Some([placed, emptied])
    }

    /// Lays tab `tab_id` out as `preset` (SPEC §10 layouts): its panes keep their tree
    /// order, whole (a file keeps its Markdown mode, a session its chat flag), then come the
    /// `new` sessions it does not show yet. `None` (nothing changes) unless that makes
    /// exactly the preset's panes, or when a new session is shown in another tab. A zoom
    /// ends; the focus stays.
    pub fn arrange(
        &mut self,
        tab_id: Ulid,
        preset: Preset,
        new: &[Ulid],
    ) -> Option<TabSetLayoutParams> {
        if new
            .iter()
            .any(|id| self.tab_of(*id).is_some_and(|tab| tab.id != tab_id))
        {
            return None;
        }
        let tab = self.tab_mut(tab_id)?;
        let mut leaves = tab.layout.as_ref().map(Node::leaves).unwrap_or_default();
        for id in new {
            let key = PaneKey::Session(*id);
            if !leaves.iter().any(|leaf| leaf.has_key(&key)) {
                leaves.push(Leaf::session(*id));
            }
        }
        tab.layout = Some(preset.build(&leaves)?);
        tab.zoomed = None;
        tab.fix_focus();
        Some(tab.write())
    }

    /// Removes the pane of `key` from whichever tab holds it; focus moves to the pane that
    /// took its place. `None` if no tab holds it. A tab left without panes stays (the caller
    /// closes it).
    pub fn close_leaf(&mut self, key: &PaneKey) -> Option<TabSetLayoutParams> {
        let tab_id = self.tab_with(key)?.id;
        let tab = self.tab_mut(tab_id)?;
        let closed = tab.layout.take()?.close(key);
        tab.layout = closed.remaining;
        if tab.focused.as_ref() == Some(key) {
            tab.focused = closed.focus;
        }
        tab.fix_focus();
        Some(tab.write())
    }

    /// Removes panes whose session `keep` rejects (e.g. sessions the daemon no longer knows)
    /// and returns the layout writes to send. Files and the diff are not sessions: they stay,
    /// and so does a tab that holds one.
    pub fn prune(&mut self, keep: impl Fn(Ulid) -> bool) -> Vec<TabSetLayoutParams> {
        let mut writes = Vec::new();
        for tab in &mut self.tabs {
            let gone: Vec<Ulid> = tab.sessions().into_iter().filter(|id| !keep(*id)).collect();
            if gone.is_empty() {
                continue;
            }
            for session in gone {
                tab.layout = tab
                    .layout
                    .take()
                    .and_then(|node| node.close(&PaneKey::Session(session)).remaining);
            }
            tab.fix_focus();
            writes.push(tab.write());
        }
        writes
    }
```

Free fns of `tabs.rs` (lines 760-955):

`newest_unplaced`, lines 762-764, replace the first guard with (a tab that holds a file or the diff is not empty):

```rust
pub fn newest_unplaced(tabs: &Tabs, sessions: &[Session]) -> Option<Ulid> {
    if tabs.active()?.layout().is_some() {
        return None;
    }
```

`tab_icons`, line 829 and the inactive branch (~858):

```rust
    let ids = tab.sessions();
```
(unchanged) and

```rust
        tab.focused_session().iter().filter_map(known).map(icon).collect()
```

`session_label` and `double_click_renames`:

```rust
pub fn session_label<'a>(tab: &Tab, sessions: &'a [Session]) -> Option<&'a str> {
    let focused = tab.focused_session().filter(|_| has_default_title(tab))?;
    sessions
        .iter()
        .find(|s| s.id == focused)
        .and_then(crate::history::title)
}
```

```rust
pub fn double_click_renames(tab: &Tab, sessions: &[Session]) -> Option<Ulid> {
    let focused = tab.focused_session().filter(|_| has_default_title(tab))?;
    let session = sessions.iter().find(|s| s.id == focused)?;
    mkrew_core::run::command_name(session)
        .is_none()
        .then_some(focused)
}
```

Update the doc comment of `session_label` so its first sentence says "the title of its focused session pane (a focused file or the diff shows its own title from Task 4 / 5 on)".

### Step 1b.4: Compile-fix the shell and the binary

Complete table of sites (line numbers are those of the worktree today; the compiler leads if a line moved).

| File:line | Old | New |
|---|---|---|
| `crates/mkrew-ui/src/main.rs:108` | `use mkrew_ui::split::{Axis, Direction, Divider, Preset, drop_axis};` | `use mkrew_ui::split::{Axis, Direction, Divider, Edge, PaneKey, Preset, drop_axis};` |
| `shell/chrome.rs:206` | `(tabs.active().and_then(Tab::focused))` | `(tabs.active().and_then(Tab::focused_session))` |
| `shell/focus.rs:184` | `tabs.active()?.focused()` | `tabs.active()?.focused_session()` |
| `shell/focus.rs:203` | `hosted.extend(shown.shown().into_iter().map(\|session\| (host, session)));` | `hosted.extend(shown.shown_sessions().into_iter().map(\|session\| (host, session)));` |
| `shell/hosts.rs:117-120` | `focused_session` body: `self.tabs.get(&project)?.tab(tab_id)?.focused()` | see the block below |
| `shell/hosts.rs:129-135` | `tabs.focus_pane(tab_id, session);` / `tabs.focus(session);` | see the block below |
| `shell/menus.rs:303` | `tab.sessions().len() > 1` | `tab.keys().len() > 1` |
| `shell/menus.rs:374` | `.map_or(0, \|tab\| tab.sessions().len());` | `.map_or(0, \|tab\| tab.keys().len());` |
| `shell/tabs.rs:95` (`apply_layout`) | `.map(\|tab\| tab.sessions().len())` | `.map(\|tab\| tab.keys().len())` |
| `shell/tabs.rs:816` (`remove_pane`) | `tabs.close_pane(session)?` | `tabs.close_leaf(&PaneKey::Session(session))?` |
| `shell/workspace.rs:316` | `tabs.focus_pane(tab_id, session);` | `tabs.focus_pane(tab_id, &PaneKey::Session(session));` |
| `shell/workspace.rs:329` | `if tabs.focus(session) {` | `if tabs.focus(&PaneKey::Session(session)) {` |
| `shell/pane_render.rs:514-525` (`render_tab`) | `tab.and_then(\|tab\| tab.focused())` ... `.map(\|(session, rect)\| {` ... `focused == Some(session)` | see the block below |
| `shell/pane_render.rs:773` | `tab.contains(*session)` | `tab.contains(&PaneKey::Session(*session))` |
| `shell/pane_drop.rs:68` | `.is_some_and(\|tab\| !tab.contains(session) && tab.layout().is_some())` | `.is_some_and(\|tab\| !tab.contains(&PaneKey::Session(session)) && tab.layout().is_some())` |
| `shell/pane_drop.rs:14-20` | `PaneDrop` struct | add `edge_of` after it, see below |
| `shell/pane_drop.rs:155-179` (`drop_pane`) | `move_pane(moved, target, axis)` | see below |
| `shell/pane_drop.rs:203` (`drop_tab_on_pane`) | `tabs.merge_tab(source, target, axis)` | `tabs.merge_tab(source, &PaneKey::Session(target), edge_of(axis))` |
| `crates/mkrew-ui/tests/lifecycle.rs:25` | `use mkrew_ui::split::Axis;` | `use mkrew_ui::split::{Axis, PaneKey};` |
| `crates/mkrew-ui/tests/lifecycle.rs:160` | `tabs.close_pane(b).unwrap()` | `tabs.close_leaf(&PaneKey::Session(b)).unwrap()` |

No change is needed at `shell/detached_actions.rs:149` and `shell/tabs.rs:665` (`.map(Tab::sessions)`, same signature), `shell/menus.rs:298` (`tab.zoomed().is_some()`), `shell/pane_render.rs:273` and `:471`, `shell/hosts.rs:53` and `:63`, `shell/chat_view.rs:115`, `shell/workspace.rs:334`, and `tests/restore.rs` (it only uses `visible`, `split_focused`, `Axis`).

**`shell/hosts.rs`**, replace `focused_session` (lines 117-120) with:

```rust
    /// The pane that has (or gets) the keyboard in `window`, of any kind.
    pub(crate) fn focused_key(&self, window: &Window) -> Option<PaneKey> {
        let (project, tab_id) = self.tab_scope(window)?;
        self.tabs.get(&project)?.tab(tab_id)?.focused().cloned()
    }

    /// The session pane that has (or gets) the keyboard in `window`: the focused pane when it
    /// is a session, `None` when a file or the diff has the focus.
    pub(crate) fn focused_session(&self, window: &Window) -> Option<Ulid> {
        self.focused_key(window)?.session()
    }
```

and `focus_session_pane` (lines 123-136) with:

```rust
    pub(crate) fn focus_session_pane(&mut self, session: Ulid) {
        let Some((project, tab_id)) = self.tab_of_session(session) else {
            return;
        };
        let detached = self.terminal_host(project, tab_id) != HostId::Main;
        let key = PaneKey::Session(session);
        if let Some(tabs) = self.tabs.get_mut(&project) {
            if detached {
                tabs.focus_pane(tab_id, &key);
            } else {
                tabs.focus(&key);
            }
        }
    }
```

**`shell/pane_render.rs`**, `render_tab` lines 514-525, replace with (a file or diff leaf is drawn from Task 4 / Task 5 on; until then such a leaf takes its place in the tree but draws nothing):

```rust
        let focused = tab.and_then(Tab::focused_session);
        let rects = tab.map(Tab::rects).unwrap_or_default();
        let dividers = tab.map(Tab::dividers).unwrap_or_default();
        let several = rects.len() > 1;
        let zoomed = tab.is_some_and(|tab| tab.zoomed().is_some());
        let half_gap = px(theme::PANE_HALF_GAP);
        // The panes' 0..1 space in px, as laid out in the last frame (`content_canvas`).
        let area = f32::from(content.get().width) - 2.0 * theme::PANE_HALF_GAP;
        let panes: Vec<_> = rects
            .into_iter()
            // File and diff panes are drawn from Task 4 / Task 5 on.
            .filter_map(|(key, rect)| Some((key.session()?, rect)))
            .map(|(session, rect)| {
                let is_focused = focused == Some(session);
```

**`shell/pane_drop.rs`**: after the `PaneDrop` struct (line 20) add:

```rust
/// The edge a drop that leans to `axis` lands on: right of the pane or below it. Task 6 reads
/// the edge from the pointer ([`Edge::nearest`]) and deletes this.
fn edge_of(axis: Axis) -> Edge {
    match axis {
        Axis::Vertical => Edge::Right,
        Axis::Horizontal => Edge::Bottom,
    }
}
```

In `drop_pane` replace the `writes` block (lines 168-173) with:

```rust
        let (moved, target) = (PaneKey::Session(moved), PaneKey::Session(target));
        let edge = edge_of(axis);
        let writes = (self.tabs.get_mut(&project_id)).and_then(|tabs| match detached {
            Some(tab_id) => {
                (tabs.with_active(tab_id, |tabs| tabs.move_pane(&moved, &target, edge))).flatten()
            }
            None => tabs.move_pane(&moved, &target, edge),
        });
```

(`moved` and `target` are used as `Ulid` before that block by `tab_of_session` and `terminal_host`; the shadowing starts after them.)

### Step 1b.5: Daemon and protocol are untouched here

No change in this sub-task: the daemon edits are in 1c.

### Step 1b.6: Compile-fix `tabs/tests.rs`

All line numbers are original (before the Step 1b.1 additions, which go at the top and the end; apply this table first, or find the lines by the quoted text).

Rules:

| Rule | Old | New |
|---|---|---|
| T1 | `Tab::focused` (path form) | `Tab::focused_session` |
| T2 | `x.focused()` on a `&Tab` compared with `Some(<Ulid>)` | `x.focused_session()` |
| T3 | `tabs.focus(X)` | `tabs.focus(&k(X))` |
| T4 | `tabs.close_pane(X)` | `tabs.close_leaf(&k(X))` |
| T5 | `tabs.focus_pane(T, X)` | `tabs.focus_pane(T, &k(X))` |
| T6 | `move_pane(X, Y, Axis::Horizontal)` | `move_pane(&k(X), &k(Y), Edge::Bottom)` |
| T7 | `move_pane(X, Y, Axis::Vertical)` | `move_pane(&k(X), &k(Y), Edge::Right)` |
| T8 | `merge_tab(S, X, Axis::Vertical)` | `merge_tab(S, &k(X), Edge::Right)` |
| T9 | `tab.contains(X)` | `tab.contains(&k(X))` |

| Lines | Rule and new text |
|---|---|
| 34, 117, 121, 295, 329, 1007, 1106 | T1 (`tabs.active().and_then(Tab::focused_session)`, and line 1106 `tabs.tab(two.id).and_then(Tab::focused_session)`) |
| 112, 118, 119, 122, 254, 271, 1182 | T4 (`tabs.close_leaf(&k(c))`, `&k(b)`, `&k(a)`, `&k(a)`, `&k(c)`, `&k(a)`, `&k(b)`) |
| 156 | T2: `assert_eq!(active.focused_session(), Some(b), "the focus stays");` |
| 157, 245, 255, 273, 1037 | `Tab::zoomed` compared with `None`: no change |
| 232 | `assert_eq!(tabs.active().and_then(Tab::zoomed), Some(&k(b)));` |
| 236 | `assert_eq!((rects[0].0.clone(), rects[0].1.w, rects[0].1.h), (k(b), 1.0, 1.0));` |
| 46, 144, 260, 262, 625, 628, 714, 857, 859, 861, 880 | T3 (`tabs.focus(&k(c))`, `&k(b)`, `&k(a)`, `&k(b)`, `&k(b)`, `&k(Ulid::new())`, `&k(a)`, `&k(named.id)`, `&k(untitled.id)`, `&k(titled.id)`, `&k(focus)`) |
| 968, 969 | T9 (`tab.contains(&k(new))`) |
| 981 | T6: `tabs.move_pane(&k(b), &k(c), Edge::Bottom)` |
| 1003 | T6: `tabs.move_pane(&k(a), &k(b), Edge::Bottom)` |
| 1010 | T7: `tabs.move_pane(&k(a), &k(c), Edge::Right)` |
| 1021-1023 | T7: `move_pane(&k(a), &k(a), Edge::Right)`, `move_pane(&k(Ulid::new()), &k(a), Edge::Right)`, `move_pane(&k(a), &k(Ulid::new()), Edge::Right)` |
| 1036 | T7: `tabs.move_pane(&k(c), &k(b), Edge::Right)` |
| 1052, 1053, 1055 | T8: `merge_tab(two.id, &k(b), Edge::Right)`, `merge_tab(empty.id, &k(a), Edge::Right)`, `merge_tab(two.id, &k(a), Edge::Right)` |
| 1104, 1107 | T5: `tabs.focus_pane(two.id, &k(c))`, `tabs.focus_pane(two.id, &k(a))` |
| 1176, 1199 | T6: `tabs.move_pane(&k(a), &k(b), Edge::Bottom)` |

Line 859 is `untitled` (spelled `untitled.id` in the file); line 857 is `named.id`; line 861 is `titled.id`.
Lines 33, 78, 110, 233, 240, 246, 252, 256, 261, 263, 320, 324, 328, 345, 362, 375, 1038 (`tabs.visible()`), 190 (`tabs.tab(one.id).unwrap().sessions()`), 1069 (`tab.sessions().is_empty()`), and every `arrange`, `prune`, `split_focused`, `with_active`, `toggle_zoom`, `focus_neighbour`, `tab_of`, `chat_of` call need no change.
After the edit, `cargo check -p mkrew-ui --all-targets -j 4` lists anything missed with the same rule.

### Step 1b.7: Update comments that name sessions as the only leaves

- `crates/mkrew-ui/src/tabs.rs:1-3` module doc: "each tab's split tree and focused pane" stays; no change needed beyond the code.
- `crates/mkrew-protocol/src/message.rs:639` moves to Step 1c.

### Step 1b.8: Format and compile

```
cargo fmt --all
cargo check -p mkrew-ui --all-targets -j 4
```

Expected: no errors.

### Step 1b.9: Run the tests

```
cargo test -p mkrew-ui --lib split -j 4
cargo test -p mkrew-ui --lib tabs -j 4
```

Expected: PASS.
The split tests include the new ones: `a_layout_with_all_three_leaf_kinds_round_trips`, `a_protocol_58_layout_reads_unchanged`, `leaves_that_do_not_parse_are_dropped_and_the_rest_loads`, `a_root_that_is_entirely_bad_loads_as_an_empty_tab`, `a_malformed_split_or_ratio_is_still_an_error`, `the_same_key_twice_in_a_layout_is_refused`, the four `insert_at_*` / `a_whole_layout_goes_in_on_the_leading_edge` tests, `file_and_diff_leaves_lay_out_close_move_and_resize_like_sessions`, `chat_flags_only_live_on_session_leaves`, `leaf_mut_changes_a_leaf_in_place`, `a_preset_builds_with_whole_leaves`.
The tabs tests include `a_file_leaf_is_a_pane_but_not_a_session`, `tab_with_finds_any_leaf_in_any_tab`, `a_tab_loads_around_a_leaf_it_cannot_read`, `a_session_that_goes_leaves_the_file_leaf_focused_and_the_tab`, `close_leaf_closes_any_kind_and_moves_the_focus`, `move_pane_carries_the_whole_leaf`, `move_pane_moves_the_diff_within_a_tab_to_each_edge`, `merge_tab_carries_whole_leaves_to_any_edge`, `arrange_carries_whole_leaves`.

If `a_tab_loads_around_a_leaf_it_cannot_read` fails on its last assert, check that `Tabs::load` (not `apply`) was used: `load` rebuilds the tab list.

### Step 1b.10: Lint the whole UI crate and the integration tests, then commit

Before the integration tests build the helper binaries:

```
cargo build -p mkrew-cli -p mkrew-fakeagent --bins -j 4
cargo clippy -p mkrew-ui --all-targets -j 4 -- -D warnings
cargo test -p mkrew-ui -j 4
```

Expected: clippy clean, all `mkrew-ui` tests (unit and `tests/lifecycle.rs`, `tests/restore.rs`) PASS.

```
git add crates/mkrew-ui/src/split.rs crates/mkrew-ui/src/tabs.rs crates/mkrew-ui/src/tabs/tests.rs crates/mkrew-ui/src/main.rs crates/mkrew-ui/src/shell/pane_drop.rs crates/mkrew-ui/src/shell/pane_render.rs crates/mkrew-ui/src/shell/hosts.rs crates/mkrew-ui/src/shell/focus.rs crates/mkrew-ui/src/shell/workspace.rs crates/mkrew-ui/src/shell/menus.rs crates/mkrew-ui/src/shell/chrome.rs crates/mkrew-ui/src/shell/tabs.rs crates/mkrew-ui/tests/lifecycle.rs
git -c user.email=tungnv975@gmail.com commit -m "refactor(ui): typed leaves in the split tree and the tabs"
```

---

#### Sub-task 1c: Tabs API for files and the diff, protocol 60, daemon tests

### Step 1c.1: Write the failing `Tabs` tests

Append to `crates/mkrew-ui/src/tabs/tests.rs`:

```rust
#[test]
fn insert_leaf_places_a_leaf_and_focuses_it() {
    let project = Ulid::new();
    let a = Ulid::new();
    let one = wire(project, "one", 1.0, pane(a));
    let empty = wire(project, "two", 2.0, serde_json::Value::Null);
    let mut tabs = Tabs::new(project);
    tabs.load(vec![one.clone(), empty.clone()]);

    // An empty tab takes the leaf as its root (`target` None).
    let write = tabs
        .insert_leaf(empty.id, None, Edge::Right, Leaf::file("src/a.rs"))
        .unwrap();
    assert_eq!((write.tab_id, &write.layout), (empty.id, &file("src/a.rs")));
    assert_eq!(
        tabs.tab(empty.id).and_then(Tab::focused),
        Some(&file_key("src/a.rs"))
    );
    assert_eq!(tabs.active().map(|t| t.id), Some(one.id), "not activated");

    // Next to a pane, on each side.
    let write = tabs
        .insert_leaf(one.id, Some(&k(a)), Edge::Left, Leaf::diff())
        .unwrap();
    assert_eq!(write.layout, split("vertical", json!({"diff": true}), pane(a)));
    assert_eq!(
        tabs.tab(one.id).and_then(Tab::focused),
        Some(&PaneKey::Diff)
    );
    let write = tabs
        .insert_leaf(one.id, Some(&k(a)), Edge::Bottom, Leaf::file("b.rs"))
        .unwrap();
    assert_eq!(
        write.layout,
        split(
            "vertical",
            json!({"diff": true}),
            split("horizontal", pane(a), file("b.rs"))
        )
    );
}

#[test]
fn insert_leaf_refuses_a_key_already_in_any_tab_of_the_workspace() {
    let project = Ulid::new();
    let [a, b] = [Ulid::new(), Ulid::new()];
    let one = wire(project, "one", 1.0, pane(a));
    let two = wire(project, "two", 2.0, split("vertical", pane(b), file("src/a.rs")));
    let mut tabs = Tabs::new(project);
    tabs.load(vec![one.clone(), two.clone()]);
    let before = tabs.clone();

    // The file is in the other tab, a session likewise: no second pane for the same key.
    assert_eq!(
        tabs.insert_leaf(one.id, Some(&k(a)), Edge::Right, Leaf::file("src/a.rs")),
        None
    );
    assert_eq!(
        tabs.insert_leaf(one.id, Some(&k(a)), Edge::Right, Leaf::session(b)),
        None
    );
    // The same file with other flags is the same key.
    let flagged = Leaf::File {
        file: "src/a.rs".into(),
        md: Some(mkrew_core::settings::MdMode::Source),
        preview: true,
    };
    assert_eq!(
        tabs.insert_leaf(one.id, Some(&k(a)), Edge::Right, flagged),
        None
    );
    // Unknown tab or target, a target in another tab, a root into a tab with panes.
    assert_eq!(
        tabs.insert_leaf(Ulid::new(), None, Edge::Right, Leaf::diff()),
        None
    );
    assert_eq!(
        tabs.insert_leaf(one.id, Some(&k(Ulid::new())), Edge::Right, Leaf::diff()),
        None
    );
    assert_eq!(
        tabs.insert_leaf(one.id, Some(&k(b)), Edge::Right, Leaf::diff()),
        None
    );
    assert_eq!(tabs.insert_leaf(one.id, None, Edge::Right, Leaf::diff()), None);
    assert_eq!(tabs, before, "nothing changed");

    // The diff is one per workspace too.
    tabs.insert_leaf(one.id, Some(&k(a)), Edge::Right, Leaf::diff())
        .unwrap();
    assert_eq!(
        tabs.insert_leaf(two.id, Some(&k(b)), Edge::Right, Leaf::diff()),
        None
    );
}

#[test]
fn update_leaf_writes_only_a_change_and_never_moves_the_key() {
    let project = Ulid::new();
    let a = Ulid::new();
    let tab = wire(project, "one", 1.0, split("vertical", pane(a), file("a.md")));
    let mut tabs = Tabs::new(project);
    tabs.load(vec![tab.clone()]);
    let key = file_key("a.md");
    let set_mode = |leaf: &mut Leaf| {
        if let Leaf::File { md, .. } = leaf {
            *md = Some(mkrew_core::settings::MdMode::Split);
        }
    };

    let write = tabs.update_leaf(&key, set_mode).unwrap();
    assert_eq!(
        write.layout,
        split("vertical", pane(a), json!({"file": "a.md", "md": "split"}))
    );
    // The same value again: nothing to write.
    assert_eq!(tabs.update_leaf(&key, set_mode), None);
    // Another change, then an unknown key.
    let write = tabs
        .update_leaf(&key, |leaf| {
            if let Leaf::File { preview, .. } = leaf {
                *preview = true;
            }
        })
        .unwrap();
    assert_eq!(
        write.layout["children"][1],
        json!({"file": "a.md", "md": "split", "preview": true})
    );
    assert_eq!(tabs.update_leaf(&file_key("nope"), set_mode), None);
    assert_eq!(tabs.update_leaf(&PaneKey::Diff, |_| {}), None);
    // The leaf is found by its key in the tab, whichever tab is active.
    let write = tabs
        .update_leaf(&k(a), |leaf| {
            if let Leaf::Session { chat, .. } = leaf {
                *chat = Some(true);
            }
        })
        .unwrap();
    assert_eq!(write.layout["children"][0]["chat"], json!(true));
}

#[test]
fn preview_tab_is_the_tab_holding_the_preview_file() {
    let project = Ulid::new();
    let [a, b] = [Ulid::new(), Ulid::new()];
    let one = wire(project, "one", 1.0, pane(a));
    let two = wire(
        project,
        "two",
        2.0,
        split("vertical", pane(b), json!({"file": "x.rs", "preview": true})),
    );
    let three = wire(project, "three", 3.0, file("y.rs"));
    let mut tabs = Tabs::new(project);
    tabs.load(vec![one, two.clone(), three]);
    assert_eq!(tabs.preview_tab().map(|t| t.id), Some(two.id));

    // Kept: the flag goes, so does the preview tab.
    tabs.update_leaf(&file_key("x.rs"), |leaf| {
        if let Leaf::File { preview, .. } = leaf {
            *preview = false;
        }
    })
    .unwrap();
    assert_eq!(tabs.preview_tab(), None);
    assert_eq!(Tabs::new(project).preview_tab(), None);
}

/// Tabs at the given `(position, pinned)` pairs, in the given order; returns their ids.
fn tabs_at(project: Ulid, places: &[(f64, bool)]) -> (Tabs, Vec<Ulid>) {
    let wires: Vec<TerminalTab> = places
        .iter()
        .map(|&(position, pinned)| TerminalTab {
            pinned,
            ..wire(project, "t", position, serde_json::Value::Null)
        })
        .collect();
    let ids = wires.iter().map(|wire| wire.id).collect();
    let mut tabs = Tabs::new(project);
    tabs.load(wires);
    (tabs, ids)
}

#[test]
fn position_after_active_is_the_midpoint_to_the_next_tab() {
    let (mut tabs, ids) = tabs_at(Ulid::new(), &[(1.0, false), (2.0, false), (4.0, false)]);
    assert!(tabs.activate(ids[0]));
    assert_eq!(tabs.position_after_active(), Some(1.5));
    assert!(tabs.activate(ids[1]));
    assert_eq!(tabs.position_after_active(), Some(3.0));
}

#[test]
fn position_after_active_of_the_last_tab_is_one_further() {
    let (mut tabs, ids) = tabs_at(Ulid::new(), &[(1.0, false), (2.0, false), (4.0, false)]);
    assert!(tabs.activate(ids[2]));
    assert_eq!(tabs.position_after_active(), Some(5.0));
    // No tabs, no position.
    assert_eq!(Tabs::new(Ulid::new()).position_after_active(), None);
}

#[test]
fn position_after_active_of_a_pinned_tab_goes_first_among_the_unpinned() {
    // Pinned tabs lead the strip: p at 5, q at 7; then the others at 1 and 2.
    let (mut tabs, ids) = tabs_at(
        Ulid::new(),
        &[(5.0, true), (7.0, true), (1.0, false), (2.0, false)],
    );
    for pinned in [ids[0], ids[1]] {
        assert!(tabs.activate(pinned));
        assert_eq!(
            tabs.position_after_active(),
            Some(0.0),
            "first of the unpinned tabs (1.0 - 1.0)"
        );
    }
    assert!(tabs.activate(ids[2]));
    assert_eq!(tabs.position_after_active(), Some(1.5));
    assert!(tabs.activate(ids[3]));
    assert_eq!(tabs.position_after_active(), Some(3.0));
    // Only pinned tabs: the new tab is the first unpinned one.
    let (mut tabs, ids) = tabs_at(Ulid::new(), &[(5.0, true), (7.0, true)]);
    assert!(tabs.activate(ids[1]));
    assert_eq!(tabs.position_after_active(), Some(1.0));
}
```

### Step 1c.2: Run the tests to see them fail

```
cargo test -p mkrew-ui --lib tabs -j 4
```

Expected: compile FAIL with `no method named `insert_leaf` found for struct `Tabs``, and the same for `update_leaf`, `preview_tab`, `position_after_active`.

### Step 1c.3: Implement the four fns

In `crates/mkrew-ui/src/tabs.rs`, add after `close_leaf` (and before `prune`):

```rust
    /// Places `leaf` in `tab_id` on `edge` of the pane of `target` (the new leaf takes that
    /// half), or, with no `target`, as the root of an empty tab, and focuses it. The tab is not
    /// activated. `None` (nothing changes) if the workspace already holds the leaf's key in
    /// any tab (a file or the diff has one pane per workspace, ADR 0108), the tab is unknown,
    /// `target` is not in that tab, or a root is placed in a tab that has panes.
    pub fn insert_leaf(
        &mut self,
        tab_id: Ulid,
        target: Option<&PaneKey>,
        edge: Edge,
        leaf: Leaf,
    ) -> Option<TabSetLayoutParams> {
        let key = leaf.key();
        if self.tab_with(&key).is_some() {
            return None;
        }
        let tab = self.tab_mut(tab_id)?;
        let node = Node::Leaf(leaf);
        match (target, &mut tab.layout) {
            (Some(target), Some(root)) => {
                if !root.insert_at(target, edge, node) {
                    return None;
                }
            }
            (None, layout @ None) => *layout = Some(node),
            _ => return None,
        }
        tab.focused = Some(key);
        tab.fix_focus();
        Some(tab.write())
    }

    /// Changes the leaf of `key` with `change` (a Markdown mode, a preview flag, a chat flag):
    /// the write to send, `None` when no tab holds it or `change` left it as it was. `change`
    /// must not change which pane the leaf is (its key).
    pub fn update_leaf(
        &mut self,
        key: &PaneKey,
        change: impl FnOnce(&mut Leaf),
    ) -> Option<TabSetLayoutParams> {
        let tab_id = self.tab_with(key)?.id;
        let tab = self.tab_mut(tab_id)?;
        let leaf = tab.layout.as_mut()?.leaf_mut(key)?;
        let before = leaf.clone();
        change(leaf);
        debug_assert!(leaf.has_key(key), "a leaf keeps its key");
        let changed = *leaf != before;
        changed.then(|| tab.write())
    }

    /// The tab that holds the workspace's preview file (the leaf with `preview` set), which
    /// the next single click in the Files tree replaces.
    pub fn preview_tab(&self) -> Option<&Tab> {
        self.tabs.iter().find(|tab| {
            (tab.layout.as_ref()).is_some_and(|node| {
                node.any_leaf(|leaf| matches!(leaf, Leaf::File { preview: true, .. }))
            })
        })
    }

    /// The `position` for a new (unpinned) tab that opens right after the active one: the
    /// midpoint between the active tab and the next tab (as [`Self::move_tab`] places a tab),
    /// or one past it when the active tab is last. While the active tab is pinned, the new
    /// tab goes first among the unpinned tabs: their first position - 1.0, or 1.0 when there
    /// is none. `None` without tabs.
    pub fn position_after_active(&self) -> Option<f64> {
        let active = self.active()?;
        if active.pinned {
            let first = self.tabs.iter().find(|tab| !tab.pinned);
            return Some(first.map_or(1.0, |tab| tab.position - 1.0));
        }
        let next = (self.tabs.iter())
            .skip_while(|tab| tab.id != active.id)
            .nth(1);
        Some(match next {
            Some(next) => (active.position + next.position) / 2.0,
            None => active.position + 1.0,
        })
    }
```

### Step 1c.4: Run the tests to see them pass

```
cargo test -p mkrew-ui --lib tabs -j 4
```

Expected: PASS.

### Step 1c.5: Protocol 60

`crates/mkrew-protocol/src/lib.rs:16`:

```rust
pub const PROTOCOL_VERSION: u32 = 60;
```

`crates/mkrew-protocol/src/message/tests.rs:1394-1397`, replace the test with:

```rust
#[test]
fn the_tern_panes_protocol_is_version_60() {
    assert_eq!(crate::PROTOCOL_VERSION, 60);
}
```

`crates/mkrew-protocol/src/message.rs:639`, replace the doc line:

```rust
    /// Split tree whose leaves are sessions, files or the diff (SPEC §5); opaque to the daemon.
```

(The `null until the first pane is placed` line below it stays.)

```
cargo test -p mkrew-protocol the_tern_panes_protocol -j 4
```

Expected: PASS (`1 passed`).
Every other protocol check reads the constant symbolically (`mkrew-daemon/tests/daemon/basics.rs:19,87`, the CLI and UI connection tests), so nothing else changes.
59 stays free for `feat/omp-tsp`; re-check both numbers at merge time (spec, Protocol).

### Step 1c.6: Daemon unit tests (they pass on the current code; they pin the behavior)

`cut` and `layout_sessions` already ignore every node without a `session` key, and `cut` collapses a split around any removed leaf, so no daemon logic changes; the tests below prove it for file and diff leaves.
`drop_panes` (the restart path of `resume.rs`) closes a tab only when `without_session` gives `Some(Value::Null)`; the tests assert the `without_session` output for a tab that keeps a file leaf, which is exactly that decision.

Append to `mod tests` of `crates/mkrew-daemon/src/tabs.rs` (after `a_surviving_pane_keeps_its_chat_flag`, before the closing `}` of the module):

```rust
    fn file(path: &str) -> Value {
        json!({"file": path})
    }

    fn diff() -> Value {
        json!({"diff": true})
    }

    fn split(axis: &str, first: Value, second: Value) -> Value {
        json!({"split": axis, "ratio": 0.5, "children": [first, second]})
    }

    #[test]
    fn layout_sessions_skips_file_and_diff_leaves() {
        let [a, b] = [Ulid::new(), Ulid::new()];
        let layout = split(
            "vertical",
            leaf(a),
            split("horizontal", file("src/a.rs"), split("vertical", diff(), leaf(b))),
        );
        assert_eq!(layout_sessions(&layout), [a, b]);
        assert!(layout_sessions(&file("src/a.rs")).is_empty());
        assert!(layout_sessions(&diff()).is_empty());
        // A tab of files only has no session for `tab.close` to remove.
        assert!(layout_sessions(&split("vertical", file("a"), diff())).is_empty());
        // A file named like a session id is still a file.
        assert!(layout_sessions(&file(&a.to_string())).is_empty());
    }

    #[test]
    fn cut_leaves_file_and_diff_leaves_alone() {
        let a = Ulid::new();
        assert_eq!(without_session(&file("src/a.rs"), a), None);
        assert_eq!(without_session(&diff(), a), None);
        assert_eq!(
            without_session(&split("vertical", file("src/a.rs"), diff()), a),
            None
        );
        // A file leaf with the id as its path is not the session.
        assert_eq!(without_session(&file(&a.to_string()), a), None);
    }

    #[test]
    fn cut_collapses_a_split_around_a_leaf_of_any_kind() {
        let a = Ulid::new();
        // The sibling is a file or the diff: it becomes the whole layout, from either side.
        assert_eq!(
            without_session(&split("vertical", leaf(a), file("x")), a),
            Some(file("x"))
        );
        assert_eq!(
            without_session(&split("vertical", diff(), leaf(a)), a),
            Some(diff())
        );
        // Deeper: the inner split goes, the outer ratio stays.
        let nested = json!({"split": "vertical", "ratio": 0.3, "children": [
            split("horizontal", leaf(a), file("x")),
            diff()
        ]});
        assert_eq!(
            without_session(&nested, a),
            Some(json!({"split": "vertical", "ratio": 0.3, "children": [file("x"), diff()]}))
        );
        // The session beside a whole subtree of files: the subtree takes the tab.
        let beside = split("vertical", leaf(a), split("horizontal", file("x"), diff()));
        assert_eq!(
            without_session(&beside, a),
            Some(split("horizontal", file("x"), diff()))
        );
        // A file leaf keeps its own keys.
        let full = json!({"file": "README.md", "md": "split", "preview": true});
        assert_eq!(
            without_session(&split("vertical", leaf(a), full.clone()), a),
            Some(full)
        );
    }

    #[test]
    fn a_tab_that_loses_its_last_session_keeps_its_file_leaf() {
        // `drop_panes` closes a tab only when the cut layout is `null`: a tab that still holds
        // a file or the diff is written back with that leaf as its root, and stays.
        let a = Ulid::new();
        let cut = without_session(&split("vertical", leaf(a), file("x")), a);
        assert_eq!(cut, Some(file("x")));
        assert_ne!(cut, Some(Value::Null));
        let cut = without_session(&split("vertical", diff(), leaf(a)), a);
        assert_eq!(cut, Some(diff()));
        // What is left has no session: `tab.close` of it removes none.
        assert!(layout_sessions(&cut.unwrap()).is_empty());
        // Only a tab whose last leaf was the session ends up empty (and closes unless pinned).
        assert_eq!(without_session(&leaf(a), a), Some(Value::Null));
    }
```

Update the comments of the same file (lines are those of today):

- Lines 1-2: replace with `//! Edits of tab layouts the daemon makes itself (SPEC §5: the layout is the UI's split tree,` / `//! stored as opaque JSON; only session leaves and the splits around them are touched here, file and diff leaves pass through).`
- Line 18 (before `fn cut`): add the doc line `/// Removes the pane of \`session\`; leaves without a \`session\` key (files, the diff) never match, and a split around the removed pane collapses to its sibling of any kind.`
- Line 53: replace the doc with `/// Every session with a pane in \`layout\`: the leaves with a \`session\` key; file and diff leaves are skipped.`

```
cargo test -p mkrew-daemon --lib tabs -j 4
```

Expected: PASS (`7 passed`: the three older tests plus `layout_sessions_skips_file_and_diff_leaves`, `cut_leaves_file_and_diff_leaves_alone`, `cut_collapses_a_split_around_a_leaf_of_any_kind`, `a_tab_that_loses_its_last_session_keeps_its_file_leaf`).

### Step 1c.6b: Daemon e2e test: `tab.close` removes only the sessions of the layout

The test goes in `crates/mkrew-daemon/tests/daemon/tabs_tasks.rs`, which already holds the tab tests and uses `use super::*;` for the helpers of `tests/daemon/main.rs`: `Env::new()`, `env.start()`, `client.add_project`, `client.call(..).into_result()`, `create_tab`, `spawn_params`, `set_default_shell`, `list_tabs`.
A session spawned with `tab_id` into an empty tab is placed there by the daemon, so the test only rewrites the layout with `tab.set_layout` (the daemon stores it as opaque JSON).
Append it after `restart_clears_the_panes_it_cannot_bring_back` (before `tasks_crud_and_order_persist_across_daemon_restart`):

```rust
/// `tab.close` removes the sessions found in the tab's layout and only those (ADR 0041, amended
/// by ADR 0108): file and diff leaves hold no session, and a session outside the layout stays.
#[test]
fn closing_a_tab_removes_only_the_sessions_of_its_layout() {
    let env = Env::new();
    let (_daemon, mut client) = env.start();
    let project = client.add_project(&env.repo());
    client
        .call(set_default_shell(&project, Some("shell")))
        .into_result::<Project>()
        .unwrap();
    let spawn_in = |client: &mut Client, tab: &TerminalTab| -> Ulid {
        let spawned: SessionSpawnResult = client
            .call(spawn_params(&project, SessionKind::Shell, Some(tab.id)))
            .into_result()
            .unwrap();
        spawned.session_id
    };
    let set_layout = |client: &mut Client, tab: &TerminalTab, layout: serde_json::Value| {
        client
            .call(Request::TabSetLayout(TabSetLayoutParams {
                tab_id: tab.id,
                layout,
            }))
            .into_result::<()>()
            .unwrap();
    };
    let sessions = |client: &mut Client| -> Vec<Ulid> {
        client
            .call(Request::SessionList(SessionListParams {
                project_id: Some(project.id),
            }))
            .into_result::<Vec<Session>>()
            .unwrap()
            .into_iter()
            .map(|s| s.id)
            .collect()
    };
    let close = |client: &mut Client, tab: &TerminalTab| {
        client
            .call(Request::TabClose(TabRef { tab_id: tab.id }))
            .into_result::<()>()
            .unwrap();
    };
    let new_tab = |client: &mut Client, title: &str| -> TerminalTab {
        client
            .call(create_tab(&project, title))
            .into_result()
            .unwrap()
    };

    // A bystander in its own tab, a session in a tab with a file, and a tab of files only.
    let other = new_tab(&mut client, "other");
    let bystander = spawn_in(&mut client, &other);
    let mixed = new_tab(&mut client, "mixed");
    let doomed = spawn_in(&mut client, &mixed);
    set_layout(
        &mut client,
        &mixed,
        serde_json::json!({"split": "vertical", "ratio": 0.5, "children": [
            {"session": doomed.to_string()},
            {"file": "src/a.rs", "md": "split", "preview": true}
        ]}),
    );
    let files = new_tab(&mut client, "files");
    set_layout(
        &mut client,
        &files,
        serde_json::json!({"split": "horizontal", "ratio": 0.5, "children": [
            {"file": "README.md"},
            {"diff": true}
        ]}),
    );
    assert!(sessions(&mut client).contains(&doomed));

    // Only file and diff leaves: no session goes, the others stay.
    close(&mut client, &files);
    let left = sessions(&mut client);
    assert!(left.contains(&bystander) && left.contains(&doomed), "{left:?}");

    // [session, file]: that session goes, nothing else.
    close(&mut client, &mixed);
    let left = sessions(&mut client);
    assert!(!left.contains(&doomed), "{left:?}");
    assert!(left.contains(&bystander), "{left:?}");
    let titles: Vec<String> = list_tabs(&mut client, &project)
        .into_iter()
        .map(|t| t.title)
        .collect();
    assert_eq!(titles, ["other"]);
    client.stop();
}
```

If `Client` is not in scope in this file under that name, use the type the existing closure `shell_in` of `restart_clears_the_panes_it_cannot_bring_back` takes (`&mut Client`); it is the same file and works there.
Run (build the binaries the daemon tests spawn first):

```
cargo build -p mkrew-cli -p mkrew-fakeagent --bins -j 4
cargo test -p mkrew-daemon --test daemon closing_a_tab_removes_only_the_sessions_of_its_layout -j 2
```

Expected: PASS (`1 passed`).
The test passes on the current daemon code (it pins the behavior, like the unit tests of Step 1c.6): `server.rs` closes a tab by calling `layout_sessions(&tab.layout)`, which skips leaves without a `session` key.

### Step 1c.7: Format, lint, run the affected suites, commit

```
cargo fmt --all
cargo build -p mkrew-cli -p mkrew-fakeagent --bins -j 4
cargo clippy -p mkrew-ui -p mkrew-protocol -p mkrew-daemon --all-targets -j 4 -- -D warnings
cargo test -p mkrew-ui -j 4
cargo test -p mkrew-protocol -j 4
cargo test -p mkrew-daemon -j 4
```

Expected: clippy clean; all PASS.

```
git add crates/mkrew-ui/src/tabs.rs crates/mkrew-ui/src/tabs/tests.rs crates/mkrew-protocol/src/lib.rs crates/mkrew-protocol/src/message.rs crates/mkrew-protocol/src/message/tests.rs crates/mkrew-daemon/src/tabs.rs crates/mkrew-daemon/tests/daemon/tabs_tasks.rs
git -c user.email=tungnv975@gmail.com commit -m "feat(ui): leaf API on the tabs, protocol 60, daemon skips file and diff leaves"
```

---

#### Additions to the contract (what Task 3 to 8 can rely on, and why)

| Addition | Why |
|---|---|
| `Node::leaves(&self) -> Vec<Leaf>` | `arrange` and `Preset::build(&[Leaf])` need whole leaves in tree order. |
| `Node::any_leaf(&self, impl Fn(&Leaf) -> bool) -> bool` | `Tabs::preview_tab` without cloning leaves. |
| `Leaf::has_key(&self, &PaneKey) -> bool` | Key comparison without the `String` clone of `key()`. |
| `Node::from_json` is the only reader; `Node` has no `Deserialize` impl | Its only user was `from_json`; `Node::parse` reads the tree as `serde_json::Value`, leaf by leaf. |
| `Node::split(target, axis, new: Ulid)` removed | No caller outside the old tests. |
| `Tabs::close_pane(session)` removed | Migrated to `close_leaf(&PaneKey::Session(..))` (callers: `shell/tabs.rs`, `tests/lifecycle.rs`). |
| `Tabs::visible()` unchanged (sessions on screen) | Attach logic, `on_screen`, `newest_unplaced` and the integration tests keep working. |
| `Tab::focused_session()` | Most old `Tab::focused` callers want a session. |

#### Deviations from the contract and spec gaps found while drafting

1. `Node::from_json` returns `Err(LayoutError::Json)` for a split that is not a split (not two children, unknown axis, ratio that is not a number), not only for ratio and duplicates.
   Reason: such a node has no leaf to keep, and the old test `invalid_layouts_are_rejected` pins the three-children case.
   Everything below the leaf level is lenient as the spec says.
2. The daemon needs no code change: `cut` and `layout_sessions` already skip leaves without a `session` key and `cut` already collapses any split around the removed leaf.
   Task 1 adds unit tests, one e2e test (`closing_a_tab_removes_only_the_sessions_of_its_layout`, Step 1c.6b) and fixes the comments.
3. Plan owner decision, `position_after_active` and pinned tabs: positions live per pin group, but a new tab is created unpinned.
   When the active tab is pinned, the function returns a position that puts the new unpinned tab first among the unpinned tabs: the first unpinned tab's position - 1.0, or 1.0 when there is no unpinned tab.
   When the active tab is unpinned it is the midpoint to the next unpinned tab, or active + 1.0 if last.
4. `newest_unplaced` now treats an active tab that holds only a file or the diff as not empty (it checks `layout().is_some()`), so a session is not auto-placed into it.
5. Until Task 4 and Task 5 draw them, a file or diff leaf takes its place in the tree but draws nothing (`render_tab` skips non-session keys), and `close_focused_pane` does nothing while one is focused.
   Nothing in the app creates such a leaf before Task 4, so this only shows with a hand-edited or newer layout.
6. `menus.rs` and `apply_layout` count `tab.keys().len()` (all panes), because presets arrange all leaves; `toggle_zoom` likewise needs two panes of any kind.

---

### Task 2: Pane look (frame color, 8 px gap, top stripe, no pane border)

Line numbers are those of the worktree at base master 0a8d404, before Task 1.
Task 1 runs first and shifts `pane_render.rs` (its `render_tab` and `pane_frame` take keys), so find every place by the quoted text, not by number.
At this point `pane_frame` still takes `session: Ulid` (Task 4 generalizes it to a `PaneKey`), so the stripe code below reads `info` (the session) exactly as the session color stripe does today.
Pure logic (`status::pane_stripe`, the theme derivation, the contrast rule) is unit tested; the frame, the gap and the title bar are gpui drawing and are checked by the smoke in Task 9.

**Files:**

- Modify `crates/mkrew-ui/src/theme.rs`: `PANE_HALF_GAP` (230-232), `PANE_ACCENT_WIDTH` doc (234-236), new `PANE_GAP_MIN_CONTRAST`, struct fields `chrome` doc (778) and `pane_focus_border` (837-838), `Theme::dark` (1058), `Theme::light` (1158), `classic_dark` (1257) and `classic_light` (1329) (renames only; they spread `..base.clone()` so they inherit `pane_gap`), `shared()` (1410), `from_palette` (1642-1673, 1696), `problems()` (2536-2551), tests (after 2612, and the broken-token cases at 2750-2777).
- Modify `crates/mkrew-ui/src/snapshots/mkrew_ui__theme__tests__mkrew_dark.snap` and `crates/mkrew-ui/src/snapshots/mkrew_ui__theme__tests__mkrew_light.snap`: one added line and one renamed line each, twice per file (top level and inside `classic`).
- Modify `crates/mkrew-ui/src/status.rs`: `pane_border` (115-132) becomes `pane_stripe`; its test (244-264) is rewritten.
- Modify `crates/mkrew-ui/src/pressable.rs` (201): the hover test also checks `pane_gap`.
- Modify `crates/mkrew-ui/src/shell/pane_render.rs`: `render_tab` (empty branch 471-513 and the panes area 552-557), `pane_frame` (589-746), new `corner_masks`, rename at 436.
- Modify `crates/mkrew-ui/src/shell/chrome.rs` (345-347): the title bar fill. `render_title_bar` serves the main and the detached windows, so there is no second bar to change.
- Modify `crates/mkrew-ui/src/shell/theme_gallery.rs` (409): the gallery picture's top strip.
- Rename-only edits (table in Step 1): `preview.rs`, `shell/{cards,comments,composer,files_render,git_diff,jira,run,send_dialog,slash}.rs`, `terminal/view.rs`.
- No change, checked: `PANE_HALF_GAP` has no user besides `render_tab` (`pane_render.rs:519,521,526`), and the width math there (`area = content.width - 2 * PANE_HALF_GAP`, `width = rect.w * area - 2 * PANE_HALF_GAP`) stays right at 4.
  The divider grip (`GRIP_WIDTH` 6) still fits inside the new 8 px gap.
  `classic_dark` / `classic_light` get no `pane_gap` line: both end in `..base.clone()`, and the sidebar's classic palette never draws the frame.
  `render_detached_tab_area` and the detached window root (`detached_render.rs:83`, `bg(theme.bg)`) need nothing: the bar is `render_title_bar` and the body is `render_tab`, which paints the frame color itself.
  The "every tab of this workspace is in another window" empty state in `chrome.rs` keeps its look (it is a message, not a frame).

**Interfaces:**

Consumes: `theme::{contrast, mix, Palette, THEMES, ThemeEntry}`, `status::SessionState`, `Tab::rects`, `Tab::focused` (as Task 1 leaves them).

Produces:

```rust
// crates/mkrew-ui/src/theme.rs
pub const PANE_HALF_GAP: f32 = space::XS;            // was space::XS / 2.0
pub const PANE_GAP_MIN_CONTRAST: f32 = 1.06;
pub struct Theme { /* .. */ pub pane_gap: u32, pub pane_focus_stripe: u32 /* renamed from pane_focus_border */, /* .. */ }

// crates/mkrew-ui/src/status.rs
pub fn pane_stripe(
    theme: &Theme,
    state: Option<SessionState>,
    color: Option<u32>,
    several: bool,
    focused: bool,
) -> Option<u32>;                                     // replaces pane_border(theme, state, several, focused)
```

Removed: `status::pane_border`, `Theme::pane_focus_border`.

**Decisions fixed by this draft (the spec leaves them open):**

- The numbers.
  Contrast is the WCAG ratio of `theme::contrast`, the same formula the unit test uses (checked with a script that mirrors it).

| Theme | `terminal_bg` | `pane_gap` | Ratio `pane_gap` on `terminal_bg` | How |
|---|---|---|---|---|
| Mkrew Dark | `0x0B0C0E` | `0x000000` | 1.0732 | Literal: black leaves the room the formula would not (`mix` to black by 70 % of `0B0C0E` is 1.02). Tern measures 1.07. |
| Mkrew Light | `0xFCFCFD` | `0xF3F4F6` | 1.0733 | Literal, a step darker. It stays 1.115 from `tab_active` (`E6E8EC`, rule "active tab" needs 1.1) and 1.037 from `hover` (`EEF0F3`), so a hovered chip still shows on the bar. |

- The derivation in `Theme::from_palette` (`terminal_bg` is `p.bg` there):
  dark palette: `mix(p.bg, 0x000000, 0.70)`; light palette: `mix(p.bg, 0x000000, 0.065)`;
  if that is under `PANE_GAP_MIN_CONTRAST` against `p.bg` (a bg that is already almost black), use `mix(p.bg, p.fg, 0.10)` instead (lighter, as the spec says).
  Resulting values (all pass 1.06; the smallest is GitHub Dark at 1.077):

| Dark theme | `pane_gap` | Ratio | Light theme | `pane_gap` | Ratio |
|---|---|---|---|---|---|
| One Dark | `0C0D10` | 1.388 | One Light | `EAEAEA` | 1.153 |
| Dracula | `0C0D10` | 1.365 | GitHub Light | `EEEEEE` | 1.160 |
| Catppuccin Mocha | `09090E` | 1.211 | Catppuccin Latte | `DFE1E5` | 1.158 |
| Tokyo Night | `08080B` | 1.170 | Solarized Light | `EDE6D4` | 1.154 |
| Gruvbox Dark | `0C0C0C` | 1.327 | Gruvbox Light | `EBE1BA` | 1.156 |
| Nord | `0E1013` | 1.526 | Rosé Pine Dawn | `EAE4DE` | 1.155 |
| Solarized Dark | `000D10` | 1.313 | Everforest Light | `EDE6D4` | 1.154 |
| GitHub Dark | `040507` | 1.077 | Ayu Light | `ECECEC` | 1.151 |
| Rosé Pine | `08070B` | 1.138 | Tokyo Night Day | `E1E2E5` | 1.157 |
| Kanagawa | `09090C` | 1.217 | | | |
| Everforest Dark | `0E1012` | 1.529 | | | |

  Why 0.065 and not the more obvious 0.05 or 0.08: at 0.05 GitHub Light's gap equals its `hover` (`F2F2F2`), so a hovered tab chip would vanish on the bar; at 0.08 five light themes fail "limits chip", "active tab" text or the toast rules on the bar.
  The band 0.06 to 0.07 passes every contrast rule; 0.065 also keeps `hover` visibly off the gap in all nine light themes (least 1.036).
- The title bar is `pane_gap` now, so three `problems()` rules that name the bar move from `chrome` to `pane_gap`: "limits chip", "active tab" and "brand logo".
  In `from_palette`, `tab_active` is `mix(pane_gap, p.fg, 0.12)` (it was `mix(p.chrome, p.fg, 0.12)`), because the chip sits on the bar; with the old formula Ayu Light fails "active tab" (1.079 against the 1.1 floor).
  `chrome` stays `p.chrome` and keeps its other users: `segmented`, the search box and the dropdown fills in `widgets.rs`, and the sidebar in derived themes.
- The title bar loses its 1 px bottom border (`border_b` + `divider`): a hairline between a bar and a frame of the same color would break "the bar and the frame read as one surface" (spec 3.1).
  This is the one visual choice beyond the spec text; it is reported to the plan owner.
- gpui here clips children to a rectangle only (`Style::overflow_mask` has no corner radii), and the terminal paints a square quad, so today's rounded look came only from the border ring drawn above it.
  With the ring gone, `corner_masks` paints the four corner slivers in `pane_gap` so the pane keeps its `radius::MEDIUM` corners (spec 3.2).

- [ ] **Step 1: Rename `pane_focus_border` to `pane_focus_stripe` at every site**

Mechanical, behavior-neutral; the token keeps its values and its non-pane callers.
Replace every occurrence of the identifier `pane_focus_border` with `pane_focus_stripe` (editor replace-all per file; do not touch `docs/`: older plans and ADRs are history, and Task 9 edits `SPEC.md`).
Do `status.rs` here as a pure rename too; Step 4 rewrites that function.

| File | Line (old text) | New text |
|---|---|---|
| `crates/mkrew-ui/src/theme.rs` | 838 `pub pane_focus_border: u32,` | `pub pane_focus_stripe: u32,` |
| `crates/mkrew-ui/src/theme.rs` | 1058 (`Theme::dark`) `pane_focus_border: 0x4A528E,` | `pane_focus_stripe: 0x4A528E,` |
| `crates/mkrew-ui/src/theme.rs` | 1158 (`Theme::light`) `pane_focus_border: 0xA7ADEA,` | `pane_focus_stripe: 0xA7ADEA,` |
| `crates/mkrew-ui/src/theme.rs` | 1257 (`classic_dark`) `pane_focus_border: 0x4A528E,` | `pane_focus_stripe: 0x4A528E,` |
| `crates/mkrew-ui/src/theme.rs` | 1329 (`classic_light`) `pane_focus_border: 0xA7ADEA,` | `pane_focus_stripe: 0xA7ADEA,` |
| `crates/mkrew-ui/src/theme.rs` | 1410 (`shared`) `pane_focus_border: 0,` | `pane_focus_stripe: 0,` |
| `crates/mkrew-ui/src/theme.rs` | 1696 (`from_palette`) `pane_focus_border: mix(p.accent, p.bg, 0.50),` | `pane_focus_stripe: mix(p.accent, p.bg, 0.50),` |
| `crates/mkrew-ui/src/status.rs` | 126 `Some(theme.pane_focus_border)`; 259 in the test | `Some(theme.pane_focus_stripe)` |
| `crates/mkrew-ui/src/preview.rs` | 737 `look.map_or(theme.pane_focus_border, ..)` | `look.map_or(theme.pane_focus_stripe, ..)` |
| `crates/mkrew-ui/src/shell/cards.rs` | 243 `theme.pane_focus_border` | `theme.pane_focus_stripe` |
| `crates/mkrew-ui/src/shell/comments.rs` | 656 | `theme.pane_focus_stripe` |
| `crates/mkrew-ui/src/shell/composer.rs` | 550 `rgb(theme.pane_focus_border)` | `rgb(theme.pane_focus_stripe)` |
| `crates/mkrew-ui/src/shell/files_render.rs` | 992 | `rgb(theme.pane_focus_stripe).into()` |
| `crates/mkrew-ui/src/shell/git_diff.rs` | 714 `.border_color(rgb(theme.pane_focus_border))` | `.border_color(rgb(theme.pane_focus_stripe))` |
| `crates/mkrew-ui/src/shell/jira.rs` | 898 and 969 | `theme.pane_focus_stripe` (both) |
| `crates/mkrew-ui/src/shell/pane_render.rs` | 436 (`render_rename` field border) | `.border_color(rgb(theme.pane_focus_stripe))` |
| `crates/mkrew-ui/src/shell/run.rs` | 651 | `theme.pane_focus_stripe` |
| `crates/mkrew-ui/src/shell/send_dialog.rs` | 359 | `rgb(theme.pane_focus_stripe)` |
| `crates/mkrew-ui/src/shell/slash.rs` | 296 | `rgb(theme.pane_focus_stripe)` |
| `crates/mkrew-ui/src/terminal/view.rs` | 876 (find bar border) | `theme.pane_focus_stripe` |
| `crates/mkrew-ui/src/snapshots/mkrew_ui__theme__tests__mkrew_dark.snap` | 40 `    pane_focus_border: 4870798,` and 162 `            pane_focus_border: 4870798,` | `    pane_focus_stripe: 4870798,` and `            pane_focus_stripe: 4870798,` |
| `crates/mkrew-ui/src/snapshots/mkrew_ui__theme__tests__mkrew_light.snap` | 40 `    pane_focus_border: 10989034,` and 162 `            pane_focus_border: 10989034,` | `    pane_focus_stripe: 10989034,` and `            pane_focus_stripe: 10989034,` |

Also reword the field doc in `theme.rs` (line 837), since the token is no longer only a border:

```rust
    /// Faint accent: the top stripe of the focused split pane, and the border of a picked card,
    /// row or comment (accent halfway to `bg`, SPEC §14).
    pub pane_focus_stripe: u32,
```

Check that nothing was missed (code only; the match list must be empty):

```
git grep -n pane_focus_border -- crates
```

Expected: no output.

- [ ] **Step 2: Run the renamed code**

```
cargo test -p mkrew-ui theme::tests -j 4
cargo test -p mkrew-ui status::tests -j 4
```

Expected: PASS (including `the_mkrew_themes_never_change`, whose snapshots were renamed with the field).

- [ ] **Step 3: Commit the rename**

```
git add crates/mkrew-ui/src
git -c user.email=tungnv975@gmail.com commit -m "refactor(ui): rename pane_focus_border to pane_focus_stripe"
```

- [ ] **Step 4: Write the failing tests and edit the snapshots by hand**

`cargo insta` is not installed, so the two goldens are edited by hand.
The Theme struct order decides the golden order: `pane_gap` goes right before `pane_focus_stripe`, so the line is added right after `qr_light`.
Values are decimal (`{:#?}` of a `u32`): `0x000000` is `0`, `0xF3F4F6` is `15987958`.
Each file has the field twice: once for the theme, once inside `classic: Some(Theme { .. })`, which inherits the base's `pane_gap` through `..base.clone()`, so both lines carry the same number.

`crates/mkrew-ui/src/snapshots/mkrew_ui__theme__tests__mkrew_dark.snap`:
insert after line 39 (`    qr_light: 16777215,`) this line (4 spaces):

```
    pane_gap: 0,
```

and, after the second `qr_light` (line 161 before the first insert, so 162 after it), this line (12 spaces):

```
            pane_gap: 0,
```

The result around each place:

```
    qr_light: 16777215,
    pane_gap: 0,
    pane_focus_stripe: 4870798,
    focus_ring: 15527407,
```

```
            qr_light: 16777215,
            pane_gap: 0,
            pane_focus_stripe: 4870798,
            focus_ring: 15001065,
```

`crates/mkrew-ui/src/snapshots/mkrew_ui__theme__tests__mkrew_light.snap`: the same two inserts with `15987958`:

```
    qr_light: 16777215,
    pane_gap: 15987958,
    pane_focus_stripe: 10989034,
    focus_ring: 1776930,
```

```
            qr_light: 16777215,
            pane_gap: 15987958,
            pane_focus_stripe: 10989034,
            focus_ring: 1776930,
```

The `---` header (`source`, `expression`) stays as it is.

Theme tests, in `crates/mkrew-ui/src/theme.rs` `mod tests`, right after `every_built_in_theme_meets_every_contrast_rule` (ends near line 2612).
`contrast` already exists (`theme.rs:2390`), so no new luminance helper is needed; "darker" is "reads less against black".

```rust
    /// `a` is darker than `b`: it reads less against black.
    fn darker(a: u32, b: u32) -> bool {
        contrast(a, 0x000000) < contrast(b, 0x000000)
    }

    #[test]
    fn the_pane_gap_sits_a_step_off_the_terminal_ground_in_every_theme() {
        for theme in all_themes() {
            let ratio = contrast(theme.pane_gap, theme.terminal_bg);
            assert!(
                ratio >= PANE_GAP_MIN_CONTRAST,
                "{}: pane_gap {:06X} on terminal_bg {:06X} reads {ratio:.3}:1",
                theme.name,
                theme.pane_gap,
                theme.terminal_bg
            );
            // Light themes: a step darker. Dark themes: darker too, unless the ground is
            // already nearly black (then the gap is lighter).
            if !theme.dark || contrast(theme.terminal_bg, 0x000000) >= 1.2 {
                assert!(
                    darker(theme.pane_gap, theme.terminal_bg),
                    "{}: pane_gap {:06X} should be darker than {:06X}",
                    theme.name,
                    theme.pane_gap,
                    theme.terminal_bg
                );
            }
        }
    }

    #[test]
    fn a_derived_pane_gap_goes_lighter_only_when_bg_has_no_room_below_it() {
        let base = *THEMES[1].palette().expect("One Dark has a palette");
        // (dark, bg, the gap is darker than bg)
        for (dark, bg, below) in [
            (true, 0x000000, false),
            (true, 0x050505, false),
            (true, 0x808080, true),
            (false, 0xFFFFFF, true),
            (false, 0x808080, true),
        ] {
            let theme = Theme::from_palette("Edge", &Palette { dark, bg, ..base });
            let ratio = contrast(theme.pane_gap, theme.terminal_bg);
            assert!(
                ratio >= PANE_GAP_MIN_CONTRAST,
                "bg {bg:06X}: {ratio:.3}:1 from {:06X}",
                theme.pane_gap
            );
            assert_eq!(darker(theme.pane_gap, bg), below, "bg {bg:06X}");
        }
    }
```

Add `pane_gap` to the hover test in `crates/mkrew-ui/src/pressable.rs` (line 201), because tab chips and icon buttons now rest on the title bar, which is `pane_gap`:

```rust
                for (rest, name) in [
                    (t.bg, "bg"),
                    (t.surface, "surface"),
                    (t.chrome, "chrome"),
                    (t.pane_gap, "pane_gap"),
                ] {
                    assert_ne!(hover, rest, "{} {look:?} hover equals {name}", t.name);
                }
```

Status test, in `crates/mkrew-ui/src/status.rs` `mod tests`: delete `needs_input_highlights_the_pane_border` (244-264) and put these three in its place.
They are the precedence table of spec 3.2, one behavior each.

```rust
    #[test]
    fn needs_input_wins_the_stripe_over_a_color_and_focus() {
        for theme in [Theme::dark(), Theme::light()] {
            let needs = Some(SessionState::NeedsInput);
            let color = Some(0x112233);
            for (several, focused) in [(false, false), (true, false), (true, true)] {
                assert_eq!(
                    pane_stripe(&theme, needs, color, several, focused),
                    Some(theme.status_needs_input),
                    "several={several} focused={focused}"
                );
            }
            assert_eq!(
                pane_stripe(&theme, needs, None, false, false),
                Some(theme.status_needs_input),
                "a lone pane needing input still shows it"
            );
        }
    }

    #[test]
    fn a_session_color_wins_the_stripe_over_focus() {
        for theme in [Theme::dark(), Theme::light()] {
            let color = Some(0x112233);
            let working = Some(SessionState::Working);
            assert_eq!(pane_stripe(&theme, working, color, true, true), color);
            assert_eq!(pane_stripe(&theme, working, color, true, false), color);
            assert_eq!(
                pane_stripe(&theme, None, color, false, false),
                color,
                "a lone colored pane keeps its stripe (ADR 0037)"
            );
        }
    }

    #[test]
    fn focus_marks_the_stripe_only_among_several_panes() {
        for theme in [Theme::dark(), Theme::light()] {
            let working = Some(SessionState::Working);
            assert_eq!(
                pane_stripe(&theme, working, None, true, true),
                Some(theme.pane_focus_stripe)
            );
            assert_eq!(pane_stripe(&theme, working, None, true, false), None);
            assert_eq!(
                pane_stripe(&theme, working, None, false, true),
                None,
                "lone pane"
            );
            assert_eq!(pane_stripe(&theme, None, None, false, false), None);
        }
    }
```

Move the three broken-token cases of `each_contrast_rule_fires_when_its_token_is_broken` that name the bar (`theme.rs` 2750-2777) to `pane_gap`, because the rules now measure it:

```rust
            (
                "limits chip",
                Theme {
                    pane_gap: t.status_needs_input,
                    ..t.clone()
                },
            ),
            (
                "active tab",
                Theme {
                    tab_active: t.pane_gap,
                    ..t.clone()
                },
            ),
            (
                "brand logo",
                Theme {
                    brand_claude: t.pane_gap,
                    ..t.clone()
                },
            ),
            (
                "brand logo",
                Theme {
                    tab_active: t.brand_codex,
                    ..t.clone()
                },
            ),
```

(The first three replace the cases that used `chrome: t.status_needs_input`, `tab_active: t.chrome` and `brand_claude: t.chrome`; the fourth is unchanged.)

- [ ] **Step 5: Run the tests to see them fail**

```
cargo test -p mkrew-ui theme::tests -j 4
```

Expected: FAIL to compile with `no field `pane_gap` on type `Theme``.

- [ ] **Step 6: Implement the theme token and the derivation**

`crates/mkrew-ui/src/theme.rs`, constants (230-236):

```rust
/// Half the gap between split panes: each pane is inset by it, so panes sit `2 * PANE_HALF_GAP`
/// (8 px) apart and the same from the frame's edges (the Tern look, ADR 0108).
pub const PANE_HALF_GAP: f32 = space::XS;

/// Least contrast ratio between [`Theme::pane_gap`] and [`Theme::terminal_bg`] (Tern measures
/// about 1.07:1). A unit test holds every built-in theme to it and [`Theme::from_palette`]
/// derives the gap to meet it.
pub const PANE_GAP_MIN_CONTRAST: f32 = 1.06;

/// Thickness of the stripe along the top edge of a pane (needs input, session color or focus
/// among several, ADR 0108) and of the grip shown on the drag strip.
pub const PANE_ACCENT_WIDTH: f32 = 2.0;
```

Struct (the `chrome` doc at 777-779 and the field pair at 837-838):

```rust
    /// Inputs and segmented controls on the title bar, and the sidebar of derived themes. The
    /// title bar itself is `pane_gap`.
    pub chrome: u32,
```

```rust
    /// The frame behind and between the panes, and the title bar fill (ADR 0108): a step off
    /// `terminal_bg` (darker, or lighter when `terminal_bg` has no room below it), at least
    /// [`PANE_GAP_MIN_CONTRAST`] away, so a pane looks raised.
    pub pane_gap: u32,
    /// Faint accent: the top stripe of the focused split pane, and the border of a picked card,
    /// row or comment (accent halfway to `bg`, SPEC §14).
    pub pane_focus_stripe: u32,
```

`Theme::dark` (before `pane_focus_stripe: 0x4A528E,`):

```rust
            // Black: 1.07:1 under `terminal_bg` (0B0C0E), the ratio Tern measures.
            pane_gap: 0x000000,
```

`Theme::light` (before `pane_focus_stripe: 0xA7ADEA,`):

```rust
            // 1.07:1 under `terminal_bg` (FCFCFD); 1.11:1 from `tab_active`, 1.04:1 from `hover`.
            pane_gap: 0xF3F4F6,
```

`shared()` (before `pane_focus_stripe: 0,`):

```rust
            pane_gap: 0,
```

`classic_dark` and `classic_light`: no line (they end in `..base.clone()` and inherit `pane_gap`).

`from_palette`: compute the gap and the chip fill before `brand_grounds` (replace line 1657-1658 and the `tab_active` field at 1673):

```rust
        // The frame is a step off `bg` (the terminal ground): toward black, so panes look
        // raised; toward `fg` when `bg` is already so dark that black is under the floor.
        let pane_gap = {
            let toward_black = mix(p.bg, 0x000000, if p.dark { 0.70 } else { 0.065 });
            if contrast(toward_black, p.bg) >= PANE_GAP_MIN_CONTRAST {
                toward_black
            } else {
                mix(p.bg, p.fg, 0.10)
            }
        };
        // The shown tab chip sits on the title bar, which is `pane_gap`.
        let tab_active = mix(pane_gap, p.fg, 0.12);
        // A brand logo sits on the bar and, in the shown tab, on `tab_active`.
        let brand_grounds = [pane_gap, tab_active];
```

and in the struct literal: `tab_active,` (shorthand, replacing `tab_active: mix(p.chrome, p.fg, 0.12),`), plus `pane_gap,` right before `pane_focus_stripe: mix(p.accent, p.bg, 0.50),`.

`problems()` (2536-2551): the three bar rules measure `pane_gap`:

```rust
    // The limits chip (ADR 0106) sits on the title bar (`pane_gap`) in body text: 4.5:1 for the
    // amber (80 to 94%) and the red (95% and more).
    for fg in [t.status_needs_input, t.danger_text] {
        check("limits chip", fg, t.pane_gap, 4.5);
    }
    // The shown tab chip of the title bar (ADR 0106) must stand out from the bar, and its
    // text must read on it.
    check("active tab", t.tab_active, t.pane_gap, 1.1);
    check("active tab", t.text, t.tab_active, 4.5);
    // An agent's brand logo in a tab chip is an icon on the bar and, in the shown tab, on
    // `tab_active` (WCAG 1.4.11, 3:1).
    for logo in crate::icons::AgentLogo::ALL {
        for ground in [t.pane_gap, t.tab_active] {
            check("brand logo", t.brand_color(*logo), ground, 3.0);
        }
    }
```

Also update the doc comment in `crates/mkrew-ui/src/shell/widgets.rs` line 581 (`tab_frame`: "it always differs from `chrome`, see `theme::problems`") to say `pane_gap`, and the `tab_active` field doc in `theme.rs` (782-783: "always differs from `chrome`") to `pane_gap`.

- [ ] **Step 7: Run the theme tests**

```
cargo test -p mkrew-ui theme::tests -j 4
cargo test -p mkrew-ui pressable -j 4
```

Expected: PASS, including `the_pane_gap_sits_a_step_off_the_terminal_ground_in_every_theme`, `a_derived_pane_gap_goes_lighter_only_when_bg_has_no_room_below_it`, `every_built_in_theme_meets_every_contrast_rule`, `the_mkrew_themes_never_change` and `derived_themes_keep_the_fixed_tokens`.
If `every_built_in_theme_meets_every_contrast_rule` names one derived theme with "active tab", "brand logo" or "limits chip", the factor `0.065` is the knob: every value from 0.060 to 0.070 was checked to pass for the nine light themes, so nudge it by 0.005 and re-run; do not touch the rules.
If `the_mkrew_themes_never_change` fails, diff the printed golden against the two snap files: the only differences allowed are the `pane_gap` and `pane_focus_stripe` lines above.

- [ ] **Step 8: Implement `status::pane_stripe`**

`crates/mkrew-ui/src/status.rs`, replace `pane_border` (115-132):

```rust
/// Color of the stripe along the top of a pane, first match wins: amber while the session
/// needs input, the session's own color, the faint accent when focused among several panes,
/// else none (ADR 0108; a lone pane blends into the frame).
pub fn pane_stripe(
    theme: &Theme,
    state: Option<SessionState>,
    color: Option<u32>,
    several: bool,
    focused: bool,
) -> Option<u32> {
    if state == Some(SessionState::NeedsInput) {
        Some(theme.status_needs_input)
    } else if color.is_some() {
        color
    } else if several && focused {
        Some(theme.pane_focus_stripe)
    } else {
        None
    }
}
```

Run:

```
cargo test -p mkrew-ui status::tests -j 4
```

Expected: PASS (the three `*_stripe` tests of Step 4).
`cargo build -p mkrew-ui -j 4` still fails to compile `pane_render.rs` (`pane_border` is gone); the next step fixes the only caller.

- [ ] **Step 9: The pane frame, the frame color, the title bar**

`crates/mkrew-ui/src/shell/pane_render.rs`, `render_tab`, panes area (the last `div()` of the function, quoted text `.p(half_gap)\n            .relative()\n            .child(content_canvas(content))`):

```rust
        div()
            .flex_1()
            .min_h_0()
            // The frame: behind and between the panes, the same color as the title bar.
            .bg(rgb(theme.pane_gap))
            .p(half_gap)
            .relative()
            .child(content_canvas(content))
```

`render_tab`, empty branch (the chain that starts with `return empty_state(` and ends `.relative().child(content_canvas(content));`): add the frame color so an empty tab sits on the frame too.

```rust
            .flex_1()
            .bg(rgb(theme.pane_gap))
            .child(match self.starting_pane {
```

(that is: insert `.bg(rgb(theme.pane_gap))` between `.flex_1()` and `.child(match self.starting_pane {`; the rest of the chain stays.)

`pane_frame`, the doc comment (589-594):

```rust
    /// One pane: the terminal (or why it is not shown; at reading width, centered, for an agent
    /// pane with it on), its composer, a stripe along the top edge (`status::pane_stripe`: needs
    /// input, the session color or the focus among several; ADR 0108). No border: the pane sits
    /// on the frame color (`pane_gap`) with rounded corners. Among several the unfocused ones get
    /// a dim and a slim handle on the top edge drags the pane. Transient signals float in the
    /// top-right corner (`pane_badges`).
    /// `zoomed`: the pane fills the tab. `voice_ring`: the listening ring over the pane;
    /// `width`: the pane's width in px in the last frame (the composer's room).
```

`pane_frame`, the head (623-627): `border` goes, `stripe` is computed after `color`:

```rust
        let reading = info.is_some_and(|info| self.reading_on(info));
        let color = (info.and_then(|info| info.color.as_deref()))
            .and_then(|name| theme.workspace_color(name));
        let stripe = status::pane_stripe(theme, info.map(|s| s.state), color, several, is_focused);
        let accent_border = theme.accent;
```

`pane_frame`, the border layer (699-709) is replaced by the corner masks:

```rust
            // The terminal paints a square quad and gpui clips to rectangles: the frame color
            // over the four corners gives the pane its rounded look.
            .child(corner_masks(theme.pane_gap))
```

`pane_frame`, the session color stripe (723-733) becomes the one stripe (same place, same geometry: `PANE_ACCENT_WIDTH` high, inset by `radius::MEDIUM` on both sides):

```rust
            .when_some(stripe, |el, stripe| {
                el.child(
                    div()
                        .absolute()
                        .top_0()
                        .left(px(radius::MEDIUM))
                        .right(px(radius::MEDIUM))
                        .h(px(theme::PANE_ACCENT_WIDTH))
                        .bg(rgb(stripe)),
                )
            })
```

The drop ring (the transparent-border overlay with `.drag_over::<DraggedPath>`), the drop overlay (`pane_drop_overlay`), `pane_badges`, `pane_edge` and the voice ring stay exactly as they are, in the same order, above the stripe's old place.

New private function at the end of `pane_render.rs`, after `content_canvas`:

```rust
/// Paints the four slivers between the corners of its parent and a rounded rectangle of
/// radius `radius::MEDIUM`, in `color` (the frame behind the pane). It paints nothing else and
/// has no hitbox, so the pane keeps every click and drop.
fn corner_masks(color: u32) -> impl IntoElement {
    canvas(
        |_, _, _| {},
        move |bounds, (), window, _| {
            let r = px(radius::MEDIUM);
            let (left, top) = (bounds.origin.x, bounds.origin.y);
            let right = bounds.origin.x + bounds.size.width;
            let bottom = bounds.origin.y + bounds.size.height;
            // (the corner, the point where the arc starts, the point where it ends): the arc
            // is a quarter circle around the point `r` inside the corner, drawn from the first
            // point to the second one counterclockwise on screen.
            let corners = [
                (point(left, top), point(left + r, top), point(left, top + r)),
                (point(right, top), point(right, top + r), point(right - r, top)),
                (point(right, bottom), point(right - r, bottom), point(right, bottom - r)),
                (point(left, bottom), point(left, bottom - r), point(left + r, bottom)),
            ];
            for (corner, from, to) in corners {
                let mut path = PathBuilder::fill();
                path.move_to(corner);
                path.line_to(from);
                path.arc_to(point(r, r), px(0.0), false, false, to);
                path.close();
                if let Ok(path) = path.build() {
                    window.paint_path(path, gpui::Hsla::from(rgb(color)));
                }
            }
        },
    )
    .absolute()
    .size_full()
}
```

If the smoke of Task 9 shows the slivers bulging the wrong way (a concave notch instead of a rounded pane), flip the `sweep` argument (the 4th, `false`) to `true` in all four arcs; nothing else changes.

`crates/mkrew-ui/src/shell/chrome.rs`, `render_title_bar`, the bar container (345-347): the fill is the frame color and the bottom hairline goes.

```rust
            .bg(rgb(theme.pane_gap))
            .font_family(theme.ui_font_family.clone())
```

(that is: replace the three lines `.bg(rgb(theme.chrome))`, `.border_b(px(theme::BORDER_WIDTH))`, `.border_color(rgb(theme.divider))` with `.bg(rgb(theme.pane_gap))`; the following `.font_family(..)` line stays.)
The same function draws the bar of detached windows (`render_host` calls `self.render_title_bar(host, ..)`), so both windows change at once.

`crates/mkrew-ui/src/shell/theme_gallery.rs`, `theme_picture` (409): the picture's top strip is the title bar of that theme.

```rust
                .bg(rgb(card.pane_gap)),
```

(replacing `.bg(rgb(card.chrome)),`; update the doc line above the function from "the chrome strip" to "the title bar strip".)

- [ ] **Step 10: Build, format, lint, test**

```
cargo fmt --all
cargo clippy -p mkrew-ui --all-targets -j 4 -- -D warnings
cargo test -p mkrew-ui theme::tests -j 4
cargo test -p mkrew-ui status::tests -j 4
cargo test -p mkrew-ui pressable -j 4
```

Expected: no clippy warnings; every test PASS.
No `pane_border`, no `pane_focus_border` remain in code:

```
git grep -nE "pane_border|pane_focus_border" -- crates
```

Expected: no output.

Visual check is part of Task 9's smoke (dark and light): no border on any pane, 8 px between panes and from the frame edge, the frame and the title bar the same color, rounded pane corners, a 2 px top stripe only for needs input, a session color, or the focused pane among two or more panes, and a hovered tab chip still visible on the bar.

- [ ] **Step 11: Commit**

```
git add crates/mkrew-ui/src/theme.rs crates/mkrew-ui/src/snapshots/mkrew_ui__theme__tests__mkrew_dark.snap crates/mkrew-ui/src/snapshots/mkrew_ui__theme__tests__mkrew_light.snap crates/mkrew-ui/src/status.rs crates/mkrew-ui/src/pressable.rs crates/mkrew-ui/src/shell/pane_render.rs crates/mkrew-ui/src/shell/chrome.rs crates/mkrew-ui/src/shell/theme_gallery.rs crates/mkrew-ui/src/shell/widgets.rs
git -c user.email=tungnv975@gmail.com commit -m "feat(ui): Tern pane look with a frame color, 8 px gap and one top stripe"
```

**Review focus for this task:**

- A derived theme whose `bg` is almost black or almost white: the fallback to `fg` and the contrast floor (test `a_derived_pane_gap_goes_lighter_only_when_bg_has_no_room_below_it`).
- A hovered chip on the title bar in every theme (`pressable` test now includes `pane_gap`).
- A lone pane with a session color keeps its stripe; a lone pane that needs input shows amber; an unfocused pane among several shows no stripe unless it has a color or needs input (the three `*_stripe` tests).
- Detached windows: same bar, same frame color (one `render_title_bar`, one `render_tab`).

---

### Task 3 and 4 (merged): Documents store, file leaves, no Editor view

**Why one task.**
Task 3 alone must keep the Editor view drawing the active file with its tab strip.
The strip (order, pinned, transient, active, cycle, close, move, tab menu) lives in `FilesView.tabs` and is driven by about 20 `Shell` functions.
A `Documents` map has no order, so Task 3 alone would need a throwaway `strip: Vec<..>` inside `FilesView` plus a second rewrite of every one of those functions, both deleted by Task 4.
So the two tasks are merged, in three parts with their own gates:

- **Part A (steps 1-6): pure move.** `FileTab` and its helpers move to `documents.rs` as `Doc`, the `Documents` store is added and tested, `FilesView` keeps its strip over `Vec<Doc>`. The app behaves exactly as before. Commit 1 compiles, clippy-clean, tests green.
- **Part B (steps 7-10): pure decisions.** `Place`, `resolve_place`, `needs_save_question`, `open_paths_without`, `keep_leaf`, `FileLabel`, `PaneIcon::File`. Tests only, no shell. Commit 2 compiles, tests green.
- **Part C (steps 11-25): cutover.** `FilesView` drops the strip, the shell moves to `documents` + `DocView` + `Place`, the Editor view and its chips and commands go. This part does not compile until step 23; use `cargo test -p mkrew-ui --lib <filter> -j 4` (the lib crate compiles alone) for the pure steps and `cargo check -p mkrew-ui -j 4` for the shell. One commit at the end (Commit 3).

Execution order: Task 1 and Task 2 are done first.
Task 3+4 is done before Tasks 5, 6, 7, 8; those drafts assume the names fixed here.

**Files**

Create:
- `crates/mkrew-ui/src/documents.rs` (pure: `Doc`, `Documents`, `Place`, `Resolution`, decisions)
- `crates/mkrew-ui/src/documents/tests.rs`
- `crates/mkrew-ui/src/shell/doc_view.rs` (`DocView`, `DocFocus`)
- `crates/mkrew-ui/src/shell/places.rs` (`open_document`, `place_leaf`, `show_pane`, `focus_file_pane`)
- `crates/mkrew-ui/src/shell/file_pane.rs` (`render_file_pane`, moved from `files_render.rs::render_file`)

Modify (main ones; the sweep tables below list the rest):
- `crates/mkrew-ui/src/lib.rs` (`pub mod documents;`)
- `crates/mkrew-ui/src/files_view.rs`, `files_view/tests.rs`
- `crates/mkrew-ui/src/tabs.rs`, `tabs/tests.rs`
- `crates/mkrew-ui/src/main.rs`, `shell.rs` (mod list)
- `crates/mkrew-ui/src/shell/{files,files_render,files_split,files_preview,navigation,sessions,workspace,focus,hosts,pane_render,tabs,reorder,composer,send_dialog,inspector,connection,workspace_actions,menu_render,detached_render,attention,update}.rs`
- `crates/mkrew-ui/src/{board,menu,palette,keymap,redraw}.rs` and their tests

Delete:
- `crates/mkrew-ui/src/shell/file_tabs.rs`
- `crates/mkrew-ui/src/shell/file_tabs_render.rs`

**Interfaces**

Consumes (Task 1, `split.rs` / `tabs.rs` / `hosts.rs`): `PaneKey`, `Leaf`, `Edge`, `Node::{leaf_of, keys}`, `Tab::{keys, focused, shown, layout}`, `Tabs::{tabs, tab, tab_with, focus, insert_leaf, update_leaf, close_leaf, preview_tab, position_after_active, activate}`, `Shell::focused_key`, `Shell::remove_pane`/`close_leaf` write path.
Consumes (Task 2): nothing.

Produces (exact):

```rust
// documents.rs
pub struct Documents;
pub struct Doc;
impl Documents {
    pub fn new() -> Documents;
    pub fn get(&self, path: &str) -> Option<&Doc>;
    pub fn get_mut(&mut self, path: &str) -> Option<&mut Doc>;
    pub fn ensure(&mut self, path: &str, mode: MdMode) -> &mut Doc;
    pub fn retain_open(&mut self, open: &HashSet<String>);   // drops docs not in `open` unless dirty
}
pub enum Place { Preview, NewTab, Split(PaneKey, Edge) }
pub enum Resolution { Focus(PaneKey), Reuse { old: PaneKey }, NewTab { position: Option<f64> }, Split { target: PaneKey, edge: Edge } }
pub fn resolve_place(tabs: &Tabs, docs: Option<&Documents>, key: &PaneKey, place: &Place) -> Resolution;
pub fn open_paths(tabs: &Tabs) -> HashSet<String>;
pub fn open_paths_without(tabs: &Tabs, skip_tabs: &[Ulid], skip_keys: &[PaneKey]) -> HashSet<String>;
pub fn needs_save_question(before: &HashSet<String>, after: &HashSet<String>, docs: &Documents) -> Vec<String>;
pub fn keep_leaf(leaf: &mut Leaf);                       // preview -> kept
pub fn set_leaf_mode(leaf: &mut Leaf, mode: MdMode);
pub fn is_missing(error: &str) -> bool;                  // the daemon's "no file X in project Y"

// tabs.rs
pub enum PaneIcon { Agent(String), Shell, Run, File(String) }   // + Diff in Task 5
pub struct FileLabel { pub name: String, pub dirty: bool, pub preview: bool }
pub fn file_label(tab: &Tab, docs: Option<&Documents>) -> Option<FileLabel>;

// shell
Shell.documents: HashMap<Ulid, Documents>;  Shell.doc_views: HashMap<(Ulid, String), DocView>;
Shell::open_document(&mut self, project_id: Ulid, path: String, line: Option<(u32, u32)>, place: Place, window: &mut Window, cx: &mut Context<'_, Self>);
Shell::place_leaf(&mut self, project_id: Ulid, leaf: Leaf, place: Place, window: &mut Window, cx: &mut Context<'_, Self>);
Shell::create_tab_then(&mut self, project_id: Ulid, position: Option<f64>, then: impl FnOnce(&mut Shell, Ulid, &mut Window, &mut Context<'_, Shell>) + 'static, window: &mut Window, cx: &mut Context<'_, Shell>);
Shell::focus_file_pane(&mut self, project_id: Ulid, path: &str, window: &mut Window, cx: &mut Context<'_, Self>);
Shell::set_file_mode(&mut self, project_id: Ulid, path: &str, mode: MdMode, window: &mut Window, cx: &mut Context<'_, Self>);
Shell::keep_leaf(&mut self, project_id: Ulid, path: &str, window: &mut Window, cx: &mut Context<'_, Self>);
Shell::focused_doc(&self, window: &Window) -> Option<(Ulid, String)>;   // hosts.rs
Shell::focused_file(&self, window: &Window) -> Option<String>;          // hosts.rs
Shell::panel_file(&self, project_id: Ulid) -> Option<String>;
Shell::render_tab(&self, project_id: Ulid, tab: Option<&Tab>, ..., window: &Window, cx)         // project_id first, window before cx
Shell::pane_frame(&self, project_id: Ulid, key: PaneKey, several: bool, zoomed: bool, width: f32, is_focused: bool, composer_focused: bool, voice_ring: Option<Hsla>, now: i64, theme: &Theme, window: &Window, cx)
Shell::render_file_pane(&self, project_id: Ulid, path: &str, window: &Window, theme: &Theme, cx) -> gpui::AnyElement
```

Name clash to know before starting: `crate::docs::Doc` (the Docs view's file state) is already imported by `main.rs` and so by every `shell/*.rs` through `use crate::*`.
The new type is always written `documents::Doc` in shell code; never import it bare there.
`documents.rs` itself does not import `docs::Doc`.

**Deviations from the contract (read them first)**

1. The find bar's state (`Option<TextFind>`) lives in `Doc`, not in `DocView`.
   The pure find, replace and "find follows a re-read" logic and their tests stay headless.
   `DocView` holds the find and replace *focus handles* only.
2. `DocView` field names: `editor_focus`, `find_focus`, `replace_focus`, `block_focus`, `preview_focus`, `code_scroll`, `code_drag`, `text_left`, `page`, `split_bounds`, `split_sync`, `zed_error`.
3. `Place::Preview` for a non-file leaf behaves as `NewTab` (`resolve_place` only reuses a preview tab for a `PaneKey::File`).
4. `Doc::take_read()` now means "asked for the first time or marked stale"; a failed read is retried only by the Retry button (`Doc::retry`) or by the next `git.changed`, so a draw never loops on a failing file.
5. The tab file icon is `Icon::File`, the same icon the Files tree row uses (there is no per-type icon function in the repo).

---

#### Part A: move `FileTab` to `documents.rs` (pure, the app is unchanged)

### Step 1: Create `documents.rs` by moving the file-tab code out of `files_view.rs`

- [ ] **Step 1: Add the module and move the types**

`crates/mkrew-ui/src/lib.rs`: add `pub mod documents;` after `pub mod docs;` (alphabetical).

Create `crates/mkrew-ui/src/documents.rs`.
Move these items out of `crates/mkrew-ui/src/files_view.rs`, verbatim (cut from `files_view.rs`, paste here, keeping doc comments):

| files_view.rs lines (before this task) | Item | New name |
|---|---|---|
| 35-41 | `enum Content` | same |
| 43-62 | `struct Shown` | same |
| 64-72 | `struct SaveRequest` | same |
| 103-116 | `static NEXT_TEXT`, `type Stamp`, `struct MdPreview` | same |
| 118-140 | `BlockEdit`, `BlockEditOutcome`, `StaleEdit` | same |
| 185-219 | `struct FileTab` | `struct Doc` (fields change, below) |
| 256-263 | `fn unshown_save` | same |
| 1002-1519 | `impl FileTab { .. }` | `impl Doc { .. }` |
| 1521-1539 | `LEAVE_ANSWERS`, `enum Leave`, `fn leave_choice` | same |

Stay in `files_view.rs`: `Listing`, `Row`, `Key`, `Requests`, `md_split_share*`, `Open`, `Close`, `FilesView` and its impl (the strip functions go in Part C), `key()`.

The head of `documents.rs`:

```rust
//! The open files of a workspace (SPEC §12A, ADR 0108): one [`Doc`] per path in a
//! [`Documents`] store per workspace, shared by every window. A [`Doc`] holds what an open file
//! is (text, editor, page, find bar, flags); where it is drawn is the layout's `Leaf::File`.
//! Pure and tested headlessly; `shell/places.rs` and `shell/file_pane.rs` do the rest.

use std::collections::{HashMap, HashSet};
use std::ops::Range;
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};

pub use mkrew_core::settings::MdMode;
use mkrew_protocol::{FsContent, FsWritten};
use ulid::Ulid;

use crate::docs;
use crate::editor::{Editor, Pos};
use crate::highlight::Code;
use crate::markdown::Lines;
use crate::preview_select::{self, Point, Selection};
use crate::split::{Edge, Leaf, PaneKey};
use crate::tabs::{Tab, Tabs};
use crate::task_dialog::DialogKey;
use crate::text_box::{Input, TextBox};
use crate::text_find::{self, TextFind};
```

The `Doc` struct (the old `FileTab` minus `path`'s strip role; `transient` and `pinned` stay until step 11):

```rust
/// One open file (SPEC §12A, ADR 0080 as amended by ADR 0108).
#[derive(Debug)]
pub struct Doc {
    /// Relative to the shown folder, `/` separators; the leaf's key.
    pub path: String,
    pub content: Content,
    /// `.md` files: the rendered preview, the editor, or both side by side. Starts as the
    /// leaf's `md`, else `[ui] md_mode`.
    pub mode: MdMode,
    /// Strip leftovers, deleted in step 11 (the leaf's `preview` flag replaces them).
    pub transient: bool,
    pub pinned: bool,
    /// The doc's [`docs::new_opening`] number: the page restarts when it changes.
    pub opening: u64,
    /// Number of the text in the editor: new for every re-read that replaced it.
    text_id: u64,
    page: Option<MdPreview>,
    page_asked: Option<Stamp>,
    /// The preview's text selection; cleared with every new page.
    pub preview_selection: Option<Selection>,
    /// The block being edited in the preview, `None` = none.
    pub block_edit: Option<BlockEdit>,
    saving: bool,
    reading: bool,
    read_again: bool,
    stale: bool,
    /// A (line, column), 1-based, that a link asked for, put under the caret once shown.
    pending_line: Option<(u32, u32)>,
}
```

`Doc::new` replaces `FileTab::new`; the strip sets `transient` itself:

```rust
impl Doc {
    pub fn new(path: &str, mode: MdMode) -> Doc {
        Doc {
            path: path.to_owned(),
            content: Content::Loading,
            mode,
            transient: false,
            pinned: false,
            opening: docs::new_opening(),
            text_id: 0,
            page: None,
            page_asked: None,
            preview_selection: None,
            block_edit: None,
            saving: false,
            reading: false,
            read_again: false,
            stale: false,
            pending_line: None,
        }
    }
```

Changes inside the moved `impl Doc` (everything else is verbatim):

1. `take_read` becomes `pub fn take_read(&mut self) -> bool` (same body for now).
2. `apply_pending_line` takes a column and clamps to a char boundary (a link's `:line:col`):

```rust
    /// Puts the caret on the (line, column) a link asked for, once the file is shown (call it
    /// after every read and after a request). Returns the 0-based line to scroll to, clamped
    /// to the text; `None` while nothing is shown or nothing is pending.
    pub fn apply_pending_line(&mut self) -> Option<usize> {
        let Content::Shown(shown) = &mut self.content else {
            return None;
        };
        let (line, col) = self.pending_line.take()?;
        let line = usize::try_from(line)
            .ok()?
            .saturating_sub(1)
            .min(shown.editor.lines().len().saturating_sub(1));
        let text: &str = &shown.editor.lines()[line];
        let mut col = usize::try_from(col).ok()?.saturating_sub(1).min(text.len());
        while !text.is_char_boundary(col) {
            col -= 1;
        }
        shown.editor.set_cursor(Pos::new(line, col), false);
        Some(line)
    }
```

3. New small methods that replace direct field pokes of `FilesView` (so the fields stay private):

```rust
    /// An `fs.read` is about to be sent; `false` = one is in flight (sent again once it
    /// answers).
    pub fn begin_read(&mut self) -> bool {
        if self.reading {
            self.read_again = true;
            return false;
        }
        self.reading = true;
        true
    }

    /// Nothing in flight any more (the connection was lost).
    pub fn reset_flight(&mut self) {
        self.saving = false;
        self.reading = false;
        self.read_again = false;
    }

    /// `git.changed` came while the doc was not drawn: read it when it is.
    pub fn mark_stale(&mut self) {
        self.stale = true;
    }

    /// The text number a page is built for ([`Documents::preview_ready`] finds the doc by it).
    pub fn text_id(&self) -> u64 {
        self.text_id
    }

    /// A link's line (and column): the caret goes there once the file is shown; the preview
    /// alone switches to the editor so the caret shows. `None` clears an earlier request.
    pub fn set_line(&mut self, line: Option<(u32, u32)>) {
        self.pending_line = line;
        if line.is_some() && self.mode == MdMode::Preview {
            self.mode = MdMode::Source;
        }
    }
```

4. `read()` also searches the find bar (the old `FilesView::read` did it for the active tab):

```rust
    fn read(&mut self, read: Result<(FsContent, Code), String>) -> bool {
        self.reading = false;
        let again = std::mem::take(&mut self.read_again);
        if read.is_err() {
            self.pending_line = None;
        }
        self.show(read);
        again
    }
```
(`search()` is added in step 12 when `find` moves here.)

Run: `cargo check -p mkrew-ui --lib -j 4`.
Expected: errors only in `files_view.rs` (`FileTab`, `Content`, ... not found).

### Step 2: Add the `Documents` store (failing test first)

- [ ] **Step 2: Write the store tests**

Create `crates/mkrew-ui/src/documents/tests.rs`; at the end of `documents.rs` add `#[cfg(test)] mod tests;`.
First the helpers (move `file_text_at` verbatim from `files_view/tests.rs:26-38`, and `page()` from line 221):

```rust
use serde_json::json;

use super::*;

/// MOVED verbatim from files_view/tests.rs:26-38.
fn file_text_at(path: &str, text: &str) -> Result<(FsContent, Code), String> {
    /* paste the body of files_view/tests.rs `file_text_at` here, unchanged */
}

const P: &str = "README.md";

/// A page for tests: what the background build would return.
fn page(text: &str) -> docs::Shown {
    docs::Shown::from_text(text)
}

/// A store showing README.md as read with `text`.
fn opened(text: &str) -> Documents {
    let mut docs = Documents::new();
    docs.ensure(P, MdMode::default());
    assert!(docs.begin_read(P));
    docs.read(P, file_text_at(P, text));
    docs
}

fn doc(docs: &Documents) -> &Doc {
    docs.get(P).expect("README.md is open")
}

fn doc_mut(docs: &mut Documents) -> &mut Doc {
    docs.get_mut(P).expect("README.md is open")
}

fn editor(docs: &mut Documents) -> &mut Editor {
    doc_mut(docs).editor_mut().expect("editable")
}

/// Another file, read.
fn read_ok(docs: &mut Documents, path: &str, text: &str) {
    docs.ensure(path, MdMode::default());
    assert!(docs.begin_read(path));
    docs.read(path, file_text_at(path, text));
}
```

New store tests (these fail until step 3 adds `Documents`):

```rust
#[test]
fn a_doc_is_dropped_when_unused_and_clean_and_kept_when_open_or_dirty() {
    let mut docs = Documents::new();
    for path in ["open.rs", "clean.rs", "dirty.rs"] {
        read_ok(&mut docs, path, "a\n");
    }
    docs.get_mut("dirty.rs").unwrap().editor_mut().unwrap().insert("x");
    let open: HashSet<String> = ["open.rs".to_owned()].into();
    docs.retain_open(&open);
    assert!(docs.get("open.rs").is_some(), "its leaf is in a layout");
    assert!(docs.get("clean.rs").is_none(), "unused and clean");
    assert!(docs.get("dirty.rs").is_some(), "unsaved text is never dropped");
}

#[test]
fn ensure_keeps_the_doc_and_its_opening() {
    let mut docs = Documents::new();
    let first = docs.ensure("a.rs", MdMode::Source).opening;
    assert_eq!(docs.ensure("a.rs", MdMode::Preview).opening, first);
    assert_eq!(docs.get("a.rs").unwrap().mode, MdMode::Source, "the first mode stays");
}

#[test]
fn a_doc_loads_once_on_its_first_draw() {
    let mut docs = Documents::new();
    docs.ensure("a.rs", MdMode::Source);
    // The shell's first draw asks; a second draw before the answer asks for nothing.
    assert!(docs.get_mut("a.rs").unwrap().take_read());
    assert!(!docs.get_mut("a.rs").unwrap().take_read());
    assert!(docs.begin_read("a.rs"));
    assert!(!docs.begin_read("a.rs"), "one read in flight; the next is queued");
    assert_eq!(docs.read("a.rs", file_text_at("a.rs", "x\n")).as_deref(), Some("a.rs"));
}

#[test]
fn refresh_reads_what_is_shown_and_marks_the_rest_stale() {
    let mut docs = Documents::new();
    read_ok(&mut docs, "shown.rs", "a\n");
    read_ok(&mut docs, "hidden.rs", "b\n");
    let reads = docs.refresh(&["shown.rs".to_owned()]);
    assert_eq!(reads, ["shown.rs"]);
    assert!(!docs.get_mut("shown.rs").unwrap().take_read());
    assert!(docs.get_mut("hidden.rs").unwrap().take_read(), "read when it is drawn");
}

#[test]
fn a_missing_file_is_told_from_other_read_errors() {
    assert!(is_missing("no file src/a.rs in project 01J0000000000000000000000"));
    assert!(is_missing("fs.read failed: no file a.rs in project 01J0000000000000000000000"));
    assert!(!is_missing("Cannot read: stream did not contain valid UTF-8"));
}

#[test]
fn a_link_column_lands_on_a_char_boundary_and_is_clamped() {
    let mut docs = opened("aé b\nxyz\n");
    // Column 3 is inside the two-byte "é" (bytes 1-2): the caret moves back to the boundary.
    doc_mut(&mut docs).set_line(Some((1, 3)));
    assert_eq!(doc_mut(&mut docs).apply_pending_line(), Some(0));
    assert_eq!(doc(&docs).shown().unwrap().editor.cursor(), Pos::new(0, 1));
    doc_mut(&mut docs).set_line(Some((2, 99)));
    assert_eq!(doc_mut(&mut docs).apply_pending_line(), Some(1));
    assert_eq!(doc(&docs).shown().unwrap().editor.cursor(), Pos::new(1, 3));
}
```

Run: `cargo test -p mkrew-ui --lib documents -j 4`
Expected: FAIL to compile, `cannot find type Documents in this scope` (and `is_missing`).

- [ ] **Step 3: Implement `Documents`**

Append to `documents.rs` (before the `#[cfg(test)]` line):

```rust
/// What the daemon answers for a file that is gone: `no file <path> in project <id>`.
/// The UI only sees the text, so this is the one place that reads it.
pub fn is_missing(error: &str) -> bool {
    error.contains("no file ") && error.contains(" in project ")
}

impl Doc {
    /// The last read said the file is gone (SPEC §9: "This file was removed.").
    pub fn removed(&self) -> bool {
        matches!(&self.content, Content::Failed(error) if is_missing(error))
    }
}

/// The open files of one workspace, by stored path (`Leaf::File::file`).
#[derive(Debug, Default)]
pub struct Documents {
    docs: HashMap<String, Doc>,
}

impl Documents {
    pub fn new() -> Documents {
        Documents::default()
    }

    pub fn get(&self, path: &str) -> Option<&Doc> {
        self.docs.get(path)
    }

    pub fn get_mut(&mut self, path: &str) -> Option<&mut Doc> {
        self.docs.get_mut(path)
    }

    /// The doc of `path`, made (and left to be read on its first draw) when missing. An
    /// existing doc keeps its mode.
    pub fn ensure(&mut self, path: &str, mode: MdMode) -> &mut Doc {
        self.docs
            .entry(path.to_owned())
            .or_insert_with(|| Doc::new(path, mode))
    }

    /// Drops the docs no layout holds a leaf for, except those with unsaved text (asked about
    /// when the window closes or the app quits).
    pub fn retain_open(&mut self, open: &HashSet<String>) {
        self.docs
            .retain(|path, doc| open.contains(path) || doc.is_dirty());
    }

    pub fn clear(&mut self) {
        self.docs.clear();
    }

    /// Every doc with unsaved changes, by path.
    pub fn unsaved_paths(&self) -> Vec<String> {
        let mut paths: Vec<String> = (self.docs.values())
            .filter(|doc| doc.is_dirty())
            .map(|doc| doc.path.clone())
            .collect();
        paths.sort();
        paths
    }

    /// The first of `paths` that is still unsaved (asked again after a save that text typed
    /// meanwhile made unsaved again).
    pub fn first_unsaved(&self, paths: &[String]) -> Option<String> {
        (paths.iter())
            .find(|path| self.get(path).is_some_and(Doc::is_dirty))
            .cloned()
    }

    /// "Don't save" / "Reload": drops `path`'s unsaved changes; returns the file to read
    /// again, so the doc shows what is on disk.
    pub fn drop_changes(&mut self, path: &str) -> Option<String> {
        let doc = self.get_mut(path)?;
        doc.discard();
        Some(doc.path.clone())
    }

    /// The store as it carries over a lost connection (spec §8): the docs with unsaved text,
    /// kept with their hash so Save still works, nothing in flight, read again when drawn.
    /// `None` when nothing is unsaved.
    pub fn into_unsaved(mut self) -> Option<Documents> {
        self.docs.retain(|_, doc| doc.is_dirty());
        if self.docs.is_empty() {
            return None;
        }
        for doc in self.docs.values_mut() {
            doc.reset_flight();
            doc.mark_stale();
        }
        Some(self)
    }

    /// An `fs.read` of `path` is about to be sent; `false` = its doc is gone, or one is in
    /// flight already (sent again once that one answers).
    pub fn begin_read(&mut self, path: &str) -> bool {
        self.get_mut(path).is_some_and(Doc::begin_read)
    }

    /// The `fs.read` answer (with its highlight) for `path` (dropped when the doc is gone).
    /// Returns the file to read again (asked for meanwhile).
    pub fn read(&mut self, path: &str, read: Result<(FsContent, Code), String>) -> Option<String> {
        let again = self.get_mut(path)?.read(read);
        again.then(|| path.to_owned())
    }

    /// Save `path` (Ctrl+S, "Save" in the unsaved-changes question). `None` when nothing is
    /// unsaved, the file cannot be saved, or a save is already in flight.
    pub fn begin_save(&mut self, path: &str) -> Option<SaveRequest> {
        self.get_mut(path)?.begin_save()
    }

    /// The `fs.write` answer for `path`, which saved the state `point`. `Some(why)` when the
    /// answer has nowhere to show because its doc is gone: the shell toasts it.
    pub fn saved(
        &mut self,
        path: &str,
        point: Option<u64>,
        written: Result<FsWritten, String>,
    ) -> Option<String> {
        match self.get_mut(path) {
            Some(doc) => doc.saved(point, written),
            None => unshown_save(written),
        }
    }

    /// The page built for `stamp`, for the doc whose text it was built from. Returns `true`
    /// when an open block edit was closed by the new page.
    pub fn preview_ready(&mut self, stamp: Stamp, shown: docs::Shown) -> bool {
        (self.docs.values_mut())
            .find(|doc| doc.text_id() == stamp.0)
            .is_some_and(|doc| doc.preview_ready(stamp, shown))
    }

    /// `git.changed`: the docs in `shown` (drawn now) are read again, the others are marked
    /// stale and read when drawn. Returns the paths to read.
    pub fn refresh(&mut self, shown: &[String]) -> Vec<String> {
        let mut reads = Vec::new();
        for (path, doc) in &mut self.docs {
            if shown.contains(path) {
                reads.push(path.clone());
            } else {
                doc.mark_stale();
            }
        }
        reads.sort();
        reads
    }
}
```

`Doc::preview_ready` (moved, was private) becomes `pub fn preview_ready`.

Run: `cargo test -p mkrew-ui --lib documents -j 4`
Expected: PASS (6 tests).

### Step 4: `FilesView` keeps its strip, over `Vec<Doc>`

- [ ] **Step 4: Point `files_view.rs` at `documents`**

At the top of `files_view.rs`, replace the `use` lines of moved things:

```rust
use crate::documents::{Content, Doc, Shown, StaleEdit};
pub use crate::documents::MdMode;
```
and delete the imports that only the moved code used (`Rc`, `AtomicU64`, `Ordering`, `Selection`, `Point`, `DialogKey`, `TextBox`, `Lines`, `Code`, `FsWritten`, `docs`, `preview_select`) as the compiler reports them.

Edits in `FilesView` (all line numbers before this task):

| Line | Old | New |
|---|---|---|
| 129, 439-467, 479-541 and every `FileTab` | `FileTab` | `Doc` |
| 553 | `FileTab::new(path, how == Open::Transient, self.md_mode)` | `{ let mut tab = Doc::new(path, self.md_mode); tab.transient = how == Open::Transient; tab }` |
| 495-499 (`into_unsaved` loop) | `tab.saving = false; tab.reading = false; tab.read_again = false;` | `tab.reset_flight();` |
| 516 | `FileTab::is_dirty` | `Doc::is_dirty` |
| 706-712 (`reveal`) | `tab.pending_line = line; if line.is_some() && tab.mode == MdMode::Preview { tab.mode = MdMode::Source; }` | `tab.set_line(line.map(\|line\| (line, 1)));` |
| 729-739 (`begin_read`) | the body | `self.tab_for_mut(path).is_some_and(Doc::begin_read)` |
| 774-776 (`preview_ready`) | `tab.text_id == stamp.0` | `tab.text_id() == stamp.0` |
| 783, 826 | `FileTab::shown` | `Doc::shown` |
| 978-980 (`refresh`) | `tab.stale \|= Some(at) != active;` | `if Some(at) != active { tab.mark_stale(); }` |

`FilesView::read` also searches (the moved `Doc::read` does not yet): it keeps `if self.file() == Some(path) { self.search(); }`.

Run: `cargo check -p mkrew-ui --lib -j 4`
Expected: PASS (lib compiles; the bin does not yet).

### Step 5: Sweep the shell for the moved names

- [ ] **Step 5: Qualify the moved items in `main.rs` and `shell/`**

In `main.rs` change the `use mkrew_ui::files_view::{..}` line: remove `Content`, `FileTab`, `Shown` (the `files_view::Shown` path), `LEAVE_ANSWERS`, `Leave`, `leave_choice`, `unshown_save`, `MdMode` and add:

```rust
use mkrew_ui::documents::{self, Documents, MdMode, Place};
```
(`docs::Shown` already imported as `Shown` stays.)

Then every error from `cargo check -p mkrew-ui -j 4` is one of these rewrites (do all of them; the full site list from `grep -n "FileTab\|files_view::\(Shown\|Content\|Leave\|LEAVE\|leave_choice\|unshown\|MdMode\)\|Content::" crates/mkrew-ui/src/shell crates/mkrew-ui/src/main.rs`):

| Old | New |
|---|---|
| `FileTab` | `documents::Doc` |
| `Content::Loading / Shown / Failed` | `documents::Content::...` |
| `files_view::Shown` | `documents::Shown` |
| `files_view::{LEAVE_ANSWERS, Leave, leave_choice, unshown_save}` | `documents::{..}` |
| `files_view::MdMode` | `documents::MdMode` |

Known sites: `shell/files.rs` (188, 293, 297, 299, 302, 501, 527), `shell/files_render.rs` (277-709, 786), `shell/files_split.rs` (114-123), `shell/files_preview.rs` (`tab: &FileTab`), `shell/sessions.rs:223` (`FileTab::previewing`), `shell/file_tabs.rs`, `shell/file_tabs_render.rs`, `main.rs` (`md_mode: files_view::MdMode`).

Run: `cargo check -p mkrew-ui -j 4`
Expected: PASS.

### Step 6: Move the document tests, run the gates, commit A

- [ ] **Step 6: Move the tests that are about a doc**

Move from `files_view/tests.rs` to `documents/tests.rs` the tests below, rewritten with these mechanical rules:

- R1 `opened(text)` / `previewed(text)` return a `Documents` (helpers above; `previewed` is the old helper with `doc_mut(&mut docs).preview_request()` and `docs.preview_ready(stamp, page(text))`).
- R2 `view.editor_mut().unwrap()` -> `editor(&mut docs)`; `tab(&view)` -> `doc(&docs)`; `tab_mut(&mut view)` -> `doc_mut(&mut docs)`; `view.shown()` -> `doc(&docs).shown()`; `text(&view)` -> `doc(&docs).shown().map(|s| s.editor.text()).unwrap_or_default()`.
- R3 `view.begin_read(p)`, `view.read(p, r)`, `view.begin_save`, `view.saved`, `view.preview_ready`, `view.drop_changes`, `view.unsaved_paths`, `view.first_unsaved`, `view.into_unsaved` keep their names on `Documents`.
- R4 `view.reveal(P, Some(n), ..)` -> `doc_mut(&mut docs).set_line(Some((n, 1)))`; `view.open_find(b)` and `view.find` stay on `FilesView` until step 12.

| files_view/tests.rs lines | Test | Destination |
|---|---|---|
| 156-176, 178-185, 187-200, 202-218 | link line tests (`a_link_line_waits_for_the_read_and_puts_the_caret_there`, ...) | documents (R4) |
| 220-247, 249-271, 297-309, 311-325, 327-336, 338-351, 353-373 | markdown page, edit-while-building, new page, only markdown, find switches to source, edits survive toggle | documents |
| 273-295 | `every_new_tab_is_a_new_opening` | documents, rewritten: `ensure` twice keeps `opening`; `retain_open(&HashSet::new())` then `ensure` gives a new `opening` |
| 399-429 | `opening_reads_once_retries_failures_and_coalesces_reads` | documents, rewritten in step 12 (retry) |
| 601-617, 619-642, 644-688 | find follows a re-read, seeded find, replace one/all | stay in `files_view` until step 12 (find moves there) |
| 690-726, 728-806, 808-828, 830-877, 879-919, 921-946 | re-read vs unsaved, save, highlight, gutter, disk change, leave answers | documents (the `FilesView` helpers `file_with_base`, `marks` become `Documents` helpers) |
| 961-1195 | preview, selection, block edit tests and `previewed`, `point` | documents |
| 1411-1480 | a read, a save and a highlight land in their own tab | documents, as `a_read_lands_in_its_own_doc` etc. over two paths |
| 1499-1509, 1511-1530, 1532-1558, 1560-1570 | unsaved paths, `into_unsaved`, first unsaved, drop changes | documents |
| 1587-1598, 1600-1613, 1615-1639 | split mode, split rebuild, task box | documents; the first becomes `ensure(P, MdMode::Split)` then `splitting()` and `open_find` switches to source |

Stay in `files_view/tests.rs` (tree): 100-155, 375-397, 431-458, 460-544 (without the `read` assertions after step 11), 546-599, 948-959, 1641-1651, and every strip test until step 11 deletes them.

Run:
```
cargo test -p mkrew-ui --lib documents -j 4
cargo test -p mkrew-ui --lib files_view -j 4
cargo fmt --all
cargo clippy -p mkrew-ui --all-targets -j 4 -- -D warnings
```
Expected: all PASS, no warnings.

- [ ] **Commit A**

```
git add crates/mkrew-ui/src/lib.rs crates/mkrew-ui/src/documents.rs crates/mkrew-ui/src/documents crates/mkrew-ui/src/files_view.rs crates/mkrew-ui/src/files_view crates/mkrew-ui/src/main.rs crates/mkrew-ui/src/shell
git -c user.email=tungnv975@gmail.com commit -m "refactor(ui): move the open-file state out of FilesView into a pure Documents store"
```

---

#### Part B: pure decisions (tests first)

### Step 7: `Place`, `resolve_place`, preview rules, save decision

- [ ] **Step 7: Write the failing tests** (append to `documents/tests.rs`)

```rust
use mkrew_protocol::TerminalTab;

fn wire(project: Ulid, position: f64, layout: serde_json::Value) -> TerminalTab {
    TerminalTab {
        id: Ulid::new(),
        project_id: project,
        title: "Terminal".to_owned(),
        position,
        layout,
        pinned: false,
    }
}

/// One tab per layout, the first one active.
fn tabs_of(layouts: Vec<serde_json::Value>) -> Tabs {
    let project = Ulid::new();
    let mut tabs = Tabs::new(project);
    let wires = (layouts.into_iter().enumerate())
        .map(|(at, layout)| wire(project, at as f64 + 1.0, layout))
        .collect();
    tabs.load(wires);
    tabs
}

fn file(path: &str) -> serde_json::Value {
    json!({"file": path})
}

fn preview(path: &str) -> serde_json::Value {
    json!({"file": path, "preview": true})
}

fn key(path: &str) -> PaneKey {
    PaneKey::File(path.to_owned())
}

#[test]
fn opening_what_is_open_focuses_it_whatever_the_place() {
    let tabs = tabs_of(vec![file("a.rs"), file("b.rs")]);
    for place in [Place::Preview, Place::NewTab, Place::Split(key("a.rs"), Edge::Left)] {
        // b.rs sits in the second tab: it is focused there, never opened a second time.
        assert_eq!(
            resolve_place(&tabs, None, &key("b.rs"), &place),
            Resolution::Focus(key("b.rs"))
        );
    }
}

#[test]
fn a_single_click_reuses_the_preview_tab_and_otherwise_opens_one() {
    let tabs = tabs_of(vec![file("a.rs"), preview("p.rs")]);
    assert_eq!(
        resolve_place(&tabs, None, &key("c.rs"), &Place::Preview),
        Resolution::Reuse { old: key("p.rs") }
    );
    let none = tabs_of(vec![file("a.rs")]);
    assert_eq!(
        resolve_place(&none, None, &key("c.rs"), &Place::Preview),
        Resolution::NewTab { position: none.position_after_active() }
    );
}

#[test]
fn a_kept_open_never_takes_the_preview_tab() {
    let tabs = tabs_of(vec![file("a.rs"), preview("p.rs")]);
    assert_eq!(
        resolve_place(&tabs, None, &key("c.rs"), &Place::NewTab),
        Resolution::NewTab { position: tabs.position_after_active() }
    );
}

#[test]
fn a_new_tab_goes_right_after_the_active_one() {
    let tabs = tabs_of(vec![file("a.rs"), file("b.rs")]);
    let position = tabs.position_after_active().expect("two tabs");
    let second = tabs.tabs()[1].position;
    assert!(position > tabs.tabs()[0].position && position < second);
    assert_eq!(
        resolve_place(&tabs, None, &key("c.rs"), &Place::NewTab),
        Resolution::NewTab { position: Some(position) }
    );
}

#[test]
fn a_drop_splits_the_target_at_the_edge() {
    let tabs = tabs_of(vec![file("a.rs")]);
    assert_eq!(
        resolve_place(&tabs, None, &key("c.rs"), &Place::Split(key("a.rs"), Edge::Top)),
        Resolution::Split { target: key("a.rs"), edge: Edge::Top }
    );
}

#[test]
fn the_diff_never_takes_a_preview_tab() {
    let tabs = tabs_of(vec![preview("p.rs")]);
    assert_eq!(
        resolve_place(&tabs, None, &PaneKey::Diff, &Place::Preview),
        Resolution::NewTab { position: tabs.position_after_active() }
    );
}

#[test]
fn a_preview_with_unsaved_text_is_not_replaced() {
    let tabs = tabs_of(vec![preview("p.rs")]);
    let mut docs = Documents::new();
    read_ok(&mut docs, "p.rs", "a\n");
    docs.get_mut("p.rs").unwrap().editor_mut().unwrap().insert("x");
    assert_eq!(
        resolve_place(&tabs, Some(&docs), &key("c.rs"), &Place::Preview),
        Resolution::NewTab { position: tabs.position_after_active() }
    );
}

/// An edit keeps the preview tab: the leaf stops being a preview, so the next single click
/// opens another tab instead of replacing the file being edited.
#[test]
fn an_edit_flips_the_preview_leaf_to_kept_once() {
    let mut tabs = tabs_of(vec![preview("p.rs")]);
    assert!(tabs.preview_tab().is_some());
    let write = tabs.update_leaf(&key("p.rs"), keep_leaf);
    assert!(write.is_some(), "the layout is saved");
    assert!(tabs.preview_tab().is_none());
    assert!(tabs.update_leaf(&key("p.rs"), keep_leaf).is_none(), "nothing changed: no write");
    assert_eq!(
        resolve_place(&tabs, None, &key("c.rs"), &Place::Preview),
        Resolution::NewTab { position: tabs.position_after_active() }
    );
}

#[test]
fn the_markdown_mode_is_kept_in_the_leaf() {
    let mut tabs = tabs_of(vec![file("a.md")]);
    assert!(tabs.update_leaf(&key("a.md"), |leaf| set_leaf_mode(leaf, MdMode::Split)).is_some());
    assert!(tabs.update_leaf(&key("a.md"), |leaf| set_leaf_mode(leaf, MdMode::Split)).is_none());
    let layout = tabs.tabs()[0].layout().expect("a layout");
    assert_eq!(
        layout.leaf_of(&key("a.md")),
        Some(&Leaf::File { file: "a.md".into(), md: Some(MdMode::Split), preview: false })
    );
}

fn dirty_store(paths: &[&str]) -> Documents {
    let mut docs = Documents::new();
    for path in paths {
        read_ok(&mut docs, path, "a\n");
        docs.get_mut(path).unwrap().editor_mut().unwrap().insert("x");
    }
    docs
}

fn set(paths: &[&str]) -> HashSet<String> {
    paths.iter().map(|path| (*path).to_owned()).collect()
}

#[test]
fn closing_a_leaf_with_unsaved_text_asks_and_moving_it_never_does() {
    let docs = dirty_store(&["a.rs", "b.rs"]);
    // Close: b.rs leaves every layout.
    assert_eq!(needs_save_question(&set(&["a.rs", "b.rs"]), &set(&["a.rs"]), &docs), ["b.rs"]);
    // Move (drag, tear-off, merge): the leaf is somewhere else afterwards, still open.
    assert!(needs_save_question(&set(&["a.rs", "b.rs"]), &set(&["a.rs", "b.rs"]), &docs).is_empty());
    // A clean file closes without a question.
    let clean = Documents::new();
    assert!(needs_save_question(&set(&["a.rs"]), &set(&[]), &clean).is_empty());
    // Several leaving at once are asked in path order.
    assert_eq!(needs_save_question(&set(&["b.rs", "a.rs"]), &set(&[]), &docs), ["a.rs", "b.rs"]);
}

#[test]
fn closing_tabs_and_panes_are_told_from_moves_by_the_remaining_open_set() {
    let tabs = tabs_of(vec![
        file("a.rs"),
        json!({"split": "vertical", "ratio": 0.5, "children": [file("b.rs"), file("c.rs")]}),
    ]);
    let (first, second) = (tabs.tabs()[0].id, tabs.tabs()[1].id);
    assert_eq!(open_paths(&tabs), set(&["a.rs", "b.rs", "c.rs"]));
    // "Close others" of the first tab: b.rs and c.rs go.
    assert_eq!(open_paths_without(&tabs, &[second], &[]), set(&["a.rs"]));
    // Closing one pane.
    assert_eq!(open_paths_without(&tabs, &[], &[key("c.rs")]), set(&["a.rs", "b.rs"]));
    // A pane moved to the first tab is in both sets: the difference is empty.
    let docs = dirty_store(&["b.rs"]);
    assert!(needs_save_question(&open_paths(&tabs), &open_paths(&tabs), &docs).is_empty());
    let _ = first;
}
```

Run: `cargo test -p mkrew-ui --lib documents -j 4`
Expected: FAIL to compile, `cannot find function resolve_place` (and `Place`, `Resolution`, `keep_leaf`, `set_leaf_mode`, `open_paths`, `open_paths_without`, `needs_save_question`).

- [ ] **Step 8: Implement them** (append to `documents.rs` before the test module)

```rust
/// Where an opened file or the diff goes (SPEC §6.1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Place {
    /// A single click in the Files tree: replaces the workspace's preview tab.
    Preview,
    /// A new tab right after the active one.
    NewTab,
    /// A drop: split that pane at that edge.
    Split(PaneKey, Edge),
}

/// What opening `key` does to the layouts.
#[derive(Debug, Clone, PartialEq)]
pub enum Resolution {
    /// It is open already, in whatever tab or window: only focus it.
    Focus(PaneKey),
    /// Replace the preview tab's leaf `old` with it, in place.
    Reuse { old: PaneKey },
    /// A new tab at `position` (`None`: no tabs yet).
    NewTab { position: Option<f64> },
    Split { target: PaneKey, edge: Edge },
}

/// Decides where `key` goes. A key that is open anywhere is focused, never opened twice
/// (one pane per file per workspace). `Preview` reuses the preview tab only for a file, and
/// never one whose text is unsaved.
pub fn resolve_place(
    tabs: &Tabs,
    docs: Option<&Documents>,
    key: &PaneKey,
    place: &Place,
) -> Resolution {
    if tabs.tab_with(key).is_some() {
        return Resolution::Focus(key.clone());
    }
    let new_tab = || Resolution::NewTab {
        position: tabs.position_after_active(),
    };
    match place {
        Place::Preview if matches!(key, PaneKey::File(_)) => {
            match replaceable_preview(tabs, docs) {
                Some(old) => Resolution::Reuse { old },
                None => new_tab(),
            }
        }
        Place::Preview | Place::NewTab => new_tab(),
        Place::Split(target, edge) => Resolution::Split {
            target: target.clone(),
            edge: *edge,
        },
    }
}

/// The preview leaf a single click may replace: clean, in the workspace's preview tab.
fn replaceable_preview(tabs: &Tabs, docs: Option<&Documents>) -> Option<PaneKey> {
    let tab = tabs.preview_tab()?;
    let layout = tab.layout()?;
    tab.keys().into_iter().find(|key| {
        let PaneKey::File(path) = key else {
            return false;
        };
        matches!(layout.leaf_of(key), Some(Leaf::File { preview: true, .. }))
            && !docs.and_then(|docs| docs.get(path)).is_some_and(Doc::is_dirty)
    })
}

/// The stored paths of every file leaf of `tabs` (all tabs, all windows).
pub fn open_paths(tabs: &Tabs) -> HashSet<String> {
    open_paths_without(tabs, &[], &[])
}

/// [`open_paths`] as it would be once the tabs `skip_tabs` and the leaves `skip_keys` are
/// gone: what a close leaves open.
pub fn open_paths_without(
    tabs: &Tabs,
    skip_tabs: &[Ulid],
    skip_keys: &[PaneKey],
) -> HashSet<String> {
    (tabs.tabs().iter())
        .filter(|tab| !skip_tabs.contains(&tab.id))
        .flat_map(Tab::keys)
        .filter(|key| !skip_keys.contains(key))
        .filter_map(|key| match key {
            PaneKey::File(path) => Some(path),
            _ => None,
        })
        .collect()
}

/// The files to ask "Save changes?" about when the open set goes from `before` to `after`:
/// those that left every layout with unsaved text, by path. A move leaves the file in `after`,
/// so it never asks.
pub fn needs_save_question(
    before: &HashSet<String>,
    after: &HashSet<String>,
    docs: &Documents,
) -> Vec<String> {
    let mut leaving: Vec<String> = before
        .difference(after)
        .filter(|path| docs.get(path).is_some_and(Doc::is_dirty))
        .cloned()
        .collect();
    leaving.sort();
    leaving
}

/// The preview leaf becomes a kept one (an edit, a double click, a pin, a drag).
pub fn keep_leaf(leaf: &mut Leaf) {
    if let Leaf::File { preview, .. } = leaf {
        *preview = false;
    }
}

/// The leaf remembers the pane's Markdown mode across restarts.
pub fn set_leaf_mode(leaf: &mut Leaf, mode: MdMode) {
    if let Leaf::File { md, .. } = leaf {
        *md = Some(mode);
    }
}
```

Run: `cargo test -p mkrew-ui --lib documents -j 4`
Expected: PASS (all documents tests).

### Step 9: Tab chip label and icon (failing tests first)

- [ ] **Step 9: Tests** (append to `tabs/tests.rs`; helpers `wire`, `pane` exist there)

```rust
#[test]
fn a_file_leaf_names_and_marks_its_tab_chip() {
    let project = Ulid::new();
    let mut tabs = Tabs::new(project);
    tabs.load(vec![
        wire(project, "Terminal", 1.0, json!({"file": "src/a.rs", "preview": true})),
        wire(project, "Terminal 2", 1.0 + 1.0, json!({"file": "b.md"})),
    ]);
    let mut docs = crate::documents::Documents::new();
    let label = file_label(&tabs.tabs()[0], Some(&docs)).expect("a file is focused");
    assert_eq!((label.name.as_str(), label.dirty, label.preview), ("a.rs", false, true));
    // Unsaved text shows as a dot; a kept file is not italic.
    docs.ensure("b.md", crate::documents::MdMode::Source);
    let kept = file_label(&tabs.tabs()[1], Some(&docs)).expect("a file is focused");
    assert_eq!((kept.name.as_str(), kept.preview), ("b.md", false));
}

#[test]
fn a_name_the_user_gave_the_tab_wins_over_the_file_label() {
    let project = Ulid::new();
    let mut tabs = Tabs::new(project);
    tabs.load(vec![wire(project, "Notes", 1.0, json!({"file": "a.rs"}))]);
    assert_eq!(file_label(&tabs.tabs()[0], None), None);
}

#[test]
fn a_session_pane_has_no_file_label() {
    let project = Ulid::new();
    let mut tabs = Tabs::new(project);
    tabs.load(vec![wire(project, "Terminal", 1.0, pane(Ulid::new()))]);
    assert_eq!(file_label(&tabs.tabs()[0], None), None);
}

#[test]
fn file_panes_have_chip_icons_and_unknown_sessions_none() {
    let project = Ulid::new();
    let split = json!({"split": "vertical", "ratio": 0.5,
        "children": [pane(Ulid::new()), {"file": "src/a.rs"}]});
    let mut tabs = Tabs::new(project);
    tabs.load(vec![wire(project, "Terminal", 1.0, split)]);
    let tab = tabs.active().expect("a tab");
    // The unknown session is skipped, as before; the file shows.
    assert_eq!(tab_icons(tab, &[], true).panes, [PaneIcon::File("src/a.rs".into())]);
    // An inactive tab shows its focused leaf only: the first (session) leaf, unknown here.
    assert!(tab_icons(tab, &[], false).panes.is_empty());
}
```

Run: `cargo test -p mkrew-ui --lib tabs -j 4`
Expected: FAIL to compile, `cannot find function file_label` / `no variant File in PaneIcon`.

- [ ] **Step 10: Implement, run the gates, commit B**

`tabs.rs`: add the variant and extend `tab_icons`.

```rust
pub enum PaneIcon {
    /// An agent pane; the profile id (its badge is looked up by the caller).
    Agent(String),
    Shell,
    Run,
    /// A file pane; its stored path (the caller draws the file icon).
    File(String),
}
```

In `tab_icons`, replace the `icon` closure use and the `panes` statement (keep the `status` code Task 1 left) with:

```rust
    let icon = |s: &Session| match s.kind {
        SessionKind::Agent => PaneIcon::Agent(s.agent_profile.clone()),
        _ if mkrew_core::run::command_name(s).is_some() => PaneIcon::Run,
        _ => PaneIcon::Shell,
    };
    let pane_icon = |key: &PaneKey| match key {
        PaneKey::Session(id) => known(id).map(icon),
        PaneKey::File(path) => Some(PaneIcon::File(path.clone())),
        // Task 5 adds the diff icon.
        PaneKey::Diff => None,
    };
    let mut panes: Vec<PaneIcon> = if active {
        tab.keys().iter().filter_map(pane_icon).collect()
    } else {
        tab.focused().into_iter().filter_map(pane_icon).collect()
    };
```

Beside `session_label` add:

```rust
/// What a tab chip shows for a focused file pane: its name, a dot while it has unsaved text,
/// italics while it is the preview tab (SPEC §5).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileLabel {
    pub name: String,
    pub dirty: bool,
    pub preview: bool,
}

/// The file label of `tab`: its focused pane is a file and the tab keeps its default name (a
/// name the user gave the tab always wins, as for [`session_label`]).
pub fn file_label(tab: &Tab, docs: Option<&Documents>) -> Option<FileLabel> {
    if !has_default_title(tab) {
        return None;
    }
    let key = tab.focused()?;
    let PaneKey::File(path) = key else {
        return None;
    };
    let preview = matches!(
        tab.layout()?.leaf_of(key),
        Some(Leaf::File { preview: true, .. })
    );
    Some(FileLabel {
        name: path.rsplit('/').next().unwrap_or(path).to_owned(),
        dirty: docs.and_then(|docs| docs.get(path)).is_some_and(Doc::is_dirty),
        preview,
    })
}
```
with `use crate::documents::{Doc, Documents};` at the top of `tabs.rs`.

Run:
```
cargo test -p mkrew-ui --lib documents -j 4
cargo test -p mkrew-ui --lib tabs -j 4
cargo fmt --all
cargo clippy -p mkrew-ui --all-targets -j 4 -- -D warnings
```
Expected: PASS. (The dirty dot of the label is checked in the shell; `Doc::is_dirty` is tested in `documents`.)

- [ ] **Commit B**

```
git add crates/mkrew-ui/src/documents.rs crates/mkrew-ui/src/documents crates/mkrew-ui/src/tabs.rs crates/mkrew-ui/src/tabs
git -c user.email=tungnv975@gmail.com commit -m "feat(ui): place resolution, save-question and preview rules for file leaves"
```

---

#### Part C: cutover

All of Part C is one commit. Do the steps in order. `cargo check -p mkrew-ui -j 4` fails until step 23; the pure steps are tested with `--lib`.

### Step 11: `FilesView` stops owning open files (pure, tests first)

- [ ] **Step 11: Strip the tree view**

Delete from `files_view.rs`: the `tabs`, `active`, `zed_error`, `find`, `md_mode` fields and their init in `FilesView::new`; the functions `tabs`, `active_index`, `tab`, `tab_mut`, `tab_for`, `tab_for_mut`, `file`, `shown`, `editor_mut`, `unsaved_paths`, `into_unsaved`, `first_unsaved`, `drop_changes`, `keep`, `position`, `pinned_count`, `open`, `activate`, `cycle`, `select_nth`, `close_targets`, `close`, `pin`, `move_tab`, `active_place`, `begin_read`, `read`, `begin_save`, `saved`, `preview_ready`, `search`, `open_find`, `find_and_editor`, `replace_one`, `replace_all`; the enums `Open` and `Close`; `SaveRequest` leftovers; the `FileTab` `transient` and `pinned` fields in `documents.rs`.

Change what stays:

```rust
    /// A click on a row: the cursor moves there; a folder opens or closes. Opening a file is
    /// the shell's (`Shell::open_document`).
    pub fn click(&mut self, path: &str) -> Requests {
        self.cursor = Some(path.to_owned());
        match self.entry(path) {
            Some(entry) if entry.is_dir => self.toggle(&entry.path),
            _ => Requests::default(),
        }
    }

    /// The row `path` is a listed file (a click on it opens it).
    pub fn is_file(&self, path: &str) -> bool {
        self.entry(path).is_some_and(|entry| !entry.is_dir)
    }

    /// Shows `path` in the tree (quick open, a terminal link): its folders open (listed as
    /// needed) and the cursor moves to it.
    pub fn reveal(&mut self, path: &str) -> Requests {
        let mut list = Vec::new();
        let mut dir = String::new();
        let mut parts: Vec<&str> = path.split('/').collect();
        parts.pop();
        for part in parts {
            if !dir.is_empty() {
                dir.push('/');
            }
            dir.push_str(part);
            list.extend(self.expand(&dir).list);
        }
        self.cursor = Some(path.to_owned());
        Requests { list }
    }
```
`Requests` loses `read`:

```rust
/// What a change asks of the shell: folders to `fs.list`.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Requests {
    pub list: Vec<String>,
}
```
Fix every `Requests { list, read }` / `read: None` in `expand`, `reveal_dir`, `toggle_ignored`, `refresh` (it returned `read: self.file()`), `apply`. `Key::Enter => return self.click(&entry.path, Open::Transient)` becomes `Key::Enter => {}` (the shell opens on Enter before calling `key`). `new_files_view(project_id, target, md_mode)` in the shell loses `md_mode`.

Update the tests that stay (table in step 6): drop the `Open` arguments (`view.click("src")`), the `.read` assertions, and the `tab(..)` ones in `revealing_a_file_opens_its_folders_and_shows_it` (it now asserts `revealed.list` and `view.cursor` only).

Delete these strip tests from `files_view/tests.rs` (their behavior is replaced by `resolve_place` and the Tabs tests): `three`, `tab_paths`, `a_single_click_replaces_the_transient_tab`, `a_double_click_or_an_edit_keeps_the_tab`, `a_dirty_transient_tab_is_never_replaced`, `opening_an_open_file_activates_its_tab`, `new_tabs_go_right_of_the_active_one_after_the_pinned`, `closing_the_active_tab_activates_the_right_then_the_left_one`, `closing_another_tab_keeps_the_active_one`, `bulk_closes_skip_pinned_tabs`, `the_editor_chip_close_takes_every_tab_pinned_too`, `pinning_moves_a_tab_to_the_pinned_group_and_keeps_it`, `reordering_stays_inside_the_group`, `next_previous_wrap_and_nine_is_the_last`, `git_changed_reads_the_active_tab_and_marks_the_others_stale`, `a_file_tabs_place_is_counted_inside_its_pin_group`.

Run: `cargo test -p mkrew-ui --lib files_view -j 4`
Expected: PASS (tree tests only).

### Step 12: The find bar moves to `Doc`; lazy-load and retry semantics (tests first)

- [ ] **Step 12: Tests** (in `documents/tests.rs`; move the three find tests from `files_view/tests.rs` 601-688 with rules R1-R3 and `view.open_find(b)` -> `doc_mut(&mut docs).open_find(b)`, `view.find` -> `doc(&docs).find`, `view.replace_one()` -> `doc_mut(&mut docs).replace_one()`)

Add:

```rust
#[test]
fn a_failed_read_is_retried_by_retry_and_not_by_a_draw() {
    let mut docs = Documents::new();
    docs.ensure("a.rs", MdMode::Source);
    assert!(docs.get_mut("a.rs").unwrap().take_read());
    assert!(docs.begin_read("a.rs"));
    docs.read("a.rs", Err("no file a.rs in project 01J0000000000000000000000".into()));
    assert!(docs.get("a.rs").unwrap().removed());
    // Drawing again asks for nothing: no loop on a failing file.
    assert!(!docs.get_mut("a.rs").unwrap().take_read());
    // Retry reads again, and a coalesced read is sent once the first answers.
    assert!(docs.get_mut("a.rs").unwrap().retry());
    assert!(docs.begin_read("a.rs"));
    assert!(!docs.begin_read("a.rs"));
    assert_eq!(docs.read("a.rs", file_text_at("a.rs", "ok\n")).as_deref(), Some("a.rs"));
    assert!(!docs.get("a.rs").unwrap().removed());
}

#[test]
fn a_file_that_comes_back_replaces_the_removed_state() {
    let mut docs = Documents::new();
    docs.ensure("a.rs", MdMode::Source);
    assert!(docs.begin_read("a.rs"));
    docs.read("a.rs", Err("no file a.rs in project 01J0000000000000000000000".into()));
    // git.changed: the drawn doc is read again and now exists.
    assert_eq!(docs.refresh(&["a.rs".to_owned()]), ["a.rs"]);
    assert!(docs.begin_read("a.rs"));
    docs.read("a.rs", file_text_at("a.rs", "back\n"));
    assert_eq!(docs.get("a.rs").unwrap().shown().unwrap().editor.text(), "back\n");
}

#[test]
fn removed_unsaved_text_is_kept() {
    let mut docs = opened("a\n");
    editor(&mut docs).insert("x");
    assert!(docs.begin_read(P));
    docs.read(P, Err("no file README.md in project 01J0000000000000000000000".into()));
    assert!(doc(&docs).is_dirty(), "a failed re-read never replaces unsaved text");
    assert!(!doc(&docs).removed());
}
```

Run: `cargo test -p mkrew-ui --lib documents -j 4`
Expected: FAIL to compile (`no method named retry`, find functions).

- [ ] **Step 12b: Implement**

In `Doc`: add `pub find: Option<TextFind>` (init `None`), make `stale: true` in `Doc::new`, and replace `take_read`:

```rust
    /// Whether the doc must be read now: it was never drawn, or `git.changed` came while it
    /// was hidden. Clears the mark. A failed read is not read again by a draw (see `retry`).
    pub fn take_read(&mut self) -> bool {
        std::mem::take(&mut self.stale)
    }

    /// Retry after a failed read: shows `Loading` again. `true` = read it.
    pub fn retry(&mut self) -> bool {
        let failed = matches!(self.content, Content::Failed(_));
        if failed {
            self.content = Content::Loading;
        }
        failed
    }

    /// Runs the find bar's query over the text again.
    pub fn search(&mut self) {
        if let (Some(find), Content::Shown(shown)) = (&mut self.find, &self.content) {
            find.search(shown.editor.lines());
        }
    }
```
`Doc::read` ends with `self.search();` before `again`. Move `open_find`, `find_and_editor`, `replace_one`, `replace_all` from `FilesView` (their bodies, with `self.active.and_then(|at| self.tabs.get_mut(at))` replaced by `self`, `self.find` kept, `tab.mode/content/block_edit/preview_selection` by `self.mode/content/block_edit/preview_selection`):

```rust
    /// Ctrl+F / Ctrl+H: opens the find bar, with the replace row when `replace`. A selection
    /// on one line becomes the query, and the current match is the first one at the selection
    /// or caret. Returns `false` when nothing is shown.
    pub fn open_find(&mut self, replace: bool) -> bool {
        if !matches!(self.content, Content::Shown(_)) {
            return false;
        }
        // Matches and the replace row act on the editor's text: the preview alone hides it.
        if self.mode == MdMode::Preview {
            self.mode = MdMode::Source;
        }
        self.block_edit = None;
        self.preview_selection = None;
        let Content::Shown(shown) = &self.content else {
            return false;
        };
        let editor = &shown.editor;
        let seed = (editor.selected_text()).filter(|text| !text.contains('\n'));
        let from = editor
            .selection()
            .map_or(editor.cursor(), |selection| selection.start);
        let find = self.find.get_or_insert_with(TextFind::default);
        if let Some(seed) = seed {
            find.query = seed.into();
        }
        if replace {
            find.replace.get_or_insert_with(Input::default);
        }
        find.search_from(editor.lines(), (from.line, from.col));
        true
    }

    /// The find bar and the editor of an editable file (not binary or too large).
    fn find_and_editor(&mut self) -> Option<(&mut TextFind, &mut Editor)> {
        match (&mut self.find, &mut self.content) {
            (Some(find), Content::Shown(shown)) if !shown.code.binary && !shown.code.too_large => {
                Some((find, &mut shown.editor))
            }
            _ => None,
        }
    }
```
`replace_one` and `replace_all` are copied unchanged from `FilesView` (lines 837-879) onto `Doc`.

Run: `cargo test -p mkrew-ui --lib documents -j 4` then `cargo test -p mkrew-ui --lib files_view -j 4`
Expected: PASS.

### Step 13: `Shell` state, `DocView`, helpers

- [ ] **Step 13: Create `shell/doc_view.rs`**

```rust
//! The gpui state of one open file pane (focus handles, scroll, page): not pure, so it is
//! not in `Documents`. One per (workspace, path), made when the file is opened or first drawn.

use crate::*;

/// Which of a pane's focus handles.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DocFocus {
    Editor,
    Find,
    Replace,
    Block,
    Preview,
}

pub(crate) struct DocView {
    /// The editor: typing goes there while it has focus (ADR 0028).
    pub(crate) editor_focus: FocusHandle,
    /// The find bar's two fields (outside `EDITOR_CONTEXT`'s arrow keys).
    pub(crate) find_focus: FocusHandle,
    pub(crate) replace_focus: FocusHandle,
    /// The block edit field of the Markdown preview (ADR 0072).
    pub(crate) block_focus: FocusHandle,
    /// The Split view's page (the editor beside it keeps `editor_focus`).
    pub(crate) preview_focus: FocusHandle,
    pub(crate) code_scroll: UniformListScrollHandle,
    /// Where the editor's scroll bar thumb is held while it is dragged.
    pub(crate) code_drag: scrollbar::Drag,
    /// Where the editor's text starts on screen (set while painting), for clicks.
    pub(crate) text_left: Rc<Cell<Pixels>>,
    /// The rendered Markdown page (a virtual list, kept across frames).
    pub(crate) page: preview::Page,
    /// Where the Split view's two halves were painted, for the divider drag.
    pub(crate) split_bounds: Rc<Cell<Bounds<Pixels>>>,
    /// The Split view's scroll sync (source -> preview, SPEC §12A).
    pub(crate) split_sync: files_split::Sync,
    /// Why the last "Open in Zed" of this file failed.
    pub(crate) zed_error: Option<String>,
}

impl DocView {
    pub(crate) fn new(cx: &App) -> DocView {
        DocView {
            editor_focus: cx.focus_handle(),
            find_focus: cx.focus_handle(),
            replace_focus: cx.focus_handle(),
            block_focus: cx.focus_handle(),
            preview_focus: cx.focus_handle(),
            code_scroll: UniformListScrollHandle::new(),
            code_drag: scrollbar::Drag::default(),
            text_left: Rc::default(),
            page: preview::Page::default(),
            split_bounds: Rc::default(),
            split_sync: files_split::Sync::default(),
            zed_error: None,
        }
    }

    pub(crate) fn handle(&self, which: DocFocus) -> &FocusHandle {
        match which {
            DocFocus::Editor => &self.editor_focus,
            DocFocus::Find => &self.find_focus,
            DocFocus::Replace => &self.replace_focus,
            DocFocus::Block => &self.block_focus,
            DocFocus::Preview => &self.preview_focus,
        }
    }
}
```
Add `mod doc_view;` and `pub(crate) use doc_view::{DocFocus, DocView};` in `shell.rs` (where the other `shell/*` modules are declared and re-exported for `use crate::*`; follow the neighbouring lines), plus `mod places;` `mod file_pane;`, and remove `mod file_tabs; mod file_tabs_render;`.

`main.rs` `Shell` struct: delete the fields `file_tabs_scroll`, `file_find_focus`, `file_replace_focus`, `file_code_scroll`, `file_code_drag`, `file_editor_focus`, `file_block_focus`, `file_text_left`, `file_preview_focus`, `md_split_bounds`, `md_split_sync`, `file_page`, `file_tab_menu`. Keep `files`, `files_focus`, `files_scroll`, `files_after_save`, `md_mode`, `md_split`, `md_split_dragging`. Add:

```rust
    /// The open files of every workspace (SPEC §12A, ADR 0108); one store per workspace.
    documents: HashMap<Ulid, Documents>,
    /// The view state of each open file pane, keyed like the store.
    doc_views: HashMap<(Ulid, String), DocView>,
```
`shell/connection.rs` init (lines 447-466): delete the matching initializers and add `documents: HashMap::new(), doc_views: HashMap::new(),`. `focus.rs::close_menus` (line 125): delete `self.file_tab_menu = None;`. `connection.rs:360`: same.

Disconnect (`connection.rs:885`) and reconnect (`967`):

```rust
        // Unsaved file text outlives the connection: Save writes it with the old hash (§8).
        self.documents = std::mem::take(&mut self.documents)
            .into_iter()
            .filter_map(|(id, docs)| Some((id, docs.into_unsaved()?)))
            .collect();
        self.doc_views.clear();
```
and in the reconnect `retain`: `shell.documents.retain(|id, _| known(id)); shell.doc_views.retain(|(id, _), _| known(id));`. The `self.files = take(..).filter_map(into_unsaved)` statement becomes `self.files.clear();` (the tree is read again).

Accessors (top of `shell/files.rs`, replacing `files_view`/`files_at`'s `md_mode`):

```rust
    pub(crate) fn files_view(&mut self, project_id: Ulid) -> &mut FilesView {
        self.files
            .entry(project_id)
            .or_insert_with(|| new_files_view(project_id, None))
    }

    pub(crate) fn doc(&self, project_id: Ulid, path: &str) -> Option<&documents::Doc> {
        self.documents.get(&project_id)?.get(path)
    }

    pub(crate) fn doc_mut(&mut self, project_id: Ulid, path: &str) -> Option<&mut documents::Doc> {
        self.documents.get_mut(&project_id)?.get_mut(path)
    }

    pub(crate) fn doc_view(&self, project_id: Ulid, path: &str) -> Option<&DocView> {
        self.doc_views.get(&(project_id, path.to_owned()))
    }

    /// The doc and view state of a file pane, made when missing. `mode` is the leaf's `md`.
    pub(crate) fn ensure_doc(
        &mut self,
        project_id: Ulid,
        path: &str,
        mode: Option<MdMode>,
        cx: &App,
    ) {
        let mode = mode.unwrap_or(self.md_mode);
        self.documents.entry(project_id).or_default().ensure(path, mode);
        (self.doc_views.entry((project_id, path.to_owned()))).or_insert_with(|| DocView::new(cx));
    }

    pub(crate) fn focus_doc(
        &self,
        project_id: Ulid,
        path: &str,
        which: DocFocus,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        if let Some(view) = self.doc_view(project_id, path) {
            window.focus(view.handle(which), cx);
        }
    }

    pub(crate) fn doc_focused(
        &self,
        project_id: Ulid,
        path: &str,
        which: DocFocus,
        window: &Window,
    ) -> bool {
        self.doc_view(project_id, path)
            .is_some_and(|view| view.handle(which).is_focused(window))
    }
```
`new_files_view(project_id, target)` drops the `md_mode` parameter (`FilesView::new` already did not take it after step 11).

`hosts.rs` (after `focused_session`; `focused_key` is Task 1's):

```rust
    /// The workspace and file of the focused pane of `window`, when it is a file pane.
    pub(crate) fn focused_doc(&self, window: &Window) -> Option<(Ulid, String)> {
        let (project, tab_id) = self.tab_scope(window)?;
        match self.tabs.get(&project)?.tab(tab_id)?.focused()? {
            PaneKey::File(path) => Some((project, path.clone())),
            _ => None,
        }
    }

    pub(crate) fn focused_file(&self, window: &Window) -> Option<String> {
        self.focused_doc(window).map(|(_, path)| path)
    }

    /// The file the right panel and prompt variables mean: the shown workspace's active tab's
    /// focused pane, when it is a file (also while a detached window has the keyboard).
    pub(crate) fn panel_file(&self, project_id: Ulid) -> Option<String> {
        match self.tabs.get(&project_id)?.active()?.focused()? {
            PaneKey::File(path) => Some(path.clone()),
            _ => None,
        }
    }
```
and the generalized focus (replaces `focus_session_pane`'s body):

```rust
    /// Focuses the pane of `key` in its tab; a tab in a detached window does not become the
    /// main window's active tab (spec §4.4).
    pub(crate) fn focus_pane_key(&mut self, key: &PaneKey) {
        let Some((project, tab_id)) = (self.tabs.iter())
            .find_map(|(project, tabs)| Some((*project, tabs.tab_with(key)?.id)))
        else {
            return;
        };
        let detached = self.terminal_host(project, tab_id) != HostId::Main;
        if let Some(tabs) = self.tabs.get_mut(&project) {
            if detached {
                tabs.focus_pane(tab_id, key);
            } else {
                tabs.focus(key);
            }
        }
    }

    pub(crate) fn focus_session_pane(&mut self, session: Ulid) {
        self.focus_pane_key(&PaneKey::Session(session));
    }
```
`tab_title` (hosts.rs:63): `let label = file_label(tab, self.documents.get(&project)).map(|label| label.name);` then `let label = label.as_deref().or_else(|| session_label(tab, self.sidebar.sessions())).unwrap_or(&tab.title);`.

Run: `cargo check -p mkrew-ui -j 4`
Expected: still FAIL (the shell is half moved); continue.

### Step 14: Files shell functions

- [ ] **Step 14: Rewrite `shell/files.rs`**

`set_files_target` (the Files target of a workspace changes: its open files start over, as today):

```rust
    pub(crate) fn set_files_target(
        &mut self,
        project_id: Ulid,
        target: Option<Ulid>,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        if self.files_view(project_id).worktree_task_id == target {
            return;
        }
        let unsaved = (self.documents.get(&project_id)).map_or_else(Vec::new, Documents::unsaved_paths);
        if !unsaved.is_empty() {
            let again: AfterSave = Box::new(move |shell, window, cx| {
                shell.set_files_target(project_id, target, window, cx);
            });
            return self.leave_documents(project_id, unsaved, again, window, cx);
        }
        self.files.insert(project_id, new_files_view(project_id, target));
        self.close_file_leaves(project_id, window, cx);
        self.files_scroll.scroll_to_item(0, ScrollStrategy::Top);
        self.refresh_files(project_id, window, cx);
    }

    /// Every file leaf of the workspace closes and its docs go (the target changed, or its
    /// worktree is gone). Unsaved text was asked about first.
    pub(crate) fn close_file_leaves(
        &mut self,
        project_id: Ulid,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        self.documents.remove(&project_id);
        self.doc_views.retain(|(project, _), _| *project != project_id);
        let keys: Vec<PaneKey> = (self.tabs.get(&project_id).into_iter())
            .flat_map(|tabs| tabs.tabs().iter().flat_map(Tab::keys))
            .filter(|key| matches!(key, PaneKey::File(_)))
            .collect();
        for key in keys {
            self.remove_leaf(key, window, cx);
        }
    }
```
`refresh_files`: unchanged except the comment ("and re-reads"): the tree only.
`files_requests`: delete the `if let Some(path) = requests.read {..}` branch.

`read_file` (replaces lines 121-173):

```rust
    /// `fs.read` of a doc's file, highlighted on the background executor; while one is in
    /// flight, the file is read once more after it answers.
    pub(crate) fn read_file(
        &mut self,
        project_id: Ulid,
        path: String,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let Some(connection) = self.connection.clone() else {
            return;
        };
        let target = self.files_view(project_id).worktree_task_id;
        let docs = self.documents.entry(project_id).or_default();
        let opening = docs.get(&path).map(|doc| doc.opening);
        if !docs.begin_read(&path) {
            return;
        }
        let read = connection.call::<FsContent>(Request::FsRead(FsReadParams {
            project_id,
            worktree_task_id: target,
            path: path.clone(),
            index_text: true,
        }));
        let background = cx.background_executor().clone();
        cx.spawn_in(window, async move |this, cx| {
            let read = match read.await {
                Ok(content) => Ok(background
                    .spawn(async move {
                        let code = Code::new(&content);
                        (content, code)
                    })
                    .await),
                Err(error) => Err(format!("{error:#}")),
            };
            let _ = this.update_in(cx, |shell, window, cx| {
                if let Err(error) = &read {
                    tracing::warn!(%project_id, path, "fs.read failed: {error}");
                }
                // Dropped when its doc was closed (and maybe opened again), or the files
                // started over on another worktree, meanwhile.
                let same_target = (shell.files.get(&project_id))
                    .is_some_and(|view| view.worktree_task_id == target);
                let again = (shell.documents.get_mut(&project_id))
                    .filter(|docs| same_target && docs.get(&path).map(|doc| doc.opening) == opening)
                    .and_then(|docs| docs.read(&path, read));
                if let Some(path) = again {
                    shell.read_file(project_id, path, window, cx);
                }
                shell.show_pending_line(project_id, &path);
                shell.build_preview(project_id, &path, window, cx);
                cx.notify();
            });
        })
        .detach();
    }

    /// The Retry button of a pane whose read failed.
    pub(crate) fn retry_read(
        &mut self,
        project_id: Ulid,
        path: &str,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        if self.doc_mut(project_id, path).is_some_and(documents::Doc::retry) {
            self.read_file(project_id, path.to_owned(), window, cx);
        }
        cx.notify();
    }
```

`build_preview` (replaces 178-207):

```rust
    pub(crate) fn build_preview(
        &mut self,
        project_id: Ulid,
        path: &str,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let Some((stamp, text)) = (self.doc_mut(project_id, path)).and_then(documents::Doc::preview_request)
        else {
            return;
        };
        let path = path.to_owned();
        let background = cx.background_executor().clone();
        cx.spawn_in(window, async move |this, cx| {
            let page = background
                .spawn(async move { Shown::from_text(&text) })
                .await;
            let _ = this.update_in(cx, |shell, window, cx| {
                // The stamp names the text: a page of a text that was replaced is dropped.
                let dropped = (shell.documents.get_mut(&project_id))
                    .is_some_and(|docs| docs.preview_ready(stamp, page));
                if dropped {
                    shell.toast_info("The file changed; your block edit was closed", cx);
                }
                shell.build_preview(project_id, &path, window, cx);
                cx.notify();
            });
        })
        .detach();
    }

    /// Puts the caret on the line a link asked for and scrolls there, once the file is shown
    /// (after a request when it already was, after the read otherwise).
    pub(crate) fn show_pending_line(&mut self, project_id: Ulid, path: &str) {
        if let Some(line) = self.doc_mut(project_id, path).and_then(documents::Doc::apply_pending_line)
            && let Some(view) = self.doc_view(project_id, path)
        {
            view.code_scroll.scroll_to_item(line, ScrollStrategy::Center);
        }
    }
```
(the old `show_pending_line(project_id, target)` in `navigation.rs:200-212` is deleted.)

`files_event` (replaces 209-225): every drawn doc follows `git.changed`, also those in a detached window.

```rust
    /// Follows `git.changed`: the open files on screen are read again (the others when drawn),
    /// and the tree on screen lists its open folders again.
    pub(crate) fn files_event(
        &mut self,
        event: &Event,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let Event::GitChanged(changed) = event else {
            return;
        };
        let project_id = changed.project_id;
        let target = self.files.get(&project_id).and_then(|view| view.worktree_task_id);
        if changed.worktree_task_id != target {
            return;
        }
        let shown: Vec<String> = (self.visible_files().into_iter())
            .filter(|(project, ..)| *project == project_id)
            .map(|(_, path, _)| path)
            .collect();
        if let Some(docs) = self.documents.get_mut(&project_id) {
            for path in docs.refresh(&shown) {
                self.read_file(project_id, path, window, cx);
            }
        }
        if self.workspace == Some(project_id)
            && self.file_tree_shown()
            && let Some(view) = self.files.get_mut(&project_id)
        {
            let requests = view.apply(event);
            self.files_requests(project_id, requests, window, cx);
        }
    }
```
(borrow note: collect `docs.refresh(&shown)` into a `Vec` first if the borrow checker complains.)

`click_file` (replaces 227-254) and the Enter branch of `files_key_down`:

```rust
    /// A click on a tree row: opens or closes a folder; a file opens as the preview tab, or
    /// as a kept one on a double click (SPEC §6.1).
    pub(crate) fn click_file(
        &mut self,
        project_id: Ulid,
        path: &str,
        double: bool,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let view = self.files_view(project_id);
        let is_file = view.is_file(path);
        let requests = view.click(path);
        self.files_requests(project_id, requests, window, cx);
        if is_file {
            let place = if double { Place::NewTab } else { Place::Preview };
            self.open_document(project_id, path.to_owned(), None, place, window, cx);
            if double {
                self.keep_leaf(project_id, path, window, cx);
            }
        }
        // The keyboard stays in the tree.
        window.focus(&self.files_focus, cx);
    }
```
`files_key_down` line 271: `return self.click_file(project_id, &cursor, false, window, cx);`.

Find and editor functions: they lose `self.workspace` and `files_view(..)` and take the pane from the focus. Replace `open_file_find` and the find key handlers like this (the rest of each function body is unchanged except as shown):

```rust
    pub(crate) fn open_file_find(
        &mut self,
        replace: bool,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let Some((project_id, path)) = self.focused_doc(window) else {
            return;
        };
        if !self.doc_mut(project_id, &path).is_some_and(|doc| doc.open_find(replace)) {
            return;
        }
        let field = if replace { DocFocus::Replace } else { DocFocus::Find };
        self.focus_doc(project_id, &path, field, window, cx);
        self.reveal_file_match(project_id, &path, cx);
    }
```

| Function (old signature) | New signature | Body edits |
|---|---|---|
| `file_find_key_down(&mut self, event, window, cx)` | same | start: `let Some((project_id, path)) = self.focused_doc(window) else { return };` (was `self.workspace`); `self.files_view(project_id).find.as_ref()` -> `self.doc(project_id, &path).and_then(\|doc\| doc.find.as_ref())`; `window.focus(&self.file_replace_focus, cx)` -> `self.focus_doc(project_id, &path, DocFocus::Replace, window, cx)`; calls get `&path` |
| `edit_file_find(project_id, edit, cx)` | `(project_id, path: &str, edit, cx)` | `let Some(doc) = self.doc_mut(project_id, path) else { return }; let Some(find) = &mut doc.find else { return }; edit(&mut find.query, cx); doc.search();` then `reveal_file_match(project_id, path, cx)` |
| `step_file_find(project_id, forward, cx)` | `(project_id, path, forward, cx)` | `self.doc_mut(..).and_then(\|d\| d.find.as_mut())` |
| `reveal_file_match(project_id, cx)` | `(project_id, path, cx)` | `found` from `doc.find`; `doc.editor_mut()`; scroll `self.doc_view(..).code_scroll` |
| `close_file_find(project_id, window, cx)` | `(project_id, path, window, cx)` | `doc.find = None`; `focus_doc(.., DocFocus::Editor, ..)` |
| `file_replace_key_down` | same | like `file_find_key_down` |
| `edit_file_replace`, `replace_in_file`, `replace_all_in_file` | add `path: &str` after `project_id` | `doc_mut(..)` for `find` and `replace_one()` / `replace_all()`; `self.file_edited(project_id, path, window, cx)` |
| `file_editor_key_down` (line 689+) | same | `let (Some(key), Some((project_id, path))) = (editor::edit_key(&event.keystroke), self.focused_doc(window)) else { return };` `let page = self.file_page_lines(project_id, &path, cx);` `files_view(project_id).editor_mut()` -> `self.doc_mut(project_id, &path).and_then(documents::Doc::editor_mut)`; `self.file_code_scroll.scroll_to_item` -> `self.doc_view(project_id, &path)` + `code_scroll`; `file_edited(project_id, &path, ..)` |
| `click_file_text(project_id, line, x, extend, window, cx)` | `(project_id, path: &str, line, x, extend, window, cx)` | `window.focus(&self.file_editor_focus)` -> `focus_doc(.., Editor, ..)`; `self.file_text_left.get()` -> `view.text_left.get()` |
| `file_page_lines(&self, cx)` | `(&self, project_id, path, cx)` | `view.code_scroll.0.borrow().last_item_size` |
| `open_file_in_zed(project_id, cx)` | `(project_id, path: &str, cx)` | `line` from `self.doc(project_id, path).and_then(Doc::line)`; target from `self.files.get(&project_id)`; the error goes to `self.doc_views.get_mut(&(project_id, path.to_owned()))`'s `zed_error` |
| `save_file(project_id, path, then, window, cx)` | same | `self.files.get_mut(&project_id)` -> `self.documents.get_mut(&project_id)`; `target` from `self.files`; `view.tab_for(&path).map(\|t\| t.opening)` -> `docs.get(&path).map(\|d\| d.opening)`; `view.begin_save(&path)` -> `docs.begin_save(&path)`; `FileTab::is_saving` -> `documents::Doc::is_saving`; in the answer: `shell.files_at(project_id, target)` -> the same `same_target` + `documents.get_mut(..)` check as `read_file`, calling `docs.saved(&save.path, save.point, written)` |
| `file_edited(project_id, window, cx)` | `(project_id, path: &str, window, cx)` | `self.keep_leaf(project_id, path, window, cx);` replaces `view.keep()`; `doc.search()`; version and re-highlight via `self.doc(project_id, path)`; the delayed task re-reads `doc.highlight_request(version)` / `doc.highlighted(..)` by path |

`file_edited`'s re-highlight task keeps its structure (spawn, `REHIGHLIGHT_DELAY`, background `Code::new`, `highlighted(version, code)`) with the doc looked up by `(project_id, path.to_owned())` instead of `tab_for(path)`.

`leave_documents` replaces `leave_files` (lines 547-605; the prompt text, `LEAVE_ANSWERS` and the spawn structure stay):

```rust
    /// Runs `then` once the unsaved changes of `paths` (docs of `project_id`) are dealt with,
    /// asking about one file at a time (its pane is shown while asked):
    /// - at once when none is unsaved;
    /// - after "Save" and a successful save, or "Don't save", for each of them.
    ///
    /// "Cancel" stops: `then` does not run (§2.5).
    pub(crate) fn leave_documents(
        &mut self,
        project_id: Ulid,
        paths: Vec<String>,
        then: AfterSave,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        // A doc saved meanwhile is skipped; one typed into during its save is asked again.
        let next = (self.documents.get(&project_id)).and_then(|docs| docs.first_unsaved(&paths));
        let Some(path) = next else {
            return then(self, window, cx);
        };
        // Its leaf is shown (in whatever tab or window holds it; a doc whose leaf is gone
        // gets a new tab): the question is about what the user sees.
        self.open_document(project_id, path.clone(), None, Place::NewTab, window, cx);
        let name = path.rsplit('/').next().unwrap_or(&path).to_owned();
        let workspace = (self.sidebar.projects().iter())
            .find(|project| project.id == project_id)
            .map_or_else(String::new, |project| format!(" in {}", project.name));
        let detail = format!("{path}{workspace} — your changes are lost if you don't save them.");
        let answer = window.prompt(
            PromptLevel::Warning,
            &format!("Save changes to {name}?"),
            Some(detail.as_str()),
            &documents::LEAVE_ANSWERS,
            cx,
        );
        cx.spawn_in(window, async move |this, cx| {
            let choice = documents::leave_choice(answer.await.ok());
            let _ = this.update_in(cx, |shell, window, cx| {
                let rest: AfterSave = Box::new(move |shell, window, cx| {
                    shell.leave_documents(project_id, paths, then, window, cx);
                });
                match choice {
                    Some(documents::Leave::Save) => {
                        shell.save_file(project_id, path, Some(rest), window, cx);
                    }
                    Some(documents::Leave::DontSave) => {
                        // The doc shows the file on disk again (it may stay open on Cancel).
                        let read = (shell.documents.get_mut(&project_id))
                            .and_then(|docs| docs.drop_changes(&path));
                        if let Some(path) = read {
                            shell.read_file(project_id, path, window, cx);
                        }
                        rest(shell, window, cx);
                    }
                    None => {}
                }
            });
        })
        .detach();
    }
```
(The old text said "— your changes ..." with an em dash in the repo string; keep the string as it is in the repo, do not retype it: copy the line from the old function.)

`ask_unsaved` (without `show_view`), lines 668-688:

```rust
        let unsaved = (self.documents.iter()).find_map(|(project_id, docs)| {
            let paths = docs.unsaved_paths();
            (!paths.is_empty()).then_some((*project_id, paths))
        });
        let Some((project_id, paths)) = unsaved else {
            return false;
        };
        // A confirmation is open (Alt+F4 reaches here): answer it first; asking now would
        // replace it.
        if window.has_active_prompt() {
            return true;
        }
        if self.workspace != Some(project_id) {
            self.activate_workspace(project_id, window, cx);
        }
        // `leave_documents` shows the file's pane, so a refused save is visible.
        self.leave_documents(project_id, paths, then, window, cx);
        true
```
Doc comments of `window_should_close`, `close_when_saved`, `ask_unsaved`: "Files changes" -> "unsaved file changes". Callers keep working: `attention.rs:93`, `update.rs:120,180`, `window_should_close`, `close_when_saved` (all call `ask_unsaved`).

`drop_gone_targets` (lines 897-926):

```rust
        for project_id in files {
            self.files.insert(project_id, new_files_view(project_id, None));
            // Not asked: the worktree is gone (as before); its files start over.
            self.close_file_leaves(project_id, window, cx);
            if shown == Some(project_id) && self.files_on_screen() {
                self.refresh_files(project_id, window, cx);
            }
        }
```

`files_preview.rs` and `files_split.rs` (all take `(project_id, path)`; every `self.active_file_tab(project_id)` becomes `self.doc_mut(project_id, path)`):

| Function | Change |
|---|---|
| `render_md_preview(project_id, tab: &Doc, split, window, theme, cx)` | `tab` -> `doc: &documents::Doc` plus `view: &DocView` read inside as `self.doc_view(project_id, &doc.path)`; `self.file_page` -> `view.page`; `self.file_preview_focus/file_editor_focus` -> `view.preview_focus/editor_focus` |
| `preview_press`, `preview_drag_to`, `file_block_wrap`, `open_block_edit`, `close_block_edit`, `apply_block_edit`, `click_block_edit` | add `path: &str` after `project_id`; `self.file_page` -> the pane's `view.page`; `window.focus(&self.file_editor_focus ..)` -> `self.focus_doc(project_id, path, DocFocus::Editor, window, cx)` |
| `file_preview_key_down`, `file_block_key_down` (event handlers) | `let Some((project_id, path)) = self.focused_doc(window) else { return };` instead of `self.workspace` |
| `set_file_mode(project_id, mode, window, cx)` | new signature `(project_id, path: &str, mode, window, cx)`; new body below |
| `toggle_file_preview`, `toggle_file_split` | `(project_id, path: &str, window, cx)` |
| `split_caret_to_block(project_id, block, cx)` | `(project_id, path, block, cx)` |
| `open_preview_link` | delete the `(true, View::Editor)` arm (line 292): a click on the file's own page resolves the file through `self.focused_file(window)`; `reveal_file(.., Open::Kept)` -> `self.reveal_file(project_id, target, path.clone(), window, cx)` then `open_document(.., Place::NewTab)` |
| `toggle_preview_task` | gate `self.view() == View::Editor` (line 354) -> `let Some((project_id, path)) = self.focused_doc(window) else { return };` |
| `open_chat_path` | unchanged (it calls `open_file_at`) |
| `follow_md_mode(mode)` | `self.md_mode = mode;` only (delete the `for view in self.files.values_mut()` loop) |
| `render_md_split`, `end_md_split_drag`, `follow_source_scroll` | `self.md_split_bounds` / `self.md_split_sync` / `self.file_page` -> the pane's `view.split_bounds` / `view.split_sync` / `view.page`; `shell.md_split_bounds.get()` inside listeners -> `shell.doc_view(project_id, &path).map(\|v\| v.split_bounds.get())` with `path` cloned into the closure |

`set_file_mode`:

```rust
    pub(crate) fn set_file_mode(
        &mut self,
        project_id: Ulid,
        path: &str,
        mode: MdMode,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let Some(doc) = self.doc_mut(project_id, path) else {
            return;
        };
        doc.set_mode(mode);
        if let Some(view) = self.doc_views.get_mut(&(project_id, path.to_owned())) {
            view.split_sync.followed.set(None);
        }
        // The leaf remembers the mode across restarts.
        let key = PaneKey::File(path.to_owned());
        let write = (self.tabs.get_mut(&project_id))
            .and_then(|tabs| tabs.update_leaf(&key, |leaf| documents::set_leaf_mode(leaf, mode)));
        if let Some(write) = write {
            self.save_layout(project_id, write, window, cx);
        }
        /* unchanged: the `[ui] md_mode` write (`if self.md_mode != mode { self.follow_md_mode(mode); .. }`) */
        self.build_preview(project_id, path, window, cx);
        let fields = self.doc_focused(project_id, path, DocFocus::Find, window)
            || self.doc_focused(project_id, path, DocFocus::Replace, window);
        if mode == MdMode::Preview && fields {
            window.focus(&self.focus_handle, cx);
        } else if mode != MdMode::Split && self.doc_focused(project_id, path, DocFocus::Preview, window) {
            self.focus_doc(project_id, path, DocFocus::Editor, window, cx);
        }
        cx.notify();
    }
```

### Step 15: `places.rs`: opening, placing, focusing

- [ ] **Step 15: Create `shell/places.rs`**

```rust
//! Opening a file or any leaf in the layouts: one way in for every entry point (SPEC §6.1).

use crate::*;

impl Shell {
    /// The only way a file opens (SPEC §6.1): where `place` says, or, when the file is open
    /// already in any tab or window, only its pane is focused. `line` is a link's
    /// (line, column), 1-based, shown in either case. Runs in the main window: from a
    /// detached one it hops there first.
    pub(crate) fn open_document(
        &mut self,
        project_id: Ulid,
        path: String,
        line: Option<(u32, u32)>,
        place: Place,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        if self.host_of(window) != HostId::Main {
            return self.in_window(HostId::Main, cx, move |shell, window, cx| {
                window.activate_window();
                shell.open_document(project_id, path, line, place, window, cx);
            });
        }
        let preview = place == Place::Preview;
        self.ensure_doc(project_id, &path, None, cx);
        self.doc_mut(project_id, &path).map(|doc| doc.set_line(line));
        let leaf = Leaf::File {
            file: path.clone(),
            md: None,
            preview,
        };
        // Terminals show the panes: Board, Docs and the Diff give way.
        self.views.insert(project_id, View::Terminals);
        if self.workspace != Some(project_id) {
            self.activate_workspace(project_id, window, cx);
        }
        self.place_leaf(project_id, leaf, place, window, cx);
        // Shown already: the line applies now; otherwise after the read.
        self.show_pending_line(project_id, &path);
        self.focus_file_pane(project_id, &path, window, cx);
    }

    /// Puts `leaf` in the layouts as `place` resolves (`documents::resolve_place`): it is
    /// focused where it is open already, replaces the preview tab, opens a new tab right after
    /// the active one, or splits a pane at an edge. Saves the layout.
    pub(crate) fn place_leaf(
        &mut self,
        project_id: Ulid,
        leaf: Leaf,
        place: Place,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let key = leaf.key();
        let Some(tabs) = self.tabs.get(&project_id) else {
            self.toast_error("The tabs of this workspace are still loading", cx);
            return;
        };
        let resolution = documents::resolve_place(tabs, self.documents.get(&project_id), &key, &place);
        match resolution {
            Resolution::Focus(key) => self.show_pane(project_id, &key, window, cx),
            Resolution::Reuse { old } => {
                let Some(tabs) = self.tabs.get_mut(&project_id) else {
                    return;
                };
                let Some(tab_id) = tabs.tab_with(&old).map(|tab| tab.id) else {
                    return;
                };
                tabs.activate(tab_id);
                // The new leaf takes the old one's place: insert beside it, then close it.
                let writes = [
                    tabs.insert_leaf(tab_id, Some(&old), Edge::Right, leaf),
                    tabs.close_leaf(&old),
                ];
                for write in writes.into_iter().flatten() {
                    self.save_layout(project_id, write, window, cx);
                }
                self.sync_panes(window, cx);
            }
            Resolution::NewTab { position } => {
                self.create_tab_then(
                    project_id,
                    position,
                    move |shell, tab_id, window, cx| {
                        let write = (shell.tabs.get_mut(&project_id))
                            .and_then(|tabs| tabs.insert_leaf(tab_id, None, Edge::Right, leaf));
                        if let Some(write) = write {
                            shell.save_layout(project_id, write, window, cx);
                        }
                        shell.sync_panes(window, cx);
                    },
                    window,
                    cx,
                );
            }
            Resolution::Split { target, edge } => {
                let Some(tabs) = self.tabs.get_mut(&project_id) else {
                    return;
                };
                let Some(tab_id) = tabs.tab_with(&target).map(|tab| tab.id) else {
                    return;
                };
                if let Some(write) = tabs.insert_leaf(tab_id, Some(&target), edge, leaf) {
                    self.save_layout(project_id, write, window, cx);
                }
                self.sync_panes(window, cx);
            }
        }
        cx.notify();
    }

    /// Brings the pane of `key` forward: its tab is selected (in a detached window, that
    /// window is raised, as `open_session` does), the pane focused.
    fn show_pane(
        &mut self,
        project_id: Ulid,
        key: &PaneKey,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let Some(tab_id) = (self.tabs.get(&project_id)).and_then(|tabs| tabs.tab_with(key)).map(|tab| tab.id)
        else {
            return;
        };
        self.focus_pane_key(key);
        match self.terminal_host(project_id, tab_id) {
            host @ HostId::Detached(window_key) => {
                let dock = DockTab::Terminal { project: project_id, tab_id };
                if let Some(at) = self.detached.tabs_of(window_key).iter().position(|t| *t == dock) {
                    self.detached.select(window_key, at);
                }
                self.windows_dirty = true;
                self.in_window(host, cx, move |shell, window, cx| {
                    window.activate_window();
                    shell.sync_panes(window, cx);
                    cx.notify();
                });
            }
            HostId::Main => {
                if let Some(tabs) = self.tabs.get_mut(&project_id) {
                    tabs.activate(tab_id);
                }
                self.sync_panes(window, cx);
            }
        }
    }

    /// The pane of `path` has the keyboard in its editor (a new file, a link, a note).
    pub(crate) fn focus_file_pane(
        &mut self,
        project_id: Ulid,
        path: &str,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        self.focus_pane_key(&PaneKey::File(path.to_owned()));
        self.focus_doc(project_id, path, DocFocus::Editor, window, cx);
    }

    /// A kept leaf: an edit, a double click or a pin ends the preview tab.
    pub(crate) fn keep_leaf(
        &mut self,
        project_id: Ulid,
        path: &str,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let key = PaneKey::File(path.to_owned());
        let write = (self.tabs.get_mut(&project_id))
            .and_then(|tabs| tabs.update_leaf(&key, documents::keep_leaf));
        if let Some(write) = write {
            self.save_layout(project_id, write, window, cx);
            cx.notify();
        }
    }

    /// The position a new tab of `project_id` takes: right after the active tab.
    pub(crate) fn position_after_active(&self, project_id: Ulid) -> Option<f64> {
        self.tabs.get(&project_id)?.position_after_active()
    }
}
```
`pub(crate) use documents::Resolution` is not needed; import with the `use mkrew_ui::documents::{self, Documents, MdMode, Place, Resolution};` line in `main.rs`.

`workspace.rs`: `create_tab_then` and `create_tab` as a wrapper:

```rust
    /// Adds a tab to a workspace and activates it; `start` (a kind, and an agent profile or
    /// the workspace's default of that kind) then spawns a session into it.
    pub(crate) fn create_tab(
        &mut self,
        project_id: Ulid,
        start: Option<(SessionKind, Option<String>)>,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        if let Some((kind, None)) = &start
            && self.ask_for_default(project_id, *kind, window, cx)
        {
            return;
        }
        self.create_tab_then(
            project_id,
            None,
            move |shell, _tab_id, window, cx| {
                if let Some((kind, profile)) = start
                    && shell.workspace == Some(project_id)
                {
                    shell.new_pane_with(kind, profile, Axis::Vertical, window, cx);
                }
            },
            window,
            cx,
        );
    }

    /// `tab.create` at `position` (`None`: last), the tab applied and activated, then `then`
    /// with its id. A failure toasts (or fails the workspace's first load) and `then` never
    /// runs.
    pub(crate) fn create_tab_then(
        &mut self,
        project_id: Ulid,
        position: Option<f64>,
        then: impl FnOnce(&mut Shell, Ulid, &mut Window, &mut Context<'_, Shell>) + 'static,
        window: &mut Window,
        cx: &mut Context<'_, Shell>,
    ) {
        let Some(connection) = self.connection.clone() else {
            return;
        };
        let count = self.tabs.get(&project_id).map_or(0, |tabs| tabs.tabs().len());
        let title = match count {
            0 => FIRST_TAB_TITLE.to_owned(),
            count => format!("{FIRST_TAB_TITLE} {}", count + 1),
        };
        let created = connection.call::<TerminalTab>(Request::TabCreate(TabCreateParams {
            project_id,
            title,
            position,
        }));
        cx.spawn_in(window, async move |this, cx| {
            let created = created.await;
            let _ = this.update_in(cx, |shell, window, cx| {
                let first = shell.loading_tabs.contains(&project_id);
                match created {
                    Ok(tab) => {
                        let tab_id = tab.id;
                        let tabs = (shell.tabs)
                            .entry(project_id)
                            .or_insert_with(|| Tabs::new(project_id));
                        // Its `tab.updated` echo is then a no-op.
                        tabs.apply(&Event::TabUpdated(tab));
                        tabs.activate(tab_id);
                        if first {
                            shell.tabs_loaded(project_id, window, cx);
                        } else {
                            shell.sync_panes(window, cx);
                        }
                        then(shell, tab_id, window, cx);
                    }
                    Err(error) => {
                        tracing::warn!(%project_id, "tab.create failed: {error:#}");
                        let error = format!("{error:#}");
                        if first {
                            shell.tabs_load_failed(project_id);
                            shell.list_failed(error);
                        } else {
                            shell.toast_error(format!("Cannot add a tab: {error}"), cx);
                        }
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }
```

### Step 16: Lazy load on first draw, documents follow the layouts

- [ ] **Step 16: `focus.rs`: `visible_files`, `sync_documents`, `sync_panes`, `focus_pane`, view helpers**

```rust
    /// Every file pane drawn now, with its leaf's Markdown mode: the main window's active
    /// tab (while it shows Terminals) and the visible detached tabs.
    pub(crate) fn visible_files(&self) -> Vec<(Ulid, String, Option<MdMode>)> {
        let mut shown = Vec::new();
        let mut add = |project: Ulid, tab: &Tab| {
            let Some(layout) = tab.layout() else {
                return;
            };
            for key in tab.shown() {
                if let PaneKey::File(path) = &key {
                    let md = match layout.leaf_of(&key) {
                        Some(Leaf::File { md, .. }) => *md,
                        _ => None,
                    };
                    shown.push((project, path.clone(), md));
                }
            }
        };
        if self.main_visible
            && self.view() == View::Terminals
            && let Some(project) = self.workspace
            && let Some(tab) = self.tabs.get(&project).and_then(Tabs::active)
        {
            add(project, tab);
        }
        for dock in self.detached.visible_tabs() {
            let DockTab::Terminal { project, tab_id } = dock;
            if let Some(tab) = self.tabs.get(&project).and_then(|tabs| tabs.tab(tab_id)) {
                add(project, tab);
            }
        }
        shown
    }

    /// The docs follow the layouts: one whose leaf left every layout goes unless it has unsaved
    /// text; a file pane about to be drawn has its doc and view state, and its file is read the
    /// first time (the `Doc` is `stale` until then). This is the "no tab, no read" fix: the read
    /// no longer hangs on a tab existing in a strip.
    pub(crate) fn sync_documents(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        for (project, tabs) in &self.tabs {
            // Tabs still loading list no leaves yet: nothing may be dropped.
            if self.loading_tabs.contains(project) {
                continue;
            }
            if let Some(docs) = self.documents.get_mut(project) {
                docs.retain_open(&documents::open_paths(tabs));
            }
        }
        let documents = &self.documents;
        self.doc_views
            .retain(|(project, path), _| documents.get(project).is_some_and(|docs| docs.get(path).is_some()));
        for (project, path, md) in self.visible_files() {
            self.ensure_doc(project, &path, md, cx);
            if self.doc_mut(project, &path).is_some_and(documents::Doc::take_read) {
                self.read_file(project, path, window, cx);
            }
        }
    }
```
`sync_panes` starts with `self.keep_main_active_visible(); self.prune_detached(cx); self.sync_documents(window, cx);`.

`focus_pane` (the `match self.focused_session(window).and_then(..)` at the end and the chat branch):

```rust
        // A file pane keeps the keyboard in its editor.
        if let Some((project_id, path)) = self.focused_doc(window) {
            match self.doc_view(project_id, &path) {
                Some(view) => window.focus(&view.editor_focus, cx),
                None => window.focus(&self.focus_handle, cx),
            }
            return;
        }
```
placed before the `// A pane in Chat mode` block. (Task 5 adds the Diff branch beside it.)

Other `focus.rs` edits: `panel_view` -> `matches!(self.view(), View::Terminals | View::Diff)`; `files_on_screen` -> `self.file_tree_shown() || !self.visible_files().is_empty()`; `panel_session` unchanged.
`workspace.rs`: `view()` filter loses the `View::Editor` arm; delete `has_file_tabs`; `show_view` loses the `if view == View::Editor` focus tail (lines 462-471). `inspector.rs:134`: `if let Some((project_id, path)) = self.focused_doc(window) { self.focus_doc(project_id, &path, DocFocus::Editor, window, cx); } else { self.focus_pane(window, cx); }`; `inspector.rs:53` loses its `View::Editor` arm. `sync_watches` (workspace.rs:476-481): `files` target stays from `self.files`, gated on `files_on_screen()` as is.

### Step 17: Tab chip, tab area, pane frame

- [ ] **Step 17: `pane_render.rs`**

`render_tab_area`: remove `view_chip`'s Editor use; `area_items(self.diff_open(project_id), &pinned)`; `AreaItem::View(chip)` stays for the Diff chip only. `view_chip` keeps `View::Diff` only: delete the `_ => (Icon::Code, "Close all files")` arm and the `_ => { .. close_file_tabs(.. Close::Every ..) }` arm (replace `match` by the `View::Diff` body: `shell.close_git_diff(project_id, window, cx)`; unreachable views cannot occur: take `chip: View` and `debug_assert_eq!(chip, View::Diff)`).

`tabs.rs` (`area_items` and its test):

```rust
/// The children of the tab area: the Diff chip (while open), a rule, the tabs (`pinned[n]` is
/// whether the `n`th drawn tab is pinned; pinned ones lead), and a rule between the pinned
/// tabs and the rest. A rule only sits between two groups.
pub fn area_items(diff: bool, pinned: &[bool]) -> Vec<AreaItem> {
    let mut items = Vec::new();
    if diff {
        items.push(AreaItem::View(View::Diff));
    }
    /* the `for` loop, unchanged */
    items
}
```
`tabs/tests.rs:1285, 1306`: delete the `editor` argument and the Editor expectations (the remaining cases keep the Diff chip, rules, tabs).

`tab_chip`: the title, italics and dot. Replace the `label` and `title` lines and the title child:

```rust
        // A focused file pane names the tab (a dot while unsaved, italic while it is the
        // preview tab); else the focused pane's session title stands in for a default name.
        let file = file_label(tab, self.documents.get(&project_id));
        let label = file
            .as_ref()
            .map(|file| file.name.as_str())
            .or_else(|| session_label(tab, self.sidebar.sessions()));
```
`pane_icons` match gets:

```rust
                PaneIcon::File(_) => icon_svg(Icon::File, theme::ICON_SIZE_SMALL, theme.text_muted)
                    .into_any_element(),
```
and the title child:

```rust
            .child(
                div()
                    .min_w_0()
                    .max_w(px(theme::TAB_TITLE_MAX_WIDTH))
                    .truncate()
                    .when(file.as_ref().is_some_and(|file| file.preview), |el| el.italic())
                    .child(format!(
                        "{}{}",
                        if file.as_ref().is_some_and(|file| file.dirty) { "● " } else { "" },
                        label.unwrap_or(&tab.title)
                    )),
            )
```
The tooltip `title` for a dirty file gets `" (unsaved)"` appended when `file.dirty` (color or a dot is never the only signal): `let title = ...; let tip = if dirty { format!("{title} (unsaved)") .. }` in the existing `tip` expression. The tab-click handler's `shell.show_view(View::Terminals, ..)` comment loses "the Editor". `render_tab_area`'s `DraggedItem.title` also uses `file.map(|f| f.name)`: `let file = file_label(tab, self.documents.get(&project_id)); let label = file.as_ref().map(|f| f.name.as_str()).or_else(|| session_label(..));` (and the same in `detached_render.rs:171`).

`render_tab` and `pane_frame` take `project_id` first and `window` before `cx`:

```rust
    pub(crate) fn render_tab(
        &self,
        project_id: Ulid,
        tab: Option<&Tab>,
        now: i64,
        composer_focused: bool,
        voice_ring: Option<Hsla>,
        theme: &Theme,
        content: Rc<Cell<gpui::Size<Pixels>>>,
        window: &Window,
        cx: &mut Context<'_, Self>,
    ) -> gpui::Div
```
Callers: `shell/chrome.rs:224` (`self.render_tab(project_id, ..., window, cx)`; `render_main` already has `window`) and `shell/detached_render.rs:63` (the detached tab's project is in its `DockTab::Terminal { project, .. }`). In the panes loop: `rects` are `(PaneKey, Rect)`; `is_focused = focused == Some(&key)`; the call is `self.pane_frame(project_id, key, several, zoomed, width, is_focused, is_focused && composer_focused, voice_ring.filter(|_| is_focused), now, theme, window, cx)`.

`pane_frame`: signature as in the interface block. Body edits (Task 2 already removed the border layer):

```rust
        let session = key.session();
        let info = session.and_then(|id| self.sidebar.sessions().iter().find(|s| s.id == id));
        let content = match &key {
            PaneKey::Session(id) => {
                if info.is_some_and(|info| self.chat_on(info)) {
                    self.render_chat(*id, is_focused, now, theme, cx)
                } else {
                    match self.panes.get(id) {
                        Some(pane) => pane.clone().into_any_element(),
                        None => {
                            let (sentence, detail) = self.pane_placeholder(*id);
                            message(theme, None, sentence, detail).size_full().into_any_element()
                        }
                    }
                }
            }
            PaneKey::File(path) => self.render_file_pane(project_id, path, window, theme, cx),
            // Task 5 draws the diff pane here.
            PaneKey::Diff => message(theme, None, "Diff".into(), None).size_full().into_any_element(),
        };
```
Session-only layers apply only to a session key: `.on_drop(.. DraggedPath ..)` (the tree-row drop on a terminal) stays for sessions only: wrap in `.when_some(session, |el, session| el.on_drop(...))`; `.children(session.and_then(|s| self.composers.shown(s).map(..)))`; `drop_catcher(DropTarget::Pane(session), ..)` and the `ExternalPaths` drag-move only `when_some(session, ..)`; `pane_badges`, `pane_edge`, the session `color` stripe and `reading` via `info`; `pane_drop_target`, `pane_drop_overlay` and `draggable_pane` stay session-only until Task 6 generalizes them (a file pane has no drag strip or drop zones in this task). Mouse down:

```rust
            .on_mouse_down(MouseButton::Left, {
                let key = key.clone();
                cx.listener(move |shell, _, window, cx| {
                    shell.focus_pane_key(&key);
                    shell.focus_pane(window, cx);
                    cx.notify();
                })
            })
```
`EDITOR_CONTEXT` goes on the file pane's own element (the root div stops carrying it):

```rust
        let frame = div()
            .relative()
            .size_full()
            .flex()
            .flex_col()
            .bg(rgb(theme.terminal_bg))
            .when(matches!(key, PaneKey::File(_)), |el| el.key_context(EDITOR_CONTEXT))
```
`main.rs:1046`: delete the `.when(self.view() == View::Editor, |el| el.key_context(EDITOR_CONTEXT))` lines; delete the `file_tab_menu` block at 1269-1275. The pane's `pane_frame` doc comment gains "A file pane is the editor (`render_file_pane`)".

`pin_tab` (shell/tabs.rs:414): before `send_edit`, a pinned tab keeps its preview file:

```rust
        if pinned {
            let keys: Vec<(Ulid, PaneKey)> = self
                .tabs
                .iter()
                .filter_map(|(project, tabs)| Some((*project, tabs.tab(tab_id)?.keys())))
                .flat_map(|(project, keys)| keys.into_iter().map(move |key| (project, key)))
                .collect();
            for (project, key) in keys {
                if let PaneKey::File(path) = key {
                    self.keep_leaf(project, &path, window, cx);
                }
            }
        }
```

### Step 18: `file_pane.rs`: the pane's content

- [ ] **Step 18: Create `shell/file_pane.rs`**

Move `render_file`, `render_shown_file` and `margin_shown` out of `files_render.rs` (lines 256-918 before this task) into `file_pane.rs` (`use crate::*;`), delete `render_editor` (lines 6-28), and rewrite the head of `render_file`. `files_render.rs` keeps the tree (`render_file_tree`, `render_new_entry`, `file_row`, `FileRowState`, `DraggedPath`, `size_label`).

The new entry (replaces `render_file`'s signature, the `panel` and the "No file open" empty state; the old empty state goes with the Editor view):

```rust
    /// The pane of an open file (SPEC §12A): the header (path with `●` while unsaved,
    /// `Ln, Col`, Markdown modes, Save, Find, Open in Zed, close), the find bar, then the
    /// editor, the preview or the split, or why it is not shown. Lives in `pane_frame`.
    pub(crate) fn render_file_pane(
        &self,
        project_id: Ulid,
        path: &str,
        window: &Window,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> gpui::AnyElement {
        let (Some(doc), Some(view)) = (self.doc(project_id, path), self.doc_view(project_id, path))
        else {
            // The doc is made by the next `sync_panes`, which also starts its read.
            return message(theme, None, "Loading…".into(), None)
                .size_full()
                .into_any_element();
        };
        let target = self.files.get(&project_id).and_then(|files| files.worktree_task_id);
        let panel = div()
            .size_full()
            .flex()
            .flex_col()
            .rounded(px(radius::MEDIUM))
            .overflow_hidden()
            .font_family(theme.ui_font_family.clone())
            .bg(rgb(theme.surface));
        let path = path.to_owned();
        /* header, find_bar, body, disk_banner as below */
        panel
            .child(header)
            .children(find_bar)
            .children(disk_banner)
            .children(view.zed_error.clone().map(|error| {
                notice(theme.text, format!("✕ {error}")).px(px(space::L)).pb(px(space::S))
            }))
            .children(doc.shown().and_then(|shown| shown.save_error.clone()).map(|error| {
                notice(theme.text, format!("✕ {error}")).px(px(space::L)).pb(px(space::S))
            }))
            .child(body)
            .into_any_element()
    }
```
The moved header, find bar, body and banner keep their code with these substitutions (all line numbers of the old `render_file`):

| Old | New |
|---|---|
| `tab` (the active `FileTab`) | `doc` |
| `view.tab()` / `view.find` | `doc` / `doc.find` |
| `view.worktree_task_id` (438) | `target` |
| `view.zed_error` (764) | `view.zed_error` (the `DocView`'s) |
| `shell.set_file_mode(project_id, mode, window, cx)` (383) | `shell.set_file_mode(project_id, &path, mode, window, cx)` with `path` cloned into the closure |
| `shell.open_file_in_zed(project_id, cx)` (398) | `shell.open_file_in_zed(project_id, &path, cx)` |
| `shell.open_file_find(false, window, cx)` (493) | unchanged (it resolves the focused doc) |
| every `self.file_find_focus` / `self.file_replace_focus` (502-653) | `view.find_focus` / `view.replace_focus` |
| every `shell.file_find_focus` in listeners (534, 559, 574, 582) | `shell.focus_doc(project_id, &path, DocFocus::Find, window, cx)`; replace likewise with `DocFocus::Replace` |
| `shell.files_view(project_id).find` (526, 531, 548, 610) | `shell.doc_mut(project_id, &path).and_then(\|doc\| doc.find.as_mut())` (and `.as_ref()` for the read at 526) |
| `shell.step_file_find(project_id, false, cx)` (575, 583) | `shell.step_file_find(project_id, &path, false, cx)` |
| `shell.close_file_find(project_id, window, cx)` (590) | `shell.close_file_find(project_id, &path, window, cx)` |
| `shell.replace_in_file`, `shell.replace_all_in_file` (642, 654) | add `&path` |
| `shell.save_file(project_id, path.clone(), None, ..)` (474) | unchanged |
| `.on_key_down(cx.listener(Self::file_find_key_down))` and `typing(&focus, Self::..)` | unchanged (the handlers find their doc from the focus) |
| the Review in Docs button (437-455) | unchanged apart from `target` |
| disk banner Reload (737-741) | `shell.reload_file(project_id, &path, window, cx)` (new, below) |
| disk banner Keep mine (752) | `shell.doc_mut(project_id, &path).map(\|doc\| doc.keep_mine());` |
| `render_md_preview(project_id, tab, false, ..)`, `render_md_split(project_id, tab, shown, ..)`, `render_shown_file(project_id, shown, ..)` | pass `doc`; `render_shown_file(project_id, doc, shown, ..)` |
| `render_shown_file`: `self.file_editor_focus`, `file_text_left`, `file_code_scroll`, `file_code_drag` | `view.editor_focus`, `view.text_left`, `view.code_scroll`, `view.code_drag` (`view` is `self.doc_view(project_id, &doc.path)`) |
| `render_shown_file` processor: `shell.files.get(&project_id)` + `view.shown()` + `view.find` (801-809) | `shell.doc(project_id, &path)` + `doc.shown()` + `doc.find` (`path` cloned into the processor) |
| `render_shown_file` listeners: `shell.click_file_text(project_id, index, x, extend, ..)` (856, 865) and `shell.file_editor_focus.is_focused(window)` (862) | `shell.click_file_text(project_id, &path, index, x, extend, ..)` and `shell.doc_focused(project_id, &path, DocFocus::Editor, window)` |
| the editor `div`: `.track_focus(&self.file_editor_focus)` / the mouse-down focus (899) | `view.editor_focus` / `shell.focus_doc(.., Editor, ..)` |
| `margin_shown(doc: &Doc)` (the Docs view's helper, `docs::Doc`) | stays in `files_render.rs` untouched (it is about `docs::Doc`) |

New header pieces: the close button, placed last in the header after `zed_button`:

```rust
            .child(
                icon_button("file-close", Icon::X, theme)
                    .tooltip(tooltip(theme, "Close file", self.keymap.label(Command::CloseFileTab)))
                    .on_click({
                        let path = path.clone();
                        cx.listener(move |shell, _, window, cx| {
                            shell.close_file_pane(project_id, path.clone(), window, cx);
                        })
                    }),
            )
```
The body arms for the failure cases (replace the `Content::Failed(error)` arm):

```rust
            Content::Failed(_) if doc.removed() => message(theme, None, "This file was removed.".into(), None)
                .flex_1()
                .child(
                    button("file-removed-close", "Close", theme, true).on_click({
                        let path = path.clone();
                        cx.listener(move |shell, _, window, cx| {
                            shell.close_file_pane(project_id, path.clone(), window, cx);
                        })
                    }),
                )
                .into_any_element(),
            Content::Failed(error) => message(
                theme,
                None,
                "Cannot read this file.".into(),
                Some(error.clone().into()),
            )
            .flex_1()
            .child(button("file-retry", "Retry", theme, true).on_click({
                let path = path.clone();
                cx.listener(move |shell, _, window, cx| {
                    shell.retry_read(project_id, &path, window, cx);
                })
            }))
            .into_any_element(),
```
(`message(..)` returns a column that accepts children, as the old Retry arm did.) The `Retry` and `Close` buttons are the existing `button(..)` widget (pressable); no new hover code.

`reload_file` (shell/files.rs, replaces the inline Reload closure):

```rust
    /// "Reload" of the changed-on-disk banner: the unsaved changes go and the file is read.
    pub(crate) fn reload_file(
        &mut self,
        project_id: Ulid,
        path: &str,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let read = (self.documents.get_mut(&project_id)).and_then(|docs| docs.drop_changes(path));
        if let Some(path) = read {
            self.read_file(project_id, path, window, cx);
        }
    }
```
`close_file_pane` is in step 20.

### Step 19: Commands, keys and the palette

- [ ] **Step 19: `sessions.rs`, `keymap.rs`, `palette.rs`, `navigation.rs` context**

`sessions.rs::run`:

1. Find-field guard (lines 216-241):

```rust
        // Same for a file pane's find / replace fields while they have the keyboard (Esc gives
        // it back), except the editor commands (Ctrl+S, Ctrl+F, Ctrl+H act from there).
        let field = (self.focused_doc(window)).filter(|(project_id, path)| {
            self.doc_focused(*project_id, path, DocFocus::Find, window)
                || self.doc_focused(*project_id, path, DocFocus::Replace, window)
        });
        if let Some((project_id, path)) = field {
            let open = (self.doc(project_id, &path))
                .is_some_and(|doc| doc.find.is_some() && !doc.previewing());
            let files_command = matches!(
                command,
                Command::SaveFile
                    | Command::FindInFile
                    | Command::ReplaceInFile
                    | Command::CloseFileTab
                    | Command::ToggleFilePreview
                    | Command::ToggleFileSplit
            );
            if open && !files_command {
                cx.propagate();
                return;
            }
            if !open {
                window.focus(&self.focus_handle, cx);
            }
        }
```
2. Delete the `// Tab keys act on the Editor's tabs` block (295-310): `NextTab`, `PreviousTab`, `Tab(n)`, `CloseTab`, `MoveTabLeft/Right` act on the general tabs (which include file tabs) and `CloseFileTab` is below.
3. `OpenInZed` (411): before the `match self.workspace`:

```rust
            Command::OpenInZed => {
                if let (Some(project_id), Some(path)) = (self.workspace, self.focused_file(window)) {
                    return self.open_file_in_zed(project_id, &path, cx);
                }
                match self.workspace { /* the existing arms, minus the `View::Editor` one (423-431) */ }
            }
```
4. `SaveFile` (530-539):

```rust
            Command::SaveFile => {
                // Only bound in a file pane (`EDITOR_CONTEXT`).
                if let Some((project_id, path)) = self.focused_doc(window) {
                    self.save_file(project_id, path, None, window, cx);
                }
            }
```
5. `OpenKept` (549): `self.click_file(project_id, &cursor, true, window, cx);`.
6. `ToggleFilePreview` / `ToggleFileSplit`: `if let Some((project_id, path)) = self.focused_doc(window) { self.toggle_file_preview(project_id, &path, window, cx); }`.
7. `Command::Find` (663-670): `if self.focused_file(window).is_some() { self.open_file_find(false, window, cx) } else if self.view() == View::Docs { window.focus(&self.docs_filter_focus, cx); cx.notify(); }`.
8. `Command::CloseFileTab => self.close_focused_pane(window, cx),` (was `{}`); delete `Command::ShowEditor` (471-475).
9. `close_focused_pane` (shell/tabs.rs:757):

```rust
    pub(crate) fn close_focused_pane(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        match self.focused_key(window) {
            Some(PaneKey::Session(session)) => self.close_pane(session, window, cx),
            Some(PaneKey::File(path)) => {
                if let Some(project_id) = self.tab_scope(window).map(|(project, _)| project) {
                    self.close_file_pane(project_id, path, window, cx);
                }
            }
            // Task 5 closes the diff here.
            Some(PaneKey::Diff) | None => {}
        }
    }
```

`keymap.rs`: delete `Command::ShowEditor` from the enum (131), `all()` (341), `name()` (453), the mac keys (582), the PC keys (680), the doc comment; the test at 1537 loses its `ctrl-shift-6` line; update the context tests at 1294-1320, 1471-1474, 1537-1546, 1733, 1743-1747 only where they name `ShowEditor` or "Editor view". `ctrl-shift-6` becomes free. `Command::CloseFileTab` keeps its doc: "File pane only: close it (Ctrl+W)".
`board.rs`: delete `View::Editor` from the enum, `ALL` (`[View::Terminals, View::Board, View::Docs, View::Diff]`), `title`, `command`; `board/tests.rs:8-18` expects `[ShowTerminals, ShowBoard, ShowDocs, ShowDiff]`.
`palette.rs`: delete 651 (the label) and 758 (`ShowEditor` availability); rewrite:

```rust
        Command::Find => {
            (on_panes && context.terminal) || context.file_pane || in_view(View::Docs)
        }
        Command::SendSelection => {
            context.file_pane
                || in_view(View::Diff)
                || in_view(View::Docs)
                || (on_panes && context.terminal)
        }
        Command::MoveTabLeft | Command::MoveTabRight => on_panes,
        Command::SaveFile
        | Command::FindInFile
        | Command::ReplaceInFile
        | Command::CloseFileTab
        | Command::ToggleFilePreview
        | Command::ToggleFileSplit => context.file_pane,
```
`Context` loses `file_tabs: usize`, gains `file_pane: bool`. `navigation.rs:316-333`:

```rust
            tabs: match (window_key, workspace) {
                (Some(key), _) => self.detached.tabs_of(key).len(),
                (None, Some(id)) => self.tabs.get(&id).map_or(0, |tabs| {
                    let main = |tab: &&Tab| self.terminal_host(id, tab.id) == HostId::Main;
                    tabs.tabs().iter().filter(main).count()
                }),
                (None, None) => 0,
            },
            file_pane: self.focused_file(window).is_some(),
```
`palette/tests.rs` 385-403 (ShowEditor / `file_tabs`), 463 and 1324: delete the ShowEditor assertions; the `context(..)` helper drops `file_tabs` and gets `file_pane: false`; add:

```rust
#[test]
fn the_file_commands_need_a_focused_file_pane() {
    let mut on_file = context(View::Terminals);
    on_file.file_pane = true;
    let offered = commands(&on_file);
    for command in [Command::SaveFile, Command::FindInFile, Command::ReplaceInFile, Command::CloseFileTab, Command::ToggleFilePreview, Command::ToggleFileSplit, Command::SendSelection] {
        assert!(offered.contains(&command), "{command:?} acts on the focused file pane");
    }
    let elsewhere = commands(&context(View::Terminals));
    assert!(!elsewhere.contains(&Command::SaveFile));
    assert!(!elsewhere.contains(&Command::FindInFile));
}
```
`composer.rs` and `send_dialog.rs` per the shapes agreed with Task 5:

```rust
    pub(crate) fn picked_file_lines(&self, project_id: Ulid, path: &str) -> Option<(String, u32, u32, String)> {
        self.doc(project_id, path)?.picked_lines()
    }
```
`picked_lines(project_id, view)` loses the `View::Editor` arm; `prompt_vars`: `file` from `self.panel_file(project_id)` (then the git selection), and the selection from `[diff pick, file pick]` / `[file pick, diff pick]` by `self.view()`; `open_selection_dialog`:

```rust
        let file = self.focused_file(window);
        let (file_path, prefill, task) = match self.view() {
            View::Terminals if file.is_some() => {
                let picked = file
                    .as_deref()
                    .and_then(|path| self.picked_file_lines(project_id, path))
                    .filter(|(_, _, _, text)| !text.trim().is_empty());
                let Some((path, start, end, text)) = picked else {
                    return;
                };
                let prefill = composer::selection_prefill(Some((&path, start, end)), &text);
                (format!("{path}:{start}-{end}"), prefill, None)
            }
            view @ (View::Diff | View::Docs) => { /* the existing body */ }
            View::Terminals => { /* the existing terminal body */ }
            _ => return,
        };
```

### Step 20: Closing and the save question

- [ ] **Step 20: Every way a file leaves the layouts asks; moves never do**

`shell/tabs.rs`: `remove_leaf` (the body of today's `remove_pane`, for any key) and the thin wrappers.

```rust
    /// Takes the pane of `key` out of its tab's layout (a session itself is untouched). A tab of
    /// the shown workspace left without panes closes too. No question: a caller that closes
    /// (rather than moves) a file asks first (`close_file_pane`, `close_tabs`).
    pub(crate) fn remove_leaf(
        &mut self,
        key: PaneKey,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let closed = self
            .tabs
            .values_mut()
            .find_map(|tabs| Some((tabs.project_id(), tabs.close_leaf(&key)?)));
        if let Some((project_id, write)) = closed {
            let emptied = write.layout.is_null().then_some(write.tab_id);
            // Saved first: requests go out in call order, so `tab.close` sees no panes and
            // leaves a hidden session alive.
            self.save_layout(project_id, write, window, cx);
            // A tab in a detached window closes too, whichever workspace the main window shows.
            if let Some(tab_id) = emptied
                && (self.workspace == Some(project_id)
                    || self.terminal_host(project_id, tab_id) != HostId::Main)
            {
                self.close_tab(project_id, tab_id, window, cx);
            }
        }
        self.sync_panes(window, cx);
        cx.notify();
    }

    pub(crate) fn remove_pane(&mut self, session: Ulid, window: &mut Window, cx: &mut Context<'_, Self>) {
        self.remove_leaf(PaneKey::Session(session), window, cx);
    }

    /// The files of `project_id` that would be lost, with unsaved text, if the tabs `tab_ids`
    /// and the leaves `keys` closed (moves leave them open and are not asked about).
    pub(crate) fn dirty_on_close(&self, project_id: Ulid, tab_ids: &[Ulid], keys: &[PaneKey]) -> Vec<String> {
        let (Some(tabs), Some(docs)) = (self.tabs.get(&project_id), self.documents.get(&project_id)) else {
            return Vec::new();
        };
        documents::needs_save_question(
            &documents::open_paths(tabs),
            &documents::open_paths_without(tabs, tab_ids, keys),
            docs,
        )
    }

    /// Closes the pane of file `path` (its ✕, Ctrl+W, the "removed" Close): unsaved text asks
    /// Save / Don't save / Cancel first.
    pub(crate) fn close_file_pane(
        &mut self,
        project_id: Ulid,
        path: String,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let key = PaneKey::File(path);
        let dirty = self.dirty_on_close(project_id, &[], std::slice::from_ref(&key));
        let then: AfterSave = Box::new(move |shell, window, cx| shell.remove_leaf(key, window, cx));
        self.leave_documents(project_id, dirty, then, window, cx);
    }
```
`close_tabs` (shell/tabs.rs:652): at the top, before reading `connection`:

```rust
        // A file with unsaved text in a tab being closed asks first; the question re-runs
        // this close once every one was answered (a Cancel stops it).
        let asked = (tabs.iter().map(|(project, _)| *project).collect::<Vec<_>>()).into_iter().find_map(|project| {
            let ids: Vec<Ulid> = (tabs.iter()).filter(|(p, _)| *p == project).map(|(_, tab)| *tab).collect();
            let dirty = self.dirty_on_close(project, &ids, &[]);
            (!dirty.is_empty()).then_some((project, dirty))
        });
        if let Some((project_id, dirty)) = asked {
            let again: AfterSave = Box::new(move |shell, window, cx| shell.close_tabs(tabs, window, cx));
            return self.leave_documents(project_id, dirty, again, window, cx);
        }
```
This one place covers the tab ✕, "Close others" and "Close tabs to the right" (`close_other_tabs`), the `CloseTab` command and the ✕ of a detached window (`detached_should_close` calls `close_tabs`). Not asked, by design: `remove_leaf` for a session, `Tabs::move_pane`, `merge_tab`, tear-off and drag between windows (Task 6 must not route a move through `close_tabs` or `close_tab`; it saves layouts only), and the Preview replacement, which `resolve_place` allows only for a clean doc and an edit ends the preview (`keep_leaf`). Quit, the main window's close and an update restart go through `ask_unsaved` (step 14). There is no UI path that drops a workspace's tabs besides a disconnect (dirty docs are kept, step 13) and a reconnect to a daemon that no longer lists the workspace (as today, its unsaved text cannot be saved anywhere).

Tests for the shell-free parts of this step are the `documents` ones of step 7 (`closing_a_leaf_with_unsaved_text_asks_and_moving_it_never_does`, `closing_tabs_and_panes_are_told_from_moves_by_the_remaining_open_set`, `an_edit_flips_the_preview_leaf_to_kept_once`, `a_preview_with_unsaved_text_is_not_replaced`).

### Step 21: Every entry point goes through `open_document` (the 11 sites)

- [ ] **Step 21: Rewire**

`navigation.rs`:

```rust
    pub(crate) fn open_file_at(
        &mut self,
        project_id: Ulid,
        target: Option<Ulid>,
        path: String,
        line: Option<u32>,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        if self.files_view(project_id).worktree_task_id != target {
            let unsaved = (self.documents.get(&project_id)).map_or_else(Vec::new, Documents::unsaved_paths);
            if !unsaved.is_empty() {
                let again: AfterSave = Box::new(move |shell, window, cx| {
                    shell.open_file_at(project_id, target, path, line, window, cx);
                });
                return self.leave_documents(project_id, unsaved, again, window, cx);
            }
            self.set_files_target(project_id, target, window, cx);
        }
        self.reveal_file(project_id, target, path.clone(), window, cx);
        self.open_document(project_id, path, line.map(|line| (line, 1)), Place::NewTab, window, cx);
    }

    /// Shows `path` in the tree of `project_id` (its folders open, the cursor lands on it). It
    /// does not open the file.
    pub(crate) fn reveal_file(
        &mut self,
        project_id: Ulid,
        target: Option<Ulid>,
        path: String,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        // Revealed only while the tree still shows the folder that was searched.
        let Some(view) = self.files_at(project_id, target) else {
            return;
        };
        let requests = view.reveal(&path);
        let at = view.rows().iter().position(
            |row| matches!(row, files_view::Row::Entry { entry, .. } if entry.path == path),
        );
        if let Some(at) = at {
            self.files_scroll.scroll_to_item(at, ScrollStrategy::Nearest);
        }
        self.files_requests(project_id, requests, window, cx);
    }
```
Delete `show_pending_line` here (it moved, step 14) and its `file_code_scroll` use (`navigation.rs:200-212`).

| # | Entry point | Site (before this task) | After |
|---|---|---|---|
| 1 | Files tree click and double click | `files_render.rs:1088` `file_row` on_press; `click_file` | `shell.click_file(project_id, &path, event.click_count() >= 2, window, cx)` (was `Open::Kept` on double click, `Open::Transient` otherwise); `click_file` calls `open_document(.., Preview)` / `(.., NewTab)` + `keep_leaf` |
| 2 | Enter in the tree | `files.rs:271` | `click_file(project_id, &cursor, false, ..)` (preview) |
| 3 | `OpenKept` (Alt+Enter, sidebar/sessions list keyboard open) | `sessions.rs:549` | `click_file(project_id, &cursor, true, ..)` (kept) |
| 4 | Retry after a read error | `files_render.rs:679` | `shell.retry_read(project_id, &path, ..)` (reloads in place, opens nothing) |
| 5 | The save question showing its file | `files.rs:283` `open_file_tab(.. Transient ..)` | `leave_documents` calls `open_document(.., Place::NewTab)`: focuses the leaf wherever it is |
| 6 | Ctrl+click on a terminal path, link hints | `navigation.rs:152-200` `open_terminal_link` -> `open_file_at` | unchanged call; `open_file_at` now ends in `open_document(.., Place::NewTab)` with the `(line, 1)` |
| 7 | Palette Files scope and Search hits | `navigation.rs:1111-1126` `open_palette_file` -> `open_file_at` | unchanged (it already hops to the main window) |
| 8 | Links in the Markdown preview and Docs | `files_split.rs:333, 337` `reveal_file(.., Open::Kept)` / `open_file_at` | `self.reveal_file(project_id, target, path.clone(), window, cx); self.open_document(project_id, path, line.map(\|l\| (l, 1)), Place::NewTab, window, cx);` for a file in the shown tree; `open_file_at` otherwise |
| 9 | Path pills in Chat messages | `files_split.rs:394-421` `open_chat_path` | unchanged (`open_file_at`) |
| 10 | A file created in the tree | `workspace_actions.rs:555-578` `show_created_entry` | `self.reveal_file(project_id, target, path.clone(), window, cx); self.open_document(project_id, path, None, Place::NewTab, window, cx);` |
| 11 | `Command::ShowDiff` and "open diff" | not here | Task 5 (`open_diff(NewTab)`); this task keeps `View::Diff` as it is |

The docs comments of `open_terminal_link` / `open_file_at` / `open_palette_file` / `open_chat_path` say "in the Editor view"; change to "as a file tab".
`FileRowState.opened` (tree highlight) = `self.focused_file(window)` in `render_file_tree` (it has `window`); `view.file()` there is gone.
`menu_render.rs`, `menu.rs`: delete `Owner::FileTab`, `Focus.file_tab`, the `FileTabMenu` rows (`file_tab_menu_rows`) and their tests (`menu.rs:477, 531, 550`); `menu_render.rs:325, 359-360, 630-690`. (The file tab strip menu had Close, Close others, Close to the right, Close all, Pin and Copy path; the terminal tab menu now covers close and pin for file tabs; "Copy path" is dropped.)
`reorder.rs`: delete `ReorderList::FileTabs` (line 18) and its arm (179-181) and the Editor branch of `move_active_tab` (242-255): `move_active_tab` is the general one. `main.rs`: delete `struct FileTabMenu` (425) and the field (582).
`redraw.rs:214, 460, 490, 528`: delete the `View::Editor` cases and the tests pinning them.

### Step 22: Delete what the cutover made dead

- [ ] **Step 22: Deletions**

```
git rm crates/mkrew-ui/src/shell/file_tabs.rs crates/mkrew-ui/src/shell/file_tabs_render.rs
```
Delete, in the remaining files: `follow_file_tabs`, `has_file_tabs`, `file_tab_shown`, `open_file_tab`, `cycle_file_tab`, `nth_file_tab`, `close_file_tabs`, `close_active_file_tab`, `pin_file_tab`, `active_file_tab` (all in the deleted files, listed so none is re-added), `FilesView` `Open`/`Close`, `render_editor`, `render_file_tabs`, the `View::Editor` arm of `render_main` (`chrome.rs:187`), `AreaItem` editor chip, `Close::Every`.
Final check that nothing is left:

```
grep -rn "View::Editor\|FileTab\b\|ReorderList::FileTabs\|follow_file_tabs\|has_file_tabs\|Open::Transient\|Open::Kept\|file_tab_menu\|ShowEditor" crates/mkrew-ui/src
```
Expected: no output.

### Step 23: Compile and fix the leftovers

- [ ] **Step 23: Build**

Run: `cargo check -p mkrew-ui --all-targets -j 4`
Expected after this step: PASS. Typical leftovers (fix, do not add shims): a `focused_session` that now must read `focused_key` (Task 1 notes); `self.files.get(..)` calls that only wanted `file()` (use `focused_file` / `panel_file`); `menu_render.rs:655` (`file_tab: files.and_then(|v| v.file())`) goes with the `Focus.file_tab` field.

### Step 24: Gates

- [ ] **Step 24: Run the tests**

```
cargo fmt --all
cargo clippy -p mkrew-ui --all-targets -j 4 -- -D warnings
cargo test -p mkrew-ui documents -j 4
cargo test -p mkrew-ui files_view -j 4
cargo test -p mkrew-ui tabs -j 4
cargo test -p mkrew-ui palette -j 4
cargo test -p mkrew-ui keymap -j 4
cargo test -p mkrew-ui board -j 4
cargo test -p mkrew-ui menu -j 4
cargo test -p mkrew-ui redraw -j 4
```
Expected: all PASS. If rustc crashes on a large `mkrew-ui` test build (STATUS_STACK_BUFFER_OVERRUN), run the same filters with `--lib`.
The keymap snapshot tests have no `insta` file naming `show_editor` (grep found none), so no `.snap` update is needed.

Manual smoke (run the app from the worktree: `cargo run -p mkrew-ui -j 4`, a workspace with two `.rs` files and one `.md`):
1. Single click a file: a tab opens after the active one, title in italics; single click another file: the same tab shows it (replaced). Edit the text: the title gets a dot and stops being italic; single click a third file: a new tab.
2. Double click a file: a kept tab (no italics). Click it in the tree again: its tab is focused, no second tab.
3. Ctrl+S saves, Ctrl+F opens the find bar in the focused file pane, Ctrl+W closes it; with unsaved text Ctrl+W asks Save / Don't save / Cancel; closing the tab with its ✕ asks too; "Close others" asks about every other tab with unsaved text.
4. Delete the open file on disk: the pane shows "This file was removed." and Close.
5. Quit with an unsaved file: the question shows that file's pane first.
6. Restart: file tabs come back and load when drawn.

- [ ] **Step 25: Commit C**

```
git add -A crates/mkrew-ui/src
git -c user.email=tungnv975@gmail.com commit -m "feat(ui): file panes in the layout; remove the Editor view, the file tab strip and their commands"
```
(`git add -A` is limited to `crates/mkrew-ui/src`; the deletions of the two shell files are included.)

---

#### Notes for the executor

- Task 1 supplies `Tab::keys`, `focused`, `shown`, `layout`, `Tabs::tab_with`, `focus`, `focus_pane`, `insert_leaf`, `update_leaf`, `close_leaf`, `preview_tab`, `position_after_active` and `hosts::focused_key`; the code above uses exactly those signatures. If a name differs after Task 1, fix the call, not the plan's structure.
- Pane drag strip, edge handle and drop zones for file panes are Task 6 (this task draws none for them).
- The diff arm of `pane_frame` and `focus_pane` is Task 5's.
- Known limitation kept from today: a leaf stores no worktree target, so changing the Files target of a workspace closes its file leaves (after the save question) and the files start over, as the strip did.
- Spec gap: the daemon answers a missing file with an error whose text is `no file <path> in project <id>`; the UI has no typed not-found, so `documents::is_missing` reads that text. A typed code on the wire would be sturdier; not done here.

---

### Task 5: Diff leaf (`open_diff(place)`, the diff pane, per-workspace diff state; no Diff view)

Line numbers are those of the worktree before Tasks 1-4; Tasks 1-4 shifted them, so find every place by the quoted text, not by number.
Spec: section 4.4 (Diff leaf), 5 (panes), 6.1 item 11 (`Command::ShowDiff` and every "open diff" call `open_diff(NewTab)`), 8 (removed).
Tasks 1-4 are done.
This task relies on these facts from them (any difference is a stop-and-ask, not a guess):

- Task 1: `PaneKey::Diff`, `Leaf::diff()`, `Tab::{focused() -> Option<&PaneKey>, shown() -> Vec<PaneKey>}`, `Tabs::{tab_with(&PaneKey) -> Option<&Tab>, focus(&PaneKey) -> bool, close_leaf(&PaneKey) -> Option<TabSetLayoutParams>}`, `Shell::focused_key(&self, window: &Window) -> Option<PaneKey>` (hosts.rs).
- Task 4: `Place` (`crate::documents::Place`), `Shell::place_leaf(&mut self, project_id: Ulid, leaf: Leaf, place: Place, window: &mut Window, cx: &mut Context<'_, Self>)` (new `shell/places.rs`; a key that is open already: its tab comes forward and the pane gets the focus; always saves the layout and calls `sync_panes`; main window only).
- Task 4: `render_tab` and `pane_frame` take `project_id: Ulid` as their first parameter after `&self`, and `pane_frame` takes `key: PaneKey` and `window: &Window`; its content is one `match key` whose `PaneKey::Diff` arm is a placeholder muted message `Diff` (this task replaces it).
- Task 4: `focus_pane` (focus.rs) is `match self.focused_key(window)` with a `Some(PaneKey::Diff)` fallback arm (`window.focus(&self.focus_handle, cx)`) that this task replaces.
- Task 4: `tabs.rs` has `PaneIcon::File(String)` and `file_label(tab, docs)`; `tab_icons` keeps its signature; `Shell::panel_file(&self, project_id) -> Option<String>`, `Shell::picked_file_lines(&self, path: &str, project_id: Ulid)`, `Shell::focused_file(&self, window) -> Option<String>`; `palette::Context` has `file_pane: bool` (no `file_tabs`); `View::Editor`, `Command::ShowEditor`, the `FileTab` strip and `has_file_tabs` are gone.

**Design decisions (the spec leaves them open):**

- The diff pane needs no focus handle of its own.
  Today the Git keyboard is `Shell.git_focus`: the panel's Git list tracks it, and the diff card tracks it only while the panel's Git tab is not drawn (one element per frame).
  The pane keeps exactly that rule, so keys, clicks in the toolbar and "the keyboard stays in the list" behave as before.
  `focus_pane` on a focused Diff leaf focuses `git_focus`.
- "The diff pane has the focus" for commands means "the focused pane of the window's tab is the Diff leaf" (`focused_key(window)`), not gpui focus.
  Commands take a `window` for it (`comment_context(window)`).
  What a typed comment draft edits needs no window: the draft field only exists where it is drawn (`drawn_comment_context()`).
- `open_git_diff` is deleted; its four callers call `open_diff(project_id, Place::NewTab, ..)` directly.
- Esc closes the leaf when the focused pane is the Diff leaf, from the Git list as well as from the pane (as the Diff view closed from both); the pick is cleared first, as today.
- "Git is on screen" (`git_shown`) now means: the panel's Git tab, or a Diff leaf drawn in the main window's tab in front or in a visible detached window's tab (`diff_on_screen`).
  A Diff leaf that comes on screen (restart, tab switch, opened) refreshes Git through one hook in `sync_panes`, as `show_view` did for the Diff view.
- The empty state is a pure decision on `GitView` (`NoFile`), unit tested.

**Files:**

- Create: `crates/mkrew-ui/src/shell/diff_pane.rs`.
- Modify: `crates/mkrew-ui/src/git_view.rs` (remove `open`, `open_diff`, `close_diff`, `diff_open`; `reselect`; add `NoFile`, `GitView::no_file`).
- Modify: `crates/mkrew-ui/src/git_view/tests.rs` (replace `an_open_diff_follows_its_selection_and_closes_when_nothing_is_left`, add the `no_file` test).
- Modify: `crates/mkrew-ui/src/tabs.rs` and `tabs/tests.rs` (remove `AreaItem::View`, the chip logic of `area_items`, `scroll_index`'s view argument; add `diff_label`, `PaneIcon::Diff`).
- Modify: `crates/mkrew-ui/src/board.rs` and `board/tests.rs` (remove `View::Diff`).
- Modify: `crates/mkrew-ui/src/palette.rs` and `palette/tests.rs` (`Context.diff` -> `Context.diff_pane`).
- Modify: `crates/mkrew-ui/src/redraw.rs` (`Shown.diff`, tests), `crates/mkrew-ui/src/menu.rs` (one test), `crates/mkrew-ui/src/keymap.rs` (doc of `ShowDiff`).
- Modify: `crates/mkrew-ui/src/main.rs`, `shell.rs`, `shell/connection.rs`, `shell/detached_render.rs` (the `git_diff_*` fields move; `diff_views`, `diff_drawn`; `ensure_diff_views` calls).
- Modify: `shell/git.rs`, `shell/git_diff.rs`, `shell/git_render.rs`, `shell/git_viewed.rs`, `shell/picking.rs`, `shell/workspace_actions.rs` (the `scroll_diff_to` callers).
- Modify: `shell/workspace.rs`, `shell/focus.rs`, `shell/chrome.rs`, `shell/inspector.rs`, `shell/pane_render.rs`, `shell/hosts.rs`, `shell/comments.rs`, `shell/composer.rs`, `shell/send_dialog.rs`, `shell/sessions.rs`, `shell/navigation.rs`.

**Interfaces:**

Consumes: the Task 1 and Task 4 names listed above.

Produces:

```rust
// git_view.rs
pub enum NoFile { Loading, Failed, Clean, Filtered, Pick }
impl NoFile { pub fn message(self, section: Section) -> (&'static str, Option<&'static str>) }
impl GitView { pub fn no_file(&self) -> NoFile }
// removed: GitView::open_diff, close_diff, diff_open (and the private field `open`)

// tabs.rs
pub enum AreaItem { Rule, Tab(usize) }
pub fn area_items(pinned: &[bool]) -> Vec<AreaItem>
pub fn scroll_index(items: &[AreaItem], active: Option<usize>) -> Option<usize>
pub fn diff_label(tab: &Tab) -> Option<&'static str>   // Some("Diff") while the focused pane is the diff and the tab has its default name
PaneIcon::Diff

// shell/diff_pane.rs
pub(crate) struct DiffViewState { list: ListState, layout, looks, scroll_to, reveal } // the git_diff_* fields of Shell
Shell.diff_views: HashMap<Ulid, DiffViewState>
Shell.diff_drawn: bool
Shell::open_diff(&mut self, project_id: Ulid, place: Place, window: &mut Window, cx: &mut Context<'_, Self>)
Shell::render_diff_pane(&self, project_id: Ulid, theme: &Theme, window: &Window, cx: &mut Context<'_, Self>) -> gpui::AnyElement
Shell::diff_on_screen(&self) -> bool
Shell::ensure_diff_views(&mut self)
// shell/focus.rs
Shell::panel_diff_focused(&self, project_id: Ulid) -> bool
// shell/comments.rs
Shell::comment_context(&self, window: &Window) -> Option<CommentContext>   // commands: Doc while Docs shows, Diff while the focused pane is the Diff leaf
Shell::drawn_comment_context(&self) -> Option<CommentContext>              // typing: Doc while Docs shows, Diff while a Diff leaf is drawn
// shell/git.rs
Shell::scroll_diff_to(&self, project_id: Ulid, row: usize)
// shell/composer.rs
Shell::picked_diff_lines(&self, project_id: Ulid) -> Option<(String, u32, u32, String)>   // replaces picked_lines(project_id, View)
// palette.rs
Context.diff_pane: bool   // replaces Context.diff
// redraw.rs
Shown.diff: bool          // a Diff pane is drawn
```

- [ ] **Step 1: The tests for the empty-state decision and the new open/close rule (git_view)**

Replace the whole test `an_open_diff_follows_its_selection_and_closes_when_nothing_is_left` in `crates/mkrew-ui/src/git_view/tests.rs` (it starts with `#[test]` just above `fn an_open_diff_follows_its_selection_and_closes_when_nothing_is_left()`, ends before the `#[test]` of `fn a_failed_status_says_so_but_a_failed_refresh_keeps_the_list`) with these two tests.
The first keeps the selection-follows part (spec 4.4) and replaces "the diff closes" with "nothing closes, the selection and the diff reset, and the pane's empty state is Clean".

```rust
#[test]
fn the_selection_follows_the_status_and_an_empty_status_closes_nothing() {
    let mut view = loaded();
    view.key(Key::Down);
    // `new.txt` got staged: the selection moves with it.
    view.select(target(Group::Untracked, "new.txt"));
    let mut status = working();
    status.untracked.clear();
    status.staged.push(file("new.txt", ChangeKind::Added));
    view.begin_status();
    view.status_loaded(Ok(status));
    assert_eq!(view.selected, Some(target(Group::Staged, "new.txt")));
    view.key(Key::Up);
    assert!(view.selected.is_some(), "moving the selection keeps one picked");
    // Everything committed: no selection and no diff; a Diff pane that shows this view stays
    // and says so (there is nothing in the view to close any more).
    view.begin_status();
    let requests = view.status_loaded(Ok(GitStatus::default()));
    assert_eq!(requests, Requests::default());
    assert_eq!(view.selected, None);
    assert_eq!(view.diff, Diff::None);
    assert_eq!(view.no_file(), NoFile::Clean);
    // Changes come back: the first file is selected again and its diff is read.
    view.begin_status();
    let requests = view.status_loaded(Ok(working()));
    assert_eq!(requests.diff, Some(target(Group::Staged, "a.rs")));
}

#[test]
fn the_diff_pane_knows_why_it_has_no_file() {
    let mut view = GitView::new(Ulid::new(), None);
    assert_eq!(view.no_file(), NoFile::Loading);
    view.begin_status();
    view.status_loaded(Err("index.lock".into()));
    assert_eq!(view.no_file(), NoFile::Failed);
    // Files are listed, the filter hides every one of them.
    let mut view = loaded();
    view.filter.insert("zzz");
    view.begin_status();
    view.status_loaded(Ok(working()));
    assert_eq!(view.selected, None);
    assert_eq!(view.no_file(), NoFile::Filtered);
    // The filter is cleared: a refresh selects the first file again.
    view.filter.clear();
    view.begin_status();
    let requests = view.status_loaded(Ok(working()));
    assert_eq!(requests.diff, Some(target(Group::Staged, "a.rs")));
    // Nothing changed at all.
    view.begin_status();
    view.status_loaded(Ok(GitStatus::default()));
    assert_eq!(view.no_file(), NoFile::Clean);
}
```

- [ ] **Step 2: Run, expect FAIL**

Run: `cargo test -p mkrew-ui git_view -j 4`
Expected: FAIL to compile with `cannot find type 'NoFile' in this scope` and `no method named 'no_file' found for struct 'GitView'`.

- [ ] **Step 3: Implement `NoFile` and `GitView::no_file` (additive)**

In `crates/mkrew-ui/src/git_view.rs`, directly above the line `/// One workspace's Git view.` (above `pub struct GitView`), add:

```rust
/// Why the diff pane has no file to show. The pane stays open and says so (spec 4.4): a leaf
/// the user placed does not vanish when everything is committed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoFile {
    /// The changes are being read.
    Loading,
    /// The changes could not be read.
    Failed,
    /// Nothing changed: everything is committed, or nothing differs from the base.
    Clean,
    /// Files are listed, but "Filter files" or "Changed by" hides every one of them.
    Filtered,
    /// Files are listed and none is picked.
    Pick,
}

impl NoFile {
    /// The pane's sentence and the line under it (`Failed` has none: the shell shows the error).
    pub fn message(self, section: Section) -> (&'static str, Option<&'static str>) {
        match (self, section) {
            (NoFile::Loading, _) => ("Loading changes…", None),
            (NoFile::Failed, _) => ("Cannot read the changes.", None),
            (NoFile::Clean, Section::Base) => ("No changes against the base.", None),
            (NoFile::Clean, Section::Since) => ("No changes since then.", None),
            (NoFile::Clean, _) => (
                "Nothing to commit.",
                Some("The diff shows the next change you make."),
            ),
            (NoFile::Filtered, _) => (
                "No file matches the filter.",
                Some("Clear the filter or \"Changed by\" in the Git panel."),
            ),
            (NoFile::Pick, _) => (
                "Select a file to see its changes.",
                Some("Use ↑ ↓ (or j k) to move between files."),
            ),
        }
    }
}
```

In `impl GitView`, directly after `fn select_none(&mut self) { .. }`, add:

```rust
    /// Why no file is selected while the changes (not the History) are listed.
    pub fn no_file(&self) -> NoFile {
        match &self.status {
            Status::Loading => NoFile::Loading,
            Status::Failed(_) => NoFile::Failed,
            Status::Loaded(_) if self.status_files(false).is_empty() => NoFile::Clean,
            Status::Loaded(_) if self.files().is_empty() => NoFile::Filtered,
            Status::Loaded(_) => NoFile::Pick,
        }
    }
```

- [ ] **Step 4: Run, expect PASS**

Run: `cargo test -p mkrew-ui git_view -j 4`
Expected: PASS (the two new tests and every existing `git_view` test).

- [ ] **Step 5: Commit the pure part**

```
cargo fmt --all
git add crates/mkrew-ui/src/git_view.rs crates/mkrew-ui/src/git_view/tests.rs
git -c user.email=tungnv975@gmail.com commit -m "feat(ui): GitView::no_file decides the diff pane's empty state"
```

- [ ] **Step 6: Update the other pure tests (they fail to compile until Step 8)**

These edits are the contract of the cutover.

**6a. `crates/mkrew-ui/src/tabs/tests.rs`.**
Replace both tests `the_tab_area_orders_view_chips_pinned_tabs_and_the_rest` and `the_scroll_target_is_the_shown_chip_or_the_active_tab` (with their doc comments; Task 4 already edited their Editor parts) by:

```rust
/// The tab area's children, in paint order: the tabs, a rule between the pinned tabs and the
/// rest; never a rule at an end.
#[test]
fn the_tab_area_orders_pinned_tabs_and_the_rest() {
    use AreaItem::{Rule, Tab};
    assert_eq!(area_items(&[]), []);
    assert_eq!(
        area_items(&[true, true, false]),
        [Tab(0), Tab(1), Rule, Tab(2)]
    );
    assert_eq!(
        area_items(&[true, true]),
        [Tab(0), Tab(1)],
        "no rule after the last group"
    );
    assert_eq!(area_items(&[false, false]), [Tab(0), Tab(1)]);
    assert_eq!(area_items(&[true, false]), [Tab(0), Rule, Tab(1)]);
}

/// The child to scroll into view is the active tab (`active` is its drawn slot).
#[test]
fn the_scroll_target_is_the_active_tab() {
    let items = area_items(&[true, false, false]);
    // [Tab(0), Rule, Tab(1), Tab(2)]
    assert_eq!(scroll_index(&items, Some(2)), Some(3));
    assert_eq!(scroll_index(&items, Some(0)), Some(0));
    assert_eq!(scroll_index(&items, None), None);
}
```

Add this test as the last item of the same file:

```rust
/// A tab whose focused pane is the diff is titled "Diff" while it keeps its default name, and
/// its chip shows the diff icon (an inactive chip shows its focused pane only).
#[test]
fn a_tab_whose_focused_pane_is_the_diff_is_titled_diff() {
    let project = Ulid::new();
    let a = Ulid::new();
    let tree = json!({"split": "vertical", "ratio": 0.5, "children": [
        {"session": a.to_string()}, {"diff": true}
    ]});
    let mut tabs = Tabs::new(project);
    tabs.load(vec![wire(project, "Terminal", 1.0, tree)]);
    assert_eq!(diff_label(tabs.active().unwrap()), None, "a session has the focus");
    assert!(tabs.focus(&PaneKey::Diff));
    let tab = tabs.active().unwrap();
    assert_eq!(diff_label(tab), Some("Diff"));
    assert_eq!(tab_icons(tab, &[], false).panes, [PaneIcon::Diff]);
    // A name the user gave the tab wins.
    let mut named = Tabs::new(project);
    named.load(vec![wire(project, "Review", 1.0, json!({"diff": true}))]);
    assert_eq!(diff_label(named.active().unwrap()), None);
}
```

**6b. `crates/mkrew-ui/src/board/tests.rs`.**
In `each_view_maps_to_its_own_show_command`, the expected list becomes:

```rust
        [
            Command::ShowTerminals,
            Command::ShowBoard,
            Command::ShowDocs
        ]
```

**6c. `crates/mkrew-ui/src/menu.rs`** test: the line `for view in [View::Terminals, View::Docs, View::Diff] {` becomes `for view in [View::Terminals, View::Docs] {`.

**6d. `crates/mkrew-ui/src/palette/tests.rs`.**
In `fn context(view: View) -> Context`, replace the two lines

```rust
        git: view == View::Diff,
        diff: true,
```

with

```rust
        git: false,
        diff_pane: false,
```

Right after `fn context`, add:

```rust
/// The Terminals view with the Diff pane focused and Git on screen: the Git keys and the diff
/// comments act.
fn diff_context() -> Context {
    Context {
        git: true,
        diff_pane: true,
        ..context(View::Terminals)
    }
}
```

Then every `context(View::Diff)` in the file becomes `diff_context()` (sites by text: `let git = commands(&context(View::Diff));` twice, `extras(&context(View::Diff))`, `..context(View::Diff)` four times, `&context(View::Diff),` in `typing_filters_ranks_and_resets_the_pick`, `commands(&context(View::Diff)).contains(&Command::AddComment)`).
In the list of views of the Phone access test (`View::Editor, View::Diff,` inside `for view in [..]`), delete the `View::Diff,` entry.
Replace `diff_and_git_commands_follow_what_is_on_screen` with:

```rust
#[test]
fn diff_and_git_commands_follow_what_is_on_screen() {
    // Show diff opens (or focuses) the diff leaf: it is there whenever a workspace is.
    assert!(commands(&context(View::Terminals)).contains(&Command::ShowDiff));
    let none = Context {
        workspace: false,
        ..context(View::Terminals)
    };
    assert!(!commands(&none).contains(&Command::ShowDiff));
    // The Git tab in the panel next to the terminals: the Git keys act on its list.
    let git_tab = Context {
        git: true,
        ..context(View::Terminals)
    };
    assert!(commands(&git_tab).contains(&Command::GitStage));
    assert!(!commands(&context(View::Terminals)).contains(&Command::GitStage));
    // Diff comments only while the focused pane is the diff.
    assert!(!commands(&git_tab).contains(&Command::AddComment));
    assert!(commands(&diff_context()).contains(&Command::AddComment));
    assert!(commands(&diff_context()).contains(&Command::SendSelection));
}
```

(`commands` is the existing helper of that file that lists the available commands.)

**6e. `crates/mkrew-ui/src/redraw.rs` tests.**
Both `Shown { .. }` literals in the tests (`terminals`, `nothing_shown`) get `diff: false,` after `all_board: false,`.
In `conflicts_show_on_the_board_and_in_pane_badges_only`, the loop `for view in [View::Docs, View::Editor, View::Diff] {` becomes `for view in [View::Docs] {` (Task 4 may already have removed `View::Editor`; the end state is `[View::Docs]`).
Replace `comments_and_ai_answers_follow_the_view_that_draws_them` (and its doc comment) with:

```rust
    /// Diff comments are drawn by a Diff pane, doc comments in Docs, both of the shown
    /// workspace; an AI answer goes to the Git tab's commit box or a pane composer.
    #[test]
    fn comments_and_ai_answers_follow_the_view_that_draws_them() {
        let w = world();
        let mut s = terminals(&w);
        s.diff = true;
        assert!(redraws(&w, &s, &comment(w.ws, CommentContext::Diff)));
        assert!(
            !redraws(&w, &s, &comment(w.other, CommentContext::Diff)),
            "other workspace"
        );
        assert!(
            !redraws(&w, &s, &comment(w.ws, CommentContext::Doc)),
            "Doc comment, no Docs view"
        );
        s.diff = false;
        assert!(
            !redraws(&w, &s, &comment(w.ws, CommentContext::Diff)),
            "no Diff pane drawn"
        );
        s.view = View::Docs;
        assert!(redraws(&w, &s, &comment(w.ws, CommentContext::Doc)));
        assert!(!redraws(&w, &s, &comment(w.ws, CommentContext::Diff)));

        let ai = Event::AiGenerated(AiGeneratedEvent {
            generation_id: Ulid::new(),
            project_id: Some(w.ws),
            text: Some("x".into()),
            error: None,
        });
        s.view = View::Terminals;
        assert!(redraws(&w, &s, &ai), "pane composer");
        s.view = View::Docs;
        assert!(!redraws(&w, &s, &ai));
        s.panel = Some(PanelTab::Git);
        assert!(redraws(&w, &s, &ai), "commit box in the Git tab");
    }
```

- [ ] **Step 7: Run, expect FAIL**

Run: `cargo test -p mkrew-ui -j 4`
Expected: FAIL to compile: `struct 'Context' has no field named 'diff_pane'`, `struct 'Shown' has no field named 'diff'`, `this function takes 3 arguments but 1 argument was supplied` (`area_items`), `cannot find function 'diff_label'`, `no variant or associated item named 'Diff' found for enum 'PaneIcon'`.

- [ ] **Step 8: The cutover (one compile unit)**

Do A to P in order; the crate compiles again at the end of P.

**A. `git_view.rs`: the open flag goes.**

1. In `struct GitView`, delete the field and its doc comment:

```rust
    /// The Diff view is open (SPEC §12B, ADR 0084): a click / Enter on a file or an entry
    /// point opens it; `×`, `Esc` and an empty list close it. A selection moving by itself
    /// (staging, ↑ ↓, a refresh) keeps it.
    open: bool,
```

2. In `GitView::new`, delete the line `open: false,`.
3. In `reselect`, replace the `None` arm:

```rust
            None => {
                // No file left (committed, discarded, clean): the open diff closes with it.
                self.open = false;
                self.select_none();
                None
            }
```

with

```rust
            None => {
                // No file left (committed, discarded, clean): nothing is selected; a Diff pane
                // shows the empty state (`no_file`) instead of closing (spec 4.4).
                self.select_none();
                None
            }
```

4. Delete the three methods `open_diff`, `close_diff`, `diff_open` with their doc comments (between `select_none` and `section`).

**B. New file `crates/mkrew-ui/src/shell/diff_pane.rs`.**
Register it in `crates/mkrew-ui/src/shell.rs` next to `pub(crate) mod git_diff;`: add `pub(crate) mod diff_pane;`.
In `crates/mkrew-ui/src/main.rs`, next to `use shell::git_diff::*;` add `use shell::diff_pane::DiffViewState;`.
If Tasks 1 and 4 did not already import `Leaf`, `PaneKey` and `Place` in `main.rs`, add them to the `use mkrew_ui::split::{..}` and `use mkrew_ui::documents::{..}` lines.

```rust
//! The Diff leaf (spec 2026-11-23 §4.4, §5): opening it, the list state of its pane, the pane.

use crate::*;

/// The diff list of one workspace's Diff leaf: the `git_diff_*` fields `Shell` held while the
/// Diff view was a view of its own. One per workspace, so a diff pane in a detached window
/// and one in the main window (two workspaces) never share it.
pub(crate) struct DiffViewState {
    /// The diff rows (a variable-height list: wrapped lines are taller).
    pub(crate) list: ListState,
    /// What `list` was last laid out for (the diff, mode, row count; another resets it).
    pub(crate) layout: Cell<Option<(usize, DiffMode, usize)>>,
    /// What changes row heights in place (wrap, picked lines: remeasured).
    pub(crate) looks: Cell<Option<(bool, Option<Selection>)>>,
    /// A row to scroll to once the list is laid out for the shown diff.
    pub(crate) scroll_to: Cell<Option<usize>>,
    /// A diff row to scroll into view once the list is laid out (a keyboard pick moved).
    pub(crate) reveal: Cell<Option<usize>>,
}

impl DiffViewState {
    pub(crate) fn new() -> DiffViewState {
        DiffViewState {
            list: ListState::new(0, ListAlignment::Top, px(theme::GIT_DIFF_OVERDRAW)),
            layout: Cell::new(None),
            looks: Cell::new(None),
            scroll_to: Cell::new(None),
            reveal: Cell::new(None),
        }
    }
}

impl Shell {
    /// A Diff leaf of the active workspace is drawn: in the main window's tab in front, or in
    /// the tab a visible detached window shows.
    pub(crate) fn diff_on_screen(&self) -> bool {
        let Some(project_id) = self.workspace else {
            return false;
        };
        let Some(tabs) = self.tabs.get(&project_id) else {
            return false;
        };
        let holds = |tab: &Tab| tab.shown().contains(&PaneKey::Diff);
        let in_front = self.view() == View::Terminals && tabs.active().is_some_and(holds);
        in_front
            || (self.detached.visible_tabs().into_iter()).any(|DockTab::Terminal { project, tab_id }| {
                project == project_id && tabs.tab(tab_id).is_some_and(holds)
            })
    }

    /// Gives every workspace that holds a Diff leaf its list state (a leaf restored from the
    /// saved layout never went through `open_diff`) and drops the state of one that no longer
    /// does. Called at the start of every window's render, which has `&mut self`.
    pub(crate) fn ensure_diff_views(&mut self) {
        self.diff_views.retain(|id, _| {
            self.tabs
                .get(id)
                .is_some_and(|tabs| tabs.tab_with(&PaneKey::Diff).is_some())
        });
        for (id, tabs) in &self.tabs {
            if tabs.tab_with(&PaneKey::Diff).is_some() {
                self.diff_views
                    .entry(*id)
                    .or_insert_with(DiffViewState::new);
            }
        }
    }

    /// Opens the diff of the Git panel's selection as a leaf (`place`: a new tab after the
    /// active one, or a split of a pane). One Diff leaf per workspace: when it is open
    /// already, its tab comes forward and its pane gets the focus (`place_leaf`). `project_id`
    /// is the active workspace (every opener activates it first). Git loads when the leaf
    /// comes on screen (`sync_panes`).
    pub(crate) fn open_diff(
        &mut self,
        project_id: Ulid,
        place: Place,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        // A detached window has no Git panel: the main window opens it.
        if self.host_of(window) != HostId::Main {
            self.in_window(HostId::Main, cx, move |shell, window, cx| {
                window.activate_window();
                shell.open_diff(project_id, place, window, cx);
            });
            return;
        }
        if self.view() != View::Terminals {
            self.show_view(View::Terminals, window, cx);
        }
        self.place_leaf(project_id, Leaf::diff(), place, window, cx);
    }

    /// The Diff leaf's content: the selected file's diff card (toolbar, diff, comment margin).
    /// The pane carries the `"Git"` key context. While the panel's Git tab is not drawn it
    /// also holds the Git keyboard (`git_focus`); one element tracks `git_focus` per frame.
    pub(crate) fn render_diff_pane(
        &self,
        project_id: Ulid,
        theme: &Theme,
        window: &Window,
        cx: &mut Context<'_, Self>,
    ) -> gpui::AnyElement {
        let Some(view) = self.git.get(&project_id) else {
            return message(theme, None, "Loading changes…".into(), None)
                .size_full()
                .into_any_element();
        };
        let card = self
            .render_git_diff(project_id, view, theme, window, cx)
            .key_context(GIT_CONTEXT);
        let card = if self.git_panel_shown() {
            card
        } else {
            card.track_focus(&self.git_focus)
                .on_key_down(cx.listener(Self::git_key_down))
        };
        div()
            .size_full()
            .min_w_0()
            .flex()
            .font_family(theme.ui_font_family.clone())
            .child(card)
            .into_any_element()
    }
}
```

**C. `main.rs` and `shell/connection.rs`: the fields move.**

In the `Shell` struct in `main.rs`, delete the five fields `git_diff_list`, `git_diff_layout`, `git_diff_looks`, `git_diff_scroll_to`, `git_diff_reveal` with their doc comments (the block that starts `/// The diff rows (a variable-height list` and ends with `git_diff_reveal: Cell<Option<usize>>,`) and put in their place:

```rust
    /// The diff list of each workspace's Diff leaf (`ensure_diff_views` creates it).
    diff_views: HashMap<Ulid, DiffViewState>,
    /// A Diff leaf was drawn at the last `sync_panes`: one that comes on screen refreshes Git.
    diff_drawn: bool,
```

In `Shell` construction in `shell/connection.rs`, replace the five lines `git_diff_list: ..`, `git_diff_layout: Cell::new(None),`, `git_diff_looks: Cell::new(None),`, `git_diff_scroll_to: Cell::new(None),`, `git_diff_reveal: Cell::new(None),` with:

```rust
            diff_views: HashMap::new(),
            diff_drawn: false,
```

In `main.rs`'s `impl Render for Shell`, `fn render(&mut self, window, cx)`: make `self.ensure_diff_views();` its first statement (before `let theme = cx.theme().clone();`).
In `shell/detached_render.rs`, `render_host`: make `self.ensure_diff_views();` its first statement (before `let theme = cx.theme().clone();`).

In `main.rs`'s `use mkrew_ui::tabs::{ .. }` list add `diff_label` (and keep `AreaItem`, `area_items`, `scroll_index`).

**D. `shell/git_diff.rs`: the card fits a pane; empty state; list state.**

1. The card `panel` (top of `render_git_diff`): the pane's frame gives margin and rounding, so the card is a plain surface.
Replace

```rust
        // A framed card (`surface`, 1 px border) inset `space::M` from the view's edges; its
        // height is what the row gives it minus the margins.
        let panel = div()
            .flex_1()
            .min_w_0()
            .flex()
            .flex_col()
            .m(px(space::M))
            .rounded(px(radius::CARD))
            .border(px(theme::BORDER_WIDTH))
            .border_color(rgb(theme.border))
            .overflow_hidden()
            .bg(rgb(theme.surface));
```

with

```rust
        // The pane's frame rounds and spaces it: the card is the pane's whole content.
        let panel = div()
            .flex_1()
            .min_w_0()
            .flex()
            .flex_col()
            .overflow_hidden()
            .bg(rgb(theme.surface));
```

2. The empty message: in the `let (sentence, detail) = match history.map(..)` block, replace the last arm

```rust
                None => (
                    "Select a file to see its changes.",
                    Some("Use ↑ ↓ (or j k) to move between files.".to_owned()),
                ),
```

with

```rust
                None => {
                    // Nothing changed shows "Nothing to commit.", not a closed pane.
                    let (sentence, detail) = view.no_file().message(view.section());
                    let failed = match &view.status {
                        git_view::Status::Failed(error) => Some(error.clone()),
                        _ => None,
                    };
                    (sentence, failed.or_else(|| detail.map(str::to_owned)))
                }
```

3. `git_diff_lines`: use the workspace's list state.
At the start of the function body (before `let mode = self.diff_mode;`) add:

```rust
        let Some(state) = self.diff_views.get(&project_id) else {
            return message(theme, None, "Loading…".into(), None)
                .flex_1()
                .into_any_element();
        };
```

Replace the block from `if self.git_diff_layout.get() != Some(layout) {` to the end of `if let Some(row) = self.git_diff_reveal.take() { .. }` with:

```rust
        if state.layout.get() != Some(layout) {
            state.layout.set(Some(layout));
            state.looks.set(Some((wrap, picked)));
            state.list.reset(rows);
        } else if state.looks.get() != Some((wrap, picked)) {
            state.looks.set(Some((wrap, picked)));
            state.list.remeasure();
        }
        if let Some(row) = state.scroll_to.take() {
            state.list.scroll_to(ListOffset {
                item_ix: row,
                offset_in_item: px(0.),
            });
        }
        if let Some(row) = state.reveal.take() {
            state.list.scroll_to_reveal_item(row);
        }
```

and `self.git_diff_list.clone(),` (first argument of `list(..)`) becomes `state.list.clone(),`.
In the doc comment of `git_diff_close`, `"×": the diff closes (also `Esc`).` stays; its body still calls `shell.close_git_diff(project_id, window, cx)` (now removes the leaf, step E).

**E. `shell/git.rs`: open, close, follow, retarget, scroll.**

1. Delete `open_git_diff` and `follow_git_diff` with their doc comments.
2. Replace `close_git_diff` with:

```rust
    /// `×` on the diff, or `Esc` while the Diff leaf is the focused pane: the leaf is removed
    /// from its tab (its Git selection stays for the next open).
    pub(crate) fn close_git_diff(
        &mut self,
        project_id: Ulid,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let closed = (self.tabs.get_mut(&project_id)).and_then(|tabs| tabs.close_leaf(&PaneKey::Diff));
        if let Some(write) = closed {
            self.save_layout(project_id, write, window, cx);
        }
        self.sync_panes(window, cx);
        // The Git tab is still on screen: the keyboard stays in its list.
        if self.workspace == Some(project_id) && self.git_panel_shown() {
            window.focus(&self.git_focus, cx);
        }
        cx.notify();
    }
```

3. `open_git`: change the doc ("the right panel on its Git tab and the Diff view" -> "and the diff leaf") and its body line `self.open_git_diff(project_id, window, cx);` -> `self.open_diff(project_id, Place::NewTab, window, cx);`.
4. `retarget_git`: replace the body after the `if old.worktree_task_id == target && old.repo == repo { return false; }` guard with:

```rust
        let draft = std::mem::take(&mut old.commit.text);
        let mut view = GitView::new(project_id, target);
        view.repo = repo;
        view.commit.text = draft;
        self.git.insert(project_id, view);
        self.git_files_scroll.scroll_to_item(0, ScrollStrategy::Top);
        self.scroll_diff_to(project_id, 0);
        true
```

(an open diff leaf shows the new view's first file once its status loads; it no longer needs a carried flag).
5. `git_requests`: delete the last two lines before `cx.notify();`:

```rust
        // The list emptied (commit, discard, clean worktree): the open diff closed with it.
        self.follow_git_diff(project_id, window, cx);
```

6. `select_git_commit` and `select_git_file`: the last statement `self.open_git_diff(project_id, window, cx);` -> `self.open_diff(project_id, Place::NewTab, window, cx);`; their doc comments say "the Diff view shows it" -> "the diff leaf shows it".
7. `git_key_down`: replace the Esc block that closes the diff

```rust
            if let Some(project_id) = self.workspace
                && self.view() == View::Diff
            {
```

with

```rust
            if let Some(project_id) = self.workspace
                && matches!(self.focused_key(window), Some(PaneKey::Diff))
            {
```

and the Enter block's `return self.open_git_diff(project_id, window, cx);` -> `return self.open_diff(project_id, Place::NewTab, window, cx);`.
8. `scroll_diff_to` takes the workspace (the state is per workspace; a workspace without a leaf has no list to scroll, and a leaf created later starts at the top):

```rust
    /// Scrolls `project_id`'s diff to `row` once its list is laid out for the shown diff (the
    /// render resets the list for another diff or mode, which would drop a scroll done now).
    pub(crate) fn scroll_diff_to(&self, project_id: Ulid, row: usize) {
        if let Some(state) = self.diff_views.get(&project_id) {
            state.scroll_to.set(Some(row));
        }
    }
```

Callers (old -> new):

| File | Old | New |
|---|---|---|
| `git.rs` `retarget_git` | `self.scroll_diff_to(0);` | `self.scroll_diff_to(project_id, 0);` (step 4) |
| `git.rs` `select_git_commit` | `self.scroll_diff_to(0);` | `self.scroll_diff_to(project_id, 0);` |
| `git.rs` `load_git_diff` | `shell.scroll_diff_to(row);` | `shell.scroll_diff_to(project_id, row);` |
| `git.rs` `select_git_file` | `self.scroll_diff_to(0);` | `self.scroll_diff_to(project_id, 0);` |
| `git.rs` `git_key` (3 sites: `self.scroll_diff_to(0);` twice, `self.scroll_diff_to(at);` once) | `self.scroll_diff_to(0);` / `(at)` | `self.scroll_diff_to(project_id, 0);` / `(project_id, at)` |
| `git_viewed.rs` `toggle_git_viewed` | `self.scroll_diff_to(0);` | `self.scroll_diff_to(project_id, 0);` |
| `workspace_actions.rs` `apply_diff_mode` | `self.scroll_diff_to(hunk_row.unwrap_or(0));` | `if let Some(project_id) = self.workspace { self.scroll_diff_to(project_id, hunk_row.unwrap_or(0)); }` |
| `picking.rs` `reveal_pick` | `self.git_diff_reveal.set(row);` | `if let Some(state) = self.diff_views.get(&project_id) { state.reveal.set(row); }` (keep the surrounding `if row.is_some()`) |

`git_key` already has `project_id` bound at its top; `select_git_commit`, `select_git_file`, `toggle_git_viewed`, `load_git_diff` take it as a parameter.

**F. `shell/focus.rs`.**

1. `panel_view`: `matches!(self.view(), View::Terminals | View::Editor | View::Diff)` (after Task 4: `View::Terminals | View::Diff`) -> `self.view() == View::Terminals`.
2. `git_shown`:

```rust
    /// Some of the active workspace's Git is drawn (the panel's list or a diff pane), so it
    /// follows `git.changed` and refreshes when it comes back (SPEC §12B).
    pub(crate) fn git_shown(&self) -> bool {
        self.diff_on_screen() || self.git_panel_shown()
    }
```

3. After `panel_session`, add:

```rust
    /// The active tab's focused pane is the diff (like [`Self::panel_session`]: not tied to a
    /// window).
    pub(crate) fn panel_diff_focused(&self, project_id: Ulid) -> bool {
        (self.tabs.get(&project_id))
            .and_then(|tabs| tabs.active())
            .is_some_and(|tab| tab.focused() == Some(&PaneKey::Diff))
    }
```

4. `focus_pane`: replace Task 4's `Some(PaneKey::Diff) => window.focus(&self.focus_handle, cx),` with:

```rust
            // The Git keyboard: the panel's list, or the diff pane itself while the panel's
            // Git tab is not drawn (both track `git_focus`).
            Some(PaneKey::Diff) => window.focus(&self.git_focus, cx),
```

5. `sync_panes`: directly before its last statement `self.focus_pane(window, cx);` add the refresh hook:

```rust
        // A Diff leaf that came on screen (opened, restored, its tab shown) loads Git, as the
        // Diff view did when it was shown.
        let drawn = self.diff_on_screen();
        let came = drawn && !self.diff_drawn;
        self.diff_drawn = drawn;
        if came && let Some(project_id) = self.workspace {
            self.refresh_git(project_id, window, cx);
        }
```

**G. `shell/workspace.rs`.**

1. `view()`: delete the arm `View::Diff => self.diff_open(id),` and `View::Editor => self.has_file_tabs(id),` if Task 4 left either; fix the doc comment ("the Editor while the workspace has no open file, the Diff while it has no open diff" -> removed).
2. Delete `diff_open` and its doc comment.
3. `show_view`: delete

```rust
        if view == View::Diff {
            window.focus(&self.git_focus, cx);
        }
```

and in its doc comment "Git (the panel's tab or the diff) coming on screen refreshes its changes" -> "Git (the panel's tab) coming on screen refreshes its changes".

**H. `shell/chrome.rs`.**
In `render_main`, delete the whole arm

```rust
            (View::Diff, _) => main.child(
                div()
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .child(self.render_diff_view(project_id, theme, window, cx))
                    .children(panel),
            ),
```

In the doc of `render_title_bar`, "the tab area (Editor / Diff chips, the terminal tabs, `+`)" -> "the tab area (the tabs, `+`)".

**I. `shell/git_render.rs`.**
Delete `render_diff_view` with its doc comment (the pane's `render_diff_pane` replaces it).

**J. `shell/pane_render.rs`, `shell/hosts.rs`, `shell/detached_render.rs`: chips go, the diff pane and chip title.**

1. `render_tab_area`: replace the `let items = area_items(..)` statement with

```rust
        let items = area_items(&pinned);
```

and delete the arm

```rust
                AreaItem::View(chip) => self
                    .view_chip(chip, project_id, view, theme, cx)
                    .into_any_element(),
```

Replace the scroll-target lines

```rust
        // The key is the scroll target only: the shown chip, or the active tab.
        let key = SharedString::from(match view {
            View::Terminals => format!("tab/{active:?}"),
            other => format!("view/{other:?}"),
        });
        let scroll_to = scroll_index(&items, view, active_slot).map(|index| (key, index));
```

with

```rust
        // The key is the scroll target only: the active tab, while Terminals shows.
        let scroll_to = scroll_index(&items, active_slot)
            .filter(|_| view == View::Terminals)
            .map(|index| (SharedString::from(format!("tab/{active:?}")), index));
```

Fix the doc of `render_tab_area`: "the Editor / Diff chips (while open), the pinned tabs" -> "the pinned tabs"; delete "With a view other than Terminals shown, its chip is the active one and every terminal tab looks inactive." and keep "A click on a tab shows Terminals (`tab_chip`)."
2. Delete `view_chip` (the whole method with its doc comment, from `/// The Editor / Diff chip: icon, name and ✕` to the closing brace before `/// The Board / Docs icon on the right`).
3. `pane_frame`: in Task 4's content `match key`, replace the `PaneKey::Diff` placeholder arm (the muted message `Diff`) with

```rust
            PaneKey::Diff => self.render_diff_pane(project_id, theme, window, cx),
```

4. Tab chip title (the diff is a pane like a file): at the places that compute a tab's label from its focused pane, the diff joins the chain in this order: file (Task 4), diff, session.

| Site | Old (after Task 4 the file alternative may precede it) | New |
|---|---|---|
| `pane_render.rs` `render_tab_area`, `let label = session_label(tab, self.sidebar.sessions());` | `session_label(tab, self.sidebar.sessions())` | `diff_label(tab).or_else(\|\| session_label(tab, self.sidebar.sessions()))` |
| `pane_render.rs` `tab_chip`, same statement | same | same |
| `hosts.rs` `tab_title`, `let label = session_label(tab, self.sidebar.sessions()).unwrap_or(&tab.title);` | `session_label(tab, self.sidebar.sessions())` | `diff_label(tab).or_else(\|\| session_label(tab, self.sidebar.sessions()))` |
| `detached_render.rs`, the drag item `title: session_label(tab, sessions).unwrap_or(&tab.title)..` | `session_label(tab, sessions)` | `diff_label(tab).or_else(\|\| session_label(tab, sessions))` |

(Where Task 4 put its file label in front, keep it first and put `diff_label` right after it.)
5. `tab_chip`'s pane icon `match pane { .. }`: add next to Task 4's `PaneIcon::File(..)` arm

```rust
                PaneIcon::Diff => icon_svg(Icon::FileDiff, theme::ICON_SIZE_SMALL, theme.text_muted)
                    .into_any_element(),
```

**K. `tabs.rs`.**

1. Delete `use crate::board::View;`.
2. In `PaneIcon` add

```rust
    /// The diff pane.
    Diff,
```

3. In `tab_icons`, where Task 4 maps the leaf keys to icons (`PaneKey::File(path) => PaneIcon::File(path.clone())`), add the arm `PaneKey::Diff => PaneIcon::Diff`.
4. Below `session_label` add:

```rust
/// The title a tab shows while its focused pane is the diff and it keeps its default name (a
/// name the user gave the tab always wins), like [`session_label`].
pub fn diff_label(tab: &Tab) -> Option<&'static str> {
    (tab.focused() == Some(&PaneKey::Diff) && has_default_title(tab)).then_some("Diff")
}
```

5. Replace `AreaItem`, `area_items` and `scroll_index` with:

```rust
/// One child of the title bar's tab area, in paint order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AreaItem {
    /// The 1 px rule between two groups.
    Rule,
    /// The `n`th terminal tab this window draws (detached tabs are not counted).
    Tab(usize),
}

/// The children of the tab area: the tabs (`pinned[n]` is whether the `n`th drawn tab is
/// pinned; pinned ones lead) and a rule between the pinned tabs and the rest. A rule only sits
/// between two groups.
pub fn area_items(pinned: &[bool]) -> Vec<AreaItem> {
    let mut items = Vec::with_capacity(pinned.len() + 1);
    for (at, &pin) in pinned.iter().enumerate() {
        if at > 0 && pinned[at - 1] && !pin {
            items.push(AreaItem::Rule);
        }
        items.push(AreaItem::Tab(at));
    }
    items
}

/// The index in `items` of the child to scroll into view: the active tab (`active` is its
/// drawn slot).
pub fn scroll_index(items: &[AreaItem], active: Option<usize>) -> Option<usize> {
    items
        .iter()
        .position(|item| matches!(item, AreaItem::Tab(at) if active == Some(*at)))
}
```

**L. Comments, picking, composer, send dialog, sessions, navigation, inspector, connection (the `View::Diff` checks).**

`shell/comments.rs`:

1. Replace `comment_context` (and its doc comment) with the two functions:

```rust
    /// Whose comments the comment commands act on: the Docs view's while Docs shows, the
    /// diff's while the focused pane is the Diff leaf.
    pub(crate) fn comment_context(&self, window: &Window) -> Option<CommentContext> {
        if self.view() == View::Docs {
            Some(CommentContext::Doc)
        } else if matches!(self.focused_key(window), Some(PaneKey::Diff)) {
            Some(CommentContext::Diff)
        } else {
            None
        }
    }

    /// Whose comment draft is drawn (what typing in it edits): the Docs view's while Docs
    /// shows, else the Diff leaf's while one is drawn. Needs no window: the field only exists
    /// where it is drawn.
    pub(crate) fn drawn_comment_context(&self) -> Option<CommentContext> {
        if self.view() == View::Docs {
            Some(CommentContext::Doc)
        } else if self.diff_on_screen() {
            Some(CommentContext::Diff)
        } else {
            None
        }
    }
```

2. Callers (old -> new):

| Function | Old | New |
|---|---|---|
| `comment_draft` | `match self.comment_context()? {` | `match self.drawn_comment_context()? {` |
| `step_comment(&mut self, forward: bool, cx)` | signature, and `self.comment_context()` | `step_comment(&mut self, forward: bool, window: &Window, cx: &mut Context<'_, Self>)`, `self.comment_context(window)` |
| `reply_selected_comment` | `self.comment_context()` | `self.comment_context(window)` |
| `resolve_selected_comment` | `self.comment_context()` | `self.comment_context(window)` |
| `close_comment_draft` | see 3 | see 3 |
| `comment_key_down` | `self.comment_context()` | `self.drawn_comment_context()` |
| `click_comment_draft` | `self.comment_context()` | `self.drawn_comment_context()` |

3. `close_comment_draft`:

```rust
    pub(crate) fn close_comment_draft(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        let context = self.drawn_comment_context();
        if let Some(comments) = (self.workspace.zip(context))
            .and_then(|(id, context)| self.comments_mut(id, context))
        {
            comments.draft = None;
        }
        if matches!(context, Some(CommentContext::Diff)) {
            window.focus(&self.git_focus, cx);
        } else {
            window.focus(&self.focus_handle, cx);
        }
        cx.notify();
    }
```

4. In `submit_comment`'s answer closure replace `if shell.view() == View::Diff {` with `if matches!(shell.drawn_comment_context(), Some(CommentContext::Diff)) {`.

`shell/picking.rs`: `pick_step(&mut self, down: bool, cx)` -> `pick_step(&mut self, down: bool, window: &Window, cx: &mut Context<'_, Self>)` and `self.comment_context()` -> `self.comment_context(window)`.

`shell/sessions.rs`:

1. `Command::PickDown | Command::PickUp` body: `self.pick_step(*command == Command::PickDown, cx);` -> `self.pick_step(*command == Command::PickDown, window, cx);`.
2. `Command::AddComment` and `Command::SendComments` bodies: `self.comment_context()` -> `self.comment_context(window)` (two sites); `Command::NextComment => self.step_comment(true, cx)` -> `self.step_comment(true, window, cx)`, `PreviousComment` likewise.
3. In the comment-draft guard at the top of `run` (`if self.comment_draft().is_some()`): unchanged.
4. `Command::OpenInZed`: the Git arm becomes

```rust
                // Git opens the selected changed file while the diff pane is the focused pane.
                Some(project_id)
                    if matches!(self.focused_key(window), Some(PaneKey::Diff))
                        && self
                            .git
                            .get(&project_id)
                            .is_some_and(|view| view.selected.is_some()) =>
                {
                    self.open_git_file_in_zed(project_id, cx);
                }
```

5. `Command::ShowDiff`:

```rust
            Command::ShowDiff => {
                if let Some(project_id) = self.workspace {
                    self.open_diff(project_id, Place::NewTab, window, cx);
                }
            }
```

`shell/composer.rs`:

1. In `prompt_vars`, delete the `let views = ..;` block and the `let selection = views.into_iter().find_map(..).map(..);` statement (Task 4 left a two-closure variant of it) and put in their place:

```rust
        // The pane in front first (one picked earlier elsewhere may linger).
        let diff = || self.picked_diff_lines(project_id);
        let file = || {
            (self.panel_file(project_id))
                .and_then(|path| self.picked_file_lines(&path, project_id))
        };
        let picked = if self.panel_diff_focused(project_id) {
            diff().or_else(file)
        } else {
            file().or_else(diff)
        };
        let selection = picked.map(|(_, _, _, text)| text);
```

2. Replace `picked_lines(&self, project_id, view: View)` (Task 4 left its `View::Diff` and `View::Docs` arms) with:

```rust
    /// The new-side lines picked in `project_id`'s diff: the file, first and last line, and
    /// their text (§20B.4).
    pub(crate) fn picked_diff_lines(
        &self,
        project_id: Ulid,
    ) -> Option<(String, u32, u32, String)> {
        let git = self.git.get(&project_id)?;
        let (Some(target), Some(selection), git_view::Diff::Shown(shown)) =
            (&git.selected, git.comments.selection, &git.diff)
        else {
            return None;
        };
        let text = shown.new_side_text(selection)?;
        Some((target.path.clone(), selection.start, selection.end, text))
    }
```

(the Docs pick is read in `send_dialog.rs` as `docs.picked_lines()`; no other caller of the old `picked_lines` exists).

`shell/send_dialog.rs`, `open_selection_dialog`: Task 4 left `View::Terminals if file.is_some() => {..}` (keep it as is), then `view @ (View::Diff | View::Docs) => {..}`, then the terminal arm.
Replace the `view @ (View::Diff | View::Docs)` arm with:

```rust
            view @ (View::Docs | View::Terminals) if view == View::Docs || diff => {
                let picked = if diff {
                    self.picked_diff_lines(project_id)
                } else {
                    (self.docs.get(&project_id)).and_then(|docs| docs.picked_lines())
                }
                .filter(|(_, _, _, text)| !text.trim().is_empty());
                let Some((path, start, end, text)) = picked else {
                    return;
                };
                let task = self
                    .git
                    .get(&project_id)
                    .filter(|_| diff)
                    .and_then(|git| git.since_session(sessions));
                let prefill = composer::selection_prefill(Some((&path, start, end)), &text);
                (format!("{path}:{start}-{end}"), prefill, task)
            }
```

and, above the `match self.view()`, next to Task 4's `let file = self.focused_file(window);`, add `let diff = matches!(self.focused_key(window), Some(PaneKey::Diff));`.
The doc comment of the function stays true ("the picked lines of Files ..., Docs or the Git diff").

`shell/navigation.rs`, palette context construction: replace `diff: workspace.is_some_and(|id| self.diff_open(id)),` with

```rust
            diff_pane: matches!(self.focused_key(window), Some(PaneKey::Diff)),
```

`shell/inspector.rs`, `panel_changed`: replace

```rust
            if self.view() == View::Diff {
                // The diff card holds `git_focus` itself.
                window.focus(&self.git_focus, cx);
            } else {
                self.focus_center(window, cx);
            }
```

with `self.focus_center(window, cx);` (`focus_pane` gives a focused Diff leaf `git_focus`, which the pane tracks now that the panel's Git tab is gone). In the doc of `focus_center` delete the sentence "(In the Git / Diff view the diff card holds `git_focus` itself: `git_shown` stays true there.)".

`shell/connection.rs`, `Shell::shown`: add `diff: self.diff_on_screen(),` after `all_board: self.all_board_shown(),`.

**M. `palette.rs`.**

1. `Context`: delete

```rust
    /// The active workspace has an open diff (the Diff view exists).
    pub diff: bool,
```

and add after `pub git: bool,`:

```rust
    /// The focused pane is the diff: the diff comments and Send selection act on it.
    pub diff_pane: bool,
```

and in the doc of `git`: "(the panel's Git tab or the Diff view)" -> "(the panel's Git tab or a diff pane)".
2. `available`: delete the arm `Command::ShowDiff => context.workspace && context.diff,` and add `| Command::ShowDiff` to the `context.workspace` group (`Command::NewShellTab | .. | Command::FocusSidebar => context.workspace`).
3. `Command::SendSelection` (after Task 4: `context.file_pane || in_view(View::Diff) || in_view(View::Docs) || (on_panes && context.terminal)`) -> `context.file_pane || context.diff_pane || in_view(View::Docs) || (on_panes && context.terminal)`.
4. The comment group: `=> in_view(View::Docs) || in_view(View::Diff),` -> `=> in_view(View::Docs) || context.diff_pane,`.

**N. `redraw.rs`.**

1. `Shown`: add after `pub all_board: bool,`

```rust
    /// A Diff pane is drawn (in the main window or a visible detached one).
    pub diff: bool,
```

2. `event_needs_redraw`: replace the `Event::CommentUpdated(comment)` arm and its comment with

```rust
        // Diff comments are drawn by a Diff pane, doc comments in Docs.
        Event::CommentUpdated(comment) => {
            in_workspace(comment.project_id)
                && match comment.context {
                    CommentContext::Diff => shown.diff,
                    CommentContext::Doc => shown.view == View::Docs,
                }
        }
```

**O. `board.rs`.**
Delete the variant `Diff` with its doc comment; `ALL` becomes `pub const ALL: [View; 3] = [View::Terminals, View::Board, View::Docs];` (after Task 4 it had four entries); delete the `View::Diff => "Diff",` arm of `title` and the `View::Diff => Command::ShowDiff,` arm of `command`.

**P. `keymap.rs`.**
Doc of `ShowDiff`: `/// Show the Diff view while a diff is open.` -> `/// Open the diff in a new tab, or focus its pane while it is open.`

- [ ] **Step 9: Compile**

Run: `cargo check -p mkrew-ui --all-targets -j 4`
Expected: no errors.
Likely leftovers if it fails: a name the greps below still find (`View::Diff`, `diff_open`, `open_git_diff`, `follow_git_diff`, `view_chip`, `git_diff_list`, `git_diff_layout`, `git_diff_looks`, `git_diff_scroll_to`, `git_diff_reveal`, `render_diff_view`, `picked_lines(project_id`), an unused import (`View` in `tabs.rs`), or a `Shown {..}` / `Context {..}` literal elsewhere without the new field.
Search `crates/` for each of those names; every hit must be gone.

- [ ] **Step 10: Run the crate tests, expect PASS**

Run: `cargo test -p mkrew-ui -j 4`
Expected: PASS, including `the_selection_follows_the_status_and_an_empty_status_closes_nothing`, `the_diff_pane_knows_why_it_has_no_file`, `a_tab_whose_focused_pane_is_the_diff_is_titled_diff`, `the_tab_area_orders_pinned_tabs_and_the_rest`, `the_scroll_target_is_the_active_tab`, `diff_and_git_commands_follow_what_is_on_screen`, `comments_and_ai_answers_follow_the_view_that_draws_them`, `each_view_maps_to_its_own_show_command`.

- [ ] **Step 11: Lint and format**

Run: `cargo fmt --all`
Run: `cargo clippy -p mkrew-ui --all-targets -j 4 -- -D warnings`
Expected: clean.

- [ ] **Step 12: Commit**

```
git add crates/mkrew-ui/src
git -c user.email=tungnv975@gmail.com commit -m "feat(ui): the diff is a leaf (open_diff, diff pane, per-workspace diff state); remove the Diff view and chips"
```

**Checks left to the smoke (Task 9), not unit tests:**

- Click a changed file in the Git panel: the Diff opens as a new tab after the active one; click another file: it follows in place; Esc and the toolbar ✕ close the leaf and the tab it was alone in shows "No panes in this tab".
- Commit everything: the open Diff pane shows "Nothing to commit." and stays; a new change selects its file again.
- Diff pane beside a terminal: ↑ / ↓ in the Git list move it; Ctrl+Alt+M comments only after a click on the diff pane (focused pane = diff); Open in Zed acts on the diff's file then.
- Restart with a Diff leaf in a layout: it loads Git on first draw (`sync_panes` hook) and shows the selection.
- Detached window holding the Diff tab: it follows `git.changed`.
- The card inside the pane frame keeps the pane's rounded corners (the card has no radius of its own now; if the frame does not clip, give the card `rounded(px(radius::MEDIUM))`).

---

### Task 6: Four-edge drops for files, tabs and panes

Tasks 1-5 have landed.
After this task every drag type (Files tree row, tab, pane) splits a pane at the edge nearest to the pointer, a pane or a file row dropped on the tab strip gets its own tab, a tree row dropped on a shown composer inserts the reference, and OS files open on file panes, diff panes and the strip.
`split::drop_axis` is gone.

Facts that shape the design (checked in the worktree and in `gpui-pre-0.3.7`, `src/elements/div.rs`):

- A drop reaches only the elements whose hitbox is hovered, innermost first, and the first listener of a payload type takes the drag (`cx.active_drag.take()`) and stops propagation.
  So the drag strip of a pane (`pane_edge`, it blocks the mouse) must repeat the pane frame's `on_drop` handlers, and a composer inside the frame can take a tree row before the frame does.
- One element may carry only one `on_drop` per payload type (a second one makes gpui panic on `expect`), so each handler set below has exactly one listener per type.
- `on_drag_move` fires in the capture phase by bounds, not by hover, parent before child.
  That is why the frame's hover code runs before the composer's and the composer can clear the half the frame lit.
- OS file drops do not use `on_drop`: `drop_catcher` + `drop_hover` (gpui-pre 0.3.7 keeps keyboard modality during OLE drags, see the ponytail comment on `drop_catcher`).

**Files:**
- Create: `crates/mkrew-ui/src/drop_zone.rs` (pure: `edge_half`, `classify_os_files`, `REFUSED_OS_FILES`, tests)
- Modify: `crates/mkrew-ui/src/lib.rs` (`pub mod drop_zone;` before `pub mod editor;`, after `docs` / `documents`)
- Modify: `crates/mkrew-ui/src/tabs.rs` (`Tabs::move_to_empty_tab`, next to `merge_tab`) and `crates/mkrew-ui/src/tabs/tests.rs` (3 tests)
- Modify: `crates/mkrew-ui/src/split.rs` (delete `drop_axis` and its test)
- Replace: `crates/mkrew-ui/src/shell/pane_drop.rs` (whole file)
- Modify: `crates/mkrew-ui/src/shell/pane_render.rs` (`render_tab`, `pane_frame`, `pane_edge`, `DropTarget`, `drop_catcher`, `tab_area_frame`)
- Modify: `crates/mkrew-ui/src/shell/chrome.rs` and `crates/mkrew-ui/src/shell/detached_render.rs` (one `render_tab` argument each)
- Modify: `crates/mkrew-ui/src/shell/navigation.rs` (delete `drop_tree_path`; add `drop_os_files`, `notes_folder`)
- Modify: `crates/mkrew-ui/src/shell/composer.rs` (`drop_tree_on_composer`, `composer_drops`, three call sites)
- Modify: `crates/mkrew-ui/src/main.rs` (import list)
- Test: `drop_zone.rs` (in-file `mod tests`), `tabs/tests.rs`

**Interfaces:**

Consumes (Tasks 1-5, exact names from the contracts):

```rust
// split.rs (Task 1)
pub enum PaneKey { Session(Ulid), File(String), Diff }   // PaneKey::session(&self) -> Option<Ulid>
pub enum Leaf { Session{..}, File{..}, Diff{..} }         // Leaf::file(impl Into<String>), Leaf::key()
pub enum Edge { Left, Right, Top, Bottom }                // Edge::nearest(x: f32, y: f32), Copy
Node::leaf_of(&self, &PaneKey) -> Option<&Leaf>
// tabs.rs (Task 1)
Tabs::{tab(Ulid), tab_with(&PaneKey), move_pane(&PaneKey, &PaneKey, Edge) -> Option<Vec<TabSetLayoutParams>>,
       merge_tab(Ulid, &PaneKey, Edge) -> Option<[TabSetLayoutParams; 2]>, close_leaf(&PaneKey), insert_leaf(Ulid, None, Edge, Leaf)}
Tab::{keys() -> Vec<PaneKey>, contains(&PaneKey), layout() -> Option<&Node>, focused() -> Option<&PaneKey>}
// Task 4 / Task 5
Shell::open_document(&mut self, project_id: Ulid, path: String, line: Option<(u32, u32)>, place: Place, window, cx)
Shell::open_diff(&mut self, project_id: Ulid, place: Place, window, cx)
Shell::create_tab_then(&mut self, project_id: Ulid, position: Option<f64>,
    then: impl FnOnce(&mut Shell, Ulid /*tab_id*/, &mut Window, &mut Context<'_, Shell>) + 'static, window, cx)
Shell::position_after_active(&self, project_id: Ulid) -> Option<f64>
documents::Place { Preview, NewTab, Split(PaneKey, Edge) }
```

Produces:

```rust
// drop_zone.rs (pure)
pub const REFUSED_OS_FILES: &str = "Only files inside the workspace or the notes folder open in Mkrew";
pub fn edge_half(edge: Edge) -> Rect;
pub struct OsFiles { pub open: Vec<String>, pub refused: usize }
pub fn classify_os_files(root: &Path, notes: Option<&Path>, files: &[PathBuf]) -> OsFiles;
// tabs.rs
Tabs::move_to_empty_tab(&mut self, key: &PaneKey, into: Ulid) -> Option<[TabSetLayoutParams; 2]>  // [into's write, source's write]
// shell/pane_drop.rs
pub(crate) struct DraggedPane { pub(crate) project_id: Ulid, pub(crate) key: PaneKey, pub(crate) title: SharedString }
pub(crate) struct PaneDrop { pub(crate) target: PaneKey, pub(crate) edge: Edge }
pub(crate) trait PaneDragged { fn takes(..) -> bool; fn land(..); fn ended(..) }   // DraggedPane, DraggedItem, DraggedPath
Shell::take_pane_drop(&mut self, target: &PaneKey) -> Option<Edge>
Shell::drop_leaf(&mut self, project_id: Ulid, at: Option<(PaneKey, Edge)>, leaf: Leaf, window, cx)
Shell::pane_to_new_tab(&mut self, project_id: Ulid, key: &PaneKey, window, cx)
Shell::pane_drop_target(&self, frame: Div, project_id: Ulid, key: &PaneKey, cx) -> Div   // hover + drops + OS catcher
Shell::pane_drops<E: InteractiveElement>(el: E, project_id: Ulid, key: &PaneKey, cx) -> E  // the drop set alone (drag strip)
Shell::pane_drop_overlay(&self, key: &PaneKey, theme, cx) -> Option<Div>
Shell::strip_drops(&self, strip: Div, theme, cx) -> Div
// pane_render.rs
enum DropTarget { Pane(PaneKey), Strip, TaskPrompt }   // no longer Copy
```

Task 8 reuses `take_pane_drop`, `pane_drop_overlay` and `PaneDragged` for `DraggedNew` (implement `PaneDragged` for it and add one `Self::hover_of::<DraggedNew, _>` / `Self::drop_of::<DraggedNew, _>` pair next to the existing ones in `pane_drop_target` and `pane_drops`).

Deviations from the contracts (the plan owner should carry them into Task 8 and Task 9):

- `DraggedPane` has a third field `project_id`.
  A file key is a path, so two workspaces shown in two windows can both hold `README.md`; without the project a pane drag could be matched to the wrong workspace.
- `PaneDrop` loses `Copy` (its key is a `String` for files); `DropTarget` loses `Copy` for the same reason.
- `render_tab`, `pane_frame` and `pane_edge` take `project_id: Ulid` (both callers know it).
- The tab strip of a detached window does not take pane or file drops (a tab created there would open in the main window; docking a new tab in a detached window is not in the spec).
  It keeps the tab reorder drop.

---

- [ ] **Step 1: Write the failing tests for `edge_half` and the OS-file classifier**

Create `crates/mkrew-ui/src/drop_zone.rs` with only the tests, and register the module in `crates/mkrew-ui/src/lib.rs` (`pub mod drop_zone;` right before `pub mod editor;`).

```rust
//! Pure parts of the four-edge drops (spec 6.2, 6.3): the half of a pane a drop fills, and which
//! OS files may open in Mkrew.

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use super::*;
    use crate::split::{Edge, Rect};

    const EDGES: [Edge; 4] = [Edge::Left, Edge::Right, Edge::Top, Edge::Bottom];

    fn base() -> PathBuf {
        std::env::temp_dir().join("mkrew-drop-zone")
    }

    fn slashed(path: &Path) -> String {
        path.to_string_lossy().replace('\\', "/")
    }

    fn contains(rect: Rect, x: f64, y: f64) -> bool {
        rect.x <= x && x <= rect.x + rect.w && rect.y <= y && y <= rect.y + rect.h
    }

    #[test]
    fn each_edge_fills_the_half_on_its_side() {
        let half = |x, y, w, h| Rect { x, y, w, h };
        assert_eq!(edge_half(Edge::Left), half(0.0, 0.0, 0.5, 1.0));
        assert_eq!(edge_half(Edge::Right), half(0.5, 0.0, 0.5, 1.0));
        assert_eq!(edge_half(Edge::Top), half(0.0, 0.0, 1.0, 0.5));
        assert_eq!(edge_half(Edge::Bottom), half(0.0, 0.5, 1.0, 0.5));
    }

    #[test]
    fn the_half_always_holds_the_pointer_that_picked_its_edge() {
        for i in 0..=10 {
            for j in 0..=10 {
                let (x, y) = (i as f32 / 10.0, j as f32 / 10.0);
                let half = edge_half(Edge::nearest(x, y));
                assert!(
                    contains(half, f64::from(x), f64::from(y)),
                    "({x}, {y}) is outside {half:?}"
                );
            }
        }
    }

    #[test]
    fn the_four_halves_are_distinct_and_cover_half_the_pane_each() {
        for (at, edge) in EDGES.iter().enumerate() {
            let half = edge_half(*edge);
            assert!((half.w * half.h - 0.5).abs() < f64::EPSILON);
            for other in &EDGES[at + 1..] {
                assert_ne!(half, edge_half(*other));
            }
        }
    }

    #[test]
    fn files_inside_the_root_open_by_their_root_relative_slashed_path() {
        let root = base().join("proj");
        let files = [root.join("src").join("a.rs"), root.join("README.md")];
        let found = classify_os_files(&root, None, &files);
        assert_eq!(found.open, ["src/a.rs", "README.md"]);
        assert_eq!(found.refused, 0);
    }

    #[test]
    fn files_outside_the_root_and_the_notes_folder_are_refused_and_counted() {
        let root = base().join("proj");
        let files = [
            root.join("a.rs"),
            base().join("other").join("b.txt"),
            base().join("elsewhere").join("c.txt"),
        ];
        let found = classify_os_files(&root, None, &files);
        assert_eq!(found.open, ["a.rs"]);
        assert_eq!(found.refused, 2);
    }

    #[test]
    fn a_path_that_only_shares_a_name_prefix_with_the_root_is_outside() {
        let root = base().join("proj");
        let found = classify_os_files(&root, None, &[base().join("proj2").join("a.rs")]);
        assert!(found.open.is_empty());
        assert_eq!(found.refused, 1);
    }

    #[test]
    fn a_file_in_the_notes_folder_outside_the_root_keeps_its_absolute_path() {
        let root = base().join("proj");
        let notes = base().join("notes");
        let note = notes.join("sub").join("n.md");
        let found = classify_os_files(&root, Some(&notes), &[note.clone()]);
        assert_eq!(found.open, [slashed(&note)]);
        assert_eq!(found.refused, 0);
    }

    #[test]
    fn the_root_wins_when_the_notes_folder_is_inside_it() {
        let root = base().join("proj");
        let notes = root.join("notes");
        let found = classify_os_files(&root, Some(&notes), &[notes.join("n.md")]);
        assert_eq!(found.open, ["notes/n.md"]);
    }

    #[test]
    fn a_folder_path_itself_and_relative_or_dotdot_paths_are_refused() {
        let root = base().join("proj");
        let files = [
            root.clone(),
            PathBuf::from("src/a.rs"),
            root.join("src").join("..").join("..").join("secret.txt"),
        ];
        let found = classify_os_files(&root, Some(&base()), &files);
        assert!(found.open.is_empty());
        assert_eq!(found.refused, 3);
    }

    #[cfg(windows)]
    #[test]
    fn windows_paths_match_the_root_without_case() {
        let root = Path::new(r"C:\Work\Proj");
        let found = classify_os_files(root, None, &[PathBuf::from(r"c:\work\proj\Src\A.rs")]);
        assert_eq!(found.open, ["Src/A.rs"]);
    }
}
```

- [ ] **Step 2: Run the tests, expect the build to fail**

Run: `cargo test -p mkrew-ui drop_zone -j 4`
Expected: FAIL to compile with `error[E0425]: cannot find function `edge_half` in this scope` and `cannot find function `classify_os_files` in this scope`.

- [ ] **Step 3: Implement the pure functions**

Insert this above `#[cfg(test)]` in `crates/mkrew-ui/src/drop_zone.rs` (add the `use` lines under the module doc comment):

```rust
use std::ffi::OsStr;
use std::path::{Component, Path, PathBuf};

use crate::split::{Edge, Rect};

/// The toast of an OS drop that held files outside the workspace and the notes folder.
pub const REFUSED_OS_FILES: &str =
    "Only files inside the workspace or the notes folder open in Mkrew";

/// The half of a pane, as fractions of the pane, on `edge`'s side: what the drop overlay tints
/// and where the dropped leaf will sit.
pub fn edge_half(edge: Edge) -> Rect {
    let (x, y, w, h) = match edge {
        Edge::Left => (0.0, 0.0, 0.5, 1.0),
        Edge::Right => (0.5, 0.0, 0.5, 1.0),
        Edge::Top => (0.0, 0.0, 1.0, 0.5),
        Edge::Bottom => (0.0, 0.5, 1.0, 0.5),
    };
    Rect { x, y, w, h }
}

/// What an OS file drop may open.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct OsFiles {
    /// Layout paths (the `PaneKey::File` string) of the files that open, in drop order.
    pub open: Vec<String>,
    /// How many files were outside the workspace root and the notes folder.
    pub refused: usize,
}

/// Sorts dropped OS `files` (folders already removed by the caller) into those that open and
/// those that do not.
/// A file inside `root` opens by its root-relative path with `/` separators; a file inside the
/// notes folder (`notes`, the absolute folder the daemon accepts, `None` while there is none)
/// but outside the root opens by its absolute path with `/` separators, as the daemon names a
/// new note outside the root.
/// Relative paths and paths with `..` never open: the daemon refuses them too.
pub fn classify_os_files(root: &Path, notes: Option<&Path>, files: &[PathBuf]) -> OsFiles {
    let mut found = OsFiles::default();
    for file in files {
        match leaf_path(root, notes, file) {
            Some(path) => found.open.push(path),
            None => found.refused += 1,
        }
    }
    found
}

fn leaf_path(root: &Path, notes: Option<&Path>, file: &Path) -> Option<String> {
    if !file.is_absolute() || file.components().any(|part| part == Component::ParentDir) {
        return None;
    }
    if let Some(inside) = relative_to(root, file) {
        return Some(slashed(&inside));
    }
    notes
        .and_then(|notes| relative_to(notes, file))
        .map(|_| slashed(file))
}

/// `path` below `base`, component by component (so `proj2` is not below `proj`); `None` for
/// `base` itself and for anything outside it. Windows compares names without ASCII case.
fn relative_to(base: &Path, path: &Path) -> Option<PathBuf> {
    let base: Vec<Component<'_>> = base.components().collect();
    let path: Vec<Component<'_>> = path.components().collect();
    let below = path.len() > base.len()
        && base
            .iter()
            .zip(&path)
            .all(|(a, b)| same(a.as_os_str(), b.as_os_str()));
    below.then(|| path[base.len()..].iter().collect())
}

fn same(a: &OsStr, b: &OsStr) -> bool {
    if cfg!(windows) {
        a.eq_ignore_ascii_case(b)
    } else {
        a == b
    }
}

fn slashed(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}
```

- [ ] **Step 4: Run the tests, expect PASS**

Run: `cargo test -p mkrew-ui drop_zone -j 4`
Expected: PASS, 8 tests on Linux and macOS, 9 on Windows (`windows_paths_match_the_root_without_case` runs there).

- [ ] **Step 5: Write the failing tests for `Tabs::move_to_empty_tab`**

Append to `crates/mkrew-ui/src/tabs/tests.rs` (it already has `wire`, `pane` and `use super::*;`):

```rust
#[test]
fn a_pane_moves_to_an_empty_tab_with_its_whole_leaf() {
    use crate::split::PaneKey;
    let project = Ulid::new();
    let session = Ulid::new();
    let split = json!({"split":"vertical","ratio":0.5,"children":[
        {"session": session.to_string(), "chat": true},
        {"file": "notes/a.md", "md": "split"},
    ]});
    let source = wire(project, "one", 1.0, split);
    let empty = wire(project, "two", 2.0, serde_json::Value::Null);
    let (source_id, empty_id) = (source.id, empty.id);
    let mut tabs = Tabs::new(project);
    tabs.load(vec![source, empty]);
    let key = PaneKey::File("notes/a.md".to_owned());
    let leaf = (tabs.tab(source_id))
        .and_then(Tab::layout)
        .and_then(|node| node.leaf_of(&key))
        .cloned();
    assert!(leaf.is_some());

    let [into, out] = tabs.move_to_empty_tab(&key, empty_id).expect("the pane moves");

    assert_eq!((into.tab_id, out.tab_id), (empty_id, source_id));
    assert_eq!(
        tabs.tab(source_id).map(Tab::keys),
        Some(vec![PaneKey::Session(session)])
    );
    let moved = tabs.tab(empty_id).expect("the new tab");
    assert_eq!(moved.keys(), vec![key.clone()]);
    assert_eq!(moved.layout().and_then(|node| node.leaf_of(&key)).cloned(), leaf);
    assert_eq!(moved.focused(), Some(&key));
}

#[test]
fn a_pane_alone_in_its_tab_does_not_move_out() {
    use crate::split::PaneKey;
    let project = Ulid::new();
    let session = Ulid::new();
    let one = wire(project, "one", 1.0, pane(session));
    let empty = wire(project, "two", 2.0, serde_json::Value::Null);
    let (one_id, empty_id) = (one.id, empty.id);
    let mut tabs = Tabs::new(project);
    tabs.load(vec![one, empty]);

    assert!(tabs
        .move_to_empty_tab(&PaneKey::Session(session), empty_id)
        .is_none());
    assert_eq!(
        tabs.tab(one_id).map(Tab::keys),
        Some(vec![PaneKey::Session(session)])
    );
    assert!(tabs.tab(empty_id).is_some_and(|tab| tab.layout().is_none()));
}

#[test]
fn a_pane_does_not_move_into_a_tab_that_has_panes_or_is_missing() {
    use crate::split::PaneKey;
    let project = Ulid::new();
    let [a, b, c] = [Ulid::new(), Ulid::new(), Ulid::new()];
    let split = json!({"split":"vertical","ratio":0.5,"children":[pane(a), pane(b)]});
    let source = wire(project, "one", 1.0, split);
    let busy = wire(project, "two", 2.0, pane(c));
    let (source_id, busy_id) = (source.id, busy.id);
    let mut tabs = Tabs::new(project);
    tabs.load(vec![source, busy]);
    let key = PaneKey::Session(a);

    assert!(tabs.move_to_empty_tab(&key, busy_id).is_none());
    assert!(tabs.move_to_empty_tab(&key, Ulid::new()).is_none());
    assert!(tabs.move_to_empty_tab(&key, source_id).is_none());
    assert_eq!(
        tabs.tab(source_id).map(Tab::keys),
        Some(vec![PaneKey::Session(a), PaneKey::Session(b)])
    );
}
```

- [ ] **Step 6: Run the tests, expect the build to fail**

Run: `cargo test -p mkrew-ui empty_tab -j 4`
Expected: FAIL to compile with `error[E0599]: no method named `move_to_empty_tab` found for struct `Tabs``.

- [ ] **Step 7: Implement `Tabs::move_to_empty_tab`**

In `crates/mkrew-ui/src/tabs.rs`, insert directly after `merge_tab` (before the doc comment of `close_leaf`):

```rust
    /// Pane `key` leaves its tab, whole leaf (chat flag, Markdown mode), for the empty tab
    /// `into`, where it becomes the root and takes the focus: a pane dragged onto the tab
    /// strip gets a tab of its own. Returns the writes, `into`'s first, then the source's, so
    /// a failure between them duplicates the pane rather than losing it.
    /// `None`, nothing changed, if no tab holds `key`, `into` is missing, is the source or has
    /// panes, or `key` is the only pane of its tab.
    pub fn move_to_empty_tab(
        &mut self,
        key: &PaneKey,
        into: Ulid,
    ) -> Option<[TabSetLayoutParams; 2]> {
        let source = self.tab_with(key)?;
        if source.id == into || source.keys().len() < 2 || self.tab(into)?.layout().is_some() {
            return None;
        }
        let leaf = source.layout()?.leaf_of(key)?.clone();
        let out = self.close_leaf(key)?;
        // `into` exists and is empty (checked above), so this cannot refuse.
        let inn = self.insert_leaf(into, None, Edge::Right, leaf)?;
        Some([inn, out])
    }
```

- [ ] **Step 8: Run the tests, expect PASS**

Run: `cargo test -p mkrew-ui empty_tab -j 4`
Expected: PASS, 3 tests.

- [ ] **Step 9: Commit the pure pieces**

```bash
cargo fmt --all
cargo clippy -p mkrew-ui --all-targets -j 4 -- -D warnings
git add crates/mkrew-ui/src/drop_zone.rs crates/mkrew-ui/src/lib.rs crates/mkrew-ui/src/tabs.rs crates/mkrew-ui/src/tabs/tests.rs
git -c user.email=tungnv975@gmail.com commit -m "feat(ui): pure edge half and OS-file classifier for four-edge drops, Tabs::move_to_empty_tab"
```

- [ ] **Step 10: Replace `shell/pane_drop.rs`**

Overwrite `crates/mkrew-ui/src/shell/pane_drop.rs` with this whole file.
It keeps `draggable_pane`, `pane_drop_target`, `pane_drop_overlay`, `drop_pane` and `drop_tab_on_pane` under the same names; every one now works on `PaneKey` and `Edge`.

```rust
//! Drops on panes and on the tab strip (SPEC §10, §20B.5).
//! A Files tree row, a tab (from the tab area) or a pane (by the grip on its top edge) dragged
//! over a pane lights the half of the pane on the side of the edge nearest to the pointer; the
//! drop puts the dragged leaf there.
//! One handler set serves every drag type ([`PaneDragged`]); the pane frame and its drag strip
//! both carry it, because a drop only reaches the element under the pointer.

use crate::*;

/// Payload of a pane drag: the pane `key` of workspace `project_id`.
#[derive(Debug, Clone)]
pub(crate) struct DraggedPane {
    pub(crate) project_id: Ulid,
    pub(crate) key: PaneKey,
    pub(crate) title: SharedString,
}

/// Where a drag over a pane lands: on the `edge` side of `target`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PaneDrop {
    pub(crate) target: PaneKey,
    pub(crate) edge: Edge,
}

/// A drag payload that may land on a pane. Task 8 adds `DraggedNew`.
pub(crate) trait PaneDragged: Sized + 'static {
    /// Whether the drag may drop on `target`'s pane of workspace `project_id`.
    fn takes(&self, shell: &Shell, project_id: Ulid, target: &PaneKey) -> bool;

    /// The drop: the dragged leaf goes to the `edge` side of `target`.
    fn land(
        &self,
        shell: &mut Shell,
        project_id: Ulid,
        target: PaneKey,
        edge: Edge,
        window: &mut Window,
        cx: &mut Context<'_, Shell>,
    );

    /// Runs on every release over the pane, before `land`, also when nothing lands.
    fn ended(&self, _shell: &mut Shell, _cx: &mut Context<'_, Shell>) {}
}

impl PaneDragged for DraggedPane {
    fn takes(&self, shell: &Shell, project_id: Ulid, target: &PaneKey) -> bool {
        self.project_id == project_id
            && self.key != *target
            && (shell.tabs.get(&project_id)).is_some_and(|tabs| tabs.tab_with(&self.key).is_some())
    }

    fn land(
        &self,
        shell: &mut Shell,
        project_id: Ulid,
        target: PaneKey,
        edge: Edge,
        window: &mut Window,
        cx: &mut Context<'_, Shell>,
    ) {
        shell.drop_pane(project_id, &self.key, &target, edge, window, cx);
    }
}

impl PaneDragged for DraggedItem {
    fn takes(&self, shell: &Shell, project_id: Ulid, target: &PaneKey) -> bool {
        shell.dragged_tab(self).is_some_and(|(project, tab)| {
            project == project_id && !tab.contains(target) && tab.layout().is_some()
        })
    }

    fn land(
        &self,
        shell: &mut Shell,
        _project_id: Ulid,
        target: PaneKey,
        edge: Edge,
        window: &mut Window,
        cx: &mut Context<'_, Shell>,
    ) {
        shell.drop_tab_on_pane(self, &target, edge, window, cx);
    }

    fn ended(&self, shell: &mut Shell, cx: &mut Context<'_, Shell>) {
        shell.end_tab_drag(cx);
    }
}

impl PaneDragged for DraggedPath {
    /// Files only: a folder row opens nothing on a pane (it still drops on a composer).
    fn takes(&self, _shell: &Shell, project_id: Ulid, target: &PaneKey) -> bool {
        !self.is_dir
            && self.project_id == project_id
            && !matches!(target, PaneKey::File(path) if *path == self.path)
    }

    fn land(
        &self,
        shell: &mut Shell,
        project_id: Ulid,
        target: PaneKey,
        edge: Edge,
        window: &mut Window,
        cx: &mut Context<'_, Shell>,
    ) {
        shell.drop_leaf(project_id, Some((target, edge)), Leaf::file(&self.path), window, cx);
    }
}

/// The element id (and hover group) of a pane's grip.
pub(crate) fn grip_id(key: &PaneKey) -> SharedString {
    SharedString::from(format!("pane-grip-{key:?}"))
}

impl Shell {
    /// What the chip of a dragged pane says: a session's title, a file's name, "Diff".
    fn pane_title(&self, key: &PaneKey) -> SharedString {
        match key {
            PaneKey::Session(id) => (self.sidebar.sessions().iter())
                .find(|s| s.id == *id)
                .map(|info| {
                    run::pane_title(info, self.ports.get(info.id))
                        .unwrap_or_else(|| info.agent_profile.clone())
                })
                .unwrap_or_default()
                .into(),
            PaneKey::File(path) => path.rsplit('/').next().unwrap_or(path).to_owned().into(),
            PaneKey::Diff => "Diff".into(),
        }
    }

    /// Makes `handle` (the top-edge strip of the pane `key`) draggable onto another pane.
    pub(crate) fn draggable_pane(
        &self,
        handle: gpui::Div,
        project_id: Ulid,
        key: &PaneKey,
        theme: &Theme,
    ) -> gpui::Stateful<gpui::Div> {
        let pane = DraggedPane {
            project_id,
            key: key.clone(),
            title: self.pane_title(key),
        };
        let style = PopupStyle::of(theme);
        let accent = theme.accent;
        handle
            .id(grip_id(key))
            .on_drag(pane, move |pane, offset, _, cx| {
                let (style, title) = (style.clone(), pane.title.clone());
                cx.new(|_| DragChip {
                    style,
                    accent,
                    title,
                    offset,
                })
            })
    }

    /// Makes the frame of the pane `key` take every drag: the half that lights up, the drops, and
    /// the OS files (`drop_hover` + `drop_catcher`).
    /// Call it before the frame gets its other children: the catcher must paint first.
    pub(crate) fn pane_drop_target(
        &self,
        frame: gpui::Div,
        project_id: Ulid,
        key: &PaneKey,
        cx: &mut Context<'_, Self>,
    ) -> gpui::Div {
        let frame = Self::hover_of::<DraggedPane, _>(frame, project_id, key, cx);
        let frame = Self::hover_of::<DraggedItem, _>(frame, project_id, key, cx);
        let frame = Self::hover_of::<DraggedPath, _>(frame, project_id, key, cx);
        let frame = Self::pane_drops(frame, project_id, key, cx);
        let target = DropTarget::Pane(key.clone());
        frame
            .on_drag_move::<ExternalPaths>(cx.listener({
                let target = target.clone();
                move |shell, event: &DragMoveEvent<ExternalPaths>, _, cx| {
                    shell.hover_os_drop(&target, event, cx);
                }
            }))
            .child(drop_catcher(target, cx))
    }

    /// The drops of every drag type on the pane `key`: one listener per payload type.
    /// The pane frame has them, and so does every element above the pane that blocks the mouse
    /// (the drag strip): a drop only reaches the element under the pointer.
    pub(crate) fn pane_drops<E: gpui::InteractiveElement>(
        el: E,
        project_id: Ulid,
        key: &PaneKey,
        cx: &mut Context<'_, Self>,
    ) -> E {
        let el = Self::drop_of::<DraggedPane, E>(el, project_id, key, cx);
        let el = Self::drop_of::<DraggedItem, E>(el, project_id, key, cx);
        Self::drop_of::<DraggedPath, E>(el, project_id, key, cx)
    }

    fn hover_of<T: PaneDragged, E: gpui::InteractiveElement>(
        el: E,
        project_id: Ulid,
        key: &PaneKey,
        cx: &mut Context<'_, Self>,
    ) -> E {
        let key = key.clone();
        el.on_drag_move(
            cx.listener(move |shell, event: &DragMoveEvent<T>, _, cx| {
                let takes = event.drag(cx).takes(shell, project_id, &key);
                shell.hover_pane_drop(&key, takes, event, cx);
            }),
        )
    }

    fn drop_of<T: PaneDragged, E: gpui::InteractiveElement>(
        el: E,
        project_id: Ulid,
        key: &PaneKey,
        cx: &mut Context<'_, Self>,
    ) -> E {
        let key = key.clone();
        el.on_drop(cx.listener(move |shell, dragged: &T, window, cx| {
            dragged.ended(shell, cx);
            let edge = shell.take_pane_drop(&key);
            cx.notify();
            // No drop while a dialog is open.
            if let Some(edge) = edge.filter(|_| !shell.modal_open()) {
                dragged.land(shell, project_id, key.clone(), edge, window, cx);
            }
        }))
    }

    /// The overlay on the half of the pane `key` a drop would fill: the side of the edge nearest
    /// to the pointer.
    pub(crate) fn pane_drop_overlay(
        &self,
        key: &PaneKey,
        theme: &Theme,
        cx: &Context<'_, Self>,
    ) -> Option<gpui::Div> {
        let drop = (self.pane_drop.as_ref())
            .filter(|drop| drop.target == *key && cx.has_active_drag())?;
        let half = mkrew_ui::drop_zone::edge_half(drop.edge);
        Some(
            div()
                .absolute()
                .left(relative(half.x as f32))
                .top(relative(half.y as f32))
                .w(relative(half.w as f32))
                .h(relative(half.h as f32))
                .rounded(px(radius::MEDIUM))
                .border(px(theme::BORDER_WIDTH))
                .border_color(rgb(theme.accent))
                .bg(theme.tint(theme.accent)),
        )
    }

    /// The workspace and tab a tab-strip drag carries.
    fn dragged_tab(&self, item: &DraggedItem) -> Option<(Ulid, &Tab)> {
        let ReorderList::Tabs(project_id, pinned) = item.list else {
            return None;
        };
        let tab = (self.tabs.get(&project_id)?.tabs().iter())
            .filter(|tab| tab.pinned == pinned)
            .nth(item.index)?;
        Some((project_id, tab))
    }

    /// A drag that `takes` (may drop on `target`'s pane) moved: over the pane it picks the edge,
    /// off the pane it lets go of it. Nothing lights up while a dialog is open.
    fn hover_pane_drop<T: 'static>(
        &mut self,
        target: &PaneKey,
        takes: bool,
        event: &DragMoveEvent<T>,
        cx: &mut Context<'_, Self>,
    ) {
        let (position, bounds) = (event.event.position, event.bounds);
        let next = if takes && !self.modal_open() && bounds.contains(&position) {
            let at = position - bounds.origin;
            // Off the tab area: no reorder line.
            if let Some(reorder) = &mut self.reorder {
                reorder.target = reorder.from;
                reorder.after = false;
            }
            Some(PaneDrop {
                target: target.clone(),
                edge: Edge::nearest(at.x / bounds.size.width, at.y / bounds.size.height),
            })
        } else if (self.pane_drop.as_ref()).is_some_and(|drop| drop.target == *target) {
            None
        } else {
            return;
        };
        if self.pane_drop != next {
            self.pane_drop = next;
            cx.notify();
        }
    }

    /// An OS file drag moved over `target` (a pane or the tab strip): it is the drop target
    /// while the pointer is inside and no dialog covers it (see `Shell::drop_hover`).
    pub(crate) fn hover_os_drop(
        &mut self,
        target: &DropTarget,
        event: &DragMoveEvent<ExternalPaths>,
        cx: &mut Context<'_, Self>,
    ) {
        // A modal dialog covers the pane: its own box takes the drop, and this catcher
        // (painted first) must not take the slot from it.
        if !self.modal_open() && event.bounds.contains(&event.event.position) {
            self.drop_hover = Some((target.clone(), event.drag(cx).clone()));
        } else if (self.drop_hover.as_ref()).is_some_and(|(hovered, _)| hovered == target) {
            self.drop_hover = None;
        }
    }

    /// The edge of `target` the drop showed, taken.
    pub(crate) fn take_pane_drop(&mut self, target: &PaneKey) -> Option<Edge> {
        let drop = self.pane_drop.take()?;
        (drop.target == *target).then_some(drop.edge)
    }

    /// Pane `moved` was dropped on the `edge` of `target`: it moves there, whole leaf, saved at
    /// once. A move never asks about unsaved edits: the file stays open.
    pub(crate) fn drop_pane(
        &mut self,
        project_id: Ulid,
        moved: &PaneKey,
        target: &PaneKey,
        edge: Edge,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        // A drop in a detached window leaves the main window's active tab alone.
        let detached = (self.tabs.get(&project_id))
            .and_then(|tabs| tabs.tab_with(target))
            .map(|tab| tab.id)
            .filter(|tab_id| self.terminal_host(project_id, *tab_id) != HostId::Main);
        let writes = (self.tabs.get_mut(&project_id)).and_then(|tabs| match detached {
            Some(tab_id) => {
                (tabs.with_active(tab_id, |tabs| tabs.move_pane(moved, target, edge))).flatten()
            }
            None => tabs.move_pane(moved, target, edge),
        });
        for write in writes.into_iter().flatten() {
            self.save_layout(project_id, write, window, cx);
        }
        self.sync_panes(window, cx);
        self.focus_pane(window, cx);
    }

    /// A tab was dropped on the `edge` of `target`: its panes move there, then the emptied tab
    /// closes.
    pub(crate) fn drop_tab_on_pane(
        &mut self,
        item: &DraggedItem,
        target: &PaneKey,
        edge: Edge,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let Some((project_id, source)) = self.dragged_tab(item).map(|(p, tab)| (p, tab.id)) else {
            return;
        };
        let Some(connection) = self.connection.clone() else {
            return;
        };
        let Some(writes) = (self.tabs.get_mut(&project_id))
            .and_then(|tabs| tabs.merge_tab(source, target, edge))
        else {
            return;
        };
        self.sync_panes(window, cx);
        self.focus_pane(window, cx);
        // One after another, the close last: `tab.close` removes the sessions of the tab's saved
        // layout (ADR 0041), so it must only run once that layout is empty.
        cx.spawn_in(window, async move |this, cx| {
            for write in writes {
                let saved = connection.call::<()>(Request::TabSetLayout(write.clone()));
                if let Err(error) = saved.await {
                    tracing::warn!(tab_id = %write.tab_id, "tab.set_layout failed: {error:#}");
                    let _ = this.update(cx, |shell, cx| {
                        if let Some(tabs) = shell.tabs.get_mut(&project_id) {
                            tabs.write_failed(&write);
                        }
                        shell.toast_error(format!("Cannot save the layout: {error:#}"), cx);
                    });
                    return;
                }
            }
            let closed =
                (connection.call::<()>(Request::TabClose(TabRef { tab_id: source }))).await;
            let _ = this.update_in(cx, |shell, window, cx| {
                match closed {
                    Ok(()) => {
                        if let Some(tabs) = shell.tabs.get_mut(&project_id) {
                            tabs.remove(source);
                        }
                        shell.sync_panes(window, cx);
                    }
                    Err(error) => {
                        tracing::warn!(tab_id = %source, "tab.close failed: {error:#}");
                        shell.toast_error(format!("Cannot close the tab: {error:#}"), cx);
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// A file (a Files tree row) dropped on the workspace: `at` = a pane and the edge to split
    /// it on, `None` = a new tab after the active one.
    /// A file that is already open is not opened twice (one pane per file, spec 4.2): dropped on
    /// a pane it moves its pane there, dropped on the strip its tab is activated.
    /// Only file and diff leaves are placed by a drop; sessions come from `DraggedPane` or the
    /// `+` row.
    pub(crate) fn drop_leaf(
        &mut self,
        project_id: Ulid,
        at: Option<(PaneKey, Edge)>,
        leaf: Leaf,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        use mkrew_ui::documents::Place;
        let key = leaf.key();
        let open = (self.tabs.get(&project_id)).is_some_and(|tabs| tabs.tab_with(&key).is_some());
        if let Some((target, edge)) = &at {
            if *target == key {
                return;
            }
            if open {
                return self.drop_pane(project_id, &key, target, *edge, window, cx);
            }
        }
        let place = match at {
            Some((target, edge)) => Place::Split(target, edge),
            None => Place::NewTab,
        };
        match leaf {
            Leaf::File { file, .. } => self.open_document(project_id, file, None, place, window, cx),
            Leaf::Diff { .. } => self.open_diff(project_id, place, window, cx),
            Leaf::Session { .. } => {
                tracing::warn!(%project_id, "a drop never places a session leaf");
            }
        }
    }

    /// Pane `key` dropped on the tab strip: it leaves its tab for a new one right after the
    /// active tab. A pane alone in its tab already is a tab of its own: nothing happens.
    pub(crate) fn pane_to_new_tab(
        &mut self,
        project_id: Ulid,
        key: &PaneKey,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let shares_its_tab = (self.tabs.get(&project_id))
            .and_then(|tabs| tabs.tab_with(key))
            .is_some_and(|tab| tab.keys().len() > 1);
        if !shares_its_tab {
            return;
        }
        let key = key.clone();
        let position = self.position_after_active(project_id);
        self.create_tab_then(
            project_id,
            position,
            move |shell, tab_id, window, cx| {
                // `tab.create` took a moment: the pane may have moved or closed since, which
                // leaves the new tab empty and the layouts as they were.
                let writes = (shell.tabs.get_mut(&project_id))
                    .and_then(|tabs| tabs.move_to_empty_tab(&key, tab_id));
                for write in writes.into_iter().flatten() {
                    shell.save_layout(project_id, write, window, cx);
                }
                shell.sync_panes(window, cx);
                shell.focus_pane(window, cx);
            },
            window,
            cx,
        );
    }

    /// Makes the main window's tab strip take a pane (to its own tab), a Files tree file row
    /// (a new tab) and OS files (opened as new tabs), and tint while one hovers.
    /// A tab drag keeps the reorder drop the strip already has.
    pub(crate) fn strip_drops(
        &self,
        strip: gpui::Div,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> gpui::Div {
        let tint = theme.tint(theme.accent);
        strip
            // A folder row opens nothing there: no tint, no drop.
            .can_drop(|dragged, _, _| {
                dragged
                    .downcast_ref::<DraggedPath>()
                    .is_none_or(|path| !path.is_dir)
            })
            .drag_over::<DraggedPane>(move |style, _, _, _| style.bg(tint))
            .drag_over::<DraggedPath>(move |style, _, _, _| style.bg(tint))
            .on_drop(cx.listener(|shell, pane: &DraggedPane, window, cx| {
                if shell.workspace == Some(pane.project_id) && !shell.modal_open() {
                    shell.pane_to_new_tab(pane.project_id, &pane.key, window, cx);
                }
            }))
            .on_drop(cx.listener(|shell, path: &DraggedPath, window, cx| {
                if shell.workspace == Some(path.project_id) && !shell.modal_open() {
                    let leaf = Leaf::file(&path.path);
                    shell.drop_leaf(path.project_id, None, leaf, window, cx);
                }
            }))
            .on_drag_move::<ExternalPaths>(cx.listener(
                |shell, event: &DragMoveEvent<ExternalPaths>, _, cx| {
                    shell.hover_os_drop(&DropTarget::Strip, event, cx);
                },
            ))
            .child(drop_catcher(DropTarget::Strip, cx))
    }
}
```

- [ ] **Step 11: Wire `pane_render.rs`: `render_tab`, `pane_frame`, `pane_edge`**

All edits are in `crates/mkrew-ui/src/shell/pane_render.rs`.
After Tasks 4-5 `pane_frame` builds the frame of every leaf kind from a `key: PaneKey`; the session-only layers (badges, voice ring, color stripe) sit behind `key.session()`.
Before editing, list the current sites:

```bash
git grep -n -E "pane_drop_target\(|pane_drop_overlay\(|pane_edge\(|draggable_pane\(|drop_tree_path|drop_catcher\(|DropTarget::|drag_over::<DraggedPath>" -- crates/mkrew-ui/src
```

Expected before the edits: the calls in `pane_render.rs` (`pane_frame`, `pane_edge`, `drop_catcher`), `pane_drop.rs` (replaced in Step 10), `navigation.rs` (`drop_tree_path`), `task_dialog.rs` (`DropTarget::TaskPrompt`, unchanged).

(a) `render_tab` gets the workspace.
New signature (second parameter) and the call it makes:

```rust
    pub(crate) fn render_tab(
        &self,
        project_id: Ulid,
        tab: Option<&Tab>,
        now: i64,
        // ... the other parameters unchanged
```

```rust
                    .child(self.pane_frame(
                        project_id,
                        key,
                        several,
                        zoomed,
                        // ... the other arguments unchanged
```

(`key` is the loop variable that Task 4 names for the `(key, rect)` pairs of `tab.rects()`; before Task 4 it was `session`.)

(b) `pane_frame` takes `project_id: Ulid` as its first parameter after `&self`, and its frame wiring becomes the following.
Delete these three blocks from `pane_frame`:

1. The `.on_drop(cx.listener(move |shell, dragged: &DraggedPath, ..| { .. shell.drop_tree_path(session, dragged, window, cx); }))` that sits on the `let frame = div()...` builder.
2. The `.on_drag_move::<ExternalPaths>(..)` block and the `.child(drop_catcher(DropTarget::Pane(session), cx))` that follow the `on_mouse_down` handler (both now live in `pane_drop_target`).
3. The drop ring: the `.child(div().absolute().inset_0().rounded(..).border(..).border_color(gpui::transparent_black()).drag_over::<DraggedPath>(..))` block, and the `let accent_border = theme.accent;` it used (the edge half replaces the ring).

Then change the three calls.
The frame builder call (it stays at the same place, right after `let frame = div()....;` and before `.on_mouse_down`, and now applies to every leaf kind, not only to sessions):

```rust
        self.pane_drop_target(frame, project_id, &key, cx)
```

The overlay (same place as before, after the dim layer):

```rust
            .children(self.pane_drop_overlay(&key, theme, cx))
```

The drag strip (last child but the voice ring; it was `.children(info.map(|info| self.pane_edge(info, several, theme, cx)))` and is now drawn for every leaf kind, because a file and a diff pane must also take drops on their top 8 px and a file pane among several must be draggable):

```rust
            .child(self.pane_edge(project_id, &key, several, theme, cx))
```

If Task 4 or 5 left a separate frame builder for file and diff panes, give it the same three calls in the same relative places and delete its copies of blocks 1-3.
Check: `git grep -n -E "pane_drop_target\(|pane_drop_overlay\(|pane_edge\(" -- crates/mkrew-ui/src/shell/pane_render.rs` shows one call of each in the frame code, none inside a `match key { PaneKey::Session(..) => .. }` arm.

(c) Replace `pane_edge` (the whole function and its doc comment) with:

```rust
    /// The top edge of a pane: for a session it records the anchor of the pane's keyboard menu
    /// and, among `several` panes, it is a slim handle (`PANE_GRIP_HEIGHT` tall, a grip shows on
    /// hover) that drags the pane onto another; a file or diff pane has the handle but no
    /// anchor.
    /// The handle blocks the mouse (not the wheel): gpui hit-tests every hitbox until one blocks,
    /// so without it the terminal under it would also get the mouse down and start a selection.
    /// Drops are delivered to the hovered hitbox only, so the strip carries the pane frame's drop
    /// set (`pane_drops`); the frame's drag-move still sets the half that lights up.
    /// A lone pane has nothing to rearrange: its edge is an empty div, only the Shift+F10 menu
    /// anchor of a session, with no hitbox and no mouse handling.
    fn pane_edge(
        &self,
        project_id: Ulid,
        key: &PaneKey,
        several: bool,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> gpui::AnyElement {
        let edge = div()
            .absolute()
            .top_0()
            .left_0()
            .right_0()
            .h(px(theme::PANE_GRIP_HEIGHT))
            .flex()
            .items_center()
            .justify_center()
            .children(
                key.session()
                    .map(|session| record_bounds(Anchor::Pane(session), cx)),
            );
        if !several {
            return edge.into_any_element();
        }
        let edge = Self::pane_drops(edge, project_id, key, cx);
        let group = grip_id(key);
        let handle = self.draggable_pane(edge.group(group.clone()), project_id, key, theme);
        grip_hover(handle, theme)
            .block_mouse_except_scroll()
            .cursor(CursorStyle::OpenHand)
            .child(
                div()
                    .w(px(space::XXL))
                    .h(px(theme::PANE_ACCENT_WIDTH))
                    .rounded_full()
                    .bg(rgb(theme.text_muted))
                    .opacity(opacity::HIDDEN)
                    .group_hover(group, |style| style.opacity(opacity::FULL)),
            )
            .into_any_element()
    }
```

- [ ] **Step 12: `DropTarget` and `drop_catcher` in `pane_render.rs`**

Replace the `DropTarget` enum and the body of `drop_catcher` (the doc comments and the ponytail comment stay):

```rust
/// What an OS file drag hovers (§20B.5, ADR 0095).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum DropTarget {
    /// A pane: a terminal types the paths, a file or diff pane opens the files.
    Pane(PaneKey),
    /// The main window's tab strip: the files open as new tabs.
    Strip,
    /// The Prompt box of the open task dialog.
    TaskPrompt,
}
```

```rust
pub(crate) fn drop_catcher(
    target: DropTarget,
    cx: &Context<'_, Shell>,
) -> impl IntoElement + use<> {
    let view = cx.entity();
    canvas(
        |_, _, _| (),
        move |_, (), window, _| {
            let view = view.clone();
            window.on_mouse_event(move |_: &MouseUpEvent, phase, window, cx| {
                // A plain click has no active drag: stale paths are never typed.
                if phase != DispatchPhase::Capture || !cx.has_active_drag() {
                    return;
                }
                view.update(cx, |shell, cx| {
                    let hovered = shell.drop_hover.take_if(|(hovered, _)| *hovered == target);
                    if let Some((_, paths)) = hovered {
                        cx.stop_propagation();
                        match &target {
                            DropTarget::Pane(PaneKey::Session(session)) => {
                                shell.drop_paths(*session, &paths, window, cx);
                            }
                            // The workspace of the window the pointer was released in.
                            DropTarget::Pane(_) => {
                                if let Some((project_id, _)) = shell.tab_scope(window) {
                                    shell.drop_os_files(project_id, &paths, window, cx);
                                }
                            }
                            DropTarget::Strip => {
                                if let Some(project_id) = shell.workspace {
                                    shell.drop_os_files(project_id, &paths, window, cx);
                                }
                            }
                            DropTarget::TaskPrompt => shell.drop_prompt_paths(&paths, window, cx),
                        }
                    }
                });
            });
        },
    )
    .absolute()
    .size_full()
}
```

`task_dialog.rs` only builds and matches `DropTarget::TaskPrompt` by reference (`matches!(shell.drop_hover, Some((DropTarget::TaskPrompt, _)))`), so it compiles unchanged.

- [ ] **Step 13: The tab strip takes the drops (`tab_area_frame` in `pane_render.rs`)**

In `tab_area_frame`, add one line right after the existing `.on_drop(cx.listener(|shell, item: &DraggedItem, window, cx| { shell.drop_reordered(item, window, cx); }))`:

```rust
            .when(host == HostId::Main, |el| self.strip_drops(el, theme, cx))
```

Chips and the `+` button are children of this frame and take no pane, file or OS drops themselves, so a drop on them falls to the frame: the whole strip is the drop area.
`strip_drops` keeps the `DraggedItem` drop working (its `can_drop` lets every payload but a folder row through).

- [ ] **Step 14: Pass the workspace to `render_tab` in its two callers**

`crates/mkrew-ui/src/shell/chrome.rs`, in `render_main` (the `(View::Terminals, Some(tabs))` arm, `project_id` is the shown workspace there):

```rust
                        self.render_tab(
                            project_id,
                            tabs.active(),
                            now,
```

`crates/mkrew-ui/src/shell/detached_render.rs`, in the `Some((project, tab_id))` arm:

```rust
                self.render_tab(
                    project,
                    tab,
                    now,
```

- [ ] **Step 15: OS files on file and diff panes and the strip (`navigation.rs`)**

In `crates/mkrew-ui/src/shell/navigation.rs` delete `drop_tree_path` (the function and its doc comment: "A file tree row dropped on `session`'s pane ...").
Its logic moves to the composer (Step 16).
Add, next to `drop_paths`:

```rust
    /// Files dropped from the OS on a file or diff pane or on the tab strip (§20B.5): each file
    /// inside the workspace root or the notes folder opens as a new tab; one toast says that
    /// the others do not; folders are skipped.
    /// In a machine window these files are on this computer, not on the machine: refused.
    pub(crate) fn drop_os_files(
        &mut self,
        project_id: Ulid,
        paths: &ExternalPaths,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        if self.modal_open() {
            return;
        }
        if let Some(name) = self.machine_name().map(str::to_owned) {
            self.toast_info(mkrew_ui::machines::local_files_refused(&name), cx);
            return;
        }
        let Some(root) = (self.sidebar.projects().iter())
            .find(|project| project.id == project_id)
            .map(|project| PathBuf::from(&project.path))
        else {
            return;
        };
        let files: Vec<PathBuf> = (paths.paths().iter())
            .filter(|path| !path.is_dir())
            .cloned()
            .collect();
        let notes = self.notes_folder();
        let found = mkrew_ui::drop_zone::classify_os_files(&root, notes.as_deref(), &files);
        if found.refused > 0 {
            self.toast_error(mkrew_ui::drop_zone::REFUSED_OS_FILES, cx);
        }
        for path in found.open {
            self.open_document(
                project_id,
                path,
                None,
                mkrew_ui::documents::Place::NewTab,
                window,
                cx,
            );
        }
    }

    /// The absolute `[notes] folder` the daemon reads and writes files in, outside the
    /// workspace root; `None` while no such folder is configured.
    /// Task 7 (the `[notes] folder` setting) replaces this body with the resolved folder; the
    /// drop check above needs no other change.
    fn notes_folder(&self) -> Option<PathBuf> {
        None
    }
```

Task 7 hand-off: Task 7 resolves the setting with its `mkrew_core::notes` folder resolution (absolute folder, or `None` when the folder is empty or inside the root) in `notes_folder`, and nothing else in this task changes.
`fs.new_note` must answer an absolute path with `/` separators (as `classify_os_files` stores it), so a note opened from `+` and the same note dropped from the OS get one `PaneKey::File`.

- [ ] **Step 16: Tree rows on the composer (`composer.rs`)**

In `crates/mkrew-ui/src/shell/composer.rs`, add to `impl Shell` (for example right before `composer_panel`):

```rust
    /// A Files tree row, file or folder, dropped on `session`'s composer (§20B.5, ADR 0081):
    /// `composer.file_ref` on an agent, the bare relative path on a shell, a folder ending in
    /// `/`, typed into the draft with no Enter.
    /// This was the pane's drop; a pane now opens the file, so only a composer that is shown
    /// takes the row.
    pub(crate) fn drop_tree_on_composer(
        &mut self,
        session: Ulid,
        dragged: &DraggedPath,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let in_workspace = (self.tab_of_session(session))
            .is_some_and(|(project_id, _)| project_id == dragged.project_id);
        if self.modal_open() || !in_workspace || self.composers.shown(session).is_none() {
            return;
        }
        let agent = (self.sidebar.sessions().iter())
            .any(|s| s.id == session && s.kind == SessionKind::Agent);
        // A missing or broken settings file types the default form (the daemon reports it).
        let template = mkrew_core::settings::load(&self.settings_file)
            .unwrap_or_default()
            .composer
            .file_ref;
        let text = composer::dropped_ref(&template, &dragged.path, dragged.is_dir, agent);
        self.focus_session_pane(session);
        self.prompt_typed(composer::Target::Composer(session), &text, window, cx);
        cx.notify();
    }

    /// Makes a composer element (the docked box, the chat card, its collapsed pill) take a
    /// tree row: an accent ring while one hovers it, and the pane's edge half is put out
    /// while the pointer is over the composer, because the drop lands here and not as a split.
    /// The frame's drag-move runs first (capture phase, parent before child), this one clears.
    fn composer_drops<E: gpui::InteractiveElement + gpui::ParentElement>(
        el: E,
        session: Ulid,
        corner: f32,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> E {
        let accent = theme.accent;
        el.on_drop(cx.listener(move |shell, dragged: &DraggedPath, window, cx| {
            shell.drop_tree_on_composer(session, dragged, window, cx);
        }))
        .on_drag_move(cx.listener(
            |shell, event: &DragMoveEvent<DraggedPath>, _, cx| {
                if event.bounds.contains(&event.event.position) && shell.pane_drop.take().is_some()
                {
                    cx.notify();
                }
            },
        ))
        .child(
            div()
                .absolute()
                .inset_0()
                .rounded(px(corner))
                .border(px(theme::BORDER_WIDTH))
                .border_color(gpui::transparent_black())
                .drag_over::<DraggedPath>(move |style, _, _, _| style.border_color(rgb(accent))),
        )
    }
```

Apply it at the three composer elements.

1. The docked box in `composer_panel`: change the start of the final expression `div()\n .flex_none()\n .flex()\n .flex_col()\n .gap(px(space::XS))\n .p(px(space::S))\n .bg(rgb(theme.surface))` to `let panel = div()...` and its end:

```rust
                    .children(tools),
            )
            .into_any_element()
    }
```

becomes

```rust
                    .children(tools),
            );
        Self::composer_drops(panel, session, radius::MEDIUM, theme, cx).into_any_element()
    }
```

2. The collapsed pill in `composer_card`: right after the `let pill = div()....child(icon_svg(Icon::ArrowUp, ..));` statement and before `return super::chat_view::chat_column(pill)`:

```rust
            let pill = Self::composer_drops(pill, session, radius::PILL, theme, cx);
```

3. The card in `composer_card`: right after the `let card = div()....child(send),\n            );` statement and before `super::chat_view::chat_column(div().flex().flex_col().children(meter).child(card))`:

```rust
        let card = Self::composer_drops(card, session, radius::LARGE, theme, cx);
```

OS files on a composer still type their absolute paths: that drop is the pane's catcher and `type_into_pane` (unchanged).
Other payloads (tab, pane) have no composer handler, so over the composer they fall to the frame and split at the lit edge.

- [ ] **Step 17: Delete `drop_axis` and fix the imports**

`crates/mkrew-ui/src/split.rs`: delete `pub fn drop_axis` with its doc comment, and the test `a_drop_leaning_right_splits_right_else_below` (it is the only test that names it; Task 1 kept both for its `pane_drop` mapping).
`Edge::nearest` and its tests (Task 1) stay.

`crates/mkrew-ui/src/main.rs`: the split import loses `drop_axis` and has the three Task 1 types (keep rustfmt's order):

```rust
use mkrew_ui::split::{Axis, Direction, Divider, Edge, Leaf, PaneKey, Preset};
```

Check that nothing names the removed pieces:

```bash
git grep -n -E "drop_axis|drop_tree_path|PaneDrop \{ target, axis|\.axis\b.*pane_drop|Axis::Vertical => half" -- crates
```

Expected: no output (`Axis` itself stays: dividers and splits use it).

- [ ] **Step 18: Build, lint, run the tests**

```bash
cargo fmt --all
cargo clippy -p mkrew-ui --all-targets -j 4 -- -D warnings
cargo test -p mkrew-ui -j 4
```

Expected: clippy clean; every `mkrew-ui` test passes, including `drop_zone`, the three `empty_tab` tests and the `Edge::nearest` tests of Task 1.
If rustc crashes on the test build (`STATUS_STACK_BUFFER_OVERRUN`), run `cargo test -p mkrew-ui --lib -j 4` instead; the shell code has no unit tests of its own.

- [ ] **Step 19: Commit the wiring**

```bash
git add crates/mkrew-ui/src/shell/pane_drop.rs crates/mkrew-ui/src/shell/pane_render.rs crates/mkrew-ui/src/shell/chrome.rs crates/mkrew-ui/src/shell/detached_render.rs crates/mkrew-ui/src/shell/navigation.rs crates/mkrew-ui/src/shell/composer.rs crates/mkrew-ui/src/split.rs crates/mkrew-ui/src/main.rs
git -c user.email=tungnv975@gmail.com commit -m "feat(ui): four-edge drops for files, tabs and panes; pane and file to the tab strip; tree rows on the composer; OS files on file panes"
```

- [ ] **Step 20: Smoke checks Task 9 must run (gpui rendering is not unit-tested)**

Task 9 adds these lines to the "2026-11-23 Tern panes" entry of `docs/HUMAN_CHECKS.md` and shoots the four-edge overlays for the screenshot list:

1. Drag a Files tree file row over a terminal pane, slowly from its left edge to its right, top and bottom: the tinted half follows the nearest edge (left, right, top, bottom), and the drop opens the file in a pane on that side.
2. Drag a row of a file that is already open onto another pane: its pane moves there (no second pane); onto its own pane: nothing happens.
3. Drag a folder row over a pane: no tint, no drop; over the composer of an agent pane: a ring, and the path with a trailing `/` lands in the draft.
4. Drag a file row onto the composer of an agent pane and of a shell pane: `@path ` and the bare path; nothing opens; the pane shows no tinted half while the pointer is over the composer.
5. Drag a tab onto each edge of a pane of another tab: its panes merge on that side and the emptied tab closes; drag a tab onto a pane of its own tab: no tint.
6. Drag a pane by its top grip onto the four edges of another pane (terminal, file, diff): it moves, a dirty file stays dirty and nothing asks to save.
7. Drag a pane (among several) and a file row onto the empty space of the tab strip: the pane gets its own tab after the active one, the file opens as a new tab; the strip tints while they hover; a lone pane has no grip.
8. Drop an OS file from the workspace and one from outside it, plus a folder, on a file pane, a diff pane and the strip: the first opens as a new tab, one toast "Only files inside the workspace or the notes folder open in Mkrew" for the outside one, the folder is skipped; on a terminal pane the absolute paths are typed as before.
9. Open a dialog (Settings) and drag over the panes: nothing lights up and nothing drops.
10. In a detached window: the strip takes tab reorder only; panes there take all four edges.

---

### Task 7: Notes in core and daemon (`[notes] folder`, `fs.new_note`, notes-folder exception)

Spec: sections 2 (last assumption), 4.5, 7.2 and 10 of `docs/superpowers/specs/2026-11-23-tern-panes-design.md`.
Protocol stays 60 (bumped in Task 1); this task adds a request and a meaning inside that number.
All paths are relative to `.worktrees/tern-panes`.

Decisions this task makes (the spec leaves them open):

- Time crate: `chrono` 0.4 is the only one in the workspace (`crates/mkrew-ui/Cargo.toml` line 25, `chrono = { version = "0.4", default-features = false, features = ["clock"] }`; `time` is only a transitive dependency and `jiff` is absent).
  `mkrew-core` takes it with no features (the pure helper only needs `NaiveDateTime`, `Datelike`, `Timelike`); `mkrew-daemon` takes it with `clock` (for `Local::now()`).
  `Cargo.lock` gains two dependency lines, no new package.
- The setting is parsed by one pure function, `mkrew_core::notes::folder`, used by `fs.new_note`, `fs.read` and `fs.write`, so the three can never disagree about what the folder is.
- What a user may type in `[notes] folder` (tested below): surrounding spaces are trimmed; both `/` and `\` work as separators (a trailing one is dropped); `.` parts are dropped; `..` anywhere is refused with a message; a leading `~` is NOT expanded and is refused with a message that says to write the full path; a path with a root but no drive on Windows (`/x`) or a drive without a root (`C:x`) is refused; a relative folder inside `.git` is refused.
- An absolute folder whose drive or parent cannot exist is an error answer (`invalid_request`) and nothing is created.
- Name clash: `fs.new_note` loops over `file_ops::create` (which is `File::create_new`, never overwrites) and moves on at `AlreadyExists`: `note-YYYYMMDD-HHMMSS.md`, then `-2`, `-3`, ... up to 1000 tries.
- Absolute-versus-relative answer is lexical: an absolute folder that starts with the target's root (`Path::strip_prefix`) is answered root-relative, anything else absolute with `/` separators.
  `ponytail:` a root and a folder that name the same place in different spellings (8.3 names, other case on Windows) answer absolute; the absolute path is accepted by `fs.read` / `fs.write` anyway, so nothing breaks.
- The Task 9 docs pass owns SPEC section 7 (the `fs.new_note` row, the `fs.read` / `fs.write` exception); this task writes only the SPEC section 20.0 row, because the settings template and SPEC section 20.0 must stay in step.
  Spec gap found: no test in the repo reads `docs/SPEC.md` for settings lines (`every_template_key_has_exactly_one_setting` in `settings_fields.rs` compares the template with the `SETTINGS` table only), so the SPEC line is not machine-checked; it is still written exactly below.

**Files:**

- Modify: `crates/mkrew-core/Cargo.toml` (dependency `chrono`)
- Modify: `crates/mkrew-core/src/lib.rs` (`pub mod notes;` after `pub mod log_tail;`... alphabetical: between `login_path`/`lookup` and `paths`: after `pub mod lookup;`)
- Create: `crates/mkrew-core/src/notes.rs`
- Modify: `crates/mkrew-core/src/settings.rs` (struct near line 33, `Settings` field, `template()` near line 637, test literal near line 962, new test)
- Modify: `crates/mkrew-core/src/settings_fields.rs` (row after `ui.board_all_workspaces` near line 175, new test)
- Modify: `crates/mkrew-protocol/src/files.rs` (`FsNewNoteParams`, `FsCreated` doc, near lines 521-541)
- Modify: `crates/mkrew-protocol/src/message.rs` (import near line 20, variant after `FsCreate` near line 271)
- Modify: `crates/mkrew-protocol/src/message/tests.rs` (entry after `Request::FsCreate` near line 286, one wire test)
- Modify: `crates/mkrew-daemon/Cargo.toml` (dependency `chrono` with `clock`)
- Modify: `crates/mkrew-daemon/src/file_ops.rs` (`new_note`, tests)
- Modify: `crates/mkrew-daemon/src/docs.rs` (`notes_file` after `relative_path` near line 57, test in `mod tests` near line 223)
- Modify: `crates/mkrew-daemon/src/server/fs.rs` (imports, `file_target`, `fs_read`, new `fs_new_note`)
- Modify: `crates/mkrew-daemon/src/server.rs` (dispatch arm near line 474)
- Modify: `crates/mkrew-daemon/tests/daemon/main.rs` (import `FsNewNoteParams`, `mod notes;`, `Env::write_settings`, `Env::set_notes_folder`)
- Create: `crates/mkrew-daemon/tests/daemon/notes.rs`
- Modify: `docs/SPEC.md` (section 20.0 block, after the `[zed]` lines near line 2231)
- Modify: `Cargo.lock` (written by cargo)

**Interfaces:**

Consumes: `mkrew_core::settings::Settings` (the `SettingsHolder::current()` clone), `file_ops::create` (creates parents, `File::create_new`, returns the path relative to its root, `AlreadyExists` error kind), `docs::relative_path`, `Daemon::watched_target`, `FsCreated`.

Produces:

```rust
// crates/mkrew-core/src/settings.rs
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct NotesSettings { pub folder: String }
// Settings.notes: NotesSettings

// crates/mkrew-core/src/notes.rs
pub enum NotesFolder { Relative(String), Absolute(PathBuf) } // Relative("") = the root; Relative has `/` separators, no `.` or `..`
pub fn folder(setting: &str) -> Result<NotesFolder, String>;   // the Err text is for the user
pub fn file_name(at: NaiveDateTime, attempt: u32) -> String;   // attempt 1: note-20261123-090507.md, attempt n >= 2: note-20261123-090507-n.md

// crates/mkrew-protocol/src/files.rs
pub struct FsNewNoteParams { pub project_id: Ulid, pub worktree_task_id: Option<Ulid> }
// Request::FsNewNote(FsNewNoteParams), wire "fs.new_note", result FsCreated { path }

// crates/mkrew-daemon/src/file_ops.rs
pub(crate) fn new_note(root: &Path, folder: &NotesFolder, at: NaiveDateTime) -> Result<String, (ErrorKind, String)>;
// crates/mkrew-daemon/src/docs.rs
pub fn notes_file(file_path: &str, folder: &Path) -> Option<(String, PathBuf)>;
// crates/mkrew-daemon/src/server/fs.rs
pub(super) async fn fs_new_note(&self, params: FsNewNoteParams) -> Outcome;
```

Task 8 consumes `Request::FsNewNote` and `FsCreated.path` (a root-relative path, or an absolute `/`-separated path for a note outside the root; both are valid `fs.read` / `fs.write` paths and valid `Leaf::File` strings).

---

#### Part A: `mkrew-core` (settings row, template, pure helpers)

- [ ] **Step 1: Write the failing settings tests**

In `crates/mkrew-core/src/settings.rs`, inside `mod tests`, after `missing_file_is_defaults`, add:

```rust
    #[test]
    fn notes_folder_defaults_to_empty_parses_and_is_in_the_template() {
        assert_eq!(Settings::default().notes.folder, "");
        let settings = parse(
            Path::new("settings.toml"),
            "[notes]\nfolder = 'C:\\notes'\n",
        )
        .unwrap();
        assert_eq!(settings.notes.folder, "C:\\notes");
        let message = parse_err("[notes]\npath = \"x\"\n");
        assert!(message.contains("unknown field `path`"), "{message}");
        assert!(template().contains("[notes]\n# folder = \"\"\n"));
    }
```

In the same module, the full-literal test that ends with `jira: JiraSettings::default(),` (near line 962) must name the new field once it exists; change

```rust
                jira: JiraSettings::default(),
            }
        );
```

to

```rust
                jira: JiraSettings::default(),
                notes: NotesSettings::default(),
            }
        );
```

In `crates/mkrew-core/src/settings_fields.rs`, inside `mod tests`, after `chat_default_is_a_general_switch_off_by_default`, add:

```rust
    #[test]
    fn the_notes_folder_is_a_general_text_field_empty_by_default() {
        let s = setting("notes.folder");
        assert_eq!((s.page, s.kind), (Page::General, Kind::Text));
        assert_eq!(s.current(&Settings::default()), "");
    }
```

- [ ] **Step 2: Run and see it fail**

Run: `cargo test -p mkrew-core notes -j 4`
Expected: FAIL to compile: `no field `notes` on type `Settings``, `cannot find struct, variant or union type `NotesSettings``, and the `unwrap()` in `the_notes_folder_is_a_general_text_field_empty_by_default` would panic once it compiles.

- [ ] **Step 3: Add the struct, the field, the template block and the settings row**

`crates/mkrew-core/src/settings.rs`: in `Settings`, after `pub jira: JiraSettings,` add

```rust
    /// `[notes]`: where New note saves its file (Tern panes spec 2026-11-23 section 7.2).
    pub notes: NotesSettings,
```

After the `JiraSettings` section (before `/// \`[jira] layout\``), add the struct:

```rust
/// `[notes]` (Tern panes spec 2026-11-23 section 7.2): the folder New note saves its file in.
/// Empty = the workspace root; relative = below the root; absolute = used as is, for every
/// workspace. [`crate::notes::folder`] turns the text into a place; the daemon applies it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct NotesSettings {
    pub folder: String,
}
```

In `template()`, after the `[zed]` block (the lines `[zed]\n\`, `# follow_debounce_ms = 1500\n\`, `\n\`), add:

```rust
     # New note: the folder a new note file is saved in. Empty: the workspace root; a relative\n\
     # path is below the root; a full path (not ~) is used as is, for every workspace.\n\
     [notes]\n\
     # folder = \"\"\n\
     \n\
```

`crates/mkrew-core/src/settings_fields.rs`: after the `ui.board_all_workspaces` row (the last row of the General "Workspace" group, so the group stays contiguous), add:

```rust
    row(
        General,
        "Workspace",
        "notes",
        "folder",
        Text,
        "Notes folder",
        "Where New note saves its file: empty is the workspace root, a relative path is below it, a full path (not ~) is used as is.",
    ),
```

- [ ] **Step 4: Run and see it pass**

Run: `cargo test -p mkrew-core settings -j 4`
Expected: PASS, including `every_template_key_has_exactly_one_setting`, `every_setting_has_a_group_and_a_groups_rows_are_contiguous`, `writing_each_default_reads_it_back_and_keeps_comments` and the two new tests.

- [ ] **Step 5: Write the failing `notes` tests**

Create `crates/mkrew-core/src/notes.rs` with the tests only plus the `use` lines (the functions come in Step 7):

```rust
//! New note helpers (Tern panes spec 2026-11-23 section 7.2), pure: the file name of a note and
//! what `[notes] folder` means. The daemon creates the file (`fs.new_note`).

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;
    use std::path::PathBuf;

    /// A full path on this OS, `/`-separated.
    const FULL: &str = if cfg!(windows) { "C:/notes" } else { "/notes" };

    fn relative(text: &str) -> NotesFolder {
        NotesFolder::Relative(text.into())
    }

    fn full() -> NotesFolder {
        NotesFolder::Absolute(PathBuf::from(FULL))
    }

    #[test]
    fn a_note_is_named_by_local_time_and_numbered_from_the_second_try() {
        let at = NaiveDate::from_ymd_opt(2026, 11, 23)
            .unwrap()
            .and_hms_opt(9, 5, 7)
            .unwrap();
        assert_eq!(file_name(at, 1), "note-20261123-090507.md");
        assert_eq!(file_name(at, 2), "note-20261123-090507-2.md");
        assert_eq!(file_name(at, 13), "note-20261123-090507-13.md");
    }

    #[test]
    fn empty_and_dot_mean_the_root() {
        for text in ["", "   ", ".", "./", " ./. "] {
            assert_eq!(folder(text), Ok(relative("")), "{text:?}");
        }
    }

    #[test]
    fn a_relative_folder_is_trimmed_and_uses_slashes_without_a_trailing_one() {
        for (text, want) in [
            ("notes", "notes"),
            ("notes/", "notes"),
            ("  inbox  ", "inbox"),
            ("a\\b", "a/b"),
            ("a/b\\", "a/b"),
            ("./a/./b/", "a/b"),
        ] {
            assert_eq!(folder(text), Ok(relative(want)), "{text:?}");
        }
    }

    #[test]
    fn dot_dot_is_refused_with_a_reason() {
        for text in ["..", "../x", "a/../b", "a/b/..", "..\\x", &format!("{FULL}/../x")] {
            let message = folder(text).unwrap_err();
            assert!(message.contains(".."), "{text:?}: {message}");
        }
    }

    #[test]
    fn a_tilde_is_not_expanded_and_the_message_asks_for_the_full_path() {
        for text in ["~", "~/notes", "~\\notes", " ~/notes "] {
            let message = folder(text).unwrap_err();
            assert!(message.contains('~') && message.contains("full path"), "{text:?}: {message}");
        }
    }

    #[test]
    fn a_full_path_is_trimmed_and_keeps_no_trailing_separator() {
        for text in [
            FULL.to_owned(),
            format!("{FULL}/"),
            format!("  {FULL}/  "),
            format!("{FULL}\\"),
            format!("{FULL}/./"),
        ] {
            assert_eq!(folder(&text), Ok(full()), "{text:?}");
        }
        assert_eq!(
            folder(&format!("{FULL}/a/b/")),
            Ok(NotesFolder::Absolute(PathBuf::from(format!("{FULL}/a/b"))))
        );
    }

    #[cfg(windows)]
    #[test]
    fn windows_full_paths_take_either_slash_and_half_paths_are_refused() {
        assert_eq!(
            folder("C:\\notes\\"),
            Ok(NotesFolder::Absolute(PathBuf::from("C:\\notes")))
        );
        assert_eq!(
            folder("c:/Users/me/notes/"),
            Ok(NotesFolder::Absolute(PathBuf::from("c:/Users/me/notes")))
        );
        // A root without a drive and a drive without a root name no fixed place.
        for text in ["/notes", "\\notes", "C:notes"] {
            let message = folder(text).unwrap_err();
            assert!(message.contains("full path"), "{text:?}: {message}");
        }
    }
}
```

In `crates/mkrew-core/src/lib.rs` add `pub mod notes;` after `pub mod lookup;`.

- [ ] **Step 6: Run and see it fail**

Run: `cargo test -p mkrew-core notes -j 4`
Expected: FAIL to compile: `unresolved import chrono`, `cannot find function `folder``, `cannot find type `NotesFolder``, `cannot find function `file_name``.

- [ ] **Step 7: Add the dependency and the implementation**

`crates/mkrew-core/Cargo.toml`, in `[dependencies]` after `directories = "6"`... alphabetical, before it:

```toml
# `notes::file_name` takes a local `NaiveDateTime`; the daemon reads the clock (it enables `clock`).
chrono = { version = "0.4", default-features = false }
```

Put the implementation above the `#[cfg(test)]` block of `crates/mkrew-core/src/notes.rs`:

```rust
use std::path::{Component, PathBuf};

use chrono::{Datelike, NaiveDateTime, Timelike};

/// What `[notes] folder` names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NotesFolder {
    /// Below the workspace root: `/`-separated, no `.` or `..` parts; `""` is the root itself.
    Relative(String),
    /// A full path, used as is for every workspace: no `..`, no `.`, no trailing separator.
    Absolute(PathBuf),
}

/// `[notes] folder` as the user typed it. Spaces around it are trimmed; `/` and `\` both
/// separate; `.` parts and a trailing separator are dropped. `..` and a leading `~` (nothing
/// expands it) are refused, and so is a path that is neither relative nor full (`/x` or `C:x`
/// on Windows). The `Err` text is for the user.
pub fn folder(setting: &str) -> Result<NotesFolder, String> {
    let setting = setting.trim();
    if setting.starts_with('~') {
        return Err(format!(
            "[notes] folder {setting:?}: ~ is not expanded; write the full path instead"
        ));
    }
    let path = PathBuf::from(setting.replace('\\', "/"));
    let absolute = path.is_absolute();
    let half = !absolute
        && (path.has_root() || path.components().any(|part| matches!(part, Component::Prefix(_))));
    if half {
        return Err(format!(
            "[notes] folder {setting:?}: use a full path with its drive, or a path below the workspace root"
        ));
    }
    let mut clean = PathBuf::new();
    for part in path.components() {
        match part {
            Component::CurDir => {}
            Component::ParentDir => {
                return Err(format!(
                    "[notes] folder {setting:?} must not contain \"..\"; write the full path instead"
                ));
            }
            part => clean.push(part),
        }
    }
    Ok(if absolute {
        NotesFolder::Absolute(clean)
    } else {
        NotesFolder::Relative(clean.to_string_lossy().replace('\\', "/"))
    })
}

/// The file name of a note made at local time `at`: `note-YYYYMMDD-HHMMSS.md`; try 2, 3, ...
/// (the name was taken) add `-2`, `-3`, ... before `.md`.
pub fn file_name(at: NaiveDateTime, attempt: u32) -> String {
    let stamp = format!(
        "{:04}{:02}{:02}-{:02}{:02}{:02}",
        at.year(),
        at.month(),
        at.day(),
        at.hour(),
        at.minute(),
        at.second()
    );
    if attempt <= 1 {
        format!("note-{stamp}.md")
    } else {
        format!("note-{stamp}-{attempt}.md")
    }
}
```

- [ ] **Step 8: Run and see it pass**

Run: `cargo test -p mkrew-core notes -j 4`
Expected: PASS (7 tests on Windows, 6 elsewhere).

- [ ] **Step 9: Format, lint, run the crate's tests, commit**

```
cargo fmt --all
cargo clippy -p mkrew-core --all-targets -j 4 -- -D warnings
cargo test -p mkrew-core -j 4
git add Cargo.lock crates/mkrew-core/Cargo.toml crates/mkrew-core/src/lib.rs crates/mkrew-core/src/notes.rs crates/mkrew-core/src/settings.rs crates/mkrew-core/src/settings_fields.rs
git -c user.email=tungnv975@gmail.com commit -m "feat(core): [notes] folder setting and the pure note name and folder helpers"
```

Expected: clippy clean, all core tests PASS.

---

#### Part B: protocol and daemon

- [ ] **Step 10: Protocol types, variant and tests**

`crates/mkrew-protocol/src/files.rs`: change the doc of `FsCreated.path`:

```rust
    /// The new entry, `/`-separated, relative to the root; `fs.new_note` answers an absolute
    /// `/`-separated path for a note outside the root (inside the notes folder).
    pub path: String,
```

and after the `FsCreated` struct add:

```rust
/// `fs.new_note` (Tern panes spec 2026-11-23 section 7.2): a new empty note file,
/// `note-YYYYMMDD-HHMMSS.md` in the daemon's local time (`-2`, `-3`, ... when taken), in the
/// notes folder of the daemon's `settings.toml` (`[notes] folder`; empty = the root of the
/// project or task worktree). Returns [`FsCreated`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FsNewNoteParams {
    pub project_id: Ulid,
    #[serde(default)]
    pub worktree_task_id: Option<Ulid>,
}
```

`crates/mkrew-protocol/src/message.rs`: add `FsNewNoteParams` to the `use crate::files::{...}` list (after `FsListParams`), and after the `FsCreate` variant:

```rust
    /// Returns [`FsCreated`]: a new empty note in the notes folder (Tern panes spec 2026-11-23
    /// section 7.2).
    #[serde(rename = "fs.new_note")]
    FsNewNote(FsNewNoteParams),
```

`crates/mkrew-protocol/src/message/tests.rs`: in the list of `every_m1_request_round_trips`, after the `Request::FsCreate(..)` entry add

```rust
        Request::FsNewNote(FsNewNoteParams {
            project_id: sid(),
            worktree_task_id: Some(sid()),
        }),
```

and after `every_m1_request_round_trips` (add it as its own test at the end of the file) :

```rust
#[test]
fn fs_new_note_is_on_the_wire_as_fs_new_note() {
    let request = Request::FsNewNote(FsNewNoteParams {
        project_id: sid(),
        worktree_task_id: None,
    });
    assert_eq!(serde_json::to_value(&request).unwrap()["method"], "fs.new_note");
}
```

Run: `cargo test -p mkrew-protocol -j 4`
Expected: PASS. Do not commit yet: `mkrew-daemon` matches `Request` exhaustively and does not compile until Step 14.

- [ ] **Step 11: Daemon dependency and the failing unit tests**

`crates/mkrew-daemon/Cargo.toml`, `[dependencies]`, after `anyhow = "1"`:

```toml
# `fs.new_note` names a note by the local time (`Local::now()`); same version and `clock` feature
# as mkrew-ui, so the tree gains no package.
chrono = { version = "0.4", default-features = false, features = ["clock"] }
```

`crates/mkrew-daemon/src/docs.rs`, inside `mod tests`, after `markdown_and_relative_paths`:

```rust
    #[test]
    fn notes_file_accepts_absolute_paths_inside_the_folder_only() {
        let (root, other) = if cfg!(windows) {
            ("C:/notes", "C:/other")
        } else {
            ("/notes", "/other")
        };
        let folder = Path::new(root);
        let inside = |path: &str| notes_file(path, folder).map(|(shown, _)| shown);
        assert_eq!(inside(&format!("{root}/a.md")), Some(format!("{root}/a.md")));
        assert_eq!(
            inside(&format!("{root}\\sub\\a.md")),
            Some(format!("{root}/sub/a.md"))
        );
        assert_eq!(inside(&format!("  {root}/a.md ")), Some(format!("{root}/a.md")));
        let (_, joined) = notes_file(&format!("{root}/sub/a.md"), folder).unwrap();
        assert_eq!(joined, Path::new(root).join("sub").join("a.md"));
        for bad in [
            String::new(),
            "a.md".into(),
            "notes/a.md".into(),
            root.into(),
            format!("{root}/"),
            format!("{root}/../a.md"),
            format!("{root}/sub/../../a.md"),
            format!("{other}/../notes/a.md"),
            format!("{root}x/a.md"),
            format!("{other}/a.md"),
        ] {
            assert_eq!(inside(&bad), None, "{bad:?}");
        }
        #[cfg(windows)]
        {
            assert!(inside("C:\\notes\\a.md").is_some());
            assert_eq!(inside("D:/notes/a.md"), None);
        }
    }
```

`crates/mkrew-daemon/src/file_ops.rs`, inside `mod tests`, after `create_makes_nested_entries_and_refuses_escapes_and_existing`:

```rust
    use chrono::NaiveDate;

    fn at() -> chrono::NaiveDateTime {
        NaiveDate::from_ymd_opt(2026, 11, 23)
            .unwrap()
            .and_hms_opt(9, 5, 7)
            .unwrap()
    }

    fn slash(path: &Path) -> String {
        path.to_string_lossy().replace('\\', "/")
    }

    #[test]
    fn new_note_numbers_a_taken_name_and_never_overwrites() {
        let root = tempfile::tempdir().unwrap();
        let root = root.path();
        let folder = NotesFolder::Relative(String::new());
        let made: Vec<String> = (0..3)
            .map(|_| new_note(root, &folder, at()).unwrap())
            .collect();
        assert_eq!(
            made,
            [
                "note-20261123-090507.md",
                "note-20261123-090507-2.md",
                "note-20261123-090507-3.md"
            ]
        );
        for name in &made {
            assert_eq!(fs::read(root.join(name)).unwrap(), b"");
        }
        fs::write(root.join("note-20261123-090507.md"), "mine").unwrap();
        assert_eq!(
            new_note(root, &folder, at()).unwrap(),
            "note-20261123-090507-4.md"
        );
        assert_eq!(
            fs::read_to_string(root.join("note-20261123-090507.md")).unwrap(),
            "mine"
        );
    }

    #[test]
    fn new_note_makes_a_relative_folder_and_answers_relative() {
        let root = tempfile::tempdir().unwrap();
        let folder = NotesFolder::Relative("notes/inbox".into());
        let made = new_note(root.path(), &folder, at()).unwrap();
        assert_eq!(made, "notes/inbox/note-20261123-090507.md");
        assert!(root.path().join(&made).is_file());
    }

    #[test]
    fn new_note_answers_relative_inside_the_root_and_absolute_outside_it() {
        let root = tempfile::tempdir().unwrap();
        let elsewhere = tempfile::tempdir().unwrap();

        let inside = root.path().join("docs").join("notes");
        let made = new_note(root.path(), &NotesFolder::Absolute(inside.clone()), at()).unwrap();
        assert_eq!(made, "docs/notes/note-20261123-090507.md");
        assert!(inside.join("note-20261123-090507.md").is_file());

        let outside = elsewhere.path().join("a").join("b");
        let made = new_note(root.path(), &NotesFolder::Absolute(outside.clone()), at()).unwrap();
        assert_eq!(made, format!("{}/note-20261123-090507.md", slash(&outside)));
        assert!(!made.contains('\\'), "an absolute answer uses `/` only: {made}");
        assert!(outside.join("note-20261123-090507.md").is_file());

        let again = new_note(root.path(), &NotesFolder::Absolute(outside.clone()), at()).unwrap();
        assert_eq!(again, format!("{}/note-20261123-090507-2.md", slash(&outside)));
    }

    #[test]
    fn new_note_in_a_folder_that_cannot_exist_is_an_error_that_names_the_folder() {
        let root = tempfile::tempdir().unwrap();
        let blocker = root.path().join("blocker");
        fs::write(&blocker, "a file, not a folder").unwrap();
        let folder = NotesFolder::Absolute(blocker.join("notes"));
        let (kind, message) = new_note(root.path(), &folder, at()).unwrap_err();
        assert_ne!(kind, ErrorKind::AlreadyExists);
        assert!(message.contains("notes folder") && message.contains("blocker"), "{message}");
        assert!(blocker.is_file());
    }
```

Add `use mkrew_core::notes::NotesFolder;` to the `mod tests` imports (it is not in `super` yet in this step, so the tests fail to compile until Step 13).

- [ ] **Step 12: Write the failing end-to-end tests**

`crates/mkrew-daemon/tests/daemon/main.rs`:

1. In the `use mkrew_protocol::{...}` list add `FsNewNoteParams` after `FsMatch`.
2. Add `mod notes;` after `mod limits;`.
3. After `enable_turns` add:

```rust
    /// `settings.toml` with `text`; the daemon reads it when it starts.
    fn write_settings(&self, text: &str) {
        let dir = self.dir.path().join("config");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("settings.toml"), text).unwrap();
    }

    /// `[notes] folder = <folder>` as a TOML literal string, so Windows backslashes and
    /// spaces stay as given.
    fn set_notes_folder(&self, folder: &str) {
        self.write_settings(&format!("[notes]\nfolder = '{folder}'\n"));
    }
```

Create `crates/mkrew-daemon/tests/daemon/notes.rs`:

```rust
//! `fs.new_note` and the notes-folder exception of `fs.read` / `fs.write` (Tern panes spec
//! 2026-11-23 section 7.2).

use std::collections::BTreeMap;

use super::*;

fn new_note(project_id: Ulid) -> Request {
    Request::FsNewNote(FsNewNoteParams {
        project_id,
        worktree_task_id: None,
    })
}

fn read(project_id: Ulid, path: &str) -> Request {
    Request::FsRead(FsReadParams {
        project_id,
        worktree_task_id: None,
        path: path.into(),
        index_text: false,
    })
}

fn write(project_id: Ulid, path: &str, content: &str, expected_hash: &str) -> Request {
    Request::FsWrite(FsWriteParams {
        project_id,
        worktree_task_id: None,
        path: path.into(),
        content: content.into(),
        expected_hash: expected_hash.into(),
    })
}

/// `/`-separated, like the paths the daemon answers.
fn slash(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn file_name(path: &str) -> &str {
    path.rsplit('/').next().unwrap()
}

/// `note-YYYYMMDD-HHMMSS.md`, or with `-N` (N >= 2) before `.md`.
fn is_note_name(name: &str) -> bool {
    let Some(stem) = name
        .strip_prefix("note-")
        .and_then(|rest| rest.strip_suffix(".md"))
    else {
        return false;
    };
    let mut parts = stem.splitn(3, '-');
    let digits = |part: Option<&str>, len: usize| {
        part.is_some_and(|p| p.len() == len && p.bytes().all(|b| b.is_ascii_digit()))
    };
    let (date, time, number) = (parts.next(), parts.next(), parts.next());
    digits(date, 8)
        && digits(time, 6)
        && number.is_none_or(|n| n.parse::<u32>().is_ok_and(|n| n >= 2))
}

fn daemon_error(outcome: Outcome) -> mkrew_protocol::ErrorBody {
    match outcome.into_result::<serde_json::Value>() {
        Err(ResponseError::Daemon(body)) => body,
        other => panic!("expected a daemon error, got {other:?}"),
    }
}

#[test]
fn a_new_note_in_the_root_is_empty_named_by_time_and_answered_relative() {
    let env = Env::new();
    let repo = env.repo();
    std::fs::write(repo.join("a.txt"), "a\n").unwrap();
    let (_daemon, mut client) = env.start();
    let id = client.add_project(&repo).id;

    let made: FsCreated = client.call(new_note(id)).into_result().unwrap();
    assert!(is_note_name(&made.path), "{}", made.path);
    assert!(!made.path.contains('/'), "a note in the root has no folder: {}", made.path);
    assert_eq!(std::fs::read(repo.join(&made.path)).unwrap(), b"");
    let opened: FsContent = client.call(read(id, &made.path)).into_result().unwrap();
    assert_eq!(opened.content.as_deref(), Some(""));

    // An absolute path stays refused when no absolute notes folder is set, as before.
    let absolute = slash(&repo.join("a.txt"));
    assert_eq!(
        error_code(client.call(read(id, &absolute))),
        ErrorCode::InvalidRequest
    );
    client.stop();
}

#[test]
fn notes_made_in_the_same_second_get_numbers() {
    let env = Env::new();
    let repo = env.repo();
    let (_daemon, mut client) = env.start();
    let id = client.add_project(&repo).id;

    let mut paths = Vec::new();
    for _ in 0..5 {
        let made: FsCreated = client.call(new_note(id)).into_result().unwrap();
        paths.push(made.path);
    }
    // Group by the stamp: inside one second the names run base, -2, -3, ... in call order.
    let mut seconds: BTreeMap<String, Vec<u32>> = BTreeMap::new();
    for path in &paths {
        assert!(is_note_name(path), "{path}");
        let parts: Vec<&str> = path.strip_suffix(".md").unwrap().split('-').collect();
        let number = parts.get(3).map_or(1, |n| n.parse().unwrap());
        seconds.entry(parts[..3].join("-")).or_default().push(number);
        assert!(repo.join(path).is_file(), "{path}");
    }
    for (stamp, numbers) in &seconds {
        let want: Vec<u32> = (1..=u32::try_from(numbers.len()).unwrap()).collect();
        assert_eq!(numbers, &want, "{stamp}");
    }
    assert!(
        seconds.values().any(|numbers| numbers.len() >= 2),
        "five calls cannot all fall in different seconds: {paths:?}"
    );
    client.stop();
}

#[test]
fn a_relative_folder_is_trimmed_made_and_answered_relative() {
    let env = Env::new();
    let repo = env.repo();
    // Spaces around it, forward slashes and a trailing one: what a user types.
    env.set_notes_folder(" notes/inbox/ ");
    let (_daemon, mut client) = env.start();
    let id = client.add_project(&repo).id;

    let made: FsCreated = client.call(new_note(id)).into_result().unwrap();
    assert!(made.path.starts_with("notes/inbox/"), "{}", made.path);
    assert!(is_note_name(file_name(&made.path)), "{}", made.path);
    assert!(repo.join("notes").join("inbox").join(file_name(&made.path)).is_file());
    let opened: FsContent = client.call(read(id, &made.path)).into_result().unwrap();
    assert_eq!(opened.content.as_deref(), Some(""));
    client.stop();
}

#[test]
fn an_absolute_folder_inside_the_root_is_answered_relative() {
    let env = Env::new();
    let repo = env.repo();
    let notes = repo.join("notes");
    env.set_notes_folder(&slash(&notes));
    let (_daemon, mut client) = env.start();
    let id = client.add_project(&repo).id;

    let made: FsCreated = client.call(new_note(id)).into_result().unwrap();
    assert!(made.path.starts_with("notes/"), "{}", made.path);
    assert!(notes.join(file_name(&made.path)).is_file());
    assert!(!made.path.contains('\\'), "{}", made.path);
    // The absolute spelling of the same file opens too.
    let absolute = format!("{}/{}", slash(&notes), file_name(&made.path));
    let opened: FsContent = client.call(read(id, &absolute)).into_result().unwrap();
    assert_eq!(opened.content.as_deref(), Some(""));
    client.stop();
}

#[test]
fn an_absolute_folder_outside_the_root_is_answered_absolute_and_opens_and_saves() {
    let env = Env::new();
    let repo = env.repo();
    std::fs::write(repo.join("a.txt"), "a\n").unwrap();
    let notes = env.dir.path().join("notes").join("deep");
    let outside = env.dir.path().join("other.md");
    let secret = env.dir.path().join("notes").join("secret.md");
    std::fs::create_dir_all(secret.parent().unwrap()).unwrap();
    std::fs::write(&outside, "outside\n").unwrap();
    std::fs::write(&secret, "secret\n").unwrap();
    // Forward slashes and a trailing separator, as a user may type it; the folder is missing.
    env.set_notes_folder(&format!("{}/", slash(&notes)));
    let (_daemon, mut client) = env.start();
    let id = client.add_project(&repo).id;

    let made: FsCreated = client.call(new_note(id)).into_result().unwrap();
    assert_eq!(
        made.path,
        format!("{}/{}", slash(&notes), file_name(&made.path))
    );
    assert!(is_note_name(file_name(&made.path)), "{}", made.path);
    assert!(notes.join(file_name(&made.path)).is_file());

    // The absolute path in the folder opens and saves.
    let opened: FsContent = client.call(read(id, &made.path)).into_result().unwrap();
    assert_eq!(opened.content.as_deref(), Some(""));
    assert_eq!(opened.path, made.path);
    assert!(!made.path.contains('\\') && !opened.path.contains('\\'), "{} {}", made.path, opened.path);
    let hash = opened.hash.expect("text has a hash");
    let saved: FsWritten = client
        .call(write(id, &made.path, "# a note\n", &hash))
        .into_result()
        .unwrap();
    assert!(saved.saved);
    assert_eq!(
        std::fs::read_to_string(notes.join(file_name(&made.path))).unwrap(),
        "# a note\n"
    );
    // The backslash spelling is the same place.
    let backslashed = made.path.replace('/', "\\");
    let again: FsContent = client.call(read(id, &backslashed)).into_result().unwrap();
    assert_eq!(again.content.as_deref(), Some("# a note\n"));
    assert!(!again.path.contains('\\'), "{}", again.path);
    assert_eq!(again.path, made.path);

    // Everything else is refused as today.
    let dotdot = format!("{}/../secret.md", slash(&notes));
    for path in [
        slash(&outside),
        slash(&secret),
        dotdot,
        slash(&notes),
        format!("{}/", slash(&notes)),
        slash(&repo.join("a.txt")),
        "../x".into(),
        "..\\other.md".into(),
        format!("{}/nope.md", slash(&notes)).replace("deep/nope.md", "deep/../nope.md"),
    ] {
        assert_eq!(
            error_code(client.call(read(id, &path))),
            ErrorCode::InvalidRequest,
            "read {path}"
        );
        assert_eq!(
            error_code(client.call(write(id, &path, "x", &hash))),
            ErrorCode::InvalidRequest,
            "write {path}"
        );
    }
    assert_eq!(std::fs::read_to_string(&outside).unwrap(), "outside\n");
    assert_eq!(std::fs::read_to_string(&secret).unwrap(), "secret\n");
    // A file of the root still opens by its relative path.
    let root_file: FsContent = client.call(read(id, "a.txt")).into_result().unwrap();
    assert_eq!(root_file.content.as_deref(), Some("a\n"));
    client.stop();
}

#[test]
fn unusable_folders_are_refused_with_a_reason_and_nothing_is_made() {
    for (setting, reason) in [
        ("../outside", ".."),
        ("a/../../outside", ".."),
        ("~", "full path"),
        ("~/notes", "full path"),
        (".git/notes", ".git"),
    ] {
        let env = Env::new();
        let repo = env.repo();
        env.set_notes_folder(setting);
        let (_daemon, mut client) = env.start();
        let id = client.add_project(&repo).id;

        let body = daemon_error(client.call(new_note(id)));
        assert_eq!(body.code, ErrorCode::InvalidRequest, "{setting}");
        assert!(body.message.contains(reason), "{setting}: {}", body.message);
        assert!(!env.dir.path().join("outside").exists(), "{setting}");
        assert!(
            std::fs::read_dir(&repo).unwrap().next().is_none(),
            "{setting}: the root stays empty"
        );
        client.stop();
    }
}

#[test]
fn a_folder_that_cannot_be_made_is_an_error_answer_and_nothing_is_made() {
    let env = Env::new();
    let repo = env.repo();
    // A file where a parent folder should be: `create_dir_all` fails on every OS.
    let blocker = env.dir.path().join("blocker");
    std::fs::write(&blocker, "a file").unwrap();
    env.set_notes_folder(&slash(&blocker.join("notes")));
    let (_daemon, mut client) = env.start();
    let id = client.add_project(&repo).id;

    let body = daemon_error(client.call(new_note(id)));
    assert_eq!(body.code, ErrorCode::InvalidRequest);
    assert!(body.message.contains("notes folder"), "{}", body.message);
    assert!(blocker.is_file());
    assert!(std::fs::read_dir(&repo).unwrap().next().is_none());
    client.stop();
}

#[cfg(windows)]
#[test]
fn a_folder_on_a_drive_that_does_not_exist_is_an_error_answer() {
    let letter = ('D'..='Z')
        .find(|letter| !Path::new(&format!("{letter}:\\")).exists())
        .expect("a free drive letter");
    let env = Env::new();
    let repo = env.repo();
    env.set_notes_folder(&format!("{letter}:/notes/"));
    let (_daemon, mut client) = env.start();
    let id = client.add_project(&repo).id;

    let body = daemon_error(client.call(new_note(id)));
    assert_eq!(body.code, ErrorCode::InvalidRequest);
    assert!(!Path::new(&format!("{letter}:\\notes")).exists());
    assert!(std::fs::read_dir(&repo).unwrap().next().is_none());
    client.stop();
}
```

Notes for the engineer on this file:

- `mkrew_protocol::ErrorBody` is the type inside `ResponseError::Daemon(body)` (`message.rs` line 375); `error_code` in `main.rs` matches the same variant and returns `body.code`, so `body.message` is there too. The type is not in the import list of `main.rs`, hence the full path.
- The last entry of the refusal table (`deep/../nope.md`) spells a path that leaves the folder through `..` and returns to it by name; it must be refused too (the check is on components, not on the final place).
- `env.dir` is the `TempDir` field of `Env`, readable from a child module of `main.rs`.

- [ ] **Step 13: Run and see them fail**

Run: `cargo test -p mkrew-daemon --lib notes -j 4`
Expected: FAIL to compile: `non-exhaustive patterns: &Request::FsNewNote(_) not covered` in `server.rs`, then (after Step 14 is half done) `cannot find function `new_note`` and `cannot find function `notes_file``.

- [ ] **Step 14: Implement the daemon side**

`crates/mkrew-daemon/src/docs.rs`, after `relative_path`:

```rust
/// A client-supplied absolute path inside `folder` (the notes folder, as
/// [`mkrew_core::notes::folder`] normalized it), with `/` separators, and the same path for
/// joining; `None` for a relative path, `folder` itself, a path outside it and one that steps
/// out of it with `..`. The exception to [`relative_path`] for `fs.read` / `fs.write` of notes
/// (Tern panes spec 2026-11-23 section 7.2).
// ponytail: compared component by component, so `c:/notes` and `C:/notes` differ; the daemon
// answers the folder in the spelling of the setting, which is what the client sends back.
pub fn notes_file(file_path: &str, folder: &Path) -> Option<(String, PathBuf)> {
    let normalized = file_path.trim().replace('\\', "/");
    let path = PathBuf::from(&normalized);
    let mut inside = path.strip_prefix(folder).ok()?.components().peekable();
    inside.peek()?;
    (path.is_absolute() && inside.all(|c| matches!(c, Component::Normal(_))))
        .then_some((normalized, path))
}
```

`crates/mkrew-daemon/src/file_ops.rs`: add to the `use` lines `use chrono::NaiveDateTime;` and `use mkrew_core::notes::{self, NotesFolder};`, and after `create` add:

```rust
/// The most names `new_note` tries before it gives up (`note-<time>.md`, `-2`, ... `-1000`).
const NOTE_TRIES: u32 = 1000;

/// An empty note named for local time `at` in `folder`: below `root` for a relative folder,
/// where it says for an absolute one; missing folders are made. A taken name moves on to the
/// next number (`create` never overwrites). Returns the note relative to `root` when it is
/// inside it, else as an absolute `/`-separated path. An error carries its [`ErrorKind`] and a
/// message for the user that names the folder.
// ponytail: "inside the root" is a lexical prefix test; a root and a folder spelled differently
// (8.3 names, another case on Windows) answer absolute, which `fs.read` / `fs.write` accept too.
pub(crate) fn new_note(
    root: &Path,
    folder: &NotesFolder,
    at: NaiveDateTime,
) -> Result<String, (ErrorKind, String)> {
    let (base, dir) = match folder {
        NotesFolder::Relative(dir) => (root, dir.as_str()),
        NotesFolder::Absolute(path) => (path.as_path(), ""),
    };
    for attempt in 1..=NOTE_TRIES {
        match create(base, dir, &notes::file_name(at, attempt), false) {
            Ok(made) => return Ok(answer(root, folder, &made)),
            Err((ErrorKind::AlreadyExists, _)) => {}
            Err((kind, message)) => {
                let place = join_relative(base.to_path_buf(), dir);
                return Err((kind, format!("{message} (notes folder {})", place.display())));
            }
        }
    }
    Err((
        ErrorKind::AlreadyExists,
        format!("No free note name in the notes folder after {NOTE_TRIES} tries"),
    ))
}

/// `made` (relative to the folder `new_note` used) as the client sees it.
fn answer(root: &Path, folder: &NotesFolder, made: &str) -> String {
    let NotesFolder::Absolute(path) = folder else {
        return made.to_owned();
    };
    match path.strip_prefix(root) {
        Ok(inside) if inside.as_os_str().is_empty() => made.to_owned(),
        Ok(inside) => format!("{}/{made}", inside.to_string_lossy().replace('\\', "/")),
        Err(_) => format!("{}/{made}", path.to_string_lossy().replace('\\', "/")),
    }
}
```

`crates/mkrew-daemon/src/server.rs`, after the `FsCreate` arm:

```rust
            Request::FsNewNote(params) => (self.fs_new_note(params).await, After::Continue),
```

`crates/mkrew-daemon/src/server/fs.rs`:

1. Imports: add `FsNewNoteParams` to the `mkrew_protocol::{...}` list and `use mkrew_core::notes::{self, NotesFolder};` above `use mkrew_protocol::{`.
2. Replace the head of `file_target` (the doc comment and the lines up to `let target = ...`):

```rust
    /// The checks `fs.read` and `fs.write` share: the project-relative path (outside `.git`),
    /// or an absolute path inside the absolute notes folder of `settings.toml` (Tern panes spec
    /// 2026-11-23 section 7.2), the watched target, and the file's place.
    async fn file_target(
        &self,
        project_id: Ulid,
        worktree_task_id: Option<Ulid>,
        path: &str,
    ) -> Result<(String, PathBuf, Target), Outcome> {
        let folder = match notes::folder(&self.settings.current().notes.folder) {
            Ok(NotesFolder::Absolute(folder)) => Some(folder),
            _ => None,
        };
        let Some((path, relative)) = docs::relative_path(path).or_else(|| {
            folder
                .as_deref()
                .and_then(|folder| docs::notes_file(path, folder))
        }) else {
            return Err(outside("path", path));
        };
        if in_git_dir(&path) {
            return Err(outside("path", &path));
        }
        let target = self
            .watched_target(project_id, worktree_task_id, None)
            .await?;
        // An absolute `relative` (a note outside the root) replaces `target.dir` in `join`.
        let file = target.dir.join(relative);
        Ok((path, file, target))
    }
```

3. In `fs_read`, the git index text only makes sense for a path inside the root; replace

```rust
        let shown = path.clone();
        let (git, root, want_text) = (Arc::clone(&self.git), target.dir.clone(), params.index_text);
```

with

```rust
        let shown = path.clone();
        let want_text = params.index_text && !Path::new(&path).is_absolute();
        let (git, root) = (Arc::clone(&self.git), target.dir.clone());
```

4. After `fs_create`, add:

```rust
    /// `fs.new_note` (Tern panes spec 2026-11-23 section 7.2): an empty
    /// `note-YYYYMMDD-HHMMSS.md` (local time) in the notes folder of `settings.toml`, read per
    /// request so a changed setting applies at once. Empty folder = the target's root.
    pub(super) async fn fs_new_note(&self, params: FsNewNoteParams) -> Outcome {
        let folder = match notes::folder(&self.settings.current().notes.folder) {
            Ok(NotesFolder::Relative(dir)) if in_git_dir(&dir) => {
                return outside("[notes] folder", &dir);
            }
            Ok(folder) => folder,
            Err(message) => return Outcome::error(ErrorCode::InvalidRequest, message),
        };
        let target = match self
            .watched_target(params.project_id, params.worktree_task_id, None)
            .await
        {
            Ok(target) => target,
            Err(outcome) => return outcome,
        };
        let at = chrono::Local::now().naive_local();
        let made =
            tokio::task::spawn_blocking(move || file_ops::new_note(&target.dir, &folder, at)).await;
        match made {
            Ok(Ok(path)) => ok(&FsCreated { path }),
            Ok(Err((ErrorKind::AlreadyExists, message))) => {
                Outcome::error(ErrorCode::AlreadyExists, message)
            }
            Ok(Err((_, message))) => Outcome::error(ErrorCode::InvalidRequest, message),
            Err(e) => Outcome::error(ErrorCode::Internal, format!("fs.new_note failed: {e}")),
        }
    }
```

- [ ] **Step 15: Run the daemon unit tests, then the end-to-end tests**

```
cargo test -p mkrew-daemon --lib notes -j 4
cargo test -p mkrew-daemon --lib new_note -j 4
cargo build -p mkrew-cli -p mkrew-fakeagent --bins -j 4
cargo test -p mkrew-daemon --test daemon notes -j 4
cargo test -p mkrew-daemon --test daemon fs_ -j 4
```

Expected: PASS for `notes_file_accepts_absolute_paths_inside_the_folder_only`, the four `new_note_*` unit tests, every test of `notes.rs` (`a_new_note_in_the_root_...`, `notes_made_in_the_same_second_get_numbers`, `a_relative_folder_is_trimmed_...`, `an_absolute_folder_inside_the_root_...`, `an_absolute_folder_outside_the_root_...`, `unusable_folders_are_refused_...`, `a_folder_that_cannot_be_made_...`, and on Windows `a_folder_on_a_drive_that_does_not_exist_...`), and the existing `fs_*` tests unchanged (`fs_list_read_and_find_follow_file_changes`, `fs_create_makes_files_and_folders_and_refuses_existing_and_outside`).

- [ ] **Step 16: SPEC section 20.0 row**

`docs/SPEC.md`, in the section 20.0 TOML block, after the two `[zed]` lines

```
[zed]
follow_debounce_ms = 1500   # §20C.4
```

add a blank line and exactly these two lines:

```
[notes]                     # Tern panes spec 2026-11-23 §7.2; where New note saves its file
folder = ""                 # "" = the workspace root; relative = below it; absolute = used as is (not ~; no ..), for every workspace; the daemon reads it
```

(The blank line before `[features]` stays as it is.)

- [ ] **Step 17: Format, lint, run, commit**

```
cargo fmt --all
cargo clippy -p mkrew-core -p mkrew-protocol -p mkrew-daemon --all-targets -j 4 -- -D warnings
cargo test -p mkrew-protocol -j 4
cargo test -p mkrew-daemon --lib -j 4
cargo test -p mkrew-daemon --test daemon -j 4
git add Cargo.lock docs/SPEC.md crates/mkrew-protocol/src/files.rs crates/mkrew-protocol/src/message.rs crates/mkrew-protocol/src/message/tests.rs crates/mkrew-daemon/Cargo.toml crates/mkrew-daemon/src/docs.rs crates/mkrew-daemon/src/file_ops.rs crates/mkrew-daemon/src/server.rs crates/mkrew-daemon/src/server/fs.rs crates/mkrew-daemon/tests/daemon/main.rs crates/mkrew-daemon/tests/daemon/notes.rs
git -c user.email=tungnv975@gmail.com commit -m "feat(daemon): fs.new_note and notes-folder access for fs.read and fs.write"
```

Expected: clippy clean; every test PASS.
Before the commit run `cargo build --workspace -j 4` once: no other crate matches `Request` exhaustively (a search for `Request::FsCreate` finds only `server.rs`, `message.rs` and tests), so it must compile.

- [ ] **Step 18: The UI's notes folder for OS drops (replaces the Task 6 stub)**

Task 6 left `Shell::notes_folder` returning `None` in `crates/mkrew-ui/src/shell/navigation.rs` (next to `drop_os_files`; locate it with `grep -n "fn notes_folder" crates/mkrew-ui/src/shell/navigation.rs`).
The UI re-reads `settings.toml` per call through `self.settings_file` (the same way `Shell::jira_settings` does in `shell/jira.rs`), so a hand edit applies at once.
Replace the whole function, doc comment included, with:

```rust
    /// The absolute `[notes] folder` the daemon reads and writes files in (`fs.read` / `fs.write`
    /// accept absolute paths inside it); `None` while the folder is empty, relative or unusable.
    /// A folder inside the workspace root is returned too: `classify_os_files` lets the root win,
    /// so those files open by their relative path. OS drops are local files, so this is the
    /// local `settings.toml`; a remote machine's daemon has its own `[notes] folder`, and
    /// `drop_os_files` refuses OS drops in a machine window before it asks.
    fn notes_folder(&self) -> Option<PathBuf> {
        let setting = mkrew_core::settings::load(&self.settings_file).ok()?.notes.folder;
        match mkrew_core::notes::folder(&setting) {
            Ok(mkrew_core::notes::NotesFolder::Absolute(folder)) => Some(folder),
            _ => None,
        }
    }
```

No new test: the folder resolution is tested in `mkrew_core::notes`, and the root-wins rule in `drop_zone::tests::the_root_wins_when_the_notes_folder_is_inside_it` (Task 6).
`NotesFolder::Absolute` holds a `/`-separated path with no trailing separator, the same spelling `classify_os_files` stores and `fs.new_note` answers, so one note has one `PaneKey::File`.

Run:

```
cargo fmt --all
cargo clippy -p mkrew-ui --all-targets -j 4 -- -D warnings
cargo test -p mkrew-ui drop_zone -j 4
git add crates/mkrew-ui/src/shell/navigation.rs
git -c user.email=tungnv975@gmail.com commit -m "feat(ui): OS drops open files in the notes folder"
```

Expected: clippy clean, `drop_zone` tests PASS.

---

### Task 8: The `+` row, `DraggedNew`, `Command::NewNote`, `Icon::NotebookPen`

Contract deviations and decisions, read first:

- `plus_row_items` takes `Option<&lifecycle::PaneAgent>` (what `pane_agents()` yields; there is no `Profile` type in the UI) and the shell's display name.
  Signature: `plus_row_items(default_agent: Option<&PaneAgent>, shell_name: Option<&str>, main: bool) -> Vec<RowItem>`.
- The default agent item shows only when the workspace marks a default (`PaneAgent.default`, which `pane_agents()` sorts first).
  With no installed profile, or profiles but no default marked, it is hidden; `▾` still lists every profile.
  Its click and drag start `NewKind::Agent(Some(id))`, so they never hit `ask_for_default`.
- No default shell (`project.default_shell == None`): the terminal item still shows ("New shell tab").
  Click and drop go through today's path (`create_tab` / `spawn_pane` -> `ask_for_default`): the toast "Pick a default shell for this workspace, then try again." and the workspace menu (main window only).
- Keyboard stepping needs no new pure function: `dialog_focus::group_step` and the `chip_arrows` widget already do "Left / Right between chips, the ends stay" and are unit tested (`arrows_move_inside_a_row_of_chips_and_stop_at_its_ends`).
  Every item is a `focus_stop` (the ring) with `chip_arrows(index, 1 + items)`; `+` is index 0.
- Expand animation uses `theme::motion::FAST` (120 ms ease-out, tokens only) instead of the spec's 150 ms.
  The only 150 ms in the design is the collapse delay, a new token `PLUS_ROW_COLLAPSE_DELAY`.
  It fades in and slides `PANEL_RISE` from the left; it does not tween the real width (needs a measured width; add if the fade looks abrupt in the smoke).
  Reduce-motion needs no code: `main.rs:233` calls `cx.set_reduce_motion(..)` and gpui draws `with_animation` in its end state (ADR 0097; same as `widgets::appear`).
- A click on `+` itself opens the full menu (today's behavior; spec is silent).
  So a pointer-less or touch user still reaches everything, and Enter on a focused `+` does the same.
- Detached windows (spec gap, decided here): the same row without `▾`.
  `new_menu` is a main-window flag (`menu_host` is reset to Main) and a detached window already has Split ▾.
  A click there starts the session or note as a pane right of the focused pane (`Edge::Right`), not as a new tab, because new tabs always belong to the main window.
  The tooltips say "pane" and show the `SplitAgent` / `SplitShell` keys; New note shows no key there (`NewNote` is main-window only, like `NewShellTab`, see `detached::handles`).
  The row there is built for the project of the window's tab (`tab_scope(window)`).
- `Shell::new_note` opens the answer through Task 4's `open_document(.., Place::NewTab | Place::Split(..))`, then `set_file_mode(.., MdMode::Source, ..)`.
  `open_document` focuses the editor of a new leaf itself (confirmed with Task 4), which gives "cursor in the editor".
  When the drop target pane is gone before the answer arrives, the note opens as `Place::NewTab`.
- Task 6 names used (read from `local://plan-t6.md`): `trait PaneDragged { fn takes(&self, shell: &Shell, project_id: Ulid, target: &PaneKey) -> bool; fn land(&self, shell: &mut Shell, project_id: Ulid, target: PaneKey, edge: Edge, window: &mut Window, cx: &mut Context<'_, Shell>); fn ended(&self, shell: &mut Shell, cx: &mut Context<'_, Shell>) {} }`, the private generic `hover_of::<T>` / `drop_of::<T>`, the two lists `pane_drop_target` (hover) and `pane_drops` (drop; used by the pane frame and the drag strip), `Shell::take_pane_drop(&mut self, target: &PaneKey) -> Option<Edge>` and `Shell::strip_drops(&self, strip: Div, theme, cx) -> Div` (main window strip only).
  `DraggedNew` joins as a fourth entry of both lists and of `strip_drops`; the generic overlay needs no change.
- Docs for this task (SPEC §10 `+` row and detached rows, §13 `NewNote`, HUMAN_CHECKS) belong to Task 9; its hand-off list is at the end of this task.

**Files:**

- Create `crates/mkrew-ui/src/plus_row.rs` (pure: `NewKind`, `RowItem`, `plus_row_items`, `default_agent`, `Reveal`, tests).
- Create `crates/mkrew-ui/src/shell/plus_row.rs` (gpui: `DraggedNew`, `Placement`, `render_new_tab_button` moved here, hover, click, drop, `new_note`).
- Create `icon/lucide/notebook-pen.svg`.
- Modify `crates/mkrew-ui/src/lib.rs` (add `pub mod plus_row;`), `crates/mkrew-ui/src/icons.rs` (macro entry), `crates/mkrew-ui/src/theme.rs` (token `PLUS_ROW_COLLAPSE_DELAY`, near `PEEK_LEAVE_GRACE` ~617).
- Modify `crates/mkrew-ui/src/keymap.rs` (variant ~58, `all()` ~289, `name()` ~407, `mac_keys()` ~538, `pc_keys()` ~627, test) and `crates/mkrew-ui/src/palette.rs` (`title()` ~616, `available()` ~744).
- Modify `crates/mkrew-ui/src/main.rs` (imports ~95 and ~151, fields after `new_menu` ~619), `crates/mkrew-ui/src/shell.rs` (`pub(crate) mod plus_row;` after `pane_render` ~15), `crates/mkrew-ui/src/shell/connection.rs` (field init after `new_menu: false` ~378).
- Modify `crates/mkrew-ui/src/shell/sessions.rs` (`new_pane_with` 20-72 -> wrapper + `spawn_pane`; `pane_agents` 76-82; `run` ~314 and ~350), `crates/mkrew-ui/src/shell/workspace.rs` (`place_session` ~300-341 + `place_session_at` + `born_chat` + `project_holding`).
- Modify `crates/mkrew-ui/src/shell/chrome.rs` (delete `render_new_tab_button` 613-692; `render_title_bar` calls ~302 and ~306), `crates/mkrew-ui/src/shell/pane_render.rs` (`render_tab_area` signature ~10 and call ~103), `crates/mkrew-ui/src/shell/detached_render.rs` (`render_detached_tab_area` ~142 and its last call ~211), `crates/mkrew-ui/src/shell/pane_drop.rs` (Task 6's `pane_drop_target`, `pane_drops`, `strip_drops`).
- Test: `crates/mkrew-ui/src/plus_row.rs` `mod tests`; `crates/mkrew-ui/src/keymap.rs` `mod tests`; existing `icons::tests::every_icon_is_served_and_is_a_lucide_svg` covers the icon.
  No test for gpui drawing, drops or `new_note` wiring (smoke in Task 9).

**Interfaces:**

Consumes (earlier tasks): `PaneKey`, `Leaf`, `Edge` (Task 1), `Tabs::{tab_with, insert_leaf}`, `Shell::focused_key(&self, window: &Window) -> Option<PaneKey>` (Task 1), `Place`, `Shell::open_document(&mut self, project_id: Ulid, path: String, line: Option<(u32, u32)>, place: Place, window, cx)`, `Shell::set_file_mode(&mut self, project_id: Ulid, path: &str, mode: MdMode, window, cx)` (Task 4), `Shell::take_pane_drop`, `Shell::hover_pane_drop`, `pane_drop_target`, the generic overlay (Task 6), `mkrew_protocol::{FsNewNoteParams, Request::FsNewNote, FsCreated}` (Task 7).
Consumes (existing): `lifecycle::{PaneAgent, pane_agents}`, `Shell::{open_new_tab, open_new_menu, ask_for_default, still_starting, tab_scope, detached_key, focus_pane, save_layout, sync_panes, refresh_files, toast_error, modal_open}`, `dialog_focus::group_step` via `DialogControl::{focus_stop, chip_arrows}`, `widgets::{agent_chip, loading_dots, tooltip, PopupStyle}`, `reorder::DragChip`, `pressable::Look`.
Produces:

```rust
// crates/mkrew-ui/src/plus_row.rs (pure)
pub enum NewKind { Agent(Option<String>), Shell, Note }
pub enum RowItem { Agent { id: String, name: String, icon: Option<String> }, Shell { name: Option<String> }, Note, More }
impl RowItem {
    pub fn kind(&self) -> Option<NewKind>;            // None for More
    pub fn label(&self, main: bool) -> String;        // tooltip text
    pub fn command(&self, main: bool) -> Option<Command>; // tooltip shortcut
}
pub fn default_agent(agents: &[PaneAgent]) -> Option<&PaneAgent>;
pub fn plus_row_items(default_agent: Option<&PaneAgent>, shell_name: Option<&str>, main: bool) -> Vec<RowItem>;
pub struct Reveal; // enter() / leave() -> u64 / collapse(u64) -> bool / dismiss() / shown()
// crates/mkrew-ui/src/shell/plus_row.rs
pub(crate) struct DraggedNew { pub(crate) kind: NewKind, pub(crate) title: SharedString }
pub(crate) enum Placement { Beside(Axis), At(PaneKey, Edge) }
impl Shell {
    pub(crate) fn new_note(&mut self, at: Option<(PaneKey, Edge)>, window: &mut Window, cx: &mut Context<'_, Self>);
    pub(crate) fn start_new(&mut self, kind: NewKind, at: Option<(PaneKey, Edge)>, window: &mut Window, cx: &mut Context<'_, Self>);
    pub(crate) fn drop_new_on_strip(&mut self, item: &DraggedNew, window: &mut Window, cx: &mut Context<'_, Self>);
}
impl PaneDragged for DraggedNew { /* takes: the pane belongs to a workspace; land: start_new at the edge */ }
// sessions.rs
Shell::spawn_pane(&mut self, kind: SessionKind, profile: Option<String>, placement: Placement, window, cx)
Shell::pane_agents_of(&self, project_id: Option<Ulid>) -> Vec<lifecycle::PaneAgent>
// workspace.rs
Shell::place_session_at(&mut self, project_id: Ulid, session: Ulid, tab_id: Ulid, target: &PaneKey, edge: Edge, window, cx)
Shell::project_holding(&self, key: &PaneKey) -> Option<(Ulid, Ulid)>  // (project, tab) that holds the pane
```
`Command::NewNote` (`new_note`, PC `ctrl-alt-n`, mac `cmd-alt-n`, global), `Icon::NotebookPen`.

- [ ] **Step 1: Check what Tasks 1-7 left**

Run grep (the `seek_grep` tool, or `rg` in a shell) in `crates/` for each pattern.
Expected: every one is found; if one is missing, stop and fix that task first, this task does not re-create it.

| Pattern | File | Why |
|---|---|---|
| `fn insert_leaf` | `crates/mkrew-ui/src/tabs.rs` | Task 1: place a session leaf on an edge |
| `fn tab_with` | `crates/mkrew-ui/src/tabs.rs` | Task 1: which tab holds a key |
| `fn focused_key` | `crates/mkrew-ui/src/shell/hosts.rs` | Task 1: detached click needs the focused pane |
| `fn open_document` and `fn set_file_mode` | `crates/mkrew-ui/src/shell` | Task 4 |
| `trait PaneDragged`, `fn pane_drops`, `fn strip_drops`, `fn take_pane_drop` | `crates/mkrew-ui/src/shell/pane_drop.rs` | Task 6 (names fixed in its draft) |
| `FsNewNoteParams` | `crates/mkrew-protocol/src` | Task 7 |
| `set_reduce_motion` | `crates/mkrew-ui/src/main.rs` | reduce-motion proof (ADR 0097), about line 233 |

Also check these names are in scope for `shell/*.rs` files (they `use crate::*;`): `Place`, `Leaf`, `PaneKey`, `Edge`, `MdMode`, `FsNewNoteParams`.
Grep `main.rs` for `use mkrew_ui::documents`, `use mkrew_ui::split`, `use mkrew_core::settings` and `use mkrew_protocol`.
If one name is missing, add it to the existing `use` line of its crate in `main.rs` (the compiler in Step 8 names any left over).

- [ ] **Step 2: Failing tests for the pure row model**

Create `crates/mkrew-ui/src/plus_row.rs` with the tests only:

```rust
//! The tab strip's `+` row (SPEC §10, ADR 0108): which items it shows, what each one starts,
//! and when it is open. Pure and tested headlessly; `shell/plus_row.rs` draws it.

#[cfg(test)]
mod tests {
    use super::*;

    fn agent(id: &str, default: bool) -> PaneAgent {
        PaneAgent {
            index: 0,
            id: id.into(),
            name: id.to_uppercase(),
            icon: None,
            default,
        }
    }

    #[test]
    fn the_row_lists_agent_shell_note_and_more() {
        let claude = agent("claude", true);
        let items = plus_row_items(Some(&claude), Some("PowerShell"), true);
        assert_eq!(
            items,
            [
                RowItem::Agent {
                    id: "claude".into(),
                    name: "CLAUDE".into(),
                    icon: None
                },
                RowItem::Shell {
                    name: Some("PowerShell".into())
                },
                RowItem::Note,
                RowItem::More,
            ]
        );
    }

    #[test]
    fn without_a_default_agent_the_row_starts_at_the_shell() {
        let items = plus_row_items(None, None, true);
        assert_eq!(
            items,
            [RowItem::Shell { name: None }, RowItem::Note, RowItem::More]
        );
    }

    #[test]
    fn only_a_marked_default_gives_the_agent_item() {
        assert_eq!(default_agent(&[]), None);
        let unmarked = [agent("codex", false), agent("claude", false)];
        assert_eq!(default_agent(&unmarked), None, "`▾` still lists them");
        let marked = [agent("claude", true), agent("codex", false)];
        assert_eq!(
            default_agent(&marked).map(|agent| agent.id.as_str()),
            Some("claude")
        );
    }

    #[test]
    fn a_detached_window_has_no_menu_item_and_splits_instead() {
        let claude = agent("claude", true);
        let items = plus_row_items(Some(&claude), None, false);
        assert!(!items.contains(&RowItem::More));
        assert_eq!(items[0].label(false), "New CLAUDE pane");
        assert_eq!(items[0].command(false), Some(Command::SplitAgent));
        assert_eq!(items[1].command(false), Some(Command::SplitShell));
        assert_eq!(RowItem::Note.command(false), None, "NewNote is main-window only");
    }

    #[test]
    fn labels_and_shortcuts_follow_the_window() {
        let shell = RowItem::Shell {
            name: Some("zsh".into()),
        };
        assert_eq!(shell.label(true), "New zsh tab");
        assert_eq!(RowItem::Shell { name: None }.label(true), "New shell tab");
        assert_eq!(shell.command(true), Some(Command::NewShellTab));
        assert_eq!(RowItem::Note.label(true), "New note");
        assert_eq!(RowItem::Note.command(true), Some(Command::NewNote));
        assert_eq!(RowItem::More.label(true), "More…");
        assert_eq!(RowItem::More.command(true), None);
    }

    #[test]
    fn items_say_what_they_start() {
        let claude = RowItem::Agent {
            id: "claude".into(),
            name: "Claude".into(),
            icon: None,
        };
        assert_eq!(claude.kind(), Some(NewKind::Agent(Some("claude".into()))));
        assert_eq!(RowItem::Shell { name: None }.kind(), Some(NewKind::Shell));
        assert_eq!(RowItem::Note.kind(), Some(NewKind::Note));
        assert_eq!(RowItem::More.kind(), None, "`▾` opens the menu, it is no drag source");
    }

    #[test]
    fn the_pointer_opens_the_row_and_the_delay_closes_it() {
        let mut row = Reveal::default();
        assert!(!row.shown());
        row.enter();
        assert!(row.shown());
        let serial = row.leave();
        assert!(row.shown(), "still open while the delay runs");
        assert!(row.collapse(serial));
        assert!(!row.shown());
    }

    #[test]
    fn coming_back_cancels_the_pending_close() {
        let mut row = Reveal::default();
        row.enter();
        let stale = row.leave();
        row.enter();
        assert!(!row.collapse(stale), "the pointer is over the row again");
        assert!(row.shown());
        let serial = row.leave();
        assert!(!row.collapse(serial.wrapping_sub(1)), "only the last leave closes");
        assert!(row.collapse(serial));
    }

    #[test]
    fn escape_closes_it_until_the_next_hover() {
        let mut row = Reveal::default();
        row.enter();
        row.dismiss();
        assert!(!row.shown());
        row.enter();
        assert!(row.shown());
    }
}
```

Add `pub mod plus_row;` to `crates/mkrew-ui/src/lib.rs` after `pub mod plan_dialog;` (before `pub mod ports;`).

Run: `cargo test -p mkrew-ui --lib plus_row -j 4` (`--lib`: the `mkrew-ui` binary does not build until Step 8, because `Shell::run` must handle the new command)
Expected: FAIL to compile: `cannot find type `PaneAgent` in this scope`, `cannot find function `plus_row_items``, `cannot find type `RowItem``.

- [ ] **Step 3: Implement the pure model**

Insert above `#[cfg(test)]` in `crates/mkrew-ui/src/plus_row.rs`:

```rust
use crate::keymap::Command;
use crate::lifecycle::PaneAgent;

/// What an item of the `+` row starts, by click or by drop.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NewKind {
    /// An agent; `None` = the workspace's default agent.
    Agent(Option<String>),
    /// The workspace's default shell.
    Shell,
    /// A new note file (`fs.new_note`).
    Note,
}

/// One item after `+` in the row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RowItem {
    /// The workspace's default agent profile.
    Agent {
        id: String,
        name: String,
        icon: Option<String>,
    },
    /// The workspace's default shell; `name` is its profile name when known.
    Shell { name: Option<String> },
    Note,
    /// `▾`: today's full menu.
    More,
}

impl RowItem {
    /// What a click or a drop of the item starts; `None` for `▾`.
    pub fn kind(&self) -> Option<NewKind> {
        match self {
            RowItem::Agent { id, .. } => Some(NewKind::Agent(Some(id.clone()))),
            RowItem::Shell { .. } => Some(NewKind::Shell),
            RowItem::Note => Some(NewKind::Note),
            RowItem::More => None,
        }
    }

    /// The tooltip text; the main window opens tabs, a detached window splits its tab.
    pub fn label(&self, main: bool) -> String {
        let what = if main { "tab" } else { "pane" };
        match self {
            RowItem::Agent { name, .. } => format!("New {name} {what}"),
            RowItem::Shell { name: Some(name) } => format!("New {name} {what}"),
            RowItem::Shell { name: None } => format!("New shell {what}"),
            RowItem::Note => "New note".to_owned(),
            RowItem::More => "More…".to_owned(),
        }
    }

    /// The shortcut the tooltip shows: the one that does the same in this window.
    pub fn command(&self, main: bool) -> Option<Command> {
        match (self, main) {
            (RowItem::Agent { .. }, true) => Some(Command::NewAgentTab),
            (RowItem::Agent { .. }, false) => Some(Command::SplitAgent),
            (RowItem::Shell { .. }, true) => Some(Command::NewShellTab),
            (RowItem::Shell { .. }, false) => Some(Command::SplitShell),
            (RowItem::Note, true) => Some(Command::NewNote),
            (RowItem::Note, false) | (RowItem::More, _) => None,
        }
    }
}

/// The agent the row offers: the workspace's marked default (`pane_agents` lists it first).
/// No installed profile, or none marked: no agent item, `▾` still lists them all.
pub fn default_agent(agents: &[PaneAgent]) -> Option<&PaneAgent> {
    agents.first().filter(|agent| agent.default)
}

/// The items after `+`, in order. `main`: the main window, which also has `▾` (a detached
/// window has Split ▾ in its title bar and no new-tab menu).
pub fn plus_row_items(
    default_agent: Option<&PaneAgent>,
    shell_name: Option<&str>,
    main: bool,
) -> Vec<RowItem> {
    let mut items = Vec::with_capacity(4);
    if let Some(agent) = default_agent {
        items.push(RowItem::Agent {
            id: agent.id.clone(),
            name: agent.name.clone(),
            icon: agent.icon.clone(),
        });
    }
    items.push(RowItem::Shell {
        name: shell_name.map(str::to_owned),
    });
    items.push(RowItem::Note);
    if main {
        items.push(RowItem::More);
    }
    items
}

/// Whether the pointer holds the `+` row open: it opens at once and closes a delay after the
/// pointer left `+` and the row (the serial and timer pattern of `peek::Hover`). A focused
/// item opens it too, but that is read from the focus, not stored here.
#[derive(Debug, Default)]
pub struct Reveal {
    over: bool,
    shown: bool,
    serial: u64,
}

impl Reveal {
    /// The pointer entered `+` or the row: open, and any pending close is stale.
    pub fn enter(&mut self) {
        self.over = true;
        self.shown = true;
        self.serial += 1;
    }

    /// The pointer left both: returns the serial to give [`Reveal::collapse`] after the delay.
    pub fn leave(&mut self) -> u64 {
        self.over = false;
        self.serial += 1;
        self.serial
    }

    /// The delay of leave `serial` ran out: true when the row closed (the pointer did not come
    /// back and no later leave replaced this one).
    pub fn collapse(&mut self, serial: u64) -> bool {
        let due = self.shown && !self.over && serial == self.serial;
        if due {
            self.shown = false;
        }
        due
    }

    /// Esc: closed now, until the pointer enters again.
    pub fn dismiss(&mut self) {
        self.over = false;
        self.shown = false;
        self.serial += 1;
    }

    pub fn shown(&self) -> bool {
        self.shown
    }
}
```

`Command::NewNote` and `Command::SplitAgent` must exist for this to compile; `NewNote` lands in Step 6.
So in this step add the keymap variant and its sites first: do Step 6 items 1-5 (variant, `all()`, `name()`, `mac_keys()`, `pc_keys()`) now, then continue.
(The `run` arm and the palette entries come in Step 6 as well; they are only needed before the shell builds.)

Run: `cargo test -p mkrew-ui --lib plus_row -j 4`
Expected: PASS, 8 tests.

- [ ] **Step 4: No commit yet**

Do not commit: the keymap variant is already added and the `mkrew-ui` binary stays red until `Shell::run` handles `NewNote` (Step 6 item 9, Step 8).
All files of this task are committed together in Step 10, except the icon (Step 5), which compiles alone.

- [ ] **Step 5: The note icon**

Create `icon/lucide/notebook-pen.svg` (same header and format as `icon/lucide/pencil.svg`, the lucide-static path data of `notebook-pen`):

```svg
<!-- @license lucide-static v1.55.0 - ISC -->
<svg
  class="lucide lucide-notebook-pen"
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
  <path d="M13.4 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2v-7.4" />
  <path d="M2 6h4" />
  <path d="M2 10h4" />
  <path d="M2 14h4" />
  <path d="M2 18h4" />
  <path d="M21.378 5.626a1 1 0 1 0-3.004-3.004l-5.01 5.012a2 2 0 0 0-.506.854l-.837 2.87a.5.5 0 0 0 .62.62l2.87-.837a2 2 0 0 0 .854-.506z" />
</svg>
```

In `crates/mkrew-ui/src/icons.rs`, in the `Icon in "lucide"` list, between `Moon => "moon",` and `OctagonAlert => "octagon-alert",` add:

```rust
        NotebookPen => "notebook-pen",
```

Run: `cargo test -p mkrew-ui --lib icons -j 4`
Expected: PASS (`every_icon_is_served_and_is_a_lucide_svg` loops `Icon::ALL` and checks `<svg` plus the 24 px `viewBox`).
If a lucide package is at hand, compare the six `d` strings with its `notebook-pen.svg`; they must be identical.

Commit:

```
git add icon/lucide/notebook-pen.svg crates/mkrew-ui/src/icons.rs
git -c user.email=tungnv975@gmail.com commit -m "feat(ui): Icon::NotebookPen"
```

- [ ] **Step 6: `Command::NewNote` (all nine sites) and its keymap test**

Failing test first. In `crates/mkrew-ui/src/keymap.rs` `mod tests`, next to the test that asserts `bound("ctrl-alt-t")` (~1527), add a new test:

```rust
    #[test]
    fn new_note_has_its_own_global_keys() {
        let keymap = Keymap::defaults_for(Os::Windows);
        assert_eq!(
            keymap.command_for(&keystroke("ctrl-alt-n")),
            Some(Command::NewNote)
        );
        assert_eq!(keymap.label(Command::NewNote), "Ctrl+Alt+N");
        assert_eq!(Command::NewNote.context(), None);
        assert_eq!(Command::NewNote.default_keys_for(Os::Mac), "cmd-alt-n");
        assert_eq!(Command::NewNote.name(), "new_note");
    }
```

Run: `cargo test -p mkrew-ui --lib new_note_has_its_own_global_keys -j 4`
Expected: FAIL to compile: `no variant or associated item named `NewNote` found for enum `Command``.

The nine sites (anchor on the neighbouring `NewAgentTab` line; line numbers are hints):

| # | File | Place | Add |
|---|---|---|---|
| 1 | `crates/mkrew-ui/src/keymap.rs` | `pub enum Command`, after `NewAgentTab,` (~58) | `/// A new note file in a new tab (ADR 0108).` then `NewNote,` |
| 2 | `keymap.rs` | `Command::all()`, after `Command::NewAgentTab,` (~289) | `Command::NewNote,` |
| 3 | `keymap.rs` | `name()`, after the `NewAgentTab` arm (~407) | `Command::NewNote => "new_note".into(),` |
| 4 | `keymap.rs` | `mac_keys()`, after `Command::NewAgentTab => "cmd-shift-a",` (~538) | `Command::NewNote => "cmd-alt-n",` |
| 5 | `keymap.rs` | `pc_keys()`, after the `NewAgentTab` arm (~627) | `Command::NewNote => "ctrl-alt-n".into(),` |
| 6 | `keymap.rs` | `context()` | no edit: `_ => None` keeps it global (checked by the test) |
| 7 | `crates/mkrew-ui/src/palette.rs` | `title()`, after `Command::NewAgentTab => "Tabs: New agent tab".into(),` (~616) | `Command::NewNote => "Tabs: New note".into(),` |
| 8 | `palette.rs` | `available()`, the arm `Command::NewShellTab \| Command::NewAgentTab \| Command::OpenInZed ... => context.workspace` (~744) | add `\| Command::NewNote` after `Command::NewAgentTab` |
| 9 | `crates/mkrew-ui/src/shell/sessions.rs` | `run`: the "Opening or picking a tab or pane shows the terminals" group (~315) and the dispatch (~350) | add `\| Command::NewNote` after `Command::NewAgentTab` in the group; after `Command::NewAgentTab => self.new_tab(SessionKind::Agent, window, cx),` add `Command::NewNote => self.new_note(None, window, cx),` |

`ctrl-alt-n` and `cmd-alt-n` are free (checked against `keymap.rs`: `ctrl-shift-n` is NewTask, `cmd-n` NewTask, `cmd-shift-n` MoveTabToNewWindow, the `ctrl-alt-<letter>` and `cmd-alt-<letter>` families exist already).
The tests `every_default_parses_and_is_unique` and `every_binding_becomes_a_gpui_key_binding` prove there is no clash and that the keys parse.
`detached::handles` is not changed: `NewNote` does nothing in a detached window, like `NewShellTab` (its test `detached_commands_are_an_explicit_list` stays as is).

Items 1-5 are done in Step 3 so `plus_row.rs` compiles; items 7-9 and the test are done here.
Item 9 calls `new_note`, written in Step 7; if you build between steps, expect `no method named new_note` until then.

- [ ] **Step 7: Placement, `spawn_pane`, `place_session_at`, `pane_agents_of`, `project_holding`**

Add the token in `crates/mkrew-ui/src/theme.rs` right after `PEEK_LEAVE_GRACE`:

```rust
/// How long the tab strip's `+` row stays open after the pointer left `+` and the row, so the
/// pointer can travel to its items (`plus_row::Reveal`).
pub const PLUS_ROW_COLLAPSE_DELAY: Duration = Duration::from_millis(150);
```

`crates/mkrew-ui/src/shell/sessions.rs`: replace `new_pane_with` (lines 18-72, from its doc comment to the closing brace) with this wrapper and the new `spawn_pane`:

```rust
    /// [`Shell::new_pane`] with an agent profile picked (`None` = the workspace's default of
    /// `kind`, which must be set; a picked profile needs no default).
    pub(crate) fn new_pane_with(
        &mut self,
        kind: SessionKind,
        profile: Option<String>,
        axis: Axis,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        self.spawn_pane(kind, profile, Placement::Beside(axis), window, cx);
    }

    /// Starts a session of `kind` (`profile` as in [`Shell::new_pane_with`]) and shows it where
    /// `placement` says: beside the focused pane, or on an edge of a given pane in the tab that
    /// holds it (a drop from the `+` row, which may be a tab in a detached window).
    pub(crate) fn spawn_pane(
        &mut self,
        kind: SessionKind,
        profile: Option<String>,
        placement: Placement,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let Some(connection) = self.connection.clone() else {
            return;
        };
        let scope = match &placement {
            Placement::Beside(_) => self.tab_scope(window),
            Placement::At(target, _) => self.project_holding(target),
        };
        let Some((project_id, tab_id)) = scope else {
            return;
        };
        if self.still_starting(cx)
            || (profile.is_none() && self.ask_for_default(project_id, kind, window, cx))
        {
            return;
        }
        self.starting_pane = Some(kind);
        cx.notify();
        let params = lifecycle::spawn_params(project_id, Some(tab_id), kind, profile.as_deref());
        let spawned = connection.call::<SessionSpawnResult>(Request::SessionSpawn(params));
        cx.spawn_in(window, async move |this, cx| {
            let spawned = spawned.await;
            let _ = this.update_in(cx, |shell, window, cx| {
                shell.starting_pane = None;
                match spawned {
                    Ok(spawned) => {
                        // In the tab the request came from when a detached window shows it,
                        // else next to the focused pane of the tab active now (the user may
                        // have switched tabs meanwhile), unless the daemon placed it already.
                        match placement {
                            Placement::Beside(axis) => shell.place_session(
                                project_id,
                                spawned.session_id,
                                Some(tab_id),
                                axis,
                                window,
                                cx,
                            ),
                            Placement::At(target, edge) => shell.place_session_at(
                                project_id,
                                spawned.session_id,
                                tab_id,
                                &target,
                                edge,
                                window,
                                cx,
                            ),
                        }
                        shell.sync_panes(window, cx);
                    }
                    Err(error) => {
                        tracing::warn!(%project_id, "session.spawn failed: {error:#}");
                        let what = lifecycle::kind_noun(kind);
                        shell.toast_error(format!("Cannot start {what}: {error:#}"), cx);
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }
```

Replace `pane_agents` (lines 74-82) with a version that takes the project, so a detached window's row reads its own workspace:

```rust
    /// The installed agent profiles a new pane can start with in the active workspace, its
    /// default first (the `+` menu and the palette).
    pub(crate) fn pane_agents(&self) -> Vec<lifecycle::PaneAgent> {
        self.pane_agents_of(self.workspace)
    }

    /// [`Shell::pane_agents`] for `project_id` (the `+` row of a detached window reads the
    /// workspace of its own tab).
    pub(crate) fn pane_agents_of(&self, project_id: Option<Ulid>) -> Vec<lifecycle::PaneAgent> {
        let default = project_id.and_then(|id| {
            let project = self.sidebar.projects().iter().find(|p| p.id == id)?;
            project.default_agent.as_deref()
        });
        lifecycle::pane_agents(&self.agent_profiles, default)
    }
```

`crates/mkrew-ui/src/shell/workspace.rs`:

1. In `place_session` (~323-325) replace the statement

```rust
        let chat = (self.sidebar.sessions().iter())
            .any(|s| s.id == session && s.kind == SessionKind::Agent)
            .then_some(self.chat_default);
```

with `let chat = self.born_chat(session);` (keep the two comment lines above it).

2. Directly after `place_session` add:

```rust
    /// The saved mode a new pane of `session` is born with: an agent gets the default of the
    /// moment (ADR 0105), so a later change of `[ui] chat_default` never flips it.
    fn born_chat(&self, session: Ulid) -> Option<bool> {
        (self.sidebar.sessions().iter())
            .any(|s| s.id == session && s.kind == SessionKind::Agent)
            .then_some(self.chat_default)
    }

    /// The workspace and the tab that hold the pane `key` (any window).
    pub(crate) fn project_holding(&self, key: &PaneKey) -> Option<(Ulid, Ulid)> {
        self.tabs.iter().find_map(|(project_id, tabs)| {
            tabs.tab_with(key).map(|tab| (*project_id, tab.id))
        })
    }

    /// Shows `session` on `edge` of `target`'s pane in tab `tab_id` (a drop from the `+` row).
    /// A session the daemon or an echo placed already stays; when `target` left the tab while
    /// the session started, it lands beside the focused pane like any new pane.
    pub(crate) fn place_session_at(
        &mut self,
        project_id: Ulid,
        session: Ulid,
        tab_id: Ulid,
        target: &PaneKey,
        edge: Edge,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let chat = self.born_chat(session);
        let Some(tabs) = self.tabs.get_mut(&project_id) else {
            return;
        };
        if tabs.tab_of(session).is_some() {
            return;
        }
        let write = tabs.insert_leaf(tab_id, Some(target), edge, Leaf::Session { session, chat });
        match write {
            Some(write) => self.save_layout(project_id, write, window, cx),
            None => self.place_session(project_id, session, Some(tab_id), Axis::Vertical, window, cx),
        }
    }
```

(`insert_leaf` focuses the new leaf and returns the write; `None` means the target or the tab is gone, so the fallback is the old path.)

- [ ] **Step 8: The row, `DraggedNew`, `new_note`, hover, click and drop**

Wire the module: in `crates/mkrew-ui/src/shell.rs` add `pub(crate) mod plus_row;` after `pub(crate) mod pane_render;`.
In `crates/mkrew-ui/src/main.rs` add `use mkrew_ui::plus_row::{Reveal, NewKind, RowItem, default_agent, plus_row_items};` after `use mkrew_ui::plan_dialog::PlanDialog;`, and `use shell::plus_row::*;` after `use shell::pane_render::*;`.
Add two fields after `new_menu: bool,` (~619):

```rust
    /// The pointer holds the tab strip's `+` row open (`plus_row::Reveal`); a focused item
    /// opens it too (`plus_focus`).
    plus: Reveal,
    /// Wraps the `+` row: `contains_focused` keeps it open while `+` or an item has the keyboard.
    plus_focus: FocusHandle,
```

and in `crates/mkrew-ui/src/shell/connection.rs`, after `new_menu: false,` (~378):

```rust
            plus: Reveal::default(),
            plus_focus: cx.focus_handle(),
```

Delete `render_new_tab_button` and its doc comment (`chrome.rs` 613-692); `open_new_menu` and `new_menu_rows` stay.

Create `crates/mkrew-ui/src/shell/plus_row.rs`:

```rust
//! The `+` row of the tab strip (SPEC §10, ADR 0108): `+` grows on hover or keyboard focus into
//! the default agent, the default terminal, a new note and `▾` (today's menu). Each item starts
//! something by click, or by drag onto a pane edge or the tab strip.

use crate::*;

/// Payload of a drag from an item of the `+` row: what to start where it is dropped.
#[derive(Debug, Clone)]
pub(crate) struct DraggedNew {
    pub(crate) kind: NewKind,
    pub(crate) title: SharedString,
}

/// Where a pane being started lands once its session exists.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Placement {
    /// Beside the focused pane of the tab in front, split along the axis (Split right / down).
    Beside(Axis),
    /// On an edge of a given pane, in the tab that holds it (a drop from the `+` row).
    At(PaneKey, Edge),
}

impl Shell {
    /// The `+` after the last tab. Hover or keyboard focus opens the row of items on the same
    /// line; the pointer may leave for `PLUS_ROW_COLLAPSE_DELAY` without closing it. While a
    /// pane starts it shows the loading dots instead (one start at a time). `host`: the main
    /// window (items open tabs, `▾` opens the menu) or a detached one (items split its tab).
    pub(crate) fn render_new_tab_button(
        &self,
        host: HostId,
        now: i64,
        theme: &Theme,
        window: &Window,
        cx: &mut Context<'_, Self>,
    ) -> gpui::AnyElement {
        if let Some(kind) = self.starting_pane {
            let what = match kind {
                SessionKind::Agent => "Starting agent",
                SessionKind::Shell => "Starting shell",
            };
            return div()
                .id("new-tab")
                .flex_none()
                .min_w(theme::rems_px(theme::CONTROL_HEIGHT))
                .h(theme::rems_px(theme::CONTROL_HEIGHT))
                .px(px(space::S))
                .flex()
                .items_center()
                .justify_center()
                .tooltip(tooltip(theme, what, ""))
                .child(loading_dots(now, rgb(theme.text_muted)))
                .into_any_element();
        }
        let main = host == HostId::Main;
        let project = if main {
            self.workspace
        } else {
            self.tab_scope(window).map(|(project, _)| project)
        };
        let agents = self.pane_agents_of(project);
        let shell_name = project.and_then(|id| self.default_shell_name(id));
        let items = plus_row_items(default_agent(&agents), shell_name.as_deref(), main);
        // `+` is tab stop 0, the items follow: ← / → stop at the ends of the row.
        let stops = items.len() + 1;
        // The menu opens below the row, outside it: the pointer is then off the row, which
        // stays open while the menu does.
        let open =
            self.plus.shown() || self.new_menu || self.plus_focus.contains_focused(window, cx);
        let plus = div()
            .id("new-tab")
            .flex_none()
            .h(theme::rems_px(theme::CONTROL_HEIGHT))
            .px(px(space::S))
            .flex()
            .items_center()
            .rounded(px(radius::MEDIUM))
            .text_color(rgb(theme.text_muted))
            .pressable(Look::Icon, true, theme)
            .focus_stop(theme)
            .chip_arrows(0, stops)
            // The row names its items once open; the tooltip would sit on top of them.
            .when(!open, |el| {
                el.tooltip(tooltip(
                    theme,
                    "New tab",
                    format!(
                        "{} / {}",
                        self.keymap.label(Command::NewAgentTab),
                        self.keymap.label(Command::NewShellTab)
                    ),
                ))
            })
            // Enter on a focused `+` clicks it too (gpui). An open menu already closed on
            // this mouse down (outside it): the click reopens it. A detached window has none.
            .on_press(
                main,
                cx.listener(|shell, _, window, cx| shell.open_new_menu(window, cx)),
            )
            .child(icon_svg(Icon::Plus, theme::ICON_SIZE, theme.text_muted));
        let row = open.then(|| {
            div()
                .flex()
                .items_center()
                .children(
                    items
                        .iter()
                        .enumerate()
                        .map(|(index, item)| self.new_row_item(item, index + 1, stops, main, theme, cx)),
                )
                .with_animation(
                    "new-row-items",
                    gpui::Animation::new(theme::motion::FAST).with_easing(gpui::ease_out_quint()),
                    |el, delta| {
                        el.opacity(delta)
                            .relative()
                            .left(px(-theme::PANEL_RISE * (1.0 - delta)))
                    },
                )
        });
        let menu = (main && self.new_menu && !self.modal_open()).then(|| {
            render_menu(
                self.menu_state(),
                "new-tab-panel",
                self.new_menu_rows(),
                None,
                Place::Dropdown,
                theme,
                cx,
            )
        });
        // No gap between `+` and its items and no padding: the pointer never leaves the row
        // between them, and a click between two items cannot focus the wrapper.
        div()
            .id("new-row")
            .flex_none()
            .flex()
            .items_center()
            .track_focus(&self.plus_focus)
            .on_hover(cx.listener(|shell, hovered: &bool, window, cx| {
                shell.hover_new_row(*hovered, window, cx);
            }))
            .on_key_down(cx.listener(|shell, event: &KeyDownEvent, window, cx| {
                if event.keystroke.key == "escape" && !event.keystroke.modifiers.modified() {
                    cx.stop_propagation();
                    shell.plus.dismiss();
                    shell.focus_pane(window, cx);
                    cx.notify();
                }
            }))
            .child(plus)
            .children(row)
            .children(menu)
            .into_any_element()
    }

    /// One item after `+`: a `pressable(Look::Icon)` with its tooltip and shortcut, a tab stop
    /// with the focus ring (`index` in the row, `+` is 0), a click that starts it and, but for
    /// `▾`, a drag that starts it where it is dropped.
    fn new_row_item(
        &self,
        item: &RowItem,
        index: usize,
        stops: usize,
        main: bool,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> gpui::AnyElement {
        let label = item.label(main);
        let keys = item
            .command(main)
            .map(|command| self.keymap.label(command))
            .unwrap_or_default();
        let glyph = match item {
            RowItem::Agent { id, icon, .. } => agent_chip(
                sidebar::agent_badge(icon.as_deref(), id),
                theme.text_muted,
                theme,
            )
            .into_any_element(),
            RowItem::Shell { .. } => {
                icon_svg(Icon::Terminal, theme::ICON_SIZE, theme.text_muted).into_any_element()
            }
            RowItem::Note => {
                icon_svg(Icon::NotebookPen, theme::ICON_SIZE, theme.text_muted).into_any_element()
            }
            RowItem::More => {
                icon_svg(Icon::ChevronDown, theme::ICON_SIZE_SMALL, theme.text_muted)
                    .into_any_element()
            }
        };
        let base = div()
            .id(SharedString::from(format!("new-row-{index}")))
            .flex_none()
            .h(theme::rems_px(theme::CONTROL_HEIGHT))
            .px(px(space::S))
            .flex()
            .items_center()
            .rounded(px(radius::MEDIUM))
            .pressable(Look::Icon, true, theme)
            .focus_stop(theme)
            .chip_arrows(index, stops)
            .tooltip(tooltip(theme, &label, keys))
            .child(glyph);
        match item.kind() {
            Some(kind) => {
                let style = PopupStyle::of(theme);
                let accent = theme.accent;
                let drag = DraggedNew {
                    kind: kind.clone(),
                    title: label.into(),
                };
                base.on_press(
                    true,
                    cx.listener(move |shell, _, window, cx| {
                        shell.click_new(kind.clone(), window, cx);
                    }),
                )
                .on_drag(drag, move |item, offset, _, cx| {
                    let (style, title) = (style.clone(), item.title.clone());
                    cx.new(|_| DragChip {
                        style,
                        accent,
                        title,
                        offset,
                    })
                })
                .into_any_element()
            }
            None => base
                .on_press(
                    true,
                    cx.listener(|shell, _, window, cx| shell.open_new_menu(window, cx)),
                )
                .into_any_element(),
        }
    }

    /// The display name of the workspace's default shell profile, if it has one.
    fn default_shell_name(&self, project_id: Ulid) -> Option<String> {
        let project = (self.sidebar.projects().iter()).find(|p| p.id == project_id)?;
        let id = project.default_shell.as_deref()?;
        (self.agent_profiles.iter())
            .find(|profile| profile.id == id)
            .map(|profile| profile.name.clone())
    }

    /// The pointer entered or left the row: it opens at once; leaving closes it after
    /// `PLUS_ROW_COLLAPSE_DELAY` unless the pointer came back (serial and timer, as `peek`).
    pub(crate) fn hover_new_row(
        &mut self,
        hovered: bool,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        if hovered {
            self.plus.enter();
            cx.notify();
            return;
        }
        let serial = self.plus.leave();
        cx.spawn_in(window, async move |this, cx| {
            cx.background_executor()
                .timer(theme::PLUS_ROW_COLLAPSE_DELAY)
                .await;
            let _ = this.update_in(cx, |shell, _, cx| {
                if shell.plus.collapse(serial) {
                    cx.notify();
                }
            });
        })
        .detach();
    }

    /// A click (or Enter) on an item. The main window opens a tab. A detached window has no
    /// new tab of its own (new tabs belong to the main window), so it splits its tab beside
    /// the focused pane, like its Split ▾.
    pub(crate) fn click_new(
        &mut self,
        kind: NewKind,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let at = self
            .detached_key(window)
            .and_then(|_| self.focused_key(window))
            .map(|key| (key, Edge::Right));
        self.start_new(kind, at, window, cx);
    }

    /// Starts `kind`: `at` `None` = in a new tab right after the active one; `Some` = in a leaf
    /// on that edge of that pane. Without a default of the kind, the usual `ask_for_default`
    /// toast and workspace menu.
    pub(crate) fn start_new(
        &mut self,
        kind: NewKind,
        at: Option<(PaneKey, Edge)>,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        match (kind, at) {
            (NewKind::Agent(profile), None) => {
                self.open_new_tab(Some((SessionKind::Agent, profile)), window, cx);
            }
            (NewKind::Shell, None) => {
                self.open_new_tab(Some((SessionKind::Shell, None)), window, cx);
            }
            (NewKind::Agent(profile), Some((key, edge))) => {
                self.spawn_pane(SessionKind::Agent, profile, Placement::At(key, edge), window, cx);
            }
            (NewKind::Shell, Some((key, edge))) => {
                self.spawn_pane(SessionKind::Shell, None, Placement::At(key, edge), window, cx);
            }
            (NewKind::Note, at) => self.new_note(at, window, cx),
        }
    }

    /// An item dropped on the tab strip (the main window's; `strip_drops`): a new tab right
    /// after the active one.
    pub(crate) fn drop_new_on_strip(
        &mut self,
        item: &DraggedNew,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        if self.modal_open() {
            return;
        }
        self.start_new(item.kind.clone(), None, window, cx);
    }

    /// New note (SPEC §20.0 `[notes] folder`): `fs.new_note` makes the file, then it opens
    /// kept, in source mode, with the cursor in the editor; in a new tab after the active one
    /// (`at` `None`) or in a leaf on an edge of a pane. A failure shows `toast_error` with the
    /// reason and nothing opens.
    pub(crate) fn new_note(
        &mut self,
        at: Option<(PaneKey, Edge)>,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let Some(connection) = self.connection.clone() else {
            return;
        };
        // The workspace of the pane dropped on, else of this window's tab, else the shown one.
        let holder = (at.as_ref())
            .and_then(|(key, _)| self.project_holding(key))
            .map(|(project, _)| project);
        let Some(project_id) = holder
            .or_else(|| self.tab_scope(window).map(|(project, _)| project))
            .or(self.workspace)
        else {
            return;
        };
        // The Files view's worktree target, as `create_entry`: the note opens from there.
        let target = (self.files.get(&project_id)).and_then(|view| view.worktree_task_id);
        let made = connection.call::<FsCreated>(Request::FsNewNote(FsNewNoteParams {
            project_id,
            worktree_task_id: target,
        }));
        cx.spawn_in(window, async move |this, cx| {
            let made = made.await;
            let _ = this.update_in(cx, |shell, window, cx| match made {
                Ok(FsCreated { path }) => {
                    // The pane dropped on may have closed while the daemon wrote the file.
                    let place = match at {
                        Some((key, edge)) if shell.project_holding(&key).is_some() => {
                            Place::Split(key, edge)
                        }
                        _ => Place::NewTab,
                    };
                    shell.open_document(project_id, path.clone(), None, place, window, cx);
                    shell.set_file_mode(project_id, &path, MdMode::Source, window, cx);
                    shell.refresh_files(project_id, window, cx);
                }
                Err(error) => {
                    tracing::warn!(%project_id, "new note failed: {error:#}");
                    shell.toast_error(format!("Cannot create the note: {error:#}"), cx);
                }
            });
        })
        .detach();
    }
}
```

Thread `host`, `now` and `window` to the row:

| File | Old | New |
|---|---|---|
| `pane_render.rs` `render_tab_area` signature (~10) | `(&self, project_id: Ulid, tabs: &Tabs, now: i64, theme: &Theme, cx: &mut Context<'_, Self>)` | add `window: &Window,` after `theme: &Theme,` |
| `pane_render.rs` last call (~103) | `Some(self.render_new_tab_button(now, theme, cx)),` | `Some(self.render_new_tab_button(HostId::Main, now, theme, window, cx)),` |
| `chrome.rs` `render_title_bar` (~302) | `self.render_tab_area(project_id, tabs, now, theme, cx)` | `self.render_tab_area(project_id, tabs, now, theme, window, cx)` |
| `chrome.rs` (~306) | `HostId::Detached(key) => Some(self.render_detached_tab_area(key, theme, cx)),` | `HostId::Detached(key) => Some(self.render_detached_tab_area(key, now, theme, window, cx)),` |
| `detached_render.rs` signature (~142) | `(&self, key: WindowKey, theme: &Theme, cx: &mut Context<'_, Self>)` | `(&self, key: WindowKey, now: i64, theme: &Theme, window: &Window, cx: &mut Context<'_, Self>)` |
| `detached_render.rs` last call (~211) | `self.tab_area_frame(host, chips, active_chip, None, theme, cx)` | `let new_row = self.render_new_tab_button(host, now, theme, window, cx);` then `self.tab_area_frame(host, chips, active_chip, Some(new_row), theme, cx)` |
| `detached_render.rs` doc (~139) | "No `+`: new tabs are made in the main window." | "The `+` row has no `▾`: its items split this window's tab." |

If Tasks 4-6 changed these signatures (the chip arguments, `area_items`), keep their shape and only add the parameters above.

Drops, three places (all Task 6 mechanisms; no raw `on_drag_move` / `on_drop` of our own on panes).

(a) At the end of `crates/mkrew-ui/src/shell/plus_row.rs` implement Task 6's trait.
`pane_drops` / `drop_of` already run `ended`, `take_pane_drop`, and the `modal_open` check before `land`, so `land` only starts the thing:

```rust
impl PaneDragged for DraggedNew {
    /// Any pane that belongs to a workspace takes it: the session or note starts in the
    /// workspace that holds the pane (`project_holding`).
    fn takes(&self, shell: &Shell, _project_id: Ulid, target: &PaneKey) -> bool {
        shell.project_holding(target).is_some()
    }

    fn land(
        &self,
        shell: &mut Shell,
        _project_id: Ulid,
        target: PaneKey,
        edge: Edge,
        window: &mut Window,
        cx: &mut Context<'_, Shell>,
    ) {
        shell.start_new(self.kind.clone(), Some((target, edge)), window, cx);
    }
}
```

(b) In `crates/mkrew-ui/src/shell/pane_drop.rs` add `DraggedNew` as the fourth entry of both lists.
In `pane_drop_target`, after the `DraggedPath` line:

```rust
        let frame = Self::hover_of::<DraggedNew, _>(frame, project_id, key, cx);
```

In `pane_drops`, replace its last line `Self::drop_of::<DraggedPath, E>(el, project_id, key, cx)` with:

```rust
        let el = Self::drop_of::<DraggedPath, E>(el, project_id, key, cx);
        Self::drop_of::<DraggedNew, E>(el, project_id, key, cx)
```

`pane_drops` is used by the pane frame and by the drag strip (`pane_edge`), so the strip over a pane takes the drop too.

(c) In `strip_drops` (same file) add the tint and the drop next to the `DraggedPath` ones:

```rust
            .drag_over::<DraggedNew>(move |style, _, _, _| style.bg(tint))
            .on_drop(cx.listener(|shell, item: &DraggedNew, window, cx| {
                shell.drop_new_on_strip(item, window, cx);
            }))
```

`strip_drops` is applied to the main window's strip only (Task 6), so a detached strip takes no `+` drop.
The `+` row sits inside the strip, so a drop on it falls to the strip.

Run: `cargo build -p mkrew-ui -j 4`
Expected: builds without warnings (the `mkrew-ui` binary is green again).
Likely compile fixes, in this order: a name missing from `main.rs` imports (`Place`, `Leaf`, `MdMode`, `FsNewNoteParams`, `PaneKey`, `Edge`, `PaneDragged`; add to the existing `use` line of its crate); `Tabs::insert_leaf` argument order (Task 1 contract: `(tab_id, target: Option<&PaneKey>, edge, leaf)`).

- [ ] **Step 9: Format, lint, tests**

Run: `cargo fmt --all`
Run: `cargo clippy -p mkrew-ui --all-targets -j 4 -- -D warnings`
Expected: clean.
Watch for: `clippy::too_many_arguments` (more than 7 arguments excluding `self`): `place_session_at` has exactly 7, `new_row_item` 6, so none should fire; if one does, fold `(project_id, tab_id)` into a tuple argument rather than adding an `allow`.

Run: `cargo test -p mkrew-ui --lib plus_row -j 4` (8 tests), then `cargo test -p mkrew-ui --lib keymap -j 4`, `cargo test -p mkrew-ui --lib palette -j 4`, `cargo test -p mkrew-ui --lib icons -j 4`, `cargo test -p mkrew-ui --lib detached -j 4`, `cargo test -p mkrew-ui --lib dialog_focus -j 4`.
Expected: all PASS, including `new_note_has_its_own_global_keys`, `every_default_parses_and_is_unique` and `detached_commands_are_an_explicit_list`.
If rustc crashes on the whole `mkrew-ui` test build (`STATUS_STACK_BUFFER_OVERRUN`), keep to the filtered runs above.

- [ ] **Step 10: Commit the shell work**

```
git add crates/mkrew-ui/src/plus_row.rs crates/mkrew-ui/src/lib.rs crates/mkrew-ui/src/shell/plus_row.rs crates/mkrew-ui/src/shell.rs crates/mkrew-ui/src/main.rs crates/mkrew-ui/src/shell/connection.rs crates/mkrew-ui/src/shell/sessions.rs crates/mkrew-ui/src/shell/workspace.rs crates/mkrew-ui/src/shell/chrome.rs crates/mkrew-ui/src/shell/pane_render.rs crates/mkrew-ui/src/shell/detached_render.rs crates/mkrew-ui/src/shell/pane_drop.rs crates/mkrew-ui/src/theme.rs crates/mkrew-ui/src/keymap.rs crates/mkrew-ui/src/palette.rs
git -c user.email=tungnv975@gmail.com commit -m "feat(ui): the + row with default agent, terminal and note, drag to an edge, New note command"
```

**Hand-off to Task 9 (docs and smoke, not done here):**

- SPEC §10 (title bar `+`): replace the `+ ▾` bullet with the row, its hover, focus, Esc and collapse behavior; add that a detached window shows the row without `▾` and its items split the window's tab.
  §13: row "Tabs | New note (a new tab with a note file, `[notes] folder`) | Ctrl+Alt+N" and the macOS table "NewNote | ctrl-alt-n | cmd-alt-n"; §20B.5: the `+` row items drag onto a pane edge or the tab strip.
- HUMAN_CHECKS "2026-11-23 Tern panes": hover `+` and see the row open at once and close about 150 ms after leaving, and not close while the pointer travels to an item; Tab to `+`, Right and Left walk the items and stop at the ends, Enter starts, Esc closes and returns the keyboard to the pane; with reduced motion the row appears at once; the agent tooltip names the default agent and shows `Ctrl+Shift+A`; no default shell: a click toasts "Pick a default shell for this workspace, then try again."; New note: the file `note-<date>-<time>.md` opens in a new tab, source mode, the cursor blinks in the editor, and a read-only folder shows an error toast and opens nothing; drag each of the three icons to each pane edge (overlay on the half) and to the tab strip; drag during a start shows the "still starting" toast; the dragged row closes while dragging out of it and the drag still drops (check the chips do not jump); a detached window shows `[agent][shell][note]` and each splits its tab; `▾` still opens the full menu and the row stays open while it is open.
- Screenshots: `docs/screenshots/tern-panes-plus-row-<dark|light>.png` with the row expanded.

---

### Task 9: ADR 0108, SPEC, HUMAN_CHECKS, smoke and screenshots

**Files:**

- Create: `docs/decisions/0108-tern-panes.md`
- Modify: `docs/decisions/0080-files-view-tabs.md` (status line, new last section; look the exact file name up with `ls docs/decisions | grep ^0080`)
- Modify: `docs/decisions/0106-tern-layout.md` (status line line 4, new last section)
- Modify: `docs/decisions/0041-closing-a-tab-removes-its-sessions.md` (header line, new last section; look the exact file name up with `ls docs/decisions | grep ^0041`)
- Modify: `docs/SPEC.md` (sections 1 Table of contents, 5, 7, 10, 11, 12A, 12B, 13, 14, 20B; section 20.0 only verified)
- Modify: `docs/HUMAN_CHECKS.md` (new section before line 24, one note under line 24)
- Modify: `docs/PROGRESS.md` (one note line after line 32, before the 2026-11-23 Tern layout line 33)
- Modify: `README.md` (lines 191-199: Terminals and Editor bullets)
- Create: `docs/screenshots/tern-panes-<view>-<dark|light>.png` (see Step 6)
- Do not edit `CHANGELOG.md`, the spec file `docs/superpowers/specs/2026-11-23-tern-panes-design.md` or any plan file.

**Interfaces:**

- Consumes: the finished code of Tasks 1 to 8 (names in `local://plan-contracts.md`).
  Before Step 3 run `git grep -nE "ShowEditor|CloseFileTab|ShowDiff|NewNote" -- crates/mkrew-ui/src/keymap.rs` and make sure the SPEC section 13 text below matches what Tasks 4, 5 and 8 really left (the text assumes: `ShowEditor` and `CloseFileTab` are deleted, `ShowDiff` stays on `ctrl-shift-7` and now opens or focuses the Diff tab, `NewNote` is `ctrl-alt-n` / `cmd-alt-n`).
  If a command differs, change the SPEC rows, not the code.
- Consumes the token values of Task 2: `pane_gap` Mkrew Dark `#000000`, Mkrew Light `#F3F4F6`; `PANE_HALF_GAP = space::XS`; `PANE_ACCENT_WIDTH` 2; `PANE_GAP_MIN_CONTRAST = 1.06`; `pane_focus_stripe` keeps the old `pane_focus_border` values `#49518D` / `#A4AAE7`.
  Re-read `crates/mkrew-ui/src/theme.rs` (`pane_gap` in `Theme::dark()` / `Theme::light()`) before Step 3 and fix the two hex values in the SPEC token table if Task 2 changed them.
- Produces: docs and screenshots only, no code.

Writing rules for every text below: no em dash (use `-`), one sentence per line in new prose, English.
The Markdown in the "replace with" blocks is the exact text to write; the "anchor" blocks are the exact current text (copy it from the file, do not retype).
Where a whole line is not quoted, the anchor is a unique substring of that line: replace only the substring.

- [ ] **Step 1: Re-check the numbers (ADR, protocol, branches)**

Run from the worktree `C:/Users/NguyenTung/Workspace/PersonalProject/mkrew-build-kit/mkrew/.worktrees/tern-panes`:

```bash
ls docs/decisions | tail -4
grep -n "PROTOCOL_VERSION: u32" crates/mkrew-protocol/src/lib.rs
git branch -a
for b in master feat/omp-tsp feat/md-split feat/macos-app feat/macos-keyboard feat/macos-terminal fix/restart-cleanup feat/chat-look; do
  echo "== $b"
  git show "$b:crates/mkrew-protocol/src/lib.rs" | grep -n "PROTOCOL_VERSION: u32"
  git ls-tree --name-only "$b" docs/decisions/ | tail -2
done
```

Also run `git branch -a` again for any branch that was not in the list on 2026-11-23 (`feat/chat-look`, `feat/macos-app`, `feat/macos-keyboard`, `feat/macos-terminal`, `feat/md-split`, `feat/omp-tsp`, `fix/restart-cleanup`, `master`, `remotes/origin/feat/macos-app`, `remotes/origin/master`) and repeat the `for` loop for each new one.

State on 2026-11-23 (when this plan was written): `master` has ADR 0106 as the newest and `PROTOCOL_VERSION = 58`; `feat/omp-tsp` has `PROTOCOL_VERSION = 59` and no ADR above 0106 committed yet (the plan reserves 0107 and 59 for it); every other branch is at or below 58.
This branch has `PROTOCOL_VERSION = 60` after Task 1.

Decision table (take the first row that applies):

| Finding | Action |
|---|---|
| `docs/decisions/0108-*` does not exist anywhere and no branch has protocol 60 or above | Keep 0108 and 60. Go on to Step 2. |
| Another branch or `master` already has `0108-*.md` | Take the next free number N (the highest ADR number on any branch plus 1, never 0107 while `feat/omp-tsp` is unmerged). Use N for the file name, the title and every reference below. |
| Another branch or `master` has `PROTOCOL_VERSION` 60 or more | Take P = the highest protocol number on any branch plus 1. Change `PROTOCOL_VERSION` in `crates/mkrew-protocol/src/lib.rs`, the test name `the_tern_panes_protocol_is_version_60` (rename it to `..._version_<P>`), and every doc text below. |
| `feat/omp-tsp` merged into `master` in the meantime | `master` is then at 59: 60 stays valid. Keep 0108 only if `feat/omp-tsp` used 0107 (check `ls docs/decisions`). |

After choosing different numbers, find every stale reference with:

```bash
git grep -nE "0108|protocol 60|Protocol 60|version_60|PROTOCOL_VERSION: u32 = 60" -- . ':!docs/superpowers/specs/2026-11-23-tern-panes-design.md'
```

and fix each hit.
The spec file itself is not edited (its status line already says to re-check at merge).
Also run `git log --oneline master..HEAD | head -1` and `git merge-base HEAD master` once: if `master` moved, run `git diff --stat HEAD...master -- docs/SPEC.md docs/decisions` to see whether a SPEC line quoted below changed (anchors then need a re-read).

- [ ] **Step 2: ADR 0108 and the amendment notes of ADR 0080, 0106 and 0041**

Create `docs/decisions/0108-tern-panes.md` with exactly this text (replace 0108 and 60 if Step 1 moved them):

````markdown
# 0108 - Tern panes: files and the diff as tabs and panes, four-edge drops, a `+` row

- Date: 2026-11-23
- Status: accepted (supersedes ADR 0080 and the Editor and Diff chips, the `+ ▾` button and the "Alternatives" line about Editor and Diff of ADR 0106; amends ADR 0041 and SPEC sections 5, 7, 10, 11, 12A, 12B, 13, 14, 20.0 and 20B.5)
- Milestone: none (human request 2026-11-23, spec `docs/superpowers/specs/2026-11-23-tern-panes-design.md`, plan `docs/superpowers/plans/2026-11-23-tern-panes.md`)

## Context

ADR 0106 gave Mkrew Tern's top bar, but the main area still had two worlds.
Terminals were tabs with split panes.
Files lived in an Editor view with its own tab strip (ADR 0080), the diff in a Diff view, and both showed as chips in the title bar.
Panes had a thin border, a 4 px gap and drops only to the right or the bottom half.
The user wants the area to work like Tern: panes with a frame and a gap, a stripe on the focused one, files and the diff that open as tabs or as panes next to the terminals, a drop zone on every edge, and a `+` that offers the common new things in one row.
A new text note should be one click.

## Decision

- **Typed leaves.**
  A leaf of the split tree is a session, a file or the diff: `Leaf::Session`, `Leaf::File { file, md, preview }`, `Leaf::Diff`.
  A leaf is identified by a `PaneKey` (`Session(Ulid)`, `File(String)`, `Diff`), and every tree and tab operation takes a key.
  The layout stays untagged JSON in the `terminal_tab.layout` column: `{"session": ...}` as before, `{"file": "src/a.rs"}`, `{"diff": true}`.
  A leaf that does not parse is dropped with a warning and the rest of the tab loads.
  There is no migration.
- **Documents store and `DocView`.**
  What `FilesView.tabs` held (text, caret, undo, dirty flag, disk stamp, preview page) moves into a pure `Documents` store, one per workspace, keyed by the stored path (`documents.rs`, no gpui).
  The gpui state of a shown file (focus, scroll handles, find bar, preview list) is a `DocView` per open file.
  A leaf whose document is missing (after a restart) loads it with `fs.read` the first time it is drawn.
  A document is dropped when its leaf leaves every layout and it is not dirty.
  The file tab strip, the Editor view and the Diff view are deleted.
- **One pane per file per workspace.**
  A file key and the Diff key appear at most once across all tabs and windows of a workspace.
  Opening what is open focuses its pane (its tab and window come to the front) and jumps to the line.
  So each path has one buffer and nothing counts references.
- **One way to open.**
  `Shell::open_document(path, place)` and `Shell::open_diff(place)` with `Place::{Preview, NewTab, Split(key, edge)}`.
  A single click in the Files tree opens the workspace's preview tab (italic title), which the next single click replaces; an edit, a double click or a drag keeps it.
  Every other open is a new tab right after the active one.
- **Four-edge drops.**
  `Edge::{Left, Right, Top, Bottom}` replaces `drop_axis`; the middle of a pane has no zone, the nearest edge wins (ties: right, then bottom).
  Files, tabs, panes and the `+` row items drop on any edge and split there 50 / 50; a pane or tab dropped on the tab strip's empty space becomes its own tab.
  A Files tree row dropped on a composer still inserts the path; dropped on a terminal pane it no longer types the path.
- **Pane look.**
  No pane border; panes sit on a frame of the new color `pane_gap` with 8 px between them and from the edge (`PANE_HALF_GAP = space::XS`), and the title bar uses the same color.
  One 2 px stripe at the pane's top edge: needs input first, then the session color, then the focus color when the tab has two or more panes.
  `pane_focus_border` is renamed `pane_focus_stripe`.
  A rule in `theme::problems()` needs `pane_gap` and `terminal_bg` to differ by at least 1.06:1.
- **The `+` row.**
  `+ ▾` becomes `+`, which grows on hover or keyboard focus into `[+] [default agent] [default terminal] [new note] [▾]`.
  Each item is a click (new tab) or a drag (leaf at the drop edge); `▾` opens today's menu.
- **Notes.**
  A new setting `[notes] folder` (empty = the workspace root, relative = under the root, absolute = as is) and a new request `fs.new_note { project_id, worktree_task_id }`.
  The daemon resolves the folder from its own `settings.toml`, names the file `note-YYYYMMDD-HHMMSS.md` in local time (`-2`, `-3`, ... on a clash), creates missing folders and answers `FsCreated { path }` (relative to the root inside it, else absolute).
  `fs.read` and `fs.write` accept one more kind of path: an absolute path inside the daemon's absolute notes folder (normalized, no `..`).
  Every other absolute path is refused as before.
  `Command::NewNote` has `ctrl-alt-n` (`cmd-alt-n` on macOS).
- **Daemon.**
  `cut()` and `layout_sessions()` look only at session leaves and skip the others; a tab whose last session ends but that still holds a file or the diff stays.
  `PROTOCOL_VERSION` 58 to 60 (59 belongs to `feat/omp-tsp`): an older UI must not read layouts with new leaves.

## Alternatives

- **B: typed tabs.**
  A tab would be a session tab, a file tab or a diff tab, and panes would stay sessions.
  A file could not sit beside a terminal in one tab, which is the point of the request, and drag and drop would need two models.
  Rejected.
- **C: documents in the daemon.**
  The daemon would keep open files so that every window and machine shares buffers.
  It would need a protocol for edits, undo and conflicts, and the UI already has all of that.
  Rejected; the daemon only learns the notes folder.
- **Keeping the Editor and Diff chips.**
  The chips were ADR 0106's answer ("Board, Editor and Diff as real tabs in the tab model: more work and a model change").
  With typed leaves the model change is small and the chips would be a second way to reach the same file.
  Rejected, and that Alternatives line of ADR 0106 no longer holds.
- **The same file in two panes.**
  Needs reference counting or two buffers for one path.
  Out of scope; one pane per file.
- **A notes folder per workspace, or a folder picker.**
  One global setting and a text field are enough for the first version (the Settings dialog has no folder picker kind).

## Consequences

- Protocol 60: a UI from before it cannot connect to the new daemon (the version check blocks it), so it never reads a layout it cannot parse.
  After the update the user restarts the daemon.
- A layout written by protocol 58 or 59 loads unchanged.
  A layout with file or diff leaves, read by an older daemon, would lose them in `cut()`; the version check prevents that pairing.
- `fs.read` and `fs.write` now know one path outside the root; the daemon never opens any other absolute path, so the rule "the daemon reads only inside the root and the notes folder" holds.
  A remote machine uses its own `settings.toml`, so its notes folder is that machine's.
- Closing a pane, a tab, a window, a workspace or the app asks about unsaved files as before (Save, Don't save, Cancel); moving a file leaf (drag, tear-off, merge) never asks.
  An edited preview tab is kept and not replaced.
- ADR 0080 is superseded: the strip, its keys (Ctrl+W, file tab cycling) and the Editor chip are gone, `Command::ShowEditor` and `Command::CloseFileTab` are deleted and Ctrl+Shift+6 and Ctrl+W are free.
  What carries over: the preview (italic) tab, the save question, one find bar per shown file.
- Detached windows draw file and diff leaves from the same store, so file tabs tear off like terminal tabs; "Plan 2 (Editor file tabs)" of ADR 0096's follow-up is not needed.
- `Command::ShowDiff` (Ctrl+Shift+7) opens the Diff tab or focuses it; it is no longer limited to "while a diff is open".
- OS files dropped on a file or diff pane open when they are inside the workspace root or the notes folder; others show one error toast.
- Typed leaves make the key the only identity: a file renamed on disk is a new key (its old pane shows "This file was removed.").
- Tabs inside a pane, Board and Docs as tabs, and restoring unsaved edits after a restart stay out of scope.
````

Append this to `docs/decisions/0080-files-view-tabs.md`:

1. Replace the line `- Status: accepted` (line 4) with:

```markdown
- Status: superseded by 0108
```

2. Append at the end of the file (after the last line `- A Markdown preview starts at the top when its tab is shown again (one shared page).`), after one blank line:

```markdown
## Superseded by 0108

ADR 0108 removes the Editor view, the file tab strip, its keys and `FilesView.tabs`.
An open file is a leaf of a terminal tab's split tree (a tab or a pane next to the terminals), its text lives in the per-workspace `Documents` store, and open files are saved in the tab layout, so they come back after a restart.
What stays from this ADR: a single click opens an italic preview tab that the next single click replaces while it is clean (the leaf's `preview` flag); closing asks "Save / Don't save / Cancel" for each unsaved file; the find bar belongs to the shown file.
What goes: the strip, pinned file tabs, Ctrl+W (`close_file_tab`), file-tab cycling under Ctrl+Tab and Ctrl+Alt+1-9, and "tabs are in memory only".
```

Edit `docs/decisions/0106-tern-layout.md`:

1. Replace line 4 (anchor, exact):

```markdown
- Status: accepted (supersedes the bar part of ADR 0102; amends ADR 0034, 0054, 0096 and SPEC sections 7, 10, 11, 13, 14 and 20.0)
```

with:

```markdown
- Status: accepted (supersedes the bar part of ADR 0102; amends ADR 0034, 0054, 0096 and SPEC sections 7, 10, 11, 13, 14 and 20.0); superseded in part by 0108 (the Editor and Diff chips, `+ ▾`, the thin pane border and the Alternatives line about Editor and Diff)
```

2. Append at the end of the file (after the last line `- The Editor view's own file-tab strip and the sidebar are unchanged.`), after one blank line:

```markdown
## Amended by 0108

ADR 0108 makes files and the diff leaves of the tab's split tree, so the Editor and Diff chips, the Editor view's file-tab strip and the Diff view are gone, and the Alternatives line "Board, Editor and Diff as real tabs in the tab model" no longer holds (Board and Docs stay icons).
`+ ▾` becomes the `+` row, panes lose their border for a frame gap and a top stripe, and the detached window's tab area has the `+` row.
The rest of this ADR (one top bar, no bottom bar, tab chip look, tab icons, no pane header, floating badges, limits chip, protocol 58) stands.
```

Edit `docs/decisions/0041-closing-a-tab-removes-its-sessions.md` (the file starts with `# 0041 — Closing a tab removes its sessions`; do not touch the title):

1. After the line `- Status: accepted` insert one line (the format of ADR 0027's header):

```markdown
- Amended by: 0108 (a layout leaf is a session, a file or the diff; only session leaves are removed)
```

2. Append at the end of the file (after `- Sessions in no tab (hidden panes) are not touched by closing a tab.`), after one blank line:

```markdown
## Amendment 2026-11-23 (ADR 0108)

A layout leaf can now be a file or the diff as well as a session.
`tab.close` removes the sessions of the tab's session leaves and only those; file and diff leaves are dropped with the tab and need no `session.remove`.
A tab whose last session ends while it still holds a file or the diff stays, with that leaf focused.
```

Run: `git add docs/decisions/0108-tern-panes.md docs/decisions/0080-*.md docs/decisions/0106-tern-layout.md docs/decisions/0041-*.md` after the SPEC step (commit below).

- [ ] **Step 3: SPEC edits (`docs/SPEC.md`)**

Edit from the bottom of the file to the top, so the line numbers quoted here stay valid while you work (line numbers are the worktree's on 2026-11-23, base `master` 0a8d404).
After each section run `git diff --stat docs/SPEC.md`.
Rule for the word "Editor": it stays as the name of the text editor inside a file pane (the "**Editor**" bullet of 12A, the `EDITOR_CONTEXT` key rows, "Editor find bar", "send selection from the Editor", "the Editor's preview", `font_size ... also the Editor and Git code views`, the unsaved-Editor-changes lines of 20J and 20K).
Only the places listed below change, because they speak of the Editor view, the Editor chip, the file tab strip or file tabs.

**Section 20B.5 (drag and drop), the numbered item `5. **Drop files / paste images.**`, lines 2400-2415.**

(a) Anchor, lines 2400-2402 (exact):

```markdown
5. **Drop files / paste images.** Dropping files from the OS onto an agent pane (gpui
   `ExternalPaths`) types their absolute paths, each quoted with `"` if needed, separated by spaces,
   with no Enter. The whole pane accepts the drop (terminal, composer, and the top-edge drag strip). While the pane's
```

Replace with:

```markdown
5. **Drop files / paste images.** Dropping files from the OS onto an agent or shell pane (gpui
   `ExternalPaths`) types their absolute paths, each quoted with `"` if needed, separated by spaces,
   with no Enter. The whole pane accepts the drop (terminal, composer, and the top-edge drag strip). While the pane's
```

(b) Anchor, lines 2407-2412 (exact):

```markdown
   **Drag from the file tree.** Dragging a row of the file tree (§12A) onto a pane types
   `composer.file_ref` with the path relative to the tree's folder (the project, or the task
   worktree) on an agent pane, the bare path on a shell pane, a folder ending with `/`, then one
   space; no Enter. The same rules as above: the whole pane accepts it, the paths go into the
   composer draft while it is shown, and the pane shows an accent ring while the row hovers it.
```

Replace with:

```markdown
   **Drag from the file tree** (ADR 0108).
   Dragging a row of the file tree (§12A) onto an agent pane's composer inserts `composer.file_ref` with the path relative to the tree's folder (the project, or the task worktree); a folder inserts its path ending with `/`, then one space; no Enter.
   A composer that is not shown takes no drop, and a row dropped on a terminal types nothing.
   The composer shows an accent ring while a row hovers it.
   A file row dropped on a pane (terminal, file or diff) opens the file there, at the edge under the pointer: the pane splits and the new file pane takes that side (§10 "Drops").
   A folder row is accepted only by a composer.
   **Drops on file and diff panes.**
   OS files dropped on a file or diff pane open each file that is inside the workspace root or the notes folder as a new tab.
   Folders and files outside both are skipped, and one error toast says "Only files inside the workspace or the notes folder open in Mkrew".
   A terminal pane keeps typing the dropped paths, as above.
```

(c) Anchor, line 2416 region: the `6. **Run with…**` line is untouched.

**Section 20B.4** (line 2394): anchor `` `Ctrl+Shift+L` in the Editor (a Markdown preview's selection ``; replace the substring `in the Editor (` with `in a file pane (`.
Anchor line 2139 (acceptance list) stays.

**Section 20.0** (`[notes] folder` settings row): not edited here.
Task 7 adds the row `[notes]` / `folder = ""` and the cross-check test needs it.
After Task 7 is merged run `git grep -n "^\[notes\]" docs/SPEC.md`: exactly one hit must exist, and its comment must say "# §10, §20.0; '' = the workspace root, relative = under the root, absolute = as is; one folder for every workspace (ADR 0108)" (add the missing words if Task 7 wrote less).
Anchor for the comment-less case: if Task 7's line has no `ADR 0108`, append ` (ADR 0108)` before the end of the comment.

**Section 14 (visual design system).**

(a) Token table.
After the row (line 1719):

```markdown
| `terminal-bg` | #0B0C0E | #FCFCFD |
```

insert:

```markdown
| `pane-gap` (the frame behind and between panes, and the title bar) | #000000 | #F3F4F6 |
```

(b) Line 1710, anchor (exact): `` | `chrome` / `sidebar` | #0F1013 | #F7F7F8 | `` stays (the sidebar keeps it).
Add after the table's last row `qr-light` (line 1735) a new paragraph line, one blank line after the table:

```markdown
`pane-gap` is darker than `terminal-bg` in dark themes when `terminal-bg` leaves room, a step darker in light themes, and always differs from `terminal-bg` by at least 1.06:1 (`theme::problems()` rule "pane gap"); a generated theme derives it with a mix from `terminal-bg` toward black or `text`.
```

(c) Line 1753, anchor (exact substring): `title bar 42 (it holds the tabs, ADR 0106), the Editor's file-tab strip and the right panel tab row 44`
Replace with: `title bar 42 (it holds the tabs, ADR 0106), the right panel tab row 44`.

(d) Lines 1756-1759, anchor (exact):

```markdown
- Pane (ADR 0106): no header.
  A pane in a split tab has a `PANE_GRIP_HEIGHT` (8 px) strip along its top edge that shows a grip on hover and drags the pane.
  Floating badges (`pane_badges`: `surface_raised` fill, 1 px `border`, radius 8, META text) sit in its top-right corner `space::S` from the edges, at most `PANE_BADGE_MAX_WIDTH` wide.
  A colored pane has a `PANE_ACCENT_WIDTH` (2 px) line of its color at the top.
```

Replace with:

```markdown
- Pane (ADR 0106, ADR 0108): no header and no border.
  Panes sit on a frame of `pane-gap` with `2 * PANE_HALF_GAP` (8 px) between them and from the edge of the area (`PANE_HALF_GAP = space::XS`); corners are `radius::MEDIUM` (8).
  One `PANE_ACCENT_WIDTH` (2 px) stripe sits at the pane's top edge, inset by the corner radius.
  Its color, first match wins: `status-needs-input` while a session pane needs input, the pane's own color, `pane-focus-stripe` while the pane is focused and its tab has two or more panes, else no stripe.
  A lone pane has no stripe unless it needs input or has a color.
  A pane that is not focused is dimmed by about 5 % (`unfocused_pane_dim`).
  File and diff panes use the same frame, stripe and dim as terminal panes.
  The gap between two panes drags to resize them and shows a grip only on hover.
  A pane in a split tab has a `PANE_GRIP_HEIGHT` (8 px) strip along its top edge that shows a grip on hover and drags the pane.
  Floating badges (`pane_badges`: `surface_raised` fill, 1 px `border`, radius 8, META text) sit in its top-right corner `space::S` from the edges, at most `PANE_BADGE_MAX_WIDTH` wide.
  The title bar uses `pane-gap` instead of `chrome`, so the bar and the frame read as one surface; the sidebar and the right panel keep their colors.
  The `+` row (§10) is a row of 30 px `Look::Icon` buttons: the `+` button, then the default agent's logo, `square-terminal`, `notebook-pen` and `chevron-down`, `space::XS` apart, growing in 150 ms ease-out (shown at once with reduce motion).
```

(e) Line 1781, anchor (exact substring): `a lone pane has no border, borders mark the focused split pane and needs-input.`
Replace with: `a pane has no border; the top stripe marks needs-input, a color and the focused split pane.`

(f) Line 1858, anchor (exact): `- Focused pane: faint 1 px accent border; unfocused panes dimmed ~5%.`
Replace with: `- Focused pane: a 2 px stripe of \`pane-focus-stripe\` at its top edge when its tab has two or more panes (needs-input and a pane color win); unfocused panes dimmed ~5%.`

(g) Line 1861, anchor (exact substring): `` `pane-focus-border` #49518D / #A4AAE7; ``
Replace with: `` `pane-focus-stripe` #49518D / #A4AAE7 (also the focus border of cards, comments and inputs); ``.

(h) Principle 2 (lines 1689-1692), anchor (exact substring): `a 3 px stripe on its sidebar card, and a 2 px line of its color at the top` + newline + `   of a colored pane.`
Leave it as it is (a colored pane still has that stripe).

**Section 13 (keyboard shortcuts).**

(a) Line 1489-1490, anchor (exact):

```markdown
Editor-only commands may use plain `Ctrl+<letter>` (no terminal has focus there, ADR 0028); the
editor's own keys (arrows, Ctrl+Z/Y/A/C/X/V) are not in the keymap.
```

Replace with:

```markdown
Editor-only commands may use plain `Ctrl+<letter>` (ADR 0028): the Editor context is set on a file pane's own element, so no terminal has the keyboard while it applies (ADR 0108); the
editor's own keys (arrows, Ctrl+Z/Y/A/C/X/V) are not in the keymap.
```

(b) Lines 1541-1543, anchors (exact), replace the two rows `Show diff` and `Show editor`:

```markdown
| View | Show diff (only while a diff is open) | Ctrl+Shift+7 |
| View | Show file tree (right panel, the tree gets the keyboard; ADR 0081) | Ctrl+Shift+3 |
| View | Show editor (only while a file tab is open) | Ctrl+Shift+6 |
```

with:

```markdown
| View | Show the diff: opens the Diff tab after the active tab, or focuses the Diff pane where it is open (ADR 0108) | Ctrl+Shift+7 |
| View | Show file tree (right panel, the tree gets the keyboard; ADR 0081) | Ctrl+Shift+3 |
```

(Ctrl+Shift+6 is unbound now.)

(c) Lines 1552-1555, anchors (exact), replace the four `Tabs` rows:

```markdown
| Tabs | Next / previous tab (file tabs while the Editor is shown) | Ctrl+Tab / Ctrl+Shift+Tab |
| Tabs | Jump to tab 1–9 (file tabs while the Editor is shown; 9 = the last) | Ctrl+Alt+1 … Ctrl+Alt+9 |
| Tabs | Rename / close tab (close: the active file tab while the Editor is shown) | Ctrl+Shift+R / Ctrl+Shift+W |
| Tabs | Move the active tab (the active file tab in the Editor) one place left / right inside its pinned or unpinned group | Ctrl+Shift+PgUp / Ctrl+Shift+PgDn |
```

with:

```markdown
| Tabs | Next / previous tab (file and diff tabs are tabs like the others) | Ctrl+Tab / Ctrl+Shift+Tab |
| Tabs | Jump to tab 1–9 (9 = the last) | Ctrl+Alt+1 … Ctrl+Alt+9 |
| Tabs | Rename / close tab (a tab with unsaved files asks Save / Don't save / Cancel) | Ctrl+Shift+R / Ctrl+Shift+W |
| Tabs | Move the active tab one place left / right inside its pinned or unpinned group | Ctrl+Shift+PgUp / Ctrl+Shift+PgDn |
| Tabs | New note: creates a note file in the notes folder (§20.0 `[notes]`) and opens it in a new tab (ADR 0108) | Ctrl+Alt+N |
```

(The key-column text is the Task 8 default; keep it equal to `keymap.rs`.)

(d) Line 1560, anchor (exact substring): `the title bar `+ ▾` menu and Ctrl+Shift+A / Ctrl+Shift+T create tabs`
Replace with: ``the title bar's `+` row and Ctrl+Shift+A / Ctrl+Shift+T create tabs``.

(e) Line 1561, anchor (exact substring): `(or from Board, Docs, Editor, Diff)`
Replace with: `(or from Board or Docs)`.
Also in the same row replace the substring `Move focus;` with `Move focus (file and diff panes count as panes);`.

(f) Line 1585, anchor (exact): `| Editor | Close the active file tab (asks about unsaved text; ADR 0080) | Ctrl+W |`
Delete the whole row (the command `close_file_tab` is gone; Ctrl+Shift+W closes the tab, Ctrl+Shift+X closes the focused pane).

(g) macOS table, lines 1621 and 1624, anchors (exact):

```markdown
| Terminals, Board, File tree, Git, Docs, Editor, Diff | ⌃⌘1 … ⌃⌘7 |
```

replace with:

```markdown
| Terminals, Board, File tree, Git, Docs / Diff | ⌃⌘1 … ⌃⌘5 / ⌃⌘7 |
```

and

```markdown
| Editor: save / find / replace / close tab / preview | ⌘S / ⌥⇧⌘F / ⌥⌘F / ⌘W / ⇧⌘V |
```

with

```markdown
| Editor: save / find / replace / preview | ⌘S / ⌥⇧⌘F / ⌥⌘F / ⇧⌘V |
```

and after the row `| Move tab left / right / to a new window | ⌃⌘← / ⌃⌘→ / ⇧⌘N |` add:

```markdown
| New note | ⌥⌘N |
```

(h) Line 1633, anchor (exact substring): `In the Editor, and while the Git list`
Replace with: `In a file pane, and while the Git list`.

**Section 12B (Git and the Diff pane).**

(a) Line 1393 and ToC line 22: replace `## §12B UI: Git (right panel tab and Diff view)` with `## §12B UI: Git (right panel tab and Diff pane)`, and `- §12B UI: Git (right panel tab and Diff view)` with `- §12B UI: Git (right panel tab and Diff pane)`.

(b) Lines 1396-1397, anchor (exact):

```markdown
The `Diff` view holds the toolbar, unified / split, hunks, the comment margin and Send to agent, and has a chip in the title bar's tab area only while a diff is open.
A click or Enter opens it; `×`, `Esc` or an emptied list close it; a selection moving by itself keeps it open.
```

Replace with:

```markdown
The Diff pane (ADR 0108) holds the toolbar (file, Unified / Split, Wrap, Viewed, Open in Zed, Discard, Stage file, `…`, ✕), unified / split, hunks, the comment margin and Send to agent.
It is a leaf of a tab's split tree like a file: a tab of its own in the title bar (diff icon, `Diff`) or a pane next to terminals and files.
A workspace has at most one Diff pane, across all its tabs and windows; it shows the diff of the Git list's selected file.
A click or Enter on a file of the Git list opens the Diff pane after the active tab, or focuses it where it is open; ↑ / ↓ in the list move it; `Ctrl+Shift+7` and the palette's "View: Diff" open or focus it.
The pane's ✕ or the tab's × closes it; `Esc` in the list unselects the picked lines and does not close it.
With nothing changed the pane shows the Diff empty state and stays.
A Diff pane in a detached window and one in the main window never share scroll or layout state.
```

(c) Line 1408, anchor (exact substring): `and when its tab or the Diff view comes on screen`
Replace with: `and when its tab or a Diff pane comes on screen`.

(d) Line 1410, anchor (exact substring): `Never run git for workspaces whose Git tab or Diff view is not on screen`
Replace with: `Never run git for workspaces whose Git tab or Diff pane is not on screen`.

(e) Line 1030 (§10 Right panel), anchor (exact substring): `a file's diff shows in the Diff view (§12B).`
Replace with: `a file's diff shows in the Diff pane (§12B).`

**Section 12A (Files).**

(a) ToC line 21 and heading line 1285: `UI: Files (tree and Editor)` becomes `UI: Files (tree and file pane)`.

(b) Lines 1294-1297, anchor (exact):

```markdown
- **Editor view:** shown by the Editor chip in the title bar's tab area only while a file tab is open. A tree click or Enter,
  the palette, a terminal link and "New file…" show it (a tree click keeps the keyboard in the
  tree); `Ctrl+Shift+6` shows it again. Closing the last tab, or switching to another worktree,
  shows Terminals.
```

Replace with:

```markdown
- **File pane** (ADR 0108, replaces the Editor view): an open file is a leaf of a terminal tab's split tree.
  It is a tab in the title bar's tab area (file-type icon, file name, `●` while unsaved, × to close) or a pane inside the tab in front, inside the same frame, stripe and dim as a terminal pane (§14).
  There is no Editor view, no Editor chip and no file tab strip.
  A workspace has at most one pane per file across all its tabs and windows: opening a file that is open focuses its pane (its tab and window come to the front) and jumps to the line.
  A single click on a file in the tree opens the workspace's **preview tab** (italic title), which the next single click replaces while it is clean; a double click, Enter in the tree, Alt+Enter, an edit, Pin or a drag keeps it.
  A tree click keeps the keyboard in the tree.
  Every other open (palette, a terminal link, a Markdown link, a chat path pill, "New file…", Search hits) opens a kept tab right after the active tab.
  Dragging a tree row onto a pane opens the file as a split at that edge (§10 "Drops").
  The open files are saved in the tab layout (`{"file": "src/a.rs"}`, plus `md` and `preview`) and come back after a restart in the same place; their text is read when the pane is first drawn.
  A file deleted on disk shows "This file was removed." with a Close button.
```

(c) Line 1298, anchor (exact substring): `- **Editor** (the center Editor view; ADR 0028):`
Replace with: `- **Editor** (the text editor of a file pane; ADR 0028):`.

(d) Lines 1306-1317, the whole `- **Tabs** (ADR 0080): ...` bullet, anchor starts with `- **Tabs** (ADR 0080): every opened file has a tab above the header (pinned tabs first; the` and ends with `live in memory only; another worktree starts with none (and shows Terminals).`
Replace the whole bullet with:

```markdown
- **Closing and tabs** (ADR 0108, replaces the file tab strip of ADR 0080): a file's tab is a tab of the title bar (§10), so pinning, reordering, Ctrl+Tab, Ctrl+Alt+1-9, Rename, Close, Close others, Close to the right and the tab menu work as for terminal tabs.
  A pane's file is closed with the tab's ×, the tab menu's Close pane, `Ctrl+Shift+X` or a middle click on its tab; each asks "Save / Don't save / Cancel" first for a file with unsaved text (Cancel closes nothing).
  The window's find bar belongs to the shown file pane: one find bar per file pane, searched on that file.
  `git.changed` reads the files of the shown panes; files of tabs that are not shown are read when shown.
  Moving a file (drag to an edge, to another tab, tear-off, merge) never asks.
  Another worktree keeps its own tabs; a file of the old worktree stays in its tab and still opens its own folder.
```

Check: the sentence "Another worktree keeps its own tabs" must match what Task 4 does with `set_files_target`.
If Task 4 left "switching worktree closes file leaves", write that instead and mention the ask; read `crates/mkrew-ui/src/shell/files.rs::set_files_target` first.

(e) Lines 1326-1328, anchor (exact):

```markdown
  "Reload" (drop your changes) or "Keep mine" (Save overwrites it). Closing a tab, switching
  to another worktree or closing the window with unsaved changes asks "Save / Don't save /
  Cancel" (once per unsaved tab). Switching workspace or view keeps them.
```

Replace with:

```markdown
  "Reload" (drop your changes) or "Keep mine" (Save overwrites it). Closing a pane, a tab, a window or a workspace, switching
  to another worktree or quitting with unsaved changes asks "Save / Don't save /
  Cancel" (once per unsaved file). Switching workspace or tab keeps them; a preview tab with unsaved edits is kept, not replaced.
```

(f) Lines 1349-1352, anchors (exact substrings): `(Editor and Docs, ADR 0092)` stays; `relative path opens that file of the folder in the Editor (` replace with `relative path opens that file of the folder in a file pane (`.

(g) Lines 1356-1359 (`another tab (or the same file opened again) starts at the top`): replace `another tab (or the same file` with `another file pane (or the same file`.

(h) Lines 1381-1384, anchors (exact):

```markdown
  Shift+F10 (⌃⌘M on macOS) opens it on the cursor row (on the active tab's menu while the editor has the keyboard).
- **Open at a line:** Ctrl+Click on a file path in a terminal (§10) opens that file in the Editor with the
  caret on the line, scrolled into view. A path in another task's worktree switches to
  that worktree (asking about unsaved text first).
```

Replace with:

```markdown
  Shift+F10 (⌃⌘M on macOS) opens it on the cursor row (on the active tab's menu while a file pane has the keyboard).
- **Open at a line:** Ctrl+Click on a file path in a terminal (§10) opens that file in a file pane (a new tab, or the pane that already shows it) with the
  caret on the line, scrolled into view. A path in another task's worktree switches to
  that worktree (asking about unsaved text first).
```

(i) Notes (`New note`).
Append a new bullet at the end of §12A (after the Find & replace bullet, which ends with `  editor.`, line 1391):

```markdown
- **New note** (ADR 0108): the `+` row's note button, `Ctrl+Alt+N` (⌥⌘N) and the palette's "New note" create a Markdown file in the notes folder and open it in a new tab, kept, in source mode, with the caret in the editor.
  The daemon makes it (`fs.new_note`, §7): the folder is `[notes] folder` of the daemon's own `settings.toml` (§20.0; empty = the workspace root, relative = under the root, absolute = as is), the name is `note-YYYYMMDD-HHMMSS.md` in local time, `-2`, `-3`, ... is added while the name exists, missing folders are created, the file starts empty.
  A note inside the root is stored in the layout as a root-relative path, a note in an absolute folder as its absolute path.
  Dragging the note button onto a pane creates the note and opens it at that edge.
  An error (no write access, disk full, a folder that cannot be made) shows a toast with the reason and opens nothing.
  If the notes folder setting changes while a note from the old folder is open, the pane shows the read error with its reason and Close.
```

**Section 11 (per-session status).**

(a) Line 1181, anchor (exact): `| needs_input | ? | highlighted pane border, counted in "Needs you" |`
Replace with: `| needs_input | ? | \`status-needs-input\` stripe at the pane's top edge, counted in "Needs you" |`.

(b) Lines 1196-1200, after the line `A double-click on a default-named tab renames the focused pane, on a named tab the tab.` (line 1200) insert:

```markdown
  The focused pane decides the chip's title and icon for every kind of leaf (ADR 0108): a session as above; a file shows its file-type icon and file name, `●` while unsaved, and an italic title while it is the preview tab; the Diff pane shows the diff icon and `Diff`.
  Only a session pane can be renamed: F2 and a double-click on a tab whose focused pane is a file or the diff rename the tab.
```

(c) Lines 1203-1204, anchor (exact):

```markdown
  The active tab has a solid card fill (the Tern chip look); in a split the panes keep only their thin
  border, the unfocused ones are dimmed. The sidebar session row also puts the badge
```

Replace with:

```markdown
  The active tab has a solid card fill (the Tern chip look); in a split the focused pane has a stripe
  at its top edge (§14), the unfocused ones are dimmed. The sidebar session row also puts the badge
```

(d) Line 1215, anchor (exact substring): `opens the file in the Editor view at its `:line`)`
Replace with: `opens the file in a file pane at its `:line`)`.

**Section 10 (UI: layout, workspaces and terminals).**
Edit bottom-up.

(a) Line 1166, anchor (exact): `- File tabs stay in the main window (until the Editor part of the spec ships).`
Replace with:

```markdown
- File and diff tabs tear off like terminal tabs (ADR 0108): a detached window draws file and diff panes from the workspace's store, one pane per file across all windows; opening a file that is open in another window focuses it there and raises that window.
```

(b) Line 1161, anchor (exact substring): `it has no workspace switcher, `+ ▾`, Board, Docs, Editor and Diff chips, Run pill or limits chip.`
Replace with: ``it has no workspace switcher, Board, Docs, Run pill or limits chip; it has the `+` row (a note goes to the window's workspace).``

(c) Line 1085-1086, anchor (exact substring): `it opens in the Editor (§12A) with the caret on that line.`
Replace with: `it opens in a file pane (§12A) with the caret on that line.`

(d) Lines 1028-1029, anchor (exact substring): `tab's focused pane, also while the Editor is shown; shown only while`
Replace with: `tab's focused pane (nothing for a file or diff pane); shown only while`.

(e) Line 1021, anchor (exact substring): `a column right of the Terminals, Editor and Diff views with`
Replace with: `a column right of the Terminals view (the tab's panes) with`.

(f) Lines 1002-1006, anchor (exact):

```markdown
  Drag a tab onto a pane of the shown tab, or a pane by the grip on its top edge (an 8 px strip that shows a grip on hover; only in a split tab) onto
  another pane, to split that pane: the half under the pointer lights up (the right half when
  the pointer is further right than down, else the bottom half) and the drop puts the dragged
  panes there, a tab keeping its layout. A pane moves within its tab or out of another one; an
  emptied tab closes after both layouts are saved (`tab.set_layout`, then `tab.close`).
```

Replace with:

```markdown
  **Drops** (ADR 0108): a pane has four drop zones, one per edge, and no zone in the middle: the edge nearest the pointer wins (ties: right, then bottom).
  The half of the pane that the new leaf will take lights up (the accent tint), and the drop splits the pane 50 / 50 there.
  Every draggable uses them: a file row of the Files tree (opens the file), a `+` row item (starts that agent, shell or note), a tab (merges its panes, keeping its layout) and a pane by the grip on its top edge (an 8 px strip that shows a grip on hover; only in a split tab), which moves the whole leaf, of any kind.
  A pane or tab dropped on the empty part of the tab area becomes a tab of its own; a file row or a `+` row item dropped there becomes a new tab after the last one.
  A pane dropped on itself, or a tab on the tab that holds only that pane, does nothing; nothing drops while a dialog is open.
  A file row dropped on an agent pane's composer inserts the path (§20B.5) and is not accepted by any terminal.
  A pane moves within its tab or out of another one; an emptied tab closes after both layouts are saved (`tab.set_layout`, then `tab.close`).
  OS files dropped on a terminal pane type their paths (§20B.5); on a file or diff pane they open as new tabs.
```

(g) Lines 981 and 980: line 974 starts the Terminals bullet.
Anchor, line 974 (exact):

```markdown
- **Terminals:** the tabs are in the title bar (above); each tab is a split tree of panes; each pane is a session with no header of its own (ADR 0106): the terminal fills it.
```

Replace with:

```markdown
- **Terminals:** the tabs are in the title bar (above); each tab is a split tree of panes; each pane is a session, a file or the diff (ADR 0108) with no header of its own (ADR 0106): a terminal fills its pane, a file pane shows the editor (§12A) and the diff pane the diff (§12B).
  A file or the diff opens as a tab right after the active tab, or as a pane when dropped on a pane (below).
  Panes sit on a frame with an 8 px gap and no border; a stripe at the top edge marks the focused pane, a pane that needs input or a pane color (§14).
  Each file and the diff exist at most once per workspace, in one pane across all tabs and windows.
```

Anchor, line 980-981 (exact):

```markdown
  The chip's tooltip says the status in words.
  `+ ▾` (new tab) follows the tabs.
```

Replace with:

```markdown
  The chip's tooltip says the status in words.
  A file or diff pane adds its file-type icon or the diff icon (in `text-muted`) to the pane icons, and the tab's title follows the focused pane (§11).
  The `+` row (below) follows the tabs.
```

Line 977 (exact substring): `a tab without panes shows none.` stays.

(h) Lines 922-923, anchors (exact):

```markdown
  - **`+ ▾`:** one row per installed agent profile (the workspace's default first with ✓ and Ctrl+Shift+A), "Agent" when none is listed, Shell (Ctrl+Shift+T), Empty tab; each row opens a new tab and shows Terminals; while a session starts the button shows the starting spinner.
    Alt+Shift+A and Alt+Shift+S add a pane to the current tab instead.
```

Replace with:

```markdown
  - **`+` row** (ADR 0108): the button is a `+`.
    On hover or keyboard focus it grows to the right, on the same line, in 150 ms ease-out (at once with reduce motion), into `[+] [default agent] [default terminal] [new note] [▾]`.
    It collapses 150 ms after the pointer leaves both the `+` and the row, so the pointer can travel to the items.
    The items are the default agent profile's logo (tooltip "New <agent> tab" with Ctrl+Shift+A; hidden when no profile is installed), `square-terminal` (tooltip "New <shell> tab" with Ctrl+Shift+T; the workspace's default shell), `notebook-pen` (tooltip "New note" with Ctrl+Alt+N) and `▾`.
    A click on an agent, shell or note item opens a new tab right after the active one; a drag starts the same thing as a leaf at the drop edge of a pane (see Drops below) or a new tab on the tab area.
    `▾` (tooltip "More…") opens the menu of before: one row per installed agent profile (the workspace's default first with ✓ and Ctrl+Shift+A), "Agent" when none is listed, Shell (Ctrl+Shift+T), Empty tab; each row opens a new tab; it is not draggable.
    Keyboard: Tab reaches `+`, ← / → move between the items, Enter picks, Esc collapses.
    While a session starts the row shows the starting spinner (loading dots) instead.
    Every item is a `Look::Icon` button with a focus ring.
    Alt+Shift+A and Alt+Shift+S add a pane to the current tab instead.
```

(i) Line 918, anchor (exact):

```markdown
  - **Tab area:** the Editor and Diff chips (icon, name, × that closes all file tabs or the diff; only while open; a click shows the view), a 1 px rule, the pinned tabs, a rule, the other tabs, and `+ ▾` (below).
    The area scrolls sideways when the tabs do not fit; the active tab is scrolled into view; a chip shrinks to `TAB_MIN_WIDTH` before the area scrolls, `TAB_TITLE_MAX_WIDTH` is its maximum; `+ ▾` stays visible after the scrolled tabs.
```

Replace with:

```markdown
  - **Tab area:** the pinned tabs, a 1 px rule, the other tabs (terminal tabs, file tabs and the Diff tab alike), and the `+` row (below).
    The area scrolls sideways when the tabs do not fit; the active tab is scrolled into view; a chip shrinks to `TAB_MIN_WIDTH` before the area scrolls, `TAB_TITLE_MAX_WIDTH` is its maximum; the `+` row stays visible after the scrolled tabs.
```

and the next line 921 (exact substring): `a click activates the tab and shows Terminals.` stays.

(j) Line 916, anchor (exact substring): `between `+ ▾` and the drag space`
Replace with: ``between the `+` row and the drag space``.

(k) Line 912, anchor (exact):

```markdown
- **Views per workspace:** Terminals (a click on any terminal tab), Board and Docs (icons in the title bar, only with `[features] docs` for Docs; a second click returns to Terminals), Editor (a chip, only while a file tab is open), Diff (a chip, only while a diff is open).
```

Replace with:

```markdown
- **Views per workspace:** Terminals (a click on any tab: terminal, file or diff), Board and Docs (icons in the title bar, only with `[features] docs` for Docs; a second click returns to Terminals).
  There is no Editor view and no Diff view: files and the diff are tabs and panes of Terminals (ADR 0108).
```

(l) Lines 840-841, anchor (exact):

```markdown
The right panel (below) is a third column right of Terminals, the Editor and the Diff while open.
Its tab row is `[Files│Session│Git]`; `Editor*` and `Diff*` are chips in the title bar's tab area only while a file tab or a diff is open.
```

Replace with:

```markdown
The right panel (below) is a third column right of Terminals.
Its tab row is `[Files│Session│Git]`; files and the diff open as tabs and panes of Terminals (ADR 0108), so the title bar has no Editor or Diff chip.
```

(m) Line 825 (the mock-up).
Anchor (exact substring): `│ ◧ A api ▾ │[Editor*][Diff*]│ [▣ ● tab ✕] [>_ ○ tab ✕] +▾   ⚠ Claude 5h 84%`
Replace with: `│ ◧ A api ▾ │ [▣ ● tab ✕] [>_ ○ tab ✕] [▤ a.rs ● ✕] [± Diff ✕] +  ⚠ Claude 5h 84%`
(keep the rest of the line, `(● 2 running ▾) ▦ ▤ ◫ ◨ ─ □ ✕ │`, as it is; if the box border column shifts, pad with spaces so the right `│` stays under the others).
In the same block, line 827 (exact substring) `│ ┌ the terminal pane, no header ──────────── ⤢ Pane zoomed ┐              │` stays.

**Section 7 (IPC protocol).**

(a) Line 601, anchor (exact substring): `close deletes the tab and removes every session with a pane in it, like `session.remove`;`
Replace with: ``close deletes the tab and removes every session with a session leaf in it (file and diff leaves have nothing to remove), like `session.remove`;``

(b) Line 603, anchor (exact): `| `tab.set_layout` | `tab_id`, `layout` | ok |`
Replace with: ``| `tab.set_layout` | `tab_id`, `layout` | ok (the daemon stores the JSON as given and validates nothing; leaves are sessions, files or the diff, §5; `cut` and `layout_sessions` read only session leaves) |``

The UI, not the daemon, drops a leaf it cannot parse (with a warning) when it reads a layout (§5).

(c) Line 612, anchor (exact):

```markdown
| `fs.read` | `project_id`, `path`, `index_text?` (bool) | content (≤ 2 MB), binary flag, language, `hash?` (opaque, set with `content`), `index_text?` (asked for: the file's text in the git index, ADR 0047) |
```

Replace with:

```markdown
| `fs.read` | `project_id`, `path`, `index_text?` (bool) | content (≤ 2 MB), binary flag, language, `hash?` (opaque, set with `content`), `index_text?` (asked for: the file's text in the git index, ADR 0047); `path` is relative to the root, or an absolute path inside the daemon's absolute notes folder (`[notes] folder`, normalized, no `..`; ADR 0108); any other absolute path is refused as before |
```

(d) Line 613 (`fs.write`): do not rewrite the line (it holds an em dash); append to its end, directly after `` `hash` = the disk's (ADR 0028) `` and before the closing ` |`, the text:

```markdown
; `path` follows the same rule as `fs.read`: root-relative, or absolute inside the notes folder (ADR 0108)
```

(e) After line 614 (the `fs.create` row) insert the new row:

```markdown
| `fs.new_note` | `project_id`, `worktree_task_id?` | `FsCreated { path }` - creates an empty `note-YYYYMMDD-HHMMSS.md` (local time; `-2`, `-3`, ... while the name exists, so `already_exists` is never answered) in the daemon's `[notes] folder` (§20.0: empty = the root of the project or task worktree, relative = under it, absolute = as is), making missing folders; `path` is relative to the root when the note is inside it, else absolute. `invalid_request` (the message names the reason) for a bad folder setting (`..`, `~`, an unusable drive, not a folder) and for a create failure (the OS message: no write access, disk full); `internal` when the blocking task fails to join; nothing is created on an error (ADR 0108) |
```


(f) After line 708 (`Protocol 58 (ADR 0106): ...`) and after a `Protocol 59` line if `feat/omp-tsp` has merged (Step 1), insert one blank line and:

```markdown
Protocol 60 (ADR 0108): a tab layout's leaves can be files (`{"file": "..."}`) and the diff (`{"diff": true}`) as well as sessions; request `fs.new_note`; `fs.read` and `fs.write` accept an absolute path inside the daemon's notes folder.
```

**Section 5 (data model).**

(a) Line 283, anchor (exact): `  layout     TEXT NOT NULL               -- JSON split tree, leaves are session ids`
Replace with: `  layout     TEXT NOT NULL               -- JSON split tree; leaves are sessions, files or the diff (ADR 0108)`

(b) Lines 346-355, anchor (exact):

```markdown
Split-tree example (`terminal_tab.layout`): vertical split, right half split horizontally.
`vertical` = children side by side (first left), `horizontal` = stacked (first on top); `ratio`
is the first child's share; every split has exactly two children; `null` = a tab without panes.

```json
{"split":"vertical","ratio":0.5,"children":[
  {"session":"01JA..."},
  {"split":"horizontal","ratio":0.6,"children":[{"session":"01JB..."},{"session":"01JC..."}]}
]}
```
```

Replace the first three lines (keep the json block) with:

```markdown
Split-tree example (`terminal_tab.layout`): vertical split, right half split horizontally.
`vertical` = children side by side (first left), `horizontal` = stacked (first on top); `ratio`
is the first child's share; every split has exactly two children; `null` = a tab without panes.
Leaves are sessions, files or the diff (ADR 0108): `{"session": "<ulid>", "chat": true}` (`chat` optional), `{"file": "src/a.rs", "md": "split", "preview": true}` (`md` and `preview` optional; the path is relative to the workspace root with `/`, or absolute for a note in an absolute notes folder) and `{"diff": true}`.
A key (a session, a file path, the diff) appears at most once in a tab, and a file or the diff at most once per workspace.
The daemon reads only the leaves with a `session` key (`cut`, `layout_sessions`) and keeps the others as they are; a leaf the UI cannot parse is dropped with a warning and the rest of the tab loads.
```

(c) ToC: no other line.

**Line 67 (section 1 limits)**: anchor (exact substring) `The Editor view has a plain editor (ADR 0028)`; replace with `A file pane has a plain editor (ADR 0028)`.

After all SPEC edits run the checks of Step 7 (grep "Editor view|Editor chip|Diff chip|Diff view|file tab" in SPEC must list only lines that talk about history, e.g. none expected):

```bash
git grep -nE "Editor view|Editor chip|Diff chip|Diff view|file tab strip|file-tab strip|\+ ▾|pane-focus-border|pane_focus_border" -- docs/SPEC.md
```

Expected: no output, except `+ ▾` inside the Split ▾ text (none) and the ADR-0106 sentence on line 839 that says "ADR 0106 removed it again" (it has no `+ ▾`).
Every other hit is a missed anchor: fix it.
Also run `git grep -nE "file tab|file tabs" -- docs/SPEC.md`: remaining hits must be about file tabs as ordinary tabs (§10, §12A, §13 as written above).

- [ ] **Step 4: `docs/HUMAN_CHECKS.md`**

(a) Insert before line 24 (`### To verify - 2026-11-23 Tern layout ...`) this whole section, then one blank line (newest first, like the file's order):

````markdown
### To verify - 2026-11-23 Tern panes (ADR 0108, plan docs/superpowers/plans/2026-11-23-tern-panes.md)

Screenshots: `docs/screenshots/tern-panes-<view>-<dark|light>.png`.

- [ ] **Restart the daemon first** (protocol 60; a window from before it shows a version mismatch).
  Open the app on an existing workspace: the tabs from before the update load unchanged.
- [ ] **Pane look (dark + light, Mkrew Light, Classic and one palette theme):** panes have no border, they sit on a darker frame (a little lighter in light themes) with an 8 px gap between them and from the edge, and the title bar has the same color as the frame.
  The focused pane has a 2 px stripe along its top edge only while the tab has two or more panes; a lone pane has none.
  A pane that needs input has an amber stripe even when it is not focused; a colored pane keeps its color; amber wins over color, color wins over focus.
  Unfocused panes are dimmed a little.
  The sidebar and the right panel keep their colors.
- [ ] **Four panes:** one tab with an agent, a shell, a `.rs` file and a Markdown file in split mode, one pane focused and one (the fake agent) needing input.
  All four use the same frame, stripe and dim; the file panes keep header, gutter, find (Ctrl+F), save (Ctrl+S) and Markdown buttons; Ctrl+S saves the focused file pane only.
- [ ] **No Editor or Diff view:** the title bar has no Editor or Diff chip, no view switch; a click on a file in the tree opens a tab with its icon and name; `Ctrl+Shift+6` does nothing; `Ctrl+W` does nothing in a file pane (use the tab's × or `Ctrl+Shift+W`).
- [ ] **Preview tab:** a single click on a file in the tree opens an italic tab; the next single click on another file replaces it; a double click, Alt+Enter, typing in it or dragging it keeps it (the title turns upright); an edited preview tab is not replaced.
- [ ] **One pane per file:** open a file, move to another tab and open the same file again from the tree, the palette (Ctrl+Shift+O), a terminal link with `:line` and a chat path pill: each time its own tab comes up, the pane is focused and the line is shown, and there is never a second pane; the same holds when the file is in a detached window (that window comes to the front).
- [ ] **Drops on four edges (dark + light):** drag a file row of the tree over a terminal pane, a file pane and the diff pane: the half under the pointer lights up for the nearest edge (left, right, top, bottom; near a corner the nearest wins), and a drop splits there 50 / 50.
  Drag a tab onto a pane edge: its panes merge there.
  Drag a pane by its top-edge grip onto another pane's edge: it moves, whatever its kind (terminal, file, diff).
  Drag a pane or a tab onto the empty part of the tab area: it becomes its own tab; drag a file row there: a new tab after the last one.
  A pane dropped on itself, or a tab on the tab that holds only that pane, does nothing; with a dialog open nothing drops.
  Dropping a tree row on a terminal does not type its path; dropped on an agent's open composer it inserts the path as before; a folder row is accepted only by a composer.
- [ ] **OS files:** drop a file from Explorer on a terminal pane: the path is typed as before.
  Drop it on a file pane or the diff pane: it opens as a new tab if it is inside the workspace or the notes folder.
  Drop one from outside both (Desktop): one error toast "Only files inside the workspace or the notes folder open in Mkrew"; folders are skipped silently.
- [ ] **The `+` row (dark + light):** the tab area ends with a `+`; hover it: it grows to the right in about 150 ms into default agent logo, terminal icon, note icon and `▾`; move the pointer to an item without it collapsing; leave: it collapses after about 150 ms.
  Tooltips: "New <agent> tab" with the shortcut, "New <shell> tab" with the shortcut, "New note" with `Ctrl+Alt+N`, "More…".
  A click on each opens a new tab right after the active tab; `▾` opens the old menu (profiles, Shell, Empty tab).
  Drag each item onto an edge of a pane: it starts there as a split; drag onto the tab area: a new tab.
  With no agent profile the agent item is hidden.
  A detached window has the same row.
- [ ] **`+` row by keyboard:** Tab from the tab chips reaches `+` and it expands with a focus ring; ← / → move between the items, Enter picks, Esc collapses it and keeps the focus; Shift+Tab leaves.
- [ ] **Reduce motion:** with Windows "Animation effects" off (or macOS "Reduce motion"), the row shows and hides at once.
- [ ] **New note:** with `[notes] folder` empty, the note button creates `note-YYYYMMDD-HHMMSS.md` (local time) in the workspace root and opens it in a new tab in source mode with the caret in the editor; two notes in the same second give `-2`; set the folder to `notes/inbox` (relative: made under the root, also nested) and to an absolute path outside the workspace such as `D:\Notes` (the note opens, its tab title is the note's name, saving works); then a path with `..` or `~` shows an error toast and creates nothing; a drive that does not exist shows an error toast and creates nothing.
  `Ctrl+Alt+N` and the palette's "New note" do the same.
  Change the folder setting while an old note from the previous absolute folder is open: its pane shows a read error with Close.
- [ ] **Notes on a remote machine:** in a machine window (`mkrew --machine <name>`) the note is created on that machine, in the folder of that machine's `settings.toml`; an absolute folder that exists only on this computer does not work there.
- [ ] **Diff pane:** select a changed file in the Git panel (`right_panel = "git"`): the Diff tab opens after the active tab with the toolbar; ↑ / ↓ in the list move the diff; drop the Diff tab's pane onto a terminal's edge: it is a pane there; `Ctrl+Shift+7` focuses it; commit everything: the pane shows the empty state and stays; its ✕ closes it; a comment on a diff line goes to the right session.
- [ ] **Close and the unsaved question:** edit a file, then close its tab (×), close the pane, close the tab menu's "Close others", close the window and close the workspace: each asks "Save / Don't save / Cancel" and Cancel closes nothing; quit the app: it asks too.
  Dragging an edited file pane to another edge, to another tab and out into a detached window never asks.
- [ ] **Restart:** with tabs holding a file, a note, a Markdown split and the diff, quit and restart the app (and, separately, `mk daemon stop` then start): every file and diff leaf is in the same tab and place, the files load when first shown, the Markdown mode is kept, and the preview tab comes back italic.
  Unsaved edits are not restored.
- [ ] **A session ends in a mixed tab:** exit the shell in a tab that has a shell and a file: the tab stays with the file focused.
  A tab with only a file does not close when you close a session elsewhere; closing it removes no session.
- [ ] **Detached file tab (dark + light):** drag a file tab out of the window: a window opens with the file pane (same frame and stripe); edit and save there; drag it back.
  A file tab does not stay in the main window any more.
- [ ] **Deleted file:** delete an open file on disk: its pane shows "This file was removed." with Close.
- [ ] **Contrast:** the gap color is visible against the terminal in every theme (the Settings page of a theme with a low-contrast `bg` shows no "pane gap" warning), dark and light.
````

(b) Under the Tern layout section heading (the line `Screenshots: \`docs/screenshots/tern-layout-<view>-<dark|light>.png\`.`, line 26 before the insert), after that line add one blank line and:

```markdown
Superseded in part by Tern panes (ADR 0108): the Editor and Diff chips, `+ ▾` and the thin pane borders of the items "Look", "Views", "`+ ▾`", "Many tabs" and "No pane header" are replaced; check those in the Tern panes section.
```

- [ ] **Step 5: `docs/PROGRESS.md` and `README.md`**

(a) `docs/PROGRESS.md`: insert a new line after line 32 (the line `## Notes for the next session` is line 31, a blank line 32) i.e. before the line that starts `- 2026-11-23 Tern layout (branch`, in the same format (one long line):

```markdown
- 2026-11-23 Tern panes (branch `feat/tern-panes`, spec docs/superpowers/specs/2026-11-23-tern-panes-design.md, plan docs/superpowers/plans/2026-11-23-tern-panes.md, ADR 0108, protocol 60, no migration): split tree leaves are typed (`Leaf::{Session, File, Diff}`, `PaneKey`, `Edge`, lenient `Node::from_json`) and `Tabs` works on keys (`insert_leaf`, `update_leaf`, `move_pane`, `merge_tab`, `close_leaf`, `preview_tab`, `position_after_active`); `documents.rs` (`Documents`, `Doc`, `Place`) replaces `FilesView.tabs`, `shell/doc_view.rs` holds the per-file gpui state, `shell/file_pane.rs` and `shell/diff_pane.rs` draw the panes inside `pane_frame`, `Shell::open_document` / `open_diff` are the only ways to open; `View::Editor`, `View::Diff`, the chips, the file tab strip, `ShowEditor` and `CloseFileTab` are gone; four-edge drops (`Edge`, `PaneDrop`, `take_pane_drop`, one overlay, `drop_leaf`) replace `drop_axis`; pane look (`pane_gap`, `pane_focus_stripe`, `status::pane_stripe`, gap 8, no border, `PANE_GAP_MIN_CONTRAST`); the `+` row with `DraggedNew`, `Command::NewNote` (`ctrl-alt-n`), `Icon::NotebookPen`; `[notes] folder`, `mkrew_core::notes`, `fs.new_note` and the notes-folder exception of `fs.read` / `fs.write`; daemon `cut` / `layout_sessions` skip file and diff leaves. Checks in HUMAN_CHECKS "2026-11-23 Tern panes"; screenshots `docs/screenshots/tern-panes-*.png`.
```

(b) `README.md` lines 191-192 and 197-199.
Anchor 1 (exact):

```markdown
- **Terminals** (`Ctrl+Shift+1`): tabs split into panes, each an agent or a shell. New agent tab
  `Ctrl+Shift+A`, shell tab `Ctrl+Shift+T`, split `Alt+Shift+=` / `Alt+Shift+-`.
```

Replace with:

```markdown
- **Terminals** (`Ctrl+Shift+1`): tabs split into panes, each an agent, a shell, a file or the diff. New agent tab
  `Ctrl+Shift+A`, shell tab `Ctrl+Shift+T`, new note `Ctrl+Alt+N`, split `Alt+Shift+=` / `Alt+Shift+-`.
  Hover the `+` after the tabs for the quick row (agent, shell, note, more).
  Drag a file, a tab, a pane or a `+` item onto any edge of a pane to split it there.
```

Anchor 2 (exact):

```markdown
- **File tree** (`Ctrl+Shift+3`, in the right panel), **Editor** (`Ctrl+Shift+6`) and **Git**
  (`Ctrl+Shift+4`): browse files, diff against the base
```

Replace with:

```markdown
- **File tree** (`Ctrl+Shift+3`, in the right panel), the **file pane** (a click opens a file as a tab, a drop on a pane opens it there) and **Git**
  (`Ctrl+Shift+4`, its diff opens as a tab or pane, `Ctrl+Shift+7`): browse files, diff against the base
```

Also run `grep -n -i "Editor view\|Editor chip\|Ctrl+Shift+6" README.md`; expected no output after the edit.

- [ ] **Step 6: Smoke on the real app and screenshots**

Build (from the worktree; `CARGO_TARGET_DIR` is the main checkout's `target`):

```bash
CARGO_BUILD_JOBS=4 cargo build -p mkrew-ui -p mkrew-cli -p mkrew-daemon -p mkrew-fakeagent --bins
```

Let `$target` be `C:\Users\NguyenTung\Workspace\PersonalProject\mkrew-build-kit\mkrew\target\debug` (where `mkrew.exe`, `mk.exe` and `mkrew-fakeagent.exe` now are).
Do not touch the installed Mkrew or any process you did not start.

**6.1 Capture script.** Save it outside the repo as `$env:TEMP\shot.ps1` (verbatim from the Tern layout plan, Task 7 Step 6):

```powershell
param([string]$Out, [int]$Width = 0, [int]$Height = 0, [string]$Mode = "print")
Add-Type -AssemblyName System.Drawing
Add-Type @"
using System; using System.Runtime.InteropServices;
public class W {
  [DllImport("user32.dll")] public static extern bool PrintWindow(IntPtr h, IntPtr dc, uint flags);
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
  [DllImport("user32.dll")] public static extern bool SetWindowPos(IntPtr h, IntPtr after, int x, int y, int w, int hh, uint f);
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
  public struct RECT { public int L, T, R, B; } }
"@
$p = Get-Process mkrew | Where-Object { $_.Path -like "*target\debug*" -and $_.MainWindowHandle -ne 0 } | Select-Object -First 1
if (-not $p) { Write-Output "no window"; exit 1 }
$h = $p.MainWindowHandle
if ($Width -gt 0) { [W]::SetWindowPos($h, [IntPtr]::Zero, 40, 40, $Width, $Height, 0x0004) | Out-Null; Start-Sleep -Milliseconds 800 }
[W]::SetForegroundWindow($h) | Out-Null; Start-Sleep -Milliseconds 400
$r = New-Object W+RECT; [W]::GetWindowRect($h, [ref]$r) | Out-Null
$w = $r.R - $r.L; $hh = $r.B - $r.T
$bmp = New-Object System.Drawing.Bitmap $w, $hh
$g = [System.Drawing.Graphics]::FromImage($bmp)
if ($Mode -eq "print") {
  # 2 = PW_RENDERFULLCONTENT (needed for DirectX windows).
  $dc = $g.GetHdc(); [W]::PrintWindow($h, $dc, 2) | Out-Null; $g.ReleaseHdc($dc)
} else {
  $g.CopyFromScreen($r.L, $r.T, 0, 0, $bmp.Size)
}
$bmp.Save($Out, [System.Drawing.Imaging.ImageFormat]::Png)
Write-Output "saved $w x $hh"
```

If a PrintWindow image is all black, run again with `-Mode screen` (the window must be on top and not covered).
A detached window has its own handle that `Get-Process` does not return: capture it by screen region (see 6.5).

**6.2 Mouse helper for drags.** Save as `$env:TEMP\hold.ps1`.
It presses, moves and releases the left button inside the Mkrew window, positions given as fractions of the window, so a screenshot can be taken (with `-Mode screen`) while the drag is held:

```powershell
param([ValidateSet("down","move","up")][string]$Phase, [double]$FromX = 0, [double]$FromY = 0, [double]$ToX = 0, [double]$ToY = 0)
Add-Type @"
using System; using System.Runtime.InteropServices;
public class M {
  [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
  [DllImport("user32.dll")] public static extern void mouse_event(uint f, uint dx, uint dy, uint d, UIntPtr e);
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
  public struct RECT { public int L, T, R, B; } }
"@
$p = Get-Process mkrew | Where-Object { $_.Path -like "*target\debug*" -and $_.MainWindowHandle -ne 0 } | Select-Object -First 1
if (-not $p) { Write-Output "no window"; exit 1 }
$r = New-Object M+RECT; [M]::GetWindowRect($p.MainWindowHandle, [ref]$r) | Out-Null
$w = $r.R - $r.L; $h = $r.B - $r.T
function Pos($fx, $fy) { [M]::SetCursorPos([int]($r.L + $fx * $w), [int]($r.T + $fy * $h)) | Out-Null }
switch ($Phase) {
  "down" { Pos $FromX $FromY; Start-Sleep -Milliseconds 150; [M]::mouse_event(0x0002, 0, 0, 0, [UIntPtr]::Zero); Start-Sleep -Milliseconds 150 }
  "move" { 1..20 | ForEach-Object { Pos ($FromX + ($ToX - $FromX) * $_ / 20) ($FromY + ($ToY - $FromY) * $_ / 20); Start-Sleep -Milliseconds 25 }; Start-Sleep -Milliseconds 300 }
  "up"   { [M]::mouse_event(0x0004, 0, 0, 0, [UIntPtr]::Zero) }
}
```

Risk: PowerShell is not DPI aware, so cursor and window coordinates are both scaled the same way and the fractions still land correctly; if the cursor lands off the target on a scaled display, set the display scale to 100 % for the run.

**6.3 Sandbox, test workspace and fake agent.** Run PowerShell from the worktree root, once per theme (`$theme = "dark"`, then again with `"light"`; delete `$root` between the runs so each run starts clean):

```powershell
$theme = "dark"
$target = "C:\Users\NguyenTung\Workspace\PersonalProject\mkrew-build-kit\mkrew\target\debug"
$root = "$env:TEMP\tern-smoke"
New-Item -ItemType Directory -Force "$root\data","$root\config","$root\ws\api\src","$root\ws\api\docs","$root\notes" | Out-Null
$env:MKREW_DATA_DIR = "$root\data"; $env:MKREW_CONFIG_DIR = "$root\config"; $env:MKREW_SOCKET = "$root\mkrew.sock"
$env:Path = "$target;$env:Path"
Set-Content "$root\config\settings.toml" "[ui]`ntheme = `"$theme`"`nright_panel = `"files`"`n`n[notes]`nfolder = `"`"`n"
# The test workspace: a git repository with a Rust file, a Markdown file and one change.
$ws = "$root\ws\api"
Set-Content "$ws\src\main.rs" "fn main() {`n    println!(`"hello`");`n}`n"
Set-Content "$ws\src\lib.rs" "pub fn add(a: u32, b: u32) -> u32 {`n    a + b`n}`n"
Set-Content "$ws\docs\plan.md" "# Plan`n`n- [ ] one`n- [ ] two`n`n| a | b |`n|---|---|`n| 1 | 2 |`n"
Set-Content "$ws\README.md" "# api`n`nA small test workspace.`n"
git -C $ws init -q
git -C $ws add -A
git -C $ws -c user.email=smoke@example.com -c user.name=smoke commit -q -m "initial"
# One edited file and one new file, so the Git panel and the Diff pane have rows.
Add-Content "$ws\src\main.rs" "`nfn helper() -> u32 {`n    42`n}"
Set-Content "$ws\src\new.rs" "pub const N: u32 = 1;`n"
Start-Process "$target\mkrew.exe"
Start-Sleep -Seconds 5
& "$target\mk.exe" project add $ws --name api
```

The UI starts the daemon with the same variables and picks the workspace up.
If `mk project add` runs before the daemon is up, run it again.
Set the workspace's default agent to `fake` (sidebar workspace menu > "Default agent" > fake) and the default shell to the first shell profile offered.
The fake agent's `ask` scenario prints `Do you want to proceed? (y/n)` and waits: that pane shows needs-input (amber bell) until you answer; leave one pane unanswered.

Set the window to 1400 x 860 in every capture (`-Width 1400 -Height 860`) so the images are comparable.

**6.4 Views and exact steps** (output names `docs\screenshots\tern-panes-<view>-$theme.png`; look at each image, not black, not empty, before `git add`):

1. `four-panes`
   Steps in the UI: new agent tab (`Ctrl+Shift+A`, the fake agent); split right (`Alt+Shift+=`) with the default shell (`Alt+Shift+S`) in the same tab; in the Files tree (right panel, `Ctrl+Shift+3`) drag `src/main.rs` onto the right edge of the shell pane (a file pane appears); drag `docs/plan.md` onto the bottom edge of that file pane, then set its mode to Split (`Ctrl+Alt+V`).
   Answer nothing in the first agent pane (it needs input), click the shell pane's terminal to focus it.
   Result: four panes, the focused shell with its stripe, the agent with the amber stripe.
   `$env:TEMP\shot.ps1 -Out docs\screenshots\tern-panes-four-panes-$theme.png -Width 1400 -Height 860`
2. `drop-edges`
   Capture at least the left and the top edge, and the right and the bottom edge when time allows (the overlay exists only while the button is held, so use `-Mode screen`, keep the window on top and run each pair of lines as one sequence).
   Start from the `four-panes` tab, drag `README.md` from the Files tree (tree row near the right panel; the fractions below assume a 1400 x 860 window with the tree open, adjust the row's fraction by looking at the first screenshot).
   ```powershell
   & $env:TEMP\hold.ps1 -Phase down -FromX 0.93 -FromY 0.30
   & $env:TEMP\hold.ps1 -Phase move -FromX 0.93 -FromY 0.30 -ToX 0.30 -ToY 0.55   # the left edge of the agent pane
   & $env:TEMP\shot.ps1 -Out docs\screenshots\tern-panes-drop-edges-left-$theme.png -Mode screen
   & $env:TEMP\hold.ps1 -Phase move -FromX 0.30 -FromY 0.55 -ToX 0.40 -ToY 0.18   # the top edge of the shell pane
   & $env:TEMP\shot.ps1 -Out docs\screenshots\tern-panes-drop-edges-top-$theme.png -Mode screen
   & $env:TEMP\hold.ps1 -Phase up
   ```
   Press Esc or drop on the pane's own edge afterwards (a drop opens the README as a pane; close it with the tab menu's Close pane).
   The names are `tern-panes-drop-edges-left-<theme>.png` and `tern-panes-drop-edges-top-<theme>.png` (and `-right-`, `-bottom-` when captured); each must show the tinted half.
3. `plus-row`
   Hover the `+` after the tabs with the real pointer (`-Mode screen` keeps the hover; the mouse stays where `hold.ps1 -Phase move` puts it):
   ```powershell
   & $env:TEMP\hold.ps1 -Phase move -FromX 0.50 -FromY 0.50 -ToX 0.40 -ToY 0.03   # the + button (adjust X after looking at the tab strip)
   Start-Sleep -Milliseconds 400
   & $env:TEMP\shot.ps1 -Out docs\screenshots\tern-panes-plus-row-$theme.png -Mode screen
   ```
   Result: `+`, agent logo, terminal icon, note icon and `▾` in a row.
4. `diff-pane`
   Open the Git panel (`Ctrl+Shift+4`), select `src/main.rs` (the Diff tab opens), then press `↓` once so the diff follows to `src/new.rs`; drag the Diff tab's pane (grip on its top edge after dropping it) so that the tab shows the agent pane and the diff side by side: easiest is to drag the Diff tab chip onto the right edge of the terminal pane of the first tab.
   `$env:TEMP\shot.ps1 -Out docs\screenshots\tern-panes-diff-pane-$theme.png -Width 1400 -Height 860`
5. `restore-after-restart`
   With the tabs of steps 1 to 4 open (plus a new note: `Ctrl+Alt+N`), close Mkrew (not the daemon) and start it again with the sandbox variables:
   ```powershell
   Get-Process mkrew | Where-Object { $_.Path -like "*target\debug*" } | Stop-Process
   Start-Process "$target\mkrew.exe"; Start-Sleep -Seconds 6
   $env:TEMP\shot.ps1 -Out docs\screenshots\tern-panes-restore-after-restart-$theme.png -Width 1400 -Height 860
   ```
   Result: the same tabs; file and diff leaves in place.
   Do the same after `mk daemon stop` and a new start of the UI once (no screenshot needed, note the result in the HUMAN_CHECKS "Restart" item if it fails).
6. `detached-file-tab`
   Drag a file tab (not the Diff tab) out of the window (tab chip to the desktop beyond the window edge); the new window opens at the pointer.
   It has its own handle, so capture by screen region: read its rectangle with `Get-Process mkrew` is not possible; instead take the whole screen and crop:
   ```powershell
   Add-Type -AssemblyName System.Windows.Forms, System.Drawing
   $b = [System.Windows.Forms.Screen]::PrimaryScreen.Bounds
   $bmp = New-Object System.Drawing.Bitmap $b.Width, $b.Height
   [System.Drawing.Graphics]::FromImage($bmp).CopyFromScreen($b.Location, [System.Drawing.Point]::Empty, $b.Size)
   $bmp.Save("docs\screenshots\tern-panes-detached-file-tab-$theme.png", [System.Drawing.Imaging.ImageFormat]::Png)
   ```
   (Full screen is fine; keep the two Mkrew windows apart so both are visible.)

If an image looks wrong (a pane without its stripe, the gap not visible, a pane with a border, a chip left in the bar, the `+` row not expanded), do not add it: fix the code in the task that owns it, rebuild and capture again.
Record each failure in a commit message, not in the images.

**Cleanup:** close Mkrew, run `& "$target\mk.exe" daemon stop` with the sandbox variables still set, then delete `$env:TEMP\tern-smoke`, `$env:TEMP\shot.ps1` and `$env:TEMP\hold.ps1`.
Do not touch processes you did not start.
Check that `git status --short` lists only the intended docs and `docs/screenshots/tern-panes-*.png`.

- [ ] **Step 7: Final verification**

Run from the worktree.

(a) Full verify (plain `bash` is WSL; use Git Bash).
This may run longer than 600 s, so give it `timeout: 0`:

```bash
"C:/Program Files/Git/bin/bash.exe" scripts/verify.sh
```

Expected last line: `verify: OK`.
If it passes 600 s or rustc crashes on the `mkrew-ui` test build (`STATUS_STACK_BUFFER_OVERRUN`), run the three parts by hand:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -j 4 -- -D warnings
INSTA_UPDATE=no cargo test --workspace -j 4
```

(The daemon tests need `cargo build -p mkrew-cli -p mkrew-fakeagent --bins -j 4` first; Step 6 already did it.)
Expected: no format diff, no warning, `test result: ok.` for every crate.

(b) Hex colors, raw pixel values and durations outside `theme.rs` in this branch's diff (ui-reviewer check 1).
Tests may hold literals; read each hit and keep only the ones in production code as findings:

```bash
git diff -U0 master...HEAD -- crates/mkrew-ui ':!crates/mkrew-ui/src/theme.rs' | grep -nE '^(\+\+\+ |\+.*(0x[0-9A-Fa-f]{6,8}\b|#[0-9A-Fa-f]{6}\b|\bpx\([0-9.]+\)|Duration::from_millis\([0-9]+\)))'
```

Expected: no `+` line in production code (a hit in a `#[cfg(test)]` module is fine; a hit elsewhere moves into `theme.rs` as a token and the caller uses it).

(c) Pressable rule, the regex of `.claude/agents/ui-reviewer.md` check 9:

```bash
git diff -U0 master...HEAD -- crates/mkrew-ui ':!crates/mkrew-ui/src/shell/widgets.rs' ':!crates/mkrew-ui/src/pressable.rs' | grep -E '^(\+\+\+ |\+.*(\.hover\(|\.active\(\||\.cursor_pointer\(\)))'
```

Expected: only `+++ b/<path>` header lines, no line with `.hover(`, `.active(|` or `.cursor_pointer()` (any hit line is a blocker; `group_hover(` is allowed).
Also: a widget plus `.pressable(..)` on one element is a blocker (second hover panics); check the `+` row items by reading `shell/chrome.rs`.

(d) Em dash in added docs text (words added by this branch only, so lines that keep an old em dash do not count):

```bash
git diff -U0 --word-diff=plain master...HEAD -- docs README.md ':!docs/superpowers' | grep -o '{+[^}]*+}' | grep -n $'\xe2\x80\x94'
```

Expected: no output.
Also `git diff -U0 master...HEAD -- crates | grep -nE '^\+.*'$'\xe2\x80\x94'`: no output (code comments and strings too).
The spec and plan files under `docs/superpowers/` are checked by their owners.

(e) Removed names must have zero hits in code and in the docs this branch owns:

```bash
git grep -nE "View::Editor|View::Diff|file_tabs|drop_axis|pane_focus_border" -- crates
git grep -nE "pane_focus_border|drop_axis|file_tabs" -- docs/SPEC.md docs/PROGRESS.md docs/HUMAN_CHECKS.md README.md
```

Expected: no output for both.
(`docs/decisions/*` and `docs/superpowers/*` may name them as history; the `docs/PROGRESS.md` Tern layout note of line 33 must not, since `git grep` on it above would list it: if it does, leave that old note as it is and only require no hit in the new note: `git diff -U0 master...HEAD -- docs/PROGRESS.md | grep -nE "pane_focus_border|drop_axis|file_tabs"` shows nothing.)

Extra zero-hit checks that Tasks 1 to 8 promised:

```bash
git grep -nE "ShowEditor|CloseFileTab|close_file_tab|show_editor|has_file_tabs|follow_file_tabs|diff_open\b|Open::Transient|Open::Kept|GitView.*\bopen\b" -- crates
git grep -nE "ctrl-shift-6|ctrl-cmd-6|CloseFileTab" -- crates docs/SPEC.md README.md
```

Expected: no output (SPEC and README no longer give `Ctrl+Shift+6`, written as `ctrl-shift-6` only in code).

(f) Doc consistency between SPEC, code and ADR:

```bash
git grep -n "PROTOCOL_VERSION: u32 = 60" -- crates/mkrew-protocol/src/lib.rs
git grep -nE "Protocol 60" -- docs/SPEC.md
git grep -nE "^\[notes\]" -- docs/SPEC.md
git grep -nE "fs\.new_note" -- docs/SPEC.md crates/mkrew-protocol/src/lib.rs
ls docs/decisions | grep -E "^0108"
```

Expected: one hit each (the `fs.new_note` check: at least one hit in each file).

(g) Screenshots exist and are in git:

```bash
ls docs/screenshots | grep -c "^tern-panes-"
git status --short docs README.md
```

Expected: at least 12 images (six views by two themes, `drop-edges` counts the left and top images) and only the files of this task listed.

- [ ] **Step 8: Commit**

Three commits, only this task's files, never push, never `git add -A`:

```bash
git add docs/decisions/0108-tern-panes.md docs/decisions/0080-files-view-tabs.md docs/decisions/0106-tern-layout.md docs/decisions/0041-closing-a-tab-removes-its-sessions.md
git -c user.email=tungnv975@gmail.com commit -m "docs(adr): 0108 Tern panes, amend 0041 and 0106, supersede 0080"
```

(use the real file names from `ls docs/decisions | grep -E "^(0080|0041)"` in the `git add`).

```bash
git add docs/SPEC.md docs/HUMAN_CHECKS.md docs/PROGRESS.md README.md
git -c user.email=tungnv975@gmail.com commit -m "docs: SPEC, HUMAN_CHECKS, PROGRESS and README for Tern panes"
```

```bash
git add docs/screenshots/tern-panes-*.png
git -c user.email=tungnv975@gmail.com commit -m "docs(screenshots): Tern panes smoke, dark and light"
```

Then `git status --short` must show nothing of this task, and `git log --oneline -3` the three commits.
Do not merge or push: the branch finishes through the finishing-a-development-branch skill, where Step 1's number check runs once more against `master`.

---

