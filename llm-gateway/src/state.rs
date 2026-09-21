use crate::config::{Config, Provider};
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
    /// round-robin cursor per provider id
    counters: HashMap<String, usize>,
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

    /// Pick the next healthy key for a provider (round-robin, skipping keys in cooldown
    /// and quarantined-invalid keys).
    pub fn pick_key(&self, provider: &Provider) -> Option<crate::config::ApiKey> {
        let now = Instant::now();
        let mut pool = self.pool.lock().unwrap();
        let n = provider.keys.len();
        if n == 0 {
            return None;
        }
        let start = *pool.counters.entry(provider.id.clone()).or_insert(0) % n;
        // prune expired cooldowns so status stays accurate
        pool.cooling.retain(|_, until| *until > now);
        pool.invalid.retain(|_, until| *until > now);
        for i in 0..n {
            let idx = (start + i) % n;
            let key = &provider.keys[idx];
            if pool.cooling.contains_key(&key.id) || pool.invalid.contains_key(&key.id) {
                continue;
            }
            pool.counters.insert(provider.id.clone(), idx + 1);
            *pool.requests.entry(key.id.clone()).or_insert(0) += 1;
            // Rolling RPM window (strategy §2.2 `rpm` attribute source).
            {
                let now = Instant::now();
                let m = pool.metrics.entry(key.id.clone()).or_default();
                m.req_times.push_back(now);
                let win = Duration::from_secs(METRIC_RPM_WINDOW_SECS);
                m.req_times.retain(|t| now.duration_since(*t) <= win);
            }
            return Some(key.clone());
        }
        None
    }

    /// For the client-facing 429 (spec §6): the soonest any *valid* (non-dead)
    /// key of this provider recovers from rate-limit cooldown, or `None` when no
    /// key is merely cooling (all dead/invalid → no Retry-After, per spec §6
    /// "无 valid 行 → 429 无 Retry-After"). Called after `pick_key` returned
    /// `None`; prunes defensively so a just-expired cooldown is not reported.
    pub fn min_retry_after_secs(&self, provider: &Provider) -> Option<u64> {
        let now = Instant::now();
        let mut pool = self.pool.lock().unwrap();
        pool.cooling.retain(|_, until| *until > now);
        pool.invalid.retain(|_, until| *until > now);
        let mut min: Option<u64> = None;
        for k in &provider.keys {
            // dead (401/403-quarantined) keys are not valid — skip, per spec §6.
            if pool.invalid.contains_key(&k.id) {
                continue;
            }
            if let Some(until) = pool.cooling.get(&k.id) {
                let secs = until.saturating_duration_since(now).as_secs();
                min = Some(min.map_or(secs, |m| m.min(secs)));
            }
        }
        min
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
