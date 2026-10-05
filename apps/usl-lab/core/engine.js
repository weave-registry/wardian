/* UslEngine: all the heavy maths. Pure functions over plain data, no DOM and no globals except Lib.
   The identical source runs inside a Web Worker (preferred) or on the page (fallback).
   Protocol: handle(method, args) -> plain, structured-cloneable data.
     analyze({id, rows:[{n,x,r}], rdiv, boots, seed}) -> fit, ci, flags, littles, findings, stats
     curve({id, sets:[{lambda,alpha,beta}], ns, band:[lo,hi]|null}) -> {ys, low, high}   */
const UslEngine = (() => {
  'use strict';
  const {model, peakInfo, clamp} = Lib;

  function profile(ns, xs, a, b){
    let sxf = 0, sff = 0;
    for (let i = 0; i < ns.length; i++){ const fi = model(ns[i], a, b); sxf += xs[i]*fi; sff += fi*fi; }
    const lam = sff > 0 ? sxf/sff : 0;
    let sse = 0;
    for (let i = 0; i < ns.length; i++){ const r = xs[i] - lam*model(ns[i], a, b); sse += r*r; }
    return {sse, lam};
  }

  function nelderMead(fn, x0, st, iters){
    let P = [x0, [x0[0]+st[0], x0[1]], [x0[0], x0[1]+st[1]]].map(p => ({p, v: fn(p)}));
    for (let it = 0; it < iters; it++){
      P.sort((u, v) => u.v - v.v);
      const b = P[0], m = P[1], w = P[2];
      if (Math.abs(w.v - b.v) < 1e-13*(1 + Math.abs(b.v))) break;
      const c = [(b.p[0]+m.p[0])/2, (b.p[1]+m.p[1])/2];
      const pt = k => [c[0] + k*(w.p[0]-c[0]), c[1] + k*(w.p[1]-c[1])];
      const r = pt(-1), rv = fn(r);
      if (rv < b.v){ const e = pt(-2), ev = fn(e); P[2] = ev < rv ? {p:e, v:ev} : {p:r, v:rv}; }
      else if (rv < m.v){ P[2] = {p:r, v:rv}; }
      else {
        const q = pt(rv < w.v ? -0.5 : 0.5), qv = fn(q);
        if (qv < Math.min(rv, w.v)) P[2] = {p:q, v:qv};
        else {
          const m2 = [b.p[0]+0.5*(m.p[0]-b.p[0]), b.p[1]+0.5*(m.p[1]-b.p[1])];
          const w2 = [b.p[0]+0.5*(w.p[0]-b.p[0]), b.p[1]+0.5*(w.p[1]-b.p[1])];
          P[1] = {p:m2, v:fn(m2)}; P[2] = {p:w2, v:fn(w2)};
        }
      }
    }
    P.sort((u, v) => u.v - v.v);
    return P[0];
  }

  // lambda is solved exactly for each (alpha, beta); grid search + Nelder-Mead handle the other two.
  function fitUSL(ns, xs, fast){
    const nA = fast ? 22 : 44, nB = fast ? 26 : 52;
    const aG = []; for (let i = 0; i <= nA; i++) aG.push(Math.pow(i/nA, 2.5));
    const bG = [0]; for (let j = 0; j < nB; j++) bG.push(Math.pow(10, -7 + 7*j/(nB-1)));
    const cands = [];
    for (const a of aG) for (const b of bG) cands.push({a, b, sse: profile(ns, xs, a, b).sse});
    cands.sort((u, v) => u.sse - v.sse);
    const starts = [];
    for (const c of cands){
      if (starts.every(s => Math.abs(s.a-c.a) > 0.04 || Math.abs(Math.sqrt(s.b)-Math.sqrt(c.b)) > 0.01)) starts.push(c);
      if (starts.length >= 3) break;
    }
    const obj = p => profile(ns, xs, clamp(p[0], 0, 1), Math.pow(clamp(p[1], 0, 1.2), 2)).sse;
    let best = null;
    for (const s of starts){
      const v0 = Math.sqrt(s.b);
      const r = nelderMead(obj, [s.a, v0], [Math.max(0.02, s.a*0.2), Math.max(0.004, v0*0.3)], fast ? 70 : 180);
      if (!best || r.v < best.v) best = r;
    }
    const alpha = clamp(best.p[0], 0, 1), beta = Math.pow(clamp(best.p[1], 0, 1.2), 2);
    const pr = profile(ns, xs, alpha, beta);
    return {alpha, beta, lambda: pr.lam, sse: pr.sse};
  }

  function mulberry32(a){
    return function(){
      a |= 0; a = a + 0x6D2B79F5 | 0;
      let t = Math.imul(a ^ a>>>15, 1 | a);
      t = t + Math.imul(t ^ t>>>7, 61 | t) ^ t;
      return ((t ^ t>>>14) >>> 0) / 4294967296;
    };
  }

  function bootstrap(ns, xs, fit, B, seed){
    const rng = mulberry32(seed), n = ns.length;
    const fitted = ns.map(N => fit.lambda*model(N, fit.alpha, fit.beta));
    const res = xs.map((x, i) => x - fitted[i]);
    const infl = n > 3 ? Math.sqrt(n/(n-3)) : 1;
    const out = [];
    for (let b = 0; b < B; b++){
      const xb = ns.map((_, i) => Math.max(fitted[i] + infl*res[(rng()*n)|0], fitted[i]*0.05));
      out.push(fitUSL(ns, xb, true));
    }
    return out;
  }

  const median = a => { const s = a.slice().sort((u, v) => u-v), m = s.length >> 1; return s.length % 2 ? s[m] : (s[m-1]+s[m])/2; };
  function pct(arr, p){
    const s = arr.slice().sort((u, v) => u-v), i = (s.length-1)*p, lo = Math.floor(i), hi = Math.ceil(i);
    if (lo === hi || s[lo] === s[hi]) return s[lo];
    if (!isFinite(s[lo]) || !isFinite(s[hi])) return (i-lo) < 0.5 ? s[lo] : s[hi];
    return s[lo] + (s[hi]-s[lo])*(i-lo);
  }

  // Bootstrap fits of the latest analysis stay inside the engine (they are large and only needed for bands).
  let store = {id: null, boot: [], bandKey: '', band: null};

  function analyze({id, rows, rdiv, boots, seed}){
    const R = rows.slice().sort((p, q) => p.n - q.n);
    const n = R.length, ns = R.map(r => r.n), xs = R.map(r => r.x);
    const distinct = new Set(ns).size;
    const fit = fitUSL(ns, xs, false);
    const mean = xs.reduce((s, v) => s+v, 0)/n;
    const sst = xs.reduce((s, v) => s + (v-mean)*(v-mean), 0);
    fit.r2 = sst > 0 ? 1 - fit.sse/sst : 1;
    fit.rmsePct = Math.sqrt(fit.sse/n)/mean;
    fit.peak = peakInfo(fit.lambda, fit.alpha, fit.beta);

    const boot = boots > 0 ? bootstrap(ns, xs, fit, boots, seed >>> 0) : [fit];
    store = {id, boot, bandKey: '', band: null};
    const q = arr => [pct(arr, 0.05), pct(arr, 0.95)];
    const pk = boot.map(b => peakInfo(b.lambda, b.alpha, b.beta));
    const ci = {
      alpha: q(boot.map(b => b.alpha)), beta: q(boot.map(b => b.beta)), lambda: q(boot.map(b => b.lambda)),
      nStar: q(pk.map(p => p.nStar)), xMax: q(pk.map(p => p.xMax))
    };

    // off-curve runs
    const flags = [];
    if (n >= 6){
      const res = R.map((r, i) => r.x - fit.lambda*model(r.n, fit.alpha, fit.beta));
      const med = median(res), mad = median(res.map(v => Math.abs(v-med)));
      const sigma = Math.max(1.4826*mad, 0.01*mean);
      res.forEach((v, i) => { if (Math.abs(v-med) > 2.8*sigma) flags.push(i); });
    }
    // Little's Law: X * R / N should be about 1
    const lit = R.map((r, i) => ({i, q: r.r > 0 ? r.x*(r.r/rdiv)/r.n : null})).filter(o => o.q !== null);
    const littles = lit.length >= 3
      ? {checked: lit.length, median: median(lit.map(o => o.q)), bad: lit.filter(o => Math.abs(o.q-1) > 0.15).map(o => o.i)}
      : null;

    // findings: codes + numbers. Wording belongs to the checks app.
    const maxN = ns[n-1], p = fit.peak, F = [];
    const add = (code, level, data) => F.push({code, level, data: data || {}});
    if (n < 6) add('few_runs', 'watch', {runs: n}); else add('enough_runs', 'ok', {runs: n, loads: distinct});
    if (p.kind === 'peak'){
      if (maxN >= p.nStar*1.1) add('past_peak', 'ok', {maxLoad: maxN, nStar: p.nStar});
      else add('not_past_peak', 'watch', {maxLoad: maxN, nStar: p.nStar, suggestLoad: Math.round(p.nStar*1.25)});
    } else add('no_peak', 'watch');
    if (fit.rmsePct > 0.2) add('poor_fit', 'problem', {rmsePct: fit.rmsePct, r2: fit.r2});
    else if (fit.rmsePct > 0.1) add('loose_fit', 'watch', {rmsePct: fit.rmsePct, r2: fit.r2});
    else add('tight_fit', 'ok', {rmsePct: fit.rmsePct, r2: fit.r2});
    if (n >= 6) { if (flags.length) add('outliers', 'watch', {count: flags.length}); else add('no_outliers', 'ok'); }
    if (littles){
      if (littles.bad.length) add('littles_off', 'watch', {count: littles.bad.length, median: littles.median});
      else add('littles_ok', 'ok', {median: littles.median});
    } else add('littles_unchecked', 'ok');
    if (p.kind === 'peak' && isFinite(ci.nStar[0]) && isFinite(ci.nStar[1]) && ci.nStar[1]/Math.max(ci.nStar[0], 1e-9) > 2)
      add('peak_range_wide', 'watch', {low: ci.nStar[0], high: ci.nStar[1]});

    return {fit, ci, flags, littles, findings: F, stats: {runs: n, loads: distinct, maxLoad: maxN}, samples: boot.length};
  }

  function curve({id, sets, ns, band}){
    const ys = sets.map(p => ns.map(x => p.lambda*model(x, p.alpha, p.beta)));
    let low = null, high = null;
    if (band){
      if (store.id !== id) throw new Error('stale');
      const key = id + '|' + ns.length + '|' + ns[ns.length-1] + '|' + band[0] + '|' + band[1];
      if (store.bandKey !== key){
        const lo = [], hi = [];
        ns.forEach(x => {
          const v = store.boot.map(b => b.lambda*model(x, b.alpha, b.beta)).sort((p, q) => p-q);
          lo.push(v[Math.floor(band[0]*(v.length-1))]); hi.push(v[Math.ceil(band[1]*(v.length-1))]);
        });
        store.bandKey = key; store.band = {low: lo, high: hi};
      }
      low = store.band.low; high = store.band.high;
    }
    return {ys, low, high};
  }

  function handle(method, args){
    if (method === 'analyze') return analyze(args);
    if (method === 'curve') return curve(args);
    throw new Error('unknown method: ' + method);
  }
  return {handle};
})();
