<#
  build-msix.ps1 — package Bard for the Microsoft Store.

  MSIX is the Store channel only. The direct download from miiso.dev is the
  signed NSIS installer; there is no downloadable MSIX, because a sideloaded
  copy would carry a different identity from the Store copy, install beside it,
  and never receive updates.

  Partner Center owns the package identity. Copy Package/Identity/Name and
  Package/Identity/Publisher from Partner Center > Product identity; the Store
  rejects a package whose identity differs, and re-signs the package itself
  after certification, so no certificate is involved here.

    # Store submission. -Store fills in the reserved identity for
    # "Bard: Clipboard, Draw and Dictate" (Store ID 9PBLVGFCJ5V1).
    .\scripts\build-msix.ps1 -Store

    # Local manifest/package validation (not submittable)
    bun run build:msix

  Build the signed release first (scripts\build-release.ps1): the package
  embeds that exact bard.exe.
#>
param(
  [string]$Version = ((Get-Content (Join-Path $PSScriptRoot "..\package.json") -Raw | ConvertFrom-Json).version),
  [string]$PackageName = "miisodev.Bard.Validation",
  [string]$Publisher = "CN=Bard Validation",
  [string]$PublisherDisplayName = "miiso.dev",
  # The package's display name, which the Store matches against the reserved
  # names. The tile and the Start menu keep the app's own name, Bard.
  [string]$DisplayName = "Bard",
  [string]$OutputDir = "src-tauri\target\release\bundle\msix",
  [switch]$Store
)

$ErrorActionPreference = "Stop"

# The reserved Store identity (Partner Center > Product identity). A Store
# build takes these unless they are passed explicitly.
if ($Store) {
  if (-not $PSBoundParameters.ContainsKey("PackageName")) { $PackageName = "miisodev.BardClipboardDrawandDictate" }
  if (-not $PSBoundParameters.ContainsKey("Publisher")) { $Publisher = "CN=61F791B1-D87E-4C90-9617-536200345DA6" }
  if (-not $PSBoundParameters.ContainsKey("PublisherDisplayName")) { $PublisherDisplayName = "miisodev" }
  if (-not $PSBoundParameters.ContainsKey("DisplayName")) { $DisplayName = "Bard: Clipboard, Draw and Dictate" }
}
$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
$binary = Join-Path $repoRoot "src-tauri\target\release\bard.exe"
$versionParts = $Version.Split(".")
if ($versionParts.Count -ne 3 -or ($versionParts | Where-Object { $_ -notmatch '^\d+$' })) {
  throw "Version must use the form 0.1.0 (three numeric components)."
}
$packageVersion = "$Version.0"
if ($PackageName -notmatch '^[A-Za-z0-9.-]+$') { throw "PackageName contains unsupported characters." }
if ($Store -and ($PackageName -eq "miisodev.Bard.Validation" -or $Publisher -eq "CN=Bard Validation")) {
  throw "A Store package needs the Identity Name and Publisher from Partner Center > Product identity."
}
$outputPath = Join-Path $repoRoot $OutputDir
$staging = Join-Path $repoRoot "src-tauri\target\msix-staging\$Version"
$package = Join-Path $outputPath "Bard_${Version}_x64_Store.msix"

function Find-SdkTool([string]$Name) {
  $tool = Get-ChildItem "C:\Program Files (x86)\Windows Kits\10\bin" -Recurse -Filter $Name -ErrorAction SilentlyContinue |
    Where-Object { $_.FullName -match "\\x64\\" } |
    Sort-Object FullName -Descending |
    Select-Object -First 1 -ExpandProperty FullName
  if (-not $tool) { throw "$Name was not found in the Windows SDK." }
  return $tool
}

$makeAppx = Find-SdkTool "makeappx.exe"

