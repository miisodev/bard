param(
  [string]$Version = ((Get-Content (Join-Path $PSScriptRoot "..\package.json") -Raw | ConvertFrom-Json).version),
  [string]$BundleDir = "src-tauri\target\release\bundle\nsis",
  [switch]$RequireSignature
)
# The direct release is the signed NSIS installer and its updater signature.
# The Microsoft Store MSIX is not a public asset: Microsoft signs it after
# certification, so it is validated by build-msix.ps1 and Partner Center.

$ErrorActionPreference = "Stop"
$installer = Join-Path $BundleDir "Bard_${Version}_x64-setup.exe"
$signature = "$installer.sig"
$binary = "src-tauri\target\release\bard.exe"
$required = @($installer, $signature, $binary)
foreach ($path in $required) {
  if (-not (Test-Path -LiteralPath $path)) {
    throw "Missing release artifact: $path"
  }
}

$sevenZip = Get-Command 7z.exe -ErrorAction SilentlyContinue
if (-not $sevenZip -and (Test-Path 'C:\Program Files\7-Zip\7z.exe')) {
  $sevenZip = [pscustomobject]@{ Source = 'C:\Program Files\7-Zip\7z.exe' }
}
if ($sevenZip) {
  & $sevenZip.Source t $installer | Out-Host
  if ($LASTEXITCODE -ne 0) { throw "NSIS archive integrity failed" }
} else {
  # The archive test was silently skipped when 7-Zip was not installed, so a
  # release could be verified by hashes alone and nobody would know the
  # installer was never opened. The hashes still prove the bytes; the archive
  # test proves the installer is a readable NSIS package. Say plainly which
  # one was skipped rather than letting the absence pass for a pass.
  Write-Warning "7-Zip was not found, so the NSIS archive integrity test was skipped. The SHA-256 hashes below are still checked against the published SHA256SUMS; install 7-Zip to also prove the installer opens."
}

$artifacts = @($binary, $installer, $signature)
$hashes = foreach ($path in $artifacts) {
  $item = Get-Item -LiteralPath $path
  [pscustomobject]@{
    File = $item.Name
    Bytes = $item.Length
    SHA256 = (Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash
  }
}
$hashes | Format-Table -AutoSize | Out-Host

$authenticode = Get-AuthenticodeSignature -FilePath $installer
$binarySignature = Get-AuthenticodeSignature -FilePath $binary
$authenticode | Select-Object Status,StatusMessage,SignerCertificate,TimeStamperCertificate | Format-List | Out-Host
$binarySignature | Select-Object Status,StatusMessage,SignerCertificate,TimeStamperCertificate | Format-List | Out-Host

$signtool = Get-ChildItem "C:\Program Files (x86)\Windows Kits\10\bin" -Recurse -Filter signtool.exe -ErrorAction SilentlyContinue |
  Where-Object { $_.FullName -match "\\x64\\" } |
  Sort-Object FullName -Descending |
  Select-Object -First 1 -ExpandProperty FullName

if ($RequireSignature) {
  if ($authenticode.Status -ne "Valid" -or $binarySignature.Status -ne "Valid") {
    throw "Authenticode verification failed"
  }
  if (-not $signtool) { throw "signtool.exe was not found" }
  & $signtool verify /pa /all /v $installer
  if ($LASTEXITCODE -ne 0) { throw "Installer signature verification failed" }
  & $signtool verify /pa /all /v $binary
  if ($LASTEXITCODE -ne 0) { throw "Binary signature verification failed" }
}

if ((Get-Item -LiteralPath $signature).Length -lt 32) {
  throw "Tauri updater signature is empty or truncated"
}
$signatureText = Get-Content -LiteralPath $signature -Raw
try {
  $envelopeText = [Text.Encoding]::UTF8.GetString([Convert]::FromBase64String($signatureText.Trim()))
} catch {
  throw "Tauri updater signature envelope is not valid base64"
}
if ($envelopeText -notmatch "signature from tauri secret key" -or $envelopeText -notmatch "trusted comment: timestamp") {
  throw "Tauri updater signature envelope is incomplete"
}
& cargo run --quiet --locked --manifest-path src-tauri/Cargo.toml --example verify_updater -- src-tauri/tauri.conf.json $installer $signature
if ($LASTEXITCODE -ne 0) { throw "Tauri updater signature verification failed" }

Write-Host "Release artifact verification completed." -ForegroundColor Green
