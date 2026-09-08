#!/usr/bin/env bash
# Sakura LLM Gateway — 一键启动脚本 (Linux / macOS / Git Bash)
# 若尚未构建则自动构建，然后启动网关
set -e
cd "$(dirname "$0")"

BIN="target/release/llm-gateway"
if [ "$(uname -s)" = "MINGW"* ] || [ "$(uname -s)" = "MSYS"* ] || [ "$(uname -s)" = "CYGWIN"* ]; then
  BIN="target/release/llm-gateway.exe"
fi

if [ ! -f "$BIN" ]; then
  echo "==> 未找到 release 产物，先执行部署（cargo build --release）..."
  if ! command -v cargo >/dev/null 2>&1; then
    echo "    未找到 cargo，请先运行 ./install.sh"
    exit 1
  fi
  cargo build --release
fi

if [ ! -f gateway.json ] && [ ! -f test/gateway.json ]; then
  echo "==> 未找到 gateway.json，请先运行 ./install.sh 生成默认配置"
  exit 1
fi

CONFIG="gateway.json"
[ -f gateway.json ] || CONFIG="test/gateway.json"

echo "==> 启动 Sakura LLM Gateway（配置: $CONFIG）"
exec "$BIN" --config "$CONFIG"
