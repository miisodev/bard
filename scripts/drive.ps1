<#
  drive.ps1 — click and type into bard's own window.

  Companion to shot.ps1. Between them a dev session can reach any surface in
  the app — settings, menus, overlays — without restarting it, which matters
  because killing the app also ends `tauri dev` and its Vite server, and the
  app then has nothing to load from.

  Coordinates are WINDOW-RELATIVE and in real pixels, matching what shot.ps1
  reports and what its captures show: read a position off a capture, pass it
  here. The process is marked DPI-aware so those pixels are physical ones on a
  scaled display, and clicks are sent as absolute SendInput events normalised
  to the virtual desktop, so they land where the capture said.

    .\scripts\drive.ps1 -X 207 -Y 30                 # click the overflow button
    .\scripts\drive.ps1 -X 207 -Y 30 -Then "Escape"  # click, then a key
    .\scripts\drive.ps1 -Key "ctrl+k"                # keys only, no click
    .\scripts\drive.ps1 -Type "hello"                # type into the focused field
    .\scripts\drive.ps1 -X 80 -Y 80 -ToX 300 -ToY 220  # press, move, release

  A drag is sent as a real sequence of moves rather than a jump, because
  pointer-driven canvases sample movement: a single leap from A to B produces
  a two-point stroke and nothing that looks drawn.

  Key names follow SendKeys: Escape, Enter, Tab, Down, "ctrl+k", "shift+Tab".
#>
param(
  [string]$Proc = "bard",
  [int]$X = -1,
  [int]$Y = -1,
  [string]$Key,
  [string]$Type,
  [string]$Then,
  [string]$Title,
  [int]$ToX = -1,
  [int]$ToY = -1,
  [int]$Steps = 14,
  [int]$SettleMs = 400
)

Add-Type -AssemblyName System.Windows.Forms
Add-Type @"
using System;
using System.Text;
using System.Runtime.InteropServices;
public class BardDrive {
  [DllImport("user32.dll")] public static extern bool SetProcessDPIAware();
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
  [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr h, int n);
  [DllImport("user32.dll")] public static extern int GetSystemMetrics(int i);
  [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr h);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern int GetWindowTextW(IntPtr h, StringBuilder s, int m);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern int GetClassNameW(IntPtr h, StringBuilder s, int m);
  [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr h, out uint p);
  [DllImport("user32.dll")] public static extern bool EnumWindows(Cb cb, IntPtr l);
  public delegate bool Cb(IntPtr h, IntPtr l);
  [DllImport("user32.dll")] public static extern uint SendInput(uint n, INPUT[] p, int cb);
  [StructLayout(LayoutKind.Sequential)] public struct RECT { public int Left, Top, Right, Bottom; }
  [StructLayout(LayoutKind.Sequential)] public struct MOUSEINPUT {
    public int dx, dy; public uint mouseData, dwFlags, time; public IntPtr dwExtraInfo;
  }
  [StructLayout(LayoutKind.Sequential)] public struct INPUT {
    public uint type; public MOUSEINPUT mi;
    // The union is sized by its largest member; MOUSEINPUT is it, and we only
    // ever send mouse events from here (keys go through SendKeys).
  }
  public const uint MOVE = 0x0001, LEFTDOWN = 0x0002, LEFTUP = 0x0004, ABSOLUTE = 0x8000;
  public const int SM_CXVIRTUALSCREEN = 78, SM_CYVIRTUALSCREEN = 79;
  public const int SM_XVIRTUALSCREEN = 76, SM_YVIRTUALSCREEN = 77;
}
"@

[BardDrive]::SetProcessDPIAware() | Out-Null

