# Sakura LLM Gateway

A local-first LLM API gateway that pools API keys per provider, rotates them automatically on rate limits with smart cooldowns, and manages everything from a built-in web console. Speaks both **OpenAI** and **Anthropic** protocols (any inbound × any upstream, translated automatically), ships as a single-file Windows executable and an Android APK, and lets any client — Claude Code, Cherry Studio, or anything that speaks OpenAI — safely share your entire key pool.

[English](README.md) | [简体中文](README.zh-CN.md) · Usage guide: [English](doc/USAGE.md) | [中文](doc/使用说明.md)

[Bugs report and communication with dev (Telegram)](https://t.me/+4NfMkc8SJDwzYWZl)

## Features

**Dual-protocol proxy** — point any agent or tool at `http://127.0.0.1:8000/v1`. The gateway accepts OpenAI Chat Completions (`POST /v1/chat/completions`) and Anthropic Messages (`POST /v1/messages`) on the same port. Each provider can be set to `openai` or `anthropic` upstream protocol, and the gateway translates bidirectionally — OpenAI clients can use a real Claude API key pool, and vice versa. Streaming SSE, tool use, image input, and `count_tokens` all pass through.

**API-Key pool with automatic rotation** — round-robin across keys per provider. When a key receives a `429`, it enters cooldown and traffic rotates to the next key immediately — the client never sees the rate limit. The retry budget is `max_attempts × key_count` (default 3×), and once the first byte of a response is sent, the stream passes through untouched.

**Smart cooldown learning** — cooldown resolution follows a priority chain: upstream `Retry-After` header → `learned_cooldown` (measured by binary-search probing) → per-key override → global default (60s). After a 429, a background prober measures the key's real rate-limit window: it brackets the window (probe at time T — success means shorter, 429 means longer) then bisects for ~5 rounds to ±1s precision. Bounded by design: at most 10 probes per measurement, a 900s window cap, and at most one recalibration per key per hour. Invalid keys (401/403) are quarantined for 30 minutes instead of short cooldown.

```
Probe interval (seconds)
T ──── 429 ──▶ [T, 2T]          success ──▶ [T/2, T]
                │                          │
                ▼ bisect ~5 rounds          ▼ bisect ~5 rounds
           converge to ±1s ◀━━━━ midpoint stored as learned_cooldown
```

**Model routing and strategy** — request models as `provider/model` (e.g. `openai/gpt-4o`) or set up short aliases. A routing strategy with two filter toggles (`lock_model_group` / `lock_provider`, both ON by default) and a user-configurable sort stack of 0–3 keys (A–J: success_rate, rpm, tpm, avg_tftt, token/bill balances, price, tps) gives four modes: exact, model-first (cross-provider fallback), provider-first, or available-first. A dry-run preview shows which key will be picked before you send.

**Managed models and free provider catalog** — per-provider model catalog with one-click import from the upstream's `/v1/models`. An optional allowlist mode rejects unlisted models. The console ships a catalog of 25 OpenAI-compatible upstreams with free tiers — SiliconFlow, Z.AI GLM, OpenRouter `:free`, Pollinations (keyless), NVIDIA NIM, Groq, Mistral, Google Gemini, and more — each with a setup guide and preset free models. Paste a key and confirm to add everything in one step.

**Image input declaration** — models can be flagged with `supports_images` (toggle in the model edit form), declared in `/v1/models` so clients can discover vision-capable models. Image content blocks pass through same-protocol (OpenAI→OpenAI) and translate cross-protocol (OpenAI↔Anthropic). The gateway forwards image requests as-is — no stripping, no size limits (axum's default 2 MiB body cap is lifted).

**Persistent statistics and measured metrics** — request stats (totals, 24h hourly histogram, per-key/provider/model success-fail counters) are merged into `gateway.json` under `stats` and survive restarts. Per-model avgTFTT (time-to-first-token) and avgTPS (generation speed) are sampled from streaming responses and persisted. Per-provider detected RPM/TPM/success_rate/avg_tftt/tps are also persisted so the UI shows the last-known value after a restart until new traffic overrides it.

**Per-key metric seed** — a brand-new key has no live metrics, so sort-based routing ranks it last. Set `seed_rpm` / `seed_tpm` / `seed_success_rate` / `seed_avg_tftt_ms` / `seed_tps` to preset initial values, automatically overridden once the key serves real traffic.

**Web console** — `http://127.0.0.1:8001/`, a single-page sakura-themed console (dark/light + EN/中文, preferences persisted): Stats, Providers, Models, Strategy (filter + sort + dry-run), API Key (live per-second status, cooldown countdown, RPM/TPM/avg_TFTT columns, per-key metric seed editor, show/copy, batch import), and Settings. Mobile-first responsive layout with a bottom tab bar on phones.

**Version display** — the sidebar footer shows the real release version (e.g. `Sakura v0.7.1`) via a build script that runs `git describe --tags` at compile time.

**Config export/import** — download `gateway.json` as a file, or import one with validate-then-swap semantics: the whole file is rejected on any error, a valid file hot-swaps with no restart (in-flight requests finish on the old state), and imported stats are adopted (not discarded).

**Single binary, no Node toolchain** — the web console is a single native HTML/JS file embedded via `rust-embed`; `cargo build` produces everything.

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

## Quick start

### Option 1: Download a release (Windows exe / Android APK, no toolchain)

Grab `sakura-llmgateway-vX.Y.Z-x86_64-pc-windows-msvc.exe` from
[GitHub Releases](https://github.com/sloxphrite73/sakura-llmgateway/releases),
put it in an empty folder, and double-click it. The web console opens at
`http://127.0.0.1:8001/`.

**Android:** grab `sakura-llmgateway-vX.Y.Z-universal.apk` from the same release and
install it (allow "install unknown apps" if asked). First launch offers to import a
`gateway.json` exported from your desktop, or you can skip and start with an empty
config and add keys in the console. The app runs the gateway as a foreground service
(notification + 10s health watchdog) and shows the same web console full-screen.
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

## Usage guide

### Configure providers and keys

1. Open the web console `http://127.0.0.1:8001/`
2. Go to the **Providers** tab and add a provider (name + OpenAI-compatible `base_url`). You can also pick one from the built-in free provider catalog (25 upstreams with setup guides).
3. Go to the **API Key** tab, select the provider from the dropdown, paste one or more API keys (one per line for batch import), and click **Add**. Keys are always displayed masked (head + tail visible); click **Show** to reveal the full key.
4. (Optional) In the **Models** tab, import the provider's model catalog from its `/v1/models` with one click, enable/disable individual models, toggle `supports_images` for vision models, set context length / input price / output price, and turn on allowlist mode
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

Open the **Stats** tab to see total requests, a 24h hourly histogram, and per-key /
per-provider / per-model success and failure counters. The API Key page shows
per-key RPM, TPM, avg_TFTT, request counts (persisted), and live cooldown status.
Stats persist into `gateway.json` (merged under the `stats` field) and survive restarts.

### Back up or migrate your config

- **Export**: click *Export config* in the console (or `GET /api/config/export`) to
  download `gateway.json` as a file
- **Import**: click *Import config* (or `POST /api/config/import`) and choose a file —
  invalid files are rejected wholesale; valid files hot-swap instantly (stats are
  adopted, not discarded), no restart needed

## Config format

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
        { "id": "gpt-4o", "enabled": true, "context_length": 128000, "supports_images": true }
      ],
      "aliases": { "fast": "gpt-4o-mini" },
      "price_table": { "gpt-4o": 2.5 },
      "output_price_table": { "gpt-4o": 10.0 }
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
    },
    "model_metrics": {
      "gpt-4o": { "avg_tps": 0.0, "avg_tftt_ms": 0 }
    }
  }
}
```

Key fields:

- `cooldown_secs: null` on a key = use global default; `learned_cooldown` is machine-written by the binary-search prober and beats `cooldown_secs`
- `strategy.filter` both ON (default) = exact `(provider, model)`, only rotate keys
- `model_groups` declares the same logical model across providers for cross-provider fallback
- `rpm_limit` / `tpm_limit` = manual cap (`null` = auto, shows live aggregate)
- `price_table` / `output_price_table` = per-model input/output price (CNY / 1M tokens, `0` = free)
- `supports_images` on a model = declares image input capability in `/v1/models`
- `context_length` on a model = max context window in tokens (shown on model card as "Nk")
- `seed_*` on a key = preset initial metrics, auto-overridden once the key serves real traffic
- `stats` = request statistics, merged into `gateway.json` (no separate `stats.json`); a pre-merge `stats.json` is auto-migrated on startup then removed. Importing a `gateway.json` with `stats` adopts those stats.

## API overview

### Gateway API (OpenAI + Anthropic compatible)

| Endpoint | Method | Description |
|----------|--------|-------------|
| `/v1/chat/completions` | POST | Chat completions (streaming + non-streaming, images) |
| `/v1/messages` | POST | Anthropic Messages (streaming + non-streaming, tools / images) |
| `/v1/messages/count_tokens` | POST | Token counting (proxied for Anthropic upstreams, local estimate for OpenAI) |
| `/v1/models` | GET | Aggregated model list (includes `supports_images` per model) |

### Console API

| Endpoint | Method | Description |
|----------|--------|-------------|
| `/api/status` | GET | Live config + key-pool state + version |
| `/api/stats` | GET | Request statistics (totals, 24h histogram, per key/provider/model) |
| `/api/config/export` | GET | Download `gateway.json` (config + stats) |
| `/api/config/import` | POST | Validate-then-swap config + stats hot reload |
| `/api/providers` | GET / POST | List / create providers |
| `/api/providers/{id}` | PUT / DELETE | Update / delete provider |
| `/api/providers/{id}/rate-limits` | PUT | Set provider RPM/TPM limits |
| `/api/providers/{id}/price-table` | PUT | Set per-model input + output prices (`{table, output_table}`) |
| `/api/providers/{id}/keys` | POST | Add key |
| `/api/providers/{id}/keys/{key_id}` | DELETE / PUT | Delete key / update metric seeds |
| `/api/providers/{id}/models` | POST | Add managed model |
| `/api/providers/{id}/models/{model_id}/context-length` | PUT | Set model context length |
| `/api/providers/{id}/models/{model_id}/supports-images` | PUT | Toggle image input support |
| `/api/keys/{key_id}/cooldown` | DELETE | Clear key cooldown |
| `/api/strategy/dry-run` | POST | Preview routing for a model |
| `/api/settings` | PUT | Update global settings |

## Testing

A mock OpenAI-compatible upstream is included:

```bash
cd llm-gateway
cargo run --bin mock_upstream &     # serves 127.0.0.1:9001, "sk-bad" always 429s
cargo run -- --config test/gateway.json
# then:
curl http://127.0.0.1:8000/v1/chat/completions -H "Content-Type: application/json" \
  -d '{"model":"mock/mock-large","messages":[{"role":"user","content":"hi"}]}'
```

## Project layout

```
llm-gateway/
├── src/
│   ├── main.rs    entrypoint, routers (API :8000, console :8001), flush timers
│   ├── config.rs  JSON config schema + validate-then-swap + atomic persistence
│   ├── state.rs   shared state + key pool (round-robin, cooldowns, quarantine)
│   ├── proxy.rs   /v1 proxy: model resolution, key rotation, probing, streaming
│   ├── stats.rs   request statistics (counters + 24h histogram, persisted)
│   ├── admin.rs   REST API behind the web console
│   ├── ui.rs      embedded web console (static/index.html)
│   └── build.rs   embeds git tag as GATEWAY_VERSION at compile time
├── static/
│   └── index.html single-file console UI
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

## Tech stack

- **Rust** (edition 2021) — the entire backend, single static binary
- **axum + tokio + hyper** — async HTTP serving and proxying
- **reqwest (rustls)** — upstream HTTPS client, no OpenSSL dependency
- **rust-embed** — console UI embedded into the binary at compile time
- **serde / serde_json** — config, stats, and API serialization

## Contributing

Issues and pull requests are welcome!

## License

[GPL-3.0](LICENSE)
