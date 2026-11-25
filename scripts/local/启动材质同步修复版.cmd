@echo off
powershell.exe -NoProfile -ExecutionPolicy Bypass -File "%~dp0start-fixed.ps1" -MaterialFix %*
exit /b %errorlevel%
