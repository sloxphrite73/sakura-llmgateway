# Provider 余额 / 配额 API 调研 — Sakura LLM Gateway 路由策略层

> 调研日期：逐家核对其 **官方一手文档**（API reference / 计费页 / 控制台帮助 / OpenAPI 规范），
> 非第三方博客。所有 `endpoint` / `value_path` / 鉴权均来自下列一手链接，未臆造。
>
> **背景**：路由策略层（见 `strategy-impl-spec.md` §2.2/§3）为每个 provider 派生两个 sort 属性：
> `token_balance`（剩余 token 配额）与 `bill_balance`（剩余 USD/CNY/credit 余额）。
> 配置标志 `has_token_balance_api` / `has_bill_balance_api` 决定该 provider 是否被纳入余额探测
> （step3 的 provider 专属余额 API 只对 `has_*_api=true` 的 provider 轮询）。
> 约定（spec §2.2）：**没有余额 API 的 provider，两值视作 `+∞`（实现里 `OptInf::None`），
> 网关直接跳过轮询**。所以「无余额 API」是合法且常见的结论——不强行判定为有。
>
> **范围**：`free-catalog.json` 全部 19 家中，Pollinations 为 keyless 匿名服务、无余额概念，
> 按任务要求跳过；另把 **OpenAI（付费）作为返回 shape 参照**单独列在末尾，它不在免费目录内。
> 故下文实际调研 18 家目录 provider + 1 家参照。
>
> **两类属性相互独立**：一家可能只有 `bill_balance` 而无 `token_balance`，反之亦然。
> 例：OpenRouter 返回的是 USD credit 余额而非 token 配额；Mistral 的 Admin API 返回的是
> token 速率上限而非「剩余」计数器。

---

## 0. 一句话结论

- **可干净轮询 `bill_balance`（用推理 API Key 同一凭证即可）**：Moonshot、SiliconFlow、
  Novita AI、Requesty、OpenRouter（OpenRouter 的 `limit_remaining` 为 per-key 上限剩余，
  未设 per-key 上限时为 `null`，详见各自小节）。
- **API 存在但需要「不同凭证 / 间接计算」**：Fireworks AI（月度消费上限，非信用余额）、
  Mistral（token 上限−已用，需 Admin key）、Alibaba DashScope（阿里云 AK/SK，非 sk- key）、
  Volcengine Ark（火山引擎 AK/SK HMAC 签名，非 ARK_API_KEY）。
- **无余额 API，两值视作 `+∞`、跳过轮询**：Z.AI、Cerebras、NVIDIA NIM、Groq、Cloudflare、
  Google AI Studio、SambaNova、Hugging Face Router、Cohere、OpenAI（参照）。
- **没有任何目录 provider 暴露干净的「剩余 token 配额」单一字段**——多数免费层只靠
  HTTP 429（部分带 `x-ratelimit-*` 响应头）限流。因此 **`token_balance` 对绝大多数 provider
  应直接置 `+∞`**，仅 Mistral 可用「limit−usage」间接算（且需 Admin key，性价比低）。

---

## 1. 汇总表

> `confidence`：confirmed = 读到一手文档/端点原文；inferred = 文档强暗示但未读到完整响应 JSON；
> unconfirmed = 公开文档无法确认。`has_*` = yes/no/unconfirmed。`—` 表示不适用。

