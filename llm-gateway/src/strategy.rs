//! Routing strategy layer (spec §3-§6; impl-spec §1-§5).
//!
//! Replaces the hardcoded both-ON round-robin `state.rs::pick_key` with a
//! filter → sort → walk selector driven by `Config.strategy`. The default
//! config (both filter toggles ON + empty sort) reproduces the current
//! round-robin-skip-cooldown behavior via D1 cursor rotation (impl-spec §0,
//! §4.1, §9): under the default, `select_pure` sorts only by
//! `(valid↓, non-cooled↓, idx↑)` and the cursor advances one per pick, so the
//! next healthy key in config order is chosen — exactly `pick_key`.
//!
//! This module holds the *pure* logic (types + `build_candidates` skeleton
//! construction from config + `comparator` + `select_pure`). The pool-locked
//! fill of cooldown/metrics state + cursor read/write lives in
//! `state.rs::select_strategy`, which calls `select_pure` under one lock.

use crate::config::{Config, SortKey};
use std::cmp::Ordering;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// One candidate row (spec §2.1, impl-spec §3): the `(provider, model,
/// api_key_id)` triple + group + cooldown-derived state + a metrics snapshot.
///
/// `build_candidates` fills the *skeleton* fields (`provider_id` … `group_id`,
/// `price`); `state.rs::select_strategy` fills the *runtime* fields
/// (`valid`/`non_cooled`/`remaining_secs` + the metrics snapshot) from the
/// pool, then `select_pure` sets `idx` (post-D1-rotation) and consumes it.
#[derive(Debug, Clone)]
pub struct Row {
    // --- skeleton (from config; constant for a request's lifetime) ---
    pub provider_id: String,
    pub upstream_model: String,
    pub api_key_id: String,
    pub key_secret: String,
    /// The logical model group id this row belongs to (spec §1). Set on build;
    /// read by the future dry-run/UI surface + carried for diagnostics.
    #[allow(dead_code)]
    pub group_id: String,
    // --- runtime (filled by select_strategy from the pool) ---
    /// Not in the `invalid` (401/403-quarantine) map (spec §2.3).
    pub valid: bool,
    /// Not in the `cooling` map (spec §2.3 `Cold`).
    pub non_cooled: bool,
    /// Seconds until this key's cooldown expires (0 when not cooling).
    pub remaining_secs: u64,
    // --- metrics snapshot (spec §2.2; zero/None until measured) ---
    pub success_rate: f64, // [0,1]; 1.0 when no history (don't penalize new keys)
    pub rpm: u32,
    pub tpm: u32,
    pub avg_tftt_ms: u32,
    pub tps: f32,
    /// None = +∞ (OptInf, spec §2.2). No catalog provider exposes a clean
    /// remaining-token-quota field, so this is effectively always None.
    pub token_balance: Option<u64>,
    /// None = +∞. USD-normalized bill balance (set by the balance fetcher).
    pub bill_balance: Option<f64>,
    // --- sort-internal (set by select_pure) ---
    pub price: f64,
    /// Post-D1-rotation position; the table-order tiebreaker (impl-spec §4.1).
    pub idx: usize,
}

/// A request's candidate set, built once from config (impl-spec §5: built
/// once, re-walked per attempt as keys cool down). `cursor_key` is the D3
/// candidate-set identifier for the cursor map.
#[derive(Debug, Clone)]
pub struct CandidateSet {
    pub rows: Vec<Row>,
    pub cursor_key: String,
}

/// Outcome of `select_pure` (spec §6 four-state terminal).
#[derive(Debug, Clone)]
pub enum SelectResult {
    /// Walked to a `valid + non-cooled` row — route it.
    Routed(Row),
    /// No valid+non-cooled row, but valid rows exist and they are all cooling.
    /// `retry_after` = the soonest any cooling valid row recovers (None when
    /// every valid row is dead/invalid — spec §6 "无 valid 行 → 429 无 Retry-After").
    AllCooling { retry_after: Option<u64> },
    /// Candidate rows exist but none is valid (all 401/403-quarantined).
    NoValid,
    /// The filter shrank the table to nothing (no candidate rows at all).
    Empty,
}

// ---------------------------------------------------------------------------
// build_candidates — §3 ①② (resolve + filter); all four filter modes (§4)
// ---------------------------------------------------------------------------

