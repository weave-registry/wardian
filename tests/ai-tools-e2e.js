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
  ok(loop.length === 3, `three steps of the loop went to Claude: the tool, the answer, and the answer sent back once (${loop.length})`);
  ok(loop[0].messages[0].content.includes('Look the hosts up with top_hosts before you'), 'with the skill from skill.md');
  ok(loop[0].messages[0].content.includes('User: Which host had the most errors?'), 'and the conversation as the task');
  ok(loop[2].messages[0].content.includes('Part of your answer cannot be shown: blocks[3]'), 'the block of no kind went back to be fixed');

  console.log('== Ask: blocks (ADR-2610231000)');
  const table = reply.locator('table.w-table');
  ok(await table.count() === 1, 'the answer draws a table');
  ok(JSON.stringify(await table.locator('th').allTextContents()) === '["host","errors"]', 'with the columns of top_hosts\' result');
  ok(JSON.stringify(await table.locator('tbody tr').allTextContents()) === '["web-140","db-112"]', 'and its two rows, as the tool returned them');
  ok(await table.locator('th.num').textContent() === 'errors', 'numbers line up on the right');
  ok(/From top_hosts/.test(await table.locator('caption').textContent()), 'the table says where it comes from');
  ok(await reply.locator('.w-tile .answer-value').textContent() === '40', 'a figure is a tile');
  const item = reply.locator('.answer-list li');
  ok(await item.textContent() === '<b>web-1</b> needs a look' && await item.locator('b').count() === 0, 'markup in a block stays text');
  ok(await reply.locator('.ask-dropped').textContent() === 'Part of this answer could not be shown.', 'a block that could not be shown is left out, and Ask says so');
  const box = await reply.boundingBox(), panel = await page.locator('#askPanel').boundingBox();
  ok(box.x >= panel.x && box.x + box.width <= panel.x + panel.width + 1, 'the blocks fit the panel');
  const next = page.locator('#askPanel .ask-suggest button');
  ok(await next.textContent() === 'Which host had the fewest errors?', 'a follow-up question is offered');
  await next.click();
  await page.locator('#askPanel .msg.say').nth(1).waitFor({ timeout: 20000 });
  ok(await page.locator('#askPanel .msg.user').nth(1).textContent() === 'Which host had the fewest errors?', 'clicking it asks it');
  ok(await page.locator('#askPanel .ask-suggest').count() === 1, 'and the old suggestions are gone');
  const second = fs.readFileSync(LOG, 'utf8').trim().split('\n').map(l => JSON.parse(l)).filter(b => JSON.stringify(b).includes('fewest errors?'))[0];
  ok(second && second.messages[0].content.includes('You: web-1 had the most errors: 40.\n\nErrors by host\nhost\terrors\nweb-1\t40'), 'the next question carries the blocks as text');
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

  console.log('== meeting-notes');
  await page.click('text=Back to the app');
  await page.locator('#apps li button', { hasText: /Meeting notes/ }).first().click();
  const suite = page.frameLocator('iframe.suiteframe');
  const reader = suite.frameLocator('iframe[title="reader"]');
  await reader.locator('#mode', { hasText: 'Claude is on' }).waitFor({ timeout: 15000 });
  ok(true, 'the reader learns from the ai part that Claude is on');
  await reader.locator('#claude').click();
  const summary = suite.frameLocator('iframe[title="summary"]');
  await summary.getByText('FAKE: the team agreed to ship on Friday.').waitFor({ timeout: 20000 });
  ok(true, "Read with Claude goes through the ai part, and the summary shows Claude's reading");
  const bodies = () => fs.readFileSync(LOG, 'utf8').trim().split('\n').map(l => JSON.parse(l));
  const read = bodies().filter(b => JSON.stringify(b).includes('Read these meeting notes')).pop();
  ok(read && read.messages[0].content.includes('Do not invent owners, dates or decisions') && read.messages[0].content.includes('"current_reading"'),
    'with the skill and tools from its files');
  await page.click('#askBtn');
  await page.fill('#askPanel textarea', 'Who writes the release notes?');
  await page.press('#askPanel textarea', 'Enter');
  await page.locator('#askPanel .msg.say').waitFor({ timeout: 20000 });
  const said = await page.locator('#askPanel .msg.say').textContent();
  ok(/Ana writes the release notes/.test(said) && /Looked at: current_reading/.test(said), 'Ask answers from the reading on screen: ' + said);
  const chat = bodies().filter(b => JSON.stringify(b).includes('1. current_reading')).pop();
  ok(chat && chat.messages[0].content.includes('Write the release notes') && chat.messages[0].content.includes('Thursday'), 'current_reading gave Claude the checklist');
  await page.keyboard.press('Escape');
  await page.click('#moreBtn');
  await page.click('#aiToolsBtn');
  await page.locator('#aiSkill').waitFor({ timeout: 5000 });
  ok(await page.locator('#aiToolList .tool').count() === 3, 'its settings list its three tools');
  ok(!/no code|proposed/.test(await page.locator('#aiToolList').textContent()), 'each has code');

  // The other examples with AI: each has Ask, and settings whose tools all have code.
  for (const [title, count] of [[/Splunk table/, 5], [/USL/, 4]]) {
    await page.click('text=Back to the app');
    await page.locator('#apps li button', { hasText: title }).first().click();
    await page.locator('#askBtn').waitFor({ timeout: 10000 });
    await page.click('#moreBtn');
    await page.click('#aiToolsBtn');
    await page.locator('#aiSkill').waitFor({ timeout: 5000 });
    const listed = await page.locator('#aiToolList .tool').count();
    ok(listed === count && !/no code|proposed/.test(await page.locator('#aiToolList').textContent()), `${title.source}: Ask, and ${listed} tools, each with code`);
  }

  ok(errors.length === 0, 'no page errors' + (errors.length ? ': ' + errors.join('; ') : ''));
  await browser.close();
  console.log(`${pass} passed, ${fail} failed`);
  process.exit(fail ? 1 : 0);
})().catch(e => { console.error(e); process.exit(1); });
