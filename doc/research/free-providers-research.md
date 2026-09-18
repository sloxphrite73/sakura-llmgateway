# Free OpenAI-Compatible LLM Providers — Research for Sakura LLM Gateway Catalog

> Research date: verified against each provider's **live official docs** (API reference, pricing pages, model catalogs, code examples) via direct fetches. All model IDs, base URLs, and free-tier terms below were confirmed against primary sources, not third-party write-ups.
>
> **Important note on model versions:** The providers' live docs currently reflect late-2026 model generations (e.g. `gemini-3.8-flash`, `gpt-oss-120b`, `GLM 5.x`, `doubao-seed-2.x`, `command-a-plus-05-2026`). These are the *exact strings the providers publish today*, verified straight from their own docs/models pages — not invented. Model IDs in this space rotate fast (especially date-suffixed and "flash/lite" variants); the implementer should re-confirm current IDs from the cited live pages before shipping the catalog.
>
> **Already in the catalog (do NOT re-add):** SiliconFlow, Z.AI, OpenRouter `:free`, Pollinations, Cerebras, NVIDIA NIM, Groq, Mistral, Cloudflare Workers AI.

---

## Summary table

| # | Provider | Accepted? | base_url | Keyless? | Why accepted / rejected (one line) | Primary source |
|---|----------|-----------|----------|----------|-------------------------------------|-----------------|
| 1 | Google AI Studio (Gemini) | ✅ ACCEPTED | `https://generativelanguage.googleapis.com/v1beta/openai/` | No | OpenAI-compat endpoint + free tier (RPM/RPD, no card) + 4 verified Flash model IDs | https://ai.google.dev/gemini-api/docs/openai |
| 2 | DeepSeek | ❌ REJECTED | — | — | OpenAI-compat, but free tier not stated on any public doc (pay-as-you-go; quota deferred to login-gated platform) | https://api-docs.deepseek.com/quick_start/pricing |
| 3 | Moonshot / Kimi | ✅ ACCEPTED | `https://api.moonshot.cn/v1` | No | OpenAI-compat + 15 CNY signup voucher after real-name auth (no payment method) | https://platform.kimi.com/docs/api/chat |
| 4 | GitHub Models | ❌ REJECTED | — | — | Fully retired 2026-07-30; inference API/playground/catalog gone | https://docs.github.com/en/github-models |
| 5 | SambaNova | ✅ ACCEPTED | `https://api.sambanova.ai/v1` | No | OpenAI-compat + Free Tier (no card, daily reset) + 3+ verified model IDs | https://docs.sambanova.ai/docs/en/features/openai-compatibility |
| 6 | Hugging Face Router | ✅ ACCEPTED | `https://router.huggingface.co/v1` | No | OpenAI-compat + recurring free monthly credit (no card) + verified chat model IDs | https://huggingface.co/docs/inference-providers/index |
| 7 | Together AI | ❌ REJECTED | — | — | Official docs: "does not currently offer free trials"; $5 min purchase + payment method required | https://docs.together.ai/docs/billing-credits |
| 8 | DeepInfra | ❌ REJECTED | — | — | Pure pay-as-you-go per-token; no free tier, no free credits, no $0 models | https://deepinfra.com/pricing |
| 9 | Fireworks AI | ✅ ACCEPTED | `https://api.fireworks.ai/inference/v1` | No | OpenAI-compat + "$1 in free credits" on self-serve signup (no card to start) | https://fireworks.ai/pricing |
| 10 | Chutes AI | ❌ REJECTED | — | — | Official pricing: "We do not offer a free tier at this time"; Bearer key required | https://chutes.ai/pricing |
| 11 | Hyperbolic | ✅ ACCEPTED (caveat) | `https://api.hyperbolic.xyz/v1` | No | OpenAI-compat (live, returns 401) + $1 promo credit on phone verify (no card); inference docs now archived | https://docs.hyperbolic.ai/docs/general/billing-payments |
| 12 | Novita AI | ✅ ACCEPTED | `https://api.novita.ai/openai/v1` | No | OpenAI-compat + $1 no-card voucher + $0 "TIME LIMITED FREE" models (verified $0 on model page) | https://novita.ai/models/model-detail/inclusionai-ling-3.0-flash-fin |
| 13 | Requesty | ✅ ACCEPTED | `https://router.requesty.ai/v1` | No | OpenAI-compat + 200 req/day free (no card, no trial expiry) + 12 verified free model IDs | https://www.requesty.ai/models/free |
| 14 | Cohere | ✅ ACCEPTED | `https://api.cohere.ai/compatibility/v1` | No | OpenAI Compatibility API + free trial key (1,000 calls/mo, no card) + verified live model IDs | https://docs.cohere.com/docs/compatibility-api |
| 15 | Alibaba DashScope (Bailian) | ✅ ACCEPTED | `https://dashscope.aliyuncs.com/compatible-mode/v1` | No | OpenAI-compat + 90-day free quota on activation (no payment method) + qwen-plus/qwen-max | https://help.aliyun.com/zh/model-studio/compatibility-of-openai-with-dashscope |
| 16 | Volcengine Ark (Doubao) | ✅ ACCEPTED | `https://ark.cn-beijing.volces.com/api/v3` | No | OpenAI-compat + 安心体验模式 free quota (real-name auth, no card) + verified doubao-seed IDs | https://www.volcengine.com/product/ark |
| 17 | Tencent Hunyuan | ❌ REJECTED | — | — | Platform decommissioned 2026-09-30; free models offline since 2026-06-22; migration target needs post-payment gate | https://cloud.tencent.com/document/product/1729/131925 |
| 18 | Lepton AI | ❌ REJECTED | — | — | Acquired by NVIDIA; old hosted `api.lepton.ai/v1` dead; current product is GPU deployment (no stable hosted free base_url) | https://docs.nvidia.com/dgx-cloud/lepton/get-started/ |
| 19 | AnyAPI | ❌ REJECTED | — | — | Free daily-quota tier exists, but exact currently-valid free model IDs unverifiable (live catalog Cloudflare-gated; proposed IDs not current) | https://anyapi.ai/ai-models |

