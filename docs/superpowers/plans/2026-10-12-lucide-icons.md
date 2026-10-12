# Lucide icons for icon buttons — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the Unicode glyphs of Mkrew's icon-only buttons (and the row disclosure chevrons) with Lucide SVG icons, give Close pane and Share an icon, and put a leading icon on the text buttons where it speeds scanning — tinted by the theme tokens, in dark and light.

**Architecture:** 48 Lucide SVG files are vendored in `icon/lucide/` and embedded with `include_bytes!` by a new `crates/mkrew-ui/src/icons.rs` (an `Icon` enum + an `AssetSource` registered with `Application::with_assets`). They are drawn with `gpui::svg().path(..)`. gpui paints an SVG as an alpha mask in the **svg element's own** `text_color`. The color is not inherited from the parent. So every icon is built by `icon_svg(icon, size, color)` with an explicit token color. The button helpers in `shell/widgets.rs` take an `Icon` (icon-only) or any `IntoElement` label. An icon beside text is `icon_text(icon, text, color)`, where `color` is the label color of that button (`button_fg(theme, primary)` for `button`).

**Tech Stack:** Rust, gpui (`gpui-pre` 0.3.7: `svg()`, `AssetSource`, `Application::with_assets`), Lucide (`lucide-static` 1.55.0, ISC).

**Spec:** `docs/SPEC.md` §14 (design system), §10 (title bar, tab strip, status bar, Run button), ADR 0027 (glyphs replaced SVG icons; this plan reverses that bullet with ADR 0051). The conversation decided: Lucide over gpui-kit icons (too heavy a dependency) and over Segoe Fluent Icons (Windows 11 only).

## Global Constraints

- Every color, size, spacing comes from `crates/mkrew-ui/src/theme.rs` (AGENTS.md, SPEC §14). No hex or raw px numbers in components.
- Dark and light must both work (SPEC §14).
- No new crate dependency. `include_bytes!` + gpui only.
- No test may use the network. Downloading the SVGs is a one-time dev step, not a test or build step.
- Deviating from the spec needs an ADR in `docs/decisions/` and a SPEC update in the same commit. The next free ADR number is **0051**.
- `cargo fmt`; `cargo clippy --workspace --all-targets -- -D warnings` clean. No `unwrap/expect` outside tests.
- Build with `-j 4` (low disk / RAM). In the omp shell run cargo and git directly and scripts via `sh` (`bash` is WSL).
- Git: stage explicit paths only; never push, never `reset`/`stash`/`clean`; Conventional Commits.
- Lucide version pinned: `https://unpkg.com/lucide-static@1.55.0/icons/<name>.svg`. Stroke width stays Lucide's `2`.

