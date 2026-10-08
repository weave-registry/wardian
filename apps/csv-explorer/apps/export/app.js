/* export: downloads the rows the table shows, in its order, as a CSV file: every row the filter keeps,
   not only the page on screen. It reads them from the database 1,000 at a time.
   Listens: dataset:ready, view:changed.  Capabilities: db, claude:downloads. */
Kernel.register({
  name: 'export',
  listens: ['dataset:ready', 'view:changed'],
  caps: ['db', 'claude:downloads'],
  init(ctx){
    const $ = ctx.$, bar = $('#bar');
    const CHUNK = 1000;                        // the most rows one db.page call returns
    let db = null, downloads = null, asked = false, data = null, view = null, busy = false, stop = false;

    function status(text, warn){ const s = $('#status'); s.className = 'status' + (warn ? ' warn' : ''); s.textContent = text; }
    // Says in words what the file will hold.
    function describe(){
      $('#csv').disabled = busy || !data || !db || !downloads;
      if (!data){ $('#what').textContent = 'No data yet.'; return; }
      const v = Filter.view(view, data), parts = [];
      parts.push(v.find ? 'The rows that match “' + v.find + '”' : 'All ' + Data.rows(data.total));
      const ways = {number: ['smallest first', 'largest first'], date: ['oldest first', 'newest first'], text: ['A to Z', 'Z to A']};
      if (v.sort.col >= 0) parts.push('sorted by ' + data.fields[v.sort.col] + ' (' + ways[data.types[v.sort.col]][v.sort.desc ? 1 : 0] + ')');
      $('#what').textContent = parts.join(', ') + ', as comma-separated values. Change the filter or the sort in Rows to change what is saved.';
      if (asked && !downloads) status('This host cannot save files from apps.', true);
    }

    $('#csv').addEventListener('click', async () => {
      if (busy || !data) return;
      busy = true; stop = false; describe(); status('');
      const d = data, lines = [Csv.line(d.fields)], n = d.columns.length;
      try {
        const first = Filter.pageRequest(d, view, 0, 1);
        if (first.filter.problems.length) throw new Error(first.filter.problems.join(' '));
        const total = (await db.page(first.req)).total;
        bar.start('Writing ' + Data.fileStem(d.name) + '.csv', {value: 0, max: total || 1, cancelable: true});
        for (let at = 0; at < total; at += CHUNK){
          if (stop) throw new Error('stopped');
          const p = await db.page(Filter.pageRequest(d, view, at, CHUNK).req);
          for (const r of p.rows) lines.push(Csv.line(r.slice(0, n)));   // the last column, _all, stays behind
          const done = Math.min(at + CHUNK, total);
          bar.update({value: done, detail: Data.count(done) + ' of ' + Data.rows(total)});
        }
        const name = Data.fileStem(d.name) + (Filter.view(view, d).find ? '-filtered' : '') + '.csv';
        await downloads.save({filename: name, data: new Blob([lines.join('\n') + '\n'], {type: 'text/csv'})});
        bar.done(Data.rows(lines.length - 1) + ' saved');
        status('Saved ' + name + '.');
      } catch (e){
        if (stop){ bar.fail('Stopped'); status('You stopped the export. No file was saved.'); }
        else { bar.fail('Could not save the file'); status('Could not save the file: ' + Data.errText(e), true); }
      } finally { busy = false; describe(); }
    });
    bar.addEventListener('cancel', () => { stop = true; });

    ctx.on('dataset:ready', m => { data = Data.valid(m && m.dataset); describe(); });
    ctx.on('view:changed', v => { view = v; describe(); });
    ctx.cap('downloads').then(x => { downloads = x; }).catch(() => {}).then(() => { asked = true; describe(); });
    ctx.cap('db').then(x => { db = x; describe(); }).catch(() => describe());
  }
});
