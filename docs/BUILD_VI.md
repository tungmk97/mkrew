# Build và chạy Mkrew trên Windows, Linux, macOS

Tài liệu cho người dùng (MK). Mkrew gồm 3 chương trình phải nằm chung một thư mục:
`mkrew` (giao diện), `mkrew-daemon` (chạy agent), `mk` (CLI, agent hooks gọi nó nên phải có trong `PATH`).

## Trạng thái từng nền tảng

| | Windows | Linux | macOS |
| --- | --- | --- | --- |
| Hỗ trợ theo SPEC §1 | chính (first-class) | có | có |
| CI (`ci.yml`: fmt, clippy, test) | có | có | **không** |
| Gói phát hành (`release.yml`) | setup .exe (Inno, cài không cần admin) | tar.gz | `Mkrew.app` (.zip) |
| Đã chạy thử thật | có | chưa | chưa |
| Hạn chế đã biết | — | xem mục Linux | phím tắt vẫn là `Ctrl` (chưa đổi sang `Cmd`); `.app` chỉ ký ad-hoc |

Không cross-compile từ Windows sang Linux/macOS được: GPUI cần thư viện đồ họa của Linux, còn
shader Metal chỉ biên dịch được bằng Xcode trên macOS. Build trên đúng hệ điều hành đó, hoặc dùng
GitHub Actions (mục cuối).

## Phát hành bản tải về cho cả 3 nền tảng (chỉ phát triển trên Windows)

Bạn code và test trên Windows như hiện tại. Bản Linux và macOS do GitHub Actions build trên máy
ảo của GitHub (`.github/workflows/release.yml`), nên bạn không cần máy Mac hay Linux.

### Người dùng tải gì

| Nền tảng | File trên trang Releases | Người dùng làm gì |
| --- | --- | --- |
| Windows x64 | `mkrew-<version>-x64-setup.exe` | Bấm đúp, chọn "Install for me only" (không cần admin), rồi mở "Mkrew" trong Start Menu. `mk` được thêm vào `PATH`. |
| Linux x86_64 | `mkrew-<version>-linux-x86_64.tar.gz` | Giải nén, thêm `bin/` vào `PATH`, chạy `mkrew` (mục Linux bên dưới). |
| macOS Apple Silicon | `mkrew-<version>-macos-arm64.zip` | Giải nén, kéo `Mkrew.app` vào `/Applications`, lần đầu chuột phải → Open (mục macOS). |

Vì sao không có `.exe` lẻ, `.dmg` hay AppImage:

- App gồm 3 chương trình (`mkrew`, `mkrew-daemon`, `mk`). Setup cài cả ba và đặt `mk` vào `PATH`.
- Bản đã cài tự kiểm tra bản mới trên GitHub (lúc mở và mỗi 6 giờ). Có bản mới thì title bar hiện
  nút mũi tên; bấm vào, Mkrew tự cài, mở lại, các agent tiếp tục hội thoại cũ (ADR 0071).
- Trên Linux/macOS, `mkrew` được chạy từ terminal để daemon nhận `PATH` của shell. App mở từ
  Finder hoặc menu Linux thường không thấy `claude`, `codex` cài qua npm/Homebrew.

### Mỗi lần phát hành

1. Một lần duy nhất: tạo repo trên GitHub, `git remote add origin <url>` rồi push nhánh chính
   (vòng lặp agent không bao giờ push). Repo public thì Actions miễn phí. Repo private bị giới hạn
   số phút chạy mỗi tháng, và phút chạy trên macOS bị tính gấp 10.
2. Tăng `version` trong mục `[workspace.package]` của `Cargo.toml` (tên file lấy từ đây), chạy
   `cargo build` một lần để `Cargo.lock` cập nhật theo, rồi commit cả hai (CI build bằng `--locked`).
3. Gắn tag cùng số phiên bản và push:

   ```bash
   git tag v0.2.0
   git push origin <nhánh-chính> v0.2.0
   ```

4. Workflow "Release" kiểm tra tag khớp `version`, build 3 gói song song, rồi job `publish` tạo
   GitHub Release **công khai ngay** (không còn bản nháp). Người dùng tải ở
   `https://github.com/<bạn>/<repo>/releases/latest`; các máy đang chạy Mkrew thấy bản mới trong
   vòng 6 giờ (hoặc bấm "Global: Check for updates" trong palette).

