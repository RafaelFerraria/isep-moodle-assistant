@echo off
setlocal
chcp 65001 >nul
cd /d "%~dp0"
python configurar_resumo.py
echo.
pause
endlocal
