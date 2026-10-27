# Sidebar Workspace Cards Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make each workspace in the full sidebar a visually separate card so several open workspaces no longer read as one long list.

**Architecture:** Render-only change in `crates/mkrew-ui/src/shell/sidebar.rs`: the row-gathering loop in `Shell::render_sidebar` wraps each workspace's header and rows into one card element (new free fn `workspace_card`), drops the `AGENTS` label, and two row tweaks follow (folded RUN shows ports, no green tint on finished-unseen rows). The view-model (`crates/mkrew-ui/src/sidebar.rs`) and protocol do not change.

**Tech Stack:** Rust, GPUI (`div()` builder, `FluentBuilder::when`), theme tokens in `crates/mkrew-ui/src/theme.rs`.

**Spec:** `docs/superpowers/specs/2026-10-19-sidebar-workspace-cards-design.md`

## Global Constraints

- Every color, spacing, radius, font size comes from `theme.rs` tokens; no hex or raw px outside `theme.rs` (SPEC §14, skill `mkrew-ui-design`).
- Spacing only 4 · 8 · 12 · 16 · 24 · 32 (`space::XS…XXL`); radius 4 / 8 / 12 (`radius::SMALL/MEDIUM/LARGE`).
- Both themes (dark / light) must work.
- Status never by color alone: the ✓ mark + semibold title must stay on finished-unseen rows.
- Deviation from SPEC needs ADR + SPEC update in the same commit: ADR number **0062** (last is 0061).
- Verify: `"C:/Program Files/Git/bin/bash.exe" scripts/verify.sh` (plain `bash` is WSL). Cargo with `-j 4`.
- Commits: ONE commit at the end of Task 3 (AGENTS.md: a spec deviation ships with its ADR and SPEC update in the same commit). Tasks 1–2 end with a checkpoint, not a commit. Explicit paths only, Conventional Commits, `git -c user.email=tungnv975@gmail.com commit …`. Never push. Do not commit the spec file (specs stay uncommitted).
- Do not touch the user's WIP: `.github/workflows`, `Cargo.lock`, `crates/mkrew-ui/Cargo.toml`, `packaging/`, `docs/BUILD_VI.md`, `docs/REDEPLOY_VI.md`.

## Review Focus

1. **Collapsed workspace** → a card with only its header; no empty indented box under it (Task 1 `workspace_card` skips the rows child when `rows` is empty).
2. **Workspace without a color** → card with no stripe, still separated by fill + border + gap (Task 1: `color` is `None` → no stripe child).
3. **Filter text or chip on** → cards show only matching rows; no `TERMINAL` label while filtering (Task 1 keeps `shown_project = open.filter(|_| !filtering)`).
4. **Drag a row inside a card** → reorder still works and the accent gap line shows inside the card (Task 1 keeps `self.reorderable(...)` per row).
5. **Light theme** → card (`#FFFFFF`) visible on sidebar (`#F4F5F7`) thanks to the `divider` border.

None of these has pure logic to unit-test: they are layout outcomes of `render_sidebar`, and this crate has no headless render harness. They are pinned by the HUMAN_CHECKS entry in Task 3 (items 1–5 each have a line there) plus the smoke run in Task 3 Step 7.

---

### Task 1: Workspace cards

**Files:**
- Modify: `crates/mkrew-ui/src/theme.rs` (after `WORKSPACE_AVATAR_TINY`, ~line 928)
- Modify: `crates/mkrew-ui/src/shell/sidebar.rs` — `Shell::render_sidebar` body branch (starts at the comment `// A workspace's AGENTS, TERMINAL and RUN groups hang under a faint guide line.`, ~lines 622–755); `workspace_row` name text size (~line 1213); new fn `workspace_card` right after `workspace_row`.

**Interfaces:**
- Consumes: `workspace_row(row, active, theme, cx) -> AnyElement`, `Shell::with_workspace_menu(row, project_id, theme, cx) -> AnyElement`, `Shell::render_run_group(project_id, theme, cx) -> Vec<AnyElement>`, `Shell::group_label(...) -> Stateful<Div>`, `Theme::workspace_color(&str) -> Option<u32>` (all existing).
- Produces: `theme::WORKSPACE_STRIPE_WIDTH: f32`; `fn workspace_card(header: gpui::AnyElement, color: Option<u32>, rows: Vec<gpui::AnyElement>, theme: &Theme) -> gpui::AnyElement` (private to `shell/sidebar.rs`).

No unit test: element layout only; the row model in `sidebar.rs` is unchanged and its tests keep passing.

- [ ] **Step 1: Add the stripe token**

