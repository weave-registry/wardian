/* rows: the table itself, with "Find in results", sorting by any column, and a pager.
   A table in the database (ADR-2610071219) shows 100 rows at a time; sorting and finding run there.
   A table in memory shows up to 1,000 rows; the CSV (keep) carries the rest.
   Emits: table:view (retained: the find text and the sort, so keep's CSV saves what is shown).
   Listens: table:ready.  Capabilities: db.
   snapshot (ADR-2610080905): a saved web page gets every row, up to 10,000, not just the page shown. */
const RowsNow = {table: null, view: {find: '', sort: {col: -1, dir: 1}}};   // read by snapshot
Kernel.register({
  name: 'rows',
  emits: {'table:view': {retain: true}},
  listens: ['table:ready'],
  caps: ['db'],
  init(ctx){
    const $ = ctx.$;
    const SHOW_ROWS = 1000;                    // rows drawn from a table in memory
    const PAGE = 100;                          // rows per page when the table is in the database
    let db = null, dbKnown = false;            // ctx.cap('db'), and whether the host has answered yet
    let table = null, pageAt = 0, drawing = 0;
    let sort = {col: -1, dir: 1};
    const empty = text => Object.assign(ctx.el('p'), {className: 'empty', textContent: text});
    const hint = (text, warn) => { $('#findHint').className = 'w-hint' + (warn ? ' warn' : ''); $('#findHint').textContent = text; };
    function tellView(){
      RowsNow.view = {find: $('#find').value.trim(), sort};
      ctx.emit('table:view', RowsNow.view);
    }

    // One table element: header clicks sort; `numeric` right-aligns number columns.
    function tableEl(fields, rows, numeric){
      const t = ctx.el('table'), thead = ctx.el('thead'), hr = ctx.el('tr');
      t.className = 'w-table';
      fields.forEach((f, i) => {
        const th = ctx.el('th'); th.textContent = f; th.title = 'Sort by ' + f; th.scope = 'col';
        if (numeric[i]) th.className = 'num';
        if (sort.col === i){ const a = ctx.el('span'); a.className = 'arrow'; a.textContent = sort.dir > 0 ? '▲' : '▼'; th.appendChild(a); }
        th.addEventListener('click', () => { sort = {col: i, dir: sort.col === i ? -sort.dir : 1}; pageAt = 0; tellView(); draw(); });
        hr.appendChild(th);
      });
      thead.appendChild(hr); t.appendChild(thead);
      const tb = ctx.el('tbody');
      for (const r of rows){
        const tr = ctx.el('tr');
        r.forEach((v, i) => { const td = ctx.el('td'); td.textContent = v == null ? '' : String(v); if (numeric[i]) td.className = 'num'; tr.appendChild(td); });
        tb.appendChild(tr);
      }
      t.appendChild(tb);
      const wrap = ctx.el('div'); wrap.className = 'w-table-wrap'; wrap.appendChild(t);
      return wrap;
    }

    // Database: asks for one page and draws it, with "rows X–Y of N" and Previous / Next.
    async function drawPage(){
      const out = $('#out'), pager = $('#pager'), mine = ++drawing;
      const {req, find} = Find.pageRequest(table, {find: $('#find').value, sort}, pageAt, PAGE);
      if (find.problems.length) hint(find.problems.join(' '), true);
      let page;
      try { page = await db.page(req); }
      catch (e){ if (mine === drawing){ out.replaceChildren(empty('Could not read the table: ' + Cells.errText(e))); pager.hidden = true; } return; }
      if (mine !== drawing) return;                         // a newer request was made meanwhile
      const total = table.dataset.total;
      if (!find.problems.length) hint(find.active ? page.total.toLocaleString() + ' of ' + total.toLocaleString() + ' rows match. Download CSV saves the matching rows.' : Find.HINT);
      if (!page.total){
        out.replaceChildren(empty(total ? 'No rows match what you typed under Find in results.' : 'The search returned no rows. Try a longer time range.'));
        pager.hidden = true; return;
      }
      out.replaceChildren(tableEl(table.fields, page.rows, Cells.numeric(table.fields, page.rows)));
      const last = Math.min(pageAt + page.rows.length, page.total);
      $('#pageInfo').textContent = 'Rows ' + (pageAt + 1).toLocaleString() + '–' + last.toLocaleString() + ' of ' + page.total.toLocaleString();
      $('#prev').disabled = pageAt === 0;
      $('#next').disabled = last >= page.total;
      pager.hidden = false;
    }
    $('#prev').addEventListener('click', () => { pageAt = Math.max(0, pageAt - PAGE); draw(); });
    $('#next').addEventListener('click', () => { pageAt += PAGE; draw(); });

    function draw(){
      const out = $('#out');
      $('#findBox').hidden = !Cells.count(table);
      $('#pager').hidden = true;
      if (!table){ out.replaceChildren(empty('No table yet. Run a search.')); return; }
      if (table.dataset){
        if (!dbKnown) return;                               // drawn again once the host answers
        if (!db){ out.replaceChildren(empty('This table is in the database, which this host does not offer. Run the search again.')); return; }
        drawPage();
        return;
      }
      if (!table.rows.length){ out.replaceChildren(empty('The search returned no rows. Try a longer time range.')); return; }
      const {rows, find, numeric} = Find.rows(table, {find: $('#find').value, sort});
      if (find.problems.length) hint(find.problems.join(' '), true);
      else hint(find.active ? rows.length + ' of ' + table.rows.length + ' rows match. Download CSV saves the matching rows.' : Find.HINT);
      if (!rows.length){ out.replaceChildren(empty('No rows match what you typed under Find in results.')); return; }
      const parts = [tableEl(table.fields, rows.slice(0, SHOW_ROWS), numeric)];
      if (rows.length > SHOW_ROWS) parts.push(Object.assign(ctx.el('p'), {className: 'note', textContent: 'Showing the first ' + SHOW_ROWS + ' of ' + rows.length + ' rows. Download the CSV to see all of them.'}));
      out.replaceChildren(...parts);
    }

    let findTimer = null;
    $('#find').addEventListener('input', () => { clearTimeout(findTimer); findTimer = setTimeout(() => { pageAt = 0; tellView(); draw(); }, 200); });
    // A new table starts unsorted and unfiltered.
    const key = t => t ? [t.at, t.savedAt, t.dataset && t.dataset.table, t.title].join('|') : '';
    ctx.on('table:ready', m => {
      const t = m && m.table && Array.isArray(m.table.fields) ? m.table : null;
      if (key(t) !== key(table) || (m && m.send)){ sort = {col: -1, dir: 1}; pageAt = 0; $('#find').value = ''; }
      table = RowsNow.table = t;
      tellView(); draw();
    });
    (async () => {
      db = await ctx.cap('db').catch(() => null);
      dbKnown = true;
      draw();
    })();
  },

  // Every row the view matches, up to 10,000, as one plain table, with a note when there are more.
  async snapshot(ctx){
    const LIMIT = 10000, t = RowsNow.table, el = tag => document.createElement(tag);
    const box = el('div'), h = el('h2'); h.textContent = 'Result'; box.appendChild(h);
    const say = text => { const p = el('p'); p.className = 'note'; p.textContent = text; box.appendChild(p); };
    if (!t){ say('No table yet.'); return box; }
    let rows = [], total = 0;
    if (t.dataset){
      const db = await ctx.cap('db').catch(() => null);
      if (!db){ say('This table is in the database, which this host does not offer.'); return box; }
      for (let at = 0; at < LIMIT; at += 1000){
        const p = await db.page(Find.pageRequest(t, RowsNow.view, at, Math.min(1000, LIMIT - at)).req);
        total = p.total; rows = rows.concat(p.rows);
        if (rows.length >= total || !p.rows.length) break;
      }
    } else { rows = Find.rows(t, RowsNow.view).rows; total = rows.length; }
    const find = RowsNow.view.find ? ' that match “' + RowsNow.view.find + '”' : '';
    say(total > LIMIT ? 'The first ' + LIMIT.toLocaleString() + ' of ' + total.toLocaleString() + ' rows' + find + '. Download the CSV in Wardian for all of them.'
      : Cells.rowsText(total) + find + '.');
    const table = el('table'), head = el('tr'), body = el('tbody'), numeric = Cells.numeric(t.fields, rows);
    table.className = 'w-table';
    t.fields.forEach((f, i) => { const th = el('th'); th.textContent = f; if (numeric[i]) th.className = 'num'; head.appendChild(th); });
    table.appendChild(el('thead')).appendChild(head);
    for (const r of rows.slice(0, LIMIT)){
      const tr = el('tr');
      r.forEach((v, i) => { const td = el('td'); td.textContent = v == null ? '' : String(v); if (numeric[i]) td.className = 'num'; tr.appendChild(td); });
      body.appendChild(tr);
    }
    table.appendChild(body); box.appendChild(table);
    return box;
  }
});
