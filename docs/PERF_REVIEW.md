# Perf review — feature nào làm app kém mượt (2026-10-10)

Nguồn: đọc code tĩnh toàn bộ `mkrew-ui`, `mkrew-daemon`, `mkrew-store`, `mkrew-core` (chưa chạy profiler).
`[ĐOÁN]` = suy luận, chưa đo. Số dòng là ở thời điểm review.

Cách dùng: mỗi mục tick **một** ô — `Xoá` (bỏ hẳn feature), `Sửa` (giữ feature, làm cách sửa rẻ đã ghi) hoặc `Giữ` (không làm gì).
Em đã ghi đề xuất ở dòng **Đề xuất**.

## Bức tranh chung

- Đường terminal (PTY → daemon → UI) đã được giới hạn tốc độ tốt: daemon gom output 16 ms (`attach.rs:21`), UI chỉ feed 1 lần mỗi frame (`main.rs:105-107`), hàng không đổi thì không shape lại.
- Thứ làm giật **không phải terminal**, mà là **root `Shell` render lại toàn bộ cây** (title bar, sidebar, tabs, view, status bar, dialog) quá thường xuyên:
  1. Timer 500 ms re-render toàn bộ khi có bất kỳ agent nào đang `working` (= gần như luôn luôn).
  2. Mọi event từ daemon (`session.usage`, `session.turn`, `session.plan`, `session.ports`, `git.changed`…) đều `cx.notify()` root, kể cả khi panel liên quan đang ẩn.
  3. Mỗi lần render, sidebar/inbox sort + clone + format lại toàn bộ session.
- Phía daemon: watcher file đệ quy + `git.changed` mỗi ≤2 s khi agent sửa file, kéo theo spawn `git status` và ghi SQLite.

---

## A. Ứng viên XOÁ (feature tốn nhiều, giá trị thấp so với độ mượt)

### A1. Hiệu ứng 3 chấm "working" chạy động
- **Để làm gì:** 3 chấm nhấp nháy cạnh agent đang chạy (sidebar, tab, header, status bar).
- **Tốn gì:** vòng lặp sống mãi mỗi 500 ms (`main.rs:522-575`, `theme.rs:67`). Có ≥1 session `Working` ở **bất kỳ workspace nào** là `cx.notify()` root → render lại toàn app 2 lần/s (`status.rs:108-110`, `main.rs:565-567`), kể cả session đó không hiện trên màn hình. Chạy suốt lúc agent làm việc = gần như cả ngày.
- **Xoá gồm:** nhánh Working trong `status_mark` (`main.rs` ~20302-20314), `status::working_dot`/`WORKING_DOTS`, `theme::STATUS_DOT*`, nhánh Working trong `redraw_due`. ~50 dòng. Thay bằng chấm tĩnh màu "working". Đồng hồ elapsed/idle giữ lại nhưng chỉ cập nhật khi chữ thay đổi (đã có sẵn cho idle).
- **Đề xuất:** **Xoá animation, giữ chấm tĩnh.** Lợi lớn nhất, rủi ro thấp nhất.
- [ ] Xoá  - [ ] Sửa (chỉ notify khi session đang hiện trên màn hình)  - [x] Giữ

