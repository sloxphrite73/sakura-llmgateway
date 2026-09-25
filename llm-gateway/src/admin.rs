use crate::config::{ApiKey, AuthSettings, ManagedModel, Provider};
use crate::state::{now_ms, App};
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;

pub(crate) fn ok<T: serde::Serialize>(v: T) -> Response {
    (StatusCode::OK, Json(v)).into_response()
}

pub(crate) fn err(status: StatusCode, msg: &str) -> Response {
    (status, Json(serde_json::json!({ "error": msg }))).into_response()
}

// ---------- providers ----------

pub async fn list_providers(State(app): State<std::sync::Arc<App>>) -> Response {
    ok(app.read_config())
}

#[derive(serde::Deserialize)]
pub struct ProviderInput {
    pub name: String,
    pub base_url: String,
    pub model_allowlist_only: Option<bool>,
    /// `"openai"` (default) or `"anthropic"` — the wire protocol this upstream speaks.
    pub protocol: Option<String>,
    /// Whether this upstream exposes a pollable token-balance API (spec §2.2).
    pub has_token_balance_api: Option<bool>,
    /// Whether this upstream exposes a pollable bill/credit-balance API (spec §2.2).
    pub has_bill_balance_api: Option<bool>,
}

pub async fn create_provider(
    State(app): State<std::sync::Arc<App>>,
    Json(input): Json<ProviderInput>,
) -> Response {
    if input.name.trim().is_empty() || input.base_url.trim().is_empty() {
        return err(StatusCode::BAD_REQUEST, "name and base_url are required");
    }
    let cfg = app.write_config(|c| {
        c.providers.push(Provider {
            id: crate::new_id(),
            name: input.name.trim().to_string(),
            base_url: input.base_url.trim().trim_end_matches('/').to_string(),
            keys: Vec::new(),
            models: Vec::new(),
            model_allowlist_only: input.model_allowlist_only.unwrap_or(false),
            aliases: Default::default(),
            protocol: input.protocol.unwrap_or_default(),
            has_token_balance_api: input.has_token_balance_api.unwrap_or(false),
            has_bill_balance_api: input.has_bill_balance_api.unwrap_or(false),
            price_table: Default::default(),
            rpm_limit: None,
            tpm_limit: None,
        });
    });
    ok(cfg)
}

pub async fn update_provider(
    State(app): State<std::sync::Arc<App>>,
    Path(id): Path<String>,
    Json(input): Json<ProviderInput>,
) -> Response {
    let mut found = false;
    let cfg = app.write_config(|c| {
        if let Some(p) = c.providers.iter_mut().find(|p| p.id == id) {
            p.name = input.name.trim().to_string();
            p.base_url = input.base_url.trim().trim_end_matches('/').to_string();
            if let Some(allowlist) = input.model_allowlist_only {
                p.model_allowlist_only = allowlist;
            }
            if let Some(protocol) = &input.protocol {
                p.protocol = protocol.clone();
            }
            if let Some(v) = input.has_token_balance_api {
                p.has_token_balance_api = v;
            }
            if let Some(v) = input.has_bill_balance_api {
                p.has_bill_balance_api = v;
            }
            found = true;
        }
    });
    if found {
        ok(cfg)
    } else {
        err(StatusCode::NOT_FOUND, "provider not found")
    }
}

#[derive(serde::Deserialize)]
pub struct RateLimitInput {
    /// Manual per-provider RPM cap (provider detail "配额与速率" panel).
    /// `None` = clear to auto; `Some(n)` = set the cap.
    pub rpm_limit: Option<u32>,
    pub tpm_limit: Option<u32>,
}

