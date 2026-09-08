# Sakura LLM Gateway

[English](README.md) | 简体中文

一个用 **Rust**（axum + tokio）编写的高性能本地 LLM API 网关。它在本机暴露一个
OpenAI 兼容的接口（`127.0.0.1:8000/v1`），对每个提供商维护一个 **API-Key 池**，
遇到 429 限流自动切换下一个 Key 继续请求，并内置一个单文件 Web 控制台，
用于管理提供商、Key、模型和别名。

## 功能特性

### OpenAI 兼容 API
任何支持 OpenAI 协议的 agent / 工具都可以直接接入 `http://127.0.0.1:8000/v1`：

- `POST /v1/chat/completions` — 同时支持流式（SSE）与非流式输出
- `GET /v1/models` — 返回所有提供商的托管模型 + 别名聚合列表

### API-Key 池（核心特性）
- **轮询调度**：同一提供商的多把 Key 自动轮换使用
- **429 自动切换**：某把 Key 被限流后立即冷却并切到下一把 Key，请求不中断
- **冷却时长**优先级：上游 `Retry-After` 响应头 → 每 Key 独立配置 → 全局默认（60 秒）
- **全部 Key 冷却时**：立即向客户端返回 429 并附说明（快速失败，不阻塞等待）
- Web 控制台实时显示每把 Key 的冷却倒计时、请求计数与最近错误

### 首字节前重试
- 收到响应首字节之前：429 / 5xx / 超时 / 连接失败都会换 Key 重试（默认最多 3 次）
- 首字节已发出后：上游流原样透传，绝不重试——保证流式输出不会出现重复或断裂文本

### 模型管理
- 每个提供商维护一份**托管模型列表**（`{ id, enabled }`，可单独停用而不删除）
- **从上游一键拉取**：调用提供商的 `/v1/models`，勾选导入（支持全选）
- 可选**白名单模式**（默认关闭）：开启后不在列表内的模型请求直接拒绝
- **别名**：在控制台给模型起短名（如 `fast` → `gpt-4o-mini`），请求时直接使用别名

### Web 控制台
打开 `http://127.0.0.1:8001/` 即可：

- 管理提供商（增删改、编辑 baseURL）
- 管理 Key 池（添加/删除、每 Key 冷却时间覆盖、实时状态）
- 管理托管模型（手动添加、从上游拉取导入、启停、白名单开关）
- 管理别名与全局设置（默认冷却时间、重试次数、端口、可选鉴权）

### 其他
- **JSON 配置**：人类可读的 `gateway.json`，每次修改原子写入，也可直接手改
- **可选鉴权**：默认关闭；开启后本地 API 要求携带网关签发的 Bearer Key
- **单二进制**：Web 控制台为单文件原生 HTML/JS，通过 `rust-embed` 内嵌进二进制，
  `cargo build` 一步出产物，无 Node 构建链

## 快速开始（一键脚本）

**Windows：** 双击 `install.bat`，完成后双击 `start.bat`。
**Linux / macOS / Git Bash：**

```bash
./install.sh   # 检查/安装 Rust、编译 release 版本、生成默认 gateway.json
./start.sh     # 如未构建则自动构建，然后启动网关
```

安装脚本在缺少 cargo 时会以用户级权限安装 rustup（无需管理员）。

## 手动构建

需要 Rust（Windows 上 GNU 工具链即可）：

```bash
cargo build --release
```

## 运行

```bash
./target/release/llm-gateway.exe            # 配置默认为 ./gateway.json
./target/release/llm-gateway.exe --config path/to/gateway.json
```

端口（API `8000`、控制台 `8001`）可在配置文件中设置，也可在控制台里修改
（修改端口后重启生效）。

## 配置格式

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
        { "id": "k-xxx", "key": "sk-...", "label": "main", "cooldown_secs": null }
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
- `models` 为空时视为"未托管"，所有请求照常放行（向后兼容旧配置）
- `aliases` 把客户端可见的名称映射到上游模型名；别名指向已删除/停用的模型时
  会返回明确的 404 错误
- 在控制台里做任何修改后，该文件都会被原子性地重写

## Agent 接入示例

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

## 测试

项目内置一个模拟的 OpenAI 兼容上游（`sk-bad` 这把 Key 恒定返回 429）：

```bash
cargo run --bin mock_upstream &     # 监听 127.0.0.1:9001
cargo run -- --config test/gateway.json
# 然后测试：
curl http://127.0.0.1:8000/v1/chat/completions -H "Content-Type: application/json" \
  -d '{"model":"mock/mock-large","messages":[{"role":"user","content":"hi"}]}'
```

## 项目结构

```
src/
  main.rs    入口，路由（API :8000，控制台 :8001）
  config.rs  JSON 配置结构 + 原子持久化
  state.rs   共享状态 + Key 池（轮询、冷却跟踪）
  proxy.rs   /v1 代理：模型解析、Key 轮换、流式透传
  admin.rs   Web 控制台背后的 REST API
  ui.rs      内嵌单文件 Web 控制台（static/index.html）
```

## 许可证

[GPL-3.0](LICENSE)
