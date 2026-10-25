/* adapters: real API responses → the prototype's mock shapes (adaptAll port). */

import type {
  Status, Stats, ModelGroup,
  UiProvider, UiModel, UiKey, UiGroup,
} from './types';

export interface Adapted {
  providers: UiProvider[];
  models: UiModel[];
  keys: UiKey[];
  groups: UiGroup[];
}

export function adaptAll(
  status: Status | null,
  stats: Stats | null,
  modelGroups: ModelGroup[] | null,
): Adapted {
  const provs = (status && status.providers) || [];
  const providers: UiProvider[] = provs.map(p => ({
    id: p.id, name: p.name, base: p.base_url, proto: p.protocol || 'openai',
    // no per-provider token/bill/quota backend: unlimited / 0 / empty (not faked)
    tb: Infinity, bb: Infinity, rpm: 0, tpm: 0, resetAt: '', resetQuota: 0,
    rpmLimit: (p.rpm_limit != null) ? p.rpm_limit : null,
    tpmLimit: (p.tpm_limit != null) ? p.tpm_limit : null,
    // server-side detected metrics (live aggregate, or persisted after a restart
    // until new traffic overrides — see state.rs detected_metrics_for).
    detectedMetrics: p.detected_metrics || null,
    dot: 'var(--color-action-primary)',
  }));

  const models: UiModel[] = [];
  const keys: UiKey[] = [];
  provs.forEach(p => {
    (p.models || []).forEach(m => {
      const aliases = Object.keys(p.aliases || {}).filter(a => (p.aliases || {})[a] === m.id);
      const price = (p.price_table && p.price_table[m.id] != null) ? p.price_table[m.id] : 0;
      const outPrice = (p.output_price_table && p.output_price_table[m.id] != null) ? p.output_price_table[m.id] : 0;
      const tftt = (stats && stats.models && stats.models[m.id]) ? (stats.models[m.id].avg_tftt_ms || 0) : 0;
      const tps = (stats && stats.models && stats.models[m.id]) ? (stats.models[m.id].avg_tps || 0) : 0;
      models.push({
        id: m.id, prov: p.id, up: m.id, pIn: price, pOut: outPrice,
        ctx: (m.context_length || 0), on: m.enabled, img: !!m.supports_images,
        alias: aliases, tftt, tps,
      });
    });
    (p.keys || []).forEach(k => {
      const mt = k.metrics || {};
      const now = (status && status.now_ms) || 0;
      const cd = k.cooling ? Math.max(0, Math.ceil((k.cooling.until_ms - now) / 1000)) : 0;
      // Persisted per-key request count from stats (survives restarts); fall
      // back to the pool's in-memory counter when stats have no entry for this key.
      const statsKey = (stats && stats.keys && stats.keys[k.id]) ? stats.keys[k.id] : null;
      const reqCount = statsKey ? ((statsKey.success || 0) + (statsKey.fail || 0)) : (k.requests || 0);
      keys.push({
        kid: k.id, prov: p.id, model: '-',
        // 显示名：空 = 回落到掩码形式（dnameOrMask）
        dname: k.display_name || '',
        valid: !k.invalid, cool: !!k.cooling, cd,
        untilMs: k.cooling ? k.cooling.until_ms : 0,
        // 生效冷却时长: 每 Key 覆盖 → 全局默认（learned_cooldown 已随探测器
        // 移除而弃用，PASS §38；冷却主要来自 429 时的窗口推算，不落盘）。
        cdDur: k.cooldown_secs != null ? k.cooldown_secs
             : ((status && status.default_cooldown_secs != null) ? status.default_cooldown_secs : 0),
        masked: k.key || '••••',
        sr: mt.success_rate || 0, rpm: mt.rpm || 0, tpm: mt.tpm || 0, tftt: mt.avg_tftt_ms || 0,
        tb: mt.token_balance == null ? Infinity : mt.token_balance,
        bb: mt.bill_balance == null ? Infinity : mt.bill_balance,
        // 实测容量（429 采样 min 估计）与当前利用率（PASS §36）
        capRpm: mt.capacity_rpm != null ? mt.capacity_rpm : null,
        capTpm: mt.capacity_tpm != null ? mt.capacity_tpm : null,
        util: mt.utilization != null ? mt.utilization : null,
        price: 0, tps: mt.tps || 0, lastErr: k.last_error || '',
        req: reqCount,
        seedRpm: k.seed_rpm, seedTpm: k.seed_tpm, seedSr: k.seed_success_rate,
        seedTftt: k.seed_avg_tftt_ms, seedTps: k.seed_tps,
      });
    });
  });

  // RPM/TPM display value: a manual cap if set, else the server-detected value
  // (live aggregate, or the persisted value after a restart until new traffic
  // overrides). Read-only in the detail panel when no manual cap is set.
  providers.forEach(pr => {
    const dm = pr.detectedMetrics || { rpm: 0, tpm: 0 };
    pr.rpm = (pr.rpmLimit != null) ? pr.rpmLimit : dm.rpm;
    pr.tpm = (pr.tpmLimit != null) ? pr.tpmLimit : dm.tpm;
  });

  const groups: UiGroup[] = (modelGroups || []).map(g => ({
    name: g.id,
    members: (g.entries || []).map(e => ({ prov: e[0], model: e[1] })),
  }));

  return { providers, models, keys, groups };
}
