/* checks: "Can you trust it?". Turns the engine's finding codes into plain words.
   Listens: analysis:ready. Emits: checks:ready (retained). */
Kernel.register({
  name: 'checks',
  emits: {'checks:ready': {retain: true}},
  listens: ['analysis:ready'],
  init(ctx){
    const $ = ctx.$, {esc, fmt} = Lib;

    function words(A){
      const u = A.units, rows = A.rows, out = [], pc = (v, d) => (v*100).toFixed(d);
      for (const fd of A.findings){
        const d = fd.data, level = fd.level;
        let title = '', detail = '';
        switch (fd.code){
          case 'few_runs': title = 'Only ' + d.runs + ' runs'; detail = 'Three parameters are being fitted. Six or more runs spread across the load range give much steadier estimates.'; break;
          case 'enough_runs': title = d.runs + ' runs at ' + d.loads + ' load levels'; detail = 'Enough to fit three parameters.'; break;
          case 'past_peak': title = 'Runs extend past the predicted peak'; detail = 'Highest run is ' + fmt(d.maxLoad) + ' ' + u.n + '; the peak is predicted at ' + fmt(d.nStar) + '. β is anchored by real data.'; break;
          case 'not_past_peak': title = 'No runs beyond the predicted peak'; detail = 'Highest run is ' + fmt(d.maxLoad) + ' ' + u.n + '; the peak is predicted at ' + fmt(d.nStar) + '. Both β and the peak are extrapolated. Add runs near ' + fmt(d.suggestLoad) + ' ' + u.n + ' to confirm.'; break;
          case 'no_peak': title = 'No peak found in this data'; detail = 'Throughput is still rising or levelling off, so β cannot be measured. If you expect it to fall at high load, run higher loads.'; break;
          case 'poor_fit': title = 'Poor fit (typical error ' + pc(d.rmsePct, 0) + '%)'; detail = 'The curve misses the data by a wide margin. Look for runs affected by warm-up, noisy neighbours, or a load generator that saturated before the system did.'; break;
          case 'loose_fit': title = 'Loose fit (typical error ' + pc(d.rmsePct, 0) + '%)'; detail = 'R² is ' + d.r2.toFixed(3) + '. Treat the ranges as the real answer, not the single numbers.'; break;
          case 'tight_fit': title = 'Tight fit (typical error ' + pc(d.rmsePct, 1) + '%)'; detail = 'R² is ' + d.r2.toFixed(3) + '.'; break;
          case 'outliers': title = A.flags.length + (A.flags.length === 1 ? ' run looks off-curve' : ' runs look off-curve');
            detail = 'At ' + A.flags.map(i => fmt(rows[i].n)).join(', ') + ' ' + u.n + '. They are drawn hollow on the chart. Re-run them, or remove them and see how much the fit moves.'; break;
          case 'no_outliers': title = 'No outlier runs'; detail = 'Every run sits close to the curve relative to the typical scatter.'; break;
          case 'littles_off': title = 'Little’s Law is off for ' + d.count + (d.count === 1 ? ' run' : ' runs');
            detail = 'Throughput × response time should be about equal to load. Median ratio is ' + d.median.toFixed(2) + '; runs at ' + A.littles.bad.slice(0, 5).map(i => fmt(rows[i].n)).join(', ') + ' ' + u.n + ' differ by more than 15%. Check how load, throughput, and latency were measured.'; break;
          case 'littles_ok': title = 'Little’s Law holds'; detail = 'Throughput × response time matches load within 15% on every run (median ratio ' + d.median.toFixed(2) + ').'; break;
          case 'littles_unchecked': title = 'Little’s Law not checked'; detail = 'Add a third column with response time to cross-check that load, throughput, and latency agree.'; break;
          case 'peak_range_wide': title = 'The peak location is uncertain'; detail = 'The 90% range runs from ' + fmt(d.low) + ' to ' + fmt(d.high) + ' ' + u.n + '. More runs near the suspected peak would narrow it.'; break;
          default: continue;
        }
        out.push({level, title, detail});
      }
      return out;
    }

    ctx.on('analysis:ready', A => {
      const items = A.ok ? words(A) : [];
      $('#checks').innerHTML = items.length ? items.map(c => {
        const lv = {ok: 'OK', watch: 'Watch', problem: 'Problem'}[c.level];
        return '<li class="'+c.level+'"><span class="lvl">'+lv+'</span><b>'+esc(c.title)+'</b><p>'+esc(c.detail)+'</p></li>';
      }).join('') : '<li class="watch"><span class="lvl">Waiting</span><b>Not enough data yet</b><p>Checks run once there are four or more runs at three or more loads.</p></li>';
      ctx.emit('checks:ready', {items});
    });
  }
});
