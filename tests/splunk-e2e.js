// End-to-end test of the splunk and claude:sample capabilities: the Splunk table app runs searches
// against a fake Splunk and sends the table on a channel; the USL lab receives it in another tab.
const { chromium } = require('playwright');
const B = process.env.BASE, SPLUNK = process.env.SPLUNK, ANTHROPIC = process.env.ANTHROPIC;
let pass = 0, fail = 0;
const ok = (c, m) => { if (c) { pass++; console.log('  ok  ', m); } else { fail++; console.log('  FAIL', m); } };
const sleep = ms => new Promise(r => setTimeout(r, ms));
const frameOf = (page, app) => page.frames().find(f => f.url().endsWith('/' + app));
const post = (path, body) => fetch(B + path, { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(body) })
  .then(async r => ({ status: r.status, body: await r.json() }));
// The lab's side panel scrolls on its own, and Chrome does not draw a sandboxed frame's parts that
// are out of sight, so scroll the button into view first, as a person would.
async function useTable(inputs) {
  await inputs.locator('#btnUseTable').evaluate(el => el.scrollIntoView({ block: 'center' }));
  await sleep(300);
  await inputs.locator('#btnUseTable').click();
}
// Answers the host's permission bar on `page` and checks what it asked.
async function answer(page, re, yes, what) {
  const bar = page.locator('.wardian-perm');
  await bar.waitFor({ timeout: 5000 });
  ok(re.test(await bar.textContent()), what + ': ' + (await bar.textContent()).slice(0, 110));
  await sleep(300);                                   // a person reads the question before answering
  await bar.locator('button', { hasText: yes ? 'Allow' : "Don't allow" }).first().click();
}

