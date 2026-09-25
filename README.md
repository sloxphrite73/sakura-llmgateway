# Sakura LLM Gateway

🎯 A local-first LLM API gateway that pools API keys per provider, rotates them automatically on rate limits with smart cooldowns, and manages everything from a built-in web console. Speaks both **OpenAI** and **Anthropic** protocols (any inbound × any upstream, translated automatically), ships as a single-file Windows exe and an Android APK, and lets any client — Claude Code, Cherry Studio, whatever — safely share your entire key pool.

[English](README.md) | [简体中文](README.zh-CN.md) · 📖 Usage guide: [English](doc/USAGE.md) | [中文](doc/使用说明.md)

[Bugs report and communication with dev(Telegram)](https://t.me/+4NfMkc8SJDwzYWZl)

## ✨ Features

- 🔌 **OpenAI-compatible API** - point any agent/tool at `http://127.0.0.1:8000/v1`
  - `POST /v1/chat/completions` (streaming SSE and non-streaming)
  - `GET /v1/models` (aggregated from all providers + aliases)
- 🌸 **Anthropic Messages protocol** - also exposes `POST /v1/messages` (Claude Code
  and Anthropic SDKs connect directly); each provider can be set to `openai` or
  `anthropic` protocol and the gateway translates bidirectionally — any inbound
  protocol × any upstream protocol works (OpenAI clients can use a real Claude API
  key pool too); tools / images / streaming events / count_tokens all supported
- 🔑 **API-Key Pool** - round-robin across keys; a key that receives `429` is put into
  cooldown and traffic rotates to the next key immediately
- ⏱️ **Smart cooldown** - resolution order: upstream `Retry-After` header →
  `learned_cooldown` (learned via binary-search probing) → per-key override → global
  default (60s); when all keys are cooling down, the gateway fails fast with `429`
- 🔬 **Cooldown learning (binary-search prober)** - after a 429, a background prober
  measures the key's real rate-limit window and stores it as `learned_cooldown`:
  starting from T (the previous learned value, else the global default), it sends a
  tiny probe after T seconds (the provider's first enabled managed model,
  `max_tokens: 1`) — success means the window is shorter, bracketing `[T/2, T]`;
  another 429 means longer, bracketing `[T, 2T]`. The bracket is then bisected for
  ~5 rounds to ±1s and the **midpoint** is stored. Bounded by design: at most
  10 probes per measurement, a 900s window cap, and at most one recalibration per
  key per hour — so it never burns quota

```
Probe interval (seconds)
T ──── 429 ──▶ [T, 2T]          success ──▶ [T/2, T]
                 │                          │
                 ▼ bisect ~5 rounds          ▼ bisect ~5 rounds
            converge to ±1s ◀━━━━ midpoint stored as learned_cooldown
```
- 🚫 **Invalid-key quarantine** - `401/403` means the key itself is dead: it is
  quarantined for 30 minutes instead of the short rotate-out cooldown
- 🔁 **Retry before first byte** - 429/5xx/timeouts rotate to the next key (budget =
  `max_attempts` × key count, default 3×); once the first byte is sent, the stream is
  passed through untouched
- 🧹 **Stream interruption handling** - a mid-stream failure emits an OpenAI-style SSE
  error chunk followed by `data: [DONE]`, so clients end cleanly with no duplicated or
  truncated text
- 🖼️ **Large context & image input** - the `/v1` proxy lifted axum's default 2 MiB
  request body limit, so ~1M-token context requests and base64 image inputs go
  through (when the upstream supports them). Image content blocks pass through
  same-protocol (OpenAI→OpenAI) and translate cross-protocol (OpenAI↔Anthropic)
- 📦 **Managed models** - per-provider model catalog (`{ id, enabled }`), one-click
  import from the provider's `/v1/models` (with checkboxes), and an optional allowlist
  mode that rejects models not in the list
- 🆓 **Free provider catalog** - the console's "Provider catalog" panel ships 25
  OpenAI-compatible upstreams with free tiers — SiliconFlow, Z.AI GLM, OpenRouter
  `:free`, Pollinations (keyless), NVIDIA NIM, Groq, Mistral, Cloudflare
  Workers AI, Google Gemini, SambaNova, Hugging Face, Fireworks, Novita, Requesty,
  Cohere, Alibaba DashScope, Volcengine Doubao, Moonshot Kimi, AI21, Baidu Qianfan,
  Stepfun, iFlyTek Spark, Tencent Hunyuan, ModelScope, Infermatic: each card carries
  a **setup-guide link** and preset free models —
  **paste key → confirm** adds everything in one step (provider + models + keys).
  The catalog is embedded in the binary and can also be refreshed from GitHub
- 🎯 **Model routing** - request models as `provider/model` (e.g. `openai/gpt-4o`), or
  set up short **aliases** in the UI (e.g. `fast` → `gpt-4o-mini`)
- 🧭 **Routing strategy** - a `(provider, model, api_key_id)` triple table; two
  filter toggles (`lock_model_group` / `lock_provider`, both ON by default = exact
  provider+model, only rotate keys = the long-standing behavior, zero-surprise
  upgrade) shrink candidates into four modes — exact / model-first (same logical
  model across all providers) / provider-first (all that provider's models) /
  available-first (whole table). Sort = fixed leading `valid ↓` + `non-cooled ↓`
  (always on, no toggle) + a user stack of 0–3 of 10 keys (A–J: success_rate,
  rpm, tpm, avg_tftt, token_balance×2, bill_balance×2, price, tps), tiebreak =
  table-order (D1 cursor rotation → round-robin). Walks to the first valid +
  non-cooled row; if all valid rows are cooling → `429` + `Retry-After:
  min(remaining_secs)` (precise seconds from the measured `learned_cooldown`,
  not a fixed value). `learned_cooldown` feeds `cold`/`remaining_secs`
  orthogonally; `model_groups` enable cross-provider same-model fallback. The
  策略 / Strategy tab has a dry-run preview
- 🌱 **Per-key metric seed** - a brand-new key has no live metrics (rpm/tpm = 0,
  success_rate = 1.0), so B/C/D/J sorts rank it last and it never gets picked
  until it accrues traffic. Set `seed_rpm` / `seed_tpm` / `seed_success_rate` /
  `seed_avg_tftt_ms` / `seed_tps` to preset initial values, automatically
  overridden once the key serves real traffic (the cooldown prober does not
  count as traffic). Edited per-key in the API Key page
- 📊 **Status & statistics** - persistent request stats (merged into `gateway.json` under `stats`): totals, a 24h
  hourly histogram, and success/fail counters per API key, per provider and per model —
  all visible in the web console and queryable via `GET /api/stats`
- 📈 **Measured metrics** - per-model avg_TFTT (time to first content token in
  streaming responses; non-streaming = 0/not shown) on the model card and in
  `/api/stats`; per-provider RPM/TPM limits are editable + persisted
  (`rpm_limit` / `tpm_limit`, optional; null = no manual cap, and the console
  shows the live aggregate of that provider's keys' measured rpm/tpm)
- 🖥️ **Web console** - `http://127.0.0.1:8001/`, a single-page sakura-themed console
  rebuilt 1:1 from a token-driven design system (dark/light + EN/中文, persisted):
  统计/Stats, 提供商/Providers, 模型/Models, 策略/Strategy (filter+sort+dry-run),
  API Key (live per-second status, smart-cooldown column, per-key metric seed
  editor, show/copy), and 设置/Settings — manage providers, the key pool, models,
  aliases, model groups, routing strategy, and settings
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

### Option 1: Download a release (Windows exe / Android APK, no toolchain)

Grab `sakura-llmgateway-vX.Y.Z-x86_64-pc-windows-msvc.exe` from
[GitHub Releases](https://github.com/sloxphrite73/sakura-llmgateway/releases),
put it in an empty folder, and double-click it. The web console opens at
`http://127.0.0.1:8001/`.

**Android:** grab `sakura-llmgateway-vX.Y.Z-universal.apk` from the same release and
install it (allow "install unknown apps" if asked). First launch offers to import a
`gateway.json` exported from your desktop, or you can skip and start with an empty
config and add keys in the console. The app runs the gateway as a foreground service
(notification + 10s health watchdog) and shows the same web console full-screen;
`gateway.json` from a file manager can be imported via "Open with → Sakura LLM Gateway"
(hot-reloads if the gateway is running).

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
per-provider / per-model success and failure counters. Stats persist into
`gateway.json` (merged under the `stats` field, no separate `stats.json`) and
survive restarts.

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
  "strategy": {
    "filter": { "lock_model_group": true, "lock_provider": true },
    "sort": ["I", "J"]
  },
  "model_groups": [
    { "id": "gpt-4o", "entries": [["openai", "gpt-4o"], ["azure", "gpt-4o"]] }
  ],
  "providers": [
    {
      "id": "p-xxx",
      "name": "openai",
      "base_url": "https://api.openai.com/v1",
      "model_allowlist_only": false,
      "rpm_limit": null,
      "tpm_limit": null,
      "keys": [
        { "id": "k-xxx", "key": "sk-...", "label": "main", "cooldown_secs": null,
          "learned_cooldown": null,
          "seed_rpm": null, "seed_tpm": null, "seed_success_rate": null,
          "seed_avg_tftt_ms": null, "seed_tps": null }
      ],
      "models": [
        { "id": "gpt-4o", "enabled": true }
      ],
      "aliases": { "fast": "gpt-4o-mini" }
    }
  ],
  "stats": {
    "total": { "success": 0, "fail": 0, "tokens": 0 },
    "hourly": {},
    "keys": {},
    "providers": {},
    "models": {},
    "rotations": {},
    "provider_metrics": {
      "p-xxx": { "rpm": 0, "tpm": 0, "success_rate": 1.0, "avg_tftt_ms": 0, "tps": 0.0 }
    }
  }
}
```

Notes:

- `cooldown_secs: null` on a key = use global default
- `learned_cooldown` is machine-written by the binary-search prober (bracket
  midpoint, ±1s accuracy) and beats `cooldown_secs`
- `strategy.filter` both ON (default) = exact `(provider, model)`, only rotate
  keys; `strategy.sort` is 0–3 of A–J (ordered, no dups) — the user sort stack
- `model_groups` declares the same logical model across providers (entries are
  `[provider_id, upstream_model]`) for cross-provider fallback; each model
  belongs to exactly one group
- `rpm_limit` / `tpm_limit` on a provider = manual RPM/TPM cap (`null` = no cap;
  the console then shows the auto-detected aggregate of the provider's keys'
  measured rpm/tpm/success_rate/avg_tftt/tps, also persisted under
  `stats.provider_metrics` so it survives a restart — the UI shows the last-known
  value until new traffic overrides it)
- `seed_*` on a key = preset initial metrics, auto-overridden once the key serves
  real traffic (all optional; omitted = built-in defaults, so old configs load
  unchanged)
- `models` empty = unmanaged (all models pass through, backward compatible)
- `stats` = request statistics (totals, 24h histogram, per key/provider/model
  counters + rotations + `provider_metrics`), merged into `gateway.json` (no longer
  a separate `stats.json`). A pre-merge `stats.json` next to `gateway.json` is
  auto-migrated into the `stats` field on startup, then removed. Omit on a
  hand-written config = empty stats (old configs load unchanged)
- the file is rewritten (atomically) on every config change + on the 5s stats flush

## 📡 API overview

### Gateway API (OpenAI + Anthropic compatible)

| Endpoint | Method | Description |
|----------|--------|-------------|
| `/v1/chat/completions` | POST | Chat completions (streaming + non-streaming) |
| `/v1/messages` | POST | Anthropic Messages (streaming + non-streaming, tools / images) |
| `/v1/messages/count_tokens` | POST | Token counting (proxied for Anthropic upstreams, local estimate for OpenAI ones) |
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
| `/api/providers/{id}/rate-limits` | PUT | Set provider RPM/TPM limits (`{rpm_limit, tpm_limit}`, null = clear) |
| `/api/providers/{id}/keys` | POST | Add key |
| `/api/providers/{id}/keys/{key_id}` | DELETE / PUT | Delete key / update metric seeds |
| `/api/strategy/dry-run` | POST | Preview routing for a model (routed key + candidates) |
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
├── install.sh / start.sh     Linux / macOS / Git Bash scripts
└── android/         Android app (Kotlin, WebView console + foreground service)
```

### Android app

The APK is a thin native shell around the same Rust binary:
the gateway is cross-compiled for `arm64-v8a` / `armeabi-v7a` / `x86_64` in CI,
packaged as `jniLibs/*/libllmgateway.so`, and spawned by a foreground service with
a 10-second HTTP watchdog (probes `/api/status`; restarts on death or hang).
The UI is the embedded console in a fullscreen WebView; config export goes through
the system save dialog, and `gateway.json` imports hot-reload a running gateway.

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
