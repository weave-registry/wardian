// Takes the core built by `cargo build --release --target wasm32-unknown-unknown` in
// ../wasm-core (see ../README.md). Serves this folder from a plain static server, loads the page twice (phase 1 plain, phase 2 after the service worker adds the
// isolation headers), prints both reports, and fails if a finding in ../README.md stops holding.
// Usage: NODE_PATH=$(npm root -g) node spikes/browser-runtime/run.js
const { chromium } = require('playwright');
const { spawn } = require('child_process');
const fs = require('fs');
const path = require('path');
(async () => {
  const built = path.join(__dirname, '../wasm-core/target/wasm32-unknown-unknown/release/wardian_wasm_core_spike.wasm');
  if (!fs.existsSync(built)) { console.error('build the core first: (cd spikes/wasm-core && cargo build --release --target wasm32-unknown-unknown)'); process.exit(2); }
  fs.copyFileSync(built, path.join(__dirname, 'core.wasm'));
  const srv = spawn('python3', ['-m', 'http.server', '8793', '--bind', '127.0.0.1', '--directory', __dirname], { stdio: 'ignore' });
  await new Promise((r) => setTimeout(r, 800));
  const browser = await chromium.launch({ executablePath: process.env.CHROME || '/opt/pw-browsers/chromium-1194/chrome-linux/chrome' });
  const page = await browser.newPage();
  const out = {};
  for (const phase of ['1', '2']) {
    await page.goto('http://127.0.0.1:8793/index.html?phase=' + phase);
    await page.waitForFunction(() => window.__results, null, { timeout: 30000 });
    out[phase] = await page.evaluate(() => window.__results);
  }
  await browser.close(); srv.kill();
  console.log(JSON.stringify(out, null, 1));
  const [a, b] = [out['1'], out['2']];
  const want = {
    'plain host: not isolated, so no SharedArrayBuffer and sleep can only busy-wait': !a.cross_origin_isolated && !a.worker.shared_array_buffer && a.worker.sleep_by === 'busy-wait',
    'headers from the service worker isolate the page': b.cross_origin_isolated === true && b.worker.shared_array_buffer,
    'isolated: the clock port sleeps with Atomics.wait, for the time asked': b.worker.sleep_by === 'atomics' && Math.abs(b.worker.sleep_200_took_ms - 200) < 50,
    'the real job runner runs a job through the browser clock and task queue': b.worker.job_started === 1 && b.worker.job_state_before === 1 && b.worker.tasks_ran === 1 && b.worker.job_state_after === 2 && b.worker.tasks_took_ms >= 300,
    'wardian check and a suite frame run in the worker': b.worker.check_errors === 0 && b.worker.frame_bytes > 0,
    'a worker can block on the network (sync XHR) and on files (OPFS sync handle)': b.worker.sync_xhr === 'from the static server' && b.worker.opfs_sync === 'kept in OPFS',
    'one worker is one thread: a request waits for the running job': b.ping_during_job_ms >= 200,
    'a sealed frame still runs, opaque and without storage, under isolation': b.sealed_frame.origin === 'null' && b.sealed_frame.storage === 'blocked',
  };
  for (const [k, ok] of Object.entries(want)) console.log((ok ? 'ok    ' : 'FAIL  ') + k);
  process.exit(Object.values(want).every(Boolean) ? 0 : 1);
})();
