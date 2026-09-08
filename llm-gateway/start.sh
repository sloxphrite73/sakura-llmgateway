#!/usr/bin/env bash
# Sakura LLM Gateway - one-click launcher (Linux / macOS / Git Bash)
# Builds if needed, then starts the gateway.
set -e
cd "$(dirname "$0")"

BIN="target/release/llm-gateway"
IS_WIN=0
case "$(uname -s)" in
  MINGW*|MSYS*|CYGWIN*) BIN="target/release/llm-gateway.exe"; IS_WIN=1 ;;
esac

# Under Git Bash on Windows the rust GNU toolchain needs MinGW's dlltool.
if [ "$IS_WIN" = "1" ]; then
  if ! command -v dlltool >/dev/null 2>&1; then
    if [ -x "$USERPROFILE/.mingw64/bin/dlltool.exe" ]; then
      export PATH="$USERPROFILE/.mingw64/bin:$PATH"
    else
      echo "[start] Rust GNU toolchain needs MinGW-w64. Please run ./install.sh first."
      exit 1
    fi
  fi
fi

if [ ! -f "$BIN" ]; then
  echo "[start] Release binary not found. Building it now; first build takes a few minutes..."
  if ! command -v cargo >/dev/null 2>&1; then
    echo "[start] cargo not found. Please run ./install.sh first."
    exit 1
  fi
  cargo build --release
fi

CONFIG="gateway.json"
[ -f "$CONFIG" ] || CONFIG="test/gateway.json"
if [ ! -f "$CONFIG" ]; then
  echo "[start] No gateway.json found. Please run ./install.sh first."
  exit 1
fi

echo "[start] Starting Sakura LLM Gateway  config: $CONFIG"
echo "[start] API: http://127.0.0.1:8000/v1"
echo "[start] UI:  http://127.0.0.1:8001/"
exec "$BIN" --config "$CONFIG"
