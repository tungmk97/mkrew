# Split Large Files for Agent Context Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Shrink the files agents must open to edit Mkrew. `crates/mkrew-ui/src/main.rs` (20 690 lines), `crates/mkrew-daemon/tests/daemon.rs` (6 671 lines) and nine sources whose inline `mod tests` is 460–1 310 lines all get split with no behavior change.

**Architecture:** `impl Shell` in `main.rs` becomes 32 child modules under `crates/mkrew-ui/src/shell/`, one per UI area. Each module holds an `impl Shell { … }` block and/or that area's free widget fns. Each starts with `use crate::*;`, so it sees every crate-root import, type and private field without changing them (Rust privacy is module-tree based, and glob imports bring in private imports). Moved declarations get `pub(crate)` so sibling modules can call them. Free widget modules are re-exported at the root (`use shell::x::*;`). `struct Shell`, `main`, the small root types and `impl Render for Shell` stay in `main.rs` (about 650 lines). The daemon integration test becomes `tests/daemon/main.rs` (helpers) plus 13 group files, and the test binary is still named `daemon`. Inline test modules move to `<module>/tests.rs`, the pattern `mkrew-store/src/tests.rs` already uses. Every move is done by a Python script given in full in this plan, so bodies are byte-identical apart from `pub(crate)` and `include_*!` depth.

**Tech Stack:** Rust 2024 workspace, GPUI, `cargo fmt`/`clippy -D warnings`, insta, Python 3.12 (throwaway scripts only, never committed).

**Spec:** No feature spec. This is a structure-only refactor driven by the user request "find the large files that weigh on the agent's context in this app and split them without breaking any feature". SPEC §4 (crate layout) and §17 (conventions) still apply.

## Global Constraints

- No behavior change. No logic edits, renames, reordering inside bodies, or new features. The only allowed text changes in moved code are added `pub(crate)` and one extra `../` in `include_bytes!/include_str!` paths.
- `cargo fmt --all` clean; `cargo clippy --workspace --all-targets -- -D warnings` clean (AGENTS.md).
- Every existing test is kept. Only daemon integration tests gain a group prefix (`basics::…`). `cargo test --workspace -- --list` before and after must be identical once that prefix is stripped (Task 6).
- No `#[allow(unused_imports)]`, `#[path]`, or `pub` wider than `pub(crate)`.
- Conventional Commits, one commit per task, staging only that task's files (never `git add -A` / `.`). Never push, reset, checkout, stash or clean.
- Never commit the helper scripts. They live in `.superpowers/split/`, which has a `*` `.gitignore`.
- Windows: run Git-Bash scripts with `sh` (in the omp shell `bash` is WSL). Run `cargo`, `git` and `python` directly.
- Run this plan when no other plan is editing `crates/mkrew-ui/src/main.rs`. Open plans `2026-10-10-inbox-perf.md` and `2026-10-10-voice-input.md` cite `main.rs:<line>`. After this split, find code by symbol (`fn dispatch`), not by line.

## Review Focus

1. **Code added to `main.rs` after this plan was written** (the repo moves fast: `4de781c` landed while this plan was being drafted). Expected: the script stops with `not found in main.rs: [...]` for a renamed or removed name, and `--report` lists any new method or free item. The executor puts each one into the `MAP` group of its neighbours, and nothing silently stays behind. Pinned by Task 3 Step 3 (`--report` must print `nothing`) and by `check_moved.py` in every main.rs task.
2. **File-relative macros after a move** (`include_bytes!("../../../icon/...")` in `logo`, `include_str!("../../../agents/fake.toml")` and friends in the daemon tests). Expected: they still point at the same file. The scripts add one `../`. Pinned by compiling (rustc fails on a missing file) in Tasks 1, 4 and 5.
3. **A moved item that is subtly edited** (a hunk dropped, two methods swapped, a doc comment lost). Expected: identical code. Pinned by `check_moved.py BASE` in Tasks 1–3, which compares every one of the ~466 items, ignoring whitespace, `pub(crate)`, rustfmt trailing commas and include depth.
4. **Insta snapshot of `git_view` after its tests move.** insta looks in `snapshots/` next to the file that calls the macro. Expected: the existing snapshot is found, not re-recorded. Pinned by Task 5 Step 4 with `CI=true` (insta then fails instead of writing a new `.snap.new`).
5. **Test discovery.** Expected: no test lost. Only the daemon tests gain a group prefix (`basics::…`), and `cargo test --test daemon` still works. Pinned by the before/after `--list` diff in Task 6.

---

## Background the executor needs

- `main.rs` is the root of the `mkrew` binary (`[[bin]] path = "src/main.rs"` in `crates/mkrew-ui/Cargo.toml`). The library (`src/lib.rs`) has no `shell` module, so `src/shell.rs` + `src/shell/` belong to the binary only.
- **Why `use crate::*;` works:** a glob import brings in every name visible at the import site, including the parent's private `use` lines and private items. Child modules may read private fields of `Shell` declared at the root. This was verified on a scratch crate and by a full dry run of this plan on a copy of `4de781c`: all three batches, the test splits, `clippy -D warnings` clean, and `check_moved` reporting 466 items with 0 differences.
- **Why `pub(crate)`:** a method in `impl Shell` inside `shell/git.rs` is private to `shell::git` unless marked. Callers in other modules (and `cx.listener(Self::x)`) need it.
- **Why `--prune`:** some areas' widgets are used only inside their own file (`inspector`, `docs`, `git`), so the root re-export `use shell::x::*;` is unused and `-D warnings` fails. `--prune` deletes exactly those lines from rustc's output. Later batches only move code out of `main.rs`, so a pruned re-export is never needed again.
- **Name clashes:** shell submodules are named `board`, `history`, `peek`, `run`… like modules imported from `mkrew_ui` at the root. That is fine because no file does `use super::*;` (which would make `board::Column` ambiguous). Only `use crate::*;` is used, where `board` resolves to `mkrew_ui::board`.
- Files that stay big after this plan, on purpose: archived plans under `docs/superpowers/plans/` (read only when executing that plan), `docs/HUMAN_CHECKS.md` (the human's queue; moving items is the human's call), and `docs/SPEC.md` (agents open sections, per AGENTS.md).

## File Structure

