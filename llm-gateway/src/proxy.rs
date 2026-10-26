use crate::protocol::{
    self, AnthropicToOpenAiStream, OpenAiToAnthropicStream, Protocol,
};
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

/// Error response rendered in the *client's* protocol (Anthropic clients expect
/// `{"type":"error","error":{...}}` shapes).
fn error_response_for(status: StatusCode, message: String, inbound: Protocol) -> Response {
    match inbound {
        Protocol::OpenAi => error_response(status, message),
        Protocol::Anthropic => {
            let body = serde_json::json!({
                "type": "error",
                "error": { "type": "gateway_error", "message": message }
            });
            (status, axum::Json(body)).into_response()
        }
    }
}

/// Like `error_response_for`, but attaches a `Retry-After` header (delta-seconds)
/// when given — used for the spec §6 "all-cooling" 429 so the client learns the
/// soonest a valid key recovers (learned_cooldown-driven; the OmniRoute
/// differentiator: precise seconds, not a fixed dead value).
fn error_response_with_retry(
    status: StatusCode,
    message: String,
    inbound: Protocol,
    retry_after: Option<u64>,
) -> Response {
    let mut resp = error_response_for(status, message, inbound);
    if let Some(secs) = retry_after {
        if let Ok(val) = axum::http::HeaderValue::from_str(&secs.to_string()) {
            resp.headers_mut().insert(axum::http::header::RETRY_AFTER, val);
        }
    }
    resp
}

/// Gateway-level auth. Accepts the token from `Authorization: Bearer <t>` or —
/// for Anthropic clients (Claude Code etc.) — from `x-api-key: <t>`.
fn extract_auth_token(headers: &HeaderMap) -> String {
    if let Some(v) = headers.get(axum::http::header::AUTHORIZATION) {
        if let Ok(s) = v.to_str() {
            if let Some(t) = s.strip_prefix("Bearer ") {
                return t.to_string();
            }
        }
    }
    headers
        .get("x-api-key")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string()
}

fn check_auth(app: &App, headers: &HeaderMap) -> Result<(), Response> {
    let cfg = app.read_config();
    if !cfg.auth.enabled {
        return Ok(());
    }
    let token = extract_auth_token(headers);
    if !token.is_empty() && cfg.auth.keys.iter().any(|k| k == &token) {
        return Ok(());
    }
    Err(error_response(StatusCode::UNAUTHORIZED, "invalid or missing gateway API key".into()))
}

// ---------------------------------------------------------------------------
// Shared upstream-call plumbing
// ---------------------------------------------------------------------------

/// Upstream URL for a chat-completion-style call, following each protocol's
/// base_url convention: OpenAI upstreams take `{base}/chat/completions`
/// (base usually ends in `/v1`); Anthropic upstreams take `{base}/messages`
/// (base may be the API root *or* already include `/v1`).
fn upstream_chat_url(base: &str, protocol: Protocol) -> String {
    let base = base.trim_end_matches('/');
    match protocol {
        Protocol::OpenAi => format!("{base}/chat/completions"),
        Protocol::Anthropic => {
            if base.ends_with("/v1") {
                format!("{base}/messages")
            } else {
                format!("{base}/v1/messages")
            }
        }
    }
}

/// Apply the upstream's auth headers to a request builder.
pub(crate) fn apply_upstream_auth(
    builder: reqwest::RequestBuilder,
    key: &str,
    protocol: Protocol,
) -> reqwest::RequestBuilder {
    match protocol {
        Protocol::OpenAi => builder.bearer_auth(key),
        Protocol::Anthropic => builder
            .header("x-api-key", key)
            .header("anthropic-version", "2023-06-01"),
    }
}

/// One upstream chat call with the given key. Returns the raw reqwest response.
async fn call_upstream(
    app: &App,
    provider: &crate::config::Provider,
    key_secret: &str,
    url: &str,
    payload: &serde_json::Value,
) -> Result<reqwest::Response, reqwest::Error> {
    let builder = apply_upstream_auth(
        app.http.post(url).timeout(Duration::from_secs(600)),
        key_secret,
        provider.protocol(),
    );
    builder.json(payload).send().await
}

// ---------------------------------------------------------------------------
// POST /v1/chat/completions
// ---------------------------------------------------------------------------

/// POST /v1/chat/completions
pub async fn chat_completions(
    State(app): State<Arc<App>>,
    headers: HeaderMap,
    axum::Json(body): axum::Json<serde_json::Value>,
) -> Response {
    if let Err(resp) = check_auth(&app, &headers) {
        return resp;
    }
    forward(
        app,
        body,
        Protocol::OpenAi,
        |app, payload, provider_id, key_id, client_model, is_stream| {
            finish_openai(app, payload, provider_id, key_id, client_model, is_stream)
        },
    )
    .await
}

