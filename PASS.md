# Sakura LLM Gateway — 开发交接文档（PASS.md）

> 本文档面向**下一位负责维护/继续开发本项目的 AI 或开发者**。
> 请先完整阅读本文，再修改代码或启动服务。它记录的是本仓库的真实现状，
> 不是理想设计——文档与代码冲突时，以代码为准，并回来更新本文。

---

## 1. 项目身份与仓库

- **项目名称**：Sakura LLM Gateway
- **仓库地址**：`https://github.com/sloxphrite73/sakura-llmgateway`（公开，GPL-3.0）
- **一句话定位**：本地高性能 LLM API 网关（Rust / axum + tokio），对外暴露
  OpenAI 兼容接口（`127.0.0.1:8000/v1`），对每个上游提供商维护一个 API-Key 池，
  遇到 429 自动换 Key 重试，并提供 Web 控制台（`127.0.0.1:8001`）管理一切。
- **技术栈**：Rust（edition 2021）、axum 0.8、tokio、hyper 1、reqwest 0.12
  （rustls-tls，无 native-tls）、serde/serde_json、futures-util、uuid、rust-embed。
  无 Node 构建链，Web UI 是单文件 HTML/JS 内嵌进二进制。
- **许可证**：GPL-3.0（`LICENSE`）。

### ⚠️ 环境说明：机器上存在两份工作副本（2026-09-11 起不再要求同步）

| 路径 | 角色 |
|---|---|
| `C:\Users\Administrator\Documents\GitHub\sakura-llmgateway` | **唯一权威仓库**（工作目录，已推送 GitHub） |
| `D:\LLMGATEWAY` | 早期工作副本，**已弃用**——用户明确指示不再同步（2026-09-11） |

历史教训：曾发生“编辑了 D: 源码，却在 C: 上构建测试”，导致“修好的 bug 反复复现”，
浪费了大量时间。现行纪律：

1. 所有修改、构建、提交一律以 **C: 仓库**为准。
2. **不再向 D: 同步**（用户 2026-09-11 指示）；D: 仅作历史遗存，
   不要读取其代码作为现状依据。
3. **绝对不要把 D:\LLMGATEWAY\llm-gateway\test\gateway.json 里的真实 API Key
   同步进 C: 仓库**（见第 9 节敏感信息）。

> 2026-09-11 核对：C: 领先 D: 一个特性（401/403 无效隔离 + UI 浅色主题，
> 提交 172bbfc / 6bf0f5f）。D: 的 src/ 与 static/ 均为旧版。

---

## 2. 快速上手

### 构建

```bash
cd llm-gateway
cargo build --release      # 产物: target/release/llm-gateway.exe
```

- Windows 上本机是 **Rust GNU 工具链**，需要 MinGW 的 `dlltool`/`gcc` 在 PATH 上。
  便携版 MinGW 已装在 `%USERPROFILE%\.mingw64`（由 install.bat 自动下载安装）。
  手动构建前先 `export PATH=~/.cargo/bin:$PATH:/tmp/mingw/mingw64/bin:$USERPROFILE/.mingw64/bin`
  （`/tmp/mingw` 是历史遗留路径，`%USERPROFILE%\.mingw64` 是脚本用的正式位置）。
- 二进制内的 Web UI 用 `rust-embed` 编译期内嵌——**改 `static/index.html` 后必须重新
  `cargo build`（debug 也行）才能生效**，只刷新浏览器没用。
- **发版走 GitHub Actions**（2026-09-11 起）：推 `v*` tag（如 `v0.1.0`）即自动构建
  Windows release 二进制并附到 Release，详见第 13 节。

### 运行

```bash
./target/release/llm-gateway.exe                          # 配置默认 ./gateway.json
./target/release/llm-gateway.exe --config test/gateway.json
```

- 启动打印：UI 地址、API 地址、鉴权开关。
- `start.bat` / `start.sh`：日常启动器，无产物时自动构建，前台运行（关窗即停）。
- `install.bat` / `install.sh`：一次性安装（装 Rust、下 MinGW、release 构建、生成默认配置）。
- `.bat` 全部为**纯 ASCII**——中文 Windows 的 cmd 用 GBK 解析 UTF-8 会闪退，
  这是踩过的坑，往 .bat 里加中文前先想清楚。

### 端口

| 服务 | 默认 | 说明 |
|---|---|---|
| OpenAI 兼容 API | `127.0.0.1:8000/v1` | `POST /v1/chat/completions`、`GET /v1/models` |
| Web 控制台 + 管理 API | `127.0.0.1:8001` | 页面 `/`，REST `/api/*` |

端口可在控制台设置或手改 `gateway.json`，**重启生效**。

---

## 3. 代码结构（按文件）

```
llm-gateway/
  Cargo.toml            依赖；default-run = "llm-gateway"（cargo run 不再问用哪个 bin）
  install.bat/.sh       一次性安装脚本（纯 ASCII / bash）
  start.bat/.sh         日常启动脚本
  gateway.json          本地真实配置（.gitignore 忽略，含真实 Key，绝不提交）
  test/gateway.json     测试配置（占位 Key；383fe7c 曾从仓库删除后于 2026-09-11 重建，
                        并已加入 .gitignore——本地测试若放入真实 Key 也不会再进 git）
  src/
    main.rs             入口：加载配置、建 App、注册两套路由（API / admin+UI）、绑定端口、
                        启动两个后台 flusher（stats.json 每 5s；gateway.json 学习冷却 10s 去抖）
    config.rs           Config/Provider/ApiKey/ManagedModel/AuthSettings + 原子持久化 + 模型解析
                        （ApiKey 含 learned_cooldown；Config::parse_str 供导入校验）
    state.rs            App 共享状态 + Key 池运行时（轮询游标、冷却、无效隔离、计数、状态快照）
                        + 统计互斥锁 + 学习冷却去抖写入（mark_config_dirty / flush_config_if_due）
                        + 探测去重（claim_probe / release_probe）
    stats.rs            【2026-09-11 新增】请求统计：total/hourly(24h)/keys/providers/models
                        五组成功失败计数 + rotations（每提供商被换出 Key 次数，2026-09-12 新增）；
                        stats.json 与配置同目录（gitignored）
    proxy.rs            /v1 代理：鉴权、模型解析、Key 轮换重试循环、流式透传与中断处理、
                        逐 Key 失败计数、非流式全量缓冲计数、/v1/models 聚合、
                        spawn_prober（429 后台二分探测学习冷却，2026-09-12 重写）
    admin.rs            管理 REST API（控制台背后全部端点，含 /api/stats、/api/config/import|export）
    ui.rs               rust-embed 内嵌 static/ 的入口
    bin/mock_upstream.rs 测试用假上游（127.0.0.1:9001；sk-bad 恒 429、sk-dead 恒 401、
                        sk-slow 前 8 秒 429 之后正常——用于验证冷却学习）
  static/index.html     单文件 Web 控制台（樱花主题、深/浅色、实时倒计时、
                        Status/Config 双页签、24h 柱状图、Key 显示/复制、配置导入导出）
  ../.github/workflows/release.yml  GitHub Actions 发版工作流（v* tag 触发，见第 13 节）
  ../README.md          英文 README（2026-09-11 按 SAMPLE_README 风格重写）
  ../README.zh-CN.md    中文 README（与英文逐节同步）
```

