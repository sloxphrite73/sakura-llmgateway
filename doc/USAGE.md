# Sakura LLM Gateway — Usage Guide

A local LLM API gateway that sits between your agents/tools and upstream
OpenAI-compatible providers. It pools multiple API keys per provider, rotates
them automatically when a key is rate-limited, and gives you a web console for
everything.

> 中文版：[使用说明.md](使用说明.md)

---

## 1. Installation & startup

### One-click (recommended)

| Platform | Steps |
|---|---|
| Windows | Double-click `llm-gateway/install.bat` **once** (installs Rust at user level if missing, downloads portable MinGW if needed, builds the release binary, creates `gateway.json`). After that, start the gateway daily with `llm-gateway/start.bat`. |
| Linux / macOS / Git Bash | `./install.sh` once, then `./start.sh`. |

Notes:

- `start.bat` / `start.sh` run the gateway **in the foreground** — the window
  must stay open. Closing it stops the gateway.
- If the release binary already exists, the start scripts skip the build and
  boot immediately.

### Manual

```bash
cd llm-gateway
cargo build --release
./target/release/llm-gateway.exe --config gateway.json   # Windows
./target/release/llm-gateway --config gateway.json       # Linux/macOS
```

After startup:

| Service | URL |
|---|---|
| OpenAI-compatible API | `http://127.0.0.1:8000/v1` |
| Anthropic Messages API | `http://127.0.0.1:8000/v1/messages` (Claude Code etc., see section 5) |
| Web console | `http://127.0.0.1:8001/` |


---

## 2. Android (APK)

The Android build is the **same Rust gateway** as the Windows build: CI
cross-compiles it for `arm64-v8a` / `armeabi-v7a` / `x86_64`, packages all three
ABIs into a universal APK, and a foreground service runs and supervises it.

### Installation