(async () => {
  console.log('== server: settings and checks');
  let r = await post('/api/splunk/config', { url: SPLUNK, token: 'wrong' });
  ok(r.status === 400 && /401/.test(r.body.error), 'a bad token is refused and not saved');
  r = await post('/api/splunk/search', { package: 'splunk-table', app: 'table', search: 'index=x' });
  ok(r.status === 400, 'no search before Splunk is set up: ' + r.body.error);
  r = await post('/api/splunk/config', { url: SPLUNK, token: 'test-token' });
  ok(r.status === 200 && r.body.ready && r.body.server.user === 'wardian-reader', 'a good token is tested and saved, as ' + r.body.server.user);
  const st = await (await fetch(B + '/api/status')).json();
  ok(st.splunk.ready && !JSON.stringify(st).includes('test-token'), 'status never shows the token');
  r = await post('/api/splunk/search', { package: 'splunk-table', app: 'table', search: 'index=x' });
  ok(/not allowed/.test(r.body.error), 'no search before the user allows it');
  r = await post('/api/splunk/search', { package: 'usl-lab', app: 'inputs', search: 'index=x' });
  ok(/does not declare/.test(r.body.error), 'the USL lab no longer runs searches itself');

  const browser = await chromium.launch({ channel: 'chrome', headless: true });
  const context = await browser.newContext({ viewport: { width: 1360, height: 1000 } });
  const tab = await context.newPage(), lab = await context.newPage();
  const pageErrors = [];
  for (const p of [tab, lab]) p.on('pageerror', e => pageErrors.push(e.message));

  console.log('== the table app asks before it searches');
  await tab.goto(B + '/run/splunk-table/'); await sleep(2500);
  let t = frameOf(tab, 'table');
  ok(await t.locator('#btnRun').isEnabled(), 'Run search is enabled');
  await t.locator('#spl').fill('index=loadtest | stats avg(tput) AS x BY concurrency | table concurrency x r');
  await t.locator('#btnRun').click();
  await answer(tab, /splunk-table.*Splunk searches/, false, 'the host asks about Splunk searches');
  await t.locator('#status', { hasText: 'Search failed' }).waitFor({ timeout: 5000 }).catch(() => {});
  ok(/did not allow/.test(await t.locator('#status').textContent()), 'refusing stops the search');

  console.log('== allowed: the table shows every row and is sent on');
  await post('/api/grants', { app: 'splunk-table', channel: 'splunk', mode: 'use', decision: 'ask' });
  await tab.reload(); await sleep(2500);
  t = frameOf(tab, 'table');
  ok((await t.locator('#spl').inputValue()).includes('stats avg(tput)'), 'the search was remembered');
  await t.locator('#btnRun').click();
  await answer(tab, /splunk-table.*Splunk searches/, true, 'asked about searches');
  await answer(tab, /send messages on the channel splunk\.table/, true, 'then asked about sending the table');
  await t.locator('#status', { hasText: 'rows from Splunk' }).waitFor({ timeout: 5000 });
  ok(/8 rows from Splunk\. Sent to other apps\./.test(await t.locator('#status').textContent()), 'status: ' + await t.locator('#status').textContent());
  ok(await t.locator('#out tbody tr').count() === 8 && (await t.locator('#out th').allTextContents()).join(',') === 'concurrency,x,r', 'the table shows 8 rows of concurrency, x, r');
  const bar = t.locator('wardian-progress');
  ok(await bar.getAttribute('state') === 'done' && /^8 rows in \d+ s$/.test(await bar.getAttribute('label')) && await bar.getAttribute('role') === 'progressbar',
    'the standard progress bar ends as done: ' + await bar.getAttribute('label'));
  ok(/stats avg\(tput\)/.test(await t.locator('#facts').textContent()) && /Last 24 hours/.test(await t.locator('#facts').textContent()), 'with the search and the time range above it');
  await t.locator('#out th', { hasText: 'concurrency' }).click();
  await t.locator('#out th', { hasText: 'concurrency' }).click();
  ok(await t.locator('#out tbody tr:first-child td:first-child').textContent() === '64', 'a header click sorts, again reverses');

  console.log('== the lab gets the table after the user allows it');
  await lab.goto(B + '/run/usl-lab/'); await sleep(3000);
  let inputs = frameOf(lab, 'inputs');
  ok(/Press Get tables/.test(await inputs.locator('#tblStatus').textContent()), 'the lab does not listen until asked');
  await inputs.locator('#btnConnect').click();
  await answer(lab, /usl-lab.*read messages on the channel splunk\.table/, true, 'the host asks about reading tables');
  await inputs.locator('#tblPick').waitFor({ state: 'visible', timeout: 5000 });
  ok(/Splunk search: 8 rows, Last 24 hours/.test(await inputs.locator('#tblName').textContent()) && /by splunk-table/.test(await inputs.locator('#tblName').textContent()), 'the latest table arrives: ' + await inputs.locator('#tblName').textContent());
  ok(await inputs.locator('#colN').inputValue() === 'concurrency' && await inputs.locator('#colX').inputValue() === 'x' && await inputs.locator('#colR').inputValue() === 'r', 'the first three columns are suggested');
  await useTable(inputs);
  await sleep(1500);
  const data = await inputs.locator('#data').inputValue();
  ok(data.split('\n')[0] === '# threads, req/s, response time (ms)' && data.split('\n').length === 9, 'measurements filled, with a readable header');
  ok(await frameOf(lab, 'chart').locator('#chart circle.pt').count() === 8, 'the chart shows the 8 rows');
  let aboutText = await frameOf(lab, 'chart').locator('#about').textContent();
  ok(/Splunk search/.test(aboutText) && /The column “concurrency”/.test(aboutText) && /Splunk table app/.test(aboutText), 'the chart says where the data came from');

  console.log('== a ready-made search: the lab updates live, with labels and the minutes column');
  await t.locator('#preset').selectOption('traffic');
  ok((await t.locator('#spl').inputValue()).includes('table n x r minutes') && await t.locator('#range').inputValue() === '-7d|', 'the traffic template search keeps the minutes column, over 7 days');
  await t.locator('#btnRun').click();
  await t.locator('#status', { hasText: '6 rows from Splunk' }).waitFor({ timeout: 5000 }).catch(async () => console.log('STATUS', await t.locator('#status').textContent()));
  ok(await tab.locator('.wardian-perm').count() === 0, 'no second question');
  ok((await t.locator('#out th').allTextContents()).join(',') === 'n,x,r,minutes', 'the minutes behind each row are visible');
  await inputs.locator('#tblName', { hasText: 'Requests in production' }).waitFor({ timeout: 5000 });
  ok(await inputs.locator('#colN').inputValue() === 'n' && await inputs.locator('#colR').inputValue() === 'r', 'the lab picks n, x, r');
  await useTable(inputs); await sleep(1500);
  ok(await inputs.locator('#nUnit').inputValue() === 'requests in progress', 'and the units');
  aboutText = await frameOf(lab, 'chart').locator('#about').textContent();
  ok(/Requests in production/.test(aboutText) && /Little's Law/.test(aboutText) && /Last 7 days/.test(aboutText), 'the traffic data is labelled');
  await inputs.locator('#data').fill((await inputs.locator('#data').inputValue()) + '\n70, 9000');
  await sleep(1200);
  ok(/changed the numbers by hand/.test(await frameOf(lab, 'chart').locator('#about').textContent()), 'editing by hand is noted');
  await lab.reload(); await sleep(3000);
  inputs = frameOf(lab, 'inputs');
  await inputs.locator('#tblPick').waitFor({ state: 'visible', timeout: 5000 }).catch(() => {});
  ok(await lab.locator('.wardian-perm').count() === 0 && /Requests in production/.test(await inputs.locator('#tblName').textContent()), 'after a reload the lab listens again without asking');

  console.log('== Splunk errors are shown');
  await t.locator('#spl').fill('| badsyntax');
  await t.locator('#btnRun').click();
  await t.locator('#status', { hasText: 'Unknown search command' }).waitFor({ timeout: 5000 }).catch(() => {});
  ok(/Unknown search command/.test(await t.locator('#status').textContent()), 'Splunk\'s own error is shown');
  ok(await t.locator('wardian-progress').getAttribute('state') === 'error', 'and the progress bar shows the failure');

  console.log('== without an Anthropic key, no AI anywhere');
  ok(await t.locator('#aiAsk').isHidden(), 'no Write with AI');
  ok(await lab.locator('iframe[title=diagnosis]').evaluate(el => el.offsetHeight) === 0, 'no diagnosis panel');

  console.log('== with a key: Claude writes the search, and the diagnosis works');
  r = await post('/api/ai/key', { key: 'org-key' });
  ok(r.status === 400 && /anthropic-workspace-id/.test(r.body.error), 'a key without a workspace is refused, with the API\'s reason');
  r = await post('/api/ai/key', { key: 'org-key', workspace: 'wrkspc_test' });
  ok(r.status === 200 && r.body.ready && r.body.workspace === 'wrkspc_test', 'the same key with a workspace ID is accepted');
  r = await post('/api/ai/key', { key: '', workspace: 'bad id!' });
  ok(r.status === 400 && /workspace ID/.test(r.body.error), 'a malformed workspace ID is refused');
  r = await post('/api/ai/key', { key: 'test-key', workspace: '' });
  ok(r.status === 200 && r.body.ready && r.body.workspace === null, 'the key is tested and saved; an empty workspace removes it');
  r = await post('/api/ai/sample', { package: 'splunk-table', app: 'table', prompt: 'hi' });
  ok(/^not_granted/.test(r.body.error), 'no AI before the user allows it');
  r = await post('/api/ai/sample', { package: 'usl-lab', app: 'chart', prompt: 'hi' });
  ok(/does not declare/.test(r.body.error), 'an app without claude:sample is refused');
  await tab.reload(); await sleep(2500);
  t = frameOf(tab, 'table');
  ok(await t.locator('#aiAsk').isVisible(), 'Write with AI shows');
  await t.locator('#aiAsk summary').click();
  await t.locator('#aiWhat').fill('throughput of the load test as users grow');
  await t.locator('#btnAi').click();
  await answer(tab, /Claude AI requests/, true, 'the host asks about Claude');
  await t.locator('#status', { hasText: 'rows from Splunk' }).waitFor({ timeout: 10000 });
  ok((await t.locator('#spl').inputValue()).startsWith('index="loadtest" sourcetype="jmeter"'), 'Claude\'s search is in the box');
  ok(/Average throughput at each concurrency step/.test(await t.locator('#status').textContent()), 'with its explanation');
  ok(await t.locator('#title').inputValue() === 'JMeter load test steps', 'and its name for the table');
  const prompts = await (await fetch(ANTHROPIC + '/prompts')).json();
  ok(prompts.length === 2 && prompts[0].model.includes('haiku'), 'two requests: a quick pick, then the search');
  ok(!JSON.stringify(prompts).includes('ann.lee@example.com') && JSON.stringify(prompts).includes('<email>'), 'email addresses never reach Claude');
  await inputs.locator('#tblName', { hasText: 'JMeter' }).waitFor({ timeout: 5000 }).catch(() => {});
  await useTable(inputs); await sleep(1500);
  aboutText = await frameOf(lab, 'chart').locator('#about').textContent();
  ok(/JMeter load test steps/.test(aboutText) && /Claude wrote this search/.test(aboutText), 'Claude\'s labels reach the lab');

  await lab.reload(); await sleep(3000);
  const diag = frameOf(lab, 'diagnosis');
  ok(await lab.locator('iframe[title=diagnosis]').evaluate(el => el.offsetHeight) > 0, 'the diagnosis panel shows');
  // Chrome stops drawing an off-screen sandboxed frame, so Playwright cannot click in it: scroll to it first.
  await lab.locator('iframe[title=diagnosis]').scrollIntoViewIfNeeded(); await sleep(300);
  await diag.locator('#btnAi').click();
  await answer(lab, /usl-lab.*Claude AI requests/, true, 'the lab asks about Claude for itself');
  await diag.locator('#aiOut h3').first().waitFor({ timeout: 10000 });
  ok(/What the curve says/.test(await diag.locator('#aiOut').textContent()), 'Interpret results writes a diagnosis');

  console.log('== without Splunk set up, the button explains');
  await post('/api/splunk/config', { url: '' });
  await tab.reload(); await sleep(2500);
  t = frameOf(tab, 'table');
  ok(await t.locator('#btnRun').isDisabled(), 'Run search is disabled');
  ok(/Settings → Splunk/.test(await t.locator('#status').textContent()), 'and says where to set it up');

  ok(pageErrors.length === 0, 'no page errors ' + JSON.stringify(pageErrors));
  for (const p of [tab, lab]) ok((await p.evaluate(() => Kernel.faults())).length === 0, 'no kernel faults in ' + new URL(p.url()).pathname);
  await browser.close();
  console.log(`\n${pass} passed, ${fail} failed`);
  process.exit(fail ? 1 : 0);
})().catch(e => { console.error(e); process.exit(1); });