// ---------------------------------------------------------------------------
// POST /v1/messages (+ count_tokens)
// ---------------------------------------------------------------------------

/// POST /v1/messages — Anthropic Messages inbound.
pub async fn anthropic_messages(
    State(app): State<Arc<App>>,
    headers: HeaderMap,
    axum::Json(body): axum::Json<serde_json::Value>,
) -> Response {
    if let Err(resp) = check_auth(&app, &headers) {
        return resp;
    }
    forward(
        app,
        body,
        Protocol::Anthropic,
        |app, payload, provider_id, key_id, client_model, is_stream| {
            finish_anthropic(app, payload, provider_id, key_id, client_model, is_stream)
        },
    )
    .await
}

/// POST /v1/messages/count_tokens — proxy to the upstream when it speaks
/// Anthropic; local estimate otherwise (never 404: Claude Code depends on this).
pub async fn anthropic_count_tokens(
    State(app): State<Arc<App>>,
    headers: HeaderMap,
    axum::Json(body): axum::Json<serde_json::Value>,
) -> Response {
    if let Err(resp) = check_auth(&app, &headers) {
        return resp;
    }
    let client_model = body.get("model").and_then(|m| m.as_str()).unwrap_or("").to_string();
    let cfg = app.read_config();
    let (provider_id, upstream_model) = match cfg.resolve_model(&client_model) {
        Ok(r) => r,
        Err(reason) => {
            return error_response_for(StatusCode::NOT_FOUND, reason, Protocol::Anthropic)
        }
    };
    let provider = match cfg.provider_by_id(&provider_id) {
        Some(p) => p.clone(),
        None => {
            return error_response_for(
                StatusCode::NOT_FOUND,
                "provider disappeared".into(),
                Protocol::Anthropic,
            )
        }
    };

    let mut req = body.clone();
    req["model"] = serde_json::Value::String(upstream_model);

    match provider.protocol() {
        Protocol::Anthropic => {
            // F3: first non-cooling, non-invalid key — not blindly keys[0].
            let key = app.first_usable_key(&provider).unwrap_or_default();
            let base = provider.base_url.trim_end_matches('/');
            let url = if base.ends_with("/v1") {
                format!("{base}/messages/count_tokens")
            } else {
                format!("{base}/v1/messages/count_tokens")
            };
            let builder = apply_upstream_auth(
                app.http.post(&url).timeout(Duration::from_secs(30)),
                &key,
                Protocol::Anthropic,
            );
            match builder.json(&req).send().await {
                Ok(resp) => {
                    let status = StatusCode::from_u16(resp.status().as_u16())
                        .unwrap_or(StatusCode::BAD_GATEWAY);
                    let bytes = resp.bytes().await.unwrap_or_default();
                    (status, bytes).into_response()
                }
                Err(e) => error_response_for(
                    StatusCode::BAD_GATEWAY,
                    format!("upstream count_tokens failed: {e}"),
                    Protocol::Anthropic,
                ),
            }
        }
        Protocol::OpenAi => {
            let est = protocol::estimate_tokens_anthropic_request(&req);
            (
                StatusCode::OK,
                axum::Json(serde_json::json!({ "input_tokens": est })),
            )
                .into_response()
        }
    }
}

// ---------------------------------------------------------------------------
// Core forwarder: protocol-agnostic key-pool loop
// ---------------------------------------------------------------------------

/// Build the request payload in the *upstream's* protocol, with the upstream
/// model name substituted in. Extracted from the old inline block so the
/// strategy layer can rebuild it per attempt when the selected candidate's
/// provider/model differs (impl-spec §5.2: "鎸?attempt 璋冪敤").
fn build_upstream_payload(
    inbound_body: &serde_json::Value,
    inbound: Protocol,
    provider: &crate::config::Provider,
    upstream_model: &str,
) -> serde_json::Value {
    let mut b = match (inbound, provider.protocol()) {
        (Protocol::OpenAi, Protocol::OpenAi) | (Protocol::Anthropic, Protocol::Anthropic) => {
            let mut b = inbound_body.clone();
            b["model"] = serde_json::Value::String(upstream_model.to_string());
            b
        }
        (Protocol::Anthropic, Protocol::OpenAi) => {
            let mut b = protocol::anthropic_to_openai_request(inbound_body);
            b["model"] = serde_json::Value::String(upstream_model.to_string());
            b
        }
        (Protocol::OpenAi, Protocol::Anthropic) => {
            let mut b = protocol::openai_to_anthropic_request(inbound_body);
            b["model"] = serde_json::Value::String(upstream_model.to_string());
            b
        }
    };
    // Stats completeness: OpenAI upstreams only emit a final `usage` chunk when
    // `stream_options.include_usage` is set. Force it on streaming requests so
    // MetricsStream can record real token counts; the extra SSE event is
    // standard and harmless to clients (they either use it or ignore it).
    if provider.protocol() == Protocol::OpenAi
        && b.get("stream").and_then(|s| s.as_bool()).unwrap_or(false)
    {
        b["stream_options"]["include_usage"] = serde_json::json!(true);
    }
    b
}

