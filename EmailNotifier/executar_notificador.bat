@echo off
setlocal
cd /d "%~dp0"

set "PY_CMD="
where python >nul 2>nul
if %errorlevel% equ 0 set "PY_CMD=python"

if not defined PY_CMD (
    where py >nul 2>nul
    if %errorlevel% equ 0 set "PY_CMD=py -3"
)

if not defined PY_CMD (
    for %%V in (313 312 311 310 39) do (
        if not defined PY_CMD (
            if exist "%LOCALAPPDATA%\Programs\Python\Python%%V\python.exe" (
                set "PY_CMD=%LOCALAPPDATA%\Programs\Python\Python%%V\python.exe"
            )
        )
    )
)

if not defined PY_CMD (
    for %%V in (313 312 311 310 39) do (
        if not defined PY_CMD (
            if exist "%ProgramFiles%\Python%%V\python.exe" (
                set "PY_CMD=%ProgramFiles%\Python%%V\python.exe"
            )
        )
    )
)

if not defined PY_CMD set "PY_CMD=python"

"%PY_CMD%" main.py >> notifier.log 2>&1
endlocal
