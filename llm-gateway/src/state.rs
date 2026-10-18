use crate::config::Config;
use crate::stats::Stats;
use std::collections::{HashMap, VecDeque};
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// Debounce window for learned-cooldown config writes (429 storms must not churn
/// gateway.json). The dirty flag is checked by a background flusher task.
pub(crate) const CONFIG_FLUSH_DEBOUNCE_MS: u64 = 10_000;

/// Rolling window for the `success_rate` strategy sort attribute (spec §2.2).
/// 5 min trades responsiveness for stability (a key isn't condemned by one bad minute).
pub(crate) const METRIC_SUCCESS_WINDOW_SECS: u64 = 300;
/// Rolling window for `rpm` (spec §2.2) — per-minute, matching "RPM" semantics.
pub(crate) const METRIC_RPM_WINDOW_SECS: u64 = 60;
/// Rolling window for `tpm` (spec §2.2) — tokens consumed in the last minute.
pub(crate) const METRIC_TPM_WINDOW_SECS: u64 = 60;
/// Rolling sample cap for `avg_tftt` / `tps` (last-N mean, spec §2.2).
pub(crate) const METRIC_SAMPLE_CAP: usize = 50;
/// Cap for per-key capacity samples (429-at-load observations). 20 samples is
/// enough for the min-estimate to track a slowly-changing limit.
pub(crate) const CAPACITY_SAMPLE_CAP: usize = 20;

/// Sort key K: prior strength for the smoothed success rate
/// `(ok + β) / (n + β)` — a prior of "1.0 with 2 phantom observations", so a
/// key with no history scores 1.0 (don't penalize new keys) and the estimate
/// converges to the measured rate as samples accrue.
pub(crate) const AVAILABILITY_PRIOR_BETA: f64 = 2.0;
/// Sort key K: floor for the capacity-headroom factor. A key estimated at/over
/// its RPM capacity is heavily deprioritized but never hard-excluded — the
/// estimate may be wrong, and true rate-limiting is the cooldown layer's job
/// (orthogonality, spec §7).
pub(crate) const AVAILABILITY_HEADROOM_FLOOR: f64 = 0.05;

/// Per-key rolling metrics consumed by the strategy layer (spec §2.2 attributes).
/// `tpm` / `avg_tftt` / `tps` / `*_balance` land in a later chunk (token counting +
/// stream timing + balance-API fetchers); zero/None until then.
#[allow(dead_code)] // forward-looking interface; consumed once the strategy layer lands
#[derive(Clone, Debug, Default)]
pub struct KeyMetricsSnapshot {
    pub success_rate: f64,            // [0,1]; 1.0 when no history (don't penalize new keys)
    pub rpm: u32,
    pub tpm: u32,                     // 0 until token counting
    pub avg_tftt_ms: u32,             // 0 until stream-first-token timing
    pub tps: f32,                     // 0.0 until token counting
    pub token_balance: Option<u64>,  // None = +∞ (OptInf, spec §2.2)
    pub bill_balance: Option<f64>,    // None = +∞
    /// Measured capacity from 429 samples (None = unknown) + current
    /// utilization (0..1+, max of rpm/tpm utilization; None = no capacity data).
    pub capacity_rpm: Option<u32>,
    pub capacity_tpm: Option<u32>,
    pub utilization: Option<f64>,
}

#[derive(Debug, Default)]
struct KeyMetrics {
    /// (time, outcome) — pruned to the success window on each push.
    success: VecDeque<(Instant, bool)>,
    /// request timestamps — pruned to the RPM window; `len()` at read = rpm.
    req_times: VecDeque<Instant>,
    /// (time, tokens) — pruned to the TPM window; sum of tokens at read = tpm.
    token_events: VecDeque<(Instant, u64)>,
    /// time-to-first-token samples (ms), last `METRIC_SAMPLE_CAP`; mean = avg_tftt.
    tftt_samples: VecDeque<u32>,
    /// tokens/sec samples, last `METRIC_SAMPLE_CAP`; mean = tps.
    tps_samples: VecDeque<f32>,
    /// USD-normalized bill balance (set by the balance fetcher; None = +∞).
    bill_balance: Option<f64>,
    /// (rpm, tpm) usage sampled at each 429 — each sample is an observed
    /// lower bound of this key's capacity (the limit was hit at that load).
    /// Capacity estimate = min over recent samples per metric (conservative:
    /// underestimating capacity just makes the gateway avoid the key earlier).
    /// Turns "every 429" into a free capacity probe (PASS §36).
    capacity_samples: VecDeque<(u32, u32)>,
}

#[derive(Debug, Default)]
pub(crate) struct PoolRuntime {
    /// key id -> cooldown expiry
    cooling: HashMap<String, Instant>,
    /// key id -> "invalid" (auth-failed) cooldown expiry. Auth failures (401/403) mean
    /// the key itself is dead — retrying in seconds never succeeds, so these get a long
    /// quarantine that round-robin skips, instead of the 5s rotate-out cooldown.
    invalid: HashMap<String, Instant>,
    /// key id -> last error string (for the UI)
    last_error: HashMap<String, String>,
    /// key id -> total requests routed through this key
    requests: HashMap<String, u64>,
    /// Per-key rolling metrics (success_rate / rpm now; tpm/tps/tftt/balance later).
    metrics: HashMap<String, KeyMetrics>,
    /// Per-model time-to-first-token samples (ms), last `METRIC_SAMPLE_CAP`; the
    /// model page's avg-TFTT card reads the mean. Keyed by the upstream model id
    /// (= the managed model id the UI shows). Streaming only (non-stream has no
    /// "first token"); 0 until the first streamed chunk of a request for that model.
    model_tftt: HashMap<String, VecDeque<u32>>,
    /// Per-model tokens/sec samples (streaming generation rate), last
    /// `METRIC_SAMPLE_CAP`; the model page's avg-TPS reads the mean. Keyed by the
    /// upstream model id (same key as `model_tftt`). Streaming only.
    model_tps: HashMap<String, VecDeque<f32>>,
    /// Strategy-layer D1 cursor map (impl-spec §1, §4.1). Keyed by the
    /// candidate-set identifier (D3: `"{group_id}\x1f{provider_or_any}"`); the
    /// value is the round-robin cursor. Replaces the old per-provider
    /// `counters` map (the both-ON special case is now cursor-keyed by
    /// `(group_id, provider_id)`).
    strategy_cursors: HashMap<String, usize>,
}

pub struct App {
    pub config: std::sync::RwLock<Config>,
    pub pool: Mutex<PoolRuntime>,
    pub http: reqwest::Client,
    pub config_path: std::path::PathBuf,
    /// Request statistics, persisted into gateway.json (merged with the config under
    /// the `stats` field) on the periodic flush + on every config change.
    pub stats: Mutex<Stats>,
    /// Legacy standalone stats.json path (next to the config file). Used only for a
    /// one-time migration into gateway.json on startup, then deleted.
    pub stats_path: std::path::PathBuf,
    /// Set when stats were migrated from a legacy standalone stats.json into
    /// gateway.json on startup. `save_all` deletes stats.json after the first
    /// successful merged write (so the stats land in gateway.json first).
    stats_migrated: std::sync::atomic::AtomicBool,
    /// Set when learned-cooldown values changed and gateway.json needs a debounced flush.
    config_dirty: Mutex<bool>,
    /// Free-provider catalog: session cache for a remote refresh (embedded catalog
    /// is compile-time and needs no state).
    pub catalog: crate::free_catalog::CatalogCache,
    /// Actual bound ports, set after startup binding. May differ from the
    /// configured `api_port`/`ui_port` when the configured port was occupied and
    /// the fallback walked to a free one. 0 = not bound yet (callers fall back to
    /// the config value).
    pub bound_api_port: std::sync::atomic::AtomicU16,
    pub bound_ui_port: std::sync::atomic::AtomicU16,
}

pub fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Ceil a duration to whole seconds for client-facing Retry-After semantics:
/// truncating (as_secs) would report "0s left" for a cooldown that still has
/// a few hundred ms to run, telling clients to retry immediately (F11).
fn ceil_secs(d: Duration) -> u64 {
    (d.as_millis() as u64 + 999) / 1000
}

