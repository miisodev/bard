param(
  [string]$Version = "0.1.0",
  [string]$PackageName = "com.miisodev.bard",
  [string]$Publisher = "",
  [string]$OutputDir = "src-tauri\target\release\bundle\msix",
  [string]$CertificatePath,
  [string]$CertificatePassword = "",
  [string]$TimestampUrl = "https://timestamp.digicert.com",
  [switch]$RequireSignature,
  [switch]$SkipBinarySigning
)

$ErrorActionPreference = "Stop"
$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
$binary = Join-Path $repoRoot "src-tauri\target\release\bard.exe"
$versionParts = $Version.Split(".")
if ($versionParts.Count -ne 3 -or ($versionParts | Where-Object { $_ -notmatch '^\d+$' })) {
  throw "Version must use the form 0.1.0 (three numeric components)."
}
$packageVersion = "$Version.0"
if ($PackageName -notmatch '^[A-Za-z0-9.-]+$') { throw "PackageName contains unsupported characters." }
$outputPath = Join-Path $repoRoot $OutputDir
$staging = Join-Path $repoRoot "src-tauri\target\msix-staging\$Version"
$package = Join-Path $outputPath "Bard_${Version}_x64.msix"

function Find-SdkTool([string]$Name) {
  $tool = Get-ChildItem "C:\Program Files (x86)\Windows Kits\10\bin" -Recurse -Filter $Name -ErrorAction SilentlyContinue |
    Where-Object { $_.FullName -match "\\x64\\" } |
    Sort-Object FullName -Descending |
    Select-Object -First 1 -ExpandProperty FullName
  if (-not $tool) { throw "$Name was not found in the Windows SDK." }
  return $tool
}

$makeAppx = Find-SdkTool "makeappx.exe"
$signTool = Find-SdkTool "signtool.exe"

if (-not (Test-Path -LiteralPath $binary)) {
  throw "Missing release binary: $binary. Run the Tauri release build first."
}
if ($RequireSignature -and [string]::IsNullOrWhiteSpace($CertificatePath)) {
  throw "A code-signing PFX is required with -RequireSignature. The package is never published unsigned."
}
if ($CertificatePath -and -not (Test-Path -LiteralPath $CertificatePath)) {
  throw "Certificate not found: $CertificatePath"
}

if ($CertificatePath -and [string]::IsNullOrEmpty($CertificatePassword)) {
  $secure = Read-Host "PFX password" -AsSecureString
  $pointer = [Runtime.InteropServices.Marshal]::SecureStringToBSTR($secure)
  try {
    $CertificatePassword = [Runtime.InteropServices.Marshal]::PtrToStringBSTR($pointer)
  } finally {
    [Runtime.InteropServices.Marshal]::ZeroFreeBSTR($pointer)
  }
}

if ([string]::IsNullOrWhiteSpace($Publisher)) {
  if ($CertificatePath) {
    $securePassword = ConvertTo-SecureString $CertificatePassword -AsPlainText -Force
    $certificateInfo = Get-PfxCertificate -FilePath $CertificatePath -Password $securePassword
    $Publisher = $certificateInfo.Subject
  } else {
    $Publisher = "CN=Kevin Miiso Novo"
  }
}

if (Test-Path -LiteralPath $staging) {
  Remove-Item -LiteralPath $staging -Recurse -Force
}
New-Item -ItemType Directory -Path $staging -Force | Out-Null
New-Item -ItemType Directory -Path (Join-Path $staging "Assets") -Force | Out-Null
New-Item -ItemType Directory -Path $outputPath -Force | Out-Null

Copy-Item -LiteralPath $binary -Destination (Join-Path $staging "Bard.exe")
foreach ($asset in @("StoreLogo.png", "Square44x44Logo.png", "Square150x150Logo.png", "Square310x310Logo.png")) {
  $assetPath = Join-Path $repoRoot "src-tauri\icons\$asset"
  if (-not (Test-Path -LiteralPath $assetPath)) { throw "Missing MSIX asset: $assetPath" }
  Copy-Item -LiteralPath $assetPath -Destination (Join-Path $staging "Assets\$asset")
}

