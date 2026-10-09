/* Wardian UI receipt: shows one channel message the same way in the app that sent it and the app
   that got it, so the user can see what went where and match the two (ADR-2610091338).
     const r = await ctx.channel('splunk.table').send(table, {name: 'Checkout latency'});
     box.replaceChildren(WardianUI.receipt({...r, channel: 'splunk.table', data: table}, {direction: 'sent'}));
     ctx.channel('splunk.table').on((data, info) => box.replaceChildren(
       WardianUI.receipt({...info, channel: 'splunk.table', data}, {direction: 'received'})));
   The third line says what the data is: give `what`, or pass `data` and WardianUI.describe(data)
   writes it ("1,000 rows, 5 columns: host, count, p95, error and 1 more"). Both apps run the same
   code on the same data, so both say the same. WardianUI.shortId(id) gives "#3f9a2c". */
(() => {
  'use strict';
  const UI = window.WardianUI = window.WardianUI || {};
  const hex = id => String(id || '').replace(/[^0-9a-f]/gi, '').slice(0, 6).toLowerCase();
  UI.shortId = id => hex(id) ? '#' + hex(id) : '';
  const n = v => Number(v).toLocaleString();
  const plural = (v, one, many) => n(v) + ' ' + (v === 1 ? one : many);
  const list = (names, max) => names.slice(0, max).join(', ') + (names.length > max ? ' and ' + (names.length - max) + ' more' : '');
  const size = b => b < 1024 ? plural(b, 'byte', 'bytes') : b < 1024 * 1024 ? Math.round(b / 1024) + ' KB' : (b / 1024 / 1024).toFixed(1) + ' MB';
  const el = (tag, cls, text) => { const e = document.createElement(tag); if (cls) e.className = cls; if (text != null) e.textContent = text; return e; };

  /** What a message's data is, in a few words. A table ({fields, rows}) counts all its rows, also
      those left in the sender's database (dataset.total). */
  UI.describe = d => {
    if (d && Array.isArray(d.fields) && Array.isArray(d.rows)) {
      const total = d.dataset && d.dataset.total > d.rows.length ? d.dataset.total : d.rows.length;
      return plural(total, 'row', 'rows') + ', ' + plural(d.fields.length, 'column', 'columns') + (d.fields.length ? ': ' + list(d.fields, 4) : '');
    }
    if (Array.isArray(d)) return plural(d.length, 'item', 'items');
    if (d && typeof d === 'object') { const k = Object.keys(d); return k.length ? 'Fields: ' + list(k, 5) : 'An empty object'; }
    if (typeof d === 'string') return 'Text, ' + plural(d.length, 'character', 'characters');
    return d == null ? 'Nothing' : String(d);
  };

  UI.receipt = (m = {}, {direction = 'received'} = {}) => {
    const sent = direction === 'sent';
    const box = el('div', 'w-receipt');
    box.dataset.direction = sent ? 'sent' : 'received';
    const h = hex(m.id);
    if (h) box.style.setProperty('--w-receipt-hue', String(parseInt(h, 16) % 360));
    const main = el('div');
    main.append(el('p', 'w-receipt-name', m.name || m.channel || 'A message'));

    const meta = el('p', 'w-receipt-meta');
    meta.append(el('strong', 'w-receipt-dir', sent ? 'Sent' : 'Received'));
    let line = sent ? ' to other apps' : (m.from ? ' from ' + m.from : '');
    if (m.channel) line += ' on ' + m.channel;
    if (m.at) line += ' at ' + new Date(m.at).toLocaleTimeString(undefined, {hour: 'numeric', minute: '2-digit'});
    let bytes = m.bytes;
    if (bytes == null && m.data !== undefined) { try { bytes = JSON.stringify(m.data).length; } catch { bytes = null; } }
    if (bytes != null) line += ', ' + size(bytes);
    meta.append(line);
    if (h) { const c = el('code', 'w-receipt-id', '#' + h); c.title = 'Message id ' + m.id; meta.append(c); }
    main.append(meta);

    const what = m.what != null ? String(m.what) : m.data !== undefined ? UI.describe(m.data) : '';
    if (what) main.append(el('p', 'w-receipt-what', what));
    const seal = el('span', 'w-receipt-seal'); seal.setAttribute('aria-hidden', 'true');
    box.append(seal, main);
    return box;
  };
})();
