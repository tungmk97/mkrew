# Inbox 2026-10-10: Perf — UI Re-renders, Daemon Watcher and SQLite, Opt-in Features, Terminal Path, Working Marks Implementation Plan

> **For agentic workers:** this plan is executed **incrementally, only the tasks the user ticked**. Read and follow `## Executor rules (incremental runs)` below before anything else; it overrides the per-task inbox-tick and ordering notes. REQUIRED SUB-SKILL: superpowers:subagent-driven-development (recommended) or superpowers:executing-plans, applied to the selected tasks only. Steps use checkbox (`- [ ]`) syntax for tracking.

> **Refreshed 2026-10-13 against HEAD `4cb293b`.** The 2026-10-10 version was never executed; since then `main.rs` was split into `crates/mkrew-ui/src/shell/*.rs`, the sidebar became a cached `SidebarView` (0606f04), terminal repaints are capped at 16 ms (95a8a6e), the Files editor caches its widest line (dd50ec3), the Markdown preview is virtual and built off-thread (42f3969), `[features] docs/turns` landed off by default (e0c245f, ADR 0046), the status bar was removed (ADR 0054), and phone access added a 1 s UI poll (ADR 0052/0053). Every task below was re-scanned against the current code: each starts with a `Refresh 2026-10-13` block saying what is still needed and what changed. Tasks 16–19 are new. **Nothing in this plan was compiled.** Execution waits for the user's review.

## Bảng quyết định

Plan này làm **dần dần**. Mỗi lần anh muốn làm thêm: tick ô ở cột **Quyết định** cho task anh chọn, rồi gọi executor (ví dụ: "chạy plan `docs/superpowers/plans/2026-10-10-inbox-perf.md`, chỉ những task đã tick"). Executor chỉ làm những dòng **đã tick một lựa chọn làm** và có **Trạng thái** = `chưa làm`. Xong task nào, nó ghi commit vào cột Trạng thái, nên lần chạy sau tự bỏ qua.

- **Ô chưa tick = chưa quyết, không làm.** Cột Đề xuất chỉ để anh tham khảo.
- Tick `Bỏ` / `Giữ` = không làm; executor ghi `bỏ` vào Trạng thái.
- Muốn làm lại một task đã `bỏ`: đổi Trạng thái về `chưa làm` và tick lại.
- Cột **Phụ thuộc** ghi task phải xong trước. Nếu anh tick một task mà task nó phụ thuộc chưa xong và chưa được tick, executor bỏ qua task đó và báo lại.

Sắp theo thứ tự em đề xuất làm (lợi nhất trước); executor cũng chạy theo thứ tự này. Mức ảnh hưởng ước lượng từ đọc code, chưa đo; "vẽ lại cả app" = GPUI dựng lại toàn bộ cửa sổ (title bar, sidebar, tabs, pane header…), là nguồn giật chính khi agent chạy. **Cỡ**: S < 50 dòng, M 50-200, L > 200 (tính cả test).

| Task | ID | Feature / chỗ tốn | Hiện tại ảnh hưởng app thế nào | Làm (sửa / xoá) thì được gì | Làm thì mất gì / rủi ro | Cỡ | Phụ thuộc | Đề xuất | Quyết định | Trạng thái |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 16 | A1+N1 | Chấm "working" động: chấm nhấp nháy 12 fps (tab, hàng workspace) + 3 chấm chạy 500 ms | **Nặng nhất.** Có 1 agent `working` đang hiện là cả app vẽ lại 12-14 lần/giây, suốt lúc agent chạy (gần như cả ngày). Gõ phím, cuộn, output terminal phải chen với các lần vẽ này | Hết vẽ lại theo animation; chỉ vẽ lại khi chữ thời gian đổi (1 lần/giây trong phút đầu, sau đó 1 lần/phút) | Không còn chuyển động: "working" kém nổi hơn, nhận biết bằng màu chấm + chữ `working · 12s`. Chấm màu tĩnh không tốn gì mỗi frame. Thay ADR/SPEC giao diện (ADR 0058) | M | — | **Làm** | [x] Làm - nhưng làm theo kiểu thay vì nhấp nháy thì mỗi status nên có chấm màu riêng để dễ nhìn - nếu không ảnh hưởng performance · [ ] Chỉ bỏ pulse · [ ] Giữ | chưa làm |
| 1 | B1 | Event từ daemon (usage, turn, plan, ports, board, comment, conflict) | Mỗi event của **bất kỳ** session nào đều vẽ lại cả app, kể cả khi inspector đóng hoặc session ở workspace khác. 2-3 agent chạy = vài chục lần vẽ thừa mỗi phút | Chỉ vẽ lại khi dữ liệu đó đang hiện trên màn hình; dữ liệu vẫn cập nhật ngầm, mở lên là thấy số mới | Không mất tính năng. Rủi ro: sót một chỗ thì panel đó hiện số cũ đến lần vẽ kế tiếp (có test cho bảng quyết định vẽ lại) | L | — | **Làm** | [x] Làm · [ ] Bỏ | chưa làm |
| 2 | B2 | Sidebar, số "N waiting", tìm session cho tab | Mỗi lần vẽ lại cả app (kể cả 12-14 lần/giây ở Task 16) sidebar lại sort, clone, format toàn bộ session; đếm waiting bằng cách clone cả danh sách | Sidebar chỉ tính lại khi dữ liệu đổi; mỗi lần vẽ rẻ hơn hẳn | Không mất tính năng. Rủi ro: quên đánh dấu "dirty" ở một chỗ thì sidebar hiện cũ (có test cho từng loại thay đổi) | L | — | **Làm** | [ ] Làm · [ ] Bỏ | chưa làm |
| 17 | N2 | Phone access: vòng poll trạng thái | Cứ 5 s vẽ lại cả app suốt đời app, **kể cả khi tắt `[web]`** và không có gì đổi. Khi mở dialog Phone: mỗi giây chạy 2 lệnh `tailscale` | Hết vẽ lại thừa; dialog chỉ gọi `tailscale` 3 s/lần | Trạng thái trong dialog Phone cập nhật chậm hơn, tối đa 3 s | S | — | **Làm** | [ ] Làm · [ ] Bỏ | chưa làm |
| 7 | A4+B7 | SQLite: lưu JSON thô của hook, `synchronous=FULL`, backup giữ khoá | Mỗi hook = 1 lần ghi + fsync đĩa; bảng `session_event` phình mãi (không ai đọc phần hook); lúc backup hằng ngày mọi request daemon phải chờ | Daemon ghi ít, không fsync mỗi commit; DB không phình (xoá event > 7 ngày, giữ `usage`); backup không chặn app | Mất JSON hook thô (không có code nào dùng). Mất điện / crash OS có thể mất vài giây dữ liệu cuối, DB không hỏng | M | — | **Làm** | [ ] Làm · [ ] Bỏ | chưa làm |
| 8 | B5 | Watcher file → `git.changed` | Agent sửa file / `cargo build` → mỗi ≤ 2 s daemon gửi `git.changed` → UI chạy `git status` + vẽ lại. Watcher của project đã từng mở Git/Docs không bao giờ dừng | Ít `git status` và ít vẽ lại khi agent chạy; watcher không dùng thì được thả | Files/Git cập nhật chậm hơn, tối đa 5 s khi có agent `working`. Bump protocol 41: UI và daemon phải cùng bản | L | — | **Làm** | [ ] Làm · [ ] Bỏ | chưa làm |
| 9 | B6 | Ghi `files_changed` vào SQLite | Mỗi đợt file đổi: 1 lần ghi DB cho mỗi session đang sống + đọc toàn bảng session | Chỉ ghi khi có file `.md` (Docs là nơi duy nhất đọc), không đọc DB mỗi đợt | Không mất gì thấy được: "changed by agent" trong Docs vẫn đúng | M | 8 | **Làm** | [ ] Làm · [ ] Bỏ | chưa làm |
| 12 | A3 | Phát hiện conflict (2 agent sửa cùng file) | Mỗi đợt file đổi: đọc DB + ghi DB; UI dọn danh sách conflict ở **mọi** event. Luôn chạy, không có cách tắt | Config: mặc định tắt, tắt thì không tốn gì; bật lại được. Xoá hẳn: bớt ~600 dòng code | Mất cảnh báo khi 2 agent sửa cùng file trong cùng thư mục (hiếm nếu dùng worktree). Config thì bật lại được, xoá thì không | M | — | **Config** | [ ] Config · [ ] Xoá hẳn · [ ] Giữ | chưa làm |
| 10 | A2 | Turn snapshots (đã tắt mặc định) | Khi `turns` tắt vẫn còn 1 chỗ: hook bắt đầu turn chờ hàng đợi snapshot → agent có thể bị chặn một chút đầu mỗi turn | Hook trả lời ngay khi `turns` tắt | Không mất gì | S | — | **Làm** | [ ] Làm · [ ] Bỏ | chưa làm |
| 13 | B9 | Daemon quét màn hình để đoán trạng thái agent | Mỗi đợt output (tới 60 lần/giây/session, cả session chạy nền): hash cả màn hình + tạo chuỗi cho mọi dòng + chạy regex, dù chỉ cần 4 dòng cuối | Daemon bớt CPU rõ khi agent in nhiều output | Trạng thái `working` / `needs_input` hiện chậm hơn, tối đa 250 ms | L | — | **Làm** | [ ] Làm · [ ] Bỏ | chưa làm |
| 14 | B10 | Terminal UI: scrollback 10.000 dòng/pane, grid cấp phát mới mỗi frame | RAM tăng theo số pane; resize/kéo chia pane phải reflow 10.000 dòng; mỗi frame có output cấp phát lại toàn bộ lưới ô | Ít RAM, kéo resize mượt hơn, bớt cấp phát mỗi frame | Cuộn ngược trong app được 5.000 dòng thay vì 10.000 (log trên đĩa vẫn đủ, `mk read` vẫn đọc được). Chạm code vẽ terminal | L | — | **Làm** | [ ] Làm · [ ] Giữ 10.000 · [ ] Bỏ | chưa làm |
| 15 | B12 | Mở lại session đã thoát (History) | Replay tới 2 MiB log vào terminal mới → mở History của session dài bị khựng | Mở nhanh hơn (1 MiB, cắt đúng ranh giới dòng/escape) | Thấy ít lịch sử hơn khi mở lại (đầu log cũ không replay; file log vẫn còn) | M | — | **Làm** | [ ] Làm · [ ] Bỏ | chưa làm |
| 3 | B3 | Docs tự đọc lại file đổi | Khi bật `[features] docs`: agent ghi file plan/spec là UI đọc + parse markdown + vẽ mermaid, kể cả khi Docs đang ẩn. Mặc định `docs` tắt nên đa số không bị | Docs ẩn thì chỉ đánh dấu cũ, mở ra mới đọc | Lúc mở lại Docs có thể thấy "Loading…" ngắn | M | — | Làm | [ ] Làm · [ ] Bỏ | chưa làm |
| 6 | B8 | Poll `settings.toml` | Đọc thông tin file trên UI thread 2 lần/giây suốt đời app (rẻ, nhưng là một timer không bao giờ ngừng) | Bỏ timer; kiểm tra settings khi cửa sổ Mkrew được focus lại | Sửa `settings.toml` bằng Zed thì phải bấm về cửa sổ Mkrew mới thấy áp dụng (trước đây ≤ 0,5 s) | M | 3 | **Làm** | [ ] Làm · [ ] Bỏ | chưa làm |
| 5 | B4 | Git summary ở title bar (branch, số file đổi) | Mỗi `git.changed` của workspace đang hiện chạy `git status` cho từng repo, bất kể đang ở view nào | Tối đa 1 lần / 2 s, luôn có lần cuối để số đúng | Badge Git có thể trễ tối đa 2 s | M | — | **Làm** | [ ] Làm · [ ] Bỏ | chưa làm |
| 18 | A5 | Sơ đồ Mermaid trong preview Markdown | Mỗi lần parse doc có sơ đồ: vẽ mỗi sơ đồ 2 lần (sáng + tối), không cache. Chạy nền, chỉ khi mở doc có mermaid | Vẽ 1 lần, cache theo nội dung; đổi theme mới vẽ lại | Đổi theme có khựng ngắn ở doc có sơ đồ. Đổi cấu trúc `Diagram`. Hôm 10/10 anh tick "Giữ" | L | — | Tuỳ anh, em nghiêng về **Bỏ** | [ ] Làm · [ ] Bỏ | chưa làm |
| 19 | N5 | Git History lane graph | Mỗi lần vẽ Git History quét tối đa 50 commit để tính độ rộng lane: gần như không tốn | Rẻ hơn chút | Không mất gì, cũng gần như không được gì | S | — | **Bỏ** | [ ] Làm · [ ] Bỏ | chưa làm |
| 4 | A6 | Live preview `.md` trong Files | Đã sửa ở 42f3969: dựng ở luồng nền, mỗi lúc 1 bản, không ảnh hưởng UI nữa | Thêm debounce không được gì | Chữ "Rendering…" hiện lâu hơn | — | — | **Bỏ** (đã có hiệu quả tương đương) | — | bỏ (không có bước) |
| 11 | A2 | UI ẩn tính năng turn khi tắt | Đã xong ở e0c245f: tắt `turns` thì không tốn gì phía UI | — | — | — | — | — | — | xong e0c245f |

Nếu chỉ chọn một nhóm: **16 + 1 + 2 + 17**. Nhóm này bỏ gần hết số lần vẽ lại cả app khi agent đang chạy (nguồn giật chính). Phần còn lại chủ yếu giảm tải phía daemon.

## Executor rules (incremental runs)

These rules override anything in a task section about ordering or ticking the inbox.

1. **Select.** A row of the decision table is selected when its `Quyết định` cell has a ticked "do" option (`[x] Làm`, `[x] Config`, `[x] Chỉ bỏ pulse`, `[x] Giữ 10.000`) **and** its `Trạng thái` is `chưa làm`. Nothing else runs: an unticked row is undecided, whatever `Đề xuất` says. A row ticked `Bỏ` / `Giữ` (12: `Giữ`; 16: `Giữ`) gets `Trạng thái` = `bỏ` and no work. Never change the user's ticks.
2. **Order.** Run the selected rows top to bottom in the table's order. If none is selected, report that and stop.
3. **Dependencies.** Before a row, check its `Phụ thuộc` column. If a dependency is neither `xong …` nor selected in this run (selected rows run earlier because the table order respects dependencies), skip the row, leave it `chưa làm`, and report "Task N skipped: needs Task M". Soft links have written fallbacks and are not dependencies:
   - Task 12 puts its `features` guard in `Shell::reload_settings_if_changed` if Task 6 is done, otherwise in the settings reload in `shell/connection.rs`.
   - Task 18 does the same with `redraw_diagrams`.
   - Task 1's `conflicts_need_redraw` works whether or not Task 12 is done.
   - Tasks 13, 14 and 15 share ADR 0057: the first of them to run creates it, the later ones append.
   - The perf HUMAN_CHECKS section: the first task to run creates it (see Global Constraints), the later ones append.
4. **Before each task.** Re-read the task's `Current state (investigated)` against the code; earlier tasks or other agents may have moved things. Re-check ADR, protocol and migration numbers (list `docs/decisions/`, read `PROTOCOL_VERSION`). Adapt names and lines, never the design. If the design no longer fits, stop that task and report why.
5. **Options.**
   - Task 16 `Chỉ bỏ pulse`: remove only the pulse (`status_dot`'s `pulse` / `id` params, `with_animation`, `PULSE_*`, the two callers). Keep the stepping dots and `status::redraw_due` unchanged, add no ADR 0058, and only edit SPEC/HUMAN_CHECKS lines that mention the pulse. Commit: `perf(ui): drop the working pulse animation`.
   - Task 14 `Giữ 10.000`: do the grid-buffer part only. Leave `SCROLLBACK_LINES`, its tests, `stress.rs` `WARM_LINES` and the scrollback SPEC lines unchanged.
   - Task 12 `Xoá hẳn`: this plan has no removal steps. Do not improvise; report "Task 12 needs a removal plan (ADR + SPEC)" and leave it `chưa làm`.
6. **After each task.** Verify and commit as the task says (Global Constraints → Commits). Then set the row's `Trạng thái` to `xong <short hash>` in this plan; the plan file itself is not staged. On a failure you cannot fix, set it to `lỗi: <one line>` and stop the run.
7. **Inbox.** Ignore a task's own "tick inbox line …" step. After a task, tick an inbox item `[~]` → `[x]` in `docs/INBOX.md` (never staged) when every task mapped to it (Inbox mapping below) is `xong …` or `bỏ`. Tasks 16–19 have no inbox line; leave INBOX alone for them.
8. **CPU check.** Task 1's HUMAN_CHECKS bullet asks for CPU before and after. If Task 1 is selected, record the HEAD commit before it in the report so the user can build "before".
9. **Report at the end.** List the rows run (commit), skipped (why) and set to `bỏ`, plus the HUMAN_CHECKS bullets added.

**Goal:** Handle the perf items in `docs/INBOX.md` (items 11–14) plus the findings of the 2026-10-13 rescan:
- The UI stops re-rendering the whole window for data nobody sees, for animations of working agents, and for unchanged phone status; it stops recomputing the sidebar and the git summary on every frame or event.
- The daemon stops sending too-frequent `git.changed`, releases watchers nobody uses, and writes less to SQLite (less fsync, nothing that is never read, pruning).
- Conflict detection becomes opt-in like turn snapshots already are (`[features]`, off by default).
- The terminal path does less work: a rate-limited screen scan, a smaller scrollback, a reusable grid buffer, and a shorter log replay.

**Architecture:**
- Item 11 (Tasks 1–6, UI): a pure `redraw` module (`Shown`, `event_needs_redraw`, `conflicts_need_redraw`) decides from what the window draws whether a changed model needs a root render; models always update. `Sidebar` caches its view, the waiting count and an id index behind a revision counter plus `Viewed::version`. `Docs` marks a hidden doc stale (`take_stale`, `Shell::show_docs`). `git_summary::Throttle` allows one summary refresh per 2 s with a trailing refresh. `Shell::reload_settings_if_changed` runs on window activation instead of the 500 ms tick (ADR 0055 amends ADR 0024).
- Item 12 (Tasks 7–9, daemon + store): no `hook` rows; `synchronous=NORMAL`; backup on its own connection; `hook` / `files_changed` / `edited` rows older than 7 days pruned (`usage` stays). `git.changed` only for batches with real changes; a batch waits up to 5 s while a session works (`Hub.working`); kept watchers released through `watch.release` (protocol 40 → 41, ADR 0056). `files_changed` written only for Markdown batches, from the in-RAM working list.
- Item 13 (Tasks 10–12): `[features] turns` already exists; Task 10 closes its last gap. `[features] conflicts` (off) makes `record_edits` return at once and the UI skip `conflicts.apply/prune` (`SettingsHolder::features()`, `Hub::use_settings` / `Hub::features()`).
- Item 14 (Tasks 13–15, terminal): `scan::ScanRate` (≤ 4 scans/s, trailing scan) and `Emulator::tail_to_cursor`; UI scrollback 5,000 and `GridCell.zerowidth: Option<Box<[char]>>` in a reused grid; `replay_start` in `crates/mkrew-core/src/log_tail.rs`, UI replay 1 MiB. ADR 0057.
- New (Tasks 16–19): still working dot and time-label-driven `status::redraw_due` (ADR 0058; the "Starting agent" and "Transcribing" marks keep their dots as `loading_dots`); `Phone::polled -> bool`; single-theme Mermaid cache; optional `History.lanes`.

**Tech Stack:** Rust 2024, GPUI (`gpui-pre =0.3.7`), alacritty_terminal 0.26, rusqlite, notify, serde, pulldown-cmark, mermaid-rs-renderer, insta. No new dependencies.

**Spec:** `docs/SPEC.md`, cited per task. Source of the choices: `docs/PERF_REVIEW.md` (IDs A1–A6, B1–B12) and the 2026-10-13 rescan (IDs N1, N2, N5). Open questions are at the end, each with the default this plan uses.

**Order:** the decision table's row order is the execution order (dependencies first: 8 before 9, 3 before 6); an incremental run takes only the selected rows (see `## Executor rules (incremental runs)`). Hard dependencies: Task 6 calls `Shell::show_docs` from Task 3; Task 9 uses `Hub::set_working` / the `working` map from Task 8. Soft links with written fallbacks: Task 12 and Task 18 edit the settings reload that Task 6 moves to `Shell::reload_settings_if_changed`; Tasks 2 and 3 edit `Shell::dispatch` arms that Task 1 also edits (re-read the arm, no API from Task 1 is needed); Tasks 13–15 share ADR 0057. Every task names what it consumes in its **Interfaces** block.

**Inbox mapping:**

| Inbox line | Task |
| --- | --- |
| 115 `11. Perf: stop whole-app re-renders in the UI` | 1, 2, 3, 4 (dropped), 5, 6 |
| 126 `12. Perf: daemon watcher, git.changed and SQLite` | 7, 8, 9 |
| 132 `13. Perf: turn snapshots and conflict detection become opt-in` | 10, 11 (done), 12 |
| 138 `14. Perf: terminal path (daemon scan, scrollback, replay)` | 13, 14, 15 |
| none yet (proposed 2026-10-13; the user adds a line if accepted) | 16, 17, 18, 19 |

## Global Constraints

- Windows native is first-class: ConPTY, named pipes, `PATHEXT`, `dunce` paths (AGENTS.md, SPEC §2.4).
- "An agent is just a terminal command. No per-agent code in the core." (SPEC §2.1). Every agent difference lives in its TOML profile. There is no `match "claude"` anywhere in Rust code.
- Every color, spacing, radius, font size and duration comes from `crates/mkrew-ui/src/theme.rs` (SPEC §14).
- No `unwrap`/`expect` outside tests unless provably infallible, with a comment saying why.
- `thiserror` in libraries, `anyhow` with context in binaries, `tracing` for logs.
- No test may call a real agent or the network.
- Deviating from the spec requires an ADR in `docs/decisions/` and a SPEC update in the same change. ADR numbers in this plan: 0055 (Task 6, amends ADR 0024), 0056 (Task 8), 0057 (Task 13; Tasks 14–15 extend it), 0058 (Task 16). Task 12 amends ADR 0046, Task 18 amends ADR 0021. The highest existing ADR is 0054. Before creating one, list `docs/decisions/` and take the next free number if the plan's number is taken (renumber the task's references).
- **Protocol version:** only Task 8 changes the wire format (`watch.release`). It bumps `PROTOCOL_VERSION` (`crates/mkrew-protocol/src/lib.rs:16`, 40 today) to 41. Check the number again before the bump; parallel branches have collided. After the bump, run `cargo build -p mkrew-cli -p mkrew-fakeagent --bins -j 4` before the daemon tests.
- **Migrations:** none (latest is 0018). Never delete a migration or a table.
- Few dependencies: add no new crates.
- GPUI rules: set `.hover()` at most once per element (`button()`/`quiet_button()`/`row_action()`/`menu_item()` already set it; `group_hover` is a separate slot). An element with `track_focus` grabs focus on any mouse down.
- **Commits:** when a task passes verification, commit it with the Conventional Commit message its last step gives. Stage only that task's paths (`git add <paths>`), and only that task's hunks (`git add -p`) in files other tasks or the user also edit (`shell/connection.rs`, `session.rs`, `theme.rs`, `SPEC.md`, `HUMAN_CHECKS.md`). Never `git add -A` / `.`, never push, reset, checkout, stash or clean. Never stage `docs/INBOX.md`, `docs/PERF_REVIEW.md`, this plan, `.github/workflows/*`, `packaging/`, `docs/BUILD_VI.md`, `Cargo.toml`/`Cargo.lock`, or the x11/wayland lines in `crates/mkrew-ui/Cargo.toml`.
- Other agents and the user edit the tree at the same time: re-read a file before editing it and leave hunks you did not write alone.
- Never write files outside this repo. Edits to the user's own `<config>/mkrew/*` files (e.g. turning on `[features]`) are **user steps**, listed in HUMAN_CHECKS.
- **Inbox:** items 11–14 already read `[~] … → plan: docs/superpowers/plans/2026-10-10-inbox-perf.md Task …`. When the last task of an item passes verification, change its line from `[~]` to `[x]` and leave the plan suffix as it is. Do not stage the inbox.
- **HUMAN_CHECKS:** Task 1 creates the section `### To verify — 2026-10-13 perf (plan docs/superpowers/plans/2026-10-10-inbox-perf.md)` directly above the first existing `### To verify —` heading. Every later task appends its bullets to the end of that section; if a task runs before Task 1, it creates the section the same way.
- Verify with `sh scripts/verify.sh` (in the omp shell `bash` is WSL). If rustc crashes on mkrew-ui test builds (STATUS_STACK_BUFFER_OVERRUN), run these three in order:
  1. `cargo fmt --all -- --check`
  2. `cargo clippy --workspace --all-targets -- -D warnings`
  3. `cargo test --workspace -j 4`
- The C: drive is nearly full: no `cargo clean`, no extra target dirs, no downloads. Check free space before a full build.
- If a build fails with "Access is denied" on an `.exe` (a running daemon or fake agent holds it), rename it to `*.locked-old.exe`. Never kill the user's sessions or the daemon.
- `cargo insta` is not installed. Write accepted `.snap` files by hand.
- `crates/mkrew-ui/src/main.rs` is ~20k lines and `impl Shell` lives in `src/shell/*.rs`: read ranges, never the whole file. Line numbers below are from 2026-10-13 and drift as tasks land. Re-find each function by name.
- UI work cannot be unit-tested headless: pure logic gets tests; every visible change gets a HUMAN_CHECKS smoke line that the user runs with `cargo run -p mkrew-ui`.
- Nothing in this plan was compiled while it was written. Where a task names an unverified name (it says so), check it against the code and adjust; do not invent a second convention.

## Review Focus

1. **A `done` or `needs_input` of a session you are not looking at (other workspace, Docs open, sidebar hidden).** Expected: the sidebar marks, the waiting count and the OS notification still update at once. Only usage/turn/plan/ports/queue/board/conflict data of hidden surfaces skips the redraw, and it is current when shown. Pinned by Task 1's `redraw` tests.
2. **A prompt printed just after a screen scan, then silence.** Expected: the session still becomes `needs_input`, at the trailing scan ≤ 250 ms later, and a queued send never goes into a busy agent. Pinned by Task 13's `ScanRate` tests.
3. **A session that worked for a week, then idles.** Expected: its token totals survive pruning (`usage` rows are never pruned), and a non-Markdown write by one agent still attributes the turn's files to the working agent. Pinned by Tasks 7 and 9.
4. **`[features]` toggled while agents run.** Expected: `conflicts` off clears warnings at once and records nothing; `turns` off never holds the turn-start hook. Pinned by Tasks 10 and 12.
5. **Pin, rename, color, archive, collapse, filter typing or viewing a `done` pane while the sidebar cache is warm.** Expected: the next frame shows it, the waiting count ignores the filter, and `17m` labels keep moving without dirtying. Pinned by Task 2.
6. **A working agent with the window in front, no output (Task 16).** Expected: the root renders once per second during the first minute of the turn (label seconds), then once per minute; no animation frames. HUMAN_CHECKS covers the look; `status::redraw_due` tests pin the timing.

Also pinned inside their tasks: a doc changed while Docs is hidden (Task 3), a git change burst and a workspace switch (Task 5), a released watcher with a live session (Task 8), stale grid cells and zero-width chars (Task 14), a log cut inside a char or an escape sequence (Task 15), phone polls that change nothing (Task 17), a theme switch with a diagram on screen (Task 18). How smooth the app feels, and `mkrew-ui` CPU with 2–3 working agents before and after, cannot be unit-tested; Task 1's HUMAN_CHECKS line covers it.

---

### Task 1: Daemon events stop re-rendering the whole app (perf B1)

> **Refresh 2026-10-13:** still needed, redesigned against the current screen.
> - Dispatch moved from `main.rs` to `shell/connection.rs:604`; `Shell::shown` goes there, the `use` stays in `main.rs`. `main.rs` no longer holds the sidebar markup.
> - The status bar is gone (ADR 0054): usage / `ctx` are drawn only in pane headers, the session panel (inspector) and Board cards; the Inbox count is the sidebar footer, which state events already redraw (rules below).
> - Ports are now drawn by the title bar Run pill (active workspace, every view) and the RUN group of the full sidebar (`shell/sidebar.rs:21`), not by workspace-row tooltips.
> - The sidebar is `SidebarView`, which observes the Shell (`shell/sidebar_view.rs:17`), so one skipped `cx.notify()` also skips the sidebar re-render. `Shown` is built lazily, so a pure output batch (~60/s, `OUTPUT_FRAME`) allocates nothing.

Inbox: `11. Perf: stop whole-app re-renders in the UI` (bullet B1 and the CPU check; do not tick yet, Task 6 ticks it). Spec: §10 (SPEC.md:685-859), §11 (:861-905), §15 (:1233-1246, Idle CPU row :1239). No SPEC change and no ADR: no observable behavior changes, only when the root re-renders.

**Current state (investigated):**
- `Shell::dispatch` (`shell/connection.rs:604`) feeds each `Incoming::Event` to `board` (:615), `turns` (:619), `usages` (:622), `plans` (:625), `ports` (:628), `composers` (:631), `follow`, `sidebar` (:643), `conflicts` (:650-653), every workspace's `tabs`, then the docs / files / git handlers. Six models end in an unconditional `cx.notify()` when their data changed: `board.apply` (also calls `drop_gone_targets`), `turns`, `usages`, `plans`, `ports`, `composers`; and `conflicts.prune(..) || detected`. `CommentUpdated` (:677) and `AiGenerated` (:695) also notify unconditionally. Each `apply` already returns "something shown changed"; nothing asks whether the changed thing is on screen. `Shell::render` (`main.rs:596`) then rebuilds title bar, view and dialogs, and `SidebarView` re-renders through its observer.
- What each drawn surface reads (re-verified by grepping the field reads on 2026-10-13):
  - `usages`: pane status `usage::context_share` (`pane_render.rs:577`, the tab bar for a single pane, the header among several), session panel `inspector::session_card` (`inspector.rs:341`, focused session only), Board cards `cards.usages.task_total` (`cards.rs:165`, `BoardCtx.usages` from `board.rs:530`). Nothing else: the status bar is gone.
  - `turns`: pane status pill `pane_text::pill_text(.., self.turns.get(session))` (`pane_render.rs:549`). Nothing else.
  - `composers` queue: pane status `⧗ N queued` (`pane_render.rs:567`) and the composer panel (`composer.rs:633`), per visible pane. `composers.apply` reacts to `SessionQueue` only.
  - `plans`: the session panel Plan card (`inspector.rs:234`) and the selected sidebar row's progress bar (`shell/sidebar.rs:538`, full sidebar only; the compact rail draws no bar), both for the focused session. `plans.apply` also reacts to `SessionUpdated` (plan unlinked) and `SessionExited`, which already answer `true` below.
  - `ports`: pane title (`pane_render.rs:723`), the title bar Run pill of the active workspace (`chrome.rs:148` → `run.rs:281`, `run_summary(project_id)` reads `ports`), and the RUN group of every open workspace of the full sidebar (`shell/sidebar.rs:27-37`, :137). The compact rail draws neither. `ports.apply` also drops ports on `SessionExited` (answers `true`).
  - `board` (`TaskUpdated` / `TaskDeleted`): the Board (`shell/board.rs:501`), the pane status folder (`pane_render.rs:533`) and the Files / Git worktree chooser (`target_chooser`, `files_render.rs:189`, `git_render.rs:209`). The title bar and the sidebar do not read it. `drop_gone_targets` (`files.rs:772`) re-reads hidden views when they are shown, and a shown Files / Git view is covered by `tasks_drawn`, so it stays unconditional.
  - `conflicts`: pane status badge (`pane_render.rs:552`) and Board cards (`cards.rs:180`). Nowhere else. `conflicts.prune` runs on every event (:651) and stays that way here: Task 12 gates it by `[features] conflict_detection`; this task only gates its notify, and the table below already covers it.
  - `CommentUpdated`: only the Git views' comments (`connection.rs:673-676`); Docs re-lists in `doc_event` (Task 3's area). `AiGenerated` feeds the Git commit box and the pane composer.
- Events the sidebar handles (`SessionState`, `SessionExited`, `SessionUpdated`, `SessionTerminalTitle`, `ProjectUpdated`; `Sidebar::apply` returns `Change::Redraw`, :643-648) feed the sidebar dots, the footer Inbox count (`shell/sidebar.rs:735`), tab urgency and `Viewed` (`Shell::render` marks the focused pane viewed, `main.rs:616-621`). These MUST keep re-rendering; this task does not touch them. OS notifications are sent by the daemon, not the UI.
- Every UI action that changes what is on screen (switch workspace / view / tab, toggle inspector, open a dialog, window activation `connection.rs:40`) already calls `cx.notify()`. A model updated silently while hidden is read fresh by the next render.
- `Shell::focused_session()` (`focus.rs:42`) is `None` unless the view is Terminals. `tabs::on_screen(window_visible, Option<&Tabs>)` (`tabs.rs:503`) lists the visible panes (a zoomed pane alone). `sidebar::Layout::hidden()` / `compact()` (`sidebar.rs:166-171`). `modal_open()` (`focus.rs:7`) is true for every dialog. `Shell::view()` (`workspace.rs:333`) reads Docs as Terminals while `[features] docs` is off (ADR 0046); `View::Docs` stays in the pure table and draws none of these models.
- Tests: `mkrew-ui` unit tests live in `mod tests` of each module. `Event` structs: `SessionUsageEvent`, `SessionTurnEvent`, `SessionPlanEvent`, `SessionPortsEvent`, `SessionQueueEvent`, `TaskDeletedEvent`, `ConflictDetectedEvent`, `SessionStateEvent`, `SessionExitedEvent` (all in `mkrew-protocol/src/message.rs`, re-exported at the crate root). `Event` has 21 variants (`message.rs:336-404`); the match below lists all of them.
- Name clash: `main.rs` already imports `mkrew_ui::docs::Shown` (:53) and `files_view::Shown` / `git_view::Shown` exist. The new type is only used as `redraw::Shown`.

**Files:**
- Create: `crates/mkrew-ui/src/redraw.rs` (`SidebarMode`, `Shown`, `event_needs_redraw`, `conflicts_need_redraw`, tests)
- Modify: `crates/mkrew-ui/src/lib.rs` (`pub mod redraw;` after `pub mod quick_open;`, line 59)
- Modify: `crates/mkrew-ui/src/main.rs` (`use mkrew_ui::redraw::{self, SidebarMode};` after the `quick_open` use, line 83)
- Modify: `crates/mkrew-ui/src/shell/connection.rs` (new `Shell::shown`, `Shell::dispatch` model block, conflicts block, comment and AI arms)
- Modify: `docs/HUMAN_CHECKS.md` (creates the shared section)

**Interfaces:**
- Produces:
  ```rust
  pub enum SidebarMode { Hidden, Compact, Full }
  pub struct Shown {
      pub workspace: Option<Ulid>,   // active workspace
      pub view: View,                // board::View of the active workspace
      pub panes: Vec<Ulid>,          // sessions whose pane is drawn (Terminals view; zoom respected)
      pub focused: Option<Ulid>,     // Shell::focused_session()
      pub inspector: bool,           // the session panel is drawn
      pub sidebar: SidebarMode,
      pub modal: bool,               // any dialog is open
  }
  pub fn event_needs_redraw(event: &Event, shown: &Shown, sessions: &[Session]) -> bool
  pub fn conflicts_need_redraw(shown: &Shown) -> bool
  ```
  and in the shell crate: `Shell::shown(&self, window: &Window) -> redraw::Shown`.
- Consumes: `board::View`, `mkrew_protocol::{Event, Session}`, `tabs::on_screen`, `Shell::focused_session`, `Shell::modal_open`, `sidebar::Layout`.
- Contract: `event_needs_redraw` is asked only after a model reported that its data changed. It answers "does the window draw this data now"; events owned by another handler (state, exit, tabs, docs, git) answer `true` so a future reader of the match sees they are not decided here. Task 12 later wraps `conflicts.prune` in the `conflict_detection` switch; `conflicts_need_redraw` stays the redraw rule for it.

- [ ] **Step 1: Failing tests**

Create `crates/mkrew-ui/src/redraw.rs` containing only the test module below, and in `lib.rs` add `pub mod redraw;` after `pub mod quick_open;` (it sorts before `router`).

```rust
#[cfg(test)]
mod tests {
    use mkrew_protocol::{
        ConflictDetectedEvent, SessionExitedEvent, SessionKind, SessionPlanEvent,
        SessionPortsEvent, SessionQueueEvent, SessionState, SessionStateEvent, SessionTurnEvent,
        SessionUsageEvent, StateSource, TaskDeletedEvent,
    };

    use super::*;

    /// `a` is the focused pane of the shown workspace, `b` lives in a hidden tab of it, `c` in
    /// another workspace.
    struct World {
        ws: Ulid,
        other: Ulid,
        sessions: Vec<Session>,
    }

    impl World {
        fn a(&self) -> Ulid {
            self.sessions[0].id
        }
        fn b(&self) -> Ulid {
            self.sessions[1].id
        }
        fn c(&self) -> Ulid {
            self.sessions[2].id
        }
    }

    fn session(project_id: Ulid) -> Session {
        Session {
            id: Ulid::new(),
            task_id: None,
            project_id,
            agent_profile: "claude".to_owned(),
            kind: SessionKind::Agent,
            title: None,
            terminal_title: None,
            color: None,
            native_session_id: None,
            state: SessionState::Working,
            state_source: StateSource::Daemon,
            state_changed_at: 0,
            exit_code: None,
            cols: 80,
            rows: 24,
            started_at: 0,
            ended_at: None,
            pinned: false,
            plan_path: None,
        }
    }

    fn world() -> World {
        let (ws, other) = (Ulid::new(), Ulid::new());
        World {
            ws,
            other,
            sessions: vec![session(ws), session(ws), session(other)],
        }
    }

    /// Terminals view of `ws`, `a` focused, inspector open, compact rail.
    fn shown(w: &World) -> Shown {
        Shown {
            workspace: Some(w.ws),
            view: View::Terminals,
            panes: vec![w.a()],
            focused: Some(w.a()),
            inspector: true,
            sidebar: SidebarMode::Compact,
            modal: false,
        }
    }

    /// Nothing of any session is drawn except the sidebar's own rows.
    fn nothing_shown(w: &World) -> Shown {
        Shown {
            workspace: Some(w.ws),
            view: View::Files,
            panes: Vec::new(),
            focused: None,
            inspector: false,
            sidebar: SidebarMode::Hidden,
            modal: false,
        }
    }

    fn usage(id: Ulid) -> Event {
        Event::SessionUsage(SessionUsageEvent {
            session_id: id,
            input: 1,
            output: 1,
            cache_read: 0,
            cache_write: 0,
            context: 0,
            context_window: None,
            model: None,
            transcript: None,
        })
    }

    fn turn(id: Ulid) -> Event {
        Event::SessionTurn(SessionTurnEvent {
            session_id: id,
            n: 1,
            started_at: 0,
            ended_at: 1,
            files: Vec::new(),
        })
    }

    fn queue(id: Ulid) -> Event {
        Event::SessionQueue(SessionQueueEvent {
            session_id: id,
            items: Vec::new(),
        })
    }

    fn plan(id: Ulid) -> Event {
        Event::SessionPlan(SessionPlanEvent {
            session_id: id,
            name: "plan".into(),
            done: 1,
            total: 2,
            items: Vec::new(),
            error: None,
        })
    }

    fn ports(id: Ulid) -> Event {
        Event::SessionPorts(SessionPortsEvent {
            session_id: id,
            ports: vec![8080],
        })
    }

    fn redraws(w: &World, shown: &Shown, event: &Event) -> bool {
        event_needs_redraw(event, shown, &w.sessions)
    }

    /// The focused pane's usage feeds the session panel and its pane status; a session of
    /// another tab or workspace feeds nothing on screen.
    #[test]
    fn usage_redraws_for_a_visible_pane_only() {
        let w = world();
        let mut s = shown(&w);
        assert!(redraws(&w, &s, &usage(w.a())), "focused pane");
        assert!(!redraws(&w, &s, &usage(w.b())), "hidden tab, inspector on another session");
        assert!(!redraws(&w, &s, &usage(w.c())), "other workspace");
        // A split pane shows its own context share in its header.
        s.panes.push(w.b());
        assert!(redraws(&w, &s, &usage(w.b())));
    }

    /// Board cards show the tokens of their task's session; the Files view shows none.
    #[test]
    fn usage_redraws_the_board_of_the_shown_workspace_only() {
        let w = world();
        let mut s = nothing_shown(&w);
        s.view = View::Board;
        assert!(redraws(&w, &s, &usage(w.b())));
        assert!(!redraws(&w, &s, &usage(w.c())), "another workspace's board is not drawn");
        s.view = View::Files;
        assert!(!redraws(&w, &s, &usage(w.b())));
    }

    #[test]
    fn turns_and_queues_need_a_visible_pane() {
        let w = world();
        let s = shown(&w);
        for event in [turn(w.a()), queue(w.a())] {
            assert!(redraws(&w, &s, &event), "{event:?}");
        }
        for event in [turn(w.b()), queue(w.b()), turn(w.c()), queue(w.c())] {
            assert!(!redraws(&w, &s, &event), "{event:?}");
        }
    }

    /// The plan is drawn for the focused session only, in the inspector or on the selected
    /// row of the full sidebar.
    #[test]
    fn plan_needs_the_inspector_or_the_full_sidebar_and_the_focused_session() {
        let w = world();
        let mut s = shown(&w);
        assert!(redraws(&w, &s, &plan(w.a())));
        assert!(!redraws(&w, &s, &plan(w.b())), "not the focused session");
        s.inspector = false;
        assert!(!redraws(&w, &s, &plan(w.a())), "inspector closed, compact rail has no bar");
        s.sidebar = SidebarMode::Full;
        assert!(redraws(&w, &s, &plan(w.a())), "the selected row's progress bar");
    }

    /// A run's ports are in its pane title, in the Run pill of the shown workspace (every
    /// view) and in the RUN group of the full sidebar (every open workspace).
    #[test]
    fn ports_show_in_the_run_pill_and_the_full_sidebars_run_group() {
        let w = world();
        let mut s = nothing_shown(&w);
        assert!(redraws(&w, &s, &ports(w.b())), "Run pill of the shown workspace");
        assert!(!redraws(&w, &s, &ports(w.c())), "other workspace, no sidebar rows");
        s.sidebar = SidebarMode::Compact;
        assert!(!redraws(&w, &s, &ports(w.c())), "the rail has no RUN group");
        s.sidebar = SidebarMode::Full;
        assert!(redraws(&w, &s, &ports(w.c())), "its workspace's RUN group");
    }

    /// The footer's Inbox count, the sidebar dots and `Viewed` depend on these: never skipped.
    #[test]
    fn state_changes_redraw_even_when_nothing_else_is_shown() {
        let w = world();
        let s = nothing_shown(&w);
        let state = Event::SessionState(SessionStateEvent {
            session_id: w.c(),
            state: SessionState::NeedsInput,
            source: StateSource::Hook,
            changed_at: 1,
        });
        let exited = Event::SessionExited(SessionExitedEvent {
            session_id: w.c(),
            exit_code: Some(0),
        });
        assert!(redraws(&w, &s, &state));
        assert!(redraws(&w, &s, &exited));
    }

    /// Dialogs read models we did not audit (compare, history, task editor): while one is open
    /// every change redraws.
    #[test]
    fn an_open_dialog_keeps_every_change_redrawing() {
        let w = world();
        let mut s = nothing_shown(&w);
        assert!(!redraws(&w, &s, &usage(w.c())));
        s.modal = true;
        assert!(redraws(&w, &s, &usage(w.c())));
    }

    #[test]
    fn task_changes_redraw_the_views_that_read_tasks() {
        let w = world();
        let event = |project_id| {
            Event::TaskDeleted(TaskDeletedEvent {
                task_id: Ulid::new(),
                project_id,
            })
        };
        let mut s = shown(&w);
        for view in [View::Board, View::Terminals, View::Files, View::Git] {
            s.view = view;
            assert!(redraws(&w, &s, &event(w.ws)), "{view:?}");
            assert!(!redraws(&w, &s, &event(w.other)), "{view:?}: another workspace");
        }
        s.view = View::Docs;
        assert!(!redraws(&w, &s, &event(w.ws)));
    }

    #[test]
    fn conflicts_show_on_the_board_and_in_pane_headers_only() {
        let w = world();
        let detected = Event::ConflictDetected(ConflictDetectedEvent {
            project_id: w.ws,
            path: "src/a.rs".into(),
            session_ids: vec![w.a(), w.b()],
            task_ids: Vec::new(),
        });
        let mut s = shown(&w);
        for view in [View::Board, View::Terminals] {
            s.view = view;
            assert!(conflicts_need_redraw(&s) && redraws(&w, &s, &detected), "{view:?}");
        }
        for view in [View::Docs, View::Files, View::Git] {
            s.view = view;
            assert!(!conflicts_need_redraw(&s) && !redraws(&w, &s, &detected), "{view:?}");
        }
    }
}
```

Run: `cargo test -p mkrew-ui --lib redraw -j 4`
Expected: FAIL to compile (`Shown`, `SidebarMode`, `event_needs_redraw`, `View` are not defined).

- [ ] **Step 2: The pure decision**

Put this above the test module in `redraw.rs`:

```rust
//! Which daemon events make the root `Shell` render again (perf review B1). A model (usage,
//! turns, plan, ports, queue, board, conflicts) always keeps its data; the root only needs to
//! redraw when the window draws that data now. Anything that changes what is on screen (a view,
//! workspace, tab or inspector switch, a dialog) re-renders on its own, so data skipped while
//! hidden is read fresh when it is shown. The sidebar view observes the Shell, so a skipped
//! notify skips it too. Pure and tested headlessly; `Shell::shown` builds [`Shown`].

use mkrew_protocol::{Event, Session};
use ulid::Ulid;

use crate::board::View;

/// How the sidebar is drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SidebarMode {
    Hidden,
    /// The rail: no progress bar, no RUN group.
    Compact,
    Full,
}

/// What the window draws now.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Shown {
    pub workspace: Option<Ulid>,
    pub view: View,
    /// Sessions whose pane is drawn: the active tab's visible panes in the Terminals view.
    pub panes: Vec<Ulid>,
    pub focused: Option<Ulid>,
    pub inspector: bool,
    pub sidebar: SidebarMode,
    /// Some dialog is open.
    pub modal: bool,
}

/// Whether an event whose model reported "my data changed" must re-render the root. Events
/// owned by another handler (sidebar state, tabs, docs, git) answer `true`: they are never
/// decided here, and their handlers keep their own redraw rules.
pub fn event_needs_redraw(event: &Event, shown: &Shown, sessions: &[Session]) -> bool {
    // Dialogs read models of their own (compare, history, task editor): keep today's behavior.
    if shown.modal {
        return true;
    }
    let in_workspace = |project_id: Ulid| shown.workspace == Some(project_id);
    let project_of = |session_id: Ulid| {
        sessions
            .iter()
            .find(|s| s.id == session_id)
            .map(|s| s.project_id)
    };
    // The inspector reads the focused pane, the pane status every visible one.
    let on_pane = |id: Ulid| shown.focused == Some(id) || shown.panes.contains(&id);
    match event {
        // Pane status, session panel; Board cards show their task's tokens.
        Event::SessionUsage(e) => {
            on_pane(e.session_id)
                || (shown.view == View::Board && project_of(e.session_id).is_some_and(in_workspace))
        }
        // Pane status pill and `⧗ N queued`.
        Event::SessionTurn(e) => on_pane(e.session_id),
        Event::SessionQueue(e) => on_pane(e.session_id),
        // Inspector Plan card, or the selected row's bar in the full sidebar.
        Event::SessionPlan(e) => {
            shown.focused == Some(e.session_id)
                && (shown.inspector || shown.sidebar == SidebarMode::Full)
        }
        // Pane title, the Run pill of the shown workspace, the RUN group of the full sidebar.
        Event::SessionPorts(e) => {
            on_pane(e.session_id)
                || shown.sidebar == SidebarMode::Full
                || project_of(e.session_id).is_some_and(in_workspace)
        }
        Event::TaskUpdated(task) => tasks_drawn(shown, task.project_id),
        Event::TaskDeleted(deleted) => tasks_drawn(shown, deleted.project_id),
        Event::ConflictDetected(_) => conflicts_need_redraw(shown),
        // Review comments of the Git view; Docs re-lists in its own handler.
        Event::CommentUpdated(comment) => {
            shown.view == View::Git && in_workspace(comment.project_id)
        }
        // The commit message box (Git) and the pane composer (Terminals).
        Event::AiGenerated(_) => matches!(shown.view, View::Git | View::Terminals),
        Event::SessionState(_)
        | Event::SessionExited(_)
        | Event::SessionUpdated(_)
        | Event::SessionTerminalTitle(_)
        | Event::ProjectUpdated(_)
        | Event::TabUpdated(_)
        | Event::SessionSnapshot(_)
        | Event::NotificationClicked(_)
        | Event::SessionEdited(_)
        | Event::DocChanged(_)
        | Event::GitChanged(_) => true,
    }
}

/// Conflict warnings are drawn on Board cards and in pane status.
pub fn conflicts_need_redraw(shown: &Shown) -> bool {
    shown.modal || matches!(shown.view, View::Board | View::Terminals)
}

/// Tasks are read by the Board, the pane status folder and the Files / Git worktree chooser
/// of the shown workspace.
fn tasks_drawn(shown: &Shown, project_id: Ulid) -> bool {
    shown.workspace == Some(project_id)
        && matches!(
            shown.view,
            View::Board | View::Terminals | View::Files | View::Git
        )
}
```

Run: `cargo test -p mkrew-ui --lib redraw -j 4`
Expected: PASS (9 tests).

- [ ] **Step 3: Use it in `dispatch`**

`main.rs`: after `use mkrew_ui::quick_open::{self, QuickOpen, Results};` (line 83) add `use mkrew_ui::redraw::{self, SidebarMode};`.

`shell/connection.rs`: add this method right before `/// Routes a batch of incoming frames; returns whether a pane got new content.` (line 603, after `list_failed`). The shell modules take `View`, `on_screen` and the rest through `use crate::*`:

```rust
    /// What the window draws now, for [`redraw::event_needs_redraw`].
    pub(crate) fn shown(&self, window: &Window) -> redraw::Shown {
        let terminals = self.view() == View::Terminals;
        let tabs = self
            .workspace
            .filter(|_| terminals)
            .and_then(|id| self.tabs.get(&id));
        redraw::Shown {
            workspace: self.workspace,
            view: self.view(),
            panes: on_screen(window.is_visible(), tabs),
            focused: self.focused_session(),
            inspector: self.inspector_open && terminals,
            sidebar: if self.sidebar_layout.hidden() {
                SidebarMode::Hidden
            } else if self.sidebar_layout.compact() {
                SidebarMode::Compact
            } else {
                SidebarMode::Full
            },
            modal: self.modal_open(),
        }
    }
```

In `dispatch`, replace

```rust
        let mut peek_changed = false;
        for incoming in batch {
            if let Incoming::Event(event) = &incoming {
                if self.board.apply(event) {
                    self.drop_gone_targets(window, cx);
                    cx.notify();
                }
                if self.turns.apply(event) {
                    cx.notify();
                }
                if self.usages.apply(event) {
                    cx.notify();
                }
                if self.plans.apply(event) {
                    cx.notify();
                }
                if self.ports.apply(event) {
                    cx.notify();
                }
                if self.composers.apply(event) {
                    cx.notify();
                }
```
with
```rust
        let mut peek_changed = false;
        // What the window draws, read once at the first event of the batch (an output-only
        // batch, up to ~60/s, never needs it). A batch that switches workspace or view (a
        // notification click) notifies on its own, and the render after it reads every model,
        // so a stale `shown` can only cost one redraw, never a stale screen.
        let mut shown_now: Option<redraw::Shown> = None;
        for incoming in batch {
            if let Incoming::Event(event) = &incoming {
                let shown = &*shown_now.get_or_insert_with(|| self.shown(window));
                // Every model keeps its data; the root only redraws what the window draws.
                // `|=` (not `||`): each model must see the event.
                let mut data_changed = false;
                if self.board.apply(event) {
                    self.drop_gone_targets(window, cx);
                    data_changed = true;
                }
                data_changed |= self.turns.apply(event);
                data_changed |= self.usages.apply(event);
                data_changed |= self.plans.apply(event);
                data_changed |= self.ports.apply(event);
                data_changed |= self.composers.apply(event);
                if data_changed && redraw::event_needs_redraw(event, shown, self.sidebar.sessions())
                {
                    cx.notify();
                }
```

Replace

```rust
                let detected = self.conflicts.apply(event);
                if self.conflicts.prune(&self.board, self.sidebar.sessions()) || detected {
                    cx.notify();
                }
```
with
```rust
                let detected = self.conflicts.apply(event);
                // `prune` first: it must run on every event (Task 12 gates it by the feature
                // switch, not by what is shown).
                let pruned = self.conflicts.prune(&self.board, self.sidebar.sessions());
                if (pruned || detected) && redraw::conflicts_need_redraw(shown) {
                    cx.notify();
                }
```

Replace the unconditional notifies of the comment and AI arms:

```rust
                    for view in self.git.values_mut() {
                        view.comments.apply_event(view.project_id, comment);
                    }
                    cx.notify();
```
with
```rust
                    for view in self.git.values_mut() {
                        view.comments.apply_event(view.project_id, comment);
                    }
                    if redraw::event_needs_redraw(event, shown, self.sidebar.sessions()) {
                        cx.notify();
                    }
```
and
```rust
                    self.composers.enhanced(done.generation_id, result);
                    cx.notify();
```
with
```rust
                    self.composers.enhanced(done.generation_id, result);
                    if redraw::event_needs_redraw(event, shown, self.sidebar.sessions()) {
                        cx.notify();
                    }
```
Do not touch the `sidebar.apply` match (`Change::Redraw` / `Refresh`), the `layout_changed` / `peek` notifies, or the docs / files / git handlers (Task 3).

Compile check: `cargo check -p mkrew-ui -j 4`
Expected: no errors. If the borrow checker rejects `shown` living across `self.drop_gone_targets(window, cx)` (it borrows only the local `shown_now`, so it should not), clone the `Shown` once per batch instead.

- [ ] **Step 4: HUMAN_CHECKS (creates the shared section)**

In `docs/HUMAN_CHECKS.md`, directly above the first `### To verify —` heading (find it with `grep -n "^### To verify" docs/HUMAN_CHECKS.md`; line 18 on 2026-10-13, and above any section another plan already put there), add:

```markdown
### To verify — 2026-10-13 perf (plan docs/superpowers/plans/2026-10-10-inbox-perf.md)

- [ ] **Events only redraw what is shown (Task 1):** 2 workspaces, an agent in each, session panel open (`◨`) on workspace A's agent, full sidebar.
  - Let workspace B's agent finish turns: A's panes, `ctx` and the session panel do not flicker, and still update live when A's own agent ends a turn (tokens and `ctx` change at once).
  - Open the Board: a card's token total updates live when its agent ends a turn. Switch to Files, let the agent end another turn, come back to Board: the total is current (nothing stale after a view switch).
  - Link a plan (`.md` checklist) to the focused agent: its Plan card and the selected sidebar row's bar update live while you tick items; close the session panel, tick more, reopen it: current.
  - Start a run command that prints a port: the title bar Run pill shows `:PORT`; in the full sidebar the RUN group of another expanded workspace shows its ports too.
  - The footer `N need you` count, the sidebar state dots and the OS notifications still follow every agent, also with the window on another workspace, on Files, and with the compact rail. Focus a `done` pane: it leaves the count at once.
- [ ] **CPU (Task 1 / item 11):** open Task Manager → Details → `mkrew.exe`, run 3 agents in `working` (long tasks) with the window visible on Terminals, and read CPU over 60 s. Before: build of the commit before this plan (copy its `target\release\mkrew.exe` aside first). After: the build after the last task of this plan. Write the numbers here: before ___ % / after ___ % of one core. (The 500 ms working-dot tick stays until Task 16, so the drop here is the avoided renders from other sessions' events.)
```

- [ ] **Step 5: Verify and commit**

Run: `cargo test -p mkrew-ui --lib redraw -j 4` (expect PASS), then the full verification: `sh scripts/verify.sh`. If rustc crashes on mkrew-ui test builds, run in order: `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace -j 4`. Do not tick the inbox line (Tasks 2–6 follow).

Commit (never push; if `git diff docs/HUMAN_CHECKS.md` shows hunks that are not this task's, stage only yours with `git add -p docs/HUMAN_CHECKS.md`):
```
git add crates/mkrew-ui/src/redraw.rs crates/mkrew-ui/src/lib.rs crates/mkrew-ui/src/main.rs crates/mkrew-ui/src/shell/connection.rs docs/HUMAN_CHECKS.md
git commit -m "perf(ui): daemon events redraw the root only when the data is on screen"
```

---

### Task 2: Sidebar, Inbox count and tab bar stop recomputing every render (perf B2)

> **Refresh 2026-10-13:** still needed, reworked around `SidebarView` (0606f04).
> - 0606f04 stops pane output from laying out the sidebar, but `SidebarView` still re-runs `Sidebar::view` and `inbox_entries()` on every Shell notify: each 500 ms working tick, each event, each click. The cache therefore stays in `Sidebar` (the model, shared by the sidebar, its footer and the peek check), and `SidebarView::render` stays a thin wrapper; no change in `sidebar_view.rs`.
> - The status bar is gone (ADR 0054): the Inbox count is read by `render_sidebar_footer` (`shell/sidebar.rs:735`), once per sidebar render, in both the full sidebar and the rail.
> - Code moved: sidebar tests are `sidebar/tests.rs`, tab tests `tabs/tests.rs`, render code `shell/sidebar.rs` and `shell/pane_render.rs`. The old plan's `time_labels` test picked the wrong row (rows are newest first); fixed below.

Inbox: `11. Perf: stop whole-app re-renders in the UI` (bullet B2; do not tick yet). Spec: §10 (SPEC.md:685-859), §11 (:861-905, the Inbox / "Needs you" definition). No SPEC change and no ADR: the rows, counts and texts are identical.

**Current state (investigated):**
- `Sidebar::view` (`sidebar.rs:~457`) runs on every `SidebarView` render: `grouped()` lowercases the filter, builds a `HashMap<Ulid, Vec<&Session>>`, filters by query (`matches` lowercases three fields per session) and sorts each workspace; then counts, `workspace_summary` `format!`s, and every row is built with cloned `String`s. `render_sidebar` (`shell/sidebar.rs:238`) and `render_compact_sidebar` (:680) call it once per render (never both); `peek.rs:181` calls it per peek check (user action).
- `render_sidebar_footer` (`shell/sidebar.rs:728`, in both modes) does `self.inbox_entries().len()` (:735): `inbox::entries` clones every needing session's strings and sorts, only for `.len()`. `inbox::entries` also leaves out sessions whose workspace is not listed (archived), so `len()` is not "sessions with `needs_you`". `inbox_entries()` (`peek.rs:6`) stays for `jump_next_inbox` and the peek cycle (user actions).
- `SessionRow` holds `state_changed_at`, not text; the right-column time (`17m`) is produced at draw time by `SessionRow::time_text(now_ms)` (`sidebar.rs:~255`), and `SidebarView::render` reads `status::now_ms()` per render. `WorkspaceRow.summary` has counts, no time. So the cacheable part is already time-free, and `status::redraw_due` (`connection.rs:102`) keeps notifying the Shell on the 500 ms tick without any cache dirtying.
- `Viewed` (`inbox.rs:29`) changes during render: `Shell::render` calls `self.viewed.view(session)` for the focused pane (`main.rs:616-621`) before its child `SidebarView` renders in the same frame, and that flips the session's `needs_you`. It is an input of both the rows' chip count and the Inbox count, so the cache must key on it. A change of the focused pane or of window activation notifies the Shell, which notifies `SidebarView` through its observer.
- Inputs of the view and every mutation path in `Sidebar` (all `&mut self` methods; fields are private): `load` (also replays events), `add_project`, `toggle`, `filter_mut` (hands out `&mut Input`), `set_chip`, `toggle_all`, `reveal`, `clear_filter`, `remove_session`, `set_title`, `set_color`, `update_session` (reached from `apply` for `SessionState`, `SessionExited`, `SessionUpdated` (pin, rename, plan link, color), `SessionTerminalTitle`), `upsert_project` (from `apply(ProjectUpdated)` and `add_project`: color, pin, rename, archive), `sort_projects` (only from `load` and `upsert_project`). `apply` for `SessionUsage` / `Turn` / `Plan` / `Ports` / `Queue` / `Edited` / `Tab*` / `Task*` returns `Change::None` and changes nothing the view reads: those must not dirty the cache.
- Tab bar (`render_tab_bar`, `shell/pane_render.rs:48,51`): per tab `status::most_urgent(sessions, &tab.sessions())` does `sessions.iter().find` per pane, and `tabs::tab_kind(tab, sessions)` another `find`: O(panes × sessions) per render. Per-frame single lookups with `sidebar.sessions().iter().find(..)`: `pane_render.rs:23` (single pane of the tab bar), `pane_render.rs:428` (`pane_frame`), `shell/inspector.rs:99`, `main.rs:618` (`Shell::render`). Other `.iter().find` sites (`composer.rs`, `navigation.rs`, `tabs.rs`, `menus.rs`, `inspector.rs:448`) are user actions or open menus: leave them.
- Tests: `sidebar/tests.rs` has `project(name)`, `session(project, started_at, state)` (`state_changed_at == started_at`), `workspace(row)`, `session_ids(rows)` and `use super::*`; `inbox.rs` `mod tests` has `project`, `session(project, state, changed_at)`; `status.rs:210` and `tabs/tests.rs:599` pin `most_urgent` / `tab_kind` with slices; `tests/lifecycle.rs:168,199` call `sidebar.rows()`.

**Files:**
- Modify: `crates/mkrew-ui/src/inbox.rs` (`Viewed.version`, `Viewed::version`, `inbox::count`, tests)
- Modify: `crates/mkrew-ui/src/sidebar.rs` (`rev`, caches, `view` → `Rc<View>`, `waiting`, `session`, `build`, `touch`)
- Modify: `crates/mkrew-ui/src/sidebar/tests.rs` (new tests)
- Modify: `crates/mkrew-ui/src/status.rs` (`most_urgent` takes a lookup, its test)
- Modify: `crates/mkrew-ui/src/tabs.rs`, `crates/mkrew-ui/src/tabs/tests.rs` (`tab_kind` takes a lookup, its test)
- Modify: `crates/mkrew-ui/src/shell/sidebar.rs` (`render_sidebar`, `render_compact_sidebar`, `render_sidebar_footer`)
- Modify: `crates/mkrew-ui/src/shell/pane_render.rs` (`render_tab_bar`, `pane_frame`), `crates/mkrew-ui/src/shell/inspector.rs` (`render_inspector`), `crates/mkrew-ui/src/main.rs` (`Shell::render`)
- Modify: `docs/HUMAN_CHECKS.md` (append to the Task 1 section)

**Interfaces:**
- Produces:
  ```rust
  // inbox.rs
  impl Viewed { pub fn version(&self) -> u64 }            // changes only when a viewed `done` changes
  pub fn count(projects: &[Project], sessions: &[Session], viewed: &Viewed) -> usize
  // sidebar.rs
  impl Sidebar {
      pub fn view(&self, viewed: &Viewed) -> Rc<View>     // was: -> View
      pub fn waiting(&self, viewed: &Viewed) -> usize     // the footer's Inbox count
      pub fn session(&self, id: Ulid) -> Option<&Session> // indexed, replaces per-frame `.iter().find`
  }
  // status.rs
  pub fn most_urgent<'a>(ids: &[Ulid], find: impl Fn(Ulid) -> Option<&'a Session>) -> Option<&'a Session>
  // tabs.rs
  pub fn tab_kind<'a>(tab: &Tab, find: impl Fn(Ulid) -> Option<&'a Session>) -> TabKind
  ```
- `View` gains `PartialEq, Eq` (tests compare a cached view with a fresh one).
- How time stays correct: the cache never holds a time. `SessionRow` keeps `state_changed_at`; the draw calls `time_text(now)` every render, and `status::redraw_due` on the 500 ms tick still decides when the Shell notifies. Nothing about time dirties the cache.
- `Sidebar` gains `RefCell` / `Rc` fields, so it is no longer `Sync`; it is only owned by `Shell` (GPUI entities need no `Send`), and the compile check in Step 5 proves nothing sends it across threads.

- [ ] **Step 1: Failing tests**

`inbox.rs` `mod tests`, after `the_status_bar_names_the_inbox_state` (the last test):

```rust
    /// The footer's number is the Inbox's length, whatever the sidebar filter says:
    /// needs input, unseen done; not seen done, working, exited, nor a workspace that is not
    /// listed (archived).
    #[test]
    fn the_count_is_the_inbox_length() {
        let (api, web, archived) = (project("api"), project("web"), project("gone"));
        let seen = session(&api, SessionState::Done, 5);
        let mut viewed = Viewed::default();
        viewed.view(&seen);
        let sessions = vec![
            session(&api, SessionState::NeedsInput, 1),
            session(&web, SessionState::Done, 2),
            seen,
            session(&api, SessionState::Working, 3),
            session(&api, SessionState::Exited, 4),
            session(&archived, SessionState::NeedsInput, 6),
        ];
        let projects = [api, web];
        assert_eq!(count(&projects, &sessions, &viewed), 2);
        assert_eq!(
            count(&projects, &sessions, &viewed),
            entries(&projects, &sessions, &viewed).len()
        );
    }

    /// The sidebar caches on this number: it moves when a `done` becomes viewed (or a later
    /// `done` of the same session is viewed), not when the same view is repeated every frame.
    #[test]
    fn viewing_moves_the_version_only_when_a_viewed_done_changes() {
        let api = project("api");
        let mut done = session(&api, SessionState::Done, 5);
        let mut viewed = Viewed::default();
        let start = viewed.version();
        viewed.view(&session(&api, SessionState::Working, 1));
        assert_eq!(viewed.version(), start, "only a done is recorded");
        viewed.view(&done);
        let first = viewed.version();
        assert_ne!(first, start);
        viewed.view(&done);
        assert_eq!(viewed.version(), first, "the same done again changes nothing");
        done.state_changed_at = 9;
        viewed.view(&done);
        assert_ne!(viewed.version(), first, "a later done is viewed anew");
    }
```

`sidebar/tests.rs`: replace line 1 `use mkrew_protocol::{SessionExitedEvent, SessionStateEvent};` with

```rust
use mkrew_protocol::{
    SessionExitedEvent, SessionPortsEvent, SessionStateEvent, SessionTerminalTitleEvent,
};
```
and add at the end of the file:

```rust
/// Two workspaces (api: a working, b idle; web: c done, titled "login"), all expanded. Rows are
/// newest first: `[api, b, a, web, c]`.
fn cache_fixture() -> (Sidebar, Vec<Project>, Vec<Session>) {
    let (api, web) = (project("api"), project("web"));
    let a = session(&api, 1_000, SessionState::Working);
    let b = session(&api, 2_000, SessionState::Idle);
    let mut c = session(&web, 3_000, SessionState::Done);
    c.title = Some("login".into());
    let mut sidebar = Sidebar::default();
    sidebar.load(
        vec![api.clone(), web.clone()],
        vec![a.clone(), b.clone(), c.clone()],
    );
    (sidebar, vec![api, web], vec![a, b, c])
}

type Mutation = fn(&mut Sidebar, &[Project], &[Session]);

/// Whatever changes what the sidebar shows (every `&mut` path of `Sidebar`, plus the events it
/// applies) is seen on the next `view`: the cached view always equals a fresh compute, and so
/// does the footer's count.
#[test]
fn the_cached_view_equals_a_fresh_one_after_every_kind_of_mutation() {
    // (name, whether the rows or counts visibly change, mutation)
    let cases: &[(&str, bool, Mutation)] = &[
        ("state event", true, |s, _, ss| {
            s.apply(&Event::SessionState(SessionStateEvent {
                session_id: ss[0].id,
                state: SessionState::NeedsInput,
                source: StateSource::Hook,
                changed_at: 9_000,
            }));
        }),
        ("exit", true, |s, _, ss| {
            s.apply(&Event::SessionExited(SessionExitedEvent {
                session_id: ss[0].id,
                exit_code: Some(0),
            }));
        }),
        ("pin a session", true, |s, _, ss| {
            s.apply(&Event::SessionUpdated(Box::new(Session {
                pinned: true,
                ..ss[1].clone()
            })));
        }),
        ("rename by event", true, |s, _, ss| {
            s.apply(&Event::SessionUpdated(Box::new(Session {
                title: Some("renamed".into()),
                ..ss[0].clone()
            })));
        }),
        ("rename by reply", true, |s, _, ss| {
            s.set_title(ss[0].id, Some("fix".into()));
        }),
        ("terminal title", true, |s, _, ss| {
            s.apply(&Event::SessionTerminalTitle(SessionTerminalTitleEvent {
                session_id: ss[0].id,
                title: Some("vim".into()),
            }));
        }),
        ("session color (not drawn in rows)", false, |s, _, ss| {
            s.set_color(ss[0].id, Some("teal".into()));
        }),
        ("pin a workspace", true, |s, ps, _| {
            s.apply(&Event::ProjectUpdated(Project {
                pinned: true,
                ..ps[1].clone()
            }));
        }),
        ("workspace color", true, |s, ps, _| {
            s.apply(&Event::ProjectUpdated(Project {
                color: Some("teal".into()),
                ..ps[0].clone()
            }));
        }),
        ("hide (archive) a workspace", true, |s, ps, _| {
            s.apply(&Event::ProjectUpdated(Project {
                archived: true,
                ..ps[1].clone()
            }));
        }),
        ("collapse one", true, |s, ps, _| s.toggle(ps[0].id)),
        ("collapse all", true, |s, _, _| s.toggle_all()),
        ("chip", true, |s, _, _| s.set_chip(Chip::Working)),
        ("filter text", true, |s, _, _| {
            s.filter_mut().insert("login");
        }),
        ("remove a session", true, |s, _, ss| s.remove_session(ss[0].id)),
        ("add a workspace", true, |s, _, _| s.add_project(project("new"))),
        ("reload", true, |s, ps, ss| {
            s.load(vec![ps[0].clone()], vec![ss[1].clone()]);
        }),
    ];
    let viewed = Viewed::default();
    for (name, changes, mutate) in cases {
        let (mut sidebar, projects, sessions) = cache_fixture();
        let before = sidebar.view(&viewed);
        mutate(&mut sidebar, &projects, &sessions);
        let after = sidebar.view(&viewed);
        assert_eq!(*after, sidebar.build(&viewed), "{name}: stale cached view");
        assert_eq!(*before != *after, *changes, "{name}: visible change");
        assert_eq!(
            sidebar.waiting(&viewed),
            inbox::count(sidebar.projects(), sidebar.sessions(), &viewed),
            "{name}: stale Inbox count"
        );
    }
}

/// A filter or chip change must not serve the old rows; an unchanged sidebar (and events it
/// ignores) reuses the very same view.
#[test]
fn a_filter_or_chip_change_invalidates_the_cache() {
    let (mut sidebar, projects, sessions) = cache_fixture();
    let viewed = Viewed::default();
    let all = sidebar.view(&viewed);
    assert!(Rc::ptr_eq(&all, &sidebar.view(&viewed)), "no change: cache hit");
    // Events the sidebar ignores (ports) leave the cache alone.
    let ignored = Event::SessionPorts(SessionPortsEvent {
        session_id: sessions[0].id,
        ports: vec![3000],
    });
    assert_eq!(sidebar.apply(&ignored), Change::None);
    assert!(Rc::ptr_eq(&all, &sidebar.view(&viewed)), "ignored event: cache hit");

    sidebar.filter_mut().insert("login");
    let narrowed = sidebar.view(&viewed);
    assert!(!Rc::ptr_eq(&all, &narrowed));
    assert_eq!(session_ids(&narrowed.rows), [sessions[2].id]);

    sidebar.clear_filter();
    assert_eq!(session_ids(&sidebar.view(&viewed).rows).len(), 3);
    sidebar.set_chip(Chip::Working);
    assert_eq!(session_ids(&sidebar.view(&viewed).rows), [sessions[0].id]);
    // Revealing a workspace drops the chip and the query too.
    sidebar.filter_mut().insert("zzz");
    sidebar.reveal(projects[1].id);
    assert_eq!(session_ids(&sidebar.view(&viewed).rows).len(), 3);
}

/// `17m` moves with the clock, the cached rows do not: the time is read per draw.
#[test]
fn time_labels_come_from_the_clock_not_from_the_cache() {
    let (sidebar, _, sessions) = cache_fixture();
    let viewed = Viewed::default();
    let first = sidebar.view(&viewed);
    let working = first
        .rows
        .iter()
        .find_map(|row| match row {
            Row::Session(row) if row.session_id == sessions[0].id => Some(row),
            _ => None,
        })
        .expect("the working session's row");
    assert_eq!(working.state, SessionState::Working);
    // `state_changed_at` is 1_000 ms.
    assert_eq!(working.time_text(91_000).as_deref(), Some("1m"));
    assert_eq!(working.time_text(3_901_000).as_deref(), Some("1h 5m"));
    assert!(Rc::ptr_eq(&first, &sidebar.view(&viewed)), "time did not dirty the cache");
}

/// Focusing a `done` pane (`Viewed::view` during render) takes it out of the chip and of the
/// footer count at once: the cache keys on `Viewed`, not only on the sidebar.
#[test]
fn viewing_a_done_session_updates_the_cached_counts() {
    let (sidebar, _, sessions) = cache_fixture();
    let mut viewed = Viewed::default();
    assert_eq!(
        (sidebar.view(&viewed).counts.needs_you, sidebar.waiting(&viewed)),
        (1, 1)
    );
    viewed.view(&sessions[2]);
    assert_eq!(
        (sidebar.view(&viewed).counts.needs_you, sidebar.waiting(&viewed)),
        (0, 0)
    );
}

/// The footer counts the whole Inbox, not the sessions the filter text leaves.
#[test]
fn the_inbox_count_ignores_the_filter_text() {
    let (mut sidebar, _, _) = cache_fixture();
    let viewed = Viewed::default();
    sidebar.filter_mut().insert("zzz");
    assert!(sidebar.view(&viewed).rows.is_empty());
    assert_eq!(sidebar.view(&viewed).counts.needs_you, 0, "the chip follows the filter");
    assert_eq!(sidebar.waiting(&viewed), 1, "the footer does not");
}

/// The id index follows every change of the session list.
#[test]
fn the_session_lookup_follows_the_list() {
    let (mut sidebar, projects, sessions) = cache_fixture();
    let id = sessions[0].id;
    assert_eq!(sidebar.session(id).map(|s| s.state), Some(SessionState::Working));
    sidebar.apply(&Event::SessionExited(SessionExitedEvent {
        session_id: id,
        exit_code: None,
    }));
    assert_eq!(sidebar.session(id).map(|s| s.state), Some(SessionState::Exited));
    sidebar.remove_session(sessions[1].id);
    assert!(sidebar.session(sessions[1].id).is_none());
    assert_eq!(sidebar.session(id).map(|s| s.id), Some(id), "indices shifted");
    sidebar.load(vec![projects[0].clone()], vec![sessions[1].clone()]);
    assert!(sidebar.session(id).is_none());
    assert!(sidebar.session(sessions[1].id).is_some());
}
```

Run: `cargo test -p mkrew-ui --lib -j 4 -- sidebar:: inbox::`
Expected: FAIL to compile (`Viewed::version`, `inbox::count`, `Sidebar::build` / `waiting` / `session`, `View: PartialEq` do not exist).

- [ ] **Step 2: Inbox: `count` and the `Viewed` version**

`inbox.rs`: replace

```rust
pub struct Viewed {
    /// Session → `state_changed_at` of its viewed `done`.
    done: HashMap<Ulid, i64>,
}

impl Viewed {
    /// `session` is on screen and focused.
    pub fn view(&mut self, session: &Session) {
        if session.state == SessionState::Done {
            self.done.insert(session.id, session.state_changed_at);
        }
    }
```
with
```rust
pub struct Viewed {
    /// Session → `state_changed_at` of its viewed `done`.
    done: HashMap<Ulid, i64>,
    /// Counts the changes of `done`; what the sidebar's cached view is keyed on.
    version: u64,
}

impl Viewed {
    /// `session` is on screen and focused. Called every frame: only a new or later `done` is a
    /// change.
    pub fn view(&mut self, session: &Session) {
        if session.state == SessionState::Done
            && self.done.insert(session.id, session.state_changed_at)
                != Some(session.state_changed_at)
        {
            self.version += 1;
        }
    }

    pub fn version(&self) -> u64 {
        self.version
    }
```

After `pub fn entries(..)` (before `/// The session "next Inbox entry"`), add:

```rust
/// `entries(..).len()` without building the entries: the sidebar footer's number.
pub fn count(projects: &[Project], sessions: &[Session], viewed: &Viewed) -> usize {
    sessions
        .iter()
        .filter(|session| needs_you(session, viewed))
        .filter(|session| projects.iter().any(|p| p.id == session.project_id))
        .count()
}
```

- [ ] **Step 3: Sidebar cache, `waiting`, `session`**

`sidebar.rs`, imports: replace `use std::collections::{HashMap, HashSet};` with

```rust
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
```

Replace

```rust
/// What the sidebar draws.
#[derive(Debug)]
pub struct View {
```
with
```rust
/// What the sidebar draws.
#[derive(Debug, PartialEq, Eq)]
pub struct View {
```

In `Sidebar`, replace

```rust
    since_load: Option<Vec<Event>>,
}

impl Sidebar {
    pub fn is_empty(&self) -> bool {
```
with
```rust
    since_load: Option<Vec<Event>>,
    /// Counts every change of what [`Sidebar::view`] and [`Sidebar::session`] read (lists,
    /// expansion, filter, chip); the caches below are valid for one value. Bumped by
    /// [`Sidebar::touch`] in every `&mut` path that changes those inputs.
    rev: u64,
    view_cache: RefCell<Option<CachedView>>,
    /// Session id → position in `sessions`, for `rev`.
    index: RefCell<Option<(u64, HashMap<Ulid, usize>)>>,
}

/// The rows, chip numbers and Inbox count for one `Sidebar::rev` and one `Viewed::version`.
/// Holds no time text: rows keep `state_changed_at` and the draw formats it.
#[derive(Debug)]
struct CachedView {
    rev: u64,
    viewed: u64,
    view: Rc<View>,
    waiting: usize,
}

impl Sidebar {
    /// What [`Sidebar::view`] and [`Sidebar::session`] read changed: drop their caches.
    fn touch(&mut self) {
        self.rev += 1;
    }

    pub fn is_empty(&self) -> bool {
```

Add `self.touch();` as the first statement of `toggle`, `filter_mut`, `set_chip`, `toggle_all`, `clear_filter` and `remove_session`, and as the last statement of `load`:

```rust
    pub fn toggle(&mut self, project_id: Ulid) {
        self.touch();
        if !self.expanded.remove(&project_id) {
```
```rust
    pub fn filter_mut(&mut self) -> &mut Input {
        // The caller edits the text: assume it changed.
        self.touch();
        &mut self.filter
    }
```
```rust
    pub fn set_chip(&mut self, chip: Chip) {
        self.touch();
        self.chip = chip;
    }
```
```rust
    pub fn toggle_all(&mut self) {
        self.touch();
        if self.any_expanded() {
```
```rust
    pub fn clear_filter(&mut self) {
        self.touch();
        self.filter.clear();
        self.chip = Chip::All;
    }
```
```rust
    pub fn remove_session(&mut self, session_id: Ulid) {
        self.touch();
        self.sessions.retain(|session| session.id != session_id);
    }
```
and in `load`, replace

```rust
                .map(|session| session.project_id)
                .collect();
        }
    }

    /// A workspace the user just added: shown and expanded.
```
with
```rust
                .map(|session| session.project_id)
                .collect();
        }
        self.touch();
    }

    /// A workspace the user just added: shown and expanded.
```
(`reveal` and `add_project` reach `clear_filter` / `upsert_project`, which touch; `set_title` / `set_color` / `apply` reach `update_session`; `sort_projects` runs only inside `load` / `upsert_project`.)

Replace `update_session` and the head of `upsert_project`:

```rust
    fn update_session(&mut self, session_id: Ulid, update: impl FnOnce(&mut Session)) -> Change {
        match self.sessions.iter_mut().find(|s| s.id == session_id) {
            Some(session) => {
                update(session);
                Change::Redraw
            }
            None => Change::Refresh,
        }
    }

    fn upsert_project(&mut self, project: Project) {
        let existing = self.projects.iter().position(|p| p.id == project.id);
```
with
```rust
    fn update_session(&mut self, session_id: Ulid, update: impl FnOnce(&mut Session)) -> Change {
        let Some(session) = self.sessions.iter_mut().find(|s| s.id == session_id) else {
            return Change::Refresh;
        };
        update(session);
        self.touch();
        Change::Redraw
    }

    fn upsert_project(&mut self, project: Project) {
        self.touch();
        let existing = self.projects.iter().position(|p| p.id == project.id);
```

Replace the head of `view` and the whole of `rows` (from `/// Rows to show and the chip numbers, in one pass …` through `pub fn rows`) so the old `view` body becomes `build`:

```rust
    /// Rows to show and the chip numbers, cached until the sidebar or `viewed` changes: every
    /// `SidebarView` render (each Shell notify, including the 500 ms working tick) shares one
    /// compute. Each workspace, then (if open, or while a filter is active) its live sessions
    /// that pass the filter and the chip. While filtering, workspaces without such a session
    /// are left out.
    pub fn view(&self, viewed: &Viewed) -> Rc<View> {
        self.cached(viewed).0
    }

    /// The footer's Inbox count: [`inbox::count`], cached with the view. Not narrowed by the
    /// filter text or the chip.
    pub fn waiting(&self, viewed: &Viewed) -> usize {
        self.cached(viewed).1
    }

    fn cached(&self, viewed: &Viewed) -> (Rc<View>, usize) {
        let mut cache = self.view_cache.borrow_mut();
        if let Some(hit) = cache
            .as_ref()
            .filter(|hit| hit.rev == self.rev && hit.viewed == viewed.version())
        {
            return (Rc::clone(&hit.view), hit.waiting);
        }
        let view = Rc::new(self.build(viewed));
        let waiting = inbox::count(&self.projects, &self.sessions, viewed);
        *cache = Some(CachedView {
            rev: self.rev,
            viewed: viewed.version(),
            view: Rc::clone(&view),
            waiting,
        });
        (view, waiting)
    }

    /// The session `id` (live or exited), found through an index that is rebuilt after any
    /// change of the list: O(1) per frame instead of a scan per pane.
    pub fn session(&self, id: Ulid) -> Option<&Session> {
        let mut index = self.index.borrow_mut();
        if index.as_ref().is_none_or(|(rev, _)| *rev != self.rev) {
            let map = self
                .sessions
                .iter()
                .enumerate()
                .map(|(at, session)| (session.id, at))
                .collect();
            *index = Some((self.rev, map));
        }
        let at = *index.as_ref()?.1.get(&id)?;
        self.sessions.get(at)
    }

    /// [`Sidebar::view`]'s rows with no `done` session counted as viewed: what the tests and the
    /// pane-lifecycle check read (only the `Needs you` chip looks at `viewed`). Not cached.
    pub fn rows(&self) -> Vec<Row> {
        self.build(&Viewed::default()).rows
    }

    /// One uncached pass over the sessions (O(sessions) plus the newest-first sort inside each
    /// workspace).
    fn build(&self, viewed: &Viewed) -> View {
        let filtering = self.filtering();
        // ... the body of the old `view` fn, unchanged (`let mut counts = Counts::default();`
        // down to `View { counts, rows }`) ...
    }
```
For `build`, keep the old `view` body exactly, only moved. Delete the old `pub fn view` header, its doc comment and the old `rows` fn.

Run: `cargo test -p mkrew-ui --lib -j 4 -- sidebar:: inbox::`
Expected: PASS (the new tests and all existing sidebar / inbox tests).

- [ ] **Step 4: Tab bar lookups**

`status.rs`: replace

```rust
pub fn most_urgent<'a>(sessions: &'a [Session], ids: &[Ulid]) -> Option<&'a Session> {
    ids.iter()
        .filter_map(|id| sessions.iter().find(|s| s.id == *id))
        .rev()
        .max_by_key(|s| urgency(s.state))
}
```
with
```rust
pub fn most_urgent<'a>(
    ids: &[Ulid],
    find: impl Fn(Ulid) -> Option<&'a Session>,
) -> Option<&'a Session> {
    ids.iter()
        .filter_map(|&id| find(id))
        .rev()
        .max_by_key(|s| urgency(s.state))
}
```
In its test `tab_shows_the_most_urgent_state` replace `most_urgent(&sessions, &ids).map(|s| s.state)` with
```rust
            most_urgent(&ids, |id| sessions.iter().find(|s| s.id == id)).map(|s| s.state)
```
and replace
```rust
        assert_eq!(
            most_urgent(&[a.clone(), b.clone()], &[b.id, a.id]),
            Some(&b)
        );
```
with
```rust
        let pair = [a.clone(), b.clone()];
        assert_eq!(
            most_urgent(&[b.id, a.id], |id| pair.iter().find(|s| s.id == id)),
            Some(&b)
        );
```

`tabs.rs` (:530): replace

```rust
pub fn tab_kind(tab: &Tab, sessions: &[Session]) -> TabKind {
    let focused = tab
        .focused()
        .and_then(|id| sessions.iter().find(|s| s.id == id));
    match focused {
```
with
```rust
pub fn tab_kind<'a>(tab: &Tab, find: impl Fn(Ulid) -> Option<&'a Session>) -> TabKind {
    match tab.focused().and_then(find) {
```
`tabs/tests.rs`, `a_tab_shows_the_kind_of_its_focused_pane` (:599): after `let sessions = [agent, shell];` add `let find = |id: Ulid| sessions.iter().find(|s| s.id == id);` and replace the four calls: `tab_kind(tabs.active().unwrap(), &sessions)` → `tab_kind(tabs.active().unwrap(), find)` (twice: the `Shell` one and the `Agent("fake")` one), `tab_kind(tabs.active().unwrap(), &[])` → `tab_kind(tabs.active().unwrap(), |_| None)`, and `tab_kind(empty.active().unwrap(), &sessions)` → `tab_kind(empty.active().unwrap(), find)`.

Run: `cargo test -p mkrew-ui --lib -j 4 -- status:: tabs::`
Expected: PASS (unchanged behavior, new call shapes).

- [ ] **Step 5: Wire the shell**

`shell/sidebar.rs`:
- `render_sidebar` keeps `let view = self.sidebar.view(&self.viewed);` (:238, now an `Rc<View>`; the `view.counts.*` reads work). Replace `for row in view.rows {` (:518) with

```rust
            // The cached rows are shared; the row builders take owned rows (their text moves
            // into the elements), so each render still clones the visible rows, but not the
            // grouping, sort, lowercase, summaries and count.
            for row in view.rows.iter().cloned() {
```
- `render_compact_sidebar`: replace `for row in self.sidebar.view(&self.viewed).rows {` (:680) with

```rust
        let view = self.sidebar.view(&self.viewed);
        for row in view.rows.iter().cloned() {
```
- `render_sidebar_footer`: replace `let waiting = self.inbox_entries().len();` (:735) with `let waiting = self.sidebar.waiting(&self.viewed);`. Keep `inbox_entries()` for `jump_next_inbox` and the peek cycle (user actions, not per frame).

`shell/pane_render.rs`, `render_tab_bar`: replace

```rust
                let urgent = status::most_urgent(self.sidebar.sessions(), &tab.sessions());
```
with
```rust
                let urgent = status::most_urgent(&tab.sessions(), |id| self.sidebar.session(id));
```
and `let icon = match tab_kind(tab, self.sidebar.sessions()) {` with `let icon = match tab_kind(tab, |id| self.sidebar.session(id)) {`.

Per-frame single lookups: replace each of
- `pane_render.rs:23` (`render_tab_bar`, `single`): `.and_then(|id| self.sidebar.sessions().iter().find(|s| s.id == id));` → `.and_then(|id| self.sidebar.session(id));`
- `pane_render.rs:428` (`pane_frame`): `let info = self.sidebar.sessions().iter().find(|s| s.id == session);` → `let info = self.sidebar.session(session);`
- `shell/inspector.rs:99` (`render_inspector`): `.and_then(|id| self.sidebar.sessions().iter().find(|s| s.id == id));` → `.and_then(|id| self.sidebar.session(id));`
- `main.rs:618` (`Shell::render`): `&& let Some(session) = self.sidebar.sessions().iter().find(|s| s.id == id)` → `&& let Some(session) = self.sidebar.session(id)`

Check nothing else used the old shapes: `grep -n "most_urgent(\|tab_kind(" crates/mkrew-ui/src --include=*.rs -r` must show only the definitions, the two lines above and the tests. `shell/sidebar_view.rs` stays unchanged.

Compile check: `cargo check -p mkrew-ui --all-targets -j 4`
Expected: no errors (`tests/lifecycle.rs` still uses `sidebar.rows()`).

- [ ] **Step 6: HUMAN_CHECKS**

Append to the end of the `### To verify — 2026-10-13 perf (...)` section in `docs/HUMAN_CHECKS.md`:

```markdown
- [ ] **Sidebar and Inbox count are current (Task 2):** with several workspaces and agents, in the full sidebar and in the compact rail:
  - A session going `needs input` / `done` (also in another workspace, collapsed) changes the footer `N need you` and the `Needs you` chip at once; focusing a `done` pane takes it out of both at once.
  - Pin / rename / change color of a session or a workspace, hide (archive) a workspace, collapse and expand, `Collapse all`: the rows change immediately.
  - Type in the filter: rows narrow per keystroke, the chips narrow with it, but the footer `N need you` does not change. Clear the filter: all rows are back.
  - The `17m` times keep advancing (working and idle rows, once per minute after the first minute) without any other change.
  - Several tabs with many panes: each tab's status dot and icon (agent badge / `>_`) follow its panes.
```

- [ ] **Step 7: Verify and commit**

Run: `cargo test -p mkrew-ui --lib -j 4 -- sidebar:: inbox:: status:: tabs:: redraw::` (expect PASS), then `cargo test -p mkrew-ui --test lifecycle -j 2` (expect PASS), then the full verification: `sh scripts/verify.sh` (fallback if rustc crashes: `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace -j 4`). Do not tick the inbox line (Tasks 3–6 follow).

Commit (never push; stage only this task's hunks of `docs/HUMAN_CHECKS.md`, see Task 1):
```
git add crates/mkrew-ui/src/inbox.rs crates/mkrew-ui/src/sidebar.rs crates/mkrew-ui/src/sidebar/tests.rs crates/mkrew-ui/src/status.rs crates/mkrew-ui/src/tabs.rs crates/mkrew-ui/src/tabs/tests.rs crates/mkrew-ui/src/shell/sidebar.rs crates/mkrew-ui/src/shell/pane_render.rs crates/mkrew-ui/src/shell/inspector.rs crates/mkrew-ui/src/main.rs docs/HUMAN_CHECKS.md
git commit -m "perf(ui): cache the sidebar rows, the Inbox count and session lookups until their inputs change"
```

---

### Task 3: Docs is not re-read while it is hidden

> **Refresh 2026-10-13:** still needed, design unchanged. Docs is off by default (`[features] docs`, ADR 0046), so this only matters for users who switched it on, or who switched it off again after opening it (the per-workspace `Docs` stays in `Shell::docs`).
> - `doc_event` moved to `shell/docs.rs:157-178`, `show_view` / `activate_workspace` to `shell/workspace.rs:343` / `:21`; `docs.rs` (`Docs::apply` :232, `select` :192, `read` :208) is unchanged since the old plan.
> - `Shell::view()` (`shell/workspace.rs:333`) reads Docs as Terminals while `features.docs` is off, so `shown` in `doc_event` is already `None` then: the file is only marked stale.
> - New edge the old plan did not have: `[features] docs` switched on by a settings reload while the workspace's stored view is Docs makes `view()` flip to Docs without `show_view`. Task 6 calls `show_docs` there (it needs this task's `show_docs`; do Task 3 before Task 6).

Inbox: line 115 `11. Perf: stop whole-app re-renders in the UI` (bullet B3; do not tick yet). Spec: §12 (SPEC.md:924, re-anchoring on `doc.changed`), §7 event `doc.changed` (SPEC.md:574), ADR 0046.

**Current state (investigated):**
- `Shell::doc_event` (`shell/docs.rs:157`) runs for every `doc.changed` / `comment.updated` (called from `shell/connection.rs:670-671`). It calls `docs.apply(event)` on EVERY workspace's `Docs` (`self.docs.values_mut()`, :164-168), and for each `Reaction { reread: Some(file) }` it calls `read_doc` at once (:170-173), whatever view is shown (`shown`, :163, only gates `relist` at :174). `read_doc` (:115) sends `doc.read`, then `Shown::new(&content)` on the background executor (markdown parse + syntect + mermaid ×2). An agent that writes a plan every few seconds costs a full parse per write in a view nobody looks at.
- `docs.rs` `Docs::apply` (:232) decides `reread` and already sets `self.doc = Doc::Loading` when the shown file is not the announced hash (:246-251). `Docs::select` (:192) resets `changed_to`. `Docs::read` (:208) drops an answer whose hash differs from `changed_to`, so a late old read is ignored; one later read of the newest hash is enough.
- The pattern in Files/Git: `files_event` (`shell/files.rs:211`) returns early unless `self.view() == View::Files`; hidden views are refreshed by `show_view` (`shell/workspace.rs:361-375`) and `activate_workspace` (:42-50). Docs does it for the LIST (`load_docs` in both, and `doc_event` relists only `if reaction.relist && shown == Some(project_id)`). Only the re-read of the selected file is missing.
- Other callers of `show_view(View::Docs)`: `shell/git.rs:811-812` (Review in Docs: `select_doc` first, then `show_view`; `select` clears the stale mark, so `show_docs` finds nothing) and `shell/sessions.rs:348` (`Command::ShowDocs`).
- Tests in `docs.rs` `mod tests` (helpers `changed(project, path, Some(hash))` :385, `shown(path, text, hash)` :352, `shown_hash(&docs)` :393) call `docs.apply(&event)` at lines 491, 499, 503, 506, 522, 552, 553, 555.

**Files:**
- Modify: `crates/mkrew-ui/src/docs.rs` (`Docs` field `stale`, `new`, `select`, `apply`, new `take_stale`, tests)
- Modify: `crates/mkrew-ui/src/shell/docs.rs` (`doc_event`, new `show_docs`)
- Modify: `crates/mkrew-ui/src/shell/workspace.rs` (`show_view`, `activate_workspace`)
- Modify: `docs/HUMAN_CHECKS.md` (append to the shared perf section)

**Interfaces:**
- Changes: `Docs::apply(&mut self, event: &Event, visible: bool) -> Reaction` (`visible` = this workspace's Docs is on screen). With `visible == false` a re-read is never returned; the file is remembered instead.
- Produces: `Docs::take_stale(&mut self) -> Option<String>` (the selected file to read now that Docs is shown again; `None` after it was taken).
- Produces: `Shell::show_docs(&mut self, project_id: Ulid, window: &mut Window, cx: &mut Context<'_, Self>)` = `load_docs` + the stale re-read (`pub(crate)`, Task 6 calls it).

- [ ] **Step 1: Failing tests**

In `docs.rs` `mod tests`, change the existing calls to the new signature (they all test a shown Docs): add `, true` as a second argument of `docs.apply(…)` at lines 491, 499, 503, 506 (`Ulid::new()`), 522 (`None`), 552 (`&comment(project)`), 553 (`&comment(Ulid::new())`), 555 (`&event(project, CommentContext::Diff)`), e.g. `docs.apply(&changed(project, "a.md", Some("h1")), true)`. Then add at the end of the module (before the closing `}`):

```rust
    /// Hidden Docs: a change only marks the file stale (a read parses, highlights and draws
    /// diagrams); showing Docs again reads it once, however many changes came.
    #[test]
    fn a_change_while_docs_is_hidden_is_read_once_when_it_shows() {
        let project = Ulid::new();
        let mut docs = Docs::new(project);
        docs.select("a.md");
        docs.read("a.md", Ok(shown("a.md", "a", "h1")));
        for hash in ["h2", "h3", "h4"] {
            let hidden = docs.apply(&changed(project, "a.md", Some(hash)), false);
            assert_eq!((hidden.relist, hidden.reread), (true, None), "{hash}");
        }
        assert_eq!(docs.doc, Doc::Loading);
        assert_eq!(docs.take_stale().as_deref(), Some("a.md"));
        assert_eq!(docs.take_stale(), None, "asked once");
        // A read sent before the last change answers late: dropped; the newest one shows.
        docs.read("a.md", Ok(shown("a.md", "a2", "h2")));
        assert_eq!(docs.doc, Doc::Loading);
        docs.read("a.md", Ok(shown("a.md", "a4", "h4")));
        assert_eq!(shown_hash(&docs), Some("h4"));
    }

    /// Nothing is stale when there is nothing to read later: the same hash, another file,
    /// a deleted file, or another file picked meanwhile (its own read replaces the mark).
    #[test]
    fn only_a_changed_selected_file_is_stale() {
        let project = Ulid::new();
        let mut docs = Docs::new(project);
        docs.select("a.md");
        docs.read("a.md", Ok(shown("a.md", "a", "h1")));
        docs.apply(&changed(project, "a.md", Some("h1")), false);
        docs.apply(&changed(project, "b.md", Some("x")), false);
        assert_eq!(docs.take_stale(), None, "same hash, other file");
        docs.apply(&changed(project, "a.md", Some("h2")), false);
        assert_eq!(docs.select("b.md").as_deref(), Some("b.md"));
        assert_eq!(docs.take_stale(), None, "another file got selected");
        docs.select("a.md");
        docs.read("a.md", Ok(shown("a.md", "a", "h1")));
        docs.apply(&changed(project, "a.md", Some("h5")), false);
        let gone = docs.apply(&changed(project, "a.md", None), false);
        assert_eq!((gone.relist, gone.reread), (true, None));
        assert_eq!(docs.doc, Doc::Deleted);
        assert_eq!(docs.take_stale(), None, "a deleted file is not read");
    }

    /// A change that comes while Docs is shown reads at once and replaces an older mark.
    #[test]
    fn a_change_while_shown_reads_at_once_and_clears_the_mark() {
        let project = Ulid::new();
        let mut docs = Docs::new(project);
        docs.select("a.md");
        docs.read("a.md", Ok(shown("a.md", "a", "h1")));
        docs.apply(&changed(project, "a.md", Some("h2")), false);
        let shown_now = docs.apply(&changed(project, "a.md", Some("h3")), true);
        assert_eq!(shown_now.reread.as_deref(), Some("a.md"));
        assert_eq!(docs.take_stale(), None);
    }
```

Run: `cargo test -p mkrew-ui --lib docs::tests -j 4`
Expected: FAIL to compile (`apply` takes one argument, `take_stale` does not exist).

- [ ] **Step 2: Logic in `docs.rs`**

`Docs`, after the `changed_to` field (end of the struct, :136):

```rust
    /// A `doc.changed` of the selected file came while Docs was hidden: it is read when Docs
    /// is shown again ([`Self::take_stale`]).
    stale: Option<String>,
```
`Docs::new`, after `changed_to: None,` (:153): `stale: None,`

`Docs::select`, replace
```rust
        self.doc = Doc::Loading;
        self.changed_to = None;
        Some(file_path.to_owned())
```
with
```rust
        self.doc = Doc::Loading;
        self.changed_to = None;
        self.stale = None;
        Some(file_path.to_owned())
```

`Docs::apply`: replace the signature line
```rust
    pub fn apply(&mut self, event: &Event) -> Reaction {
```
with
```rust
    pub fn apply(&mut self, event: &Event, visible: bool) -> Reaction {
```
and replace
```rust
                self.changed_to.clone_from(&changed.hash);
                match &changed.hash {
                    None => self.doc = Doc::Deleted,
                    Some(hash) => {
                        let current = matches!(&self.doc, Doc::Shown(shown) if shown.hash == *hash);
                        // A read in flight may predate the change: read again after it.
                        if !current {
                            self.doc = Doc::Loading;
                            reaction.reread = Some(changed.file_path.clone());
                        }
                    }
                }
```
with
```rust
                self.changed_to.clone_from(&changed.hash);
                self.stale = None;
                match &changed.hash {
                    None => self.doc = Doc::Deleted,
                    Some(hash) => {
                        let current = matches!(&self.doc, Doc::Shown(shown) if shown.hash == *hash);
                        // A read in flight may predate the change: read again after it. While
                        // Docs is hidden nobody sees the page, so it is only remembered.
                        if !current {
                            self.doc = Doc::Loading;
                            if visible {
                                reaction.reread = Some(changed.file_path.clone());
                            } else {
                                self.stale = Some(changed.file_path.clone());
                            }
                        }
                    }
                }
```
Add after `apply`:
```rust
    /// The selected file to read now that Docs is shown again, when it changed while hidden.
    pub fn take_stale(&mut self) -> Option<String> {
        self.stale.take()
    }
```
Module doc (lines 3-4 of `docs.rs`): `` `doc.changed` re-reads the shown file and re-lists; `comment.updated` `` → `` `doc.changed` re-reads the shown file (when Docs is hidden: when it is shown again) and re-lists; `comment.updated` `` (re-wrap the comment lines).

Run: `cargo test -p mkrew-ui --lib docs::tests -j 4`
Expected: PASS (the three new tests and the old ones with `true`).

- [ ] **Step 3: Shell**

`shell/docs.rs` `doc_event`, replace
```rust
            .map(|docs| (docs.project_id, docs.apply(event)))
            .collect();
```
with
```rust
            .map(|docs| (docs.project_id, docs.apply(event, shown == Some(docs.project_id))))
            .collect();
```
Update its doc comment (:155-156): `re-reads a shown file that changed and re-lists the Docs view on screen (hidden ones re-list when shown again)` → `re-reads a changed file and re-lists when its Docs is on screen (a hidden one does both when shown again, \`show_docs\`)`.

Add after `doc_event` (before `open_doc_in_zed`):
```rust
    /// Docs of `project_id` came on screen: re-lists its files and reads the selected one
    /// again when it changed while Docs was hidden.
    pub(crate) fn show_docs(
        &mut self,
        project_id: Ulid,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        self.load_docs(project_id, window, cx);
        let stale = self.docs.get_mut(&project_id).and_then(Docs::take_stale);
        if let Some(file_path) = stale {
            self.read_doc(project_id, file_path, window, cx);
        }
    }
```
`shell/workspace.rs` `show_view` (:361-363), replace
```rust
        if view == View::Docs && shown != View::Docs {
            self.load_docs(project_id, window, cx);
        }
```
with
```rust
        if view == View::Docs && shown != View::Docs {
            self.show_docs(project_id, window, cx);
        }
```
`activate_workspace` (:42-44), replace
```rust
        if switched && self.view() == View::Docs {
            self.load_docs(project_id, window, cx);
        }
```
with
```rust
        if switched && self.view() == View::Docs {
            self.show_docs(project_id, window, cx);
        }
```
(`load_docs` keeps its other callers: the `doc_event` relist, its own `again`, the Retry button in `shell/docs.rs`.)

Compile check: `cargo check -p mkrew-ui -j 4`
Expected: no errors.

- [ ] **Step 4: HUMAN_CHECKS**

Append to the end of the section `### To verify — 2026-10-13 perf (plan docs/superpowers/plans/2026-10-10-inbox-perf.md)` in `docs/HUMAN_CHECKS.md` (created by Task 1):

```markdown
- [ ] **Docs while hidden (B3):** set `[features] docs = true` in `settings.toml`. Open a workspace's Docs on a plan file, switch to Terminals. Let an agent (or Zed) save that file several times → `mkrew-ui` CPU stays flat in Task Manager. Back to Docs → the page shows the latest content (a brief "Loading…" is fine) and comments still work. Also: change the file while another workspace is shown, then switch to the workspace and open Docs → latest content. A file that was deleted meanwhile shows the deleted notice.
```

- [ ] **Step 5: Verify and commit**

Run: `cargo test -p mkrew-ui --lib docs -j 4` (expect PASS), then `sh scripts/verify.sh` (fallback if rustc crashes: `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace -j 4`). Do not tick inbox line 115 (Task 6 does).
Commit: `git add crates/mkrew-ui/src/docs.rs crates/mkrew-ui/src/shell/docs.rs crates/mkrew-ui/src/shell/workspace.rs` (plus the HUMAN_CHECKS hunk with `git add -p docs/HUMAN_CHECKS.md`, never the whole file if it holds others' edits), then `git commit -m "perf(ui): Docs reads a changed file only when it is shown"`. Never push.

---

### Task 4: Markdown live preview in Files is debounced 300 ms — DROPPED

> **Refresh 2026-10-13:** already done in substance by 42f3969 and the background build in `shell/files.rs`; a 300 ms time debounce no longer buys anything and makes one thing worse. **No steps, no files, no commit.** The old `PreviewStep` / `preview_wait` / `PREVIEW_DELAY` design is not carried over.
> - Inbox item 11 bullet A6 (docs/INBOX.md:123) says "300 ms debounce": when ticking item 11 in Task 6, reword it to "A6: dropped, the preview already builds off the UI thread, once per text" (Task 6 Step 6).

Inbox: line 115 (bullet A6). Spec: §12A (SPEC.md:954-958), ADR 0021.

**Evidence (why a time debounce is not worth it now):**
- **When a build can start.** `FilesView::preview_request` (`files_view.rs:627`) returns a text only if `previewing()` (:598: Preview mode AND `.md` AND a text) and the text moved on, once per stamp (`preview_asked`). Callers of `Shell::build_preview` (`shell/files.rs:181`): the end of `read_file` (:171) and the Preview/Source toggle (`shell/files_render.rs:301`). Nothing builds per keystroke: while Source is shown `previewing()` is false, and in Preview the editor is not editable. So a burst of typing never parses; the toggle back to Preview parses once (the old page stays on screen meanwhile, because `preview()` :618 only drops a page of another `text_id`, and edits keep the `text_id`).
- **Where the cost lands.** The parse (`Shown::from_text`, syntect + mermaid ×2) runs on `cx.background_executor()` (`shell/files.rs:194-198`); the UI thread only stores the page (`preview_ready` :642, `shell/files.rs:199-205`) and the page is a virtual list (`preview.rs:119`, 42f3969). One request is in flight at a time; `build_preview` asks again after it answers only if the text moved on (stamp/`preview_asked`), and `preview_ready` drops a page older than the stored one (:643). That is the coalescing a debounce would add.
- **Re-reads already skip equal text.** `FilesView::show` (`files_view.rs:439-469`): a re-read with the same hash keeps the text (no new `text_id`, no rebuild); unsaved text is never replaced. Only a real change on disk (an agent saving the open `.md` while Files is on screen, via `files_event` → `read_file`, rate-limited by the daemon's `git.changed` batching, SPEC.md:577) gives a new `text_id` and one build.
- **A debounce would hurt.** After such a re-read the text has a new `text_id`, so `preview()` is `None` and the pane shows "Rendering the preview…" (`shell/docs.rs:620-621`). Waiting 300 ms first lengthens that flash on every agent save. The remaining cost (one background parse per distinct saved text, at most back to back) is bounded by parse time and off the UI thread.
- **Not measured.** No CPU number exists for repeated agent saves of a big `.md` in Files Preview. If the HUMAN_CHECKS CPU check in Task 6 shows `mkrew-ui` busy in that case, the cheaper fix is in `diagram.rs` (PERF_REVIEW A5: render the current theme only, cache per block hash), not a timer here. Optional follow-up (not in this plan): keep the previous page on screen while the new one builds, to remove the flash.

---

### Task 5: Title/status bar git summary refresh is throttled to once per 2 s

> **Refresh 2026-10-13:** still needed, design unchanged (pure `Throttle` in `git_summary.rs`). The busy/again coalescing in `load_git_summary` (`shell/git.rs:156-199`) is what the old plan already assumed; it does not rate limit.
> - Code moved: event loop now `shell/connection.rs:679-688`, `Shell` fields `main.rs:499-504`, field init `shell/connection.rs:362-364`, `activate_workspace` `shell/workspace.rs:36-38`, `load_git_summary` `shell/git.rs:156`; `git_summary.rs` is unchanged apart from tests.
> - The summary also feeds the Git view's dirty marker (`shell/git_render.rs:647-650`), the repo chooser (`shell/git_menus.rs:139,168`) and the inspector "Changes" card (`shell/inspector.rs:182`); they now lag up to 2 s behind `git.changed` (the Git view's own list still refreshes at once via `git_event`). HUMAN_CHECKS says so.
> - `Shell` fields are in `main.rs`, the `Shell::new` initialiser in `shell/connection.rs`; imports come through `use crate::*` in the shell modules, so only `main.rs:63` changes.

Inbox: line 115 `11. Perf: stop whole-app re-renders in the UI` (bullet B4; do not tick yet). Spec: §10 (SPEC.md:746-750, title bar git chip and `Git N` badge), §7 event `git.changed` (SPEC.md:577).

**Current state (investigated):**
- `shell/connection.rs:679-688`: for every `Event::GitChanged` it runs `files_event`, `git_event`, and when `changed.worktree_task_id.is_none() && self.workspace == Some(changed.project_id)` calls `self.load_git_summary(changed.project_id, window, cx)` (:686), whatever view is shown.
- `Shell::load_git_summary` (`shell/git.rs:156`) is the single-flight loader: `git_summary_busy` / `git_summary_again`; its request is `git_summary_of` (`shell/git.rs:819`): `git.repos` + one `git.status` per repository. A change that arrives while one runs sets `git_summary_again` and reloads once when it answers; the UI is notified only if the summary differs (:179-186). It is not rate limited: with `git.changed` every ~300 ms (SPEC.md:577) a busy agent keeps it running back to back (a git process per repo each time).
- Other callers of `load_git_summary`: `activate_workspace` (`shell/workspace.rs:36-38`, `switched || self.git_summary.is_none()`) and its own `shown != project_id` retry (`shell/git.rs:191-195`). Fields `git_summary`, `git_summary_busy`, `git_summary_again` at `main.rs:501-504`, initialised at `shell/connection.rs:362-364`; `git_summary = None` on reconnect (`shell/connection.rs:245`) and on a workspace switch (`shell/workspace.rs:34`), both followed by a load through `activate_workspace`.
- `git_summary.rs` is the pure module (`RepoSummary`, `GitSummary`, `mod tests`), imported in `main.rs:63` as `use mkrew_ui::git_summary::{GitSummary, RepoSummary};`. The throttle goes there so it is tested headlessly. `status::now_ms() -> i64` (Unix ms) is the clock the shell uses (`use std::time::{Duration, Instant}` is at `main.rs:13`).

**Files:**
- Modify: `crates/mkrew-ui/src/git_summary.rs` (`REFRESH_EVERY_MS`, `Due`, `Throttle`, tests)
- Modify: `crates/mkrew-ui/src/main.rs` (import :63, field after :504)
- Modify: `crates/mkrew-ui/src/shell/connection.rs` (field init after :364, event loop :686)
- Modify: `crates/mkrew-ui/src/shell/workspace.rs` (`activate_workspace`)
- Modify: `crates/mkrew-ui/src/shell/git.rs` (new `git_summary_due`, doc of `load_git_summary`)
- Modify: `docs/HUMAN_CHECKS.md` (append)

**Interfaces:**
- Produces: `git_summary::REFRESH_EVERY_MS: i64 = 2_000`.
- Produces: `git_summary::Due { Now, After(i64), Skip }`.
- Produces: `git_summary::Throttle { new via Default }` with `changed(&mut self, now_ms: i64) -> Due` (a `git.changed` arrived), `timer(&mut self, now_ms: i64) -> Due` (the trailing timer fired), `refreshed(&mut self, now_ms: i64)` (a refresh was started by something else: starts a new window and drops a pending trailing one).
- Produces: `Shell::git_summary_due(&mut self, project_id: Ulid, due: Due, window, cx)`.
- Consumes: `Shell::load_git_summary` unchanged.

Rules: first change after `refreshed` or after a quiet 2 s: `Now`. Changes inside the 2 s window: one `After(remaining)` for the first (the trailing timer), `Skip` for the rest, so the last change always ends with one refresh at the window end. `timer` at the window end: `Now` (and a new window opens), `After(rest)` if it fired early, `Skip` if nothing is pending. A workspace switch calls `refreshed` and loads at once. A clock that steps back opens a new window (no frozen refresh).

- [ ] **Step 1: Failing tests**

In `git_summary.rs` `mod tests` (end of the module), add:

```rust
    /// The first change refreshes at once; a burst inside 2 s gets exactly one trailing
    /// refresh at the end of the window, so the last change always shows.
    #[test]
    fn a_burst_gets_one_immediate_and_one_trailing_refresh() {
        let mut throttle = Throttle::default();
        assert_eq!(throttle.changed(10_000), Due::Now);
        assert_eq!(throttle.changed(10_100), Due::After(1_900));
        assert_eq!(throttle.changed(10_500), Due::Skip, "the timer is already set");
        assert_eq!(throttle.changed(11_999), Due::Skip);
        assert_eq!(throttle.timer(11_999), Due::After(1), "fired a ms early");
        assert_eq!(throttle.timer(12_000), Due::Now);
        assert_eq!(throttle.timer(12_001), Due::Skip, "nothing pending");
        // A change after that opens the next window from the trailing refresh.
        assert_eq!(throttle.changed(12_500), Due::After(1_500));
    }

    /// Boundaries: exactly 2 s after a refresh is free, 1 ms earlier waits 1 ms; a quiet gap
    /// longer than 2 s refreshes at once.
    #[test]
    fn the_window_is_exactly_two_seconds() {
        let mut throttle = Throttle::default();
        assert_eq!(throttle.changed(0), Due::Now);
        assert_eq!(throttle.changed(2_000), Due::Now);
        assert_eq!(throttle.changed(3_999), Due::After(1));
        assert_eq!(throttle.timer(4_000), Due::Now);
        assert_eq!(throttle.changed(60_000), Due::Now);
        assert_eq!(REFRESH_EVERY_MS, 2_000);
    }

    /// A workspace switch refreshes at once (the caller loads) and its window replaces the
    /// old one: the old trailing timer has nothing left to do, and the next change waits for
    /// the new window.
    #[test]
    fn a_switch_refreshes_at_once_and_restarts_the_window() {
        let mut throttle = Throttle::default();
        assert_eq!(throttle.changed(0), Due::Now);
        assert_eq!(throttle.changed(500), Due::After(1_500));
        throttle.refreshed(1_000);
        assert_eq!(throttle.timer(2_000), Due::Skip, "the switch refreshed already");
        assert_eq!(throttle.changed(1_500), Due::After(1_500), "2 s after the switch, not 0");
        assert_eq!(throttle.timer(2_000), Due::After(1_000), "an old timer asks again");
        assert_eq!(throttle.timer(3_000), Due::Now);
    }

    /// A clock that stepped back (NTP, manual change) must not freeze the refresh for hours.
    #[test]
    fn a_clock_that_stepped_back_starts_a_new_window() {
        let mut throttle = Throttle::default();
        assert_eq!(throttle.changed(10_000_000), Due::Now);
        assert_eq!(throttle.changed(5_000), Due::After(REFRESH_EVERY_MS));
        assert_eq!(throttle.timer(7_000), Due::Now);
    }
```

Run: `cargo test -p mkrew-ui --lib git_summary::tests -j 4`
Expected: FAIL to compile (`Throttle`, `Due`, `REFRESH_EVERY_MS` do not exist).

- [ ] **Step 2: Logic in `git_summary.rs`**

After the `impl GitSummary { … }` block (ends at :100) and before `#[cfg(test)]` (:102) add:

```rust
/// The least time between two refreshes of the title bar git summary (perf B4).
pub const REFRESH_EVERY_MS: i64 = 2_000;

/// What to do about a `git.changed` (or the timer it set).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Due {
    /// Refresh now.
    Now,
    /// Call [`Throttle::timer`] again in this many ms (the trailing refresh).
    After(i64),
    /// Nothing: a timer is already set, or nothing is pending.
    Skip,
}

/// Trailing throttle of the summary refresh: at most one per [`REFRESH_EVERY_MS`] while
/// changes keep coming, and the last change always ends with a refresh. Pure; the shell
/// passes `status::now_ms()`.
#[derive(Debug, Default)]
pub struct Throttle {
    /// When the last refresh started.
    last: Option<i64>,
    /// A trailing timer is set (one at a time).
    trailing: bool,
}

impl Throttle {
    /// A `git.changed` of the shown workspace arrived.
    pub fn changed(&mut self, now_ms: i64) -> Due {
        let wait = self.wait(now_ms);
        if wait == 0 {
            self.refreshed(now_ms);
            return Due::Now;
        }
        if std::mem::replace(&mut self.trailing, true) {
            Due::Skip
        } else {
            Due::After(wait)
        }
    }

    /// The trailing timer fired.
    pub fn timer(&mut self, now_ms: i64) -> Due {
        if !self.trailing {
            return Due::Skip;
        }
        let wait = self.wait(now_ms);
        if wait > 0 {
            return Due::After(wait);
        }
        self.refreshed(now_ms);
        Due::Now
    }

    /// A refresh starts for another reason (a workspace switch): a new window opens and a
    /// pending trailing refresh is no longer needed.
    pub fn refreshed(&mut self, now_ms: i64) {
        self.last = Some(now_ms);
        self.trailing = false;
    }

    /// Ms left in the window (0 = a refresh is allowed).
    fn wait(&mut self, now_ms: i64) -> i64 {
        let Some(last) = self.last else {
            return 0;
        };
        // A clock that stepped back opens a new window instead of freezing the refresh.
        let last = if now_ms < last {
            self.last = Some(now_ms);
            now_ms
        } else {
            last
        };
        (last + REFRESH_EVERY_MS - now_ms).max(0)
    }
}
```

Run: `cargo test -p mkrew-ui --lib git_summary::tests -j 4`
Expected: PASS.

- [ ] **Step 3: Shell**

`main.rs:63`: replace `use mkrew_ui::git_summary::{GitSummary, RepoSummary};` with `use mkrew_ui::git_summary::{Due, GitSummary, RepoSummary, Throttle};`.

`main.rs` field, after `git_summary_again: bool,` (:504):
```rust
    /// At most one summary refresh per 2 s while `git.changed` keeps coming; a trailing one
    /// always follows the last change (perf B4).
    git_summary_throttle: Throttle,
```
`shell/connection.rs` init, after `git_summary_again: false,` (:364): `git_summary_throttle: Throttle::default(),`

`shell/workspace.rs` `activate_workspace`, replace
```rust
        if switched || self.git_summary.is_none() {
            self.load_git_summary(project_id, window, cx);
        }
```
with
```rust
        if switched || self.git_summary.is_none() {
            // A switch refreshes at once and opens a new 2 s window.
            self.git_summary_throttle.refreshed(status::now_ms());
            self.load_git_summary(project_id, window, cx);
        }
```
`shell/connection.rs` event loop (:686), replace
```rust
                        self.load_git_summary(changed.project_id, window, cx);
```
with
```rust
                        let due = self.git_summary_throttle.changed(status::now_ms());
                        self.git_summary_due(changed.project_id, due, window, cx);
```
`shell/git.rs`, add after `load_git_summary` (before `git_requests`, :201):
```rust
    /// Does what the throttle decided for a `git.changed` of `project_id`: refresh the title
    /// bar summary now, or set the one trailing timer that refreshes it at the end of the
    /// 2 s window (only if that workspace is still the shown one).
    fn git_summary_due(
        &mut self,
        project_id: Ulid,
        due: Due,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        match due {
            Due::Now => self.load_git_summary(project_id, window, cx),
            Due::Skip => {}
            Due::After(ms) => {
                let background = cx.background_executor().clone();
                let wait = Duration::from_millis(ms.unsigned_abs());
                cx.spawn_in(window, async move |this, cx| {
                    background.timer(wait).await;
                    let _ = this.update_in(cx, |shell, window, cx| {
                        let due = shell.git_summary_throttle.timer(status::now_ms());
                        // A switch meanwhile refreshed the new workspace and cleared the
                        // pending timer, so `Skip`; otherwise this is still the shown one.
                        if let Some(shown) = shell.workspace.filter(|_| due != Due::Skip) {
                            shell.git_summary_due(shown, due, window, cx);
                        }
                    });
                })
                .detach();
            }
        }
    }
```
Update the doc of `load_git_summary` (:152-155): `Asked on a workspace switch and on its \`git.changed\`, never polled` → `Asked on a workspace switch and (throttled, \`git_summary_due\`) on its \`git.changed\`, never polled`.

Compile check: `cargo check -p mkrew-ui -j 4`
Expected: no errors.

- [ ] **Step 4: HUMAN_CHECKS**

Append to the perf section in `docs/HUMAN_CHECKS.md`:

```markdown
- [ ] **Git summary throttle (B4):** in Terminals, with a shell in a repo run `1..30 | % { Add-Content t.txt $_; Start-Sleep -Milliseconds 300 }` → the title bar `Git N` badge and the branch chip update about every 2 s (not on every write), and the final count shows within about 2 s after the loop ends; `git.exe` processes in Task Manager no longer spawn continuously. Switch to another workspace → its branch and badge show at once. In the Git view, commit → its own file list updates at once; the title bar badge, the Git view's dirty marker and the inspector "Changes" card follow within about 2 s (they used to be immediate).
```

- [ ] **Step 5: Verify and commit**

Run: `cargo test -p mkrew-ui --lib git_summary -j 4` (expect PASS), then `sh scripts/verify.sh` (fallback as in Task 3). Do not tick inbox line 115 (Task 6 does).
Commit: `git add crates/mkrew-ui/src/git_summary.rs crates/mkrew-ui/src/main.rs crates/mkrew-ui/src/shell/git.rs crates/mkrew-ui/src/shell/connection.rs crates/mkrew-ui/src/shell/workspace.rs` (HUMAN_CHECKS hunk via `git add -p`), then `git commit -m "perf(ui): refresh the title bar git summary at most once per 2 s"`. Never push.

---

### Task 6: No settings.toml poll; settings are checked on window activation

> **Refresh 2026-10-13:** B8 still needed. **B11 is done** (dd50ec3) except one clone that is not worth removing: **no B11 steps remain**.
> - The poll body is now in `shell/connection.rs:54-93` (status tick in `Shell::new`) and does more than before: it also applies `diff_wrap`, `reading_width` and `[features]` (`shell/connection.rs:76-87`, `sync_panes` when `features` changed). The new `reload_settings_if_changed` keeps all of that.
> - New ADR number **0055** (old 0044), which amends ADR 0024 (check timing) and ADR 0046 ("Applies live" now means: when the window is activated).
> - New step: when a reload switches `[features] docs` on while the workspace's stored view is Docs, call `show_docs` (Task 3) so the page and list load (otherwise `view()` flips to Docs with no `show_view`, and with Task 3 the file would stay `Loading`). Do Task 3 first.
> - `open_settings` doc says "fonts apply on save (ADR 0024)": reworded.

Inbox: line 115 `11. Perf: stop whole-app re-renders in the UI` (bullet B8; the last task of item 11: tick it, see Step 6). Spec: §20.0 (SPEC.md:1618-1624, 1660-1664), ADR 0024 (font settings, live reload), ADR 0046.

**B11 evidence (already done, nothing to plan):** `Editor::longest` (`editor.rs:111-113`, `longest_line()` :178, kept in `splice`) and the test `longest_line_is_the_widest_row_and_follows_edits` (`editor.rs:950`); `render_editor` uses `shown.editor.longest_line()` (`shell/files_render.rs:696-697`). The leftover `let row_theme = theme.clone();` at `shell/files_render.rs:131` is the FILE TREE list (not the editor) and is **dropped, not worth it**: `Theme` is ~70 `u32` tokens plus two font `Arc`s (`theme.rs:464-563`), cloned once per render of the Files view (not per row); `Shell::render` clones it too (`main.rs:597`) and `git_rows.rs:20` / `git_diff.rs:576` do the same. The list closure is `'static` (`cx.processor(move |…|)`) and `file_row(…, &row_theme, cx)` needs the theme and `cx` together, so it cannot borrow from `cx.theme()`; removing the clone would need an `Rc<Theme>` plumbing for no measurable gain.

**Current state (investigated):**
- B8: `Shell::new` (`shell/connection.rs:49-112`) spawns the status loop `cx.spawn_in(window, async move |this, cx| { let mut last = status::now_ms(); loop { background timer(theme::STATUS_TICK) …` and inside every 500 ms tick runs `std::fs::metadata(&shell.settings_file)…modified()` on the UI thread (:55-59) and compares with `shell.settings_modified` (field `main.rs:350-351`, init `None` at `shell/connection.rs:288`). On a change it runs `theme::reload(&shell.settings_file, &installed, &last, mode)` (`theme.rs:449`): `Ok(changed)` clears `settings_error`, sets the `theme::Fonts` global, `theme::refresh`, `cx.refresh_windows()`, then a second `mkrew_core::settings::load` applies `diff_mode` (`apply_diff_mode`, `shell/workspace_actions.rs:412`), `diff_wrap`, `reading_default` and `features`; `Err(error)` sets `settings_error` and keeps the last good fonts/theme (the palette lists it first, `shell/navigation.rs:388`, §20.0).
- Start-up: `main.rs:172-176` sets the `Fonts` global from the file; `Shell::new` reads `features`, `diff_mode`, `diff_wrap`, `reading_width` (:17-23). `settings_modified` starts `None`, so the FIRST tick reloads once; that first look also reports a broken file at start in the palette. Keep that: the first look must still happen.
- `observe_window_activation` exists (`shell/connection.rs:40-41`): `cx.observe_window_activation(window, |_, _, cx| cx.notify()).detach();` (the Inbox re-render on return). `window.is_window_active()` is used at `main.rs:616`. The callback fires on activation and on deactivation.
- Our own writes (`write_ui_value` for theme / inspector width / diff mode / diff wrap, `mkrew_core::settings::write_web_enabled` from the Phone access dialog, `shell/phone_access.rs:90`) change the mtime too; today the next tick re-loads the file and applies what we just wrote (no visible change). After this task the next activation does (also no visible change).
- `theme::reload` has the test `a_broken_settings_file_keeps_the_last_settings_and_says_why` (`theme.rs:997`): it pins "broken file keeps the last good settings and reports `file:line:col`" and "unchanged: no redraw". That logic does not move.
- Other timers are unrelated: the phone poll (`PHONE_TICK`, `shell/connection.rs:113-128`) reads `daemon.status`, not the file; the 500 ms status tick stays for the working dots / times (`status::redraw_due`).
- SPEC §20.0 (SPEC.md:1621-1622): "The UI reads it the same way and re-reads it within 500 ms of a change (fonts apply live, ADR 0024)". ADR 0024 (`docs/decisions/0024-font-settings.md:26-27`): "checked on the existing 500 ms status tick". ADR 0046 (`:30-31`): "`docs` off: … Applies live." SPEC.md:1661 `docs = false  # … applies live`.

**Files:**
- Modify: `crates/mkrew-ui/src/shell/workspace_actions.rs` (new `reload_settings_if_changed` before `open_settings`; `open_settings` doc)
- Modify: `crates/mkrew-ui/src/shell/connection.rs` (activation observer, status loop)
- Modify: `crates/mkrew-ui/src/main.rs` (field docs :347-351)
- Create: `docs/decisions/0055-settings-reload-on-activation.md`
- Modify: `docs/SPEC.md` §20.0 (:1621-1622)
- Modify: `docs/decisions/0024-font-settings.md` (:26-27)
- Modify: `docs/decisions/0046-feature-switches.md` (:31)
- Modify: `docs/HUMAN_CHECKS.md` (append)
- Modify: `docs/INBOX.md` (tick line 115; edit only, see Step 6)

**Interfaces:**
- Produces: `Shell::reload_settings_if_changed(&mut self, window: &mut Window, cx: &mut Context<'_, Self>)` (the former poll body, run at start and when the window is activated).
- Consumes: `theme::reload`, `mkrew_core::settings::load`, `Shell::apply_diff_mode`, `Shell::sync_panes`, `Shell::show_docs` (Task 3), `Shell::view()` (unchanged).

No failing test: the activation callback is GPUI-only, and `theme::reload` carries the decision logic and its existing test.

- [ ] **Step 1: `reload_settings_if_changed`**

`shell/workspace_actions.rs`, new method before `open_settings` (:379):

```rust
    /// Re-reads `settings.toml` when its mtime moved since the last look (ADR 0055). Run at
    /// start and whenever the window is activated, never on a timer: a hand edit of fonts,
    /// theme, `diff_mode`, `diff_wrap`, `reading_width` or `[features]` applies when the
    /// user comes back to the window. A broken file keeps the last good settings and lists
    /// why in the palette (§20.0).
    pub(crate) fn reload_settings_if_changed(
        &mut self,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let modified = std::fs::metadata(&self.settings_file)
            .and_then(|m| m.modified())
            .ok();
        if modified == self.settings_modified {
            return;
        }
        self.settings_modified = modified;
        let installed = cx.text_system().all_font_names();
        let last = cx.global::<theme::Fonts>().clone();
        let mode = cx.theme().mode;
        match theme::reload(&self.settings_file, &installed, &last, mode) {
            Ok(changed) => {
                self.settings_error = None;
                if let Some((fonts, mode)) = changed {
                    cx.set_global(fonts);
                    theme::refresh(mode, window, cx);
                    cx.refresh_windows();
                }
                // `theme::reload` only returns fonts and theme; a hand edit of `diff_mode` /
                // `diff_wrap` / `reading_width` / `[features]` applies too (the file just
                // parsed, so this second read only happens when it changed).
                if let Ok(settings) = mkrew_core::settings::load(&self.settings_file) {
                    self.apply_diff_mode(settings.ui.diff_mode);
                    self.diff_wrap = settings.ui.diff_wrap;
                    self.reading_default = settings.ui.reading_width;
                    if self.features != settings.features {
                        let docs_on = !self.features.docs && settings.features.docs;
                        // `view()` may now map Docs to Terminals, or back.
                        self.features = settings.features;
                        self.sync_panes(window, cx);
                        // A workspace left on Docs shows it again without `show_view`.
                        if docs_on
                            && let Some(project_id) = self.workspace
                            && self.view() == View::Docs
                        {
                            self.show_docs(project_id, window, cx);
                        }
                    }
                }
            }
            // The last good settings stay; the palette lists why (§20.0).
            Err(error) => self.settings_error = Some(error),
        }
        cx.notify();
    }
```
`open_settings` doc (:379-380): `fonts apply on save (ADR 0024)` → `they apply when you come back to the window (ADR 0055)`.

- [ ] **Step 2: Activation observer, first look, no poll**

`shell/connection.rs`, replace (:39-41)
```rust
        // Coming back to the window views the focused pane (a done session leaves the Inbox).
        cx.observe_window_activation(window, |_, _, cx| cx.notify())
            .detach();
```
with
```rust
        // Coming back to the window views the focused pane (a done session leaves the Inbox).
        // Back from Zed after a hand edit of settings.toml: fonts, theme, diff and feature
        // switches apply now (ADR 0055).
        cx.observe_window_activation(window, |shell, window, cx| {
            if window.is_window_active() {
                shell.reload_settings_if_changed(window, cx);
            }
            cx.notify();
        })
        .detach();
```
Status loop: replace (:49-51)
```rust
        cx.spawn_in(window, async move |this, cx| {
            let mut last = status::now_ms();
            loop {
```
with
```rust
        cx.spawn_in(window, async move |this, cx| {
            let mut last = status::now_ms();
            // The first look at settings.toml (reports a broken file at start; the activation
            // observer does the later ones).
            let _ = this.update_in(cx, |shell, window, cx| {
                shell.reload_settings_if_changed(window, cx);
            });
            loop {
```
and delete, inside the loop's `this.update_in(cx, |shell, window, cx| {` closure, the whole poll block from the comment `// ponytail: mtime poll on the 500 ms tick; a file watcher if it ever shows` (:55) through the `}` that closes `if modified != shell.settings_modified {` (:93, the `cx.notify();` + `}` right before `let sessions = shell.sidebar.sessions();`; about 39 lines). Keep `let sessions = shell.sidebar.sessions();` and everything after it unchanged (`window` is still used by `window.is_visible()`).

`main.rs` field docs (:347-351): replace
```rust
    /// `<config>/mkrew/settings.toml`, read when a composer `@` pick is typed and polled for
    /// font changes (§20.0).
    settings_file: PathBuf,
    /// Its mtime at the last font check (ADR 0024); `None` until the first status tick reads it.
```
with
```rust
    /// `<config>/mkrew/settings.toml`, read when a composer `@` pick is typed and checked for
    /// changes when the window is activated (§20.0, ADR 0055).
    settings_file: PathBuf,
    /// Its mtime at the last check (ADR 0024); `None` until the first check reads it.
```

Compile check: `cargo check -p mkrew-ui -j 4`
Expected: no errors, no unused-variable warnings from the status loop.
Run: `cargo test -p mkrew-ui --lib theme::tests::a_broken_settings_file -j 4`
Expected: PASS (unchanged logic).

- [ ] **Step 3: ADR 0055, SPEC, ADR 0024, ADR 0046**

Create `docs/decisions/0055-settings-reload-on-activation.md`:

```markdown
# 0055 — The UI checks settings.toml when its window is activated, not every 500 ms

- Date: 2026-10-13
- Status: accepted
- Milestone: perf review (inbox item 11, B8)
- Amends: ADR 0024 (when the file is checked), ADR 0046 ("applies live")

## Context

ADR 0024 re-reads `settings.toml` when its mtime changes, checked on the 500 ms status tick.
That is a `std::fs::metadata` call on the UI thread twice a second for the whole life of the
app (`docs/PERF_REVIEW.md` B8). The reload now also applies `diff_mode`, `diff_wrap`,
`reading_width` and `[features]` (ADR 0046). The file is hand-edited config: it changes while
the user is in Zed or another editor, so the Mkrew window is not in front when it happens.

## Options

1. Keep the poll, check less often — still a timer on the UI thread; delays live feedback.
2. A file watcher in the UI — a new crate or thread for one small file; rejected in ADR 0024.
3. Check when the window is activated (and once at start) — no timer; the edit applies the
   moment the user comes back.

## Decision

Option 3. `Shell::reload_settings_if_changed` runs from `observe_window_activation` and once at
start. It keeps the mtime compare, `theme::reload` and the rule that a broken file keeps the
last good settings and shows the error in the palette (§20.0). The daemon is unchanged (it
watches its own config dir, 300 ms debounce).

## Consequences

- SPEC §20.0: "re-reads it within 500 ms of a change" becomes "re-reads it when its window is
  activated". ADR 0024 and ADR 0046 point here for the check; "applies live" for the UI means
  "when the window is activated".
- A settings edit made while the Mkrew window stays in front (for example by a script) applies
  at the next activation. The UI's own writes (theme toggle, inspector width, diff mode, Phone
  access) need no reload: the state is already applied.
- Switching `[features] docs` on while a workspace is left on Docs loads its list and file at
  that reload.
- HUMAN_CHECKS lists the check.
```

`docs/SPEC.md` §20.0, replace
```
`daemon.status` (`settings_error`) and by the palette like keymap errors. The UI reads it the
same way and re-reads it within 500 ms of a change (fonts apply live, ADR 0024). Palette
```
with
```
`daemon.status` (`settings_error`) and by the palette like keymap errors. The UI reads it the
same way at start and again each time its window is activated, if the file changed (no
polling, ADR 0055): fonts, theme, `diff_mode` and `[features]` apply when you come back to the
window. Palette
```
`docs/decisions/0024-font-settings.md`, replace
```
`settings.toml` is re-read when its mtime changes, checked on the existing 500 ms status tick —
```
with
```
`settings.toml` is re-read when its mtime changes, checked when the window is activated (ADR 0055;
first version: the 500 ms status tick) —
```
`docs/decisions/0046-feature-switches.md` (:30-31), replace
```
  workspace left on Docs shows Terminals. Applies live.
```
with
```
  workspace left on Docs shows Terminals. Applies live (the UI when its window is activated,
  ADR 0055).
```

- [ ] **Step 4: HUMAN_CHECKS**

Append to the perf section in `docs/HUMAN_CHECKS.md`:

```markdown
- [ ] **Settings reload on activation (B8):** with Mkrew open, edit `settings.toml` in Zed (`[terminal] font_size = 16`, or `[ui] theme = "dark"`, or `diff_mode`), save, click back into the Mkrew window → the font size / theme / diff layout changes at once (it no longer changes while Zed is in front). Set `[features] docs = true`, save, come back → the Docs tab appears in the rail (and, if the workspace was left on Docs, its file list and page load). Make the file invalid (`font_size = 4`), save, come back → nothing changes, the palette (Ctrl+Shift+P) lists `Settings error: settings.toml:…` first; fix it, come back → the error is gone. Start Mkrew with a hand-edited file → its fonts apply at start; start it with an invalid file → the error is in the palette at once. Toggle the theme with the title bar button and open Phone access → no visible change on the next activation.
- [ ] **CPU before / after (item 11):** with 2–3 agents `working` and the window visible, read `mkrew-ui` CPU in Task Manager over one minute: before this plan (note the number from your last build) vs now. Write both numbers under this item in `docs/PERF_REVIEW.md` (end of the file) or here. Also: an agent saving a large `.md` repeatedly while it is open in Files Preview → CPU stays low (if not, see Task 4 evidence: the cost would be in `diagram.rs`).
```

- [ ] **Step 5: Verify**

Run: `cargo test -p mkrew-ui --lib docs git_summary theme editor -j 4` (module filters one by one if the single command takes only one filter), then `sh scripts/verify.sh` (fallback as in Task 3).

- [ ] **Step 6: Tick the inbox and commit**

`docs/INBOX.md` line 115: change `- [~] **11. Perf: stop whole-app re-renders in the UI**` to `- [x] **11. Perf: stop whole-app re-renders in the UI**` (the plan suffix is already there). Reword two bullets: B11 (:122) → `B11: Files editor caches the longest line width per text version (done in dd50ec3); the per-render \`Theme\` clone is kept (not worth it).`; A6 (:123) → `A6: dropped: the preview already builds off the UI thread, once per text (plan Task 4 evidence).` Edit it, but do NOT stage `docs/INBOX.md` (INBOX stays the user's).
Commit: `git add crates/mkrew-ui/src/shell/workspace_actions.rs crates/mkrew-ui/src/shell/connection.rs crates/mkrew-ui/src/main.rs docs/decisions/0055-settings-reload-on-activation.md docs/decisions/0024-font-settings.md docs/decisions/0046-feature-switches.md docs/SPEC.md` (HUMAN_CHECKS hunk via `git add -p`), then `git commit -m "perf(ui): check settings.toml on window activation instead of polling"`. Never push.

---

### Task 7: SQLite writes: no `hook` events, `synchronous=NORMAL`, backup on its own connection, pruning

> **Refresh 2026-10-13:** still needed (nothing of A4/B7 landed: `session.rs:1228-1239` still inserts `hook` rows, `mkrew-store/src/lib.rs:101-102` has no `synchronous`, `backup.rs:37` still copies under `hub.store()`). Changes vs the 2026-10-10 version:
> - the daemon integration tests were split into `crates/mkrew-daemon/tests/daemon/{main,states,turns,docs,…}.rs`; the two `hook` tests are now in `states.rs:94` and `:185`;
> - inbox line 129 says "remove the tests that only pin it": the `hook`-row assertions are deleted, not turned into "is empty" assertions; the `commits_use_synchronous_normal` test is dropped (it only pins a pragma default);
> - the "backup while the store mutex is held" test holds the lock on a helper thread, not a `MutexGuard` across `.await` (clippy `await_holding_lock` fails `-D warnings`);
> - SPEC / `session.rs` / `server.rs` line numbers re-read. Phone web (`crates/mkrew-cli/web/app.mjs`, `src/web/relay.rs`) and `mk` read no `session_event` rows (checked below).

Inbox: line 126 `12. Perf: daemon watcher, git.changed and SQLite` (bullets A4 and B7; do not tick yet). Spec: §5 (SPEC.md:255-261 table comment, :352 WAL line), §8 (SPEC.md:652 backups), §15 NFR table (:1357).

**Current state (investigated):**
- A4. `Actor::on_hook` (`crates/mkrew-daemon/src/session.rs:1223`) builds `record = {agent, event, payload}` and calls `self.hub.store().insert_session_event(self.session.id, "hook", &record, at)` on EVERY `hook.report` (lines 1228-1239). `let at = crate::server::now_ms();` (1224) is only used by that insert. `report.agent` / `report.event` stay used by the `tracing::debug!` at :1227; `report.payload` by `edited_paths` (:1240).
- Every reader of `session_event` (grep `session_events(` and `project_session_events(` over `crates`):
  - `server/doc.rs:29` kind `files_changed` (`doc.list`, last 24 h, `docs::RECENT_MS`, `doc.rs:26`).
  - `server/turn.rs:112` kind `edited` (`session.files`; the UI sends `since_ms = now − 24 h`, `mkrew-ui/src/git_view.rs:764`).
  - `server/turn.rs:143` kind `usage` with `since = 0` (`session.usage`: the newest usage of each session; totals survive restarts, SPEC §20E.3 :1913-1914).
  - Tests only: `mkrew-store/src/tests.rs:334-345` and `:1043-1063` (store-level, any kind string), `mkrew-daemon/tests/daemon/states.rs:160`, `:203` (kind `hook`), `tests/daemon/main.rs:569` (`files_changed`, helper `changed_files`), `tests/daemon/turns.rs:178` writes a `usage` row.
  - **No production code reads kind `hook`.** `state_changed` is never written either (only named in `0001_init.sql:46` and SPEC :258).
  - Other consumers checked: `mk` (`crates/mkrew-cli/src/*`) talks `hook.report` / `session.read` / `session.wait` and never reads session events; the phone relay (`crates/mkrew-cli/src/web/relay.rs`) forwards requests and passes events through unchanged; `web/app.mjs:223-259` reacts to `session.state`, `session.exited`, `session.updated`, `session.terminal_title`, `project.updated`, `session.snapshot` only. Nothing there depends on `hook` rows.
- Pruning: the 24 h readers need rows at most 1 day old, so `files_changed`, `edited` and the old `hook` rows are safe to drop after 7 days. **`usage` is NOT pruned**: `session.usage` reads it with `since = 0` and the UI totals (inspector) need the newest usage row of every session, including sessions older than a week.
- B7. `Store::open` (`crates/mkrew-store/src/lib.rs:99-106`) sets `journal_mode=WAL` (101) and `foreign_keys` (102); `synchronous` stays at the default FULL. `Store::backup_to` (:113-122) runs `VACUUM INTO` on the Store's own connection. `backup::run(store: &Store, …)` (`backup.rs:70`) is called from `backup::start` (:34-37) as `run(&hub.store(), &dir, now)`, so the global `Hub::store` mutex is held for the whole copy and every other request that needs the store waits. The check runs hourly (`EVERY`; the first tick is immediate) in `spawn_blocking`; it also calls `attachment::cleanup`, which takes `hub.store()` per session dir.
- `backup::start` is called from `server.rs:120-124` with `self.paths.backups_dir()` and `self.paths.attachments_dir()`; the database path is `Paths::db_file()` (`mkrew-core/src/paths.rs:57`).
- Tests: `backup.rs` `mod tests` (`use super::*`, `NOON` :155, `names(dir)` :157; the two run-based tests at :192 and :216); `mkrew-store/src/tests.rs` (helpers `open(&dir)`, `session(project_id, started_at)`; `backup_to_writes_a_readable_copy` at :62-74).
- SPEC: :258 comment `state_changed|hook|files_changed|...`; :352 `SQLite runs in WAL mode.`; :652 `Backups: daily copy of mkrew.db …, keep 7`; :1357 `Data | SQLite WAL; daily DB backup, keep 7`. No statement about durability or `synchronous`. No ADR needed: no behavior in the SPEC is contradicted (the table stays, no migration; latest migration is 0018).

**Files:**
- Modify: `crates/mkrew-store/src/lib.rs` (`Store::open`, replace `Store::backup_to` with free fn `backup`, add `Store::prune_session_events`)
- Modify: `crates/mkrew-store/src/tests.rs`
- Modify: `crates/mkrew-daemon/src/backup.rs` (`start`, `run`, new `prune_events`, tests)
- Modify: `crates/mkrew-daemon/src/server.rs` (call of `backup::start`, :120)
- Modify: `crates/mkrew-daemon/src/session.rs` (`Actor::on_hook`)
- Modify: `crates/mkrew-daemon/tests/daemon/states.rs` (`hook_reports_drive_the_state_of_live_sessions_only`, `screen_only_profiles_record_hooks_but_ignore_them`)
- Modify: `docs/SPEC.md` (:258, :352, :652, :1357), `docs/HUMAN_CHECKS.md` (append to the shared perf section)

**Interfaces:**
- Produces: `mkrew_store::backup(db: &Path, to: &Path) -> Result<()>` (own connection; replaces `Store::backup_to`, whose only callers are `backup.rs:86` and the store test).
- Produces: `Store::prune_session_events(&self, kind: &str, before: i64, limit: usize) -> Result<usize>` (rows of `kind` with `at < before`, at most `limit`, returns how many were deleted).
- Produces: `backup::run(db: &Path, dir: &Path, now_ms: i64) -> anyhow::Result<Option<PathBuf>>`, `backup::prune_events(hub: &Hub, now_ms: i64, chunk: usize) -> Result<usize, mkrew_store::StoreError>`, `backup::start(hub: Arc<Hub>, db: PathBuf, dir: PathBuf, attachments: PathBuf) -> JoinHandle<()>`.

- [ ] **Step 1: Failing tests**

`crates/mkrew-store/src/tests.rs`: replace the test `backup_to_writes_a_readable_copy` (:61-74) with (same body, free function, db path):

```rust
#[test]
fn backup_writes_a_readable_copy() {
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path().join("repo");
    std::fs::create_dir(&repo).unwrap();
    let db = dir.path().join("mkrew.db");
    let store = open(&dir);
    let project = store.add_project(&repo, Some("repo")).unwrap();
    let copy = dir.path().join("copy.db");
    backup(&db, &copy).unwrap();
    let restored = Store::open(&copy).unwrap();
    assert_eq!(restored.list_projects().unwrap(), vec![project]);
    // An existing copy is never overwritten.
    assert!(backup(&db, &copy).is_err());
}

/// The copy is made on its own connection: a writer in the middle of a transaction neither
/// blocks it (WAL) nor leaks its uncommitted row into the copy.
#[test]
fn backup_does_not_wait_for_a_writer_and_skips_its_uncommitted_rows() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("mkrew.db");
    let store = open(&dir);
    let p = store.add_project(dir.path(), None).unwrap();
    let s = session(p.id, 1);
    store.insert_session(&s).unwrap();
    store
        .insert_session_event(s.id, "usage", &serde_json::json!({}), 1)
        .unwrap();

    let writer = Connection::open(&db).unwrap();
    writer.execute_batch("BEGIN IMMEDIATE").unwrap();
    writer
        .execute(
            "INSERT INTO session_event (session_id, kind, payload, at) VALUES (?1, 'files_changed', '{}', 2)",
            [s.id.to_string()],
        )
        .unwrap();

    let copy = dir.path().join("copy.db");
    backup(&db, &copy).unwrap();
    writer.execute_batch("COMMIT").unwrap();

    let restored = Store::open(&copy).unwrap();
    assert_eq!(restored.session_events(s.id, "usage").unwrap().len(), 1);
    assert!(restored.session_events(s.id, "files_changed").unwrap().is_empty());
    assert_eq!(store.session_events(s.id, "files_changed").unwrap().len(), 1);
}

/// Rows of the kind older than `before` go, oldest first, at most `limit` per call; other
/// kinds and newer rows stay.
#[test]
fn prune_session_events_deletes_old_rows_of_one_kind_in_chunks() {
    let dir = tempfile::tempdir().unwrap();
    let store = open(&dir);
    let p = store.add_project(dir.path(), None).unwrap();
    let s = session(p.id, 1);
    store.insert_session(&s).unwrap();
    for at in [1, 2, 3, 10] {
        store
            .insert_session_event(s.id, "hook", &serde_json::json!({}), at)
            .unwrap();
    }
    store
        .insert_session_event(s.id, "usage", &serde_json::json!({}), 1)
        .unwrap();

    assert_eq!(store.prune_session_events("hook", 5, 2).unwrap(), 2);
    assert_eq!(store.prune_session_events("hook", 5, 2).unwrap(), 1);
    assert_eq!(store.prune_session_events("hook", 5, 2).unwrap(), 0);
    let left: Vec<i64> = store
        .session_events(s.id, "hook")
        .unwrap()
        .iter()
        .map(|e| e.at)
        .collect();
    assert_eq!(left, [10]);
    assert_eq!(store.session_events(s.id, "usage").unwrap().len(), 1);
}
```

`crates/mkrew-daemon/src/backup.rs` `mod tests`: add at the top of the module (below `use super::*;`) and the tests at its end (before the closing `}`):

```rust
    use std::sync::Arc;

    use mkrew_protocol::{Session, SessionKind, SessionState, StateSource};
    use mkrew_store::Store;
    use ulid::Ulid;

    /// A hub whose store holds one session with a `session_event` row per `(kind, at)`.
    fn hub_with_events(events: &[(&str, i64)]) -> (tempfile::TempDir, Arc<Hub>, Ulid) {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(&dir.path().join("mkrew.db")).unwrap();
        let project = store.add_project(dir.path(), Some("repo")).unwrap();
        let id = Ulid::new();
        store
            .insert_session(&Session {
                id,
                task_id: None,
                project_id: project.id,
                agent_profile: "fake".into(),
                kind: SessionKind::Agent,
                title: None,
                terminal_title: None,
                color: None,
                native_session_id: None,
                state: SessionState::Idle,
                state_source: StateSource::Daemon,
                state_changed_at: 0,
                exit_code: None,
                cols: 80,
                rows: 24,
                started_at: 0,
                ended_at: None,
                pinned: false,
                plan_path: None,
            })
            .unwrap();
        for &(kind, at) in events {
            store
                .insert_session_event(id, kind, &serde_json::json!({}), at)
                .unwrap();
        }
        (dir, Arc::new(Hub::new(store)), id)
    }

    /// The `at` of the session's remaining events of `kind`, oldest first.
    fn left(hub: &Hub, session: Ulid, kind: &str) -> Vec<i64> {
        let events = hub.store().session_events(session, kind).unwrap();
        events.iter().map(|e| e.at).collect()
    }

    /// Rows of the pruned kinds go once they are older than a week (several chunks);
    /// exactly 7 days old and newer rows stay, and so does every `usage` row: `session.usage`
    /// totals read them with `since = 0`.
    #[test]
    fn events_past_a_week_are_pruned_but_recent_ones_and_usage_stay() {
        let now = 100 * DAY_MS;
        let old = now - EVENT_KEEP_MS - 1;
        let edge = now - EVENT_KEEP_MS;
        let recent = now - DAY_MS;
        let mut events = vec![("hook", old); 5];
        events.extend([
            ("hook", recent),
            ("files_changed", old),
            ("files_changed", edge),
            ("edited", old),
            ("usage", old),
            ("usage", recent),
        ]);
        let (_dir, hub, session) = hub_with_events(&events);

        assert_eq!(prune_events(&hub, now, 2).unwrap(), 7);

        assert_eq!(left(&hub, session, "hook"), [recent]);
        assert_eq!(left(&hub, session, "files_changed"), [edge]);
        assert!(left(&hub, session, "edited").is_empty());
        assert_eq!(left(&hub, session, "usage"), [old, recent]);
    }

    /// The hourly check writes today's backup while another thread holds the store mutex (a
    /// long write); only the pruning after it waits for the lock.
    #[tokio::test(flavor = "multi_thread")]
    async fn the_backup_is_written_while_the_store_mutex_is_held() {
        let data = tempfile::tempdir().unwrap();
        let db = data.path().join("mkrew.db");
        let hub = Arc::new(Hub::new(Store::open(&db).unwrap()));
        let dir = data.path().join("backups");

        // A helper thread holds the lock (a `MutexGuard` must not live across an `.await`).
        let (locked_tx, locked) = std::sync::mpsc::channel::<()>();
        let (release, held) = std::sync::mpsc::channel::<()>();
        let holder = {
            let hub = Arc::clone(&hub);
            std::thread::spawn(move || {
                let _guard = hub.store();
                locked_tx.send(()).unwrap();
                let _ = held.recv();
            })
        };
        locked.recv().unwrap();

        let task = start(Arc::clone(&hub), db, dir.clone(), data.path().join("attachments"));
        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        while !(dir.exists() && names(&dir).iter().any(|n| is_backup(n))) {
            assert!(
                std::time::Instant::now() < deadline,
                "no backup while the store mutex is held"
            );
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        release.send(()).unwrap();
        holder.join().unwrap();
        task.abort();
    }
```

`crates/mkrew-daemon/tests/daemon/states.rs`:

In `hook_reports_drive_the_state_of_live_sessions_only` (:94) delete the block at :157-168 (from `let db = env.data_dir().join("mkrew.db");` through `assert_eq!(events[0].payload["payload"]["session_id"], "native-1");` and the blank line after it). Change the comment at :142 `// A known agent's unknown event is recorded but changes nothing.` to `// A known agent's unknown event changes nothing.`.

Rename `screen_only_profiles_record_hooks_but_ignore_them` (:185) to `screen_only_profiles_take_the_native_id_but_ignore_hook_states` and delete its last block (:200-205, `let db = …; let events = Store::open(&db)….session_events(id, "hook")….unwrap(); assert_eq!(events.len(), 1);`), keeping the two state assertions and the `native_session_id` assertion before it. (`Store` is still used by `changed_files` in `main.rs`; the daemon tests keep their `use super::*`.)

Run: `cargo test -p mkrew-store -p mkrew-daemon --no-run -j 4`
Expected: FAIL to compile (`backup`, `prune_session_events`, `prune_events`, `EVENT_KEEP_MS`, the new `start` signature do not exist).

- [ ] **Step 2: Store: `synchronous`, own-connection backup, prune**

`crates/mkrew-store/src/lib.rs`, `Store::open` (:99-106): replace

```rust
        conn.pragma_update(None, "foreign_keys", true)?;
```

with

```rust
        conn.pragma_update(None, "foreign_keys", true)?;
        // WAL + NORMAL: a commit no longer fsyncs the log. A crash of the app never loses a
        // commit; only an OS crash or power cut can lose the last few and the file stays
        // consistent (SPEC §5).
        conn.pragma_update(None, "synchronous", "NORMAL")?;
```

and in its doc comment (:98) replace `enables WAL and runs migrations.` with `enables WAL (synchronous NORMAL) and runs migrations.`.

Delete `Store::backup_to` (the doc comment `/// Writes a consistent copy of the database to `path` (`VACUUM INTO`, safe while in use).` through its closing `}`, lines 113-122) and add this free function directly above `fn user_version` (`grep -n "^fn user_version" crates/mkrew-store/src/lib.rs`, :1219):

```rust
/// Writes a consistent copy of the database file `db` to `to` (`VACUUM INTO`). It opens a
/// connection of its own, so the caller needs no [`Store`] (and no lock around one): in WAL
/// mode the copy reads one snapshot while writers go on, and never contains an uncommitted
/// row. SQLite refuses to overwrite an existing non-empty file.
pub fn backup(db: &Path, to: &Path) -> Result<()> {
    let target = to.to_str().ok_or_else(|| StoreError::Path {
        path: to.to_path_buf(),
        source: std::io::Error::new(std::io::ErrorKind::InvalidInput, "path is not UTF-8"),
    })?;
    let conn = Connection::open(db)?;
    conn.busy_timeout(std::time::Duration::from_secs(5))?;
    conn.execute("VACUUM INTO ?1", [target])?;
    Ok(())
}
```

In `impl Store`, after `project_session_events` (starts :889; add below its closing `}`), add:

```rust
    /// Deletes up to `limit` `session_event` rows of `kind` older than `before` (ms), oldest
    /// first, and returns how many went. Callers loop until it returns less than `limit`, so
    /// one call holds the connection only briefly.
    pub fn prune_session_events(&self, kind: &str, before: i64, limit: usize) -> Result<usize> {
        let limit = i64::try_from(limit).unwrap_or(i64::MAX);
        Ok(self.conn.execute(
            "DELETE FROM session_event WHERE id IN (
                SELECT id FROM session_event WHERE kind = ?1 AND at < ?2 ORDER BY id LIMIT ?3)",
            params![kind, before, limit],
        )?)
    }
```

(`params!` is already imported in this file; check with `grep -n "params!" crates/mkrew-store/src/lib.rs | head -1` and add `params` to the `use rusqlite::{…}` list if not.)

Run: `cargo test -p mkrew-store -j 4`
Expected: PASS (all store tests). If `backup_does_not_wait_for_a_writer…` fails with `database is locked`, `VACUUM INTO` needed a write lock here: drop the `BEGIN IMMEDIATE` half of that test (keep the plain copy) and say so in the Open questions row.

- [ ] **Step 3: Daemon: backup without the mutex, pruning in the hourly check**

`crates/mkrew-daemon/src/backup.rs`:

Delete the production line `use mkrew_store::Store;` (:14; the tests import it themselves, Step 1). After `const DAY_MS: i64 = 24 * 60 * 60 * 1000;` add:

```rust
/// `session_event` kinds nobody reads once they are a day old (`doc.list` and `session.files`
/// look back 24 h; `hook` is not written any more). `usage` is not here: `session.usage` reads
/// the newest usage row of every session, however old (SPEC §20E.3).
const PRUNED_KINDS: [&str; 3] = ["hook", "files_changed", "edited"];
/// Pruned events older than this are deleted.
const EVENT_KEEP_MS: i64 = 7 * DAY_MS;
/// Rows deleted per lock of the store, so a big backlog never stalls the daemon.
const PRUNE_CHUNK: usize = 5_000;
```

Replace the doc comment and signature of `start` (:26-28):

```rust
/// Checks for today's backup now and then every [`EVERY`]; each check also deletes old
/// attachments in `attachments` ([`attachment::cleanup`], SPEC §20B.5). Runs until aborted.
pub fn start(hub: Arc<Hub>, dir: PathBuf, attachments: PathBuf) -> JoinHandle<()> {
```

with

```rust
/// Checks for today's backup now and then every [`EVERY`]; each check also deletes old
/// attachments in `attachments` ([`attachment::cleanup`], SPEC §20B.5) and old session events
/// ([`prune_events`]). The backup copies the database file `db` on its own connection, never
/// under the store mutex. Runs until aborted.
pub fn start(hub: Arc<Hub>, db: PathBuf, dir: PathBuf, attachments: PathBuf) -> JoinHandle<()> {
```

Replace (:34-44)

```rust
            let (hub, dir, attachments) = (Arc::clone(&hub), dir.clone(), attachments.clone());
            let done = tokio::task::spawn_blocking(move || {
                let now = crate::server::now_ms();
                let backed_up = run(&hub.store(), &dir, now);
                let cleaned = attachment::cleanup(&attachments, now, |id| {
                    hub.store().get_session(id).ok().and_then(|s| s.ended_at)
                });
                (backed_up, cleaned)
            })
            .await;
            let (backed_up, cleaned) = match done {
```

with

```rust
            let (hub, db, dir, attachments) = (
                Arc::clone(&hub),
                db.clone(),
                dir.clone(),
                attachments.clone(),
            );
            let done = tokio::task::spawn_blocking(move || {
                let now = crate::server::now_ms();
                let backed_up = run(&db, &dir, now);
                let pruned = prune_events(&hub, now, PRUNE_CHUNK);
                let cleaned = attachment::cleanup(&attachments, now, |id| {
                    hub.store().get_session(id).ok().and_then(|s| s.ended_at)
                });
                (backed_up, pruned, cleaned)
            })
            .await;
            let (backed_up, pruned, cleaned) = match done {
```

and after the `match backed_up { … }` block (:51-55) add:

```rust
            match pruned {
                Ok(0) => {}
                Ok(removed) => tracing::info!(removed, "old session events deleted"),
                Err(e) => tracing::warn!("session events not pruned: {e}"),
            }
```

Replace `run`'s doc and signature (:68-70):

```rust
/// Copies the database to today's file unless it already exists, then prunes to [`KEEP`].
/// Returns the file it created, if any.
pub fn run(store: &Store, dir: &Path, now_ms: i64) -> anyhow::Result<Option<PathBuf>> {
```
→
```rust
/// Copies the database file `db` to today's file unless it already exists, then prunes to
/// [`KEEP`]. Returns the file it created, if any.
pub fn run(db: &Path, dir: &Path, now_ms: i64) -> anyhow::Result<Option<PathBuf>> {
```

and its copy call (:85-87):

```rust
        store
            .backup_to(&partial)
            .with_context(|| format!("write {}", partial.display()))?;
```
→
```rust
        mkrew_store::backup(db, &partial)
            .with_context(|| format!("write {}", partial.display()))?;
```

Add after `run` (before `/// Deletes all but the newest `keep` backups`):

```rust
/// Deletes the events of [`PRUNED_KINDS`] older than [`EVENT_KEEP_MS`], `chunk` rows per lock
/// of the store. Returns how many were deleted.
pub fn prune_events(
    hub: &Hub,
    now_ms: i64,
    chunk: usize,
) -> Result<usize, mkrew_store::StoreError> {
    let before = now_ms - EVENT_KEEP_MS;
    let mut total = 0;
    for kind in PRUNED_KINDS {
        loop {
            // The guard is a temporary: the lock is free again between chunks.
            let removed = hub.store().prune_session_events(kind, before, chunk)?;
            total += removed;
            if removed < chunk {
                break;
            }
        }
    }
    Ok(total)
}
```

Existing tests in this module (`one_backup_per_day_and_the_newest_seven_are_kept` :192, `a_leftover_partial_file_is_replaced` :216): replace `let store = Store::open(&data.path().join("mkrew.db")).unwrap();` with

```rust
        let db = data.path().join("mkrew.db");
        Store::open(&db).unwrap();
```

and every `run(&store, ` with `run(&db, ` (`grep -n "run(&store" crates/mkrew-daemon/src/backup.rs`: :197, :201, :205, :223).

`crates/mkrew-daemon/src/server.rs:120-124`: replace

```rust
        let backups = backup::start(
            Arc::clone(&self.hub),
            self.paths.backups_dir(),
```

with

```rust
        let backups = backup::start(
            Arc::clone(&self.hub),
            self.paths.db_file(),
            self.paths.backups_dir(),
```

Run: `cargo test -p mkrew-daemon --lib backup -j 4`
Expected: PASS (`events_past_a_week_are_pruned_but_recent_ones_and_usage_stay`, `the_backup_is_written_while_the_store_mutex_is_held` and the older backup tests).

- [ ] **Step 4: Stop writing `hook` events (A4)**

`crates/mkrew-daemon/src/session.rs`, `Actor::on_hook` (:1221-1258): replace the doc comment

```rust
    /// Records the report as a `hook` session event, takes the agent's session id and lets
    /// the state machine decide (it ignores hooks for screen-only profiles).
    fn on_hook(&mut self, agent: HookAgent, report: HookReportParams) {
        let at = crate::server::now_ms();
        let native = agent.native_session_id(&report.payload);
```

with

```rust
    /// Takes the agent's session id, records the files the report names as edited and lets the
    /// state machine decide (it ignores hooks for screen-only profiles). The raw report is not
    /// stored: nothing reads it (perf A4).
    fn on_hook(&mut self, agent: HookAgent, report: HookReportParams) {
        let native = agent.native_session_id(&report.payload);
```

and delete this block (:1228-1239):

```rust
        let record = serde_json::json!({
            "agent": report.agent,
            "event": report.event,
            "payload": report.payload,
        });
        if let Err(e) = self
            .hub
            .store()
            .insert_session_event(self.session.id, "hook", &record, at)
        {
            tracing::error!(id = %self.session.id, "saving hook event failed: {e}");
        }
```

Verify before deleting: `grep -rn '"hook"' crates --include=*.rs` shows only the profile `StateMode` serde name (`mkrew-core/src/profile.rs:91`), `source_str`/`parse_source` (`StateSource::Hook`), the fake-agent scenario `action = "hook"`, store-level tests that use `"hook"` as an arbitrary kind (`mkrew-store/src/tests.rs`), and the `states.rs` blocks deleted in Step 1.

Run: `cargo test -p mkrew-daemon --test daemon -j 4 hook_reports_drive screen_only_profiles_take`
Expected: PASS (build `cargo build -p mkrew-cli -p mkrew-fakeagent -j 4` first if the bins are stale).

- [ ] **Step 5: SPEC**

- `docs/SPEC.md:258`: replace `-- state_changed|hook|files_changed|...` with `-- files_changed|edited|usage|... (hook reports are not stored)`.
- `:352`: replace `SQLite runs in WAL mode. Schema changes` with `SQLite runs in WAL mode with `synchronous=NORMAL` (a commit is not fsynced: an OS crash or power cut can lose the last few commits, never corrupt the file). Events of kinds `files_changed` and `edited` older than 7 days are deleted by the hourly maintenance check (§8); `usage` events are kept, `session.usage` needs them. Schema changes`.
- `:652`: replace `daily copy of `mkrew.db` to `<data>/mkrew/backups/`, keep 7.` with `daily copy of `mkrew.db` to `<data>/mkrew/backups/` (`VACUUM INTO` on its own connection, not under the daemon's store lock), keep 7. The same hourly check deletes old session events (§5).`
- `:1357`: replace `SQLite WAL; daily DB backup, keep 7` with `SQLite WAL (`synchronous=NORMAL`); daily DB backup, keep 7`.

- [ ] **Step 6: HUMAN_CHECKS**

Append to the end of the section `### To verify — 2026-10-13 perf (plan docs/superpowers/plans/2026-10-10-inbox-perf.md)` (Task 1 created it):

```markdown
- [ ] **SQLite after the perf change:** start the app with a database that has a few weeks of use. After the first hour (or a restart: the check runs at startup) open `mkrew.db` with `sqlite3`: `select kind, count(*) from session_event group by kind;` has no `hook` rows older than 7 days, and no `files_changed` / `edited` rows older than 7 days; `usage` rows of old sessions are still there and the session panel still shows their token totals. While an agent works, `select count(*) from session_event where kind='hook';` does not grow. Today's file in `backups/` opens in `sqlite3` and lists your sessions; during the backup (start the app, immediately click around) the UI does not freeze.
```

- [ ] **Step 7: Verify and commit**

Run: `cargo test -p mkrew-store -p mkrew-daemon -j 4` (expect PASS), then `sh scripts/verify.sh` (fallback if rustc crashes: `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace -j 4`). Do not tick inbox line 126 yet (Tasks 8 and 9 follow).
Commit: `git add crates/mkrew-store/src/lib.rs crates/mkrew-store/src/tests.rs crates/mkrew-daemon/src/backup.rs crates/mkrew-daemon/src/server.rs crates/mkrew-daemon/src/session.rs crates/mkrew-daemon/tests/daemon/states.rs docs/SPEC.md docs/HUMAN_CHECKS.md` (`session.rs`, `server.rs`, `SPEC.md`, `HUMAN_CHECKS.md` are shared with other tasks: if the tree holds hunks that are not this task's, stage this task's with `git add -p <path>`). Never push.
Message: `perf(daemon): no hook events, synchronous=NORMAL, backup off the store lock, prune old events`

---

### Task 8: Watcher: `git.changed` only for real changes, 5 s cap while working, release kept watchers

> **Refresh 2026-10-13:** still needed (`watch.rs:18` `MAX_DELAY` is 2 s only, `session.rs:662-676` sends `GitChanged` for every batch, nothing clears `ProjectWatch::kept`). Changes vs the 2026-10-10 version:
> - protocol bump is **40 → 41** (`mkrew-protocol/src/lib.rs:16`), ADR **0056** (was 0042); latest ADR on disk is 0054, 0055 is reserved for Task 6;
> - the UI moved into `crates/mkrew-ui/src/shell/*.rs`: `Shell` fields stay in `main.rs` (`git_summary_busy` :503) but their initializer is in `shell/connection.rs:363`, and the four call sites are `shell/workspace.rs` (`show_view` :343, `activate_workspace` :21), `shell/files.rs:59` (`refresh_files`) and `shell/git.rs:94` (`refresh_git`); `Shell::view()` (`workspace.rs:333`) already maps Docs to Terminals while `[features] docs` is off;
> - protocol tests moved to `crates/mkrew-protocol/src/message/tests.rs` (`sid()` :21, `every_m1_request_round_trips` :79, `SessionPorts` line :242); `message.rs:19-23` needs `WatchReleaseParams` in its `use crate::files::{…}` list (missing in the old plan);
> - `Hub::working_sessions` moved to Task 9 (private and unused until then: `-D warnings` would fail Task 8's clippy);
> - phone access (ADR 0052) checked: `mk web` relays requests and events unchanged, `web/app.mjs` ignores `git.changed` and never sends `fs.*` / `git.*` / `doc.*`, so it neither depends on the 2 s cadence nor keeps watchers; `Hello` takes `PROTOCOL_VERSION` from the crate, so the bump needs no relay change.

Inbox: line 126 (bullet B5; do not tick yet). Spec: §7 (SPEC.md:513 `session.ports` row, :539-540 `doc.*` rows, :581 `git.changed`), §12B (SPEC.md:1075), ADR 0056 (new).

**Current state (investigated):**
- `watch::watch` (`crates/mkrew-daemon/src/watch.rs:135-149`) starts one `RecursiveMode::Recursive` watcher over the project root. Every event path goes to `debounce` (:153), which filters afterwards with `Filter::relative_with` (`.git`, `node_modules`, `target`, `.gitignore`; Markdown is kept even when git-ignored, :73-75) and `Filter::is_git_state` (`HEAD`, `index`, `packed-refs`, `refs/…` of the root or of a direct child `.git`). `notify` 8 has no per-directory exclude, so the recursion stays; a `.git`-only storm already costs no `stat` (`relative_with` returns before `is_dir` for ignored dirs).
- `debounce` already delivers nothing for an empty batch: `if !batch.is_empty() || git { on_batch(…) }` (:183). The closure in `Hub::watch` (`session.rs:663-676`) sends `Event::GitChanged` for every delivered batch; today a delivered batch always has files or `git`, so this is not a leak, but the send condition is made explicit. The real cost is the cadence: a non-empty batch is cut at most every `MAX_DELAY` = 2 s while files keep changing (an agent writing all the time ⇒ a `git status` per UI client every 2 s; the UI re-asks on `git.changed`, `shell/connection.rs:679-688`).
- `MAX_DELAY` (`watch.rs:18`, 2 s) is read in `debounce` (`let deadline = tokio::time::Instant::now() + MAX_DELAY;`, :174). The only user outside `watch.rs` is a comment in `tests/daemon/docs.rs:152` (`two_sessions_editing_one_file_raise_a_conflict`).
- Where `working` is known: `Actor::set_state` (`session.rs:1330`) is the single place a session's state is assigned (`self.session.state = state;`, :1332); the exit goes through it too (`finish`, :1511, after the session left `live`, :1485). The Hub keeps no per-session state (the store row is what `files_changed` reads with `list_sessions`, :731). This task adds `Hub::working: Mutex<HashMap<Ulid, Ulid>>` (session → project), updated in `set_state`, used by the watcher's delay and by Task 9.
- Kept watchers: `Hub::watch` (`session.rs:649`) stores `ProjectWatch { _watch, kept }` (struct :189) in `Hub::watches`; `kept |= true` by `keep_watched` (:621; callers `server/doc.rs:87` `watched_project` for `doc.list`/`doc.read`, `server/target.rs:134` `watched_target` for `fs.list`/`fs.read`/`fs.find`/`git.status`…) and `keep_worktree_watched` (:629, key = task id, same `watched_target`, `target.rs:137`). Nothing clears `kept`: only `unwatch_worktree` (:635, task worktree removal, `server/task_worktree.rs`) and `unwatch_idle_project` (:692, last session exit, only when not kept) remove a watch.
- How the UI shows a view: `Shell::show_view` (`shell/workspace.rs:343`) sets `self.views[project]`, and for Docs / Files / Git calls `load_docs` (`doc.list`), `refresh_files` (`fs.list`), `refresh_git` (`git.status`) when it was not already shown; `activate_workspace` (`workspace.rs:21`) does the same on a workspace switch (`open_session`, `run.rs:69/100` set `views` to Terminals and then call it); `set_files_target` (`shell/files.rs:26`, ends in `refresh_files`) and `retarget_git` (`shell/git.rs:71`; callers `set_git_target`, `set_git_repo`, `review_diff`, each followed by `refresh_git` or `show_view`) change the task-worktree target. The title-bar summary (`load_git_summary`, `git.rs:156`) asks `git.status` for the shown workspace, so a shown workspace always keeps its project watcher alive (needed: the title bar follows `git.changed`). The Quick-open dialog's `fs.find` on a worktree target (`navigation.rs:14`) also keeps that worktree watcher; the UI does not track it (it stays until the daemon stops, as today).
- Existing protocol has no request that says a view closed (`Request` variants: no `view.*`, `watch.*`; `session.ports` / `session.usage` take `ProjectRef`). `ProjectRef { project_id }` (`mkrew-protocol/src/files.rs:14`) cannot name a worktree target, so a new request is needed: `watch.release` (protocol 40 → 41).
- Existing tests: `watch.rs` `mod tests` (`debounce_batches_until_quiet_and_caps_the_delay` :313, `debounce_flags_git_state_changes_without_files` :400, `watcher_reports_project_changes_but_not_ignored_ones` :433, `start_paused` virtual time); `session.rs` `mod tests` (:1591, `use super::*`, only pacer / flood tests, no Hub fixture); `message/tests.rs` (`every_m1_request_round_trips` :79, `sid()` :21, `request_wire_shape_matches_spec` :48, `param_less_requests_omit_params` :67).

**Files:**
- Modify: `crates/mkrew-daemon/src/watch.rs` (`MAX_DELAY_WORKING`, `max_delay`, `watch`, `debounce`, tests)
- Modify: `crates/mkrew-daemon/src/session.rs` (`Hub` fields + `new`, `set_working`, `has_working`, `release_watch`, `Hub::watch`, `Actor::set_state`, docs, tests)
- Modify: `crates/mkrew-daemon/src/server.rs` (dispatch arm :437), `crates/mkrew-daemon/src/server/target.rs` (`watch_release`)
- Modify: `crates/mkrew-protocol/src/files.rs` (`WatchReleaseParams`), `crates/mkrew-protocol/src/message.rs` (import, `Request::WatchRelease`, `GitChanged` doc), `crates/mkrew-protocol/src/message/tests.rs`, `crates/mkrew-protocol/src/lib.rs` (`PROTOCOL_VERSION`)
- Create: `crates/mkrew-ui/src/watches.rs`; Modify: `crates/mkrew-ui/src/lib.rs` (`pub mod watches;`), `crates/mkrew-ui/src/main.rs` (`use mkrew_ui::watches;`, `Shell.watch_needs`), `crates/mkrew-ui/src/shell/connection.rs` (initializer), `crates/mkrew-ui/src/shell/workspace.rs` (`sync_watches`, two call sites), `crates/mkrew-ui/src/shell/files.rs`, `crates/mkrew-ui/src/shell/git.rs` (call sites)
- Modify: `crates/mkrew-daemon/tests/daemon/docs.rs` (comment at :151-152)
- Create: `docs/decisions/0056-release-kept-watchers.md`
- Modify: `docs/SPEC.md`, `docs/HUMAN_CHECKS.md`

**Interfaces:**
- Produces: `watch::MAX_DELAY_WORKING: Duration` (5 s), `watch::max_delay(working: bool) -> Duration`, `watch::watch(root: &Path, max_delay: impl Fn() -> Duration + Send + 'static, on_batch: impl Fn(Batch) + Send + 'static) -> notify::Result<Watch>`.
- Produces: `Hub::set_working(&self, id: Ulid, project_id: Ulid, working: bool)` (private, `fn`), `Hub::has_working(&self, project_id: Ulid) -> bool` (private), `Hub::release_watch(&self, project_id: Ulid, worktree_task_id: Option<Ulid>)` (pub). Task 9 adds `Hub::working_sessions(&self, project_id: Ulid) -> Vec<Ulid>` on top of the same `working` map.
- Produces: `mkrew_protocol::WatchReleaseParams { project_id: Ulid, worktree_task_id: Option<Ulid> }` and `Request::WatchRelease(WatchReleaseParams)` (wire `watch.release`, result `null`). `PROTOCOL_VERSION` = 41.
- Produces (UI): `watches::Needs::of(workspace: Option<Ulid>, view: View, files: Option<Ulid>, git: Option<Ulid>) -> Needs`, `Needs::released(&self, now: &Needs) -> Vec<WatchReleaseParams>`.

- [ ] **Step 1: Failing tests (watch.rs, Hub, protocol)**

In `crates/mkrew-daemon/src/watch.rs` `mod tests`, add at the end (before the closing `}`):

```rust
    /// Sends one change every 100 ms from now on (the stream never goes quiet) and returns
    /// how long after the first change the batch was cut.
    async fn time_to_cut(
        tx: &mpsc::UnboundedSender<Vec<PathBuf>>,
        root: &Path,
        out: &mut mpsc::UnboundedReceiver<tokio::time::Instant>,
    ) -> Duration {
        let start = tokio::time::Instant::now();
        for i in 0..100 {
            tx.send(vec![root.join(format!("f{i}"))]).unwrap();
            tokio::time::sleep(Duration::from_millis(100)).await;
            if let Ok(at) = out.try_recv() {
                return at - start;
            }
        }
        panic!("no batch after 10 s of changes");
    }

    /// A stream of changes is cut after 5 s while a session is working and after 2 s when it
    /// is not; the flag is read when each batch starts (perf B5).
    #[tokio::test(start_paused = true)]
    async fn a_working_session_stretches_the_cap_to_five_seconds() {
        use std::sync::Arc;
        use std::sync::atomic::{AtomicBool, Ordering};

        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_path_buf();
        let working = Arc::new(AtomicBool::new(true));
        let flag = Arc::clone(&working);
        let (tx, rx) = mpsc::unbounded_channel();
        let (out_tx, mut out) = mpsc::unbounded_channel();
        tokio::spawn(debounce(
            Filter::new(&root),
            rx,
            move || max_delay(flag.load(Ordering::SeqCst)),
            move |_: Batch| {
                let _ = out_tx.send(tokio::time::Instant::now());
            },
        ));

        assert_eq!(
            time_to_cut(&tx, &root, &mut out).await,
            Duration::from_secs(5)
        );
        // Let the change sent at the deadline go out in its own batch, then start clean.
        tokio::time::sleep(Duration::from_secs(1)).await;
        while out.try_recv().is_ok() {}

        working.store(false, Ordering::SeqCst);
        assert_eq!(
            time_to_cut(&tx, &root, &mut out).await,
            Duration::from_secs(2)
        );
    }

    /// Events that only touch git internals (objects, lock files, logs) or ignored folders
    /// deliver nothing: no batch, so no `git.changed`.
    #[tokio::test(start_paused = true)]
    async fn git_internals_and_ignored_folders_alone_deliver_no_batch() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_path_buf();
        let (tx, rx) = mpsc::unbounded_channel();
        let (out_tx, mut out) = mpsc::unbounded_channel();
        let task = tokio::spawn(debounce(Filter::new(&root), rx, || MAX_DELAY, move |batch| {
            let _ = out_tx.send(batch);
        }));
        tx.send(vec![
            root.join(".git/objects/ab/cd"),
            root.join(".git/index.lock"),
            root.join(".git/logs/HEAD"),
            root.join("target/debug/x.o"),
            root.join("node_modules/a/b.js"),
        ])
        .unwrap();
        tokio::time::sleep(QUIET * 3).await;
        assert!(out.try_recv().is_err(), "an empty batch was delivered");

        tx.send(vec![root.join("a.rs")]).unwrap();
        drop(tx);
        task.await.unwrap();
        assert_eq!(out.recv().await.unwrap().files, ["a.rs"]);
    }
```

Update the three existing call sites in the same `mod tests`:
- `debounce_batches_until_quiet_and_caps_the_delay` (:318): replace `tokio::spawn(debounce(Filter::new(&root), rx, move |batch: Batch| {` with `tokio::spawn(debounce(Filter::new(&root), rx, || MAX_DELAY, move |batch: Batch| {`;
- `debounce_flags_git_state_changes_without_files` (:405): replace `tokio::spawn(debounce(Filter::new(&root), rx, move |batch| {` with `tokio::spawn(debounce(Filter::new(&root), rx, || MAX_DELAY, move |batch| {`;
- `watcher_reports_project_changes_but_not_ignored_ones` (:442): replace `let _watch = watch(&root, move |batch| {` with `let _watch = watch(&root, || MAX_DELAY, move |batch| {`.

Hub tests: in `crates/mkrew-daemon/src/session.rs` `mod tests` (:1591; ends with `a_session_floods_past_the_bytes_of_one_window`), add at the end:

```rust
    fn hub() -> (tempfile::TempDir, Arc<Hub>, Ulid) {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(&dir.path().join("mkrew.db")).unwrap();
        let project = store.add_project(dir.path(), Some("repo")).unwrap();
        (dir, Arc::new(Hub::new(store)), project.id)
    }

    fn watched(hub: &Hub, key: Ulid) -> bool {
        lock(&hub.watches).contains_key(&key)
    }

    /// A session of the project registered as live (no task behind it).
    fn live(hub: &Hub, project_id: Ulid) -> Ulid {
        let id = Ulid::new();
        let (commands, _rx) = mpsc::channel(8);
        let (input, _input_rx) = mpsc::channel(1);
        lock(&hub.live).insert(
            id,
            Live {
                project_id,
                commands,
                input: Arc::new(tokio::sync::Mutex::new(input)),
            },
        );
        id
    }

    /// Docs / Git opened → kept; the view closing releases it at once, except while a session
    /// of the project is live (its exit drops the watch then, as it always did).
    #[tokio::test]
    async fn releasing_a_kept_project_watch_drops_it_unless_a_session_lives() {
        let (dir, hub, project) = hub();
        hub.keep_watched(project, dir.path());
        assert!(watched(&hub, project));
        hub.release_watch(project, None);
        assert!(!watched(&hub, project), "released and nothing runs");

        hub.keep_watched(project, dir.path());
        let session = live(&hub, project);
        hub.release_watch(project, None);
        assert!(watched(&hub, project), "a live session still needs it");
        lock(&hub.live).remove(&session);
        hub.unwatch_idle_project(project);
        assert!(!watched(&hub, project), "no longer kept: its last exit drops it");
    }

    /// A task worktree's watcher is only kept for its Files / Git view: release drops it, and
    /// releasing something never watched is a no-op.
    #[tokio::test]
    async fn releasing_a_worktree_watch_drops_it() {
        let (dir, hub, project) = hub();
        let task = Ulid::new();
        hub.keep_worktree_watched(project, task, dir.path());
        assert!(watched(&hub, task));
        hub.release_watch(project, Some(task));
        assert!(!watched(&hub, task));
        hub.release_watch(project, Some(Ulid::new()));
        hub.release_watch(Ulid::new(), None);
    }

    /// `set_working` tracks per project; two sessions of one project count once they stop.
    #[test]
    fn has_working_follows_the_sessions_of_each_project() {
        let (_dir, hub, project) = hub();
        let (a, b, other) = (Ulid::new(), Ulid::new(), Ulid::new());
        assert!(!hub.has_working(project));
        hub.set_working(a, project, true);
        hub.set_working(b, project, true);
        assert!(hub.has_working(project));
        assert!(!hub.has_working(other));
        hub.set_working(a, project, false);
        assert!(hub.has_working(project), "b still works");
        hub.set_working(b, project, false);
        assert!(!hub.has_working(project));
    }
```

(`hub()` is a plain `Hub::new`; it needs no runtime, so the last test is a plain `#[test]`.)

Protocol tests: in `crates/mkrew-protocol/src/message/tests.rs` `every_m1_request_round_trips`, after the line `Request::SessionPorts(ProjectRef { project_id: sid() }),` (:242) add:

```rust
        Request::WatchRelease(WatchReleaseParams {
            project_id: sid(),
            worktree_task_id: Some(sid()),
        }),
```

and after `param_less_requests_omit_params` (:67-76) add:

```rust
/// `worktree_task_id` may be left out: the project's own watcher.
#[test]
fn watch_release_without_a_worktree_means_the_project_folder() {
    let wire = json!({
        "type": "request", "id": 3, "method": "watch.release",
        "params": {"project_id": sid().to_string()}
    });
    let Message::Request { request, .. } = serde_json::from_value(wire).unwrap() else {
        panic!("a request");
    };
    assert_eq!(
        request,
        Request::WatchRelease(WatchReleaseParams {
            project_id: sid(),
            worktree_task_id: None,
        })
    );
}
```

(The module is `use super::*;` of `message.rs`, so `WatchReleaseParams` comes from the import added in Step 4.)

Run: `cargo test -p mkrew-daemon --lib watch -j 4` and `cargo test -p mkrew-protocol -j 4`
Expected: FAIL to compile (`max_delay`, new `debounce`/`watch` signatures, `release_watch`, `set_working`, `has_working`, `WatchReleaseParams`, `WatchRelease` do not exist).

- [ ] **Step 2: `watch.rs`: delay by working state**

After `pub const MAX_DELAY: Duration = Duration::from_secs(2);` (:18) add:

```rust
/// [`MAX_DELAY`] while a session of the project is `working`: an agent writes files all the
/// time, and every batch makes each UI client run `git status` (perf B5).
pub const MAX_DELAY_WORKING: Duration = Duration::from_secs(5);

/// The longest a batch may wait after its first change.
pub fn max_delay(working: bool) -> Duration {
    if working { MAX_DELAY_WORKING } else { MAX_DELAY }
}
```

Replace the doc comment and signature of `watch` (:130-135):

```rust
/// reported too (T5.6 attributes such paths like files). Non-UTF-8 paths are skipped. Must run
/// inside the tokio runtime.
pub fn watch(root: &Path, on_batch: impl Fn(Batch) + Send + 'static) -> ::notify::Result<Watch> {
```

with

```rust
/// reported too (T5.6 attributes such paths like files). Non-UTF-8 paths are skipped.
/// `max_delay` is asked when a batch starts (see [`max_delay`]); a batch with nothing to report
/// after filtering is never delivered. Must run inside the tokio runtime.
pub fn watch(
    root: &Path,
    max_delay: impl Fn() -> Duration + Send + 'static,
    on_batch: impl Fn(Batch) + Send + 'static,
) -> ::notify::Result<Watch> {
```

Replace `tokio::spawn(debounce(Filter::new(root), rx, on_batch));` (:147) with `tokio::spawn(debounce(Filter::new(root), rx, max_delay, on_batch));`.

Replace the `debounce` doc comment and signature (:151-157):

```rust
/// Collects paths until [`QUIET`] passes without a change (or [`MAX_DELAY`] since the first
/// one), then delivers the batch. Ends when the watcher is dropped.
async fn debounce(
    filter: Filter,
    mut rx: mpsc::UnboundedReceiver<Vec<PathBuf>>,
    on_batch: impl Fn(Batch),
) {
```

with

```rust
/// Collects paths until [`QUIET`] passes without a change (or `max_delay()` since the first
/// one), then delivers the batch. Ends when the watcher is dropped.
async fn debounce(
    filter: Filter,
    mut rx: mpsc::UnboundedReceiver<Vec<PathBuf>>,
    max_delay: impl Fn() -> Duration,
    on_batch: impl Fn(Batch),
) {
```

and replace `let deadline = tokio::time::Instant::now() + MAX_DELAY;` (:174) with `let deadline = tokio::time::Instant::now() + max_delay();`.

- [ ] **Step 3: Hub: working map, delay, release**

`crates/mkrew-daemon/src/session.rs`, `Hub` struct: after the `watches` field (:79, `watches: Mutex<HashMap<Ulid, ProjectWatch>>,`) add:

```rust
    /// Sessions in `working` (any kind) and their project, kept by `Actor::set_state`: the
    /// watcher's delay and the attribution of a batch read it instead of the store. Never held
    /// with another lock.
    working: Mutex<HashMap<Ulid, Ulid>>,
```

In `Hub::new` (:320), after `watches: Mutex::default(),` add `working: Mutex::default(),`.

After `unwatch_worktree` (:635-638) add:

```rust
    /// A client closed the view that kept the watcher of project `project_id` (or of the
    /// worktree of task `worktree_task_id`): stops it unless it is not kept anymore only
    /// because a session of the project is live (that session's exit drops it then, see
    /// [`Hub::unwatch_idle_project`]).
    pub fn release_watch(&self, project_id: Ulid, worktree_task_id: Option<Ulid>) {
        tracing::debug!(%project_id, ?worktree_task_id, "watch released");
        match worktree_task_id {
            Some(task_id) => self.unwatch_worktree(task_id),
            None => {
                if let Some(watch) = lock(&self.watches).get_mut(&project_id) {
                    watch.kept = false;
                }
                self.unwatch_idle_project(project_id);
            }
        }
    }

    /// Records whether session `id` of `project_id` is `working`.
    fn set_working(&self, id: Ulid, project_id: Ulid, working: bool) {
        let mut sessions = lock(&self.working);
        if working {
            sessions.insert(id, project_id);
        } else {
            sessions.remove(&id);
        }
    }

    /// Is a session of the project `working`?
    fn has_working(&self, project_id: Ulid) -> bool {
        lock(&self.working).values().any(|&p| p == project_id)
    }
```

Lock order: `release_watch` takes `watches`, drops it (temporary), then `unwatch_idle_project` takes `watches` then `live`: the documented order holds. `unwatch_worktree` needs no `kept` change: the worktree watch is only ever kept.

Doc updates: `ProjectWatch` (:186-188) from `so it stays watched after its last session exits, until the daemon stops (HUMAN_CHECKS T6.4).` to `so it stays watched after its last session exits, until the client releases it (`watch.release`, ADR 0056) or the daemon stops (HUMAN_CHECKS T6.4).`; `keep_watched` (:619-620) and `keep_worktree_watched` (:625-628): replace `until the daemon stops` with `until released (`Hub::release_watch`) or the daemon stops`.

`Hub::watch` (:662-676): replace

```rust
        let hub = Arc::downgrade(self);
        let started = watch::watch(root, move |batch| {
            if let Some(hub) = hub.upgrade() {
                if !batch.files.is_empty() {
                    hub.file_index.invalidate(key);
                    if worktree_task_id.is_none() {
                        hub.files_changed(project_id, &batch.files);
                    }
                }
                let _ = hub.events.send(Event::GitChanged(GitChangedEvent {
                    project_id,
                    worktree_task_id,
                }));
            }
        });
```

with

```rust
        let hub = Arc::downgrade(self);
        let delay_hub = Arc::downgrade(self);
        let max_delay = move || {
            watch::max_delay(
                delay_hub
                    .upgrade()
                    .is_some_and(|hub| hub.has_working(project_id)),
            )
        };
        let started = watch::watch(root, max_delay, move |batch| {
            if let Some(hub) = hub.upgrade() {
                if !batch.files.is_empty() {
                    hub.file_index.invalidate(key);
                    if worktree_task_id.is_none() {
                        hub.files_changed(project_id, &batch.files);
                    }
                }
                // `watch` only delivers a batch that has files or a git-state change.
                if !batch.files.is_empty() || batch.git {
                    let _ = hub.events.send(Event::GitChanged(GitChangedEvent {
                        project_id,
                        worktree_task_id,
                    }));
                }
            }
        });
```

`Actor::set_state` (:1330-1332): after `self.session.state = state;` add

```rust
        self.hub.set_working(
            self.session.id,
            self.session.project_id,
            state == SessionState::Working,
        );
```

The `Hub::watch` doc (`Every batch also broadcasts `git.changed`: a reported file or the git state changed.`, :641-642) stays true.

- [ ] **Step 4: Protocol and daemon request**

`crates/mkrew-protocol/src/files.rs`, after `ProjectRef` (:12-16):

```rust
/// Params of `watch.release`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WatchReleaseParams {
    pub project_id: Ulid,
    /// The task worktree whose watcher is released; `None` = the project folder.
    #[serde(default)]
    pub worktree_task_id: Option<Ulid>,
}
```

`message.rs`: add `WatchReleaseParams` to the `use crate::files::{…}` list (:19-23; after `ProjectRef`). In `Request`, after the `SessionPorts(ProjectRef),` variant (:256) add:

```rust
    /// Returns nothing (`null`): the client closed the view (Docs, Files, Git, title bar) that
    /// kept the project's file watcher — or, with `worktree_task_id`, that task worktree's —
    /// running. The watcher stops unless a session of the project is live (SPEC §8, ADR 0056).
    #[serde(rename = "watch.release")]
    WatchRelease(WatchReleaseParams),
```

Update the `GitChanged` doc (:368) from `The git state of a workspace changed (debounced, ≤ once per 300 ms; SPEC §12B).` to `The git state of a workspace changed (debounced: 300 ms quiet, at most 2 s after the first change, 5 s while a session of the project is working; never sent for a batch with nothing to report; SPEC §12B).`

`lib.rs:16`: `pub const PROTOCOL_VERSION: u32 = 40;` → `= 41;`. Nothing else hard-codes 40: `grep -rn "protocol 40\|PROTOCOL_VERSION: u32" crates docs` shows only this constant (every client and the daemon tests use the constant).

`server.rs`, next to `Request::SessionPorts(params) => {…}` (:437-439) add the arm:

```rust
            Request::WatchRelease(params) => (self.watch_release(params), After::Continue),
```

`server/target.rs`: change `use mkrew_protocol::{ErrorCode, Outcome, Project};` (:7) to `use mkrew_protocol::{ErrorCode, Outcome, Project, WatchReleaseParams};` and `use super::{Daemon, store_outcome, task_dir};` (:10) to `use super::{Daemon, ok, store_outcome, task_dir};`; at the end of `impl Daemon` (after `watched_target`) add:

```rust
    /// `watch.release`: the client left the view that kept this watcher (ADR 0056).
    pub(super) fn watch_release(&self, params: WatchReleaseParams) -> Outcome {
        self.hub
            .release_watch(params.project_id, params.worktree_task_id);
        ok(&())
    }
```

Run: `cargo test -p mkrew-protocol -j 4` and `cargo test -p mkrew-daemon --lib -j 4`
Expected: PASS (all `watch`, `session`, `backup` unit tests; the round-trip tests).

- [ ] **Step 5: UI: say when a view closes**

Create `crates/mkrew-ui/src/watches.rs`:

```rust
//! Which daemon file watchers this window still needs (SPEC §8, ADR 0056). The daemon keeps a
//! project's watcher after Docs / Files / Git asked for it; [`Needs::released`] names the ones
//! to give back with `watch.release` when the window stops showing what needed them. Pure and
//! tested headlessly; `shell/workspace.rs` sends the requests.

use std::collections::BTreeSet;

use mkrew_protocol::WatchReleaseParams;
use ulid::Ulid;

use crate::board::View;

/// The watchers in use: `(project, None)` = the project folder, `(project, Some(task))` = a
/// task worktree.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Needs(BTreeSet<(Ulid, Option<Ulid>)>);

impl Needs {
    /// What showing `workspace` in `view` uses. The shown workspace always needs its project
    /// watcher (the title bar git summary follows `git.changed`). The Files or Git view adds
    /// its task worktree (`files` / `git` = the target each view has) only while it is shown.
    pub fn of(workspace: Option<Ulid>, view: View, files: Option<Ulid>, git: Option<Ulid>) -> Self {
        let mut needs = BTreeSet::new();
        if let Some(project) = workspace {
            needs.insert((project, None));
            let task = match view {
                View::Files => files,
                View::Git => git,
                View::Board | View::Terminals | View::Docs => None,
            };
            if let Some(task) = task {
                needs.insert((project, Some(task)));
            }
        }
        Self(needs)
    }

    /// The watchers `self` used and `now` does not: one `watch.release` each.
    pub fn released(&self, now: &Needs) -> Vec<WatchReleaseParams> {
        self.0
            .difference(&now.0)
            .map(|&(project_id, worktree_task_id)| WatchReleaseParams {
                project_id,
                worktree_task_id,
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn release(project_id: Ulid, worktree_task_id: Option<Ulid>) -> WatchReleaseParams {
        WatchReleaseParams {
            project_id,
            worktree_task_id,
        }
    }

    /// Switching workspace gives back the old one's project watcher and its worktree watcher.
    #[test]
    fn switching_workspace_releases_the_old_project_and_its_worktree() {
        let (a, b, task) = (Ulid(1), Ulid(2), Ulid(3));
        let before = Needs::of(Some(a), View::Git, None, Some(task));
        let after = Needs::of(Some(b), View::Git, None, None);
        assert_eq!(
            before.released(&after),
            [release(a, None), release(a, Some(task))]
        );
    }

    /// Leaving a worktree's Files / Git view releases only that worktree: the shown project
    /// keeps its watcher for the title bar, and the project folder's own views release nothing.
    #[test]
    fn leaving_a_worktree_view_keeps_the_shown_project() {
        let (a, task) = (Ulid(1), Ulid(3));
        let files = Needs::of(Some(a), View::Files, Some(task), None);
        let terminals = Needs::of(Some(a), View::Terminals, Some(task), None);
        assert_eq!(files.released(&terminals), [release(a, Some(task))]);

        let on_project = Needs::of(Some(a), View::Git, None, None);
        assert!(on_project.released(&terminals).is_empty());
        assert!(terminals.released(&on_project).is_empty());
    }

    /// Each view counts only its own target, and picking another worktree gives back the old.
    #[test]
    fn a_view_counts_its_own_target_and_a_retarget_releases_the_old_one() {
        let (a, t1, t2) = (Ulid(1), Ulid(3), Ulid(4));
        // Files points at a worktree, but the Git view is shown on the project folder.
        assert_eq!(
            Needs::of(Some(a), View::Git, Some(t1), None),
            Needs::of(Some(a), View::Git, None, None)
        );
        let before = Needs::of(Some(a), View::Git, None, Some(t1));
        let after = Needs::of(Some(a), View::Git, None, Some(t2));
        assert_eq!(before.released(&after), [release(a, Some(t1))]);
    }
}
```

(`Ulid(n)` constructs from a `u128`, as `git_view/tests.rs` does; `View` is `crate::board::View`, variants Board / Terminals / Docs / Files / Git.)

`crates/mkrew-ui/src/lib.rs`: add `pub mod watches;` between `pub mod voice;` (:78) and `pub mod workspace_menu;` (:79).

`crates/mkrew-ui/src/main.rs`:
- add `use mkrew_ui::watches;` after `use mkrew_ui::voice;` (:103);
- `Shell` struct: after the field `git_summary_again: bool,` (:504) add
  ```rust
      /// The watchers the daemon keeps for what this window shows (`watches`).
      watch_needs: watches::Needs,
  ```
`crates/mkrew-ui/src/shell/connection.rs`, the `Shell` initializer: after `git_summary_again: false,` (:364) add `watch_needs: watches::Needs::default(),`. (`shell/*.rs` start with `use crate::*;`, which brings `watches`, `Request`, `Outcome`, `Ulid` and `View` in.)

`crates/mkrew-ui/src/shell/workspace.rs`: new method after `show_view`'s closing brace (before the final `}` of the `impl`, :380-381):

```rust
    /// Gives the daemon back the file watchers this window no longer shows anything for
    /// (`watch.release`, [`watches`]). Cheap: it compares two small sets.
    pub(crate) fn sync_watches(&mut self, cx: &mut Context<'_, Self>) {
        let files = (self.workspace)
            .and_then(|id| self.files.get(&id))
            .and_then(|view| view.worktree_task_id);
        let git = (self.workspace)
            .and_then(|id| self.git.get(&id))
            .and_then(|view| view.worktree_task_id);
        let needs = watches::Needs::of(self.workspace, self.view(), files, git);
        let released = self.watch_needs.released(&needs);
        self.watch_needs = needs;
        let Some(connection) = self.connection.clone() else {
            return;
        };
        for params in released {
            let project_id = params.project_id;
            let reply = connection.request(Request::WatchRelease(params));
            cx.spawn(async move |_, _| match reply.await {
                Ok(Outcome::Result(_)) => {}
                Ok(Outcome::Error(body)) => {
                    tracing::warn!(%project_id, "watch.release failed: {}", body.message);
                }
                Err(error) => tracing::warn!(%project_id, "watch.release failed: {error:#}"),
            })
            .detach();
        }
    }
```

(Same shape as the `session.unqueue` spawn at `shell/composer.rs:388`.)

Call it where the shown view, workspace or worktree target changes:
- `show_view` (`workspace.rs:343-380`): replace its tail

  ```rust
          if shown != view {
              self.sync_panes(window, cx);
              cx.notify();
          }
      }
  ```
  with
  ```rust
          self.sync_watches(cx);
          if shown != view {
              self.sync_panes(window, cx);
              cx.notify();
          }
      }
  ```
- `activate_workspace` (`workspace.rs:21-53`): add `self.sync_watches(cx);` as the line before its final `self.sync_panes(window, cx);` (:51). `open_session` and the run-tab paths set `views` and then call it, so they are covered.
- `refresh_files` (`shell/files.rs:59`): first line of the body `self.sync_watches(cx);` (covers `set_files_target`, which ends in `refresh_files`).
- `refresh_git` (`shell/git.rs:94`): first line of the body `self.sync_watches(cx);` (covers `set_git_target`, `set_git_repo`; `review_diff` goes through `show_view`).

All four are ordinary `&mut self` methods that already take `cx`. A release is sent before any later request that re-asks (requests go out in call order, `client.rs:164`), and a view that is shown again asks again (`fs.list` / `git.status` / `doc.list` re-keep the watcher), so the order against the new view's request cannot lose a keep.

Run: `cargo test -p mkrew-ui --lib watches -j 4` then `cargo check -p mkrew-ui -j 4`
Expected: PASS and no errors.

- [ ] **Step 6: Existing integration test comment, ADR, SPEC**

`crates/mkrew-daemon/tests/daemon/docs.rs:151-152` (`two_sessions_editing_one_file_raise_a_conflict`): replace

```
    // The first spinner lets the project watcher start (it starts after the first spawn); the
    // second keeps the session `working` until the batch landed (≤ `watch::MAX_DELAY` = 2 s).
```

with

```
    // The first spinner lets the project watcher start (it starts after the first spawn); the
    // second keeps the session `working` until the batch landed (300 ms quiet; a continuous
    // stream would wait ≤ `watch::MAX_DELAY_WORKING`).
```

Run `ls docs/decisions | tail -5` first; create `docs/decisions/0056-release-kept-watchers.md` (template `0000-template.md`; 0056 is fixed by the plan contract, 0055 belongs to Task 6):

```markdown
# 0056 — Release kept file watchers when their view closes

- Date: 2026-10-13
- Status: accepted
- Milestone: none (perf review, inbox item 12)

## Context

- SPEC §7 says `doc.list`, `doc.read`, and (HUMAN_CHECKS T6.4 / T7.4) `fs.*` and `git.status` keep the project watched until the daemon stops. A project the user looked at once keeps a recursive watcher for the whole daemon lifetime, and every batch makes each UI client re-run `git status` (perf review B5).
- There is no request that says a view closed; `ProjectRef` cannot name a task worktree.
- A phone (`mk web`, ADR 0052) is a second client of the daemon, but its page never sends `fs.*`, `git.*` or `doc.*`, so it keeps no watcher.

## Options

1. Keep watchers until the daemon stops (today). Simple; idle CPU, handles and `git status` runs for projects nobody looks at.
2. Daemon-side timeout after the last request. No protocol change, but a view that stays open and idle loses its refresh; needs a heartbeat.
3. New request `watch.release` sent by the UI when it stops showing what needed the watcher. One small request; the UI already knows its shown workspace, view and worktree target.

## Decision

Option 3: `watch.release { project_id, worktree_task_id? }` (protocol 41). The daemon clears the `kept` flag of the project watcher and stops it unless a session of the project is live (its exit then drops it as before); a worktree watcher is stopped at once. The UI keeps the shown workspace's project watcher (the title bar summary follows `git.changed`) and a worktree watcher only while its Files / Git view is shown. There is no reference counting: the app has one UI window.

## Consequences

- SPEC §7: new `watch.release` row, `doc.*` rows say "until released or the daemon stops", `git.changed` debounce text; §12B note. Protocol 40 → 41.
- A second desktop UI window that still shows a released workspace would lose `git.changed` for it until its next `fs.*` / `git.status` / `doc.*` request re-keeps it. Acceptable: only one UI window exists; revisit with a ref count if multi-window arrives.
- The Quick-open dialog's `fs.find` on a worktree keeps that worktree watched and is not released until the daemon stops (as before).
- HUMAN_CHECKS T5.5, T6.4 and T7.4 notes updated.
```

SPEC edits (`docs/SPEC.md`; line numbers of 2026-10-13):
- Add after the `session.ports` row (:513):
  `| `watch.release` | `project_id`, `worktree_task_id?` | ok (`null`) — the UI no longer shows that workspace (or task worktree) and gives back the file watcher that `doc.*`, `fs.*` and `git.status` keep running; it stops unless a session of the project is live. A later request of those kinds keeps it again (ADR 0056) |`
- `:539` and `:540`: replace `Keeps the project watched for `doc.changed` until the daemon stops` with `Keeps the project watched for `doc.changed` until released (`watch.release`) or the daemon stops`; replace `Also keeps the project watched` with `Also keeps the project watched (until released)`.
- `:581`: replace `workspace git state changed (debounced, ≤ once per 300 ms)` with `workspace git state changed (debounced: 300 ms quiet, at most 2 s after the first change, 5 s while a session of the project is working; a batch with nothing to report after filtering sends nothing)`.
- `:1075`: replace `Refresh is triggered by the file watcher (debounced 300 ms)` with `Refresh is triggered by the file watcher (debounced 300 ms, §7 `git.changed`)`.

`docs/HUMAN_CHECKS.md` (re-read each spot; text of 2026-10-13):
- `:2130-2131` (T5.5): replace `Batches = 300 ms quiet, at\n  most 2 s;` with `Batches = 300 ms quiet, at\n  most 2 s (5 s while a session of the project is `working`, ADR 0056);`.
- `:2205-2208` (T6.4): replace `and it then stays until the\n  daemon stops — even after the last session exits (one watcher per project ever opened in\n  the Docs view; no unwatch on client disconnect / archive).` with `and it then stays until the UI gives it back\n  (`watch.release`, ADR 0056: that workspace is no longer shown) or the daemon stops — even after\n  the last session exits (no unwatch on client disconnect / archive).`
- `:2326` (T7.4): replace `` `git.status` keeps the project watched until the daemon stops (like docs). `` with `` `git.status` keeps the project watched until released or the daemon stops (like docs, ADR 0056). ``

- [ ] **Step 7: HUMAN_CHECKS (new bullets)**

Append to the end of the shared perf section:

```markdown
- [ ] **Watchers stop with their view:** start the daemon with `MKREW_LOG=debug`. Add a second workspace B. In B open Files or Git (the Docs view only exists with `[features] docs = true`), then switch to workspace A with no sessions in B: the log shows `watch released` and `stopped watching project files` for B. Open the Git view of a task worktree, then switch to Terminals: `watch released` with that task id. Come back to Git / Files: they refresh and follow changes again (edit a file outside the app: the Git list updates).
- [ ] **`git.changed` rate:** with one agent `working` and writing files, the Git view and the title bar git summary refresh about every 5 s (not every 2 s); once the agent is idle, the next change shows within about 0.5 s. A `git fetch` / commit from outside still shows within ~0.5 s with no agent working. Touching only `.git/objects` (a background `git gc`) changes nothing on screen.
- [ ] **Old UI, new daemon:** after the protocol bump (41) an old `mkrew-ui` gets `version_mismatch` and asks to restart the daemon (expected, SPEC §7). A running `mk web` (phone) must be restarted too: it greets with the protocol of its own build.
```

- [ ] **Step 8: Verify and commit**

Run: `cargo build -p mkrew-cli -p mkrew-fakeagent -j 4` (protocol changed: the daemon tests use these bins), then `cargo test -p mkrew-protocol -p mkrew-daemon -p mkrew-ui --lib -j 4` (expect PASS), then `cargo test -p mkrew-daemon --test daemon -j 4 two_sessions_editing file_watcher_records` (expect PASS), then `sh scripts/verify.sh` (fallback if rustc crashes on mkrew-ui test builds: `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace -j 4`). Do not tick inbox line 126 yet (Task 9 follows).
Commit: `git add crates/mkrew-daemon/src/watch.rs crates/mkrew-daemon/src/session.rs crates/mkrew-daemon/src/server.rs crates/mkrew-daemon/src/server/target.rs crates/mkrew-daemon/tests/daemon/docs.rs crates/mkrew-protocol/src/files.rs crates/mkrew-protocol/src/message.rs crates/mkrew-protocol/src/message/tests.rs crates/mkrew-protocol/src/lib.rs crates/mkrew-ui/src/watches.rs crates/mkrew-ui/src/lib.rs crates/mkrew-ui/src/main.rs crates/mkrew-ui/src/shell/connection.rs crates/mkrew-ui/src/shell/workspace.rs crates/mkrew-ui/src/shell/files.rs crates/mkrew-ui/src/shell/git.rs docs/decisions/0056-release-kept-watchers.md docs/SPEC.md docs/HUMAN_CHECKS.md` (shared files: `session.rs`, `main.rs`, `shell/*.rs`, `SPEC.md`, `HUMAN_CHECKS.md` may hold other tasks' hunks: stage only this task's with `git add -p <path>`). Never push.
Message: `perf(daemon): 5 s watcher cap while working, release kept watchers (protocol 41)`

---

### Task 9: `files_changed` writes only for Markdown, and no `list_sessions` per batch

> **Refresh 2026-10-13:** still needed (`session.rs:711-750` still inserts one `files_changed` row per live session for every batch and calls `store.list_sessions` per batch). Changes vs the 2026-10-10 version:
> - integration tests moved to `crates/mkrew-daemon/tests/daemon/{main,states,turns}.rs` (`changed_files` and `answer_after_watcher_saw_notes` are in `main.rs:565` / `:638`);
> - `Hub::working_sessions` is defined here (Task 8 keeps only `set_working` / `has_working`, see Task 8's refresh note);
> - `Filter::relative_with` now reports Markdown even when git-ignored (`watch.rs:73-75`), so ignored `.md` batches still reach Docs "changed by": no code change, HUMAN_CHECKS T6.4 (:2209) already says so;
> - `[features] docs` is off by default (ADR 0046), so under defaults nothing reads `files_changed` at all; the daemon keeps recording Markdown batches anyway so switching Docs on later still shows recent "changed by" (see Open questions); the `done · N files` summary and turn files still work for any file type because `Command::TurnFiles` does not use the event;
> - SPEC line numbers are now :539 and :1807-1809; the inbox tick is an edit of `docs/INBOX.md` that is NOT staged (agents never commit INBOX).

Inbox: line 126 (bullet B6; this is the last task of item 12: tick `[~]`→`[x]`; the title already carries `→ plan: … Task 7, 8, 9`). Spec: §12 Docs (SPEC.md:539 `doc.list`), §20C item 4 (SPEC.md:1807-1809), HUMAN_CHECKS T5.5/T5.6/T6.4.

**Current state (investigated):**
- `Hub::files_changed` (`crates/mkrew-daemon/src/session.rs:711-750`), called by the project watcher for every non-empty batch (`:667-669`, not for worktree watchers): (1) `docs_changed` (:754) reads and hashes each changed `.md` on the watcher task (only when the batch has a `.md` path, `docs::is_markdown`); (2) collects the live sessions of the project from `Hub::live` (in memory); (3) inserts one `files_changed` session event per live session, **for every batch**, with all paths; (4) calls `store.list_sessions(Some(project_id))` and keeps those `Working` and live; (5) if exactly one is working: `record_edits` (conflict tracker) and `Command::TurnFiles(paths)` to it.
- The only reader of `files_changed` is `Daemon::doc_list` (`server/doc.rs:29`) → `docs::doc_files` (`docs.rs:109`), which keeps only `is_markdown` paths for "last changed by" (`DocFile.last_session_id` / `last_changed_at`; UI comment send target). `session.files` reads `edited` events (hook edits) and turn rows, not `files_changed` (`server/turn.rs:104-115`). Integration tests read it through `changed_files` (`tests/daemon/main.rs:565`), used by `file_watcher_records_changed_files_of_live_sessions_except_ignored_ones` (`states.rs:335`), by `answer_after_watcher_saw_notes` (`main.rs:638`, waits for `notes.txt`; callers `turns.rs:38`, `:246`) and by `docs_are_listed_read_and_external_edits_reanchor_comments` (`docs.rs:9`, `last_session_id` of `notes/todo.md`).
- Task 8 adds `Hub::working: Mutex<HashMap<session, project>>`, `Hub::set_working` (kept by `Actor::set_state`), which has the same content as the old `list_sessions` + `state == Working` filter, minus the SQLite read. Nothing else is needed: `Hub::live` already holds the live session ids. This task adds the reader `Hub::working_sessions`.
- `docs_changed` reads the changed `.md` files on the watcher task. The review flags it, but its output (`doc.changed` + comment re-anchoring) is a feature with its own HUMAN_CHECKS (T6.4) and runs only for `.md` paths; moving it off the task is not part of this change (the read is bounded: ≤ 2 MiB, only paths of the batch).
- `Command::TurnFiles` and `record_edits` do not depend on the `files_changed` event; a non-`.md` batch must still reach them (HUMAN_CHECKS T5.6, turn files: `a_turn_that_edits_two_files_reports_both` expects the watcher-seen `notes` file in the turn, `turns.rs:8`). `record_edits` (`session.rs:874`) still calls `list_sessions` (:887), but only after a conflict was found, not per batch: unchanged.
- Item 13 (Tasks 10–12: feature gates) also touches this function; anchor on the quoted text, not on lines.

**Files:**
- Modify: `crates/mkrew-daemon/src/session.rs` (`Hub::working_sessions`, `Hub::files_changed`; tests)
- Modify: `crates/mkrew-daemon/tests/daemon/states.rs` (`file_watcher_records_changed_files_of_live_sessions_except_ignored_ones`), `crates/mkrew-daemon/tests/daemon/main.rs` (`answer_after_watcher_saw_notes`), `crates/mkrew-daemon/tests/daemon/turns.rs` (`a_turn_that_edits_two_files_reports_both`, `a_hook_turn_start_is_snapshotted_before_the_agent_edits`)
- Modify: `docs/SPEC.md` (:539, :1807-1809), `docs/HUMAN_CHECKS.md` (T5.5 note + shared section), `docs/INBOX.md` (tick line 126, not staged)

**Interfaces:**
- Consumes (Task 8): `Hub::set_working(&self, id: Ulid, project_id: Ulid, working: bool)`, the `working` map.
- Produces: `Hub::working_sessions(&self, project_id: Ulid) -> Vec<Ulid>` (private); `fn files_changed(&self, project_id: Ulid, paths: &[String])` keeps its signature.

- [ ] **Step 1: Failing tests (session.rs)**

In `crates/mkrew-daemon/src/session.rs` `mod tests`, below the Task 8 helpers (`hub`, `watched`, `live`), add:

```rust
    /// A hub with a project and one live session per entry of `states`, each registered the way
    /// `spawn` and `set_state` do (in the store, in `live`, and in `working` when working).
    struct Fixture {
        _dir: tempfile::TempDir,
        hub: Arc<Hub>,
        project: Ulid,
        sessions: Vec<(Ulid, mpsc::Receiver<Command>)>,
    }

    fn fixture(states: &[SessionState]) -> Fixture {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(&dir.path().join("mkrew.db")).unwrap();
        let project = store.add_project(dir.path(), Some("repo")).unwrap().id;
        let hub = Arc::new(Hub::new(store));
        let mut sessions = Vec::new();
        for &state in states {
            let id = Ulid::new();
            hub.store()
                .insert_session(&Session {
                    id,
                    task_id: None,
                    project_id: project,
                    agent_profile: "fake".into(),
                    kind: SessionKind::Agent,
                    title: None,
                    terminal_title: None,
                    color: None,
                    native_session_id: None,
                    state,
                    state_source: StateSource::Daemon,
                    state_changed_at: 0,
                    exit_code: None,
                    cols: 80,
                    rows: 24,
                    started_at: 0,
                    ended_at: None,
                    pinned: false,
                    plan_path: None,
                })
                .unwrap();
            let (commands, rx) = mpsc::channel(8);
            let (input, _input_rx) = mpsc::channel(1);
            lock(&hub.live).insert(
                id,
                Live {
                    project_id: project,
                    commands,
                    input: Arc::new(tokio::sync::Mutex::new(input)),
                },
            );
            hub.set_working(id, project, state == SessionState::Working);
            sessions.push((id, rx));
        }
        Fixture {
            _dir: dir,
            hub,
            project,
            sessions,
        }
    }

    fn recorded(f: &Fixture, session: Ulid) -> Vec<serde_json::Value> {
        let events = f.hub.store().session_events(session, "files_changed");
        events.unwrap().into_iter().map(|e| e.payload).collect()
    }

    /// Only Docs reads `files_changed`: a batch of source files writes no row, however many
    /// sessions are live (perf B6).
    #[tokio::test]
    async fn a_batch_without_markdown_writes_no_event() {
        let f = fixture(&[SessionState::Working, SessionState::Idle]);
        f.hub
            .files_changed(f.project, &["src/lib.rs".to_owned(), "notes.txt".to_owned()]);
        for (id, _) in &f.sessions {
            assert!(recorded(&f, *id).is_empty());
        }
    }

    /// A batch with Markdown is recorded on every live session with just its Markdown paths
    /// (any case of the extension), in batch order.
    #[tokio::test]
    async fn a_markdown_batch_is_recorded_on_every_live_session_with_only_its_md_paths() {
        let f = fixture(&[SessionState::Working, SessionState::Idle]);
        let batch = ["README.MD", "docs/a.md", "src/lib.rs"].map(str::to_owned);
        f.hub.files_changed(f.project, &batch);
        for (id, _) in &f.sessions {
            assert_eq!(
                recorded(&f, *id),
                [serde_json::json!({"paths": ["README.MD", "docs/a.md"]})]
            );
        }
    }

    /// The one working session is the editor of every batch (also one with no Markdown); the
    /// others are not told. Two working: nobody is blamed; when one stops working
    /// (`set_working`), the other is the editor.
    #[tokio::test]
    async fn the_only_working_session_gets_the_batch_for_its_turn() {
        let mut f = fixture(&[SessionState::Idle, SessionState::Working]);
        let paths = vec!["src/lib.rs".to_owned()];
        f.hub.files_changed(f.project, &paths);
        assert!(f.sessions[0].1.try_recv().is_err(), "idle session blamed");
        assert!(matches!(
            f.sessions[1].1.try_recv(),
            Ok(Command::TurnFiles(p)) if p == paths
        ));

        let mut f = fixture(&[SessionState::Working, SessionState::Working]);
        f.hub.files_changed(f.project, &paths);
        for (_, rx) in &mut f.sessions {
            assert!(rx.try_recv().is_err(), "two working: the editor is unknown");
        }
        let first = f.sessions[0].0;
        f.hub.set_working(first, f.project, false);
        f.hub.files_changed(f.project, &paths);
        assert!(f.sessions[0].1.try_recv().is_err());
        assert!(matches!(f.sessions[1].1.try_recv(), Ok(Command::TurnFiles(_))));
    }
```

(`Session`, `SessionKind`, `SessionState`, `StateSource` come from the `use mkrew_protocol::{…}` at the top of `session.rs` (:16-21); `Command::TurnFiles(Vec<String>)` is the variant the actor handles at :1121. If a name is missing from the parent's imports, add it to the test module's `use`.)

`crates/mkrew-daemon/tests/daemon/states.rs`, `file_watcher_records_changed_files_of_live_sessions_except_ignored_ones` (:335-370): rename it `file_watcher_records_only_markdown_files_of_live_sessions_except_ignored_ones`; replace

```rust
    for dir in ["src", "target", "node_modules"] {
```
with
```rust
    for dir in ["src", "target", "node_modules", "notes"] {
```
replace
```rust
    // Ignored writes first: once `src/lib.rs` is recorded, their batch has been handled too.
```
with
```rust
    // Ignored writes first: once `notes/todo.md` is recorded, their batch has been handled too.
    // `src/lib.rs` is watched but not Markdown: Docs is the only reader, so it is not recorded.
```
replace `["target/a.o", "node_modules/b.js", "debug.log", "src/lib.rs"]` with `["target/a.o", "node_modules/b.js", "debug.log", "src/lib.rs", "notes/todo.md"]`, and replace the three occurrences of `src/lib.rs` in the wait and the assertion (`.contains("src/lib.rs")`, `"src/lib.rs never recorded"`, `BTreeSet::from(["src/lib.rs".to_owned()])`) with `notes/todo.md`.

Turn tests that waited for the watcher through `files_changed` must now wait on a Markdown file:
- `tests/daemon/main.rs:636-641` (`answer_after_watcher_saw_notes`) and its doc comment: replace `notes.txt` with `notes.md` (three places: the doc comment line 637, `.contains("notes.txt")`, `"notes.txt never recorded"`).
- `tests/daemon/turns.rs`, `a_turn_that_edits_two_files_reports_both` (:8): in the scenario string (:14) replace `path = \"notes.txt\"` with `path = \"notes.md\"`; in the assertions (:53-54) replace the comment `// `notes.txt` was written without a hook: only the file watcher saw it.` with the same text naming `notes.md`, and `["a.rs", "notes.txt", "src/b.rs"]` with `["a.rs", "notes.md", "src/b.rs"]`.
- `turns.rs`, `a_hook_turn_start_is_snapshotted_before_the_agent_edits` (:208): in the scenario (:225) replace `path = \"notes.txt\"` with `path = \"notes.md\"`, and in the counts assertion (:260) `("notes.txt", Some(1), Some(0))` with `("notes.md", Some(1), Some(0))` (sorted order stays `a.txt` < `notes.md`). The other `notes.txt` uses in `turns.rs` (:329 onward) write the file from the test itself and do not wait on the watcher: leave them.

Run: `cargo test -p mkrew-daemon --lib session::tests::a_batch -j 4 -- --nocapture` and `cargo test -p mkrew-daemon --lib the_only_working -j 4`
Expected: compiles once Task 8 is in (`set_working` comes from it). `a_batch_without_markdown_writes_no_event` and `a_markdown_batch_is_recorded…` FAIL (the first finds a row per session; the second finds all three paths). `the_only_working_session_gets_the_batch_for_its_turn` PASSES against today's code (the fixture stores `Working` in the row and in the map): it pins that the in-memory list replaces the store read.

- [ ] **Step 2: `files_changed`: Markdown only, working list from memory**

`crates/mkrew-daemon/src/session.rs`: after `Hub::has_working` (Task 8) add:

```rust
    /// The sessions of the project in `working`, from memory.
    fn working_sessions(&self, project_id: Ulid) -> Vec<Ulid> {
        lock(&self.working)
            .iter()
            .filter(|&(_, &p)| p == project_id)
            .map(|(&id, _)| id)
            .collect()
    }
```

Replace the whole of `Hub::files_changed`, from its doc comment (:707)

```rust
    /// Records a debounced batch of changed files on every live session of the project
    /// (session event `files_changed`, payload `{"paths": [...]}`), and attributes the changes
    /// for conflict detection to the project's session in `working`. With several sessions
    /// working the editor is unknown, so the batch is not attributed (HUMAN_CHECKS T5.6).
    fn files_changed(&self, project_id: Ulid, paths: &[String]) {
```

through its closing `}` (the line before `/// [`Hub::doc_seen`] for each changed Markdown file.`, :750), with:

```rust
    /// Handles a debounced batch of changed files of the project. The Markdown ones are
    /// recorded on every live session (session event `files_changed`, payload
    /// `{"paths": [...]}`): Docs "changed by" is the only reader, so a batch without Markdown
    /// writes nothing. The batch is attributed for conflict detection and to the turn of the
    /// project's session in `working` (from memory: [`Hub::working_sessions`]). With several
    /// sessions working the editor is unknown, so the batch is not attributed (HUMAN_CHECKS T5.6).
    fn files_changed(&self, project_id: Ulid, paths: &[String]) {
        self.docs_changed(project_id, paths);
        let sessions: Vec<Ulid> = lock(&self.live)
            .iter()
            .filter(|(_, live)| live.project_id == project_id)
            .map(|(id, _)| *id)
            .collect();
        // A batch flushed after the last session exited.
        if sessions.is_empty() {
            return;
        }
        let at = crate::server::now_ms();
        let docs: Vec<&str> = paths
            .iter()
            .map(String::as_str)
            .filter(|path| docs::is_markdown(path))
            .collect();
        if !docs.is_empty() {
            let payload = serde_json::json!({ "paths": docs });
            let store = self.store();
            for &id in &sessions {
                if let Err(e) = store.insert_session_event(id, "files_changed", &payload, at) {
                    tracing::warn!(%id, "files_changed not recorded: {e}");
                }
            }
        }
        let working: Vec<Ulid> = self
            .working_sessions(project_id)
            .into_iter()
            .filter(|id| sessions.contains(id))
            .collect();
        if let [editor] = working[..] {
            self.record_edits(project_id, paths, editor, at);
            // A batch flushed after the turn ended is dropped by the tracker (HUMAN_CHECKS T9.5).
            if !self.command(editor, Command::TurnFiles(paths.to_vec())) {
                tracing::debug!(%editor, "changed files not added to the turn");
            }
        }
    }
```

`SessionState` stays imported (used elsewhere in the file). `Store::list_sessions` is no longer called per batch.

Run: `cargo test -p mkrew-daemon --lib -j 4`
Expected: PASS (the three new tests included).

- [ ] **Step 3: Integration tests**

Run: `cargo build -p mkrew-cli -p mkrew-fakeagent -j 4`, then `cargo test -p mkrew-daemon --test daemon -j 4 file_watcher_records_only docs_are_listed a_turn_that_edits a_hook_turn_start two_sessions_editing`
Expected: PASS. `docs_are_listed_read_and_external_edits_reanchor_comments` is the Docs "changed by" proof: the agent writes `notes/todo.md` and `doc.list` still reports `last_session_id == the working session`. `two_sessions_editing_one_file_raise_a_conflict` proves `record_edits` still runs for non-Markdown `shared.txt` without a `files_changed` row.

- [ ] **Step 4: SPEC and HUMAN_CHECKS**

`docs/SPEC.md`:
- `:539` (`doc.list`): replace `md files in `files_changed` session events of the last 24 h` with `md files in `files_changed` session events of the last 24 h (the daemon records a watcher batch there only when it has a Markdown path, on every live session of the project)` (Task 8 edited the end of the same row; the two substrings do not overlap).
- `:1807-1809` (§20C item 4): replace `sessions that have `files_changed`/hook edits in this project in the last 24 h` with `sessions that have hook edits (`edited` events) or turn files in this project in the last 24 h`. Check first with `read docs/SPEC.md:1805-1810` that the text still matches; the data source is `session.files` (`edited` events and turn files), not `files_changed`, so this corrects the sentence.

`docs/HUMAN_CHECKS.md`: T5.5 (`:2132-2133`, read `docs/HUMAN_CHECKS.md:2130-2134` first): replace `Each batch becomes a `files_changed` session event (`{"paths": [...]}`,\n  `/`-separated, relative) on EVERY live session of the project (attribution is T5.6).` with `Each batch that has a Markdown file becomes a `files_changed` session event (`{"paths": [...]}`\n  with only its Markdown paths, `/`-separated, relative) on EVERY live session of the project\n  (attribution is T5.6); other batches write no row (Docs is the only reader).` Keep the rest of the sentence ("Review SHOULD left open: …").

Append to the end of the shared perf section:

```markdown
- [ ] **Docs "changed by" still works (needs `[features] docs = true`):** with one agent working, have it write a new `notes/todo.md` (not matching the review globs). Docs lists it, and its "changed by" / the comment "send to agent" default points at that agent's session. Have the agent edit only `.rs` files: Docs shows nothing new, Git "Changed by" (hook edits) and the turn's file list (`✓ Done · N files`) still show them, and with two agents editing the same file a conflict warning still appears. In `sqlite3 mkrew.db`: `select count(*) from session_event where kind='files_changed' and payload not like '%.md%';` does not grow while the agent edits source files (rows written before this change excepted).
```

- [ ] **Step 5: Verify, tick, commit**

Run: `cargo test -p mkrew-daemon -p mkrew-store -j 4` (expect PASS), then `sh scripts/verify.sh` (fallback if rustc crashes: `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace -j 4`). Tick inbox line 126 `[~]`→`[x]` in `docs/INBOX.md` (all of A4, B5, B6, B7 done); do not stage INBOX.
Commit: `git add crates/mkrew-daemon/src/session.rs crates/mkrew-daemon/tests/daemon/states.rs crates/mkrew-daemon/tests/daemon/main.rs crates/mkrew-daemon/tests/daemon/turns.rs docs/SPEC.md docs/HUMAN_CHECKS.md` (shared files: stage only this task's hunks with `git add -p <path>` if others' changes are in the tree). Never push.
Message: `perf(daemon): record files_changed only for Markdown batches, in-memory working sessions`

---

### Task 10: Opt-in turn snapshots (daemon) — done by e0c245f; one small gap left

> **Refresh 2026-10-13:** mostly done by e0c245f (ADR 0046). Reduced from 10 steps to 3. Dropped: the `turn_snapshots` / `conflict_detection` names (the switch is `[features] turns`), `Hub::features()`, `Job::Start.snapshot`, the `invalid_state` changes in `server/turn.rs` / `server/git.rs`, ADR 0041 (0041 is taken). The switch is read once **when a session starts**, not at each turn; ADR 0046 chose that, so there is no per-turn hot toggle (a toggle applies to new sessions). Left: the turn-start hook still waits for the recorder queue; `Recorder::snapshot` still hops to `spawn_blocking` when off.

Inbox: line 132 `13. Perf: turn snapshots and conflict detection become opt-in` (bullets "Off = zero cost"; do not tick yet). Spec: §20.0, §20C (SPEC.md ~1780-1796).

**Done already (verified in the code at HEAD 4cb293b):**

| Part | Where |
| --- | --- |
| `FeatureSettings { docs, turns }`, `Settings.features`, template lines, tests | `crates/mkrew-core/src/settings.rs:33-41`, template `:421-425` |
| Daemon reads `[features] turns` when a session starts | `server.rs:865` `turn_snapshots: self.settings.current().features.turns` → `SpawnSpec.turn_snapshots` (`session.rs:301`, destructured `:367`) → `turns::record(.., turn_snapshots, jobs)` (`session.rs:511-518`) |
| Off = no git at all: `in_git: (!snapshots).then_some(false)` makes `Recorder::snapshot` return `None`, so no `git add`/`write-tree`/`update-ref`, no `refs/mkrew/turns/*`, no snapshot lock, and `counted` skips `git diff --numstat` (both trees `None`) | `turns.rs:54-55`, `:151-161`, `:180-187` |
| Off still records: turn rows, `session.turn` with file paths (`added`/`removed` `None`), the `Done · 3 files · 4m` summary | `turns.rs:87-120` (`start_turn`, `end_turn`, `Event::SessionTurn`); files via `Command::TurnFiles` (`session.rs:746`, `:1121`) and `turns.add_files` (`session.rs:1246`) |
| `turn.restore_preview` / `turn.restore` with no start tree answer `invalid_state` ("turn N of session S has no snapshot (not a git repo, or older than 7 days)") | `server/turn.rs:163-188` (`restore_target`) |
| `delete_expired_refs` still runs at startup, so refs of an earlier "on" period still expire | `turns.rs:221` |
| Tests: recorder with snapshots off (`turns_outside_git_or_with_snapshots_off_have_no_trees_or_counts`, `turns.rs:467`), three daemon tests run with `env.enable_turns()` (`tests/daemon/main.rs:110`, `tests/daemon/turns.rs:210,282,401`) | |

**Gaps (only these remain):**
1. The hook of a turn start still waits when off. `Actor::track_turn` (`session.rs:1355-1360`) moves `self.hook_reply.take()` into `Job::Start`; the recorder drops it after `recorder.start` (`turns.rs:61-64`). Off, that wait is not git, but it is the recorder queue, a `spawn_blocking` hop and the `start_turn` SQLite write under the store mutex. `server.rs:1460` (`hook_report`) holds the agent's hook until then.
2. `Recorder::snapshot` (`turns.rs:151`) calls `spawn_blocking` even when `in_git == Some(false)` (twice per turn: start and end).

Not gaps (decided): `turn.restore*` message does not name the setting (see Open questions); `git.status` / `git.diff` with a tree `since` are unchanged (a tree only exists for a turn that was snapshotted, and the UI offers it only from the Turns dialog); `task.undo*` uses the HEAD checkpoint and stays.

**Files:**
- Modify: `crates/mkrew-daemon/src/session.rs` (`Actor.turn_snapshots`, `track_turn`)
- Modify: `crates/mkrew-daemon/src/turns.rs` (`Recorder::snapshot`)
- Modify: `docs/SPEC.md` (§20C.1, the sentence at ~1793)

**Interfaces:**
- Produces: nothing public. `Job::Start` keeps its shape (`n`, `at`, `reply`).

- [ ] **Step 1: Do not hold the hook when snapshots are off**

`crates/mkrew-daemon/src/session.rs`, `struct Actor`: after the field `hook_reply: Option<oneshot::Sender<()>>,` (doc `/// The reply of the hook being handled; a turn it starts keeps it until the snapshot.`) add:

```rust
    /// `[features] turns` when the session started: only then a turn start holds its hook.
    turn_snapshots: bool,
```

In the `Actor { … }` literal (where `hook_reply: None,` is, ~`:544`), after `hook_reply: None,` add `turn_snapshots,` (`turn_snapshots` is a `bool`, still in scope: `turns::record(.., turn_snapshots, jobs)` took a copy).

In `fn track_turn`, replace

```rust
            Some(TurnEvent::Started(n)) => turns::Job::Start {
                n,
                at,
                reply: self.hook_reply.take(),
            },
```

with

```rust
            Some(TurnEvent::Started(n)) => turns::Job::Start {
                n,
                at,
                // Off (ADR 0046): nothing to wait for. The reply stays in `hook_reply` and is
                // dropped when `on_hook` returns, so the agent is not held up.
                reply: if self.turn_snapshots {
                    self.hook_reply.take()
                } else {
                    None
                },
            },
```

Update the doc of `Command::Hook` (`session.rs:238-240`): `reply` is dropped once handled; for a hook that started a turn, after its start snapshot (SPEC §20C.1) — add ` (only with turn snapshots on)`.

- [ ] **Step 2: No thread hop when there is no git**

`crates/mkrew-daemon/src/turns.rs`, `Recorder::snapshot`: first statement of the body (before `let hub = Arc::clone(&self.hub);`):

```rust
        // Outside git, or snapshots off (`record` sets `in_git` to `Some(false)`): nothing to do.
        if self.in_git == Some(false) {
            return None;
        }
```

The existing test `turns_outside_git_or_with_snapshots_off_have_no_trees_or_counts` (`turns.rs:467`) covers both cases; a unit test of the hook reply would only test the wiring, so there is none. `Command::Hook` ordering is exercised by the daemon tests that run with `enable_turns()` and by every hook test with turns off.

- [ ] **Step 3: SPEC, verify, commit**

`docs/SPEC.md` §20C.1, in the sentence that starts `For a hook turn start (`UserPromptSubmit`) the `hook.report` reply is sent after the snapshot`, add after `so the agent has not edited yet`: ` (with `[features] turns` off there is no snapshot, so the reply is not held)`.

Run: `cargo test -p mkrew-daemon --lib -j 4 -- turns::` (expect PASS), then `sh scripts/verify.sh` (fallback: `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace -j 4`). Do not tick inbox line 132 (Task 12 does).

```
git add crates/mkrew-daemon/src/turns.rs docs/SPEC.md
git add -p crates/mkrew-daemon/src/session.rs   # only the Actor.turn_snapshots / track_turn hunks (other tasks edit this file)
git commit -m "perf(daemon): a turn-start hook is not held while turn snapshots are off"
```

---

### Task 11: The UI hides turn features when snapshots are off — already done by e0c245f

> **Refresh 2026-10-13:** already done by e0c245f (ADR 0046); no steps left. The old plan's `palette::Context.turn_snapshots`, `GitView::turn_features_off`, `Shell::apply_features` for turns and the pane-header `pane-turns` button are not needed: the Turns entry points are now `[features] turns`-gated, and Undo-turn / Since-turn are reachable only through the Turns dialog.

Evidence (HEAD 4cb293b):
- State: `Shell.features` (`crates/mkrew-ui/src/main.rs:529-530`), read in `Shell::new` (`shell/connection.rs:17-18,378`) and re-read with the other settings (`shell/connection.rs:57-86`, `shell.features = settings.features` + `sync_panes`).
- "Turns…": the tab strip `⋯` menu item is built only `.when(agent && self.features.turns, …)` (`shell/menus.rs:409`); the palette entry `Extra::Turns` needs `context.features.turns` (`palette.rs:124-125`); `Context.features` is filled in `open_palette`. There is no keymap command for Turns. The pane header has no Turns button any more (`pane-turns` does not exist; "Turns…" lives in the `⋯` menu, SPEC §10 ~924).
- Test: `palette/tests.rs:28` `features_off_hide_docs_and_turns`.
- Undo dialog for turns (`UndoDialog::for_turn`, `undo_dialog.rs:179`): only called by `Shell::restore_before_turn` (`shell/history.rs:160`), which runs only from the Turns dialog. The "Undo task" dialog (checkpoint based) is separate and stays.
- Git view "Since turn" (`Since::turn`, `git_view.rs:139`): only created by `Shell::diff_since_turn` (`shell/history.rs:187`), again only from the Turns dialog. A task's "since" (`board.rs:54`, `Origin::Task`) is not a turn feature and stays.
- "Changed by" (`git_menus.rs:413` `git_changed_by`, `shell/git.rs:360` `load_session_files`) is **not** hidden and should not be: its data is `session.files`, built from `files_changed` / hook `edited` session events (SPEC §20C.4), not from snapshots, so it works with `turns = false`. Hiding it was in the 2026-10-10 inbox bullet only because it was assumed to need snapshots. See Open questions.
- Not done on purpose: closing an open Turns dialog / turn Restore / "Since turn" listing when `turns` is switched off while they are open. They keep working (the trees exist for 7 days), the next open is blocked by the hidden entry points. Add only if reported (`ponytail:`).

---

### Task 12: Conflict detection is opt-in — `[features] conflicts` (daemon and UI)

> **Refresh 2026-10-13:** still needed, rewritten. Changes vs 2026-10-10: (1) the switch is `[features] conflicts = false`, named like `docs` / `turns` (was `conflict_detection`), and ADR 0046 is **amended** (was new ADR 0041, now taken); (2) `[features]` already exists, so the task adds one field, not the whole section; (3) the daemon `Hub` has no settings access yet, so this task adds `SettingsHolder::features()`, `Hub::use_settings` / `Hub::features()` (the old Task 10 plumbing, minus the turn parts); (4) the UI is a plain `if self.features.conflicts` guard in `shell/connection.rs` plus clearing the warnings on a switch-off (no `Conflicts::track`, which only wrapped two calls); (5) test moved from a timing-based daemon test to a unit test of `record_edits`.

Inbox: line 132 (bullet "Off = zero cost" for A3 and "Off = UI hides … conflict warnings"; **tick it `[~]`→`[x]`**, last task of item 13). Spec: §7 events (SPEC.md:577), M5 checklist (:1484), §10 pane header (:924), §20.0 (:1660), ADR 0046.

**Current state (investigated at HEAD 4cb293b; item 12 (T7–T9) edits `session.rs` too, so re-find by name):**
- Daemon `session.rs`: `Hub.conflicts: Mutex<HashMap<Ulid, conflict::Tracker<Ulid>>>` (`:85`). `fn record_edits(&self, project_id, paths: &[String], editor: Ulid, at: i64)` (`:874`) has two callers: `files_changed` (`:744`, watcher batch, only when exactly one session of the project is `working`) and `hook_edits` (`:862`, hook-reported edits). It locks the tracker, `prune`s, `edit`s each path, and only on an overlap reads `store().list_sessions(Some(project_id))` (`:887`) and broadcasts `Event::ConflictDetected`. `unwatch_idle_project` (`:704`) drops the project's tracker. `files_changed` still needs its `working` list for `Command::TurnFiles` (turn summaries) and `hook_edits` still writes the `edited` event (for "Changed by"), so only `record_edits` is gated; its lock, tracker and the overlap query are the cost.
- The daemon `Hub` has no settings. `Daemon::new` (`server.rs:88-101`) builds `Hub::new(store)` and then `SettingsHolder::load(paths.settings_file())` as a separate `Arc`. Other `Hub::new` callers are tests only (`turns.rs:315`, `notify.rs:302`, `awake.rs:369`).
- **How the daemon re-reads settings today:** `Daemon::serve` (`server.rs:109`) starts `settings::watch(Arc<SettingsHolder>)` → `watch_file` on the config dir (non-recursive, file-name filter, editors' temp+rename safe) → `debounce` `QUIET` = 300 ms (`daemon/settings.rs:15,95-142`) → `SettingsHolder::reload()` (`:46`): a good file replaces the state, a bad one keeps the last good settings and sets `error()` (shown in `daemon.status.settings_error`, `server.rs:484`). Consumers read at use time: `notify.rs:71` `settings.current().notify` per state event, `awake.rs:48` per loop, `web.rs:80,141,163` `settings.current().web.enabled` plus `settings.changed().await` (a single-waiter `Notify`, owned by the `mk web` supervisor: do not wait on it elsewhere). `[features] turns` is read only at session start (`server.rs:865`). `current()` clones the whole `Settings` (strings), so a per-batch read should use a `Copy` accessor.
- **How the UI re-reads settings today:** `Shell::new` reads the file once (`shell/connection.rs:17`); the 500 ms status tick compares the file mtime (`:57-60`), runs `theme::reload`, then reads the file again and applies `diff_mode`, `diff_wrap`, `reading_width` and `features` (`:76-86`, `if shell.features != settings.features { shell.features = settings.features; shell.sync_panes(..) }`). **Task 6 (ADR 0055) moves this poll to window activation: anchor on the line `if shell.features != settings.features {`, wherever Task 6 left it.**
- UI: `Conflicts` (`crates/mkrew-ui/src/conflicts.rs`) is applied on **every** event: `shell/connection.rs:650-653` `let detected = self.conflicts.apply(event); if self.conflicts.prune(..) || detected { cx.notify(); }`, and after each list refresh `:491-493` `shell.conflicts.prune(..)`. Badges render from `Shell.conflicts` (`shell/cards.rs:180` `for_task`, `shell/pane_render.rs:552` `for_session`, `widgets.rs:633` `conflict_badge`), so an empty `Conflicts` is already "no warning UI"; nothing in the render code changes.
- `tests/daemon/docs.rs:148` `two_sessions_editing_one_file_raise_a_conflict` (two writers, then hook edits of `src/hooked.rs`) needs the switch on. `Env` (`tests/daemon/main.rs`) has `enable_turns()` (`:110`) writing `settings.toml` into `<dir>/config`.
- Other consumers of `Event::ConflictDetected`: only the UI (`conflicts.rs`) and the protocol round-trip test; `mk` and `mk web` do not use it.

**Files:**
- Modify: `crates/mkrew-core/src/settings.rs` (`FeatureSettings.conflicts`, `template()`, `full_file_parses`)
- Modify: `crates/mkrew-daemon/src/settings.rs` (`SettingsHolder::features`)
- Modify: `crates/mkrew-daemon/src/session.rs` (`Hub.settings`, `use_settings`, `features`, `record_edits`, unit test)
- Modify: `crates/mkrew-daemon/src/server.rs` (`Daemon::new`)
- Modify: `crates/mkrew-daemon/tests/daemon/main.rs` (`Env::write_settings`), `crates/mkrew-daemon/tests/daemon/docs.rs` (the conflict test)
- Modify: `crates/mkrew-ui/src/shell/connection.rs` (event handler, list refresh, settings reload), `crates/mkrew-ui/src/palette/tests.rs` (struct literal)
- Modify: `docs/decisions/0046-feature-switches.md`, `docs/SPEC.md`, `docs/HUMAN_CHECKS.md`, `docs/INBOX.md`

**Interfaces:**
- Produces: `FeatureSettings.conflicts: bool` (default `false`).
- Produces: `SettingsHolder::features(&self) -> FeatureSettings` (`Copy`).
- Produces: `Hub::use_settings(&self, settings: Arc<SettingsHolder>)` (set once; the daemon calls it) and `Hub::features(&self) -> FeatureSettings` (defaults until `use_settings`; hot: it follows `reload()`). Later tasks may use `Hub::features()`.
- Protocol: no wire change (`PROTOCOL_VERSION` stays 40, Task 8 bumps it to 41). No migration.

- [ ] **Step 1: Failing test and fixture updates**

`crates/mkrew-daemon/src/session.rs`, `mod tests`, at the end:

```rust
    /// `[features] conflicts` (ADR 0046): off, overlapping edits are neither tracked nor
    /// announced; a hot reload to on applies to the next edits.
    #[test]
    fn overlapping_edits_are_announced_only_while_conflicts_is_on() {
        let dir = tempfile::tempdir().unwrap();
        let hub = Hub::new(Store::open(&dir.path().join("mkrew.db")).unwrap());
        let file = dir.path().join("settings.toml");
        let settings = Arc::new(SettingsHolder::load(file.clone()));
        hub.use_settings(Arc::clone(&settings));
        let mut events = hub.events.subscribe();
        let (project, a, b) = (Ulid::new(), Ulid::new(), Ulid::new());
        let paths = ["src/a.rs".to_owned()];

        hub.record_edits(project, &paths, a, 0);
        hub.record_edits(project, &paths, b, 1_000);
        assert!(events.try_recv().is_err(), "off: nothing is announced");
        assert!(lock(&hub.conflicts).is_empty(), "off: no tracker was created");

        std::fs::write(&file, "[features]\nconflicts = true\n").unwrap();
        settings.reload();
        hub.record_edits(project, &paths, a, 2_000);
        hub.record_edits(project, &paths, b, 3_000);
        let Ok(Event::ConflictDetected(found)) = events.try_recv() else {
            panic!("no conflict.detected");
        };
        assert_eq!(found.path, "src/a.rs");
    }
```

(`Store`, `Event`, `Ulid`, `Arc`, `lock` come from `use super::*;`; add `use crate::settings::SettingsHolder;` to the top-level imports of `session.rs` in Step 3, which the test then sees too.)

`crates/mkrew-core/src/settings.rs`, `mod tests`, `full_file_parses` (the TOML at ~`:569` and the literal at ~`:606`): change the `[features]` text to

```toml
[features]
docs = true
conflicts = true
```

and the literal to

```rust
                features: FeatureSettings {
                    docs: true,
                    turns: false,
                    conflicts: true,
                },
```

`crates/mkrew-ui/src/palette/tests.rs:20-23`: the `FeatureSettings { docs: true, turns: true }` literal gets `conflicts: false,` (it does not read it).

`crates/mkrew-daemon/tests/daemon/main.rs`: replace `enable_turns` (`:109-114`) with

```rust
    /// `settings.toml` of the daemon's config dir. Before `start`, or later for a hot reload.
    fn write_settings(&self, toml: &str) {
        let dir = self.dir.path().join("config");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("settings.toml"), toml).unwrap();
    }

    /// `[features] turns = true`: sessions take turn snapshots (ADR 0046).
    fn enable_turns(&self) {
        self.write_settings("[features]\nturns = true\n");
    }
```

`crates/mkrew-daemon/tests/daemon/docs.rs`, `two_sessions_editing_one_file_raise_a_conflict`: directly after `let env = Env::new();` add

```rust
    env.write_settings("[features]\nconflicts = true\n");
```

Run: `cargo test -p mkrew-daemon --lib -j 4 -- session::tests::overlapping_edits`
Expected: FAIL to compile (`use_settings`, `FeatureSettings.conflicts` do not exist).

- [ ] **Step 2: `conflicts` in core**

`crates/mkrew-core/src/settings.rs`, `pub struct FeatureSettings`: after `pub turns: bool,` add

```rust
    /// Two sessions editing one file within a minute: `conflict.detected` (SPEC §7) and the
    /// ⚠ badge on cards and pane headers.
    pub conflicts: bool,
```

`template()`: replace the `[features]` block (`:421-425`)

```
     # Optional modules, off by default. docs: the Docs view. turns: the agent's turn\n\
     # snapshots and its Turns… dialog (applies to sessions started after the change).\n\
     [features]\n\
     # docs = false\n\
     # turns = false\n\
```

with

```
     # Optional modules, off by default. docs: the Docs view. turns: the agent's turn\n\
     # snapshots and its Turns… dialog (applies to sessions started after the change).\n\
     # conflicts: a warning when two sessions edit the same file within a minute.\n\
     [features]\n\
     # docs = false\n\
     # turns = false\n\
     # conflicts = false\n\
```

(the test `the_template_parses_to_the_defaults_commented_or_not` still passes: the new line defaults to `false`.)

Run: `cargo test -p mkrew-core --lib settings -j 4` (expect PASS).

- [ ] **Step 3: Daemon: settings access and the gate**

`crates/mkrew-daemon/src/settings.rs`: change the import to `use mkrew_core::settings::{self, FeatureSettings, Settings, SettingsError};` and after `pub fn current(&self) -> Settings { … }` add

```rust
    /// The `[features]` switches of the last good settings (`Copy`: no clone of the strings).
    pub fn features(&self) -> FeatureSettings {
        self.lock().settings.features
    }
```

`crates/mkrew-daemon/src/session.rs`:
- Imports: `use crate::settings::SettingsHolder;` (next to the other `use crate::…` lines) and `use mkrew_core::settings::FeatureSettings;`.
- `pub struct Hub`, right after `pub snapshots: Snapshots,`:

```rust
    /// The daemon's settings, for `[features]` (ADR 0046); set once by [`Hub::use_settings`].
    settings: std::sync::OnceLock<Arc<SettingsHolder>>,
```

- `Hub::new`, after `snapshots: Snapshots::default(),`: `settings: std::sync::OnceLock::new(),`
- `impl Hub`, after `pub fn store(&self) -> MutexGuard<'_, Store> { … }`:

```rust
    /// Where `[features]` come from; the daemon calls it once. Without it (unit tests of other
    /// parts) every switch is off, the default.
    pub fn use_settings(&self, settings: Arc<SettingsHolder>) {
        let _ = self.settings.set(settings);
    }

    /// The current `[features]`. The holder re-reads `settings.toml` after each change
    /// (debounced 300 ms), so each call sees a switch made since the last one.
    pub fn features(&self) -> FeatureSettings {
        self.settings
            .get()
            .map_or_else(FeatureSettings::default, |settings| settings.features())
    }
```

- `fn record_edits`, first statement (before `let found: Vec<(String, Vec<Ulid>)> = {`):

```rust
        // Off by default (ADR 0046): no tracker lock, no session query, no `conflict.detected`.
        if !self.features().conflicts {
            return;
        }
```

`crates/mkrew-daemon/src/server.rs`, `Daemon::new`: replace

```rust
        let hub = Arc::new(Hub::new(store));
        Arc::new(Self {
            git: Arc::clone(&hub.git),
            hub,
            settings: Arc::new(SettingsHolder::load(paths.settings_file())),
```

with

```rust
        let hub = Arc::new(Hub::new(store));
        let settings = Arc::new(SettingsHolder::load(paths.settings_file()));
        hub.use_settings(Arc::clone(&settings));
        Arc::new(Self {
            git: Arc::clone(&hub.git),
            hub,
            settings,
```

Run: `cargo test -p mkrew-daemon --lib -j 4 -- session::tests::overlapping_edits` (expect PASS), then `cargo test -p mkrew-daemon --test daemon -j 4 -- two_sessions_editing_one_file_raise_a_conflict` (expect PASS; needs the built `mkrew-fakeagent`, like its neighbours: build the `mkrew-cli` and `mkrew-fakeagent` bins first).

- [ ] **Step 4: UI skips the work and clears the warnings**

`crates/mkrew-ui/src/shell/connection.rs`:
- Event handler (`dispatch`, ~`:649-653`): replace

```rust
                // After the board and sidebar, so a task leaving Running clears its warning.
                let detected = self.conflicts.apply(event);
                if self.conflicts.prune(&self.board, self.sidebar.sessions()) || detected {
                    cx.notify();
                }
```

with

```rust
                // After the board and sidebar, so a task leaving Running clears its warning.
                // Off (the default, ADR 0046): the daemon sends none; nothing is pruned per event.
                if self.features.conflicts {
                    let detected = self.conflicts.apply(event);
                    if self.conflicts.prune(&self.board, self.sidebar.sessions()) || detected {
                        cx.notify();
                    }
                }
```

- After the list refresh (~`:490-493`): replace

```rust
                shell
                    .conflicts
                    .prune(&shell.board, shell.sidebar.sessions());
```

with

```rust
                if shell.features.conflicts {
                    shell
                        .conflicts
                        .prune(&shell.board, shell.sidebar.sessions());
                }
```

- Settings reload (anchor `if shell.features != settings.features {`, ~`:82`): add as the first statement inside that block

```rust
                                        // Off clears what was shown; on starts again with the
                                        // next `conflict.detected` (not persisted, SPEC §7).
                                        if !settings.features.conflicts {
                                            shell.conflicts = Conflicts::default();
                                        }
```

(`Conflicts` is imported in `main.rs:50`; `connection.rs` has `use crate::*;`. The `cx.notify()` after the reload block repaints the cards and pane headers.)

No UI unit test: this is wiring (an `if` and a reset); `Conflicts::apply` / `prune` are unchanged and keep their tests (`conflicts.rs` tests).

Run: `cargo check -p mkrew-ui -j 4` (expect clean).

- [ ] **Step 5: ADR 0046 amendment, SPEC, HUMAN_CHECKS, inbox**

`docs/decisions/0046-feature-switches.md`:
- Title line stays. After `- Status: accepted` add `- Amended: 2026-10-13 (`conflicts` switch; perf review `docs/PERF_REVIEW.md` A3, inbox item 13)`.
- Context, append: 

```markdown
- Conflict detection (SPEC §7 `conflict.detected`, M5) runs for every file-watcher batch and every
  hook edit: a tracker lock and, on an overlap, a session query; the UI applies and prunes its
  warnings on every event. Most runs have one agent at a time, so it is cost without use.
```

- Options, item 2: change to ``2. A `[features]` table: `docs` gates the UI; `turns` gates the UI and the daemon's snapshots; `conflicts` gates the daemon's edit tracker and the UI's warnings. (Chosen.)``
- Decision, `[features]` line: ``- `[features] docs = false`, `turns = false`, `conflicts = false` (defaults). The session panel gets no switch.`` and append the bullet:

```markdown
- `conflicts` off: the daemon's `record_edits` returns at once (no tracker, no session query, no
  `conflict.detected`); the UI does not apply or prune warnings per event and shows no ⚠ badge on
  cards or pane headers. Applies live: the daemon reads the switch at every watcher batch / hook
  edit, the UI when it re-reads the settings; switching off clears the warnings shown.
```

- Consequences, append:

```markdown
- Default behavior changes for existing users: the M5 conflict warning is gone until `conflicts =
  true`. SPEC §7, §10, §20.0 and the M5 checklist say so.
- `conflicts` is the only switch the daemon reads live through `Hub::features()`; `turns` is still
  read when a session starts.
```

`docs/SPEC.md`:
- §7 events table (`:577`): replace `two sessions edited the same file within 60 s (M5): `project_id`, `path`, `session_ids`, `task_ids`` with `two sessions edited the same file within 60 s (M5), only with `[features] conflicts = true` (default off, ADR 0046): `project_id`, `path`, `session_ids`, `task_ids``.
- M5 checklist (`:1484`): replace `- [ ] Two sessions in one project edit the same file within 60 s → `conflict.detected`, UI warns on both cards.` with `- [ ] With `[features] conflicts = true`: two sessions in one project edit the same file within 60 s → `conflict.detected`, UI warns on both cards; off (default), neither.`
- §10 pane header (`:924`): `a conflict warning,` → `a conflict warning (only with `[features] conflicts = true`, ADR 0046),`.
- §20.0 toml block: after the `turns = false …` line (`:1662`) add `conflicts = false           # §7 conflict.detected and the ⚠ badge on cards and pane headers; applies live`.

`docs/HUMAN_CHECKS.md`:
- Section `### To verify — 2026-10-11 feature switches (ADR 0046)`, after the **Turns** bullet (`:223-225`) add:

```markdown
- [ ] **Conflicts off (default), item 13:** two agents in one workspace both edit `shared.txt`
  (two fake or real agents). No `⚠ Conflict` badge on the cards or the pane headers, and the
  daemon log (`MKREW_LOG=info`) has no `conflict detected` line.
- [ ] **Conflicts on:** add `[features]` / `conflicts = true`, save (no restart), repeat: the
  `⚠ Conflict: shared.txt` badge shows on both cards and panes, `Dismiss` clears it. Set it back
  to `false`: the badges disappear within ~1 s (the UI re-reads the file) and no new ones appear.
```

- The T5.7 item (`:1180`): change `Run two tasks in one workspace whose agents edit the same file` to `With `[features] conflicts = true`, run two tasks in one workspace whose agents edit the same file`.

`docs/INBOX.md` line 132: change `- [~] **13. Perf: turn snapshots and conflict detection become opt-in**` to `- [x] **13. Perf: turn snapshots and conflict detection become opt-in**`, and rewrite the "New settings" bullet's suggestion to say the switches are `[features] turns` (done in e0c245f) and `[features] conflicts` (the plan link stays; the plan owner edits the rest).

- [ ] **Step 6: Verify and commit**

Run: `cargo test -p mkrew-core --lib settings -j 4`, `cargo test -p mkrew-daemon --lib -j 4 -- session:: settings::`, `cargo test -p mkrew-ui --lib -j 4 -- conflicts:: palette::`, `cargo test -p mkrew-daemon --test daemon -j 4 -- two_sessions_editing_one_file_raise_a_conflict hook_edits_are` (expect PASS), then `sh scripts/verify.sh` (fallback: `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace -j 4`). This task ticks inbox line 132.

```
git add crates/mkrew-core/src/settings.rs crates/mkrew-daemon/src/settings.rs crates/mkrew-daemon/src/server.rs crates/mkrew-daemon/tests/daemon/main.rs crates/mkrew-daemon/tests/daemon/docs.rs crates/mkrew-ui/src/shell/connection.rs crates/mkrew-ui/src/palette/tests.rs docs/decisions/0046-feature-switches.md docs/SPEC.md docs/HUMAN_CHECKS.md
git add -p crates/mkrew-daemon/src/session.rs   # only the Hub.settings / use_settings / features / record_edits / test hunks
git commit -m "feat: conflict detection is opt-in via [features] conflicts (ADR 0046)"
```

(INBOX.md is not committed by the agent: the rule is never to commit INBOX; leave the edit for the user.)

---

### Task 13: Daemon screen scan builds 4 lines, at most 4 scans/s per session (perf B9)

> **Refresh 2026-10-13:** still needed, nothing of it landed (`crates/mkrew-daemon/src/session.rs:1203-1214` still hashes the screen and builds every row on every batch; `emulator.rs:113` `lines_to_cursor` unchanged). Changes vs 2026-10-10:
> - ADR number is now **0057** (0043 is taken); date 2026-10-13.
> - Daemon e2e tests moved to `crates/mkrew-daemon/tests/daemon/{states,io,runs}.rs` (one `daemon` test binary; run them with `-- <filters>`); the ports test is `run_sessions_report_the_ports_they_print`.
> - `Actor::on_output` is now also fed by the flood path (`waiting` batch + `Pacer`, `session.rs:1053-1060`) and `drain_after_exit` (`:1533`); both call `on_output`, so the one throttle covers them. The flood pacing (ADR 0002) limits how often a flooding session reaches `on_output`; it does not limit the scan of a normal busy agent (≤ 60 batches/s), so this task is still needed.
> - The SPEC lines moved (§6 ends :460, §8 fan-out :631-632); the HUMAN_CHECKS perf section may not exist yet (Task 1 creates it).

Inbox: line 138 `14. Perf: terminal path (daemon scan, scrollback, replay)` (bullet B9; do not tick yet). Spec: §6 (SPEC.md:447-460), §8 (:625-632), ADR 0015 (`docs/decisions/0015-visible-change-is-activity.md`), ADR 0003 (state source). New ADR 0057.

**Current state (investigated 2026-10-13):**
- `Actor::on_output` (`crates/mkrew-daemon/src/session.rs:1184-1219`) runs for every output batch of every live session, background ones included (≤ 60/s). After the log write, DSR reply, title and attachments it calls `Emulator::text_fingerprint()` (`emulator.rs:134`, SipHash over every cell). Only when the hash changed (`:1206-1207`) it calls `Actor::screen_lines()` (`:1323`) = `Emulator::lines_to_cursor()` (`emulator.rs:113`): one `String` per row from the top to the cursor row, then `Actor::scan_ports(&lines)` (`:1413`, returns at once unless `self.ports.is_some()`, i.e. a run session) and `StateMachine::on_output(Instant::now(), &lines)`.
- The state machine reads only the last `PATTERN_TAIL_LINES = 4` non-blank rows (`crates/mkrew-core/src/state.rs:21`; `needs_input` at `:144-163` trims, drops blanks, takes 4). The `tick` branch of `Actor::run` (`session.rs:1156-1163`, `sleep_until(tick_at)`, only armed while `Working`) also builds the full text.
- Other consumers of the full text: only `Actor::scan_ports`. Not users: `session.read` uses `Emulator::last_lines` (own code), the terminal title `Emulator::take_title`, peek/attach `snapshot()`.
- `Actor::deliver()` runs at the end of every `on_output` (`:1218`) and in the tick branch. `SendQueue::on_output` clears `in_flight`, then `deliver()` pops the next text if `can_deliver(self.session.state)`. A throttled scan would leave a stale `idle` there and deliver the second queued text into a busy agent, so a non-empty queue must bypass the throttle (`ScanRate::on_output(now, urgent)`).
- Hooks still do NOT cover the scan (re-checked; reasons in Open questions): no shipped profile has `mode = "hook"` (`agents/*.toml`: claude, codex, copilot, omp are `hook+screen`; the rest `screen`); no hook reports `idle`; Claude/Codex profiles carry patterns (`'❯ 1\. Yes'`, `'(?i)allow command'`) hooks do not replace; the daemon cannot tell whether the user ran `mk integration install`.
- `mod` list in `crates/mkrew-daemon/src/lib.rs:6-32` is alphabetical (`mod plan;` `mod server;` at :19-20). `session.rs` has `mod tests` (:1590) in pure-logic style with `ms(n)` helper.
- `mkrew_core` exports `StateConfig`, `StateMachine`, `StateMode` at the crate root (`lib.rs:28-33`); `PATTERN_TAIL_LINES` is `mkrew_core::state::PATTERN_TAIL_LINES`.
- Daemon e2e that must keep passing: `tests/daemon/states.rs` `fake_agent_session_reaches_needs_input_and_is_killed` (the fake agent prints a prompt then goes silent: needs the trailing scan); `tests/daemon/io.rs` `send_while_working_is_queued_until_the_agent_asks_and_wait_sees_it_finish`, `a_queued_text_removed_while_working_is_never_typed`, `exiting_with_queued_texts_announces_an_empty_queue`; `tests/daemon/runs.rs` `run_sessions_report_the_ports_they_print`.
- Line numbers in `session.rs` shift when Tasks 7-12 land (they edit this file). Anchors below are quoted from the code.

**Files:**
- Create: `crates/mkrew-daemon/src/scan.rs` (`ScanRate`, `SCAN_GAP`, tests)
- Create: `docs/decisions/0057-terminal-path-limits.md`
- Modify: `crates/mkrew-daemon/src/lib.rs` (`mod scan;`)
- Modify: `crates/mkrew-daemon/src/emulator.rs` (`lines_to_cursor`, new `tail_to_cursor`, `cursor_row`, `row_chars`, `row_text`; tests)
- Modify: `crates/mkrew-daemon/src/session.rs` (`Actor` field `scan`, `Actor::run` branch, `on_output`, new `scan_screen`, `screen_tail`, `finish`)
- Modify: `crates/mkrew-core/src/state.rs` (doc of `PATTERN_TAIL_LINES` only)
- Modify: `docs/SPEC.md` §6 (after :460), §8 (:631-632)
- Modify: `docs/HUMAN_CHECKS.md` (append to the shared perf section)

**Interfaces:**
- Produces: `Emulator::tail_to_cursor(&self, count: usize) -> Vec<String>`: the last `count` non-blank rows from the top to the cursor row, top-down, trailing blanks trimmed.
- Produces: `crate::scan::ScanRate` with `new(gap: Duration) -> Self`, `on_output(&mut self, now: Instant, urgent: bool) -> bool`, `deadline(&self) -> Option<Instant>`, `scanned(&mut self, now: Instant)`, `take_pending(&mut self) -> bool`; `pub const SCAN_GAP: Duration` (250 ms).
- Consumes: `mkrew_core::state::PATTERN_TAIL_LINES`, `StateMachine::on_output/tick`.

- [ ] **Step 1: Failing tests**

In `crates/mkrew-daemon/src/emulator.rs` `mod tests`, after `lines_stop_at_the_cursor_row`, add:

```rust
    /// What the state machine took of the old full text: the last `count` non-blank rows
    /// (trailing blanks trimmed), top to bottom.
    fn old_feed(emu: &Emulator, count: usize) -> Vec<String> {
        let mut tail: Vec<String> = emu
            .lines_to_cursor()
            .into_iter()
            .rev()
            .filter(|line| !line.trim_end().is_empty())
            .take(count)
            .collect();
        tail.reverse();
        tail
    }

    #[test]
    fn the_tail_is_what_the_full_text_fed_the_state_machine() {
        let scrolled: String = (0..12).map(|n| format!("line{n}\r\n")).collect();
        let screens: [(&str, &[u8]); 9] = [
            ("empty", b""),
            ("one line", b"hello"),
            ("blank rows between", b"a\r\n\r\n   \r\nb\r\n\r\n\r\nc"),
            ("more than four", b"1\r\n2\r\n3\r\n4\r\n5\r\n6"),
            ("wide char", "形x\r\nok".as_bytes()),
            ("cursor above the content", b"1\r\n2\r\n3\r\n4\r\n5\r\n\x1b[2;1H"),
            ("scrolled", scrolled.as_bytes()),
            ("whitespace-only rows", b"x\r\n \t \r\ny"),
            (
                "colored prompt with trailing blanks",
                b"\x1b[32mDo you want to proceed? (y/n)\x1b[0m   \r\n",
            ),
        ];
        for (name, bytes) in screens {
            let mut emu = Emulator::new(20, 8);
            emu.advance(bytes);
            for count in [1, 2, mkrew_core::state::PATTERN_TAIL_LINES] {
                assert_eq!(emu.tail_to_cursor(count), old_feed(&emu, count), "{name}, {count}");
            }
        }
    }
```

Create `crates/mkrew-daemon/src/scan.rs` containing only the tests (the type does not exist yet):

```rust
//! Rate limit of the per-session screen scan (ADR 0057). Pure: the caller passes `now`.

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use mkrew_core::state::PATTERN_TAIL_LINES;
    use mkrew_core::{StateConfig, StateMachine, StateMode};
    use mkrew_protocol::SessionState;

    use super::*;
    use crate::emulator::Emulator;

    fn ms(n: u64) -> Duration {
        Duration::from_millis(n)
    }

    #[test]
    fn the_first_output_scans_at_once_and_a_close_one_waits_for_the_trailing_scan() {
        let t0 = Instant::now();
        let mut rate = ScanRate::new(ms(250));
        assert!(rate.on_output(t0, false));
        assert_eq!(rate.deadline(), None);
        assert!(!rate.on_output(t0 + ms(10), false));
        assert_eq!(rate.deadline(), Some(t0 + ms(250)));
        // The gap runs from the last scan, not from the last output.
        assert!(!rate.on_output(t0 + ms(249), false));
        assert_eq!(rate.deadline(), Some(t0 + ms(250)));
        rate.scanned(t0 + ms(250));
        assert_eq!(rate.deadline(), None);
        assert!(!rate.take_pending());
        // Quiet for a whole gap: the next output scans at once again.
        assert!(rate.on_output(t0 + ms(500), false));
    }

    /// 300 batches at 16 ms (about 60/s): never two scans closer than the gap, and the
    /// last batch is always followed by a scan within one gap.
    #[test]
    fn a_flood_scans_at_most_four_times_a_second_and_the_last_batch_is_scanned() {
        let t0 = Instant::now();
        let mut rate = ScanRate::new(SCAN_GAP);
        let mut scans = Vec::new();
        let mut last_batch = t0;
        for i in 0..300 {
            let now = t0 + ms(i * 16);
            // The trailing scan fires between two batches when it is due.
            while let Some(due) = rate.deadline().filter(|due| *due <= now) {
                rate.scanned(due);
                scans.push(due);
            }
            if rate.on_output(now, false) {
                scans.push(now);
            }
            last_batch = now;
        }
        let due = rate.deadline().expect("the last batch is owed a scan");
        assert!(due <= last_batch + SCAN_GAP);
        rate.scanned(due);
        scans.push(due);
        for pair in scans.windows(2) {
            assert!(pair[1] - pair[0] >= SCAN_GAP, "{pair:?}");
        }
        // 4.8 s of output.
        assert!(scans.len() <= 21, "{} scans", scans.len());
    }

    /// A queued `session.send` is delivered when the state allows it; the state must be
    /// current then, so a waiting send scans every batch.
    #[test]
    fn a_waiting_send_is_never_held_back() {
        let t0 = Instant::now();
        let mut rate = ScanRate::new(SCAN_GAP);
        assert!(rate.on_output(t0, false));
        assert!(rate.on_output(t0 + ms(1), true));
        assert_eq!(rate.deadline(), None);
    }

    #[test]
    fn an_exit_can_claim_the_owed_scan_once() {
        let t0 = Instant::now();
        let mut rate = ScanRate::new(SCAN_GAP);
        rate.on_output(t0, false);
        rate.on_output(t0 + ms(5), false);
        assert!(rate.take_pending());
        assert!(!rate.take_pending());
        assert_eq!(rate.deadline(), None);
    }

    fn machine() -> StateMachine {
        StateMachine::new(&StateConfig {
            mode: StateMode::Screen,
            idle_after_ms: 3000,
            needs_input_patterns: vec![r"(?i)do you want to".into()],
        })
        .unwrap()
    }

    /// What `Actor::scan_screen` feeds the machine: the tail of the real emulator.
    fn scan(emu: &Emulator, machine: &mut StateMachine, now: Instant) -> Option<SessionState> {
        let tail = emu.tail_to_cursor(PATTERN_TAIL_LINES);
        let tail: Vec<&str> = tail.iter().map(String::as_str).collect();
        machine.on_output(now, &tail)
    }

    /// The agent asks 100 ms after the last scan and then stays silent: the prompt must still
    /// become `needs_input` (at the trailing scan), never be missed.
    #[test]
    fn a_prompt_that_lands_inside_the_gap_is_classified_by_the_trailing_scan() {
        let t0 = Instant::now();
        let mut emu = Emulator::new(40, 6);
        let mut machine = machine();
        let mut rate = ScanRate::new(SCAN_GAP);
        emu.advance(b"Compiling...\r\n");
        assert!(rate.on_output(t0, false));
        assert_eq!(scan(&emu, &mut machine, t0), Some(SessionState::Working));

        emu.advance(b"Do you want to proceed?\r\n");
        assert!(!rate.on_output(t0 + ms(100), false));
        assert_eq!(machine.state(), SessionState::Working);

        let due = rate.deadline().expect("a trailing scan is owed");
        assert_eq!(due, t0 + SCAN_GAP);
        rate.scanned(due);
        assert_eq!(scan(&emu, &mut machine, due), Some(SessionState::NeedsInput));
    }
}
```

In `crates/mkrew-daemon/src/lib.rs`, between `mod plan;` and `mod server;` add `mod scan;`.

Run: `cargo test -p mkrew-daemon --lib -j 4 -- scan::tests emulator::tests::the_tail`
Expected: FAIL to compile (`ScanRate`, `SCAN_GAP`, `tail_to_cursor` do not exist).

- [ ] **Step 2: `Emulator::tail_to_cursor`**

`crates/mkrew-daemon/src/emulator.rs`: replace the whole `lines_to_cursor` method (from `/// Visible lines from the top of the screen down to the cursor row, trailing blanks` through its closing `}`, `emulator.rs:111-129`) with:

```rust
    /// Visible lines from the top of the screen down to the cursor row, trailing blanks
    /// trimmed. Every row: only a run session's port scan needs all of it (SPEC §10); the
    /// state machine reads [`Self::tail_to_cursor`] (SPEC §6).
    pub fn lines_to_cursor(&self) -> Vec<String> {
        (0..=self.cursor_row())
            .map(|row| self.row_text(row))
            .collect()
    }

    /// The last `count` non-blank rows from the top of the screen down to the cursor row, top
    /// to bottom, trailing blanks trimmed: what the state machine matches
    /// `needs_input_patterns` against (`PATTERN_TAIL_LINES` rows). Walks up from the cursor and
    /// builds a `String` only for the rows it keeps, so a screen scan costs 4 allocations, not
    /// one per row.
    pub fn tail_to_cursor(&self, count: usize) -> Vec<String> {
        let mut lines: Vec<String> = (0..=self.cursor_row())
            .rev()
            .filter(|&row| !self.row_chars(row).all(char::is_whitespace))
            .take(count)
            .map(|row| self.row_text(row))
            .collect();
        lines.reverse();
        lines
    }

    /// The screen row the cursor is on (clamped to the screen).
    fn cursor_row(&self) -> usize {
        let row = self.term.grid().cursor.point.line.0.max(0);
        usize::try_from(row)
            .unwrap_or(0)
            .min(self.term.screen_lines().saturating_sub(1))
    }

    /// The characters of screen row `row`, one per column.
    fn row_chars(&self, row: usize) -> impl Iterator<Item = char> + '_ {
        let line = &self.term.grid()[Line(i32::try_from(row).unwrap_or(i32::MAX))];
        (0..self.term.columns()).map(move |column| line[Column(column)].c)
    }

    /// Screen row `row` as text, trailing blanks trimmed.
    fn row_text(&self, row: usize) -> String {
        let mut text: String = self.row_chars(row).collect();
        text.truncate(text.trim_end().len());
        text
    }
```

`crates/mkrew-core/src/state.rs:20`, in the doc of `PATTERN_TAIL_LINES`, replace `The daemon passes the screen up to the cursor row.` with `The daemon builds only these rows (\`Emulator::tail_to_cursor\`) of the screen up to the cursor row.`

Run: `cargo test -p mkrew-daemon --lib -j 4 -- emulator::tests`
Expected: PASS (`the_tail_is_what_the_full_text_fed_the_state_machine`, and the existing `lines_stop_at_the_cursor_row`, `last_lines_skip_blank_rows_and_keep_order`, the scrolled-screen test).

- [ ] **Step 3: `ScanRate`**

`crates/mkrew-daemon/src/scan.rs`: after the `//!` line and before `#[cfg(test)]`, add:

```rust
use std::time::{Duration, Instant};

/// Two scans of one session are at least this far apart: at most 4 a second (ADR 0057).
pub const SCAN_GAP: Duration = Duration::from_millis(250);

/// When a session's screen is scanned: right away after a quiet gap, else once the gap since
/// the last scan has passed (the trailing scan, so the final screen is always seen).
#[derive(Debug)]
pub struct ScanRate {
    gap: Duration,
    last: Option<Instant>,
    /// Output arrived inside the gap: a scan is owed at `last + gap`.
    pending: bool,
}

impl ScanRate {
    pub const fn new(gap: Duration) -> Self {
        Self {
            gap,
            last: None,
            pending: false,
        }
    }

    /// Output arrived at `now`: `true` = scan the screen now. `urgent` (a `session.send` waits
    /// for the state) always scans.
    pub fn on_output(&mut self, now: Instant, urgent: bool) -> bool {
        let due = self
            .last
            .is_none_or(|last| now.saturating_duration_since(last) >= self.gap);
        if due || urgent {
            self.scanned(now);
            true
        } else {
            self.pending = true;
            false
        }
    }

    /// When the owed trailing scan is due; `None` when nothing is held back.
    pub fn deadline(&self) -> Option<Instant> {
        if self.pending {
            self.last.map(|last| last + self.gap)
        } else {
            None
        }
    }

    /// A scan ran at `now` (the trailing one included).
    pub fn scanned(&mut self, now: Instant) {
        self.last = Some(now);
        self.pending = false;
    }

    /// Takes the owed scan: `true` if there was one (the session ended before it fired).
    pub fn take_pending(&mut self) -> bool {
        std::mem::take(&mut self.pending)
    }
}
```

Run: `cargo test -p mkrew-daemon --lib -j 4 -- scan::tests`
Expected: PASS (5 tests).

- [ ] **Step 4: Wire it into the actor**

`crates/mkrew-daemon/src/session.rs`:

(a) After `use crate::launch;` (alphabetical with the other `crate::` uses; `use crate::emulator::Emulator;` is at :30) add:
```rust
use crate::scan::{SCAN_GAP, ScanRate};
```
and next to the other `mkrew_core` use lines add a separate line (do not edit the existing `use mkrew_core::{AgentProfile, …}` line, other tasks touch it):
```rust
use mkrew_core::state::PATTERN_TAIL_LINES;
```

(b) `struct Actor`: replace
```rust
    screen_print: Option<u64>,
    /// `session.send` texts waiting for the session to be ready.
```
with
```rust
    screen_print: Option<u64>,
    /// At most one screen scan per [`SCAN_GAP`], plus a trailing one (ADR 0057).
    scan: ScanRate,
    /// `session.send` texts waiting for the session to be ready.
```

(c) In the `Actor { … }` literal in `Hub::spawn` (:527-547), replace `            screen_print: None,` with
```rust
            screen_print: None,
            scan: ScanRate::new(SCAN_GAP),
```

(d) `Actor::run`: after the `let tick_at = self.machine.next_deadline().map(tokio::time::Instant::from_std);` statement (:1026-1029) add:
```rust
            let scan_at = self.scan.deadline().map(tokio::time::Instant::from_std);
```
In the `tokio::select!`, replace the tick branch head
```rust
                () = sleep_until(tick_at), if tick_at.is_some() => {
                    let lines = self.screen_lines();
                    let lines: Vec<&str> = lines.iter().map(String::as_str).collect();
                    if let Some(state) = self.machine.tick(Instant::now(), &lines) {
```
with
```rust
                () = sleep_until(scan_at), if scan_at.is_some() => {
                    // The scan held back by `ScanRate`: the final screen is always classified.
                    let now = Instant::now();
                    self.scan.scanned(now);
                    self.scan_screen(now);
                    self.deliver();
                }
                () = sleep_until(tick_at), if tick_at.is_some() => {
                    let lines = self.screen_tail();
                    let lines: Vec<&str> = lines.iter().map(String::as_str).collect();
                    if let Some(state) = self.machine.tick(Instant::now(), &lines) {
```
(the rest of the tick branch stays).

(e) `Actor::on_output`: replace
```rust
        // Only visible changes are work: idle TUIs keep sending cursor/title/progress bytes,
        // which would otherwise hold the session in `working` forever (ADR 0015). The first
        // batch always counts, so hook-only profiles still leave `starting`.
        let print = self.emulator().text_fingerprint();
        if self.screen_print.replace(print) != Some(print) {
            let lines = self.screen_lines();
            let lines: Vec<&str> = lines.iter().map(String::as_str).collect();
            self.scan_ports(&lines);
            if let Some(state) = self.machine.on_output(Instant::now(), &lines) {
                self.set_state(state, StateSource::Screen);
            }
        }
```
with
```rust
        // The screen work runs at most every `SCAN_GAP` (ADR 0057). A waiting `session.send`
        // needs the current state when `deliver()` runs below, so it scans every batch.
        let now = Instant::now();
        if self.scan.on_output(now, !self.queue.is_empty()) {
            self.scan_screen(now);
        }
```

(f) Replace the method `fn screen_lines(&self) -> Vec<String> { … }` (:1323-1328) with:
```rust
    /// Every row up to the cursor: only [`Actor::scan_ports`] of a run session needs it.
    fn screen_lines(&self) -> Vec<String> {
        self.emulator
            .as_ref()
            .map(Emulator::lines_to_cursor)
            .unwrap_or_default()
    }

    /// The rows the state machine reads ([`PATTERN_TAIL_LINES`]), built without the rest.
    fn screen_tail(&self) -> Vec<String> {
        self.emulator
            .as_ref()
            .map(|emulator| emulator.tail_to_cursor(PATTERN_TAIL_LINES))
            .unwrap_or_default()
    }

    /// The screen work of output: the ports a run session printed, then the state machine.
    /// `now` is when the scan runs (the trailing scan fires up to [`SCAN_GAP`] after the output).
    fn scan_screen(&mut self, now: Instant) {
        // Only visible changes are work: idle TUIs keep sending cursor/title/progress bytes,
        // which would otherwise hold the session in `working` forever (ADR 0015). The first
        // scan always counts, so hook-only profiles still leave `starting`.
        let print = self.emulator().text_fingerprint();
        if self.screen_print.replace(print) == Some(print) {
            return;
        }
        if self.ports.is_some() {
            let lines = self.screen_lines();
            let lines: Vec<&str> = lines.iter().map(String::as_str).collect();
            self.scan_ports(&lines);
        }
        let tail = self.screen_tail();
        let tail: Vec<&str> = tail.iter().map(String::as_str).collect();
        if let Some(state) = self.machine.on_output(now, &tail) {
            self.set_state(state, StateSource::Screen);
        }
    }
```

(g) `Actor::finish` (:1479): replace
```rust
    fn finish(mut self, exit_code: Option<i32>) {
        if let Err(e) = self.log.flush() {
```
with
```rust
    fn finish(mut self, exit_code: Option<i32>) {
        // An exit cut off the trailing scan: a run session still owes the ports it printed.
        // (Not the state: a process that exited is not waiting for input.) Before the ports
        // map entry is removed below.
        if self.scan.take_pending() && self.ports.is_some() {
            let lines = self.screen_lines();
            let lines: Vec<&str> = lines.iter().map(String::as_str).collect();
            self.scan_ports(&lines);
        }
        if let Err(e) = self.log.flush() {
```

Compile check: `cargo check -p mkrew-daemon -j 4`
Expected: no errors, no new warnings (`screen_lines` is still used by `scan_screen` and `finish`).

- [ ] **Step 5: Daemon e2e still holds**

Run: `cargo test -p mkrew-daemon --test daemon -j 4 -- fake_agent_session_reaches_needs_input send_while_working a_queued_text_removed exiting_with_queued run_sessions_report_the_ports`
Expected: PASS (the prompt is found by the trailing scan; queued texts still go one at a time because a non-empty queue scans every batch; a run that prints its port and exits at once still reports it, via the scan in `finish`). Build `mkrew-fakeagent` first if the harness needs it (`cargo build -p mkrew-fakeagent -p mkrew-cli -j 4`).

- [ ] **Step 6: ADR 0057 and SPEC**

Create `docs/decisions/0057-terminal-path-limits.md`:

```markdown
# 0057 — Terminal path limits: screen scan rate, UI scrollback, log replay

- Date: 2026-10-13
- Status: accepted
- Milestone: post-M13 perf (docs/PERF_REVIEW.md B9, B10, B12)

## Context

The perf review found three costs on the terminal path. (B9) The daemon hashes every cell of a
session's screen after each output batch (up to 60/s per session, background ones too) and, when
the hash changed, builds a `String` for every row up to the cursor and runs the state patterns,
although the state machine reads only the last `PATTERN_TAIL_LINES` = 4 rows (ADR 0015 added the
fingerprint, ADR 0003 the state source). (B10) The UI keeps 10,000 scrollback lines per pane and
rebuilds its whole cell grid, each cell with a `Vec<char>`, on every frame with output. (B12)
Opening an exited session in History replays its log into a new emulator.

## Options

1. Cut the cost per scan only (build 4 rows), keep one scan per batch.
2. Also cap the scan rate, with a trailing scan so the final screen is always seen.
3. Skip the scan for agents with hooks. Rejected: no hook reports `idle`, Codex hooks report only
   `done`, and the daemon cannot tell whether the user installed the hooks.

## Decision

- **Screen scan (B9).** Option 2. The state machine gets only the last 4 non-blank rows up to the
  cursor (`Emulator::tail_to_cursor`). A session's screen is scanned at most every 250 ms
  (`scan::ScanRate`); output inside the gap is scanned by a trailing scan when the gap ends, so a
  prompt that is the last thing printed is never missed. A session with a queued `session.send`
  scans every batch (the send queue needs the current state). A state change can therefore show up
  to 250 ms after the output that caused it. The fingerprint (ADR 0015) is compared between two
  scans: a change that is undone inside one gap is not seen.

## Consequences

- SPEC §6 and §8 say so.
- `working` can start up to 250 ms late and `needs_input` up to 250 ms late; `idle` (3 s quiet) is
  unaffected.
```

`docs/SPEC.md` §6: after the sentence ending `…gives garbage.` (:460) add a new paragraph:

```markdown
The daemon scans a session's screen at most 4 times a second (a scan after the last output
closes the gap, so the final screen is always classified) and the state machine reads only the
last 4 non-blank rows up to the cursor; a state shown by the screen can therefore appear up to
250 ms after the output (ADR 0057). A session with a queued `session.send` is scanned on every
output.
```

§8 (:631-632): replace
```markdown
- Output fan-out per chunk: emulator → state detector, session log file (buffered writer),
  attached clients (binary frames, §7).
```
with
```markdown
- Output fan-out per chunk: emulator → state detector (at most 4 scans/s per session, ADR 0057),
  session log file (buffered writer), attached clients (binary frames, §7).
```

- [ ] **Step 7: HUMAN_CHECKS**

Append to the end of the perf section (`### To verify — 2026-10-13 perf (plan docs/superpowers/plans/2026-10-10-inbox-perf.md)`; Task 1 creates it above the first `### To verify —` heading, create it the same way if it is still missing) in `docs/HUMAN_CHECKS.md`, after its last bullet:

```markdown
- [ ] **Terminal path, screen scan (T13):** restart the daemon. With claude (hooks off, patterns only) and a fake agent or a shell in the other two panes:
  - Ask claude something that needs approval: the pane shows `needs_input` (yellow) and the notification fires within about half a second, also when the prompt is the last thing printed and the pane is in a background tab.
  - Answer it: the state goes back to `working`, then `idle` about 3 s after the output stops.
  - `mk send <id> "one"` and `mk send <id> "two"` while the agent works: both are typed one after the other, "two" only after the agent reacted to "one".
  - A run command that prints `Local: http://localhost:5173` shows `:5173` in the Run button, also when it prints that line and exits at once.
  - Task Manager: the daemon's CPU with three agents `working` is lower than before (note the numbers here).
```

- [ ] **Step 8: Verify and commit**

Run: `cargo test -p mkrew-daemon --lib -j 4 -- scan emulator` and `cargo test -p mkrew-core state -j 4` (expect PASS), then `sh scripts/verify.sh` (fallback if rustc crashes: `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace -j 4`). Do not tick inbox line 138 yet (Tasks 14 and 15 follow).

```
git add crates/mkrew-daemon/src/scan.rs crates/mkrew-daemon/src/lib.rs crates/mkrew-daemon/src/emulator.rs crates/mkrew-daemon/src/session.rs crates/mkrew-core/src/state.rs docs/decisions/0057-terminal-path-limits.md docs/SPEC.md docs/HUMAN_CHECKS.md
git commit -m "perf(daemon): scan the screen at most 4 times a second and build only the 4 lines the state machine reads"
```
(`session.rs`, `lib.rs`, `SPEC.md` and `HUMAN_CHECKS.md` are shared with other tasks: if another task's hunks are uncommitted there, use `git add -p` for these four and stage only this task's hunks.)

---

### Task 14: UI scrollback 5,000 lines and a reusable terminal grid (perf B10)

> **Refresh 2026-10-13:** still needed (`crates/mkrew-ui/src/terminal/emulator.rs:26` is still 10_000; `grid()` still allocates `vec![vec![blank; cols]; rows]` with a `Vec<char>` per cell, `:357-405`). Changes vs 2026-10-10:
> - ADR 0057 (not 0043). SPEC §10 line is now `SPEC.md:850`.
> - 95a8a6e (`OUTPUT_FRAME` = 16 ms, `main.rs:128`, used in `shell/connection.rs:174-197`) already caps how often a flooding pane is fed and so how often `grid()` is rebuilt (≤ ~60/s per window). The per-rebuild cost (rows × cols cells, a `Vec` per row and per zero-width cell) is unchanged by it, so the buffer reuse stays; it only has to cope with a grid rebuilt once per frame, which it does: the painter's `Prepainted.grid: Arc<Grid>` (`view.rs:839-840, 915-916`) is dropped when the frame ends, before the next feed.
> - The old Task 14 test and doc anchors are re-based on the current `emulator.rs`/`layout.rs`/`stress.rs` lines; `tests/stress.rs` (`WARM_LINES`) now needs the new scrollback number.

Inbox: line 138 (bullet B10; do not tick yet). Spec: §10 terminal rendering (SPEC.md:849-859), ADR 0030 (`docs/decisions/0030-snapshot-carries-history.md`), ADR 0057.

**Current state (investigated 2026-10-13):**
- `crates/mkrew-ui/src/terminal/emulator.rs:26`: `const SCROLLBACK_LINES: usize = 10_000;` (used in `Emulator::new` as `scrolling_history`, and by tests at `:671`, `:722`, `:724` through the constant, so they follow the new value). The comment in `view_point` (`:346`) says `the history 10k lines`.
- `Emulator::grid()` (`:357-405`) builds `vec![vec![blank; cols]; screen_lines]` (a new `Vec` per row; every `GridCell` holds `zerowidth: Vec<char>`, `:36`), then overwrites every cell from `renderable_content().display_iter`. `shared_grid()` (`:352`) calls it inside `last_grid.get_or_insert_with` once per change, and `Emulator::changed()` (`:143`, called by every `&mut self` method: `advance`, `resize`, scroll, selection, search) sets `last_grid` to `None`.
- Consumers of the cell type: `Grid::to_text` (`:86`) and `Grid::row_chars` (`:96`, returns `Vec<char>` of base chars, not zero-width ones); `layout.rs` (`layout_row` `:58`: `!cell.zerowidth.is_empty()` at `:83`; `is_blank` `:107-109`; `push_text` `:111-114`: `segment.text.extend(cell.zerowidth.iter())`; `CachedRow { cells: Vec<GridCell> }` `:296` + `RowCache::row` compares `self.cells == cells` and `cells.to_vec()` at `:354`); `view.rs` (`Prepainted.grid: Arc<Grid>` `:839-840`, taken from `shared_grid()` at `:915-916`; `Pane::link_at` `:359-361` uses `shared_grid().row_chars(row)`); `links.rs` takes `&[char]` and never sees the cell type; `find.rs` tests read `.found`; selection and copy use alacritty's `selection_to_string`, not the grid; tests `tests/terminal_runs.rs` (`grid.rows`, `layout_row`) and `tests/stress.rs` (`pane.grid().to_text()`). No file outside `terminal/` constructs `GridCell { … }` (grep `GridCell \{` finds only `emulator.rs`).
- Painting holds an `Arc<Grid>` (`Prepainted`) only for the duration of a frame, so by the next change `last_grid` is normally the sole owner and `Arc::try_unwrap` succeeds; `link_at` clones it only inside one call.
- SPEC §10 (`SPEC.md:850`) says `scrollback (UI-side, 10,000 lines, only for attached panes),`. `tests/stress.rs:48-49` has `WARM_LINES = 10_100` with the comment `(the UI emulator keeps 10,000)`. HUMAN_CHECKS mentions the 10k scrollback only in old, historical records (not edited). ADR 0030's option 2 mentions "10,000 lines each" as a rejected option (not edited). SPEC §20-notes `the 10k-line history` (HUMAN_CHECKS.md ~:1915, a historical implementation note) is not edited.
- `Cursor` and `Grid` do not derive `Default` today.
- Chosen type: `Option<Box<[char]>>` (`None` for ~every cell: no allocation, 16 bytes; the field cannot be `Some(empty)`), with an accessor `zerowidth() -> &[char]` so consumers stay one-liners.

**Files:**
- Modify: `crates/mkrew-ui/src/terminal/emulator.rs` (`SCROLLBACK_LINES`, `GridCell`, `Cursor`/`Grid` derives, `Grid::to_text`, `Emulator` field `spare`, `new`, `changed`, `shared_grid`, `grid`, new `fill_grid`, comment in `view_point`, tests)
- Modify: `crates/mkrew-ui/src/terminal/layout.rs` (`layout_row`, `is_blank`, `push_text`, test)
- Modify: `crates/mkrew-ui/tests/stress.rs` (`WARM_LINES`, its comment)
- Modify: `docs/SPEC.md` §10 (:850)
- Modify: `docs/decisions/0057-terminal-path-limits.md` (append bullets)
- Modify: `docs/HUMAN_CHECKS.md` (append to the shared perf section)

**Interfaces:**
- Produces: `GridCell { …, pub zerowidth: Option<Box<[char]>>, … }` with `pub fn zerowidth(&self) -> &[char]`.
- Produces: `Grid: Default`, `Cursor: Default`; `Emulator::grid(&self) -> Grid` unchanged signature (a fresh grid); `Emulator::shared_grid(&self) -> Arc<Grid>` unchanged signature, now rebuilt into the previous frame's buffer when nobody else holds it.

- [ ] **Step 1: Failing tests**

`crates/mkrew-ui/src/terminal/emulator.rs` `mod tests`, after `resize_changes_dimensions`:

```rust
    #[test]
    fn zero_width_chars_survive_in_the_grid() {
        let mut emu = Emulator::new(10, 2);
        // e + combining acute, a space, a wide thumbs-up + VS16, x.
        emu.advance("e\u{301} \u{1F44D}\u{FE0F}x".as_bytes());
        let grid = emu.grid();
        assert_eq!(grid.rows[0][0].c, 'e');
        assert_eq!(grid.rows[0][0].zerowidth().to_vec(), vec!['\u{301}']);
        assert!(grid.rows[0][1].zerowidth().is_empty());
        assert_eq!(grid.rows[0][2].c, '\u{1F44D}');
        assert_eq!(grid.rows[0][2].zerowidth().to_vec(), vec!['\u{FE0F}']);
        assert_eq!(
            grid.to_text().lines().next(),
            Some("e\u{301} \u{1F44D}\u{FE0F}x")
        );
    }

    /// A grid being painted is shared: the next frame is built elsewhere, never over it.
    #[test]
    fn a_grid_still_being_painted_is_never_overwritten() {
        let mut emu = Emulator::new(10, 2);
        emu.advance(b"old");
        let painting = emu.shared_grid();
        emu.advance(b"\rnew");
        let next = emu.shared_grid();
        assert!(!Arc::ptr_eq(&painting, &next));
        assert_eq!(painting.to_text().lines().next(), Some("old"));
        assert_eq!(next.to_text().lines().next(), Some("new"));
    }

    /// The frame after a released one is built into the same cell buffers (no per-frame
    /// allocation of rows x cols cells).
    #[test]
    fn a_released_grid_buffer_is_reused_for_the_next_frame() {
        let mut emu = Emulator::new(10, 3);
        emu.advance(b"one");
        let first = emu.shared_grid().rows[0].as_ptr();
        emu.advance(b"\rtwo");
        let next = emu.shared_grid();
        assert_eq!(next.rows[0].as_ptr(), first, "same cell buffer");
        assert_eq!(next.to_text().lines().next(), Some("two"));
    }

    /// A rebuilt grid never shows cells of the buffer it reused: after a shrink, a grow and a
    /// cleared selection it equals a grid built from scratch, in every dimension.
    #[test]
    fn a_reused_buffer_shows_no_stale_cells() {
        let mut emu = Emulator::new(12, 4);
        emu.advance("hello e\u{301}\r\nworld\r\nthird\r\nfourth".as_bytes());
        emu.start_selection(0, 0, Side::Left);
        emu.update_selection(1, 3, Side::Right);
        drop(emu.shared_grid());

        emu.resize(5, 2);
        let small = emu.shared_grid();
        assert_eq!((small.cols, small.rows.len()), (5, 2));
        assert!(small.rows.iter().all(|row| row.len() == 5));
        assert_eq!(*small, emu.grid());
        drop(small);

        emu.clear_selection();
        emu.resize(14, 6);
        let big = emu.shared_grid();
        assert_eq!((big.cols, big.rows.len()), (14, 6));
        assert!(big.rows.iter().all(|row| row.len() == 14));
        assert!(
            big.rows
                .iter()
                .flatten()
                .all(|cell| !cell.selected && cell.found == Found::No)
        );
        assert_eq!(*big, emu.grid());
    }
```

`crates/mkrew-ui/src/terminal/layout.rs` `mod tests`, after the helper `fn style` (:379-387):

```rust
    /// A base char with zero-width chars is shaped as one cluster (its own segment), so the
    /// accent is not drawn on the next cell.
    #[test]
    fn zero_width_chars_stay_with_their_base_cell() {
        let theme = Theme::dark();
        let cells = row("ae\u{301}b".as_bytes(), 8);
        let layout = layout_row(&cells, None, &theme);
        let segments: Vec<(&str, usize)> = layout
            .segments
            .iter()
            .map(|segment| (segment.text.as_str(), segment.start_col))
            .collect();
        assert_eq!(segments, [("a", 0), ("e\u{301}", 1), ("b", 2)]);
    }
```

Run: `cargo test -p mkrew-ui --lib -j 4 -- terminal`
Expected: FAIL to compile (`GridCell::zerowidth()` does not exist).

- [ ] **Step 2: Cell type without a `Vec` per cell**

`emulator.rs`:

Replace
```rust
const SCROLLBACK_LINES: usize = 10_000;
```
with
```rust
const SCROLLBACK_LINES: usize = 5_000;
```
In `view_point`, replace `the history 10k lines` with `the history 5k lines`.

Replace
```rust
    /// Zero-width chars attached to `c` (combining marks, VS16, ZWJ sequences).
    pub zerowidth: Vec<char>,
```
with
```rust
    /// Zero-width chars attached to `c` (combining marks, VS16, ZWJ sequences). `None` for
    /// nearly every cell, so a grid holds no allocation per cell; never `Some(empty)`.
    pub zerowidth: Option<Box<[char]>>,
```
After the `GridCell` struct, add:
```rust
impl GridCell {
    /// The zero-width chars attached to [`GridCell::c`], in order.
    pub fn zerowidth(&self) -> &[char] {
        self.zerowidth.as_deref().unwrap_or(&[])
    }
}
```
`#[derive(Clone, Copy, Debug, PartialEq, Eq)] pub struct Cursor` → `#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]`; `#[derive(Clone, Debug, PartialEq)] pub struct Grid` → `#[derive(Clone, Debug, Default, PartialEq)]`.

In `Grid::to_text` replace `.flat_map(|cell| std::iter::once(cell.c).chain(cell.zerowidth.iter().copied()))` with `.flat_map(|cell| std::iter::once(cell.c).chain(cell.zerowidth().iter().copied()))`.

`layout.rs`: in `layout_row` replace `|| !cell.zerowidth.is_empty() {` with `|| !cell.zerowidth().is_empty() {`; in `is_blank` replace `cell.zerowidth.is_empty()` with `cell.zerowidth().is_empty()`; in `push_text` replace `segment.text.extend(cell.zerowidth.iter());` with `segment.text.extend(cell.zerowidth().iter());`.

`crates/mkrew-ui/tests/stress.rs:48-49`: replace
```rust
/// A pane fed this many lines has a full scrollback (the UI emulator keeps 10,000) and screen.
const WARM_LINES: usize = 10_100;
```
with
```rust
/// A pane fed this many lines has a full scrollback (the UI emulator keeps 5,000) and screen.
const WARM_LINES: usize = 5_100;
```

- [ ] **Step 3: Rebuild into the released buffer**

`emulator.rs`, struct `Emulator`: after the `last_grid` field (:124) add
```rust
    /// The previous grid when [`Emulator::changed`] found no one painting it: the next
    /// [`Emulator::shared_grid`] rebuilds into its buffers instead of allocating rows x cols
    /// cells per frame.
    spare: RefCell<Option<Grid>>,
```
In `Emulator::new`, after `last_grid: RefCell::default(),` add `spare: RefCell::default(),`.

Replace
```rust
    fn changed(&mut self) {
        *self.last_grid.get_mut() = None;
    }
```
with
```rust
    fn changed(&mut self) {
        // Sole owner (the painter dropped its clone): keep the buffers for the next frame.
        // A grid still being painted is left alone and freed by its last owner.
        if let Some(last) = self.last_grid.get_mut().take()
            && let Ok(grid) = Arc::try_unwrap(last)
        {
            *self.spare.get_mut() = Some(grid);
        }
    }
```
Replace `shared_grid` (:351-355, doc comment kept) body with
```rust
    pub fn shared_grid(&self) -> Arc<Grid> {
        let mut last = self.last_grid.borrow_mut();
        Arc::clone(last.get_or_insert_with(|| {
            let mut grid = self.spare.borrow_mut().take().unwrap_or_default();
            self.fill_grid(&mut grid);
            Arc::new(grid)
        }))
    }
```
Replace the whole `pub fn grid(&self) -> Grid { … }` (:357-405, from `pub fn grid(&self) -> Grid {` to its closing `}`, ending with `Grid { cols, rows, cursor }`) with:

```rust
    /// A freshly built snapshot of the visible screen ([`Emulator::shared_grid`] reuses
    /// buffers instead).
    pub fn grid(&self) -> Grid {
        let mut grid = Grid::default();
        self.fill_grid(&mut grid);
        grid
    }

    /// Rewrites `grid` to show the visible screen: every row is cleared and refilled with blank
    /// cells first, so nothing of the previous frame (cells, selection, find marks, zero-width
    /// chars, a different size) survives.
    fn fill_grid(&self, grid: &mut Grid) {
        let cols = self.term.columns();
        let blank = GridCell {
            c: ' ',
            zerowidth: None,
            fg: Color::Named(NamedColor::Foreground),
            bg: Color::Named(NamedColor::Background),
            flags: Flags::empty(),
            selected: false,
            found: Found::No,
        };
        grid.cols = cols;
        grid.rows.resize_with(self.term.screen_lines(), Vec::new);
        for row in &mut grid.rows {
            row.clear();
            row.resize(cols, blank.clone());
        }
        let content = self.term.renderable_content();
        let offset = content.display_offset as i32;
        let selection = content.selection;
        for indexed in content.display_iter {
            let row = (indexed.point.line.0 + offset) as usize;
            let col = indexed.point.column.0;
            if let Some(slot) = grid.rows.get_mut(row).and_then(|r| r.get_mut(col)) {
                *slot = GridCell {
                    selected: selection.is_some_and(|range| range.contains(indexed.point)),
                    found: Found::No,
                    c: indexed.cell.c,
                    zerowidth: indexed
                        .cell
                        .zerowidth()
                        .filter(|chars| !chars.is_empty())
                        .map(Box::from),
                    fg: indexed.cell.fg,
                    bg: indexed.cell.bg,
                    flags: indexed.cell.flags,
                };
            }
        }
        grid.cursor = Cursor {
            row: (content.cursor.point.line.0 + offset).max(0) as usize,
            col: content.cursor.point.column.0,
            visible: content.cursor.shape != CursorShape::Hidden,
        };
        // One pass over the matched cells (the current match last, so it wins), not a lookup
        // per cell: a one-letter query can match thousands of times on screen.
        for found in self.visible_matches() {
            mark(&mut grid.rows, &found, offset, Found::Match);
        }
        if let Some(current) = &self.current_match {
            mark(&mut grid.rows, current, offset, Found::Current);
        }
    }
```
(The body of the old `grid()` that this replaces is the same logic: blank cell, `display_iter` overwrite, cursor, `visible_matches` / `current_match` marks. Only the target changed from local `rows` to `grid.rows` and the per-frame allocations went away.)

Run: `cargo test -p mkrew-ui --lib -j 4 -- terminal`
Expected: PASS (the four new emulator tests, the layout test, and the existing `shared_grid_is_rebuilt_after_every_change`, `selection_marks_cells_and_copies_text`, `history_is_capped_at_the_scrollback_limit`, find tests). Then `cargo test -p mkrew-ui --test terminal_runs -j 4` (expect PASS, snapshot unchanged: the grid content is identical).

- [ ] **Step 4: SPEC and ADR**

`docs/SPEC.md:850`, replace `scrollback (UI-side, 10,000 lines, only for attached panes),` with `scrollback (UI-side, 5,000 lines, only for attached panes; ADR 0057),`.

`docs/decisions/0057-terminal-path-limits.md`: after the `- **Screen scan (B9).** …` bullet in `## Decision` add:
```markdown
- **UI scrollback (B10).** 10,000 lines per pane become 5,000 (RAM and resize reflow grow with the
  history); the log on disk still keeps everything. A `GridCell` keeps its zero-width chars in an
  `Option<Box<[char]>>` instead of a `Vec<char>`, and `Emulator::shared_grid` rebuilds into the
  buffers of the previous frame when the painter released it. The repaint cap of one fed batch per
  16 ms frame (`OUTPUT_FRAME`) is unchanged: it bounds how often the grid is rebuilt, this bounds
  what a rebuild costs.
```
and after the last bullet of `## Consequences` add:
```markdown
- SPEC §10 says 5,000 lines. A pane scrolls back at most 5,000 lines; older output is in History
  (the session log).
```

- [ ] **Step 5: HUMAN_CHECKS**

Append to the end of the perf section in `docs/HUMAN_CHECKS.md`:

```markdown
- [ ] **Terminal path, scrollback and grid (T14):**
  - In a pwsh pane run `1..8000 | % { "line $_" }`, then Shift+PgUp until it stops: the oldest line is about `line 3000`-`3100`, and Ctrl+Shift+F finds `line 7000` but not `line 100`. `mk session history <id>` still prints all 8000 lines.
  - In Git Bash run `printf 'e\xcc\x81 \xf0\x9f\x91\x8d\xef\xb8\x8f \xf0\x9f\x91\xa8\xe2\x80\x8d\xf0\x9f\x91\xa9 x\n'`: the accent sits on the `e`, the thumbs-up and the person pair are drawn whole and `x` is on the same line, in light and dark.
  - Drag the pane splitter and the window edge while a pane prints: no leftover glyphs or stripes after the resize, selection highlight and find highlight follow the text.
  - With six panes flooding (the stress setup of T2.9) the UI memory is lower than with 10,000 lines (note the numbers).
```

- [ ] **Step 6: Verify and commit**

Run: `cargo test -p mkrew-ui --lib -j 4 -- terminal` and `cargo test -p mkrew-ui --test terminal_runs --test stress -j 4` (expect PASS; the memory/timing asserts of `stress` apply in release only), then `sh scripts/verify.sh` (fallback as in Task 13). Do not tick inbox line 138 yet (Task 15 follows).

```
git add crates/mkrew-ui/src/terminal/emulator.rs crates/mkrew-ui/src/terminal/layout.rs crates/mkrew-ui/tests/stress.rs docs/SPEC.md docs/decisions/0057-terminal-path-limits.md docs/HUMAN_CHECKS.md
git commit -m "perf(ui): 5,000-line pane scrollback and a terminal grid that reuses its buffer without a Vec per cell"
```
(`SPEC.md` and `HUMAN_CHECKS.md` are shared: `git add -p` if other tasks' hunks are uncommitted.)

---

### Task 15: History replays only the last 1 MiB of a log, from a safe boundary (perf B12)

> **Refresh 2026-10-13:** still needed. Changes vs 2026-10-10:
> - `read_tail` and `Tail` moved to **`crates/mkrew-core/src/log_tail.rs`** (c71282f); the desktop (`crates/mkrew-ui/src/shell/history.rs:321`) and `mk web` (`crates/mkrew-cli/src/web/relay.rs:97`, `TAIL_BYTES = 512 KiB`, line 40) both call it. The safe-cut function `replay_start` and its tests therefore go into `mkrew-core` and fix both callers; only the cap constant `history::TAIL_BYTES` (UI, `history.rs:12`, 2 MiB → 1 MiB) stays in the UI. The web relay keeps 512 KiB (it is already below the new cap); only its stale comment (`relay.rs:39`, "the desktop History dialog shows 2 MiB") changes.
> - ADR 0057 (not 0043). SPEC.md has no History-replay note (the old "SPEC :1826/:1829" text lives in `docs/HUMAN_CHECKS.md` implementation notes at :2178-2182 and the T5.9 check at :1193); those wordings are edited there, and SPEC §15's History row (`SPEC.md:1359`) gets the cap.
> - `read_tail` still drops only up to the first `\n` of a cut stream (`log_tail.rs:42-44`): the no-line-break bug is still there.

Inbox: line 138 (bullet B12; ticks `[~]`→`[x]`: the last task of item 14). Spec: §8 session logs, §15 (SPEC.md:1359), ADR 0030, ADR 0052 (web relay), ADR 0057.

**Current state (investigated 2026-10-13; the review's premise is out of date):**
- The review says opening an exited session replays ~10 MB. `TerminalPane::replay(session, log: &[u8], …)` only builds an `Emulator` and feeds it whatever bytes it is given. Its caller `Shell::load_history_log` (`crates/mkrew-ui/src/shell/history.rs:299-366`) already reads `mkrew_core::log_tail::read_tail(&history.files, history::TAIL_BYTES)` on `background_executor` (off the UI thread), and `crates/mkrew-ui/src/history.rs:12` has `pub const TAIL_BYTES: u64 = 2 * 1024 * 1024;`. Today it replays at most **2 MiB**, not 10 MB. This task lowers the cap to 1 MiB (the inbox's ~1 MB).
- The daemon does not serve the log bytes: `Request::SessionHistory` returns only the paths (`<id>.log.1`, `<id>.log`; rotation at 5 MB), and `read_tail` opens them in the client process. No protocol change.
- `read_tail` (`crates/mkrew-core/src/log_tail.rs:20-46`) reads from the end (`seek(len - left)`), so only the cap is read. When cut it drops everything up to the first `\n` of the joined bytes (`:42-44`). `\n` never occurs inside a UTF-8 multibyte char, so the char boundary and the escape sequences are safe. **Bug:** when the cut tail has no `\n` at all (a TUI that repaints with cursor moves), nothing is dropped and the replay starts mid-sequence or mid-char. The fix is a pure fn `replay_start` with fallbacks. Existing test: `read_tail_joins_the_parts_and_cuts_at_a_line_break` (`log_tail.rs:53`), stays valid.
- No SGR reset is needed: every replay is a fresh emulator with default attributes, and the cut drops whole lines before the start. Modes set before the cut (alt screen `ESC[?1049h`) are lost; the HUMAN_CHECKS T5.9 note (:2179-2181) already documents that a session ending on the alternate screen shows its last screen with little scrollback.
- ADR 0030: attach snapshots carry the daemon's ≤ 200 history lines, "older output is in the History dialog (session log)". Consistent: History stays the place for old output, now the newest 1 MiB; the dialog's note (`shell/history.rs:684`, `Older output is left out; mk session history {} prints all of it.`) already points to the full log.
- UI test `a_replayed_tail_shows_the_sessions_last_screen` (`crates/mkrew-ui/src/history.rs:301-313`): the cut lands inside an escape sequence of the old part with a line break after it; stays valid.
- "Log write blocking on the session task" (review): `Actor::on_output` calls `SessionLog::write` (a `BufWriter` with the default 8 KiB buffer) on the session's async task. Not cheap and safe to fix (see Open questions): not changed here.
- Wording to update: `docs/HUMAN_CHECKS.md:1193` `A log > 2 MB shows the "Older output is left out" note.`; `:2179` `newest 2 MiB only, starting after the first line break past the cut;`; `:2182` `(a 2 MiB log may stall one frame)`. (Line 2182's "stall one frame" remark is now stale in its own right: the read is off the UI thread; only the replay `advance` runs on it.)

**Files:**
- Modify: `crates/mkrew-core/src/log_tail.rs` (new `replay_start`, `read_tail` cut, tests)
- Modify: `crates/mkrew-ui/src/history.rs` (`TAIL_BYTES`)
- Modify: `crates/mkrew-cli/src/web/relay.rs` (comment at :39 only)
- Modify: `docs/SPEC.md` (§15 History row, :1359)
- Modify: `docs/decisions/0057-terminal-path-limits.md` (append bullets)
- Modify: `docs/HUMAN_CHECKS.md` (:1193, :2179, :2182 wording; append to the shared perf section)
- Modify: `docs/INBOX.md` (tick line 138; left uncommitted, INBOX is never part of a task commit)

**Interfaces:**
- Produces: `pub fn replay_start(bytes: &[u8]) -> usize` in `crates/mkrew-core/src/log_tail.rs` (`mkrew_core::log_tail::replay_start`).
- Changes: `pub const TAIL_BYTES: u64` (UI) = 1 MiB. `read_tail(files: &[PathBuf], limit: u64) -> io::Result<Tail>` keeps its signature.

- [ ] **Step 1: Failing tests**

`crates/mkrew-core/src/log_tail.rs` `mod tests`, after `read_tail_joins_the_parts_and_cuts_at_a_line_break`:

```rust
    #[test]
    fn replay_start_prefers_a_line_break_then_an_escape_then_a_char_boundary() {
        // The tail of "€" (E2 82 AC) cut after its first byte, then a whole line.
        assert_eq!(replay_start(b"\x82\xac rest\nnext"), 8);
        // A line break wins even when an escape sequence comes first.
        assert_eq!(replay_start(b"x\x1b[1my\nz"), 7);
        // No line break: the first ESC starts a whole sequence ("1;32m" is the end of one).
        assert_eq!(replay_start(b"1;32mgreen\x1b[0m"), 10);
        // Neither: only the continuation bytes of a cut char go.
        assert_eq!(replay_start(b"\x82\xacabc"), 2);
        assert_eq!(replay_start(b"\x82\xac"), 2);
        assert_eq!(replay_start(b"abc"), 0);
        assert_eq!(replay_start(b"abc\n"), 4);
        assert_eq!(replay_start(b""), 0);
    }

    /// Marks the bytes inside a CSI sequence (after its ESC, up to and including its final byte).
    fn inside_csi(stream: &[u8]) -> Vec<bool> {
        let mut inside = vec![false; stream.len()];
        let mut open = false;
        for (at, &byte) in stream.iter().enumerate() {
            if byte == 0x1b {
                open = true;
                continue;
            }
            if open {
                inside[at] = true;
                if (0x40..=0x7e).contains(&byte) && byte != b'[' {
                    open = false;
                }
            }
        }
        inside
    }

    /// SGR, a wide char, a 3-byte char, cursor moves, a last line without a break.
    const STREAM: &str = "\x1b[1;32mDone\x1b[0m \u{5f62} \u{20ac}5\r\n\x1b[2K\x1b[1Ahello\r\n\x1b[38;5;196mred\x1b[0m\r\nlast";

    #[test]
    fn a_replay_never_starts_inside_a_char_or_an_escape_sequence() {
        let stream = STREAM.as_bytes();
        let inside = inside_csi(stream);
        for cut in 0..=stream.len() {
            let start = cut + replay_start(&stream[cut..]);
            assert!(
                std::str::from_utf8(&stream[start..]).is_ok(),
                "cut {cut}: starts inside a char"
            );
            assert!(
                start == stream.len() || !inside[start],
                "cut {cut}: starts inside an escape sequence"
            );
        }
    }

    #[test]
    fn a_cut_log_without_line_breaks_starts_at_an_escape_sequence() {
        let dir = tempfile::tempdir().unwrap();
        let log = dir.path().join("s.log");
        std::fs::write(&log, b"\x1b[1mbold\x1b[0m long line no break").unwrap();
        // The last 26 bytes begin inside "bold": that text goes, the next sequence stays.
        let tail = read_tail(&[log], 26).unwrap();
        assert!(tail.cut);
        assert_eq!(tail.bytes, b"\x1b[0m long line no break");
    }

    #[test]
    fn a_big_log_is_read_as_whole_lines_within_the_limit() {
        let dir = tempfile::tempdir().unwrap();
        let log = dir.path().join("s.log");
        std::fs::write(&log, b"0123456789abcdef\r\n".repeat(200_000)).unwrap();
        let limit = 1024 * 1024;
        let tail = read_tail(&[log], limit).unwrap();
        assert!(tail.cut);
        let len = tail.bytes.len() as u64;
        assert!(len <= limit && len > limit - 64, "{len} bytes");
        assert!(tail.bytes.starts_with(b"0123456789abcdef"));
    }
```
Byte check: `b"\x82\xac rest\nnext"` is 2 + 5 + 1 = 8 bytes before `next`; `b"x\x1b[1my\nz"`: x, ESC, `[`, `1`, `m`, y, `\n` = 7; `b"1;32mgreen"` = 10 bytes before the ESC; the 31-byte log of the third test minus its first 5 bytes (`ESC [ 1 m b`) is the 26 bytes `old` + ESC`[0m` + ` long line no break`.

Run: `cargo test -p mkrew-core --lib -j 4 -- log_tail`
Expected: FAIL to compile (`replay_start` does not exist).

- [ ] **Step 2: Cap and safe boundary**

`crates/mkrew-core/src/log_tail.rs`: replace the doc comment and head of `read_tail` (:17-19)
```rust
/// Reads the last `limit` bytes of `files` as one stream. A cut stream starts after its first
/// line break, so the replay does not begin inside an escape sequence. A file that is gone
/// (rotated away meanwhile) is skipped.
```
with
```rust
/// Where the replay of a cut log may start: just after its first line break (a UTF-8 char never
/// contains `\n`, and the partial line before it is dropped whole); with no line break, at the
/// first ESC (a whole escape sequence); with neither, after the continuation bytes of a cut
/// char. A fresh emulator starts with default attributes, so nothing needs resetting.
// ponytail: a log with no line break and no ESC for the whole tail keeps its first bytes as they
// are (plain text, harmless); an ESC inside an OSC/DCS string would start there. Scan back from
// before the cut if a real log shows it.
pub fn replay_start(bytes: &[u8]) -> usize {
    if let Some(newline) = bytes.iter().position(|&byte| byte == b'\n') {
        return newline + 1;
    }
    if let Some(escape) = bytes.iter().position(|&byte| byte == 0x1b) {
        return escape;
    }
    // 0b10xx_xxxx: the continuation bytes of a UTF-8 char.
    bytes
        .iter()
        .position(|&byte| byte & 0xC0 != 0x80)
        .unwrap_or(bytes.len())
}

/// Reads the last `limit` bytes of `files` as one stream. A cut stream starts at
/// [`replay_start`], so the replay does not begin inside an escape sequence or a char. A file
/// that is gone (rotated away meanwhile) is skipped.
```
In the body of `read_tail`, replace
```rust
    if cut && let Some(newline) = bytes.iter().position(|&byte| byte == b'\n') {
        bytes.drain(..=newline);
    }
```
with
```rust
    if cut {
        bytes.drain(..replay_start(&bytes));
    }
```
Update the module doc if it names the cap (it does not; `//!` stays).

`crates/mkrew-ui/src/history.rs:11-12`: replace
```rust
/// Most log bytes replayed (the newest ones); `mk session history <id>` prints all of it.
pub const TAIL_BYTES: u64 = 2 * 1024 * 1024;
```
with
```rust
/// Most log bytes replayed (the newest ones, ADR 0057); `mk session history <id>` prints all of it.
pub const TAIL_BYTES: u64 = 1024 * 1024;
```

`crates/mkrew-cli/src/web/relay.rs:39`: replace `(the desktop History dialog shows 2 MiB)` with `(the desktop History dialog shows 1 MiB)`. The value `TAIL_BYTES = 512 * 1024` stays.

Run: `cargo test -p mkrew-core --lib -j 4 -- log_tail` and `cargo test -p mkrew-ui --lib -j 4 -- history`
Expected: PASS (the four new core tests; the existing `read_tail_joins_the_parts_and_cuts_at_a_line_break` and the UI `a_replayed_tail_shows_the_sessions_last_screen` are unchanged and pass).

- [ ] **Step 3: SPEC, ADR, HUMAN_CHECKS wording**

`docs/SPEC.md:1359`, replace `| History | per-session output log, rotated at 5 MB |` with `| History | per-session output log, rotated at 5 MB; the History dialog replays its newest 1 MiB (ADR 0057) |`.

`docs/decisions/0057-terminal-path-limits.md`: after the `- **UI scrollback (B10).** …` bullet add:
```markdown
- **History replay (B12).** Opening an exited session replays the newest 1 MiB of its log (was
  2 MiB; the review's "10 MB" was the rotated log size, the UI already cut at 2 MiB). The cut
  starts after the first line break, else at the first ESC, so the emulator never starts inside a
  char or an escape sequence (`mkrew_core::log_tail::replay_start`, shared with `mk web`, which
  keeps its own 512 KiB cap). Older output stays in the log (`mk session history`), as ADR 0030
  says for output beyond the daemon's 200 history lines.
```
and after the last bullet of `## Consequences` add:
```markdown
- History shows the newest 1 MiB; the dialog's note points to `mk session history` for the rest.
```

`docs/HUMAN_CHECKS.md`: at :1193 replace `A log > 2 MB shows the "Older output is left out" note.` with `A log > 1 MB shows the "Older output is left out" note.`; at :2179 replace `newest 2 MiB only, starting after the first line break past the cut;` with `newest 1 MiB only (ADR 0057), starting after the first line break past the cut (at the first escape sequence when there is none);`; at :2182 replace `(a 2 MiB log may stall one frame)` with `(a 1 MiB log may stall one frame)`.

- [ ] **Step 4: HUMAN_CHECKS**

Append to the end of the perf section in `docs/HUMAN_CHECKS.md`:

```markdown
- [ ] **Terminal path, History replay (T15):**
  - In a pwsh pane run `1..60000 | % { "`e[32mline $_`e[0m" }` (a log of more than 1 MB), exit the session, open History and select it: the "Older output is left out" note shows, the replay opens at once, the last lines are `line 60000`, green, and the first visible line after scrolling to the top is a whole `line N` (no stray `[32m`, no `?` box at the start of the first line).
  - A short session (a few KB) replays completely with no note. A full-screen TUI session (claude or omp) shows its last screen without garbage at the top.
```

- [ ] **Step 5: Verify, tick and commit**

Run: `cargo test -p mkrew-core --lib -j 4 -- log_tail`, `cargo test -p mkrew-ui --lib -j 4 -- history` and `cargo test -p mkrew-cli -j 4 -- web` (expect PASS), then `sh scripts/verify.sh` (fallback as in Task 13). This is the last task of item 14: in `docs/INBOX.md` change line 138 `- [~] **14. Perf: terminal path (daemon scan, scrollback, replay)**` to `- [x] **14. Perf: terminal path (daemon scan, scrollback, replay)**` (the `→ plan: …Task 13, 14, 15` suffix stays). Do not commit INBOX.

```
git add crates/mkrew-core/src/log_tail.rs crates/mkrew-ui/src/history.rs crates/mkrew-cli/src/web/relay.rs docs/SPEC.md docs/decisions/0057-terminal-path-limits.md docs/HUMAN_CHECKS.md
git commit -m "perf(ui): History replays the newest 1 MiB of a session log from a safe boundary"
```
(`SPEC.md` and `HUMAN_CHECKS.md` are shared: `git add -p` if other tasks' hunks are uncommitted.)

---

### Task 16: A working agent shows a still dot: no pulse, no stepping dots (perf A1 + N1, ADR 0058)

> **Refresh 2026-10-13:** NEW task, written from the 2026-10-13 rescan. **A proposal that needs the user's OK before anyone executes it:** on 2026-10-10 he ticked `Giữ` for A1 (`docs/PERF_REVIEW.md:27`; `docs/INBOX.md:116` says "A1 (working dots) stays as is"). The 12 fps pulse (N1) was added after that tick (ADR 0054, tab dot and sidebar workspace pill) and costs 12 whole-window renders per second for as long as a working agent's tab or workspace row is on screen, about 24× the 500 ms dots. If he keeps `Giữ`, drop this task; the rest of the plan does not depend on it.
> - The pulse is removed outright (`status_dot`'s `pulse` / `id` params, `with_animation`, `PULSE_*`).
> - The working mark everywhere (pane pill, cards, inspector, peek, send dialog, sidebar row) becomes one still dot in the working color; the elapsed text next to it (`working · 2m`) is the "alive" signal.
> - `status::redraw_due` treats Working like Idle: it redraws only when the elapsed label changes (each second for the first minute, then each minute).
> - The "Starting agent" button and the voice "Transcribing" marker keep their stepping dots (they last seconds); the helper is renamed `loading_dots` because Working no longer uses it. See Open questions.
> - **User decision 2026-10-14 (ticked `Làm` with a note):** instead of motion, every status gets its own clearly distinct still dot color, so states are told apart at a glance. Use the existing per-state tokens (`sidebar::status_color`: `status_working`, `status_needs_input`, `status_done`, `status_exited`; dark and light in `theme.rs`) for every still dot this task draws, filled, not hollow, for Working. Before Step 3, check where a state is drawn hollow or in a neutral color (Idle is a hollow ring today, `status_dot(.., hollow)`); if Idle has no own token, add `status_idle` to `Theme` (dark + light, muted, distinguishable from the other four) and use it. A still colored dot has no per-frame cost (no animation, no timer), so this does not change the perf result. Add the color check to Step 7's HUMAN_CHECKS bullet (each state's dot is distinct in dark and light).

Inbox: none yet (proposed 2026-10-13; the user adds a line if accepted). Spec: §10 (SPEC.md:726-727 workspace pill, :795-797 tab dot), §11 (:910-916 status table, :942), §14 principle 5 (:1238-1240), §20G voice (:1947), §15 idle CPU (:1354); ADR 0054 (:30-33 says working dots pulse), ADR 0027 (:61). New ADR 0058.

**Current state (investigated):**
- Pulse: `status_dot(id, color, hollow, pulse)` (`shell/widgets.rs:718-746`) wraps the dot in `with_animation(Animation::new(theme::PULSE_PERIOD).repeat_synced().with_easing(pulsating_between(..)).with_max_fps(theme::PULSE_FPS), ..)` when `pulse`. Tokens: `theme.rs:916-918` (`PULSE_PERIOD`, `PULSE_MIN_OPACITY`) and `:922-923` (`PULSE_FPS = 12.0`, whose comment says "each redraw renders the window"). Imports: `widgets.rs:4` `use gpui::{Animation, AnimationExt, pulsating_between};` (no other use in the crate).
- `pulse = true` callers: the tab dot (`Shell::tab_dot`, `shell/pane_render.rs:785-810`, the `SessionState::Working if urgent.kind == SessionKind::Agent` arm, used at `:122-129`) and the sidebar workspace pill (`shell/sidebar.rs:885-890`). `pulse = false` callers: `shell/sidebar.rs:123`, `shell/run.rs:339`, `:413-418`, `shell/phone_access.rs:156`.
- Stepping dots: `status::WORKING_DOTS` / `status::working_dot(now)` (`status.rs:13-29`, one lit per `theme::STATUS_TICK` = 500 ms, `theme.rs:67`) and `widgets::working_dots(now, color)` (`shell/widgets.rs:698-716`). Users: `status_mark` Working branch (`widgets.rs:681-696`; callers `cards.rs:330`, `:733`, `inspector.rs:391`, `pane_render.rs:934` via `status_pill`, `peek.rs:525`, `send_dialog.rs:362`), `mark_slot` (`shell/sidebar.rs:1242-1272`, one dot blinking on `working_dot(now) == 0`; caller `:1234`), `starting_button` (`pane_render.rs:838-860`; callers `chrome.rs:450`, `pane_render.rs:311`), `voice_status` Transcribing (`sidebar.rs:845`).
- Redraw: the status loop (`shell/connection.rs:48-112`) wakes every 500 ms and notifies the root when `status::redraw_due(sessions, last, now)` (`status.rs:108-117`: `Working => true`, `Idle =>` label changed). `due` also includes `starting_pane.is_some()` and `transcribing` (`connection.rs:100-101`); those stay.
- Elapsed labels of a Working session: `status::time(state, changed_at, now)` (`status.rs:50-53`) returns `format_duration(now - changed_at)` for Working and Idle alike. It is shown by `status::text` (`working · 2m`: pane pill `pane_render.rs`, cards `cards.rs:331`, inspector `inspector.rs:71`, peek `peek.rs:544`, send dialog `send_dialog.rs:388`, fan-out `fanout.rs:86`) and by the sidebar row's time (`sidebar.rs:261`). Hover ages are computed when the tooltip opens. `format_duration` changes each second below 60 s, then each minute: the same cadence Idle already uses.
- `SessionState::Working`'s glyph is `"•••"` (`sidebar.rs:40`), now only reached through `status::icon` in places that skip Working (`sidebar.rs:1104` `!working`, `mark_slot`, `status_mark`), so it only documents the SPEC table. The phone web UI has its own mark (`mkrew-cli/web/model.mjs:5`); not touched.
- SPEC already rejects a pulse for the Run pill (`SPEC.md:879`; `run.rs:336` comment).

**Files:**
- Modify: `crates/mkrew-ui/src/status.rs` (`redraw_due`, rename `WORKING_DOTS`/`working_dot` → `LOADING_DOTS`/`loading_dot`, docs, tests)
- Modify: `crates/mkrew-ui/src/sidebar.rs` (`status_display` Working glyph)
- Modify: `crates/mkrew-ui/src/theme.rs` (delete `PULSE_*`; token docs)
- Modify: `crates/mkrew-ui/src/shell/widgets.rs` (`status_mark`, `loading_dots`, `status_dot`, import)
- Modify: `crates/mkrew-ui/src/shell/pane_render.rs`, `shell/sidebar.rs`, `shell/run.rs`, `shell/phone_access.rs`, `shell/cards.rs`, `shell/inspector.rs`, `shell/peek.rs`, `shell/send_dialog.rs`, `shell/connection.rs` (comment), `board.rs` (comment)
- Modify: `docs/SPEC.md`, `docs/HUMAN_CHECKS.md`, `docs/decisions/0054-simplified-terminals-screen.md`, `docs/decisions/0027-calm-redesign.md`
- Create: `docs/decisions/0058-static-working-marks.md`

**Interfaces:**
- Changes: `widgets::status_dot(color: u32, hollow: bool) -> gpui::Div` (no `id`, no `pulse`).
- Changes: `widgets::status_mark(state, theme)` (no `now`); `widgets::loading_dots(now, color)` (renamed from `working_dots`); `status::loading_dot`, `status::LOADING_DOTS` (renamed); `Shell::tab_dot` returns `Option<(u32, bool)>`; `pane_render::status_pill(session, pill, theme)` (no `now`); `sidebar::mark_slot(mark, theme)` (no `now`).
- Removes: `theme::PULSE_PERIOD`, `PULSE_MIN_OPACITY`, `PULSE_FPS`.
- Behavior: `status::redraw_due` is true only when a Working or Idle session's `status::time` label differs between `last_ms` and `now_ms`.

- [ ] **Step 1: Failing test**

In `status.rs` `mod tests`, replace `redraws_only_when_the_shown_status_changes` with:

```rust
    #[test]
    fn redraws_only_when_the_shown_status_changes() {
        let tick = theme::STATUS_TICK.as_millis() as i64;
        let still = [
            session(SessionState::Done, 0),
            session(SessionState::NeedsInput, 0),
            session(SessionState::Exited, 0),
        ];
        assert!(!redraw_due(&still, 0, 600 * SEC));

        // Working and idle show a time: each second under a minute, then only when the
        // minute changes. Nothing else moves while an agent works (ADR 0058).
        for state in [SessionState::Working, SessionState::Idle] {
            let sessions = [session(state, 0)];
            assert!(redraw_due(&sessions, 4 * SEC + 600, 5 * SEC + 100), "{state:?}");
            assert!(!redraw_due(&sessions, 5 * SEC + 100, 5 * SEC + 600), "{state:?}");
            assert!(!redraw_due(&sessions, 90 * SEC, 90 * SEC + tick), "{state:?}");
            assert!(redraw_due(&sessions, 119 * SEC + 800, 120 * SEC + 300), "{state:?}");
        }
    }
```

Rename the existing test `working_dots_take_turns_one_per_tick` to `loading_dots_take_turns_one_per_tick`, and its calls `working_dot(..)` to `loading_dot(..)` (same assertions).

Run: `cargo test -p mkrew-ui --lib status::tests -j 4`
Expected: FAIL (`loading_dot` does not exist; the Working case of `redraw_due` is still `true` every tick).

- [ ] **Step 2: `status.rs` and the glyph**

`status.rs` lines 1-3 module doc: replace `working dots, elapsed/idle time` with `working/idle time`. Replace lines 13-14 and 24-34 with:

```rust
/// Dots of the loading mark (a pane being started, a voice transcription): one is lit per
/// [`theme::STATUS_TICK`]. Working sessions show a still dot instead (ADR 0058).
pub const LOADING_DOTS: usize = 3;
```
```rust
/// The lit dot of the loading mark at `now_ms`: steps on the status tick.
pub fn loading_dot(now_ms: i64) -> usize {
    let step = theme::STATUS_TICK.as_millis().max(1) as i64;
    (now_ms.max(0) / step) as usize % LOADING_DOTS
}

/// The state's glyph; `working` is a still dot.
pub fn icon(state: SessionState) -> &'static str {
    status_display(state).icon
}
```
Replace `redraw_due` (lines 105-117) with:

```rust
/// Whether the status display changed between `last_ms` and `now_ms` (checked every
/// [`theme::STATUS_TICK`]): a working or idle session's time text changes once per second,
/// then once per minute. Nothing animates (ADR 0058).
pub fn redraw_due(sessions: &[Session], last_ms: i64, now_ms: i64) -> bool {
    sessions.iter().any(|s| {
        time(s.state, s.state_changed_at, last_ms) != time(s.state, s.state_changed_at, now_ms)
    })
}
```
`sidebar.rs:40`: `SessionState::Working => ("•••", "working"),` → `SessionState::Working => ("●", "working"),`.

- [ ] **Step 3: Widgets**

In `shell/widgets.rs` delete line 4 (`use gpui::{Animation, AnimationExt, pulsating_between};`). Replace `status_mark`, `working_dots` and `status_dot` (lines 679-746) with:

```rust
/// A session's status mark (SPEC §11): one still dot while working, else the glyph. Fixed
/// width, so rows with different states line up.
pub(crate) fn status_mark(state: SessionState, theme: &Theme) -> gpui::AnyElement {
    let color = sidebar::status_color(theme, state);
    let mark = div()
        .flex_none()
        .w(theme::rems_px(theme::STATUS_MARK_WIDTH))
        .flex()
        .items_center()
        .justify_center();
    if state != SessionState::Working {
        return mark
            .text_color(rgb(color))
            .child(status::icon(state))
            .into_any_element();
    }
    mark.child(status_dot(color, false)).into_any_element()
}

/// The loading mark: three dots taking turns, lit per [`theme::STATUS_TICK`] (a pane being
/// started, a voice transcription: both last seconds, ADR 0058).
pub(crate) fn loading_dots(now: i64, color: gpui::Rgba) -> gpui::Div {
    let lit = status::loading_dot(now);
    div()
        .flex()
        .items_center()
        .gap(theme::rems_px(theme::STATUS_DOT_GAP))
        .children((0..status::LOADING_DOTS).map(move |i| {
            div()
                .size(theme::rems_px(theme::STATUS_DOT))
                .rounded_full()
                .bg(color)
                .opacity(if i == lit {
                    opacity::FULL
                } else {
                    opacity::STATUS_DOT_DIM
                })
        }))
}

/// A round status dot: filled in `color`, or `hollow` (a ring: idle, stopped). Always still: a
/// looping animation renders the whole window for as long as it is on screen (ADR 0058).
pub(crate) fn status_dot(color: u32, hollow: bool) -> gpui::Div {
    div()
        .flex_none()
        .size(theme::rems_px(theme::STATUS_DOT_LARGE))
        .rounded_full()
        .when(hollow, |el| {
            el.border(px(theme::BORDER_WIDTH)).border_color(rgb(color))
        })
        .when(!hollow, |el| el.bg(rgb(color)))
}
```

- [ ] **Step 4: Callers**

`shell/pane_render.rs`:
- `:122-129` becomes
  ```rust
                    .children(dot.map(|(color, hollow)| status_dot(color, hollow)))
  ```
- `tab_dot` (`:782-810`): doc line becomes `/// A tab's status dot from its most urgent pane: amber needs input, red exited, accent while an agent works, green for done or a live run process, else a hollow ring (idle, starting). `(color, hollow)`; `None` for an empty tab.`; return type `Option<(u32, bool)>`; the arms become
  ```rust
        Some(match urgent.state {
            SessionState::NeedsInput => (theme.status_needs_input, false),
            SessionState::Exited => (theme.status_exited, false),
            SessionState::Working if urgent.kind == SessionKind::Agent => {
                (theme.status_working, false)
            }
            _ if run_live => (theme.status_done, false),
            SessionState::Done => (theme.status_done, false),
            _ => (theme.status_idle, true),
        })
  ```
- `starting_button` (`:837-860`): `working_dots(now, rgb(color))` → `loading_dots(now, rgb(color))`; doc: `with the loading dots`.
- `status_pill` (`:908-936`): drop the `now: i64` parameter and the call becomes `.child(status_mark(state, theme))`; its caller `:551` becomes `status_pill(info, pill, theme)`.

`shell/sidebar.rs`:
- `:123` → `.child(status_dot(color, hollow))` (if `id` is then unused in that function, drop it; rustc says).
- `:845` → `loading_dots(status::now_ms(), rgb(theme.text_muted))`.
- `:885-890` → `.child(status_dot(theme.status_working, false))` (drop the `SharedString::from(format!("workspace-working-{project_id}"))` id). Doc of `workspace_row` (`:857-859`): `(pulsing dot)` → `(still dot)`.
- `mark_slot` (`:1239-1272`) becomes
  ```rust
/// A sidebar row's state mark in a narrow fixed slot (empty without a mark), so agent chips
/// stay in one column without pushing the rows in: one still dot while working, else the
/// state glyph.
fn mark_slot(mark: Option<SessionState>, theme: &Theme) -> gpui::AnyElement {
    let slot = div()
        .flex_none()
        .w(theme::rems_px(theme::SIDEBAR_MARK_WIDTH))
        .flex()
        .items_center()
        .justify_center();
    let Some(state) = mark else {
        return slot.into_any_element();
    };
    let color = rgb(sidebar::status_color(theme, state));
    if state != SessionState::Working {
        return slot
            .text_size(font_size::META)
            .text_color(color)
            .child(status::icon(state))
            .into_any_element();
    }
    slot.child(
        div()
            .size(theme::rems_px(theme::STATUS_DOT))
            .rounded_full()
            .bg(color),
    )
    .into_any_element()
}
  ```
  and its caller `:1234` → `mark_slot(row.mark(), theme)` (`now` is still used by `session_tip` in `compact_session_row`).

`shell/run.rs`: `:339` → `status_dot(dot, false)`; `:413-418` → `.child(status_dot(color, hollow))`. `shell/phone_access.rs:156` → `status_dot(theme.status_done, false)`. `shell/cards.rs:330` → `status_mark(session.state, theme)`, `:733` → `status_mark(session.state, theme)`; `inspector.rs:391`, `peek.rs:525`, `send_dialog.rs:362` the same (`choice.state`). If a `now` parameter or binding is then unused in one of those functions (the inspector's session card is the likely one), remove it up the call chain; `-D warnings` shows each.

Comments: `shell/connection.rs:48` `// Spinner and elapsed/idle times move on their own (§11).` → `// Elapsed/idle times and the loading dots move on their own (§11).`; `board.rs:511` `(session spinners are covered by [`status::redraw_due`])` → `(session times are covered by [`status::redraw_due`])`.

- [ ] **Step 5: Tokens**

`theme.rs`: delete `PULSE_PERIOD`, `PULSE_MIN_OPACITY` (lines 916-918 with their doc line) and `PULSE_FPS` (922-923 with its doc line); the `READING_WIDTH` doc and constant (919-921) stay. Doc edits: `:75` → `/// Diameter of one dot of the loading mark and of the sidebar's still working dot (SPEC §11).`; `:77` → `/// Gap between the dots of the loading mark.`; `:82` → `/// The sidebar's narrower mark slot: one still dot while working, else the glyph.`; `:139` → `/// The unlit dots of the loading mark.`

Compile check: `cargo check -p mkrew-ui --all-targets -j 4` Expected: no errors, no unused `now` / `SharedString` / `Duration` warnings (fix any it names). Run: `cargo test -p mkrew-ui --lib status::tests -j 4` Expected: PASS.

- [ ] **Step 6: ADR 0058, SPEC, older ADR notes**

Create `docs/decisions/0058-static-working-marks.md`:

```markdown
# 0058 — A working agent shows a still dot

- Date: 2026-10-13
- Status: proposed (accepted when the user OKs it; he ticked `Giữ` for perf A1 on 2026-10-10)
- Milestone: perf review (A1, N1)

## Context

ADR 0027 replaced the pulsing dot with three dots stepping on the 500 ms status tick, and ADR
0054 added a 12 fps pulse to the tab dot and the sidebar workspace pill. Every animation frame
renders the whole window (`theme.rs` said so itself). While any agent works, the UI rendered 2
times a second for the dots and 12 times a second for the pulse, all day, against SPEC §15's
idle CPU budget. The run pill already has a still dot for this reason (ADR 0018).

## Options

1. Keep both (today). Most lively; 14 renders per second.
2. Drop the pulse, keep the stepping dots. 2 renders per second.
3. A still dot in the working color; only the elapsed text moves, once per second for the first
   minute, then once per minute (as for idle time). Nothing animates.

## Decision

Option 3. `status_dot` has no pulse; the `working` mark, the sidebar slot and the tab dot are
still dots. `status::redraw_due` redraws when a working or idle session's time text changes. The
loading marks (a pane being started, a voice transcription) keep three stepping dots: they last
seconds and the status tick already runs for them.

## Consequences

- SPEC §10, §11, §14 and §20G say so; ADR 0054 and ADR 0027 point here.
- A still dot says "working" less loudly than motion; the `working · 12s` text next to it is the
  live signal (never color alone, §11).
- gpui's missing reduce-motion query no longer matters for these marks.
- Reverting is local: `status_dot` and `status_mark`.
```

`docs/SPEC.md` (replace the fragment on each line):
- `:727` `` `● 1 working` pill (pulsing dot) `` → `` `● 1 working` pill (a still dot) ``
- `:796` `pulsing accent while an` → `accent while an`
- `:913` the whole row → `| working | ● a still dot in the working color (no animation, ADR 0058) | elapsed time |`
- `:942` `(a pill, pulsing dot)` → `(a pill, still dot)`
- `:1239-1240` `Small, slow working dots.` → `Nothing loops while an agent works: its mark is a still dot and its time text moves (ADR 0058).`
- `:1947` `the `working`\n   dots and `Transcribing`` → `the loading\n   dots and `Transcribing``

`docs/decisions/0054-simplified-terminals-screen.md:30-33`: replace `Working dots (sidebar pill, tab dot) do pulse\n  (`with_animation`, which gpui draws still under reduce-motion); an agent that works prints\n  output anyway.` with `Working dots (sidebar pill, tab dot) pulsed in this ADR; ADR 0058 made them still.`

`docs/decisions/0027-calm-redesign.md:61-62`: append to the bullet `(a GPUI animation redraws the whole window every frame); ADR 0058 later made the working mark a still dot too`.

- [ ] **Step 7: HUMAN_CHECKS**

Edit the old checks that describe motion: `docs/HUMAN_CHECKS.md:130-131` (`one blinking dot in a narrow slot` → `one still dot in a narrow slot`), `:751-753` (replace the whole `**Working dots:**` bullet with the new one below), `:789` (`its `••• N` count steps` → `its `● N working` pill shows the count`), `:748`, `:785`, `:1092` (`three dots` / `the three dots take turns` → `a still dot`), `:1984-1989` (T3.6: `every tick while a session works (4-frame spinner), for idle only when its time text changes` → `for a working or idle session only when its time text changes (each second, then each minute; ADR 0058)`; drop the sentence `gpui has no reduce-motion query, so the spinner always turns (slow enough to pass; revisit if gpui adds one).`). Then run Grep `three dots|••• |blinking dot|4-frame spinner` over `docs/HUMAN_CHECKS.md` and fix what is left in the same way (history sections that describe old behavior may stay).

Append to the perf `### To verify —` section (created by Task 1; if it is missing, insert it above the first `### To verify —` heading):

```markdown
- [ ] **Still working marks (A1, N1, ADR 0058):** start 2 agents that work for a minute+. In the sidebar row, the workspace `● 2 working` pill, the tab dot, the pane pill, the Inbox / board cards and the inspector, a working agent shows one steady dot in the working color (nothing pulses or steps) next to `working · 12s`; the text still counts seconds, then minutes. Idle `○`, needs input `?` amber, done `✓` look as before. Dark and light. A pane being started shows `Starting agent` with three stepping dots; a voice transcription shows the dots and `Transcribing`.
- [ ] **CPU (A1, N1):** with 2-3 agents `working` and the window visible, `mkrew-ui` CPU in Task Manager over one minute is lower than before this task (before: dots 2 renders/s plus the 12 fps pulse).
```

- [ ] **Step 8: Verify**

Run: `sh scripts/verify.sh` (omp shell; `bash` is WSL). Fallback if rustc crashes: `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace -j 4`. Expected: PASS.

- [ ] **Step 9: Commit**

```
git add crates/mkrew-ui/src/status.rs crates/mkrew-ui/src/sidebar.rs crates/mkrew-ui/src/theme.rs crates/mkrew-ui/src/board.rs crates/mkrew-ui/src/shell/widgets.rs crates/mkrew-ui/src/shell/pane_render.rs crates/mkrew-ui/src/shell/sidebar.rs crates/mkrew-ui/src/shell/run.rs crates/mkrew-ui/src/shell/phone_access.rs crates/mkrew-ui/src/shell/cards.rs crates/mkrew-ui/src/shell/inspector.rs crates/mkrew-ui/src/shell/peek.rs crates/mkrew-ui/src/shell/send_dialog.rs crates/mkrew-ui/src/shell/connection.rs docs/SPEC.md docs/HUMAN_CHECKS.md docs/decisions/0054-simplified-terminals-screen.md docs/decisions/0027-calm-redesign.md docs/decisions/0058-static-working-marks.md
git commit -m "perf(ui): working agents show a still dot, no pulse or stepping dots"
```
Stage only the hunks this task made in files other tasks also edit (`connection.rs`, `HUMAN_CHECKS.md`, `SPEC.md`); never `-A` / `.`. Do not push.

---

### Task 17: The phone poll renders only on a change, and Tailscale is read every 3 s (perf N2)

> **Refresh 2026-10-13:** NEW task from the rescan (finding N2); not in `docs/PERF_REVIEW.md`, so no earlier tick of the user to override.
> - The Phone access poll (ADR 0053, added after the 2026-10-10 plan) wakes the UI thread every second for the app's whole life and ends every poll with an unconditional `cx.notify()`: a full root render every 5 s forever, even with `[web] enabled = false` and nothing changed.
> - `Phone::polled` returns whether anything the title bar icon or the dialog shows changed; the shell notifies only then.
> - While the dialog is open the two `tailscale` processes run every 3 s instead of every 1 s (still every tick while a Start / Stop click waits for the daemon).

Inbox: none yet (proposed 2026-10-13; the user adds a line if accepted). Spec: §10 title bar phone icon, ADR 0053 (`docs/decisions/0053-phone-access-ui.md`), §15 idle CPU.

**Current state (investigated):**
- `shell/connection.rs:113-128`: a loop sleeps `theme::PHONE_TICK` (1 s, `theme.rs:72`) and calls `shell.poll_phone` when `shell.phone.due(Instant::now(), shell.phone_open)`. `Phone::due` (`phone_dialog.rs:277-283`): `open || last_poll.is_none_or(|last| now - last >= theme::PHONE_POLL_IDLE)` (5 s, `theme.rs:73`). Other callers of `poll_phone`: the dialog opening (`phone_access.rs:20`), after Start / Stop (`:98`), the first load (`connection.rs:169`).
- `Shell::poll_phone` (`shell/phone_access.rs:44-80`): `daemon.status` always; when the dialog is open also `Local::read` on the background executor (`phone_dialog.rs:194-208`: `tailscale::serve_status()` then `tailscale::dns_name()`: two process spawns) and the `web_token` file. The answer lands in `Phone::polled(web, local, now, colors)` (`phone_dialog.rs:297-341`) and `cx.notify()` follows unconditionally (`phone_access.rs:76`).
- What `polled` changes: `polling`, `copied` (the "Copy link" label says "Copied" until the next poll, `:218`), `pending` (Start / Stop wait), `daemon`, and with `local`: `serve`, `host`, `token`, `link`. What is visible: `running()` (title bar icon), `status_row`, `serve_row`, `link_block`, `link`, `pending()`, `copied`.
- `polled` has no return value; tests call it as a statement (`phone_dialog.rs` tests: `polled_on`, `:474-519`, `:542`).
- `Daemon`, `Serve`, `Setup` derive `PartialEq` (`phone_dialog.rs:19, 27, 38`); `WebStatus` derives `PartialEq`.

**Files:**
- Modify: `crates/mkrew-ui/src/phone_dialog.rs` (`polled`, `due`, tests)
- Modify: `crates/mkrew-ui/src/theme.rs` (`PHONE_POLL_OPEN`, docs)
- Modify: `crates/mkrew-ui/src/shell/phone_access.rs` (notify only on change)

**Interfaces:**
- Changes: `Phone::polled(&mut self, web, local, now, colors) -> bool` (true when anything visible changed).
- Produces: `theme::PHONE_POLL_OPEN: Duration = 3 s`.
- Behavior: `Phone::due(now, open)`: closed, every `PHONE_POLL_IDLE`; open, every `PHONE_POLL_OPEN`; open while a Start / Stop click waits (`pending` is set), on every tick. `PHONE_TICK` (the loop's wake) stays 1 s: waking without a poll renders nothing.

- [ ] **Step 1: Failing tests**

In `phone_dialog.rs` `mod tests` add (they use the existing `web`, `local`, `token`, `COLORS`):

```rust
    #[test]
    fn a_poll_that_changes_nothing_says_so() {
        let t0 = Instant::now();
        let mut phone = Phone::default();
        assert!(phone.polled(Ok(web(true, true, None)), None, t0, COLORS), "loading -> known");
        assert!(!phone.polled(Ok(web(true, true, None)), None, t0, COLORS));
        assert!(phone.polled(Ok(web(true, false, None)), None, t0, COLORS), "mk web stopped");
        assert!(phone.polled(Err("pipe closed".into()), None, t0, COLORS));
        assert!(
            !phone.polled(Err("pipe closed".into()), None, t0, COLORS),
            "the same failure again"
        );
    }

    #[test]
    fn the_dialogs_own_readings_count_only_when_they_differ() {
        let t0 = Instant::now();
        let mut phone = Phone::default();
        let on = |phone: &mut Phone, local: Local| {
            phone.polled(Ok(web(true, true, None)), Some(local), t0, COLORS)
        };
        let host = Some("pc.tail1.ts.net");
        assert!(on(&mut phone, local(host, Some(token("a1")))), "first answer");
        assert!(!on(&mut phone, local(host, Some(token("a1")))), "same host and token");
        assert!(on(&mut phone, local(host, Some(token("b2")))), "a new token");
        assert!(on(&mut phone, local(host, None)), "the token file is gone");
        assert!(on(&mut phone, local(None, None)), "Tailscale lost its name");
        let missing = Local {
            serve: Serve::Missing,
            host: host.map(str::to_owned),
            token: None,
        };
        assert!(on(&mut phone, missing), "serve state and host changed");
    }

    #[test]
    fn clearing_copied_and_ending_the_wait_are_visible_changes() {
        let t0 = Instant::now();
        let mut phone = Phone::default();
        let link = || Some(local(Some("pc.tail1.ts.net"), Some(token("a1"))));
        phone.polled(Ok(web(true, true, None)), link(), t0, COLORS);
        assert!(phone.copy_link().is_some());
        assert!(phone.polled(Ok(web(true, true, None)), link(), t0, COLORS), "Copied -> Copy link");
        assert!(!phone.polled(Ok(web(true, true, None)), link(), t0, COLORS));
        // A click waits for the daemon to agree; the wait ends by timeout with the daemon
        // unchanged, which is still something to show.
        phone.written(false, t0);
        let later = t0 + Duration::from_secs(1);
        assert!(!phone.polled(Ok(web(true, true, None)), None, later, COLORS), "still waiting");
        let timeout = t0 + PENDING_TIMEOUT;
        assert!(phone.polled(Ok(web(true, true, None)), None, timeout, COLORS));
        assert_eq!(phone.pending(), None);
    }

    #[test]
    fn polls_are_spaced_by_what_each_costs() {
        let t0 = Instant::now();
        let mut phone = Phone::default();
        assert!(phone.due(t0, false) && phone.due(t0, true), "never polled");
        assert!(phone.begin_poll(t0));
        let just_before = |every: Duration| t0 + every - Duration::from_millis(1);
        // Closed: the title bar icon only.
        assert!(!phone.due(just_before(theme::PHONE_POLL_IDLE), false));
        assert!(phone.due(t0 + theme::PHONE_POLL_IDLE, false));
        // Open: Tailscale is two process runs.
        assert!(!phone.due(just_before(theme::PHONE_POLL_OPEN), true));
        assert!(phone.due(t0 + theme::PHONE_POLL_OPEN, true));
        // Waiting for the daemon to agree with a Start / Stop click: every tick.
        phone.written(true, t0);
        assert!(phone.due(t0, true));
        assert!(!phone.due(t0, false), "closed: the tick is not faster");
    }
```

Run: `cargo test -p mkrew-ui --lib phone_dialog::tests -j 4`
Expected: FAIL to compile (`polled` returns `()`, `PHONE_POLL_OPEN` does not exist).

- [ ] **Step 2: `theme.rs`**

Replace `theme.rs:69-73` with:

```rust
/// The Phone access poll clock (ADR 0053): the loop wakes every [`PHONE_TICK`] and polls when
/// `Phone::due`: closed, `daemon.status` alone every [`PHONE_POLL_IDLE`] (it keeps the title
/// bar icon true); open, also Tailscale (two process runs) every [`PHONE_POLL_OPEN`], and on
/// every tick while a Start / Stop click waits for the daemon.
pub const PHONE_TICK: Duration = Duration::from_secs(1);
pub const PHONE_POLL_IDLE: Duration = Duration::from_secs(5);
pub const PHONE_POLL_OPEN: Duration = Duration::from_secs(3);
```

- [ ] **Step 3: `Phone::due` and `Phone::polled`**

`phone_dialog.rs`, replace `due` (`:277-283`):

```rust
    /// The loop's tick: how long since the last poll decides (see [`theme::PHONE_POLL_OPEN`]).
    pub fn due(&self, now: Instant, open: bool) -> bool {
        let every = match (open, self.pending) {
            (false, _) => theme::PHONE_POLL_IDLE,
            // A click waits for the daemon to agree: look at every tick.
            (true, Some(_)) => Duration::ZERO,
            (true, None) => theme::PHONE_POLL_OPEN,
        };
        self.last_poll
            .is_none_or(|last| now.duration_since(last) >= every)
    }
```

Replace `polled` (`:295-341`):

```rust
    /// A poll answered: `web` from `daemon.status`, `local` only when the dialog is open.
    /// `colors` = `#rrggbb` of `qr_dark` / `qr_light`. Returns whether anything the title bar
    /// icon or the dialog shows changed: the caller renders only then.
    pub fn polled(
        &mut self,
        web: Result<WebStatus, String>,
        local: Option<Local>,
        now: Instant,
        colors: (&str, &str),
    ) -> bool {
        self.polling = false;
        // "Copied" lasts until the next poll: clearing it is a visible change.
        let mut changed = std::mem::take(&mut self.copied);
        let daemon = match web {
            Ok(web) => {
                if let Some((wanted, since)) = self.pending
                    && (web.enabled == wanted || now.duration_since(since) >= PENDING_TIMEOUT)
                {
                    self.pending = None;
                    changed = true;
                }
                Daemon::Known(web)
            }
            Err(error) => Daemon::Failed(error),
        };
        changed |= self.daemon != daemon;
        self.daemon = daemon;
        let Some(local) = local else {
            return changed;
        };
        changed |= self.serve != local.serve
            || self.host != local.host
            || self.token != local.token;
        self.serve = local.serve;
        self.host = local.host;
        self.token = local.token;
        // Every poll lands here: render the QR code again only for another link (a changed
        // host or token is already counted above).
        let text = match (&self.host, &self.token) {
            (Some(host), Some(token)) => Some(phone::link(host, token)),
            _ => None,
        };
        if self.link.as_ref().map(|link| &link.text) != text.as_ref() {
            self.link = text.map(|text| {
                let image = phone::qr_svg(&text, colors.0, colors.1).map(|svg| {
                    Arc::new(gpui::Image::from_bytes(
                        gpui::ImageFormat::Svg,
                        svg.into_bytes(),
                    ))
                });
                Link { text, image }
            });
        }
        changed
    }
```

- [ ] **Step 4: Notify only on change**

`shell/phone_access.rs:68-77` becomes:

```rust
            let _ = this.update(cx, |shell, cx| {
                let (dark, light) = {
                    let theme = cx.theme();
                    (theme::hex(theme.qr_dark), theme::hex(theme.qr_light))
                };
                // A poll that found nothing new renders nothing (perf N2).
                let changed = shell
                    .phone
                    .polled(web, local, Instant::now(), (&dark, &light));
                if changed {
                    cx.notify();
                }
            });
```
and the doc of `poll_phone` (`:41-43`) gets the sentence `The window renders only when the answer changed something.`

Compile check: `cargo check -p mkrew-ui --all-targets -j 4` Expected: no errors. Run: `cargo test -p mkrew-ui --lib phone_dialog::tests -j 4` Expected: PASS (the older tests ignore the new return value).

- [ ] **Step 5: HUMAN_CHECKS**

Append to the perf `### To verify —` section:

```markdown
- [ ] **Phone access poll (N2, ADR 0053):** open "Phone access" with Tailscale running and `[web]` on: the rows and the QR are right as before. Quit Tailscale, wait: the Tailscale row turns to "Install and sign in to Tailscale first" within a few seconds (it was 1 s, now about 3 s); start it again, same. Press Start / Stop: the row still shows "Starting…" then `● On` within about a second of the daemon agreeing. Press Copy link: it reads "Copied" until the next poll (≤ 3 s), then "Copy link". With the dialog closed and `[web] enabled = false`, `mkrew-ui` CPU is flat at idle (no render every 5 s).
```

- [ ] **Step 6: Verify**

Run: `sh scripts/verify.sh` (fallback: `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace -j 4`). Expected: PASS.

- [ ] **Step 7: Commit**

```
git add crates/mkrew-ui/src/phone_dialog.rs crates/mkrew-ui/src/theme.rs crates/mkrew-ui/src/shell/phone_access.rs docs/HUMAN_CHECKS.md
git commit -m "perf(ui): the phone poll renders only on a change and reads Tailscale every 3 s"
```
Stage only this task's hunks in `theme.rs` and `HUMAN_CHECKS.md`. Do not push.

---

### Task 18: Mermaid draws only the current appearance and caches by block text (perf A5)

> **Refresh 2026-10-13:** NEW task (proposal for the A5 `Sửa` option). **Needs the user's OK:** on 2026-10-10 he ticked `Giữ` for A5 (`docs/PERF_REVIEW.md:55`), i.e. do nothing. The cost is still there after the Markdown work: `Shown::from_text` runs `diagram::render` for every mermaid block on every parse (Docs re-read on `doc.changed`; Files preview after each text change, one build in flight), and `draw` lays out and renders each block twice (light and dark), with no cache. If he keeps `Giữ`, drop this task.
> - Today both appearances are drawn into one `Diagram { light, dark, width, height }` and `Diagram::image(theme)` picks one at render time (`diagram.rs:17-31`, `preview.rs:413`). The pair exists only so a theme switch needs no redraw.
> - New: one drawing per block for `Shown`'s appearance, kept in a process-wide cache keyed by (hash of the block text, dark). Unchanged blocks are free on every re-parse; a new keystroke redraws only the edited block.
> - A theme switch (toggle, OS appearance, `settings.toml`) builds the shown Docs page and Files preview again, only when they contain a diagram; drawings seen before come from the cache. A page without diagrams is never rebuilt.
> - The Files preview build (Task 4 was dropped, nothing to wait for) and the virtual preview (42f3969) are unchanged; `FilesView::preview_request` gains the appearance.

Inbox: none yet (proposed 2026-10-13; the user adds a line if accepted). Spec: §12 (SPEC.md:968-970 mermaid), §12A (:1029-1040 Markdown preview), §14, ADR 0021 (`docs/decisions/0021-docs-preview-rendering.md:30-44`).

**Current state (investigated):**
- `diagram.rs:33-65`: `render(source)` → `catch_unwind(draw)`; `draw` parses once, then per appearance runs `compute_layout` + `render_svg` + `measure_svg_dimensions` and wraps both SVGs in `Arc<Image>`; width/height are the larger of the two. `Diagram::image(theme)` picks `theme.dark` (`theme.rs:561` doc "picks diagram images"). No cache.
- Callers: `docs::Shown::from_text` (`docs.rs:73-95`) inserts `diagram::render(text)` per mermaid block into `Shown.diagrams: HashMap<u32, Result<Diagram, String>>` (key = the block's first source line). `Shown::new(&DocContent)` (`:64-69`) wraps it. Built on the background executor by `Shell::read_doc` (`shell/docs.rs:115-153`, `Shown::new(&content)`) and `Shell::build_preview` (`shell/files.rs:181-208`, `Shown::from_text(&text)`), both off the UI thread. Also `tests/docs_view.rs:108`, `docs.rs:352-353` (test helper), `preview.rs:723-724`, `files_view/tests.rs:190-192`.
- Render: `preview::block` (`preview.rs:236-249`) draws `diagram_view(diagram, theme)` (`:401-420`, `img(diagram.image(theme))`) or the error with the source.
- Theme changes: `theme::refresh(mode, window, cx)` at `workspace_actions.rs:399-401` (`toggle_theme`), `connection.rs:28-31` (OS appearance) and `connection.rs:70-71` (settings reload). Nothing re-parses a page today.
- Files preview state: `FilesView::preview_request` / `preview_ready` / `preview_asked: Option<Stamp>` (`files_view.rs:160, 627-651`), `MdPreview { stamp, shown }` (`:110-113`); `build_preview` re-asks after every ready (`shell/files.rs:203`).
- `Docs` (`docs.rs:116-137`) has pub `selected: Option<String>` and `doc: Doc`; `Docs::read` replaces the page (`:208-221`).

**Files:**
- Modify: `crates/mkrew-ui/src/diagram.rs` (one appearance, cache, tests)
- Modify: `crates/mkrew-ui/src/docs.rs` (`Shown.dark`, `Shown::new/from_text(.., dark)`, `Shown::stale_diagrams`, `Docs::redraw_for`, tests)
- Modify: `crates/mkrew-ui/src/preview.rs` (`diagram_view`, test helper)
- Modify: `crates/mkrew-ui/src/files_view.rs` and `files_view/tests.rs` (`preview_request(dark)`, `preview_ready`)
- Modify: `crates/mkrew-ui/src/shell/docs.rs`, `shell/files.rs`, `shell/connection.rs`, `shell/workspace_actions.rs`
- Modify: `crates/mkrew-ui/tests/docs_view.rs`
- Modify: `docs/SPEC.md`, `docs/decisions/0021-docs-preview-rendering.md`, `docs/HUMAN_CHECKS.md`

**Interfaces:**
- Changes: `diagram::render(source: &str, dark: bool) -> Result<Diagram, String>`; `Diagram { image: Arc<Image>, width, height }` (no `light` / `dark`, no `image(theme)`).
- Changes: `docs::Shown::new(content: &DocContent, dark: bool)`, `Shown::from_text(text: &str, dark: bool)`; new field `Shown.dark: bool` ("the appearance its diagrams were drawn for"); `Shown::stale_diagrams(&self, dark) -> bool` (it has diagrams and was drawn for the other appearance).
- Produces: `Docs::redraw_for(&self, dark) -> Option<&str>` (the selected file when its page must be built again); `FilesView::preview_request(&mut self, dark: bool)`; `Shell::redraw_diagrams(&mut self, window, cx)`.
- Cache: `static CACHE` in `diagram.rs`, (hash of block text, dark) → `Result<Diagram, String>`, emptied when it holds 64 entries. Errors are cached too (a crash is not repeated on every re-parse).

- [ ] **Step 1: Failing tests**

`diagram.rs` `mod tests`: replace `flowcharts_draw_in_both_appearances_and_bad_sources_say_why` and adapt the pie test:

```rust
    const FLOW: &str = "flowchart LR\n  A[Plan] --> B{Review}\n  B -->|ok| C[Ship]\n";

    fn svg(image: &Arc<Image>) -> String {
        String::from_utf8(image.bytes.clone()).unwrap()
    }

    #[test]
    fn flowcharts_draw_in_the_asked_appearance_and_bad_sources_say_why() {
        let dark = render(FLOW, true).unwrap();
        let light = render(FLOW, false).unwrap();
        assert!(dark.width > 0.0 && dark.height > 0.0);
        assert!(svg(&dark.image).contains(&format!("#{:06X}", Theme::dark().surface)));
        assert!(svg(&light.image).contains(&format!("#{:06X}", Theme::light().surface)));
        assert_ne!(dark.image, light.image);
        let bad = render("flowchart LR\n  A -->", false).unwrap_err();
        assert!(bad.contains("A -->"), "{bad}");
    }

    #[test]
    fn a_block_is_drawn_once_per_appearance() {
        let first = render(FLOW, false).unwrap();
        let again = render(FLOW, false).unwrap();
        assert!(Arc::ptr_eq(&first.image, &again.image), "cached by text and appearance");
        let other = render(FLOW, true).unwrap();
        assert!(!Arc::ptr_eq(&first.image, &other.image));
        let edited = render(&format!("{FLOW}  C --> D\n"), false).unwrap();
        assert!(!Arc::ptr_eq(&first.image, &edited.image), "another text, another drawing");
        // A bad source is remembered too.
        assert_eq!(render("flowchart LR\nA[/]\n", false), render("flowchart LR\nA[/]\n", false));
    }
```
`renderer_crashes_become_a_reason` becomes `render("flowchart LR\nA[/]\n", false)`. In `pie_labels_read_on_every_slice_and_outside_in_both_appearances` replace the drawing part:

```rust
        // A 3 % slice has its label outside, on the label box.
        for (dark, theme) in [(true, Theme::dark()), (false, Theme::light())] {
            let svg = svg(&render("pie\n  \"A\": 95\n  \"B\": 3\n", dark).unwrap().image);
            let first = format!("#{:06X}", theme.workspace_colors[0]);
            assert!(svg.contains(&first), "slices in this appearance's colors");
            let label_box = format!("ry=\"2\" fill=\"#{:06X}\" stroke=\"none\"", theme.text);
            assert!(svg.contains(&label_box), "outside label box");
        }
```

`docs.rs` `mod tests`: change the helper to `Shown::new(&content(path, text, hash), false)`, update `mermaid_blocks_are_drawn_and_other_code_is_not` as is (it uses the helper), and add:

```rust
    #[test]
    fn a_page_with_diagrams_is_built_again_for_the_other_appearance() {
        let doc = "```mermaid\ngraph TD\n  A --> B\n```\n";
        let mut docs = Docs::new(Ulid::new());
        docs.selected = Some("a.md".into());
        docs.read("a.md", Ok(Shown::new(&content("a.md", doc, "h"), false)));
        assert_eq!(docs.redraw_for(false), None);
        assert_eq!(docs.redraw_for(true), Some("a.md"));
        docs.read("a.md", Ok(Shown::new(&content("a.md", doc, "h"), true)));
        assert_eq!(docs.redraw_for(true), None);
        // Nothing drawn, nothing to draw again.
        docs.read("a.md", Ok(Shown::new(&content("a.md", "# T\n", "h2"), false)));
        assert_eq!(docs.redraw_for(true), None);
    }
```

`files_view/tests.rs`: `page(text)` becomes `docs::Shown::from_text(text, false)`; every `view.preview_request()` becomes `view.preview_request(false)`. Add:

```rust
#[test]
fn a_preview_with_diagrams_is_built_again_for_the_other_appearance() {
    let mut view = opened("```mermaid\ngraph TD\n  A --> B\n```\n");
    let (stamp, text) = view.preview_request(false).unwrap();
    view.preview_ready(stamp, docs::Shown::from_text(&text, false));
    assert_eq!(view.preview_request(false), None, "current");
    let (again, same_text) = view.preview_request(true).expect("drawn for light, shown dark");
    assert_eq!((again, &same_text), (stamp, &text));
    assert_eq!(view.preview_request(true), None, "asked once while it is built");
    view.preview_ready(again, docs::Shown::from_text(&same_text, true));
    assert_eq!(view.preview_request(true), None);

    // A page without a diagram looks the same in both appearances.
    let mut plain = opened("a\n");
    let (stamp, text) = plain.preview_request(false).unwrap();
    plain.preview_ready(stamp, docs::Shown::from_text(&text, false));
    assert_eq!(plain.preview_request(true), None);
}
```

Run: `cargo test -p mkrew-ui --lib diagram::tests docs::tests files_view -j 4`
Expected: FAIL to compile (new signatures).

- [ ] **Step 2: `diagram.rs`**

Replace the module doc (lines 1-4), imports, `Diagram`, `render` and `draw` (lines 1-65) with:

```rust
//! Mermaid diagrams of the Docs preview (SPEC §12, ADR 0021): ```mermaid blocks drawn to SVG
//! by `mermaid-rs-renderer` (pure Rust, no browser) in the theme's colors for one appearance,
//! shown with `gpui::img`. Drawings are cached by (block text, appearance), so a re-parse
//! redraws only the blocks that changed. Slow the first time (it loads the system fonts):
//! run it off the UI thread.

use std::collections::HashMap;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::panic::AssertUnwindSafe;
use std::sync::{Arc, LazyLock, Mutex, PoisonError};

use gpui::{Image, ImageFormat};
use mermaid_rs_renderer::ir::DiagramKind;
use mermaid_rs_renderer::{
    LayoutConfig, compute_layout, measure_svg_dimensions, parse_mermaid_strict, render_svg,
};

use crate::theme::{self, Theme, hex};

#[derive(Debug, Clone, PartialEq)]
pub struct Diagram {
    pub image: Arc<Image>,
    /// Natural size, in px.
    pub width: f32,
    pub height: f32,
}

/// Most drawings kept; the cache is emptied when it is full.
// ponytail: clear-all at 64 entries (an SVG is tens of KB); an LRU if redrawing after a clear
// ever shows in a profile.
const CACHE_MAX: usize = 64;

type Key = (u64, bool);

static CACHE: LazyLock<Mutex<HashMap<Key, Result<Diagram, String>>>> = LazyLock::new(Mutex::default);

/// Draws `source` for the dark or the light appearance; `Err` says why it cannot be drawn
/// (shown above its source). Asked again for the same text and appearance, it is the cached
/// drawing.
pub fn render(source: &str, dark: bool) -> Result<Diagram, String> {
    let mut hasher = DefaultHasher::new();
    source.hash(&mut hasher);
    let key = (hasher.finish(), dark);
    if let Some(drawn) = lock().get(&key) {
        return drawn.clone();
    }
    // The renderer panics on some half-typed sources (e.g. `A[/]`); this runs on a gpui
    // background thread, where a panic aborts the whole UI. Not drawn under the lock: the
    // first drawing loads the system fonts and takes seconds.
    let drawn = std::panic::catch_unwind(AssertUnwindSafe(|| draw(source, dark)))
        .unwrap_or_else(|_| Err("the diagram renderer crashed on this source".to_owned()));
    let mut cache = lock();
    if cache.len() >= CACHE_MAX {
        cache.clear();
    }
    cache.insert(key, drawn.clone());
    drawn
}

fn lock() -> std::sync::MutexGuard<'static, HashMap<Key, Result<Diagram, String>>> {
    CACHE.lock().unwrap_or_else(PoisonError::into_inner)
}

fn draw(source: &str, dark: bool) -> Result<Diagram, String> {
    let parsed = parse_mermaid_strict(source).map_err(|error| error.to_string())?;
    let config = LayoutConfig::default();
    // Some layouts (pie, journey, mindmap) bake the theme's colors in.
    let theme = if dark { Theme::dark() } else { Theme::light() };
    let colors = colors(&theme, parsed.graph.kind);
    let layout = compute_layout(&parsed.graph, &colors, &config);
    let size = measure_svg_dimensions(&layout, &config, None);
    let svg = render_svg(&layout, &colors, &config);
    Ok(Diagram {
        image: Arc::new(Image::from_bytes(ImageFormat::Svg, svg.into_bytes())),
        width: size.width,
        height: size.height,
    })
}
```
(`colors` and `token_color` below stay.) In `theme.rs:561` change the doc `/// Dark appearance (picks diagram images, ADR 0021).` to `/// Dark appearance (the diagrams are drawn for it, ADR 0021).`

- [ ] **Step 3: `docs.rs` and the callers of `Shown`**

`docs.rs`: `use crate::diagram::{self, Diagram};` stays. In `Shown` add after `diagrams`:

```rust
    /// The appearance the diagrams were drawn for (their colors differ).
    pub dark: bool,
```
Replace `Shown::new` / `from_text` (`:63-96`) with:

```rust
impl Shown {
    pub fn new(content: &DocContent, dark: bool) -> Shown {
        Shown {
            hash: content.hash.clone(),
            ..Shown::from_text(&content.content, dark)
        }
    }

    /// A text parsed for the preview, with `hash` empty; its diagrams are drawn for `dark`
    /// (cached per block text, [`diagram::render`]). The Files view parses its editor's
    /// text, which is not a `DocContent`.
    pub fn from_text(text: &str, dark: bool) -> Shown {
        let blocks = markdown::parse(text);
        let mut code = HashMap::new();
        let mut diagrams = HashMap::new();
        markdown::walk(&blocks, &mut |block| {
            if let BlockKind::Code { lang, text } = &block.kind {
                code.insert(block.lines.start, highlight::fenced(lang.as_deref(), text));
                if lang
                    .as_deref()
                    .is_some_and(|lang| lang.eq_ignore_ascii_case("mermaid"))
                {
                    diagrams.insert(block.lines.start, diagram::render(text, dark));
                }
            }
        });
        Shown {
            hash: String::new(),
            lines: source_lines(text),
            blocks,
            code,
            diagrams,
            dark,
        }
    }

    /// Whether the page has diagrams drawn for the other appearance: it is built again when
    /// the theme switches. A page without diagrams looks the same in both.
    pub fn stale_diagrams(&self, dark: bool) -> bool {
        self.dark != dark && !self.diagrams.is_empty()
    }
}
```
In `impl Docs`, after `read`:

```rust
    /// The selected file when its page has to be built again for `dark` (the theme switched).
    pub fn redraw_for(&self, dark: bool) -> Option<&str> {
        match (&self.doc, &self.selected) {
            (Doc::Shown(shown), Some(path)) if shown.stale_diagrams(dark) => Some(path),
            _ => None,
        }
    }
```
Callers: `shell/docs.rs:136-139` → add `let dark = cx.theme().dark;` before `let background = ...` and `Shown::new(&content, dark)`; `tests/docs_view.rs:108` → `Shown::new(&content, false)`; `preview.rs:724` → `Rc::new(Shown::from_text(text, false))`.

`preview.rs` `diagram_view`: `img(diagram.image(theme))` → `img(Arc::clone(&diagram.image))` (add `use std::sync::Arc;` if missing; `theme` is still used for the card colors).

- [ ] **Step 4: `files_view.rs`**

`preview_asked: Option<Stamp>` (`:160`) → `preview_asked: Option<(Stamp, bool)>`, with the doc `/// The stamp and appearance a build was last asked for.` Replace `preview_request` and `preview_ready` (`:624-651`) with:

```rust
    /// The text to parse for the preview, once per text and appearance: `None` when no preview
    /// is shown, the page is current, or one is being built for this text. A page whose
    /// diagrams were drawn for the other appearance is built again (`dark` = the theme now).
    /// The shell parses it off the UI thread and answers with [`Self::preview_ready`].
    pub fn preview_request(&mut self, dark: bool) -> Option<(Stamp, String)> {
        if !self.previewing() {
            return None;
        }
        let stamp = self.stamp()?;
        let current = (self.preview.as_ref()).is_some_and(|preview| {
            preview.stamp == stamp && !preview.shown.stale_diagrams(dark)
        });
        if current || self.preview_asked == Some((stamp, dark)) {
            return None;
        }
        self.preview_asked = Some((stamp, dark));
        Some((stamp, self.shown()?.editor.text()))
    }

    /// The page built for `stamp`. Dropped when another text replaced the one it was built
    /// from, or when a newer page is already here (builds can finish out of order). The same
    /// text drawn for the other appearance replaces the page.
    pub fn preview_ready(&mut self, stamp: Stamp, shown: docs::Shown) {
        let newer = (self.preview.as_ref()).is_some_and(|preview| {
            preview.stamp > stamp || (preview.stamp == stamp && preview.shown.dark == shown.dark)
        });
        if stamp.0 != self.text_id || newer {
            return;
        }
        self.preview = Some(MdPreview {
            stamp,
            shown: Rc::new(shown),
        });
        // The build for this text came back; a build for a newer edit may still run.
        if self.preview_asked.is_some_and(|(asked, _)| asked == stamp) {
            self.preview_asked = None;
        }
    }
```
`files_view.rs:361-362` (`self.preview_asked = None;`) stays.

- [ ] **Step 5: `shell/files.rs`, `redraw_diagrams` and the three theme sites**

`Shell::build_preview` (`shell/files.rs:181-208`): at the top `let dark = cx.theme().dark;`, `view.preview_request(dark)`, and `.spawn(async move { Shown::from_text(&text, dark) })`.

In `shell/docs.rs`, after `read_doc`, add:

```rust
    /// The appearance changed (the toggle, the OS, `settings.toml`): the shown Docs page and
    /// Files preview that contain Mermaid diagrams are built again for it. Drawings made
    /// before come from the cache; pages without diagrams are left alone.
    pub(crate) fn redraw_diagrams(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        let dark = cx.theme().dark;
        let reads: Vec<(Ulid, String)> = self
            .docs
            .iter()
            .filter_map(|(project_id, docs)| {
                docs.redraw_for(dark).map(|path| (*project_id, path.to_owned()))
            })
            .collect();
        for (project_id, path) in reads {
            self.read_doc(project_id, path, window, cx);
        }
        let views: Vec<Ulid> = self.files.keys().copied().collect();
        for project_id in views {
            self.build_preview(project_id, window, cx);
        }
    }
```
Call it right after `theme::refresh`:
- `workspace_actions.rs:399-401`: `theme::refresh(mode, window, cx); self.redraw_diagrams(window, cx); cx.notify();`
- `connection.rs:28-31`: `cx.observe_window_appearance(window, |shell, window, cx| { theme::refresh(cx.theme().mode, window, cx); shell.redraw_diagrams(window, cx); cx.notify(); })`
- `connection.rs:70`: after `theme::refresh(mode, window, cx);` add `shell.redraw_diagrams(window, cx);` If Task 6 has landed, this reload lives in `Shell::reload_settings_if_changed` (`shell/workspace_actions.rs`); add the call after its `theme::refresh` instead.

Compile check: `cargo check -p mkrew-ui --all-targets -j 4`. Expected: no errors. Run: `cargo test -p mkrew-ui --lib diagram::tests -j 4`, then `cargo test -p mkrew-ui --lib docs::tests -j 4`, then `cargo test -p mkrew-ui --lib files_view -j 4`, then `cargo test -p mkrew-ui --test docs_view -j 4`. Expected: PASS (the mermaid tests take a few seconds in debug: system fonts).

- [ ] **Step 6: SPEC, ADR 0021, HUMAN_CHECKS**

`docs/SPEC.md:969-970`: `` ```` ```mermaid ```` blocks are drawn as diagrams (pure Rust\n  renderer, theme colors); `` → `` ```` ```mermaid ```` blocks are drawn as diagrams (pure Rust\n  renderer, for the current appearance only, cached per block text; a theme switch draws the shown page's diagrams again); ``.

`docs/decisions/0021-docs-preview-rendering.md`, in the Mermaid addendum replace `one layout per\n  appearance (some layouts bake the colors in), in `Shown::new`` with `one layout for the\n  current appearance (some layouts bake the colors in; cached by block text and appearance,\n  a theme switch builds the shown page again, 2026-10-13), in `Shown::new``.

Append to the perf `### To verify —` section:

```markdown
- [ ] **Mermaid, one appearance (A5):** open a `.md` with 2-3 mermaid blocks in Docs and in Files (Preview), dark. Diagrams show in dark colors. Switch the theme (palette `Toggle theme`, or the title bar button): the page redraws in light colors within a moment (a few seconds the first time), scroll position kept; switch back: instant. Edit one block in Files Source and go back to Preview (or pause typing in the split preview): only that block takes a moment, the others stay. A broken block still shows its source and the reason. A `.md` without diagrams does not flicker on a theme switch.
- [ ] **Memory (A5):** with 3 diagrams open and the theme switched twice, the UI process stays under 150 MB.
```

- [ ] **Step 7: Verify**

Run: `sh scripts/verify.sh` (fallback: `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace -j 4`). Expected: PASS.

- [ ] **Step 8: Commit**

```
git add crates/mkrew-ui/src/diagram.rs crates/mkrew-ui/src/docs.rs crates/mkrew-ui/src/preview.rs crates/mkrew-ui/src/files_view.rs crates/mkrew-ui/src/files_view/tests.rs crates/mkrew-ui/src/theme.rs crates/mkrew-ui/src/shell/docs.rs crates/mkrew-ui/src/shell/files.rs crates/mkrew-ui/src/shell/connection.rs crates/mkrew-ui/src/shell/workspace_actions.rs crates/mkrew-ui/tests/docs_view.rs docs/SPEC.md docs/decisions/0021-docs-preview-rendering.md docs/HUMAN_CHECKS.md
git commit -m "perf(ui): draw Mermaid for the current theme only and cache drawings by text"
```
Stage only this task's hunks in files other tasks also edit (`files_view.rs`, `shell/files.rs`, `shell/docs.rs`, `connection.rs`, `theme.rs`, `SPEC.md`, `HUMAN_CHECKS.md`). Do not push.

---

### Task 19: The Git History's graph width is counted once per log (perf N5, optional, lowest priority)

> **Refresh 2026-10-13:** NEW task from the rescan (finding N5), **reduced in scope after checking the code; recommend skipping it unless a profile shows the Git view.**
> - The rescan said `list_rows()` is "built about twice per render". It is not: `shell/git.rs:539` is the key handler (`git_key`, once per key press), and the render builds the rows once (`shell/git_render.rs:26`) and passes them to `git_list` (`shell/git_rows.rs:7-16`). Only `graph_lanes()` (`git_rows.rs:22`) is called per render too, and it scans at most 50 graph rows (the History lists the last 50 commits, `git_history.rs:1`).
> - A cache of `list_rows` "per load" is not safe to add cheaply: its result depends on `status`, `mode`, `since`, `filter`, `changed_by`, `collapsed` and the whole `History` (commits, selected, detail), and the shell writes several of those pub fields directly (`shell/git_render.rs:111` `filter.clear()`, `git_diff.rs:704`, the filter input, `collapsed` toggles). A stale list would show wrong files. See Open questions.
> - Kept here: the width is computed when the log is loaded, which is the only place `History.graph` is assigned.

Inbox: none yet (proposed 2026-10-13; the user adds a line if accepted). Spec: §12B (Git view History, lane graph); no SPEC change.

**Current state (investigated):**
- `History { commits, graph: Vec<GraphRow>, selected, detail }` (`git_history.rs:27-35`); `graph` is assigned only in `History::logged` (`:76` `git_graph::layout`, `:64` cleared when a first load fails).
- `GitView::graph_lanes` (`git_view.rs:1097-1102`) = `git_graph::lanes(&history.graph)` per call; callers: `shell/git_rows.rs:22` (render) and tests (`git_view/tests.rs:999`).
- `GitView::list_rows` (`git_view.rs:1024-1095`): see the refresh block; its History branch clones up to 50 commits and graph rows, the Working branch clones the changed files.

**Files:**
- Modify: `crates/mkrew-ui/src/git_history.rs` (`lanes` field, `logged`, test)
- Modify: `crates/mkrew-ui/src/git_view.rs` (`graph_lanes`)

**Interfaces:**
- Produces: `History.lanes: usize` (the width of `graph` in lanes, `git_graph::lanes`), set by `logged`.
- Changes: `GitView::graph_lanes` returns `history.lanes` (same value, no scan).

- [ ] **Step 1: Failing test**

In `git_history.rs` `mod tests` add:

```rust
    #[test]
    fn the_graph_width_is_counted_when_the_log_loads() {
        let mut history = History::default();
        assert_eq!(history.lanes, 0);
        let mut merge = commit("m");
        merge.parents = vec!["a".into(), "b".into()];
        history.logged(Ok(vec![merge, commit("b"), commit("a")]));
        assert_eq!(history.lanes, git_graph::lanes(&history.graph));
        assert!(history.lanes > 1, "a merge needs two lanes");
        history.logged(Ok(vec![commit("a")]));
        assert_eq!(history.lanes, 1);
        let mut failed = History::default();
        failed.logged(Err("not a repo".into()));
        assert_eq!(failed.lanes, 0);
    }
```

Run: `cargo test -p mkrew-ui --lib git_history::tests -j 4`
Expected: FAIL to compile (`History.lanes` does not exist).

- [ ] **Step 2: Implement**

`git_history.rs`: in `History` add `/// Width of [`Self::graph`] in lanes (0 for none), counted when the log loads.` `pub lanes: usize,` after `graph`; `Default` gets `lanes: 0,`; in `logged`, the failure branch adds `self.lanes = 0;` after `self.graph.clear();`, and the success path becomes
```rust
        self.graph = git_graph::layout(&commits);
        self.lanes = git_graph::lanes(&self.graph);
```
`git_view.rs:1097-1102`:
```rust
    /// Width of the History's graph column in lanes (0 outside the History).
    pub fn graph_lanes(&self) -> usize {
        self.history.as_ref().map_or(0, |history| history.lanes)
    }
```
(`use crate::git_graph::{self, GraphCell, GraphRow}` in `git_view.rs:25` keeps its other uses; if `git_graph::` is no longer used there, drop it from the import: rustc says.)

Run: `cargo test -p mkrew-ui --lib git_history::tests -j 4` and `cargo test -p mkrew-ui --lib git_view -j 4`
Expected: PASS (`git_view/tests.rs:999` still expects 2 lanes).

- [ ] **Step 3: Verify**

Run: `sh scripts/verify.sh` (fallback: `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace -j 4`). Expected: PASS. No HUMAN_CHECKS line: nothing visible changes.

- [ ] **Step 4: Commit**

```
git add crates/mkrew-ui/src/git_history.rs crates/mkrew-ui/src/git_view.rs
git commit -m "perf(ui): count the Git history graph width when the log loads"
```
Stage only this task's hunks in `git_view.rs`. Do not push.

---

## Open questions (defaults used above)

| Area (Tn) | Question | Options | Default used |
| --- | --- | --- | --- |
| T1 | While any dialog is open, should hidden-model events still redraw? (dialogs such as compare / history / task editor read models not audited) | a) yes, keep today's behavior; b) audit each dialog | a) `Shown.modal` forces `true` |
| T1 | Window minimized: `panes` is empty, so pane-only data skips the redraw; the restore re-renders (`observe_window_visibility`, `connection.rs:35`). Gate everything on `window.is_visible()`? | a) no, leave; b) also skip all redraws while minimized | a) leave (not in the item) |
| T1 | Terminal-title and other sidebar-owned events (`SessionTerminalTitle`, `SessionUpdated`) still always redraw; narrow them to expanded workspaces? | a) keep (Inbox / dots / `Viewed` depend on them); b) narrow later | a) keep |
| T1 | The HUMAN_CHECKS section `### To verify — 2026-10-13 perf (…)` is created by this task above the first `### To verify —` heading; other plans of the day add their own sections there. | a) insert directly above the first heading at execution time; b) merge into another section | a) |
| T1 | Ports in the full sidebar redraw for any session of any workspace (the RUN group draws only open, unfiltered workspaces). Narrow to open workspaces? | a) keep (a port line is rare); b) add `expanded` to `Shown` | a) keep |
| T1 | A pane output flood still re-renders the Shell root (only the sidebar is cached, 0606f04); this task does not change that. Cache more of the Shell? | a) out of scope; b) later item | a) out of scope |
| T2 | Row builders take owned rows, so the sidebar render still clones the visible rows each time (strings), though the sort / HashMap / lowercase / `format!` / count are cached. Change `workspace_row` / `session_row` / compact variants to `&Row`? | a) leave (text moves into elements anyway); b) change signatures | a) leave |
| T2 | `most_urgent` / `tab_kind` signatures change (lookup closure instead of a slice); their tests change shape, not behavior. | a) change (clean cutover); b) keep slice overloads | a) change |
| T2 | The cache lives in `Sidebar` (model), not in `SidebarView`: the footer count, the peek check and the tests read the same cache and `SidebarView::render` stays a wrapper. | a) in `Sidebar`; b) in `SidebarView` (the footer and tests could not share it) | a) |
| T2 | `Sidebar` becomes `!Sync` (`RefCell` / `Rc`). It is only owned by `Shell`; if a later change needs it on another thread, the cache must move to `Mutex` / `Arc`. | a) accept; b) `Mutex` / `Arc` now | a) accept |
| Docs hidden (T3) | Re-read a changed doc while Docs is hidden? | (a) mark stale, read once when shown; (b) keep reading (today) | (a) `Docs::stale` + `show_docs` |
| Files `.md` preview (T4) | Add a 300 ms time debounce to the Files preview build? | (a) drop: build is already off-thread, one in flight, once per text, equal-hash re-reads skipped; (b) debounce 300 ms (longer "Rendering the preview…" flash after every agent save) | (a) dropped, no code |
| Git summary (T5) | How often may the title bar git summary refresh? | (a) at most once per 2 s, trailing so the last change always shows; (b) only while Git/Files/inspector is open; (c) every `git.changed` (today) | (a) pure `Throttle`, 2 s |
| Settings reload (T6) | When does the UI re-read `settings.toml`? | (a) at start and on window activation (ADR 0055); (b) poll every 500 ms (today); (c) file watcher | (a) |
| Files tree theme clone (T6) | Remove `row_theme = theme.clone()` (`shell/files_render.rs:131`)? | (a) keep: one ~70-token clone per render, list closure is `'static`, same pattern in git_rows/git_diff/root render; (b) `Rc<Theme>` plumbing | (a) keep, dropped from plan |
| T7 | Which `session_event` kinds may be pruned? | a) all kinds older than 7 days; b) only `hook`, `files_changed`, `edited` | b: `usage` rows are read with `since = 0` by `session.usage` (UI totals) and must outlive a week |
| T7 | `synchronous=NORMAL` can lose the last commits on an OS crash or power cut (never corrupts the file). Accept? | a) NORMAL (inbox item 12); b) keep FULL | a; documented in SPEC §5, no pragma test (it would only pin a default) |
| T7 | Old `hook` rows already in users' databases. | a) pruned after 7 days with the other kinds (`hook` stays in `PRUNED_KINDS`); b) one-off `DELETE` at start | a: no migration, no extra code path |
| T7 | A4 tests: inbox says "remove the tests that only pin it". | a) delete the `hook`-row assertions; b) assert the rows are absent | a |
| T7 | The `backup::start` signature gains the db path (`Paths::db_file()`); `Store::backup_to` is removed. | a) free fn `mkrew_store::backup`; b) keep `backup_to` and open a second `Store` | a |
| T7 | `VACUUM INTO` on its own connection while another connection holds `BEGIN IMMEDIATE` (store test). Needs a write lock? | a) no, WAL snapshot read (test as written); b) it blocks | a; if the test fails with `database is locked`, drop the open-writer half and keep the plain-copy test |
| T8 | The review says the watcher "ALWAYS" sends `GitChanged`; in code `debounce` already drops empty batches. Is more needed? | a) only make the send condition explicit; b) also stop watching ignored dirs at the OS level (`notify` has no exclude, would need per-directory non-recursive watches) | a (the `.git`/`target` storm already costs no `stat`; b is a bigger design) |
| T8 | "Any agent `working`" for the 5 s cap: only `SessionKind::Agent`, or every kind? | a) agents only; b) any session kind (what `files_changed` attribution always did) | b: one `working` map shared with T9's attribution; a shell printing output counts as working |
| T8 | A closed view needs a new request (`watch.release`) → protocol 41 + ADR 0056; no existing request fits (`ProjectRef` cannot name a worktree). | a) new request; b) daemon idle timeout; c) keep as is | a |
| T8 | A second desktop UI window would lose a watcher the first released. The phone page (`mk web`) is a second client but never keeps watchers. | a) no ref count (one window); b) ref count per client | a |
| T8 | The Quick-open dialog's `fs.find` keeps a worktree watcher that the UI does not release. | a) leave (stays until the daemon stops, as today); b) include the dialog's target in `Needs` | a: rare, bounded by the number of task worktrees |
| T8 | `Hub::working_sessions` in Task 8 or 9? | a) Task 8 (old plan): private and unused until Task 9, so clippy `-D warnings` fails Task 8; b) Task 9 | b |
| T9 | `docs_changed` still reads and hashes each changed `.md` on the watcher task (review B6 second half). | a) leave it (bounded, feature with HUMAN_CHECKS T6.4); b) move to `spawn_blocking` | a: not in the inbox line; separate change if it shows in a profile |
| T9 | `[features] docs` is off by default (ADR 0046): nothing reads `files_changed` then. Skip recording and `docs_changed` while it is off? | a) keep recording Markdown batches (switching Docs on later still has "changed by"); b) gate on the setting (daemon would need `Hub` to read settings, owned by the feature-switch tasks) | a |
| T9 | Turn tests used `notes.txt` + `files_changed` as their "watcher saw it" barrier. | a) switch the scenarios to `notes.md`; b) add another barrier | a |
| Turn snapshots (T10) | Should the hook of a turn start be answered at once when `[features] turns` is off? | Yes (one `if` in `track_turn`) / leave (the wait is a queue hop + one SQLite write, no git) | Yes |
| Turn snapshots (T10) | Should `[features] turns` be hot per turn (`Hub::features()` read at each turn start, old plan) instead of read at session start (ADR 0046)? | Per session start (today) / per turn | Per session start; not changed, ADR 0046 decided it |
| Turn snapshots (T10) | Should `turn.restore*` name the setting when there is no snapshot ("…set `[features] turns = true`")? | Keep the generic "no snapshot (not a git repo, or older than 7 days)" / name the setting | Keep; the UI hides the entry points, only `mk`/raw clients see the message |
| UI turn features (T11) | "Changed by" in the Git view was listed to be hidden with snapshots off. It reads `session.files` (watcher + hook events), not snapshots. Hide it with `turns = false`? | Keep visible / hide with `turns` | Keep visible |
| UI turn features (T11) | On `turns` switched off while the Turns dialog, a turn Restore or a "Since turn" listing is open: close it? | Leave open (works, trees live 7 days) / close and return to working changes | Leave open |
| Conflicts (T12) | Name of the switch | `conflicts` (like `docs`, `turns`) / `conflict_detection` | `[features] conflicts` |
| Conflicts (T12) | Apply live in the daemon (a `Hub` read at each watcher batch / hook edit) or at session start like `turns`? | Live (needs `Hub::use_settings`) / per session | Live, as the inbox asks ("hot-reloaded") |
| Conflicts (T12) | Clear the daemon tracker when switched off? | Leave (entries older than 60 s expire on the next edit; a project's tracker is dropped when it is unwatched) / clear | Leave |
| Conflicts (T12) | UI: add `Conflicts::track(enabled, ..)` or plain `if features.conflicts` guards? | Guards (two `if`s, a reset on switch-off) / new method with a unit test | Guards |
| Hooks vs scan (T13) | Skip the scan for agents with hooks? | (a) skip when `mode` includes `hook`; (b) keep; (c) skip only for a hypothetical `mode = "hook"` | (b) keep. No hook reports `idle` (it is the screen going quiet), Codex hooks report only `done`, Claude's patterns (`❯ 1\. Yes`) are not replaced by hooks, and the daemon cannot tell whether the user ran `mk integration install`. No shipped profile uses `mode = "hook"` (`agents/*.toml`), so (c) saves nothing today. |
| Scan gap (T13) | Is 250 ms (4 scans/s) right? A state can show up to 250 ms late. | 100 / 250 / 500 ms | 250 ms (`scan::SCAN_GAP`) as the inbox says ~4/s. |
| Flood path (T13) | A flooding session already reaches `on_output` only via the `Pacer` turn (ADR 0002). Is the scan throttle still needed? | (a) yes, a normal busy agent sends ≤ 60 batches/s and never floods; (b) drop the throttle and keep only the 4-row build | (a): the throttle sits in `on_output`, so flood batches, exit drains and normal batches share it. |
| Grid reuse vs OUTPUT_FRAME (T14) | 95a8a6e already caps feeds at one per 16 ms frame; is buffer reuse still worth it? | (a) yes, it removes rows × cols cell and per-cell `Vec` allocations per rebuilt frame; (b) only lower the scrollback | (a), together with 5,000 lines. The reuse falls back to a fresh grid whenever the painter still holds the previous `Arc<Grid>`. |
| Web replay cap (T15) | `mk web` replays 512 KiB while the desktop now replays 1 MiB. Align them? | (a) leave 512 KiB (phone, ADR 0052); (b) share one constant in `mkrew-core` | (a); only the stale comment changes. `replay_start` lives in `mkrew_core::log_tail` and fixes both callers. |
| Premise (T15) | The review says History replays ~10 MB; the code already cuts at 2 MiB (`history::TAIL_BYTES`). | Lower to 1 MiB (done); or only document | Lowered to 1 MiB; the review's number was the log rotation size (5 MB × 2). |
| SGR reset (T15) | Prepend `ESC[0m` to a cut replay? | Add / not | Not added: each replay is a fresh emulator with default attributes and whole lines are dropped before the start. |
| Alt screen (T15) | A cut can drop the `ESC[?1049h` of a TUI session, so its replay paints on the main screen. | Scan back for the last `?1049h`/`?1049l` and prepend it / leave | Leave (HUMAN_CHECKS T5.9 note already documents the limitation). |
| Log write blocking (T15) | `SessionLog::write` (`BufWriter`, 8 KiB) writes on the session's async task. | (a) leave; (b) `BufWriter::with_capacity(64 KiB)` in `session_log::open` (one line, 8× fewer syscalls, but the log of a live session lags up to 64 KiB for `session.history`); (c) a writer thread fed by a channel (changes flush/rotate/exit ordering, needs its own task) | (a) leave: (b) changes what a live History shows, (c) is not cheap. Decide with item 12's SQLite/IO work. |
| INBOX tick (T15) | Task 15 ticks inbox line 138, but INBOX is never part of a task commit. | (a) edit the file, leave it uncommitted; (b) skip the tick | (a). |
| Area (Tn) | Question | Options | Default used |
| T16 | The user ticked `Giữ` for A1 on 2026-10-10; the 12 fps pulse (N1) was added after that. Is a still working dot OK? | a) still dot, redraw on time-label change only (this task); b) keep the stepping dots, drop only the pulse (2 renders/s); c) keep both (today, 14 renders/s) | a), as a proposal; the task runs only after his OK. b) is Steps 1-2 without the `status_mark` / `mark_slot` / `redraw_due` changes |
| T16 | The "Starting agent" button and the voice "Transcribing" marker use the stepping dots (`pane_render.rs:838`, `sidebar.rs:845`); the status tick runs for them (`connection.rs:100-101`). | a) keep the dots (seconds long); b) a still dot too, then `loading_dots`, `status::loading_dot`, `LOADING_DOTS`, `STATUS_DOT_GAP`, `STATUS_DOT_DIM` and the two `due` terms in `connection.rs` go (about 30 lines) | a) keep, renamed `loading_dots` |
| T16 | A still dot says "working" less loudly than motion. Is the elapsed text enough? | a) text carries it (`working · 12s` moves each second for a minute); b) a brighter ring or glyph for Working | a) |
| T16 | Working still redraws the window once per second for the first minute of each turn (label seconds), then once per minute. | a) accept (same as Idle); b) show the elapsed label in minutes only | a) |
| T17 | Poll `daemon.status` and Tailscale together every 3 s while the dialog is open, or split them (status every tick, Tailscale every 3 s)? | a) one `due` for both, every tick while a Start / Stop click waits; b) two clocks | a) (simpler; the wait for a click is the only case that needs speed) |
| T17 | `PHONE_TICK` still wakes the loop every second for the app's life (no render, no poll). | a) leave; b) wake every 5 s when the dialog is closed | a) (a wake that finds `due` false costs nothing visible) |
| T17 | The title bar phone icon (always on) still polls `daemon.status` every 5 s with `[web]` off. | a) keep (it shows `✕ Not running` / enabled state); b) stop polling when `[web]` was never enabled | a) (renders nothing now unless it changed) |
| T18 | The user ticked `Giữ` for A5 on 2026-10-10. | a) do the `Sửa` option (one appearance + cache) as a proposal; b) keep both appearances | a), only after his OK |
| T18 | A theme switch builds the shown Docs page and Files preview again (only when they contain a diagram). A page shown while another workspace's Docs is hidden is rebuilt too. | a) rebuild every Docs page with stale diagrams now; b) only the visible view, others when shown | a) (a switch is rare, drawings are cached; b) needs Task 3's stale flag) |
| T18 | A theme switch within the few hundred ms of a doc read in flight lands a page for the old appearance and nothing rebuilds it until the next switch or doc change. (Files re-asks after every build, so it converges.) | a) accept; b) `update_in` + `redraw_for` check after `docs.read` in `read_doc` | a) |
| T18 | The cache is a process-wide `static` cleared at 64 entries, keyed by a 64-bit `DefaultHasher` of the block text. | a) this; b) an LRU; c) key by the text itself | a) (a collision would show another block's drawing; 2^-64 per pair) |
| T18 | `Shown::new` / `from_text` take `dark`; the build threads read `cx.theme().dark` before spawning. | a) a parameter; b) a global read of the active theme inside the background task | a) (explicit, testable) |
| T19 | `list_rows()` is rebuilt once per render (not twice, see the refresh block). Cache it? | a) no, only the lane width per load (this task); b) a cache keyed by a revision number bumped in every `&mut GitView` method and every direct write of `filter` / `collapsed` / `changed_by`; c) `Rc` rows (`ListRow::Commit { commit: Rc<Commit>, .. }`) so clones are cheap | a); b) is fragile (pub fields written from `shell/git_render.rs:111` and `git_diff.rs`), c) is a bigger change for 50 rows. Revisit only with a profile of the Git view |
| T19 | Is the task worth doing at all? | a) do (3 lines, tested); b) skip, nothing measurable | a) as optional; skipping loses nothing |