$escapedPublisher = [Security.SecurityElement]::Escape($Publisher)
$manifest = @"
<?xml version="1.0" encoding="utf-8"?>
<Package xmlns="http://schemas.microsoft.com/appx/manifest/foundation/windows10"
  xmlns:uap="http://schemas.microsoft.com/appx/manifest/uap/windows10"
  xmlns:uap10="http://schemas.microsoft.com/appx/manifest/uap/windows10/10"
  xmlns:rescap="http://schemas.microsoft.com/appx/manifest/foundation/windows10/restrictedcapabilities">
  <Identity Name="$PackageName" Version="$packageVersion" Publisher="$escapedPublisher" ProcessorArchitecture="x64" />
  <Properties>
    <DisplayName>Bard</DisplayName>
    <PublisherDisplayName>miiso.dev</PublisherDisplayName>
    <Description>Capture. Dictate. Keep. A local-first Windows workspace.</Description>
    <Logo>Assets\StoreLogo.png</Logo>
  </Properties>
  <Resources>
    <Resource Language="en-us" />
  </Resources>
  <Dependencies>
    <TargetDeviceFamily Name="Windows.Desktop" MinVersion="10.0.17763.0" MaxVersionTested="10.0.26100.0" />
  </Dependencies>
  <Capabilities>
    <rescap:Capability Name="runFullTrust" />
  </Capabilities>
  <Applications>
    <Application Id="Bard" Executable="Bard.exe" EntryPoint="Windows.FullTrustApplication" uap10:RuntimeBehavior="packagedClassicApp" uap10:TrustLevel="mediumIL">
      <uap:VisualElements DisplayName="Bard" Description="Capture. Dictate. Keep." Square150x150Logo="Assets\Square150x150Logo.png" Square44x44Logo="Assets\Square44x44Logo.png" BackgroundColor="transparent" />
    </Application>
  </Applications>
</Package>
"@
Set-Content -LiteralPath (Join-Path $staging "AppxManifest.xml") -Value $manifest -Encoding UTF8

if ($CertificatePath -and -not $SkipBinarySigning) {
  & $signTool sign /fd SHA256 /tr $TimestampUrl /td SHA256 /f $CertificatePath /p $CertificatePassword $binary
  if ($LASTEXITCODE -ne 0) { throw "Authenticode signing failed for Bard.exe." }
}
if ($CertificatePath) {
  & $signTool verify /pa /all $binary
  if ($LASTEXITCODE -ne 0) { throw "Bard.exe signature verification failed." }
  Copy-Item -LiteralPath $binary -Destination (Join-Path $staging "Bard.exe") -Force
}

if (Test-Path -LiteralPath $package) { Remove-Item -LiteralPath $package -Force }
& $makeAppx pack /d $staging /p $package /l
if ($LASTEXITCODE -ne 0) { throw "MakeAppx failed to create the MSIX package." }
$validationDir = Join-Path $repoRoot "src-tauri\target\msix-validation\$Version"
if (Test-Path -LiteralPath $validationDir) { Remove-Item -LiteralPath $validationDir -Recurse -Force }
& $makeAppx unpack /p $package /d $validationDir /l
if ($LASTEXITCODE -ne 0) { throw "MakeAppx could not unpack the MSIX package for validation." }
Remove-Item -LiteralPath $validationDir -Recurse -Force

if ($CertificatePath) {
  & $signTool sign /fd SHA256 /tr $TimestampUrl /td SHA256 /f $CertificatePath /p $CertificatePassword $package
  if ($LASTEXITCODE -ne 0) { throw "MSIX package signing failed." }
  & $signTool verify /pa /all $package
  if ($LASTEXITCODE -ne 0) { throw "MSIX package signature verification failed." }
}

$item = Get-Item -LiteralPath $package
[pscustomobject]@{
  File = $item.FullName
  Bytes = $item.Length
  SHA256 = (Get-FileHash -LiteralPath $package -Algorithm SHA256).Hash
  Signed = [bool]$CertificatePath
} | Format-List
Write-Host "MSIX package created. It is ready for local validation; publication still requires the release signature gate." -ForegroundColor Green
