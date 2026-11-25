# Pane render performance Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Typing and scrolling stay smooth next to busy agents and in long omp sessions: each pane is a cached GPUI view, and the omp transcript lays out only the rows on screen.

**Architecture:** A new `PaneView` (one per pane slot, `.cached()`, observes `Shell` like `SidebarView`) renders `Shell::pane_frame`; the terminal grid is embedded `.cached()` too.
The omp surface's `main` becomes a `gpui::list` whose `ListState` hears row splices and remeasures from `Mirror` frames.
State recorded while painting (menu anchors, the voice typing target, text hit areas) is cleared by the view that records it again.

**Tech Stack:** Rust, `gpui-pre 0.3.7` (`gpui::list`, `ListState`, `AnyView::cached`), the existing `mkrew-ui` crate and its test harnesses.

**Spec:** `docs/superpowers/specs/2026-11-25-pane-render-perf-design.md` (read it first; section numbers below refer to it).

## Global Constraints

- Work only in `.worktrees/pane-perf` (branch `feat/pane-perf`). Give every omp tool path as `.worktrees/pane-perf/...` or absolute; set `cwd` for bash (AGENTS.md, worktrees rule).
- Plain bash in the omp shell is WSL: run scripts with `"C:/Program Files/Git/bin/bash.exe"`.
- No protocol change (master is at 63), no migration. ADR number 0114; re-check `docs/decisions/` on `master` and unmerged branches before the final commit.
- Every color, spacing, radius, font size and UI constant comes from `crates/mkrew-ui/src/theme.rs` (SPEC §14).
- No `unwrap`/`expect` outside tests. No em dash in any file. Each Markdown sentence on its own line.
- Nothing may look different: same layout, same colors, same behavior (spec §1 success list).
- Commit with `git -c user.email=tungnv975@gmail.com commit`, staging files by explicit path; Conventional Commits; no co-author lines.
- Never kill the installed Mkrew, its daemon or live sessions; only stop processes whose exe is inside a sandbox root this plan made.
- Before building, check free space on C: (`powershell -NoProfile -Command "(Get-PSDrive C).Free/1GB"`); below 15 GB run the `mkrew-cleanup` skill first.

## Review Focus

- omp streams into the last row while the user is at the end: the page keeps following the end as that row grows (Task 2 remeasures the kept last row; Task 6 checks it by hand).
- The user scrolled up to read while new rows arrive: what they read does not move (gpui `splice` keeps the scroll-top item; Task 6 checks it by hand).
- Light/dark or font size changes while scrolled to the middle of a long transcript: rows do not overlap or leave gaps (Task 2 remeasures on a theme or font change; Task 6 checks it).
- A tab moved to a detached window and back: its panes show and update in both windows, and no view of the old slot keeps drawing (Task 3 keys views by host and prunes them; Task 6 checks it).
- Voice input while the focused pane reused its last frame: the text goes into that pane's field and the ring shows (Task 3 owner rule; Task 6 checks it).

---

### Task 1: Dirty rows and row changes (pure, `tsp.rs`)

**Files:**
- Modify: `crates/mkrew-ui/src/tsp.rs` (the `Mirror` struct near line 416, `Mirror::apply` near line 527, `reveal_index` near line 761, the tests module from line 1693)

**Interfaces:**
- Produces:
  - `pub fn row_of(doc: &Document, id: &str) -> Option<String>`
  - `pub fn rows_named(doc: &Document, ops: &[Value]) -> HashSet<String>`
  - `pub fn image_rows(doc: &Document, blobs: &[TspBlob]) -> HashSet<String>`
  - `#[derive(Debug, Clone, PartialEq, Eq)] pub enum RowChange { Splice { range: std::ops::Range<usize>, count: usize }, Remeasure(usize) }`
  - `pub fn row_changes(old: &[String], new: &[String], dirty: &HashSet<String>) -> Vec<RowChange>`
  - `Mirror::take_dirty_rows(&mut self) -> HashSet<String>`

- [ ] **Step 1: Write the failing tests** (append inside `mod tests` of `tsp.rs`, after `add_x`)

```rust
    /// `main` with rows `a` (holding `a1`) and `b` (holding `b1`), and a dock holding `d1`.
    fn two_rows() -> Vec<serde_json::Value> {
        let mut ops = main_added();
        ops.push(json!(["add", "a", "main", null, {"id": "a", "k": "col", "c": [
            {"id": "a1", "k": "text", "p": {"text": "x"}}
        ]}]));
        ops.push(json!(["add", "b", "main", null, {"id": "b", "k": "col", "c": [
            {"id": "b1", "k": "text", "p": {"text": "y"}}
        ]}]));
        ops.push(json!(["add", "dock", "s:1", null, {"id": "dock", "k": "col", "c": [
            {"id": "d1", "k": "text", "p": {"text": "z"}}
        ]}]));
        ops
    }

    /// The rows one frame of `ops` dirties in a mirror of [`two_rows`].
    fn dirtied(ops: Vec<serde_json::Value>) -> Vec<String> {
        let d = doc(two_rows());
        let mut m = mirror(vec![surface("s:1", "inline", 0, &d)]);
        assert_eq!(m.apply(&frame("s:1", 1, ops)), MirrorResult::Applied);
        let mut rows: Vec<String> = m.take_dirty_rows().into_iter().collect();
        rows.sort();
        assert!(m.take_dirty_rows().is_empty(), "taking clears them");
        rows
    }

    #[test]
    fn a_frame_dirties_the_rows_its_ops_name_before_and_after() {
        assert_eq!(dirtied(vec![json!(["set", "a1", {"text": "w"}])]), ["a"]);
        assert_eq!(
            dirtied(vec![json!(["add", "b2", "b", null, {"id": "b2", "k": "text"}])]),
            ["b"]
        );
        assert_eq!(
            dirtied(vec![json!(["add", "c", "main", null, {"id": "c", "k": "text"}])]),
            ["c"],
            "a new row names itself; `main` names no row"
        );
        assert_eq!(dirtied(vec![json!(["move", "a1", "b", null])]), ["a", "b"]);
        assert_eq!(dirtied(vec![json!(["del", "a1"])]), ["a"], "found before the delete");
        assert_eq!(dirtied(vec![json!(["del", "a"])]), ["a"]);
        assert_eq!(dirtied(vec![json!(["set", "d1", {"text": "w"}])]), Vec::<String>::new());
        assert_eq!(dirtied(vec![json!(["set", "main", {}])]), Vec::<String>::new());
        assert_eq!(dirtied(vec![json!(["reveal", "a1"])]), Vec::<String>::new());
    }

    #[test]
    fn row_changes_splice_the_middle_and_remeasure_kept_dirty_rows() {
        let ids = |list: &[&str]| list.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        let set = |list: &[&str]| list.iter().map(|s| s.to_string()).collect::<HashSet<_>>();
        use RowChange::{Remeasure, Splice};
        let none = HashSet::new();
        assert_eq!(row_changes(&ids(&["a", "b"]), &ids(&["a", "b"]), &none), []);
        assert_eq!(
            row_changes(&ids(&["a", "b"]), &ids(&["a", "b", "c"]), &none),
            [Splice { range: 2..2, count: 1 }]
        );
        assert_eq!(
            row_changes(&ids(&["a", "b", "c"]), &ids(&["a", "x", "b", "c"]), &none),
            [Splice { range: 1..1, count: 1 }]
        );
        assert_eq!(
            row_changes(&ids(&["a", "b", "c"]), &ids(&["b", "c"]), &none),
            [Splice { range: 0..1, count: 0 }]
        );
        assert_eq!(
            row_changes(&ids(&["a", "b", "c"]), &ids(&["a", "y", "c"]), &none),
            [Splice { range: 1..2, count: 1 }]
        );
        assert_eq!(
            row_changes(&ids(&["a", "b"]), &ids(&["c"]), &none),
            [Splice { range: 0..2, count: 1 }]
        );
        assert_eq!(
            row_changes(
                &ids(&["a", "b", "c"]),
                &ids(&["a", "b", "c", "d"]),
                &set(&["a", "c", "d"])
            ),
            [Splice { range: 3..3, count: 1 }, Remeasure(0), Remeasure(2)],
            "a new row is measured anyway"
        );
        assert_eq!(
            row_changes(&ids(&["a", "b"]), &ids(&["a"]), &set(&["b"])),
            [Splice { range: 1..2, count: 0 }],
            "a dirty row that left is not remeasured"
        );
    }

    #[test]
    fn image_rows_are_the_rows_holding_an_image_of_the_new_blobs() {
        let mut ops = main_added();
        ops.push(json!(["add", "a", "main", null, {"id": "a", "k": "col", "c": [
            {"id": "a1", "k": "image", "p": {"blob": "p1"}}
        ]}]));
        ops.push(json!(["add", "b", "main", null, {"id": "b", "k": "image", "p": {"blob": "p2"}}]));
        let d = doc(ops);
        let blob = |id: &str| TspBlob {
            id: id.into(),
            mime: "image/png".into(),
            data: Vec::new(),
        };
        assert_eq!(image_rows(&d, &[blob("p1")]), HashSet::from(["a".to_owned()]));
        assert_eq!(image_rows(&d, &[blob("p2")]), HashSet::from(["b".to_owned()]));
        assert!(image_rows(&d, &[blob("p9")]).is_empty());
    }
```

