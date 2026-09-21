# Sakura 路由策略层 — Rust 实现规格（impl spec）

> 上游权威：`sakura-strategy-spec.md`（路由策略模块开发 SPEC 定稿，DSH 附件
> `7e840d6a`）。本文是**如何在现 Rust 代码库里落地它**——定边界、列改动、标决策，
> 不重述 spec 本身（对照 spec 时以 spec §号为准）。
>
> 现状审计见 `PASS.md` §20.3：cooldown 层（spec §7）已完整实现且经 mock_upstream
> 测试覆盖；策略层（§3-§6）未实现，现状 = spec §4 **both-ON 的硬编码版**（单 provider
> 内 round-robin 跳冷却，`state.rs::pick_key` L223）。

---

## 0. 目标与向北兼容铁律

- 在现有 cooldown 层之上叠加 spec §3-§6 的 filter/sort/walk/terminal。
- **向后兼容铁律**：默认配置（`lock_model_group=true`、`lock_provider=true`、用户 sort 空）
  必须复现当前网关行为 = 单 provider 内 **round-robin 跳冷却**。升级零意外。
- cooldown 层（`mark_cooldown` / `learned_cooldown` / 二分 prober / `invalid` 隔离 /
  防抖落盘）**一律不动**，策略层只读它产出的 `valid` / `cold` / `remaining_secs`。
- 术语沿用 spec：行 = `(provider, model, api_key_id)` 三元组；模型组 = 同一逻辑模型
  跨 provider（`group_id` = 逻辑模型名，由 model 推导）。

---

## 1. 模块布局

新增 `llm-gateway/src/strategy.rs`，集中策略逻辑；`state.rs` / `config.rs` /
`proxy.rs` 只做最小接线。

```
strategy.rs
  pub struct Row            // §3 行 + 派生状态 + metrics 快照
  pub fn build_candidates(app, &ReqCtx) -> Vec<Row>      // §2 filter
  pub fn comparator(user_sorts: &[SortKey]) -> impl Fn(&Row,&Row)->Ordering  // §4
  pub fn select(app, rows, sorts, cursor) -> SelectResult   // sort + walk + terminal
  enum SelectResult { Routed(Row), AllCooling { retry_after: Option<u64> },
                      NoValid, Empty }
config.rs
  + Strategy { filter: Filter, sort: Vec<SortKey> }        // §7
  + ModelGroup { id, entries: Vec<(provider_id, upstream_model)> }
  + Provider { has_token_balance_api, has_bill_balance_api, price_table }
state.rs
  + MetricsMap（§3 step3 产出，键 = api_key_id）
  + cursor map（D1，键 = 候选集标识）
proxy.rs::forward
  接入 select() 替换 pick_key；每次 attempt 按 select 返回的 provider 重建 payload
```

---

## 2. 数据模型改动（`config.rs`）

### 2.1 `Config` 顶层
```rust
pub struct Config {
    // …现有字段不变…
    #[serde(default)]
    pub strategy: Strategy,
    #[serde(default)]
    pub model_groups: Vec<ModelGroup>,
}
pub struct Strategy {
    pub filter: Filter,          // 默认 { lock_model_group: true, lock_provider: true }
    pub sort: Vec<SortKey>,      // 0..=3，有序不重复，∈ A..J；默认空 = 纯 round-robin
}
pub struct Filter { pub lock_model_group: bool, pub lock_provider: bool }
```
> `Strategy::default()` = 两 toggle 都 ON + 空 sort = 当前行为（向北兼容铁律）。

### 2.2 `Provider` 增字段
```rust
#[serde(default)]
pub has_token_balance_api: bool,
#[serde(default)]
pub has_bill_balance_api: bool,
#[serde(default)]
pub price_table: BTreeMap<String, f64>,   // model(逻辑名) -> ¥/1M tok；缺省 = 0
```
> spec §2.2：免费模型 price=0；`price_table` 缺条目按 0。balance 字段决定该 provider
> 是否被纳入余额探测（step3 的 provider 专属余额 API 只对 `has_*_api=true` 跑）。