/// Translate + dispatch + retry-loop shared by both inbound endpoints. Drives
/// the strategy layer (spec §3-§6): candidates are built once, then each
/// attempt `select_strategy` re-walks the (cooling/invalid-updated) candidate
/// set and returns either a routed row or a §6 terminal (AllCooling / NoValid /
/// Empty). `finish` renders the final success response in the client's protocol.
async fn forward<F, Fut>(
    app: Arc<App>,
    inbound_body: serde_json::Value,
    inbound: Protocol,
    finish: F,
) -> Response
where
    F: FnOnce(Arc<App>, reqwest::Response, String, String, String, bool) -> Fut,
    Fut: std::future::Future<Output = Response>,
{
    use crate::strategy::{build_candidates, SelectResult};

    let client_model = inbound_body
        .get("model")
        .and_then(|m| m.as_str())
        .unwrap_or("")
        .to_string();

    let cfg = app.read_config();
    let set = match build_candidates(&cfg, &client_model) {
        Ok(s) => s,
        Err(reason) => {
            // Unresolvable model = no key touched it; still count it as a failed request.
            app.record_stat(false, None, "unknown", &client_model);
            return error_response_for(StatusCode::NOT_FOUND, reason, inbound);
        }
    };
    if set.rows.is_empty() {
        // Resolved provider has no keys (both-ON), or the filter shrank the table
        // to nothing. Preserve the old 503 "no keys configured" semantics.
        let pid = cfg
            .resolve_model(&client_model)
            .ok()
            .map(|(p, _)| p)
            .unwrap_or_default();
        let pname = cfg
            .provider_by_id(&pid)
            .map(|p| p.name.clone())
            .unwrap_or_else(|| pid.clone());
        app.record_stat(false, None, &pid, &client_model);
        return error_response_for(
            StatusCode::SERVICE_UNAVAILABLE,
            format!("provider `{pname}` has no API keys configured"),
            inbound,
        );
    }

    // Stream flag is protocol-stable through translation; read it once.
    let is_stream = inbound_body
        .get("stream")
        .and_then(|s| s.as_bool())
        .unwrap_or(false);

    // Retry budget note: every key that fails is marked cooling/invalid, so
    // select_strategy never hands the same key back until it recovers. There is
    // no attempt counter — between holds the ready set strictly shrinks, and
    // each hold strictly spends the hold budget, so the loop always terminates.
    // Zero-failure mode (PASS §38): when every valid key is cooling, HOLD the
    // request until the soonest key recovers instead of 429ing the client —
    // the client sees latency, not failure. Total hold bounded by
    // max_hold_secs; beyond it we finally 429 with Retry-After.
    let max_hold_ms = cfg.max_hold_secs.saturating_mul(1000);
    // Sort stack snapshot once (editing strategy mid-request is an edge case).
    let sorts = cfg.strategy.sort.clone();
    drop(cfg); // re-read per attempt for the live provider (protocol may change)

    // Per-model stats key: the RESOLVED upstream model id when the candidate set
    // has one, so failures land in the same bucket as successes (finish() keys
    // them by upstream_model). Only unresolvable models fall back to the raw
    // client string.
    let stats_model = set
        .rows
        .first()
        .map(|r| r.upstream_model.clone())
        .unwrap_or_else(|| client_model.clone());

    // No attempt counter: every failed attempt marks its key cooling/invalid,
    // so the ready set strictly shrinks between holds, and each hold strictly
    // spends the hold budget — the loop always terminates (PASS §38). The
    // legacy max_attempts budget is superseded by the hold budget.
    let mut held_ms = 0u64;
    loop {
        let row = match app.select_strategy(&set, &sorts) {
            SelectResult::Routed(r) => r,
            SelectResult::AllCooling { retry_after } => {
                match retry_after {
                    // A valid key recovers soon and the hold budget allows it:
                    // wait for it and re-walk the table (does NOT consume an
                    // attempt — no key was tried).
                    Some(secs) if held_ms.saturating_add(secs * 1000) <= max_hold_ms => {
                        let wait_ms = secs.saturating_mul(1000).saturating_add(500);
                        tokio::time::sleep(Duration::from_millis(wait_ms)).await;
                        held_ms += wait_ms;
                        continue;
                    }
                    _ => {
                        // No recovery in sight (or hold budget spent) → 429 + Retry-After.
                        app.record_stat(false, None, "unknown", &stats_model);
                        return error_response_with_retry(
                            StatusCode::TOO_MANY_REQUESTS,
                            "all candidate keys are rate-limited (cooling down)".into(),
                            inbound,
                            retry_after,
                        );
                    }
                }
            }
            SelectResult::NoValid => {
                // §6: no valid row → 429 with no Retry-After (invalid keys
                // don't recover by waiting — holding would never help).
                app.record_stat(false, None, "unknown", &stats_model);
                return error_response_with_retry(
                    StatusCode::TOO_MANY_REQUESTS,
                    "all candidate keys are invalid (auth-failed)".into(),
                    inbound,
                    None,
                );
            }
            SelectResult::Empty => {
                // Shouldn't happen (no-keys checked before the loop), but handle.
                app.record_stat(false, None, "unknown", &stats_model);
                return error_response_with_retry(
                    StatusCode::TOO_MANY_REQUESTS,
                    "no candidate keys available".into(),
                    inbound,
                    None,
                );
            }
        };

        // Re-read config per attempt: the provider may have been edited
        // (protocol/base_url) since the candidate set was built.
        let cfg = app.read_config();
        let provider = match cfg.provider_by_id(&row.provider_id) {
            Some(p) => p.clone(),
            None => {
                // Provider vanished mid-request (edited out): fail fast.
                app.record_stat(false, Some(&row.api_key_id), &row.provider_id, &row.upstream_model);
                return error_response_for(
                    StatusCode::SERVICE_UNAVAILABLE,
                    "provider disappeared".into(),
                    inbound,
                );
            }
        };
        let upstream_proto = provider.protocol();
        // Rebuild the payload per attempt: the selected candidate may carry a
        // different upstream_model (cross-provider) or provider protocol.
        let upstream_payload =
            build_upstream_payload(&inbound_body, inbound, &provider, &row.upstream_model);
        let url = upstream_chat_url(&provider.base_url, upstream_proto);

        let resp = call_upstream(&app, &provider, &row.key_secret, &url, &upstream_payload).await;

        let resp = match resp {
            Ok(r) => r,
            Err(e) => {
                // Timeout / connection failure: rotate to the next key. The key gets a
                // short cooldown (not a rate-limit cooldown) so it recovers quickly.
                // Per-key tally: this key failed and is being rotated out.
                app.record_key_stat(false, &row.api_key_id);
                app.record_rotation(&row.provider_id);
                app.mark_cooldown(&row.provider_id, &row.api_key_id, Some(5), &format!("network error: {e}"));
                continue;
            }
        };

        let status = resp.status();
        if status.is_success() {
            app.clear_error(&row.api_key_id);
            // The `client_model` param of finish_generic is the per-model stats key —
            // pass the upstream model id (= the managed model id the UI's model card
            // shows) so per-model request/token/tftt buckets key on mo.id, not the
            // raw client string (which may be "provider/model" or an alias).
            return finish(app, resp, row.provider_id, row.api_key_id, row.upstream_model.clone(), is_stream).await;
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
            app.record_key_stat(false, &row.api_key_id);
            app.record_rotation(&row.provider_id);
            if code == 429 {
                // Quota-exhausted (not a rate window): the key is dead until the
                // user tops up / the plan resets — quarantine it like an auth
                // failure instead of cooling it for a minute and retrying forever.
                let lower = snippet.to_lowercase();
                let quota_dead = ["entitlement exhausted", "quota exhausted", "insufficient_quota",
                    "exceeded your current quota", "insufficient balance", "余额不足"]
                    .iter().any(|m| lower.contains(m));
                if quota_dead {
                    app.mark_invalid(&row.provider_id, &row.api_key_id, 30 * 60, &reason);
                    continue;
                }
                // Rate limited: cooldown = Retry-After → per-key override →
                // window-drain estimate → global default (PASS §38; the active
                // prober was removed — it burned the limited keys' own RPM to
                // measure them, and 1-token probes never matched real payloads).
                app.mark_rate_limited(&row.provider_id, &row.api_key_id, retry_after, &reason);
                // Short politeness gap before trying the next key. The old 1500ms
                // backoff dated from the single-provider era (guard against shared
                // account-level TPM cascades); observed pools are per-key limited,
                // where the next key has independent quota and the long sleep was
                // pure client-visible latency (~2s per rotation).
                tokio::time::sleep(Duration::from_millis(250)).await;
            } else if code == 401 || code == 403 {
                // Auth failure = the key itself is dead (invalid/revoked/out of quota
                // tier). A 5s cooldown just lets round-robin feed it again forever;
                // quarantine it for 30 minutes and surface it as 无效 in the UI.
                app.mark_invalid(&row.provider_id, &row.api_key_id, 30 * 60, &reason);
            } else {
                // Other key-scoped errors (408/5xx): short rotate-out cooldown, long
                // enough to not retry this round, short enough to recover quickly.
                app.mark_cooldown(&row.provider_id, &row.api_key_id, Some(5), &reason);
            }
            // Loop continues: select_strategy skips cooling/invalid keys and serves a fresh one.
            continue;
        }

        // Request-scoped client error: same result on every key — don't burn the pool.
        // F1: not attributed to the key either (the key served fine; the request was
        // bad) — attributing it would dent the key's success_rate / K availability
        // and its per-key fail tally for no fault of its own.
        app.record_stat(false, None, &row.provider_id, &row.upstream_model);
        return error_response_for(
            StatusCode::from_u16(status.as_u16()).unwrap_or(StatusCode::BAD_GATEWAY),
            format!("upstream error: {snippet}"),
            inbound,
        );
    }
}

