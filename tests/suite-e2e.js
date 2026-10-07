// End-to-end test of the host kernel: the USL lab suite, then a hostile suite.
const { chromium } = require('playwright');
const B = process.env.BASE || 'http://127.0.0.1:8001';
let pass = 0, fail = 0;
const ok = (c, m) => { if (c) { pass++; console.log('  ok  ', m); } else { fail++; console.log('  FAIL', m); } };
const sleep = ms => new Promise(r => setTimeout(r, ms));
const frameOf = (page, app) => page.frames().find(f => f.url().endsWith('/' + app));

(async () => {
  const browser = await chromium.launch({ channel: 'chrome', headless: true });
  const ctx = await browser.newContext({ acceptDownloads: true, viewport: { width: 1360, height: 1000 } });
  const page = await ctx.newPage();
  const pageErrors = [];
  page.on('pageerror', e => pageErrors.push(e.message));

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

  ok(pageErrors.length === 0, 'no errors on the kernel page ' + pageErrors.join('; '));
  console.log(`\n${pass} passed, ${fail} failed`);
  await browser.close();
  process.exit(fail ? 1 : 0);
})().catch(e => { console.error(e); process.exit(2); });
