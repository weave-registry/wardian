// Loads the spike's .wasm in headless Chromium, calls each probe, and fails if a finding in
// ../../README.md no longer holds.
// Usage: NODE_PATH=$(npm root -g) node spikes/wasm-core/probe/run.js <file.wasm>
const { chromium } = require('playwright');
const fs = require('fs');
(async () => {
  const bytes = [...fs.readFileSync(process.argv[2])];
  const browser = await chromium.launch({ executablePath: process.env.CHROME || '/opt/pw-browsers/chromium-1194/chrome-linux/chrome' });
  const page = await browser.newPage();
  const r = await page.evaluate(async (bytes) => {
    const { instance } = await WebAssembly.instantiate(new Uint8Array(bytes), {});
    const res = {};
    for (const name of ['probe_check', 'probe_frame', 'probe_now', 'probe_thread']) {
      try { res[name] = String(instance.exports[name]()); } catch (e) { res[name] = 'TRAP: ' + e.message; }
    }
    return res;
  }, bytes);
  await browser.close();
  console.log(JSON.stringify(r, null, 1));
  const want = {
    'wardian check runs, and passes a minimal module app': r.probe_check === '0',
    'a suite part\'s frame document is built': Number(r.probe_frame) > 0,
    'the clock traps (needs a Clock port)': r.probe_now.startsWith('TRAP'),
    'a thread traps (needs a job-runner port)': r.probe_thread.startsWith('TRAP'),
  };
  for (const [k, ok] of Object.entries(want)) console.log((ok ? 'ok    ' : 'FAIL  ') + k);
  process.exit(Object.values(want).every(Boolean) ? 0 : 1);
})();