### 模块职责与关键约定

- **config.rs**：`Config::load` 在文件不存在时**自动生成默认配置并保存**；
  `save` 是“写 tmp 再 rename”的原子写。**每次控制台修改都会全量重写该文件**——
  手改会被覆盖，这是设计而非 bug。
  `Provider::model_allowed`：白名单关 → 放行；开但列表空 → 放行（向后兼容）；
  开且有列表 → 只放行列表中 enabled 的模型。
  `resolve_model`：先全局查别名（别名优先），再匹配 `provider/model`（provider 可
  用 id 或 name）；未命中返回 `unknown model` 错误串。
- **state.rs**：`PoolRuntime` 全部字段是 `HashMap`，用 `Mutex` 保护。
  - `cooling`：临时冷却（429/5xx/网络错），秒级。
  - `invalid`：**401/403 的 30 分钟无效隔离**（“key 本身死了，重试不可能成功”）。
  - `last_error`、`requests`：给 UI 展示。
  - `pick_key`：轮询调度，跳过 cooling/invalid，命中后游标前移并 `requests+1`；
    全池冷却返回 `None`。**注意它会 retain 掉过期条目**。
  - `mark_cooldown`：冷却时长优先级 = `Retry-After` → learned_cooldown（探测学习）→ 每 Key 覆盖 → 全局默认，最少 1 秒。
  - `mark_invalid(secs=1800)`：只写 invalid + last_error。
  - `status()`：**同样会先清理过期条目**（这是修过的一个 bug：以前只有 pick_key 清理，
    UI 轮询 status 时永远看到“冷却 · 剩余 0s”）。返回 `cooling` / `invalid` / `requests` /
    `last_error` 四组映射，`until_ms` 用 `now_ms + remaining*1000` 计算。
- **proxy.rs**（核心，改动需谨慎）：
  - 重试预算 = `max_attempts(clamp 1..10) × keys.len()`——**按 Key 数缩放**，
    保证“还有可用 Key 就必须试到”，这是修过的 Bug A。
  - 循环内 `pick_key` 返回 `None` → 立即向客户端返回 429（附 Key 列表，快速失败）。
  - 429：`mark_cooldown`（尊重 Retry-After）+ **sleep 1500ms 再试下一个 Key**
    （防 TPM 连环引爆，修过的坑）+ **spawn_prober 后台探测**（见下）。
  - 401/403：`mark_invalid(30min)`，不再短冷却反复喂（修过的坑）。
  - 408/5xx：`mark_cooldown(5s)`；网络错：`mark_cooldown(5s)`。
  - **400 等请求级错误立即返回**（换 Key 结果一样，别浪费池子）。
  - **逐 Key 统计**：被轮换出去的 Key（429/401/403/408/5xx）都会 record_key_stat(false)
    计入该 Key 自己的失败数；客户端级请求只由最终服务它的 Key 计一次（record_stat）。
  - **finish_ok（2026-09-11 重构）**：非流式响应全量缓冲后再计数/返回——上游中途断连
    计为失败，绝不把截断 body 当成功发给客户端；流式响应在 stream error 时追加
    OpenAI 风格 SSE 错误块 + data: [DONE]（客户端干净收尾，不重复内容；已发出部分
    内容后不重试）。
  - **spawn_prober（智能冷却学习）**：Key 被 429 后，后台按 5s→10s→…→300s 间隔发
    1-token 探测请求（max_tokens=1），成功时的等待秒数写入该 Key 的 learned_cooldown。
    claim_probe 去重（每 Key 同时仅一个探测器）、12 轮上限、401/403/其他错误视为
    不可学习直接退出。写入走 10 秒去抖（mark_config_dirty + main.rs flusher 任务），
    429 风暴不会高频重写 gateway.json。
  - 循环尾部兜底：预算耗尽但没 return 时返回 429（曾用 `unreachable!()`，会 panic，
    已换成安全快速失败）。
  - `check_auth`：开启鉴权后要求 `Authorization: Bearer <gateway key>`。
  - `list_models`：聚合各提供商 enabled 托管模型（`provider/model`）+ 有效别名；
    未托管提供商（models 空）不贡献条目；悬空别名不广告。
- **admin.rs**：纯 REST，无鉴权（**控制台/管理 API 完全裸奔**，只绑 127.0.0.1，
  见第 8 节已知问题）。Key 值在 status 里永远 `mask`（前4后4 + 星号）。
- **ui.rs**：`#[derive(RustEmbed)] #[folder = "static/"]`。

---

