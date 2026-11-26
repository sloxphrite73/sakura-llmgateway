# Sakura LLM Gateway · 事故档案（Bug Ledger）

> **来源**：本文档整理自 `llm-gateway/src/*.rs` 与 `ui/src` 内嵌注释。作者把注释当事故档案用：现象、根因、修复三件套，关键案例附生产真实样本的回归测试（多条测试注释直书 "the exact production failure"）。本文档为注释自述的转录整理，非独立复现。
> **整理日期**：2026-11-22（对应 commit d6553d9）
> **编号体系**：`F0–F11` = 内联修复编号；`PASS §36/§38` = 带教训的测试通过批次；`D1/D3` = 策略层设计决策；`§30.3` = 实现规范条目（「高估冷却 = 停摆 key，禁方向」）。

---

## 一、P0 · 生产宕机 / 数据损坏 / panic

### BUG-01 · 中文流被 panic 当场杀死

- **症状**：中文内容的长流中途死亡，客户端看到 `terminated`。
- **根因**：`trim_tail` 用**原始字节偏移**调 `String::drain`，切口落在 UTF-8 多字节字符内部 → panic，整个流式转发任务阵亡。（proxy.rs `trim_tail`）
- **修复**：回退到最近字符边界再切，允许超出预算几个字节——"buffer is diagnostic, not a hard budget"。
- **证据**：`tail_trim_never_splits_a_multibyte_char`——233 个「中」+ "ab" = 701 字节，`len-600=101` 恰好落在字符中间。即 commit d6553d9。

### BUG-02 · gateway.json 两次变成 23 KB 全零

- **症状**：配置文件（连同全部 key）「长度正确但全是 NUL 字节」——生产中发生过两次。
- **根因**：直接覆写时硬 kill 落在写入中途；无 fsync 的 rename 在 NTFS 上提交了 size + 目录项但数据块仍留在 page cache 脏页，断电即 NUL。（config.rs `write_atomic` 头注释）
- **修复**：`write_atomic`（temp 写完 + `sync_all` 之后才 rename）；`.bak` 每分钟最多刷一次且**只晋升仍能解析的文件**；`load_strict` 专门识别 NUL 形状并从 `.bak` 恢复（原样恢复，避免重序列化把 stats 块清零）。
- **证据**：测试断言错误消息必须提到 NUL。

### BUG-03 · ~40% 的流记成 0 token

- **症状**：大量请求记为 `success` 但 token 数为 0，统计失真；无 usage 时控制台「看起来冻结」而实际请求在成功。
- **根因**：SSE 的 usage 行被 TCP 拆到两个 read，逐 chunk 解析「被解析两次但从未成功」。（proxy.rs `take_complete_lines` / `MetricsStream`）
- **修复**：跨 chunk 行重组缓冲 `carry`（64 KB 失控上限）+ 无 usage 时真 BPE 本地计数兜底，**绝不记 0**。
- **证据**：`the final usage event is split by TCP`、`usage_split_across_two_chunks_is_still_captured`。

---

## 二、P1 · 全池状态被错误污染

### BUG-04 · 健康 key 全池被 bench 5 小时

- **根因**：只信 429 body 的 `code: "insufficient_quota"`；而 sensenova 把普通 tpm/rpm 限流也打这个 code → 整池健康 key 被按额度耗尽 benched 5 小时。（proxy.rs `is_quota_exhausted`）
- **修复**：**消息内容优先**——`"exceeds tpm/rpm"` 等关键词 → 限流窗口；`entitlement exhausted`/`余额不足` 等 → 额度耗尽。用户规则直接写进测试：**「其他的 429 只要出现 rpm 就还是冷却」**。
- **证据**：`real_sensenova_*` 三个测试用的全是生产捕获的 body 原文。

### BUG-05 · 样本驱动容量三次打崩全池（PASS §38 的由来）

