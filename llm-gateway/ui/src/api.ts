/* api helper — same semantics as the single-file console's api()/apiText(). */

export async function api<T = any>(method: string, url: string, body?: unknown): Promise<T> {
  const r = await fetch(url, {
    method,
    headers: { 'Content-Type': 'application/json' },
    body: body !== undefined ? JSON.stringify(body) : undefined,
  });
  if (!r.ok) {
    let msg = '' + r.status;
    try {
      const e = await r.json();
      if (e && e.error) msg = e.error;
    } catch (_) { /* keep status */ }
    throw new Error(msg);
  }
  const txt = await r.text();
  return (txt ? JSON.parse(txt) : null) as T;
}

/** Raw-text variant for change-detected polling (stats 1s refresh): lets the
    caller compare payloads byte-for-byte and skip the render when unchanged. */
export async function apiText(method: string, url: string): Promise<string> {
  const r = await fetch(url, { method });
  if (!r.ok) throw new Error('' + r.status);
  return r.text();
}

export function copyText(s: string): void {
  if (navigator.clipboard && navigator.clipboard.writeText) {
    navigator.clipboard.writeText(s).catch(() => {});
  } else {
    const ta = document.createElement('textarea');
    ta.value = s;
    document.body.appendChild(ta);
    ta.select();
    try { document.execCommand('copy'); } catch (e) { /* ignore */ }
    ta.remove();
  }
}
