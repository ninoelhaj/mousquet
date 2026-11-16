@echo off
rem mousquet // cayo perico - mode chasse : FiveM sans tes mods ni ton pack, tout revient a la fermeture.
setlocal
if not defined MQ_CONHOST (
    set "MQ_CONHOST=1"
    start "" conhost.exe cmd /c ""%~f0""
    exit /b
)
title MOUSQUET // CAYO PERICO
mode con: cols=52 lines=25 >nul 2>&1
cd /d "%~dp0"
set "PY=%LOCALAPPDATA%\Programs\Python\Python312\python.exe"
if not exist "%PY%" set "PY=python"
"%PY%" -X utf8 mousquet.py
if errorlevel 1 pause
