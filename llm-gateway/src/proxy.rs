use crate::state::{now_ms, App};
use axum::body::Body;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use futures_util::StreamExt;
use std::sync::Arc;
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
    State(app): State<Arc<App>>,
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
            // Unresolvable model = no key touched it; still count it as a failed request.
            app.record_stat(false, None, "unknown", &client_model);
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
        app.record_stat(false, None, &provider_id, &client_model);
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
                app.record_stat(false, None, &provider_id, &client_model);
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
                // Per-key tally: this key failed and is being rotated out.
                app.record_key_stat(false, &key.id);
                app.record_rotation(&provider_id);
                app.mark_cooldown(&provider_id, &key.id, Some(5), &format!("network error: {e}"));
                continue;
            }
        };

        let status = resp.status();
        if status.is_success() {
            app.clear_error(&key.id);
            return finish_ok(
                &app,
                resp,
                &provider_id,
                &key.id,
                &client_model,
                body.get("stream").and_then(|s| s.as_bool()).unwrap_or(false),
            )
            .await;
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
            let code = status.as_u16();
            // Per-key tally: this key failed and is being rotated out. The client-level
            // request is counted separately by whichever key ultimately serves it.
            app.record_key_stat(false, &key.id);
            app.record_rotation(&provider_id);
            if code == 429 {
                // Rate limited: honor Retry-After (or the learned/global default), and
                // back off briefly before hammering the next key — TPM limits are
                // per-minute, so an instant retry with the same large payload burns the
                // next key too. A background prober then measures the key's real window.
                app.mark_cooldown(&provider_id, &key.id, retry_after, &reason);
                // Probe with the first enabled managed model (real upstreams reject
                // unknown models with 400, which made the old "probe" model unlearnable).
                let probe_model = provider
                    .models
                    .iter()
                    .find(|m| m.enabled)
                    .map(|m| m.id.clone())
                    .unwrap_or_else(|| "probe".to_string());
                spawn_prober(
                    app.clone(),
                    provider_id.clone(),
                    key.id.clone(),
                    key.key.clone(),
                    url.clone(),
                    probe_model,
                );
                tokio::time::sleep(Duration::from_millis(1500)).await;
            } else if code == 401 || code == 403 {
                // Auth failure = the key itself is dead (invalid/revoked/out of quota
                // tier). A 5s cooldown just lets round-robin feed it again forever;
                // quarantine it for 30 minutes and surface it as 无效 in the UI.
                app.mark_invalid(&provider_id, &key.id, 30 * 60, &reason);
            } else {
                // Other key-scoped errors (408/5xx): short rotate-out cooldown, long
                // enough to not retry this round, short enough to recover quickly.
                app.mark_cooldown(&provider_id, &key.id, Some(5), &reason);
            }
            // Loop continues: pick_key() skips cooling/invalid keys and serves a fresh one.
            continue;
        }

        // Request-scoped client error: same result on every key — don't burn the pool.
        app.record_stat(false, Some(&key.id), &provider_id, &client_model);
        return error_response(
            StatusCode::from_u16(status.as_u16()).unwrap_or(StatusCode::BAD_GATEWAY),
            format!("upstream error: {}", snippet),
        );
    }

    // Only reachable if a short cooldown expires mid-loop and keys keep failing until the
    // budget runs out. Never panic on the request path — fail fast with 429 instead.
    app.record_stat(false, None, &provider_id, &client_model);
    error_response(
        StatusCode::TOO_MANY_REQUESTS,
        format!("retry budget exhausted for provider `{}` (all keys failing)", provider.name),
    )
}

/// Wrap a successful upstream response: stats counting and interruption handling happen
/// around the body, since "did the client receive bytes" is only known while streaming.
fn finish_ok(
    app: &Arc<App>,
    resp: reqwest::Response,
    provider_id: &str,
    key_id: &str,
    client_model: &str,
    is_stream: bool,
) -> impl std::future::Future<Output = Response> {
    let app = app.clone();
    let provider_id = provider_id.to_string();
    let key_id = key_id.to_string();
    let client_model = client_model.to_string();
    async move {
        // Headers first — the client must receive them regardless of how the body ends.
        let status = resp.status();
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

        // --- non-streaming: buffer fully so upstream deaths before completion count as
        // a fail (and never deliver a truncated body as a success).
        if !is_stream {
            match resp.bytes().await {
                Ok(bytes) => {
                    let ok = serde_json::from_slice::<serde_json::Value>(&bytes)
                        .map(|v| v.get("error").is_none())
                        .unwrap_or(true);
                    app.record_stat(ok, Some(&key_id), &provider_id, &client_model);
                    let mut response = Response::new(Body::from(bytes));
                    *response.headers_mut() = headers;
                    *response.status_mut() = status;
                    return response;
                }
                Err(e) => {
                    // Connection dropped before the client saw anything: counted as a
                    // failed request.
                    app.record_stat(false, Some(&key_id), &provider_id, &client_model);
                    return error_response(
                        StatusCode::BAD_GATEWAY,
                        format!("upstream connection failed mid-body: {e}"),
                    );
                }
            }
        }

        // --- streaming: forward chunks as they arrive. Success is counted up-front
        // (the key served the request); a mid-stream interruption after bytes were
        // sent to the client cannot be retried, so we append an OpenAI-style SSE error
        // chunk and end the stream cleanly.
        app.record_stat(true, Some(&key_id), &provider_id, &client_model);
        let upstream = resp.bytes_stream();
        let stream = upstream.map(move |r| match r {
            Ok(chunk) => Ok(chunk),
            Err(e) => {
                let err_chunk = format!(
                    "data: {}\n\ndata: [DONE]\n\n",
                    serde_json::json!({
                        "error": {
                            "message": format!("stream interrupted before finish_reason: {e}"),
                            "type": "gateway_error",
                            "code": 502,
                        }
                    })
                );
                Ok::<_, std::io::Error>(err_chunk.into_bytes().into())
            }
        });
        let mut response = Response::new(Body::from_stream(stream));
        *response.headers_mut() = headers;
        *response.status_mut() = status;
        response
    }
}

