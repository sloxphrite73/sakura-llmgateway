use crate::config::Config;
use crate::stats::Stats;
use std::collections::{HashMap, VecDeque};
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// Debounce window for learned-cooldown config writes (429 storms must not churn
/// gateway.json). The dirty flag is checked by a background flusher task.
pub(crate) const CONFIG_FLUSH_DEBOUNCE_MS: u64 = 10_000;

/// Rolling window for the `success_rate` strategy sort attribute (spec §2.2).
/// 5 min trades responsiveness for stability (a key isn't condemned by one bad minute).
pub(crate) const METRIC_SUCCESS_WINDOW_SECS: u64 = 300;
/// Rolling window for `rpm` (spec §2.2) — per-minute, matching "RPM" semantics.
pub(crate) const METRIC_RPM_WINDOW_SECS: u64 = 60;
/// Rolling window for `tpm` (spec §2.2) — tokens consumed in the last minute.
pub(crate) const METRIC_TPM_WINDOW_SECS: u64 = 60;
/// Rolling sample cap for `avg_tftt` / `tps` (last-N mean, spec §2.2).
pub(crate) const METRIC_SAMPLE_CAP: usize = 50;

/// Per-key rolling metrics consumed by the strategy layer (spec §2.2 attributes).
/// `tpm` / `avg_tftt` / `tps` / `*_balance` land in a later chunk (token counting +
/// stream timing + balance-API fetchers); zero/None until then.
#[allow(dead_code)] // forward-looking interface; consumed once the strategy layer lands
#[derive(Clone, Debug, Default)]
pub struct KeyMetricsSnapshot {
    pub success_rate: f64,            // [0,1]; 1.0 when no history (don't penalize new keys)
    pub rpm: u32,
    pub tpm: u32,                     // 0 until token counting
    pub avg_tftt_ms: u32,             // 0 until stream-first-token timing
    pub tps: f32,                     // 0.0 until token counting
    pub token_balance: Option<u64>,  // None = +∞ (OptInf, spec §2.2)
    pub bill_balance: Option<f64>,    // None = +∞
}

#[derive(Debug, Default)]
struct KeyMetrics {
    /// (time, outcome) — pruned to the success window on each push.
    success: VecDeque<(Instant, bool)>,
    /// request timestamps — pruned to the RPM window; `len()` at read = rpm.
    req_times: VecDeque<Instant>,
    /// (time, tokens) — pruned to the TPM window; sum of tokens at read = tpm.
    token_events: VecDeque<(Instant, u64)>,
    /// time-to-first-token samples (ms), last `METRIC_SAMPLE_CAP`; mean = avg_tftt.
    tftt_samples: VecDeque<u32>,
    /// tokens/sec samples, last `METRIC_SAMPLE_CAP`; mean = tps.
    tps_samples: VecDeque<f32>,
    /// USD-normalized bill balance (set by the balance fetcher; None = +∞).
    bill_balance: Option<f64>,
}

#[derive(Debug, Default)]
pub(crate) struct PoolRuntime {
    /// key id -> cooldown expiry
    cooling: HashMap<String, Instant>,
    /// key id -> "invalid" (auth-failed) cooldown expiry. Auth failures (401/403) mean
    /// the key itself is dead — retrying in seconds never succeeds, so these get a long
    /// quarantine that round-robin skips, instead of the 5s rotate-out cooldown.
    invalid: HashMap<String, Instant>,
    /// key id -> last error string (for the UI)
    last_error: HashMap<String, String>,
    /// key id -> total requests routed through this key
    requests: HashMap<String, u64>,
    /// key ids currently being probe-measured by a background task (dedup guard)
    probing: std::collections::HashSet<String>,
    /// Per-key rolling metrics (success_rate / rpm now; tpm/tps/tftt/balance later).
    metrics: HashMap<String, KeyMetrics>,
    /// Per-model time-to-first-token samples (ms), last `METRIC_SAMPLE_CAP`; the
    /// model page's avg-TFTT card reads the mean. Keyed by the upstream model id
    /// (= the managed model id the UI shows). Streaming only (non-stream has no
    /// "first token"); 0 until the first streamed chunk of a request for that model.
    model_tftt: HashMap<String, VecDeque<u32>>,
    /// Strategy-layer D1 cursor map (impl-spec §1, §4.1). Keyed by the
    /// candidate-set identifier (D3: `"{group_id}\x1f{provider_or_any}"`); the
    /// value is the round-robin cursor. Replaces the old per-provider
    /// `counters` map (the both-ON special case is now cursor-keyed by
    /// `(group_id, provider_id)`).
    strategy_cursors: HashMap<String, usize>,
}

