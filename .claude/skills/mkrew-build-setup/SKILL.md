---
name: mkrew-build-setup
description: Build the Windows installer of Mkrew (dist\mkrew-<version>-x64-setup.exe, per-user or all-users install) from the current code, smoke-test it, and report the path. Use after code changes when the user asks to "build exe", "build installer", "build setup", "đóng gói", "build file cài đặt", or wants a fresh .exe to install.
---

# Build the Windows setup .exe

Produces `dist\mkrew-<version>-x64-setup.exe` (Inno Setup 6). On its first page the installer
asks **Install for me only** (no admin; `%LOCALAPPDATA%\Programs\Mkrew`, user PATH) or **Install
for all users** (UAC; `C:\Program Files\Mkrew`, system PATH). Sources: `packaging/mkrew.iss`,
`scripts/package-setup.ps1`. Version comes from `[workspace.package] version` in `Cargo.toml`.

## 1. Prerequisites (check, install only what is missing)

- **Inno Setup 6**: `%LOCALAPPDATA%\Programs\Inno Setup 6\ISCC.exe` (or Program Files). Missing →
  `winget install --id JRSoftware.InnoSetup -e --scope user --silent --accept-package-agreements --accept-source-agreements` (no admin).
- **fxc.exe** (gpui compiles its HLSL shaders in release builds): `where.exe fxc.exe`. On this
  machine it is on the user PATH from the portable MSVC at
  `C:\Users\NguyenTung\msvc\Windows Kits\10\bin\<ver>\x64\fxc.exe`. If not found, set
  `GPUI_FXC_PATH` to that file. Never guess a path: list the folder first.
- Disk: a release build needs a few GB in `target\release`. C: is nearly full on this machine;
  if the link fails with LNK1318/no space, ask the user before running `cargo clean --release`.

## 2. Build

Run from the repo root, ALWAYS in the background (`async`) with a long timeout: the release
build takes 3–10 minutes.

```
pwsh -NoProfile -ExecutionPolicy Bypass -File scripts/package-setup.ps1
```

It runs `cargo build --release --locked -p mkrew-ui -p mkrew-cli -p mkrew-daemon`, then ISCC, and
prints the setup path. `-NoBuild` repackages existing `target\release` binaries only.

Failures:
- `Failed to find fxc.exe` → step 1.
- "Access is denied" on a `target\release\*.exe` → a running release app/daemon holds it. Ask
  the user to close Mkrew, or `mv` the exe aside (`*.locked-old.exe`). Never kill their sessions.
- `--locked` fails → `Cargo.lock` is out of date: tell the user, do not drop `--locked` silently.

## 3. Smoke test (per-user, silent, into a temp folder)

```powershell
$d = Join-Path $env:TEMP "mkrew-setup-smoke"
$setup = (Get-ChildItem dist\mkrew-*-x64-setup.exe | Sort-Object LastWriteTime | Select-Object -Last 1).FullName
$before = [Environment]::GetEnvironmentVariable("Path","User")
(Start-Process $setup -ArgumentList "/VERYSILENT","/CURRENTUSER","/SUPPRESSMSGBOXES","/NORESTART","/DIR=$d" -Wait -PassThru).ExitCode   # 0
& "$d\bin\mk.exe" --version                                                    # mk <version>
(Start-Process "$d\unins000.exe" -ArgumentList "/VERYSILENT","/SUPPRESSMSGBOXES","/NORESTART" -Wait -PassThru).ExitCode   # 0
[Environment]::GetEnvironmentVariable("Path","User") -eq $before              # True
```

Expected: 3 exes in `bin\`, 5 profiles in `agents\`, PATH restored after uninstall. The all-users
mode needs a UAC prompt, so it is the user's manual check (double-click the setup, pick
"Install for all users").

## 4. Report (in Vietnamese)

Full path of the setup file, its size, the version, smoke-test results, and how to install:
double-click → pick "Install for me only" (no admin) or "Install for all users" (admin). The exe
is not code-signed: SmartScreen may say "Windows protected your PC" → "More info" → "Run anyway".
Bump `version` in `Cargo.toml` before building a release the user will hand out; an install of the
same version over itself is fine (it replaces the files). `dist/` is git-ignored; never commit it.