> **余额 API 现实（2026-09-17 调研，详见 `doc/research/provider-balance-apis.md`）**：
> - `bill_balance` 仅 **Tier A 5 家**有干净可轮询 API（同推理 key）：Moonshot
>   `GET /v1/users/me/balance` → `data.available_balance`（CNY，最干净）；SiliconFlow
>   `/v1/user/info` → `data.totalBalance`（CNY）；Novita `/openapi/v1/billing/balance/detail`
>   → `availableBalance`（**÷10000**，单位 1/10000 USD）；Requesty `api-v2…/v1/manage/org`
>   → `balance`（USD）；OpenRouter `/api/v1/key` → `data.limit_remaining`（USD，**null→+∞**）。
> - Tier B 3 家（Fireworks=月度消费上限非信用余额、Volcengine/Alibaba 需云 AK/SK 非
>   推理 key）→ 默认 +∞，除非已有云凭证。
> - **`token_balance`：无一家目录 provider 暴露干净的"剩余 token 配额"单一字段**
>   （Mistral 仅间接 limit−usage 且需 Admin key，低价值）。→ **token_balance 对所有
>   provider 置 +∞，sort E/F 默认不启用**。
> - **跨货币坑**：bill_balance 单位混 CNY/USD/credit÷10000 → sort G/H 跨 provider
>   不可直接比，需归一化（按汇率折 USD）或仅同币种 provider 间比。**开放决策**：
>   归一化方案在策略层实现时定（见 §10 候补）。

### 2.3 `ModelGroup` 与 model→group 推导
```rust
pub struct ModelGroup {
    pub id: String,                                   // 逻辑模型名（= group_id）
    pub entries: Vec<(String, String)>,               // (provider_id, upstream_model)
}
```
- group_id = 逻辑模型名。行的 `model` 字段存逻辑名；upstream 真名由 group entries 推。
- **跨 provider 同模型但上游名不同**（如 OpenAI `gpt-4o` vs 某镜像 `gpt-4o-2024-08`）
  靠 `entries` 的 upstream 字段承载——这是 spec 未写死、实现需定的点（见 §10 D2）。
- 解析：请求 model → 先查 `model_groups` 精确命中得 group_id + 可选 provider；否则回退
  现有 `resolve_model`（`provider/model` + alias），此时 group_id = 该 model 单成员组。

### 2.4 `ApiKey` 不加策略属性
8 个 sort 属性**不进 `ApiKey`**（配置是用户编辑面，不该塞机器实测值）。它们存
`state.rs::MetricsMap`（runtime，键 = api_key_id），由 step3 的实测管线写入。`learned_cooldown`
已是此模式（machine-written、不手填）的先例。

---

## 3. Row 构造与 metrics 接口（依赖 step3）

```rust
// strategy.rs
pub struct Row {
    pub provider_id: String,
    pub upstream_model: String,    // 真上游名（来自 group entries）
    pub api_key_id: String,
    pub key_secret: String,        // call_upstream 要用；从 config 取
    pub group_id: String,
    pub idx: usize,                // 候选集内序号（D1 旋转后）
    // 派生（cooldown 层灌入，只读）
    pub valid: bool,
    pub non_cooled: bool,
    pub remaining_secs: u64,
    // 属性（step3 metrics 快照）
    pub success_rate: f64,
    pub rpm: u32, pub tpm: u32, pub avg_tftt_ms: u32, pub tps: f32,
    pub token_balance: OptInf,     // inf = None（keyless / 无余额 API）
    pub bill_balance: OptInf,
    pub price: f64,
}
```
- `valid` = 该 key **不在** `invalid` map（401/403 隔离）。
- `cold` = 在 `cooling` map；`remaining_secs` = cooling 到期 - now。
- `OptInf` = `Option<T>` 的语义包装：`None` = `+∞`（spec §2.2 inf 约定仅用于两类 balance）。

