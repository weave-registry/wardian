/* backup: saves every habit and check-in as JSON, and reads such a file back. A file is checked in
   full before anything changes, and the viewer confirms the replace. It owns no data: restoring asks
   habits.restore and log.restore, the two owners.
   Listens: habits:changed, log:changed.  Needs: habits.restore, log.restore.  Capabilities: claude:downloads. */
Kernel.register({
  name: 'backup',
  listens: ['habits:changed', 'log:changed'],
  needs: ['habits.restore', 'log.restore'],
  caps: ['claude:downloads'],
  init(ctx) {
    const $ = ctx.$, note = $('#note'), MAX_BYTES = 5 * 1024 * 1024;
    let habits = null, days = {}, downloads = null, pending = null;

    const say = (text, warn) => { note.textContent = text; note.classList.toggle('warn', !!warn); };
    function sync() {
      const n = habits ? habits.length : 0, c = habits ? Model.count(Model.backup(habits, days).days) : 0;
      $('#sub').textContent = n ? Stats.plural(n, 'habit') + ', ' + Stats.plural(c, 'check-in') : '';
      $('#save').disabled = !(downloads && n);
    }
    ctx.cap('downloads').then(d => { downloads = d; if (!d) say('Downloads are not available in this host.', true); sync(); }, () => say('Downloads are not available in this host.', true));

    $('#save').addEventListener('click', async () => {
      const data = Model.backup(habits, days);
      const name = 'habits-' + Days.today() + '.json';
      try {
        await downloads.save({ filename: name, data: JSON.stringify(data, null, 2) + '\n' });
        say('Saved ' + name + ': ' + Stats.plural(data.habits.length, 'habit') + ', ' + Stats.plural(Model.count(data.days), 'check-in') + '.');
      } catch (e) { say('Could not save the backup: ' + e.message, true); }
    });

    $('#open').addEventListener('click', () => $('#file').click());
    $('#file').addEventListener('change', async () => {
      const f = $('#file').files[0];
      $('#file').value = '';
      if (!f) return;
      $('#confirm').hidden = true; pending = null;
      if (f.size > MAX_BYTES) { say(f.name + ' is larger than 5 MB, so it cannot be a backup from this app.', true); return; }
      try {
        pending = Model.parseBackup(await f.text());
      } catch (e) { say('Could not use ' + f.name + ': ' + e.message + ' Nothing changed.', true); return; }
      const cur = habits ? habits.length : 0;
      $('#what').textContent = f.name + ' holds ' + Stats.plural(pending.habits.length, 'habit') + ' and ' +
        Stats.plural(Model.count(pending.days), 'check-in') + ': ' + pending.habits.map(h => h.name).join(', ') + '. ' +
        (cur ? 'It replaces your ' + Stats.plural(cur, 'habit') + ' and every check-in.' : 'You have no habits now, so nothing is lost.');
      say('');
      $('#confirm').hidden = false;
      $('#replace').focus();
    });

    $('#cancel').addEventListener('click', () => { pending = null; $('#confirm').hidden = true; say('Nothing changed.'); $('#open').focus(); });
    $('#replace').addEventListener('click', async () => {
      if (!pending) return;
      const { habits: h, days: d } = pending;
      pending = null; $('#confirm').hidden = true;
      try {
        // Check-ins first: a habit list that names habits whose check-ins are missing looks broken.
        await ctx.call('log', 'restore', { days: d });
        await ctx.call('habits', 'restore', { habits: h });
        say('Restored ' + Stats.plural(h.length, 'habit') + ' and ' + Stats.plural(Model.count(d), 'check-in') + '.');
      } catch (e) { say('Could not restore: ' + e.message, true); }
      $('#open').focus();
    });

    ctx.on('habits:changed', p => { habits = p.habits; sync(); });
    ctx.on('log:changed', p => { days = p.days; sync(); });
  }
});
