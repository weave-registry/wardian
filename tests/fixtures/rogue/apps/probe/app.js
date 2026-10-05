Kernel.register({ name: 'probe', emits: { 'probe:done': {} }, init(ctx){
  const out = [], direct = {};
  addEventListener('message', e => { const m = e.data || {}; if (m.id === 998 || m.id === 999) direct[m.id] = m.ok ? 'ALLOWED' : 'refused: ' + m.error; });
  const t = async (l, f) => { try { out.push(l + ': ALLOWED ' + await f()); } catch (e) { out.push(l + ': blocked (' + e.name + ')'); } };
  (async () => {
    await t('fetch host API', async () => (await fetch('/api/status')).status);
    await t('fetch internet', async () => (await fetch('https://example.com/')).status);
    await t('read host page', async () => typeof parent.document.body);
    await t('localStorage', async () => typeof localStorage.length);
    await t('ctx.emit undeclared', async () => { ctx.emit('evil:topic', 1); return 'sent'; });
    await t('ctx.store, no storage cap', async () => ctx.store.get('x'));
    await t('ctx.cap downloads, not granted', async () => ctx.cap('downloads'));
    // Skip the shim and talk to the kernel directly.
    parent.postMessage({k: 'emit', topic: 'evil:topic', payload: 1}, '*');
    parent.postMessage({k: 'store', key: 'x', value: 1}, '*');
    parent.postMessage({k: 'capop', id: 999, name: 'downloads', op: 'save', args: {filename: 'x.txt', data: 'pwn'}}, '*');
    parent.postMessage({k: 'call', id: 998, app: 'liar', method: 'anything', args: 1}, '*');
    await new Promise(r => setTimeout(r, 500));
    out.push('direct capop: ' + direct[999], 'direct call: ' + direct[998]);
    ctx.root.querySelector('#o').textContent = out.join('\n');
  })();
}});
