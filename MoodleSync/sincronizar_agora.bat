@echo off
chcp 65001 > nul
title Sincronização Manual - Moodle ISEP
echo ======================================================
echo    A sincronizar todas as disciplinas do Moodle ISEP...
echo ======================================================
echo.
python "%~dp0moodle_sync.py" --run
echo.
echo Concluído! Pressiona qualquer tecla para fechar.
pause > nul