// ---------------------------------------------------------------------------
// Success rendering — OpenAI client
// ---------------------------------------------------------------------------

fn finish_openai(
    app: Arc<App>,
    resp: reqwest::Response,
    provider_id: String,
    key_id: String,
    client_model: String,
    is_stream: bool,
) -> impl std::future::Future<Output = Response> {
    finish_generic(
        app,
        resp,
        provider_id,
        key_id,
        client_model,
        is_stream,
        /*client_is_anthropic*/ false,
    )
}

// ---------------------------------------------------------------------------
// Success rendering — Anthropic client
// ---------------------------------------------------------------------------

fn finish_anthropic(
    app: Arc<App>,
    resp: reqwest::Response,
    provider_id: String,
    key_id: String,
    client_model: String,
    is_stream: bool,
) -> impl std::future::Future<Output = Response> {
    finish_generic(
        app,
        resp,
        provider_id,
        key_id,
        client_model,
        is_stream,
        /*client_is_anthropic*/ true,
    )
}

/// Shared success path: forward headers, then either buffer (non-streaming,
/// translating if protocols differ) or stream (translating SSE chunk-by-chunk if
/// protocols differ). Stats and mid-stream interruption handling live here.
fn finish_generic(
    app: Arc<App>,
    resp: reqwest::Response,
    provider_id: String,
    key_id: String,
    client_model: String,
    client_wants_stream: bool,
    client_is_anthropic: bool,
) -> impl std::future::Future<Output = Response> {
    async move {
        // Headers first — the client must receive them regardless of how the body ends.
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
        if !client_wants_stream {
            match resp.bytes().await {
                Ok(bytes) => {
                    let parsed: Result<serde_json::Value, _> = serde_json::from_slice(&bytes);
                    let ok = parsed.as_ref().map(|v| v.get("error").is_none()).unwrap_or(true);
                    app.record_stat(ok, Some(&key_id), &provider_id, &client_model);
                    if !ok {
                        // Some providers return HTTP 200 with an auth-failure error body
                        // instead of a proper 401 status. Detect and quarantine the key.
                        let err_text = parsed.as_ref().ok().and_then(|v| {
                            let e = v.get("error")?;
                            Some(serde_json::to_string(e).unwrap_or_default().to_lowercase())
                        }).unwrap_or_default();
                        if err_text.contains("invalid_api_key") || err_text.contains("invalid api key")
                            || err_text.contains("authentication") || err_text.contains("unauthorized")
                            || err_text.contains("\"401\"") || err_text.contains("api key not")
                            || err_text.contains("permission_denied")
                        {
                            app.mark_invalid(&provider_id, &key_id, 30 * 60,
                                &format!("upstream 200 with auth error: {}", err_text.chars().take(200).collect::<String>()));
                        }
                        return error_response_for(
                            StatusCode::BAD_GATEWAY,
                            "upstream returned an error body".into(),
                            if client_is_anthropic { Protocol::Anthropic } else { Protocol::OpenAi },
                        );
                    }
                    let inbound = if client_is_anthropic { Protocol::Anthropic } else { Protocol::OpenAi };
                    let body = match parsed {
                        Ok(v) => v,
                        Err(_) => return error_response_for(
                            StatusCode::BAD_GATEWAY,
                            "upstream returned non-JSON body".into(),
                            inbound,
                        ),
                    };
                    // Strategy §2.2 `tpm` source: count tokens from the upstream's
                    // reported `usage` (actual), falling back to a rough body estimate
                    // when absent (decision 鈶燗). Non-stream contributes to tpm only;
                    // tftt/tps are streaming-only (0 here, wired next chunk).
                    {
                        let up = upstream_proto_of(&app, &provider_id);
                        let total = match protocol::extract_usage_tokens(&body, up) {
                            Some((i, o)) => i + o,
                            None => protocol::estimate_response_tokens(&body),
                        };
                        app.record_token_count(&key_id, total);
                        app.record_tokens(total, Some(&key_id), &provider_id, &client_model);
                    }
                    // Translate only when the upstream protocol differs from the client's.
                    // The upstream response is in the *upstream's* format; the client
                    // needs it in *its* format.
                    let out = match (inbound, upstream_proto_of(&app, &provider_id)) {
                        (Protocol::Anthropic, Protocol::OpenAi) => {
                            // OpenAI-shaped upstream response -> Anthropic client.
                            let model = body.get("model").and_then(|m| m.as_str()).unwrap_or("gateway").to_string();
                            protocol::openai_to_anthropic_response(&body, &model)
                        }
                        (Protocol::OpenAi, Protocol::Anthropic) => {
                            // Anthropic-shaped upstream response -> OpenAI client.
                            let model = body.get("model").and_then(|m| m.as_str()).unwrap_or("gateway").to_string();
                            protocol::anthropic_to_openai_response(&body, &model)
                        }
                        _ => body,
                    };
                    let mut response = Response::new(Body::from(serde_json::to_vec(&out).unwrap_or_default()));
                    headers.insert(
                        axum::http::header::CONTENT_TYPE,
                        axum::http::HeaderValue::from_static("application/json"),
                    );
                    *response.headers_mut() = headers;
                    *response.status_mut() = StatusCode::OK;
                    return response;
                }
                Err(e) => {
                    // Connection dropped before the client saw anything: counted as a
                    // failed request.
                    app.record_stat(false, Some(&key_id), &provider_id, &client_model);
                    return error_response_for(
                        StatusCode::BAD_GATEWAY,
                        format!("upstream connection failed mid-body: {e}"),
                        if client_is_anthropic { Protocol::Anthropic } else { Protocol::OpenAi },
                    );
                }
            }
        }

        // --- streaming: forward chunks as they arrive. Success is counted up-front
        // (the key served the request); a mid-stream interruption after bytes were
        // sent to the client cannot be retried, so we append an error event in the
        // client's protocol and end the stream cleanly.
        app.record_stat(true, Some(&key_id), &provider_id, &client_model);

        let up_proto = upstream_proto_of(&app, &provider_id);
        let needs_translation = client_is_anthropic != (up_proto == Protocol::Anthropic);
        if !needs_translation {
            // Same protocol both sides: raw byte passthrough (the old fast path),
            // wrapped in MetricsStream so tftt / usage / tps are still measured.
            let upstream = MetricsStream::new(resp.bytes_stream(), app.clone(), key_id.clone(), provider_id.clone(), client_model.clone(), up_proto);
            let stream = upstream.map(move |r| match r {
                Ok(chunk) => Ok::<_, std::io::Error>(chunk),
                Err(e) => Ok(openai_stream_error_chunk(&e).into_bytes().into()),
            });
            let mut response = Response::new(Body::from_stream(stream));
            *response.headers_mut() = headers;
            *response.status_mut() = StatusCode::OK;
            return response;
        }

        if client_is_anthropic {
            // OpenAI upstream -> Anthropic client: synthesize Anthropic SSE events.
            let model = app.read_config()
                .provider_by_id(&provider_id)
                .map(|_| "gateway".to_string())
                .unwrap_or_else(|| "gateway".to_string());
            let mut state = OpenAiToAnthropicStream::new(model);
            let upstream = MetricsStream::new(resp.bytes_stream(), app.clone(), key_id.clone(), provider_id.clone(), client_model.clone(), up_proto);
            let stream = upstream
                .map(move |r| -> Result<axum::body::Bytes, std::io::Error> {
                    match r {
                        Ok(chunk) => Ok(translate_openai_chunk(&mut state, &chunk).into_bytes().into()),
                        Err(e) => Ok(openai_stream_error_chunk(&e).into_bytes().into()),
                    }
                });
            let mut response = Response::new(Body::from_stream(stream));
            headers.insert(
                axum::http::header::CONTENT_TYPE,
                axum::http::HeaderValue::from_static("text/event-stream"),
            );
            *response.headers_mut() = headers;
            *response.status_mut() = StatusCode::OK;
            return response;
        }

        // Anthropic upstream -> OpenAI client: parse Anthropic SSE events, emit chunks.
        let mut state = AnthropicToOpenAiStream::new();
        let mut buffer: Vec<u8> = Vec::new();
        let upstream = MetricsStream::new(resp.bytes_stream(), app.clone(), key_id.clone(), provider_id.clone(), client_model.clone(), up_proto);
        let stream = upstream.map(move |r| -> Result<axum::body::Bytes, std::io::Error> {
            match r {
                Ok(chunk) => Ok(translate_anthropic_chunk(&mut state, &mut buffer, &chunk).into_bytes().into()),
                Err(e) => Ok(openai_stream_error_chunk(&e).into_bytes().into()),
            }
        });
        headers.insert(
            axum::http::header::CONTENT_TYPE,
            axum::http::HeaderValue::from_static("text/event-stream"),
        );
        let mut response = Response::new(Body::from_stream(stream));
        *response.headers_mut() = headers;
        *response.status_mut() = StatusCode::OK;
        response
    }
}

