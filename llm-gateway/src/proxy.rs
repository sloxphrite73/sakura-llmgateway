use crate::state::{now_ms, App};
use axum::body::Body;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use futures_util::StreamExt;
use std::time::Duration;

/// Status codes that trigger a key-rotate retry (before first byte).
fn is_retryable(status: u16) -> bool {
    status == 429 || status == 500 || status == 502 || status == 503 || status == 504
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

    let max_attempts = cfg.max_attempts.clamp(1, 10);
    let url = format!("{}/chat/completions", provider.base_url.trim_end_matches('/'));

    for attempt in 1..=max_attempts {
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
            // no total timeout; streams may be long (connect timeout set on client)
            .timeout(Duration::from_secs(600))
            .json(&payload)
            .send()
            .await;

        let resp = match resp {
            Ok(r) => r,
            Err(e) => {
                app.mark_cooldown(&provider_id, &key.id, None, &format!("network error: {e}"));
                if attempt < max_attempts {
                    continue;
                }
                return error_response(StatusCode::BAD_GATEWAY, format!("upstream unreachable: {e}"));
            }
        };

        let status = resp.status();
        if status.is_success() {
            app.clear_error(&key.id);
            return passthrough(resp).await;
        }

        // Error before first byte -> maybe rotate to next key.
        let retry_after = resp
            .headers()
            .get("retry-after")
            .and_then(|v| v.to_str().ok())
            .and_then(parse_retry_after);
        let snippet: String = resp.text().await.unwrap_or_default().chars().take(500).collect();
        app.mark_cooldown(
            &provider_id,
            &key.id,
            if status.as_u16() == 429 { retry_after } else { None },
            &format!("HTTP {}: {}", status.as_u16(), snippet),
        );

        if is_retryable(status.as_u16()) && attempt < max_attempts {
            continue;
        }

        // Not retryable or out of attempts -> return upstream error body as-is.
        return error_response(
            StatusCode::from_u16(status.as_u16()).unwrap_or(StatusCode::BAD_GATEWAY),
            format!("upstream error: {}", snippet),
        );
    }
    unreachable!()
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
