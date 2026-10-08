// A page app reaches only its own package (ADR-2610081003). Opens the hostile page
// tests/fixtures/rogue-page with a second, "outside" server, and fails if:
//   - any request reaches the outside server (counted there, so the page cannot lie about it);
//   - the page reads Wardian's API or another package's file;
//   - the page cannot load its own files, its .wasm, or /sdk/wardian.js.
// Run by tests/run-page-sandbox-e2e.sh, which sets BASE.
const http = require('http');
const { chromium } = require('playwright');
const launchOpts = require('./browser');

const BASE = process.env.BASE;
const failures = [];
const check = (ok, what) => { console.log(`${ok ? 'ok  ' : 'FAIL'}  ${what}`); if (!ok) failures.push(what); };

(async () => {
  // The outside server: answers everything, and remembers every request and upgrade.
  const hits = [];
  const outside = http.createServer((req, res) => { hits.push(`${req.method} ${req.url}`); res.setHeader('Access-Control-Allow-Origin', '*'); res.end('outside'); });
  outside.on('upgrade', (req, socket) => { hits.push(`UPGRADE ${req.url}`); socket.destroy(); });
  // A connection that does not speak HTTP, such as WebRTC to a TURN server.
  outside.on('clientError', (e, socket) => { hits.push('TCP (not HTTP)'); socket.destroy(); });
  await new Promise(r => outside.listen(0, '127.0.0.1', r));
  const OUT = `http://127.0.0.1:${outside.address().port}`;

  const browser = await chromium.launch(launchOpts());
  try {
    const page = await browser.newPage();
    await page.goto(`${BASE}/apps/rogue-page/index.html?outside=${encodeURIComponent(OUT)}`);
    await page.waitForFunction(() => document.title === 'done', null, { timeout: 60000 });
    await page.waitForTimeout(1500);   // late requests: the beacon, the prefetch, the CSS image
    const r = JSON.parse(await page.textContent('#results'));
    console.log(JSON.stringify(r, null, 1));

    for (const k of ['own fetch', 'own wasm', 'sdk loaded', 'progress element']) check(r[k] === 'reached', `the page can still use: ${k}`);
    for (const k of ['api fetch', 'other package']) check(String(r[k]).startsWith('blocked'), `the page cannot reach Wardian outside its package: ${k} (${r[k]})`);
    for (const k of Object.keys(r).filter(k => k.startsWith('outside')))
      check(!/^reached|^queued/.test(r[k]), `the page reports ${k} as not sent (${r[k]})`);
    check(hits.length === 0, `no request reached the outside server${hits.length ? ': ' + hits.join(', ') : ''}`);

    // ADR-2610081003 / security.md Page apps: no cookies and no storage, opened on its own...
    for (const k of ['cookie', 'localStorage', 'sessionStorage', 'indexedDB']) check(String(r[k]).startsWith('blocked'), `opened on its own, the page has no ${k} (${r[k]})`);
    // ...and inside Wardian's app list, where it also cannot reach the page around it.
    const listed = await browser.newPage();
    await listed.goto(BASE);
    await listed.click('#apps button:has-text("Rogue page")');
    let inner = null;
    for (let i = 0; i < 100 && !inner; i++) {
      inner = listed.frames().find(f => f.url().includes('/apps/rogue-page/'));
      if (!inner || await inner.evaluate(() => document.title).catch(() => '') !== 'done') { inner = null; await listed.waitForTimeout(100); }
    }
    const framed = inner ? JSON.parse(await inner.textContent('#results')) : {};
    console.log(JSON.stringify(framed));
    for (const k of ['cookie', 'localStorage', 'sessionStorage', 'indexedDB', 'parent.document']) check(String(framed[k]).startsWith('blocked'), `in Wardian's frame, the page has no ${k} (${framed[k]})`);
    // security.md Page apps: Wardian's frame uses the same sandbox list as the page's CSP header.
    const sandboxAttr = await listed.locator('iframe.appframe').getAttribute('sandbox');
    const csp = (await (await fetch(`${BASE}/apps/rogue-page/index.html`)).headers.get('content-security-policy')) || '';
    const cspSandbox = (csp.split(';').find(d => d.trim().startsWith('sandbox')) || '').trim().split(/\s+/).slice(1).sort().join(' ');
    check(cspSandbox !== '' && (sandboxAttr || '').split(/\s+/).sort().join(' ') === cspSandbox, `Wardian's frame has the CSP's sandbox list (frame "${sandboxAttr}", CSP "${cspSandbox}")`);
    await listed.close();

    // The routes a policy cannot close, which the security page says stay open for page apps and
    // suite parts alike. If one closes, this fails so the page is corrected: the docs must never
    // claim more, or less, than the browser does.
    const saw = p => hits.some(h => h.includes(p));
    const openRoutes = async (who, page, frameOf) => {
      hits.length = 0;
      const f = await frameOf();
      const popup = page.waitForEvent('popup', { timeout: 5000 }).catch(() => null);
      await f.click('body');
      await popup;
      await page.waitForTimeout(3000);   // ICE gathering opens the TURN connection
      await f.evaluate(() => window.leave()).catch(() => {});
      await page.waitForTimeout(1500);
      check(saw('/popup'), `still open in a ${who}, as the security page says: a pop-up after a click`);
      check(saw('/navigate'), `still open in a ${who}, as the security page says: navigating itself away`);
      check(saw('TCP (not HTTP)'), `still open in a ${who}, as the security page says: WebRTC to a TURN server`);
    };

    const open = await browser.newPage();
    await open.goto(`${BASE}/apps/rogue-page/index.html?open=1&outside=${encodeURIComponent(OUT)}`);
    await openRoutes('page app', open, async () => {
      await open.waitForFunction(() => document.title === 'open routes ready', null, { timeout: 30000 });
      return open.mainFrame();
    });

    require('fs').writeFileSync(require('path').join(process.env.APPS, 'rogue-suite-open', 'outside.txt'), OUT);
    const suite = await browser.newPage();
    await suite.goto(`${BASE}/run/rogue-suite-open/`);
    await openRoutes('suite part', suite, async () => {
      for (let i = 0; i < 100; i++) {
        for (const f of suite.frames()) if ((await f.textContent('#state').catch(() => '')) === 'open routes ready') return f;
        await suite.waitForTimeout(200);
      }
      throw new Error('the suite part never got ready');
    });
  } finally {
    await browser.close();
    outside.close();
  }
  if (failures.length) { console.log(`\nFAIL: ${failures.length} check(s)`); process.exit(1); }
  console.log('\nOK: the page app reaches only its own package');
})().catch(e => { console.error(e); process.exit(1); });