## 4. 配置格式（gateway.json）

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
        { "id": "k-xxx", "key": "sk-...", "label": "main", "cooldown_secs": null,
          "learned_cooldown": 90 }   // 429 探测自动学习，机器写入，勿手改
      ],
      "models": [ { "id": "gpt-4o", "enabled": true } ],
      "model_allowlist_only": false,
      "aliases": { "fast": "gpt-4o-mini" }
    }
  ]
}
```

字段语义详见 `doc/USAGE.md` 与 `doc/使用说明.md`（第 7 节表格）。要点：

- `max_attempts` 是**重试预算倍数**而非总次数上限：实际预算 = `× Key 数`。
- `models` 空 = 未托管，全部放行（向后兼容老配置）。
- 控制台任何修改都会原子重写文件；运行中手改文件 OK，但下次控制台改动会覆盖。
- `learned_cooldown`（2026-09-11 新增）：429 后台探测学到的真实限流窗口，
  机器写入 + 10 秒去抖持久化；冷却解析时优先于 `cooldown_secs`。UI 的 Config 页
  以 pill 展示，与用户手设值区分。

---

## 5. 管理 REST API（http://127.0.0.1:8001）

| 方法与路径 | 用途 |
|---|---|
| `GET /api/status` | 完整快照：设置 + 每提供商 Key（掩码）及 cooling/invalid/requests/last_error |
| `GET/POST /api/providers` | 列出 / 创建提供商 |
| `PUT/DELETE /api/providers/{id}` | 更新 / 删除提供商 |
| `POST /api/providers/{id}/keys` · `DELETE /api/providers/{id}/keys/{key_id}` | 加 / 删 Key |
| `DELETE /api/keys/{key_id}/cooldown` | 清除冷却**或**无效隔离（UI 的“解除无效”就是它） |
| `POST /api/providers/{id}/models` · `DELETE/PUT .../models/{model_id}` | 加 / 删 / 启停模型 |
| `GET /api/providers/{id}/upstream-models` | 拉取上游实时 `/v1/models`（用第一个 Key） |
| `POST /api/providers/{id}/models/import` | 批量导入 `{ "models": ["a","b"] }`（幂等） |
| `POST /api/providers/{id}/aliases` · `DELETE .../aliases/{alias}` | 设置 / 删除别名 |
| `PUT /api/settings` | 更新全局设置（端口重启生效） |
| `GET /api/stats` | 统计：total / 24h histogram / keys / providers / models 成功失败计数 |
| `GET /api/config/export` | 导出 gateway.json（浏览器下载，attachment） |
| `POST /api/config/import` | 导入配置热生效：`{ "content": "<gateway.json 原文>" }`，
  validate-then-swap（无效整体拒绝），含端口鉴权全量替换，无需重启 |

路由定义集中在 `main.rs` 的 `admin` Router 里。UI 就是这些端点的前端。

---

## 6. Web 控制台（static/index.html）

- **单文件**，约 1000+ 行，原生 JS + CSS 变量（token 化），樱花主题：
  深色基底 `#0d0e13`，点缀粉 `#f2a6b8`；浅色（`data-theme="light"`）加深粉 `#c95d7c`。
- 页面结构（2026-09-11 改版）：masthead（品牌 + 主题切换 + **Status/Config 双页签**）。
  - **Status 页**：总成功/失败/成功率卡片、24h 请求柱状图（成功绿/失败红堆叠，
    每 5s 轮询 /api/stats）、供应商/Key/模型三张成功失败表。
  - **Config 页**：配置文件导入导出按钮 + 全局设置区 + 每提供商卡片
    （信息 + Key 表格 + 模型管理 + 别名）——原单页全部内容迁到这里。
- **Key 状态三色**：🟢 可用 / 🟡 冷却（倒计时）/ 🔴 无效（30 分钟隔离，可“解除无效”）。
- **实时倒计时**：服务端 `status()` 下发 `until_ms`（绝对时间戳），前端每秒本地递减；
  到 0 自动翻绿并触发一次刷新。**改过这个逻辑**——原来 5 秒轮询且不清理过期，
  导致“卡在 0s”。不要再退回轮询式。
- **Key 显示/复制**（2026-09-11 新增）：每 Key 一行配「显示」「复制」按钮。
  status 接口只发掩码；显示/复制时前端临时 GET /api/config/export 取真实值
  （同一 127.0.0.1 信任域）。「显示」切换明文/掩码，「复制」写剪贴板后回显已复制。
- 主题选择存 `localStorage`，默认深色。
- `static/` 下只有 `index.html`。改完必须重新构建二进制（见第 2 节）。

---

## 7. 测试与验证方法

### mock 上游

```bash
cargo run --bin mock_upstream          # 127.0.0.1:9001
cargo run -- --config test/gateway.json
```

> 2026-09-11 注：仓库内的 `test/gateway.json` 曾在 383fe7c 删除，现已重建
> （占位 Key：sk-bad / sk-bad2 / sk-good，仅 mock 提供商）。该文件已加入 .gitignore，
> 本地测试时若临时放入真实 Key 也不会进 git，但仍建议保持占位符。
> CI 的冒烟测试**不使用本文件**——它在工作流里临时生成 `ci-gateway.json`
> （test/ 目录在 CI checkout 上不存在，见第 13 节）。

`src/bin/mock_upstream.rs` 语义：
- `sk-bad` → 恒定 429 + `Retry-After: 2`；
- `sk-dead` → 恒定 401；
- `sk-slow` → 该 Key 首次请求后 8 秒内恒 429，之后正常（模拟限流窗口，
  验证二分探测能否学到 learned_cooldown ≈ 8s）；
- 其他 Key → 正常返回流式/非流式响应。

### 端到端验证套路（本机实测过）

1. 起 mock（9001）+ 网关（测试配置）。
2. Bug A（还有好 Key 却 429）：配置 keys `[bad, dead, good]`、`max_attempts: 2`，
   单发一次请求 → 期望客户端 **200**，bad/dead 进冷却/隔离，good 无 last_error。
   （注：现重建的测试配置没有 sk-dead——需要验证 401 隔离时，手动把某个 Key 改成
   `sk-dead` 即可，mock 会返回 401。）
3. Bug B（冷却自愈）：429 的 Key（Retry-After 2s）到期 2.5s 后查 `/api/status` →
   期望 cooling 为空。
4. 全坏 Key → 期望快速 429（fail-fast 路径）。
5. 401 检疫：`sk-dead` 请求后 status 应显示 `invalid.remaining ≈ 1770s`，
   连发请求其 `requests` 计数不再增长。