/// A resolved request route (spec §3 ①): the logical model + the provider the
/// request pinned (if any). A bare model name with no `provider/` prefix and no
/// alias leaves `provider` `None`, which auto-degrades `lock_provider` to OFF
/// (spec §3 ① / §10 #2): you can't lock a provider the request didn't name.
struct Route {
    /// `Some` when the request named a provider (`provider/model` or an alias,
    /// which is intrinsically provider-scoped); `None` for a bare model name.
    provider: Option<String>,
    /// The logical model id — the group id to look up in `model_groups`, and
    /// the price-table key for group-derived rows.
    logical_model: String,
}

/// Resolve a client `model` string into `(provider, logical_model)` (spec §3 ①,
/// §2.3). Mirrors `Config::resolve_model`'s alias + `provider/model` rules but
/// returns `Ok((None, model))` (rather than an error) for a bare model name, so
/// the strategy layer can still try a model group or degrade `lock_provider`.
fn resolve_request(cfg: &Config, client_model: &str) -> Result<Route, String> {
    // 1. alias (unique across providers) — intrinsically provider-scoped.
    for p in &cfg.providers {
        if let Some(target) = p.aliases.get(client_model) {
            if !p.model_allowed(target) {
                return Err(format!(
                    "alias `{client_model}` points to model `{target}` which does not exist or is disabled on provider `{}`",
                    p.name
                ));
            }
            return Ok(Route { provider: Some(p.id.clone()), logical_model: target.clone() });
        }
    }
    // 2. composite `provider/model` — match on id or name.
    if let Some((head, tail)) = client_model.split_once('/') {
        if let Some(p) = cfg.providers.iter().find(|p| p.id == head || p.name == head) {
            if !p.model_allowed(tail) {
                return Err(format!(
                    "model `{tail}` is not in the managed model list of provider `{}` (allowlist mode is on)",
                    p.name
                ));
            }
            return Ok(Route { provider: Some(p.id.clone()), logical_model: tail.to_string() });
        }
    }
    // 3. bare model — no provider resolved; `lock_provider` degrades OFF.
    Ok(Route { provider: None, logical_model: client_model.to_string() })
}

/// `(provider_id, upstream_model, price_key)` — one filtered candidate entry
/// before it is expanded into per-key rows. `price_key` is the price-table
/// lookup key: the logical model for group-derived rows (price is per logical
/// model, spec §2.2), the upstream model id for provider/available-first rows
/// (which span models without a group).
type Entry = (String, String, String);

/// All `(provider, upstream_model)` entries the given provider serves (its
/// enabled managed-model catalog). Used by provider-first (§4 row 3).
fn entries_for_provider(cfg: &Config, pid: &str) -> Vec<Entry> {
    let p = match cfg.provider_by_id(pid) {
        Some(p) => p,
        None => return Vec::new(),
    };
    p.models
        .iter()
        .filter(|m| m.enabled)
        .map(|m| (pid.to_string(), m.id.clone(), m.id.clone()))
        .collect()
}

/// The whole table: every provider × every enabled managed model (§4 row 4,
/// available-first). Unmanaged providers (empty `models`) contribute nothing —
/// they have no concrete model rows to enumerate.
fn all_entries(cfg: &Config) -> Vec<Entry> {
    let mut out = Vec::new();
    for p in &cfg.providers {
        for m in &p.models {
            if m.enabled {
                out.push((p.id.clone(), m.id.clone(), m.id.clone()));
            }
        }
    }
    out
}

