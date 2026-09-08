@echo off
rem Sakura LLM Gateway — 一键启动脚本 (Windows)
setlocal
cd /d "%~dp0"

set "BIN=target\release\llm-gateway.exe"

if not exist "%BIN%" (
  echo ==^> 未找到 release 产物，先执行部署（cargo build --release）...
  where cargo >nul 2>nul
  if errorlevel 1 (
    echo     未找到 cargo，请先运行 install.bat
    pause
    exit /b 1
  )
  cargo build --release
  if errorlevel 1 (
    echo 构建失败，请检查上方错误信息。
    pause
    exit /b 1
  )
)

if not exist gateway.json (
  if exist test\gateway.json (
    set "CONFIG=test\gateway.json"
  ) else (
    echo 未找到 gateway.json，请先运行 install.bat 生成默认配置
    pause
    exit /b 1
  )
) else (
  set "CONFIG=gateway.json"
)

echo ==^> 启动 Sakura LLM Gateway（配置: %CONFIG%）
"%BIN%" --config "%CONFIG%"
pause