6. 清场：`taskkill //F //IM llm-gateway.exe //IM mock_upstream.exe`（Windows）。

> 无自动化测试套件。**改 proxy/state 后请按上述手动流程验证**，这是项目现状。

---

## 8. 已知问题与坑（按重要性排序）

1. **管理 API 无鉴权**：`/api/*` 和 Web 控制台没有任何认证（`auth` 只作用于
   `/v1/*`）。只绑定 127.0.0.1，单机场景可接受，但任何“暴露到局域网/公网”的
   需求都必须先解决这个。
2. **Key 值明文落盘**：`gateway.json` 明文保存真实 Key（.gitignore 已忽略该文件）。
   无加密、无混淆。D: 工作副本的 `test/gateway.json` 也含真实 Key，**已明确警告勿提交**。
   另：真实 Key 曾随 test/gateway.json 进入公开仓库历史（383fe7c 已删文件，历史未清，
   见第 9 节顶部）。
3. **双工作副本**（第 1 节）：D: 副本已弃用、不再同步；一切以 C: 为准。
4. **UI 改动需重新编译**（rust-embed 编译期内嵌），容易忘。
5. **计数器是进程内内存计数**：重启清零，不代表真实请求总量。
   曾观察到“用量虚高”——主因是死 Key 反复被喂 + 429 连环，已用
   401 隔离 + 1.5s 退避缓解。
6. **`pick_key`/`status` 的 `until_ms` 是估算值**（剩余秒×1000 + now），
   跨秒边界 UI 倒计时可能有 1s 抖动，属可接受。
7. **`should_rotate` 把 401/403 也列进去了**，配合 `mark_invalid` 使用；
   若未来改这块逻辑，注意 401/403 与 429 的处置差异（隔离 vs 短冷却）。
8. **`.bat` 编码坑**：必须纯 ASCII（GBK 解析问题）；`if ( ... )` 块内 echo
   不要出现未转义的括号。
9. **端口冲突**：旧网关实例占着 8000/8001 时新实例起不来，先 `taskkill //F //IM llm-gateway.exe`。
10. **测试配置的 `max_attempts` 曾为 2**（重建的 test/gateway.json 保持 2 以复现
    Bug A 场景），与默认 3 不同——测试时注意区分，别把测试配置当默认。
11. **统计语义（2026-09-12 定版）**：total/provider 的 fail 只计**客户端可见的
    整条请求失败**；被换出的 Key 记 per-key fail + rotations 计数器（Status 页
    "换 Key 次数"卡片 + 供应商表"换Key"列）。用户曾误以为"全是 0 是 bug"——
    是语义 + 旧二进制双重原因；语义保留，新增 rotations 让吸收掉的故障可见。
12. **探测学习（2026-09-12 重写为二分法）**：初始 T = learned_cooldown（无则全局
    默认）；等 T 后探测——成功 → 区间 [T/2, T]，429 → 区间 [T, 2T]；再二分 5 轮
    取中点写入 learned_cooldown。上限 10 次探测/次测量；1 小时内的测量不重学。
    探测请求用 provider 第一个 enabled 托管模型（旧版用 "probe" 模型名，真实上游
    返回 400 导致永远学不到——已修的 Bug）。
13. **不要在默认配置里塞占位供应商（2026-09-14 修，v0.2.5）**：安卓端「跳过导入」
    生成的 DefaultConfig.kt 曾硬编码一个 sensenova 占位 provider——当时误以为
    Rust 解析器要求 providers 非空。实际上 `Config` 是 `#[serde(default)]`，
    `providers: []` 是一等公民状态（桌面端配置文件缺失时生成的就是它）。
    结果：全新安装跳过导入后控制台凭空显示 Sensenova。教训：**跨语言生成配置
    前，先用真实解析器验证空/边界状态是否合法，别靠假设**；Kotlin 侧硬编码的
    schema 假设要用真实 Rust 二进制做冒烟验证。已验证空 providers 能正常启动
    并返回 200（commit 6c797e7）。

---

## 9. 敏感信息与安全红线（对下一位 AI 的硬约束）

> ### 🚨 2026-09-11 排查结论：真实 Key 已进入公开仓库的 git 历史（文件已删，历史未清）
>
> - 引入：8ea052a 首次提交即携带含 **15 个 sensenova 真实 Key** 的 `test/gateway.json`，
>   直至 b4a8bd1 的每个提交树里都有它。383fe7c 删除了该文件并已推 GitHub——**但任何人
>   仍可从历史还原**：`git show 3ac2ca0:llm-gateway/test/gateway.json`。
> - 含 Key 副本现状：`D:\LLMGATEWAY\llm-gateway\test\gateway.json`（唯一遗存副本）；
>   C: 的 `llm-gateway/gateway.json`（.gitignore 已拦，安全）。
> - 本文原判「仓库内是占位符」**有误**——那是只看 tip 不看历史得出的结论，已在此更正。
> - 处置决定（用户 2026-09-11）：**暂不处理**——用户判断外部人士不知道曾上传过真实 Key，
>   风险可接受。不做 git 历史重写；保留本记录供未来重新评估。
> - 如未来改变主意：先在 sensenova 后台吊销/轮换这 15 个 Key（唯一彻底缓解），
>   再考虑 git filter-repo + force push（须用户明确授权）。

- ❌ 绝不提交 `llm-gateway/gateway.json`（.gitignore 已拦，但手工 `git add -f` 会绕过）。
- ❌ 绝不把 D: 的 `test/gateway.json`（含真实 Key）同步进 C: 仓库或推 GitHub。
- ✅ `test/gateway.json` 现已加入 .gitignore——本地测试临时放真实 Key 也不会进 git，
  但提交前仍照常检查 `git diff` 里没有 `sk-` / `Bearer` 之类的真实凭据。
- ✅ 回复用户时 Key 值保持掩码习惯（前 4 后 4）。
- 仓库是**公开**的——README、文档里只放占位符。



---

## 11. 常见开发/排障命令速查

