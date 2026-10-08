/* export: saves every session as CSV through the host's download capability, reading the log a
   thousand rows at a time. Listens: log:changed.  Capabilities: db, claude:downloads. */
Kernel.register({
  name: 'export',
  listens: ['log:changed'],
  caps: ['db', 'claude:downloads'],
  init(ctx){
    const $ = ctx.$, bar = $('#bar'), note = t => { $('#note').textContent = t; };
    const COLS = ['day', 'started', 'ended', 'label', 'minutes', 'completed', 'source'];
    let downloads = null, total = 0;
    const cell = v => { const s = v == null ? '' : String(v); return /[",\n]/.test(s) ? '"' + s.replace(/"/g, '""') + '"' : s; };

    async function count(){
      try {
        const d = await Log.db(ctx);
        total = d ? (await d.query({sql: 'SELECT COUNT(*) FROM sessions', params: []})).rows[0][0] : 0;
      } catch { total = 0; }
      $('#sub').textContent = total ? total.toLocaleString() + (total === 1 ? ' session' : ' sessions') : '';
      $('#csv').disabled = !total || !downloads;
      if (!downloads) note('Downloads are not available in this host.');
    }
    $('#csv').addEventListener('click', async () => {
      $('#csv').disabled = true;
      const lines = [COLS.join(',')];
      try {
        const d = await Log.db(ctx);
        if (bar.start) bar.start('Writing the CSV', {value: 0, max: total || 1});
        for (let at = 0; ; at += 1000){
          const p = await d.page({table: Log.TABLE, offset: at, limit: 1000, orderBy: 'started'});
          const idx = COLS.map(c => p.columns.indexOf(c));
          for (const r of p.rows) lines.push(idx.map(i => cell(r[i])).join(','));
          if (bar.update) bar.update({value: Math.min(at + 1000, p.total)});
          if (at + 1000 >= p.total) break;
        }
        const filename = 'focus-log-' + Log.day(new Date()) + '.csv';
        await downloads.save({filename, data: lines.join('\n') + '\n'});
        if (bar.done) bar.done((lines.length - 1).toLocaleString() + ' sessions saved');
        note('Saved ' + filename + '.');
      } catch (e){
        if (bar.fail) bar.fail('Could not save the file');
        note('Could not save the file: ' + Log.errText(e));
      } finally { $('#csv').disabled = !total || !downloads; }
    });
    ctx.cap('downloads').then(d => { downloads = d; count(); }).catch(() => count());
    ctx.on('log:changed', count);
  }
});
