# 更新日志

## [v0.8.6-beta2] — 2026-11-11

控制台重写为 React 18 + TypeScript，并修复 token 记账、429/401 分级与配置持久化三类长期问题。构建在本地完成。

### 新增

- 控制台重构：六个视图全部移植，交互与旧版保持一致；构建产物嵌入单文件 exe，构建 exe 不再依赖 Node。
- 主题系统：`themes/<id>/` 下由 `theme.json`、css/js 与可选预览图组成；内置 sakura-dark / sakura-light，支持外部 `themes/` 覆盖与运行时切换，选择持久化到 `gateway.json`。
- 设置项：`auth_invalid_secs`（默认 1h）、`quota_cooldown_secs`（默认 5h）。
- 运行时日志 `sakura-gateway.log`：轮换、冷却、异常同时输出到终端与文件；每请求一行 `[req]`，含 ttfb、首字延迟、总时长、字节数、token 数及来源。
- 配置路径三级兜底：工作目录 → exe 所在目录 → `%LOCALAPPDATA%`，启动时打印实际使用的路径。

### 修复

- **token 记账**：修复流式 usage 缺失的四个成因——OpenAI 未请求 usage、Anthropic 将 usage 拆分在 `message_start` 与 `message_delta`、usage 事件跨 TCP chunk 被切分、客户端收到 `[DONE]` 即断开后回落为本地估算。上游不报 usage 时改用 BPE 分词计数，不再按字符数估算。
- **429/401 分级**：仅 401/403 判为异常（隔离 1h）；配额耗尽改按冷却 5h 处理，优先采用上游 `Retry-After`；限流信号优先于配额标记，避免正常限流被误判为配额耗尽而整池隔离。
- **配置持久化**：写入改为 `写临时文件 → fsync → rename`，保留 `gateway.json.bak`；全零或损坏的配置自动从备份还原；加载失败时区分“读不出来”与“写不下去”。
- **上游连接**：新增 120s 读超时（此前仅 600s 总超时，上游停顿时请求可能挂起十分钟以上）。
- **控制台**：修复预览空白、模型列表左偏、首屏加载失败无重试、提供商探测值不随轮询刷新。
- **启动**：配置不可写时不再静默退出（写 `startup-error.log` 并等待确认）；端口回退等待由约 17.5s 缩短至约 4s。

### 变更

- 策略排序键维持 A–K（开发期间临时新增的 L 已合并删除）。
- 文档：使用说明重写；开发指南改为 React 构建链，补充主题系统与完整性标签排障。

### 升级说明

- 配置向后兼容，旧 `gateway.json` 可直接使用。
- 首次保存后会在配置旁生成 `gateway.json.bak`，为正常的恢复副本。
- 若此前启动横幅显示 `NOT WRITABLE`，建议删除 `%LOCALAPPDATA%\sakura-llmgateway\gateway.json` 中的空配置后重新启动。
- 控制台版本串显示 `v0.8.6-beta2`。

### 已知问题

- 上游首字节耗时波动较大（0.7s–22s），偶发单请求总时长超过 100s。
- TPM 打满时单个请求会轮换多把 Key，属预期行为。
- 从受限环境复制的 exe 可能带 Low 完整性标签，需执行 `icacls /setintegritylevel Medium`。

实测：长上下文请求的记账结果与上游 usage 一致（556,886 token 量级）；单元测试 85 项通过。