/// Build the candidate row skeletons for a request (spec §3 ①②). Reads only
/// config — no pool state — so it is built once per request and re-walked each
/// attempt by `select_strategy` as keys cool.
///
/// All four filter modes (spec §4) are supported:
/// - **both ON** (default, north-compat): exact `(provider, model)` → that
///   provider's keys. Identical to the old `pick_key` key set.
/// - **model-first** (lock_model_group ON, lock_provider OFF): the model's
///   group across all providers — cross-provider same-model fallback.
/// - **provider-first** (lock_model_group OFF, lock_provider ON): every model
///   of the resolved provider — try other models, sorted.
/// - **available-first** (both OFF): the whole table.
///
/// `lock_provider` auto-degrades OFF when the request named no provider (§3 ①),
/// so a bare model + both-ON effectively becomes model-first.
pub fn build_candidates(cfg: &Config, client_model: &str) -> Result<CandidateSet, String> {
    let filter = &cfg.strategy.filter;
    let route = resolve_request(cfg, client_model)?;
    let logical_model = &route.logical_model;

    // lock_provider can't hold when no provider was resolved (spec §3 ①).
    let lock_provider = filter.lock_provider && route.provider.is_some();

    // The model group, if the logical model is a declared group id (spec §2.3).
    let group = cfg.model_groups.iter().find(|g| &g.id == logical_model);

    // Build the filtered entry list (§4 four modes). `price_key` = logical
    // model for group-derived rows, the upstream model id otherwise.
    let entries: Vec<Entry> = match (filter.lock_model_group, lock_provider) {
        (true, true) => {
            // both ON: exact (resolved_provider, this model).
            let pid = route.provider.as_ref().unwrap();
            match group {
                Some(g) => g
                    .entries
                    .iter()
                    .filter(|(p, _)| p == pid)
                    .map(|(p, um)| (p.clone(), um.clone(), logical_model.clone()))
                    .collect(),
                None => vec![(pid.clone(), logical_model.clone(), logical_model.clone())],
            }
        }
        (true, false) => {
            // model-first: the whole group (all providers offering this model).
            match group {
                Some(g) => g
                    .entries
                    .iter()
                    .map(|(p, um)| (p.clone(), um.clone(), logical_model.clone()))
                    .collect(),
                None => {
                    // No group: degrade to a single entry — the resolved provider
                    // if any, else fall back to resolve_model for a usable route.
                    match &route.provider {
                        Some(pid) => vec![(pid.clone(), logical_model.clone(), logical_model.clone())],
                        None => match cfg.resolve_model(client_model) {
                            Ok((pid, um)) => vec![(pid, um.clone(), um)],
                            Err(_) => Vec::new(),
                        },
                    }
                }
            }
        }
        (false, true) => {
            // provider-first: every model of the resolved provider (across groups).
            entries_for_provider(cfg, route.provider.as_ref().unwrap())
        }
        (false, false) => {
            // available-first: the whole table.
            all_entries(cfg)
        }
    };

    // Expand entries → per-key rows. A row = (provider, upstream_model, key).
    // `group_id` carries the request's logical model (the candidate set's
    // identity); selection never reads it, but the future dry-run/UI will.
    let mut rows: Vec<Row> = Vec::new();
    for (pid, upstream_model, price_key) in &entries {
        let provider = match cfg.provider_by_id(pid) {
            Some(p) => p,
            None => continue, // provider vanished from config since route resolved
        };
        let price = provider.price_table.get(price_key).copied().unwrap_or(0.0);
        for k in &provider.keys {
            rows.push(Row {
                provider_id: pid.clone(),
                upstream_model: upstream_model.clone(),
                api_key_id: k.id.clone(),
                key_secret: k.key.clone(),
                group_id: logical_model.clone(),
                valid: true,
                non_cooled: true,
                remaining_secs: 0,
                success_rate: 1.0,
                rpm: 0,
                tpm: 0,
                avg_tftt_ms: 0,
                tps: 0.0,
                token_balance: None,
                bill_balance: None,
                price,
                idx: 0,
            });
        }
    }

    // D3 cursor key = (group_id, resolved_provider_id_or_"any"). both-ON /
    // provider-first lock the provider; model-first / available-first share an
    // "any" cursor so equivalent keys load-balance across providers.
    let provider_or_any = match (lock_provider, &route.provider) {
        (true, Some(pid)) => pid.clone(),
        _ => "any".to_string(),
    };
    let cursor_key = format!("{}\x1f{}", logical_model, provider_or_any);

    if rows.is_empty() {
        // Surface a clear "unknown model" error when nothing resolved (a bare
        // model with no group and no alias). Otherwise an empty candidate set
        // would silently 429 instead of the familiar 404.
        return match cfg.resolve_model(client_model) {
            Ok(_) => Ok(CandidateSet { rows, cursor_key }), // provider has no keys
            Err(e) => Err(e),
        };
    }
    Ok(CandidateSet { rows, cursor_key })
}

// ---------------------------------------------------------------------------
// comparator — §5.1 fixed leading + §5.2 user A-J + table-order fallback
// ---------------------------------------------------------------------------

