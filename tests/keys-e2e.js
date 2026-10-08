// End-to-end test of Settings → Keys, Models and limits, and Usage (ADR-2610081500).
// Run with tests/run-keys-e2e.sh: BASE=<Wardian> ANTHROPIC=<fake Anthropic API> DATA=<data dir> node tests/keys-e2e.js
const { chromium } = require('playwright');
const fs = require('fs');
const path = require('path');
const B = process.env.BASE, A = process.env.ANTHROPIC, DATA = process.env.DATA;
let pass = 0, fail = 0;
const ok = (c, m) => { if (c) { pass++; console.log('  ok  ', m); } else { fail++; console.log('  FAIL', m); } };
const sleep = ms => new Promise(r => setTimeout(r, ms));
// The API as another program on this machine sees it: no token unless one is given.
const call = async (p, body, token) => {
  const headers = { 'Content-Type': 'application/json' };
  if (token) headers['X-Admin-Token'] = token;
  const r = await fetch(B + p, body === undefined ? { headers } : { method: 'POST', headers, body: JSON.stringify(body) });
  return { status: r.status, body: await r.json().catch(() => ({})) };
};
// What meeting-notes' AI part asks, as the kernel sends it.
const sample = (token) => call('/api/ai/sample', { package: 'meeting-notes', app: 'ai', prompt: 'Summarise: we met.' }, token);
const row = (page, id) => page.locator(`#keysList li[data-key="${id}"]`);
const until = async (fn, ms = 5000) => { const t = Date.now(); while (Date.now() - t < ms) { if (await fn()) return true; await sleep(100); } return false; };