/// PUT /api/providers/{id}/rate-limits — set a provider's manual RPM/TPM caps
/// (informational; not enforced as throttling — the strategy layer's `rpm`/`tpm`
/// sort attributes use per-key live metrics, spec §2.2). `null` clears to auto.
/// A dedicated endpoint (rather than folding into `update_provider`) so `null`
/// is distinguishable from "field omitted" (serde collapses `null` → outer
/// `None` on `Option<Option<_>>`, so a nullable-in-PUT needs its own route).
pub async fn set_rate_limits(
    State(app): State<std::sync::Arc<App>>,
    Path(id): Path<String>,
    Json(input): Json<RateLimitInput>,
) -> Response {
    let cfg = app.write_config(|c| {
        if let Some(p) = c.providers.iter_mut().find(|p| p.id == id) {
            p.rpm_limit = input.rpm_limit;
            p.tpm_limit = input.tpm_limit;
        }
    });
    ok(cfg)
}

/// PUT /api/providers/{id}/price-table — replace a provider's per-model price
/// table (spec §2.2 `price`, sort I). Body `{table: {model: ¥/1M tok}}`;
/// missing models = 0 (free). Lets the UI edit model prices (the prototype's
/// model-edit price field) without a full config import.
pub async fn update_price_table(
    State(app): State<std::sync::Arc<App>>,
    Path(id): Path<String>,
    Json(input): Json<PriceTableInput>,
) -> Response {
    let mut found = false;
    let cfg = app.write_config(|c| {
        if let Some(p) = c.providers.iter_mut().find(|p| p.id == id) {
            p.price_table = input.table;
            found = true;
        }
    });
    if found {
        ok(cfg)
    } else {
        err(StatusCode::NOT_FOUND, "provider not found")
    }
}

#[derive(serde::Deserialize)]
pub struct PriceTableInput {
    /// logical model id -> CNY per 1M tokens. Missing entry = 0 (free).
    pub table: std::collections::BTreeMap<String, f64>,
}

pub async fn delete_provider(
    State(app): State<std::sync::Arc<App>>,
    Path(id): Path<String>,
) -> Response {
    let mut found = false;
    let cfg = app.write_config(|c| {
        let before = c.providers.len();
        c.providers.retain(|p| p.id != id);
        found = c.providers.len() < before;
    });
    if found {
        ok(cfg)
    } else {
        err(StatusCode::NOT_FOUND, "provider not found")
    }
}

// ---------- keys ----------

#[derive(serde::Deserialize)]
pub struct KeyInput {
    pub key: String,
    #[serde(default)]
    pub label: String,
    pub cooldown_secs: Option<u64>,
    // Spec §2.2 "手动 seed": optional initial values for a brand-new key with no
    // live history. All optional; omitted = use built-in defaults.
    #[serde(default)]
    pub seed_rpm: Option<u32>,
    #[serde(default)]
    pub seed_tpm: Option<u32>,
    #[serde(default)]
    pub seed_success_rate: Option<f64>,
    #[serde(default)]
    pub seed_avg_tftt_ms: Option<u32>,
    #[serde(default)]
    pub seed_tps: Option<f32>,
}

pub async fn add_key(
    State(app): State<std::sync::Arc<App>>,
    Path(id): Path<String>,
    Json(input): Json<KeyInput>,
) -> Response {
    if input.key.trim().is_empty() {
        return err(StatusCode::BAD_REQUEST, "key is required");
    }
    let mut found = false;
    let cfg = app.write_config(|c| {
        if let Some(p) = c.providers.iter_mut().find(|p| p.id == id) {
            p.keys.push(ApiKey {
                id: crate::new_id(),
                key: input.key.trim().to_string(),
                label: input.label.trim().to_string(),
                cooldown_secs: input.cooldown_secs,
                learned_cooldown: None,
                seed_rpm: input.seed_rpm,
                seed_tpm: input.seed_tpm,
                seed_success_rate: input.seed_success_rate,
                seed_avg_tftt_ms: input.seed_avg_tftt_ms,
                seed_tps: input.seed_tps,
            });
            found = true;
        }
    });
    if found {
        ok(cfg)
    } else {
        err(StatusCode::NOT_FOUND, "provider not found")
    }
}

