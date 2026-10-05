/* chart: the hero chart, legend and hover readout.
   Listens: analysis:ready, whatif:changed, engine:status.  Calls: engine.curve.  Emits: nothing. */
Kernel.register({
  name: 'chart',
  listens: ['analysis:ready', 'whatif:changed', 'engine:status'],
  needs: ['engine.curve'],
  init(ctx){
    const $ = ctx.$;
    const {esc, fmt, fmtTick, niceTicks, model, peakInfo, wiActive, whatIf, domainMax} = Lib;
    const host = $('#chart');
    let A = null, wi = {a: 1, b: 1, l: 1}, eng = null, seq = 0, last = null, view = null;

    ctx.on('analysis:ready', a => { A = a; draw(); });
    ctx.on('whatif:changed', w => { wi = w; if (A && A.ok) draw(); });
    ctx.on('engine:status', s => { eng = s; sub(); });
    ctx.observe(host, () => { if (last) paint(); });

    function sub(){
      $('#chartSub').textContent = A && A.ok
        ? A.rows.length + ' runs, load in ' + A.units.n + (eng ? ', engine ' + (eng.mode === 'worker' ? 'in a background worker' : 'in the page') : '')
        : '';
    }

    async function draw(){
      const my = ++seq;
      if (!A || !A.ok){
        view = null; last = null;
        host.innerHTML = '<div class="empty">Add at least four runs at three or more different loads. The fitted curve appears here as soon as there is enough data.</div>';
        legend(); sub(); hover(null);
        return;
      }
      const fit = A.fit, on = wiActive(wi), hi = domainMax(A.rows, fit, wi), steps = 160, ns = [];
      for (let i = 0; i <= steps; i++) ns.push(hi*i/steps);
      const sets = [{lambda: fit.lambda, alpha: fit.alpha, beta: fit.beta}];
      if (on){ const w = whatIf(fit, wi); sets.push({lambda: w.lam, alpha: w.a, beta: w.b}); }
      let c;
      try { c = await ctx.call('engine', 'curve', {id: A.id, sets, ns, band: [0.05, 0.95]}); }
      catch (e) { return; }                       // stale analysis or engine swap: a newer draw is on its way
      if (my !== seq) return;
      last = {hi, ns, fy: c.ys[0], wy: on ? c.ys[1] : [], bl: c.low, bh: c.high, on};
      paint(); legend(); sub();
    }

    function paint(){
      if (!A || !A.ok || !last) return;
      const u = A.units, flags = new Set(A.flags), fit = A.fit, ci = A.ci;
      const {hi, ns: Ns, fy, wy, bl, bh, on} = last, steps = Ns.length - 1;
      const W = Math.max(300, Math.floor(host.clientWidth || 700)), H = W < 560 ? 300 : 400;
      const m = {l: W < 560 ? 48 : 60, r: 18, t: 16, b: 46}, pw = W - m.l - m.r, ph = H - m.t - m.b;
      const ymax = Math.max(...A.rows.map(r => r.x), ...fy, ...(on ? wy : [0])) * 1.12;
      const X = n => m.l + n/hi*pw, Y = v => m.t + ph - v/ymax*ph;
      view = {m, pw, ph, hi, ymax, W, H};
      const path = ys => ys.map((v, i) => (i ? 'L' : 'M') + X(Ns[i]).toFixed(1) + ' ' + Y(Math.min(v, ymax*1.5)).toFixed(1)).join('');
      let g = '';
      const yt = niceTicks(ymax, 5), xt = niceTicks(hi, W < 560 ? 4 : 7);
      yt.forEach(v => { g += '<line class="gridl" x1="'+m.l+'" x2="'+(m.l+pw)+'" y1="'+Y(v).toFixed(1)+'" y2="'+Y(v).toFixed(1)+'"/><text class="ax" x="'+(m.l-8)+'" y="'+(Y(v)+4).toFixed(1)+'" text-anchor="end">'+fmtTick(v)+'</text>'; });
      xt.forEach(v => { g += '<line class="gridl" y1="'+m.t+'" y2="'+(m.t+ph)+'" x1="'+X(v).toFixed(1)+'" x2="'+X(v).toFixed(1)+'"/><text class="ax" x="'+X(v).toFixed(1)+'" y="'+(m.t+ph+18)+'" text-anchor="middle">'+fmtTick(v)+'</text>'; });
      g += '<line class="axis" x1="'+m.l+'" x2="'+(m.l+pw)+'" y1="'+(m.t+ph)+'" y2="'+(m.t+ph)+'"/>';

      const n1 = Math.min(hi, ymax/fit.lambda);
      const band = Ns.map((n, i) => (i ? 'L' : 'M') + X(n).toFixed(1) + ' ' + Y(Math.min(bh[i], ymax*1.5)).toFixed(1)).join('') +
        Ns.map((n, i) => 'L' + X(Ns[steps-i]).toFixed(1) + ' ' + Y(bl[steps-i]).toFixed(1)).join('') + 'Z';
      const cid = 'clip' + Math.floor(Math.random()*1e6);
      let body = '<g clip-path="url(#'+cid+')">' +
        '<path class="c-band" d="'+band+'"/>' +
        '<line class="c-lin" x1="'+X(0)+'" y1="'+Y(0)+'" x2="'+X(n1).toFixed(1)+'" y2="'+Y(fit.lambda*n1).toFixed(1)+'"/>' +
        '<path class="c-fit" d="'+path(fy)+'"/>' + (on ? '<path class="c-wi" d="'+path(wy)+'"/>' : '') + '</g>';

      const pk = fit.peak;
      if (pk.kind === 'peak' && pk.nStar <= hi){
        const px = X(pk.nStar), py = Y(pk.xMax), anchor = px > m.l+pw-70 ? 'end' : 'middle';
        body += '<line class="pk" x1="'+px.toFixed(1)+'" x2="'+px.toFixed(1)+'" y1="'+py.toFixed(1)+'" y2="'+(m.t+ph)+'"/>' +
          '<circle class="pkd" cx="'+px.toFixed(1)+'" cy="'+py.toFixed(1)+'" r="5.5"/>' +
          '<text class="pkt" x="'+(anchor === 'end' ? px+6 : px).toFixed(1)+'" y="'+(py-12).toFixed(1)+'" text-anchor="'+anchor+'">peak at '+fmt(pk.nStar)+'</text>';
        const lo = ci.nStar[0], hh = ci.nStar[1];
        if (isFinite(lo)){
          const yb = m.t+ph-9, x1 = X(Math.min(lo, hi)), x2 = X(Math.min(hh, hi));
          body += '<path class="pkb" d="M'+x1.toFixed(1)+' '+(yb-5)+'V'+(yb+5)+'M'+x1.toFixed(1)+' '+yb+'H'+x2.toFixed(1)+(hh <= hi ? 'M'+x2.toFixed(1)+' '+(yb-5)+'V'+(yb+5) : 'M'+(x2-6).toFixed(1)+' '+(yb-5)+'L'+x2.toFixed(1)+' '+yb+'L'+(x2-6).toFixed(1)+' '+(yb+5))+'"/>';
        }
      }
      if (on){
        const w = whatIf(fit, wi), pwk = peakInfo(w.lam, w.a, w.b);
        if (pwk.kind === 'peak' && pwk.nStar <= hi) body += '<circle class="wid" cx="'+X(pwk.nStar).toFixed(1)+'" cy="'+Y(pwk.xMax).toFixed(1)+'" r="5"/>';
      }
      A.rows.forEach((r, i) => { body += '<circle class="pt'+(flags.has(i) ? ' flag' : '')+'" cx="'+X(r.n).toFixed(1)+'" cy="'+Y(r.x).toFixed(1)+'" r="'+(flags.has(i) ? 5 : 4.5)+'"/>'; });
      body += '<line class="xh" id="xhl" y1="'+m.t+'" y2="'+(m.t+ph)+'" x1="0" x2="0" visibility="hidden"/><circle class="xhd" id="xhd" r="4.5" cx="0" cy="0" visibility="hidden"/>';

      const label = 'Throughput versus load. ' + (pk.kind === 'peak' ? 'Fitted peak near ' + fmt(pk.nStar) + ' ' + u.n + '.' : 'No peak found in the fitted range.');
      host.innerHTML = '<svg viewBox="0 0 '+W+' '+H+'" width="'+W+'" height="'+H+'" role="img" aria-label="'+esc(label)+'">' +
        '<defs><clipPath id="'+cid+'"><rect x="'+m.l+'" y="'+m.t+'" width="'+pw+'" height="'+ph+'"/></clipPath></defs>' +
        '<rect class="plotbg" x="'+m.l+'" y="'+m.t+'" width="'+pw+'" height="'+ph+'"/>' + g + body +
        '<text class="axt" x="'+(m.l+pw/2)+'" y="'+(H-6)+'" text-anchor="middle">Load ('+esc(u.n)+')</text>' +
        '<text class="axt" transform="translate(13 '+(m.t+ph/2)+') rotate(-90)" text-anchor="middle">Throughput ('+esc(u.x)+')</text></svg>';
      hover(null);
    }

    function hover(n){
      const rd = $('#hoverReadout'), hl = $('#xhl'), hd = $('#xhd');
      if (n === null || !A || !A.ok || !view){
        if (hl){ hl.setAttribute('visibility', 'hidden'); hd.setAttribute('visibility', 'hidden'); }
        rd.textContent = A && A.ok ? 'Move across the chart to read the fitted curve at any load.' : '';
        return;
      }
      const fit = A.fit, u = A.units, xf = fit.lambda*model(n, fit.alpha, fit.beta), eff = n > 0 ? model(n, fit.alpha, fit.beta)/n : 1;
      const px = view.m.l + n/view.hi*view.pw, py = view.m.t + view.ph - xf/view.ymax*view.ph;
      hl.setAttribute('x1', px); hl.setAttribute('x2', px); hl.setAttribute('visibility', 'visible');
      hd.setAttribute('cx', px); hd.setAttribute('cy', py); hd.setAttribute('visibility', 'visible');
      let t = fmt(n) + ' ' + u.n + ': ' + fmt(xf, 4) + ' ' + u.x + ' fitted, ' + (eff*100).toFixed(0) + '% of linear scaling';
      if (wiActive(wi)){ const w = whatIf(fit, wi); t += '; with changes ' + fmt(w.lam*model(n, w.a, w.b), 4) + ' ' + u.x; }
      rd.textContent = t;
    }
    host.addEventListener('pointermove', e => {
      if (!view) return;
      const svg = $('#chart svg'); if (!svg) return;
      const r = svg.getBoundingClientRect(), px = (e.clientX - r.left)*(view.W/r.width);
      const n = (px - view.m.l)/view.pw*view.hi;
      hover(n >= 0 && n <= view.hi ? n : null);
    });
    host.addEventListener('pointerleave', () => hover(null));

    function legend(){
      const lg = $('#legend');
      if (!A || !A.ok){ lg.innerHTML = ''; return; }
      const sw = inner => '<svg width="24" height="12" viewBox="0 0 24 12" aria-hidden="true">' + inner + '</svg>';
      let h = '<span>' + sw('<circle cx="12" cy="6" r="4.5" fill="var(--ink)"/>') + 'Measured runs</span>';
      if (A.flags.length) h += '<span>' + sw('<circle cx="12" cy="6" r="4.5" fill="none" stroke="var(--peak)" stroke-width="2.4"/>') + 'Off-curve runs</span>';
      h += '<span>' + sw('<path d="M1 6H23" stroke="var(--fit)" stroke-width="2.6"/>') + 'Fitted curve</span>';
      h += '<span>' + sw('<rect x="1" y="1" width="22" height="10" fill="var(--fit)" opacity=".18"/>') + '90% range</span>';
      h += '<span>' + sw('<path d="M1 6H23" stroke="var(--linear)" stroke-width="1.5" stroke-dasharray="5 4"/>') + 'Linear scaling</span>';
      if (A.fit.peak.kind === 'peak') h += '<span>' + sw('<path d="M2 2V10M2 6H22M22 2V10" stroke="var(--peak)" stroke-width="2" fill="none"/>') + 'Peak load, 90% range</span>';
      if (wiActive(wi)) h += '<span>' + sw('<path d="M1 6H23" stroke="var(--whatif)" stroke-width="2.4" stroke-dasharray="2 5" stroke-linecap="round"/>') + 'With your changes</span>';
      lg.innerHTML = h;
    }
  }
});
