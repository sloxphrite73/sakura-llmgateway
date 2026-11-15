"""Long-context probe: send a big prompt to the local gateway and report timings.

Usage: python lc_probe.py <base_url> <model> <approx_tokens> [stream]
Builds a synthetic prompt of roughly <approx_tokens> tokens (CJK+ASCII mix),
streams the response, and prints ttfb / total / first-visible-byte / tokens.
"""
import json
import sys
import time
import urllib.request

BASE = sys.argv[1].rstrip("/")
MODEL = sys.argv[2]
WANT = int(sys.argv[3])
STREAM = (len(sys.argv) < 5) or sys.argv[5 - 1].lower() != "nostream"

# ~1.4 chars per token for this mixed text; pad to reach WANT tokens.
BLOCK = (
    "网关在长上下文下的行为需要逐项验证：上游 prefill 耗时、读超时边界、"
    "错误分类、token 记账、Key 轮换与冷却策略、以及客户端断开时的收尾路径。"
    "这段文字会被重复拼接以构造一个足够长的请求体，用来观察 ttfb 与总耗时。"
    "The quick brown fox jumps over the lazy dog. 0123456789 abcdefghij. "
)
REPS = max(1, int(WANT * 1.45 / len(BLOCK)))
prompt = BLOCK * REPS
print(f"prompt chars={len(prompt):,}  approx_tokens={WANT:,}  model={MODEL}  stream={STREAM}")

body = json.dumps(
    {
        "model": MODEL,
        "stream": STREAM,
        "max_tokens": 64,
        "messages": [{"role": "user", "content": prompt + "\n\n以上内容请用一个词概括。"}],
    }
).encode()
print(f"request bytes={len(body):,}")

req = urllib.request.Request(
    BASE + "/v1/chat/completions",
    data=body,
    headers={"Content-Type": "application/json"},
)
t0 = time.time()
try:
    with urllib.request.urlopen(req, timeout=1800) as r:
        ttfb = time.time() - t0
        print(f"http={r.status}  ttfb={ttfb*1000:.0f}ms")
        first = None
        usage = None
        chunks = 0
        buf = b""
        while True:
            data = r.read(65536)
            if not data:
                break
            chunks += 1
            if first is None:
                first = time.time() - t0
            buf += data
            for line in buf.split(b"\n"):
                if line.startswith(b"data:") and b'"usage"' in line:
                    usage = line.decode("utf-8", "replace")[:300]
            if len(buf) > 1 << 20:
                buf = buf[-4096:]
        total = time.time() - t0
        print(f"first_byte={0 if first is None else first*1000:.0f}ms  total={total*1000:.0f}ms  chunks={chunks}")
        if usage:
            print("usage_line=" + usage)
except urllib.error.HTTPError as e:
    detail = e.read().decode("utf-8", "replace")[:400]
    print(f"http={e.code}  elapsed={(time.time()-t0)*1000:.0f}ms")
    print("body=" + detail)
except Exception as e:
    print(f"EXC after {(time.time()-t0)*1000:.0f}ms: {e}")