| # | Provider | has_token_balance_api | has_bill_balance_api | endpoint | method | auth | value_path | unit | confidence |
|---|----------|:---:|:---:|---|---|---|---|---|---|
| 1 | SiliconFlow 硅基流动 | no | yes | `https://api.siliconflow.cn/v1/user/info` | GET | `Authorization: Bearer <token>` | `data.totalBalance`（另 `data.balance` 赠送 / `data.chargeBalance` 充值） | credits(CNY) | confirmed |
| 2 | Z.AI (智谱 GLM) | no | no | — | — | — | — | n/a | confirmed |
| 3 | OpenRouter `:free` | no | yes | `https://openrouter.ai/api/v1/key` | GET | `Authorization: Bearer <key>` | `data.limit_remaining`（per-key 上限剩余，无上限时 `null`）；另 `data.usage` 全历史已用、`data.free_model_daily_requests.remaining` 当日免费模型剩余请求数 | usd | confirmed |
| 4 | Cerebras | no | no | — | — | — | — | n/a | confirmed |
| 5 | NVIDIA NIM | no | no | — | — | — | — | n/a | confirmed |
| 6 | Groq | no | no | — | — | — | — | n/a | confirmed |
| 7 | Mistral La Plateforme | yes* | no | `https://api.mistral.ai/v1/admin/rate-limit`（配合 `/v1/admin/usage`） | GET | `Authorization: Bearer <admin key>`（AdminApiKey，非推理 key） | `tokens_limits_by_model`（配置上限，非剩余；剩余需 `−/admin/usage` 自算） | tokens | confirmed |
| 8 | Cloudflare Workers AI | no | no | — | — | — | — | n/a | confirmed |
| 9 | Google AI Studio (Gemini) | no | no | — | — | — | — | n/a | confirmed |
| 10 | SambaNova | no | no | — | — | — | — | n/a | confirmed |
| 11 | Hugging Face Router | no | no | — | — | — | — | n/a | confirmed |
| 12 | Fireworks AI | no | yes* | `https://api.fireworks.ai/v1/accounts/{account_id}/quotas`（单查 `…/quotas/monthly-spend-usd`） | GET | `Authorization: Bearer <API_KEY>` | `quotas[monthly-spend-usd].maxValue`(批准上限)/`.value`(生效上限)/`.usage`(已消费)；剩余≈`value/maxValue−usage` | usd | confirmed |
| 13 | Novita AI | no | yes | `https://api.novita.ai/openapi/v1/billing/balance/detail` | GET | `Authorization: Bearer <API_KEY>` | `availableBalance`（单位 = 1/10000 美元，需 ÷10000） | usd | confirmed |
| 14 | Requesty | no | yes | `https://api-v2.requesty.ai/v1/manage/org` | GET | `Authorization: Bearer <REQUESTY_API_KEY>` | `balance`（组织剩余 USD 余额） | usd | confirmed |
| 15 | Cohere | no | no | — | — | — | — | n/a | confirmed |
| 16 | Alibaba DashScope (Bailian) | no | yes | `https://bssopenapi.aliyuncs.com/?Action=QueryAccountBalance&Version=2017-12-14` | GET | 阿里云 AccessKey（AK/SK）签名 + RAM 权限 `bss:DescribeAccountBalance`（**非** 百炼 sk- key） | `Data.AvailableAmount`（CNY） | credits(CNY) | inferred |
| 17 | Volcengine Ark (Doubao) | no | yes | `https://open.volcengineapi.com/?Action=QueryBalanceAcct&Version=2022-01-01` | GET | 火山引擎 AccessKey（AK/SK）HMAC-SHA256 签名（**非** 方舟 Bearer ARK_API_KEY） | `Result.AvailableBalance`（CNY；另 `CashBalance`/`ArrearsBalance`/`CreditLimit`/`FreezeAmount`） | credits(CNY) | confirmed |
| 18 | Moonshot Kimi | no | yes | `https://api.moonshot.cn/v1/users/me/balance` | GET | `Authorization: Bearer <MOONSHOT_API_KEY>`（与推理同 key） | `data.available_balance`（另 `data.voucher_balance` 代金券 / `data.cash_balance` 现金，CNY） | credits(CNY) | confirmed |
| 19 | OpenAI（付费参照，不在目录） | no | no | — | — | — | — | n/a | confirmed |

> \* Mistral：`has_token_balance_api=yes*` 表示需 `limit − usage` 自行计算，非单一「剩余」字段；
> Fireworks：`has_bill_balance_api=yes*` 表示该值是「月度消费上限剩余」而非预付信用余额本身。

**汇总统计**（18 家目录 provider）：
- `bill_balance` 可查：8 家（Moonshot、SiliconFlow、Novita、Requesty、OpenRouter、Fireworks、Alibaba DashScope、Volcengine Ark）——其中 4 家需换凭证（Fireworks 同 key 但语义为消费上限；Mistral/DashScope/Ark 需 AK/SK 系凭证）。
- `token_balance` 可查：1 家且为间接计算（Mistral）；**没有任何 provider 给出干净的「剩余 token 配额」单一字段**。
- 两值均 `+∞`、跳过轮询：10 家（Z.AI、Cerebras、NVIDIA、Groq、Cloudflare、Google、SambaNova、HF Router、Cohere，+ OpenAI 参照）。

---

## 2. 逐家详情

### 1. SiliconFlow 硅基流动 — `bill_balance` 可查 ✅

- **判定**：`has_bill_balance_api=yes`（CNY 信用余额），`has_token_balance_api=no`。
- **端点**：`GET https://api.siliconflow.cn/v1/user/info`，`Authorization: Bearer <token>`。
  返回 `data.balance`（赠送余额）、`data.chargeBalance`（充值余额）、`data.totalBalance`（总余额 = balance + chargeBalance），单位人民币元。
- **一手来源**：
  - https://docs.siliconflow.com/cn/api-reference/userinfo/get-user-info
  - https://docs.siliconflow.com/cn/faqs/misc_finance
- **Gotcha**：
  - 文档示例用的是 `api.siliconflow.com` 域名，本目录 `base_url` 为 `api.siliconflow.cn`；两者都是硅基流动官方域名，`/user/info` 路径应一致，但**网关上线前需自行验证 `.cn` 域名是否同样返回这些字段**。
  - 免费模型（DeepSeek/Qwen 等小模型）无 token 配额概念，超额按 429 限流，**无 token 余额查询接口**。
  - 账单明细（消费记录/发票）只能在 `cloud.siliconflow.cn/bills` 网页控制台查看，无单独明细 API。

---

### 2. Z.AI (智谱 GLM) — 无余额 API ❌

- **判定**：两项均 `no`，两值置 `+∞`、跳过轮询。
- **一手来源**：
  - https://docs.z.ai/llms.txt （完整 API 索引：仅推理/图像/视频/音频/工具/Managed Agents，无余额/账户端点）
  - https://docs.bigmodel.cn/llms.txt
  - https://docs.bigmodel.cn/cn/faq/fee-issues （费用 FAQ：消费明细仅网页控制台可见）
  - https://docs.z.ai/devpack/faq
