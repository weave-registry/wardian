/* calendar: a year of days per habit, one square a day, like a contribution graph. The first map
   shows all habits together, darker when more were kept. Each map is one keyboard stop: the arrow
   keys move a cursor, Enter ticks or unticks the day through log.set.
   Listens: habits:changed, log:changed.  Needs: log.set. */
Kernel.register({
  name: 'calendar',
  listens: ['habits:changed', 'log:changed'],
  needs: ['log.set'],
  init(ctx) {
    const $ = ctx.$, NS = 'http://www.w3.org/2000/svg';
    const CELL = 12, STEP = 15, LEFT = 30, TOP = 18;
    let habits = null, days = {}, today = Days.today();
    const cursor = {};                                  // map id -> the day under the keyboard cursor

    const svgEl = (tag, attrs) => { const e = document.createElementNS(NS, tag); for (const k in attrs) e.setAttribute(k, attrs[k]); return e; };
    const range = () => { const start = Days.add(today, -Days.weekday(today) - 52 * 7); return { start, n: Days.diff(start, today) + 1 }; };
    const where = (start, d) => { const i = Days.diff(start, d); return { x: LEFT + Math.floor(i / 7) * STEP, y: TOP + Days.weekday(d) * STEP }; };

    // `level(day)` is 0..1; `say(day)` is the words for that day.
    function heatmap(id, title, color, level, say, editable) {
      const { start, n } = range();
      const weeks = Math.ceil(n / 7);
      const w = LEFT + weeks * STEP, h = TOP + 7 * STEP;
      const svg = svgEl('svg', { viewBox: '0 0 ' + w + ' ' + h, class: editable ? 'heat' : 'heat ro', tabindex: '0', role: 'application',
        'aria-roledescription': 'calendar', 'aria-label': title + '. Arrow keys move between days' + (editable ? '; Enter ticks or unticks the day.' : '.') });
      svg.dataset.map = id;
      ['Mon', '', 'Wed', '', 'Fri', '', ''].forEach((w, r) => { if (w) svg.append(Object.assign(svgEl('text', { x: 0, y: TOP + r * STEP + 10, class: 'ax' }), { textContent: w })); });
      let lastMonthX = -99;
      for (let i = 0; i < n; i++) {
        const d = Days.add(start, i), { x, y } = where(start, d), v = level(d);
        if (d.slice(8) === '01' || i === 0) {
          const mx = LEFT + Math.floor(i / 7) * STEP;
          if (mx - lastMonthX >= STEP * 3) {
            svg.append(Object.assign(svgEl('text', { x: mx, y: 11, class: 'ax' }), { textContent: Days.label(d, { month: 'short' }) }));
            lastMonthX = mx;
          }
        }
        const r = svgEl('rect', { x, y, width: CELL, height: CELL, rx: 2.5, class: v ? 'day on' : 'day' });
        if (v) { r.style.fill = color; r.style.fillOpacity = String(v); }
        r.dataset.day = d;
        svg.append(r);
      }
      const box = svgEl('rect', { width: CELL + 4, height: CELL + 4, rx: 4, class: 'cursor' });
      svg.append(box);

      const wrap = ctx.el('div'), head = ctx.el('div'), h3 = ctx.el('h3'), meta = ctx.el('span'), scroll = ctx.el('div'), read = ctx.el('p');
      wrap.className = 'map'; head.className = 'maphead'; scroll.className = 'scroll'; read.className = 'readout';
      read.setAttribute('aria-live', 'polite');
      h3.innerHTML = '<span class="dot"></span><span></span>';
      h3.firstChild.style.background = color; h3.lastChild.textContent = title;
      let total = 0; for (let i = 0; i < n; i++) if (level(Days.add(start, i)) === 1) total++;
      meta.className = 'sub'; meta.textContent = editable ? Stats.plural(total, 'day') + ' in the last year' : '';
      head.append(h3, meta); scroll.append(svg); wrap.append(head, scroll, read);

      const show = (d, move) => {
        cursor[id] = d;
        const { x, y } = where(start, d);
        box.setAttribute('x', x - 2); box.setAttribute('y', y - 2);
        read.textContent = Days.label(d) + ': ' + say(d);
        if (move) { const left = x * svg.clientWidth / w - scroll.clientWidth / 2; if (Math.abs(scroll.scrollLeft - left) > scroll.clientWidth / 2 - 30) scroll.scrollLeft = left; }
      };
      svg.addEventListener('focus', () => show(cursor[id] || today, true));
      svg.addEventListener('blur', () => { read.textContent = ''; });
      svg.addEventListener('mouseover', e => { const d = e.target.dataset && e.target.dataset.day; if (d) read.textContent = Days.label(d) + ': ' + say(d); });
      svg.addEventListener('mouseleave', () => { read.textContent = document.activeElement === svg ? Days.label(cursor[id]) + ': ' + say(cursor[id]) : ''; });
      svg.addEventListener('keydown', e => {
        const d = cursor[id] || today;
        const step = { ArrowLeft: -7, ArrowRight: 7, ArrowUp: -1, ArrowDown: 1, PageUp: -28, PageDown: 28 }[e.key];
        if (step !== undefined) {
          e.preventDefault();
          const next = Days.add(d, step);
          if (next >= start && next <= today) show(next, true);
        } else if (e.key === 'Home' || e.key === 'End') { e.preventDefault(); show(e.key === 'Home' ? start : today, true); }
        else if ((e.key === 'Enter' || e.key === ' ') && editable) { e.preventDefault(); toggle(id, d); }
      });
      if (editable) svg.addEventListener('click', e => { const d = e.target.dataset && e.target.dataset.day; if (d) { cursor[id] = d; svg.focus(); toggle(id, d); } });
      return { wrap, svg, scroll, show };
    }

    async function toggle(habit, day) {
      const done = !(days[habit] || []).includes(day);
      try { await ctx.call('log', 'set', { habit, day, done }); $('#note').textContent = ''; }
      catch (e) { $('#note').textContent = 'Could not save that: ' + e.message; }
    }

    function render() {
      if (!habits) return;
      const active = document.activeElement && document.activeElement.dataset ? document.activeElement.dataset.map : null;
      const maps = $('#maps');
      const scrolls = Object.fromEntries([...maps.querySelectorAll('svg')].map(s => [s.dataset.map, s.parentElement.scrollLeft]));
      maps.replaceChildren();
      $('#empty').hidden = habits.length > 0;
      $('#sub').textContent = habits.length ? 'last 12 months' : '';
      if (!habits.length) return;
      const sets = Object.fromEntries(habits.map(h => [h.id, new Set(days[h.id] || [])]));
      const built = [];
      if (habits.length > 1) {
        const count = d => habits.filter(h => sets[h.id].has(d)).length;
        built.push(['all', heatmap('all', 'All habits', 'var(--accent)', d => { const c = count(d); return c ? Math.max(0.25, c / habits.length) : 0; },
          d => count(d) + ' of ' + habits.length + ' habits', false)]);
      }
      for (const h of habits) {
        built.push([h.id, heatmap(h.id, h.name, h.color, d => sets[h.id].has(d) ? 1 : 0, d => sets[h.id].has(d) ? 'done' : 'not done', true)]);
      }
      for (const [id, m] of built) {
        maps.append(m.wrap);
        m.scroll.scrollLeft = id in scrolls ? scrolls[id] : m.scroll.scrollWidth;
        if (id === active) { m.svg.focus({ preventScroll: true }); m.show(cursor[id] || today, false); }
      }
    }

    ctx.on('habits:changed', p => { habits = p.habits; render(); });
    ctx.on('log:changed', p => {
      if (p.today !== today) for (const k in cursor) delete cursor[k];   // a new day: start the cursors on it
      days = p.days; today = p.today; render();
    });
  }
});
