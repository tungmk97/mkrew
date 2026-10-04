# Hướng dẫn làm việc với Claude Code và Oh My Pi (omp)

Tài liệu cho người dùng (MK), không phải cho agent. Agent đọc `AGENTS.md`, `CLAUDE.md` và `docs/BUILD_LOOP.md`.

## 1. Cài đặt một lần (Windows)

1. **Rust:** cài rustup (chọn toolchain MSVC) và Visual Studio Build Tools với gói "Desktop development with C++".
2. **Git for Windows:** cung cấp Git Bash. Claude Code trên Windows cũng dùng Git Bash, và các script của bộ kit chạy trong đó.
3. **Claude Code:** cài theo hướng dẫn chính thức, chạy `claude` một lần để đăng nhập.
4. **omp (tùy chọn):** trong PowerShell chạy `irm https://omp.sh/install.ps1 | iex`, sau đó `omp` để đăng nhập/chọn model. Chạy `omp --help` để xem đúng tên cờ chạy không tương tác và tự duyệt (bộ kit mặc định dùng `-p --auto-approve`; nếu khác, đặt biến `OMP_FLAGS`).
5. **Zed (tùy chọn):** để đọc code agent viết.

## 2. Khởi tạo repo

```bash
# trong Git Bash, tại thư mục đã giải nén
cd mkrew
git init
git add -A
git commit -m "chore: bootstrap mkrew build kit"
```

## 3. Chạy thử một task có người xem (nên làm)

```bash
claude
```

Trong Claude Code:

1. Gõ `/` để thấy các skill `mkrew-next`, `mkrew-plan-milestone`, `mkrew-verify`, `mkrew-handoff`, `mkrew-ui-design`.
2. Gõ `/agents` để thấy 3 subagent: `researcher`, `reviewer`, `ui-reviewer`.
3. Gõ `/hooks` để kiểm tra hook SessionStart đã được nhận (nó in trạng thái build vào ngữ cảnh).
4. Gõ `/mkrew-next` và xem agent làm task T0.1 từ đầu tới commit.

Nếu agent hỏi xin quyền một lệnh mà bạn thấy an toàn và hay dùng, thêm lệnh đó vào `permissions.allow` trong `.claude/settings.json`, vì ở chế độ tự động các lệnh ngoài danh sách sẽ bị từ chối.

Với omp: chạy `omp` trong thư mục repo. omp tự đọc `AGENTS.md`, `CLAUDE.md` và các skill trong `.claude/skills/`. Gõ: "Follow docs/BUILD_LOOP.md and do exactly one task." omp không dùng được subagent của Claude, nên theo quy trình nó tự review diff trước khi commit.

## 4. Chạy tự động

Mở một tab terminal để nguyên (tốt nhất trong Herdr hoặc Windows Terminal), chạy:

```bash
bash scripts/autobuild.sh --agent claude --until M0
```

- Mỗi vòng là một phiên agent mới, ngữ cảnh sạch, làm đúng một task rồi commit.
- `--until M0` dừng sau milestone M0. Bỏ cờ này để chạy tới khi gặp cổng hoặc bị chặn.
- Dùng omp thay Claude: `--agent omp`. Hai agent đọc chung `docs/PROGRESS.md`, nên có thể đổi qua lại giữa các lần chạy, ví dụ Claude cho M0–M2 (khó, nhiều UI), omp với model rẻ hơn cho các milestone sau.
- Chọn model: `--model <tên>`.
- Dừng nhẹ nhàng: tạo file `.mkrew-build/STOP` (phiên đang chạy làm xong task rồi mới dừng).

Vòng lặp tự dừng khi: trạng thái khác `RUNNING`, có file STOP, 3 vòng liền không có commit mới, hoặc hết `--max-iter` (mặc định 40).

## 5. Kiểm tra hằng ngày (5 phút)

```bash
bash scripts/status.sh
```

- Xem trạng thái, task đang làm, task tiếp theo, tag milestone đã xong.
- Đọc `docs/HUMAN_CHECKS.md`:
  - **Waiting (blocking):** phải trả lời thì vòng lặp mới chạy tiếp.
  - **To verify:** những thứ agent không tự kiểm được (nhìn giao diện, chạy agent thật). Không chặn vòng lặp.
  - **Questions with assumed defaults:** quyết định agent tự đưa ra. Chỉ cần sửa nếu bạn không đồng ý.
- Log từng phiên trong `.mkrew-build/logs/`, tóm tắt trong `.mkrew-build/loop.log`.

## 6. Cổng M0: quyết định GPUI hay Tauri

Khi xong M0, trạng thái chuyển sang `WAITING_HUMAN`. Việc của bạn:

1. Đọc `docs/decisions/0001-ui-framework.md` (agent ghi lại kết quả đo và đề xuất).
2. Chạy bản thử, ví dụ `cargo run -p spike-term -- claude`. Kiểm tra: màu, khung không vỡ, gõ phím, Ctrl+C, paste nhiều dòng, kéo giãn cửa sổ, cảm giác độ trễ.
3. Ghi quyết định dưới mục tương ứng trong `docs/HUMAN_CHECKS.md` và chuyển nó sang "Done".
4. Trong `docs/PROGRESS.md` đổi `status: WAITING_HUMAN` thành `status: RUNNING`.
5. `git commit -am "chore: M0 decision"` rồi chạy lại vòng lặp.

## 7. Khi bị chặn

- `status: BLOCKED` hoặc task có dấu `[!]`: đọc ghi chú của task đó trong `docs/PROGRESS.md` và log phiên cuối.
- Gợi ý cho agent: viết vào mục "Notes for the next session" (ví dụ "dùng API X thay vì Y", "bỏ qua test Z trên Linux"), đổi `[!]` thành `[ ]`, đặt `status: RUNNING`, commit, chạy lại.
- Lỗi thiếu công cụ hệ thống (Windows SDK, font...): agent ghi vào HUMAN_CHECKS; bạn cài rồi đánh dấu xong.

## 8. Muốn can thiệp có chủ đích

- **Thêm việc:** thêm một dòng task vào hàng đợi trong `docs/PROGRESS.md` theo đúng định dạng (`- [ ] T3.9 ... (spec: §x)`).
- **Đổi thiết kế:** sửa `docs/SPEC.md`, ghi một dòng vào "Notes for the next session" để agent biết. Thay đổi lớn: viết một ADR trong `docs/decisions/`.
- **Đổi thứ tự:** sắp xếp lại các dòng task; agent luôn lấy task `[ ]` đầu tiên.
- **Xem lại code:** `git log --oneline`, mỗi milestone có tag `mN-done`. Không thích kết quả của một task thì `git revert <commit>` và ghi chú lý do cho phiên sau.

## 9. Chi phí và giới hạn

- Mỗi vòng tốn usage của gói Claude/omp. Dùng `--max-iter` và `--until` để chạy theo từng đợt.
- Claude Code báo hết hạn mức thì phiên thất bại, không có commit; sau 3 lần vòng lặp tự dừng. Chờ hạn mức hồi rồi chạy lại.

## 10. An toàn

- Agent chỉ sửa trong repo này, không bao giờ `git push`; các lệnh nguy hiểm bị chặn trong `.claude/settings.json`.
- Chỉ dùng `CLAUDE_FLAGS="--dangerously-skip-permissions"` khi chạy trong máy ảo hoặc container riêng.
