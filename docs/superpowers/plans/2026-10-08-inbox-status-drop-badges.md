# Inbox 2026-10-08 (2): Status Icons, File Drop, Agent Badges Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Handle three items from `docs/INBOX.md`:
- Status icons for in progress / idle / done / needs input.
- Drag and drop a file into a terminal.
- An icon for each agent.

**Architecture:**
- Status icons already exist (SPEC §11, `sidebar::status_display` + `status::icon`). The states the user misses (done, needs input) come only from hooks, and **no hook integration is installed** on this machine. Task 1 is the setup step plus one small render gap (the workspace row's working count does not spin).
- File drop already exists (T10.8, `TerminalPane::drop_paths`). Task 2 is the open manual check, no code.
- Agent badges are new: an optional `icon` key in the agent profile TOML (SPEC §2.1: agent differences live in TOML), carried on `AgentProfileInfo`, cached by the UI on refresh, shown before the agent name. A missing key falls back to the id's first letter. ADR 0025.

**Tech Stack:** Rust 2024, GPUI (`gpui-pre =0.3.7`), serde/toml, insta. No new crates.

**Spec:** `docs/SPEC.md` §6, §10, §11, §20B.5, cited per task. There is no separate design doc. This plan comes from `docs/INBOX.md` (2026-10-08, lines 18–20) and the investigation quoted under each task. Open questions are listed at the end, each with the default this plan uses.

**Inbox mapping:**

| Inbox line | Task |
| --- | --- |
| Should have icon the the agent that showing when agent inprogress, iddle, done, or ask me | 1 |
| Check if we don't have feature to drag drop a file into the terminal chat | 2 |
| For the agent in the workspace do we have icon for each agent, if not yet then | 3 |

## Global Constraints

- Windows native is first-class: ConPTY, named pipes, `PATHEXT`, `dunce` paths (AGENTS.md, SPEC §2.4).
- "An agent is just a terminal command. No per-agent code in the core." (SPEC §2.1). Every agent difference lives in its TOML profile — no `match "claude"` anywhere in Rust code.
- Every color, spacing, radius and font size comes from `crates/mkrew-ui/src/theme.rs` (SPEC §14).
- No `unwrap`/`expect` outside tests unless provably infallible, with a comment saying why.
- `thiserror` in libraries, `anyhow` with context in binaries, `tracing` for logs.
- No test may call a real agent or the network.
- Deviating from the spec requires an ADR in `docs/decisions/` and a SPEC update in the same change. Next free ADR number: 0025.
- Few dependencies: add no new crates.
- GPUI rule: set `.hover()` at most once per element (`button()`/`quiet_button()` already set it).
- **User preference: never commit.** Leave every change uncommitted. The last step of each task prints the Conventional Commit message the user may use; do not run `git commit`.
- Leave the existing uncommitted human edits alone (`.github/workflows/*`, `Cargo.lock`, `crates/mkrew-ui/Cargo.toml`, `packaging/mkrew.iss`, `docs/BUILD_VI.md`, `design/`).
- Never write files outside this repo. Hook installs (`mk integration install …`) and edits to the user's own `<config>/mkrew/agents/*.toml` are **user steps**, listed in HUMAN_CHECKS.
- **Inbox:** when a task passes verification, change its `docs/INBOX.md` line from `[~]` to `[x]` (see the mapping above).
- Verify on Windows with Git Bash: `& "C:\Program Files\Git\bin\bash.exe" scripts/verify.sh`. If rustc crashes on mkrew-ui test builds (STATUS_STACK_BUFFER_OVERRUN), run these three in order:
  1. `cargo fmt --all -- --check`
  2. `cargo clippy --workspace --all-targets -- -D warnings`
  3. `cargo test --workspace -j 4`
- If a build fails with "Access is denied" on an `.exe` (a running daemon or fake agent holds it), rename it to `*.locked-old.exe`. Never kill the user's sessions.
- `cargo insta` is not installed. Write accepted `.snap` files by hand.

## Review Focus

1. **Existing user profiles have no `icon`** (seeding never overwrites `<config>/mkrew/agents/*.toml`). Expected: they still parse, and the row shows the id's first letter. Pinned by Task 3 `icon_is_trimmed_optional_and_short` (no key → `None`) and `agent_badge_uses_the_profile_icon_else_the_first_letter`.
2. **A long or blank `icon`** (`icon = "claude-code"`, `icon = "  "`). Expected: the long one is a profile error with `file:line:col` (it would stretch every row); the blank one means "no icon". Pinned by Task 3 `icon_is_trimmed_optional_and_short`.
3. **`agent.profiles` fails or an older daemon omits `icon`.** Expected: the sidebar and panes still load; badges fall back to letters. Pinned by Task 3 `agent_profile_info_without_icon_decodes`; the refresh keeps the listing independent of the profiles call (Step 8).
4. **A user expects "done" / "needs input" with no hook installed.** Expected: by SPEC §6, `done` is hook-only and omp/Copilot have no `needs_input_patterns`, so those states never show. Task 1 makes this a setup step with a manual check; the status hover already names the source (`screen`).
5. **A dropped file while Mkrew runs elevated.** Expected: Windows blocks Explorer → elevated-window drops (UIPI), nothing is typed. Not code; listed in Task 4's manual check so a failure there is not misread as a bug.

---

## File Structure

| File | Change | Task |
| --- | --- | --- |
| `crates/mkrew-ui/src/main.rs` | workspace row count spins; `agent_icons` cache; badge in session row + pane header | 1, 3 |
| `crates/mkrew-core/src/profile.rs` | `icon: Option<String>`, ≤ 3 chars | 3 |
| `crates/mkrew-core/src/snapshots/mkrew_core__profile__tests__snapshot_unknown_field.snap` | allowed key list gains `icon` | 3 |
| `crates/mkrew-protocol/src/message.rs` | `AgentProfileInfo.icon` | 3 |
| `crates/mkrew-daemon/src/server.rs` | map `icon` | 3 |
| `crates/mkrew-ui/src/sidebar.rs` | `agent_badge` | 3 |
| `crates/mkrew-ui/src/{workspace_menu,task_dialog}.rs` | test helpers: `icon: None` | 3 |
| `agents/*.toml` | `icon = "…"` | 3 |
| `docs/SPEC.md`, `docs/decisions/0025-agent-badges.md` | spec, ADR | 3 |
| `docs/HUMAN_CHECKS.md`, `docs/PROGRESS.md`, `docs/INBOX.md` | checks, notes, ticks | 1–4 |

---

### Task 1: Status icons — hook setup and a spinning workspace count

Inbox: `Should have icon the the agent that showing when agent inprogress, iddle, done, or ask me`. Spec: §6 (session states, hook integrations), §11.

**Current state (investigated):**
- Every state already has an icon and a color: `sidebar::status_display` (sidebar.rs:19-30), `sidebar::status_color` (sidebar.rs:33-42), animated by `status::icon` (status.rs:25-32, ◐◓◑◒ on the 500 ms `theme::STATUS_TICK`). They render in the sidebar session row (main.rs:15008), Inbox row, pane header (main.rs:16078-16083), tab label, board card, fan-out row, peek, send dialog.
- **Root cause of the missing states:** `mk integration status claude|codex|omp|copilot` prints `not installed` for all four (run 2026-10-08). By SPEC §6, `done` comes only from a hook, and `agents/omp.toml` / `agents/copilot.toml` have `needs_input_patterns = []`, so without hooks those sessions only ever show working ↔ idle. Claude and Codex catch some prompts through screen patterns, never `done`.
- **Render gap:** the collapsed workspace row shows the working count with the fixed glyph `◐` (main.rs:14931-14939, `sidebar::status_display(SessionState::Working)`), so it never spins while every other place does.

**Files:**
- Modify: `crates/mkrew-ui/src/main.rs:14839-14846` (`workspace_row` signature), `:14931-14939` (count), `:7093` (caller)

- [ ] **Step 1: Spin the workspace row's working count**

Add `now: i64` to `workspace_row` (after `tokens`):

```rust
fn workspace_row(
    row: WorkspaceRow,
    active: bool,
    run_tip: Option<String>,
    tokens: Option<u64>,
    now: i64,
    theme: &Theme,
    cx: &mut Context<'_, Shell>,
) -> gpui::AnyElement {
```

Replace main.rs:14931-14939 with:

```rust
        .when(row.working > 0, |el| {
            el.child(
                div()
                    .text_size(font_size::META)
                    .text_color(rgb(theme.status_working))
                    // Same spinner as the session rows (§11).
                    .child(format!("{} {}", status::icon(SessionState::Working, now), row.working)),
            )
        })
```

Caller main.rs:7093 (`now` is already in scope, it is passed to `session_row` at :7106):

```rust
                            let row = workspace_row(row, active, run_tip, tokens, now, theme, cx);
```

No new test: `status::icon` is pinned by `status::tests::spinner_turns_only_while_working`, and the redraw is already driven by `status::redraw_due` (any working session). This step is wiring only.

- [ ] **Step 2: Build**

Run: `cargo clippy -p mkrew-ui --all-targets -j 4 -- -D warnings`
Expected: no warnings (`workspace_row` now has 7 arguments, the clippy limit).

- [ ] **Step 3: Record the setup step**

The hook install writes outside the repo, so it is a user step. Task 4 adds it to HUMAN_CHECKS.

- [ ] **Step 4: Verify and tick**

Run the full verification. Tick inbox line 18 `[x]` after the user confirms the Task 4 check "states with hooks" (until then leave it `[~]` and say so in the report).
Suggested message: `fix(ui): workspace row working count spins like the session rows`

---

### Task 2: File drop into a terminal — already built, manual check only

Inbox: `Check if we don't have feature to drag drop a file into the terminal chat if not need to add in`. Spec: §20B.5 (SPEC.md:1371-1375), M10 criterion (SPEC.md:1185).

**Current state (investigated):** built in T10.8 (`docs/progress-archive.md:78`).
- `TerminalPane` root div: `.on_drop(cx.listener(Self::drop_paths))` (crates/mkrew-ui/src/terminal/view.rs:627), on every live pane, agent or shell.
- `drop_paths` (view.rs:542-555) → `type_paths` (view.rs:558-569) → `composer::typed_paths` (quotes a path with `"` only when it has whitespace, space-joins several) → `paste_bytes(text, input_mode)` (bracketed when the program asked) → `session.input`. No Enter.
- Windows delivers drops: `gpui-pre-windows-0.3.7/src/window.rs:1137-1280` implements `IDropTarget` and emits `FileDropEvent`; `RegisterDragDrop` at :1517.
- Known limits (HUMAN_CHECKS.md:1872-1877): no drag-over highlight; no drag from the app's own Files view.
- The manual Windows check at HUMAN_CHECKS.md:1878-1881 is still open.

**Files:** none (Task 4 adds a pointer in HUMAN_CHECKS).

- [ ] **Step 1: Confirm nothing regressed**

Run: `cargo test -p mkrew-ui --lib composer::tests::typed_paths_are_space_separated_and_quoted_with_whitespace -j 4`
Expected: PASS.

- [ ] **Step 2: Tick**

Tick inbox line 19 `[x]` and append ` (already built in T10.8; manual check in HUMAN_CHECKS)`. If Open question 3 gets answer b or c, add that work as a new task instead.

No commit message: no code change.

---

### Task 3: Agent badges from the profile's `icon` key (ADR 0025)

Inbox: `For the agent in the workspace do we have icon for each agent, if not yet then` (the line is cut off; this plan reads it as "add one", see Open question 4). Spec: §6, §10, §11.

**Current state (investigated):**
- No per-agent icon exists. The session row (main.rs:14978-14981) and pane header (main.rs:16084-16090) print the profile id (`row.agent` / `session.agent_profile`) after the shared status icon.
- `AgentProfile` (profile.rs:16-34) and `RawProfile` (profile.rs:315-333, `deny_unknown_fields`) have no icon field. `AgentProfileInfo` (message.rs:1287-1295) is the wire shape; the daemon maps it in `Server::agent_profiles` (server.rs:1308-1331).
- The UI fetches `agent.profiles` only when a menu or dialog opens (main.rs:3768, 4073, 5181, 5281). `Shell::refresh` (main.rs:660-737) loads projects, sessions and tasks.
- No SVG asset pipeline exists; all icons are Unicode glyphs. A glyph or 1–3 characters keeps that convention.
- The design mock (`design/sidebar.rs` `render_session`, :251-296) has no agent icon.

**Files:**
- Modify: `crates/mkrew-core/src/profile.rs:16-34, 293-313, 315-333`, tests after :460
- Modify: `crates/mkrew-core/src/snapshots/mkrew_core__profile__tests__snapshot_unknown_field.snap`
- Modify: `crates/mkrew-protocol/src/message.rs:1287-1295`, tests at :1962-1978
- Modify: `crates/mkrew-daemon/src/server.rs:1313-1318`
- Modify: `crates/mkrew-ui/src/sidebar.rs` (after :42; test after :393)
- Modify: `crates/mkrew-ui/src/main.rs` (`Shell` fields :242, init :591, `refresh` :660-737, :7106, `session_row` :14969, :13406, `pane_header` :16057)
- Modify: `crates/mkrew-ui/src/workspace_menu.rs:95-102`, `crates/mkrew-ui/src/task_dialog.rs:320-327`
- Modify: `agents/claude.toml`, `codex.toml`, `copilot.toml`, `fake.toml`, `omp.toml`, `shell.toml`
- Modify: `docs/SPEC.md` §6 (:352-375, :424-427), §11 (:731)
- Create: `docs/decisions/0025-agent-badges.md`

**Interfaces:**
- Produces: `mkrew_core::AgentProfile.icon: Option<String>` (trimmed, non-empty, ≤ 3 chars); `mkrew_protocol::AgentProfileInfo.icon: Option<String>` (`#[serde(default)]`); `mkrew_ui::sidebar::agent_badge(icon: Option<&str>, agent: &str) -> String`; `Shell.agent_icons: HashMap<String, String>` (profile id → icon); `Shell::agent_badge(&self, agent: &str) -> String`.

- [ ] **Step 1: Write the failing core test**

In `crates/mkrew-core/src/profile.rs` tests, after `usage_section_parses_known_and_unknown_formats`:

```rust
    #[test]
    fn icon_is_trimmed_optional_and_short() {
        let icon = |src: &str| parse(src).unwrap().icon;
        assert_eq!(icon("command = \"x\"\nicon = \" π \""), Some("π".into()));
        assert_eq!(icon("command = \"x\"\nicon = \">_\""), Some(">_".into()));
        // Blank = no icon, like an empty `resume`.
        assert_eq!(icon("command = \"x\"\nicon = \"  \""), None);
        // Profiles written before `icon` existed still parse.
        assert_eq!(icon("command = \"x\""), None);

        let long = parse_err("command = \"x\"\nicon = \"claude\"");
        assert!(long.starts_with("agents/broken.toml:2:"), "{long}");
        assert!(
            long.contains("`icon` must be at most 3 characters, got `claude`"),
            "{long}"
        );
    }
```

- [ ] **Step 2: Run it and confirm it fails**

Run: `cargo test -p mkrew-core --lib profile::tests::icon -j 4`
Expected: FAIL to compile, `no field icon on type AgentProfile`.

- [ ] **Step 3: Implement in core**

`AgentProfile` (after `name`, profile.rs:19):

```rust
    /// Short badge shown before the agent name (≤ [`MAX_ICON_CHARS`] characters, ADR 0025);
    /// `None` = the UI uses the id's first letter.
    pub icon: Option<String>,
```

Above `parse_profile`:

```rust
/// Longest `icon`: a glyph plus a variation selector, or two or three letters.
pub const MAX_ICON_CHARS: usize = 3;
```

`RawProfile` (after `name`, so the snapshot key list reads `name`, `icon`, …):

```rust
    icon: Option<Spanned<String>>,
```

In `parse_profile`, before `let mut needs_input_patterns`:

```rust
    let icon = match raw.icon {
        Some(icon) => {
            let span = icon.span();
            let icon = icon.into_inner().trim().to_owned();
            if icon.chars().count() > MAX_ICON_CHARS {
                return Err(err(
                    Some(span),
                    format!("`icon` must be at most {MAX_ICON_CHARS} characters, got `{icon}`"),
                ));
            }
            (!icon.is_empty()).then_some(icon)
        }
        None => None,
    };
```

and in the `Ok(AgentProfile { … })` literal, after `name`:

```rust
        icon,
```

`AgentProfile { ..parsed }` / `..self.clone()` / `..p` literals (profile.rs:124, 136, 502, 509; daemon launch.rs:178) use struct update syntax and need no change.

Update `crates/mkrew-core/src/snapshots/mkrew_core__profile__tests__snapshot_unknown_field.snap` last line to:

```
agents/broken.toml:2:1: unknown field `promt_mode`, expected one of `name`, `icon`, `kind`, `command`, `args`, `prompt_mode`, `env`, `resume`, `oneshot`, `state`, `usage`
```

- [ ] **Step 4: Run core tests**

Run: `cargo test -p mkrew-core --lib profile -j 4`
Expected: PASS, including `snapshot_unknown_field` and `repo_sample_profiles_parse`.

- [ ] **Step 5: Protocol field + compat test**

`crates/mkrew-protocol/src/message.rs` `AgentProfileInfo`, after `command`:

```rust
    /// The profile's badge (ADR 0025); absent from older daemons.
    #[serde(default)]
    pub icon: Option<String>,
```

In the `agent_profiles_result_round_trips` fixture add `icon: Some("ƒ".into()),`. After that test add:

```rust
    #[test]
    fn agent_profile_info_without_icon_decodes() {
        let info: AgentProfileInfo = serde_json::from_value(serde_json::json!({
            "id": "x", "name": "X", "kind": "agent", "command": "x"
        }))
        .unwrap();
        assert_eq!(info.icon, None);
    }
```

`PROTOCOL_VERSION` stays 26: an added `#[serde(default)]` field is compatible both ways.

Run: `cargo test -p mkrew-protocol agent_profile -j 4`
Expected: PASS.

- [ ] **Step 6: Daemon mapping and samples**

`crates/mkrew-daemon/src/server.rs:1313-1318`:

```rust
            .map(|p| AgentProfileInfo {
                kind: session_kind(p.kind),
                id: p.id,
                name: p.name,
                command: p.command,
                icon: p.icon,
            })
```

Add one line after `name = …` in each sample (comment only in `claude.toml`, the documented one):

| File | Line |
| --- | --- |
| `agents/claude.toml` | `# Short badge before the agent name (≤ 3 characters). Missing = the id's first letter.`<br>`icon    = "✻"` |
| `agents/codex.toml` | `icon = "⌬"` |
| `agents/copilot.toml` | `icon = "⊙"` |
| `agents/fake.toml` | `icon = "ƒ"` |
| `agents/omp.toml` | `icon = "π"` |
| `agents/shell.toml` | `icon = ">_"` |

Fix the two UI test helpers that build `AgentProfileInfo` literally (`workspace_menu.rs:96-101`, `task_dialog.rs:321-326`): add `icon: None,`.

Run: `cargo test -p mkrew-core --lib profile::tests::repo_sample_profiles_parse -j 4`
Expected: PASS.

- [ ] **Step 7: UI — failing badge test, then the helper**

`crates/mkrew-ui/src/sidebar.rs` tests, after `every_state_has_an_icon_and_text`:

```rust
    #[test]
    fn agent_badge_uses_the_profile_icon_else_the_first_letter() {
        assert_eq!(agent_badge(Some("π"), "omp"), "π");
        assert_eq!(agent_badge(None, "claude"), "C");
        // Uppercasing works beyond ASCII.
        assert_eq!(agent_badge(None, "ølm"), "Ø");
    }
```

Run: `cargo test -p mkrew-ui --lib sidebar::tests::agent_badge -j 4` → FAIL (`cannot find function agent_badge`).

After `status_color` (sidebar.rs:42):

```rust
/// The badge before an agent name (ADR 0025): the profile's `icon`, else the first letter of
/// its id, uppercased (profiles written before `icon` existed, or not loaded yet).
pub fn agent_badge(icon: Option<&str>, agent: &str) -> String {
    match icon {
        Some(icon) => icon.to_owned(),
        None => agent.chars().next().map(|c| c.to_uppercase().collect()).unwrap_or_default(),
    }
}
```

Run it again → PASS.

- [ ] **Step 8: UI — cache the icons on refresh**

`Shell` field (after `ports`, main.rs:242) and init (after `ports: Ports::default(),`, main.rs:591):

```rust
    /// Profile id → `icon` (ADR 0025), reloaded on every refresh.
    agent_icons: HashMap<String, String>,
```

```rust
            agent_icons: HashMap::new(),
```

Method on `Shell` (next to `refresh`):

```rust
    fn agent_badge(&self, agent: &str) -> String {
        sidebar::agent_badge(self.agent_icons.get(agent).map(String::as_str), agent)
    }
```

In `refresh` (main.rs:671-680), start the call with the others and await it separately, so a failure never blocks the listing:

```rust
        let profiles = connection.call::<AgentProfilesResult>(Request::AgentProfiles);
```

```rust
            let tasks = tasks.await;
            let profiles = profiles.await;
```

and inside `this.update_in`, before `match tasks`:

```rust
                match profiles {
                    Ok(listed) => {
                        shell.agent_icons = listed
                            .profiles
                            .into_iter()
                            .filter_map(|p| Some((p.id, p.icon?)))
                            .collect();
                    }
                    // Badges fall back to letters; the listing below is unaffected.
                    Err(error) => tracing::warn!("agent.profiles failed: {error:#}"),
                }
```

- [ ] **Step 9: UI — render the badge**

`session_row` gains `badge: String` after `row` (main.rs:14969); insert this child between the status icon child (:15007-15012) and the name child (:15013):

```rust
        .child(
            div()
                .flex_none()
                .text_size(font_size::BODY)
                .text_color(rgb(theme.text_muted))
                .child(badge),
        )
```

Caller main.rs:7106:

```rust
                            let badge = self.agent_badge(&row.agent);
                            let row = session_row(row, badge, selected, now, theme, cx);
```

`pane_header` gains `badge: String` after `session` (main.rs:16058); insert between the status icon child (:16078-16083) and the agent name child (:16084):

```rust
        .child(div().flex_none().text_color(rgb(theme.text_muted)).child(badge))
```

Caller main.rs:13406: pass `self.agent_badge(&info.agent_profile),` as the second argument.

Run: `cargo clippy -p mkrew-ui --all-targets -j 4 -- -D warnings`
Expected: clean (`pane_header` already allows `too_many_arguments`; `session_row` has 6).

- [ ] **Step 10: ADR and SPEC**

Create `docs/decisions/0025-agent-badges.md`:

```markdown
# 0025 — Agent badges from the profile's `icon`

- Date: 2026-10-08
- Status: accepted
- Milestone: inbox 2026-10-08

## Context

The user asked for an icon per agent in the workspace. Rows and pane headers showed only the
profile id. SPEC §2.1 forbids per-agent code; the UI has no SVG asset pipeline.

## Options

1. Optional `icon` key in the profile TOML (≤ 3 characters), sent on `agent.profiles`, cached by
   the UI on refresh; missing = the id's first letter.
2. UI-only letter badge from the id — no config, but `claude` and `copilot` both read `C`.
3. Brand SVG logos — needs an asset pipeline and logo licensing.

## Decision

Option 1. The agent difference stays in TOML (§2.1), glyphs match the existing Unicode icons, and
old profiles keep working through the letter fallback (seeding never overwrites them). A longer
`icon` is a profile error so rows cannot stretch. Shown in the sidebar session row and the pane
header, muted, between the status icon and the agent name; status keeps its own icon and text
(§11).
```

`docs/SPEC.md` §6:
- In the `agents/claude.toml` block, after `name    = "Claude Code"` (:354), add:

```toml
# Short badge before the agent name (≤ 3 characters). Missing = the id's first letter.
icon    = "✻"
```

- In the codex / omp / copilot blocks, after their `name =` line, add `icon = "⌬"` / `icon = "π"` / `icon = "⊙"`.
- In the key paragraph (:424-427), after the `kind` sentence, add: ``Optional key `icon` (≤ 3 characters, ADR 0025) is the badge shown before the agent name; a missing or blank one shows the id's first letter, uppercased; a longer one is a parse error.``

`docs/SPEC.md` §11 (:731), replace the pane header line with:

```markdown
- Pane header: status icon, agent badge (§6 `icon`), agent name, session/task title, time. The
  sidebar session row also puts the badge before the agent name.
```

- [ ] **Step 11: Verify and tick**

Run the full verification. Tick inbox line 20 `[x]`.
Suggested message: `feat: agent badges from the profile icon key (ADR 0025)`

---

### Task 4: Manual checks and notes

**Files:**
- Modify: `docs/HUMAN_CHECKS.md` (new section under "To verify")
- Modify: `docs/PROGRESS.md` ("Notes for the next session": one line)

- [ ] **Step 1: HUMAN_CHECKS**

Add `### To verify — 2026-10-08 inbox: status, drop, badges (plan docs/superpowers/plans/2026-10-08-inbox-status-drop-badges.md)` with:

```markdown
- [ ] **States with hooks (user setup):** run `mk integration install claude`, `… codex`,
  `… omp`, `… copilot`, then restart the agents. Claude: send a prompt → ◐ spins; it asks for a
  permission → `?` and an amber pane border; the turn ends → `✓ done` and an Inbox entry; wait →
  `○ idle`. Same in omp. Hover a status: the source says `hook`. Without the install, `done`
  never appears and omp/Copilot never show `?` (SPEC §6: done is hook-only).
- [ ] Collapse a workspace with a working agent: its `◐ N` count spins.
- [ ] **File drop** (T10.8 check above, still open): drag 2 files (one with a space) from
  Explorer onto a Claude pane and a pwsh pane → both paths typed, the spaced one quoted, no Enter.
  Mkrew must not run as administrator: Windows blocks Explorer drops into elevated windows.
- [ ] **Agent badges:** the sample profiles show ✻ claude, ⌬ codex, π omp, ⊙ copilot, >_ shell
  in the sidebar rows and pane headers (light + dark; check the glyphs render, not boxes). Your
  existing `%APPDATA%\mkrew\agents\*.toml` [INFERENCE: `dirs` config dir] are not updated: they
  show claude `C`, codex `C`, omp `O`, copilot `C` until you add `icon = "…"` to each; the change shows after the next
  refresh (reconnect or restart the UI). `icon = "claude"` → the profile error lists `file:line:col`.
```

- [ ] **Step 2: PROGRESS note**

Append to "Notes for the next session":

```markdown
- 2026-10-08 inbox 2 (plan docs/superpowers/plans/2026-10-08-inbox-status-drop-badges.md): status icons already existed — missing done/needs-input came from uninstalled hooks (user setup in HUMAN_CHECKS); workspace row count spins; file drop already built (T10.8); profile `icon` key + badges (ADR 0025). Outside the milestone loop.
```

Suggested message: `docs: manual checks and notes for the 2026-10-08 inbox (2)`

---

## Open questions (defaults used above)

| # | Question | Options | Default in this plan |
| --- | --- | --- | --- |
| 1 | Status icon style (item 18) | a) keep today's glyphs (◐ ? ✓ ○ ✕), fix only the workspace count · b) the mock's colored 8 px dots with a pulse (`design/sidebar.rs` `status_dot`) — label text must stay beside it (§11 "never color alone"), GPUI animation repaints every frame while an agent works · c) an SVG icon set (new asset pipeline) | **a** |
| 2 | Workspace row for done sessions | a) nothing (the Inbox already lists unviewed done) · b) `✓ N` for done sessions not yet viewed (needs the `Viewed` set in the sidebar model) | **a** |
| 3 | File drop extras (item 19) | a) none, the feature exists · b) drag-over highlight on the pane · c) b + drag a file from the Files view into a pane | **a** |
| 4 | Item 20 was cut off ("…if not yet then"). Meaning? | a) add a badge per agent (this plan) · b) something else — tell me | **a** |
| 5 | Badge source | a) TOML `icon` key + letter fallback · b) UI letters only (claude/copilot collide) · c) brand SVG logos | **a** |
| 6 | Where the badge shows | a) sidebar session row + pane header · b) a + Inbox row, board card, tab label | **a** |
| 7 | Sample glyphs | a) ✻ claude, ⌬ codex, π omp, ⊙ copilot, >_ shell, ƒ fake · b) letters (CC, CX, OP, GH) | **a** |
