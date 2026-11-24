<#
.SYNOPSIS
    Installateur Windows clé en main pour TideNative (per-user, sans élévation UAC requise).
.DESCRIPTION
    Copie TideNative.exe, son manifeste DPI Windows 11 et son icône dans
    %LOCALAPPDATA%\Programs\TideNative, crée les raccourcis Menu Démarrer et Bureau,
    et enregistre TideNative dans Paramètres Windows > Applications installées.
#>
[CmdletBinding()]
param(
    [switch]$NoDesktopShortcut,
    [switch]$LaunchAfterInstall
)

$ErrorActionPreference = 'Stop'
$AppName = "TideNative"
$AppVersion = "0.1.0"
$Publisher = "TideNative Audio"
$SourceDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$InstallDir = Join-Path $env:LOCALAPPDATA "Programs\$AppName"
$DataDir = Join-Path $env:LOCALAPPDATA $AppName

Write-Host "====================================================" -ForegroundColor Cyan
Write-Host "  Installation de $AppName $AppVersion (Windows 10/11)" -ForegroundColor Cyan
Write-Host "  Client TIDAL 100 % Natif Rust - WASAPI Bit-Perfect" -ForegroundColor DarkCyan
Write-Host "====================================================" -ForegroundColor Cyan

$SourceExe = Join-Path $SourceDir "TideNative.exe"
if (-not (Test-Path $SourceExe)) {
    $FallbackDist = Join-Path (Split-Path -Parent $SourceDir) "dist\TideNative-Windows-x64"
    if (Test-Path (Join-Path $FallbackDist "TideNative.exe")) {
        $SourceDir = $FallbackDist
        $SourceExe = Join-Path $SourceDir "TideNative.exe"
    } else {
        throw "Introuvable : $SourceExe. Veuillez lancer l'installateur depuis dist\TideNative-Windows-x64\ ou exécuter installer\Build-Release-Dist.ps1."
    }
}

# Ferme proprement une éventuelle instance en cours
Get-Process -Name "TideNative" -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
Start-Sleep -Milliseconds 300

New-Item -ItemType Directory -Force -Path $InstallDir | Out-Null
New-Item -ItemType Directory -Force -Path $DataDir | Out-Null

Write-Host "[1/4] Copie des fichiers dans $InstallDir..." -ForegroundColor White
$LegacyManifest = Join-Path $InstallDir "TideNative.exe.manifest"
if (Test-Path $LegacyManifest) { Remove-Item -Force $LegacyManifest -ErrorAction SilentlyContinue }
Copy-Item -Path $SourceExe -Destination (Join-Path $InstallDir "TideNative.exe") -Force
foreach ($extra in @("WebView2Loader.dll", "TideNative.ico", "Uninstall-TideNative.ps1", "LISEZMOI.txt")) {
    $src = Join-Path $SourceDir $extra
    if (Test-Path $src) {
        Copy-Item -Path $src -Destination (Join-Path $InstallDir $extra) -Force
    }
}

$InstalledExe = Join-Path $InstallDir "TideNative.exe"
$InstalledIcon = Join-Path $InstallDir "TideNative.ico"
$InstalledUninstaller = Join-Path $InstallDir "Uninstall-TideNative.ps1"

Write-Host "[2/4] Création du raccourci Menu Démarrer..." -ForegroundColor White
$WshShell = New-Object -ComObject WScript.Shell
$StartMenuDir = Join-Path $env:APPDATA "Microsoft\Windows\Start Menu\Programs"
$StartMenuLnk = Join-Path $StartMenuDir "$AppName.lnk"
$sc = $WshShell.CreateShortcut($StartMenuLnk)
$sc.TargetPath = $InstalledExe
$sc.WorkingDirectory = $InstallDir
$sc.Description = "TideNative - Client TIDAL Bit-Perfect WASAPI Exclusif"
if (Test-Path $InstalledIcon) { $sc.IconLocation = "$InstalledIcon,0" }
$sc.Save()

if (-not $NoDesktopShortcut) {
    Write-Host "[3/4] Création du raccourci Bureau..." -ForegroundColor White
    $DesktopDir = [Environment]::GetFolderPath("Desktop")
    $DesktopLnk = Join-Path $DesktopDir "$AppName.lnk"
    $dsc = $WshShell.CreateShortcut($DesktopLnk)
    $dsc.TargetPath = $InstalledExe
    $dsc.WorkingDirectory = $InstallDir
    $dsc.Description = "TideNative - Client TIDAL Bit-Perfect WASAPI Exclusif"
    if (Test-Path $InstalledIcon) { $dsc.IconLocation = "$InstalledIcon,0" }
    $dsc.Save()
}

Write-Host "[4/4] Enregistrement dans Windows (Applications installées)..." -ForegroundColor White
$UninstallKey = "HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\$AppName"
New-Item -Path $UninstallKey -Force | Out-Null
$SizeKB = [int]([Math]::Ceiling((Get-Item $InstalledExe).Length / 1KB))

Set-ItemProperty -Path $UninstallKey -Name "DisplayName" -Value $AppName
Set-ItemProperty -Path $UninstallKey -Name "DisplayVersion" -Value $AppVersion
Set-ItemProperty -Path $UninstallKey -Name "Publisher" -Value $Publisher
Set-ItemProperty -Path $UninstallKey -Name "InstallLocation" -Value $InstallDir
Set-ItemProperty -Path $UninstallKey -Name "DisplayIcon" -Value $(if (Test-Path $InstalledIcon) { $InstalledIcon } else { $InstalledExe })
Set-ItemProperty -Path $UninstallKey -Name "UninstallString" -Value "powershell.exe -NoProfile -ExecutionPolicy Bypass -File `"$InstalledUninstaller`""
Set-ItemProperty -Path $UninstallKey -Name "NoModify" -Value 1 -Type DWord
Set-ItemProperty -Path $UninstallKey -Name "NoRepair" -Value 1 -Type DWord
Set-ItemProperty -Path $UninstallKey -Name "EstimatedSize" -Value $SizeKB -Type DWord

Write-Host ""
Write-Host "Installation terminée avec succès !" -ForegroundColor Green
Write-Host "  - Exécutable : $InstalledExe" -ForegroundColor Gray
Write-Host "  - Données    : $DataDir" -ForegroundColor Gray
Write-Host "  - Raccourci  : Menu Démarrer > $AppName" -ForegroundColor Gray

if ($LaunchAfterInstall) {
    Start-Process -FilePath $InstalledExe -WorkingDirectory $InstallDir
}
