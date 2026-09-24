param(
  [string]$Version = "0.1.0",
  [string]$BundleDir = "src-tauri\target\release\bundle\nsis",
  [string]$MsixDir = "src-tauri\target\release\bundle\msix",
  [switch]$RequireSignature,
  [switch]$RequireMsix
)

$ErrorActionPreference = "Stop"
$installer = Join-Path $BundleDir "Bard_${Version}_x64-setup.exe"
$signature = "$installer.sig"
$binary = "src-tauri\target\release\bard.exe"
$msix = Join-Path $MsixDir "Bard_${Version}_x64.msix"

$required = @($installer, $signature, $binary)
if ($RequireMsix) { $required += $msix }
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
}

$artifacts = @($binary, $installer, $signature)
if (Test-Path -LiteralPath $msix) { $artifacts += $msix }
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
  if ($RequireMsix) {
    & $signtool verify /pa /all /v $msix
    if ($LASTEXITCODE -ne 0) { throw "MSIX package signature verification failed" }
  }
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
