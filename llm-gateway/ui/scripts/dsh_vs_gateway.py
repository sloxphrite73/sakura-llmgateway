"""Compare DSH session usage (hourly) against the gateway's /api/stats histogram.

Usage: python scripts/dsh_vs_gateway.py <gateway.json> <sessions_dir> [hours]

Decompresses every DSH session log, buckets assistant-message usage by hour,
and prints DSH tokens vs gateway histogram tokens for the same hours. Also
reports which provider/model each DSH request went through, so a mismatch can be
traced to requests that never reached the gateway.
"""
import json
import os
import sys
import collections
import zstandard

gateway_cfg = sys.argv[1] if len(sys.argv) > 1 else r"L:\sakura\0.8.6ontest\gateway.json"
sessions_dir = sys.argv[2] if len(sys.argv) > 2 else r"C:\Users\Administrator\.dsh\sessions"
hours_back = int(sys.argv[3]) if len(sys.argv) > 3 else 72
# Only requests routed through the gateway provider are comparable: DSH's global
# counter also includes every other provider it talks to directly.
ONLY_PROVIDER = os.environ.get("ONLY_PROVIDER", "sakura")

# ---------- gateway side ----------
with open(gateway_cfg, "r", encoding="utf-8") as f:
    cfg = json.load(f)
stats = cfg.get("stats", {})
print("gateway total:", stats.get("total"))
gw_hourly = stats.get("hourly", {})  # {hour_index: {success, fail, tokens}}
gw_by_hour = {int(k): v for k, v in gw_hourly.items()}

# ---------- DSH side ----------
dsh_by_hour = collections.defaultdict(lambda: {"req": 0, "in": 0, "out": 0, "cache": 0, "total": 0})
by_provider = collections.Counter()
by_model = collections.Counter()
n_records = 0
dctx = zstandard.ZstdDecompressor()

for root, _dirs, files in os.walk(sessions_dir):
    for name in files:
        if not name.endswith(".jsonl.zstd"):
            continue
        path = os.path.join(root, name)
        try:
            with open(path, "rb") as fh:
                raw = dctx.stream_reader(fh).read()
        except Exception:
            continue
        for line in raw.split(b"\n"):
            if b'"usage"' not in line:
                continue
            try:
                rec = json.loads(line)
            except Exception:
                continue
            data = rec.get("data") or {}
            usage = data.get("usage")
            if not isinstance(usage, dict):
                continue
            ts = rec.get("time") or 0
            if not ts:
                continue
            src = (data.get("message") or {}).get("source") or {}
            provider = str(src.get("provider"))
            by_provider[provider] += 1
            by_model[str(src.get("model"))] += 1
            if ONLY_PROVIDER and provider != ONLY_PROVIDER:
                continue
            hour = ts // 3_600_000
            b = dsh_by_hour[hour]
            b["req"] += 1
            tin = int(usage.get("inputTokens") or 0)
            tout = int(usage.get("outputTokens") or 0)
            tcache = int(usage.get("cacheReadTokens") or 0)
            b["in"] += tin
            b["out"] += tout
            b["cache"] += tcache
            # DSH sometimes omits totalTokens; its definition is in+out+cache.
            b["total"] += int(usage.get("totalTokens") or (tin + tout + tcache))
            n_records += 1

print(f"\nDSH records: {n_records}")
print("by provider:", dict(by_provider))
print("by model:", dict(by_model.most_common(6)))

now_hour = max(list(dsh_by_hour.keys()) + [0])
print(f"\n{'hour':>6} {'DSH req':>8} {'DSH total':>12} {'DSH in':>11} {'DSH cache':>11} "
      f"{'GW req':>7} {'GW tokens':>11}  ratio")
all_hours = sorted(set(dsh_by_hour) | set(gw_by_hour))
for h in all_hours[-hours_back:]:
    d = dsh_by_hour.get(h, {"req": 0, "total": 0, "in": 0, "cache": 0})
    g = gw_by_hour.get(h, {"success": 0, "tokens": 0})
    ratio = (d["total"] / g["tokens"]) if g["tokens"] else 0
    if d["req"] == 0 and not g.get("tokens"):
        continue
    print(f"{h:>6} {d['req']:>8} {d['total']:>12} {d['in']:>11} {d['cache']:>11} "
          f"{g.get('success', 0):>7} {g.get('tokens', 0):>11}  {ratio:>5.1f}x")
