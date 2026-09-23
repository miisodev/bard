param(
  [string]$Version = "0.1.0",
  [string]$CertificatePath,
  [string]$CertificatePassword = "",
  [string]$TimestampUrl = "https://timestamp.digicert.com",
  [string]$BundleDir = "src-tauri\target\release\bundle\nsis"
)

$ErrorActionPreference = "Stop"
if ([string]::IsNullOrWhiteSpace($CertificatePath)) {
  throw "Pass -CertificatePath pointing to a code-signing PFX. The release never stores this file."
}
if (-not (Test-Path -LiteralPath $CertificatePath)) {
  throw "Certificate not found: $CertificatePath"
}
if ([string]::IsNullOrEmpty($CertificatePassword)) {
  $secure = Read-Host "PFX password" -AsSecureString
  $pointer = [Runtime.InteropServices.Marshal]::SecureStringToBSTR($secure)
  try {
    $CertificatePassword = [Runtime.InteropServices.Marshal]::PtrToStringBSTR($pointer)
  } finally {
    [Runtime.InteropServices.Marshal]::ZeroFreeBSTR($pointer)
  }
}
$signtool = Get-ChildItem "C:\Program Files (x86)\Windows Kits\10\bin" -Recurse -Filter signtool.exe -ErrorAction SilentlyContinue |
  Where-Object { $_.FullName -match "\\x64\\" } |
  Sort-Object FullName -Descending |
  Select-Object -First 1 -ExpandProperty FullName
if (-not $signtool) { throw "signtool.exe was not found" }

$installer = Join-Path $BundleDir "Bard_${Version}_x64-setup.exe"
$binary = "src-tauri\target\release\bard.exe"
foreach ($artifact in @($binary, $installer)) {
  if (-not (Test-Path -LiteralPath $artifact)) { throw "Missing release artifact: $artifact" }
  & $signtool sign /fd SHA256 /tr $TimestampUrl /td SHA256 /f $CertificatePath /p $CertificatePassword $artifact
  if ($LASTEXITCODE -ne 0) { throw "Authenticode signing failed for $artifact" }
  & $signtool verify /pa /all $artifact
  if ($LASTEXITCODE -ne 0) { throw "Authenticode verification failed for $artifact" }
}
Write-Host "Authenticode signing completed." -ForegroundColor Green
