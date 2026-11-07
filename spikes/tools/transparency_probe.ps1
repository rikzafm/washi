# Transparency probe: does the window show per-pixel alpha over the desktop?
# usage: tools/transparency_probe.ps1 -Exe <path> [-ExtraArgs "..."]
param([string]$Exe, [string]$ExtraArgs = "--notes 1 --x 200 --y 200", [int]$X = 200, [int]$Y = 200)
Add-Type -AssemblyName System.Drawing
Add-Type @"
using System; using System.Runtime.InteropServices;
public static class Dpi { [DllImport("user32.dll")] public static extern bool SetProcessDPIAware(); [DllImport("user32.dll")] public static extern uint GetDpiForSystem(); }
"@
[Dpi]::SetProcessDPIAware() | Out-Null
Add-Type @"
using System; using System.Runtime.InteropServices;
public static class Cap { [DllImport("gdi32.dll")] public static extern bool BitBlt(IntPtr d,int x,int y,int w,int h,IntPtr s,int sx,int sy,int rop); [DllImport("user32.dll")] public static extern IntPtr GetDC(IntPtr h); [DllImport("user32.dll")] public static extern int ReleaseDC(IntPtr h, IntPtr dc); }
"@
$s = [Dpi]::GetDpiForSystem() / 96.0
Add-Type -AssemblyName System.Windows.Forms
$scr = [System.Windows.Forms.Screen]::PrimaryScreen.Bounds
function Grab { $bmp = New-Object System.Drawing.Bitmap $scr.Width, $scr.Height; $g = [System.Drawing.Graphics]::FromImage($bmp); $hd = $g.GetHdc(); $sd = [Cap]::GetDC([IntPtr]::Zero); [Cap]::BitBlt($hd,0,0,$scr.Width,$scr.Height,$sd,0,0,0x00CC0020 -bor 0x40000000) | Out-Null; [Cap]::ReleaseDC([IntPtr]::Zero,$sd) | Out-Null; $g.ReleaseHdc($hd); $g.Dispose(); $bmp }
$bg = Start-Process powershell -ArgumentList "-NoProfile","-File","`"$PSScriptRoot\busy_bg.ps1`"" -PassThru
Start-Sleep -Seconds 3
$before = Grab
$p = Start-Process $Exe -ArgumentList $ExtraArgs -PassThru
Start-Sleep -Seconds 4
$after = Grab
$p | Stop-Process -Force; $bg | Stop-Process -Force
function Px($bmp, $lx, $ly) { $c = $bmp.GetPixel([int]($X + $lx*$s), [int]($Y + $ly*$s)); "{0},{1},{2}" -f $c.R,$c.G,$c.B }
foreach ($pt in @(@("empty margin corner",5,5), @("50% red probe",417,12), @("paper centre",215,225), @("shadow edge (below paper)",215,425))) {
  "{0,-28} before={1,-14} after={2}" -f $pt[0], (Px $before $pt[1] $pt[2]), (Px $after $pt[1] $pt[2])
}
"dpi scale=$s"
$after.Save("$env:TEMP\washi_out\probe_after.png"); $before.Save("$env:TEMP\washi_out\probe_before.png")



