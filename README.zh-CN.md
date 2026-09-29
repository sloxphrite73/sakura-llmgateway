# Sakura LLM Gateway

Rust 打造的高性能本地 LLM API 网关——同时提供 **OpenAI 兼容**与 **Anthropic Messages** 端点，按提供商维护智能负载均衡的 API-Key 池，遇到 429 自动换 Key 重试，支持流式输出，内置 Web 控制台。单个可执行文件（Windows exe / Android APK），任何 OpenAI 兼容客户端——Claude Code、Cherry Studio 等——都能安全共享你的整个 Key 池。

[English](README.md) | [简体中文](README.zh-CN.md) · 使用说明：[中文](doc/使用说明.md) | [English](doc/USAGE.md)

反馈 Bug 和功能建议进 QQ 群 348900172

## 特性

**双协议代理** — 任何 agent 或工具指向 `http://127.0.0.1:8000/v1`。网关在同一端口接受 OpenAI Chat Completions（`POST /v1/chat/completions`）和 Anthropic Messages（`POST /v1/messages`）。每个提供商可选 `openai` 或 `anthropic` 上游协议，网关自动做双向翻译——OpenAI 客户端能用真 Claude API 的 Key 池，反之亦然。流式 SSE、工具调用、图片输入、`count_tokens` 全部透传。

**API-Key 池自动轮换** — 每个提供商的 Key 按轮询调度。某把 Key 收到 `429` 立即冷却并切到下一把，客户端完全无感。重试预算 = `max_attempts × Key 数`（默认 3×）；首字节发出后流原样透传，绝不重试。

**智能冷却学习** — 冷却时长按优先级链解析：上游 `Retry-After` → `learned_cooldown`（二分探测实测）→ 每 Key 覆盖 → 全局默认（60 秒）。Key 被 429 后，后台探测器用二分搜索测量真实限流窗口：在时间 T 处探测——成功则窗口更短、进入 `[T/2, T]`，仍 429 则更长、进入 `[T, 2T]`，然后二分约 5 轮收敛到 ±1 秒。有界设计：每次测量最多 10 次探测、窗口上限 900 秒、每把 Key 至多每小时重新校准一次。无效 Key（401/403）隔离 30 分钟。

```
探测间隔（秒）
T ──── 429 ──▶ [T, 2T]          成功 ──▶ [T/2, T]
                │                       │
                ▼ 二分 ~5 轮             ▼ 二分 ~5 轮
             收敛至 ±1s ◀━━━━ 区间中点写入 learned_cooldown
```

**模型路由与策略** — 以 `provider/model` 形式请求（如 `openai/gpt-4o`）或配置短别名。两个过滤开关（`lock_model_group` / `lock_provider`，默认都开）+ 用户可堆 0–3 个排序 key（A–J：success_rate、rpm、tpm、avg_tftt、token/bill balance ×2、price、tps）→ 四种模式：精确 / 模型优先（跨提供商 fallback）/ 提供商优先 / 可用优先。策略页有 dry-run 预览。

**托管模型与免费目录** — 每个提供商维护模型目录，支持从上游 `/v1/models` 一键导入。可选白名单模式拒绝列表外模型。控制台内置 25 家免费/有免费额度的 OpenAI 兼容上游——SiliconFlow、Z.AI GLM、OpenRouter `:free`、Pollinations（免 Key）、NVIDIA NIM、Groq、Mistral、Google Gemini 等——每张卡片带教程链接与预置免费模型，粘贴 Key → 确认即可一步完成（供应商 + 模型 + Key）。

**图片输入声明** — 模型可标记 `supports_images`（模型编辑页开关），在 `/v1/models` 中声明，客户端可发现支持图片的模型。图片内容块同协议透传（OpenAI→OpenAI）、跨协议翻译（OpenAI↔Anthropic）。网关原样转发图片请求，无大小限制（解除了 axum 默认 2 MiB 请求体上限）。

**持久化统计与实测指标** — 请求统计（总量、24h 逐时柱状图、按 Key/提供商/模型的成功失败计数）合并进 `gateway.json` 的 `stats` 字段，重启不丢失。每模型 avgTFTT（首字延迟）和 avgTPS（生成速度）从流式响应采样并持久化。每提供商检测的 RPM/TPM/success_rate/avg_tftt/tps 也持久化，重启后先显存盘值，新流量后覆盖。

