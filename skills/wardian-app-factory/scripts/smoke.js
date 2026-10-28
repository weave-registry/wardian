#!/usr/bin/env node
// Smoke test for one Wardian package: opens it the way a user would and reports what broke.
//
//   node smoke.js --serve <apps-folder> <app-name>   start a private Wardian on a free port, test, stop it
//   node smoke.js <base-url> <app-name>              test against a Wardian that is already running
//   e.g. node smoke.js --serve apps hello-suite
//
// --serve starts its own Wardian with a throwaway data folder, so it never touches the user's
// server or settings. The program is $WARDIAN_BIN, else target/release/wardian under the current
// folder (a checkout of the Wardian repository), else `wardian` on PATH.
//
// Needs the playwright package (NODE_PATH=$(npm root -g) if installed globally) and Google Chrome.
// Exit status: 0 = no faults or errors, 1 = something failed, 2 = could not run.
const args = process.argv.slice(2);
const serve = args[0] === '--serve';
let [base, name] = serve ? args.slice(1) : args;
if (!base || !name) { console.error('usage: node smoke.js --serve <apps-folder> <app-name>\n       node smoke.js <base-url> <app-name>'); process.exit(2); }

// Starts Wardian on port 0 (the system picks a free one) and reads the real address it prints.
function startServer(appsDir) {
  const { spawn } = require('child_process');
  const fs = require('fs'), os = require('os'), path = require('path');
  const local = path.resolve('target/release/wardian');
  const bin = process.env.WARDIAN_BIN || (fs.existsSync(local) ? local : 'wardian');
  const data = fs.mkdtempSync(path.join(os.tmpdir(), 'wardian-smoke-'));
  const proc = spawn(bin, [appsDir], { env: { ...process.env, ADDR: '127.0.0.1:0', DATA_DIR: data } });
  return new Promise((resolve, reject) => {
    let out = '';
    const timer = setTimeout(() => reject(new Error('Wardian did not start: ' + out)), 10000);
    const read = d => {
      out += d;
      const m = out.match(/listening on (http:\/\/\S+)/);
      if (m) { clearTimeout(timer); resolve({ url: m[1], stop: () => { proc.kill(); fs.rmSync(data, { recursive: true, force: true }); } }); }
    };
    proc.stdout.on('data', read); proc.stderr.on('data', read);
    proc.on('error', e => { console.error(`could not start ${bin} (${e.message}): install Wardian, or set WARDIAN_BIN`); process.exit(2); });
    proc.on('exit', code => reject(new Error(`Wardian exited (${code}): ${out}`)));
  });
}

let chromium;
try { ({ chromium } = require('playwright')); }
catch { console.error('playwright is not installed: npm i -g playwright, then set NODE_PATH=$(npm root -g)'); process.exit(2); }

const sleep = ms => new Promise(r => setTimeout(r, ms));

