#!/usr/bin/env bash
# Sakura LLM Gateway - one-click installer (Linux / macOS / Git Bash)
set -e
cd "$(dirname "$0")"

BIN="target/release/llm-gateway"
IS_WIN=0
case "$(uname -s)" in
  MINGW*|MSYS*|CYGWIN*) BIN="target/release/llm-gateway.exe"; IS_WIN=1 ;;
esac

echo "[1/3] Checking Rust toolchain"
if ! command -v cargo >/dev/null 2>&1; then
  echo "      cargo not found. Installing rustup at user level, no admin needed..."
  if [ "$IS_WIN" = "1" ]; then
    curl -Lo "$TEMP/rustup-init.exe" https://static.rust-lang.org/rustup/dist/x86_64-pc-windows-msvc/rustup-init.exe
    "$TEMP/rustup-init.exe" -y --default-toolchain stable-x86_64-pc-windows-gnu
    export PATH="$USERPROFILE/.cargo/bin:$PATH"
  else
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
    export PATH="$HOME/.cargo/bin:$PATH"
  fi
fi
echo "      Rust: $(cargo --version)"

# Under Git Bash on Windows the rust GNU toolchain needs MinGW's dlltool.
if [ "$IS_WIN" = "1" ] && ! command -v dlltool >/dev/null 2>&1; then
  if [ -x "$USERPROFILE/.mingw64/bin/dlltool.exe" ]; then
    export PATH="$USERPROFILE/.mingw64/bin:$PATH"
  else
    echo "      GNU toolchain needs MinGW-w64. Please install it and add its bin to PATH."
    echo "      Download page: https://winlibs.com/"
    exit 1
  fi
fi

echo "[2/3] Building release binary; first build takes a few minutes..."
cargo build --release

echo "[3/3] Preparing config file"
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
  echo "      Created default gateway.json. Open it to add your providers and keys."
else
  echo "      gateway.json already exists, skipping."
fi

echo ""
echo "Install complete."
echo "  Run:     ./start.sh   or $BIN"
echo "  API:     http://127.0.0.1:8000/v1"
echo "  Console: http://127.0.0.1:8001/"