pub async fn delete_key(
    State(app): State<std::sync::Arc<App>>,
    Path((id, key_id)): Path<(String, String)>,
) -> Response {
    let mut found = false;
    let cfg = app.write_config(|c| {
        if let Some(p) = c.providers.iter_mut().find(|p| p.id == id) {
            let before = p.keys.len();
            p.keys.retain(|k| k.id != key_id);
            found = p.keys.len() < before;
        }
    });
    if found {
        ok(cfg)
    } else {
        err(StatusCode::NOT_FOUND, "key not found")
    }
}

/// Update a key's metric seeds (spec §2.2 "手动 seed"). Dedicated endpoint for
/// nullable fields: a plain `Option<T>` where JSON `null`/omitted → `None` (clear
/// the seed) and a number → `Some` (set it). The UI sends all five fields each
/// save (empty input → null → clear). `cooldown_secs` is create-time only and
/// not touched here.
#[derive(serde::Deserialize)]
pub struct KeySeedInput {
    #[serde(default)]
    pub seed_rpm: Option<u32>,
    #[serde(default)]
    pub seed_tpm: Option<u32>,
    #[serde(default)]
    pub seed_success_rate: Option<f64>,
    #[serde(default)]
    pub seed_avg_tftt_ms: Option<u32>,
    #[serde(default)]
    pub seed_tps: Option<f32>,
}

pub async fn update_key_seed(
    State(app): State<std::sync::Arc<App>>,
    Path((id, key_id)): Path<(String, String)>,
    Json(input): Json<KeySeedInput>,
) -> Response {
    let mut found = false;
    let cfg = app.write_config(|c| {
        if let Some(p) = c.providers.iter_mut().find(|p| p.id == id) {
            if let Some(k) = p.keys.iter_mut().find(|k| k.id == key_id) {
                k.seed_rpm = input.seed_rpm;
                k.seed_tpm = input.seed_tpm;
                k.seed_success_rate = input.seed_success_rate;
                k.seed_avg_tftt_ms = input.seed_avg_tftt_ms;
                k.seed_tps = input.seed_tps;
                found = true;
            }
        }
    });
    if found {
        ok(cfg)
    } else {
        err(StatusCode::NOT_FOUND, "key not found")
    }
}

pub async fn clear_key_cooldown(
    State(app): State<std::sync::Arc<App>>,
    Path(key_id): Path<String>,
) -> Response {
    app.clear_cooldown(&key_id);
    ok(serde_json::json!({ "ok": true }))
}

// ---------- aliases ----------

#[derive(serde::Deserialize)]
pub struct AliasInput {
    pub alias: String,
    pub model: String,
}

pub async fn set_alias(
    State(app): State<std::sync::Arc<App>>,
    Path(id): Path<String>,
    Json(input): Json<AliasInput>,
) -> Response {
    let mut found = false;
    let cfg = app.write_config(|c| {
        if let Some(p) = c.providers.iter_mut().find(|p| p.id == id) {
            p.aliases.insert(input.alias.trim().to_string(), input.model.trim().to_string());
            found = true;
        }
    });
    if found {
        ok(cfg)
    } else {
        err(StatusCode::NOT_FOUND, "provider not found")
    }
}

pub async fn delete_alias(
    State(app): State<std::sync::Arc<App>>,
    Path((id, alias)): Path<(String, String)>,
) -> Response {
    let mut found = false;
    let cfg = app.write_config(|c| {
        if let Some(p) = c.providers.iter_mut().find(|p| p.id == id) {
            found = p.aliases.remove(&alias).is_some();
        }
    });
    if found {
        ok(cfg)
    } else {
        err(StatusCode::NOT_FOUND, "alias not found")
    }
}

// ---------- managed models ----------

#[derive(serde::Deserialize)]
pub struct ModelInput {
    pub id: String,
}

