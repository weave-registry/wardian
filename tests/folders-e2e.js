// End-to-end test of folders in the app list (ADR-2610081830).
// Run with tests/run-folders-e2e.sh: BASE (examples and an app of the viewer's own, "my-sums") and
// BASE2 (only the examples). SHOTS=<folder> also saves screenshots of the app list.
const { chromium } = require('playwright');
const path = require('path');
const B = process.env.BASE, B2 = process.env.BASE2, SHOTS = process.env.SHOTS;
let pass = 0, fail = 0;
const ok = (c, m) => { if (c) { pass++; console.log('  ok  ', m); } else { fail++; console.log('  FAIL', m); } };
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const get = async (base, p) => (await fetch(base + p, { cache: 'no-store' })).json();
const post = async (base, p, body) => (await fetch(base + p, { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(body) })).json();
const kept = () => get(B, '/api/state/folders');
const byName = (rec, name) => rec.folders.find((f) => f.name === name);
const row = (name) => `li.app[data-app="${name}"]`;
const app = (name) => `#apps ${row(name)}`;
const folderLi = (page, name) => page.locator('#apps li.folder', { has: page.locator('.folder-name', { hasText: new RegExp(`^${name}$`) }) });
// Waits until the page's writes have reached the server.
const settled = (page) => page.waitForFunction(() => !foldersSaving, null, { timeout: 5000 });

