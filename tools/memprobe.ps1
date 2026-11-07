# Memory probe (IMPLEMENTATION_PLAN 9.4). Seeds a throwaway profile with N notes, launches the
# release build on it, idles, and records the private working set (what Task Manager shows as
# "Memory (active private working set)"), private bytes (commit) and total working set.
#
#   pwsh tools/memprobe.ps1                          # N = 1, 3, 10, appends to docs/PERF.md
#   pwsh tools/memprobe.ps1 -Notes 3 -Enforce        # CI: fail if active N=3 is above the budget
#   pwsh tools/memprobe.ps1 -StickyNotes             # also measure Microsoft Sticky Notes as the baseline
#   pwsh tools/memprobe.ps1 -NoLog                   # do not touch docs/PERF.md
#
# Budget (-Enforce): active private working set with 3 notes <= 15 MB, and not above the Sticky
# Notes baseline when -StickyNotes measured one. The suspended-state budget (<= 5 MB) is measured
# once suspension exists (-Suspended is accepted and recorded as "not built yet" until then).
param(
    [int[]]$Notes = @(1, 3, 10),
    [string]$Exe = 'target/release/washi.exe',
    [string]$Seed = 'target/release/seed.exe',
    [int]$IdleSec = 10,
    [double]$BudgetMb = 15,
    [switch]$Enforce,
    [switch]$StickyNotes,
    [switch]$Suspended,
    [switch]$NoLog,
    [string]$Log = 'docs/PERF.md'
)
$ErrorActionPreference = 'Stop'
Set-Location (Split-Path $PSScriptRoot -Parent)

function Measure-Process([int]$id) {
    $pp = Get-CimInstance Win32_PerfFormattedData_PerfProc_Process -Filter "IDProcess=$id"
    $p = Get-Process -Id $id
    [pscustomobject]@{
        PrivWS = [math]::Round($pp.WorkingSetPrivate / 1MB, 2)
        Commit = [math]::Round($p.PrivateMemorySize64 / 1MB, 2)
        WS     = [math]::Round($p.WorkingSet64 / 1MB, 2)
    }
}

function Wait-Windows([System.Diagnostics.Process]$p, [int]$count) {
    # the note windows are top-level windows of the process
    Add-Type -Namespace Probe -Name Win -MemberDefinition @'
[DllImport("user32.dll")] public static extern bool EnumWindows(EnumProc f, IntPtr l);
[DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);
[DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr h);
public delegate bool EnumProc(IntPtr h, IntPtr l);
'@ -ErrorAction SilentlyContinue
    $deadline = (Get-Date).AddSeconds(30)
    while ((Get-Date) -lt $deadline) {
        $n = 0
        $cb = [Probe.Win+EnumProc] { param($h, $l) $procId = 0; [void][Probe.Win]::GetWindowThreadProcessId($h, [ref]$procId); if ($procId -eq $p.Id -and [Probe.Win]::IsWindowVisible($h)) { $script:cnt++ }; $true }
        $script:cnt = 0
        [void][Probe.Win]::EnumWindows($cb, [IntPtr]::Zero)
        if ($script:cnt -ge $count) { return $script:cnt }
        if ($p.HasExited) { throw "washi exited early (code $($p.ExitCode))" }
        Start-Sleep -Milliseconds 250
    }
    throw "timed out waiting for $count note window(s)"
}

foreach ($f in $Exe, $Seed) { if (-not (Test-Path $f)) { Write-Host "memprobe: $f is missing. Build: cargo build --release -p washi -p snap --bin washi --bin seed"; exit 1 } }
$git = (git rev-parse --short HEAD 2>$null)
$date = Get-Date -Format 'yyyy-MM-dd HH:mm'
$results = @()

