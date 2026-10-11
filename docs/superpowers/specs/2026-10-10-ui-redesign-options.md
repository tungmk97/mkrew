# UI redesign: danh sách lựa chọn (2026-10-10)

So sánh mockup redesign (ảnh đính kèm `01M4GMT6K7G35TBCC03GG7NFX4.png`) với code hiện tại
(`crates/mkrew-ui/src/main.rs`, `sidebar.rs`, `composer.rs`, `usage.rs`; daemon `usage.rs`/`session.rs`;
`mk integration`). Màu nền **giữ nguyên** token `theme.rs` (bg `#17181C`, sidebar `#131418`, …).
Chưa sửa code nào.

**Cách chọn:** đánh `[x]` vào cột "Chọn" (hoặc ghi chú bên cạnh), rồi báo em. Em viết plan theo
quy trình inbox/plan và dừng để anh duyệt trước khi code.

Effort: **S** = nhỏ, chỉ UI · **M** = cần dữ liệu/protocol mới · **L** = lớn, phụ thuộc định dạng từng agent.

## Lưu ý trước khi chọn

1. **Phần lớn dữ liệu của panel phải (Run) và thanh trạng thái chưa có.** Daemon chỉ biết state,
   token input/output/context (đọc lúc kết thúc turn) và file bị sửa. Không có: chi phí $, model,
   effort, tool call, subagent, plan/todo, tok/s, cache tách riêng.
2. **Mockup chụp pane omp, nhưng omp chưa có usage.** Chỉ `agents/claude.toml` và `agents/codex.toml`
   có `[usage]`, nên `ctx %` và token đang trống với omp (xem B6).
3. Phần giữa mockup ("Task 1 agent…", "waiting on 1 job") là output terminal của omp, không phải UI
   mới. Terminal vẫn là phần chính.

## A. Thay đổi UI (dữ liệu đã có hoặc dễ lấy)

| Chọn | ID | Vùng | Hiện tại | Mockup | Effort | Hiệu năng | Đề xuất |
|---|---|---|---|---|---|---|---|
| [x] | A1 | Title bar | Thanh tiêu đề OS; toolbar 44px nằm riêng bên dưới | Gộp làm một: logo, workspace ▾, branch, views, History/+Agent/+Shell/Run, nút ─ □ ✕ tự vẽ | M | Không đáng kể; lấy lại ~32px chiều cao | **Nên làm.** gpui-pre 0.3.7 có `WindowControlArea`. Rủi ro: Snap Layouts Win11, double-click maximize, macOS/Linux. Cần ADR |
| [x] | A2 | Toolbar: workspace ▾ | Chuyển qua sidebar hoặc Alt+1..9 | Dropdown tên workspace | S | 0 | Làm nếu có A1 hoặc C1 (khi ẩn sidebar) |
| [x] | A3 | Toolbar: branch | Branch chỉ hiện trong Git view | `⎇ master` | S | 1 lần `git.status` khi đổi workspace và khi có `git.changed`, chỉ workspace đang mở | **Nên làm** |
| [x] | A4 | Badge `Git 26` | Không có | Số file thay đổi trên tab Git | S | Dùng chung dữ liệu A3 | **Nên làm** (chỉ workspace đang mở) |
| [x] | A5 | Sidebar: ô lọc session | Không có (chỉ palette) | "Filter sessions…" | S | Lọc O(n) ở client | Nên làm |
| [x] | A6 | Sidebar: chip All / Working / Needs you | Không có | Chip lọc kèm số đếm | S | 0 | **Nên làm**, đi cùng A12 |
| [x] | A7 | Hàng workspace | Avatar, tên, token, pill đang chạy, `?` | Thêm chữ "1 working" / "2 idle" | S | 0 | Nên làm (bỏ số token ở hàng này, đưa xuống thanh trạng thái) |
| [x] | A8 | Nút thu gọn tất cả | Chỉ thu gọn từng hàng | Nút ⇅ ở header sidebar | S | 0 | Tùy anh |
| [x] | A9 | Hàng session: dòng 2 | Một dòng `agent · title` | Session đang chọn có dòng 2 (title/task) + progress bar | S (dòng 2) / M (progress, cần B4) | 0 | Dòng 2: nên làm. Progress bar: chỉ khi làm B4 |
| [x] | A10 | Tab strip | Status mark + tên + ✕; `+` nằm sát tab | Icon loại tab; bên phải có `+`, split, `⋯` | S | 0 | Nên làm (split hiện chỉ có phím Alt+Shift+=/-) |
| [x] | A11 | Header của pane | Status, chip, tên, ctx%, chữ "working · 1h 25m", Follow, Turns | Breadcrumb `kit › mkrew › omp` + pill "Working · 1h 25m" | S | 0 | **Nên làm.** Giữ header riêng từng pane vì có split |
| [x] | A12 | Inbox | Card cuối sidebar | Bỏ card; thay bằng chip "Needs you" + "Inbox clear" ở thanh trạng thái | S | 0 | **Nên làm.** Bớt code. Chip phải gồm cả session `done` chưa xem; peek đã có trên hàng sidebar |
| [x] | A13 | Thanh trạng thái dưới cùng | Không có | Nút ẩn sidebar, Inbox, hash HEAD, Context %, Out, elapsed, $, nút ẩn panel | S (với dữ liệu đã có) | Re-render khi có `session.usage`, rẻ | **Nên làm**, chỉ với dữ liệu thật: inbox, HEAD, ctx%, in/out, elapsed |
| [ ] | A14 | Composer | Mở/tắt bằng Ctrl+Shift+Enter; gửi chỉ bằng Ctrl+Enter | Luôn hiện, có nút gửi ↑ và Interrupt | S | 0 | Nút gửi + Interrupt (gửi Esc vào PTY): **nên làm**. "Luôn hiện": **không nên** (trùng ô nhập của agent, chiếm chỗ terminal); có thể để một dòng thu gọn |
| — | A15 | Màu | — | Mockup hơi xám xanh | — | — | Giữ token hiện tại (theo ý anh) |