(async () => {
  const browser = await chromium.launch(require('./browser')({ headless: true }));
  const ctx = await browser.newContext({ viewport: { width: 1280, height: 900 } });
  const page = await ctx.newPage();
  const errors = [];
  page.on('pageerror', (e) => errors.push(e.message));

  const list = await get(B, '/api/app-list');
  const examples = list.map((a) => a.name).filter((n) => n !== 'my-sums');
  ok(list.some((a) => a.name === 'my-sums') && examples.length >= 10, `A serves ${list.length} apps, one of them the viewer's own`);

  console.log('== the examples start in "Examples"');
  await page.goto(B + '/'); await sleep(800);
  const ex = folderLi(page, 'Examples');
  ok(await ex.count() === 1, 'A has a folder named Examples');
  ok(await ex.locator('.folder-toggle').getAttribute('aria-expanded') === 'false' && await ex.locator('ul.folder-apps').isHidden(), 'closed, because the viewer has an app of their own');
  ok(await ex.locator('.count').textContent() === String(examples.length), `its count says ${examples.length}`);
  ok(await page.locator('#apps > li.app').count() === 1 && await page.locator(`#apps > ${row('my-sums')}`).count() === 1, 'the viewer\'s own app is listed first, in no folder');
  let rec = await kept();
  ok(rec.seeded === true && byName(rec, 'Examples').apps.length === examples.length, 'the server keeps them filed, and seeded');
  const other = await (await browser.newContext()).newPage();
  await other.goto(B2 + '/'); await sleep(800);
  ok(await folderLi(other, 'Examples').locator('.folder-toggle').getAttribute('aria-expanded') === 'true', 'B, with only examples, starts with Examples open');
  ok(await other.locator('#apps > li.app').count() === 0, 'and nothing outside it');
  await other.context().close();

  console.log('== making, renaming and opening folders');
  await page.click('#newFolder');
  ok(await page.evaluate(() => document.activeElement.matches('input.folder-rename') && document.activeElement.value === 'New folder'), 'New folder makes one and puts its name up for editing');
  await page.keyboard.type('Work');
  await page.keyboard.press('Enter');
  await settled(page);
  ok(await folderLi(page, 'Work').count() === 1 && !!byName(await kept(), 'Work'), 'the name is kept: Work');
  ok(await page.evaluate(() => document.activeElement.matches('.folder-toggle')), 'the focus is on the folder');
  await folderLi(page, 'Work').locator('.folder-more').click();
  ok(await page.locator('.menu[role=menu]').isVisible(), 'the folder menu opens');
  await page.locator('.menu [role=menuitem]', { hasText: 'Rename' }).click();
  ok(await page.evaluate(() => document.activeElement.matches('input.folder-rename') && document.activeElement.selectionEnd - document.activeElement.selectionStart === 4), 'Rename… selects the name');
  await page.keyboard.type('Projects');
  await page.keyboard.press('Enter');
  await settled(page);
  ok(await folderLi(page, 'Projects').count() === 1 && !!byName(await kept(), 'Projects') && !byName(await kept(), 'Work'), 'renamed to Projects');
  await folderLi(page, 'Projects').locator('.folder-more').click();
  await page.locator('.menu [role=menuitem]', { hasText: 'Rename' }).click();
  await page.keyboard.type(' (not kept)');
  await page.keyboard.press('Escape');
  ok(await folderLi(page, 'Projects').count() === 1, 'Escape keeps the old name');

  console.log('== moving apps: menu, keyboard, drag');
  await page.locator(app('my-sums')).hover();
  await page.locator(`${app('my-sums')} .app-move`).click();
  ok(await page.locator(`${app('my-sums')} .app-move`).getAttribute('aria-expanded') === 'true', 'Move to… opens its menu');
  ok(await page.locator('.menu [role=menuitemradio][aria-checked=true]').textContent().then((t) => /No folder/.test(t)), 'the menu marks where the app is now');
  await page.locator('.menu [role=menuitemradio]', { hasText: 'Projects' }).click();
  await settled(page);
  ok(byName(await kept(), 'Projects').apps.includes('my-sums'), 'moved into Projects by menu');
  ok(await folderLi(page, 'Projects').locator(row('my-sums')).isVisible(), 'and shown there, the folder open');

  // The keyboard: Tab to the button, Enter, arrows, Enter; Escape closes.
  await page.focus(`${app('my-sums')} .app-open`);
  await page.keyboard.press('Tab');
  ok(await page.evaluate(() => document.activeElement.matches('.app-move')), 'Tab reaches Move to…');
  await page.keyboard.press('Enter');
  ok(await page.evaluate(() => document.activeElement.getAttribute('role') === 'menuitemradio' && document.activeElement.textContent.includes('Projects')), 'Enter opens the menu, the focus on the folder it is in');
  await page.keyboard.press('Escape');
  ok(await page.locator('.menu').count() === 0 && await page.evaluate(() => document.activeElement.matches('.app-move')), 'Escape closes it and the focus goes back');
  await page.keyboard.press('Enter');
  await page.keyboard.press('Home');
  await page.keyboard.press('Enter');
  await settled(page);
  ok(!byName(await kept(), 'Projects').apps.includes('my-sums') && await page.locator(`#apps > ${row('my-sums')}`).count() === 1, 'moved back to no folder by keyboard');

  await ex.locator('.folder-toggle').click();
  await settled(page);
  ok(byName(await kept(), 'Examples').open === true, 'opening Examples is kept');
  await page.locator(app('adder')).dragTo(folderLi(page, 'Projects').locator('.folder-row'));
  await settled(page);
  ok(byName(await kept(), 'Projects').apps.includes('adder') && !byName(await kept(), 'Examples').apps.includes('adder'), 'Adder dragged from Examples into Projects');
  await page.locator(`#apps li.app[data-app="mandelbrot"]`).hover();
  const zone = page.locator('#dropLoose');
  ok(await zone.isHidden(), 'the "no folder" drop target shows only while dragging');
  await page.locator(app('mandelbrot')).dragTo(page.locator(app('my-sums')));
  await settled(page);
  rec = await kept();
  ok(!rec.folders.some((f) => f.apps.includes('mandelbrot')), 'Mandelbrot dragged out of its folder, onto an app in no folder');
  await folderLi(page, 'Projects').locator('.folder-row').dragTo(ex.locator('.folder-row'), { targetPosition: { x: 20, y: 3 } });
  await settled(page);
  rec = await kept();
  ok(rec.folders.map((f) => f.name).join() === 'Projects,Examples', 'Projects dragged above Examples: ' + rec.folders.map((f) => f.name).join());
  await folderLi(page, 'Projects').locator('.folder-more').click();
  await page.locator('.menu [role=menuitem]', { hasText: 'Move down' }).click();
  await settled(page);
  ok((await kept()).folders.map((f) => f.name).join() === 'Examples,Projects', 'and back down with the keyboard path, Move down');

  console.log('== the open state survives a reload');
  await folderLi(page, 'Projects').locator('.folder-toggle').click();
  await settled(page);
  await page.reload(); await sleep(800);
  ok(await ex.locator('.folder-toggle').getAttribute('aria-expanded') === 'true' && await folderLi(page, 'Projects').locator('.folder-toggle').getAttribute('aria-expanded') === 'false', 'Examples open, Projects closed after a reload');

  if (SHOTS) {
    // Something to see: an app open, one in a closed folder.
    await page.locator(`${app('usl-lab')} .app-open`).click(); await sleep(600);
    for (const [w, h, label] of [[1280, 900, 'desktop'], [390, 844, 'phone']]) {
      for (const scheme of ['light', 'dark']) {
        await page.setViewportSize({ width: w, height: h });
        await page.emulateMedia({ colorScheme: scheme });
        await page.mouse.move(5, 5); await sleep(250);
        await page.screenshot({ path: path.join(SHOTS, `folders-${label}-${scheme}.png`), clip: { x: 0, y: 0, width: w, height: Math.min(h, 900) } });
      }
    }
    await page.setViewportSize({ width: 1280, height: 900 });
    await page.emulateMedia({ colorScheme: 'light' });
    await page.locator(`${app('life')} .app-open`).hover();
    await page.locator(`${app('life')} .app-move`).click(); await sleep(200);
    await page.screenshot({ path: path.join(SHOTS, 'folders-move-menu.png'), clip: { x: 0, y: 0, width: 640, height: 900 } });
    await page.keyboard.press('Escape');
    await page.emulateMedia({ colorScheme: 'dark' });
    await page.locator(`${app('life')} .app-move`).click(); await sleep(200);
    await page.screenshot({ path: path.join(SHOTS, 'folders-move-menu-dark.png'), clip: { x: 0, y: 0, width: 640, height: 900 } });
    await page.keyboard.press('Escape');
    await page.emulateMedia({ colorScheme: 'light' });
  }

  console.log('== the filter');
  await ex.locator('.folder-toggle').click();
  await settled(page);
  await page.fill('#filter', 'mandel');
  ok(await page.locator('#apps li.app:visible').count() === 1, 'only the matching app is shown');
  await page.fill('#filter', 'unit');
  ok(await ex.locator('.folder-toggle').getAttribute('aria-expanded') === 'true' && await ex.locator(row('unit-converter')).isVisible(), 'a folder holding a match opens while the filter has text');
  ok(await folderLi(page, 'Projects').count() === 0, 'a folder with no match is not shown');
  await sleep(300);
  ok(byName(await kept(), 'Examples').open === false, 'and that is not saved');
  await page.fill('#filter', '');
  ok(await ex.locator('.folder-toggle').getAttribute('aria-expanded') === 'false', 'clearing the filter closes it again');

  console.log('== deleting a folder keeps its apps');
  await folderLi(page, 'Projects').locator('.folder-more').click();
  await page.locator('.menu [role=menuitem]', { hasText: 'Delete' }).click();
  await settled(page);
  rec = await kept();
  ok(!byName(rec, 'Projects') && await page.locator(`#apps > ${row('adder')}`).count() === 1, 'Projects is gone; Adder is in no folder');
  ok((await get(B, '/api/app-list')).some((a) => a.name === 'adder'), 'and still an app');

  console.log('== a removed app leaves its folder');
  const removed = await post(B, '/api/apps/remove', { name: 'life' });
  ok(!removed.error, 'life removed to the trash');
  await page.reload(); await sleep(800);
  ok(await page.locator(app('life')).count() === 0 && await ex.locator('.count').textContent() === String(examples.length - 3), 'it is not in Examples any more');
  ok(!byName(await kept(), 'Examples').apps.includes('life'), 'and the server dropped its name');

  console.log('== limits the server keeps');
  const bad = await fetch(B + '/api/state/folders', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ folders: [{ id: 'x', name: 'x'.repeat(61), open: false, apps: [] }] }) });
  ok(bad.status === 400 && /1 to 60/.test((await bad.json()).error), 'a 61-character name is refused');
  const many = Array.from({ length: 101 }, (_, i) => ({ id: 'f' + i, name: 'F' + i, open: false, apps: [] }));
  ok((await post(B, '/api/state/folders', { folders: many })).error.includes('at most 100'), '101 folders are refused');

  ok(errors.length === 0, 'no page errors ' + JSON.stringify(errors));
  await browser.close();
  console.log(`\n${pass} passed, ${fail} failed`);
  process.exit(fail ? 1 : 0);
})().catch((e) => { console.error(e); process.exit(1); });
