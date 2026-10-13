/* about: what the current table is: its name, how many rows, the time range, when it was fetched
   (or saved), and the search behind it; and whether the USL lab can use it (UslReady, ADR-2610121400):
   which columns are the load, the throughput and the response time, and what to watch, by Gunther's
   rules. Open when the table was made for the lab or a load and a throughput are found.
   A table in the database is checked from its first 1000 rows, read with db.
   Listens: table:ready.  Capabilities: db. */
Kernel.register({
  name: 'about',
  listens: ['table:ready'],
  caps: ['db'],
  init(ctx){
    const CHECK_ROWS = 1000;
    let shown = 0;   // the latest table: a slower read of an older one is dropped
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
      if (!table){ usl(null); return; }
      const n = Cells.count(table);
      fact('Table', table.title);
      fact('Rows', n.toLocaleString() + (n === 1 ? ' row' : ' rows') + ' × ' + table.fields.length + ' columns' +
        (table.truncated ? ' (Splunk returned more; only the first ' + n.toLocaleString() + ' are kept)' : '') + (table.dataset ? ', kept in this app\'s database' : ''));
      fact('Time range', table.rangeLabel);
      fact('Fetched', table.at ? new Date(table.at).toLocaleString() + (table.seconds ? ', the search took ' + table.seconds + ' s' : '') : '');
      if (table.savedAt) fact('Saved', new Date(table.savedAt).toLocaleString());
      fact('Search', table.search, true);
      usl(table);
    });

    const LABEL = {ok: 'OK', watch: 'Watch', problem: 'Problem'};
    async function rowsOf(table){
      if (table.rows && table.rows.length) return table.rows;
      if (!table.dataset || !table.dataset.total) return [];
      const db = await ctx.cap('db').catch(() => null);
      if (!db) return [];
      try { return (await db.page({table: table.dataset.table, offset: 0, limit: Math.min(CHECK_ROWS, table.dataset.total)})).rows || []; }
      catch { return []; }
    }
    async function usl(table){
      const box = ctx.$('#usl'), list = ctx.$('#uslFindings'), mine = ++shown;
      box.hidden = true;
      const rows = table ? await rowsOf(table) : [];
      if (mine !== shown || !rows.length) return;
      const A = UslReady.assess(table.fields, rows, table.use);
      const u = A.use;
      ctx.$('#uslVerdict').textContent = A.ready
        ? 'Yes: load ' + u.load + ', throughput ' + u.throughput + (u.response ? ', response time ' + u.response : '') + '.' +
          (A.findings.some(f => f.level === 'watch') ? ' Some things to watch.' : '')
        : 'Not yet: ' + A.findings.find(f => f.level === 'problem').text.split(/(?<=\.)\s/)[0];
      list.replaceChildren(...A.findings.map(f => {
        const li = ctx.el('li'), b = ctx.el('strong');
        li.className = f.level; b.textContent = LABEL[f.level] + ': ';
        li.append(b, f.text);
        return li;
      }));
      const total = Cells.count(table);
      ctx.$('#uslNote').textContent = (rows.length < total ? 'Checked the first ' + rows.length.toLocaleString() + ' of ' + total.toLocaleString() + ' rows. ' : '') +
        'Send to other apps passes these columns to the USL lab, which you can still change there.';
      box.hidden = false;
      box.open = !!(table.use || table.units) || A.ready;
    }
  }
});