```bash
# 构建（Windows GNU 工具链需 MinGW 在 PATH）
export PATH=~/.cargo/bin:$PATH:$USERPROFILE/.mingw64/bin
cd llm-gateway && cargo build --release

# 起 mock + 网关
./target/debug/mock_upstream.exe &
./target/debug/llm-gateway.exe --config test/gateway.json &

# 冒烟
curl -s http://127.0.0.1:8000/v1/models | head -c 500
curl -s http://127.0.0.1:8001/api/status | head -c 800

# 杀进程（Windows）
taskkill //F //IM llm-gateway.exe //IM mock_upstream.exe
```

---

## 12. 给下一位 AI 的开发纪律

1. **先读本文，再动手**；改 `proxy.rs`/`state.rs` 后按第 7 节手动验证。
2. **工作目录用 C: 仓库**（D: 副本已弃用，2026-09-11 起不再同步）；
   绝不提交任何含真实 Key 的文件。
3. 提交粒度：一次一个主题；commit message 描述“为什么”而非“改了什么”。
4. 之前约定的提交/推送习惯：用户在对话中明确要求时才推送；`gateway.json`
   永远不进 git。
5. 用户偏好：中文回复（2026-09-11 起会话语言为英文，按用户所用语言回复）。
6. 更新本文档：凡是改变架构、配置 schema、API 路由、Key 池语义的改动，
   顺手更新 PASS.md 与 `doc/` 双语文档。

---

## 13. 发版流程（GitHub Actions，2026-09-11 建立）

`.github/workflows/release.yml`：推 `v*` tag 触发（也支持 workflow_dispatch），在
`windows-latest` 上 `cargo build --release` → 冒烟测试 → 产物重命名为
`sakura-llmgateway-<tag>-x86_64-pc-windows-msvc.exe` → softprops/action-gh-release
创建 Release 并附二进制。已验证：v0.1.0 发布成功（4.4 MB 单文件）。

v0.2.0（2026-09-13）— Android 首发，历经 4 轮 CI 修复：
- 产物同 Release：sakura-llmgateway-v0.2.0-universal.apk（11.3MB）+ exe（4.4MB）。
- 修复轮次：①workflow 缺 NDK 安装步骤（android-actions/setup-android 不带 NDK，
  需 sdkmanager --install + 导出 ANDROID_NDK_LATEST_HOME 给 cargo-ndk）；
  ②cargo-ndk 4.x 的 -p 是 --package，平台参数是大写 -P，且需 --manifest-path
  （本地用 NDK r26d 完整验证了该命令三架构编译通过）；
  ③仓库没有提交 gradlew wrapper，CI 改用 gradle/actions/setup-gradle 装 Gradle 8.9
  直接调 gradle；④android/ 缺 gradle.properties 的 android.useAndroidX=true，
  checkReleaseAarMetadata 拒绝 AndroidX 依赖。
- 顺带修的真 bug：MainActivity 必须继承 androidx ComponentActivity 才能用
  registerForActivityResult（android.app.Activity 会在启动时崩）。
- 匿名拉不到 CI 日志时，让用户开浏览器贴日志最快；本轮就是靠日志一击定位。

v0.1.1（2026-09-12）一次通过：tag 指向 06db941（统计失败计数修复 +
二分探测学习冷却），CI 首次即成功，产物
sakura-llmgateway-v0.1.1-x86_64-pc-windows-msvc.exe（4.4 MB）。

### 冒烟测试的三次踩坑（改工作流前必读）

1. **`test/gateway.json` 在 CI checkout 上不存在**：它被 .gitignore 忽略，且是 test/
   目录下唯一文件——目录本身不会被 git 跟踪，CI 上连目录都没有，`Set-Content`
   无法创建父目录。工作流改为在 llm-gateway 工作目录根生成 `ci-gateway.json`。
2. **生成的配置必须含 provider `name` 字段**（serde 无默认值，缺了解析失败，
   网关直接 exit(1)），且健康检查要探 **UI 端口**（配置里 ui_port 指向的那个），
   不是 API 端口。
3. **PowerShell here-string（@'…'@）在 YAML 块标量里不能用**：结束符 `'@` 必须顶格
   在第 0 列，YAML 缩进使其必然成为语法错误。改用单行 JSON 字符串 +
   `utf8NoBOM`（BOM 会让 serde_json 解析失败）。

本地复现 CI 冒烟：按工作流步骤用 node/手写生成同样的单行 JSON 配置 → 起网关 →
curl UI 端口期待 200（曾用此法在改工作流前验证修复）。

## 14. Android APK 发布（2026-09-13 方案定稿并实现）

方案（grill-me 全轮收口）：手机自用；Rust 网关零改动，作为子进程进 APK；
Universal APK（arm64-v8a/armeabi-v7a/x86_64）；WebView 全屏加载现有内嵌控制台；
配置存 App 私有目录，首次启动引导页可选导入或跳过（自动生成空配置）；
注册 gateway.json 的系统"打开方式"，运行中经 /api/config/import 热加载；
导出走 SAF 保存对话框；CI secret 签名；minSdk 26；前台服务 + 10s HTTP 探活
看门狗（/api/status，死亡或假死重启）+ START_STICKY；versionName=tag、
versionCode=提交数；APK 与 exe 同一 Release。

实现要点：
- `android/` 顶层 Gradle 工程（Kotlin + Compose 引导页 + WebView + GatewayService）。
- 网关二进制以 `jniLibs/<abi>/libllmgateway.so` 打包（PackageManager 解压到
  nativeLibraryDir 后可 exec，规避 Android 10+ W^X 对直接 exec 的限制）。
- CI：release.yml 拆为 build-windows / build-android / release 三个 job；
  Android 用 cargo-ndk（-p 26）交叉编译三架构 → 拷入 jniLibs → gradle assembleRelease
  → 无 keystore secret 时降级出 unsigned APK 并告警。
- index.html 增加移动端响应式 CSS（触控目标、表格横滚、卡片堆叠）。