// ---------------------------------------------------------------------------
// Streaming metrics: a transparent wrapper over the upstream bytes stream.
// Records time-to-first-token (spec §2.2 `avg_tftt`, decision 鈶), scans for the
// upstream's `usage` chunk (decision 鈶燗), and at stream end records token count
// (`tpm`) + generation rate (`tps`). Wraps the raw upstream so the translation
// state machines need no changes.
// ---------------------------------------------------------------------------

/// Best-effort SSE usage scanner: returns (input, output) when a `data:` line in
/// this chunk carries a JSON object with `usage` (OpenAI final chunk with
// `stream_options.include_usage`, or Anthropic `message_delta`). An event split
/// across chunk boundaries may be missed (then tpm/tps undercount for that one
/// request; non-stream is always exact). Cheap `contains` pre-filter on every chunk.
fn scan_usage(chunk: &axum::body::Bytes, proto: Protocol) -> (Option<u64>, Option<u64>) {
    // Partial per event by design — the caller merges across chunks (Anthropic
    // splits input/output across message_start / message_delta).
    let Ok(text) = std::str::from_utf8(chunk) else {
        return (None, None);
    };
    if !text.contains("\"usage\"") {
        return (None, None); // pre-filter: most chunks carry no usage
    }
    for line in text.split_inclusive('\n') {
        let payload = match line.trim().strip_prefix("data:") {
            Some(p) => p.trim(),
            None => continue,
        };
        if payload == "[DONE]" || payload.is_empty() {
            continue;
        }
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(payload) {
            let u = protocol::extract_usage_partial(&v, proto);
            if u.0.is_some() || u.1.is_some() {
                return u;
            }
        }
    }
    (None, None)
}

