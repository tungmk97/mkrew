# Làm việc với Oh My Pi (omp) để hoàn thành Mkrew

Bổ sung cho `docs/GUIDE_VI.md`, dành riêng cho omp. Tên cờ và lệnh có thể khác chút theo phiên bản omp — kiểm tra bằng `omp --help`.

## 1. omp dùng bộ kit như thế nào

omp tự đọc `AGENTS.md`, `CLAUDE.md` và các skill trong `.claude/skills/`. Thứ omp KHÔNG dùng được là subagent trong `.claude/agents/` (định dạng của Claude Code). Thay thế:

| Trong bộ kit (Claude Code) | Với omp |
| --- | --- |
| Subagent `reviewer` | `/review` (xếp hạng P0–P3), hoặc vai trò `advisor` soát mọi lượt |
| Subagent `researcher` | `web_search` có sẵn; subagent `task` chỉ để đọc/tra cứu |
| Hook SessionStart | Không cần: lời nhắc vòng lặp bảo omp đọc `docs/PROGRESS.md` trước |
| `/mkrew-next` | Gõ: "Follow docs/BUILD_LOOP.md exactly and do ONE task." |

## 2. Cấu hình một lần

1. **Model theo vai trò** (`/model` hoặc `~/.omp/agent/config.yml`):
   - `default`: model code mạnh nhất bạn có.
   - `advisor`: model khác hãng, để soát lỗi chéo trong lúc làm.
   - `smol`: model rẻ cho subagent tra cứu.
2. **Chuỗi dự phòng** `retry.fallbackChains`: khi model chính bị 429/hết hạn mức, model kế tiếp làm nốt. Nên bật nếu chạy vòng lặp qua đêm.
3. **rust-analyzer**: `rustup component add rust-analyzer`. omp nối LSP vào mỗi lần ghi file nên thấy lỗi biên dịch ngay.
4. **Cờ chạy tự động**: script dùng `omp -p --auto-approve`. Nếu bản của bạn khác, đặt `OMP_FLAGS`, ví dụ `OMP_FLAGS="--auto-approve --model <tên>"`.
5. **Bộ nhớ của omp**: để tắt, hoặc chỉ dùng cho bài học kỹ thuật. `docs/PROGRESS.md` là trí nhớ duy nhất của vòng lặp; hai nguồn trí nhớ dễ lệch nhau.

## 3. Task đầu tiên có người xem

```bash
cd mkrew
omp
```

Gõ:

```
Follow docs/BUILD_LOOP.md exactly and do ONE task. Instead of the reviewer subagent,
run /review on the uncommitted changes and fix every P0/P1 before committing.
```

- `Alt+A`: Agent Hub, xem subagent đang làm gì.
- Task khó: thêm từ `ultrathink` vào lời nhắc.

## 4. Hai điều nên tránh

- **Không dùng `task` để chia việc SỬA CODE song song.** Nó tách ra worktree riêng, phá nguyên tắc "một phiên một task, commit gọn" và dễ đụng `PROGRESS.md`. Chỉ dùng subagent để đọc/tra cứu.
- **Không dùng `omp commit` để tự chia commit.** Vòng lặp yêu cầu code và cập nhật `PROGRESS.md` nằm chung một commit.

## 5. Lộ trình hoàn thành dự án

| Giai đoạn | Cách chạy | Ghi chú |
| --- | --- | --- |
| M0 (terminal trong GPUI, rủi ro cao) | Tương tác, model mạnh nhất, bật advisor, `ultrathink` | Xong thì làm cổng quyết định GPUI/Tauri (GUIDE_VI mục 6) |
| M1 (daemon + CLI) | `bash scripts/autobuild.sh --agent omp --until M1` | Phần không có UI, hợp chạy tự động |
| M2 (UI terminal) | Tự động, xen phiên tương tác sau mỗi 2–3 task | Mở app xem, ghi nhận xét vào "Notes for the next session" |
| M3–M6 | Tự động từng milestone (`--until M3`, rồi `--until M4`…) | Sau mỗi milestone: `status.sh`, `HUMAN_CHECKS.md`, `git log` |
| M7 (Files & Git) | Như M2 | Nhiều UI, cần mắt người |
| M8 (hoàn thiện, đóng gói) | Tự động | Kiểm tra bản cài trên Windows bằng tay |

Có thể dùng Claude Code xen kẽ bất kỳ lúc nào: hai agent đọc chung `docs/PROGRESS.md`.

## 6. Khi omp bị kẹt

1. Đọc log phiên cuối trong `.mkrew-build/logs/`.
2. Mở `omp` tương tác, gõ: `Read docs/PROGRESS.md, investigate why task Tx.y is blocked, ultrathink, then fix it following docs/BUILD_LOOP.md.`
3. Lỗi khó trong daemon/PTY: gợi ý omp dùng debugger (lldb) để bước tới chỗ lỗi thay vì thêm log.
4. Vẫn kẹt: ghi gợi ý vào "Notes for the next session", đổi `[!]` thành `[ ]`, đặt `status: RUNNING`, commit, chạy lại — hoặc giao task đó cho Claude Code.
