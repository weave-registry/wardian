// End-to-end test of Arrange: each viewer's own layout of a suite, kept in this browser.
// Run with a Wardian serving apps/: BASE=http://127.0.0.1:PORT node tests/layout-e2e.js
const { chromium } = require('playwright');
const B = process.env.BASE;
let pass = 0, fail = 0;
const ok = (c, m) => { if (c) { pass++; console.log('  ok  ', m); } else { fail++; console.log('  FAIL', m); } };
const sleep = ms => new Promise(r => setTimeout(r, ms));
const order = (page, col) => page.$$eval('.' + col + ' > [data-arrange-panel]', ps => ps.map(p => p.getAttribute('data-arrange-panel') + (p.hasAttribute('data-arrange-hidden') ? '(hidden)' : '')));
const boots = page => page.evaluate(() => Kernel.trace().filter(t => t.kind === 'boot').length);

(async () => {
  const browser = await chromium.launch({ channel: 'chrome', headless: true });
  const page = await (await browser.newContext({ viewport: { width: 1360, height: 1000 } })).newPage();
  const errors = [];
  page.on('pageerror', e => errors.push(e.message));
  await page.goto(B + '/run/usl-lab/'); await sleep(3000);
  const main0 = await order(page, 'main');
  ok(main0.join() === 'chart,meaning,whatif,readouts,checks,diagnosis,export' && (await order(page, 'aside')).join() === 'inputs', 'the package layout by default: ' + main0.join());
  const b0 = await boots(page);

  console.log('== Arrange');
  await page.click('.w-arrange-btn');
  await sleep(400);   // the covers raise each panel's height; let the page settle before clicking in it
  ok(await page.locator('.w-arrange-bar').isVisible() && await page.locator('.w-arrange-cover').first().isVisible(), 'Arrange shows the bar and a cover on each panel');
  await page.locator('[data-arrange-panel=readouts] > .w-arrange-cover button[data-a=up]').click();
  ok((await order(page, 'main')).join() === 'chart,meaning,whatif,readouts,checks,diagnosis,export'.replace('whatif,readouts', 'readouts,whatif'), 'Move up');
  await page.locator('[data-arrange-panel=meaning] > .w-arrange-cover button[data-a=hide]').click();
  ok((await order(page, 'main')).includes('meaning(hidden)') && /Show Meaning/.test(await page.locator('.w-arrange-bar .hidden-list').textContent()), 'Hide, and the bar offers to show it again');
  await page.locator('[data-arrange-panel=checks] > .w-arrange-cover button[data-a=side]').click();
  ok((await order(page, 'aside')).join() === 'inputs,checks', 'To side panel');
  await page.locator('[data-arrange-panel=export] > .w-arrange-cover').dragTo(page.locator('[data-arrange-panel=diagnosis] > .w-arrange-cover'), { targetPosition: { x: 60, y: 6 } });
  const dragged = await order(page, 'main');
  ok(dragged.indexOf('export') === dragged.indexOf('diagnosis') - 1, 'drag and drop puts export above diagnosis: ' + dragged.join());
  await page.selectOption('.w-arrange-bar select', 'swap');
  ok(await page.locator('#suite.w-arrange-swap').count() === 1, 'side panel on the right');
  ok(await boots(page) === b0, 'no app restarted while moving (' + b0 + ' starts)');
  await page.click('.w-arrange-bar button.yes');
  ok(await page.locator('.w-arrange-bar').isHidden() && !(await page.locator('.w-arrange-cover').first().isVisible()), 'Done hides the covers');
  ok(await page.locator('[data-arrange-panel=meaning]').isHidden(), 'the hidden panel stays hidden');

  console.log('== kept for this viewer');
  await page.reload(); await sleep(3000);
  const saved = await order(page, 'main');
  ok(saved.join() === dragged.join() && (await order(page, 'aside')).join() === 'inputs,checks' && await page.locator('#suite.w-arrange-swap').count() === 1, 'the layout survives a reload');
  ok(await page.evaluate(() => Kernel.faults().length) === 0, 'no kernel faults with the new layout');

  console.log('== kept by the server: another browser sees it, and a browser\'s own copy moves over');
  // A fresh browser context has empty storage, like a new browser, profile or cleared site data.
  const fresh = await (await browser.newContext({ viewport: { width: 1360, height: 1000 } })).newPage();
  fresh.on('pageerror', e => errors.push(e.message));
  await fresh.goto(B + '/run/usl-lab/'); await sleep(3000);
  ok((await order(fresh, 'main')).join() === saved.join() && await fresh.locator('#suite.w-arrange-swap').count() === 1, 'an empty browser gets the same layout from the server');
  const served = await (await fetch(B + '/api/state/layout/usl-lab')).json();
  ok(served && served.columns && served.columns.aside.join() === 'inputs,checks', 'the server holds it: ' + JSON.stringify(served && served.columns));
  // A browser that arranged before the server kept layouts: its copy is uploaded once.
  const old = await browser.newContext({ viewport: { width: 1360, height: 1000 } });
  await old.addInitScript(() => { if (location.pathname.startsWith('/run/loan-planner')) localStorage.setItem('wardian-layout:loan-planner', JSON.stringify({ v: 1, mode: 'two', columns: { aside: ['inputs'], main: ['chart', 'summary', 'export'] }, hidden: ['export'] })); });
  const oldPage = await old.newPage();
  await oldPage.goto(B + '/run/loan-planner/'); await sleep(2500);
  ok(await oldPage.locator('[data-arrange-panel=export]').isHidden() && (await order(oldPage, 'main'))[0] === 'chart', 'the browser\'s old layout still applies');
  const moved = await (await fetch(B + '/api/state/layout/loan-planner')).json();
  ok(moved && moved.hidden && moved.hidden[0] === 'export', 'and is now on the server too');
  await old.close(); await fresh.close();
  ok(await page.frames().find(f => f.url().endsWith('/chart')).locator('#chart circle.pt').count() > 0, 'the apps still work: the chart has points');

  console.log('== Reset, and one column');
  await page.click('.w-arrange-btn');
  await page.selectOption('.w-arrange-bar select', 'one');
  ok(await page.locator('#suite.w-arrange-one').count() === 1, 'one column');
  await page.click('.w-arrange-bar button:has-text("Reset")');
  ok((await order(page, 'main')).join() === main0.join() && (await order(page, 'aside')).join() === 'inputs' && await page.locator('#suite.w-arrange-one, #suite.w-arrange-swap').count() === 0, 'Reset brings back the package layout');
  await page.keyboard.press('Escape');
  ok(await page.locator('.w-arrange-bar').isHidden(), 'Escape closes Arrange');

  console.log('== a suite with one panel');
  await page.goto(B + '/run/splunk-table/'); await sleep(2000);
  const box = await page.locator('iframe[title=table]').boundingBox();
  ok(box && box.height > 100, 'its panel has a height: ' + (box && box.height));

  console.log('== from the app list, Arrange is in view and works');
  // The app's frame is taller than the window, so a button in the frame's corner can sit below
  // the screen. The toolbar's Arrange button must be inside the window.
  await page.goto(B + '/'); await sleep(800);
  await page.locator('#apps li button', { hasText: /USL/i }).first().click();
  const tool = page.locator('#arrangeTool');
  await tool.waitFor({ state: 'visible', timeout: 6000 }).catch(() => {});
  const tb = await tool.boundingBox();
  ok(tb && tb.y >= 0 && tb.y + tb.height <= 1000, 'the suite\'s Arrange button is in the toolbar, inside the window: ' + JSON.stringify(tb));
  await tool.click();
  const suiteFrame = page.frames().find(f => f.url().endsWith('/run/usl-lab/'));
  await suiteFrame.locator('.w-arrange-bar').waitFor({ state: 'visible', timeout: 5000 }).catch(() => {});
  ok(await suiteFrame.locator('.w-arrange-bar').isVisible() && (await tool.textContent()) === 'Done arranging', 'pressing it opens Arrange in the app');
  ok(!(await suiteFrame.locator('.w-arrange-btn').isVisible()), 'the frame\'s own corner button is hidden inside the app list');
  await tool.click(); await sleep(300);
  ok(await suiteFrame.locator('.w-arrange-bar').isHidden(), 'pressing it again closes Arrange');
  await page.locator('#apps li button', { hasText: /mandelbrot/i }).first().click();
  await tool.waitFor({ state: 'visible', timeout: 6000 }).catch(() => {});
  ok(await page.locator('#arrangeTool').isVisible(), 'a page app with panels gets the toolbar button too');

  console.log('== a module app: Wardian draws its functions as cards, with Arrange');
  await page.goto(B + '/'); await sleep(800);
  await page.locator('#apps li button', { hasText: /adder/i }).first().click();
  await page.locator('.fn-card').first().waitFor({ timeout: 5000 });
  const fns = await page.$$eval('.fn-cards > [data-arrange-panel]', ps => ps.map(p => p.getAttribute('data-arrange-panel')));
  ok(fns.length >= 1 && await page.locator('.fn-card .w-button').count() === fns.length, 'one card per function, with library buttons: ' + fns.join());
  await page.locator('.fn-card .w-input').first().fill('2');
  if (await page.locator('.fn-card').first().locator('.w-input').count() > 1) await page.locator('.fn-card').first().locator('.w-input').nth(1).fill('3');
  await page.locator('.fn-card .w-button').first().click();
  ok((await page.locator('.fn-card output').first().textContent()).length > 0, 'Run still works: ' + await page.locator('.fn-card output').first().textContent());
  await page.locator('#runner .w-arrange-btn').click();
  await page.locator(`[data-arrange-panel="${fns[0]}"] > .w-arrange-cover button[data-a=hide]`).click();
  ok(await page.locator(`[data-arrange-panel="${fns[0]}"]`).isHidden(), 'a function can be hidden');
  await page.locator('#runner .w-arrange-bar button.yes').click();
  await page.reload(); await sleep(800);
  await page.locator('#apps li button', { hasText: /adder/i }).first().click();
  await page.locator('.fn-cards').waitFor({ state: 'attached', timeout: 5000 });
  await sleep(300);
  ok(await page.locator(`[data-arrange-panel="${fns[0]}"]`).isHidden(), 'and stays hidden after a reload');
  await page.locator('#apps li button').filter({ hasNotText: /adder/i }).first().click(); await sleep(500);
  ok(await page.locator('.w-arrange-bar, .w-arrange-btn').count() === 0, 'opening another app removes the module\'s Arrange');

  ok(errors.length === 0, 'no page errors ' + JSON.stringify(errors));
  await browser.close();
  console.log(`\n${pass} passed, ${fail} failed`);
  process.exit(fail ? 1 : 0);
})().catch(e => { console.error(e); process.exit(1); });
