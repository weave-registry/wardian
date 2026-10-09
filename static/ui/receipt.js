/* Wardian UI receipt: shows one channel message the same way in the app that sent it and the app
   that got it, so the user can match them (ADR-2610091338).
     const r = await ctx.channel('splunk.table').send(table, {name: 'Checkout latency'});
     box.replaceChildren(WardianUI.receipt({...r, channel: 'splunk.table', data: table}, {direction: 'sent'}));
     ctx.channel('splunk.table').on((data, info) => box.replaceChildren(WardianUI.receipt({...info, data}, {direction: 'received'})));
   Pass bytes, or data to have its size measured. WardianUI.shortId(id) gives "#3f9a2c". */
(() => {
  'use strict';
  const UI = window.WardianUI = window.WardianUI || {};
  const hex = id => String(id || '').replace(/[^0-9a-f]/gi, '').slice(0, 6).toLowerCase();
  UI.shortId = id => hex(id) ? '#' + hex(id) : '';
  const size = n => n < 1024 ? n + ' bytes' : n < 1024 * 1024 ? Math.round(n / 1024) + ' KB' : (n / 1024 / 1024).toFixed(1) + ' MB';
  const el = (tag, cls, text) => { const e = document.createElement(tag); if (cls) e.className = cls; if (text != null) e.textContent = text; return e; };

  UI.receipt = (m = {}, {direction = 'received'} = {}) => {
    const box = el('div', 'w-receipt');
    box.dataset.direction = direction;
    const h = hex(m.id);
    if (h) box.style.setProperty('--w-receipt-hue', String(parseInt(h, 16) % 360));
    const main = el('div');
    main.append(el('p', 'w-receipt-name', m.name || m.channel || 'A message'));
    const parts = [direction === 'sent' ? (m.channel ? 'Sent on ' + m.channel : 'Sent') : (m.from ? 'From ' + m.from : 'Received')];
    if (m.at) parts.push('at ' + new Date(m.at).toLocaleTimeString(undefined, {hour: 'numeric', minute: '2-digit'}));
    let bytes = m.bytes;
    if (bytes == null && m.data !== undefined) { try { bytes = JSON.stringify(m.data).length; } catch { bytes = null; } }
    const meta = el('p', 'w-receipt-meta', parts.join(' ') + (bytes != null ? ', ' + size(bytes) : ''));
    if (h) { const c = el('code', 'w-receipt-id', '#' + h); c.title = String(m.id); meta.append(c); }
    main.append(meta);
    const seal = el('span', 'w-receipt-seal'); seal.setAttribute('aria-hidden', 'true');
    box.append(seal, main);
    return box;
  };
})();
