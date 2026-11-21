# Polish: scoped palette, glass panels, Settings and pressable states - Design

- Date: 2026-11-21
- Status: approved by the user in brainstorming (2026-11-21), section by section; the user pre-approved this spec and its plans.
- ADR: `docs/decisions/0097-glass-panels-and-pressable.md` (written by the first task of Plan P1).
- Protocol: `PROTOCOL_VERSION` 53 -> 54 (Plan P2).
- Inspiration: Tern by Stencil (v0.9.29), tried on this machine: its command palette, Settings window and hover feedback.
- Sub-project 1 of 5 taken from Tern: Polish -> Theme gallery -> Widget bar -> Review & MD -> Chat read mode.
  Each later sub-project gets its own spec and plans when its turn comes.

## 1. Goal

Mkrew should feel as crisp as Tern when the user reaches for a command, a file, a setting or any button.
Done means:

- One palette with scope tabs (All, Commands, Sessions, Files, Search, Branches, Commits) replaces the command palette and Quick open.
- Settings is a larger floating panel with a search over every page, settings grouped in cards, shared controls and a footer with the version.
- The palette, Settings and every dialog share one translucent "glass" panel.
- Every clickable element in `crates/mkrew-ui` reacts to hover, press, keyboard focus and the disabled state the same way, through shared widgets.

## 2. What the user decided

| Topic | Decision |
| --- | --- |
| Palette scopes | All, Commands, Sessions, Files, Search (file contents), Branches, Commits. No Hosts scope. |
| Palette architecture | Approach A: one palette, a `Scope` enum, synchronous sources (Commands, Sessions) and asynchronous daemon sources (Files, Search, Branches, Commits). Quick open is removed. |
| Search backend | `fs.search` in the daemon with the ripgrep crates, a walk per query, a result limit and cancellation. No persistent index. |
| Glass | Fake glass: a translucent surface, a 1 px light inner border, an accent tint gradient, a stronger shadow and a lighter scrim. No real backdrop blur (GPUI 0.3.7 cannot blur behind one element). |
| Settings | Search over every page, settings grouped in cards, shared controls, footer with version and file links, a larger glass panel. |
| Hover / press scope | The whole app: every clickable element in `crates/mkrew-ui`. |
| Delivery | One spec, three plans: P1 foundation, P2 palette and protocol, P3 the hover / press audit (section 9). |

## 3. Non-goals

- A theme gallery, theme files, the widget bar, review overlays, Markdown editing and the chat view (later sub-projects).
- Layout presets (Rail / Tiles / Float / Studio), picture in picture, file glimpse cards.
- A persistent search index, a Hosts scope, a search over keyboard shortcuts from the global Settings search.
- Real backdrop blur or a window-level Mica / acrylic background.
- Creating, renaming or deleting branches from the palette (only checkout).

## 4. Glass panel and tokens

### 4.1 Tokens (`crates/mkrew-ui/src/theme.rs`)

New `Theme` fields, set for every built-in theme from its `Palette` (dark and light differ):

| Field | Dark | Light | Use |
| --- | --- | --- | --- |
| `glass: u32` (RGBA) | `surface` at alpha 0.88 | `surface` at alpha 0.92 | Fill of every floating panel. |
| `glass_border: u32` (RGBA) | white at 8 % | black at 8 % | 1 px inner border of a floating panel. |
| `glass_tint: u32` (RGBA) | `accent` at 5 % | `accent` at 4 % | Top of a vertical gradient that fades to transparent over `GLASS_TINT_HEIGHT`. |

Changed field: `scrim` becomes black at 24 % in dark themes and black at 16 % in light themes (today 35 % in both).

New function: `theme::shadow_float(theme: &Theme) -> Vec<gpui::BoxShadow>`.
Dark: two layers, offset (0, 8) blur 24 black 35 %, and offset (0, 2) blur 6 black 20 %.
Light: the same geometry at half the alpha.
It replaces `shadow_md()` on floating panels.

New size constants:

