use crate::config::{Config, Provider};
use crate::stats::Stats;
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// Debounce window for learned-cooldown config writes (429 storms must not churn
/// gateway.json). The dirty flag is checked by a background flusher task.
pub(crate) const CONFIG_FLUSH_DEBOUNCE_MS: u64 = 10_000;

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
    }

    /// Count a per-key outcome for a key that was rotated out mid-request (429 / auth
    /// failure / timeout): the client-level request is counted by whoever ultimately
    /// serves it, but this key's own success/fail tally must still move.
    pub fn record_key_stat(&self, success: bool, key_id: &str) {
        let mut stats = self.stats.lock().unwrap();
        stats.record_key_only(success, key_id);
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
            return Some(key.clone());
        }
        None
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
