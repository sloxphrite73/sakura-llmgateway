# 开发指导：系统目录配置同步 + 冷却探测修复

> 状态：方案已评审定稿（grilling 会话），待实现。
> 范围：两个独立工作项——(A) 配置文件的系统目录中央仓库与自动采纳；(B) 智能冷却二分探测器的正确性修复。
> 两者无耦合，可分开实施、分开发布。

---

## A. 系统目录配置同步

### A.1 背景与目标

Windows 桌面用户的路径是"下载 exe → 放进空文件夹 → 双击"。配置 `gateway.json`
生成在 exe 旁边；用户更新版本时若换了文件夹，配置"丢失"（实际是没带过来）。

目标：引入一个**系统目录中央仓库**，日常运行仍完全读写本地 `./gateway.json`，
中央仓库只做两件事：

1. **冷备份**：用户点按钮时把本实例配置 push 上去；
2. **新实例引导**：本地配置缺失/为空时，自动从中央仓库采纳最新条目。

Android 不涉及（`filesDir` 随应用升级天然持久）。三桌面平台统一实现。

### A.2 数据流

```
日常运行：./gateway.json  ◀──读/写──▶  网关（现状不变）
                                         │
用户点按钮 ──push（config + stats）──▶  系统目录 instances.json
                                         │
本地配置缺失/为空 ──pull（采纳时间戳最新条目）──▶  物化为本地 ./gateway.json
```

### A.3 决策记录（已拍板，勿再变更）

| # | 决策 | 结论 |
|---|------|------|
| A-D1 | 系统目录路径 | Windows `%LOCALAPPDATA%\sakura-llmgateway\`；Linux `~/.config/sakura-llmgateway/`；macOS `~/Library/Application Support/sakura-llmgateway/`。不引入 `dirs` crate，手写 env 解析（约十几行），保持零新依赖 |
| A-D2 | 中央文件名 | `instances.json`，单文件存所有实例条目 |
| A-D3 | 配置解析优先级 | `--config` > 本地 `./gateway.json` > 系统目录（仅兜底采纳）。本地存在时中央仓库完全不参与读写 |
| A-D4 | 实例身份 | 首次同步时生成随机 `instance_id`（uuid v4，项目已有依赖）写入本地 `gateway.json` 顶层字段；中央条目附带 exe 路径作为展示标签 |
| A-D5 | 采纳触发 | 本地文件不存在，或解析成功但 `providers` 为空。0 字节/坏 JSON 的本地文件先隔离为 `gateway.json.corrupt-<时间戳>` 再视为缺失 |
| A-D6 | 采纳来源 | 中央文件中 `synced_at` 时间戳最新的条目（**不是**按网关版本号） |
| A-D7 | 采纳物化 | 采纳后立即写成本地 `./gateway.json`（复用现有原子写），之后回归纯本地模式 |
| A-D8 | 同步动作 | 控制台设置页按钮「同步数据到系统目录（更新时会自动采纳）」，旁显上次同步时间。上行内容 = config + stats（复用导出的序列化逻辑 `state.rs::serialize_merged`） |
| A-D9 | 中央写入 | 重新读入 instances.json → 内存合并本实例条目 → tmp+rename 原子写（复用现有模式）。不加文件锁 |
| A-D10 | 损坏与清理 | 中央文件损坏 → 改名 `instances.json.corrupt-<时间戳>` 隔离；每次同步时顺手删除 `synced_at` 超过 180 天的条目 |
| A-D11 | 脚本 | `start.bat/sh` 不动（显式 `--config gateway.json` 与本地优先兼容）；`install.bat/sh` **停止预生成**默认 `gateway.json`（否则永久挡住采纳），README/使用说明同步更新 |
| A-D12 | debug 闸 | `cfg!(debug_assertions)` 构建维持 `./gateway.json` 默认值，不读不写系统目录；仅 release 启用新逻辑 |
| A-D13 | 非目标 | 不做运行期自动上行；不加环境变量覆盖；中央文件不加密（与本地明文现状一致）；不多实例实时合并 |

### A.4 实现要点

**新增模块** `src/central.rs`：

```rust
/// 中央仓库条目（instances.json 的一个元素）
struct CentralEntry {
    instance_id: String,          // uuid v4
    exe_dir: String,              // 展示标签
    synced_at: u64,               // epoch 秒，采纳排序的唯一依据
    gateway: serde_json::Value,   // 完整的合并 gateway.json 内容（config + stats）
}

/// 系统目录解析。debug 构建返回 None（A-D12）。
fn central_dir() -> Option<PathBuf>;

/// 启动兜底采纳：本地缺失/为空时调用。返回采纳到的 Config（已物化）。
fn try_adopt_from_central(local_path: &Path) -> Option<Config>;

/// 按钮上行：重读 → 合并本实例条目（按 instance_id 替换）→ 清 180 天旧条目 → 原子写。
fn sync_to_central(app: &App) -> Result<()>;
```

**改动点**：

- `config.rs`：`Config` 增加 `instance_id: Option<String>` 与 `last_sync_at: Option<u64>`，
  均 `#[serde(default)]`；首次同步时回填 `instance_id` 并落盘
- `main.rs`：配置加载失败/为空 → 走 `try_adopt_from_central` → 物化 → 正常加载
- `admin.rs`：新增 `POST /api/config/sync-central`；`GET /api/status` 带出
  `last_sync_at`；设置页加按钮（文案已定：「同步数据到系统目录（更新时会自动采纳）」）
- `install.bat` / `install.sh`：删除预生成 `gateway.json` 的分支
- `state.rs`：`save_all` 不变（中央写走独立路径，不动热路径）

