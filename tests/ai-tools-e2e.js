// End-to-end test of an AI app's skill and tools in App settings, and of Ask (ADR-2610131300).
const { chromium } = require('playwright');
const fs = require('fs'), path = require('path');
const BASE = process.env.BASE, APPS = process.env.APPS, LOG = process.env.CLAUDE_LOG;
let pass = 0, fail = 0;
const ok = (c, m) => { if (c) { pass++; console.log('  ok  ', m); } else { fail++; console.log('  FAIL', m); } };
const post = (p, body) => fetch(BASE + p, { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(body) }).then(r => r.json());

(async () => {
  const key = await post('/api/ai/key', { key: 'sk-ant-test-0123456789-abcd' });
  ok(key.ready === true, 'Claude is set up (the fake)');

  const browser = await chromium.launch(require('./browser')({ headless: true }));
  const page = await browser.newPage({ viewport: { width: 1280, height: 900 } });
  const errors = [];
  page.on('pageerror', e => errors.push(e.message));
  await page.goto(BASE + '/');
  await page.locator('#apps li button', { hasText: /Hosts/ }).first().click();

  console.log('== Ask');
  await page.locator('#askBtn').waitFor({ timeout: 5000 });
  ok(true, 'an app whose ai app provides chat has Ask');
  await page.click('#askBtn');
  ok(await page.locator('#askBtn').getAttribute('aria-expanded') === 'true', 'Ask says it is open');
  const input = page.locator('#askPanel textarea');
  ok(await input.evaluate(n => n === document.activeElement), 'the question box has focus');
  await input.fill('Which host had the most errors?');
  await input.press('Enter');
  const reply = page.locator('#askPanel .msg.say');
  await reply.waitFor({ timeout: 20000 });
  ok(/web-1 had the most errors: 40\./.test(await reply.textContent()), 'the ai app answers through its tools: ' + await reply.textContent());
  ok(/Looked at: top_hosts/.test(await reply.textContent()), 'and says which tools it used');
  const log = fs.readFileSync(LOG, 'utf8').trim().split('\n').map(l => JSON.parse(l));
  const loop = log.filter(b => JSON.stringify(b).includes('<steps>'));
  ok(loop.length === 2, `two steps of the loop went to Claude (${loop.length})`);
  ok(loop[0].messages[0].content.includes('Look the hosts up with top_hosts before you'), 'with the skill from skill.md');
  ok(loop[0].messages[0].content.includes('User: Which host had the most errors?'), 'and the conversation as the task');
  await page.keyboard.press('Escape');
  ok(await page.locator('#askPanel').count() === 0, 'Escape closes Ask');
  ok(await page.locator('#askBtn').evaluate(n => n === document.activeElement), 'and focus goes back to Ask');

  console.log('== AI skill and tools');
  await page.click('#moreBtn');
  await page.click('#aiToolsBtn');
  await page.locator('#aiSkill').waitFor({ timeout: 5000 });
  ok(/Look the hosts up with top_hosts/.test(await page.locator('#aiSkill').inputValue()), 'the skill is shown');
  const cards = page.locator('#aiToolList .tool');
  ok(await cards.count() === 1 && await cards.first().locator('input.w-input').inputValue() === 'top_hosts', 'and the tool');
  ok(!/no code|proposed/.test(await cards.first().textContent()), 'top_hosts has code: no badge');

  await page.fill('#aiWanted', 'Add a lookup for one host');
  await page.click('#aiImprove');
  await page.locator('.aitools-improve button', { hasText: 'Undo' }).waitFor({ timeout: 20000 });
  ok(/Named the method; added find_host/.test(await page.locator('.aitools-improve').textContent()), "Claude's notes are shown");
  ok(await cards.count() === 2, "Claude's version is in the editor");
  ok(/proposed: needs code/.test(await cards.nth(1).textContent()), 'find_host is marked as a proposal');
  ok(await page.locator('#aiBuild').isVisible(), 'with Build proposed tools offered');
  ok(fs.readFileSync(path.join(APPS, 'hosts/apps/ai/skill.md'), 'utf8').startsWith('You answer questions'), 'nothing is saved yet');
  const improve = JSON.parse(fs.readFileSync(LOG, 'utf8').trim().split('\n').pop());
  ok(improve.messages[0].content[0].text.includes('Add a lookup for one host') && improve.messages[0].content[0].text.includes('top_hosts: async'), 'Improve sends the wish and the code');

  await page.click('#aiSave');
  await page.locator('.aitools-save .ok').waitFor({ timeout: 10000 });
  ok(/Saved as version \d+/.test(await page.locator('.aitools-save').textContent()), 'Save says the version');
  const tools = JSON.parse(fs.readFileSync(path.join(APPS, 'hosts/apps/ai/tools.json'), 'utf8'));
  ok(tools.tools.map(t => `${t.name}:${t.proposed}`).join(',') === 'top_hosts:false,find_host:true', 'tools.json holds both, find_host proposed');
  ok(fs.readFileSync(path.join(APPS, 'hosts/apps/ai/skill.md'), 'utf8').startsWith('Look the hosts up with top_hosts first.'), 'skill.md holds the new skill');

  // A bad schema is refused in the editor, and nothing is saved.
  await cards.nth(0).locator('details summary').click();
  await cards.nth(0).locator('textarea.mono').fill('{ not json');
  await cards.nth(0).locator('textarea.mono').blur();
  ok(await cards.nth(0).locator('p.err').isVisible(), 'a schema that is not JSON is flagged at once');
  await page.click('#aiSave');
  await page.locator('.aitools-save .err').waitFor({ timeout: 5000 });
  ok(/not valid JSON/.test(await page.locator('.aitools-save').textContent()), 'and Save says why it did not save');

  console.log('== History');
  await page.click('text=Back to the app');
  await page.click('#moreBtn');
  await page.click('#historyBtn');
  await page.locator('.ver').first().waitFor({ timeout: 5000 });
  ok(/App settings/.test(await page.locator('.ver').first().textContent()), 'the save is a version by App settings');

  ok(errors.length === 0, 'no page errors' + (errors.length ? ': ' + errors.join('; ') : ''));
  await browser.close();
  console.log(`${pass} passed, ${fail} failed`);
  process.exit(fail ? 1 : 0);
})().catch(e => { console.error(e); process.exit(1); });
