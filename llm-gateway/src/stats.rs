use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;

/// Success/fail counters for one entity (total, hour bucket, key, provider or model).
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct Bucket {
    pub success: u64,
    pub fail: u64,
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

/// Request statistics. Persisted to `stats.json` (next to gateway.json) on a debounce
/// timer so the counters and the 24h histogram survive gateway restarts.
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
    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        let tmp = path.with_extension("json.tmp");
        let pretty = serde_json::to_string_pretty(self)
            .map_err(|e| std::io::Error::other(e.to_string()))?;
        std::fs::write(&tmp, pretty)?;
        std::fs::rename(&tmp, path)?;
        Ok(())
    }
}
