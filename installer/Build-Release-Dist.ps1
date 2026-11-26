<#
.SYNOPSIS
    Construit TideNative (frontend React 120 Hz + binaire Rust Tauri v2)
    et génère l'installateur moderne Velopack (1 clic + splash OLED + mises à jour delta).
.EXAMPLE
    powershell -NoProfile -File installer/Build-Release-Dist.ps1
    powershell -NoProfile -File installer/Build-Release-Dist.ps1 -Version 0.2.0 -ReleaseNotes CHANGELOG.md
#>
param(
    [string]$Version = "",
    [string]$ReleaseNotes = "",
    [string]$Channel = "win"
)

$ErrorActionPreference = 'Stop'
$RepoRoot = Split-Path -Parent (Split-Path -Parent $MyInvocation.MyCommand.Path)
$InstallerDir = Join-Path $RepoRoot "installer"
$StageDir = Join-Path $RepoRoot "dist\velopack-stage"
$ReleasesDir = Join-Path $RepoRoot "dist\velopack-releases"
$TargetReleaseDir = "C:\Users\KEDI-ADMIN\AppData\Local\tidenative-target\release"
$TargetExe = Join-Path $TargetReleaseDir "TideNative.exe"
$TargetWebViewDll = Join-Path $TargetReleaseDir "WebView2Loader.dll"

if ([string]::IsNullOrWhiteSpace($Version)) {
    $tauriConf = Get-Content (Join-Path $RepoRoot "app-tauri\tauri.conf.json") -Raw | ConvertFrom-Json
    $Version = $tauriConf.version
}

function Resolve-VpkRunner {
    $globalVpk = Get-Command vpk -ErrorAction SilentlyContinue
    if ($globalVpk) {
        return @{ Exe = $globalVpk.Source; PrefixArgs = @() }
    }

    $vpkVersion = "1.2.161"
    $toolDir = Join-Path $env:LOCALAPPDATA "tidenative-tools\vpk-$vpkVersion"
    $vpkDll = Join-Path $toolDir "tools\net8.0\any\vpk.dll"

    if (-not (Test-Path $vpkDll)) {
        Write-Host "Téléchargement de Velopack CLI (vpk $vpkVersion)..." -ForegroundColor Yellow
        New-Item -ItemType Directory -Force -Path $toolDir | Out-Null
        $nupkgPath = Join-Path $toolDir "vpk.nupkg"
        $url = "https://github.com/velopack/velopack/releases/download/$vpkVersion/vpk.$vpkVersion.nupkg"
        & curl.exe -fsSL $url -o $nupkgPath
        if ($LASTEXITCODE -ne 0) { throw "Échec du téléchargement de vpk $vpkVersion" }
        Add-Type -AssemblyName System.IO.Compression.FileSystem
        [System.IO.Compression.ZipFile]::ExtractToDirectory($nupkgPath, $toolDir)
        Remove-Item $nupkgPath -Force
    }

    if (-not (Test-Path $vpkDll)) {
        throw "vpk.dll introuvable dans $toolDir"
    }
    return @{ Exe = "dotnet"; PrefixArgs = @($vpkDll) }
}

Push-Location $RepoRoot
try {
    Write-Host "[1/4] Build du frontend React 120 Hz (ui/dist)..." -ForegroundColor Cyan
    pnpm --dir ui build
    if ($LASTEXITCODE -ne 0) { throw "Échec pnpm --dir ui build" }

    Write-Host "[2/4] Build du binaire natif Tauri v2 Rust (release)..." -ForegroundColor Cyan
    cargo build --release -p tidenative-tauri --bin TideNative
    if ($LASTEXITCODE -ne 0) { throw "Échec cargo build --release" }
} finally {
    Pop-Location
}

Write-Host "[3/4] Préparation du répertoire de staging Velopack..." -ForegroundColor Cyan
if (Test-Path $StageDir) { Remove-Item -Recurse -Force $StageDir }
New-Item -ItemType Directory -Force -Path $StageDir | Out-Null
New-Item -ItemType Directory -Force -Path $ReleasesDir | Out-Null

Copy-Item -Path $TargetExe -Destination (Join-Path $StageDir "TideNative.exe") -Force
if (Test-Path $TargetWebViewDll) {
    Copy-Item -Path $TargetWebViewDll -Destination (Join-Path $StageDir "WebView2Loader.dll") -Force
}

Write-Host "[4/4] Packaging de l'installateur Velopack v$Version..." -ForegroundColor Cyan
$vpk = Resolve-VpkRunner
$iconPath = Join-Path $InstallerDir "TideNative.ico"
$splashPath = Join-Path $InstallerDir "splash.png"

$packArgs = @()
$packArgs += $vpk.PrefixArgs
$packArgs += @(
    "pack",
    "--packId", "TideNative",
    "--packVersion", $Version,
    "--packTitle", "TideNative",
    "--packAuthors", "TideNative",
    "--packDir", $StageDir,
    "--mainExe", "TideNative.exe",
    "--runtime", "win-x64",
    "--channel", $Channel,
    "--outputDir", $ReleasesDir,
    "--icon", $iconPath,
    "--splashImage", $splashPath,
    "--splashProgressColor", "#33F3FF",
    "--shortcuts", "Desktop,StartMenuRoot",
    "--delta", "BestSpeed"
)

if (-not [string]::IsNullOrWhiteSpace($ReleaseNotes) -and (Test-Path $ReleaseNotes)) {
    $packArgs += @("--releaseNotes", (Resolve-Path $ReleaseNotes).Path)
}

& $vpk.Exe @packArgs
if ($LASTEXITCODE -ne 0) { throw "Échec vpk pack" }

$setupExe = Join-Path $ReleasesDir "TideNative-$Channel-Setup.exe"
$distSetupCopy = Join-Path $RepoRoot "dist\TideNative-Setup.exe"
if (Test-Path $setupExe) {
    Copy-Item -Path $setupExe -Destination $distSetupCopy -Force
    $setupMB = [Math]::Round((Get-Item $setupExe).Length / 1MB, 2)
    Write-Host ""
    Write-Host "Installateur Velopack prêt :" -ForegroundColor Green
    Write-Host "  - Setup 1-clic       : $distSetupCopy ($setupMB Mo)" -ForegroundColor White
    Write-Host "  - Dossier Release    : $ReleasesDir" -ForegroundColor White
    Write-Host "  - Pour publier sur GitHub Releases :" -ForegroundColor Cyan
    Write-Host "    gh release create v$Version `"$ReleasesDir\*`" --title `"TideNative v$Version`"" -ForegroundColor Gray
}