**Result: 11 newly-accepted providers** (Google, Moonshot, SambaNova, HuggingFace, Fireworks, Hyperbolic, Novita, Requesty, Cohere, Alibaba DashScope, Volcengine Ark); 8 rejected.

---

## Accepted providers (catalog-ready)

### 1. Google AI Studio (Gemini)

- **Verdict: ACCEPTED** — Official Google docs document a raw-HTTP OpenAI-compatible endpoint (`POST /v1beta/openai/chat/completions` with `Authorization: Bearer` + OpenAI body), a free tier requiring no payment method (RPM/RPD quotas reset daily at midnight PT), and 4 confirmed free Flash model IDs.

**Catalog fields:**
- `id`: `google-ai-studio`
- `name`: `Google AI Studio (Gemini)`
- `base_url`: `https://generativelanguage.googleapis.com/v1beta/openai/`
- `protocol`: `openai`
- `keyless`: `false`
- `signup_url`: `https://aistudio.google.com/apikey`
- `free_models`:
  - `gemini-3.8-flash`
  - `gemini-3.7-flash`
  - `gemini-3.6-flash`
  - `gemini-3.5-flash`
- `notes_zh`: `免费层无需绑卡；RPM/RPD 配额每日太平洋时间午夜重置；gemini-3.x-flash 系列在免费层为 $0（需在 aistudio.google.com 申请 API key）。`
- `notes_en`: `Free tier needs no payment method; RPM/RPD quotas reset daily at midnight PT; gemini-3.x-flash models are $0 under the free tier (a Gemini API key from aistudio.google.com is required).`

**Primary sources verified:**
- https://ai.google.dev/gemini-api/docs/openai — OpenAI-compat endpoint & base path
- https://ai.google.dev/gemini-api/docs/models — exact current model IDs (gemini-3.8-flash, 3.7-flash, 3.6-flash, 3.5-flash all listed as Stable)
- https://ai.google.dev/gemini-api/docs/pricing — free-tier $0 pricing on Flash models
- https://ai.google.dev/gemini-api/docs/rate-limits — RPM/RPD quotas, daily reset

**Gotchas:**
- base_url ends in `/v1beta/openai/` (NOT `/v1`) — Google's documented OpenAI-compat path; use verbatim with the trailing slash.
- A Gemini API key from aistudio.google.com is **required** (not keyless).
- Free-tier $0 pricing is governed by per-project (not per-key) RPM/RPD rate limits; RPD resets daily at midnight Pacific time, so heavy burst usage will 429.
- Free-tier content may be used to improve Google products (data logging on by default for the free tier) — disable data sharing in project settings if that matters.
- Model IDs are versioned and rotate (e.g. `gemini-3.x-flash`); verify the live models page before hard-coding.

---

### 2. Moonshot / Kimi

- **Verdict: ACCEPTED** — OpenAI-compatible `https://api.moonshot.cn/v1` confirmed via raw HTTP in official docs (Bearer + OpenAI-shaped body/response); 15 CNY free signup voucher after real-name auth (no payment method) usable on `kimi-k2.6` & `kimi-k2.7-code`; exact model IDs cross-verified on Fireworks' live pricing page (which lists Moonshot's "Kimi K2.6" and "Kimi K2.7 Code").

**Catalog fields:**
- `id`: `moonshot`
- `name`: `Moonshot Kimi (Kimi API 开放平台)`
- `base_url`: `https://api.moonshot.cn/v1`
- `protocol`: `openai`
- `keyless`: `false`
- `signup_url`: `https://platform.kimi.com`
- `free_models`:
  - `kimi-k2.6`
  - `kimi-k2.7-code`
- `notes_zh`: `注册并完成实名认证后赠送 15 元代金券（无需支付方式），可用于 kimi-k2.6 / kimi-k2.7-code（旗舰 kimi-k3 不支持代金券）；额度用尽后按量付费。`
- `notes_en`: `15 CNY free voucher on signup after real-name auth (no payment method); usable on kimi-k2.6 / kimi-k2.7-code (NOT flagship kimi-k3); pay-as-you-go after credits are spent.`

**Primary sources verified:**
- https://platform.kimi.com/docs/api/chat — API + base_url
- https://platform.kimi.com/docs/models — model IDs
- https://platform.kimi.com/docs/guide/account-and-payments — 15 CNY voucher terms
- https://docs.fireworks.ai/serverless/pricing — cross-confirms `Kimi K2.6` and `Kimi K2.7 Code` are real current Moonshot models (Fireworks hosts them)

**Gotchas:**
- Free tier is a **one-time** 15 CNY voucher (not recurring); granted only after Chinese real-name authentication (个人认证) — this is identity verification (ID/phone), **not** a payment method, but may be a hard barrier for users without a Chinese ID/phone.
- The voucher explicitly does **not** cover the flagship `kimi-k3` (the default model in docs); only `kimi-k2.6` / `kimi-k2.7-code`.
- Legacy IDs `moonshot-v1-8k` / `moonshot-v1-32k` / `moonshot-v1-128k` / `moonshot-v1-auto` were decommissioned 2026-08-31 — do NOT use them.
- Docs domain redirected from `platform.moonshot.cn` to `platform.kimi.com`, but the API base_url remains `api.moonshot.cn/v1`.

---

### 3. SambaNova

