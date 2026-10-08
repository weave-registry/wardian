// End-to-end test of the splunk and claude:sample capabilities: the Splunk table app runs searches
// against a fake Splunk and sends the table on a channel; the USL lab receives it in another tab.
// The table app is a suite of five parts (ADR-2610080900), each in its own frame: search (the form,
// Run, saved searches), ask (Claude), about (the facts), rows (find, sort, the table, the pager) and
// keep (Send, CSV, Save table, saved tables).
const { chromium } = require('playwright');
const B = process.env.BASE, SPLUNK = process.env.SPLUNK, ANTHROPIC = process.env.ANTHROPIC;
let pass = 0, fail = 0;
const ok = (c, m) => { if (c) { pass++; console.log('  ok  ', m); } else { fail++; console.log('  FAIL', m); } };
const sleep = ms => new Promise(r => setTimeout(r, ms));
const frameOf = (page, app) => page.frames().find(f => f.url().endsWith('/' + app));
// The parts of the table app on `page`.
const partsOf = page => ({ s: frameOf(page, 'search'), a: frameOf(page, 'about'), rw: frameOf(page, 'rows'), k: frameOf(page, 'keep') });
// A freshly loaded sandboxed frame runs in its own process, and a mouse click sent in its first
// moments can be lost; these tests are about the app, so they press buttons from inside the frame.
const press = (frame, sel) => frame.locator(sel).dispatchEvent('click');
// A part's frame can be below the window, or still growing: scroll until `sel` is in view.
async function inView(page, frame, sel) {
  for (let i = 0; i < 5; i++) {
    await frame.locator(sel).evaluate(el => el.scrollIntoView({ block: 'center' })); await sleep(300);
    const box = await frame.locator(sel).boundingBox();
    if (box && box.y >= 0 && box.y + box.height <= page.viewportSize().height) break;
  }
}
const panels = page => page.$$eval('[data-arrange-panel]', ps => ps.map(p => p.getAttribute('data-arrange-panel') + (p.hasAttribute('data-arrange-hidden') ? '(hidden)' : '')));
const frameHeight = (page, part) => page.locator('iframe[title=' + part + ']').evaluate(el => el.offsetHeight);
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
  r = await post('/api/splunk/search', { package: 'splunk-table', app: 'search', search: 'index=x' });
  ok(r.status === 400, 'no search before Splunk is set up: ' + r.body.error);
  r = await post('/api/splunk/config', { url: SPLUNK, token: 'test-token' });
  ok(r.status === 200 && r.body.ready && r.body.server.user === 'wardian-reader', 'a good token is tested and saved, as ' + r.body.server.user);
  const st = await (await fetch(B + '/api/status')).json();
  ok(st.splunk.ready && !JSON.stringify(st).includes('test-token'), 'status never shows the token');
  r = await post('/api/splunk/search', { package: 'splunk-table', app: 'search', search: 'index=x' });
  ok(/not allowed/.test(r.body.error), 'no search before the user allows it');
  r = await post('/api/splunk/search', { package: 'usl-lab', app: 'inputs', search: 'index=x' });
  ok(/does not declare/.test(r.body.error), 'the USL lab no longer runs searches itself');

  const browser = await chromium.launch(require('./browser')({ headless: true }));
  const context = await browser.newContext({ viewport: { width: 1360, height: 1000 } });
  const tab = await context.newPage(), lab = await context.newPage();
  const pageErrors = [];
  for (const p of [tab, lab]) p.on('pageerror', e => pageErrors.push(e.message));

  console.log('== the table app asks before it searches');
  await tab.goto(B + '/run/splunk-table/'); await sleep(2500);
  let { s, a, rw, k } = partsOf(tab);
  ok((await panels(tab)).join() === 'search,ask,about,rows,keep' && (await tab.evaluate(() => Kernel.started())).length === 5, 'five parts, each its own panel: ' + (await panels(tab)).join());
  const heights = await Promise.all(['search', 'about', 'rows', 'keep'].map(p => frameHeight(tab, p)));
  ok(heights.every(h => h > 40) && await frameHeight(tab, 'ask') === 0, 'the four panels show; ask stays folded away without Claude: ' + heights.join(', '));
  ok(await s.locator('#btnRun').isEnabled(), 'Run search is enabled');
  await s.locator('#spl').fill('index=loadtest | stats avg(tput) AS x BY concurrency | table concurrency x r');
  await s.locator('#btnRun').click();
  await answer(tab, /splunk-table.*Splunk searches/, false, 'the host asks about Splunk searches');
  await s.locator('#status', { hasText: 'Search failed' }).waitFor({ timeout: 5000 }).catch(() => {});
  ok(/did not allow/.test(await s.locator('#status').textContent()), 'refusing stops the search');

  console.log('== allowed: the table shows every row and is sent on');
  await post('/api/grants', { app: 'splunk-table', channel: 'splunk', mode: 'use', decision: 'ask' });
  await tab.reload(); await sleep(2500);
  ({ s, a, rw, k } = partsOf(tab));
  ok((await s.locator('#spl').inputValue()).includes('stats avg(tput)'), 'the search was remembered');
  await press(s, '#btnRun');
  await answer(tab, /splunk-table.*Splunk searches/, true, 'asked about searches');
  await answer(tab, /send messages on the channel splunk\.table/, true, 'then asked about sending the table');
  await s.locator('#status', { hasText: 'rows from Splunk' }).waitFor({ timeout: 5000 });
  ok(/^8 rows from Splunk, kept in this app's database\.$/.test(await s.locator('#status').textContent()), 'search says: ' + await s.locator('#status').textContent());
  await k.locator('#status', { hasText: 'Sent' }).waitFor({ timeout: 5000 }).catch(() => {});
  ok(/^Sent to other apps\.$/.test(await k.locator('#status').textContent()), 'keep sends it on at once: ' + await k.locator('#status').textContent());
  ok(await rw.locator('#out tbody tr').count() === 8 && (await rw.locator('#out th').allTextContents()).join(',') === 'concurrency,x,r', 'the table shows 8 rows of concurrency, x, r');
  const bar = s.locator('wardian-progress');
  ok(await bar.getAttribute('state') === 'done' && /^8 rows in \d+ s$/.test(await bar.getAttribute('label')) && await bar.getAttribute('role') === 'progressbar',
    'the standard progress bar ends as done: ' + await bar.getAttribute('label'));
  ok(/stats avg\(tput\)/.test(await a.locator('#facts').textContent()) && /Last 24 hours/.test(await a.locator('#facts').textContent()), 'with the search and the time range above it');
  await inView(tab, rw, '#out th:first-child');
  await rw.locator('#out th', { hasText: 'concurrency' }).dispatchEvent('click');
  await rw.locator('#out th .arrow', { hasText: '▲' }).waitFor({ timeout: 5000 });
  await rw.locator('#out th', { hasText: 'concurrency' }).dispatchEvent('click');
  // Sorting runs in the database, so the page redraws when the answer arrives.
  await rw.locator('#out tbody tr:first-child td:first-child', { hasText: /^64$/ }).waitFor({ timeout: 5000 }).catch(() => {});
  ok(await rw.locator('#out tbody tr:first-child td:first-child').textContent() === '64', 'a header click sorts, again reverses');

  console.log('== Save search and Save table');
  // window.prompt asks for the name; the test answers it as a person would.
  tab.once('dialog', d => d.accept('My load test'));
  await press(s, '#btnSaveSearch');
  await s.locator('#savedSearches li', { hasText: 'My load test' }).waitFor({ timeout: 5000 }).catch(() => {});
  ok(/My load test/.test(await s.locator('#savedSearches').textContent()) && /Last 24 hours/.test(await s.locator('#savedSearches').textContent()), 'the search is listed under Saved searches');
  await s.locator('#spl').fill('index=other');
  await press(s, '#savedSearches li button[aria-label="Use My load test"]');
  ok((await s.locator('#spl').inputValue()).includes('stats avg(tput)') && /Loaded the search/.test(await s.locator('#status').textContent()), 'Use puts it back in the form');
  tab.once('dialog', d => d.accept('Eight rows'));
  await press(k, '#btnKeep');
  await k.locator('#savedTables li', { hasText: 'Eight rows' }).waitFor({ timeout: 5000 }).catch(() => {});
  ok(/Eight rows/.test(await k.locator('#savedTables').textContent()) && /8 rows · saved/.test(await k.locator('#savedTables').textContent()), 'Save table lists a copy: ' + await k.locator('#savedTables').textContent());
  await sleep(500);                                   // the host writes storage in the background
  const kept = await (await fetch(B + '/api/state/apps/splunk-table')).json();
  ok(kept.search.savedSearches[0].name === 'My load test' && kept.keep.savedTables[0].name === 'Eight rows' && !kept.table, 'each part keeps its own: search the searches, keep the tables');
  const saved = await post('/api/db/tables', { package: 'splunk-table', app: 'keep' });
  ok(saved.body.tables.some(x => /^saved_/.test(x.name) && x.rows === 8), 'the saved table is a copy in the database');

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
  // Wait for the chart rather than a fixed time: Playwright's headless Chromium can take longer.
  await frameOf(lab, 'chart').locator('#chart circle.pt').nth(7).waitFor({ timeout: 10000 }).catch(() => {});
  await sleep(500);
  const data = await inputs.locator('#data').inputValue();
  ok(data.split('\n')[0] === '# threads, req/s, response time (ms)' && data.split('\n').length === 9, 'measurements filled, with a readable header');
  ok(await frameOf(lab, 'chart').locator('#chart circle.pt').count() === 8, 'the chart shows the 8 rows');
  let aboutText = await frameOf(lab, 'chart').locator('#about').textContent();
  ok(/Splunk search/.test(aboutText) && /The column “concurrency”/.test(aboutText) && /Splunk table app/.test(aboutText), 'the chart says where the data came from');

  console.log('== a ready-made search: the lab updates live, with labels and the minutes column');
  await s.locator('#preset').selectOption('traffic');
  ok((await s.locator('#spl').inputValue()).includes('table n x r minutes') && await s.locator('#range').inputValue() === '-7d|', 'the traffic template search keeps the minutes column, over 7 days');
  await press(s, '#btnRun');
  await s.locator('#status', { hasText: '6 rows from Splunk' }).waitFor({ timeout: 5000 }).catch(async () => console.log('STATUS', await s.locator('#status').textContent()));
  ok(await tab.locator('.wardian-perm').count() === 0, 'no second question');
  await rw.locator('#out th', { hasText: 'minutes' }).waitFor({ timeout: 5000 }).catch(() => {});
  ok((await rw.locator('#out th').allTextContents()).join(',') === 'n,x,r,minutes', 'the minutes behind each row are visible');
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

  console.log('== a saved table opens again, in every part');
  await press(k, '#savedTables li button[aria-label="Open Eight rows"]');
  await a.locator('#facts', { hasText: 'Eight rows' }).waitFor({ timeout: 5000 }).catch(() => {});
  ok(/Eight rows/.test(await a.locator('#facts').textContent()) && /Saved/.test(await a.locator('#facts').textContent()), 'about names the saved table and when it was saved');
  await rw.locator('#out th', { hasText: 'concurrency' }).waitFor({ timeout: 5000 }).catch(() => {});
  ok(await rw.locator('#out tbody tr').count() === 8 && /Opened the saved table/.test(await k.locator('#status').textContent()), 'rows shows its 8 rows again');

  console.log('== a large table: 50,000 rows in the database, paged, sorted, filtered, and read by the lab');
  await s.locator('#preset').selectOption('');
  await s.locator('#spl').fill('index=big bigtable | table n host status ms');
  await s.locator('#title').fill('Big table');
  await press(s, '#btnRun');
  await s.locator('#status', { hasText: '50,000 rows from Splunk' }).waitFor({ timeout: 60000 });
  await rw.locator('#pageInfo', { hasText: 'of 50,000' }).waitFor({ timeout: 10000 });
  ok(await rw.locator('#out tbody tr').count() === 100 && /Rows 1–100 of 50,000/.test(await rw.locator('#pageInfo').textContent()), 'the page shows 100 of 50,000 rows');
  await k.locator('#status', { hasText: '50,000' }).waitFor({ timeout: 10000 }).catch(() => {});
  ok(/all 50,000 rows, which they read/.test(await k.locator('#status').textContent()), 'the channel carries a reference to all of them');
  // The pager sits below a hundred rows; Chrome does not draw a sandboxed frame's parts out of
  // sight, so scroll to it first, as a person would.
  await inView(tab, rw, '#next'); await press(rw, '#next');
  await rw.locator('#pageInfo', { hasText: 'Rows 101–200' }).waitFor({ timeout: 5000 }).catch(() => {});
  ok(await rw.locator('#out tbody tr:first-child td:first-child').textContent() === '101', 'Next shows rows 101–200');
  await inView(tab, rw, '#prev'); await press(rw, '#prev');
  await rw.locator('#pageInfo', { hasText: 'Rows 1–100' }).waitFor({ timeout: 5000 }).catch(() => {});
  ok(await rw.locator('#out tbody tr:first-child td:first-child').textContent() === '1', 'Previous goes back');
  await inView(tab, rw, '#out th:nth-child(4)');
  await rw.locator('#out th', { hasText: 'ms' }).dispatchEvent('click');
  await rw.locator('#out th .arrow', { hasText: '▲' }).waitFor({ timeout: 5000 });
  await rw.locator('#out th', { hasText: 'ms' }).dispatchEvent('click');
  await rw.locator('#out tbody tr:first-child td:nth-child(4)', { hasText: /^909$/ }).waitFor({ timeout: 5000 }).catch(() => {});
  ok(await rw.locator('#out tbody tr:first-child td:nth-child(4)').textContent() === '909', 'sorting all 50,000 rows by ms, largest first, is done by the database');
  // Save as web page (ADR-2610080905): the rows part puts every row in the file, up to 10,000.
  // Opened the way a user opens it, from Wardian's page, where the button is.
  const host = await context.newPage();
  await host.goto(B + '/');
  await host.locator('#apps li button', { hasText: /Splunk table/i }).first().click();
  await host.locator('#saveWebBtn').waitFor({ timeout: 10000 });
  await sleep(4000);                                  // every part starts and shows the table
  await host.click('#saveWebBtn');
  const [dl] = await Promise.all([host.waitForEvent('download', { timeout: 60000 }), host.click('#saveWebGo')]);
  const webFile = require('path').join(require('os').tmpdir(), 'splunk-web-' + process.pid + '.html');
  await dl.saveAs(webFile);
  const web = require('fs').readFileSync(webFile, 'utf8'); require('fs').unlinkSync(webFile);
  const webRows = (web.match(/<tr[ >]/g) || []).length - 1;
  await host.close();
  ok(webRows === 10000 && /10,000/.test(web) && !/<script/i.test(web), 'Save as web page holds 10,000 of the 50,000 rows, says so, and has no script', '(' + webRows + ' rows)');
  let expected = 0;
  for (let i = 0; i < 50000; i++) if (i % 10 === 0 && i % 7 === 3) expected++;
  await rw.locator('#find').fill('status=500 web-3');
  await rw.locator('#findHint', { hasText: expected.toLocaleString() + ' of 50,000 rows match' }).waitFor({ timeout: 5000 }).catch(() => {});
  ok((await rw.locator('#findHint').textContent()).startsWith(expected.toLocaleString() + ' of 50,000 rows match'), 'Find in results filters in the database: ' + await rw.locator('#findHint').textContent());
  await sleep(300);                                   // the find text reaches keep through the kernel
  await inView(tab, k, '#btnCsv');
  const [csvFile] = await Promise.all([tab.waitForEvent('download', { timeout: 20000 }), k.locator('#btnCsv').dispatchEvent('click')]);
  const csvLines = require('fs').readFileSync(await csvFile.path(), 'utf8').trim().split('\n');
  ok(csvLines.length === expected + 1 && csvLines.slice(1).every(l => l.includes('web-3') && l.includes(',500,')), 'the CSV holds exactly the matching rows: ' + (csvLines.length - 1));
  await inputs.locator('#tblName', { hasText: '50,000 rows' }).waitFor({ timeout: 10000 });
  await inputs.locator('#colN').selectOption('n');
  await inputs.locator('#colX').selectOption('ms');
  await inputs.locator('#colR').selectOption('');
  await useTable(inputs);
  await answer(lab, /usl-lab wants to read the tables of splunk-table/, true, 'the lab asks once to read the table app\'s tables');
  await inputs.locator('#tblStatus', { hasText: 'Using 10,000 rows' }).waitFor({ timeout: 60000 }).catch(() => {});
  ok(/Using 10,000 rows \(the first 10,000 of 50,000; the fit reads at most 10,000\)/.test(await inputs.locator('#tblStatus').textContent()), 'the lab reads the table in pages, up to its 10,000-row cap, and says so: ' + await inputs.locator('#tblStatus').textContent());
  ok((await inputs.locator('#data').inputValue()).split('\n').length === 10001, 'and they fill its measurements');
  const undeclared = await post('/api/db/page', { package: 'usl-lab', app: 'chart', table: 'search', source: 'splunk-table' });
  ok(/does not declare/.test(undeclared.body.error || ''), 'an app that does not declare db is refused on the server');

  console.log('== long loads run as background jobs (ADR-2610072118)');
  // A slow fake search ("slowtable") stays running for 12 s. Meanwhile the app list and another app
  // load as usual, the user leaves the table app, and comes back to find the load finished.
  const home = await context.newPage();
  home.on('pageerror', e => pageErrors.push(e.message));
  await home.goto(B + '/');
  await home.locator('#apps button', { hasText: 'Splunk table' }).click();
  await sleep(2500);
  let ht = frameOf(home, 'search');
  await ht.locator('#preset').selectOption('');
  await ht.locator('#spl').fill('index=big slowtable | table n host');
  await ht.locator('#title').fill('Slow table');
  await ht.locator('#btnRun').dispatchEvent('click');   // a click on a fresh sandboxed frame can be lost
  await sleep(1000);
  ok(await ht.locator('wardian-progress').getAttribute('state') !== 'done' && /Searching Splunk/.test(await ht.locator('wardian-progress').getAttribute('label')), 'the slow load is running');
  // Six more long searches from this browser: each answers at once, so none holds a connection.
  const started = await home.evaluate(async () => Promise.all([...Array(6)].map(async () => {
    const t = Date.now();
    const r = await fetch('/api/splunk/search', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ package: 'splunk-table', app: 'search', search: 'index=x slowtable', background: true }) });
    return { ms: Date.now() - t, body: await r.json() };
  })));
  ok(started.every(x => /^j\d+-/.test(x.body.job || '') && x.ms < 2000), 'six more long searches start at once: ' + started.map(x => x.ms + ' ms').join(', '));
  let mine = (await (await fetch(B + '/api/jobs?package=splunk-table')).json()).jobs;
  ok(mine.filter(j => j.state === 'running').length === 7 && mine.some(j => j.kind === 'splunk.into' && j.label === 'index=big slowtable | table n host'), 'seven jobs run for the table app');
  ok((await (await fetch(B + '/api/jobs?package=usl-lab')).json()).jobs.length === 0, 'another package sees none of them');
  ok((await fetch(B + '/api/jobs/' + mine[0].id + '?package=usl-lab')).status === 400, 'nor any one of them');
  let at = Date.now();
  await home.reload();
  await home.locator('#apps button', { hasText: 'USL' }).waitFor({ timeout: 10000 });
  const listMs = Date.now() - at;
  ok(listMs < 4000, 'the app list loads while they run: ' + listMs + ' ms');
  at = Date.now();
  await home.locator('#apps button', { hasText: 'USL' }).click();
  let labInputs = null;
  for (let i = 0; i < 100 && !labInputs; i++) { await sleep(100); labInputs = frameOf(home, 'inputs'); }
  // The lab is running once it has the latest table from the channel.
  await labInputs.locator('#tblName', { hasText: 'rows' }).waitFor({ timeout: 10000 });
  const labMs = Date.now() - at;
  ok(labMs < 5000, 'and another app opens: ' + labMs + ' ms');
  await home.locator('#jobsBtn', { hasText: 'running' }).waitFor({ timeout: 5000 }).catch(() => {});
  ok(/7 running/.test(await home.locator('#jobsBtn').textContent()), 'the jobs badge counts them: ' + await home.locator('#jobsBtn').textContent());
  await home.locator('#jobsBtn').click();
  await home.locator('#jobsPanel').waitFor({ state: 'visible', timeout: 3000 });
  const rows = await home.locator('#jobsList li[data-state="running"]').allTextContents();
  ok(rows.length === 7 && rows.some(r => /Splunk table/.test(r) && /slowtable/.test(r) && /Splunk load into a table/.test(r)), 'the list names the app, the search and the kind');
  ok(await home.locator('#jobsList li button', { hasText: 'Cancel' }).count() === 7, 'each running job has a Cancel button');
  await home.locator('#jobsClose').click();
  await home.locator('#jobsBtn[data-state="finished"]').waitFor({ timeout: 25000 }).catch(() => {});
  ok(/done/.test(await home.locator('#jobsBtn').textContent()) && /Splunk table finished/.test(await home.locator('#jobsBtn').getAttribute('aria-label')),
    'when the load finishes while its app is not open, the badge says so: ' + await home.locator('#jobsBtn').getAttribute('aria-label'));
  await home.locator('#jobsBtn').click();
  await sleep(2500);
  ht = frameOf(home, 'search');
  await ht.locator('#status', { hasText: '300 rows from Splunk' }).waitFor({ timeout: 10000 }).catch(() => {});
  ok(/300 rows from Splunk, kept in this app's database/.test(await ht.locator('#status').textContent()), 'pressing it opens the table app with the load finished: ' + await ht.locator('#status').textContent());
  await frameOf(home, 'rows').locator('#pageInfo', { hasText: 'of 300' }).waitFor({ timeout: 5000 }).catch(() => {});
  ok(/Rows 1–100 of 300/.test(await frameOf(home, 'rows').locator('#pageInfo').textContent()) && /Slow table/.test(await frameOf(home, 'about').locator('#facts').textContent()), 'with its table and its name');
  ok(mine.every(j => j.app === 'search'), 'the jobs belong to the search part');
  ok(!/done/.test(await home.locator('#jobsBtn').textContent()), 'and the notice is gone');

  console.log('== a cancel stops the load, and the search on Splunk');
  const cancelledBefore = (await (await fetch(SPLUNK + '/fake/cancelled')).json()).length;
  await ht.locator('#btnRun').dispatchEvent('click');   // a click on a fresh sandboxed frame can be lost
  await sleep(1500);
  await home.locator('#jobsBtn').click();
  await home.locator('#jobsPanel').waitFor({ state: 'visible', timeout: 3000 });
  const live = home.locator('#jobsList li[data-state="running"]');
  await live.first().waitFor({ timeout: 5000 }).catch(() => {});
  ok(await live.count() === 1, 'the new load is in the list');
  await live.locator('button', { hasText: 'Cancel' }).click();
  for (let i = 0; i < 50 && (await (await fetch(SPLUNK + '/fake/cancelled')).json()).length === cancelledBefore; i++) await sleep(200);
  ok((await (await fetch(SPLUNK + '/fake/cancelled')).json()).length === cancelledBefore + 1, 'the search job on Splunk was cancelled');
  await ht.locator('#status', { hasText: 'You stopped the search' }).waitFor({ timeout: 8000 }).catch(() => {});
  ok(/You stopped the search/.test(await ht.locator('#status').textContent()) && await ht.locator('wardian-progress').getAttribute('state') === 'error', 'the app says it was stopped: ' + await ht.locator('#status').textContent());
  mine = (await (await fetch(B + '/api/jobs?package=splunk-table')).json()).jobs;
  ok(mine[0].state === 'cancelled', 'and the job is cancelled');
  await home.locator('#jobsClose').click();
  // The app's own Stop button does the same.
  await ht.locator('#btnRun').dispatchEvent('click');   // a click on a fresh sandboxed frame can be lost
  await sleep(1500);
  await ht.locator('wardian-progress button', { hasText: 'Stop' }).dispatchEvent('click');
  await ht.locator('#status', { hasText: 'You stopped the search' }).waitFor({ timeout: 8000 }).catch(() => {});
  ok(/You stopped the search/.test(await ht.locator('#status').textContent()), 'the progress bar\'s Stop button cancels too');
  ok((await home.evaluate(() => [...document.querySelectorAll('iframe')].map(f => f.contentWindow.Kernel && f.contentWindow.Kernel.faults()).filter(Boolean).flat())).length === 0, 'no kernel faults in the app list\'s table app');
  await home.close();

  console.log('== Splunk errors are shown');
  await s.locator('#spl').fill('| badsyntax');
  await press(s, '#btnRun');
  await s.locator('#status', { hasText: 'Unknown search command' }).waitFor({ timeout: 5000 }).catch(() => {});
  ok(/Unknown search command/.test(await s.locator('#status').textContent()), 'Splunk\'s own error is shown');
  ok(await s.locator('wardian-progress').getAttribute('state') === 'error', 'and the progress bar shows the failure');

  console.log('== without an Anthropic key, no AI anywhere');
  ok(await frameHeight(tab, 'ask') === 0, 'no Write with AI: the ask panel is folded away');
  ok(await lab.locator('iframe[title=diagnosis]').evaluate(el => el.offsetHeight) === 0, 'no diagnosis panel');

  console.log('== with Claude set up (' + (process.env.PROVIDER || 'anthropic') + '): Claude writes the search, and the diagnosis works');
  if (process.env.PROVIDER === 'bedrock') {
    r = await post('/api/ai/provider', { provider: 'bedrock', region: 'us-east-1', auth: 'api-key', token: 'wrong-token' });
    ok(r.status === 400 && /refused the sign-in|security token/.test(r.body.error), 'a wrong Bedrock key is refused and not saved: ' + r.body.error);
    r = await post('/api/ai/provider', { provider: 'bedrock', region: 'us-east-1', auth: 'api-key' });
    ok(r.status === 400 && /API key/.test(r.body.error), 'a Bedrock key is needed');
    r = await post('/api/ai/provider', { provider: 'bedrock', region: 'Not A Region', auth: 'api-key', token: 'test-bedrock-token' });
    ok(r.status === 400 && /region/.test(r.body.error), 'a malformed region is refused');
    r = await post('/api/ai/provider', { provider: 'bedrock', region: 'us-east-1', auth: 'api-key', token: 'test-bedrock-token' });
    ok(r.status === 200 && r.body.ready && r.body.provider === 'bedrock' && r.body.bedrock.settings.auth === 'api-key', 'the Bedrock key is tested and saved');
    const st2 = await (await fetch(B + '/api/status')).json();
    ok(!JSON.stringify(st2).includes('test-bedrock-token'), 'status never shows the Bedrock key');
    r = await post('/api/ai/provider', { provider: 'bedrock', region: 'us-west-2', auth: 'api-key' });
    ok(r.status === 200 && r.body.bedrock.settings.region === 'us-west-2', 'the region changes without typing the key again');
    r = await post('/api/ai/provider', { provider: 'bedrock', region: 'us-east-1', auth: 'api-key' });
  } else {
    r = await post('/api/ai/key', { key: 'org-key' });
    ok(r.status === 400 && /anthropic-workspace-id/.test(r.body.error), 'a key without a workspace is refused, with the API\'s reason');
    r = await post('/api/ai/key', { key: 'org-key', workspace: 'wrkspc_test' });
    ok(r.status === 200 && r.body.ready && r.body.workspace === 'wrkspc_test', 'the same key with a workspace ID is accepted');
    r = await post('/api/ai/key', { key: '', workspace: 'bad id!' });
    ok(r.status === 400 && /workspace ID/.test(r.body.error), 'a malformed workspace ID is refused');
    r = await post('/api/ai/key', { key: 'test-key', workspace: '' });
    ok(r.status === 200 && r.body.ready && r.body.workspace === null, 'the key is tested and saved; an empty workspace removes it');
  }
  r = await post('/api/ai/sample', { package: 'splunk-table', app: 'ask', prompt: 'hi' });
  ok(/^not_granted/.test(r.body.error), 'no AI before the user allows it');
  r = await post('/api/ai/sample', { package: 'usl-lab', app: 'chart', prompt: 'hi' });
  ok(/does not declare/.test(r.body.error), 'an app without claude:sample is refused');
  await tab.reload(); await sleep(2500);
  ({ s, a, rw, k } = partsOf(tab));
  const ai = frameOf(tab, 'ask');
  await ai.locator('#btnAi').waitFor({ state: 'visible', timeout: 5000 }).catch(() => {});
  ok(await frameHeight(tab, 'ask') > 40 && await ai.locator('#btnAi').isVisible(), 'the ask panel shows, with Write with AI');
  await ai.locator('#aiWhat').fill('throughput of the load test as users grow');
  await press(ai, '#btnAi');
  await answer(tab, /Claude AI requests/, true, 'the host asks about Claude');
  await s.locator('#status', { hasText: 'rows from Splunk' }).waitFor({ timeout: 10000 });
  ok((await s.locator('#spl').inputValue()).startsWith('index="loadtest" sourcetype="jmeter"'), 'Claude\'s search is in the search part\'s box');
  ok(/Average throughput at each concurrency step/.test(await s.locator('#status').textContent()), 'with its explanation');
  ok(await s.locator('#title').inputValue() === 'JMeter load test steps', 'and its name for the table');
  ok(/Claude wrote the search/.test(await ai.locator('#status').textContent()) && await ai.locator('wardian-progress').getAttribute('state') === 'done', 'ask says it handed the search over');
  const prompts = await (await fetch(ANTHROPIC + '/prompts')).json();
  ok(prompts.length === 2 && prompts[0].model.includes('haiku'), 'two requests: a quick pick, then the search');
  if (process.env.PROVIDER === 'bedrock') ok(prompts[0].model.startsWith('us.anthropic.') && (await (await fetch(process.env.BEDROCK + '/seen')).json()).includes('bearer'), 'the requests went through Bedrock with its model ids and the bearer key');
  ok(!JSON.stringify(prompts).includes('ann.lee@example.com') && JSON.stringify(prompts).includes('<email>'), 'email addresses never reach Claude');
  await inputs.locator('#tblName', { hasText: 'JMeter' }).waitFor({ timeout: 5000 }).catch(() => {});
  await useTable(inputs);
  // The lab may still be fitting the 50,000 rows of the step before; wait for the new labels.
  const labelsAt = Date.now();
  await frameOf(lab, 'chart').locator('#about', { hasText: 'JMeter load test steps' }).waitFor({ timeout: 30000 }).catch(() => {});
  aboutText = await frameOf(lab, 'chart').locator('#about').textContent();
  ok(/JMeter load test steps/.test(aboutText) && /Claude wrote this search/.test(aboutText), 'Claude\'s labels reach the lab (after ' + (Date.now() - labelsAt) + ' ms)');

  await lab.reload(); await sleep(3000);
  const diag = frameOf(lab, 'diagnosis');
  ok(await lab.locator('iframe[title=diagnosis]').evaluate(el => el.offsetHeight) > 0, 'the diagnosis panel shows');
  // Chrome stops drawing an off-screen sandboxed frame, so Playwright cannot click in it: scroll to it first.
  await lab.locator('iframe[title=diagnosis]').scrollIntoViewIfNeeded(); await sleep(300);
  await diag.locator('#btnAi').click();
  await answer(lab, /usl-lab.*Claude AI requests/, true, 'the lab asks about Claude for itself');
  await diag.locator('#aiOut h3').first().waitFor({ timeout: 10000 });
  ok(/What the curve says/.test(await diag.locator('#aiOut').textContent()), 'Interpret results writes a diagnosis');

  console.log('== Arrange: a viewer can hide the ask panel');
  await tab.click('.w-arrange-btn'); await sleep(400);
  await tab.locator('[data-arrange-panel=ask] > .w-arrange-cover button[data-a=hide]').click();
  await tab.click('.w-arrange-bar button.yes');
  ok(await tab.locator('[data-arrange-panel=ask]').isHidden() && await tab.locator('[data-arrange-panel=search]').isVisible(), 'ask is hidden, search still shows');
  await tab.reload(); await sleep(2500);
  ok((await panels(tab)).join() === 'search,ask(hidden),about,rows,keep' && await tab.locator('[data-arrange-panel=ask]').isHidden(), 'and stays hidden after a reload: ' + (await panels(tab)).join());
  ({ s, a, rw, k } = partsOf(tab));
  await rw.locator('#out th', { hasText: 'concurrency' }).waitFor({ timeout: 5000 }).catch(() => {});
  ok(await rw.locator('#out tbody tr').count() > 0 && /JMeter/.test(await a.locator('#facts').textContent()), 'the other parts still show the last table');

  console.log('== without Splunk set up, the button explains');
  await post('/api/splunk/config', { url: '' });
  await tab.reload(); await sleep(2500);
  s = frameOf(tab, 'search');
  ok(await s.locator('#btnRun').isDisabled(), 'Run search is disabled');
  ok(/Settings → Splunk/.test(await s.locator('#status').textContent()), 'and says where to set it up');

  ok(pageErrors.length === 0, 'no page errors ' + JSON.stringify(pageErrors));
  for (const p of [tab, lab]) ok((await p.evaluate(() => Kernel.faults())).length === 0, 'no kernel faults in ' + new URL(p.url()).pathname);
  await browser.close();
  console.log(`\n${pass} passed, ${fail} failed`);
  process.exit(fail ? 1 : 0);
})().catch(e => { console.error(e); process.exit(1); });