| Path | Responsibility |
|---|---|
| `crates/mkrew-ui/src/main.rs` | imports, consts, `main`, root types (`Status`, `Chip`, `Renaming`, `FileMenu`…), `struct Shell`, `impl Render for Shell`, `mod shell;` + widget re-exports |
| `crates/mkrew-ui/src/shell.rs` | `//!` overview + `mod` list |
| `crates/mkrew-ui/src/shell/<area>.rs` ×32 | `impl Shell` methods and free widgets of one area (each file's `//!` line says which) |
| `crates/mkrew-daemon/tests/daemon/main.rs` | test harness: `Env`, `Daemon`, `Client`, scenario consts, every request helper |
| `crates/mkrew-daemon/tests/daemon/<group>.rs` ×13 | the `#[test]` fns of one area |
| `<crate>/src/<module>/tests.rs` ×9 | unit tests previously inline at the end of `<module>.rs` |
| `.superpowers/split/*.py` | throwaway move/check scripts (git-ignored) |

Areas (largest file ≈ 870 lines): widgets, cards, sidebar, menus, chrome, pane_render, inspector · files, files_render, docs, git, git_rows, git_render, git_diff, git_dialogs, comments, send_dialog · connection, run, workspace, navigation, focus, workspace_actions, dialogs, task_dialog, worktrees, history, board, composer, sessions, tabs, peek.

---

### Task 1: Move tooling + first batch (shared widgets, sidebar, menus, chrome, panes, inspector)

**Files:**
- Create (git-ignored): `.superpowers/split/.gitignore`, `.superpowers/split/split_shell.py`, `.superpowers/split/check_moved.py`, `.superpowers/split/split_daemon_tests.py`, `.superpowers/split/move_tests.py`
- Create: `crates/mkrew-ui/src/shell.rs`, `crates/mkrew-ui/src/shell/{widgets,cards,sidebar,menus,chrome,pane_render,inspector}.rs`
- Modify: `crates/mkrew-ui/src/main.rs`

**Interfaces:**
- Consumes: nothing.
- Produces: `python .superpowers/split/split_shell.py MODULE...` (moves the `MAP` groups, creates or extends `shell.rs`, adds `mod shell;` and `use shell::x::*;` to `main.rs`), `split_shell.py --prune`, `split_shell.py --report`, `check_moved.py BASE_REV`, `split_daemon_tests.py`, `move_tests.py FILE...`. Moved items become `pub(crate)`. Free widget fns stay callable unqualified everywhere through the root re-exports.

This is a pure move, so there is no new behavior test to write first. The red/green gate is `check_moved.py` (fails on any changed item) plus clippy and the existing suite.

- [ ] **Step 1: Record the base commit and the test list**

```bash
mkdir -p .superpowers/split && printf '*\n' > .superpowers/split/.gitignore
git rev-parse HEAD > .superpowers/split/BASE
cargo test --workspace -j 4 -- --list 2>/dev/null | grep -E ': (test|benchmark)$' | sort > .superpowers/split/tests-before.txt
wc -l .superpowers/split/tests-before.txt
```
Expected: a commit hash in `.superpowers/split/BASE`, and about 1 140 tests (the count at `4de781c`, higher if tests were added since).

- [ ] **Step 2: Write the scripts**

`.superpowers/split/.gitignore`:
```
*
```

`.superpowers/split/split_shell.py`:
```python
"""Move `impl Shell` methods and free items out of crates/mkrew-ui/src/main.rs.

Usage (repo root): python .superpowers/split/split_shell.py MODULE [MODULE ...]
Each MODULE becomes crates/mkrew-ui/src/shell/MODULE.rs. Bodies are copied verbatim; the only
edits are `pub(crate)` on moved declarations (and on fields of moved structs / fns of moved
inherent impls) so sibling modules can reach them. Run `cargo fmt --all` afterwards.
"""
import re
import sys
from pathlib import Path

SRC = Path("crates/mkrew-ui/src")
MAIN = SRC / "main.rs"
SHELL_RS = SRC / "shell.rs"
SHELL_DIR = SRC / "shell"

MAP = {
    "board": (
        'Board view actions: card drops, approve, run / run with / fan-out, follow-up.',
        """
        drop_card approve_task view_task_changes show_git_since move_cards run_task
        open_run_with run_task_with open_fan_out run_fan_out card_clicked open_follow_up
        follow_up_shown close_follow_up follow_up_key_down send_follow_up render_board
        """,
        '',
    ),
    "cards": (
        'Board widgets: columns, drop slots, task cards and their buttons, the drag preview.',
        """""",
        'CardContext|board_column|drop_slot|task_card|run_with_button|fan_out_button|fan_out_child_row|card_action|follow_up_field|CardPreview|impl Render for CardPreview',
    ),
    "chrome": (
        'Window chrome: the main area, title bar, view tabs, branch chip, workspace switcher.',
        """
        render_main render_title_bar render_view_tabs render_branch_chip
        render_workspace_switcher
        """,
        '',
    ),
    "comments": (
        'Review comments: drafts, line picks, threads, submit and status.',
        """
        comments_mut comment_context load_comments comment_draft pick_lines pick_diff_line
        start_comment start_reply close_comment_draft comment_key_down submit_comment
        set_comment_status add_comment_button send_comments_button render_comments
        comment_thread comment_field
        """,
        '',
    ),
    "composer": (
        'The composer (agent input) and the prompt picker.',
        """
        composer_session focused_agent open_composer composer_key_down composer_paste_image
        enhance_composer send_composer prompt_vars picked_lines open_prompt_picker
        close_prompt_picker prompt_picker_key_down insert_prompt_pick unqueue
        render_prompt_picker composer_panel
        """,
        '',
    ),
    "connection": (
        'Startup and the daemon link: `Shell::new`, refresh, loads, incoming-frame dispatch.',
        """
        new refresh agent_badge load_usage load_ports load_plans list_failed dispatch
        status_line
        """,
        '',
    ),
    "dialogs": (
        'Broadcast and undo dialogs.',
        """
        open_broadcast close_broadcast broadcast_key_down send_broadcast open_undo_dialog
        load_undo_preview open_undo close_undo_dialog undo_dialog_key_down confirm_undo
        render_undo_dialog render_broadcast
        """,
        '',
    ),
    "docs": (
        'Docs view: list, filter, read, preview and render.',
        """
        load_docs select_doc docs_filter_key_down read_doc doc_event open_doc_in_zed
        render_docs render_doc mode_toggle render_md_preview
        """,
        'doc_row',
    ),
    "files": (
        'Files view state: tree, read, find/replace, editor keys, save.',
        """
        files_view files_at set_files_target refresh_files files_requests list_dir read_file
        build_preview files_event click_file files_key_down open_file_find
        file_find_key_down edit_file_find step_file_find reveal_file_match close_file_find
        file_replace_key_down edit_file_replace replace_in_file replace_all_in_file
        save_file leave_file window_should_close file_editor_key_down click_file_text
        file_edited file_page_lines workspace_file target_file open_file_in_zed
        drop_gone_targets
        """,
        '',
    ),
    "files_render": (
        'Files view rendering: tree rows, the file pane and the editor.',
        """render_files render_file render_editor""",
        'margin_shown|FileRowState|file_row|size_label',
    ),
    "focus": (
        'Focus, modals and attaching terminal panes.',
        """
        modal_open close_menus focused_session sync_panes typing focus_pane attach
        modal_shade
        """,
        '',
    ),
    "git": (
        'Git view state: repos, status, log, diff, stage, commit, remotes.',
        """
        git_view git_at set_git_target set_git_repo review_diff retarget_git refresh_git
        load_git_repos git_summary load_git_summary git_requests load_git_log load_git_show
        select_git_commit show_git_section load_git_status load_session_files
        filter_git_changed_by load_git_diff git_event select_git_file git_key_down git_key
        open_git_file_in_zed stage_git git_commit_key_down git_commit_shown commit_git
        generate_commit_message run_git_remote review_in_docs
        """,
        'git_summary_of',
    ),
    "git_dialogs": (
        'Git discard and base dialogs.',
        """
        open_git_discard close_git_discard git_discard_key_down confirm_git_discard
        open_git_base close_git_base git_base_key_down set_git_base render_git_discard
        render_git_base
        """,
        '',
    ),
    "git_diff": (
        'Git diff rendering: unified and split rows, hunk buttons, line picks.',
        """render_git_diff""",
        'hunk_stage_button|git_diff_row|diff_code|DiffSide|split_half|git_split_row|pick_on_click',
    ),
    "git_render": (
        'Git view rendering: header, choosers, remote actions, commit box.',
        """
        render_git git_header target_chooser repo_chooser chip_row git_changed_by
        git_remote_action git_commit_box
        """,
        '',
    ),
    "git_rows": (
        'Git list rows: changed files and commits.',
        """git_list""",
        'change_counts|git_file_row|git_commit_row|row_action',
    ),
    "history": (
        'Follow mode, the turns dialog and session history.',
        """
        toggle_follow follow_wait open_turns close_turns_dialog turns_dialog_key_down
        session_state restore_before_turn diff_since_turn show_history open_history
        select_history load_history_log close_history history_key_down render_turns_dialog
        render_history
        """,
        '',
    ),
    "inspector": (
        'The session panel (inspector) and its cards.',
        """
        toggle_inspector end_inspector_drag inspector_toggle status_agent_fields
        render_inspector render_changes_card render_plan_card render_session_card
        open_transcript stop_session
        """,
        'InspectorGrip|inspector_card|plan_row',
    ),
    "menus": (
        'Workspace, file, tab and pane menus.',
        """
        render_workspace_menu render_file_menu render_tab_menu render_pane_menu
        with_workspace_menu click_workspace_row
        """,
        'agent_rows',
    ),
    "navigation": (
        'Quick open, links, file reveal and the command palette.',
        """
        open_quick_open close_quick_open open_file_ref_picker composer_typed drop_paths
        find_files quick_open_key_down open_quick_pick link_scope open_terminal_link
        open_file_at show_pending_line open_revealed reveal_file open_palette close_palette
        palette_key_down run_palette_pick render_quick_open render_palette
        """,
        '',
    ),
    "pane_render": (
        'Tab bar, tabs, rename field, pane frame and header.',
        """render_tab_bar render_rename render_tab pane_frame pane_placeholder""",
        'new_pane_button|starting_button|detach|drop_catcher|context_meter|pane_header',
    ),
    "peek": (
        'Sidebar inbox navigation and the peek popover.',
        """
        inbox_entries toggle_sidebar toggle_compact_sidebar jump_next_inbox peekable
        open_peek take_peek_focus close_peek read_peek update_peek hover_row hover_peek
        close_unused_peek peek_key_down answer_peek send_peek with_peek render_peek
        """,
        '',
    ),
    "run": (
        'Run button, run menu and run setup.',
        """
        run_summary run_main_click run_action show_run_tab reveal_run_tab open_run_setup
        close_run_setup run_setup_key_down save_run_setup render_run_button render_run_menu
        render_run_setup
        """,
        '',
    ),
    "send_dialog": (
        'Send comments / selection dialogs and the Changes dialog.',
        """
        open_send_dialog open_changes open_doc_dialog close_doc_dialog doc_dialog_key_down
        send_comments open_selection_dialog confirm_send_dialog send_selection
        doc_dialog_panel render_send_dialog render_changes
        """,
        '',
    ),
    "sessions": (
        'New panes, the default-agent prompt and the action dispatcher `Shell::run`.',
        """new_pane ask_for_default run""",
        '',
    ),
    "sidebar": (
        'Sidebar (full and compact) and the status bar.',
        """render_sidebar render_compact_sidebar render_status_bar""",
        'workspace_row|workspace_avatar|session_row|compact_workspace_row|compact_session_row',
    ),
    "tabs": (
        'Tabs and panes: switch, split, rename, pin, color, close, kill.',
        """
        cycle_workspace switch_tab new_tab split start_rename open_tab_menu
        start_pane_rename finish_rename rename_tab rename_session pin_session pin_tab
        set_pane_color open_pane_menu send_edit rename_key_down filter_key_down close_tab
        close_focused_pane close_pane remove_pane kill remove_session
        """,
        '',
    ),
    "task_dialog": (
        'The task dialog: create, edit, delete.',
        """
        open_task_dialog open_dialog close_task_dialog task_dialog_key_down save_task
        delete_task new_task_button render_task_dialog
        """,
        '',
    ),
    "widgets": (
        'Shared widgets: buttons, text fields, menus, status marks, tooltips.',
        """""",
        'message|empty_state|notice|button_frame|button|run_fill_button|run_icon_button|secondary_button|quiet_button|disabled_button|danger_button|kbd|inert_layer|window_control|icon_button|FieldEdit|impl FieldEdit|field_edit|typing_input|text_field|diff_row|ai_result|meta_line|menu_item|menu_panel|color_swatch|pin_mark|progress_bar|conflict_badge|status_mark|working_dots|logo|agent_chip|status_tooltip|tooltip|PopupStyle|impl PopupStyle|Tooltip|impl Render for Tooltip',
    ),
    "workspace": (
        'Workspaces and tabs: activate, load, place sessions, save layout, views.',
        """
        initial_workspace activate_workspace load_tabs create_tab tabs_loaded
        tabs_load_failed project_of project_of_reveal is_exited open_session
        notification_clicked place_session fill_empty_tab save_layout active_tabs_mut view
        show_view
        """,
        '',
    ),
    "workspace_actions": (
        'Workspace actions: add, menus, open in Zed/Explorer, settings, theme, archive.',
        """
        add_workspace pick_plan link_plan open_workspace_menu open_in_zed
        open_workspace_in_explorer open_file_menu open_path_in_explorer open_path_in_zed
        open_keymap_settings open_settings toggle_theme apply_diff_mode set_diff_mode
        open_prompts_dir archive_workspace set_default_agent set_default_shell
        set_workspace_color pin_workspace update_project
        """,
        '',
    ),
    "worktrees": (
        'Worktree dialog and the fan-out Compare dialog.',
        """
        open_worktree_dialog show_worktree_dialog open_compare close_compare
        compare_key_down compare_git compare_pick remove_other_worktrees
        load_worktree_dialog open_worktree close_worktree_dialog worktree_dialog_key_down
        confirm_worktree worktree_dialog_git render_worktree_dialog render_compare
        """,
        '',
    ),
}

ITEM = re.compile(r"^(?P<ind>(?:    )?)(?:pub(?:\([a-z]+\))? )?(?:async )?(?P<kind>fn|struct|enum|impl|type|const|static)\b")
LEAD = re.compile(r"^\s*(///|#\[|//)")


def item_name(line):
    s = line.strip()
    m = re.match(r"^impl(?:<[^>]*>)?\s+(\w+)(?:<[^>]*>)?(?:\s+for\s+(\w+))?", s)
    if m:
        return "impl " + m.group(1) + (" for " + m.group(2) if m.group(2) else "")
    m = re.match(r"^(?:pub(?:\([a-z]+\))? )?(?:async )?(?:fn|struct|enum|type|const|static)\s+(\w+)", s)
    return m.group(1)


def spans(lines, lo, hi, indent):
    """Items with exactly `indent` inside lines[lo:hi]: (name, start, end) half-open, doc/attrs
    included, trailing blank lines excluded."""
    found = []
    for i in range(lo, hi):
        m = ITEM.match(lines[i])
        if m and m.group("ind") == indent:
            s = i
            while s - 1 >= lo and LEAD.match(lines[s - 1]) and lines[s - 1].startswith(indent):
                if indent == "" and lines[s - 1].startswith(" "):
                    break
                s -= 1
            found.append([item_name(lines[i]), s, None])
    for k, it in enumerate(found):
        e = found[k + 1][1] if k + 1 < len(found) else hi
        while e > it[1] and not lines[e - 1].strip():
            e -= 1
        it[2] = e
    return found


def find_impl_shell(lines):
    start = lines.index("impl Shell {")
    end = next(i for i in range(start + 1, len(lines)) if lines[i] == "}")
    return start, end


def deepen(text):
    """`include_*!` paths are relative to the file: one directory deeper now."""
    return re.sub(r'include_(bytes|str)!\(\s*"\.\./', r'include_\1!("../../', text)


def publish_method(text):
    return re.sub(r"^    fn ", "    pub(crate) fn ", text, count=1, flags=re.M)


def publish_free(name, text):
    if name.startswith("impl ") and " for " in name:
        return text  # trait impl: items take the trait's visibility
    if name.startswith("impl "):
        return re.sub(r"^    fn ", "    pub(crate) fn ", text, flags=re.M)
    text = re.sub(r"^(async fn|fn|struct|enum|type|const|static) ", r"pub(crate) \1 ", text, count=1, flags=re.M)
    if name[0].isupper() and re.search(r"^pub\(crate\) struct \w+(<[^>]*>)? \{", text, re.M):
        text = re.sub(r"^    (?!pub)(\w+):", r"    pub(crate) \1:", text, flags=re.M)
    return text


def main(modules):
    unknown = [m for m in modules if m not in MAP]
    if unknown:
        sys.exit(f"unknown modules: {unknown}")
    lines = MAIN.read_text(encoding="utf-8").split("\n")
    s, e = find_impl_shell(lines)
    methods = {n: (a, b) for n, a, b in spans(lines, s + 1, e, "    ")}
    free = {n: (a, b) for n, a, b in spans(lines, 0, s, "") + spans(lines, e + 1, len(lines), "")}
    cut = []
    SHELL_DIR.mkdir(exist_ok=True)
    for mod in modules:
        doc, mnames, fnames = MAP[mod]
        mnames, fnames = mnames.split(), [x for x in fnames.split("|") if x]
        missing = [n for n in mnames if n not in methods] + [n for n in fnames if n not in free]
        if missing:
            sys.exit(f"{mod}: not found in main.rs: {missing}")
        target = SHELL_DIR / f"{mod}.rs"
        if target.exists():
            sys.exit(f"{target} already exists")
        parts = [f"//! {doc}", "", "use crate::*;", ""]
        if mnames:
            body = ["\n".join(lines[a:b]) for a, b in (methods[n] for n in mnames)]
            parts += ["impl Shell {", "\n\n".join(deepen(publish_method(t)) for t in body), "}", ""]
            cut += [methods[n] for n in mnames]
        for n in fnames:
            a, b = free[n]
            parts += [deepen(publish_free(n, "\n".join(lines[a:b]))), ""]
            cut.append((a, b))
        target.write_text("\n".join(parts), encoding="utf-8", newline="\n")
    drop = set()
    for a, b in cut:
        drop.update(range(a, b))
    kept = [l for i, l in enumerate(lines) if i not in drop]
    s, e = find_impl_shell(kept)
    if all(not l.strip() for l in kept[s + 1 : e]):  # every method moved: drop the empty block
        del kept[s : e + 1]
    if "mod shell;" not in kept:  # first run: declare the module right after the `use` block
        last_use = max(i for i, l in enumerate(kept[:s]) if re.match(r"^use .*;$", l) or l == "};")
        kept[last_use + 1 : last_use + 1] = ["", "mod shell;"]
    text = re.sub(r"\n{3,}", "\n\n", "\n".join(kept))
    decls = SHELL_RS.read_text(encoding="utf-8").split("\n") if SHELL_RS.exists() else [
        "//! `impl Shell` split by area; `struct Shell`, `main` and `Render` stay in `main.rs`.",
        "//! Free widget fns are `pub(crate)` and re-exported at the crate root (`use shell::x::*`),",
        "//! so every module reaches crate-root items and widgets through `use crate::*`.",
        "",
    ]
    for mod in modules:
        has_free = bool(MAP[mod][2])
        decls.append(f"{'pub(crate) ' if has_free else ''}mod {mod};")
        if has_free:
            text = text.replace("\nmod shell;\n", f"\nmod shell;\nuse shell::{mod}::*;\n", 1)
    SHELL_RS.write_text("\n".join(l for l in decls if l is not None).rstrip("\n") + "\n", encoding="utf-8", newline="\n")
    MAIN.write_text(text, encoding="utf-8", newline="\n")


def prune():
    """Drop crate-root `use shell::x::*;` re-exports rustc reports unused (x's widgets are only
    used inside x). Later batches only move code out of main.rs, so they never need it back."""
    import subprocess

    out = subprocess.run(
        ["cargo", "check", "-p", "mkrew-ui", "--all-targets", "--message-format=short"],
        capture_output=True, text=True,
    ).stderr
    unused = set(re.findall(r"unused import: `(shell::\w+::\*)`", out))
    text = MAIN.read_text(encoding="utf-8")
    for path in unused:
        text = text.replace(f"use {path};\n", "")
    MAIN.write_text(text, encoding="utf-8", newline="\n")
    print("pruned:", sorted(unused) or "nothing")


# Crate-root items that stay in main.rs on purpose.
KEEP = {
    "PAINT_TIMEOUT", "DRAIN_BATCH", "FIRST_TAB_TITLE", "CUSTOM_TITLE_BAR", "DRAWN_CONTROLS", "main",
    "Status", "AfterSave", "Chip", "RenameTarget", "Renaming", "FileMenu", "Shell",
    "impl Render for Shell",
}


def report():
    """List what is still in main.rs but in neither MAP nor KEEP (code added after the plan)."""
    lines = MAIN.read_text(encoding="utf-8").split("\n")
    left = []
    if "impl Shell {" in lines:
        s, e = find_impl_shell(lines)
        left += ["Shell::" + n for n, _, _ in spans(lines, s + 1, e, "    ")]
        top = spans(lines, 0, s, "") + spans(lines, e + 1, len(lines), "")
    else:
        top = spans(lines, 0, len(lines), "")
    left += [n for n, _, _ in top if n not in KEEP]
    print("left in main.rs:", left or "nothing")


if __name__ == "__main__":
    if sys.argv[1:] == ["--prune"]:
        prune()
    elif sys.argv[1:] == ["--report"]:
        report()
    else:
        main(sys.argv[1:])
```

`.superpowers/split/check_moved.py`:
```python
"""Prove the main.rs split moved code verbatim.

Usage (repo root): python .superpowers/split/check_moved.py BASE_REV
Compares every item of BASE_REV:crates/mkrew-ui/src/main.rs (methods of `impl Shell` and
crate-root items) with the same item in the working tree (main.rs + shell/*.rs), ignoring only
whitespace, `pub(crate)`, rustfmt trailing commas and the extra `../` of moved `include_*!` paths.
Prints every difference; exits 1 if any item is missing, duplicated or changed.
"""
import re
import subprocess
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))
from split_shell import SRC, spans  # noqa: E402


def norm(text):
    text = re.sub(r"\s+", "", text.replace("pub(crate) ", ""))
    text = re.sub(r'include_(bytes|str)!\("(\.\./)+', r'include_\1!("UP/', text)  # depth: rustc checks
    return re.sub(r",([)\]}>])", r"\1", text)


def items(text, impl_methods_too=True):
    lines = text.split("\n")
    out = {}
    if "impl Shell {" in lines:
        s = lines.index("impl Shell {")
        e = next(i for i in range(s + 1, len(lines)) if lines[i] == "}")
        for n, a, b in spans(lines, s + 1, e, "    "):
            out.setdefault("Shell::" + n, []).append(norm("\n".join(lines[a:b])))
        top = spans(lines, 0, s, "") + spans(lines, e + 1, len(lines), "")
    else:
        top = spans(lines, 0, len(lines), "")
    for n, a, b in top:
        out.setdefault(n, []).append(norm("\n".join(lines[a:b])))
    return out


def main(base):
    old = subprocess.run(
        ["git", "show", f"{base}:crates/mkrew-ui/src/main.rs"], capture_output=True, text=True,
        encoding="utf-8", check=True,
    ).stdout.replace("\r\n", "\n")
    before = items(old)
    after = {}
    for path in [SRC / "main.rs", *sorted((SRC / "shell").glob("*.rs"))]:
        for n, bodies in items(path.read_text(encoding="utf-8")).items():
            after.setdefault(n, []).extend(bodies)
    bad = 0
    for n in sorted(set(before) | set(after)):
        if before.get(n) != after.get(n):
            bad += 1
            print(f"DIFF {n}: before={len(before.get(n, []))} after={len(after.get(n, []))}")
    print(f"{len(before)} items checked, {bad} differ")
    sys.exit(1 if bad else 0)


if __name__ == "__main__":
    main(sys.argv[1])
```

`.superpowers/split/split_daemon_tests.py`:
```python
"""Split crates/mkrew-daemon/tests/daemon/main.rs: `#[test]` fns move into one file per group.

Usage (repo root, after `git mv crates/mkrew-daemon/tests/daemon.rs crates/mkrew-daemon/tests/daemon/main.rs`):
    python .superpowers/split/split_daemon_tests.py
Helpers (every non-test item) stay in main.rs; children reach them with `use super::*;`.
Test bodies are copied verbatim. Run `cargo fmt --all` afterwards.
"""
import re
import sys
from pathlib import Path

DIR = Path("crates/mkrew-daemon/tests/daemon")
MAIN = DIR / "main.rs"

GROUPS = {
    "basics": (
        'Daemon startup, settings, hello, projects, agent profiles, `session.wait` errors.',
        """
        starting_twice_never_yields_two_daemons
        settings_are_reread_and_errors_reported_in_status
        hello_is_required_and_version_checked projects_are_added_and_listed
        startup_marks_stale_sessions_exited agent_profiles_lists_samples_and_broken_files
        wait_times_out_and_rejects_bad_requests
        """,
    ),
    "comments": (
        'Doc comment threads and comments sent to agents.',
        """
        comment_threads_reply_resolve_reopen_and_agent_replies_address_them
        comments_sent_to_a_working_agent_are_typed_once_it_asks
        fake_agent_addresses_sent_comments_and_replies_to_each
        fake_agent_fixes_code_for_diff_comments_and_replies
        """,
    ),
    "states": (
        'Session states from the fake agent: hooks, screen, attachments, env, the file watcher.',
        """
        fake_agent_session_reaches_needs_input_and_is_killed
        attachments_are_saved_per_session_and_bad_ones_rejected
        hook_reports_drive_the_state_of_live_sessions_only
        screen_only_profiles_record_hooks_but_ignore_them
        fake_agents_go_through_every_state_transition
        session_env_cwd_and_exit_code_reach_the_store
        file_watcher_records_changed_files_of_live_sessions_except_ignored_ones
        """,
    ),
    "docs": (
        'Docs listing/reading/re-anchoring and edit conflicts.',
        """
        docs_are_listed_read_and_external_edits_reanchor_comments
        two_sessions_editing_one_file_raise_a_conflict
        """,
    ),
    "io": (
        'Attach, output streaming, paste back-pressure, send queue, `session.read`.',
        """
        daemon_stop_terminates_live_sessions
        attach_streams_output_and_reattach_restores_the_screen
        slow_attached_client_gets_a_snapshot_instead_of_stalling_the_session
        big_paste_to_a_child_that_does_not_read_leaves_the_daemon_responsive
        send_while_working_is_queued_until_the_agent_asks_and_wait_sees_it_finish
        a_queued_text_removed_while_working_is_never_typed
        exiting_with_queued_texts_announces_an_empty_queue
        session_read_shows_the_prompt_line_and_the_answer_goes_through
        """,
    ),
    "turns": (
        'Turns: edits, token usage, snapshots, restore, hook-attributed diffs.',
        """
        a_turn_that_edits_two_files_reports_both
        a_turn_end_reads_token_usage_from_the_agent_transcript
        session_usage_lists_events_stored_before_the_cache_split
        a_hook_turn_start_is_snapshotted_before_the_agent_edits
        turns_are_listed_previewed_and_restored
        hook_edits_are_announced_attributed_and_diffed_since_a_turn
        """,
    ),
    "tabs_tasks": (
        'Tabs/layout and tasks persisting across daemon restarts.',
        """
        tabs_and_layout_persist_across_daemon_restart
        tasks_crud_and_order_persist_across_daemon_restart
        """,
    ),
    "git": (
        'Git requests: base, status, diff, stage, commit, log, remotes, child repos.',
        """
        git_set_base_is_announced_persists_and_null_means_auto
        git_status_and_diff_follow_the_project_base_and_reject_bad_input
        git_stage_unstage_discard_and_commit_with_git_changed_events
        git_log_show_push_and_fetch_with_a_local_remote
        git_requests_reach_child_repositories_with_project_relative_paths
        """,
    ),
    "runs": (
        'Project run settings and run commands.',
        """
        project_run_settings_are_checked_trimmed_and_saved
        run_commands_start_in_their_folders_with_env_side_by_side_restart_and_stop
        invalid_run_setups_are_refused_before_anything_starts
        run_restarts_that_cannot_start_keep_the_running_command
        run_commands_auto_start_after_a_daemon_restart_and_end_with_their_command
        run_sessions_report_the_ports_they_print
        """,
    ),
    "task_runs": (
        'Spawn defaults, task runs, checkpoints, undo, resume after restart.',
        """
        spawn_defaults_to_the_project_agent_or_the_shell_and_fills_an_empty_tab
        task_run_delivers_the_prompt_per_mode_and_the_card_follows_the_session
        task_run_stores_head_as_checkpoint
        task_undo_previews_then_discards_changes_since_the_checkpoint
        restart_resumes_running_tasks_with_their_native_session_id
        restart_resumes_free_agent_panes_in_tabs_once
        an_exited_session_leaves_its_tab_but_a_daemon_stop_keeps_the_pane
        """,
    ),
    "worktrees": (
        'Worktree tasks, worktree-targeted fs/git, merge back, fan-out, npm shims.',
        """
        worktree_tasks_run_and_resume_in_their_own_worktree
        fs_and_git_requests_target_a_task_worktree
        worktree_tasks_merge_back_and_remove_their_worktree
        fanout_runs_each_agent_in_its_own_worktree
        task_run_passes_metacharacter_prompts_through_npm_cmd_shims
        """,
    ),
    "fs_ai": (
        '`fs.*` requests and `ai.generate`.',
        """
        fs_list_read_and_find_follow_file_changes
        ai_generate_answers_with_the_oneshot_output ai_generate_reports_what_is_missing
        """,
    ),
    "workspaces": (
        'Archive, remove, rename, pin, color, plans, terminal titles.',
        """
        archived_workspaces_stop_their_sessions_and_come_back_when_added_again
        archived_workspaces_are_not_resumed_until_added_again
        removed_sessions_stop_and_leave_the_list
        sessions_are_renamed_live_and_exited_but_run_sessions_refuse
        a_linked_plan_reports_its_checklist_until_it_is_unlinked
        sessions_are_pinned_live_and_exited_and_keep_the_pin_through_saves
        workspace_and_tab_pins_persist_across_daemon_restart
        sessions_are_colored_live_and_exited_and_the_name_is_checked
        agent_terminal_titles_name_the_session_once_per_change
        workspace_colors_are_set_cleared_and_checked
        """,
    ),
}


def deepen(text):
    """`include_*!` paths are relative to the file, which now sits one directory deeper."""
    return re.sub(r'include_(bytes|str)!\(\s*"\.\./', r'include_\1!("../../', text)


def test_spans(lines):
    """`#[test]` fns at column 0: name -> (start, end) half-open, attrs/docs included."""
    found = {}
    for i, line in enumerate(lines):
        m = re.match(r"^fn (\w+)", line)
        if not m:
            continue
        s = i
        while s - 1 >= 0 and re.match(r"^(///|#\[|//)", lines[s - 1]):
            s -= 1
        if not any(lines[k].startswith("#[test]") for k in range(s, i)):
            continue
        e = next(k for k in range(i, len(lines)) if lines[k] == "}") + 1
        found[m.group(1)] = (s, e)
    return found


def main():
    lines = deepen(MAIN.read_text(encoding="utf-8")).split("\n")
    spans = test_spans(lines)
    wanted = [n for _, names in GROUPS.values() for n in names.split()]
    missing = [n for n in wanted if n not in spans]
    unplaced = [n for n in spans if n not in wanted]
    if missing or unplaced or len(wanted) != len(set(wanted)):
        sys.exit(f"missing={missing} unplaced={unplaced}")
    drop = set()
    for group, (doc, names) in GROUPS.items():
        bodies = ["\n".join(lines[a:b]) for a, b in (spans[n] for n in names.split())]
        (DIR / f"{group}.rs").write_text(
            f"//! {doc}\n\nuse super::*;\n\n" + "\n\n".join(bodies) + "\n", encoding="utf-8", newline="\n"
        )
        for n in names.split():
            drop.update(range(*spans[n]))
    kept = [l for i, l in enumerate(lines) if i not in drop]
    last_use = max(i for i, l in enumerate(kept) if re.match(r"^use .*;$", l) or l == "};")
    kept[last_use + 1 : last_use + 1] = [""] + [f"mod {g};" for g in GROUPS]
    MAIN.write_text(re.sub(r"\n{3,}", "\n\n", "\n".join(kept)), encoding="utf-8", newline="\n")


if __name__ == "__main__":
    main()
```

`.superpowers/split/move_tests.py`:
```python
"""Move a trailing inline `#[cfg(test)] mod tests { ... }` into `<module dir>/tests.rs`.

Usage (repo root): python .superpowers/split/move_tests.py FILE.rs [FILE.rs ...]
`src/a/b.rs` gets `#[cfg(test)] mod tests;` and its body goes to `src/a/b/tests.rs` verbatim
(still indented; `cargo fmt --all` dedents code and never touches string literals). The module
path (`crate::a::b::tests`) and so every test name and insta snapshot name stay the same.
"""
import re
import sys
from pathlib import Path


def deepen(text):
    """`include_*!` paths are relative to the file, which now sits one directory deeper."""
    return re.sub(r'include_(bytes|str)!\(\s*"\.\./', r'include_\1!("../../', text)


def move(path):
    lines = path.read_text(encoding="utf-8").split("\n")
    starts = [i for i in range(len(lines) - 1) if lines[i] == "#[cfg(test)]" and re.match(r"^mod tests \{$", lines[i + 1])]
    if len(starts) != 1:
        sys.exit(f"{path}: expected one `#[cfg(test)]\\nmod tests {{`, found {len(starts)}")
    s = starts[0]
    end = max(i for i, l in enumerate(lines) if l.strip())
    if lines[end] != "}":
        sys.exit(f"{path}: `mod tests` is not the last item")
    target = path.with_suffix("") / "tests.rs"
    if target.exists():
        sys.exit(f"{target} already exists")
    target.parent.mkdir(exist_ok=True)
    target.write_text(deepen("\n".join(lines[s + 2 : end])) + "\n", encoding="utf-8", newline="\n")
    path.write_text("\n".join(lines[:s] + ["#[cfg(test)]", "mod tests;", ""]), encoding="utf-8", newline="\n")
    print(f"{path} -> {target}")


if __name__ == "__main__":
    for arg in sys.argv[1:]:
        move(Path(arg))
```

Run: `git check-ignore -q .superpowers/split/split_shell.py && echo ignored`
Expected: `ignored`

- [ ] **Step 3: Move the first batch**

```bash
python .superpowers/split/split_shell.py widgets cards sidebar menus chrome pane_render inspector
cargo fmt --all
python .superpowers/split/split_shell.py --prune
```
Expected: the script prints nothing, then `pruned: ['shell::inspector::*']`. If it exits with `not found in main.rs: [...]`, a name was renamed or removed upstream. Find its new name with `grep -n "fn <old>"` and fix that name in `MAP`. Do not change bodies.

- [ ] **Step 4: Verify verbatim move, lint and tests**

```bash
python .superpowers/split/check_moved.py $(cat .superpowers/split/BASE)
cargo clippy -p mkrew-ui --all-targets -- -D warnings
cargo test -p mkrew-ui -j 4
```
Expected: `N items checked, 0 differ`; clippy has no warnings; tests pass. If `check_moved` reports `DIFF X` and nothing touched X, X was changed by a commit after BASE (another session committed). Stop and re-run Steps 1–4 on a fresh BASE before continuing.

- [ ] **Step 5: Commit**

```bash
git add crates/mkrew-ui/src/main.rs crates/mkrew-ui/src/shell.rs crates/mkrew-ui/src/shell/
git commit -m "refactor(ui): move shared widgets, sidebar, menus, chrome and pane rendering out of main.rs"
```

---

### Task 2: Second batch (Files, Docs, Git, comments, send dialog)

**Files:**
- Create: `crates/mkrew-ui/src/shell/{files,files_render,docs,git,git_rows,git_render,git_diff,git_dialogs,comments,send_dialog}.rs`
- Modify: `crates/mkrew-ui/src/main.rs`, `crates/mkrew-ui/src/shell.rs`

**Interfaces:**
- Consumes: `.superpowers/split/split_shell.py`, `check_moved.py`, `.superpowers/split/BASE` from Task 1.
- Produces: the Files/Docs/Git methods (`Shell::load_git_status`, `Shell::render_git`, `Shell::render_file`…) as `pub(crate)` in their area files; `git_summary_of` (async) in `shell/git.rs`.

- [ ] **Step 1: Move the batch**

```bash
python .superpowers/split/split_shell.py files files_render docs git git_rows git_render git_diff git_dialogs comments send_dialog
cargo fmt --all
python .superpowers/split/split_shell.py --prune
```
Expected: `pruned: ['shell::docs::*', 'shell::git::*']`.

- [ ] **Step 2: Verify**

```bash
python .superpowers/split/check_moved.py $(cat .superpowers/split/BASE)
cargo clippy -p mkrew-ui --all-targets -- -D warnings
cargo test -p mkrew-ui -j 4
```
Expected: `0 differ`, no warnings, tests pass.

- [ ] **Step 3: Commit**

```bash
git add crates/mkrew-ui/src/main.rs crates/mkrew-ui/src/shell.rs crates/mkrew-ui/src/shell/
git commit -m "refactor(ui): move Files, Docs, Git and comment code out of main.rs"
```

---

### Task 3: Last batch; `impl Shell` leaves main.rs; agent map

**Files:**
- Create: `crates/mkrew-ui/src/shell/{connection,run,workspace,navigation,focus,workspace_actions,dialogs,task_dialog,worktrees,history,board,composer,sessions,tabs,peek}.rs`
- Modify: `crates/mkrew-ui/src/main.rs`, `crates/mkrew-ui/src/shell.rs`, `AGENTS.md` (Map table)

**Interfaces:**
- Consumes: Task 1 scripts and BASE.
- Produces: `main.rs` with no `impl Shell {` block. `Shell::new` lives in `shell/connection.rs`, the action dispatcher `Shell::run` in `shell/sessions.rs`, and frame dispatch `Shell::dispatch` in `shell/connection.rs`.

- [ ] **Step 1: Move the batch**

```bash
python .superpowers/split/split_shell.py connection run workspace navigation focus workspace_actions dialogs task_dialog worktrees history board composer sessions tabs peek
cargo fmt --all
python .superpowers/split/split_shell.py --prune
```
Expected: `pruned: nothing`.

- [ ] **Step 2: Verify**

```bash
python .superpowers/split/check_moved.py $(cat .superpowers/split/BASE)
cargo clippy -p mkrew-ui --all-targets -- -D warnings
cargo test -p mkrew-ui -j 4
```
Expected: `0 differ`, no warnings, tests pass.

- [ ] **Step 3: Check that nothing was left behind**

Run: `python .superpowers/split/split_shell.py --report`
Expected: `left in main.rs: nothing`. If it lists names (code added after this plan), open `main.rs`, cut each listed item and paste it unchanged into the area file of the code it sits next to. Methods go inside that file's `impl Shell { … }`; free items go at the end. Add `pub(crate)` to the declaration. Then re-run Steps 2–3.

- [ ] **Step 4: Check sizes**

Run: `wc -l crates/mkrew-ui/src/main.rs crates/mkrew-ui/src/shell/*.rs | sort -rn | awk 'NR<=4'`
Expected: `main.rs` about 650 lines (651 in the dry run at `4de781c`); no `shell/*.rs` over about 900 lines.

- [ ] **Step 5: Point agents at the new layout**

In `AGENTS.md`, `## Map` table, add this row right after the `` `crates/` `` row:

```markdown
|`crates/mkrew-ui/src/shell/`|the app window's `impl Shell`, one file per area (list in `shell.rs`); `main.rs` keeps `struct Shell` and `Render`|
```

- [ ] **Step 6: Commit**

```bash
git add crates/mkrew-ui/src/main.rs crates/mkrew-ui/src/shell.rs crates/mkrew-ui/src/shell/ AGENTS.md
git commit -m "refactor(ui): finish splitting impl Shell into src/shell/ by area"
```

---

### Task 4: Split the daemon integration test

**Files:**
- Move: `crates/mkrew-daemon/tests/daemon.rs` → `crates/mkrew-daemon/tests/daemon/main.rs`
- Create: `crates/mkrew-daemon/tests/daemon/{basics,comments,states,docs,io,turns,tabs_tasks,git,runs,task_runs,worktrees,fs_ai,workspaces}.rs`

**Interfaces:**
- Consumes: `.superpowers/split/split_daemon_tests.py`.
- Produces: cargo test target `daemon` (unchanged name; Cargo builds `tests/<name>/main.rs` as target `<name>`). The 72 test fns are now at `daemon::<group>::<test>`.

The test names gain a group prefix in `--list` (`basics::hello_is_required…`). That is the one expected difference, and Task 6 accounts for it.

- [ ] **Step 1: Move the file with history, then split**

```bash
mkdir -p crates/mkrew-daemon/tests/daemon
git mv crates/mkrew-daemon/tests/daemon.rs crates/mkrew-daemon/tests/daemon/main.rs
python .superpowers/split/split_daemon_tests.py
cargo fmt --all
```
Expected: no output from the script. If it exits with `missing=[...]` or `unplaced=[...]`, tests were added or renamed upstream. Put each unplaced test into the group whose neighbours it sat between in the old file, and fix renamed names, in `GROUPS`.

- [ ] **Step 2: Verify**

```bash
cargo clippy -p mkrew-daemon --all-targets -- -D warnings
grep -c '^#\[test\]' crates/mkrew-daemon/tests/daemon/main.rs
cargo test -p mkrew-daemon --test daemon -j 4
```
Expected: no warnings; `0` (only helpers remain in `main.rs`; the comment on line 3 does not start with `#[test]`); `test result: ok. 72 passed` (or however many `#[test]` fns BASE had). Timing-sensitive test, body unchanged by this move: under heavy parallel load `comments::comments_sent_to_a_working_agent_are_typed_once_it_asks` missed its 15 s `wait(Working)` once in the dry run. Re-run it alone with `cargo test -p mkrew-daemon --test daemon comments_sent_to_a_working`. It passed 3/3 alone in the dry run. Treat it as a real failure only if it also fails alone.

- [ ] **Step 3: Commit**

```bash
git add crates/mkrew-daemon/tests/daemon.rs crates/mkrew-daemon/tests/daemon/
git commit -m "test(daemon): split the daemon integration test into one file per area"
```

---

### Task 5: Move large inline unit tests into `<module>/tests.rs`

**Files:**
- Modify + Create `…/tests.rs` for: `crates/mkrew-ui/src/{git_view,files_view,sidebar,board,tabs,palette}.rs`, `crates/mkrew-protocol/src/message.rs`, `crates/mkrew-daemon/src/{git_view,worktree}.rs`
- Move: `crates/mkrew-ui/src/snapshots/mkrew_ui__git_view__tests__changed_by_keeps_only_the_sessions_files.snap` → `crates/mkrew-ui/src/git_view/snapshots/`

**Interfaces:**
- Consumes: `.superpowers/split/move_tests.py`.
- Produces: each file ends with `#[cfg(test)]\nmod tests;`. Module paths (`mkrew_ui::git_view::tests::…`) are unchanged, so test and snapshot names stay the same.

- [ ] **Step 1: Move the test modules**

```bash
python .superpowers/split/move_tests.py crates/mkrew-ui/src/git_view.rs crates/mkrew-protocol/src/message.rs crates/mkrew-ui/src/files_view.rs crates/mkrew-ui/src/sidebar.rs crates/mkrew-ui/src/board.rs crates/mkrew-daemon/src/git_view.rs crates/mkrew-ui/src/tabs.rs crates/mkrew-ui/src/palette.rs crates/mkrew-daemon/src/worktree.rs
```
Expected: nine `X.rs -> X/tests.rs` lines.

- [ ] **Step 2: Move the one affected insta snapshot**

```bash
mkdir -p crates/mkrew-ui/src/git_view/snapshots
git mv crates/mkrew-ui/src/snapshots/mkrew_ui__git_view__tests__changed_by_keeps_only_the_sessions_files.snap crates/mkrew-ui/src/git_view/snapshots/
```
Then, in the moved `.snap` file, change its header line `source: crates/mkrew-ui/src/git_view.rs` to `source: crates/mkrew-ui/src/git_view/tests.rs`, with the `edit` tool. Only metadata changes; the snapshot body stays.

- [ ] **Step 3: Format (dedents the moved code; string literals are untouched)**

```bash
cargo fmt --all
```

- [ ] **Step 4: Verify. The snapshot must be found, not re-recorded**

```bash
cargo clippy --workspace --all-targets -- -D warnings
CI=true cargo test -p mkrew-ui -j 4 git_view
CI=true cargo test -p mkrew-ui -p mkrew-protocol -p mkrew-daemon --lib -j 4
git status --short crates/mkrew-ui/src | grep -c 'snap.new' || true
```
Expected: no warnings; all pass; `0` `.snap.new` files.

- [ ] **Step 5: Commit**

```bash
git add crates/mkrew-ui/src/git_view.rs crates/mkrew-ui/src/git_view/ crates/mkrew-ui/src/snapshots/ crates/mkrew-ui/src/files_view.rs crates/mkrew-ui/src/files_view/ crates/mkrew-ui/src/sidebar.rs crates/mkrew-ui/src/sidebar/ crates/mkrew-ui/src/board.rs crates/mkrew-ui/src/board/ crates/mkrew-ui/src/tabs.rs crates/mkrew-ui/src/tabs/ crates/mkrew-ui/src/palette.rs crates/mkrew-ui/src/palette/ crates/mkrew-protocol/src/message.rs crates/mkrew-protocol/src/message/ crates/mkrew-daemon/src/git_view.rs crates/mkrew-daemon/src/git_view/ crates/mkrew-daemon/src/worktree.rs crates/mkrew-daemon/src/worktree/
git commit -m "test: move large inline test modules into sibling tests.rs files"
```

---

### Task 6: Whole-workspace proof and human smoke check

**Files:**
- Modify: `docs/HUMAN_CHECKS.md` (add one item under `## To verify (non-blocking)`)

**Interfaces:**
- Consumes: `.superpowers/split/tests-before.txt` from Task 1.
- Produces: nothing new.

- [ ] **Step 1: Test list unchanged (modulo the daemon group prefix)**

```bash
cargo test --workspace -j 4 -- --list 2>/dev/null | grep -E ': (test|benchmark)$' | sort > .superpowers/split/tests-after.txt
sed -E 's/^(basics|comments|states|docs|io|turns|tabs_tasks|git|runs|task_runs|worktrees|fs_ai|workspaces)::([a-z0-9_]+: test)$/\2/' .superpowers/split/tests-after.txt | sort > .superpowers/split/tests-after-norm.txt
diff .superpowers/split/tests-before.txt .superpowers/split/tests-after-norm.txt && echo IDENTICAL
```
Expected: `IDENTICAL`. Unit-test names like `git_view::tests::x: test` keep their `tests::` segment, so the prefix strip only touches daemon names. If a non-daemon line differs, a test was lost; find it in the diff and stop.

- [ ] **Step 2: Full verify**

Run: `sh scripts/verify.sh`
Expected: fmt check, clippy and all tests pass.

- [ ] **Step 3: Smoke the binary**

Run: `cargo build -p mkrew-ui --bin mkrew` and then, if a display is available, `cargo run -p mkrew-ui --bin mkrew`. Open Board, Docs, Files and Git, the palette (Ctrl+Shift+P), a terminal pane, the sidebar peek and one dialog, then close.
Expected: builds. If launched, every view renders and reacts as before. If no display is available, record that in Step 4 and do not claim a visual check.

- [ ] **Step 4: Ask the human for the GUI pass**

Add under `## To verify (non-blocking)` in `docs/HUMAN_CHECKS.md`:

```markdown
### To verify — 2026-10-10 large-file split (plan docs/superpowers/plans/2026-10-10-split-large-files.md)

Structure-only refactor (main.rs → `crates/mkrew-ui/src/shell/`, daemon tests and nine inline test modules split). Tests, clippy and a verbatim-move check passed. Please click through once: Board (drag a card), Docs (open, comment), Files (open, find, edit, save), Git (stage, diff split/unified, commit box), palette, quick open, peek, a run, the worktree/turns/history dialogs, theme toggle, compact sidebar. Expected: everything behaves exactly as before.
```

- [ ] **Step 5: Commit**

```bash
git add docs/HUMAN_CHECKS.md
git commit -m "chore(progress): ask for a GUI pass after the large-file split"
```
