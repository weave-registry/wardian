/* week: minutes focused on each of the last 7 days, today on the right, as an SVG bar chart.
   Hover a bar, or focus the chart and use the arrow keys, to read a day.
   Listens: log:changed.  Capabilities: db. */
Kernel.register({
  name: 'week',
  listens: ['log:changed'],
  caps: ['db'],
  init(ctx){
    const host = ctx.$('#chart'), readout = ctx.$('#readout'), NS = 'http://www.w3.org/2000/svg';
    const S = (tag, attrs, text) => { const n = document.createElementNS(NS, tag); for (const k in attrs) n.setAttribute(k, attrs[k]); if (text !== undefined) n.textContent = text; return n; };
    let days = [], cursor = 6;

    async function load(){
      const from = Log.daysAgo(6);
      days = Array.from({length: 7}, (_, i) => ({day: Log.daysAgo(6 - i), minutes: 0, count: 0}));
      try {
        const d = await Log.db(ctx);
        if (d){
          const r = await d.query({sql: 'SELECT day, SUM(minutes), COUNT(*) FROM sessions WHERE day >= ? GROUP BY day', params: [from]});
          for (const [day, minutes, count] of r.rows){ const x = days.find(v => v.day === day); if (x){ x.minutes = minutes; x.count = count; } }
        }
      } catch (e){ ctx.$('#sub').textContent = 'Could not read the log: ' + Log.errText(e); }
      const total = days.reduce((s, v) => s + v.minutes, 0), active = days.filter(v => v.count).length;
      ctx.$('#sub').textContent = Log.minutes(total) + ' in all' + (active ? ' · ' + Log.minutes(total / active) + ' a day on the days you focused' : '');
      draw();
    }

    function draw(){
      host.replaceChildren();
      const W = Math.max(300, host.clientWidth), H = 220, m = {l: 44, r: 8, t: 18, b: 26};
      const peak = Math.max(60, ...days.map(v => v.minutes));
      const step = [15, 30, 60, 120, 240].find(s => peak / s <= 5) || 480, top = Math.ceil(peak / step) * step;
      const y = v => m.t + (1 - v / top) * (H - m.t - m.b), slot = (W - m.l - m.r) / 7, bw = Math.min(56, slot - 10);
      const svg = S('svg', {viewBox: `0 0 ${W} ${H}`, width: W, height: H, 'aria-hidden': 'true'});
      for (let v = 0; v <= top; v += step){
        svg.append(S('line', {x1: m.l, x2: W - m.r, y1: y(v), y2: y(v), class: 'grid'}),
          S('text', {x: m.l - 6, y: y(v) + 4, 'text-anchor': 'end', class: 'ax'}, v >= 60 && v % 60 === 0 ? v / 60 + ' h' : v + 'm'));
      }
      days.forEach((v, i) => {
        const cx = m.l + slot * (i + 0.5), x0 = cx - bw / 2, base = y(0);
        const hit = S('rect', {x: m.l + slot * i, y: m.t, width: slot, height: H - m.t - m.b, class: 'hit'});
        hit.addEventListener('pointerenter', () => show(i));
        svg.append(hit);
        if (v.minutes > 0){
          const h = Math.max(2, base - y(v.minutes)), r = Math.min(4, h, bw / 2);
          const bar = S('path', {class: 'bar' + (i === 6 ? ' today' : ''), d: `M${x0},${base}V${base - h + r}q0,-${r} ${r},-${r}h${bw - 2 * r}q${r},0 ${r},${r}V${base}Z`});
          svg.append(bar);
          svg.append(S('text', {x: cx, y: base - h - 5, 'text-anchor': 'middle', class: 'val'}, Math.round(v.minutes)));
        }
        const [yy, mm, dd] = v.day.split('-').map(Number);
        const name = i === 6 ? 'Today' : new Date(yy, mm - 1, dd).toLocaleDateString(undefined, {weekday: 'short'});
        svg.append(S('text', {x: cx, y: H - 8, 'text-anchor': 'middle', class: 'ax' + (i === 6 ? ' strong' : '')}, name));
      });
      host.append(svg);
      show(cursor);
    }

    function show(i){
      cursor = Math.max(0, Math.min(6, i));
      const v = days[cursor]; if (!v) return;
      readout.textContent = (cursor === 6 ? 'Today, ' : '') + Log.date(v.day) + ': ' +
        (v.count ? Log.minutes(v.minutes) + ' in ' + v.count + (v.count === 1 ? ' session' : ' sessions') : 'no sessions') + '.';
      host.querySelectorAll('.hit').forEach((h, k) => h.classList.toggle('on', k === cursor));
    }
    host.addEventListener('keydown', e => {
      if (e.key === 'ArrowRight'){ show(cursor + 1); e.preventDefault(); }
      if (e.key === 'ArrowLeft'){ show(cursor - 1); e.preventDefault(); }
    });
    let lastWidth = 0;
    ctx.observe(host, () => { if (days.length && host.clientWidth !== lastWidth){ lastWidth = host.clientWidth; draw(); } });
    ctx.on('log:changed', load);
    let shownDay = Log.day(new Date());
    setInterval(() => { if (Log.day(new Date()) !== shownDay){ shownDay = Log.day(new Date()); load(); } }, 60000);
  }
});
