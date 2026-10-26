@echo off
chcp 65001 > nul
title Configuração - Moodle ISEP Sync
echo ======================================================
echo    Configurador do Sincronizador Moodle ISEP
echo ======================================================
echo.
python "%~dp0moodle_sync.py" --setup
echo.
pause
