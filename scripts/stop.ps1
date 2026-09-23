<#
  stop.ps1 — stop bard the way the app considers clean.

  bard spawns a guardian on launch (src-tauri/src/guardian.rs) that waits 20s
  for the main process and, if it dies inside that window without leaving a
  clean-exit marker, relaunches it. Killing bard directly therefore resurrects
  it — the new instance then holds the single-instance lock, and a `tauri dev`
  started afterwards bounces off it and exits 255.

  So: write the marker first, then kill, then sweep anything the guardian
  managed to spawn in between.

  (That the marker is needed at all is a product defect, not just a dev
  inconvenience — End Task in Task Manager hits the same resurrection. Tracked
  as D3 in docs/PARITY.md.)
#>
param([int]$TimeoutMs = 5000)

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
# start/stop becomes deterministic.
if (-not (Get-Process bard -ErrorAction SilentlyContinue)) {
  Write-Output "bard stopped"
} else {
  Write-Output "bard still running after ${TimeoutMs}ms"
  exit 1
}