pub async fn add_model(
    State(app): State<std::sync::Arc<App>>,
    Path(id): Path<String>,
    Json(input): Json<ModelInput>,
) -> Response {
    let mid = input.id.trim().to_string();
    if mid.is_empty() {
        return err(StatusCode::BAD_REQUEST, "model id is required");
    }
    let mut result: Result<(), String> = Ok(());
    let cfg = app.write_config(|c| {
        if let Some(p) = c.providers.iter_mut().find(|p| p.id == id) {
            if p.models.iter().any(|m| m.id == mid) {
                result = Err(format!("model `{mid}` already exists on provider `{}`", p.name));
                return;
            }
            p.models.push(ManagedModel { id: mid, enabled: true, context_length: None });
        }
    });
    match result {
        Err(msg) => err(StatusCode::CONFLICT, &msg),
        Ok(()) => ok(cfg),
    }
}

pub async fn delete_model(
    State(app): State<std::sync::Arc<App>>,
    Path((id, model_id)): Path<(String, String)>,
) -> Response {
    let mut found = false;
    let cfg = app.write_config(|c| {
        if let Some(p) = c.providers.iter_mut().find(|p| p.id == id) {
            let before = p.models.len();
            p.models.retain(|m| m.id != model_id);
            found = p.models.len() < before;
        }
    });
    if found {
        ok(cfg)
    } else {
        err(StatusCode::NOT_FOUND, "model not found")
    }
}

pub async fn toggle_model(
    State(app): State<std::sync::Arc<App>>,
    Path((id, model_id)): Path<(String, String)>,
) -> Response {
    let mut found = false;
    let cfg = app.write_config(|c| {
        if let Some(p) = c.providers.iter_mut().find(|p| p.id == id) {
            if let Some(m) = p.models.iter_mut().find(|m| m.id == model_id) {
                m.enabled = !m.enabled;
                found = true;
            }
        }
    });
    if found {
        ok(cfg)
    } else {
        err(StatusCode::NOT_FOUND, "model not found")
    }
}

/// GET /api/providers/{id}/upstream-models — fetch the provider's live /v1/models.
pub async fn upstream_models(
    State(app): State<std::sync::Arc<App>>,
    Path(id): Path<String>,
) -> Response {
    let cfg = app.read_config();
    let provider = match cfg.provider_by_id(&id) {
        Some(p) => p.clone(),
        None => return err(StatusCode::NOT_FOUND, "provider not found"),
    };
    if provider.keys.is_empty() {
        return err(StatusCode::BAD_REQUEST, "provider has no API keys configured");
    }
    let url = format!("{}/models", provider.base_url.trim_end_matches('/'));
    match app
        .http
        .get(&url)
        .bearer_auth(&provider.keys[0].key)
        .timeout(std::time::Duration::from_secs(15))
        .send()
        .await
    {
        Ok(resp) if resp.status().is_success() => {
            let mut models: Vec<String> = Vec::new();
            if let Ok(body) = resp.json::<serde_json::Value>().await {
                if let Some(list) = body.get("data").and_then(|d| d.as_array()) {
                    for m in list {
                        if let Some(mid) = m.get("id").and_then(|i| i.as_str()) {
                            models.push(mid.to_string());
                        }
                    }
                }
            }
            models.sort();
            ok(serde_json::json!({ "models": models }))
        }
        Ok(resp) => err(
            StatusCode::BAD_GATEWAY,
            &format!("upstream returned HTTP {}", resp.status()),
        ),
        Err(e) => err(StatusCode::BAD_GATEWAY, &format!("upstream unreachable: {e}")),
    }
}

#[derive(serde::Deserialize)]
pub struct ImportModelsInput {
    pub models: Vec<String>,
}

/// POST /api/providers/{id}/models/import — bulk import (idempotent: existing kept).
pub async fn import_models(
    State(app): State<std::sync::Arc<App>>,
    Path(id): Path<String>,
    Json(input): Json<ImportModelsInput>,
) -> Response {
    let incoming: Vec<String> = input
        .models
        .iter()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();
    if incoming.is_empty() {
        return err(StatusCode::BAD_REQUEST, "models list is empty");
    }
    let mut imported = 0usize;
    let cfg = app.write_config(|c| {
        if let Some(p) = c.providers.iter_mut().find(|p| p.id == id) {
            for mid in incoming {
                if !p.models.iter().any(|m| m.id == mid) {
                    p.models.push(ManagedModel { id: mid, enabled: true, context_length: None });
                    imported += 1;
                }
            }
        }
    });
    ok(serde_json::json!({ "imported": imported, "config": cfg }))
}

