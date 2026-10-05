/* session: the coordinator. Turns published data into an analysis by asking the engine.
   Listens: data:changed, engine:status.  Calls: engine.analyze.  Emits: analysis:ready (retained). No UI. */
Kernel.register({
  name: 'session',
  emits: {'analysis:ready': {retain: true}},
  listens: ['data:changed', 'engine:status'],
  needs: ['engine.analyze'],
  init(ctx){
    let seq = 0, data = null, prevMode = null;

    async function run(){
      if (!data) return;
      const my = ++seq, d = data;
      const base = {id: my, rows: d.rows, units: d.units, rdiv: d.rdiv};
      const distinct = new Set(d.rows.map(r => r.n)).size;
      if (d.rows.length < 4 || distinct < 3){ ctx.emit('analysis:ready', Object.assign({ok: false}, base)); return; }
      try {
        const res = await ctx.call('engine', 'analyze', {id: my, rows: d.rows, rdiv: d.rdiv, boots: 240, seed: 20240611});
        if (my !== seq) return;                      // newer data arrived while the engine was working
        ctx.emit('analysis:ready', Object.assign({ok: true}, base, res));
      } catch (e) {
        if (my !== seq) return;
        ctx.emit('analysis:ready', Object.assign({ok: false, error: String(e && e.message || e)}, base));
      }
    }

    ctx.on('data:changed', d => { data = d; run(); });
    // If the worker was swapped for the in-page engine, its stored bootstrap samples are gone: analyse again.
    ctx.on('engine:status', s => { if (prevMode === 'worker' && s.mode === 'main') run(); prevMode = s.mode; });
  }
});
