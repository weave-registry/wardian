/* history: every session, newest first, 20 to a page, read from the database a page at a time
   (db.page). Each row can be removed.
   Listens: log:changed.  Emits: log:changed (after a removal).  Capabilities: db. */
Kernel.register({
  name: 'history',
  listens: ['log:changed'],
  emits: {'log:changed': {retain: true}},
  caps: ['db'],
  init(ctx){
    const $ = ctx.$, PAGE = 20;
    let at = 0, drawing = 0;

    function td(text, cls){ const c = ctx.el('td'); c.textContent = text; if (cls) c.className = cls; return c; }
    async function draw(){
      const mine = ++drawing, out = $('#out');
      let page, d;
      try {
        d = await Log.db(ctx);
        if (!d){ out.replaceChildren(Log.empty(ctx, 'This Wardian has no database for apps.')); return; }
        page = await d.page({table: Log.TABLE, offset: at, limit: PAGE, orderBy: 'started', desc: true});
        if (page.total && at >= page.total){ at = Math.max(0, (Math.ceil(page.total / PAGE) - 1) * PAGE); return draw(); }
      } catch (e){ if (mine === drawing) out.replaceChildren(Log.empty(ctx, 'Could not read the log: ' + Log.errText(e))); return; }
      if (mine !== drawing) return;                         // a newer read started meanwhile
      $('#sub').textContent = page.total ? page.total.toLocaleString() + (page.total === 1 ? ' session' : ' sessions') : '';
      $('#pager').hidden = page.total <= PAGE;
      if (!page.total){ out.replaceChildren(Log.empty(ctx, 'No sessions yet. Finish a focus session in the Focus timer, or press “Send a test session” there.')); return; }
      const col = name => page.columns.indexOf(name);
      const [I, DAY, LABEL, MIN, START, END, DONE, SRC] = ['id', 'day', 'label', 'minutes', 'started', 'ended', 'completed', 'source'].map(col);
      const table = ctx.el('table'); table.className = 'w-table';
      table.innerHTML = '<thead><tr><th scope="col">Day</th><th scope="col">Time</th><th scope="col">Label</th><th scope="col" class="num">Minutes</th><th scope="col">Result</th><th scope="col">From</th><th scope="col"><span class="w-sr">Remove</span></th></tr></thead>';
      const body = ctx.el('tbody');
      for (const r of page.rows){
        const tr = ctx.el('tr');
        const res = ctx.el('td'), badge = ctx.el('span');
        badge.className = 'w-badge'; badge.dataset.variant = r[DONE] ? 'success' : 'outline';
        badge.textContent = r[DONE] ? 'Completed' : 'Stopped early';
        res.appendChild(badge);
        const rm = ctx.el('button');
        rm.type = 'button'; rm.className = 'w-button'; rm.dataset.variant = 'ghost'; rm.dataset.size = 'icon';
        rm.textContent = '×'; rm.title = 'Remove this session'; rm.setAttribute('aria-label', 'Remove the session “' + r[LABEL] + '” of ' + Log.date(r[DAY]));
        rm.addEventListener('click', () => remove(d, r[I], r[LABEL]));
        const rmCell = ctx.el('td'); rmCell.appendChild(rm);
        tr.append(td(Log.date(r[DAY])), td(Log.time(r[START]) + '–' + Log.time(r[END]), 'nowrap'), td(r[LABEL]),
          td(Number(r[MIN]).toFixed(1), 'num'), res, td(r[SRC], 'muted'), rmCell);
        body.appendChild(tr);
      }
      table.appendChild(body);
      const wrap = ctx.el('div'); wrap.className = 'w-table-wrap'; wrap.appendChild(table);
      out.replaceChildren(wrap);
      const last = Math.min(at + page.rows.length, page.total);
      $('#pageInfo').textContent = (at + 1) + '–' + last + ' of ' + page.total.toLocaleString();
      $('#prev').disabled = at === 0;
      $('#next').disabled = last >= page.total;
    }
    async function remove(d, id, label){
      if (!window.confirm('Remove the session “' + label + '” from the log?')) return;
      try { await d.query({sql: 'DELETE FROM sessions WHERE id = ?', params: [id]}); }
      catch (e){ $('#pageInfo').textContent = 'Could not remove it: ' + Log.errText(e); return; }
      ctx.emit('log:changed', {at: Date.now()});
    }
    $('#prev').addEventListener('click', () => { at = Math.max(0, at - PAGE); draw(); });
    $('#next').addEventListener('click', () => { at += PAGE; draw(); });
    ctx.on('log:changed', draw);
  }
});