/// Total ordering over two rows for given user sorts (spec §5.1, §5.2;
/// impl-spec §4.2):
/// ① `valid` desc (fixed; never route to a dead key), ② `non-cooled` desc
/// (fixed), ③-⑤ the user's 0..=3 sort keys in order, ⑥ `idx` ascending
/// (table-order — the D1 cursor-rotated position, so equivalent keys
/// load-balance across requests).
///
/// A plain function (not a closure-returning `impl Fn`) so it can borrow the
/// user sort slice without a captured-lifetime leak (E0700).
pub fn compare(user: &[SortKey], a: &Row, b: &Row) -> Ordering {
    // ① valid desc: a valid key must always sort before an invalid one.
    let av = a.valid as u8;
    let bv = b.valid as u8;
    bv.cmp(&av)
        // ② non-cooled desc: a ready key sorts before a cooling one.
        .then_with(|| {
            let an = a.non_cooled as u8;
            let bn = b.non_cooled as u8;
            bn.cmp(&an)
        })
        // ③ ④ ⑤ user keys (0..=3), applied in declared order.
        .then_with(|| user.iter().fold(Ordering::Equal, |o, k| o.then(k.cmp(a, b))))
        // ⑥ table-order: the post-D1-rotation index (stable tiebreak).
        .then_with(|| a.idx.cmp(&b.idx))
}

impl SortKey {
    /// Compare `a` vs `b` for this sort key (Less = a before b), per the
    /// attribute + direction in spec §5.2. The inf convention (E/F/G/H) is
    /// two-direction self-consistent: `None` → `f64::INFINITY`, which sorts
    /// first under `desc` (most balance) and last under `asc` (impl-spec §4.2).
    fn cmp(&self, a: &Row, b: &Row) -> Ordering {
        let bal_u64 = |v: Option<u64>| -> f64 { v.map(|x| x as f64).unwrap_or(f64::INFINITY) };
        let bal_f64 = |v: Option<f64>| -> f64 { v.unwrap_or(f64::INFINITY) };
        match self {
            // A success_rate desc — highest success rate first.
            SortKey::A => cmp_f64(b.success_rate, a.success_rate),
            // B rpm desc — highest throughput first.
            SortKey::B => b.rpm.cmp(&a.rpm),
            // C tpm desc — most tokens/min first (long-context).
            SortKey::C => b.tpm.cmp(&a.tpm),
            // D avg_tftt asc — lowest first-token latency first.
            SortKey::D => a.avg_tftt_ms.cmp(&b.avg_tftt_ms),
            // E token_balance desc — most remaining tokens first.
            SortKey::E => cmp_f64(bal_u64(b.token_balance), bal_u64(a.token_balance)),
            // F token_balance asc — fewest remaining first (drain expiring quota).
            SortKey::F => cmp_f64(bal_u64(a.token_balance), bal_u64(b.token_balance)),
            // G bill_balance desc — most remaining credit first.
            SortKey::G => cmp_f64(bal_f64(b.bill_balance), bal_f64(a.bill_balance)),
            // H bill_balance asc — fewest remaining credit first.
            SortKey::H => cmp_f64(bal_f64(a.bill_balance), bal_f64(b.bill_balance)),
            // I price asc — cheapest first.
            SortKey::I => cmp_f64(a.price, b.price),
            // J tps desc — fastest generation first.
            SortKey::J => cmp_f64(b.tps as f64, a.tps as f64),
        }
    }
}

/// Total ordering of two `f64`s, treating NaN as equal (no row carries NaN in
/// practice, but `partial_cmp` requires a fallback to satisfy `Ord`).
fn cmp_f64(a: f64, b: f64) -> Ordering {
    a.partial_cmp(&b).unwrap_or(Ordering::Equal)
}

// ---------------------------------------------------------------------------
// select_pure — §3 ③④⑤ (sort + walk) + §6 terminal; D1 cursor rotation
// ---------------------------------------------------------------------------

