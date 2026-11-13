# 实验分支：TPM 429 短线轮换（exp/tpm-fast-rotate）

## 现象与证据

长上下文场景下，`[key] rate-limit cooldown` 全部是 **60s**，17 把 Key 在一分钟内被逐个打满，
之后请求进入 hold；同时出现大量 `network error: error sending request`（每次 benched 5s），
客户端感觉"轮询很慢、还有 Key 闲着没被用"。

生产日志统计（`L:\sakura-dev\run6\sakura-gateway.log`，2654 行）：

| 项 | 数量 |
|---|---|
| 冷却事件总数 | 1175 |
| 其中 60s | **1175（100%）** |
| 429 原文 `inference exceeds tpm/rpm limit` | 927 |
| 429 原文 `tpm exhausted` | 201 |
| 429 原文 `rps exhausted` | 47 |
| 401/403 invalid | 7 |

关键前提（用户观察 + 本次据此设计）：**被 429 拒绝的请求在上游不计入 RPM/TPM**。
也就是说，失败的那次尝试没有花掉配额，网关却按"最坏情况"给它算了一次消耗，并把它记进自己的 60s 窗口。

## 根因

1. **窗口推算假设"下一次还会吃 avg_req"**：`window_recovery_secs` 里 `total + avg_req > cap`，
   长上下文时 `avg_req` 本身就能顶满一分钟窗口，于是**任何** 429 都推到 60s 上限（×0.9 → 54s，再落到 60s 档）。
2. **失败的尝试被计入我们自己的 RPM 窗口**：`pick_key`/`select_strategy` 在派发时就 `req_times.push_back`，
   429 后没有撤销 → 我们的窗口比上游真实用量虚高，后续估算继续偏长。
3. **传输层失败（连不上/被重置）被当成 5s 冷却**，且和配额尝试共享预算：一次抖动就能把 17 把 Key
   在几秒内全部 benched，而它们其实**一点配额都没花**。

## 分支改了什么

- 新增 `rate_limit_cooldown_secs`（本分支默认 **5s**）：给窗口推算设上限，**只缩短不延长**；
  `Retry-After` 与每 Key 的 `cooldown_secs` 仍然优先。
- 新增 `rate_limit_assume_free`（本分支默认 **true**）：推算只用"已消耗量"（`total - cap`），
  不再加 `avg_req`。
- `mark_rate_limited` 先**退还**本次尝试的 RPM 样本，再采样诊断。
- 新增 `transport_error_cooldown_secs`（默认 **1s**，原为硬编码 5s）与 `max_transport_retries`
  （默认 24）：传输层失败**不占配额**，所以单独预算、短冷却；超过预算才给客户端 502。
- 传输层失败同样退还 RPM 样本。

## 预期信号（怎么判断有没有用）

跑一段时间后看日志：

| 指标 | 基线 | 期望 |
|---|---|---|
| `rate-limit cooldown` 秒数 | 几乎全是 60s | 大量 5s |
| 每请求 `rotate` 次数 | 少 | **变多**（这是设计目标：用轮换换吞吐） |
| 客户端 hold 时间 | 长 | 短 |
| `[req] total=` 中位耗时 | 高 | 下降 |
| 成功率 | — | 不应下降（若下降说明 429 后重试确实无益） |

> 如果 429 后短时间内重试**仍然全 429**，说明上游的 TPM 是"整分钟桶"（等待并不会更快恢复），
> 这时更短的冷却只是把同样的失败提前重放一遍，吞吐不会提升——那就把 `rate_limit_cooldown_secs`
> 调回 30–60s，只保留 `assume_free` + 退还样本（这两条是无损的）。

## 怎么试

1. 新二进制：`L:\sakura-dev\run6\gw-tpm-exp.exe`（与当前运行的 `gw6.exe` 并存，不覆盖）。
2. 想改强度不改代码：`gateway.json` 里加
   ```json
   "rate_limit_cooldown_secs": 5,
   "rate_limit_assume_free": true,
   "transport_error_cooldown_secs": 1,
   "max_transport_retries": 24
   ```
3. 回滚：删掉这几个字段即回到旧行为（默认值仍是 5s/true 的只在**本分支**上；合回 main 前需确认默认值）。

## 合并前要决定

- 这两条默认值是否保留（`Some(5)` / `true`）——激进策略适合长上下文重度用户，但对 RPM 敏感的场景
  可能制造更多 429；建议合并时把默认值保持旧行为（`None` / `false`），由配置开启。