step3 须提供（`state.rs`）：
```rust
impl App {
    pub fn metrics_of(&self, provider_id: &str, key_id: &str) -> KeyMetrics;
    pub fn price_of(&self, provider_id: &str, model: &str) -> f64;   // 读 config.price_table
}
```

---

## 4. Comparator 与 D1（向北兼容的关键决策）

### 4.1 决策 D1：table-order = cursor 旋转后的配置序

spec §5.2 末位 fallback = table-order。若 table-order = 纯配置序（恒从 index 0），
则 **both-ON + 空 sort 会恒选 key 0 直到它冷却** —— 这**不是**当前 round-robin
行为，破坏向北兼容铁律。

**决策**：候选集排序前先按 cursor 旋转（`rows.rotate_left(cursor)`），使 cursor 指向的
行落到 idx=0；再用**稳定排序**按 comparator 排，`idx`（旋转后序号）作末位 tiebreaker。
效果：
- both-ON + 空 sort：稳定排 `(valid↓, non-cooled↓, idx↑)` → 取第一个 valid+非冷却
  = cursor 处 = **round-robin 下一把**。✓ 复现当前行为。
- both-ON + 用户 sort（如 price↑）：主序 = price；等价 tie 由 cursor 旋转序破 →
  **等价键间也负载均衡**（spec 未要求但净赚）。
- cursor 每次选号后 `cursor = (cursor + 1) % n`（成功/失败均推进，匹配现 `pick_key`
  `counters = idx+1`）。

### 4.2 comparator 签名
```rust
pub enum SortKey { A,B,C,D,E,F,G,H,I,J }   // 对照 spec §5.2
impl SortKey {
    fn cmp(&self, a: &Row, b: &Row) -> Ordering { /* 按 spec 方向 */ }
}
pub fn comparator(user: &[SortKey]) -> impl Fn(&Row,&Row)->Ordering {
    move |a, b| b.valid.cmp(&a.valid)                    // ① valid desc（固定前导，不暴露）
        .then_with(|| b.non_cooled.cmp(&a.non_cooled))   // ② non-cooled desc（固定前导）
        .then_with(|| user.iter().fold(Ordering::Equal,
            |o, k| o.then(k.cmp(a, b))))                  // ③④⑤ 用户 0..3 个
        .then_with(|| a.idx.cmp(&b.idx))                  //   table-order（D1 旋转后 idx）
}
```
- inf 约定（E/F/G/H）：`OptInf::None` 排序时按 `+∞`：`desc` 排最前、`asc` 排最后。
  实现 = `None` 在 `desc` 时视作 `f64::INFINITY`、`asc` 时视作 `+∞`（仍最后）——一个
  `Option`→`f64` 的 helper 两方向自洽，不特判（spec §2.2）。
- 用户 sort 去重/上限 3 在 `Config::load` 校验，超界截断 + warn。

---

## 5. `forward()` 接入（主要改动面）

现 `forward()`（`proxy.rs` L271-455）：`resolve_model` → 取定**单 provider** → 建 payload
一次 → 循环 `pick_key(provider)`。**全程不换 provider**。

策略层接入后，循环体每次 attempt 的 provider 可能变（模型优先/可用优先模式）。
改造点：

1. 解析阶段：`resolve_model` → `(resolved_provider_id, model, group_id)`。
   - resolved_provider_id 可能为 `None`（裸 model 命中多 provider 的 group）→ `lock_provider`
     自动降级 OFF（spec §3 ① / §10 #2）。**新行为**（现状必解析到 provider）。
2. 抽 `build_upstream_payload(inbound_body, inbound_proto, provider, upstream_model)
   -> serde_json::Value`（把现 L309-326 的协议互译逻辑提成函数，按 attempt 调用）。
