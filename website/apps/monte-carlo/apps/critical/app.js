/* critical: how often each task is on the critical path (its criticality index), highest first.
   Listens: sim:result. */
Kernel.register({
  name: 'critical',
  listens: ['sim:result'],
  init(ctx){
    const $ = ctx.$;
    ctx.on('sim:result', r => {
      const ok = r && r.ok;
      $('#empty').hidden = ok;
      $('.w-table-wrap').hidden = !ok;
      $('#sub').textContent = ok ? 'in ' + Fmt.int(r.done) + ' trials' : '';
      if (!ok){ $('#rows').replaceChildren(); return; }
      $('#rows').replaceChildren(...r.crit.map(t => {
        const tr = ctx.el('tr'), td = (text, cls) => { const c = ctx.el('td'); c.textContent = text; if (cls) c.className = cls; return c; };
        const idx = ctx.el('td'), box = ctx.el('div'); idx.className = 'idx'; box.className = 'idxbox'; idx.appendChild(box);
        const meter = ctx.el('span'); meter.className = 'meter';
        const fill = ctx.el('span'); fill.style.width = (t.index * 100).toFixed(1) + '%';
        meter.appendChild(fill);
        const pct = ctx.el('span'); pct.className = 'pct'; pct.textContent = Fmt.pct(t.index, t.index > 0 && t.index < 0.01 ? 1 : 0);
        box.append(meter, pct);
        tr.append(td(t.num, 'n'), td(t.name), idx, td(Fmt.num(t.o, 1), 'num'), td(Fmt.num(t.m, 1), 'num'), td(Fmt.num(t.p, 1), 'num'));
        return tr;
      }));
    });
  }
});
