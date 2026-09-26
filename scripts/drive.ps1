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

  -Tap drives the raw-input gesture map, which -Key cannot reach. SendKeys emits
  a generic VK_SHIFT, so it cannot say *which* Shift and it cannot express a
  double-tap of one modifier; between them that leaves Right Shift x2, Left Alt
  x2, Right Alt x2 and the Ctrl gesture unreachable. -Tap sends real scan codes
  with KEYEVENTF_SCANCODE (and KEYEVENTF_EXTENDEDKEY for the right-hand keys), so
  the low-level hook sees the same events a person's fingers produce:

    .\scripts\drive.ps1 -Tap lshift                     # capture
    .\scripts\drive.ps1 -Tap rshift                     # toggle the panel
    .\scripts\drive.ps1 -Tap rshift -Then 1             # paste timeline item 1
    .\scripts\drive.ps1 -Tap lalt                       # Draw
    .\scripts\drive.ps1 -Tap ralt                       # Settings
    .\scripts\drive.ps1 -Tap ctrl                       # dictate, tap to lock
    .\scripts\drive.ps1 -Tap ctrl -TapSecondHoldMs 400  # dictate, push-to-talk
    .\scripts\drive.ps1 -Tap lshift,lalt,ralt           # several, comma-separated
    .\scripts\drive.ps1 -Tap rshift -TapDryRun          # print, do not send

  -Tap needs no window: the gestures work from any application, which is the
  point of them, so -Tap alone runs even when bard's panel is hidden.
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
  [int]$SettleMs = 400,
  # One or more modifier gestures to double-tap: lshift/rshift/lalt/ralt/lctrl/rctrl
  [string[]]$Tap,
  [int]$TapGapMs = 120,
  [int]$TapHoldMs = 40,
  # The second press only, so push-to-talk (Ctrl held past 300ms) is reachable.
  [int]$TapSecondHoldMs = -1,
  # Print the exact events and timings instead of sending them. A machine with no
  # interactive input desktop refuses every SendInput — including the mouse path
  # that has always worked — so this is the only way to audit a gesture run on
  # a build agent or over a remote shell.
  [switch]$TapDryRun
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
  // A real union, not two sibling fields. INPUT must stay 40 bytes on x64 —
  // SendInput is told sizeof(INPUT) and parses the payload from that — and a
  // KEYBDINPUT sitting after MOUSEINPUT would make it 64 and quietly misalign
  // every event. MOUSEINPUT and KEYBDINPUT both start at offset 8 because that
  // is where the union begins: DWORD type, then padding to the union's
  // 8-byte alignment.
  [StructLayout(LayoutKind.Explicit)]
  public struct INPUT {
    [FieldOffset(0)] public uint type;
    [FieldOffset(8)] public MOUSEINPUT mi;
    [FieldOffset(8)] public KEYBDINPUT ki;
  }
  [StructLayout(LayoutKind.Sequential)] public struct KEYBDINPUT {
    public ushort wVk, wScan; public uint dwFlags, time; public IntPtr dwExtraInfo;
  }
  public const uint MOVE = 0x0001, LEFTDOWN = 0x0002, LEFTUP = 0x0004, ABSOLUTE = 0x8000;
  public const uint KEYBOARD = 1, EXTENDEDKEY = 0x0001, KEYUP = 0x0002, SCANCODE = 0x0008;
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


# A gesture is the one thing that must work with no window of bard's on screen:
# it is what the gestures are *for*, and requiring a visible panel to test them
# would test the wrong thing. Everything else still needs a window.
$gestureOnly = $Tap -and $X -lt 0 -and -not $Key -and -not $Type -and -not $Then

$wins = Find-Windows $Proc $false 2000 $Title
if ($wins.Count -eq 0 -and -not $gestureOnly) { Write-Error "no '$Proc' window"; exit 1 }

if ($wins.Count -gt 0) {
  $h = $wins[0].Raw
  if (-not $gestureOnly) {
    [BardDrive]::ShowWindow($h, 5) | Out-Null
    [BardDrive]::SetForegroundWindow($h) | Out-Null
    Start-Sleep -Milliseconds 350
  }
}

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

# Scan code set 1, which is what KEYEVENTF_SCANCODE speaks. The right-hand keys
# share a scan code with their left-hand twin and are told apart only by
# KEYEVENTF_EXTENDEDKEY, which is the whole reason -Key cannot reach them: a
# generic VK_SHIFT carries no side at all.
$SCANS = @{
  "lshift" = @{ Make = 0x2A; Break = 0xAA; Extended = $false; Name = "Left Shift" }
  "rshift" = @{ Make = 0x36; Break = 0xB6; Extended = $true;  Name = "Right Shift" }
  "lalt"   = @{ Make = 0x38; Break = 0xB8; Extended = $false; Name = "Left Alt" }
  "ralt"   = @{ Make = 0x38; Break = 0xB8; Extended = $true;  Name = "Right Alt" }
  "lctrl"  = @{ Make = 0x1D; Break = 0x9D; Extended = $false; Name = "Left Ctrl" }
  "rctrl"  = @{ Make = 0x1D; Break = 0x9D; Extended = $true;  Name = "Right Ctrl" }
}
# Aliases, so "lshift" and "shiftleft" both work. Bare "shift", "alt" and "ctrl"
# are deliberately NOT accepted: a gesture has to name a side, and quietly
# defaulting to the left one lets a test pass for the wrong key.
foreach ($pair in @(
  @("shiftleft", "lshift"), @("shiftright", "rshift"), @("leftshift", "lshift"), @("rightshift", "rshift"),
  @("altleft", "lalt"), @("altright", "ralt"), @("leftalt", "lalt"), @("rightalt", "ralt"),
  @("controlleft", "lctrl"), @("controlright", "rctrl"),
  @("leftctrl", "lctrl"), @("rightctrl", "rctrl"),
  # "ctrl" alone IS accepted, unlike "shift" and "alt": gesture.rs maps
  # LeftControl and RightControl to the same dictation action, so there is no
  # side to get wrong. It sends Left Ctrl; "rctrl" is there when the right key
  # is what is being tested.
  @("ctrl", "lctrl"), @("control", "lctrl")
)) { $SCANS[$pair[0]] = $SCANS[$pair[1]] }