**Which text buttons get an icon (decided in the conversation, owned by Task 5):**
- *Icon only:* Close pane (it sits in the tab strip's icon row `+ ◫ ⋯`, has a tooltip with its shortcut).
- *Icon before the text:* frequent toolbar, header and card actions with a well-known icon — Share, Turns…, Show all panes, `+ New ▾`, idle `▶ Run` and the Run split `▾`, the workspace switcher `▾`, the empty states' New agent / New shell / Add workspace / New task / Open prompts folder, card Undo / Changes / Compare / Run with… / Several agents…, Comment, Send to agent, Open in Zed, Review in Docs, Save, Find, Discard…, Push, `✦ Generate`, `By: All ▾`, Open transcript, Link plan… / Unlink, Prompts…, Run setup Duplicate / Remove / `+ Add command` / `↑ Up`.
- *Text only:* dialog footers (Cancel, Close, Save, Send, Delete, Discard changes, Use this folder, Open composer, Show terminals, Auto-detect, Saving…, Sending…), Retry, empty-state navigation (Show changes, Working changes, Clear filter, Show all, Show ignored, Set base…, Open history), comment thread actions (Reply, View changes, Reopen, Resolve), Replace / Replace all, Reload / Keep mine, Restore before / Diff since, From prompt…, Change…, Clear (Esc), row actions (Stage, Unstage, Stage hunk, Review diff), the Ignored chip (ADR 0050), danger buttons (Stop, Discard changes). Reason: SPEC §14 "little chrome"; Linear / Zed keep these as text, and an icon on every button is noise.

**Out of scope (stay text glyphs):** status icons (§11 `◌ ••• ? ○ ✓ ✕ ·`, `status.rs`, `sidebar::status_display`), delivery/comment/error marks inside messages and notices (`✓ ✕ ⚠ ⧗ ↑ ⤢ …`), menu checkmarks (`✓`), the live Run label (`● Running 2/2 …`, `✕ Running …`: status marks) and the Run state text (`○ stopped`), the sidebar row's `▶` run indicator, the pin emoji, agent badges (`π`, `✻`…), the empty-state glyphs, and the diff `+`/`−` gutter.

## Review Focus

1. **Window close hover:** hovering the title bar's close button turns its background red and the X icon `on_accent` (white). The icon does not inherit the parent's hover text color, so `window_control` needs `group` + `group_hover` on the svg. Pinned by a HUMAN_CHECKS line in Task 6 (Task 2 owns the code).
2. **Primary ↔ ghost switching:** Save turns primary when the file is dirty, Compare when the card is selected, Run is primary while idle and quiet while live. The icon must switch with the label (`on_accent` on the accent fill, `text_muted` on ghost), or a dark icon sits on the blue fill. Task 5 computes the color from the same `primary` value through `button_fg`; pinned in HUMAN_CHECKS (Task 6).
3. **Maximize ↔ restore:** the maximize button shows `square` while windowed and `copy` (restore) while maximized. It flips on maximize, double-click on the title bar, and Snap. Pinned in HUMAN_CHECKS (Task 6).
4. **Larger `[ui] font_size` (e.g. 18):** icons scale with `rems_px`, stay centered in their buttons, beside their text, and the tree and git row chevrons stay aligned with the names. The Run split halves keep one height. Pinned in HUMAN_CHECKS (Task 6).
5. **A missing or broken SVG file:** gpui draws nothing (no error) when the asset source returns `None` or the bytes are not an SVG. The button is then blank and still clickable. Pinned by `every_icon_is_served_and_is_a_lucide_svg` in Task 1.

---

## File structure

| File | Change | Responsibility |
|---|---|---|
| `icon/lucide/*.svg`, `icon/lucide/LICENSE` | create | the 48 vendored Lucide icons + ISC license |
| `crates/mkrew-ui/src/icons.rs` | create | `Icon` enum, `Assets` (`AssetSource`), `icon_svg()` |
| `crates/mkrew-ui/src/lib.rs` | modify | `pub mod icons;` |
| `crates/mkrew-ui/src/theme.rs` | modify | `ICON_SIZE`, `ICON_SIZE_SMALL`; `mode_glyph` → `mode_icon` |
| `crates/mkrew-ui/src/main.rs` | modify | import `icons`; `.with_assets(icons::Assets)` |
| `crates/mkrew-ui/src/shell/widgets.rs` | modify | `icon_button`, `window_control`, `run_icon_button` take `Icon`; button labels `impl IntoElement` |
| `crates/mkrew-ui/src/shell/git_rows.rs` | modify | `row_action` label `impl IntoElement`; chevrons |
| `crates/mkrew-ui/src/shell/{chrome,sidebar,git_diff,git_menus,git_render,run,pane_render,files_render,composer,inspector,cards,comments,docs,task_dialog}.rs` | modify | call sites |
| `crates/mkrew-ui/src/terminal/view.rs` | modify | find bar close |
| `docs/decisions/0051-lucide-icons.md` | create | ADR |
| `docs/SPEC.md` | modify | §14 icons bullet |
| `docs/HUMAN_CHECKS.md` | modify | visual checks |

## Glyph → icon map (used by every task)

| Glyph | `Icon` | Lucide file | Where |
|---|---|---|---|
| `+` | `Plus` | `plus` | sidebar add workspace (×3), tab strip new tab |
| `▾` / `▸` | `ChevronDown` / `ChevronRight` | `chevron-down` / `chevron-right` | collapse all, git repos menu, find-bar replace toggle, row chevrons |
| `‹` / `›` | `ChevronLeft` / `ChevronRight` | `chevron-left` / `chevron-right` | git diff previous / next file |
| `⋯` | `Ellipsis` | `ellipsis` | git diff more, fetch running, tab menu, workspace row menu |
| `↓` (fetch) | `ArrowDownToLine` | `arrow-down-to-line` | git fetch |
| `↑` / `↓` (find) | `ArrowUp` / `ArrowDown` | `arrow-up` / `arrow-down` | Files find previous / next |
| `◷` | `History` | `history` | title bar History |
| `⌫` | `Delete` | `delete` | sidebar remove idle agents |
| `◐` `☀` `☾` | `SunMoon` `Sun` `Moon` | `sun-moon` `sun` `moon` | theme toggle |
| `─` `□` `❐` `✕` | `Minus` `Square` `Copy` `X` | `minus` `square` `copy` `x` | window controls |
| `✕` (button) | `X` | `x` | tab close, session remove, find-bar close (Files, terminal), unqueue |
| `⌕` | `Search` | `search` | Files search row, sidebar filter |
| `◫` | `Columns2` | `columns-2` | tab strip split |
| `◧` | `PanelLeft` | `panel-left` | status bar sidebar toggle |
| `⇤` / `⇥` | `PanelLeftClose` / `PanelLeftOpen` | `panel-left-close` / `panel-left-open` | status bar compact toggle |
| `◨` | `PanelRight` | `panel-right` | status bar session panel toggle |
| `↻` `■` `▶` | `RotateCw` `CircleStop` `Play` | `rotate-cw` `circle-stop` `play` | Run live buttons, Run menu rows |
| `✦` | `Sparkles` | `sparkles` | composer improve prompt, commit box Generate |

Icons beside text (Task 5) and Close pane:

| Button | `Icon` | Lucide file |
|---|---|---|
| Close pane (icon only) | `SquareX` | `square-x` |
| Share | `Share2` | `share-2` |
| Turns…, card Undo… | `History` / `Undo2` | `history` / `undo-2` |
| Show all panes | `Minimize2` | `minimize-2` |
| New agent / New shell | `Bot` / `SquareTerminal` | `bot` / `square-terminal` |
| Add workspace (empty sidebar) | `FolderPlus` | `folder-plus` |
| `+ New ▾`, New task, `+ Add command` | `Plus` (+ trailing `ChevronDown` for New) | `plus` |
| idle `▶ Run`, card Run with… | `Play` | `play` |
| Run split `▾`, switcher `▾`, `By: All ▾` | `ChevronDown` (after the text) | `chevron-down` |
| card Changes | `FileDiff` | `file-diff` |
| card Compare | `GitCompare` | `git-compare` |
| card Several agents… | `Users` | `users` |
| Comment | `MessageSquarePlus` | `message-square-plus` |
| Send to agent | `Send` | `send` |
| Open in Zed | `ExternalLink` | `external-link` |
| Review in Docs, Open transcript | `FileText` | `file-text` |
| Save | `Save` | `save` |
| Find | `Search` | `search` |
| Discard… | `Undo2` | `undo-2` |
| Push | `ArrowUpFromLine` | `arrow-up-from-line` |
| `✦ Generate` / `✦ Writing…` | `Sparkles` | `sparkles` |
| Link plan… / Unlink | `Link` / `Unlink` | `link` / `unlink` |
| Prompts… | `Library` | `library` |
| Open prompts folder | `FolderOpen` | `folder-open` |
| Run setup Duplicate / Remove | `Copy` / `Trash2` | `copy` / `trash-2` |
| Run setup `↑ Up` | `ArrowUp` | `arrow-up` |

---

### Task 1: Vendored icons, `icons.rs`, size tokens, asset registration

**Files:**
- Create: `icon/lucide/*.svg` (48 files), `icon/lucide/LICENSE`
- Create: `crates/mkrew-ui/src/icons.rs`
- Modify: `crates/mkrew-ui/src/lib.rs:42` (module list, alphabetical)
- Modify: `crates/mkrew-ui/src/theme.rs:91-92`
- Modify: `crates/mkrew-ui/src/main.rs:66` (imports), `:156` (application)
- Create: `docs/decisions/0051-lucide-icons.md`
- Modify: `docs/SPEC.md:1263` (§14 icons bullet)

**Interfaces:**
- Produces:
  - `mkrew_ui::icons::Icon`: `#[derive(Clone, Copy, Debug, PartialEq, Eq)] enum` with the 48 variants of the two maps above; `Icon::ALL: &'static [Icon]`; `fn path(self) -> &'static str` (`"icons/<file>.svg"`).
  - `mkrew_ui::icons::Assets`: unit struct, `impl gpui::AssetSource`.
  - `mkrew_ui::icons::icon_svg(icon: Icon, size: f32, color: u32) -> gpui::Svg`: `size` is a design px size (a `theme` constant), `color` a theme token (`u32` RGB).
  - `theme::ICON_SIZE: f32 = 16.0`, `theme::ICON_SIZE_SMALL: f32 = 14.0`.
  - `mkrew` binary: `use mkrew_ui::icons;` only (Task 2 adds `Icon`, `icon_svg` when they are first used, so no unused-import warning in between).

- [ ] **Step 1: Download the icons and license**

Run from the repo root:

```bash
mkdir -p icon/lucide
for n in arrow-down arrow-down-to-line arrow-up arrow-up-from-line bot chevron-down chevron-left chevron-right circle-stop columns-2 copy delete ellipsis external-link file-diff file-text folder-open folder-plus git-compare history library link message-square-plus minimize-2 minus moon panel-left panel-left-close panel-left-open panel-right play plus rotate-cw save search send share-2 sparkles square square-terminal square-x sun sun-moon trash-2 undo-2 unlink users x; do curl -fsSL "https://unpkg.com/lucide-static@1.55.0/icons/$n.svg" -o "icon/lucide/$n.svg" || echo "FAILED $n"; done
curl -fsSL https://unpkg.com/lucide-static@1.55.0/LICENSE -o icon/lucide/LICENSE
ls icon/lucide | wc -l
```

Expected: no `FAILED` line; `49` (48 SVGs + LICENSE). Each file starts with `<!-- @license lucide-static v1.55.0 - ISC -->` and has `viewBox="0 0 24 24"`, `stroke="currentColor"`.

- [ ] **Step 2: Write the failing test**

Create `crates/mkrew-ui/src/icons.rs` with only the test module (the rest comes in Step 4):

```rust
#[cfg(test)]
mod tests {
    use super::*;

    /// gpui draws nothing for a path the asset source does not serve, or for bytes that are not
    /// an SVG: the button would be blank but still clickable.
    #[test]
    fn every_icon_is_served_and_is_a_lucide_svg() {
        for &icon in Icon::ALL {
            let bytes = Assets
                .load(icon.path())
                .unwrap()
                .unwrap_or_else(|| panic!("{icon:?} is not served"));
            let text = std::str::from_utf8(&bytes).unwrap();
            assert!(text.contains("<svg"), "{icon:?}: not an SVG");
            assert!(
                text.contains("viewBox=\"0 0 24 24\""),
                "{icon:?}: not a 24 px Lucide icon"
            );
        }
        assert!(Assets.load("icons/missing.svg").unwrap().is_none());
    }
}
```

Add to `crates/mkrew-ui/src/lib.rs`, between `pub mod highlight;` and `pub mod history;`:

```rust
pub mod icons;
```

- [ ] **Step 3: Run the test to verify it fails**

Run: `cargo test -p mkrew-ui -j 4 --lib icons`
Expected: FAIL to compile with `cannot find type Icon` / `cannot find value Assets`.

- [ ] **Step 4: Write the implementation**

Put this above the test module in `crates/mkrew-ui/src/icons.rs`:

```rust
//! Lucide icons (ISC, `icon/lucide/`, ADR 0051) for icon buttons and row chevrons. One
//! monochrome file per icon: gpui paints an SVG as an alpha mask in the svg element's own
//! `text_color`, so [`icon_svg`] always sets it (the parent's color is not inherited).

use std::borrow::Cow;

use gpui::prelude::*;
use gpui::{AssetSource, SharedString, Svg, rgb, svg};

use crate::theme;

macro_rules! icons {
    ($($variant:ident => $file:literal,)*) => {
        /// A Lucide icon embedded in the binary.
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        pub enum Icon {
            $($variant,)*
        }

        impl Icon {
            pub const ALL: &'static [Icon] = &[$(Icon::$variant,)*];

            /// The asset path `svg().path(..)` asks [`Assets`] for.
            pub fn path(self) -> &'static str {
                match self {
                    $(Icon::$variant => concat!("icons/", $file, ".svg"),)*
                }
            }

            fn bytes(self) -> &'static [u8] {
                match self {
                    $(Icon::$variant => include_bytes!(concat!("../../../icon/lucide/", $file, ".svg")),)*
                }
            }
        }
    };
}

icons! {
    ArrowDown => "arrow-down",
    ArrowDownToLine => "arrow-down-to-line",
    ArrowUp => "arrow-up",
    ArrowUpFromLine => "arrow-up-from-line",
    Bot => "bot",
    ChevronDown => "chevron-down",
    ChevronLeft => "chevron-left",
    ChevronRight => "chevron-right",
    CircleStop => "circle-stop",
    Columns2 => "columns-2",
    Copy => "copy",
    Delete => "delete",
    Ellipsis => "ellipsis",
    ExternalLink => "external-link",
    FileDiff => "file-diff",
    FileText => "file-text",
    FolderOpen => "folder-open",
    FolderPlus => "folder-plus",
    GitCompare => "git-compare",
    History => "history",
    Library => "library",
    Link => "link",
    MessageSquarePlus => "message-square-plus",
    Minimize2 => "minimize-2",
    Minus => "minus",
    Moon => "moon",
    PanelLeft => "panel-left",
    PanelLeftClose => "panel-left-close",
    PanelLeftOpen => "panel-left-open",
    PanelRight => "panel-right",
    Play => "play",
    Plus => "plus",
    RotateCw => "rotate-cw",
    Save => "save",
    Search => "search",
    Send => "send",
    Share2 => "share-2",
    Sparkles => "sparkles",
    Square => "square",
    SquareTerminal => "square-terminal",
    SquareX => "square-x",
    Sun => "sun",
    SunMoon => "sun-moon",
    Trash2 => "trash-2",
    Undo2 => "undo-2",
    Unlink => "unlink",
    Users => "users",
    X => "x",
}

/// Serves the [`Icon`] files to gpui (`Application::with_assets`).
pub struct Assets;

impl AssetSource for Assets {
    fn load(&self, path: &str) -> gpui::Result<Option<Cow<'static, [u8]>>> {
        Ok(Icon::ALL
            .iter()
            .find(|icon| icon.path() == path)
            .map(|icon| Cow::Borrowed(icon.bytes())))
    }

    fn list(&self, path: &str) -> gpui::Result<Vec<SharedString>> {
        Ok(Icon::ALL
            .iter()
            .map(|icon| icon.path())
            .filter(|asset| asset.starts_with(path))
            .map(SharedString::from)
            .collect())
    }
}

/// `icon` as a `size` px square (a `theme` size, scaled with `[ui] font_size`) in `color`.
pub fn icon_svg(icon: Icon, size: f32, color: u32) -> Svg {
    svg()
        .path(icon.path())
        .flex_none()
        .size(theme::rems_px(size))
        .text_color(rgb(color))
}
```

In `crates/mkrew-ui/src/theme.rs`, replace lines 91-92:

```rust
/// A square icon-only button (theme toggle, sidebar +).
pub const ICON_BUTTON_SIZE: f32 = 26.0;
/// A Lucide icon in an icon-only button ([`ICON_BUTTON_SIZE`] square).
pub const ICON_SIZE: f32 = 16.0;
/// A Lucide icon among body / meta text: text buttons, row actions, row chevrons, window
/// controls, the status bar.
pub const ICON_SIZE_SMALL: f32 = 14.0;
```

In `crates/mkrew-ui/src/main.rs`, after `use mkrew_ui::history::{self, HistoryDialog, Listing, Log};` (line 66) add:

```rust
use mkrew_ui::icons;
```

and change line 156 from `gpui_platform::application().run(move |cx: &mut App| {` to:

```rust
    gpui_platform::application()
        .with_assets(icons::Assets)
        .run(move |cx: &mut App| {
```

(`cargo fmt` re-indents the closure body.)

- [ ] **Step 5: Run the test to verify it passes**

Run: `cargo test -p mkrew-ui -j 4 --lib icons`
Expected: PASS, `every_icon_is_served_and_is_a_lucide_svg ... ok`.

Run: `cargo build -p mkrew-ui -j 4`
Expected: builds, no new warning.

- [ ] **Step 6: Write the ADR (same commit as the change, AGENTS.md)**

`docs/decisions/0051-lucide-icons.md`:

```markdown
# 0051 — Icon buttons draw Lucide SVG icons

- Date: 2026-10-12
- Status: accepted (reverses ADR 0027's "SVG icons became Unicode glyphs" for icon buttons)

## Context

Icon-only buttons showed Unicode glyphs (`◷ ◧ ◫ ⋯ ⌕ ⇤ ✦ …`) in the UI font. Each glyph came from
a different fallback font, with different weights, sizes and baselines; some looked wrong on
Windows. The human asked for better-looking icons from an existing set, not hand-drawn ones.

## Decision

- Lucide (`lucide-static` 1.55.0, ISC; license in `icon/lucide/LICENSE`): 48 SVGs vendored in
  `icon/lucide/`, embedded with `include_bytes!` by `crates/mkrew-ui/src/icons.rs` (`Icon`) and
  served to gpui by `icons::Assets` (`Application::with_assets`).
- Drawn with `gpui::svg()` as a monochrome mask in a theme token color (`icon_svg`), so one file
  serves dark and light. Sizes `ICON_SIZE` 16 (icon-only buttons) and `ICON_SIZE_SMALL` 14
  (among text, rows, window controls, status bar), scaled with `[ui] font_size`. Stroke width is
  Lucide's 2.
- In scope: icon-only buttons, toggles, window controls, row actions, row chevrons; Close pane
  becomes an icon-only `square-x`. A 14 px icon goes before the text of frequent toolbar,
  header, card and empty-state actions: Share, Turns…, Show all panes, `+ New ▾`, idle Run and
  its `▾`, the workspace switcher `▾`, New agent / New shell / Add workspace / New task / Open
  prompts folder, card Undo / Changes / Compare / Run with… / Several agents…, Comment, Send to
  agent, Open in Zed, Review in Docs, Save, Find, Discard…, Push, Generate, `By: … ▾`, Open
  transcript, Link plan… / Unlink, Prompts…, Run setup Duplicate / Remove / Add command / Up.
- Text only: dialog footers, Retry, empty-state navigation, comment thread actions, Replace,
  Reload / Keep mine, Restore before / Diff since, row actions (Stage, Review diff), the Ignored
  chip (ADR 0050), danger buttons; status icons (§11), marks inside messages, menu checkmarks,
  the live Run label, agent badges and the pin. An icon on every button is noise (§14).
- Adding an icon: download `https://unpkg.com/lucide-static@1.55.0/icons/<name>.svg` into
  `icon/lucide/` and add one `Variant => "<name>",` line to the `icons!` list.

## Alternatives rejected

- `gpui-kit` (gpui-component) `Icon`: the same Lucide set behind a whole component library and its
  asset crate.
- Segoe Fluent Icons font: Windows 11 only; Linux has no such font.
- Better Unicode glyphs: still one font per glyph, still uneven.

## Consequences

The binary grows by about 25 KB. An SVG gpui cannot load draws nothing (blank button): the
`icons` test checks that every `Icon` is served and is a 24 px Lucide SVG.
```

- [ ] **Step 7: SPEC §14 bullet**

Insert after line 1263 (`  harmonized with the theme.`):

```markdown
- Icons (ADR 0051): icon-only buttons, toggles, window controls, row actions and row chevrons
  draw Lucide SVG icons (`icon/lucide/`, `crates/mkrew-ui/src/icons.rs`) in a theme token color,
  16 px in icon buttons and 14 px among text (`ICON_SIZE`, `ICON_SIZE_SMALL`). The glyphs this
  spec names for those buttons map to Lucide: `+` plus, `▾ ▸ ‹ ›` chevrons, `⋯` ellipsis, `◷`
  history, `⌫` delete, `◐ ☀ ☾` sun-moon / sun / moon, `─ □ ❐ ✕` minus / square / copy / x, `⌕`
  search, `◫` columns-2, `◧ ◨` panel-left / panel-right, `⇤ ⇥` panel-left-close / -open,
  `↻ ■ ▶` rotate-cw / circle-stop / play, `✦` sparkles, Fetch `↓` arrow-down-to-line. Close
  pane is an icon-only `square-x` in the tab strip. Frequent toolbar, header, card and
  empty-state actions carry a 14 px icon before their text (Share, `+ New`, idle Run, Save, Find,
  Comment, Send to agent, Open in Zed, Push, …; the list is in ADR 0051); a `▾` after a label
  is a chevron-down icon. Dialog footers, Retry, thread actions, row actions and danger buttons
  stay text. Status icons (§11), marks in messages and the live Run label stay text.
```

- [ ] **Step 8: Commit**

```bash
cargo fmt --all
git add icon/lucide crates/mkrew-ui/src/icons.rs crates/mkrew-ui/src/lib.rs crates/mkrew-ui/src/theme.rs crates/mkrew-ui/src/main.rs docs/decisions/0051-lucide-icons.md docs/SPEC.md
git commit -m "feat(ui): embed Lucide icons and serve them to gpui (ADR 0051)"
```

If `docs/SPEC.md` holds uncommitted changes that are not from this plan, stage only this plan's hunk (`git add -p docs/SPEC.md`).

---

### Task 2: Icon button helpers take `Icon`; migrate their callers

**Files:**
- Modify: `crates/mkrew-ui/src/shell/widgets.rs:83-284` (`button_frame`, `button`, `run_icon_button`, `secondary_button`, `quiet_button`, `disabled_button`, `danger_button`, `window_control`, `icon_button`)
- Modify: `crates/mkrew-ui/src/shell/git_rows.rs:411-432` (`row_action`)
- Modify: `crates/mkrew-ui/src/theme.rs:829-836` (`mode_glyph` → `mode_icon`)
- Modify callers: `shell/chrome.rs:137,150,168-191`, `shell/git_diff.rs:186-211,427`, `shell/git_menus.rs:120`, `shell/git_render.rs:299-306`, `shell/run.rs:318-328`, `shell/sidebar.rs:44,60,72-76,92,353`
- Modify: `crates/mkrew-ui/src/main.rs:66` (import `Icon`, `icon_svg`)

**Interfaces:**
- Consumes: `Icon`, `icon_svg`, `theme::ICON_SIZE`, `theme::ICON_SIZE_SMALL` (Task 1).
- Produces:
  - `icon_button(id: impl Into<gpui::ElementId>, glyph: Icon, theme: &Theme) -> gpui::Stateful<gpui::Div>`
  - `window_control(id: &'static str, glyph: Icon, area: WindowControlArea, theme: &Theme) -> gpui::Stateful<gpui::Div>`
  - `run_icon_button(id: &'static str, glyph: Icon, action: RunAction, theme: &Theme) -> gpui::Stateful<gpui::Div>`
  - `button`, `quiet_button`, `secondary_button`, `danger_button`, `button_frame`: `label: impl IntoElement` (was `impl Into<SharedString>`); `disabled_button` and `row_action`: `label: impl IntoElement` (was `&'static str`). Text callers are unchanged (`&'static str`, `String`, `SharedString` are all `IntoElement`).
  - `theme::mode_icon(mode: ThemeMode) -> crate::icons::Icon`.

There is no new permanent test in this task. The change is wiring, and the rules forbid wiring tests. The compiler and Task 6's visual checks cover it.

- [ ] **Step 0: Bring `Icon` and `icon_svg` into the binary's scope**

In `crates/mkrew-ui/src/main.rs`, change Task 1's `use mkrew_ui::icons;` to:

```rust
use mkrew_ui::icons::{self, Icon, icon_svg};
```

Every `shell/*.rs` file sees them through `use crate::*`.

- [ ] **Step 1: Change the helpers in `widgets.rs`**

`button_frame` (lines 83-100):

```rust
/// The shared frame of [`button`], [`quiet_button`] and [`danger_button`]: no colors, no hover
/// (gpui allows one `hover` per element). `label` is text or an [`icon_svg`].
pub(crate) fn button_frame(
    id: &'static str,
    label: impl IntoElement,
) -> gpui::Stateful<gpui::Div> {
    div()
        .id(id)
        .flex()
        .items_center()
        .gap(px(space::S))
        .px(px(space::S))
        .py(px(space::XS))
        .rounded(px(radius::SMALL))
        .cursor_pointer()
        .text_size(font_size::BODY)
        .child(label)
}
```

In `button` (line 105), `secondary_button` (138), `quiet_button` (154), `danger_button` (192), change `label: impl Into<SharedString>,` to `label: impl IntoElement,`. Bodies stay the same.

`run_icon_button` (lines 121-133):

```rust
/// A ghost icon button of a live Run: the icon in the action's color, its tint on hover.
pub(crate) fn run_icon_button(
    id: &'static str,
    glyph: Icon,
    action: RunAction,
    theme: &Theme,
) -> gpui::Stateful<gpui::Div> {
    let colors = run::action_colors(action, theme);
    button_frame(id, icon_svg(glyph, theme::ICON_SIZE_SMALL, colors.fg))
        .hover(move |style| style.bg(rgb(colors.tint)))
        .active(|style| style.opacity(opacity::ACTIVE))
}
```

`disabled_button` (lines 165-186): change `label: &'static str,` to `label: impl IntoElement,`. The body is unchanged (`.child(label)`).

`window_control` (lines 232-262):

```rust
/// One minimize / maximize / close button of the Windows title bar. `area` makes Windows treat
/// it as the system caption button (hover shows here; the click and the Snap Layouts flyout are
/// the system's). The close icon turns `on_accent` on the red hover through the `id` group: an
/// svg does not inherit its parent's hover color.
pub(crate) fn window_control(
    id: &'static str,
    glyph: Icon,
    area: WindowControlArea,
    theme: &Theme,
) -> gpui::Stateful<gpui::Div> {
    let close = matches!(area, WindowControlArea::Close);
    let (hover, on_hover) = (theme.hover, theme.on_accent);
    let red = theme.status_exited;
    div()
        .id(id)
        .group(id)
        .window_control_area(area)
        .flex_none()
        .w(theme::rems_px(theme::WINDOW_CONTROL_WIDTH))
        .h_full()
        .flex()
        .items_center()
        .justify_center()
        .hover(move |style| style.bg(rgb(if close { red } else { hover })))
        .child(
            icon_svg(glyph, theme::ICON_SIZE_SMALL, theme.text_muted).when(close, |icon| {
                icon.group_hover(id, move |style| style.text_color(rgb(on_hover)))
            }),
        )
}
```

`icon_button` (lines 264-284):

```rust
/// A square ghost button holding one icon (theme toggle, sidebar +).
pub(crate) fn icon_button(
    id: impl Into<gpui::ElementId>,
    glyph: Icon,
    theme: &Theme,
) -> gpui::Stateful<gpui::Div> {
    let hover = theme.hover;
    div()
        .id(id)
        .flex_none()
        .size(theme::rems_px(theme::ICON_BUTTON_SIZE))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(radius::SMALL))
        .cursor_pointer()
        .hover(move |style| style.bg(rgb(hover)))
        .active(|style| style.opacity(opacity::ACTIVE))
        .child(icon_svg(glyph, theme::ICON_SIZE, theme.text_muted))
}
```

- [ ] **Step 2: `row_action` label in `git_rows.rs` (lines 411-432)**

Change the doc and signature. The body is unchanged:

```rust
/// A small action inside a list row ("Stage" on a file row, "Stage hunk" on a hunk header, or
/// an [`icon_svg`]): no vertical padding, so the row keeps its height.
pub(crate) fn row_action(
    id: SharedString,
    label: impl IntoElement,
    theme: &Theme,
) -> gpui::Stateful<gpui::Div> {
```

- [ ] **Step 3: `mode_icon` in `theme.rs` (lines 829-836)**

```rust
/// The toggle's icon: sun and moon (follows the OS) / sun / moon.
pub fn mode_icon(mode: ThemeMode) -> crate::icons::Icon {
    use crate::icons::Icon;
    match mode {
        ThemeMode::System => Icon::SunMoon,
        ThemeMode::Light => Icon::Sun,
        ThemeMode::Dark => Icon::Moon,
    }
}
```

- [ ] **Step 4: Migrate the callers**

`shell/chrome.rs`:
- line 137: `icon_button("toggle-theme", theme::mode_icon(theme.mode), theme)`
- line 150: `icon_button("history", Icon::History, theme)`
- line 168: `let max_icon = if window.is_maximized() { Icon::Copy } else { Icon::Square };`
- line 176: `Icon::Minus,` · line 182: `max_icon,` · line 188: `Icon::X,`
- doc line 63: `(Agent / Shell) and Run; on Windows ─ □ ✕.` → `(Agent / Shell) and Run; on Windows minimize, maximize, close.`

`shell/git_diff.rs`:
- line 186: `let nav = |id: &'static str, glyph: Icon, tip: &'static str, keys, key| {`
- line 200: `Icon::ChevronLeft,` · line 207: `Icon::ChevronRight,`
- line 427: `icon_button("git-diff-more", Icon::Ellipsis, theme)`

`shell/git_menus.rs` line 120: `icon_button("git-repos-menu", Icon::ChevronDown, theme)`

`shell/git_render.rs`:
- line 299: `Some(Remote::Fetch) => icon_button("git-fetch", Icon::Ellipsis, theme)`
- lines 302 and 306: `icon_button("git-fetch", Icon::ArrowDownToLine, theme)`

`shell/run.rs` lines 318-325:

```rust
            for (id, glyph, tip, action) in [
                (
                    "run-restart",
                    Icon::RotateCw,
                    "Restart all run commands",
                    RunAction::Restart,
                ),
                ("run-stop", Icon::CircleStop, "Stop all run commands", RunAction::Stop),
            ] {
                bar = bar.child(
                    run_icon_button(id, glyph, action, theme)
```

`shell/sidebar.rs`:
- lines 44, 92, 353: `icon_button("add-workspace", Icon::Plus, theme)`
- line 60: `icon_button("remove-idle-agents", Icon::Delete, theme)`
- line 74: `if any_open { Icon::ChevronDown } else { Icon::ChevronRight },`

- [ ] **Step 5: Build**

Run: `cargo build -p mkrew-ui -j 4`
Expected: builds. A `the trait IntoElement is not implemented for X` error at a text-button call site (a `Cow<str>`, `&String`, `Arc<str>` label) is fixed by wrapping that argument in `SharedString::from(…)`, nothing else. A `mismatched types: expected Icon, found &str` error is a call site this step missed: map it with the glyph table.

Run: `cargo test -p mkrew-ui -j 4`
Expected: PASS (no test pins the changed glyphs).

- [ ] **Step 6: Commit**

```bash
cargo fmt --all
git add crates/mkrew-ui/src/main.rs crates/mkrew-ui/src/shell/widgets.rs crates/mkrew-ui/src/shell/git_rows.rs crates/mkrew-ui/src/theme.rs crates/mkrew-ui/src/shell/chrome.rs crates/mkrew-ui/src/shell/git_diff.rs crates/mkrew-ui/src/shell/git_menus.rs crates/mkrew-ui/src/shell/git_render.rs crates/mkrew-ui/src/shell/run.rs crates/mkrew-ui/src/shell/sidebar.rs
git commit -m "feat(ui): icon buttons and window controls draw Lucide icons"
```

---

### Task 3: The remaining glyph-only buttons and toggles

**Files:**
- Modify: `crates/mkrew-ui/src/shell/pane_render.rs:103-107,173-196`
- Modify: `crates/mkrew-ui/src/shell/sidebar.rs:148-157,454,478,678-682,822-826`
- Modify: `crates/mkrew-ui/src/shell/files_render.rs:54,407-413,460-481`
- Modify: `crates/mkrew-ui/src/shell/composer.rs:647-662,729-732`
- Modify: `crates/mkrew-ui/src/shell/run.rs:384-434`
- Modify: `crates/mkrew-ui/src/shell/inspector.rs:74`
- Modify: `crates/mkrew-ui/src/terminal/view.rs:609-619` (+ its `use` block)

**Interfaces:**
- Consumes: `Icon`, `icon_svg`, `theme::ICON_SIZE_SMALL` (Task 1); `icon_button(id, Icon, theme)`, `row_action(id, impl IntoElement, theme)`, `quiet_button`/`disabled_button` with `impl IntoElement` labels (Task 2).
- Produces: nothing new.

There is no new permanent test (wiring only). Build, the existing suite, and Task 6's visual checks cover it.

- [ ] **Step 1: Tab strip (`pane_render.rs`)**

Line 105 (tab close `row_action`): `icon_svg(Icon::X, theme::ICON_SIZE_SMALL, theme.text_muted),`

Lines 174, 181, 189. These ghost text buttons become square icon buttons (same 26 px height as the text buttons they replace):

```rust
                icon_button("new-tab", Icon::Plus, theme)
```
```rust
                icon_button("tab-split", Icon::Columns2, theme)
```
```rust
                    icon_button("tab-menu-button", Icon::Ellipsis, theme)
```

- [ ] **Step 2: Sidebar (`sidebar.rs`)**

Line 157 (filter field icon; the wrapper keeps its position styles):

```rust
                            .child(icon_svg(Icon::Search, theme::ICON_SIZE_SMALL, theme.text_muted)),
```

Line 454 (status bar sidebar toggle): `.child(icon_svg(Icon::PanelLeft, theme::ICON_SIZE_SMALL, theme.text_muted))`

Line 478 (compact toggle):

```rust
            .child(icon_svg(
                if compact { Icon::PanelLeftOpen } else { Icon::PanelLeftClose },
                theme::ICON_SIZE_SMALL,
                theme.text_muted,
            ))
```

Line 680 (workspace row menu): `icon_svg(Icon::Ellipsis, theme::ICON_SIZE_SMALL, theme.text_muted),`
Line 824 (session remove): `icon_svg(Icon::X, theme::ICON_SIZE_SMALL, theme.text_muted),`

The two toggles at 454/478 are `div`s with `.px(px(space::XS))` and no fixed height. Add `.flex().items_center()` to both (after `.flex_none()`) so the svg centers in the status bar like the text did.

- [ ] **Step 3: Files (`files_render.rs`)**

Line 54: `.child(icon_svg(Icon::Search, theme::ICON_SIZE_SMALL, theme.text_muted))`

Lines 408-413:

```rust
                        icon_button(
                            "file-find-toggle-replace",
                            if find.replace.is_some() {
                                Icon::ChevronDown
                            } else {
                                Icon::ChevronRight
                            },
                            theme,
                        )
```

Lines 461, 469, 477:

```rust
                        icon_button("file-find-previous", Icon::ArrowUp, theme)
```
```rust
                        icon_button("file-find-next", Icon::ArrowDown, theme)
```
```rust
                        icon_button("file-find-close", Icon::X, theme)
```

- [ ] **Step 4: Composer (`composer.rs`)**

Line 662 (unqueue): `.child(icon_svg(Icon::X, theme::ICON_SIZE_SMALL, theme.text_muted)),`
The div at 648-656 gets `.flex().items_center()` after `.flex_none()`.

Lines 729-732:

```rust
                        let sparkles = icon_svg(Icon::Sparkles, theme::ICON_SIZE_SMALL, theme.text_muted);
                        let improve = if draft.enhancing.is_some() {
                            disabled_button("composer-enhance", sparkles, theme, false)
                        } else {
                            quiet_button("composer-enhance", sparkles, theme, theme.surface_raised)
```

(`Svg` is not `Clone`; only one branch runs, so one `sparkles` value moved into one branch compiles.)

- [ ] **Step 5: Run menu rows (`run.rs` 384-434)**

```rust
            let controls: &[(Icon, &'static str, RunAction)] = if running {
                &[
                    (Icon::RotateCw, "Restart this command", RunAction::Restart),
                    (Icon::CircleStop, "Stop this command", RunAction::Stop),
                ]
            } else {
                &[(Icon::Play, "Run this command", RunAction::Start)]
            };
```

In the loop (417-434): `for &(glyph, tip, action) in controls {`. The id becomes `SharedString::from(format!("run-{action:?}-{}", row.name))` (the glyph is no longer a string; `RunAction` derives `Debug`). Add `.flex().items_center()` after `.id(..)`. Replace `.text_color(rgb(colors.fg))` + `.child(label)` with `.child(icon_svg(glyph, theme::ICON_SIZE_SMALL, colors.fg))`.

- [ ] **Step 6: Session panel toggle (`inspector.rs` line 74)**

```rust
            .child(icon_svg(
                Icon::PanelRight,
                theme::ICON_SIZE_SMALL,
                if self.inspector_open {
                    theme.accent_text
                } else {
                    theme.text_muted
                },
            ))
```

Add `.flex().items_center()` to the toggle div (after `.flex_none()` or at the start of its chain), so it centers like `◧`.

- [ ] **Step 7: Terminal find bar close (`terminal/view.rs`)**

Add `use crate::icons::{Icon, icon_svg};` to the file's `use` block. Line 619:

```rust
            .child(icon_svg(Icon::X, theme::ICON_SIZE_SMALL, theme.text_muted));
```

Add `.flex().items_center()` after `.id("find-close")`. The `theme` module is already in scope there (line 617 uses `theme::opacity::ACTIVE`).

- [ ] **Step 8: Build and test**

Run: `cargo build -p mkrew-ui -j 4 && cargo test -p mkrew-ui -j 4`
Expected: builds, PASS.

Check that no in-scope glyph is left as a button child (out-of-scope ones in the constraints list are expected):

Use the grep tool, pattern `"[◷◧◨◫⇤⇥⌕⋯‹›⌫]"`, path `crates/mkrew-ui/src`.
Expected: only doc comments and `peek.rs`/`main.rs` comments; no `.child("…")`, `button(…)` or `row_action(…)` argument.

- [ ] **Step 9: Commit**

```bash
cargo fmt --all
git add crates/mkrew-ui/src/shell/pane_render.rs crates/mkrew-ui/src/shell/sidebar.rs crates/mkrew-ui/src/shell/files_render.rs crates/mkrew-ui/src/shell/composer.rs crates/mkrew-ui/src/shell/run.rs crates/mkrew-ui/src/shell/inspector.rs crates/mkrew-ui/src/terminal/view.rs
git commit -m "feat(ui): tab strip, find bars, status bar and row actions draw Lucide icons"
```

---

### Task 4: Row disclosure chevrons

**Files:**
- Modify: `crates/mkrew-ui/src/shell/sidebar.rs:629-634` (workspace row)
- Modify: `crates/mkrew-ui/src/shell/git_rows.rs:300-306,381-387` (group header, commit row)
- Modify: `crates/mkrew-ui/src/shell/files_render.rs:842-846,879-885` (Files tree)
- Modify: `crates/mkrew-ui/src/shell/run.rs:814-820` (Run setup folder picker)

**Interfaces:**
- Consumes: `Icon::{ChevronDown, ChevronRight}`, `icon_svg`, `theme::ICON_SIZE_SMALL`, `Theme::chevron_color()`.
- Produces: nothing new.

The text chevrons sat in `space::M` (12 px) columns. The 14 px icon gets a column of its own size, so rows stay aligned with each other. `chevron_color()` stays the chevron color. Its ≥ 3:1 contrast test (`theme.rs` `avatar_letters_and_chevrons_stay_readable`) still applies.

- [ ] **Step 1: Sidebar workspace row (`sidebar.rs` 629-634)**

```rust
        .child(icon_svg(
            if row.expanded {
                Icon::ChevronDown
            } else {
                Icon::ChevronRight
            },
            theme::ICON_SIZE_SMALL,
            theme.chevron_color(),
        ))
```

- [ ] **Step 2: Git rows (`git_rows.rs`)**

Lines 300-306:

```rust
        .child(icon_svg(
            if folded {
                Icon::ChevronRight
            } else {
                Icon::ChevronDown
            },
            theme::ICON_SIZE_SMALL,
            theme.chevron_color(),
        ))
```

Lines 381-387:

```rust
        .child(icon_svg(
            if expanded {
                Icon::ChevronDown
            } else {
                Icon::ChevronRight
            },
            theme::ICON_SIZE_SMALL,
            theme.chevron_color(),
        ))
```

- [ ] **Step 3: Files tree (`files_render.rs`)**

Lines 842-846:

```rust
    let chevron = match (entry.is_dir, expanded) {
        (false, _) => None,
        (true, false) => Some(Icon::ChevronRight),
        (true, true) => Some(Icon::ChevronDown),
    };
```

Lines 879-885 (files keep an empty column, so names line up under folders):

```rust
        .child(
            div()
                .flex_none()
                .w(theme::rems_px(theme::ICON_SIZE_SMALL))
                .children(
                    chevron.map(|icon| icon_svg(icon, theme::ICON_SIZE_SMALL, theme.text_muted)),
                ),
        )
```

- [ ] **Step 4: Run setup folder picker (`run.rs` 814-820)**

```rust
                    .child(
                        div()
                            .flex_none()
                            .w(theme::rems_px(theme::ICON_SIZE_SMALL))
                            .when(is_dir, |el| {
                                el.child(icon_svg(
                                    Icon::ChevronRight,
                                    theme::ICON_SIZE_SMALL,
                                    theme.text_muted,
                                ))
                            }),
                    )
```

- [ ] **Step 5: Build and test**

Run: `cargo build -p mkrew-ui -j 4 && cargo test -p mkrew-ui -j 4`
Expected: builds, PASS.

Use the grep tool, pattern `\.child\(if .*"▾"|"▸"`, path `crates/mkrew-ui/src/shell`.
Expected: no match.

- [ ] **Step 6: Commit**

```bash
cargo fmt --all
git add crates/mkrew-ui/src/shell/sidebar.rs crates/mkrew-ui/src/shell/git_rows.rs crates/mkrew-ui/src/shell/files_render.rs crates/mkrew-ui/src/shell/run.rs
git commit -m "feat(ui): row chevrons draw Lucide icons"
```

---

### Task 5: Close pane, Share and icons beside text

**Files:**
- Modify: `crates/mkrew-ui/src/shell/widgets.rs` (add `icon_text`, `button_fg`; `button` uses `button_fg`)
- Modify: `crates/mkrew-ui/src/run.rs:151-153,344,349` (idle label without `▶`)
- Modify: `crates/mkrew-ui/src/shell/pane_render.rs:144,162,270,277,425,462`
- Modify: `crates/mkrew-ui/src/shell/chrome.rs:354-360,435`
- Modify: `crates/mkrew-ui/src/shell/run.rs:300,338,605,619,726,850,852`
- Modify: `crates/mkrew-ui/src/shell/cards.rs:373,388,446,448,496,559`
- Modify: `crates/mkrew-ui/src/shell/comments.rs:305,309,335,344`
- Modify: `crates/mkrew-ui/src/shell/composer.rs:513,748`
- Modify: `crates/mkrew-ui/src/shell/docs.rs:404`
- Modify: `crates/mkrew-ui/src/shell/files_render.rs:359,370,383`
- Modify: `crates/mkrew-ui/src/shell/git_diff.rs:158,349,666`
- Modify: `crates/mkrew-ui/src/shell/git_render.rs:328-351,844-856`
- Modify: `crates/mkrew-ui/src/shell/git_menus.rs:433-438`
- Modify: `crates/mkrew-ui/src/shell/inspector.rs:312,320,354,362`
- Modify: `crates/mkrew-ui/src/shell/sidebar.rs:199`
- Modify: `crates/mkrew-ui/src/shell/task_dialog.rs:213`

**Interfaces:**
- Consumes: `Icon`, `icon_svg`, `theme::ICON_SIZE_SMALL` (Task 1); `icon_button(id, Icon, theme)`, button helpers with `label: impl IntoElement` (Task 2).
- Produces (in `shell/widgets.rs`):
  - `icon_text(glyph: Icon, text: impl IntoElement, color: u32) -> gpui::Div`: the 14 px icon, then `text`, on one line, `space::XS` apart, both meant to be in `color`.
  - `button_fg(theme: &Theme, primary: bool) -> u32`: `on_accent` if `primary`, else `text_muted` (the label color of `button`).

**Color rule (every row below follows it; an svg never inherits the button's color):** `button(…, p)` → `button_fg(theme, p)`; `quiet_button` and `disabled_button(…, false)` → `theme.text_muted`; `secondary_button` → `theme.text`; a button whose `text_color` is overridden uses that same token.

- [ ] **Step 1: Pin the new idle Run label (failing test first)**

In `crates/mkrew-ui/src/run.rs` tests, change the two assertions at lines 344 and 349 from `"▶ Run"` to `"Run"` (the ▶ becomes an icon drawn by the button).

Run: `cargo test -p mkrew-ui -j 4 --lib run::`
Expected: FAIL, `left: "▶ Run"`, `right: "Run"`.

In `Summary::label` (line 152): `return "Run".to_owned();`

Run: `cargo test -p mkrew-ui -j 4 --lib run::`
Expected: PASS.

- [ ] **Step 2: `icon_text` and `button_fg` in `widgets.rs`**

Add after `button` (line ~119):

```rust
/// The label color of [`button`]: `on_accent` on the primary fill, else `text_muted`. An
/// [`icon_text`] inside a button takes the same color (an svg does not inherit it).
pub(crate) fn button_fg(theme: &Theme, primary: bool) -> u32 {
    if primary {
        theme.on_accent
    } else {
        theme.text_muted
    }
}

/// A button label with a leading icon: `glyph`, then `text`, on one line, both in `color`.
pub(crate) fn icon_text(glyph: Icon, text: impl IntoElement, color: u32) -> gpui::Div {
    div()
        .flex()
        .items_center()
        .gap(px(space::XS))
        .child(icon_svg(glyph, theme::ICON_SIZE_SMALL, color))
        .child(text)
}
```

In `button`, change `.text_color(rgb(theme.on_accent))` to `.text_color(rgb(button_fg(theme, true)))`, so the two stay one rule.

- [ ] **Step 3: Close pane and the pane strip / header (`pane_render.rs`)**

Line 162: `icon_button("close-pane", Icon::SquareX, theme)` (it joins `+ ◫ ⋯` at the strip's end; the tooltip with its shortcut stays).

Line 144: `button("unzoom", icon_text(Icon::Minimize2, "Show all panes", button_fg(theme, false)), theme, false)`

Line 270: `button("empty-new-agent", icon_text(Icon::Bot, "New agent", button_fg(theme, true)), theme, true)`

Line 277: `secondary_button("empty-new-shell", icon_text(Icon::SquareTerminal, "New shell", theme.text), theme)`

Line 425:

```rust
                            button_frame(
                                "pane-share",
                                icon_text(
                                    Icon::Share2,
                                    "Share",
                                    if following { theme.text } else { theme.text_muted },
                                ),
                            )
```

Line 462: `quiet_button("pane-turns", icon_text(Icon::History, "Turns…", theme.text_muted), theme, theme.surface_raised)`

- [ ] **Step 4: Title bar (`chrome.rs`, `run.rs` shell)**

`chrome.rs` line 435:

```rust
        let fg = button_fg(theme, false);
        let button = button(
            "new-pane",
            icon_text(Icon::Plus, "New", fg)
                .child(icon_svg(Icon::ChevronDown, theme::ICON_SIZE_SMALL, fg)),
            theme,
            false,
        )
```

`chrome.rs` lines 354-360 (workspace switcher caret): replace the `div()…child("▾")` child with

```rust
            .child(icon_svg(Icon::ChevronDown, theme::ICON_SIZE_SMALL, theme.text_muted));
```

`shell/run.rs` line 300 (main half) and 338 (menu half). Both halves get the icon-button height, so the split button stays one height whether a half holds text or only an icon:

```rust
        let label = if live {
            summary.label().into_any_element()
        } else {
            icon_text(Icon::Play, summary.label(), button_fg(theme, true)).into_any_element()
        };
        let main = button("run-main", label, theme, !live)
            .flex_none()
            .h(theme::rems_px(theme::ICON_BUTTON_SIZE))
```

```rust
        let menu = button(
            "run-menu",
            icon_svg(Icon::ChevronDown, theme::ICON_SIZE_SMALL, button_fg(theme, !live)),
            theme,
            !live,
        )
        .h(theme::rems_px(theme::ICON_BUTTON_SIZE))
        .when(!live, |el| {
```

(The rest of both chains is unchanged. A live warning label keeps its `status_needs_input` text color; it has no icon.)

- [ ] **Step 5: Empty states and cards**

`sidebar.rs` 199: `button("add-first-workspace", icon_text(Icon::FolderPlus, "Add workspace", button_fg(theme, true)), theme, true)`

`task_dialog.rs` 213: `button("new-task", icon_text(Icon::Plus, "New task", button_fg(theme, primary)), theme, primary)`

`composer.rs` 513: `button("prompt-open-dir", icon_text(Icon::FolderOpen, "Open prompts folder", button_fg(theme, true)), theme, true)`

`composer.rs` 748: `icon_text(Icon::Library, "Prompts…", theme.text_muted),`

`cards.rs`:
- 373: `quiet_button("card-undo", icon_text(Icon::Undo2, "Undo", theme.text_muted), theme, edit_hover)`
- 388: `quiet_button("card-changes", icon_text(Icon::FileDiff, "Changes", theme.text_muted), theme, edit_hover)`
- 446: `button("card-compare", icon_text(Icon::GitCompare, "Compare", button_fg(theme, true)), theme, true)`
- 448: `quiet_button("card-compare", icon_text(Icon::GitCompare, "Compare", theme.text_muted), theme, edit_hover)`
- 496: `quiet_button("card-run-with", icon_text(Icon::Play, "Run with…", theme.text_muted), theme, theme.border)`
- 559: `quiet_button("card-fan-out", icon_text(Icon::Users, "Several agents…", theme.text_muted), theme, theme.border)`

- [ ] **Step 6: Docs, Files, Git toolbars**

`comments.rs`:
- 305: `disabled_button("doc-comment", icon_text(Icon::MessageSquarePlus, "Comment", theme.text_muted), theme, false)`
- 309: `button("doc-comment", icon_text(Icon::MessageSquarePlus, "Comment", button_fg(theme, false)), theme, false)`
- 335: `disabled_button("doc-send", icon_text(Icon::Send, "Send to agent", theme.text_muted), theme, false)`
- 344: `button("doc-send", icon_text(Icon::Send, "Send to agent", button_fg(theme, false)), theme, false)`

`docs.rs` 404: `button("doc-zed", icon_text(Icon::ExternalLink, "Open in Zed", button_fg(theme, false)), theme, false)`

`files_render.rs`:
- 359: `button("file-review", icon_text(Icon::FileText, "Review in Docs", button_fg(theme, false)), theme, false)`
- 370: `button("file-save", icon_text(Icon::Save, "Save", button_fg(theme, dirty)), theme, dirty)`
- 383: `button("file-find", icon_text(Icon::Search, "Find", button_fg(theme, false)), theme, false)`

`git_diff.rs`:
- 158 (`zed_button`): `button(id, icon_text(Icon::ExternalLink, "Open in Zed", button_fg(theme, primary)), theme, primary)`
- 349 (Discard…, red text): `quiet_button("git-discard", icon_text(Icon::Undo2, "Discard…", red), theme, theme.hover)` (`red` is the local `theme.diff_del_text` defined at 348)
- 666: `button("git-pick-comment", icon_text(Icon::MessageSquarePlus, "Comment", button_fg(theme, true)), theme, true)`

`git_render.rs` Push (328-351). After `let tip = …;` (line 337) the early-return branch and the two live branches become:

```rust
        if running.is_some() || no_upstream {
            return button_frame(
                "git-push",
                icon_text(Icon::ArrowUpFromLine, label, theme.text_muted),
            )
            .flex_none()
            .cursor_default()
            .opacity(opacity::DISABLED)
            .text_color(rgb(theme.text_muted))
            .tooltip(tooltip(theme, tip, ""))
            .into_any_element();
        }
        let push = if ahead > 0 {
            secondary_button("git-push", icon_text(Icon::ArrowUpFromLine, label, theme.text), theme)
        } else {
            quiet_button(
                "git-push",
                icon_text(Icon::ArrowUpFromLine, label, theme.text_muted),
                theme,
                theme.hover,
            )
        };
```

`git_render.rs` Generate (844-856):

```rust
        let write = if draft.busy || generating {
            let label = if generating { "Writing…" } else { "Generate" };
            disabled_button(
                "git-commit-generate",
                icon_text(Icon::Sparkles, label, theme.text_muted),
                theme,
                false,
            )
        } else {
            quiet_button(
                "git-commit-generate",
                icon_text(Icon::Sparkles, "Generate", theme.text_muted),
                theme,
                theme.hover,
            )
            .on_click(
```

(the `.on_click(…)` body is unchanged).

`git_menus.rs` 433-438 (`By: … ▾`, the name may be long and truncates; the chevron stays visible):

```rust
                    quiet_button(
                        "git-changed-by",
                        div()
                            .flex()
                            .items_center()
                            .gap(px(space::XS))
                            .min_w_0()
                            .child(div().min_w_0().truncate().child(format!("By: {shown}")))
                            .child(icon_svg(
                                Icon::ChevronDown,
                                theme::ICON_SIZE_SMALL,
                                theme.text_muted,
                            )),
                        theme,
                        theme.hover,
                    )
```

- [ ] **Step 7: Session panel and Run setup**

`inspector.rs`:
- 312: `quiet_button("plan-unlink", icon_text(Icon::Unlink, "Unlink", theme.text_muted), theme, theme.hover)`
- 320: `button("plan-link", icon_text(Icon::Link, "Link plan…", button_fg(theme, true)), theme, true)`
- 354: `button("inspector-transcript", icon_text(Icon::FileText, "Open transcript", button_fg(theme, false)), theme, false)`
- 362: `disabled_button("inspector-transcript", icon_text(Icon::FileText, "Open transcript", theme.text_muted), theme, false)`

`shell/run.rs`:
- 605 (label line of `run-duplicate`): `icon_text(Icon::Copy, "Duplicate", theme.text_muted),`
- 619: `quiet_button("run-remove", icon_text(Icon::Trash2, "Remove", theme.text_muted), theme, theme.surface_raised)`
- 726: `button("run-add", icon_text(Icon::Plus, "Add command", button_fg(theme, false)), theme, false)`
- 850: `disabled_button("run-picker-up", icon_text(Icon::ArrowUp, "Up", theme.text_muted), theme, false)`
- 852: `quiet_button("run-picker-up", icon_text(Icon::ArrowUp, "Up", theme.text_muted), theme, theme.surface_raised)`

- [ ] **Step 8: Build and test**

Run: `cargo build -p mkrew-ui -j 4 && cargo test -p mkrew-ui -j 4`
Expected: builds, PASS.

Use the grep tool, pattern `"(\+ New ▾|▶ Run|✦ Generate|✦ Writing…|\+ Add command|↑ Up|Close pane)"`, path `crates/mkrew-ui/src`.
Expected: no match outside comments (the tooltip "Close the focused pane" does not match).

- [ ] **Step 9: Commit**

```bash
cargo fmt --all
git add crates/mkrew-ui/src/run.rs crates/mkrew-ui/src/shell/widgets.rs crates/mkrew-ui/src/shell/pane_render.rs crates/mkrew-ui/src/shell/chrome.rs crates/mkrew-ui/src/shell/run.rs crates/mkrew-ui/src/shell/cards.rs crates/mkrew-ui/src/shell/comments.rs crates/mkrew-ui/src/shell/composer.rs crates/mkrew-ui/src/shell/docs.rs crates/mkrew-ui/src/shell/files_render.rs crates/mkrew-ui/src/shell/git_diff.rs crates/mkrew-ui/src/shell/git_render.rs crates/mkrew-ui/src/shell/git_menus.rs crates/mkrew-ui/src/shell/inspector.rs crates/mkrew-ui/src/shell/sidebar.rs crates/mkrew-ui/src/shell/task_dialog.rs
git commit -m "feat(ui): Close pane and Share icons; icons before frequent button labels"
```

---

### Task 6: Human checks, full verify, smoke run

**Files:**
- Modify: `docs/HUMAN_CHECKS.md:17` (top of "To verify (non-blocking)")

**Interfaces:**
- Consumes: everything above. Produces: docs only.

- [ ] **Step 1: HUMAN_CHECKS entry**

Insert before line 18 (`### To verify — 2026-10-12 sidebar marks, remove idle agents`):

```markdown
### To verify — 2026-10-12 Lucide icons (ADR 0051, plan docs/superpowers/plans/2026-10-12-lucide-icons.md)

- [ ] **Icons (dark + light):** title bar (theme toggle cycles sun-moon → sun → moon, History
  clock), sidebar header (+, collapse chevron, remove-idle), tab strip (+, split, ⋯, tab ✕),
  status bar (sidebar, compact, session panel toggles), Git (repos ▾, fetch, ‹ ›, ⋯), Files
  (search row, find bar ▸ ↑ ↓ ✕), terminal find ✕, composer ✦ and queue ✕, Run (live ↻ ■, menu
  rows ↻ ■ ▶): every icon is drawn (no blank or box), centered, muted, same stroke weight.
- [ ] **Window controls (Windows):** hover close → red background and a white X; minimize and
  maximize hover show the soft background. Maximize (button, title bar double-click, Snap) →
  the middle icon becomes the restore icon; restore → a square again.
- [ ] **Hidden / disabled:** session ✕, workspace ⋯ and inactive-tab ✕ appear only on hover; the
  composer ✦ is faded while the prompt is being improved.
- [ ] **Chevrons:** workspace rows, Git group headers and History commits, Files tree folders, Run
  setup folders: chevrons flip on expand/collapse; names stay aligned between files and folders.
- [ ] **Font size:** set `[ui] font_size = 18`, restart: icons grow with the text and stay
  centered; nothing clips; the Run button's two halves keep one height.
- [ ] **Icons beside text (dark + light):** tab strip ends with Close pane (square ✕), +, split,
  ⋯; pane header Share and Turns…; title bar `+ New ▾`, idle Run (▶ icon) and its ▾, switcher
  ▾; empty states (New agent, New shell, Add workspace, New task, Open prompts folder); card
  Undo / Changes / Compare / Run with… / Several agents…; Docs and Git Comment, Send to agent,
  Open in Zed; Files Review in Docs, Save, Find; Git Discard… (red icon), Push, Generate,
  `By: … ▾`; session panel Open transcript, Link plan…, Unlink; composer Prompts…; Run setup
  Duplicate, Remove, Add command, Up. Icon and text share one color and sit on one line.
- [ ] **Primary ↔ ghost:** edit a file → Save turns blue with a white icon, save → muted again.
  Select a card for Compare → white icon on blue. Run idle (blue, white ▶ and ▾) → start → quiet
  (muted ▾). Share while following → icon as bright as the text.
```

- [ ] **Step 2: Full verify**

Run: `sh scripts/verify.sh`
Expected: fmt check, clippy `-D warnings`, all tests green. Fix any clippy finding in the changed files (for example `needless_borrow`, unused import) and rerun.

- [ ] **Step 3: Smoke run**

Build and start the app against the running daemon (do not stop the user's daemon). If `target\debug\mkrew.exe` is locked, rename it to `mkrew.locked-old.exe` first.

Run: `cargo build -p mkrew-ui -j 4`, then start `target/debug/mkrew.exe` with `MKREW_LOG=warn`, e.g. in PowerShell `$env:MKREW_LOG="warn"; & "$PWD\target\debug\mkrew.exe"`. Let it run about 10 s with a workspace open, then close it.
Expected: the window opens; stderr has no `svg`, `asset` or `usvg` error lines (gpui logs failed SVG loads through `log_err`). Take a screenshot of the title bar and sidebar if possible. Otherwise the HUMAN_CHECKS entry covers the look.

- [ ] **Step 4: Commit**

```bash
git add docs/HUMAN_CHECKS.md
git commit -m "docs(ui): human checks for the Lucide icons"
```

If `docs/HUMAN_CHECKS.md` holds uncommitted changes that are not from this plan, stage only this plan's hunk (`git add -p docs/HUMAN_CHECKS.md`).
