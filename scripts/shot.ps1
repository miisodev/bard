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
    .\scripts\shot.ps1 -Out g.png -Screen      # composited, for Glass

  -Screen grabs the screen region the window occupies instead of asking the
  window to draw itself. PrintWindow renders the window in isolation, so an
  acrylic or mica backdrop — which is composited from what lies *behind* the
  window — comes back empty. Use -Screen whenever the backdrop is the thing
  being checked; the window must be unobscured and frontmost.
#>
param(
  [string]$Proc = "bard",
  [string]$Handle,
  [string]$Out,
  [switch]$Screen,
  [switch]$Show,
  [string]$Title,
  [int]$MinArea = 2000
)

Add-Type -AssemblyName System.Drawing
Add-Type @"
using System;
using System.Text;
using System.Runtime.InteropServices;
public class Win32 {
  public delegate bool Cb(IntPtr h, IntPtr l);
  [DllImport("user32.dll")] public static extern bool EnumWindows(Cb cb, IntPtr l);
  [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr h);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern int GetWindowTextW(IntPtr h, StringBuilder s, int m);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern int GetClassNameW(IntPtr h, StringBuilder s, int m);
  [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr h, out uint p);
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
  [DllImport("user32.dll")] public static extern bool PrintWindow(IntPtr h, IntPtr hdc, uint flags);
  [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr h, int n);
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
  [DllImport("user32.dll")] public static extern bool SetProcessDPIAware();
  [StructLayout(LayoutKind.Sequential)] public struct RECT { public int Left, Top, Right, Bottom; }
}
"@

[Win32]::SetProcessDPIAware() | Out-Null

# Finding the app's real windows is not as simple as MainWindowHandle. A Tauri
# process owns a crowd of helpers — the single-instance window ("<id>-siw"), a
# tray_icon_app and a global_hotkey_app (both 1200x618 and one of them visible),
# a Tao thread event target, IME windows — and Windows reports the *single
# instance* helper as MainWindowHandle. In a debug build the largest visible
# window is a helper, not the app.
#
# The app's own windows are exactly those of class "Tauri Window": here, the
# 380x620 panel titled "Bard" and the 320x96 dictation overlay titled
# "Recording". Match on the class and the crowd disappears.
function Find-Windows($procName, $requireVisible, $minArea, $titleLike) {
  $owners = @{}
  foreach ($proc in (Get-Process $procName -ErrorAction SilentlyContinue)) { $owners[[uint32]$proc.Id] = $true }
  if ($owners.Count -eq 0) { return @() }

  $acc = New-Object System.Collections.ArrayList
  $cb = [Win32+Cb]{
    param($h, $l)
    $owner = [uint32]0
    [Win32]::GetWindowThreadProcessId($h, [ref]$owner) | Out-Null
    if ($owners.ContainsKey($owner)) {
      $cls = New-Object System.Text.StringBuilder 256
      [Win32]::GetClassNameW($h, $cls, 256) | Out-Null
      if ($cls.ToString() -eq "Tauri Window") {
        if ((-not $requireVisible) -or [Win32]::IsWindowVisible($h)) {
          $sb = New-Object System.Text.StringBuilder 512
          [Win32]::GetWindowTextW($h, $sb, 512) | Out-Null
          $title = $sb.ToString()
          if ((-not $titleLike) -or ($title -like $titleLike)) {
            $r = New-Object Win32+RECT
            [Win32]::GetWindowRect($h, [ref]$r) | Out-Null
            $w = $r.Right - $r.Left; $ht = $r.Bottom - $r.Top
            if ($w * $ht -ge $minArea) {
              $null = $acc.Add([pscustomobject]@{
                Handle = ("0x{0:X}" -f [int64]$h); Raw = $h
                W = $w; H = $ht; X = $r.Left; Y = $r.Top; Title = $title
              })
            }
          }
        }
      }
    }
    return $true
  }
  [Win32]::EnumWindows($cb, [IntPtr]::Zero) | Out-Null
  return @($acc | Sort-Object { -($_.W * $_.H) })
}


if ($Show) {
  # Ask a hidden panel back on screen before looking for it.
  $any = Find-Windows $Proc $false $MinArea $Title
  if ($any.Count -gt 0) {
    [Win32]::ShowWindow($any[0].Raw, 5) | Out-Null
    [Win32]::SetForegroundWindow($any[0].Raw) | Out-Null
    Start-Sleep -Milliseconds 450
  }
}

$found = Find-Windows $Proc $true $MinArea $Title
if ($found.Count -eq 0) {
  Write-Error "no visible '$Proc' window (try -Show)"
  exit 1
}

function Save-Window($h, $w, $ht, $path, $x, $y, $useScreen) {
  $bmp = New-Object System.Drawing.Bitmap($w, $ht)
  $g = [System.Drawing.Graphics]::FromImage($bmp)
  if ($useScreen) {
    [Win32]::ShowWindow($h, 5) | Out-Null
    [Win32]::SetForegroundWindow($h) | Out-Null
    Start-Sleep -Milliseconds 350
    $g.CopyFromScreen($x, $y, 0, 0, (New-Object System.Drawing.Size($w, $ht)))
    $ok = $true
    $g.Dispose()
  } else {
    $hdc = $g.GetHdc()
    $ok = [Win32]::PrintWindow($h, $hdc, 2)   # PW_RENDERFULLCONTENT
    $g.ReleaseHdc($hdc); $g.Dispose()
  }
  $dir = Split-Path -Parent $path
  if ($dir -and -not (Test-Path $dir)) { New-Item -ItemType Directory -Path $dir -Force | Out-Null }
  $bmp.Save($path, [System.Drawing.Imaging.ImageFormat]::Png)
  $bmp.Dispose()
  return $ok
}

if (-not $Out) {
  $found | Format-Table Handle, W, H, X, Y, Title -AutoSize
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
  $ok = Save-Window $t.Raw $t.W $t.H $path $t.X $t.Y $Screen
  Write-Output ("{0}  {1}x{2}  PrintWindow={3}  {4}" -f $t.Handle, $t.W, $t.H, $ok, $path)
}
