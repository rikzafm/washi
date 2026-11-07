# Busy reference background: topmost borderless form with diagonal stripes at (80,80) 760x760.
Add-Type -AssemblyName System.Windows.Forms, System.Drawing
Add-Type @"
using System; using System.Runtime.InteropServices;
public static class D2 { [DllImport("user32.dll")] public static extern bool SetProcessDPIAware(); }
"@
[D2]::SetProcessDPIAware() | Out-Null
$f = New-Object System.Windows.Forms.Form
$f.FormBorderStyle = 'None'; $f.StartPosition = 'Manual'; $f.Location = '80,80'; $f.Size = '760,760'; $f.TopMost = $true; $f.ShowInTaskbar = $false
$f.Add_Paint({ param($s,$e) $b1 = [System.Drawing.Brushes]::SteelBlue; $b2 = [System.Drawing.Brushes]::Goldenrod
  for ($i = 0; $i -lt 160; $i++) { $e.Graphics.FillPolygon($(if ($i % 2) {$b1} else {$b2}), @([System.Drawing.Point]::new($i*10-760,0),[System.Drawing.Point]::new($i*10-750,0),[System.Drawing.Point]::new($i*10,760),[System.Drawing.Point]::new($i*10-10,760))) } })
[System.Windows.Forms.Application]::Run($f)