# gesture.rs: TAP_WINDOW is 400ms from the first tap's *release* to the second
# tap's press, and TAP_HOLD_LIMIT is 500ms for a tap that may not be held like a
# normal modifier. Refuse to build a sequence the state machine will reject,
# rather than let a mistimed parameter look like a broken gesture.
if ($TapGapMs -ge 400) { Write-Error "-TapGapMs must be under 400 (gesture.rs TAP_WINDOW)"; exit 2 }
if ($TapHoldMs -ge 500) { Write-Error "-TapHoldMs must be under 500 (gesture.rs TAP_HOLD_LIMIT)"; exit 2 }
$secondHold = if ($TapSecondHoldMs -ge 0) { $TapSecondHoldMs } else { $TapHoldMs }
if ($secondHold -ge 300) {
  Write-Output "note: the second press is held ${secondHold}ms, past DICTATION_HOLD_LIMIT (300ms), so Ctrl is push-to-talk rather than tap-to-lock"
}

function Send-Scan([int]$code, [bool]$extended, [bool]$up) {
  # Same trap as New-MouseInput: assign the nested struct in whole, or every
  # field goes out zeroed and SendInput still reports success.
  $ki = New-Object BardDrive+KEYBDINPUT
  $ki.wVk = 0
  $ki.wScan = [uint16]$code
  $ki.dwFlags = [BardDrive]::SCANCODE
  if ($extended) { $ki.dwFlags = $ki.dwFlags -bor [BardDrive]::EXTENDEDKEY }
  if ($up) { $ki.dwFlags = $ki.dwFlags -bor [BardDrive]::KEYUP }
  $ki.time = 0
  $ki.dwExtraInfo = [IntPtr]::Zero

  $one = New-Object BardDrive+INPUT
  $one.type = [BardDrive]::KEYBOARD
  # mi and ki overlap at offset 8, so clear the unused half by assigning it
  # first: a KEYBDINPUT only fills 24 of the union's 32 bytes, and the mouse
  # bytes behind it are whatever was on the stack.
  $one.mi = New-Object BardDrive+MOUSEINPUT
  $one.ki = $ki
  $size = [System.Runtime.InteropServices.Marshal]::SizeOf([type]([BardDrive+INPUT]))
  if ($TapDryRun) {
    return "scan=0x{0:X2} flags=0x{1:X} ({2}{3}{4}) size={5}" -f $code, $ki.dwFlags, `
      $(if ($ki.dwFlags -band [BardDrive]::SCANCODE) { "SCANCODE " } else { "" }), `
      $(if ($extended) { "EXTENDEDKEY " } else { "" }), `
      $(if ($up) { "KEYUP" } else { "MAKE" }), $size
  }
  return [BardDrive]::SendInput(1, @($one), $size)
}

function Send-Tap($spec) {
  $key = $spec.ToLower().Trim()
  if (-not $SCANS.ContainsKey($key)) {
    Write-Error "unknown -Tap '$spec'. Use one of: $(($SCANS.Keys | Sort-Object) -join ', ')"
    exit 2
  }
  $scan = $SCANS[$key]
  $first = Send-Scan $scan.Make $scan.Extended $false
  Start-Sleep -Milliseconds $TapHoldMs
  $firstUp = Send-Scan $scan.Break $scan.Extended $true
  Start-Sleep -Milliseconds $TapGapMs
  $second = Send-Scan $scan.Make $scan.Extended $false
  Start-Sleep -Milliseconds $secondHold
  $secondUp = Send-Scan $scan.Break $scan.Extended $true
  if (-not $TapDryRun) {
    if (($first, $firstUp, $second, $secondUp) | Where-Object { $_ -ne 1 }) {
      throw "SendInput refused one of the four $($scan.Name) events (sent $first/$firstUp/$second/$secondUp)."
    }
  }
  Write-Output ("tap $($scan.Name) x2  (hold {0}ms, gap {1}ms, second hold {2}ms{3})" -f `
    $TapHoldMs, $TapGapMs, $secondHold, $(if ($TapDryRun) { ", dry run" } else { "" }))
  if ($TapDryRun) {
    Write-Output ("    press 1  " + $first)
    Write-Output ("    release  " + $firstUp)
    Write-Output ("    press 2  " + $second)
    Write-Output ("    release  " + $secondUp)
  }
}

if ($Tap) { foreach ($spec in $Tap) { Send-Tap $spec }; Start-Sleep -Milliseconds $SettleMs }

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
