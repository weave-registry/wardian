/* histogram: two charts on one time axis. Above, the share of trials that finish in each bin;
   below, the S-curve: the chance of finishing by each time. P50, P80 and P95 are marked on both.
   Hover, or focus the chart and use the arrow keys, to read one bin.
   Listens: sim:result.  Draws SVG in its own frame and redraws when the frame changes width. */
Kernel.register({
  name: 'histogram',
  listens: ['sim:result'],
  init(ctx){
    const host = ctx.$('#chart'), readout = ctx.$('#readout'), S = Fmt.svg;
    let r = null, geom = null, cursor = null;

    function draw(){
      host.replaceChildren(); geom = null;
      const ok = r && r.ok;
      ctx.$('#empty').hidden = ok; host.hidden = !ok; ctx.$('.legend').hidden = !ok;
      if (!ok){ readout.textContent = ''; ctx.$('#sub').textContent = ''; return; }
      const {lo, width, counts} = r.hist, N = r.done, hi = lo + width * counts.length;
      const W = Math.max(320, host.clientWidth), m = {l: 48, r: 40}, H1 = 190, H2 = 150, GAP = 34, B = 26;
      const H = H1 + GAP + H2 + B, top2 = H1 + GAP;
      const x = v => m.l + (v - lo) / (hi - lo) * (W - m.l - m.r);
      const peak = Math.max(...counts) / N;
      const step = peak > 0.2 ? 0.1 : peak > 0.08 ? 0.05 : peak > 0.04 ? 0.02 : 0.01, yMax = Math.ceil(peak / step) * step;
      const y1 = v => 8 + (1 - v / yMax) * (H1 - 8), y2 = v => top2 + (1 - v) * H2;
      const svg = S('svg', {viewBox: `0 0 ${W} ${H}`, width: W, height: H, 'aria-hidden': 'true'});

      for (let v = 0; v <= yMax + 1e-9; v += step){
        svg.append(S('line', {x1: m.l, x2: W - m.r, y1: y1(v), y2: y1(v), class: 'grid'}),
          S('text', {x: m.l - 6, y: y1(v) + 4, 'text-anchor': 'end', class: 'ax'}, Fmt.pct(v)));
      }
      for (const v of [0, 0.25, 0.5, 0.75, 1]){
        svg.append(S('line', {x1: m.l, x2: W - m.r, y1: y2(v), y2: y2(v), class: 'grid'}),
          S('text', {x: m.l - 6, y: y2(v) + 4, 'text-anchor': 'end', class: 'ax'}, Fmt.pct(v)));
      }
      // Time along the bottom, on round values.
      const span = hi - lo, raw = span / Math.max(3, Math.floor((W - 80) / 70)), mag = Math.pow(10, Math.floor(Math.log10(raw)));
      const tick = [1, 2, 5, 10].map(s => s * mag).find(s => s >= raw);
      for (let v = Math.ceil(lo / tick) * tick; v <= hi + 1e-9; v += tick){
        svg.append(S('text', {x: x(v), y: H - 8, 'text-anchor': 'middle', class: 'ax'}, Fmt.num(v, tick < 1 ? 1 : 0)));
      }
      svg.append(S('text', {x: W - 2, y: H - 8, 'text-anchor': 'end', class: 'ax unit'}, r.unit));

      // Bars: a 2px gap between neighbours, rounded at the top.
      const bw = Math.max(1, x(lo + width) - x(lo) - 2);
      counts.forEach((c, i) => {
        if (!c) return;
        const x0 = x(lo + i * width) + 1, h = Math.max(1, H1 - y1(c / N)), rad = Math.min(4, bw / 2, h);
        svg.append(S('path', {class: 'bar', d: `M${x0},${H1}V${H1 - h + rad}q0,-${rad} ${rad},-${rad}h${bw - 2 * rad}q${rad},0 ${rad},${rad}V${H1}Z`}));
      });
      // The S-curve from the percentiles: P0 … P100.
      const pts = r.pct.map((v, q) => [x(v), y2(q / 100)]);
      svg.append(S('path', {class: 'curve', d: 'M' + x(lo) + ',' + y2(0) + pts.map(([a, b]) => 'L' + a.toFixed(1) + ',' + b.toFixed(1)).join('')}));
      // P50, P80, P95 on both charts, labelled once.
      for (const q of [50, 80, 95]){
        const px = x(r.pct[q]);
        svg.append(S('line', {x1: px, x2: px, y1: 4, y2: H1, class: 'mark'}), S('line', {x1: px, x2: px, y1: top2, y2: top2 + H2, class: 'mark'}),
          S('circle', {cx: px, cy: y2(q / 100), r: 4, class: 'dot'}),
          S('text', {x: px + 4, y: 14, class: 'mlabel'}, 'P' + q));
      }
      const hair = S('line', {y1: 4, y2: top2 + H2, class: 'hair', visibility: 'hidden'});
      const band = S('rect', {y: 4, height: H1 - 4, class: 'band', visibility: 'hidden'});
      svg.prepend(band); svg.append(hair);
      host.append(svg);
      geom = {x, W, m, lo, width, counts, N, hair, band, svg};
      ctx.$('#sub').textContent = Fmt.int(N) + ' trials · bins of ' + Fmt.dur(width, r.unit, width < 1 ? 2 : 0);
      show(cursor === null ? counts.indexOf(Math.max(...counts)) : cursor);
    }

    function show(i){
      if (!geom) return;
      const {x, lo, width, counts, N, hair, band} = geom;
      cursor = Math.max(0, Math.min(counts.length - 1, i));
      const a = lo + cursor * width, b = a + width;
      let cum = 0; for (let k = 0; k <= cursor; k++) cum += counts[k];
      band.setAttribute('x', x(a)); band.setAttribute('width', Math.max(1, x(b) - x(a))); band.setAttribute('visibility', 'visible');
      hair.setAttribute('x1', x(b)); hair.setAttribute('x2', x(b)); hair.setAttribute('visibility', 'visible');
      const date = Fmt.date(r.start, b, r.unit);
      readout.textContent = Fmt.num(a) + '–' + Fmt.num(b) + ' ' + Fmt.unitWord(r.unit, 2) + ': ' + Fmt.pct(counts[cursor] / N, 1) +
        ' of trials finish in this bin; ' + Fmt.pct(cum / N, 1) + ' finish by ' + Fmt.dur(b, r.unit) + (date ? ' (' + date + ')' : '') + '.';
    }

    host.addEventListener('pointermove', e => {
      if (!geom) return;
      const box = geom.svg.getBoundingClientRect(), px = (e.clientX - box.left) * (geom.W / box.width);
      const v = geom.lo + (px - geom.m.l) / (geom.W - geom.m.l - geom.m.r) * geom.width * geom.counts.length;
      show(Math.floor((v - geom.lo) / geom.width));
    });
    host.addEventListener('keydown', e => {
      if (e.key === 'ArrowRight'){ show((cursor || 0) + 1); e.preventDefault(); }
      if (e.key === 'ArrowLeft'){ show((cursor || 0) - 1); e.preventDefault(); }
    });
    let lastWidth = 0;
    ctx.observe(host, () => { if (r && host.clientWidth !== lastWidth){ lastWidth = host.clientWidth; draw(); } });
    ctx.on('sim:result', p => { r = p; cursor = null; draw(); });
  }
});
