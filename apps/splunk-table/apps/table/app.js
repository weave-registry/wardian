/* table: runs a Splunk search on the Wardian server and shows every row it returns.
   The time range can be a ready-made one, "last N minutes/hours/days…", two dates, or Splunk time codes.
   Sends the table on the channel "splunk.table", so other apps (the USL lab) can use it.
   Capabilities: storage, splunk, claude:sample, claude:downloads.  Channels: sends splunk.table. */
Kernel.register({
  name: 'table',
  caps: ['storage', 'splunk', 'claude:sample', 'claude:downloads'],
  channels: {send: ['splunk.table']},
  init(ctx){
    const $ = ctx.$;
    const SHOW_ROWS = 1000;                    // rows drawn on the page; the CSV and the channel carry more
    const CHANNEL_BYTES = 240 * 1024;          // a channel message may be at most 256 KB

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

    // What the search in the box means, for the apps that use the table. Cleared when you edit the search.
    let meta = null;                           // {about, units, use}
    let table = null;                          // the latest result, as sent on the channel
    let sort = {col: -1, dir: 1};

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
    if (saved.meta && typeof saved.meta === 'object') meta = saved.meta;
    if (saved.table && Array.isArray(saved.table.fields)) table = saved.table;
    function persist(){
      const state = {s: $('#spl').value, t: $('#range').value, name: $('#title').value, meta, table,
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
    showRange();

    function useSearch(p){
      $('#spl').value = p.search;
      if (p.range !== undefined) setEarliest(p.range);
      if (p.about && p.about.title) $('#title').value = p.about.title;
      meta = {about: p.about || null, units: p.units || null, use: p.use || null};
      persist();
    }
    $('#preset').addEventListener('change', () => {
      const p = PRESETS[$('#preset').value]; if (!p) return;
      useSearch(p); status(p.note);
    });

    // ---------- the result ----------
    function draw(){
      const out = $('#out'), facts = $('#facts');
      $('#btnSend').disabled = !table; $('#btnCsv').disabled = !table || !downloads;
      if (!table){ facts.hidden = true; return; }
      facts.replaceChildren();
      const fact = (k, v, code) => {
        if (!v) return;
        const dt = ctx.el('dt'), dd = ctx.el('dd'); dt.textContent = k;
        if (code){ const c = ctx.el('code'); c.textContent = v; dd.appendChild(c); } else dd.textContent = v;
        facts.append(dt, dd);
      };
      fact('Table', table.title);
      fact('Rows', table.rows.length + (table.rows.length === 1 ? ' row' : ' rows') + ' × ' + table.fields.length + ' columns' + (table.truncated ? ' (Splunk returned more; only the first ' + table.rows.length + ' are kept)' : ''));
      fact('Time range', table.rangeLabel);
      fact('Fetched', new Date(table.at).toLocaleString() + (table.seconds ? ', the search took ' + table.seconds + ' s' : ''));
      fact('Search', table.search, true);
      facts.hidden = false;

      if (!table.rows.length){ out.replaceChildren(Object.assign(ctx.el('p'), {className: 'empty', textContent: 'The search returned no rows. Try a longer time range.'})); return; }
      const numeric = table.fields.map((_, i) => table.rows.every(r => r[i] == null || r[i] === '' || isNum(r[i])));
      let rows = table.rows;
      if (sort.col >= 0){
        const i = sort.col;
        rows = rows.slice().sort((a, b) => sort.dir * (numeric[i] ? num(a[i]) - num(b[i]) : String(a[i] ?? '').localeCompare(String(b[i] ?? ''))));
      }
      const t = ctx.el('table'), thead = ctx.el('thead'), hr = ctx.el('tr');
      t.className = 'w-table';
      table.fields.forEach((f, i) => {
        const th = ctx.el('th'); th.textContent = f; th.title = 'Sort by ' + f; th.scope = 'col';
        if (numeric[i]) th.className = 'num';
        if (sort.col === i){ const a = ctx.el('span'); a.className = 'arrow'; a.textContent = sort.dir > 0 ? '▲' : '▼'; th.appendChild(a); }
        th.addEventListener('click', () => { sort = {col: i, dir: sort.col === i ? -sort.dir : 1}; draw(); });
        hr.appendChild(th);
      });
      thead.appendChild(hr); t.appendChild(thead);
      const tb = ctx.el('tbody');
      for (const r of rows.slice(0, SHOW_ROWS)){
        const tr = ctx.el('tr');
        r.forEach((v, i) => { const td = ctx.el('td'); td.textContent = v == null ? '' : String(v); if (numeric[i]) td.className = 'num'; tr.appendChild(td); });
        tb.appendChild(tr);
      }
      t.appendChild(tb);
      const wrap = ctx.el('div'); wrap.className = 'w-table-wrap'; wrap.appendChild(t);
      const parts = [wrap];
      if (rows.length > SHOW_ROWS) parts.push(Object.assign(ctx.el('p'), {className: 'note', textContent: 'Showing the first ' + SHOW_ROWS + ' of ' + rows.length + ' rows. Download the CSV to see all of them.'}));
      out.replaceChildren(...parts);
    }

    // ---------- sending to other apps ----------
    // The message carries the rows and everything needed to judge them: the search, the time range,
    // when it ran, and (for ready-made searches) what each column means.
    async function share(){
      if (!table) return;
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
    $('#btnSend').addEventListener('click', async () => { status(await share()); });

    // ---------- CSV ----------
    let downloads = null;
    ctx.cap('downloads').then(d => { downloads = d; draw(); }).catch(() => {});
    const csvCell = v => { const s = v == null ? '' : String(v); return /[",\n\r]/.test(s) ? '"' + s.replace(/"/g, '""') + '"' : s; };
    $('#btnCsv').addEventListener('click', async () => {
      if (!table || !downloads) return;
      const lines = [table.fields.map(csvCell).join(',')].concat(table.rows.map(r => r.map(csvCell).join(',')));
      const name = (table.title || 'splunk-table').replace(/[^\w.\- ]+/g, ' ').trim().slice(0, 80) || 'splunk-table';
      try { await downloads.save({filename: name + '.csv', data: lines.join('\n') + '\n'}); }
      catch (e) { status('Could not save the file: ' + (e && e.message || e), 'warn'); }
    });

    // ---------- running the search ----------
    async function run(why){
      const splunk = await ctx.cap('splunk');
      const search = $('#spl').value.trim();
      if (!search){ status('Type a search first, or pick one under Start from.', 'warn'); $('#spl').focus(); return; }
      const range = readRange();
      if (range.error){ status(range.error, 'warn'); showRange(); return; }
      persist();
      $('#btnRun').disabled = true;
      // Splunk does not say how far a search has got, so the bar shows a clock rather than a share.
      if (!why) bar.start('Searching Splunk', {detail: 'A search over many days can take a few minutes. Wardian stops it after 15.'});
      else bar.update({label: 'Running the search', detail: ''});
      status('');
      try {
        const q = {search, earliest: range.earliest};
        if (range.latest) q.latest = range.latest;
        const res = await splunk.search(q);
        const m = meta || {};
        table = {
          title: $('#title').value.trim() || (m.about && m.about.title) || 'Splunk search',
          fields: res.fields || [], rows: res.rows || [], truncated: !!res.truncated,
          search, range: range.earliest, latest: range.latest, rangeLabel: range.label,
          at: new Date().toISOString(), seconds: res.seconds || 0,
          about: m.about || null, units: m.units || null, use: m.use || null,
        };
        sort = {col: -1, dir: 1};
        persist(); draw();
        bar.done(table.rows.length + (table.rows.length === 1 ? ' row' : ' rows') + ' in ' + bar.seconds + ' s');
        const msgs = (res.messages || []).filter(Boolean);
        const sent = table.rows.length ? await share() : '';
        status([why, table.rows.length + (table.rows.length === 1 ? ' row' : ' rows') + ' from Splunk.', sent, msgs.length ? 'Splunk says: ' + msgs.join(' ') : ''].filter(Boolean).join(' '),
          table.truncated || !table.rows.length ? 'warn' : '');
      } catch (e) {
        bar.fail('Search failed');
        status('Search failed: ' + (e && e.message || e), 'warn');
      } finally { $('#btnRun').disabled = false; }
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
          $('#preset').value = '';
          await run(out.explanation);
        } catch (e) {
          bar.fail('Could not write the search');
          status('Could not write the search: ' + (e && e.message || e), 'warn');
        } finally { $('#btnAi').disabled = false; $('#btnRun').disabled = false; }
      });
    }

    draw();
    (async () => {
      const splunk = await ctx.cap('splunk').catch(() => null);
      const ready = splunk && (await splunk.status().catch(() => ({ready: false}))).ready;
      if (!ready){ $('#btnRun').disabled = true; status('Splunk is not set up in this Wardian. An admin adds it in Settings → Splunk.', 'warn'); return; }
      if (table) status('This is the last table you fetched. Press Send to other apps to use it again, or run the search for fresh rows.');
      setupAi();
    })();
  }
});
