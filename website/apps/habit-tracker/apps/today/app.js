/* today: tick off today's habits, and the six days before it, in one press each.
   It never writes check-ins itself: each press asks log.set, and the table redraws from log:changed.
   Listens: habits:changed, log:changed.  Needs: log.set. */
Kernel.register({
  name: 'today',
  listens: ['habits:changed', 'log:changed'],
  needs: ['log.set'],
  init(ctx) {
    const $ = ctx.$;
    let habits = null, days = {}, today = Days.today();
    const busy = new Set();                           // "habit|day" while a press is on its way

    function render() {
      if (!habits) return;
      $('#date').textContent = Days.label(today);
      $('#empty').hidden = habits.length > 0;
      $('#week').hidden = habits.length === 0;
      const done = habits.filter(h => (days[h.id] || []).includes(today)).length;
      $('#tally').textContent = habits.length ? (done === habits.length ? 'All ' + habits.length + ' done today. Well done.' : done + ' of ' + habits.length + ' done today.') : '';
      $('#tally').classList.toggle('all', habits.length > 0 && done === habits.length);

      const week = Array.from({ length: 7 }, (_, i) => Days.add(today, i - 6));
      const focused = document.activeElement && document.activeElement.dataset ? document.activeElement.dataset.cell : null;
      const table = $('#week');
      table.replaceChildren();
      const head = ctx.el('thead'), hr = ctx.el('tr'), corner = ctx.el('th');
      corner.scope = 'col'; corner.innerHTML = '<span class="sr">Habit</span>';
      hr.append(corner);
      for (const d of week) {
        const th = ctx.el('th');
        th.scope = 'col';
        th.className = d === today ? 'is-today' : '';
        th.innerHTML = '<span class="wd"></span><span class="dn"></span>';
        th.firstChild.textContent = d === today ? 'Today' : Days.label(d, { weekday: 'short' });
        th.lastChild.textContent = String(Number(d.slice(8)));
        th.title = Days.label(d);
        hr.append(th);
      }
      head.append(hr);
      const body = ctx.el('tbody');
      for (const h of habits) {
        const tr = ctx.el('tr'), name = ctx.el('th'), set = new Set(days[h.id] || []);
        name.scope = 'row'; name.className = 'hname';
        name.innerHTML = '<span class="dot"></span><span></span>';
        name.firstChild.style.background = h.color;
        name.lastChild.textContent = h.name;
        tr.append(name);
        for (const d of week) {
          const td = ctx.el('td'), b = ctx.el('button'), on = set.has(d), key = h.id + '|' + d;
          b.type = 'button'; b.className = 'tick' + (d === today ? ' is-today' : '');
          b.dataset.cell = key;
          b.setAttribute('aria-pressed', String(on));
          b.setAttribute('aria-label', h.name + ', ' + Days.label(d, { weekday: 'long', day: 'numeric', month: 'long' }) + (on ? ': done' : ': not done'));
          b.style.setProperty('--c', h.color);
          if (busy.has(key)) b.setAttribute('aria-busy', 'true');
          b.innerHTML = on ? '<svg viewBox="0 0 16 16" aria-hidden="true"><path d="M3.5 8.5l3 3 6-7"/></svg>' : '';
          td.append(b); tr.append(td);
        }
        body.append(tr);
      }
      table.append(head, body);
      if (focused) { const b = table.querySelector('[data-cell="' + focused + '"]'); if (b) b.focus(); }
    }

    $('#week').addEventListener('click', async e => {
      const b = e.target.closest('button.tick');
      if (!b || busy.has(b.dataset.cell)) return;
      const [habit, day] = b.dataset.cell.split('|'), done = b.getAttribute('aria-pressed') !== 'true';
      busy.add(b.dataset.cell);
      b.setAttribute('aria-pressed', String(done));       // show it at once; log:changed confirms it
      try { await ctx.call('log', 'set', { habit, day, done }); $('#note').textContent = ''; }
      catch (err) { $('#note').textContent = 'Could not save that: ' + err.message; }
      finally { busy.delete(b.dataset.cell); render(); }
    });
    // Arrow keys move between the boxes, like a grid.
    $('#week').addEventListener('keydown', e => {
      const b = e.target.closest('button.tick');
      const step = { ArrowLeft: [0, -1], ArrowRight: [0, 1], ArrowUp: [-1, 0], ArrowDown: [1, 0] }[e.key];
      if (!b || !step) return;
      const td = b.parentElement, tr = td.parentElement, rows = [...tr.parentElement.children];
      const col = [...tr.children].indexOf(td), row = rows.indexOf(tr);
      const target = rows[row + step[0]] && rows[row + step[0]].children[col + step[1]];
      const next = target && target.querySelector('button.tick');
      if (next) { e.preventDefault(); next.focus(); }
    });

    ctx.on('habits:changed', p => { habits = p.habits; render(); });
    ctx.on('log:changed', p => { days = p.days; today = p.today; render(); });
  }
});
