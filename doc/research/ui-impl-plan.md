# Sakura 控制台 UI 实装计划（Phase 3b/c）

> 范围：把路由策略层（已发布的 admin API §11.5）接进**真实网关 WebUI**，
> 并落地原型的新功能（双卡添加提供商）。本计划供审阅，确认后再动手。
>
> 上游：`doc/research/strategy-impl-spec.md` §8/§11.5-6；原型
> `D:\Test\sakura-console-prototype.html`；UI 规则 `D:\sakura-ui-rules.md`。

---

## 1. 现状对照

| | 真实 WebUI `llm-gateway/static/index.html` | 原型 `D:\Test\sakura-console-prototype.html` |
|---|---|---|
| 体量 | 1119 行 / 55 KB，**单文件 + 内联 `<style>`** | 863 行 HTML + 3 CSS + 10 woff2 + design-tokens.json |
| 布局 | **顶栏 Tab**（Status / Config 两页） | **侧栏 + 顶栏**（多板块导航） |
| 设计系统 | 自有内联 CSS（`.card`/`.section-title`/`.stat-box` 等） | **token 系统**（design-tokens.json → sakura-theme.css；sakura-ui.css 组件层；自托管 Inter/JetBrains/Sarasa 字面） |
| i18n | `I18N` 字典 + `t()` + `data-i18n`（zh/en，已落地，有 中/EN 切换） | 同套 `t()`/`data-i18n`（zh/en，已落地） |
| 数据 | **接真实 API**：`api(method,url,body)` helper；`refresh()`→`/api/status`；`/api/stats`；`/api/free-catalog`；`/api/providers` CRUD | **sample 数据**：`PROVIDERS`/`MODELS`/`KEYS`/`CATALOG` 内置数组；`computeRoute()` 本地模拟，不接 API |
| 已有面板 | status（统计+直方图+providers/keys/models 三表）、config（导入导出+全局设置+免费目录+**简单 add-provider 表单**+提供商列表） | overview、providers 板（列表+详情）、models（信息列表+编辑+**模型组管理 subtab**）、api-keys 全局表、**策略板**、**双卡 add-provider**（自定义 / 从目录） |
| **缺什么** | **策略面板、模型组编辑器**；add-provider 是简陋单表单（非双卡） | 全有，但跑在 sample 数据上 |

**关键事实**：真实 WebUI 已是接真实 API 的成品（含 i18n/主题/可达性），只是设计不如原型精致；原型是设计参考（token 系统 + 富面板），但跑 sample 数据。

用户铁律（之前对话）：「网关的 UI 要严格按照我给你的原型，日后加功能也要严格按照设计规范」。

---

## 2. 路径决策（需你拍板）

### Path A — 全量迁移原型进网关（= 严格照原型）
把原型的 HTML + 3 CSS + fonts + tokens 搬进 `static/`，**重写每个面板**从 sample 数据 → 真实 admin API。

- **优点**：完全复刻原型设计（token/字面/侧栏布局/富面板），满足「严格照原型」。
- **代价**：
  - 体量：fonts ~4.5 MB（Sarasa Gothic SC regular/semibold/bold 各 ~1.5 MB）会被 `rust_embed` **编进二进制**（含 Android 交叉构建）→ 二进制膨胀 ~4.5 MB。
  - 工作量：6+ 面板（providers/models/keys/status/strategy/groups/add-provider/catalog）从 sample→API 重接，每面板都要改数据加载 + 渲染。中-大。
  - `ui.rs` 要加 `woff2`/`woff2` mime（现 fallback 到 octet-stream），并验证 `rust_embed` 编入 fonts 不爆构建。
- **风险**：半途态——若分阶段，迁移中部分面板接真实、部分还跑 sample，UI 割裂。建议**一次性整块迁移**（shell + 全面板），但这是大活。

### Path B — 增量进现有 WebUI（快，但不复刻原型设计）
在 `static/index.html` 里**新增**策略面板 + 模型组编辑器 + 双卡 add-provider，沿用其 Tab/内联 CSS/`t()` i18n 约定，接刚发布的 admin API。

