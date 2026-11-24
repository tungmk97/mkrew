<#
.SYNOPSIS
Sandbox Mkrew for UI smoke tests and screenshots (Windows).

.DESCRIPTION
Runs a private copy of the debug binaries against its own data, config and socket,
so the installed Mkrew, its daemon and other sessions' builds are never touched.
Only processes whose exe lives in "<Root>\bin" are ever stopped.

  smoke.ps1 start [-Theme dark|light]   copy the exes, make the sandbox and a test workspace "api", start the UI, return
  smoke.ps1 mk <args...>                run the sandbox's mk (example: smoke.ps1 mk project list)
  smoke.ps1 shot -Out x.png [-Width 1400 -Height 860] [-Mode print|screen] [-AllWindows]
  smoke.ps1 drag -Phase down|move|up -FromX .2 -FromY .5 [-ToX .8 -ToY .5]   positions are fractions of the main window
  -Width / -Height and saved images are in physical pixels (the script is DPI aware).
  smoke.ps1 stop                        stop the sandbox daemon and UI, delete the sandbox

Build first: cargo build -p mkrew-ui -p mkrew-daemon -p mkrew-cli -p mkrew-fakeagent --bins
The fake agent profile is "fake" (mkrew-fakeagent is on the sandbox PATH).
If a PrintWindow image is all black, use -Mode screen (the window must be on top and uncovered).
#>
param(
    [Parameter(Mandatory = $true, Position = 0)][ValidateSet("start", "mk", "shot", "drag", "stop")][string]$Command,
    [ValidateSet("dark", "light")][string]$Theme = "dark",
    [string]$Root = "$env:TEMP\mkrew-sandbox",
    [string]$Target = "",
    [string]$Out,
    [int]$Width = 0,
    [int]$Height = 0,
    [ValidateSet("print", "screen")][string]$Mode = "print",
    [switch]$AllWindows,
    [ValidateSet("down", "move", "up")][string]$Phase,
    [double]$FromX = 0,
    [double]$FromY = 0,
    [double]$ToX = 0,
    [double]$ToY = 0,
    [Parameter(ValueFromRemainingArguments = $true)][string[]]$Rest
)
$ErrorActionPreference = "Stop"
# Windows PowerShell leaves $PSScriptRoot empty while parameter defaults are evaluated.
if (-not $Target) { $Target = Join-Path $PSScriptRoot "..\target\debug" }
$bin = Join-Path $Root "bin"
# Marks a folder this script made, so start and stop never delete anyone else's folder.
$marker = Join-Path $Root ".smoke-ps1"
$env:MKREW_DATA_DIR = Join-Path $Root "data"
$env:MKREW_CONFIG_DIR = Join-Path $Root "config"
$env:MKREW_SOCKET = Join-Path $Root "mkrew.sock"
$env:Path = "$bin;$env:Path"

