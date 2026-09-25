# Free OpenAI-Compatible LLM Providers — Research for Sakura LLM Gateway Catalog

> **Update pass — verified against each provider's live official docs (API references, pricing pages, model catalogs, official SDK/curl examples) via direct page fetches. Research date: September 2026 (search/live index dated ~2026-09-25).** Every material claim cites an official-doc URL. The catalog under review is the snapshot at `llm-gateway/free-catalog.json` (`"updated": "2026-09-17"`).
>
> **Provenance note (important — read before acting on "futuristic" IDs).** The harness web tools return the providers' **real, current official pages** (dated ~Sep 2026). This was confirmed **first-hand** by the research agent across many providers: e.g. Google's models page lists `gemini-3.8-flash` as the *newest* Stable model; Cohere lists `command-a-plus-05-2026` as Live; Moonshot lists `kimi-k2.6`/`kimi-k2.7-code` current (and explicitly retired `kimi-k2.5`/`moonshot-v1-*` on 2026-08-31); SambaNova lists `DeepSeek-V3.1`/`gpt-oss-120b` as Production; Requesty's live free-models page enumerates `nemotron-3-ultra-550b-a55b`, `gemma-4-31b-it`, `leanstral-1-5`, `muse-glimmer-30b`; Fireworks' pricing table lists `glm-5p2`, `deepseek-v4-flash-0731`. **These "future-looking" IDs are genuine and current — they are not invented and do not need fixing where the catalog already lists them.** A prior subagent flagged a "the fetched pages are synthetic/future, don't trust the IDs" caveat; that caveat is **incorrect** and is rebutted by the first-hand confirmations above. The genuine staleness is concentrated on a small number of providers whose model lists rotate fast (siliconflow free-SKUs, zai GLM major version, openrouter `:free`, mistral, alibaba qwen, cloudflare qwen1.5, groq llama/qwen, volcengine date-suffixed doubao) — those are real FIXes, detailed below.
>
> The parent agent should still independently re-fetch the **high-risk** items flagged at the end (unusual base_urls, rotated model IDs, changed free tiers) before applying edits — but it should treat the confirmed-current IDs as real, not as artifacts.

---

## Section 1 — Existing 19 providers: re-verification

### Summary table

| # | id | Verdict | What changed (if anything) | Confirming official URL |
|---|----|---------|----------------------------|-------------------------|
| 1 | siliconflow | **FIX** | Refresh free model IDs — catalog's free SKUs (DeepSeek-V3/Qwen2.5-7B/glm-4-9b-chat) rotated forward; pick current 免费 models. base_url + free mechanism unchanged. | https://siliconflow.cn/pricing |
| 2 | zai | **FIX** | `glm-4-flash`/`glm-4-flashx` are deprecated (not in current pricing). Refresh to current FREE flash models — **confirmed free first-hand**: `glm-4.5-flash` (still free) + `glm-4.7-flash`. ⚠️ `glm-5.3-flash`/`glm-5.3-flashx` are PAID, not free. base_url unchanged. | https://docs.z.ai/guides/overview/quick-start , https://docs.z.ai/guides/overview/pricing |
| 3 | openrouter | **FIX** | All 6 catalog `:free` IDs rotated (grep of live `/api/v1/models` = 0 matches); re-enumerate current `:free` IDs. base_url + `:free` mechanism unchanged. | https://openrouter.ai/api/v1/models |
| 4 | pollinations | **KEEP** | keyless confirmed; `openai`=`openai-fast` confirmed. Minor: `openai-large` appears retired — drop it. | https://text.pollinations.ai/models |
| 5 | cerebras | **EXCLUDE** | Free tier now requires a verified payment method ($5 credit trial, no perpetually-free tier) — fails the no-card rule. | https://inference-docs.cerebras.ai/support/rate-limits |
| 6 | nvidia | **KEEP** | base_url + all 4 model IDs confirmed on build.nvidia.com; 1000-credit/no-card trial confirmed. | https://build.nvidia.com/models , https://forums.developer.nvidia.com/t/nim-api-credits/305703 |
| 7 | groq | **FIX** | 4 of 5 `free_models` are wrong (llama-3.3-70b-versatile & llama-3.1-8b-instant → Enterprise/paid; qwen3-32b & kimi-k2-instruct not on Groq). Replace with current free IDs. base_url unchanged. | https://console.groq.com/docs/models , https://console.groq.com/docs/rate-limits , https://console.groq.com/docs/deprecations |
| 8 | mistral | **FIX** | Model IDs rotated (Mistral Small 4 / Medium 3.5 era); refresh to current IDs **and confirm Free-mode terms/card policy**. base_url unchanged. | https://docs.mistral.ai/ , https://docs.mistral.ai/models |
| 9 | cloudflare | **KEEP** | 10k neurons/day no-card confirmed; `@cf/meta/llama-3.1-8b-instruct` confirmed. Minor: `@cf/qwen/qwen1.5-14b-chat-awq` retired → use `@cf/qwen/qwen3-30b-a3b-fp8`; verify `@cf/mistralai/` vs `@cf/mistral/` path. | https://developers.cloudflare.com/workers-ai/platform/pricing/ , https://developers.cloudflare.com/workers-ai/configuration/open-ai-compatibility/ |
| 10 | google-ai-studio | **KEEP** | All 4 gemini flash IDs confirmed current + free-tier "Free of charge". | https://ai.google.dev/gemini-api/docs/models , https://ai.google.dev/gemini-api/docs/pricing |
| 11 | sambanova | **KEEP** | base_url + all 3 Production models confirmed; Free Tier 20 RPM/20 RPD/200K TPD daily, no card. | https://docs.sambanova.ai/docs/en/get-started/api-keys-urls , https://docs.sambanova.ai/docs/en/models/sambacloud-models , https://docs.sambanova.ai/docs/en/models/rate-limits |
| 12 | huggingface-router | **KEEP** | base_url + both model IDs confirmed; recurring free tier exists (no card). | https://huggingface.co/docs/inference-providers/en/index |
| 13 | fireworks-ai | **KEEP** | base_url + all 3 model IDs confirmed; $1 free credit, no card to start. | https://docs.fireworks.ai/tools-sdks/openai-compatibility , https://docs.fireworks.ai/serverless/pricing , https://fireworks.ai/pricing |
| 14 | novita-ai | **KEEP** | base_url `https://api.novita.ai/openai` (no `/v1`) confirmed correct; both `ling-3.0-flash-*` confirmed $0/free. | https://docs.novita.ai/guides/llm-api , https://novita.ai/models/model-detail/inclusionai-ling-3.0-flash-fin |
| 15 | requesty | **KEEP** | base_url + all 6 free-model IDs confirmed on live free-models page; 200 req/day, no card. | https://www.requesty.ai/models/free |
| 16 | cohere | **KEEP** | base_url + all 4 command IDs confirmed Live; free trial key 1,000 calls/mo, no card. | https://docs.cohere.com/docs/models , https://docs.cohere.com/docs/compatibility-api , https://docs.cohere.com/docs/rate-limits |
| 17 | alibaba-dashscope | **FIX** | `qwen-plus`/`qwen-max` may be superseded by `qwen3.7-plus`/`qwen3.8-max` — **confirm whether the unversioned aliases still resolve** + re-confirm free-quota terms. base_url unchanged. | https://help.aliyun.com/zh/model-studio/models |
| 18 | volcengine-ark | **FIX** | `doubao-seed-2-1-pro-260628` is a legacy version (current `doubao-seed-2-1-pro-260915`); `doubao-seed-2-0-lite-260428` is legacy **and not actually free**. Replace with current free-quota IDs. base_url unchanged. | https://docs.volcengine.com/docs/ark/model-list , https://www.volcengine.com/product/ark , https://docs.volcengine.com/docs/ark/free-inference-quota |
| 19 | moonshot | **KEEP** | base_url + `kimi-k2.6`/`kimi-k2.7-code` confirmed current; 15 CNY voucher (real-name auth, no card); kimi-k3 excluded from voucher. | https://platform.kimi.com/docs/models , https://platform.kimi.com/docs/api/chat , https://platform.kimi.com/docs/guide/account-and-payments |

