# Sakura LLM Gateway

English | [简体中文](README.zh-CN.md) · 📖 Full usage guide: [doc/USAGE.md](doc/USAGE.md) · 完整使用说明：[doc/使用说明.md](doc/使用说明.md)

A high-performance local LLM API gateway written in **Rust** (axum + tokio). It exposes an
OpenAI-compatible endpoint on `127.0.0.1:8000/v1`, load-balances across a pool of API keys
per provider, and offers a built-in web console for managing providers, keys and aliases.

## Features

- **OpenAI-compatible API** — point any agent/tool at `http://127.0.0.1:8000/v1`
  - `POST /v1/chat/completions` (streaming SSE and non-streaming)
  - `GET  /v1/models` (aggregated from all providers + aliases)
- **API-Key Pool** — round-robin across keys; a key that receives `429` is put into cooldown
  and traffic rotates to the next key immediately
  - cooldown duration: upstream `Retry-After` header wins, else per-key override, else
    global default (60s)
  - when **all** keys are cooling down, the gateway fails fast with `429`
- **Retry before first byte** — 429/5xx/timeouts rotate to the next key (up to
  `max_attempts`, default 3). Once the first response byte has been sent, the upstream
  stream is passed through untouched.
- **Managed models** — per-provider model catalog (`{ id, enabled }`), one-click import
  from the provider's `/v1/models` (with checkboxes), and an optional allowlist mode that
  rejects models not in the list
- **Model routing** — request model as `provider/model` (e.g. `openai/gpt-4o`), or set up
  short **aliases** in the UI (e.g. `fast` → `gpt-4o-mini`)
- **Web console** — `http://127.0.0.1:8001/` to manage providers (baseURL), the key pool
  (add/remove, per-key cooldown, live status), aliases, and settings
- **JSON config** — human-readable `gateway.json`, saved atomically on every change
- **Optional auth** — off by default; enable in the UI to require a gateway-issued
  Bearer key on the local API

## Quick start (one-click)

**Windows:** double-click `install.bat`, then `start.bat`.
**Linux / macOS / Git Bash:**

```bash
./install.sh   # checks/installs Rust, builds release, creates default gateway.json
./start.sh     # builds if needed, then starts the gateway
```

The install script installs rustup at user level (no admin needed) if cargo is missing.

## Build

Requires Rust (GNU toolchain on Windows works):

```bash
cargo build --release
```

## Run

```bash
./target/release/llm-gateway.exe            # config defaults to ./gateway.json
./target/release/llm-gateway.exe --config path/to/gateway.json
```

Ports (API `8000`, UI `8001`) are set in the config file or editable in the UI
(port changes take effect on restart).

## Config format

```json
{
  "api_port": 8000,
  "ui_port": 8001,
  "default_cooldown_secs": 60,
  "max_attempts": 3,
  "auth": { "enabled": false, "keys": [] },
  "providers": [
    {
      "id": "p-xxx",
      "name": "openai",
      "base_url": "https://api.openai.com/v1",
      "keys": [
        { "id": "k-xxx", "key": "sk-...", "label": "main", "cooldown_secs": null }
      ],
      "aliases": { "fast": "gpt-4o-mini" }
    }
  ]
}
```

Notes:

- `cooldown_secs: null` on a key = use global default
- `aliases` map a client-visible name to an upstream model name
- the file is rewritten (atomically) whenever you change something in the UI

## Using with agents

```bash
curl http://127.0.0.1:8000/v1/chat/completions \
  -H "Content-Type: application/json" \
  -d '{
    "model": "openai/gpt-4o",
    "messages": [{"role": "user", "content": "hello"}],
    "stream": true
  }'
```

Or with the OpenAI SDK:

```python
from openai import OpenAI
client = OpenAI(base_url="http://127.0.0.1:8000/v1", api_key="anything")
```

## Testing

A mock OpenAI-compatible upstream is included:

```bash
cargo run --bin mock_upstream &     # serves 127.0.0.1:9001, "sk-bad" always 429s
cargo run -- --config test/gateway.json
# then:
curl http://127.0.0.1:8000/v1/chat/completions -H "Content-Type: application/json" \
  -d '{"model":"mock/mock-large","messages":[{"role":"user","content":"hi"}]}'
```

## Project layout

```
src/
  main.rs    entrypoint, routers (API :8000, admin/UI :8001)
  config.rs  JSON config schema + atomic persistence
  state.rs   shared state + key pool (round-robin, cooldown tracking)
  proxy.rs   /v1 proxy: model resolution, key rotation, streaming passthrough
  admin.rs   REST API behind the web console
  ui.rs      embedded single-file web console (static/index.html)
```