In `crates/mkrew-ui/src/theme.rs`, after the `WORKSPACE_AVATAR_TINY` line, add:

```rust
/// The workspace color stripe on the left edge of a sidebar workspace card (ADR 0062).
pub const WORKSPACE_STRIPE_WIDTH: f32 = 3.0;
```

- [ ] **Step 2: Add `workspace_card`**

In `crates/mkrew-ui/src/shell/sidebar.rs`, right after the closing `}` of `pub(crate) fn workspace_row(…)` (before the `workspace_avatar` doc comment), add:

```rust
/// A workspace as one card (ADR 0062): its header row, then its rows indented under it. A
/// colored workspace gets a thin stripe of its color along the card's left edge; a collapsed
/// one is just the header.
fn workspace_card(
    header: gpui::AnyElement,
    color: Option<u32>,
    rows: Vec<gpui::AnyElement>,
    theme: &Theme,
) -> gpui::AnyElement {
    div()
        .relative()
        .flex()
        .flex_col()
        .p(px(space::XS))
        .rounded(px(radius::MEDIUM))
        .bg(rgb(theme.card))
        .border(px(theme::BORDER_WIDTH))
        .border_color(rgb(theme.divider))
        .children(color.map(|color| {
            div()
                .absolute()
                .left_0()
                .top(px(space::S))
                .bottom(px(space::S))
                .w(px(theme::WORKSPACE_STRIPE_WIDTH))
                .rounded(px(radius::SMALL))
                .bg(rgb(color))
        }))
        .child(header)
        .when(!rows.is_empty(), |el| {
            el.child(div().pl(px(space::L)).flex().flex_col().children(rows))
        })
        .into_any_element()
}
```

- [ ] **Step 3: Gather rows into cards**

In `Shell::render_sidebar`, replace the whole `} else { … };` branch of `let body = if connected && self.sidebar.is_empty() { … }` — from the line `// A workspace's AGENTS, TERMINAL and RUN groups hang under a faint guide line.` through the closing `div().flex().flex_col().px(px(space::XS)).children(list).into_any_element()` and its `};` — with:

```rust
        } else {
            let filtering = self.sidebar.filtering();
            let mut list = Vec::new();
            let mut agents = Vec::new();
            let mut terminals = Vec::new();
            // The workspace being gathered: its header row (with its menu) and its color.
            let mut card: Option<(gpui::AnyElement, Option<u32>)> = None;
            // Its id while it is open: its RUN group closes the rows.
            let mut open: Option<Ulid> = None;
            let mut empty = Vec::new();
            // Ends the gathered workspace as one card: header, agents (never labeled), the
            // TERMINAL group (labeled only beside agents or a RUN group), the RUN group.
            let close = |list: &mut Vec<gpui::AnyElement>,
                         card: Option<(gpui::AnyElement, Option<u32>)>,
                         agents: &mut Vec<gpui::AnyElement>,
                         terminals: &mut Vec<gpui::AnyElement>,
                         open: Option<Ulid>,
                         cx: &mut Context<'_, Self>| {
                let mut rows = std::mem::take(agents);
                let Some((header, color)) = card else {
                    terminals.clear();
                    return;
                };
                let shown_project = open.filter(|_| !filtering);
                let run = shown_project
                    .map(|project_id| self.render_run_group(project_id, theme, cx))
                    .unwrap_or_default();
                // A label (and so folding) needs the workspace's own, unfiltered rows.
                let labeled = !terminals.is_empty() && (!rows.is_empty() || !run.is_empty());
                match shown_project.filter(|_| labeled) {
                    Some(project_id) => {
                        let collapsed =
                            (self.collapsed_groups).contains(&(project_id, RowGroup::Terminal));
                        let count = terminals.len();
                        rows.push(
                            self.group_label(
                                project_id,
                                RowGroup::Terminal,
                                count,
                                collapsed,
                                theme,
                                cx,
                            )
                            .into_any_element(),
                        );
                        if collapsed {
                            terminals.clear();
                        } else {
                            rows.append(terminals);
                        }
                    }
                    None => rows.append(terminals),
                }
                rows.extend(run);
                list.push(workspace_card(header, color, rows, theme));
            };
            for row in view.rows {
                match row {
                    Row::Workspace(row) => {
                        close(
                            &mut list,
                            card.take(),
                            &mut agents,
                            &mut terminals,
                            open.take(),
                            cx,
                        );
                        let (project_id, active) =
                            (row.project_id, self.workspace == Some(row.project_id));
                        // Nothing to show under it: on the "N empty workspaces" line instead
                        // (the shown and pinned ones stay in place).
                        if row.empty && !active && !row.pinned && !filtering {
                            empty.push(row);
                            continue;
                        }
                        open = row.expanded.then_some(project_id);
                        let color =
                            (row.color.as_deref()).and_then(|name| theme.workspace_color(name));
                        let row = workspace_row(row, active, theme, cx);
                        card = Some((self.with_workspace_menu(row, project_id, theme, cx), color));
                    }
                    Row::Session(row) => {
                        let (selected, session_id) =
                            (shown == Some(row.session_id), row.session_id);
                        let (group, rows_of) = if row.shell {
                            (RowGroup::Terminal, &mut terminals)
                        } else {
                            (RowGroup::Agents, &mut agents)
                        };
                        let item = DraggedItem {
                            list: ReorderList::Rows(row.project_id, group),
                            index: rows_of.len(),
                            title: row
                                .title
                                .clone()
                                .unwrap_or_else(|| row.agent.clone())
                                .into(),
                        };
                        // One lookup, for the selected row only.
                        let progress = selected.then(|| self.plans.fraction(session_id)).flatten();
                        let badge = self.agent_badge(&row.agent);
                        let row = session_row(row, badge, selected, progress, now, theme, cx);
                        let row = self.with_peek(row, session_id, now, theme, cx);
                        // Dragging reorders the whole group: off while a filter hides rows.
                        rows_of.push(if filtering {
                            row
                        } else {
                            self.reorderable(row, item, false, 0.0, theme, cx)
                        });
                    }
                    // Hide what's empty: an open workspace without sessions shows nothing.
                    Row::NoSessions { .. } => {}
                }
            }
            close(&mut list, card.take(), &mut agents, &mut terminals, open, cx);
            if !empty.is_empty() {
                list.push(self.render_empty_workspaces(empty, theme, cx));
            }
            if list.is_empty() && filtering {
                // A query or chip that nothing matches.
                list.push(
                    div()
                        .px(px(space::S))
                        .py(px(space::XS))
                        .text_size(font_size::META)
                        .text_color(rgb(theme.text_muted))
                        .child("No sessions match.")
                        .into_any_element(),
                );
            }
            // Cards apart from each other and from the sidebar edges.
            div()
                .flex()
                .flex_col()
                .gap(px(space::S))
                .p(px(space::S))
                .children(list)
                .into_any_element()
        };
```

