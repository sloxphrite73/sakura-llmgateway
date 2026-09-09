use crate::state::{now_ms, App};
use axum::body::Body;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use futures_util::StreamExt;
use std::time::Duration;

/// Status codes that mean "this key can't serve right now" -> rotate to the next key.
/// (429 rate-limit, key-scoped auth failures, request timeouts and upstream 5xx.)
fn should_rotate(status: u16) -> bool {
    status == 429
        || status == 401
        || status == 403
        || status == 408
        || status == 500
        || status == 502
        || status == 503
        || status == 504
}

fn parse_retry_after(v: &str) -> Option<u64> {
    v.trim().parse::<u64>().ok()
}

fn error_response(status: StatusCode, message: String) -> Response {
    let body = serde_json::json!({
        "error": { "message": message, "type": "gateway_error", "code": status.as_u16() }
    });
    (status, axum::Json(body)).into_response()
}

fn check_auth(app: &App, headers: &HeaderMap) -> Result<(), Response> {
    let cfg = app.read_config();
    if !cfg.auth.enabled {
        return Ok(());
    }
    let token = headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .unwrap_or("");
    if !token.is_empty() && cfg.auth.keys.iter().any(|k| k == token) {
        return Ok(());
    }
    Err(error_response(StatusCode::UNAUTHORIZED, "invalid or missing gateway API key".into()))
}

/// POST /v1/chat/completions
pub async fn chat_completions(
    State(app): State<std::sync::Arc<App>>,
    headers: HeaderMap,
    axum::Json(mut body): axum::Json<serde_json::Value>,
) -> Response {
    if let Err(resp) = check_auth(&app, &headers) {
        return resp;
    }

    let client_model = body.get("model").and_then(|m| m.as_str()).unwrap_or("").to_string();
    let (provider_id, upstream_model) = match app.read_config().resolve_model(&client_model) {
        Ok(r) => r,
        Err(reason) => {
            return error_response(StatusCode::NOT_FOUND, reason);
        }
    };
    body["model"] = serde_json::Value::String(upstream_model);

    let cfg = app.read_config();
    let provider = match cfg.provider_by_id(&provider_id) {
        Some(p) => p.clone(),
        None => return error_response(StatusCode::NOT_FOUND, "provider disappeared".into()),
    };
    if provider.keys.is_empty() {
        return error_response(
            StatusCode::SERVICE_UNAVAILABLE,
            format!("provider `{}` has no API keys configured", provider.name),
        );
    }

    // Retry budget: every key that fails is marked cooling, so pick_key() never hands out
    // the same key again until its cooldown expires. The budget is therefore bounded by
    // (one attempt per key) x max_attempts, and the request only fails to the client once
    // the whole pool is exhausted (pick_key -> None). Usable keys are never left untried.
    let max_attempts = (cfg.max_attempts.clamp(1, 10) as usize)
        .saturating_mul(provider.keys.len())
        .max(1);
    let url = format!("{}/chat/completions", provider.base_url.trim_end_matches('/'));

    for _attempt in 1..=max_attempts {
        let key = match app.pick_key(&provider) {
            Some(k) => k,
            None => {
                // All keys cooling down -> fail fast with 429 (per spec).
                let cooling: Vec<String> = provider.keys.iter().map(|k| k.label.clone()).collect();
                return error_response(
                    StatusCode::TOO_MANY_REQUESTS,
                    format!(
                        "all {} key(s) for provider `{}` are rate-limited (cooling down). Keys: {}",
                        provider.keys.len(),
                        provider.name,
                        cooling.join(", ")
                    ),
                );
            }
        };

        let payload = body.clone();
        let resp = app
            .http
            .post(&url)
            .bearer_auth(&key.key)
            // long enough for streams; connect timeout is set on the client
            .timeout(Duration::from_secs(600))
            .json(&payload)
            .send()
            .await;

        let resp = match resp {
            Ok(r) => r,
            Err(e) => {
                // Timeout / connection failure: rotate to the next key. The key gets a
                // short cooldown (not a rate-limit cooldown) so it recovers quickly.
                app.mark_cooldown(&provider_id, &key.id, Some(5), &format!("network error: {e}"));
                continue;
            }
        };

        let status = resp.status();
        if status.is_success() {
            app.clear_error(&key.id);
            return passthrough(resp).await;
        }

        // Error before first byte. Rotate to the next key for key-scoped failures
        // (429 / 401 / 403 / 408 / 5xx); a 4xx that is the *request's* fault (e.g. 400
        // bad body) would fail identically on every key, so return it immediately.
        let retry_after = resp
            .headers()
            .get("retry-after")
            .and_then(|v| v.to_str().ok())
            .and_then(parse_retry_after);
        let snippet: String = resp.text().await.unwrap_or_default().chars().take(500).collect();
        let reason = format!("HTTP {}: {}", status.as_u16(), snippet);

        if should_rotate(status.as_u16()) {
            if status.as_u16() == 429 {
                app.mark_cooldown(&provider_id, &key.id, retry_after, &reason);
            } else {
                // Short cooldown for rotated-out keys: long enough to not retry it this
                // round, short enough that fixing the key is picked up quickly.
                app.mark_cooldown(&provider_id, &key.id, Some(5), &reason);
            }
            // Loop continues: pick_key() skips cooling keys and serves a fresh one.
            continue;
        }

        // Request-scoped client error: same result on every key — don't burn the pool.
        return error_response(
            StatusCode::from_u16(status.as_u16()).unwrap_or(StatusCode::BAD_GATEWAY),
            format!("upstream error: {}", snippet),
        );
    }

    // Only reachable if a short cooldown expires mid-loop and keys keep failing until the
    // budget runs out. Never panic on the request path — fail fast with 429 instead.
    error_response(
        StatusCode::TOO_MANY_REQUESTS,
        format!("retry budget exhausted for provider `{}` (all keys failing)", provider.name),
    )
}