1. Download `sakura-llmgateway-vX.Y.Z-universal.apk` from
   [GitHub Releases](https://github.com/sloxphrite73/sakura-llmgateway/releases)
   (the phone's browser is fine).
2. Tap to install; if prompted about "unknown apps", allow installs from that source.
3. Open the app — the launcher icon is the same sakura 🌸 as the onboarding screen.

### First launch

- **Import gateway.json** — pick the config file exported from your desktop in the
  file picker; it takes effect after validation.
- **Skip, start with an empty config** — the app generates a valid empty config and
  starts the gateway right away; add providers and keys in the console afterwards.

### Daily use

| Topic | Details |
|---|---|
| Foreground service | The gateway runs behind an ongoing notification ("Sakura LLM Gateway"); tapping it returns to the console, and it carries a Stop action |
| Notification permission | On Android 13+ the first launch asks for the notification permission; if denied the service still runs — only the notification is hidden, and it can be granted later in system settings (added in v0.3.1) |
| Console | A fullscreen WebView inside the app — the same page as `http://127.0.0.1:8001/`; in-page import/export and confirm dialogs work |
| Endpoint | On-device agents/tools point at `http://127.0.0.1:8000/v1` |
| Lifecycle | The service keeps running in the background; START_STICKY restarts it if the system kills it; a health watchdog (first check at 2s, then every 10s) restarts the gateway on hang or death |
| In-app import | The console's import button opens the system file picker; imports hot-reload |
| System-level import | "Open with → Sakura LLM Gateway" on a `gateway.json` in any file manager; hot-reloads while the gateway runs |
| Export | The console's export button opens the system "Save as" dialog |

### Migrating a config from desktop to phone

1. **Desktop**: console → Config tab → **Export** to get `gateway.json`
   (all providers, keys, models, aliases, and learned cooldowns included).
2. **Transfer**: send it to the phone by USB, chat apps, cloud drive — anything.
3. **Import** (either way):
   - On first launch, choose "Import gateway.json"; or
   - If already running: locate the file in a file manager → Open with →
     **Sakura LLM Gateway**. The config hot-swaps with no restart.
4. **Verify**: back in the app console, the Status tab should show all keys online;
   make one `http://127.0.0.1:8000/v1/chat/completions` call from the phone's
   browser or an on-device agent.

> Tip: emulators (MuMu / LDPlayer / Nox) work too. If anything misbehaves, upgrade to **v0.3.2+** first (fixes the startup race, the notification permission prompt, and undersized onboarding buttons on small screens),
> or tap Stop in the notification and reopen the app.

---
## 3. Web console

Open `http://127.0.0.1:8001/`. The console is a multi-tab single page
(统计/Stats · 提供商/Providers · 模型/Models · 策略/Strategy · API Key · 设置/Settings;
dark/light + EN/中文, persisted to localStorage). By section:

### Global settings

- **Default cooldown (seconds)** — how long a 429'd key rests when the upstream
  sends no `Retry-After` and the key has no override (default 60).
- **Max attempts per request** — retry budget multiplier (default 3). The
  effective budget is `max_attempts × number of keys`, so a request only fails
  after every available key has been tried.
- **API port / UI port** — take effect after a restart.
- **Local API auth** — off by default. When enabled, agents must present one of
  the configured keys as a Bearer token on `/v1/*`.

### Provider catalog (free tiers)

The "Provider catalog" panel at the top of the Config page ships a curated set of
**free / free-tier OpenAI-compatible upstreams** — SiliconFlow, Z.AI GLM, OpenRouter
`:free`, Pollinations (keyless), Cerebras, NVIDIA NIM, Groq, Mistral, Cloudflare
Workers AI, Google Gemini, SambaNova, Hugging Face, Fireworks, Novita, Requesty,
Cohere, Alibaba DashScope, Volcengine Doubao, Moonshot Kimi:

- Each card shows free-tier notes, the preset free model list (expandable), a
  **setup guide** link (pointing at step-by-step docs in this repo under
  `doc/free-providers/`), and a **sign up / get key** link.
- To add: claim your key via the guide → paste it into the card's input box (one
  key per line) → click **Add**. The gateway creates the provider, imports the
  preset free models, and stores the keys in one step.
- **Pollinations is keyless** — add it with an empty key box (anonymous use has
  rate limits).
- Already-added cards show "✓ Added · N key"; you can always append more keys.
- "Update from GitHub" pulls the latest catalog (added in v0.3.3; the catalog is
  also embedded in the binary, so it works offline).

### Providers

Each provider card manages one upstream:

- **Edit / delete** the provider (name + base URL).
- **API keys** — add/remove keys, optional per-key cooldown override, live
  status, request count, last error. The API key value is always shown masked.
- **Models** — the managed model catalog (see section 4).
- **Aliases** — short names mapped to upstream model names (see section 5).

**Key status meanings:**

| Status | Meaning |
|---|---|
| 🟢 可用 / Available | Healthy, participates in rotation |
| 🟡 冷却 · 剩余 Xs / Cooling | Temporarily benched (429 or 5xx); auto-recovers at 0s |
| 🔴 无效 · 剩余 Xs / Invalid | Auth was rejected (401/403) — quarantined for 30 min because retrying cannot succeed. Click **解除无效 / Clear** after fixing the key to restore it immediately. |

### Strategy

The **策略 / Strategy** tab configures the routing strategy (see §4.2):

- **Two filter toggles** — `lock_model_group` and `lock_provider` (both ON by
  default = exact provider+model, only rotate keys). The combination picks one of
  four candidate modes (exact / model-first / provider-first / available-first).
- **Sort stack** — three slots for the A–J sort keys (success_rate, rpm, tpm,
  avg_tftt, token/bill balance ×2, price, tps); the fixed `valid ↓` + `non-cooled ↓`
  leads are always on and not shown.
- **Dry-run preview** — pick a model (+ optional provider) and run a simulated
  route: it shows the routed key and the full candidate list with each key's
  valid/cooling/remaining state and metrics.

### API Key page

A global table of every key across all providers (one row per key):

- **智能冷却时长 / Smart cooldown** column — each key's effective cooldown
  (`learned_cooldown` → per-key `cooldown_secs` → global default).
- **状态 / Status** column — 可用 / 冷却 · 剩 Xs / 无效; the page polls
  `/api/status` **once per second** and surgically updates the status, cooldown
  countdown, request count and error cells, so the countdown ticks down live and
  keys flip 可用↔冷却 as they recover. The key show/hide toggle and any open
  editor are not disrupted.
- **Seed** — a `Seed` button per key opens an edit card for the five metric seeds
  (RPM / TPM / success_rate / avg_tftt / TPS; empty = clear), see §4.3.
- **显示 / 复制 / 删除** — reveal (masked → full), copy, or delete a key.

### Theme & language

- The ☀️/🌙 button in the masthead switches between light and dark mode; the adjacent 「EN / 中」 button switches the whole interface between English and Chinese — every string, including tables, buttons, dialogs and input placeholders. Both choices are remembered per browser (localStorage); the language defaults to the browser's setting (added in v0.3.2).
- The masthead also shows both endpoints side by side: `OpenAI-compatible …/v1` and `Anthropic Messages …/v1/messages` — **click either one to copy it** (added in v0.3.2).

---

## 4. Key pool behavior (what happens on 429)

1. Requests are distributed round-robin across the provider's keys.
2. A key that receives **429** is benched: cooldown = upstream `Retry-After`
   header → `learned_cooldown` (learned via binary-search probing, see §4.1) →
   per-key override → global default. The gateway waits **1.5s** and
   retries the same request on the next key.

### 4.1 Cooldown learning (binary-search prober)

After a key gets a 429, a background prober measures its real rate-limit window
and stores it as `learned_cooldown` (visible in the console and config file):

1. **Initial T** = the previous learned value (else the global default), clamped
   to 2–900 seconds.
2. **Bracket**: after waiting T seconds, send a tiny probe (the provider's first
   enabled managed model, `max_tokens: 1`) — success means the window is shorter,
   bracketing `[T/2, T]`; another 429 means longer, bracketing `[T, 2T]`
   (900s cap).
3. **Bisect**: bisect the bracket for ~5 rounds (probe the midpoint; success takes
   the upper half, 429 the lower), converging to ±1 second.
4. **Store**: the bracket **midpoint** becomes the key's `learned_cooldown`
   (debounced config write).

Guard rails: at most 10 probes per measurement; 401/403/other 4xx/network errors
are "unlearnable" and abort the search; each key recalibrates at most once per
hour (`RELEARN_AFTER_SECS = 3600`); only one prober per key at a time.
3. **401/403** (invalid key) quarantines the key for 30 minutes instead —
   retrying a dead key just wastes calls.
4. Other key-scoped errors (408/5xx, network errors) bench the key for 5s and
   rotate.
5. Request-scoped errors (e.g. **400** bad body) are returned to the client
   immediately — they would fail identically on every key.
6. Only when **every** key is cooling/invalid does the client get a `429` with
   a list of the benched keys (fail fast, no waiting).
7. Once the first response byte has been forwarded, the stream is passed
   through untouched — no mid-stream key switching (it would corrupt output).
8. Every failed request in an agent conversation is just another
   `/v1/chat/completions` call: one client request → one key per attempt.

### 4.2 Routing strategy

On top of the round-robin + cooldown behavior above sits a configurable strategy
layer (the 策略 / Strategy tab). It treats the pool as a `(provider, model,
api_key_id)` triple table and, per request:

1. **Resolve** the request model → `(provider, model)` + model group. A bare
   model name (no `provider/` prefix, no alias) leaves the provider unresolved,
   which auto-degrades `lock_provider` to OFF (you can't lock a provider the
   request didn't name).
2. **Filter** — two toggles shrink the table to a candidate set:

   | `lock_model_group` | `lock_provider` | Candidate set |
   |---|---|---|
   | ON | ON | exact `(provider, model)` — only rotate keys (default) |
   | ON | OFF | the model's group across all providers (model-first) |
   | OFF | ON | every model of the resolved provider (provider-first) |
   | OFF | OFF | the whole table (available-first) |

3. **Sort** — comparator = fixed leading `valid ↓` + `non-cooled ↓` (always on,
   no toggle) + the user's 0–3 sort keys (A–J), then table-order (D1 cursor
   rotation → round-robin load balancing):

   | ID | Attribute | Dir | ID | Attribute | Dir |
   |---|---|---|---|---|---|
   | A | success_rate | ↓ | G | bill_balance | ↓ |
   | B | rpm | ↓ | H | bill_balance | ↑ |
   | C | tpm | ↓ | I | price | ↑ |
   | D | avg_tftt | ↑ | J | tps | ↓ |
   | E | token_balance | ↓ | | | |
   | F | token_balance | ↑ | | | |

   `token_balance` / `bill_balance` use `+∞` when the provider doesn't report
   them (`None`); `∞` sorts first under `↓` (best) and last under `↑`. The
   measured attributes (rpm/tpm/success_rate/avg_tftt/tps) are always finite.
4. **Walk** — scan top-down, take the first `valid + non-cooled` row; route it.
5. **Terminal**:
   - routed → serve that `(provider, model, api_key_id)`;
   - no valid+non-cooled but valid rows exist (all cooling) → `429` +
     `Retry-After: min(remaining_secs)` (precise seconds from the measured
     `learned_cooldown` — the differentiator vs fixed-cooldown routers);
   - no valid row → `429` (no Retry-After; invalid keys don't recover by
     waiting);
   - empty candidate set → `429`.

`learned_cooldown` (§4.1) feeds `cold`/`remaining_secs` **orthogonally** — the
strategy layer only reads it. **Model groups** (`model_groups` in the config) let
"model-first" fall back across providers: a group is the *same* logical model
offered by one or more providers (e.g. `gpt-4o` = `{openai/gpt-4o, azure/gpt-4o}`);
each model belongs to exactly one group.

### 4.3 Per-key metric seed (manual seed + auto-override)

A brand-new key has no live metrics — `rpm`/`tpm` = 0, `success_rate` = 1.0,
`avg_tftt` = 0, `tps` = 0 — so any B/C/D/J sort ranks it last and it never gets
picked until it accrues traffic. To let new keys participate from the start, set
a **seed**: `seed_rpm`, `seed_tpm`, `seed_success_rate`, `seed_avg_tftt_ms`,
`seed_tps`. These preset initial values are used only while the key has no live
measurement history; the moment the key serves real traffic the live values take
over (the cooldown prober does **not** count as traffic, so probe-only keys keep
using their seed). Set/clear them per key in the API Key page (`Seed` button) or
via `PUT /api/providers/{id}/keys/{key_id}` (null = clear). All optional; omitted
= built-in defaults, so old configs load unchanged.

---

## 5. Anthropic protocol (Claude Code integration)

The gateway also exposes **Anthropic Messages protocol** endpoints, so Claude Code,
Anthropic SDKs and similar clients connect directly with no translation layer:

```
ANTHROPIC_BASE_URL=http://127.0.0.1:8000   ANTHROPIC_API_KEY=<gateway key>  claude
```

| Endpoint | Description |
|----------|-------------|
| `POST /v1/messages` | Chat (streaming + non-streaming, tools / image blocks) |
| `POST /v1/messages/count_tokens` | Token counting |

- **Auth**: both `Authorization: Bearer <key>` and `x-api-key: <key>` are accepted;
  the `anthropic-version` header is ignored.
- **Provider protocol**: each provider has a `protocol` field (dropdown when adding
  a provider in the console, or `"protocol": "openai" | "anthropic"` in the config;
  default openai):
  - `openai` — upstream is an OpenAI-compatible API (sensenova, openai, ...);
    `/v1/messages` requests are translated to OpenAI format, and the response/stream
    is translated back;
  - `anthropic` — upstream is itself Anthropic-compatible (e.g. api.anthropic.com);
    requests go to `{base_url}/v1/messages` with `x-api-key` + `anthropic-version`.
- **Full matrix**: both inbound endpoints (`/v1/chat/completions`, `/v1/messages`)
  × both upstream protocols — every combination works, so OpenAI clients can use a
  real Claude API key pool too.
- **Translation coverage**: top-level system field, max_tokens (required by
  Anthropic), stop_sequences, tools/tool_choice (function ↔ tool_use both ways),
  image content blocks (base64/URL), streaming event sequences (message_start →
  content_block_delta... → message_stop ↔ OpenAI data: chunks), stop_reason ↔
  finish_reason.
- **count_tokens**: proxied to the real endpoint for Anthropic-protocol upstreams;
  locally estimated (~4 chars/token) for OpenAI upstreams — never 404s.
- The key pool, cooldowns, binary-search probing and stats all apply to Anthropic
  upstreams too (probe bodies are built in the upstream's protocol).

---
## 6. Model management

Per provider, the **模型 / Models** section manages a catalog of allowed
models. Each entry is `{ id, enabled }`:

- **Add manually** — type the upstream model id (e.g. `gpt-4o`).
- **Fetch from upstream** — pulls the provider's live `/v1/models`, shows a
  checkbox list (already-added models are unchecked), select and import.
- **Enable/disable** a model without deleting it (e.g. quota exhausted).
- **Allowlist mode** (仅允许列表内模型, default off):
  - off + empty list → unmanaged, every model passes through (backward
    compatible);
  - on → only enabled models may be requested; anything else gets a clear 404.
- Disabled models and dangling aliases are hidden from `/v1/models`.

---

## 7. Model naming & aliases

Agents request models as:

- **Composite:** `provider/model` — e.g. `openai/gpt-4o` (`provider` matches
  the provider's name or id).
- **Alias:** any short name you define, e.g. `fast` → `gpt-4o-mini`. Aliases
  win over composite names and are global across providers.

`GET /v1/models` lists every enabled managed model as `provider/model` plus
all valid aliases.

---

## 8. Connecting agents & tools

Anything that speaks OpenAI works:

```python
from openai import OpenAI
client = OpenAI(base_url="http://127.0.0.1:8000/v1", api_key="anything")
```

```bash
curl http://127.0.0.1:8000/v1/chat/completions \
  -H "Content-Type: application/json" \
  -d '{"model":"openai/gpt-4o","messages":[{"role":"user","content":"hi"}],"stream":true}'
```

If local API auth is enabled, use one of the gateway's auth keys as
`api_key` (Bearer token).

Streaming (SSE) and non-streaming responses are both supported and passed
through byte-for-byte.

---

## 9. Config file reference (`gateway.json`)

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
      "protocol": "openai",
      "rpm_limit": null,
      "tpm_limit": null,
      "keys": [
        { "id": "k-xxx", "key": "sk-...", "label": "main", "cooldown_secs": null,
          "seed_rpm": null, "seed_tpm": null, "seed_success_rate": null,
          "seed_avg_tftt_ms": null, "seed_tps": null }
      ],
      "models": [ { "id": "gpt-4o", "enabled": true } ],
      "model_allowlist_only": false,
      "aliases": { "fast": "gpt-4o-mini" }
    }
  ]
}
```

| Field | Meaning |
|---|---|
| `api_port` / `ui_port` | Local API and console ports (restart to apply) |
| `default_cooldown_secs` | 429 cooldown when no `Retry-After` and no per-key override |
| `max_attempts` | Retry budget multiplier (effective budget = × key count) |
| `auth` | Optional Bearer auth for the local `/v1` API |
| `keys[].cooldown_secs` | Per-key cooldown override; `null` = global default |
| `keys[].seed_*` | Preset initial metrics (`seed_rpm`/`seed_tpm`/`seed_success_rate`/`seed_avg_tftt_ms`/`seed_tps`); auto-overridden once the key serves traffic. All optional, see §4.3 |
| `providers[].rpm_limit` / `tpm_limit` | Manual RPM/TPM cap; `null` = no cap (console shows the live auto-detected aggregate) |
| `strategy.filter` | `lock_model_group` + `lock_provider` (both ON = exact provider+model); see §4.2 |
| `strategy.sort` | 0–3 of A–J (ordered, no dups) — the user sort stack |
| `model_groups` | Same logical model across providers (entries `[provider_id, upstream_model]`); see §4.2 |
| `models` | Managed catalog; empty = unmanaged |
| `model_allowlist_only` | Reject models not in the (non-empty) list |
| `aliases` | Client-visible name → upstream model |
| `providers[].protocol` | Upstream protocol: `openai` (default) or `anthropic`, see section 5 (added in v0.3.0) |

The file is rewritten atomically on every console change. Editing it by hand
while the gateway runs is fine — but console changes overwrite manual ones.

> **Warning:** `llm-gateway/test/gateway.json` in some working copies contains
> real API keys used for local testing — never commit or share it.

---

## 10. Testing with the mock upstream

```bash
cargo run --bin mock_upstream          # 127.0.0.1:9001
cargo run -- --config test/gateway.json
```

`sk-bad` always returns 429 (`Retry-After: 2`), `sk-dead` always returns 401 —
handy for watching rotation and quarantine in action.

---

## 11. Troubleshooting

| Symptom | Cause & fix |
|---|---|
| Client gets 429 although keys look available | The remaining keys are all cooling/invalid; hover a key's status for its last error. Fix or clear the invalid keys. |
| A key shows 无效 / Invalid | Upstream rejected the auth (401/403) — the key is revoked, mistyped, or out of quota tier. Fix it and click 解除无效 / Clear. |
| Request counter climbs fast with few real requests | Each agent tool call is one API request, and each 429 retry counts per attempt. The 401 quarantine + 429 backoff (added in v0.2) keep this minimal; dead keys now cost at most 1 call per 30 min. |
| `cargo run` asks which binary | Use `cargo run --bin llm-gateway`, or just run the built executable / start scripts. |
| Port already in use | Another gateway instance is running (`taskkill //F //IM llm-gateway.exe` on Windows) or change `api_port`/`ui_port`. |
| Port change didn't take effect | Ports apply on restart. |
| `.bat` window flashes and closes | Re-download the fixed scripts (pure ASCII versions); run them from an existing cmd window to see the error. |

