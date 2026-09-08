@echo off
rem Sakura LLM Gateway — 一键部署脚本 (Windows)
setlocal
cd /d "%~dp0"

echo ==^> [1/3] 检查 Rust 工具链
where cargo >nul 2>nul
if errorlevel 1 (
  echo     未找到 cargo，正在安装 rustup（用户级，无需管理员）...
  if not exist "%TEMP%\rustup-init.exe" (
    curl -Lo "%TEMP%\rustup-init.exe" https://static.rust-lang.org/rustup/dist/x86_64-pc-windows-msvc/rustup-init.exe
  )
  "%TEMP%\rustup-init.exe" -y --default-toolchain stable-x86_64-pc-windows-gnu
  set "PATH=%USERPROFILE%\.cargo\bin;%PATH%"
)
cargo --version

echo ==^> [2/3] 编译 release 版本（首次构建需要几分钟）
cargo build --release
if errorlevel 1 (
  echo 构建失败，请检查上方错误信息。
  pause
  exit /b 1
)

echo ==^> [3/3] 准备配置文件
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
  echo     已生成默认配置 gateway.json（用编辑器添加你的提供商和 Key）
) else (
  echo     已存在 gateway.json，跳过
)

echo.
echo 部署完成 ✔
echo   启动:   start.bat   (或直接运行 target\release\llm-gateway.exe)
echo   API:    http://127.0.0.1:8000/v1
echo   控制台: http://127.0.0.1:8001/
pause