`TspBlob` is `{ id: String, mime: String, data: Vec<u8> }` (`crates/mkrew-protocol/src/message.rs`).

- [ ] **Step 2: Run the tests to see them fail**

Run (cwd `.worktrees/pane-perf`): `cargo test -p mkrew-ui --lib tsp::tests::`
Expected: compile errors: `take_dirty_rows`, `row_changes`, `RowChange` and `image_rows` not found.

- [ ] **Step 3: Implement**

In the `Mirror` struct add the field (`Default` stays derived):

```rust
    /// The rows (direct children of `main`) of the last applied frame's surface that its ops
    /// named, before and after they ran (ADR 0114); taken by the page list.
    dirty_rows: HashSet<String>,
```

In `Mirror::apply`, as the first line of the function body:

```rust
        self.dirty_rows.clear();
```

and replace the single line `doc.apply(ops);` of the `Frame` arm with:

```rust
                // The page list hears which rows changed: a `del` or `move` is found where the
                // node was, everything else where it is now.
                let mut dirty = rows_named(doc, ops);
                doc.apply(ops);
                dirty.extend(rows_named(doc, ops));
                self.dirty_rows = dirty;
```

(If the borrow checker rejects `self.dirty_rows` while `doc` is borrowed from `self.surfaces`, keep `dirty` in a local and assign it after the last use of `meta` and `doc` in that arm.)

Add to `impl Mirror`:

```rust
    /// The rows the last applied frame dirtied (see [`rows_named`]); empty after this call.
    pub fn take_dirty_rows(&mut self) -> HashSet<String> {
        std::mem::take(&mut self.dirty_rows)
    }
```

Replace `reveal_index` and add the new functions next to it:

```rust
/// The direct child of `main` that is `id` or holds it; `None` for `main` itself and for nodes
/// outside it (the dock, a layer).
pub fn row_of(doc: &Document, id: &str) -> Option<String> {
    let mut at = doc.get(id)?;
    loop {
        let parent = at.parent.as_deref()?;
        if parent == "main" {
            return Some(at.id.clone());
        }
        at = doc.get(parent)?;
    }
}

/// Index among `main`'s children of the row that holds `id`: what a `reveal` op scrolls to.
pub fn reveal_index(doc: &Document, id: &str) -> Option<usize> {
    let row = row_of(doc, id)?;
    doc.children("main").position(|node| node.id == row)
}

/// The rows holding the nodes `ops` name in `doc`: each op's node, and the parent an `add` or a
/// `move` names. `reveal`, `scroll` and `focus` only point at a node and change nothing drawn.
pub fn rows_named(doc: &Document, ops: &[Value]) -> HashSet<String> {
    let mut rows = HashSet::new();
    for op in ops {
        let Some(args) = op.as_array() else { continue };
        let named: &[usize] = match args.first().and_then(Value::as_str) {
            Some("add" | "move") => &[1, 2],
            Some("set" | "text" | "splice" | "del" | "settle") => &[1],
            _ => &[],
        };
        for &at in named {
            if let Some(row) = (args.get(at).and_then(Value::as_str)).and_then(|id| row_of(doc, id))
            {
                rows.insert(row);
            }
        }
    }
    rows
}

/// The rows holding an `image` node that names one of `blobs`: an image that just arrived
/// changes its row's height.
pub fn image_rows(doc: &Document, blobs: &[TspBlob]) -> HashSet<String> {
    let wanted: HashSet<&str> = blobs.iter().map(|blob| blob.id.as_str()).collect();
    let mut rows = HashSet::new();
    for row in doc.children("main") {
        let mut stack = vec![row];
        while let Some(node) = stack.pop() {
            let blob = node.props.get("blob").and_then(Value::as_str);
            if node.kind == "image" && blob.is_some_and(|id| wanted.contains(id)) {
                rows.insert(row.id.clone());
                break;
            }
            stack.extend(doc.children(&node.id));
        }
    }
    rows
}

/// What the page list must hear when `main`'s rows go from `old` to `new` (ADR 0114).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RowChange {
    /// Rows `range` of the old list became `count` new rows.
    Splice {
        range: std::ops::Range<usize>,
        count: usize,
    },
    /// Row `.0` (numbered as in the new list) changed its content: measure it again.
    Remeasure(usize),
}

/// The common prefix and suffix of `old` and `new` stay; the middle is one splice. A kept row in
/// `dirty` is remeasured. Apply the changes in order.
pub fn row_changes(old: &[String], new: &[String], dirty: &HashSet<String>) -> Vec<RowChange> {
    let prefix = old.iter().zip(new).take_while(|(a, b)| a == b).count();
    let room = old.len().min(new.len()) - prefix;
    let suffix = (old.iter().rev())
        .zip(new.iter().rev())
        .take(room)
        .take_while(|(a, b)| a == b)
        .count();
    let mut changes = Vec::new();
    let (old_end, new_end) = (old.len() - suffix, new.len() - suffix);
    if old_end > prefix || new_end > prefix {
        changes.push(RowChange::Splice {
            range: prefix..old_end,
            count: new_end - prefix,
        });
    }
    for (ix, id) in new.iter().enumerate() {
        if (ix < prefix || ix >= new_end) && dirty.contains(id) {
            changes.push(RowChange::Remeasure(ix));
        }
    }
    changes
}
```

`tsp.rs` already imports `TspBlob` from `mkrew_protocol` and `HashSet` from `std::collections`.

- [ ] **Step 4: Run the tests**

Run: `cargo test -p mkrew-ui --lib tsp::`
Expected: all `tsp::` tests pass, the three new ones and the existing `reveal_index` tests included.

- [ ] **Step 5: Commit**

```bash
git add crates/mkrew-ui/src/tsp.rs
git -c user.email=tungnv975@gmail.com commit -m "feat(ui): omp mirror reports the rows each frame changes (ADR 0114)"
```

---

### Task 2: The omp transcript as a virtual list (`shell/tsp_view.rs`)

**Files:**
- Modify: `crates/mkrew-ui/src/shell/tsp_view.rs` (`TspUi` lines 36-160, `fetch_tsp` near 205, `tsp_update` near 228-300, `tsp_todo_phase` near 776, `render_tsp` near 983-1201, the popover height near 2840)
- Modify: `crates/mkrew-ui/src/theme.rs` (next to `TSP_POPOVER_MAX_FRACTION`)
- Modify: `crates/mkrew-ui/src/tsp.rs` (delete `paged_offset` near line 926 and its test)

**Interfaces:**
- Consumes (Task 1): `tsp::row_of`, `tsp::image_rows`, `tsp::row_changes`, `tsp::RowChange`, `Mirror::take_dirty_rows`.
- Produces: `TspUi::sync_rows(&mut self, surface: Option<&str>, dirty: &HashSet<String>)`, `TspUi::follow_end(&self)`, `TspUi::row_index(&self, id: &str) -> Option<usize>`, `Shell::tsp_page_row(...)`, `theme::TSP_LIST_OVERDRAW: f32`.

This task is UI wiring over gpui's list; its logic is Task 1's pure functions. It is verified by building, by the existing tests and by hand in Task 6.

- [ ] **Step 1: The overdraw constant** (`theme.rs`, next to `TSP_POPOVER_MAX_FRACTION`)

