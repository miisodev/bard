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
  [int]$SettleMs = 400
)

Add-Type -AssemblyName System.Windows.Forms
Add-Type @"
using System;
using System.Text;
using System.Runtime.InteropServices;
public class Win32 {
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


$wins = Find-Windows $Proc $false 2000 $Title
if ($wins.Count -eq 0) { Write-Error "no '$Proc' window"; exit 1 }
$h = $wins[0].Raw

[Win32]::ShowWindow($h, 5) | Out-Null
[Win32]::SetForegroundWindow($h) | Out-Null
Start-Sleep -Milliseconds 350

function Send-Click($screenX, $screenY) {
  # SendInput's absolute coordinates are 0..65535 across the whole virtual
  # desktop, not the primary monitor — on a multi-monitor setup the origin can
  # be negative, so both offset and span come from the virtual metrics.
  $vx = [Win32]::GetSystemMetrics([Win32]::SM_XVIRTUALSCREEN)
  $vy = [Win32]::GetSystemMetrics([Win32]::SM_YVIRTUALSCREEN)
  $vw = [Win32]::GetSystemMetrics([Win32]::SM_CXVIRTUALSCREEN)
  $vh = [Win32]::GetSystemMetrics([Win32]::SM_CYVIRTUALSCREEN)

  $nx = [int]((($screenX - $vx) * 65535) / ($vw - 1))
  $ny = [int]((($screenY - $vy) * 65535) / ($vh - 1))

  $mk = { param($flags)
    $i = New-Object Win32+INPUT
    $i.type = 0
    $i.mi = New-Object Win32+MOUSEINPUT
    $i.mi.dx = $nx; $i.mi.dy = $ny; $i.mi.dwFlags = $flags
    return $i
  }
  $size = [System.Runtime.InteropServices.Marshal]::SizeOf([type]([Win32+INPUT]))
  foreach ($f in @(([Win32]::MOVE -bor [Win32]::ABSOLUTE),
                   ([Win32]::LEFTDOWN -bor [Win32]::ABSOLUTE),
                   ([Win32]::LEFTUP -bor [Win32]::ABSOLUTE))) {
    [Win32]::SendInput(1, @((& $mk $f)), $size) | Out-Null
    Start-Sleep -Milliseconds 30
  }
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
  $r = New-Object Win32+RECT
  [Win32]::GetWindowRect($h, [ref]$r) | Out-Null
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
  Write-Output ("click window({0},{1}) -> screen({2},{3})" -f $X, $Y, $sx, $sy)
  Send-Click $sx $sy
  Start-Sleep -Milliseconds $SettleMs
}

if ($Key)  { [System.Windows.Forms.SendKeys]::SendWait((To-SendKeys $Key)); Start-Sleep -Milliseconds $SettleMs }
if ($Type) { [System.Windows.Forms.SendKeys]::SendWait($Type);              Start-Sleep -Milliseconds $SettleMs }
if ($Then) { [System.Windows.Forms.SendKeys]::SendWait((To-SendKeys $Then)); Start-Sleep -Milliseconds $SettleMs }

Write-Output "ok"
