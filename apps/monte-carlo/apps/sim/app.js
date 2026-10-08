/* sim: runs the simulation off the main thread. It has no panel.
   It starts SimEngine (engine.js) in a Web Worker made from the inlined scripts, so the page stays
   responsive during a million trials. Where workers are blocked it runs the same code here, in
   slices, and says so. A change to the project starts a new run; the run in progress is dropped.
   Listens: project:changed.  Emits: sim:progress, sim:result (both retained).
   Provides: run (run the current project again), cancel (stop and keep the trials done so far).
   Capabilities: worker, source. */
Kernel.register({
  name: 'sim',
  listens: ['project:changed'],
  emits: {'sim:progress': {retain: true}, 'sim:result': {retain: true}},
  provides: ['run', 'cancel'],
  caps: ['worker', 'source'],
  init(ctx){
    const GLUE = '\nconst handle = SimEngine.serve(m => self.postMessage(m));\n' +
      'self.onmessage = e => handle(e.data);\nself.postMessage({type: "ready"});\n';
    let mode = 'worker', worker = null, local = null, project = null;
    let runId = 0, running = null, lastSent = 0, timer = null, startTimer = null;

    const progress = p => ctx.emit('sim:progress', Object.assign({mode}, p));
    function deliver(m){
      if (m.type === 'ready'){ clearTimeout(startTimer); return; }
      if (!running || m.id !== running.id) return;          // an old run's last words
      if (m.type === 'progress'){
        const now = Date.now();
        if (now - lastSent < 100) return;                   // about ten updates a second is plenty
        lastSent = now;
        progress({state: 'running', done: m.done, trials: m.trials, seed: running.seed});
      } else if (m.type === 'result'){
        const r = Object.assign(m.result, {mode, unit: running.unit, start: running.start, at: new Date().toISOString()});
        progress({state: r.stopped ? 'stopped' : 'done', done: r.done, trials: r.trials, seed: r.seed, ms: r.ms});
        ctx.emit('sim:result', r);
        running = null;
      } else if (m.type === 'error'){
        progress({state: 'error', error: m.error});
        ctx.emit('sim:result', {ok: false, problem: m.error});
        running = null;
      }
    }
    function send(m){
      if (mode === 'worker') worker.postMessage(m);
      else local(m);
    }
    function useMainThread(reason){
      if (mode === 'main') return;
      clearTimeout(startTimer);
      mode = 'main';
      if (worker){ try { worker.terminate(); } catch { /* already gone */ } worker = null; }
      local = SimEngine.serve(m => setTimeout(() => deliver(m), 0));
      console.warn('monte-carlo: running on the main thread: ' + reason);
      if (running) send({type: 'run', id: running.id, project: running.project, trials: running.trials, seed: running.seed});
    }

    function run(){
      if (!project || !project.valid){
        if (running) send({type: 'stop', id: running.id});
        running = null;
        progress({state: 'idle', problem: project ? project.problem : 'Waiting for the task list.'});
        ctx.emit('sim:result', {ok: false, problem: project ? project.problem : ''});
        return {started: false};
      }
      const s = project.settings;
      running = {id: ++runId, project: {tasks: project.tasks, dist: s.dist}, trials: s.trials, seed: s.seed, unit: s.unit, start: s.start};
      lastSent = 0;
      progress({state: 'running', done: 0, trials: s.trials, seed: s.seed});
      send({type: 'run', id: running.id, project: running.project, trials: running.trials, seed: running.seed});
      return {started: true, trials: s.trials, seed: s.seed};
    }
    function cancel(){
      if (!running) return {stopping: false};
      send({type: 'stop', id: running.id});
      return {stopping: true};
    }
    ctx.provide({run, cancel});

    // The worker gets the same scripts this frame has: the project checks and the engine.
    worker = ctx.spawn(ctx.source('project-src') + '\n' + ctx.source('engine-src') + GLUE);
    if (!worker) useMainThread('workers are not available here');
    else {
      worker.onmessage = e => deliver(e.data || {});
      worker.onerror = e => { e.preventDefault && e.preventDefault(); useMainThread('the worker failed'); };
      startTimer = setTimeout(() => useMainThread('the worker did not start'), 2500);   // CSP can block workers silently
    }

    // Edits come in bursts while someone types: wait for a short pause, then run.
    ctx.on('project:changed', p => {
      project = p;
      clearTimeout(timer);
      timer = setTimeout(run, 300);
    });
  }
});
