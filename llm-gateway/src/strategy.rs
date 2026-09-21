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
// build_candidates — §3 ①② (resolve + filter); both-ON for now (impl-spec §11.3)
// ---------------------------------------------------------------------------

/// Build the candidate row skeletons for a request (spec §3 ①②). Reads only
/// config — no pool state — so it is built once per request and re-walked each
/// attempt by `select_strategy` as keys cool.
///
/// **Step 3 (north-compat):** only the both-ON path is exercised — `resolve_model`
/// pins a single `(provider, model)`, and every row is one of that provider's
/// keys with the same `upstream_model`. This is exactly `pick_key`'s key set.
/// Cross-provider modes (model-first / provider-first / available-first) land
/// in Step 4 (impl-spec §11.4).
pub fn build_candidates(cfg: &Config, client_model: &str) -> Result<CandidateSet, String> {
    let (provider_id, upstream_model) = cfg.resolve_model(client_model)?;
    let provider = cfg
        .provider_by_id(&provider_id)
        .ok_or_else(|| format!("provider `{provider_id}` not found in config"))?;
    // group_id = the logical model. With no model_groups configured, the model
    // is a single-member group (impl-spec §9) and group_id = upstream_model.
    let group_id = upstream_model.clone();
    let price = provider.price_table.get(&group_id).copied().unwrap_or(0.0);
    let rows: Vec<Row> = provider
        .keys
        .iter()
        .map(|k| Row {
            provider_id: provider_id.clone(),
            upstream_model: upstream_model.clone(),
            api_key_id: k.id.clone(),
            key_secret: k.key.clone(),
            group_id: group_id.clone(),
            // runtime fields — filled by select_strategy; zeroed here.
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
        })
        .collect();
    // D3 cursor key = (group_id, resolved_provider_id). both-ON locks the
    // provider, so the second component is the provider id.
    let cursor_key = format!("{group_id}\x1f{provider_id}");
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
/// North-compat (impl-spec §4.1): under both-ON + empty sort the comparator is
/// `(valid↓, non-cooled↓, idx↑)`. After rotating so the cursor row lands at
/// `idx=0`, the first valid+non-cooled row is the cursor row itself — the
/// round-robin next pick — and the cursor advances one, matching `pick_key`'s
/// `counters = idx+1` for the all-healthy case.
pub fn select_pure(rows: &mut Vec<Row>, sorts: &[SortKey], cursor: usize) -> (SelectResult, usize) {
    if rows.is_empty() {
        return (SelectResult::Empty, cursor);
    }
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
    for (i, r) in rows.iter().enumerate() {
        if !r.valid {
            continue;
        }
        any_valid = true;
        if r.non_cooled {
            // Route this row. Advance the cursor one past the *cursor* position
            // (D1: `cursor = (cursor + 1) % n`), not past the selected row —
            // under a user sort the selected row may be far from the cursor, and
            // advancing one keeps equivalent keys load-balanced across requests.
            let row = rows.remove(i);
            let new_cursor = (cursor + 1) % n;
            return (SelectResult::Routed(row), new_cursor);
        }
        // A valid-but-cooling row: track the soonest recovery for §6 Retry-After.
        cooling_valid_min = Some(cooling_valid_min.map_or(r.remaining_secs, |m| m.min(r.remaining_secs)));
    }

    // No valid+non-cooled row was found.
    let result = if any_valid {
        // Valid rows exist but all are cooling → §6 AllCooling.
        SelectResult::AllCooling { retry_after: cooling_valid_min }
    } else {
        // No valid row at all (every candidate is 401/403-quarantined).
        SelectResult::NoValid
    };
    (result, cursor)
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
}