pub struct App {
    pub config: std::sync::RwLock<Config>,
    pub pool: Mutex<PoolRuntime>,
    pub http: reqwest::Client,
    pub config_path: std::path::PathBuf,
    /// Request statistics, persisted to stats.json periodically.
    pub stats: Mutex<Stats>,
    /// stats.json sits next to the config file.
    pub stats_path: std::path::PathBuf,
    /// Set when learned-cooldown values changed and gateway.json needs a debounced flush.
    config_dirty: Mutex<bool>,
    /// key id -> Instant when its learned_cooldown was last written (in-process only;
    /// used to rate-limit re-learning probes, not persisted).
    learned_at: Mutex<std::collections::HashMap<String, std::time::Instant>>,
    /// Free-provider catalog: session cache for a remote refresh (embedded catalog
    /// is compile-time and needs no state).
    pub catalog: crate::free_catalog::CatalogCache,
}

pub fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Fill candidate rows' runtime fields — cooldown-derived state (`valid` /
/// `non_cooled` / `remaining_secs`, spec §2.3) + the metrics snapshot
/// (`success_rate`/`rpm`/`tpm`/`avg_tftt`/`tps`/`token_balance`/`bill_balance`,
/// spec §2.2) — from the pool. Prunes expired cooldowns first. Called under the
/// caller's held pool lock by `select_strategy` (hot path) and `strategy_dry_run`
/// (preview), so the two never diverge on what "cooling" means.
fn fill_rows(pool: &mut PoolRuntime, rows: &mut [crate::strategy::Row], now: Instant) {
    pool.cooling.retain(|_, until| *until > now);
    pool.invalid.retain(|_, until| *until > now);
    for r in rows {
        r.valid = !pool.invalid.contains_key(&r.api_key_id);
        if let Some(until) = pool.cooling.get(&r.api_key_id) {
            r.non_cooled = false;
            r.remaining_secs = until.saturating_duration_since(now).as_secs();
        } else {
            r.non_cooled = true;
            r.remaining_secs = 0;
        }
        // Metrics snapshot (spec §2.2): success_rate/rpm/tpm/avg_tftt/tps are
        // live; token_balance is always None (no catalog provider exposes a
        // clean remaining-token-quota field); bill_balance is set by the balance
        // fetcher (None = +∞).
        match pool.metrics.get(&r.api_key_id) {
            None => {
                // No live history: keep the skeleton's seed values (spec §2.2
                // "手动 seed" — set in build_candidates) for the measured
                // metrics. Balances are already None (no provider-reported
                // data for an untouched key). Nothing to overwrite.
            }
            Some(m) => {
                let total = m.success.len();
                r.success_rate = if total == 0 {
                    1.0
                } else {
                    let ok = m.success.iter().filter(|e| e.1).count() as f64;
                    ok / total as f64
                };
                r.rpm = m.req_times.len() as u32;
                r.tpm = m.token_events.iter().map(|(_, n)| *n).sum::<u64>() as u32;
                r.avg_tftt_ms = if m.tftt_samples.is_empty() {
                    0
                } else {
                    (m.tftt_samples.iter().map(|&v| v as u64).sum::<u64>()
                        / m.tftt_samples.len() as u64) as u32
                };
                r.tps = if m.tps_samples.is_empty() {
                    0.0
                } else {
                    m.tps_samples.iter().map(|&v| v).sum::<f32>() / m.tps_samples.len() as f32
                };
                r.token_balance = None;
                r.bill_balance = m.bill_balance;
            }
        }
        // spec §2.3 quota gate: valid = not-banned AND (when a balance is
        // measurable) at least one measurable balance > 0. See valid_with_quota.
        r.valid = valid_with_quota(r.valid, r.token_balance, r.bill_balance);
    }
}

