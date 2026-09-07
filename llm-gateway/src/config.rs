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
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManagedModel {
    pub id: String,
    #[serde(default = "bool_true")]
    pub enabled: bool,
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
}

impl Provider {
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
        }
    }
}

impl Config {
    pub fn load(path: &Path) -> ConfigResult<Self> {
        if !path.exists() {
            let cfg = Config::default();
            cfg.save(path)?;
            return Ok(cfg);
        }
        let raw = std::fs::read_to_string(path)?;
        let cfg: Config = serde_json::from_str(&raw)
            .map_err(|e| format!("failed to parse {}: {e}", path.display()))?;
        Ok(cfg)
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
