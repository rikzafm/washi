# Performance probe (IMPLEMENTATION_PLAN 9.4, goals G1-G5). Seeds a throwaway profile with N notes,
# launches the release build on it and measures, per N:
#   - active: private working set (what Task Manager shows as "Memory (active private working set)"),
#     private bytes (commit) and total working set after an idle period
#   - with -Suspended: minimizes every note, waits for the 10 s suspend delay plus margin, measures
#     again, then restores one note and times the wake (G4w)
#   - startup (G3): process start to the first turn of the event loop with the notes shown
#   - idle CPU (G4): processor time used over -CpuSec seconds of idle, active (and suspended)
#
#   pwsh tools/memprobe.ps1                              # N = 1, 3, 10, active only, appends to docs/PERF.md
#   pwsh tools/memprobe.ps1 -Suspended                   # also suspended memory, wake time
#   pwsh tools/memprobe.ps1 -Notes 1,3 -Suspended -Enforce -NoLog   # CI
#   pwsh tools/memprobe.ps1 -Startup -CpuSec 60          # startup time (5 launches) and 60 s idle CPU
#   pwsh tools/memprobe.ps1 -NoTrim                      # without the post-start-up working-set trim (what the heap really holds)
#   pwsh tools/memprobe.ps1 -StickyNotes                # also measure Microsoft Sticky Notes as the baseline
#   pwsh tools/memprobe.ps1 -NoLog                       # do not touch docs/PERF.md
#
# Budgets (-Enforce fails when one is broken):
#   active, 3 notes, private WS            <= 15 MB   (-BudgetMb), and not above Sticky Notes when measured
#   suspended, 3 notes, private WS         <= 5 MB    (-SuspendedBudgetMb), needs -Suspended and 3 in -Notes
#   each extra note (least-squares slope)  <= 2 MB    (-PerNoteBudgetMb), needs two or more -Notes values
#   startup (-Startup), median             <= 150 ms; wake (-Suspended), median <= 50 ms
param(
    # strings: `pwsh tools/memprobe.ps1 -Notes 1,3,10` from another shell arrives as the one string "1,3,10"
    [string[]]$Notes = @("1", "3", "10"),
    [string]$Exe = 'target/release/washi.exe',
    [string]$Seed = 'target/release/seed.exe',
    [int]$IdleSec = 10,
    [int]$SuspendWaitSec = 15,
    [double]$BudgetMb = 15,
    [double]$SuspendedBudgetMb = 5,
    [double]$PerNoteBudgetMb = 2,
    [double]$StartupBudgetMs = 150,
    [double]$WakeBudgetMs = 50,
    [int]$CpuSec = 0,
    [int]$StartupRuns = 5,
    [switch]$Enforce,
    [switch]$StickyNotes,
    [switch]$Suspended,
    [switch]$Startup,
    [switch]$NoTrim,
    [switch]$NoLog,
    [string]$Log = 'docs/PERF.md'
)
$ErrorActionPreference = 'Stop'
Set-Location (Split-Path $PSScriptRoot -Parent)
[int[]]$Notes = @(($Notes -join ',') -split '[,\s]+' | Where-Object { $_ } | ForEach-Object { [int]$_ })
# `-Exe`/`-Seed` default to ./target; honour CARGO_TARGET_DIR when the defaults are untouched.
if ($env:CARGO_TARGET_DIR) {
    if (-not $PSBoundParameters.ContainsKey('Exe')) { $Exe = Join-Path $env:CARGO_TARGET_DIR 'release/washi.exe' }
    if (-not $PSBoundParameters.ContainsKey('Seed')) { $Seed = Join-Path $env:CARGO_TARGET_DIR 'release/seed.exe' }
}