(async () => {
  const browser = await chromium.launch(require('./browser')({ headless: true }));
  const page = await (await browser.newContext({ viewport: { width: 1280, height: 1000 } })).newPage();
  const errors = [];
  page.on('pageerror', e => errors.push(e.message));
  await page.goto(B + '/');
  await page.click('#settingsBtn');
  await page.click('#setTab-keys');

  console.log('== Keys');
  await until(async () => await page.locator('#keysList li').count() === 5);
  const ids = await page.$$eval('#keysList li', ls => ls.map(l => l.dataset.key));
  ok(ids.join() === 'anthropic,bedrock,splunk,drive,admin', 'every secret is listed: ' + ids.join());
  ok(/not set/.test(await row(page, 'anthropic').locator('.where').textContent()), 'the Anthropic key is not set');
  ok(await row(page, 'anthropic').getByRole('button', { name: /Test the/ }).count() === 0, 'nothing to test before a key is set');

  await row(page, 'anthropic').getByRole('button', { name: 'Set up the Anthropic API key' }).click();
  ok(await page.locator('#setTab-claude').getAttribute('aria-selected') === 'true', 'Set up opens the Claude section');
  await page.fill('#aiKey', 'test-key');
  await page.click('#aiKeySave');
  await until(async () => /Ready/.test(await page.locator('#aiKeyState').textContent()));
  await page.click('#setTab-keys');
  await until(async () => /saved in Settings/.test(await row(page, 'anthropic').locator('.where').textContent()));
  ok(/saved in Settings/.test(await row(page, 'anthropic').locator('.where').textContent()), 'the key shows as saved in Settings');
  ok(/Tested just now: accepted/.test(await row(page, 'anthropic').locator('.check').textContent()), 'the test before the save is recorded');
  ok(!(await page.locator('#keysList').textContent()).includes('test-key'), 'the key itself is not shown');
  await row(page, 'anthropic').getByRole('button', { name: 'Test the Anthropic API key again' }).click();
  await until(async () => /works/.test(await page.locator('#keysMsg').textContent()));
  ok(/The Anthropic API key works/.test(await page.locator('#keysMsg').textContent()), 'Test again tests the saved key');

  console.log('== Models and limits');
  await page.click('#setTab-claude');
  await until(async () => await page.locator('#agentModel-anthropic-main').count() === 1);
  ok(await page.getAttribute('#agentModel-anthropic-main', 'placeholder') === 'claude-opus-5-5', 'the default model is shown in grey');
  await page.fill('#agentModel-anthropic-main', 'claude-chosen');
  await page.fill('#agentLimit-sample_max_tokens', '300');
  await page.click('#agentSave');
  await until(async () => /Saved|Not saved/.test(await page.locator('#agentMsg').textContent()));
  ok(await page.locator('#agentMsg').textContent() === 'Saved.', 'a model the API knows is saved: ' + await page.locator('#agentMsg').textContent());
  await page.fill('#agentLimit-max_steps', '2');
  await page.click('#agentSave');
  await until(async () => /Not saved/.test(await page.locator('#agentMsg').textContent()));
  ok(/max_steps must be a whole number from 5 to 200/.test(await page.locator('#agentMsg').textContent()), 'a limit out of bounds is refused');
  await page.fill('#agentLimit-max_steps', '40');
  await page.click('#agentReset');
  await until(async () => /defaults/.test(await page.locator('#agentMsg').textContent()));
  const reset = JSON.parse(fs.readFileSync(path.join(DATA, 'agent.json'), 'utf8'));
  ok(reset.models.anthropic.main === '' && reset.sample_max_tokens === 4000 && await page.inputValue('#agentModel-anthropic-main') === '', 'Back to the defaults clears the model and the limits');
  await page.fill('#agentModel-anthropic-main', 'claude-chosen');
  await page.click('#agentSave');
  await until(async () => /Saved|Not saved/.test(await page.locator('#agentMsg').textContent()));
  const saved = JSON.parse(fs.readFileSync(path.join(DATA, 'agent.json'), 'utf8'));
  ok(saved.models.anthropic.main === 'claude-chosen' && saved.max_steps === 40, 'agent.json holds what was saved, and not the refused limit');
  await page.fill('#agentLimit-sample_max_tokens', '300');
  await page.click('#agentSave');
  await until(async () => /Saved|Not saved/.test(await page.locator('#agentMsg').textContent()));

  console.log('== Usage and caps');
  ok((await call('/api/grants', { app: 'meeting-notes', channel: 'ai', mode: 'use', decision: 'allow' })).status === 200, 'meeting-notes may use Claude');
  const first = await sample();
  ok(first.status === 200 && first.body.text, 'an app asks Claude');
  const asked = await (await fetch(A + '/prompts')).json();
  ok(asked.length && asked[asked.length - 1].model === 'claude-chosen', 'the request names the chosen model: ' + (asked[asked.length - 1] || {}).model);
  await sample();
  await page.click('#setTab-usage');
  await until(async () => await page.locator('#usageRows tr[data-payer="meeting-notes"]').count() === 1);
  const cells = await page.locator('#usageRows tr[data-payer="meeting-notes"] td').allTextContents();
  ok(cells[1] === '320' && cells[3] === '2', 'Usage shows today\'s tokens and the requests: ' + cells.join(' | '));
  await page.locator('#usageRows tr[data-payer="meeting-notes"] input').fill('300');
  await page.getByRole('button', { name: 'Save the daily cap for meeting-notes' }).click();
  await until(async () => /Saved the cap/.test(await page.locator('#usageMsg').textContent()));
  const over = await sample();
  ok(over.status === 400 && /^over_budget: /.test(over.body.error), 'past its cap the app gets over_budget: ' + over.body.error);
  await until(async () => await page.locator('#usageRows tr[data-payer="meeting-notes"] td.over').count() === 1);
  ok(await page.locator('#usageRows tr[data-payer="meeting-notes"] td.over').count() === 1, 'the row shows the app is over its cap');

  console.log('== Admin token');
  await page.click('#setTab-keys');
  await page.click('#adminTokenMake');
  await until(async () => /^[0-9a-f]{64}$/.test(await page.locator('#adminTokenValue').textContent()));
  const token = await page.locator('#adminTokenValue').textContent();
  ok(/^[0-9a-f]{64}$/.test(token), 'Make one for me shows a new token once');
  ok((await call('/api/keys')).status === 403, 'without the token, this machine is no longer an admin');
  ok((await call('/api/keys', undefined, token)).status === 200, 'with it, it is');
  ok(/saved in Settings/.test(await row(page, 'admin').locator('.where').textContent()), 'the token shows as saved in Settings');
  await page.reload();
  await page.click('#settingsBtn');
  await until(async () => await page.locator('#settings').isVisible());
  ok(await page.locator('#lockedCard').isHidden() && await page.locator('#settings').evaluate(d => d.classList.contains('is-admin')), 'this tab keeps the token after a reload');
  const other = await (await browser.newContext()).newPage();
  await other.goto(B + '/');
  await other.click('#settingsBtn');
  await until(async () => await other.locator('#lockedCard').isVisible());
  ok(await other.locator('#lockedCard').isVisible(), 'another browser is locked out');
  await other.fill('#tokenInput', token);
  await other.click('#tokenSave');
  await until(async () => await other.locator('#lockedCard').isHidden());
  ok(await other.locator('#lockedCard').isHidden(), 'and unlocks with the token');
  await other.close();

  await page.click('#setTab-keys');
  await until(async () => await row(page, 'admin').getByRole('button', { name: 'Remove the Admin token' }).count() === 1);
  await row(page, 'admin').getByRole('button', { name: 'Remove the Admin token' }).click();
  ok(/Click again/.test(await row(page, 'admin').locator('.acts .danger').textContent()), 'Remove asks for a second click');
  await row(page, 'admin').locator('.acts .danger').click();
  await until(async () => /not set/.test(await row(page, 'admin').locator('.where').textContent()));
  ok((await call('/api/keys')).status === 200, 'with the token removed, this machine is an admin again');

  console.log('== Remove a key');
  await row(page, 'anthropic').locator('.acts .danger').click();
  await row(page, 'anthropic').locator('.acts .danger').click();
  await until(async () => /not set/.test(await row(page, 'anthropic').locator('.where').textContent()));
  ok(/not set/.test(await row(page, 'anthropic').locator('.where').textContent()), 'the removed key is gone');
  ok(!fs.existsSync(path.join(DATA, 'anthropic-key')), 'and its file too');
  ok((await sample()).status === 400, 'an app can no longer use Claude');

  ok(errors.length === 0, 'no page errors: ' + errors.join(' | '));
  await browser.close();
  console.log(`\n${pass} passed, ${fail} failed`);
  process.exit(fail ? 1 : 0);
})().catch(e => { console.error(e); process.exit(1); });