# Finding the app's real windows is not as simple as MainWindowHandle. A Tauri
# process owns a crowd of helpers — the single-instance window ("<id>-siw"), a
# tray_icon_app and a global_hotkey_app (both 1200x618 and one of them visible),
# a Tao thread event target, IME windows — and Windows reports the *single
# instance* helper as MainWindowHandle. In a debug build the largest visible
# window is a helper, not the app.
#
# The app's own windows are exactly those of class "Tauri Window": here, the
# 380x620 panel titled "Bard" and the 260x60 dictation overlay titled
# "Recording". Match on the class and the crowd disappears.
function Find-Windows($procName, $requireVisible, $minArea, $titleLike) {
  $owners = @{}
  foreach ($proc in (Get-Process $procName -ErrorAction SilentlyContinue)) { $owners[[uint32]$proc.Id] = $true }
  if ($owners.Count -eq 0) { return @() }

  $acc = New-Object System.Collections.ArrayList
  $cb = [BardDrive+Cb]{
    param($h, $l)
    $owner = [uint32]0
    [BardDrive]::GetWindowThreadProcessId($h, [ref]$owner) | Out-Null
    if ($owners.ContainsKey($owner)) {
      $cls = New-Object System.Text.StringBuilder 256
      [BardDrive]::GetClassNameW($h, $cls, 256) | Out-Null
      if ($cls.ToString() -eq "Tauri Window") {
        if ((-not $requireVisible) -or [BardDrive]::IsWindowVisible($h)) {
          $sb = New-Object System.Text.StringBuilder 512
          [BardDrive]::GetWindowTextW($h, $sb, 512) | Out-Null
          $title = $sb.ToString()
          if ((-not $titleLike) -or ($title -like $titleLike)) {
            $r = New-Object BardDrive+RECT
            [BardDrive]::GetWindowRect($h, [ref]$r) | Out-Null
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
  [BardDrive]::EnumWindows($cb, [IntPtr]::Zero) | Out-Null
  return @($acc | Sort-Object { -($_.W * $_.H) })
}


$wins = Find-Windows $Proc $false 2000 $Title
if ($wins.Count -eq 0) { Write-Error "no '$Proc' window"; exit 1 }
$h = $wins[0].Raw

[BardDrive]::ShowWindow($h, 5) | Out-Null
[BardDrive]::SetForegroundWindow($h) | Out-Null
Start-Sleep -Milliseconds 350

function New-MouseInput($nx, $ny, $flags) {
  # PowerShell copies a nested value-type field on access, so mutating
  # $input.mi.dx writes to a temporary and the event goes out with every field
  # zeroed — which SendInput happily accepts and reports as success. Build the
  # MOUSEINPUT as its own variable and assign it in whole.
  $mi = New-Object BardDrive+MOUSEINPUT
  $mi.dx = $nx
  $mi.dy = $ny
  $mi.mouseData = 0
  $mi.dwFlags = $flags
  $mi.time = 0
  $mi.dwExtraInfo = [IntPtr]::Zero

  $one = New-Object BardDrive+INPUT
  $one.type = 0
  $one.mi = $mi
  return $one
}

function Send-Raw($screenX, $screenY, $flags) {
  $vx = [BardDrive]::GetSystemMetrics([BardDrive]::SM_XVIRTUALSCREEN)
  $vy = [BardDrive]::GetSystemMetrics([BardDrive]::SM_YVIRTUALSCREEN)
  $vw = [BardDrive]::GetSystemMetrics([BardDrive]::SM_CXVIRTUALSCREEN)
  $vh = [BardDrive]::GetSystemMetrics([BardDrive]::SM_CYVIRTUALSCREEN)
  $nx = [int]((($screenX - $vx) * 65535) / ($vw - 1))
  $ny = [int]((($screenY - $vy) * 65535) / ($vh - 1))
  $one = New-MouseInput $nx $ny $flags
  $size = [System.Runtime.InteropServices.Marshal]::SizeOf([type]([BardDrive+INPUT]))
  [BardDrive]::SendInput(1, @($one), $size) | Out-Null
}

function Send-Click($screenX, $screenY) {
  Send-Raw $screenX $screenY ([BardDrive]::MOVE -bor [BardDrive]::ABSOLUTE)
  Start-Sleep -Milliseconds 40
  Send-Raw $screenX $screenY ([BardDrive]::LEFTDOWN -bor [BardDrive]::ABSOLUTE)
  Start-Sleep -Milliseconds 40
  Send-Raw $screenX $screenY ([BardDrive]::LEFTUP -bor [BardDrive]::ABSOLUTE)
}

function Send-Drag($x1, $y1, $x2, $y2, $steps) {
  Send-Raw $x1 $y1 ([BardDrive]::MOVE -bor [BardDrive]::ABSOLUTE)
  Start-Sleep -Milliseconds 40
  Send-Raw $x1 $y1 ([BardDrive]::LEFTDOWN -bor [BardDrive]::ABSOLUTE)
  for ($i = 1; $i -le $steps; $i++) {
    $t = $i / $steps
    $x = [int]($x1 + ($x2 - $x1) * $t)
    $y = [int]($y1 + ($y2 - $y1) * $t)
    Send-Raw $x $y ([BardDrive]::MOVE -bor [BardDrive]::ABSOLUTE)
    Start-Sleep -Milliseconds 16
  }
  Send-Raw $x2 $y2 ([BardDrive]::LEFTUP -bor [BardDrive]::ABSOLUTE)
}

function To-SendKeys([string]$spec) {
  $mods = ""
  $parts = $spec -split '\+'
  $key = $parts[-1]
  foreach ($m in $parts[0..([Math]::Max(0, $parts.Count - 2))]) {
    switch ($m.ToLower()) {
      "ctrl"  { $mods += "^" }
      "shift" { $mods += "+" }
      "alt"   { $mods += "%" }
    }
  }
  if ($parts.Count -eq 1) { $mods = "" }
  $named = @{
    escape = "{ESC}"; esc = "{ESC}"; enter = "{ENTER}"; tab = "{TAB}";
    up = "{UP}"; down = "{DOWN}"; left = "{LEFT}"; right = "{RIGHT}";
    space = " "; delete = "{DEL}"; backspace = "{BACKSPACE}"; home = "{HOME}"; end = "{END}"
  }
  $k = if ($named.ContainsKey($key.ToLower())) { $named[$key.ToLower()] } else { $key }
  return "$mods$k"
}

if ($X -ge 0 -and $Y -ge 0) {
  $r = New-Object BardDrive+RECT
  [BardDrive]::GetWindowRect($h, [ref]$r) | Out-Null
  $w = $r.Right - $r.Left; $ht = $r.Bottom - $r.Top

  # A hidden window reports a zero rect, and clicking relative to it would send
  # a real click into whatever happens to be at that spot on the desktop.
  # Refuse rather than guess.
  if ($w -le 0 -or $ht -le 0) {
    Write-Error "window is hidden (rect ${w}x${ht}) - show the panel first"
    exit 2
  }
  if ($X -ge $w -or $Y -ge $ht) {
    Write-Error "($X,$Y) is outside the ${w}x${ht} window"
    exit 2
  }

  $sx = $r.Left + $X
  $sy = $r.Top + $Y
  if ($ToX -ge 0 -and $ToY -ge 0) {
    Write-Output ("drag window({0},{1})->({2},{3})" -f $X, $Y, $ToX, $ToY)
    Send-Drag $sx $sy ($r.Left + $ToX) ($r.Top + $ToY) $Steps
  } else {
    Write-Output ("click window({0},{1}) -> screen({2},{3})" -f $X, $Y, $sx, $sy)
    Send-Click $sx $sy
  }
  Start-Sleep -Milliseconds $SettleMs
}

if ($Key)  { [System.Windows.Forms.SendKeys]::SendWait((To-SendKeys $Key)); Start-Sleep -Milliseconds $SettleMs }
if ($Type) { [System.Windows.Forms.SendKeys]::SendWait($Type);              Start-Sleep -Milliseconds $SettleMs }
if ($Then) { [System.Windows.Forms.SendKeys]::SendWait((To-SendKeys $Then)); Start-Sleep -Milliseconds $SettleMs }

Write-Output "ok"