The `guide` closure is gone (the card replaces the guide line). `RowGroup::Agents` stays: drag still uses `ReorderList::Rows(project_id, RowGroup::Agents)`; only its label is never drawn.

- [ ] **Step 4: Bigger workspace name**

In `workspace_row`, in the name child (the block ending `.child(row.name)`), change `.text_size(font_size::BODY)` to `.text_size(font_size::CARD_TITLE)` (keep `.font_weight(FontWeight(weight::SEMIBOLD))`).

Change the first line of the doc comment above `workspace_row` to:
`/// A workspace header row (the top of its card, ADR 0062): chevron, avatar, bold name, pin; on the`

- [ ] **Step 5: Build, lint, test**

Run: `cargo fmt --all && cargo clippy -p mkrew-ui --all-targets -j 4 -- -D warnings && cargo test -p mkrew-ui -j 4 sidebar`
Expected: no warnings (an unused-closure warning means the old `guide` closure is still there — delete it); all `sidebar::tests` pass.

- [ ] **Step 6: Grep the diff for literals**

Run: `git diff -U0 -- crates/mkrew-ui/src/shell/sidebar.rs | grep -nE '#[0-9A-Fa-f]{6}|px\([0-9]'`
Expected: no output.

- [ ] **Step 7: Checkpoint (no commit)**

Leave the changes in the working tree; Task 3 commits them with the ADR and SPEC. Reviewer reads `git diff -- crates/mkrew-ui/src/theme.rs crates/mkrew-ui/src/shell/sidebar.rs`.

---

### Task 2: Quieter rows — folded RUN shows ports, no tint on finished-unseen rows

**Files:**
- Modify: `crates/mkrew-ui/src/shell/sidebar.rs` — `Shell::render_run_group` (after `let mut label = self.group_label(project_id, RowGroup::Run, …)`, ~line 67); `session_row` (the `// A finish not yet viewed: tinted until the pane is focused.` block); `compact_session_row` (the `.when(row.unseen && !selected, …)` block).
- Modify: `crates/mkrew-ui/src/sidebar.rs` — doc comment of `SessionRow::unseen` (~line 254).

**Interfaces:**
- Consumes: `run::Summary::ports_text(&self) -> Option<String>` (existing; tested by `run::tests::running_commands_show_count_and_ports_in_config_order` and `more_than_four_ports_are_counted`).
- Produces: nothing new.

