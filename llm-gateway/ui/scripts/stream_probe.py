"""Mimic the console client: stream, note the upstream `usage`, then disconnect
the moment `data: [DONE]` arrives -- never giving the gateway a chance to see EOF.

Usage: python stream_probe.py <base_url> <model> [prompt]
"""
import json
import socket
import sys
import time

host_port = sys.argv[1].split("//", 1)[-1].split("/")[0]
host, port = host_port.split(":")
model = sys.argv[2]
prompt = sys.argv[3] if len(sys.argv) > 3 else "count from 1 to 5, no extra words"

body = json.dumps(
    {
        "model": model,
        "stream": True,
        "messages": [{"role": "user", "content": prompt}],
    }
).encode()

sock = socket.create_connection((host, int(port)), timeout=300)
sock.sendall(
    b"POST /v1/chat/completions HTTP/1.1\r\n"
    b"Host: " + host_port.encode() + b"\r\n"
    b"Content-Type: application/json\r\n"
    b"Accept: text/event-stream\r\n"
    b"Content-Length: " + str(len(body)).encode() + b"\r\n"
    b"Connection: close\r\n\r\n" + body
)

start = time.time()
buf = b""
usage = None
done_at = None
status = None
while True:
    chunk = sock.recv(65536)
    if not chunk:
        break
    buf += chunk
    if status is None and b"\r\n\r\n" in buf:
        head = buf.split(b"\r\n\r\n", 1)[0].decode("latin-1")
        status = head.split("\r\n", 1)[0]
        print(f"http: {status}  ttfb={(time.time()-start)*1000:.0f}ms")
    for line in buf.split(b"\n"):
        if line.startswith(b"data:") and b'"usage"' in line:
            usage = line.decode("utf-8", "replace").strip()
        if line.strip() == b"data: [DONE]":
            done_at = time.time() - start
    if done_at is not None:
        break

print(f"done_at={done_at*1000:.0f}ms" if done_at else "no [DONE]")
print(f"usage_line={usage}")
print("body_head=" + buf[:400].decode("utf-8", "replace").replace("\r\n", " | "))
# The interesting part: hard close straight after [DONE], like the desktop client.
sock.close()
print("closed immediately after [DONE]")
