/* export: saves the latest result as CSV through the host's download capability: the
   percentiles of the finish time, and each task's criticality index.
   Listens: sim:result.  Capabilities: claude:downloads. */
Kernel.register({
  name: 'export',
  listens: ['sim:result'],
  caps: ['claude:downloads'],
  init(ctx){
    const $ = ctx.$, note = t => { $('#note').textContent = t; };
    let r = null, downloads = null;
    const cell = v => { const s = v == null ? '' : String(v); return /[",\n]/.test(s) ? '"' + s.replace(/"/g, '""') + '"' : s; };
    const line = vs => vs.map(cell).join(',');
    const round = v => Math.round(v * 1000) / 1000;
    function sync(){
      const ok = !!(r && r.ok && downloads);
      $('#pct').disabled = !ok; $('#crit').disabled = !ok;
      $('#sub').textContent = r && r.ok ? Fmt.int(r.done) + ' trials · seed ' + r.seed : '';
      if (!downloads) note('Downloads are not available in this host.');
    }
    async function save(name, lines){
      const filename = name + '-seed-' + r.seed + '-' + r.done + '-trials.csv';
      try { await downloads.save({filename, data: lines.join('\n') + '\n'}); note('Saved ' + filename + '.'); }
      catch (e){ note('Could not save the file: ' + Fmt.errText(e)); }
    }
    $('#pct').addEventListener('click', () => {
      const dated = !!Fmt.date(r.start, 0, r.unit);
      const lines = [line(['percentile', r.unit].concat(dated ? ['date'] : []))];
      r.pct.forEach((v, q) => lines.push(line([q, round(v)].concat(dated ? [Fmt.date(r.start, v, r.unit, true)] : []))));
      save('finish-percentiles', lines);
    });
    $('#crit').addEventListener('click', () => {
      const lines = [line(['task', 'name', 'criticality_index', 'best_' + r.unit, 'likely_' + r.unit, 'worst_' + r.unit])];
      r.crit.forEach(t => lines.push(line([t.num, t.name, round(t.index), t.o, t.m, t.p])));
      save('criticality', lines);
    });
    ctx.cap('downloads').then(d => { downloads = d; sync(); }).catch(() => sync());
    ctx.on('sim:result', p => { r = p; note(''); sync(); });
  }
});
