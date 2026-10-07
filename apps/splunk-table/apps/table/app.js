/* table: runs a Splunk search on the Wardian server and shows every row it returns.
   The time range can be a ready-made one, "last N minutes/hours/days…", two dates, or Splunk time codes.
   "Find words in all my data" builds a keyword search for you; "Find in results" filters the rows shown.
   Sends the table on the channel "splunk.table", so other apps (the USL lab) can use it.
   With the db capability (ADR-2610071219) the results load into this app's own SQLite table on the
   server, up to a million rows: the page shows 100 at a time, and sorting, "Find in results" and the
   CSV run in the database. The channel then carries a reference to the table, not its rows.
   Capabilities: storage, splunk, db, claude:sample, claude:downloads.  Channels: sends splunk.table. */
Kernel.register({
  name: 'table',
  caps: ['storage', 'splunk', 'db', 'claude:sample', 'claude:downloads'],
  channels: {send: ['splunk.table']},
  init(ctx){
    const $ = ctx.$;
    const SHOW_ROWS = 1000;                    // rows drawn on the page; the CSV and the channel carry more
    const CHANNEL_BYTES = 240 * 1024;          // a channel message may be at most 256 KB
    const PAGE = 100;                          // rows per page when the table is in the database
    const INLINE_ROWS = 1000;                  // a table in the database also travels inline up to this many rows
    const TABLE_NAME = 'search';               // this app's table in its database
    let db = null;                             // ctx.cap('db'), or null on a host without it
    let dbKnown = false;                       // whether the host has answered about db yet
    let pageAt = 0;                            // the first row on the page (database mode)
    let pageView = null;                       // {columns, rows, total} of the page shown

    const bar = $('#bar');
    function status(text, kind){ const st = $('#status'); st.className = 'status' + (kind ? ' ' + kind : ''); st.textContent = text; }
    const isNum = v => v !== null && v !== '' && !Array.isArray(v) && isFinite(Number(String(v).replace(/,/g, '')));
    const num = v => Number(String(v).replace(/,/g, ''));

    // ---------- time range ----------
    // Ready-made options hold "earliest|latest" in Splunk time codes. "rel", "abs" and "adv" read the extra fields.
    const UNITS = {m: ['minute', 'minutes'], h: ['hour', 'hours'], d: ['day', 'days'], w: ['week', 'weeks'], mon: ['month', 'months']};
    const CODE = /^[A-Za-z0-9@+\-:\/._]{1,40}$/;   // what a Splunk time code may look like
    const fmtTime = t => new Date(t).toLocaleString([], {dateStyle: 'medium', timeStyle: 'short'});
    const pad = n => String(n).padStart(2, '0');
    const toLocalInput = d => d.getFullYear() + '-' + pad(d.getMonth() + 1) + '-' + pad(d.getDate()) + 'T' + pad(d.getHours()) + ':' + pad(d.getMinutes());

    // Returns {earliest, latest, label}, or {error} with a short message.
    function readRange(){
      const v = $('#range').value;
      if (v === 'rel'){
        const n = Number($('#relN').value), u = $('#relUnit').value;
        if (!Number.isInteger(n) || n < 1 || n > 100000 || !UNITS[u]) return {error: 'Type a whole number of 1 or more under How many.'};
        return {earliest: '-' + n + u, latest: '', label: 'Last ' + n + ' ' + UNITS[u][n === 1 ? 0 : 1]};
      }
      if (v === 'abs'){
        const a = $('#absFrom').value, b = $('#absTo').value;
        const ta = a ? new Date(a).getTime() : NaN, tb = b ? new Date(b).getTime() : Date.now();
        if (!a || isNaN(ta)) return {error: 'Choose the date and time to start from.'};
        if (isNaN(tb)) return {error: 'The end date is not valid.'};
        if (tb <= ta) return {error: 'The end must be after the start.'};
        return {earliest: String(Math.floor(ta / 1000)), latest: b ? String(Math.floor(tb / 1000)) : '',
          label: fmtTime(ta) + ' to ' + (b ? fmtTime(tb) : 'now')};
      }
      if (v === 'adv'){
        const e = $('#advE').value.trim(), l = $('#advL').value.trim();
        if ((e && !CODE.test(e)) || (l && !CODE.test(l))) return {error: 'Splunk time codes look like -2d@d, @w1, -90m or now.'};
        return {earliest: e, latest: l, label: 'Splunk time ' + (e || 'all time') + ' to ' + (l || 'now')};
      }
      const [e = '', l = ''] = v.split('|');
      const opt = $('#range').selectedOptions[0];
      return {earliest: e, latest: l, label: opt ? opt.textContent : ''};
    }

    // Shows the extra fields for the chosen kind of range, and what it means or what is wrong.
    function showRange(){
      const v = $('#range').value;
      $('#rangeRel').hidden = v !== 'rel'; $('#rangeAbs').hidden = v !== 'abs'; $('#rangeAdv').hidden = v !== 'adv';
      const hint = $('#rangeHint');
      if (!['rel', 'abs', 'adv'].includes(v)){ hint.textContent = ''; hint.className = 'w-hint range-hint'; return; }
      const r = readRange();
      hint.className = 'w-hint range-hint' + (r.error ? ' warn' : '');
      hint.textContent = r.error || ('Searches: ' + r.label + '.');
    }

    // Picks the time range for a Splunk "earliest" code (from a ready-made search or from Claude).
    function setEarliest(e){
      e = typeof e === 'string' ? e.trim() : '-7d';
      const v = e + '|';
      if ([...$('#range').options].some(o => o.value === v)) $('#range').value = v;
      else if (CODE.test(e)){ $('#range').value = 'adv'; $('#advE').value = e; $('#advL').value = ''; }
      else $('#range').value = '-7d|';
      showRange();
    }

    // ---------- ready-made searches ----------
    // Templates only: change the index and field names to match your data. Production traffic has no
    // load level, so the traffic one measures it once a minute with Little's Law: requests in progress
    // = throughput x average time per request. "minutes" says how many minutes each row averages, so
    // you can see which rows rest on little data.
    const LITTLE = (filter, durationField, minMinutes) => filter +
      ' | bin _time span=1m | stats count AS reqs avg(' + durationField + ') AS r_ms BY _time' +
      ' | eval x=reqs/60, n=x*r_ms/1000 | eval n=round(n*2)/2 | where n>0' +
      ' | stats count AS minutes avg(x) AS x avg(r_ms) AS r BY n | where minutes>=' + minMinutes +
      ' | sort n | table n x r minutes';
    const PRESETS = {
      loadtest: {search: 'index=loadtest | stats avg(throughput) AS x avg(latency_ms) AS r BY concurrency | table concurrency x r',
        range: '-24h', units: {n: 'users', x: 'req/s', r: 'ms'}, use: {load: 'concurrency', throughput: 'x', response: 'r'},
        note: 'Change the index and field names to match your load test, then run it.',
        about: {title: 'Load test from Splunk', load: 'Concurrent users in each step of the test', throughput: 'Requests completed per second in that step',
          response: 'Average response time in that step, in milliseconds', method: 'Each point is one load level of the test, averaged over all its samples.'}},
      traffic: {search: LITTLE('index=web sourcetype=app_requests status=200 duration_ms=*', 'duration_ms', 10),
        range: '-7d', units: {n: 'requests in progress', x: 'req/s', r: 'ms'}, use: {load: 'n', throughput: 'x', response: 'r'},
        note: 'Change the index, sourcetype and duration field to match your request logs: one event per finished request, with its time in milliseconds. The minutes column shows how many minutes each row averages.',
        about: {title: 'Requests in production (live traffic)',
          load: 'How many requests were being handled at the same time, on average, during a minute',
          throughput: 'Requests finished per second during those minutes',
          response: 'Average time to answer one request, in milliseconds',
          method: 'Not a load test: real users made this traffic. Splunk counts the finished requests in each minute and their average time. Requests in progress = requests per second × seconds per request (Little\'s Law). Minutes with the same load are averaged into one point; a load seen in fewer than 10 minutes is left out.'}},
    };

    // ---------- "Find words in all my data" ----------
    // Builds a plain keyword search: every word (or "quoted phrase") must appear in the event.
    function wordList(){
      return ($('#words').value.match(/"[^"]*"|[^\s"]+/g) || [])
        .map(w => w.replace(/["\\]/g, '').trim()).filter(Boolean);
    }
    function findSearch(){
      const words = wordList().map(w => '"' + w + '"');
      const idx = $('#wordsIndex').value.trim().replace(/[^\w\-*.]/g, '') || '*';
      return 'index=' + idx + (words.length ? ' ' + words.join(' ') : '') +
        ' | head 1000 | table _time index sourcetype host source _raw';
    }
    function fillFind(){
      $('#spl').value = findSearch();
      const w = wordList();
      $('#title').value = w.length ? 'Events with: ' + w.join(', ').slice(0, 80) : 'Events with words';
      meta = null;
      persist();
    }

    // What the search in the box means, for the apps that use the table. Cleared when you edit the search.
    let meta = null;                           // {about, units, use}
    let table = null;                          // the latest result, as sent on the channel
    let pending = null;                        // the search running as a job: what it is for
    let handled = '';                          // the id of the last job whose table is shown
    let sort = {col: -1, dir: 1};
    let shown = [];                            // rows that match "Find in results", in sorted order

    const saved = ctx.store.get('state') || {};
    $('#spl').value = typeof saved.s === 'string' ? saved.s : '';
    // Default dates for "Between two dates": from the start of yesterday until now.
    const dayAgo = new Date(); dayAgo.setDate(dayAgo.getDate() - 1); dayAgo.setHours(0, 0, 0, 0);
    $('#absFrom').value = toLocalInput(dayAgo);
    const tr = saved.tr && typeof saved.tr === 'object' ? saved.tr : {};
    if (typeof tr.n === 'string' || typeof tr.n === 'number') $('#relN').value = tr.n;
    if (UNITS[tr.u]) $('#relUnit').value = tr.u;
    if (typeof tr.from === 'string' && tr.from) $('#absFrom').value = tr.from;
    if (typeof tr.to === 'string') $('#absTo').value = tr.to;
    if (typeof tr.e === 'string') $('#advE').value = tr.e;
    if (typeof tr.l === 'string') $('#advL').value = tr.l;
    if (typeof saved.t === 'string'){
      // Older versions saved only the earliest code, such as "-24h".
      const t = saved.t.includes('|') || ['rel', 'abs', 'adv'].includes(saved.t) ? saved.t : saved.t + '|';
      $('#range').value = t;
      if ($('#range').value !== t) $('#range').value = '-24h|';
    }
    $('#title').value = typeof saved.name === 'string' ? saved.name : '';
    if (typeof saved.words === 'string') $('#words').value = saved.words;
    if (typeof saved.wordsIndex === 'string') $('#wordsIndex').value = saved.wordsIndex;
    if (saved.preset === 'find'){ $('#preset').value = 'find'; $('#findWords').hidden = false; }
    if (saved.meta && typeof saved.meta === 'object') meta = saved.meta;
    if (saved.table && Array.isArray(saved.table.fields)) table = saved.table;
    if (saved.pending && typeof saved.pending === 'object' && typeof saved.pending.search === 'string') pending = saved.pending;
    if (typeof saved.handled === 'string') handled = saved.handled;
    function persist(){
      const state = {s: $('#spl').value, t: $('#range').value, name: $('#title').value, meta, table, pending, handled,
        preset: $('#preset').value, words: $('#words').value, wordsIndex: $('#wordsIndex').value,
        tr: {n: $('#relN').value, u: $('#relUnit').value, from: $('#absFrom').value, to: $('#absTo').value, e: $('#advE').value, l: $('#advL').value}};
      // A big table may not fit in this browser's storage: then keep the search and drop the rows.
      if (JSON.stringify(state).length > 1500000) state.table = null;
      ctx.store.set('state', state);
    }
    $('#spl').addEventListener('input', () => { meta = null; persist(); });
    $('#range').addEventListener('change', () => { showRange(); persist(); });
    for (const id of ['#relN', '#relUnit', '#absFrom', '#absTo', '#advE', '#advL']){
      $(id).addEventListener('input', () => { showRange(); persist(); });
      $(id).addEventListener('change', () => { showRange(); persist(); });
    }
    $('#title').addEventListener('input', persist);
    for (const id of ['#words', '#wordsIndex']){
      $(id).addEventListener('input', () => { if ($('#preset').value === 'find') fillFind(); });
      $(id).addEventListener('keydown', e => { if (e.key === 'Enter') run(); });
    }
    showRange();

    function useSearch(p){
      $('#spl').value = p.search;
      if (p.range !== undefined) setEarliest(p.range);
      if (p.about && p.about.title) $('#title').value = p.about.title;
      meta = {about: p.about || null, units: p.units || null, use: p.use || null};
      persist();
    }
    $('#preset').addEventListener('change', () => {
      const v = $('#preset').value;
      $('#findWords').hidden = v !== 'find';
      if (v === 'find'){
        fillFind();
        status('Type the words to find. Each word must appear in the event; put a phrase in "quotes". Then press Run search.');
        $('#words').focus();
        return;
      }
      persist();
      const p = PRESETS[v]; if (!p) return;
      useSearch(p); status(p.note);
    });

    // ---------- "Find in results": filter the rows on the page ----------
    // Splits the text into words and rules. A word matches any cell; a rule looks in one column.
    // Returns {test(row), problems: [short messages]}.
    function parseFind(text, fields){
      const lower = fields.map(f => String(f).toLowerCase());
      const tests = [], problems = [];
      const tokens = text.match(/(?:[^\s"]+|"[^"]*")+/g) || [];
      for (const tok of tokens){
        const m = tok.match(/^([^=<>!"]+)(!=|>=|<=|=|>|<)(.*)$/);
        if (!m){
          const w = tok.replace(/"/g, '').toLowerCase();
          if (w) tests.push(r => r.some(v => v != null && String(v).toLowerCase().includes(w)));
          continue;
        }
        const i = lower.indexOf(m[1].toLowerCase());
        if (i < 0){ problems.push('There is no column called ' + m[1] + '.'); continue; }
        const op = m[2], val = m[3].replace(/"/g, '');
        const cell = r => (r[i] == null ? '' : String(r[i]));
        if (op === '=') tests.push(r => cell(r).toLowerCase().includes(val.toLowerCase()));
        else if (op === '!=') tests.push(r => !cell(r).toLowerCase().includes(val.toLowerCase()));
        else {
          if (!isNum(val)){ problems.push(m[1] + op + ' needs a number after it.'); continue; }
          const n = num(val);
          const cmp = {'>': (a, b) => a > b, '<': (a, b) => a < b, '>=': (a, b) => a >= b, '<=': (a, b) => a <= b}[op];
          tests.push(r => isNum(r[i]) && cmp(num(r[i]), n));
        }
      }
      return {test: r => tests.every(t => t(r)), problems, active: tests.length > 0};
    }
    // The same rules as a condition for the database: {where, params, problems}. Each value is a
    // bound parameter; columns are the table's own names, matched from the fields people see.
    function findWhere(text, fields, columns){
      const lower = fields.map(f => String(f).toLowerCase());
      const parts = [], params = [], problems = [];
      const like = v => '%' + String(v).replace(/[\\%_]/g, c => '\\' + c) + '%';
      const tokens = text.match(/(?:[^\s"]+|"[^"]*")+/g) || [];
      for (const tok of tokens){
        const m = tok.match(/^([^=<>!"]+)(!=|>=|<=|=|>|<)(.*)$/);
        if (!m){
          const w = tok.replace(/"/g, '');
          if (!w) continue;
          params.push(like(w));
          const n = params.length;
          parts.push('(' + columns.map(c => 'CAST("' + c + '" AS TEXT) LIKE ?' + n + " ESCAPE '\\'").join(' OR ') + ')');
          continue;
        }
        const i = lower.indexOf(m[1].toLowerCase());
        if (i < 0){ problems.push('There is no column called ' + m[1] + '.'); continue; }
        const op = m[2], val = m[3].replace(/"/g, ''), col = '"' + columns[i] + '"';
        if (op === '=' || op === '!='){
          params.push(like(val));
          parts.push(op === '=' ? 'CAST(' + col + ' AS TEXT) LIKE ?' + params.length + " ESCAPE '\\'" : '(' + col + ' IS NULL OR CAST(' + col + ' AS TEXT) NOT LIKE ?' + params.length + " ESCAPE '\\')");
        } else {
          if (!isNum(val)){ problems.push(m[1] + op + ' needs a number after it.'); continue; }
          params.push(num(val));
          parts.push('CAST(' + col + ' AS REAL) ' + op + ' ?' + params.length);
        }
      }
      return {where: parts.join(' AND '), params, problems, active: parts.length > 0};
    }
    // One page request for the table in the database, with the sort and "Find in results".
    function pageRequest(offset, limit){
      const d = table.dataset, f = findWhere($('#find').value.trim(), table.fields, d.columns);
      return {req: {table: d.table, offset, limit, orderBy: sort.col >= 0 ? d.columns[sort.col] : '', desc: sort.dir < 0, where: f.where, params: f.params}, find: f};
    }
    let findTimer = null;
    $('#find').addEventListener('input', () => { clearTimeout(findTimer); findTimer = setTimeout(() => { pageAt = 0; draw(); }, 200); });

    // ---------- the result ----------
    // One table element for rows: header clicks sort; `numeric` right-aligns number columns.
    function tableEl(fields, rows, numeric){
      const t = ctx.el('table'), thead = ctx.el('thead'), hr = ctx.el('tr');
      t.className = 'w-table';
      fields.forEach((f, i) => {
        const th = ctx.el('th'); th.textContent = f; th.title = 'Sort by ' + f; th.scope = 'col';
        if (numeric[i]) th.className = 'num';
        if (sort.col === i){ const a = ctx.el('span'); a.className = 'arrow'; a.textContent = sort.dir > 0 ? '▲' : '▼'; th.appendChild(a); }
        th.addEventListener('click', () => { sort = {col: i, dir: sort.col === i ? -sort.dir : 1}; pageAt = 0; draw(); });
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

    function drawFacts(){
      const facts = $('#facts');
      facts.replaceChildren();
      const fact = (k, v, code) => {
        if (!v) return;
        const dt = ctx.el('dt'), dd = ctx.el('dd'); dt.textContent = k;
        if (code){ const c = ctx.el('code'); c.textContent = v; dd.appendChild(c); } else dd.textContent = v;
        facts.append(dt, dd);
      };
      const n = table.dataset ? table.dataset.total : table.rows.length;
      fact('Table', table.title);
      fact('Rows', n.toLocaleString() + (n === 1 ? ' row' : ' rows') + ' × ' + table.fields.length + ' columns' +
        (table.truncated ? ' (Splunk returned more; only the first ' + n.toLocaleString() + ' are kept)' : '') + (table.dataset ? ', kept in this app\'s database' : ''));
      fact('Time range', table.rangeLabel);
      fact('Fetched', new Date(table.at).toLocaleString() + (table.seconds ? ', the search took ' + table.seconds + ' s' : ''));
      fact('Search', table.search, true);
      facts.hidden = false;
    }

    // Database mode: asks for one page and draws it, with "rows X–Y of N" and Previous / Next.
    let drawing = 0;
    async function drawPage(){
      const out = $('#out'), hint = $('#findHint'), pager = $('#pager'), mine = ++drawing;
      const {req, find} = pageRequest(pageAt, PAGE);
      if (find.problems.length){ hint.className = 'w-hint warn'; hint.textContent = find.problems.join(' '); }
      let page;
      try { page = await db.page(req); }
      catch (e){ if (mine === drawing){ out.replaceChildren(Object.assign(ctx.el('p'), {className: 'empty', textContent: 'Could not read the table: ' + (e && e.message || e)})); pager.hidden = true; } return; }
      if (mine !== drawing) return;                         // a newer request was made meanwhile
      pageView = page;
      const total = table.dataset.total;
      if (!find.problems.length){
        if (find.active){ hint.className = 'w-hint'; hint.textContent = page.total.toLocaleString() + ' of ' + total.toLocaleString() + ' rows match. Download CSV saves the matching rows.'; }
        else { hint.className = 'w-hint'; hint.textContent = 'Words keep rows that contain them anywhere. Use column=text, column!=text, or column>number (also <, >=, <=) to look in one column. Put spaces inside "quotes".'; }
      }
      if (!page.total){
        out.replaceChildren(Object.assign(ctx.el('p'), {className: 'empty', textContent: total ? 'No rows match what you typed under Find in results.' : 'The search returned no rows. Try a longer time range.'}));
        pager.hidden = true; return;
      }
      const numeric = table.fields.map((_, i) => page.rows.every(r => r[i] == null || r[i] === '' || isNum(r[i])));
      out.replaceChildren(tableEl(table.fields, page.rows, numeric));
      const last = Math.min(pageAt + page.rows.length, page.total);
      $('#pageInfo').textContent = 'Rows ' + (pageAt + 1).toLocaleString() + '–' + last.toLocaleString() + ' of ' + page.total.toLocaleString();
      $('#prev').disabled = pageAt === 0;
      $('#next').disabled = last >= page.total;
      pager.hidden = false;
    }
    $('#prev').addEventListener('click', () => { pageAt = Math.max(0, pageAt - PAGE); draw(); });
    $('#next').addEventListener('click', () => { pageAt += PAGE; draw(); });

    function draw(){
      const out = $('#out'), facts = $('#facts'), hint = $('#findHint');
      $('#btnSend').disabled = !table; $('#btnCsv').disabled = !table || !downloads;
      $('#findBox').hidden = !table || !(table.dataset ? table.dataset.total : table.rows.length);
      $('#pager').hidden = true;
      shown = [];
      if (!table){ facts.hidden = true; return; }
      if (table.dataset){
        if (!dbKnown) return;                               // drawn again once the host answers
        if (!db){ out.replaceChildren(Object.assign(ctx.el('p'), {className: 'empty', textContent: 'This table is in the database, which this host does not offer. Run the search again.'})); return; }
        drawFacts();
        drawPage();
        return;
      }
      drawFacts();

      if (!table.rows.length){ out.replaceChildren(Object.assign(ctx.el('p'), {className: 'empty', textContent: 'The search returned no rows. Try a longer time range.'})); return; }
      const numeric = table.fields.map((_, i) => table.rows.every(r => r[i] == null || r[i] === '' || isNum(r[i])));

      // Filter with "Find in results", then sort.
      const find = parseFind($('#find').value.trim(), table.fields);
      let rows = find.active ? table.rows.filter(r => { try { return find.test(r); } catch { return false; } }) : table.rows;
      if (find.problems.length){ hint.className = 'w-hint warn'; hint.textContent = find.problems.join(' '); }
      else if (find.active){ hint.className = 'w-hint'; hint.textContent = rows.length + ' of ' + table.rows.length + ' rows match. Download CSV saves the matching rows.'; }
      else { hint.className = 'w-hint'; hint.textContent = 'Words keep rows that contain them anywhere. Use column=text, column!=text, or column>number (also <, >=, <=) to look in one column. Put spaces inside "quotes".'; }

      if (sort.col >= 0){
        const i = sort.col;
        rows = rows.slice().sort((a, b) => sort.dir * (numeric[i] ? num(a[i]) - num(b[i]) : String(a[i] ?? '').localeCompare(String(b[i] ?? ''))));
      }
      shown = rows;
      if (!rows.length){ out.replaceChildren(Object.assign(ctx.el('p'), {className: 'empty', textContent: 'No rows match what you typed under Find in results.'})); return; }

      const parts = [tableEl(table.fields, rows.slice(0, SHOW_ROWS), numeric)];
      if (rows.length > SHOW_ROWS) parts.push(Object.assign(ctx.el('p'), {className: 'note', textContent: 'Showing the first ' + SHOW_ROWS + ' of ' + rows.length + ' rows. Download the CSV to see all of them.'}));
      out.replaceChildren(...parts);
    }

    // ---------- sending to other apps ----------
    // The message carries the rows and everything needed to judge them: the search, the time range,
    // when it ran, and (for ready-made searches) what each column means.
    async function share(){
      if (!table) return;
      if (table.dataset) return shareDataset();
      let msg = Object.assign({}, table, {cut: false});
      if (JSON.stringify(msg).length > CHANNEL_BYTES){
        let keep = table.rows.length;
        while (keep > 0 && JSON.stringify(Object.assign({}, msg, {rows: table.rows.slice(0, keep)})).length > CHANNEL_BYTES) keep = Math.floor(keep * 0.8);
        msg = Object.assign({}, msg, {rows: table.rows.slice(0, keep), cut: true});
      }
      try {
        await ctx.channel('splunk.table').send(msg);
        return 'Sent to other apps' + (msg.cut ? ', but only the first ' + msg.rows.length + ' rows fit in one message' : '') + '.';
      } catch (e) {
        return 'Not sent to other apps: ' + (e && e.message || e) + '. Settings → App permissions can change that.';
      }
    }
    // A table in the database travels as a reference: the receiving app reads the pages it needs,
    // after the user allows it. Up to INLINE_ROWS rows also travel inline, for apps that read only rows.
    async function shareDataset(){
      const d = table.dataset;
      let rows = [];
      try {
        for (let at = 0; at < Math.min(d.total, INLINE_ROWS); at += 1000){
          const p = await db.page({table: d.table, offset: at, limit: Math.min(1000, INLINE_ROWS - at)});
          rows = rows.concat(p.rows);
        }
      } catch { rows = []; }
      const msg = Object.assign({}, table, {rows, cut: rows.length < d.total, dataset: {package: 'splunk-table', table: d.table, total: d.total, columns: d.columns, fields: table.fields}});
      if (JSON.stringify(msg).length > CHANNEL_BYTES){
        let keep = rows.length;
        while (keep > 0 && JSON.stringify(Object.assign({}, msg, {rows: rows.slice(0, keep)})).length > CHANNEL_BYTES) keep = Math.floor(keep * 0.8);
        msg.rows = rows.slice(0, keep); msg.cut = true;
      }
      try {
        await ctx.channel('splunk.table').send(msg);
        return msg.rows.length >= d.total ? 'Sent to other apps.'
          : 'Sent to other apps: all ' + d.total.toLocaleString() + ' rows, which they read from this app\'s database after you allow it.';
      } catch (e) {
        return 'Not sent to other apps: ' + (e && e.message || e) + '. Settings → App permissions can change that.';
      }
    }
    $('#btnSend').addEventListener('click', async () => { status(await share()); });

    // ---------- CSV ----------
    // Saves the rows on the page: all of them, or only those that match "Find in results".
    let downloads = null;
    ctx.cap('downloads').then(d => { downloads = d; draw(); }).catch(() => {});
    const csvCell = v => { const s = v == null ? '' : String(v); return /[",\n\r]/.test(s) ? '"' + s.replace(/"/g, '""') + '"' : s; };
    $('#btnCsv').addEventListener('click', async () => {
      if (!table || !downloads) return;
      if (table.dataset) return csvFromDatabase();
      const rows = $('#find').value.trim() ? shown : table.rows;
      const lines = [table.fields.map(csvCell).join(',')].concat(rows.map(r => r.map(csvCell).join(',')));
      const name = (table.title || 'splunk-table').replace(/[^\w.\- ]+/g, ' ').trim().slice(0, 80) || 'splunk-table';
      try { await downloads.save({filename: name + '.csv', data: lines.join('\n') + '\n'}); }
      catch (e) { status('Could not save the file: ' + (e && e.message || e), 'warn'); }
    });

    // Reads every matching row from the database, a thousand at a time, in the order shown.
    async function csvFromDatabase(){
      $('#btnCsv').disabled = true;
      const lines = [table.fields.map(csvCell).join(',')];
      try {
        const first = await db.page(pageRequest(0, 1).req);
        const total = first.total;
        bar.start('Writing the CSV', {value: 0, max: total || 1});
        for (let at = 0; at < total; at += 1000){
          const p = await db.page(pageRequest(at, 1000).req);
          for (const r of p.rows) lines.push(r.map(csvCell).join(','));
          bar.update({value: Math.min(at + 1000, total), detail: Math.min(at + 1000, total).toLocaleString() + ' of ' + total.toLocaleString() + ' rows'});
        }
        const name = (table.title || 'splunk-table').replace(/[^\w.\- ]+/g, ' ').trim().slice(0, 80) || 'splunk-table';
        await downloads.save({filename: name + '.csv', data: lines.join('\n') + '\n'});
        bar.done((lines.length - 1).toLocaleString() + ' rows saved');
      } catch (e) {
        bar.fail('Could not save the file');
        status('Could not save the file: ' + (e && e.message || e), 'warn');
      } finally { $('#btnCsv').disabled = false; }
    }

    // ---------- running the search ----------
    // The host runs each search as a background job (ADR-2610072118), so leaving this app does not
    // stop it. What the search is for is kept as `pending` until it ends; opening the app again
    // finds the job, shows its progress, and shows the table when it is done.
    let jobNow = null;                         // the id of the job this page is waiting on
    const kindOf = p => p.db ? 'splunk.into' : 'splunk.search';
    // The newest job of this app that matches `p`, the search waiting to finish.
    async function findJob(splunk, p){
      const jobs = await splunk.jobs().catch(() => []);
      return (jobs || []).find(j => j.app === 'table' && j.kind === kindOf(p) && j.started >= p.at - 60000) || null;
    }
    // Waits for `promise` (the search or load), showing the rows loaded so far, as the host reports them.
    async function follow(promise, splunk, p){
      let busy = false;
      const tick = setInterval(async () => {
        if (busy) return;
        busy = true;
        try {
          const j = await findJob(splunk, p);
          if (j && j.state === 'running'){
            jobNow = j.id;
            if (j.progress != null) bar.update({detail: j.progress.toLocaleString() + (j.progress === 1 ? ' row' : ' rows') + ' loaded so far'});
          }
        } finally { busy = false; }
      }, 2000);
      try { return await promise; } finally { clearInterval(tick); jobNow = null; }
    }
    bar.addEventListener('cancel', async () => {
      const splunk = await ctx.cap('splunk').catch(() => null);
      if (!splunk || !pending) return;
      const id = jobNow || ((await findJob(splunk, pending)) || {}).id;
      if (id) await splunk.cancel(id).catch(e => status('Could not stop the search: ' + (e && e.message || e), 'warn'));
    });

    // Shows a finished search: `res` is what the host answered, `p` what the search was for.
    async function finish(res, p, why){
      pending = null;
      handled = res.job || handled;
      table = {
        title: p.title, fields: res.fields || [], rows: p.db ? [] : (res.rows || []), truncated: !!res.truncated,
        search: p.search, range: p.range, latest: p.latest, rangeLabel: p.rangeLabel,
        at: new Date().toISOString(), seconds: res.seconds || 0,
        about: p.about || null, units: p.units || null, use: p.use || null,
      };
      if (p.db) table.dataset = {table: res.table, total: res.total || 0, columns: res.columns || []};
      sort = {col: -1, dir: 1}; pageAt = 0;
      $('#find').value = '';
      persist(); draw();
      const n = p.db ? table.dataset.total : table.rows.length;
      bar.done(n.toLocaleString() + (n === 1 ? ' row' : ' rows') + ' in ' + bar.seconds + ' s');
      const msgs = (res.messages || []).filter(Boolean);
      const sent = n ? await share() : '';
      status([why, n.toLocaleString() + (n === 1 ? ' row' : ' rows') + ' from Splunk' + (p.db ? ', kept in this app\'s database.' : '.'), sent, msgs.length ? 'Splunk says: ' + msgs.join(' ') : ''].filter(Boolean).join(' '),
        table.truncated || !n ? 'warn' : '');
    }
    function failed(e){
      pending = null; persist();
      if (/^cancelled/.test(String(e && e.message || e))){ bar.fail('Stopped'); status('You stopped the search.' + (table ? ' The table below is the one from before.' : ''), 'warn'); return; }
      bar.fail('Search failed');
      status('Search failed: ' + (e && e.message || e), 'warn');
    }

    async function run(why){
      const splunk = await ctx.cap('splunk').catch(() => null);
      if (!splunk){ status('Splunk is not set up in this Wardian.', 'warn'); return; }
      if (!why && $('#preset').value === 'find' && !wordList().length){ status('Type the words to find first.', 'warn'); $('#words').focus(); return; }
      const search = $('#spl').value.trim();
      if (!search){ status('Type a search first, or pick one under Start from.', 'warn'); $('#spl').focus(); return; }
      const range = readRange();
      if (range.error){ status(range.error, 'warn'); showRange(); return; }
      $('#btnRun').disabled = true;
      // Splunk does not say how far a search has got, so the bar shows a clock rather than a share.
      if (!why) bar.start('Searching Splunk', {detail: 'A search over many days can take a few minutes. You may leave this app meanwhile; Wardian stops it after 15.', cancelable: true});
      else bar.update({label: 'Running the search', detail: ''});
      status('');
      const m = meta || {};
      const p = {search, range: range.earliest, latest: range.latest, rangeLabel: range.label, db: !!db, at: Date.now(),
        title: $('#title').value.trim() || (m.about && m.about.title) || 'Splunk search',
        about: m.about || null, units: m.units || null, use: m.use || null};
      pending = p;
      persist();
      try {
        const q = {search, earliest: range.earliest};
        if (range.latest) q.latest = range.latest;
        // With the database, the rows go into this app's table, read from Splunk in chunks; the page
        // then shows 100 rows at a time.
        const res = await follow(db ? db.searchInto(Object.assign({table: TABLE_NAME}, q)) : splunk.search(q), splunk, p);
        await finish(res, p, why);
      } catch (e) {
        failed(e);
      } finally { $('#btnRun').disabled = false; }
    }

    // Opening the app again: a search started earlier may still be running, or have finished
    // while the app was closed.
    async function resume(splunk){
      const p = pending;
      if (!p) return false;
      const j = await findJob(splunk, p);
      if (!j){
        pending = null; persist();
        status('The search you started earlier was lost: Wardian restarted while it ran. Run it again.', 'warn');
        return true;
      }
      if (j.id === handled) return false;
      $('#btnRun').disabled = true;
      bar.start(j.state === 'running' ? 'Searching Splunk (started earlier)' : 'Reading the finished search',
        {detail: j.progress != null ? j.progress.toLocaleString() + ' rows loaded so far' : 'You may leave this app meanwhile.', cancelable: j.state === 'running'});
      try { await finish(await follow(splunk.wait(j.id), splunk, p), p, ''); }
      catch (e) { failed(e); }
      finally { $('#btnRun').disabled = false; }
      return true;
    }
    $('#btnRun').addEventListener('click', () => run());

    // ---------- Claude writes the search ----------
    // Find the likely index, read its fields and a few events, then write SPL.
    async function setupAi(){
      const sample = await ctx.cap('sample').catch(() => null);
      if (!sample) return;                                  // no Anthropic key in this Wardian
      $('#aiAsk').hidden = false;
      const noEmails = s => String(s).replace(/[\w.+-]+@[\w-]+(\.[\w-]+)+/g, '<email>');
      $('#btnAi').addEventListener('click', async () => {
        const what = $('#aiWhat').value.trim(), usl = $('#aiUsl').checked;
        if (!what){ status('Describe what you want first.', 'warn'); $('#aiWhat').focus(); return; }
        const splunk = await ctx.cap('splunk');
        $('#btnAi').disabled = true; $('#btnRun').disabled = true;
        try {
          bar.start('Step 1 of 4: listing the data in Splunk', {value: 0, max: 4});
          const inv = await splunk.search({search: '| tstats count WHERE index=* BY index sourcetype | sort -count | head 60', earliest: '-24h'});
          const list = inv.rows.map(r => r.join('  ')).join('\n');
          bar.update({value: 1, label: 'Step 2 of 4: Claude picks where to look'});
          const goal = usl ? 'A user wants to fit the Universal Scalability Law (throughput against load) to data in Splunk.' : 'A user wants a table of data from Splunk.';
          const pick = await sample.json(goal + ' What they want: "' + what.replace(/"/g, "'") + '"\n\n' +
            'Events in the last 24 hours, by index and sourcetype:\n' + list + '\n\n' +
            'Pick the one index and sourcetype most likely to hold this data. Reply as JSON: {"index":"…","sourcetype":"…","why":"one sentence"}', {modelTier: 'quick'});
          if (!pick || !pick.index) throw new Error('Claude did not pick an index.');
          const where = 'index="' + String(pick.index).replace(/"/g, '') + '"' + (pick.sourcetype ? ' sourcetype="' + String(pick.sourcetype).replace(/"/g, '') + '"' : '');
          bar.update({value: 2, label: 'Step 3 of 4: reading the fields of ' + where});
          const fields = await splunk.search({search: where + ' | head 5000 | fieldsummary | where count>50 | sort -count | head 40 | table field count distinct_count', earliest: '-24h'});
          const raws = await splunk.search({search: where + ' | head 2000 | dedup punct | head 6 | table _raw', earliest: '-24h'});
          bar.update({value: 3, label: 'Step 4 of 4: Claude writes the search'});
          const rules = usl ? [
            '- It must start with ' + where + ' and end with "| table <load> <throughput> <response time>": those columns first, in that order, numbers only, one row per load level, load > 0. You may add one more column after them that says how much data each row rests on (for example minutes or count).',
            '- If the data is a load test with a load or concurrency field, group by it.',
            '- If it is production traffic with one event per finished request and a duration field, use Little\'s Law: per 1-minute bucket, throughput x = count/60, response time r = average duration, load n = x * r (in seconds); round n to a step that gives 10 to 30 levels; average x and r per level, and count the minutes per level; keep levels seen in at least 10 minutes.',
            '- Leave out requests that are not normal work, such as long-polls, event streams, health checks or failures, when the fields show them.',
          ] : [
            '- It must start with ' + where + ' and end with "| table <columns>", naming the columns the user wants.',
            '- Keep the result small: aggregate with stats or timechart rather than return raw events, unless the user asks for events.',
          ];
          const out = await sample.json('Write a Splunk search (SPL) for this request.\n\n' +
            'What the user wants: "' + what.replace(/"/g, "'") + '"\nData: ' + where + ' (' + (pick.why || '') + ')\n\n' +
            'Fields (name, events with it, distinct values), from 5000 recent events:\n' + fields.rows.map(r => r.join('  ')).join('\n') + '\n\n' +
            'Sample events:\n' + raws.rows.map(r => noEmails(r[0]).slice(0, 600)).join('\n') + '\n\n' +
            'Rules for the search:\n' + rules.join('\n') + '\n- Use only fields that exist above. Do not invent fields.\n\n' +
            'Reply as JSON: {"search":"…","earliest":"a Splunk time like -7d","explanation":"two sentences for the user: what the search measures and any caveat",' +
            '"title":"a short name for this data"' + (usl ? ',"loadUnit":"…","throughputUnit":"…","responseUnit":"ms or s","load":"one plain sentence: what the load column counts","throughput":"one plain sentence: what the throughput column counts","response":"one plain sentence: what the response time column is, with its unit"' : '') +
            ',"method":"two or three plain sentences: how the search turns events into rows, for someone who does not know Splunk"}');
          if (!out || typeof out.search !== 'string') throw new Error('Claude did not return a search.');
          const str = v => typeof v === 'string' ? v.slice(0, 600) : '';
          useSearch({search: out.search.trim(), range: typeof out.earliest === 'string' ? out.earliest : '-7d',
            units: usl ? {n: str(out.loadUnit), x: str(out.throughputUnit), r: out.responseUnit === 's' ? 's' : 'ms'} : null,
            about: {title: str(out.title) || 'Search written by Claude', load: str(out.load), throughput: str(out.throughput), response: str(out.response),
              method: str(out.method) + ' Claude wrote this search from: “' + what.slice(0, 200) + '”.'}});
          $('#preset').value = ''; $('#findWords').hidden = true;
          await run(out.explanation);
        } catch (e) {
          bar.fail('Could not write the search');
          status('Could not write the search: ' + (e && e.message || e), 'warn');
        } finally { $('#btnAi').disabled = false; $('#btnRun').disabled = false; }
      });
    }

    (async () => {
      db = await ctx.cap('db').catch(() => null);
      dbKnown = true;
      draw();
      const splunk = await ctx.cap('splunk').catch(() => null);
      const ready = splunk && (await splunk.status().catch(() => ({ready: false}))).ready;
      if (!ready){ $('#btnRun').disabled = true; status('Splunk is not set up in this Wardian. An admin adds it in Settings → Splunk.', 'warn'); return; }
      setupAi();
      if (await resume(splunk)) return;
      if (table) status('This is the last table you fetched. Press Send to other apps to use it again, or run the search for fresh rows.');
    })();
  }
});
