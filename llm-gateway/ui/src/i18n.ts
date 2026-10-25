/* i18n (zh identity / en lookup).
   t() returns its arg unchanged in zh; in en it looks up EN.
   Parametrized strings use {param} placeholders, replaced by the caller. */

export type Lang = 'zh' | 'en';

let current: Lang = 'zh';
export function setLang(l: Lang): void {
  current = l;
  try { localStorage.setItem('lang', l); } catch (_) { /* ignore */ }
}
export function getLang(): Lang { return current; }
export function initLang(): Lang {
  try { current = localStorage.getItem('lang') === 'en' ? 'en' : 'zh'; } catch (_) { current = 'zh'; }
  return current;
}

const EN: Record<string, string> = {
  '控制台':'Console', '运行中':'Running',
  '导航':'Navigate', 'Key 池':'Key pool',
  '统计':'Stats', '提供商':'Providers', '模型':'Models', '策略':'Strategy', 'API Key':'API Keys', '设置':'Settings',
  '可用':'Ready', '冷却':'Cooling', '冷却中':'Cooling', '异常':'Invalid',
  '概览':'Overview', '网关全局快照':'Gateway global snapshot',
  '可用 Key':'Available keys', '异常 Key':'Invalid keys', '模型条目':'Model entries', '模型组':'Model groups',
  '提供商概览':'Provider overview', '点行进详情':'Click row for detail',
  '协议':'Protocol', 'token 余额':'Token balance', '账单余额':'Bill balance', 'RPM':'RPM', 'TPM':'TPM', 'Key':'Key',
  '不限':'Unlimited',
  '+ 添加':'+ Add', '基本信息 + 限额（已归并到提供商下）':'Basic info + quota (merged under provider)',
  '基本信息':'Basic info', '名称':'Name', 'Base URL':'Base URL',
  '托管模型 {mc} 个 · Key {kc} 把（Key 在 API Key 板统一管理）':'{mc} managed models · {kc} keys (managed in API Keys board)',
  '配额与速率':'Quota & rate', 'RPM/TPM 可手填或采用自动检测':'RPM/TPM manual or auto-detected',
  '配额重置时间':'Quota reset time', '重置配额（token）':'Reset quota (tokens)', '重置配额':'Reset quota', '采用自动检测结果':'Use auto-detection',
  '保存':'Save', '删除提供商':'Delete provider',
  '冷却 · 剩 {cd}s':'Cooling · {cd}s left', '智能冷却时长':'Smart cooldown',
  '模型信息：所有模型条目大列表，点一条进编辑页 · 模型组：跨提供商同模型编组':'Model info: all models in one list, click to edit · Groups: cross-provider same-model grouping',
  '模型信息':'Model info', '模型组管理':'Model groups',
  '从上游获取模型（演示）':'Fetch models from upstream (demo)',
  '上下文':'Context', '输入':'Input', '输出':'Output', '启用':'Enabled', '禁用':'Disabled', '无':'None',
  '← 返回':'← Back',
  '模型 ID':'Model ID', '上游模型名':'Upstream model', '上下文长度':'Context length', '输入价格':'Input price', '输出价格':'Output price', '别名':'Aliases',
  'tokens':'tokens', '¥/1M tok':'¥/1M tok', '是':'Yes', '否':'No', '添加别名':'Add alias', '添加':'Add', '取消':'Cancel',
  '同一模型跨提供商编组 · 每个模型恒属且仅属一组':'Same model grouped across providers · each model belongs to exactly one group',
  '组名':'Group', '成员 (provider · model)':'Members (provider · model)', '成员数':'Count', '编辑':'Edit',
  '+ 新建模型组':'+ New group', '多成员组在"模型优先"模式下跨提供商兜底，长上下文靠排序而非换模型':'Multi-member groups cross-provider fallback in "model-priority" mode; long context via sort, not model switch',
  '暂无可用模型':'No models available', '选择模型':'Select model', '已编组':'Grouped',
  '过滤器':'Filters', '不改变模型组':'Lock model group', '不改变提供商':'Lock provider', '当前：':'Now: ',
  '优先策略':'Priority', '关键字 3 / 4 / 5（1=valid↓ 2=非冷却↓ 固定前导）':'Keys 3/4/5 (1=valid↓ 2=non-cooled↓ fixed lead)',
  '关键字 {n}':'Key {n}', '（跳过）':'(skip)', '演示':'Demo', 'dry-run 实时':'dry-run live', '模拟请求':'Simulate request',
  '路由命中':'Routed', '429 全冷却':'429 all cooling', '429 无可用':'429 no valid', '429 候选为空':'429 empty',
  '返回':'returns', '候选 {n} 条均无效':'{n} candidates, all invalid', '当前过滤下无候选':'No candidates under current filter',
  '#':'#', 'valid':'valid', 'cool':'cool', '单价':'Price', 'TPS':'TPS', '成功率':'Success rate', '候选为空':'No candidates',
  '精确 (provider,model) 轮换 Key - 当前行为，零惊喜':'Exact (provider,model) key rotation - current behavior, zero surprise',
  '模型优先 - 跨提供商同模型':'Model priority - cross-provider same model', '提供商优先 - 换模型':'Provider priority - switch model', '可用优先 - 所有候选':'Availability priority - all candidates',
  '精确轮换':'Exact', '模型优先':'Model priority', '提供商优先':'Provider priority', '可用优先':'Availability',
  '全局状态总览 · 跨所有提供商一张表':'Global status · one table across all providers', '已回收 {n} 把':'{n} recycled',
  '全部':'All', '粘贴 API Key，多个每行一个可批量添加':'Paste API key; multi-line for batch add', '批量添加':'Batch add',
  '移动所有无效Key至回收站':'Move all invalid keys to recycle',
  '状态':'Status', '请求数':'Requests', '最近错误':'Last error', '操作':'Actions', '账单':'Bill',
  '显示':'Show', '复制':'Copy', '删除':'Delete', '从剪贴板粘贴':'Paste from clipboard', '已复制':'Copied',
  '请求与 Token 用量 · 数据持久化，重启不丢失':'Request & token usage · persisted across restarts',
  '总览':'Overview', '过去 24 小时请求':'Past 24h requests', '过去 24 小时 Token 用量':'Past 24h token usage',
  '按提供商':'By provider', '按模型':'By model', '按 API Key':'By API key',
  '成功请求':'Successful requests', '失败请求':'Failed requests', '换 Key 次数':'Key rotations', 'Token 总消耗':'Total tokens',
  '每小时一柱 · 绿=成功 红=失败':'One bar per hour · green=success red=fail',
  '成功':'Success', '失败':'Fail', 'Token 消耗':'Tokens',
  '{h}:00 · 成功 {ok} / 失败 {fail}':'{h}:00 · {ok} ok / {fail} fail', '{h}:00 · {n} tok':'{h}:00 · {n} tok',
  '全局设置 · 端口修改后重启生效':'Global settings · port changes need restart', '基本':'Basic',
  '默认冷却时间（秒）':'Default cooldown (s)', '每请求最大尝试次数':'Max attempts per request', 'API 端口':'API port', 'UI 端口':'UI port',
  '启用本地 API 鉴权':'Enable local API auth', '鉴权 Key，逗号分隔':'Auth keys, comma-separated', '保存设置':'Save settings',
  '全冷却最长等待（秒）':'Max hold when all cooling (s)',
  '配置文件':'Config file', '导出配置':'Export config', '导入配置':'Import config', '导入旧版配置':'Import legacy config', 'gateway.json · 导入即时热生效':'gateway.json · import hot-reloads', '旧版导入：仅采纳提供商/Key/模型/智能冷却，保留当前全局设置':'Legacy import: adopts only providers/keys/models/learned-cooldown; keeps current global settings', '导入旧版配置 {n}？仅采纳提供商/Key/模型/智能冷却，保留当前全局设置。':'Import legacy config {n}? Adopts only providers/keys/models/learned-cooldown; keeps current global settings.',
  '添加提供商':'Add provider', '从免费目录选一家，或自定义填入':'Pick from the free catalog, or fill in custom',
  '自定义提供商':'Custom provider', '手动填写 base_url、协议与模型':'Manually fill base_url, protocol, models',
  '已有的提供商':'Existing providers', '从免费目录选一家，预填 base_url / 协议 / 模型':'Pick from the free catalog; prefill base_url / protocol / models',
  '免费目录 · 点一张预填配置':'Free catalog · click one to prefill', '添加：':'Add: ',
  '已从目录预填，可改 · 粘 Key 后添加':'Prefilled from catalog, editable · paste key then add', '手动填写 · 粘 Key 后添加':'Fill in manually · paste key then add',
  '托管模型':'Managed models', '添加模型 id':'Add model id', '删除模型':'Delete model', '粘贴 API Key':'Paste API key',
  '切换语言 / switch language':'切换语言 / switch language',
  '从上游获取模型':'Fetch models from upstream', '启用/禁用':'Enable/Disable', '上游':'Upstream',
  '策略管理':'Strategy management', '请求模型':'Request model', '请求提供商':'Request provider',
  '隐藏':'Hide', '每小时一柱':'One bar per hour', '请求':'Requests', '从免费目录选择':'Pick from free catalog',
  '成功率 ↓':'Success rate ↓', 'RPM ↓':'RPM ↓', 'TPM ↓':'TPM ↓', '首字延迟 ↑':'TFTT ↑',
  'token余额 ↓':'Token balance ↓', 'token余额 ↑':'Token balance ↑',
  '账单余额 ↓':'Bill balance ↓', '账单余额 ↑':'Bill balance ↑', '单价 ↑':'Price ↑', '生成速度TPS ↓':'Gen speed TPS ↓',
  '可用性 ↓':'Availability ↓', '利用率':'Utilization', '实测容量':'Measured capacity',
  /* feedback strings for the wired (real-API) mutations — no new UI elements */
  '删除该提供商及其所有 Key？':'Delete this provider and all its keys?',
  '删除该 Key？':'Delete this key?',
  '删除模型 {m}？指向它的别名将无法使用。':'Delete model {m}? Aliases pointing to it will stop working.',
  '导入 {n} 将整体替换当前配置（含端口与鉴权设置），确定继续？':'Importing {n} will fully replace the current config (ports & auth included). Continue?',
  '暂无数据':'No data yet', '暂无提供商，点击「+ 添加」创建':'No providers yet — click "+ Add" to create one',
  /* toast feedback strings for wired (real-API) mutations */
  '已保存':'Saved', '已删除':'Deleted', '添加成功':'Added', '添加失败: ':'Add failed: ',
  '导入成功':'Imported', '导入失败: ':'Import failed: ', '已触发下载':'Download started',
  '安卓后台保活':'Android background keep-alive',
  '为防止系统（尤其小米/澎湃）清理后台：① 免除电池优化；② 允许自启动；③ 在最近任务页把本应用加锁':'To stop the system (esp. Xiaomi/HyperOS) from killing the app in background: 1) exempt battery optimization; 2) allow autostart; 3) lock the app in recents',
  '免除电池优化':'Exempt battery optimization', '打开自启动设置':'Open autostart settings',
  '自启动权限只能由你手动开启（系统不允许应用代开）；另外建议在最近任务页长按本应用卡片选择「锁定」。开机后网关会自动启动。':'Autostart can only be enabled by you (apps cannot grant it themselves); also long-press this app\'s card in recents and choose "Lock". The gateway starts automatically after a reboot.',
  '未找到该 Key':'Key not found', '失败: ':'Failed: ',
  /* api-key metric seed editor (spec §2.2 手动 seed) */
  'Seed':'Seed', '预置':'Seed', '指标预置':'Metric seed',
  '名字':'Name', '编辑Key信息':'Edit key info', '留空 = 显示掩码 Key':'Empty = show masked key',
  '粘贴 API Key，多个每行一个；可在 Key 前加名字，空格分隔':'Paste API keys, one per line; optionally prefix each with a display name separated by a space',
  '无实测史时的预置初值，有实测后自动覆盖；留空 = 清除':'Preset initial values used before any live measurement; auto-overridden once measured; empty = clear',
  /* upstream-fetch auto-import + model-group editor + readonly hints */
  '请先选择提供商':'Pick a provider first', '拉取中…':'Fetching…', '拉取失败: ':'Fetch failed: ',
  '上游未返回任何模型':'Upstream returned no models', '新添加 {n} 个模型':'Added {n} models',
  '无后端':'No backend',
  '添加成员':'Add member', 'provider':'provider', 'upstream model':'upstream model',
  '删除该模型组？':'Delete this model group?', 'K':'K',
};

export function t(k: string): string {
  return current === 'en' ? (EN[k] != null ? EN[k] : k) : k;
}

/** t() + {param} substitution, e.g. tp('已回收 {n} 把', { n: 3 }) */
export function tp(k: string, params: Record<string, string | number>): string {
  let s = t(k);
  for (const [key, v] of Object.entries(params)) s = s.replace('{' + key + '}', String(v));
  return s;
}
