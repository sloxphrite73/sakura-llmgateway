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
| Web console | `http://127.0.0.1:8001/` |

---

## 2. Web console

Open `http://127.0.0.1:8001/`. From top to bottom:

### Global settings

- **Default cooldown (seconds)** — how long a 429'd key rests when the upstream
  sends no `Retry-After` and the key has no override (default 60).
- **Max attempts per request** — retry budget multiplier (default 3). The
  effective budget is `max_attempts × number of keys`, so a request only fails
  after every available key has been tried.
- **API port / UI port** — take effect after a restart.
- **Local API auth** — off by default. When enabled, agents must present one of
  the configured keys as a Bearer token on `/v1/*`.

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

### Theme

The ☀️/🌙 button in the masthead switches between light and dark mode. The
choice is remembered per browser (localStorage); dark is the default.

---

## 3. Key pool behavior (what happens on 429)

1. Requests are distributed round-robin across the provider's keys.
2. A key that receives **429** is benched: cooldown = upstream `Retry-After`
   header → per-key override → global default. The gateway waits **1.5s** and
   retries the same request on the next key.
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

---

## 4. Model management

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

## 5. Model naming & aliases

Agents request models as:

- **Composite:** `provider/model` — e.g. `openai/gpt-4o` (`provider` matches
  the provider's name or id).
- **Alias:** any short name you define, e.g. `fast` → `gpt-4o-mini`. Aliases
  win over composite names and are global across providers.

`GET /v1/models` lists every enabled managed model as `provider/model` plus
all valid aliases.

---

## 6. Connecting agents & tools

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

## 7. Config file reference (`gateway.json`)

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
| `models` | Managed catalog; empty = unmanaged |
| `model_allowlist_only` | Reject models not in the (non-empty) list |
| `aliases` | Client-visible name → upstream model |

The file is rewritten atomically on every console change. Editing it by hand
while the gateway runs is fine — but console changes overwrite manual ones.

> **Warning:** `llm-gateway/test/gateway.json` in some working copies contains
> real API keys used for local testing — never commit or share it.

---

## 8. Testing with the mock upstream

```bash
cargo run --bin mock_upstream          # 127.0.0.1:9001
cargo run -- --config test/gateway.json
```

`sk-bad` always returns 429 (`Retry-After: 2`), `sk-dead` always returns 401 —
handy for watching rotation and quarantine in action.

---

## 9. Troubleshooting

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

## 10. Admin REST API (used by the console)

All on `http://127.0.0.1:8001`:

| Method & path | Purpose |
|---|---|
| `GET /api/status` | Full snapshot: settings + per-provider keys (masked) with cooling/invalid/requests/last_error |
| `GET/POST /api/providers` | List / create providers |
| `PUT/DELETE /api/providers/{id}` | Update / delete a provider |
| `POST /api/providers/{id}/keys` · `DELETE /api/providers/{id}/keys/{key_id}` | Add / remove keys |
| `DELETE /api/keys/{key_id}/cooldown` | Clear a key's cooldown **or** invalid quarantine |
| `POST /api/providers/{id}/models` · `DELETE .../models/{model_id}` · `PUT .../models/{model_id}` | Add / remove / toggle a managed model |
| `GET /api/providers/{id}/upstream-models` | Fetch the provider's live `/v1/models` |
| `POST /api/providers/{id}/models/import` | Bulk import `{ "models": ["a", "b"] }` |
| `POST /api/providers/{id}/aliases` · `DELETE .../aliases/{alias}` | Set / delete an alias |
| `PUT /api/settings` | Update global settings |
