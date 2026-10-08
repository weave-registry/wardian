/* forecast: when the project finishes. P50, P80 and P95, the mean and its spread, and how the
   plan made from the most likely estimates compares. Shows the run's progress, with Stop.
   Listens: sim:result, sim:progress.  Needs: sim.cancel. */
Kernel.register({
  name: 'forecast',
  listens: ['sim:result', 'sim:progress'],
  needs: ['sim.cancel'],
  init(ctx){
    const $ = ctx.$, bar = $('#bar');
    const say = t => { $('#story').textContent = t; };

    function draw(r){
      const ok = r && r.ok;
      $('#tiles').hidden = !ok;
      $('#empty').hidden = ok;
      if (!ok){
        $('#empty').textContent = r && r.problem ? 'Fix the task list first. ' + r.problem : 'Waiting for the first run.';
        $('#sub').textContent = ''; say(''); return;
      }
      const u = r.unit, d = v => Fmt.dur(v, u), when = v => Fmt.date(r.start, v, u);
      for (const q of [50, 80, 95]){ $('#p' + q).textContent = d(r.pct[q]); $('#p' + q).dataset.value = r.pct[q]; $('#p' + q + 'd').textContent = when(r.pct[q]) || ' '; }
      $('#mean').textContent = Fmt.num(r.mean) + ' ± ' + Fmt.num(r.sd);
      $('#range').textContent = 'Range ' + Fmt.num(r.min) + ' – ' + Fmt.num(r.max) + ' ' + Fmt.unitWord(u, 2);
      $('#sub').textContent = Fmt.int(r.done) + (r.stopped ? ' of ' + Fmt.int(r.trials) : '') + ' trials · ' +
        (r.dist === 'pert' ? 'PERT' : 'triangular') + ' · seed ' + r.seed;
      const likely = r.likely, gap = r.pct[80] - likely.value;
      let text = 'With every task at its most likely duration, the plan finishes in ' + d(likely.value) + '. ' +
        (likely.chance < 0.5
          ? 'Only ' + Fmt.pct(likely.chance) + ' of trials finish that soon: '
            + 'delays add up along the path, and tasks that run in parallel make the wait for the slowest one longer. '
          : Fmt.pct(likely.chance) + ' of trials finish that soon. ');
      if (gap > 0) text += 'To promise a date you will meet 4 times in 5, allow ' + d(gap) + ' more (P80).';
      if (r.stopped) text += ' The run was stopped early, so these numbers rest on fewer trials.';
      say(text);
    }

    SimBar.attach(ctx, bar, say);
    ctx.on('sim:progress', p => SimBar.show(bar, p));
    ctx.on('sim:result', draw);
  }
});
