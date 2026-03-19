@echo off
:: ─────────────────────────────────────────────────────────────────────────────
:: ItsyHome – Dev / Test session
:: Avvia Vite HMR + Tauri dev in un'unica finestra.
:: Chiudi la finestra per fermare entrambi i processi.
:: ─────────────────────────────────────────────────────────────────────────────
title ItsyHome Dev

:: Trova cargo nella posizione standard
set "PATH=%PATH%;%USERPROFILE%\.cargo\bin"

echo.
echo  ╔══════════════════════════════════════════╗
echo  ║   ItsyHome — sessione di test (dev)      ║
echo  ║   Ctrl+C  per fermare                    ║
echo  ╚══════════════════════════════════════════╝
echo.

cd /d "%~dp0"

:: Controlla che le dipendenze npm siano installate
if not exist "node_modules\" (
    echo [setup] Installazione dipendenze npm...
    npm install
)

:: Avvia tauri dev (compila Rust + Vite in un unico processo)
echo [dev] Avvio Tauri dev...
npx tauri dev

pause
