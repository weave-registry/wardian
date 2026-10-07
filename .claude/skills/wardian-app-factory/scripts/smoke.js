#!/usr/bin/env node
// Smoke test for one Wardian package: opens it the way a user would and reports what broke.
//
//   node smoke.js --serve <apps-folder> <app-name>   start a private Wardian on a free port, test, stop it
//   node smoke.js <base-url> <app-name>              test against a Wardian that is already running
//   e.g. node smoke.js --serve apps hello-suite
//
// --serve runs target/release/wardian from the current folder (the repo root), with a throwaway
// data folder, so it never touches the user's server or settings.
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
  const bin = path.resolve('target/release/wardian');
  if (!fs.existsSync(bin)) { console.error(`no ${bin}: run this from the repo root after cargo build --release`); process.exit(2); }
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
    await page.goto(`${base}/run/${encodeURIComponent(name)}/`);
    await sleep(3000);
    const r = await page.evaluate(() => ({
      apps: Kernel.apps().map(a => a.name),
      booted: Kernel.trace().filter(t => t.kind === 'boot').map(t => t.app),
      faults: Kernel.faults(),
      events: Kernel.trace().length,
    }));
    const missing = r.apps.filter(a => !r.booted.includes(a));
    console.log(`  apps:    ${r.apps.join(', ')}`);
    console.log(`  started: ${r.booted.length}/${r.apps.length}${missing.length ? ' (not started: ' + missing.join(', ') + ')' : ''}`);
    console.log(`  events:  ${r.events}`);
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
