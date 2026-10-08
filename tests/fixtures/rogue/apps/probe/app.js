Kernel.register({ name: 'probe', emits: { 'probe:done': {}, 'probe:data': {}, 'probe:tick': {} },
  needs: ['late.ping', 'mute.never', 'liar.nothing'], init(ctx){
  window.probeCtx = ctx;   // the test drives ctx from outside (Kernel.trace, toString)
  const out = [], direct = {}, timing = {};
  // A cap the host does not give resolves to null (SPEC 6.5), so for cap only null is a refusal.
  addEventListener('message', e => { const m = e.data || {}; if (m.k === 'reply' && m.id >= 990 && m.id <= 999) direct[m.id] = m.ok && !(m.id === 997 && m.value === null) ? 'ALLOWED' : 'refused: ' + m.error; });
  const t = async (l, f) => { try { out.push(l + ': ALLOWED ' + await f()); } catch (e) { out.push(l + ': blocked (' + e.name + ')'); } };
  const t0 = performance.now(), ms = () => Math.round(performance.now() - t0);
  // SPEC 6.7.5: a call to an app still starting waits for it; one that never provides fails after 15 s.
  ctx.call('late', 'ping').then(v => { timing.ping = {v, ms: ms()}; }, e => { timing.ping = {e: e.message, ms: ms()}; });
  ctx.call('mute', 'never').then(v => { timing.never = {v, ms: ms()}; }, e => { timing.never = {e: e.message, ms: ms()}; })
    .then(() => { ctx.root.querySelector('#t').textContent = JSON.stringify(timing); });
  // SPEC 6.7.4: only the called app can answer. Forge an answer to every call the kernel may hold,
  // while mute's call to late.slow is waiting (late provides at 1 s and answers 3 s later).
  for (const at of [1500, 2500, 3500]) setTimeout(() => { for (let id = 1; id <= 300; id++) parent.postMessage({k: 'result', id, ok: true, value: 'forged'}, '*'); }, at);
  // SPEC 6.5: every payload is copied. The listener must get n: 1, not the n: 2 set after emit.
  setTimeout(() => { const p = {n: 1}; ctx.emit('probe:data', p); p.n = 2; }, 2000);
  // ADR-2610071110: ctx.observe runs its callback on the next animation frame, at most once per
  // frame, so a callback that changes the size does not feed back into the same frame's notices.
  const box = ctx.el('div'); box.style.cssText = 'width:10px;height:4px;background:#999';
  ctx.root.append(box);
  let frame = 0; const tick = () => { frame++; if (frame < 240) requestAnimationFrame(tick); }; requestAnimationFrame(tick);
  window.observed = []; window.loopErrors = 0;
  addEventListener('error', e => { if (/ResizeObserver loop/.test(String(e.message || ''))) window.loopErrors++; });
  ctx.observe(box, () => { window.observed.push(frame); if (window.observed.length < 20) box.style.width = (10 + window.observed.length) + 'px'; });
  // ADR-2610071110: an unhandled promise rejection is a fault.
  Promise.reject(new Error('probe unhandled rejection'));
  (async () => {
    await t('fetch host API', async () => (await fetch('/api/status')).status);
    await t('fetch internet', async () => (await fetch('https://example.com/')).status);
    await t('read host page', async () => typeof parent.document.body);
    await t('localStorage', async () => typeof localStorage.length);
    await t('ctx.emit undeclared', async () => { ctx.emit('evil:topic', 1); return 'sent'; });
    // C9: a topic named like an Object.prototype member is not declared either.
    await t('ctx.emit toString', async () => { ctx.emit('toString', 1); return 'sent'; });
    await t('ctx.emit constructor', async () => { ctx.emit('constructor', 1); return 'sent'; });
    await t('ctx.store, no storage cap', async () => ctx.store.get('x'));
    await t('ctx.cap downloads, not granted', async () => ctx.cap('downloads'));
    await t('ctx.call, declared but not provided in suite.json', async () => ctx.call('liar', 'nothing'));
    // Skip the shim and talk to the kernel directly. SPEC 6.7.6: each refusal is a fault.
    parent.postMessage({k: 'emit', topic: 'evil:topic', payload: 1}, '*');
    parent.postMessage({k: 'emit', topic: 'toString', payload: 1}, '*');
    parent.postMessage({k: 'store', key: 'x', value: 1}, '*');
    parent.postMessage({k: 'capop', id: 999, name: 'downloads', op: 'save', args: {filename: 'x.txt', data: 'pwn'}}, '*');
    parent.postMessage({k: 'call', id: 998, app: 'liar', method: 'anything', args: 1}, '*');
    parent.postMessage({k: 'cap', id: 997, name: 'downloads'}, '*');
    parent.postMessage({k: 'asset', id: 996, path: 'suite.json'}, '*');
    parent.postMessage({k: 'chsend', id: 995, channel: 'evil', data: 1}, '*');
    parent.postMessage({k: 'chon', id: 994, channel: 'evil'}, '*');
    await new Promise(r => setTimeout(r, 500));
    out.push('direct capop: ' + direct[999], 'direct call: ' + direct[998], 'direct cap: ' + direct[997],
      'direct asset: ' + direct[996], 'direct chsend: ' + direct[995], 'direct chon: ' + direct[994]);
    ctx.root.querySelector('#o').textContent = out.join('\n');
  })();
}});
