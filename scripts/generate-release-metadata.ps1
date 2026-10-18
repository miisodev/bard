param(
  [string]$Version = ((Get-Content (Join-Path $PSScriptRoot "..\package.json") -Raw | ConvertFrom-Json).version),
  [string]$BundleDir = "src-tauri\target\release\bundle\nsis",
  [string]$OutputDir = "release-metadata"
)
$ErrorActionPreference = "Stop"
New-Item -ItemType Directory -Force $OutputDir | Out-Null
$installer = Join-Path $BundleDir "Bard_${Version}_x64-setup.exe"
$binary = "src-tauri\target\release\bard.exe"
$signature = "$installer.sig"
$artifactPaths = @($binary, $installer, $signature)
$artifacts = $artifactPaths | ForEach-Object {
  $item = Get-Item -LiteralPath $_
  [ordered]@{
    file = $item.Name
    bytes = $item.Length
    sha256 = (Get-FileHash -LiteralPath $_ -Algorithm SHA256).Hash
  }
}
$commit = (git rev-parse HEAD).Trim()
$provenance = [ordered]@{
  schema = "https://miiso.dev/schemas/bard-release-provenance-1.json"
  product = "Bard"
  version = $Version
  git_commit = $commit
  generated_at = [DateTime]::UtcNow.ToString("o")
  source_archive = "Not generated; the source is private and Bard releases carry no source archive."
  artifacts = $artifacts
  signing = [ordered]@{
    authenticode = [ordered]@{ required = $false; status = "unsigned by decision; SmartScreen shows its notice once" }
    microsoft_store = [ordered]@{ public_asset = $false; status = "MSIX submitted through Partner Center; Microsoft signs it after certification" }
    tauri_updater = [ordered]@{ required = $true; envelope = $signature }
  }
}
$provenance | ConvertTo-Json -Depth 10 | Set-Content (Join-Path $OutputDir "release-provenance.json") -Encoding utf8

$cargo = cargo metadata --manifest-path src-tauri/Cargo.toml --locked --format-version 1 --no-deps | ConvertFrom-Json
$package = Get-Content package.json -Raw | ConvertFrom-Json
$components = @(
  $cargo.packages | ForEach-Object {
    [ordered]@{ type = "library"; name = $_.name; version = $_.version; purl = "pkg:cargo/$($_.name)@$($_.version)" }
  }
  [ordered]@{ type = "application"; name = $package.name; version = $package.version; purl = "pkg:npm/$($package.name)@$($package.version)" }
)
[ordered]@{
  bomFormat = "CycloneDX"
  specVersion = "1.5"
  serialNumber = "urn:uuid:$([guid]::NewGuid())"
  version = 1
  metadata = [ordered]@{ component = [ordered]@{ type = "application"; name = "Bard"; version = $Version } }
  components = $components
} | ConvertTo-Json -Depth 10 | Set-Content (Join-Path $OutputDir "bard-sbom.json") -Encoding utf8
Write-Host "Release metadata written to $OutputDir" -ForegroundColor Green
