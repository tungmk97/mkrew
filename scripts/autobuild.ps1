<#
.SYNOPSIS
  Mkrew autonomous build loop for Windows PowerShell (pwsh 7+ recommended).
  Runs one fresh agent session per task until docs/PROGRESS.md status is not RUNNING,
  a STOP file appears, or no progress is made in 3 iterations.

.EXAMPLE
  pwsh scripts/autobuild.ps1 -Agent claude
  pwsh scripts/autobuild.ps1 -Agent omp -Until M2
  New-Item .mkrew-build/STOP   # graceful stop after the current session
#>
param(
  [ValidateSet('claude', 'omp')] [string]$Agent = 'claude',
  [int]$MaxIter = 40,
  [string]$Until = '',
  [int]$TimeoutMin = 90,
  [int]$MaxTurns = 200,
  [string]$Model = ''
)
$ErrorActionPreference = 'Continue'
Set-Location (Join-Path $PSScriptRoot '..')

$StateDir = '.mkrew-build'; $LogDir = Join-Path $StateDir 'logs'; $LoopLog = Join-Path $StateDir 'loop.log'
New-Item -ItemType Directory -Force -Path $LogDir | Out-Null
Remove-Item -ErrorAction SilentlyContinue (Join-Path $StateDir 'STOP')

function Log($msg) { $line = "$(Get-Date -Format 'yyyy-MM-dd HH:mm:ss') $msg"; Write-Host $line; Add-Content -Path $LoopLog -Value $line }
function Field($name) { (Select-String -Path 'docs/PROGRESS.md' -Pattern "^${name}:\s*(\S+)" | Select-Object -First 1).Matches[0].Groups[1].Value }
function MilestoneNum($m) { [int]($m -replace '\D', '') }
function Head { $h = git rev-parse HEAD 2>$null; if ($LASTEXITCODE -ne 0) { 'none' } else { $h } }

if (-not (Get-Command $Agent -ErrorAction SilentlyContinue)) { Write-Host "agent command '$Agent' not found in PATH"; exit 2 }
if (-not (Test-Path '.git')) { git init -q; git add -A; git commit -qm 'chore: bootstrap mkrew build kit'; Log 'initialized git repository' }

$Prompt = @'
You are running one headless iteration of the Mkrew autonomous build.
Follow docs/BUILD_LOOP.md exactly (skill: mkrew-next): read docs/PROGRESS.md, do exactly ONE task
end to end (implement, test, verify, review, update PROGRESS, commit), then stop.
There is no human available: never ask questions; record assumptions in docs/HUMAN_CHECKS.md.
'@

function Build-Args {
  if ($Agent -eq 'claude') {
    # Skip user-level settings (global hooks/plugins) so headless runs only use this repo's config.
    $a = @('-p', $Prompt, '--setting-sources', 'project,local', '--permission-mode', 'acceptEdits', '--max-turns', "$MaxTurns")
    if ($Model) { $a += @('--model', $Model) }
    if ($env:CLAUDE_FLAGS) { $a += $env:CLAUDE_FLAGS -split ' ' }
  } else {
    # Verify flags with `omp --help` on your version; override via $env:OMP_FLAGS.
    $flags = if ($env:OMP_FLAGS) { $env:OMP_FLAGS -split ' ' } else { @('--auto-approve') }
    $a = @('-p') + $flags
    if ($Model) { $a += @('--model', $Model) }
    $a += $Prompt
  }
  return $a
}

Log "loop start: agent=$Agent max_iter=$MaxIter until=$(if ($Until) { $Until } else { 'none' })"
$noProgress = 0; $iter = 0
while ($iter -lt $MaxIter) {
  $status = Field 'status'; $milestone = Field 'milestone'
  if (Test-Path (Join-Path $StateDir 'STOP')) { Log 'STOP file found - stopping.'; break }
  if ($status -ne 'RUNNING') { Log "status is $status - stopping. See docs/HUMAN_CHECKS.md"; break }
  if ($Until -and (MilestoneNum $milestone) -gt (MilestoneNum $Until)) { Log "reached milestone $milestone (past -Until $Until) - stopping."; break }

  $iter++
  $before = Head
  $logFile = Join-Path $LogDir ("iter-{0}-{1}.log" -f (Get-Date -Format 'yyyyMMdd-HHmmss'), $iter)
  Log "iteration $iter ($milestone) -> $logFile"

  $cmd = (Get-Command $Agent).Source
  $job = Start-Job -ScriptBlock { param($c, $a, $l) & $c @a *> $l; $LASTEXITCODE } -ArgumentList $cmd, (Build-Args), (Resolve-Path -LiteralPath $LogDir | Join-Path -ChildPath (Split-Path $logFile -Leaf))
  if (-not (Wait-Job $job -Timeout ($TimeoutMin * 60))) { Stop-Job $job; Log "  timeout after $TimeoutMin min" }
  $rc = Receive-Job $job -ErrorAction SilentlyContinue | Select-Object -Last 1
  Remove-Job $job -Force

  $after = Head
  if ($before -ne $after) {
    $noProgress = 0
    $count = (git log --oneline "$before..$after" 2>$null | Measure-Object -Line).Lines
    Log "  rc=$rc  new commits: $count  last: $(git log -1 --pretty=%s)"
  } else {
    $noProgress++
    Log "  rc=$rc  no new commit ($noProgress/3)"
    if ($noProgress -ge 3) { Log "no progress in 3 iterations - stopping. Check the last logs in $LogDir"; break }
  }
  Start-Sleep -Seconds 3
}
Log "loop end after $iter iteration(s); status=$(Field 'status') milestone=$(Field 'milestone')"
[console]::beep(880, 300)
if (Get-Command bash -ErrorAction SilentlyContinue) { bash scripts/status.sh }
