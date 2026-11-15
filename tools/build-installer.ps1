# Builds the Windows installer: target\installer\WashiNote-<version>-setup.exe.
#   pwsh tools/build-installer.ps1            release build of washi, then the installer
#   pwsh tools/build-installer.ps1 -NoBuild   package the target\release\washi.exe that is there
# Needs Inno Setup 6 (winget install JRSoftware.InnoSetup). The script is installer\washi.iss.
param([switch]$NoBuild)
$ErrorActionPreference = 'Stop'
Set-Location (Split-Path $PSScriptRoot -Parent)

$iscc = (Get-Command ISCC.exe -ErrorAction SilentlyContinue).Source
if (-not $iscc) {
    $iscc = @("$env:LOCALAPPDATA\Programs\Inno Setup 6\ISCC.exe", "${env:ProgramFiles(x86)}\Inno Setup 6\ISCC.exe", "$env:ProgramFiles\Inno Setup 6\ISCC.exe") |
        Where-Object { Test-Path $_ } | Select-Object -First 1
}
if (-not $iscc) { Write-Host 'FAIL: Inno Setup 6 not found (winget install JRSoftware.InnoSetup)'; exit 1 }

if (-not $NoBuild) {
    cargo build --release -p washi
    if ($LASTEXITCODE -ne 0) { exit 1 }
}
$targetDir = if ($env:CARGO_TARGET_DIR) { $env:CARGO_TARGET_DIR } else { 'target' }
$exe = Join-Path $targetDir 'release\washi.exe'
if (-not (Test-Path $exe)) { Write-Host "FAIL: $exe not built (cargo build --release -p washi)"; exit 1 }
$exe = (Resolve-Path $exe).Path

if ((Get-Content washi\Cargo.toml -Raw) -notmatch '(?m)^version\s*=\s*"([^"]+)"') { Write-Host 'FAIL: no version in washi\Cargo.toml'; exit 1 }
$version = $Matches[1]

$outDir = Join-Path (Resolve-Path $targetDir).Path 'installer'
New-Item -ItemType Directory -Force $outDir | Out-Null

# The setup program's own icon: an .ico made of the app icon's PNGs (washi\src\assets).
$sizes = 16, 24, 32, 48, 64, 128, 256
$images = $sizes | ForEach-Object { , [IO.File]::ReadAllBytes((Resolve-Path "washi\src\assets\icon_$_.png")) }
$ico = [IO.MemoryStream]::new()
$w = [IO.BinaryWriter]::new($ico)
$w.Write([uint16]0); $w.Write([uint16]1); $w.Write([uint16]$sizes.Count)
$offset = 6 + 16 * $sizes.Count
for ($i = 0; $i -lt $sizes.Count; $i++) {
    $dim = if ($sizes[$i] -ge 256) { 0 } else { $sizes[$i] }
    $w.Write([byte]$dim); $w.Write([byte]$dim); $w.Write([byte]0); $w.Write([byte]0)
    $w.Write([uint16]1); $w.Write([uint16]32)
    $w.Write([uint32]$images[$i].Length); $w.Write([uint32]$offset)
    $offset += $images[$i].Length
}
foreach ($image in $images) { $w.Write($image) }
$icon = Join-Path $outDir 'washi.ico'
[IO.File]::WriteAllBytes($icon, $ico.ToArray())

& $iscc /Qp "/DAppVersion=$version" "/DExePath=$exe" "/DOutputDir=$outDir" "/DSetupIcon=$icon" installer\washi.iss
if ($LASTEXITCODE -ne 0) { Write-Host 'FAIL: Inno Setup could not build the installer'; exit 1 }
$setup = Join-Path $outDir "WashiNote-$version-setup.exe"
Write-Host ("OK: {0} ({1:N2} MB)" -f $setup, ((Get-Item $setup).Length / 1MB))