- **Gotcha**：计费按 token，优先扣资源包再扣现金余额，但**余量仅网页可见**（`bigmodel.cn/finance/overview`），无可轮询接口。GLM-4-Flash 系列免费但靠限流控制。余额不足时推理返回错误码 `1113`（Insufficient Balance），即通过错误响应强制，无可查询端点。Coding Plan(devpack) 的周配额也只能在 `z.ai/manage-apikey/subscription` 网页查看。

---

### 3. OpenRouter `:free` — `bill_balance` 可查 ✅（per-key 上限语义）

- **判定**：`has_bill_balance_api=yes`（USD credit，per-key 上限语义），`has_token_balance_api=no`。
- **端点**：`GET https://openrouter.ai/api/v1/key`，`Authorization: Bearer <OPENROUTER_API_KEY>`。
  返回 `data.limit_remaining`（该 key 的 per-key credit 上限剩余，**未设 per-key 上限时为 `null`**）、`data.usage`（全历史已用 credit）、`data.usage_daily/weekly/monthly`、`data.is_free_tier`、`data.free_model_daily_requests.remaining`（当日免费模型剩余请求数，单位 requests 非 token）。
- **一手来源**（已抓取核对响应 shape）：
  - https://openrouter.ai/docs/api_reference/limits （官方明确推荐「proactively call GET /api/v1/key to track limit_remaining and usage」）
- **Gotcha**：
  - `limit_remaining` 是 **per-key 上限**的剩余，**不是账户级总余额**；若该 key 未设 per-key 上限则为 `null`——此时网关拿不到真实账户余额。账户级余额（`402` 来源之一）并不作为独立字段返回。
  - `:free` 模型走 429 + `X-RateLimit-*` 头限流；付费超额返回 `402`。免费模型日请求数：充 < $10 为 50/天，≥ $10 为 1000/天，由 `data.free_model_daily_requests.limit` 反映。
  - credit ≈ USD（约 1 credit ≈ $1），按 `usd` 处理。
  - **网关建议**：把它当「per-key 剩余信用」来轮询，对设了 per-key 上限的 key 有效；未设上限的 key `token_balance/bill_balance` 可回退 `+∞`，改由 402/429 兜底。

---

### 4. Cerebras — 无余额 API ❌

- **判定**：两项均 `no`。
- **一手来源**：
  - https://inference-docs.cerebras.ai/llms.txt （端点全集：chat/completions/models/batch/file/metrics/customer_management_api，无 billing/credits/balance/usage）
  - https://inference-docs.cerebras.ai/support/rate-limits
  - https://inference-docs.cerebras.ai/console/account-billing （明示「剩余 credit 仅控制台 Overview 标签页显示」）
  - https://inference-docs.cerebras.ai/console/usage-monitoring
- **Gotcha**：账户有 $5 免费试用 credit（30 天过期）但**仅网页控制台可见，无 API**。限流以 RPM/TPM 计（token bucket），超限 429。`Retrieve metrics` 端点（GET，Prometheus 格式，仅 dedicated endpoint）返回的是请求/token/延迟/健康运营指标，并非余额/配额。**只能靠 429 兜底**。

---

### 5. NVIDIA NIM — 无余额 API ❌

- **判定**：两项均 `no`。
- **一手来源**：
  - https://docs.api.nvidia.com/nim/reference （仅推理 POST 端点：chat/embedding/rerank/visual/multimodal/healthcare/route/climate，无 billing/credits/balance/usage/me/key）
  - https://docs.api.nvidia.com/nim/docs/faq
  - https://docs.api.nvidia.com/nim/docs/api-quickstart
- **Gotcha**：`build.nvidia.com` 注册赠送免费 API call credits（每 credit≈1 次请求，免费层约 1000 credits + 40 RPM），但**credits 仅网页控制台可见**，远程 API 调用扣减 trial credits，耗尽或超 RPM 返回 429（论坛多用户反映无法在 API 侧查剩余 credits）。生产使用需 NVIDIA AI Enterprise 授权。**只能靠 429 兜底**。

---

### 6. Groq — 无余额 API ❌

- **判定**：两项均 `no`。
- **一手来源**：
  - https://console.groq.com/docs/api-reference （端点仅 chat/responses/audio/models/batches/files/fine-tuning，无 billing/usage/credits/balance/key）
  - https://console.groq.com/docs/rate-limits
- **Gotcha**：限流以 RPM/RPD/TPM/TPD 计、超限 429；**唯一程序化信号是推理响应头** `x-ratelimit-remaining-tokens` / `x-ratelimit-remaining-requests` / `x-ratelimit-reset-*`，但反映的是当前分钟/日窗口的剩余速率（持续回填，非总量配额余额），且**仅随推理请求返回、并非可独立轮询的端点**。确切限流需在账户 settings 的 limits 网页查看。Spend Limits/Billing 均在网页控制台。**只能靠 429 + 响应头兜底**。

---

### 7. Mistral La Plateforme — `token_balance` 间接可算 ⚠️（需 Admin key）