## B. Tính năng mới (cần dữ liệu mới)

| Chọn | ID | Tính năng | Nguồn dữ liệu | Effort | Hiệu năng | Đề xuất |
|---|---|---|---|---|---|---|
| [x] | B1 | Panel phải (khung, bật/tắt, độ rộng, theo pane đang focus) | Chỉ UI | M | Panel ~300px làm terminal mất cột, agent TUI phải reflow | Nên làm, **mặc định đóng**, có phím tắt |
| [x] | B2 | Card "Changes · Review diff M23 ?3" | `git.status` đã có; bấm mở Git view | S | Dùng chung A3 | **Nên làm** |
| [x] | B3 | Open Transcript + Stop | Path transcript Claude/Codex đã biết (`locate`); Stop = kill session | S | 0 | Nên làm (omp cần B6) |
| [x] | B4 | Plan checklist (6/20, danh sách task) | Gắn file plan `.md` vào session, đếm `- [ ]`/`- [x]`, watch file | M | Watch 1 file + parse md nhỏ, rẻ | **Nên làm** cách này (không phụ thuộc agent). Bỏ thanh phase "Setup/Tasks/Finish" (định dạng riêng của SDD ledger) |
| [ ] | B5 | Tool hiện tại + số tool call ("bash clippy… · 3m 20s", 27 tools) | Claude: thêm hook `PreToolUse`/`PostToolUse`. omp: extension đã nghe `tool_execution_start/end`, chỉ cần báo thêm. Thêm tracker trong daemon + event protocol | M | ⚠ Mỗi tool call của Claude spawn thêm `mk hook` đồng bộ, agent chậm mỗi lần gọi tool [ước tính vài chục ms trên Windows]. omp gửi async, không ảnh hưởng | Cân nhắc. Làm cho omp trước; Claude tùy chọn |
| [x] | B6 | Usage cho omp (ctx%, token) | Reader mới `omp_jsonl` cho định dạng session của omp (cần kiểm tra định dạng) | M | Đọc tăng dần ở cuối turn như Claude | **Nên làm** nếu anh dùng omp chính; không thì A13/B1 trống với omp |
| [ ] | B7 | Usage live giữa turn + tok/s | Watch transcript liên tục thay vì chỉ đọc cuối turn | M | Watcher + parse mỗi lần agent ghi, cần throttle ~1s; tốn CPU hơn | Không nên, giá trị thấp |
| [x] | B8 | Tách cache (Cache 42M / 1.4M) | `UsageReader` đã đọc các field, hiện gộp vào input | S | 0 | Tùy anh, giá trị thấp |
| [x] | B9 | Hiển thị model hiện tại | Claude JSONL có `message.model`, reader đã đọc file này | S | 0 | Tùy anh |
| [ ] | B10 | Chọn model / Effort trong composer | Gửi `/model` vào TUI, mỗi agent một kiểu; daemon không biết trạng thái | M–L | 0 | **Không nên.** Dễ lệch với trạng thái thật của agent |
| [ ] | B11 | Chip "Auto-approve (yolo)" | Suy từ args profile (`--dangerously-skip-permissions`, `--yolo`…) | S | 0 | Tùy anh, hữu ích như cảnh báo |
| [ ] | B12 | `/` commands trong composer | Danh sách lệnh builtin/skill/`.claude/commands` của từng agent | M | 0 | Không nên. Gõ `/` thẳng vào terminal đã được |
| [ ] | B13 | Card subagent (mô tả, 27 tool calls, 20 req, $) | Parse subagent riêng từng agent (sidechain Claude, session con omp) | L | Parse thêm nhiều file | **Không nên.** Định dạng hay đổi, dễ vỡ |
| [ ] | B14 | Chi phí $ (status bar, card) | Bảng giá theo model | M | 0 | **Không nên.** SPEC §20E.3 ghi "No money estimate"; giá đổi; gói subscription thì $ vô nghĩa. Làm thì cần ADR + sửa SPEC |
| [ ] | B15 | "Rules in effect" (contract.md, rulings.md) | Không có nguồn chung (file riêng của SDD) | M | 0 | Không nên |
| [x] | B16 | Ctrl+click link/path trong terminal | Nhận diện URL/path theo hàng | M | Chỉ quét khi hover/Ctrl, không ảnh hưởng đường render nóng | Tùy anh, hữu ích nhưng không bắt buộc |

