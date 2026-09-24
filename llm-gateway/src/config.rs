use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

type ConfigResult<T> = std::result::Result<T, Box<dyn std::error::Error>>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiKey {
    #[serde(default = "crate::new_id")]
    pub id: String,
    pub key: String,
    #[serde(default)]
    pub label: String,
    /// Per-key cooldown override in seconds. `None` = use global default.
    #[serde(default)]
    pub cooldown_secs: Option<u64>,
    /// Cooldown in seconds learned by post-429 probing. Machine-written, distinct from
    /// the user-set `cooldown_secs`; beats it in cooldown resolution order.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub learned_cooldown: Option<u64>,
    // --- metric seeds (spec §2.2 "手动 seed + 自动覆盖"): pre-set initial values
    // used only while a key has no live measurement history (fill_rows `None`
    // branch leaves the skeleton; the `Some(m)` branch overwrites with live
    // data once any request touches the key). All `None` = use the built-in
    // defaults (success_rate 1.0, others 0), so old configs load unchanged.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seed_rpm: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seed_tpm: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seed_success_rate: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seed_avg_tftt_ms: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seed_tps: Option<f32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManagedModel {
    pub id: String,
    #[serde(default = "bool_true")]
    pub enabled: bool,
    /// Max context window in tokens (informational; shown on the model card as
    /// e.g. "128k"). Edited from the UI in K units (UI value × 1000 → tokens).
    /// `None`/missing = unknown (shown as 0). Backward-compatible: old configs
    /// without the field load as `None`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_length: Option<u32>,
}

fn bool_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Provider {
    #[serde(default = "crate::new_id")]
    pub id: String,
    pub name: String,
    /// OpenAI-compatible base URL, e.g. `https://api.openai.com/v1`
    pub base_url: String,
    #[serde(default)]
    pub keys: Vec<ApiKey>,
    /// Managed model catalog. Empty = unmanaged (all models pass through).
    #[serde(default)]
    pub models: Vec<ManagedModel>,
    /// When true and `models` is non-empty, only enabled models in the list may be requested.
    #[serde(default)]
    pub model_allowlist_only: bool,
    /// alias -> upstream model name, e.g. {"gpt4o": "gpt-4o-2024-08-06"}
    #[serde(default)]
    pub aliases: std::collections::BTreeMap<String, String>,
    /// Wire protocol this upstream speaks: `"openai"` (default) or `"anthropic"`.
    /// Auth header, request path and body format all follow from this.
    #[serde(default)]
    pub protocol: String,
    /// Whether this upstream exposes a token-balance/quota API the gateway can poll
    /// (fills the `token_balance` strategy sort attribute, spec §2.2). `false` (default)
    /// = treat as unlimited (+∞ per spec); the gateway skips polling. Of the free
    /// catalog only Mistral could (indirectly, via admin rate-limit) — low value, so
    /// effectively always +∞. See doc/research/provider-balance-apis.md.
    #[serde(default)]
    pub has_token_balance_api: bool,
    /// Whether this upstream exposes a bill/credit balance API (fills the
    /// `bill_balance` sort attribute, spec §2.2). `false` (default) = +∞, skip polling.
    /// Tier A (same key as inference): Moonshot / SiliconFlow / Novita / Requesty /
    /// OpenRouter. Tier B (cloud AK/SK, skip unless creds exist): Fireworks /
    /// Volcengine / Alibaba.
    #[serde(default)]
    pub has_bill_balance_api: bool,
    /// Per-model price in CNY per 1M tokens (logical model id -> price). Missing entry
    /// = 0 (free). Feeds the `price` strategy sort attribute (I, spec §2.2).
    #[serde(default)]
    pub price_table: std::collections::BTreeMap<String, f64>,
    /// Manual per-provider RPM cap (provider detail "配额与速率" panel). `None` =
    /// auto — the UI shows the live aggregate of this provider's key metrics
    /// (read-only) and saves nothing. Informational only (not enforced as
    /// throttling; the strategy layer's `rpm` sort attribute uses per-key live
    /// metrics, spec §2.2).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rpm_limit: Option<u32>,
    /// Manual per-provider TPM cap (spec §2.2). `None` = auto.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tpm_limit: Option<u32>,
}

impl Provider {
    /// Wire protocol of this upstream (`openai` when unset — back-compat).
    pub fn protocol(&self) -> crate::protocol::Protocol {
        crate::protocol::Protocol::parse(&self.protocol)
    }

