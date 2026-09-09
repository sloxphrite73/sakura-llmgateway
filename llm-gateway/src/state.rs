use crate::config::{Config, Provider};
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

#[derive(Debug, Default)]
pub(crate) struct PoolRuntime {
    /// round-robin cursor per provider id
    counters: HashMap<String, usize>,
    /// key id -> cooldown expiry
    cooling: HashMap<String, Instant>,
    /// key id -> last error string (for the UI)
    last_error: HashMap<String, String>,
    /// key id -> total requests routed through this key
    requests: HashMap<String, u64>,
}

pub struct App {
    pub config: std::sync::RwLock<Config>,
    pub pool: Mutex<PoolRuntime>,
    pub http: reqwest::Client,
    pub config_path: std::path::PathBuf,
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
        Self {
            config: std::sync::RwLock::new(config),
            pool: Mutex::new(PoolRuntime::default()),
            http,
            config_path,
        }
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
        let secs = retry_after_secs.or(per_key).unwrap_or(default_secs).max(1);
        let mut pool = self.pool.lock().unwrap();
        pool.cooling.insert(key_id.to_string(), Instant::now() + Duration::from_secs(secs));
        pool.last_error.insert(key_id.to_string(), reason.to_string());
    }

    /// Pick the next healthy key for a provider (round-robin, skipping keys in cooldown).
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
        for i in 0..n {
            let idx = (start + i) % n;
            let key = &provider.keys[idx];
            if pool.cooling.contains_key(&key.id) {
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
        self.pool.lock().unwrap().cooling.remove(key_id);
    }
    /// Pool status snapshot for the UI: per key -> { cooling_until_ms, requests, last_error }.
    pub fn status(&self) -> serde_json::Value {
        let mut pool = self.pool.lock().unwrap();
        // Expire cooldowns here too (not just in pick_key): the UI polls status, not the
        // proxy path, so without this a finished cooldown would show "0s" forever.
        pool.cooling.retain(|_, until| *until > Instant::now());
        let cooling: serde_json::Map<String, serde_json::Value> = pool
            .cooling
            .iter()
            .map(|(k, until)| {
                let remaining = until.saturating_duration_since(Instant::now()).as_secs();
                (
                    k.clone(),
                    serde_json::json!({
                        "until_ms": now_ms() + remaining * 1000,
                        "remaining_secs": remaining,
                    }),
                )
            })
            .collect();
        serde_json::json!({
            "cooling": cooling,
            "requests": pool.requests,
            "last_error": pool.last_error,
        })
    }
}
