/* Server response shapes (admin REST API) — mirrors llm-gateway/src/admin.rs. */

export interface KeyMetrics {
  success_rate?: number;
  rpm?: number;
  tpm?: number;
  avg_tftt_ms?: number;
  token_balance?: number | null;
  bill_balance?: number | null;
  capacity_rpm?: number | null;
  capacity_tpm?: number | null;
  utilization?: number | null;
  tps?: number;
}

export interface ServerKey {
  id: string;
  /** masked key (prefix+••••+suffix), never the real secret */
  key: string;
  display_name?: string;
  invalid?: boolean;
  cooling?: { until_ms: number } | null;
  cooldown_secs?: number | null;
  last_error?: string;
  requests?: number;
  metrics?: KeyMetrics;
  seed_rpm?: number | null;
  seed_tpm?: number | null;
  seed_success_rate?: number | null;
  seed_avg_tftt_ms?: number | null;
  seed_tps?: number | null;
}

export interface ServerModel {
  id: string;
  enabled: boolean;
  context_length?: number | null;
  supports_images?: boolean;
}

export interface ServerProvider {
  id: string;
  name: string;
  base_url: string;
  protocol?: string;
  rpm_limit?: number | null;
  tpm_limit?: number | null;
  detected_metrics?: { rpm: number; tpm: number } | null;
  models?: ServerModel[];
  keys?: ServerKey[];
  aliases?: Record<string, string>;
  price_table?: Record<string, number>;
  output_price_table?: Record<string, number>;
}

export interface Status {
  providers: ServerProvider[];
  default_cooldown_secs?: number;
  max_attempts?: number;
  max_hold_secs?: number;
  api_port: number;
  ui_port: number;
  auth?: { enabled: boolean; keys: string[] };
  now_ms?: number;
  version?: string;
}

export interface StatsHour { hour_ms: number; success: number; fail: number; tokens: number; }
export interface StatEntry { name?: string; success: number; fail: number; tokens: number; avg_tftt_ms?: number; avg_tps?: number; }
export interface Stats {
  total: { success: number; fail: number; tokens: number };
  rotations_total?: number;
  histogram?: StatsHour[];
  providers?: Record<string, StatEntry>;
  models?: Record<string, StatEntry>;
  keys?: Record<string, StatEntry>;
}

export interface Strategy {
  filter: { lock_model_group: boolean; lock_provider: boolean };
  sort: string[];
}

export type GroupEntry = [string, string]; // [provider_id, model_id]
export interface ModelGroup { id: string; entries: GroupEntry[]; }

export interface CatalogEntry {
  id?: string;
  name: string;
  base_url: string;
  protocol: string;
  keyless?: boolean;
  free_models?: string[];
  notes_zh?: string;
  notes_en?: string;
  note?: string;
}

export interface DryRunCandidate {
  routed: boolean;
  api_key_id: string;
  provider_id: string;
  upstream_model: string;
  valid: boolean;
  non_cooled: boolean;
  remaining_secs?: number;
  price?: number;
  tps?: number;
  success_rate?: number;
  availability?: number;
}
export interface DryRunResult {
  result: string; // 'routed' | 'all_cooling' | 'no_valid' | ...
  retry_after?: number | null;
  candidates?: DryRunCandidate[];
}

/* ============ adapted UI shapes (the old prototype's mock shapes) ============ */

export interface UiProvider {
  id: string; name: string; base: string; proto: string;
  tb: number; bb: number; rpm: number; tpm: number;
  resetAt: string; resetQuota: number;
  rpmLimit: number | null; tpmLimit: number | null;
  detectedMetrics: { rpm: number; tpm: number } | null;
  dot: string;
}

export interface UiModel {
  id: string; prov: string; up: string;
  pIn: number; pOut: number; ctx: number;
  on: boolean; img: boolean; alias: string[];
  tftt: number; tps: number;
}

export interface UiKey {
  kid: string; prov: string; model: string;
  dname: string; valid: boolean; cool: boolean; cd: number;
  untilMs: number; cdDur: number; masked: string;
  sr: number; rpm: number; tpm: number; tftt: number;
  tb: number; bb: number;
  capRpm: number | null; capTpm: number | null; util: number | null;
  price: number; tps: number; lastErr: string; req: number;
  seedRpm?: number | null; seedTpm?: number | null; seedSr?: number | null;
  seedTftt?: number | null; seedTps?: number | null;
}

export interface UiGroup { name: string; members: { prov: string; model: string }[]; }

/** computeRoute() shape kept from the prototype. */
export interface RouteCandidate {
  routed: boolean; kid: string; prov: string; model: string;
  valid: boolean; cool: boolean; cd: number;
  price: number; tps: number; sr: number; avail: number;
}
export interface RouteResult {
  state: 'routed' | '429-retry' | '429-novalid' | '429-empty';
  winner: RouteCandidate | null;
  cand: RouteCandidate[];
  retry?: number | null;
}