- **判定**：`has_token_balance_api=yes*`（需 `limit − usage` 自行计算，非单一剩余字段），`has_bill_balance_api=no`。
- **端点**：`GET https://api.mistral.ai/v1/admin/rate-limit`（配合 `GET /v1/admin/usage`），`Authorization: Bearer <admin api key>`（**AdminApiKey，非普通推理 key**）。
  - `/admin/rate-limit` 返回 `requests_per_second` 与 `tokens_limits_by_model`（按模型映射，每项含 `tokens_per_minute` / `tokens_per_month`）——这是**配置的速率/token 上限**，不是「已用/剩余」计数器。
  - 「剩余」= 上述 `tokens_per_month` − `GET /v1/admin/usage` 返回的该模型已消耗量，**自行相减**。
- **一手来源**：
  - https://docs.mistral.ai/api/endpoint/beta/admin/billing
  - https://docs.mistral.ai/api
- **Gotcha**：
  - USD 余额方面：`GET /v1/admin/spend-limit` 仅返回 `currency:"USD"`、`monthly_limit_reached` 布尔与 `last_payment_failure`，**不返回限额金额或剩余 USD 数值**，故 `has_bill_balance_api=no`。
  - Mistral 为**月度后付费**账单模式，无预付代金券/余额概念。
  - 需 **Admin API key**（与推理 key 不同），网关若仅持推理 key 无法直接轮询；且为「limit−usage」计算，**性价比低**，建议仅在有强需求时启用，否则置 `+∞`。

---

### 8. Cloudflare Workers AI — 无余额 API ❌

- **判定**：两项均 `no`。
- **一手来源**：
  - https://developers.cloudflare.com/workers-ai/platform/pricing/
  - https://developers.cloudflare.com/api/resources/ai/ （REST 资源仅 run/finetunes/assets/public，无 usage/quota 子资源）
  - https://developers.cloudflare.com/workers-ai/llms.txt
  - https://developers.cloudflare.com/workers-ai/platform/limits/
  - https://developers.cloudflare.com/ai-gateway/features/unified-billing/
- **Gotcha**：免费层每日 10,000 Neurons（UTC 00:00 重置），**仅 `dash.cloudflare.com` 的 Workers AI 仪表板可看用量**，无可查询「剩余 Neurons」REST 端点；超额请求直接失败。限额以 RPM 按任务类型/模型强制（见 limits 页）。AI Gateway 的预付 credits（Unified Billing）是独立功能，仅在仪表板「Credits Available」卡片充值/管理，文档未提供查询剩余 credit 的 REST 端点。**只能靠限流失败兜底**。

---

### 9. Google AI Studio (Gemini) — 无余额 API ❌

- **判定**：两项均 `no`。
- **一手来源**：
  - https://ai.google.dev/gemini-api/docs/rate-limits
  - https://ai.google.dev/api （端点含 Interactions/generateContent/streamGenerateContent/Live API/Batch/Files/Tokens/Caching/Embeddings/Model API/Agents/Webhooks/Triggers/Environments，无 quota/billing/balance/usage）
- **Gotcha**：仅以 RPM/TPM/RPD 限额，每日 RPD 在太平洋时间午夜重置；超额返回 `429 RESOURCE_EXHAUSTED`。当前速率限额**只能在 `aistudio.google.com/rate-limit` 网页查看，无 API 查剩余配额**。Free 层无消费概念；付费层走 Google Cloud 账单，无可查询「剩余 USD 余额」端点。**只能靠 429 兜底**。

---

### 10. SambaNova — 无余额 API ❌

- **判定**：两项均 `no`。
- **一手来源**：
  - https://raw.githubusercontent.com/sambanova/sambanova-inference-api-spec/refs/heads/main/openapi.documented.json （官方 OpenAPI，server=`https://api.sambanova.ai/v1`，schemas 全为推理相关：chat completions / Anthropic 兼容 messages+count_tokens / embeddings / content / tools / errors，无 account/billing/credits/quota/usage/key 端点）
  - https://docs.sambanova.ai/docs/llms.txt
  - https://docs.sambanova.ai/docs/en/api-reference/overview
- **Gotcha**：免费层限流制（每模型 20 RPM / 20 RPD / 20 万 TPD，每日重置），通过 `429`（`rate_limit_error`）强制；**额度仅可在 `cloud.sambanova.ai` Web 控制台查看**。推理鉴权 `Authorization: Bearer <key>`（`/messages` 亦接受 `x-api-key`）。**只能靠 429 兜底**。

---

### 11. Hugging Face Router — 无余额 API ❌

- **判定**：两项均 `no`。
- **一手来源**：
  - https://huggingface.co/docs/inference-providers/pricing （免费 $0.10/月、PRO $2.00/月、Team/Enterprise 每席 $2.00/月）
  - https://huggingface.co/docs/inference-providers/hub-api （仅 `POST /v1/chat/completions` 与 `GET /v1/models`）
  - https://huggingface.co/docs/inference-providers/index
  - https://huggingface.co/.well-known/openapi.md （含 `GET /api/whoami-v2`，但仅返回认证信息，非余额）
  - https://github.com/huggingface/huggingface.js/blob/main/packages/inference/src/InferenceClient.ts （方法全由 tasks 导出，无 credits/balance）
- **Gotcha**：HF 每月发 USD 免费额度，但**官方文档明确把余额/用量查询导向 Web 页面**：`huggingface.co/settings/billing`、`huggingface.co/settings/inference-providers/overview`。`/api/whoami-v2` 仅返回用户/认证信息，非余额。社区假设的 `/v1/credits` 在官方 inference-providers 文档与一方客户端源码中**均未记载**，不得作为可轮询端点。鉴权 `Authorization: Bearer $HF_TOKEN`。**只能靠 402/429 兜底**。

