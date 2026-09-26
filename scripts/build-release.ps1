<#
  build-release.ps1 — build the direct-download release on this machine.

  A Bard release carries two signatures, and they are made at different times
  on purpose.

  **Authenticode** names the publisher. Code-signing keys have been
  hardware-bound since June 2023 (CA/B Forum): a certificate arrives on a USB
  token or behind a CA's cloud-signing service, never as a PFX file. Once its
  token or signing client is set up, the certificate shows up in the Windows
  certificate store and Tauri signs with it by thumbprint *during the build*:

    bard.exe -> the NSIS installer and its uninstaller

  That has to happen inside the build, in that order. Signing bard.exe after
  the installer is built leaves the installer carrying the unsigned copy, and
  signing the installer after the updater signature is made breaks the
  signature every client verifies.

    .\scripts\build-release.ps1 -CertificateThumbprint <SHA-1 thumbprint>

  A cloud signer that has no certificate-store integration signs through a
  command instead; %1 is the file Tauri passes:

    .\scripts\build-release.ps1 -SignCommand "<signer> sign ... %1"

  Until a certificate is bought, the direct download ships unsigned, by
  decision (2026-09-24): Windows shows its SmartScreen prompt on first run and
  the installer is otherwise identical. That build is asked for by name, so an
  unsigned release is never the result of a forgotten parameter:

    .\scripts\build-release.ps1 -Unsigned

  **The updater signature** is what protects the download, and it is
  deliberately NOT made by the build. The build runs with
  `bundle.createUpdaterArtifacts` off and the signing environment removed, and
  the finished installer is signed afterwards by scripts/tauri-sign.mjs:

    bard.exe -> the NSIS installer -> <installer>.sig

  The reason is the trust boundary, not the ordering. `bun run tauri build`
  hands its environment to bun, to the JavaScript in node_modules/@tauri-apps/
  cli that bun then loads, to every lifecycle script bun runs, to cargo, and
  to every build.rs in the dependency graph. Put TAURI_SIGNING_PRIVATE_KEY in
  that environment and one compromised transitive npm dependency with a build
  hook can read the updater key. The signature it produced would be *genuinely
  valid*, so tauri-plugin-updater would accept its installer on every
  installed client and the compromise would be invisible. That is the only key
  in this project whose loss is both silent and total, so it is kept out of the
  build entirely and used once, afterwards, by a process that loads no
  JavaScript from node_modules at all.

  The key comes from $HOME\.tauri\bard-updater.key with its password in
  bard-updater-password.txt (override with -UpdaterKeyPath /
  -UpdaterPasswordPath). The updater signature is independent of Authenticode
  and is made either way.
#>
param(
  [string]$Version = ((Get-Content (Join-Path $PSScriptRoot "..\package.json") -Raw | ConvertFrom-Json).version),
  [string]$CertificateThumbprint,
  [string]$SignCommand,
  [string]$TimestampUrl = "http://timestamp.digicert.com",
  [string]$UpdaterKeyPath = "$HOME\.tauri\bard-updater.key",
  [string]$UpdaterPasswordPath = "$HOME\.tauri\bard-updater-password.txt",
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

# The updater key is checked before the build, not after. Discovering a missing
# key fifteen minutes into a release build is how people end up publishing an
# unsigned updater artefact.
foreach ($required in @($UpdaterKeyPath, $UpdaterPasswordPath)) {
  if (-not (Test-Path -LiteralPath $required)) {
    throw "Missing $required. The updater signature is made after the build and is required either way; generate a key with 'bun run tauri signer generate -w <path> -p <password>'."
  }
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

# createUpdaterArtifacts is turned OFF for the build. Leaving it on is not
# merely a leak: the bundler reads the private key on the way past and fails
# with "A public key has been found, but no private key" the moment it is
# missing, so the build cannot be trusted to opt out on its own. The signature
# is written by the separate signing step below.
$bundle = @{ createUpdaterArtifacts = $false }
if (-not $Unsigned) { $bundle.windows = $windows }
$configPath = Join-Path $repoRoot "src-tauri\target\bundle-config.json"
New-Item -ItemType Directory -Force (Split-Path -Parent $configPath) | Out-Null
# No BOM: the config is parsed as JSON, and a byte-order mark is not JSON.
[System.IO.File]::WriteAllText(
  $configPath,
  (@{ bundle = $bundle } | ConvertTo-Json -Depth 5),
  (New-Object System.Text.UTF8Encoding($false))
)

$installer = "src-tauri\target\release\bundle\nsis\Bard_${Version}_x64-setup.exe"
$signature = "$installer.sig"
# A signature from an earlier build must never be mistaken for this one's.
Remove-Item -LiteralPath $signature -Force -ErrorAction SilentlyContinue

# Take the updater key out of the build's environment, and put back whatever was
# there so the operator's own shell is not left changed.
$signingEnv = @("TAURI_SIGNING_PRIVATE_KEY", "TAURI_SIGNING_PRIVATE_KEY_PASSWORD", "TAURI_SIGNING_PRIVATE_KEY_PATH")
$saved = @{}
foreach ($name in $signingEnv) {
  if (Test-Path "env:$name") { $saved[$name] = (Get-Item "env:$name").Value }
  Remove-Item "env:$name" -ErrorAction SilentlyContinue
}
if ($saved.Count -gt 0) {
  Write-Warning "Ignoring $($saved.Keys -join ', ') for the build. The updater key is used by the separate signing step, never by the build."
}
$env:BARD_NO_GUARDIAN = "1"

try {
  bun run tauri build --bundles nsis --config $configPath
  if ($LASTEXITCODE -ne 0) { throw "The release build failed." }
} finally {
  foreach ($name in $signingEnv) { Remove-Item "env:$name" -ErrorAction SilentlyContinue }
  foreach ($name in $saved.Keys) { Set-Item "env:$name" $saved[$name] }
}

# Step 2: the updater signature, over the finished installer, in a process that
# has not evaluated a line of JavaScript from node_modules.
Write-Host "Signing the updater artefact (separate step, no build in this process's environment)."
$node = Get-Command node.exe -ErrorAction SilentlyContinue
if ($node) { $interpreter = $node.Source } else {
  $bun = Get-Command bun.exe -ErrorAction SilentlyContinue
  if (-not $bun) { throw "Neither node nor bun is on PATH; the signing step needs one of them." }
  $interpreter = $bun.Source
}
Write-Host "  signer host: $interpreter"
& $interpreter (Join-Path $PSScriptRoot "tauri-sign.mjs") `
  --target $installer `
  --key $UpdaterKeyPath `
  --password-file $UpdaterPasswordPath
if ($LASTEXITCODE -ne 0) { throw "The updater signature failed." }

$verify = @{ Version = $Version }
if (-not $Unsigned) { $verify.RequireSignature = $true }
& (Join-Path $PSScriptRoot "verify-release.ps1") @verify
if ($LASTEXITCODE -ne 0) { throw "Release verification failed." }
Remove-Item -LiteralPath $configPath -Force -ErrorAction SilentlyContinue
Write-Host "Release built and verified$(if ($Unsigned) { ' (unsigned)' } else { ' (signed)' })." -ForegroundColor Green
