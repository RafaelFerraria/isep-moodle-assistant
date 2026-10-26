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

rem Preferir o executavel Rust (mais leve) quando existe; senao usar Python.
set "RUST_EXE="
if exist "%~dp0..\server\target\release\isep-assistant-api.exe" set "RUST_EXE=%~dp0..\server\target\release\isep-assistant-api.exe"
if exist "%~dp0..\bin\isep-assistant-api.exe" set "RUST_EXE=%~dp0..\bin\isep-assistant-api.exe"
if defined RUST_EXE (
    "%RUST_EXE%" digest >nul 2>> notifier.log
    goto :done
)
rem o main.py ja escreve em notifier.log; so os erros nao tratados (stderr) vao para la tambem
"%PY_CMD%" main.py >nul 2>> notifier.log
:done
endlocal