/// Background prober: after a key gets 429'd, measure its real rate-limit window with a
/// binary search. Start from T (the key's learned_cooldown if one exists, else the
/// global default): probe at T — success means the window is shorter (explore [T/2, T]),
/// 429 means it is longer (explore [T, 2T]). Then bisect the bracket for a few rounds
/// and store the midpoint as the key's `learned_cooldown` (debounced config write).
///
/// Bounded: bisection rounds and probe count are capped so a dead/limited key doesn't
/// burn quota forever; only one prober per key runs at a time (`claim_probe`).
fn spawn_prober(
    app: Arc<App>,
    provider_id: String,
    key_id: String,
    key_secret: String,
    url: String,
    probe_model: String,
) {
    /// Minimum window the search considers (seconds).
    const PROBE_MIN_SECS: u64 = 2;
    /// Maximum window the search considers (seconds) — 15 min covers common per-minute
    /// and per-hour windows without probing all day.
    const PROBE_MAX_SECS: u64 = 900;
    /// Bisection iterations after the initial bracket is found.
    const PROBE_BISECT_ROUNDS: u32 = 5;
    /// Hard cap on probes per measurement (bracketing + bisection).
    const PROBE_MAX_PROBES: u32 = 10;
    /// Re-learn interval: recalibrate only if the last measurement is older than this.
    const RELEARN_AFTER_SECS: u64 = 3600;

    if !app.claim_probe(&key_id) {
        return; // a prober is already measuring this key
    }
    tokio::spawn(async move {
        // Skip re-learning when a fresh measurement already exists.
        if app.learned_cooldown_age_secs(&provider_id, &key_id)
            .map(|age| age < RELEARN_AFTER_SECS)
            .unwrap_or(false)
        {
            app.release_probe(&key_id);
            return;
        }

        // Initial guess: previous learned value, else the global default cooldown.
        let t = app
            .learned_cooldown_of(&provider_id, &key_id)
            .unwrap_or_else(|| app.default_cooldown_secs())
            .clamp(PROBE_MIN_SECS, PROBE_MAX_SECS);

        let probe_body = serde_json::json!({
            "model": probe_model,
            "messages": [{"role": "user", "content": "1"}],
            "max_tokens": 1,
        });
        let mut probes_used = 0u32;

        // One probe after waiting `secs`. Ok(true)=success, Ok(false)=still 429,
        // Err=unlearnable (401/403/4xx/network) — abort without writing a value.
        async fn probe(
            app: &Arc<App>,
            url: &str,
            key_secret: &str,
            body: &serde_json::Value,
            secs: u64,
            probes_used: &mut u32,
        ) -> Result<bool, ()> {
            if *probes_used >= PROBE_MAX_PROBES {
                return Err(());
            }
            *probes_used += 1;
            tokio::time::sleep(Duration::from_secs(secs)).await;
            let result = app
                .http
                .post(url)
                .bearer_auth(key_secret)
                .timeout(Duration::from_secs(20))
                .json(body)
                .send()
                .await;
            match result {
                Ok(r) if r.status().is_success() => Ok(true),
                Ok(r) if r.status().as_u16() == 429 => Ok(false),
                Ok(r) if r.status().as_u16() == 401 || r.status().as_u16() == 403 => Err(()),
                Ok(_) | Err(_) => Err(()), // unknown model / upstream hiccup: inconclusive
            }
        }

        // Phase 1 — bracket the window around the initial guess T.
        let (mut low, mut high) = match probe(&app, &url, &key_secret, &probe_body, t, &mut probes_used).await {
            Ok(true) => (t / 2, t), // window is shorter than T
            Ok(false) => (t, (t * 2).min(PROBE_MAX_SECS)), // longer than T
            Err(()) => {
                app.release_probe(&key_id);
                return;
            }
        };

        // Phase 2 — bisect [low, high] until the bracket is tight.
        for _ in 0..PROBE_BISECT_ROUNDS {
            if high - low <= 1 {
                break;
            }
            let mid = (low + high) / 2;
            match probe(&app, &url, &key_secret, &probe_body, mid, &mut probes_used).await {
                Ok(true) => high = mid,
                Ok(false) => low = mid,
                Err(()) => break, // keep whatever bracket we have; still write midpoint
            }
        }

        app.release_probe(&key_id);
        let learned = (low + high) / 2;
        if learned >= PROBE_MIN_SECS {
            app.set_learned_cooldown(&provider_id, &key_id, learned);
            app.clear_error(&key_id);
        }
    });
}

/// GET /v1/models — serves the managed model catalog per provider (`provider/model`),
/// plus aliases. Providers with no managed models (unmanaged) contribute nothing.
pub async fn list_models(
    State(app): State<Arc<App>>,
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
