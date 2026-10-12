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

  console.log('== the header keeps to one line; the Menu is tidy (ADR-2610111760)');
  ok(await page.locator('#hdrPin').count() === 0, 'there is no Auto-hide button: Full window is the one way to hide everything');
  for (const [w, h] of [[1440, 900], [1280, 800], [1024, 800], [768, 900], [390, 844], [360, 740]]) {
    const v = await (await browser.newContext({ viewport: { width: w, height: h } })).newPage();
    v.on('pageerror', e => errors.push(`${w}px: ${e.message}`));
    await v.goto(B + '/'); await sleep(900);
    const r = await v.evaluate(() => {
      const hdr = document.querySelector('body > header');
      const ctl = [...hdr.querySelectorAll(':scope > a, :scope > button, .hdr-menu-wrap > button')].filter(n => n.offsetParent);
      const store = document.querySelector('#storeLink');
      return { tall: ctl.filter(n => n.getBoundingClientRect().height > 40).map(n => n.id || n.textContent.trim()),
        over: hdr.scrollWidth - hdr.clientWidth, wide: document.documentElement.scrollWidth - document.documentElement.clientWidth,
        apps: !!document.querySelector('#appsBtn').offsetParent, store: !!store.offsetParent && !store.closest('.hdr-menu'), hdrH: Math.round(hdr.getBoundingClientRect().height) };
    });
    ok(r.tall.length === 0 && r.over <= 1 && r.wide <= 0 && r.apps && r.hdrH <= 56, `${w}px: one slim line with Apps in it, nothing wraps, no sideways scroll ` + JSON.stringify(r));
    if (w >= 1024) ok(r.store, `${w}px: App store shows in the header`);
    await v.focus('#hdrMenuBtn'); await v.keyboard.press('Enter'); await sleep(200);
    const m = await v.evaluate(() => {
      const items = [...document.querySelectorAll('#hdrMenu a, #hdrMenu button')].filter(n => n.offsetParent);
      const src = document.querySelector('#badge');
      return { expanded: document.querySelector('#hdrMenuBtn').getAttribute('aria-expanded'), focus: document.activeElement.closest('#hdrMenu') !== null,
        store: !!document.querySelector('#storeLink').offsetParent, tall: items.filter(n => n.getBoundingClientRect().height > 40).map(n => n.textContent.trim()),
        names: items.map(n => n.textContent.trim()), source: src.textContent.trim(), sourceTitle: src.title, width: Math.round(document.querySelector('#hdrMenu').getBoundingClientRect().width) };
    });
    ok(m.expanded === 'true' && m.focus && m.store && m.tall.length === 0 && m.width <= 260 && m.names.some(n => /^Docs/.test(n)) && m.names.some(n => /^Refresh/.test(n)),
      `${w}px: Enter opens the Menu, focus inside, App store reachable, every item one line: ` + JSON.stringify(m.names));
    ok(!m.source.includes('/') && m.sourceTitle.length > m.source.length, `${w}px: the Menu names the source in short ("${m.source}"), the path in its tooltip`);
    await v.keyboard.press('Escape'); await sleep(150);
    ok(await v.getAttribute('#hdrMenuBtn', 'aria-expanded') === 'false' && await v.evaluate(() => document.activeElement.id) === 'hdrMenuBtn',
      `${w}px: Escape closes it and the focus is back on Menu`);
    await v.context().close();
  }

  console.log('== an open app gets the room; the list is a rail of apps (ADR-2610111760)');
  for (const [w, h] of [[1440, 900], [390, 844]]) {
    const ctx = await browser.newContext({ viewport: { width: w, height: h } });
    const v = await ctx.newPage();
    v.on('pageerror', e => errors.push(`app view ${w}px: ${e.message}`));
    await v.goto(B + '/'); await sleep(800);
    await v.locator('#apps li button', { hasText: /loan/i }).first().click(); await sleep(1200);
    const r = await v.evaluate(() => {
      const f = document.querySelector('#runner iframe.appframe').getBoundingClientRect();
      const side = document.querySelector('aside').getBoundingClientRect(), hdr = document.querySelector('body > header').getBoundingClientRect();
      return { frame: [f.x, f.y, f.width, f.height].map(Math.round), rail: Math.round(side.width), hdr: Math.round(hdr.height), folded: document.body.classList.contains('side-folded'),
        footer: !!document.querySelector('#appFooter').offsetParent, wide: document.documentElement.scrollWidth - innerWidth };
    });
    const [x, y, fw, fh] = r.frame;
    ok(r.folded && !r.footer && r.wide <= 0 && fw >= w - r.rail - 24 && y + fh <= h + 1 && fh >= h - r.hdr - (w < 701 ? 110 : 70),
      `${w}x${h}: with no setting changed, the app fills what the slim header and the rail leave: frame ${r.frame}, rail ${r.rail}, header ${r.hdr}`);
    if (w < 701) {
      ok(r.rail === 0, '390px: the list is hidden while an app is open');
      await v.click('#appsBtn'); await sleep(250);
      ok(await v.locator('#switcher').isVisible(), '390px: Apps opens the switcher');
      await v.keyboard.press('Escape');
    }
    await ctx.close();
  }
  await page.goto(B + '/'); await sleep(800);
  await page.locator('#apps li button', { hasText: /life/i }).first().click(); await sleep(900);
  const tileOf = (re) => page.locator('#apps button.app-open', { hasText: re }).first();
  ok(await page.evaluate(() => document.body.classList.contains('side-folded')) && await tileOf(/loan/i).isVisible()
    && (await tileOf(/loan/i).boundingBox()).width <= 44, 'the folded list shows every app as a tile');
  ok(await tileOf(/life/i).getAttribute('aria-current') === 'page', 'the open app is marked current');
  await tileOf(/loan/i).click(); await sleep(900);
  ok(/loan/i.test(await page.locator('#runner h2').textContent()) && await tileOf(/loan/i).getAttribute('aria-current') === 'page', 'one click on a tile switches apps');
  await tileOf(/loan/i).focus();
  const label = () => page.evaluate(() => document.activeElement.querySelector('.label')?.textContent || '');
  const before = await label();
  await page.keyboard.press('ArrowDown');
  const after = await label();
  await page.keyboard.press('Enter'); await sleep(900);
  ok(after && after !== before && (await page.locator('#runner h2').textContent()).trim() === after.trim(), `↓ moves to the next tile and Enter opens it (${before} → ${after})`);
  const contentW = () => page.evaluate(() => document.querySelector('#content').getBoundingClientRect().width);
  const w0 = await contentW();
  await page.click('#sideToggle'); await sleep(450);
  ok(await page.getAttribute('#sideToggle', 'aria-expanded') === 'true' && await page.locator('#filter').isVisible() && await contentW() < w0 - 100,
    'the toggle keeps the list open, with its names and filter');
  await page.reload(); await sleep(800);
  await page.locator('#apps li button', { hasText: /life/i }).first().click(); await sleep(800);
  ok(await page.getAttribute('#sideToggle', 'aria-expanded') === 'true' && !(await page.evaluate(() => document.body.classList.contains('side-folded'))), 'and stays open for the next app, after a reload');
  await page.click('#sideToggle'); await sleep(450);
  ok(await page.evaluate(() => document.body.classList.contains('side-folded')), 'the toggle folds it again');

  console.log('== the quick switcher: Ctrl+K (ADR-2610111760)');
  await page.evaluate(() => document.activeElement.blur());
  await page.keyboard.press('Control+k'); await sleep(250);
  ok(await page.locator('#switcher').isVisible() && await page.evaluate(() => document.activeElement.id) === 'switcherInput', 'Ctrl+K opens it, ready to type');
  const firstTwo = await page.$$eval('#switcherList li', lis => lis.slice(0, 2).map(l => l.textContent));
  ok(/life/i.test(firstTwo[0]) && /loan|mandel|meeting|number|monte|schedule/i.test(firstTwo[1] || ''), 'the apps opened last come first: ' + JSON.stringify(firstTwo));
  await page.keyboard.type('mandel'); await sleep(150);
  const hits = await page.$$eval('#switcherList li', lis => lis.map(l => l.textContent));
  ok(hits.length === 1 && /mandelbrot/i.test(hits[0]), 'typing filters by name: ' + JSON.stringify(hits));
  await page.keyboard.press('Enter'); await sleep(900);
  ok(!(await page.locator('#switcher').isVisible()) && /mandelbrot/i.test(await page.locator('#runner h2').textContent()), 'Enter opens it and closes the switcher');
  await page.keyboard.press('Control+k'); await sleep(200);
  await page.keyboard.press('ArrowDown'); await page.keyboard.press('ArrowDown');
  ok(await page.getAttribute('#switcherInput', 'aria-activedescendant') === 'sw-2', '↓ moves the choice');
  await page.keyboard.press('Escape'); await sleep(200);
  ok(!(await page.locator('#switcher').isVisible()) && /mandelbrot/i.test(await page.locator('#runner h2').textContent()), 'Escape closes it and changes nothing');
  await page.click('#appsBtn'); await sleep(200);
  ok(await page.locator('#switcher').isVisible(), 'the Apps button opens it too');
  await page.keyboard.press('Escape'); await sleep(150);

  console.log("== the app's head is one line; the rest is in ⋯ (ADR-2610111760)");
  for (const [w, h] of [[1440, 900], [1024, 800], [768, 900], [390, 844], [360, 740]]) {
    const ctx = await browser.newContext({ viewport: { width: w, height: h } });
    const v = await ctx.newPage();
    v.on('pageerror', e => errors.push(`head ${w}px: ${e.message}`));
    await v.goto(B + '/'); await sleep(700);
    await v.locator('#apps li button', { hasText: /loan/i }).first().click(); await sleep(1500);
    const r = await v.evaluate(() => {
      const row = document.querySelector('.apphead');
      const ctl = [...row.querySelectorAll('button, a, h2')].filter(n => n.offsetParent && !n.closest('#moreMenu'));
      return { h: Math.round(row.getBoundingClientRect().height), over: row.scrollWidth - row.clientWidth,
        tall: ctl.filter(n => n.getBoundingClientRect().height > 44).map(n => n.id || n.textContent.trim()),
        shown: ctl.map(n => n.id || n.tagName), wide: document.documentElement.scrollWidth - innerWidth };
    });
    ok(r.h <= 48 && r.over <= 1 && r.tall.length === 0 && r.wide <= 0 && r.shown.includes('moreBtn'), `${w}px: the head is one line, with ⋯, and does not scroll ` + JSON.stringify(r));
    if (w >= 1024) ok(r.shown.includes('arrangeTool') && r.shown.includes('changeBtn'), `${w}px: Arrange and Change this app stay in view`);
    await v.focus('#moreBtn'); await v.keyboard.press('Enter'); await sleep(200);
    const m = await v.evaluate(() => ({ expanded: document.querySelector('#moreBtn').getAttribute('aria-expanded'), focus: document.activeElement.closest('#moreMenu') !== null,
      items: [...document.querySelectorAll('#moreMenu a, #moreMenu button')].filter(n => n.offsetParent).map(n => n.id),
      tall: [...document.querySelectorAll('#moreMenu a, #moreMenu button')].filter(n => n.offsetParent && n.getBoundingClientRect().height > 40).length }));
    ok(m.expanded === 'true' && m.focus && m.tall === 0 && ['historyBtn', 'downloadBtn', 'saveWebBtn', 'openTabLink', 'removeBtn'].every(id => m.items.includes(id))
      && (m.items.includes('changeBtn') || r.shown.includes('changeBtn')), `${w}px: Enter opens ⋯ with the focus inside, every action one line; Change this app in the head or here: ` + m.items.join());
    await v.keyboard.press('ArrowDown');
    await v.keyboard.press('Escape'); await sleep(150);
    ok(await v.getAttribute('#moreBtn', 'aria-expanded') === 'false' && await v.evaluate(() => document.activeElement.id) === 'moreBtn', `${w}px: Escape closes ⋯ and the focus is back on it`);
    if (w === 1440) {
      ok(await v.getAttribute('#moreBtn', 'aria-label') === 'More actions for Loan planner' && await v.getAttribute('#moreBtn', 'aria-haspopup') === 'true', '⋯ is named for the app and says it opens a menu');
      const link = await v.$eval('#openTabLink', a => ({ href: a.getAttribute('href'), target: a.target, rel: a.rel }));
      ok(link.target === '_blank' && /noopener/.test(link.rel) && /\/run\/loan-planner\//.test(link.href), 'Open in a new tab opens the app on its own: ' + JSON.stringify(link));
      await v.click('#moreBtn'); await v.click('#removeBtn'); await sleep(200);
      ok(await v.locator('#moreMenu').isHidden() && /Remove loan-planner\?/.test(await v.locator('.removebar').textContent())
        && await v.evaluate(() => document.activeElement.getAttribute('aria-label')) === 'Remove loan-planner', 'Remove app… closes ⋯ and asks first, the focus on Remove');
      await v.locator('.removebar button', { hasText: 'Cancel' }).click(); await sleep(200);
      ok(await v.locator('.removebar button').count() === 0 && await v.evaluate(() => document.activeElement.id) === 'moreBtn'
        && /loan/i.test(await v.locator('#runner h2').textContent()), 'Cancel keeps the app and gives the focus back to ⋯');
      await v.click('#moreBtn'); await v.click('#historyBtn');
      await v.locator('#historyList').waitFor({ timeout: 5000 });
      ok(await v.locator('#historyList').isVisible(), 'History, from ⋯, opens the app\'s history');
      await v.locator('#apps li button', { hasText: /loan/i }).first().click(); await sleep(1200);
      await v.click('#moreBtn'); await v.click('#downloadBtn');
      await v.locator('#exportList strong').first().waitFor({ timeout: 5000 });
      ok(await v.locator('#exportList strong').first().isVisible(), 'Download, from ⋯, opens the download of the app');
      await v.locator('#apps li button', { hasText: /loan/i }).first().click(); await sleep(1200);
      await v.click('#moreBtn'); await v.click('#saveWebBtn'); await sleep(300);
      ok(await v.locator('#saveWebBar').isVisible(), 'Save as web page, from ⋯, offers to save it');
    }
    await ctx.close();
  }

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
      await fw.click('#hdrMenuBtn'); await fw.click('#fullBtn');
      const hb = () => fw.evaluate(() => document.querySelector('body > header').getBoundingClientRect().bottom);
      // The click leaves the pointer near the top edge, which rightly keeps the header out; once it
      // moves into the app the header goes. Wait for that rather than for a fixed time (CI is slower).
      await fw.mouse.move(700, 450);
      for (let i = 0; i < 30 && await hb() > 1; i++) await sleep(100);
      ok(await hb() <= 1 && await fw.getAttribute('#fullBtn', 'aria-pressed') === 'true', "the Menu's Full window enters it and the header waits above");
      await fw.mouse.move(700, 2); await sleep(500);
      ok(await hb() > 30 && near(await box()), 'the top edge brings the header back, over the app, which keeps its size');
      ok(await fw.locator('#appsBtn').isVisible(), 'the header that comes back has Apps in it');
      await fw.click('#appsBtn'); await sleep(250);
      await fw.keyboard.type('loan'); await fw.keyboard.press('Enter'); await sleep(1200);
      ok(/loan/i.test(await fw.locator('#runner h2').textContent()) && await fw.evaluate(() => document.body.classList.contains('full-window')) && near(await box()),
        'switching apps from there keeps Full window');
      await fw.mouse.move(700, 2); await sleep(500);
      await fw.mouse.move(700, 600, { steps: 12 }); await sleep(1100);   // down across the header, as a hand moves
      ok(await hb() <= 1, 'and it goes again when the pointer leaves');
      await fw.focus('#appsBtn'); await sleep(400);
      ok(await hb() > 30, 'Tab into the header shows it');
      await fw.evaluate(() => document.activeElement.blur()); await sleep(300);
      await fw.reload(); await sleep(800);
      await fw.locator('#apps li button', { hasText: /life/i }).first().click(); await sleep(1000);
      ok(await fw.evaluate(() => document.body.classList.contains('full-window')) && near(await box()), 'the choice is kept in this browser: the next app opens in Full window');
      await fw.keyboard.press('f'); await sleep(400);
      ok(!(await fw.evaluate(() => document.body.classList.contains('full-window'))), 'F leaves it again');
      await fw.goto(B + '/'); await sleep(700);
      ok(await fw.locator('#fullBtn').count() === 1 && await fw.evaluate(() => document.querySelector('#fullBtn').classList.contains('hidden')) && await fw.locator('#appFooter').isVisible(),
        'on the home screen Full window is not offered, and the version line shows');
    }
    await ctx.close();
  }

  ok(errors.length === 0, 'no page errors ' + JSON.stringify(errors));
  await browser.close();
  console.log(`\n${pass} passed, ${fail} failed`);
  process.exit(fail ? 1 : 0);
})().catch(e => { console.error(e); process.exit(1); });
