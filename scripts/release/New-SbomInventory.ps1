#Requires -Version 5.1
<#
.SYNOPSIS
  Write a lightweight SBOM-style dependency inventory for MediaDeck RC packaging.

.DESCRIPTION
  Emits JSON with Rust crate metadata (via cargo metadata) and frontend package
  dependencies from package.json / pnpm-lock.yaml presence. This is intentionally
  offline-friendly and does not require cargo-cyclonedx.
#>
[CmdletBinding()]
param(
  [string]$RepoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..\..")).Path,
  [string]$OutputPath = ""
)

$ErrorActionPreference = "Stop"

if (-not $OutputPath) {
  $OutputPath = Join-Path $RepoRoot "docs\releases\sbom-inventory.json"
}

$tauriDir = Join-Path $RepoRoot "src-tauri"
$packageJsonPath = Join-Path $RepoRoot "package.json"
$packageJson = Get-Content -Raw -Path $packageJsonPath | ConvertFrom-Json

Push-Location $tauriDir
try {
  $cargoMetadataRaw = cargo metadata --format-version 1 --no-deps 2>$null
  if ($LASTEXITCODE -ne 0) {
    # Fall back to full metadata when --no-deps is unavailable or fails.
    $cargoMetadataRaw = cargo metadata --format-version 1
  }
  $cargoMetadata = $cargoMetadataRaw | ConvertFrom-Json
}
finally {
  Pop-Location
}

$rustPackages = @()
foreach ($pkg in $cargoMetadata.packages) {
  if ($pkg.name -eq "media-deck" -or $pkg.source) {
    $rustPackages += [ordered]@{
      name = $pkg.name
      version = $pkg.version
      license = $pkg.license
      id = $pkg.id
      source = $pkg.source
    }
  }
}

# Prefer the workspace package graph when --no-deps omitted transitive crates.
if ($rustPackages.Count -le 1) {
  Push-Location $tauriDir
  try {
    $cargoMetadata = (cargo metadata --format-version 1 | ConvertFrom-Json)
  }
  finally {
    Pop-Location
  }
  $rustPackages = @()
  foreach ($pkg in $cargoMetadata.packages) {
    $rustPackages += [ordered]@{
      name = $pkg.name
      version = $pkg.version
      license = $pkg.license
      id = $pkg.id
      source = $pkg.source
    }
  }
}

$frontendDeps = @()
foreach ($name in @($packageJson.dependencies.PSObject.Properties.Name)) {
  $frontendDeps += [ordered]@{
    name = $name
    version = $packageJson.dependencies.$name
    scope = "dependencies"
  }
}
foreach ($name in @($packageJson.devDependencies.PSObject.Properties.Name)) {
  $frontendDeps += [ordered]@{
    name = $name
    version = $packageJson.devDependencies.$name
    scope = "devDependencies"
  }
}

$inventory = [ordered]@{
  schema = "mediadeck.sbom-inventory/1"
  generatedAt = (Get-Date).ToUniversalTime().ToString("o")
  product = [ordered]@{
    name = "MediaDeck"
    version = $packageJson.version
    identifier = "com.mediadeck.desktop"
    target = "x86_64-pc-windows-msvc"
  }
  notes = @(
    "Updater plugin is not shipped; automatic update channel is disabled for RC.",
    "Repeat docs/DEPENDENCY_AUDIT.md commands before publishing an RC build.",
    "User data lives under %LOCALAPPDATA%\MediaDeck and is retained across uninstall."
  )
  rust = [ordered]@{
    packageCount = $rustPackages.Count
    packages = $rustPackages
  }
  frontend = [ordered]@{
    packageManager = $packageJson.packageManager
    packageCount = $frontendDeps.Count
    packages = $frontendDeps
  }
}

$directory = Split-Path -Parent $OutputPath
if ($directory) {
  New-Item -ItemType Directory -Force -Path $directory | Out-Null
}

$json = $inventory | ConvertTo-Json -Depth 8
Set-Content -Path $OutputPath -Value $json -Encoding utf8
Write-Host "Wrote $OutputPath ($($rustPackages.Count) Rust packages, $($frontendDeps.Count) frontend packages)"
