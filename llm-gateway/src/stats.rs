use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;

/// Success/fail counters for one entity (total, hour bucket, key, provider or model).
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct Bucket {
    pub success: u64,
    pub fail: u64,
    /// Tokens (input+output) attributed to this bucket. Populated from the
    /// upstream's reported `usage` (or a rough body estimate when absent); 0 for
    /// streaming requests that report no usage chunk (non-stream is always exact).
    #[serde(default)]
    pub tokens: u64,
}

impl Bucket {
    fn add(&mut self, success: bool) {
        if success {
            self.success += 1;
        } else {
            self.fail += 1;
        }
    }
}

/// Auto-detected per-provider live metrics (the aggregate of a provider's keys'
/// measured rpm/tpm/success_rate/avg_tftt/tps). Persisted under `stats.provider_metrics`
/// in gateway.json so the detected values survive a restart: the UI shows the
/// persisted value until the provider serves new traffic, then live takes over.
/// See state.rs `detected_metrics_for`.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct ProviderMetrics {
    pub rpm: u32,
    pub tpm: u32,
    pub success_rate: f64, // [0,1]; 1.0 when no history
    pub avg_tftt_ms: u32,
    pub tps: f32,
}

/// Request statistics. Persisted into gateway.json (under the `stats` field, merged
/// with the config) on a debounce timer so the counters and the 24h histogram
/// survive gateway restarts.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct Stats {
    pub total: Bucket,
    /// hour-epoch-ms -> bucket, trailing 24h only (pruned on save/serve)
    pub hourly: BTreeMap<u64, Bucket>,
    /// key id -> bucket
    pub keys: BTreeMap<String, Bucket>,
    /// provider id -> bucket
    pub providers: BTreeMap<String, Bucket>,
    /// client-facing model string -> bucket
    pub models: BTreeMap<String, Bucket>,
    /// Times a key was rotated out mid-request (429/401/403/408/5xx/network error)
    /// while the client request ultimately succeeded on another key. These are real
    /// upstream failures the client never saw — surfaced separately so the Status page
    /// reflects provider health even when the retry layer absorbs them.
    #[serde(default)]
    pub rotations: BTreeMap<String, u64>,
    /// Auto-detected per-provider live metrics (provider id -> metrics), sampled on
    /// each flush from the pool's per-key metrics. Survives restart so the UI shows
    /// the last-known detected RPM/TPM/etc. until new traffic overrides. Only
    /// providers with live traffic are refreshed on a flush; others keep their
    /// last-persisted entry. See state.rs `detected_metrics_for`.
    #[serde(default)]
    pub provider_metrics: BTreeMap<String, ProviderMetrics>,
}

const HOUR_MS: u64 = 3_600_000;

impl Stats {
    pub fn record(&mut self, success: bool, key: Option<&str>, provider: &str, model: &str, now_ms: u64) {
        self.total.add(success);
        self.hourly.entry(now_ms / HOUR_MS).or_default().add(success);
        if let Some(k) = key {
            self.keys.entry(k.to_string()).or_default().add(success);
        }
        self.providers.entry(provider.to_string()).or_default().add(success);
        if !model.is_empty() {
            self.models.entry(model.to_string()).or_default().add(success);
        }
    }

    /// Count a per-key outcome without touching the request-level totals. Used when a
    /// key is rotated out mid-request (429/auth/timeout): the client request is counted
    /// once by the key that ultimately serves (or fails) it, but every key that was
    /// tried deserves its own failure tally.
    pub fn record_key_only(&mut self, success: bool, key: &str) {
        if success {
            self.keys.entry(key.to_string()).or_default().success += 1;
        } else {
            self.keys.entry(key.to_string()).or_default().fail += 1;
        }
    }

    /// Record that a key was rotated out for `provider` (key-scoped failure the retry
    /// layer absorbed). Bumped per provider; shown on the Status page as 换Key次数.
    pub fn record_rotation(&mut self, provider: &str) {
        *self.rotations.entry(provider.to_string()).or_insert(0) += 1;
    }

    /// Attribute `tokens` (input+output) to the total, the current hour bucket, the
    /// key (if any), the provider, and the model (if non-empty). Skips 0 so a
    /// zero-token request doesn't spawn empty buckets.
    pub fn add_tokens(&mut self, tokens: u64, key: Option<&str>, provider: &str, model: &str, now_ms: u64) {
        if tokens == 0 {
            return;
        }
        self.total.tokens += tokens;
        self.hourly.entry(now_ms / HOUR_MS).or_default().tokens += tokens;
        if let Some(k) = key {
            self.keys.entry(k.to_string()).or_default().tokens += tokens;
        }
        self.providers.entry(provider.to_string()).or_default().tokens += tokens;
        if !model.is_empty() {
            self.models.entry(model.to_string()).or_default().tokens += tokens;
        }
    }

    /// Keep only the trailing 24 hourly buckets (current hour + previous 23).
    pub fn prune(&mut self, now_ms: u64) {
        let cutoff = (now_ms / HOUR_MS).saturating_sub(23);
        self.hourly.retain(|h, _| *h >= cutoff);
    }

    pub fn load(path: &Path) -> Stats {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|raw| serde_json::from_str(&raw).ok())
            .unwrap_or_default()
    }

    /// Atomic-ish save: temp file then rename over the target (same pattern as config).
    /// Unused after the gateway.json+stats merge (state.rs `save_all` writes the merged
    /// file), but kept for the legacy migration path + tests.
    #[allow(dead_code)]
    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        let tmp = path.with_extension("json.tmp");
        let pretty = serde_json::to_string_pretty(self)
            .map_err(|e| std::io::Error::other(e.to_string()))?;
        std::fs::write(&tmp, pretty)?;
        std::fs::rename(&tmp, path)?;
        Ok(())
    }
}
