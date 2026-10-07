# Windows setup .exe (SPEC §4): dist\mkrew-<version>-x64-setup.exe from packaging\mkrew.iss.
# The installer asks "for me only" (no admin) or "for all users" (admin).
# Needs Inno Setup 6 (`winget install JRSoftware.InnoSetup --scope user`) and, for the release
# build, fxc.exe from the Windows SDK on PATH or in GPUI_FXC_PATH (gpui compiles its shaders).
# Usage: scripts/package-setup.ps1 [-NoBuild] [-BinDir DIR] [-Out DIR]
# Prints the setup path. Nothing is published.
param(
  [switch]$NoBuild,
  [string]$BinDir,
  [string]$Out
)
$ErrorActionPreference = 'Stop'
$root = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
if (-not $BinDir) { $BinDir = Join-Path $root 'target\release' }
if (-not $Out) { $Out = Join-Path $root 'dist' }

$match = Select-String -Path (Join-Path $root 'Cargo.toml') -Pattern '^version = "(.*)"$' | Select-Object -First 1
if (-not $match) { throw 'package-setup: no version in Cargo.toml' }
$version = $match.Matches[0].Groups[1].Value

$iscc = @(
  (Get-Command ISCC.exe -ErrorAction SilentlyContinue | Select-Object -ExpandProperty Source),
  (Join-Path $env:LOCALAPPDATA 'Programs\Inno Setup 6\ISCC.exe'),
  (Join-Path ${env:ProgramFiles(x86)} 'Inno Setup 6\ISCC.exe'),
  (Join-Path $env:ProgramFiles 'Inno Setup 6\ISCC.exe')
) | Where-Object { $_ -and (Test-Path $_) } | Select-Object -First 1
if (-not $iscc) { throw 'package-setup: ISCC.exe not found; run: winget install JRSoftware.InnoSetup --scope user' }

if (-not $NoBuild) {
  cargo build --release --locked -p mkrew-ui -p mkrew-cli -p mkrew-daemon
  if ($LASTEXITCODE -ne 0) { throw 'package-setup: cargo build failed' }
}
foreach ($bin in 'mkrew', 'mk', 'mkrew-daemon') {
  if (-not (Test-Path (Join-Path $BinDir "$bin.exe"))) { throw "package-setup: missing $bin.exe in $BinDir" }
}

New-Item -ItemType Directory -Force -Path $Out | Out-Null
$outDir = (Resolve-Path $Out).Path
& $iscc /Q `
  "/DMyVersion=$version" `
  "/DBinDir=$((Resolve-Path $BinDir).Path)" `
  "/DAgentsDir=$(Join-Path $root 'agents')" `
  "/DOutDir=$outDir" `
  (Join-Path $root 'packaging\mkrew.iss')
if ($LASTEXITCODE -ne 0) { throw 'package-setup: ISCC failed' }
Write-Output (Join-Path $outDir "mkrew-$version-x64-setup.exe")
