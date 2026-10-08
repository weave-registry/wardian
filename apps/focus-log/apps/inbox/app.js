/* inbox: receives finished focus sessions on the channel "focus.session" and keeps each one in the
   log's database. Shows whether the channel is open, and explains what to do when it is not.
   A session already in the log (same sender, same start) is not added again.
   Emits: log:changed (retained: tells the views to read the log again).
   Capabilities: db.  Channels: receives focus.session. */
Kernel.register({
  name: 'inbox',
  emits: {'log:changed': {retain: true}},
  caps: ['db'],
  channels: {receive: ['focus.session']},
  init(ctx){
    const $ = ctx.$;
    let added = 0, listening = false;
    const state = (text, variant) => { $('#state').textContent = text; $('#state').dataset.variant = variant; };
    const status = (text, warn) => { $('#status').textContent = text; $('#status').classList.toggle('warn', !!warn); };
    const changed = () => ctx.emit('log:changed', {at: Date.now()});

    async function receive(data, meta){
      const {row, problem} = Log.clean(data, meta && meta.from);
      if (problem){ status('A message from ' + ((meta && meta.from) || 'another app') + ' was not a session: ' + problem + '.', true); return; }
      try {
        const d = await Log.db(ctx);
        if (!d){ status('This Wardian has no database for apps, so sessions cannot be kept.', true); return; }
        const isNew = await Log.insert(d, row);
        $('#facts').hidden = false;
        $('#last').textContent = '“' + row[2] + '”, ' + Log.minutes(row[3]) + (row[6] ? '' : ' (stopped early)') + ', ended ' + Log.time(row[5]) + ' · from ' + row[7];
        if (isNew){ added++; changed(); }
        $('#count').textContent = added === 1 ? '1 new session' : added + ' new sessions';
        status(isNew ? '' : 'The latest session was already in the log.');
      } catch (e){ status('Could not keep the session: ' + Log.errText(e) + '.', true); }
    }

    function listen(){
      if (listening) return;
      state('Asking you', 'outline');
      $('#refused').hidden = true;
      status('Wardian is asking whether this log may read the channel focus.session. Answer above.');
      ctx.channel('focus.session').on(receive).then(() => {
        listening = true;
        state('Receiving', 'success');
        status('');
      }).catch(e => {
        state('Not allowed', 'destructive');
        $('#refused').hidden = false;
        status(/outside|not available|no channel/i.test(Log.errText(e)) ? 'Open this app from Wardian\'s app list to receive sessions.' : '');
      });
    }
    $('#retry').addEventListener('click', listen);

    // The views read the log as soon as it exists, whether or not the channel is open.
    Log.db(ctx).then(d => {
      if (!d) status('This Wardian has no database for apps, so sessions cannot be kept.', true);
      changed();
    }).catch(e => status('Could not open the log: ' + Log.errText(e) + '.', true));
    listen();
  }
});