/// spec §2.3 `valid` gate (pure, for unit testing). A key is valid iff it is
/// not-banned AND has available quota. A balance is "measurable" (the provider
/// reports it via `has_*_balance_api`) exactly when it is `Some` here; `None` =
/// unmeasurable = +∞ (the keyless case, where not-banned alone suffices). The
/// spec rule "token_balance>0 OR bill_balance>0" means: if any balance is
/// measurable, at least one must be positive — exhaust every measurable balance
/// and the key is not valid (skip it proactively instead of routing then 429'ing).
fn valid_with_quota(
    not_banned: bool,
    token_balance: Option<u64>,
    bill_balance: Option<f64>,
) -> bool {
    if !not_banned {
        return false;
    }
    let measurable = token_balance.is_some() || bill_balance.is_some();
    if !measurable {
        return true; // keyless / unmeasured → not-banned suffices
    }
    matches!(token_balance, Some(t) if t > 0) || matches!(bill_balance, Some(b) if b > 0.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keyless_unmeasured_is_valid_when_not_banned() {
        // sensenova-style: no balance API → both None → not-banned alone.
        assert!(valid_with_quota(true, None, None));
        assert!(!valid_with_quota(false, None, None));
    }

    #[test]
    fn exhausted_bill_balance_is_not_valid() {
        // has_bill_balance_api provider, balance measured at 0 → not valid.
        assert!(!valid_with_quota(true, None, Some(0.0)));
    }

    #[test]
    fn positive_bill_balance_is_valid() {
        assert!(valid_with_quota(true, None, Some(2.5)));
    }

    #[test]
    fn exhausted_token_balance_is_not_valid() {
        assert!(!valid_with_quota(true, Some(0), None));
    }

    #[test]
    fn one_positive_balance_rescues_the_other() {
        // spec "token_balance>0 OR bill_balance>0": either positive is enough.
        assert!(valid_with_quota(true, Some(0), Some(1.0)));
        assert!(valid_with_quota(true, Some(5), Some(0.0)));
    }

    #[test]
    fn both_balances_zero_is_not_valid() {
        assert!(!valid_with_quota(true, Some(0), Some(0.0)));
    }
}

impl App {
    pub fn new(config: Config, config_path: std::path::PathBuf) -> Self {
        let http = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(10))
            .build()
            .expect("failed to build http client");
        let stats_path = config_path.with_file_name("stats.json");
        let stats = Stats::load(&stats_path);
        Self {
            config: std::sync::RwLock::new(config),
            pool: Mutex::new(PoolRuntime::default()),
            http,
            config_path,
            stats: Mutex::new(stats),
            stats_path,
            config_dirty: Mutex::new(false),
            learned_at: Mutex::new(std::collections::HashMap::new()),
            catalog: crate::free_catalog::CatalogCache::new(),
        }
    }

    /// Try to claim the probe slot for a key. Returns false when a probe task is
    /// already measuring this key (dedup: one prober per key at a time).
    pub fn claim_probe(&self, key_id: &str) -> bool {
        let mut pool = self.pool.lock().unwrap();
        pool.probing.insert(key_id.to_string())
    }

    pub fn release_probe(&self, key_id: &str) {
        self.pool.lock().unwrap().probing.remove(key_id);
    }

    /// Record a finished request (success = upstream returned 2xx and the client got it).
    pub fn record_stat(&self, success: bool, key_id: Option<&str>, provider_id: &str, model: &str) {
        let mut stats = self.stats.lock().unwrap();
        stats.record(success, key_id, provider_id, model, now_ms());
        drop(stats);
        if let Some(k) = key_id {
            self.record_key_metric(success, k);
        }
    }

    /// Count a per-key outcome for a key that was rotated out mid-request (429 / auth
    /// failure / timeout): the client-level request is counted by whoever ultimately
    /// serves it, but this key's own success/fail tally must still move.
    pub fn record_key_stat(&self, success: bool, key_id: &str) {
        let mut stats = self.stats.lock().unwrap();
        stats.record_key_only(success, key_id);
        drop(stats);
        self.record_key_metric(success, key_id);
    }

    /// Record that a key was rotated out (real upstream failure the retry layer
    /// absorbed). Shown on the Status page so provider health is visible even when
    /// every client request ultimately succeeds.
    pub fn record_rotation(&self, provider_id: &str) {
        let mut stats = self.stats.lock().unwrap();
        stats.record_rotation(provider_id);
    }

    /// Attribute `tokens` (input+output) to the total, current hour, key, provider
    /// and model stats buckets. Called from the proxy once the upstream's `usage` is
    /// known (non-stream: from the buffered body; stream: from the usage chunk).
    pub fn record_tokens(&self, tokens: u64, key_id: Option<&str>, provider_id: &str, model: &str) {
        let mut stats = self.stats.lock().unwrap();
        stats.add_tokens(tokens, key_id, provider_id, model, now_ms());
    }

    /// Save stats.json (call from the periodic flusher). Prunes the histogram first.
    pub fn flush_stats(&self) {
        let mut stats = self.stats.lock().unwrap();
        stats.prune(now_ms());
        if let Err(e) = stats.save(&self.stats_path) {
            eprintln!("[gateway] WARNING: failed to persist stats: {e}");
        }
    }

    /// Mark gateway.json dirty (learned-cooldown changed); the background flusher
    /// writes it once the debounce window has passed without further changes.
    pub fn mark_config_dirty(&self) {
        *self.config_dirty.lock().unwrap() = true;
    }

    /// If dirty and the debounce window has elapsed since `last_change`, write the
    /// config. Returns true when a write happened. `last_change` is caller-tracked
    /// because the flusher owns the timer.
    pub fn flush_config_if_due(&self, last_change: &mut Option<std::time::Instant>) -> bool {
        let mut dirty = self.config_dirty.lock().unwrap();
        if !*dirty {
            return false;
        }
        let due = match *last_change {
            Some(t) => t.elapsed() >= Duration::from_millis(CONFIG_FLUSH_DEBOUNCE_MS),
            None => true,
        }; // never-marked = long overdue, write now
        if due {
            let cfg = self.config.read().unwrap().clone();
            if let Err(e) = cfg.save(&self.config_path) {
                eprintln!("[gateway] WARNING: failed to persist learned cooldown: {e}");
            } else {
                *dirty = false;
            }
            *last_change = Some(Instant::now());
        }
        due
    }

    /// Global default cooldown (prober's initial guess when nothing was learned yet).
    pub fn default_cooldown_secs(&self) -> u64 {
        self.read_config().default_cooldown_secs
    }

    /// The key's current learned_cooldown, if any.
    pub fn learned_cooldown_of(&self, provider_id: &str, key_id: &str) -> Option<u64> {
        self.read_config()
            .provider_by_id(provider_id)?
            .keys
            .iter()
            .find(|k| k.id == key_id)?
            .learned_cooldown
    }

    /// Seconds since learned_cooldown was last written for this key (None = never).
    /// learned_cooldown_changed_at is updated by set_learned_cooldown.
    pub fn learned_cooldown_age_secs(&self, _provider_id: &str, key_id: &str) -> Option<u64> {
        let at = *self
            .learned_at
            .lock()
            .unwrap()
            .get(key_id)?;
        let elapsed = std::time::Instant::now().saturating_duration_since(at);
        Some(elapsed.as_secs())
    }

    pub fn read_config(&self) -> Config {
        self.config.read().unwrap().clone()
    }

    pub fn write_config<F: FnOnce(&mut Config)>(&self, f: F) -> Config {
        let mut cfg = self.config.write().unwrap();
        f(&mut cfg);
        let snapshot = cfg.clone();
        if let Err(e) = snapshot.save(&self.config_path) {
            eprintln!("[gateway] WARNING: failed to persist config: {e}");
        }
        snapshot
    }

    /// Mark a key as cooling down. `retry_after_secs` wins if present.
    pub fn mark_cooldown(&self, provider_id: &str, key_id: &str, retry_after_secs: Option<u64>, reason: &str) {
        let cfg = self.read_config();
        let default_secs = cfg.default_cooldown_secs;
        let per_key = cfg
            .provider_by_id(provider_id)
            .and_then(|p| p.keys.iter().find(|k| k.id == key_id))
            .and_then(|k| k.cooldown_secs);
        // A learned cooldown (measured by post-429 probing) beats the static defaults:
        // it reflects the key's real rate-limit window.
        let learned = cfg
            .provider_by_id(provider_id)
            .and_then(|p| p.keys.iter().find(|k| k.id == key_id))
            .and_then(|k| k.learned_cooldown);
        let secs = retry_after_secs
            .or(learned)
            .or(per_key)
            .unwrap_or(default_secs)
            .max(1);
        let mut pool = self.pool.lock().unwrap();
        pool.cooling.insert(key_id.to_string(), Instant::now() + Duration::from_secs(secs));
        pool.last_error.insert(key_id.to_string(), reason.to_string());
    }

    /// Quarantine a key whose credentials were rejected (401/403): round-robin skips it
    /// until `secs` elapse. Manual "解除冷却" in the UI clears it immediately.
    pub fn mark_invalid(&self, _provider_id: &str, key_id: &str, secs: u64, reason: &str) {
        let mut pool = self.pool.lock().unwrap();
        pool.invalid.insert(key_id.to_string(), Instant::now() + Duration::from_secs(secs));
        pool.last_error.insert(key_id.to_string(), reason.to_string());
    }

    /// Push a per-key outcome into the rolling success window (feeds the
    /// `success_rate` strategy sort attribute, spec §2.2). Folded into
    /// record_stat / record_key_stat so every key attempt's outcome is captured
    /// with no new call sites in the proxy hot path.
    fn record_key_metric(&self, success: bool, key_id: &str) {
        let now = Instant::now();
        let mut pool = self.pool.lock().unwrap();
        let m = pool.metrics.entry(key_id.to_string()).or_default();
        m.success.push_back((now, success));
        let win = Duration::from_secs(METRIC_SUCCESS_WINDOW_SECS);
        m.success.retain(|e| now.duration_since(e.0) <= win);
    }

    /// Push a token-count sample into the rolling TPM window (spec §2.2 `tpm`
    /// source). Called from the success path with the request's total tokens
    /// (input+output) — actual `usage` when the upstream reports it, else a rough
    /// estimate (decision ①A). Streaming + non-stream both feed this.
    pub(crate) fn record_token_count(&self, key_id: &str, tokens: u64) {
        if tokens == 0 {
            return;
        }
        let now = Instant::now();
        let mut pool = self.pool.lock().unwrap();
        let m = pool.metrics.entry(key_id.to_string()).or_default();
        m.token_events.push_back((now, tokens));
        let win = Duration::from_secs(METRIC_TPM_WINDOW_SECS);
        m.token_events.retain(|(t, _)| now.duration_since(*t) <= win);
    }

    /// Record a time-to-first-token sample (ms) — streaming only (decision ②A).
    #[allow(dead_code)] // wired in the streaming chunk
    pub(crate) fn record_tftt(&self, key_id: &str, ms: u32) {
        let mut pool = self.pool.lock().unwrap();
        let m = pool.metrics.entry(key_id.to_string()).or_default();
        m.tftt_samples.push_back(ms);
        while m.tftt_samples.len() > METRIC_SAMPLE_CAP {
            m.tftt_samples.pop_front();
        }
    }

    /// Record a per-model time-to-first-token sample (ms) — streaming only. Keys
    /// the model-page avg-TFTT card by the upstream model id (= managed model id).
    pub(crate) fn record_model_tftt(&self, model: &str, ms: u32) {
        let mut pool = self.pool.lock().unwrap();
        let s = pool.model_tftt.entry(model.to_string()).or_default();
        s.push_back(ms);
        while s.len() > METRIC_SAMPLE_CAP {
            s.pop_front();
        }
    }

    /// Mean per-model TFTT (ms), or 0 when no streamed sample yet.
    pub fn model_avg_tftt(&self, model: &str) -> u32 {
        let pool = self.pool.lock().unwrap();
        match pool.model_tftt.get(model) {
            None => 0,
            Some(s) if s.is_empty() => 0,
            Some(s) => (s.iter().map(|&v| v as u64).sum::<u64>() / s.len() as u64) as u32,
        }
    }

    /// Record a tokens/sec sample — streaming generation rate (spec §2.2 `tps`).
    #[allow(dead_code)] // wired in the streaming chunk
    pub(crate) fn record_tps(&self, key_id: &str, tps: f32) {
        let mut pool = self.pool.lock().unwrap();
        let m = pool.metrics.entry(key_id.to_string()).or_default();
        m.tps_samples.push_back(tps);
        while m.tps_samples.len() > METRIC_SAMPLE_CAP {
            m.tps_samples.pop_front();
        }
    }

    /// Set the USD-normalized bill balance (spec §2.2 `bill_balance`, decision ④A
    /// currency normalization). None = +∞. Set by the balance fetcher.
    #[allow(dead_code)] // wired in the balance-fetcher chunk
    pub(crate) fn set_bill_balance(&self, key_id: &str, usd: Option<f64>) {
        let mut pool = self.pool.lock().unwrap();
        let m = pool.metrics.entry(key_id.to_string()).or_default();
        m.bill_balance = usd;
    }

    /// Snapshot of a key's rolling metrics for the strategy layer (spec §2.2).
    /// success_rate/rpm/tpm are live (tpm from non-stream now; streaming next);
    /// avg_tftt/tps land with streaming timing; bill_balance with the fetcher.
    #[allow(dead_code)] // consumed by the strategy layer once implemented (step2 impl-spec §3)
    pub fn metrics_of(&self, _provider_id: &str, key_id: &str) -> KeyMetricsSnapshot {
        let pool = self.pool.lock().unwrap();
        match pool.metrics.get(key_id) {
            None => KeyMetricsSnapshot {
                success_rate: 1.0, // no history = don't penalize (valid default)
                ..Default::default()
            },
            Some(m) => {
                let total = m.success.len();
                let success_rate = if total == 0 {
                    1.0
                } else {
                    let ok = m.success.iter().filter(|e| e.1).count() as f64;
                    ok / total as f64
                };
                let tpm = m.token_events.iter().map(|(_, n)| *n).sum::<u64>() as u32;
                let avg_tftt_ms = if m.tftt_samples.is_empty() {
                    0
                } else {
                    (m.tftt_samples.iter().map(|&v| v as u64).sum::<u64>()
                        / m.tftt_samples.len() as u64) as u32
                };
                let tps = if m.tps_samples.is_empty() {
                    0.0
                } else {
                    m.tps_samples.iter().map(|&v| v).sum::<f32>() / m.tps_samples.len() as f32
                };
                KeyMetricsSnapshot {
                    success_rate,
                    rpm: m.req_times.len() as u32,
                    tpm,
                    avg_tftt_ms,
                    tps,
                    token_balance: None, // +∞ — no catalog provider exposes a clean
                                        // remaining-token-quota field (see
                                        // doc/research/provider-balance-apis.md)
                    bill_balance: m.bill_balance,
                }
            }
        }
    }

    /// Per-model price (CNY / 1M tok) from the provider's `price_table`.
    /// Missing entry = 0 (free). Feeds the `price` strategy sort attribute (I, §2.2).
    #[allow(dead_code)] // consumed by the strategy layer once implemented
    pub fn price_of(&self, provider_id: &str, model: &str) -> f64 {
        self.read_config()
            .provider_by_id(provider_id)
            .and_then(|p| p.price_table.get(model).copied())
            .unwrap_or(0.0)
    }

    /// Strategy-layer select (spec §3 ③④⑤ + §6; impl-spec §4): fill the
    /// candidate rows' cooldown + metrics state from the pool, then call
    /// `strategy::select_pure` (D1 rotate → stable sort → walk → terminal)
    /// under one lock so the cursor stays consistent with the selection.
    ///
    /// Built once per request (`build_candidates`), re-walked each attempt as
    /// keys cool down — `pick_key`'s replacement on the request hot path.
    /// North-compat: both-ON + empty sort reproduces `pick_key`'s
    /// round-robin-skip-cooldown (impl-spec §0, verified by `select_pure` tests).
    pub fn select_strategy(
        &self,
        set: &crate::strategy::CandidateSet,
        sorts: &[crate::config::SortKey],
    ) -> crate::strategy::SelectResult {
        // Clone the skeletons so the caller's set stays reusable across attempts;
        // the sort reorders the local copy only.
        let mut rows = set.rows.clone();
        let now = Instant::now();
        let mut pool = self.pool.lock().unwrap();
        fill_rows(&mut pool, &mut rows, now);
        // D1 cursor: read under the same lock, advance inside select_pure, persist.
        let cursor = *pool.strategy_cursors.entry(set.cursor_key.clone()).or_insert(0);
        drop(pool); // release before select_pure (pure; no pool access)
        let (result, new_cursor) = crate::strategy::select_pure(&mut rows, sorts, cursor);
        let mut pool = self.pool.lock().unwrap();
        pool.strategy_cursors.insert(set.cursor_key.clone(), new_cursor);
        // A routed row bumps the rolling RPM window (matches pick_key's
        // req_times push, so the `rpm` sort attribute tracks live load).
        if let crate::strategy::SelectResult::Routed(ref row) = result {
            let now = Instant::now();
            let m = pool.metrics.entry(row.api_key_id.clone()).or_default();
            m.req_times.push_back(now);
            let win = Duration::from_secs(METRIC_RPM_WINDOW_SECS);
            m.req_times.retain(|t| now.duration_since(*t) <= win);
            // Count the routed attempt (pick_key bumped `requests` for the UI).
            *pool.requests.entry(row.api_key_id.clone()).or_insert(0) += 1;
        }
        result
    }

    /// Dry-run preview (impl-spec §8): the same fill + sort + walk as
    /// `select_strategy`, but read-only — it does **not** advance the persisted
    /// cursor or bump the RPM window. Returns the §6 terminal + the full sorted
    /// candidate list (the would-be-routed row marked) so the UI strategy board
    /// (prototype `computeRoute()` + `resultBanner`) can show what the selector
    /// *would* route. Never exposes `key_secret` (the secret never leaves the
    /// process; only `api_key_id` is reported).
    pub fn strategy_dry_run(
        &self,
        set: &crate::strategy::CandidateSet,
        sorts: &[crate::config::SortKey],
    ) -> serde_json::Value {
        let mut rows = set.rows.clone();
        let now = Instant::now();
        let mut pool = self.pool.lock().unwrap();
        fill_rows(&mut pool, &mut rows, now);
        let cursor = *pool.strategy_cursors.entry(set.cursor_key.clone()).or_insert(0);
        drop(pool);
        let (result, routed_idx, _new_cursor) =
            crate::strategy::select_preview(&mut rows, sorts, cursor);
        let (result_type, retry_after) = match &result {
            crate::strategy::SelectResult::Routed(_) => ("routed", None),
            crate::strategy::SelectResult::AllCooling { retry_after } => {
                ("all_cooling", *retry_after)
            }
            crate::strategy::SelectResult::NoValid => ("no_valid", None),
            crate::strategy::SelectResult::Empty => ("empty", None),
        };
        let routed_summary = match &result {
            crate::strategy::SelectResult::Routed(r) => Some(serde_json::json!({
                "provider_id": r.provider_id,
                "upstream_model": r.upstream_model,
                "api_key_id": r.api_key_id,
                "group_id": r.group_id,
            })),
            _ => None,
        };
        let candidates: Vec<serde_json::Value> = rows
            .iter()
            .enumerate()
            .map(|(i, r)| {
                serde_json::json!({
                    "provider_id": r.provider_id,
                    "upstream_model": r.upstream_model,
                    "api_key_id": r.api_key_id,
                    "group_id": r.group_id,
                    "valid": r.valid,
                    "non_cooled": r.non_cooled,
                    "remaining_secs": r.remaining_secs,
                    "success_rate": r.success_rate,
                    "rpm": r.rpm,
                    "tpm": r.tpm,
                    "avg_tftt_ms": r.avg_tftt_ms,
                    "tps": r.tps,
                    "token_balance": r.token_balance,
                    "bill_balance": r.bill_balance,
                    "price": r.price,
                    "idx": r.idx,
                    "routed": routed_idx == Some(i),
                })
            })
            .collect();
        serde_json::json!({
            "result": result_type,
            "retry_after": retry_after,
            "routed": routed_summary,
            "candidates": candidates,
            "cursor": cursor,
        })
    }

    /// Soonest a *valid* (non-dead) key in the given candidate set recovers
    /// from rate-limit cooldown (spec §6 "全冷却" Retry-After), or `None` when
    /// no key is merely cooling (all dead/invalid → no Retry-After). The
    /// generalization of `min_retry_after_secs` to a cross-provider candidate
    /// set (impl-spec §6, decision D6): signature takes the candidate rows
    /// rather than a single `&Provider`.
    #[allow(dead_code)] // wired in forward() for the §6 terminal
    pub fn min_retry_after_candidates(&self, set: &crate::strategy::CandidateSet) -> Option<u64> {
        let now = Instant::now();
        let mut pool = self.pool.lock().unwrap();
        pool.cooling.retain(|_, until| *until > now);
        pool.invalid.retain(|_, until| *until > now);
        let mut min: Option<u64> = None;
        for r in &set.rows {
            // Dead (401/403-quarantined) keys are not valid — skip (spec §6).
            if pool.invalid.contains_key(&r.api_key_id) {
                continue;
            }
            if let Some(until) = pool.cooling.get(&r.api_key_id) {
                let secs = until.saturating_duration_since(now).as_secs();
                min = Some(min.map_or(secs, |m| m.min(secs)));
            }
        }
        min
    }

    pub fn clear_error(&self, key_id: &str) {
        self.pool.lock().unwrap().last_error.remove(key_id);
    }

    pub fn clear_cooldown(&self, key_id: &str) {
        let mut pool = self.pool.lock().unwrap();
        pool.cooling.remove(key_id);
        pool.invalid.remove(key_id);
        pool.probing.remove(key_id);
    }

    /// Record a measured cooldown (seconds) learned by post-429 probing into the key's
    /// config, and mark gateway.json dirty for the debounced flush.
    pub fn set_learned_cooldown(&self, provider_id: &str, key_id: &str, secs: u64) {
        // Write under the write lock directly; no immediate file save (debounced).
        let mut cfg = self.config.write().unwrap();
        if let Some(p) = cfg.provider_by_id_mut(provider_id) {
            if let Some(k) = p.keys.iter_mut().find(|k| k.id == key_id) {
                k.learned_cooldown = Some(secs);
            }
        }
        drop(cfg);
        self.learned_at
            .lock()
            .unwrap()
            .insert(key_id.to_string(), std::time::Instant::now());
        self.mark_config_dirty();
    }
    /// Pool status snapshot for the UI: per key -> { cooling_until_ms, requests, last_error }.
    pub fn status(&self) -> serde_json::Value {
        let mut pool = self.pool.lock().unwrap();
        // Expire cooldowns here too (not just in pick_key): the UI polls status, not the
        // proxy path, so without this a finished cooldown would show "0s" forever.
        pool.cooling.retain(|_, until| *until > Instant::now());
        pool.invalid.retain(|_, until| *until > Instant::now());
        let now_ms = now_ms();
        let fmt_remaining = |until: &Instant| -> u64 {
            until.saturating_duration_since(Instant::now()).as_secs()
        };
        let cooling: serde_json::Map<String, serde_json::Value> = pool
            .cooling
            .iter()
            .map(|(k, until)| {
                let remaining = fmt_remaining(until);
                (
                    k.clone(),
                    serde_json::json!({
                        "until_ms": now_ms + remaining * 1000,
                        "remaining_secs": remaining,
                    }),
                )
            })
            .collect();
        let invalid: serde_json::Map<String, serde_json::Value> = pool
            .invalid
            .iter()
            .map(|(k, until)| {
                let remaining = fmt_remaining(until);
                (
                    k.clone(),
                    serde_json::json!({
                        "until_ms": now_ms + remaining * 1000,
                        "remaining_secs": remaining,
                    }),
                )
            })
            .collect();
        serde_json::json!({
            "cooling": cooling,
            "invalid": invalid,
            "requests": pool.requests,
            "last_error": pool.last_error,
        })
    }
}