/// Best-effort SSE first-token scanner: true when a `data:` line in this chunk
/// carries the first piece of *generated content* — OpenAI `choices[0].delta.content`
/// (non-empty) or Anthropic `delta.text` (content_block_delta). Distinguishes the
/// "棣栧瓧寤惰繜"/TFTT metric from the role/usage/stop chunks (which arrive immediately
/// or at the end and carry no text). Same split-across-chunks caveat as `scan_usage`.
fn scan_has_content(chunk: &axum::body::Bytes, proto: Protocol) -> bool {
    let text = match std::str::from_utf8(chunk) {
        Ok(s) => s,
        Err(_) => return false,
    };
    if !text.contains("content") && !text.contains("\"text\"") {
        return false; // pre-filter: role/usage/stop chunks carry neither
    }
    for line in text.split_inclusive('\n') {
        let payload = match line.trim().strip_prefix("data:") {
            Some(p) => p.trim(),
            None => continue,
        };
        if payload == "[DONE]" || payload.is_empty() {
            continue;
        }
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(payload) {
            let has = match proto {
                Protocol::OpenAi => v
                    .pointer("/choices/0/delta/content")
                    .and_then(|c| c.as_str())
                    .map_or(false, |s| !s.is_empty()),
                Protocol::Anthropic => v
                    .pointer("/delta/text")
                    .and_then(|c| c.as_str())
                    .map_or(false, |s| !s.is_empty()),
            };
            if has {
                return true;
            }
        }
    }
    false
}