#[derive(serde::Deserialize)]
pub struct ContextInput {
    /// New context window in tokens. `None` = clear (unknown).
    pub context_length: Option<u32>,
}

/// PUT /api/providers/{id}/models/{model_id}/context-length — set a managed
/// model's `context_length` (informational; shown on the card as "Nk").
pub async fn set_model_context(
    State(app): State<std::sync::Arc<App>>,
    Path((id, model_id)): Path<(String, String)>,
    Json(input): Json<ContextInput>,
) -> Response {
    let cfg = app.write_config(|c| {
        if let Some(p) = c.providers.iter_mut().find(|p| p.id == id) {
            if let Some(m) = p.models.iter_mut().find(|m| m.id == model_id) {
                m.context_length = input.context_length;
            }
        }
    });
    ok(cfg)
}

// ---------- settings ----------

#[derive(serde::Deserialize)]
pub struct SettingsInput {
    pub default_cooldown_secs: u64,
    pub max_attempts: u32,
    pub auth: AuthSettings,
    pub api_port: u16,
    pub ui_port: u16,
}

pub async fn update_settings(
    State(app): State<std::sync::Arc<App>>,
    Json(input): Json<SettingsInput>,
) -> Response {
    let cfg = app.write_config(|c| {
        c.default_cooldown_secs = input.default_cooldown_secs.max(1);
        c.max_attempts = input.max_attempts.clamp(1, 10);
        c.auth = input.auth;
        c.api_port = input.api_port;
        c.ui_port = input.ui_port;
    });
    ok(cfg)
}

// ---------- config import / export ----------

/// GET /api/config/export — returns gateway.json as a download.
pub async fn export_config(State(app): State<std::sync::Arc<App>>) -> Response {
    let cfg = app.read_config();
    match serde_json::to_string_pretty(&cfg) {
        Ok(body) => {
            let filename = app
                .config_path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("gateway.json")
                .to_string();
            (
                StatusCode::OK,
                [
                    ("content-type", "application/json; charset=utf-8"),
                    (
                        "content-disposition",
                        Box::leak(format!("attachment; filename=\"{filename}\"\0").into_boxed_str())
                            .strip_suffix('\0')
                            .unwrap_or("attachment"),
                    ),
                ],
                body,
            )
                .into_response()
        }
        Err(e) => err(StatusCode::INTERNAL_SERVER_ERROR, &format!("serialize failed: {e}")),
    }
}

#[derive(serde::Deserialize)]
pub struct ImportConfigInput {
    /// Raw gateway.json content. Must be a complete, valid config: the whole file is
    /// rejected on any error (validate-then-swap).
    pub content: String,
}

/// POST /api/config/import — validate-then-swap hot reload of gateway.json.
/// In-flight requests finish on the old state (config is behind a RwLock); no restart.
pub async fn import_config(
    State(app): State<std::sync::Arc<App>>,
    Json(input): Json<ImportConfigInput>,
) -> Response {
    let cfg = match crate::config::Config::parse_str(&input.content) {
        Ok(c) => c,
        Err(e) => return err(StatusCode::BAD_REQUEST, &e.to_string()),
    };
    // Atomic swap under the write lock, then persist the new file to disk.
    {
        let mut cur = app.config.write().unwrap();
        *cur = cfg.clone();
    }
    if let Err(e) = cfg.save(&app.config_path) {
        return err(
            StatusCode::INTERNAL_SERVER_ERROR,
            &format!("imported in memory but failed to persist: {e}"),
        );
    }
    ok(cfg)
}

