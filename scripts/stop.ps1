<#
  stop.ps1 — stop bard the way the app considers clean.

  **The crash guardian is opt-in and off by default.** src-tauri/src/main.rs
  spawns a guardian only when BARD_ENABLE_GUARDIAN is set in the environment,
  and BARD_NO_GUARDIAN suppresses it. Nothing in a normal launch — the tray, the
  autostart entry, a second instance, the updater restart — sets it, so the
  guardian described below is usually *not* running and killing bard is just
  killing bard. Check before you go looking for a resurrection:

    $env:BARD_ENABLE_GUARDIAN   # set → a guardian is watching this process
    $env:BARD_NO_GUARDIAN       # set → a guardian was explicitly suppressed

  When the guardian *is* running it waits 20s for the main process and, if that
  dies inside the window without a clean-exit marker, relaunches it. The
  resurrected instance then holds the single-instance lock, and a `tauri dev`
  started afterwards bounces off it and exits 255. That is the whole reason
  this script writes the marker first, then kills, then sweeps.

  So: marker, kill, sweep.

  (The guardian being needed at all is a product defect, not just a dev
  inconvenience — End Task in Task Manager hits the same resurrection when it is
  enabled. Tracked as D3 in docs/PARITY.md.)
#>
param([int]$TimeoutMs = 5000)

$guardian = (Test-Path Env:\BARD_ENABLE_GUARDIAN) -and -not (Test-Path Env:\BARD_NO_GUARDIAN)

$base = if ($env:LOCALAPPDATA) { $env:LOCALAPPDATA } else { $env:APPDATA }
$marker = Join-Path $base "com.miisodev.bard\clean-exit"
$dir = Split-Path -Parent $marker
if (Test-Path $dir) { Set-Content -Path $marker -Value "1" -NoNewline }

$deadline = [Environment]::TickCount + $TimeoutMs
do {
  $procs = Get-Process bard -ErrorAction SilentlyContinue
  if (-not $procs) { break }
  $procs | Stop-Process -Force -ErrorAction SilentlyContinue
  Start-Sleep -Milliseconds 400
} while ([Environment]::TickCount -lt $deadline)

# Leave the marker alone. A guardian spawned by the process we just killed can
# still be inside its 20s window and has not looked yet; clearing the marker
# here is what makes it resurrect the app a moment later, which then holds the
# single-instance lock and makes the next `tauri dev` exit 255. The guardian
# removes the marker itself once it has seen the clean exit.
#
# For a dev loop, prefer BARD_NO_GUARDIAN=1 — no guardian is spawned at all and
# start/stop becomes deterministic. This script's environment is not the app's
# environment, though: BARD_ENABLE_GUARDIAN is inherited by a child bard.exe, so
# reading it here is the answer for a child started from this shell. A bard that
# was launched from somewhere else, or by Windows at login, is not covered.
if (-not (Get-Process bard -ErrorAction SilentlyContinue)) {
  Write-Output "bard stopped (guardian $(if ($guardian) { 'enabled' } else { 'not enabled - no resurrection expected' }))"
} else {
  Write-Output "bard still running after ${TimeoutMs}ms"
  exit 1
}