---

## 12. Admin REST API (used by the console)

All on `http://127.0.0.1:8001`:

| Method & path | Purpose |
|---|---|
| `GET /api/status` | Full snapshot: settings + per-provider keys (masked) with cooling/invalid/requests/last_error |
| `GET/POST /api/providers` | List / create providers |
| `PUT/DELETE /api/providers/{id}` | Update / delete a provider |
| `POST /api/providers/{id}/keys` · `DELETE /api/providers/{id}/keys/{key_id}` · `PUT /api/providers/{id}/keys/{key_id}` | Add / remove keys / update a key's metric seeds |
| `PUT /api/providers/{id}/rate-limits` | Set provider RPM/TPM limits (`{rpm_limit, tpm_limit}`, null = clear) |
| `POST /api/strategy/dry-run` | Preview routing for a model (routed key + candidates) |
| `DELETE /api/keys/{key_id}/cooldown` | Clear a key's cooldown **or** invalid quarantine |
| `POST /api/providers/{id}/models` · `DELETE .../models/{model_id}` · `PUT .../models/{model_id}` | Add / remove / toggle a managed model |
| `GET /api/providers/{id}/upstream-models` | Fetch the provider's live `/v1/models` |
| `POST /api/providers/{id}/models/import` | Bulk import `{ "models": ["a", "b"] }` |
| `POST /api/providers/{id}/aliases` · `DELETE .../aliases/{alias}` | Set / delete an alias |
| `PUT /api/settings` | Update global settings |