**每 Key 指标 seed** — 全新 Key 无实测指标，排序会把它排最后。设置 `seed_rpm` / `seed_tpm` / `seed_success_rate` / `seed_avg_tftt_ms` / `seed_tps` 预置初值，Key 服务真实流量后自动覆盖。

**Web 控制台** — `http://127.0.0.1:8001/`，单页樱花主题控制台（深/浅色 + EN/中文，偏好持久）：统计、提供商、模型、策略（filter + sort + dry-run）、API Key（实时状态、冷却倒计时、RPM/TPM/avg_TFTT 列、每 Key 指标 seed 编辑器、显示/复制、批量导入）、设置。移动优先响应式布局，手机端底栏导航。

**版本显示** — 侧栏底部显示真实版本号（如 `Sakura v0.7.1`），由编译时 `git describe --tags` 嵌入。

**配置导出/导入** — 浏览器直接下载 `gateway.json`；导入采用"先验证后切换"：非法文件整份拒绝，合法文件热切换无需重启（统计也一并采纳，不丢弃），进行中的请求在旧状态上完成。

**单二进制，无 Node 工具链** — Web 控制台为单文件原生 HTML/JS，通过 `rust-embed` 内嵌；`cargo build` 一步出产物。

```
┌──────────────────────────────────────────────────────────┐
│           Agent / 工具（OpenAI SDK 等）                   │
└──────────────────────────────────────────────────────────┘
                            │
              ┌─────────────┴─────────────┐
              ▼                           ▼
      ┌──────────────┐            ┌──────────────┐
      │  网关 API     │            │   Web 控制台  │
      │  :8000 /v1   │            │     :8001    │
      └──────────────┘            └──────────────┘
              │                           │
              └─────────────┬─────────────┘
                            ▼
                 ┌─────────────────────┐
                 │  Sakura LLM Gateway │
                 │  Key 池 · 模型 ·     │
                 │  统计 · 冷却管理     │
                 └─────────────────────┘
                            │
              ┌─────────────┼─────────────┐
              ▼             ▼             ▼
        ┌─────────┐   ┌─────────┐   ┌─────────┐
        │ 提供商 A │   │ 提供商 B │   │ 提供商 C │
        │ Key 1..n │   │ Key 1..n │   │ Key 1..n │
        └─────────┘   └─────────┘   └─────────┘
```

## 快速开始

### 方式一：下载 Release（Windows exe / 安卓 APK，无需工具链）

