# Sakura LLM Gateway

🎯 A local-first LLM API gateway that pools API keys per provider, rotates them automatically on rate limits, and manages everything from a built-in web console.

[English](README.md) | [简体中文](README.zh-CN.md) · 📖 Usage guide: [English](doc/USAGE.md) | [中文](doc/使用说明.md)

## ✨ Features

- 🔌 **OpenAI-compatible API** - point any agent/tool at `http://127.0.0.1:8000/v1`
  - `POST /v1/chat/completions` (streaming SSE and non-streaming)
  - `GET /v1/models` (aggregated from all providers + aliases)
- 🔑 **API-Key Pool** - round-robin across keys; a key that receives `429` is put into
  cooldown and traffic rotates to the next key immediately
- ⏱️ **Smart cooldown** - resolution order: upstream `Retry-After` header →
  `learned_cooldown` (measured by background probing) → per-key override → global
  default (60s); when all keys are cooling down, the gateway fails fast with `429`
- 🔬 **Cooldown learning** - after a 429, a background prober sends tiny probe requests
  at increasing intervals (5s → 300s) until one succeeds, then stores the measured
  rate-limit window as `learned_cooldown` (debounced config writes, one prober per key)
- 🚫 **Invalid-key quarantine** - `401/403` means the key itself is dead: it is
  quarantined for 30 minutes instead of the short rotate-out cooldown
- 🔁 **Retry before first byte** - 429/5xx/timeouts rotate to the next key (budget =
  `max_attempts` × key count, default 3×); once the first byte is sent, the stream is
  passed through untouched
- 🧹 **Stream interruption handling** - a mid-stream failure emits an OpenAI-style SSE
  error chunk followed by `data: [DONE]`, so clients end cleanly with no duplicated or
  truncated text
- 📦 **Managed models** - per-provider model catalog (`{ id, enabled }`), one-click
  import from the provider's `/v1/models` (with checkboxes), and an optional allowlist
  mode that rejects models not in the list
- 🎯 **Model routing** - request models as `provider/model` (e.g. `openai/gpt-4o`), or
  set up short **aliases** in the UI (e.g. `fast` → `gpt-4o-mini`)
- 📊 **Status & statistics** - persistent request stats (`stats.json`): totals, a 24h
  hourly histogram, and success/fail counters per API key, per provider and per model —
  all visible in the web console and queryable via `GET /api/stats`
- 🖥️ **Web console** - `http://127.0.0.1:8001/` with **Status** and **Config** tabs:
  manage providers, the key pool (add/remove, per-key cooldown, live status, show/copy
  key buttons), models, aliases, and settings
- 💾 **JSON config** - human-readable `gateway.json`, saved atomically on every change
- 📤 **Config export/import** - download `gateway.json` as a browser file, or import one
  with validate-then-swap semantics: the whole file is rejected on any error, and a valid
  file hot-swaps with **no restart** (in-flight requests finish on the old state)
- 🔐 **Optional auth** - off by default; enable in the UI to require a gateway-issued
  Bearer key on the local API
- 📦 **Single binary** - the web console is a single native HTML/JS file embedded via
  `rust-embed`; `cargo build` produces everything, no Node toolchain

```
┌──────────────────────────────────────────────────────────┐
│              Agents / tools (OpenAI SDK, etc.)           │
└──────────────────────────────────────────────────────────┘
                            │
              ┌─────────────┴─────────────┐
              ▼                           ▼
     ┌──────────────┐            ┌──────────────┐
     │  Gateway API │            │  Web console │
     │  :8000 /v1   │            │     :8001    │
     └──────────────┘            └──────────────┘
              │                           │
              └─────────────┬─────────────┘
                            ▼
                 ┌─────────────────────┐
                 │  Sakura LLM Gateway │
                 │  key pool · models  │
                 │  stats · cooldowns  │
                 └─────────────────────┘
                            │
              ┌─────────────┼─────────────┐
              ▼             ▼             ▼
        ┌─────────┐   ┌─────────┐   ┌─────────┐
        │ Provider│   │ Provider│   │ Provider│
        │  key 1..n│  │  key 1..n│  │  key 1..n│
        └─────────┘   └─────────┘   └─────────┘
```

## 🚀 Quick start

### Option 1: Download a release (Windows, no toolchain)

