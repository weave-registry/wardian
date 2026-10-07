/* readouts: "What the fit says". Listens: analysis:ready. Emits: nothing. */
Kernel.register({
  name: 'readouts',
  listens: ['analysis:ready'],
  init(ctx){
    const $ = ctx.$, {esc, fmt, fmtRange} = Lib;
    ctx.on('analysis:ready', A => {
      const host = $('#rows'), sub = $('#fitSub');
      if (!A.ok){ host.innerHTML = ''; sub.textContent = ''; return; }
      const f0 = A.fit, ci = A.ci, u = A.units, pk = f0.peak;
      sub.textContent = 'Ranges are 90% bootstrap intervals from ' + A.samples + ' refits.';
      const row = (k, kk, v, unit, r, m) => '<div class="row"><div class="k">'+k+'<small>'+kk+'</small></div><div class="v">'+esc(v)+(unit ? '<i>'+esc(unit)+'</i>' : '')+'</div><div class="r">'+esc(r)+'</div><div class="m">'+esc(m)+'</div></div>';
      let h = '';
      h += row('Contention', 'α', fmt(f0.alpha, 2), '', fmtRange(ci.alpha[0], ci.alpha[1], 2),
        f0.alpha > 1e-6 ? 'About ' + (f0.alpha*100).toFixed(1) + '% of the work is effectively serialized. On its own, that would cap throughput near ' + fmt(f0.lambda/f0.alpha, 4) + ' ' + u.x + ' however much capacity you add.' : 'No measurable serialization in this data.');
      h += row('Coherency', 'β', fmt(f0.beta, 2), '', fmtRange(ci.beta[0], ci.beta[1], 2),
        f0.beta > 1e-8 ? 'The cost of workers coordinating with each other. This is what makes throughput fall after the peak.' : 'Not detectable here, so the fit shows no drop at high load.');
      h += row('Single-unit rate', 'λ', fmt(f0.lambda, 4), u.x, fmtRange(ci.lambda[0], ci.lambda[1], 4), (/^\w+s$/.test(u.n) && !/ss$/.test(u.n) ? 'Fitted throughput of one ' + u.n.slice(0, -1) + ' running alone.' : 'Fitted throughput at a load of 1 (' + u.n + ').'));
      if (pk.kind === 'peak'){
        h += row('Peak load', 'N*', fmt(pk.nStar), u.n, fmtRange(ci.nStar[0], ci.nStar[1]), 'Past this load, adding more makes the system slower.');
        h += row('Peak throughput', 'X max', fmt(pk.xMax, 4), u.x, fmtRange(ci.xMax[0], ci.xMax[1], 4), 'The most the model says this system can deliver.');
      } else if (pk.kind === 'ceiling'){
        h += row('Peak load', 'N*', 'none', '', 'β ≈ 0', 'Throughput levels off instead of falling, so there is no peak to find.');
        h += row('Throughput ceiling', 'λ/α', fmt(pk.xMax, 4), u.x, fmtRange(ci.xMax[0], ci.xMax[1], 4), 'The level throughput approaches as load grows.');
      }
      host.innerHTML = h;
    });
  }
});
