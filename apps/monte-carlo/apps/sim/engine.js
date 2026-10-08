/* SimEngine: the Monte Carlo simulation itself. Pure JavaScript with no DOM, so the same text runs
   in a Web Worker (the normal case) or in the sim frame when workers are blocked.
   Each trial draws a duration for every task, schedules the tasks in dependency order, and records
   when the project finishes and which tasks were on the critical path.
   The random numbers come from a seeded generator, so one seed always gives the same results,
   however the work is cut into slices. */
const SimEngine = (() => {
  'use strict';

  // sfc32, seeded through splitmix32: fast, small, and good enough for simulation.
  function rng(seed){
    let s = seed >>> 0;
    const split = () => { s = (s + 0x9e3779b9) >>> 0; let z = s; z = Math.imul(z ^ (z >>> 16), 0x85ebca6b); z = Math.imul(z ^ (z >>> 13), 0xc2b2ae35); return (z ^ (z >>> 16)) >>> 0; };
    let a = split(), b = split(), c = split(), d = split();
    return () => {
      const t = (((a + b) >>> 0) + d) >>> 0;
      d = (d + 1) >>> 0; a = b ^ (b >>> 9); b = (c + (c << 3)) >>> 0;
      c = (c << 21) | (c >>> 11); c = (c + t) >>> 0;
      return (t + 0.5) / 4294967296;                       // never exactly 0 or 1
    };
  }
  const normal = r => Math.sqrt(-2 * Math.log(r())) * Math.cos(2 * Math.PI * r());
  // Marsaglia and Tsang's gamma sampler; shape k >= 1 here (PERT shapes are at least 1).
  function gamma(r, k){
    const d = k - 1 / 3, c = 1 / Math.sqrt(9 * d);
    for (;;){
      let x, v;
      do { x = normal(r); v = 1 + c * x; } while (v <= 0);
      v = v * v * v;
      const u = r();
      if (u < 1 - 0.0331 * x * x * x * x || Math.log(u) < 0.5 * x * x + d * (1 - v + Math.log(v))) return d * v;
    }
  }

  // Everything one run needs, in typed arrays, tasks in dependency order.
  function start(project, trials, seed){
    const {tasks, dist} = project;
    const chk = Project.check(tasks);
    if (!chk.ok) throw new Error(chk.problem);
    const n = tasks.length, order = chk.order, pos = new Map(order.map((ti, k) => [tasks[ti].id, k]));
    const o = new Float64Array(n), m = new Float64Array(n), p = new Float64Array(n), al = new Float64Array(n), be = new Float64Array(n);
    const predAt = new Int32Array(n + 1), preds = [];
    order.forEach((ti, k) => {
      const t = tasks[ti];
      o[k] = t.o; m[k] = t.m; p[k] = t.p;
      if (t.p > t.o){ al[k] = 1 + 4 * (t.m - t.o) / (t.p - t.o); be[k] = 1 + 4 * (t.p - t.m) / (t.p - t.o); }
      predAt[k] = preds.length;
      t.after.forEach(id => preds.push(pos.get(id)));
    });
    predAt[n] = preds.length;
    return {
      tasks, order, n, o, m, p, al, be, predAt, preds: Int32Array.from(preds), pert: dist !== 'triangular',
      end: new Float64Array(n), from: new Int32Array(n), crit: new Float64Array(n),
      finishes: new Float64Array(trials), trials, done: 0, seed, rand: rng(seed), began: Date.now(),
    };
  }

  function draw(st, k){
    const o = st.o[k], p = st.p[k];
    if (p <= o) return o;
    const r = st.rand;
    if (st.pert){ const x = gamma(r, st.al[k]), y = gamma(r, st.be[k]); return o + (p - o) * x / (x + y); }
    const m = st.m[k], u = r(), f = (m - o) / (p - o);
    return u < f ? o + Math.sqrt(u * (p - o) * (m - o)) : p - Math.sqrt((1 - u) * (p - o) * (p - m));
  }

  // Runs up to `count` more trials.
  function step(st, count){
    const {n, predAt, preds, end, from, crit, finishes} = st;
    const stop = Math.min(st.trials, st.done + count);
    for (let t = st.done; t < stop; t++){
      let last = 0, lastK = 0;
      for (let k = 0; k < n; k++){
        let s = 0, f = -1;
        for (let q = predAt[k]; q < predAt[k + 1]; q++){ const j = preds[q]; if (f < 0 || end[j] > s){ s = end[j]; f = j; } }
        end[k] = s + draw(st, k); from[k] = f;
        if (end[k] >= last){ last = end[k]; lastK = k; }
      }
      for (let k = lastK; k >= 0; k = from[k]) crit[k]++;  // walk the critical path back
      finishes[t] = last;
    }
    st.done = stop;
  }

  // Summary of the trials done so far: percentiles, mean, spread, histogram, criticality.
  function finish(st, stopped){
    const N = st.done, xs = st.finishes.subarray(0, N).slice().sort();
    let sum = 0; for (let i = 0; i < N; i++) sum += xs[i];
    const mean = sum / N;
    let sq = 0; for (let i = 0; i < N; i++) sq += (xs[i] - mean) * (xs[i] - mean);
    const at = q => xs[Math.min(N - 1, Math.max(0, Math.ceil(q * N) - 1))];
    const pct = []; for (let q = 0; q <= 100; q++) pct.push(q === 0 ? xs[0] : at(q / 100));
    const below = v => { let lo = 0, hi = N; while (lo < hi){ const mid = (lo + hi) >> 1; if (xs[mid] <= v) lo = mid + 1; else hi = mid; } return lo / N; };
    // About 40 bins, on a round width.
    const span = Math.max(xs[N - 1] - xs[0], 1e-9), raw = span / 40, mag = Math.pow(10, Math.floor(Math.log10(raw)));
    const width = [1, 2, 2.5, 5, 10].map(s => s * mag).find(w => w >= raw);
    const lo = Math.floor(xs[0] / width) * width, counts = new Array(Math.max(1, Math.ceil((xs[N - 1] - lo) / width + 1e-9))).fill(0);
    for (let i = 0; i < N; i++) counts[Math.min(counts.length - 1, Math.floor((xs[i] - lo) / width))]++;
    const T = st.tasks, likely = Project.finish(T, st.order, t => t.m), pertMean = Project.finish(T, st.order, t => (t.o + 4 * t.m + t.p) / 6);
    return {
      ok: true, seed: st.seed, trials: st.trials, done: N, stopped: !!stopped, dist: st.pert ? 'pert' : 'triangular',
      ms: Date.now() - st.began, mean, sd: Math.sqrt(sq / Math.max(1, N - 1)), min: xs[0], max: xs[N - 1], pct,
      hist: {lo, width, counts},
      likely: {value: likely, chance: below(likely)}, pertMean: {value: pertMean, chance: below(pertMean)},
      crit: st.order.map((ti, k) => ({id: T[ti].id, num: ti + 1, name: T[ti].name, o: T[ti].o, m: T[ti].m, p: T[ti].p, index: st.crit[k] / N}))
        .sort((a, b) => b.index - a.index || a.num - b.num),
    };
  }

  /* serve(post): a message handler for one simulation at a time. In: {type: 'run', id, project,
     trials, seed} and {type: 'stop', id}. Out: {type: 'progress' | 'result' | 'error', id, ...}.
     A new run replaces the one in progress. Stop ends the run and reports the trials done so far.
     Work goes in slices of about 40 ms, so a stop message gets through between slices. */
  function serve(post){
    let active = 0, stopping = 0;
    async function run(m){
      let st;
      try { st = start(m.project, m.trials, m.seed); }
      catch (e){ post({type: 'error', id: m.id, error: String(e && e.message || e)}); return; }
      while (st.done < st.trials){
        const t0 = Date.now();
        do step(st, 500); while (st.done < st.trials && Date.now() - t0 < 40);
        post({type: 'progress', id: m.id, done: st.done, trials: st.trials});
        await new Promise(r => setTimeout(r, 0));
        if (active !== m.id) return;                        // replaced by a newer run
        if (stopping === m.id) break;
      }
      active = 0;
      post({type: 'result', id: m.id, result: finish(st, st.done < st.trials)});
    }
    return m => {
      if (m.type === 'run'){ active = m.id; run(m); }
      else if (m.type === 'stop' && m.id === active) stopping = m.id;
    };
  }

  return Object.freeze({serve, start, step, finish, rng});
})();
