/* stats: one line per column, computed in the database with db.query for the rows the filter keeps:
   how many cells are filled and empty, how many distinct values, and then min, mean and max for
   numbers, the first and last date for dates, or the most common values for text.
   Listens: dataset:ready, view:changed (only the filter matters here, not the sort).
   Capabilities: db. */
Kernel.register({
  name: 'stats',
  listens: ['dataset:ready', 'view:changed'],
  caps: ['db'],
  init(ctx){
    const $ = ctx.$;
    const GROUP = 40;                          // columns per aggregate statement, to keep each one short
    const TOP = 3;                             // most common values shown for a text column
    let db = null, dbKnown = false, data = null, find = '', shown = null, run = 0;
    const empty = text => Object.assign(ctx.el('p'), {className: 'empty', textContent: text});
    const el = (tag, cls, text) => { const e = ctx.el(tag); if (cls) e.className = cls; if (text != null) e.textContent = text; return e; };

    // Counts, distinct values and ranges for every column, a group of columns per statement.
    async function aggregate(t, where, params){
      const out = [];
      let all = 0;
      for (let g = 0; g < data.columns.length; g += GROUP){
        const cols = data.columns.slice(g, g + GROUP), parts = ['count(*)'];
        cols.forEach((c, k) => {
          const q = Data.q(c), num = data.types[g + k] === 'number';
          parts.push('count(' + q + ')', 'count(DISTINCT ' + q + ')', 'min(' + q + ')', 'max(' + q + ')', num ? 'avg(' + q + ')' : 'NULL');
        });
        const r = (await db.query({sql: 'SELECT ' + parts.join(', ') + ' FROM ' + t + where, params})).rows[0];
        all = r[0];
        cols.forEach((_, k) => { const b = 1 + k * 5; out.push({filled: r[b], distinct: r[b + 1], min: r[b + 2], max: r[b + 3], mean: r[b + 4]}); });
      }
      return {all, cols: out};
    }

    async function compute(){
      const mine = ++run, out = $('#statsOut');
      if (!data){ $('#scope').textContent = ''; out.replaceChildren(empty(dbKnown ? 'No data yet.' : 'Waiting for data.')); return; }
      if (!dbKnown) return;
      if (!db){ out.replaceChildren(empty('This host has no database for apps.')); return; }
      const f = Filter.where(find, data), t = Data.q(data.table), where = f.where ? ' WHERE ' + f.where : '';
      out.classList.add('busy');
      try {
        const {all, cols} = await aggregate(t, where, f.params);
        if (mine !== run) return;
        // The most common values of each text column.
        for (let i = 0; i < cols.length; i++){
          if (data.types[i] !== 'text' || !cols[i].filled) continue;
          const q = Data.q(data.columns[i]);
          const r = await db.query({sql: 'SELECT ' + q + ', count(*) AS n FROM ' + t + ' WHERE ' + Data.and(f.where, q + ' IS NOT NULL') + ' GROUP BY ' + q + ' ORDER BY n DESC, ' + q + ' LIMIT ' + TOP, params: f.params});
          if (mine !== run) return;
          cols[i].top = r.rows;
        }
        $('#scope').textContent = f.active ? 'For the ' + Data.rows(all) + ' that match the filter, of ' + Data.count(data.total) + '.' : 'For all ' + Data.rows(all) + '.';
        out.replaceChildren(all ? render(cols, all) : empty('No rows match the filter.'));
      } catch (e){
        if (mine === run) out.replaceChildren(empty('Could not compute the summaries: ' + Data.errText(e)));
      } finally { if (mine === run) out.classList.remove('busy'); }
    }

    function summary(c, type){
      const box = el('span', 'summary');
      if (!c.filled) return box;
      if (type === 'number') box.textContent = 'min ' + Data.num(c.min) + ' · mean ' + Data.num(c.mean) + ' · max ' + Data.num(c.max);
      else if (type === 'date') box.textContent = c.min === c.max ? String(c.min) : c.min + ' → ' + c.max;
      else if (c.distinct === c.filled && c.filled > TOP) box.textContent = 'every value is different';
      else (c.top || []).forEach(([v, n], k) => {
        if (k) box.append(', ');
        box.append(el('span', 'value', v), el('span', 'times', ' ×' + Data.count(n)));
      });
      return box;
    }

    function render(cols, all){
      const t = el('table', 'w-table'), hr = el('tr'), tb = ctx.el('tbody');
      [['Column'], ['Type'], ['Filled', 'num'], ['Empty', 'num'], ['Distinct', 'num'], ['Summary']].forEach(([h, cls]) => {
        const th = el('th', cls, h); th.scope = 'col'; hr.appendChild(th);
      });
      cols.forEach((c, i) => {
        const tr = ctx.el('tr'), type = data.types[i], badge = el('span', 'w-badge', type);
        badge.dataset.variant = type === 'number' ? 'secondary' : 'outline';
        const name = el('td', 'col-name', data.fields[i]), typeCell = ctx.el('td'), sum = ctx.el('td');
        typeCell.appendChild(badge); sum.appendChild(summary(c, type));
        const nulls = all - c.filled;
        tr.append(name, typeCell, el('td', 'num', Data.count(c.filled)), el('td', 'num' + (nulls ? ' warn-num' : ''), Data.count(nulls)), el('td', 'num', Data.count(c.distinct)), sum);
        tb.appendChild(tr);
      });
      const thead = ctx.el('thead'); thead.appendChild(hr); t.append(thead, tb);
      const wrap = el('div', 'w-table-wrap'); wrap.appendChild(t);
      return wrap;
    }

    // Recomputes only when the dataset or the filter changes: sorting does not change a summary.
    function maybe(){
      const key = data ? data.table + '|' + data.total + '|' + find : '';
      if (key === shown && dbKnown) return;
      shown = dbKnown ? key : null;
      compute();
    }
    ctx.on('dataset:ready', m => { const d = Data.valid(m && m.dataset); if (!d || !data || d.table !== data.table) find = ''; data = d; maybe(); });
    ctx.on('view:changed', v => { find = Filter.view(v, data).find; maybe(); });
    (async () => { db = await ctx.cap('db').catch(() => null); dbKnown = true; maybe(); })();
  }
});