/// Pure selection: D1-rotate → assign `idx` → stable sort → walk to the first
/// `valid + non-cooled` row → return the §6 four-state terminal (impl-spec §4).
///
/// `cursor` is the pre-rotation cursor (read from the pool by the caller);
/// the returned `usize` is the new cursor to persist. The caller owns `rows`
/// (a freshly-filled clone), so reordering it in place is safe.
///
/// D1 rotate → assign `idx` → stable sort → walk to the first `valid +
/// non-cooled` row (spec §3 ③④). Precondition: `rows` is non-empty (callers
/// handle the empty case). Returns:
/// - `routed_idx`: `Some(i)` where `rows[i]` is the routed row (it stays in the
///   Vec — `select_pure` removes it; `select_preview` keeps it for display).
/// - `cooling_min`: the soonest recovery among cooling valid rows (§6 Retry-After).
/// - `any_valid`: whether any candidate is valid (distinguishes NoValid vs Empty).
/// - `new_cursor`: `(cursor + 1) % n` only when a row was routed (cursor doesn't
///   move when nothing is served — matches `pick_key` leaving `counters` alone).
fn sort_and_walk(
    rows: &mut Vec<Row>,
    sorts: &[SortKey],
    cursor: usize,
) -> (Option<usize>, Option<u64>, bool, usize) {
    let n = rows.len();
    // D1: rotate so the cursor's row lands at idx=0. `cursor % n` guards a
    // cursor that drifted past n (e.g. after a provider's keys were edited).
    let rot = cursor % n;
    if rot != 0 {
        rows.rotate_left(rot);
    }
    // Assign idx = post-rotation position (the table-order tiebreaker).
    for (i, r) in rows.iter_mut().enumerate() {
        r.idx = i;
    }
    // Stable sort by the comparator. `sort_by` is stable, so rows equal under
    // the comparator keep the rotation order — the D1 load-balancing effect.
    rows.sort_by(|a, b| compare(sorts, a, b));
    // Walk: first valid + non-cooled row (spec §3 ④).
    let mut cooling_valid_min: Option<u64> = None;
    let mut any_valid = false;
    let mut routed_idx: Option<usize> = None;
    for (i, r) in rows.iter().enumerate() {
        if !r.valid {
            continue;
        }
        any_valid = true;
        if r.non_cooled {
            routed_idx = Some(i);
            break;
        }
        cooling_valid_min = Some(cooling_valid_min.map_or(r.remaining_secs, |m| m.min(r.remaining_secs)));
    }
    let new_cursor = if routed_idx.is_some() { (cursor + 1) % n } else { cursor };
    (routed_idx, cooling_valid_min, any_valid, new_cursor)
}

/// Select for the request hot path (spec §3 ③④⑤ + §6). Moves the routed row
/// out of `rows` (no clone) and advances the cursor — `select_strategy` calls
/// this and persists the new cursor.
///
/// North-compat (impl-spec §4.1): under both-ON + empty sort the comparator is
/// `(valid↓, non-cooled↓, idx↑)`. After rotating so the cursor row lands at
/// `idx=0`, the first valid+non-cooled row is the cursor row itself — the
/// round-robin next pick — and the cursor advances one, matching `pick_key`'s
/// `counters = idx+1` for the all-healthy case.
pub fn select_pure(rows: &mut Vec<Row>, sorts: &[SortKey], cursor: usize) -> (SelectResult, usize) {
    if rows.is_empty() {
        return (SelectResult::Empty, cursor);
    }
    let (routed_idx, cooling_min, any_valid, new_cursor) = sort_and_walk(rows, sorts, cursor);
    let result = match routed_idx {
        Some(i) => SelectResult::Routed(rows.remove(i)),
        None => {
            if any_valid {
                SelectResult::AllCooling { retry_after: cooling_min }
            } else {
                SelectResult::NoValid
            }
        }
    };
    (result, new_cursor)
}

/// Dry-run preview (impl-spec §8): sort + walk like `select_pure` but **without**
/// removing the routed row or advancing the persisted cursor — returns the
/// routed row's index so the caller can mark it in the candidate list it shows.
/// The returned `SelectResult::Routed` carries a clone (the row stays in `rows`).
pub fn select_preview(
    rows: &mut Vec<Row>,
    sorts: &[SortKey],
    cursor: usize,
) -> (SelectResult, Option<usize>, usize) {
    if rows.is_empty() {
        return (SelectResult::Empty, None, cursor);
    }
    let (routed_idx, cooling_min, any_valid, new_cursor) = sort_and_walk(rows, sorts, cursor);
    let result = match routed_idx {
        Some(i) => SelectResult::Routed(rows[i].clone()),
        None => {
            if any_valid {
                SelectResult::AllCooling { retry_after: cooling_min }
            } else {
                SelectResult::NoValid
            }
        }
    };
    (result, routed_idx, new_cursor)
}

