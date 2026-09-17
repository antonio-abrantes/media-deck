#Requires -Version 5.1
<#
.SYNOPSIS
  Build MediaDeck release artifacts and generate checksums + SBOM inventory.

.DESCRIPTION
  Runs `pnpm tauri build`, then writes SHA-256 checksums and a dependency
  inventory under docs/releases/<version>/ (or -OutputDirectory).

  Code signing is optional. When WINDOWS_CERTIFICATE_THUMBPRINT is set in the
  environment and mirrored into tauri.conf.json (or signCommand), Tauri signs
  during the bundle step. This script never embeds certificate secrets.
#>
[CmdletBinding()]
param(
  [string]$Version = "0.1.0",
  [string]$RepoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..\..")).Path,
  [string]$OutputDirectory = "",
  [switch]$SkipBuild
)

$ErrorActionPreference = "Stop"

if (-not $OutputDirectory) {
  $OutputDirectory = Join-Path $RepoRoot "docs\releases\$Version"
}

New-Item -ItemType Directory -Force -Path $OutputDirectory | Out-Null

Push-Location $RepoRoot
try {
  $cargoBin = Join-Path $env:USERPROFILE ".cargo\bin"
  if (Test-Path $cargoBin) {
    $env:Path = "$cargoBin;$env:Path"
  }

  if (-not $SkipBuild) {
    Write-Host "Building MediaDeck $Version (NSIS + release EXE)..."
    corepack pnpm tauri build
  }

  $candidateRoots = @()
  if ($env:CARGO_TARGET_DIR) {
    $candidateRoots += (Join-Path $env:CARGO_TARGET_DIR "release")
  }
  $candidateRoots += (Join-Path $RepoRoot "src-tauri\target\release")

  $exePath = $null
  $nsisDir = $null
  foreach ($root in $candidateRoots) {
    $candidateExe = Join-Path $root "media-deck.exe"
    $candidateNsis = Join-Path $root "bundle\nsis"
    if ((Test-Path $candidateExe) -and (Test-Path $candidateNsis)) {
      $exePath = $candidateExe
      $nsisDir = $candidateNsis
      break
    }
  }

  if (-not $exePath) {
    throw "Release EXE not found. Checked: $($candidateRoots -join '; ')"
  }

  $setupCandidates = @(Get-ChildItem -Path $nsisDir -Filter "MediaDeck_*_x64-setup.exe" -ErrorAction SilentlyContinue)
  if ($setupCandidates.Count -lt 1) {
    throw "NSIS setup EXE not found under $nsisDir"
  }

  $setupPath = $setupCandidates[0].FullName
  $artifactDir = Join-Path $OutputDirectory "artifacts"
  New-Item -ItemType Directory -Force -Path $artifactDir | Out-Null

  Copy-Item -Force $exePath (Join-Path $artifactDir "media-deck.exe")
  Copy-Item -Force $setupPath (Join-Path $artifactDir $setupCandidates[0].Name)

  $checksumPath = Join-Path $OutputDirectory "SHA256SUMS.txt"
  $lines = @()
  Get-ChildItem -Path $artifactDir -File | ForEach-Object {
    $hash = (Get-FileHash -Algorithm SHA256 -Path $_.FullName).Hash.ToLowerInvariant()
    $lines += "$hash  $($_.Name)"
  }
  Set-Content -Path $checksumPath -Value ($lines -join "`n") -Encoding ascii
  Write-Host "Wrote $checksumPath"

  & (Join-Path $PSScriptRoot "New-SbomInventory.ps1") `
    -RepoRoot $RepoRoot `
    -OutputPath (Join-Path $OutputDirectory "sbom-inventory.json")

  Write-Host "Release packaging complete: $OutputDirectory"
  Write-Host "Next: fill RELEASE_MATRIX.md evidence and verify install/uninstall on a clean VM."
}
finally {
  Pop-Location
}