---

### 12. Fireworks AI — `bill_balance` 可查 ✅（语义为「月度消费上限剩余」⚠️）

- **判定**：`has_bill_balance_api=yes*`（USD 月度消费上限剩余，**非预付信用余额本身**），`has_token_balance_api=no`。
- **端点**：`GET https://api.fireworks.ai/v1/accounts/{account_id}/quotas`（列表）或单查 `GET https://api.fireworks.ai/v1/accounts/{account_id}/quotas/monthly-spend-usd`，`Authorization: Bearer <API_KEY>`。
  响应每项含 `name` / `value`(string\<int64\>) / `maxValue`(string\<int64\>) / `usage`(number\<double\>) / `updateTime`。对 `monthly-spend-usd` 项：`maxValue`=批准的月度消费上限、`value`=实际生效上限（可手动调低）、`usage`=本月已消费，**剩余 ≈ `value/maxValue − usage`**。
- **一手来源**：
  - https://docs.fireworks.ai/api-reference/introduction
  - https://docs.fireworks.ai/api-reference/list-quotas
  - https://docs.fireworks.ai/api-reference/get-quota
  - https://docs.fireworks.ai/api-reference/update-quota
  - https://docs.fireworks.ai/guides/quotas_usage/account-quotas
  - https://docs.fireworks.ai/llms.txt
- **Gotcha**：
  - **重要语义差异**：这是「月度消费限额」的剩余，**并非预付费信用余额本身**。Fireworks 为预付费信用制（新户一次性 $1 及充值的信用余额），但**信用余额仅在 `fireworks.ai/billing` Web 账单控制台可见，此配额 API 不返回信用余额**。对未设消费限额的免费账户，该 API **不能反映 $1 免费额度的真实剩余**（`maxValue` 可能高于实际信用）。
  - 端点位于**管理 API base** `https://api.fireworks.ai/v1`，与推理 `base_url` `https://api.fireworks.ai/inference/v1` 不同。
  - `value/maxValue` 为 `string<int64>`、`usage` 为 `double`，文档未明确 USD 小数缩放（美元整数或美分），示例以美元计。
  - serverless 为自适应 TPM 限流，经 429 强制，**无可查询的剩余 token 配额**。
  - **网关建议**：可作为「本月还能花多少」的弱信号轮询，但**不建议直接当作 $1 免费额度剩余**；未设限额时回退 `+∞`。

---

### 13. Novita AI — `bill_balance` 可查 ✅

- **判定**：`has_bill_balance_api=yes`（USD 信用余额），`has_token_balance_api=no`。
- **端点**：`GET https://api.novita.ai/openapi/v1/billing/balance/detail`，`Authorization: Bearer <API_KEY>`。
  返回 `availableBalance`（可用信用余额，推荐取此值）、`cashBalance`（充值余额）、`creditLimit`（信用额度上限）、`pendingCharges`（待结费用，默认 0）、`outstandingInvoices`（欠款）。
- **一手来源**：
  - https://novita.ai/docs/api-reference/basic-get-user-balance.md
  - https://docs.novita.ai/llms.txt
- **Gotcha**：
  - **余额单位是 1/10000 美元**（例 `10000` = $1.00、`1000000` = $100.00），**网关需 ÷10000 换算成美元**后填入 `bill_balance`。
  - 端点路径位于 `/openapi/v1/billing/`，与 OpenAI 兼容 `base_url`（`/openai`）**不同**，网关需单独配置该端点调用。
  - 无 token 剩余配额端点：免费额度以美元信用形式发放（注册赠 $1、另有 $0 限时免费模型），超额返回 402/429。另有 `basic-query-monthly-bill` 端点可查月账单。

---

### 14. Requesty — `bill_balance` 可查 ✅

- **判定**：`has_bill_balance_api=yes`（USD 组织余额），`has_token_balance_api=no`。
- **端点**：`GET https://api-v2.requesty.ai/v1/manage/org`，`Authorization: Bearer <REQUESTY_API_KEY>`（与推理 key 同一凭证）。
  返回 `{name, balance}`，`balance` 为组织剩余美元余额；余额耗尽返回 `402 Payment Required`。
- **一手来源**：
  - https://docs.requesty.ai/api-reference/endpoint/manage-org-get.md
  - https://docs.requesty.ai/api-reference/endpoint/manage-apikey/manage-api-key-get.md
  - https://docs.requesty.ai/api-reference/endpoint/manage-apikey/manage-api-key-get-usage.md
  - https://docs.requesty.ai/llms.txt
- **Gotcha**：
  - 端点位于**管理 API 域** `api-v2.requesty.ai/v1/manage`，与推理路由 `base_url`（`router.requesty.ai/v1`）**不同**，需用同一 Requesty API Key 单独调用。
  - 另有 `GET /v1/manage/apikey/self` 返回该 key 当月 `monthly_spend` 与 `monthly_limit`（0=无限制），以及 `GET /v1/manage/apikey/{id}/usage` 返回按时间聚合的 `input_tokens`/`output_tokens`/`total_tokens`/`spend`/`completions_requests`（**历史用量，非剩余配额**）。
  - 免费层（每日 200 次请求）通过 429 + `Retry-After` 头控制，新账户赠送的免费 credits 体现为组织 USD 余额而非 token 额度。**无 token 剩余配额端点**。

