@echo off
rem mousquet : compile mousquet.exe depuis les sources (Rust), puis le pose a cote de ce fichier.
setlocal
cd /d "%~dp0"
title mousquet - compilation

where cargo >/dev/null 2>&1
if errorlevel 1 if exist "%USERPROFILE%\.cargo\bin\cargo.exe" set "PATH=%USERPROFILE%\.cargo\bin;%PATH%"
where cargo >/dev/null 2>&1
if errorlevel 1 (
    echo.
    echo  Rust n'est pas installe.
    echo.
    echo  1. Installe-le depuis https://rustup.rs  ^(rustup-init.exe, choix par defaut^)
    echo     ou dans un terminal :  winget install Rustlang.Rustup
    echo  2. Si l'installeur le demande, installe aussi les "Outils de build C++" de Visual Studio.
    echo  3. Relance compiler.bat.
    echo.
    pause
    exit /b 1
)

echo.
echo  Compilation de mousquet.exe (la premiere fois : quelques minutes)...
echo.
cargo build --release
if errorlevel 1 (
    echo.
    echo  La compilation a echoue : voir les messages au-dessus.
    pause
    exit /b 1
)
copy /y "target\release\mousquet.exe" "mousquet.exe" >nul
if errorlevel 1 (
    echo.
    echo  mousquet.exe est compile dans target\release\ mais n'a pas pu etre copie ici
    echo  ^(mousquet tourne encore ? ferme FiveM puis relance compiler.bat^).
    pause
    exit /b 1
)
echo.
echo  Fini : mousquet.exe est pret dans ce dossier. Double-clique dessus pour lancer.
echo.
pause
