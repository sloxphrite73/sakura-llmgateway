@echo off
rem Sakura LLM Gateway - one-click installer (Windows)
setlocal
cd /d "%~dp0"

echo [1/4] Checking Rust toolchain...
where cargo >nul 2>nul
if errorlevel 1 (
  echo      cargo not found. Installing rustup at user level, no admin needed...
  if not exist "%TEMP%\rustup-init.exe" (
    curl -Lo "%TEMP%\rustup-init.exe" https://static.rust-lang.org/rustup/dist/x86_64-pc-windows-msvc/rustup-init.exe
  )
  "%TEMP%\rustup-init.exe" -y --default-toolchain stable-x86_64-pc-windows-gnu
  set "PATH=%USERPROFILE%\.cargo\bin;%PATH%"
)
cargo --version
if errorlevel 1 (
  echo      cargo is still unavailable. Check the error above.
  pause
  exit /b 1
)

echo [2/4] Checking MinGW-w64 linker...
where dlltool >nul 2>nul
if errorlevel 1 (
  if exist "%USERPROFILE%\.mingw64\bin\dlltool.exe" (
    set "PATH=%USERPROFILE%\.mingw64\bin;%PATH%"
    echo      Using existing MinGW at %%USERPROFILE%%\.mingw64.
  ) else (
    echo      GNU toolchain needs MinGW-w64. Downloading a portable build...
    if not exist "%TEMP%\mingw64.zip" (
      curl -L -o "%TEMP%\mingw64.zip" "https://github.com/brechtsanders/winlibs_mingw/releases/download/16.2.0posix-14.0.0-ucrt-r1/winlibs-x86_64-posix-seh-gcc-16.2.0-mingw-w64ucrt-14.0.0-r1.zip"
    )
    if errorlevel 1 (
      echo      Download failed. Install MinGW-w64 manually and add its bin to PATH.
      echo      Download page: https://winlibs.com/
      pause
      exit /b 1
    )
    echo      Extracting MinGW-w64...
    powershell -NoProfile -Command "Expand-Archive -Force '%TEMP%\mingw64.zip' '%TEMP%'"
    if exist "%TEMP%\mingw64\bin\dlltool.exe" (
      xcopy /E /I /Y "%TEMP%\mingw64" "%USERPROFILE%\.mingw64" >nul
    )
    if exist "%USERPROFILE%\.mingw64\bin\dlltool.exe" (
      set "PATH=%USERPROFILE%\.mingw64\bin;%PATH%"
    ) else (
      echo      Could not locate dlltool after extraction. Install MinGW-w64 manually.
      pause
      exit /b 1
    )
  )
)
dlltool --version

echo [3/4] Building release binary; first build takes a few minutes...
call cargo build --release
if errorlevel 1 (
  echo      Build failed. See the errors above.
  pause
  exit /b 1
)

echo [4/4] Preparing config file...
if not exist gateway.json (
  (
    echo {
    echo   "api_port": 8000,
    echo   "ui_port": 8001,
    echo   "default_cooldown_secs": 60,
    echo   "max_attempts": 3,
    echo   "auth": { "enabled": false, "keys": [] },
    echo   "providers": []
    echo }
  ) > gateway.json
  echo      Created default gateway.json. Open it to add your providers and keys.
) else (
  echo      gateway.json already exists, skipping.
)

echo.
echo Install complete.
echo   Run:     start.bat   or target\release\llm-gateway.exe
echo   API:     http://127.0.0.1:8000/v1
echo   Console: http://127.0.0.1:8001/
pause
