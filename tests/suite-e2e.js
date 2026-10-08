// End-to-end test of the host kernel: the USL lab suite, then a hostile suite.
const { chromium } = require('playwright');
const B = process.env.BASE || 'http://127.0.0.1:8001';
let pass = 0, fail = 0;
const ok = (c, m) => { if (c) { pass++; console.log('  ok  ', m); } else { fail++; console.log('  FAIL', m); } };
const sleep = ms => new Promise(r => setTimeout(r, ms));
const frameOf = (page, app) => page.frames().find(f => f.url().endsWith('/' + app));

(async () => {
  const browser = await chromium.launch(require('./browser')({ headless: true }));
  const ctx = await browser.newContext({ acceptDownloads: true, viewport: { width: 1360, height: 1000 } });
  const page = await ctx.newPage();
  const pageErrors = [];
  page.on('pageerror', e => pageErrors.push(e.message));

  console.log('== every part starts even when the saved layout is slow to arrive');
  // A part with no panel (the engine) is in the page at once, while the kernel waits for the layout,
  // so its hello can come before the kernel has learned its window. The kernel still hears it.
  {
    const slow = await ctx.newPage();
    await slow.route('**/api/state/layout**', async r => { await sleep(3000); await r.continue(); });
    await slow.goto(B + '/run/usl-lab/');
    let n = 0;
    for (let i = 0; i < 150 && n < 10; i++) { await sleep(100); n = await slow.evaluate(() => Kernel.started().length).catch(() => 0); }
    ok(n === 10, 'with the layout 3 s late, all 10 apps start: ' + n);
    await slow.close();
  }

  console.log('== USL lab boots');
  await page.goto(B + '/run/usl-lab/');
  await sleep(3500);
  const faults = await page.evaluate(() => Kernel.faults());
  ok(faults.length === 0, 'no kernel faults ' + JSON.stringify(faults));
  const boots = await page.evaluate(() => Kernel.trace().filter(t => t.kind === 'boot').map(t => t.app).sort());
  ok(boots.length === 10, '10 apps booted: ' + boots.join(','));
  let chart = frameOf(page, 'chart');
  ok(await chart.locator('#chart circle.pt').count() === 12, '12 points drawn');
  ok(await chart.locator('#chart circle.pt.flag').count() === 1, '1 point flagged off-curve');
  const sub = await chart.locator('#chartSub').textContent();
  ok(/background worker/.test(sub), 'engine runs in a worker: ' + sub);
  ok(await frameOf(page, 'readouts').locator('.row').count() > 0, 'readouts filled');
  ok(await frameOf(page, 'checks').locator('body').textContent().then(t => t.length > 50), 'checks filled');
  const diagH = await page.locator('iframe[title=diagnosis]').evaluate(el => el.offsetHeight);
  ok(diagH === 0, 'diagnosis hidden (no AI in this host), height ' + diagH);

  console.log('== what-if slider -> engine.curve -> chart');
  const wi = frameOf(page, 'whatif');
  await wi.locator('input[type=range]').first().evaluate(s => { s.value = 0.5; s.dispatchEvent(new Event('input', { bubbles: true })); });
  await sleep(800);
  ok(await chart.locator('#chart .c-wi').count() > 0, 'what-if curve drawn');

  console.log('== edit data -> new analysis -> stored -> survives reload');
  await frameOf(page, 'inputs').locator('#data').fill('1,1000\n2,1900\n4,3400\n8,5600\n16,7000\n32,6500');
  await sleep(1500);
  ok(await chart.locator('#chart circle.pt').count() === 6, '6 points after edit');
  const stored = await page.evaluate(() => localStorage.getItem('kernel:usl-lab:inputs') || '');
  ok(stored.includes('16,7000'), 'host stored the inputs');
  await page.reload(); await sleep(3500);
  chart = frameOf(page, 'chart');
  ok(await chart.locator('#chart circle.pt').count() === 6, '6 points after reload');
  // The app's data is kept by the server, so an empty browser gets it too.
  const kept = await (await fetch(B + '/api/state/apps/usl-lab')).json();
  ok(JSON.stringify(kept.inputs || {}).includes('16,7000'), 'the server keeps the inputs app\'s data');
  const other = await (await browser.newContext({ viewport: { width: 1360, height: 1000 } })).newPage();
  await other.goto(B + '/run/usl-lab/'); await sleep(3500);
  ok(await frameOf(other, 'chart').locator('#chart circle.pt').count() === 6, 'an empty browser shows the same 6 points');
  await other.close();

  console.log('== browser notices are not faults; real errors are, and repeats are counted');
  const chartF = frameOf(page, 'chart');
  const before = await page.evaluate(() => Kernel.faults().length);
  await chartF.evaluate(() => window.dispatchEvent(new ErrorEvent('error', { message: 'ResizeObserver loop completed with undelivered notifications.' })));
  await sleep(300);
  ok(await page.evaluate(() => Kernel.faults().length) === before, 'the ResizeObserver loop notice is not a fault');
  await chartF.evaluate(() => { for (let i = 0; i < 3; i++) window.dispatchEvent(new ErrorEvent('error', { message: 'test fault', error: new Error('test fault') })); });
  await sleep(300);
  ok(await page.evaluate(() => Kernel.faults().length) === before + 3, 'a real error is a fault, each time');
  ok(/chart: test fault ×3/.test(await page.locator('#faults').textContent()), 'the box shows it once, with a count: ' + await page.locator('#faults').textContent());
  await page.reload(); await sleep(3500);

  console.log('== export -> claude:downloads -> a real file');
  const exp = frameOf(page, 'export');
  // Chrome stops drawing a sandboxed frame while it is off screen, and Playwright waits for the
  // button to stop moving by watching it draw. Scroll the frame into view first, as a person would.
  await page.locator('iframe[title=export]').scrollIntoViewIfNeeded(); await sleep(300);
  const [dl] = await Promise.all([page.waitForEvent('download', { timeout: 5000 }), exp.locator('#btnCurve').click()]);
  ok(dl.suggestedFilename() === 'usl-fitted-curve.csv', 'download named ' + dl.suggestedFilename());
  const csv = require('fs').readFileSync(await dl.path(), 'utf8');
  ok(csv.split('\n').length > 10, 'csv has rows: ' + csv.split('\n')[0]);
  ok(await page.evaluate(() => Kernel.faults().length) === 0, 'still no faults');

  console.log('== hostile suite');
  await page.goto(B + '/run/rogue/'); await sleep(2500);
  const probe = frameOf(page, 'probe');
  const report = await probe.locator('#o').textContent();
  console.log(report.split('\n').map(l => '        ' + l).join('\n'));
  ok(!/ALLOWED/.test(report), 'probe was blocked everywhere');
  const rf = await page.evaluate(() => Kernel.faults().map(f => f.app + ': ' + f.message));
  ok(rf.some(f => /^liar: .*differs from suite.json/.test(f)), 'liar refused: contract differs');
  ok(rf.some(f => /^probe: may not emit "evil:topic"/.test(f)), 'direct emit refused by kernel');
  ok(rf.some(f => /^probe: did not declare capability "storage"/.test(f)), 'direct store refused by kernel');
  ok(await page.evaluate(() => localStorage.getItem('kernel:rogue:probe') === null && localStorage.getItem('kernel:rogue:liar') === null), 'nothing stored for rogue apps');
  // SPEC 6.7.6: any refusal is recorded as a fault, not only answered.
  for (const [re, what] of [
    [/^probe: may not call "liar\.anything"/, 'an undeclared call'],
    [/^probe: liar\.nothing is not available/, 'a call the callee does not provide in suite.json'],
    [/^probe: did not declare capability "claude:downloads"/, 'an undeclared cap'],
    [/^probe: capability downloads\.save is not granted/, 'a capop that is not granted'],
    [/^probe: did not declare capability "asset"/, 'an undeclared asset'],
    [/^probe: may not send on channel "evil"/, 'an undeclared channel send'],
    [/^probe: may not receive on channel "evil"/, 'an undeclared channel receive'],
  ]) ok(rf.some(f => re.test(f)), `a refusal is a fault: ${what}`);
  // C9 (ADR-2610081041): a topic named like an Object.prototype member is undeclared too.
  ok(/ctx\.emit toString: blocked/.test(report) && /ctx\.emit constructor: blocked/.test(report), 'the shim refuses ctx.emit("toString") and ("constructor")');
  ok(rf.some(f => /^probe: may not emit "toString"/.test(f)), 'the kernel refuses a direct emit of "toString", as a fault');
  ok(!(await page.evaluate(() => Kernel.trace().some(t => t.kind === 'emit' && (t.what === 'toString' || t.what === 'constructor')))), 'no "toString" emit in the trace');
  // ADR-2610071110: an unhandled promise rejection in an app is a fault.
  ok(rf.some(f => /^probe: probe unhandled rejection/.test(f)), 'an unhandled rejection is a fault');
  // SPEC 6.3: an inlined script may not hold </script, so the host refuses that app's frame.
  const closer = await page.evaluate(async () => { const r = await fetch('/frame/rogue/closer'); return {status: r.status, body: await r.text()}; });
  ok(closer.status !== 200 && /<\/script/i.test(closer.body) && !(await page.evaluate(() => Kernel.started().includes('closer'))), 'an app.js holding </script is refused, the app does not start: ' + closer.status + ' ' + closer.body.slice(0, 80));
  // SPEC 6.7.6: Kernel.apps() returns every contract, as in suite.json.
  const apps = await page.evaluate(() => Kernel.apps());
  const rogueJson = require('./fixtures/rogue/suite.json');
  ok(JSON.stringify(apps.map(a => a.name)) === JSON.stringify(rogueJson.apps.map(a => a.name))
    && JSON.stringify(apps.find(a => a.name === 'probe').needs) === JSON.stringify(rogueJson.apps[0].needs)
    && JSON.stringify(apps.find(a => a.name === 'late').provides) === '["ping","slow"]', 'Kernel.apps() returns every contract from suite.json');

  console.log('== who may talk to the kernel');
  await sleep(3000);   // the probe's forged answers (to 3.5 s) and late's real one (about 4 s)
  // security.md Contract / SPEC 6.8: the kernel knows the sender by its frame; other windows are ignored.
  const traceLen = await page.evaluate(() => Kernel.trace().length), faultLen = await page.evaluate(() => Kernel.faults().length);
  await page.evaluate(() => { window.postMessage({k: 'emit', topic: 'probe:data', payload: 'from the top page'}, '*'); window.postMessage({k: 'size', h: 1}, '*'); });
  await sleep(300);
  ok(await page.evaluate(() => Kernel.trace().length) === traceLen && await page.evaluate(() => Kernel.faults().length) === faultLen, 'a message posted by the top page is ignored: no trace entry, no fault');
  // SPEC 6.7.4: only the called app can answer. The probe forged an answer to every call id while
  // mute's call to late.slow was waiting; mute still got late's real answer.
  const mute = page.frames().find(f => f.url().endsWith('/mute'));
  const lateF = page.frames().find(f => f.url().endsWith('/late'));
  ok(await mute.evaluate(() => window.slowAnswer) === 'real', 'a forged result does not answer another app\'s call: ' + await mute.evaluate(() => window.slowAnswer));
  ok((await page.evaluate(() => Kernel.faults().map(f => f.app + ': ' + f.message))).some(f => /^probe: may not answer a call it was not asked/.test(f)), 'the forged result is a fault');
  // SPEC 6.5: payloads are copied. The probe set n = 2 right after it emitted {n: 1}.
  ok(JSON.stringify(await lateF.evaluate(() => window.gotData)) === '{"n":1}', 'a payload is copied: changing it after emit does not change what the listener got');
  // SPEC 6.7.2: a message goes only to apps that listen.
  ok(JSON.stringify(await mute.evaluate(() => window.msgs)) === '[]', 'an app that does not listen receives nothing');
  // ADR-2610071110: ctx.observe calls back at most once per animation frame, with no loop notice.
  const obs = await probe.evaluate(() => ({seen: window.observed, loops: window.loopErrors}));
  ok(obs.seen.length >= 5 && new Set(obs.seen).size === obs.seen.length && obs.loops === 0, 'ctx.observe calls back at most once per frame, with no ResizeObserver loop: ' + JSON.stringify(obs));

  console.log('== a call waits for a late app, and gives up after 15 s');
  // SPEC 6.7.5: a call to an app that has not finished starting waits for up to 15 seconds.
  await probe.locator('#t:not(:has-text("waiting"))').waitFor({ timeout: 25000 }).catch(() => {});
  const timing = JSON.parse(await probe.locator('#t').textContent().then(t => t === 'waiting' ? '{}' : t));
  ok(timing.ping && timing.ping.v === 'pong' && timing.ping.ms >= 800, 'a call to an app that provides 1 s later resolves: ' + JSON.stringify(timing.ping));
  ok(timing.never && /did not start in time/.test(timing.never.e) && timing.never.ms >= 14500 && timing.never.ms <= 17000, 'a call to an app that never provides rejects after about 15 s: ' + JSON.stringify(timing.never));

  console.log('== Kernel.trace keeps the last 400 events');
  // SPEC 6.7.6: Kernel.trace() returns the last 400 events.
  await probe.evaluate(() => { for (let i = 0; i < 450; i++) probeCtx.emit('probe:tick', i); });
  await sleep(500);
  const tr = await page.evaluate(() => Kernel.trace());
  ok(tr.length === 400 && tr.every(t => t.kind === 'emit' && t.what === 'probe:tick'), 'Kernel.trace() keeps 400 events, the newest: ' + tr.length);

  // The probe's own unhandled rejection is on purpose (tested above as a fault).
  const realErrors = pageErrors.filter(m => m !== 'probe unhandled rejection');
  ok(realErrors.length === 0, 'no errors on the kernel page ' + realErrors.join('; '));
  console.log(`\n${pass} passed, ${fail} failed`);
  await browser.close();
  process.exit(fail ? 1 : 0);
})().catch(e => { console.error(e); process.exit(2); });