Muốn build thử mà không tạo release: vào Actions → "Release" → "Run workflow", rồi tải các artifact
ở cuối trang của lần chạy đó. Artifact là file zip, phải đăng nhập GitHub mới tải được và bị xóa
sau 90 ngày, nên chỉ dùng để thử.

Trước khi phát hành lần đầu: bản Linux và macOS chưa ai chạy thật. Tải artifact về một máy
Linux/Mac (hoặc nhờ người có máy) và làm theo mục "Kiểm tra sau khi cài" ở cuối file.

### Giới hạn hiện tại

- **Chưa ký số.**
  - Windows: SmartScreen báo "Windows protected your PC", người dùng bấm "More info" rồi "Run anyway".
  - macOS: Gatekeeper chặn. Gỡ bằng `xattr -dr com.apple.quarantine <thư mục>`, hoặc vào System
    Settings → Privacy & Security → "Open Anyway".
  - Để bỏ hết các cảnh báo này cần chứng chỉ code signing cho Windows và tài khoản Apple Developer
    (99 USD/năm) kèm notarization. Phần này chưa làm.
- **macOS chỉ có bản Apple Silicon:** runner `macos-latest` là máy arm64. Muốn có bản cho Mac
  Intel thì thêm một runner Intel vào `matrix.os`.
- **Linux:** máy người dùng cần driver Vulkan và `libxkbcommon-x11`. Phần lớn bản desktop đã có
  sẵn hai thứ này.

## Yêu cầu chung

