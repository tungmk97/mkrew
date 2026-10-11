# File viewer scroll bar and smooth large files Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** The Files viewer (editor and Markdown preview) gets a scroll bar on the right, and large files (a 6,000-line `.md` first of all) scroll as smoothly as in Zed.

**Architecture:** A new `scrollbar` module draws one vertical bar over the right edge of a scrolling element. It works with either a `uniform_list` `ScrollHandle` (the Files editor) or a `ListState` (the preview). The thumb maths are pure and tested; the element is a `canvas` with a hitbox and mouse listeners, because gpui-pre 0.3.7 has no scroll bar. The Markdown preview (Files and Docs) changes from one `overflow_y_scroll` div that lays out every block on every frame to a gpui `list` (a virtual list). It lays out only the blocks on screen and measures all heights once per page, so the bar is exact. The editor stops scanning the whole file for its widest line on every frame.

**Tech Stack:** Rust 2024, `gpui-pre =0.3.7` (imported as `gpui`): `list`/`ListState`, `uniform_list`/`ScrollHandle`, `canvas`, `Window::insert_hitbox`/`on_mouse_event`/`paint_quad`.

**Spec:** `docs/SPEC.md` §12A (Files view: editor + Markdown preview), §12 (Docs preview shares the page), §14 (design tokens), ADR 0021 (preview page: centered column ≤ 720 px), ADR 0028 (Files editor). User request 2026-10-10: "File › view file should have a scroll bar on the right" and "viewing a large file (checked with `.md`) lags and is hard to scroll; Zed handles it better".

## Global Constraints

