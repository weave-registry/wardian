/* meaning: "What it means". Turns the fit into plain advice with fixed rules (no AI), answers
   "what load reaches this throughput?", and explains N, X, R and the Universal Scalability Law.
   Listens: analysis:ready, checks:ready.  Emits: target:changed (retained).  Capabilities: storage. */
Kernel.register({
  name: 'meaning',
  emits: {'target:changed': {retain: true}},
  listens: ['analysis:ready', 'checks:ready'],
  caps: ['storage'],
  init(ctx){
    const $ = ctx.$, {esc, fmt, model} = Lib;
    let A = null, checks = [];

    // Seconds in the time base of a throughput unit: "req/s" -> 1, "rpm" -> 60. null when unclear.
    function secsOf(unit){
      const u = String(unit || '').toLowerCase();
      if (/\b(rps|qps|tps|ops)\b|\/\s*s(ec(ond)?)?\b|per\s+sec/.test(u)) return 1;
      if (/\brpm\b|\/\s*min|per\s+min/.test(u)) return 60;
      if (/\/\s*h(ou)?r?\b|per\s+hour/.test(u)) return 3600;
      return null;
    }
    // A time span given in the throughput's time base, written for people.
    function span(t, secs){
      const s = t*secs;
      return s < 1 ? fmt(s*1000) + ' ms' : s < 120 ? fmt(s) + ' s' : fmt(s/60) + ' min';
    }

    const X = (f, n) => f.lambda*model(n, f.alpha, f.beta);
    // The smallest load at which the fitted curve reaches throughput t, on the rising side.
    function loadFor(f, t){
      let lo = 0, hi = f.peak.kind === 'peak' ? f.peak.nStar : 1;
      if (f.peak.kind !== 'peak') while (X(f, hi) < t && hi < 1e12) hi *= 2;
      if (X(f, hi) < t*(1 - 1e-9)) return null;
      for (let i = 0; i < 200; i++){ const mid = (lo + hi)/2; if (X(f, mid) < t) lo = mid; else hi = mid; }
      return hi;
    }

    const P = s => '<p>' + s + '</p>';
    const B = s => '<strong>' + esc(s) + '</strong>';

    function interpret(){
      const out = $('#meanOut');
      if (!A || !A.ok){ out.innerHTML = P('Add at least four runs at three or more loads. The meaning of the curve appears here.'); return; }
      const f = A.fit, u = A.units, pk = f.peak, maxN = A.rows[A.rows.length-1].n, secs = secsOf(u.x);
      const h = [];

      // 1. Where the system is on its curve.
      if (pk.kind === 'peak'){
        const xAtMax = X(f, maxN), drop = 1 - xAtMax/pk.xMax;
        if (maxN >= pk.nStar*1.1)
          h.push(P(B('You tested past the peak.') + ' Throughput is highest at about ' + esc(fmt(pk.nStar)) + ' ' + esc(u.n) + ' (' + esc(fmt(pk.xMax, 4)) + ' ' + esc(u.x) + '). At ' + esc(fmt(maxN)) + ' ' + esc(u.n) + ' it is ' + esc((drop*100).toFixed(0)) + '% lower: past the peak, more load makes the system slower, not faster.'));
        else if (maxN >= pk.nStar*0.8)
          h.push(P(B('Your tests reached the peak.') + ' Throughput tops out near ' + esc(fmt(pk.nStar)) + ' ' + esc(u.n) + ' at about ' + esc(fmt(pk.xMax, 4)) + ' ' + esc(u.x) + '. Beyond that, more load lowers throughput.'));
        else
          h.push(P(B('Throughput is still climbing in your data.') + ' The model predicts a peak of ' + esc(fmt(pk.xMax, 4)) + ' ' + esc(u.x) + ' at about ' + esc(fmt(pk.nStar)) + ' ' + esc(u.n) + ', but your highest test was ' + esc(fmt(maxN)) + '. Treat the peak as an estimate until you test near it.'));
      } else if (pk.kind === 'ceiling'){
        h.push(P(B('Throughput levels off instead of falling.') + ' It approaches about ' + esc(fmt(pk.xMax, 4)) + ' ' + esc(u.x) + ' and never passes it, however much load you add.'));
      } else {
        h.push(P(B('Throughput grows in step with load') + ' across everything you measured. The data shows no limit yet, so test higher loads to find one.'));
      }

      // 2. What holds it back: contention (α) or coherency (β), compared at the peak or the highest test.
      if (pk.kind !== 'linear'){
        const n = pk.kind === 'peak' ? Math.min(pk.nStar, Math.max(maxN, 2)) : Math.max(maxN, 2);
        const c = f.alpha*(n - 1), k = f.beta*n*(n - 1);
        const eff = X(f, n)/(f.lambda*n);
        let why;
        if (c > 2*k) why = B('Contention (α) is the main limit.') + ' Workers wait in line for something only one can use at a time: a lock, a single database writer, a shared queue, a small connection pool. Shortening or splitting that shared thing helps most. Try the α slider below.';
        else if (k > 2*c) why = B('Coherency (β) is the main limit.') + ' Workers spend time keeping each other in step: cache invalidation, distributed locks, chatty calls between services. This cost grows with the square of the load, which is why the curve turns down. Cutting cross-talk helps most. Try the β slider below.';
        else why = B('Contention (α) and coherency (β) limit it about equally.') + ' Both waiting in line and keeping workers in step cost real throughput. Try both sliders below.';
        h.push(P(why + ' At ' + esc(fmt(n)) + ' ' + esc(u.n) + ', each unit does only ' + esc((eff*100).toFixed(0)) + '% of what it would do alone.'));
      }

      // 3. A sensible operating point: 90% of the best throughput, before response time climbs steeply.
      if (pk.kind !== 'linear'){
        const n90 = loadFor(f, 0.9*pk.xMax);
        if (n90){
          let s = B('A safe place to run is about ' + fmt(n90) + ' ' + u.n + '.') + ' There you get 90% of the best throughput (' + esc(fmt(0.9*pk.xMax, 4)) + ' ' + esc(u.x) + ').';
          if (secs) s += ' By Little’s Law each request then takes about ' + esc(span(n90/X(f, n90), secs)) + ', against ' + esc(span(1/f.lambda, secs)) + ' with nothing else running.';
          s += ' Going further buys little throughput and costs a lot of waiting.';
          h.push(P(s));
        }
      }

      // 4. How much to trust it.
      const worry = checks.filter(c => c.level !== 'ok');
      h.push(P(worry.length
        ? B('Read this with care:') + ' ' + worry.map(c => esc(c.title)).join('; ') + '. See “Can you trust it?” below.'
        : 'The data checks below found nothing worrying, so the fit is a fair guide.'));
      out.innerHTML = h.join('');
    }

    // ---------- target ----------
    let target = ctx.store.get('target') || {value: '', per: '60'};
    $('#tgtVal').value = target.value; $('#tgtPer').value = target.per;

    function forecast(){
      const out = $('#tgtOut'), raw = $('#tgtVal').value.replace(/[, _]/g, ''), want = Number(raw);
      if (!raw){ out.innerHTML = ''; ctx.emit('target:changed', {x: null}); return; }
      if (!(want > 0) || !isFinite(want)){ out.innerHTML = P('Type a positive number, such as 10000.'); ctx.emit('target:changed', {x: null}); return; }
      if (!A || !A.ok){ out.innerHTML = P('Add measurements first.'); ctx.emit('target:changed', {x: null}); return; }
      const f = A.fit, u = A.units, pk = f.peak, maxN = A.rows[A.rows.length-1].n, secs = secsOf(u.x);
      const per = Number($('#tgtPer').value);
      // In the data's own unit. If the data's time base is unclear, take the number as it is.
      const t = secs ? want*secs/per : want;
      const asked = fmt(want, 6) + ' ' + ({1: 'per second', 60: 'per minute', 3600: 'per hour'}[per]);
      const same = secs && secs !== per ? ' (' + fmt(t, 4) + ' ' + u.x + ')' : '';
      ctx.emit('target:changed', {x: t});
      const h = [];
      if (!secs) h.push(P('I can’t tell the time unit of “' + esc(u.x) + '”, so I read your target as ' + esc(fmt(want, 6)) + ' ' + esc(u.x) + '. Name the unit like “req/s” or “req/min” in Units to convert.'));

      const n = loadFor(f, t);
      if (n !== null && t <= f.lambda){
        let s = B(asked + same + ' is easy for this system.') + ' One unit working alone does about ' + esc(fmt(f.lambda, 4)) + ' ' + esc(u.x) + ', which already covers it.';
        if (secs) s += ' Each request takes about ' + esc(span(1/f.lambda, secs)) + '.';
        if (pk.kind !== 'linear') s += ' The target is ' + esc((t/pk.xMax*100).toFixed(t/pk.xMax < 0.01 ? 2 : 0)) + '% of the most it can do, so there is a lot of headroom.';
        h.push(P(s));
      } else if (n !== null){
        const share = pk.kind === 'linear' ? 0 : t/pk.xMax;
        let s = B(asked + same + ' is reachable.') + ' The curve gets there at about ' + B(fmt(n) + ' ' + u.n) + '.';
        if (secs) s += ' Each request then takes about ' + esc(span(n/X(f, n), secs)) + ' (Little’s Law).';
        h.push(P(s));
        if (pk.kind !== 'linear') h.push(P('That is ' + esc((share*100).toFixed(0)) + '% of the most this system can do.' + (share > 0.9 ? ' ' + B('That leaves almost no headroom:') + ' a burst of traffic pushes it to the peak, where waiting grows fast. Plan for more capacity.' : share > 0.75 ? ' It works, with little room for bursts.' : ' There is room for bursts.')));
        if (n > maxN*1.05) h.push(P('Your highest test was ' + esc(fmt(maxN)) + ' ' + esc(u.n) + ', so this is a forecast beyond your data. Test near ' + esc(fmt(Math.ceil(n))) + ' ' + esc(u.n) + ' to confirm it.'));
        if (A.ci && pk.kind !== 'linear' && isFinite(A.ci.xMax[0]) && t > A.ci.xMax[0]) h.push(P('The fit is uncertain here: the peak could be as low as ' + esc(fmt(A.ci.xMax[0], 4)) + ' ' + esc(u.x) + ', which is below your target.'));
      } else {
        const copiesPeak = Math.ceil(t/pk.xMax), copiesSafe = Math.ceil(t/(0.8*pk.xMax));
        h.push(P(B(asked + same + ' is more than one system like this can do.') + ' Its best is about ' + esc(fmt(pk.xMax, 4)) + ' ' + esc(u.x) + (pk.kind === 'peak' ? ', at ' + esc(fmt(pk.nStar)) + ' ' + esc(u.n) : ', a ceiling it only approaches') + '. Adding load will not get you there.'));
        h.push(P('Ways to get there:'));
        h.push('<ul>' +
          '<li>' + B('Run copies side by side.') + ' You need at least ' + esc(copiesPeak) + ' running flat out, or ' + esc(copiesSafe) + ' running at a safe 80%. This only works if the copies share nothing, such as the same database, lock or queue.</li>' +
          '<li>' + B('Make one unit faster.') + ' Throughput scales with λ, so each unit must be about ' + esc((t/pk.xMax).toFixed(2)) + '× as fast. Set the λ slider below to that and check.</li>' +
          '<li>' + B('Remove the bottleneck.') + ' Lower α or β with the sliders below until the dotted curve passes your target line.</li>' +
          '</ul>');
      }
      $('#tgtOut').innerHTML = h.join('');
    }
    function changed(){
      target = {value: $('#tgtVal').value, per: $('#tgtPer').value};
      ctx.store.set('target', target);
      forecast();
    }
    $('#tgtVal').addEventListener('input', changed);
    $('#tgtPer').addEventListener('change', changed);
    $('#tgtClear').addEventListener('click', () => { $('#tgtVal').value = ''; changed(); });

    // ---------- glossary ----------
    function glossary(){
      const u = (A && A.units) || {n: 'threads', x: 'req/s'};
      $('#glossBody').innerHTML =
        '<p>Think of a kitchen. The cooks are the load, the dishes that leave per hour are the throughput, and the time one order takes is the response time. More cooks help, until they queue for the one stove or spend their time talking to each other.</p>' +
        '<dl>' +
        '<dt>N, load</dt><dd>How much work is in the system at the same time: users, threads, connections or requests in progress. Here it is measured in ' + B(u.n) + '. This is the first column of your measurements.</dd>' +
        '<dt>X, throughput</dt><dd>How much work finishes per unit of time. Here it is ' + B(u.x) + '. This is the second column.</dd>' +
        '<dt>R, response time</dt><dd>How long one piece of work takes from start to finish. This is the optional third column.</dd>' +
        '<dt>Little’s Law</dt><dd><code>N = X × R</code>. The work in progress equals the rate it finishes times how long each piece takes. Know two and you know the third. The lab uses it to check that your three columns agree.</dd>' +
        '<dt>λ, single-unit speed</dt><dd>The throughput of one unit working alone. With perfect scaling, N units would give <code>λ × N</code>: the straight grey line on the chart.</dd>' +
        '<dt>α, contention</dt><dd>The share of work that must happen one at a time: waiting in line for the one stove. It makes throughput level off.</dd>' +
        '<dt>β, coherency</dt><dd>The cost of units keeping each other in step: every cook checking with every other cook. It grows with N², so it makes throughput fall after a peak.</dd>' +
        '<dt>N*, the peak</dt><dd>The load with the highest throughput, <code>√((1 − α) / β)</code>. Past it, more load means less work done.</dd>' +
        '</dl>' +
        '<p class="formula">The Universal Scalability Law: <code>X(N) = λN / (1 + α(N − 1) + βN(N − 1))</code>. The top is perfect scaling; each term underneath takes some of it away.</p>';
    }

    ctx.on('analysis:ready', a => { A = a; interpret(); glossary(); forecast(); });
    ctx.on('checks:ready', c => { checks = c.items; interpret(); });
    glossary(); interpret();
  }
});
