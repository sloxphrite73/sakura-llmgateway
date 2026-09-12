//! Mock OpenAI-compatible upstream for testing the gateway.
//! - `sk-bad` key always returns 429 with Retry-After: 2
//! - `sk-slow` key returns 429 unless at least `SLOW_WINDOW_SECS` have passed since its
//!   first request (simulates a rate-limit window, for testing cooldown learning)
//! - any other key returns a chat completion (SSE if "stream": true) and a model list.

use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use futures_util::StreamExt;
use std::sync::Arc;
use std::time::Duration;

/// Rate-limit window simulated for the `sk-slow` key (seconds).
const SLOW_WINDOW_SECS: u64 = 8;

#[tokio::main]
async fn main() {
    let slow_first = Arc::new(tokio::sync::Mutex::new(Option::<std::time::Instant>::None));
    let app = Router::new()
        .route("/v1/chat/completions", post(chat))
        .route("/v1/models", get(models))
        .with_state(slow_first);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:9001").await.unwrap();
    println!("[mock-upstream] listening on 127.0.0.1:9001");
    axum::serve(listener, app).await.unwrap();
}

async fn models(_state: State<Arc<tokio::sync::Mutex<Option<std::time::Instant>>>>) -> Response {
    (
        StatusCode::OK,
        Json(serde_json::json!({
            "object": "list",
            "data": [
                { "id": "mock-large", "object": "model" },
                { "id": "mock-small", "object": "model" }
            ]
        })),
    )
        .into_response()
}

async fn chat(
    State(slow_first): State<Arc<tokio::sync::Mutex<Option<std::time::Instant>>>>,
    headers: HeaderMap,
    Json(body): Json<serde_json::Value>,
) -> Response {
    let auth = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    if auth.contains("sk-bad") {
        return (
            StatusCode::TOO_MANY_REQUESTS,
            [("retry-after", "2")],
            Json(serde_json::json!({ "error": { "message": "rate limited" } })),
        )
            .into_response();
    }
    if auth.contains("sk-slow") {
        // 429 until SLOW_WINDOW_SECS have elapsed since the key's first request;
        // then behave like a normal key. Lets the prober's bisection converge.
        let mut first = slow_first.lock().await;
        let t0 = *first.get_or_insert(std::time::Instant::now());
        drop(first);
        if t0.elapsed() < Duration::from_secs(SLOW_WINDOW_SECS) {
            return (
                StatusCode::TOO_MANY_REQUESTS,
                Json(serde_json::json!({ "error": { "message": "rate limited (slow window)" } })),
            )
                .into_response();
        }
    }
    if auth.contains("sk-dead") {
        // Invalid credentials: retrying can never succeed — used to verify the
        // gateway's long quarantine for 401/403 keys.
        return (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({ "error": { "code": 16, "message": "Forbidden" } })),
        )
            .into_response();
    }

    let model = body.get("model").and_then(|m| m.as_str()).unwrap_or("?");
    let stream = body.get("stream").and_then(|s| s.as_bool()).unwrap_or(false);

    if !stream {
        return (
            StatusCode::OK,
            Json(serde_json::json!({
                "id": "chatcmpl-mock",
                "object": "chat.completion",
                "model": model,
                "choices": [{ "index": 0, "message": { "role": "assistant", "content": format!("hello from {model}") }, "finish_reason": "stop" }]
            })),
        )
            .into_response();
    }

    let chunks = vec![
        format!("data: {{\"id\":\"chatcmpl-mock\",\"model\":\"{model}\",\"choices\":[{{\"delta\":{{\"content\":\"hello \"}}}}]}}\n\n"),
        format!("data: {{\"id\":\"chatcmpl-mock\",\"model\":\"{model}\",\"choices\":[{{\"delta\":{{\"content\":\"from {model}\"}}}}]}}\n\n"),
        "data: [DONE]\n\n".to_string(),
    ];
    let stream = futures_util::stream::iter(chunks.into_iter().map(Ok::<String, std::io::Error>))
        .chain(futures_util::stream::once(async {
            tokio::time::sleep(Duration::from_millis(200)).await;
            Ok(String::new())
        }));
    (
        StatusCode::OK,
        [("content-type", "text/event-stream")],
        axum::body::Body::from_stream(stream),
    )
        .into_response()
}