---

### 15. Cohere — 无余额 API ❌

- **判定**：两项均 `no`。
- **一手来源**：
  - https://docs.cohere.com/docs/rate-limits
  - https://docs.cohere.com/reference/about （仅 chat/embed/rerank/tokenize/detokenize/parse/audio 推理端点，无 usage/billing/balance/quota）
  - https://docs.cohere.com/v1/llms.txt
- **Gotcha**：免费试用(evaluation) key 限制为每月 1000 次调用 + 每模型 20 req/min，生产(production) key 500 req/min，**均通过 HTTP 429 + `Retry-After` 头强制限流，无可查询剩余配额端点**。试用 key 为免费调用次数制（无美元余额概念），生产 key 计费仅通过控制台/销售渠道可见。**只能靠捕获 429 推断额度状态**。

---

### 16. Alibaba DashScope (Bailian) — `bill_balance` 可查 ⚠️（需阿里云 AK/SK，非 sk- key）

- **判定**：`has_bill_balance_api=yes`（CNY 账户余额，`confidence=inferred`），`has_token_balance_api=no`。
- **端点**：`GET https://bssopenapi.aliyuncs.com/?Action=QueryAccountBalance&Version=2017-12-14`（阿里云费用中心 BssOpenApi，**非** 百炼 `base_url`），鉴权 = 阿里云 AccessKey（AK/SK）签名 + RAM 权限 `bss:DescribeAccountBalance`（**非** 百炼 sk- API Key）。返回 `Data.AvailableAmount`（可用余额，CNY）。
- **一手来源**：
  - https://help.aliyun.com/zh/model-studio/developer-reference/compatibility-of-openai-with-dashscope （百炼 OpenAI 兼容仅 `/chat/completions`，`usage` 仅单次请求 token 计数）
  - https://help.aliyun.com/zh/model-studio/billing-for-model-studio （百炼计费文档，无可查询余额/额度 API）
  - https://help.aliyun.com/zh/user-center/developer-reference/api-bssopenapi-2017-12-14-queryaccountbalance
  - https://api.aliyun.com/api/BssOpenApi/2017-12-14/QueryAccountBalance
- **Gotcha**：
  - **网关若仅持有百炼模型 API Key（sk-）则无法直接轮询**，须额外存阿里云 AK/SK 并签名调用，部署成本高。
  - `confidence=inferred`：端点/鉴权/用途已在一手文档确认，`value_path Data.AvailableAmount` 依据官方 BSS 参考与 OpenAPI Explorer（`AvailableAmount` 为可用余额字段，CNY），但本次未逐页读取完整响应 JSON，标记为 inferred。
  - 该余额为阿里云账户级现金/代金券余额（百炼按量付费从中扣减）；但**「首次开通百炼即发的 90 天新人免费额度」属百炼侧免费 token 额度，不在此账户余额内、亦无 API 可查**，耗尽按配额/错误返回。
  - **网关建议**：除非已存阿里云 AK/SK，否则置 `+∞`、靠 429/配额错误兜底。

---

### 17. Volcengine Ark (Doubao) — `bill_balance` 可查 ⚠️（需火山引擎 AK/SK，非 ARK_API_KEY）

- **判定**：`has_bill_balance_api=yes`（CNY 账户余额），`has_token_balance_api=no`。
- **端点**：`GET https://open.volcengineapi.com/?Action=QueryBalanceAcct&Version=2022-01-01`（火山引擎费用中心 Billing OpenAPI，**非** 方舟 `base_url`），鉴权 = 火山引擎 AccessKey（AK/SK）HMAC-SHA256 签名（**非** 方舟 Bearer `ARK_API_KEY`）。返回 `Result.AvailableBalance`（可用余额，CNY）及 `Result.CashBalance`/`ArrearsBalance`/`CreditLimit`/`FreezeAmount`。
- **一手来源**：
  - https://docs.volcengine.com/docs/ark/base-url-and-authentication （方舟数据面仅 chat/responses/messages/files，无 balance/quota/usage）
  - https://docs.volcengine.com/docs/ark/get-afp-usage-api
  - https://www.volcengine.com/docs/82379/2479849
  - https://docs.volcengine.com/docs/BillingCenter/QueryBalanceAcct-Queryuseraccountbalanceinformation
- **Gotcha**：
  - **网关若仅持 `ARK_API_KEY` 无法直接轮询**，须额外存火山引擎 AK/SK 并 HMAC-SHA256 签名，部署成本高；资源包另有 `ListResourcePackages`/`ListPackageUsageDetails`。
  - 管控面（`ark.cn-beijing.volcengineapi.com/`，均仅 Access Key 签名、不支持 Bearer）：`GetAFPUsage`（POST，`Action=GetAFPUsage`）返回个人版 AgentPlan 的 AFP 配额与已用量（5h/日/周/月滚动窗口，剩余=Quota−Used，**单位 AFP 非 token**，仅 AgentPlan 套餐，企业版用 `GetSeatAFPUsage`）；`GetUsageDetails`（POST）返回已消耗 token 用量明细（历史用量而非剩余配额）；`GetInferenceUsage` 查推理用量。**免费「安心体验模式」额度不可经此查询**。
  - `has_token_balance_api=no`：数据面无配额端点；管控面 token 相关接口仅返回用量历史/AFP（AK/SK、套餐限定），**无可直接查询的剩余 token 配额快照**。
  - **网关建议**：除非已存火山引擎 AK/SK，否则置 `+∞`、靠 429/配额错误兜底。