## C. Phím tắt

| Chọn | ID | Tính năng | Hiện tại | Effort | Đề xuất |
|---|---|---|---|---|---|
| [x] | C1 | Ẩn/hiện sidebar và panel phải | `keymap.rs` chưa có lệnh này | S | **Nên làm**, đi cùng A13/B1 |

## Gợi ý chia gói

| Chọn | Gói | Gồm | Ghi chú |
|---|---|---|---|
| [ ] | **Gói 1: chỉ UI, rủi ro thấp** (mặc định em đề xuất) | A3, A4, A5, A6, A7, A9 (dòng 2), A10, A11, A12, A13, A14 (nút gửi + Interrupt), C1 | Không cần protocol mới |
| [ ] | Gói 2: title bar | A1, A2 | Tách riêng vì rủi ro cửa sổ Windows; cần ADR |
| [ ] | Gói 3: panel phải tối thiểu | B1, B2, B3, B4 (+ B6 nếu dùng omp chính) | |
| [ ] | Gói 4: theo dõi tool | B5 (omp trước), B9, B11 | |
| [ ] | Không làm (trừ khi anh muốn) | B7, B10, B12, B13, B14, B15 | B14 cần ADR + sửa SPEC |

## Ghi chú của anh

- Đối với các task liên quan đến git thì thường workspace của a sẽ mở là 1 hoặc nhiều git repo, nên là sẽ cần vẫn phải support cho trường hợp này cả show đủ và đúng cho cả 1 hoặc N repo, không nên nếu mở ở root, không có .git thì không show gì
