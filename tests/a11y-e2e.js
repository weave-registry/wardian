// End-to-end test of first use (ADR-2610072033): the first-run setup, and that every control in the
// app list, Settings, Arrange and History has an accessible name and is reached by the keyboard.
// Run with tests/run-a11y-e2e.sh: BASE=http://127.0.0.1:PORT DATA=<its data dir> node tests/a11y-e2e.js
// The names are the ones Chrome computes for screen readers (the accessibility tree, over CDP).
const { chromium } = require('playwright');
const fs = require('fs'), path = require('path');
const B = process.env.BASE;
let pass = 0, fail = 0;
const ok = (c, m) => { if (c) { pass++; console.log('  ok  ', m); } else { fail++; console.log('  FAIL', m); } };
const sleep = ms => new Promise(r => setTimeout(r, ms));

// Every control Chrome exposes in the page (not inside app frames) that has no name.
const CONTROL_ROLES = new Set(['button', 'link', 'textbox', 'searchbox', 'checkbox', 'radio', 'combobox', 'listbox', 'tab', 'switch', 'slider', 'spinbutton', 'menuitem', 'menuitemradio', 'PopUpButton', 'DisclosureTriangle']);
async function names(page, scope) {
  const cdp = await page.context().newCDPSession(page);
  const { nodes } = await cdp.send('Accessibility.getFullAXTree');
  const controls = [], unnamed = [];
  for (const n of nodes) {
    if (n.ignored || !n.role || !CONTROL_ROLES.has(n.role.value) || n.backendDOMNodeId === undefined) continue;
    // Only controls inside `scope` (a CSS selector), when given.
    if (scope) {
      const { object } = await cdp.send('DOM.resolveNode', { backendNodeId: n.backendDOMNodeId });
      const { result } = await cdp.send('Runtime.callFunctionOn', { objectId: object.objectId, functionDeclaration: `function(s){ return !!this.closest(s); }`, arguments: [{ value: scope }], returnByValue: true });
      if (!result.value) continue;
    }
    const name = (n.name && n.name.value || '').trim();
    controls.push(n.role.value + ' "' + name + '"');
    if (!name) {
      const { node } = await cdp.send('DOM.describeNode', { backendNodeId: n.backendDOMNodeId });
      unnamed.push(`${n.role.value} <${node.localName} ${(node.attributes || []).join(' ')}>`);
    }
  }
  await cdp.detach();
  return { controls, unnamed };
}
async function checkNames(page, scope, what, min = 1) {
  const { controls, unnamed } = await names(page, scope);
  ok(controls.length >= min && unnamed.length === 0, `${what}: ${controls.length} controls, all named` + (unnamed.length ? ' — unnamed: ' + unnamed.join('; ') : ''));
  return controls;
}

// Presses Tab until every visible control in `scope` has had the focus. Radios of one group and the
// tabs of one tab list count as reached when one of them is (arrow keys move inside them).
async function checkTabOrder(page, scope, what, opts = {}) {
  const want = await page.evaluate(([scope]) => {
    const vis = (n) => n.getClientRects().length && getComputedStyle(n).visibility !== 'hidden' && !n.closest('[hidden], .hidden, [inert]');
    const list = [...document.querySelectorAll(scope)].flatMap((root) => [...root.querySelectorAll('button, a[href], input:not([type=hidden]), select, textarea, summary, [tabindex]:not([tabindex="-1"])')])
      .filter((n) => !n.disabled && vis(n));
    let i = 0;
    for (const n of list) {
      n.dataset.a11y = String(i++);
      const group = n.type === 'radio' ? 'radio:' + n.name : n.getAttribute('role') === 'tab' ? 'tabs:' + (n.closest('[role=tablist]') ? [...document.querySelectorAll('[role=tablist]')].indexOf(n.closest('[role=tablist]')) : '') : '';
      n.dataset.a11yGroup = group || 'own:' + n.dataset.a11y;
    }
    return list.map((n) => ({ key: n.dataset.a11yGroup, label: (n.getAttribute('aria-label') || n.textContent || n.id || n.tagName).trim().slice(0, 40) }));
  }, [scope]);
  const keys = new Map(want.map((w) => [w.key, w.label]));
  const seen = new Set();
  // Start at the given element, or at the first control of the view. An app's frame in the way
  // holds controls of its own, so allow presses enough to go through it.
  if (opts.start) await page.focus(opts.start);
  else { await page.focus('[data-a11y="0"]'); seen.add(want[0].key); }
  for (let i = 0; i < keys.size * 2 + 150 && seen.size < keys.size; i++) {
    await page.keyboard.press('Tab');
    const k = await page.evaluate(() => document.activeElement && document.activeElement.dataset ? document.activeElement.dataset.a11yGroup || '' : '');
    if (k) seen.add(k);
    if (opts.inside) ok_inside.push(await page.evaluate((s) => !!document.activeElement.closest(s), opts.inside));
  }
  const missed = [...keys].filter(([k]) => !seen.has(k)).map(([, l]) => l);
  ok(keys.size > 0 && missed.length === 0, `${what}: Tab reaches all ${keys.size} controls` + (missed.length ? ' — missed: ' + missed.join(', ') : ''));
  await page.evaluate(() => document.querySelectorAll('[data-a11y]').forEach((n) => { delete n.dataset.a11y; delete n.dataset.a11yGroup; }));
}
let ok_inside = [];