| Constant | Value | Replaces |
| --- | --- | --- |
| `GLASS_TINT_HEIGHT` | 120.0 | - |
| `PALETTE_WIDTH` | 640.0 | `DIALOG_WIDTH` for the palette |
| `PALETTE_LIST_HEIGHT` | 480.0 | `QUICK_OPEN_LIST_HEIGHT` (removed) |
| `PALETTE_SELECTED_BAR` | 3.0 | - |
| `PALETTE_GROUP_LIMIT` | 5 (usize) | - |
| `SETTINGS_DIALOG_WIDTH` | 880.0 | 760.0 |
| `SETTINGS_DIALOG_HEIGHT` | 640.0 | 560.0 |
| `SETTINGS_NAV_WIDTH` | 200.0 | 180.0 |

### 4.2 `floating_panel`

`widgets::floating_panel(id: impl Into<gpui::ElementId>, theme: &Theme) -> gpui::Stateful<gpui::Div>` draws, in order: the `glass` fill, the `glass_tint` gradient over the top `GLASS_TINT_HEIGHT` px, the `glass_border` 1 px inner border, `shadow_float`, `radius::LARGE`, `.occlude()`.
Callers set the size and the content.
The palette, Settings and every modal dialog that today builds its own panel inside `modal_shade` move to it.
After P1 no dialog builds its own panel frame.

### 4.3 Motion

A floating panel appears with a fade plus a 4 px rise over `motion::FAST`, ease-out (gpui-pre 0.3.7 has no element transform, so no scale).
It disappears at once.
The motion is `widgets::appear`, applied by `modal_shade` and the confirm prompt, not inside `floating_panel` (`with_animation` changes the element type callers chain on).
When reduced motion is on, gpui draws the end state, so the panel appears at once.
Reduced motion is read on Windows from `SPI_GETCLIENTAREAANIMATION` at startup and on window activation and given to `cx.set_reduce_motion`; on macOS and Linux it stays off (gpui has no read there).

### 4.4 Contrast check

Body text on `glass` keeps a contrast of at least 4.5:1 in the worst case (a light page right behind the panel in a dark theme, a dark page in a light theme).
A test computes it for every built-in theme over both `bg` and `text` behind the panel; a failing theme gets a higher alpha.

## 5. Pressable states for the whole app

### 5.1 The rule

Every element with a click or mouse-down handler gets its hover, press, focus and disabled look from `widgets.rs`.
Only `widgets.rs` and `pressable.rs` call `.hover(` and `.active(` directly after P3; `group_hover` stays allowed anywhere.
The rule is written in SPEC §14, the `mkrew-ui-design` skill and the `ui-reviewer` checklist.

### 5.2 API (`crates/mkrew-ui/src/pressable.rs`, re-exported by `shell/widgets.rs`)

`Look` and `Pressable` live in the lib crate because lib modules (e.g. `preview.rs`) need them; `widgets.rs` is in the bin crate.
Filled controls (the primary button, the switch) dim on hover with `opacity::HOVER` instead of changing fill.

```rust
/// How a pressable element looks at rest, hovered and pressed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Look {
    /// A list or menu row: hover fill `hover`, no border change.
    Row,
    /// A chip or pill: hover fill `hover`.
    Chip,
    /// An icon-only control: hover fill `hover`, icon to full text color.
    Icon,
    /// A text link: no fill, hover underline and full text color.
    Text,
    /// An element on a `surface_raised` fill (menus, toasts, cards, the find bar): hover fill `border`.
    Raised,
}

/// The resolved style of a pressable element; pure, so it is tested without a window.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PressStyle {
    pub hover_bg: Option<u32>,
    pub hover_fg: Option<u32>,
    pub underline_on_hover: bool,
    pub active_opacity: f32,
    pub opacity: f32,
    pub pointer: bool,
}

pub fn press_style(look: Look, enabled: bool, theme: &Theme) -> PressStyle;

pub trait Pressable: Sized {
    /// Applies `press_style(look, enabled, theme)`: cursor, hover, active, disabled opacity.
    fn pressable(self, look: Look, enabled: bool, theme: &Theme) -> Self;
    /// Attaches `handler` to clicks only when `enabled`.
    fn on_press(
        self,
        enabled: bool,
        handler: impl Fn(&gpui::ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self;
}

impl Pressable for gpui::Stateful<gpui::Div> { /* ... */ }
```