- **根因**：用 429 采样估计容量甚至触发冷却——但一次 429 只能说明 `rpm≥limit ∨ tpm≥limit`，**无法定位是哪个维度**，错误归因级联成全池下线。（state.rs `fill_rows`）
- **修复**：容量真相只来自配置（`seed_rpm`/`rpm_limit`）；样本降级为 K 的**软排序信号**（`tpm_obs`：成功反证即清、TTL 老化、绝不进冷却）。

### BUG-06 · 主动探测器适得其反

- **根因**：探测 key 是否恢复的主动 prober **烧被限流 key 自己的 RPM** 去测量，且 1-token 探测永远对不上真实 payload。（state.rs `mark_rate_limited`）
- **修复**：移除 prober，改被动窗口排空估算——"real requests after cooldown ARE the probe now"。

### BUG-07 · 401/403 波级联成全池 invalid【注释与实现存在漂移】

- **症状**：单次瞬时 auth 波把所有 key 隔离，网关整体瘫痪。（state.rs `PoolRuntime::invalid`）
- **注释自述**：设计为「连续**三次** 401/403 strike、中间无成功才隔离」。
- **⚠ 漂移**：代码实现是**单次** 401/403 即 `mark_invalid`；全仓搜索 `strike` 只命中该注释。三振机制只存在于注释（见第九节）。 **漂移注释已于 6d25045 修正。**

### BUG-08 · Anthropic 流式 usage 全部丢失

- **根因**：旧提取器要求 input/output 在同一对象里；Anthropic 把 input 放 `message_start`（且**嵌套在 `message.usage` 下**）、output 放 `message_delta`（顶层）→ 全部 miss。（protocol.rs `extract_usage_partial`）
- **修复**：部分提取 + 跨事件合并，嵌套/顶层两处都取。
- **证据**：`partial_anthropic_message_start_nested_usage` 注释："streamed Anthropic requests recorded 0"。

### BUG-09 · 「200 包 auth 错误 body」的供应商

- **症状**：有的供应商不返回真 401，而是 HTTP 200 + 错误 body——key 实际已失效却一直被路由。
- **修复**：非流式缓冲路径识别 auth 族关键词（`invalid_api_key`/`unauthorized`/`"401"` 等）→ 按 401 同等隔离。（proxy.rs `finish_generic`）

---

## 三、P1 · 统计与估算自激

### BUG-10 · 所有冷却都精确等于 60s

- **根因**：窗口排空估算要覆盖 `total + avg_req`，长上下文的 `avg_req` **单独就能撑满 60s 窗口** → 无论真实情况，估算永远是 60s。（state.rs `window_recovery_secs`）
- **修复**：`rate_limit_assume_free`——被拒的请求没花 TPM，排空只需覆盖实际消耗。

### BUG-11 · 429 不退款 → 估算自激

- **根因**：429 时的 RPM 采样不退 → 自己的窗口虚高 → 下次估算更长 → 正反馈越来越长。
- **修复**：`mark_rate_limited` 第一步先退款采样（`req_times.pop_back`），再记 `tpm_obs`、算冷却。

### BUG-12 · 10K 请求只报 7.5K token

- **根因**：`[DONE]` 即关连接的正常客户端让 hyper 直接丢 body，走不到记录分支；Drop 补记路径又用了**本地估算**而非已见过的精确 usage。
- **修复**：Drop 路径优先用已见到的上游 usage。
- **证据**：`stream_closed_after_done_records_upstream_usage_not_estimate`（注释写明 7.5K/10K 事件）。

### BUG-13 · 1/1 的新 key 压过 99/100 的老 key

- **根因**：裸成功率排序对小样本极不公平。
- **修复**：贝叶斯平滑 `(ok+β)/(n+β)`，β=2。（state.rs K 键）

### BUG-14 · 没被路由的 key 被陈旧样本永久压低（F5）

