# Sakura LLM Gateway

🎯 Rust 打造的高性能本地 LLM API 网关 —— 同时提供 **OpenAI 兼容**与 **Anthropic
Messages** 端点，按提供商维护智能负载均衡的 API-Key 池，支持流式输出，
内置 Web 控制台。

[English](README.md) | [简体中文](README.zh-CN.md) · 📖 使用说明：[中文](doc/使用说明.md) | [English](doc/USAGE.md)

## ✨ 特性

- 🔌 **OpenAI 兼容 API** - 任何 agent / 工具直接接入 `http://127.0.0.1:8000/v1`
  - `POST /v1/chat/completions`（流式 SSE 与非流式）
  - `GET /v1/models`（聚合所有提供商的托管模型 + 别名）
- 🌸 **Anthropic Messages 协议** - 同时暴露 `POST /v1/messages`（Claude Code、
  Anthropic SDK 可直接接入）；每个提供商可选 `openai` 或 `anthropic` 协议，
  网关自动做双向翻译——任意入站协议 × 任意上游协议均可组合（OpenAI 客户端
  也能用真 Claude API 的 Key 池）；支持 tools / 图片 / 流式事件互转 / count_tokens
- 🔑 **API-Key 池** - 多把 Key 轮询调度；某把 Key 收到 `429` 立即冷却并切到下一把，
  请求不中断
- ⏱️ **智能冷却** - 冷却时长解析顺序：上游 `Retry-After` 响应头 →
  `learned_cooldown`（二分探测学习所得）→ 每 Key 独立覆盖 → 全局默认（60 秒）；
  全部 Key 冷却时快速失败返回 `429`
- 🔬 **冷却学习（二分探测）** - Key 被 429 后，后台探测器自动测出该 Key 真实的
  限流窗口并写入 `learned_cooldown`：从上次学习值（或全局默认）T 开始，隔 T 秒
  发一个微小探测请求（该提供商第一个启用的托管模型，`max_tokens: 1`）——
  成功则窗口更短、进入区间 `[T/2, T]`，仍 429 则更长、进入 `[T, 2T]`；随后对
  区间二分约 5 轮收敛到 ±1 秒，取**区间中点**写入。有界设计：每次测量最多
  10 次探测、窗口上限 900 秒，且每把 Key 至多每小时重新校准一次，不会浪费配额

```
探测间隔（秒）
T ──── 429 ──▶ [T, 2T]          成功 ──▶ [T/2, T]
                 │                       │
                 ▼ 二分 ~5 轮             ▼ 二分 ~5 轮
              收敛至 ±1s ◀━━━━ 区间中点写入 learned_cooldown
```
- 🚫 **无效 Key 隔离** - `401/403` 说明 Key 本身已失效：隔离 30 分钟（而非短冷却），
  轮询直接跳过
- 🔁 **首字节前重试** - 429 / 5xx / 超时自动换 Key 重试（重试预算 =
  `max_attempts` × Key 数，默认 3×）；首字节发出后流原样透传，绝不重试
- 🧹 **流中断处理** - 流式传输中途出错时，追加一个 OpenAI 风格的 SSE 错误块并以
  `data: [DONE]` 干净收尾，客户端不会看到重复或截断的文本
- 📦 **托管模型** - 每个提供商维护模型目录（`{ id, enabled }`），支持从上游
  `/v1/models` 一键导入（带勾选），可选白名单模式拒绝列表外的模型
- 🎯 **模型路由** - 以 `provider/model` 形式请求（如 `openai/gpt-4o`），或在控制台
  配置短**别名**（如 `fast` → `gpt-4o-mini`）
- 📊 **状态与统计** - 请求统计持久化到 `stats.json`：总量、24 小时逐时柱状图，以及
  按 Key / 提供商 / 模型的成功失败计数——控制台可视化，也可 `GET /api/stats` 查询
- 🖥️ **Web 控制台** - `http://127.0.0.1:8001/`，含**状态**与**配置**两个页签：
  管理提供商、Key 池（增删、每 Key 冷却、实时状态、显示/复制按钮）、模型、别名与设置
- 💾 **JSON 配置** - 人类可读的 `gateway.json`，每次修改原子写入
- 📤 **配置导出/导入** - 浏览器直接下载 `gateway.json`；导入采用"先验证后切换"：
  任何错误整份拒绝，合法文件**热切换无需重启**（进行中的请求在旧状态上完成）
- 🔐 **可选鉴权** - 默认关闭；开启后本地 API 要求携带网关签发的 Bearer Key
- 📦 **单二进制** - Web 控制台为单文件原生 HTML/JS，通过 `rust-embed` 内嵌进二进制；
  `cargo build` 一步出产物，无 Node 构建链

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

## 🚀 快速开始

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

安装脚本在缺少 cargo 时会以用户级权限安装 rustup（无需管理员）。

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

## 📖 使用指南

### 配置提供商与 Key

