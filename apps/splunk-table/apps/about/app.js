/* about: what the current table is: its name, how many rows, the time range, when it was fetched
   (or saved), and the search behind it.  Listens: table:ready. */
Kernel.register({
  name: 'about',
  listens: ['table:ready'],
  init(ctx){
    const facts = ctx.$('#facts');
    function fact(k, v, code){
      if (!v) return;
      const dt = ctx.el('dt'), dd = ctx.el('dd'); dt.textContent = k;
      if (code){ const c = ctx.el('code'); c.textContent = v; dd.appendChild(c); } else dd.textContent = v;
      facts.append(dt, dd);
    }
    ctx.on('table:ready', m => {
      const table = m && m.table && Array.isArray(m.table.fields) ? m.table : null;
      facts.replaceChildren();
      facts.hidden = !table; ctx.$('#none').hidden = !!table;
      if (!table) return;
      const n = Cells.count(table);
      fact('Table', table.title);
      fact('Rows', n.toLocaleString() + (n === 1 ? ' row' : ' rows') + ' × ' + table.fields.length + ' columns' +
        (table.truncated ? ' (Splunk returned more; only the first ' + n.toLocaleString() + ' are kept)' : '') + (table.dataset ? ', kept in this app\'s database' : ''));
      fact('Time range', table.rangeLabel);
      fact('Fetched', table.at ? new Date(table.at).toLocaleString() + (table.seconds ? ', the search took ' + table.seconds + ' s' : '') : '');
      if (table.savedAt) fact('Saved', new Date(table.savedAt).toLocaleString());
      fact('Search', table.search, true);
    });
  }
});
