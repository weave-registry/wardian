// Serves this folder from a plain static server, which has no /apps/, so whatever loads came
// from the service worker. Prints each frame's report and what the static server was asked
// for, and fails if a finding in ../README.md no longer holds.
// Usage: NODE_PATH=$(npm root -g) node spikes/sealed-frames/run.js
const { chromium } = require('playwright');
const { spawn } = require('child_process');
(async () => {
  const srv = spawn('python3', ['-u', '-m', 'http.server', '8791', '--bind', '127.0.0.1', '--directory', __dirname]);
  let log = '';
  srv.stderr.on('data', (d) => (log += d));
  await new Promise((r) => setTimeout(r, 800));
  const browser = await chromium.launch({ executablePath: process.env.CHROME || '/opt/pw-browsers/chromium-1194/chrome-linux/chrome' });
  const page = await browser.newPage();
  await page.goto('http://127.0.0.1:8791/index.html');
  await page.waitForFunction(() => window.__results, null, { timeout: 30000 });
  const r = await page.evaluate(() => window.__results);
  await browser.close();
  srv.kill();
  console.log(JSON.stringify(r, null, 1));
  console.log('static server was asked for:\n' + log.split('\n').filter((l) => l.includes('"GET')).map((l) => '  ' + l.replace(/^.*"GET (\S+).*" (\d+).*$/, '$1 $2')).join('\n'));
  const want = {
    'A: a sealed frame is never handed to the service worker': r.A_sandboxed_src === 'no report within 5s' && log.includes('GET /apps/demo/index.html'),
    'B: an unsealed frame works but can read the host\'s storage': r.B_same_origin_src.script && r.B_same_origin_src.storage === 'readable',
    'C: a built, sealed frame runs its script and reads its data': r.C_sandboxed_built.script && r.C_sandboxed_built.data === 'from the package',
    'C: and stays sealed (no storage, no outside, opaque origin)': r.C_sandboxed_built.storage === 'blocked' && r.C_sandboxed_built.outside === 'blocked' && r.C_sandboxed_built.origin === 'null',
  };
  for (const [k, ok] of Object.entries(want)) console.log((ok ? 'ok    ' : 'FAIL  ') + k);
  process.exit(Object.values(want).every(Boolean) ? 0 : 1);
})();
