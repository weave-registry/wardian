// The core in a dedicated worker: Wardian's domain, ports and use cases, compiled to WebAssembly,
// with the browser's clock. Each test reports what worked.
let core, i32;
const sleep = (ms) => {
  if (i32) { Atomics.wait(i32, 0, 0, ms); return 'atomics'; }
  const end = Date.now() + ms; while (Date.now() < end) {} return 'busy-wait';
};
let how = '';
async function load() {
  if (typeof SharedArrayBuffer === 'function') i32 = new Int32Array(new SharedArrayBuffer(4));
  const bytes = await (await fetch('core.wasm')).arrayBuffer();
  const env = { now_ms: () => Date.now(), sleep_ms: (ms) => { how = sleep(ms); } };
  core = (await WebAssembly.instantiate(bytes, { env })).instance.exports;
}
async function tests() {
  const r = { shared_array_buffer: typeof SharedArrayBuffer === 'function' };
  r.check_errors = core.probe_check();
  r.frame_bytes = core.probe_frame();
  r.clock_skew_ms = Math.abs(core.probe_now() - Date.now());
  r.sleep_200_took_ms = core.probe_sleep(200);
  r.sleep_by = how;
  r.job_started = core.job_start();
  r.job_state_before = core.job_state();
  const t = Date.now();
  r.tasks_ran = core.run_tasks();
  r.tasks_took_ms = Date.now() - t;
  r.job_state_after = core.job_state();
  try {
    const x = new XMLHttpRequest(); x.open('GET', 'data.txt', false); x.send();
    r.sync_xhr = x.responseText.trim();
  } catch (e) { r.sync_xhr = 'failed: ' + e.message; }
  try {
    const root = await navigator.storage.getDirectory();
    const fh = await root.getFileHandle('probe.txt', { create: true });
    const h = await fh.createSyncAccessHandle();
    const data = new TextEncoder().encode('kept in OPFS');
    h.truncate(0); h.write(data, { at: 0 }); h.flush();
    const back = new Uint8Array(h.getSize()); h.read(back, { at: 0 }); h.close();
    r.opfs_sync = new TextDecoder().decode(back);
  } catch (e) { r.opfs_sync = 'failed: ' + e.message; }
  return r;
}
onmessage = async (e) => {
  if (e.data === 'tests') { await load(); postMessage({ tests: await tests() }); }
  if (e.data === 'long-task') { core.job_start(); core.run_tasks(); postMessage({ done: 'long-task' }); }
  if (e.data === 'ping') postMessage({ pong: true });
};
