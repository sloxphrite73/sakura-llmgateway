use crate::config::{ApiKey, AuthSettings, ManagedModel, Provider};
use crate::state::{now_ms, App};
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;

fn ok<T: serde::Serialize>(v: T) -> Response {
    (StatusCode::OK, Json(v)).into_response()
}

fn err(status: StatusCode, msg: &str) -> Response {
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
            found = true;
        }
    });
    if found {
        ok(cfg)
    } else {
        err(StatusCode::NOT_FOUND, "provider not found")
    }
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
            p.models.push(ManagedModel { id: mid, enabled: true });
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
                    p.models.push(ManagedModel { id: mid, enabled: true });
                    imported += 1;
                }
            }
        }
    });
    ok(serde_json::json!({ "imported": imported, "config": cfg }))
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

// ---------- status ----------

pub async fn status(State(app): State<std::sync::Arc<App>>) -> Response {
    let cfg = app.read_config();
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
                    serde_json::json!({
                        "id": k.id,
                        "key": mask(&k.key),
                        "label": k.label,
                        "cooldown_secs": k.cooldown_secs,
                        "cooling": cooling,
                        "invalid": invalid,
                        "requests": pool.get("requests").and_then(|r| r.get(&k.id)).cloned().unwrap_or(serde_json::json!(0)),
                        "last_error": pool.get("last_error").and_then(|r| r.get(&k.id)).cloned().unwrap_or(serde_json::Value::Null),
                    })
                })
                .collect();
            serde_json::json!({
                "id": p.id,
                "name": p.name,
                "base_url": p.base_url,
                "aliases": p.aliases,
                "model_allowlist_only": p.model_allowlist_only,
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
        "api_port": cfg.api_port,
        "ui_port": cfg.ui_port,
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
