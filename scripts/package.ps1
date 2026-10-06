# Windows installer (SPEC §4): dist\mkrew-<version>-x64.msi from packaging\mkrew.wxs.
# Needs WiX 7: `dotnet tool install --global wix --version 7.0.0`. WiX 7 refuses to build until
# its OSMF EULA is accepted: pass -AcceptWixEula (or run `wix eula accept wix7` once).
# Usage: scripts/package.ps1 [-NoBuild] [-BinDir DIR] [-Out DIR] [-AcceptWixEula]
# Prints the installer path. Nothing is published.
param(
  [switch]$NoBuild,
  [string]$BinDir,
  [string]$Out,
  [switch]$AcceptWixEula
)
$ErrorActionPreference = 'Stop'
$root = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
if (-not $BinDir) { $BinDir = Join-Path $root 'target\release' }
if (-not $Out) { $Out = Join-Path $root 'dist' }

$match = Select-String -Path (Join-Path $root 'Cargo.toml') -Pattern '^version = "(.*)"$' | Select-Object -First 1
if (-not $match) { throw 'package: no version in Cargo.toml' }
$version = $match.Matches[0].Groups[1].Value
# MSI versions are numeric a.b.c: drop any pre-release/build suffix.
$msiVersion = ($version -split '[-+]')[0]

if (-not $NoBuild) {
  cargo build --release --locked -p mkrew-ui -p mkrew-cli -p mkrew-daemon
  if ($LASTEXITCODE -ne 0) { throw 'package: cargo build failed' }
}
foreach ($bin in 'mkrew', 'mk', 'mkrew-daemon') {
  if (-not (Test-Path (Join-Path $BinDir "$bin.exe"))) { throw "package: missing $bin.exe in $BinDir" }
}

New-Item -ItemType Directory -Force -Path $Out | Out-Null
$msi = Join-Path (Resolve-Path $Out).Path "mkrew-$version-x64.msi"
# Absolute paths: WiX 7 resolves relative Files/@Include against the .wxs folder.
$wixArgs = @(
  'build', (Join-Path $root 'packaging\mkrew.wxs'), '-arch', 'x64',
  '-d', "Version=$msiVersion",
  '-d', "BinDir=$((Resolve-Path $BinDir).Path)",
  '-d', "AgentsDir=$(Join-Path $root 'agents')",
  '-o', $msi
)
if ($AcceptWixEula) { $wixArgs += @('-acceptEula', 'wix7') }
wix @wixArgs
if ($LASTEXITCODE -ne 0) { throw 'package: wix build failed' }
Write-Output $msi