- **根因**：滚动窗口只在 push 时修剪；停止被路由的 key 陈旧样本永远在——rpm 虚高、成功率是过期窗口值，K 的 headroom 被冤枉地压低。
- **修复**：读路径（`fill_rows`）也修剪过期窗口。

---

## 四、P2 · 性能退化与误导性故障

### BUG-15 · 大上下文请求「越轮越慢」

- **根因**：读超时被当网络故障轮转——每个 retry **重发几百 KB prompt 再等同样长的 prefill**，轮转成了放大器。（proxy.rs 经典模式超时分支）
- **修复**：超时 = 上游 prefill 中，不轮转，诚实 504，key 不记过。

### BUG-16 · 长上下文 BPE 阻塞整个 tokio worker

- **症状**：worker 上**其他所有请求**跟着 stall——表象是「网关停止轮转」，极难归因。（proxy.rs est_in 注释）
- **根因**：400K token 的 BPE（秒级纯 CPU）内联跑在 async 上下文。
- **修复**：`spawn_blocking` + `OnceLock` 每请求只算一次。

### BUG-17 · 上游沉默数分钟才回响应头

- **根因**：旧硬编码读超时太短，每次 retry 都撞同一堵 prefill 墙，长上下文请求全军覆没。（config.rs `upstream_read_timeout_secs`）
- **修复**：可配置，默认 300s——"covers the observed worst"。

### BUG-18 · 烂网络一次烧穿全池

- **根因**：传输错误冷却硬编码 5s——连接抖动一阵就把整个池 bench 光，**每个**客户端请求 stall。（config.rs `transport_error_cooldown_secs`）
- **修复**：独立传输预算（不占「花过额度的尝试」）+ 默认冷却降到 1s + 退款 RPM 采样。

### BUG-19 · 每次轮转白等 1.5s

- **根因**：1500ms 礼貌间隔是单 provider 时代防账户级 TPM 级联的产物；实际池是 per-key 限的，纯客户端可见延迟。
- **修复**：降到 250ms（经典模式保留，对冲模式删除）。

### BUG-20 · 「挂了 25 分钟」无法排查

- **根因**：双击的控制台窗口一关（或滚屏），证据全丢，无法远程诊断。（logger.rs 模块头）
- **修复**：终端 + 文件双写日志（`sakura-gateway.log`，每行 flush）。

### BUG-21 · Notepad 的 BOM 让 serde 拒收

- **修复**：`load_strict` 剥 UTF-8 BOM。（config.rs）

### BUG-22 · chars/4 粗估 token

- **修复**：响应 token 估算从「每 4 字符 ≈ 1 token」换成真 BPE（`tiktoken-rs`）。（protocol.rs NOTE）

---

## 五、P2 · 启动与可用性

### BUG-23 · 端口被占直接崩溃 + 17.5s×2 像挂死

- **根因**：双击第二个实例 → AddrInUse 崩溃；旧重试预算 17.5s × 2 端口 = 35 秒的「假 hang」。（main.rs `bind_with_fallback`）
- **修复**：~4s 指数退避重试窗（覆盖 Android 服务重启竞态的瞬时占用）+ 走上探第一个空闲端口，**永不因 AddrInUse 崩溃**。

### BUG-24 · exe 目录不可写（os error 5）

- **现象**：锁定机器上 exe 自己的目录恰好不可写——连 fallback 日志写入都被拒绝过。（main.rs 配置候选注释）
- **修复**：多级候选位置（配置目录 → exe 目录 → LOCALAPPDATA），**永不因配置不可写拒绝启动**；已有配置文件永远原址使用（绝不静默搬家）。

### BUG-25 · 错误消息把读者带向歧途

- **根因**：NUL 损坏文件曾被报成 "cannot be written"——**「不可读」和「不可写」指向完全不同的修复**，绝不能互报。（config.rs `load_strict`、main.rs `ConfigFailure`）
- **修复**：`ConfigFailure` 带 label/detail 区分两类；NUL 文件报 "truncated by a hard kill or an interrupted copy"。

