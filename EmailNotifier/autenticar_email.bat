@echo off
setlocal
chcp 65001 >nul
cd /d "%~dp0"

set "PY_CMD="
where python >nul 2>nul
if %errorlevel% equ 0 set "PY_CMD=python"
if not defined PY_CMD (
    where py >nul 2>nul
    if %errorlevel% equ 0 set "PY_CMD=py -3"
)
if not defined PY_CMD (
    if exist "%LOCALAPPDATA%\Programs\Python\Python39\python.exe" (
        set "PY_CMD=%LOCALAPPDATA%\Programs\Python\Python39\python.exe"
    )
)
if not defined PY_CMD set "PY_CMD=python"

%PY_CMD% autenticar_email.py

echo.
pause
endlocal