/// POST /api/config/import-legacy — selective merge for migrating a pre-UI-refactor
/// gateway config into the current one. Parses the imported config (lenient — serde
/// defaults fill the new fields the old config lacks), then adopts ONLY its
/// `providers` (which carry keys + models + `learned_cooldown`). The current global
/// settings (ports/auth/cooldown/max_attempts/strategy/model_groups/currency_rates)
/// are kept untouched — only provider data is replaced.
pub async fn import_legacy_config(
    State(app): State<std::sync::Arc<App>>,
    Json(input): Json<ImportConfigInput>,
) -> Response {
    let parsed = match crate::config::Config::parse_str(&input.content) {
        Ok(c) => c,
        Err(e) => return err(StatusCode::BAD_REQUEST, &e.to_string()),
    };
    let snapshot = app.write_config(|c| {
        c.providers = parsed.providers;
    });
    ok(snapshot)
}

// ---------- strategy ----------

/// GET /api/strategy — the current routing strategy (filter toggles + sort
/// stack). Read counterpart to `update_strategy`; the UI loads it on refresh.
pub async fn get_strategy(State(app): State<std::sync::Arc<App>>) -> Response {
    ok(app.read_config().strategy)
}

#[derive(serde::Deserialize)]
pub struct StrategyInput {
    pub filter: crate::config::Filter,
    #[serde(default)]
    pub sort: Vec<crate::config::SortKey>,
}

/// PUT /api/strategy — write the routing strategy (filter toggles + the ordered
/// sort stack, spec §5). The sort is validated (≤3 keys; serde rejects letters
/// outside A-J with a 400) and normalized (deduped) on write (impl-spec §4.2).
pub async fn update_strategy(
    State(app): State<std::sync::Arc<App>>,
    Json(input): Json<StrategyInput>,
) -> Response {
    if input.sort.len() > 3 {
        return err(StatusCode::BAD_REQUEST, "sort may have at most 3 keys");
    }
    let cfg = app.write_config(|c| {
        c.strategy.filter = input.filter;
        c.strategy.sort = input.sort;
        c.normalize_strategy();
    });
    ok(cfg)
}

/// POST /api/strategy/dry-run — simulate the selector for a model without
/// routing or advancing the cursor (impl-spec §8). `{model, provider?, sorts?}`
/// → `{result, retry_after, routed, candidates, cursor}`. `provider` qualifies a
/// bare model as `provider/model` (both-ON preview); `sorts` overrides the
/// configured stack to explore "what if" orderings. Never exposes `key_secret`.
pub async fn strategy_dry_run(
    State(app): State<std::sync::Arc<App>>,
    Json(input): Json<DryRunInput>,
) -> Response {
    let cfg = app.read_config();
    // Compose the model string: a given `provider` + a bare model → "provider/model".
    let model = match (&input.provider, input.model.split_once('/')) {
        (Some(p), None) => format!("{p}/{}", input.model),
        _ => input.model.clone(),
    };
    let set = match crate::strategy::build_candidates(&cfg, &model) {
        Ok(s) => s,
        Err(e) => return err(StatusCode::NOT_FOUND, &e),
    };
    let sorts = input.sorts.unwrap_or_else(|| cfg.strategy.sort.clone());
    if sorts.len() > 3 {
        return err(StatusCode::BAD_REQUEST, "sorts may have at most 3 keys");
    }
    ok(app.strategy_dry_run(&set, &sorts))
}

#[derive(serde::Deserialize)]
pub struct DryRunInput {
    pub model: String,
    #[serde(default)]
    pub provider: Option<String>,
    #[serde(default)]
    pub sorts: Option<Vec<crate::config::SortKey>>,
}

// ---------- model groups ----------

#[derive(serde::Deserialize)]
pub struct ModelGroupsInput {
    pub groups: Vec<crate::config::ModelGroup>,
}

/// GET /api/model-groups — the cross-provider model groups (spec §1, §2.3).
pub async fn list_model_groups(State(app): State<std::sync::Arc<App>>) -> Response {
    ok(app.read_config().model_groups)
}

