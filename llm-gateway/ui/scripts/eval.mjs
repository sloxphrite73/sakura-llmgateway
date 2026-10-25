// One-off CDP eval printer: node scripts/eval.mjs <url> <evalJs...> — prints last expression's JSON
import { spawn } from 'node:child_process';
import WebSocket from 'ws';
import http from 'node:http';

const [url, ...parts] = process.argv.slice(2);
const evalJs = parts.join(' ');
const EDGE = 'C:\\Program Files (x86)\\Microsoft\\Edge\\Application\\msedge.exe';
const PORT = 9335;
const edge = spawn(EDGE, ['--headless', '--disable-gpu', `--remote-debugging-port=${PORT}`,
  `--user-data-dir=${process.env.TEMP}\\edge-cdp-e-${Date.now()}`, '--window-size=1680,957', 'about:blank']);
const get = (path) => new Promise((res, rej) => http.get({ host: '127.0.0.1', port: PORT, path }, r => {
  let b = ''; r.on('data', c => b += c); r.on('end', () => res(JSON.parse(b)));
}).on('error', rej));
let version = null;
for (let i = 0; i < 30; i++) { try { version = await get('/json/version'); break; } catch { await new Promise(r => setTimeout(r, 500)); } }
const ws = new WebSocket(version.webSocketDebuggerUrl);
let id = 0; const pending = new Map();
ws.on('message', m => { const d = JSON.parse(m); if (d.id && pending.has(d.id)) { pending.get(d.id)(d); pending.delete(d.id); } });
const send = (method, params, sessionId) => new Promise(res => { const i = ++id; pending.set(i, res); ws.send(JSON.stringify({ id: i, method, params, sessionId })); });
await new Promise(r => ws.on('open', r));
const { result: { targetId } } = await send('Target.createTarget', { url: 'about:blank' });
const { result: { sessionId } } = await send('Target.attachToTarget', { targetId, flatten: true });
await send('Page.enable', {}, sessionId);
await send('Page.navigate', { url }, sessionId);
await new Promise(r => setTimeout(r, 3500));
const res = await send('Runtime.evaluate', { expression: evalJs, returnByValue: true }, sessionId);
console.log(JSON.stringify(res.result.result.value ?? res.result, null, 1));
edge.kill();
process.exit(0);