/// Transparent wrapper over an upstream `bytes_stream()`: yields each item
/// unchanged while recording tftt (first chunk), scanning for usage, and on
/// stream end recording token count + tps. The downstream `.map()` (translation
/// or passthrough error-map) runs unchanged on the wrapper's output.
struct MetricsStream<S> {
    inner: S,
    app: std::sync::Arc<App>,
    key_id: String,
    provider_id: String,
    model: String,
    start: std::time::Instant,
    first: bool,
    /// Merged partial usage: Anthropic splits input (message_start) and output
    /// (message_delta) across events; OpenAI's include_usage chunk has both.
    usage_in: Option<u64>,
    usage_out: Option<u64>,
    proto: Protocol,
}

impl<S> MetricsStream<S>
where
    S: futures_util::Stream<Item = Result<axum::body::Bytes, reqwest::Error>> + Unpin,
{
    fn new(inner: S, app: std::sync::Arc<App>, key_id: String, provider_id: String, model: String, proto: Protocol) -> Self {
        Self {
            inner,
            app,
            key_id,
            provider_id,
            model,
            start: std::time::Instant::now(),
            first: true,
            usage_in: None,
            usage_out: None,
            proto,
        }
    }
}

impl<S> futures_util::Stream for MetricsStream<S>
where
    S: futures_util::Stream<Item = Result<axum::body::Bytes, reqwest::Error>> + Unpin + Send,
    Self: Unpin,
{
    type Item = Result<axum::body::Bytes, reqwest::Error>;

    fn poll_next(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        use std::task::Poll;
        let this = self.get_mut(); // safe: Self: Unpin
        match this.inner.poll_next_unpin(cx) {
            Poll::Ready(Some(Ok(chunk))) => {
                // TFTT = time to the first *generated content* token (棣栧瓧寤惰繜),
                // not the first chunk — OpenAI's role chunk and Anthropic's
                // message_start arrive immediately and would read ~0. So wait for
                // the first chunk whose data: line carries actual text.
                if this.first && scan_has_content(&chunk, this.proto) {
                    this.first = false;
                    let ms = this.start.elapsed().as_millis() as u32;
                    this.app.record_tftt(&this.key_id, ms);
                    this.app.record_model_tftt(&this.model, ms);
                }
                let (ui, uo) = scan_usage(&chunk, this.proto);
                if ui.is_some() {
                    this.usage_in = ui;
                }
                if uo.is_some() {
                    this.usage_out = uo;
                }
                Poll::Ready(Some(Ok(chunk)))
            }
            Poll::Ready(Some(Err(e))) => Poll::Ready(Some(Err(e))),
            Poll::Ready(None) => {
                // Stream ended: record token count (tpm) + generation rate (tps).
                if this.usage_in.is_some() || this.usage_out.is_some() {
                    let i = this.usage_in.take().unwrap_or(0);
                    let o = this.usage_out.take().unwrap_or(0);
                    let total = i + o;
                    this.app.record_token_count(&this.key_id, total);
                    this.app.record_tokens(total, Some(&this.key_id), &this.provider_id, &this.model);
                    let dur = this.start.elapsed().as_secs_f32();
                    if dur > 0.0 && o > 0 {
                        let tps = o as f32 / dur;
                        this.app.record_tps(&this.key_id, tps);
                        this.app.record_model_tps(&this.model, tps);
                    }
                }
                Poll::Ready(None)
            }
            Poll::Pending => Poll::Pending,
        }
    }
}

