# Full verification for Windows PowerShell: fmt check, clippy (-D warnings), tests.
$ErrorActionPreference = 'Stop'
Set-Location (Join-Path $PSScriptRoot '..')

if (-not (Test-Path 'Cargo.toml')) {
  Write-Host 'verify: no Cargo.toml yet (workspace not created) - nothing to check.'
  exit 0
}

function Step($name, [scriptblock]$cmd) {
  Write-Host "`n=== $name ==="
  & $cmd
  if ($LASTEXITCODE -ne 0) { Write-Host "verify: FAILED at $name"; exit $LASTEXITCODE }
}

$env:INSTA_UPDATE = 'no'
Step 'cargo fmt --check' { cargo fmt --all -- --check }
Step 'cargo clippy'      { cargo clippy --workspace --all-targets -- -D warnings }
Step 'cargo test'        { cargo test --workspace }
Write-Host "`nverify: OK"