### A2. Turn snapshots + Undo theo turn + diff theo turn
- **Để làm gì:** mỗi lượt (turn) của agent chụp snapshot git để xem "turn này sửa gì" và hoàn tác 1 turn (Turns dialog, Undo dialog, `turn.restore`).
- **Tốn gì:** mỗi turn của **mọi** agent: copy git index + `git add -A -- :/` + `write-tree` + `update-ref` lúc bắt đầu VÀ kết thúc, thêm `git diff --numstat` (`git.rs:551-571`, `turns.rs:84-117`) ≈ 4-7 lần spawn git/turn, ~50 ms/lần trên Windows. Khoá theo project nên nhiều agent cùng project phải xếp hàng. Hook bắt đầu turn **đợi** snapshot xong → agent bị chậm theo [ĐOÁN thời gian]. Refs tích luỹ trong `refs/mkrew/turns` 7 ngày. Thêm ghi SQLite + broadcast `session.turn` → root re-render.
- **Xoá gồm:** `daemon/snapshot.rs` (~390), `daemon/turns.rs` (~505), `daemon/undo.rs` (~490), `daemon/server/turn.rs`, `core/turn.rs`, `ui/turns_dialog.rs`, `ui/undo_dialog.rs`, `ui/turn.rs`, protocol `session.turn`/`turn.*`, `git.diff since=tree`. Bảng `turn` để lại (migration không được xoá). ~2.500 dòng. Phụ thuộc: Git view "since turn", "Changed by" trong inspector.
- **Đề xuất:** **Xoá** nếu anh không dùng Undo theo turn (git + checkpoint/rewind của chính agent CLI đã làm việc này). Nếu còn dùng: Sửa = chỉ snapshot khi bật setting.
- [ ] Xoá  - [x] Sửa (opt-in setting, mặc định tắt)  - [ ] Giữ

### A3. Phát hiện conflict (2 agent sửa cùng file)
- **Để làm gì:** cảnh báo khi hai session cùng sửa một file trong cùng project.
- **Tốn gì:** mỗi batch watcher (≤2 s khi agent làm việc): `list_sessions` toàn bảng (`session.rs:727`), `record_edits` (+ thêm 1 `list_sessions` khi trùng), broadcast `conflict.detected`; UI gọi `conflicts.prune(&board, sessions)` trên **mọi** event (`main.rs` ~8454-8457).
- **Xoá gồm:** `core/conflict.rs`, `ui/conflicts.rs` (~335), `record_edits` + Tracker trong `session.rs`, event `conflict.detected`. ~600 dòng.
- **Đề xuất:** **Xoá.** Chỉ hữu ích khi nhiều agent cùng sửa một thư mục mà không dùng worktree — hiếm, và khi có nhiều agent `working` thì nó đã không attribute được (`session.rs:705-706`).
- [ ] Xoá  - [x] Sửa (otp-in setting, mặc định tắt)  - [ ] Giữ

### A4. Lưu nguyên payload hook vào SQLite
- **Để làm gì:** ghi lại JSON thô của mỗi hook event (`insert_session_event("hook", …)`, `session.rs:1231`).
- **Tốn gì:** 1 lần ghi SQLite (synchronous=FULL, giữ mutex store toàn cục) mỗi hook; bảng `session_event` phình mãi, không có dọn dẹp. **Không có code production nào đọc lại** kind `hook` (chỉ test).
- **Xoá gồm:** 1 lời gọi insert + test liên quan. ~10 dòng. Rủi ro: không.
- **Đề xuất:** **Xoá.** Không mất gì.
- [x] Xoá  - [ ] Giữ