    /// Allowlist check: pass when allowlist is off, or the list is empty (unmanaged),
    /// or the model is present and enabled.
    pub fn model_allowed(&self, model: &str) -> bool {
        !self.model_allowlist_only
            || self.models.is_empty()
            || self.models.iter().any(|m| m.id == model && m.enabled)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthSettings {
    #[serde(default)]
    pub enabled: bool,
    /// Bearer tokens accepted on the local /v1 API when enabled.
    #[serde(default)]
    pub keys: Vec<String>,
}

impl Default for AuthSettings {
    fn default() -> Self {
        Self { enabled: false, keys: Vec::new() }
    }
}

// ---------------------------------------------------------------------------
// Routing strategy layer (spec §2.1, §2.3, §4.2; impl-spec §2.1, §4.2)
// ---------------------------------------------------------------------------

/// One user-selectable sort key (spec §5.2: A–J). Serializes as the single
/// uppercase letter `"A"`..`"J"` so `gateway.json` reads `"sort": ["I","J"]`.
/// The comparison direction + attribute live in `strategy.rs` (`impl SortKey`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SortKey {
    A, // success_rate      desc
    B, // rpm               desc
    C, // tpm               desc
    D, // avg_tftt          asc
    E, // token_balance     desc
    F, // token_balance     asc
    G, // bill_balance      desc
    H, // bill_balance      asc
    I, // price             asc
    J, // tps               desc
}

impl SortKey {
    /// Parse a single letter (case-insensitive) into a `SortKey`. Used by the
    /// `PUT /api/strategy` write path (impl-spec §8); serde handles the
    /// deserialize side for config files.
    #[allow(dead_code)]
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_uppercase().as_str() {
            "A" => Some(Self::A),
            "B" => Some(Self::B),
            "C" => Some(Self::C),
            "D" => Some(Self::D),
            "E" => Some(Self::E),
            "F" => Some(Self::F),
            "G" => Some(Self::G),
            "H" => Some(Self::H),
            "I" => Some(Self::I),
            "J" => Some(Self::J),
            _ => None,
        }
    }
}

/// Two filter toggles (spec §4) that shrink the full `(provider, model, key)`
/// table to a candidate subset. Defaults are both `true` = the north-compat
/// "exact (provider, model), only round keys" = current gateway behavior.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Filter {
    /// ON = restrict to the requested model's group (same logical model across
    /// providers). ON + `lock_provider` = exact (provider, model).
    #[serde(default = "bool_true")]
    pub lock_model_group: bool,
    /// ON = restrict to the provider resolved from the request. When the request
    /// carries no provider, this auto-degrades to OFF (spec §3 ①).
    #[serde(default = "bool_true")]
    pub lock_provider: bool,
}

impl Default for Filter {
    fn default() -> Self {
        Self { lock_model_group: true, lock_provider: true }
    }
}

/// User strategy: 2 filter toggles + an ordered, de-duplicated sort stack of
/// 0..=3 keys (spec §5.2). `Strategy::default()` = both ON + empty sort, which
/// under D1 cursor rotation reproduces the current round-robin-skip-cooldown
/// behavior (impl-spec §0 north-compat rule, §9).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Strategy {
    #[serde(default)]
    pub filter: Filter,
    /// Ordered user sort keys (3rd/4th/5th in the comparator). 0..=3, no dups.
    /// Validated/deduped/capped on load (impl-spec §4.2).
    #[serde(default)]
    pub sort: Vec<SortKey>,
}

impl Default for Strategy {
    fn default() -> Self {
        Self { filter: Filter::default(), sort: Vec::new() }
    }
}

/// A logical model group (spec §1, §2.3): the same logical model offered by one
/// or more providers, each possibly under a different upstream model id. An
/// empty `model_groups` = no cross-provider routing; the request model becomes
/// a single-member group and behavior is unchanged (impl-spec §9).
///
/// `entries` are `(provider_id, upstream_model)` pairs (impl-spec §2.3, §7) so
/// cross-provider same-model-different-upstream-name cases (e.g. OpenAI
/// `gpt-4o` vs a mirror's `gpt-4o-2024-08`) carry the real upstream name.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelGroup {
    pub id: String,
    #[serde(default)]
    pub entries: Vec<(String, String)>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub api_port: u16,
    pub ui_port: u16,
    /// Default cooldown for a 429'd key (when neither Retry-After nor per-key override exists).
    pub default_cooldown_secs: u64,
    /// Max attempts per client request (retry on 429/5xx/timeout before first byte).
    pub max_attempts: u32,
    pub auth: AuthSettings,
    pub providers: Vec<Provider>,
    /// Currency -> USD rate (e.g. `{"CNY": 0.14}`), for normalizing `bill_balance`
    /// to USD so the G/H sorts are comparable cross-currency (decision ④A). USD
    /// values need no entry; unknown currency -> +inf (not comparable). See
    /// doc/research/provider-balance-apis.md.
    #[serde(default)]
    pub currency_rates: std::collections::BTreeMap<String, f64>,
    /// Routing strategy (spec §5). Default = both filter toggles ON + empty
    /// sort = current round-robin-skip-cooldown behavior (impl-spec §0).
    #[serde(default)]
    pub strategy: Strategy,
    /// Logical model groups enabling cross-provider fallback (spec §1, §2.3).
    /// Empty = every model is a single-member group; behavior unchanged.
    #[serde(default)]
    pub model_groups: Vec<ModelGroup>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            api_port: 8000,
            ui_port: 8001,
            default_cooldown_secs: 60,
            max_attempts: 3,
            auth: AuthSettings::default(),
            providers: Vec::new(),
            currency_rates: std::collections::BTreeMap::new(),
            strategy: Strategy::default(),
            model_groups: Vec::new(),
        }
    }
}