Disabled: `opacity::DISABLED`, default cursor, no hover, no active, and `on_press` attaches nothing.
Active: `opacity::ACTIVE`.
The focus ring stays where it is today (`DialogControl`), and `pressable` adds nothing to focus.

New widgets built on `Pressable`:

- `toggle(id, on: bool, enabled: bool, theme: &Theme) -> gpui::Stateful<gpui::Div>`: the switch, `SWITCH_WIDTH` x `SWITCH_HEIGHT`, knob inset `SWITCH_INSET`, accent track when on.
- `dropdown_trigger(id, label: impl IntoElement, open: bool, enabled: bool, theme: &Theme) -> gpui::Stateful<gpui::Div>`: a field frame with the label and a chevron; the frame is accent while `open`.
- `list_row(id, selected: bool, theme: &Theme) -> gpui::Stateful<gpui::Div>`: a `Look::Row` row; `selected` adds the `selected` fill and the `PALETTE_SELECTED_BAR` accent bar on the left.

Existing widgets (`button`, `secondary_button`, `quiet_button`, `danger_button`, `disabled_button`, `icon_button_in`, `segment`, `menu_item`) are rebuilt on `pressable`, keeping their signatures.

### 5.3 The audit (Plan P3)

Today `crates/mkrew-ui/src` has 327 click or mouse-down handlers in 48 files (294 `.on_click(`, 33 `.on_mouse_down(`), against 76 `.hover(` calls.
208 already use a shared widget, 71 are hand-built and get converted, 48 are exempt.
P3 walks them by area: chrome and sidebar; tabs and panes; Board and Jira; Files and Docs; Git; inspector, thread and composer; dialogs; Run, prompts and leftovers.
Each clickable element moves to a widget or to `.pressable(..)`.
P3 adds the helpers the audit needs on top of P1: `pressable_unless` (a selected row keeps full opacity), `card_press` (board cards hover by border), `disabled_icon_button`, `grip_hover` and `pick_surface` (resize grips and the diff line pick hover without being buttons), and `row_action` moved from `git_rows.rs` into `widgets.rs`.
Areas that are not buttons get no hover and are listed in the plan with the reason: terminal surfaces, drop zones, the scrim, text fields (they keep their I-beam).
Why not `surface_raised` for hover: in Mkrew Dark and Light it equals `selected`, so a hovered row would look selected, and menus and cards already sit on it.

## 6. Settings

### 6.1 Frame

Settings opens in a `floating_panel` of `SETTINGS_DIALOG_WIDTH` x `SETTINGS_DIALOG_HEIGHT`, capped at the viewport minus a margin as today.
The left navigation is `SETTINGS_NAV_WIDTH` wide and lists the pages under three overlines, in the order of `settings_dialog::NAV` (the single source of the order; `Page::ALL` keeps the TOML order):

- **App**: General, Appearance, Notifications, Voice, Features.
- **Integrations**: Jira, Phone, Machines.
- **Input**: Keyboard, Prompts.

The right side has the page title and a close button.

### 6.2 Search

