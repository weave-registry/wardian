/* chart: how many rows fall in each group of one column, counted in the database with GROUP BY for
   the rows the filter keeps. Numbers group into about 20 equal ranges, dates into months, years or
   decades, and text into its 15 most common values. Bars (draw.js) draws the SVG.
   Listens: dataset:ready, view:changed (only the filter matters here).  Capabilities: db. */
Kernel.register({
  name: 'chart',
  listens: ['dataset:ready', 'view:changed'],
  caps: ['db'],
  init(ctx){
    const $ = ctx.$;
    const TOP = 15;
    let db = null, dbKnown = false, data = null, find = '', col = -1, shown = null, run = 0, result = null;
    const empty = text => Object.assign(ctx.el('p'), {className: 'empty', textContent: text});
    const compact = new Intl.NumberFormat(undefined, {notation: 'compact', maximumFractionDigits: 1});
    const q1 = async (sql, params) => (await db.query({sql, params})).rows;

    // Ranges of one step for a number column; empty ranges are kept so the shape stays honest.
    async function numberGroups(t, q, where, params){
      const [[lo, hi, n]] = await q1('SELECT min(' + q + '), max(' + q + '), count(*) FROM ' + t + ' WHERE ' + where, params);
      if (!n) return [];
      if (lo === hi) return [{label: Data.num(lo), short: Data.num(lo), n}];
      const step = Bars.step(hi - lo, 20), start = Math.floor(lo / step) * step, count = Math.floor((hi - start) / step) + 1;
      const k = params.length;
      const rows = await q1('SELECT CAST((' + q + ' - ?' + (k + 1) + ') / ?' + (k + 2) + ' AS INTEGER) AS b, count(*) FROM ' + t + ' WHERE ' + where + ' GROUP BY b', params.concat(start, step));
      const at = new Array(count).fill(0);
      for (const [b, c] of rows) at[Math.max(0, Math.min(count - 1, b))] += c;
      const edge = i => +(start + i * step).toPrecision(12);
      return at.map((c, i) => ({label: Data.num(edge(i)) + ' to under ' + Data.num(edge(i + 1)), short: compact.format(edge(i)), n: c, edge: true}));
    }

    // Months, years or decades, whichever gives a readable number of groups; gaps count as zero.
    async function dateGroups(t, q, where, params){
      const [[lo, hi]] = await q1('SELECT min(' + q + '), max(' + q + ') FROM ' + t + ' WHERE ' + where, params);
      if (lo == null) return [];
      const y0 = +lo.slice(0, 4), y1 = +hi.slice(0, 4), span = y1 - y0;
      const unit = span <= 2 ? 'month' : span <= 60 ? 'year' : 'decade';
      const key = unit === 'month' ? 'substr(' + q + ', 1, 7)' : unit === 'year' ? 'CAST(substr(' + q + ', 1, 4) AS INTEGER)' : '(CAST(substr(' + q + ', 1, 4) AS INTEGER) / 10) * 10';
      const counts = new Map(await q1('SELECT ' + key + ' AS k, count(*) FROM ' + t + ' WHERE ' + where + ' GROUP BY k', params));
      const keys = [];
      if (unit === 'month') for (let y = y0, m = +lo.slice(5, 7); y < y1 || (y === y1 && m <= +hi.slice(5, 7)); m === 12 ? (y++, m = 1) : m++) keys.push(y + '-' + String(m).padStart(2, '0'));
      else for (let y = unit === 'year' ? y0 : Math.floor(y0 / 10) * 10; y <= y1; y += unit === 'year' ? 1 : 10) keys.push(y);
      return keys.map(k => ({label: unit === 'decade' ? k + 's' : String(k), short: unit === 'decade' ? k + 's' : String(k), n: counts.get(k) || 0}));
    }

    // The most common values; the rest are added up as one "Other" bar.
    async function textGroups(t, q, where, params){
      const rows = await q1('SELECT ' + q + ', count(*) AS n FROM ' + t + ' WHERE ' + where + ' GROUP BY ' + q + ' ORDER BY n DESC, ' + q + ' LIMIT ' + TOP, params);
      const [[filled, distinct]] = await q1('SELECT count(*), count(DISTINCT ' + q + ') FROM ' + t + ' WHERE ' + where, params);
      const short = s => s.length > 24 ? s.slice(0, 23) + '…' : s;
      const groups = rows.map(([v, n]) => ({label: String(v), short: short(String(v)), n}));
      const rest = filled - groups.reduce((a, g) => a + g.n, 0);
      if (rest > 0) groups.push({label: 'Other (' + Data.count(distinct - groups.length) + ' values)', short: 'Other', n: rest});
      return groups;
    }

    async function compute(){
      const mine = ++run;
      result = null;
      if (!data || !dbKnown || !db || col < 0){ paint(); return; }
      const f = Filter.where(find, data), q = Data.q(data.columns[col]), t = Data.q(data.table), type = data.types[col];
      const where = Data.and(f.where, q + ' IS NOT NULL');
      $('#plot').classList.add('busy');
      try {
        const groups = await (type === 'number' ? numberGroups : type === 'date' ? dateGroups : textGroups)(t, q, where, f.params);
        const [[all, filled]] = await q1('SELECT count(*), count(' + q + ') FROM ' + t + (f.where ? ' WHERE ' + f.where : ''), f.params);
        if (mine !== run) return;
        result = {groups, type, all, filled, field: data.fields[col], filtered: f.active};
      } catch (e){
        if (mine === run) result = {error: Data.errText(e)};
      }
      if (mine === run){ $('#plot').classList.remove('busy'); paint(); }
    }

    function paint(){
      const plot = $('#plot'), cap = $('#caption'), read = $('#readout');
      read.textContent = ''; cap.textContent = ''; $('#asTable').hidden = true;
      if (!data){ plot.replaceChildren(empty(dbKnown ? 'No data yet.' : 'Waiting for data.')); return; }
      if (dbKnown && !db){ plot.replaceChildren(empty('This host has no database for apps.')); return; }
      if (!result) return;
      if (result.error){ plot.replaceChildren(empty('Could not count the rows: ' + result.error)); return; }
      const r = result;
      if (!r.groups.length){ plot.replaceChildren(empty(r.all ? 'Every ' + r.field + ' cell in these rows is empty.' : 'No rows match the filter.')); return; }
      const kind = r.type === 'number' ? 'ranges of ' + r.field : r.type === 'date' ? r.field + ' by ' + (r.groups[0].label.length === 7 ? 'month' : /s$/.test(r.groups[0].label) ? 'decade' : 'year') : 'the most common values of ' + r.field;
      cap.textContent = 'Rows in ' + kind + ', for ' + (r.filtered ? 'the ' + Data.rows(r.all) + ' that match the filter' : 'all ' + Data.rows(r.all)) + '.' + (r.all > r.filled ? ' ' + Data.count(r.all - r.filled) + ' empty cells are left out.' : '');
      const say = i => { read.textContent = i < 0 ? '' : r.groups[i].label + ': ' + Data.rows(r.groups[i].n) + ' (' + (r.groups[i].n / r.filled * 100).toFixed(1) + '%)'; };
      const svg = Bars.draw(r.groups, {width: Math.max(280, plot.clientWidth || 600), columns: r.type !== 'text', onPick: say});
      svg.setAttribute('aria-label', cap.textContent);
      plot.replaceChildren(svg);
      // The same numbers as a table, for reading exact values and for screen readers.
      const t = ctx.el('table'), tb = ctx.el('tbody');
      t.className = 'w-table';
      t.innerHTML = '<thead><tr><th scope="col"></th><th scope="col" class="num">Rows</th></tr></thead>';
      t.querySelector('th').textContent = r.field;
      for (const g of r.groups){
        const tr = ctx.el('tr'), a = ctx.el('td'), b = ctx.el('td');
        a.textContent = g.label; b.className = 'num'; b.textContent = Data.count(g.n);
        tr.append(a, b); tb.appendChild(tr);
      }
      t.appendChild(tb);
      $('#tableOut').replaceChildren(t);
      $('#asTable').hidden = false;
    }

    // A new dataset starts on its first number column that is not an id (a flat, dull chart), else
    // its first number column, else its first column.
    function start(d){
      const i = d.types.findIndex((t, k) => t === 'number' && !/(^|[\s_-])id$|^#$|^no\.?$/i.test(d.fields[k]));
      return i >= 0 ? i : Math.max(0, d.types.indexOf('number'));
    }
    function options(){
      const sel = $('#col');
      $('#pickBox').hidden = !data;
      if (!data){ sel.replaceChildren(); return; }
      sel.replaceChildren(...data.fields.map((f, i) => Object.assign(ctx.el('option'), {value: String(i), textContent: f + ' (' + data.types[i] + ')'})));
      sel.value = String(col);
    }
    $('#col').addEventListener('change', e => { col = Number(e.target.value); maybe(); });

    function maybe(){
      const key = data ? [data.table, data.total, find, col].join('|') : '';
      if (key === shown && dbKnown) return;
      shown = dbKnown ? key : null;
      compute();
    }
    ctx.on('dataset:ready', m => {
      const d = Data.valid(m && m.dataset);
      if (!d || !data || d.table !== data.table){ find = ''; col = d ? start(d) : -1; }
      data = d; options(); maybe();
    });
    ctx.on('view:changed', v => { find = Filter.view(v, data).find; maybe(); });
    let width = 0;
    ctx.observe($('#plot'), () => { const w = $('#plot').clientWidth; if (Math.abs(w - width) > 8 && result){ width = w; paint(); } });
    (async () => { db = await ctx.cap('db').catch(() => null); dbKnown = true; paint(); maybe(); })();
  }
});
