// CDP measure tool: node scripts/measure.mjs <url> <evalJsBefore> <selector,...>
// Prints getBoundingClientRect for each selector (first match).
import { spawn } from 'node:child_process';
import WebSocket from 'ws';
import http from 'node:http';

const [url, evalJs, selectors] = process.argv.slice(2);
const EDGE = 'C:\\Program Files (x86)\\Microsoft\\Edge\\Application\\msedge.exe';
const PORT = 9334;

const edge = spawn(EDGE, ['--headless', '--disable-gpu', `--remote-debugging-port=${PORT}`,
  `--user-data-dir=${process.env.TEMP}\\edge-cdp-m-${Date.now()}`, '--window-size=1680,957', 'about:blank']);

const get = (path) => new Promise((res, rej) => http.get({ host: '127.0.0.1', port: PORT, path }, r => {
  let b = ''; r.on('data', c => b += c); r.on('end', () => res(JSON.parse(b)));
}).on('error', rej));

let version = null;
for (let i = 0; i < 30; i++) {
  try { version = await get('/json/version'); break; } catch { await new Promise(r => setTimeout(r, 500)); }
}
const ws = new WebSocket(version.webSocketDebuggerUrl);
let id = 0;
const pending = new Map();
ws.on('message', m => { const d = JSON.parse(m); if (d.id && pending.has(d.id)) { pending.get(d.id)(d); pending.delete(d.id); } });
const send = (method, params, sessionId) => new Promise(res => { const i = ++id; pending.set(i, res); ws.send(JSON.stringify({ id: i, method, params, sessionId })); });
await new Promise(r => ws.on('open', r));
const { result: { targetId } } = await send('Target.createTarget', { url: 'about:blank' });
const { result: { sessionId } } = await send('Target.attachToTarget', { targetId, flatten: true });
await send('Page.enable', {}, sessionId);
await send('Page.navigate', { url }, sessionId);
await new Promise(r => setTimeout(r, 3500));
if (evalJs) { await send('Runtime.evaluate', { expression: evalJs }, sessionId); await new Promise(r => setTimeout(r, 1500)); }
const x = await send('Runtime.evaluate', { expression: 'window.__x ? JSON.stringify(window.__x) : ""', returnByValue: true }, sessionId);
if (x.result.result.value) console.log(x.result.result.value);
const expr = `JSON.stringify(${JSON.stringify(selectors.split(','))}.map(s => { const el = document.querySelector(s); if (!el) return [s, null]; const r = el.getBoundingClientRect(); return [s, { l: Math.round(r.left), r: Math.round(r.right), w: Math.round(r.width) }]; }))`;
const res = await send('Runtime.evaluate', { expression: expr, returnByValue: true }, sessionId);
console.log(JSON.parse(res.result.result.value).map(([s, r]) => r ? `${s}  left=${r.l} right=${r.r} w=${r.w}` : `${s}  NOT FOUND`).join('\n'));
edge.kill();
process.exit(0);