(async () => {
  let server = null;
  if (serve) { server = await startServer(base); base = server.url; console.log(`(started Wardian at ${base})`); }
  process.on('exit', () => server && server.stop());
  const browser = await chromium.launch({ channel: 'chrome' }).catch(() => chromium.launch());
  const page = await browser.newPage();
  const problems = [];
  page.on('pageerror', e => problems.push('page error: ' + e.message));
  // Report failed requests by URL; the browser's own "Failed to load resource" line has no URL.
  page.on('console', m => { if (m.type() === 'error' && !m.text().startsWith('Failed to load resource')) problems.push('console error: ' + m.text()); });
  page.on('response', r => { if (r.status() >= 400 && !r.url().endsWith('/favicon.ico')) problems.push(`HTTP ${r.status()}: ${r.url()}`); });
  // Visible text without scripts (a hidden frame's innerText would include script source).
  const textOf = f => f.evaluate(() => { const b = document.body.cloneNode(true); b.querySelectorAll('script,style').forEach(n => n.remove()); return b.textContent; }).catch(() => '');

  const list = await (await fetch(`${base}/api/app-list`)).json();
  const app = list.find(a => a.name === name);
  if (!app) { console.log(`FAIL: "${name}" is not in the app list (${list.map(a => a.name).join(', ')})`); process.exit(1); }
  if (app.error) { console.log(`FAIL: ${app.error}`); process.exit(1); }

  if (app.suite) {
    console.log(`suite ${name}`);
    const t0 = Date.now();
    await page.goto(`${base}/run/${encodeURIComponent(name)}/`);
    // Wait for every part to start, up to the 15 s the kernel holds a call for a part not yet
    // started: a cold load on a busy machine can take more than a few seconds. Then watch at least
    // 3 s from the load, and 1 s after the last start, for faults from the parts' init.
    // Kernel.started() rather than the trace's boot events: the trace keeps only the last 400.
    const state = () => page.evaluate(() => typeof Kernel === 'undefined' ? null : ({
      apps: Kernel.apps().map(a => a.name),
      booted: Kernel.started(),
      faults: Kernel.faults(),
      events: Kernel.trace().length,
    })).catch(() => null);
    let r = await state();
    while (!(r && r.booted.length >= r.apps.length) && Date.now() - t0 < 15000) { await sleep(100); r = await state(); }
    const waited = Date.now() - t0;
    await sleep(Math.max(1000, t0 + 3000 - Date.now()));
    r = await state();
    if (!r) { console.log('FAIL: the suite page has no kernel'); process.exit(1); }
    const missing = r.apps.filter(a => !r.booted.includes(a));
    console.log(`  apps:    ${r.apps.join(', ')}`);
    console.log(`  started: ${r.booted.length}/${r.apps.length}${missing.length ? ' (not started: ' + missing.join(', ') + ')' : ''}`);
    console.log(`  events:  ${r.events}`);
    console.log(`  waited:  ${waited} ms for the parts to start`);
    r.faults.forEach(f => problems.push(`fault in ${f.app}: ${f.message}`));
    if (missing.length) problems.push('apps that never started: ' + missing.join(', '));
    for (const f of page.frames().filter(f => f !== page.mainFrame())) {
      const text = (await textOf(f)).trim().replace(/\s+/g, ' ');
      if (text) console.log(`  [${f.url().split('/').pop() || 'header'}] ${text.slice(0, 140)}`);
    }
  } else if (app.page) {
    console.log(`page ${name} (${app.page})`);
    await page.goto(`${base}/apps/${encodeURIComponent(name)}/${app.page}`);
    await sleep(1500);
    const text = (await textOf(page.mainFrame())).trim().replace(/\s+/g, ' ');
    console.log(`  shows: ${text.slice(0, 300)}`);
  } else {
    console.log(`module ${name}`);
    await page.goto(`${base}/`);
    await sleep(500);
    const r = await page.evaluate(async n => {
      const resp = await fetch(`/apps/${encodeURIComponent(n)}/app.wasm`);
      const mod = await WebAssembly.compile(await resp.arrayBuffer());
      const imports = WebAssembly.Module.imports(mod).map(i => `${i.module}.${i.name} (${i.kind})`);
      const exports = WebAssembly.Module.exports(mod).filter(e => e.kind === 'function').map(e => e.name);
      return { imports, exports };
    }, name);
    console.log(`  functions: ${r.exports.join(', ') || '(none)'}`);
    const unknown = r.imports.filter(i => i !== 'env.log (function)');
    if (unknown.length) console.log(`  note: imports the host stubs or cannot provide: ${unknown.join(', ')}`);
    if (!r.exports.length) problems.push('the module exports no functions');
  }

  await browser.close();
  if (problems.length) { console.log('FAIL'); problems.forEach(p => console.log('  - ' + p)); process.exit(1); }
  console.log('OK: no faults or errors');
  process.exit(0);   // exit explicitly: the server started by --serve would keep Node alive
})().catch(e => { console.error('could not run the smoke test:', e.message); process.exit(2); });