/// The upstream protocol as recorded in the current config (read live because the
/// user may have edited the provider while a request is in flight).
fn upstream_proto_of(app: &App, provider_id: &str) -> Protocol {
    app.read_config()
        .provider_by_id(provider_id)
        .map(|p| p.protocol())
        .unwrap_or(Protocol::OpenAi)
}

/// Mid-stream failure chunk for OpenAI clients.
fn openai_stream_error_chunk(e: &reqwest::Error) -> String {
    format!(
        "data: {}\n\ndata: [DONE]\n\n",
        serde_json::json!({
            "error": {
                "message": format!("stream interrupted before finish_reason: {e}"),
                "type": "gateway_error",
                "code": 502,
            }
        })
    )
}

/// Feed one raw OpenAI SSE byte chunk through the translator, returning the
/// Anthropic SSE bytes to forward. Buffers partially-received lines itself.
fn translate_openai_chunk(state: &mut OpenAiToAnthropicStream, chunk: &[u8]) -> String {
    let mut out = String::new();
    let text = String::from_utf8_lossy(chunk);
    for line in text.split_inclusive('\n') {
        let line = line.trim();
        let payload = match line.strip_prefix("data:") {
            Some(p) => p.trim(),
            None => continue,
        };
        if payload == "[DONE]" {
            for e in state.feed(None) {
                out.push_str(&e);
            }
            continue;
        }
        if payload.is_empty() {
            continue;
        }
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(payload) {
            for e in state.feed(Some(&v)) {
                out.push_str(&e);
            }
        }
    }
    out
}

/// Feed one raw Anthropic SSE byte chunk through the translator, returning the
/// OpenAI SSE bytes to forward. SSE events can span chunk boundaries, so a
/// carry-over buffer holds partial events until their terminating blank line.
fn translate_anthropic_chunk(
    state: &mut AnthropicToOpenAiStream,
    buffer: &mut Vec<u8>,
    chunk: &[u8],
) -> String {
    buffer.extend_from_slice(chunk);
    let mut out = String::new();
    // SSE events are separated by a blank line; process every complete one.
    while let Some(pos) = find_event_end(buffer) {
        let raw: Vec<u8> = buffer.drain(..pos).collect();
        let text = String::from_utf8_lossy(&raw);
        let mut event_name = String::from("message");
        let mut data = String::new();
        for line in text.lines() {
            if let Some(name) = line.strip_prefix("event: ") {
                event_name = name.trim().to_string();
            } else if let Some(d) = line.strip_prefix("data: ") {
                data.push_str(d);
            }
        }
        if data.is_empty() {
            continue;
        }
        match serde_json::from_str::<serde_json::Value>(&data) {
            Ok(v) => {
                for c in state.feed(&event_name, &v) {
                    out.push_str(&c);
                }
            }
            Err(_) => {} // keep-alive comments / partial JSON: skip
        }
    }
    out
}

/// Index just past the double-newline terminating a complete SSE event.
fn find_event_end(buf: &[u8]) -> Option<usize> {
    buf.windows(4).position(|w| w == b"\r\n\r\n").map(|p| p + 4)
        .or_else(|| buf.windows(2).position(|w| w == b"\n\n").map(|p| p + 2))
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
                "supports_images": m.supports_images,
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