/// PUT /api/model-groups — replace the whole group list. Each group id must be
/// non-empty and every entry's provider must exist in config (impl-spec §2.3);
/// an unknown provider is rejected with a 400 so the UI gets a clear error.
pub async fn update_model_groups(
    State(app): State<std::sync::Arc<App>>,
    Json(input): Json<ModelGroupsInput>,
) -> Response {
    let cfg = app.read_config();
    for g in &input.groups {
        if g.id.trim().is_empty() {
            return err(StatusCode::BAD_REQUEST, "group id is required");
        }
        for (pid, _um) in &g.entries {
            if cfg.provider_by_id(pid).is_none() {
                return err(
                    StatusCode::BAD_REQUEST,
                    &format!("group `{}` references unknown provider `{pid}`", g.id),
                );
            }
        }
    }
    let cfg = app.write_config(|c| {
        c.model_groups = input.groups;
    });
    ok(cfg)
}

// ---------- stats ----------

/// GET /api/stats — aggregated success/fail stats for the Status page.
pub async fn get_stats(State(app): State<std::sync::Arc<App>>) -> Response {
    let stats = app.stats.lock().unwrap().clone();
    let cfg = app.read_config();
    let now = now_ms();

    // Current hour + previous 23 buckets -> [{hour_ms, success, fail}]
    let cutoff = now / 3_600_000 - 23;
    let histogram: Vec<serde_json::Value> = (cutoff..=now / 3_600_000)
        .map(|h| {
            let b = stats.hourly.get(&h);
            serde_json::json!({
                "hour_ms": h * 3_600_000,
                "success": b.map(|b| b.success).unwrap_or(0),
                "fail": b.map(|b| b.fail).unwrap_or(0),
                "tokens": b.map(|b| b.tokens).unwrap_or(0),
            })
        })
        .collect();

    let key_name = |id: &str| -> String {
        cfg.providers
            .iter()
            .find_map(|p| p.keys.iter().find(|k| k.id == id))
            .map(|k| {
                // Show the masked key so rows are never blank even when the user
                // never set a label; the label (if any) prefixes it.
                if k.label.is_empty() {
                    mask(&k.key)
                } else {
                    format!("{} ({})", k.label, mask(&k.key))
                }
            })
            .unwrap_or_else(|| id.to_string())
    };
    let provider_name = |id: &str| -> String {
        cfg.provider_by_id(id)
            .map(|p| p.name.clone())
            .unwrap_or_else(|| id.to_string())
    };

    let keys: serde_json::Map<String, serde_json::Value> = stats
        .keys
        .iter()
        .map(|(id, b)| {
            (
                id.clone(),
                serde_json::json!({ "name": key_name(id), "success": b.success, "fail": b.fail, "tokens": b.tokens }),
            )
        })
        .collect();
    let providers: serde_json::Map<String, serde_json::Value> = stats
        .providers
        .iter()
        .map(|(id, b)| {
            (
                id.clone(),
                serde_json::json!({ "name": provider_name(id), "success": b.success, "fail": b.fail, "tokens": b.tokens,
                    "rotations": stats.rotations.get(id).copied().unwrap_or(0) }),
            )
        })
        .collect();
    let models: serde_json::Map<String, serde_json::Value> = stats
        .models
        .iter()
        .map(|(m, b)| {
            (
                m.clone(),
                serde_json::json!({ "success": b.success, "fail": b.fail, "tokens": b.tokens, "avg_tftt_ms": app.model_avg_tftt(m) }),
            )
        })
        .collect();

    ok(serde_json::json!({
        "now_ms": now,
        "total": { "success": stats.total.success, "fail": stats.total.fail, "tokens": stats.total.tokens },
        "rotations_total": stats.rotations.values().sum::<u64>(),
        "histogram": histogram,
        "keys": keys,
        "providers": providers,
        "models": models,
    }))
}

// ---------- status ----------

