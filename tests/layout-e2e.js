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
  const browser = await chromium.launch(require('./browser')({ headless: true }));
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

  console.log('== a suite of small parts: each panel is its own, with a height');
  await page.goto(B + '/run/splunk-table/'); await sleep(2000);
  const box = await page.locator('iframe[title=search]').boundingBox();
  ok(box && box.height > 100 && (await order(page, 'main')).join() === 'about,rows,keep', 'the Splunk table\'s search panel has a height: ' + (box && box.height));

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
  await page.locator('#apps li.app .app-open').filter({ hasNotText: /adder/i }).first().click(); await sleep(500);
  ok(await page.locator('.w-arrange-bar, .w-arrange-btn').count() === 0, 'opening another app removes the module\'s Arrange');

  console.log('== the header links to the app store (ADR-2610111740)');
  await page.goto(B + '/'); await sleep(800);
  const store = await page.$eval('#storeLink', a => ({ href: a.href, target: a.target, rel: a.rel, text: a.textContent.trim(), shown: a.getBoundingClientRect().width > 0 }));
  ok(store.href === 'https://wardian.studio/store/' && store.target === '_blank' && /noopener/.test(store.rel) && /^App store/.test(store.text) && store.shown,
    'App store opens wardian.studio/store/ in a new tab: ' + JSON.stringify(store));
  ok(await page.locator('#runner a[href="https://wardian.studio/store/"]', { hasText: 'Find apps in the store' }).count() === 1, 'the home card offers the store too');
  ok(/opens .*in a new tab/.test(await page.getAttribute('#storeLink', 'aria-label')), 'its name says it opens a new tab');

  console.log('== the header keeps to one line; what has no room goes into the Menu (ADR-2610111760)');
  for (const [w, h] of [[1440, 900], [1280, 800], [1024, 800], [768, 900], [390, 844], [360, 740]]) {
    const v = await (await browser.newContext({ viewport: { width: w, height: h } })).newPage();
    v.on('pageerror', e => errors.push(`${w}px: ${e.message}`));
    await v.goto(B + '/'); await sleep(900);
    const r = await v.evaluate(() => {
      const hdr = document.querySelector('body > header');
      const ctl = [...hdr.querySelectorAll(':scope > a, :scope > button, .hdr-menu-wrap > button')].filter(n => n.offsetParent);
      const store = document.querySelector('#storeLink');
      return { tall: ctl.filter(n => n.getBoundingClientRect().height > 44).map(n => n.id || n.textContent.trim()),
        over: hdr.scrollWidth - hdr.clientWidth, wide: document.documentElement.scrollWidth - document.documentElement.clientWidth,
        store: !!store.offsetParent && !store.closest('.hdr-menu'), inMenu: !!store.closest('.hdr-menu') };
    });
    ok(r.tall.length === 0 && r.over <= 1 && r.wide <= 0, `${w}px: no header control wraps, the header fits, no sideways scroll ` + JSON.stringify(r));
    if (w >= 1024) ok(r.store, `${w}px: App store shows in the header`);
    else {
      ok(r.store || r.inMenu, `${w}px: App store is in the header or the Menu (${r.store ? 'header' : 'Menu'})`);
      await v.focus('#hdrMenuBtn'); await v.keyboard.press('Enter'); await sleep(200);
      const open = await v.evaluate(() => ({ expanded: document.querySelector('#hdrMenuBtn').getAttribute('aria-expanded'),
        store: !!document.querySelector('#storeLink').offsetParent, focus: document.activeElement.closest('#hdrMenu') !== null }));
      ok(open.expanded === 'true' && open.store && open.focus, `${w}px: Enter opens the Menu, with the store in it and the focus inside ` + JSON.stringify(open));
      await v.keyboard.press('Escape'); await sleep(150);
      ok(await v.getAttribute('#hdrMenuBtn', 'aria-expanded') === 'false' && await v.evaluate(() => document.activeElement.id) === 'hdrMenuBtn',
        `${w}px: Escape closes it and the focus is back on Menu`);
    }
    await v.context().close();
  }

  console.log('== the app list folds to a rail (ADR-2610111760)');
  await page.goto(B + '/'); await sleep(800);
  await page.locator('#apps li button', { hasText: /life/i }).first().click(); await sleep(800);
  const contentW = () => page.evaluate(() => document.querySelector('#content').getBoundingClientRect().width);
  const w0 = await contentW();
  await page.click('#sideToggle'); await sleep(450);
  const w1 = await contentW();
  ok(await page.getAttribute('#sideToggle', 'aria-expanded') === 'false' && await page.getAttribute('#sideToggle', 'aria-label') === 'Expand the app list'
    && await page.locator('#apps').isHidden() && w1 > w0 + 150, `folding hides the list and gives the app the width: ${w0} → ${w1}px`);
  await page.reload(); await sleep(800);
  ok(await page.getAttribute('#sideToggle', 'aria-expanded') === 'false' && await page.locator('#apps').isHidden(), 'it stays folded after a reload');
  await page.click('#sideToggle'); await sleep(450);
  ok(await page.getAttribute('#sideToggle', 'aria-expanded') === 'true' && await page.locator('#apps').isVisible(), 'and opens again');

  console.log('== the header hides while you work, when asked to');
  const hdrTop = () => page.evaluate(() => document.querySelector('body > header').getBoundingClientRect().bottom);
  await page.goto(B + '/'); await sleep(800);
  ok(await page.getAttribute('#hdrPin', 'aria-pressed') === 'false' && await hdrTop() > 0, 'off at first: the header stays');
  await page.click('#hdrPin');
  ok(await page.getAttribute('#hdrPin', 'aria-pressed') === 'true', 'Auto-hide is pressed');
  await page.locator('#apps li button', { hasText: /life/i }).first().click(); await sleep(800);
  await page.mouse.click(700, 600); await sleep(500);
  ok(await hdrTop() <= 1, 'working in the app hides the header (bottom at ' + await hdrTop() + ')');
  const roomy = await page.evaluate(() => document.querySelector('main').getBoundingClientRect().height);
  ok(roomy >= 1000 - 60, 'and the app gets the room: ' + roomy + 'px of 1000');
  await page.mouse.move(700, 3); await sleep(500);
  ok(await hdrTop() > 20, 'pointing at the top edge brings it back');
  await page.mouse.move(700, 600); await sleep(1300);
  ok(await hdrTop() <= 1, 'and it goes again when the pointer leaves it');
  await page.focus('#settingsBtn'); await sleep(400);
  ok(await hdrTop() > 20, 'Tab into the header shows it');
  await page.reload(); await sleep(800);
  ok(await page.getAttribute('#hdrPin', 'aria-pressed') === 'true', 'the choice is kept in this browser');
  await page.click('#hdrPin');
  await page.mouse.click(700, 600); await sleep(500);
  ok(await hdrTop() > 20, 'off again: the header stays');

  console.log('== Full window: the open app fills the window (ADR-2610111760)');
  for (const [w, h] of [[1440, 900], [390, 844]]) {
    const ctx = await browser.newContext({ viewport: { width: w, height: h } });
    const fw = await ctx.newPage();
    fw.on('pageerror', e => errors.push(`full ${w}px: ${e.message}`));
    const open = async (name) => { await fw.goto(B + '/'); await sleep(800); await fw.locator('#apps li button', { hasText: name }).first().click(); await sleep(1200); };
    const box = () => fw.evaluate(() => { const r = document.querySelector('#runner iframe.appframe').getBoundingClientRect(); return [r.x, r.y, r.width, r.height].map(Math.round); });
    const near = (b) => Math.abs(b[0]) <= 2 && Math.abs(b[1]) <= 2 && Math.abs(b[2] - w) <= 2 && Math.abs(b[3] - h) <= 2;
    for (const name of [/loan/i, /life/i]) {
      await open(name);
      ok(await fw.locator('#appFooter').isHidden(), `${w}px ${name}: the version line takes no room while an app is open`);
      await fw.evaluate(() => document.activeElement.blur());
      await fw.keyboard.press('f'); await sleep(700);
      const b = await box();
      ok(await fw.evaluate(() => document.body.classList.contains('full-window')) && near(b)
        && await fw.evaluate(() => document.documentElement.scrollWidth - innerWidth) <= 0, `${w}px ${name}: F fills the window with the app: ${b} of ${w}x${h}`);
      await fw.keyboard.press('Escape'); await sleep(400);
      ok(!(await fw.evaluate(() => document.body.classList.contains('full-window'))) && (await box())[3] < h - 50, `${w}px ${name}: Escape leaves`);
    }
    if (w === 1440) {
      await fw.click('#fullBtn'); await sleep(700);
      const hb = () => fw.evaluate(() => document.querySelector('body > header').getBoundingClientRect().bottom);
      ok(await hb() <= 1 && await fw.getAttribute('#fullBtn', 'aria-pressed') === 'true', 'the button enters Full window and the header waits above');
      await fw.mouse.move(700, 2); await sleep(500);
      ok(await hb() > 30 && near(await box()), 'the top edge brings the header back, over the app, which keeps its size');
      await fw.mouse.move(700, 600, { steps: 12 }); await sleep(1100);   // down across the header, as a hand moves
      ok(await hb() <= 1, 'and it goes again when the pointer leaves');
      await fw.focus('#settingsBtn'); await sleep(400);
      ok(await hb() > 30, 'Tab into the header shows it');
      await fw.reload(); await sleep(800);
      await fw.locator('#apps li button', { hasText: /life/i }).first().click(); await sleep(1000);
      ok(await fw.evaluate(() => document.body.classList.contains('full-window')) && near(await box()), 'the choice is kept in this browser: the next app opens in Full window');
      await fw.focus('#fullBtn'); await fw.keyboard.press('Enter'); await sleep(400);
      ok(!(await fw.evaluate(() => document.body.classList.contains('full-window'))), 'the button, from the keyboard, leaves it again');
      await fw.goto(B + '/'); await sleep(700);
      ok(await fw.locator('#fullBtn').isHidden() && await fw.locator('#appFooter').isVisible(), 'on the home screen there is no Full window button, and the version line shows');
    }
    await ctx.close();
  }

  ok(errors.length === 0, 'no page errors ' + JSON.stringify(errors));
  await browser.close();
  console.log(`\n${pass} passed, ${fail} failed`);
  process.exit(fail ? 1 : 0);
})().catch(e => { console.error(e); process.exit(1); });