3. 候选集：`strategy::build_candidates(app, &req)` 按 `cfg.strategy.filter` 缩。
4. 循环：
   ```
   loop (max_attempts):
     let sel = strategy::select(app, &mut rows, &cfg.strategy.sort, &mut cursor);
     match sel {
       Routed(row) => {
         let provider = cfg.provider_by_id(row.provider_id);
         let payload = build_upstream_payload(inbound_body, inbound, provider, row.upstream_model);
         let url = upstream_chat_url(&provider.base_url, provider.protocol());
         call_upstream(provider, row.key_secret, url, payload) -> on fail mark_cooldown/invalid
            -> re-walk (rows 已更新 cold) -> continue
         on success -> finish(app, resp, row.provider_id, row.api_key_id, is_stream)
       }
       AllCooling { retry_after } => return 429 + Retry-After（§6，step1 已实现接线）
       NoValid | Empty => return 429（无 Retry-After）
     }
   ```
5. `finish_*` / 流转译逻辑不动（已按 provider_id 读 live protocol，见
   `upstream_proto_of` L655）。

> **改动量评估**：`forward()` 的循环体重写（~50 行），其余（协议互译、流转译、finish）
> 复用。cooldown/prober/stats 全不动。

---

## 6. Terminal 429 + Retry-After（已接线，step1 完成）

- `proxy.rs` 两条 429 路径（L345 区、L450 区）已改用 `error_response_with_retry`，
  `Retry-After` = `app.min_retry_after_secs(&provider)`（min over 该 provider 的
  "冷却中且未死"key 的 remaining_secs；全死则不发头）。
- 策略层落地后，`min_retry_after_secs` 要泛化为"min over **候选集**（跨 provider）
  的冷却 valid 行"——签名从 `&Provider` 改为 `&[Row]` 或候选集 key 列表。step1 的单
  provider 版本 = both-ON 的特例，泛化是渐进的。
- spec §6 四态映射：
  | select 结果 | 返回 |
  |---|---|
  | `Routed` | 路由该行 |
  | `AllCooling { retry_after: Some }` | 429 + `Retry-After` |
  | `AllCooling { retry_after: None }` / `NoValid` | 429（无头） |
  | `Empty` | 429（无头） |

---

## 7. 配置 schema（`gateway.json`，渐进增字段，全 `#[serde(default)]`）

```jsonc
{
  "api_port": 8000, "ui_port": 8001,
  "default_cooldown_secs": 60, "max_attempts": 3,
  "auth": { /* … */ },
  "strategy": {                          // 新；缺省 = 默认 toggle 都 ON + 空 sort
    "filter": { "lock_model_group": true, "lock_provider": true },
    "sort": []                           // 例 ["I","J"]；0..=3，有序不重复
  },
  "model_groups": [                      // 新；可空（无组则单 provider 行为不变）
    { "id": "gpt-4o", "entries": [["OpenAI","gpt-4o"],["Azure","gpt-4o"]] }
  ],
  "providers": [{
    "id": "OpenAI", "name": "OpenAI", "base_url": "https://api.openai.com/v1",
    "protocol": "openai", "keys": [ /* … */ ], "models": [ /* … */ ],
    "aliases": {}, "model_allowlist_only": false,
    "has_token_balance_api": false,      // 新
    "has_bill_balance_api": false,       // 新
    "price_table": { "gpt-4o": 0.005 }   // 新
  }]
}
```
> 旧 `gateway.json` 无 `strategy`/`model_groups`/新 provider 字段 → `#[serde(default)]`
> 全部回落默认 = 当前行为。**无需迁移**。

---

## 8. UI / API 面（`admin.rs`，策略层落地后补）

- `GET /api/config` 已暴露 config（`admin.rs` ~L554），加 `strategy`/`model_groups`/
  provider 新字段即随序列化出现。
- 新增 `PUT /api/strategy`（写 `cfg.strategy`，校验 sort ∈ A-J、≤3、不重复）。
- 新增 `GET/PUT /api/model-groups`（组管理）。
- 策略 dry-run：`POST /api/strategy/dry-run {model, provider}` → 返回 select 结果
  （`Routed(row)/AllCooling/NoValid/Empty` + 候选集排序预览），供 UI 策略板演示
  （对应原型 `computeRoute()` + `resultBanner`）。