**边界用例**（测试清单）：

1. 本地不存在 + 中央有 2 条目 → 采纳 `synced_at` 最新者并物化
2. 本地 `providers: []` → 触发采纳；本地有 providers → 不触发
3. 本地坏 JSON → 隔离后采纳；中央坏 JSON → 隔离中央、按无中央处理、正常空配置启动
4. `--config foo.json` 显式指定 → 完全不碰系统目录
5. debug 构建 → 完全不碰系统目录
6. 同步两次 → 中央只有一条本实例条目（按 `instance_id` 替换，不追加）
7. 中央存在 200 天旧条目 → 同步后被清除
8. `synced_at` 相同的两条目 → 任取其一（确定性 tie-break：instance_id 字典序）

---

## B. 智能冷却二分探测修复

### B.1 背景

`spawn_prober`（`src/proxy.rs:1048`）在 Key 被 429 后用二分搜索测量真实限流窗口，
写入 `learned_cooldown`。评审发现 4 个正确性/健壮性问题。

### B.2 问题清单与修复

**B-1（正确性，必修）时间基准漂移。**
当前每轮 `probe(mid)` 从"现在"再睡 `mid` 秒，但限流窗口锚定在触发 429 的请求上，
探测之间墙钟累积。例：真实窗口 100s、T=60 → 第 1 轮 t=60 处 429（bracket [60,120]）；
第 2 轮再睡 90s，探测发生在 t=**150**，成功被解释为 `high=90`，最终写入 75。
轮数越多低估越狠——恰是"精度最高"的路径错得最多。
**修复**：spawn 时记录 `t0 = Instant::now()`，每轮睡到 `t0 + mid`
（`sleep(mid.saturating_sub(t0.elapsed()))`），让 `mid` 语义统一为"距 429 触发点的时长"。

**B-2（策略，必修）写入上界而非中点。**
`(low+high)/2` 有约 50% 概率短于真实窗口。低估的后果被 `RELEARN_AFTER_SECS = 3600`
放大：冷却到期 → 真实请求再 429 → 按偏小的 learned 值再冷却 → 重学被守卫拒绝 →
一小时内循环自伤。冷却场景"宁多等勿再撞"。
**修复**：`learned = high`（收敛后 `high` 是已知必成功的最短等待）。可再加
`+1s` 固定余量。

**B-3（边界，必修）触顶假精确。**
T=900（`PROBE_MAX_SECS`）仍 429 时，`high = (t*2).min(900) == low`，bracket 塌缩，
写入 learned=900 仿佛测量成功；真值可能是按小时/按天的窗口，之后每 900s 必撞一次 429。
**修复**：首轮探测在 `PROBE_MAX_SECS` 仍 429 → 判定**不可学**，不写 `learned_cooldown`，
该 Key 退化为 `Retry-After`/指数退避默认值（本次先用现有默认链即可，
指数退避留作后续增强）。

**B-4（健壮性，应修）失败探测无负缓存。**
首轮探测 Err（401/403/400/网络错）时什么都不写直接返回；下一次 429 又 spawn
一个探测器。一个"永远失败"的 Key 会在每次 429 时反复跑探测（睡 T 秒 + 一次请求）。
**修复**：探测失败也记录 `learned_at`（或单独的 `probe_attempted_at`），
让 `RELEARN_AFTER_SECS` 守卫对失败尝试同样生效。

### B.3 顺带改进（同批实施，成本低）

- **B-5 探测模型**：用触发 429 的那个模型（`row.upstream_model`），
  而不是"第一个 enabled 托管模型"（`proxy.rs:497`）——很多上游限流是 per-model 的
- **B-6 探测期排除路由**：探测期间 Key 保留在 cooling 状态直到 prober 结束
  （防止冷却旧值先于探测结束到期，真实流量与探测互相污染测量）

### B.4 后续增强（不在本次范围，记录备查）

- **解析 `x-ratelimit-*` / `retry-after-ms` 响应头**（OpenAI/Anthropic/Groq 均有），
  精确且零探测成本；探测降级为无头时的兜底。插入优先级链：
  `Retry-After` > ratelimit 头 > learned > 每 Key 覆盖 > 全局默认
- **learned_cooldown 按 (key, model) 维度存储**，取代纯 per-key
- **AIMD 自适应**：429 则冷却 ×2（封顶）、连续成功则 ×0.5 衰减，
  作为探测的替代或补充——零探测成本、天然适应窗口漂移
- `learned_at` 持久化（当前进程内 `Instant`，重启后守卫清零）

### B.5 验证方式

`src/bin/mock_upstream.rs` 已支持"前 N 秒 429 后放行"的窗口模拟
（README 记录其用于冷却学习测试）。为它加一个**固定窗口模式**（如 100s），
断言修复前后 learned 值的差异：修复前应系统性低估（~75），修复后应落在
`[100, 101]`（写 high + 余量）。另补：触顶用例（窗口 > 900s → 不写 learned）、
失败负缓存用例（连续两次 429 之间只 spawn 一次探测）。

---

## 实施顺序建议

1. **B-1 + B-2 + B-3 + B-4**（冷却修复，一个 PR，带 mock 测试）——正确性问题，优先
2. **B-5 + B-6**（可并入 1，视 PR 大小）
3. **A 系统目录配置同步**（独立 PR：central.rs + main/admin/install 脚本 + UI 按钮）
4. 文档更新：README 双語 + `doc/使用说明.md` / `doc/USAGE.md` 的快速开始段落
   （install 不再预生成配置、更新时自动采纳的说明）