- **优点**：快（只加 3 块，沿用现有 `api()`/`data-i18n`/CSS 约定）；零二进制膨胀；与现有已发布 UI 风格一致；策略面板立刻可用。
- **代价**：**设计 ≠ 原型**（Tab vs 侧栏；内联 CSS vs token 系统；不复刻 Sarasa 字面分流）。视觉上不如原型精致。
- 适合「先把策略层 UI 跑起来可用」，把「全量复刻原型」留作后续独立里程碑。

### 推荐
若「严格照原型」是硬约束 → **Path A**（接受体量代价，一次性迁移）。
若「先让策略层在真实网关里可用」优先 → **Path B**（快、稳、一致），全量复刻原型作为后续独立任务。

> 我的倾向：**Path B 先行**——刚发布的 admin API 急需一个真实 UI 验证其可用性；策略面板 + 模型组 + 双卡 add-provider 三块在现有 WebUI 里增量落地，1 轮可交付且自洽。Path A（全量复刻 token 设计 + 自托管字面）是独立的、更大的视觉工程，值得单独立项、单独审设计稿，不该和「策略层 UI 接线」混在一起。

---

## 3. Path B 详细步骤（若选 B）

全部在 `llm-gateway/static/index.html` 内增量改；沿用现有 `api()` helper、`data-i18n`/`t()`、`.card`/`.section-title`/`.stat-box`/`table` 等 class、zh/en 字典。

### 3.1 策略面板（§11.6 核心）
- **位置**：Config 页新增一个 `<div class="section">`（在「全局设置」与「现有提供商」之间），或在 Status/Config 之外加第三个 Tab「策略」。倾向**加 Tab**（策略是独立关注点，与配置并列）。
- **结构**（照原型 `renderStrategy`）：
  - 过滤器：2 个 toggle（`lock_model_group` / `lock_provider`）+ 当前模式文案（精确轮换/模型优先/提供商优先/可用优先）。
  - 优先策略：3 个 `<select>`（sorts[0..2]，选项 A–J + 跳过），照 spec §5.2 的 A-J 语义标注。
  - 演示：`模型` + `提供商`(可选) 两个 `<select>` → 调 `api('POST','/api/strategy/dry-run',{model,provider,sorts})` → 渲染 `result-banner`（路由命中 / 429 全冷却+Retry-After / 429 无可用 / 候选为空）+ 候选表（winner 行 `.row-winner`，列：#/Key/提供商/模型/valid/cool/单价/TPS/成功率）。
- **保存**：`api('PUT','/api/strategy',{filter:{...},sort:[...]})`；切换 toggle/option 即 PUT（或「保存策略」按钮）。
- **加载**：`refresh()` 里加 `STRATEGY = await api('GET','/api/strategy')`（从 `/api/config`=list_providers 已含 `strategy`）；填 toggle/select 初值。
- **i18n**：`I18N.zh`/`en` 各加 ~15 个 key（策略管理/过滤器/不改变模型组/不改变提供商/优先策略/关键字 {n}/（跳过）/演示/路由命中/429 全冷却/429 无可用/候选为空/精确轮换/模型优先/提供商优先/可用优先/…）。

### 3.2 模型组编辑器
- **位置**：策略 Tab 内子区，或 Config 页「提供商」之后。
- **结构**：列出 `model_groups`（id + entries 表：provider/upstream_model，可增删行）+ 「新增组」+ 「保存组表」`api('PUT','/api/model-groups',{groups})`。
- **加载**：从 `/api/config` 取 `model_groups`；provider 下拉从 `/api/status` 的 providers 填。
- **i18n**：模型组管理/新增组/逻辑模型/上游模型/… ~6 key。