- **Verdict: ACCEPTED** — OpenAI-compatible raw HTTP base_url `https://api.sambanova.ai/v1` with `/chat/completions` + Bearer auth; Free Tier requires **no payment method** and resets daily (20 RPM / 20 RPD / 200K TPD per model); 3+ confirmed exact free model IDs from the official models page.

**Catalog fields:**
- `id`: `sambanova`
- `name`: `SambaNova (SambaCloud)`
- `base_url`: `https://api.sambanova.ai/v1`
- `protocol`: `openai`
- `keyless`: `false`
- `signup_url`: `https://cloud.sambanova.ai/`
- `free_models`:
  - `Meta-Llama-3.3-70B-Instruct`
  - `DeepSeek-V3.1`
  - `gpt-oss-120b`
- `notes_zh`: `免费层无需绑卡：每模型 20 RPM / 20 RPD / 20万 TPD，每日重置。免费模型含 Meta-Llama-3.3-70B-Instruct、DeepSeek-V3.1、gpt-oss-120b。`
- `notes_en`: `Free Tier needs no payment method: 20 RPM / 20 RPD / 200K TPD per model, resets daily. Free models: Meta-Llama-3.3-70B-Instruct, DeepSeek-V3.1, gpt-oss-120b.`

**Primary sources verified:**
- https://docs.sambanova.ai/docs/en/get-started/api-keys-urls — base_url + API key URL
- https://docs.sambanova.ai/docs/en/features/openai-compatibility — OpenAI-compat (chat/completions + Bearer)
- https://docs.sambanova.ai/docs/en/models/sambacloud-models — exact model IDs (`Meta-Llama-3.3-70B-Instruct`, `DeepSeek-V3.1`, `gpt-oss-120b`, preview `gemma-4-31B-it`/`DeepSeek-V3.2`)
- https://docs.sambanova.ai/docs/en/models/rate-limits — Free Tier 20 RPM/20 RPD/200K TPD, daily reset

**Gotchas:**
- API key required (not keyless; up to 25 keys per account, generated at **cloud.sambanova.ai** — note the hinted `platform.sambanova.ai` is dead/Cloudflare DNS error).
- base_url is exactly `https://api.sambanova.ai/v1` (ends in `/v1`).
- Model IDs are case/suffix-sensitive (`Meta-Llama-3.3-70B-Instruct`, `DeepSeek-V3.1`, `gpt-oss-120b`).
- `MiniMax-M2.7` is on SambaCloud but is **Developer/paid only** — do not list it as free.
- Preview models `DeepSeek-V3.2` and `gemma-4-31B-it` are also free but may be removed at short notice.
- OpenAI's `presence_penalty`/`frequency_penalty` are silently ignored; SambaNova adds `top_k` (not exposed by the OpenAI client).

---

### 4. Hugging Face Router (Inference Providers)

- **Verdict: ACCEPTED** — Official HF docs document a raw OpenAI-compatible HTTP base_url (`https://router.huggingface.co/v1/chat/completions` with Bearer token + OpenAI body) and a recurring free monthly credit (no payment method) usable on chat models, with confirmed exact model IDs in the docs' own curl examples.

**Catalog fields:**
- `id`: `huggingface-router`
- `name`: `Hugging Face Router (Inference Providers)`
- `base_url`: `https://router.huggingface.co/v1`
- `protocol`: `openai`
- `keyless`: `false`
- `signup_url`: `https://huggingface.co/settings/tokens`
- `free_models`:
  - `openai/gpt-oss-120b`
  - `deepseek-ai/DeepSeek-R1`
- `notes_zh`: `免费用户每月有循环免费额度（无需支付方式），可用于所有 Inference Providers 聊天模型，每月重置；需免费申请 HF token。`
- `notes_en`: `Free users get a recurring monthly credit usable on all Inference Providers chat models; resets monthly; no payment method needed; a free Hugging Face token is required.`

**Primary sources verified:**
- https://huggingface.co/docs/inference-providers/index — base_url `https://router.huggingface.co/v1`, curl example with `openai/gpt-oss-120b` and `deepseek-ai/DeepSeek-R1`, "generous free tier" statement
- https://huggingface.co/docs/inference-providers/billing — free credit terms (subagent-verified; current monthly amount — confirm on the live billing page)

**Gotchas:**
- Free tier is small (historically ~$0.10/month, subject to change) — easily exhausted by a single long conversation. Treat as a "try it" tier, not production.
- No individually $0-priced models: every model is pay-as-you-go, covered only up to the recurring monthly credit.
- A free Hugging Face token (fine-grained, with "Make calls to Inference Providers" permission) is required — **not keyless**.
- The OpenAI-compatible `/v1` endpoint is **chat-completions only** (no embeddings/images there).
- Model IDs accept optional routing suffixes appended with a colon: `:fastest` (default), `:cheapest`, `:preferred`, or `:<provider>` (e.g. `:groq`). The suffix is part of the model string the user sends.
- Credits apply only to HF-routed requests (not custom provider keys).

---

### 5. Fireworks AI

- **Verdict: ACCEPTED** — OpenAI-compatible raw HTTP base_url `https://api.fireworks.ai/inference/v1` (confirmed via docs curl with `Authorization: Bearer`); "$1 in free credits" on self-serve signup ("Start building in seconds, self-serve" — no card to start) confirmed on the official pricing page; 3 exact current model IDs confirmed on the live serverless pricing table.

**Catalog fields:**
- `id`: `fireworks-ai`
- `name`: `Fireworks AI`
- `base_url`: `https://api.fireworks.ai/inference/v1`
- `protocol`: `openai`
- `keyless`: `false`
- `signup_url`: `https://fireworks.ai/signup`
- `free_models`:
  - `accounts/fireworks/models/glm-5p2`
  - `accounts/fireworks/models/gpt-oss-120b`
  - `accounts/fireworks/models/deepseek-v4-flash-0731`
