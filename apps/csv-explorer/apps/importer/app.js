/* importer: opens a CSV file (chosen, dropped, or the sample), reads it, and loads it into this
   app's own database, a batch at a time. The dataset survives a reload: on start the importer reads
   which table is current from the table "csv_meta" and announces it again.
   Emits: dataset:ready (retained: {dataset}, or {dataset: null} when nothing is loaded).
   Capabilities: db, asset (the sample file). */
Kernel.register({
  name: 'importer',
  emits: {'dataset:ready': {retain: true}},
  caps: ['db', 'asset'],
  init(ctx){
    const $ = ctx.$, bar = $('#bar');
    const MAX_FILE = 150 * 1024 * 1024;        // larger files take minutes to read in a browser
    const BATCH_BYTES = 1500000;               // one insert call must stay under the host's 2 MB body limit
    let db = null, current = null, busy = false, stop = false;

    function status(text, warn){ const s = $('#status'); s.className = 'status' + (warn ? ' warn' : ''); s.textContent = text; }
    function setBusy(on){ busy = on; for (const id of ['#choose', '#sample']) $(id).disabled = on || !db; draw(); }
    const sql = (text, params = []) => db.query({sql: text, params});
    const announce = d => { current = d; ctx.emit('dataset:ready', {dataset: d}); draw(); };

    // ---------- what is loaded now ----------
    function draw(){
      const box = $('#current');
      if (!current){ box.replaceChildren(); return; }
      const d = current, el = (tag, cls, text) => { const e = ctx.el(tag); if (cls) e.className = cls; if (text != null) e.textContent = text; return e; };
      const head = el('div', 'current-head');
      head.append(el('b', 'current-name', d.name), el('span', 'current-meta', Data.rows(d.total) + ' · ' + d.columns.length + ' columns · loaded ' + Data.when(d.loadedAt)));
      const list = el('ul', 'columns');
      d.fields.forEach((f, i) => {
        const li = el('li'), badge = el('span', 'w-badge', d.types[i]);
        badge.dataset.variant = d.types[i] === 'number' ? 'secondary' : 'outline';
        li.append(el('span', 'col-name', f), badge);
        list.appendChild(li);
      });
      const remove = el('button', 'w-button', 'Remove data');
      remove.type = 'button'; remove.dataset.variant = 'ghost'; remove.dataset.size = 'sm'; remove.disabled = busy;
      remove.addEventListener('click', removeData);
      box.replaceChildren(head, list, remove);
    }

    async function removeData(){
      if (!current || busy || !window.confirm('Remove “' + current.name + '” from this app? The file on your computer stays.')) return;
      try {
        await sql('DELETE FROM csv_meta WHERE k = ?1', ['current']);
        await sql('DROP TABLE IF EXISTS ' + Data.q(current.table));
        status('Removed the data. Open a file to start again.');
        announce(null);
      } catch (e){ status('Could not remove the data: ' + Data.errText(e), true); }
    }

    // ---------- loading a file ----------
    // Reads the text, creates a new table typed from the file, and inserts the rows in batches.
    // The table shown before stays until the new one is complete, so a failed load loses nothing.
    async function load(text, name){
      if (busy || !db) return;
      setBusy(true); stop = false; status('');
      bar.start('Reading ' + name, {cancelable: true});
      await new Promise(r => setTimeout(r, 30));          // let the bar draw before the parse
      const t0 = performance.now();
      const csv = Csv.read(text);
      if (!csv.fields.length || !csv.rows.length){
        bar.fail('Nothing to load');
        status(csv.fields.length ? 'The file has a header line but no rows.' : 'The file is empty.', true);
        return setBusy(false);
      }
      const table = 'ds_' + Date.now().toString(36);
      const columns = csv.fields.map((_, i) => 'c' + i);
      const decl = columns.map((c, i) => Data.q(c) + (csv.types[i] === 'number' ? ' REAL' : ' TEXT'));
      const withAll = r => r.concat(r.map(v => v == null ? '' : String(v)).join(' ').toLowerCase());
      const total = csv.rows.length;
      try {
        await sql('CREATE TABLE ' + Data.q(table) + ' (' + decl.join(', ') + ', "_all" TEXT)');
        const sample = csv.rows.slice(0, 200).map(withAll);
        const per = Math.max(50, Math.min(20000, Math.floor(BATCH_BYTES / (JSON.stringify(sample).length / sample.length))));
        bar.update({label: 'Loading ' + name, value: 0, max: total});
        for (let at = 0; at < total; at += per){
          if (stop) throw new Error('stopped');
          await db.insertRows({table, columns: columns.concat('_all'), rows: csv.rows.slice(at, at + per).map(withAll)});
          const done = Math.min(at + per, total);
          bar.update({value: done, detail: Data.count(done) + ' of ' + Data.rows(total)});
        }
        const d = {table, name, fields: csv.fields, columns, types: csv.types, total, loadedAt: new Date().toISOString(), delimiter: csv.delimiter};
        await sql('INSERT OR REPLACE INTO csv_meta (k, v) VALUES (?1, ?2)', ['current', JSON.stringify(d)]);
        const old = current;
        announce(d);
        if (old && old.table !== table) await sql('DROP TABLE IF EXISTS ' + Data.q(old.table)).catch(() => {});
        bar.done(Data.rows(total) + ' in ' + Math.max(0.1, (performance.now() - t0) / 1000).toFixed(1) + ' s');
        const notes = [];
        if (csv.ragged) notes.push(Data.rows(csv.ragged) + ' had a different number of values from the header; missing values are empty and extra ones were left out.');
        if (csv.unclosed) notes.push('The file ends inside a quoted value, so the last row may be cut short.');
        status('Loaded ' + Data.rows(total) + ', separated by ' + Csv.DELIMITER_NAMES[csv.delimiter] + '. ' + notes.join(' '), notes.length > 0);
      } catch (e){
        await sql('DROP TABLE IF EXISTS ' + Data.q(table)).catch(() => {});
        if (stop){ bar.fail('Stopped'); status('You stopped the load.' + (current ? ' The data shown is the one from before.' : ''), true); }
        else { bar.fail('Could not load the file'); status('Could not load the file: ' + Data.errText(e), true); }
      } finally { setBusy(false); }
    }
    bar.addEventListener('cancel', () => { stop = true; });

    async function openFile(file){
      if (!file) return;
      if (file.size > MAX_FILE){ status('That file is ' + Math.round(file.size / 1048576) + ' MB. This app reads files up to ' + (MAX_FILE / 1048576) + ' MB.', true); return; }
      try { await load(await file.text(), file.name); }
      catch (e){ status('Could not read the file: ' + Data.errText(e), true); }
    }
    $('#choose').addEventListener('click', () => $('#file').click());
    $('#file').addEventListener('change', e => { openFile(e.target.files[0]); e.target.value = ''; });
    $('#sample').addEventListener('click', async () => {
      try { await load(new TextDecoder().decode(await ctx.asset('sample/cities.csv')), 'cities.csv'); }
      catch (e){ status('Could not read the sample: ' + Data.errText(e), true); }
    });
    const drop = $('#drop');
    drop.addEventListener('dragover', e => { e.preventDefault(); drop.classList.add('over'); });
    drop.addEventListener('dragleave', () => drop.classList.remove('over'));
    drop.addEventListener('drop', e => { e.preventDefault(); drop.classList.remove('over'); openFile(e.dataTransfer.files[0]); });

    // ---------- on start: what is already loaded ----------
    (async () => {
      setBusy(true);
      db = await ctx.cap('db').catch(() => null);
      if (!db){ status('This host has no database for apps, so there is nowhere to load a file.', true); setBusy(false); announce(null); return; }
      try {
        await sql('CREATE TABLE IF NOT EXISTS csv_meta (k TEXT PRIMARY KEY, v TEXT)');
        const r = await sql('SELECT v FROM csv_meta WHERE k = ?1', ['current']);
        let d = r.rows.length ? Data.valid(JSON.parse(r.rows[0][0])) : null;
        if (d){
          const n = await sql('SELECT count(*) FROM ' + Data.q(d.table)).catch(() => null);
          d = n ? Object.assign(d, {total: n.rows[0][0]}) : null;
        }
        // Tables left by a load that was interrupted (the page closed half-way) are dropped.
        for (const t of await db.tables()) if (Data.TABLE.test(t.name) && (!d || t.name !== d.table)) await sql('DROP TABLE IF EXISTS ' + Data.q(t.name));
        announce(d);
        if (!d) status('No data yet. Open a CSV file, or press Load sample.');
      } catch (e){ status('Could not read the database: ' + Data.errText(e), true); announce(null); }
      finally { setBusy(false); }
    })();
  }
});
