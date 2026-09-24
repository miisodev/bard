<#
  build-release.ps1 — build the signed direct-download release on this machine.

  Code-signing keys have been hardware-bound since June 2023: a certificate
  arrives on a USB token or behind a CA's cloud-signing service, never as a
  PFX file. Once its token or signing client is set up, the certificate shows
  up in the Windows certificate store, and Tauri signs with it by thumbprint
  during the build itself:

    bard.exe → the NSIS installer and its uninstaller → the updater signature

  That order is the point. Signing bard.exe after the installer is built leaves
  the installer carrying the unsigned copy, and signing the installer after the
  updater signature is made breaks the signature every client verifies.

    .\scripts\build-release.ps1 -CertificateThumbprint <SHA-1 thumbprint>

  A cloud signer that has no certificate-store integration signs through a
  command instead; %1 is the file Tauri passes:

    .\scripts\build-release.ps1 -SignCommand "<signer> sign ... %1"

  Until a certificate is bought, the direct download ships unsigned, by
  decision (2026-09-24): Windows shows its SmartScreen prompt on first run and
  the installer is otherwise identical. That build is asked for by name, so an
  unsigned release is never the result of a forgotten parameter:

    .\scripts\build-release.ps1 -Unsigned

  The updater key comes from TAURI_SIGNING_PRIVATE_KEY(_PASSWORD), or from
  $HOME\.tauri\bard-updater.key and bard-updater-password.txt. The updater
  signature is independent of Authenticode and is made either way.
#>
param(
  [string]$Version = "0.1.0",
  [string]$CertificateThumbprint,
  [string]$SignCommand,
  [string]$TimestampUrl = "http://timestamp.digicert.com",
  [switch]$Unsigned
)

$ErrorActionPreference = "Stop"
$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
Set-Location $repoRoot

$signing = -not ([string]::IsNullOrWhiteSpace($CertificateThumbprint) -and [string]::IsNullOrWhiteSpace($SignCommand))
if (-not $signing -and -not $Unsigned) {
  throw "Pass -CertificateThumbprint (token or store certificate), -SignCommand (cloud signer), or -Unsigned."
}
if ($signing -and $Unsigned) {
  throw "-Unsigned cannot be combined with a certificate or a sign command."
}
if (git status --porcelain) {
  throw "The worktree is dirty. A release is built from a committed tree only."
}

$windows = [ordered]@{ digestAlgorithm = "sha256"; timestampUrl = $TimestampUrl; tsp = $true }
if ($Unsigned) {
  Write-Warning "Building an unsigned direct release. Windows SmartScreen prompts on first run."
} elseif ($CertificateThumbprint) {
  $thumbprint = ($CertificateThumbprint -replace '\s', '').ToUpperInvariant()
  $certificate = Get-ChildItem Cert:\CurrentUser\My, Cert:\LocalMachine\My -ErrorAction SilentlyContinue |
    Where-Object { $_.Thumbprint -eq $thumbprint } |
    Select-Object -First 1
  if (-not $certificate) { throw "No certificate with thumbprint $thumbprint is in the Windows certificate store." }
  if (-not ($certificate.EnhancedKeyUsageList.ObjectId -contains "1.3.6.1.5.5.7.3.3")) {
    throw "Certificate $thumbprint is not a code-signing certificate."
  }
  Write-Host "Signing as: $($certificate.Subject)"
  $windows.certificateThumbprint = $thumbprint
} else {
  $windows = [ordered]@{ signCommand = $SignCommand }
}
$config = @{ bundle = @{ windows = $windows } } | ConvertTo-Json -Depth 5 -Compress

if ([string]::IsNullOrEmpty($env:TAURI_SIGNING_PRIVATE_KEY)) {
  $env:TAURI_SIGNING_PRIVATE_KEY = Get-Content "$HOME\.tauri\bard-updater.key" -Raw
}
if ([string]::IsNullOrEmpty($env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD)) {
  $env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD = (Get-Content "$HOME\.tauri\bard-updater-password.txt" -Raw).Trim()
}
$env:BARD_NO_GUARDIAN = "1"

if ($Unsigned) {
  bun run tauri build --bundles nsis
} else {
  bun run tauri build --bundles nsis --config $config
}
if ($LASTEXITCODE -ne 0) { throw "The release build failed." }

$verify = @{ Version = $Version }
if (-not $Unsigned) { $verify.RequireSignature = $true }
& (Join-Path $PSScriptRoot "verify-release.ps1") @verify
if ($LASTEXITCODE -ne 0) { throw "Release verification failed." }
Write-Host "Release built and verified$(if ($Unsigned) { ' (unsigned)' } else { ' (signed)' })." -ForegroundColor Green
