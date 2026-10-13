/* UslReady: is a table something the Universal Scalability Law can be fitted to, and which columns are
   the load, the throughput and the response time? (ADR-2610121400.) The rules are Neil Gunther's, from
   "How to Quantify Scalability" (perfdynamics.com/Manifesto/USLscalability.html):
   - the load N is the independent variable: users, threads, processes or requests in progress that the
     test sets or that are counted, never a dependent one such as a utilisation or a load average;
   - the throughput X(N) is a rate, measured near steady state, one value per load;
   - at least half a dozen load levels; X(1) need not be measured (the three-parameter form fits it);
   - data that diverge from the model mean: stop and find out why, such as throughput per unit of load
     that rises (superlinear) or a load that disagrees with Little's Law.
   assess(fields, rows, use?) -> {use: {load, throughput, response} | null, findings: [{level, text}],
   ready, checked}, level being 'ok', 'watch' or 'problem'. Pure: no DOM, no state. Uses Cells. */
const UslReady = (() => {
  const words = name => String(name).toLowerCase().replace(/([a-z])([A-Z])/g, '$1 $2').split(/[^a-z0-9]+/).filter(Boolean);
  const has = (name, list) => { const w = words(name), joined = w.join('_'); return list.some(k => k.includes('_') ? joined.includes(k) : w.includes(k)); };

  // What a column's name says it holds.
  const LOAD = ['n', 'load', 'concurrency', 'concurrent', 'vu', 'vus', 'virtual_users', 'users', 'user', 'threads', 'thread', 'allthreads',
    'grpthreads', 'workers', 'clients', 'connections', 'sessions', 'processes', 'procs', 'cpus', 'cores', 'nodes', 'instances', 'pods',
    'replicas', 'servers', 'inflight', 'in_flight', 'in_progress', 'parallelism', 'level'];
  const DEPENDENT = ['util', 'utilization', 'utilisation', 'pct', 'percent', 'busy', 'load_avg', 'loadavg', 'load1', 'load5', 'load15',
    'cpu_load', 'queue', 'queued', 'memory', 'mem', 'idle'];
  const RATE = ['x', 'throughput', 'tps', 'rps', 'qps', 'ops', 'rate', 'per_sec', 'persec', 'per_second', 'per_s', 'ps', 'hits_per',
    'req_s', 'reqs_s', 'tput'];
  const COUNT = ['count', 'events', 'hits', 'reqs', 'requests', 'transactions', 'samples_total'];
  const RESPONSE = ['r', 'rt', 'latency', 'response', 'duration', 'elapsed', 'ms', 'resp', 'response_time', 'wait'];
  const SAMPLES = ['minutes', 'samples', 'seconds', 'buckets', 'windows', 'periods'];

  const median = a => { const s = a.slice().sort((p, q) => p - q), m = s.length >> 1; return s.length % 2 ? s[m] : (s[m - 1] + s[m]) / 2; };
  const fmt = v => Number(v.toPrecision(3)).toLocaleString('en-US');

  function pick(fields, numeric, given){
    const ok = name => name && fields.includes(name) && numeric[fields.indexOf(name)];
    const nums = fields.filter((f, i) => numeric[i] && !/^_/.test(f));
    const first = (list, skip) => nums.find(f => !skip.includes(f) && list(f));
    let load = ok(given && given.load) ? given.load : first(f => has(f, LOAD) && !has(f, DEPENDENT), []);
    let throughput = ok(given && given.throughput) ? given.throughput : first(f => has(f, RATE) && !has(f, RESPONSE.filter(k => k !== 'r')), [load]);
    let guessed = false, counted = false;
    if (!throughput){ throughput = first(f => has(f, COUNT), [load]); counted = !!throughput; }
    if (!load && !throughput && nums.length >= 2){ load = nums[0]; throughput = nums[1]; guessed = true; }
    const response = ok(given && given.response) ? given.response : (first(f => has(f, RESPONSE), [load, throughput]) || '');
    const samples = first(f => has(f, SAMPLES) || (has(f, ['count']) && f !== throughput), [load, throughput, response]) || '';
    return {load: load || '', throughput: throughput || '', response, samples, guessed, counted};
  }

  function assess(fields, rows, given){
    fields = Array.isArray(fields) ? fields : []; rows = Array.isArray(rows) ? rows : [];
    const F = [], add = (level, text) => F.push({level, text});
    const numeric = Cells.numeric(fields, rows);
    const c = pick(fields, numeric, given);
    const dependent = fields.filter((f, i) => numeric[i] && has(f, DEPENDENT));

    if (!c.load){
      add('problem', dependent.length
        ? 'No load column. ' + dependent.join(', ') + ' is a result of the load, not the load: the USL needs what you set or count, such as users, threads or requests in progress.'
        : 'No load column: name it for what it counts, such as concurrency, users, threads or n, and make it a number.');
    } else if (has(c.load, DEPENDENT)){
      add('problem', 'The load column, ' + c.load + ', looks like a utilisation or a load average. That is a result of the load, not the load: use what you set or count, such as users, threads or requests in progress.');
    }
    if (!c.throughput) add('problem', 'No throughput column: a rate such as requests per second, named for example throughput, tps or x.');
    if (c.guessed) add('watch', 'No column is named like a load or a throughput, so the first two number columns are used: ' + c.load + ' and ' + c.throughput + '. Check that is right.');
    if (c.counted) add('watch', c.throughput + ' looks like a count, not a rate. The USL needs throughput per second, measured over the same length of time at every load: divide by the seconds in each window.');
    if (!c.load || !c.throughput) return {use: null, findings: F, ready: false, checked: rows.length};

    const iN = fields.indexOf(c.load), iX = fields.indexOf(c.throughput), iR = c.response ? fields.indexOf(c.response) : -1;
    const iS = c.samples ? fields.indexOf(c.samples) : -1;
    const pts = [];
    let skipped = 0;
    rows.forEach(r => {
      if (!Cells.isNum(r[iN]) || !Cells.isNum(r[iX]) || !(Cells.num(r[iN]) > 0) || !(Cells.num(r[iX]) > 0)){ skipped++; return; }
      pts.push({n: Cells.num(r[iN]), x: Cells.num(r[iX]), r: iR >= 0 && Cells.isNum(r[iR]) ? Cells.num(r[iR]) : null, s: iS >= 0 && Cells.isNum(r[iS]) ? Cells.num(r[iS]) : null});
    });
    if (skipped) add('watch', skipped + (skipped === 1 ? ' row has' : ' rows have') + ' no load or throughput above 0, so the USL lab will leave ' + (skipped === 1 ? 'it' : 'them') + ' out.');

    const loads = [...new Set(pts.map(p => p.n))].sort((a, b) => a - b);
    if (fields.includes('_time') || pts.length > loads.length * 1.5)
      add('watch', 'There are ' + pts.length + ' rows for ' + loads.length + ' loads. The USL wants one steady-state throughput per load: end the search with "| stats avg(' + c.throughput + ') AS ' + c.throughput + ' BY ' + c.load + '".');
    if (pts.length < 4 || loads.length < 3) add('problem', 'Only ' + loads.length + (loads.length === 1 ? ' load level' : ' load levels') + ': the USL lab needs at least 4 runs at 3 or more loads, and Gunther advises half a dozen or more.');
    else if (loads.length < 6) add('watch', loads.length + ' load levels. Gunther advises at least half a dozen for a fit you can trust.');
    else add('ok', loads.length + ' load levels, from ' + fmt(loads[0]) + ' to ' + fmt(loads[loads.length - 1]) + '.');
    if (loads.length >= 2 && loads[loads.length - 1] / loads[0] < 4)
      add('watch', 'The loads span only ×' + fmt(loads[loads.length - 1] / loads[0]) + '. Over a narrow range contention and coherency look alike; test lower and higher loads if you can.');
    if (loads.length && loads[0] > 1.5) add('ok', 'No run at a load of 1: the lab estimates the throughput of one unit as part of the fit, as Gunther\'s three-parameter form does.');

    // One throughput per load (the mean of its rows), in load order.
    const byN = loads.map(n => { const xs = pts.filter(p => p.n === n).map(p => p.x); return {n, x: xs.reduce((s, v) => s + v, 0) / xs.length}; });
    if (byN.length >= 3){
      const base = byN[0].x / byN[0].n;
      const super_ = byN.slice(1).find(p => p.x / p.n > base * 1.1);
      if (super_) add('watch', 'At a load of ' + fmt(super_.n) + ', throughput per unit of load is ' + fmt(super_.x / super_.n / base) + '× what it is at ' + fmt(byN[0].n) +
        '. That is superlinear scaling, which the USL lab does not model: check for warm-up, caching or a bad measurement before you trust the fit.');
      const top = byN.reduce((m, p) => p.x > m.x ? p : m, byN[0]);
      if (top === byN[byN.length - 1]) add('watch', 'Throughput is still rising at the highest load, so the peak the lab finds will be a prediction beyond the data. Test higher loads to see it.');
      else add('ok', 'Throughput peaks at a load of ' + fmt(top.n) + ' and falls after it, so the cost of coherency can be measured.');
    }

    // Little's Law: requests in progress = throughput × response time, if the response times are in ms or s.
    const lit = pts.filter(p => p.r > 0);
    if (lit.length >= 3){
      const q = div => median(lit.map(p => p.x * (p.r / div) / p.n));
      const ms = q(1000), s = q(1);
      if (Math.abs(ms - 1) > 0.25 && Math.abs(s - 1) > 0.25)
        add('watch', 'Load and throughput × response time disagree (Little\'s Law): the median ratio is ' + fmt(ms) + ' with milliseconds and ' + fmt(s) +
          ' with seconds, where 1 is expected. Fine for a load test with think time; otherwise the load may not count work in progress, or the throughput is not per second.');
      else add('ok', 'Load, throughput and response time agree with Little\'s Law, with the response time in ' + (Math.abs(ms - 1) <= 0.25 ? 'milliseconds' : 'seconds') + '.');
    }
    // Steady state: a row averaged over little data is mostly transient.
    const sized = pts.filter(p => p.s !== null);
    if (sized.length >= 3){
      const thin = sized.filter(p => p.s < Math.max(3, 0.1 * median(sized.map(q => q.s))));
      if (thin.length) add('watch', thin.length + (thin.length === 1 ? ' load rests' : ' loads rest') + ' on little data (' + c.samples + ' under ' + fmt(Math.max(3, 0.1 * median(sized.map(q => q.s)))) +
        '): near steady state needs enough of it. Consider leaving ' + (thin.length === 1 ? 'it' : 'them') + ' out.');
    }

    return {use: {load: c.load, throughput: c.throughput, response: c.response}, findings: F, ready: !F.some(f => f.level === 'problem'), checked: rows.length};
  }

  return Object.freeze({assess});
})();
