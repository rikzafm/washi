# Hover-through-children probe: moves the real cursor outside -> over the child TextInput -> outside,
# and prints the HOVER transitions the Slint app logs (parent TouchArea.has-hover).
param([string]$Exe)
Add-Type @"
using System; using System.Runtime.InteropServices;
public static class Cur { [DllImport("user32.dll")] public static extern bool SetCursorPos(int x,int y); [DllImport("user32.dll")] public static extern bool SetProcessDPIAware(); }
"@
[Cur]::SetProcessDPIAware() | Out-Null
$out = "$env:TEMP\washi_out\hover_$([IO.Path]::GetFileNameWithoutExtension($Exe)).txt"
$p = Start-Process $Exe -ArgumentList "--notes 1 --x 300 --y 200 --no-probe" -PassThru -RedirectStandardOutput $out
Start-Sleep 3
function MoveTo($x,$y,$label){ [Cur]::SetCursorPos($x,$y) | Out-Null; Start-Sleep -Milliseconds 600; "{0,-34} -> log so far: {1}" -f $label, ((Get-Content $out -ErrorAction SilentlyContinue) -join ' | ') }
MoveTo 100 100 "outside window"
MoveTo 300 350 "window transparent margin"
MoveTo (300+215) (200+225) "over child TextInput (paper centre)"
MoveTo (300+215) (200+100) "over child TextInput (upper)"
MoveTo 100 100 "outside again"
$p | Stop-Process -Force

