<#
.SYNOPSIS
    Assemble le paquet distribuable final dans tidenative\dist\TideNative-Windows-x64\
    et génère l'archive ZIP prête à partager ou installer.
#>
$ErrorActionPreference = 'Stop'
$RepoRoot = Split-Path -Parent (Split-Path -Parent $MyInvocation.MyCommand.Path)
$InstallerDir = Join-Path $RepoRoot "installer"
$DistDir = Join-Path $RepoRoot "dist\TideNative-Windows-x64"
$TargetReleaseDir = "C:\Users\KEDI-ADMIN\AppData\Local\tidenative-target\release"
$TargetExe = Join-Path $TargetReleaseDir "TideNative.exe"
$TargetWebViewDll = Join-Path $TargetReleaseDir "WebView2Loader.dll"

Push-Location $RepoRoot
try {
    Write-Host "[1/3] Build du frontend React 120 Hz (ui/dist)..." -ForegroundColor Cyan
    pnpm --dir ui build
    if ($LASTEXITCODE -ne 0) { throw "Échec pnpm --dir ui build" }

    Write-Host "[2/3] Build du binaire natif Tauri v2 Rust (release)..." -ForegroundColor Cyan
    cargo build --release -p tidenative-tauri --bin TideNative
    if ($LASTEXITCODE -ne 0) { throw "Échec cargo build --release" }
} finally {
    Pop-Location
}

Write-Host "[3/3] Assemblage du paquet Windows x64..." -ForegroundColor Cyan
if (Test-Path $DistDir) { Remove-Item -Recurse -Force $DistDir }
New-Item -ItemType Directory -Force -Path $DistDir | Out-Null

Copy-Item -Path $TargetExe -Destination (Join-Path $DistDir "TideNative.exe") -Force
if (Test-Path $TargetWebViewDll) {
    Copy-Item -Path $TargetWebViewDll -Destination (Join-Path $DistDir "WebView2Loader.dll") -Force
}
foreach ($f in @("TideNative.ico", "Install-TideNative.ps1", "Uninstall-TideNative.ps1", "TideNative-Setup.cmd", "LISEZMOI.txt")) {
    Copy-Item -Path (Join-Path $InstallerDir $f) -Destination (Join-Path $DistDir $f) -Force
}

$ZipPath = Join-Path $RepoRoot "dist\TideNative-0.1.0-Windows-x64.zip"
if (Test-Path $ZipPath) { Remove-Item -Force $ZipPath }
Compress-Archive -Path (Join-Path $DistDir "*") -DestinationPath $ZipPath -CompressionLevel Optimal

$ExeMB = [Math]::Round((Get-Item (Join-Path $DistDir "TideNative.exe")).Length / 1MB, 2)
$ZipMB = [Math]::Round((Get-Item $ZipPath).Length / 1MB, 2)
Write-Host "Distribution assemblée :" -ForegroundColor Green
Write-Host "  - Dossier portable + setup : $DistDir ($ExeMB Mo)" -ForegroundColor White
Write-Host "  - Archive ZIP              : $ZipPath ($ZipMB Mo)" -ForegroundColor White
