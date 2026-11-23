# Windows only: puts the pinned ConPTY pair (conpty.dll + OpenConsole.exe, from Microsoft's
# Microsoft.Windows.Console.ConPTY NuGet package, MIT) next to the Mkrew binaries. portable-pty
# loads a conpty.dll found next to the exe, and that newer ConPTY passes omp's TSP escape
# sequences through (ADR 0107).
# Usage: scripts/fetch-conpty.ps1 [-Dest DIR]   (default: target\debug and target\release)
# The package is cached in %LOCALAPPDATA%\mkrew\conpty (the same file scripts/fetch-conpty.sh
# uses); the package and both files are checked by SHA-256. Safe to run again. Does nothing off
# Windows.
param([string]$Dest)
$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
if ($env:OS -ne 'Windows_NT') { Write-Host 'fetch-conpty: not Windows, nothing to do'; exit 0 }

$version = '1.24.260710001'
$url = "https://www.nuget.org/api/v2/package/Microsoft.Windows.Console.ConPTY/$version"
$nupkgSha = '175640566a3b59c4b132070ee96c2c77e5ab7edd2e92732a5eb3610bbf63d90e'
$files = @(
  @{ Name = 'conpty.dll'; Entry = 'runtimes/win-x64/native/conpty.dll'; Sha = '39fba2713e2495117b1591ae8c32a3b904bea7aa66069cf7815e2844c76d75d8' },
  @{ Name = 'OpenConsole.exe'; Entry = 'build/native/runtimes/x64/OpenConsole.exe'; Sha = 'b7fd936c2668b87b9ecf7b3366dc6568afc1c6f981874cba3e955a1c35cf8160' }
)
function Sha($path) { (Get-FileHash -Algorithm SHA256 -LiteralPath $path).Hash.ToLower() }

$root = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$dests = if ($Dest) { @($Dest) } else { @((Join-Path $root 'target\debug'), (Join-Path $root 'target\release')) }
$base = if ($env:LOCALAPPDATA) { $env:LOCALAPPDATA } else { Join-Path $HOME '.cache' }
$cache = Join-Path $base 'mkrew\conpty'
$nupkg = Join-Path $cache "$version.nupkg"

if ((Test-Path -LiteralPath $nupkg) -and (Sha $nupkg) -eq $nupkgSha) {
  Write-Host "fetch-conpty: cache hit $nupkg"
} else {
  Write-Host "fetch-conpty: downloading $url"
  New-Item -ItemType Directory -Force -Path $cache | Out-Null
  $part = "$nupkg.part"
  Invoke-WebRequest -Uri $url -OutFile $part -UseBasicParsing
  if ((Sha $part) -ne $nupkgSha) {
    Remove-Item -LiteralPath $part -Force
    throw 'fetch-conpty: the downloaded package does not match the pinned SHA-256'
  }
  Move-Item -LiteralPath $part -Destination $nupkg -Force
}

Add-Type -AssemblyName System.IO.Compression.FileSystem
$tmp = Join-Path ([IO.Path]::GetTempPath()) ('mkrew-conpty-' + [guid]::NewGuid())
New-Item -ItemType Directory -Path $tmp | Out-Null
try {
  $zip = [IO.Compression.ZipFile]::OpenRead($nupkg)
  try {
    foreach ($f in $files) {
      $entry = $zip.GetEntry($f.Entry)
      if (-not $entry) { throw "fetch-conpty: $($f.Entry) is not in the package" }
      $out = Join-Path $tmp $f.Name
      [IO.Compression.ZipFileExtensions]::ExtractToFile($entry, $out, $true)
      if ((Sha $out) -ne $f.Sha) { throw "fetch-conpty: $($f.Entry) in the package has an unexpected SHA-256" }
    }
  } finally { $zip.Dispose() }

  foreach ($d in $dests) {
    New-Item -ItemType Directory -Force -Path $d | Out-Null
    $d = (Resolve-Path -LiteralPath $d).Path
    $current = $true
    foreach ($f in $files) {
      $p = Join-Path $d $f.Name
      if (-not (Test-Path -LiteralPath $p) -or (Sha $p) -ne $f.Sha) { $current = $false }
    }
    if ($current) { Write-Host "fetch-conpty: $d up to date"; continue }
    foreach ($f in $files) {
      try {
        Copy-Item -LiteralPath (Join-Path $tmp $f.Name) -Destination (Join-Path $d $f.Name) -Force
      } catch {
        throw "fetch-conpty: cannot write $d\$($f.Name) (a daemon or session running from there keeps it open; try: mk daemon stop): $_"
      }
    }
    Write-Host "fetch-conpty: copied to $d"
  }
} finally {
  Remove-Item -LiteralPath $tmp -Recurse -Force -ErrorAction SilentlyContinue
}