- gpui stays `gpui-pre = "=0.3.7"`; no new crates (AGENTS.md, ADR 0001).
- Every color, spacing, radius and size comes from `crates/mkrew-ui/src/theme.rs`; no literals in components (SPEC §14, AGENTS.md).
- Spacing only from 4 · 8 · 12 · 16 · 24 · 32; radius 4 / 8 / 12 (SPEC §14).
- No `unwrap`/`expect` outside tests unless provably infallible, with a comment saying why (SPEC §17).
- `cargo fmt --all -- --check` and `cargo clippy --workspace --all-targets -- -D warnings` clean (AGENTS.md).
- No test calls a real agent or the network (AGENTS.md).
- Conventional Commits (`feat(ui): …`, `perf(ui): …`). Stage only the files the task names. Never push. Never stage `docs/INBOX.md`, `docs/PERF_REVIEW.md`, `docs/superpowers/plans/2026-10-10-inbox-perf.md` or `docs/superpowers/specs/…` (the human's uncommitted files).
- C: is nearly full. Run `df -h /c` before the first build. Never create a new target dir. Use `-j 4`.
- If a build fails with "Access is denied" on `target/debug/mkrew.exe`, the app is running. Rename the exe to `mkrew.locked-old.exe` (`mv`) and build again. Never kill the user's processes.

## Review Focus

1. **An agent rewrites the open `.md` while you read the middle of it (Files Preview or Docs).** Expected: the page updates and stays where you were (clamped if the file got shorter). Another file starts at the top. Pinned by Task 2 test `a_new_page_of_the_same_file_keeps_its_place_and_another_file_starts_at_the_top`.
2. **Resizing the window or dragging the session panel edge with a large `.md` preview open.** Each width change measures every block once more. Expected: the resize stays usable; the page and bar are correct afterwards. Pinned by Task 2's HUMAN_CHECKS line "Resize".
3. **A Mermaid diagram scrolled into view for the first time.** The image decodes asynchronously. Expected: no jump, and the thumb does not change size when it appears (its height comes from its known aspect ratio). Pinned by Task 2 Step 5 and its HUMAN_CHECKS line "Diagrams".
4. **Dragging the editor's thumb while the editor has focus, then moving the pointer over the text or out of the window.** Expected: only scrolling happens. No caret move, no selection, and the drag ends on mouse-up anywhere. Pinned by Task 1's HUMAN_CHECKS line "Drag".
5. **A file that fits on screen.** Expected: no bar, and a click at the far right of a line still places the caret. Pinned by Task 1 test `the_thumb_is_the_visible_share_of_the_track_and_follows_the_offset` (`None` when it fits) and its HUMAN_CHECKS line "Short file".

---

## File structure

| File | Change | Responsibility |
| --- | --- | --- |
| `crates/mkrew-ui/src/scrollbar.rs` | Create | Thumb maths (`Metrics`, `thumb`, `offset_for`), `Source` (uniform-list handle / list state), the `bar` element |
| `crates/mkrew-ui/src/lib.rs` | Modify | `pub mod scrollbar;`, crate doc |
| `crates/mkrew-ui/src/theme.rs` | Modify | `SCROLLBAR_WIDTH`, `SCROLLBAR_THUMB_WIDTH`, `SCROLLBAR_MIN_THUMB` |
| `crates/mkrew-ui/src/preview.rs` | Modify | `Page` (the preview's `ListState` + current page), `Wrap`, `page()`; Mermaid aspect ratio |
| `crates/mkrew-ui/src/editor.rs` | Modify | `longest` cache + `longest_line()` |
| `crates/mkrew-ui/src/main.rs` | Modify | Editor bar, `doc_page`/`file_page` fields, Docs/Files preview call sites; delete `preview_page`/`preview_blocks`; `render_editor` uses `longest_line()` and borrows the theme |
| `docs/SPEC.md` §12A | Modify | Scroll bar and virtual preview sentences |
| `docs/HUMAN_CHECKS.md` | Modify | New "To verify" section |
| `docs/PROGRESS.md` | Modify | One note line |
| `docs/superpowers/plans/2026-10-10-inbox-perf.md` | Modify (do not stage) | Mark its Task 6 B11 as done by this plan |

---

### Task 1: Scroll bar module, on the Files editor

**Files:**
- Create: `crates/mkrew-ui/src/scrollbar.rs`
- Modify: `crates/mkrew-ui/src/lib.rs` (module list; crate doc line 13)
- Modify: `crates/mkrew-ui/src/theme.rs` (after `CARET_WIDTH`, line 163)
- Modify: `crates/mkrew-ui/src/main.rs` (imports line 81; `Shell` field ~line 394; `Shell::new` ~line 774; end of `render_editor` ~lines 11762–11764)
- Modify: `docs/SPEC.md` §12A (editor bullet, lines 956–962)
- Modify: `docs/HUMAN_CHECKS.md` (new section after line 16)
- Test: `crates/mkrew-ui/src/scrollbar.rs` `mod tests`

**Interfaces:**
- Consumes: `theme::{radius, Theme}` (`Theme.subtle`, `Theme.text_muted` are `u32` RGB).
- Produces (later tasks rely on these exact names):
  - `pub struct scrollbar::Metrics { pub viewport: f32, pub content: f32, pub offset: f32 }` (`Clone, Copy, Debug, PartialEq`)
  - `pub fn scrollbar::thumb(metrics: Metrics, track: f32) -> Option<(f32, f32)>`: `(top, height)` in px from the track's top
  - `pub fn scrollbar::offset_for(top: f32, metrics: Metrics, track: f32) -> f32`
  - `pub enum scrollbar::Source { Handle(gpui::ScrollHandle), List(gpui::ListState) }` (`Clone, Debug`)
  - `pub type scrollbar::Drag = Rc<Cell<Option<f32>>>`
  - `pub fn scrollbar::bar(source: Source, drag: Drag, theme: &Theme) -> gpui::Div`: absolutely positioned over the right edge of its `relative` parent
  - `theme::SCROLLBAR_WIDTH`, `theme::SCROLLBAR_THUMB_WIDTH`, `theme::SCROLLBAR_MIN_THUMB` (`f32`)

**Background for the implementer:**
- gpui-pre 0.3.7 (`~/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/gpui-pre-0.3.7/src/`) has no scroll bar element. Building blocks:
  - `ScrollHandle::{offset, max_offset, bounds, set_offset}` (`elements/div.rs:4258-4397`): offset `y` is ≤ 0 (negative = scrolled down), `max_offset.y` ≥ 0, `bounds` = the viewport.
  - `ListState::{viewport_bounds, max_offset_for_scrollbar, scroll_px_offset_for_scrollbar, set_offset_from_scrollbar, scrollbar_drag_started, scrollbar_drag_ended}` (`elements/list.rs:739-801`).
- The editor is `uniform_list("file-code", …).track_scroll(&self.file_code_scroll)`. Its `ScrollHandle` is `self.file_code_scroll.0.borrow().base_handle` (public field, `uniform_list.rs:116`). `set_offset` writes the same `Rc` the list reads, so the next frame scrolls.
- `HitboxBehavior::BlockMouseExceptScroll` (`window.rs:907-933`): hitboxes behind the bar are not hovered, so the editor rows' hover-gated `on_mouse_down` (caret), the `IBeam` cursor and the editor div's `track_focus` ignore presses on the bar. The wheel still scrolls the list below.
- Drag: while the thumb is held, a Capture-phase `MouseMoveEvent` listener calls `cx.stop_propagation()`, so the editor's drag-select (`on_mouse_move` with `pressed_button == Left`) never sees those moves (`window.rs:5764-5777`: a stopped capture skips the bubble phase).

- [ ] **Step 1: Write the failing tests**

Create `crates/mkrew-ui/src/scrollbar.rs` with only the tests for now:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn at(offset: f32) -> Metrics {
        Metrics {
            viewport: 100.0,
            content: 400.0,
            offset,
        }
    }

    /// A quarter of the content is visible: the thumb is a quarter of the track and slides
    /// with the offset; past the ends it stays at the ends; content that fits draws no bar;
    /// a huge file still gets a thumb you can grab.
    #[test]
    fn the_thumb_is_the_visible_share_of_the_track_and_follows_the_offset() {
        assert_eq!(thumb(at(0.0), 200.0), Some((0.0, 50.0)));
        assert_eq!(thumb(at(150.0), 200.0), Some((75.0, 50.0)));
        assert_eq!(thumb(at(300.0), 200.0), Some((150.0, 50.0)));
        assert_eq!(thumb(at(900.0), 200.0), Some((150.0, 50.0)));
        assert_eq!(thumb(at(-10.0), 200.0), Some((0.0, 50.0)));
        let fits = Metrics {
            viewport: 100.0,
            content: 100.0,
            offset: 0.0,
        };
        assert_eq!(thumb(fits, 200.0), None);
        assert_eq!(thumb(at(0.0), 0.0), None, "no room for a bar");
        let huge = Metrics {
            viewport: 100.0,
            content: 1_000_000.0,
            offset: 0.0,
        };
        assert_eq!(
            thumb(huge, 200.0).map(|(_, height)| height),
            Some(theme::SCROLLBAR_MIN_THUMB)
        );
    }

    /// Dragging the thumb's top to `top` scrolls to the matching offset, clamped to both ends,
    /// and maps back exactly even with the minimum thumb.
    #[test]
    fn dragging_the_thumb_maps_back_to_the_offset_and_stops_at_the_ends() {
        assert_eq!(offset_for(75.0, at(0.0), 200.0), 150.0);
        assert_eq!(offset_for(-20.0, at(0.0), 200.0), 0.0);
        assert_eq!(offset_for(500.0, at(0.0), 200.0), 300.0);
        let huge = Metrics {
            viewport: 100.0,
            content: 1_000_000.0,
            offset: 500_000.0,
        };
        let (top, _) = thumb(huge, 200.0).unwrap();
        assert!((offset_for(top, huge, 200.0) - 500_000.0).abs() < 1.0);
        let fits = Metrics {
            viewport: 100.0,
            content: 100.0,
            offset: 0.0,
        };
        assert_eq!(offset_for(50.0, fits, 200.0), 0.0);
    }
}
```

In `crates/mkrew-ui/src/lib.rs`, add `pub mod scrollbar;` between `pub mod run_setup;` and `pub mod sidebar;`.

- [ ] **Step 2: Run the tests to see them fail**

Run: `df -h /c && cargo test -p mkrew-ui --lib scrollbar -j 4`
Expected: FAIL to compile: `Metrics`, `thumb`, `offset_for`, `theme` not found.

- [ ] **Step 3: Add the theme tokens**

In `crates/mkrew-ui/src/theme.rs`, after

```rust
/// Width of the Files editor's caret.
pub const CARET_WIDTH: f32 = 2.0;
```

add

```rust

/// The scroll bar over the right edge of the Files editor and the Markdown preview: its
/// grab area, the thumb drawn in it (round ends of `radius::SMALL`), and the shortest thumb
/// (a 2 MB file still gets one you can grab).
pub const SCROLLBAR_WIDTH: f32 = space::M;
pub const SCROLLBAR_THUMB_WIDTH: f32 = space::S;
pub const SCROLLBAR_MIN_THUMB: f32 = space::XL;
```

- [ ] **Step 4: Write the module**

Put this above the `#[cfg(test)]` block in `crates/mkrew-ui/src/scrollbar.rs`:

```rust
//! A vertical scroll bar over the right edge of a scrolling view (SPEC §12A): the Files
//! editor (a `uniform_list`) and the Markdown preview (a `list`). gpui-pre 0.3.7 has no scroll
//! bar element. The thumb maths ([`thumb`], [`offset_for`]) are pure and tested; [`bar`] draws
//! the thumb and turns presses and drags into scroll offsets.

use std::cell::Cell;
use std::rc::Rc;

use gpui::prelude::*;
use gpui::{
    Bounds, DispatchPhase, Div, Hitbox, HitboxBehavior, ListState, MouseButton, MouseDownEvent,
    MouseMoveEvent, MouseUpEvent, ScrollHandle, canvas, div, fill, point, px, rgb, size,
};

use crate::theme::{self, Theme, radius};

/// How far a view scrolls, in px.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Metrics {
    /// Height of the visible part.
    pub viewport: f32,
    /// Height of all the content.
    pub content: f32,
    /// How far it is scrolled down (0 = top).
    pub offset: f32,
}

/// The thumb in a `track` px tall bar: its top and height. `None` when everything fits.
pub fn thumb(metrics: Metrics, track: f32) -> Option<(f32, f32)> {
    let max = metrics.content - metrics.viewport;
    if max <= 0.0 || track <= 0.0 {
        return None;
    }
    let height = (track * metrics.viewport / metrics.content)
        .max(theme::SCROLLBAR_MIN_THUMB)
        .min(track);
    let top = (track - height) * (metrics.offset / max).clamp(0.0, 1.0);
    Some((top, height))
}

/// The scroll offset that puts the thumb's top at `top`, clamped to both ends.
pub fn offset_for(top: f32, metrics: Metrics, track: f32) -> f32 {
    let Some((_, height)) = thumb(metrics, track) else {
        return 0.0;
    };
    let room = track - height;
    if room <= 0.0 {
        return 0.0;
    }
    (top / room).clamp(0.0, 1.0) * (metrics.content - metrics.viewport)
}

/// What the bar scrolls.
#[derive(Debug, Clone)]
pub enum Source {
    /// A `uniform_list`: `UniformListScrollHandle.0.borrow().base_handle`.
    Handle(ScrollHandle),
    /// A `list`.
    List(ListState),
}

impl Source {
    /// From the last layout; all zero before the first one.
    fn metrics(&self) -> Metrics {
        match self {
            Source::Handle(handle) => {
                let viewport = f32::from(handle.bounds().size.height);
                Metrics {
                    viewport,
                    content: viewport + f32::from(handle.max_offset().y),
                    offset: -f32::from(handle.offset().y),
                }
            }
            Source::List(list) => {
                let viewport = f32::from(list.viewport_bounds().size.height);
                Metrics {
                    viewport,
                    content: viewport + f32::from(list.max_offset_for_scrollbar().y),
                    offset: -f32::from(list.scroll_px_offset_for_scrollbar().y),
                }
            }
        }
    }

    fn scroll_to(&self, offset: f32) {
        match self {
            Source::Handle(handle) => handle.set_offset(point(handle.offset().x, px(-offset))),
            Source::List(list) => list.set_offset_from_scrollbar(point(px(0.), px(-offset))),
        }
    }

    /// A list keeps its content height still while the thumb is held (items measured
    /// meanwhile would move the thumb away from the pointer).
    fn hold(&self, held: bool) {
        if let Source::List(list) = self {
            if held {
                list.scrollbar_drag_started();
            } else {
                list.scrollbar_drag_ended();
            }
        }
    }
}

/// Where the thumb was grabbed (px below its top) while it is dragged. The view keeps one per
/// bar, because the element is rebuilt every frame.
pub type Drag = Rc<Cell<Option<f32>>>;

/// The bar over the right edge of its `relative` parent. It draws only when the content is
/// taller than the view: a thumb in `subtle`, or `text_muted` while held. Dragging the thumb
/// scrolls; pressing the track jumps the thumb's middle there. The wheel still scrolls the
/// view below, and presses on the bar never reach it (no caret move, no selection).
pub fn bar(source: Source, drag: Drag, theme: &Theme) -> Div {
    let (idle, held) = (theme.subtle, theme.text_muted);
    let measured = source.clone();
    div()
        .absolute()
        .top_0()
        .right_0()
        .bottom_0()
        .w(px(theme::SCROLLBAR_WIDTH))
        .child(
            canvas(
                move |bounds, window, _| {
                    let metrics = measured.metrics();
                    let thumb = thumb(metrics, f32::from(bounds.size.height));
                    let hitbox: Option<Hitbox> = thumb.map(|_| {
                        window.insert_hitbox(bounds, HitboxBehavior::BlockMouseExceptScroll)
                    });
                    (metrics, thumb, hitbox)
                },
                move |bounds, (metrics, thumb, hitbox), window, _| {
                    let (Some((top, height)), Some(hitbox)) = (thumb, hitbox) else {
                        return;
                    };
                    let track = f32::from(bounds.size.height);
                    let inset = (theme::SCROLLBAR_WIDTH - theme::SCROLLBAR_THUMB_WIDTH) / 2.0;
                    let shape = Bounds::new(
                        point(bounds.left() + px(inset), bounds.top() + px(top)),
                        size(px(theme::SCROLLBAR_THUMB_WIDTH), px(height)),
                    );
                    let color = if drag.get().is_some() { held } else { idle };
                    window.paint_quad(fill(shape, rgb(color)).corner_radii(px(radius::SMALL)));
                    let origin = bounds.top();
                    window.on_mouse_event({
                        let (source, drag) = (source.clone(), Rc::clone(&drag));
                        move |event: &MouseDownEvent, phase, window, cx| {
                            if phase != DispatchPhase::Bubble
                                || event.button != MouseButton::Left
                                || !hitbox.is_hovered(window)
                            {
                                return;
                            }
                            let y = f32::from(event.position.y - origin);
                            let grab = if (top..top + height).contains(&y) {
                                y - top
                            } else {
                                height / 2.0
                            };
                            source.hold(true);
                            source.scroll_to(offset_for(y - grab, metrics, track));
                            drag.set(Some(grab));
                            cx.stop_propagation();
                            window.refresh();
                        }
                    });
                    // While held, every move in the window scrolls, even off the bar, and
                    // nothing below sees it (the editor would drag-select).
                    window.on_mouse_event({
                        let (source, drag) = (source.clone(), Rc::clone(&drag));
                        move |event: &MouseMoveEvent, phase, window, cx| {
                            let Some(grab) = drag.get() else {
                                return;
                            };
                            if phase != DispatchPhase::Capture {
                                return;
                            }
                            // Released outside the window: no mouse-up came.
                            if event.pressed_button != Some(MouseButton::Left) {
                                drag.set(None);
                                source.hold(false);
                                window.refresh();
                                return;
                            }
                            let y = f32::from(event.position.y - origin);
                            source.scroll_to(offset_for(y - grab, source.metrics(), track));
                            cx.stop_propagation();
                            window.refresh();
                        }
                    });
                    window.on_mouse_event(move |_: &MouseUpEvent, phase, window, _| {
                        if phase == DispatchPhase::Capture && drag.take().is_some() {
                            source.hold(false);
                            window.refresh();
                        }
                    });
                },
            )
            .size_full(),
        )
}
```

Notes:
- `drag` and `source` are moved into the paint closure; each listener clones what it needs first, so the closures stay `'static`.
- If the compiler rejects `fill(shape, rgb(color))`, write `fill(shape, gpui::Background::from(rgb(color)))` (`color.rs:960` has `From<Rgba> for Background`).

- [ ] **Step 5: Run the tests**

Run: `cargo test -p mkrew-ui --lib scrollbar -j 4`
Expected: PASS (2 tests).

- [ ] **Step 6: Put the bar on the Files editor**

`crates/mkrew-ui/src/main.rs`:

1. Imports: after `use mkrew_ui::run_setup::{self, RunSetup};` (line 81) add `use mkrew_ui::scrollbar;`.
2. `Shell` fields: after
   ```rust
       /// Keeps the current find match of the Files viewer in view.
       file_code_scroll: UniformListScrollHandle,
   ```
   add
   ```rust
       /// Where the Files editor's scroll bar thumb is held while it is dragged.
       file_code_drag: scrollbar::Drag,
   ```
3. `Shell::new`: after `file_code_scroll: UniformListScrollHandle::new(),` add `file_code_drag: scrollbar::Drag::default(),`.
4. `render_editor`, replace the end
   ```rust
               .child(self.typing(&self.file_editor_focus, Self::file_editor_key_down, cx))
               .child(list)
               .into_any_element()
   ```
   with
   ```rust
               .child(self.typing(&self.file_editor_focus, Self::file_editor_key_down, cx))
               .child(list)
               .child(scrollbar::bar(
                   scrollbar::Source::Handle(self.file_code_scroll.0.borrow().base_handle.clone()),
                   Rc::clone(&self.file_code_drag),
                   theme,
               ))
               .into_any_element()
   ```
   Check the `file-editor` div above has `.relative()`; if not, add `.relative()` after its `.flex_col()` so the bar covers its right edge from top to bottom.
5. Crate doc in `crates/mkrew-ui/src/lib.rs` line 13: replace `usage, and platform text input (IME, Unikey) for the fields and terminals, and the Run button.` with `usage, platform text input (IME, Unikey) for the fields and terminals, the Run button, and the scroll bar of the Files editor and the Markdown preview.` (Re-wrap at 100 columns.)

Run: `cargo clippy -p mkrew-ui --all-targets -j 4 -- -D warnings`
Expected: no warnings.

- [ ] **Step 7: SPEC and manual checks**

`docs/SPEC.md` §12A, editor bullet: after `or binary: a read-only notice and "Open in Zed".` (line 962) add ` A scroll bar on the right shows where the view is (only when the file is taller than the view): drag its thumb, or press the track to jump there.`

`docs/HUMAN_CHECKS.md`: insert after line 16 (the `_Things the agent could not verify…_` paragraph), before `### To verify - 2026-10-10 sidebar session rows`:

```markdown
### To verify - 2026-10-10 file viewer scroll bar and large files (plan docs/superpowers/plans/2026-10-10-file-viewer-scroll.md)

- [ ] **Editor bar (dark and light):** open a long source file in Files. A thin rounded thumb sits on the right edge; its size is the visible share; it moves as you scroll with the wheel, PgDn, Ctrl+End and Find (Enter).
- [ ] **Drag:** click in the text (the editor has focus), then drag the thumb up and down, also with the pointer over the text and outside the window, and release there. Only scrolling happens: the caret does not move, nothing gets selected, and the thumb goes back to its idle color on release. Pressing the track above or below the thumb jumps there.
- [ ] **Short file:** a file that fits on screen shows no bar; a click at the far right end of a long line still puts the caret there.
```

- [ ] **Step 8: Commit**

```bash
git add crates/mkrew-ui/src/scrollbar.rs crates/mkrew-ui/src/lib.rs crates/mkrew-ui/src/theme.rs crates/mkrew-ui/src/main.rs docs/SPEC.md docs/HUMAN_CHECKS.md
git commit -m "feat(ui): scroll bar on the Files editor"
```

---

### Task 2: Virtual Markdown preview with the scroll bar (Files and Docs)

**Files:**
- Modify: `crates/mkrew-ui/src/preview.rs` (imports lines 6–22; module doc lines 1–4; new `Page`/`Wrap`/`page` after `blocks`, line 45; `diagram_view` ~lines 275–285; tests)
- Modify: `crates/mkrew-ui/src/main.rs` (`Shell` field ~line 400; `Shell::new` ~line 773; `render_doc` `Mode::Preview` arm ~lines 11034–11037; `render_md_preview` ~lines 13417–13421; delete `preview_page` 17695–17718 and `preview_blocks` 17720–17756)
- Modify: `docs/SPEC.md` §12A (Markdown preview bullet, lines 968–975)
- Modify: `docs/HUMAN_CHECKS.md` (append to Task 1's section)
- Test: `crates/mkrew-ui/src/preview.rs` `mod tests`

**Interfaces:**
- Consumes: `scrollbar::{bar, Source::List, Drag}` (Task 1); `docs::Shown { blocks: Vec<Block>, … }` (`Shown::from_text(&str) -> Shown`, `#[derive(Debug, PartialEq)]`); `markdown::Block { kind, lines: Lines { start: u32, end: u32 } }`; `Shell::pick_lines(&mut self, project_id: Ulid, context: CommentContext, start: u32, end: u32, extend: bool, cx: &mut Context<'_, Self>)`.
- Produces:
  - `pub struct preview::Page` (`Default`, `Debug`) with `pub fn show(&self, path: &str, shown: &Rc<Shown>) -> bool` (`true` = the list was reset for a new page)
  - `pub type preview::Wrap = Rc<dyn Fn(usize, &Block, AnyElement) -> AnyElement>`
  - `pub fn preview::page(page: &Page, path: &str, shown: &Rc<Shown>, wrap: Option<Wrap>, theme: &Theme) -> Div`

**Background for the implementer:**
- Why it lags today: `preview_page` (main.rs:17697) is a `div().overflow_y_scroll()` with every block as a child. gpui lays out and prepaints every child on every frame, and every wheel event is a frame. Highlighted code blocks are many `StyledText` lines each, so a 6,000-line plan pays that per frame. Zed uses a virtual list.
- gpui `list(state, render_item)` (`elements/list.rs`) lays out only the items in view. `ListState::measure_all()` measures every item once after each `reset` or width change, so `max_offset_for_scrollbar` is exact. `reset(count)` clears the scroll position (list.rs:355-367). `scroll_to(ListOffset)` sets it again; an index past the end is clamped to the end (list.rs:660-666).
- The old page was a centered column of at most `DOC_PREVIEW_MAX_WIDTH` with `gap(space::M)` between blocks, `px(space::XL)`/`py(space::L)` around it, body size `doc::BODY` and line height `doc::LINE_HEIGHT` (ADR 0021). Every list item now carries that look itself: the column width, `pb(space::M)` as the gap (`space::L` for the last block), and `pt(space::L)` on the first.
- `preview::block` returns `None` for a text-less paragraph. Such items are an empty div with no padding (the old `filter_map` skipped them).
- Docs wraps each block so a click picks its lines for a comment (old `preview_blocks`). That becomes the `Wrap` closure, which reaches `Shell` through a `WeakEntity`.

- [ ] **Step 1: Write the failing test**

In `crates/mkrew-ui/src/preview.rs` `mod tests`, add after `use super::*;`:

```rust
    use gpui::ListOffset;

    fn page_of(text: &str) -> Rc<Shown> {
        Rc::new(Shown::from_text(text))
    }
```

and at the end of the module:

```rust
    /// Drawing the same page again (every frame) changes nothing. A new page of the same file
    /// (re-read after an agent wrote it, or edited) keeps the reader's place, clamped when it
    /// got shorter. Another file starts at the top.
    #[test]
    fn a_new_page_of_the_same_file_keeps_its_place_and_another_file_starts_at_the_top() {
        let page = Page::default();
        let long: String = (0..50).map(|i| format!("Paragraph {i}\n\n")).collect();
        let first = page_of(&long);
        assert!(page.show("a.md", &first));
        assert_eq!(page.state.item_count(), 50);
        page.state.scroll_to(ListOffset {
            item_ix: 30,
            offset_in_item: px(5.),
        });
        assert!(!page.show("a.md", &first), "the same page is not laid out again");
        assert!(page.show("a.md", &page_of(&long)));
        let top = page.state.logical_scroll_top();
        assert_eq!((top.item_ix, top.offset_in_item), (30, px(5.)));
        assert!(page.show("a.md", &page_of("# One\n\nTwo\n")));
        assert_eq!(page.state.item_count(), 2);
        assert_eq!(page.state.logical_scroll_top().item_ix, 2, "clamped to the end");
        page.state.scroll_to(ListOffset {
            item_ix: 1,
            offset_in_item: px(0.),
        });
        assert!(page.show("b.md", &page_of(&long)));
        assert_eq!(page.state.logical_scroll_top().item_ix, 0);
    }
```

- [ ] **Step 2: Run it to see it fail**

Run: `cargo test -p mkrew-ui --lib preview::tests::a_new_page -j 4`
Expected: FAIL to compile: `Page` not found.

- [ ] **Step 3: Write `Page`, `Wrap` and `page`**

`crates/mkrew-ui/src/preview.rs` imports: change `use std::cell::Cell;` to `use std::cell::{Cell, RefCell};`. Add `ListAlignment`, `ListState` and `list` to the `gpui::{…}` list. Change `use crate::theme::{self, Theme, font_size, radius, space, weight};` to `use crate::theme::{self, ActiveTheme, Theme, font_size, radius, space, weight};`, and add `use crate::scrollbar::{self, Source};`. (Let `cargo fmt` sort them.)

Module doc (lines 1–4): replace `the rendered preview, and the source view` with ``the rendered preview (a virtual list of blocks with a scroll bar, [`page`]), and the source view``.

After `pub fn blocks(…) { … }` (ends line 45) add:

```rust

/// The preview's virtual list and the page it shows; one per view (Docs, Files), kept across
/// frames.
#[derive(Debug)]
pub struct Page {
    state: ListState,
    drag: scrollbar::Drag,
    /// The file's path and the page last shown.
    shown: RefCell<Option<(String, Rc<Shown>)>>,
}

impl Default for Page {
    fn default() -> Self {
        Page {
            // Every block measured once per page: the scroll bar is exact, and only the blocks
            // on screen are laid out per frame.
            state: ListState::new(0, ListAlignment::Top, Pixels::ZERO).measure_all(),
            drag: scrollbar::Drag::default(),
            shown: RefCell::default(),
        }
    }
}

impl Page {
    /// Points the list at `shown`, the page of `path`. The same page again changes nothing.
    /// A new page of the same file (re-read, edited) keeps the scroll position, clamped to
    /// its blocks. Another file starts at the top. Returns whether the list was reset.
    pub fn show(&self, path: &str, shown: &Rc<Shown>) -> bool {
        let mut current = self.shown.borrow_mut();
        let same_file = match current.as_ref() {
            Some((file, page)) if file == path && Rc::ptr_eq(page, shown) => return false,
            Some((file, _)) => file == path,
            None => false,
        };
        let at = self.state.logical_scroll_top();
        self.state.reset(shown.blocks.len());
        if same_file {
            self.state.scroll_to(at);
        }
        *current = Some((path.to_owned(), Rc::clone(shown)));
        true
    }
}

/// Wraps the element of top-level block `index` (the Docs view makes blocks pickable).
pub type Wrap = Rc<dyn Fn(usize, &Block, AnyElement) -> AnyElement>;

/// The preview's page (ADR 0021): `shown`'s blocks in a centered column of at most
/// `DOC_PREVIEW_MAX_WIDTH`, in a virtual list (only the blocks on screen are laid out), with a
/// scroll bar on the right. `path` names the file, so another file starts at the top.
pub fn page(
    page: &Page,
    path: &str,
    shown: &Rc<Shown>,
    wrap: Option<Wrap>,
    theme: &Theme,
) -> Div {
    page.show(path, shown);
    let shown = Rc::clone(shown);
    let last = shown.blocks.len().saturating_sub(1);
    let blocks = list(page.state.clone(), move |index, _, cx| {
        let theme = cx.theme();
        let Some(block) = shown.blocks.get(index) else {
            return div().into_any_element();
        };
        // `self::block`: the local `block` shadows the module's fn.
        let Some(element) = self::block(block, &shown, theme) else {
            return div().into_any_element();
        };
        let element = match &wrap {
            Some(wrap) => wrap(index, block, element),
            None => element,
        };
        div()
            .w_full()
            .px(px(space::XL))
            .when(index == 0, |el| el.pt(px(space::L)))
            .pb(px(if index == last { space::L } else { space::M }))
            .child(
                div()
                    .w_full()
                    .max_w(px(theme::DOC_PREVIEW_MAX_WIDTH))
                    .mx_auto()
                    .text_size(px(theme::doc::BODY))
                    .line_height(relative(theme::doc::LINE_HEIGHT))
                    .child(element),
            )
            .into_any_element()
    })
    .size_full();
    div()
        .relative()
        .flex_1()
        .min_h_0()
        .child(blocks)
        .child(scrollbar::bar(
            Source::List(page.state.clone()),
            Rc::clone(&page.drag),
            theme,
        ))
}
```

If the borrow checker rejects `cx.theme()` held while `self::block(…, theme)` runs, clone it once per item: `let theme = &cx.theme().clone();`. If `theme` the local shadows the module `theme` in `theme::DOC_PREVIEW_MAX_WIDTH`, name the local `item_theme`.

- [ ] **Step 4: Run the test**

Run: `cargo test -p mkrew-ui --lib preview -j 4`
Expected: PASS (the new test and the existing preview tests).

- [ ] **Step 5: Mermaid diagrams keep their height before the image decodes**

`diagram_view` in `preview.rs`: replace

```rust
        .child(img(diagram.image(theme)).w(px(diagram.width)).max_w_full())
```

with

```rust
        // The image decodes after its first layout: the known ratio gives it its height at
        // once, so measured page heights (and the scroll bar) do not jump when it appears.
        .child(
            img(diagram.image(theme))
                .w(px(diagram.width))
                .max_w_full()
                .when(diagram.height > 0.0, |img| {
                    img.aspect_ratio(diagram.width / diagram.height)
                }),
        )
```

(`img.rs:350` keeps an explicit `aspect_ratio`; `Diagram.width`/`height` are `f32`, `diagram.rs:22-23`.)

- [ ] **Step 6: Use the page in Files and Docs, delete the old one**

`crates/mkrew-ui/src/main.rs`:

1. `Shell` fields: after
   ```rust
       /// The Docs list filter field.
       docs_filter_focus: FocusHandle,
   ```
   add
   ```rust
       /// The rendered Markdown page of Docs and of the Files view (virtual lists, kept
       /// across frames).
       doc_page: preview::Page,
       file_page: preview::Page,
   ```
2. `Shell::new`: after `docs_filter_focus: cx.focus_handle(),` add
   ```rust
               doc_page: preview::Page::default(),
               file_page: preview::Page::default(),
   ```
3. `render_md_preview`: replace
   ```rust
               Some(shown) => {
                   preview_page("file-preview", preview::blocks(&shown.blocks, shown, theme))
               }
   ```
   with
   ```rust
               Some(shown) => {
                   let path = view.file.as_deref().unwrap_or_default();
                   preview::page(&self.file_page, path, shown, None, theme).into_any_element()
               }
   ```
4. `render_doc`, `Doc::Shown(shown)` arm: replace
   ```rust
                   Mode::Preview => preview_page(
                       "doc-preview",
                       preview_blocks(project_id, shown, docs.comments.selection, theme, cx),
                   ),
   ```
   with
   ```rust
                   Mode::Preview => {
                       // A click picks the block's lines to comment on (Shift extends); picked
                       // blocks are highlighted.
                       let this = cx.entity().downgrade();
                       let selection = docs.comments.selection;
                       let (picked_bg, hover_bg) = (theme.terminal_selection_bg, theme.surface_raised);
                       let wrap: preview::Wrap = Rc::new(
                           move |index: usize,
                                 block: &mkrew_ui::markdown::Block,
                                 element: gpui::AnyElement| {
                               let (start, end) = (block.lines.start, block.lines.end);
                               let picked = selection.is_some_and(|s| s.contains(start, end));
                               let this = this.clone();
                               div()
                                   .id(("doc-block", index))
                                   .px(px(space::XS))
                                   .rounded(px(radius::SMALL))
                                   .cursor_pointer()
                                   .when(picked, |el| el.bg(rgb(picked_bg)))
                                   .when(!picked, |el| el.hover(move |style| style.bg(rgb(hover_bg))))
                                   .on_mouse_down(
                                       MouseButton::Left,
                                       move |event: &MouseDownEvent, _, cx| {
                                           let extend = event.modifiers.shift;
                                           let _ = this.update(cx, |shell, cx| {
                                               shell.pick_lines(
                                                   project_id,
                                                   CommentContext::Doc,
                                                   start,
                                                   end,
                                                   extend,
                                                   cx,
                                               );
                                           });
                                       },
                                   )
                                   .child(element)
                                   .into_any_element()
                           },
                       );
                       let path = docs.selected.as_deref().unwrap_or_default();
                       preview::page(&self.doc_page, path, shown, Some(wrap), theme)
                           .into_any_element()
                   }
   ```
   (`let _ =`: the update fails only when the Shell is gone, i.e. the window closed; nothing to do then.)
5. Delete the free functions `preview_page` (from `/// The preview's page: a scrolling, centered column of \`blocks\`` through its closing `}`, lines 17695–17718) and `preview_blocks` (from `/// The preview's top-level blocks, each picked by a click` through its closing `}`, lines 17720–17756).

Run: `cargo clippy -p mkrew-ui --all-targets -j 4 -- -D warnings`
Expected: no warnings. If clippy reports imports that are now unused in `main.rs` (e.g. `Selection` or `Shown`), remove only those names. `preview::blocks` may become unused outside `preview.rs`; if clippy reports it dead, delete it.

- [ ] **Step 7: SPEC and manual checks**

`docs/SPEC.md` §12A, Markdown preview bullet: after `are not clickable (ADR 0021).` (line 975) add ` The page is a virtual list (only the blocks on screen are laid out; every block's height is measured once per page) with the editor's scroll bar, so long files scroll smoothly; Docs shows its preview the same way. A re-read or edited page keeps the scroll position; another file starts at the top.`

`docs/HUMAN_CHECKS.md`: append to the section Task 1 added:

```markdown
- [ ] **Large Markdown (the reported lag):** in Files, open `docs/superpowers/plans/2026-10-10-inbox-perf.md` (about 6,400 lines). The preview shows within about a second. Wheel, PgDn and thumb drag scroll smoothly, side by side with Zed. The thumb reaches the very end and the last block is fully visible. Same file in Source: smooth too. Same in the Docs view.
- [ ] **Keeps its place:** scroll to the middle of a `.md` preview and let an agent (or Zed) save the file: the page updates and stays where it was. Open another `.md`: it starts at the top.
- [ ] **Resize:** with that large preview open, resize the window and drag the session panel's edge: it stays usable, and afterwards the column is centered and the thumb matches the position.
- [ ] **Diagrams:** a `.md` with a mermaid block lower down. Scroll to it: it appears without the page jumping; the thumb keeps its size.
- [ ] **Docs picking:** in the Docs preview, click a block: it highlights and the comment margin offers to comment on its lines; Shift+click extends; hover shows the hover band. Dark and light.
```

- [ ] **Step 8: Commit**

```bash
git add crates/mkrew-ui/src/preview.rs crates/mkrew-ui/src/main.rs docs/SPEC.md docs/HUMAN_CHECKS.md
git commit -m "perf(ui): virtual Markdown preview with a scroll bar in Files and Docs"
```

---

### Task 3: The editor stops scanning the whole file every frame

This is bullet B11 of `docs/superpowers/plans/2026-10-10-inbox-perf.md` Task 6. That plan is not executed yet. This task does B11 now (it is part of "large files lag") and marks it done there.

**Files:**
- Modify: `crates/mkrew-ui/src/editor.rs` (struct ~line 110; `Editor::new` ~lines 132–146; after `version()` line 169; `splice` line 492; new `widest_line` after `display_col` ~line 535; test)
- Modify: `crates/mkrew-ui/src/main.rs` (`render_editor` lines 11653–11708)
- Modify (do not stage): `docs/superpowers/plans/2026-10-10-inbox-perf.md` (Task 6, after line 2161)
- Modify: `docs/HUMAN_CHECKS.md` (append), `docs/PROGRESS.md` (one note)
- Test: `crates/mkrew-ui/src/editor.rs` `mod tests`

**Interfaces:**
- Consumes: `editor::display_col(line: &str, byte: usize) -> usize`, `highlight::TAB` (`"    "`, already imported in `editor.rs`), `Editor::{new, lines, set_cursor, insert, undo, cut}`.
- Produces: `Editor::longest_line(&self) -> usize`: the index of the widest line in screen cells, kept per text change. An editor always has at least one line, so it is a valid index.

**Background:** `render_editor` computes `longest` on every render: `lines.iter().enumerate().max_by_key(|(_, line)| editor::display_col(line, line.len()))` walks every char of the file (up to 2 MB) per frame. It also clones the whole `Theme` per render (`let row_theme = theme.clone();`). `Editor::splice` (line 482) is the only place that changes `lines` (it bumps `version`). Confirm before relying on it: `grep -n "self.lines" crates/mkrew-ui/src/editor.rs` must show no assignment or `push`/`insert`/`remove` outside `splice`.

- [ ] **Step 1: Write the failing test**

In `editor.rs` `mod tests`, add at the end:

```rust
    /// The widest row in screen cells (a tab is `TAB` cells, more than its one byte) follows
    /// every edit and undo, so the render never scans the file again.
    #[test]
    fn longest_line_is_the_widest_row_and_follows_edits() {
        let widest = |editor: &Editor| {
            let line = &editor.lines()[editor.longest_line()];
            display_col(line, line.len())
        };
        let tabs = 2 * TAB.len() + 1;
        let mut editor = Editor::new("short\n\t\tx\n12345678\n");
        assert_eq!(editor.longest_line(), 1, "two tabs and x are wider than 8 bytes");
        assert_eq!(widest(&editor), tabs);
        // A longer line typed at the end of the last row wins…
        editor.set_cursor(Pos::new(2, 8), false);
        editor.insert("9012345678");
        assert_eq!((editor.longest_line(), widest(&editor)), (2, 18));
        // …and undo gives the old widest row back.
        assert!(editor.undo());
        assert_eq!(widest(&editor), tabs);
        // Cutting the widest row's text leaves the next widest.
        editor.set_cursor(Pos::new(1, 0), false);
        editor.set_cursor(Pos::new(1, 3), true);
        editor.cut();
        assert_eq!((editor.longest_line(), widest(&editor)), (2, 8));
        // An empty file still has a line 0.
        assert_eq!(Editor::new("").longest_line(), 0);
    }
```

- [ ] **Step 2: Run it to see it fail**

Run: `cargo test -p mkrew-ui --lib editor::tests::longest_line -j 4`
Expected: FAIL to compile: no method `longest_line`.

- [ ] **Step 3: Cache it in `editor.rs`**

`Editor` struct: after
```rust
    /// Bumped by every text change (re-highlighting, find).
    version: u64,
```
add
```rust
    /// Index of the widest line in screen cells ([`Self::longest_line`]), found when the text
    /// changes instead of on every frame.
    longest: usize,
```
(If the `version` doc comment differs, add the field right after `version: u64,`.)

`Editor::new`: where `lines` is collected and the `Editor { lines, … }` literal starts, compute `let longest = widest_line(&lines);` just before the literal (add `::<Vec<String>>` to the `collect` if inference needs it), and add `longest,` in the literal after `version: 0,`.

`Editor::splice`: after `self.version += 1;` (line 492) add
```rust
        // ponytail: one scan per text change (it was one per frame); update from the replaced
        // lines only if typing in a 2 MB file ever feels slow.
        self.longest = widest_line(&self.lines);
```

After
```rust
    pub fn version(&self) -> u64 {
        self.version
    }
```
add
```rust

    /// Index of the widest line in screen cells (tabs and multibyte chars differ from the
    /// byte length), for the editor's horizontal scroll width.
    pub fn longest_line(&self) -> usize {
        self.longest
    }
```

After the free fn `display_col` (before `display_byte`) add
```rust

/// Index of the widest of `lines` in screen cells ([`display_col`] of its end); ties go to the
/// last one, an empty slice to 0.
fn widest_line(lines: &[String]) -> usize {
    lines
        .iter()
        .enumerate()
        .max_by_key(|(_, line)| display_col(line, line.len()))
        .map_or(0, |(index, _)| index)
}
```

Run: `cargo test -p mkrew-ui --lib editor -j 4`
Expected: PASS.

- [ ] **Step 4: `render_editor` uses the cache and borrows the theme**

In `main.rs` `render_editor`, replace
```rust
        // Widest row in screen cells (tabs and multibyte chars differ from the byte length).
        let longest = (lines.iter().enumerate())
            .max_by_key(|(_, line)| editor::display_col(line, line.len()))
            .map(|(index, _)| index);
        let text_left = Rc::clone(&self.file_text_left);
        let row_theme = theme.clone();
```
with
```rust
        // Widest row in screen cells, kept by the editor per text change.
        let longest = Some(shown.editor.longest_line());
        let text_left = Rc::clone(&self.file_text_left);
```
In the `cx.processor(move |shell, range: std::ops::Range<usize>, _, cx| {` closure, after
```rust
                let Some(shown) = view.shown() else {
                    return Vec::new();
                };
```
add `let theme = cx.theme();`. In the `preview::code_row(` call, replace the last argument `&row_theme,` with `theme,`.

Run: `cargo check -p mkrew-ui -j 4`
Expected: no errors. If the borrow checker rejects `cx.theme()` held across `cx.listener(...)` in that closure, use `let theme = &cx.theme().clone();` there instead (one clone per visible-range build, not per render) and say so in the report. If `lines` in `render_editor` is now used only for `total`, write `let total = shown.editor.lines().len();` and drop `lines`.

- [ ] **Step 5: Mark B11 done in the perf plan (do not stage that file)**

In `docs/superpowers/plans/2026-10-10-inbox-perf.md`, under `### Task 6: No settings.toml poll; the Files editor width is cached`, insert after the `Inbox: line 115 …` paragraph (line 2161):

```markdown

> **B11 is done** by `docs/superpowers/plans/2026-10-10-file-viewer-scroll.md` Task 3 (`Editor::longest_line`, `render_editor` borrows the theme). Skip the B11 steps and the B11 HUMAN_CHECKS line here; do only B8.
```

- [ ] **Step 6: Manual check and progress note**

`docs/HUMAN_CHECKS.md`: append to this plan's section:

```markdown
- [ ] **Large source file:** open a 1 MB or larger source file in Files (not Markdown). Type, hold an arrow key, scroll and drag the thumb: smooth, no per-frame stall. The horizontal scroll width still fits the widest line (a very long line scrolls fully; after deleting it the width shrinks; a line with tabs scrolls as wide as it looks).
```

`docs/PROGRESS.md` "Notes for the next session": after the `- 2026-10-10 compact sidebar …` line add:

```markdown
- 2026-10-10 file viewer scroll (plan docs/superpowers/plans/2026-10-10-file-viewer-scroll.md): new `scrollbar.rs` (pure thumb maths + canvas bar over a `uniform_list` handle or a `ListState`) on the Files editor and the Markdown preview. The preview (Files + Docs) is a gpui `list` (`preview::Page`, `measure_all`) instead of a div laying out every block per frame; `preview_page`/`preview_blocks` are gone. `Editor::longest_line` replaces the per-frame scan (perf plan Task 6 B11 marked done). UI only; manual checks in HUMAN_CHECKS "2026-10-10 file viewer scroll bar". Outside the milestone loop.
```

- [ ] **Step 7: Full verification and smoke run**

Run, in order:
1. `cargo fmt --all` then `cargo fmt --all -- --check`. Expected: no diff.
2. `cargo clippy --workspace --all-targets -j 4 -- -D warnings`. Expected: no warnings.
3. `cargo test -p mkrew-ui -j 4`. Expected: all pass. Run `bash scripts/verify.sh` if disk allows.
4. Smoke: `cargo build -p mkrew-ui --bin mkrew -j 4`, then launch `target/debug/mkrew.exe` as a named background service for about 15 s and stop only that service, never the user's daemon or agents. Expected: the window opens and stderr shows no panic. The visual checks wait for the human in HUMAN_CHECKS; say so in the report.

- [ ] **Step 8: Commit**

```bash
git add crates/mkrew-ui/src/editor.rs crates/mkrew-ui/src/main.rs docs/HUMAN_CHECKS.md docs/PROGRESS.md
git commit -m "perf(ui): Files editor keeps its widest line per text change"
```
