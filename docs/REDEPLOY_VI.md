# Cài lại bản mới của Mkrew (redeploy) — 2026-10-14

Chạy các lệnh trong **PowerShell** (`pwsh`), tại thư mục repo
`C:\Users\NguyenTung\Workspace\PersonalProject\mkrew-build-kit\mkrew`.

## Vì sao cần làm đủ các bước

- App và daemon đang chạy là bản smoke test trong `%TEMP%\mkrew-setup-smoke3\bin\` (0.1.5, protocol 40).
- Bản cài chính trong `%LOCALAPPDATA%\Programs\Mkrew\bin\` vẫn là 0.1.0 (cài ngày 10/10, protocol 37).
- Hook của agent gọi `mk hook …` qua PATH, nên đang dùng `mk` 0.1.0. Daemon từ chối bản này
  (`client speaks protocol 37, daemon speaks 40`). Từ 11/10 18:11 daemon không nhận được hook nào,
  nên **Needs you** không hoạt động.
- Daemon chạy riêng: đóng cửa sổ app không tắt daemon. Bản app mới không làm việc được với daemon cũ.

## 1. Build bản cài

```powershell
pwsh -NoProfile -ExecutionPolicy Bypass -File scripts/package-setup.ps1
```

- Mất 3–10 phút. Kết quả: `dist\mkrew-<version>-x64-setup.exe`.
- Lỗi `Access is denied` với file `target\release\*.exe`: một app hoặc daemon bản release đang giữ file.
  Làm bước 2 trước, rồi build lại.
- Lỗi `Failed to find fxc.exe`: xem `.claude/skills/mkrew-build-setup/SKILL.md`, mục 1.

## 2. Tắt app và daemon cũ

1. Đóng cửa sổ Mkrew.
2. Dừng daemon bằng `mk` **cùng phiên bản với daemon**:

   ```powershell
   & "$env:TEMP\mkrew-setup-smoke3\bin\mk.exe" daemon stop
   ```

   Lệnh in ra `daemon stopped (pid …)`.

> Shell và lệnh Run sẽ bị tắt. Agent sẽ được mở lại (resume) khi daemon mới chạy.

3. Kiểm tra không còn tiến trình nào của bản cũ (lệnh không được in ra gì):

   ```powershell
   Get-Process mkrew, mkrew-daemon, mk -ErrorAction SilentlyContinue
   ```

## 3. Cài bản mới

1. Chạy file setup mới nhất trong `dist\`:

   ```powershell
   Start-Process (Get-ChildItem dist\mkrew-*-x64-setup.exe | Sort-Object LastWriteTime | Select-Object -Last 1).FullName
   ```

2. Chọn **Install for me only**. Bản mới sẽ ghi đè lên `%LOCALAPPDATA%\Programs\Mkrew`.
3. Nếu SmartScreen báo "Windows protected your PC", bấm **More info**, rồi **Run anyway**.

## 4. Gỡ các bản smoke test và dọn PATH

```powershell
Get-ChildItem "$env:TEMP\mkrew-setup-smoke*\unins000.exe" | % { Start-Process $_ "/VERYSILENT","/SUPPRESSMSGBOXES","/NORESTART" -Wait }
$p = [Environment]::GetEnvironmentVariable("Path","User") -split ';' | ? { $_ -and $_ -notmatch 'mkrew-setup-smoke' }
[Environment]::SetEnvironmentVariable("Path", ($p -join ';'), "User")
```

## 5. Mở terminal mới và kiểm tra `mk`

Đóng terminal cũ rồi mở một terminal mới, để PATH mới có hiệu lực.

```powershell
where.exe mk     # dòng đầu tiên: ...\AppData\Local\Programs\Mkrew\bin\mk.exe
mk --version     # đúng bản vừa build (không phải 0.1.0)
```

Nếu còn dòng `mkrew-setup-smoke…` thì làm lại bước 4.

## 6. Mở Mkrew

1. Mở Mkrew từ Start menu.
2. Kiểm tra daemon:

   ```powershell
   mk daemon status
   ```

   Lần chạy đầu, migration 0019 sẽ đổi tab `run` thành `Run`.

## 7. Cập nhật hook của agent

Chạy lại nhiều lần cũng không sao:

```powershell
mk integration install claude
mk integration install omp
mk integration install codex     # nếu có dùng Codex
```

Sau đó khởi động lại các agent đang mở (đóng pane rồi mở agent mới), để agent nạp hook mới.

## 8. Kiểm tra

- [ ] **Needs you:** cho Claude xin quyền chạy một lệnh. Chip **Needs you** phải lên `1`, dòng của
      agent có dấu `?`.
- [ ] **Hook về đến daemon:** gửi một prompt cho agent. Agent chuyển sang working (có dấu chấm nháy)
      ngay khi gửi, không phải đợi màn hình thay đổi.
- [ ] **Sidebar:** làm theo mục "2026-10-14 Sidebar groups" trong `docs/HUMAN_CHECKS.md` (nhóm
      AGENTS / TERMINAL / RUN, kéo thả, gập nhóm, tab `Run`).

## Nếu Needs you vẫn không lên

```powershell
mk --version
where.exe mk
Get-Content "$env:APPDATA\mkrew\settings.toml"
```

Gửi kết quả cho em. Em sẽ kiểm tra trong DB xem daemon có nhận được hook mới không.
