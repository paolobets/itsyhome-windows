@echo off
set PATH=%USERPROFILE%\.cargo\bin;%PATH%
cd /d C:\Users\Betse\itsyhome-windows
echo [1/3] Building frontend (Vite)...
call npm run build:vite
if errorlevel 1 ( echo Vite build FAILED & exit /b 1 )
echo [2/3] Building Rust binary...
cd src-tauri
cargo build --release
if errorlevel 1 ( echo Cargo build FAILED & exit /b 1 )
cd ..
echo [3/3] Bundling MSI + NSIS...
call npx tauri bundle
if errorlevel 1 ( echo Bundle FAILED & exit /b 1 )
echo.
echo Done. Installers in src-tauri\target\release\bundle\