if (-not (Test-Path -LiteralPath $binary)) {
  throw "Missing release binary: $binary. Run the release build first."
}
# Microsoft signs the whole package after certification, and a Store install
# never meets SmartScreen, so the Store channel does not wait for Bard's own
# certificate. Once that certificate exists, the package embeds the signed exe.
if ($Store -and (Get-AuthenticodeSignature -FilePath $binary).Status -ne "Valid") {
  Write-Warning "bard.exe is not Authenticode-signed. Acceptable for the Store, which signs the package."
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

# uap10 (packagedClassicApp, mediumIL) needs Windows 10 2004, build 19041.
# The startup task is how a packaged app starts at login: Windows owns it and
# lists it under Settings > Apps > Startup, and Bard's General pane opens that
# page instead of writing a Run key the package could never expose.
$escapedPublisher = [Security.SecurityElement]::Escape($Publisher)
$escapedPublisherName = [Security.SecurityElement]::Escape($PublisherDisplayName)
$escapedDisplayName = [Security.SecurityElement]::Escape($DisplayName)
$manifest = @"
<?xml version="1.0" encoding="utf-8"?>
<Package xmlns="http://schemas.microsoft.com/appx/manifest/foundation/windows10"
  xmlns:uap="http://schemas.microsoft.com/appx/manifest/uap/windows10"
  xmlns:uap10="http://schemas.microsoft.com/appx/manifest/uap/windows10/10"
  xmlns:desktop="http://schemas.microsoft.com/appx/manifest/desktop/windows10"
  xmlns:rescap="http://schemas.microsoft.com/appx/manifest/foundation/windows10/restrictedcapabilities"
  IgnorableNamespaces="uap10 desktop rescap">
  <Identity Name="$PackageName" Version="$packageVersion" Publisher="$escapedPublisher" ProcessorArchitecture="x64" />
  <Properties>
    <DisplayName>$escapedDisplayName</DisplayName>
    <PublisherDisplayName>$escapedPublisherName</PublisherDisplayName>
    <Description>Clipboard, Draw and Dictate. A local-first Windows workspace.</Description>
    <Logo>Assets\StoreLogo.png</Logo>
  </Properties>
  <Resources>
    <Resource Language="en-us" />
  </Resources>
  <Dependencies>
    <TargetDeviceFamily Name="Windows.Desktop" MinVersion="10.0.19041.0" MaxVersionTested="10.0.26100.0" />
  </Dependencies>
  <Capabilities>
    <Capability Name="internetClient" />
    <rescap:Capability Name="runFullTrust" />
    <DeviceCapability Name="microphone" />
  </Capabilities>
  <Applications>
    <Application Id="Bard" Executable="Bard.exe" EntryPoint="Windows.FullTrustApplication" uap10:RuntimeBehavior="packagedClassicApp" uap10:TrustLevel="mediumIL">
      <uap:VisualElements DisplayName="Bard" Description="Clipboard, Draw and Dictate" Square150x150Logo="Assets\Square150x150Logo.png" Square44x44Logo="Assets\Square44x44Logo.png" BackgroundColor="transparent" />
      <Extensions>
        <desktop:Extension Category="windows.startupTask" Executable="Bard.exe" EntryPoint="Windows.FullTrustApplication">
          <desktop:StartupTask TaskId="BardStartup" Enabled="true" DisplayName="Bard" />
        </desktop:Extension>
      </Extensions>
    </Application>
  </Applications>
</Package>
"@
Set-Content -LiteralPath (Join-Path $staging "AppxManifest.xml") -Value $manifest -Encoding UTF8

if (Test-Path -LiteralPath $package) { Remove-Item -LiteralPath $package -Force }
& $makeAppx pack /d $staging /p $package /l
if ($LASTEXITCODE -ne 0) { throw "MakeAppx failed to create the MSIX package." }
$validationDir = Join-Path $repoRoot "src-tauri\target\msix-validation\$Version"
if (Test-Path -LiteralPath $validationDir) { Remove-Item -LiteralPath $validationDir -Recurse -Force }
& $makeAppx unpack /p $package /d $validationDir /l
if ($LASTEXITCODE -ne 0) { throw "MakeAppx could not unpack the MSIX package for validation." }
Remove-Item -LiteralPath $validationDir -Recurse -Force

$item = Get-Item -LiteralPath $package
[pscustomobject]@{
  File = $item.FullName
  Bytes = $item.Length
  SHA256 = (Get-FileHash -LiteralPath $package -Algorithm SHA256).Hash
  Identity = "$PackageName / $Publisher"
  Submittable = [bool]$Store
} | Format-List
if ($Store) {
  Write-Host "Store package created. Upload it in Partner Center; Microsoft signs it after certification." -ForegroundColor Green
} else {
  Write-Host "Validation package created. It carries a placeholder identity and is not submittable." -ForegroundColor Yellow
}
