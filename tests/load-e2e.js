// Load test (ADR-2610072033): no request may be left unanswered.
// 1. A browser loads the app list and a suite LOADS times each (default 200), and fails if any
//    request the page makes gets no answer within WAIT_MS.
// 2. Raw HTTP: bursts of keep-alive connections opened at once, idle connections that never send
//    a request, slow senders and bodies that are never read, while short requests must still be
//    answered.
// Run with a Wardian serving apps/usl-lab: BASE=http://127.0.0.1:PORT node tests/load-e2e.js
const { chromium } = require('playwright');
const net = require('net');
const B = process.env.BASE;
const LOADS = +(process.env.LOADS || 200);
const WAIT_MS = +(process.env.WAIT_MS || 10000);
const SUITE = process.env.SUITE || 'usl-lab';
let pass = 0, fail = 0;
const ok = (c, m) => { if (c) { pass++; console.log('  ok  ', m); } else { fail++; console.log('  FAIL', m); } };
const sleep = ms => new Promise(r => setTimeout(r, ms));
const { hostname: HOST, port: PORT } = new URL(B);

// Opens a connection; `send` writes raw bytes; `response()` resolves with the next complete
// response (status line + Content-Length body) or null after `ms`.
function conn() {
  return new Promise((resolve, reject) => {
    const s = net.connect(+PORT, HOST, () => {
      let buf = Buffer.alloc(0), waiter = null;
      const check = () => {
        if (!waiter) return;
        const at = buf.indexOf('\r\n\r\n');
        if (at < 0) return;
        const head = buf.slice(0, at).toString('latin1');
        const len = +((/content-length:\s*(\d+)/i.exec(head) || [])[1] || 0);
        if (buf.length < at + 4 + len) return;
        const status = +head.split(' ')[1];
        buf = buf.slice(at + 4 + len);
        const w = waiter; waiter = null; clearTimeout(w.t); w.resolve(status);
      };
      s.on('data', d => { buf = Buffer.concat([buf, d]); check(); });
      s.on('error', () => {});
      s.on('close', () => { if (waiter) { const w = waiter; waiter = null; clearTimeout(w.t); w.resolve(null); } });
      resolve({
        send: data => s.write(data),
        response: ms => new Promise(res => { waiter = { resolve: res, t: setTimeout(() => { waiter = null; res(null); }, ms) }; check(); }),
        close: () => s.destroy(),
      });
    });
    s.on('error', reject);
  });
}
const get = path => `GET ${path} HTTP/1.1\r\nHost: ${HOST}:${PORT}\r\nConnection: keep-alive\r\n\r\n`;

async function browserLoads() {
  console.log(`== a browser loads the app list and ${SUITE} ${LOADS} times`);
  const browser = await chromium.launch({ channel: 'chrome', headless: true });
  const pending = new Map();
  let requests = 0;
  const stuck = [];
  for (let i = 0; i < LOADS; i++) {
    // A new context each time is a browser with no open connections, like a new window or one
    // that has closed its idle ones: every load opens its connections at once.
    const ctx = await browser.newContext({ viewport: { width: 1360, height: 1000 } });
    const page = await ctx.newPage();
    page.on('request', r => { requests++; pending.set(r, Date.now()); });
    page.on('requestfinished', r => pending.delete(r));
    page.on('requestfailed', r => pending.delete(r));
    for (const url of [B + '/', B + '/run/' + SUITE + '/']) {
      await page.goto(url, { waitUntil: 'load', timeout: WAIT_MS }).catch(e => stuck.push(`${url}: ${e.message.split('\n')[0]}`));
      // Every request the page started (frames, scripts, app files, API calls) must finish. Pages
      // keep asking for a moment after "load", so wait for the network to stay quiet for 300 ms;
      // leaving earlier would abort their requests, which is not the server's doing.
      let quietSince = Date.now();
      while (Date.now() - quietSince < 300) {
        await sleep(25);
        if (pending.size) quietSince = Date.now();
        const old = [...pending.entries()].filter(([, t]) => Date.now() - t >= WAIT_MS);
        for (const [r] of old) { stuck.push(`load ${i + 1} of ${url}: no answer for ${r.method()} ${r.url()}`); pending.delete(r); }
      }
    }
    await ctx.close();
    pending.clear();
    if ((i + 1) % 50 === 0) console.log(`  ${i + 1} loads, ${requests} requests`);
  }
  await browser.close();
  ok(stuck.length === 0, `every one of ${requests} browser requests answered` + (stuck.length ? ':\n    ' + stuck.slice(0, 10).join('\n    ') : ''));
}

async function burst(n, path) {
  const cs = await Promise.all(Array.from({ length: n }, conn));
  const out = await Promise.all(cs.map(c => { c.send(get(path)); return c.response(WAIT_MS); }));
  cs.forEach(c => c.close());
  return out.filter(s => s !== 200).length;
}

async function raw() {
  console.log('== raw HTTP: bursts of keep-alive connections');
  let lost = 0;
  for (let round = 0; round < 300; round++) lost += await burst(round % 2 ? 8 : 16, round % 3 ? '/api/status' : '/apps/' + SUITE + '/suite.json');
  ok(lost === 0, `bursts of 8 and 16 connections, 300 rounds: ${lost} requests unanswered`);

  console.log('== raw HTTP: idle, slow and unread connections do not hold up others');
  // Connections a browser opens ahead of time and leaves idle, many more than any worker pool.
  const idle = await Promise.all(Array.from({ length: 64 }, conn));
  // A sender that stops halfway through its headers.
  const slow = await conn();
  slow.send(`GET /api/status HTTP/1.1\r\nHost: ${HOST}`);
  // A POST whose body is announced but never sent; the server answers 403/400 without it or waits.
  const unread = await conn();
  unread.send(`POST /api/refresh HTTP/1.1\r\nHost: ${HOST}:${PORT}\r\nContent-Type: application/json\r\nContent-Length: 50000\r\n\r\n{"x":`);
  let answered = 0;
  for (let i = 0; i < 50; i++) answered += (await burst(4, '/api/status')) === 0 ? 1 : 0;
  ok(answered === 50, `with 66 stalled connections open, ${answered} of 50 bursts fully answered`);
  // Keep-alive: the same connection answers many requests in a row.
  const ka = await conn();
  let inRow = 0;
  for (let i = 0; i < 200; i++) { ka.send(get(i % 2 ? '/api/status' : '/logo.svg')); if ((await ka.response(WAIT_MS)) === 200) inRow++; }
  ok(inRow === 200, `one keep-alive connection answers ${inRow} of 200 requests in a row`);
  // The idle connections still work when they finally ask.
  const late = await Promise.all(idle.slice(0, 16).map(c => { c.send(get('/api/status')); return c.response(WAIT_MS); }));
  ok(late.every(s => s === 200), `connections idle for a while are still answered (${late.filter(s => s === 200).length} of 16)`);
  [...idle, slow, unread, ka].forEach(c => c.close());
}

(async () => {
  if (process.env.SKIP_RAW !== '1') await raw();
  if (process.env.SKIP_BROWSER !== '1') await browserLoads();
  console.log(`\n${pass} passed, ${fail} failed`);
  process.exit(fail ? 1 : 0);
})().catch(e => { console.error(e); process.exit(1); });
