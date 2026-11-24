@echo off
title Installation de TideNative (TIDAL Natif Bit-Perfect)
powershell.exe -NoProfile -ExecutionPolicy Bypass -File "%~dp0Install-TideNative.ps1" -LaunchAfterInstall
if errorlevel 1 (
    echo.
    echo Une erreur s'est produite lors de l'installation.
    pause
)