---

### 18. Moonshot Kimi — `bill_balance` 可查 ✅（最干净）

- **判定**：`has_bill_balance_api=yes`（CNY 余额），`has_token_balance_api=no`。
- **端点**：`GET https://api.moonshot.cn/v1/users/me/balance`（相对 `base_url` 路径 `/users/me/balance`），`Authorization: Bearer <MOONSHOT_API_KEY>`（**与推理同一 API key**）。
  返回 `data.available_balance`（可用余额，人民币元）= `data.cash_balance` + `data.voucher_balance`；另 `data.voucher_balance`（代金券余额，≥0）、`data.cash_balance`（现金余额，可为负表欠费，为负时 `available_balance` 等于 `voucher_balance`）。`available_balance ≤ 0` 时推理返回 `exceeded_current_quota_error`。
- **一手来源**（已抓取核对响应 shape 与字段说明表）：
  - https://platform.kimi.com/docs/api/balance
  - https://platform.kimi.com/docs/llms-full.txt
- **Gotcha**：
  - **最干净的 bill_balance 候选**：同 key、同域、单一字段、文档完备。`unit` 记为 `credits`，实为人民币元（CNY）。
  - 中国站 `platform.kimi.com` / `api.moonshot.cn` 与国际站 `platform.kimi.ai` / `api.moonshot.ai` 的账户、余额与 API Key **完全隔离**，混用返回 401；本目录 `base_url` 为中国站。
  - 新用户实名认证赠 15 元代金券计入 `voucher_balance`，但**该代金券不可用于 `kimi-k3`**（旗舰）。
  - 余额为 CNY 计费余额，按 token 计费从中扣减；账户接口表仅此一个，**无独立剩余 token 配额端点**，故 `has_token_balance_api=no`。

---

### 附：OpenAI（付费，返回 shape 参照，不在免费目录）

- **判定**：两项均 `no`（后付费，无「剩余 token 配额」或「剩余 USD 余额」概念）。
- **一手来源**：
  - https://developers.openai.com/api/reference/resources/admin/subresources/organization/subresources/usage
  - https://developers.openai.com/api/reference/resources/admin/subresources/organization/subresources/usage/methods/costs
  - https://developers.openai.com/api/reference/resources/admin/subresources/organization/subresources/usage/methods/completions
- **Gotcha（shape 参照）**：
  - 旧预付额度端点 `/dashboard/billing/credit_grants`（曾返回 `total_granted`/`total_used`/`total_available`）与 `/v1/usage` 已废弃，且需浏览器登录态（session cookie），无法用普通 API Key 直接查。
  - 当前公开能力集中在新 Admin API（base `https://api.openai.com/v1`，需专门 Admin API Key，`Authorization: Bearer <key>`）：
    - `GET /organization/usage/completions` 返回**已用** `input_tokens`/`output_tokens`/`num_model_requests` 聚合用量（按 1m/1h/1d 时间桶，`value` 在 `data[].results[].input_tokens`/`output_tokens`）——历史用量报表，非剩余配额。
    - `GET /organization/costs` 返回**已发生** USD 成本（按时间桶）——非剩余余额。
    - `/organization/rate_limits` 返回 RPM/TPM 限额配置而非剩余；Projects 下 Spend Limit/Alerts 是月度花费上限/告警阈值，不返回剩余余额。
  - 实时配额通过 HTTP 429 + `x-ratelimit-*` 响应头执行，无单独可查「剩余配额」端点。
  - **结论**：若网关想跟踪 OpenAI 用量，只能轮询上述 usage/costs 做「已用累加」自算，拿不到官方剩余额；故作 shape 参照，不在目录内。

---

## 3. 推荐（网关该轮询谁 vs. 置 `+∞`）

### 3.1 建议轮询 `bill_balance`（`has_bill_balance_api=true`）

按「凭证干净度」分两档：

**A 档 — 用推理 API Key 同一凭证即可轮询，建议优先接入：**

| Provider | 端点 | value_path | 单位 | 备注 |
|---|---|---|---|---|
| **Moonshot** | `GET https://api.moonshot.cn/v1/users/me/balance` | `data.available_balance` | CNY | 最干净：同 key、同域、单一字段、文档完备。中国站/国际站 key 隔离。 |
| **SiliconFlow** | `GET https://api.siliconflow.cn/v1/user/info` | `data.totalBalance` | CNY | 上线前自验 `.cn` 域名返回字段与文档示例 `.com` 一致。 |
| **Novita AI** | `GET https://api.novita.ai/openapi/v1/billing/balance/detail` | `availableBalance` | USD(÷10000) | 注意路径在 `/openapi/v1/billing/`（非 `/openai`）；单位需 ÷10000。 |
| **Requesty** | `GET https://api-v2.requesty.ai/v1/manage/org` | `balance` | USD | 管理域 `api-v2` 与推理 `router` 不同，但同 key。 |
| **OpenRouter** | `GET https://openrouter.ai/api/v1/key` | `data.limit_remaining` | USD(credit) | `limit_remaining` 为 per-key 上限剩余，**未设 per-key 上限时为 `null` → 回退 `+∞`**；账户级总余额不返回。 |

