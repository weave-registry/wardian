/* export: report (.md) and fitted curve (.csv). Listens: analysis:ready, whatif:changed, checks:ready,
   diagnosis:updated, context:changed, capability:ai, engine:status.  Calls: engine.curve.  Capabilities: claude:downloads. */
Kernel.register({
  name: 'export',
  listens: ['analysis:ready', 'whatif:changed', 'checks:ready', 'diagnosis:updated', 'context:changed', 'capability:ai', 'engine:status'],
  needs: ['engine.curve'],
  caps: ['claude:downloads'],
  init(ctx){
    const $ = ctx.$, {fmt, fmtRange, peakInfo, whatIf, wiActive, describe, domainMax, model} = Lib;
    let A = null, wi = {a: 1, b: 1, l: 1, note: ''}, checks = [], dx = {text: '', stale: false}, context = '', eng = null, dl = null;

    ctx.cap('downloads').then(d => { dl = d; sync(); });
    ctx.on('analysis:ready', a => { A = a; sync(); });
    ctx.on('whatif:changed', w => { wi = w; });
    ctx.on('checks:ready', c => { checks = c.items; });
    ctx.on('diagnosis:updated', d => { dx = d; });
    ctx.on('context:changed', c => { context = c.text; });
    ctx.on('engine:status', s => { eng = s; });
    ctx.on('capability:ai', c => { $('#noAi').hidden = c.available; });

    const sync = () => { const ok = !!(A && A.ok && dl); $('#btnReport').disabled = !ok; $('#btnCurve').disabled = !ok; };

    function report(){
      const f0 = A.fit, ci = A.ci, u = A.units, pk = f0.peak, L = [], rowsHaveR = A.rows.some(r => r.r);
      L.push('# USL scalability report', '', 'Generated ' + new Date().toISOString().slice(0, 10) + '. Load is in ' + u.n + '; throughput is in ' + u.x + '.', '');
      const s = A.source;
      if (s && s.title){
        L.push('## Data', '', '**' + s.title + '**' + (s.range ? ' (' + s.range + ')' : '') + (s.at ? ', fetched ' + s.at.slice(0, 16).replace('T', ' ') + ' UTC' : ''), '');
        [['Load', s.load], ['Throughput', s.throughput], ['Response time', s.response], ['How it was made', s.method]].forEach(([k, v]) => { if (v) L.push('- **' + k + ':** ' + v); });
        if (s.edited) L.push('- **Note:** the numbers were changed by hand after loading.');
        if (s.search) L.push('', '```', s.search, '```');
        L.push('');
      }
      if (context) L.push('## System', '', context, '');
      L.push('## Fit', '', '| Parameter | Value | 90% range |', '|---|---|---|');
      L.push('| α (contention) | ' + fmt(f0.alpha, 3) + ' | ' + fmtRange(ci.alpha[0], ci.alpha[1], 3) + ' |');
      L.push('| β (coherency) | ' + fmt(f0.beta, 3) + ' | ' + fmtRange(ci.beta[0], ci.beta[1], 3) + ' |');
      L.push('| λ (single-unit rate) | ' + fmt(f0.lambda, 4) + ' | ' + fmtRange(ci.lambda[0], ci.lambda[1], 4) + ' |');
      if (pk.kind === 'peak'){
        L.push('| Peak load N* | ' + fmt(pk.nStar) + ' | ' + fmtRange(ci.nStar[0], ci.nStar[1]) + ' |');
        L.push('| Peak throughput | ' + fmt(pk.xMax, 4) + ' | ' + fmtRange(ci.xMax[0], ci.xMax[1], 4) + ' |');
      } else if (pk.kind === 'ceiling') L.push('| Throughput ceiling λ/α | ' + fmt(pk.xMax, 4) + ' | ' + fmtRange(ci.xMax[0], ci.xMax[1], 4) + ' |');
      L.push('', 'R² = ' + f0.r2.toFixed(3) + '; typical error ' + (f0.rmsePct*100).toFixed(1) + '%.', '', '## Data checks', '');
      checks.forEach(c => L.push('- **' + ({ok: 'OK', watch: 'Watch', problem: 'Problem'}[c.level]) + ':** ' + c.title + '. ' + c.detail));
      if (wiActive(wi)){
        const w = whatIf(f0, wi), pw = peakInfo(w.lam, w.a, w.b);
        L.push('', '## What-if', '', 'Factors: α ×' + wi.a.toFixed(2) + ', β ×' + wi.b.toFixed(2) + ', λ ×' + wi.l.toFixed(2) + '.', '', 'Current: ' + describe(pk, u) + '.', '', 'With changes: ' + describe(pw, u) + '.');
        if (wi.note) L.push('', wi.note);
      }
      if (dx.text) L.push('', '## Diagnosis' + (dx.stale ? ' (written before the data last changed)' : ''), '', dx.text.replace(/^#{1,2}\s/gm, '### '));
      L.push('', '## Measurements', '', '| Load | Throughput |' + (rowsHaveR ? ' Response time |' : ''), '|---|---|' + (rowsHaveR ? '---|' : ''));
      A.rows.forEach(r => L.push('| ' + r.n + ' | ' + r.x + ' |' + (rowsHaveR ? ' ' + (r.r || '') + ' |' : '')));
      if (eng) L.push('', '_Computed ' + (eng.mode === 'worker' ? 'in a background worker' : 'in the page') + '._');
      return L.join('\n');
    }

    async function curveCsv(){
      const f0 = A.fit, hi = domainMax(A.rows, f0, wi), ns = [];
      for (let i = 0; i <= 60; i++) ns.push(Math.max(hi*i/60, 0.0001));
      const c = await ctx.call('engine', 'curve', {id: A.id, sets: [{lambda: f0.lambda, alpha: f0.alpha, beta: f0.beta}], ns, band: [0.05, 0.95]});
      const out = ['load,fitted_throughput,low_90,high_90,linear_scaling,efficiency'];
      ns.forEach((n, i) => out.push([n.toFixed(3), c.ys[0][i].toFixed(3), c.low[i].toFixed(3), c.high[i].toFixed(3), (f0.lambda*n).toFixed(3), (model(n, f0.alpha, f0.beta)/n).toFixed(4)].join(',')));
      return out.join('\n');
    }

    async function save(filename, make){
      const note = $('#exportNote'); note.textContent = '';
      if (!dl){ note.textContent = 'Saving files is not available in this view.'; return; }
      try { await dl.save({filename, data: await make()}); note.textContent = 'Saved ' + filename + '.'; }
      catch (e) { if (!(e && e.code === 'declined')) note.textContent = 'Could not save the file.'; }
    }
    $('#btnReport').addEventListener('click', () => A && A.ok && save('usl-report.md', report));
    $('#btnCurve').addEventListener('click', () => A && A.ok && save('usl-fitted-curve.csv', curveCsv));
  }
});