No new unit test: `ports_text` is already tested; the rest removes a fill.

- [ ] **Step 1: Ports on the folded RUN label**

In `Shell::render_run_group`, directly after

```rust
        let mut label =
            self.group_label(project_id, RowGroup::Run, rows.len(), collapsed, theme, cx);
```

add:

```rust
        // Folded, the label still says what runs: `:8082 :5180` (ADR 0062).
        if collapsed && let Some(ports) = summary.ports_text() {
            label = label.child(
                div()
                    .flex_none()
                    .whitespace_nowrap()
                    .font_family(theme.terminal_font_family.clone())
                    .font_weight(FontWeight(weight::REGULAR))
                    .text_color(rgb(theme.status_done))
                    .child(ports),
            );
        }
```

- [ ] **Step 2: Drop the tint in `session_row`**

Delete from `session_row`:

```rust
        // A finish not yet viewed: tinted until the pane is focused.
        .when(unseen && !selected, |el| {
            el.bg(theme.tint(theme.status_done))
        })
```

Keep `let unseen = row.unseen;`: the title still uses it for `weight::SEMIBOLD`.

- [ ] **Step 3: Drop the tint in `compact_session_row`**

Delete:

```rust
        .when(row.unseen && !selected, |el| {
            el.bg(theme.tint(theme.status_done))
        })
```

The rail's `mark_slot(row.mark(), …)` still shows done's ✓ for an unseen row; `selected` is still read by `.when(selected, |el| el.bg(rgb(theme.selected)))`.

- [ ] **Step 4: Fix the `unseen` doc**

In `crates/mkrew-ui/src/sidebar.rs`, replace the doc comment on `SessionRow::unseen`:

```rust
    /// Finished (done, or idle right after working) and not viewed since: the row is
    /// highlighted until its pane is focused ([`inbox::needs_you`]).
```

with:

```rust
    /// Finished (done, or idle right after working) and not viewed since: the row shows done's
    /// ✓ and a semibold title until its pane is focused ([`inbox::needs_you`]).
```

- [ ] **Step 5: Build, lint, test**

Run: `cargo fmt --all && cargo clippy -p mkrew-ui --all-targets -j 4 -- -D warnings && cargo test -p mkrew-ui -j 4 sidebar run::`
Expected: no warnings, all pass.

- [ ] **Step 6: Checkpoint (no commit)**

Leave the changes in the working tree; Task 3 commits them. Reviewer reads `git diff -- crates/mkrew-ui/src/shell/sidebar.rs crates/mkrew-ui/src/sidebar.rs`.

---

### Task 3: ADR, SPEC, HUMAN_CHECKS, full verify

**Files:**
- Create: `docs/decisions/0062-sidebar-workspace-cards.md`
- Modify: `docs/SPEC.md` §10 (~lines 751–761), §11 (~lines 985–989), §14 rule 2 (~lines 1278–1280)
- Modify: `docs/HUMAN_CHECKS.md` (new entry at the top of `## To verify (non-blocking)`, before `### To verify — 2026-10-15 Ask AI dialog (ADR 0061)`)

- [ ] **Step 1: Check the ADR number is free**

Run: `ls docs/decisions | sort | tail -n 2`
Expected: last is `0061-…`. If `0062` is taken, use the next free number here and in the `ADR 0062` comments from Tasks 1–2.

- [ ] **Step 2: Write the ADR**

`docs/decisions/0062-sidebar-workspace-cards.md`:

```markdown
# 0062 — Sidebar workspaces as cards

- Date: 2026-10-19
- Status: accepted (extends ADR 0020, ADR 0054; SPEC §10, §11, §14)
- Milestone: none (user request 2026-10-19)

## Context

With five workspaces open the full sidebar read as one long list: a workspace row had the
height and weight of a session row, nothing separated one workspace's rows from the next
header, the workspace color showed only on a 20 px avatar, every mixed workspace added
`AGENTS` / `TERMINAL` / `RUN` label lines, and the green tint of finished-unseen rows was the
loudest thing on screen.

## Decision

- Each workspace in the full sidebar is one card: `card` fill, 1 px `divider` border, radius 8,
  4 px padding, 8 px apart. Its rows sit 16 px in; the 1 px guide line is gone.
- A colored workspace has a 3 px stripe of its color on the card's left edge
  (`WORKSPACE_STRIPE_WIDTH`). No automatic color for uncolored workspaces.
- The workspace name is `CARD_TITLE` size, semibold.
- No `AGENTS` label; `TERMINAL` is labeled only beside agents or a RUN group; `RUN` keeps its
  label and, while folded, shows its ports (`:8082 :5180`).
- Finished-unseen rows lose the green fill; ✓ and the semibold title stay until viewed.
- The compact rail and the `N empty workspaces` line are unchanged.

## Consequences

- Workspaces are told apart by shape, gap and color stripe without reading names.
- Cards cost about 16 px of height per workspace (padding + gap); each mixed workspace saves
  the `AGENTS` line.
- Not done: sticky headers, automatic colors, one-open-at-a-time accordion; each can be added
  if the cards are not enough.
```