- `notes_zh`: `新账户注册即获一次性 $1 免费额度（邮箱/社交自助注册、无需信用卡即可开始），可用于任意按量计费的 serverless 模型；额度用尽后需添加付款方式购买更多额度。`
- `notes_en`: `New accounts get a one-time $1 free credit via self-serve email/social signup (no card needed to start), usable on any per-token serverless model; add a payment method only to buy more credits once it runs out.`

**Primary sources verified:**
- https://fireworks.ai/pricing — "Serverless Inference … Get started with $1 in free credits." + "Start building in seconds, self-serve"
- https://docs.fireworks.ai/serverless/pricing — exact current model IDs (`glm-5p2`, `gpt-oss-120b`, `deepseek-v4-flash-0731`, `kimi-k2p6`, etc.)
- https://docs.fireworks.ai/tools-sdks/openai-compatibility — OpenAI-compat base_url + Bearer
- https://docs.fireworks.ai/getting-started/quickstart — quickstart
- https://docs.fireworks.ai/faq-new/billing-pricing/how-does-billing-and-credit-usage-work — pre-paid credits model (payment method required only to *purchase* more credits)

**Gotchas:**
- **Not keyless**: an API key (Bearer token) is required, created in the dashboard at `app.fireworks.ai`.
- The $1 free credit is **one-time and non-recurring** (not a daily/monthly reset); once depleted you must add a payment method to purchase more credits (pre-paid system).
- base_url is `https://api.fireworks.ai/inference/v1` (note the `/inference/v1` path, not bare `/v1`).
- Model IDs must use the `accounts/fireworks/models/<name>` prefix.
- Some serverless models are US-only; from Sept 2026 US-only models carry a 1.5x price premium (drains the $1 faster).
- The earlier-researched `llama-v3p1-8b-instruct` / `deepseek-v3p1` IDs are **no longer** in the current pricing table — use the current IDs above instead.

---

### 6. Hyperbolic (Hyperbolic Labs)

- **Verdict: ACCEPTED (with caveat)** — OpenAI-compatible base_url `https://api.hyperbolic.xyz/v1` is live (probes return 401, i.e. auth required, not 404) and documented in Hyperbolic's own inference docs; the current live Account Management/Billing docs confirm a no-payment-method free tier with inference access + $1 phone-verification credit; ≥2 model IDs confirmed from Hyperbolic's own (now-archived) inference docs.

**Catalog fields:**
- `id`: `hyperbolic`
- `name`: `Hyperbolic (Hyperbolic Labs)`
- `base_url`: `https://api.hyperbolic.xyz/v1`
- `protocol`: `openai`
- `keyless`: `false`
- `signup_url`: `https://app.hyperbolic.ai/signup`
- `free_models`:
  - `meta-llama/Meta-Llama-3-70B-Instruct`
  - `meta-llama/Meta-Llama-3.1-405B-Instruct`
- `notes_zh`: `免费层无需付款方式：手机验证赠 $1 信用 + 60 RPM（405B 为 5 RPM）；超出按 token 计费。OpenAI 兼容地址 https://api.hyperbolic.xyz/v1；模型 id 为 HF 仓库格式。`
- `notes_en`: `Free tier needs no payment method: $1 promo credit on phone verification + 60 RPM (5 RPM for 405B); per-token pricing beyond the credit. OpenAI-compatible at https://api.hyperbolic.xyz/v1; model ids are HF repo form.`

**Primary sources verified:**
- https://docs.hyperbolic.ai/docs/general/account-management — current live docs (free tier, no payment method)
- https://docs.hyperbolic.ai/docs/general/billing-payments — $1 phone-verification credit
- https://web.archive.org/web/20250613052051/https://docs.hyperbolic.xyz/docs/inference-api — base_url `https://api.hyperbolic.xyz/v1` + OpenAI-compat (archived inference docs; product has since pivoted toward GPU compute)
- https://web.archive.org/web/20251030052837/https://docs.hyperbolic.xyz/docs/hyperbolic-pricing — model IDs (HF repo form) + Basic tier 60 RPM

**Gotchas (read before shipping):**
- The inference API reference is **no longer in the current live docs** — `docs.hyperbolic.ai` pivoted to GPU compute; base_url and exact model IDs are confirmed from **archived** provider inference docs (Jun–Oct 2025) plus a live probe (`api.hyperbolic.xyz/v1/models` returns 401, host alive). The free tier itself is still confirmed in the **current** live Account Management/Billing docs.
- Free amount is small and **one-time**: $1 promo credit granted on phone verification (no payment method; not a recurring quota); per-token pricing applies beyond it.
- Model IDs use Hugging Face repo form (`meta-llama/Meta-Llama-3-70B-Instruct`), NOT the short display names ("Llama 3 70B").
- The live model catalog (`app.hyperbolic.xyz/models` → `app.hyperbolic.ai/models`) is **login-gated**; the current exact model list could not be re-verified (latest snapshot Oct 2025). The two IDs above are stable, widely-hosted open models and very likely still served, but confirm against the live console before shipping.
- Domain split: API base_url is `api.hyperbolic.xyz` (still live, not redirected); the app/dashboard is now `app.hyperbolic.ai` (`app.hyperbolic.xyz` redirects to `.ai`).
- If the implementer cannot confirm the current model catalog is still hosted, prefer to omit Hyperbolic rather than ship stale IDs.

---

### 7. Novita AI

