@echo off
powershell.exe -NoProfile -ExecutionPolicy Bypass -File "%~dp0start-fixed.ps1" -Combined -ManualPlay %*