- Git và Rust qua [rustup](https://rustup.rs). `rust-toolchain.toml` tự chọn bản `stable` kèm
  `rustfmt`, `clippy`.
- Các agent muốn dùng (`claude`, `codex`, `omp`) đã cài và đăng nhập. Tùy chọn: Zed với lệnh `zed`
  trong `PATH` (cho "Open in Zed").

## Windows

1. Cài rustup (toolchain MSVC) và Visual Studio Build Tools với gói "Desktop development with C++".
2. Build và chạy thử:

   ```powershell
   cargo build --release --locked -p mkrew-ui -p mkrew-cli -p mkrew-daemon
   .\target\release\mkrew.exe
   ```

3. Cài vào máy, chọn một trong hai cách:
   - **Từ source:** `cargo install --locked --path crates/mkrew-ui`, rồi tương tự với
     `crates/mkrew-cli` và `crates/mkrew-daemon`. Cả ba vào `~\.cargo\bin` (đã có trong `PATH`).
   - **Gói setup:** cài Inno Setup 6 (`winget install JRSoftware.InnoSetup --scope user`), rồi
     `pwsh scripts/package-setup.ps1`. Kết quả nằm ở `dist\mkrew-<version>-x64-setup.exe`.
     "Install for me only" cài vào `%LOCALAPPDATA%\Programs\Mkrew\` (không cần admin).

## Linux

1. Cài trình biên dịch C (cho SQLite đóng gói sẵn) và thư viện GPUI cần (danh sách theo
   [script/linux của Zed](https://github.com/zed-industries/zed/blob/main/script/linux)):

   ```bash
   # Ubuntu / Debian
   sudo apt-get install -y build-essential pkg-config libxkbcommon-x11-dev libwayland-dev \
     libx11-xcb-dev libfontconfig-dev libvulkan1 mesa-vulkan-drivers xdg-desktop-portal
   # Fedora
   sudo dnf install -y gcc pkgconf libxkbcommon-x11-devel wayland-devel libxcb-devel \
     fontconfig-devel vulkan-loader mesa-vulkan-drivers xdg-desktop-portal
   # Arch
   sudo pacman -S --needed base-devel libxkbcommon-x11 wayland libxcb fontconfig \
     vulkan-icd-loader xdg-desktop-portal
   curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
   ```

2. Build và chạy thử: `cargo build --release --locked -p mkrew-ui -p mkrew-cli -p mkrew-daemon`,
   sau đó `./target/release/mkrew`.
3. Đóng gói và cài:

   ```bash
   bash scripts/package.sh              # in ra dist/mkrew-<version>-linux-x86_64.tar.gz
   tar -xzf dist/mkrew-*-linux-*.tar.gz -C /tmp
   mkdir -p ~/.local/opt && mv /tmp/mkrew-*-linux-* ~/.local/opt/mkrew
   echo 'export PATH="$HOME/.local/opt/mkrew/bin:$PATH"' >> ~/.bashrc   # hoặc ~/.zshrc
   ```

Lưu ý khi chạy:

- Cần môi trường đồ họa X11 hoặc Wayland và driver Vulkan (GPUI vẽ bằng wgpu). Không có màn
  hình (SSH thuần) thì chỉ dùng được `mk` + `mkrew-daemon`.
- Hộp chọn thư mục ("Add workspace") đi qua `xdg-desktop-portal`. Nếu thiếu portal, thêm workspace
  bằng `mk project add <path>`.
- Font terminal mặc định là JetBrains Mono; nếu không có thì dùng DejaVu Sans Mono.
- "Keep awake" cần `systemd-inhibit`. Nếu không có, daemon ghi log và chạy tiếp.
- Repo lớn có thể chạm giới hạn inotify. Tăng giới hạn bằng
  `sudo sysctl fs.inotify.max_user_watches=524288`.

## macOS

1. Cài **Xcode đầy đủ** từ App Store (Command Line Tools thôi là không đủ: GPUI biên dịch shader
   bằng `xcrun metal`):

   ```bash
   sudo xcode-select --switch /Applications/Xcode.app/Contents/Developer
   sudo xcodebuild -license accept
   xcrun -sdk macosx metal --version      # phải chạy được
   curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
   ```

   Nếu `xcrun` báo không tìm thấy `metal` (Xcode mới tách Metal Toolchain ra riêng), chạy
   `xcodebuild -downloadComponent MetalToolchain`.
2. Build và chạy thử: `cargo build --release --locked -p mkrew-ui -p mkrew-cli -p mkrew-daemon`,
   `./target/release/mkrew`. Đóng gói app: `bash scripts/package.sh --app` (ra
   `dist/mkrew-<version>-macos-arm64.zip` chứa `Mkrew.app` ký ad-hoc).
3. Giải nén, kéo `Mkrew.app` vào `/Applications`. Bản tải từ máy khác chưa ký Developer ID nên lần
   đầu Gatekeeper chặn: chuột phải → Open → Open, hoặc
   `xattr -dr com.apple.quarantine /Applications/Mkrew.app`. Các lần cập nhật trong app không bị chặn.

Hạn chế: mở từ Finder thì daemon không có `PATH` của shell; nếu không thấy `claude`/`codex`, chạy
`mk daemon stop` rồi mở `/Applications/Mkrew.app/Contents/MacOS/mkrew &` từ terminal. Phím tắt vẫn
dùng `Ctrl` như Windows. Thông báo không báo lại khi bạn bấm vào.

## Build cả 3 nền tảng bằng GitHub Actions

`.github/workflows/release.yml` build trên `windows-latest`, `ubuntu-latest` và `macos-latest`.
Mỗi máy chạy đúng script đóng gói của nó và tải gói lên thành artifact của lần chạy. Khi push tag
`v*`, workflow kiểm tra tag khớp `version` rồi publish GitHub Release có đủ 3 gói.

1. Push repo lên GitHub (vòng lặp agent không bao giờ push, bạn tự làm).
2. Chạy workflow bằng một trong hai cách: tab Actions → "Release" → "Run workflow", hoặc tạo tag:
   `git tag v0.1.0 && git push origin v0.1.0`.
3. Tải `mkrew-<tag>-<os>` ở cuối trang của lần chạy đó.

Windows dùng Inno Setup (cài bằng `choco` trên runner); không còn MSI/WiX.

## Kiểm tra sau khi cài

```bash
mk --help
mkrew                  # cửa sổ mở, tự khởi động mkrew-daemon
mk daemon status       # daemon đang chạy, in thư mục dữ liệu
```

Kiểm tra code trên máy hiện tại: `bash scripts/verify.sh` (Windows: `pwsh scripts/verify.ps1`).
Cấu hình agent, hooks, gỡ cài đặt: xem `README.md`.