A search box sits at the top of the navigation; `Ctrl+F` focuses it while Settings is open.
With a query, the right side lists the matching settings of every page instead of the current page.
Each result row shows a muted breadcrumb (`Appearance › Font`), the label with the matched characters in accent semibold (the palette's convention), the help line and the real control, usable in place.
A setting matches when `palette::fuzzy` scores above zero on its label, its help, its TOML key or its page name; results are ranked by the best of those scores, with a fixed bonus on a label match.
Pages with no match are dimmed in the navigation.
Esc clears a non-empty query first; Esc on an empty query closes Settings.
The Keyboard page keeps its own search; the global search does not search shortcuts.
An empty result shows the empty state "No setting matches “<query>”." with the action "Clear search" (Esc).

### 6.3 Groups as cards

`Setting` (`crates/mkrew-core/src/settings_fields.rs`) gets `group: &'static str`, a UI group independent of `section` (which stays the TOML table name).
A page renders each group as an overline title followed by one card (`radius::LARGE`, `surface` fill) holding its rows, with a faint 1 px divider between rows.
A row is the label and the muted help on the left and the control on the right.
Invariants checked by tests, over the non-hidden settings (hidden settings have no group): every one has a non-empty `group`; rows of one group are contiguous in `SETTINGS` (rows are reordered where needed, e.g. `ui.close_to_tray`, `features.session`); the table still matches the template (existing test).
Matching help text with `palette::fuzzy` (a subsequence match) is noisy on short queries, so a label match always ranks above a help-only match.

### 6.4 Controls

Bool uses `toggle`, Choice uses `segmented` + `segment`, Font and List use `dropdown_trigger`, Text and Number keep `caret_field`.
Every control has hover, press, focus and disabled states through `Pressable`.

### 6.5 Footer

Left: `v{CARGO_PKG_VERSION}` in meta text, then the links "settings.toml" and "keymap.toml" (they open the files in Zed, as today).
Right: "Changes apply at once."; on the Prompts page the existing hint "Ctrl+N new · Ctrl+S saves · Esc closes" stays instead.

Storage, `Setting::write` and apply-at-once behavior do not change.

## 7. Palette

### 7.1 Model (`crates/mkrew-ui/src/palette.rs`)

```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Scope { All, Commands, Sessions, Files, Search, Branches, Commits }
```

`Scope::ORDER` is the order above; Tab moves to the next scope and Shift+Tab to the previous one, wrapping.
`Row` gains `section: Scope`, `detail: Option<String>` (a muted path, short hash or session state) and `age: Option<String>` (from `git_history::age`).
`Action` gains `OpenFile { path: String }`, `OpenSearchHit { path: String, line: u32 }`, `Checkout { repo: Option<String>, branch: String }` and `ShowCommit { repo: Option<String>, sha: String }`; it becomes `Clone` instead of `Copy`.
A session's state moves from the shortcut column to `detail`.
`palette::fuzzy` and `palette::filter` stay as they are.

### 7.2 Sources

- Synchronous: Commands (commands, extras, agent profiles, Run commands, machines) and Sessions (workspaces, live sessions with waiting ones first, tasks) are filtered locally as today.
- Asynchronous, each request carries a `serial` and late answers are dropped (the `quick_open.rs` rule):
  - Files: `fs.find` per keystroke, debounced `PALETTE_FILES_DELAY` (80 ms).
  - Search: `fs.search` per keystroke, debounced `PALETTE_SEARCH_DELAY` (150 ms), only from 2 characters.
  - Branches: `git.branches` once per palette opening, when the Branches or All scope is first shown, then filtered locally.
  - Commits: `git.log` with `limit: 500` once per palette opening, when the Commits or All scope is first shown, then filtered locally.
- A scope waiting for its answer shows a "Loading…" row and a failed request a muted error row, in that scope's own tab only; All leaves the scope out until it has rows.

The workspace whose files, branches and commits the palette reads is the active workspace (and its task worktree when a task tab is shown), the same one Quick open reads today.
The repository is the one the Git view of that workspace has selected (`repo: Option<String>`, ADR 0029: a direct child folder that is its own repository, `None` = the workspace folder).
With no workspace open, Files, Search, Branches and Commits show the empty state "Open a workspace to search it."

### 7.3 The All scope

Empty query: Sessions first (waiting sessions on top), then the recently run commands (kept in memory by the UI for this run, newest first; lost on restart).
With a query: one section per scope in `Scope::ORDER` except Search, each with at most `PALETTE_GROUP_LIMIT` rows and a trailing "+N more ›" row that switches to that scope.
Search never runs from All: a content search per keystroke over every scope is too heavy.
Branches and Commits join All only after they are loaded (opening All asks for both in the background); no loading or error rows in All.

### 7.4 Keys and entry points

| Key | Opens |
| --- | --- |
| `Ctrl+Shift+P` (`Command::CommandPalette`) | All |
| `Ctrl+Shift+O` (`Command::QuickOpen`) | Files |
| `Ctrl+Alt+Shift+F` (new `Command::SearchInFiles`, `search_in_files`) | Search |
| `Ctrl+Alt+J` (`Command::SelectTheme`) | the theme picker, without scope tabs (unchanged) |
| `@` in a composer | Files in insert mode: Enter inserts the path instead of opening it |

Inside the palette: Tab / Shift+Tab switch scope, Up / Down move, Enter runs, Esc closes, a click on a scope tab switches to it.

### 7.5 Rendering

A `floating_panel` `PALETTE_WIDTH` wide.
Top: the query field with a search icon and an `esc` key cap.
Below: the scope tabs as pills (the current one accent-outlined).
The list: section overlines, `list_row` rows (selected: fill plus the accent bar), matched characters in accent semibold, `detail` muted after the title, `age` right-aligned, shortcuts as `widgets::kbd` key caps.
Footer: "↑↓ navigate · ↵ run · Tab next scope" on the left and "esc close" on the right.
The list is at most `PALETTE_LIST_HEIGHT` tall and scrolls.

### 7.6 Actions

- `OpenFile`: opens the file in the Files view (today's Quick open behavior).
- `OpenSearchHit`: opens the file and puts the caret on `line`.
- `Checkout`: if the repository has uncommitted changes (from `RepoSummary`), a confirm dialog first; if a live agent session runs in that workspace (and the same task worktree, when one is shown), the confirm says so; then `git.checkout`; a failure is shown as a toast with git's message.
  A task worktree's own uncommitted changes are not checked before the confirm (the title bar summary does not cover them); git refuses a checkout that would lose them, and its message is shown.
- `ShowCommit`: opens the Git view on that commit.

### 7.7 Removal

`crates/mkrew-ui/src/quick_open.rs`, `render_quick_open` and `QUICK_OPEN_LIST_HEIGHT` are removed.
Their behavior (serial drop, Loading / Loaded / Failed, `insert_into`) moves into the palette's Files scope.
`Command::QuickOpen` stays as the key name so user keymaps keep working.

## 8. Protocol and daemon

`PROTOCOL_VERSION` goes from 53 to 54 (checked against unmerged branches on 2026-11-21: `feat/md-split` and `fix/restart-cleanup` are at 51).

### 8.1 `git.branches`

```rust
pub struct GitBranchesParams {
    pub project_id: Ulid,
    #[serde(default)] pub worktree_task_id: Option<Ulid>,
    #[serde(default)] pub repo: Option<String>,   // ADR 0029, as in `GitLogParams`
}
pub struct Branch {
    pub name: String,          // "main", "origin/feature-x"
    pub remote: bool,
    pub current: bool,
    pub upstream: Option<String>,
    pub last_commit_at: i64,   // ms since the epoch
    pub subject: String,
}
// result: Vec<Branch>
```

The daemon runs `git for-each-ref --sort=-committerdate --format=... refs/heads refs/remotes` through the `git()` helper of `crates/mkrew-daemon/src/git.rs`.
`refs/remotes/*/HEAD` is dropped.
A remote branch whose short name matches a local branch is dropped.

### 8.2 `git.checkout`

```rust
pub struct GitCheckoutParams {
    pub project_id: Ulid,
    #[serde(default)] pub worktree_task_id: Option<Ulid>,
    #[serde(default)] pub repo: Option<String>,
    pub branch: String,
}
// result: ()
```

The daemon runs `git switch <branch>`; for a remote branch it runs `git switch --track <remote>/<name>`.
Git's stderr is returned as the error message, unchanged.

### 8.3 `fs.search`

```rust
pub struct FsSearchParams {
    pub project_id: Ulid,
    pub worktree_task_id: Option<Ulid>,
    pub query: String,
    pub regex: bool,
    pub limit: u32,            // the UI sends 200; params carry serde defaults like `fs.find`
}
pub struct SearchHit { pub path: String, pub line: u32, pub preview: String, pub ranges: Vec<(u32, u32)> }
pub struct FsSearchResult { pub hits: Vec<SearchHit>, pub truncated: bool }
```

- Walk: the `files::walker` that `fs.find` uses (`ignore::WalkBuilder`), so Search sees exactly the files the Files scope lists, `.gitignore` respected.
- Match: `grep-searcher` + `grep-regex` + `grep-matcher` (new dependencies, all from the ripgrep project; `grep-matcher` gives the match ranges).
  With `regex: false` the query is escaped; smart case: an all-lowercase query is case-insensitive.
- Skips binary files and files over 2 MB.
- Stops once `limit` hits are found and sets `truncated`.
- `path` is relative to the searched folder with `/` separators; `line` is 1-based; `preview` is the matching line trimmed and cut to 200 characters; `ranges` are byte ranges into `preview`.
- An invalid regex returns an error naming the problem.
- Concurrency and cancellation: `Daemon::connection` answers each request before reading the next one, so `fs.search` runs beside the read loop (replies on a connection may come out of order; the UI matches replies by id).
  Each connection keeps an `AtomicU64` search generation; a new `fs.search` bumps it, the walkers of older searches stop before opening their next file, and the replaced search answers `invalid_state`.
- The code carries `// ponytail: no index; add a persistent trigram index if search on big repos measurably lags`.

## 9. Delivery: three plans

| Plan | Contents | Depends on |
| --- | --- | --- |
| P1 foundation | ADR 0097; glass tokens, `shadow_float`, sizes; `floating_panel` and motion; `Look`, `PressStyle`, `press_style`, `Pressable`, `toggle`, `dropdown_trigger`, `list_row`; existing widgets rebuilt on `Pressable`; every dialog moved to `floating_panel`; Settings (§6). | - |
| P2 palette and protocol | Protocol 54 with §8; daemon handlers; the scoped palette (§7); Quick open removed; `Command::SearchInFiles`. | P1 |
| P3 audit | §5.3 over the 48 files; SPEC §14, skill and reviewer checklist updated with the rule. | P1 |

P2 and P3 may run in either order after P1.
Each plan ends with `scripts/verify.sh` green and screenshots (dark and light) of what it changed, or "check <view> visually (dark/light)" lines in `docs/HUMAN_CHECKS.md`.

## 10. Errors and edge cases

- The daemon answers after the palette closed or the query changed: the answer is dropped by serial.
- `git.branches` or `git.log` fails (not a repository, git missing): the scope shows the error as a muted row; other scopes keep working.
- `fs.search` on a huge repository: the answer comes back whole, capped by `limit`; a newer query cancels the walk.
- Checkout with local changes git refuses: the toast shows git's message; nothing else changes.
- Settings search with a query that matches only hidden settings: hidden settings are never listed; the empty state shows.
- A dialog taller than the window: `floating_panel` callers keep today's viewport caps.
- Reduced motion: the panel appears at once.

## 11. Testing

No test calls a real agent or the network; git tests create temporary repositories.
There is no headless harness for `Shell`, so shell wiring (Quick open opens Files, Esc in `@` mode) is covered by model tests on `palette` and by `docs/HUMAN_CHECKS.md` lines.

| Area | Behavior tests |
| --- | --- |
| `palette` | Tab / Shift+Tab cycle `Scope::ORDER`; All caps each section at `PALETTE_GROUP_LIMIT` and adds "+N more"; Search never appears in All; late serials are dropped; `Ctrl+Shift+O` opens Files; insert mode returns the path; Enter on each row kind yields its `Action`. |
| Settings | Search matches label, help, key and page; pages without matches are dimmed; Esc clears then closes; every setting has a group; groups are contiguous. |
| `Pressable` | `press_style` per `Look` in dark and light; disabled has `opacity::DISABLED`, no pointer, no hover. |
| Glass | Body text on `glass` keeps 4.5:1 over `bg` and over `text` for every built-in theme. |
| `fs.search` (daemon) | `.gitignore` respected; binary and > 2 MB files skipped; `limit` and `truncated`; smart case; invalid regex error; a newer search stops an older one. |
| `git.branches` / `git.checkout` | Newest first; current flag; remote duplicate dropped; `HEAD` dropped; checkout switches; git's error passes through. |
| Protocol | Serde round trips of the new params and results; `PROTOCOL_VERSION` is 54. |

## 12. Docs updated

- ADR 0097 (glass tokens, `shadow_float`, `Pressable` as the only place for hover and active).
- SPEC §13 (palette keys, `Ctrl+Alt+Shift+F`), §14 (glass, pressable rule, palette and Settings layout), the protocol section (three methods, version 54).
- `.claude/skills/mkrew-ui-design/SKILL.md` and `.claude/agents/ui-reviewer.md`: the pressable rule.
- `docs/HUMAN_CHECKS.md`: visual checks not captured automatically.
