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

# The whole resolved graph for the Windows target — not --no-deps: an SBOM
# whose component list is the product itself cannot answer what it exists to
# answer. --filter-platform scopes it to the crates Windows actually compiles
# (no GTK/WebKit) and resolves without building anything.
$cargo = cargo metadata --manifest-path src-tauri/Cargo.toml --locked --format-version 1 --filter-platform x86_64-pc-windows-msvc | ConvertFrom-Json
$package = Get-Content package.json -Raw | ConvertFrom-Json

# CycloneDX 1.5 takes either a licence object or an SPDX expression. Most
# crates carry an SPDX expression; the older "MIT/Apache-2.0" spelling is a
# cargo convention, not SPDX, so it travels as a named licence instead.
$spdxToken = '^(?:\(*[A-Za-z0-9.+-]+\)*|OR|AND|WITH)$'
function Get-Licenses($Text) {
  if ([string]::IsNullOrWhiteSpace($Text)) { return }
  $value = "$Text".Trim()
  if (@($value -split '\s+' | Where-Object { $_ -notmatch $spdxToken }).Count -eq 0) {
    return ,@{ expression = $value }
  }
  return ,@{ license = @{ name = $value } }
}

$crates = @($cargo.packages | Sort-Object { "pkg:cargo/$($_.name)@$($_.version)" } | ForEach-Object {
  $component = [ordered]@{ type = "library"; name = $_.name; version = $_.version; purl = "pkg:cargo/$($_.name)@$($_.version)" }
  $licenses = @(Get-Licenses $_.license)
  if ($licenses.Count -gt 0) { $component["licenses"] = $licenses }
  $component
})

if (-not (Test-Path node_modules)) {
  throw "node_modules is missing; the SBOM needs the installed frontend dependencies. Run bun install --frozen-lockfile first."
}
$npm = @($package.dependencies.PSObject.Properties.Name | Sort-Object | ForEach-Object {
  $name = $_
  $installed = Join-Path "node_modules" (Join-Path $name "package.json")
  if (-not (Test-Path $installed)) { throw "node_modules/$name is missing; run bun install --frozen-lockfile first." }
  $entry = Get-Content $installed -Raw | ConvertFrom-Json
  $license = if ($entry.license) { $entry.license } elseif ($entry.licenses) { $entry.licenses } else { $null }
  if ($license -is [array]) {
    $license = ($license | ForEach-Object { if ($_ -is [string]) { $_ } else { $_.type } } | Where-Object { $_ }) -join " OR "
  } elseif ($license -isnot [string] -and $license.type) {
    $license = $license.type
  }
  $component = [ordered]@{ type = "library"; name = $name; version = $entry.version; purl = "pkg:npm/$name@$($entry.version)" }
  $licenses = @(Get-Licenses $license)
  if ($licenses.Count -gt 0) { $component["licenses"] = $licenses }
  $component
})

$components = @($crates) + @($npm) + @(
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
