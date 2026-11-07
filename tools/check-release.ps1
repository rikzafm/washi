# Release-binary checks (run by CI, runnable locally). Requires a release build of washi.
#   1. no GPU renderer in washi's dependency tree
#   2. dev-only tool crates are not in washi's dependency tree
#   3. no system-font scanning: Latin text uses the fonts embedded from fonts/ (ticket 08)
#   4. washi.exe is a GUI-subsystem exe (no console window) with the PerMonitorV2 manifest
#   5. washi.exe is <= 8 MB (G5) and needs no DLL that does not ship with Windows
$ErrorActionPreference = 'Stop'
Set-Location (Split-Path $PSScriptRoot -Parent)
$fail = $false

$tree = cargo tree -p washi -e normal,build --prefix none --format "{p}" 2>&1 | Out-String
# tiny-skia / image are CPU-only and pulled in by Slint itself; only GPU stacks are forbidden.
foreach ($bad in 'femtovg', 'skia-safe', 'i-slint-renderer-skia', 'i-slint-renderer-femtovg', 'glutin\S*', 'glow', 'wgpu\S*', 'ash', 'gl', 'opengl\S*', 'vulkano') {
    if ($tree -match "(?im)^($bad) ") { Write-Host "FAIL: GPU stack crate '$bad' in dependency tree"; $fail = $true }
}
foreach ($bad in 'snap', 'uidiff', 'tempfile') {
    if ($tree -match "(?im)^$bad ") { Write-Host "FAIL: dev-only crate '$bad' in washi's dependency tree"; $fail = $true }
}

$features = cargo tree -p washi -e features --prefix none 2>&1 | Out-String
if ($features -match 'software-renderer-systemfonts') { Write-Host 'FAIL: slint feature software-renderer-systemfonts is on (fonts must be embedded)'; $fail = $true }

$targetDir = if ($env:CARGO_TARGET_DIR) { $env:CARGO_TARGET_DIR } else { 'target' }
$exe = Join-Path $targetDir 'release\washi.exe'
if (-not (Test-Path $exe)) { Write-Host "FAIL: $exe not built (cargo build --release -p washi)"; exit 1 }
$bytes = [IO.File]::ReadAllBytes((Resolve-Path $exe))
$pe = [BitConverter]::ToInt32($bytes, 0x3C)
$subsystem = [BitConverter]::ToUInt16($bytes, $pe + 24 + 68)
if ($subsystem -ne 2) { Write-Host "FAIL: PE subsystem is $subsystem, expected 2 (WINDOWS_GUI)"; $fail = $true }
$text = [Text.Encoding]::ASCII.GetString($bytes)
if ($text -notmatch 'PerMonitorV2') { Write-Host "FAIL: PerMonitorV2 manifest not found in $exe"; $fail = $true }

# 5. size budget (G5): one exe of at most 8 MB, and it imports only DLLs that ship with Windows
$budget = 8MB
if ($bytes.Length -gt $budget) { Write-Host ("FAIL: {0} is {1:N2} MB, above the {2} MB budget (G5)" -f $exe, ($bytes.Length / 1MB), ($budget / 1MB)); $fail = $true }
$dlls = [regex]::Matches($text, '(?i)\b[A-Za-z0-9_\-\.]{1,60}\.dll\b') | ForEach-Object { $_.Value.ToLower() } | Sort-Object -Unique
foreach ($d in $dlls) {
    if ($d -like 'api-ms-win-*' -or $d -like 'ext-ms-win-*') { continue }
    if (-not (Test-Path (Join-Path $env:windir "System32\$d"))) { Write-Host "FAIL: $exe references '$d', which is not a Windows system DLL"; $fail = $true }
}
$stray = Get-ChildItem (Split-Path $exe) -Filter *.dll -ErrorAction SilentlyContinue
if ($stray) { Write-Host ("FAIL: DLLs next to the exe: " + ($stray.Name -join ', ')); $fail = $true }

if ($fail) { exit 1 }
Write-Host ("OK: no GPU renderer, no dev crates, GUI subsystem, PerMonitorV2. exe = {0:N2} MB" -f ($bytes.Length / 1MB))