/// Fill candidate rows' runtime fields — cooldown-derived state (`valid` /
/// `non_cooled` / `remaining_secs`, spec §2.3) + the metrics snapshot
/// (`success_rate`/`rpm`/`tpm`/`avg_tftt`/`tps`/`token_balance`/`bill_balance`,
/// spec §2.2) — from the pool. Prunes expired cooldowns first. Called under the
/// caller's held pool lock by `select_strategy` (hot path) and `strategy_dry_run`
/// (preview), so the two never diverge on what "cooling" means.
fn fill_rows(
    pool: &mut PoolRuntime,
    rows: &mut [crate::strategy::Row],
    now: Instant,
) {
    pool.cooling.retain(|_, until| *until > now);
    pool.invalid.retain(|_, until| *until > now);
    for r in rows {
        r.valid = !pool.invalid.contains_key(&r.api_key_id);
        if let Some(until) = pool.cooling.get(&r.api_key_id) {
            r.non_cooled = false;
            r.remaining_secs = ceil_secs(until.saturating_duration_since(now));
        } else {
            r.non_cooled = true;
            r.remaining_secs = 0;
        }
        // Capacity for K's headroom comes ONLY from config (the skeleton set by
        // build_candidates: seed_rpm > provider limit split). The 429 samples
        // are kept for diagnostics but provably can't measure capacity (a 429
        // says rpm≥limit OR tpm≥limit — a disjunction that can't assign either;
        // samples pinned utilization at 100% in practice, PASS §38).
        // Metrics snapshot (spec §2.2): success_rate/rpm/tpm/avg_tftt/tps are
        // live; token_balance is always None (no catalog provider exposes a
        // clean remaining-token-quota field); bill_balance is set by the balance
        // fetcher (None = +∞).
        match pool.metrics.get_mut(&r.api_key_id) {
            None => {
                // No live history: keep the skeleton's seed values (spec §2.2
                // "手动 seed" — set in build_candidates) for the measured
                // metrics. Balances are already None (no provider-reported
                // data for an untouched key). Nothing to overwrite.
                // K: no live load → headroom 1.0, so availability collapses to
                // the seed success_rate (or 1.0 for a brand-new key).
                r.availability = r.success_rate.clamp(0.0, 1.0);
            }
            Some(m) => {
                // F5: prune the rolling windows on READ, not just on push —
                // otherwise a key that stopped being routed keeps stale samples
                // forever: rpm reads high (K's headroom sinks), success_rate is
                // a stale-window rate. (Writes still prune on push; this makes
                // the read path self-consistent for keys not recently pushed.)
                let rpm_win = Duration::from_secs(METRIC_RPM_WINDOW_SECS);
                let ok_win = Duration::from_secs(METRIC_SUCCESS_WINDOW_SECS);
                m.req_times.retain(|t| now.duration_since(*t) <= rpm_win);
                m.success.retain(|e| now.duration_since(e.0) <= ok_win);
                m.token_events.retain(|(t, _)| now.duration_since(*t) <= Duration::from_secs(METRIC_TPM_WINDOW_SECS));
                let total = m.success.len();
                r.success_rate = if total == 0 {
                    1.0
                } else {
                    let ok = m.success.iter().filter(|e| e.1).count() as f64;
                    ok / total as f64
                };
                r.rpm = m.req_times.len() as u32;
                r.tpm = m.token_events.iter().map(|(_, n)| *n).sum::<u64>() as u32;
                r.avg_tftt_ms = if m.tftt_samples.is_empty() {
                    0
                } else {
                    (m.tftt_samples.iter().map(|&v| v as u64).sum::<u64>()
                        / m.tftt_samples.len() as u64) as u32
                };
                r.tps = if m.tps_samples.is_empty() {
                    0.0
                } else {
                    m.tps_samples.iter().map(|&v| v).sum::<f32>() / m.tps_samples.len() as f32
                };
                r.token_balance = None;
                r.bill_balance = m.bill_balance;
                // K = smoothed success_rate × capacity headroom.
                // sr: (ok + β)/(n + β) — prior 1.0 with strength β, so few
                // samples stay near 1.0 and the estimate converges to the
                // measured rate (a 1/1 key no longer beats a 99/100 key).
                let ok = m.success.iter().filter(|e| e.1).count() as f64;
                let sr = (ok + AVAILABILITY_PRIOR_BETA) / (total as f64 + AVAILABILITY_PRIOR_BETA);
                // headroom from the WORST-utilized capacity dimension (whichever
                // of rpm/tpm is closer to its limit is what will 429 first).
                // Unknown capacity → that metric doesn't constrain. Both unknown
                // → 1.0 (neutral). Floor 0.05: deprioritize, never exclude.
                let mut util: Option<f64> = None;
                let mut acc = |used: f64, cap: Option<u32>| {
                    if let Some(c) = cap {
                        if c > 0 {
                            util = Some(util.unwrap_or(0.0).max(used / c as f64));
                        }
                    }
                };
                acc(m.req_times.len() as f64, r.capacity_rpm);
                acc(r.tpm as f64, r.capacity_tpm);
                let headroom = match util {
                    Some(u) => (1.0 - u).clamp(AVAILABILITY_HEADROOM_FLOOR, 1.0),
                    None => 1.0,
                };
                r.availability = sr * headroom;
            }
        }
        // spec §2.3 quota gate: valid = not-banned AND (when a balance is
        // measurable) at least one measurable balance > 0. See valid_with_quota.
        r.valid = valid_with_quota(r.valid, r.token_balance, r.bill_balance);
    }
}

