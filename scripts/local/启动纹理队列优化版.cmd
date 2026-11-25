@echo off
powershell.exe -NoProfile -ExecutionPolicy Bypass -File "%~dp0start-fixed.ps1" -TextureQueue -ManualPlay %*
exit /b %errorlevel%
