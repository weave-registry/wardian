/* streaks: for each habit, the current streak, the longest, and how often you kept it in the last
   30 and 90 days. Read-only: it draws from the retained topics and writes nothing.
   Listens: habits:changed, log:changed. */
Kernel.register({
  name: 'streaks',
  listens: ['habits:changed', 'log:changed'],
  init(ctx) {
    const $ = ctx.$;
    let habits = null, days = {}, today = Days.today(), rows = [];

    function tile(label, value, detail) {
      const t = ctx.el('div');
      t.className = 'tile';
      t.innerHTML = '<div class="label"></div><div class="value"></div><div class="detail"></div>';
      t.children[0].textContent = label; t.children[1].textContent = value; t.children[2].textContent = detail || '';
      return t;
    }
    function bar(r, color) {
      const cell = ctx.el('td'), wrap = ctx.el('div'), track = ctx.el('div'), fill = ctx.el('div'), text = ctx.el('span');
      wrap.className = 'meter';
      track.className = 'track'; fill.className = 'fill';
      fill.style.width = r.pct === null ? '0' : Math.round(r.pct * 100) + '%';
      fill.style.background = color;
      text.textContent = Stats.pct(r);
      text.title = r.days ? r.done + ' of ' + Stats.plural(r.days, 'day') : 'No days yet';
      track.append(fill); wrap.append(track, text); cell.append(wrap);
      return cell;
    }

    function render() {
      if (!habits) return;
      rows = habits.map(h => ({ h, s: Stats.of(days[h.id] || [], today, h.created) }));
      $('#empty').hidden = habits.length > 0;
      $('#wrap').hidden = habits.length === 0;
      $('#sub').textContent = habits.length ? 'as of ' + Days.label(today, { weekday: 'short', day: 'numeric', month: 'short' }) : '';

      const tiles = $('#tiles');
      tiles.replaceChildren();
      if (rows.length) {
        const best = rows.reduce((a, b) => b.s.current > a.s.current ? b : a);
        const record = rows.reduce((a, b) => b.s.longest > a.s.longest ? b : a);
        const all30 = rows.reduce((n, r) => n + r.s.d30.done, 0), of30 = rows.reduce((n, r) => n + r.s.d30.days, 0);
        tiles.append(
          tile('Best current streak', Stats.plural(best.s.current, 'day'), best.s.current ? best.h.name : 'Tick a habit to start one'),
          tile('Longest ever', Stats.plural(record.s.longest, 'day'), record.s.longest ? record.h.name : ''),
          tile('Kept, last 30 days', of30 ? Math.round(all30 / of30 * 100) + '%' : '–', of30 ? all30 + ' of ' + of30 + ' habit-days' : ''));
      }

      const body = $('#rows');
      body.replaceChildren();
      for (const { h, s } of rows) {
        const tr = ctx.el('tr'), name = ctx.el('th');
        name.scope = 'row';
        name.innerHTML = '<span class="dot"></span><span></span>';
        name.firstChild.style.background = h.color;
        name.lastChild.textContent = h.name;
        const num = (v, title) => { const td = ctx.el('td'); td.className = 'num'; td.textContent = v; if (title) td.title = title; return td; };
        const cur = num(Stats.plural(s.current, 'day'), s.doneToday ? 'Includes today' : s.current ? 'Tick today to keep it going' : '');
        if (s.current && !s.doneToday) cur.classList.add('open');
        tr.append(name, cur, num(Stats.plural(s.longest, 'day')), bar(s.d30, h.color), bar(s.d90, h.color), num(s.total));
        body.append(tr);
      }
    }

    ctx.on('habits:changed', p => { habits = p.habits; render(); });
    ctx.on('log:changed', p => { days = p.days; today = p.today; render(); });
  }
});