本机限制：无 NDK/Java，无法本地出 Android 产物；本地已验证
cargo check（桌面）+ 控制台 200 + 移动 CSS 已内嵌；
Android Rust targets 已安装（aarch64-linux-android / armv7-linux-androideabi /
x86_64-linux-android；armv7 曾因 rustup 组件冲突安装失败，清掉残留目录后成功）。
首次发布前需用户在 GitHub Secrets 配置：
ANDROID_KEYSTORE_B64 / ANDROID_KEYSTORE_PASSWORD / ANDROID_KEY_ALIAS /
ANDROID_KEY_PASSWORD（keystore 本地永久备份）。

### 注意

- tag 前先确认 main 上工作流是最新的——**tag 指向的提交决定用哪版工作流**；
  v0.1.0 曾因此 force-move 过两次（修 CI 时 tag 停留在旧提交，连续失败）。
- CI 日志匿名不可拉取（需 repo admin token），排障只能看 API 的
  steps conclusion + 本地复现。
- `fetch --tags` 后如果 tag 被本地 force-move 过，记得 `git push -f origin <tag>`
  同步远端。

---

## Session 2026-09-14: 双语文档同步（冷却学习 + 安卓使用）

- **grill-me 两轮敲定**：文档写明具体探测参数（非定性描述）；安卓章节完整成章、
  插在安装章节之后（后续章节顺延编号）；加 ASCII 二分示意；写明「电脑→手机」
  完整迁移流程；纯文档改动不发版。
- **README（中英）**：智能冷却/冷却学习两条重写为二分探测算法（初始 T → 定界
  [T/2,T] / [T,2T] → 二分 ~5 轮 → 写入区间中点；≤10 次探测、900s 上限、
  每 Key 每小时至多一次校准），替换了过时的「递增间隔 5s→300s」描述，
  并加了 ASCII 区间收敛示意图。
- **doc/使用说明.md & doc/USAGE.md**：新增「## 2. 安卓端（APK）/ Android (APK)」
  完整章节（安装、首启、前台服务/控制台/生命周期表格、系统级导入、导出、
  电脑→手机四步迁移、模拟器+ERR_CONNECTION_REFUSED 提示）；Key 池行为章节
  新增 4.1 小节详述二分探测算法；后续章节 3–11 顺延编号（英文文档原第 6 节
  标题是 "Connecting agents & tools" 不是 "Hooking up"，首轮替换 MISS 后补改）。
- **技巧**：node 脚本改 Markdown 时，模板字符串会被正文里的反引号打断——
  用 `[...].join('\n')` 数组拼接代替。
- 纯文档改动，只提交推送、不发 Release（用户选定）。

---

## Session 2026-09-14: 修复跳过导入后凭空出现 Sensenova（v0.2.5）

- **症状**：安卓全新安装、跳过导入后，控制台供应商列表里已有 sensenova。
- **根因**：DefaultConfig.kt 的跳过路径硬编码了 sensenova 占位 provider，
  源于「Rust 解析器要求 providers 非空」的错误假设；实际 `providers: []`
  完全合法（等于 Config::default()）。
- **修复**：跳过路径改为生成空 providers；用真实二进制冒烟验证（启动 +
  /api/providers 返回空列表 + 200）。commit 6c797e7，Release v0.2.5。
- **注意**：已安装设备若之前跳过过导入，其 gateway.json 仍带旧占位条目，
  需在控制台手动删除或清应用数据。
- **流程教训（已记入第 8 节第 13 条）**：跨语言写默认配置前，先用真实解析器
  验证边界状态；Kotlin/Rust 两边 schema 假设要有一致性冒烟检查。

---

## Session 2026-09-14: Anthropic Messages 协议支持（v0.3.0）

- **grill-me 两轮敲定**：仅入站起步 → 实际做了完整矩阵（Q4 用户选推荐）；
  全量对齐（tools/图片/count_tokens/流式互转）；显式 `protocol` 字段（不嗅探）；
  双头鉴权（Bearer + x-api-key）；E2E + v0.3.0。
- **实现**：
  - `protocol.rs`（新）：纯 JSON 双向翻译 + 10 个单元测试。请求（system 顶层字段、
    max_tokens、stop_sequences、tools/tool_choice、图片块 base64/URL）、非流式响应
    （content blocks ↔ choices、stop_reason ↔ finish_reason 映射表）、流式状态机
    （OpenAiToAnthropicStream / AnthropicToOpenAiStream，SSE 事件序列互转）。
  - `proxy.rs` 重写为协议无关的 `forward()` 核心：入站协议 × 上游协议 4 组合
    都走同一个 Key 池/冷却/重试循环；`call_upstream` 按 provider.protocol() 发
    x-api-key 或 Bearer；prober 的探测体也按上游协议构造。
  - 路由：`/v1/messages`、`/v1/messages/count_tokens`（Anthropic 上游透传，
    OpenAI 上游本地估算 ~4 字符/token，永不 404）。
  - `config.rs`：Provider 新增 `protocol` 字段（serde default "" = openai，向后兼容）。
  - 控制台：添加提供商下拉框选协议 + 列表徽标；编辑走 prompt。
  - mock_upstream 增加 `/v1/messages`（x-api-key 同款 sk-bad/sk-slow/sk-dead 行为）。
- **E2E 实测过**：4 个组合（OpenAI/Anthropic 入站 × OpenAI/Anthropic 上游）
  非流式 + 流式全通，翻译正确（含跨协议流式：OpenAI chunk → Anthropic SSE 事件、
  Anthropic 事件 → OpenAI chunk）；count_tokens 两种上游各返回估算/透传值。
- **坑**：
  - 翻译方向第一次写反了——上游响应是「上游的格式」，应翻成「客户端的格式」；
    E2E 抓住（组合 2/3 空 content），已修。
  - Rust 闭包内 `async fn` 不能捕获环境变量（upstream_proto 要作参数显式传入）。
  - SSE 事件可能跨 chunk 边界——Anthropic→OpenAI 方向要按空行分帧缓冲
    （find_event_end），不能逐 chunk 贪婪解析。
  - node 改文档的 rep() 若不把返回值赋回去等于没改（这次真踩了：14 个 "OK"
    替换全是幻影，标题不匹配导致 MISS 后才发现文件根本没写入）。