Grab `sakura-llmgateway-vX.Y.Z-x86_64-pc-windows-msvc.exe` from
[GitHub Releases](https://github.com/sloxphrite73/sakura-llmgateway/releases),
put it in an empty folder, and double-click it. The web console opens at
`http://127.0.0.1:8001/`.

### Option 2: One-click scripts

**Windows:** double-click `llm-gateway/install.bat` once (installs Rust at user level if
missing, builds the release binary, creates `gateway.json`), then start the gateway
daily with `llm-gateway/start.bat`.

**Linux / macOS / Git Bash:**

```bash
./install.sh   # checks/installs Rust, builds release, creates default gateway.json
./start.sh     # builds if needed, then starts the gateway
```

The install script installs rustup at user level (no admin needed) if cargo is missing.

### Option 3: Build from source

Requires Rust (GNU toolchain on Windows works):

```bash
cd llm-gateway
cargo build --release
./target/release/llm-gateway.exe            # config defaults to ./gateway.json
./target/release/llm-gateway.exe --config path/to/gateway.json
```

### After startup

| Service | URL | Description |
|---------|-----|-------------|
| OpenAI-compatible API | `http://127.0.0.1:8000/v1` | point agents/tools here |
| Web console | `http://127.0.0.1:8001/` | manage everything |

Ports (API `8000`, UI `8001`) are set in the config file or editable in the UI
(port changes take effect on restart).

## 📖 Usage guide

### Configure providers and keys

1. Open the web console `http://127.0.0.1:8001/`
2. Go to the **Config** tab and add a provider (name + OpenAI-compatible `base_url`)
3. Add one or more API keys to the provider's key pool
4. (Optional) Import the provider's model catalog from its `/v1/models` with one click,
   enable/disable individual models, and turn on allowlist mode
5. (Optional) Set up aliases, e.g. `fast` → `gpt-4o-mini`

### Use the gateway API

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

### Monitor traffic

Open the **Status** tab to see total requests, a 24h hourly histogram, and per-key /
per-provider / per-model success and failure counters. Stats persist to `stats.json`
(next to `gateway.json`) and survive restarts.

### Back up or migrate your config

- **Export**: click *Export config* in the console (or `GET /api/config/export`) to
  download `gateway.json` as a file
- **Import**: click *Import config* (or `POST /api/config/import`) and choose a file —
  invalid files are rejected wholesale; valid files hot-swap instantly, no restart

## ⚙️ Config format

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
      "model_allowlist_only": false,
      "keys": [
        { "id": "k-xxx", "key": "sk-...", "label": "main", "cooldown_secs": null,
          "learned_cooldown": null }
      ],
      "models": [
        { "id": "gpt-4o", "enabled": true }
      ],
      "aliases": { "fast": "gpt-4o-mini" }
    }
  ]
}
```

Notes:

- `cooldown_secs: null` on a key = use global default
- `learned_cooldown` is machine-written by the 429 prober and beats `cooldown_secs`
- `models` empty = unmanaged (all models pass through, backward compatible)
- the file is rewritten (atomically) whenever you change something in the UI

## 📡 API overview

### Gateway API (OpenAI-compatible)

| Endpoint | Method | Description |
|----------|--------|-------------|
| `/v1/chat/completions` | POST | Chat completions (streaming + non-streaming) |
| `/v1/models` | GET | Aggregated model list |

### Console API

| Endpoint | Method | Description |
|----------|--------|-------------|
| `/api/status` | GET | Live config + key-pool state |
| `/api/stats` | GET | Request statistics (totals, 24h histogram, per key/provider/model) |
| `/api/config/export` | GET | Download `gateway.json` |
| `/api/config/import` | POST | Validate-then-swap config hot reload |
| `/api/providers` | GET / POST | List / create providers |
| `/api/providers/{id}` | PUT / DELETE | Update / delete provider |
| `/api/providers/{id}/keys` | POST | Add key |
| `/api/providers/{id}/keys/{key_id}` | DELETE | Delete key |
| `/api/keys/{key_id}/cooldown` | DELETE | Clear key cooldown |
| `/api/providers/{id}/models` | POST | Add managed model |
| `/api/providers/{id}/models/import` | POST | Import models from upstream `/v1/models` |
| `/api/providers/{id}/aliases` | POST | Set alias |
| `/api/settings` | PUT | Update global settings |

## 🧪 Testing

A mock OpenAI-compatible upstream is included:

```bash
cd llm-gateway
cargo run --bin mock_upstream &     # serves 127.0.0.1:9001, "sk-bad" always 429s
cargo run -- --config test/gateway.json
# then:
curl http://127.0.0.1:8000/v1/chat/completions -H "Content-Type: application/json" \
  -d '{"model":"mock/mock-large","messages":[{"role":"user","content":"hi"}]}'
```

## 🏗️ Project layout

```
llm-gateway/
├── src/
│   ├── main.rs    entrypoint, routers (API :8000, console :8001), flush timers
│   ├── config.rs  JSON config schema + validate-then-swap + atomic persistence
│   ├── state.rs   shared state + key pool (round-robin, cooldowns, quarantine)
│   ├── proxy.rs   /v1 proxy: model resolution, key rotation, probing, streaming
│   ├── stats.rs   request statistics (counters + 24h histogram, persisted)
│   ├── admin.rs   REST API behind the web console
│   └── ui.rs      embedded web console (static/index.html)
├── static/
│   └── index.html single-file console UI (Status + Config tabs)
├── test/
│   └── gateway.json   mock-upstream test config (gitignored; create your own)
├── install.bat / start.bat   Windows one-click scripts
└── install.sh / start.sh     Linux / macOS / Git Bash scripts
```

## 🛠️ Tech stack

- **Rust** (edition 2021) - the entire backend, single static binary
- **axum + tokio + hyper** - async HTTP serving and proxying
- **reqwest (rustls)** - upstream HTTPS client, no OpenSSL dependency
- **rust-embed** - console UI embedded into the binary at compile time
- **serde / serde_json** - config, stats and API serialization

## 🤝 Contributing

Issues and pull requests are welcome!

## 📄 License

[GPL-3.0](LICENSE)