- [ ] **Step 3: Update SPEC §10**

In `docs/SPEC.md` §10 **Sidebar**, replace the text from `A workspace row: chevron, colored letter avatar, bold name, 📌, then its status: a` through `guide line: agent glyph (in an accent chip while working; its hover says the state), title` with:

```text
  Each workspace is a card (ADR 0062): `card` fill, faint border, radius 8, 8 px between cards,
  a 3 px stripe of the workspace color on its left edge (none without a color). Its header row:
  chevron, colored letter avatar, bold name (card-title size), 📌, then its status: a
  `● 1 working` pill (pulsing dot) while an agent works, else muted `2 idle`, plus amber `? N`
  while N need input; the ⋯ menu on hover (or right-click). Its sessions sit indented in the
  card, agents first without a label (TERMINAL is labeled beside agents or RUN): agent glyph
  (in an accent chip while working; its hover says the state), title
```

In the same paragraph, change `name and its ports in green monospace; a running one opens its pane on click` to `name and its ports in green monospace (folded, its label shows the ports); a running one opens its pane on click`.

- [ ] **Step 4: Update SPEC §11 and §14**

§11 Workspace row bullet: change `Sidebar session rows` / `hang under a faint guide line:` to `Sidebar session rows` / `sit indented in the workspace's card (ADR 0062):`.

Then run `grep -n -iE "not yet viewed|unseen|highlight" docs/SPEC.md`; where a sidebar row of a finished, not-yet-viewed agent is described as highlighted or tinted, change it to say it shows ✓ and a semibold title (no fill).

§14 rule 2: change `a small tinted letter avatar per` / `workspace,` to `a small tinted letter avatar per` / `workspace and a 3 px stripe on its sidebar card,`.

- [ ] **Step 5: Add the HUMAN_CHECKS entry**

Insert before `### To verify — 2026-10-15 Ask AI dialog (ADR 0061)`:

```markdown
### To verify — 2026-10-19 Sidebar workspace cards (ADR 0062)

- [ ] **Cards (dark + light):** with 3+ workspaces open, each is a separate rounded card, 8 px
  apart, visible on the sidebar in light theme too; a colored workspace has a thin stripe of its
  color on the left, an uncolored one none. A collapsed workspace is a card with only its
  header. The name is a bit larger than the rows.
- [ ] **Groups:** no `AGENTS` line; a workspace with agents and shells shows `TERMINAL N`; fold
  RUN → its label shows the ports (`:8082 :5180`) in green.
- [ ] **Finished agent:** let an agent finish while another pane is focused → its row has ✓ and a
  bold title but no green fill; focusing it clears both. Same in the compact rail (✓ only).
- [ ] **Still works:** filter text and chips (only matching cards, no TERMINAL label), drag a
  row inside a card (accent gap line), right-click a header / row, hover peek,
  `N empty workspaces ▸`.
```

- [ ] **Step 6: Full verify**

Run: `"C:/Program Files/Git/bin/bash.exe" scripts/verify.sh`
Expected: fmt check, clippy `-D warnings` and all tests pass. If an exe is locked by a running Mkrew, rename it to `*.locked-old.exe` (never kill the process) and rerun.

- [ ] **Step 7: Smoke the real surface**

Run: `cargo build -p mkrew-ui -j 4`. Start `"$PWD/target/debug/mkrew.exe"` and look at the sidebar with 3+ workspaces (dark and light, toggle via the title bar theme button). If the dev UI cannot attach to the running daemon (version mismatch), do not install or stop anything: leave it to the HUMAN_CHECKS entry and say so in the report.

- [ ] **Step 8: Commit everything once**

```bash
git add crates/mkrew-ui/src/theme.rs crates/mkrew-ui/src/shell/sidebar.rs crates/mkrew-ui/src/sidebar.rs docs/decisions/0062-sidebar-workspace-cards.md docs/SPEC.md docs/HUMAN_CHECKS.md
git -c user.email=tungnv975@gmail.com commit -m "feat(ui): sidebar workspaces as cards (ADR 0062)"
```