- 文档：README 中英特性/API 表更新；doc/使用说明.md & USAGE.md 新增 §5
  （Claude Code 接入示例 `ANTHROPIC_BASE_URL`），后续章节顺延到 12；中英同步核对。


## 15. 2026-09-15 会话日志：v0.3.1 —— 卡在"网关启动中" + 不弹通知权限（用户 bug 报告）

**报告**：Android 16（Redmi K80 Ultra，arm64-v8a），v0.3.0 APK 全新安装后永远卡在"网关启动中…"界面，且应用从未弹出通知权限请求。

### 诊断（diagnosing-bugs 技能）

先排除了两个假设（本地可证伪）：
- **16KB 页对齐**：Android 16 设备逐步启用 16KB 内核页。用 llvm-readelf 检查 arm64 ELF 的 LOAD 段——NDK r26d 已经输出 0x4000（16KB）对齐，排除。
- **FGS 类型/清单声明**：foregroundServiceType="specialUse" + PROPERTY_SPECIAL_USE_FGS_SUBTYPE 都在，排除。

**根因 1（卡死）——主线程网络 I/O 被 catch 吞掉**。v0.2.2 引入的健康门控里：

```kotlin
LaunchedEffect(Unit) {          // 默认跑在主线程
    while (true) {
        gatewayUp = isGatewayUp()   // HttpURLConnection 网络调用！
        ...
    }
}
```

Android 在主线程做 socket I/O 会抛 NetworkOnMainThreadException，被 `catch (_: Exception) { false }` 吞掉 → 永远返回 false → 永远停在等待页。**每台设备、从 v0.2.2 起都有此 bug**；v0.2.2 之前用户直接看到的是 ERR_CONNECTION_REFUSED 死页面，同样异常但症状不同。之前模拟器上"看起来正常"是因为我们从未走完这条路径去仔细观察——其实 v0.2.2 修复 AddrInUse 后，这个门控从未真正通过过，用户是第一个在真机上撞到前门的人。修复：`withContext(Dispatchers.IO) { isGatewayUp() }`。

**根因 2（不弹通知权限）**：POST_NOTIFICATIONS 自 Android 13 (API 33) 起是运行时权限。清单里声明了但从未 requestPermissions() → 通知（含"停止"按钮）在 13+ 全部不可见。服务本身照常运行，只是用户看不到。修复：onCreate 里 SDK>=33 时请求。

**附带**：看门狗首次健康检查从 10s 提前到 2s（`FIRST_CHECK_DELAY_MS`），"运行中"状态翻转更快。

### CI 迭代（3 次）

1. **setup-android@v3 失败**：runner 镜像的 cmdline-tools 升到 16.0 后删除了遗留的 `tools` 包，action 内部安装它时报 "Failed to find package 'tools'"。
2. **升到 v4 失败**：同样原因——v4 也没移除对 `tools` 包的尝试。
3. **最终方案**：彻底删掉 setup-android action。ubuntu-24.04 镜像自带完整 SDK（sdkmanager 在 cmdline-tools/latest），直接 `yes | sdkmanager --licenses` + 安装 NDK 即可。

### 教训

- **Compose 协程默认主线程**：任何 LaunchedEffect 里的 I/O（网络、文件）必须显式 `withContext(Dispatchers.IO)`。且**吞掉所有异常的 catch 会把系统性失败伪装成"条件不满足"**——NetworkOnMainThreadException 被吞后症状是"永远 false"，而不是崩溃，极难察觉。新代码里 catch 块至少要 Log 一次。
- **运行时权限必须在代码里请求**：清单声明 ≠ 授权。Android 13+ 上前台服务的可见性完全依赖 POST_NOTIFICATIONS 的运行时授予。新权限加进清单时要同步问自己"这是不是运行时权限"。
- **GitHub Actions 的 runner 镜像会漂移**：android-actions/setup-android 在新镜像上 v3/v4 都会挂（'tools' 包被 cmdline-tools 移除）。镜像自带 SDK 足够时，直接用 sdkmanager 反而更稳。CI 挂了先看是不是镜像漂移，再怀疑自己的代码。
- Release: v0.3.1（commits 6703dce / eb443a8 / bc356d4），APK 11.6 MB + exe 4.7 MB。

## 16. WebUI i18n + Anthropic 地址展示（v0.3.2 发布，commit d8b53c1）

**需求**（grill-me 两轮敲定）：顶部 api-line 在 OpenAI 地址旁并列展示 Anthropic Messages 地址；顶栏加中/英语言切换；全量翻译（含 confirm/prompt 弹窗、placeholder、动态表格/toast）；默认语言跟随浏览器（`navigator.language`），手动切换后 localStorage 记住；切换控件用与主题按钮一致的小按钮（EN/中）；两个地址点击复制。

**实现**（`static/index.html`，单文件内 ~119 对 key 的 `I18N` 字典 + `applyLang()` + `data-i18n`/`data-i18n-ph` 标记）：

- 静态 HTML 用 data-i18n 标记；JS 渲染的表格/按钮/toast/confirm/prompt 全部走 `t(key)` 查表；带参数的文案用函数值（`modal_selected: n => ...`）。
- `applyLang()` 切换后立即重渲染（renderConfig/renderStats），保证动态区域同步。
- 初始化顺序：i18n 先于 theme（theme 按钮文案走 t()），二者互不依赖。
- api-line 两个地址 `<code>` 均可点击复制（clipboard API + execCommand 兜底——Android WebView 老内核可能没有 navigator.clipboard）。

**验证反馈回路**：本地 release 构建 + mock_upstream 起真实进程，preview 截图 + 快照验证：中文默认 → 点 EN 全部切换（含表格表头、按钮、placeholder）→ localStorage 持久化 → 切回中文恢复；两地址内容正确（`:9400/v1` 与 `:9400/v1/messages`）。改完 UI 必须 cargo build 重新内嵌（rust-embed）——只刷新浏览器无效。

**坑**：