impl Config {
    /// Parse a config from a raw JSON string without touching disk. Used by the import
    /// endpoint's validate-then-swap: the whole file is rejected on any error.
    pub fn parse_str(raw: &str) -> ConfigResult<Self> {
        let mut cfg: Config = serde_json::from_str(raw)
            .map_err(|e| format!("invalid config: {e}"))?;
        cfg.normalize_strategy();
        Ok(cfg)
    }

    pub fn load(path: &Path) -> ConfigResult<Self> {
        if !path.exists() {
            let cfg = Config::default();
            cfg.save(path)?;
            return Ok(cfg);
        }
        let raw = std::fs::read_to_string(path)?;
        let mut cfg: Config = serde_json::from_str(&raw)
            .map_err(|e| format!("failed to parse {}: {e}", path.display()))?;
        cfg.normalize_strategy();
        Ok(cfg)
    }

    /// Enforce the strategy sort invariants (impl-spec §4.2): ordered, no
    /// duplicates, at most 3 keys. Logs a warning when a loaded config needed
    /// fixing. Idempotent and safe to call on an already-valid config.
    pub fn normalize_strategy(&mut self) {
        let orig = self.strategy.sort.clone();
        let mut seen = std::collections::HashSet::new();
        let mut deduped: Vec<SortKey> = Vec::with_capacity(orig.len());
        for k in &orig {
            if seen.insert(*k) {
                deduped.push(*k);
            }
        }
        if deduped.len() > 3 {
            deduped.truncate(3);
        }
        if deduped != orig {
            eprintln!(
                "[gateway] strategy.sort had duplicates or exceeded 3 keys; \
                 normalized to {:?}",
                deduped
            );
        }
        self.strategy.sort = deduped;
    }

    /// Atomic-ish save: write temp file then rename over the target.
    pub fn save(&self, path: &Path) -> ConfigResult<()> {
        let tmp: PathBuf = path.with_extension("json.tmp");
        let pretty = serde_json::to_string_pretty(self)?;
        std::fs::write(&tmp, pretty)?;
        std::fs::rename(&tmp, path)?;
        Ok(())
    }

    pub fn provider_by_id(&self, id: &str) -> Option<&Provider> {
        self.providers.iter().find(|p| p.id == id)
    }

    pub fn provider_by_id_mut(&mut self, id: &str) -> Option<&mut Provider> {
        self.providers.iter_mut().find(|p| p.id == id)
    }

    /// Resolve a client-supplied `model` string to (provider, upstream model).
    /// Supports `provider/model` and aliases (unique across providers).
    /// Returns `Err(reason)` when a route exists but the model is not allowed.
    pub fn resolve_model(&self, model: &str) -> Result<(String, String), String> {
        // exact alias match first (aliases win over composite names)
        for p in &self.providers {
            if let Some(m) = p.aliases.get(model) {
                if !p.model_allowed(m) {
                    return Err(format!(
                        "alias `{model}` points to model `{m}` which does not exist or is disabled on provider `{}`",
                        p.name
                    ));
                }
                return Ok((p.id.clone(), m.clone()));
            }
        }
        // composite `provider/model` — match on id or name
        if let Some((head, tail)) = model.split_once('/') {
            if let Some(p) = self
                .providers
                .iter()
                .find(|p| p.id == head || p.name == head)
            {
                if !p.model_allowed(tail) {
                    return Err(format!(
                        "model `{tail}` is not in the managed model list of provider `{}` (allowlist mode is on)",
                        p.name
                    ));
                }
                return Ok((p.id.clone(), tail.to_string()));
            }
        }
        Err(format!(
            "unknown model `{model}`. Use `provider/model` or a configured alias."
        ))
    }
}
