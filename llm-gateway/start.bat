@echo off
rem Sakura LLM Gateway - one-click launcher (Windows)
setlocal
cd /d "%~dp0"

rem Ensure the GNU linker is available for the rust GNU toolchain
where dlltool >nul 2>nul
if errorlevel 1 (
  if exist "%USERPROFILE%\.mingw64\bin\dlltool.exe" (
    set "PATH=%USERPROFILE%\.mingw64\bin;%PATH%"
  ) else (
    echo [setup] Rust GNU toolchain needs MinGW-w64. Please run install.bat first.
    pause
    exit /b 1
  )
)

set "BIN=target\release\llm-gateway.exe"

if not exist "%BIN%" (
  echo [start] Release binary not found. Building it now; first build takes a few minutes...
  where cargo >nul 2>nul
  if errorlevel 1 (
    echo [start] cargo not found. Please run install.bat first.
    pause
    exit /b 1
  )
  call cargo build --release
  if errorlevel 1 (
    echo [start] Build failed. See the errors above.
    pause
    exit /b 1
  )
)

set "CONFIG=gateway.json"
if not exist "%CONFIG%" (
  if exist test\gateway.json (
    set "CONFIG=test\gateway.json"
  ) else (
    echo [start] No gateway.json found. Please run install.bat first.
    pause
    exit /b 1
  )
)

echo [start] Starting Sakura LLM Gateway  config: %CONFIG%
echo [start] API: http://127.0.0.1:8000/v1
echo [start] UI:  http://127.0.0.1:8001/
echo [start] Press Ctrl+C to stop.
"%BIN%" --config "%CONFIG%"
pause
