# Build và chạy Mkrew trên Windows, Linux, macOS

Tài liệu cho người dùng (MK). Mkrew gồm 3 chương trình phải nằm chung một thư mục:
`mkrew` (giao diện), `mkrew-daemon` (chạy agent), `mk` (CLI, agent hooks gọi nó nên phải có trong `PATH`).

## Trạng thái từng nền tảng

| | Windows | Linux | macOS |
| --- | --- | --- | --- |
| Hỗ trợ theo SPEC §1 | chính (first-class) | có | có |
| CI (`ci.yml`: fmt, clippy, test) | có | có | **không** |
| Gói phát hành (`release.yml`) | MSI | tar.gz | tar.gz |
| Đã chạy thử thật | có | chưa | chưa |
| Hạn chế đã biết | — | xem mục Linux | phím tắt vẫn là `Ctrl` (chưa đổi sang `Cmd`); không phải `.app`, chưa ký số |

Không cross-compile từ Windows sang Linux/macOS được: GPUI cần thư viện đồ họa của Linux, còn
shader Metal chỉ biên dịch được bằng Xcode trên macOS. Build trên đúng hệ điều hành đó, hoặc dùng
GitHub Actions (mục cuối).

## Phát hành bản tải về cho cả 3 nền tảng (chỉ phát triển trên Windows)

Bạn code và test trên Windows như hiện tại. Bản Linux và macOS do GitHub Actions build trên máy
ảo của GitHub (`.github/workflows/release.yml`), nên bạn không cần máy Mac hay Linux.

### Người dùng tải gì

| Nền tảng | File trên trang Releases | Người dùng làm gì |
| --- | --- | --- |
| Windows x64 | `mkrew-<version>-x64.msi` | Bấm đúp để cài, rồi mở "Mkrew" trong Start Menu. `mk` được thêm vào `PATH`. |
| Linux x86_64 | `mkrew-<version>-linux-x86_64.tar.gz` | Giải nén, thêm `bin/` vào `PATH`, chạy `mkrew` (mục Linux bên dưới). |
| macOS Apple Silicon | `mkrew-<version>-darwin-arm64.tar.gz` | Giải nén, gỡ cờ cách ly, thêm `bin/` vào `PATH`, chạy `mkrew` trong Terminal (mục macOS). |

Vì sao không có `.exe` lẻ, `.dmg` hay AppImage:

- App gồm 3 chương trình (`mkrew`, `mkrew-daemon`, `mk`). MSI cài cả ba và đặt `mk` vào `PATH`.
- Trên Linux/macOS, `mkrew` được chạy từ terminal để daemon nhận `PATH` của shell. App mở từ
  Finder hoặc menu Linux thường không thấy `claude`, `codex` cài qua npm/Homebrew.

### Mỗi lần phát hành

1. Một lần duy nhất: tạo repo trên GitHub, `git remote add origin <url>` rồi push nhánh chính
   (vòng lặp agent không bao giờ push). Repo public thì Actions miễn phí. Repo private bị giới hạn
   số phút chạy mỗi tháng, và phút chạy trên macOS bị tính gấp 10.
2. Tăng `version` trong mục `[workspace.package]` của `Cargo.toml` (tên file lấy từ đây), rồi commit.
3. Gắn tag cùng số phiên bản và push:

   ```bash
   git tag v0.2.0
   git push origin <nhánh-chính> v0.2.0
   ```

4. Workflow "Release" build 3 gói song song. Job `publish` tạo một **bản nháp** (draft) GitHub
   Release có đủ 3 file.
5. Vào tab Releases, mở bản nháp, kiểm tra rồi bấm "Publish release". Người dùng tải ở
   `https://github.com/<bạn>/<repo>/releases/latest`.

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
   - **Gói MSI:** cài .NET SDK 8, rồi `dotnet tool install --global wix --version 7.0.0`, rồi
     `pwsh scripts/package.ps1 -AcceptWixEula`. Kết quả nằm ở `dist\mkrew-<version>-x64.msi`. MSI cài
     vào `C:\Program Files\Mkrew\` và thêm `bin\` vào `PATH`.

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
2. Build, chạy thử và đóng gói giống Linux: `cargo build --release --locked -p mkrew-ui -p mkrew-cli -p mkrew-daemon`,
   `./target/release/mkrew`, `bash scripts/package.sh` (ra `dist/mkrew-<version>-darwin-arm64.tar.gz`).
   Giải nén và thêm `bin` vào `PATH` như Linux (`~/.zshrc`).
3. File tar.gz tải về từ máy khác (ví dụ từ GitHub Actions) chưa được ký số, nên Gatekeeper sẽ
   chặn. Gỡ cờ cách ly bằng: `xattr -dr com.apple.quarantine ~/.local/opt/mkrew`.

Hạn chế: chạy `mkrew` từ terminal (không có `.app` trong Dock/Launchpad). Phím tắt vẫn dùng `Ctrl`
như Windows. Thông báo không báo lại khi bạn bấm vào.

## Build cả 3 nền tảng bằng GitHub Actions

`.github/workflows/release.yml` build trên `windows-latest`, `ubuntu-latest` và `macos-latest`.
Mỗi máy chạy đúng script đóng gói của nó và tải gói lên thành artifact của lần chạy. Khi push tag
`v*`, workflow còn tạo một bản nháp GitHub Release có đủ 3 gói (xem mục "Phát hành bản tải về").

1. Push repo lên GitHub (vòng lặp agent không bao giờ push, bạn tự làm).
2. Chạy workflow bằng một trong hai cách: tab Actions → "Release" → "Run workflow", hoặc tạo tag:
   `git tag v0.1.0 && git push origin v0.1.0`.
3. Tải `mkrew-<tag>-<os>` ở cuối trang của lần chạy đó.

WiX 7 yêu cầu chấp nhận EULA OSMF. Workflow đang truyền `-AcceptWixEula`; quyết định này thuộc
về bạn (xem `docs/HUMAN_CHECKS.md`).

## Kiểm tra sau khi cài

```bash
mk --help
mkrew                  # cửa sổ mở, tự khởi động mkrew-daemon
mk daemon status       # daemon đang chạy, in thư mục dữ liệu
```

Kiểm tra code trên máy hiện tại: `bash scripts/verify.sh` (Windows: `pwsh scripts/verify.ps1`).
Cấu hình agent, hooks, gỡ cài đặt: xem `README.md`.
