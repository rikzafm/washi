# Live-window capture: the real washi.exe on a desk-colored full-screen window, grabbed from the
# screen and compared with the mockups. Catches what the headless renderer cannot see: DWM
# transparency, per-monitor scale, partial-repaint artifacts.
#
#   pwsh tools/capture.ps1 [-Out target/fidelity] [-Scale 1.5] [-Exe target/release/washi.exe]
#
# Needs a desktop session at 150% display scale (Settings > System > Display > Scale). It seeds a
# throwaway profile with the mockup text (tools/snap `seed`), so it never touches your own notes.
# It writes <Out>/live-idle.png and live-hover.png and runs `uidiff --compare` on each.
param(
    [string]$Out = 'target/fidelity',
    [double]$Scale = 1.5,
    [string]$Exe = 'target/release/washi.exe',
    [string]$Seed = 'target/release/seed.exe',
    [string]$Fixture = 'docs/design/fixtures/mockup.md',
    [string]$Baseline = 'docs/design/fidelity-baseline.txt',
    [int[]]$Canvas = @(720, 840)
)
$ErrorActionPreference = 'Stop'
Set-Location (Split-Path $PSScriptRoot -Parent)
Add-Type -AssemblyName System.Windows.Forms, System.Drawing
Add-Type -Namespace Cap -Name Native -MemberDefinition @'
[DllImport("user32.dll")] public static extern bool SetProcessDPIAware();
[DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
[DllImport("user32.dll")] public static extern bool EnumWindows(EnumProc f, IntPtr l);
[DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);
[DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr h);
[DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
[DllImport("user32.dll")] public static extern IntPtr GetDC(IntPtr h);
[DllImport("user32.dll")] public static extern int ReleaseDC(IntPtr h, IntPtr dc);
[DllImport("gdi32.dll")] public static extern int GetDeviceCaps(IntPtr dc, int index);
public delegate bool EnumProc(IntPtr h, IntPtr l);
[StructLayout(LayoutKind.Sequential)] public struct RECT { public int Left, Top, Right, Bottom; }
'@ -ErrorAction SilentlyContinue
[void][Cap.Native]::SetProcessDPIAware()

foreach ($f in $Exe, $Seed) { if (-not (Test-Path $f)) { Write-Host "capture: $f is missing. Build: cargo build --release -p washi -p snap --bin washi --bin seed"; exit 1 } }
$uidiffManifest = 'tools/uidiff/Cargo.toml'
$uidiff = (cargo metadata --no-deps --format-version 1 --manifest-path $uidiffManifest | ConvertFrom-Json).target_directory + '/debug/uidiff.exe'
if (-not (Test-Path $uidiff)) { Write-Host 'capture: build uidiff first (cargo build --manifest-path tools/uidiff/Cargo.toml)'; exit 1 }

$dc = [Cap.Native]::GetDC([IntPtr]::Zero)
$dpi = [Cap.Native]::GetDeviceCaps($dc, 88) # LOGPIXELSX
[void][Cap.Native]::ReleaseDC([IntPtr]::Zero, $dc)
$actual = $dpi / 96.0
if ([math]::Abs($actual - $Scale) -gt 0.01) {
    Write-Host ("capture: WARNING the display scale is {0:P0}, the mockups are at {1:P0}. The numbers below are not comparable; set the display to {1:P0}." -f $actual, $Scale)
}

New-Item -ItemType Directory -Force $Out | Out-Null
$screen = [System.Windows.Forms.Screen]::PrimaryScreen.Bounds

function Pump([int]$ms) {
    $end = (Get-Date).AddMilliseconds($ms)
    while ((Get-Date) -lt $end) { [System.Windows.Forms.Application]::DoEvents(); Start-Sleep -Milliseconds 20 }
}

function Get-WindowRect([int]$procId) {
    $script:found = $null
    $cb = [Cap.Native+EnumProc] { param($h, $l)
        $p = 0; [void][Cap.Native]::GetWindowThreadProcessId($h, [ref]$p)
        if ($p -eq $procId -and [Cap.Native]::IsWindowVisible($h)) { $r = New-Object Cap.Native+RECT; [void][Cap.Native]::GetWindowRect($h, [ref]$r); $script:found = $r }
        $true }
    [void][Cap.Native]::EnumWindows($cb, [IntPtr]::Zero)
    $script:found
}

# the desk
$desk = New-Object System.Windows.Forms.Form
$desk.FormBorderStyle = 'None'
$desk.StartPosition = 'Manual'
$desk.Bounds = $screen
$desk.BackColor = [System.Drawing.ColorTranslator]::FromHtml('#D9DDE1')
$desk.ShowInTaskbar = $false
$desk.TopMost = $false # the note is launched after, so it lands above
$desk.Show()
Pump 500

$dir = Join-Path ([IO.Path]::GetTempPath()) ('washi-capture-' + [guid]::NewGuid().ToString('N').Substring(0, 8))
& $Seed --data-dir $dir --notes 1 --text-file $Fixture | Out-Null
$env:WASHI_DATA_DIR = $dir
$p = Start-Process (Resolve-Path $Exe) -PassThru
$failed = $false
try {
    $r = $null
    for ($i = 0; $i -lt 60 -and -not $r; $i++) { Pump 250; $r = Get-WindowRect $p.Id }
    if (-not $r) { throw 'the note window did not appear' }
    Pump 800
    $r = Get-WindowRect $p.Id
    $cx = [int](($r.Left + $r.Right) / 2); $cy = [int](($r.Top + $r.Bottom) / 2)
    $x0 = [math]::Max($screen.Left, [math]::Min($cx - [int]($Canvas[0] / 2), $screen.Right - $Canvas[0]))
    $y0 = [math]::Max($screen.Top, [math]::Min($cy - [int]($Canvas[1] / 2), $screen.Bottom - $Canvas[1]))

    foreach ($variant in 'idle', 'hover') {
        if ($variant -eq 'hover') { [void][Cap.Native]::SetCursorPos($cx, $cy) } else { [void][Cap.Native]::SetCursorPos($screen.Right - 2, $screen.Bottom - 2) }
        Pump 700
        $bmp = New-Object System.Drawing.Bitmap $Canvas[0], $Canvas[1]
        $g = [System.Drawing.Graphics]::FromImage($bmp)
        $g.CopyFromScreen($x0, $y0, 0, 0, $bmp.Size)
        $g.Dispose()
        $png = Join-Path $Out "live-$variant.png"
        $bmp.Save((Join-Path (Get-Location) $png), [System.Drawing.Imaging.ImageFormat]::Png)
        $bmp.Dispose()
        Write-Host "=== compare live $variant ==="
        & $uidiff --compare "docs/design/reference/mockup-$variant.png" $png --mockup $variant --scale $Scale --baseline $Baseline --out (Join-Path $Out "live-$variant")
        if ($LASTEXITCODE -ne 0) { $failed = $true }
    }
} finally {
    if (-not $p.HasExited) { $p.Kill() }
    Remove-Item Env:WASHI_DATA_DIR -ErrorAction SilentlyContinue
    $desk.Close()
    Start-Sleep -Milliseconds 300
    Remove-Item -Recurse -Force $dir -ErrorAction SilentlyContinue
}
if ($failed) { exit 1 }
