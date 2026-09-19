<#
  shot.ps1 — enumerate and capture bard's own windows.

  A frameless always-on-top Tauri panel cannot be screenshotted by the usual
  desktop-capture route (it is composited by DWM and masked by capture tools),
  so this walks the process's top-level windows and pulls each one's pixels
  straight out of DWM with PrintWindow(PW_RENDERFULLCONTENT).

    .\scripts\shot.ps1                      # list windows
    .\scripts\shot.ps1 -Out shots           # capture every visible window
    .\scripts\shot.ps1 -Handle 0x1234 -Out shots\panel.png
    .\scripts\shot.ps1 -Proc handy -Out shots # any other process
#>
param(
  [string]$Proc = "bard",
  [string]$Handle,
  [string]$Out,
  [int]$MinArea = 2000
)

Add-Type -AssemblyName System.Drawing
Add-Type @"
using System;
using System.Text;
using System.Runtime.InteropServices;
public class Shot {
  public delegate bool Cb(IntPtr h, IntPtr l);
  [DllImport("user32.dll")] public static extern bool EnumWindows(Cb cb, IntPtr l);
  [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr h);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern int GetWindowTextW(IntPtr h, StringBuilder s, int m);
  [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr h, out uint p);
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
  [DllImport("user32.dll")] public static extern bool PrintWindow(IntPtr h, IntPtr hdc, uint flags);
  [DllImport("user32.dll")] public static extern bool SetProcessDPIAware();
  [StructLayout(LayoutKind.Sequential)] public struct RECT { public int Left, Top, Right, Bottom; }
}
"@

[Shot]::SetProcessDPIAware() | Out-Null

$pids = @{}
foreach ($p in (Get-Process $Proc -ErrorAction SilentlyContinue)) { $pids[[uint32]$p.Id] = $true }
if ($pids.Count -eq 0) { Write-Error "no '$Proc' process running"; exit 1 }

$found = New-Object System.Collections.ArrayList
$cb = [Shot+Cb]{
  param($h, $l)
  $owner = [uint32]0
  [Shot]::GetWindowThreadProcessId($h, [ref]$owner) | Out-Null
  if ($pids.ContainsKey($owner) -and [Shot]::IsWindowVisible($h)) {
    $sb = New-Object System.Text.StringBuilder 512
    [Shot]::GetWindowTextW($h, $sb, 512) | Out-Null
    $r = New-Object Shot+RECT
    [Shot]::GetWindowRect($h, [ref]$r) | Out-Null
    $w = $r.Right - $r.Left; $ht = $r.Bottom - $r.Top
    if ($w * $ht -ge $MinArea) {
      $null = $found.Add([pscustomobject]@{
        Handle = ("0x{0:X}" -f [int64]$h); Raw = $h
        W = $w; H = $ht; X = $r.Left; Y = $r.Top
        Title = $sb.ToString()
      })
    }
  }
  return $true
}
[Shot]::EnumWindows($cb, [IntPtr]::Zero) | Out-Null

function Save-Window($h, $w, $ht, $path) {
  $bmp = New-Object System.Drawing.Bitmap($w, $ht)
  $g = [System.Drawing.Graphics]::FromImage($bmp)
  $hdc = $g.GetHdc()
  $ok = [Shot]::PrintWindow($h, $hdc, 2)   # PW_RENDERFULLCONTENT
  $g.ReleaseHdc($hdc); $g.Dispose()
  $dir = Split-Path -Parent $path
  if ($dir -and -not (Test-Path $dir)) { New-Item -ItemType Directory -Path $dir -Force | Out-Null }
  $bmp.Save($path, [System.Drawing.Imaging.ImageFormat]::Png)
  $bmp.Dispose()
  return $ok
}

if (-not $Out) {
  $found | Sort-Object { -($_.W * $_.H) } | Format-Table Handle, W, H, X, Y, Title -AutoSize
  exit 0
}

$targets = if ($Handle) { $found | Where-Object { $_.Handle -eq $Handle } } else { $found }
if (-not $targets) { Write-Error "no matching window"; exit 1 }

$single = $Out -match '\.png$'
$i = 0
foreach ($t in $targets) {
  $i++
  $path = if ($single) { $Out } else {
    $slug = ($t.Title -replace '[^\w\-]', '_'); if (-not $slug) { $slug = "win" }
    Join-Path $Out ("{0}_{1}_{2}x{3}.png" -f $i, $slug, $t.W, $t.H)
  }
  $ok = Save-Window $t.Raw $t.W $t.H $path
  Write-Output ("{0}  {1}x{2}  PrintWindow={3}  {4}" -f $t.Handle, $t.W, $t.H, $ok, $path)
}
