/* labels: time per label (what each session was for), most time first, over a chosen period.
   Listens: log:changed.  Capabilities: db, storage (the period). */
Kernel.register({
  name: 'labels',
  listens: ['log:changed'],
  caps: ['db', 'storage'],
  init(ctx){
    const $ = ctx.$, TOP = 12;
    $('#period').value = String(ctx.store.get('period') ?? 7);

    async function draw(){
      const days = Number($('#period').value), list = $('#list');
      let rows = [];
      try {
        const d = await Log.db(ctx);
        if (!d) return;
        const where = days ? ' WHERE day >= ?' : '';
        const r = await d.query({sql: 'SELECT label, SUM(minutes) AS m, COUNT(*) FROM sessions' + where + ' GROUP BY label ORDER BY m DESC, label', params: days ? [Log.daysAgo(days - 1)] : []});
        rows = r.rows;
      } catch (e){ list.replaceChildren(Log.empty(ctx, 'Could not read the log: ' + Log.errText(e))); return; }
      if (!rows.length){ list.replaceChildren(Log.empty(ctx, days ? 'No sessions in this period.' : 'No sessions yet. Finish one in the Focus timer.')); return; }
      // The rest beyond the top labels share one line, so the list stays short.
      if (rows.length > TOP){
        const rest = rows.slice(TOP - 1);
        rows = rows.slice(0, TOP - 1).concat([[rest.length + ' other labels', rest.reduce((s, r) => s + r[1], 0), rest.reduce((s, r) => s + r[2], 0), true]]);
      }
      const total = rows.reduce((s, r) => s + r[1], 0), most = rows[0][1];
      list.replaceChildren(...rows.map(([label, mins, count, other]) => {
        const li = ctx.el('li');
        const name = ctx.el('span'); name.className = 'name' + (other ? ' other' : ''); name.textContent = label;
        const meter = ctx.el('span'); meter.className = 'meter';
        const fill = ctx.el('span'); fill.style.width = (mins / most * 100).toFixed(1) + '%'; meter.appendChild(fill);
        const num = ctx.el('span'); num.className = 'num';
        num.textContent = Log.minutes(mins) + ' · ' + Math.round(mins / total * 100) + '%';
        num.title = count + (count === 1 ? ' session' : ' sessions');
        li.append(name, meter, num);
        return li;
      }));
    }
    $('#period').addEventListener('change', () => { ctx.store.set('period', Number($('#period').value)); draw(); });
    ctx.on('log:changed', draw);
  }
});
