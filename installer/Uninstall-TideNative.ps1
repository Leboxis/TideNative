<#
.SYNOPSIS
    Désinstallateur propre de TideNative (conserve les préférences/cache sauf si -PurgeData).
#>
[CmdletBinding()]
param(
    [switch]$PurgeData
)

$ErrorActionPreference = 'SilentlyContinue'
$AppName = "TideNative"
$InstallDir = Join-Path $env:LOCALAPPDATA "Programs\$AppName"
$DataDir = Join-Path $env:LOCALAPPDATA $AppName

Write-Host "Désinstallation de $AppName..." -ForegroundColor Yellow

Get-Process -Name "TideNative" | Stop-Process -Force
Start-Sleep -Milliseconds 300

Remove-Item -Path (Join-Path $env:APPDATA "Microsoft\Windows\Start Menu\Programs\$AppName.lnk") -Force
Remove-Item -Path (Join-Path ([Environment]::GetFolderPath("Desktop")) "$AppName.lnk") -Force
Remove-Item -Path "HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\$AppName" -Recurse -Force

if (Test-Path $InstallDir) {
    Remove-Item -Path $InstallDir -Recurse -Force
}

if ($PurgeData -and (Test-Path $DataDir)) {
    Remove-Item -Path $DataDir -Recurse -Force
}

Write-Host "$AppName a été désinstallé proprement." -ForegroundColor Green
