/* keep: what to do with the current table. Send it to other apps on the channel "splunk.table" (the
   USL lab reads it), download it as CSV, or save a named copy ("Save table") to open again later.
   A fresh search result is sent at once. A saved table is a copy: in the database a table named
   "saved_…", else its rows in this part's storage.
   Listens: table:ready, table:view.  Emits: table:ready (when a saved table is opened or deleted).
   Under the buttons, a receipt shows the last table sent: its name, id, rows and columns, the same as
   the receiving app shows it (ui/receipt.js).
   Capabilities: storage, db, claude:downloads.  Channels: sends splunk.table. */
Kernel.register({
  name: 'keep',
  emits: {'table:ready': {retain: true}},
  listens: ['table:ready', 'table:view'],
  caps: ['storage', 'db', 'claude:downloads'],
  channels: {send: ['splunk.table']},
  init(ctx){
    const $ = ctx.$, bar = $('#bar');
    const CHANNEL_BYTES = 240 * 1024;          // a channel message may be at most 256 KB
    const INLINE_ROWS = 1000;                  // a table in the database also travels inline up to this many rows
    const SAVED_ROWS_BYTES = 700000;           // the host keeps at most 1 MB per part, saved tables included
    const SAVED_NAME = /^saved_[a-z0-9]{1,40}$/;
    let db = null, downloads = null, table = null, view = Find.view(null);
    function status(text, kind){ const st = $('#status'); st.className = 'status' + (kind ? ' ' + kind : ''); st.textContent = text; }
    function buttons(){ $('#btnSend').disabled = !table; $('#btnKeep').disabled = !table; $('#btnCsv').disabled = !table || !downloads; }

    // ---------- sending to other apps ----------
    // The message carries the rows and everything needed to judge them: the search, the time range,
    // when it ran, and (for ready-made searches) what each column means. Its shape must not change:
    // the USL lab reads it.
    function fit(msg, rows){
      if (JSON.stringify(msg).length <= CHANNEL_BYTES) return msg;
      let keep = rows.length;
      while (keep > 0 && JSON.stringify(Object.assign({}, msg, {rows: rows.slice(0, keep)})).length > CHANNEL_BYTES) keep = Math.floor(keep * 0.8);
      return Object.assign({}, msg, {rows: rows.slice(0, keep), cut: true});
    }
    async function share(){
      if (!table) return '';
      let msg;
      if (table.dataset){
        // A table in the database travels as a reference: the receiving app reads the pages it needs,
        // after the user allows it. Up to INLINE_ROWS rows also travel inline, for apps that read only rows.
        const d = table.dataset;
        let rows = [];
        try {
          for (let at = 0; at < Math.min(d.total, INLINE_ROWS); at += 1000){
            const p = await db.page({table: d.table, offset: at, limit: Math.min(1000, INLINE_ROWS - at)});
            rows = rows.concat(p.rows);
          }
        } catch { rows = []; }
        msg = fit(Object.assign({}, table, {rows, cut: rows.length < d.total, dataset: {package: 'splunk-table', table: d.table, total: d.total, columns: d.columns, fields: table.fields}}), rows);
      } else msg = fit(Object.assign({}, table, {cut: false}), table.rows);
      try {
        const name = (table.title || 'Splunk table') + (table.rangeLabel ? ', ' + table.rangeLabel : '');
        const sent = await ctx.channel('splunk.table').send(msg, {name: name.slice(0, 120)});
        showSent({...sent, channel: 'splunk.table', bytes: JSON.stringify(msg).length, what: WardianUI.describe(msg)});
        if (table.dataset) return msg.rows.length >= table.dataset.total ? 'Sent to other apps.'
          : 'Sent to other apps: all ' + table.dataset.total.toLocaleString() + ' rows, which they read from this app\'s database after you allow it.';
        return 'Sent to other apps' + (msg.cut ? ', but only the first ' + msg.rows.length + ' rows fit in one message' : '') + '.';
      } catch (e) {
        return 'Not sent to other apps: ' + Cells.errText(e) + '. Settings → App permissions can change that.';
      }
    }
    $('#btnSend').addEventListener('click', async () => { status(await share()); });
    // The last receipt is kept, so it is still there when the app opens again.
    function showSent(r){
      ctx.store.set('lastSent', r);
      $('#sentReceipt').replaceChildren(WardianUI.receipt(r, {direction: 'sent'}));
    }
    const lastSent = ctx.store.get('lastSent');
    if (lastSent && lastSent.id) $('#sentReceipt').replaceChildren(WardianUI.receipt(lastSent, {direction: 'sent'}));

    // ---------- CSV ----------
    // Saves the rows that "Find in results" keeps (all of them when it is empty), in the order shown.
    $('#btnCsv').addEventListener('click', async () => {
      if (!table || !downloads) return;
      if (table.dataset) return csvFromDatabase();
      const rows = view.find ? Find.rows(table, view).rows : table.rows;
      const lines = [Cells.csvLine(table.fields)].concat(rows.map(Cells.csvLine));
      try { await downloads.save({filename: Cells.fileName(table.title) + '.csv', data: lines.join('\n') + '\n'}); }
      catch (e) { status('Could not save the file: ' + Cells.errText(e), 'warn'); }
    });
    // Reads every matching row from the database, a thousand at a time.
    async function csvFromDatabase(){
      $('#btnCsv').disabled = true;
      const lines = [Cells.csvLine(table.fields)];
      try {
        const total = (await db.page(Find.pageRequest(table, view, 0, 1).req)).total;
        bar.start('Writing the CSV', {value: 0, max: total || 1});
        for (let at = 0; at < total; at += 1000){
          const p = await db.page(Find.pageRequest(table, view, at, 1000).req);
          for (const r of p.rows) lines.push(Cells.csvLine(r));
          const done = Math.min(at + 1000, total);
          bar.update({value: done, detail: done.toLocaleString() + ' of ' + total.toLocaleString() + ' rows'});
        }
        await downloads.save({filename: Cells.fileName(table.title) + '.csv', data: lines.join('\n') + '\n'});
        bar.done(Cells.rowsText(lines.length - 1) + ' saved');
      } catch (e) {
        bar.fail('Could not save the file');
        status('Could not save the file: ' + Cells.errText(e), 'warn');
      } finally { buttons(); }
    }

    // ---------- saved tables ----------
    let tables = Saved.list(ctx.store, 'savedTables');
    function drawSaved(){
      $('#savedTables').replaceChildren(...(tables.length ? tables.map(x => Saved.item(x.name,
        Cells.rowsText(x.dataset ? Number(x.dataset.total) || 0 : Number(x.rowCount) || 0) + ' · saved ' + Cells.when(x.savedAt),
        [['Open', () => openTable(x)], ['Delete', () => deleteTable(x), 'ghost']]))
        : [Saved.empty('No saved tables. Run a search, then press Save table.')]));
      $('#savedCount').textContent = tables.length ? '(' + tables.length + ')' : '';
    }
    $('#btnKeep').addEventListener('click', async () => {
      if (!table) return;
      const name = Saved.askName('Name for this table:', table.title, t => status(t, 'warn'));
      if (!name) return;
      const id = Saved.newId();
      const entry = Object.assign({}, table, {id, name, title: name, savedAt: new Date().toISOString(), rows: []});
      $('#btnKeep').disabled = true;
      try {
        if (table.dataset){
          if (!db) throw new Error('the database is not available here');
          const tname = 'saved_' + id.replace(/[^a-z0-9]/g, '');
          const from = String(table.dataset.table).replace(/"/g, '""');
          await db.query({sql: 'CREATE TABLE "' + tname + '" AS SELECT * FROM "' + from + '"', params: []});
          entry.dataset = Object.assign({}, table.dataset, {table: tname});
        } else {
          const size = JSON.stringify(table.rows).length;
          if (size > SAVED_ROWS_BYTES) throw new Error('it is too big to keep in this app (' + (size / 1e6).toFixed(1) + ' MB). Download the CSV instead');
          ctx.store.set('savedRows:' + id, table.rows);
          entry.rowCount = table.rows.length;
        }
        tables.unshift(entry);
        ctx.store.set('savedTables', tables);
        drawSaved(); $('#savedBox').open = true;
        status('Saved the table “' + name + '”. Open it any time under Saved tables.');
      } catch (e){
        status('Could not save the table: ' + Cells.errText(e) + '.', 'warn');
      } finally { buttons(); }
    });
    function openTable(x){
      const t = Object.assign({}, x);
      delete t.id; delete t.name; delete t.rowCount;
      if (!x.dataset){
        const rows = ctx.store.get('savedRows:' + x.id);
        if (!Array.isArray(rows)){ status('The rows of “' + x.name + '” are missing. Delete it and save it again.', 'warn'); return; }
        t.rows = rows;
      } else if (!db){ status('“' + x.name + '” is kept in the database, which this host does not offer.', 'warn'); return; }
      ctx.emit('table:ready', {table: t, send: false});
      status('Opened the saved table “' + x.name + '”. Press Send to other apps to use it there.');
    }
    async function deleteTable(x){
      if (!window.confirm('Delete the saved table “' + x.name + '”? Its rows are removed.')) return;
      try {
        if (x.dataset){
          if (!db) throw new Error('the database is not available here');
          if (SAVED_NAME.test(x.dataset.table)) await db.query({sql: 'DROP TABLE IF EXISTS "' + x.dataset.table + '"', params: []});
        } else ctx.store.set('savedRows:' + x.id, null);
      } catch (e){ status('Could not delete the table: ' + Cells.errText(e) + '.', 'warn'); return; }
      tables = tables.filter(y => y.id !== x.id);
      ctx.store.set('savedTables', tables);
      // The table on screen was this saved copy: clear it, since its rows are gone.
      if (table && table.dataset && x.dataset && table.dataset.table === x.dataset.table) ctx.emit('table:ready', {table: null, send: false});
      drawSaved(); status('Deleted the table “' + x.name + '”.');
    }
    drawSaved();

    // ---------- the current table ----------
    ctx.on('table:view', v => { view = Find.view(v); });
    ctx.on('table:ready', async m => {
      table = m && m.table && Array.isArray(m.table.fields) ? m.table : null;
      buttons();
      if (!m || !m.send || !Cells.count(table)) return;
      if (table.dataset && !db) db = await ctx.cap('db').catch(() => null);
      status(await share());
    });
    ctx.cap('downloads').then(d => { downloads = d; buttons(); }).catch(() => {});
    ctx.cap('db').then(d => { db = d; }).catch(() => {});
  }
});
