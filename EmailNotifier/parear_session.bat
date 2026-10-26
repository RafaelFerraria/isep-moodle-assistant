@echo off
setlocal
chcp 65001 >nul
cd /d "%~dp0"
node session_sender.cjs --pair --config config.json
echo.
pause
endlocal
