// End-to-end test of the example apps on the static website (ADR-2610081900): each runnable
// example's page shows it running with no error, a module's Run works, and every example page
// links its download. Run with tests/run-website-e2e.sh: BASE=<static server of website/> node tests/website-e2e.js
const { chromium } = require('playwright');
const B = process.env.BASE;
let pass = 0, fail = 0;
const ok = (c, m) => { if (c) { pass++; console.log('  ok  ', m); } else { fail++; console.log('  FAIL', m); } };
const RUNNABLE = ['adder', 'number-lab', 'unit-converter', 'image-lab', 'life', 'mandelbrot', 'text-tools'];
// Suites the website runs: the kernel with no server behind it, storage kept in the browser.
const SUITES = ['habit-tracker', 'loan-planner', 'meeting-notes', 'monte-carlo'];
const SERVER_ONLY = ['csv-explorer', 'focus-log', 'focus-timer', 'splunk-table', 'usl-lab'];

(async () => {
  const browser = await chromium.launch(require('./browser')({ headless: true }));
  const ctx = await browser.newContext({ viewport: { width: 1200, height: 900 } });
  for (const app of RUNNABLE) {
    const page = await ctx.newPage();
    const errors = [];
    page.on('pageerror', (e) => errors.push(e.message));
    page.on('console', (m) => m.type() === 'error' && errors.push(m.text()));
    await page.goto(`${B}/docs/examples/${app}/`);
    const frameEl = page.locator('iframe.try-frame');
    ok(await frameEl.count() === 1, `${app}: the page shows it running`);
    const frame = await (await frameEl.elementHandle()).contentFrame();
    await frame.waitForLoadState('load');
    await page.waitForTimeout(1500);
    const text = (await frame.locator('body').innerText()).trim();
    ok(text.length > 20 && !/did not load/.test(text), `${app}: the app drew something (${text.slice(0, 50).replace(/\s+/g, ' ')}…)`);
    if (app === 'adder') {
      const row = frame.locator('.fn').first();
      const inputs = row.locator('input');
      for (let i = 0; i < await inputs.count(); i++) await inputs.nth(i).fill(String(i + 2));
      await row.getByRole('button').click();
      ok(/^= \d+/.test(await row.locator('.out').textContent()), `adder: Run gives ${await row.locator('.out').textContent()}`);
    }
    ok(await page.locator(`a[href="/downloads/${app}.zip"]`).count() === 1, `${app}: the download is linked`);
    ok(errors.length === 0, `${app}: no errors ${errors.join(' | ')}`);
    await page.close();
  }
  for (const app of SUITES) {
    const page = await ctx.newPage();
    const errors = [];
    page.on('pageerror', (e) => errors.push(e.message));
    await page.goto(`${B}/docs/examples/${app}/`);
    const frameEl = page.locator('iframe.try-frame');
    ok(await frameEl.count() === 1, `${app}: the page shows the suite running`);
    const kernel = await (await frameEl.elementHandle()).contentFrame();
    await kernel.waitForLoadState('load');
    // Every part has started, and the kernel recorded no fault.
    let state = null;
    for (let i = 0; i < 40; i++) {
      state = await kernel.evaluate(() => (typeof Kernel === 'undefined' ? null : { boots: Kernel.started().length, parts: Kernel.apps().length, faults: Kernel.faults().map((f) => JSON.stringify(f)) }));
      if (state && state.parts > 0 && state.boots >= state.parts) break;
      await page.waitForTimeout(250);
    }
    ok(state && state.parts > 0 && state.boots >= state.parts, `${app}: every part started (${state && state.boots} of ${state && state.parts})`);
    ok(state && state.faults.length === 0, `${app}: no faults ${state ? state.faults.join(' | ') : 'no kernel'}`);
    ok(/stays in this browser/.test(await page.locator('section.try').innerText()), `${app}: says its data stays in the browser`);
    ok(errors.length === 0, `${app}: no page errors ${errors.join(' | ')}`);
    await page.close();
  }
  for (const app of SERVER_ONLY) {
    const page = await ctx.newPage();
    await page.goto(`${B}/docs/examples/${app}/`);
    ok(await page.locator('iframe.try-frame').count() === 0 && /needs? a Wardian/.test(await page.locator('section.try').innerText()), `${app}: says it needs a Wardian`);
    const res = await page.request.get(`${B}/downloads/${app}.zip`);
    ok(res.ok() && (await res.body()).slice(0, 2).toString() === 'PK', `${app}: the download is a zip`);
    await page.close();
  }
  await browser.close();
  console.log(`\n${pass} passed, ${fail} failed`);
  process.exit(fail ? 1 : 0);
})().catch((e) => { console.error(e); process.exit(1); });
