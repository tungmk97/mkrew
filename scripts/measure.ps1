# SPEC §15 measurements (T8.9), release builds: UI first frame, idle CPU and memory with six fake
# sessions (opens a real window, needs a desktop session), the six-pane flood stress test, and
# git status on a 10,000-file repo. Results: docs/decisions/0009-non-functional-measurements.md.
# The release UI build needs fxc.exe (Windows SDK) on PATH or GPUI_FXC_PATH.
# Usage: pwsh scripts/measure.ps1   Logs: target/measure/*.log
$ErrorActionPreference = 'Stop'
Set-Location (Join-Path $PSScriptRoot '..')

$out = 'target/measure'
New-Item -ItemType Directory -Force $out | Out-Null

function Step($log, [string[]]$cargoArgs) {
  # Test reports go to stderr; keep them as plain lines.
  $ErrorActionPreference = 'Continue'
  & cargo @cargoArgs 2>&1 | ForEach-Object { "$_" } | Tee-Object -FilePath "$out/$log"
  if ($LASTEXITCODE -ne 0) { Write-Host "measure: FAILED: cargo $cargoArgs"; exit $LASTEXITCODE }
}

# The tests start the daemon and fake agent from the target dir.
cargo build --release -p mkrew-daemon -p mkrew-fakeagent
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

Step 'window.log' @('test', '--release', '-p', 'mkrew-ui', '--test', 'measure', '--', '--ignored', '--nocapture')
Step 'flood.log' @('test', '--release', '-p', 'mkrew-ui', '--test', 'stress', '--', '--nocapture')
Step 'git.log' @('test', '--release', '-p', 'mkrew-daemon', '--lib', 'status_of_a_10000_file_repo_meets_the_budget', '--', '--nocapture')

Write-Host "`n=== §15 results ==="
Get-Content "$out/window.log", "$out/flood.log", "$out/git.log" |
  Select-String -Pattern '^(measure|stress|git\.status)' |
  ForEach-Object { $_.Line }
