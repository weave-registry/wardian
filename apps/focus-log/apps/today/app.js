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

    // The goal is checked when the user leaves the field; a wrong value is said, not replaced, and
    // the last valid goal keeps working until it is fixed.
    const HINT = $('#goal-hint').textContent;
    function mark(error){
      const input = $('#goal'), hint = $('#goal-hint');
      if (error){ input.setAttribute('aria-invalid', 'true'); hint.textContent = error + ' Until then, the goal stays ' + Log.minutes(goal) + '.'; hint.classList.add('error'); }
      else { input.removeAttribute('aria-invalid'); hint.textContent = HINT; hint.classList.remove('error'); }
    }
    $('#goal').addEventListener('change', () => {
      const r = Log.parseGoal($('#goal').value);
      if ('error' in r){ mark(r.error); return; }
      mark(''); goal = r.value; ctx.store.set('goal', goal); draw();
    });
    $('#goal').addEventListener('input', () => { if ($('#goal').getAttribute('aria-invalid') && !('error' in Log.parseGoal($('#goal').value))) mark(''); });
    $('#goal').addEventListener('keydown', e => { if (e.key === 'Enter') $('#goal').dispatchEvent(new Event('change')); });
    ctx.on('log:changed', draw);
    // A new day starts at midnight, with the app still open.
    setInterval(() => { if (Log.day(new Date()) !== shownDay) draw(); }, 60000);
  }
});
