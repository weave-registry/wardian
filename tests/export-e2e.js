// End-to-end test of Download this app and of importing the file into another Wardian (ADR-2610071248).
const { chromium } = require('playwright');
const fs = require('fs'), path = require('path'), zlib = require('zlib');
const A = process.env.A, B = process.env.B, BDATA = process.env.BDATA, TMP = process.env.TMPDIR_E2E;
let pass = 0, fail = 0;
const ok = (c, m) => { if (c) { pass++; console.log('  ok  ', m); } else { fail++; console.log('  FAIL', m); } };
const sleep = ms => new Promise(r => setTimeout(r, ms));
const post = (base, p, body) => fetch(base + p, { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(body) }).then(r => r.json());

// The names and contents of a zip's entries, read with Node alone (stored or deflated entries).
function unzip(buf) {
  const out = [];
  let eocd = buf.lastIndexOf(Buffer.from([0x50, 0x4b, 0x05, 0x06]));
  const count = buf.readUInt16LE(eocd + 10);
  let p = buf.readUInt32LE(eocd + 16);
  for (let i = 0; i < count; i++) {
    const method = buf.readUInt16LE(p + 10), size = buf.readUInt32LE(p + 20), nlen = buf.readUInt16LE(p + 28), elen = buf.readUInt16LE(p + 30), clen = buf.readUInt16LE(p + 32), local = buf.readUInt32LE(p + 42);
    const name = buf.slice(p + 46, p + 46 + nlen).toString();
    const lnlen = buf.readUInt16LE(local + 26), lelen = buf.readUInt16LE(local + 28);
    const data = buf.slice(local + 30 + lnlen + lelen, local + 30 + lnlen + lelen + size);
    out.push({ name, body: method === 8 ? zlib.inflateRawSync(data) : data });
    p += 46 + nlen + elen + clen;
  }
  return out;
}