pub async fn status(State(app): State<std::sync::Arc<App>>) -> Response {
    let cfg = app.read_config();
    // Actual bound ports (may differ from config if the configured port was
    // occupied at startup and the fallback walked to a free one). 0 = not yet
    // bound (e.g. a unit-test App) → fall back to the configured value.
    let bound_api = app.bound_api_port.load(std::sync::atomic::Ordering::Relaxed);
    let bound_ui = app.bound_ui_port.load(std::sync::atomic::Ordering::Relaxed);
    let api_port = if bound_api != 0 { bound_api } else { cfg.api_port };
    let ui_port = if bound_ui != 0 { bound_ui } else { cfg.ui_port };
    let pool = app.status();
    let providers: Vec<serde_json::Value> = cfg
        .providers
        .iter()
        .map(|p| {
            let keys: Vec<serde_json::Value> = p
                .keys
                .iter()
                .map(|k| {
                    let cooling = pool
                        .get("cooling")
                        .and_then(|c| c.get(&k.id))
                        .cloned()
                        .unwrap_or(serde_json::Value::Null);
                    let invalid = pool
                        .get("invalid")
                        .and_then(|c| c.get(&k.id))
                        .cloned()
                        .unwrap_or(serde_json::Value::Null);
                    // Per-key strategy metrics (spec §2.2): the same snapshot the
                    // selector's fill_rows uses, exposed so the UI can show the
                    // success_rate / rpm / tpm / tftt / tps / token+bill balance
                    // columns the prototype had. None balances = +∞ (shown as 不限).
                    let m = app.metrics_of(&p.id, &k.id);
                    serde_json::json!({
                        "id": k.id,
                        "key": mask(&k.key),
                        "label": k.label,
                        "cooldown_secs": k.cooldown_secs,
                        "learned_cooldown": k.learned_cooldown,
                        "seed_rpm": k.seed_rpm,
                        "seed_tpm": k.seed_tpm,
                        "seed_success_rate": k.seed_success_rate,
                        "seed_avg_tftt_ms": k.seed_avg_tftt_ms,
                        "seed_tps": k.seed_tps,
                        "cooling": cooling,
                        "invalid": invalid,
                        "requests": pool.get("requests").and_then(|r| r.get(&k.id)).cloned().unwrap_or(serde_json::json!(0)),
                        "last_error": pool.get("last_error").and_then(|r| r.get(&k.id)).cloned().unwrap_or(serde_json::Value::Null),
                        "metrics": {
                            "success_rate": m.success_rate,
                            "rpm": m.rpm,
                            "tpm": m.tpm,
                            "avg_tftt_ms": m.avg_tftt_ms,
                            "tps": m.tps,
                            "token_balance": m.token_balance,
                            "bill_balance": m.bill_balance,
                        },
                    })
                })
                .collect();
            serde_json::json!({
                "id": p.id,
                "name": p.name,
                "base_url": p.base_url,
                "protocol": p.protocol,
                "aliases": p.aliases,
                "model_allowlist_only": p.model_allowlist_only,
                "has_token_balance_api": p.has_token_balance_api,
                "has_bill_balance_api": p.has_bill_balance_api,
                "rpm_limit": p.rpm_limit,
                "tpm_limit": p.tpm_limit,
                "price_table": p.price_table,
                "models": p.models,
                "keys": keys,
            })
        })
        .collect();
    ok(serde_json::json!({
        "now_ms": now_ms(),
        "default_cooldown_secs": cfg.default_cooldown_secs,
        "max_attempts": cfg.max_attempts,
        "auth": cfg.auth,
        "api_port": api_port,
        "ui_port": ui_port,
        "providers": providers,
    }))
}

fn mask(key: &str) -> String {
    let chars: Vec<char> = key.chars().collect();
    if chars.len() <= 8 {
        "••••".into()
    } else {
        let head: String = chars[..4].iter().collect();
        let tail: String = chars[chars.len() - 4..].iter().collect();
        format!("{head}••••{tail}")
    }
}