fn passthrough(resp: reqwest::Response) -> impl std::future::Future<Output = Response> {
    async move {
        let mut headers = HeaderMap::new();
        for (name, value) in resp.headers() {
            if name == reqwest::header::CONTENT_LENGTH || name == reqwest::header::TRANSFER_ENCODING {
                continue;
            }
            if let (Ok(n), Ok(v)) = (
                axum::http::HeaderName::from_bytes(name.as_str().as_bytes()),
                axum::http::HeaderValue::from_bytes(value.as_bytes()),
            ) {
                headers.insert(n, v);
            }
        }
        let stream = resp.bytes_stream().map(|r| r.map_err(std::io::Error::other));
        let mut response = Response::new(Body::from_stream(stream));
        *response.headers_mut() = headers;
        response
    }
}

/// GET /v1/models — serves the managed model catalog per provider (`provider/model`),
/// plus aliases. Providers with no managed models (unmanaged) contribute nothing.
pub async fn list_models(
    State(app): State<std::sync::Arc<App>>,
    headers: HeaderMap,
) -> Response {
    if let Err(resp) = check_auth(&app, &headers) {
        return resp;
    }
    let cfg = app.read_config();

    let mut out: Vec<serde_json::Value> = Vec::new();
    for p in &cfg.providers {
        for m in &p.models {
            if !m.enabled {
                continue;
            }
            out.push(serde_json::json!({
                "id": format!("{}/{}", p.name, m.id),
                "object": "model",
                "owned_by": p.name,
                "provider_id": p.id,
                "upstream_id": m.id,
            }));
        }
        for (alias, target) in &p.aliases {
            if !p.model_allowed(target) {
                continue; // alias pointing to a missing/disabled model is not advertised
            }
            out.push(serde_json::json!({
                "id": alias,
                "object": "model",
                "owned_by": p.name,
                "provider_id": p.id,
                "upstream_id": target,
                "alias": true,
            }));
        }
    }

    (
        StatusCode::OK,
        axum::Json(serde_json::json!({ "object": "list", "data": out, "created": now_ms() / 1000 })),
    )
        .into_response()
}
