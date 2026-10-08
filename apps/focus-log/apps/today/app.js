/* today: minutes focused today, sessions, the longest one, and progress toward a daily goal.
   Listens: log:changed.  Capabilities: db, storage (the goal). */
Kernel.register({
  name: 'today',
  listens: ['log:changed'],
  caps: ['db', 'storage'],
  init(ctx){
    const $ = ctx.$;
    let goal = Number(ctx.store.get('goal')) || 120, shownDay = '';
    $('#goal').value = goal;

    async function draw(){
      const today = Log.day(new Date());
      shownDay = today;
      $('#sub').textContent = Log.date(today);
      let row = [0, 0, 0, null, null];
      try {
        const d = await Log.db(ctx);
        if (!d) return;
        const r = await d.query({sql: 'SELECT COALESCE(SUM(minutes), 0), COUNT(*), COALESCE(SUM(completed), 0), MAX(minutes), MAX(ended) FROM sessions WHERE day = ?', params: [today]});
        row = r.rows[0] || row;
      } catch (e){ $('#sub').textContent = 'Could not read the log: ' + Log.errText(e); return; }
      const [mins, count, done, longest, lastEnd] = row;
      $('#mins').textContent = Log.minutes(mins);
      $('#count').textContent = count;
      $('#done').textContent = count ? done + ' completed' + (count - done ? ', ' + (count - done) + ' stopped early' : '') : 'None yet';
      $('#longest').textContent = longest ? Log.minutes(longest) : '–';
      $('#lastEnd').textContent = lastEnd ? 'Last ended ' + Log.time(lastEnd) : '';
      const share = Math.min(1, mins / goal);
      $('#fill').style.width = (share * 100).toFixed(1) + '%';
      $('#fill').classList.toggle('met', mins >= goal);
      $('#goalText').textContent = mins >= goal ? 'Goal met' : Log.minutes(goal - mins) + ' to your goal';
    }

    $('#goal').addEventListener('change', () => {
      const v = Math.round(Number($('#goal').value));
      if (v >= 5 && v <= 1440){ goal = v; ctx.store.set('goal', goal); }
      $('#goal').value = goal;
      draw();
    });
    ctx.on('log:changed', draw);
    // A new day starts at midnight, with the app still open.
    setInterval(() => { if (Log.day(new Date()) !== shownDay) draw(); }, 60000);
  }
});
