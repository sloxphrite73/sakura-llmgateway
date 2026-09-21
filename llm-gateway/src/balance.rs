//! Tier-A bill-balance fetcher (strategy spec §2.2 `bill_balance`; decisions
//! ③A background polling + ④A USD normalization).
//!
//! Polls providers that expose a balance API with the *same key as inference*
//! (Tier A: Moonshot / SiliconFlow / Novita / Requesty / OpenRouter), normalizes
//! to USD via `Config::currency_rates`, caches via `App::set_bill_balance`.
//! Providers without a balance API stay +inf (None). Tier B (Fireworks /
//! Volcengine / Alibaba — need cloud AK/SK, not the inference key) and everything
//! else are skipped. See doc/research/provider-balance-apis.md for endpoints.

use crate::state::App;
use std::time::Duration;

/// Per-fetch timeout (caps a dead/slow balance API so the poll loop can't hang).
const FETCH_TIMEOUT_SECS: u64 = 15;

/// Fetch one provider/key's bill balance via its Tier-A endpoint. Returns
/// (raw_value, currency) or None when the host isn't a known Tier-A provider,
/// the request failed, or the value was absent/null (OpenRouter null -> None/+inf).
async fn fetch_one(
    http: &reqwest::Client,
    base_url: &str,
    key: &str,
) -> Option<(f64, &'static str)> {
    let host = host_of(base_url);
    let rel_url: String; // base + relative (same domain as inference)
    let req = match host.as_str() {
        h if h.contains("moonshot") => {
            rel_url = format!("{}/users/me/balance", base_url.trim_end_matches('/'));
            http.get(&rel_url).bearer_auth(key)
        }
        h if h.contains("siliconflow") => {
            rel_url = format!("{}/user/info", base_url.trim_end_matches('/'));
            http.get(&rel_url).bearer_auth(key)
        }
        h if h.contains("novita") => {
            // Novita's billing API lives under /openapi/v1, not the /openai
            // inference base — absolute URL.
            http.get("https://api.novita.ai/openapi/v1/billing/balance/detail")
                .bearer_auth(key)
        }
        h if h.contains("requesty") => {
            // Balance is on the api-v2 management domain, not the router one.
            http.get("https://api-v2.requesty.ai/v1/manage/org")
                .bearer_auth(key)
        }
        h if h.contains("openrouter") => {
            rel_url = format!("{}/key", base_url.trim_end_matches('/'));
            http.get(&rel_url).bearer_auth(key)
        }
        _ => return None, // no known Tier-A balance API for this host -> +inf
    };
    let resp = req
        .timeout(Duration::from_secs(FETCH_TIMEOUT_SECS))
        .send()
        .await
        .ok()?;
    if !resp.status().is_success() {
        return None;
    }
    let body: serde_json::Value = resp.json().await.ok()?;
    match host.as_str() {
        h if h.contains("moonshot") => parse_moonshot(&body).map(|v| (v, "CNY")),
        h if h.contains("siliconflow") => parse_siliconflow(&body).map(|v| (v, "CNY")),
        h if h.contains("novita") => parse_novita(&body).map(|v| (v, "USD")),
        h if h.contains("requesty") => parse_requesty(&body).map(|v| (v, "USD")),
        h if h.contains("openrouter") => parse_openrouter(&body).map(|v| (v, "USD")),
        _ => None,
    }
}

fn parse_moonshot(b: &serde_json::Value) -> Option<f64> {
    // data.available_balance (CNY). voucher_balance/cash_balance also present;
    // available_balance is the spendable total.
    b.pointer("/data/available_balance").and_then(|v| v.as_f64())
}
fn parse_siliconflow(b: &serde_json::Value) -> Option<f64> {
    b.pointer("/data/totalBalance").and_then(|v| v.as_f64())
}
fn parse_novita(b: &serde_json::Value) -> Option<f64> {
    // availableBalance is in 1/10000 USD -> divide (see provider-balance-apis.md).
    b.get("availableBalance")
        .and_then(|v| v.as_f64())
        .map(|v| v / 10000.0)
}
fn parse_requesty(b: &serde_json::Value) -> Option<f64> {
    b.get("balance").and_then(|v| v.as_f64())
}
fn parse_openrouter(b: &serde_json::Value) -> Option<f64> {
    // data.limit_remaining (USD credit); null when no per-key limit set -> None (+inf).
    b.pointer("/data/limit_remaining").and_then(|v| v.as_f64())
}

/// Normalize a raw balance to USD (decision ④A). USD passes through; other
/// currencies multiply by the configured rate; unknown currency -> None (+inf,
/// not comparable cross-currency). Configure e.g. `{"CNY": 0.14}` to enable
/// CNY providers (Moonshot / SiliconFlow) in the G/H sorts.
fn normalize(
    raw: f64,
    currency: &str,
    rates: &std::collections::BTreeMap<String, f64>,
) -> Option<f64> {
    if currency == "USD" {
        Some(raw)
    } else {
        rates.get(currency).map(|r| raw * r)
    }
}

fn host_of(base_url: &str) -> String {
    base_url
        .split("//")
        .nth(1)
        .and_then(|s| s.split('/').next())
        .unwrap_or("")
        .to_lowercase()
}

/// Poll every provider flagged `has_bill_balance_api` and cache USD-normalized
/// balances via `App::set_bill_balance`. Run from a background task: never on the
/// request hot path (decision ③A). Sequential over (provider, key) — bounded by
/// FETCH_TIMEOUT_SECS per fetch; a failed/null fetch just leaves the prior value
/// in place (the 60s cadence refreshes it). Poll interval is owned by the caller.
pub async fn poll_all(app: &App) {
    let cfg = app.read_config();
    let http = app.http.clone(); // cheap (Arc inside reqwest::Client)
    let rates = cfg.currency_rates.clone();
    for p in &cfg.providers {
        if !p.has_bill_balance_api {
            continue;
        }
        for k in &p.keys {
            let usd = match fetch_one(&http, &p.base_url, &k.key).await {
                Some((raw, cur)) => normalize(raw, cur, &rates),
                None => None, // +inf: no Tier-A endpoint / fetch failed / null
            };
            app.set_bill_balance(&k.id, usd);
        }
    }
}
