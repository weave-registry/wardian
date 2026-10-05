/* engine host: runs the pure maths in a background Web Worker, or in the page if workers are blocked.
   The caller never knows which: same methods, same results.
   Provides: analyze, curve.  Emits: engine:status (retained).  Capabilities: worker, source. */
Kernel.register({
  name: 'engine',
  emits: {'engine:status': {retain: true}},
  provides: ['analyze', 'curve'],
  caps: ['worker', 'source'],
  init(ctx){
    const GLUE = "self.onmessage=function(e){var m=e.data;try{var r=UslEngine.handle(m.method,m.args);" +
      "self.postMessage({id:m.id,result:r});}catch(err){self.postMessage({id:m.id,error:String(err&&err.message||err)});}};" +
      "self.postMessage({ready:true});";
    let mode = 'main', worker = null, nextId = 1, timer = null;
    const pending = new Map();
    const status = reason => ctx.emit('engine:status', {mode, reason: reason || ''});
    const local = (method, args) => Promise.resolve().then(() => UslEngine.handle(method, args));

    function fallback(reason){
      if (mode === 'main') return;
      clearTimeout(timer);
      mode = 'main';
      if (worker){ try { worker.terminate(); } catch (e) { /* already gone */ } worker = null; }
      status(reason);
      const waiting = Array.from(pending.values()); pending.clear();
      waiting.forEach(p => local(p.method, p.args).then(p.res, p.rej));
    }
    function run(method, args){
      if (mode === 'worker') return new Promise((res, rej) => { const id = nextId++; pending.set(id, {res, rej, method, args}); worker.postMessage({id, method, args}); });
      return local(method, args);
    }
    ctx.provide({analyze: a => run('analyze', a), curve: a => run('curve', a)});

    worker = ctx.spawn(ctx.source('lib-src') + '\n' + ctx.source('engine-src') + '\n' + GLUE);
    if (!worker){ status('workers unavailable here'); return; }
    mode = 'worker';
    worker.onmessage = e => {
      const m = e.data || {};
      if (m.ready){ clearTimeout(timer); return; }
      const p = pending.get(m.id); if (!p) return;
      pending.delete(m.id);
      if (m.error) p.rej(new Error(m.error)); else p.res(m.result);
    };
    worker.onerror = () => fallback('worker failed');
    timer = setTimeout(() => fallback('worker did not start'), 2500);     // CSP can block workers silently
    status();
  }
});
