# Progress archive

Done tasks moved out of PROGRESS.md to keep it short. Newest first.

## M13 — Isolation & usage (tag `m13-done`)

Acceptance criteria at completion (all automated parts ticked; the real-agent and design checks
wait on T13.11 in HUMAN_CHECKS "To verify" and the T13.3–T13.10b "Questions with assumed
defaults" entries):

- [x] Worktree task: Run creates the worktree and `mkrew/…` branch from HEAD, copies `.worktreeinclude` entries, runs the agent inside; the main folder's branch, HEAD, index and files are unchanged (real-git test). Resume after a daemon restart uses the worktree.
- [x] Git and Files views work on a task worktree; Merge only when both sides are clean and aborts cleanly on conflict; Remove keeps an unmerged branch (real-git tests).
- [x] Fan-out runs 2–5 agents in their own worktrees; Compare and Pick work; e2e with fake profiles.
- [x] Usage: fixture transcripts in the current Claude and Codex formats give the expected input/output/context tokens, read incrementally; shown on pane header, card and sidebar.
- [x] M9–M11 and M13 manual checks listed in HUMAN_CHECKS (T9.11, T10.10, T11.9, T13.11).

Tasks:

- [x] T13.1 Task worktree fields + dialog checkbox — feat(store): task worktree fields and dialog checkbox
- [x] T13.2 Git worktree primitives — feat(daemon): git worktree primitives
- [x] T13.3 Run and resume in the worktree — feat(daemon): run and resume worktree tasks in their worktree
- [x] T13.4a `fs.*` / `git.*` on a task worktree — feat(daemon): fs and git requests on a task worktree
- [x] T13.4b Worktree target chooser — feat(ui): worktree target chooser in Files and Git views
- [x] T13.5 `task.merge` / `task.remove_worktree` — feat(daemon): merge back and remove task worktrees
- [x] T13.6 Merge back / Remove worktree dialogs — feat(ui): merge back and remove worktree dialogs
- [x] T13.7 `task.fanout` — feat(daemon): fan a task out to several agents
- [x] T13.8a Run with several agents UI — feat(ui): run a task with several agents
- [x] T13.8b Compare and Pick — feat(ui): compare fan-out results and pick one
- [x] T13.9 Usage readers — feat(core): incremental token usage readers for Claude and Codex transcripts
- [x] T13.10a Daemon usage wiring — feat(daemon): read token usage from agent transcripts at turn ends
- [x] T13.10b Usage on panes, cards, workspaces — feat(ui): show token usage on panes, cards and workspaces
- [x] T13.11 M13 manual checks + wrap-up — chore(progress): complete M13

## M11 — Reviewing per turn (tag `m11-done`)

Acceptance criteria at completion (the unticked one waits on the T11.9 manual checks in
HUMAN_CHECKS "To verify" and the T11.1–T11.7 "Questions with assumed defaults" entries):

- [x] A turn snapshot is taken at each turn start without changing HEAD, branches, the index or any file (real-git test asserts all four); the `hook.report` reply for a turn-start hook waits for it.
- [x] `turn.list`, `turn.restore_preview`, `turn.restore`: preview lists changed/created/deleted files; restore only with `confirm` and a matching fingerprint; e2e with the fake agent over 2 turns.
- [x] Snapshot on the generated 10,000-file repo < 300 ms on Windows (perf test like §15).
- [x] Git view: "Changes since turn n" (`since` accepts a turn's tree) and the "Changed by" session filter.
- [ ] Follow in Zed: debouncer unit-tested; manual check with a real agent.

Tasks:

- [x] T11.1 Git turn snapshot primitive — feat(daemon): git turn snapshot primitive
- [x] T11.2 Turns migration + store API — feat(store): persist agent turns
- [x] T11.3 Snapshot and persist turns in the daemon — feat(daemon): snapshot and persist agent turns
- [x] T11.4 `turn.list` / `turn.restore_preview` / `turn.restore` — feat(daemon): list and restore agent turns
- [x] T11.5 `since` a tree, `session.files`, `session.edited` — feat(daemon): diff since a turn, session files and edit events
- [x] T11.6 Turns dialog + restore before a turn — feat(ui): turns dialog and restore before a turn
- [x] T11.7 Git view since a turn + Changed by — feat(ui): git view since a turn and changed-by filter
- [x] T11.8 Follow in Zed — feat(ui): follow an agent's edits in Zed
- [x] T11.9 M11 manual checks + wrap-up — chore(progress): complete M11

## M10 — Instructing (tag `m10-done`)

Acceptance criteria at completion (unticked ones wait on the T10.10 manual checks in
HUMAN_CHECKS "To verify" and the T10.2–T10.8 "Questions with assumed defaults" entries):

- [ ] Composer: a multi-line send arrives as one bracketed paste; `@` inserts `file_ref`-formatted paths; drafts survive switching panes.
- [x] Send queue: `session.send` returns `{send_id, queued}`, `session.queue` events, `session.unqueue`; e2e with the fake agent: a text queued while working and removed is never typed.
- [ ] Prompt library: samples seeded on first run, variables rendered (unit tests), insertable from the palette, the composer and the task dialog.
- [ ] Send selection from Files, Git diff and a terminal opens the right session's composer, prefilled.
- [ ] Dropping files types their paths; pasting an image stores it via `attachment.save` and types the path; attachments of sessions ended > 7 days ago are deleted.
- [ ] "Run with…" on a Todo/Review card updates the task's agent and runs it.

Tasks:

- [x] T10.1 Daemon send queue — feat(daemon): visible send queue with unqueue
- [x] T10.2 Prompt library — feat(core): prompt library with render and seeded samples
- [x] T10.3 Composer + text box — feat(ui): prompt composer docked on agent panes
- [x] T10.4 Composer `@` + queue list — feat(ui): composer @ file refs and send queue list
- [x] T10.5 Prompt picker + task text boxes — feat(ui): prompt picker and multi-line task/follow-up text boxes
- [x] T10.6 Send selection — feat(ui): send selection to an agent's composer
- [x] T10.7 `attachment.save` + cleanup — feat(daemon): attachment.save with 7-day cleanup
- [x] T10.8 Drop files / paste image — feat(ui): drop files and paste images into terminal panes
- [x] T10.9 Run with… — feat(ui): Run with… profile picker on board cards
- [x] T10.10 M10 manual checks + wrap-up — chore(progress): complete M10

## M9 — Attention (tag `m9-done`)

Acceptance criteria at completion (unticked ones wait on the T9.6–T9.10 and T9.11 manual checks
in HUMAN_CHECKS "To verify" / "Questions with assumed defaults"):

- [x] `settings.toml` (§20.0): defaults when missing, parse errors reported in `daemon.status` with the last good settings kept, re-read after the file changes; unit tests.
- [ ] `session.read` returns the last screen lines; the peek popover shows them for an Inbox or sidebar row, and its quick reply answers a fake agent's "(y/n)" without leaving the view (e2e for the request + manual check for the popover).
- [ ] Turns start and end as §20A.3 defines; a fake agent that edits 2 files in one turn produces `session.turn` with both paths (+/− counts arrive in M11); Inbox row and pane header show the summary.
- [ ] `notify.needs_input` / `notify.done` / `notify.sound` switches are respected (unit tests on the decision; manual check for the sound).
- [ ] Keep awake: inhibitor held while an agent works and released ≤ 5 s after the last one stops (fake inhibitor test); real backends per OS; manual check (`powercfg /requests`).
- [ ] Broadcast sends one text to every checked session; each result is shown.
- [ ] Inbox rows show the last non-blank screen line (§20A.4).

Tasks:

- [x] T9.1 Settings model — feat(core): settings.toml model and loader
- [x] T9.2 Daemon settings holder — feat(daemon): watch settings.toml and report errors in daemon.status
- [x] T9.3 `session.read` — feat(daemon): session.read returns the last screen lines
- [x] T9.4 Turn model + fake `edit_files` — feat(core): turn model and fake agent edit_files step
- [x] T9.5 Daemon turn tracking — feat(daemon): track turns and broadcast session.turn
- [x] T9.6 Notification switches + sound — feat(daemon): notification switches and sound from settings
- [x] T9.7 Keep awake — feat(daemon): keep the system awake while agents work
- [x] T9.8 Inbox last line + turn summary — feat(ui): Inbox last line and turn summary
- [x] T9.9 Peek popover — feat(ui): peek popover with quick reply
- [x] T9.10 Broadcast — feat(ui): broadcast to agents dialog
- [x] T9.11 M9 manual checks + wrap-up — chore(progress): complete M9

## M8 — Polish & packaging (tag `m8-done`)

Acceptance criteria at completion (unticked ones wait on the T8.3–T8.9 and T8.10 manual checks
in HUMAN_CHECKS "To verify"):

- [ ] Full command palette; keymap customization via `keymap.toml` with duplicate detection.
- [ ] Dev server per workspace: run `dev_command` in its own tab; optional auto-start after daemon start.
- [ ] Windows installer (MSI or similar) and tar.gz for Linux/macOS.
- [x] README: install, configure agents, uninstall.
- [ ] Non-functional checks from §15 measured and recorded in `docs/decisions/` or the README.

Tasks:

- [x] T8.1 Dev server config — feat(project): dev server command and auto-start settings
- [x] T8.2 Daemon dev server — feat(daemon): dev server start/stop and auto-start
- [x] T8.3 UI dev server — feat(ui): dev server settings, start/stop and indicator
- [x] T8.4 Command palette model — feat(ui): command palette model
- [x] T8.5 Command palette UI — feat(ui): command palette dialog and palette-only actions
- [x] T8.6 Keymap settings — feat(ui): keymap settings template and palette keymap errors
- [x] T8.7 Packaging — feat(release): MSI and tar.gz packaging scripts and release workflow
- [x] T8.8 README — docs: product README with install, agent setup and uninstall
- [x] T8.9 §15 measurements — test(ui): §15 measurement scripts and ADR 0009 results
- [x] T8.9a Idle UI CPU — test(ui): measure GPUI idle floor and judge idle CPU per machine
- [x] T8.10 M8 manual checks + wrap-up — chore(progress): complete M8

## M7 — Files & Git (tag `m7-done`)

Acceptance criteria at completion (unticked ones wait on the T7.7–T7.11b and T7.12 manual
checks in HUMAN_CHECKS "To verify"):

- [ ] Files view: lazy tree, respects `.gitignore`, git markers; read-only viewer with syntax
      highlighting; large/binary files handled.
- [ ] Quick open fuzzy-finds files in a 10,000-file repo in < 100 ms.
- [ ] Git view header: branch, upstream, ahead/behind; Working changes grouped Staged/Unstaged/Untracked.
- [ ] Against base uses `merge-base` correctly; base auto-detects `origin/HEAD` → `main` → `master`
      and is configurable per workspace (UI and `mk project update --base`).
- [ ] Unified diff; stage/unstage per file and per hunk; discard always confirms.
- [ ] Commit from the UI; push current branch to upstream (never force); fetch.
- [ ] History of 50 commits with per-commit diffs.
- [ ] "View changes" on a task card shows the diff since the task checkpoint.
- [ ] Diff-line comments sent to an agent; agent fixes and replies via `mk comment reply`
      (e2e test with the fake agent).
- [x] Integration tests run real `git` on temp repos (`tempfile`): status, diff, hunk staging,
      commit, merge-base.
- [x] `GitBackend` trait; status refresh meets §15 on a generated 10,000-file repo.

Tasks:

- [x] T7.1 Migration 0006 + files/git protocol + `git.set_base` — feat(protocol): files/git types, git.set_base and mk project update --base
- [x] T7.2 Core git parsers — feat(core): git status/diff parsers and hunk patches
- [x] T7.3 `git.status`/`git.diff` — feat(daemon): git.status and git.diff with base detection and caches
- [x] T7.4 Stage/unstage/discard/commit — feat(daemon): git stage/unstage/discard/commit and git.changed events
- [x] T7.5 Log/show/fetch/push + perf — feat(daemon): git.log/show/fetch/push and 10,000-file status perf test
- [x] T7.6 `fs.list/read/find` — feat(daemon): fs.list/read/find with git markers and a watched file index
- [x] T7.7 Files view — feat(ui): Files view with lazy git-marked tree and syntax-highlighted viewer
- [x] T7.8 Quick open — feat(ui): quick open fuzzy file finder
- [x] T7.9a Git view — feat(ui): Git view with grouped changes and unified diff viewer
- [x] T7.9b Stage/unstage/discard UI — feat(ui): Git stage/unstage per file and hunk, discard confirmation
- [x] T7.9c Base menu + Git keys — feat(ui): Git base menu and keymap commands for Git keys
- [x] T7.9d Diff highlighting — feat(ui): syntax highlighting in the Git diff viewer
- [x] T7.10a Commit box, Fetch/Push — feat(ui): Git commit box and Fetch/Push actions
- [x] T7.10b History — feat(ui): Git History section with per-commit files and diffs
- [x] T7.10c Task View changes — feat(git): task View changes since its checkpoint
- [x] T7.11a Diff comments backend — feat(comments): diff-line comment context and fix-the-code prompt
- [x] T7.11b Diff comments UI — feat(ui): diff-line comments in the Git view
- [x] T7.12 M7 manual checks + wrap-up — chore(progress): complete M7

## M6 — Markdown review (tag `m6-done`)

Acceptance criteria at completion (unticked ones wait on the T6.8, T6.9, T6.10 and T6.11 manual
checks in HUMAN_CHECKS "To verify"):

- [ ] Docs view lists files by `review_globs` + md files agents just changed; preview renders
      headings, lists, task lists, tables, code blocks correctly.
- [ ] Comment on a selection; threads with reply, resolve, reopen; persisted in SQLite.
- [ ] External edits (e.g. in Zed) refresh the preview and re-anchor comments; deleted passages → `outdated`.
- [ ] "Send to agent" types the prompt into the right session; queued while working.
- [ ] Agent runs `mk comment reply` → reply appears in the thread, comment becomes `addressed`.
- [ ] "View changes" diffs send-time snapshot vs current file.
- [x] E2E with the fake agent: plan written, 2 comments sent, both edited and answered.

Tasks:

- [x] T6.1 Comment store + protocol — feat(store): doc review comments in SQLite and protocol
- [x] T6.2 Daemon `comment.*` + `mk comment` — feat(daemon): comment threads over IPC and mk comment list/reply
- [x] T6.3 Core review modules — feat(core): review re-anchoring, agent prompt and review globs
- [x] T6.4 `doc.list`/`doc.read`/`doc.changed` — feat(daemon): doc list/read and re-anchoring on doc.changed
- [x] T6.5 `comment.send_to_agent` — feat(daemon): comment.send_to_agent with bracketed-paste delivery
- [x] T6.6 Fake agent review E2E — test(daemon): fake agent answers sent review comments end to end
- [x] T6.7 Markdown render model — feat(ui): Markdown render model with source line ranges
- [x] T6.8 Docs view — feat(ui): Docs view with Markdown preview, source view and live refresh
- [x] T6.9 Docs comments — feat(ui): Docs review comments with line picking and margin threads
- [x] T6.10 Send to agent / View changes — feat(ui): Docs send-to-agent picker and view-changes diff
- [x] T6.11 M6 manual checks + wrap-up — chore(progress): complete M6

## M5 — Resume, checkpoints, conflicts, history (tag `m5-done`)

Acceptance criteria at completion (unticked ones wait on the T5.4, T5.7, T5.9 and T5.11 manual
checks in HUMAN_CHECKS "To verify"):

- [ ] Stop and restart the daemon: running Claude/Codex tasks resume the right conversation.
- [ ] Before Run in a git repo: store HEAD in `task.checkpoint`; "Undo task" shows the diff to be
      discarded and only runs `git restore`/`git clean` after explicit confirmation.
- [ ] Two sessions in one project edit the same file within 60 s → `conflict.detected`, UI warns on both cards.
- [x] Watcher ignores `.git`, `node_modules`, `target` and respects `.gitignore`.
- [ ] Per-session output logs; history of finished sessions viewable after a daemon restart.
- [x] Daily DB backups, keep 7.

Tasks:

- [x] T5.1 Resume on daemon startup — feat(daemon): resume interrupted tasks on startup
- [x] T5.2 Git checkpoint on Run — feat(daemon): store git HEAD as task checkpoint on run
- [x] T5.3 Undo backend (ADR 0005) — feat(daemon): task undo preview and confirmed undo
- [x] T5.4 Undo dialog — feat(ui): undo task dialog with file list and diff
- [x] T5.5 Project file watcher — feat(daemon): project file watcher records changed files
- [x] T5.6 Conflict detection — feat(daemon): detect file conflicts between sessions
- [x] T5.7 Conflict badges — feat(ui): conflict warning badges on cards and pane headers
- [x] T5.8 Session history backend — feat(daemon): session history logs readable after restart
- [x] T5.9 History viewer — feat(ui): session history dialog replaying finished logs
- [x] T5.10 Daily DB backups — feat(daemon): daily database backups keeping the newest 7
- [x] T5.11 M5 manual checks + wrap-up — chore(progress): complete M5

## M4 — Task board (tag `m4-done`)

Acceptance criteria at completion (unticked ones wait on the T4.7–T4.10 manual checks in
HUMAN_CHECKS "To verify"):

- [ ] Create/edit/delete tasks; drag between columns; order persists.
- [ ] Run spawns the right agent (task's, else workspace default) in the right folder; prompt delivered per `prompt_mode`.
- [x] Task status follows session state (§5 transitions): needs_input, running, review on finish.
- [ ] Task card opens its session's terminal.
- [ ] Follow-up input on running cards sends via `session.send`.
- [x] `mk task add/list/run` share data with the UI.

Tasks:

- [x] T4.1 Task CRUD below the UI — feat(daemon): task CRUD + task.updated/task.deleted (ADR 0004)
- [x] T4.2 Pure task rules in core — feat(core): task status rules + drop positions
- [x] T4.3 `task.run` + prompt modes — feat(daemon): task.run with prompt modes; cards follow their session
- [x] T4.4 `.cmd` shim prompt args — feat(daemon): run npm .cmd shim targets directly so arg prompts keep metacharacters
- [x] T4.5 `mk task add/list/run` — feat(cli): mk task add/list/run
- [x] T4.6 Board view + switcher — feat(ui): board view with live task columns and a Board/Terminals switcher
- [x] T4.7 Task dialog — feat(ui): task dialog to create, edit and delete board tasks
- [x] T4.8 Drag & drop — feat(ui): drag cards between and within board columns
- [x] T4.9 Card actions — feat(ui): card actions — run, open terminal, follow-up, approve
- [x] T4.10 Shared-data UI test + M4 manual checks — test(ui): mk task changes reach the board; complete M4

## M3 — Agent state, inbox, notifications (tag `m3-done`)

Acceptance criteria at completion (unticked ones wait on the T3.4–T3.9 manual checks in
HUMAN_CHECKS "To verify"):

- [x] `mk integration install claude` and `codex` work: backup, idempotent, uninstall.
- [ ] Claude Code asks for approval → within 2 s the session is needs_input, Inbox has an entry, OS notification fires.
- [ ] Clicking the OS notification brings the app up focused on that pane.
- [ ] Per-session status correct on pane header, tab label and sidebar (§11); hover shows source.
- [ ] omp (screen detection) transitions correctly between working and idle.
- [x] Unit tests for the state machine; e2e tests with the fake agent for every transition.

Tasks:

- [x] T3.1 Detach panes while minimized — feat(ui): detach panes while the window is minimized
- [x] T3.2 `hook.report` + state source — feat(daemon): hook.report drives session state
- [x] T3.3 `mk hook` + fake agent hook step — feat(cli): mk hook + fake agent hook step
- [x] T3.4 `mk integration` — feat(cli): mk integration install/uninstall/status
- [x] T3.5 OS notifications — feat(daemon): OS notifications on needs_input/done
- [x] T3.6 UI per-session status — feat(ui): per-session status in pane headers, tabs and sidebar
- [x] T3.7 Inbox + notification click — feat(ui): sidebar Inbox and notification click focuses the pane
- [x] T3.7a Ctrl+Shift+I next Inbox entry — feat(ui): Ctrl+Shift+I jumps to the next Inbox entry
- [x] T3.8 State e2e + omp scenario — test(daemon): e2e every state transition + omp scenario
- [x] T3.9 M3 manual checks in HUMAN_CHECKS — chore(progress): complete M3

## M2 — UI: workspaces & terminals (tag `m2-done`)

Acceptance criteria at completion (unticked ones wait on the T2.4–T2.9 manual checks in
HUMAN_CHECKS "To verify"; minimized-window detach (§10) carried over to M3):

- [x] UI spawns the daemon if needed.
- [ ] Workspace sidebar; add workspace via folder picker.
- [ ] Many terminal tabs per workspace; split panes vertically/horizontally; agent or shell panes.
- [ ] Tabs and split layout persist across UI restarts and daemon restarts.
- [ ] Workspace/tab/pane/copy/paste/search shortcuts from §13 work; unbound keys reach the agent
      (verify Ctrl+C, Ctrl+R, Esc inside Claude Code).
- [x] Close and reopen UI: sessions still alive and re-attached.
- [ ] "Open in Zed" works.
- [ ] Design tokens in `theme.rs`; design check from §14 done.
- [ ] UI idle with 3 workspaces, 6 sessions: < 150 MB RAM.
- [ ] Terminal rendering follows the §10 performance rules; stress test (fake agent `flood` in
      6 panes) meets §15 throughput.
- [x] Default agent per workspace used for new agent panes.

Tasks:

- [x] T2.1 Daemon + store: tabs, `project.update`, spawn defaults — feat(daemon,store): terminal tabs, project.update, spawn defaults
- [x] T2.2 `mkrew-ui` crate, design tokens, IPC client, connect-or-spawn — feat(ui): mkrew-ui crate, design tokens, IPC client, connect-or-spawn
- [x] T2.3 Terminal pane fed from daemon attach — feat(ui): terminal pane fed from daemon attach
- [x] T2.4 Workspace sidebar + add-workspace picker — feat(ui): workspace sidebar with session status and add-workspace picker
- [x] T2.5a Split-tree and tabs models — feat(ui): split-tree and terminal tabs models
- [x] T2.5b Tabs + split panes, restore, visible-only attach — feat(ui): terminal tabs and split panes with restore and visible-only attach
- [x] T2.6 Pane lifecycle (agent/shell panes, kill-or-hide, running warning) — feat(ui): new agent/shell panes, close with kill-or-hide dialog, running warning
- [x] T2.7a §13 keymap — feat(ui): keymap.toml with §13 workspace, tab and pane shortcuts
- [x] T2.7b Selection, copy, scrollback — feat(ui): terminal selection, copy and scrollback paging
- [x] T2.7c Find in scrollback — feat(ui): find in terminal scrollback
- [x] T2.7d Pane resize, zoom, tab rename — feat(ui): pane resize, zoom and inline tab rename
- [x] T2.8 Open in Zed + default agent menu — feat(ui): workspace menu with Open in Zed and default agent
- [x] T2.9 Six-pane flood stress test — test(ui): six-pane flood stress test; perf(daemon): coalesce queued PTY output
- [x] T2.9a Flood pacing, byte-bounded UI inbox — perf(daemon,ui): pace flood output, byte-bound the UI inbox

## M1 — Daemon & CLI, no UI (tag `m1-done`)

Acceptance criteria at completion (unticked ones are in HUMAN_CHECKS "To verify": T1.10 real
terminal `mk attach`, T1.12 CI run green):

- [x] `mk daemon start` runs detached; `status`/`stop` work; starting twice never yields two daemons.
- [x] `mk project add/list` persist to SQLite.
- [ ] Sessions spawn from TOML profiles; `mk attach` is fully interactive; `Ctrl+]` detaches, agent keeps running.
- [x] Re-attaching restores the screen instantly from the daemon snapshot.
- [x] Binary output frames + backpressure (§7); PTY reader threads + headless emulator (§8).
- [x] Broken profile TOML → clear error with file and line; daemon does not crash.
- [x] `mkrew-fakeagent` + `agents/fake.toml` exist; `mk send` and `mk wait` work with it.
- [x] E2E test: real daemon in a temp dir, fake agent, `mk send` an answer, `mk wait` until done.
- [ ] CI workflow for Windows + Linux.

Tasks:

- [x] T1.1 Create `mkrew-protocol`: frame codec (u32 BE len + kind byte; 0x01 JSON, 0x02 = 16-byte ULID + raw bytes), Request/Response/Event enums for the M1 methods (hello, daemon.*, project.add/list, session.spawn/list/attach/detach/input/send/wait/resize/kill, agent.profiles), error codes incl. `version_mismatch`; round-trip + partial-read unit tests; covers: binary output frames. (spec: §4, §7) — feat(protocol): frame codec and M1 message types
- [x] T1.2 Create `mkrew-core` profiles: AgentProfile TOML parsing (prompt_mode, env, resume, [state]), directory loader returning profiles + per-file errors with file and line/column, sample `agents/{claude,codex,omp,shell,fake}.toml`; insta snapshots of parse errors; covers: broken profile TOML, `agents/fake.toml`. (spec: §6, §16) — feat(core): agent profile parsing and sample profiles
- [x] T1.3 Create `mkrew-store`: rusqlite bundled, WAL, numbered migrations runner + `0001_init.sql`, project add/list (dunce-normalized unique path) and session insert/update/list; tempdir tests; covers: project persist. (spec: §5, §17) — feat(store): SQLite store with migrations, projects and sessions
- [x] T1.4 `mkrew-core` session state machine (pure): starting/working/idle/needs_input/exited/unknown from output activity, `idle_after_ms`, needs_input patterns over emulated screen lines, process exit; hook-event input stubbed for M3; plus send-queue policy (deliver when idle/done/needs_input); unit tests with a fake clock; covers: groundwork for `mk send`/`mk wait`. (spec: §6, §8) — feat(core): session state machine and send queue
- [x] T1.5 Create `mkrew-daemon` skeleton: data/config dirs (`directories`, override via env for tests), single-instance lock file, `interprocess` local socket (named pipe current-user only / Unix socket), hello + protocol_version check, daemon.status/stop, project.add/list via store, startup marks stale sessions exited, tracing to rotating files; integration test: two daemons in one temp dir → second exits; covers: starting twice never yields two daemons. (spec: §3, §8, §17) — feat(daemon): daemon skeleton with lock, local socket and projects
- [x] T1.6 Create `mkrew-cli` (`mk`, clap): IPC client, `mk daemon start` (spawn detached, `CREATE_NO_WINDOW | DETACHED_PROCESS` on Windows, wait for hello) / `stop` / `status`, `mk project add/list`; integration test against a temp-dir daemon; covers: daemon start/status/stop, project add/list. (spec: §8, §9) — feat(cli): mk daemon start/stop/status and project add/list
- [x] T1.7 Create `mkrew-fakeagent`: scenario file (TOML) steps — print (colored), spinner, prompt "Do you want to proceed? (y/n)" and read a line, sleep, write file, exit code; `flood` + `ask` sample scenarios in `crates/mkrew-fakeagent/scenarios/`; point `agents/fake.toml` at it; unit tests on scenario parsing + a PTY-free run test; covers: `mkrew-fakeagent` exists. (spec: §16) — feat(fakeagent): scriptable fake agent with ask and flood scenarios
- [x] T1.8 Daemon sessions: `session.spawn/list/kill` from profiles (command lookup with PATHEXT + cmd-safe `.cmd` quoting, reuse `spike-term/src/launch.rs`), PTY env `MKREW_SESSION_ID`/`MKREW_SOCKET`/`TERM`, one reader OS thread → bounded channel → tokio, headless `alacritty_terminal` (200 lines scrollback, DSR replies written back), state machine wiring + `session.state`/`session.exited` events, session log file (5 MB rotation), shutdown kills PTYs ≤ 5 s; integration test spawning the fake agent; covers: sessions spawn from profiles, PTY reader threads + headless emulator. (spec: §6, §8) — feat(daemon): spawn, list and kill PTY sessions from agent profiles
- [x] T1.9 Daemon attach path: `session.attach` returns grid snapshot (cells + cursor + modes) and subscribes, binary 0x02 output frames batched ≤ 16 ms, per-client bounded queue (4 MB) with drop-and-resnapshot on overflow, `session.input/resize/detach`; tests incl. a slow-client overflow test; covers: binary output frames + backpressure, re-attach snapshot. (spec: §7, §8) — feat(daemon): attach with screen snapshots, batched output frames and backpressure
- [x] T1.9a Dedicated PTY writer OS thread per session (bounded channel; `session.input` awaits room on the connection side, never in the session task) so a big paste while the child's stdout is backed up cannot deadlock the session or block a tokio worker; DSR replies go through it too; test: a multi-MB `session.input` to a child that does not read stdin leaves the daemon and other sessions responsive. (spec: §8) — feat(daemon): dedicated PTY writer thread with bounded input queue
- [x] T1.10 `mk attach <id>` + `mk session list` (+ `mk session spawn` helper or `--agent` flag, record choice): raw mode (crossterm), paint snapshot then stream frames, forward stdin as `session.input`, resize on terminal resize, `Ctrl+]` detaches leaving the agent running; integration test detach → re-attach shows the same screen; covers: `mk attach` interactive, Ctrl+] detach, instant re-attach. (spec: §9, §7) — feat(cli): mk attach, session list and session spawn
- [x] T1.11 `session.send` (queued while working) + `session.wait` (until states, timeout) in the daemon, `mk send` / `mk wait`; E2E test: real daemon in a temp dir, fake agent `ask` scenario, `mk send y`, `mk wait --until idle,done` until finished; covers: `mk send`/`mk wait`, E2E test. (spec: §7, §9, §16) — feat(daemon,cli): session.send queue, session.wait, mk send and mk wait
- [x] T1.12 GitHub Actions CI (`windows-latest` + `ubuntu-latest`: fmt, clippy -D warnings, tests); add manual checks to HUMAN_CHECKS (`mk attach` with a real shell/`claude`, detached daemon has no console window, CI run green); covers: CI workflow. (spec: §17, §18) — ci: GitHub Actions verify on Windows and Linux

## M0 — Terminal-in-GPUI spike (tag `m0-done`)

Acceptance criteria at completion (unticked ones are in HUMAN_CHECKS "Waiting (blocking)" / T0.4):

- [ ] Runs `claude` (or `codex`) in the window; TUI renders with correct colors, no broken frames.
- [ ] Typing, arrows, Ctrl+C, multi-line paste work.
- [ ] Window resize redraws the TUI correctly.
- [ ] Works on native Windows.
- [ ] Automated: grid-dump snapshot test fed with recorded agent output.
- [x] Measured on Windows (in the ADR): idle RAM with one terminal, key-to-glyph latency, CPU during an output flood.
- [x] ADR `docs/decisions/0001-ui-framework.md` + HUMAN_CHECKS entries; status set to WAITING_HUMAN.

Tasks:

- [x] T0.9 Wrap-up: finalize ADR 0001 with recommendation (GPUI vs Tauri) and evidence; add manual checks (real agents, Windows, resize, paste) to HUMAN_CHECKS as "Waiting (blocking)"; run Step 8 of BUILD_LOOP. (spec: §18, §19) — docs(adr): recommend GPUI for the M0 gate
- [x] T0.8a Wake-on-output: replace the 16 ms poll in `view.rs` with a wake from the PTY reader thread, re-measure, update the ADR. (spec: §10, §15) — perf(spike): wake the UI on PTY output instead of polling
- [x] T0.8 Measurements: `--flood` mode + harness for idle RAM, key latency, flood CPU; numbers in the ADR. (spec: §15) — feat(spike): flood mode, --stats and measurement harness
- [x] T0.7 Launch options: `spike-term -- <command> [args]`; Windows `PATHEXT` lookup. (spec: §8, §17) — feat(spike): launch a command via PATH/PATHEXT lookup
- [x] T0.6 Input & resize: key → bytes mapping, bracketed paste, resize to PTY and grid. (spec: §10, §13) — feat(spike): keyboard input, paste and resize
- [x] T0.5 GPUI window: render the grid with tokens from a minimal `theme.rs`. (spec: §10, §14) — feat(spike): render terminal grid in a GPUI window
- [x] T0.4 Snapshot tests: ANSI fixtures + `insta` snapshots of the dumped grid. (spec: §16) — test(spike): grid-dump snapshot tests from ANSI fixtures
- [x] T0.3 Headless terminal core: `portable-pty` + `alacritty_terminal`, `Grid` snapshot, `--dump`. (spec: §8, §10) — feat(spike): headless terminal core with grid dump
- [x] T0.2 Research spike: gpui, gpui-component, portable-pty, alacritty_terminal → ADR 0001 Context. (spec: §4, §19) — docs(adr): draft ADR 0001 context from research spike
- [x] T0.1 Cargo workspace skeleton with `spike-term`; verify green. (spec: §4, §17) — chore: cargo workspace skeleton with spike-term
