/* Lib: pure helpers shared by every app and by the engine worker.
   No DOM, no state, no I/O. Frozen, so no app can change what another app sees. */
const Lib = (() => {
  'use strict';
  const esc = s => String(s).replace(/[&<>"']/g, c => ({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[c]));
  const clamp = (v, lo, hi) => Math.min(hi, Math.max(lo, v));

  // C(N) = N / (1 + a(N-1) + b N (N-1))
  const model = (N, a, b) => N <= 0 ? 0 : N / (1 + a*(N-1) + b*N*(N-1));

  function peakInfo(lam, a, b){
    if (b > 1e-8){
      const n = Math.max(1, Math.sqrt(Math.max(0, 1-a)/b));
      return {kind:'peak', nStar:n, xMax: lam*model(n, a, b)};
    }
    if (a > 1e-6) return {kind:'ceiling', nStar:Infinity, xMax: lam/a};
    return {kind:'linear', nStar:Infinity, xMax:Infinity};
  }

  function fmt(v, sig){
    sig = sig || 3;
    if (v === Infinity) return 'no limit';
    if (!isFinite(v)) return '–';
    if (v === 0) return '0';
    const av = Math.abs(v);
    if (av < 1e-3) return v.toExponential(1);
    if (av >= 1e9) return v.toExponential(2);
    return Number(v.toPrecision(sig)).toLocaleString('en-US', {maximumFractionDigits: 6});
  }
  function fmtTick(v){
    if (v >= 1e6) return Number((v/1e6).toFixed(2)) + 'M';
    if (v >= 1000) return Number((v/1000).toFixed(2)) + 'k';
    return String(Number(v.toFixed(4)));
  }
  function niceTicks(max, count){
    const raw = max/count, mag = Math.pow(10, Math.floor(Math.log10(raw))), nm = raw/mag;
    const step = (nm < 1.5 ? 1 : nm < 3 ? 2 : nm < 7 ? 5 : 10) * mag, t = [];
    for (let v = 0; v <= max + 1e-9; v += step) t.push(+v.toFixed(10));
    return t;
  }
  const fmtRange = (lo, hi, sig) => hi === Infinity ? fmt(lo, sig) + ' to no limit' : fmt(lo, sig) + ' to ' + fmt(hi, sig);

  // What-if: scale the fitted parameters.
  const wiActive = wi => Math.abs(wi.a-1) > 1e-6 || Math.abs(wi.b-1) > 1e-6 || Math.abs(wi.l-1) > 1e-6;
  const whatIf = (fit, wi) => ({a: clamp(fit.alpha*wi.a, 0, 1), b: fit.beta*wi.b, lam: fit.lambda*wi.l});

  function describe(p, u){
    if (p.kind === 'peak') return 'a peak of ' + fmt(p.xMax, 4) + ' ' + u.x + ' at ' + fmt(p.nStar) + ' ' + u.n;
    if (p.kind === 'ceiling') return 'no peak, levelling off toward ' + fmt(p.xMax, 4) + ' ' + u.x;
    return 'no limit within the model';
  }

  // Chart x-range: past the highest run, and far enough to show the peak (and the what-if peak).
  function domainMax(rows, fit, wi){
    const maxN = rows[rows.length-1].n;
    let hi = maxN*1.15;
    const ps = [fit.peak];
    if (wiActive(wi)){ const A = whatIf(fit, wi); ps.push(peakInfo(A.lam, A.a, A.b)); }
    ps.forEach(p => { if (p.kind === 'peak') hi = Math.max(hi, Math.min(p.nStar*1.45, maxN*5)); });
    return Math.max(hi, 2);
  }

  /* One parser per input, shared by the box, a file and a table from the Splunk table app. */
  // Runs, one per line: load, throughput and, optionally, response time. A first line of words is a
  // header. Returns the rows sorted by load, the lines skipped (with why), and whether a fit can run.
  const MIN_RUNS = 4, MIN_LOADS = 3;
  function parseMeasurements(text){
    const rows = [], skipped = [];
    let seen = 0;
    String(text || '').split(/\r?\n/).forEach((raw, i) => {
      const line = raw.trim(); if (!line) return;
      const nums = line.split(/[,;\t ]+/).filter(Boolean).map(Number);
      if (isNaN(nums[0]) && !seen && /[a-z]/i.test(line)){ seen++; return; }   // a header line
      seen++;
      if (nums.length < 2 || nums.slice(0, 2).some(v => !isFinite(v))) skipped.push({line: i + 1, why: 'needs two numbers: a load and a throughput'});
      else if (!(nums[0] > 0)) skipped.push({line: i + 1, why: 'the load must be above 0'});
      else if (!(nums[1] > 0)) skipped.push({line: i + 1, why: 'the throughput must be above 0'});
      else rows.push({n: nums[0], x: nums[1], r: nums.length > 2 && nums[2] > 0 && isFinite(nums[2]) ? nums[2] : null});
    });
    rows.sort((p, q) => p.n - q.n);
    const loads = new Set(rows.map(r => r.n)).size;
    const enough = rows.length >= MIN_RUNS && loads >= MIN_LOADS;
    const need = enough ? '' : 'Add at least ' + MIN_RUNS + ' runs at ' + MIN_LOADS + ' or more different loads to fit the curve.';
    return {rows, skipped, enough, need};
  }
  // A target throughput: a number above 0.
  function parseTarget(text){
    const s = String(text == null ? '' : text).trim();
    if (s === '') return {value: null};
    const v = Number(s);
    if (!isFinite(v) || !(v > 0)) return {error: 'Enter a throughput above 0, like 10000.'};
    return {value: v};
  }

  return Object.freeze({esc, clamp, model, peakInfo, fmt, fmtTick, niceTicks, fmtRange, wiActive, whatIf, describe, domainMax, parseMeasurements, parseTarget, MIN_RUNS, MIN_LOADS});
})();