### 3.3 双卡添加提供商（新功能）
- 现有：简陋单表单（name/base_url/protocol + 添加）。升级为原型的双卡：
  - 卡 1「自定义提供商」：手填 name/base_url/协议/托管模型(可增删)/API Key → `api('POST','/api/providers',{name,base_url,protocol})` 建 provider，再 `api('POST','/api/providers/{id}/keys',{key})` + `api('POST','/api/providers/{id}/models',{id})` 逐个加模型。
  - 卡 2「从免费目录选择」：渲染 `CATALOG`（`/api/free-catalog`）为模型卡网格，点一张预填表单 → 同上 POST。
- 现有 `renderCatalog()` + 简单 add 表单可合并/重构为双卡。
- **i18n**：自定义提供商/已有的提供商/从免费目录选择/托管模型/添加模型 id/粘贴 API Key/… ~8 key（部分已存在）。

### 3.4 验证
- `cargo build`（确认 `static/index.html` 改动编进 rust_embed）+ `cargo test`（31 不回归）。
- 起 gateway（test/gateway.json + mock_upstream），浏览器开 UI：
  - 策略 Tab：改 toggle/sort → PUT 生效；dry-run 表显示真实候选 + winner；result-banner 四态。
  - 模型组：增删 entry → PUT 生效；`/v1/chat/completions` 裸模型走跨 provider。
  - 双卡 add：自定义建 provider+加 key+加模型；从目录选一张预填+建。
  - zh/en 切换新 key 全有；浅/深主题；窄宽不溢出。

---

## 4. Path A 详细步骤（若选 A）

### 4.1 资产搬迁
- 复制 `D:\Test\{sakura-fonts.css, sakura-theme.css, sakura-ui.css, design-tokens.json}` + `D:\Test\fonts\*.woff2`（10 个）→ `llm-gateway/static/`（建 `static/fonts/`）。
- 改 prototype HTML 的 `<link href="sakura-fonts.css">` → `/static/sakura-fonts.css`（gateway `ui::asset` 路由 `/static/{name}`）。
- `ui.rs::serve_file` 加 `woff2`→`font/woff2`、`json`→`application/json` mime（现 fallback octet-stream）；`rust_embed` 自动编入 `static/` 全部文件。
- **体量确认**：二进制 +~4.5 MB（fonts）；Android aarch64/armv7/x86_64 三目标各 +4.5 MB。需你接受。

### 4.2 shell + 全面板重接
- 把 prototype 的 topbar/sidebar/main 骨架搬进 `static/index.html`，保留其 `t()`/theme/lang。
- **逐面板 sample→API 重接**：
  - overview → `/api/stats` + `/api/status`。
  - providers 板 → `/api/status`(providers+keys+pool) + `/api/providers` CRUD。
  - models（信息列表 + 编辑） → `/api/status`(models) + `/api/providers/{id}/models` CRUD + 上游拉取 `/api/providers/{id}/upstream-models`。
  - api-keys 全局表 → `/api/status`(keys+cooling+invalid+requests)。
  - **策略板** → `PUT /api/strategy` + `POST /api/strategy/dry-run`（computeRoute 替换为真实 dry-run）。
  - **模型组** → `GET/PUT /api/model-groups`。
  - **双卡 add-provider** → `/api/free-catalog` + `/api/providers` + keys/models CRUD。
- 这是大活，建议**一次性整块**迁移（避免半途割裂），分提交：资产搬迁 → shell → 逐面板（每面板一提交，cargo build + 浏览器测）。

### 4.3 验证
- 同 3.4，外加：`scripts/` 门（check_no_emoji/lint_hardcodes/validate_theme_refs）若可移植则跑（D:\Test 的脚本依赖 Playwright + Chrome，gateway 侧未必装；至少人工核令牌一致）。
- 字面分流：中文走 Sarasa、拉丁走 Inter（靠 `unicode-range`）肉眼核。

---

## 5. 待你拍板

1. **Path A（全量复刻原型，二进制 +~4.5 MB，大活）还是 Path B（增量进现有 WebUI，快、自洽，设计≠原型）？**
2. 若 B：策略面板放**新 Tab「策略」**还是 Config 页内嵌 section？
3. add-provider 双卡是**替换**现有简陋表单，还是**并存**？
