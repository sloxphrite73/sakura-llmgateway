#!/usr/bin/env bash
# Sakura LLM Gateway — 一键部署脚本 (Linux / macOS / Git Bash)
# 用法: ./install.sh
set -e
cd "$(dirname "$0")"

BIN="target/release/llm-gateway"
if [ "$(uname -s)" = "MINGW"* ] || [ "$(uname -s)" = "MSYS"* ] || [ "$(uname -s)" = "CYGWIN"* ]; then
  BIN="target/release/llm-gateway.exe"
fi

echo "==> [1/3] 检查 Rust 工具链"
if ! command -v cargo >/dev/null 2>&1; then
  echo "    未找到 cargo，正在安装 rustup（用户级，无需管理员）..."
  if [ "$(uname -s)" = "MINGW"* ] || [ "$(uname -s)" = "MSYS"* ] || [ "$(uname -s)" = "CYGWIN"* ]; then
    curl -Lo "$TEMP/rustup-init.exe" https://static.rust-lang.org/rustup/dist/x86_64-pc-windows-msvc/rustup-init.exe
    "$TEMP/rustup-init.exe" -y --default-toolchain stable-x86_64-pc-windows-gnu
    export PATH="$USERPROFILE/.cargo/bin:$PATH"
  else
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
    export PATH="$HOME/.cargo/bin:$PATH"
  fi
fi
echo "    Rust: $(cargo --version)"

echo "==> [2/3] 编译 release 版本（首次构建需要几分钟）"
cargo build --release

echo "==> [3/3] 准备配置文件"
if [ ! -f gateway.json ]; then
  cat > gateway.json <<'EOF'
{
  "api_port": 8000,
  "ui_port": 8001,
  "default_cooldown_secs": 60,
  "max_attempts": 3,
  "auth": { "enabled": false, "keys": [] },
  "providers": []
}
EOF
  echo "    已生成默认配置 gateway.json（用编辑器添加你的提供商和 Key）"
else
  echo "    已存在 gateway.json，跳过"
fi

echo ""
echo "部署完成 ✔"
echo "  启动:   ./start.sh   (或直接运行 $BIN)"
echo "  API:    http://127.0.0.1:8000/v1"
echo "  控制台: http://127.0.0.1:8001/"