**Tally: 11 KEEP · 7 FIX · 1 EXCLUDE** (of the 19). The catalog is substantially accurate; the 7 FIXes are almost all **model-ID refreshes** on fast-rotating providers, plus one genuine policy-driven EXCLUDE (Cerebras).

---

### Detail — FIX providers

#### volcengine-ark — FIX
- **Verified base_url:** `https://ark.cn-beijing.volces.com/api/v3` (CONFIRMED — official "数据面 API" + curl `https://ark.cn-beijing.volces.com/api/v3/chat/completions`). Source: https://docs.volcengine.com/docs/ark/base-url-and-authentication
- **Problem:** Catalog `free_models` are stale/incorrect.
  - `doubao-seed-2-1-pro-260628` — valid API ID but now a **legacy version** (往期模型); current is `doubao-seed-2-1-pro-260915`. Base model "Doubao-Seed-2.1-pro" *does* have the 500K-token free quota.
  - `doubao-seed-2-0-lite-260428` — legacy version **and NOT in the free-quota list** (it's a paid model, 0.6 元/M input). Catalog incorrectly lists it as free.
- **Current free-quota text models (each 500K tokens, one-time):** Doubao-Seed-2.1-pro, Doubao-Seed-2.1-turbo, Doubao-Seed-Evolving, Doubao-Seed-Character.
- **Recommended change:** `free_models` → `["doubao-seed-2-1-pro-260915", "doubao-seed-2-1-turbo-260628"]`. Optionally include the rotation-resistant alias `doubao-seed-evolving` (weekly-iterated, always-current).
- **Free tier:** register Volcengine → 500K-token free quota per qualifying model (one-time, shared across base+fine-tuned versions); 安心体验模式 (safe-trial mode) consumes only free quota and stops before charges; requires real-name-authenticated account that has NOT opened a paid model service; **no payment method/card**. Source: https://docs.volcengine.com/docs/ark/free-inference-quota , https://docs.volcengine.com/docs/ark/free-tokens-only-mode
- **Risk:** HIGH rotation — date-suffixed doubao IDs rotate frequently (pro went 260628→260915; both 2.0-lite versions 260215/260428 are now legacy). Re-verify current IDs in the Volcengine console model list before shipping.

#### groq — FIX
- **Verified base_url:** `https://api.groq.com/openai/v1` (CONFIRMED — curl `https://api.groq.com/openai/v1/models`; SDK `baseURL: 'https://api.groq.com/openai/v1'`). Source: https://console.groq.com/docs/models
- **Problem:** 4 of 5 catalog `free_models` are wrong.
  - `llama-3.3-70b-versatile` — now **Enterprise/Contact Sales** (deprecated, shutdown 2026-08-16 per deprecations page); NOT in the free rate-limits table.
  - `llama-3.1-8b-instant` — now **Enterprise/Contact Sales** (deprecated, shutdown 2026-08-16); NOT free.
  - `qwen/qwen3-32b` — **does not exist** on Groq (deprecated, shutdown 2026-07-16); current Groq Qwen is `qwen/qwen3.8-27b` (preview).
  - `moonshotai/kimi-k2-instruct` — **not found** on the current Groq models page (a secondary Mar-2026 article still listed it, but the official Sep-2026 models page does not — re-verify).
  - `openai/gpt-oss-120b` — CONFIRMED free (30 RPM / 1K RPD / 8K TPM / 200K TPD, daily reset).
- **Current free-tier chat models (official):** `openai/gpt-oss-120b`, `openai/gpt-oss-20b`, `qwen/qwen3.8-27b` (preview), `openai/gpt-oss-safeguard-20b`.
- **Recommended change:** `free_models` → `["openai/gpt-oss-120b", "openai/gpt-oss-20b", "qwen/qwen3.8-27b", "openai/gpt-oss-safeguard-20b"]`.
- **Free tier:** Groq has a **Free tier (no card)** distinct from the paid Developer tier (card required). Free-tier per-model limits (chat): 30 RPM, 1K RPD, 8K TPM, 200K TPD; RPD/TPD reset daily. Source: https://console.groq.com/docs/rate-limits , https://console.groq.com/docs/billing-faqs ("To upgrade from the Free tier to the Developer tier, you'll need to provide a valid payment method")
- **Risk:** The rate-limits doc states **8K TPM** for the free tier, while the models page shows "250K TPM/1K RPM" labeled "Developer plan" for `gpt-oss-120b` — the 250K figure is the **paid** Developer tier, not free. Re-verify the free-tier TPM on the console /settings/limits page. Preview models (`qwen3.8-27b`, `gpt-oss-safeguard-20b`) may be discontinued at short notice.

#### zai — FIX
- **Verified base_url:** `https://api.z.ai/api/paas/v4` (CONFIRMED — the international z.ai OpenAI-compat endpoint; CN equivalent `https://open.bigmodel.cn/api/paas/v4`). Source: https://docs.z.ai/guides/overview/quick-start
- **Problem:** Catalog `glm-4-flash`/`glm-4-flashx` are no longer in the current z.ai pricing list (deprecated/renamed). z.ai docs carry a "Migrate to GLM-5.3" guide, and the *latest* flash models are `glm-5.3-flash`/`glm-5.3-flashx` — **but those are PAID** ($0.15/$0.50 and $0.37/$1.25 per 1M), NOT free. The current FREE flash models are `glm-4.5-flash` and `glm-4.7-flash` (both marked Free in/out/cached on the pricing page).
- **Confirmed free (first-hand, z.ai pricing page):** `glm-4.5-flash` (Free), `glm-4.7-flash` (Free). Also `glm-4.6v-flash` (vision, Free).
- **Recommended change:** `free_models` → `["glm-4.5-flash", "glm-4.7-flash"]`. (Keep the still-free `glm-4.5-flash`; replace the deprecated `glm-4-flash`/`glm-4-flashx` with `glm-4.7-flash`. Do **NOT** use `glm-5.3-flash` — it is paid.)
- **Risk:** HIGH — GLM line migrates fast and the "latest" flash is paid while an older flash stays free; re-verify the free-flash row on the live z.ai pricing page before shipping.

#### openrouter — FIX
- **Verified base_url:** `https://openrouter.ai/api/v1` (CONFIRMED — `GET /api/v1/models` returns valid JSON). Source: https://openrouter.ai/api/v1/models
- **Problem:** All 6 catalog `:free` IDs have rotated (grep of live `/api/v1/models` for the 6 catalog IDs = 0 matches). The `:free` suffix scheme persists.
- **Current free IDs seen in the live list (examples, not exhaustive):** `inclusionai/ling-3.0-flash-fin:free`, `nex-agi/nex-n2.5-mini:free`, `nex-agi/nex-n2.5-pro:free`, plus zero-priced `stealth/space-bunny-alpha`.
- **Recommended change:** Re-enumerate `:free` IDs from live `https://openrouter.ai/api/v1/models` (or `https://openrouter.ai/models?max_price=0`) at catalog-update time and replace all 6.
- **Free tier:** `:free` models are $0 prompt+completion; needs an OpenRouter account + API key, **no card** for free models. Source: https://openrouter.ai/docs/quickstart
- **Risk:** Highest rotation risk of any provider — re-enumerate at deploy time.

#### mistral — FIX
- **Verified base_url:** `https://api.mistral.ai/v1` (matches known-real; OpenAI-compat). Source: https://docs.mistral.ai/
- **Problem:** Catalog `mistral-small-latest`/`open-mistral-nemo`/`codestral-latest` are not the current "Latest models" (docs now show Mistral Small 4 v26.03, Mistral Medium 3.5 v26.04, etc.). The `-latest` aliases may still resolve.
- **Recommended change:** Refresh `free_models` to current IDs **after confirming** (a) which current models are on the free Experiment/Studio tier and (b) that the free tier still needs no card (historically true; unconfirmed in the 2026 fetch).
- **Free tier:** "Activate Studio in Free mode and generate an API key" — a Free mode exists. Source: https://docs.mistral.ai/ , https://docs.mistral.ai/models
- **Risk:** Re-confirm free-mode terms + card policy on the live pricing page.

#### alibaba-dashscope — FIX
- **Verified base_url:** `https://dashscope.aliyuncs.com/compatible-mode/v1` (matches known-real DashScope OpenAI-compat endpoint). Source: https://help.aliyun.com/zh/model-studio/compatibility-of-openai-with-dashscope
- **Problem:** Current text-gen models lead with `qwen3.8-max`/`qwen3.7-plus`/`qwen3.8-flash` (plus `deepseek-v4-pro-0813`, `kimi-k3`, `glm-5.2`, `ZHIPU/GLM-5.3`). The catalog's `qwen-plus`/`qwen-max` **may still resolve as aliases** — this was not confirmed either way.
- **Recommended change:** Confirm whether `qwen-plus`/`qwen-max` still resolve; if not, refresh to `qwen3.7-plus`/`qwen3.8-max`. Re-confirm the 90-day free-quota-on-activation terms (no card) still hold.
- **Free tier (catalog claim, to re-confirm):** ~1M tokens/model free quota auto-granted on Model Studio activation, 90 days, no payment method; Beijing region only. Source: https://help.aliyun.com/zh/model-studio/new-free-quota
- **Risk:** Bailian naming migrates with Qwen releases; confirm alias resolution + free-quota terms.

#### siliconflow — FIX
- **Verified base_url:** `https://api.siliconflow.cn/v1` (matches known-real). Source: https://siliconflow.cn/pricing , https://api-docs.siliconflow.cn
- **Problem:** The catalog's 5 free SKUs (`deepseek-ai/DeepSeek-R1-Distill-Qwen-7B`, `deepseek-ai/DeepSeek-V3`, `Qwen/Qwen2.5-7B-Instruct`, `Qwen/Qwen2.5-Coder-7B-Instruct`, `THUDM/glm-4-9b-chat`) are no longer in the current 免费 ($0) list — the current 免费 rows include `tencent/Hunyuan-MT-7B`, `XingChenAGI/Xing4.0-29B`, `PaddlePaddle/PaddleOCR-VL-1.5` (+ more behind "展开更多").
- **Recommended change:** Refresh `free_models` to current 免费 ($0) **chat** SKUs from https://siliconflow.cn/pricing. ⚠️ Some 免费 rows seen in the live pricing are **not chat models** (e.g. `PaddlePaddle/PaddleOCR-VL-1.5` = OCR, `tencent/Hunyuan-MT-7B` = translation) — the parent must pick chat-capable 免费 models (DeepSeek/Qwen/GLM chat variants) when re-enumerating.
- **Free tier (unchanged mechanism):** perpetually-free ($0) small models + "$1 in free credits" on signup, **no card** to use 免费 models (phone-auth account). Source: https://siliconflow.cn/pricing
- **Risk:** Free SKUs rotate often; re-check the 免费 list before shipping.

---

### Detail — EXCLUDE provider

#### cerebras — EXCLUDE
- **Verified base_url:** `https://api.cerebras.ai/v1` (matches known-real OpenAI-compat endpoint).
- **Reason:** Cerebras's free tier is now a **credit-card-required trial**, not a no-card free tier. The official Rate Limits FAQ states verbatim:
  > *"New accounts receive **$5 in free credits after adding a verified payment method**. These credits expire 30 days… If you skip adding a payment method at sign-up, Playground and API access remain inactive until you do."* — *"Is there a permanently free tier? No. The Free Trial is time- and credit-bounded: $5 in credits that expire 30 days… Cerebras doesn't currently offer a no-cost tier that renews automatically or a per-model always-free allowance."*
- This **fails the catalog's hard rule** ("a free trial that requires a credit card is NOT a free provider"). The prior catalog/research (no-card ~1M tokens/day) reflected an older Cerebras policy that has since changed.
- **Current Free-Trial models** (only 2, both subject to the card-required trial): `gpt-oss-120b`, `qwen-3.8-27b`. The catalog's `llama3.1-8b`/`llama-3.3-70b`/`qwen-3-32b` are no longer in the public catalog.
- **Sources:** https://inference-docs.cerebras.ai/support/rate-limits (FAQ + Free Trial tier table), https://inference-docs.cerebras.ai/models/choose-a-model
- **Action:** Remove `cerebras` from the catalog. (If the implementer independently finds Cerebras reintroduced a no-card free tier, re-evaluate — but as of the live Sep-2026 docs, it is card-required.)

---

### KEEP providers — compact confirmation (with sources)

- **google-ai-studio** — base_url `…/v1beta/openai/` + all 4 flash IDs (`gemini-3.8/3.7/3.6/3.5-flash`) confirmed current + "Free of charge" tier, no card. https://ai.google.dev/gemini-api/docs/openai , /models , /pricing
- **nvidia** — base_url + all 4 IDs (`deepseek-ai/deepseek-r1`, `qwen/qwen3-coder-480b-a35b-instruct`, `meta/llama-3.3-70b-instruct`, `mistralai/mixtral-8x22b-instruct-v0.1`) confirmed on build.nvidia.com; 1000-credit/no-card trial (5000 total cap, one-time). https://build.nvidia.com/models , https://forums.developer.nvidia.com/t/nim-api-credits/305703
- **sambanova** — base_url + all 3 Production models (`Meta-Llama-3.3-70B-Instruct`, `DeepSeek-V3.1`, `gpt-oss-120b`) confirmed; Free Tier 20 RPM/20 RPD/200K TPD daily, no card. https://docs.sambanova.ai/docs/en/get-started/api-keys-urls , /models/sambacloud-models , /models/rate-limits
- **requesty** — base_url + all 6 free IDs (`nvidia/nemotron-3-super-120b-a12b`, `google/gemma-4-31b-it`, `meta/muse-glimmer-30b`, `mistral/leanstral-1-5`, `inclusionai/ling-3.0-tiny`, `nvidia/nemotron-3-ultra-550b-a55b`) confirmed on live free page; 200 req/day, no card, no trial expiry. https://www.requesty.ai/models/free
- **fireworks-ai** — base_url + all 3 IDs (`accounts/fireworks/models/glm-5p2`, `…/gpt-oss-120b`, `…/deepseek-v4-flash-0731`) confirmed; $1 free credit, no card to start. https://docs.fireworks.ai/serverless/pricing , https://fireworks.ai/pricing
- **novita-ai** — base_url `https://api.novita.ai/openai` (NO `/v1`) confirmed correct across 3 official pages; both `inclusionai/ling-3.0-flash-fin` & `…/ling-3.0-flash-sante` confirmed $0/token. (The "$1 signup credit, no card" figure could not be confirmed from accessible docs without login, but the two $0 models independently satisfy the free criterion.) https://docs.novita.ai/guides/llm-api , https://novita.ai/models/model-detail/inclusionai-ling-3.0-flash-fin
- **cohere** — base_url + all 4 command IDs (`command-a-plus-05-2026`, `command-a-03-2025`, `command-r-08-2024`, `command-r-plus-08-2024`) confirmed Live; free trial key 1,000 calls/mo + 20 req/min, no card. https://docs.cohere.com/docs/models , /compatibility-api , /rate-limits
- **moonshot** — base_url + `kimi-k2.6`/`kimi-k2.7-code` confirmed current (kimi-k2.5/moonshot-v1 retired 2026-08-31); 15 CNY voucher after real-name auth (no card), NOT usable on flagship kimi-k3. https://platform.kimi.com/docs/models , /api/chat , /guide/account-and-payments
- **huggingface-router** — base_url + `openai/gpt-oss-120b` & `deepseek-ai/DeepSeek-R1` confirmed; recurring free tier, no card. https://huggingface.co/docs/inference-providers/en/index
- **pollinations** — keyless confirmed; `openai`/`openai-fast` confirmed (drop retired `openai-large`). https://text.pollinations.ai/models
- **cloudflare** — base_url + 10k neurons/day (no card) confirmed; `@cf/meta/llama-3.1-8b-instruct` confirmed. (Minor refresh: `@cf/qwen/qwen1.5-14b-chat-awq` is retired → use `@cf/qwen/qwen3-30b-a3b-fp8`; verify `@cf/mistralai/` vs `@cf/mistral/` namespace.) https://developers.cloudflare.com/workers-ai/platform/pricing/ , /configuration/open-ai-compatibility/

---

## Section 2 — New candidates (ADD / EXCLUDE)

**31 candidates investigated** (17 international + 13 China/Asia + Hyperbolic re-investigation). **Result: 8 ADD · 23 EXCLUDE.** Hyperbolic (re-investigated per task) = EXCLUDE (pivoted to GPU rental; model catalog login-gated; no verifiable no-card free tier). All 8 ADDs require an API key (keyless=false); none are keyless.

### ADD candidates — summary table

| Candidate | base_url | Free-tier summary | Confirming URL |
|-----------|----------|-------------------|----------------|
| AI21 | `https://api.ai21.com/studio/v1` | $10 credit, 3 months, no card to start | https://docs.ai21.com/reference/jamba-1-6-api-ref.md , https://docs.ai21.com/docs/usage-cost.md |
| Kluster AI ⚠️ provisional | `https://api.kluster.ai/v1` | $5 credits on email verify, no card | https://x.com/klusterai/status/1884700560009683033 (+ secondary blog; official docs DNS-unreachable from research env) |
| Baidu ERNIE / Qianfan | `https://qianfan.baidubce.com/v2` | ¥20 voucher on real-name auth, no card, 1 month | https://cloud.baidu.com/doc/qianfan/s/rmh4stn9m , https://cloud.baidu.com/doc/qianfan/s/wmh4sv6ya |
| Stepfun (阶跃星辰) | `https://api.stepfun.com/v1` | Gifted "赠送账户" credits, real-name auth, no card (expires) | https://platform.stepfun.com/docs/zh/guides/developer/openai , https://platform.stepfun.com/docs/zh/guides/pricing/details |
| iFlyTek Spark (讯飞星火) | `https://spark-api-open.xf-yun.com/v1` | Lite model perpetually free + claimable free quota, real-name, no card | https://www.xfyun.cn/doc/spark/HTTP%E8%B0%83%E7%94%A8%E6%96%87%E6%A1%A3.html , https://xinghuo.xfyun.cn/sparkapi |
| Tencent Hunyuan ⚠️ reverses prior rejection | `https://api.hunyuan.cloud.tencent.com/v1` | 1M free tokens on first activation, real-name, no card, 1 year | https://cloud.tencent.com/document/product/1729/97731 , https://cloud.tencent.com/document/product/1729/111007 |
| ModelScope (魔搭社区) | `https://api-inference.modelscope.cn/v1` | "Free of charge" API-Inference + 200 Magicubes on signup, no card | https://modelscope.ai/docs/model-service/API-Inference/limits (+ GitHub repo #1615) |
| Infermatic | `https://api.totalgpt.ai/v1` | Free plan + dedicated Free-models tier | https://infermatic.ai/docs/overview/ , https://infermatic.ai/pricing/ |

### ADD candidates — full proposed catalog entries

#### ai21 — ADD
- **Entry:** `id="ai21"`, `name="AI21"`, `base_url="https://api.ai21.com/studio/v1"`, `protocol="openai"`, `keyless=false`, `signup_url="https://studio.ai21.com/"`, `guide_url="doc/free-providers/ai21.md"`
- `notes_zh`: 新账号赠送 $10 免费额度（3 个月有效），无需信用卡即可开始；额度用完或到期后才需绑定支付方式。OpenAI 兼容 /chat/completions，提供 Jamba Large/Mini（Mamba-Transformer 架构，256K 上下文）。
- `notes_en`: $10 free credit for 3 months on new accounts (no card to start; billing info required only after trial expires/exceeds). OpenAI-compatible /chat/completions; Jamba Large (v1.7) + Jamba Mini (v2), 256K context, Mamba-Transformer hybrid.
- `free_models`: `["jamba-large", "jamba-mini"]`
- **Free tier:** "$10 credit good for three months"; billing info required only AFTER trial expires/exceeds → no card to start. Verified from official curl `POST https://api.ai21.com/studio/v1/chat/completions` (Bearer + OpenAI body).
- **Sources:** https://docs.ai21.com/reference/jamba-1-6-api-ref.md , https://docs.ai21.com/docs/usage-cost.md , https://docs.ai21.com/docs/jamba-foundation-models.md
- **Risk:** Confirm the exact API model ID strings (`jamba-large`/`jamba-mini` vs `-latest`/versioned aliases) against the live API ref before shipping.

#### klusterai — ADD (PROVISIONAL)
- **Entry:** `id="klusterai"`, `name="Kluster AI"`, `base_url="https://api.kluster.ai/v1"`, `protocol="openai"`, `keyless=false`, `signup_url="https://kluster.ai/"`, `guide_url="doc/free-providers/klusterai.md"`
- `notes_zh`: 注册并验证邮箱即送 $5 免费额度，OpenAI 兼容，支持 Llama/DeepSeek；自适应推理（实时/异步/批处理）。⚠️ 官方文档在本次研究环境无法访问（kluster.ai 全域 DNS 无法解析），base_url 与模型 ID 经官方 X 账号 + 二手博客核实，正式收录前请再次核对官方文档。
- `notes_en`: $5 free credits on signup + email verify (no card); OpenAI-compatible; Llama/DeepSeek models; adaptive inference (real-time/async/batch). ⚠️ Official kluster.ai docs were UNREACHABLE from this research environment (entire kluster.ai domain failed DNS resolution — likely an env/network issue); base_url + model IDs corroborated via Kluster's official X account + a secondary blog — re-verify against live official docs before shipping.
- `free_models`: `["klusterai/Meta-Llama-3.1-8B-Instruct-Turbo", "klusterai/Meta-Llama-3.3-70B-Instruct-Turbo", "klusterai/Meta-Llama-3.1-405B-Instruct-Turbo", "deepseek-ai/DeepSeek-R1"]`
- **Free tier:** $5 free credits on signup + email verification (no card mentioned). Source: Kluster official X (@klusterai) + secondary blog with working OpenAI SDK code.
- **Sources:** https://x.com/klusterai/status/1884700560009683033 , https://walterpinem.com/getting-started-with-kluster-ai/
- **Risk:** HIGH — **provisional only**. Official kluster.ai docs were DNS-unreachable from the research environment (apex/www/api all ENOTFOUND). base_url + model IDs are secondary-source-confirmed (official X + blog), NOT from a directly-fetched official doc/curl. **Re-verify against live kluster.ai docs before shipping.**

#### baidu-qianfan — ADD
- **Entry:** `id="baidu-qianfan"`, `name="Baidu ERNIE / Qianfan (百度千帆)"`, `base_url="https://qianfan.baidubce.com/v2"`, `protocol="openai"`, `keyless=false`, `signup_url="https://cloud.baidu.com/product-s/qianfan_home"`, `guide_url="doc/free-providers/baidu-qianfan.md"`
- `notes_zh`: 百度千帆 ModelBuilder，OpenAI 兼容（base_url https://qianfan.baidubce.com/v2，鉴权 Bearer bce-v3/… 千帆 API Key）。新用户实名认证后赠 20 元代金券，全平台无门槛、有效期 1 个月（无需绑卡）。模型：ernie-4.5-turbo-32k / ernie-5.0 / deepseek-v4.1-flash / glm-5.3 等（用代金券免费调用）。代金券仅 1 个月，到期按量付费。
- `notes_en`: Baidu Qianfan ModelBuilder, OpenAI-compatible (base_url https://qianfan.baidubce.com/v2; auth Bearer bce-v3/… Qianfan API Key). New users get a ¥20 voucher after real-name auth, platform-wide, no threshold, valid 1 month (no card). Models: ernie-4.5-turbo-32k / ernie-5.0 / deepseek-v4.1-flash / glm-5.3 (free via voucher). Voucher expires in 1 month, then pay-as-you-go.
- `free_models`: `["ernie-4.5-turbo-32k", "ernie-4.5-turbo-128k-preview", "ernie-5.0", "deepseek-v4.1-flash", "deepseek-v3.2", "glm-5.3"]`
- **Free tier:** ¥20 voucher on real-name auth (实名认证, identity verification — not a card), platform-wide, no threshold, valid 1 month. Verified from official quickstart curl `https://qianfan.baidubce.com/v2/chat/completions` + OpenAI SDK `base_url="https://qianfan.baidubce.com/v2"`.
- **Sources:** https://cloud.baidu.com/doc/qianfan/s/rmh4stn9m , https://cloud.baidu.com/doc/qianfan/s/1mh4su5jg , https://cloud.baidu.com/doc/qianfan/s/wmh4sv6ya
- **Risk:** Voucher is one-time / 1-month; model lineup churns (ernie-5.x, deepseek-v4.x). Freeze `free_models` via `/v2/models` at catalog-build time.

#### stepfun — ADD
- **Entry:** `id="stepfun"`, `name="Stepfun (阶跃星辰)"`, `base_url="https://api.stepfun.com/v1"`, `protocol="openai"`, `keyless=false`, `signup_url="https://platform.stepfun.com/"`, `guide_url="doc/free-providers/stepfun.md"`
- `notes_zh`: 阶跃星辰开放平台，OpenAI 兼容。新用户赠送额度（赠送账户，实名认证无需绑卡，有有效期）；文本模型（step-5-preview/step-3.7-flash/step-3.5-flash）按量计费可用赠送额度抵扣；另有 4 个限时免费语音模型。速率分档按累计充值金额，仅赠送额度时为 V0（5 并发 / 100 RPM / 500K TPM）。
- `notes_en`: Stepfun open platform, OpenAI-compatible. New users get a gifted-credit account (real-name auth, no card, expires); text models (step-5-preview/step-3.7-flash/step-3.5-flash) are metered but callable free via gifted credits; 4 audio models are limited-time free. Rate tiers scale with cumulative recharge; free-only users sit at V0 (5 concurrency / 100 RPM / 500K TPM).
- `free_models`: `["step-5-preview", "step-3.7-flash", "step-3.5-flash"]`
- **Free tier:** Gifted "赠送账户" credits (spent before recharge account, with expiry); personal accounts require real-name auth (no card). Free-only users sit at rate tier V0 (5 concurrency / 100 RPM / 500K TPM). Verified from official OpenAI-migration guide + quickstart curl `https://api.stepfun.com/v1/chat/completions`.
- **Sources:** https://platform.stepfun.com/docs/zh/guides/developer/openai , https://platform.stepfun.com/docs/zh/quickstart/overview , https://platform.stepfun.com/docs/zh/guides/pricing/details
- **Risk:** Gifted amount not numerically specified in docs; credits expire; audio 限时免费 models may end. Re-verify gifted-credit amount + expiry.

#### iflytek-spark — ADD
- **Entry:** `id="iflytek-spark"`, `name="iFlyTek Spark (讯飞星火)"`, `base_url="https://spark-api-open.xf-yun.com/v1"`, `protocol="openai"`, `keyless=false`, `signup_url="https://xinghuo.xfyun.cn/sparkapi"`, `guide_url="doc/free-providers/iflytek-spark.md"`
- `notes_zh`: 科大讯飞星火认知大模型，OpenAI 兼容（base_url https://spark-api-open.xf-yun.com/v1）。鉴权用控制台 APIPassword 作 Bearer token。Lite 版支持免费使用；可在产品页面领取免费额度（实名认证无需绑卡）。模型 ID：lite（免费）、generalv3.5（Max）、generalv3（Pro）、4.0Ultra、max-32k、pro-128k。注意 Max 套餐 2026-03-10 下线升级为 Ultra。
- `notes_en`: iFlyTek Spark, OpenAI-compatible. Auth via console APIPassword as Bearer token. Lite model is perpetually free; free quota claimable on the product page (real-name auth, no card). Model IDs: lite (free), generalv3.5 (Max), generalv3 (Pro), 4.0Ultra, max-32k, pro-128k. Note Max subscription package offline 2026-03-10, upgraded to Ultra.
- `free_models`: `["lite", "generalv3", "generalv3.5", "4.0Ultra"]`
- **Free tier:** Lite model perpetually free ("Lite…支持免费使用"); claimable free quota on the product page; iFlyTek open platform uses real-name auth (no card). Verified from official HTTP doc + curl `https://spark-api-open.xf-yun.com/v1/chat/completions` + OpenAI SDK `base_url`.
- **Sources:** https://www.xfyun.cn/doc/spark/HTTP%E8%B0%83%E7%94%A8%E6%96%87%E6%A1%A3.html , https://xinghuo.xfyun.cn/sparkapi
- **Risk:** Model lineup churns (Max→Ultra 2026-03-10). Confirm Lite is still free + free-quota claimable at runtime.

#### tencent-hunyuan — ADD (⚠️ REVERSES prior research's "decommissioned" rejection)
- **Entry:** `id="tencent-hunyuan"`, `name="Tencent Hunyuan (腾讯混元)"`, `base_url="https://api.hunyuan.cloud.tencent.com/v1"`, `protocol="openai"`, `keyless=false`, `signup_url="https://cloud.tencent.com/document/product/1729"`, `guide_url="doc/free-providers/tencent-hunyuan.md"`
- `notes_zh`: 腾讯混元大模型，OpenAI 兼容（base_url https://api.hunyuan.cloud.tencent.com/v1）。首次开通（腾讯云个人/企业实名认证，无需绑卡）即赠免费资源包：生文模型共享 100 万 tokens、Hunyuan-embedding 100 万 tokens，有效期 1 年。模型：hunyuan-a13b / hunyuan-role-latest / hunyuan-translation / hunyuan-translation-lite / hunyuan-turbos-vision / hunyuan-t1-vision / hunyuan-embedding。注意：官方公告混元功能正逐步迁移至 TokenHub，原平台停止新购（但免费体验额度仍按首次开通发放，计费页更新于 2026-06-26）。建议运行时确认免费额度仍可领取。
- `notes_en`: Tencent Hunyuan, OpenAI-compatible. First activation (Tencent Cloud personal/enterprise real-name auth, no card) grants a free resource package: shared 1M tokens for text models + 1M for Hunyuan-embedding, valid 1 year. Models: hunyuan-a13b / hunyuan-role-latest / hunyuan-translation / hunyuan-translation-lite / hunyuan-turbos-vision / hunyuan-t1-vision / hunyuan-embedding. CAVEAT: official notice says Hunyuan features are gradually migrating to TokenHub; the original platform stops NEW paid purchases but the free trial quota is still documented as granted on first activation (billing page updated 2026-06-26). Re-verify free-quota availability at runtime.
- `free_models`: `["hunyuan-a13b", "hunyuan-role-latest", "hunyuan-translation", "hunyuan-translation-lite", "hunyuan-turbos-vision", "hunyuan-t1-vision", "hunyuan-embedding"]`
- **Free tier:** Official billing page (updated 2026-06-26): first activation grants a one-time free resource package — text models share 1,000,000 tokens + Hunyuan-embedding 1M tokens, 1-year validity; requires Tencent Cloud personal/enterprise real-name auth (no card); free package spent before paid. Verified OpenAI-compat: official doc + curl `https://api.hunyuan.cloud.tencent.com/v1/chat/completions`.
- **Sources:** https://cloud.tencent.com/document/product/1729/97731 (free-quota table) , https://cloud.tencent.com/document/product/1729/111007 (OpenAI-compat base_url + curl)
- **Risk:** HIGH — (a) **reverses the prior research's "decommissioned 2026-09-30" rejection**; a TokenHub migration notice is present but the free-quota-on-first-activation is still documented → parent must carefully re-verify the free tier is still claimable. (b) **base_url RESOLVED** — canonical is `https://api.hunyuan.cloud.tencent.com/v1` per the authoritative OpenAI-compat doc (1729/111007, updated 2026-04-27): verbatim *"base_url：https://api.hunyuan.cloud.tencent.com/v1"* + curl + OpenAI/Node/Go SDK all identical. The `https://hunyuan.cloud.tencent.com/openai/v1` form (seen only in the TRTC integration doc 647/79679, model `hunyuan-2.0-thinking-20251109`) is a non-canonical/older artifact and does NOT appear in the official OpenAI-compat doc; the Anthropic-compat sibling is `api.hunyuan.cloud.tencent.com/anthropic`, confirming the host pattern `api.hunyuan.cloud.tencent.com/{v1|anthropic}`. Use `https://api.hunyuan.cloud.tencent.com/v1` (gateway `+ /chat/completions` hits the official full-path endpoint). (c) Model lineup churns during migration.

#### modelscope — ADD
- **Entry:** `id="modelscope"`, `name="ModelScope (魔搭社区)"`, `base_url="https://api-inference.modelscope.cn/v1"`, `protocol="openai"`, `keyless=false`, `signup_url="https://www.modelscope.cn/"`, `guide_url="doc/free-providers/modelscope.md"`
- `notes_zh`: 阿里达摩院开源模型即服务（MaaS）平台，API-Inference 免费提供 1000+ 开源模型推理（注册送 200 Magicubes，每日续赠）；OpenAI 兼容。与已收录的 DashScope/Bailian 是不同服务（免费开源模型 vs 官方付费 API），非重复。模型 ID 为 org/model 格式，建议运行时拉取 /v1/models 列表。
- `notes_en`: Alibaba DAMO open-source MaaS hub; API-Inference serves 1000+ open-source models free (200 Magicubes on signup + daily top-up); OpenAI-compatible. Distinct from the cataloged DashScope/Bailian (free open-source inference vs official paid API) — not a duplicate. Model IDs use org/model form; enumerate via /v1/models at runtime.
- `free_models`: `["deepseek-ai/DeepSeek-R1", "Qwen/Qwen2.5-72B-Instruct", "deepseek-ai/DeepSeek-V3"]`
- **Free tier:** Official: API-Inference is "free of charge for developers to experience"; signup grants 200 Magicubes (+50/day when an Alibaba Cloud account is linked); no card. Free tier covers ALL hosted open-source models (1000+). Verified OpenAI-compatible via VoltAgent integration doc ("OpenAI-compatible adapter, default base URL https://api-inference.modelscope.cn/v1") + official ModelScope GitHub repo issue #1615 (real call).
- **Sources:** https://modelscope.ai/docs/model-service/API-Inference/limits , https://github.com/modelscope/modelscope/issues/1615 , https://voltagent.dev/models-docs/providers/modelscope/
- **Risk:** The official quickstart page is JS-rendered (returned only footer/popup to the fetcher); base_url is corroborated by the official GitHub repo + VoltAgent integration doc, **not a directly-fetched official curl** — re-verify the exact base_url from official docs. Model IDs rotate (Qwen/DeepSeek versions); enumerate `/v1/models` at runtime.

#### infermatic — ADD
- **Entry:** `id="infermatic"`, `name="Infermatic"`, `base_url="https://api.totalgpt.ai/v1"`, `protocol="openai"`, `keyless=false`, `signup_url="https://infermatic.ai/"`, `guide_url="doc/free-providers/infermatic.md"`
- `notes_zh`: Infermatic（infermatic.ai，API 域名 api.totalgpt.ai），OpenAI 兼容（vLLM 后端，/v1/chat/completions）。有免费计划（Start with a free plan）及专属 Free 模型档（如 TheDrummer-Rocinante-12B-v1.1），另有 $9/$20 付费档。鉴权 Bearer API Key。注意：任务给定的 infermatic.com SSL 报错，真实域名为 infermatic.ai。建议运行时确认免费档无需绑卡。
- `notes_en`: Infermatic (infermatic.ai; API host api.totalgpt.ai), OpenAI-compatible (vLLM backend, /v1/chat/completions). Has a free plan with a dedicated Free-models tier (e.g., TheDrummer-Rocinante-12B-v1.1) plus paid $9/$20 tiers. Auth Bearer API Key. NOTE: the task's infermatic.com SSL-errored; the real domain is infermatic.ai. Confirm at signup that the free plan requires no card.
- `free_models`: `["TheDrummer-Rocinante-12B-v1.1", "Sao10K-L3.3-70B-Euryale-v2.3-FP8-Dynamic", "Sao10K-72B-Qwen2.5-Kunou-v1-FP8-Dynamic", "TheDrummer-Anubis-70B-v1-FP8-Dynamic"]`
- **Free tier:** "Start with a free plan" (homepage) + a dedicated "Free models" tier; flat-rate gateway otherwise ($9/$20 paid tiers). Verified OpenAI-compatible from official API docs (`GET /v1/models`, `POST /v1/chat/completions`, `/v1/completions`, `/v1/embeddings`; Bearer; vLLM backend).
- **Sources:** https://infermatic.ai/docs/overview/ , https://infermatic.ai/ , https://infermatic.ai/pricing/
- **Risk:** Confirm at signup that the free plan needs no card. Note the real domain is `infermatic.ai` (the task's `infermatic.com` was the wrong TLD / SSL error).

### EXCLUDE candidates

| Candidate | Reason | Confirming URL |
|-----------|--------|----------------|
| **Hyperbolic** (re-investigated per task) | Pivoted to GPU rental; official docs (`www.hyperbolic.ai/docs`) contain NO inference/chat API reference (only GPU-rental APIs); `app.hyperbolic.ai/models` login-gated; old `api.hyperbolic.xyz/v1` only in third-party profiles (likely deprecated); no verifiable no-card free tier. Fails confirm-or-exclude on all 3 criteria. | https://www.hyperbolic.ai/docs/overview/overview , https://www.hyperbolic.ai/docs/general/billing-payments |
| Together AI | PAYG per-token, no no-card free tier; only 1 $0 model (Ternary Bonsai 27B) — fails ≥2. | https://www.together.ai/pricing |
| DeepInfra | Card or pre-pay mandatory ("you won't be able to use our services"); all models >$0. | https://deepinfra.com/pricing |
| Chutes AI | No free tier — authoritative `/llms.txt` confirms lowest plan $3/mo or PAYG balance top-up; anonymous (no Bearer) → 429; all models >$0. (Prior "no free tier" rejection CONFIRMED current, not outdated.) | https://chutes.ai/llms.txt , https://chutes.ai/pricing |
| AI/ML API (aimlapi) | Free Tier officially PAUSED; "-free"-suffixed models actually bill (official example shows `usd_spent: 0.06`); no no-card free access. | https://docs.aimlapi.com/faq/free-tier.md , https://aimlapi.com/ai-ml-api-pricing |
| Unify (unify.ai) | Pivoted to a continual-learning research lab; `docs.unify.ai` does not resolve (ENOTFOUND); no hosted OpenAI-compat LLM API/free base_url remains. | https://unify.ai |
| Fal | Queue/prediction API (`queue.fal.run/<model>`), NOT `/chat/completions`; PAYG, no free tier. | https://fal.ai/docs/documentation/quickstart , https://fal.ai/pricing |
| Replicate | Predictions API (`api.replicate.com/v1/predictions`), NOT `/chat/completions`; "free limits" only for select models via predictions API, card/prepaid for sustained use. | https://replicate.com/docs/reference/http , https://replicate.com/docs/topics/billing |
| Perplexity | Paid API (Sonar/Agent/Router/Search/Embeddings all per-token/per-request with billing setup); no free tier/credits. | https://docs.perplexity.ai/docs/getting-started/pricing |
| Writer | Paid per-token (Palmyra X6 etc.); enterprise-focused; no free tier/credits. | https://dev.writer.com/home/pricing |
| GooseAI | Credit pre-purchase system, no free credits; models are stale 2022-era (GPT-Neo/GPT-J/GPT-NeoX). | https://goose.ai/pricing |
| NLP Cloud | Task-specific API (`/v1/<model>/<task>`), NOT `/chat/completions`. | https://nlpcloud.com |
| Lepton AI | Acquired by NVIDIA (2025) → "NVIDIA DGX Cloud Lepton" GPU compute platform; old hosted OpenAI-compat LLM API + free credits gone; "free plan" is for compute, not a hosted LLM base_url. | https://www.nvidia.com/en-us/data-center/dgx-cloud-lepton/ , https://docs.nvidia.com/dgx-cloud/lepton/guides/ |
| AnyScale | Pivoted to "Production-scale AI with Ray" compute; old OpenAI-compat LLM Endpoints deprecated; $100 credit is for Ray compute, not a free hosted LLM base_url. | https://www.anyscale.com |
| Databricks | OpenAI-compat Foundation Model APIs but require a Databricks workspace + PAT (per-workspace base_url); paid platform, no free hosted no-card base_url. | https://docs.databricks.com/aws/en/machine-learning/foundation-model-apis/ |
| Llama-API | Third-party `llama-api.com` currently down (Cloudflare 522); not official Meta; Meta's llama.meta.com offers no free OpenAI-compat API. | https://llama-api.com |
| ZhipuAI BigModel (open.bigmodel.cn) | DUPLICATE of the cataloged `zai` entry — z.ai is ZhipuAI's international brand; `open.bigmodel.cn` is the same platform's domestic host (same `/api/paas/v4` path, same `zai-sdk`, identical GLM models). Keep `zai`; do not add separately. | https://docs.bigmodel.cn/cn/guide/start/quick-start |
| Baichuan (百川) | Business-application-gated ("申请体验测试，商务跟进"), not self-serve; public API doc is a JS app showing only the apply shell — base_url not verifiable. | https://platform.baichuan-ai.com/docs/api |
| Minimax | No free text-chat tier — text models (MiniMax-M3/M2.7) are pay-as-you-go; the only "Free" models (MiniMax-H3/H3-Max) are video-generation, not chat completions; no free signup credits. | https://platform.minimax.io/docs/guides/pricing-paygo |
| 01.AI (Yi) | Platform winding down consumer API (official 2026-08-03 notice: "逐步停止…API 调用及充值"); pivoting to enterprise. | https://platform.lingyiwanwu.com/ |
| Targon | Confidential GPU/CPU compute rental marketplace (Bittensor-backed; per-GPU-hour pricing), not an OpenAI-compat LLM chat API. | https://targon.com |
| Sherpa | Enterprise federated-learning / privacy-preserving AI SaaS ("Book a demo"), not an OpenAI chat API. | https://sherpa.ai |
| OpenBase | `openbase.com` does not resolve (DNS ENOTFOUND); unreachable/defunct (historically an AI-tools directory, not an LLM API). | https://openbase.com |

---

## Section 3 — High-risk flags (parent must independently re-fetch)

### Existing providers
1. **volcengine-ark model IDs (HIGH rotation).** Date-suffixed `doubao-seed-*-<YYMMDD>` IDs rotate frequently; the recommended `doubao-seed-2-1-pro-260915` / `doubao-seed-2-1-turbo-260628` are current as of the live model-list page but will rotate again. Consider the `doubao-seed-evolving` alias for rotation resistance. Also: the catalog's `doubao-seed-2-0-lite-260428` was not merely stale but **non-free** — confirm replacement is in the free-quota list.
2. **groq free_models + free-tier TPM.** 4/5 catalog IDs are wrong/deprecated; recommended replacement list (`openai/gpt-oss-120b`, `openai/gpt-oss-20b`, `qwen/qwen3.8-27b`, `openai/gpt-oss-safeguard-20b`) must be re-confirmed on the live models page. Discrepancy: rate-limits doc says **8K TPM** for free tier vs models page "250K TPM" (that 250K is the paid Developer tier) — confirm the actual free-tier TPM. Also re-verify whether `moonshotai/kimi-k2-instruct` is truly gone.
3. **cerebras EXCLUDE (policy change).** First-hand confirmed card-required $5 trial + "no permanently free tier" — but this reverses the prior catalog, so the parent should re-fetch `inference-docs.cerebras.ai/support/rate-limits` to confirm the policy truly changed (and there's no separate no-card tier) before removing.
4. **zai free-flash status (RESOLVED first-hand).** z.ai pricing page confirms the FREE flash models are `glm-4.5-flash` and `glm-4.7-flash`; the "latest" `glm-5.3-flash`/`glm-5.3-flashx` are **PAID**. Use `["glm-4.5-flash", "glm-4.7-flash"]`. (A subagent had wrongly suggested `glm-5.3-flash` — corrected by direct fetch of https://docs.z.ai/guides/overview/pricing.)
5. **novita-ai "$1 credit, no card" claim.** base_url (no `/v1`) and the two $0 models are confirmed; the "$1 signup credit, no card" figure could not be confirmed from public docs without login. The $0 models satisfy the free criterion regardless, but if the catalog's `notes` cite the $1 credit, verify on `novita.ai/billing` after signup.
6. **alibaba-dashscope alias resolution + free-quota terms.** Confirm `qwen-plus`/`qwen-max` still resolve (else refresh to `qwen3.7-plus`/`qwen3.8-max`) and that the 90-day no-card free quota still applies.
7. **mistral free-mode terms.** Confirm which current models are free and that the free tier still needs no card.
8. **siliconflow / openrouter free-SKU rotation.** Both lists rotate fast; re-enumerate from live pricing / `/api/v1/models` at catalog-update time.
9. **nvidia trial caps.** 1000-credit grant is a one-time trial (5000 total cap), not perpetually free; build.nvidia.com now also shows paid partner per-token pricing alongside the free-credit NIM endpoint — confirm the 4 catalog models remain free-credit-eligible. Also re-confirm the exact API `model` string for `meta/llama-3.3-70b-instruct` and `mistralai/mixtral-8x22b-instruct-v0.1` (dotted form per catalog vs underscored URL slug).
10. **cloudflare model namespace.** Confirm `@cf/mistralai/mistral-7b-instruct-v0.1` (catalog) vs `@cf/mistral/mistral-7b-instruct-v0.1` (current pricing page path) — possible namespace change.

### New candidates
11. **Tencent Hunyuan (REVERSAL — base_url resolved).** This ADD **reverses the prior research's "decommissioned 2026-09-30" rejection** — the parent must carefully re-verify the free tier (1M tokens on first activation) is still claimable, because a TokenHub migration notice is present. base_url is now **RESOLVED**: canonical = `https://api.hunyuan.cloud.tencent.com/v1` per the authoritative OpenAI-compat doc (1729/111007, updated 2026-04-27, verbatim base_url + curl + OpenAI/Node/Go SDK identical); the `hunyuan.cloud.tencent.com/openai/v1` form is a non-canonical TRTC-integration artifact, not in the official OpenAI-compat doc. (Anthropic-compat sibling: `api.hunyuan.cloud.tencent.com/anthropic`.) So no base_url re-verification needed — only the free-tier/migration re-check remains.
12. **Kluster AI (provisional ADD).** Official `kluster.ai` docs were DNS-unreachable from the research environment (entire domain ENOTFOUND). base_url (`https://api.kluster.ai/v1`) + model IDs are secondary-source-confirmed (Kluster official X + a blog), NOT from a directly-fetched official curl. **Re-verify against live official docs before shipping** — do not ship on secondary sources alone.
13. **ModelScope base_url.** Official quickstart is JS-walled; base_url `https://api-inference.modelscope.cn/v1` is corroborated by the official ModelScope GitHub repo + VoltAgent integration doc, but not a directly-fetched official curl. Re-verify the exact base_url from official docs.
14. **AI21 model ID strings.** Confirm exact API model strings (`jamba-large`/`jamba-mini` vs `-latest`/versioned aliases) against the live API reference.
15. **Infermatic free-plan card policy + domain.** Confirm the free plan requires no card at signup. Real domain is `infermatic.ai` (API host `api.totalgpt.ai`); the task's `infermatic.com` was the wrong TLD (SSL error).
16. **Chinese-provider voucher/quota churn (all 5 CN ADDs).** Baidu ¥20 (1 month), Stepfun gifted credits (expire), iFlyTek free quota, Hunyuan 1M (1 year), ModelScope Magicubes — all are time-limited grants, not perpetually-free models (except iFlyTek `lite` + ModelScope's free-inference mechanism). Model lineups churn fast (iFlyTek Max→Ultra 2026-03-10; Stepfun audio 限时免费; Hunyuan TokenHub migration; Baidu ernie-5.x). **Freeze each `free_models` list by hitting the provider's `GET /models` (or equivalent) at catalog-build time.**
