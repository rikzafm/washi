# Memory probe. Launches an exe, idles, prints private working set (what Task Manager's
# "Memory (active private working set)" shows), private bytes (commit) and total working set.
# usage: tools/memprobe.ps1 -Label x -Exe path -ExeArgs "--notes 3 ..." [-IdleSec 10] [-AfterMinimizeSec 0]
param([string]$Label, [string]$Exe, [string]$ExeArgs = "", [int]$IdleSec = 10, [int]$AfterMinimizeSec = 0)
function Snap($pid_) {
  $pp = Get-CimInstance Win32_PerfFormattedData_PerfProc_Process -Filter "IDProcess=$pid_"
  $p = Get-Process -Id $pid_
  [pscustomobject]@{ PrivWS_MB = [math]::Round($pp.WorkingSetPrivate/1MB,2); Commit_MB = [math]::Round($p.PrivateMemorySize64/1MB,2); WS_MB = [math]::Round($p.WorkingSet64/1MB,2) }
}
$p = Start-Process $Exe -ArgumentList $ExeArgs -PassThru
Start-Sleep -Seconds $IdleSec
$a = Snap $p.Id
$line = "{0,-44} idle: privWS={1,6} MB commit={2,6} MB WS={3,6} MB" -f $Label, $a.PrivWS_MB, $a.Commit_MB, $a.WS_MB
if ($AfterMinimizeSec -gt 0) {
  Start-Sleep -Seconds $AfterMinimizeSec
  $b = Snap $p.Id
  $line += " | minimized: privWS={0,6} MB commit={1,6} MB WS={2,6} MB" -f $b.PrivWS_MB, $b.Commit_MB, $b.WS_MB
}
$line
$p | Stop-Process -Force