// Moves the focus with Tab until `selector` has it, then presses `key` (Enter by default).
async function keyboardPress(page, selector, key = 'Enter') {
  for (let i = 0; i < 200; i++) {
    if (await page.evaluate((s) => document.activeElement && document.activeElement.matches(s), selector)) { await page.keyboard.press(key); return true; }
    await page.keyboard.press('Tab');
  }
  return false;
}

const api = async (p, body) => {
  const r = await fetch(B + p, body === undefined ? {} : { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(body) });
  return r.json();
};

(async () => {
  const browser = await chromium.launch(require('./browser')({ headless: true }));
  const ctx = await browser.newContext({ viewport: { width: 1360, height: 1000 } });
  const page = await ctx.newPage();
  const errors = [];
  page.on('pageerror', (e) => errors.push(e.message));

  console.log('== the first-run setup, on a first start with an empty data folder');
  ok((await api('/api/status')).first_run === true && fs.existsSync(path.join(process.env.DATA, 'first-run')), 'the server says it is a first start');
  await page.goto(B + '/'); await sleep(1000);
  ok(await page.locator('#setup').isVisible(), 'the setup is shown in place of the home page');
  const setupText = await page.locator('#setup').textContent();
  ok(/Where apps come from/.test(setupText) && /Claude/.test(setupText) && /ADMIN_TOKEN/.test(setupText), 'it covers where apps come from, Claude, and the admin token');
  ok(await page.locator('#setup #aiCard #aiProvBedrock').count() === 1 && await page.locator('#setup #aiCard #aiKey').isVisible(), 'it offers the Claude settings form itself (Anthropic or Bedrock)');
  ok(await page.evaluate(() => document.activeElement.id) === 'setupTitle', 'the focus starts on its heading');
  await checkNames(page, '#setup', 'setup', 5);
  await page.check('#aiProvBedrock');
  await checkNames(page, '#setup', 'setup with Bedrock chosen', 5);
  await page.check('#aiProvAnthropic');
  await checkTabOrder(page, '#setup', 'setup', { start: '#setupTitle' });
  // Settings borrows the Claude card back while it is open.
  await page.click('#setup button:has-text("Import an app")');
  await page.locator('#settings[open]').waitFor({ timeout: 3000 });
  ok(await page.locator('#settings #aiCard').count() === 1 && await page.locator('#setTab-import').getAttribute('aria-selected') === 'true', 'Import an app opens Settings at Import; the Claude card is back in Settings');
  await page.keyboard.press('Escape');
  await sleep(200);
  ok(await page.locator('#setup #aiCard').count() === 1, 'closing Settings puts the Claude card back in the setup');
  await page.focus('#setupTitle');
  ok(await keyboardPress(page, '#setupSkip'), 'Skip setup is reached and pressed by keyboard');
  await sleep(500);
  ok(await page.locator('#setup').count() === 0 && await page.locator('.home').isVisible(), 'Skip shows the home page');
  ok((await api('/api/status')).first_run === false && !fs.existsSync(path.join(process.env.DATA, 'first-run')), 'the server forgets the first start');
  await page.reload(); await sleep(1000);
  ok(await page.locator('#setup').count() === 0 && await page.locator('.home').isVisible(), 'after a reload the setup is not shown again');
  const other = await (await browser.newContext()).newPage();
  await other.goto(B + '/'); await sleep(800);
  ok(await other.locator('#setup').count() === 0, 'nor in another browser');
  await other.context().close();

  console.log('== the app list');
  await checkNames(page, null, 'home and app list', 10);
  await checkTabOrder(page, 'header, aside, #runner', 'home and app list');
  await page.fill('#filter', 'usl');
  ok(await page.locator('#apps li.app').count() === 1, 'the filter is typed into');
  await page.fill('#filter', '');

  console.log('== folders in the app list (ADR-2610081830)');
  const toggle = page.locator('#apps .folder-toggle').first();
  ok(/^Examples, \d+ apps$/.test(await toggle.getAttribute('aria-label')) && await toggle.getAttribute('aria-expanded') === 'true', 'the Examples folder says its name, its count and that it is open: ' + await toggle.getAttribute('aria-label'));
  const moves = await page.$$eval('#apps .app-move', (bs) => bs.map((b) => b.getAttribute('aria-label')));
  ok(moves.length > 5 && moves.every((n) => /^Move .+ to…$/.test(n)), `every app has a named Move to… button (${moves.length})`);
  await toggle.focus();
  await page.keyboard.press('Enter');
  ok(await toggle.getAttribute('aria-expanded') === 'false' && await page.locator('#apps ul.folder-apps').first().isHidden(), 'Enter closes the folder');
  await checkTabOrder(page, 'aside', 'the app list with Examples closed');
  await toggle.focus();
  await page.keyboard.press('Enter');
  ok(await toggle.getAttribute('aria-expanded') === 'true', 'and opens it again');
  await page.keyboard.press('Tab');
  ok(await page.evaluate(() => document.activeElement.matches('.folder-more') && document.activeElement.getAttribute('aria-haspopup') === 'menu'), 'Tab reaches the folder actions');
  await page.keyboard.press('Tab');
  await page.keyboard.press('Tab');
  ok(await page.evaluate(() => document.activeElement.matches('.app-move')), 'then an app, then its Move to… button');
  await page.keyboard.press('Enter');
  ok(await page.locator('.menu[role=menu]').isVisible() && await page.evaluate(() => /^menuitem/.test(document.activeElement.getAttribute('role'))), 'Enter opens the Move menu with the focus in it');
  await checkNames(page, '.menu', 'the Move menu', 3);
  await page.keyboard.press('ArrowDown');
  ok(await page.evaluate(() => document.activeElement.closest('.menu') !== null), 'ArrowDown moves inside the menu');
  await page.keyboard.press('Escape');
  ok(await page.locator('.menu').count() === 0 && await page.evaluate(() => document.activeElement.matches('.app-move') && document.activeElement.getAttribute('aria-expanded') === 'false'), 'Escape closes the Move menu and the focus goes back to its button');
  await page.focus('#apps .folder-more');
  await page.keyboard.press('Enter');
  await checkNames(page, '.menu', 'the folder menu', 2);
  await page.keyboard.press('Escape');
  ok(await page.locator('.menu').count() === 0, 'Escape closes the folder menu');
  await page.focus('#newFolder');
  ok(await page.evaluate(() => document.activeElement.id === 'newFolder' && document.activeElement.textContent.trim() === 'New folder'), 'New folder is a named button reached by keyboard');

  console.log('== Settings');
  await page.focus('#settingsBtn');
  await page.keyboard.press('Enter');
  await page.locator('#settings[open]').waitFor({ timeout: 3000 });
  ok(await page.locator('#settingsTabs [role=tablist]').isVisible(), 'Settings opens with a list of sections');
  const sections = await page.$$eval('#settingsTabs [role=tab]', (ts) => ts.map((t) => t.id));
  ok(sections.length >= 6, 'sections: ' + sections.join(', '));
  // Arrow keys move between sections (the library's tabs).
  await page.focus('#' + sections[0]);
  await page.keyboard.press('ArrowRight');
  ok(await page.evaluate(() => document.activeElement.id) === sections[1] && await page.locator('#' + sections[1]).getAttribute('aria-selected') === 'true', 'ArrowRight moves to the next section');
  for (const id of sections) {
    await page.click('#' + id);
    const panel = await page.locator('#' + id).getAttribute('aria-controls');
    ok(await page.locator('#' + panel).isVisible() && (await page.locator('#settingsTabs [role=tabpanel]:visible').count()) === 1, `${id}: only its section is shown`);
    await checkNames(page, '#settings', `Settings, ${await page.locator('#' + id).textContent()}`, sections.length + 1);
    ok_inside = [];
    await checkTabOrder(page, '#settings', `Settings, ${await page.locator('#' + id).textContent()}`, { start: '#' + id, inside: '#settings' });
    ok(ok_inside.length > 0 && ok_inside.every(Boolean), `Settings, ${await page.locator('#' + id).textContent()}: Tab never leaves the dialog (${ok_inside.length} presses)`);
  }
  // The parts that show only after a choice.
  await page.click('#setTab-claude');
  await page.check('#aiProvBedrock');
  await page.selectOption('#bdAuth', 'access-keys');
  await checkNames(page, '#settings', 'Settings, Claude on Bedrock with access keys', 10);
  await page.selectOption('#bdAuth', 'profile');
  await page.locator('#bdProfile option').first().waitFor({ state: 'attached', timeout: 5000 }).catch(() => {});
  await checkNames(page, '#settings', 'Settings, Claude on Bedrock with an AWS profile', 10);
  await page.selectOption('#bdAuth', 'api-key');
  await page.check('#aiProvAnthropic');
  await page.click('#setTab-source');
  await page.click('.tabs button[data-tab=drive]');
  ok(await page.locator('.tabs button[data-tab=drive]').getAttribute('aria-pressed') === 'true', 'the app source choice says which is pressed');
  await checkNames(page, '#settings', 'Settings, Google Drive source', 10);
  ok_inside = [];
  await checkTabOrder(page, '#settings', 'Settings, Google Drive source', { start: '#setTab-source', inside: '#settings' });
  // Shift+Tab from the first control wraps to the last.
  await page.focus('#closeSettings');
  await page.keyboard.press('Shift+Tab');
  ok(await page.evaluate(() => document.getElementById('settings').contains(document.activeElement) && document.activeElement.id !== 'closeSettings'), 'Shift+Tab from the first control stays in the dialog');
  await page.keyboard.press('Escape');
  await sleep(200);
  ok(!(await page.locator('#settings').evaluate((d) => d.open)), 'Escape closes Settings');
  ok(await page.evaluate(() => document.activeElement.id) === 'settingsBtn', 'and the focus goes back to the Settings button');

  console.log('== Arrange');
  await page.locator('#apps li button', { hasText: /USL/i }).first().focus();
  await page.keyboard.press('Enter');
  await page.locator('#arrangeTool').waitFor({ state: 'visible', timeout: 6000 });
  await checkNames(page, null, 'a suite in the app list, with its Arrange button', 10);
  await checkTabOrder(page, 'header, aside, #runner', 'a suite in the app list');
  await page.focus('#arrangeTool');
  await page.keyboard.press('Enter');
  await sleep(500);
  ok(await page.locator('#arrangeTool').getAttribute('aria-pressed') === 'true', 'the Arrange button opens Arrange by keyboard and says it is pressed');
  await page.keyboard.press('Enter');
  await sleep(300);
  // A module app: Wardian draws its functions and their Arrange here.
  await page.locator('#apps li button', { hasText: /adder/i }).first().focus();
  await page.keyboard.press('Enter');
  await page.locator('#runner .w-arrange-btn').waitFor({ timeout: 6000 });
  await page.focus('#runner .w-arrange-btn');
  await page.keyboard.press('Enter');
  await page.locator('#runner .w-arrange-bar').waitFor({ state: 'visible', timeout: 3000 });
  await checkNames(page, '#runner', 'a module app with Arrange open', 6);
  await checkTabOrder(page, '#runner', 'a module app with Arrange open', { start: '#runner h2' });
  await page.keyboard.press('Escape');
  ok(await page.locator('#runner .w-arrange-bar').isHidden(), 'Escape closes Arrange');
  // The suite's own page, as it opens full screen.
  const suite = await ctx.newPage();
  suite.on('pageerror', (e) => errors.push(e.message));
  await suite.goto(B + '/run/usl-lab/'); await sleep(2500);
  await suite.focus('.w-arrange-btn');
  await suite.keyboard.press('Enter');
  await suite.locator('.w-arrange-bar').waitFor({ state: 'visible', timeout: 3000 });
  await checkNames(suite, null, 'a suite page with Arrange open', 15);
  await checkTabOrder(suite, '.w-arrange-bar, .w-arrange-cover', 'a suite page with Arrange open', { start: '.w-arrange-bar' });
  await suite.focus('[data-arrange-panel=readouts] > .w-arrange-cover button[data-a=up]');
  await suite.keyboard.press('Enter');
  const order = await suite.$$eval('.main > [data-arrange-panel]', (ps) => ps.map((p) => p.getAttribute('data-arrange-panel')));
  ok(order.indexOf('readouts') < order.indexOf('whatif'), 'a panel moves up by keyboard: ' + order.join());
  await suite.locator('.w-arrange-bar button:has-text("Reset")').focus();
  await suite.keyboard.press('Enter');
  await suite.keyboard.press('Escape');
  ok(await suite.locator('.w-arrange-bar').isHidden(), 'Escape closes it');
  await suite.close();

  console.log('== History');
  // Two versions: importing an app over itself keeps the original as version 1.
  const zip = Buffer.from(await (await fetch(B + '/api/apps/loan-planner/export?data=0')).arrayBuffer());
  const imp = await (await fetch(B + '/api/import?name=loan-planner.wardian&replace=1', { method: 'POST', headers: { 'Content-Type': 'application/zip' }, body: zip })).json();
  ok(imp.apps && imp.apps.includes('loan-planner'), 'loan-planner imported over itself');
  await page.reload(); await sleep(800);
  await page.locator('#apps li button', { hasText: /loan/i }).first().focus();
  await page.keyboard.press('Enter');
  await page.locator('#historyBtn').waitFor({ timeout: 5000 });
  ok(await keyboardPress(page, '#historyBtn'), 'History is reached and pressed by keyboard');
  await page.locator('#historyList .ver').first().waitFor({ timeout: 5000 });
  ok(await page.locator('#historyList .ver').count() >= 2, 'History lists the versions');
  await checkNames(page, '#runner', 'History', 3);
  await checkTabOrder(page, '#runner', 'History', { start: '#runner h2' });
  const cmp = await page.locator('#historyList .ver button', { hasText: 'Compare with current' }).first().elementHandle();
  ok(/^Compare version \d+ with current$/.test(await cmp.getAttribute('aria-label')), 'Compare says which version: ' + await cmp.getAttribute('aria-label'));
  await cmp.focus();
  await page.keyboard.press('Enter');
  await page.locator('#historyList .diffbox:not([hidden])').first().waitFor({ timeout: 5000 });
  ok(await cmp.getAttribute('aria-expanded') === 'true', 'and that its changes are shown');
  await checkNames(page, '#runner', 'History with a comparison open', 3);
  const restore = page.locator('#historyList .ver button', { hasText: /^Restore$/ }).first();
  await restore.focus();
  await page.keyboard.press('Enter');
  ok(/^Restore version \d+$/.test(await page.evaluate(() => document.activeElement.textContent)), 'Restore asks to confirm, with the focus on the confirmation');
  await checkNames(page, '#runner', 'History asking to confirm', 3);

  ok(errors.length === 0, 'no page errors ' + JSON.stringify(errors));
  await browser.close();
  console.log(`\n${pass} passed, ${fail} failed`);
  process.exit(fail ? 1 : 0);
})().catch((e) => { console.error(e); process.exit(1); });
