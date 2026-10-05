/* chart: the remaining balance month by month, with and without the extra payment.
   Listens: plan:ready.  Draws SVG in its own frame and redraws when the frame changes width. */
Kernel.register({
  name: 'chart',
  listens: ['plan:ready'],
  init(ctx) {
    const host = ctx.$('#chart'), readout = ctx.$('#readout');
    const NS = 'http://www.w3.org/2000/svg';
    const svgEl = (tag, attrs, text) => {
      const n = document.createElementNS(NS, tag);
      for (const [k, v] of Object.entries(attrs)) n.setAttribute(k, v);
      if (text !== undefined) n.textContent = text;
      return n;
    };
    let plan = null, cursor = null, geom = null;

    function draw() {
      host.replaceChildren();
      if (!plan || !plan.ok) { readout.textContent = ''; return; }
      const W = Math.max(320, host.clientWidth), H = W < 560 ? 240 : 300;
      const m = { l: 64, r: 14, t: 12, b: 30 };
      const months = Math.max(plan.baseline.months, plan.months);
      const x = i => m.l + (i / months) * (W - m.l - m.r);
      const y = v => m.t + (1 - v / plan.loan.amount) * (H - m.t - m.b);
      const svg = svgEl('svg', { viewBox: `0 0 ${W} ${H}`, width: W, height: H, role: 'img', 'aria-hidden': 'true' });
      const compact = v => { try { return new Intl.NumberFormat(undefined, { style: 'currency', currency: plan.loan.currency, notation: 'compact', maximumFractionDigits: 1 }).format(v); } catch { return String(Math.round(v)); } };

      for (let k = 0; k <= 4; k++) {                        // horizontal grid and amounts
        const v = (plan.loan.amount * k) / 4;
        svg.append(svgEl('line', { x1: m.l, x2: W - m.r, y1: y(v), y2: y(v), class: 'grid' }));
        svg.append(svgEl('text', { x: m.l - 8, y: y(v) + 4, 'text-anchor': 'end', class: 'ax' }, compact(v)));
      }
      const years = Math.ceil(months / 12), step = years > 20 ? 5 : years > 8 ? 2 : 1;
      for (let yr = 0; yr <= years; yr += step) {           // years along the bottom
        svg.append(svgEl('text', { x: x(Math.min(yr * 12, months)), y: H - 8, 'text-anchor': 'middle', class: 'ax' }, `${yr}y`));
      }
      // Balances start at the full amount before the first payment.
      const pts = bal => [[0, plan.loan.amount], ...bal.map((b, i) => [i + 1, b])];
      const path = list => list.map(([i, v], k) => `${k ? 'L' : 'M'}${x(i).toFixed(1)},${y(v).toFixed(1)}`).join('');
      const base = pts(plan.baseline.balances), mine = pts(plan.rows.map(r => r[2]));
      svg.append(svgEl('path', { d: path(base), class: 'base' }));
      svg.append(svgEl('path', { d: `${path(mine)}L${x(plan.months)},${y(0)}L${x(0)},${y(0)}Z`, class: 'area' }));
      svg.append(svgEl('path', { d: path(mine), class: 'plan' }));
      const hair = svgEl('line', { y1: m.t, y2: H - m.b, class: 'hair', visibility: 'hidden' });
      const dot = svgEl('circle', { r: 4.5, class: 'dot', visibility: 'hidden' });
      svg.append(hair, dot);
      host.append(svg);
      geom = { x, y, m, W, months, base, mine, hair, dot, svg };
      show(cursor ?? Math.round(plan.months / 3));
    }

    function show(i) {
      if (!geom) return;
      const { months, base, mine, x, y, hair, dot } = geom;
      cursor = Math.max(0, Math.min(months, i));
      const left = (mine[cursor] || [cursor, 0])[1], baseLeft = (base[cursor] || [cursor, 0])[1];
      hair.setAttribute('x1', x(cursor)); hair.setAttribute('x2', x(cursor)); hair.setAttribute('visibility', 'visible');
      dot.setAttribute('cx', x(cursor)); dot.setAttribute('cy', y(left)); dot.setAttribute('visibility', 'visible');
      const cur = plan.loan.currency;
      readout.textContent = cursor === 0
        ? `Before the first payment: ${Fmt.money(plan.loan.amount, cur)} owed.`
        : `${Fmt.month(plan.loan.start, cursor - 1)} (payment ${cursor}): ${Fmt.money(left, cur)} left` +
          (plan.loan.extra > 0 ? `, ${Fmt.money(baseLeft, cur)} with regular payments only.` : '.');
    }

    host.addEventListener('pointermove', e => {
      if (!geom) return;
      const r = geom.svg.getBoundingClientRect(), px = (e.clientX - r.left) * (geom.W / r.width);
      show(Math.round(((px - geom.m.l) / (geom.W - geom.m.l - geom.m.r)) * geom.months));
    });
    host.addEventListener('keydown', e => {
      const jump = e.shiftKey ? 12 : 1;
      if (e.key === 'ArrowRight') { show((cursor || 0) + jump); e.preventDefault(); }
      if (e.key === 'ArrowLeft') { show((cursor || 0) - jump); e.preventDefault(); }
    });
    // Redraw on width changes only: drawing changes the height, which would otherwise loop.
    let lastWidth = 0;
    ctx.observe(host, () => { if (plan && host.clientWidth !== lastWidth) { lastWidth = host.clientWidth; draw(); } });
    ctx.on('plan:ready', p => { plan = p; cursor = null; draw(); });
  }
});