(async () => {
  console.log('== A: an app with data');
  await post(A, '/api/state/apps/splunk-table', { app: 'search', key: 'state', value: { s: 'index=web | stats count', name: 'Checkout errors' } });
  await post(A, '/api/state/layout/splunk-table', { layout: { v: 1, mode: 'one', columns: { main: ['rows'] }, hidden: [] } });
  const ins = await post(A, '/api/db/insert', { package: 'splunk-table', app: 'keep', table: 'errors', columns: ['host', 'n'], rows: [['web-1', 3], ['web-2', 5], ['web-3', 8]], create: true });
  ok(ins.inserted === 3, 'a table of 3 rows');

  const browser = await chromium.launch(require('./browser')({ headless: true }));
  const ctx = await browser.newContext({ acceptDownloads: true, viewport: { width: 1280, height: 900 } });
  const page = await ctx.newPage();
  const errors = [];
  page.on('pageerror', e => errors.push(e.message));
  await page.goto(A + '/'); await sleep(800);
  await page.locator('#apps li button', { hasText: /Splunk table/i }).first().click(); await sleep(800);
  await page.click('#downloadBtn');
  await page.locator('#exportList strong').first().waitFor({ timeout: 5000 });
  ok(/splunk-table\.wardian · \d+ files/.test(await page.locator('#exportList').textContent()), 'the preview names the file and counts its files');
  ok(!/Your data in this file/.test(await page.locator('#exportList').textContent()), 'no data by default');
  await page.check('#exportData');
  await page.locator('.export-data').waitFor({ timeout: 5000 });
  const dataText = await page.locator('.export-data').textContent();
  ok(/what the app has saved/.test(dataText) && /your Arrange layout/.test(dataText) && /table errors: 3 rows/.test(dataText), 'with data, the preview lists it: ' + dataText.slice(0, 160));
  ok(/Never included: keys and accounts/.test(await page.locator('#exportList').textContent()), 'and says what never goes in');
  const [dl] = await Promise.all([page.waitForEvent('download', { timeout: 10000 }), page.click('#exportGo')]);
  ok(dl.suggestedFilename() === 'splunk-table.wardian', 'Download saves splunk-table.wardian');
  const file = path.join(TMP, 'splunk-table.wardian');
  await dl.saveAs(file);
  const entries = unzip(fs.readFileSync(file));
  ok(entries.every(e => e.name.startsWith('splunk-table/')), 'one package folder at the top');
  ok(['storage.json', 'layout.json', 'tables.sqlite'].every(f => entries.some(e => e.name === 'splunk-table/.wardian/data/' + f)), 'the data is inside');
  const all = entries.map(e => e.body.toString('latin1')).join('\n');
  ok(!/SECRET|"allow":true/.test(all), 'no key, account or permission answer anywhere in the file');
  const head = await fetch(A + '/api/apps/splunk-table/export?data=0');
  ok(head.headers.get('content-type') === 'application/vnd.wardian+zip', 'served as application/vnd.wardian+zip');

  console.log('== B: preview, then import with its data');
  await page.goto(B + '/'); await sleep(800);
  await page.click('#settingsBtn'); await page.click('#setTab-import'); await sleep(300);
  await page.setInputFiles('#zipFile', file);
  await page.locator('#importPreview:not(.hidden)').waitFor({ timeout: 5000 });
  const prev = await page.locator('#importPreview').textContent();
  ok(/Splunk table/.test(prev) && /What it may do/.test(prev) && /Table errors: 3 rows/.test(prev) && /Runs Splunk searches/.test(prev), 'the preview shows the app, what it may use and its data');
  ok(!(await page.isChecked('#importData')), '"Also install its data" is off by default');
  await page.check('#importData');
  await page.click('#importGo');
  await page.locator('#importMsg', { hasText: 'Imported' }).waitFor({ timeout: 10000 });
  const installed = ((await page.locator('#importMsg').textContent()).match(/Its data was installed \(([^)]*)\)/) || [, ''])[1].split(', ').sort().join(',');
  ok(installed === 'layout,storage,tables', 'imported with its data: ' + await page.locator('#importMsg').textContent());
  const st = await (await fetch(B + '/api/state/apps/splunk-table')).json();
  ok(st.search && st.search.state && st.search.state.name === 'Checkout errors', 'B has the same saved data');
  const lay = await (await fetch(B + '/api/state/layout/splunk-table')).json();
  ok(lay && lay.mode === 'one', 'and the same layout');
  const tb = await post(B, '/api/db/tables', { package: 'splunk-table', app: 'keep' });
  ok(tb.tables && tb.tables[0] && tb.tables[0].name === 'errors' && tb.tables[0].rows === 3, 'and the same table');
  ok(!fs.existsSync(path.join(BDATA, 'grants.json')) || !fs.readFileSync(path.join(BDATA, 'grants.json'), 'utf8').includes('splunk-table'), 'no permission answers came along: B asks again');

  console.log('== an app-only import of the same file');
  await post(B, '/api/apps/remove', { name: 'splunk-table' });
  await post(B, '/api/state/apps/splunk-table', { app: 'search', key: 'state', value: null });
  await page.setInputFiles('#zipFile', file);
  await page.locator('#importPreview:not(.hidden)').waitFor({ timeout: 5000 });
  await page.click('#importGo');
  await page.locator('#importMsg', { hasText: 'Imported' }).waitFor({ timeout: 10000 });
  ok(!/Its data was installed/.test(await page.locator('#importMsg').textContent()), 'without the tick, only the app is installed');
  const st2 = await (await fetch(B + '/api/state/apps/splunk-table')).json();
  ok(!(st2.search && st2.search.state), 'and no data');
  ok(errors.length === 0, 'no page errors ' + JSON.stringify(errors));
  await browser.close();
  console.log(`\n${pass} passed, ${fail} failed`);
  process.exit(fail ? 1 : 0);
})().catch(e => { console.error(e); process.exit(1); });
