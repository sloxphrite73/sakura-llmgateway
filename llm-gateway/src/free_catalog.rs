//! Free-provider catalog: curated list of OpenAI-compatible upstreams with free
//! tiers, embedded at compile time and refreshable from GitHub.
//!
//! Design notes (per the approved plan):
//! - The catalog is data only. Adding a provider from it goes through the exact
//!   same admin APIs (create_provider / import_models / add_key) as manual setup —
//!   the catalog never creates special provider types.
//! - A remote refresh fetches the catalog JSON from GitHub raw, validates the
//!   structure, and caches it in memory. It never silently replaces the embedded
//!   catalog on disk: if the gateway restarts it falls back to the embedded copy
//!   (the UI shows the fetched content for the session, which the user chose).
//! - `guide_url` fields are repo-relative links to tutorial docs; the UI rewrites
//!   them into GitHub blob URLs.

use axum::extract::State;
use axum::http::StatusCode;
use serde::Deserialize;
use std::sync::Mutex;

/// Embedded at compile time from `free-catalog.json` next to this crate's Cargo.toml.
pub const EMBEDDED_CATALOG: &str = include_str!("../free-catalog.json");

/// Where the UI can fetch a fresher catalog. Update the ref when the catalog evolves.
pub const REMOTE_CATALOG_URL: &str = "https://raw.githubusercontent.com/sloxphrite73/sakura-llmgateway/main/llm-gateway/free-catalog.json";

/// GitHub base for turning repo-relative `guide_url`s into clickable links.
pub const GITHUB_BASE: &str = "https://github.com/sloxphrite73/sakura-llmgateway/blob/main/";

#[derive(Debug, Clone, Deserialize, serde::Serialize)]
pub struct CatalogEntry {
    pub id: String,
    pub name: String,
    pub base_url: String,
    #[serde(default = "default_protocol")]
    pub protocol: String,
    #[serde(default)]
    pub keyless: bool,
    pub signup_url: String,
    /// Repo-relative doc path, resolved to a GitHub URL by the UI.
    pub guide_url: String,
    pub notes_zh: String,
    pub notes_en: String,
    pub free_models: Vec<String>,
}

fn default_protocol() -> String {
    "openai".into()
}

#[derive(Debug, Clone, Deserialize, serde::Serialize)]
pub struct Catalog {
    pub version: u32,
    pub updated: String,
    pub providers: Vec<CatalogEntry>,
}

/// App-level state: the latest successfully-fetched remote catalog (session only).
#[derive(Default)]
pub struct CatalogCache {
    remote: Mutex<Option<Catalog>>,
}

impl CatalogCache {
    pub fn new() -> Self {
        Self { remote: Mutex::new(None) }
    }
}

fn parse_catalog(raw: &str) -> Result<Catalog, String> {
    let c: Catalog =
        serde_json::from_str(raw).map_err(|e| format!("invalid catalog JSON: {e}"))?;
    if c.providers.is_empty() {
        return Err("catalog has no providers".into());
    }
    // Every entry must be usable: non-empty essentials, openai protocol only for now.
    for p in &c.providers {
        if p.id.trim().is_empty() || p.name.trim().is_empty() || p.base_url.trim().is_empty() {
            return Err(format!("entry `{}` missing id/name/base_url", p.id));
        }
        if p.protocol != "openai" {
            return Err(format!(
                "entry `{}` has protocol `{}` — catalog currently supports openai only",
                p.id, p.protocol
            ));
        }
    }
    Ok(c)
}

fn with_guide_links(c: &Catalog) -> serde_json::Value {
    let providers: Vec<serde_json::Value> = c
        .providers
        .iter()
        .map(|p| {
            let mut v = serde_json::to_value(p).unwrap_or(serde_json::Value::Null);
            if let Some(obj) = v.as_object_mut() {
                obj.insert(
                    "guide_link".into(),
                    serde_json::json!(format!("{}{}", GITHUB_BASE, p.guide_url.trim_start_matches("./"))),
                );
            }
            v
        })
        .collect();
    serde_json::json!({
        "version": c.version,
        "updated": c.updated,
        "source": "embedded",
        "providers": providers,
    })
}