- **UI 严格按 `sakura-console-prototype.html` 策略板**（filter 两 toggle + 3 个 sort
  下拉 + dry-run 表 + result-banner）；加功能按 `sakura-ui-rules.md`。

---

## 9. 迁移与默认

- `Strategy::default()` = `{ filter: {lock_model_group: true, lock_provider: true},
  sort: [] }`。**这是向北兼容的落点**：D1 下 = 当前 round-robin。
- `model_groups` 空 = 无跨 provider 组；`resolve_model` 走老路（`provider/model` + alias），
  group_id = model 单成员。即"无组"时行为与现状一致。
- `price_table` 缺 = 0（免费）；只影响 sort I（便宜优先）在配置了价目时才真排序。
- metrics 未就绪前（step3 未完成），`metrics_of` 返回零值快照 → sort B-J 全平局，
  仅 A（成功率，可从 stats 导出）能动；D1 仍保 round-robin 兜底。**策略层可先于 step3
  上线**，metrics 是渐进增强。

---

## 10. 决策清单（需用户拍板，实现前定）

- **D1（推荐）** table-order = cursor 旋转后配置序 → both-ON+空 sort = round-robin
  （向北兼容）。备选 D2 = 纯配置序（恒从 index 0，破坏 round-robin 负载均衡）。
  → **采用 D1**。
- **D2** 模型组跨 provider 不同上游名（OpenAI `gpt-4o` vs 镜像 `gpt-4o-2024-08`）：
  `group.entries` 用 `(provider_id, upstream_model)` 承载，行存逻辑 `model`=group_id。
  → **采用**（spec 未写死，此为实现选择）。
- **D3** cursor 键：候选集标识 = `(group_id, resolved_provider_id_or_"any")`。
  both-ON/provider-优先 用 resolved_provider_id；model-优先/可用 优先 用 `"any"`
  （跨 provider 共享一个 cursor 做负载均衡）。→ **采用**，细节实现时定。
- **D4** `max_attempts`：现 = `max_attempts.clamp(1,10) * keys.len()`。策略层下候选集
  可能跨 provider，键数 = 候选集行数。保持 `* 候选集行数` 语义。→ **沿用**。
- **D5** metrics 滚动窗口径：success_rate/rpm/tpm 用 60s 滑窗还是累计？spec §2.2
  说"滚动窗"。→ 建议成功/失败用 5min 滑窗（成功率更稳），rpm/tpm 用 60s 滑窗。
  属 step3 范畴，此处仅定向。
- **D6** step1 的 `min_retry_after_secs` 策略层泛化时机：随策略层跨 provider 一起改
  （候选集行而非单 provider）。→ **策略层实现时同步泛化**。

---

## 11. 建议实现顺序（策略层本身，metrics 见 step3）

1. `config.rs`：加 `Strategy`/`ModelGroup`/Provider 新字段（全 default）+ 校验。
2. `strategy.rs`：`Row` + `build_candidates`（both-ON 候选 = 现 pick_key 的等价集）+
   `comparator` + `select`（含 D1 旋转）。
3. `proxy.rs::forward`：接入 `select`，抽 `build_upstream_payload`，循环体改写。
   先**只支持 both-ON**（候选 = 单 provider keys），验证 = 现 round-robin 行为不变
   （mock_upstream 回归）。
4. 解锁跨 provider：`build_candidates` 支持 model-优先/可用优先；`forward` 每 attempt
   按选中 provider 重建 payload/url。
5. `admin.rs`：strategy/model-groups API + dry-run 端点。
6. UI：策略板按原型接线（步骤4 的 UI 总活的一部分）。

> 每步可独立 build + `cargo test`（mock_upstream 回归）+ 手测。步骤 3 完成即达"向北
> 兼容铁律"里程碑：默认配置行为不变，但底座已是策略驱动，后续 toggle/sort 解锁零风险。