/// spec §2.3 `valid` gate (pure, for unit testing). A key is valid iff it is
/// not-banned AND has available quota. A balance is "measurable" (the provider
/// reports it via `has_*_balance_api`) exactly when it is `Some` here; `None` =
/// unmeasurable = +∞ (the keyless case, where not-banned alone suffices). The
/// spec rule "token_balance>0 OR bill_balance>0" means: if any balance is
/// measurable, at least one must be positive — exhaust every measurable balance
/// and the key is not valid (skip it proactively instead of routing then 429'ing).
fn valid_with_quota(
    not_banned: bool,
    token_balance: Option<u64>,
    bill_balance: Option<f64>,
) -> bool {
    if !not_banned {
        return false;
    }
    let measurable = token_balance.is_some() || bill_balance.is_some();
    if !measurable {
        return true; // keyless / unmeasured → not-banned suffices
    }
    matches!(token_balance, Some(t) if t > 0) || matches!(bill_balance, Some(b) if b > 0.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keyless_unmeasured_is_valid_when_not_banned() {
        // sensenova-style: no balance API → both None → not-banned alone.
        assert!(valid_with_quota(true, None, None));
        assert!(!valid_with_quota(false, None, None));
    }

    #[test]
    fn exhausted_bill_balance_is_not_valid() {
        // has_bill_balance_api provider, balance measured at 0 → not valid.
        assert!(!valid_with_quota(true, None, Some(0.0)));
    }

    #[test]
    fn positive_bill_balance_is_valid() {
        assert!(valid_with_quota(true, None, Some(2.5)));
    }

    #[test]
    fn exhausted_token_balance_is_not_valid() {
        assert!(!valid_with_quota(true, Some(0), None));
    }

    #[test]
    fn one_positive_balance_rescues_the_other() {
        // spec "token_balance>0 OR bill_balance>0": either positive is enough.
        assert!(valid_with_quota(true, Some(0), Some(1.0)));
        assert!(valid_with_quota(true, Some(5), Some(0.0)));
    }

    #[test]
    fn both_balances_zero_is_not_valid() {
        assert!(!valid_with_quota(true, Some(0), Some(0.0)));
    }

    // --- sort key K: availability = smoothed success_rate × headroom ---

    /// Minimal strategy Row for fill_rows tests (only the fields fill_rows reads).
    fn k_row(key_id: &str, seed_sr: f64, capacity: Option<u32>) -> crate::strategy::Row {
        crate::strategy::Row {
            provider_id: "p".into(),
            upstream_model: "m".into(),
            api_key_id: key_id.into(),
            key_secret: String::new(),
            group_id: "m".into(),
            valid: true,
            non_cooled: true,
            remaining_secs: 0,
            success_rate: seed_sr,
            rpm: 0,
            tpm: 0,
            avg_tftt_ms: 0,
            tps: 0.0,
            token_balance: None,
            bill_balance: None,
            price: 0.0,
            idx: 0,
            capacity_rpm: capacity,
            capacity_tpm: None,
            availability: 1.0,
        }
    }

    #[test]
    fn availability_no_history_uses_seed_success_rate() {
        // No live metrics → availability = the skeleton's seed success_rate
        // (headroom 1.0: no load). A brand-new key (seed None → 1.0) scores 1.0.
        let mut pool = PoolRuntime::default();
        let mut rows = vec![k_row("seeded", 0.85, Some(10)), k_row("fresh", 1.0, None)];
        fill_rows(&mut pool, &mut rows, Instant::now());
        assert!((rows[0].availability - 0.85).abs() < 1e-9);
        assert!((rows[1].availability - 1.0).abs() < 1e-9);
    }

    #[test]
    fn availability_penalizes_near_capacity() {
        // 9 requests in the 60s window against a capacity of 10 → headroom 0.1
        // → availability collapses despite a perfect success history.
        let mut pool = PoolRuntime::default();
        let m = pool.metrics.entry("hot".into()).or_default();
        for _ in 0..9 {
            m.req_times.push_back(Instant::now());
            m.success.push_back((Instant::now(), true));
        }
        let mut rows = vec![k_row("hot", 1.0, Some(10))];
        fill_rows(&mut pool, &mut rows, Instant::now());
        // sr = (9+2)/(9+2) = 1.0; headroom = 1 - 9/10 = 0.1
        assert!((rows[0].availability - 0.1).abs() < 1e-9, "got {}", rows[0].availability);
    }

    #[test]
    fn availability_floors_at_005_not_zero() {
        // At/over capacity the key is deprioritized but never hard-excluded —
        // true rate-limiting belongs to the cooldown layer (spec §7 orthogonality).
        let mut pool = PoolRuntime::default();
        let m = pool.metrics.entry("maxed".into()).or_default();
        for _ in 0..15 {
            m.req_times.push_back(Instant::now());
        }
        let mut rows = vec![k_row("maxed", 1.0, Some(10))];
        fill_rows(&mut pool, &mut rows, Instant::now());
        // sr = (0+2)/(0+2) = 1.0 (no success samples); headroom clamped to 0.05
        assert!((rows[0].availability - AVAILABILITY_HEADROOM_FLOOR).abs() < 1e-9);
    }

    #[test]
    fn availability_unknown_capacity_is_neutral() {
        // No capacity estimate → headroom 1.0 → availability = smoothed sr.
        let mut pool = PoolRuntime::default();
        let m = pool.metrics.entry("plain".into()).or_default();
        m.req_times.push_back(Instant::now());
        m.success.push_back((Instant::now(), true));
        m.success.push_back((Instant::now(), false));
        let mut rows = vec![k_row("plain", 1.0, None)];
        fill_rows(&mut pool, &mut rows, Instant::now());
        // sr = (1+2)/(2+2) = 0.75; headroom 1.0
        assert!((rows[0].availability - 0.75).abs() < 1e-9);
    }

    #[test]
    fn smoothing_damps_small_sample_failures() {
        // The prior (mean 1.0, strength β=2) keeps all-success keys at 1.0
        // regardless of sample size (no evidence to distinguish them), and
        // damps small-sample failure rates: 1/2 (raw 0.5) reads 0.75, while a
        // proven 9/10 key reads ~0.917 and outranks it.
        let mut pool = PoolRuntime::default();
        {
            let m = pool.metrics.entry("coinflip".into()).or_default();
            m.success.push_back((Instant::now(), true));
            m.success.push_back((Instant::now(), false));
        }
        {
            let m = pool.metrics.entry("veteran".into()).or_default();
            for i in 0..10 {
                m.success.push_back((Instant::now(), i != 0)); // 9 ok / 10
            }
        }
        let mut rows = vec![k_row("coinflip", 1.0, None), k_row("veteran", 1.0, None)];
        fill_rows(&mut pool, &mut rows, Instant::now());
        assert!((rows[0].availability - 0.75).abs() < 1e-9, "coinflip: got {}", rows[0].availability);
        assert!((rows[1].availability - 11.0 / 12.0).abs() < 1e-9, "veteran: got {}", rows[1].availability);
        assert!(rows[1].availability > rows[0].availability);
    }

    // --- F5/F9/F11 + first_usable_key (F3) ---
    #[test]
    fn ceil_secs_rounds_up_sub_second() {
        assert_eq!(ceil_secs(Duration::from_millis(0)), 0);
        assert_eq!(ceil_secs(Duration::from_millis(1)), 1, "1ms left must not say 0");
        assert_eq!(ceil_secs(Duration::from_millis(1500)), 2);
        assert_eq!(ceil_secs(Duration::from_secs(10)), 10);
    }

    #[test]
    fn fill_rows_prunes_expired_windows_on_read() {
        // F5: samples older than the window must not count even if no push has
        // pruned them (the key hasn't been routed lately).
        let mut pool = PoolRuntime::default();
        let old = Instant::now() - Duration::from_secs(120); // beyond the 60s rpm window
        {
            let m = pool.metrics.entry("k".into()).or_default();
            m.req_times.push_back(old);
            m.req_times.push_back(old);
            m.success.push_back((old, false));
        }
        let mut rows = vec![k_row("k", 1.0, Some(4))];
        fill_rows(&mut pool, &mut rows, Instant::now());
        assert_eq!(rows[0].rpm, 0, "expired rpm samples pruned on read");
        // success window is 300s — the 120s-old failure still counts.
        assert!((rows[0].success_rate - 0.0).abs() < 1e-9, "300s window keeps 120s-old sample");
    }

    #[test]
    fn first_usable_key_skips_cooling_and_invalid() {
        let dir = std::env::temp_dir().join(format!("fuk-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut cfg = Config::default();
        let key = |id: &str| crate::config::ApiKey {
            id: id.into(), key: format!("sk-{id}"), display_name: String::new(),
            cooldown_secs: None, learned_cooldown: None,
            seed_rpm: None, seed_tpm: None, seed_success_rate: None,
            seed_avg_tftt_ms: None, seed_tps: None,
        };
        cfg.providers.push(crate::config::Provider {
            id: "p".into(), name: "P".into(), base_url: "http://p/v1".into(),
            keys: vec![key("a"), key("b")],
            models: vec![], model_allowlist_only: false,
            aliases: Default::default(), protocol: String::new(),
            has_token_balance_api: false, has_bill_balance_api: false,
            price_table: Default::default(), output_price_table: Default::default(),
            rpm_limit: None, tpm_limit: None,
        });
        let app = App::new(cfg, dir.join("gateway.json"));
        let p = app.read_config().providers[0].clone();
        // Both healthy → first key.
        assert_eq!(app.first_usable_key(&p).as_deref(), Some("sk-a"));
        // a cooling → skip to b.
        app.mark_cooldown("p", "a", Some(30), "test");
        assert_eq!(app.first_usable_key(&p).as_deref(), Some("sk-b"));
        // b invalid, a cooling → both benched → fall back to first key.
        app.mark_invalid("p", "b", 60, "test");
        assert_eq!(app.first_usable_key(&p).as_deref(), Some("sk-a"), "all benched → first key");
        let _ = std::fs::remove_dir_all(&dir);
    }

    // --- real-429 cooldown chain = v0.7.9 semantics (mark_rate_limited) ---

    /// Remaining cooldown seconds for a key (test helper).
    fn cooldown_remaining(app: &App, key_id: &str) -> u64 {
        let pool = app.pool.lock().unwrap();
        pool.cooling
            .get(key_id)
            .map(|u| u.saturating_duration_since(Instant::now()).as_secs())
            .unwrap_or(0)
    }

    fn app_with_provider(learned: Option<u64>, per_key: Option<u64>) -> (App, String) {
        let dir = std::env::temp_dir().join(format!("rl-test-{}-{:?}-{:?}", std::process::id(), learned, per_key));
        std::fs::create_dir_all(&dir).unwrap();
        let mut cfg = Config::default(); // default_cooldown 60
        cfg.providers.push(crate::config::Provider {
            id: "p".into(),
            name: "P".into(),
            base_url: "http://p/v1".into(),
            keys: vec![crate::config::ApiKey {
                id: "k".into(),
                key: "sk-x".into(),
                display_name: String::new(),
                cooldown_secs: per_key,
                learned_cooldown: learned,
                seed_rpm: None,
                seed_tpm: None,
                seed_success_rate: None,
                seed_avg_tftt_ms: None,
                seed_tps: None,
            }],
            models: vec![],
            model_allowlist_only: false,
            aliases: Default::default(),
            protocol: String::new(),
            has_token_balance_api: false,
            has_bill_balance_api: false,
            price_table: Default::default(),
            output_price_table: Default::default(),
            rpm_limit: None,
            tpm_limit: None,
        });
        let app = App::new(cfg, dir.join("gateway.json"));
        (app, dir.to_string_lossy().into_owned())
    }

    #[test]
    fn real_429_learned_is_ignored_default_applies() {
        // PASS §38: learned_cooldown is deprecated (the prober was removed) —
        // a stale learned value must NOT affect the chain; with no window data
        // the global default applies.
        let (app, dir) = app_with_provider(Some(4), None);
        app.mark_rate_limited("p", "k", None, "r");
        assert!((59..=60).contains(&cooldown_remaining(&app, "k")), "learned ignored → default 60");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn real_429_default_when_no_window_data() {
        let (app, dir) = app_with_provider(Some(221), None);
        app.mark_rate_limited("p", "k", None, "r");
        assert!((59..=60).contains(&cooldown_remaining(&app, "k")), "no metrics → default 60");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn real_429_retry_after_and_per_key_precedence() {
        // Chain: Retry-After → learned → per-key → default (v0.7.9 order).
        let (app, dir) = app_with_provider(Some(4), None);
        app.mark_rate_limited("p", "k", Some(10), "r");
        assert!((9..=10).contains(&cooldown_remaining(&app, "k")), "Retry-After beats learned");
        let _ = std::fs::remove_dir_all(&dir);
        let (app2, dir2) = app_with_provider(None, Some(15));
        app2.mark_rate_limited("p", "k", None, "r");
        assert!((14..=15).contains(&cooldown_remaining(&app2, "k")), "per-key used when no learned");
        let _ = std::fs::remove_dir_all(&dir2);
    }

    #[test]
    fn extend_cooldown_at_least_never_shortens() {
        let dir = std::env::temp_dir().join(format!("extend-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let app = App::new(Config::default(), dir.join("gateway.json"));
        app.mark_rate_limited("p", "k", None, "r"); // 60s (default floor)
        // A shorter probe hold must not shorten the AIMD cooldown.
        app.extend_cooldown_at_least("k", 5);
        assert!((59..=60).contains(&cooldown_remaining(&app, "k")), "must not shorten");
        // A longer hold extends.
        app.extend_cooldown_at_least("k", 120);
        assert!((119..=120).contains(&cooldown_remaining(&app, "k")), "must extend");
        let _ = std::fs::remove_dir_all(&dir);
    }

    // --- measured capacity (PASS §36): 429 samples → min estimate → K headroom ---

    #[test]
    fn rate_limit_marks_sample_capacity() {
        let (app, dir) = app_with_provider(None, None);
        // Simulate two requests having been routed (rpm window) then a 429.
        {
            let mut pool = app.pool.lock().unwrap();
            let m = pool.metrics.entry("k".into()).or_default();
            m.req_times.push_back(Instant::now());
            m.req_times.push_back(Instant::now());
            m.token_events.push_back((Instant::now(), 500));
        }
        app.mark_rate_limited("p", "k", None, "429");
        let pool = app.pool.lock().unwrap();
        let m = pool.metrics.get("k").unwrap();
        assert_eq!(m.capacity_samples.len(), 1);
        assert_eq!(m.capacity_samples[0], (2, 500), "sampled (rpm, tpm) at the 429");
        drop(pool);
        // A (0,0)-usage 429 later is uninformative and must NOT be recorded
        // (limit hit by traffic outside our window would zero the estimate).
        {
            let mut pool = app.pool.lock().unwrap();
            let m = pool.metrics.get_mut("k").unwrap();
            m.req_times.clear();
            m.token_events.clear();
        }
        app.mark_rate_limited("p", "k", None, "429 again");
        let pool = app.pool.lock().unwrap();
        assert_eq!(pool.metrics.get("k").unwrap().capacity_samples.len(), 1, "zero sample skipped");
        drop(pool);
        let _ = std::fs::remove_dir_all(&dir);
    }

    // --- window-drain recovery estimate (PASS §38) ---

    #[test]
    fn window_recovery_rpm_oldest_must_age_out() {
        // cap 3 rpm; 3 requests (chronological: oldest first) at now−32/−31/−30s →
        // the oldest ages out in 28s; ×0.9 bias → ceil(25.2) = 26.
        let mut m = KeyMetrics::default();
        let now = Instant::now();
        for i in [32, 31, 30] {
            m.req_times.push_back(now - Duration::from_secs(i));
        }
        assert_eq!(window_recovery_secs(&m, now, Some(3), None), Some(26));
    }

    #[test]
    fn window_recovery_tpm_drains_oldest_events() {
        // cap 1000 tok/min; events 600@(now−30s) + 600@(now−10s), avg req 600 →
        // need 800 drained → both events must expire → recovery = 50s → ×0.9 = 45.
        let mut m = KeyMetrics::default();
        let now = Instant::now();
        m.req_times.push_back(now - Duration::from_secs(30));
        m.req_times.push_back(now - Duration::from_secs(10));
        m.token_events.push_back((now - Duration::from_secs(30), 600));
        m.token_events.push_back((now - Duration::from_secs(10), 600));
        assert_eq!(window_recovery_secs(&m, now, None, Some(1000)), Some(45));
    }

    #[test]
    fn window_recovery_none_without_capacity_or_explanation() {
        let mut m = KeyMetrics::default();
        let now = Instant::now();
        m.req_times.push_back(now);
        // No capacity configured → None (caller falls back to the default).
        assert_eq!(window_recovery_secs(&m, now, None, None), None);
        // Capacity configured but window usage is BELOW it (the 429 came from
        // something the window can't explain) → None too.
        assert_eq!(window_recovery_secs(&m, now, Some(10), None), None);
    }

    #[test]
    fn window_recovery_caps_at_900_and_floors_at_2() {
        // A 429 an instant ago at capacity → oldest ages out in ~60s (fine).
        // Cap: huge tpm backlog → clamped to 900. Floor: tiny wait → 2s.
        let mut m = KeyMetrics::default();
        let now = Instant::now();
        m.req_times.push_back(now);
        assert_eq!(window_recovery_secs(&m, now, Some(1), None), Some(54)); // 60×0.9
        // TPM floor: total=800, avg=400, cap=1000 → need 200 drained → oldest
        // event expires in 1s → ×0.9 → clamp to 2s.
        let mut m3 = KeyMetrics::default();
        m3.req_times.push_back(now - Duration::from_secs(59));
        m3.req_times.push_back(now - Duration::from_secs(58));
        m3.token_events.push_back((now - Duration::from_secs(59), 400));
        m3.token_events.push_back((now - Duration::from_secs(58), 400));
        // total=800, avg=400, cap=1000 → need 800+400-1000=200 → oldest event
        // (400 ≥ 200) expires in 1s → ×0.9 = 0.9 → ceil 1 → clamp 2s.
        assert_eq!(window_recovery_secs(&m3, now, None, Some(1000)), Some(2));
    }

    #[test]
    fn fill_rows_uses_configured_capacity_not_429_samples() {
        // PASS §38: 429 samples provably can't measure capacity (a 429 says
        // rpm≥limit OR tpm≥limit — the disjunction can't assign either), so
        // they must NOT feed the headroom. The seed stays the capacity.
        let mut pool = PoolRuntime::default();
        {
            let m = pool.metrics.entry("k".into()).or_default();
            for _ in 0..5 {
                m.req_times.push_back(Instant::now());
            }
            m.capacity_samples.push_back((10, 0));
        }
        let mut rows = vec![k_row("k", 1.0, Some(1000))];
        fill_rows(&mut pool, &mut rows, Instant::now());
        assert_eq!(rows[0].capacity_rpm, Some(1000), "seed wins; samples are diagnostics only");
        // sr = (0+2)/(0+2) = 1.0 (no success samples); headroom = 1 - 5/1000 = 0.995
        assert!((rows[0].availability - 0.995).abs() < 1e-9, "got {}", rows[0].availability);
    }
}

impl App {
    pub fn new(config: Config, config_path: std::path::PathBuf) -> Self {
        let http = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(10))
            .build()
            .expect("failed to build http client");
        let stats_path = config_path.with_file_name("stats.json");
        // Stats now live inside gateway.json (under `stats`, merged with the config).
        // Load from there; if absent (pre-merge gateway.json), migrate from the legacy
        // standalone stats.json. The flag lets save_all delete stats.json after the
        // first successful merged write (so the stats land in gateway.json first).
        let (mut stats, migrated) = load_stats_merged(&config_path, &stats_path);
        // Prune the 24h hourly histogram on startup: buckets older than 24h
        // (from a previous session that was down for >24h) are removed, but
        // recent buckets (e.g. 2pm data when restarting at 9pm) are kept —
        // the sliding window continues across restarts.
        stats.prune(now_ms());
        Self {
            config: std::sync::RwLock::new(config),
            pool: Mutex::new(PoolRuntime::default()),
            http,
            config_path,
            stats: Mutex::new(stats),
            stats_path,
            stats_migrated: std::sync::atomic::AtomicBool::new(migrated),
            config_dirty: Mutex::new(false),
            catalog: crate::free_catalog::CatalogCache::new(),
            bound_api_port: std::sync::atomic::AtomicU16::new(0),
            bound_ui_port: std::sync::atomic::AtomicU16::new(0),
        }
    }

    /// Record a finished request (success = upstream returned 2xx and the client got it).
    pub fn record_stat(&self, success: bool, key_id: Option<&str>, provider_id: &str, model: &str) {
        let mut stats = self.stats.lock().unwrap();
        stats.record(success, key_id, provider_id, model, now_ms());
        drop(stats);
        if let Some(k) = key_id {
            self.record_key_metric(success, k);
        }
    }

    /// Count a per-key outcome for a key that was rotated out mid-request (429 / auth
    /// failure / timeout): the client-level request is counted by whoever ultimately
    /// serves it, but this key's own success/fail tally must still move.
    pub fn record_key_stat(&self, success: bool, key_id: &str) {
        let mut stats = self.stats.lock().unwrap();
        stats.record_key_only(success, key_id);
        drop(stats);
        self.record_key_metric(success, key_id);
    }

    /// Record that a key was rotated out (real upstream failure the retry layer
    /// absorbed). Shown on the Status page so provider health is visible even when
    /// every client request ultimately succeeds.
    pub fn record_rotation(&self, provider_id: &str) {
        let mut stats = self.stats.lock().unwrap();
        stats.record_rotation(provider_id);
    }

    /// Attribute `tokens` (input+output) to the total, current hour, key, provider
    /// and model stats buckets. Called from the proxy once the upstream's `usage` is
    /// known (non-stream: from the buffered body; stream: from the usage chunk).
    pub fn record_tokens(&self, tokens: u64, key_id: Option<&str>, provider_id: &str, model: &str) {
        let mut stats = self.stats.lock().unwrap();
        stats.add_tokens(tokens, key_id, provider_id, model, now_ms());
    }

    /// Persist gateway.json (config + stats, with provider_metrics sampled from live
    /// traffic). Called by the 5s flusher. save_all does the prune + the atomic write.
    pub fn flush_stats(&self) {
        self.save_all();
    }

    /// Mark gateway.json dirty; the background flusher writes it once the
    /// debounce window has passed without further changes.
    /// Currently unused (the learned-cooldown writer went away with the
    /// prober, PASS §38) — kept as the generic dirty-flush mechanism.
    #[allow(dead_code)]
    pub fn mark_config_dirty(&self) {
        *self.config_dirty.lock().unwrap() = true;
    }

    /// If dirty and the debounce window has elapsed since `last_change`, write the
    /// config. Returns true when a write happened. `last_change` is caller-tracked
    /// because the flusher owns the timer.
    pub fn flush_config_if_due(&self, last_change: &mut Option<std::time::Instant>) -> bool {
        let mut dirty = self.config_dirty.lock().unwrap();
        if !*dirty {
            return false;
        }
        let due = match *last_change {
            Some(t) => t.elapsed() >= Duration::from_millis(CONFIG_FLUSH_DEBOUNCE_MS),
            None => true,
        }; // never-marked = long overdue, write now
        if due {
            if self.save_all() {
                *dirty = false;
            }
            *last_change = Some(Instant::now());
        }
        due
    }

    pub fn read_config(&self) -> Config {
        self.config.read().unwrap().clone()
    }

    pub fn write_config<F: FnOnce(&mut Config)>(&self, f: F) -> Config {
        let snapshot = {
            let mut cfg = self.config.write().unwrap();
            f(&mut cfg);
            cfg.clone()
        }; // write lock dropped before save_all (which re-locks config for read)
        if !self.save_all() {
            eprintln!("[gateway] WARNING: failed to persist gateway.json");
        }
        snapshot
    }

    /// Per-provider detected metrics for the UI: the live aggregate when the provider
    /// has served traffic since startup, else the persisted `stats.provider_metrics`
    /// value (so the UI shows the last-known detected RPM/TPM/etc. right after a
    /// restart, until new traffic overrides).
    pub fn detected_metrics_for(&self, cfg: &Config, pid: &str) -> crate::stats::ProviderMetrics {
        let p = match cfg.provider_by_id(pid) {
            Some(p) => p,
            None => return Default::default(),
        };
        {
            let pool = self.pool.lock().unwrap();
            if let Some(lm) = provider_live_metrics_from_pool(&pool, p) {
                return lm;
            }
        }
        let stats = self.stats.lock().unwrap();
        stats.provider_metrics.get(pid).cloned().unwrap_or_default()
    }

    /// Per-model detected metrics for the model card / /api/stats: the live
    /// avgTPS+avgTFTT when the model has served streaming traffic since startup,
    /// else the persisted `stats.model_metrics` value (so the card shows the
    /// last-known value right after a restart, until new streaming traffic overrides).
    pub fn detected_model_metrics_for(&self, model: &str) -> crate::stats::ModelMetrics {
        {
            let pool = self.pool.lock().unwrap();
            if let Some(lm) = model_live_metrics_from_pool(&pool, model) {
                return lm;
            }
        }
        let stats = self.stats.lock().unwrap();
        stats.model_metrics.get(model).cloned().unwrap_or_default()
    }

    /// Per-key detected metrics: live pool values when the key has served traffic
    /// since startup, else the persisted `stats.key_metrics` value (so the API Key
    /// page shows the last-known RPM/TPM/success_rate/avg_tftt/tps after a restart).
    /// Returns a KeyMetricsSnapshot (includes token_balance/bill_balance from live).
    /// Capacity + utilization come from the CONFIGURED capacity (seed/provider
    /// limits), the only sound source (PASS §38).
    pub fn detected_key_metrics_for(&self, key_id: &str) -> KeyMetricsSnapshot {
        let cfg = self.read_config();
        let (cap_rpm, cap_tpm) = cfg
            .providers
            .iter()
            .find(|p| p.keys.iter().any(|k| k.id == key_id))
            .map(|p| configured_capacity(&cfg, &p.id, key_id))
            .unwrap_or((None, None));
        {
            let pool = self.pool.lock().unwrap();
            if let Some(lm) = key_live_metrics_from_pool(&pool, key_id) {
                let bill_balance = pool.metrics.get(key_id).and_then(|m| m.bill_balance);
                return KeyMetricsSnapshot {
                    success_rate: lm.success_rate,
                    rpm: lm.rpm,
                    tpm: lm.tpm,
                    avg_tftt_ms: lm.avg_tftt_ms,
                    tps: lm.tps,
                    token_balance: None,
                    bill_balance,
                    capacity_rpm: cap_rpm,
                    capacity_tpm: cap_tpm,
                    utilization: utilization_of(lm.rpm, lm.tpm, cap_rpm, cap_tpm),
                };
            }
        }
        let stats = self.stats.lock().unwrap();
        if let Some(pm) = stats.key_metrics.get(key_id) {
            return KeyMetricsSnapshot {
                success_rate: pm.success_rate,
                rpm: pm.rpm,
                tpm: pm.tpm,
                avg_tftt_ms: pm.avg_tftt_ms,
                tps: pm.tps,
                token_balance: None,
                bill_balance: None,
                capacity_rpm: cap_rpm,
                capacity_tpm: cap_tpm,
                utilization: utilization_of(pm.rpm, pm.tpm, cap_rpm, cap_tpm),
            };
        }
        KeyMetricsSnapshot {
            success_rate: 1.0,
            ..Default::default()
        }
    }

    /// Serialize the merged gateway.json content: config fields + a `stats` field
    /// (with provider_metrics refreshed from live traffic for providers that have
    /// served since startup). Used by save_all (write) + admin export_config
    /// (download). Also mirrors the refreshed provider_metrics back into the live
    /// in-memory stats so the next detected_metrics_for() fallback is current.
    pub(crate) fn config_plus_stats_json(&self) -> String {
        // admin::export_config reuses this to serve the merged gateway.json content
        // as a portable download (config + stats with sampled provider_metrics).
        let cfg = self.config.read().unwrap().clone();
        let mut stats = self.stats.lock().unwrap().clone();
        {
            let pool = self.pool.lock().unwrap();
            for p in &cfg.providers {
                if let Some(lm) = provider_live_metrics_from_pool(&pool, p) {
                    stats.provider_metrics.insert(p.id.clone(), lm);
                }
                for mo in &p.models {
                    if let Some(lm) = model_live_metrics_from_pool(&pool, &mo.id) {
                        stats.model_metrics.insert(mo.id.clone(), lm);
                    }
                }
                for k in &p.keys {
                    if let Some(lm) = key_live_metrics_from_pool(&pool, &k.id) {
                        // Live windows carry fresh rpm/tpm/sr; capacity estimates
                        // fall back to the persisted value when this key has no
                        // 429 samples in memory (e.g. after a restart).
                        let mut lm = lm;
                        if lm.capacity_rpm.is_none() || lm.capacity_tpm.is_none() {
                            if let Some(old) = stats.key_metrics.get(&k.id) {
                                lm.capacity_rpm = lm.capacity_rpm.or(old.capacity_rpm);
                                lm.capacity_tpm = lm.capacity_tpm.or(old.capacity_tpm);
                            }
                        }
                        stats.key_metrics.insert(k.id.clone(), lm);
                    }
                }
            }
        }
        stats.prune(now_ms());
        let mut val = serde_json::to_value(&cfg).unwrap_or(serde_json::Value::Null);
        if let Some(obj) = val.as_object_mut() {
            obj.insert(
                "stats".into(),
                serde_json::to_value(&stats).unwrap_or(serde_json::Value::Null),
            );
        }
        {
            let mut live = self.stats.lock().unwrap();
            live.provider_metrics = stats.provider_metrics.clone();
            live.model_metrics = stats.model_metrics.clone();
            live.key_metrics = stats.key_metrics.clone();
        }
        serde_json::to_string_pretty(&val).unwrap_or_default()
    }

    /// Atomic save of the merged gateway.json (config + stats with sampled
    /// provider_metrics). All persistence routes through here: the 5s stats flusher,
    /// the debounced learned-cooldown flusher, and immediate console-change writes.
    /// Returns false on write failure (callers keep the dirty flag to retry). Also
    /// deletes the legacy stats.json after the first successful merged write that
    /// follows a migration (stats.json is removed only once its data is in gateway.json).
    pub fn save_all(&self) -> bool {
        let pretty = self.config_plus_stats_json();
        let tmp = self.config_path.with_extension("json.tmp");
        let renamed =
            std::fs::write(&tmp, pretty).is_ok() && std::fs::rename(&tmp, &self.config_path).is_ok();
        if !renamed {
            eprintln!("[gateway] WARNING: failed to persist gateway.json");
            return false;
        }
        if self
            .stats_migrated
            .swap(false, std::sync::atomic::Ordering::Relaxed)
        {
            let _ = std::fs::remove_file(&self.stats_path);
        }
        true
    }

    /// Mark a key as cooling down. `retry_after_secs` wins if present.
    /// (learned_cooldown is no longer read anywhere — PASS §38 removed the
    /// prober; the field stays in the schema for backward compatibility.)
    pub fn mark_cooldown(&self, provider_id: &str, key_id: &str, retry_after_secs: Option<u64>, reason: &str) {
        let cfg = self.read_config();
        let default_secs = cfg.default_cooldown_secs;
        let per_key = cfg
            .provider_by_id(provider_id)
            .and_then(|p| p.keys.iter().find(|k| k.id == key_id))
            .and_then(|k| k.cooldown_secs);
        let secs = retry_after_secs
            .or(per_key)
            .unwrap_or(default_secs)
            .max(1);
        let mut pool = self.pool.lock().unwrap();
        pool.cooling.insert(key_id.to_string(), Instant::now() + Duration::from_secs(secs));
        pool.last_error.insert(key_id.to_string(), reason.to_string());
    }

    /// Quarantine a key whose credentials were rejected (401/403): round-robin skips it
    /// until `secs` elapse. Manual "解除冷却" in the UI clears it immediately.
    pub fn mark_invalid(&self, _provider_id: &str, key_id: &str, secs: u64, reason: &str) {
        let mut pool = self.pool.lock().unwrap();
        pool.invalid.insert(key_id.to_string(), Instant::now() + Duration::from_secs(secs));
        pool.last_error.insert(key_id.to_string(), reason.to_string());
    }

    /// 429 cooldown for a REAL request. Chain: `Retry-After` (upstream is
    /// authoritative) → per-key `cooldown_secs` (explicit user intent) →
    /// **window-drain estimate** (when enough of the 60s usage window slides
    /// out for the next request to fit, given the configured capacity) →
    /// global default. The active prober was removed (PASS §38): it burned the
    /// limited keys' own RPM to measure them, and 1-token probes never matched
    /// real payloads. Real requests after cooldown ARE the probe now.
    ///
    /// Also samples the key's 60s-window (rpm, tpm) usage at the 429 moment —
    /// kept for diagnostics (PASS §36 sampling; §38: NOT a capacity source).
    pub fn mark_rate_limited(&self, provider_id: &str, key_id: &str, retry_after_secs: Option<u64>, reason: &str) {
        let cfg = self.read_config();
        let default_secs = cfg.default_cooldown_secs;
        let key_cfg = cfg
            .provider_by_id(provider_id)
            .and_then(|p| p.keys.iter().find(|k| k.id == key_id));
        let per_key = key_cfg.and_then(|k| k.cooldown_secs);
        let (cap_rpm, cap_tpm) = configured_capacity(&cfg, provider_id, key_id);

        let mut pool = self.pool.lock().unwrap();
        // Diagnostic sample of the load at the 429 moment. (0,0) is
        // uninformative (the limit was hit by traffic outside our window).
        {
            let now = Instant::now();
            let m = pool.metrics.entry(key_id.to_string()).or_default();
            m.req_times.retain(|t| now.duration_since(*t) <= Duration::from_secs(METRIC_RPM_WINDOW_SECS));
            m.token_events
                .retain(|(t, _)| now.duration_since(*t) <= Duration::from_secs(METRIC_TPM_WINDOW_SECS));
            let rpm = m.req_times.len() as u32;
            let tpm = m.token_events.iter().map(|(_, n)| *n).sum::<u64>() as u32;
            if rpm > 0 || tpm > 0 {
                m.capacity_samples.push_back((rpm, tpm));
                while m.capacity_samples.len() > CAPACITY_SAMPLE_CAP {
                    m.capacity_samples.pop_front();
                }
            }
        }
        let secs = match retry_after_secs {
            Some(ra) => ra.max(1),
            None => match per_key {
                Some(pk) => pk.max(1),
                None => pool
                    .metrics
                    .get(key_id)
                    .and_then(|m| window_recovery_secs(m, Instant::now(), cap_rpm, cap_tpm))
                    .unwrap_or(default_secs)
                    .max(1),
            },
        };
        pool.cooling.insert(key_id.to_string(), Instant::now() + Duration::from_secs(secs));
        pool.last_error.insert(key_id.to_string(), reason.to_string());
    }

    /// Extend a key's cooling to at least `secs` from now — never shortens an
    /// existing cooldown.
    #[allow(dead_code)] // kept for future probe-hold designs; the prober must not
                        // hold keys (benching during measurement collapses the pool
                        // under 429 storms — see PASS §30.3)
    pub fn extend_cooldown_at_least(&self, key_id: &str, secs: u64) {
        let mut pool = self.pool.lock().unwrap();
        let target = Instant::now() + Duration::from_secs(secs);
        pool.cooling
            .entry(key_id.to_string())
            .and_modify(|u| {
                if *u < target {
                    *u = target;
                }
            })
            .or_insert(target);
    }

    /// Push a per-key outcome into the rolling success window (feeds the
    /// `success_rate` strategy sort attribute, spec §2.2). Folded into
    /// record_stat / record_key_stat so every key attempt's outcome is captured
    /// with no new call sites in the proxy hot path.
    fn record_key_metric(&self, success: bool, key_id: &str) {
        let now = Instant::now();
        let mut pool = self.pool.lock().unwrap();
        let m = pool.metrics.entry(key_id.to_string()).or_default();
        m.success.push_back((now, success));
        let win = Duration::from_secs(METRIC_SUCCESS_WINDOW_SECS);
        m.success.retain(|e| now.duration_since(e.0) <= win);
    }

    /// Push a token-count sample into the rolling TPM window (spec §2.2 `tpm`
    /// source). Called from the success path with the request's total tokens
    /// (input+output) — actual `usage` when the upstream reports it, else a rough
    /// estimate (decision ①A). Streaming + non-stream both feed this.
    pub(crate) fn record_token_count(&self, key_id: &str, tokens: u64) {
        if tokens == 0 {
            return;
        }
        let now = Instant::now();
        let mut pool = self.pool.lock().unwrap();
        let m = pool.metrics.entry(key_id.to_string()).or_default();
        m.token_events.push_back((now, tokens));
        let win = Duration::from_secs(METRIC_TPM_WINDOW_SECS);
        m.token_events.retain(|(t, _)| now.duration_since(*t) <= win);
    }

    /// Record a time-to-first-token sample (ms) — streaming only (decision ②A).
    #[allow(dead_code)] // wired in the streaming chunk
    pub(crate) fn record_tftt(&self, key_id: &str, ms: u32) {
        let mut pool = self.pool.lock().unwrap();
        let m = pool.metrics.entry(key_id.to_string()).or_default();
        m.tftt_samples.push_back(ms);
        while m.tftt_samples.len() > METRIC_SAMPLE_CAP {
            m.tftt_samples.pop_front();
        }
    }

    /// Record a per-model time-to-first-token sample (ms) — streaming only. Keys
    /// the model-page avg-TFTT card by the upstream model id (= managed model id).
    pub(crate) fn record_model_tftt(&self, model: &str, ms: u32) {
        let mut pool = self.pool.lock().unwrap();
        let s = pool.model_tftt.entry(model.to_string()).or_default();
        s.push_back(ms);
        while s.len() > METRIC_SAMPLE_CAP {
            s.pop_front();
        }
    }

    /// Mean per-model TFTT (ms), or 0 when no streamed sample yet.
    #[allow(dead_code)] // superseded by detected_model_metrics_for (live-vs-persisted); kept as a live accessor
    pub fn model_avg_tftt(&self, model: &str) -> u32 {
        let pool = self.pool.lock().unwrap();
        match pool.model_tftt.get(model) {
            None => 0,
            Some(s) if s.is_empty() => 0,
            Some(s) => (s.iter().map(|&v| v as u64).sum::<u64>() / s.len() as u64) as u32,
        }
    }

    /// Record a per-model tokens/sec sample — streaming generation rate.
    pub(crate) fn record_model_tps(&self, model: &str, tps: f32) {
        let mut pool = self.pool.lock().unwrap();
        let s = pool.model_tps.entry(model.to_string()).or_default();
        s.push_back(tps);
        while s.len() > METRIC_SAMPLE_CAP {
            s.pop_front();
        }
    }

    /// Mean per-model TPS (tokens/sec), or 0.0 when no streamed sample yet.
    #[allow(dead_code)] // superseded by detected_model_metrics_for (live-vs-persisted); kept as a live accessor
    pub fn model_avg_tps(&self, model: &str) -> f32 {
        let pool = self.pool.lock().unwrap();
        match pool.model_tps.get(model) {
            None => 0.0,
            Some(s) if s.is_empty() => 0.0,
            Some(s) => s.iter().map(|&v| v).sum::<f32>() / s.len() as f32,
        }
    }

    /// Record a tokens/sec sample — streaming generation rate (spec §2.2 `tps`).
    #[allow(dead_code)] // wired in the streaming chunk
    pub(crate) fn record_tps(&self, key_id: &str, tps: f32) {
        let mut pool = self.pool.lock().unwrap();
        let m = pool.metrics.entry(key_id.to_string()).or_default();
        m.tps_samples.push_back(tps);
        while m.tps_samples.len() > METRIC_SAMPLE_CAP {
            m.tps_samples.pop_front();
        }
    }

    /// Set the USD-normalized bill balance (spec §2.2 `bill_balance`, decision ④A
    /// currency normalization). None = +∞. Set by the balance fetcher.
    #[allow(dead_code)] // wired in the balance-fetcher chunk
    pub(crate) fn set_bill_balance(&self, key_id: &str, usd: Option<f64>) {
        let mut pool = self.pool.lock().unwrap();
        let m = pool.metrics.entry(key_id.to_string()).or_default();
        m.bill_balance = usd;
    }

    /// Per-model price (CNY / 1M tok) from the provider's `price_table`.
    /// Missing entry = 0 (free). Feeds the `price` strategy sort attribute (I, §2.2).
    #[allow(dead_code)] // consumed by the strategy layer once implemented
    pub fn price_of(&self, provider_id: &str, model: &str) -> f64 {
        self.read_config()
            .provider_by_id(provider_id)
            .and_then(|p| p.price_table.get(model).copied())
            .unwrap_or(0.0)
    }

    /// Strategy-layer select (spec §3 ③④⑤ + §6; impl-spec §4): fill the
    /// candidate rows' cooldown + metrics state from the pool, then call
    /// `strategy::select_pure` (D1 rotate → stable sort → walk → terminal)
    /// under one lock so the cursor stays consistent with the selection.
    ///
    /// Built once per request (`build_candidates`), re-walked each attempt as
    /// keys cool down — `pick_key`'s replacement on the request hot path.
    /// North-compat: both-ON + empty sort reproduces `pick_key`'s
    /// round-robin-skip-cooldown (impl-spec §0, verified by `select_pure` tests).
    pub fn select_strategy(
        &self,
        set: &crate::strategy::CandidateSet,
        sorts: &[crate::config::SortKey],
    ) -> crate::strategy::SelectResult {
        // Clone the skeletons so the caller's set stays reusable across attempts;
        // the sort reorders the local copy only.
        let mut rows = set.rows.clone();
        let now = Instant::now();
        // F8: one lock for fill + cursor read + select + cursor persist + rpm bump.
        // select_pure is pure nanosecond-scale compute, so holding the pool lock
        // across it is safe — and it closes the read-cursor → write-cursor race
        // window where two concurrent requests could pick the same row.
        let mut pool = self.pool.lock().unwrap();
        fill_rows(&mut pool, &mut rows, now);
        let cursor = *pool.strategy_cursors.entry(set.cursor_key.clone()).or_insert(0);
        let (result, new_cursor) = crate::strategy::select_pure(&mut rows, sorts, cursor);
        pool.strategy_cursors.insert(set.cursor_key.clone(), new_cursor);
        // A routed row bumps the rolling RPM window (matches pick_key's
        // req_times push, so the `rpm` sort attribute tracks live load).
        if let crate::strategy::SelectResult::Routed(ref row) = result {
            let now = Instant::now();
            let m = pool.metrics.entry(row.api_key_id.clone()).or_default();
            m.req_times.push_back(now);
            let win = Duration::from_secs(METRIC_RPM_WINDOW_SECS);
            m.req_times.retain(|t| now.duration_since(*t) <= win);
            // Count the routed attempt (pick_key bumped `requests` for the UI).
            *pool.requests.entry(row.api_key_id.clone()).or_insert(0) += 1;
        }
        result
    }

    /// Dry-run preview (impl-spec §8): the same fill + sort + walk as
    /// `select_strategy`, but read-only — it does **not** advance the persisted
    /// cursor or bump the RPM window. Returns the §6 terminal + the full sorted
    /// candidate list (the would-be-routed row marked) so the UI strategy board
    /// (prototype `computeRoute()` + `resultBanner`) can show what the selector
    /// *would* route. Never exposes `key_secret` (the secret never leaves the
    /// process; only `api_key_id` is reported).
    pub fn strategy_dry_run(
        &self,
        set: &crate::strategy::CandidateSet,
        sorts: &[crate::config::SortKey],
    ) -> serde_json::Value {
        let mut rows = set.rows.clone();
        let now = Instant::now();
        let mut pool = self.pool.lock().unwrap();
        fill_rows(&mut pool, &mut rows, now);
        let cursor = *pool.strategy_cursors.entry(set.cursor_key.clone()).or_insert(0);
        drop(pool);
        let (result, routed_idx, _new_cursor) =
            crate::strategy::select_preview(&mut rows, sorts, cursor);
        let (result_type, retry_after) = match &result {
            crate::strategy::SelectResult::Routed(_) => ("routed", None),
            crate::strategy::SelectResult::AllCooling { retry_after } => {
                ("all_cooling", *retry_after)
            }
            crate::strategy::SelectResult::NoValid => ("no_valid", None),
            crate::strategy::SelectResult::Empty => ("empty", None),
        };
        let routed_summary = match &result {
            crate::strategy::SelectResult::Routed(r) => Some(serde_json::json!({
                "provider_id": r.provider_id,
                "upstream_model": r.upstream_model,
                "api_key_id": r.api_key_id,
                "group_id": r.group_id,
            })),
            _ => None,
        };
        let candidates: Vec<serde_json::Value> = rows
            .iter()
            .enumerate()
            .map(|(i, r)| {
                serde_json::json!({
                    "provider_id": r.provider_id,
                    "upstream_model": r.upstream_model,
                    "api_key_id": r.api_key_id,
                    "group_id": r.group_id,
                    "valid": r.valid,
                    "non_cooled": r.non_cooled,
                    "remaining_secs": r.remaining_secs,
                    "success_rate": r.success_rate,
                    "rpm": r.rpm,
                    "tpm": r.tpm,
                    "avg_tftt_ms": r.avg_tftt_ms,
                    "tps": r.tps,
                    "token_balance": r.token_balance,
                    "bill_balance": r.bill_balance,
                    "price": r.price,
                    "idx": r.idx,
                    "capacity_rpm": r.capacity_rpm,
                    "capacity_tpm": r.capacity_tpm,
                    "availability": r.availability,
                    "routed": routed_idx == Some(i),
                })
            })
            .collect();
        serde_json::json!({
            "result": result_type,
            "retry_after": retry_after,
            "routed": routed_summary,
            "candidates": candidates,
            "cursor": cursor,
        })
    }

    /// Soonest a *valid* (non-dead) key in the given candidate set recovers
    /// from rate-limit cooldown (spec §6 "全冷却" Retry-After), or `None` when
    /// no key is merely cooling (all dead/invalid → no Retry-After). The
    /// generalization of `min_retry_after_secs` to a cross-provider candidate
    /// set (impl-spec §6, decision D6): signature takes the candidate rows
    /// rather than a single `&Provider`.
    #[allow(dead_code)] // wired in forward() for the §6 terminal
    pub fn min_retry_after_candidates(&self, set: &crate::strategy::CandidateSet) -> Option<u64> {
        let now = Instant::now();
        let mut pool = self.pool.lock().unwrap();
        pool.cooling.retain(|_, until| *until > now);
        pool.invalid.retain(|_, until| *until > now);
        let mut min: Option<u64> = None;
        for r in &set.rows {
            // Dead (401/403-quarantined) keys are not valid — skip (spec §6).
            if pool.invalid.contains_key(&r.api_key_id) {
                continue;
            }
            if let Some(until) = pool.cooling.get(&r.api_key_id) {
                let secs = ceil_secs(until.saturating_duration_since(now));
                min = Some(min.map_or(secs, |m| m.min(secs)));
            }
        }
        min
    }

    pub fn clear_error(&self, key_id: &str) {
        self.pool.lock().unwrap().last_error.remove(key_id);
    }

    /// The first key of `provider` that is neither cooling nor invalid — for
    /// auxiliary upstream calls (count_tokens, upstream model fetch) that don't
    /// go through the strategy selector but still shouldn't hammer a benched
    /// key (F3). Falls back to the first key when all are benched.
    pub fn first_usable_key(&self, provider: &crate::config::Provider) -> Option<String> {
        let now = Instant::now();
        let mut pool = self.pool.lock().unwrap();
        pool.cooling.retain(|_, u| *u > now);
        pool.invalid.retain(|_, u| *u > now);
        provider
            .keys
            .iter()
            .find(|k| !pool.cooling.contains_key(&k.id) && !pool.invalid.contains_key(&k.id))
            .or_else(|| provider.keys.first())
            .map(|k| k.key.clone())
    }

    pub fn clear_cooldown(&self, key_id: &str) {
        let mut pool = self.pool.lock().unwrap();
        pool.cooling.remove(key_id);
        pool.invalid.remove(key_id);
    }

    /// Pool status snapshot for the UI: per key -> { cooling_until_ms, requests, last_error }.
    pub fn status(&self) -> serde_json::Value {
        let mut pool = self.pool.lock().unwrap();
        // Expire cooldowns here too (not just in pick_key): the UI polls status, not the
        // proxy path, so without this a finished cooldown would show "0s" forever.
        pool.cooling.retain(|_, until| *until > Instant::now());
        pool.invalid.retain(|_, until| *until > Instant::now());
        let now_ms = now_ms();
        let fmt_remaining = |until: &Instant| -> u64 {
            until.saturating_duration_since(Instant::now()).as_secs()
        };
        let cooling: serde_json::Map<String, serde_json::Value> = pool
            .cooling
            .iter()
            .map(|(k, until)| {
                let remaining = fmt_remaining(until);
                (
                    k.clone(),
                    serde_json::json!({
                        "until_ms": now_ms + remaining * 1000,
                        "remaining_secs": remaining,
                    }),
                )
            })
            .collect();
        let invalid: serde_json::Map<String, serde_json::Value> = pool
            .invalid
            .iter()
            .map(|(k, until)| {
                let remaining = fmt_remaining(until);
                (
                    k.clone(),
                    serde_json::json!({
                        "until_ms": now_ms + remaining * 1000,
                        "remaining_secs": remaining,
                    }),
                )
            })
            .collect();
        serde_json::json!({
            "cooling": cooling,
            "invalid": invalid,
            "requests": pool.requests,
            "last_error": pool.last_error,
        })
    }
}

/// A key's configured capacity: seed_rpm wins per key; else the provider's
/// rpm_limit/tpm_limit split evenly across its keys. This is the ONLY capacity
/// source — 429 samples provably can't measure limits (PASS §38).
fn configured_capacity(
    cfg: &Config,
    provider_id: &str,
    key_id: &str,
) -> (Option<u32>, Option<u32>) {
    let p = match cfg.provider_by_id(provider_id) {
        Some(p) => p,
        None => return (None, None),
    };
    let n = (p.keys.len() as u32).max(1);
    let seed = p.keys.iter().find(|k| k.id == key_id).and_then(|k| k.seed_rpm);
    (seed.or_else(|| p.rpm_limit.map(|l| l / n)), p.tpm_limit.map(|l| l / n))
}

/// Passive window-drain recovery estimate (PASS §38): the key recovers when
/// enough of its 60s-window usage slides out for one more request to fit under
/// the configured capacity. No probe requests are burned — the window contents
/// are already known. Returns None when no capacity is configured or the
/// windows don't explain the 429 (caller falls back to the global default).
///
/// Bias low (×0.9): underestimating costs one cheap rotation and the next 429
/// re-estimates with fresher data; overestimating parks a key (the forbidden
/// direction, §30.3).
fn window_recovery_secs(
    m: &KeyMetrics,
    now: Instant,
    cap_rpm: Option<u32>,
    cap_tpm: Option<u32>,
) -> Option<u64> {
    let win = Duration::from_secs(METRIC_RPM_WINDOW_SECS);
    let mut parts: Vec<u64> = Vec::new();

    if let Some(c) = cap_rpm.filter(|c| *c > 0) {
        let n = m.req_times.len();
        let c = c as usize;
        if n >= c {
            // The oldest (n−c+1) requests must age out; the newest of those sits
            // at index n−c. Recovery = its timestamp + window − now.
            let idx = n - c;
            let t = m.req_times[idx];
            parts.push(ceil_secs(win.saturating_sub(now.saturating_duration_since(t))));
        }
    }
    if let Some(c) = cap_tpm.filter(|c| *c > 0) {
        let total: u64 = m.token_events.iter().map(|(_, n)| *n).sum();
        let avg_req = {
            let reqs = m.req_times.len() as u64;
            if reqs > 0 { (total / reqs).max(1) } else { 2000 }
        };
        if total + avg_req > c as u64 {
            let need_drain = total + avg_req - c as u64;
            let mut acc = 0u64;
            for (t, n) in &m.token_events {
                acc += n;
                if acc >= need_drain {
                    parts.push(win.saturating_sub(now.saturating_duration_since(*t)).as_secs());
                    break;
                }
            }
        }
    }
    if parts.is_empty() {
        return None;
    }
    // max over the constrained dimensions, ×0.9 bias-low, clamped [2, 900].
    let est = *parts.iter().max().unwrap();
    let biased = (est as f64 * 0.9).ceil() as u64;
    Some(biased.clamp(2, 900))
}

/// Current utilization (max of the rpm/tpm dimensions) against known capacities.
/// None = no capacity data at all (neutral).
fn utilization_of(rpm: u32, tpm: u32, cap_rpm: Option<u32>, cap_tpm: Option<u32>) -> Option<f64> {
    let mut u: Option<f64> = None;
    let mut acc = |used: u32, cap: Option<u32>| {
        if let Some(c) = cap {
            if c > 0 {
                u = Some(u.unwrap_or(0.0).max(used as f64 / c as f64));
            }
        }
    };
    acc(rpm, cap_rpm);
    acc(tpm, cap_tpm);
    u
}

/// Aggregate a provider's keys' live metrics from the (already-locked) pool. Returns
/// None when the provider has served no traffic since startup (all its keys have
/// empty req_times) — the caller then falls back to the persisted
/// `stats.provider_metrics` so the UI shows the last-known value after a restart.
fn model_live_metrics_from_pool(
    pool: &PoolRuntime,
    model: &str,
) -> Option<crate::stats::ModelMetrics> {
    let tftt = pool.model_tftt.get(model);
    let tps = pool.model_tps.get(model);
    let has_tftt = tftt.map_or(false, |s| !s.is_empty());
    let has_tps = tps.map_or(false, |s| !s.is_empty());
    if !has_tftt && !has_tps {
        return None; // no streaming traffic for this model since startup
    }
    let avg_tftt_ms = match tftt {
        Some(s) if !s.is_empty() => {
            (s.iter().map(|&v| v as u64).sum::<u64>() / s.len() as u64) as u32
        }
        _ => 0,
    };
    let avg_tps = match tps {
        Some(s) if !s.is_empty() => s.iter().map(|&v| v).sum::<f32>() / s.len() as f32,
        _ => 0.0,
    };
    Some(crate::stats::ModelMetrics { avg_tps, avg_tftt_ms })
}

fn provider_live_metrics_from_pool(
    pool: &PoolRuntime,
    p: &crate::config::Provider,
) -> Option<crate::stats::ProviderMetrics> {
    let mut rpm = 0u64;
    let mut tpm = 0u64;
    let mut ok = 0u64;
    let mut total = 0u64;
    let mut tftt_sum = 0u64;
    let mut tftt_n = 0u64;
    let mut tps_sum = 0.0f32;
    let mut tps_n = 0u32;
    let mut has_live = false;
    for k in &p.keys {
        if let Some(m) = pool.metrics.get(&k.id) {
            if !m.req_times.is_empty() {
                has_live = true;
            }
            rpm += m.req_times.len() as u64;
            tpm += m.token_events.iter().map(|(_, n)| *n).sum::<u64>();
            let t = m.success.len() as u64;
            total += t;
            ok += m.success.iter().filter(|e| e.1).count() as u64;
            if !m.tftt_samples.is_empty() {
                tftt_sum += m.tftt_samples.iter().map(|&v| v as u64).sum::<u64>();
                tftt_n += m.tftt_samples.len() as u64;
            }
            if !m.tps_samples.is_empty() {
                tps_sum += m.tps_samples.iter().map(|&v| v).sum::<f32>();
                tps_n += m.tps_samples.len() as u32;
            }
        }
    }
    if !has_live {
        return None;
    }
    let success_rate = if total == 0 { 1.0 } else { ok as f64 / total as f64 };
    let avg_tftt_ms = if tftt_n == 0 { 0 } else { (tftt_sum / tftt_n) as u32 };
    let tps = if tps_n == 0 { 0.0 } else { tps_sum / tps_n as f32 };
    Some(crate::stats::ProviderMetrics {
        rpm: rpm as u32,
        tpm: tpm as u32,
        success_rate,
        avg_tftt_ms,
        tps,
    })
}

/// Per-key live metrics from the pool. Returns None when the key has no live
/// traffic since startup (all rolling windows empty) — the caller falls back
/// to the persisted `stats.key_metrics` so the API Key page shows the last-known
/// value after a restart.
fn key_live_metrics_from_pool(
    pool: &PoolRuntime,
    key_id: &str,
) -> Option<crate::stats::KeyMetricsPersisted> {
    let m = pool.metrics.get(key_id)?;
    // Only return live metrics when the key has RECENT traffic (RPM or TPM window
    // non-empty). The 60s rolling windows expire when the key hasn't been used
    // recently — at that point we must NOT return Some with RPM=0/TPM=0, because
    // that would overwrite the persisted last-known value with zeros on the next
    // flush. Fall back to persisted instead (avg_TFTT/TPS samples are not
    // time-pruned, but they're meaningless without recent traffic context).
    if m.req_times.is_empty() && m.token_events.is_empty() {
        return None;
    }
    let total = m.success.len();
    let success_rate = if total == 0 {
        1.0
    } else {
        let ok = m.success.iter().filter(|e| e.1).count() as f64;
        ok / total as f64
    };
    let tpm = m.token_events.iter().map(|(_, n)| *n).sum::<u64>() as u32;
    let avg_tftt_ms = if m.tftt_samples.is_empty() {
        0
    } else {
        (m.tftt_samples.iter().map(|&v| v as u64).sum::<u64>() / m.tftt_samples.len() as u64) as u32
    };
    let tps = if m.tps_samples.is_empty() {
        0.0
    } else {
        m.tps_samples.iter().map(|&v| v).sum::<f32>() / m.tps_samples.len() as f32
    };
    // capacity_* stay None: the 429-sample estimate was unsound (PASS §38) and
    // nothing reads it anymore; capacity comes from configured limits.
    Some(crate::stats::KeyMetricsPersisted {
        rpm: m.req_times.len() as u32,
        tpm,
        success_rate,
        avg_tftt_ms,
        tps,
        capacity_rpm: None,
        capacity_tpm: None,
    })
}

/// Load stats from gateway.json's `stats` sub-field; if absent (a pre-merge
/// gateway.json), migrate from the legacy standalone stats.json (returns the stats
/// + a migrated flag so save_all can delete stats.json after the first merged write).
fn load_stats_merged(
    config_path: &std::path::Path,
    stats_path: &std::path::Path,
) -> (crate::stats::Stats, bool) {
    if let Ok(raw) = std::fs::read_to_string(config_path) {
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&raw) {
            if let Some(s) = v.get("stats") {
                if let Ok(st) = serde_json::from_value::<crate::stats::Stats>(s.clone()) {
                    return (st, false);
                }
            }
        }
    }
    // No `stats` field in gateway.json (pre-merge) → migrate from legacy stats.json.
    if stats_path.exists() {
        return (crate::stats::Stats::load(stats_path), true);
    }
    (crate::stats::Stats::default(), false)
}
