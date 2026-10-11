# Build dist\mkrew-<version>-x64-setup.exe from the current code. Args pass through (e.g. -NoBuild).
pwsh -NoProfile -ExecutionPolicy Bypass -File (Join-Path $PSScriptRoot 'scripts\package-setup.ps1') @args
exit $LASTEXITCODE