1. 写 node 转换脚本改 markdown/HTML 时，模板字符串里的反引号会被 bash 吃掉——上次的教训仍然适用；这次直接用 str_replace 分批改，避开脚本（本条目本身也是用数组拼接 + `String.fromCharCode(96)` 绕开的）。
2. 验证脚本里 `t('textarea')` 误匹配（`document.createElement('textarea')`），排查 missing-keys 假阳性时要排除这类非 i18n 调用。

**遗留**：~~Issue #2（小屏设备滚动跳过后底部按钮不可点）待修~~ → 已在下节修复并随 v0.3.2 发布。

## 17. Issue #2 修复：小屏设备引导页按钮过小/不可点（v0.3.2）

**症状**（用户反馈，Android 8.1 / 441x537 / armeabi-v8a）：0.3.1 校验通过，但小屏设备上引导页最下面的「跳过，用空配置启动」按钮过小无法点击；跳过后在页面里点任何按钮都无响应。

**诊断**：不是 WebUI 的问题——是 Compose 引导页布局问题。Onboarding() 用固定高度 + `Arrangement.Center` 的 Column，内容（大号 emoji + 标题 + 三行说明 + 两个按钮）在小屏上超出视口时，Center 排列会从上下两边同时压缩子元素，按钮被压到 48dp 最小点击目标以下，视觉上存在但 tap 区域几乎为零。等待页（网关启动中）同布局同隐患。WebUI 本身早有 mobile 媒体查询（40-44px 触控目标），不受此问题影响。

**修复**：两个全屏 Compose Column 都改为 `verticalScroll(rememberScrollState())` 可滚动 + `heightIn(min = 48.dp)` 强制按钮最小高度 + `navigationBarsPadding()` 避开手势条；内容能放下时仍然垂直居中（Center 在可滚动容器内对超出内容自动退化为顶部对齐，不再挤压）。

**验证**：无本地 Android 构建环境，反馈回路是 CI（历次 6 个 release 全绿）；修复本身是布局约束调整，无逻辑分支。

### v0.3.2 发布记录

- Release: https://github.com/sloxphrite73/sakura-llmgateway/releases/tag/v0.3.2（universal APK 11.6 MB + Windows exe 4.7 MB，CI 一次通过）。
- 包含：WebUI 双语 i18n + Anthropic 地址展示（d8b53c1）、Issue #2 小屏修复（3898a75）。
- 发布前处理过一次 push 被拒：远端多了用户加的 README 反馈链接提交（318d5f7/43e0d43），rebase 后推送，无冲突。
- 文档同步（2026-09-16）：使用说明.md / USAGE.md 已补上——启动服务表加 Anthropic Messages 地址行；安卓「日常使用」表加通知权限说明（Android 13+ 运行时请求）并更新看门狗描述（首次 2 秒、之后每 10 秒）；模拟器提示改为升级 v0.3.2+（涵盖启动竞态、通知权限、小屏按钮三修）；Web 控制台「主题」小节扩为「主题与语言」（EN/中 切换 + localStorage + 跟随浏览器 + 顶部双地址点击复制）；配置参考示例加 `"protocol": "openai"`，字段表加 `providers[].protocol`。中英两份同步修改，结构一致。

## 18. 免费供应商目录（现有提供商面板）+ 统计图例修复

**背景**：用户要求对齐 OmniRoute 的「白嫖免费」能力。经 grill-me 两轮定界：本次只做**目录层**（降低免费上游接入成本），跨供应商回退/路由策略明确推迟（等路由策略需求定稿再做）；目录仅收录 OpenAI 兼容上游；更新机制 = 二进制内嵌 + GitHub raw 按需拉取（不静默自动更新）。

**实现**：

- `llm-gateway/free-catalog.json`：9 家供应商（SiliconFlow、Z.AI、OpenRouter :free、Pollinations(keyless)、Cerebras、NVIDIA NIM、Groq、Mistral、Cloudflare Workers AI），字段含 base_url/protocol/keyless/signup_url/guide_url/notes_zh/notes_en/free_models。`include_str!` 编译期内嵌。
- `src/free_catalog.rs`：`GET /api/free-catalog`（内嵌或本会话已拉取的远程缓存）、`POST /api/free-catalog/refresh`（GitHub raw 拉取 → 结构校验（空列表/非法 protocol 拒绝）→ 内存缓存，仅会话级）。guide_url 重写为 GitHub blob 链接。`CatalogCache` 挂在 App 上。
- 教程文档 `doc/free-providers/*.md` ×9（中文，注册→取 Key→领额度→回网关添加的分步说明）。
- WebUI 配置页新增「现有提供商」卡片面板：免费说明 + 可展开模型列表 + 教程/注册双链接 + 多 Key 输入框（每行一个）+ 确认添加。添加动作 = 现有 admin API 组合（create_provider → models/import → 逐个 add_key），不引入新供应商类型。已添加卡片显示「✓ 已添加 · N key」。i18n 中英双语全覆盖。
- 顺手修复统计面板图例文案：「粉=成功」→「绿=成功」（hist bars 用的是 --ok 绿色，文案写错色；zh 静态 HTML + zh 字典 + en 字典三处）。

**坑与教训**：

1. `let CATALOG` 声明放在了 initLang() 之后——`applyLang` 在初始化时同步调用 `renderCatalog`（通过 `if (CATALOG) renderCatalog()`）会触发 TDZ "Cannot access before initialization"。教训：**给 applyLang 增加新依赖状态时，声明必须提到 applyLang 定义之前**（和 LANG 本身一样）。
2. cargo build 报 os error 5（拒绝访问）＝旧 exe 进程还活着锁着文件；先 Stop-Process 再 build。
3. /api/free-catalog/refresh 在目录 JSON 推上 GitHub 之前会 404——这是预期行为，优雅失败（toast 报错、目录保持内嵌版）；**推完代码再验证 refresh**。

**验证反馈回路**：cargo check/test（13 通过）→ release 构建 → 真实进程 + preview：9 张卡片渲染、Pollinations 免 Key 添加成功（provider + 3 模型）、OpenRouter 双 Key 添加成功（6 模型 + 2 key 掩码入库）、刷新后「已添加」标签持久、图例文案正确。refresh 的 GitHub 路径待推送后可通（404 → 200）。