### BUG-26 · 双击窗口闪退看不清错误

- **修复**：`fatal()` 写 `startup-error.log`（exe 目录 → LOCALAPPDATA 两级尝试）+ `is_terminal()` 门控的「按回车关闭」；管道/服务/CI 下绝不等待。（main.rs）

---

## 六、P2 · UI / 前端

### BUG-27 · vite 热重启后代理失灵 → 空面板

- **根因**：非根 base 下配置热重启后 dev server 不再拦截 `/api` → SPA fallback → 板块全空。（vite.config.ts）
- **修复**：dev 用 `/`、build 用 `/static/` 双 base。

### BUG-28 · localhost 只解析到 ::1

- **根因**：vite 默认 'localhost' 只绑 IPv6，先试 127.0.0.1 的浏览器直接挂。（vite.config.ts）
- **修复**：显式绑 127.0.0.1。

### BUG-29 · 首载竞态 → 永久空壳

- **根因**：首载撞上网关未就绪/代理抖动，控制台永远空着。（store.tsx）
- **修复**：每 3s 重试直到 status 就绪。

### BUG-30 · 第一次 dry-run 被跳过

- **根因**：`stateRef` 要渲染后才更新，refresh 里第一次 `runDryRun` 读到旧空 `reqModel`。
- **修复**：显式传新鲜选择的值（注释直书原因）。

### BUG-31 · 主题闪白

- **修复**：`index.html` 首绘前 boot injector 从 localStorage 应用缓存的 manifest。

### BUG-32 · Android 转屏丢状态

- **根因**：WebView 转屏 = 整页重载。
- **修复**：UI 状态（页签/草稿/过滤器）localStorage 持久化。

---

## 七、兼容性与边角

| # | 事故 | 修复 |
|---|---|---|
| BUG-33 | count_tokens 盲用 `keys[0]`（F3）——冷却/无效时 401 | 首个可用 key；OpenAI 上游本地估算兜底，**绝不 404**（Claude Code 依赖） |
| BUG-34 | 请求自身 4xx 被记到 key 头上（F1）——dents 成功率/K | 不归属 key，只记请求失败 |
| BUG-35 | Retry-After 截断报 0s（F11）——冷却剩 300ms 报 "0s left" 骗客户端立即重试 | `ceil_secs` 向上取整 |
| BUG-36 | 并发请求选中同一行（F8）——读游标→select→写游标不在同一锁内 | 一把池锁包全场（fill+读写游标+select+采样） |
| BUG-37 | 旧版控制台 PUT 时**静默重置**新设置 | 字段缺省 = 保持现状（admin.rs update_settings） |
| BUG-38 | 配置导入丢 stats——`Config` 解析器没有 `stats` 字段被静默丢弃 | admin 先解析 raw Value 抽取 stats 再合并 |
| BUG-39 | woff2/json 曾 fallback 到 octet-stream | ui.rs 补齐 mime 表（记录于 ui-impl-plan.md） |
| BUG-40 | **测试自身的事故**：两个测试共享临时目录，`remove_dir_all` 与另一个的写竞态 → 只在调度恰好交错时出现的 PermissionDenied | 每测试唯一目录（state.rs 测试注释） |

---

## 八、元结论（这份档案的共性规律）

1. **每个魔法数字都是一次事故的墓碑**：`β=2`、`0.05 下限`、`×0.9 偏低`、`clamp[2,900]`、`300s`、`+500ms 宽限`——全部能在注释里找到为什么是这个值。
2. **禁止方向的明确化**：低估冷却 = 一次便宜轮转，高估 = 停摆 key（§30.3）；NUL 报权限错误 = 把读者带离真相——作者反复写 "forbidden direction"。
3. **生产样本入库**：sensenova 429 原文、701 字节中文 tail、TCP 拆分的 usage 事件——回归测试用的是事故现场本身，不是抽象近似。
4. **单一失败原因也要修消息**：BUG-25 表明作者对「错误消息本身成为故障的一部分」有明确意识。