1. 打开 Web 控制台 `http://127.0.0.1:8001/`
2. 进入**配置**页签，添加提供商（名称 + OpenAI 兼容 `base_url`）
3. 为该提供商的 Key 池添加一把或多把 API Key
4. （可选）一键从上游 `/v1/models` 导入模型目录，单独启停模型，并可开启白名单模式
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

打开**状态**页签：请求总量、24 小时逐时柱状图，以及按 Key / 提供商 / 模型的
成功失败计数。统计持久化到 `stats.json`（位于 `gateway.json` 旁边），重启不丢失。

### 备份或迁移配置

- **导出**：点击控制台中的"导出配置"（或 `GET /api/config/export`），浏览器直接
  下载 `gateway.json`
- **导入**：点击"导入配置"（或 `POST /api/config/import`）并选择文件——非法文件
  整份拒绝；合法文件立即热切换，无需重启

## ⚙️ 配置格式

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
      "model_allowlist_only": false,
      "keys": [
        { "id": "k-xxx", "key": "sk-...", "label": "main", "cooldown_secs": null,
          "learned_cooldown": null }
      ],
      "models": [
        { "id": "gpt-4o", "enabled": true }
      ],
      "aliases": { "fast": "gpt-4o-mini" }
    }
  ]
}
```

说明：

- Key 的 `cooldown_secs: null` 表示使用全局默认值
- `learned_cooldown` 由二分探测器自动写入（区间中点，±1 秒精度），优先级高于
  `cooldown_secs`
- `models` 为空时视为"未托管"，所有请求照常放行（向后兼容旧配置）
- 在控制台里做任何修改后，该文件都会被原子性地重写

## 📡 API 概览

### 网关 API（OpenAI + Anthropic 兼容）

| 接口 | 方法 | 说明 |
|------|------|------|
| `/v1/chat/completions` | POST | Chat Completions（流式 + 非流式） |
| `/v1/messages` | POST | Anthropic Messages（流式 + 非流式，tools / 图片） |
| `/v1/messages/count_tokens` | POST | Token 计数（Anthropic 上游透传，OpenAI 上游本地估算） |
| `/v1/models` | GET | 聚合模型列表 |

### 控制台 API

| 接口 | 方法 | 说明 |
|------|------|------|
| `/api/status` | GET | 实时配置 + Key 池状态 |
| `/api/stats` | GET | 请求统计（总量、24h 柱状图、按 Key/提供商/模型） |
| `/api/config/export` | GET | 下载 `gateway.json` |
| `/api/config/import` | POST | 配置热重载（先验证后切换） |
| `/api/providers` | GET / POST | 列出 / 创建提供商 |
| `/api/providers/{id}` | PUT / DELETE | 更新 / 删除提供商 |
| `/api/providers/{id}/keys` | POST | 添加 Key |
| `/api/providers/{id}/keys/{key_id}` | DELETE | 删除 Key |
| `/api/keys/{key_id}/cooldown` | DELETE | 清除 Key 冷却 |
| `/api/providers/{id}/models` | POST | 添加托管模型 |
| `/api/providers/{id}/models/import` | POST | 从上游 `/v1/models` 导入模型 |
| `/api/providers/{id}/aliases` | POST | 设置别名 |
| `/api/settings` | PUT | 更新全局设置 |

## 🧪 测试

项目内置一个模拟的 OpenAI 兼容上游（`sk-bad` 这把 Key 恒定返回 429）：

```bash
cd llm-gateway
cargo run --bin mock_upstream &     # 监听 127.0.0.1:9001
cargo run -- --config test/gateway.json
# 然后测试：
curl http://127.0.0.1:8000/v1/chat/completions -H "Content-Type: application/json" \
  -d '{"model":"mock/mock-large","messages":[{"role":"user","content":"hi"}]}'
```

## 🏗️ 项目结构

```
llm-gateway/
├── src/
│   ├── main.rs    入口，路由（API :8000，控制台 :8001）、落盘定时器
│   ├── config.rs  JSON 配置结构 + 先验证后切换 + 原子持久化
│   ├── state.rs   共享状态 + Key 池（轮询、冷却、无效隔离）
│   ├── proxy.rs   /v1 代理：模型解析、Key 轮换、探测、流式透传
│   ├── stats.rs   请求统计（计数器 + 24h 柱状图，持久化）
│   ├── admin.rs   Web 控制台背后的 REST API
│   └── ui.rs      内嵌 Web 控制台（static/index.html）
├── static/
│   └── index.html 单文件控制台 UI（状态 + 配置页签）
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

## 🛠️ 技术栈

- **Rust**（edition 2021）- 全部后端，单个静态二进制
- **axum + tokio + hyper** - 异步 HTTP 服务与代理
- **reqwest (rustls)** - 上游 HTTPS 客户端，无 OpenSSL 依赖
- **rust-embed** - 控制台 UI 编译期内嵌
- **serde / serde_json** - 配置、统计与 API 序列化

## 🤝 贡献

欢迎提交 Issue 和 Pull Request！

## 📄 许可证

[GPL-3.0](LICENSE)
