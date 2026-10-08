/* table: the rows, one page of 100 at a time, read from the database with db.page. Click a column
   to sort by it (again to reverse, a third time to stop sorting). "Filter" becomes a WHERE condition
   with bound parameters (shared/filter.js), so 100,000 rows sort and filter in the database.
   Listens: dataset:ready.  Emits: view:changed (retained: the filter and the sort, which stats, chart
   and export follow).  Capabilities: db. */
Kernel.register({
  name: 'table',
  emits: {'view:changed': {retain: true}},
  listens: ['dataset:ready'],
  caps: ['db'],
  init(ctx){
    const $ = ctx.$;
    const PAGE = 100;
    let db = null, dbKnown = false, data = null, view = {find: '', sort: {col: -1, desc: false}};
    let pageAt = 0, total = 0, drawing = 0;
    const empty = text => Object.assign(ctx.el('p'), {className: 'empty', textContent: text});
    const hint = (text, warn) => { const h = $('#findHint'); h.className = 'w-hint' + (warn ? ' warn' : ''); h.textContent = text; };
    function tell(){ ctx.emit('view:changed', {table: data ? data.table : null, find: view.find, sort: view.sort}); }

    // The header: each column name is a button that sorts; aria-sort says which way.
    function head(){
      const tr = ctx.el('tr');
      data.fields.forEach((f, i) => {
        const th = ctx.el('th'), b = ctx.el('button'), on = view.sort.col === i;
        th.scope = 'col';
        if (data.types[i] === 'number') th.className = 'num';
        th.setAttribute('aria-sort', on ? (view.sort.desc ? 'descending' : 'ascending') : 'none');
        b.type = 'button'; b.className = 'sort'; b.textContent = f;
        b.title = 'Sort by ' + f + ' (' + data.types[i] + ')';
        if (on){ const a = ctx.el('span'); a.className = 'arrow'; a.textContent = view.sort.desc ? '▼' : '▲'; b.appendChild(a); }
        b.addEventListener('click', () => {
          const s = view.sort;
          view.sort = s.col !== i ? {col: i, desc: false} : !s.desc ? {col: i, desc: true} : {col: -1, desc: false};
          pageAt = 0; tell(); draw();
        });
        th.appendChild(b); tr.appendChild(th);
      });
      const thead = ctx.el('thead'); thead.appendChild(tr);
      return thead;
    }

    function body(rows){
      const tb = ctx.el('tbody'), n = data.columns.length;
      for (const r of rows){
        const tr = ctx.el('tr');
        for (let i = 0; i < n; i++){
          const td = ctx.el('td'), v = r[i];
          if (data.types[i] === 'number') td.className = 'num';
          if (v == null){ td.classList.add('null'); td.textContent = '—'; }
          else td.textContent = Data.cell(v, data.types[i]);
          tr.appendChild(td);
        }
        tb.appendChild(tr);
      }
      return tb;
    }

    // Asks the database for one page and draws it. A newer request wins over an older one.
    async function draw(){
      const out = $('#out'), pager = $('#pager'), mine = ++drawing;
      $('#filterBox').hidden = !data;
      if (!data){ out.replaceChildren(empty(dbKnown ? 'No data yet. Open a CSV file under Data.' : 'Waiting for data.')); pager.hidden = true; return; }
      if (!dbKnown) return;
      if (!db){ out.replaceChildren(empty('This host has no database for apps.')); pager.hidden = true; return; }
      const {req, filter} = Filter.pageRequest(data, {table: data.table, find: view.find, sort: view.sort}, pageAt, PAGE);
      let page;
      try { page = await db.page(req); }
      catch (e){ if (mine === drawing){ out.replaceChildren(empty('Could not read the rows: ' + Data.errText(e))); pager.hidden = true; } return; }
      if (mine !== drawing) return;
      total = page.total;
      if (filter.problems.length) hint(filter.problems.join(' '), true);
      else hint(filter.active ? Data.count(total) + ' of ' + Data.rows(data.total) + ' match.' : Filter.HINT);
      if (pageAt > 0 && pageAt >= total){ pageAt = Math.max(0, Math.floor((total - 1) / PAGE) * PAGE); return draw(); }
      if (!total){ out.replaceChildren(empty(data.total ? 'No rows match the filter.' : 'The file has no rows.')); pager.hidden = true; return; }
      const t = ctx.el('table'), wrap = ctx.el('div');
      t.className = 'w-table'; t.append(head(), body(page.rows));
      wrap.className = 'w-table-wrap'; wrap.appendChild(t);
      out.replaceChildren(wrap);
      const last = Math.min(pageAt + page.rows.length, total);
      $('#pageInfo').textContent = 'Rows ' + Data.count(pageAt + 1) + '–' + Data.count(last) + ' of ' + Data.count(total);
      $('#first').disabled = $('#prev').disabled = pageAt === 0;
      $('#next').disabled = $('#last').disabled = last >= total;
      pager.hidden = false;
    }
    const go = at => { pageAt = Math.max(0, at); draw(); };
    $('#first').addEventListener('click', () => go(0));
    $('#prev').addEventListener('click', () => go(pageAt - PAGE));
    $('#next').addEventListener('click', () => go(pageAt + PAGE));
    $('#last').addEventListener('click', () => go(Math.floor((total - 1) / PAGE) * PAGE));

    let timer = null;
    $('#find').addEventListener('input', () => {
      clearTimeout(timer);
      timer = setTimeout(() => { view.find = $('#find').value.trim(); pageAt = 0; tell(); draw(); }, 250);
    });

    // A new dataset starts unsorted and unfiltered; the same one announced again keeps the view.
    ctx.on('dataset:ready', m => {
      const d = Data.valid(m && m.dataset);
      if (!d || !data || d.table !== data.table){ view = {find: '', sort: {col: -1, desc: false}}; $('#find').value = ''; pageAt = 0; }
      data = d; tell(); draw();
    });
    (async () => {
      db = await ctx.cap('db').catch(() => null);
      dbKnown = true;
      draw();
    })();
  }
});