---

## 九、本次审计中新识别（非注释记载，来自代码审阅）

> 以下为整理本文档时的代码审阅发现。除标注外**未经作者确认**，列出供排期参考。

1. **`theme_asset` Windows 反斜杠路径穿越 —— 已修（6d25045）**：防护只按 `/` 切段，Windows 上 `\` 也是 `Path::join` 分隔符 → `%5C` 编码可穿越读任意文件。修复：提取 `valid_theme_path` 拒绝反斜杠，单测 `theme_path_rejects_backslash_traversal`；缓解面：仅绑 127.0.0.1。
2. **`translate_openai_chunk` 缺跨 chunk carry —— 已修（6d25045）**：BUG-03 的翻译管道版——行尾部分 JSON `from_str` 失败被静默丢弃、按 chunk `from_utf8_lossy` 可产生替换符。修复：与 MetricsStream 同款完整行组装（carry + 64KB 上限），回归测试 `openai_chunk_split_mid_line_is_not_dropped` / `openai_chunk_split_inside_multibyte_char_is_reassembled`。
3. **admin 控制面零鉴权（部分已修）**：CSRF 向量已关闭（a8cbea3）——admin 面全路由加 Origin 环回校验（127.0.0.1/::1/localhost 之外 403，`null`（沙箱 iframe）与 `127.0.0.1.evil.com` 后缀伪造拒绝，无 Origin 的 curl/脚本/服务端代理不受影响）；vite dev 代理与 Android WebView 均环回可用。剩余：凭据鉴权本身（admin 无 auth key、`/api/config/export` 吐全部 key 明文）仍待作者定夺信任模型（影响双端 UX）。
4. **三振机制文档漂移 —— 注释已修正（6d25045，见 BUG-07）**：`PoolRuntime::invalid` 注释已改为如实描述「单次 401/403 即隔离、无 strike 计数」；是否引入真正的三振机制属行为变更，留待作者决策。
5. **rlog! 持池锁同步 I/O —— 已修（e72e766）**：`mark_cooldown/mark_invalid/mark_rate_limited` 持池锁时调 rlog，终端写（conhost 可达毫秒级）+ 文件写 + 每行 flush 全在临界区内——池锁是所有请求 select_strategy 的必经锁，429 风暴时排队放大尾延迟（BUG-16 同类）。修复：logger 改有界 channel(1024) + 专职写线程，enqueue=try_send 亚微秒；满队列丢行+补报；时间戳改记入队时刻。附 3 个 writer 单测。
6. **非流式缓冲路径无响应大小上限（未修）**：`DefaultBodyLimit` 已禁用 + 非流式整包缓冲 + 协议翻译再序列化（内存 2x），失控/恶意上游可打高网关内存。
7. **日志轮转 —— 已修（d7d9cb2）；级别/开关仍缺**：16MB × 3 份（`.log.1/.2/.3`，最旧淘汰）按大小轮转，磁盘上限 64MB，轮转在写线程内进行；无级别/开关、无法关闭每请求摘要行（未修）。
8. **观测覆盖缺口（部分已修）**：非流式成功请求与 hedge 胜者已可见（0c401ce——非流式分支补 [req] 行、两处 Walk::Winner 补 [hedge] winner 行；fire 仍不记，池有界且败者各有 [key] 行）；admin 控制面操作审计记录仍缺（即使第 3 项加鉴权后仍需独立记录）。

---

*本档案正文按整理时的代码现状（commit d6553d9）固定；第九节随修复推进更新（最近：0c401ce）。后续新事故应追加至对应分级小节，并保持「症状 / 根因 / 修复 / 证据」四件套格式。*