```rust
/// How far past the visible part the omp page list lays out rows (px), so a wheel turn shows
/// rows that are already measured (ADR 0114).
pub const TSP_LIST_OVERDRAW: f32 = 600.0;
```

- [ ] **Step 2: `TspUi` state**

In `struct TspUi` delete the fields `scroll: ScrollHandle` and `reveal_pending: Cell<bool>` (and their doc comments), and add:

```rust
    /// The page list (ADR 0114): only the rows on screen (and `TSP_LIST_OVERDRAW` around them)
    /// are laid out. Item 0 and item `n + 1` are spacers; item `i + 1` is row `i` of `main`.
    rows: gpui::ListState,
    /// The ids of `main`'s children the list was last told about, and their surface.
    row_ids: Vec<String>,
    rows_surface: Option<String>,
    /// The theme and fonts the rows were measured with: a change measures them again.
    measured: RefCell<Option<(Theme, theme::Fonts)>>,
```

In `TspUi::new` delete `scroll: ScrollHandle::new(),` and `reveal_pending: Cell::new(false),` and add:

```rust
            rows: new_rows(),
            row_ids: Vec::new(),
            rows_surface: None,
            measured: RefCell::new(None),
```

Add above `impl TspUi` (the file's own `struct ListState` for nested lists stays; always write the gpui one as `gpui::ListState`):

```rust
/// An empty page list (its two spacers) that follows the end.
fn new_rows() -> gpui::ListState {
    let rows = gpui::ListState::new(2, gpui::ListAlignment::Top, px(theme::TSP_LIST_OVERDRAW));
    rows.set_follow_mode(gpui::FollowMode::Tail);
    rows
}
```

Replace `fn page` and add the new methods in `impl TspUi`:

```rust
    /// Scrolls the page list a page (the height it shows) up or down.
    fn page(&self, up: bool) {
        let height = self.rows.viewport_bounds().size.height;
        self.rows.scroll_by(if up { -height } else { height });
    }

    /// Back to the end of the page, following it again.
    fn follow_end(&self) {
        self.rows.set_follow_mode(gpui::FollowMode::Tail);
    }

    /// The page list index of the row holding node `id` of the shown surface.
    fn row_index(&self, id: &str) -> Option<usize> {
        let (_, doc) = self.mirror.as_ref()?.shown()?;
        let row = tsp::row_of(doc, id)?;
        let at = self.row_ids.iter().position(|known| *known == row)?;
        Some(at + 1)
    }

    /// Tells the page list what the mirror's last change did to `main` (ADR 0114). `surface`:
    /// the surface a frame, open or close was for; `None` after a snapshot. A snapshot or a
    /// different shown surface resets the list; a change of the shown surface splices and
    /// remeasures; a change of a hidden surface leaves it.
    fn sync_rows(&mut self, surface: Option<&str>, dirty: &HashSet<String>) {
        let Some((meta, doc)) = self.mirror.as_ref().and_then(Mirror::shown) else {
            self.row_ids.clear();
            self.rows_surface = None;
            self.rows.reset(2);
            return;
        };
        let new: Vec<String> = doc.children("main").map(|node| node.id.clone()).collect();
        let shown = meta.id.clone();
        if surface.is_none() || self.rows_surface.as_deref() != Some(shown.as_str()) {
            self.rows.reset(new.len() + 2);
        } else if surface == Some(shown.as_str()) {
            for change in tsp::row_changes(&self.row_ids, &new, dirty) {
                match change {
                    tsp::RowChange::Splice { range, count } => {
                        self.rows.splice(range.start + 1..range.end + 1, count);
                    }
                    tsp::RowChange::Remeasure(ix) => self.rows.remeasure_items(ix + 1..ix + 2),
                }
            }
        } else {
            return;
        }
        self.rows_surface = Some(shown);
        self.row_ids = new;
    }
```

- [ ] **Step 3: Feed the list from the mirror**

In `fetch_tsp`, in the `Ok(snapshot)` arm, after `ui.prune();` add:

```rust
                        ui.sync_rows(None, &HashSet::new());
```

In `tsp_update`, right after the `if mirror.apply(update) == MirrorResult::Gap { ... }` block, add:

```rust
        let mut dirty = (ui.mirror.as_mut()).map(Mirror::take_dirty_rows).unwrap_or_default();
        if let TspChange::Frame { blobs, .. } = &update.change
            && !blobs.is_empty()
            && let Some((_, doc)) = (ui.mirror.as_ref()).and_then(|m| m.surface(&update.surface))
        {
            dirty.extend(tsp::image_rows(doc, blobs));
        }
```

and after the `if tsp::removes_nodes(&update.change) { ui.prune(); }` block add:

```rust
        ui.sync_rows(Some(&update.surface), &dirty);
```

In the same function replace the reveal and scroll handling (the `if let Some((list, ix)) = in_list { ... }` statement and the `match tsp::scroll_in(ops) { ... }` after it):

```rust
        if let Some((list, ix)) = in_list {
            list_handle(&ui.lists, &list).scroll_to_item(ix);
        } else if let Some(ix) = reveal {
            // Item 0 of the page list is the spacer under the pane's drag strip.
            ui.rows.pause_following_tail();
            ui.rows.scroll_to_reveal_item(ix + 1);
        }
        match tsp::scroll_in(ops) {
            Some(ListScroll::Top) => ui.rows.scroll_to(gpui::ListOffset::default()),
            Some(ListScroll::Bottom) => ui.follow_end(),
            Some(ListScroll::Page { up }) => ui.page(up),
            None => {}
        }
```

- [ ] **Step 4: A folded checklist phase measures its row again**

In `tsp_todo_phase`, before `cx.notify();`:

```rust
        if let Some(ix) = ui.row_index(node) {
            ui.rows.remeasure_items(ix..ix + 1);
        }
```

- [ ] **Step 5: Render the page as a list**

In `render_tsp`, delete the block that starts `// Follow the end while the user is at it` (the `at_end` and `reveal_pending` lines).
Right after `let Some((ui, mirror, (meta, doc))) = shown else { ... };` add:

```rust
        // The rows were measured with another theme or font: measure them again (ADR 0114).
        let measuring = (theme.clone(), cx.global::<theme::Fonts>().clone());
        if ui.measured.borrow().as_ref() != Some(&measuring) {
            ui.rows.remeasure();
            ui.measured.replace(Some(measuring));
        }
        // Text hit areas are recorded again by the rows painted in this frame; a row scrolled
        // out must not keep its old ones.
        ui.hits.borrow_mut().clear();
```

Replace the comment above it and the `let rows: Vec<AnyElement> = if screen { ... } else { ... };` statement with the screen rows only (keep the comment lines about the columns and `max`):

```rust
        let screen_rows: Vec<AnyElement> = if screen {
            (doc.children("main"))
                .map(|node| {
                    page_column(self.tsp_node(&screen_paint, node, cx), max, true)
                        .into_any_element()
                })
                .collect()
        } else {
            Vec::new()
        };
```

Replace the inner child `div().id(SharedString::from(format!("tsp-main-{session}")))` ... `.children(rows),` with:

```rust
                .child({
                    let main = div()
                        .id(SharedString::from(format!("tsp-main-{session}")))
                        .size_full()
                        // A drag extends the selection; a press off any text clears it.
                        .on_mouse_move(cx.listener(move |shell, event: &MouseMoveEvent, _, cx| {
                            if event.dragging() {
                                shell.tsp_drag(session, event.position, cx);
                            }
                        }))
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |shell, _: &MouseDownEvent, _, cx| {
                                shell.tsp_clear_selection(session, cx);
                            }),
                        );
                    if screen {
                        main.flex()
                            .flex_col()
                            .pt(px(space::L))
                            .pb(px(space::M))
                            .overflow_hidden()
                            .children(screen_rows)
                    } else {
                        let shell = cx.entity().downgrade();
                        let count = ui.row_ids.len();
                        let row_theme = theme.clone();
                        let compact = paint.compact;
                        main.child(
                            gpui::list(ui.rows.clone(), move |ix, _, cx| {
                                // The spacers keep the first row clear of the drag strip and
                                // part the last one from the dock.
                                if ix == 0 || ix > count {
                                    return div().h(px(space::L)).into_any_element();
                                }
                                shell
                                    .update(cx, |shell, cx| {
                                        shell.tsp_page_row(
                                            session, ix - 1, is_focused, compact, max, now,
                                            &row_theme, cx,
                                        )
                                    })
                                    .unwrap_or_else(|_| div().into_any_element())
                            })
                            .size_full(),
                        )
                    }
                })
```

Replace `.when(!at_end && !screen, |el| {` with `.when(!ui.rows.is_following_tail() && !screen, |el| {`, and in that button's press handler replace `ui.scroll.scroll_to_bottom();` with `ui.follow_end();`.

Add the row builder to `impl Shell` in this file, after `render_tsp` (`tsp_row` already exists for the `row` node kind; do not reuse that name):

```rust
    /// Row `index` of the shown surface's `main`, in its column: one item of the page list.
    #[allow(clippy::too_many_arguments)]
    fn tsp_page_row(
        &self,
        session: Ulid,
        index: usize,
        focused: bool,
        compact: bool,
        max: Option<f32>,
        now: i64,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> AnyElement {
        let shown = (self.tsps.get(&session))
            .and_then(|ui| (ui.mirror.as_ref()).and_then(|m| m.shown().map(|s| (ui, m, s))));
        let Some((ui, mirror, (meta, doc))) = shown else {
            return div().into_any_element();
        };
        let Some(node) = ui.row_ids.get(index).and_then(|id| doc.get(id)) else {
            return div().into_any_element();
        };
        let paint = Paint {
            session,
            doc,
            mirror,
            shown_id: &meta.id,
            ui,
            now,
            theme,
            focused,
            modal: doc.children("layer").any(|n| !bool_prop(n, "hidden")),
            in_layer: false,
            bounded: false,
            compact,
        };
        page_column(self.tsp_node(&paint, node, cx), max, false).into_any_element()
    }
```

- [ ] **Step 6: The popover height**

Near line 2840 replace `p.ui.scroll.bounds().size.height` with `p.ui.rows.viewport_bounds().size.height`.
`tsp_command`'s PageUp / PageDown arm keeps calling `ui.page(..)` and `cx.notify()`; nothing else to change there.

- [ ] **Step 7: Delete what the list made obsolete**

Delete `pub fn paged_offset` (and its doc comment) from `tsp.rs` and the test that calls it (`rg -n paged_offset crates/mkrew-ui/src`).
Run `rg -n "reveal_pending|ui\.scroll\b|\.scroll\.scroll_to_bottom" crates/mkrew-ui/src/shell/tsp_view.rs`; expected: no match.
Remove imports that became unused (the compiler lists them).

- [ ] **Step 8: Build, lint and test**

Run (cwd `.worktrees/pane-perf`): `cargo clippy -p mkrew-ui --all-targets -- -D warnings`
Expected: no warnings.
Run: `cargo test -p mkrew-ui --lib tsp::` then `cargo test -p mkrew-ui --bin mkrew tsp`
Expected: PASS.

- [ ] **Step 9: Commit**

```bash
git add crates/mkrew-ui/src/shell/tsp_view.rs crates/mkrew-ui/src/theme.rs crates/mkrew-ui/src/tsp.rs
git -c user.email=tungnv975@gmail.com commit -m "perf(ui): omp transcript lays out only the rows on screen (ADR 0114)"
```

---

### Task 3: Cached pane views (`PaneView`)

**Files:**
- Create: `crates/mkrew-ui/src/shell/pane_view.rs`
- Modify: `crates/mkrew-ui/src/shell.rs` (module list)
- Modify: `crates/mkrew-ui/src/main.rs` (`struct Shell` near line 936, frame start near line 1002)
- Modify: `crates/mkrew-ui/src/shell/connection.rs` (Shell construction near line 534)
- Modify: `crates/mkrew-ui/src/shell/pane_render.rs` (`render_tab` lines 442-576, `pane_frame` line 618)
- Modify: `crates/mkrew-ui/src/shell/chrome.rs` (the `render_tab` call near line 220)
- Modify: `crates/mkrew-ui/src/shell/detached_render.rs` (the `render_tab` call near line 65)
- Modify: `crates/mkrew-ui/src/shell/focus.rs` (`sync_panes` line 287)
- Modify: `crates/mkrew-ui/src/shell/menu_render.rs` (`Anchor` near line 548, `MenuAnchors` near 565, tests near 696)
- Modify: `crates/mkrew-ui/src/ime.rs` (`TypingTarget` near line 134)
- Modify: `crates/mkrew-ui/src/shell/widgets.rs` (`typing_input_for` near line 894)

**Interfaces:**
- Produces:
  - `pub(crate) struct PaneSlot { pub(crate) host: HostId, pub(crate) project_id: Ulid, pub(crate) tab_id: Ulid, pub(crate) key: PaneKey }` (`Clone, Debug, PartialEq, Eq, Hash`)
  - `pub(crate) struct PaneView` with `PaneView::new(shell: &Entity<Shell>, slot: PaneSlot, cx: &mut Context<'_, PaneView>) -> PaneView`
  - `Shell::render_pane(&self, slot: &PaneSlot, view: gpui::EntityId, window: &mut Window, cx: &mut Context<'_, Shell>) -> gpui::AnyElement`
  - `Shell.pane_views: RefCell<HashMap<PaneSlot, Entity<PaneView>>>`
  - `render_tab(&self, host: HostId, project_id: Ulid, tab: Option<&Tab>, now: i64, theme: &Theme, content: Rc<Cell<gpui::Size<Pixels>>>, window: &Window, cx: &mut Context<'_, Self>) -> gpui::Div`
  - `MenuAnchors::clear_pane(&self, window: gpui::WindowId, session: Ulid)`
  - `TypingTarget::owner: Cell<Option<gpui::EntityId>>`

- [ ] **Step 1: Write the failing anchor test**

Replace `the_cached_sidebar_keeps_its_anchors_across_shell_frames` in `menu_render.rs`; its expectation that a `Shell` frame drops pane anchors is the contract this task changes.

```rust
    #[test]
    fn cached_views_keep_their_anchors_across_shell_frames() {
        let (id, other_id) = (Ulid::from(1u128), Ulid::from(2u128));
        let (main, other) = (gpui::WindowId::from(1u64), gpui::WindowId::from(2u64));
        let anchors = MenuAnchors::default();
        for key in [
            Anchor::Switcher,
            Anchor::Pane(id),
            Anchor::Pane(other_id),
            Anchor::WorkspaceRow(id),
            Anchor::SessionRow(id),
        ] {
            anchors
                .0
                .borrow_mut()
                .insert((main, key), Bounds::default());
        }
        anchors
            .0
            .borrow_mut()
            .insert((other, Anchor::Pane(id)), Bounds::default());
        let left = |anchors: &MenuAnchors| {
            let mut keys: Vec<_> = anchors.0.borrow().keys().cloned().collect();
            keys.sort_by_key(|key| format!("{key:?}"));
            keys
        };
        anchors.clear(main);
        assert_eq!(
            left(&anchors),
            [
                (main, Anchor::Pane(id)),
                (main, Anchor::Pane(other_id)),
                (main, Anchor::SessionRow(id)),
                (main, Anchor::WorkspaceRow(id)),
                (other, Anchor::Pane(id)),
            ],
            "a Shell frame keeps what the cached sidebar and panes recorded"
        );
        anchors.clear_pane(main, id);
        anchors.clear_sidebar();
        assert_eq!(
            left(&anchors),
            [(main, Anchor::Pane(other_id)), (other, Anchor::Pane(id))],
            "each cached view clears only its own anchors"
        );
    }
```

- [ ] **Step 2: Run it to see it fail**

Run: `cargo test -p mkrew-ui --bin mkrew cached_views_keep_their_anchors`
Expected: compile error, `clear_pane` not found.

- [ ] **Step 3: Anchors** (`menu_render.rs`)

```rust
impl Anchor {
    /// Owners inside the cached sidebar view (it re-records them only when it re-renders).
    fn in_sidebar(&self) -> bool {
        matches!(self, Self::WorkspaceRow(_) | Self::SessionRow(_))
    }

    /// Owners inside a cached view (the sidebar, a pane): a `Shell` frame keeps them.
    fn in_cached_view(&self) -> bool {
        self.in_sidebar() || matches!(self, Self::Pane(_))
    }
}
```

In `MenuAnchors::clear` replace `key.in_sidebar()` with `key.in_cached_view()` and make its doc comment "Forgets `window`'s anchors but those of cached views, and leaves other windows' alone (the `Shell` frame start)". Add:

```rust
    /// Forgets the anchor of `session`'s pane in `window` (its `PaneView` records it again while
    /// it renders).
    pub(crate) fn clear_pane(&self, window: gpui::WindowId, session: Ulid) {
        let key = (window, Anchor::Pane(session));
        self.0.borrow_mut().remove(&key);
    }
```

Change the doc comment of `MenuAnchors` to end with "except the keys of cached views: the sidebar and each pane clear and record them themselves".

Run: `cargo test -p mkrew-ui --bin mkrew cached_views_keep_their_anchors`
Expected: PASS.

- [ ] **Step 4: Typing target owner** (`ime.rs`, `widgets.rs`)

In `TypingTarget` add the field, and `owner: Cell::new(None),` to its `Default`:

```rust
    /// The view whose paint registered `field` (`Window::current_view`): a cached pane view
    /// keeps it across frames that reuse its paint (ADR 0114).
    pub owner: Cell<Option<gpui::EntityId>>,
```

In `typing_input_for`, right after `typing_target.field.replace(Some((focus.clone(), on_key)));` add:

```rust
                typing_target.owner.set(Some(window.current_view()));
```

- [ ] **Step 5: `PaneView`** (create `shell/pane_view.rs`; add `pub(crate) mod pane_view;` to `shell.rs` in alphabetical order)

```rust
//! One pane of a tab as its own view, embedded `cached` (ADR 0114): output, frames and keys of
//! other panes do not lay it out again. Like `SidebarView` it renders again whenever the
//! `Shell` notifies, the window refreshes, its bounds change or something inside it scrolls.

use gpui::{Subscription, WeakEntity};

use crate::*;

/// Where a pane is drawn: its window, workspace, tab and leaf.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) struct PaneSlot {
    pub(crate) host: HostId,
    pub(crate) project_id: Ulid,
    pub(crate) tab_id: Ulid,
    pub(crate) key: PaneKey,
}

// ponytail: every Shell notify renders every pane view; notify only the pane of the session
// that changed if a profile shows Shell notifies dominate.
pub(crate) struct PaneView {
    shell: WeakEntity<Shell>,
    slot: PaneSlot,
    _shell_changed: Subscription,
}

impl PaneView {
    pub(crate) fn new(shell: &Entity<Shell>, slot: PaneSlot, cx: &mut Context<'_, Self>) -> Self {
        Self {
            shell: shell.downgrade(),
            slot,
            _shell_changed: cx.observe(shell, |_, _, cx| cx.notify()),
        }
    }
}

impl Render for PaneView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) -> impl IntoElement {
        let view = cx.entity_id();
        let slot = self.slot.clone();
        self.shell
            .update(cx, |shell, cx| shell.render_pane(&slot, view, window, cx))
            .unwrap_or_else(|_| gpui::Empty.into_any_element())
    }
}
```

- [ ] **Step 6: `Shell` holds the views and keeps pane-owned state at its frame start**

`main.rs`, in `struct Shell` next to `sidebar_view`:

```rust
    /// One cached view per pane slot (ADR 0114), made by `render_tab`, pruned by `sync_panes`.
    pane_views: RefCell<HashMap<shell::pane_view::PaneSlot, Entity<shell::pane_view::PaneView>>>,
```

`connection.rs`, next to `sidebar_view: None,`: `pane_views: RefCell::default(),`.

`main.rs`, replace `self.typing_target.field.replace(None);` in `Render for Shell` with:

```rust
        // A pane's field stays the target while its cached view reuses its paint; that view
        // clears it when it renders again (`render_pane`).
        let pane_owned = (self.typing_target.owner.get()).is_some_and(|owner| {
            (self.pane_views.borrow().values()).any(|view| view.entity_id() == owner)
        });
        if !pane_owned {
            self.typing_target.field.replace(None);
        }
```

- [ ] **Step 7: `render_pane` and the new `render_tab`** (`pane_render.rs`)

Import the new types the way `main.rs` reaches `shell::sidebar_view` (for example `use super::pane_view::{PaneSlot, PaneView};` at the top of `pane_render.rs`).
Add to `impl Shell`:

```rust
    /// The pane of `slot`, for its `PaneView` (ADR 0114): what `render_tab` used to pass to
    /// `pane_frame` is read from the slot's tab, the window and the `Shell`. A slot whose tab or
    /// leaf is gone draws nothing (`sync_panes` drops it).
    pub(crate) fn render_pane(
        &self,
        slot: &PaneSlot,
        view: gpui::EntityId,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) -> gpui::AnyElement {
        if let Some(session) = slot.key.session() {
            cx.global::<MenuAnchors>()
                .clear_pane(window.window_handle().window_id(), session);
        }
        if self.typing_target.owner.get() == Some(view) {
            self.typing_target.field.replace(None);
        }
        let tab = (self.tabs.get(&slot.project_id)).and_then(|tabs| tabs.tab(slot.tab_id));
        let Some(tab) = tab else {
            return gpui::Empty.into_any_element();
        };
        let rects = tab.rects();
        let Some(rect) = (rects.iter())
            .find(|(key, _)| *key == slot.key)
            .map(|(_, rect)| *rect)
        else {
            return gpui::Empty.into_any_element();
        };
        let theme = cx.theme().clone();
        let several = rects.len() > 1;
        let zoomed = tab.zoomed().is_some();
        let is_focused = tab.focused() == Some(&slot.key);
        // The panes' 0..1 space in px, as laid out in the last frame (`content_canvas`).
        let area = (self.hosts.get(&slot.host)).map_or(0.0, |ui| f32::from(ui.content.get().width));
        // Voice text goes to the focused terminal when it has the keyboard: ring it.
        let voice_ring = (slot.host == HostId::Main && is_focused)
            .then(|| self.voice_ring(&theme, cx))
            .flatten()
            .filter(|_| {
                (slot.key.session())
                    .and_then(|session| self.panes.get(&session))
                    .is_some_and(|pane| pane.read(cx).has_focus(window, cx))
            });
        self.pane_frame(
            slot.project_id,
            slot.key.clone(),
            several,
            zoomed,
            // Another pane above or below it: its composer goes compact unfocused.
            rect.h < 1.0 && !zoomed,
            rect.w as f32 * area,
            is_focused,
            is_focused && self.composer_focus.is_focused(window),
            voice_ring,
            status::now_ms(),
            &theme,
            window,
            cx,
        )
        .into_any_element()
    }
```

Change `render_tab`:
- New signature: `render_tab(&self, host: HostId, project_id: Ulid, tab: Option<&Tab>, now: i64, theme: &Theme, content: Rc<Cell<gpui::Size<Pixels>>>, window: &Window, cx: &mut Context<'_, Self>) -> gpui::Div`; drop `composer_focused` and `voice_ring` and their doc lines.
- Replace `if tab.and_then(|tab| tab.layout()).is_none() {` with `let Some(tab) = tab.filter(|tab| tab.layout().is_some()) else {` (the empty-state body stays and still ends in `.child(content_canvas(content));`, now as a `return ...;` followed by `};`).
- Replace everything from `let focused = tab.and_then(Tab::focused);` to the end of the `let panes: Vec<_> = ...collect();` statement with:

```rust
        let rects = tab.rects();
        let dividers = tab.dividers();
        let shell = cx.entity();
        let panes: Vec<_> = rects
            .into_iter()
            .map(|(key, rect)| {
                let slot = PaneSlot {
                    host,
                    project_id,
                    tab_id: tab.id,
                    key: key.clone(),
                };
                let view = (self.pane_views.borrow_mut())
                    .entry(slot.clone())
                    .or_insert_with(|| cx.new(|cx| PaneView::new(&shell, slot, cx)))
                    .clone();
                div()
                    .id(pane_id(&key))
                    .absolute()
                    .left(relative(rect.x as f32))
                    .top(relative(rect.y as f32))
                    .w(relative(rect.w as f32))
                    .h(relative(rect.h as f32))
                    .child(view.cached(gpui::StyleRefinement::default().size_full()))
            })
            .collect();
```

(`tab.dividers()` replaces `tab.map(Tab::dividers).unwrap_or_default()`; the `dividers` mapping below stays.)

In `pane_frame`, embed the grid cached: replace `Some(pane) => pane.clone().into_any_element(),` with:

```rust
                        // Cached: output renders the grid alone, and a pane render reuses it.
                        Some(pane) => pane
                            .clone()
                            .cached(gpui::StyleRefinement::default().size_full())
                            .into_any_element(),
```

- [ ] **Step 8: Update both `render_tab` callers**

`chrome.rs`: delete the `let voice_ring = ...;` statement above the call (it moved into `render_pane`), and call:

```rust
                        self.render_tab(
                            HostId::Main,
                            project_id,
                            tabs.active(),
                            now,
                            theme,
                            self.hosts
                                .get(&HostId::Main)
                                .map(|ui| ui.content.clone())
                                .unwrap_or_default(),
                            window,
                            cx,
                        )
```

`detached_render.rs`:

```rust
                self.render_tab(host, project, tab, now, &theme, content, window, cx)
                    .into_any_element()
```

- [ ] **Step 9: Prune views** (`focus.rs`, at the end of `sync_panes`)

```rust
        // A pane view lives while its tab, in its window, still holds its leaf.
        let live: HashSet<PaneSlot> = (self.tabs.iter())
            .flat_map(|(project_id, tabs)| {
                tabs.tabs().iter().flat_map(move |tab| {
                    tab.rects()
                        .into_iter()
                        .map(move |(key, _)| (*project_id, tab.id, key))
                })
            })
            .map(|(project_id, tab_id, key)| PaneSlot {
                host: self.terminal_host(project_id, tab_id),
                project_id,
                tab_id,
                key,
            })
            .collect();
        self.pane_views.get_mut().retain(|slot, _| live.contains(slot));
```

- [ ] **Step 10: Check every state change a cached grid could miss**

Run: `rg -n "\.update\(cx, \|pane, _\|" crates/mkrew-ui/src`
For each hit that changes something a `TerminalPane` draws, confirm a `cx.notify()` on that pane follows (as `dispatch` does for the `fed` panes); add one where it is missing, with a one-line comment.
Run: `rg -n "canvas\(" crates/mkrew-ui/src/shell/pane_render.rs crates/mkrew-ui/src/shell/tsp_view.rs crates/mkrew-ui/src/shell/chat_view.rs crates/mkrew-ui/src/shell/composer.rs crates/mkrew-ui/src/shell/file_pane.rs crates/mkrew-ui/src/shell/diff_pane.rs crates/mkrew-ui/src/shell/browser.rs`
For each recorder, write in the task report whether a reused paint keeps it right (its bounds did not change) or it needs clearing like `hits`; fix any that needs it the same way (clear it in the view that records it).

- [ ] **Step 11: Build, lint, test**

Run: `cargo clippy -p mkrew-ui --all-targets -- -D warnings`
Expected: no warnings.
Run: `cargo test -p mkrew-ui`
Expected: PASS.

- [ ] **Step 12: Commit** (add any file Step 10 changed)

```bash
git add crates/mkrew-ui/src/shell/pane_view.rs crates/mkrew-ui/src/shell.rs crates/mkrew-ui/src/main.rs crates/mkrew-ui/src/shell/connection.rs crates/mkrew-ui/src/shell/pane_render.rs crates/mkrew-ui/src/shell/chrome.rs crates/mkrew-ui/src/shell/detached_render.rs crates/mkrew-ui/src/shell/focus.rs crates/mkrew-ui/src/shell/menu_render.rs crates/mkrew-ui/src/ime.rs crates/mkrew-ui/src/shell/widgets.rs
git -c user.email=tungnv975@gmail.com commit -m "perf(ui): each pane is a cached view; the terminal grid renders alone (ADR 0114)"
```

---

### Task 4: Measurement with a long omp transcript (`tests/measure.rs`)

**Files:**
- Modify: `crates/mkrew-ui/tests/measure.rs`

**Interfaces:**
- Consumes: the existing helpers of `measure.rs` (`paths`, `sibling_exe`, `StopOnDrop`, `Probe`, `launch`, `wait_for`, `SETTLE`, `IDLE_WINDOW`) and its imports.

- [ ] **Step 1: Add the test**

```rust
/// Rows of the long omp transcript (ADR 0114).
const TSP_ROWS: usize = 2_000;

/// A TSP fixture for `mkrew-fakeagent --tsp`: an inline surface whose `main` gets `rows`
/// Markdown rows, 500 per frame.
fn long_tsp_fixture(rows: usize) -> String {
    let mut lines = vec![json!({"dir": "out", "verb": "o", "body": {
        "id": "s:1", "mode": "inline", "title": "omp", "role": "omp.session"
    }})];
    let mut ops = vec![
        json!(["add", "main", "s:1", null, {"id": "main", "k": "col"}]),
        json!(["add", "dock", "s:1", null, {"id": "dock", "k": "col"}]),
        json!(["add", "layer", "s:1", null, {"id": "layer", "k": "col"}]),
    ];
    let mut seq = 1;
    for i in 0..rows {
        let id = format!("r{i}");
        let text = format!(
            "### Turn {i}\n\nThe agent read `src/lib.rs` and **changed** three functions; \
             the tests pass.\n\n- first point\n- second point"
        );
        ops.push(json!(["add", id, "main", null, {"id": id, "k": "md", "p": {"text": text}}]));
        if ops.len() >= 500 || i + 1 == rows {
            lines.push(json!({"dir": "out", "verb": "f", "body": {
                "sf": "s:1", "s": seq, "ops": std::mem::take(&mut ops)
            }}));
            seq += 1;
        }
    }
    lines.iter().map(|line| format!("{line}\n")).collect()
}

/// A profile `id` running the built fake agent with `args` (a TOML array) and `extra` keys.
fn write_fake_profile_as(paths: &Paths, id: &str, args: &str, extra: &str) {
    let fake = sibling_exe("mkrew-fakeagent");
    let shipped = include_str!("../../../agents/fake.toml");
    let profile = shipped
        .replace(
            "command = \"mkrew-fakeagent\"",
            &format!("command = '{}'", fake.display()),
        )
        .replace("args = []", &format!("args = {args}\n{extra}"));
    assert!(profile.contains(args), "fake.toml's `args = []` line moved");
    let agents = paths.config_dir.join("agents");
    std::fs::create_dir_all(&agents).unwrap();
    std::fs::write(agents.join(format!("{id}.toml")), profile).unwrap();
}

#[test]
#[ignore = "opens a real window; run by scripts/measure.*"]
fn ui_cpu_with_a_long_omp_transcript_beside_a_printing_pane() {
    let dir = tempfile::tempdir().unwrap();
    let paths = paths(dir.path());
    let fixture = dir.path().join("long.tsp.jsonl");
    std::fs::write(&fixture, long_tsp_fixture(TSP_ROWS)).unwrap();
    let scenario = dir.path().join("spin.toml");
    std::fs::write(
        &scenario,
        "[[step]]\naction = \"spinner\"\ntext = \"Working\"\nms = 600000\n",
    )
    .unwrap();
    let quoted = |path: &Path| format!("'{}'", path.display());
    write_fake_profile_as(
        &paths,
        "printing",
        &format!("['--scenario', {}]", quoted(&scenario)),
        "",
    );
    write_fake_profile_as(
        &paths,
        "longtsp",
        &format!("['--tsp', {}]", quoted(&fixture)),
        "tsp = true",
    );
    let mut stop = StopOnDrop(paths.clone(), None);
    let project_dir = dir.path().join("proj");
    std::fs::create_dir(&project_dir).unwrap();

    let mut launcher =
        DaemonLauncher::new(paths.clone(), sibling_exe("mkrew-daemon")).without_notifications();
    let connection = connect_or_spawn(&mut launcher, TIMEOUT, POLL).unwrap();
    let project: Project = block_on(connection.call(Request::ProjectAdd(ProjectAddParams {
        path: project_dir.to_string_lossy().into_owned(),
        name: None,
    })))
    .unwrap();
    let tab: TerminalTab = block_on(connection.call(Request::TabCreate(TabCreateParams {
        project_id: project.id,
        title: "Agents".into(),
        position: None,
    })))
    .unwrap();
    let mut sessions = Vec::new();
    for profile in ["printing", "longtsp"] {
        let mut params =
            lifecycle::spawn_params(project.id, Some(tab.id), SessionKind::Agent, None);
        params.agent_profile = Some(profile.into());
        let spawned: SessionSpawnResult =
            block_on(connection.call(Request::SessionSpawn(params))).unwrap();
        sessions.push(spawned.session_id);
    }
    let pane = |id: Ulid| json!({ "session": id.to_string() });
    block_on(
        connection.call::<serde_json::Value>(Request::TabSetLayout(TabSetLayoutParams {
            tab_id: tab.id,
            layout: json!({"split": "vertical", "ratio": 0.5, "children": [
                pane(sessions[0]), pane(sessions[1]),
            ]}),
        })),
    )
    .unwrap();

    let launched = Instant::now();
    let (window, lines) = launch(
        Command::new(env!("CARGO_BIN_EXE_mkrew"))
            .env(ENV_DATA_DIR, &paths.data_dir)
            .env(ENV_CONFIG_DIR, &paths.config_dir)
            .env(ENV_SOCKET, &paths.socket)
            .env("MKREW_LOG", "info")
            .env("NO_COLOR", "1"),
    );
    let window_pid = window.id();
    stop.1 = Some(window);
    wait_for(&lines, "first connected frame", launched);
    // The fake omp says hello, gets the reply and sends its frames; the spinner keeps printing.
    std::thread::sleep(SETTLE * 2);
    let mut ui = Probe::new(window_pid);
    std::thread::sleep(IDLE_WINDOW);
    let (_, ui_cpu) = ui.sample();
    let verdict = if ui_cpu < 15.0 { "ok" } else { "OVER BUDGET" };
    eprintln!(
        "measure: long omp transcript ({TSP_ROWS} rows) beside a printing pane: ui \
         {ui_cpu:.1}% of a core over {IDLE_WINDOW:?} ({verdict}, budget 15%, ADR 0114)"
    );
}
```

- [ ] **Step 2: Build and run it**

Run (cwd `.worktrees/pane-perf`): `cargo build --release -p mkrew-daemon -p mkrew-fakeagent` then `cargo test --release -p mkrew-ui --test measure ui_cpu_with_a_long -- --ignored --nocapture`
Expected: one `measure: long omp transcript ...` line with verdict `ok`. Keep the number for ADR 0009 and ADR 0114 (Task 5).
Run the same test once on `master` (the `probe-before` worktree of Task 6, with this test copied in) for the "before" number, if Task 6 already made that worktree; else write "not measured" for "before" in Task 5.

- [ ] **Step 3: Lint and commit**

Run: `cargo clippy -p mkrew-ui --all-targets -- -D warnings`

```bash
git add crates/mkrew-ui/tests/measure.rs
git -c user.email=tungnv975@gmail.com commit -m "test(ui): measure UI CPU with a 2,000-row omp transcript beside a printing pane"
```

---

### Task 5: Records (ADR 0114, SPEC, ADR 0009, PROGRESS, HUMAN_CHECKS)

**Files:**
- Create: `docs/decisions/0114-pane-render-perf.md`
- Modify: `docs/SPEC.md` (§10 omp native surface bullet about `main`, near line 1260; §15 table)
- Modify: `docs/decisions/0009-non-functional-measurements.md`
- Modify: `docs/PROGRESS.md`, `docs/HUMAN_CHECKS.md`

Do this task after Task 6 Steps 1-4 (it needs their numbers and screenshots).

- [ ] **Step 1: ADR 0114**

Write `docs/decisions/0114-pane-render-perf.md` with this content, putting the numbers measured in Task 4 and Task 6 Step 3 into the "After" column (the file must hold no `<...>` text when committed):

```markdown
# 0114 - Cached pane views and a virtual omp transcript

- Date: 2026-11-25
- Status: accepted (amends SPEC §10 omp native surface and §15)
- Spec: `docs/superpowers/specs/2026-11-25-pane-render-perf-design.md`

## Context

The user reported that Mkrew felt choppy, typing included.
The UI thread of 0.9.0 used 46-47% of one core with about 20 live omp sessions.
In a sandbox with one printing pane and one omp pane holding a real 14 MB session, a frame took 52 ms on average (65 ms at most) and the UI used 70% of a core; 40 ms of each frame was layout of the whole transcript.

Two causes:

- GPUI marks every ancestor of a notifying view dirty, and nothing below `Shell` was cached, so every output frame, omp frame, status tick and key press laid out every pane again.
- The omp surface built every row of `main` inside one scrolling div.

## Decision

- Each pane is a `PaneView` (`shell/pane_view.rs`) embedded `cached`, keyed by window, workspace, tab and leaf.
  Like `SidebarView` it observes `Shell` and renders again on every `Shell` notify, on a window refresh, on new bounds and on a scroll inside it.
- The terminal grid is embedded `cached`: output renders only the grid.
- The omp transcript is a `gpui::list`.
  `Mirror::apply` records the rows each frame's ops name (before and after the ops run), `tsp::row_changes` turns them into splices and remeasures, and the list lays out only the rows on screen plus 600 px around them.
  It follows the end with `FollowMode::Tail`.
- State recorded while painting survives a reused frame: pane menu anchors and the voice typing target are cleared by the pane view that records them; text hit areas are cleared at every omp pane render.

## Measurements

| Scenario | Before | After |
|---|---|---|
| Frame time, printing pane + omp pane with a 14 MB session (mean / max) | <before mean> / <before max> ms | <after mean> / <after max> ms |
| UI CPU, same scenario | <before>% of a core | <after>% of a core |
| UI CPU, `measure.rs` 2,000-row transcript beside a printing pane | <before or "not measured"> | <after>% of a core |

## Consequences

- Typing in a long omp session or next to busy agents no longer waits for a whole-transcript layout.
- A state change that does not notify `Shell` (or the `TerminalPane` it belongs to) is not drawn until something else renders that pane; code that changes pane state must notify, as before.
- `ponytail:` on `PaneView`: every `Shell` notify still renders every pane view; notifying only the pane of the session that changed is the next step if a profile shows `Shell` notifies dominate.
- The Chat list is not virtualized (it holds at most 50 turns).
```

- [ ] **Step 2: SPEC**

In §10, in the omp native surface bullet (the sentence starting "`main` is a scrolling list that follows its end, with the scroll-to-end button"), after "with the scroll-to-end button" insert ", laid out only around what is on screen (ADR 0114)".
In §15, change the Input latency row's requirement cell to "key press → glyph on screen < 16 ms (local), also beside busy panes and in an omp pane with a 2,000-row transcript (ADR 0114)".

- [ ] **Step 3: ADR 0009** — add a row to its results table, in the same column format as the other rows (number from Task 4):

`| Long omp transcript (2,000 rows) beside a printing pane | UI < 15% of a core | <Task 4 number>% of a core | ok |`

- [ ] **Step 4: PROGRESS** — add one line at the end of its notes list:

`- 2026-11-25 pane render perf (spec docs/superpowers/specs/2026-11-25-pane-render-perf-design.md, plan docs/superpowers/plans/2026-11-25-pane-render-perf.md, ADR 0114): cached \`PaneView\` per pane slot, cached terminal grid, virtual omp transcript (\`tsp::row_changes\`); no protocol or migration change. Manual checks in HUMAN_CHECKS "2026-11-25 pane render performance".`

- [ ] **Step 5: HUMAN_CHECKS** — insert above "### To verify - 2026-11-25 omp composer edited by Mkrew (ADR 0113)":

```markdown
### To verify - 2026-11-25 pane render performance (ADR 0114)

Screenshots: `docs/screenshots/2026-11-25-pane-perf-<view>-<dark|light>.png`.

- [ ] **Typing in a long omp session:** open an omp session with a long history; type quickly in its composer; letters appear at once, no stutter.
- [ ] **Typing next to busy agents:** with two or three agents working in panes of the same tab, type in another pane; no stutter.
- [ ] **Scrolling a long omp transcript:** wheel and PageUp / PageDown through it; no blank rows, no rows on top of each other; while omp writes, the page stays at the end; scrolled up, it stays where you read and the round button brings you back.
- [ ] **Theme and font:** switch light and dark, then change the font size in Settings, while scrolled to the middle of a long omp transcript; the rows redraw at their new size without overlaps.
- [ ] **Voice input** (if you use it): start it in an omp composer and in a terminal pane; the text goes there and the ring shows.
```

- [ ] **Step 6: Commit**

```bash
git add docs/decisions/0114-pane-render-perf.md docs/SPEC.md docs/decisions/0009-non-functional-measurements.md docs/PROGRESS.md docs/HUMAN_CHECKS.md docs/screenshots/2026-11-25-pane-perf-*.png
git -c user.email=tungnv975@gmail.com commit -m "docs: pane render performance ADR 0114, SPEC §10/§15, measurements and checks"
```

---

### Task 6: Before/after frame time, end-to-end checks, full verify

**Files:**
- Create (throwaway, never merged): worktrees `.worktrees/probe-before` (from `master`) and `.worktrees/probe-after` (from `feat/pane-perf`)
- Create: `docs/screenshots/2026-11-25-pane-perf-<view>-<dark|light>.png` (committed in Task 5)

- [ ] **Step 1: The frame timer** (throwaway)

The timer lives uncommitted in `.worktrees/perf-probe/crates/mkrew-ui/src/perf.rs` (branch `perf/probe`, never merged).
Make `git worktree add -b perf/probe-before .worktrees/probe-before master` and `git worktree add -b perf/probe-after .worktrees/probe-after feat/pane-perf`, and in each:

- copy `perf.rs` into `crates/mkrew-ui/src/perf.rs` and add `pub mod perf;` to `lib.rs` after `pub mod pane_text;`;
- in `Render for Shell` (`main.rs`) add `mkrew_ui::perf::frame_start();` as the first line of `render`, and as the last child of the root `div()`:

```rust
            .child(
                gpui::canvas(|_, _, _| {}, |_, _, _, _| mkrew_ui::perf::frame_end())
                    .absolute()
                    .size_0(),
            )
```

- build with `cargo build --release -p mkrew-ui -p mkrew-daemon -p mkrew-cli -p mkrew-fakeagent --bins` (cwd the probe worktree). Both worktrees build into the main checkout's `target/release`: right after each build copy `mkrew.exe`, `mkrew-daemon.exe`, `mk.exe`, `mkrew-fakeagent.exe` (and `conpty.dll`, `OpenConsole.exe` if `scripts/smoke.ps1` asks for them) to `%TEMP%\mkrew-probe-before-bin` or `%TEMP%\mkrew-probe-after-bin`.

- [ ] **Step 2: The scenario** (the same for both builds; one sandbox at a time)

- In a PowerShell with `$env:MKREW_PERF = "$env:TEMP\mkrew-probe.log"` set, run `powershell -File scripts/smoke.ps1 start -Root $env:TEMP\mkrew-probe -Target <the build's exe folder>`, so the UI writes the timer log.
- Write `%TEMP%\mkrew-perf-spin.py` (12 redraws a second of 40 colored lines on the alt screen, like an agent TUI):

```python
import sys, time
out = sys.stdout
out.write("\x1b[?1049h\x1b[?25l")
i = 0
while True:
    buf = ["\x1b[H"]
    for r in range(40):
        buf.append(f"\x1b[{31 + (r + i) % 6}m{r:03d} {'lorem ipsum dolor sit amet ' * 3}{i:08d}\x1b[0m\x1b[K\r\n")
    buf.append(f"\x1b[1;36m{'|/-\\'[i % 4]} Working... {i}\x1b[0m\x1b[K")
    out.write("".join(buf))
    out.flush()
    i += 1
    time.sleep(1 / 12)
```

- Profile `<root>\config\agents\spin.toml`:

```toml
name = "Spin"
command = "python"
args = ["C:/Users/NguyenTung/AppData/Local/Temp/mkrew-perf-spin.py"]
prompt_mode = "type"
```

- Copy the user's long session `C:/Users/NguyenTung/.omp/agent/sessions/-Workspace-PersonalProject-mkrew-build-kit-mkrew/2026-11-21T08-33-30-452Z_01a1fd3d-3954-770f-9082-f9f8c6d9e4e3.jsonl` to `<root>\omp-sessions\long.jsonl` (never open or change the original), and write `<root>\config\agents\omplong.toml` (`<root>` with forward slashes; TOML reads backslashes as escapes):

```toml
name = "OmpLong"
command = "omp"
args = ["--session-dir", "<root>/omp-sessions", "--resume", "<root>/omp-sessions/long.jsonl"]
prompt_mode = "type"

[state]
mode = "hook+screen"
idle_after_ms = 3000

[usage]
format = "omp_jsonl"
context_window = 200000
```

- `smoke.ps1 mk -Root <root> session spawn api --agent spin`, then `... --agent omplong`; show the omp session with the sandbox's `mkrew.exe "mkrew://session/<omp session id>?project=<api project id>"` (same `MKREW_*` environment as the sandbox), so both panes show side by side in one tab, as in spec §1.
- Wait 15 s for omp to load its transcript; take a screenshot (`smoke.ps1 shot -Root <root> -Out <file>`) and look at it to confirm the scenario.

- [ ] **Step 3: Measure**

- UI CPU over 10 s: read `(Get-Process -Id <ui pid>).TotalProcessorTime` before and after `Start-Sleep 10`; % of a core = delta ms / 100.
- Frame time: the last lines of `%TEMP%\mkrew-probe.log` give `frame(render..paint)` per-second count, total ms per second and max; mean = total / count.
- Do it for the before build and the after build; these are the ADR 0114 numbers.
- Acceptance (spec §1): after mean frame < 8 ms and UI CPU < 15% of a core. If either misses, stop and report the numbers and the timer breakdown instead of tuning blindly.
- Length check on the after build: the same scenario with a fresh omp session that has written a few turns (`mk send` a short prompt) gives about the same frame time as the long one.

- [ ] **Step 4: End-to-end checks on the after build** (sandbox of Step 2; take screenshots with `smoke.ps1 shot`)

Check each by hand and write the result in the task report:

- The transcript follows the end while omp writes; scroll up with the wheel: it stays while rows arrive; the round button appears and brings the page back to the end.
- PageUp, PageDown, Home, End in the omp pane.
- Drag-select text in an omp Markdown row, Ctrl+C, paste it into the other pane: the same text.
- Switch light and dark from the title bar while scrolled to the middle: rows redraw without overlap; then change the font size in Settings: the same.
- Open a pane's menu from the keyboard: it opens under the pane's top edge.
- Move the tab to a new window and back (tab menu): both panes draw and update in both windows.
- Open a browser pane beside them (`+` row, Browser), resize the window, drag the divider, zoom the pane: the page stays inside its pane.
- Voice input, if a key is configured in the sandbox; else write "not checked, no key" in the report (the HUMAN_CHECKS item covers it).

Screenshots in dark and light of the split tab (`omp-split`) and of the omp pane scrolled up with the round button (`scrolled`): `docs/screenshots/2026-11-25-pane-perf-<view>-<dark|light>.png`.
Take the same views from the before build and compare: nothing may look different.

- [ ] **Step 5: Clean up**

- `smoke.ps1 stop -Root <root>` for every sandbox this task started.
- `git worktree remove --force .worktrees/probe-before`, `.worktrees/probe-after` and `.worktrees/perf-probe`; `git branch -D perf/probe-before perf/probe-after perf/probe`.
- Delete `%TEMP%\mkrew-perf*`, `%TEMP%\mkrew-probe*`, `%TEMP%\perf-shot*.png`, `%TEMP%\cpu.ps1`, `%TEMP%\mkp.sh`, `%TEMP%\winpost.ps1`.

- [ ] **Step 6: Full verify**

Run (cwd `.worktrees/pane-perf`): `"C:/Program Files/Git/bin/bash.exe" scripts/verify.sh`
Expected: `verify: OK`. If the test step runs past 600 s, run `cargo test --workspace -j 2` on its own.
Then do Task 5 and commit it.
Before reporting, run `git -C C:/Users/NguyenTung/Workspace/PersonalProject/mkrew-build-kit/mkrew status --short`; if it lists a file this plan touched, say so and do not restore it.

---

## Spec coverage

- Spec §3.1-3.6: Task 3. §4.1-4.6: Tasks 1-2. §5: Task 2 Step 5 (`hits`); the rest stays as it is. §6: the file lists of Tasks 1-5. §7: Tasks 1, 4 and 6. §8: ADR 0114 consequences (Task 5).
- Names across tasks: `row_of`, `rows_named`, `image_rows`, `RowChange`, `row_changes`, `take_dirty_rows` (Task 1) are the ones Task 2 calls; `PaneSlot`, `PaneView::new`, `render_pane`, `pane_views`, `clear_pane`, `TypingTarget::owner` are defined and used in Task 3.