从 [GitHub Releases](https://github.com/sloxphrite73/sakura-llmgateway/releases)
下载 `sakura-llmgateway-vX.Y.Z-x86_64-pc-windows-msvc.exe`，放到一个空文件夹，
双击运行即可。Web 控制台地址：`http://127.0.0.1:8001/`。

**安卓：** 在同一 Release 下载 `sakura-llmgateway-vX.Y.Z-universal.apk` 安装
（如提示需允许"安装未知应用"）。首次启动可导入电脑上导出的 `gateway.json`，
也可以先跳过、用空配置启动后在控制台里添加。App 以前台服务方式运行网关
（常驻通知 + 10 秒探活看门狗），全屏显示同一个 Web 控制台；在文件管理器里对
`gateway.json` 用"打开方式 → Sakura LLM Gateway"即可导入（网关运行中会热加载）。

### 方式二：一键脚本

**Windows：** 双击一次 `llm-gateway/install.bat`（缺少 Rust 时以用户级权限安装、
编译 release 版本、生成 `gateway.json`），之后每天用 `llm-gateway/start.bat` 启动。

**Linux / macOS / Git Bash：**

```bash
./install.sh   # 检查/安装 Rust、编译 release 版本、生成默认 gateway.json
./start.sh     # 如未构建则自动构建，然后启动网关
```

### 方式三：从源码构建

需要 Rust（Windows 上 GNU 工具链即可）：

```bash
cd llm-gateway
cargo build --release
./target/release/llm-gateway.exe            # 配置默认为 ./gateway.json
./target/release/llm-gateway.exe --config path/to/gateway.json
```

### 启动后

| 服务 | 地址 | 说明 |
|------|------|------|
| OpenAI 兼容 API | `http://127.0.0.1:8000/v1` | agent / 工具接入点 |
| Web 控制台 | `http://127.0.0.1:8001/` | 管理一切 |

端口（API `8000`、控制台 `8001`）可在配置文件中设置，也可在控制台里修改
（修改端口后重启生效）。

## 使用指南

### 配置提供商与 Key

1. 打开 Web 控制台 `http://127.0.0.1:8001/`
2. 进入**提供商**页签，添加提供商（名称 + OpenAI 兼容 `base_url`）。也可从内置免费目录（25 家上游，带教程链接）选一家一步添加。
3. 进入 **API Key** 页签，从下拉框选择提供商，在输入框中粘贴一把或多把 API Key（多个每行一个，自动批量添加），点击**添加**。Key 始终以掩码显示（头尾可见）；点**显示**可查看完整 Key。
4. （可选）进入**模型**页签，一键从上游 `/v1/models` 导入模型目录，单独启停模型，为视觉模型开启 `supports_images`，设置上下文长度 / 输入价格 / 输出价格，并可开启白名单模式
5. （可选）配置别名，如 `fast` → `gpt-4o-mini`

### 使用网关 API

```bash
curl http://127.0.0.1:8000/v1/chat/completions \
  -H "Content-Type: application/json" \
  -d '{
    "model": "openai/gpt-4o",
    "messages": [{"role": "user", "content": "hello"}],
    "stream": true
  }'
```

或使用 OpenAI SDK：

```python
from openai import OpenAI
client = OpenAI(base_url="http://127.0.0.1:8000/v1", api_key="anything")
```

### 监控流量

打开**统计**页签：请求总量、24 小时逐时柱状图，以及按 Key / 提供商 / 模型的
成功失败计数。API Key 页显示每把 Key 的 RPM、TPM、avg_TFTT、请求数（持久化）、
实时冷却状态。统计持久化进 `gateway.json`（合并到 `stats` 字段），重启不丢失。

### 备份或迁移配置

- **导出**：点击控制台中的"导出配置"（或 `GET /api/config/export`），浏览器直接
  下载 `gateway.json`
- **导入**：点击"导入配置"（或 `POST /api/config/import`）并选择文件——非法文件
  整份拒绝；合法文件立即热切换（统计也一并采纳），无需重启

## 配置格式

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

关键字段：

- Key 的 `cooldown_secs: null` 表示使用全局默认值；`learned_cooldown` 由二分探测器自动写入，优先级高于 `cooldown_secs`
- `strategy.filter` 都开（默认）= 精确 `(provider, model)`、只轮换 Key
- `model_groups` 声明同一逻辑模型跨提供商，用于跨提供商 fallback
- `rpm_limit` / `tpm_limit` = 手动 RPM/TPM 上限（`null` = 自动，显实时聚合）
- `price_table` / `output_price_table` = 每模型输入/输出价格（CNY / 1M tokens，`0` = 免费）
- `supports_images` = 在 `/v1/models` 中声明模型支持图片输入
- `context_length` = 模型上下文窗口（tokens，模型卡显示为 "Nk"）
- `seed_*` = 预置初始指标，Key 服务真实流量后自动覆盖
- `stats` = 请求统计，合并进 `gateway.json`（不再单独 `stats.json`）；旧 `stats.json` 启动时自动迁入后删除。导入 `gateway.json` 时统计也一并采纳。

## API 概览

### 网关 API（OpenAI + Anthropic 兼容）

| 接口 | 方法 | 说明 |
|------|------|------|
| `/v1/chat/completions` | POST | Chat Completions（流式 + 非流式，图片） |
| `/v1/messages` | POST | Anthropic Messages（流式 + 非流式，tools / 图片） |
| `/v1/messages/count_tokens` | POST | Token 计数（Anthropic 上游透传，OpenAI 上游本地估算） |
| `/v1/models` | GET | 聚合模型列表（含 `supports_images` 声明） |

### 控制台 API

| 接口 | 方法 | 说明 |
|------|------|------|
| `/api/status` | GET | 实时配置 + Key 池状态 + 版本号 |
| `/api/stats` | GET | 请求统计（总量、24h 柱状图、按 Key/提供商/模型） |
| `/api/config/export` | GET | 下载 `gateway.json`（含统计） |
| `/api/config/import` | POST | 配置 + 统计热重载（先验证后切换） |
| `/api/providers` | GET / POST | 列出 / 创建提供商 |
| `/api/providers/{id}` | PUT / DELETE | 更新 / 删除提供商 |
| `/api/providers/{id}/rate-limits` | PUT | 设置提供商 RPM/TPM 限额 |
| `/api/providers/{id}/price-table` | PUT | 设置每模型输入 + 输出价格（`{table, output_table}`） |
| `/api/providers/{id}/keys` | POST | 添加 Key |
| `/api/providers/{id}/keys/{key_id}` | DELETE / PUT | 删除 Key / 更新指标 seed |
| `/api/providers/{id}/models` | POST | 添加托管模型 |
| `/api/providers/{id}/models/{model_id}/context-length` | PUT | 设置模型上下文长度 |
| `/api/providers/{id}/models/{model_id}/supports-images` | PUT | 切换图片输入支持 |
| `/api/keys/{key_id}/cooldown` | DELETE | 清除 Key 冷却 |
| `/api/strategy/dry-run` | POST | 预览某模型的路由 |
| `/api/settings` | PUT | 更新全局设置 |

## 测试

项目内置一个模拟的 OpenAI 兼容上游（`sk-bad` 这把 Key 恒定返回 429）：

```bash
cd llm-gateway
cargo run --bin mock_upstream &     # 监听 127.0.0.1:9001
cargo run -- --config test/gateway.json
# 然后测试：
curl http://127.0.0.1:8000/v1/chat/completions -H "Content-Type: application/json" \
  -d '{"model":"mock/mock-large","messages":[{"role":"user","content":"hi"}]}'
```

## 项目结构

```
llm-gateway/
├── src/
│   ├── main.rs    入口，路由（API :8000，控制台 :8001）、落盘定时器
│   ├── config.rs  JSON 配置结构 + 先验证后切换 + 原子持久化
│   ├── state.rs   共享状态 + Key 池（轮询、冷却、无效隔离）
│   ├── proxy.rs   /v1 代理：模型解析、Key 轮换、探测、流式透传
│   ├── stats.rs   请求统计（计数器 + 24h 柱状图，持久化）
│   ├── admin.rs   Web 控制台背后的 REST API
│   ├── ui.rs      内嵌 Web 控制台（static/index.html）
│   └── build.rs   编译时嵌入 git tag 作为版本号
├── static/
│   └── index.html 单文件控制台 UI
├── test/
│   └── gateway.json   mock 上游测试配置（已 gitignore，请自行创建）
├── install.bat / start.bat   Windows 一键脚本
├── install.sh / start.sh     Linux / macOS / Git Bash 脚本
└── android/         安卓应用（Kotlin，WebView 控制台 + 前台服务）
```

### 安卓端

APK 是同一个 Rust 二进制的原生薄壳：CI 里交叉编译 `arm64-v8a` / `armeabi-v7a` /
`x86_64` 三个架构，以 `jniLibs/*/libllmgateway.so` 打包，由前台服务 spawn 拉起，
并带 10 秒 HTTP 看门狗（探活 `/api/status`，死亡或假死即重启）。UI 就是内嵌控制台
的全屏 WebView；配置导出走系统保存对话框，`gateway.json` 导入对运行中的网关热加载。

## 技术栈

- **Rust**（edition 2021）— 全部后端，单个静态二进制
- **axum + tokio + hyper** — 异步 HTTP 服务与代理
- **reqwest (rustls)** — 上游 HTTPS 客户端，无 OpenSSL 依赖
- **rust-embed** — 控制台 UI 编译期内嵌
- **serde / serde_json** — 配置、统计与 API 序列化

## 贡献

欢迎提交 Issue 和 Pull Request！

## 许可证

[GPL-3.0](LICENSE)