// ---------------------------------------------------------------------------
// Tests — pure logic (comparator directions, inf convention, D1 round-robin,
// north-compat; no pool/config needed).
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn row(id: &str, valid: bool, non_cooled: bool, remaining: u64, idx: usize) -> Row {
        Row {
            provider_id: "p".into(),
            upstream_model: "m".into(),
            api_key_id: id.into(),
            key_secret: String::new(),
            group_id: "m".into(),
            valid,
            non_cooled,
            remaining_secs: remaining,
            success_rate: 1.0,
            rpm: 0,
            tpm: 0,
            avg_tftt_ms: 0,
            tps: 0.0,
            token_balance: None,
            bill_balance: None,
            price: 0.0,
            idx,
        }
    }

    #[test]
    fn empty_table_is_empty() {
        let mut rows = vec![];
        let (res, c) = select_pure(&mut rows, &[], 0);
        assert!(matches!(res, SelectResult::Empty));
        assert_eq!(c, 0);
    }

    #[test]
    fn no_valid_row_is_no_valid() {
        // All three keys quarantined (401/403). cursor doesn't move.
        let mut rows = vec![row("a", false, false, 0, 0), row("b", false, false, 0, 1)];
        let (res, c) = select_pure(&mut rows, &[], 0);
        assert!(matches!(res, SelectResult::NoValid));
        assert_eq!(c, 0, "cursor unchanged when nothing routed");
    }

    #[test]
    fn all_cooling_yields_retry_after_min() {
        // Two valid-but-cooling rows; the sooner (10s) wins Retry-After.
        let mut rows = vec![
            row("a", true, false, 30, 0),
            row("b", true, false, 10, 1),
        ];
        let (res, _c) = select_pure(&mut rows, &[], 0);
        match res {
            SelectResult::AllCooling { retry_after } => assert_eq!(retry_after, Some(10)),
            other => panic!("expected AllCooling, got {other:?}"),
        }
    }

    #[test]
    fn north_compat_round_robin_advances_cursor_one_per_pick() {
        // both-ON + empty sort, 3 healthy keys, cursor 0 → picks idx0, cursor→1.
        let mut rows = vec![row("a", true, true, 0, 0), row("b", true, true, 0, 1), row("c", true, true, 0, 2)];
        let (res, c) = select_pure(&mut rows, &[], 0);
        let routed = match res { SelectResult::Routed(r) => r, other => panic!("{other:?}") };
        assert_eq!(routed.api_key_id, "a", "cursor 0 → first key");
        assert_eq!(c, 1);
        // next pick continues round-robin
        let mut rows = vec![row("a", true, true, 0, 0), row("b", true, true, 0, 1), row("c", true, true, 0, 2)];
        let (res, c) = select_pure(&mut rows, &[], 1);
        let routed = match res { SelectResult::Routed(r) => r, other => panic!("{other:?}") };
        assert_eq!(routed.api_key_id, "b", "cursor 1 → second key");
        assert_eq!(c, 2);
    }

    #[test]
    fn cursor_wraps_around() {
        // cursor at the last position (1 of 2): picks b, then wraps to 0.
        let mut rows = vec![row("a", true, true, 0, 0), row("b", true, true, 0, 1)];
        let (res, c) = select_pure(&mut rows, &[], 1);
        let routed = match res { SelectResult::Routed(r) => r, other => panic!("{other:?}") };
        assert_eq!(routed.api_key_id, "b", "cursor 1 of 2 → key at position 1");
        assert_eq!(c, 0, "cursor wraps to 0");
    }

    #[test]
    fn valid_skips_cooling_and_advances_one() {
        // cursor 0, key a cooling → must skip to b (idx1) and advance cursor to 1.
        let mut rows = vec![row("a", true, false, 5, 0), row("b", true, true, 0, 1)];
        let (res, c) = select_pure(&mut rows, &[], 0);
        let routed = match res { SelectResult::Routed(r) => r, other => panic!("{other:?}") };
        assert_eq!(routed.api_key_id, "b");
        assert_eq!(c, 1);
    }

    #[test]
    fn invalid_key_never_routed_even_at_cursor() {
        // a is invalid (401), cursor 0 → skip to b.
        let mut rows = vec![row("a", false, false, 0, 0), row("b", true, true, 0, 1)];
        let (res, _c) = select_pure(&mut rows, &[], 0);
        let routed = match res { SelectResult::Routed(r) => r, other => panic!("{other:?}") };
        assert_eq!(routed.api_key_id, "b");
    }

    #[test]
    fn sort_i_price_asc_cheap_first() {
        // Two healthy keys: b cheaper. With empty sort cursor 0 → a; with [I]
        // price asc → b (cheaper wins, ties broken by idx).
        let mut rows = vec![
            { let mut r = row("a", true, true, 0, 0); r.price = 5.0; r },
            { let mut r = row("b", true, true, 0, 1); r.price = 1.0; r },
        ];
        let (res, _c) = select_pure(&mut rows, &[SortKey::I], 0);
        let routed = match res { SelectResult::Routed(r) => r, other => panic!("{other:?}") };
        assert_eq!(routed.api_key_id, "b", "price asc → cheapest (b) first");
    }

    #[test]
    fn sort_j_tps_desc_fastest_first() {
        let mut rows = vec![
            { let mut r = row("a", true, true, 0, 0); r.tps = 10.0; r },
            { let mut r = row("b", true, true, 0, 1); r.tps = 50.0; r },
        ];
        let (res, _c) = select_pure(&mut rows, &[SortKey::J], 0);
        let routed = match res { SelectResult::Routed(r) => r, other => panic!("{other:?}") };
        assert_eq!(routed.api_key_id, "b", "tps desc → fastest (b) first");
    }

    #[test]
    fn inf_balance_desc_ranks_none_first() {
        // E (token_balance desc): None = +∞ → ranks first (most balance).
        let mut rows = vec![
            { let mut r = row("a", true, true, 0, 0); r.token_balance = Some(100); r },
            { let mut r = row("b", true, true, 0, 1); r.token_balance = None; r }, // +∞
        ];
        let (res, _c) = select_pure(&mut rows, &[SortKey::E], 0);
        let routed = match res { SelectResult::Routed(r) => r, other => panic!("{other:?}") };
        assert_eq!(routed.api_key_id, "b", "None=∞ ranks first under desc");
    }

    #[test]
    fn inf_balance_asc_ranks_none_last() {
        // F (token_balance asc): None = +∞ → ranks last.
        let mut rows = vec![
            { let mut r = row("a", true, true, 0, 0); r.token_balance = None; r },    // +∞ last
            { let mut r = row("b", true, true, 0, 1); r.token_balance = Some(100); r },
        ];
        let (res, _c) = select_pure(&mut rows, &[SortKey::F], 0);
        let routed = match res { SelectResult::Routed(r) => r, other => panic!("{other:?}") };
        assert_eq!(routed.api_key_id, "b", "None=∞ ranks last under asc");
    }

    #[test]
    fn valid_leading_beats_user_sort() {
        // Even with price asc favoring a, a is invalid → b (valid) must win.
        let mut rows = vec![
            { let mut r = row("a", false, false, 0, 0); r.price = 1.0; r },
            { let mut r = row("b", true, true, 0, 1); r.price = 9.0; r },
        ];
        let (res, _c) = select_pure(&mut rows, &[SortKey::I], 0);
        let routed = match res { SelectResult::Routed(r) => r, other => panic!("{other:?}") };
        assert_eq!(routed.api_key_id, "b", "valid leading key outranks cheaper-but-dead");
    }

    // --- build_candidates: the four filter modes (spec §4) ---

    use crate::config::{ApiKey, Config, Filter, ManagedModel, ModelGroup, Provider, Strategy};
    use std::collections::BTreeMap;

    /// Two providers + one cross-provider model group, for filter-mode tests.
    /// `pa`: keys a1/a2, models gpt-4o & gpt-4o-mini, gpt-4o priced at 5.0.
    /// `pb`: key b1, model gpt-4o (no price entry → free/0).
    /// group `gpt-4o`: [(pa, gpt-4o), (pb, gpt-4o)].
    fn cfg_two_providers() -> Config {
        let key = |id: &str| ApiKey {
            id: id.into(),
            key: format!("sk-{id}"),
            label: id.into(),
            cooldown_secs: None,
            learned_cooldown: None,
        };
        let model = |id: &str| ManagedModel { id: id.into(), enabled: true };
        let mut pa_price = BTreeMap::new();
        pa_price.insert("gpt-4o".to_string(), 5.0);
        let pa = Provider {
            id: "pa".into(),
            name: "PA".into(),
            base_url: "http://pa/v1".into(),
            keys: vec![key("a1"), key("a2")],
            models: vec![model("gpt-4o"), model("gpt-4o-mini")],
            model_allowlist_only: false,
            aliases: BTreeMap::new(),
            protocol: String::new(),
            has_token_balance_api: false,
            has_bill_balance_api: false,
            price_table: pa_price,
        };
        let pb = Provider {
            id: "pb".into(),
            name: "PB".into(),
            base_url: "http://pb/v1".into(),
            keys: vec![key("b1")],
            models: vec![model("gpt-4o")],
            model_allowlist_only: false,
            aliases: BTreeMap::new(),
            protocol: String::new(),
            has_token_balance_api: false,
            has_bill_balance_api: false,
            price_table: BTreeMap::new(),
        };
        let groups = vec![ModelGroup {
            id: "gpt-4o".into(),
            entries: vec![("pa".into(), "gpt-4o".into()), ("pb".into(), "gpt-4o".into())],
        }];
        Config {
            providers: vec![pa, pb],
            model_groups: groups,
            ..Default::default()
        }
    }

    /// Sorted `(provider_id, api_key_id)` pairs from a candidate set — order
    /// independent so assertions aren't sensitive to config iteration.
    fn key_pairs(set: &CandidateSet) -> Vec<(String, String)> {
        let mut v: Vec<(String, String)> = set
            .rows
            .iter()
            .map(|r| (r.provider_id.clone(), r.api_key_id.clone()))
            .collect();
        v.sort();
        v
    }

    #[test]
    fn build_both_on_single_provider_keys() {
        // both ON (default) + "pa/gpt-4o" → pa's 2 keys, all upstream gpt-4o.
        let cfg = cfg_two_providers();
        let set = build_candidates(&cfg, "pa/gpt-4o").unwrap();
        assert_eq!(key_pairs(&set), vec![("pa".into(), "a1".into()), ("pa".into(), "a2".into())]);
        assert!(set.rows.iter().all(|r| r.upstream_model == "gpt-4o"));
        assert!(set.rows.iter().all(|r| r.price == 5.0), "price from pa's table");
        assert_eq!(set.cursor_key, "gpt-4o\x1fpa", "D3: (group, provider)");
    }

    #[test]
    fn build_model_first_spans_group() {
        // lock_provider OFF + bare "gpt-4o" → the whole group (pa + pb).
        let mut cfg = cfg_two_providers();
        cfg.strategy.filter = Filter { lock_model_group: true, lock_provider: false };
        let set = build_candidates(&cfg, "gpt-4o").unwrap();
        assert_eq!(
            key_pairs(&set),
            vec![("pa".into(), "a1".into()), ("pa".into(), "a2".into()), ("pb".into(), "b1".into())],
        );
        assert_eq!(set.cursor_key, "gpt-4o\x1fany", "model-first shares an 'any' cursor");
    }

    #[test]
    fn build_provider_first_all_models() {
        // lock_model_group OFF + lock_provider ON + "pa/gpt-4o" → every model
        // of pa (gpt-4o, gpt-4o-mini) × each key = 2 models × 2 keys = 4 rows.
        let mut cfg = cfg_two_providers();
        cfg.strategy.filter = Filter { lock_model_group: false, lock_provider: true };
        let set = build_candidates(&cfg, "pa/gpt-4o").unwrap();
        assert_eq!(set.rows.len(), 4);
        assert!(set.rows.iter().all(|r| r.provider_id == "pa"));
        let models: Vec<String> = {
            let mut m: Vec<String> = set.rows.iter().map(|r| r.upstream_model.clone()).collect();
            m.sort();
            m.dedup();
            m
        };
        assert_eq!(models, vec!["gpt-4o".to_string(), "gpt-4o-mini".to_string()]);
        assert_eq!(set.cursor_key, "gpt-4o\x1fpa");
    }

    #[test]
    fn build_available_first_whole_table() {
        // both OFF → every provider × every enabled model.
        let mut cfg = cfg_two_providers();
        cfg.strategy.filter = Filter { lock_model_group: false, lock_provider: false };
        let set = build_candidates(&cfg, "pa/gpt-4o").unwrap();
        // pa: 2 models × 2 keys = 4; pb: 1 model × 1 key = 1 → 5 rows.
        assert_eq!(set.rows.len(), 5);
        assert_eq!(set.cursor_key, "gpt-4o\x1fany");
        assert!(set.rows.iter().any(|r| r.provider_id == "pb"), "table spans providers");
    }

    #[test]
    fn build_bare_model_degrades_lock_provider_off() {
        // both-ON default + bare "gpt-4o" (no provider named) → lock_provider
        // auto-degrades OFF (spec §3 ①) → behaves like model-first.
        let cfg = cfg_two_providers(); // default both ON
        let set = build_candidates(&cfg, "gpt-4o").unwrap();
        assert_eq!(
            key_pairs(&set),
            vec![("pa".into(), "a1".into()), ("pa".into(), "a2".into()), ("pb".into(), "b1".into())],
        );
        assert_eq!(set.cursor_key, "gpt-4o\x1fany", "degraded to model-first cursor");
    }

    #[test]
    fn build_unknown_bare_model_errors() {
        // bare model with no group, no alias, no provider → resolve_model Err.
        let cfg = cfg_two_providers();
        assert!(build_candidates(&cfg, "no-such-model").is_err());
    }
}
