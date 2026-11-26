# 长程开发 TTFT 基准（Long-Term-Development TTFT Bench）

长程开发速度测试。「跑自测」默认指本基准。

## 测试环境

+ **Harness**：本地的 **DeepSeek Harness**（headless 档）
+ **模型**：`sensenova/glm-5.3`（经待测网关实例转发）
+ **测试仓库**：`sakura-llmgateway` 最新版本的**一次性克隆**（agent 会在里面做真实提交，跑完丢弃；绝不能在真仓库里跑）

## 测试要求

将待测试实例（当前标准基线：`strategy.mode=ttft` · `hedge_pool_cap=5` · `hedge_header_deadline_secs=2` · 生产配置副本，独立端口，如 8100/8101，**绝不触碰生产实例**）接入 **DeepSeek Harness**，并在该环境中逐一向模型发送以下提示词，记录模型每一个消息的 TTFT 和 **DeepSeek Harness** 中显示的 TTFT 平均值。模型必须能完成工具调用等操作。

**TTFT 口径**（与 DSH 自身统计一致，`dsh-session-stats`）：每一步（step）从 `step_start` 到第一个输出片段的耗时；「平均值」= 全部步骤 TTFT 的平均（即 DSH 统计对话里的「首 token 平均（TTFT）」）。

以下是提示词列表，需顺序发送：

+ 你好
+ 介绍一下你自己，你是什么模型？
+ proxy.rs 代码逻辑梳理一下
+ 梳理策略层的实现
+ 梳理 UI 的实现
+ 梳理统计的实现
+ 了解 ui.rs
+ 整理一下这个项目鲁棒性具体在哪里体现和有什么不足之处
+ 梳理一下关于日志这一块的代码
+ 整理一下目前的对冲策略逻辑
+ 梳理一下经典模式的代码逻辑
+ 梳理一下主题的代码逻辑
+ 把项目代码里面记录的注释整理成一份bug文档
+ 可以落到仓库里，直接提交
+ 梳理一下这个仓库的提交历史
+ 你改一下readme.md，删掉里面的必须署名这个规定然后提交。
+ 彻底放开署名要求。
+ 你自己看看还有啥可以做的

这些消息全部发送完后预计上下文在 ~300K 规模。目前标准：**avg_TTFT** 需在 5s 及以下。**最高**不超过 10s。

## 自动化执行（headless）

基准以 `dsh --profile headless` 全自动执行：

```powershell
# 1. 待测实例就绪（例：8100/8101，ttft · 池5 · 2s）
# 2. 一次性克隆最新仓库作为 agent 工作目录
# 3. 运行（runner 内置 18 条提示词、会话串联、按步 TTFT 采集、标准判定）
$env:SAKURA_BENCH_KEY = 'bench-placeholder'   # 网关无鉴权，占位即可
python run_bench.py
```

要点：
+ 覆盖层把 `llm-pi-ai` 路由指向待测网关（`api: openai-completions` + `baseURL` + `compat.thinkingFormat: deepseek` 解析 glm 的 `reasoning_content`），`agent-default-model` 选 `sensenova/glm-5.3`——CLI 层 `--patch` 只作用于当次调用，不影响任何用户配置。
+ headless 每条提示词一个进程，`--session-id` 串联成一个会话，上下文逐条累积。
+ 提交类提示词需要 `--permission-mode workspace-write --add-dir <repo>\.git --approval-policy never`。
+ 报告输出每步 TTFT、每轮墙钟、usage、avg/max 与 5s/10s 标准的判定；网关日志（`[req]` 赢家行、`hedge rotate` 行）作诊断层。

runner（`run_bench.py`）与覆盖层（`sakura-bench.patch.yml`）目前为基准工作区工具（`L:\sakura-dev\bench\`），与仓库分离；如需固化进仓库另行决定。
