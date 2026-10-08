/* search: the search form. Runs a Splunk search on the Wardian server as a background job, picks it
   up again when the app is opened later, and keeps named searches ("Save search").
   With the db capability (ADR-2610071219) the rows load into the package's own SQLite table, up to a
   million; the other parts read them from there. It also keeps the current table, so it comes back
   when the app opens: from a search, or a saved table opened by keep.
   Emits: table:ready (retained: {table, send}).  Listens: search:use (from ask), table:ready (to keep it).
   Capabilities: storage, splunk, db. */
Kernel.register({
  name: 'search',
  emits: {'table:ready': {retain: true}},
  listens: ['search:use', 'table:ready'],
  caps: ['storage', 'splunk', 'db'],
  init(ctx){
    const $ = ctx.$, bar = $('#bar');
    const TABLE_NAME = 'search';               // this suite's table of fresh results in its database
    const KEEP_BYTES = 900000;                 // the host keeps at most 1 MB per part: drop rows above this
    let db = null;                             // ctx.cap('db'), or null on a host without it
    let table = null;                          // the current table, as the other parts get it
    let pending = null;                        // the search running as a job: what it is for
    let handled = '';                          // the id of the last job whose table was shown
    const form = SearchForm($);
    function status(text, kind){ const st = $('#status'); st.className = 'status' + (kind ? ' ' + kind : ''); st.textContent = text; }
    function persist(){
      const state = Object.assign(form.state(), {table, pending, handled});
      // A big table may not fit in storage: then keep the search and drop the rows.
      if (JSON.stringify(state).length > KEEP_BYTES) state.table = null;
      ctx.store.set('state', state);
    }

    // ---------- the form ----------
    // Older versions may have left a damaged or partial state: every field is checked as it is read.
    const saved = ctx.store.get('state');
    const st = saved && typeof saved === 'object' ? saved : {};
    form.apply(st);
    if (st.table && Array.isArray(st.table.fields)) table = st.table;
    if (st.pending && typeof st.pending === 'object' && typeof st.pending.search === 'string') pending = st.pending;
    if (typeof st.handled === 'string') handled = st.handled;

    $('#spl').addEventListener('input', () => { form.forget(); persist(); });
    for (const id of ['#range', '#relN', '#relUnit', '#absFrom', '#absTo', '#advE', '#advL'])
      for (const ev of ['input', 'change']) $(id).addEventListener(ev, () => { form.showRange(); persist(); });
    $('#title').addEventListener('input', persist);
    for (const id of ['#words', '#wordsIndex']){
      $(id).addEventListener('input', () => { if ($('#preset').value === 'find'){ form.fillFind(); persist(); } });
      $(id).addEventListener('keydown', e => { if (e.key === 'Enter') run(); });
    }
    $('#preset').addEventListener('change', () => {
      const v = $('#preset').value;
      $('#findWords').hidden = v !== 'find';
      if (v === 'find'){
        form.fillFind(); persist();
        status('Type the words to find. Each word must appear in the event; put a phrase in "quotes". Then press Run search.');
        $('#words').focus();
        return;
      }
      const p = Presets[v];
      if (p){ form.use(p); status(p.note); }
      persist();
    });

    // ---------- the current table ----------
    // Every part draws from table:ready. A table keep opens comes back here, so it is kept too.
    const key = t => t ? [t.at, t.savedAt, t.dataset && t.dataset.table, t.title].join('|') : '';
    ctx.on('table:ready', m => {
      const t = m && m.table && Array.isArray(m.table.fields) ? m.table : null;
      if (key(t) !== key(table)){ table = t; persist(); }
    });
    if (table) ctx.emit('table:ready', {table, send: false});
    // Ask (Claude) wrote a search: put it in the form and run it.
    ctx.on('search:use', p => {
      if (!p || typeof p.search !== 'string') return;
      form.use(p);
      $('#preset').value = ''; $('#findWords').hidden = true;
      persist();
      run(typeof p.why === 'string' ? p.why : '');
    });

    // ---------- running the search ----------
    // The host runs each search as a background job (ADR-2610072118), so leaving this app does not
    // stop it. What the search is for is kept as `pending` until it ends; opening the app again
    // finds the job, shows its progress, and shows the table when it is done.
    let jobNow = null;                         // the id of the job this page is waiting on
    const kindOf = p => p.db ? 'splunk.into' : 'splunk.search';
    // The newest job of this part that matches `p`. Jobs started before the suite was split into
    // parts carry the old part name, "table", and are picked up too.
    async function findJob(splunk, p){
      const jobs = await splunk.jobs().catch(() => []);
      return (jobs || []).find(j => (j.app === 'search' || j.app === 'table') && j.kind === kindOf(p) && j.started >= p.at - 60000) || null;
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
            if (j.progress != null) bar.update({detail: Cells.rowsText(j.progress) + ' loaded so far'});
          }
        } finally { busy = false; }
      }, 2000);
      try { return await promise; } finally { clearInterval(tick); jobNow = null; }
    }
    bar.addEventListener('cancel', async () => {
      const splunk = await ctx.cap('splunk').catch(() => null);
      if (!splunk || !pending) return;
      const id = jobNow || ((await findJob(splunk, pending)) || {}).id;
      if (id) await splunk.cancel(id).catch(e => status('Could not stop the search: ' + Cells.errText(e), 'warn'));
    });

    // A finished search: `res` is what the host answered, `p` what the search was for.
    function finish(res, p, why){
      pending = null;
      handled = res.job || handled;
      table = {
        title: p.title, fields: res.fields || [], rows: p.db ? [] : (res.rows || []), truncated: !!res.truncated,
        search: p.search, range: p.range, latest: p.latest, rangeLabel: p.rangeLabel,
        at: new Date().toISOString(), seconds: res.seconds || 0,
        about: p.about || null, units: p.units || null, use: p.use || null,
      };
      if (p.db) table.dataset = {table: res.table, total: res.total || 0, columns: res.columns || []};
      persist();
      // send: a fresh table goes out to other apps at once (the keep part sends it).
      ctx.emit('table:ready', {table, send: true});
      const n = Cells.count(table), msgs = (res.messages || []).filter(Boolean);
      bar.done(Cells.rowsText(n) + ' in ' + bar.seconds + ' s');
      status([why, Cells.rowsText(n) + ' from Splunk' + (p.db ? ', kept in this app\'s database.' : '.'), msgs.length ? 'Splunk says: ' + msgs.join(' ') : ''].filter(Boolean).join(' '),
        table.truncated || !n ? 'warn' : '');
    }
    function failed(e){
      pending = null; persist();
      if (/^cancelled/.test(Cells.errText(e))){ bar.fail('Stopped'); status('You stopped the search.' + (table ? ' The table shown is the one from before.' : ''), 'warn'); return; }
      bar.fail('Search failed');
      status('Search failed: ' + Cells.errText(e), 'warn');
    }

    async function run(why){
      const splunk = await ctx.cap('splunk').catch(() => null);
      if (!splunk){ status('Splunk is not set up in this Wardian.', 'warn'); return; }
      if (!why && $('#preset').value === 'find' && !form.words().length){ status('Type the words to find first.', 'warn'); $('#words').focus(); return; }
      const search = $('#spl').value.trim();
      if (!search){ status('Type a search first, or pick one under Start from.', 'warn'); $('#spl').focus(); return; }
      const range = form.range();
      if (range.error){ status(range.error, 'warn'); form.showRange(); return; }
      $('#btnRun').disabled = true;
      // Splunk does not say how far a search has got, so the bar shows a clock rather than a share.
      bar.start('Searching Splunk', {detail: 'A search over many days can take a few minutes. You may leave this app meanwhile; Wardian stops it after 15.', cancelable: true});
      status('');
      const m = form.meta();
      const p = {search, range: range.earliest, latest: range.latest, rangeLabel: range.label, db: !!db, at: Date.now(),
        title: $('#title').value.trim() || (m.about && m.about.title) || 'Splunk search',
        about: m.about || null, units: m.units || null, use: m.use || null};
      pending = p;
      persist();
      try {
        const q = {search, earliest: range.earliest};
        if (range.latest) q.latest = range.latest;
        // With the database, the rows go into the suite's table, read from Splunk in chunks.
        finish(await follow(db ? db.searchInto(Object.assign({table: TABLE_NAME}, q)) : splunk.search(q), splunk, p), p, why);
      } catch (e) {
        failed(e);
      } finally { $('#btnRun').disabled = false; }
    }
    $('#btnRun').addEventListener('click', () => run());

    // Opening the app again: a search started earlier may still be running, or have finished
    // while the app was closed.
    async function resume(splunk){
      const p = pending;
      if (!p) return false;
      const j = await findJob(splunk, p);
      if (!j){
        pending = null; persist();
        status('The search you started earlier was lost: Wardian restarted while it ran, or the app was updated. Run it again.', 'warn');
        return true;
      }
      if (j.id === handled) return false;
      $('#btnRun').disabled = true;
      bar.start(j.state === 'running' ? 'Searching Splunk (started earlier)' : 'Reading the finished search',
        {detail: j.progress != null ? Cells.rowsText(j.progress) + ' loaded so far' : 'You may leave this app meanwhile.', cancelable: j.state === 'running'});
      try { finish(await follow(splunk.wait(j.id), splunk, p), p, ''); }
      catch (e) { failed(e); }
      finally { $('#btnRun').disabled = false; }
      return true;
    }

    // ---------- saved searches ----------
    let searches = Saved.list(ctx.store, 'savedSearches');
    function drawSaved(){
      $('#savedSearches').replaceChildren(...(searches.length ? searches.map(x => Saved.item(x.name,
        [x.rangeLabel, 'saved ' + Cells.when(x.at)].filter(Boolean).join(' · '),
        [['Use', () => loadSearch(x)], ['Delete', () => deleteSearch(x), 'ghost']]))
        : [Saved.empty('No saved searches. Write a search, then press Save search.')]));
      $('#savedCount').textContent = searches.length ? '(' + searches.length + ')' : '';
    }
    $('#btnSaveSearch').addEventListener('click', () => {
      if (!$('#spl').value.trim()){ status('Write a search first, then save it.', 'warn'); $('#spl').focus(); return; }
      const name = Saved.askName('Name for this search:', $('#title').value.trim() || $('#spl').value.trim().slice(0, 60), t => status(t, 'warn'));
      if (!name) return;
      const range = form.range();
      const same = searches.findIndex(x => x.name.toLowerCase() === name.toLowerCase());
      if (same >= 0 && !window.confirm('A search called “' + name + '” is already saved. Replace it?')) return;
      // "name" is the list name; the table's name stays in "title".
      const entry = Object.assign(form.state(), {id: same >= 0 ? searches[same].id : Saved.newId(), title: $('#title').value,
        rangeLabel: range.error ? '' : range.label, at: new Date().toISOString(), name});
      if (same >= 0) searches.splice(same, 1);
      searches.unshift(entry);
      ctx.store.set('savedSearches', searches);
      drawSaved(); $('#savedBox').open = true;
      status('Saved the search “' + name + '”. Find it under Saved searches.');
    });
    function loadSearch(x){
      form.apply(Object.assign({}, x, {name: typeof x.title === 'string' && x.title ? x.title : x.name}));
      persist();
      status('Loaded the search “' + x.name + '”. Press Run search for fresh rows.');
      $('#spl').scrollIntoView({block: 'nearest'});
    }
    function deleteSearch(x){
      if (!window.confirm('Delete the saved search “' + x.name + '”?')) return;
      searches = searches.filter(y => y.id !== x.id);
      ctx.store.set('savedSearches', searches);
      drawSaved(); status('Deleted the search “' + x.name + '”.');
    }
    drawSaved();

    (async () => {
      db = await ctx.cap('db').catch(() => null);
      const splunk = await ctx.cap('splunk').catch(() => null);
      const ready = splunk && (await splunk.status().catch(() => ({ready: false}))).ready;
      if (!ready){ $('#btnRun').disabled = true; status('Splunk is not set up in this Wardian. An admin adds it in Settings → Splunk. You can still open saved tables.', 'warn'); return; }
      if (await resume(splunk)) return;
      if (table) status('The table shown is the last one you fetched. Run the search for fresh rows.');
    })();
  }
});