Add-Type -Namespace Probe -Name Win -MemberDefinition @'
[DllImport("user32.dll")] public static extern bool EnumWindows(EnumProc f, IntPtr l);
[DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);
[DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr h);
[DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern int GetClassName(IntPtr h, System.Text.StringBuilder s, int n);
[DllImport("user32.dll")] public static extern bool IsIconic(IntPtr h);
[DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr h, int cmd);
[DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
[DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
[DllImport("user32.dll")] public static extern void keybd_event(byte vk, byte scan, uint flags, UIntPtr extra);
public delegate bool EnumProc(IntPtr h, IntPtr l);
'@ -ErrorAction SilentlyContinue

function Get-NoteWindows([int]$procId) {
    $list = New-Object System.Collections.Generic.List[IntPtr]
    $cb = [Probe.Win+EnumProc] {
        param($h, $l)
        $owner = 0
        [void][Probe.Win]::GetWindowThreadProcessId($h, [ref]$owner)
        if ($owner -eq $procId -and [Probe.Win]::IsWindowVisible($h)) {
            # winit keeps one visible 16x16 message window ("Winit Thread Event Target"): not a note
            $cls = New-Object System.Text.StringBuilder 100
            [void][Probe.Win]::GetClassName($h, $cls, 100)
            if ($cls.ToString() -notlike 'Winit Thread*') { $list.Add($h) }
        }
        $true
    }
    [void][Probe.Win]::EnumWindows($cb, [IntPtr]::Zero)
    , $list.ToArray()
}

function Wait-Windows([System.Diagnostics.Process]$p, [int]$count) {
    $deadline = (Get-Date).AddSeconds(30)
    while ((Get-Date) -lt $deadline) {
        $w = Get-NoteWindows $p.Id
        if ($w.Count -ge $count) { return , $w }
        if ($p.HasExited) { throw "washi exited early (code $($p.ExitCode))" }
        Start-Sleep -Milliseconds 100
    }
    throw "timed out waiting for $count note window(s)"
}

function Measure-Process([int]$id) {
    $pp = Get-CimInstance Win32_PerfFormattedData_PerfProc_Process -Filter "IDProcess=$id"
    $p = Get-Process -Id $id
    [pscustomobject]@{
        PrivWS = [math]::Round($pp.WorkingSetPrivate / 1MB, 2)
        Commit = [math]::Round($p.PrivateMemorySize64 / 1MB, 2)
        WS     = [math]::Round($p.WorkingSet64 / 1MB, 2)
    }
}

function Now-Ms { [DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds() }

function Read-Trace([string]$file, [string]$label) {
    if (-not (Test-Path $file)) { return $null }
    foreach ($l in Get-Content $file) {
        $parts = $l -split ' ', 2
        if ($parts[1] -eq $label) { return [int64]$parts[0] }
    }
    $null
}

function Median([double[]]$xs) {
    $s = $xs | Sort-Object
    $s[[math]::Floor(($s.Count - 1) / 2)]
}

# Washi is single-instance per user: a washi.exe started by anyone else (another checkout, the user)
# makes our launch hand off and exit, and makes theirs open a note in ours. Wait for a quiet machine.
function Wait-QuietMachine {
    $deadline = (Get-Date).AddSeconds(120)
    while (@(Get-Process -Name washi -ErrorAction SilentlyContinue).Count -gt 0) {
        if ((Get-Date) -gt $deadline) { throw 'another washi.exe is running (single-instance): close it and rerun' }
        Write-Host 'memprobe: another washi.exe is running; waiting for it to exit'; Start-Sleep -Seconds 5
    }
}

function New-Profile([int]$n) {
    $dir = Join-Path ([IO.Path]::GetTempPath()) ("washi-memprobe-" + [guid]::NewGuid().ToString('N').Substring(0, 8))
    & $Seed --data-dir $dir --notes $n | Out-Null
    if ($LASTEXITCODE -ne 0) { throw "seeding $n notes failed" }
    $dir
}

if ($NoTrim) { $env:WASHI_NO_TRIM = '1' }   # measure without the one-shot working-set trim after start-up
function Stop-Probe($p, $dir) {
    if ($p -and -not $p.HasExited) { $p.Kill(); $p.WaitForExit() }
    Remove-Item Env:WASHI_DATA_DIR, Env:WASHI_TRACE -ErrorAction SilentlyContinue
    Start-Sleep -Milliseconds 300
    Remove-Item -Recurse -Force $dir -ErrorAction SilentlyContinue
}

foreach ($f in $Exe, $Seed) { if (-not (Test-Path $f)) { Write-Host "memprobe: $f is missing. Build: cargo build --release -p washi -p snap --bin washi --bin seed"; exit 1 } }
$git = (git rev-parse --short HEAD 2>$null)
$date = Get-Date -Format 'yyyy-MM-dd HH:mm'
$results = @()   # rows: Label Notes State PrivWS Commit WS
$extra = @()     # free-form lines for the log: startup, wake, cpu
$wakes = @()
$suspendMissing = @()

foreach ($n in $Notes) {
    Wait-QuietMachine
    $dir = New-Profile $n
    $trace = Join-Path $dir 'trace.txt'
    $env:WASHI_DATA_DIR = $dir
    $env:WASHI_TRACE = $trace
    $p = Start-Process (Resolve-Path $Exe) -PassThru
    try {
        $windows = Wait-Windows $p $n
        Start-Sleep -Seconds $IdleSec
        $m = Measure-Process $p.Id
        $results += [pscustomobject]@{ Label = 'washi'; Notes = $n; State = 'active'; PrivWS = $m.PrivWS; Commit = $m.Commit; WS = $m.WS }
        Write-Host ("washi, {0,2} note(s) ({1} windows) active    idle {2} s: private WS {3,6} MB   commit {4,6} MB   WS {5,6} MB" -f $n, $windows.Count, $IdleSec, $m.PrivWS, $m.Commit, $m.WS)
        if ($CpuSec -gt 0 -and $n -eq 3) {
            $c0 = $p.TotalProcessorTime; Start-Sleep -Seconds $CpuSec; $c1 = $p.TotalProcessorTime
            $ms = ($c1 - $c0).TotalMilliseconds
            $extra += "| idle CPU, $n notes, active, Washi in the background (notes not covered) | $([math]::Round($ms, 1)) ms of CPU in $CpuSec s ($([math]::Round($ms / ($CpuSec * 10), 3)) % of one core) |"
            Write-Host ("idle CPU, {0} notes, active, in the background: {1:N1} ms in {2} s" -f $n, $ms, $CpuSec)
            # now in the foreground: the plan's "0% idle CPU" case (no timer of any kind should be running)
            [Probe.Win]::keybd_event(0xA4, 0, 0, [UIntPtr]::Zero); [Probe.Win]::keybd_event(0xA4, 0, 2, [UIntPtr]::Zero)
            [void][Probe.Win]::SetForegroundWindow($windows[0])
            Start-Sleep -Milliseconds 700
            $fgPid = 0; [void][Probe.Win]::GetWindowThreadProcessId([Probe.Win]::GetForegroundWindow(), [ref]$fgPid)
            if ($fgPid -eq $p.Id) {
                $c0 = $p.TotalProcessorTime; Start-Sleep -Seconds $CpuSec; $c1 = $p.TotalProcessorTime
                $ms = ($c1 - $c0).TotalMilliseconds
                $extra += "| idle CPU, $n notes, active, Washi in the foreground | $([math]::Round($ms, 1)) ms of CPU in $CpuSec s |"
                Write-Host ("idle CPU, {0} notes, active, in the foreground: {1:N1} ms in {2} s" -f $n, $ms, $CpuSec)
            } else { Write-Host 'memprobe: could not bring a note to the foreground; foreground idle CPU not measured' }
        }
        if ($Suspended) {
            foreach ($h in $windows) { [void][Probe.Win]::ShowWindow($h, 7) }   # SW_SHOWMINNOACTIVE
            Start-Sleep -Seconds $SuspendWaitSec
            # a change of foreground app can restart the 10 s grace; give a late suspend up to 15 s more
            $late = 0
            while (-not (Read-Trace $trace 'suspend-end') -and $late -lt 15) { Start-Sleep -Seconds 1; $late++ }
            if ($late -gt 0 -and (Read-Trace $trace 'suspend-end')) { Write-Host "memprobe: note: suspended $late s after the nominal $SuspendWaitSec s wait (grace restarted by a window change)"; Start-Sleep -Seconds 1 }
            if (-not (Read-Trace $trace 'suspend-end')) { $suspendMissing += $n; Write-Host "memprobe: washi did not suspend with $n note(s) minimized; trace:"; Get-Content $trace | ForEach-Object { Write-Host "    $_" } }
            $m = Measure-Process $p.Id
            $results += [pscustomobject]@{ Label = 'washi'; Notes = $n; State = 'suspended'; PrivWS = $m.PrivWS; Commit = $m.Commit; WS = $m.WS }
            Write-Host ("washi, {0,2} note(s) ({1} windows) suspended after {2} s: private WS {3,6} MB   commit {4,6} MB   WS {5,6} MB" -f $n, $windows.Count, $SuspendWaitSec, $m.PrivWS, $m.Commit, $m.WS)
            if ($CpuSec -gt 0 -and $n -eq 3) {
                $c0 = $p.TotalProcessorTime; Start-Sleep -Seconds $CpuSec; $c1 = $p.TotalProcessorTime
                $ms = ($c1 - $c0).TotalMilliseconds
                $extra += "| idle CPU, $n notes, suspended | $([math]::Round($ms, 1)) ms of CPU in $CpuSec s |"
                Write-Host ("idle CPU, {0} notes, suspended: {1:N1} ms in {2} s" -f $n, $ms, $CpuSec)
            }
            # wake: restore one note and time until the app's next event-loop turn after it woke
            Remove-Item $trace -ErrorAction SilentlyContinue
            $t0 = Now-Ms
            [void][Probe.Win]::ShowWindow($windows[0], 9)   # SW_RESTORE
            $deadline = (Get-Date).AddSeconds(5)
            $t1 = $null
            while (-not $t1 -and (Get-Date) -lt $deadline) { $t1 = Read-Trace $trace 'wake-next-turn'; if (-not $t1) { Start-Sleep -Milliseconds 2 } }
            if ($t1) { $wakes += ($t1 - $t0); Write-Host ("wake ({0} notes): {1} ms from restore to the first event-loop turn after waking" -f $n, ($t1 - $t0)) }
            else { Write-Host "memprobe: no wake recorded for $n notes" }
        }
    } finally { Stop-Probe $p $dir }
}

$startups = @()
if ($Startup) {
    for ($i = 0; $i -lt $StartupRuns; $i++) {
        Wait-QuietMachine
        $dir = New-Profile 3
        $trace = Join-Path $dir 'trace.txt'
        $env:WASHI_DATA_DIR = $dir
        $env:WASHI_TRACE = $trace
        $t0 = Now-Ms
        $p = Start-Process (Resolve-Path $Exe) -PassThru
        try {
            $deadline = (Get-Date).AddSeconds(20)
            $t1 = $null
            while (-not $t1 -and (Get-Date) -lt $deadline) { $t1 = Read-Trace $trace 'windows-attached'; if (-not $t1) { Start-Sleep -Milliseconds 2 } }
            if (-not $t1) { throw 'no windows-attached mark in the trace' }
            $ps = Read-Trace $trace 'process-start'; $shown = Read-Trace $trace 'notes-shown'
            $startups += ($t1 - $t0)
            Write-Host ("startup run {0}: {1} ms to windows shown (process start +{2} ms, Slint run loop entered +{3} ms, 3 notes)" -f ($i + 1), ($t1 - $t0), ($ps - $t0), ($shown - $t0))
        } finally { Stop-Probe $p $dir }
        Start-Sleep -Milliseconds 500
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
        $results += [pscustomobject]@{ Label = 'Sticky Notes'; Notes = 'as found'; State = 'active'; PrivWS = $m.PrivWS; Commit = $m.Commit; WS = $m.WS }
        Write-Host ("Sticky Notes (notes as found) idle: private WS {0,6} MB   commit {1,6} MB   WS {2,6} MB" -f $m.PrivWS, $m.Commit, $m.WS)
        $sn | Stop-Process -Force
    } else { Write-Host 'memprobe: Sticky Notes did not start (not installed?); no baseline recorded' }
}

# verdicts against the budgets
$fail = @()
function Row($n, $state) { $results | Where-Object { $_.Label -eq 'washi' -and $_.Notes -eq $n -and $_.State -eq $state } | Select-Object -First 1 }
$three = Row 3 'active'
if ($three) {
    if ($three.PrivWS -gt $BudgetMb) { $fail += "active 3 notes: $($three.PrivWS) MB is above the $BudgetMb MB budget" }
    if ($baseline -and $three.PrivWS -gt $baseline.PrivWS) { $fail += "active 3 notes: $($three.PrivWS) MB is above Sticky Notes ($($baseline.PrivWS) MB)" }
} elseif ($Enforce) { $fail += 'budget not checked: -Notes must include 3' }
if ($Suspended) {
    foreach ($n in $suspendMissing) { $fail += "never suspended with $n note(s) minimized" }
    $s3 = Row 3 'suspended'
    if ($s3) { if ($s3.PrivWS -gt $SuspendedBudgetMb) { $fail += "suspended 3 notes: $($s3.PrivWS) MB is above the $SuspendedBudgetMb MB budget" } }
    elseif ($Enforce) { $fail += 'suspended budget not checked: -Notes must include 3' }
    if ($wakes.Count -gt 0) { if ((Median $wakes) -gt $WakeBudgetMs) { $fail += "wake: median $(Median $wakes) ms is above $WakeBudgetMs ms" } }
    elseif ($Enforce) { $fail += 'wake time not measured' }
}
$active = @($results | Where-Object { $_.Label -eq 'washi' -and $_.State -eq 'active' })
$slope = $null
if ($active.Count -ge 2) {
    $mx = ($active | Measure-Object Notes -Average).Average; $my = ($active | Measure-Object PrivWS -Average).Average
    $num = 0.0; $den = 0.0
    foreach ($r in $active) { $num += ($r.Notes - $mx) * ($r.PrivWS - $my); $den += [math]::Pow($r.Notes - $mx, 2) }
    if ($den -gt 0) {
        $slope = [math]::Round($num / $den, 3)
        Write-Host "per extra note (active, least squares over N = $(($active.Notes) -join ',')): $slope MB"
        if ($slope -gt $PerNoteBudgetMb) { $fail += "per extra note: $slope MB is above the $PerNoteBudgetMb MB budget" }
    }
}
if ($Startup -and $startups.Count -gt 0) {
    $med = Median $startups
    $extra += "| startup: process start to all 3 note windows created and shown (the first frame is drawn on the next event-loop turn) (median of $($startups.Count): $($startups -join ', ') ms) | $med ms (budget $StartupBudgetMs ms) |"
    if ($med -gt $StartupBudgetMs) { $fail += "startup: median $med ms is above $StartupBudgetMs ms" }
}
if ($wakes.Count -gt 0) { $extra += "| wake, restore to next event-loop turn (runs: $($wakes -join ', ') ms) | median $(Median $wakes) ms (budget $WakeBudgetMs ms) |" }
if ($slope -ne $null) { $extra += "| per extra note, active, N = $(($active.Notes) -join ',') | $slope MB (budget $PerNoteBudgetMb MB) |" }
$verdict = if ($fail.Count -eq 0) { 'ok' } else { 'OVER BUDGET' }

# perf log
$rows = $results | ForEach-Object {
    $v = ''
    if ($_.Label -eq 'washi' -and $_.Notes -eq 3) { $v = $verdict }
    "| $date | $git | $($_.Label) | $($_.Notes) | $($_.State) | $($_.PrivWS) | $($_.Commit) | $($_.WS) | $v |"
}
if (-not $NoLog) {
    if (-not (Test-Path $Log)) {
        Set-Content $Log @("# Performance log", "", "| date | commit | app | notes | state | private WS MB | commit MB | WS MB | vs budget |", "|---|---|---|---|---|---|---|---|---|")
    }
    Add-Content $Log $rows
    if ($extra.Count -gt 0) { Add-Content $Log (@("", "Probe at $git, $date", "", "| measurement | result |", "|---|---|") + $extra + @("")) }
    Write-Host "memprobe: recorded $($rows.Count) row(s) in $Log"
}
if ($env:GITHUB_STEP_SUMMARY) {
    Add-Content $env:GITHUB_STEP_SUMMARY (@("### memprobe", "", "| date | commit | app | notes | state | private WS MB | commit MB | WS MB | vs budget |", "|---|---|---|---|---|---|---|---|---|") + $rows)
    if ($extra.Count -gt 0) { Add-Content $env:GITHUB_STEP_SUMMARY (@("", "| measurement | result |", "|---|---|") + $extra) }
}

foreach ($f in $fail) { Write-Host "memprobe: FAIL: $f" }
if ($fail.Count -gt 0 -and $Enforce) { exit 1 }
if ($fail.Count -eq 0) { Write-Host "memprobe: OK (active 3 <= $BudgetMb MB, suspended 3 <= $SuspendedBudgetMb MB, <= $PerNoteBudgetMb MB per extra note, where measured)" }