Add-Type -AssemblyName System.Drawing
Add-Type @"
using System; using System.Collections.Generic; using System.Runtime.InteropServices;
public static class SmokeWin {
  public struct RECT { public int L, T, R, B; }
  delegate bool EnumProc(IntPtr h, IntPtr p);
  [DllImport("user32.dll")] static extern bool EnumWindows(EnumProc f, IntPtr p);
  [DllImport("user32.dll")] static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);
  [DllImport("user32.dll")] static extern bool IsWindowVisible(IntPtr h);
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
  [DllImport("user32.dll")] public static extern bool SetWindowPos(IntPtr h, IntPtr after, int x, int y, int w, int hh, uint f);
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
  [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
  [DllImport("user32.dll")] public static extern bool PrintWindow(IntPtr h, IntPtr dc, uint flags);
  [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
  [DllImport("user32.dll")] public static extern bool SetProcessDPIAware();
  [DllImport("user32.dll")] public static extern void keybd_event(byte vk, byte scan, uint f, UIntPtr e);
  [DllImport("dwmapi.dll")] public static extern int DwmGetWindowAttribute(IntPtr h, int attr, out RECT r, int size);
  [DllImport("user32.dll")] public static extern void mouse_event(uint f, uint dx, uint dy, uint d, UIntPtr e);
  // Visible top-level windows of the given processes, largest first.
  public static List<IntPtr> Windows(int[] pids) {
    var found = new List<IntPtr>();
    EnumWindows((h, p) => {
      uint pid; GetWindowThreadProcessId(h, out pid);
      RECT r;
      if (IsWindowVisible(h) && Array.IndexOf(pids, (int)pid) >= 0 && GetWindowRect(h, out r) && r.R - r.L > 50 && r.B - r.T > 50) found.Add(h);
      return true;
    }, IntPtr.Zero);
    found.Sort((a, b) => Area(b).CompareTo(Area(a)));
    return found;
  }
  static long Area(IntPtr h) { RECT r; GetWindowRect(h, out r); return (long)(r.R - r.L) * (r.B - r.T); }
}
"@
# Physical pixels everywhere: without this, window sizes are scaled but PrintWindow draws every pixel.
[SmokeWin]::SetProcessDPIAware() | Out-Null

function Get-SandboxProcess {
    Get-Process -ErrorAction SilentlyContinue | Where-Object { $_.Path -and $_.Path.StartsWith("$bin\", [StringComparison]::OrdinalIgnoreCase) }
}

function Get-SandboxWindow {
    $pids = [int[]]@(Get-SandboxProcess | Where-Object { $_.ProcessName -eq "mkrew" } | ForEach-Object { $_.Id })
    if ($pids.Count -eq 0) { throw "no sandbox UI running; run: smoke.ps1 start" }
    $windows = [SmokeWin]::Windows($pids)
    if ($windows.Count -eq 0) { throw "the sandbox UI has no visible window" }
    $windows
}

function Get-Rect([IntPtr]$h) {
    $r = New-Object SmokeWin+RECT
    [SmokeWin]::GetWindowRect($h, [ref]$r) | Out-Null
    $r
}

# The visible frame; GetWindowRect also covers the invisible resize border. 9 = DWMWA_EXTENDED_FRAME_BOUNDS.
function Get-Frame([IntPtr]$h) {
    $f = New-Object SmokeWin+RECT
    if ([SmokeWin]::DwmGetWindowAttribute($h, 9, [ref]$f, 16) -ne 0) { return Get-Rect $h }
    $f
}

# Another window can take the foreground between commands; never send input or a screen copy to it.
function Set-Front([IntPtr]$h) {
    # Windows refuses SetForegroundWindow from a background process unless a key was just pressed: tap Alt (0x12; 2 = key up).
    [SmokeWin]::keybd_event(0x12, 0, 0, [UIntPtr]::Zero)
    [SmokeWin]::keybd_event(0x12, 0, 2, [UIntPtr]::Zero)
    [SmokeWin]::SetForegroundWindow($h) | Out-Null
    Start-Sleep -Milliseconds 400
    if ([SmokeWin]::GetForegroundWindow() -ne $h) { throw "another window is in front of the sandbox window; bring Mkrew to the front and retry" }
}

function Invoke-Mk([string[]]$MkArgs) {
    # Windows PowerShell turns redirected stderr of a native exe into errors; mk's stderr is not fatal here.
    $ErrorActionPreference = "Continue"
    & (Join-Path $bin "mk.exe") @MkArgs
}

switch ($Command) {
    "start" {
        if (Get-SandboxProcess) { throw "a sandbox is already running from $bin; run: smoke.ps1 stop" }
        $exes = "mkrew.exe", "mkrew-daemon.exe", "mk.exe", "mkrew-fakeagent.exe"
        foreach ($exe in $exes) {
            if (-not (Test-Path (Join-Path $Target $exe))) { throw "missing $Target\$exe; build first (see the header of this script)" }
        }
        if (Test-Path $Root) {
            if (-not (Test-Path $marker)) { throw "$Root exists and was not made by this script; pass another -Root" }
            Remove-Item $Root -Recurse -Force
        }
        $ws = Join-Path $Root "ws\api"
        New-Item -ItemType Directory -Force $bin, $env:MKREW_DATA_DIR, $env:MKREW_CONFIG_DIR, "$ws\src", "$ws\docs" | Out-Null
        Set-Content $marker ""
        # A private copy: other sessions rebuild target\debug while the smoke runs.
        foreach ($exe in $exes) { Copy-Item (Join-Path $Target $exe) $bin }
        Set-Content (Join-Path $env:MKREW_CONFIG_DIR "settings.toml") "[ui]`ntheme = `"$Theme`"`n"
        # The test workspace: a git repository with a Rust file, a Markdown file, one edit and one new file.
        Set-Content "$ws\src\main.rs" "fn main() {`n    println!(`"hello`");`n}`n"
        Set-Content "$ws\src\lib.rs" "pub fn add(a: u32, b: u32) -> u32 {`n    a + b`n}`n"
        Set-Content "$ws\docs\plan.md" "# Plan`n`n- [ ] one`n- [ ] two`n`n| a | b |`n|---|---|`n| 1 | 2 |`n"
        Set-Content "$ws\README.md" "# api`n`nA small test workspace.`n"
        git -C $ws init -q
        git -C $ws config core.autocrlf false
        git -C $ws add -A
        git -C $ws -c user.email=smoke@example.com -c user.name=smoke commit -q -m "initial"
        Add-Content "$ws\src\main.rs" "`nfn helper() -> u32 {`n    42`n}"
        Set-Content "$ws\src\new.rs" "pub const N: u32 = 1;`n"
        # mk starts the sandbox daemon from its own folder. The workspace is added before the UI opens,
        # because a first workspace added while the UI is open stays on "Opening the workspace...".
        # Start-Process passes no handles: a daemon spawned straight from here would inherit this script's
        # stdout and keep any pipe that reads it open forever. (-Wait would also wait for the daemon.)
        (Start-Process (Join-Path $bin "mk.exe") -ArgumentList "daemon", "start" -WindowStyle Hidden -PassThru).WaitForExit()
        $added = $false
        for ($i = 0; $i -lt 30 -and -not $added; $i++) {
            Invoke-Mk @("project", "add", $ws, "--name", "api") 2>$null | Out-Null
            $added = $LASTEXITCODE -eq 0
            if (-not $added) { Start-Sleep -Seconds 1 }
        }
        if (-not $added) { throw "the sandbox daemon did not answer within 30 s" }
        Start-Process (Join-Path $bin "mkrew.exe") -WorkingDirectory $Root | Out-Null
        Start-Sleep -Seconds 5
        Write-Output "sandbox up: $Root (theme $Theme, workspace api at $ws)"
    }
    "mk" {
        Invoke-Mk $Rest
        exit $LASTEXITCODE
    }
    "shot" {
        if (-not $Out) { throw "shot needs -Out <file.png>" }
        $windows = @(Get-SandboxWindow)
        if (-not $AllWindows) { $windows = @($windows[0]) }
        $n = 0
        foreach ($h in $windows) {
            if ($Width -gt 0 -and $n -eq 0) {
                # 0x0004 = SWP_NOZORDER
                [SmokeWin]::SetWindowPos($h, [IntPtr]::Zero, 40, 40, $Width, $Height, 0x0004) | Out-Null
                Start-Sleep -Milliseconds 800
            }
            if ($Mode -eq "screen") { Set-Front $h }
            $r = Get-Rect $h
            $f = Get-Frame $h
            $full = New-Object System.Drawing.Bitmap ($r.R - $r.L), ($r.B - $r.T)
            $g = [System.Drawing.Graphics]::FromImage($full)
            if ($Mode -eq "print") {
                # 2 = PW_RENDERFULLCONTENT, needed for DirectX windows.
                $dc = $g.GetHdc()
                [SmokeWin]::PrintWindow($h, $dc, 2) | Out-Null
                $g.ReleaseHdc($dc)
            } else {
                $g.CopyFromScreen($r.L, $r.T, 0, 0, $full.Size)
            }
            $g.Dispose()
            $crop = New-Object System.Drawing.Rectangle ($f.L - $r.L), ($f.T - $r.T), ($f.R - $f.L), ($f.B - $f.T)
            $bmp = $full.Clone($crop, $full.PixelFormat)
            $full.Dispose()
            $file = if ($n -eq 0) { $Out } else { [IO.Path]::ChangeExtension($Out, $null).TrimEnd(".") + "-$n.png" }
            $bmp.Save([IO.Path]::GetFullPath($file), [System.Drawing.Imaging.ImageFormat]::Png)
            Write-Output "saved $file ($($bmp.Width) x $($bmp.Height))"
            $bmp.Dispose()
            $n++
        }
    }
    "drag" {
        if (-not $Phase) { throw "drag needs down, move or up" }
        $h = @(Get-SandboxWindow)[0]
        if ($Phase -ne "up") { Set-Front $h }
        $r = Get-Frame $h
        $w = $r.R - $r.L
        $hh = $r.B - $r.T
        function Move-To([double]$fx, [double]$fy) { [SmokeWin]::SetCursorPos([int]($r.L + $fx * $w), [int]($r.T + $fy * $hh)) | Out-Null }
        switch ($Phase) {
            # 0x0002 = left down, 0x0004 = left up
            "down" { Move-To $FromX $FromY; Start-Sleep -Milliseconds 150; [SmokeWin]::mouse_event(0x0002, 0, 0, 0, [UIntPtr]::Zero); Start-Sleep -Milliseconds 150 }
            "move" { 1..20 | ForEach-Object { Move-To ($FromX + ($ToX - $FromX) * $_ / 20) ($FromY + ($ToY - $FromY) * $_ / 20); Start-Sleep -Milliseconds 25 }; Start-Sleep -Milliseconds 300 }
            "up" { [SmokeWin]::mouse_event(0x0004, 0, 0, 0, [UIntPtr]::Zero) }
        }
    }
    "stop" {
        if (-not (Test-Path $marker)) { throw "$Root was not made by this script; nothing stopped" }
        if (Test-Path (Join-Path $bin "mk.exe")) { Invoke-Mk @("daemon", "stop") 2>$null | Out-Null }
        Start-Sleep -Seconds 2
        Get-SandboxProcess | Stop-Process -Force -ErrorAction SilentlyContinue
        Start-Sleep -Seconds 1
        Remove-Item $Root -Recurse -Force
        Write-Output "sandbox stopped and removed: $Root"
    }
}