- **Verdict: ACCEPTED** — OpenAI-compatible raw HTTP base_url (confirmed via official curl + code example with Bearer); genuinely free right now via a $1 no-card signup voucher plus $0 "TIME LIMITED FREE" models (`inclusionai/ling-3.0-flash-fin` priced **$0 in / $0 out** on the provider's own model-detail page) with exact IDs confirmed from provider model-detail pages.

**Catalog fields:**
- `id`: `novita-ai`
- `name`: `Novita AI`
- `base_url`: `https://api.novita.ai/openai/v1`
- `protocol`: `openai`
- `keyless`: `false`
- `signup_url`: `https://novita.ai/console`
- `free_models`:
  - `inclusionai/ling-3.0-flash-fin`
  - `inclusionai/ling-3.0-flash-sante`
- `notes_zh`: `注册即赠 $1 信用额度且无需绑卡；另有 inclusionai/ling-3.0-flash-fin、ling-3.0-flash-sante 等“限时免费” $0 模型。OpenAI 兼容 base_url 为 https://api.novita.ai/openai/v1，使用 Bearer key。`
- `notes_en`: `Genuinely free right now: $1 signup credit voucher (no payment method required) plus $0 "TIME LIMITED FREE" models (inclusionai/ling-3.0-flash-fin, inclusionai/ling-3.0-flash-sante). OpenAI-compatible base_url https://api.novita.ai/openai/v1, Bearer auth.`

**Primary sources verified:**
- https://novita.ai/models/model-detail/inclusionai-ling-3.0-flash-fin — model is real, priced **$0 in / $0 out** (124B MoE, 5.1B activated), with official code example `base_url="https://api.novita.ai/openai"` + Bearer + `model="inclusionai/ling-3.0-flash-fin"`
- https://docs.novita.ai/guides/llm-api.md — OpenAI-compat endpoint
- https://docs.novita.ai/guides/quickstart — quickstart + $1 voucher
- https://docs.novita.ai/guides/faq — voucher terms
- https://novita.ai/models — model library

**Gotchas:**
- The $0 models are **"TIME LIMITED FREE"** promotional models and may end at any time; the $1 signup voucher is one-time and small (revoked for fraudulent/duplicate sign-ups).
- **base_url nuance**: the OpenAI SDK examples use `https://api.novita.ai/openai` (no `/v1`) and the SDK appends `/chat/completions`; the curl-documented raw-HTTP endpoint is `https://api.novita.ai/openai/v1/chat/completions`. Use the `/v1` form (`https://api.novita.ai/openai/v1`) so a raw `POST {base_url}/chat/completions` matches the catalog convention; both forms work.
- Model ID prefix is lowercase `inclusionai/` (no hyphen), e.g. `inclusionai/ling-3.0-flash-fin`.
- Some free models also expose an Anthropic API option on their detail page; use the OpenAI-compatible endpoint.
- `ling-3.0-flash-fin` is directly verified $0; `ling-3.0-flash-sante` is cited from the provider's model-detail page (same InclusionAI Ling-3.0-flash family, domain = health) — confirm both are still listed as $0 before shipping.

---

### 8. Requesty (LLM Router)

- **Verdict: ACCEPTED** — Official docs + live free-models catalog confirm OpenAI-compatible base_url `https://router.requesty.ai/v1` with Bearer auth and `/v1/chat/completions`; free tier gives 200 requests/day with no credit card and no trial expiry, restricted to free models; 12 exact free model IDs enumerated on the live `requesty.ai/models/free` page.

**Catalog fields:**
- `id`: `requesty`
- `name`: `Requesty (LLM Router)`
- `base_url`: `https://router.requesty.ai/v1`
- `protocol`: `openai`
- `keyless`: `false`
- `signup_url`: `https://app.requesty.ai/sign-up`
- `free_models`:
  - `nvidia/nemotron-3-super-120b-a12b`
  - `google/gemma-4-31b-it`
  - `mistral/leanstral-1-5`
  - `meta/muse-glimmer-30b`
  - `inclusionai/ling-3.0-tiny`
  - `nvidia/nemotron-3-ultra-550b-a55b`
- `notes_zh`: `免费层每日 200 次请求、无需信用卡，仅限免费模型（如 nvidia/nemotron-3-super-120b-a12b）；需注册获取 Bearer API key（非免密钥）。`
- `notes_en`: `Free tier: 200 requests/day, no credit card, no trial expiry, free models only (e.g. nvidia/nemotron-3-super-120b-a12b); signup required for a Bearer API key (not keyless).`

**Primary sources verified:**
- https://www.requesty.ai/free-models — "200 requests per day … No credit card, no trial timer"; "point your OpenAI-compatible client at https://router.requesty.ai/v1"
- https://www.requesty.ai/models/free — live catalog enumerating 12 exact free model IDs (all marked "Free")
- https://docs.requesty.ai/quickstart — base_url + Bearer
- https://docs.requesty.ai/api-reference/overview — API reference
- https://www.requesty.ai/pricing — pricing

**Gotchas:**
- Official base_url is `https://router.requesty.ai/v1` (**NOT** `api.requesty.ai/v1` as some hints suggested).
- Not keyless: requires signup at `app.requesty.ai` to generate a Bearer API key.
- Free models are marked "free for now" and the catalogue **changes over time**; confirm current free IDs via the live model list (`requesty.ai/models/free`) or `GET /v1/models` before shipping.
- Quota is 200 requests/day and 20 requests/min for new orgs.
- Some free models are **region-locked** (e.g. `mistral/leanstral-1-5` EU-only; `poolside/*` and most `nvidia/*` US-only).
- Requesty also exposes an Anthropic-protocol endpoint (`ANTHROPIC_BASE_URL=https://router.requesty.ai`) for Claude Code, but the OpenAI-compatible `/v1/chat/completions` path is the verified one for this catalog.

---

### 9. Cohere

- **Verdict: ACCEPTED** — Cohere exposes a raw-HTTP OpenAI-compatible endpoint (`https://api.cohere.ai/compatibility/v1/chat/completions`, Bearer auth, OpenAI JSON body) per official docs (exact base_url in multiple code examples); free trial key needs no payment method (1,000 calls/month) with multiple confirmed live model IDs.

**Catalog fields:**
- `id`: `cohere`
- `name`: `Cohere`
- `base_url`: `https://api.cohere.ai/compatibility/v1`
- `protocol`: `openai`
- `keyless`: `false`
- `signup_url`: `https://dashboard.cohere.com/api-keys`
- `free_models`:
  - `command-a-plus-05-2026`
  - `command-a-03-2025`
  - `command-r-08-2024`
  - `command-r-plus-08-2024`
- `notes_zh`: `免费试用 API key（无需信用卡），每月 1,000 次调用上限，每模型 20 req/min；通过 OpenAI 兼容端点使用。`
- `notes_en`: `Free trial API key (no credit card), 1,000 calls/month cap + 20 req/min per model; works via the OpenAI-compat endpoint.`

**Primary sources verified:**
- https://docs.cohere.com/docs/compatibility-api.md — exact base_url `https://api.cohere.ai/compatibility/v1` (Python/TS/cURL examples), model `command-a-plus-05-2026`, `Authorization: Bearer`
- https://docs.cohere.com/docs/models — `command-a-plus-05-2026`, `command-a-03-2025`, `command-r-08-2024`, `command-r-plus-08-2024` all marked "Live"; bare `command-r`/`command-r-plus` deprecated Sept 15, 2025
- https://docs.cohere.com/docs/rate-limits — trial key 1,000 calls/month + 20 req/min
- https://docs.cohere.com/v2/docs/how-does-cohere-pricing-work — trial key needs no payment method

**Gotchas:**
- base_url is `https://api.cohere.ai/compatibility/v1` (note the **.ai** TLD + `/compatibility/v1` path — NOT a bare `/v1`). The docs' audio-transcription example confusingly uses `api.cohere.com` for that one endpoint, but all chat-completions examples use `api.cohere.ai/compatibility/v1`; use the latter.
- Trial key limited to 1,000 calls/month total + 20 req/min per model.
- The bare aliases `command-r` and `command-r-plus` are **DEPRECATED** as of Sept 15, 2025 — use the dated variants (`command-r-plus-08-2024`, `command-a-03-2025`, etc.).
- The Compatibility API drops some OpenAI params (`store`, `metadata`, `logit_bias`, `n`, `top_logprobs`, `modalities`, etc.) and only supports `reasoning_effort` none/high.

---

### 10. Alibaba Cloud Bailian (DashScope)

- **Verdict: ACCEPTED** — Verified OpenAI-compatible raw-HTTP endpoint (official curl with `Authorization: Bearer`) at `https://dashscope.aliyuncs.com/compatible-mode/v1`; 90-day free quota auto-granted on activation and usable with no payment method (unverified users OK); `qwen-plus` and `qwen-max` confirmed as free model IDs.

**Catalog fields:**
- `id`: `alibaba-dashscope`
- `name`: `Alibaba Cloud Bailian (DashScope)`
- `base_url`: `https://dashscope.aliyuncs.com/compatible-mode/v1`
- `protocol`: `openai`
- `keyless`: `false`
- `signup_url`: `https://help.aliyun.com/zh/model-studio/get-api-key`
- `free_models`:
  - `qwen-plus`
  - `qwen-max`
- `notes_zh`: `首次开通阿里云百炼即自动发放各模型 90 天新人免费额度（如 qwen-plus/qwen-max，约 100 万 Token/模型）；未认证用户也可调用，无需绑定支付方式；额度耗尽或到期后转为按量付费。`
- `notes_en`: `Free quota auto-granted on Model Studio activation (~1M tokens per model, valid 90 days, e.g. qwen-plus/qwen-max); usable by unverified accounts with no payment method bound; switches to pay-as-you-go only after quota exhaustion/expiry.`

**Primary sources verified:**
- https://help.aliyun.com/zh/model-studio/compatibility-of-openai-with-dashscope — OpenAI-compat endpoint + curl with `Authorization: Bearer`
- https://help.aliyun.com/zh/model-studio/new-free-quota — 90-day free quota terms
- https://help.aliyun.com/zh/model-studio/get-api-key — API key + signup
- https://www.alibabacloud.com/help/en/model-studio/first-api-call-to-qwen — first call
- https://www.alibabacloud.com/help/en/model-studio/model-pricing — pricing/free rows
- https://help.aliyun.com/zh/model-studio/rate-limit — rate limits

**Gotchas:**
- Free quota exists **only in the Beijing (华北2) region** for the China product; API keys are region-specific and not interchangeable across Beijing/Virginia/Singapore.
- The legacy base_url `https://dashscope.aliyuncs.com/compatible-mode/v1` still works, but newer docs prefer the workspace-specific form `https://{WorkspaceId}.cn-beijing.maas.aliyuncs.com/compatible-mode/v1` (requires the user's WorkspaceId). Use the legacy form for the catalog (works without a WorkspaceId).
- Free quota is a **one-time 90-day grant** (NOT recurring daily/monthly) and is non-renewable per real-name entity.
- Use the unversioned aliases (`qwen-plus`, `qwen-max`) for stability; the exact per-model free-token amounts and model lineup change — always check the live pricing page.
- `qwen-turbo` is also a current model but confirm its free-quota row before relying on it.

---

### 11. Volcengine Ark (Doubao)

- **Verdict: ACCEPTED** — OpenAI-compatible base_url `https://ark.cn-beijing.volces.com/api/v3` verified in official docs with Bearer auth + OpenAI-shaped body; new users get a free quota via 安心体验模式 (safe-experience mode) with real-name authentication (ID verification, **no payment method**); 2+ free `doubao-seed-*` model IDs confirmed from official docs/code examples and the live model-price page (API IDs use dashed+date-suffix format).

> Note: **Tencent Hunyuan is REJECTED** (separate candidate #17 below). Only Volcengine Ark (Doubao) is accepted here.

**Catalog fields:**
- `id`: `volcengine-ark`
- `name`: `Volcengine Ark (Doubao)`
- `base_url`: `https://ark.cn-beijing.volces.com/api/v3`
- `protocol`: `openai`
- `keyless`: `false`
- `signup_url`: `https://console.volcengine.com/ark`
- `free_models`:
  - `doubao-seed-2-0-lite-260428`
  - `doubao-seed-1-6-251015`
- `notes_zh`: `火山方舟新用户经实名认证后可享安心体验模式：50 万 token 免费额度，无需绑卡，耗尽即停；每个模型另有免费额度。`
- `notes_en`: `Volcengine Ark: new users get a 500k-token free quota via safe-experience mode (no payment method; service pauses before charges), and every model has its own free call quota.`

**Primary sources verified:**
- https://docs.volcengine.com/docs/82379/1298459 — base_url `https://ark.cn-beijing.volces.com/api/v3` + OpenAI-compat
- https://docs.volcengine.com/docs/82379/1399009 — OpenAI SDK (`from openai import OpenAI` with `ARK_API_KEY`) migration example
- https://docs.volcengine.com/docs/82379/1330626 — 安心体验模式 free quota
- https://www.volcengine.com/docs/82379/1544106 — live model-price page confirming `doubao-seed-2.0-lite`, `doubao-seed-1.6`, etc. as current models
- https://github.com/volcengine/OpenViking/blob/main/docs/zh/guides/02-volcengine-purchase-guide.md — confirms API ID format is dashed+date-suffixed: `doubao-seed-2-0-lite-260428` (published 2025-12-28)
- https://www.volcengine.com/product/ark — product page

**Gotchas:**
- base_url is `/api/v3` (**NOT `/v1`**) — it is the documented OpenAI-compatible equivalent; the OpenAI Python SDK works by setting `base_url` to this value.
- Free tier is a **one-time 500k-token signup quota** via 安心体验模式 for users who have NOT opened a paid model service; it is not a recurring daily quota by default (the product page also advertises a claimable daily up-to-5M-tokens-per-model promo, but its exact terms were not confirmable).
- Volcengine account registration requires real-name authentication (实名认证 = ID verification, **NOT a credit card**) — no payment method is needed to use the free quota.
- Model IDs use a dashed+date format and are **updated frequently** (e.g. `doubao-seed-2-0-lite-260428`, `doubao-seed-1-6-251015`) — verify current IDs in the Volcengine console model list (doc 1330310) before shipping. The two IDs above follow the documented format and reference models confirmed live on the price page (`doubao-seed-2.0-lite`, `doubao-seed-1.6`).
- The Tencent Hunyuan endpoint (`api.hunyuan.cloud.tencent.com/v1`) is fully decommissioned on 2026-09-30 and its free models went offline 2026-06-22; its migration target TokenHub requires enabling post-payment (a payment gate) — Hunyuan does **not** qualify and must not be added.

---

## Rejected providers

### 2. DeepSeek — ❌ REJECTED
- **Reason:** OpenAI-compatible (`https://api.deepseek.com`, `POST /chat/completions` works) and exact current model IDs (`deepseek-flash`, `deepseek-v4-pro`) are confirmed from official docs, but the **free tier is not clearly stated on any public primary source** — the pricing page is pay-as-you-go and free-quota specifics are deferred (ToS §6.2) to the login-gated `platform.deepseek.com`. The free-tier requirement is not met (fails "clearly stated").
- **Sources:** https://api-docs.deepseek.com/quick_start/pricing , https://api-docs.deepseek.com/api/get-user-balance , https://cdn.deepseek.com/policies/en-US/deepseek-open-platform-terms-of-service.html
- **Gotchas:** base_url `https://api.deepseek.com` has no `/v1` suffix but `POST /chat/completions` works (the `/v1` form is also historically accepted). If DeepSeek later publishes a no-card free quota on a public page, this becomes acceptable.

### 4. GitHub Models — ❌ REJECTED
- **Reason:** GitHub Models was **fully retired as of 2026-07-30** — the inference API, playground, model catalog, and BYOK are no longer available to any customer, so there is no working free OpenAI-compatible endpoint.
- **Sources:** https://docs.github.com/en/github-models , https://models.github.ai/inference
- **Gotchas:** GitHub directs users to Azure AI Foundry or GitHub Copilot instead; neither is a drop-in free keyless OpenAI-compatible endpoint under the `github.ai`/`models.inference.azure.com` domain.

### 7. Together AI — ❌ REJECTED
- **Reason:** OpenAI-compatible (`https://api.together.ai/v1`, raw HTTP `/v1/chat/completions` with Bearer key) but **NOT genuinely free**: official billing docs state "Together AI does not currently offer free trials" and platform access requires a minimum $5 credit purchase with a payment method (fully prepaid).
- **Sources:** https://docs.together.ai/docs/billing-credits , https://docs.together.ai/docs/inference/openai-compatibility , https://www.together.ai/pricing
- **Gotchas:** The catalog lists one Free-priced model (`Prism-ML/Ternary-Bonsai-27B`) but it still cannot be called without the $5 prepaid balance + payment method on file. The "$5 free credits" hint is contradicted by the provider's current official docs.

### 8. DeepInfra — ❌ REJECTED
- **Reason:** OpenAI-compatible (verified base_url `https://api.deepinfra.com/v1/openai`, raw HTTP + Bearer auth), but **REJECTED**: DeepInfra is a pure pay-as-you-go per-token provider with **no documented free tier, no free signup credits, no recurring free quota, and no $0/free models** in any official source.
- **Sources:** https://docs.deepinfra.com/chat/overview , https://deepinfra.com/pricing , https://deepinfra.com/models/text-generation
- **Gotchas:** The hinted base_url `/v1` is wrong (documented form is `https://api.deepinfra.com/v1/openai`); `meta-llama/Meta-Llama-3.1-8B-Instruct` is paid, not free — the actual id is `meta-llama/Meta-Llama-3.1-8B-Instruct-Turbo` ($0.02/1M in). Strong low-cost provider, but not a free-tier provider by this catalog's criteria.

### 10. Chutes AI — ❌ REJECTED
- **Reason:** Despite being OpenAI-compatible (verified base_url `https://llm.chutes.ai/v1`), Chutes' official pricing FAQ states **"We do not offer a free tier at this time"** (top-up/PAYG only, no free credits), and inference requires a Bearer key (anonymous requests 429) — fails the "genuinely free right now" requirement.
- **Sources:** https://chutes.ai/pricing , https://chutes.ai/llms.txt , https://chutes.ai/agents/connect
- **Gotchas:** The "Chutes is a free LLM router" reputation is outdated. For reference only: verified OpenAI base_url is `https://llm.chutes.ai/v1` (NOT `api.chutes.ai`, which is the management API); signup at `https://chutes.ai/auth/start` (no card required, but no free credits granted); every catalog model is paid per-token.

### 17. Tencent Hunyuan — ❌ REJECTED
- **Reason:** The Hunyuan platform `api.hunyuan.cloud.tencent.com` is **fully decommissioned on 2026-09-30** with free models offline since 2026-06-22, and its migration target TokenHub (`tokenhub.tencentmaas.com/v1`) requires enabling post-payment (a payment gate).
- **Sources:** https://cloud.tencent.com/document/product/1729/131925 , https://cloud.tencent.com/document/product/1729/111007
- **Gotchas:** Do not add Tencent Hunyuan. (Volcengine Ark above is the accepted Chinese-cloud provider.)

### 18. Lepton AI — ❌ REJECTED
- **Reason:** Lepton AI was acquired by NVIDIA; the old hosted OpenAI-compatible LLM API at `api.lepton.ai` is dead (resolves to a non-public IP). The current product, NVIDIA DGX Cloud Lepton, is an enterprise GPU deployment platform with only per-deployment `${your-endpoint-url}` placeholders (no stable public `/v1` base_url) where users deploy their own Hugging Face models — so there is no hosted base_url, no recurring free hosted-inference tier, and no provider-supplied free model id strings.
- **Sources:** https://docs.nvidia.com/dgx-cloud/lepton/get-started/ , https://docs.nvidia.com/dgx-cloud/lepton/get-started/endpoint/ , https://docs.nvidia.com/dgx-cloud/lepton/features/endpoints/create-llm/
- **Gotchas:** Third-party claims of "$10 free signup credits" refer to the pre-acquisition hosted API that no longer exists. Even if a deployment exposes an OpenAI-shaped surface, the base_url and model IDs are user-deployment-specific, so it cannot be added as a one-click provider.

### 19. AnyAPI — ❌ REJECTED
- **Reason:** AnyAPI is a real provider with an apparent free tier ("Start Free", a "Free" tier filter, and a reported recurring 100K anytokens/day quota), and it is OpenAI-compatible (`https://api.anyapi.ai/v1`, Bearer). However, the **exact currently-valid free model IDs could not be confirmed** from an accessible primary source: the live model catalog (`anyapi.ai/ai-models`) is behind a Cloudflare challenge and could not be enumerated, and the model IDs the research surfaced (`openai/gpt-4o`, `openai/gpt-4-turbo`) are not current in the live catalog (which now lists `openai/gpt-6-astra`, `z-ai/glm-5-3-flash`, `google/gemini-3-8-flash`, etc.). Fails "at least 2 exact free model IDs you can list" against a clean primary source.
- **Sources:** https://anyapi.ai/ai-models , https://docs.anyapi.ai/ , https://anyapi.ai/
- **Gotchas:** If the implementer can enumerate the current Free-tier model IDs (e.g. by logging into the dashboard), AnyAPI could become acceptable — the free daily-quota tier (recurring, no card statement) and OpenAI-compat base_url are otherwise plausible. Listed IDs (gpt-4o/gpt-4-turbo) are likely deprecated in the current 2026 catalog; do not ship them.

---

## Methodology notes

- Each provider's OpenAI compatibility was confirmed against the provider's **own** docs/code examples (curl or SDK `base_url`/`baseURL` + `Authorization: Bearer` + OpenAI-shaped body), not third-party blog roundups.
- Free-tier terms were confirmed against the provider's own pricing/billing/rate-limit docs. "No payment method required" was required for credit-based free tiers; identity verification (real-name auth, phone) is treated as distinct from a payment method and allowed.
- Model IDs were taken **verbatim** from the providers' live official pages (model catalogs, pricing tables, code examples, model-detail pages). Where a sub-agent's proposed ID was found to be outdated/not on the current page (e.g. Fireworks `llama-v3p1-8b-instruct`, AnyAPI `gpt-4o`), it was corrected or the provider rejected.
- Cross-corroboration: several "futuristic-looking" model IDs (e.g. `gpt-oss-120b`, `gemma-4-31b-it`, `nemotron-3-super-120b-a12b`, `kimi-k2.6`, `glm-5p2`) were independently confirmed to be real and current because they appear on **multiple** providers' live official pages (SambaNova, Novita, Fireworks, Requesty, HuggingFace, Cohere) — these are genuine current open-weight models, not inventions.