fn with_guide_links_remote(mut v: serde_json::Value) -> serde_json::Value {
    if let Some(obj) = v.as_object_mut() {
        obj.insert("source".into(), serde_json::json!("remote"));
        if let Some(providers) = obj.get_mut("providers").and_then(|p| p.as_array_mut()) {
            for p in providers {
                if let Some(po) = p.as_object_mut() {
                    if let Some(guide) = po.get("guide_url").and_then(|g| g.as_str()) {
                        let link = format!("{}{}", GITHUB_BASE, guide.trim_start_matches("./"));
                        po.insert("guide_link".into(), serde_json::json!(link));
                    }
                }
            }
        }
    }
    v
}

/// GET /api/free-catalog — embedded catalog, or the cached remote copy if a
/// refresh happened this session.
pub async fn get_catalog(State(app): State<std::sync::Arc<crate::state::App>>) -> axum::response::Response {
    let cached = app
        .catalog
        .remote
        .lock()
        .unwrap()
        .clone();
    match cached {
        Some(c) => crate::admin::ok(with_guide_links_remote(
            serde_json::to_value(&c).unwrap_or(serde_json::Value::Null),
        )),
        None => match parse_catalog(EMBEDDED_CATALOG) {
            Ok(c) => crate::admin::ok(with_guide_links(&c)),
            // Embedded JSON is compile-checked by tests; this is unreachable in practice.
            Err(e) => crate::admin::err(StatusCode::INTERNAL_SERVER_ERROR, &e),
        },
    }
}

/// POST /api/free-catalog/refresh — fetch the latest catalog from GitHub raw,
/// validate it, and cache it for this session.
pub async fn refresh_catalog(State(app): State<std::sync::Arc<crate::state::App>>) -> axum::response::Response {
    let resp = app
        .http
        .get(REMOTE_CATALOG_URL)
        .header("User-Agent", "sakura-llmgateway")
        .send()
        .await;
    let body = match resp {
        Ok(r) if r.status().is_success() => match r.text().await {
            Ok(t) => t,
            Err(e) => return crate::admin::err(StatusCode::BAD_GATEWAY, &format!("read failed: {e}")),
        },
        Ok(r) => {
            return crate::admin::err(
                StatusCode::BAD_GATEWAY,
                &format!("remote returned {}", r.status()),
            )
        }
        Err(e) => {
            return crate::admin::err(
                StatusCode::BAD_GATEWAY,
                &format!("fetch failed (offline?): {e}"),
            )
        }
    };
    let catalog = match parse_catalog(&body) {
        Ok(c) => c,
        Err(e) => {
            return crate::admin::err(
                StatusCode::BAD_GATEWAY,
                &format!("remote catalog rejected: {e}"),
            )
        }
    };
    let val = with_guide_links_remote(serde_json::to_value(&catalog).unwrap_or(serde_json::Value::Null));
    *app.catalog.remote.lock().unwrap() = Some(catalog);
    crate::admin::ok(val)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_catalog_parses_and_is_valid() {
        let c = parse_catalog(EMBEDDED_CATALOG).expect("embedded catalog must parse");
        assert!(c.providers.len() >= 5);
        for p in &c.providers {
            assert!(p.free_models.len() >= 2, "{} needs models", p.id);
            assert!(p.guide_url.starts_with("doc/"), "{} guide in repo", p.id);
            assert!(p.signup_url.starts_with("https://"), "{} signup https", p.id);
        }
    }

    #[test]
    fn rejects_bad_protocol() {
        let raw = r#"{"version":1,"updated":"x","providers":[{"id":"a","name":"a","base_url":"https://x/v1","protocol":"anthropic","signup_url":"https://x","guide_url":"doc/x.md","notes_zh":"","notes_en":"","free_models":["m1","m2"]}]}"#;
        assert!(parse_catalog(raw).is_err());
    }

    #[test]
    fn rejects_empty_providers() {
        assert!(parse_catalog(r#"{"version":1,"updated":"x","providers":[]}"#).is_err());
    }
}