**B 档 — API 存在但需不同凭证或语义有坑，按需/谨慎接入：**

| Provider | 端点 | value_path | 单位 | 为何谨慎 |
|---|---|---|---|---|
| **Fireworks AI** | `GET https://api.fireworks.ai/v1/accounts/{id}/quotas/monthly-spend-usd` | `maxValue`/`value`/`usage` → 剩余=`value−usage` | USD | 语义是「月度消费上限剩余」**非** $1 免费信用余额本身；未设限额时不反映真实剩余 → 回退 `+∞`。同 key。 |
| **Volcengine Ark** | `GET https://open.volcengineapi.com/?Action=QueryBalanceAcct&Version=2022-01-01` | `Result.AvailableBalance` | CNY | 需火山引擎 AK/SK HMAC 签名，**非** `ARK_API_KEY`；部署成本高。免费「安心体验」额度不可查。 |
| **Alibaba DashScope** | `GET https://bssopenapi.aliyuncs.com/?Action=QueryAccountBalance&Version=2017-12-14` | `Data.AvailableAmount` | CNY | 需阿里云 AK/SK + RAM 权限，**非** sk- key；confidence=inferred。90 天新人免费额度不在此账户余额内、无 API。 |

### 3.2 建议轮询 `token_balance`（`has_token_balance_api=true`）

| Provider | 端点 | value_path | 单位 | 备注 |
|---|---|---|---|---|
| **Mistral** | `GET https://api.mistral.ai/v1/admin/rate-limit`（配合 `/admin/usage`） | `tokens_limits_by_model` − `usage` 自算 | tokens | 需 Admin key（非推理 key）；为「limit−usage」计算非单一剩余字段，性价比低。**无强需求建议置 `+∞`**。 |

> **关键现实**：**没有任何目录 provider 暴露干净的「剩余 token 配额」单一字段**——多数免费层
> 只靠 HTTP 429（Groq/Cerebras/Google 等部分带 `x-ratelimit-*` 响应头，但那是当前窗口剩余
> 速率、持续回填，非总量配额，且仅随推理请求返回、非独立可轮询端点）。因此
> **`token_balance` 对绝大多数 provider 应直接置 `+∞`**，靠 429 兜底，不要强行造一个值。

### 3.3 建议置 `+∞`、跳过轮询（`has_*_api=false`）

以下 9 家目录 provider 两项均无 API，`token_balance`/`bill_balance` 全置 `+∞`、跳过轮询，
统一靠 429/402/`Retry-After` 响应兜底（OpenAI 参照同此，但不在目录内）：

- **Z.AI (智谱 GLM)** — 余额仅网页控制台（错误码 `1113` 强制）。
- **Cerebras** — credit 仅控制台 Overview；RPM/TPM 限流 429。
- **NVIDIA NIM** — trial credits 仅网页；429 限流。
- **Groq** — 仅推理响应头 `x-ratelimit-*`（当前窗口速率，非可轮询端点）。
- **Cloudflare Workers AI** — Neurons 仅仪表板；无 REST。
- **Google AI Studio** — RPM/RPD 仅 `aistudio.google.com/rate-limit` 网页。
- **SambaNova** — 限流 429；额度仅 `cloud.sambanova.ai`。
- **Hugging Face Router** — 余额/用量导向 `huggingface.co/settings/billing` 网页。
- **Cohere** — 调用次数制，429 + `Retry-After` 兜底。
- **OpenAI（参照）** — 后付费，usage/costs 仅历史用量，无剩余额；429 + `x-ratelimit-*`。

### 3.4 给实现者的一句话建议

1. 把 `config.rs::Provider` 的 `has_token_balance_api`/`has_bill_balance_api` 按 §3.1/§3.2 标 `true`，
   其余全 `false`（默认值）。
2. step3 余额探测管线对 `true` 的 provider 跑上表端点；**对 OpenRouter / Fireworks 这类「可能
   返回 null / 语义非信用余额」的，返回 `null` 时按 spec §2.2 折成 `+∞`（`OptInf::None`）**，
   不要报错。
3. **`token_balance` 实务上几乎全员 `+∞`**（仅 Mistral 可选算，但需 Admin key、性价比低），
   建议排序策略里 `token_balance` sort 默认不启用，或仅对有 API 的 provider 生效。
4. 中国云（DashScope/Volcengine）若要真正轮询 `bill_balance`，需网关额外存对应云的 AK/SK 并实现
   HMAC 签名——成本高、且只反映账户现金余额、不反映免费 token 额度。**除非已有云凭证体系，
   否则建议这两家也置 `+∞`、靠配额错误码兜底**。
5. `bill_balance` 单位不统一（CNY vs USD vs credit ÷10000）：sort 比较时**仅在同类 provider 间
   有可比性**；跨货币比较需先归一化（如统一折算成「调用预算」概念），或按 provider 分组排序。
