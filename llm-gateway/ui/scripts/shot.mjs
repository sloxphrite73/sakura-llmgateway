// CDP screenshot tool: node scripts/shot.mjs <url> <outPng> [evalJs]
// Launches headless Edge with remote debugging, navigates, optionally
// evaluates JS (e.g. tab switch), waits 1.5s, captures viewport screenshot.
import { spawn } from 'node:child_process';
import { writeFileSync } from 'node:fs';
import WebSocket from 'ws';
import http from 'node:http';

const [url, out, evalJs] = process.argv.slice(2);
const EDGE = 'C:\\Program Files (x86)\\Microsoft\\Edge\\Application\\msedge.exe';
const PORT = 9333;

const edge = spawn(EDGE, [
  '--headless', '--disable-gpu', `--remote-debugging-port=${PORT}`,
  `--user-data-dir=${process.env.TEMP}\\edge-cdp-${Date.now()}`,
  '--window-size=1680,957', 'about:blank',
]);

const get = (path) => new Promise((res, rej) => http.get({ host: '127.0.0.1', port: PORT, path }, r => {
  let b = ''; r.on('data', c => b += c); r.on('end', () => res(JSON.parse(b)));
}).on('error', rej));

let version = null;
for (let i = 0; i < 30; i++) {
  try { version = await get('/json/version'); break; } catch { await new Promise(r => setTimeout(r, 500)); }
}
if (!version) { console.error('edge cdp failed'); process.exit(1); }

const ws = new WebSocket(version.webSocketDebuggerUrl);
let id = 0;
const pending = new Map();
ws.on('message', m => {
  const d = JSON.parse(m);
  if (d.id && pending.has(d.id)) { pending.get(d.id)(d); pending.delete(d.id); }
});
const send = (method, params, sessionId) => new Promise(res => {
  const i = ++id;
  pending.set(i, res);
  ws.send(JSON.stringify({ id: i, method, params, sessionId }));
});
await new Promise(r => ws.on('open', r));

const { result: { targetId } } = await send('Target.createTarget', { url: 'about:blank' });
const { result: { sessionId } } = await send('Target.attachToTarget', { targetId, flatten: true });
await send('Page.enable', {}, sessionId);
await send('Emulation.setDeviceMetricsOverride', { width: 1680, height: 957, deviceScaleFactor: 1, mobile: false }, sessionId);
await send('Page.navigate', { url }, sessionId);
await new Promise(r => setTimeout(r, 3500));
if (evalJs) {
  await send('Runtime.evaluate', { expression: evalJs }, sessionId);
  await new Promise(r => setTimeout(r, 1500));
}
const shot = await send('Page.captureScreenshot', { format: 'png' }, sessionId);
writeFileSync(out, Buffer.from(shot.result.data, 'base64'));
console.log('saved', out);
edge.kill();
process.exit(0);
