# The fidelity loop in one command: build, snap, compare against the mockups, check the size
# invariants, print the merged summary, open target/fidelity/index.html.
#
#   pwsh tools/fidelity.ps1                       # headless loop (what CI runs with -NoOpen)
#   pwsh tools/fidelity.ps1 -Only title,tape_pink # show only these groups (rows of other groups are dropped)
#   pwsh tools/fidelity.ps1 -Live                 # also capture the real window (tools/capture.ps1)
#   pwsh tools/fidelity.ps1 -NoOpen               # do not open the report page
#
# Known failures (work not done yet) live in docs/design/fidelity-baseline.txt, one `<report>:<group>`
# per line. They do not fail the run; anything else does, and so does a listed group that passes now:
# delete its line when it does.
param(
    [string[]]$Only = @(),
    [switch]$Live,
    [switch]$NoOpen,
    [string]$Out = 'target/fidelity',
    [double]$Scale = 1.5
)
$ErrorActionPreference = 'Stop'
Set-Location (Split-Path $PSScriptRoot -Parent)
if (Test-Path 'C:\Strawberry\perl\bin') { $env:PATH = 'C:\Strawberry\perl\bin;' + $env:PATH }
$sw = [Diagnostics.Stopwatch]::StartNew()

# paper sizes (logical px) of the two mockups and of the large snap used for the invariants
$hoverPaper = '362.9x432.6' # hover mockup: 362.9 x 432.6
$idlePaper = '362.9x383.7'  # idle mockup: 362.9 x 383.7
$largePaper = '545x576'   # 1.5 x the default
$largeCanvas = '1000x1040'
$baseline = 'docs/design/fidelity-baseline.txt'
$uidiffManifest = 'tools/uidiff/Cargo.toml'

function Run([string]$what, [scriptblock]$cmd) {
    & $cmd
    if ($LASTEXITCODE -ne 0) { Write-Host "fidelity: $what failed (exit $LASTEXITCODE)"; exit 1 }
}

# builds are quiet unless they fail (the linker's openssl PDB warnings are noise)
function Build([string]$what, [string[]]$cargoArgs) {
    $log = & cargo build @cargoArgs 2>&1 | Out-String
    if ($LASTEXITCODE -ne 0) { Write-Host $log; Write-Host "fidelity: build of $what failed"; exit 1 }
}
Build 'snap' @('-p', 'snap')
Build 'uidiff' @('--manifest-path', $uidiffManifest)
$snap = (cargo metadata --no-deps --format-version 1 | ConvertFrom-Json).target_directory + '/debug/snap.exe'
$uidiff =(cargo metadata --no-deps --format-version 1 --manifest-path $uidiffManifest | ConvertFrom-Json).target_directory + '/debug/uidiff.exe'

Remove-Item -Recurse -Force $Out -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force $Out | Out-Null

# 1. snap: the real note component on the desk color
Run 'snap hover' { & $snap --fixture mockup --hover --scale $Scale --paper $hoverPaper --out "$Out/hover.png" | Out-Null }
Run 'snap idle' { & $snap --fixture mockup --scale $Scale --paper $idlePaper --out "$Out/idle.png" | Out-Null }
Run 'snap large' { & $snap --fixture long --hover --scale $Scale --paper $largePaper --canvas $largeCanvas --out "$Out/large.png" | Out-Null }

$only = @(); if ($Only.Count -gt 0) { $only = @('--only', ($Only -join ',')) }
$failed = $false

# 2. compare with the mockups
foreach ($m in 'hover', 'idle') {
    Write-Host "=== compare $m ==="
    & $uidiff --compare "docs/design/reference/mockup-$m.png" "$Out/$m.png" --mockup $m --scale $Scale --baseline $baseline --out "$Out/$m" @only
    if ($LASTEXITCODE -ne 0) { $failed = $true }
}

# 3. invariants: default-size (hover) snap vs the large one
Write-Host '=== size invariants ==='
& $uidiff --invariants "$Out/hover.png" "$Out/large.png" --scale $Scale --baseline $baseline --out "$Out/invariants" @only
if ($LASTEXITCODE -ne 0) { $failed = $true }

# 4. live capture of the real window
if ($Live) {
    Write-Host '=== live capture ==='
    & (Join-Path $PSScriptRoot 'capture.ps1') -Out $Out -Scale $Scale
    if ($LASTEXITCODE -ne 0) { $failed = $true }
}

$index = & $uidiff --index $Out
Write-Host ('fidelity: {0} in {1:N1} s. Report: {2}' -f $(if ($failed) { 'FAILED' } else { 'OK' }), $sw.Elapsed.TotalSeconds, (Resolve-Path $index))
if (-not $NoOpen) { Start-Process (Resolve-Path $index) }
if ($failed) { exit 1 }
