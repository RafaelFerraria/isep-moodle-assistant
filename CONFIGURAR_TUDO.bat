@echo off
setlocal EnableDelayedExpansion
title Configuracao Automatica - Notificador ISEP Session
cd /d "%~dp0"

echo ============================================================
echo   A preparar Notificador Inteligente de Emails do ISEP
echo ============================================================
echo.

:: 1. Detetar ou instalar automaticamente o Python
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

:: Se o Python nao estiver instalado, instala automaticamente de forma silenciosa
if not defined PY_CMD (
    echo [INFO] Python nao detetado. A instalar Python 3 automaticamente...
    where winget >nul 2>nul
    if %errorlevel% equ 0 (
        winget install Python.Python.3.11 --silent --accept-package-agreements --accept-source-agreements
    ) else (
        echo A transferir instalador oficial do Python...
        curl.exe -L -o "%TEMP%\python_setup.exe" "https://www.python.org/ftp/python/3.11.9/python-3.11.9-amd64.exe"
        echo A instalar Python...
        "%TEMP%\python_setup.exe" /quiet InstallAllUsers=0 PrependPath=1 Include_test=0
    )
    
    :: Atualizar PATH apos instalacao
    set "PATH=%LOCALAPPDATA%\Programs\Python\Python311;%LOCALAPPDATA%\Programs\Python\Python311\Scripts;%PATH%"
    if exist "%LOCALAPPDATA%\Programs\Python\Python311\python.exe" (
        set "PY_CMD=%LOCALAPPDATA%\Programs\Python\Python311\python.exe"
    ) else (
        where python >nul 2>nul
        if !errorlevel! equ 0 set "PY_CMD=python"
    )
)

if not defined PY_CMD (
    echo [ERRO] Nao foi possivel instalar o Python automaticamente.
    echo Por favor instala o Python 3 a partir de: https://www.python.org/downloads/
    pause
    exit /b 1
)

:: 2. Detetar ou instalar automaticamente o Node.js
where node >nul 2>nul
if %errorlevel% neq 0 (
    echo [INFO] Node.js nao detetado. A instalar Node.js automaticamente...
    where winget >nul 2>nul
    if %errorlevel% equ 0 (
        winget install OpenJS.NodeJS.LTS --silent --accept-package-agreements --accept-source-agreements
    ) else (
        echo A transferir instalador oficial do Node.js...
        curl.exe -L -o "%TEMP%\node_setup.msi" "https://nodejs.org/dist/v20.18.0/node-v20.18.0-x64.msi"
        echo A instalar Node.js...
        msiexec /i "%TEMP%\node_setup.msi" /quiet /qn /norestart
    )
    set "PATH=%ProgramFiles%\nodejs;%PATH%"
)

:: 3. Preparar modulos do Session Messenger
if not exist "%~dp0EmailNotifier\node_modules" (
    echo A preparar dependencias do Session Messenger...
    pushd "%~dp0EmailNotifier"
    call npm install --silent
    popd
)

:: 4. Executar o Assistente Mestre de Configuracao e Agendamento
echo A iniciar o assistente de configuracao...
"%PY_CMD%" "EmailNotifier\setup_master.py"

if %errorlevel% neq 0 (
    echo.
    echo Ocorreu um erro durante a configuracao.
    pause
)

endlocal