### A5. Mermaid diagram trong preview Markdown
- **Để làm gì:** block ```` ```mermaid ```` trong Docs/Files preview vẽ thành sơ đồ.
- **Tốn gì:** mỗi block layout + render SVG **2 lần** (sáng và tối) mỗi lần parse lại doc (`diagram.rs:41-65`); lần đầu load font hệ thống (chậm). Kết hợp A6/B3 thì chạy mỗi lần agent sửa doc hoặc mỗi phím gõ trong Files preview. Crate `mermaid-rs-renderer` nặng compile + binary; renderer có thể panic với input gõ dở (đã `catch_unwind`, `diagram.rs:31`).
- **Xoá gồm:** `ui/diagram.rs` (175), field `diagrams` trong `docs.rs`, nhánh diagram trong `preview.rs` (~40), dependency `mermaid-rs-renderer` (crate trong workspace). Block mermaid sẽ hiện như code block (fallback đã có).
- **Đề xuất:** **Xoá** nếu anh ít đọc sơ đồ trong app (Zed / GitHub vẫn xem được). Nếu cần: Sửa = chỉ render theme đang dùng + cache theo hash nội dung block.
- [ ] Xoá  - [ ] Sửa (1 theme + cache theo hash)  - [x] Giữ

### A6. Live preview khi đang sửa file .md trong Files
- **Để làm gì:** trong Files editor, xem preview Markdown cập nhật theo từng phím gõ (Docs view vẫn có preview riêng).
- **Tốn gì:** mỗi thay đổi text: copy toàn bộ text (`files_view.rs:595-606`) + parse markdown + syntect + mermaid ×2, **không debounce** (`main.rs` ~1944-1963). Render dựng lại toàn bộ element tree của doc mỗi frame, clone từng dòng code (`preview.rs:264-267`), không virtualize.
- **Xoá gồm:** chế độ Preview cho file đang sửa: `MdPreview`/`preview_*` trong `files_view.rs` (~80), `build_preview` + `render_md_preview` + toggle trong `main.rs` (~90).
- **Đề xuất:** **Sửa** (debounce 300 ms) nếu anh hay viết plan/spec trong app; **Xoá** nếu anh viết .md bằng Zed.
- [ ] Xoá  - [x] Sửa (debounce 300 ms)  - [ ] Giữ

---

## B. GIỮ feature nhưng SỬA rẻ (đang làm tốn vô ích)

### B1. Event daemon nào cũng re-render toàn app
- **Để làm gì:** cập nhật usage, turns, plan, ports, composer, board, sidebar… theo thời gian thực.
- **Tốn gì:** `Shell::dispatch` (`main.rs` ~8417-8530) chuyển mọi event qua ~12 model; `session.usage`/`turn`/`plan`/`ports` của **bất kỳ** session nào đều `cx.notify()` root, kể cả khi inspector đóng / session ở workspace khác.
- **Sửa:** chỉ notify khi model đó đang hiển thị (inspector mở + đúng session, Board đang mở…).
- [x] Sửa  - [ ] Giữ

### B2. Sidebar / inbox / tab tính lại mỗi frame
- **Để làm gì:** danh sách session có đếm số, chip, filter; nhãn "N need you"; dấu khẩn trên tab.
- **Tốn gì:** `Sidebar::view` (`sidebar.rs:437-551`) mỗi render: lowercase filter, dựng HashMap, sort, clone String từng hàng, `format!` summary; gọi 1-2 lần/frame. `inbox_entries()` (`inbox.rs:62-78`) clone + sort cả danh sách chỉ để lấy `.len()` cho status bar. Tab bar `.find` O(panes×sessions).
- **Sửa:** cache view + đếm inbox, đánh dấu dirty khi `apply`/`load`/đổi filter. ~60 dòng.
- [x] Sửa  - [ ] Giữ

### B3. Docs tự đọc lại doc kể cả khi đang ẩn
- **Để làm gì:** doc đang chọn trong Docs view tự cập nhật khi file đổi trên đĩa.
- **Tốn gì:** `doc_event` gọi `read_doc` cho **mọi** workspace có doc đang chọn, không kiểm tra Docs có đang hiện không (`main.rs:1743-1747`) → parse markdown + syntect + mermaid mỗi lần agent ghi file plan/spec.
- **Sửa:** khi ẩn chỉ đánh dấu stale, đọc lại khi mở. ~15 dòng (Files/Git đã làm đúng kiểu này).
- [x] Sửa  - [ ] Giữ

### B4. Git summary trên title bar/status bar refresh mỗi `git.changed`
- **Để làm gì:** tên branch + số file thay đổi.
- **Tốn gì:** mỗi batch watcher → `git.repos` + `git status` cho từng repo của workspace đang hiện, bất kể đang ở view nào (`main.rs` ~2669-2710, ~8493-8500). Agent sửa file liên tục = spawn git liên tục.
- **Sửa:** throttle ≥2 s, hoặc chỉ refresh khi Git/Files view hay inspector mở.
- [x] Sửa  - [ ] Giữ

### B5. Watcher file + broadcast `git.changed` (daemon)
- **Để làm gì:** Files tree, Git view, Docs, attribution file theo session cập nhật tự động.
- **Tốn gì:** watcher đệ quy toàn bộ project root (`watch.rs:135-147`); event từ `.git`/`target`/`node_modules` vẫn đến rồi mới bị lọc → `cargo build` của agent tạo bão event. Mỗi batch (300 ms quiet / tối đa 2 s) **luôn** gửi `GitChanged` tới mọi UI client, kể cả batch rỗng (`session.rs:661-670`). Watcher của project "kept" (đã mở Git/Docs) không bao giờ dừng (`session.rs` ~186-192).
- **Sửa:** chỉ gửi khi batch có thay đổi thật; tăng MAX_DELAY lên ~5 s khi có agent `working`; dừng watcher "kept" khi view đóng.
- [x] Sửa  - [ ] Giữ

### B6. Ghi `files_changed` vào SQLite mỗi batch watcher
- **Để làm gì:** Docs view biết file .md nào do agent sửa ("changed by", đọc ở `server/doc.rs:29`).
- **Tốn gì:** mỗi batch: 1 insert/session live + `list_sessions` (`session.rs:707-738`), `docs_changed` đọc + hash mọi file .md đổi ngay trên task watcher.
- **Sửa:** chỉ ghi khi batch có file `.md` (người đọc duy nhất là Docs); dùng danh sách session trong RAM thay vì `list_sessions`.
- [x] Sửa  - [ ] Xoá (mất "changed by agent" trong Docs)  - [ ] Giữ

### B7. SQLite `synchronous=FULL` + backup `VACUUM INTO` giữ mutex
- **Để làm gì:** an toàn dữ liệu; backup DB hằng ngày (giữ 7 bản).
- **Tốn gì:** chỉ set WAL + foreign_keys (`store/lib.rs:100-101`), mặc định synchronous=FULL → fsync mỗi commit (hook, state, usage…). Backup `VACUUM INTO` chạy dưới mutex store toàn cục → mọi request chờ (`backup.rs:22`).
- **Sửa:** `PRAGMA synchronous=NORMAL` (an toàn với WAL), VACUUM trên connection riêng, thêm dọn `session_event` cũ (7 ngày).
- [x] Sửa  - [ ] Giữ

### B8. Poll file settings mỗi 500 ms (hot-reload)
- **Để làm gì:** sửa `settings.toml` bằng tay thì font/theme/diff_mode áp dụng ngay.
- **Tốn gì:** `std::fs::metadata` trên UI thread mỗi 500 ms, suốt đời app (`main.rs:528-532`).
- **Sửa:** chỉ kiểm tra khi cửa sổ được focus lại (`observe_window_activation` đã có, `main.rs` ~513). ~10 dòng.
- [x] Sửa  - [ ] Xoá (phải restart app khi đổi settings)  - [ ] Giữ

### B9. Daemon quét màn hình terminal mỗi batch output
- **Để làm gì:** phát hiện working/idle/needs_input từ nội dung màn hình (agent không có hook).
- **Tốn gì:** mỗi batch (≤60/s/session, cả session chạy nền): hash mọi ô màn hình bằng SipHash, nếu đổi thì cấp phát `String` cho mọi dòng tới cursor + chạy regex (`session.rs:1198-1209`, `daemon/emulator.rs:109-141`), dù state machine chỉ dùng 4 dòng cuối (`core/state.rs:21`).
- **Sửa:** chỉ dựng 4 dòng cuối; giới hạn quét ~4 lần/s; bỏ qua khi agent có hook.
- [x] Sửa  - [ ] Giữ

### B10. UI terminal: scrollback 10.000 dòng + dựng lại grid mỗi frame
- **Để làm gì:** cuộn xa / find trong lịch sử; snapshot grid để vẽ.
- **Tốn gì:** `SCROLLBACK_LINES = 10_000` mỗi pane (`terminal/emulator.rs:26`) → RAM + reflow khi resize tỉ lệ theo lịch sử. `grid()` (`emulator.rs:357-410`) cấp phát lại toàn bộ `rows×cols`, mỗi ô có `Vec<char>` zerowidth, mỗi frame có output.
- **Sửa:** scrollback 3.000-5.000 (log trên đĩa vẫn giữ toàn bộ); bỏ `Vec` per-cell, tái dùng buffer.
- [x] Sửa  - [ ] Giữ

### B11. Files editor tính dòng dài nhất mỗi frame
- **Để làm gì:** chiều rộng cuộn ngang của editor.
- **Tốn gì:** `max_by_key` + `display_col` trên toàn file mỗi render (`main.rs` ~11650). Clone `Theme` mỗi render.
- **Sửa:** cache theo version text. ~10 dòng.
- [x] Sửa  - [ ] Giữ

### B12. Mở session đã thoát → replay toàn bộ log
- **Để làm gì:** xem lại output của session cũ.
- **Tốn gì:** replay tới ~10 MB log vào emulator mới khi mở (`terminal/view.rs:134-135`); ghi log blocking trên task session.
- **Sửa:** chỉ replay ~1 MB cuối.
- [x] Sửa  - [ ] Giữ

---

## C. GIỮ — chỉ tốn khi dùng, hoặc rẻ

| Feature | Để làm gì | Vì sao giữ |
|---|---|---|
| Worktrees / fan-out | Chạy nhiều agent trên branch riêng | Chỉ tốn khi dùng (thêm 1 watcher/worktree khi mở Files/Git). Xoá chỉ giảm độ phức tạp, không tăng độ mượt. |
| Board view tick | Đồng hồ trên card task | Chỉ chạy khi Board đang mở (`main.rs:561-564`). |
| Usage (token/context %) | Hiện token, context % | Đọc incremental 1 lần/turn (`daemon/usage.rs:129`). Phần notify root đã nằm ở B1. |
| Plan checklist | Tiến độ plan `.md` gắn với session | 1 watcher/plan, debounce 300 ms, chỉ khi đã link. |
| OS notifications | Báo khi agent cần anh / xong | Chỉ trên event state, throttle 30 s. |
| Keep-awake | Không cho máy ngủ khi agent chạy | Không poll; có setting tắt. |
| AI generate (commit msg…) | Sinh nội dung bằng agent | Chỉ khi bấm. |
| Follow in Zed | Mở file agent đang sửa trong Zed | Mặc định tắt, debounce. |
| Terminal title tracking | Tự đặt tên pane theo title agent | 1 lock + so sánh mỗi batch; chỉ broadcast khi đổi. |
| Port scan | Hiện port của Run command | Chỉ session kiểu run. |
| Link Ctrl+Click, Find trong terminal | Mở link; tìm text | Chỉ khi giữ Ctrl / khi mở find. |
| Syntax highlight (syntect) | Tô màu code | Chạy nền, giới hạn 512 KB, có cache. |
| Peek hover, palette, quick open, các dialog | Tiện ích | Timer 1 lần / chỉ khi mở. |
| Session log | Lịch sử output, `mk read` | BufWriter, không fsync; xem B12. |
| Double emulation (daemon + UI) | Daemon cần màn hình cho state/attach | Kiến trúc lõi; bỏ = viết lại. |

---

## Thứ tự em đề xuất làm (lợi nhất trước)

1. A1 (chấm tĩnh) + B1 + B2 → hết re-render toàn app 2 lần/s, đây là nguồn giật chính khi agent chạy.
2. A4 + B7 → bớt fsync, DB không phình.
3. B3 + B4 + B5 → hết chuỗi "agent sửa file → spawn git → re-render" khi không xem Git.
4. A2 / A3 / A5 / A6 tuỳ anh tick.
5. B8–B12 dọn nốt.

Sau khi làm nên đo lại: CPU của `mkrew-ui` khi 2-3 agent cùng `working`, trước/sau.