foreach ($n in $Notes) {
    $dir = Join-Path ([IO.Path]::GetTempPath()) ("washi-memprobe-" + [guid]::NewGuid().ToString('N').Substring(0, 8))
    & $Seed --data-dir $dir --notes $n | Out-Null
    if ($LASTEXITCODE -ne 0) { throw "seeding $n notes failed" }
    $env:WASHI_DATA_DIR = $dir
    $p = Start-Process (Resolve-Path $Exe) -PassThru
    try {
        $windows = Wait-Windows $p $n
        Start-Sleep -Seconds $IdleSec
        $m = Measure-Process $p.Id
        $results += [pscustomobject]@{ Label = 'washi'; Notes = $n; Windows = $windows; PrivWS = $m.PrivWS; Commit = $m.Commit; WS = $m.WS }
        Write-Host ("washi, {0,2} note(s) ({1} windows) idle {2} s: private WS {3,6} MB   commit {4,6} MB   WS {5,6} MB" -f $n, $windows, $IdleSec, $m.PrivWS, $m.Commit, $m.WS)
    } finally {
        if (-not $p.HasExited) { $p.Kill() }
        Remove-Item Env:WASHI_DATA_DIR -ErrorAction SilentlyContinue
        Start-Sleep -Milliseconds 300
        Remove-Item -Recurse -Force $dir -ErrorAction SilentlyContinue
    }
}

$baseline = $null
if ($StickyNotes) {
    # Sticky Notes keeps whatever notes the user has; we cannot seed it, so this is a side-by-side reference only.
    Start-Process 'explorer.exe' 'shell:AppsFolder\Microsoft.MicrosoftStickyNotes_8wekyb3d8bbwe!App'
    Start-Sleep -Seconds ($IdleSec + 5)
    $sn = Get-Process -Name 'Microsoft.Notes' -ErrorAction SilentlyContinue
    if ($sn) {
        $m = Measure-Process (@($sn)[0].Id)
        $baseline = $m
        $results += [pscustomobject]@{ Label = 'Sticky Notes'; Notes = 'as found'; Windows = ''; PrivWS = $m.PrivWS; Commit = $m.Commit; WS = $m.WS }
        Write-Host ("Sticky Notes (notes as found) idle: private WS {0,6} MB   commit {1,6} MB   WS {2,6} MB" -f $m.PrivWS, $m.Commit, $m.WS)
        $sn | Stop-Process -Force
    } else { Write-Host 'memprobe: Sticky Notes did not start; no baseline recorded' }
}

# verdict against the budget
$fail = @()
$three = $results | Where-Object { $_.Label -eq 'washi' -and $_.Notes -eq 3 } | Select-Object -First 1
if ($three) {
    if ($three.PrivWS -gt $BudgetMb) { $fail += "active 3 notes: $($three.PrivWS) MB is above the $BudgetMb MB budget" }
    if ($baseline -and $three.PrivWS -gt $baseline.PrivWS) { $fail += "active 3 notes: $($three.PrivWS) MB is above Sticky Notes ($($baseline.PrivWS) MB)" }
} elseif ($Enforce) { $fail += 'budget not checked: -Notes must include 3' }
$verdict = if ($fail.Count -eq 0) { 'ok' } else { 'OVER BUDGET' }

if ($Suspended) { Write-Host 'memprobe: -Suspended: suspension is not built yet; nothing measured for the suspended state' }

# perf log
$rows = $results | ForEach-Object { "| $date | $git | $($_.Label) | $($_.Notes) | active | $($_.PrivWS) | $($_.Commit) | $($_.WS) | $(if ($_.Label -eq 'washi' -and $_.Notes -eq 3) { $verdict } else { '' }) |" }
if (-not $NoLog) {
    if (-not (Test-Path $Log)) {
        Set-Content $Log @("# Performance log", "", "Written by tools/memprobe.ps1. Private WS is what Task Manager shows as active private working set.", "Budget: active, 3 notes, private WS <= 15 MB (CI-enforced) and not above Sticky Notes. Suspended 3 notes <= 5 MB (not built yet).", "", "| date | commit | app | notes | state | private WS MB | commit MB | WS MB | vs budget |", "|---|---|---|---|---|---|---|---|---|")
    }
    Add-Content $Log $rows
    Write-Host "memprobe: recorded $($rows.Count) row(s) in $Log"
}
if ($env:GITHUB_STEP_SUMMARY) {
    Add-Content $env:GITHUB_STEP_SUMMARY (@("### memprobe", "", "| date | commit | app | notes | state | private WS MB | commit MB | WS MB | vs budget |", "|---|---|---|---|---|---|---|---|---|") + $rows)
}

foreach ($f in $fail) { Write-Host "memprobe: FAIL: $f" }
if ($fail.Count -gt 0 -and $Enforce) { exit 1 }
if ($fail.Count -eq 0) { Write-Host "memprobe: OK (budget $BudgetMb MB for 3 notes)" }
